//! Hitbox history for lag compensation: where every suit was on each of the last
//! [`HISTORY_TICKS`] ticks. Recorded after integration, so `history[T]` is exactly what snapshot `T`
//! showed the clients.

use alloc::boxed::Box;
use glam::{Quat, Vec3};

use crate::config::HISTORY_TICKS;
use crate::storage::{BitSet, boxed};

pub struct History {
    ticks: [u32; HISTORY_TICKS],
    pos: Box<[Vec3]>,
    rot: Box<[Quat]>,
    alive: [BitSet; HISTORY_TICKS],
    max: usize,
}

impl History {
    pub fn new(max_suits: usize) -> Self {
        Self {
            ticks: [u32::MAX; HISTORY_TICKS],
            pos: boxed(max_suits * HISTORY_TICKS, Vec3::ZERO),
            rot: boxed(max_suits * HISTORY_TICKS, Quat::IDENTITY),
            alive: core::array::from_fn(|_| BitSet::new(max_suits)),
            max: max_suits,
        }
    }

    /// Records tick `t`.
    pub fn record(&mut self, t: u32, alive: &BitSet, pos: impl Fn(usize) -> (Vec3, Quat)) {
        let ring = t as usize % HISTORY_TICKS;
        self.ticks[ring] = t;
        self.alive[ring].copy_from(alive);
        let base = ring * self.max;
        for i in alive.iter() {
            let (p, r) = pos(i);
            self.pos[base + i] = p;
            self.rot[base + i] = r;
        }
    }

    /// Pose of suit `i` at tick `t`, if recorded and alive then.
    #[inline]
    pub fn pose(&self, t: u32, i: usize) -> Option<(Vec3, Quat)> {
        let ring = t as usize % HISTORY_TICKS;
        if self.ticks[ring] != t || !self.alive[ring].get(i) {
            return None;
        }
        let k = ring * self.max + i;
        Some((self.pos[k], self.rot[k]))
    }

    /// Pose at fractional time `t + frac` (linear position, nlerp rotation).
    pub fn pose_lerp(&self, t: u32, frac: f32, i: usize) -> Option<(Vec3, Quat)> {
        let (p0, r0) = self.pose(t, i)?;
        if frac <= 0.0 {
            return Some((p0, r0));
        }
        match self.pose(t + 1, i) {
            Some((p1, r1)) => {
                let r1 = if r0.dot(r1) < 0.0 { -r1 } else { r1 };
                Some((p0.lerp(p1, frac), crate::math::quat_normalize(r0.lerp(r1, frac))))
            }
            None => Some((p0, r0)),
        }
    }

    /// Estimated acceleration of suit `i` around tick `t - 5` from a second difference over 10 ticks.
    pub fn accel_estimate(&self, t: u32, i: usize, dt: f32) -> Option<Vec3> {
        let (p2, _) = self.pose(t, i)?;
        let (p1, _) = self.pose(t.checked_sub(5)?, i)?;
        let (p0, _) = self.pose(t.checked_sub(10)?, i)?;
        let h = 5.0 * dt;
        Some((p2 - 2.0 * p1 + p0) / (h * h))
    }
}
