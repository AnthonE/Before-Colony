//! What's been taken out of the ring's town's crowd, as this client has it (`docs/FREE_ROAM.md`,
//! P2's taken set): the server's word ([`Said`], from `Update::Street`: the districts round the
//! pilot, each whole, and the traffic's cars taken anywhere round the ring), and what the pilot's own
//! hands did a moment ago that the word hasn't caught up with ([`Mine`]), as the pilot's own hit is
//! drawn at once: a kerb they took a car from, a car of the traffic they took, the car they left,
//! one they took where it was left. Every screen draws what the server holds, the pilot's own a
//! moment sooner; whatever the server didn't take is forgotten after [`GRACE`].

use bc_econ::wire::{Down, LeftCar, TakenDistrict};
use bc_sim::TICK_HZ;
use bc_sim::colony::deck::{self, RING};

/// The server's word on the town's street.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Said {
    /// The districts round the pilot, as last sent.
    pub districts: Vec<TakenDistrict>,
    /// The traffic's cars taken out of their lanes (`bc_sim::colony::street::Car::id`).
    pub lanes: Vec<u32>,
}

impl Said {
    /// Takes in an `Update::Street`: the districts `near` the pilot now (any other is forgotten),
    /// those of them sent, whole, and the traffic's cars taken if they were sent.
    pub fn apply(&mut self, near: &[u16], districts: Vec<TakenDistrict>, lanes: Option<Vec<u32>>) {
        self.districts.retain(|d| near.contains(&d.district));
        for d in districts.into_iter().filter(|d| near.contains(&d.district)) {
            match self.districts.iter_mut().find(|o| o.district == d.district) {
                Some(o) => *o = d,
                None => self.districts.push(d),
            }
        }
        if let Some(lanes) = lanes {
            self.lanes = lanes;
        }
    }

    /// Who's down (or away as a fare) in the districts round the pilot.
    pub fn down(&self) -> Vec<Down> {
        self.districts.iter().flat_map(|d| d.down.iter().copied()).collect()
    }

    /// The kerbs whose cars have been driven off (`bc_sim::colony::street::kerb`).
    pub fn kerbs(&self) -> impl Iterator<Item = u32> + '_ {
        self.districts.iter().flat_map(|d| d.kerbs.iter().copied())
    }

    /// The cars left about.
    pub fn left(&self) -> impl Iterator<Item = &LeftCar> + '_ {
        self.districts.iter().flat_map(|d| d.left.iter())
    }
}

/// How long the pilot's own word stands without the server's, ticks.
pub const GRACE: u32 = 4 * TICK_HZ;
/// How near where the pilot left a car the server's car left there must stand to be that one, m.
const SAME_CAR: f32 = 2.5;

/// What the pilot's own hands did that the server's word may not have yet: each with the tick it's
/// forgotten at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mine {
    kerbs: Vec<(u32, u32)>,
    lanes: Vec<(u32, u32)>,
    /// Cars left about they took, by the server's id.
    took: Vec<(u32, u32)>,
    /// The car they left (its `id` 0: not the server's yet), and whether they took it back before
    /// the server's word of it came.
    left: Option<(LeftCar, u32, bool)>,
}

impl Mine {
    /// They drove off with the car parked at `kerb` at tick `tick`.
    pub fn took_parked(&mut self, kerb: u32, tick: u32) {
        self.kerbs.push((kerb, tick + GRACE));
    }

    /// They took car `car` out of the traffic.
    pub fn jacked(&mut self, car: u32, tick: u32) {
        self.lanes.push((car, tick + GRACE));
    }

    /// They took the car left about that's `id` (0: the one they'd just left themselves, before the
    /// server's word of it came: [`Mine::settle`] takes it when it does).
    pub fn took_left(&mut self, id: u32, tick: u32) {
        if id == 0 {
            if let Some(l) = self.left.as_mut() {
                l.2 = true;
            }
            return;
        }
        self.took.push((id, tick + GRACE));
    }

    /// They left `car` (where, which way, where it came from) at tick `tick`.
    pub fn left(&mut self, car: LeftCar, tick: u32) {
        self.left = Some((LeftCar { id: 0, ..car }, tick + GRACE, false));
    }

    /// Forgets what the server's word `said` now has (or has been without for long enough) at tick
    /// `tick`. If the car they just left and took back again is now the server's, its id: to be
    /// asked for (`Request::TakeLeft`).
    pub fn settle(&mut self, said: &Said, tick: u32) -> Option<u32> {
        self.kerbs.retain(|&(k, until)| until > tick && !said.kerbs().any(|o| o == k));
        self.lanes.retain(|&(c, until)| until > tick && !said.lanes.contains(&c));
        self.took.retain(|&(id, until)| until > tick && said.left().any(|o| o.id == id));
        let (car, until, back) = self.left?;
        let theirs = said.left().find(|o| near(o, &car) && !self.took.iter().any(|t| t.0 == o.id));
        if let Some(o) = theirs {
            self.left = None;
            if back {
                self.took.push((o.id, tick + GRACE));
                return Some(o.id);
            }
        } else if until <= tick {
            self.left = None;
        }
        None
    }

    /// The kerbs empty: the server's, and the pilot's own.
    pub fn kerbs(&self, said: &Said) -> Vec<u32> {
        let mut all: Vec<u32> = said.kerbs().collect();
        for &(k, _) in &self.kerbs {
            if !all.contains(&k) {
                all.push(k);
            }
        }
        all
    }

    /// The traffic's cars taken: the server's, and the pilot's own.
    pub fn lanes(&self, said: &Said) -> Vec<u32> {
        let mut all = said.lanes.clone();
        for &(c, _) in &self.lanes {
            if !all.contains(&c) {
                all.push(c);
            }
        }
        all
    }

    /// The cars left about: the server's (but those the pilot's just taken), and the one they just
    /// left (unless they've taken it back), `id` 0.
    pub fn left_about(&self, said: &Said) -> Vec<LeftCar> {
        let mut all: Vec<LeftCar> =
            said.left().filter(|o| !self.took.iter().any(|t| t.0 == o.id)).copied().collect();
        if let Some((car, _, false)) = self.left {
            all.push(car);
        }
        all
    }
}

/// Whether two cars left about stand in the same place.
fn near(a: &LeftCar, b: &LeftCar) -> bool {
    (a.x - b.x).hypot(deck::round_from(RING, a.s, b.s)) <= SAME_CAR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn district(d: u16, version: u64, kerbs: Vec<u32>, left: Vec<LeftCar>) -> TakenDistrict {
        TakenDistrict { district: d, version, down: Vec::new(), kerbs, left }
    }

    fn car(id: u32, x: f32, s: f32) -> LeftCar {
        LeftCar { id, x, s, yaw: 0.0, kerb: Some(5), car: None, by: 1, at: 0 }
    }

    #[test]
    fn the_servers_word_keeps_the_districts_round_the_pilot_whole() {
        let mut said = Said::default();
        said.apply(
            &[11, 0, 1],
            vec![district(11, 1, vec![], vec![]), district(0, 3, vec![7], vec![])],
            Some(vec![9]),
        );
        said.apply(&[11, 0, 1], vec![district(1, 1, vec![8], vec![])], None);
        assert_eq!(said.kerbs().collect::<Vec<_>>(), [7, 8]);
        assert_eq!(said.lanes, [9], "as last sent");
        // On into district 1: 11 forgotten, 0 and 1 kept, 2 new; a district not near isn't taken.
        said.apply(
            &[0, 1, 2],
            vec![district(2, 0, vec![], vec![]), district(5, 2, vec![99], vec![])],
            Some(vec![]),
        );
        let held: Vec<u16> = said.districts.iter().map(|d| d.district).collect();
        assert_eq!(held, [0, 1, 2]);
        assert_eq!(said.kerbs().collect::<Vec<_>>(), [7, 8]);
        // 0 sent again, changed.
        said.apply(&[0, 1, 2], vec![district(0, 4, vec![], vec![car(3, 1.0, 2.0)])], None);
        assert_eq!(said.kerbs().collect::<Vec<_>>(), [8]);
        assert_eq!(said.left().count(), 1);
    }

    #[test]
    fn the_pilots_own_hands_show_at_once_till_the_servers_word_comes() {
        let mut said = Said::default();
        let mut mine = Mine::default();
        mine.took_parked(7, 100);
        mine.jacked(65_540, 100);
        assert_eq!((mine.kerbs(&said), mine.lanes(&said)), (vec![7], vec![65_540]));
        // The server has the kerb; it never had the car of the traffic (somebody else's): forgotten.
        said.apply(&[0], vec![district(0, 1, vec![7], vec![])], None);
        mine.settle(&said, 101);
        assert_eq!(mine.kerbs(&said), [7]);
        assert_eq!(mine.lanes(&said), [65_540]);
        mine.settle(&said, 100 + GRACE);
        assert_eq!((mine.kerbs(&said), mine.lanes(&said)), (vec![7], vec![]));
        // They leave it: drawn where they left it at once, then the server's in its place.
        mine.left(car(0, 10.0, 500.0), 200);
        assert_eq!(mine.left_about(&said), [car(0, 10.0, 500.0)]);
        said.apply(&[0], vec![district(0, 2, vec![7], vec![car(4, 10.0, 500.2)])], None);
        assert_eq!(mine.settle(&said, 201), None);
        assert_eq!(mine.left_about(&said), [car(4, 10.0, 500.2)]);
        // Somebody's car taken where it was left: gone at once, and for good when the server says.
        mine.took_left(4, 300);
        assert!(mine.left_about(&said).is_empty());
        said.apply(&[0], vec![district(0, 3, vec![7], vec![])], None);
        mine.settle(&said, 301);
        assert!(mine.left_about(&said).is_empty() && mine.took.is_empty());
        // Left and taken back before the server's word: when it comes, it's asked for.
        mine.left(car(0, -3.0, 40.0), 400);
        mine.took_left(0, 401);
        assert!(mine.left_about(&said).is_empty());
        said.apply(&[0], vec![district(0, 4, vec![7], vec![car(6, -3.0, 40.0)])], None);
        assert_eq!(mine.settle(&said, 402), Some(6));
        assert!(mine.left_about(&said).is_empty(), "theirs again");
    }
}
