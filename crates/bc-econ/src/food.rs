//! What's eaten (`docs/LIFE.md`, 2): the colony's dishes, how long each keeps a pilot going, and
//! what the staff's counter charges for it. A [`Meal`] is a dish as it was cooked: by the colony's
//! staff, at their baseline, or by an Arrival in a cook's seat, as well as they cook it
//! (`crate::seats`). Only a good one makes a pilot well fed.

use serde::{Deserialize, Serialize};

use crate::seats::{MASTER_QUALITY, NOVICE_CEILING, STAFF_QUALITY};

/// An hour, s.
pub const HOUR: u32 = 3_600;
/// A meal this good or better makes a pilot well fed. The staff cook below it, and so does a new
/// cook, so only an Arrival who has practised can.
pub const GOOD_QUALITY: u8 = 70;
const _: () = assert!(STAFF_QUALITY < GOOD_QUALITY && NOVICE_CEILING < GOOD_QUALITY);
const _: () = assert!(GOOD_QUALITY <= MASTER_QUALITY);

/// A dish.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dish {
    /// Pressed grain and protein, sold everywhere: a snack.
    RationBar,
    /// Eggs, flatbread and orchard fruit.
    Breakfast,
    /// Gardens' fruit and greens.
    OrchardPlate,
    /// A bowl of noodles in broth: the docks' meal.
    Noodles,
    /// Fish from Canal's tanks, with bread: the big one.
    FishSupper,
}

impl Dish {
    pub const COUNT: usize = 5;
    /// Smallest first.
    pub const ALL: [Dish; Dish::COUNT] =
        [Dish::RationBar, Dish::OrchardPlate, Dish::Breakfast, Dish::Noodles, Dish::FishSupper];

    pub fn slug(self) -> &'static str {
        match self {
            Dish::RationBar => "ration_bar",
            Dish::Breakfast => "breakfast",
            Dish::OrchardPlate => "orchard_plate",
            Dish::Noodles => "noodles",
            Dish::FishSupper => "fish_supper",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dish::RationBar => "Ration bar",
            Dish::Breakfast => "Colony breakfast",
            Dish::OrchardPlate => "Orchard plate",
            Dish::Noodles => "Noodle bowl",
            Dish::FishSupper => "Canal fish supper",
        }
    }

    /// How long it keeps a pilot going, s awake.
    pub fn food_s(self) -> u32 {
        match self {
            Dish::RationBar => 3 * HOUR / 2,
            Dish::OrchardPlate => 3 * HOUR,
            Dish::Breakfast => 4 * HOUR,
            Dish::Noodles => 5 * HOUR,
            Dish::FishSupper => 6 * HOUR,
        }
    }

    /// What the staff's counter charges for it, cr.
    pub fn price(self) -> u64 {
        match self {
            Dish::RationBar => 8,
            Dish::OrchardPlate => 18,
            Dish::Breakfast => 20,
            Dish::Noodles => 25,
            Dish::FishSupper => 40,
        }
    }
}

/// A dish as it was cooked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meal {
    pub dish: Dish,
    /// How well it was cooked, 0 to 100.
    pub quality: u8,
}

impl Meal {
    /// `dish` from the colony's staff.
    pub fn staff(dish: Dish) -> Self {
        Self { dish, quality: STAFF_QUALITY }
    }

    /// How long it makes a pilot well fed, s: not at all under [`GOOD_QUALITY`], and from there its
    /// quality's share of the dish's hours.
    pub fn glow_s(self) -> u32 {
        if self.quality < GOOD_QUALITY {
            return 0;
        }
        (u64::from(self.dish.food_s()) * u64::from(self.quality.min(100)) / 100) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dishes_run_from_a_snack_to_a_supper() {
        let mut slugs: Vec<&str> = Dish::ALL.iter().map(|d| d.slug()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), Dish::COUNT);
        for w in Dish::ALL.windows(2) {
            assert!(w[0].food_s() < w[1].food_s() && w[0].price() < w[1].price(), "{w:?}");
        }
        // Three meals cover a waking day.
        assert_eq!(Dish::Noodles.food_s(), 5 * HOUR);
        for d in Dish::ALL {
            let json = serde_json::to_string(&d).unwrap();
            assert_eq!(json, format!("\"{}\"", d.slug()));
            assert_eq!(serde_json::from_str::<Dish>(&json).unwrap(), d);
        }
    }

    #[test]
    fn only_a_good_meal_makes_a_pilot_well_fed() {
        assert_eq!(Meal::staff(Dish::FishSupper).glow_s(), 0);
        assert_eq!(Meal { dish: Dish::Noodles, quality: GOOD_QUALITY - 1 }.glow_s(), 0);
        let good = Meal { dish: Dish::Noodles, quality: GOOD_QUALITY }.glow_s();
        let best = Meal { dish: Dish::Noodles, quality: 100 }.glow_s();
        assert!(0 < good && good < best);
        assert_eq!(best, Dish::Noodles.food_s());
        assert_eq!(Meal { dish: Dish::Noodles, quality: 255 }.glow_s(), best, "never more than the dish");
    }
}
