//! Shooter controls (the default, `settings::FlightControls::Shooter`): flying free, the suit
//! handles as a first-person shooter's player moves, not as a spacecraft does.
//!
//! - **An upright view.** The view's up settles onto the ground's ([`ground`]): away from the
//!   colony out in space (`bc_sim::world::colony_up`, the lock-on's ground too), toward its axis
//!   inside it, and onto the surface an armed grip is bringing the suit down to. The mouse turns
//!   the aim about that up, as on foot, and never past [`PITCH_LIMIT`] of the horizon ([`turn`]).
//!   Q / E roll the view off it while they're held; let go, and it levels out ([`level`]).
//! - **Moving where you look** ([`shape`]). W flies along the crosshair, A / D strafe square to it
//!   along the ground, Space / C rise and sink along the up, at the frame's cruise (a diagonal no
//!   faster). It travels as a lock-on with nothing to hold to (`bc_proto::LockOn`, its velocity
//!   zero and its up the view's): flight assist reads the stick in those axes and rolls the suit
//!   level with them, so the server and the owner's prediction fly exactly the same command.
//! - **Turning at once.** The suit fires its RCS whenever the aim is off its nose, as drones and
//!   agents always do: free under the anime rules, a few kilograms a hard turn under the real ones.
//!   The view turns with the mouse at once; the suit's arms reach 50° off its nose meanwhile.
//!
//! It stands aside where the keys mean something else ([`flies`]): on a body, flight assist off,
//! the Orrery flying the suit, and Aquila (a fighter's controls), and falling inside the colony with
//! the grip armed (the hover's keys are its own). The view's up then follows the suit's own. The
//! auto-nav flies its own command; locked on, the lock-on's shaping moves the suit about its target
//! (`crate::lockon::shape`) and this only turns it.

use bc_proto::buttons::{BURST, FLIGHT_ASSIST, GRIP, RCS_SHARP};
use bc_proto::snapshot::orrery_mode;
use bc_proto::{FrameId, InputCmd, LockOn};
use bc_sim::colony::frame::up_at;
use bc_sim::content::frame;
use bc_sim::flight::ROLL_LEVEL_GAIN;
use bc_sim::ground::{Footing, lockon_assist};
use bc_sim::world::colony_up;
use glam::{Quat, Vec3};

use crate::InputContext;
use crate::predict::Predictor;
use crate::surface::surface_hint;
use crate::world::World;

/// The furthest the aim pitches off the horizon, rad (89°, a shooter's): straight up or down, the
/// mouse's turn would spin the view about the aim. A foe straight overhead is a degree off it.
pub const PITCH_LIMIT: f32 = 1.5533;
/// The suit turns on its RCS while the aim is more than this off its nose, rad (2°), or it's
/// turning faster than [`RCS_SPIN`], rad/s (still coming round, or stopping).
pub const RCS_ANGLE: f32 = 0.035;
pub const RCS_SPIN: f32 = 0.3;
/// Levelling out, the view turns at least this fast, rad/s, so it finishes rather than creeping.
const LEVEL_LEAST: f32 = 0.05;
/// Following the suit's own up (where it doesn't fly upright), the view turns this many times
/// faster per radian still to go, and at least [`FOLLOW_LEAST`] rad/s: a tenth of a second behind.
const FOLLOW_GAIN: f32 = 10.0;
const FOLLOW_LEAST: f32 = 1.0;

/// Whether the shooter controls fly the suit now: it's alive and free, on flight assist (in
/// `buttons`), in a form with a pilot's controls, and not in the Orrery's hands.
pub fn flies(p: &Predictor, world: &World, buttons: u16) -> bool {
    let Some(own) = world.own.filter(|o| o.alive) else { return false };
    own.orrery_mode != orrery_mode::SEIZED
        && p.mover().footing == Footing::Free
        && buttons & FLIGHT_ASSIST != 0
        && p.form.frame != FrameId::Aquila
}

/// The ground's up where the predicted suit is, at tick `tick`: inside the colony, toward its axis;
/// out in space with the grip armed (`grip`), the surface it's coming down to; else away from the
/// colony.
pub fn ground(p: &Predictor, tick: u32, grip: bool) -> Vec3 {
    let s = &p.state;
    if p.interior() {
        return up_at(s.pos);
    }
    if grip && let Some(h) = surface_hint(&p.bodies(tick), s) {
        return h.normal;
    }
    colony_up(s.pos)
}

/// `up` turned toward `to` by at most `step`, rad.
fn toward(up: Vec3, to: Vec3, step: f32) -> Vec3 {
    let angle = up.angle_between(to);
    if angle <= step || angle.is_nan() {
        return to;
    }
    let axis = up.cross(to).try_normalize().unwrap_or_else(|| up.any_orthonormal_vector());
    (Quat::from_axis_angle(axis, step) * up).normalize()
}

/// The view's up levelling out onto `to` for `dt` s, as the suit rolls level (`ROLL_LEVEL_GAIN` per
/// radian still to go, up to the frame's roll rate `rate`), so the view and the suit come level
/// together.
pub fn level(up: Vec3, to: Vec3, rate: f32, dt: f32) -> Vec3 {
    let w = (ROLL_LEVEL_GAIN * up.angle_between(to)).clamp(LEVEL_LEAST, rate.max(LEVEL_LEAST));
    toward(up, to, w * dt)
}

/// The view's up following the suit's own, `to`, for `dt` s: close behind, never snapping.
pub fn follow(up: Vec3, to: Vec3, dt: f32) -> Vec3 {
    toward(up, to, (FOLLOW_GAIN * up.angle_between(to)).max(FOLLOW_LEAST) * dt)
}

/// The view's up rolled about the aim by `angle`, rad (positive: clockwise seen from behind, as the
/// command's roll is).
pub fn roll(up: Vec3, aim: Vec3, angle: f32) -> Vec3 {
    (Quat::from_axis_angle(aim.normalize_or(Vec3::Z), angle) * up).normalize()
}

/// The aim turned by the mouse about the view's `up`: `yaw` about it (as `Quat::from_axis_angle(up,
/// -yaw)` turns it: positive is the mouse to the right) and `pitch` toward it (positive: up), the
/// pitch never past [`PITCH_LIMIT`] of the horizon.
pub fn turn(aim: Vec3, up: Vec3, yaw: f32, pitch: f32) -> Vec3 {
    let along = aim.dot(up).clamp(-1.0, 1.0);
    let flat = (aim - up * along).normalize_or(up.any_orthonormal_vector());
    let heading = Quat::from_axis_angle(up, -yaw) * flat;
    let el = (along.asin() + pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    (heading * el.cos() + up * el.sin()).normalize_or(aim)
}

/// The aim kept within [`PITCH_LIMIT`] of `up`'s horizon (as it is, if it is).
pub fn keep(aim: Vec3, up: Vec3) -> Vec3 {
    if aim.dot(up).abs() > PITCH_LIMIT.sin() { turn(aim, up, 0.0, 0.0) } else { aim }
}

/// The command for this tick under shooter controls, `cmd` being what the keys and the mouse ask
/// for (its thrust the keys: x right, y up, z forward) and `up` the view's. Flying free, the suit
/// turns on its RCS while the aim is off its nose; where the shooter controls fly it ([`flies`]),
/// the keys are reshaped to move where the pilot looks and go out with the [`LockOn`] that reads
/// them. A command the lock-on has already shaped is only turned.
pub fn shape(cmd: InputCmd, up: Vec3, ctx: &InputContext) -> InputCmd {
    let p = ctx.predict;
    let s = p.state;
    let mut out = cmd;
    if p.mover().footing != Footing::Free {
        return out;
    }
    let nose = s.rot * Vec3::Z;
    if nose.angle_between(cmd.aim) > RCS_ANGLE || s.ang_vel.length() > RCS_SPIN {
        out.buttons |= RCS_SHARP;
    }
    // Falling inside the colony, the hover's keys are its own (Space climbs, C sinks).
    let falling = p.interior() && cmd.pressed(GRIP);
    if cmd.lockon.is_some() || falling || !flies(p, ctx.world, cmd.buttons) {
        return out;
    }
    // The wire's own numbers, so the stick is read back exactly as it's meant.
    let probe = InputCmd { lockon: Some(LockOn { ref_vel: Vec3::ZERO, up }), ..out }.quantized();
    let Some(axes) = lockon_assist(&probe, &s, frame(p.form.frame)) else { return out };
    out.lockon = probe.lockon;
    // A burst step's press goes along the keys themselves, in these axes (`flight::Burst`).
    if cmd.pressed(BURST) && !s.burst.held {
        return out;
    }
    let keys = cmd.thrust_vec();
    let aim = probe.aim.normalize_or(axes.fwd);
    let want = (aim * keys.z + axes.right * keys.x + axes.up * keys.y).normalize_or_zero();
    let q8 = |v: f32| (v * 127.0).round().clamp(-127.0, 127.0) as i8;
    out.thrust = [q8(want.dot(axes.right)), q8(want.dot(axes.up)), q8(want.dot(axes.fwd))];
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mouse_turns_about_the_up_and_stops_short_of_straight_up() {
        let up = Vec3::new(0.2, 1.0, -0.1).normalize();
        let aim = turn(Vec3::Z, up, 0.0, 0.0);
        // A turn across keeps the pitch; a turn up gains it, up to the limit and no further.
        let el = |a: Vec3| a.dot(up).asin();
        let across = turn(aim, up, 0.7, 0.0);
        assert!((el(across) - el(aim)).abs() < 1e-5, "{} vs {}", el(across), el(aim));
        assert!(across.angle_between(aim) > 0.5);
        let climbed = turn(aim, up, 0.0, 0.4);
        assert!((el(climbed) - (el(aim) + 0.4)).abs() < 1e-4);
        let mut a = aim;
        for _ in 0..40 {
            a = turn(a, up, 0.01, 0.1);
            assert!(el(a) <= PITCH_LIMIT + 1e-4, "{}", el(a));
        }
        assert!((el(a) - PITCH_LIMIT).abs() < 1e-4);
        // Back down, and below the horizon as far.
        for _ in 0..80 {
            a = turn(a, up, -0.01, -0.1);
        }
        assert!((el(a) + PITCH_LIMIT).abs() < 1e-4);
        // An aim from before, straight up, is brought back within the limit; one within it is kept.
        assert!((el(keep(up, up)) - PITCH_LIMIT).abs() < 1e-4);
        assert_eq!(keep(across, up), across);
        // The mouse to the right turns the aim as the simulator's mouse does.
        let sim = Quat::from_axis_angle(up, -0.3) * aim;
        assert!(turn(aim, up, 0.3, 0.0).distance(sim) < 1e-5);
    }

    #[test]
    fn the_view_levels_out_as_the_suit_does_and_follows_it_closely() {
        let to = Vec3::Y;
        let mut up = Vec3::new(1.0, 1.0, 0.0).normalize();
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        while up != to {
            up = level(up, to, 1.4, dt);
            t += dt;
            assert!(t < 4.0, "never levelled: {up}");
        }
        // 45° at the Argo's roll rate (1.4 rad/s) then easing: about a second and a half.
        assert!(t > 0.8 && t < 3.0, "{t}");
        let mut up = Vec3::X;
        for _ in 0..30 {
            up = follow(up, to, dt);
        }
        assert_eq!(up, to, "half a second behind at most");
        // Rolled with Q / E about the aim: clockwise from behind, as the command's roll is.
        let r = roll(Vec3::Y, Vec3::Z, 0.5);
        assert!((r.angle_between(Vec3::Y) - 0.5).abs() < 1e-5 && r.z.abs() < 1e-6);
        // Straight away from where it's going: it still gets there.
        let mut up = -Vec3::Y;
        for _ in 0..600 {
            up = level(up, Vec3::Y, 1.4, dt);
        }
        assert_eq!(up, Vec3::Y);
    }
}
