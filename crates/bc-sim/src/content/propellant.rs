//! Propellant grades: how pure what's in the tank is. Volatiles cracked harder make a cleaner
//! propellant whose exhaust runs faster, so each kilogram pushes further: a grade multiplies the
//! suit's specific impulse ([`Grade::isp`]), under the real rules its delta-v and under anime rules
//! how long its boost lasts. A suit flies one grade at a time (the bay pumps the tank out to change
//! it), and its owner's snapshot says which, so prediction burns as the server does.

/// What's in a suit's tank.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Grade {
    /// Cracked from the colony's water, as every suit has always flown on.
    #[default]
    Standard = 0,
    /// Refined further: three-quarters of the volatiles come out.
    Refined = 1,
    /// Refined again over an exotic-metal catalyst: half come out.
    UltraPure = 2,
}

/// Specific impulse of each grade, of Standard's.
pub const REFINED_ISP: f32 = 1.15;
pub const ULTRA_PURE_ISP: f32 = 1.35;

impl Grade {
    pub const COUNT: usize = 3;
    pub const ALL: [Grade; Self::COUNT] = [Grade::Standard, Grade::Refined, Grade::UltraPure];

    /// The grade with code `v` (unknown codes: Standard).
    pub fn from_code(v: u8) -> Grade {
        match v {
            1 => Grade::Refined,
            2 => Grade::UltraPure,
            _ => Grade::Standard,
        }
    }

    /// Its specific impulse, of Standard's.
    pub fn isp(self) -> f32 {
        match self {
            Grade::Standard => 1.0,
            Grade::Refined => REFINED_ISP,
            Grade::UltraPure => ULTRA_PURE_ISP,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Grade::Standard => "standard",
            Grade::Refined => "refined",
            Grade::UltraPure => "ultra",
        }
    }

    pub fn from_slug(s: &str) -> Option<Grade> {
        Self::ALL.into_iter().find(|g| g.slug() == s)
    }

    pub fn name(self) -> &'static str {
        match self {
            Grade::Standard => "Standard",
            Grade::Refined => "Refined",
            Grade::UltraPure => "Ultra-pure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_round_trip_and_go_further() {
        for g in Grade::ALL {
            assert_eq!(Grade::from_code(g as u8), g);
            assert_eq!(Grade::from_slug(g.slug()), Some(g));
        }
        assert_eq!(Grade::from_code(3), Grade::Standard);
        assert!(
            Grade::Standard.isp() < Grade::Refined.isp() && Grade::Refined.isp() < Grade::UltraPure.isp()
        );
    }
}
