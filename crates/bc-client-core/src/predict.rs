//! Own-suit prediction and reconciliation.
//!
//! The client steps its own suit with exactly the server's flight model (`bc_sim::flight`, bit-
//! identical on wasm) and the exact quantized commands it sent. When a snapshot arrives it rewinds
//! to the server's state for that tick and replays the unacknowledged commands.
//!
//! Every tick flown is kept ([`Sample`]), so the suit is drawn between the last two
//! ([`Predictor::pose_at_on`]) rather than stepping at the tick rate, and a correction can be
//! measured where the suit was drawn; the client blends it out (`own`).
//!
//! A change of form (Wing Zero ↔ Neo-Bird) is predicted too: each command steps the form as the
//! server does (`bc_sim::transform`), which sets the frame flown and cuts thrust mid-change. So are
//! the arms (`bc_sim::arms`): a blade's lunge drives the suit, and busy arms slow its turning, on
//! the very ticks they do on the server.
//!
//! So are the bodies: each tick is the server's own step among them (`bc_sim::ground::move_step`),
//! so the suit is caught, lands, walks, hops and lets go on the very ticks it does there. On a body
//! the server sends it in the body's frame; it is seeded there, and how far the prediction was off
//! is measured there, so the body's motion (exact on both sides) never reads as an error. A rock
//! that shatters underfoot does so from the tick the server broke it, however late the news.

use bc_proto::buttons::{GRIP, MODE};
use bc_proto::snapshot::{footing, own_flags};
use bc_proto::{FrameId, InputCmd, OwnState, Part};
use bc_sim::arms::{ArmsClock, busy_ambac};
use bc_sim::bodies::{Bodies, Body, BodyPose, MAX_LANDMARKS, landmark_pose};
use bc_sim::config::MAX_REWIND_TICKS;
use bc_sim::content::frame;
use bc_sim::content::landmarks::{LANDMARKS, LandmarkDef};
use bc_sim::field::Field;
use bc_sim::flight::{FlightMods, FlightOut, FlightState};
use bc_sim::ground::{Anchor, Footing, MoveCtx, MoveOut, Mover, derive, move_step, place};
use bc_sim::math::integrate_rotation;
use bc_sim::transform::{Form, transform_step, transform_thrust};
use bc_sim::tuning::{FlightRules, Tuning, flight_mods, own_tuning, sputter};
use bc_sim::{DT, TICK_HZ};
use glam::{Quat, Vec3};

use crate::inputs::InputHistory;
use crate::interp::GroundPose;
use crate::surface::{body_pose, body_shape};

const HISTORY: usize = 128;
/// The server's suit this far from where the prediction had it at the same tick was moved there
/// (a relocation), not mispredicted, m.
pub const RELOCATION: f32 = 150.0;
/// The longest gap in the commands sent (a stall) that the prediction flies through on the
/// server's stand-ins as it goes; a longer one waits for the next snapshot.
const MAX_GAP: u32 = 32;
/// Rock breaks remembered, for replays that reach back before them.
const ROCK_DEATHS: usize = 64;

/// One tick the prediction flew: the suit's state after it, and what it did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u32,
    /// The suit in the sector's frame.
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
    /// How it stood with respect to the bodies, and on one, the body.
    pub footing: Footing,
    pub body: Body,
    /// On a body: the suit in its frame (the anchor), and how high it rides over the surface.
    pub local: Vec3,
    pub local_rot: Quat,
    pub local_vel: Vec3,
    pub local_ang_vel: Vec3,
    pub stance: f32,
    /// It landed over the tick, this fast into the surface, m/s.
    pub touchdown: Option<f32>,
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
        footing: Footing::Free,
        body: Body::None,
        local: Vec3::ZERO,
        local_rot: Quat::IDENTITY,
        local_vel: Vec3::ZERO,
        local_ang_vel: Vec3::ZERO,
        stance: 0.0,
        touchdown: None,
    };

    fn of(tick: u32, m: &Mover, out: &MoveOut, frame: FrameId, arms: &ArmsClock) -> Self {
        let (s, a) = (&m.flight, &m.anchor);
        Self {
            tick,
            pos: s.pos,
            vel: s.vel,
            rot: s.rot,
            ang_vel: s.ang_vel,
            g_load: s.g_load,
            g_strain: s.g_strain,
            blackout: s.blackout,
            boosting: out.flight.boosting,
            throttle: out.flight.throttle,
            g_limited: out.flight.g_limited,
            frame,
            strike: arms.striking(),
            footing: m.footing,
            body: a.body,
            local: a.local,
            local_rot: a.rot,
            local_vel: a.vel,
            local_ang_vel: a.ang_vel,
            stance: a.stance,
            touchdown: out.touchdown,
        }
    }

    /// The body it was on, if it was on one.
    pub fn on(&self) -> Option<Body> {
        (self.footing != Footing::Free).then_some(self.body)
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
    /// On a body: which, and how it stands on it.
    pub ground: Option<GroundPose>,
    /// It landed over the tick drawn, this fast into the surface, m/s.
    pub touchdown: Option<f32>,
}

impl OwnPose {
    /// The body it's drawn on, if it's on one.
    pub fn on(&self) -> Option<Body> {
        self.ground.map(|g| g.body)
    }

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
            ground: None,
            touchdown: b.touchdown,
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
            ground: None,
            touchdown: a.touchdown,
        }
    }

    /// This pose, in the frame of `body` as posed `then`, put on the body as posed `now`: where
    /// it's drawn if drawn glued to the body as the body is drawn.
    pub fn moved_with(&self, then: &BodyPose, now: &BodyPose) -> Self {
        let turn = now.rot * then.rot.conjugate();
        let local = then.to_local(self.pos);
        let pos = now.to_world(local);
        let rel = turn * (self.vel - then.point_vel(self.pos));
        let rel_d = turn * (self.dpos - then.point_vel(self.pos));
        Self {
            pos,
            rot: (turn * self.rot).normalize(),
            vel: now.point_vel(pos) + rel,
            dpos: now.point_vel(pos) + rel_d,
            ..*self
        }
    }
}

/// A suit on a body in the body's frame: its place, orientation and velocity there.
#[derive(Clone, Copy, Debug)]
struct Local {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
}

#[derive(Clone, Debug)]
pub struct Predictor {
    /// Predicted state after the newest generated command, in the sector's frame (on a body,
    /// where the body carries it).
    pub state: FlightState,
    /// How it stands with respect to the bodies after the newest command, and on one, where.
    footing: Footing,
    anchor: Anchor,
    pub tick: u32,
    /// The form flown after the newest generated command (its frame, and a change under way).
    pub form: Form,
    /// The arms after the newest generated command: a strike under way, how lately a weapon fired.
    pub arms: ArmsClock,
    /// The suit's stat sheet and flight modifiers, as the server's (AMBAC's with the arms idle).
    flying: Flying,
    /// The suit's legs are there (it can walk and hop).
    legs_ok: bool,
    /// What the server flies the suit on until the client's first command reaches it: the input
    /// the sector left it with (gripping, if it woke on a body).
    unheard: InputCmd,
    /// The ticks flown, by tick: what the suit is drawn from, and what the server's state for the
    /// same tick is checked against.
    samples: Box<[Sample; HISTORY]>,
    /// Samples before this tick are from an earlier life.
    first: u32,
    /// Last measured |predicted − server| at the same tick, m (relocations excluded); on the same
    /// body, in its frame.
    pub last_error: f32,
    /// Server-side relocations that no prediction could foresee.
    pub relocations: u32,
    pub initialized: bool,
    /// The sector's debris field (from the Welcome), which the suit collides with as on the server.
    pub field: std::sync::Arc<Field>,
    /// How many of the landmarks the sector has (from the Welcome): as solid here as there.
    landmarks: u8,
    /// Rocks that shattered, and the first tick each is gone for (the tick after the server
    /// broke it): a replay from before then still stands on it.
    rock_deaths: Vec<(u16, u32)>,
    /// How the sector's suits fly (from the Welcome).
    rules: FlightRules,
    /// The sector is the colony's inside (from the Welcome): suits fly its pull, its air and its
    /// city (`bc_sim::colony::interior`).
    interior: bool,
}

impl Default for Predictor {
    fn default() -> Self {
        Self {
            state: FlightState::default(),
            footing: Footing::Free,
            anchor: Anchor::default(),
            tick: 0,
            form: Form { frame: FrameId::Leo, timer: 0 },
            arms: ArmsClock::default(),
            flying: Flying::default(),
            legs_ok: true,
            unheard: InputCmd::default(),
            samples: Box::new([Sample::NONE; HISTORY]),
            first: 0,
            last_error: 0.0,
            relocations: 0,
            initialized: false,
            field: std::sync::Arc::new(Field::empty()),
            // Every one of them, until the Welcome says how many the sector has.
            landmarks: LANDMARKS.len() as u8,
            rock_deaths: Vec::new(),
            rules: FlightRules::Real,
            interior: false,
        }
    }
}

/// The server's suit from `own`, with the bodies at the snapshot's tick: on one, its anchor there
/// (sent in the body's frame) and its world state derived from it exactly as the server derives
/// it. A body this sector doesn't have can't be stood on: the state is taken as it came.
fn mover_from(own: &OwnState, bodies: &Bodies) -> Mover {
    let mut m = Mover {
        flight: FlightState {
            pos: own.pos,
            vel: own.vel,
            rot: own.rot,
            ang_vel: own.ang_vel,
            propellant: own.propellant,
            g_load: 0.0,
            g_strain: own.g_strain,
            blackout: own.flags & own_flags::BLACKOUT != 0,
        },
        footing: Footing::Free,
        anchor: Anchor::default(),
    };
    let Some(on) = own.surface else { return m };
    let body = Body::from(on.body);
    let Some(pose) = bodies.pose(body) else { return m };
    m.footing = match on.footing {
        footing::GROUNDED => Footing::Grounded,
        footing::ALOFT => Footing::Aloft,
        _ => return m,
    };
    m.anchor = Anchor {
        body,
        local: own.pos,
        rot: own.rot,
        vel: own.vel,
        ang_vel: own.ang_vel,
        stance: f32::from(on.stance_q) / 16.0,
    };
    derive(&pose, &m.anchor, &mut m.flight);
    m
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
    fn mods_from(own: &OwnState, rules: FlightRules) -> Flying {
        let tuning = own_tuning(own);
        Flying { mods: flight_mods(&tuning, rules, false, own.extra_mass_kg), tuning, slot: own.slot }
    }

    /// How the sector's suits fly, from the Welcome.
    pub fn set_rules(&mut self, rules: FlightRules) {
        self.rules = rules;
    }

    /// Whether the sector is the colony's inside, from the Welcome.
    pub fn set_interior(&mut self, interior: bool) {
        self.interior = interior;
    }

    pub fn interior(&self) -> bool {
        self.interior
    }

    pub fn rules(&self) -> FlightRules {
        self.rules
    }

    /// The frame flown now.
    pub fn frame(&self) -> FrameId {
        self.form.frame
    }

    /// The suit as the server's step moves it, after the newest generated command: its world
    /// state, and how it stands on a body.
    pub fn mover(&self) -> Mover {
        Mover { flight: self.state, footing: self.footing, anchor: self.anchor }
    }

    fn set_mover(&mut self, m: &Mover) {
        (self.state, self.footing, self.anchor) = (m.flight, m.footing, m.anchor);
    }

    /// One command's tick, in the server's order: the form steps (a change drops a strike), the
    /// suit moves among the bodies with the arms as they stood, then the arms move on.
    fn step(
        bodies: &Bodies,
        m: &mut Mover,
        form: &mut Form,
        arms: &mut ArmsClock,
        flying: &Flying,
        legs_ok: bool,
        cmd: &InputCmd,
    ) -> MoveOut {
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
        if mods.interior {
            let flight = bc_sim::colony::interior::step(&mut m.flight, cmd, spec, &mods, DT);
            arms.tick(spec, cmd, form.changing(), cmd.tick);
            return MoveOut { flight, touchdown: None, caught: false, released: false };
        }
        let cx = MoveCtx { spec, mods, can_grip: spec.has_legs() && !form.changing(), legs_ok };
        let out = move_step(bodies, m, cmd, &cx, DT);
        arms.tick(spec, cmd, form.changing(), cmd.tick);
        out
    }

    /// Flies `cmd` from the newest state and keeps the tick.
    fn fly(&mut self, cmd: &InputCmd) {
        self.rocks_as_at(cmd.tick);
        let mut m = self.mover();
        let bodies = Bodies::at(&self.field, self.landmarks(), cmd.tick);
        let out =
            Self::step(&bodies, &mut m, &mut self.form, &mut self.arms, &self.flying, self.legs_ok, cmd);
        self.set_mover(&m);
        self.keep(Sample::of(cmd.tick, &m, &out, self.form.frame, &self.arms));
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
        self.rock_deaths.clear();
    }

    /// How many landmarks the sector has, from the Welcome (no more than this build knows of).
    pub fn set_landmarks(&mut self, n: u8) {
        self.landmarks = n.min(LANDMARKS.len().min(MAX_LANDMARKS) as u8);
    }

    /// The sector's landmarks, by id.
    pub fn landmarks(&self) -> &'static [LandmarkDef] {
        &LANDMARKS[..usize::from(self.landmarks)]
    }

    /// The sector's bodies (its field and landmarks) at tick `t`.
    pub fn bodies(&self, t: u32) -> Bodies<'_> {
        Bodies::at(&self.field, self.landmarks(), t)
    }

    /// Where `body` is at time `t` (ticks, fractional).
    pub fn body_pose(&self, body: Body, t: f64) -> Option<BodyPose> {
        body_pose(&self.field, self.landmarks(), body, t)
    }

    /// The normal of the ground under the suit (sector frame) while it stands on a body, else zero:
    /// which way is up off it, for perception and the ZERO System, as the server has it.
    pub fn surface_n(&self) -> Vec3 {
        let a = &self.anchor;
        if self.footing != Footing::Grounded {
            return Vec3::ZERO;
        }
        match (
            self.body_pose(a.body, f64::from(self.tick)),
            body_shape(&self.field, self.landmarks(), a.body),
        ) {
            (Some(p), Some(shape)) => p.rot * place(&shape, a.local, a.stance).1,
            _ => Vec3::ZERO,
        }
    }

    /// The suit after `cmd`'s tick as its pilot saw it when it fired: on a body that moves, on the
    /// body as it was at the command's view tick (as far back as lag compensation goes), exactly
    /// as the server places a shot's muzzle and a blade (its view shift). Free, or on a body that
    /// doesn't move, it's where it is.
    pub fn as_seen(&self, cmd: &InputCmd) -> FlightState {
        let mut f = self.state;
        let view_q4 = cmd.view_tick_q4.max(cmd.tick.saturating_sub(MAX_REWIND_TICKS) << 4);
        let rewind = cmd.tick.saturating_sub(view_q4 >> 4);
        if rewind == 0 || self.footing == Footing::Free {
            return f;
        }
        let Body::Landmark(k) = self.anchor.body else { return f };
        let Some(d) = self.landmarks().get(usize::from(k)) else { return f };
        let pose = landmark_pose(d, cmd.tick - rewind, (view_q4 & 15) as f32 / 16.0);
        if pose.moving {
            derive(&pose, &self.anchor, &mut f);
        }
        f
    }

    /// Rock `i` shattered (or grew back), as a rock record says. A shattering the server's event
    /// has dated is left to that date ([`Predictor::note_rock_break`]); one it hasn't (the event
    /// was never heard) is taken as from now. Growing back is from now.
    pub fn set_rock_dead(&mut self, i: usize, dead: bool) {
        if dead && self.rock_deaths.iter().any(|&(r, _)| usize::from(r) == i) {
            return;
        }
        if !dead {
            self.rock_deaths.retain(|&(r, _)| usize::from(r) != i);
        }
        if self.field.is_dead(i) != dead {
            std::sync::Arc::make_mut(&mut self.field).set_dead(i, dead);
        }
    }

    /// Rock `rock` is gone from tick `from` on (the server broke it the tick before, and its
    /// riders let go on this one): a replay of earlier ticks still has it.
    pub fn note_rock_break(&mut self, rock: u16, from: u32) {
        match self.rock_deaths.iter_mut().find(|(r, _)| *r == rock) {
            Some(d) => d.1 = from,
            None => {
                if self.rock_deaths.len() == ROCK_DEATHS {
                    self.rock_deaths.remove(0);
                }
                self.rock_deaths.push((rock, from));
            }
        }
    }

    /// The dated rock breaks as they stood at tick `t`.
    fn rocks_as_at(&mut self, t: u32) {
        for &(r, from) in &self.rock_deaths {
            let (i, dead) = (usize::from(r), t >= from);
            if self.field.is_dead(i) != dead {
                std::sync::Arc::make_mut(&mut self.field).set_dead(i, dead);
            }
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
            let last = history.last_at_or_before(self.tick).unwrap_or(self.unheard);
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
        self.flying = Self::mods_from(own, self.rules);
        self.flying.mods.interior = self.interior;
        self.legs_ok = own.parts[Part::Legs as usize] > 0.0;
        // The server's state for that tick (the G and thrust aren't sent; see below).
        let mut m = mover_from(own, &self.bodies(server_tick));
        // Before any command of the client's, the server stood in with what it left the suit
        // with: on a body (it woke there), its grip.
        let grip = if m.footing == Footing::Free { 0 } else { GRIP };
        self.unheard = InputCmd::neutral(server_tick, m.flight.rot * Vec3::Z, grip);
        // The command the server flew that tick (the prediction's, or the stand-in it flew).
        let mut last = history.last_at_or_before(server_tick).unwrap_or(self.unheard);
        let flown = if last.tick == server_tick {
            last
        } else {
            InputCmd::stand_in(&last, server_tick, server_tick - last.tick)
        };
        self.arms = ArmsClock::from_own(own, server_tick, flown.buttons);
        // A break dated at or before the snapshot is behind every tick replayed from it.
        let (field, deaths) = (&mut self.field, &mut self.rock_deaths);
        deaths.retain(|&(r, from)| {
            if from > server_tick {
                return true;
            }
            if !field.is_dead(usize::from(r)) {
                std::sync::Arc::make_mut(field).set_dead(usize::from(r), true);
            }
            false
        });
        // With what the prediction knows it did over that tick (the G and thrust aren't sent).
        self.rocks_as_at(server_tick);
        if !own.alive {
            self.set_mover(&m);
            self.tick = server_tick;
            self.initialized = true;
            return;
        }
        let kept = self.sample(server_tick).copied();
        if let Some(p) = kept {
            // On the same body, in its frame: the body is where both say.
            let e = match p.on() {
                Some(b) if m.footing != Footing::Free && m.anchor.body == b => {
                    (p.local - m.anchor.local).length()
                }
                _ => (p.pos - m.flight.pos).length(),
            };
            if e > RELOCATION {
                self.relocations += 1; // moved by the server: not a misprediction
            } else {
                self.last_error = e;
            }
        }
        let done =
            kept.unwrap_or(Sample { g_load: 0.0, throttle: Vec3::ZERO, g_limited: false, ..Sample::NONE });
        m.flight.g_load = done.g_load;
        let out = MoveOut {
            flight: FlightOut {
                boosting: own.flags & own_flags::BOOSTING != 0,
                throttle: done.throttle,
                g_limited: done.g_limited,
                ..FlightOut::default()
            },
            touchdown: done.touchdown,
            ..MoveOut::default()
        };
        self.keep(Sample::of(server_tick, &m, &out, self.form.frame, &self.arms));
        self.set_mover(&m);
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
    /// tick past the newest. Before the oldest tick held: that tick. On a body, with the body
    /// where it is then.
    pub fn pose_at(&self, t: f64) -> Option<OwnPose> {
        self.pose_at_on(t, t)
    }

    /// The suit at time `t_own` (ticks) as [`Predictor::pose_at`], but on a body, put on the
    /// body as it is at `t_body`: its place on the body as predicted, on the body as drawn. Flying
    /// free, `t_body` is beside the point.
    pub fn pose_at_on(&self, t_own: f64, t_body: f64) -> Option<OwnPose> {
        let newest = self.sample(self.tick)?;
        let last = f64::from(newest.tick);
        if t_own >= last {
            return Some(self.ahead(newest, (t_own - last).min(1.0) as f32, t_body));
        }
        let oldest = self.first.max(self.tick.saturating_sub(HISTORY as u32 - 1));
        let t = t_own.max(f64::from(oldest));
        let k = t.floor() as u32;
        let u = (t - f64::from(k)) as f32;
        Some(match (self.sample(k), self.sample(k + 1)) {
            (Some(a), Some(b)) => self.between(a, b, u, t_body),
            (Some(a), None) => self.ahead(a, u, t_body),
            (None, Some(b)) => self.ahead(b, 0.0, t_body),
            (None, None) => self.ahead(newest, 0.0, t_body),
        })
    }

    /// `u` of the way from tick `a` to tick `b`. On a body after `b`, in its frame (`a` carried
    /// into it exactly at its own tick if it was in another), then put on the body at `t_body`.
    fn between(&self, a: &Sample, b: &Sample, u: f32, t_body: f64) -> OwnPose {
        let world = OwnPose::between(a, b, u);
        let Some(body) = b.on() else { return world };
        let (Some(now), Some(at_a)) = (self.body_pose(body, t_body), self.body_pose(body, f64::from(a.tick)))
        else {
            return world;
        };
        let la = if a.on() == Some(body) {
            Local { pos: a.local, rot: a.local_rot, vel: a.local_vel }
        } else {
            let inv = at_a.rot.conjugate();
            Local { pos: at_a.to_local(a.pos), rot: inv * a.rot, vel: inv * (a.vel - at_a.point_vel(a.pos)) }
        };
        let rb = if la.rot.dot(b.local_rot) < 0.0 { -b.local_rot } else { b.local_rot };
        let local = Local {
            pos: la.pos.lerp(b.local, u),
            rot: la.rot.lerp(rb, u).normalize(),
            vel: la.vel.lerp(b.local_vel, u),
        };
        let chord = (b.local - la.pos) * TICK_HZ as f32;
        self.on_body(world, body, b.footing, &now, local, chord)
    }

    /// `u` ticks on from tick `a`: on a body, carried on over it, on the body at `t_body`.
    fn ahead(&self, a: &Sample, u: f32, t_body: f64) -> OwnPose {
        let world = OwnPose::ahead(a, u);
        let Some(body) = a.on() else { return world };
        let Some(now) = self.body_pose(body, t_body) else { return world };
        let local = Local {
            pos: a.local + a.local_vel * (u * DT),
            rot: integrate_rotation(a.local_rot, a.local_ang_vel, u * DT),
            vel: a.local_vel,
        };
        self.on_body(world, body, a.footing, &now, local, a.local_vel)
    }

    /// `pose` (for all but its place and motion) with the suit at `local` on `body`, posed `p`;
    /// `chord` is how fast it moves over the body as drawn.
    fn on_body(
        &self,
        pose: OwnPose,
        body: Body,
        footing: Footing,
        p: &BodyPose,
        local: Local,
        chord: Vec3,
    ) -> OwnPose {
        let pos = p.to_world(local.pos);
        let probe = body_shape(&self.field, self.landmarks(), body).map(|s| s.probe(local.pos));
        let ground = GroundPose {
            body,
            aloft: footing == Footing::Aloft,
            up: p.rot * probe.map_or(Vec3::Y, |pr| pr.normal),
            rel_vel: p.rot * local.vel,
            height: probe.map_or(0.0, |pr| pr.dist),
        };
        OwnPose {
            pos,
            rot: (p.rot * local.rot).normalize(),
            vel: p.point_vel(pos) + p.rot * local.vel,
            dpos: p.point_vel(pos) + p.rot * chord,
            ground: Some(ground),
            ..pose
        }
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
        let field = Field::empty();
        let (mut m, mods) = (
            mover_from(own, &Bodies::at(&field, &LANDMARKS, 100)),
            Predictor::mods_from(own, FlightRules::Real),
        );
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
            let bodies = Bodies::at(&field, &LANDMARKS, t);
            Predictor::step(&bodies, &mut m, &mut form, &mut arms, &mods, true, &c);
        }
        m.flight
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
    fn an_own_state_on_a_body_seeds_the_prediction_where_the_body_carries_it() {
        use bc_proto::BodyRef;
        use bc_proto::snapshot::{OwnSurface, footing};
        use bc_sim::bodies::Body;

        let local = Vec3::new(-230.0, 0.0, 89.125);
        let own = OwnState {
            pos: local,
            vel: Vec3::new(1.0, 0.0, 0.0),
            surface: Some(OwnSurface {
                footing: footing::GROUNDED,
                body: BodyRef::Landmark(0),
                stance_q: 146,
            }),
            ..own_at(local)
        };
        let mut p = Predictor::default();
        p.reconcile(4_000, &own, &InputHistory::default());
        let deck = p.bodies(4_000).pose(Body::Landmark(0)).unwrap();
        assert_eq!(p.state.pos, deck.to_world(local));
        assert_eq!(p.state.vel, deck.point_vel(p.state.pos) + deck.rot * own.vel);
        assert!(p.state.rot.dot(deck.rot).abs() > 1.0 - 1e-6);
        // A sector without it: the state can't be placed, and is taken as it came.
        p.set_landmarks(0);
        p.reconcile(4_001, &own, &InputHistory::default());
        assert_eq!(p.state.pos, local);
    }

    #[test]
    fn until_its_first_command_a_suit_woken_on_a_body_keeps_its_grip() {
        use bc_proto::BodyRef;
        use bc_proto::buttons::GRIP;
        use bc_proto::snapshot::{OwnSurface, footing};

        // Woken on its feet on MO-II's aft module. Until the client's first command reaches the
        // server, the server flies the suit on what it left it with, gripping: so does the
        // prediction, through the ticks before that command.
        let local = Vec3::new(-230.0, 0.0, 80.0 + bc_sim::ground::STANCE);
        let own = OwnState {
            pos: local,
            rot: bc_sim::math::look_rotation(Vec3::X, Vec3::Z),
            surface: Some(OwnSurface {
                footing: footing::GROUNDED,
                body: BodyRef::Landmark(0),
                stance_q: 146,
            }),
            ..own_at(local)
        };
        let mut history = InputHistory::default();
        let mut p = Predictor::default();
        p.reconcile(4_000, &own, &history);
        let first =
            InputCmd { tick: 4_006, aim: p.state.rot * Vec3::Z, buttons: GRIP, ..InputCmd::default() }
                .quantized();
        history.push(first);
        p.advance(&first, &history);
        // Replayed from the server's word of a later tick before the command, the same.
        p.reconcile(4_002, &own, &history);
        for t in 4_001..=4_006 {
            let s = p.sample(t).unwrap_or_else(|| panic!("tick {t} flown"));
            assert_eq!((s.footing, s.body), (Footing::Grounded, Body::Landmark(0)), "let go at {t}");
            assert!(s.local.distance(local) < 1e-3, "moved off its spot at {t}");
        }
    }

    #[test]
    fn on_a_body_the_suit_is_drawn_on_the_body_as_drawn() {
        use bc_proto::BodyRef;
        use bc_proto::buttons::GRIP;
        use bc_proto::snapshot::{OwnSurface, footing};

        // Standing still on MO-II's aft module, its grip held.
        let local = Vec3::new(-230.0, 0.0, 80.0 + bc_sim::ground::STANCE);
        let own = OwnState {
            pos: local,
            rot: bc_sim::math::look_rotation(Vec3::X, Vec3::Z),
            surface: Some(OwnSurface {
                footing: footing::GROUNDED,
                body: BodyRef::Landmark(0),
                stance_q: 146,
            }),
            ..own_at(local)
        };
        let mut history = InputHistory::default();
        let mut p = Predictor::default();
        p.reconcile(4_000, &own, &history);
        for t in 4_001..=4_010 {
            let c = InputCmd { tick: t, aim: p.state.rot * Vec3::Z, buttons: GRIP, ..InputCmd::default() }
                .quantized();
            history.push(c);
            p.advance(&c, &history);
        }
        let mo_ii = |t: f64| p.body_pose(Body::Landmark(0), t).unwrap();
        // Its place on the body at the input clock's time, on the body as at the view clock's.
        for (own_t, view_t) in [(4_008.25, 4_002.5), (4_009.0, 4_001.0), (4_010.4, 4_004.0)] {
            let pose = p.pose_at_on(own_t, view_t).unwrap();
            let deck = mo_ii(view_t);
            assert!(pose.pos.distance(deck.to_world(local)) < 1e-3, "at {own_t} on {view_t}");
            assert!(pose.vel.distance(deck.point_vel(pose.pos)) < 1e-3);
            assert!(pose.dpos.distance(deck.point_vel(pose.pos)) < 1e-3, "moves with the deck as drawn");
            let g = pose.ground.expect("on MO-II");
            assert!(g.body == Body::Landmark(0) && !g.aloft && (g.height - 9.125).abs() < 1e-3);
        }
        // On one clock, it's where it truly is.
        let pose = p.pose_at(4_009.0).unwrap();
        assert!(pose.pos.distance(p.sample(4_009).unwrap().pos) < 1e-3);
        // Seeded where the server had it, and measured in the body's frame: no error at all.
        p.reconcile(4_005, &own, &history);
        assert_eq!(p.last_error, 0.0);
    }

    #[test]
    fn a_shot_from_a_rider_leaves_where_the_server_fires_it() {
        use bc_proto::buttons::{FIRE_PRIMARY, GRIP};
        use bc_proto::events::Event;
        use bc_proto::{Faction, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter};
        use bc_sim::math::look_rotation;
        use bc_sim::{Sim, SimConfig};

        let wire = |own: &OwnState| {
            let mut buf = [0u8; 256];
            let mut w = SnapshotWriter::new(&mut buf, 256);
            w.header(&SnapshotHeader::default());
            w.own(Some(own));
            let n = w.finish().unwrap();
            SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
        };
        // A Leo standing on MO-II's rim, its pilot seeing the world 6 ticks back (lag compensated),
        // fires: the server puts the muzzle on the deck as it was then, where the pilot saw it.
        let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
        let id = sim
            .spawn_at(
                FrameId::Leo,
                Faction::Colonies,
                PilotKind::Human,
                Vec3::splat(5_000.0),
                look_rotation(Vec3::Z, Vec3::Y),
            )
            .unwrap();
        assert!(sim.place_on(id, Body::Landmark(0), Vec3::new(-230.0, 80.0, 80.0)));
        let i = id.idx();
        let mut history = InputHistory::default();
        let mut owns = vec![wire(&sim.own_state(i))];
        let mut origins = Vec::new();
        for t in 1..=40 {
            let aim = sim.suits.flight[i].rot * Vec3::Z;
            let fire = if t >= 30 { FIRE_PRIMARY } else { 0 };
            let c = InputCmd {
                tick: t,
                view_tick_q4: (t - 6) << 4 | 8,
                aim,
                buttons: GRIP | fire,
                ..InputCmd::default()
            }
            .quantized();
            history.push(c);
            sim.set_input(id, c);
            let from = sim.events.next_seq();
            sim.step();
            owns.push(wire(&sim.own_state(i)));
            for e in (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s)) {
                if let Event::BeamSpawn { origin, tick, .. } = *e {
                    origins.push((t, tick, origin));
                }
            }
        }
        let (t, spawn, origin) = *origins.first().expect("it fired");
        assert_eq!(spawn, t - 6, "lag compensated");
        // The client's prediction of the same tick, from the snapshot before it.
        let mut p = Predictor::default();
        p.reconcile(t - 1, &owns[t as usize - 1], &history);
        let cmd = history.get(t).unwrap();
        p.advance(&cmd, &history);
        let mount = bc_sim::content::frame(FrameId::Leo).loadout[0].unwrap();
        let f = p.as_seen(&cmd);
        let muzzle = f.pos + f.rot * mount.arm.muzzle();
        assert!(muzzle.distance(origin) < 1e-3, "{} m off the server's", muzzle.distance(origin));
        // Not where the suit is now: the deck has moved on since the pilot's view.
        let now = p.state.pos + p.state.rot * mount.arm.muzzle();
        assert!(now.distance(origin) > 0.1, "{} m", now.distance(origin));
    }

    #[test]
    fn rock_death_is_tick_stamped() {
        use bc_proto::buttons::GRIP;
        use bc_proto::{Faction, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter};
        use bc_sim::math::look_rotation;
        use bc_sim::{Sim, SimConfig};

        let wire = |own: &OwnState| {
            let mut buf = [0u8; 256];
            let mut w = SnapshotWriter::new(&mut buf, 256);
            w.header(&SnapshotHeader::default());
            w.own(Some(own));
            let n = w.finish().unwrap();
            SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
        };
        // A Leo standing on a rock, which breaks during tick 20: the server lets it go on 21.
        let cfg = SimConfig { target_dolls: 0, ..SimConfig::default() };
        let mut sim = Sim::new(cfg);
        let r = (0..sim.field.len()).find(|&i| sim.field.rocks()[i].axes.min_element() >= 12.0).unwrap();
        let id = sim
            .spawn_at(
                FrameId::Leo,
                Faction::Colonies,
                PilotKind::Human,
                Vec3::splat(5_000.0),
                look_rotation(Vec3::Z, Vec3::Y),
            )
            .unwrap();
        assert!(sim.place_on(id, Body::Rock(r as u16), Vec3::Y));
        let mut history = InputHistory::default();
        let mut owns = vec![wire(&sim.own_state(id.idx()))];
        let mut footings = vec![sim.footing(id.idx())];
        for t in 1..=30 {
            let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
            let c = InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons: GRIP, ..InputCmd::default() }
                .quantized();
            history.push(c);
            sim.set_input(id, c);
            sim.step();
            if t == 20 {
                sim.field.set_dead(r, true);
            }
            owns.push(wire(&sim.own_state(id.idx())));
            footings.push(sim.footing(id.idx()));
        }
        assert_eq!((footings[20], footings[21]), (Footing::Grounded, Footing::Free));
        // Seeded before the break, with the news of it (the break, and the rock's record): the
        // replay still stands on the rock until the server's tick, then lets go with it.
        let replay = |dated: bool| {
            let mut p = Predictor::default();
            p.set_field(Field::generate(cfg.field_seed, cfg.field_rocks));
            if dated {
                p.note_rock_break(r as u16, 21);
            }
            p.set_rock_dead(r, true);
            p.reconcile(15, &owns[15], &history);
            for t in 16..=30 {
                p.advance(&history.get(t).unwrap(), &history);
            }
            p
        };
        let p = replay(true);
        for t in 16..=30 {
            assert_eq!(p.sample(t).map(|s| s.footing), Some(footings[t as usize]), "tick {t}");
        }
        assert!(p.field.is_dead(r), "and it's gone now");
        // Without its date, it's taken as gone from the snapshot on: the suit lets go too soon.
        let p = replay(false);
        assert_eq!(p.sample(16).map(|s| s.footing), Some(Footing::Free));
        // Once the server has gone past it, the date is behind every replay.
        let mut p = replay(true);
        p.reconcile(25, &owns[25], &history);
        assert!(p.rock_deaths.is_empty() && p.field.is_dead(r));
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
