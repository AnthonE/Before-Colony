//! The ring's town's people out of its crowd, as this client draws them (`docs/FREE_ROAM.md`, 3: the
//! promoted tier; the server's `crowd`): the server's word on those round the pilot
//! (`Update::Crowd`), each eased from where it's drawn to where it's said to be till the next word
//! comes, as the Watch's units are (`crate::watch::Pursuit`). The crowd's closed form draws everyone
//! but these; these are drawn where the server has them, in their own look (`street::seed_of`).

use bc_econ::wire::{Placed, Promoted};
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::street::{self, Person, Pose};

/// How long a word of the server's is drawn towards, s: about till the next comes.
const EASE: f32 = 0.35;
/// Further than this from where they're drawn, m, someone is put where they're said to be.
const SNAP: f32 = 30.0;
/// A stride walking, m, and how much longer it gets for each m/s faster (`bc_sim::colony::crowd`'s).
const STRIDE: f32 = 1.45;
const STRIDE_MORE: f32 = 0.35;

/// One of them as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub id: u32,
    pub seed: u32,
    /// Where they're drawn on the ring's floor (across, round: the deck's `s`), the way they face,
    /// how fast they go (the server's word near; how fast they're drawn moving further off) and
    /// where they are in their stride.
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    pub speed: f32,
    pub stride: f32,
    pub scared: bool,
    /// Said whole (near the pilot), or only placed (further off).
    pub whole: bool,
    from: (f32, f32, f32),
    to: (f32, f32, f32),
    u: f32,
}

impl Drawn {
    fn new(p: &Promoted, whole: bool) -> Drawn {
        let to = (p.x, p.s, p.yaw);
        Drawn {
            id: p.id,
            seed: street::seed_of(p.id),
            x: p.x,
            s: p.s,
            yaw: p.yaw,
            speed: if whole { p.speed } else { 0.0 },
            stride: street::seed_of(p.id) as f32 / u32::MAX as f32,
            scared: p.scared,
            whole,
            from: to,
            to,
            u: 1.0,
        }
    }

    fn step_by(&mut self, dt: f32) {
        use std::f32::consts::{PI, TAU};
        let (x, s) = (self.x, self.s);
        self.u = (self.u + dt / EASE).min(1.0);
        let ds = round_from(RING, self.from.1, self.to.1);
        let turn = (self.to.2 - self.from.2 + PI).rem_euclid(TAU) - PI;
        self.x = self.from.0 + (self.to.0 - self.from.0) * self.u;
        self.s = self.from.1 + ds * self.u;
        self.yaw = self.from.2 + turn * self.u;
        let moved = (self.x - x).hypot(round_from(RING, s, self.s));
        if !self.whole {
            self.speed = if dt > 0.0 { moved / dt } else { 0.0 };
        }
        let stride = STRIDE + STRIDE_MORE * (self.speed - 1.3).max(0.0);
        self.stride = (self.stride + moved / stride).fract();
    }

    /// As one of the crowd's is drawn, `t` m round the ring from where the pilot's `t` is `at_t`
    /// and their deck `s` is `at_s` (so it comes back in the pilot's own turn of the ring).
    pub fn person(&self, at_s: f32, at_t: f32) -> Person {
        Person {
            id: self.id,
            x: self.x,
            t: at_t + round_from(RING, at_s, self.s),
            yaw: self.yaw,
            speed: self.speed,
            stride: self.stride,
            pose: if self.speed > 0.05 { Pose::Walk } else { Pose::Stand },
            seed: self.seed,
        }
    }
}

/// The town's people out of its crowd round the pilot, as the server says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loose {
    pub people: Vec<Drawn>,
    /// Their ids, in order (the crowd's closed form draws none of them).
    ids: Vec<u32>,
    heard: u64,
}

impl Loose {
    /// The server's `heard`th word on them (`Update::Crowd`): each eased from where they're drawn
    /// to where they're said to be, one new to the pilot put there, one no longer said gone (back in
    /// the crowd, or out of reach). Heard already, nothing.
    pub fn hear(&mut self, heard: u64, near: &[Promoted], mid: &[Placed]) {
        if heard == self.heard {
            return;
        }
        self.heard = heard;
        let said =
            near.iter().map(|p| (*p, true)).chain(mid.iter().map(|&m| (Promoted::from_placed(m), false)));
        let mut next = Vec::with_capacity(near.len() + mid.len());
        for (p, whole) in said {
            let to = (p.x, p.s, p.yaw);
            let d = match self.people.iter().find(|d| d.id == p.id) {
                Some(d) if (p.x - d.x).hypot(round_from(RING, d.s, p.s)) <= SNAP => Drawn {
                    from: (d.x, d.s, d.yaw),
                    to,
                    u: 0.0,
                    speed: if whole { p.speed } else { d.speed },
                    scared: p.scared,
                    whole,
                    ..*d
                },
                _ => Drawn::new(&p, whole),
            };
            next.push(d);
        }
        self.people = next;
        self.ids = self.people.iter().map(|d| d.id).collect();
        self.ids.sort_unstable();
    }

    /// `dt` s on.
    pub fn step(&mut self, dt: f32) {
        for d in &mut self.people {
            d.step_by(dt);
        }
    }

    /// Whether the person `id` is out of the crowd (round the pilot, as last said).
    pub fn has(&self, id: u32) -> bool {
        self.ids.binary_search(&id).is_ok()
    }

    /// Those drawn within `reach` m of `(x, s)` on the ring's floor.
    pub fn within(&self, x: f32, s: f32, reach: f32) -> impl Iterator<Item = &Drawn> + '_ {
        self.people.iter().filter(move |d| (d.x - x).hypot(round_from(RING, s, d.s)) <= reach)
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    /// Forgotten (out of the town).
    pub fn clear(&mut self) {
        self.people.clear();
        self.ids.clear();
    }
}

/// How long after their time lying in the road is up someone still isn't drawn where the crowd has
/// them, ticks: till the server's word that they're up (out of its crowd, where they lay) has come.
pub const UP_GRACE: u32 = 2 * bc_sim::TICK_HZ;

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::town;

    fn p(id: u32, x: f32, s: f32, speed: f32) -> Promoted {
        Promoted { id, x, s, yaw: 0.0, speed, scared: speed > 3.0 }
    }

    #[test]
    fn the_servers_word_is_eased_and_its_people_hidden_from_the_crowd() {
        let s0 = town::deck_s(town::CELL * 10.5);
        let mut l = Loose::default();
        l.hear(1, &[p(7, 20.0, s0, 5.0)], &[p(9, 40.0, s0 + 200.0, 0.0).placed()]);
        assert!(l.has(7) && l.has(9) && !l.has(8));
        l.hear(2, &[p(7, 20.0, s0 + 2.0, 5.0)], &[p(9, 40.0, s0 + 201.0, 0.0).placed()]);
        l.step(EASE * 0.5);
        let d = l.people.iter().find(|d| d.id == 7).unwrap();
        assert!((d.s - (s0 + 1.0)).abs() < 1e-2 && d.speed == 5.0 && d.whole && d.scared, "{d:?}");
        assert!(d.person(s0, 0.0).pose == Pose::Walk);
        let far = l.people.iter().find(|d| d.id == 9).unwrap();
        assert!(!far.whole && far.speed > 0.5, "{far:?}");
        assert_eq!(l.within(20.0, s0, 3.0).count(), 1);
        // No longer said: back in the crowd.
        l.hear(3, &[], &[]);
        assert!(l.is_empty() && !l.has(7));
    }
}
