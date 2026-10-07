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
//! - **Anime rules** ([`BoostGauge`], `crate::tuning::FlightRules`): the tank is a boost gauge. Only
//!   boost burns it; flying, turning on RCS and a blade's lunge are free, and work on an empty
//!   tank; and it fills back up whenever boost is let go.

use bc_proto::InputCmd;
use bc_proto::buttons::{BOOST, BRAKE, BURST, FLIGHT_ASSIST, RCS_SHARP};
/// The burst step's state ([`burst_tick`]): the own snapshot carries it as it is.
pub use bc_proto::snapshot::OwnBurst as Burst;
use glam::{Quat, Vec3};

use crate::config::G0;
use crate::content::FrameSpec;
use crate::field::Field;
use crate::math::{atan2, clamp_to_cone, integrate_rotation, length, normalize_or, sqrt};

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
/// How far under a pilot's tolerance flight assist holds them, g.
pub const FA_G_MARGIN: f32 = 0.03;
/// Flight assist holds a healthy pilot just under what they bear for good, g, unless they boost.
pub const FA_G_CAP: f32 = HUMAN_G_TOLERANCE - FA_G_MARGIN;
/// A blade's lunge drives forward at this much of full main thrust (never boosted).
pub const LUNGE_THRUST: f32 = 1.5;
/// A lunge homes: it drives along the aim when the aim is within this much of the nose, rad
/// (15°), and along the edge of that cone when it's further off.
pub const LUNGE_CONE: f32 = 0.2618;
/// The burst step ([`burst_tick`]): it drives this many ticks (0.3 s)...
pub const BURST_TICKS: u8 = 9;
/// ...at this much, m/s² (about 12 g, which a pilot bears under the anime rules and feels under the
/// real ones), adding 36 m/s along the stick...
pub const BURST_ACCEL: f32 = 120.0;
/// ...and the next can start this many ticks after its press (1.2 s).
pub const BURST_COOLDOWN: u8 = 36;
/// A stick past this (of 127) counts toward a step's direction.
const BURST_STICK: i8 = 32;
/// How hard roll-level turns the feet toward a surface (1/s): the roll rate asked for per radian
/// of roll still to go, up to the frame's roll rate.
pub const ROLL_LEVEL_GAIN: f32 = 2.0;

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
    /// The burst step under way, and the wait for the next.
    pub burst: Burst,
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
            burst: Burst::default(),
        }
    }
}

/// The burst step: a press of BURST (a double-tapped direction) with the stick off centre starts a
/// step along it, if the last has cooled down and the suit `can` (propellant in the tank, the pilot
/// awake, boosters that work). It drives for [`BURST_TICKS`] at [`BURST_ACCEL`], burning as boost
/// does, in the suit's own axes or, locked on, in the fight's ([`LockOnAssist`]); flight assist lets
/// it be until it's done, then brings the suit back to what the stick asks. One tick of it: whether
/// it drives this tick.
pub fn burst_tick(b: &mut Burst, cmd: &InputCmd, can: bool) -> bool {
    let held = cmd.pressed(BURST);
    let pressed = held && !b.held;
    b.held = held;
    b.cooldown = b.cooldown.saturating_sub(1);
    let dir = cmd.thrust.map(|t| {
        if t > BURST_STICK {
            1
        } else if t < -BURST_STICK {
            -1
        } else {
            0
        }
    });
    if pressed && b.cooldown == 0 && can && dir != [0; 3] {
        *b = Burst { left: BURST_TICKS, cooldown: BURST_COOLDOWN, dir, held };
    }
    if b.left > 0 {
        b.left -= 1;
        true
    } else {
        false
    }
}

/// Modifiers from damage, equipment and what the arms are doing (see `crate::tuning`, which builds
/// them the same way on the server and in the owner's prediction).
#[derive(Clone, Copy, Debug)]
pub struct FlightMods {
    /// AMBAC authority 0..1 (lost limbs, busy arms).
    pub ambac: f32,
    /// Thrust authority on every axis, applied after flight assist asks for its force (a change of
    /// form cutting it).
    pub thrust: f32,
    /// What each axis' thrusters can give, of the frame's own: main (forward), side (lateral and
    /// vertical) and retro. Flight assist only asks for what they can give.
    pub main: f32,
    pub side: f32,
    pub retro: f32,
    /// How much of boost's extra thrust the boosters give (0: they can't boost at all).
    pub boost: f32,
    /// Specific impulse, of the frame's own.
    pub isp: f32,
    /// Sustained G the pilot bears before strain builds.
    pub g_tolerance: f32,
    /// Propellant lost from a holed tank, kg/s.
    pub leak_kg_s: f32,
    /// Mobile Dolls: no G-strain.
    pub g_immune: bool,
    /// Beam saber lunge: full forward thrust at [`LUNGE_THRUST`]×.
    pub lunge: bool,
    /// How far off the nose a lunge drives toward the aim, rad ([`LUNGE_CONE`]); 0: straight
    /// along the nose.
    pub lunge_cone: f32,
    /// Mass beyond the frame's own (cargo, a chunk in hand) less the parts shot off, kg. Whole
    /// kilograms, so prediction uses exactly the server's number.
    pub extra_mass_kg: i32,
    /// Rolls the suit's feet toward a surface, in place of the pilot's roll: its up turns about the
    /// nose toward this direction (the surface's outward normal, in the frame flown), at up to the
    /// roll rate. The nose stays on the aim.
    pub roll_level: Option<Vec3>,
    /// Flight assist aloft over a surface that grips the suit ([`HopAssist`]).
    pub hop: Option<HopAssist>,
    /// Flight assist locked on to a target ([`LockOnAssist`]).
    pub lockon: Option<LockOnAssist>,
    /// Anime rules: the tank is a boost gauge ([`BoostGauge`]). None: every newton burns.
    pub gauge: Option<BoostGauge>,
    /// Inside the colony, in its own frame (`colony::interior`): the spin's pull, Coriolis and the
    /// air act on the suit, and flight assist holds against them.
    pub interior: bool,
    /// Staggered (`sim::stagger`): its attitude control does nothing, so it tumbles as the blow
    /// left it, and it can neither boost nor step (its thrust is cut in [`FlightMods::thrust`]).
    pub staggered: bool,
}

impl Default for FlightMods {
    fn default() -> Self {
        Self {
            ambac: 1.0,
            thrust: 1.0,
            main: 1.0,
            side: 1.0,
            retro: 1.0,
            boost: 1.0,
            isp: 1.0,
            g_tolerance: HUMAN_G_TOLERANCE,
            leak_kg_s: 0.0,
            g_immune: false,
            lunge: false,
            lunge_cone: 0.0,
            extra_mass_kg: 0,
            roll_level: None,
            hop: None,
            lockon: None,
            gauge: None,
            interior: false,
            staggered: false,
        }
    }
}

/// Anime rules: the tank is a boost gauge. Only boost burns propellant (all the thrust it gives,
/// as ever); everything else the thrusters do is free and works on an empty tank, and once boost
/// is let go the tank fills back up. Boost's cruise needs propellant to hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoostGauge {
    /// The tank's size, of the frame's own (an auxiliary tank's bigger).
    pub tank: f32,
    /// The share of the tank that comes back a second while boost isn't held.
    pub refill: f32,
}

/// Flight assist aloft over a surface that grips the suit: it holds the stick's speed along the
/// surface, but along the normal only what the pilot asks for (up or down on the stick). Otherwise
/// it leaves the normal speed alone, never holding altitude: grip gravity brings the suit down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HopAssist {
    /// The surface's outward normal, in the suit's own frame.
    pub up: Vec3,
    /// Speed along the surface at full stick, m/s.
    pub cruise: f32,
    /// Speed along the normal at full up or down stick, m/s.
    pub climb: f32,
}

/// Flight assist locked on to a target (`bc_proto::LockOn`, built by `crate::ground::lockon_assist`):
/// it holds the velocity the stick asks for relative to the target's, `ref_vel`, and reads the
/// stick in the fight's axes rather than the suit's own: `right`, `up` and `fwd` (the aim laid flat
/// on the fight's ground), all in the frame flown. A free suit only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LockOnAssist {
    pub ref_vel: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
}

/// The speed flight assist holds at full stick, m/s: the frame's cruise, or boost's while the pilot
/// holds it (even through a blackout that cuts the boost itself: flight assist mustn't brake them
/// for it). An empty boost gauge can't hold it, and boosters that can't boost don't.
pub fn fa_cruise(spec: &FrameSpec, mods: &FlightMods, cmd: &InputCmd, propellant: f32) -> f32 {
    let boost_cruise = cmd.pressed(BOOST) && mods.boost > 0.0 && (propellant > 0.0 || mods.gauge.is_none());
    spec.fa_speed * if boost_cruise { FA_BOOST_CRUISE } else { 1.0 }
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

/// Advances one suit by `dt` under `cmd`, kept inside the sector and out of the colony hull.
pub fn step(s: &mut FlightState, cmd: &InputCmd, spec: &FrameSpec, mods: &FlightMods, dt: f32) -> FlightOut {
    let out = integrate(s, cmd, spec, mods, dt);
    crate::world::constrain(s);
    out
}

/// Advances one suit by `dt` under `cmd` with nothing in its way: attitude, thrust, propellant and
/// pilot G. The frame is whatever `s` is in: the sector's, or a body's for a suit in its grip.
pub fn integrate(
    s: &mut FlightState,
    cmd: &InputCmd,
    spec: &FrameSpec,
    mods: &FlightMods,
    dt: f32,
) -> FlightOut {
    let own = spec.mass(s.propellant);
    let mass = own + mods.extra_mass_kg as f32;
    // Extra mass slows turns too (limbs and thrusters swing more); a lighter suit gains nothing.
    let turn = (own / mass).min(1.0);
    let has_prop = s.propellant > 0.0;
    // Under anime rules only boost needs propellant.
    let powered = has_prop || mods.gauge.is_some();
    let authority = if s.blackout { 0.25 } else { 1.0 };
    // Staggered, its attitude control does nothing: it tumbles as the blow left it.
    let attitude = if mods.staggered { 0.0 } else { authority };
    // Inside the colony: what the spin and the air do to it this tick.
    let ext = if mods.interior { Some(crate::colony::interior::accel(s.pos, s.vel, mass)) } else { None };

    // --- Attitude: chase the aim direction, plus commanded roll (or roll-level). ---
    let fwd = s.rot * Vec3::Z;
    let aim = normalize_or(cmd.aim, fwd);
    let axis = fwd.cross(aim);
    let sin_a = length(axis);
    let cos_a = fwd.dot(aim);
    let angle = atan2(sin_a, cos_a);
    let rcs = cmd.pressed(RCS_SHARP) && powered;
    let max_rate = if rcs { spec.rcs_rate } else { spec.ambac_rate } * attitude;
    let ambac = spec.ambac_accel * mods.ambac * attitude * turn;
    let rcs_accel = if rcs { spec.rcs_accel * attitude * turn } else { 0.0 };
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
    match mods.roll_level {
        None => w_des += fwd * (cmd.roll_f32() * spec.roll_rate * attitude),
        Some(n) => {
            // The roll from up to `n`, both seen along the nose (none if `n` is along the nose).
            let up_s = s.rot * Vec3::Y;
            let u_p = up_s - fwd * up_s.dot(fwd);
            let n_p = n - fwd * n.dot(fwd);
            let roll = if n_p.length_squared() > 0.01 {
                let ang = atan2(fwd.dot(u_p.cross(n_p)), u_p.dot(n_p));
                (ROLL_LEVEL_GAIN * ang).clamp(-spec.roll_rate * attitude, spec.roll_rate * attitude)
            } else {
                0.0
            };
            w_des += fwd * roll;
        }
    }
    let dw = w_des - s.ang_vel;
    let dw_len = length(dw);
    let max_dw = accel_cap * dt;
    let applied = if dw_len > max_dw && dw_len > 0.0 { dw * (max_dw / dw_len) } else { dw };
    s.ang_vel += applied;
    if rcs_accel > 0.0 && mods.gauge.is_none() {
        s.propellant -= spec.rcs_propellant * length(applied) * (rcs_accel / accel_cap);
    }
    s.rot = integrate_rotation(s.rot, s.ang_vel, dt);

    // --- Translation. ---
    // Boosters that can't boost don't: no boosted cruise, and flight assist keeps its G guard.
    let can_boost = mods.boost > 0.0;
    let boosting = cmd.pressed(BOOST) && has_prop && !s.blackout && can_boost && !mods.staggered;
    // Exactly the frame's multiplier when the boosters are whole (a lerp needn't round back to it).
    let boost_mult =
        if mods.boost >= 1.0 { spec.boost_mult } else { 1.0 + (spec.boost_mult - 1.0) * mods.boost };
    let main = spec.main_thrust * mods.main * if boosting { boost_mult } else { 1.0 };
    let side = spec.side_thrust * mods.side;
    let retro = spec.retro_thrust * mods.retro;
    let brake = cmd.pressed(BRAKE);
    // A burst step, along the stick at its press (in the fight's axes when locked on).
    let burst =
        burst_tick(&mut s.burst, cmd, has_prop && !s.blackout && can_boost && !mods.staggered).then(|| {
            let d =
                Vec3::new(f32::from(s.burst.dir[0]), f32::from(s.burst.dir[1]), f32::from(s.burst.dir[2]));
            match mods.lockon {
                Some(l) => {
                    normalize_or(s.rot.conjugate() * (l.right * d.x + l.up * d.y + l.fwd * d.z), Vec3::Z)
                }
                None => normalize_or(d, Vec3::Z),
            }
        });
    let assisted = (brake || cmd.pressed(FLIGHT_ASSIST)) && burst.is_none();
    let stick = if brake { Vec3::ZERO } else { cmd.thrust_vec() };
    let mut f_local = if assisted {
        let cruise = fa_cruise(spec, mods, cmd, s.propellant);
        let v_local = s.rot.conjugate() * s.vel;
        let gap = match (mods.hop, mods.lockon) {
            (Some(h), _) => {
                // Along the surface, the stick's speed; along the normal, the climb asked for, or
                // else the speed it has (so no force at all that way).
                let (s_n, v_n) = (stick.dot(h.up), v_local.dot(h.up));
                let s_t = stick - h.up * s_n;
                let t_n = if cmd.thrust[1] == 0 { v_n } else { s_n * h.climb };
                s_t * h.cruise + h.up * t_n - v_local
            }
            // Locked on: the stick's velocity in the fight's axes, on top of the target's.
            (None, Some(l)) => {
                let want = l.ref_vel + (l.right * stick.x + l.up * stick.y + l.fwd * stick.z) * cruise;
                s.rot.conjugate() * want - v_local
            }
            (None, None) => stick * cruise - v_local,
        };
        let response = if mods.g_immune || length(gap) < FA_SETTLE { dt } else { FA_RESPONSE.max(dt) };
        let mut f_req = gap * (mass / response);
        // Holding a velocity inside the colony means holding against its pull too.
        if let Some(e) = ext {
            f_req -= (s.rot.conjugate() * e) * mass;
        }
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
        let drive = spec.main_thrust * mods.main * LUNGE_THRUST;
        if mods.lunge_cone > 0.0 {
            // It homes: along the aim, within its cone of the nose, on top of the stick's sideways.
            let along = s.rot.conjugate() * clamp_to_cone(aim, s.rot * Vec3::Z, mods.lunge_cone);
            f_local = Vec3::new(f_local.x + along.x * drive, f_local.y + along.y * drive, along.z * drive);
        } else {
            f_local.z = drive;
        }
    }
    if let Some(d) = burst {
        f_local += d * (BURST_ACCEL * mass);
    }
    f_local *= mods.thrust * authority;
    if !powered {
        f_local = Vec3::ZERO;
    }
    // Flight assist spares its pilot's body: short of boost (or a blade's lunge), it holds them
    // under what they bear for good, whatever their tank and the thrusters could do.
    let guard = assisted && !boosting && !mods.lunge && !mods.g_immune;
    let most = (mods.g_tolerance - FA_G_MARGIN) * G0 * mass;
    let pull = length(f_local);
    let g_limited = guard && pull > most;
    if g_limited {
        f_local *= most / pull;
    }
    // Under anime rules only boost burns (and a burst step, which is one).
    if mods.gauge.is_none() || boosting || burst.is_some() {
        let burn =
            (f_local.x.abs() + f_local.y.abs() + f_local.z.abs()) * dt / (spec.exhaust_velocity() * mods.isp);
        s.propellant = (s.propellant - burn).max(0.0);
    }
    if mods.leak_kg_s > 0.0 {
        s.propellant = (s.propellant - mods.leak_kg_s * dt).max(0.0);
    }
    // It fills only once boost is let go (and a step is done), so a pilot leaning on an empty gauge
    // gets nothing.
    if !(cmd.pressed(BOOST) && can_boost) && burst.is_none() {
        refill(s, spec, mods, dt);
    }
    let axial = if f_local.z >= 0.0 { spec.main_thrust } else { spec.retro_thrust };
    // Against the healthy caps: damaged thrusters read as a lower throttle (the sound, the plumes).
    let throttle = f_local / Vec3::new(spec.side_thrust, spec.side_thrust, axial).max(Vec3::ONE);
    let accel = (s.rot * f_local) / mass;
    s.vel += accel * dt;
    // (Not felt as G: a free fall is weightless, whatever pulls it.)
    if let Some(e) = ext {
        s.vel += e * dt;
    }
    s.pos += s.vel * dt;

    // --- Pilot G. ---
    pilot_g(s, accel, mods, dt);
    // A step is seen as boost is (its plumes, its heat on sensors).
    FlightOut { boosting: boosting || burst.is_some(), accel, throttle, g_limited }
}

/// Anime rules ([`BoostGauge`]): `dt` of the tank filling back up, to its size. Nothing under the
/// real rules. Flying suits refill in [`integrate`] while boost isn't held; a suit on its feet
/// (whose legs burn nothing, and whose Shift runs) refills through this from the ground step.
pub fn refill(s: &mut FlightState, spec: &FrameSpec, mods: &FlightMods, dt: f32) {
    if let Some(g) = mods.gauge {
        let tank = spec.propellant_cap * g.tank;
        if s.propellant < tank {
            s.propellant = (s.propellant + tank * g.refill * dt).min(tank);
        }
    }
}

/// Pilot G: the suit felt `accel` (m/s²) for `dt`. Above what a pilot bears for good, strain
/// builds; below it, strain eases. At 1.0 they black out, until it falls under 0.5. A Mobile Doll
/// (`g_immune`) feels nothing. A pilot bears `mods.g_tolerance` for good.
pub fn pilot_g(s: &mut FlightState, accel: Vec3, mods: &FlightMods, dt: f32) {
    let g = length(accel) / G0;
    s.g_load = g;
    if mods.g_immune {
        s.g_strain = 0.0;
        s.blackout = false;
    } else {
        if g > mods.g_tolerance {
            s.g_strain += (g - mods.g_tolerance) / STRAIN_GAIN_DIV * dt;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DT, SECTOR_LIMIT};
    use crate::content::frame;
    use crate::math::{Rng, cos, quat_normalize, sin};
    use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
    use bc_proto::FrameId;

    fn state() -> FlightState {
        FlightState { propellant: 2_400.0, pos: Vec3::new(0.0, 2_000.0, 0.0), ..FlightState::default() }
    }

    /// A lunge homes: with the aim off the nose it drives along the aim, and along the edge of its
    /// cone once the aim's further off than that; with no cone it drives along the nose.
    #[test]
    fn a_lunge_drives_along_the_aim_within_its_cone() {
        let spec = frame(FrameId::Leo);
        let lunge = |deg: f32, cone: f32| {
            let mut s = state();
            let a = deg.to_radians();
            let cmd = InputCmd { aim: Vec3::new(sin(a), 0.0, cos(a)), ..InputCmd::default() };
            let mods = FlightMods { lunge: true, lunge_cone: cone, ..FlightMods::default() };
            let out = integrate(&mut s, &cmd, spec, &mods, DT);
            (out.accel.normalize(), s.rot * Vec3::Z, cmd.aim)
        };
        let (drive, _, aim) = lunge(10.0, LUNGE_CONE);
        assert!(drive.angle_between(aim) < 0.002, "off the aim by {}", drive.angle_between(aim));
        let (drive, nose, aim) = lunge(40.0, LUNGE_CONE);
        assert!((drive.angle_between(nose) - LUNGE_CONE).abs() < 0.002, "{}", drive.angle_between(nose));
        assert!(drive.angle_between(aim) < aim.angle_between(nose), "toward the aim");
        let (drive, nose, _) = lunge(10.0, 0.0);
        assert!(drive.angle_between(nose) < 0.002, "{}", drive.angle_between(nose));
    }

    /// The burst step: on BURST's press, 36 m/s along the stick in 0.3 s, burning the gauge even
    /// under the anime rules; none on a held button, a centred stick or before it's cooled down;
    /// locked on, along the fight's axes; and flight assist brings the suit back after it.
    #[test]
    fn a_burst_step_dashes_along_the_stick_once_per_cooldown() {
        let spec = frame(FrameId::Leo);
        let anime = FlightMods {
            gauge: Some(BoostGauge { tank: 1.0, refill: 0.1 }),
            g_tolerance: HUMAN_G_TOLERANCE * 2.0,
            ..FlightMods::default()
        };
        let fly = |s: &mut FlightState, buttons: u16, thrust: [i8; 3], mods: &FlightMods| {
            let cmd =
                InputCmd { aim: Vec3::Z, thrust, buttons: FLIGHT_ASSIST | buttons, ..InputCmd::default() };
            integrate(s, &cmd, spec, mods, DT)
        };
        // D double-tapped: BURST pressed with the stick right, then everything let go.
        let mut s = state();
        let out = fly(&mut s, BURST, [127, 0, 0], &anime);
        assert!(out.boosting, "a step is seen as boost is");
        for _ in 1..BURST_TICKS {
            fly(&mut s, 0, [0; 3], &anime);
        }
        // (The press's tick also has the held key's own thrust.)
        assert!((s.vel - Vec3::X * 36.0).length() < 1.0, "{}", s.vel);
        assert!((30.0..50.0).contains(&(2_400.0 - s.propellant)), "burnt {}", 2_400.0 - s.propellant);
        assert!(s.g_strain < 0.05 && !s.blackout, "strain {}", s.g_strain);
        // Pressed again too soon, or held, or with the stick centred: nothing.
        fly(&mut s, BURST, [127, 0, 0], &anime);
        fly(&mut s, BURST, [127, 0, 0], &anime);
        assert!(s.burst.left == 0 && s.burst.cooldown < BURST_COOLDOWN - BURST_TICKS, "{:?}", s.burst);
        // Flight assist brings it back (on the side thrusters: about 2 s).
        for _ in 0..90 {
            fly(&mut s, 0, [0; 3], &anime);
        }
        assert!(s.vel.length() < 0.1, "flight assist brought it back: {}", s.vel);
        fly(&mut s, BURST, [0; 3], &anime);
        assert_eq!((s.burst.left, s.burst.cooldown), (0, 0), "a centred stick doesn't step");
        // Locked on, S steps back from the target, along the fight's axes rather than the nose.
        let up = Vec3::Y;
        let fwd = Vec3::new(1.0, 0.0, 1.0).normalize();
        let lock = LockOnAssist { ref_vel: Vec3::ZERO, right: up.cross(fwd), up, fwd };
        let locked = FlightMods { lockon: Some(lock), ..anime };
        let mut s = state();
        fly(&mut s, BURST, [0, 0, -127], &locked);
        for _ in 1..BURST_TICKS {
            fly(&mut s, 0, [0; 3], &locked);
        }
        assert!((s.vel + fwd * 36.0).length() < 1.0 && s.vel.y.abs() < 0.01, "{}", s.vel);
        // Under the real rules the pilot feels it.
        let mut s = state();
        for k in 0..BURST_TICKS {
            let press = if k == 0 { BURST } else { 0 };
            fly(&mut s, press, [0, 127, 0], &FlightMods::default());
        }
        assert!(s.g_strain > 0.3 && !s.blackout, "strain {}", s.g_strain);
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

    /// The flight model as it stood in protocol v8, verbatim: what the refactor into [`integrate`],
    /// [`pilot_g`] and `world::constrain` is held to, bit for bit.
    mod v8 {
        use super::super::*;

        pub fn step(
            s: &mut FlightState,
            cmd: &InputCmd,
            spec: &FrameSpec,
            mods: &FlightMods,
            dt: f32,
        ) -> FlightOut {
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
            let rate = |angle: f32| {
                (angle * AIM_GAIN).min(max_rate).min(sqrt(2.0 * ATTITUDE_BRAKE * accel_cap * angle))
            };
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
            // Boosters that can't boost don't: no boosted cruise, and flight assist keeps its G guard.
            let can_boost = mods.boost > 0.0;
            let boosting = cmd.pressed(BOOST) && has_prop && !s.blackout && can_boost;
            // Exactly the frame's multiplier when the boosters are whole (a lerp needn't round back to it).
            let boost_mult =
                if mods.boost >= 1.0 { spec.boost_mult } else { 1.0 + (spec.boost_mult - 1.0) * mods.boost };
            let main = spec.main_thrust * mods.main * if boosting { boost_mult } else { 1.0 };
            let side = spec.side_thrust * mods.side;
            let retro = spec.retro_thrust * mods.retro;
            let brake = cmd.pressed(BRAKE);
            let assisted = brake || cmd.pressed(FLIGHT_ASSIST);
            let stick = if brake { Vec3::ZERO } else { cmd.thrust_vec() };
            let mut f_local = if assisted {
                // Boost's cruise while the pilot holds it, even through a blackout that cuts the boost
                // itself (flight assist mustn't brake them for it).
                let cruise =
                    spec.fa_speed * if cmd.pressed(BOOST) && can_boost { FA_BOOST_CRUISE } else { 1.0 };
                let v_local = s.rot.conjugate() * s.vel;
                let gap = stick * cruise - v_local;
                let response =
                    if mods.g_immune || length(gap) < FA_SETTLE { dt } else { FA_RESPONSE.max(dt) };
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
                f_local.z = spec.main_thrust * mods.main * LUNGE_THRUST;
            }
            f_local *= mods.thrust * authority;
            if !has_prop {
                f_local = Vec3::ZERO;
            }
            // Flight assist spares its pilot's body: short of boost (or a blade's lunge), it holds them
            // under what they bear for good, whatever their tank and the thrusters could do.
            let guard = assisted && !boosting && !mods.lunge && !mods.g_immune;
            let most = (mods.g_tolerance - FA_G_MARGIN) * G0 * mass;
            let pull = length(f_local);
            let g_limited = guard && pull > most;
            if g_limited {
                f_local *= most / pull;
            }
            let burn = (f_local.x.abs() + f_local.y.abs() + f_local.z.abs()) * dt
                / (spec.exhaust_velocity() * mods.isp);
            s.propellant = (s.propellant - burn).max(0.0);
            if mods.leak_kg_s > 0.0 {
                s.propellant = (s.propellant - mods.leak_kg_s * dt).max(0.0);
            }
            let axial = if f_local.z >= 0.0 { spec.main_thrust } else { spec.retro_thrust };
            // Against the healthy caps: damaged thrusters read as a lower throttle (the sound, the plumes).
            let throttle = f_local / Vec3::new(spec.side_thrust, spec.side_thrust, axial).max(Vec3::ONE);
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
                if g > mods.g_tolerance {
                    s.g_strain += (g - mods.g_tolerance) / STRAIN_GAIN_DIV * dt;
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

        /// The pilot-G block of v8's `step`, verbatim.
        pub fn pilot_g(s: &mut FlightState, accel: Vec3, mods: &FlightMods, dt: f32) {
            let g = length(accel) / G0;
            s.g_load = g;
            if mods.g_immune {
                s.g_strain = 0.0;
                s.blackout = false;
            } else {
                if g > mods.g_tolerance {
                    s.g_strain += (g - mods.g_tolerance) / STRAIN_GAIN_DIV * dt;
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
        }
    }

    /// Every bit of a flight state (`==` would take -0.0 for 0.0, and never NaN for NaN).
    fn bits(s: &FlightState) -> [u32; 17] {
        [
            s.pos.x.to_bits(),
            s.pos.y.to_bits(),
            s.pos.z.to_bits(),
            s.vel.x.to_bits(),
            s.vel.y.to_bits(),
            s.vel.z.to_bits(),
            s.rot.x.to_bits(),
            s.rot.y.to_bits(),
            s.rot.z.to_bits(),
            s.rot.w.to_bits(),
            s.ang_vel.x.to_bits(),
            s.ang_vel.y.to_bits(),
            s.ang_vel.z.to_bits(),
            s.propellant.to_bits(),
            s.g_load.to_bits(),
            s.g_strain.to_bits(),
            u32::from(s.blackout),
        ]
    }

    fn out_bits(o: &FlightOut) -> [u32; 8] {
        [
            u32::from(o.boosting),
            o.accel.x.to_bits(),
            o.accel.y.to_bits(),
            o.accel.z.to_bits(),
            o.throttle.x.to_bits(),
            o.throttle.y.to_bits(),
            o.throttle.z.to_bits(),
            u32::from(o.g_limited),
        ]
    }

    fn random_vec(rng: &mut Rng, scale: f32) -> Vec3 {
        Vec3::new(rng.signed(), rng.signed(), rng.signed()) * scale
    }

    /// A random suit, command, frame and modifiers. A third of the suits are out in the sector, a
    /// third skim the colony's hull and a third its edge, so `constrain` has work to do; tanks run
    /// dry, pilots black out, and a quarter of the stick axes are idle.
    fn random_case(rng: &mut Rng) -> (FlightState, InputCmd, &'static FrameSpec, FlightMods) {
        let pos = match rng.next_u32() % 3 {
            0 => random_vec(rng, SECTOR_LIMIT),
            1 => {
                let a = rng.signed() * core::f32::consts::PI;
                let r = COLONY_RADIUS + rng.signed() * 20.0;
                let x = rng.signed() * (COLONY_HALF_LENGTH + 20.0);
                COLONY_CENTER + Vec3::new(x, r * cos(a), r * sin(a))
            }
            _ => {
                let mut p = random_vec(rng, SECTOR_LIMIT);
                let side = if rng.next_u32().is_multiple_of(2) { 1.0 } else { -1.0 };
                p[(rng.next_u32() % 3) as usize] = (SECTOR_LIMIT + rng.signed() * 20.0) * side;
                p
            }
        };
        let rot = quat_normalize(Quat::from_xyzw(rng.signed(), rng.signed(), rng.signed(), rng.signed()));
        let s = FlightState {
            pos,
            vel: random_vec(rng, 300.0),
            rot,
            ang_vel: random_vec(rng, 2.0),
            propellant: if rng.next_u32().is_multiple_of(5) { 0.0 } else { rng.next_f32() * 3_000.0 },
            g_load: rng.next_f32() * 10.0,
            g_strain: rng.next_f32() * 1.2,
            blackout: rng.next_u32().is_multiple_of(4),
            // No step under way (v8 had none).
            burst: Burst::default(),
        };
        let axis = |rng: &mut Rng| if rng.next_u32().is_multiple_of(4) { 0 } else { rng.next_u32() as i8 };
        let cmd = InputCmd {
            aim: if rng.next_u32().is_multiple_of(10) { Vec3::ZERO } else { random_vec(rng, 1.0) },
            thrust: [axis(rng), axis(rng), axis(rng)],
            roll: axis(rng),
            // Bit 15 was free in v8 (the burst step came later; its own tests hold it).
            buttons: rng.next_u32() as u16 & !BURST,
            ..InputCmd::default()
        };
        let spec = frame(FrameId::ALL[rng.next_u32() as usize % FrameId::COUNT]);
        let mods = FlightMods {
            ambac: rng.next_f32(),
            thrust: rng.next_f32(),
            g_immune: rng.next_u32().is_multiple_of(2),
            lunge: rng.next_u32().is_multiple_of(8),
            extra_mass_kg: (rng.next_u32() % 30_000) as i32 - 5_000,
            // Wear and tear's per-axis modifiers, so the old expressions are held to with them too.
            main: 0.5 + rng.next_f32() * 0.5,
            side: 0.5 + rng.next_f32() * 0.5,
            retro: 0.5 + rng.next_f32() * 0.5,
            boost: rng.next_f32(),
            isp: 0.7 + rng.next_f32() * 0.3,
            g_tolerance: 4.0 + rng.next_f32() * 2.0,
            leak_kg_s: if rng.next_u32().is_multiple_of(4) { rng.next_f32() * 3.0 } else { 0.0 },
            ..FlightMods::default()
        };
        (s, cmd, spec, mods)
    }

    #[test]
    fn step_is_integrate_then_constrain() {
        let mut rng = Rng::new(0xF11E);
        let mut constrained = 0;
        for n in 0..10_000 {
            let (s0, cmd, spec, mods) = random_case(&mut rng);
            let (mut a, mut b) = (s0, s0);
            let out_a = step(&mut a, &cmd, spec, &mods, DT);
            let out_b = integrate(&mut b, &cmd, spec, &mods, DT);
            constrained += u32::from(crate::world::constrain(&mut b));
            assert_eq!(bits(&a), bits(&b), "case {n}: {s0:?} {cmd:?}");
            assert_eq!(out_bits(&out_a), out_bits(&out_b), "case {n}");
            // Pilot G on its own, against the block it was lifted from.
            let accel = random_vec(&mut rng, 150.0);
            let gm = FlightMods { g_immune: rng.next_u32().is_multiple_of(2), ..mods };
            let (mut p, mut q) = (s0, s0);
            pilot_g(&mut p, accel, &gm, DT);
            v8::pilot_g(&mut q, accel, &gm, DT);
            assert_eq!(bits(&p), bits(&q), "case {n}: {s0:?} felt {accel}");
        }
        assert!(constrained > 3_000, "only {constrained} of the cases met the hull or the edge");
    }

    #[test]
    fn none_mods_are_the_old_expressions() {
        let mut rng = Rng::new(0x0008);
        let (mut rolled, mut hopped, mut locked) = (0, 0, 0);
        for n in 0..10_000 {
            let (s0, cmd, spec, mods) = random_case(&mut rng);
            assert!(mods.roll_level.is_none() && mods.hop.is_none() && mods.lockon.is_none());
            let (mut a, mut b) = (s0, s0);
            let out_a = step(&mut a, &cmd, spec, &mods, DT);
            let out_b = v8::step(&mut b, &cmd, spec, &mods, DT);
            assert_eq!(bits(&a), bits(&b), "case {n}: {s0:?} {cmd:?}");
            assert_eq!(out_bits(&out_a), out_bits(&out_b), "case {n}");
            // Set, they are wired in: the same case flies otherwise.
            let up = random_vec(&mut rng, 1.0).normalize_or(Vec3::Y);
            let (mut c, mut d) = (s0, s0);
            step(&mut c, &cmd, spec, &FlightMods { roll_level: Some(up), ..mods }, DT);
            let hop = HopAssist { up, cruise: 8.0, climb: 20.0 };
            step(&mut d, &cmd, spec, &FlightMods { hop: Some(hop), ..mods }, DT);
            let fwd = (s0.rot * Vec3::Z - up * (s0.rot * Vec3::Z).dot(up))
                .normalize_or(up.any_orthonormal_vector());
            let lock = LockOnAssist { ref_vel: random_vec(&mut rng, 400.0), right: up.cross(fwd), up, fwd };
            let mut e = s0;
            step(&mut e, &cmd, spec, &FlightMods { lockon: Some(lock), ..mods }, DT);
            rolled += u32::from(bits(&c) != bits(&a));
            hopped += u32::from(bits(&d) != bits(&a));
            locked += u32::from(bits(&e) != bits(&a));
        }
        assert!(rolled > 5_000 && hopped > 1_000, "roll-level changed {rolled}, hop assist {hopped}");
        assert!(locked > 1_000, "a lock-on changed {locked}");
    }

    /// The suit's own axes and nothing to hold relative to: locked on is plain flight assist.
    #[test]
    fn a_lockon_with_no_reference_in_the_suits_own_axes_is_plain_flight_assist() {
        let mut rng = Rng::new(0x10C0);
        let own = LockOnAssist { ref_vel: Vec3::ZERO, right: Vec3::X, up: Vec3::Y, fwd: Vec3::Z };
        for n in 0..2_000 {
            let (s0, cmd, spec, mods) = random_case(&mut rng);
            let s0 = FlightState { rot: Quat::IDENTITY, ang_vel: Vec3::ZERO, ..s0 };
            let cmd = InputCmd { aim: Vec3::Z, roll: 0, ..cmd };
            let (mut a, mut b) = (s0, s0);
            let out_a = step(&mut a, &cmd, spec, &mods, DT);
            let out_b = step(&mut b, &cmd, spec, &FlightMods { lockon: Some(own), ..mods }, DT);
            assert_eq!(bits(&a), bits(&b), "case {n}: {s0:?} {cmd:?}");
            assert_eq!(out_bits(&out_a), out_bits(&out_b), "case {n}");
        }
    }

    /// A target faster than flight assist's cruise: plain flight assist can't keep up with it, locked
    /// on it holds station on it, and the stick's speed comes on top. Brake brakes to the target.
    #[test]
    fn locked_on_flight_assist_keeps_pace_with_its_target() {
        let spec = frame(FrameId::Leo);
        // Mostly ahead, where the main thrusters push; some across, where the side ones do.
        let target = Vec3::new(50.0, -60.0, 380.0);
        assert!(target.length() > spec.fa_speed);
        let lock = LockOnAssist { ref_vel: target, right: Vec3::X, up: Vec3::Y, fwd: Vec3::Z };
        let mods = FlightMods { lockon: Some(lock), ..FlightMods::default() };
        let fly = |thrust: [i8; 3], buttons: u16, from: Vec3| {
            let mut s = FlightState { vel: from, ..state() };
            let cmd = InputCmd { aim: Vec3::Z, thrust, buttons, ..InputCmd::default() };
            for _ in 0..1_200 {
                integrate(&mut s, &cmd, spec, &mods, DT);
            }
            s
        };
        let s = fly([0; 3], FLIGHT_ASSIST, Vec3::ZERO);
        assert!((s.vel - target).length() < 0.01, "{}", s.vel);
        // Flight assist spared the pilot all the way.
        assert!(s.g_strain == 0.0 && !s.blackout);
        let s = fly([0, 0, 127], FLIGHT_ASSIST, Vec3::ZERO);
        assert!((s.vel - target - Vec3::Z * spec.fa_speed).length() < 0.01, "{}", s.vel);
        // Brake (with the stick held) brakes to the target's velocity, not the sector's rest.
        let s = fly([127, 0, 127], BRAKE, Vec3::new(-50.0, 0.0, 100.0));
        assert!((s.vel - target).length() < 0.01, "{}", s.vel);
    }

    #[test]
    fn roll_level_rolls_the_feet_down_and_keeps_the_nose() {
        let spec = frame(FrameId::Leo);
        // Lying on its side (up along +X) over a surface below it (its normal +Y), rolling hard
        // the other way: roll-level flies instead of the roll command.
        let mut s = FlightState { rot: crate::math::look_rotation(Vec3::Z, Vec3::X), ..state() };
        let cmd = InputCmd { aim: Vec3::Z, roll: 127, ..InputCmd::default() };
        let mods = FlightMods { roll_level: Some(Vec3::Y), ..FlightMods::default() };
        for _ in 0..90 {
            step(&mut s, &cmd, spec, &mods, DT);
        }
        let (up, nose) = (s.rot * Vec3::Y, s.rot * Vec3::Z);
        assert!(up.dot(Vec3::Y) > 0.999, "up {up}");
        assert!(nose.dot(Vec3::Z) > 0.999_99, "the nose left the aim: {nose}");
    }

    #[test]
    fn hop_assist_never_holds_altitude_and_climbs_when_asked() {
        let spec = frame(FrameId::Leo);
        let hop = HopAssist { up: Vec3::Y, cruise: 8.0, climb: 20.0 };
        let fly = |thrust: [i8; 3], hop: Option<HopAssist>| {
            let mut s = FlightState { vel: Vec3::new(0.0, -5.0, 0.0), ..state() };
            let cmd = InputCmd { aim: Vec3::Z, thrust, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
            for _ in 0..90 {
                step(&mut s, &cmd, spec, &FlightMods { hop, ..FlightMods::default() }, DT);
            }
            s.vel
        };
        // Stick forward: it runs at the cruise speed, and falls on as it was falling.
        let v = fly([0, 0, 127], Some(hop));
        assert!((v.z - 8.0).abs() < 0.05, "{v}");
        assert_eq!(v.y, -5.0, "flight assist pushed along the normal");
        // Plain flight assist would have stopped the fall.
        assert!(fly([0, 0, 127], None).y.abs() < 0.05);
        // Up on the stick, it climbs at the climb speed.
        let v = fly([0, 127, 0], Some(hop));
        assert!((v.y - 20.0).abs() < 0.05 && v.x.abs() < 1e-3 && v.z.abs() < 1e-3, "{v}");
    }
}
