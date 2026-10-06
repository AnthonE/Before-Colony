//! Seats (`docs/LIFE.md`, 1): every job in the colony is a seat at a post, worked by an Arrival who
//! sits down in it or, when none does, by the colony's staff. The world is never short of hands,
//! and Arrivals take over from the staff, never the other way round.
//!
//! An Arrival is a pilot with a body or an agent without one, by the same rules (`STORY.md`), so a
//! seat doesn't ask which. The staff work at a fair baseline ([`STAFF_QUALITY`]) and never better;
//! an Arrival's work runs from [`NOVICE_QUALITY`] up to a ceiling their practice raises
//! ([`Skills`]), by how well they do the job's own task. What a seat sells pays its worker
//! [`WAGE_PCT`] of the price and the colony the rest ([`Seat::pay`]): all of it, under the staff.

use std::collections::BTreeMap;

use bc_sim::content::city::STRIP_NAMES;
use serde::{Deserialize, Serialize};

use crate::hangar::Done;

/// What the staff's work comes out at: a fair baseline, never better.
pub const STAFF_QUALITY: u8 = 50;
/// The worst an Arrival's work comes out at.
pub const NOVICE_QUALITY: u8 = 40;
/// The best an Arrival's work comes out at before any practice.
pub const NOVICE_CEILING: u8 = 60;
/// The best there is.
pub const MASTER_QUALITY: u8 = 100;
// A new Arrival can do worse than the staff, and better.
const _: () = assert!(NOVICE_QUALITY < STAFF_QUALITY && STAFF_QUALITY < NOVICE_CEILING);
const _: () = assert!(NOVICE_CEILING < MASTER_QUALITY);
/// Units of work (a dish served, a delivery made) that take an Arrival's ceiling half the way from
/// [`NOVICE_CEILING`] to [`MASTER_QUALITY`].
pub const HALF_PRACTICE: u32 = 200;
/// A worker's share of what their seat sells, %; the rest is the colony's, the seat's rent.
pub const WAGE_PCT: u64 = 80;
/// An Arrival away from their seat this long has let their shift lapse, s.
pub const SHIFT_LAPSE_S: u64 = 10 * 60;

fn refuse<T>(why: impl Into<String>) -> Result<T, String> {
    Err(why.into())
}

/// A job in the colony.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    /// The Arrival's kitchen: meals.
    Cook,
    /// The Arrival's bar.
    Barkeep,
    /// Meals and parcels across the city and up the cap lift to the bays.
    Courier,
    /// The cap lift's scanner at a strip's Hub Gate.
    Customs,
    /// Approach slots for suits coming home to the dock.
    Dockmaster,
    /// The zero-G foundry's melt.
    Foundry,
    /// Overhauls done for hire in the bays.
    Mechanic,
}

impl Job {
    pub const COUNT: usize = 7;
    pub const ALL: [Job; Job::COUNT] =
        [Job::Cook, Job::Barkeep, Job::Courier, Job::Customs, Job::Dockmaster, Job::Foundry, Job::Mechanic];

    pub fn slug(self) -> &'static str {
        match self {
            Job::Cook => "cook",
            Job::Barkeep => "barkeep",
            Job::Courier => "courier",
            Job::Customs => "customs",
            Job::Dockmaster => "dockmaster",
            Job::Foundry => "foundry",
            Job::Mechanic => "mechanic",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Job::Cook => "Cook",
            Job::Barkeep => "Barkeep",
            Job::Courier => "Courier",
            Job::Customs => "Customs officer",
            Job::Dockmaster => "Dockmaster",
            Job::Foundry => "Foundry operator",
            Job::Mechanic => "Mechanic",
        }
    }

    /// The HUD's label.
    pub fn tag(self) -> &'static str {
        match self {
            Job::Cook => "COOK",
            Job::Barkeep => "BARKEEP",
            Job::Courier => "COURIER",
            Job::Customs => "CUSTOMS",
            Job::Dockmaster => "DOCKMASTER",
            Job::Foundry => "FOUNDRY",
            Job::Mechanic => "MECHANIC",
        }
    }

    /// The job's own task: what an Arrival does well or badly.
    pub fn work(self) -> &'static str {
        match self {
            Job::Cook => "A kitchen line: tickets in, dishes out, on time and in order.",
            Job::Barkeep => "Pouring, and keeping the room's mood.",
            Job::Courier => "Routing across the city's traffic, its trams and the cap lift.",
            Job::Customs => "The cap lift's scanner: who and what comes down.",
            Job::Dockmaster => "Approach slots for suits coming home.",
            Job::Foundry => "Keeping the melt in its band.",
            Job::Mechanic => "Overhauls done for hire.",
        }
    }
}

/// Where a seat is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Post {
    /// The Arrival, the bar off Charter Square.
    Arrival,
    /// Nowhere in particular: the city and the lifts.
    Streets,
    /// A strip's Hub Gate, where its cap lift comes down.
    HubGate { strip: u8 },
    /// The dock off the hub's mouth.
    Dock,
    /// The hub's zero-G foundry.
    Foundry,
    /// The pilots' bays in the hub's spin ring.
    Bays,
}

impl Post {
    pub fn name(self) -> String {
        match self {
            Post::Arrival => "The Arrival".into(),
            Post::Streets => "the streets".into(),
            Post::HubGate { strip } => match STRIP_NAMES.get(usize::from(strip)) {
                Some(s) => {
                    let mut s = s.to_lowercase();
                    s[..1].make_ascii_uppercase();
                    format!("{s}'s Hub Gate")
                }
                None => "Hub Gate".into(),
            },
            Post::Dock => "the dock".into(),
            Post::Foundry => "the foundry".into(),
            Post::Bays => "the bays".into(),
        }
    }
}

/// The Arrival working a seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    /// Their key (a wallet's, or a guest's for the visit; never shown).
    pub who: String,
    pub name: String,
    /// When they sat down, and when they were last at work, unix seconds.
    pub since: u64,
    pub seen: u64,
}

/// Who works a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Worker<'a> {
    /// The colony's staff.
    Staff,
    Arrival(&'a Holder),
}

/// What a sale at a seat pays: its worker's wage and the colony's share, which always make the
/// price between them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pay {
    pub wage: u64,
    pub colony: u64,
}

/// One job at one post.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub id: u16,
    pub job: Job,
    pub post: Post,
    /// The Arrival working it; none, the colony's staff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Holder>,
}

impl Seat {
    pub fn worker(&self) -> Worker<'_> {
        self.held.as_ref().map_or(Worker::Staff, Worker::Arrival)
    }

    /// What the work comes out at: the staff's baseline, or what the Arrival working it makes of the
    /// job's task (`score`, 0 to 1) with their `skills`.
    pub fn quality(&self, skills: &Skills, score: f64) -> u8 {
        match self.worker() {
            Worker::Staff => STAFF_QUALITY,
            Worker::Arrival(_) => skills.quality(self.job, score),
        }
    }

    /// What selling something for `price` pays.
    pub fn pay(&self, price: u64) -> Pay {
        match self.worker() {
            Worker::Staff => Pay { wage: 0, colony: price },
            Worker::Arrival(_) => {
                let wage = price * WAGE_PCT / 100;
                Pay { wage, colony: price - wage }
            }
        }
    }

    fn lapsed(&self, now: u64) -> bool {
        self.held.as_ref().is_some_and(|h| now >= h.seen + SHIFT_LAPSE_S)
    }
}

/// The colony's seats: each job, its post, and how many.
const COLONY: [(Job, Post, u16); 9] = [
    (Job::Cook, Post::Arrival, 2),
    (Job::Barkeep, Post::Arrival, 1),
    (Job::Courier, Post::Streets, 6),
    (Job::Customs, Post::HubGate { strip: 0 }, 1),
    (Job::Customs, Post::HubGate { strip: 1 }, 1),
    (Job::Customs, Post::HubGate { strip: 2 }, 1),
    (Job::Dockmaster, Post::Dock, 1),
    (Job::Foundry, Post::Foundry, 2),
    (Job::Mechanic, Post::Bays, 3),
];

/// A colony's seats, and who's working them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seats {
    seats: Vec<Seat>,
}

impl Seats {
    /// The colony's seats, all worked by its staff.
    pub fn colony() -> Self {
        let mut seats = Vec::new();
        for (job, post, n) in COLONY {
            for _ in 0..n {
                seats.push(Seat { id: seats.len() as u16 + 1, job, post, held: None });
            }
        }
        Self { seats }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Seat> {
        self.seats.iter()
    }

    pub fn seat(&self, id: u16) -> Option<&Seat> {
        self.seats.iter().find(|s| s.id == id)
    }

    /// The seat `who` is working.
    pub fn held_by(&self, who: &str) -> Option<&Seat> {
        self.seats.iter().find(|s| s.held.as_ref().is_some_and(|h| h.who == who))
    }

    /// The first seat of `job` at `post` the staff are working.
    pub fn open(&self, job: Job, post: Post) -> Option<&Seat> {
        self.seats.iter().find(|s| s.job == job && s.post == post && s.held.is_none())
    }

    /// `who` (going by `name`) sits down in seat `id`: one the staff are working, or one whose
    /// Arrival has let their shift lapse. One seat at a time.
    pub fn take(&mut self, id: u16, who: &str, name: &str, now: u64) -> Done {
        if let Some(s) = self.held_by(who) {
            return if s.id == id {
                refuse("you're working it already")
            } else {
                refuse(format!(
                    "you're already on shift as {} at {}",
                    s.job.name().to_lowercase(),
                    s.post.name()
                ))
            };
        }
        let Some(seat) = self.seats.iter_mut().find(|s| s.id == id) else {
            return refuse("there's no such seat");
        };
        if let Some(h) = &seat.held
            && !seat.lapsed(now)
        {
            return refuse(format!("{} is working it", h.name));
        }
        seat.held = Some(Holder { who: who.into(), name: name.into(), since: now, seen: now });
        Ok(format!("ON SHIFT · {} AT {}", seat.job.tag(), seat.post.name().to_uppercase()))
    }

    /// `who` gets up from their seat, and the staff take it back.
    pub fn leave(&mut self, who: &str) -> Done {
        let Some(seat) = self.seats.iter_mut().find(|s| s.held.as_ref().is_some_and(|h| h.who == who)) else {
            return refuse("you're not on shift");
        };
        seat.held = None;
        Ok(format!("OFF SHIFT · THE COLONY'S STAFF TAKE OVER AS {}", seat.job.tag()))
    }

    /// `who` is at work in their seat; whether they hold one.
    pub fn at_work(&mut self, who: &str, now: u64) -> bool {
        match self.seats.iter_mut().find_map(|s| s.held.as_mut().filter(|h| h.who == who)) {
            Some(h) => {
                h.seen = h.seen.max(now);
                true
            }
            None => false,
        }
    }

    /// Hands back to the staff every seat whose Arrival has been away [`SHIFT_LAPSE_S`] or more:
    /// who they were.
    pub fn lapse(&mut self, now: u64) -> Vec<String> {
        let mut gone = Vec::new();
        for seat in &mut self.seats {
            if seat.lapsed(now)
                && let Some(h) = seat.held.take()
            {
                gone.push(h.who);
            }
        }
        gone
    }
}

/// What an Arrival has practised, by job: units of work done (a dish served, a delivery made).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Skills(BTreeMap<Job, u32>);

impl Skills {
    pub fn practice(&self, job: Job) -> u32 {
        self.0.get(&job).copied().unwrap_or(0)
    }

    /// Counts `units` of work done at `job`.
    pub fn practise(&mut self, job: Job, units: u32) {
        let p = self.0.entry(job).or_insert(0);
        *p = p.saturating_add(units);
    }

    /// The best their work at `job` comes out at: [`NOVICE_CEILING`] before any practice, half the
    /// way to [`MASTER_QUALITY`] after [`HALF_PRACTICE`], and nearer it ever after.
    pub fn ceiling(&self, job: Job) -> u8 {
        let p = u64::from(self.practice(job));
        let room = u64::from(MASTER_QUALITY - NOVICE_CEILING);
        NOVICE_CEILING + (room * p / (p + u64::from(HALF_PRACTICE))) as u8
    }

    /// What their work at `job` comes out at, done this well (`score`, 0 to 1): from
    /// [`NOVICE_QUALITY`] at the worst to their ceiling at the best.
    pub fn quality(&self, job: Job, score: f64) -> u8 {
        let score = if score.is_finite() { score.clamp(0.0, 1.0) } else { 0.0 };
        let (lo, hi) = (f64::from(NOVICE_QUALITY), f64::from(self.ceiling(job)));
        (lo + score * (hi - lo)).round() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::GOOD_QUALITY;

    fn count(s: &Seats, job: Job) -> usize {
        s.iter().filter(|x| x.job == job).count()
    }

    #[test]
    fn the_colony_has_its_seats_all_worked_by_its_staff() {
        let s = Seats::colony();
        assert_eq!(count(&s, Job::Cook), 2);
        assert_eq!(count(&s, Job::Barkeep), 1);
        assert_eq!(count(&s, Job::Courier), 6);
        assert_eq!(count(&s, Job::Customs), 3, "one for each strip's Hub Gate");
        assert_eq!(count(&s, Job::Dockmaster), 1);
        assert_eq!(count(&s, Job::Foundry), 2);
        assert_eq!(count(&s, Job::Mechanic), 3);
        assert!(Job::ALL.iter().all(|j| count(&s, *j) > 0), "every job has a seat");
        let mut ids: Vec<u16> = s.iter().map(|x| x.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), s.iter().count(), "ids are unique");
        assert!(s.iter().all(|x| x.worker() == Worker::Staff));
        assert_eq!(Post::HubGate { strip: 1 }.name(), "Canal's Hub Gate");
        assert_eq!(Post::HubGate { strip: 9 }.name(), "Hub Gate");
    }

    #[test]
    fn an_arrival_takes_a_seat_and_the_staff_take_it_back() {
        let mut s = Seats::colony();
        let cook = s.open(Job::Cook, Post::Arrival).unwrap().id;
        let note = s.take(cook, "a", "Alice", 100).unwrap();
        assert!(note.contains("COOK") && note.contains("THE ARRIVAL"), "{note}");
        assert!(matches!(s.seat(cook).unwrap().worker(), Worker::Arrival(h) if h.name == "Alice"));
        assert_eq!(s.held_by("a").unwrap().id, cook);
        // The Arrival's other cook's seat is still the staff's.
        assert_ne!(s.open(Job::Cook, Post::Arrival).unwrap().id, cook);
        s.leave("a").unwrap();
        assert_eq!(s.seat(cook).unwrap().worker(), Worker::Staff);
        assert!(s.leave("a").is_err(), "not on shift");
    }

    #[test]
    fn one_seat_at_a_time_and_nobody_elses_until_their_shift_lapses() {
        let mut s = Seats::colony();
        let (cook, bar) =
            (s.open(Job::Cook, Post::Arrival).unwrap().id, s.open(Job::Barkeep, Post::Arrival).unwrap().id);
        s.take(cook, "a", "Alice", 0).unwrap();
        assert!(s.take(cook, "a", "Alice", 1).is_err(), "working it already");
        assert!(s.take(bar, "a", "Alice", 1).is_err(), "one seat at a time");
        assert!(s.take(cook, "b", "Bob", 1).unwrap_err().contains("Alice"));
        // Alice keeps at it, so it stays hers.
        assert!(s.at_work("a", SHIFT_LAPSE_S - 1));
        assert!(s.take(cook, "b", "Bob", SHIFT_LAPSE_S + 1).is_err());
        assert!(s.lapse(2 * SHIFT_LAPSE_S - 2).is_empty());
        // Then she wanders off.
        assert_eq!(s.lapse(2 * SHIFT_LAPSE_S), vec!["a".to_string()]);
        assert_eq!(s.seat(cook).unwrap().worker(), Worker::Staff);
        assert!(!s.at_work("a", 2 * SHIFT_LAPSE_S));
        // A lapsed shift can be taken over without waiting for the colony to notice.
        s.take(cook, "a", "Alice", 3 * SHIFT_LAPSE_S).unwrap();
        s.take(cook, "b", "Bob", 4 * SHIFT_LAPSE_S).unwrap();
        assert!(s.held_by("a").is_none() && s.held_by("b").is_some());
        assert!(s.take(999, "c", "Cy", 0).is_err());
    }

    #[test]
    fn a_sale_pays_the_worker_and_the_colony_and_makes_nothing() {
        let mut s = Seats::colony();
        let cook = s.open(Job::Cook, Post::Arrival).unwrap().id;
        for price in [0, 1, 7, 25, 40, 999, 1_000_000] {
            let staff = s.seat(cook).unwrap().pay(price);
            assert_eq!(staff, Pay { wage: 0, colony: price }, "the staff's sales are all the colony's");
        }
        s.take(cook, "a", "Alice", 0).unwrap();
        for price in [0, 1, 7, 25, 40, 999, 1_000_000] {
            let p = s.seat(cook).unwrap().pay(price);
            assert_eq!(p.wage + p.colony, price);
            assert_eq!(p.wage, price * WAGE_PCT / 100);
        }
    }

    #[test]
    fn practice_raises_the_ceiling_and_only_the_practised_cook_well() {
        let mut k = Skills::default();
        assert_eq!(k.ceiling(Job::Cook), NOVICE_CEILING);
        assert_eq!(k.quality(Job::Cook, 0.0), NOVICE_QUALITY);
        assert_eq!(k.quality(Job::Cook, 1.0), NOVICE_CEILING);
        assert_eq!(k.quality(Job::Cook, f64::NAN), NOVICE_QUALITY);
        // A novice can't make anyone well fed, nor can the staff (`food`'s assertion).
        assert!(k.quality(Job::Cook, 1.0) < GOOD_QUALITY);
        k.practise(Job::Cook, HALF_PRACTICE);
        assert_eq!(k.ceiling(Job::Cook), (NOVICE_CEILING + MASTER_QUALITY) / 2);
        assert!(k.quality(Job::Cook, 1.0) >= GOOD_QUALITY, "a practised cook can");
        assert_eq!(k.ceiling(Job::Courier), NOVICE_CEILING, "practice is by job");
        k.practise(Job::Cook, u32::MAX);
        assert!(k.ceiling(Job::Cook) < MASTER_QUALITY + 1);
        let mut last = 0;
        for score in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let q = k.quality(Job::Cook, score);
            assert!(q >= last);
            last = q;
        }
        // A seat's quality is the staff's unless an Arrival's working it.
        let mut s = Seats::colony();
        let cook = s.open(Job::Cook, Post::Arrival).unwrap().id;
        assert_eq!(s.seat(cook).unwrap().quality(&k, 1.0), STAFF_QUALITY);
        s.take(cook, "a", "Alice", 0).unwrap();
        assert_eq!(s.seat(cook).unwrap().quality(&k, 1.0), k.quality(Job::Cook, 1.0));
    }

    #[test]
    fn seats_and_skills_keep_as_readable_json() {
        let mut s = Seats::colony();
        let cook = s.open(Job::Cook, Post::Arrival).unwrap().id;
        s.take(cook, "a", "Alice", 5).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""job":"cook""#) && json.contains(r#"{"hub_gate":{"strip":2}}"#), "{json}");
        assert_eq!(serde_json::from_str::<Seats>(&json).unwrap(), s);
        let mut k = Skills::default();
        k.practise(Job::Courier, 12);
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, r#"{"courier":12}"#);
        assert_eq!(serde_json::from_str::<Skills>(&json).unwrap(), k);
    }
}
