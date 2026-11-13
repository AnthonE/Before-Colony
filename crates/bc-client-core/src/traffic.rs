//! The town's traffic as one client has it: the closed form's cars (`bc_sim::colony::street`),
//! each held back along its lane by whatever stands in its way.
//!
//! The closed form says where every car is at a tick, the same for everyone, and nothing stops it:
//! its cars would drive through a pilot stood in the road and through the car they left across a
//! lane. So each client keeps, for the cars near its own pilot, how far behind the closed form's
//! place each one is ([`Traffic`]): a driver who sees something ahead in their lane brakes for it,
//! stops a little short, and the ones behind queue up; when the way clears they pull off and make
//! up the ground. A car stopped like that is one a pilot can take ([`Traffic::at_hand`],
//! [`Traffic::take`]): it's gone from the traffic, and theirs to drive.
//!
//! This is the ambient tier's answer (`docs/FREE_ROAM.md`, P1): what holds a car is what this
//! client sees (its pilot, the others near, the cars left about, the Watch's), so two clients agree
//! on a queue only as far as they see the same things. Traffic in the tick is P2.

use std::collections::HashMap;

use bc_sim::car::RING_FLOOR;
use bc_sim::colony::deck::{self, DeckPos, RING};
use bc_sim::colony::{street, town};

use crate::vehicle::{Kind, Vehicle};

/// Something in a lane's way: where its middle is (`x` across the ring, `s` round it) and half how
/// much of the lane it takes up, across and along, m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InTheWay {
    pub x: f32,
    pub s: f32,
    pub half: (f32, f32),
}

impl InTheWay {
    /// Somebody on foot.
    pub fn on_foot(x: f32, s: f32) -> Self {
        Self { x, s, half: (0.45, 0.45) }
    }

    /// A car, however it's turned.
    pub fn car(v: &Vehicle) -> Self {
        let (fx, fs) = v.forward();
        let (l, w) = (0.5 * v.spec().length, 0.5 * v.spec().width);
        Self { x: v.x, s: v.s, half: (fx.abs() * l + fs.abs() * w, fs.abs() * l + fx.abs() * w) }
    }
}

/// A car of the traffic where this client has it: the closed form's car with its place and speed
/// as held (`car.t`, `car.speed`), and its `s` in the asker's turn of the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub car: street::Car,
    pub s: f32,
}

impl Shown {
    /// As a car to drive: where it stands, its nose the way it was going.
    pub fn vehicle(&self) -> Vehicle {
        Vehicle::new(Kind::Car, RING_FLOOR, self.s, self.car.x, self.car.yaw)
    }
}

/// How far either side of its lane's middle a driver minds something, past the thing's own half
/// width, m.
const HALF_LANE: f32 = 1.3;
/// Half a car's length, m (`bc_sim::car`'s car).
const CAR_HALF: f32 = 2.2;
/// How far short of what's in its way a car stops, bumper to it, m.
const GAP: f32 = 2.0;
/// How hard a driver brakes, and pulls away again, m/s².
const BRAKE: f32 = 7.0;
const PULL_AWAY: f32 = 3.2;
/// How much faster than its lane a car goes while it has ground to make up.
const CATCH_UP: f32 = 1.35;
/// How far behind its place a car lets itself be held, m: past that it drives on through whatever
/// it was, at its lane's pace (about a minute and a quarter at a street's; nothing waits for ever).
const PATIENCE: f32 = 900.0;
/// A car this slow is stopped, for whoever would take it, m/s.
pub const STOPPED: f32 = 1.2;
/// How many cars a pilot's taken stay gone from the traffic (the oldest come back).
const MAX_TAKEN: usize = 48;

/// The traffic near one pilot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traffic {
    /// Each held car, by its id: where it is (the town's `t`, wrapped), how fast it's going, m/s,
    /// and how far behind the closed form's place that was, m.
    held: HashMap<u32, (f32, f32, f32)>,
    /// Cars taken out of the traffic.
    taken: Vec<u32>,
    /// The cars in reach, as they are now.
    shown: Vec<Shown>,
}

/// Which way a car's lane runs: 1 the way `t` grows.
fn dir(c: &street::Car) -> f32 {
    if c.yaw > 1.0 { 1.0 } else { -1.0 }
}

impl Traffic {
    /// `dt` s on, round a pilot at `at`: the closed form's cars within `reach` m at `tick` plus
    /// `frac`, each braked for whatever of `in_the_way` is ahead in its lane and for the car in
    /// front, and brought back up to its place when nothing is.
    pub fn step(
        &mut self,
        at: DeckPos,
        reach: f32,
        (tick, frac): (u32, f32),
        dt: f32,
        in_the_way: &[InTheWay],
    ) {
        let dt = dt.clamp(1e-3, 1.0);
        let t = town::town_t(at.s);
        // The held ones' places have run on ahead of them: look that much further.
        let most = self.held.values().fold(0.0f32, |a, h| a.max(h.2)) + 60.0;
        let (held, taken) = (&self.held, &self.taken);
        // Each car, where it was a moment ago (its own place if it's held, else the closed
        // form's), and how fast.
        let mut cars: Vec<(street::Car, f32, f32)> = Vec::new();
        let (t0, t1) = (t - reach - most, t + reach + most);
        street::each_car(at.x - reach, at.x + reach, t0, t1, tick, frac, &mut |c| {
            if taken.contains(&c.id) {
                return;
            }
            let d = dir(c);
            let was = match held.get(&c.id) {
                // (Its place in the turn of the ring the closed form's is in.)
                Some(&(place, speed, _)) => {
                    let behind = (d * (c.t - place)).rem_euclid(town::ROUND);
                    (behind <= PATIENCE + 60.0).then_some((c.t - d * behind, speed))
                }
                None => None,
            };
            let (was, speed) = was.unwrap_or((c.t - d * c.speed * dt, c.speed));
            if (c.t - t).abs() <= reach || (was - t).abs() <= reach {
                cars.push((*c, was, speed));
            }
        });
        // Lane by lane, the one in front first: each minds the one ahead as it's just been put.
        cars.sort_by(|a, b| {
            let lane = (a.0.id >> 16).cmp(&(b.0.id >> 16));
            lane.then((dir(&b.0) * b.1).total_cmp(&(dir(&a.0) * a.1)))
        });
        self.shown.clear();
        let mut ahead: Option<(u32, f32)> = None;
        let mut still = HashMap::new();
        for (c, was, speed) in cars {
            let d = dir(&c);
            let lane = c.id >> 16;
            let behind = d * (c.t - was);
            // How far it can go before it must be stopped: to the car in front, and to whatever's
            // in the lane ahead (or against its side: nobody drives off through someone).
            let mut free = f32::MAX;
            if let Some((l, front)) = ahead
                && l == lane
            {
                free = free.min(d * (front - was) - 2.0 * CAR_HALF - GAP);
            }
            // (Held past its patience, it minds nothing but the cars of its own lane.)
            if behind < PATIENCE {
                for b in in_the_way {
                    let off = d * (deck::round_from(RING, at.s, b.s) - (was - t));
                    if (b.x - c.x).abs() < HALF_LANE + b.half.0 && off > -1.0 {
                        free = free.min(off - b.half.1 - CAR_HALF - GAP);
                    }
                }
            }
            let free = free.max(0.0);
            let want = if behind > c.speed * dt + 0.05 { c.speed * CATCH_UP } else { c.speed };
            let may = (2.0 * BRAKE * free).sqrt().min(free / dt);
            let v = want.min(may).min(speed + PULL_AWAY * dt).max(0.0);
            // On by that much, and never ahead of its place.
            let now = was + d * v * dt;
            let behind = d * (c.t - now);
            let (now, v) = if behind <= 0.02 { (c.t, c.speed) } else { (now, v) };
            if behind > 0.02 {
                still.insert(c.id, (now.rem_euclid(town::ROUND), v, behind));
            }
            ahead = Some((lane, now));
            self.shown.push(Shown { car: street::Car { t: now, speed: v, ..c }, s: at.s + (now - t) });
        }
        self.held = still;
    }

    /// The cars in reach, as they are now.
    pub fn cars(&self) -> &[Shown] {
        &self.shown
    }

    /// The cars taken out of it, by id.
    pub fn taken(&self) -> &[u32] {
        &self.taken
    }

    /// How many are held back.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// A car of the traffic stopped within `reach` m of the side of a pilot on foot at `at`: the
    /// nearest, for the taking.
    pub fn at_hand(&self, at: DeckPos, reach: f32) -> Option<Shown> {
        let near = |c: &Shown| {
            let (along, across) = ((at.s - c.s).abs() - CAR_HALF, (at.x - c.car.x).abs() - 0.9);
            along.max(0.0).hypot(across.max(0.0))
        };
        self.shown
            .iter()
            .filter(|c| c.car.speed <= STOPPED && near(c) <= reach)
            .min_by(|a, b| near(a).total_cmp(&near(b)))
            .copied()
    }

    /// The pilot takes car `id` out of the traffic: it's theirs, and gone from here.
    pub fn take(&mut self, id: u32) {
        if self.taken.len() >= MAX_TAKEN {
            self.taken.remove(0);
        }
        self.taken.push(id);
        self.held.remove(&id);
        self.shown.retain(|c| c.car.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stretch of a street between the blocks with a few cars coming, and one of them: the
    /// pilot's place 40 m ahead of it in its lane.
    fn ahead_of_a_car(tick: u32) -> (DeckPos, street::Car) {
        let mut found = None;
        street::each_car(100.0, 500.0, 2_000.0, 6_000.0, tick, 0.0, &mut |c| {
            if found.is_none() && c.kind == street::Kind::Car {
                found = Some(*c);
            }
        });
        let c = found.expect("a car on a street");
        (DeckPos::new(RING, c.x, town::deck_s(c.t + dir(&c) * 40.0), 0.0), c)
    }

    fn run(tr: &mut Traffic, at: DeckPos, from: u32, secs: f32, way: &[InTheWay]) -> u32 {
        let n = (secs * bc_sim::TICK_HZ as f32) as u32;
        for k in 0..n {
            tr.step(at, 120.0, (from + k, 0.0), bc_sim::DT, way);
        }
        from + n
    }

    #[test]
    fn nothing_in_the_way_the_traffic_is_the_closed_forms() {
        let (at, _) = ahead_of_a_car(900);
        let mut tr = Traffic::default();
        let tick = run(&mut tr, at, 900, 5.0, &[]);
        assert_eq!(tr.held(), 0);
        let mut n = 0;
        let t = town::town_t(at.s);
        street::each_car(at.x - 120.0, at.x + 120.0, t - 120.0, t + 120.0, tick - 1, 0.0, &mut |c| {
            let s = tr.cars().iter().find(|s| s.car.id == c.id).expect("every car in reach is shown");
            assert!((s.car.t - c.t).abs() < 1e-3 && s.car.speed == c.speed);
            n += 1;
        });
        assert!(n > 3 && n == tr.cars().len(), "{n} cars, {} shown", tr.cars().len());
    }

    #[test]
    fn a_car_stops_for_someone_in_its_lane_the_next_queues_and_both_drive_on_after() {
        let (at, c) = ahead_of_a_car(900);
        let d = dir(&c);
        let me = [InTheWay::on_foot(at.x, at.s)];
        let mut tr = Traffic::default();
        let mut tick = 900;
        let mut worst = f32::MAX;
        for _ in 0..(25.0 * 30.0) as u32 {
            tick = run(&mut tr, at, tick, bc_sim::DT, &me);
            for s in tr.cars().iter().filter(|s| s.car.x == c.x) {
                // Nobody's driven through: every car of the lane that was behind is still behind.
                let off = d * (at.s - s.s);
                if (0.0..60.0).contains(&off) {
                    worst = worst.min(off);
                }
            }
        }
        assert!(worst > CAR_HALF + 1.0, "a car came within {worst} m of the pilot's middle");
        // The first is stopped a little short; one's queued behind it, a car's length and a gap back.
        let mut lane: Vec<Shown> =
            tr.cars().iter().filter(|s| s.car.x == c.x && d * (at.s - s.s) > 0.0).copied().collect();
        lane.sort_by(|a, b| (d * (at.s - a.s)).total_cmp(&(d * (at.s - b.s))));
        let first = lane[0];
        assert!(
            first.car.speed < 0.2 && (d * (at.s - first.s) - (CAR_HALF + GAP + 0.45)).abs() < 1.0,
            "{first:?}"
        );
        assert!(lane.len() >= 2 && lane[1].car.speed < 0.5, "{} in the queue", lane.len());
        let gap = d * (lane[0].s - lane[1].s);
        assert!((gap - (2.0 * CAR_HALF + GAP)).abs() < 1.0, "the queue's gap is {gap} m");
        // It's there for the taking; a moving one isn't.
        let beside = DeckPos::new(RING, first.car.x + 2.0, first.s, 0.0);
        assert_eq!(tr.at_hand(beside, 2.6).map(|s| s.car.id), Some(first.car.id));
        // Out of the road: they pull away, make up the ground, and are the closed form's again.
        let held = tr.held();
        assert!(held >= 2);
        tick = run(&mut tr, at, tick, 2.0, &[]);
        let moving = tr.cars().iter().find(|s| s.car.id == first.car.id);
        assert!(moving.is_none_or(|s| s.car.speed > 3.0), "{moving:?}");
        run(&mut tr, at, tick, 240.0, &[]);
        assert_eq!(tr.held(), 0, "all caught up");
    }

    #[test]
    fn a_car_taken_is_gone_from_the_traffic_and_one_held_too_long_drives_on() {
        let (at, c) = ahead_of_a_car(900);
        let me = [InTheWay::on_foot(at.x, at.s)];
        let mut tr = Traffic::default();
        let tick = run(&mut tr, at, 900, 20.0, &me);
        let stopped =
            tr.at_hand(DeckPos::new(RING, at.x + 2.0, at.s - dir(&c) * 5.0, 0.0), 4.0).expect("one stopped");
        let v = stopped.vehicle();
        assert!(v.fits(v.s, v.x, v.yaw), "{v:?}");
        tr.take(stopped.car.id);
        let tick = run(&mut tr, at, tick, 3.0, &me);
        assert!(tr.cars().iter().all(|s| s.car.id != stopped.car.id));
        // Stood there for minutes: the queue's patience runs out and it moves again.
        let before: f32 = tr.cars().iter().map(|s| s.car.speed).sum();
        run(&mut tr, at, tick, 200.0, &me);
        let after: f32 = tr.cars().iter().filter(|s| s.car.x == c.x).map(|s| s.car.speed).sum();
        assert!(before < 60.0 && after > 5.0, "before {before}, after {after}");
    }

    #[test]
    fn a_car_left_in_a_lane_holds_it_and_the_lane_beside_it_runs() {
        let (at, c) = ahead_of_a_car(900);
        // Square across, it takes up more of the road than along it.
        let across =
            InTheWay::car(&Vehicle::new(Kind::Car, RING_FLOOR, at.s, at.x, std::f32::consts::FRAC_PI_2));
        assert!(across.half.0 > 2.0 && across.half.1 < 1.0, "{across:?}");
        // Left in the lane, nose the way the lane runs.
        let way = [InTheWay::car(&Vehicle::new(Kind::Car, RING_FLOOR, at.s, at.x, c.yaw))];
        let mut tr = Traffic::default();
        let mut slowest_beside = f32::MAX;
        let mut tick = 900;
        for _ in 0..(20.0 * 30.0) as u32 {
            tick = run(&mut tr, at, tick, bc_sim::DT, &way);
            for s in tr.cars().iter().filter(|s| (2.0..8.0).contains(&(s.car.x - c.x).abs())) {
                slowest_beside = slowest_beside.min(s.car.speed);
            }
        }
        assert!(tr.cars().iter().any(|s| s.car.x == c.x && s.car.speed < 0.2), "its lane is held");
        assert!(
            slowest_beside > 3.0 && slowest_beside < f32::MAX,
            "the lane beside it: {slowest_beside} m/s"
        );
    }
}
