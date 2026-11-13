//! The ring's town's street as the server keeps it (`docs/FREE_ROAM.md`, the first step of P2): the
//! town's people are closed forms nobody stores, but one a pilot's car knocks down is the server's
//! from then on, till they get up, so everyone in the town sees the same person lying in the same
//! place. Off the sector's tick, like the plaza.
//!
//! A hit is a pilot's word ([`bc_econ::wire::Request::Hit`]), taken only if it could be: they're at
//! the wheel in the town, and the person named was within a car's length or two of where the plaza
//! last had them, at the sector's tick or a moment before (`bc_sim::colony::street`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::wire::Down;
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_sim::TICK_HZ;
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::{street, town};

/// How long someone knocked down lies there before they get up, ticks.
pub const DOWN_TICKS: u32 = 25 * TICK_HZ;
/// The most people down at once: the oldest gets up to make room.
pub const MAX_DOWN: usize = 64;
/// How far from the car the person named may have been, m: a pose is up to a few metres old at a
/// car's speed.
const REACH: f32 = 8.0;
/// How far back the hit may have been, ticks, a few at a time.
const BACK: [u32; 4] = [0, 6, 12, 20];

/// The town's people down, and a count that moves whenever they change.
#[derive(Default)]
pub struct Street {
    down: Mutex<Vec<Down>>,
    version: AtomicU64,
}

impl Street {
    /// A pilot at `pose` (as the plaza last had them) says their car hit `person` at about tick
    /// `tick`: true if it could be, and that person is down from now (false too if they already
    /// are).
    pub fn hit(&self, person: u32, pose: &PersonPose, tick: u32) -> bool {
        if pose.strip != RING_DECK || !pose.driving() {
            return false;
        }
        let t = town::town_t(pose.s);
        let mut found = None;
        for back in BACK {
            street::each_person(
                pose.x - REACH,
                pose.x + REACH,
                t - REACH,
                t + REACH,
                tick.saturating_sub(back),
                0.0,
                &mut |p| {
                    if p.id == person && (p.x - pose.x).hypot(p.t - t) <= REACH {
                        found = found.or(Some(*p));
                    }
                },
            );
            if found.is_some() {
                break;
            }
        }
        let Some(p) = found else { return false };
        let Ok(mut down) = self.down.lock() else { return false };
        down.retain(|d| d.until > tick);
        if down.iter().any(|d| d.id == person) {
            return false;
        }
        if down.len() >= MAX_DOWN {
            down.remove(0);
        }
        // Where they were, in the deck's own `s`; knocked the way the car was going.
        let s = town::deck_s(p.t);
        down.push(Down { id: p.id, x: p.x, s, yaw: pose.yaw, until: tick + DOWN_TICKS, away: false });
        self.version.fetch_add(1, Ordering::Release);
        true
    }

    /// A pilot stopped at `pose` takes `person` as a fare: someone standing in a lift's square
    /// within hail of their car, not down and not already gone. They're off the street from now
    /// (till [`Street::set_down`], or [`FARE_TICKS`]); the lift whose square they stood in.
    pub fn pick_up(&self, person: u32, pose: &PersonPose, tick: u32) -> Option<u16> {
        if pose.strip != RING_DECK || !pose.driving() || pose.speed > 3.0 {
            return None;
        }
        let t = town::town_t(pose.s);
        let mut found = None;
        street::each_person(pose.x - HAIL, pose.x + HAIL, t - HAIL, t + HAIL, tick, 0.0, &mut |p| {
            if p.id == person && p.pose == street::Pose::Stand && (p.x - pose.x).hypot(p.t - t) <= HAIL {
                found = Some(*p);
            }
        });
        let p = found?;
        let from = lift_near(p.x, town::deck_s(p.t), SQUARE)?;
        let mut down = self.down.lock().ok()?;
        down.retain(|d| d.until > tick);
        if down.iter().any(|d| d.id == person) {
            return None;
        }
        if down.len() >= MAX_DOWN {
            down.remove(0);
        }
        let s = town::deck_s(p.t);
        down.push(Down { id: p.id, x: p.x, s, yaw: p.yaw, until: tick + FARE_TICKS, away: true });
        self.version.fetch_add(1, Ordering::Release);
        Some(from)
    }

    /// A fare set down (or lost): back on the street, where the town has them.
    pub fn set_down(&self, person: u32) {
        let Ok(mut down) = self.down.lock() else { return };
        let before = down.len();
        down.retain(|d| !(d.away && d.id == person));
        if down.len() != before {
            self.version.fetch_add(1, Ordering::Release);
        }
    }

    /// Moves whenever who's down changes.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Who's down at tick `tick` (those whose time is up get up).
    pub fn view(&self, tick: u32) -> Vec<Down> {
        let Ok(mut down) = self.down.lock() else { return Vec::new() };
        let before = down.len();
        down.retain(|d| d.until > tick);
        if down.len() != before {
            self.version.fetch_add(1, Ordering::Release);
        }
        down.clone()
    }

    /// How many are down (for `/status`).
    pub fn count(&self) -> usize {
        self.down.lock().map_or(0, |d| d.len())
    }
}

/// How near a stopped car someone hails it from, m; how near a lift's step counts as its square;
/// and how long a fare stays off the street at most, ticks.
pub const HAIL: f32 = 12.0;
pub const SQUARE: f32 = 190.0;
pub const FARE_TICKS: u32 = 20 * 60 * TICK_HZ;
/// How near the lift's step a fare is set down, m.
pub const SET_DOWN: f32 = 60.0;

/// The lift whose step is within `reach` m of `(x, s)` on the ring's floor, if one is.
pub fn lift_near(x: f32, s: f32, reach: f32) -> Option<u16> {
    (0..town::LIFTS).find(|k| {
        let (lx, lt) = town::out_of_the_lift(*k);
        apart((x, s), (lx, town::deck_s(lt))) <= reach
    })
}

/// Where the fare `person` from lift `from`'s square asks to go, and what they pay: another lift's
/// square, one to three round from theirs, [`bc_econ::wire::FARE_PER_KM`] a kilometre of the way.
pub fn fare_for(person: u32, from: u16) -> (u16, u64) {
    let hops = 1 + (town::mix(person, u32::from(from), 0xFA2E) % 3) as u16;
    let back = town::mix(person, 7, 0xFA2E) & 1 == 1;
    let to = if back { (from + town::LIFTS - hops) % town::LIFTS } else { (from + hops) % town::LIFTS };
    let km = f32::from(hops) * town::ROUND / f32::from(town::LIFTS) / 1_000.0;
    (to, (km * bc_econ::wire::FARE_PER_KM as f32).round() as u64)
}

/// What lift `k`'s square is called: Hub Gate, or the district it opens on.
pub fn gate_name(k: u16) -> &'static str {
    town::lift_name(k)
}

/// How far apart two places on the ring's floor are, m (the short way round).
pub fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(round_from(RING, a.1, b.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::presence::RIDE_CAR;

    #[test]
    fn a_hit_is_taken_only_from_a_car_beside_the_person_and_they_get_up_again() {
        // Somebody walking near Hub Gate at tick 900.
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 200.0, x + 200.0, t - 200.0, t + 200.0, 900, 0.0, &mut |p| {
            if p.pose == street::Pose::Walk {
                who = who.or(Some(*p));
            }
        });
        let p = who.expect("somebody about");
        let at = |dx: f32, ride: u8| PersonPose {
            strip: RING_DECK,
            x: p.x + dx,
            s: town::deck_s(p.t),
            ride,
            grounded: true,
            ..PersonPose::default()
        };
        let street = Street::default();
        // On foot beside them: no. In a car 40 m off: no. Somebody who isn't there: no.
        assert!(!street.hit(p.id, &at(1.0, 0), 900));
        assert!(!street.hit(p.id, &at(40.0, RIDE_CAR), 900));
        assert!(!street.hit(p.id ^ 0x5555, &at(1.0, RIDE_CAR), 900));
        assert_eq!((street.version(), street.count()), (0, 0));
        // In a car at their side, a few ticks on: down, where they were, once.
        assert!(street.hit(p.id, &at(2.0, RIDE_CAR), 905));
        assert!(!street.hit(p.id, &at(2.0, RIDE_CAR), 906), "already down");
        let down = street.view(906);
        assert_eq!(down.len(), 1);
        assert!(apart((down[0].x, down[0].s), (p.x, town::deck_s(p.t))) < 1.5, "{:?} vs {p:?}", down[0]);
        assert_eq!(down[0].until, 905 + DOWN_TICKS);
        let seen = street.version();
        // Their time up, they're gone from the list, and the count has moved for whoever watches.
        assert!(street.view(905 + DOWN_TICKS + 1).is_empty());
        assert!(street.version() > seen);
    }

    #[test]
    fn a_fare_is_someone_standing_in_a_lifts_square_by_a_stopped_car() {
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 110.0, x - 15.0, t + 15.0, t + 110.0, 0, 0.0, &mut |p| {
            if p.pose == street::Pose::Stand {
                who = who.or(Some(*p));
            }
        });
        let p = who.expect("somebody standing in Hub Gate's square");
        let by = |dx: f32, speed: f32, ride: u8| PersonPose {
            strip: RING_DECK,
            x: p.x + dx,
            s: town::deck_s(p.t),
            speed,
            ride,
            grounded: true,
            ..PersonPose::default()
        };
        let street = Street::default();
        // Not on foot, not driving past, not from across the square.
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, 0), 0), None);
        assert_eq!(street.pick_up(p.id, &by(3.0, 9.0, RIDE_CAR), 0), None);
        assert_eq!(street.pick_up(p.id, &by(30.0, 0.0, RIDE_CAR), 0), None);
        // Stopped beside them: taken, from Hub Gate's square; gone from the street, and not twice.
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, RIDE_CAR), 0), Some(0));
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, RIDE_CAR), 1), None);
        assert!(street.view(1).iter().any(|d| d.id == p.id && d.away));
        assert!(!street.hit(p.id, &by(1.0, 9.0, RIDE_CAR), 2), "nobody's there to hit");
        // Where to: another gate, one to three round, paid by the way.
        let (to, pay) = fare_for(p.id, 0);
        assert!(to != 0 && to < town::LIFTS);
        let hops = u64::from(to.min(town::LIFTS - to));
        // (A gate to the next is a sixth of the ring: π km.)
        assert_eq!(pay, (hops as f32 * std::f32::consts::PI * 40.0).round() as u64, "{hops} gates round");
        assert_eq!((gate_name(0), gate_name(1)), ("HUB GATE", "TOWER HILL"));
        // Set down, they're back on the street.
        street.set_down(p.id);
        assert!(street.view(3).is_empty());
        assert_eq!(lift_near(x, town::deck_s(t), 5.0), Some(0));
        assert_eq!(lift_near(x, town::deck_s(t) + 800.0, SQUARE), None);
    }
}
