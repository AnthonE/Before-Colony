//! Heat and the Watch in the ring's town, a first pass (`docs/OPEN_CITY.md`, "Heat and the Watch";
//! `docs/FREE_ROAM.md`, 4): what a pilot does in the street draws the Watch's eye, and its cars come.
//!
//! - **Heat** ([`Heat`]) is how much the Watch wants a pilot, 0 to 5. Offences add to it
//!   ([`Offence`]: a car taken from a kerb, a knock against another car, driving where people walk);
//!   time without one lets it fall, a level at a time.
//! - **The pursuit** ([`Pursuit`]) is as many of the Watch's cars as the heat says, come from out of
//!   sight along the town's own roads: each drives `bc_sim::car`'s car round the blocks to the pilot
//!   (along its road to the cross street nearest them, across to their street, and in), against the
//!   same walls. A pilot who's stopped with one of them at their side for a few seconds is fined
//!   ([`Pursuit::step`] says so), and the heat is paid off.
//!
//! This is the ambient tier's Watch: each client runs the pursuit of its own pilot, as it draws its
//! own traffic. The fine is the server's (`Request::Fine`). The Watch in the tick, seen the same by
//! everyone and answering with more than cars, is `docs/FREE_ROAM.md`'s W1 and P2.

use bc_sim::car::{Drive, RING_FLOOR, forward};
use bc_sim::colony::deck::{self, DeckPos, RING};
use bc_sim::colony::{street, town};

use crate::town::Obstacles;
use crate::vehicle::{Kind, Vehicle};

/// What draws the Watch's eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offence {
    /// A car taken from a kerb.
    TookACar,
    /// The same, in sight of one of the Watch's own cars.
    TookACarInSight,
    /// A knock against another car.
    Knock,
    /// A second of driving at speed where people walk.
    Reckless,
}

impl Offence {
    /// What it adds to a pilot's heat.
    pub fn points(self) -> f32 {
        match self {
            // (Nobody much minds a car borrowed from a kerb, till there's more.)
            Offence::TookACar => 0.6,
            Offence::TookACarInSight => 3.0,
            Offence::Knock => 1.0,
            Offence::Reckless => 0.8,
        }
    }
}

/// The most the Watch wants anyone.
pub const MAX_HEAT: u8 = 5;
/// The points each level starts at.
const LEVELS: [f32; MAX_HEAT as usize] = [1.0, 3.0, 6.0, 10.0, 15.0];
/// How long without an offence before the heat falls a level, s.
pub const COOL: f32 = 28.0;
/// The Watch's cars sent at each level of heat.
const UNITS: [usize; MAX_HEAT as usize + 1] = [0, 1, 2, 3, 4, 6];

/// How much the Watch wants a pilot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heat {
    points: f32,
    /// Since the last offence, s.
    quiet: f32,
}

impl Heat {
    /// 0 (nothing) to [`MAX_HEAT`].
    pub fn level(&self) -> u8 {
        LEVELS.iter().take_while(|at| self.points >= **at).count() as u8
    }

    /// An offence: true if the heat rose a level.
    pub fn offend(&mut self, o: Offence) -> bool {
        let before = self.level();
        self.points = (self.points + o.points()).min(LEVELS[LEVELS.len() - 1] + 3.0);
        self.quiet = 0.0;
        self.level() > before
    }

    /// `dt` s on: true if the heat fell a level (after [`COOL`] s without an offence).
    pub fn cool(&mut self, dt: f32) -> bool {
        let level = self.level();
        if level == 0 {
            self.points = 0.0;
            return false;
        }
        self.quiet += dt;
        if self.quiet < COOL {
            return false;
        }
        self.quiet = 0.0;
        // To the level below's own start (none: nothing).
        self.points = if level >= 2 { LEVELS[usize::from(level) - 2] } else { 0.0 };
        true
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

/// The Watch's fine at `level` of heat, credits (the server's: `bc_econ::wire::watch_fine`).
pub fn fine(level: u8) -> u64 {
    bc_econ::wire::watch_fine(level)
}

/// The heat as the HUD shows it: a diamond a level, the lit ones first ("◆◆◇◇◇"); nothing at 0.
pub fn stars(level: u8) -> String {
    if level == 0 {
        return String::new();
    }
    (0..MAX_HEAT).map(|k| if k < level { '◆' } else { '◇' }).collect()
}

/// Whether one of the Watch's own cars on its beat is within `reach` m of `at` at tick `tick`.
pub fn watch_car_near(at: DeckPos, reach: f32, (tick, frac): (u32, f32)) -> bool {
    let t = town::town_t(at.s);
    let mut near = false;
    street::each_car(at.x - reach, at.x + reach, t - reach, t + reach, tick, frac, &mut |c| {
        near |= c.kind == street::Kind::Watch && (c.x - at.x).hypot(c.t - t) <= reach;
    });
    near
}

/// Whether a car driven at `at` is where people walk: on a block, a pavement or a rim walk.
pub fn where_people_walk(at: DeckPos) -> bool {
    matches!(
        town::ground_at(at.x, town::town_t(at.s)),
        town::Ground::Block(_) | town::Ground::Pavement | town::Ground::RimWalk
    )
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

/// The Watch's cars after one pilot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pursuit {
    pub units: Vec<Unit>,
    /// How long one of them has had the pilot stopped at its side, s.
    held: f32,
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
        let want = UNITS[usize::from(heat.min(MAX_HEAT))];
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
        self.held >= CATCH.2
    }

    /// Where a car at `(x, s)` drives for next on its way to a pilot at `at`: straight at them once
    /// it's on their road or their cross street and near; else along its road to the cross street
    /// nearest them, along that to the road nearest them, and in.
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
        // Along the cross street to the pilot's road.
        (road_near(at.x), cross)
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
    fn heat_rises_with_what_a_pilot_does_and_falls_when_they_stop() {
        let mut h = Heat::default();
        assert_eq!((h.level(), stars(h.level())), (0, String::new()));
        assert!(!h.offend(Offence::TookACar), "a car from a kerb, unseen: nothing yet");
        assert!(h.offend(Offence::Knock));
        assert_eq!((h.level(), stars(h.level()).as_str()), (1, "◆◇◇◇◇"));
        assert!(!h.offend(Offence::Knock), "still the first level");
        assert!(h.offend(Offence::Knock));
        assert_eq!(h.level(), 2);
        // In sight of the Watch it's worse; and it never goes past the top.
        for _ in 0..20 {
            h.offend(Offence::TookACarInSight);
        }
        assert_eq!((h.level(), stars(h.level()).as_str()), (MAX_HEAT, "◆◆◆◆◆"));
        assert_eq!((fine(1), fine(3), h.fine()), (150, 1_350, 3_750));
        // Quiet, it falls a level every while; an offence starts the wait again.
        let mut fell = 0;
        for k in 0..(COOL * 60.0 * 2.5) as u32 {
            fell += u32::from(h.cool(1.0 / 60.0));
            if k == (COOL * 60.0 * 1.5) as u32 {
                h.offend(Offence::Reckless);
                assert_eq!(h.level(), 4);
            }
        }
        assert_eq!((fell, h.level()), (1, 4), "one level in a quiet stretch, then the wait began again");
        for _ in 0..(COOL * 60.0 * 5.0) as u32 {
            h.cool(1.0 / 60.0);
        }
        assert_eq!(h.level(), 0);
        h.offend(Offence::TookACarInSight);
        h.clear();
        assert_eq!(h.level(), 0);
    }

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
        assert!(where_people_walk(DeckPos::new(RING, -300.0, town::deck_s(town::CELL * 30.5), 0.0)));
        assert!(!where_people_walk(at));
    }
}
