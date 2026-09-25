//! Snapshot interpolation for remote suits (Hermite on position/velocity, nlerp on rotation).

use bc_proto::EntityState;
use bc_sim::TICK_HZ;
use glam::{Quat, Vec3};

const SAMPLES: usize = 8;
const HZ: f32 = TICK_HZ as f32;
/// Extrapolate at most this far past the newest sample (ticks).
const MAX_EXTRAPOLATION: f64 = 8.0;

/// An interpolated pose.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub aim: Vec3,
}

#[derive(Clone, Debug)]
pub struct EntityTrack {
    samples: [(u32, EntityState); SAMPLES],
    n: usize,
    head: usize,
    /// Newest state received.
    pub latest: EntityState,
    pub latest_tick: u32,
}

impl EntityTrack {
    pub fn new(tick: u32, e: EntityState) -> Self {
        let mut t = Self { samples: [(tick, e); SAMPLES], n: 0, head: 0, latest: e, latest_tick: tick };
        t.push(tick, e);
        t
    }

    pub fn push(&mut self, tick: u32, e: EntityState) {
        if self.n > 0 && tick <= self.latest_tick {
            return; // out of order or duplicate
        }
        self.samples[self.head] = (tick, e);
        self.head = (self.head + 1) % SAMPLES;
        self.n = (self.n + 1).min(SAMPLES);
        self.latest = e;
        self.latest_tick = tick;
    }

    fn get(&self, k: usize) -> &(u32, EntityState) {
        // k = 0 oldest .. n-1 newest
        let idx = (self.head + SAMPLES - self.n + k) % SAMPLES;
        &self.samples[idx]
    }

    /// Pose at time `t` (ticks).
    pub fn sample(&self, t: f64) -> Pose {
        let newest = self.get(self.n - 1);
        if t >= f64::from(newest.0) {
            let dt = ((t - f64::from(newest.0)).min(MAX_EXTRAPOLATION) as f32) / HZ;
            let e = &newest.1;
            return Pose { pos: e.pos + e.vel * dt, rot: e.rot, vel: e.vel, aim: e.aim };
        }
        let oldest = self.get(0);
        if t <= f64::from(oldest.0) {
            let e = &oldest.1;
            return Pose { pos: e.pos, rot: e.rot, vel: e.vel, aim: e.aim };
        }
        for k in 0..self.n - 1 {
            let (ta, a) = self.get(k);
            let (tb, b) = self.get(k + 1);
            if f64::from(*ta) <= t && t <= f64::from(*tb) {
                let span = (tb - ta) as f32;
                let u = ((t - f64::from(*ta)) as f32 / span).clamp(0.0, 1.0);
                let h = span / HZ;
                let pos = hermite(a.pos, a.vel * h, b.pos, b.vel * h, u);
                let rb = if a.rot.dot(b.rot) < 0.0 { -b.rot } else { b.rot };
                let rot = a.rot.lerp(rb, u).normalize();
                return Pose {
                    pos,
                    rot,
                    vel: a.vel.lerp(b.vel, u),
                    aim: a.aim.lerp(b.aim, u).normalize_or(b.aim),
                };
            }
        }
        let e = &newest.1;
        Pose { pos: e.pos, rot: e.rot, vel: e.vel, aim: e.aim }
    }

    /// The replicated state (flags, armour) of the newest sample at or before `t` (ticks), so it
    /// lines up with the pose drawn at `t`. Before the oldest sample: the oldest.
    pub fn state_at(&self, t: f64) -> &EntityState {
        let mut best = &self.get(0).1;
        for k in 1..self.n {
            let (tk, e) = self.get(k);
            if f64::from(*tk) > t {
                break;
            }
            best = e;
        }
        best
    }

    /// Acceleration estimated from the two newest samples.
    pub fn accel_estimate(&self) -> Vec3 {
        if self.n < 2 {
            return Vec3::ZERO;
        }
        let (ta, a) = self.get(self.n - 2);
        let (tb, b) = self.get(self.n - 1);
        let dt = (tb - ta) as f32 / HZ;
        if dt <= 0.0 { Vec3::ZERO } else { (b.vel - a.vel) / dt }
    }
}

fn hermite(p0: Vec3, m0: Vec3, p1: Vec3, m1: Vec3, u: f32) -> Vec3 {
    let u2 = u * u;
    let u3 = u2 * u;
    p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + m0 * (u3 - 2.0 * u2 + u) + p1 * (-2.0 * u3 + 3.0 * u2) + m1 * (u3 - u2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(flags: u16) -> EntityState {
        EntityState { flags, ..Default::default() }
    }

    #[test]
    fn state_follows_the_drawn_time() {
        let mut track = EntityTrack::new(10, state(1));
        track.push(12, state(2));
        track.push(14, state(4));
        assert_eq!(track.state_at(9.0).flags, 1);
        assert_eq!(track.state_at(11.5).flags, 1);
        assert_eq!(track.state_at(12.0).flags, 2);
        assert_eq!(track.state_at(13.9).flags, 2);
        assert_eq!(track.state_at(20.0).flags, 4);
    }
}
