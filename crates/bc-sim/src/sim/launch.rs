//! Survival: suits their pilots built, launched from the colony's docking hub and brought home
//! again. (Under arcade rules, [`Sim::join`](super::Sim::join) hands out any frame at its
//! faction's spawn point instead.)
//!
//! - [`Sim::launch`](super::Sim::launch) puts a [`Loadout`] in the sector at the docking hub's
//!   mouth: the parts its pilot fitted (as worn as they are), the weapons fitted on its mounts,
//!   the rounds loaded and the propellant in the tank.
//! - [`Sim::dock`](super::Sim::dock) takes a suit that has come to rest in the dock out of the
//!   sector again, and says what it brings home ([`Homecoming`]): what's left of it, its hold,
//!   whatever it has in hand, and the bounties it earned.

use bc_proto::{CARGO_KINDS, ChunkDesc, FrameId, Part};

/// A suit as its pilot built it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loadout {
    /// Armour per part, as a fraction of the frame's (0: not fitted). The torso is the suit, so
    /// it must be fitted.
    pub parts: [f32; Part::COUNT],
    /// A bit per loadout slot (primary, secondary, melee): its weapon is fitted.
    pub mounts: u8,
    /// Rounds loaded, per loadout slot.
    pub ammo: [u16; 3],
    /// In the tank, kg.
    pub propellant: f32,
}

impl Loadout {
    /// Everything fitted, new, loaded and full (tests, and arcade frames).
    pub fn full(frame_id: FrameId) -> Self {
        let spec = crate::content::frame(frame_id);
        let mut ammo = [0; 3];
        for (a, m) in ammo.iter_mut().zip(spec.loadout.iter()) {
            if let Some(m) = m {
                *a = crate::content::weapon(m.weapon).ammo;
            }
        }
        Self { parts: [1.0; Part::COUNT], mounts: 0b111, ammo, propellant: spec.propellant_cap }
    }
}

/// What a suit brings home when it docks, or what its pilot gets when it's lost (the bounty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homecoming {
    /// The frame line (Neo-Bird comes home as Wing Zero).
    pub frame: FrameId,
    /// Armour per part as a fraction of the frame's; 0: gone (shot off, or never fitted).
    pub parts: [f32; Part::COUNT],
    pub mounts: u8,
    pub ammo: [u16; 3],
    pub propellant: f32,
    /// The hold, kg per cargo kind.
    pub cargo_kg: [u16; CARGO_KINDS],
    /// Whatever it had in hand (a hulk it towed in, a limb, ore).
    pub held: Option<ChunkDesc>,
    /// Credits the colony pays for the Mobile Dolls it destroyed.
    pub bounty: u32,
}
