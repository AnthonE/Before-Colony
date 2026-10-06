//! Weathering: what a suit's paint shows of the life it has had. Every sortie, every hour under
//! thrust, every hit it took and every time it overheated leaves its mark, and servicing and
//! overhauls don't touch it (they're about what's inside): a suit starts clean, and earns its
//! look. Its level, 0 (factory fresh) to [`MAX`] (a veteran's), goes on the roster with its pilot
//! (`bc_proto::control::roster_flags`), so every client draws it: the hull shader fades the
//! paint, settles grime in its crevices and down its plates, chips its edges to the red primer
//! and then to bare metal, flakes its faces, scratches it and leaves old scorch marks.
//!
//! | What's counted | Points |
//! |---|---|
//! | a sortie flown | [`PER_SORTIE`] |
//! | a minute under thrust (burning or boosting) | 1 |
//! | armour lost, per [`DAMAGE_PER_POINT`] percentage points over the parts | 1 |
//! | the suit overheating | [`PER_OVERHEAT`] |

use bc_proto::Part;
use bc_sim::TICK_HZ;
use bc_sim::sim::Usage;
use serde::{Deserialize, Serialize};

/// The most weathered a suit gets.
pub const MAX: u8 = 7;
/// Points for a sortie flown, for an overheat, and the armour lost per point.
pub const PER_SORTIE: u32 = 3;
pub const PER_OVERHEAT: u32 = 4;
pub const DAMAGE_PER_POINT: u32 = 8;
/// Where each level from 1 up starts, in points: a sortie or two scuffs a suit, a few dozen
/// make it a veteran.
pub const LEVELS: [u32; MAX as usize] = [8, 25, 55, 100, 170, 270, 420];

/// What a suit has been through, for its paint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weathering {
    #[serde(default)]
    pub sorties: u32,
    /// Ticks under thrust: burning and boosting.
    #[serde(default)]
    pub thrust: u32,
    /// Armour lost, percentage points summed over the parts and sorties (repairs don't take it
    /// back: the paint remembers).
    #[serde(default)]
    pub damage: u32,
    #[serde(default)]
    pub overheats: u32,
}

impl Weathering {
    pub fn is_new(&self) -> bool {
        *self == Weathering::default()
    }

    /// A sortie's marks: what it used (`usage`), and each part's condition, %, as it launched
    /// (`before`) and as it came home (`after`; a part shot off lost all it had).
    pub fn sortie(
        &mut self,
        usage: &Usage,
        before: &[Option<u8>; Part::COUNT],
        after: &[Option<u8>; Part::COUNT],
    ) {
        self.sorties = self.sorties.saturating_add(1);
        self.thrust = self.thrust.saturating_add(usage.burn).saturating_add(usage.boost);
        self.overheats = self.overheats.saturating_add(u32::from(usage.overheats));
        let lost: u32 = before
            .iter()
            .zip(after)
            .map(|(b, a)| u32::from(b.unwrap_or(0).saturating_sub(a.unwrap_or(0))))
            .sum();
        self.damage = self.damage.saturating_add(lost);
    }

    /// Its points (see the module's table).
    pub fn points(&self) -> u32 {
        let minutes = self.thrust / (60 * TICK_HZ);
        self.sorties
            .saturating_mul(PER_SORTIE)
            .saturating_add(minutes)
            .saturating_add(self.damage / DAMAGE_PER_POINT)
            .saturating_add(self.overheats.saturating_mul(PER_OVERHEAT))
    }

    /// How weathered its paint looks, 0 (factory fresh) to [`MAX`].
    pub fn level(&self) -> u8 {
        let p = self.points();
        LEVELS.iter().filter(|&&from| p >= from).count() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(minutes: u32, overheats: u16) -> Usage {
        Usage { burn: minutes * 60 * TICK_HZ, overheats, ..Usage::default() }
    }

    #[test]
    fn a_suit_starts_clean_and_earns_its_look() {
        let mut w = Weathering::default();
        assert_eq!(w.level(), 0);
        let whole = [Some(100); Part::COUNT];
        // A quiet patrol scuffs it a little.
        w.sortie(&usage(4, 0), &whole, &whole);
        assert_eq!((w.points(), w.level()), (7, 0));
        w.sortie(&usage(4, 0), &whole, &whole);
        assert_eq!(w.level(), 1);
        // A hard fight marks it far more: an arm shot off, the rest knocked about, an overheat.
        let after = [Some(60), Some(40), None, Some(70), Some(55), Some(80)];
        w.sortie(&usage(9, 1), &whole, &after);
        assert_eq!(w.damage, 40 + 60 + 100 + 30 + 45 + 20);
        assert!(w.level() >= 2, "{w:?}: {}", w.points());
        // It only ever grows, and stops at the most.
        let mut last = w.level();
        for _ in 0..200 {
            w.sortie(&usage(10, 1), &whole, &after);
            assert!(w.level() >= last);
            last = w.level();
        }
        assert_eq!(w.level(), MAX);
    }

    #[test]
    fn levels_climb() {
        assert!(LEVELS.windows(2).all(|p| p[0] < p[1]));
    }
}
