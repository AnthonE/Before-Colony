//! The Watch's units, sent by road (`docs/FREE_ROAM.md`, 4, W1): every district's station keeps its
//! cars and its Sirius hounds (`bc_sim::colony::watch::UnitKind`), and the server sends them.
//! Off the sector's tick, like the Watch's heat (`crate::watch`): a task steps them ten times a
//! second, on the sector's clock, in fixed arrays sized at start-up.
//!
//! - **An incident** (`Watch`'s queue: an offender's, or one raised by hand) is answered as its
//!   level says (`watch::answer`): the nearest free units by road, each station's door routed to
//!   the place (`roads::route`; the shortest route wins), from as many of the nearest stations as
//!   the level calls on. Each drives its route at its kind's speed (by the level's haste) to its
//!   place in the cordon round the incident, holds it for the level's time, drives back to its
//!   station and is free again. An incident risen a level is answered again, with what the new
//!   level sends more.
//! - **A wanted pilot** in the town (`Watch::wanted`) is chased: the units sent after them find
//!   the way to their last pose again every [`tables::REROUTE`], and while there are fewer after
//!   them than their level sends, more come. Held slow with one of them within its reach (or, where
//!   none can come to their side, near enough for its officers to walk over), they're stopped:
//!   [`Watch::stop`] clears the heat and owes its fine, which their session takes. No word from
//!   their client is needed for any of it. A unit near them has them in sight, so nothing cools.
//! - **Every unit keeps to the roads**: it drives only the legs of its routes, each along a road or
//!   across the ring on a cross street, from the road in front of its station's door.
//!
//! Sessions down in the town are sent the units near them ([`Dispatch::near`], `Update::Units`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::wire::{self, UnitState};
use bc_sim::TICK_HZ;
use bc_sim::colony::roads::{self, MOST, Waypoint};
use bc_sim::colony::street;
use bc_sim::colony::town::{self, DISTRICT_COUNT, ROUND};
use bc_sim::colony::watch::{self as tables, AFOOT, Answer, CATCH, STATION_UNITS, UNIT_SIGHT, UnitKind};

use crate::watch::{MAX_INCIDENTS, Sought, Watch};

/// The Watch's stations: one a district.
pub const STATIONS: usize = DISTRICT_COUNT as usize;
/// Every unit the Watch has.
pub const MAX_UNITS: usize = STATIONS * STATION_UNITS;
/// The most wanted pilots chased at once.
pub const MAX_CHASED: usize = 64;
/// How often the units are stepped.
pub const STEP: std::time::Duration = std::time::Duration::from_millis(100);
/// A session down in the town is sent the units within this far of it, m, at most this many
/// (nearest first), this often.
pub const SEND_NEAR: f32 = 1_200.0;
pub const SEND_MOST: usize = 24;
pub const SEND_EVERY: std::time::Duration = std::time::Duration::from_millis(300);
/// The most a step moves them for, s (the clock held up: they don't jump).
const MOST_DT: f32 = 0.5;
/// A pilot's speed is measured between poses this far apart, ticks.
const SPEED_OVER: u32 = TICK_HZ / 2;
/// A chase short of units is topped up at most this often, ticks.
const TOP_UP_EVERY: u32 = 2 * TICK_HZ;
/// A unit after a pilot finds the way again early once they're this far from where it's going, m.
const STRAYED: f32 = 25.0;
/// Where the units after one pilot each make for, round them (by their place in the chase), m
/// round the ring: so they don't stand on each other.
const CHASE_SPREAD: [f32; 4] = [0.0, 7.0, -7.0, 14.0];
/// How long the heat of a pilot holds while the units sent after them are on their way and none
/// has had them in sight yet, ticks (else a first level cools before any could come).
const ON_THE_WAY: u32 = 90 * TICK_HZ;
/// The Watch's cars on patrol in the traffic (`street::Kind::Watch`) this near an incident or a
/// wanted pilot, m, answer it before any from a station: at most this many looked at.
pub const PATROL_REACH: f32 = 700.0;
const MOST_PATROLS: usize = 6;

/// Units sent out of one station together pull out this far apart, s (else they drive as one).
const PULL_OUT: f32 = 1.5;

/// What a unit is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// In its station, free.
    Home,
    Driving,
    Cordon,
    Returning,
}

/// One of the Watch's units.
#[derive(Clone, Copy, Debug)]
struct Unit {
    kind: UnitKind,
    station: u16,
    state: State,
    /// Where it is on the town's floor: across, round from Hub Gate (in its route's turn), and the
    /// way it faces (the walker's yaw).
    x: f32,
    t: f32,
    yaw: f32,
    route: [Waypoint; MOST],
    len: usize,
    /// The waypoint it's making for.
    next: usize,
    /// What it was sent to: the incident (0: none, a chase), the pilot it's after (0: nobody), the
    /// level it answers, and its place among the units sent.
    incident: u32,
    after: u16,
    level: u8,
    slot: u8,
    /// Where it's making for, `(x, t)`, and the tick it last found the way.
    goal: (f32, f32),
    routed: u32,
    /// Holding its cordon till this tick.
    until: u32,
    /// Still at its station's door this long before it pulls out, s.
    wait: f32,
    /// The car of the traffic it was on patrol as, promoted to answer (0: out of its station).
    patrol: u32,
}

impl Unit {
    fn at_home(kind: UnitKind, station: u16) -> Self {
        let (x, t) = town::watch_door(station);
        Self {
            kind,
            station,
            state: State::Home,
            x,
            t,
            yaw: 0.0,
            route: [Waypoint::default(); MOST],
            len: 0,
            next: 0,
            incident: 0,
            after: 0,
            level: 0,
            slot: 0,
            goal: (x, t),
            routed: 0,
            until: 0,
            wait: 0.0,
            patrol: 0,
        }
    }

    fn out(&self) -> bool {
        self.state != State::Home
    }

    /// After pilot `id`, and not on its way home.
    fn chasing(&self, id: u16) -> bool {
        self.after == id && matches!(self.state, State::Driving | State::Cordon)
    }

    fn speed(&self) -> f32 {
        self.kind.speed() * tables::answer(self.level.max(1)).haste
    }

    fn arrived(&self) -> bool {
        self.next >= self.len
    }

    /// The way from where it is to `goal`, at tick `tick`: false (and the old way kept) if there's
    /// none.
    fn route_to(&mut self, goal: (f32, f32), tick: u32) -> bool {
        // (Brought into the ring's first turn first: the route comes out in the turn it sets out in.)
        let from = (self.x, town::wrap(self.t));
        let mut way = [Waypoint::default(); MOST];
        let n = roads::route(from, goal, &mut way).min(MOST);
        self.goal = goal;
        self.routed = tick;
        if n == 0 {
            return false;
        }
        self.t = from.1;
        self.route = way;
        self.len = n;
        self.next = 0;
        true
    }

    /// `dt` s along its route.
    fn drive(&mut self, dt: f32) {
        let waited = self.wait.min(dt);
        self.wait -= waited;
        let mut left = self.speed() * (dt - waited);
        while left > 0.0 && self.next < self.len {
            let w = self.route[self.next];
            let (dx, dt) = (w.x - self.x, w.t - self.t);
            let d = dx.hypot(dt);
            if d > 1e-4 {
                // Forward is `(sin yaw, −cos yaw)` in `(x, s)`.
                self.yaw = dx.atan2(-dt);
            }
            if d <= left {
                (self.x, self.t) = (w.x, w.t);
                left -= d;
                self.next += 1;
            } else {
                self.x += dx / d * left;
                self.t += dt / d * left;
                left = 0.0;
            }
        }
    }
}

/// A wanted pilot the units are after.
#[derive(Clone, Copy, Debug)]
struct Chase {
    id: u16,
    level: u8,
    /// Where they are, `(x, t)`.
    at: (f32, f32),
    /// Where they were when their speed was last measured, and the tick; their speed then (none
    /// yet: not known to be slow).
    sample: (f32, f32, u32),
    speed: Option<f32>,
    /// How long they've been held slow with a unit near, s.
    held: f32,
    /// The tick more units were last sent after them.
    topped: Option<u32>,
    /// The tick the chase began, and whether a unit has had them in sight since.
    since: u32,
    met: bool,
}

/// How far apart two places on the town's floor are, `(x, t)`, m (round the ring the short way).
fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dt = b.1 - a.1;
    (b.0 - a.0).hypot(dt - (dt / ROUND).round() * ROUND)
}

/// A unit as the dispatcher has it, for the tests and `/status`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub id: u16,
    pub kind: UnitKind,
    pub station: u16,
    pub state: State,
    /// On the town's floor: across, round from Hub Gate (wrapped), and its heading.
    pub x: f32,
    pub t: f32,
    pub yaw: f32,
    pub incident: u32,
    pub after: u16,
    pub level: u8,
    /// Where it's making for, and how far that is along its route, m.
    pub goal: (f32, f32),
    pub to_go: f32,
    pub until: u32,
    /// Still at its door this long before it pulls out, s.
    pub wait: f32,
    /// The car of the traffic it was on patrol as (0: it came out of its station).
    pub patrol: u32,
}

struct Inner {
    units: Box<[Unit; MAX_UNITS]>,
    chases: [Option<Chase>; MAX_CHASED],
    /// The incidents answered, and the level each was answered at.
    answered: [Option<(u32, u8)>; MAX_INCIDENTS],
    sought: [Sought; MAX_CHASED],
    /// The tick of the last step.
    tick: Option<u32>,
    /// The cars of the traffic promoted since the step's caller last took them: each car's id and
    /// where it was (the deck's `s`), for the street's taken set.
    promoted: Vec<(u32, f32)>,
}

/// Every unit of the Watch's, and who they're after.
pub struct Dispatch {
    inner: Mutex<Inner>,
    /// Pilots stopped by the units, ever.
    stops: AtomicU64,
}

impl Default for Dispatch {
    fn default() -> Self {
        let units = Box::new(core::array::from_fn(|k| {
            let station = (k / STATION_UNITS) as u16;
            let kind = if k % STATION_UNITS < usize::from(UnitKind::Car.kept()) {
                UnitKind::Car
            } else {
                UnitKind::Hound
            };
            Unit::at_home(kind, station)
        }));
        Self {
            inner: Mutex::new(Inner {
                units,
                chases: [None; MAX_CHASED],
                answered: [None; MAX_INCIDENTS],
                sought: [Sought::default(); MAX_CHASED],
                tick: None,
                promoted: Vec::new(),
            }),
            stops: AtomicU64::new(0),
        }
    }
}

impl Inner {
    /// Sends what `answer` sends to `goal` (`(x, t)`): free units from the nearest of its stations
    /// by road, each to its own place in the cordon round it (after `after`: their own places
    /// round the pilot). `have` of each kind already answer it. How many went.
    fn send(
        &mut self,
        answer: Answer,
        have: (u8, u8),
        goal: (f32, f32),
        (incident, after, level): (u32, u16, u8),
        tick: u32,
    ) -> usize {
        let want = [answer.cars.saturating_sub(have.0), answer.hounds.saturating_sub(have.1)];
        if want == [0, 0] {
            return 0;
        }
        // The stations, nearest by road first.
        let mut by_road = [(f32::INFINITY, 0u16); STATIONS];
        for (d, r) in by_road.iter_mut().enumerate() {
            let mut way = [Waypoint::default(); MOST];
            let n = roads::route(town::watch_door(d as u16), goal, &mut way).min(MOST);
            if n > 0 {
                *r = (roads::length(&way[..n]), d as u16);
            }
        }
        by_road.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let total = u32::from(answer.cars) + u32::from(answer.hounds);
        let mut slot = u32::from(have.0) + u32::from(have.1);
        let mut sent = 0;
        // How many each station has sent so far: each pulls out after the one before.
        let mut pulled = [0u8; STATIONS];
        // The Watch's own cars on patrol in the traffic near it answer first: promoted where they
        // are, each into a free car of the nearest station that has one.
        let mut patrols = [(f32::INFINITY, None::<street::Car>); MOST_PATROLS];
        street::each_car(
            goal.0 - PATROL_REACH,
            goal.0 + PATROL_REACH,
            goal.1 - PATROL_REACH,
            goal.1 + PATROL_REACH,
            tick,
            0.0,
            &mut |c| {
                let far = apart((c.x, c.t), goal);
                if c.kind != street::Kind::Watch
                    || far > PATROL_REACH
                    || self.units.iter().any(|u| u.patrol == c.id)
                {
                    return;
                }
                if let Some(worst) = patrols.iter_mut().max_by(|a, b| a.0.total_cmp(&b.0))
                    && far < worst.0
                {
                    *worst = (far, Some(*c));
                }
            },
        );
        patrols.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut left_cars = want[0];
        for c in patrols.iter().filter_map(|p| p.1) {
            if left_cars == 0 {
                break;
            }
            let free = by_road.iter().filter(|r| r.0.is_finite()).find_map(|&(_, station)| {
                let base = usize::from(station) * STATION_UNITS;
                (base..base + STATION_UNITS)
                    .find(|&k| self.units[k].kind == UnitKind::Car && !self.units[k].out())
            });
            let Some(k) = free else { break };
            let place = if after != 0 {
                (goal.0, goal.1 + CHASE_SPREAD[slot as usize % CHASE_SPREAD.len()])
            } else {
                let a = std::f32::consts::TAU * slot as f32 / total.max(1) as f32;
                (goal.0 + answer.cordon * a.cos(), goal.1 + answer.cordon * a.sin())
            };
            let u = &mut self.units[k];
            *u = Unit::at_home(UnitKind::Car, u.station);
            (u.x, u.t, u.yaw) = (c.x, c.t, c.yaw);
            if !u.route_to(place, tick) {
                *u = Unit::at_home(UnitKind::Car, u.station);
                continue;
            }
            u.state = State::Driving;
            (u.incident, u.after, u.level, u.slot) = (incident, after, level, slot as u8);
            u.patrol = c.id;
            self.promoted.push((c.id, town::deck_s(c.t)));
            slot += 1;
            left_cars -= 1;
            sent += 1;
        }
        let want = [left_cars, want[1]];
        for (k, kind) in UnitKind::ALL.into_iter().enumerate() {
            let mut left = want[k];
            for &(far, station) in by_road.iter().take(usize::from(answer.stations)) {
                if left == 0 || !far.is_finite() {
                    break;
                }
                let base = usize::from(station) * STATION_UNITS;
                for u in self.units[base..base + STATION_UNITS].iter_mut() {
                    if left == 0 {
                        break;
                    }
                    if u.kind != kind || u.out() {
                        continue;
                    }
                    let place = if after != 0 {
                        (goal.0, goal.1 + CHASE_SPREAD[slot as usize % CHASE_SPREAD.len()])
                    } else {
                        let a = std::f32::consts::TAU * slot as f32 / total.max(1) as f32;
                        (goal.0 + answer.cordon * a.cos(), goal.1 + answer.cordon * a.sin())
                    };
                    *u = Unit::at_home(kind, station);
                    if !u.route_to(place, tick) {
                        continue;
                    }
                    // Out of the station's door onto the road in front of it.
                    (u.x, u.t) = (u.route[0].x, u.route[0].t);
                    u.state = State::Driving;
                    (u.incident, u.after, u.level, u.slot) = (incident, after, level, slot as u8);
                    let p = &mut pulled[usize::from(station)];
                    u.wait = PULL_OUT * f32::from(*p);
                    *p = p.saturating_add(1);
                    slot += 1;
                    left -= 1;
                    sent += 1;
                }
            }
        }
        sent
    }

    /// Units of each kind after pilot `id`.
    fn after(&self, id: u16) -> (u8, u8) {
        let mut n = (0u8, 0u8);
        for u in self.units.iter().filter(|u| u.chasing(id)) {
            match u.kind {
                UnitKind::Car => n.0 += 1,
                UnitKind::Hound => n.1 += 1,
            }
        }
        n
    }
}

impl Dispatch {
    /// The units on the sector's tick `tick`: incidents answered, wanted pilots chased (and
    /// stopped), and every unit moved on.
    pub fn step(&self, watch: &Watch, tick: u32) {
        self.step_and(watch, tick, &mut |_, _| {});
    }

    /// [`Dispatch::step`], telling `promoted` of each car of the traffic promoted to answer (its
    /// id, and where it was: the deck's `s`): gone from the traffic for everyone while it's out.
    pub fn step_and(&self, watch: &Watch, tick: u32, promoted: &mut impl FnMut(u32, f32)) {
        self.step_inner(watch, tick);
        let taken = self.inner.lock().map(|mut i| std::mem::take(&mut i.promoted)).unwrap_or_default();
        for (car, s) in taken {
            promoted(car, s);
        }
    }

    fn step_inner(&self, watch: &Watch, tick: u32) {
        let Ok(mut guard) = self.inner.lock() else { return };
        let inner = &mut *guard;
        let dt = match inner.tick {
            Some(t) if tick > t => ((tick - t) as f32 / TICK_HZ as f32).min(MOST_DT),
            Some(_) => return,
            None => 0.0,
        };
        inner.tick = Some(tick);

        // The incidents: each answered as its level says, once a level.
        let mut live = [None; MAX_INCIDENTS];
        let mut n = 0;
        watch.each_incident(tick, |i| {
            if n < live.len() {
                live[n] = Some(*i);
                n += 1;
            }
        });
        for a in inner.answered.iter_mut() {
            if a.is_some_and(|(id, _)| !live.iter().flatten().any(|i| i.id == id)) {
                *a = None;
            }
        }
        for i in live.iter().flatten() {
            let k = inner.answered.iter().position(|a| a.is_some_and(|(id, _)| id == i.id));
            let before = k.and_then(|k| inner.answered[k]).map_or(0, |(_, l)| l);
            if i.level <= before {
                continue;
            }
            let (was, now) = (tables::answer(before), tables::answer(i.level));
            let goal = (i.x, town::town_t(i.s));
            inner.send(now, (was.cars, was.hounds), goal, (i.id, i.offender, i.level), tick);
            let k = k.or_else(|| inner.answered.iter().position(Option::is_none));
            if let Some(k) = k {
                inner.answered[k] = Some((i.id, i.level));
            }
        }

        // Who's wanted in the town: chased, and more sent after them while they're short.
        let n = watch.wanted(&mut inner.sought);
        let sought = inner.sought;
        let sought = &sought[..n];
        for c in inner.chases.iter_mut() {
            if c.is_some_and(|c| !sought.iter().any(|s| s.id == c.id)) {
                *c = None;
            }
        }
        for s in sought {
            let at = (s.x, town::town_t(s.s));
            let k = match inner.chases.iter().position(|c| c.is_some_and(|c| c.id == s.id)) {
                Some(k) => k,
                None => {
                    let Some(k) = inner.chases.iter().position(Option::is_none) else { continue };
                    inner.chases[k] = Some(Chase {
                        id: s.id,
                        level: s.level,
                        at,
                        sample: (at.0, at.1, tick),
                        speed: None,
                        held: 0.0,
                        topped: None,
                        since: tick,
                        met: false,
                    });
                    k
                }
            };
            let Some(c) = inner.chases[k].as_mut() else { continue };
            (c.level, c.at) = (s.level, at);
            let over = tick.saturating_sub(c.sample.2);
            if over >= SPEED_OVER {
                c.speed = Some(apart((c.sample.0, c.sample.1), at) / (over as f32 / TICK_HZ as f32));
                c.sample = (at.0, at.1, tick);
            }
            let due = c.topped.is_none_or(|t| tick.saturating_sub(t) >= TOP_UP_EVERY);
            let have = inner.after(s.id);
            let answer = tables::answer(s.level);
            if due && (have.0 < answer.cars || have.1 < answer.hounds) {
                if let Some(c) = inner.chases[k].as_mut() {
                    c.topped = Some(tick);
                }
                inner.send(answer, have, at, (0, s.id, s.level), tick);
            }
        }

        // Every unit on: after its pilot, along its route, holding its cordon, or home.
        let reroute = (tables::REROUTE * TICK_HZ as f32) as u32;
        for u in inner.units.iter_mut().filter(|u| u.out()) {
            let chase = (u.after != 0)
                .then(|| inner.chases.iter().flatten().find(|c| c.id == u.after).copied())
                .flatten();
            if u.after != 0 && chase.is_none() {
                // Nobody to chase now: on to where it was going, and a cordon there.
                u.after = 0;
            }
            match u.state {
                State::Driving | State::Cordon if chase.is_some() => {
                    let Some(c) = chase else { continue };
                    u.state = State::Driving;
                    u.level = u.level.max(c.level);
                    let place = (c.at.0, c.at.1 + CHASE_SPREAD[usize::from(u.slot) % CHASE_SPREAD.len()]);
                    // The way found again every while once they've moved, and at once if they've
                    // gone from where it's got to.
                    let moved = apart(u.goal, place);
                    let due = tick.saturating_sub(u.routed) >= reroute;
                    if (due && moved > 3.0) || (u.arrived() && moved > STRAYED) {
                        u.route_to(place, tick);
                    }
                    u.drive(dt);
                }
                State::Driving => {
                    u.drive(dt);
                    if u.arrived() {
                        u.state = State::Cordon;
                        u.until = tick + (tables::answer(u.level).hold * TICK_HZ as f32) as u32;
                    }
                }
                State::Cordon => {
                    if tick >= u.until {
                        u.state = State::Returning;
                        u.route_to(town::watch_door(u.station), tick);
                    }
                }
                State::Returning => {
                    u.drive(dt);
                    if u.arrived() {
                        *u = Unit::at_home(u.kind, u.station);
                    }
                }
                State::Home => {}
            }
        }

        // The pilots they're after: in sight of one, their heat holds; held slow near one, stopped.
        for k in 0..MAX_CHASED {
            let Some(mut c) = inner.chases[k] else { continue };
            let mut nearest = (f32::INFINITY, 0.0f32);
            for u in inner.units.iter().filter(|u| matches!(u.state, State::Driving | State::Cordon)) {
                let d = apart((u.x, u.t), c.at);
                if d - u.kind.reach() < nearest.0 - nearest.1 {
                    nearest = (d, u.kind.reach());
                }
            }
            if nearest.0 <= UNIT_SIGHT {
                c.met = true;
                watch.seen(c.id, tick);
            } else if !c.met
                && tick.saturating_sub(c.since) < ON_THE_WAY
                && inner.units.iter().any(|u| u.chasing(c.id))
            {
                // On their way, none there yet: it doesn't cool before they could have come.
                watch.seen(c.id, tick);
            }
            let slow = c.speed.is_some_and(|v| v < CATCH.0);
            let wait = tables::catch_after(nearest.0, nearest.1).filter(|_| slow);
            c.held = if wait.is_some() { c.held + dt } else { 0.0 };
            inner.chases[k] = Some(c);
            if wait.is_none_or(|w| c.held < w) {
                continue;
            }
            // Stopped: the Watch has them. Its units near hold a cordon where they are.
            if let Some(level) = watch.stop(c.id, tick) {
                self.stops.fetch_add(1, Ordering::Relaxed);
                tracing::info!(session = c.id, level, "stopped by the Watch's units");
            }
            inner.chases[k] = None;
            for u in inner.units.iter_mut().filter(|u| u.chasing(c.id)) {
                u.after = 0;
                if apart((u.x, u.t), c.at) <= AFOOT.0 {
                    u.state = State::Cordon;
                    u.until = tick + (tables::answer(u.level).hold * TICK_HZ as f32) as u32;
                }
            }
        }
    }

    /// The units out within `reach` m of `(x, s)` on the ring's floor (the deck's `s`), nearest
    /// first, at most `most`, as the wire has them.
    pub fn near(&self, (x, s): (f32, f32), reach: f32, most: usize) -> Vec<wire::Unit> {
        let Ok(inner) = self.inner.lock() else { return Vec::new() };
        let at = (x, town::town_t(s));
        let mut near: Vec<(f32, wire::Unit)> = inner
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.out())
            .filter_map(|(id, u)| {
                let d = apart((u.x, u.t), at);
                let state = match u.state {
                    State::Driving | State::Home => UnitState::Driving,
                    State::Cordon => UnitState::Cordon,
                    State::Returning => UnitState::Returning,
                };
                let s = town::deck_s(town::wrap(u.t));
                (d <= reach).then_some((
                    d,
                    wire::Unit { id: id as u16, kind: u.kind, x: u.x, s, heading: u.yaw, state },
                ))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.into_iter().take(most).map(|(_, u)| u).collect()
    }

    /// Every unit, as the dispatcher has it.
    pub fn seen(&self) -> Vec<Seen> {
        let Ok(inner) = self.inner.lock() else { return Vec::new() };
        inner
            .units
            .iter()
            .enumerate()
            .map(|(id, u)| {
                let mut to_go = if u.next < u.len {
                    let w = u.route[u.next];
                    (w.x - u.x).hypot(w.t - u.t)
                } else {
                    0.0
                };
                if u.next < u.len {
                    to_go += roads::length(&u.route[u.next..u.len]);
                }
                Seen {
                    id: id as u16,
                    kind: u.kind,
                    station: u.station,
                    state: u.state,
                    x: u.x,
                    t: town::wrap(u.t),
                    yaw: u.yaw,
                    incident: u.incident,
                    after: u.after,
                    level: u.level,
                    goal: u.goal,
                    to_go,
                    until: u.until,
                    wait: u.wait,
                    patrol: u.patrol,
                }
            })
            .collect()
    }

    /// How many units are out of their stations (for `/status`).
    pub fn out(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.units.iter().filter(|u| u.out()).count())
    }

    /// How many pilots the units have stopped.
    pub fn stops(&self) -> u64 {
        self.stops.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::watch::Offence;

    /// A step's ticks (the task's 100 ms).
    const DT: u32 = 3;

    /// Every unit out on a road.
    fn on_the_roads(d: &Dispatch) {
        for u in d.seen().iter().filter(|u| u.state != State::Home) {
            assert!(roads::drivable(u.x, u.t), "{u:?} is off the roads");
        }
    }

    /// Steps `d` from `tick` till `done` says so (or `most` ticks), each step checked by `each`:
    /// the tick it was done at.
    fn run(
        d: &Dispatch,
        w: &Watch,
        tick: &mut u32,
        most: u32,
        mut each: impl FnMut(u32, &[Seen]),
        mut done: impl FnMut(&[Seen]) -> bool,
    ) -> Option<u32> {
        let end = *tick + most;
        while *tick < end {
            *tick += DT;
            d.step(w, *tick);
            on_the_roads(d);
            let seen = d.seen();
            each(*tick, &seen);
            if done(&seen) {
                return Some(*tick);
            }
        }
        None
    }

    /// The stations by road from `goal`, nearest first.
    fn by_road(goal: (f32, f32)) -> Vec<u16> {
        let mut far: Vec<(f32, u16)> = (0..STATIONS as u16)
            .map(|d| {
                let mut way = [Waypoint::default(); MOST];
                let n = roads::route(town::watch_door(d), goal, &mut way).min(MOST);
                (roads::length(&way[..n]), d)
            })
            .collect();
        far.sort_by(|a, b| a.0.total_cmp(&b.0));
        far.into_iter().map(|(_, d)| d).collect()
    }

    #[test]
    fn the_watchs_car_on_patrol_nearest_an_incident_answers_it_from_where_it_was() {
        let (w, d) = (Watch::default(), Dispatch::default());
        let tick = 2_000;
        d.step(&w, tick);
        // One of the Watch's cars on patrol, and an incident on the road 120 m ahead of it.
        let mut patrol = None;
        street::each_car(-900.0, 900.0, 3_000.0, 9_000.0, tick + DT, 0.0, &mut |c| {
            if c.kind == street::Kind::Watch && patrol.is_none() {
                patrol = Some(*c);
            }
        });
        let c = patrol.expect("a car of the Watch's on patrol");
        let dir = if c.yaw > 1.0 { 1.0 } else { -1.0 };
        let goal = (c.x, c.t + dir * 120.0);
        let id = w.raise((goal.0, town::deck_s(goal.1)), 1, Offence::Knock, tick);
        let mut promoted = Vec::new();
        d.step_and(&w, tick + DT, &mut |car, s| promoted.push((car, s)));
        let sent: Vec<Seen> = d.seen().into_iter().filter(|u| u.incident == id).collect();
        let p = sent.iter().find(|u| u.patrol != 0).expect("a patrol answered");
        assert_eq!(p.kind, UnitKind::Car);
        // It answers from about where it was (the nearest patrol: this one, or one nearer still).
        assert!(apart((p.x, p.t), goal) <= apart((c.x, c.t), goal) + 1.0, "{p:?} vs {c:?}");
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].0, p.patrol);
        // The station's hound still comes; no second car.
        assert_eq!(sent.iter().filter(|u| u.kind == UnitKind::Car).count(), 1, "{sent:?}");
        // Promoted once: a second incident there doesn't take it again.
        let again = w.raise((goal.0, town::deck_s(goal.1 + 5.0)), 1, Offence::Knock, tick + 2 * DT);
        d.step(&w, tick + 2 * DT);
        let twice = d.seen().into_iter().filter(|u| u.incident == again && u.patrol == p.patrol).count();
        assert_eq!(twice, 0);
    }

    #[test]
    fn an_incident_brings_the_nearest_stations_units_who_hold_a_cordon_and_go_home() {
        let (w, d) = (Watch::default(), Dispatch::default());
        let mut tick = 1_000;
        d.step(&w, tick);
        assert_eq!(d.out(), 0);
        // On the boulevard 200 m round from district 3's station.
        let (dx, dt) = town::watch_door(3);
        let goal = (dx - 15.0, dt + 200.0);
        assert_eq!(by_road(goal)[0], 3);
        let id = w.raise((goal.0, town::deck_s(goal.1)), 1, Offence::Knock, tick);
        tick += DT;
        d.step(&w, tick);
        let sent: Vec<Seen> = d.seen().into_iter().filter(|u| u.state != State::Home).collect();
        assert_eq!(sent.len(), 2, "{sent:?}");
        // (A car of the Watch's on patrol near answers for its station's: `promoted_patrols`.)
        assert!(
            sent.iter()
                .all(|u| (u.station == 3 || u.patrol != 0) && u.incident == id && u.state == State::Driving)
        );
        assert_eq!(sent.iter().filter(|u| u.kind == UnitKind::Car).count(), 1);
        // One after the other out of the door, not as one.
        let mut waits: Vec<f32> = sent.iter().filter(|u| u.patrol == 0).map(|u| u.wait).collect();
        waits.sort_by(f32::total_cmp);
        assert!(waits[0] == 0.0, "{waits:?}");
        assert!(waits.len() < 2 || (waits[1] > PULL_OUT * 0.5 && waits[1] <= PULL_OUT), "{waits:?}");
        on_the_roads(&d);
        // Each there within its route's length at its speed, and a little.
        let haste = tables::answer(1).haste;
        let due: Vec<(u16, u32)> = sent
            .iter()
            .map(|u| {
                (u.id, tick + ((u.wait + u.to_go / (u.kind.speed() * haste) + 1.0) * TICK_HZ as f32) as u32)
            })
            .collect();
        let mut arrived = vec![None; MAX_UNITS];
        let all_there = run(
            &d,
            &w,
            &mut tick,
            120 * TICK_HZ,
            |t, seen| {
                for u in seen.iter().filter(|u| u.state == State::Cordon) {
                    arrived[usize::from(u.id)].get_or_insert(t);
                }
            },
            |seen| seen.iter().filter(|u| u.state == State::Cordon).count() == 2,
        );
        assert!(all_there.is_some(), "never there");
        for (id, by) in due {
            let at = arrived[usize::from(id)].expect("arrived");
            assert!(at <= by, "unit {id} there at {at}, due by {by}");
        }
        // Round the place, on the roads nearest the cordon's ring.
        for u in d.seen().iter().filter(|u| u.state == State::Cordon) {
            let off = apart((u.x, u.t), goal);
            assert!(off < tables::answer(1).cordon + 40.0, "{u:?}: {off} m off");
        }
        // Held the level's time, then home, and free.
        let hold = (tables::answer(1).hold * TICK_HZ as f32) as u32;
        let first = arrived.iter().flatten().min().copied().unwrap();
        let back = run(
            &d,
            &w,
            &mut tick,
            90 * TICK_HZ,
            |_, _| {},
            |seen| seen.iter().any(|u| u.state == State::Returning),
        )
        .expect("never went home");
        assert!(back >= first + hold && back <= first + hold + 2 * DT, "{back} vs {first} + {hold}");
        run(&d, &w, &mut tick, 120 * TICK_HZ, |_, _| {}, |seen| seen.iter().all(|u| u.state == State::Home))
            .expect("never got home");
        assert_eq!(d.out(), 0);
        // Free: the next incident there has the same.
        w.raise((goal.0, town::deck_s(goal.1)), 1, Offence::Knock, tick);
        tick += DT;
        d.step(&w, tick);
        let again: Vec<u16> = d.seen().iter().filter(|u| u.state != State::Home).map(|u| u.id).collect();
        assert_eq!(again, sent.iter().map(|u| u.id).collect::<Vec<_>>());
    }

    #[test]
    fn a_worse_incident_sends_more_from_further_and_a_rise_sends_the_difference() {
        let (w, d) = (Watch::default(), Dispatch::default());
        let mut tick = 10;
        d.step(&w, tick);
        let at = (-300.0, town::CELL * 30.5);
        w.raise((at.0, town::deck_s(at.1)), 5, Offence::TookTheWatchs, tick);
        tick += DT;
        d.step(&w, tick);
        let sent: Vec<Seen> = d.seen().into_iter().filter(|u| u.state != State::Home).collect();
        let top = tables::answer(5);
        assert_eq!(sent.iter().filter(|u| u.kind == UnitKind::Car).count(), usize::from(top.cars));
        assert_eq!(sent.iter().filter(|u| u.kind == UnitKind::Hound).count(), usize::from(top.hounds));
        let nearest = &by_road(at)[..usize::from(top.stations)];
        assert!(sent.iter().all(|u| nearest.contains(&u.station)), "{nearest:?}: {sent:?}");
        on_the_roads(&d);
        // An offender's, risen from 1 to 3: what 3 sends more, to the same incident (she's not in the
        // town to chase: no pose).
        let (w, d) = (Watch::default(), Dispatch::default());
        let place = (20.0, town::CELL * 80.5);
        let pose = (place.0, town::deck_s(place.1));
        w.offend(9, "Maren", Offence::Knock, pose, 0);
        d.step(&w, 0);
        d.step(&w, DT);
        assert_eq!(d.out(), 2);
        w.offend(9, "Maren", Offence::TookTheWatchs, pose, 2 * DT);
        assert_eq!(w.level(9), 3);
        d.step(&w, 3 * DT);
        let three = tables::answer(3);
        assert_eq!(d.out(), usize::from(three.cars + three.hounds));
        let theirs = w.incidents(3 * DT)[0].id;
        assert!(
            d.seen().iter().filter(|u| u.state != State::Home).all(|u| u.incident == theirs && u.after == 0)
        );
        on_the_roads(&d);
    }

    #[test]
    fn a_wanted_pilot_is_chased_and_stopped_by_the_units_with_no_word_from_their_client() {
        let (w, d) = (Watch::default(), Dispatch::default());
        let id = 4;
        // On the boulevard 150 m round from district 5's station.
        let (_, dt) = town::watch_door(5);
        let mut at = (24.0, dt + 150.0);
        let pose = |at: (f32, f32)| Some((at.0, town::deck_s(at.1)));
        let start = (at.0, town::deck_s(at.1));
        let mut tick = 100;
        w.offend(id, "Maren", Offence::TookTheWatchs, start, tick);
        let level = w.level(id);
        assert_eq!(level, 3);
        // Driving on along the boulevard at 12 m/s for half a minute: chased, never stopped, and
        // held in the units' sight their heat doesn't cool.
        let mut seen_near = false;
        for _ in 0..(30 * TICK_HZ / DT) {
            tick += DT;
            at.1 += 12.0 * DT as f32 / TICK_HZ as f32;
            w.tick(id, pose(at), tick);
            d.step(&w, tick);
            on_the_roads(&d);
            seen_near |= d.seen().iter().any(|u| u.after == id && apart((u.x, u.t), at) < UNIT_SIGHT);
        }
        assert_eq!(d.stops(), 0);
        assert!(seen_near, "the units never caught up");
        assert_eq!(w.level(id), level, "it cooled under their eye");
        // Stopped there: theirs within a few seconds, and the fine owed, with nothing said.
        let before = tick;
        while w.level(id) > 0 && tick < before + 60 * TICK_HZ {
            tick += DT;
            w.tick(id, pose(at), tick);
            d.step(&w, tick);
            on_the_roads(&d);
        }
        let secs = (tick - before) as f32 / TICK_HZ as f32;
        assert_eq!(w.level(id), 0, "never stopped");
        assert!(secs < 20.0, "stopped after {secs} s");
        assert_eq!(d.stops(), 1);
        assert_eq!(w.take_fine(id), Some(level));
        // Those that had them hold a cordon there, then go home.
        let cordon = d.seen().iter().filter(|u| u.state == State::Cordon).count();
        assert!(cordon > 0);
        assert!(d.seen().iter().all(|u| u.after == 0));
        run(&d, &w, &mut tick, 400 * TICK_HZ, |_, _| {}, |seen| seen.iter().all(|u| u.state == State::Home))
            .expect("never all home");
    }

    #[test]
    fn a_pilot_off_the_road_is_walked_over_to_and_one_who_leaves_the_town_isnt_chased() {
        // In the middle of a block's park, off every road.
        let r = town::block_rect(town::BlockId::new(31, 7));
        let park = (0.5 * (r.x0 + r.x1), 0.5 * (r.t0 + r.t1));
        assert!(!roads::drivable(park.0, park.1));
        let (w, d) = (Watch::default(), Dispatch::default());
        let spot = (park.0, town::deck_s(park.1));
        let at = Some(spot);
        let mut tick = 0;
        w.offend(2, "Ines", Offence::HitSomeone, spot, tick);
        while w.level(2) > 0 && tick < 120 * TICK_HZ {
            tick += DT;
            w.tick(2, at, tick);
            d.step(&w, tick);
            on_the_roads(&d);
        }
        assert_eq!((w.level(2), d.stops()), (0, 1), "never stopped in the park");
        // Gone up the lift (no pose in the town): the chase is off, and they go home in time.
        let (w, d) = (Watch::default(), Dispatch::default());
        w.offend(2, "Ines", Offence::HitSomeone, spot, 0);
        w.tick(2, at, 0);
        d.step(&w, 0);
        d.step(&w, DT);
        assert!(d.out() > 0);
        w.tick(2, None, 2 * DT);
        let mut tick = 2 * DT;
        run(&d, &w, &mut tick, 600 * TICK_HZ, |_, _| {}, |seen| seen.iter().all(|u| u.state == State::Home))
            .expect("never all home");
        assert_eq!((d.stops(), w.level(2) > 0), (0, true));
    }
}
