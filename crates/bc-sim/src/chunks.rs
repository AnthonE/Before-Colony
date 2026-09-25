//! Salvage chunks: loose ore, limbs blown off suits, and hulks (what's left of destroyed suits).
//!
//! Fixed capacity (ids fit the wire's 10 bits), allocated once. A free chunk moves on a closed-form
//! [`Segment`]: constant velocity and spin from its start tick, quantized to the wire grid when it
//! starts. The server and every client evaluate it with [`segment_pos`] and [`segment_rot`], so
//! they agree to the bit at whole ticks. Meeting the colony or a rock starts a new segment (a
//! bounce). A held chunk rides its holder's hand.

use alloc::boxed::Box;
use bc_proto::{ChunkDesc, ChunkKind, NO_CHUNK, Part, Segment};
use glam::{Quat, Vec3};

use crate::config::DT;
use crate::content::{ArmSlot, frame};
use crate::math::{cbrt, length, quat_axis_angle, quat_normalize};
use crate::storage::{BitSet, FreeList, boxed};

/// Chunk ids run `0..MAX_CHUNKS` ([`NO_CHUNK`] means none).
pub const MAX_CHUNKS: usize = NO_CHUNK as usize;

/// How a chunk moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Free(Segment),
    /// In suit `holder`'s hand (the right one if `right`, else the left) since tick `since`,
    /// turned by `rot` relative to the holder.
    Held {
        holder: u16,
        right: bool,
        rot: Quat,
        since: u32,
    },
}

/// Seconds since a segment started, at tick `t` (whole ticks on the server; clients also ask
/// between them). Integer ticks give the same `f32` on every machine.
#[inline]
fn since(s: &Segment, t: f64) -> f32 {
    ((t - f64::from(s.t0)) as f32) * DT
}

/// Where a segment puts its chunk at tick `t`.
pub fn segment_pos(s: &Segment, t: f64) -> Vec3 {
    s.pos + s.vel * since(s, t)
}

/// How a segment turns its chunk by tick `t`.
pub fn segment_rot(s: &Segment, t: f64) -> Quat {
    let w = length(s.spin);
    if w <= 1e-6 {
        return s.rot;
    }
    quat_normalize(quat_axis_angle(s.spin / w, w * since(s, t)) * s.rot)
}

/// Size of a chunk for reach, contact and drawing, m.
pub fn radius(desc: &ChunkDesc) -> f32 {
    match desc.kind {
        // Rubble, a little bigger than solid rock of its mass would be.
        ChunkKind::Ore { .. } => 0.9 * cbrt((desc.mass_kg as f32 / 100.0).max(0.1)),
        ChunkKind::Limb { part, .. } => match part {
            Part::Head => 1.6,
            Part::Torso => 3.5,
            Part::ArmL | Part::ArmR => 3.0,
            Part::Legs => 4.6,
            Part::Backpack => 2.6,
        },
        ChunkKind::Hulk { frame: f, .. } => frame(f).radius * 0.7,
    }
}

/// Where a held chunk sits in its holder's frame: in the hand, out in front by its size.
pub fn held_offset(right: bool, radius: f32) -> Vec3 {
    let hand = if right { ArmSlot::Right } else { ArmSlot::Left }.muzzle();
    hand + Vec3::new(0.0, 0.0, radius)
}

/// A held chunk's world pose, given its holder's.
pub fn held_pose(holder_pos: Vec3, holder_rot: Quat, right: bool, rot: Quat, radius: f32) -> (Vec3, Quat) {
    (holder_pos + holder_rot * held_offset(right, radius), quat_normalize(holder_rot * rot))
}

pub struct Chunks {
    pub alive: BitSet,
    /// Low 2 bits go on the wire, so a reused id reads as a new chunk.
    pub generation: Box<[u8]>,
    pub desc: Box<[ChunkDesc]>,
    pub motion: Box<[Motion]>,
    /// Bumped whenever what clients know of the chunk changes (a new segment, a grab, a release).
    pub version: Box<[u8]>,
    /// Tick a free chunk disappears (held ones stay).
    pub expire: Box<[u32]>,
    /// Tick it appeared: when the pool is full, the oldest free chunk makes way.
    pub born: Box<[u32]>,
    free: FreeList,
    count: usize,
}

impl Default for Chunks {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunks {
    pub fn new() -> Self {
        Self {
            alive: BitSet::new(MAX_CHUNKS),
            generation: boxed(MAX_CHUNKS, 0u8),
            desc: boxed(MAX_CHUNKS, ChunkDesc::default()),
            motion: boxed(MAX_CHUNKS, Motion::Free(Segment::default())),
            version: boxed(MAX_CHUNKS, 0u8),
            expire: boxed(MAX_CHUNKS, 0u32),
            born: boxed(MAX_CHUNKS, 0u32),
            free: FreeList::full(MAX_CHUNKS),
            count: 0,
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Adds a chunk. When the pool is full the oldest free chunk is recycled; if every chunk is
    /// held, nothing is added.
    pub fn spawn(&mut self, desc: ChunkDesc, motion: Motion, expire: u32, t: u32) -> Option<u16> {
        if self.free.is_empty() {
            let mut oldest: Option<(u32, usize)> = None;
            for k in self.alive.iter() {
                if matches!(self.motion[k], Motion::Free(_)) && oldest.is_none_or(|(b, _)| self.born[k] < b) {
                    oldest = Some((self.born[k], k));
                }
            }
            self.kill(oldest?.1);
        }
        let k = self.free.pop()? as usize;
        self.alive.set(k, true);
        self.generation[k] = self.generation[k].wrapping_add(1);
        self.desc[k] = desc;
        self.motion[k] = motion;
        self.version[k] = self.version[k].wrapping_add(1);
        self.expire[k] = expire;
        self.born[k] = t;
        self.count += 1;
        Some(k as u16)
    }

    pub fn kill(&mut self, k: usize) {
        if self.alive.get(k) {
            self.alive.set(k, false);
            self.free.push(k as u16);
            self.count -= 1;
        }
    }

    /// Sets a chunk's motion, telling clients.
    pub fn set_motion(&mut self, k: usize, motion: Motion) {
        self.motion[k] = motion;
        self.touch(k);
    }

    /// Marks chunk `k` changed (a hulk that lost a part), telling clients.
    pub fn touch(&mut self, k: usize) {
        self.version[k] = self.version[k].wrapping_add(1);
    }

    /// Whether `id` names a live chunk.
    #[inline]
    pub fn is_alive(&self, id: u16) -> bool {
        (id as usize) < MAX_CHUNKS && self.alive.get(id as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ore(kg: u32) -> ChunkDesc {
        ChunkDesc { kind: ChunkKind::Ore { ore: 0 }, seed: 0, mass_kg: kg }
    }

    #[test]
    fn segments_match_stepping_to_a_millimetre_over_a_minute() {
        let s = Segment {
            t0: 100,
            pos: Vec3::new(-2_000.0, 900.0, 40.0),
            vel: Vec3::new(13.25, -2.5, 7.75),
            rot: Quat::IDENTITY,
            spin: Vec3::new(0.0, 0.5, 0.0),
        }
        .quantized();
        for t in 101..=100 + 1_800 {
            let exact = s.pos.as_dvec3() + s.vel.as_dvec3() * (f64::from(t - 100) * f64::from(DT));
            assert!((segment_pos(&s, f64::from(t)).as_dvec3() - exact).length() < 1e-3, "tick {t}");
        }
        // Half a turn at 0.5 rad/s takes 2π s.
        let q = segment_rot(&s, 100.0 + 2.0 * core::f64::consts::PI / f64::from(DT));
        assert!((q * Vec3::Z).dot(-Vec3::Z) > 0.999);
    }

    #[test]
    fn a_full_pool_recycles_the_oldest_free_chunk_but_never_a_held_one() {
        let mut c = Chunks::new();
        let held = Motion::Held { holder: 1, right: false, rot: Quat::IDENTITY, since: 0 };
        let first = c.spawn(ore(10), held, u32::MAX, 0).unwrap();
        for t in 1..MAX_CHUNKS as u32 {
            c.spawn(ore(10), Motion::Free(Segment::default()), u32::MAX, t).unwrap();
        }
        assert_eq!(c.count(), MAX_CHUNKS);
        // Full: the oldest *free* one (born at tick 1) goes, not the held one from tick 0.
        let born_1 = (0..MAX_CHUNKS).find(|&k| c.born[k] == 1).unwrap();
        let g = c.generation[born_1];
        let k = c.spawn(ore(20), Motion::Free(Segment::default()), u32::MAX, 5_000).unwrap();
        assert_eq!(k as usize, born_1);
        assert_ne!(c.generation[born_1], g);
        assert!(c.is_alive(first));
        assert_eq!(c.count(), MAX_CHUNKS);
    }

    #[test]
    fn sizes() {
        assert!((radius(&ore(100)) - 0.9).abs() < 1e-4);
        assert!(radius(&ore(2_500)) > 2.5);
    }
}
