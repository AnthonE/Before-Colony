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
        down.push(Down { id: p.id, x: p.x, s, yaw: pose.yaw, until: tick + DOWN_TICKS });
        self.version.fetch_add(1, Ordering::Release);
        true
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
}
