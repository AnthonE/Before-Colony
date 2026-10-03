//! The colony's sights (`bc_sim::content::city::SIGHTS`): each named once a pilot on foot comes
//! within reach of it, and kept on their found-list, a setting (`sights_found`: a bit a sight), so
//! the map can show what's been found and what's still out there.

use bc_sim::colony::city::{BANK_ROW, BLOCK, ROWS, grid_x, row_at, row_span};
use bc_sim::content::city::SIGHTS;

/// A sight is reached within this distance of its block's middle, m.
pub const SIGHT_REACH: f32 = 140.0;

/// Every sight's bit.
pub const ALL: u32 = if SIGHTS.len() >= 32 { u32::MAX } else { (1 << SIGHTS.len()) - 1 };

const _: () = assert!(SIGHTS.len() <= 32, "the found-list keeps a bit a sight");

/// Sight `i`: its strip, and `(s, x)` at its block's middle.
pub fn sight_at(i: usize) -> (u8, f32, f32) {
    let (strip, bx, row, _) = SIGHTS[i];
    let (s0, s1) = row_span(row);
    (strip, (s0 + s1) * 0.5, grid_x(bx) + BLOCK * 0.5)
}

/// Sight `i`'s name.
pub fn name(i: usize) -> &'static str {
    SIGHTS[i].3
}

/// Where on the streets sight `i` is seen from: `(s, x)` on the street along the axis nearest it
/// (by the window banks, the bank road), abreast of its middle: the end of a walk to it.
pub fn stand(i: usize) -> (f32, f32) {
    let (_, s, x) = sight_at(i);
    let street = match row_at(s) {
        r if r <= -BANK_ROW => row_span(-ROWS).0,
        r if r >= BANK_ROW => row_span(ROWS).1,
        _ => crate::city_nav::street_line(s),
    };
    (street, x)
}

/// The sight a pilot standing at `(s, x)` on strip `strip` has reached: the nearest within
/// [`SIGHT_REACH`].
pub fn reached(strip: u8, s: f32, x: f32) -> Option<usize> {
    let d = |i: usize| {
        let (k, ss, sx) = sight_at(i);
        (k == strip).then(|| (ss - s).hypot(sx - x)).filter(|d| *d < SIGHT_REACH)
    };
    (0..SIGHTS.len()).filter_map(|i| d(i).map(|d| (d, i))).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, i)| i)
}

/// Sight `i`'s bit on the found-list.
pub fn bit(i: usize) -> u32 {
    1 << i
}

/// How many sights the found-list holds.
pub fn count(found: u32) -> usize {
    (found & ALL).count_ones() as usize
}

/// What a pilot is told on reaching sight `i`, with `found` the list before: its name, and the
/// first time, that it's found and how many of them have been.
pub fn news(i: usize, found: u32) -> String {
    if found & bit(i) != 0 {
        return name(i).to_string();
    }
    format!("SIGHT FOUND · {} · {} OF {}", name(i), count(found | bit(i)), SIGHTS.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{KERB, Stage, solid};
    use bc_sim::colony::frame::STRIP_WIDTH;
    use glam::Vec3;

    /// Whether a walker (0.6 × 1.8 m) can stand at `(s, x)` on `strip`, on the street or a kerb.
    fn standable(strip: u8, s: f32, x: f32) -> bool {
        [0.0, KERB + 0.01].iter().any(|&h| {
            !solid(strip, Vec3::new(x - 0.3, h, -s - 0.3), Vec3::new(x + 0.3, h + 1.8, -s + 0.3), Stage(0))
        })
    }

    #[test]
    fn every_sight_can_be_reached_on_foot_and_named() {
        let mut names = std::collections::HashSet::new();
        for i in 0..SIGHTS.len() {
            assert!(names.insert(name(i)), "{} twice", name(i));
            let (strip, s, x) = sight_at(i);
            assert!((0.0..=STRIP_WIDTH).contains(&s) && x.abs() < 16_000.0, "{} is off its strip", name(i));
            // Its spot on the street is somewhere to stand, and standing there reaches it.
            let (ps, px) = stand(i);
            assert!(standable(strip, ps, px), "nowhere to stand by {}: ({ps}, {px})", name(i));
            assert_eq!(reached(strip, ps, px), Some(i), "{} from ({ps}, {px})", name(i));
            assert_eq!(reached((strip + 1) % 3, ps, px), None, "{} on another strip", name(i));
        }
    }

    #[test]
    fn the_found_list_counts_and_says_so_once() {
        assert_eq!(count(0), 0);
        assert_eq!(count(ALL), SIGHTS.len());
        assert_eq!(count(u32::MAX), SIGHTS.len(), "bits past the sights don't count");
        let first = news(1, 0);
        assert!(first.starts_with("SIGHT FOUND") && first.contains(name(1)), "{first}");
        assert!(first.ends_with(&format!("1 OF {}", SIGHTS.len())), "{first}");
        assert_eq!(news(1, bit(1) | bit(4)), name(1), "found already: just its name");
        assert!(news(2, bit(1) | bit(4)).contains("3 OF"), "{}", news(2, bit(1) | bit(4)));
        // Far from every sight, nothing is reached.
        assert_eq!(reached(0, STRIP_WIDTH * 0.5, -15_000.0), None);
    }
}
