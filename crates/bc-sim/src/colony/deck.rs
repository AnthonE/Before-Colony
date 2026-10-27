//! The ring's decks (`docs/RING.md`): the floors people walk inside the First Colony, each a band
//! round its axis turning with it: the ring's floor, where the town is, and the base's deck in the
//! hub's drum, where the Muster is. Like the cylinder's strips before them (`frame::CityPos`), a deck
//! is unrolled into coordinates where plumb (towards the axis) is straight up, so a building whose
//! walls are plumb is a box in them; it is bent round the deck only where it's drawn.
//!
//! A place on a deck is a [`DeckPos`]: `x` along the axis (the colony's own frame), `s` round the
//! deck from its seam in metres at its floor, `h` up from the floor. On foot the walker uses the same
//! right-handed frame with Y up as on the strips: `(x, h, −s)`.

use core::f32::consts::{PI, TAU};

use glam::{Quat, Vec3};

use crate::content::base::SITES;
use crate::math::{atan2, cos, floor, quat_axis_angle, sin, sqrt};
use crate::pieces::parts::Aabb;
use crate::world::{COLONY_HALF_WIDTH, COLONY_INNER_RADIUS, COLONY_RADIUS, SPIN_RATE};

/// A deck: a band round the axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deck {
    /// Its floor's radius, m.
    pub floor_r: f32,
    /// Its ends along the axis, m (the colony's frame).
    pub x0: f32,
    pub x1: f32,
    /// How high its ceiling is over its floor, m.
    pub roof_h: f32,
    /// Where `s` is 0 round it, rad from +Y towards +Z (before the spin).
    pub seam: f32,
}

impl Deck {
    /// How far round it is at its floor, m.
    pub fn round(&self) -> f32 {
        TAU * self.floor_r
    }
}

/// The base's deck in the hub's drum: the Muster's floor, at 0.4 g, behind the bays.
pub const BASE: u8 = 0;
/// The ring's floor: the town, at 1 g.
pub const RING: u8 = 1;
/// The decks there are.
pub const DECKS: [Deck; 2] = [
    Deck { floor_r: 400.0, x0: -602.0, x1: -350.0, roof_h: 280.0, seam: PI },
    Deck {
        floor_r: COLONY_RADIUS,
        x0: -COLONY_HALF_WIDTH,
        x1: COLONY_HALF_WIDTH,
        roof_h: COLONY_RADIUS - COLONY_INNER_RADIUS,
        seam: PI,
    },
];

/// A place on a deck.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeckPos {
    pub deck: u8,
    /// Along the axis, m (the colony's own frame).
    pub x: f32,
    /// Round the deck from its seam, m at its floor: 0 up to its [`Deck::round`].
    pub s: f32,
    /// Up from the floor, m.
    pub h: f32,
}

impl DeckPos {
    pub const fn new(deck: u8, x: f32, s: f32, h: f32) -> Self {
        Self { deck, x, s, h }
    }

    fn of(&self) -> &'static Deck {
        &DECKS[self.deck as usize % DECKS.len()]
    }

    /// The angle round the axis, rad.
    pub fn angle(&self) -> f32 {
        let d = self.of();
        d.seam + self.s / d.floor_r
    }

    /// Where it is in the colony's own frame.
    pub fn to_colony(&self) -> Vec3 {
        let a = self.angle();
        let r = self.of().floor_r - self.h;
        Vec3::new(self.x, r * cos(a), r * sin(a))
    }

    /// In the walker's frame on its deck: `(x, h, −s)`.
    pub fn walker(&self) -> Vec3 {
        Vec3::new(self.x, self.h, -self.s)
    }

    /// Back from the walker's frame on deck `deck`.
    pub fn from_walker(deck: u8, w: Vec3) -> Self {
        Self { deck, x: w.x, s: -w.z, h: w.y }
    }
}

/// The deck a point of the colony's own frame is over, if it's over one (between its ends, and
/// between its floor and its ceiling), and where on it.
pub fn deck_at(p: Vec3) -> Option<DeckPos> {
    let r = sqrt(p.y * p.y + p.z * p.z);
    DECKS.iter().enumerate().find_map(|(k, d)| {
        let h = d.floor_r - r;
        ((d.x0..=d.x1).contains(&p.x) && (-1.0..=d.roof_h).contains(&h)).then(|| {
            let a = atan2(p.z, p.y) - d.seam;
            let a = a - floor(a / TAU) * TAU;
            DeckPos { deck: k as u8, x: p.x, s: a * d.floor_r, h }
        })
    })
}

/// The pull on deck `deck` at `h` m up from its floor, m/s².
pub fn gravity(deck: u8, h: f32) -> f32 {
    SPIN_RATE * SPIN_RATE * (DECKS[deck as usize % DECKS.len()].floor_r - h)
}

/// How long a metre of `s` is at height `h` on deck `deck`: the deck narrows towards the axis.
pub fn s_scale(deck: u8, h: f32) -> f32 {
    let r = DECKS[deck as usize % DECKS.len()].floor_r;
    (r - h) / r
}

/// Turns the walker's frame on deck `deck` at `s` into the colony's own: +X stays the axis, +Y
/// becomes up (towards the axis) there, +Z becomes −s.
pub fn local_frame(deck: u8, s: f32) -> Quat {
    let d = &DECKS[deck as usize % DECKS.len()];
    quat_axis_angle(Vec3::X, d.seam + s / d.floor_r + PI)
}

/// Whether the box `min..max` (the walker's frame on deck `deck`, `(x, h, −s)`) touches anything
/// solid there: the deck's floor, its ends, its ceiling, the bulkhead at its seam, or a building
/// standing on it (`content::base::SITES`). What a walker meets on the deck, and what the server's
/// pose check asks, as `city::solid` is on the cylinder's strips.
pub fn solid(deck: u8, min: Vec3, max: Vec3) -> bool {
    let d = &DECKS[deck as usize % DECKS.len()];
    // The floor, the deck's ends, its ceiling; round it, the seam (`s` from 0 to the round, so the
    // walker's z from −round to 0) is a bulkhead till the ring's town crosses it.
    if min.y < 0.0 || min.x < d.x0 || max.x > d.x1 || max.y > d.roof_h || max.z > 0.0 || min.z < -d.round() {
        return true;
    }
    let b = Aabb::new(min, max);
    SITES.iter().any(|site| site.deck == deck && site.hits(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::G0;

    #[test]
    fn the_ring_is_a_g_and_the_base_four_tenths() {
        assert!((gravity(RING, 0.0) / G0 - 1.0).abs() < 1e-3, "{}", gravity(RING, 0.0) / G0);
        assert!((gravity(BASE, 0.0) / G0 - 0.4).abs() < 5e-3, "{}", gravity(BASE, 0.0) / G0);
        assert!((gravity(BASE, 0.0) - crate::colony::hub::bay_g()).abs() < 1e-3);
    }

    #[test]
    fn places_go_round_trip_on_every_deck() {
        for (k, d) in DECKS.iter().enumerate() {
            for i in 0..=20 {
                let s = (d.round() * i as f32 / 20.0).clamp(0.01, d.round() - 0.01);
                let x = d.x0 + (d.x1 - d.x0) * i as f32 / 20.0;
                for h in [0.0f32, 1.62, 30.0] {
                    let p = DeckPos::new(k as u8, x, s, h);
                    let q = deck_at(p.to_colony()).expect("over its deck");
                    assert_eq!(q.deck, k as u8);
                    assert!((q.x - p.x).abs() < 1e-3);
                    assert!((q.s - p.s).abs() < 5e-3, "{p:?} → {q:?}");
                    assert!((q.h - p.h).abs() < 5e-3, "{p:?} → {q:?}");
                    assert_eq!(DeckPos::from_walker(k as u8, p.walker()), p);
                }
            }
        }
    }

    #[test]
    fn walls_are_plumb_and_the_walkers_frame_turns_right_handed() {
        for k in 0..DECKS.len() as u8 {
            for s in [0.0f32, 300.0, 1_000.0] {
                let foot = DeckPos::new(k, -400.0 * f32::from(k == BASE), s, 0.0).to_colony();
                let roof = DeckPos::new(k, -400.0 * f32::from(k == BASE), s, 20.0).to_colony();
                let up = (roof - foot).normalize();
                assert!((up - Vec3::new(0.0, -foot.y, -foot.z).normalize()).length() < 1e-5);
                let q = local_frame(k, s);
                assert!((q * Vec3::Y - up).length() < 1e-5, "up");
                assert!((q * Vec3::X - Vec3::X).length() < 1e-5, "the axis");
                // Across, the way `s` grows: a chord either side of `s`, so it's the tangent there.
                let across = DeckPos::new(k, 0.0, s + 0.5, 0.0).to_colony()
                    - DeckPos::new(k, 0.0, s - 0.5, 0.0).to_colony();
                assert!((q * Vec3::Z + across.normalize()).length() < 1e-3, "−s");
            }
        }
    }

    #[test]
    fn the_decks_fit_the_colony() {
        let (base, ring) = (DECKS[BASE as usize], DECKS[RING as usize]);
        // The base deck inside the drum, behind the bays' face, above the spine.
        assert!(base.x0 > crate::colony::hub::MOUTH_X && base.x1 <= crate::colony::hub::DRUM_X.1);
        assert!(base.floor_r < crate::colony::hub::DRUM_RADIUS);
        assert!(base.floor_r - base.roof_h > crate::colony::hub::SPINE_RADIUS);
        // The ring's floor is the hull's inside, its roof the ceiling.
        assert_eq!(ring.floor_r - ring.roof_h, COLONY_INNER_RADIUS);
    }
}

#[cfg(test)]
mod solid_tests {
    use super::*;

    #[test]
    fn the_deck_holds_a_walker_up_and_in() {
        let d = &DECKS[BASE as usize];
        let at = |x: f32, s: f32, h: f32| {
            let w = DeckPos::new(BASE, x, s, h).walker();
            (w - Vec3::new(0.3, 0.0, 0.3), w + Vec3::new(0.3, 1.8, 0.3))
        };
        // Standing on the floor, clear of everything; a hair into it, not.
        let (lo, hi) = at(-450.0, 1_500.0, 0.0);
        assert!(!solid(BASE, lo, hi));
        let (lo, hi) = at(-450.0, 1_500.0, -0.01);
        assert!(solid(BASE, lo, hi));
        // Its ends and its seam are walls.
        let (lo, hi) = at(d.x0 + 0.1, 1_500.0, 0.0);
        assert!(solid(BASE, lo, hi));
        let (lo, hi) = at(-450.0, 0.1, 0.0);
        assert!(solid(BASE, lo, hi));
    }
}
