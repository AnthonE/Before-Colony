//! Own-suit prediction and reconciliation.
//!
//! The client steps its own suit with exactly the server's flight model (`bc_sim::flight`, bit-
//! identical on wasm) and the exact quantized commands it sent. When a snapshot arrives it rewinds
//! to the server's state for that tick and replays the unacknowledged commands.
//!
//! Every tick flown is kept ([`Sample`]), so the suit is drawn between the last two
//! ([`Predictor::pose_at`]) rather than stepping at the tick rate, and a correction can be measured
//! where the suit was drawn; the client blends it out (`own`).
//!
//! A change of form (Wing Zero ↔ Neo-Bird) is predicted too: each command steps the form as the
//! server does (`bc_sim::transform`), which sets the frame flown and cuts thrust mid-change. So are
//! the arms (`bc_sim::arms`): a blade's lunge drives the suit, and busy arms slow its turning, on
//! the very ticks they do on the server.

use bc_proto::buttons::MODE;
use bc_proto::snapshot::own_flags;
use bc_proto::{FrameId, InputCmd, OwnState};
use bc_sim::arms::{ArmsClock, busy_ambac};
use bc_sim::content::frame;
use bc_sim::field::Field;
use bc_sim::flight::{FlightMods, FlightOut, FlightState, step_in};
use bc_sim::math::integrate_rotation;
use bc_sim::transform::{Form, transform_step, transform_thrust};
use bc_sim::tuning::{Tuning, flight_mods, own_tuning, sputter};
use bc_sim::{DT, TICK_HZ};
use glam::{Quat, Vec3};

use crate::inputs::InputHistory;

const HISTORY: usize = 128;
/// The server's suit this far from where the prediction had it at the same tick was moved there
/// (a relocation), not mispredicted, m.
pub const RELOCATION: f32 = 150.0;
/// The longest gap in the commands sent (a stall) that the prediction flies through on the
/// server's stand-ins as it goes; a longer one waits for the next snapshot.
const MAX_GAP: u32 = 32;

/// One tick the prediction flew: the suit's state after it, and what it did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub ang_vel: Vec3,
    /// Felt acceleration over the tick, g.
    pub g_load: f32,
    pub g_strain: f32,
    pub blackout: bool,
    pub boosting: bool,
    /// Thrust applied per local axis, as a fraction of each axis' unboosted maximum.
    pub throttle: Vec3,
    /// Flight assist held the pilot's G down.
    pub g_limited: bool,
    /// The frame flown.
    pub frame: FrameId,
    /// The mount of a strike in its windup or stroke after the tick.
    pub strike: Option<u8>,
}

impl Sample {
    const NONE: Sample = Sample {
        tick: u32::MAX,
        pos: Vec3::ZERO,
        vel: Vec3::ZERO,
        rot: Quat::IDENTITY,
        ang_vel: Vec3::ZERO,
        g_load: 0.0,
        g_strain: 0.0,
        blackout: false,
        boosting: false,
        throttle: Vec3::ZERO,
        g_limited: false,
        frame: FrameId::Leo,
        strike: None,
    };

    fn of(tick: u32, s: &FlightState, out: &FlightOut, frame: FrameId, arms: &ArmsClock) -> Self {
        Self {
            tick,
            pos: s.pos,
            vel: s.vel,
            rot: s.rot,
            ang_vel: s.ang_vel,
            g_load: s.g_load,
            g_strain: s.g_strain,
            blackout: s.blackout,
            boosting: out.boosting,
            throttle: out.throttle,
            g_limited: out.g_limited,
            frame,
            strike: arms.striking(),
        }
    }
}

/// The own suit at a moment between the ticks flown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnPose {
    pub pos: Vec3,
    pub rot: Quat,
    /// The flight model's velocity.
    pub vel: Vec3,
    /// How fast the pose moves along what is drawn, m/s at real time (the chord between ticks).
    pub dpos: Vec3,
    pub g_load: f32,
    pub g_strain: f32,
    pub blackout: bool,
    pub boosting: bool,
    pub throttle: Vec3,
    pub g_limited: bool,
    pub frame: FrameId,
    /// The mount of a strike in its windup or stroke.
    pub strike: Option<u8>,
}

impl OwnPose {
    /// `u` (0..1) of the way from tick `a` to tick `b`: position and velocity lerped (exact for
    /// the flight model's integrator), rotation nlerped. What happened over the tick (thrust,
    /// boost, the frame) is `b`'s.
    fn between(a: &Sample, b: &Sample, u: f32) -> Self {
        let rb = if a.rot.dot(b.rot) < 0.0 { -b.rot } else { b.rot };
        Self {
            pos: a.pos.lerp(b.pos, u),
            rot: a.rot.lerp(rb, u).normalize(),
            vel: a.vel.lerp(b.vel, u),
            dpos: (b.pos - a.pos) * TICK_HZ as f32,
            g_load: a.g_load + (b.g_load - a.g_load) * u,
            g_strain: a.g_strain + (b.g_strain - a.g_strain) * u,
            blackout: b.blackout,
            boosting: b.boosting,
            throttle: a.throttle.lerp(b.throttle, u),
            g_limited: b.g_limited,
            frame: b.frame,
            strike: b.strike,
        }
    }

    /// `u` ticks on from tick `a`, carried on at its velocity and spin.
    fn ahead(a: &Sample, u: f32) -> Self {
        Self {
            pos: a.pos + a.vel * (u * DT),
            rot: integrate_rotation(a.rot, a.ang_vel, u * DT),
            vel: a.vel,
            dpos: a.vel,
            g_load: a.g_load,
            g_strain: a.g_strain,
            blackout: a.blackout,
            boosting: a.boosting,
            throttle: a.throttle,
            g_limited: a.g_limited,
            frame: a.frame,
            strike: a.strike,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Predictor {
    /// Predicted state after the newest generated command.
    pub state: FlightState,
    pub tick: u32,
    /// The form flown after the newest generated command (its frame, and a change under way).
    pub form: Form,
    /// The arms after the newest generated command: a strike under way, how lately a weapon fired.
    pub arms: ArmsClock,
    /// The suit's stat sheet and flight modifiers, as the server's (AMBAC's with the arms idle).
    flying: Flying,
    /// The ticks flown, by tick: what the suit is drawn from, and what the server's state for the
    /// same tick is checked against.
    samples: Box<[Sample; HISTORY]>,
    /// Samples before this tick are from an earlier life.
    first: u32,
    /// Last measured |predicted − server| at the same tick, m (relocations excluded).
    pub last_error: f32,
    /// Server-side relocations that no prediction could foresee.
    pub relocations: u32,
    pub initialized: bool,
    /// The sector's debris field (from the Welcome), which the suit collides with as on the server.
    pub field: std::sync::Arc<Field>,
}

impl Default for Predictor {
    fn default() -> Self {
        Self {
            state: FlightState::default(),
            tick: 0,
            form: Form { frame: FrameId::Leo, timer: 0 },
            arms: ArmsClock::default(),
            flying: Flying::default(),
            samples: Box::new([Sample::NONE; HISTORY]),
            first: 0,
            last_error: 0.0,
            relocations: 0,
            initialized: false,
            field: std::sync::Arc::new(Field::empty()),
        }
    }
}

fn flight_from(own: &OwnState) -> FlightState {
    FlightState {
        pos: own.pos,
        vel: own.vel,
        rot: own.rot,
        ang_vel: own.ang_vel,
        propellant: own.propellant,
        g_load: 0.0,
        g_strain: own.g_strain,
        blackout: own.flags & own_flags::BLACKOUT != 0,
    }
}

/// What the own suit flies with between snapshots: its stat sheet (built from the snapshot as the
/// server builds it), the flight modifiers from it, and its slot (whose sputter it is).
#[derive(Clone, Copy, Debug, Default)]
struct Flying {
    mods: FlightMods,
    tuning: Tuning,
    slot: u16,
}

impl Predictor {
    fn mods_from(own: &OwnState) -> Flying {
        let tuning = own_tuning(own);
        Flying { mods: flight_mods(&tuning, false, own.extra_mass_kg), tuning, slot: own.slot }
    }

    /// The frame flown now.
    pub fn frame(&self) -> FrameId {
        self.form.frame
    }

    /// One command's tick, in the server's order: the form steps (a change drops a strike), the
    /// suit flies with the arms as they stood, then the arms move on.
    fn step(
        field: &Field,
        s: &mut FlightState,
        form: &mut Form,
        arms: &mut ArmsClock,
        flying: &Flying,
        cmd: &InputCmd,
    ) -> FlightOut {
        if transform_step(form, cmd.pressed(MODE)) {
            arms.drop_strike();
        }
        let spec = frame(form.frame);
        let mut mods = flying.mods;
        mods.main *= sputter(&flying.tuning, cmd.tick, flying.slot);
        if form.changing() {
            mods.thrust *= transform_thrust(form);
        }
        if arms.busy(cmd.tick) {
            mods.ambac = busy_ambac(mods.ambac);
        }
        mods.lunge = arms.lunging(spec);
        let out = step_in(field, s, cmd, spec, &mods, DT);
        arms.tick(spec, cmd, form.changing(), cmd.tick);
        out
    }

    /// Flies `cmd` from the newest state and keeps the tick.
    fn fly(&mut self, cmd: &InputCmd) {
        let out = Self::step(&self.field, &mut self.state, &mut self.form, &mut self.arms, &self.flying, cmd);
        self.keep(Sample::of(cmd.tick, &self.state, &out, self.form.frame, &self.arms));
    }

    fn keep(&mut self, s: Sample) {
        self.samples[s.tick as usize % HISTORY] = s;
    }

    /// The tick flown at `tick`, if it is still held.
    pub fn sample(&self, tick: u32) -> Option<&Sample> {
        let s = &self.samples[tick as usize % HISTORY];
        (s.tick == tick && tick >= self.first).then_some(s)
    }

    /// The sector's field, from the Welcome.
    pub fn set_field(&mut self, field: Field) {
        self.field = std::sync::Arc::new(field);
    }

    /// Rock `i` shattered (or grew back): the suit flies through where it was, as on the server.
    pub fn set_rock_dead(&mut self, i: usize, dead: bool) {
        if self.field.is_dead(i) != dead {
            std::sync::Arc::make_mut(&mut self.field).set_dead(i, dead);
        }
    }

    /// Forgets every tick flown before `tick`: a new life, and nothing before it is to be drawn
    /// or checked.
    pub fn forget_before(&mut self, tick: u32) {
        self.first = tick;
    }

    /// Steps the prediction with a newly generated command (already in `history`). Ticks skipped
    /// since the last one (the client stalled) are flown on the server's stand-ins first.
    pub fn advance(&mut self, cmd: &InputCmd, history: &InputHistory) {
        if !self.initialized || cmd.tick <= self.tick {
            return; // for a tick the server has already been heard from
        }
        if cmd.tick - self.tick <= MAX_GAP {
            let last = history.last_at_or_before(self.tick).unwrap_or_default();
            for t in self.tick + 1..cmd.tick {
                self.fly(&InputCmd::stand_in(&last, t, t - last.tick));
            }
        }
        self.fly(cmd);
        self.tick = cmd.tick;
    }

    /// Reconciles with the server's state at `server_tick`, replaying commands after it. A tick the
    /// client sent nothing for is flown as the server flies it: on a stand-in.
    pub fn reconcile(&mut self, server_tick: u32, own: &OwnState, history: &InputHistory) {
        // The server's form at that tick: a transformable frame's special timer is its change.
        let mut form = Form { frame: own.frame, timer: 0 };
        if matches!(frame(own.frame).special, bc_sim::content::SpecialKind::Transform { .. }) {
            form.timer = u16::from(own.special_timer);
        }
        self.form = form;
        self.flying = Self::mods_from(own);
        // The command the server flew that tick (the prediction's, or the stand-in it flew).
        let mut last = history.last_at_or_before(server_tick).unwrap_or_default();
        let flown = if last.tick == server_tick {
            last
        } else {
            InputCmd::stand_in(&last, server_tick, server_tick - last.tick)
        };
        self.arms = ArmsClock::from_own(own, server_tick, flown.buttons);
        if !own.alive {
            self.state = flight_from(own);
            self.tick = server_tick;
            self.initialized = true;
            return;
        }
        let kept = self.sample(server_tick).copied();
        if let Some(p) = kept {
            let e = (p.pos - own.pos).length();
            if e > RELOCATION {
                self.relocations += 1; // moved by the server: not a misprediction
            } else {
                self.last_error = e;
            }
        }
        // The server's state for that tick, with what the prediction knows it did over it (the G
        // and thrust aren't sent).
        let mut s = flight_from(own);
        let done =
            kept.unwrap_or(Sample { g_load: 0.0, throttle: Vec3::ZERO, g_limited: false, ..Sample::NONE });
        s.g_load = done.g_load;
        let out = FlightOut {
            boosting: own.flags & own_flags::BOOSTING != 0,
            throttle: done.throttle,
            g_limited: done.g_limited,
            ..FlightOut::default()
        };
        self.keep(Sample::of(server_tick, &s, &out, self.form.frame, &self.arms));
        self.state = s;
        let target = self.tick.max(server_tick);
        for t in server_tick + 1..=target {
            let cmd = match history.get(t) {
                Some(cmd) => {
                    last = cmd;
                    cmd
                }
                None => InputCmd::stand_in(&last, t, t - last.tick),
            };
            self.fly(&cmd);
        }
        self.tick = target;
        self.initialized = true;
    }

    /// The suit at time `t` (ticks), between the two ticks either side, or carried on at most a
    /// tick past the newest. Before the oldest tick held: that tick.
    pub fn pose_at(&self, t: f64) -> Option<OwnPose> {
        let newest = self.sample(self.tick)?;
        let last = f64::from(newest.tick);
        if t >= last {
            return Some(OwnPose::ahead(newest, (t - last).min(1.0) as f32));
        }
        let oldest = self.first.max(self.tick.saturating_sub(HISTORY as u32 - 1));
        let t = t.max(f64::from(oldest));
        let k = t.floor() as u32;
        let u = (t - f64::from(k)) as f32;
        Some(match (self.sample(k), self.sample(k + 1)) {
            (Some(a), Some(b)) => OwnPose::between(a, b, u),
            (Some(a), None) => OwnPose::ahead(a, u),
            (None, Some(b)) => OwnPose::ahead(b, 0.0),
            (None, None) => OwnPose::ahead(newest, 0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::buttons::FLIGHT_ASSIST;

    fn own_at(pos: Vec3) -> OwnState {
        OwnState { alive: true, frame: FrameId::Leo, pos, propellant: 2_400.0, ..OwnState::default() }
    }

    fn cmd(tick: u32, forward: i8) -> InputCmd {
        let aim = Vec3::new(0.3, 0.1, 1.0).normalize();
        InputCmd { tick, aim, thrust: [0, 0, forward], buttons: FLIGHT_ASSIST, ..InputCmd::default() }
            .quantized()
    }

    /// The server's flight from `own` at tick 100 through `to`, on stand-ins where `history` has no
    /// command.
    fn server_flies(own: &OwnState, history: &InputHistory, to: u32) -> FlightState {
        let (mut s, mods, field) = (flight_from(own), Predictor::mods_from(own), Field::empty());
        let mut form = Form { frame: own.frame, timer: 0 };
        let mut arms = ArmsClock::from_own(own, 100, 0);
        let mut last = InputCmd::default();
        for t in 101..=to {
            let c = match history.get(t) {
                Some(c) => {
                    last = c;
                    c
                }
                None => InputCmd::stand_in(&last, t, t - last.tick),
            };
            Predictor::step(&field, &mut s, &mut form, &mut arms, &mods, &c);
        }
        s
    }

    #[test]
    fn drawn_exactly_at_each_tick_and_smoothly_between() {
        let own = own_at(Vec3::new(0.0, 2_000.0, 0.0));
        let mut history = InputHistory::default();
        let mut p = Predictor::default();
        p.reconcile(100, &own, &history);
        let mut flown = vec![(100, p.state)];
        for t in 101..=160 {
            let c = cmd(t, if t < 130 { 127 } else { -127 });
            history.push(c);
            p.advance(&c, &history);
            flown.push((t, p.state));
        }
        for (t, s) in &flown {
            let pose = p.pose_at(f64::from(*t)).unwrap();
            assert_eq!(pose.pos, s.pos, "at tick {t}");
            assert!(pose.rot.dot(s.rot).abs() > 1.0 - 1e-6, "at tick {t}");
        }
        // Between ticks it never jumps: sampled a hundred times a tick, it moves no further than
        // the suit flies.
        let fastest = flown.iter().map(|(_, s)| s.vel.length()).fold(0.0, f32::max);
        let mut prev = p.pose_at(100.0).unwrap();
        for k in 1..=6_000 {
            let pose = p.pose_at(100.0 + f64::from(k) / 100.0).unwrap();
            assert!(pose.pos.distance(prev.pos) <= fastest * DT / 100.0 + 1e-4, "jumped at {k}");
            assert!(pose.rot.dot(prev.rot).abs() > 0.999_9, "turned at {k}");
            prev = pose;
        }
        // Past the newest tick it carries on, but no more than a tick.
        let newest = p.pose_at(160.0).unwrap();
        let ahead = p.pose_at(170.0).unwrap();
        assert!(ahead.pos.distance(newest.pos + newest.vel * DT) < 1e-3);
        // A new life forgets the old one: nothing before it is drawn.
        p.forget_before(150);
        assert!(p.sample(149).is_none() && p.sample(150).is_some());
        assert_eq!(p.pose_at(120.0).unwrap().pos, flown[50].1.pos);
    }

    #[test]
    fn prediction_flies_the_servers_stand_ins_through_a_gap() {
        let own = own_at(Vec3::new(0.0, 2_000.0, 0.0));
        let mut history = InputHistory::default();
        let mut p = Predictor::default();
        p.reconcile(100, &own, &history);
        // Commands for 101..=105, nothing for 106..=118 (a stall: repeats, then hands-off), then
        // 119..=121.
        for t in (101..=105).chain(119..=121) {
            let c = cmd(t, if t < 104 { 127 } else { -60 });
            history.push(c);
            p.advance(&c, &history);
        }
        let server = server_flies(&own, &history, 121);
        assert_eq!(p.tick, 121);
        assert_eq!(p.state, server, "advancing across the gap");
        // A snapshot from before the gap: the replay flies through it rather than stopping there.
        p.reconcile(100, &own, &history);
        assert_eq!(p.tick, 121);
        assert_eq!(p.state, server, "replaying across the gap");
        assert_eq!(p.sample(121).map(|s| s.pos), Some(server.pos));
    }
}
