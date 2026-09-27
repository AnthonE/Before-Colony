//! Pilots who leave: their suits stay in the sector, the pilot asleep in the cockpit.
//!
//! A sleeping suit keeps its armour, its hold and its credits, but nobody flies it. It drifts on
//! the velocity and spin it had, fully Newtonian (no flight assist, no attitude hold), and fetches
//! up against rocks and the colony as a wreck would. If it was resting against an asteroid when
//! its pilot left, it's parked instead: held where it sat, and hidden from sensors beyond visual
//! range. Shatter the rock and it floats free.
//!
//! Mobile Dolls leave sleepers alone; players can hunt them. A sleeper destroyed stays gone (no
//! respawn) and its pilot is told when they're back ([`SleeperFate`]). When suit slots run short,
//! the longest asleep is cleared.
//!
//! What a suit can rest on is a [`Body`]: asteroids today; crater floors in a lunar sector are one
//! more variant, with its pose in [`Sim::body_pose`].

use bc_proto::InputCmd;
use glam::{Quat, Vec3};

use super::Sim;
use crate::config::DT;
use crate::field::SUIT_CLEARANCE;
use crate::handle::SuitId;
use crate::math::{integrate_rotation, length, normalize_or};

/// The fastest a suit can be moving and still park, m/s.
pub const PARK_SPEED: f32 = 3.0;
/// How far out from a rock's surface (past the suit's clearance) still counts as resting on it, m.
pub const PARK_REACH: f32 = 1.5;
/// A shattered rock's sleepers float off at this speed, m/s.
pub const UNPARK_SPEED: f32 = 1.5;
/// Parked sleepers are seen within this range, and not on sensors beyond it, m.
pub const PARKED_VISUAL: f32 = 400.0;

/// What a sleeping suit rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Body {
    /// Nothing: it drifts.
    #[default]
    None,
    /// An asteroid of the field, by index.
    Rock(u16),
}

/// Where a parked suit sits, relative to its body.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Anchor {
    pub body: Body,
    /// Position and orientation in the body's frame.
    pub local: Vec3,
    pub rot: Quat,
}

/// Why a sleeper is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gone {
    /// Destroyed by `killer` (an entity slot).
    Destroyed { killer: u16 },
    /// Cleared to make room.
    Evicted,
}

/// A sleeping suit that won't be there when its pilot comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleeperFate {
    pub suit: u16,
    /// The generation it slept as (the server's handle on it).
    pub generation: u16,
    pub gone: Gone,
    pub tick: u32,
}

impl Sim {
    /// The pose of `body` (rocks don't move; a moving body would, here).
    fn body_pose(&self, body: Body) -> Option<(Vec3, Quat)> {
        match body {
            Body::None => None,
            Body::Rock(r) => {
                let i = usize::from(r);
                let rock = self.field.rocks().get(i)?;
                (!self.field.is_dead(i)).then_some((rock.pos, rock.rot))
            }
        }
    }

    /// The rock suit `i` is resting against, if it could park there now.
    pub fn parkable(&self, i: usize) -> Option<u16> {
        let f = &self.suits.flight[i];
        if !self.suits.alive.get(i) || length(f.vel) > PARK_SPEED {
            return None;
        }
        let reach = SUIT_CLEARANCE + PARK_REACH;
        let pad = Vec3::splat(reach);
        let mut found = None;
        self.field.for_each_in_box(f.pos - pad, f.pos + pad, |r| {
            if found.is_none() && !self.field.is_dead(r) && self.field.rocks()[r].touches(f.pos, reach) {
                found = Some(r as u16);
            }
        });
        found
    }

    /// The pilot of `id` left: their suit sleeps, parked if it's resting on a rock. `false` if it
    /// can't (it's destroyed, or gone): release it instead.
    pub fn sleep(&mut self, id: SuitId) -> bool {
        if !self.suits.valid(id) || !self.suits.alive.get(id.idx()) {
            return false;
        }
        let i = id.idx();
        if self.suits.sleeping.get(i) {
            return true;
        }
        self.make_room_for_sleeper();
        let t = self.tick();
        let s = &mut self.suits;
        // Hands off everything: no thrust, no assist, no ZERO, nothing held (the hold stays).
        let aim = s.flight[i].rot * Vec3::Z;
        s.input[i] = InputCmd { tick: t, view_tick_q4: t << 4, aim, ..InputCmd::default() };
        s.zero[i] = Default::default();
        s.boosting[i] = false;
        s.sleeping.set(i, true);
        s.slept_at[i] = t;
        s.anchor[i] = Anchor::default();
        if let Some(k) = self.held_chunk(i) {
            self.release(i, k, Vec3::ZERO, t);
        }
        if let Some(r) = self.parkable(i) {
            let rock = self.field.rocks()[usize::from(r)];
            let f = &mut self.suits.flight[i];
            // Settle onto the surface, and stop dead.
            f.pos = rock.surface(f.pos, SUIT_CLEARANCE);
            f.vel = Vec3::ZERO;
            f.ang_vel = Vec3::ZERO;
            let inv = rock.rot.conjugate();
            self.suits.anchor[i] =
                Anchor { body: Body::Rock(r), local: inv * (f.pos - rock.pos), rot: inv * f.rot };
        }
        true
    }

    /// The pilot of `id` is back. `false` if the suit isn't sleeping there any more.
    pub fn wake(&mut self, id: SuitId) -> bool {
        if !self.suits.valid(id) || !self.suits.sleeping.get(id.idx()) || !self.suits.alive.get(id.idx()) {
            return false;
        }
        let i = id.idx();
        self.suits.sleeping.set(i, false);
        self.suits.anchor[i] = Anchor::default();
        let t = self.tick();
        let aim = self.suits.flight[i].rot * Vec3::Z;
        self.suits.aim[i] = aim;
        self.suits.input[i] = InputCmd { tick: t, view_tick_q4: t << 4, aim, ..InputCmd::default() };
        true
    }

    pub fn is_sleeping(&self, i: usize) -> bool {
        i < self.suits.cap && self.suits.sleeping.get(i)
    }

    /// Asleep and parked on something.
    pub fn is_parked(&self, i: usize) -> bool {
        self.is_sleeping(i) && self.suits.anchor[i].body != Body::None
    }

    /// Suits asleep.
    pub fn sleepers(&self) -> usize {
        self.suits.sleeping.count()
    }

    /// Suits asleep and parked.
    pub fn parked(&self) -> usize {
        self.suits.sleeping.iter().filter(|&i| self.suits.anchor[i].body != Body::None).count()
    }

    /// Clears the longest-asleep sleeper. `false` if there's none.
    pub fn evict_oldest_sleeper(&mut self) -> bool {
        let mut oldest: Option<(u32, usize)> = None;
        for i in self.suits.sleeping.iter() {
            let at = self.suits.slept_at[i];
            if oldest.is_none_or(|(o, _)| at < o) {
                oldest = Some((at, i));
            }
        }
        let Some((_, i)) = oldest else { return false };
        let t = self.tick();
        self.note_fate(i, Gone::Evicted, t);
        self.spill(i, t, true);
        self.suits.release(i);
        true
    }

    /// Frees suit slots for `n` more (a join), clearing the longest-asleep sleepers if it must.
    pub fn ensure_free_suits(&mut self, n: usize) {
        while self.suits.free_slots() < n && self.evict_oldest_sleeper() {}
    }

    /// Keeps the sleepers under their cap (one more is about to sleep).
    fn make_room_for_sleeper(&mut self) {
        while self.sleepers() >= self.cfg.max_sleepers && self.evict_oldest_sleeper() {}
    }

    pub(crate) fn note_fate(&mut self, i: usize, gone: Gone, t: u32) {
        let generation = self.suits.generation[i];
        // The sector drains this every tick, so 64 sleepers lost in one tick won't happen; if they
        // were, the rest would go unreported (their pilots hear "lost" anyway).
        let _ = self.fates.try_push(SleeperFate { suit: i as u16, generation, gone, tick: t });
    }

    /// Hands over what became of sleepers since the last call.
    pub fn drain_fates(&mut self, mut f: impl FnMut(SleeperFate)) {
        for fate in self.fates.as_slice() {
            f(*fate);
        }
        self.fates.clear();
    }

    /// A sleeper's tick of motion: held to its body, or drifting.
    pub(crate) fn sleeper_drift(&mut self, i: usize) {
        let anchor = self.suits.anchor[i];
        if anchor.body != Body::None {
            match self.body_pose(anchor.body) {
                Some((pos, rot)) => {
                    let f = &mut self.suits.flight[i];
                    f.pos = pos + rot * anchor.local;
                    f.rot = rot * anchor.rot;
                    f.vel = Vec3::ZERO;
                    f.ang_vel = Vec3::ZERO;
                    return;
                }
                None => {
                    // The rock is gone: float off it.
                    let away = anchor_normal(self.suits.flight[i].pos, anchor, self);
                    self.suits.flight[i].vel = away * UNPARK_SPEED;
                    self.suits.anchor[i] = Anchor::default();
                }
            }
        }
        let f = &mut self.suits.flight[i];
        let prev = f.pos;
        f.pos += f.vel * DT;
        f.rot = integrate_rotation(f.rot, f.ang_vel, DT);
        self.field.collide(prev, f);
        crate::world::constrain(f);
        self.suits.boosting[i] = false;
    }
}

/// Which way is off the (now shattered) rock a suit was parked on.
fn anchor_normal(pos: Vec3, anchor: Anchor, sim: &Sim) -> Vec3 {
    match anchor.body {
        Body::Rock(r) => sim
            .field
            .rocks()
            .get(usize::from(r))
            .map_or(Vec3::Y, |rock| normalize_or(pos - rock.pos, Vec3::Y)),
        Body::None => Vec3::Y,
    }
}
