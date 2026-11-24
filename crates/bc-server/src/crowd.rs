//! The ring's town's people out of its crowd, as the server keeps them (`docs/FREE_ROAM.md`, 3: the
//! promoted tier, P2's second half). The crowd is a closed form nobody stores
//! (`bc_sim::colony::street`); once something is done by one of its people they're the server's
//! (`bc_sim::colony::crowd::Loose`, from where the crowd had them on that tick), stepped by the
//! street's task ten times a second on the sector's clock, and sent to everyone near them, till
//! they've settled and both they and the place the crowd would have them are out of everyone's
//! sight: then the crowd has them back.
//!
//! What takes them out of it: a car driven at speed where people walk (those ahead of it run), a
//! car taken from its driver and someone run down (those round it run), and getting up again once
//! run down (they stand about a moment, then walk on).
//!
//! In a fixed pool sized at start-up ([`MAX_LOOSE`]): full, a new one takes the place of the settled
//! one furthest from every pilot, or isn't taken out of the crowd at all.

use bc_econ::wire::{Placed, Promoted};
use bc_sim::colony::crowd::{Loose, round_dt};
use bc_sim::colony::{street, town};

use crate::street::{DISTRICTS, SIGHT, district_at};

/// The most out of the crowd at once.
pub const MAX_LOOSE: usize = 512;
/// A pilot down in the town is sent those within this far of them whole, m, at most this many
/// (nearest first)...
pub const SEND_NEAR: f32 = 130.0;
pub const NEAR_MOST: usize = 64;
/// ...and past those, out to this far (further than any screen draws the crowd's people: the life
/// tiers' 180 m, half as far again in the town), placed in a few numbers, at most this many more.
pub const SEND_MID: f32 = 320.0;
pub const MID_MOST: usize = 256;
/// How often the settled are looked over for letting go, ticks.
const RELEASE_EVERY: u32 = bc_sim::TICK_HZ;

/// One out of the crowd, and where the crowd would have them now (`(x, t)`, last looked).
#[derive(Clone, Copy, Debug)]
struct Entry {
    loose: Loose,
    home: (f32, f32),
}

/// Who's out of the crowd: a fixed pool.
pub struct Crowd {
    slots: Box<[Option<Entry>]>,
    /// Where the pilots down in the town were at the last step (`(x, t)` on the town's floor).
    sights: Vec<(f32, f32)>,
    released: Option<u32>,
}

impl Default for Crowd {
    fn default() -> Self {
        Self { slots: vec![None; MAX_LOOSE].into_boxed_slice(), sights: Vec::new(), released: None }
    }
}

/// How far apart two places on the town's floor (`(x, t)`) are, m, the short way round.
fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(round_dt(a.1, b.1))
}

impl Crowd {
    /// Whether the person `id` is out of the crowd.
    pub fn has(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    /// The person `id`, out of the crowd.
    pub fn get(&self, id: u32) -> Option<&Loose> {
        self.slots.iter().flatten().map(|e| &e.loose).find(|l| l.id == id)
    }

    /// How many are out of it.
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The person `id` taken out of the pool (run down: the street has them now).
    pub fn take(&mut self, id: u32) -> Option<Loose> {
        let slot = self.slots.iter_mut().find(|s| s.is_some_and(|e| e.loose.id == id))?;
        slot.take().map(|e| e.loose)
    }

    /// `loose` out of the crowd from now: in a free place, or the place of the settled one
    /// furthest from every pilot. False if there's none to give up (or they're out already).
    pub fn put(&mut self, loose: Loose) -> bool {
        if self.has(loose.id) {
            return false;
        }
        let entry = Entry { loose, home: (loose.x, loose.t) };
        if let Some(free) = self.slots.iter_mut().find(|s| s.is_none()) {
            *free = Some(entry);
            return true;
        }
        let sights = &self.sights;
        let nearest =
            |e: &Entry| sights.iter().map(|&p| apart(p, (e.loose.x, e.loose.t))).fold(f32::MAX, f32::min);
        let furthest = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(k, s)| s.filter(|e| e.loose.settled()).map(|e| (k, nearest(&e))))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((k, _)) = furthest else { return false };
        self.slots[k] = Some(entry);
        true
    }

    /// Something frightening at `from` on the town's floor (`(x, t)`) at tick `tick`: everyone
    /// within `reach` m of `around` runs from it, those of the crowd's (but `lying`, run down) taken
    /// out of it. How many of the crowd's were.
    pub fn scare(
        &mut self,
        from: (f32, f32),
        (x, t): (f32, f32),
        reach: f32,
        tick: u32,
        lying: &impl Fn(u32) -> bool,
    ) -> usize {
        for e in self.slots.iter_mut().flatten() {
            if apart((e.loose.x, e.loose.t), (x, t)) <= reach {
                e.loose.scare(from, tick);
            }
        }
        let mut found = Vec::new();
        street::each_person(x - reach, x + reach, t - reach, t + reach, tick, 0.0, &mut |p| {
            if (p.x - x).hypot(p.t - t) <= reach {
                found.push(*p);
            }
        });
        let mut n = 0;
        for p in found {
            if lying(p.id) || self.has(p.id) {
                continue;
            }
            let Some(mut loose) = Loose::from_crowd(&p) else { continue };
            loose.scare(from, tick);
            if self.put(loose) {
                n += 1;
            }
        }
        n
    }

    /// At tick `tick`, `dt` s on from the last, with the pilots down in the town at `sights`
    /// (`(x, t)`): everyone on, and (once a second) the settled let go whose place and whose place
    /// in the crowd are both out of everyone's sight.
    pub fn step(&mut self, tick: u32, dt: f32, sights: &[(f32, f32)]) {
        self.sights.clear();
        self.sights.extend_from_slice(sights);
        for e in self.slots.iter_mut().flatten() {
            e.loose.step(tick, dt);
        }
        if self.released.is_some_and(|r| tick.wrapping_sub(r) < RELEASE_EVERY) {
            return;
        }
        self.released = Some(tick);
        let seen = |at: (f32, f32)| sights.iter().any(|&p| apart(p, at) <= SIGHT);
        for slot in self.slots.iter_mut() {
            let Some(e) = slot.as_mut() else { continue };
            if let Some(p) = street::person(e.loose.id, tick) {
                e.home = (p.x, p.t);
            }
            if e.loose.settled() && !seen((e.loose.x, e.loose.t)) && !seen(e.home) {
                *slot = None;
            }
        }
    }

    /// What a pilot at `(x, t)` on the town's floor is sent: those within [`SEND_NEAR`] whole,
    /// nearest first, at most [`NEAR_MOST`]; then those (or their places in the crowd) within
    /// [`SEND_MID`], placed; and how many more there are in the districts round them.
    pub fn near(&self, (x, t): (f32, f32)) -> (Vec<Promoted>, Vec<Placed>, u32) {
        let mut by: Vec<(f32, &Entry)> =
            self.slots.iter().flatten().map(|e| (apart((e.loose.x, e.loose.t), (x, t)), e)).collect();
        by.sort_by(|a, b| a.0.total_cmp(&b.0));
        let districts = crate::street::near(town::deck_s(t));
        let (mut near, mut mid, mut far) = (Vec::new(), Vec::new(), 0);
        for (d, e) in by {
            let l = &e.loose;
            let p = Promoted {
                id: l.id,
                x: l.x,
                s: town::deck_s(l.t),
                yaw: l.yaw,
                speed: l.speed,
                scared: l.scared(),
            };
            if d <= SEND_NEAR && near.len() < NEAR_MOST {
                near.push(p);
            } else if (d <= SEND_MID || apart(e.home, (x, t)) <= SEND_MID) && mid.len() < MID_MOST {
                mid.push(p.placed());
            } else if districts.contains(&(district_at(p.s) as u16)) {
                far += 1;
            }
        }
        (near, mid, far)
    }

    /// For `/status`: how many are out of the crowd, how many of them running scared, and how many
    /// in each district.
    pub fn counts(&self) -> (usize, usize, [u16; DISTRICTS]) {
        let mut by = [0u16; DISTRICTS];
        let mut scared = 0;
        for e in self.slots.iter().flatten() {
            by[district_at(town::deck_s(e.loose.t))] += 1;
            scared += usize::from(e.loose.scared());
        }
        (self.len(), scared, by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::street::Person;

    /// The crowd's people in a cell of a busy district, at tick `tick`.
    fn crowd_at(tick: u32) -> Vec<Person> {
        let t0 = town::CELL * 30.0;
        let mut all = Vec::new();
        street::each_person(-300.0, 300.0, t0, t0 + town::CELL, tick, 0.0, &mut |p| all.push(*p));
        all
    }

    #[test]
    fn a_fright_takes_those_round_it_out_of_the_crowd_and_they_go_back_out_of_sight() {
        let tick = 50_000;
        let people = crowd_at(tick);
        let p = people.iter().find(|p| p.pose == street::Pose::Walk && p.id >> 28 != 0xF).copied().unwrap();
        let mut crowd = Crowd::default();
        let here = (p.x + 2.0, p.t);
        let n = crowd.scare(here, here, 15.0, tick, &|_| false);
        assert!(n >= 1 && crowd.has(p.id), "{n}");
        // Nobody already out of it (nor anyone lying in the road) is taken twice.
        assert_eq!(crowd.scare(here, here, 15.0, tick, &|id| id == p.id), 0);
        let l = *crowd.get(p.id).unwrap();
        assert!(l.scared() && (l.x - p.x).abs() < 1e-2);
        // A pilot near: they run, then settle, but stay the server's while in sight.
        let sights = [here];
        for k in 0..200u32 {
            crowd.step(tick + 3 * k, 0.1, &sights);
        }
        let l = *crowd.get(p.id).expect("still out of the crowd, in sight");
        assert!(l.settled());
        // Sent to the pilot near them whole, and to one further off placed.
        let (near, mid, _) = crowd.near(here);
        assert!(near.iter().any(|q| q.id == p.id) && mid.is_empty());
        let (near, mid, _) = crowd.near((here.0, here.1 + 250.0));
        assert!(near.is_empty() && mid.iter().any(|q| q.0 == p.id));
        // Everyone gone off: they're let go.
        crowd.step(tick + 700, 0.1, &[]);
        assert!(crowd.is_empty(), "{}", crowd.len());
    }

    #[test]
    fn a_full_pool_gives_up_the_settled_one_furthest_off() {
        let p = crowd_at(1_000)[0];
        let mut crowd = Crowd::default();
        for k in 0..MAX_LOOSE as u32 {
            let q = Person { id: p.id + 16 * k, t: p.t + k as f32, ..p };
            assert!(crowd.put(Loose::from_crowd(&q).unwrap()));
        }
        // (Where the pilots were at the last step.)
        crowd.sights = vec![(p.x, p.t)];
        let furthest = p.id + 16 * (MAX_LOOSE as u32 - 1);
        assert!(crowd.has(furthest));
        let other = Person { id: p.id + 16 * MAX_LOOSE as u32, ..p };
        assert!(crowd.put(Loose::from_crowd(&other).unwrap()));
        assert_eq!(crowd.len(), MAX_LOOSE);
        assert!(!crowd.has(furthest) && crowd.has(other.id));
        // None settled: nobody's given up.
        for e in crowd.slots.iter_mut().flatten() {
            e.loose.scare((p.x, p.t), 1_000);
        }
        let last = Person { id: other.id + 16, ..p };
        assert!(!crowd.put(Loose::from_crowd(&last).unwrap()));
    }
}
