//! The flight model, shared bit-for-bit by the server and the client's own-suit prediction.
//!
//! - **Newtonian 6DOF**, no drag. Thrust is limited per axis (main, side, retro) and burns
//!   propellant at `|F| / (Isp·g0)`, so mass drops and delta-v is finite.
//! - **AMBAC** turns the suit by swinging its limbs: free but modest. Damaged limbs and busy arms
//!   reduce it. **RCS** (held) adds much more authority at a propellant cost.
//! - **Flight assist** turns the thrust stick into a velocity command (it brakes to a stop when
//!   released); off, the stick is raw thrust and velocity persists. **Brake** always retro-burns.
//! - **Pilot G**: sustained load above tolerance builds G-strain. At 1.0 the pilot blacks out and
//!   control authority collapses until it recovers below 0.5. Mobile Dolls have no body to protect.

use bc_proto::InputCmd;
use bc_proto::buttons::{BOOST, BRAKE, FLIGHT_ASSIST, RCS_SHARP};
use glam::{Quat, Vec3};

use crate::config::G0;
use crate::content::FrameSpec;
use crate::field::Field;
use crate::math::{atan2, integrate_rotation, length, normalize_or};

/// Sustained G a trained human pilot tolerates before strain builds.
pub const HUMAN_G_TOLERANCE: f32 = 6.0;
/// Strain per second per G above tolerance (divided by this).
const STRAIN_GAIN_DIV: f32 = 4.0;
const STRAIN_RECOVERY: f32 = 0.35;
/// How hard the attitude controller chases the aim (1/s).
const AIM_GAIN: f32 = 3.0;

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
    /// Beam saber lunge: full forward thrust at 1.5×.
    pub lunge: bool,
}

impl Default for FlightMods {
    fn default() -> Self {
        Self { ambac: 1.0, thrust: 1.0, g_immune: false, lunge: false }
    }
}

/// What the step did (for visuals and signatures).
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightOut {
    pub boosting: bool,
    /// World-frame acceleration applied, m/s².
    pub accel: Vec3,
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
    let mass = spec.mass(s.propellant);
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
    let mut w_des = if sin_a > 1e-6 {
        axis * ((angle * AIM_GAIN).min(max_rate) / sin_a)
    } else if cos_a < 0.0 {
        (s.rot * Vec3::Y) * max_rate // exactly behind: pitch over
    } else {
        Vec3::ZERO
    };
    w_des += fwd * (cmd.roll_f32() * spec.roll_rate * authority);
    let ambac = spec.ambac_accel * mods.ambac * authority;
    let rcs_accel = if rcs { spec.rcs_accel * authority } else { 0.0 };
    let accel_cap = ambac + rcs_accel;
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
    let main =
        spec.main_thrust * if boosting { spec.boost_mult } else { 1.0 } * if mods.lunge { 1.5 } else { 1.0 };
    let side = spec.side_thrust;
    let retro = spec.retro_thrust;
    let brake = cmd.pressed(BRAKE);
    let assisted = brake || cmd.pressed(FLIGHT_ASSIST);
    let stick = if brake { Vec3::ZERO } else { cmd.thrust_vec() };
    let mut f_local = if assisted {
        let cruise = spec.fa_speed * if boosting { 1.8 } else { 1.0 };
        let v_local = s.rot.conjugate() * s.vel;
        let f_req = (stick * cruise - v_local) * (mass / dt);
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
        f_local.z = main;
    }
    f_local *= mods.thrust * authority;
    if !has_prop {
        f_local = Vec3::ZERO;
    }
    let burn = (f_local.x.abs() + f_local.y.abs() + f_local.z.abs()) * dt / spec.exhaust_velocity();
    s.propellant = (s.propellant - burn).max(0.0);
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
    FlightOut { boosting, accel }
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
