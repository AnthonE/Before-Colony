//! How a suit holds the gun in its right hand, as the client poses it (`bc-client`'s `anim`) and
//! as the models are built to fit (the tests in `lib.rs` hold every frame's gun clear of the suit
//! in both poses).
//!
//! The gun is built level in the hanging fist, its grip along the hand ([`Bone::Weapon`] at rest),
//! but it's never shown so. It turns in the fist about the grip, which then lies across the palm
//! as a pistol's does:
//!
//! - At the ready (in flight, or firing): the upper arm swings forward by [`READY_UPPER`] and the
//!   forearm bends up from it by [`READY_FORE`], near level, the fist in line with it; the gun
//!   turns back by both, so it keeps the aim, its body over the fist and the forearm right behind.
//! - Carried (parked, standing, walking, asleep): the arm hangs and the gun turns muzzle down by
//!   [`CARRY`], hanging down the outside of the leg.
//!
//! So nothing of a gun may lie behind its grip lower than the top of the forearm at the ready:
//! there it would run into the forearm, and carried it would stand up into it. What rides higher
//! (a sight, a handle, a stock over the forearm) clears it both ways: over it at the ready, ahead
//! of it carried.

use glam::Quat;

use crate::rig::Bone;

/// The gun arm at the ready: how far the upper arm swings forward and the forearm bends up from it
/// (rad, about the suit's x).
pub const READY_UPPER: f32 = -0.35;
pub const READY_FORE: f32 = -0.73;

/// How far the gun turns muzzle down in the fist when it's carried (rad, about the hand's x).
pub const CARRY: f32 = std::f32::consts::FRAC_PI_2;

/// A pose of the gun arm, each bone's turn about its joint relative to its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// At the ready, aimed straight ahead.
    Ready,
    /// Carried at the side.
    Carried,
}

impl Hold {
    pub const BOTH: [Hold; 2] = [Hold::Ready, Hold::Carried];

    /// The turn of `bone` in this pose (the identity for the bones it leaves at rest).
    pub fn turn(self, bone: Bone) -> Quat {
        match (self, bone) {
            (Hold::Ready, Bone::UpperArmR) => Quat::from_rotation_x(READY_UPPER),
            (Hold::Ready, Bone::ForearmR) => Quat::from_rotation_x(READY_FORE),
            (Hold::Ready, Bone::Weapon) => Quat::from_rotation_x(-(READY_UPPER + READY_FORE)),
            (Hold::Carried, Bone::Weapon) => Quat::from_rotation_x(CARRY),
            _ => Quat::IDENTITY,
        }
    }
}
