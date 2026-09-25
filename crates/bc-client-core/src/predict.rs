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

const HISTORY: usize = 128;
/// Jumps larger than this are server relocations (respawns), not prediction error, m.
const TELEPORT: f32 = 25.0;

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

    /// Steps the prediction with a newly generated command.
    pub fn advance(&mut self, cmd: &InputCmd) {
        if !self.initialized {
            return;
        }
        Self::step(&self.field, &mut self.state, &mut self.form, &self.mods, cmd);
        self.tick = cmd.tick;
        self.predicted[cmd.tick as usize % HISTORY] = (cmd.tick, self.state.pos);
    }

    /// Reconciles with the server's state at `server_tick`, replaying commands after it.
    pub fn reconcile(&mut self, server_tick: u32, own: &OwnState, history: &crate::inputs::InputHistory) {
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
        let mut t = server_tick;
        let target = self.tick.max(server_tick);
        while t < target {
            t += 1;
            match history.get(t) {
                Some(cmd) => {
                    Self::step(&self.field, &mut s, &mut self.form, &self.mods, &cmd);
                    self.predicted[t as usize % HISTORY] = (t, s.pos);
                }
                None => break,
            }
        }
        self.state = s;
        self.tick = t;
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
