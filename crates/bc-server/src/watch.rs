//! The Watch as the server keeps it (`docs/FREE_ROAM.md`, 4; `docs/OPEN_CITY.md`, "Heat and the
//! Watch" and "A huge deal"): how much it wants each pilot, by their session, on the sector's tick
//! (`bc_sim::colony::watch`'s tables), and the incidents it's answering, which everyone down in the
//! town is told of. Off the sector's tick, like the plaza and the street.
//!
//! What raises a pilot's heat:
//! - **What the server sees for itself:** somebody run down (a hit the street takes,
//!   `crate::street`); driving at speed where people walk, or past them, from the poses the plaza
//!   takes ([`Watch::moved`]); and nothing cools while one of the Watch's cars on its beat has them
//!   in sight ([`Watch::tick`]).
//! - **What the client reports** ([`Watch::report`], `Request::Offence`): what only its own screen
//!   has (a car taken from a kerb or from its driver, one of the Watch's own, a knock against
//!   another car). A report can only ever add heat, the same one is taken at most once a second, and
//!   whether the Watch saw it is judged here. So is a sighting by one of the client's pursuing cars
//!   ([`Watch::sighted`]): it holds the heat where it is, and never adds or takes any.
//! - **The stop** is still the client's word (the cars after a pilot are each client's own), but
//!   the fine is of the heat held here: [`Watch::stop`] clears it and owes the fine, which the
//!   session takes ([`Watch::take_fine`]). The Watch's units, once they're the server's, stop a
//!   pilot through the same two calls.
//!
//! An incident is raised when a pilot's heat rises: one live incident an offender, moved and raised
//! with them as it rises, over [`lasts`] after its last rise or when they're stopped; and by hand
//! ([`Watch::raise`]: the dev HTTP's `/dev/incident`, for tests). The queue is a fixed size (the
//! soonest over makes room). One raised to [`NEWS_AT`] or more is news, to everyone.
//!
//! Everyone's level is also kept where the plaza reads it without a lock ([`Watch::level`]), to
//! write into each pose it relays.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use bc_econ::wire::Incident;
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_sim::TICK_HZ;
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::town;
use bc_sim::colony::watch::{self as tables, Heat, MAX_HEAT, Offence};

/// The most incidents live at once.
pub const MAX_INCIDENTS: usize = 32;
/// An incident at this level or over is news to everyone.
pub const NEWS_AT: u8 = 3;
/// The same report from a client is taken at most this often, ticks; and a sighting.
pub const REPORT_EVERY: u32 = TICK_HZ;
/// A driver's speed is measured over at least this long between poses, ticks.
const SPEED_OVER: u32 = TICK_HZ * 2 / 5;
/// A second of driving at speed where people walk is reckless, s; and so is passing this near one
/// of the town's people at speed, m.
const RECKLESS_FOR: f32 = 1.0;
const PASSING: f32 = 2.5;
/// The lines of news kept for sessions to catch up on.
const NEWS_KEPT: usize = 16;

/// How long an incident at `level` lasts after its last rise, ticks.
pub fn lasts(level: u8) -> u32 {
    (60 + 30 * u32::from(level.min(MAX_HEAT))) * TICK_HZ
}

/// One pilot the Watch knows of.
#[derive(Default)]
struct Wanted {
    heat: Heat,
    /// What raised it last.
    cause: Option<Offence>,
    /// Where they last were at the wheel, and the tick, to measure their speed by; and how long
    /// they've driven recklessly since it last counted, s.
    drive: Option<(f32, f32, u32)>,
    reckless: f32,
    /// The tick each offence (by `Offence::ALL`'s order) was last reported at, and the last sighting.
    reported: [Option<u32>; Offence::ALL.len()],
    sighted: Option<u32>,
    /// Their live incident's id.
    incident: Option<u32>,
    /// Stopped: the fine owed, of the level they were stopped at, till the session takes it.
    fine: Option<u8>,
}

#[derive(Default)]
struct Inner {
    wanted: HashMap<u16, Wanted>,
    incidents: [Option<Incident>; MAX_INCIDENTS],
    next_id: u32,
    news: VecDeque<(u64, String)>,
    news_seq: u64,
}

/// Every pilot's heat and the incidents, and a count that moves whenever the incidents change.
pub struct Watch {
    /// By session id: each one's level, for the plaza.
    levels: Box<[AtomicU8]>,
    inner: Mutex<Inner>,
    version: AtomicU64,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            levels: (0..=u16::MAX).map(|_| AtomicU8::new(0)).collect(),
            inner: Mutex::new(Inner::default()),
            version: AtomicU64::new(0),
        }
    }
}

/// What a district is called, at `s` round the ring's floor.
fn district_at(s: f32) -> &'static str {
    town::district(town::cell_at(town::town_t(s)) / town::CELLS_PER_DISTRICT).name
}

/// What happened, as the news says it.
fn what(cause: Offence, s: f32) -> String {
    let d = district_at(s);
    match cause {
        Offence::TookACar | Offence::TookACarInSight => format!("A CAR TAKEN IN {d}"),
        Offence::Jacked | Offence::JackedInSight => format!("A DRIVER PULLED FROM THEIR CAR IN {d}"),
        Offence::TookTheWatchs => format!("ONE OF THE WATCH'S OWN CARS TAKEN IN {d}"),
        Offence::Knock => format!("A CRASH IN {d}"),
        Offence::Reckless => format!("RECKLESS DRIVING IN {d}"),
        Offence::HitSomeone => format!("SOMEBODY RUN DOWN IN {d}"),
    }
}

impl Inner {
    /// `id`'s live incident, if it still is.
    fn live_of(&self, id: u32, tick: u32) -> Option<usize> {
        self.incidents.iter().position(|i| i.is_some_and(|i| i.id == id && i.until > tick))
    }

    /// Raises an incident at `(x, s)`: `level`, `cause`; `offender`'s (their session, and what
    /// they're called; 0 and none for nobody's), whose live one (`theirs`) is moved there and raised
    /// instead. News if it's come to [`NEWS_AT`] or over. Its id.
    #[allow(clippy::too_many_arguments)]
    fn raise(
        &mut self,
        (offender, name): (u16, Option<&str>),
        theirs: &mut Option<u32>,
        (x, s): (f32, f32),
        level: u8,
        cause: Offence,
        tick: u32,
    ) -> u32 {
        let level = level.clamp(1, MAX_HEAT);
        let (k, before) = match theirs.and_then(|id| self.live_of(id, tick)) {
            Some(k) => (k, self.incidents[k].map_or(0, |i| i.level)),
            None => {
                // A free place, or the one soonest over.
                let free = self.incidents.iter().position(|i| i.is_none_or(|i| i.until <= tick));
                let soonest = || {
                    let by_end =
                        self.incidents.iter().enumerate().min_by_key(|(_, i)| i.map_or(0, |i| i.until));
                    by_end.map_or(0, |(k, _)| k)
                };
                let k = free.unwrap_or_else(soonest);
                self.next_id += 1;
                let id = self.next_id;
                self.incidents[k] =
                    Some(Incident { id, x, s, level: 0, cause, at: tick, until: 0, offender });
                *theirs = Some(id);
                (k, 0)
            }
        };
        let Some(i) = self.incidents[k].as_mut() else { return 0 };
        let level = level.max(i.level);
        *i = Incident { x, s, level, cause, until: tick + lasts(level), ..*i };
        let id = i.id;
        if level >= NEWS_AT && level > before {
            let stars = "◆".repeat(usize::from(level));
            let text = match name {
                Some(n) => format!("THE WATCH WANTS {} · {} · {stars}", n.to_uppercase(), what(cause, s)),
                None => format!("THE WATCH IS CALLED OUT · {} · {stars}", what(cause, s)),
            };
            self.news_seq += 1;
            if self.news.len() >= NEWS_KEPT {
                self.news.pop_front();
            }
            self.news.push_back((self.news_seq, text));
        }
        id
    }
}

impl Watch {
    /// How much the Watch wants session `id`, 0 to 5 (no lock: the plaza reads it for every pose).
    pub fn level(&self, id: u16) -> u8 {
        self.levels[usize::from(id)].load(Ordering::Relaxed)
    }

    /// `id`'s level, and what last raised it.
    pub fn heat(&self, id: u16) -> (u8, Option<Offence>) {
        let Ok(inner) = self.inner.lock() else { return (0, None) };
        inner.wanted.get(&id).map_or((0, None), |w| (w.heat.level(), w.cause))
    }

    /// Runs `f` on `id`'s record, then keeps their level where the plaza reads it, and bumps the
    /// count if the incidents changed.
    fn with<T>(&self, id: u16, f: impl FnOnce(&mut Inner, &mut Wanted) -> T) -> Option<T> {
        let mut inner = self.inner.lock().ok()?;
        let mut w = inner.wanted.remove(&id).unwrap_or_default();
        let before = inner.incidents;
        let out = f(&mut inner, &mut w);
        self.levels[usize::from(id)].store(w.heat.level(), Ordering::Relaxed);
        inner.wanted.insert(id, w);
        if inner.incidents != before {
            self.version.fetch_add(1, Ordering::Release);
        }
        Some(out)
    }

    /// `id` (called `name`) did `o` at `(x, s)` on the ring's floor at tick `tick`: their heat goes
    /// up, and their incident with it if it rose a level. True if it did.
    pub fn offend(&self, id: u16, name: &str, o: Offence, at: (f32, f32), tick: u32) -> bool {
        self.with(id, |inner, w| {
            let rose = w.heat.offend(o, tick);
            if rose {
                w.cause = Some(o);
                inner.raise((id, Some(name)), &mut w.incident, at, w.heat.level(), o, tick);
            }
            rose
        })
        .unwrap_or(false)
    }

    /// The client of `id` says they did `o` at `at`: taken if it's one a client may report
    /// (`Offence::reported`) and they haven't said the same within [`REPORT_EVERY`]; worse if one of
    /// the Watch's cars on its beat saw it. True if their heat rose a level.
    pub fn report(&self, id: u16, name: &str, o: Offence, at: (f32, f32), tick: u32) -> bool {
        if !o.reported() {
            return false;
        }
        let k = Offence::ALL.iter().position(|a| *a == o).unwrap_or(0);
        let taken = self
            .with(id, |_, w| {
                let fresh = w.reported[k].is_none_or(|t| tick.saturating_sub(t) >= REPORT_EVERY);
                if fresh {
                    w.reported[k] = Some(tick);
                }
                fresh
            })
            .unwrap_or(false);
        if !taken {
            return false;
        }
        let seen = tables::car_sees(at.0, at.1, tables::SIGHT, tick, 0.0);
        self.offend(id, name, if seen { o.in_sight() } else { o }, at, tick)
    }

    /// One of the cars after `id` has them in sight (their client says, at most once in
    /// [`REPORT_EVERY`]): nothing cools while it does.
    pub fn sighted(&self, id: u16, tick: u32) {
        self.with(id, |_, w| {
            if w.sighted.is_none_or(|t| tick.saturating_sub(t) >= REPORT_EVERY) {
                w.sighted = Some(tick);
                w.heat.hold(tick);
            }
        });
    }

    /// `id` (called `name`) was at `pose` in the town at tick `tick` (a pose the plaza took): at the
    /// wheel faster than [`tables::RECKLESS_SPEED`] where people walk, or past one of the town's
    /// people, each second of it is reckless. True if their heat rose a level.
    pub fn moved(&self, id: u16, name: &str, pose: &PersonPose, tick: u32) -> bool {
        if pose.strip != RING_DECK {
            return false;
        }
        let reckless = self
            .with(id, |_, w| {
                if !pose.driving() {
                    w.drive = None;
                    return false;
                }
                let Some((x, s, t0)) = w.drive else {
                    w.drive = Some((pose.x, pose.s, tick));
                    return false;
                };
                let dt = tick.saturating_sub(t0);
                if dt < SPEED_OVER {
                    return false;
                }
                w.drive = Some((pose.x, pose.s, tick));
                let secs = dt as f32 / TICK_HZ as f32;
                let speed = (pose.x - x).hypot(round_from(RING, s, pose.s)) / secs;
                let fast = speed > tables::RECKLESS_SPEED;
                if fast
                    && (tables::where_people_walk(pose.x, pose.s)
                        || tables::person_near(pose.x, pose.s, PASSING, tick, &[]))
                {
                    // (A long gap between poses counts a second at most.)
                    w.reckless += secs.min(RECKLESS_FOR);
                }
                let counts = w.reckless >= RECKLESS_FOR;
                if counts {
                    w.reckless = 0.0;
                }
                counts
            })
            .unwrap_or(false);
        reckless && self.offend(id, name, Offence::Reckless, (pose.x, pose.s), tick)
    }

    /// `id`'s session's tick, at tick `tick`, where the plaza last had them in the town (if they
    /// are): held under the eye of one of the Watch's cars on its beat while they're wanted, and
    /// cooled. True if it fell.
    pub fn tick(&self, id: u16, at: Option<(f32, f32)>, tick: u32) -> bool {
        // (Asked only of the wanted: the beats are a closed form, but no need to look.)
        let hot = self.level(id) > 0;
        let seen = hot && at.is_some_and(|(x, s)| tables::car_sees(x, s, tables::SIGHT, tick, 0.0));
        self.with(id, |_, w| {
            if seen {
                w.heat.hold(tick);
            }
            let fell = w.heat.cool(tick);
            if w.heat.level() == 0 {
                w.cause = None;
            }
            fell
        })
        .unwrap_or(false)
    }

    /// The Watch has `id`, stopped, at tick `tick`: their heat is cleared, their incident over, and
    /// the fine of the level they were at owed ([`Watch::take_fine`]). That level; none if the Watch
    /// had nothing on them.
    pub fn stop(&self, id: u16, tick: u32) -> Option<u8> {
        self.with(id, |inner, w| {
            w.heat.cool(tick);
            let level = w.heat.level();
            if level == 0 {
                return None;
            }
            w.heat.clear();
            w.cause = None;
            w.reckless = 0.0;
            if let Some(k) = w.incident.take().and_then(|i| inner.live_of(i, tick)) {
                inner.incidents[k] = None;
            }
            w.fine = Some(w.fine.map_or(level, |f| f.max(level)));
            Some(level)
        })
        .flatten()
    }

    /// The fine `id` owes for a stop (the level they were stopped at), once.
    pub fn take_fine(&self, id: u16) -> Option<u8> {
        self.with(id, |_, w| w.fine.take()).flatten()
    }

    /// `id`'s session is over: the Watch forgets them (their incident runs its time).
    pub fn leave(&self, id: u16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.wanted.remove(&id);
        }
        self.levels[usize::from(id)].store(0, Ordering::Relaxed);
    }

    /// An incident raised by hand at `(x, s)` on the ring's floor (a test hook, the dev HTTP's
    /// `/dev/incident`): `level` (1 to 5), `cause`, nobody's. Its id.
    pub fn raise(&self, at: (f32, f32), level: u8, cause: Offence, tick: u32) -> u32 {
        let Ok(mut inner) = self.inner.lock() else { return 0 };
        let id = inner.raise((0, None), &mut None, at, level, cause, tick);
        self.version.fetch_add(1, Ordering::Release);
        id
    }

    /// The incidents live at tick `tick`, oldest first (those over go).
    pub fn incidents(&self, tick: u32) -> Vec<Incident> {
        let Ok(mut inner) = self.inner.lock() else { return Vec::new() };
        let mut gone = false;
        for i in inner.incidents.iter_mut() {
            if i.is_some_and(|i| i.until <= tick) {
                *i = None;
                gone = true;
            }
        }
        if gone {
            self.version.fetch_add(1, Ordering::Release);
        }
        let mut live: Vec<Incident> = inner.incidents.iter().flatten().copied().collect();
        live.sort_by_key(|i| i.id);
        live
    }

    /// How many incidents are live at tick `tick` (for `/status`).
    pub fn live(&self, tick: u32) -> usize {
        self.inner
            .lock()
            .map_or(0, |inner| inner.incidents.iter().flatten().filter(|i| i.until > tick).count())
    }

    /// Moves whenever the incidents change.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// The newest line of news's number.
    pub fn news_seq(&self) -> u64 {
        self.inner.lock().map_or(0, |inner| inner.news_seq)
    }

    /// The news after line `seq` (as many as are kept), and the newest's number.
    pub fn news_since(&self, seq: u64) -> (u64, Vec<String>) {
        let Ok(inner) = self.inner.lock() else { return (seq, Vec::new()) };
        let lines = inner.news.iter().filter(|(n, _)| *n > seq).map(|(_, t)| t.clone()).collect();
        (inner.news_seq, lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::presence::RIDE_CAR;
    use bc_sim::colony::watch::COOL_TICKS;

    /// At the wheel at `(x, s)` on the ring's floor.
    fn driving(x: f32, s: f32) -> PersonPose {
        PersonPose { strip: RING_DECK, x, s, ride: RIDE_CAR, grounded: true, ..PersonPose::default() }
    }

    #[test]
    fn heat_rises_with_an_incident_that_follows_it_and_falls_with_time() {
        let w = Watch::default();
        let at = (20.0, town::deck_s(town::CELL * 20.5));
        assert_eq!((w.level(1), w.version(), w.incidents(0).len()), (0, 0, 0));
        // Somebody run down: level 2 at once, and an incident there.
        assert!(w.offend(1, "Maren", Offence::HitSomeone, at, 100));
        assert_eq!(w.level(1), 2);
        assert_eq!(w.heat(1), (2, Some(Offence::HitSomeone)));
        let live = w.incidents(100);
        assert_eq!(live.len(), 1);
        let first = live[0];
        assert_eq!(
            (first.level, first.offender, first.cause, first.until),
            (2, 1, Offence::HitSomeone, 100 + lasts(2))
        );
        // Again, further on: the same incident, moved and raised; and at 3 it's news.
        let (_, before) = w.news_since(0);
        assert!(before.is_empty());
        let on = (at.0, at.1 + 40.0);
        assert!(w.offend(1, "Maren", Offence::HitSomeone, on, 130));
        let live = w.incidents(130);
        assert_eq!(live.len(), 1, "{live:?}");
        assert_eq!((live[0].id, live[0].level, live[0].s), (first.id, 3, on.1));
        let (seq, news) = w.news_since(0);
        assert_eq!(news.len(), 1);
        assert!(
            news[0].starts_with("THE WATCH WANTS MAREN · SOMEBODY RUN DOWN IN ")
                && news[0].ends_with(" · ◆◆◆"),
            "{}",
            news[0]
        );
        assert_eq!(w.news_since(seq).1.len(), 0);
        // Somebody else's is their own.
        assert!(w.offend(2, "Pagan", Offence::TookTheWatchs, at, 140));
        assert_eq!(w.incidents(140).len(), 2);
        assert_eq!(w.level(2), 3);
        // Quiet, it cools a level a while; the incident stays its time.
        let v = w.version();
        assert!(!w.tick(1, None, 130 + COOL_TICKS - 1));
        assert!(w.tick(1, None, 130 + COOL_TICKS));
        assert_eq!(w.level(1), 2);
        assert_eq!(w.incidents(130 + COOL_TICKS)[0].level, 3);
        assert_eq!(w.version(), v);
        // Over in time: gone, and the count's moved.
        assert!(w.incidents(130 + lasts(3)).iter().all(|i| i.offender != 1));
        assert!(w.version() > v);
        // Forgotten when they go.
        w.leave(2);
        assert_eq!((w.level(2), w.heat(2)), (0, (0, None)));
    }

    #[test]
    fn a_report_only_adds_once_a_second_and_never_a_hit() {
        let w = Watch::default();
        let at = (20.0, town::deck_s(town::CELL * 20.5));
        // A hit or reckless driving is the server's to see, not the client's to say.
        assert!(!w.report(1, "Maren", Offence::HitSomeone, at, 0));
        assert!(!w.report(1, "Maren", Offence::Reckless, at, 0));
        assert!(!w.report(1, "Maren", Offence::TookACarInSight, at, 0));
        assert_eq!(w.level(1), 0);
        // A knock, said over and over: once a second.
        assert!(w.report(1, "Maren", Offence::Knock, at, 10));
        for t in 11..10 + REPORT_EVERY {
            assert!(!w.report(1, "Maren", Offence::Knock, at, t));
        }
        let (level, points) = (w.level(1), w.with(1, |_, w| w.heat.points()).unwrap());
        assert_eq!((level, points), (1, 1.0));
        assert!(
            !w.report(1, "Maren", Offence::Knock, at, 10 + REPORT_EVERY),
            "1 to 2 points: still the first"
        );
        assert_eq!(w.level(1), 1);
        // A sighting holds it, and adds nothing.
        let hold = 10 + COOL_TICKS - 5;
        w.sighted(1, hold);
        assert!(!w.tick(1, None, 10 + REPORT_EVERY + COOL_TICKS - 1));
        assert_eq!(w.level(1), 1);
        assert!(w.tick(1, None, hold + COOL_TICKS));
        assert_eq!(w.level(1), 0);
    }

    #[test]
    fn driving_fast_where_people_walk_is_reckless_and_on_the_road_isnt() {
        let w = Watch::default();
        // On the boulevard's carriageway at 20 m/s: nothing, however long.
        let s0 = town::deck_s(town::CELL * 20.5);
        for k in 0..20u32 {
            assert!(!w.moved(1, "Maren", &driving(20.0, s0 + 10.0 * k as f32), k * 15));
        }
        assert_eq!(w.level(1), 0);
        // Across a block at 15 m/s, a pose every half second: each second of it counts.
        let (x, s1) = (-300.0, town::deck_s(town::CELL * 30.2));
        assert!(tables::where_people_walk(x, s1) && tables::where_people_walk(x, s1 + 67.5));
        let mut rose = 0;
        for k in 0..10u32 {
            rose += u32::from(w.moved(2, "Pagan", &driving(x, s1 + 7.5 * k as f32), 1_000 + k * 15));
        }
        // Nine half-second steps: four seconds and a half, four offences of 0.8 (risen at 1.6 and 3.2).
        assert_eq!((rose, w.level(2)), (2, 2), "{:?}", w.heat(2));
        assert_eq!(w.heat(2).1, Some(Offence::Reckless));
        // Slow there, nothing; on foot, nothing.
        let mut slow = 0;
        for k in 0..10u32 {
            slow += u32::from(w.moved(3, "Ines", &driving(x, s1 + 1.5 * k as f32), 1_000 + k * 15));
        }
        assert_eq!((slow, w.level(3)), (0, 0));
    }

    #[test]
    fn a_stop_clears_the_heat_ends_the_incident_and_owes_its_fine_once() {
        let w = Watch::default();
        let at = (20.0, town::deck_s(town::CELL * 20.5));
        assert_eq!(w.stop(1, 0), None, "nothing on them");
        assert_eq!(w.take_fine(1), None);
        w.offend(1, "Maren", Offence::TookTheWatchs, at, 0);
        assert_eq!(w.level(1), 3);
        assert_eq!(w.incidents(5).len(), 1);
        assert_eq!(w.stop(1, 5), Some(3));
        assert_eq!((w.level(1), w.heat(1)), (0, (0, None)));
        assert!(w.incidents(5).is_empty(), "the Watch has them");
        assert_eq!(w.take_fine(1), Some(3));
        assert_eq!(w.take_fine(1), None, "once");
        // Cooled away before the stop, there's nothing to stop them for.
        w.offend(1, "Maren", Offence::Knock, at, 10);
        assert_eq!(w.stop(1, 10 + COOL_TICKS), None);
    }

    #[test]
    fn the_queue_is_fixed_and_an_incident_by_hand_is_nobodys() {
        let w = Watch::default();
        let at = (20.0, town::deck_s(town::CELL * 20.5));
        let id = w.raise(at, 9, Offence::Reckless, 50);
        let live = w.incidents(50);
        assert_eq!((live.len(), live[0].id, live[0].level, live[0].offender), (1, id, MAX_HEAT, 0));
        assert!(w.news_since(0).1[0].starts_with("THE WATCH IS CALLED OUT · RECKLESS DRIVING IN "));
        for k in 0..(MAX_INCIDENTS as u32 + 10) {
            w.raise(at, 1, Offence::Knock, 60 + k);
        }
        let live = w.incidents(100);
        assert_eq!(live.len(), MAX_INCIDENTS);
        assert!(live.iter().any(|i| i.id == id), "the one longest to run is kept");
        assert_eq!(w.live(100), MAX_INCIDENTS);
        assert_eq!(w.news_since(0).1.len(), 1, "a level 1 is no news");
    }
}
