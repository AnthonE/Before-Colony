//! Content tables: mobile-suit frames and weapons.
//!
//! All numbers are SI units (kg, N, m, s, rad). They are tuned for play but kept physically
//! consistent: acceleration comes from thrust and current mass, propellant burns at
//! `thrust / (Isp·g0)`, and delta-v is finite.

mod frames;
mod names;
pub mod salvage;
mod weapons;

pub use frames::{ArmSlot, Capsule, FrameSpec, Mount, frame};
pub use names::{frame_designation, frame_name, weapon_name};
pub use weapons::{WeaponSpec, weapon};
