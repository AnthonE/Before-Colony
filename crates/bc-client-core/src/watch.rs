//! Heat and the Watch in the ring's town, as the client draws it (`docs/OPEN_CITY.md`, "Heat and the
//! Watch"; `docs/FREE_ROAM.md`, 4): what a pilot does in the street draws the Watch's eye, and its
//! cars come.
//!
//! - **Heat** is how much the Watch wants a pilot, 0 to 5, and it's the server's: it keeps every
//!   pilot's by `bc_sim::colony::watch`'s tables (what each [`Offence`] adds, the levels, the
//!   cool-down) and says so (`Update::Heat`; everyone else's rides in the plaza's poses). What only
//!   this client sees it do (a car taken from a kerb or its driver, a knock) it reports
//!   (`Request::Offence`), which can only ever add to it.
//! - **The pursuit** ([`Pursuit`]) is as many of the Watch's cars as the heat says, come from out of
//!   sight along the town's own roads: each drives `bc_sim::car`'s car round the blocks to the pilot
//!   (along its road to the cross street nearest them, across to their street, and in), against the
//!   same walls. A pilot who's stopped with one of them at their side for a few seconds is stopped
//!   ([`Pursuit::step`] says so: `Request::Stopped`), and the server takes its fine of the heat it
//!   holds and clears it.
//!
//! This is the ambient tier's Watch: each client draws the pursuit of its own pilot, as it draws its
//! own traffic. The Watch's units on the server, seen the same by everyone and answering with more
//! than cars, are `docs/FREE_ROAM.md`'s W1 and P2.

use bc_sim::car::{Drive, RING_FLOOR, forward};
use bc_sim::colony::deck::{self, DeckPos, RING};
use bc_sim::colony::{street, town};

use crate::town::Obstacles;
use crate::vehicle::{Kind, Vehicle};

pub use bc_sim::colony::watch::{COOL, MAX_HEAT, Offence, units};

/// The Watch's fine at `level` of heat, credits (the server's: `bc_sim::colony::watch::fine`).
pub fn fine(level: u8) -> u64 {
    bc_sim::colony::watch::fine(level)
}

/// The heat as the HUD shows it: a diamond a level ("◆◆"); nothing at 0. (Only the lit ones: the
/// page's fonts have no ◇, and one borrowed from another font draws twice the size.)
pub fn stars(level: u8) -> String {
    "◆".repeat(usize::from(level.min(MAX_HEAT)))
}

/// Whether one of the Watch's own cars on its beat is within `reach` m of `at` at tick `tick`.
pub fn watch_car_near(at: DeckPos, reach: f32, (tick, frac): (u32, f32)) -> bool {
    bc_sim::colony::watch::car_near(at.x, at.s, reach, tick, frac)
}

/// Whether one of the Watch's own cars on its beat within `reach` m of `at` has a clear line to
/// it: nothing of the town's (a building, a lift's pier) between them at eye height.
pub fn watch_car_sees(at: DeckPos, reach: f32, (tick, frac): (u32, f32)) -> bool {
    bc_sim::colony::watch::car_sees(at.x, at.s, reach, tick, frac)
}

/// Whether a car driven at `at` is where people walk: on a block, a pavement or a rim walk.
pub fn where_people_walk(at: DeckPos) -> bool {
    bc_sim::colony::watch::where_people_walk(at.x, at.s)
}

/// One of the Watch's cars in pursuit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub car: Vehicle,
    /// Backing out of whatever it's caught on, s left.
    backing: f32,
    /// How long it's been stopped short of where it's going, s.
    stuck: f32,
    /// Called off: driving away till it's out of sight.
    leaving: bool,
    /// Once called off: how long till it's gone wherever it's got to, s.
    left: f32,
}

/// How far from the pilot the Watch's cars come into the pursuit, m, and how far off a car called
/// off is gone.
const SPAWN: f32 = 170.0;
const GONE: f32 = 260.0;
/// A car called off is gone after this long, however far it's got, s.
const GONE_AFTER: f32 = 20.0;
/// A car this near a pilot this slow, for this long, has them: m, m/s, s.
const CATCH: (f32, f32, f32) = (9.0, 2.5, 2.5);
/// The Watch's cars drive this hard after someone: the share of a car's throttle.
const PRESS: f32 = 0.85;
/// Where a car can't follow (a quay, a park, between two houses) its officers get out and walk: a
/// pilot stopped within this far of one, m, is theirs after [`ON_FOOT`]'s wait and the walk across at
/// this pace, m/s.
const FOOT: (f32, f32) = (80.0, 4.0);
const ON_FOOT: f32 = 8.0;
/// A car of the Watch's this near has a pilot in sight, m: nothing cools while one has.
const IN_SIGHT: f32 = 90.0;

/// The Watch's cars after one pilot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pursuit {
    pub units: Vec<Unit>,
    /// How long one of them has had the pilot stopped at its side, s.
    held: f32,
    /// How long the pilot's been stopped with one of them near but not at their side, s.
    afoot: f32,
    /// Units sent so far: each comes from a different way.
    sent: u32,
}

/// The roads that run round the ring, by where they are across it: each lane of the traffic's.
fn roads() -> impl Iterator<Item = f32> {
    (0..street::LANES).filter_map(street::lane).map(|l| l.x)
}

/// The road round the ring nearest `x` across it.
fn road_near(x: f32) -> f32 {
    roads().min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs())).unwrap_or(x)
}

/// The cross street (a cell's edge) nearest `t` round the ring, in `t`'s own turn.
fn cross_near(t: f32) -> f32 {
    (t / town::CELL).round() * town::CELL
}

/// The heading (the walker's yaw) from `(x, s)` towards `(tx, ts)`.
fn heading(x: f32, s: f32, tx: f32, ts: f32) -> f32 {
    // Forward is `(sin yaw, −cos yaw)` in `(x, s)`.
    (tx - x).atan2(-(ts - s))
}

impl Pursuit {
    /// Nobody after anyone.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// `dt` s of the pursuit of a pilot at `at` going `speed` m/s, wanted at `heat`: cars sent to
    /// make up the heat's number (from out of sight, along the roads), each driven on towards the
    /// pilot among `cars`; those called off (the heat fallen, or paid) driven away till they're
    /// gone. True when one of them has the pilot: stopped, at its side, for long enough.
    pub fn step(&mut self, heat: u8, at: DeckPos, speed: f32, dt: f32, cars: &Obstacles) -> bool {
        let want = units(heat);
        let after = self.units.iter().filter(|u| !u.leaving).count();
        if after < want {
            if let Some(car) = self.spawn(at) {
                self.units.push(Unit { car, backing: 0.0, stuck: 0.0, leaving: false, left: GONE_AFTER });
            }
        } else if after > want {
            // The last sent is the first called off.
            if let Some(u) = self.units.iter_mut().rev().find(|u| !u.leaving) {
                u.leaving = true;
            }
        }
        let mut holding = false;
        let mut nearest = f32::MAX;
        let others: Vec<Vehicle> = self.units.iter().map(|u| u.car).collect();
        for (k, u) in self.units.iter_mut().enumerate() {
            // Where it is in the pilot's turn of the ring.
            let s = at.s + deck::round_from(RING, at.s, u.car.s);
            u.car.s = s;
            let (dx, ds) = (at.x - u.car.x, at.s - s);
            let off = dx.hypot(ds);
            // Where it's going: away, if called off; else round the blocks to the pilot.
            let (tx, ts) =
                if u.leaving { (u.car.x, s - ds.signum() * 400.0) } else { Self::way(u.car.x, s, at) };
            if u.leaving {
                u.left -= dt;
            }
            let near = !u.leaving && off < CATCH.0;
            holding |= near && speed.abs() < CATCH.1;
            if !u.leaving {
                nearest = nearest.min(off);
            }
            let mut d = Drive::default();
            if u.backing > 0.0 {
                u.backing -= dt;
                d.throttle = -1.0;
                d.steer = if k.is_multiple_of(2) { 0.8 } else { -0.8 };
            } else if near && u.car.speed.abs() < 6.0 {
                // At their side: stopped.
                d.throttle = if u.car.speed > 0.3 { -1.0 } else { 0.0 };
                d.handbrake = true;
            } else {
                let want = heading(u.car.x, s, tx, ts);
                let turn = (want - u.car.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                d.steer = (turn * 1.6).clamp(-1.0, 1.0);
                // Slower into a turn, and closing on the pilot.
                let ease = if turn.abs() > 0.5 { 0.35 } else { 1.0 };
                let closing = if !u.leaving && off < 30.0 { 0.5 } else { 1.0 };
                let top = 26.0 * ease * closing;
                d.throttle = if u.car.speed > top { -0.4 } else { PRESS };
            }
            // Among the cars standing about and each other.
            let before = u.car;
            crate::town::drive(&mut u.car, &d, dt, cars);
            let bumped = others
                .iter()
                .enumerate()
                .any(|(j, o)| j != k && (o.x - u.car.x).hypot(o.s - u.car.s) < 3.0 && j < k);
            if bumped {
                u.car = Vehicle { speed: 0.0, ..before };
            }
            // Caught on something: back out of it.
            let moving = u.car.speed.abs() > 0.6;
            u.stuck = if moving || near || u.backing > 0.0 { 0.0 } else { u.stuck + dt };
            if u.stuck > 1.2 {
                u.stuck = 0.0;
                u.backing = 1.4;
            }
        }
        // Those called off and gone.
        self.units
            .retain(|u| !(u.leaving && ((u.car.x - at.x).hypot(u.car.s - at.s) > GONE || u.left <= 0.0)));
        self.held = if holding { self.held + dt } else { 0.0 };
        // Stopped where no car comes to their side: its officers walk over.
        let waiting = speed.abs() < CATCH.1 && nearest < FOOT.0;
        self.afoot = if waiting { self.afoot + dt } else { 0.0 };
        self.held >= CATCH.2 || (waiting && self.afoot >= ON_FOOT + nearest / FOOT.1)
    }

    /// Whether one of the cars after a pilot at `at` has them in sight (near enough: the heat
    /// doesn't cool under its eye).
    pub fn in_sight(&self, at: DeckPos) -> bool {
        self.units
            .iter()
            .any(|u| !u.leaving && (u.car.x - at.x).hypot(deck::round_from(RING, at.s, u.car.s)) < IN_SIGHT)
    }

    /// Where a car at `(x, s)` drives for next on its way to a pilot at `at`: straight at them once
    /// it's level with them across the ring, or near; else along its road to the cross street
    /// nearest them, along that till it's level with them (the cross streets run from rim to rim,
    /// over the canal), and in, off the road if that's where they are.
    fn way(x: f32, s: f32, at: DeckPos) -> (f32, f32) {
        let (dx, ds) = (at.x - x, at.s - s);
        if dx.abs() < 14.0 || dx.hypot(ds) < 40.0 {
            return (at.x, at.s);
        }
        // The cross street nearest the pilot, in this car's turn of the ring's `s`.
        let t = town::town_t(at.s);
        let cross = at.s + (cross_near(t) - t);
        if (s - cross).abs() > 8.0 {
            // Along its own road to it.
            return (road_near(x), cross);
        }
        // Along the cross street till it's level with them.
        (at.x, cross)
    }

    /// A car of the Watch's come into the pursuit: on the road nearest the pilot, [`SPAWN`] m off
    /// round the ring (ahead and behind by turns, a road over for each after the second), nose
    /// towards them. None if nothing fits there.
    fn spawn(&mut self, at: DeckPos) -> Option<Vehicle> {
        let n = self.sent;
        self.sent += 1;
        let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        let over = (n / 2) as f32 * 40.0 * if n % 4 < 2 { 1.0 } else { -1.0 };
        let x = road_near(at.x + over);
        let s = at.s + side * SPAWN;
        let yaw = if side > 0.0 { 0.0 } else { std::f32::consts::PI };
        let car = Vehicle::new(Kind::Car, RING_FLOOR, s, x, yaw);
        car.fits(s, x, yaw).then_some(car)
    }

    /// Everyone called off (the heat paid).
    pub fn call_off(&mut self) {
        for u in &mut self.units {
            u.leaving = true;
        }
        self.held = 0.0;
        self.afoot = 0.0;
    }

    /// The nose-first heading and place of each car after the pilot, for whoever draws them.
    pub fn cars(&self) -> impl Iterator<Item = &Vehicle> {
        self.units.iter().map(|u| &u.car)
    }
}

/// Which way a unit's car faces, for the tests.
pub fn facing(v: &Vehicle) -> (f32, f32) {
    forward(v.yaw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watchs_cars_come_round_the_blocks_stop_a_stopped_pilot_and_go_when_called_off() {
        // A pilot stopped on a street two columns fore of the boulevard, a little way round from Hub
        // Gate.
        let x = town::column(4).1 + 1.75;
        let at = DeckPos::new(RING, x, town::deck_s(town::CELL * 3.4), 0.0);
        let cars = Obstacles::default();
        let mut p = Pursuit::default();
        let mut caught = None;
        for n in 0..(90.0 * 30.0) as u32 {
            if p.step(3, at, 0.0, 1.0 / 30.0, &cars) {
                caught = Some(n as f32 / 30.0);
                break;
            }
            for u in &p.units {
                assert!(u.car.fits(u.car.s, u.car.x, u.car.yaw), "a unit in a wall: {u:?}");
            }
        }
        let secs = caught.expect("the Watch never caught a pilot standing still");
        assert!((5.0..80.0).contains(&secs), "caught after {secs} s");
        assert_eq!(p.units.len(), 3, "three cars at heat 3");
        let nearest = p.cars().map(|c| (c.x - at.x).hypot(c.s - at.s)).fold(f32::MAX, f32::min);
        assert!(nearest < CATCH.0, "{nearest} m off");
        // Moving, nobody has them however near.
        let mut q = p.clone();
        assert!(!(0..300).any(|_| q.step(3, at, 12.0, 1.0 / 30.0, &cars)));
        // Called off, they drive away and are gone.
        p.call_off();
        for _ in 0..(60.0 * 30.0) as u32 {
            assert!(!p.step(0, at, 0.0, 1.0 / 30.0, &cars));
        }
        assert!(p.is_empty(), "{} still about", p.units.len());
    }

    #[test]
    fn a_pilot_stopped_off_the_road_is_still_caught() {
        // On the canal's quay in the building site, 128 m from the nearest lane of traffic; and in
        // the middle of a block's park.
        let quay = DeckPos::new(RING, 557.8, 10_891.0, 0.0);
        let r = town::block_rect(town::BlockId::new(31, 7));
        let park = DeckPos::new(RING, 0.5 * (r.x0 + r.x1), town::deck_s(0.5 * (r.t0 + r.t1)), 0.0);
        for at in [quay, park] {
            let cars = Obstacles::default();
            let mut p = Pursuit::default();
            let caught = (0..(90.0 * 30.0) as u32).find(|_| p.step(2, at, 0.0, 1.0 / 30.0, &cars));
            let secs = caught.expect("the Watch never got to them") as f32 / 30.0;
            assert!(secs < 70.0, "caught after {secs} s at {at:?}");
            assert!(p.in_sight(at));
            // Walking on, nobody has them.
            let mut q = Pursuit::default();
            assert!(!(0..(60.0 * 30.0) as u32).any(|_| q.step(2, at, 3.5, 1.0 / 30.0, &cars)));
        }
        assert!(!Pursuit::default().in_sight(quay));
    }

    #[test]
    fn a_unit_blocks_away_drives_the_roads_to_the_pilot() {
        // The pilot on the boulevard; a car of the Watch's four columns fore of it, half a kilometre
        // round: it gets there without leaving the roads for long.
        let at = DeckPos::new(RING, 20.0, town::deck_s(town::CELL * 20.5), 0.0);
        let from_x = town::column(2).1 + 1.75;
        let mut p = Pursuit {
            units: vec![Unit {
                car: Vehicle::new(Kind::Car, RING_FLOOR, at.s - 520.0, from_x, std::f32::consts::PI),
                backing: 0.0,
                stuck: 0.0,
                leaving: false,
                left: GONE_AFTER,
            }],
            held: 0.0,
            afoot: 0.0,
            sent: 1,
        };
        let cars = Obstacles::default();
        let (mut off_road, mut arrived) = (0u32, None);
        for n in 0..(120.0 * 30.0) as u32 {
            p.step(1, at, 9.0, 1.0 / 30.0, &cars);
            let c = p.units[0].car;
            off_road += u32::from(town::ground_at(c.x, town::town_t(c.s)) != town::Ground::Road);
            if (c.x - at.x).hypot(c.s - at.s) < 12.0 {
                arrived = Some(n as f32 / 30.0);
                break;
            }
        }
        let secs = arrived.expect("it never got there");
        assert!(secs < 100.0 && off_road < 240, "there in {secs} s, {off_road} ticks off the road");
        assert!(watch_car_near(at, 3_000.0, (0, 0.0)), "the Watch's cars are on their beats");
        // One of them sees down its own street, and not through the houses.
        let mut beat = None;
        street::each_car(-1_000.0, 1_000.0, 0.0, 6_000.0, 0, 0.0, &mut |c| {
            if c.kind == street::Kind::Watch && c.x.abs() > 100.0 {
                beat = beat.or(Some(*c));
            }
        });
        let c = beat.expect("one of the Watch's on a street between the blocks");
        let ahead = DeckPos::new(RING, c.x, town::deck_s(c.t + 30.0), 0.0);
        let behind_a_house = DeckPos::new(RING, c.x + 45.0, town::deck_s(c.t + 30.0), 0.0);
        assert!(watch_car_sees(ahead, 80.0, (0, 0.0)));
        assert!(watch_car_near(behind_a_house, 80.0, (0, 0.0)));
        let on = town::ground_at(behind_a_house.x, town::town_t(behind_a_house.s));
        assert!(
            !watch_car_sees(behind_a_house, 80.0, (0, 0.0)) || !matches!(on, town::Ground::Block(_)),
            "{on:?}"
        );
        assert!(where_people_walk(DeckPos::new(RING, -300.0, town::deck_s(town::CELL * 30.5), 0.0)));
        assert!(!where_people_walk(at));
    }
}
