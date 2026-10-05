//! The pilot's body (`docs/LIFE.md`, 2): how much food is in them, on the wall clock, whatever the
//! colony's sky is doing.
//!
//! Awake (in the world) food burns second for second. Asleep (logged out, in their bunk) it burns
//! at a quarter of the rate ([`SLEEP_DIV`]), and sleep never takes a pilot past hungry: an Arrival
//! wakes wanting breakfast, never starving. A meal lasts about five hours awake, so three cover a
//! waking day. What it does in flight is the condition alone ([`Fed`], and its G in
//! `bc_sim::content::body`).

use bc_sim::content::body::Fed;
use serde::{Deserialize, Serialize};

use crate::food::{HOUR, Meal};
use crate::hangar::Done;

/// The most food a pilot holds, s awake.
pub const FULL_S: u32 = 8 * HOUR;
/// With this little food left or less, a pilot is peckish, s.
pub const PECKISH_S: u32 = HOUR;
/// Awake on an empty stomach this long, a hungry pilot is starving, s.
pub const STARVING_AFTER_S: u32 = 3 * HOUR;
/// Asleep, food burns at a quarter of the waking rate.
pub const SLEEP_DIV: u64 = 4;
/// What an Arrival wakes with the first time: a meal's worth, s.
pub const ARRIVAL_FOOD_S: u32 = 5 * HOUR;

/// A pilot's body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    /// Food in them, s awake it lasts.
    pub food_s: u32,
    /// Of that, how long they're well fed for, s awake.
    #[serde(default)]
    pub well_fed_s: u32,
    /// How long they've been awake on an empty stomach, s.
    #[serde(default)]
    pub empty_s: u32,
    /// In the world (or asleep in their bunk).
    #[serde(default)]
    pub awake: bool,
    /// How far the clock has run, unix seconds (0: not yet; it starts at the next [`Body::settle`]).
    #[serde(default)]
    pub at: u64,
}

impl Default for Body {
    /// An Arrival, fed and not yet woken.
    fn default() -> Self {
        Self { food_s: ARRIVAL_FOOD_S, well_fed_s: 0, empty_s: 0, awake: false, at: 0 }
    }
}

impl Body {
    /// Runs the clock to `now` (unix seconds), at the rate for being awake or asleep. A clock that
    /// has gone back changes nothing.
    pub fn settle(&mut self, now: u64) {
        if self.at == 0 {
            self.at = now;
            return;
        }
        let Some(dt) = now.checked_sub(self.at) else { return };
        let burn = if self.awake {
            self.at = now;
            let dt = u32::try_from(dt).unwrap_or(u32::MAX);
            self.empty_s = self.empty_s.saturating_add(dt.saturating_sub(self.food_s));
            dt
        } else {
            // What a quarter of the rate leaves over waits for the next settle, so a sleeper
            // settled often burns what one settled once does.
            let burn = dt / SLEEP_DIV;
            self.at += burn * SLEEP_DIV;
            u32::try_from(burn).unwrap_or(u32::MAX)
        };
        self.food_s = self.food_s.saturating_sub(burn);
        self.well_fed_s = self.well_fed_s.saturating_sub(burn).min(self.food_s);
    }

    /// In the world from `now`.
    pub fn wake(&mut self, now: u64) {
        self.settle(now);
        self.awake = true;
    }

    /// Asleep in their bunk from `now`.
    pub fn sleep(&mut self, now: u64) {
        self.settle(now);
        self.awake = false;
    }

    /// How fed they are.
    pub fn condition(&self) -> Fed {
        if self.food_s == 0 {
            if self.empty_s >= STARVING_AFTER_S { Fed::Starving } else { Fed::Hungry }
        } else if self.well_fed_s > 0 {
            Fed::WellFed
        } else if self.food_s <= PECKISH_S {
            Fed::Peckish
        } else {
            Fed::Fed
        }
    }

    /// Eats `meal` at `now`: refused if more than half of it wouldn't fit.
    pub fn eat(&mut self, meal: Meal, now: u64) -> Done {
        self.settle(now);
        let food = meal.dish.food_s();
        if self.food_s + food / 2 > FULL_S {
            return Err("you couldn't eat another bite".into());
        }
        self.food_s = (self.food_s + food).min(FULL_S);
        self.empty_s = 0;
        self.well_fed_s = self.well_fed_s.max(meal.glow_s()).min(self.food_s);
        Ok(format!("{} · {}", meal.dish.name().to_uppercase(), self.condition().tag()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::{Dish, GOOD_QUALITY};

    const T0: u64 = 1_800_000_000;

    fn awake() -> Body {
        let mut b = Body::default();
        b.wake(T0);
        b
    }

    #[test]
    fn an_arrival_wakes_fed_and_the_clock_starts_without_burning() {
        let b = awake();
        assert_eq!((b.food_s, b.at, b.awake), (ARRIVAL_FOOD_S, T0, true));
        assert_eq!(b.condition(), Fed::Fed);
    }

    #[test]
    fn awake_it_burns_second_for_second_and_goes_peckish_hungry_then_starving() {
        let mut b = awake();
        b.settle(T0 + u64::from(HOUR));
        assert_eq!(b.food_s, ARRIVAL_FOOD_S - HOUR);
        b.settle(T0 + u64::from(ARRIVAL_FOOD_S - PECKISH_S));
        assert_eq!(b.condition(), Fed::Peckish);
        b.settle(T0 + u64::from(ARRIVAL_FOOD_S));
        assert_eq!((b.food_s, b.empty_s, b.condition()), (0, 0, Fed::Hungry));
        b.settle(T0 + u64::from(ARRIVAL_FOOD_S + STARVING_AFTER_S - 1));
        assert_eq!(b.condition(), Fed::Hungry);
        b.settle(T0 + u64::from(ARRIVAL_FOOD_S + STARVING_AFTER_S));
        assert_eq!(b.condition(), Fed::Starving);
        // Eating puts it right at once.
        b.eat(Meal::staff(Dish::Noodles), T0 + u64::from(ARRIVAL_FOOD_S + STARVING_AFTER_S)).unwrap();
        assert_eq!((b.food_s, b.empty_s, b.condition()), (Dish::Noodles.food_s(), 0, Fed::Fed));
    }

    #[test]
    fn asleep_it_burns_a_quarter_and_never_starves_anyone() {
        let mut b = awake();
        b.sleep(T0);
        b.settle(T0 + 4 * u64::from(HOUR));
        assert_eq!(b.food_s, ARRIVAL_FOOD_S - HOUR);
        // A week away.
        b.settle(T0 + 7 * 24 * u64::from(HOUR));
        assert_eq!((b.food_s, b.empty_s, b.condition()), (0, 0, Fed::Hungry));
        // Back in the world, it's breakfast or starve.
        let back = T0 + 7 * 24 * u64::from(HOUR);
        b.wake(back);
        b.settle(back + u64::from(STARVING_AFTER_S));
        assert_eq!(b.condition(), Fed::Starving);
    }

    #[test]
    fn a_sleeper_settled_often_burns_what_one_settled_once_does() {
        let (mut often, mut once) = (awake(), awake());
        often.sleep(T0);
        once.sleep(T0);
        let mut t = T0;
        for step in (1..2_000).cycle().take(5_000) {
            t += step % 7;
            often.settle(t);
        }
        once.settle(t);
        assert_eq!(often.food_s, once.food_s);
        assert!(often.food_s < ARRIVAL_FOOD_S);
    }

    #[test]
    fn a_clock_gone_back_changes_nothing() {
        let mut b = awake();
        b.settle(T0 + 100);
        let before = b.clone();
        b.settle(T0 + 50);
        assert_eq!(b, before);
        b.settle(T0 + 100);
        assert_eq!(b, before);
    }

    #[test]
    fn a_full_pilot_is_refused_and_three_meals_make_a_day() {
        let mut b = awake();
        // 5 h in, a noodle bowl takes them to the most they hold.
        b.eat(Meal::staff(Dish::Noodles), T0).unwrap();
        assert_eq!(b.food_s, FULL_S);
        assert!(b.eat(Meal::staff(Dish::RationBar), T0).is_err(), "not another bite");
        // Eat when it runs out, three times: fed from breakfast to bed.
        let mut b = Body { food_s: 0, ..awake() };
        let mut t = T0;
        for _ in 0..3 {
            b.eat(Meal::staff(Dish::Noodles), t).unwrap();
            t += u64::from(Dish::Noodles.food_s());
            b.settle(t - 1);
            assert_ne!(b.condition(), Fed::Hungry);
        }
        assert!(t - T0 >= 15 * u64::from(HOUR));
    }

    #[test]
    fn a_good_cooks_meal_makes_a_pilot_well_fed_until_it_wears_off() {
        let mut b = Body { food_s: 0, ..awake() };
        b.eat(Meal::staff(Dish::Noodles), T0).unwrap();
        assert_eq!(b.condition(), Fed::Fed, "the staff's food fills, no more");
        let mut b = Body { food_s: 0, ..awake() };
        // Its glow wears off with more than an hour of the bowl still in them.
        let meal = Meal { dish: Dish::Noodles, quality: 75 };
        let note = b.eat(meal, T0).unwrap();
        assert!(note.contains("WELL FED"), "{note}");
        b.settle(T0 + u64::from(meal.glow_s()) - 1);
        assert_eq!(b.condition(), Fed::WellFed);
        b.settle(T0 + u64::from(meal.glow_s()));
        assert_eq!(b.condition(), Fed::Fed);
        // Asleep, the glow lasts as the food does: a quarter of the rate.
        let mut b = Body { food_s: 0, ..awake() };
        b.eat(Meal { dish: Dish::Noodles, quality: GOOD_QUALITY }, T0).unwrap();
        b.sleep(T0);
        b.settle(T0 + 4 * u64::from(HOUR));
        assert_eq!(b.condition(), Fed::WellFed);
        assert!(b.well_fed_s <= b.food_s);
    }

    #[test]
    fn a_body_keeps_as_json_and_an_old_record_gets_an_arrivals() {
        let mut b = awake();
        b.eat(Meal { dish: Dish::FishSupper, quality: 80 }, T0 + 60).unwrap();
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<Body>(&json).unwrap(), b);
        let old: Body = serde_json::from_str(r#"{"food_s": 7200}"#).unwrap();
        assert_eq!(old, Body { food_s: 7_200, ..Body::default() });
    }
}
