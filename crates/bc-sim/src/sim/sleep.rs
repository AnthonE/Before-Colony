//! Pilots who leave: their suits stay in the sector, the pilot asleep in the cockpit.
//!
//! A sleeping suit keeps its armour, its hold and its credits, but nobody flies it. It drifts on
//! the velocity and spin it had, fully Newtonian (no flight assist, no attitude hold), and fetches
//! up against rocks, landmarks and the colony as a wreck would.
//!
//! - **Parked.** A suit standing on a body, or resting against one, when its pilot leaves is parked
//!   instead: held where it sat (or stood, kneeling if it was crouched), moving with the body. Its
//!   reactor idles down [`POWER_DOWN_TICKS`](super::POWER_DOWN_TICKS) after the pilot leaves, or
//!   [`FOUGHT_DARK_TICKS`](super::FOUGHT_DARK_TICKS) after its last fight, and then it is hidden
//!   from its enemies' sensors beyond visual range (`conceal`). One aloft in a body's grip settles
//!   onto it first, and parks where it lands. Shatter the rock and it floats free.
//! - **Asleep, nobody works the frame's special:** a Neo-Bird stays a bird, and a jammer goes off.
//! - **Hunted.** Mobile Dolls leave sleepers alone; players can hunt them. A sleeper destroyed stays
//!   gone (no respawn) and its pilot is told when they're back ([`SleeperFate`]).
//! - **Room.** When suit slots run short, the longest asleep is cleared, those in a hide spot last.
//!
//! A pilot back wakes where the suit is: on its feet (or knees) and still gripping, if it was
//! standing, and still hidden if it lay hidden. One left in a landmark's hide spot can outlive the
//! server (survival): `launch.rs`'s [`ParkRecord`](super::ParkRecord).
//!
//! What a suit can rest on is a [`Body`] (`crate::bodies`): an asteroid of the field, or a
//! landmark. Its pose is in [`Bodies::pose`].

use bc_proto::InputCmd;
use bc_proto::buttons::{GRIP, MODE};
use glam::Vec3;

use super::Sim;
use crate::bodies::{Bodies, Body, BodyPose, landmark_pose};
use crate::config::DT;
use crate::content::frame;
use crate::field::{Field, SUIT_CLEARANCE};
use crate::flight::FlightState;
use crate::ground::{self, Anchor, Footing, STANCE, UNPARK_SPEED};
use crate::handle::SuitId;
use crate::math::{integrate_rotation, length, look_rotation, normalize_or};
use crate::suits::{NO_SPOT, Suits};

/// The fastest a suit can be moving and still park, m/s.
pub const PARK_SPEED: f32 = 3.0;
/// How far out from a rock's surface (past the suit's clearance) still counts as resting on it, m.
pub const PARK_REACH: f32 = 1.5;
/// Parked sleepers gone dark are seen by their enemies within this range (out of a hide spot), and
/// not on sensors beyond it, m.
pub const PARKED_VISUAL: f32 = 400.0;

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
    /// What suit `i` could park on now: the body it stands on, if it's still enough; or one it's
    /// resting against, a rock first (the first the field lists), then a landmark (by id). Nothing
    /// while it's aloft.
    pub fn parkable(&self, i: usize) -> Option<Body> {
        let s = &self.suits;
        if !s.alive.get(i) {
            return None;
        }
        match s.footing[i] {
            Footing::Grounded => (length(s.anchor[i].vel) <= PARK_SPEED).then_some(s.anchor[i].body),
            Footing::Aloft => None,
            Footing::Free => {
                let f = &s.flight[i];
                self.resting_on_rock(f)
                    .map(Body::Rock)
                    .or_else(|| self.resting_on_landmark(f).map(Body::Landmark))
            }
        }
    }

    /// The rock a free suit is at rest against.
    fn resting_on_rock(&self, f: &FlightState) -> Option<u16> {
        if length(f.vel) > PARK_SPEED {
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

    /// The landmark a free suit is at rest against: touching it, and moving with its surface there.
    fn resting_on_landmark(&self, f: &FlightState) -> Option<u8> {
        let reach = SUIT_CLEARANCE + PARK_REACH;
        let t = self.tick();
        self.landmarks().iter().enumerate().find_map(|(k, d)| {
            if length(f.pos - d.center) > d.bound + d.orbit_radius + reach {
                return None;
            }
            let pose = landmark_pose(d, t, 0.0);
            let touching = d.shape.probe(pose.to_local(f.pos)).dist <= reach;
            (touching && length(f.vel - pose.point_vel(f.pos)) <= PARK_SPEED).then_some(k as u8)
        })
    }

    /// The pilot of `id` left: their suit sleeps, parked if it's standing on a body or resting on
    /// one. `false` if it can't (it's destroyed, or gone): release it instead.
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
        // Hands off everything but the frame's mode and the grip on the ground: no thrust, no
        // assist, no ZERO, nothing held (the hold stays), and the special off (a jammer, Full Open).
        let aim = s.flight[i].rot * Vec3::Z;
        s.aim[i] = aim;
        s.input[i] = InputCmd::neutral(t, aim, s.input[i].buttons & (MODE | GRIP));
        s.zero[i] = Default::default();
        s.boosting[i] = false;
        s.special[i].active = false;
        s.sleeping.set(i, true);
        s.slept_at[i] = t;
        if let Some(k) = self.held_chunk(i) {
            self.release(i, k, Vec3::ZERO, t);
        }
        match self.suits.footing[i] {
            Footing::Grounded => {
                // It stays where it stands (or kneels), parked, and still over the body.
                let landmarks = self.landmarks();
                let a = &mut self.suits.anchor[i];
                a.vel = Vec3::ZERO;
                a.ang_vel = Vec3::ZERO;
                if let Some(p) = Bodies::at(&self.field, landmarks, t).pose(a.body) {
                    hold(&p, a, &mut self.suits.flight[i]);
                }
            }
            // It stops where it is in the air, and grip gravity brings it down (`flight_step`) to
            // park where it lands. Hands off, nothing else would slow it: a climb or a run over a
            // curved hull it kept could carry it out of the grip (T5) and off for good.
            Footing::Aloft => self.suits.anchor[i].vel = Vec3::ZERO,
            Footing::Free => {
                self.suits.anchor[i] = Anchor::default();
                match self.parkable(i) {
                    Some(Body::Rock(r)) => {
                        let rock = self.field.rocks()[usize::from(r)];
                        let f = &mut self.suits.flight[i];
                        // Settle onto the surface, and stop dead.
                        f.pos = rock.surface(f.pos, SUIT_CLEARANCE);
                        f.vel = Vec3::ZERO;
                        f.ang_vel = Vec3::ZERO;
                        let inv = rock.rot.conjugate();
                        self.suits.anchor[i] = Anchor {
                            body: Body::Rock(r),
                            local: inv * (f.pos - rock.pos),
                            rot: inv * f.rot,
                            ..Anchor::default()
                        };
                    }
                    Some(Body::Landmark(k)) => {
                        // Settle onto the surface, and move with it.
                        let d = &self.landmarks()[usize::from(k)];
                        let pose = landmark_pose(d, t, 0.0);
                        let f = &mut self.suits.flight[i];
                        let mut local = pose.to_local(f.pos);
                        let pr = d.shape.probe(local);
                        local += pr.normal * (SUIT_CLEARANCE - pr.dist);
                        let a = Anchor {
                            body: Body::Landmark(k),
                            local,
                            rot: pose.rot.conjugate() * f.rot,
                            ..Anchor::default()
                        };
                        hold(&pose, &a, f);
                        self.suits.anchor[i] = a;
                    }
                    // (No sleeper is parked on the colony's city: the inside keeps none.)
                    Some(Body::City | Body::None) | None => {}
                }
            }
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
        let t = self.tick();
        let aim = self.suits.flight[i].rot * Vec3::Z;
        self.suits.aim[i] = aim;
        let keep = if self.suits.footing[i] == Footing::Free {
            // Resting against a body, it lets go of it, moving as its surface does there (still,
            // on a rock).
            let a = self.suits.anchor[i];
            let bodies = Bodies::at(&self.field, self.landmarks(), t);
            if let Some(p) = bodies.pose(a.body).filter(|_| bodies.alive(a.body)) {
                let f = &mut self.suits.flight[i];
                f.vel = p.point_vel(f.pos);
            }
            self.suits.anchor[i] = Anchor::default();
            0
        } else {
            // On its feet (or aloft in the grip): it wakes there, still gripping, and crouched if
            // it was.
            GRIP
        };
        self.suits.input[i] = InputCmd::neutral(t, aim, keep);
        true
    }

    pub fn is_sleeping(&self, i: usize) -> bool {
        i < self.suits.cap && self.suits.sleeping.get(i)
    }

    /// Asleep and parked on something (not still settling onto it, aloft).
    pub fn is_parked(&self, i: usize) -> bool {
        self.is_sleeping(i)
            && self.suits.anchor[i].body != Body::None
            && self.suits.footing[i] != Footing::Aloft
    }

    /// How suit `i` stands with respect to the bodies.
    pub fn footing(&self, i: usize) -> Footing {
        if i < self.suits.cap { self.suits.footing[i] } else { Footing::Free }
    }

    /// Stands suit `id` on `body` (tests, scenarios): upright on its outermost surface straight out
    /// from its origin along `dir_local` (its frame), at rest, facing along the ground, gripping.
    /// `false` if the suit isn't there awake, has no legs, or can't grip the body.
    pub fn place_on(&mut self, id: SuitId, body: Body, dir_local: Vec3) -> bool {
        if !self.suits.valid(id) || !self.suits.alive.get(id.idx()) || self.suits.sleeping.get(id.idx()) {
            return false;
        }
        let i = id.idx();
        let bodies = Bodies::at(&self.field, self.landmarks(), self.tick);
        let (true, true, Some(pose), Some(shape), Some((p, n))) = (
            frame(self.suits.frame[i]).has_legs(),
            bodies.grippable(body),
            bodies.pose(body),
            bodies.shape(body),
            bodies.surface_along(body, dir_local),
        ) else {
            return false;
        };
        let (local, n, _) = ground::place(&shape, p + n * STANCE, STANCE);
        let (local, n) = ground::settle(&shape, local, n, STANCE);
        let fwd = normalize_or(Vec3::Z - n * n.z, Vec3::X - n * n.x);
        let a = Anchor { body, local, rot: look_rotation(fwd, n), stance: STANCE, ..Anchor::default() };
        let s = &mut self.suits;
        ground::derive(&pose, &a, &mut s.flight[i]);
        (s.footing[i], s.anchor[i]) = (Footing::Grounded, a);
        let aim = s.flight[i].rot * Vec3::Z;
        s.aim[i] = aim;
        s.input[i] = InputCmd::neutral(self.tick, aim, GRIP);
        true
    }

    /// Takes sleeper `id` out of the sector without a trace: nothing spilled, and no fate (its
    /// pilot's record no longer names it: a suit put back after the server stopped waiting for
    /// it). `false` if it isn't there asleep.
    pub fn discard_sleeper(&mut self, id: SuitId) -> bool {
        if !self.suits.valid(id) || !self.suits.sleeping.get(id.idx()) {
            return false;
        }
        self.suits.release(id.idx());
        true
    }

    /// Suits asleep.
    pub fn sleepers(&self) -> usize {
        self.suits.sleeping.count()
    }

    /// Suits asleep and parked.
    pub fn parked(&self) -> usize {
        self.suits.sleeping.iter().filter(|&i| self.is_parked(i)).count()
    }

    /// Clears the longest-asleep sleeper out in the open; the longest-asleep in a hide spot only
    /// when there's none. `false` if there's none at all.
    pub fn evict_oldest_sleeper(&mut self) -> bool {
        let mut oldest: Option<((bool, u32), usize)> = None;
        for i in self.suits.sleeping.iter() {
            let key = (self.suits.hide_spot[i] != NO_SPOT, self.suits.slept_at[i]);
            if oldest.is_none_or(|(o, _)| key < o) {
                oldest = Some((key, i));
            }
        }
        let Some((_, i)) = oldest else { return false };
        let t = self.tick();
        self.note_fate(i, Gone::Evicted, t);
        self.spill(i, t, true);
        self.suits.release(i);
        true
    }

    /// Frees suit slots for `n` more (a join), clearing sleepers if it must (those out in the open
    /// first, the longest asleep first).
    pub fn ensure_free_suits(&mut self, n: usize) {
        while self.suits.free_slots() < n && self.evict_oldest_sleeper() {}
    }

    /// Keeps the sleepers under their cap (one more is about to sleep).
    pub(super) fn make_room_for_sleeper(&mut self) {
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
}

/// A sleeper's tick of motion: held to its body, or drifting (and fetching up against rocks, the
/// colony and the landmarks, as a free suit does).
pub(crate) fn sleeper_drift(suits: &mut Suits, bodies: &Bodies, i: usize) {
    let anchor = suits.anchor[i];
    if anchor.body != Body::None {
        match bodies.pose(anchor.body).filter(|_| bodies.alive(anchor.body)) {
            Some(p) => {
                hold(&p, &anchor, &mut suits.flight[i]);
                look_ahead(suits, i);
                return;
            }
            None => {
                // The rock is gone: float off it.
                let away = anchor_normal(suits.flight[i].pos, anchor, bodies.field);
                suits.flight[i].vel = away * UNPARK_SPEED;
                suits.anchor[i] = Anchor::default();
                suits.footing[i] = Footing::Free;
            }
        }
    }
    let f = &mut suits.flight[i];
    let prev = f.pos;
    f.pos += f.vel * DT;
    f.rot = integrate_rotation(f.rot, f.ang_vel, DT);
    bodies.field.collide(prev, f);
    crate::world::constrain(f);
    bodies.collide_landmarks(prev, f, None);
    suits.boosting[i] = false;
    look_ahead(suits, i);
}

/// A sleeper looks where its nose points, turning as its body (or its tumble) turns it: an aim
/// left fixed in the sector's frame would sweep round a suit parked on a spinning landmark.
pub(crate) fn look_ahead(suits: &mut Suits, i: usize) {
    suits.aim[i] = suits.flight[i].rot * Vec3::Z;
}

/// A parked suit's world state on its body, posed `p`: where the body carries it, turned as it
/// turns, and moving with its surface there (exactly still on a rock, which doesn't move).
pub(crate) fn hold(p: &BodyPose, a: &Anchor, f: &mut FlightState) {
    f.pos = p.pos + p.rot * a.local;
    f.rot = p.rot * a.rot;
    f.vel = p.point_vel(f.pos);
    f.ang_vel = if p.moving { p.ang_vel } else { Vec3::ZERO };
}

/// Which way is off the (now shattered) rock a suit was parked on.
fn anchor_normal(pos: Vec3, anchor: Anchor, field: &Field) -> Vec3 {
    match anchor.body {
        Body::Rock(r) => {
            field.rocks().get(usize::from(r)).map_or(Vec3::Y, |rock| normalize_or(pos - rock.pos, Vec3::Y))
        }
        // Landmarks (and the colony's city) never go away.
        Body::Landmark(_) | Body::City | Body::None => Vec3::Y,
    }
}
