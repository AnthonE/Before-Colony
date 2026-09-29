//! Own-suit prediction and reconciliation.
//!
//! The client steps its own suit with exactly the server's flight model (`bc_sim::flight`, bit-
//! identical on wasm) and the exact quantized commands it sent. When a snapshot arrives it rewinds
//! to the server's state for that tick and replays the unacknowledged commands. Any correction is
//! blended out visually rather than snapped.
//!
//! A change of form (Wing Zero ↔ Neo-Bird) is predicted too: each command steps the form as the
//! server does (`bc_sim::transform`), which sets the frame flown and cuts thrust mid-change.

use bc_proto::buttons::MODE;
use bc_proto::snapshot::own_flags;
use bc_proto::{FrameId, InputCmd, OwnState};
use bc_sim::DT;
use bc_sim::content::frame;
use bc_sim::field::Field;
use bc_sim::flight::{FlightMods, FlightState, step_in};
use bc_sim::transform::{Form, transform_step, transform_thrust};
use glam::Vec3;

use crate::inputs::InputHistory;

const HISTORY: usize = 128;
/// Jumps larger than this are server relocations (respawns), not prediction error, m.
const TELEPORT: f32 = 25.0;
/// The longest gap in the commands sent (a stall) that the prediction flies through on the
/// server's stand-ins as it goes; a longer one waits for the next snapshot.
const MAX_GAP: u32 = 32;

#[derive(Clone, Debug)]
pub struct Predictor {
    /// Predicted state after the newest generated command.
    pub state: FlightState,
    pub tick: u32,
    /// The form flown after the newest generated command (its frame, and a change under way).
    pub form: Form,
    mods: FlightMods,
    /// Predicted positions by tick, to measure prediction error when the server's arrives.
    predicted: [(u32, Vec3); HISTORY],
    /// Visual offset that decays toward zero after a correction.
    pub error: Vec3,
    /// Last measured |predicted − server| at the same tick, m (teleports excluded).
    pub last_error: f32,
    /// Server-side relocations (respawns) that no prediction could foresee.
    pub teleports: u32,
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
            mods: FlightMods::default(),
            predicted: [(u32::MAX, Vec3::ZERO); HISTORY],
            error: Vec3::ZERO,
            last_error: 0.0,
            teleports: 0,
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

impl Predictor {
    fn mods_from(own: &OwnState) -> FlightMods {
        FlightMods {
            ambac: own.ambac_factor,
            thrust: own.thrust_factor,
            g_immune: false,
            lunge: own.flags & own_flags::LUNGE != 0,
            extra_mass_kg: own.extra_mass_kg,
        }
    }

    /// The frame flown now.
    pub fn frame(&self) -> FrameId {
        self.form.frame
    }

    /// One command's tick: the form steps first (as on the server), then the flight.
    fn step(field: &Field, s: &mut FlightState, form: &mut Form, mods: &FlightMods, cmd: &InputCmd) {
        transform_step(form, cmd.pressed(MODE));
        let mut mods = *mods;
        if form.changing() {
            mods.thrust *= transform_thrust(form);
        }
        step_in(field, s, cmd, frame(form.frame), &mods, DT);
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

    /// Steps the prediction with a newly generated command (already in `history`). Ticks skipped
    /// since the last one (the client stalled) are flown on the server's stand-ins first.
    pub fn advance(&mut self, cmd: &InputCmd, history: &InputHistory) {
        if !self.initialized || cmd.tick <= self.tick {
            return; // for a tick the server has already been heard from
        }
        if cmd.tick - self.tick <= MAX_GAP {
            let last = history.last_at_or_before(self.tick).unwrap_or_default();
            for t in self.tick + 1..cmd.tick {
                let stand_in = InputCmd::stand_in(&last, t, t - last.tick);
                Self::step(&self.field, &mut self.state, &mut self.form, &self.mods, &stand_in);
                self.predicted[t as usize % HISTORY] = (t, self.state.pos);
            }
        }
        Self::step(&self.field, &mut self.state, &mut self.form, &self.mods, cmd);
        self.tick = cmd.tick;
        self.predicted[cmd.tick as usize % HISTORY] = (cmd.tick, self.state.pos);
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
        self.mods = Self::mods_from(own);
        if !own.alive {
            self.state = flight_from(own);
            self.tick = server_tick;
            self.initialized = true;
            self.error = Vec3::ZERO;
            return;
        }
        let (pt, ppos) = self.predicted[server_tick as usize % HISTORY];
        if pt == server_tick {
            let e = (ppos - own.pos).length();
            if e > TELEPORT {
                self.teleports += 1; // respawn: not a misprediction
            } else {
                self.last_error = e;
            }
        }
        let before = self.state.pos;
        let had = self.initialized;
        let mut s = flight_from(own);
        let target = self.tick.max(server_tick);
        let mut last = history.last_at_or_before(server_tick).unwrap_or_default();
        for t in server_tick + 1..=target {
            let cmd = match history.get(t) {
                Some(cmd) => {
                    last = cmd;
                    cmd
                }
                None => InputCmd::stand_in(&last, t, t - last.tick),
            };
            Self::step(&self.field, &mut s, &mut self.form, &self.mods, &cmd);
            self.predicted[t as usize % HISTORY] = (t, s.pos);
        }
        self.state = s;
        self.tick = target;
        self.initialized = true;
        if had {
            let jump = before - self.state.pos;
            // Big corrections (respawn, teleports) snap; small ones blend out.
            self.error = if jump.length() > TELEPORT { Vec3::ZERO } else { self.error + jump };
        }
    }

    /// Decays the visual correction; call once per rendered frame.
    pub fn decay(&mut self, frame_dt: f32) {
        self.error *= (-frame_dt / 0.12).exp();
    }

    /// Position to render the own suit at.
    pub fn render_pos(&self) -> Vec3 {
        self.state.pos + self.error
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
        let mut last = InputCmd::default();
        for t in 101..=to {
            let c = match history.get(t) {
                Some(c) => {
                    last = c;
                    c
                }
                None => InputCmd::stand_in(&last, t, t - last.tick),
            };
            Predictor::step(&field, &mut s, &mut form, &mods, &c);
        }
        s
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
        assert_eq!(p.predicted[121 % HISTORY], (121, server.pos));
    }
}
