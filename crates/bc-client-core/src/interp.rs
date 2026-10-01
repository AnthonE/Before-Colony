//! Snapshot interpolation for remote suits (Hermite on position/velocity, nlerp on rotation).
//!
//! A suit on a body (a rider) is sent in the body's frame, and is interpolated there: its place on
//! the body, then the body where it is at the moment drawn. So a rider standing still on the
//! rolling MO-II stays glued to the deck, and one walking on it is drawn exactly where the server
//! had it on the deck, at every moment, however the body moves. A suit that lands, or takes off,
//! between two snapshots is carried from one frame into the other exactly, with no pop.

use bc_proto::EntityState;
use bc_sim::TICK_HZ;
use bc_sim::bodies::{Body, BodyPose};
use glam::{Quat, Vec3};

use crate::surface::BodySet;

const SAMPLES: usize = 8;
const HZ: f32 = TICK_HZ as f32;
/// Extrapolate at most this far past the newest sample (ticks).
const MAX_EXTRAPOLATION: f64 = 8.0;
/// Tracks not refreshed for this long are dropped (ticks)...
pub const STALE_TICKS: u32 = 90;
/// ...or this long, for a suit standing still on a body: the server refreshes those at a tenth of
/// the rate, and each refresh would say the same.
pub const STALE_STILL_TICKS: u32 = 300;

/// A rider's relation to its body, as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundPose {
    pub body: Body,
    /// In the air in the body's grip, rather than standing on it.
    pub aloft: bool,
    /// The surface's outward normal under it (sector frame).
    pub up: Vec3,
    /// Its velocity relative to the body (sector frame), m/s.
    pub rel_vel: Vec3,
    /// How high its origin is over the surface, m: its stance, when standing.
    pub height: f32,
}

/// An interpolated pose.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub aim: Vec3,
    /// On a body: which, and how it stands on it.
    pub ground: Option<GroundPose>,
}

/// A suit's motion in one frame: the sector's, or a body's (place, orientation, velocity).
#[derive(Clone, Copy, Debug)]
struct Kin {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
}

/// The frame a record is in: the body it's on, if it's on one this client knows.
fn frame_of(e: &EntityState, bodies: &BodySet) -> Option<Body> {
    e.on.filter(|on| bodies.knows(on.body)).map(|on| Body::from(on.body))
}

/// `k`, in `body`'s frame (posed `p`), in the sector's.
fn to_world(p: &BodyPose, k: Kin) -> Kin {
    let pos = p.to_world(k.pos);
    Kin { pos, rot: p.rot * k.rot, vel: p.point_vel(pos) + p.rot * k.vel }
}

/// `k`, in the sector's frame, in the frame of a body posed `p`.
fn to_local(p: &BodyPose, k: Kin) -> Kin {
    let inv = p.rot.conjugate();
    Kin { pos: p.to_local(k.pos), rot: inv * k.rot, vel: inv * (k.vel - p.point_vel(k.pos)) }
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

    /// How long the track lives without news (ticks): longer for a suit standing still on a body.
    pub fn stale_ticks(&self) -> u32 {
        let e = &self.latest;
        match e.on {
            Some(on) if !on.aloft && e.vel == Vec3::ZERO => STALE_STILL_TICKS,
            _ => STALE_TICKS,
        }
    }

    /// Sample `k`'s motion in the sector's frame at its own tick.
    fn world_at(&self, k: usize, bodies: &BodySet) -> Kin {
        let (tk, e) = self.get(k);
        let kin = Kin { pos: e.pos, rot: e.rot, vel: e.vel };
        match frame_of(e, bodies).and_then(|b| bodies.pose_at(b, f64::from(*tk))) {
            Some(p) => to_world(&p, kin),
            None => kin,
        }
    }

    /// Pose at time `t` (ticks), with the bodies where they are then.
    pub fn sample(&self, t: f64, bodies: &BodySet) -> Pose {
        let newest = self.get(self.n - 1);
        if t >= f64::from(newest.0) {
            // On: in the frame it was last in (a still rider stays glued to its body).
            let dt = ((t - f64::from(newest.0)).min(MAX_EXTRAPOLATION) as f32) / HZ;
            let e = &newest.1;
            let kin = Kin { pos: e.pos + e.vel * dt, rot: e.rot, vel: e.vel };
            return self.pose(kin, e, t, bodies);
        }
        let oldest = self.get(0);
        if t <= f64::from(oldest.0) {
            let e = &oldest.1;
            return self.pose(Kin { pos: e.pos, rot: e.rot, vel: e.vel }, e, t, bodies);
        }
        for k in 0..self.n - 1 {
            let (ta, a) = self.get(k);
            let (tb, b) = self.get(k + 1);
            if f64::from(*ta) <= t && t <= f64::from(*tb) {
                let span = (tb - ta) as f32;
                let u = ((t - f64::from(*ta)) as f32 / span).clamp(0.0, 1.0);
                let h = span / HZ;
                // In `b`'s frame: `a` as it was, carried into it exactly at its own tick if it was
                // in another.
                let frame = frame_of(b, bodies);
                let ka = if frame_of(a, bodies) == frame {
                    Kin { pos: a.pos, rot: a.rot, vel: a.vel }
                } else {
                    let w = self.world_at(k, bodies);
                    match frame.and_then(|f| bodies.pose_at(f, f64::from(*ta))) {
                        Some(p) => to_local(&p, w),
                        None => w,
                    }
                };
                let rb = if ka.rot.dot(b.rot) < 0.0 { -b.rot } else { b.rot };
                let kin = Kin {
                    pos: hermite(ka.pos, ka.vel * h, b.pos, b.vel * h, u),
                    rot: ka.rot.lerp(rb, u).normalize(),
                    vel: ka.vel.lerp(b.vel, u),
                };
                let mut pose = self.pose(kin, b, t, bodies);
                pose.aim = a.aim.lerp(b.aim, u).normalize_or(b.aim);
                return pose;
            }
        }
        let e = &newest.1;
        self.pose(Kin { pos: e.pos, rot: e.rot, vel: e.vel }, e, t, bodies)
    }

    /// `kin`, in the frame of record `e`, drawn at `t`.
    fn pose(&self, kin: Kin, e: &EntityState, t: f64, bodies: &BodySet) -> Pose {
        let on = frame_of(e, bodies).and_then(|b| Some((b, bodies.pose_at(b, t)?, bodies.shape(b)?)));
        let Some((body, p, shape)) = on else {
            return Pose { pos: kin.pos, rot: kin.rot, vel: kin.vel, aim: e.aim, ground: None };
        };
        let w = to_world(&p, kin);
        let probe = shape.probe(kin.pos);
        let ground = GroundPose {
            body,
            aloft: e.on.is_some_and(|on| on.aloft),
            up: p.rot * probe.normal,
            rel_vel: p.rot * kin.vel,
            height: probe.dist,
        };
        Pose { pos: w.pos, rot: w.rot, vel: w.vel, aim: e.aim, ground: Some(ground) }
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

    /// Acceleration estimated from the two newest samples (sector frame): on a body, relative to
    /// it, so a suit standing on a spinning one isn't seen to accelerate.
    pub fn accel_estimate(&self, bodies: &BodySet) -> Vec3 {
        if self.n < 2 {
            return Vec3::ZERO;
        }
        let (ta, a) = self.get(self.n - 2);
        let (tb, b) = self.get(self.n - 1);
        let dt = (tb - ta) as f32 / HZ;
        if dt <= 0.0 {
            return Vec3::ZERO;
        }
        let frame = frame_of(b, bodies);
        match frame.and_then(|f| bodies.pose_at(f, f64::from(*tb))) {
            Some(p) if frame_of(a, bodies) == frame => p.rot * ((b.vel - a.vel) / dt),
            _ => (self.world_at(self.n - 1, bodies).vel - self.world_at(self.n - 2, bodies).vel) / dt,
        }
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
    use bc_proto::{BodyRef, RiderOn};
    use bc_sim::field::Field;
    use bc_sim::ground::STANCE;
    use std::sync::Arc;

    fn state(flags: u16) -> EntityState {
        EntityState { flags, ..Default::default() }
    }

    fn sector() -> BodySet {
        BodySet::new(Arc::new(Field::empty()), 2)
    }

    const MO_II: Body = Body::Landmark(0);

    /// A rider on MO-II at `local`, moving over it at `vel` (its frame), as the wire has it.
    fn rider(local: Vec3, vel: Vec3) -> EntityState {
        EntityState {
            on: Some(RiderOn { body: BodyRef::Landmark(0), aloft: false }),
            pos: local,
            vel,
            rot: Quat::IDENTITY,
            aim: Vec3::Z,
            ..EntityState::default()
        }
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

    #[test]
    fn a_rider_interpolates_in_its_body_frame() {
        // Walking along the top of MO-II's core at 8 m/s, sampled every 3 ticks, as MO-II rolls and
        // drifts under it.
        let bodies = sector();
        let top = Vec3::new(100.0, 60.0 + STANCE, 0.0);
        let walk = Vec3::X * 8.0;
        let local = |t: f64| top + walk * ((t as f32 - 3_000.0) / HZ);
        let mut track = EntityTrack::new(3_000, rider(local(3_000.0), walk));
        for k in 1..8u32 {
            let t = 3_000 + 3 * k;
            track.push(t, rider(local(f64::from(t)), walk));
        }
        let mut worst = 0.0f32;
        for k in 0..=200 {
            let t = 3_000.0 + f64::from(k) * 0.1;
            let pose = track.sample(t, &bodies);
            let p = bodies.pose_at(MO_II, t).unwrap();
            let want = p.to_world(local(t));
            worst = worst.max(pose.pos.distance(want));
            // Its velocity is the deck's under it, plus its walk.
            assert!(pose.vel.distance(p.point_vel(want) + p.rot * walk) < 1e-3, "at {t}");
            let g = pose.ground.expect("on MO-II");
            assert!(g.body == MO_II && !g.aloft);
            assert!((g.height - STANCE).abs() < 1e-3 && g.up.distance(p.rot * Vec3::Y) < 1e-4);
            assert!(g.rel_vel.distance(p.rot * walk) < 1e-3);
        }
        assert!(worst < 0.01, "{worst} m off the deck");
        // Relative to the deck it is walking steadily: no phantom acceleration from the roll.
        assert!(track.accel_estimate(&bodies).length() < 1e-3);
    }

    #[test]
    fn a_frame_switch_between_samples_is_continuous() {
        // Flying free at tick 100 and 103, then caught by MO-II: in its frame from 106 on.
        let bodies = sector();
        let p103 = bodies.pose_at(MO_II, 103.0).unwrap();
        let p106 = bodies.pose_at(MO_II, 106.0).unwrap();
        let landing = Vec3::new(0.0, 60.0 + STANCE + 12.0, 10.0);
        let fall = Vec3::new(0.0, -6.0, 2.0);
        let free_at = |p: &BodyPose, local: Vec3| {
            let pos = p.to_world(local);
            EntityState { pos, vel: p.point_vel(pos) + p.rot * fall, aim: Vec3::Z, ..EntityState::default() }
        };
        let mut track = EntityTrack::new(100, free_at(&bodies.pose_at(MO_II, 100.0).unwrap(), landing));
        let a = free_at(&p103, landing + fall * (3.0 / HZ));
        track.push(103, a);
        let mut caught = rider(landing + fall * (6.0 / HZ), fall);
        caught.on = Some(RiderOn { body: BodyRef::Landmark(0), aloft: true });
        track.push(106, caught);
        // At each sample, exactly where it was.
        assert!(track.sample(103.0, &bodies).pos.distance(a.pos) < 1e-3);
        assert!(track.sample(106.0, &bodies).pos.distance(p106.to_world(caught.pos)) < 1e-3);
        // Across the switch nothing jumps: positions and velocities move on smoothly, sampled at
        // a hundredth of a tick.
        let mut prev = track.sample(101.0, &bodies);
        for k in 1..=500 {
            let t = 101.0 + f64::from(k) / 100.0;
            let now = track.sample(t, &bodies);
            assert!(now.pos.distance(prev.pos) < 10.0 * 0.01 / HZ + 1e-3, "jumped at {t}");
            assert!(now.vel.distance(prev.vel) < 0.05, "lurched at {t}: {:?} to {:?}", prev.vel, now.vel);
            prev = now;
        }
        assert!(track.sample(104.0, &bodies).ground.is_some_and(|g| g.aloft));
    }

    #[test]
    fn extrapolation_keeps_a_still_rider_on_the_body() {
        let bodies = sector();
        let local = Vec3::new(-230.0, 0.0, 80.0 + STANCE);
        let mut track = EntityTrack::new(5_000, rider(local, Vec3::ZERO));
        track.push(5_010, rider(local, Vec3::ZERO));
        for k in 0..=80 {
            let t = 5_010.0 + f64::from(k) * 0.1;
            let p = bodies.pose_at(MO_II, t).unwrap();
            let pose = track.sample(t, &bodies);
            assert!(pose.pos.distance(p.to_world(local)) < 1e-3, "slid off at {t}");
            assert!(pose.vel.distance(p.point_vel(pose.pos)) < 1e-3, "at {t}");
        }
        // The deck moved under it meanwhile (it rides along rather than staying put).
        let moved = track.sample(5_018.0, &bodies).pos.distance(track.sample(5_010.0, &bodies).pos);
        assert!(moved > 0.3, "{moved} m");
    }

    #[test]
    fn still_riders_are_kept_longer() {
        let still = EntityTrack::new(0, rider(Vec3::Y * 70.0, Vec3::ZERO));
        assert_eq!(still.stale_ticks(), STALE_STILL_TICKS);
        let walking = EntityTrack::new(0, rider(Vec3::Y * 70.0, Vec3::Z));
        assert_eq!(walking.stale_ticks(), STALE_TICKS);
        let mut aloft = rider(Vec3::Y * 90.0, Vec3::ZERO);
        aloft.on = Some(RiderOn { body: BodyRef::Landmark(0), aloft: true });
        assert_eq!(EntityTrack::new(0, aloft).stale_ticks(), STALE_TICKS);
        assert_eq!(EntityTrack::new(0, state(0)).stale_ticks(), STALE_TICKS, "flying free");
    }
}
