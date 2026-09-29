//! The flight model, shared bit-for-bit by the server and the client's own-suit prediction.
//!
//! - **Newtonian 6DOF**, no drag. Thrust is limited per axis (main, side, retro) and burns
//!   propellant at `|F| / (Isp·g0)`, so mass drops and delta-v is finite.
//! - **AMBAC** turns the suit by swinging its limbs: free but modest. Damaged limbs and busy arms
//!   reduce it. **RCS** (held) adds much more authority at a propellant cost.
//! - **Flight assist** turns the thrust stick into a velocity command (it brakes to a stop when
//!   released); off, the stick is raw thrust and velocity persists. **Brake** always retro-burns.
//!   A pilot's flight assist eases onto the velocity asked for (a fifth of a second) and spares
//!   their body: short of boost, it never pulls more G than they bear for good. A Mobile Doll's
//!   snaps onto it at full thrust.
//! - **Pilot G**: sustained load above tolerance builds G-strain. At 1.0 the pilot blacks out and
//!   control authority collapses until it recovers below 0.5. Mobile Dolls have no body to protect.

use bc_proto::InputCmd;
use bc_proto::buttons::{BOOST, BRAKE, FLIGHT_ASSIST, RCS_SHARP};
use glam::{Quat, Vec3};

use crate::config::G0;
use crate::content::FrameSpec;
use crate::field::Field;
use crate::math::{atan2, integrate_rotation, length, normalize_or, sqrt};

/// Sustained G a trained human pilot tolerates before strain builds.
pub const HUMAN_G_TOLERANCE: f32 = 6.0;
/// Strain per second per G above tolerance (divided by this).
const STRAIN_GAIN_DIV: f32 = 4.0;
const STRAIN_RECOVERY: f32 = 0.35;
/// How hard the attitude controller chases the aim (1/s).
const AIM_GAIN: f32 = 3.0;
/// The share of its turning authority the attitude controller plans to brake with: it never turns
/// faster than it can stop from in time, so it settles on the aim instead of swinging past.
const ATTITUDE_BRAKE: f32 = 0.8;
/// How long a pilot's flight assist takes to close a gap in velocity (s): short of full thrust, it
/// eases onto the velocity asked for rather than snapping, so G fades in and out.
pub const FA_RESPONSE: f32 = 0.2;
/// A gap this small (m/s) flight assist closes in one tick: the last of it, not a trickle forever.
const FA_SETTLE: f32 = 0.005;
/// Holding boost raises flight assist's cruise speed by this much.
pub const FA_BOOST_CRUISE: f32 = 1.8;
/// Flight assist holds a pilot just under what they bear for good, g, unless they boost.
pub const FA_G_CAP: f32 = HUMAN_G_TOLERANCE - 0.03;
/// A blade's lunge drives forward at this much of full main thrust (never boosted).
pub const LUNGE_THRUST: f32 = 1.5;

/// Kinematic and pilot state of a suit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightState {
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    /// World-frame angular velocity, rad/s.
    pub ang_vel: Vec3,
    /// kg
    pub propellant: f32,
    /// Felt acceleration last tick, in g.
    pub g_load: f32,
    pub g_strain: f32,
    pub blackout: bool,
}

impl Default for FlightState {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            ang_vel: Vec3::ZERO,
            propellant: 0.0,
            g_load: 0.0,
            g_strain: 0.0,
            blackout: false,
        }
    }
}

/// Modifiers from damage and what the arms are doing.
#[derive(Clone, Copy, Debug)]
pub struct FlightMods {
    /// AMBAC authority 0..1 (lost limbs, busy arms).
    pub ambac: f32,
    /// Thrust authority 0..1 (damaged backpack/legs).
    pub thrust: f32,
    /// Mobile Dolls: no G-strain.
    pub g_immune: bool,
    /// Beam saber lunge: full forward thrust at [`LUNGE_THRUST`]×.
    pub lunge: bool,
    /// Mass beyond the frame's own (cargo, a chunk in hand) less the parts shot off, kg. Whole
    /// kilograms, so prediction uses exactly the server's number.
    pub extra_mass_kg: i32,
}

impl Default for FlightMods {
    fn default() -> Self {
        Self { ambac: 1.0, thrust: 1.0, g_immune: false, lunge: false, extra_mass_kg: 0 }
    }
}

/// What the step did (for visuals and signatures).
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightOut {
    pub boosting: bool,
    /// World-frame acceleration applied, m/s².
    pub accel: Vec3,
    /// Thrust applied along each local axis (x right, y up, z forward), as a fraction of that
    /// axis' unboosted maximum: what the thrusters are actually doing, whatever the stick says.
    /// Boost and a lunge take it past 1.
    pub throttle: Vec3,
    /// Flight assist held the pilot's G down this tick.
    pub g_limited: bool,
}

#[inline]
fn clamp_axis(f: f32, pos_max: f32, neg_max: f32) -> f32 {
    f.clamp(-neg_max, pos_max)
}

/// [`step`], then kept out of the rocks: what the server and the client's prediction both run.
pub fn step_in(
    field: &Field,
    s: &mut FlightState,
    cmd: &InputCmd,
    spec: &FrameSpec,
    mods: &FlightMods,
    dt: f32,
) -> FlightOut {
    let prev = s.pos;
    let out = step(s, cmd, spec, mods, dt);
    field.collide(prev, s);
    out
}

/// Advances one suit by `dt` under `cmd`.
pub fn step(s: &mut FlightState, cmd: &InputCmd, spec: &FrameSpec, mods: &FlightMods, dt: f32) -> FlightOut {
    let own = spec.mass(s.propellant);
    let mass = own + mods.extra_mass_kg as f32;
    // Extra mass slows turns too (limbs and thrusters swing more); a lighter suit gains nothing.
    let turn = (own / mass).min(1.0);
    let has_prop = s.propellant > 0.0;
    let authority = if s.blackout { 0.25 } else { 1.0 };

    // --- Attitude: chase the aim direction, plus commanded roll. ---
    let fwd = s.rot * Vec3::Z;
    let aim = normalize_or(cmd.aim, fwd);
    let axis = fwd.cross(aim);
    let sin_a = length(axis);
    let cos_a = fwd.dot(aim);
    let angle = atan2(sin_a, cos_a);
    let rcs = cmd.pressed(RCS_SHARP) && has_prop;
    let max_rate = if rcs { spec.rcs_rate } else { spec.ambac_rate } * authority;
    let ambac = spec.ambac_accel * mods.ambac * authority * turn;
    let rcs_accel = if rcs { spec.rcs_accel * authority * turn } else { 0.0 };
    let accel_cap = ambac + rcs_accel;
    // Toward the aim, but never faster than it can stop from in time: arms busy firing or with a
    // blade take AMBAC's limbs, not its top rate, and it mustn't swing past the aim for it.
    let rate =
        |angle: f32| (angle * AIM_GAIN).min(max_rate).min(sqrt(2.0 * ATTITUDE_BRAKE * accel_cap * angle));
    let mut w_des = if sin_a > 1e-6 {
        axis * (rate(angle) / sin_a)
    } else if cos_a < 0.0 {
        (s.rot * Vec3::Y) * rate(angle) // exactly behind: pitch over
    } else {
        Vec3::ZERO
    };
    w_des += fwd * (cmd.roll_f32() * spec.roll_rate * authority);
    let dw = w_des - s.ang_vel;
    let dw_len = length(dw);
    let max_dw = accel_cap * dt;
    let applied = if dw_len > max_dw && dw_len > 0.0 { dw * (max_dw / dw_len) } else { dw };
    s.ang_vel += applied;
    if rcs_accel > 0.0 {
        s.propellant -= spec.rcs_propellant * length(applied) * (rcs_accel / accel_cap);
    }
    s.rot = integrate_rotation(s.rot, s.ang_vel, dt);

    // --- Translation. ---
    let boosting = cmd.pressed(BOOST) && has_prop && !s.blackout;
    let main = spec.main_thrust * if boosting { spec.boost_mult } else { 1.0 };
    let side = spec.side_thrust;
    let retro = spec.retro_thrust;
    let brake = cmd.pressed(BRAKE);
    let assisted = brake || cmd.pressed(FLIGHT_ASSIST);
    let stick = if brake { Vec3::ZERO } else { cmd.thrust_vec() };
    let mut f_local = if assisted {
        // Boost's cruise while the pilot holds it, even through a blackout that cuts the boost
        // itself (flight assist mustn't brake them for it).
        let cruise = spec.fa_speed * if cmd.pressed(BOOST) { FA_BOOST_CRUISE } else { 1.0 };
        let v_local = s.rot.conjugate() * s.vel;
        let gap = stick * cruise - v_local;
        let response = if mods.g_immune || length(gap) < FA_SETTLE { dt } else { FA_RESPONSE.max(dt) };
        let f_req = gap * (mass / response);
        Vec3::new(
            clamp_axis(f_req.x, side, side),
            clamp_axis(f_req.y, side, side),
            clamp_axis(f_req.z, main, retro),
        )
    } else {
        Vec3::new(
            stick.x * side,
            stick.y * side,
            if stick.z >= 0.0 { stick.z * main } else { stick.z * retro },
        )
    };
    if mods.lunge {
        f_local.z = spec.main_thrust * LUNGE_THRUST;
    }
    f_local *= mods.thrust * authority;
    if !has_prop {
        f_local = Vec3::ZERO;
    }
    // Flight assist spares its pilot's body: short of boost (or a blade's lunge), it holds them
    // under what they bear for good, whatever their tank and the thrusters could do.
    let guard = assisted && !boosting && !mods.lunge && !mods.g_immune;
    let most = FA_G_CAP * G0 * mass;
    let pull = length(f_local);
    let g_limited = guard && pull > most;
    if g_limited {
        f_local *= most / pull;
    }
    let burn = (f_local.x.abs() + f_local.y.abs() + f_local.z.abs()) * dt / spec.exhaust_velocity();
    s.propellant = (s.propellant - burn).max(0.0);
    let axial = if f_local.z >= 0.0 { spec.main_thrust } else { spec.retro_thrust };
    let throttle = f_local / Vec3::new(side, side, axial).max(Vec3::ONE);
    let accel = (s.rot * f_local) / mass;
    s.vel += accel * dt;
    s.pos += s.vel * dt;

    // --- Pilot G. ---
    let g = length(accel) / G0;
    s.g_load = g;
    if mods.g_immune {
        s.g_strain = 0.0;
        s.blackout = false;
    } else {
        if g > HUMAN_G_TOLERANCE {
            s.g_strain += (g - HUMAN_G_TOLERANCE) / STRAIN_GAIN_DIV * dt;
        } else {
            s.g_strain -= STRAIN_RECOVERY * dt;
        }
        s.g_strain = s.g_strain.clamp(0.0, 1.2);
        if s.g_strain >= 1.0 {
            s.blackout = true;
        } else if s.g_strain < 0.5 {
            s.blackout = false;
        }
    }

    crate::world::constrain(s);
    FlightOut { boosting, accel, throttle, g_limited }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::frame;
    use bc_proto::FrameId;

    fn state() -> FlightState {
        FlightState { propellant: 2_400.0, pos: Vec3::new(0.0, 2_000.0, 0.0), ..FlightState::default() }
    }

    #[test]
    fn no_thrust_keeps_velocity() {
        let mut s = FlightState { vel: Vec3::new(10.0, -5.0, 120.0), ..state() };
        let cmd = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
        let spec = frame(FrameId::Leo);
        for _ in 0..90 {
            step(&mut s, &cmd, spec, &FlightMods::default(), crate::config::DT);
        }
        assert!((s.vel - Vec3::new(10.0, -5.0, 120.0)).length() < 1e-3, "{:?}", s.vel);
        assert_eq!(s.propellant, 2_400.0);
    }

    #[test]
    fn ambac_turns_without_propellant() {
        let mut s = state();
        let cmd = InputCmd { aim: Vec3::X, ..InputCmd::default() };
        for _ in 0..90 {
            step(&mut s, &cmd, frame(FrameId::Leo), &FlightMods::default(), crate::config::DT);
        }
        assert!((s.rot * Vec3::Z).dot(Vec3::X) > 0.99);
        assert_eq!(s.propellant, 2_400.0);
    }
}
