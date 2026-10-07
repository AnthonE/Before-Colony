//! Stagger, after Armored Core VI's attitude control (`docs/PEERS.md`, "The mech games"; the
//! simulation's side is `sim::stagger`): every blow's impact builds up on a suit's attitude
//! control, and past what its frame stands the suit is staggered: it tumbles, its thrusters and
//! weapons all but stop, and blows that land meanwhile are direct hits.
//!
//! What a frame stands is its *stability*; how hard a weapon hits, its *impact* per armour point
//! of the blow (before the target's armour: a gundanium plate stops damage, not a blow's push).

use bc_proto::{FrameId, WeaponKind};

use crate::config::secs;
use crate::content::{WeaponClass, weapon};

/// How long a stagger lasts, ticks (1 s).
pub const STAGGER_TICKS: u8 = secs(1.0) as u8;
/// Damage a blow does to a staggered suit, times its own (a direct hit).
pub const DIRECT_HIT: f32 = 1.5;
/// A staggered suit's thrusters give this much of their thrust (its control computer is busy
/// catching it), and its attitude control nothing.
pub const STAGGER_THRUST: f32 = 0.25;
/// The spin a stagger knocks into a suit, rad/s.
pub const STAGGER_SPIN: f32 = 1.2;
/// Impact drains away once a suit has gone this long without being hit, ticks...
pub const RECOVER_AFTER: u32 = secs(0.8);
/// ...at this share of its frame's stability a second.
pub const RECOVER_RATE: f32 = 0.5;

/// How much impact a frame stands before it's staggered (impact points: armour points of blows,
/// times each weapon's [`impact`]). Heavier and steadier frames stand more.
pub fn stability(frame: FrameId) -> f32 {
    match frame {
        FrameId::Leo => 300.0,
        FrameId::Taurus => 240.0,
        FrameId::Virgo => 380.0,
        FrameId::WingZero | FrameId::WingZeroBird => 340.0,
        FrameId::Heavyarms => 380.0,
        FrameId::Sandrock => 460.0,
        FrameId::Deathscythe => 320.0,
        FrameId::Shenlong => 340.0,
    }
}

/// A weapon's impact per armour point of a blow: a beam's 1, a solid round's more, a missile's
/// warhead and a blade's stroke more still, a flame's little. The Twin Buster Rifle and a reactor's
/// blast hit hardest of all.
pub fn impact(kind: WeaponKind) -> f32 {
    match kind {
        WeaponKind::TwinBusterRifle => 1.6,
        WeaponKind::Reactor => 2.0,
        _ => match weapon(kind).class {
            WeaponClass::Beam => 1.0,
            WeaponClass::Ballistic => 1.3,
            WeaponClass::Missile => 2.0,
            WeaponClass::Melee => 1.8,
            WeaponClass::Cone => 0.5,
        },
    }
}

// A stagger fits its wire field (`bc_proto::snapshot::STAGGER_MAX`).
const _: () = assert!(STAGGER_TICKS > 0 && STAGGER_TICKS <= bc_proto::snapshot::STAGGER_MAX);
