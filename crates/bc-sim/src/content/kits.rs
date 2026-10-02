//! Consumables: what a suit carries in its rack for the field (survival rules), used from the
//! cockpit's hotbar (keys 1–4). They're made at the fabricator and used up in fights, so they're
//! always in demand.
//!
//! - **Patch kit**: a field repair. It seals a leaking tank, or brings the worst-off system it can
//!   reach back a level (failed to damaged, damaged to working); a part shot off is beyond it.
//! - **Coolant flush**: dumps the suit's heat at once (Full Open's lockout still holds).
//! - **Chaff**: a cloud of foil and flares. Missiles tracking the suit lose it, every lock on it
//!   is broken, and for [`CHAFF_TICKS`] no new one can be built.
//! - **Stim**: the pilot bears [`STIM_G`] more for [`STIM_TICKS`], then crashes, bearing
//!   [`CRASH_G`] less, for [`CRASH_TICKS`]. One at a time.

use crate::TICK_HZ;

/// A kind of consumable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Kit {
    Patch = 0,
    Coolant = 1,
    Chaff = 2,
    Stim = 3,
}

impl Kit {
    pub const COUNT: usize = 4;
    /// In hotbar order (keys 1–4).
    pub const ALL: [Kit; Kit::COUNT] = [Kit::Patch, Kit::Coolant, Kit::Chaff, Kit::Stim];

    pub fn from_index(k: usize) -> Option<Kit> {
        Kit::ALL.get(k).copied()
    }

    pub fn slug(self) -> &'static str {
        match self {
            Kit::Patch => "patch_kit",
            Kit::Coolant => "coolant",
            Kit::Chaff => "chaff",
            Kit::Stim => "stim",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kit::Patch => "Patch kit",
            Kit::Coolant => "Coolant flush",
            Kit::Chaff => "Chaff",
            Kit::Stim => "Stim",
        }
    }

    /// The hotbar's short label.
    pub fn tag(self) -> &'static str {
        match self {
            Kit::Patch => "PATCH",
            Kit::Coolant => "COOL",
            Kit::Chaff => "CHAFF",
            Kit::Stim => "STIM",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Kit::Patch => "Seals a leaking tank, or brings the worst-off system back a level, in the field.",
            Kit::Coolant => "Dumps the suit's heat at once.",
            Kit::Chaff => "Breaks every lock on the suit; missiles tracking it lose it.",
            Kit::Stim => "The pilot bears 1 g more for a minute, then crashes for half a minute.",
        }
    }
}

/// The most of each kind a suit's rack holds.
pub const RACK: u8 = 3;
/// A stim: how long it lasts, then how long the crash does, ticks; and what the pilot bears more
/// (less, crashing), g.
pub const STIM_TICKS: u16 = 60 * TICK_HZ as u16;
pub const CRASH_TICKS: u16 = 30 * TICK_HZ as u16;
pub const STIM_G: f32 = 1.0;
pub const CRASH_G: f32 = -1.0;
/// How long chaff keeps locks off the suit, ticks.
pub const CHAFF_TICKS: u8 = 3 * TICK_HZ as u8;

/// What a stim's clock (`Status::stim`: ticks left, the crash's last) does to the G a pilot bears.
#[inline]
pub fn stim_g(stim: u16) -> f32 {
    if stim > CRASH_TICKS {
        STIM_G
    } else if stim > 0 {
        CRASH_G
    } else {
        0.0
    }
}

/// A rack: how many of each kind, 2 bits each, in [`Kit`] order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Kits(pub u8);

impl Kits {
    /// Bits a [`Kits`] takes on the wire.
    pub const BITS: u32 = 2 * Kit::COUNT as u32;
    pub const NONE: Kits = Kits(0);

    pub fn get(self, kit: Kit) -> u8 {
        (self.0 >> (2 * kit as u8)) & 3
    }

    pub fn set(&mut self, kit: Kit, n: u8) {
        let shift = 2 * kit as u8;
        self.0 = (self.0 & !(3 << shift)) | (n.min(RACK) << shift);
    }

    /// Takes one of `kit`, if there is one.
    pub fn take(&mut self, kit: Kit) -> bool {
        let n = self.get(kit);
        if n == 0 {
            return false;
        }
        self.set(kit, n - 1);
        true
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rack_holds_three_of_each() {
        let mut k = Kits::NONE;
        k.set(Kit::Chaff, 5);
        k.set(Kit::Stim, 1);
        assert_eq!((k.get(Kit::Patch), k.get(Kit::Chaff), k.get(Kit::Stim)), (0, RACK, 1));
        assert!(k.take(Kit::Stim) && !k.take(Kit::Stim));
        assert!(!k.take(Kit::Patch));
        assert_eq!(k.get(Kit::Chaff), RACK);
    }

    #[test]
    fn a_stim_lifts_then_crashes() {
        let t = STIM_TICKS + CRASH_TICKS;
        assert_eq!(stim_g(t), STIM_G);
        assert_eq!(stim_g(CRASH_TICKS + 1), STIM_G);
        assert_eq!(stim_g(CRASH_TICKS), CRASH_G);
        assert_eq!(stim_g(0), 0.0);
    }
}
