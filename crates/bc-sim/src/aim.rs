//! Where a shot goes: from its gun to the point under its pilot's crosshair.
//!
//! The crosshair is the line from the pilot's eye ([`FrameSpec::eye`], the head's main camera,
//! which the cockpit view looks from) along the aim. A gun sits metres off that line, at a hand or
//! a shoulder, so a shot fired parallel to the aim lands that far off whatever the crosshair is on:
//! a target's breadth, in a room. The command says how far along the aim the crosshair's point is
//! ([`InputCmd::converge`], from what its pilot's client sees there), and the gun aims at that
//! point. A shot leaves with the suit's own velocity added, so the gun aims off by what that would
//! carry it aside: its path, not just its barrel, crosses the point (the suits' fire control does
//! it; it buys the shot nothing it hasn't).
//!
//! Pure in the command and the suit's state, so the server and the owner's prediction fire the same
//! shot to the bit. A command without a range ([`InputCmd::converge`] 0: an agent's, or one from
//! before the crosshair had one) fires along the aim, as every shot once did.
//!
//! [`FrameSpec::eye`]: crate::content::FrameSpec::eye
//! [`InputCmd::converge`]: bc_proto::InputCmd::converge

use bc_proto::input::CONVERGE_UNIT;
use glam::{Quat, Vec3};

use crate::math::{floor, normalize_or, sqrt};

/// The nearest a crosshair's point is taken to be, m: nearer, a gun at a hand would swing across
/// the suit's own body to reach it.
pub const CONVERGE_MIN: f32 = 10.0;

/// The range a command's crosshair is on, m, at most `max` (its weapon's reach): none for a command
/// that doesn't say (`converge` 0).
#[inline]
pub fn range(converge: u16, max: f32) -> Option<f32> {
    (converge != 0).then(|| (f32::from(converge) * CONVERGE_UNIT).clamp(CONVERGE_MIN, max.max(CONVERGE_MIN)))
}

/// A range on the wire ([`bc_proto::InputCmd::converge`]): quarter metres, never 0.
#[inline]
pub fn encode(range: f32) -> u16 {
    floor(range / CONVERGE_UNIT + 0.5).clamp(1.0, f32::from(u16::MAX)) as u16
}

/// The point the crosshair is on: `range` m along `aim` from the eye of a suit at `pos`, turned
/// `rot`, whose eye is at `eye` in its own frame.
#[inline]
pub fn point(pos: Vec3, rot: Quat, eye: Vec3, aim: Vec3, range: f32) -> Vec3 {
    pos + rot * eye + aim * range
}

/// Which way a gun at `muzzle` on a suit moving at `vel` fires a shot of `speed` so its path
/// crosses `p`: the line to it, less what the suit's velocity would carry the shot aside. Where no
/// shot that fast can make up the suit's own drift, straight at the point (`or` if the point is
/// the muzzle).
pub fn through(p: Vec3, muzzle: Vec3, vel: Vec3, speed: f32, or: Vec3) -> Vec3 {
    let u = normalize_or(p - muzzle, or);
    let along = vel.dot(u);
    let side = vel - u * along;
    let s2 = speed * speed - side.length_squared();
    if s2 <= 0.0 {
        return u;
    }
    let ahead = sqrt(s2);
    if along + ahead <= 0.0 {
        return u;
    }
    // speed·dir = ahead·u − side: the shot's velocity, vel + speed·dir, is (along + ahead)·u.
    normalize_or(u * ahead - side, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_crosses_the_point_however_the_suit_drifts() {
        let p = Vec3::new(5.0, -3.0, 60.0);
        let muzzle = Vec3::new(3.4, 0.6, 3.0);
        for vel in [Vec3::ZERO, Vec3::new(150.0, 0.0, 0.0), Vec3::new(-40.0, 30.0, 200.0)] {
            let speed = 4_000.0;
            let d = through(p, muzzle, vel, speed, Vec3::Z);
            assert!((d.length() - 1.0).abs() < 1e-5);
            let v = vel + d * speed;
            let miss = (p - muzzle).cross(v.normalize()).length();
            assert!(miss < 1e-3, "{vel}: misses by {miss}");
            assert!(v.dot(p - muzzle) > 0.0);
        }
    }

    #[test]
    fn ranges_on_the_wire() {
        assert_eq!(range(0, 4_000.0), None);
        assert_eq!(range(encode(57.3), 4_000.0), Some(57.25));
        assert_eq!(range(encode(2.0), 4_000.0), Some(CONVERGE_MIN));
        assert_eq!(range(encode(9_000.0), 4_000.0), Some(4_000.0));
        assert_eq!(encode(1e9), u16::MAX);
    }
}
