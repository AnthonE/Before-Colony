//! The pilot's body: how fed they are, and what that does to the G they bear (`docs/LIFE.md`).
//!
//! The clock that feeds and starves them is the economy's (`bc_econ::body`, on the wall clock, off
//! the tick); what reaches flight is only the condition, as the stim's clock does: a few bits of the
//! own snapshot, added to the stat sheet where [`super::kits::stim_g`] is. [`Fed::Fed`] is 0, so a
//! zeroed field changes nothing.

/// How fed a pilot is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Fed {
    /// More than an hour's food in them.
    #[default]
    Fed = 0,
    /// A good meal's glow still on them.
    WellFed = 1,
    /// An hour's food or less.
    Peckish = 2,
    /// Nothing left.
    Hungry = 3,
    /// Awake on an empty stomach for hours.
    Starving = 4,
}

impl Fed {
    pub const COUNT: usize = 5;
    /// Bits a [`Fed`] takes on the wire.
    pub const BITS: u32 = 3;
    /// Best first.
    pub const ALL: [Fed; Fed::COUNT] = [Fed::WellFed, Fed::Fed, Fed::Peckish, Fed::Hungry, Fed::Starving];

    pub fn from_index(k: u8) -> Option<Fed> {
        match k {
            0 => Some(Fed::Fed),
            1 => Some(Fed::WellFed),
            2 => Some(Fed::Peckish),
            3 => Some(Fed::Hungry),
            4 => Some(Fed::Starving),
            _ => None,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Fed::Fed => "fed",
            Fed::WellFed => "well_fed",
            Fed::Peckish => "peckish",
            Fed::Hungry => "hungry",
            Fed::Starving => "starving",
        }
    }

    /// The HUD's label.
    pub fn tag(self) -> &'static str {
        match self {
            Fed::Fed => "FED",
            Fed::WellFed => "WELL FED",
            Fed::Peckish => "PECKISH",
            Fed::Hungry => "HUNGRY",
            Fed::Starving => "STARVING",
        }
    }
}

/// What a pilot bears more (less, hungry), g, under the real flight rules (the anime rules double
/// it, as they do the stim's). For scale: a pilot bears 6 g, a damaged cockpit 5.
pub const WELL_FED_G: f32 = 0.5;
pub const HUNGRY_G: f32 = -0.5;
pub const STARVING_G: f32 = -1.0;

/// What being `fed` does to the G a pilot bears.
#[inline]
pub fn fed_g(fed: Fed) -> f32 {
    match fed {
        Fed::WellFed => WELL_FED_G,
        Fed::Fed | Fed::Peckish => 0.0,
        Fed::Hungry => HUNGRY_G,
        Fed::Starving => STARVING_G,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_condition_round_trips_through_its_index_and_fits_its_bits() {
        for f in Fed::ALL {
            assert_eq!(Fed::from_index(f as u8), Some(f));
            assert!(u32::from(f as u8) < 1 << Fed::BITS);
        }
        assert_eq!(Fed::from_index(Fed::COUNT as u8), None);
        assert_eq!(Fed::default() as u8, 0, "a zeroed field is the condition that changes nothing");
        assert_eq!(fed_g(Fed::default()), 0.0);
    }

    #[test]
    fn the_better_fed_bear_more() {
        let g = Fed::ALL.map(fed_g);
        assert!(g.windows(2).all(|w| w[0] >= w[1]), "{g:?}");
        assert!(g[0] > 0.0 && g[Fed::COUNT - 1] < 0.0);
    }
}
