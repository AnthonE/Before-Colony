//! Content tables: mobile-suit frames and weapons, the sector's landmarks, the colony's city, and
//! what being fed does to a pilot.
//!
//! All numbers are SI units (kg, N, m, s, rad). They are tuned for play but kept physically
//! consistent: acceleration comes from thrust and current mass, propellant burns at
//! `thrust / (Isp·g0)`, and delta-v is finite.

pub mod aces;
pub mod body;
pub mod city;
mod frames;
pub mod kits;
pub mod landmarks;
pub mod melee;
pub mod modules;
mod names;
pub mod salvage;
pub mod specials;
pub mod stagger;
pub mod systems;
mod weapons;

pub use frames::{
    AiHints, ArmSlot, Capsule, FrameSpec, Mount, PLAYABLE_ORDER, SPECIAL_MOUNT, SpecialKind, frame,
};
pub use kits::{Kit, Kits};
pub use melee::{ConeSpec, MeleeSpec, MissileSpec, Stroke};
pub use modules::{ModuleKind, Modules};
pub use names::{doll_name, frame_designation, frame_name, weapon_name};
pub use systems::{System, Systems};
pub use weapons::{ChargedShot, Replication, WeaponClass, WeaponSpec, weapon};

/// Whether pilots and agents may fly `id` (the server refuses the rest).
pub fn playable(id: bc_proto::FrameId) -> bool {
    frame(id).playable
}
