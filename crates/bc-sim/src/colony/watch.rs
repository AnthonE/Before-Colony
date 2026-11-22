//! Heat and the Watch's tables (`docs/OPEN_CITY.md`, "Heat and the Watch"; `docs/FREE_ROAM.md`, 4):
//! what each offence adds to a pilot's heat, the levels it climbs through, how fast it cools, what
//! each level sends out of the Watch's stations ([`ANSWERS`]: its cars and hounds, how fast, from how
//! many stations, the cordon they hold) and what its fine is; and what the Watch's own cars on their
//! beats see. The server keeps every pilot's heat by these (`bc-server`'s `watch`), sends its units
//! by them (`bc-server`'s `dispatch`), and the client draws both (`bc_client_core::watch`): data, so
//! the mechanics can tune it without code.
//!
//! Heat is on the tick's clock ([`Heat`]): it falls a level each [`COOL_TICKS`] without an offence,
//! and doesn't fall while one of the Watch's cars has the pilot in sight ([`Heat::hold`]). Nothing
//! here allocates or keeps a clock of its own.

use glam::Vec3;
use libm::{ceilf, hypotf};

use super::deck::{self, RING};
use super::{street, town};
use crate::TICK_HZ;

/// What draws the Watch's eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offence {
    /// A car taken from a kerb.
    TookACar,
    /// The same, in sight of one of the Watch's own cars.
    TookACarInSight,
    /// A car taken from its driver, stopped in the road; the same in the Watch's sight; and one
    /// of the Watch's own.
    Jacked,
    JackedInSight,
    TookTheWatchs,
    /// A knock against another car.
    Knock,
    /// A second of driving at speed where people walk.
    Reckless,
    /// Somebody run down.
    HitSomeone,
}

impl Offence {
    pub const ALL: [Offence; 8] = [
        Offence::TookACar,
        Offence::TookACarInSight,
        Offence::Jacked,
        Offence::JackedInSight,
        Offence::TookTheWatchs,
        Offence::Knock,
        Offence::Reckless,
        Offence::HitSomeone,
    ];

    /// What it adds to a pilot's heat.
    pub const fn points(self) -> f32 {
        match self {
            // (Nobody much minds a car borrowed from a kerb, till there's more.)
            Offence::TookACar => 0.6,
            Offence::TookACarInSight => 1.2,
            // (Its driver's there to say so.)
            Offence::Jacked => 1.2,
            Offence::JackedInSight => 3.0,
            Offence::TookTheWatchs => 6.0,
            Offence::Knock => 1.0,
            Offence::Reckless => 0.8,
            Offence::HitSomeone => 4.0,
        }
    }

    /// Its name on the wire and in the logs.
    pub const fn slug(self) -> &'static str {
        match self {
            Offence::TookACar => "took_a_car",
            Offence::TookACarInSight => "took_a_car_in_sight",
            Offence::Jacked => "jacked",
            Offence::JackedInSight => "jacked_in_sight",
            Offence::TookTheWatchs => "took_the_watchs",
            Offence::Knock => "knock",
            Offence::Reckless => "reckless",
            Offence::HitSomeone => "hit_someone",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.slug() == s)
    }

    /// The same done in sight of one of the Watch's cars (only taking a car is worse for it).
    pub const fn in_sight(self) -> Self {
        match self {
            Offence::TookACar => Offence::TookACarInSight,
            Offence::Jacked => Offence::JackedInSight,
            o => o,
        }
    }

    /// Whether a client may say it did this: what the server can't yet see for itself (the cars
    /// at the kerbs and in the traffic are each client's own, and so is what its car knocks
    /// into). A report can only ever add heat. Whether the Watch saw it, the server judges.
    pub const fn reported(self) -> bool {
        matches!(self, Offence::TookACar | Offence::Jacked | Offence::TookTheWatchs | Offence::Knock)
    }

    /// Whether it adds heat only if somebody saw it (`docs/OPEN_CITY.md`: seen, not done). The
    /// rest are seen by their nature: a driver put out or knocked into, the one run down, the
    /// people driven at, the Watch's own car. Only a car taken from a kerb can go unseen.
    pub const fn wants_a_witness(self) -> bool {
        matches!(self, Offence::TookACar)
    }

    /// What the pilot is told when the Watch takes it: who saw what.
    pub const fn seen_line(self) -> &'static str {
        match self {
            Offence::TookACar => "SEEN TAKING A CAR",
            Offence::TookACarInSight => "THE WATCH SAW YOU TAKE THAT CAR",
            Offence::Jacked => "ITS DRIVER SAW YOUR FACE",
            Offence::JackedInSight => "THE WATCH SAW YOU PUT ITS DRIVER OUT",
            Offence::TookTheWatchs => "YOU TOOK ONE OF THE WATCH'S OWN",
            Offence::Knock => "THE CAR YOU KNOCKED CALLED IT IN",
            Offence::Reckless => "SEEN DRIVING WHERE PEOPLE WALK",
            Offence::HitSomeone => "SEEN RUNNING SOMEONE DOWN",
        }
    }

    /// What the pilot is told when nobody saw it.
    pub const fn unseen_line(self) -> &'static str {
        match self {
            Offence::TookACar | Offence::TookACarInSight => "NOBODY SAW YOU TAKE IT",
            _ => "NOBODY SAW",
        }
    }
}

/// The most the Watch wants anyone.
pub const MAX_HEAT: u8 = 5;
/// The points each level starts at.
pub const LEVELS: [f32; MAX_HEAT as usize] = [1.0, 3.0, 6.0, 10.0, 15.0];
/// The most points anyone carries: a little over the top level's start.
pub const MOST: f32 = LEVELS[LEVELS.len() - 1] + 3.0;
/// How long without an offence before the heat falls a level, s and ticks.
pub const COOL: f32 = 28.0;
pub const COOL_TICKS: u32 = 28 * TICK_HZ;

/// What the Watch sends out of its stations (`docs/FREE_ROAM.md`, 4; `docs/OPEN_CITY.md`, "Heat and
/// the Watch"): its cars, and its Sirius hounds, the drones it flies low over the roads. (Its suits,
/// the Aldebaran, the Regulus and the Fomalhaut, come later.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Car,
    Hound,
}

impl UnitKind {
    pub const ALL: [UnitKind; 2] = [UnitKind::Car, UnitKind::Hound];

    /// How many every station of the Watch's keeps.
    pub const fn kept(self) -> u8 {
        match self {
            UnitKind::Car => 3,
            UnitKind::Hound => 4,
        }
    }

    /// How fast it goes along the roads at full haste, m/s.
    pub const fn speed(self) -> f32 {
        match self {
            UnitKind::Car => 17.0,
            UnitKind::Hound => 22.0,
        }
    }

    /// How near a pilot it stops them, m: a car's officers out of it, a hound's grip from above
    /// the road.
    pub const fn reach(self) -> f32 {
        match self {
            UnitKind::Car => 10.0,
            UnitKind::Hound => 14.0,
        }
    }

    /// Its name on the wire.
    pub const fn slug(self) -> &'static str {
        match self {
            UnitKind::Car => "car",
            UnitKind::Hound => "hound",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.slug() == s)
    }
}

/// The units every station keeps, of every kind.
pub const STATION_UNITS: usize = UnitKind::Car.kept() as usize + UnitKind::Hound.kept() as usize;

/// What the Watch sends at a level of heat or of an incident: how many of each kind, from how many
/// of its stations (the nearest by road first), how hard they drive, and the cordon they hold round
/// the place once there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Answer {
    pub cars: u8,
    pub hounds: u8,
    /// The most stations it's sent from, nearest by road first.
    pub stations: u8,
    /// The share of its kind's top speed each drives at.
    pub haste: f32,
    /// How far round the place its units stand, m, and how long they hold there, s.
    pub cordon: f32,
    pub hold: f32,
}

const fn answer_of(cars: u8, hounds: u8, stations: u8, haste: f32, cordon: f32, hold: f32) -> Answer {
    Answer { cars, hounds, stations, haste, cordon, hold }
}

/// What each level sends, 0 to [`MAX_HEAT`]: data, so the mechanics can tune it without code.
pub const ANSWERS: [Answer; MAX_HEAT as usize + 1] = [
    answer_of(0, 0, 0, 0.0, 0.0, 0.0),
    answer_of(1, 1, 1, 0.7, 30.0, 30.0),
    answer_of(2, 2, 2, 0.8, 45.0, 45.0),
    answer_of(3, 3, 2, 0.9, 60.0, 60.0),
    answer_of(4, 4, 3, 1.0, 80.0, 75.0),
    answer_of(6, 6, 4, 1.0, 100.0, 90.0),
];

/// What the Watch sends at `level` (past the top, the top's).
pub fn answer(level: u8) -> Answer {
    ANSWERS[usize::from(level.min(MAX_HEAT))]
}

/// The Watch's cars sent at each level of heat ([`ANSWERS`]' cars).
pub const UNITS: [u8; MAX_HEAT as usize + 1] = {
    let mut u = [0; MAX_HEAT as usize + 1];
    let mut k = 0;
    while k < u.len() {
        u[k] = ANSWERS[k].cars;
        k += 1;
    }
    u
};

/// Units after a wanted pilot find the way to their last pose again this often, s.
pub const REROUTE: f32 = 1.5;
/// A pilot slower than this, m/s, with one of the Watch's units within its reach for this long, s,
/// is stopped.
pub const CATCH: (f32, f32) = (2.5, 2.5);
/// Stopped where no unit can come to their side (a park, a quay) with one within this far, m, the
/// officers walk over at this pace, m/s, after this wait, s.
pub const AFOOT: (f32, f32, f32) = (80.0, 4.0, 8.0);
/// A unit this near a wanted pilot has them in sight, m: their heat doesn't cool under its eye.
pub const UNIT_SIGHT: f32 = 90.0;

/// How long a pilot held at `far` m from the nearest of the Watch's units (none within
/// [`AFOOT`]'s reach: never) and slow is free yet, s: within the unit's `reach`, [`CATCH`]'s
/// wait; further, the officers' walk over.
pub fn catch_after(far: f32, reach: f32) -> Option<f32> {
    if far <= reach {
        Some(CATCH.1)
    } else if far <= AFOOT.0 {
        Some(AFOOT.2 + far / AFOOT.1)
    } else {
        None
    }
}
/// What the Watch's fine at level 1 is, credits: it goes with the level's square.
pub const FINE: u64 = 150;

/// The level (0 to [`MAX_HEAT`]) `points` of heat is at.
pub fn level(points: f32) -> u8 {
    LEVELS.iter().take_while(|at| points >= **at).count() as u8
}

/// The Watch's cars sent at `level`.
pub fn units(level: u8) -> usize {
    usize::from(UNITS[usize::from(level.min(MAX_HEAT))])
}

/// The Watch's fine for a pilot stopped at `level`, credits.
pub const fn fine(level: u8) -> u64 {
    let l = (if level > MAX_HEAT { MAX_HEAT } else { level }) as u64;
    FINE * l * l
}

/// What `points` of heat comes to after a quiet spell of `quiet` ticks: a level lower for each
/// [`COOL_TICKS`] of it, each fall to the start of the level below (nothing, from the first); under
/// the first level, nothing after one.
pub fn cooled(mut points: f32, quiet: u32) -> f32 {
    for _ in 0..(quiet / COOL_TICKS).min(u32::from(MAX_HEAT) + 1) {
        let l = level(points);
        points = if l >= 2 { LEVELS[usize::from(l) - 2] } else { 0.0 };
    }
    points
}

/// How much the Watch wants a pilot, on the tick's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heat {
    points: f32,
    /// The tick the quiet began: the last offence, the last time a car of the Watch's saw them, or
    /// the last fall.
    since: u32,
}

impl Heat {
    /// 0 (nothing) to [`MAX_HEAT`].
    pub fn level(&self) -> u8 {
        level(self.points)
    }

    pub fn points(&self) -> f32 {
        self.points
    }

    /// An offence at tick `tick`: true if the heat rose a level.
    pub fn offend(&mut self, o: Offence, tick: u32) -> bool {
        self.cool(tick);
        let before = self.level();
        self.points = (self.points + o.points()).min(MOST);
        self.since = tick;
        self.level() > before
    }

    /// In sight of one of the Watch's cars at tick `tick`: nothing cools under its eye.
    pub fn hold(&mut self, tick: u32) {
        if self.points > 0.0 {
            self.since = self.since.max(tick);
        }
    }

    /// The quiet up to tick `tick`: true if the heat fell (a level, or more for a long quiet).
    pub fn cool(&mut self, tick: u32) -> bool {
        if self.points <= 0.0 {
            self.points = 0.0;
            self.since = tick;
            return false;
        }
        let quiet = tick.saturating_sub(self.since);
        let falls = quiet / COOL_TICKS;
        if falls == 0 {
            return false;
        }
        let before = self.level();
        self.points = cooled(self.points, quiet);
        self.since += falls * COOL_TICKS;
        self.level() < before
    }

    /// The tick it next falls at, if nothing more happens (none: it's nothing).
    pub fn falls_at(&self) -> Option<u32> {
        (self.points > 0.0).then(|| self.since.saturating_add(COOL_TICKS))
    }

    /// `points` of heat kept while its pilot was away for `quiet` ticks, at tick `tick`: cooled as
    /// that quiet would have.
    pub fn resumed(points: f32, quiet: u32, tick: u32) -> Self {
        let points = cooled(points.clamp(0.0, MOST), quiet);
        Self { points, since: tick.saturating_sub(quiet % COOL_TICKS) }
    }

    /// Paid off (a fine): nothing wanted.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The fine the Watch takes at this heat, credits.
    pub fn fine(&self) -> u64 {
        fine(self.level())
    }
}

/// How far one of the Watch's cars sees down a street, m.
pub const SIGHT: f32 = 80.0;
/// How near one of the town's people sees a car taken from its kerb, m.
pub const WITNESS: f32 = 30.0;
/// A car driven faster than this where people walk is driven recklessly, m/s.
pub const RECKLESS_SPEED: f32 = 9.0;

/// Whether one of the Watch's own cars on its beat is within `reach` m of `(x, s)` on the ring's
/// floor (across it, round it) at tick `tick` plus `frac`.
pub fn car_near(x: f32, s: f32, reach: f32, tick: u32, frac: f32) -> bool {
    let t = town::town_t(s);
    let mut near = false;
    street::each_car(x - reach, x + reach, t - reach, t + reach, tick, frac, &mut |c| {
        near |= c.kind == street::Kind::Watch && hypotf(c.x - x, c.t - t) <= reach;
    });
    near
}

/// Whether one of the Watch's own cars on its beat within `reach` m of `(x, s)` has a clear line to
/// it: nothing of the town's (a building, a lift's pier) between them at eye height.
pub fn car_sees(x: f32, s: f32, reach: f32, tick: u32, frac: f32) -> bool {
    let t = town::town_t(s);
    let mut seen = false;
    street::each_car(x - reach, x + reach, t - reach, t + reach, tick, frac, &mut |c| {
        let (dx, dt) = (c.x - x, c.t - t);
        let far = hypotf(dx, dt);
        if seen || c.kind != street::Kind::Watch || far > reach {
            return;
        }
        let steps = ceilf(far / 2.5).max(1.0) as u32;
        seen = (1..steps).all(|k| {
            let u = k as f32 / steps as f32;
            let (px, ps) = (x + dx * u, s + dt * u);
            !deck::solid(RING, Vec3::new(px - 0.3, 1.0, -ps - 0.3), Vec3::new(px + 0.3, 1.6, -ps + 0.3))
        });
    });
    seen
}

/// Whether a car driven at `(x, s)` is where people walk: on a block, a pavement or a rim walk.
pub fn where_people_walk(x: f32, s: f32) -> bool {
    matches!(
        town::ground_at(x, town::town_t(s)),
        town::Ground::Block(_) | town::Ground::Pavement | town::Ground::RimWalk
    )
}

/// Whether one of the town's people is within `reach` m of `(x, s)` at tick `tick` (not one of
/// `down`, who aren't walking).
pub fn person_near(x: f32, s: f32, reach: f32, tick: u32, down: &[u32]) -> bool {
    let t = town::town_t(s);
    let mut near = false;
    street::each_person(x - reach, x + reach, t - reach, t + reach, tick, 0.0, &mut |p| {
        near |= !down.contains(&p.id) && hypotf(p.x - x, p.t - t) <= reach;
    });
    near
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_rises_with_what_a_pilot_does_and_falls_when_they_stop() {
        let mut h = Heat::default();
        assert_eq!(h.level(), 0);
        assert!(!h.offend(Offence::TookACar, 0), "a car from a kerb, unseen: nothing yet");
        assert!(h.offend(Offence::Knock, 1));
        assert_eq!(h.level(), 1);
        assert!(!h.offend(Offence::Knock, 2), "still the first level");
        assert!(h.offend(Offence::Knock, 3));
        assert_eq!(h.level(), 2);
        // In sight of the Watch it's worse; and it never goes past the top.
        for k in 0..20 {
            h.offend(Offence::TookACar.in_sight(), 4 + k);
        }
        assert_eq!((h.level(), h.points()), (MAX_HEAT, MOST));
        assert_eq!((fine(1), fine(3), h.fine(), fine(9)), (150, 1_350, 3_750, 3_750));
        // Quiet, it falls a level every while; an offence starts the wait again.
        let at = 23;
        assert_eq!(h.falls_at(), Some(at + COOL_TICKS));
        assert!(!h.cool(at + COOL_TICKS - 1));
        assert!(h.cool(at + COOL_TICKS));
        assert_eq!(h.level(), 4);
        h.offend(Offence::Reckless, at + COOL_TICKS + 10);
        assert!(!h.cool(at + 2 * COOL_TICKS), "the wait began again");
        assert_eq!(h.level(), 4);
        // Held in the Watch's sight, it doesn't fall at all.
        let t = at + COOL_TICKS + 10;
        for k in 1..100 {
            h.hold(t + k * 30);
            assert!(!h.cool(t + k * 30));
        }
        // A long quiet takes it down level by level, to nothing.
        assert!(h.cool(t + 99 * 30 + 2 * COOL_TICKS));
        assert_eq!(h.level(), 2);
        assert!(h.cool(t + 99 * 30 + 9 * COOL_TICKS));
        assert_eq!((h.level(), h.points(), h.falls_at()), (0, 0.0, None));
        // Under the first level, it's gone after one quiet spell.
        h.offend(Offence::TookACar, 0);
        assert!(!h.cool(COOL_TICKS) && h.points() == 0.0);
        h.offend(Offence::TookTheWatchs, 5);
        assert_eq!(h.level(), 3);
        h.clear();
        assert_eq!(h, Heat::default());
    }

    #[test]
    fn the_tables_say_what_each_level_is() {
        assert_eq!(
            [0.0, 0.99, 1.0, 2.9, 3.0, 6.0, 9.9, 10.0, 15.0, MOST].map(level),
            [0, 0, 1, 1, 2, 3, 3, 4, 5, 5]
        );
        assert_eq!(core::array::from_fn::<usize, 7, _>(|l| units(l as u8)), [0, 1, 2, 3, 4, 6, 6]);
        assert_eq!(cooled(16.0, COOL_TICKS), 10.0);
        assert_eq!(cooled(16.0, 3 * COOL_TICKS - 1), 6.0);
        assert_eq!(cooled(2.0, COOL_TICKS), 0.0);
        assert_eq!(cooled(16.0, u32::MAX), 0.0);
        for o in Offence::ALL {
            assert_eq!(Offence::from_slug(o.slug()), Some(o));
            assert!(o.in_sight().points() >= o.points());
        }
        assert_eq!(Offence::from_slug("loitering"), None);
        // What a client may say it did: never a hit or reckless driving (the server sees those).
        assert!(!Offence::HitSomeone.reported() && !Offence::Reckless.reported());
        assert!(!Offence::TookACarInSight.reported() && !Offence::JackedInSight.reported());
    }

    #[test]
    fn each_level_sends_more_from_further_and_the_stations_have_them() {
        assert_eq!(answer(0), Answer { cars: 0, hounds: 0, stations: 0, haste: 0.0, cordon: 0.0, hold: 0.0 });
        assert_eq!(answer(9), answer(MAX_HEAT));
        for w in ANSWERS.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(b.cars >= a.cars && b.hounds >= a.hounds && b.stations >= a.stations, "{a:?} {b:?}");
            assert!(b.cars + b.hounds > a.cars + a.hounds, "{b:?} sends no more than {a:?}");
            assert!(b.haste >= a.haste && b.cordon > a.cordon && b.hold > a.hold, "{a:?} {b:?}");
        }
        for (l, a) in ANSWERS.iter().enumerate().skip(1) {
            // The stations it calls on keep enough of each kind, and it drives no faster than flat out.
            let n = u32::from(a.stations);
            assert!(u32::from(a.cars) <= n * u32::from(UnitKind::Car.kept()), "level {l}: {a:?}");
            assert!(u32::from(a.hounds) <= n * u32::from(UnitKind::Hound.kept()), "level {l}: {a:?}");
            assert!((0.5..=1.0).contains(&a.haste), "level {l}: {a:?}");
            assert!(u16::from(a.stations) <= town::DISTRICT_COUNT);
        }
        assert_eq!(UNITS, ANSWERS.map(|a| a.cars));
        assert_eq!(STATION_UNITS, 7);
        for k in UnitKind::ALL {
            assert_eq!(UnitKind::from_slug(k.slug()), Some(k));
            assert!(k.speed() > 10.0 && k.reach() > 5.0);
        }
        assert_eq!(UnitKind::from_slug("tank"), None);
        // Held slow: within reach, a moment; further, the walk over; too far, never.
        assert_eq!(catch_after(5.0, UnitKind::Car.reach()), Some(CATCH.1));
        assert_eq!(catch_after(40.0, UnitKind::Car.reach()), Some(AFOOT.2 + 10.0));
        assert_eq!(catch_after(AFOOT.0 + 1.0, UnitKind::Hound.reach()), None);
        assert!(UNIT_SIGHT > AFOOT.0);
    }

    #[test]
    fn the_watchs_cars_see_down_their_street_and_not_through_the_houses() {
        // One of the Watch's on a street between the blocks.
        let mut beat = None;
        street::each_car(-1_000.0, 1_000.0, 0.0, 6_000.0, 0, 0.0, &mut |c| {
            if c.kind == street::Kind::Watch && c.x.abs() > 100.0 {
                beat = beat.or(Some(*c));
            }
        });
        let c = beat.expect("one of the Watch's on a street between the blocks");
        let ahead = town::deck_s(c.t + 30.0);
        assert!(car_near(c.x, ahead, SIGHT, 0, 0.0));
        assert!(car_sees(c.x, ahead, SIGHT, 0, 0.0));
        let behind = (c.x + 45.0, town::deck_s(c.t + 30.0));
        assert!(car_near(behind.0, behind.1, SIGHT, 0, 0.0));
        let on = town::ground_at(behind.0, town::town_t(behind.1));
        assert!(
            !car_sees(behind.0, behind.1, SIGHT, 0, 0.0) || !matches!(on, town::Ground::Block(_)),
            "{on:?}"
        );
        assert!(where_people_walk(-300.0, town::deck_s(town::CELL * 30.5)));
        assert!(!where_people_walk(20.0, town::deck_s(town::CELL * 20.5)), "the boulevard's carriageway");
    }

    #[test]
    fn somebody_about_is_near_unless_theyre_down() {
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 200.0, x + 200.0, t - 200.0, t + 200.0, 900, 0.0, &mut |p| {
            who = who.or(Some(*p));
        });
        let p = who.expect("somebody about");
        let s = town::deck_s(p.t);
        assert!(person_near(p.x + 1.0, s, 3.0, 900, &[]));
        // With everyone within reach down, nobody's about.
        let mut there = [u32::MAX; 16];
        let mut n = 0;
        street::each_person(p.x - 4.0, p.x + 6.0, p.t - 5.0, p.t + 5.0, 900, 0.0, &mut |q| {
            if n < there.len() {
                there[n] = q.id;
                n += 1;
            }
        });
        assert!(n < there.len() && !person_near(p.x + 1.0, s, 3.0, 900, &there[..n]));
    }
}
