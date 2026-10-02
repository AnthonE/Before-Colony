//! On foot: a first-person body (a box 0.6 m across and 1.8 m tall) under the gravity of where it
//! is. It slides along what it runs into, steps up stairs and kerbs, jumps, and falls. What it
//! walks among is a [`Solid`]: the bay's layout, the colony's city (`crate::city`), the inside of
//! a car it rides. And a guide that walks it along a route, for tests, agents and the browser's
//! autopilot.

use glam::Vec3;

use crate::bay::{GRAVITY, Layout};

/// What a walker walks among, in its frame (Y up).
pub trait Solid {
    /// Whether the box `min..max` touches anything.
    fn hits(&self, min: Vec3, max: Vec3) -> bool;

    /// The pull down −Y at `feet`, m/s².
    fn gravity(&self, _feet: Vec3) -> f32 {
        GRAVITY
    }

    /// The frame's own push on what's in it (a car braking: its acceleration, taken away), m/s².
    fn push(&self) -> Vec3 {
        Vec3::ZERO
    }
}

impl Solid for Layout {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        Layout::hits(self, min, max)
    }
}

/// Eye height above the feet, m.
pub const EYE: f32 = 1.62;
const HALF: f32 = 0.3;
const HEIGHT: f32 = 1.8;
/// The highest ledge a stride steps up onto, m.
const STEP: f32 = 0.45;
const WALK: f32 = 4.2;
const RUN: f32 = 7.0;
const JUMP: f32 = 3.6;
const ACCEL_GROUND: f32 = 40.0;
const ACCEL_AIR: f32 = 6.0;
/// Longest physics step, s.
const SUBSTEP: f32 = 1.0 / 120.0;
/// Pitch stops short of straight up or down.
const PITCH_MAX: f32 = 1.45;

/// What the pilot's legs are asked to do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stride {
    /// Forward (+) or back, and right (+) or left, each −1..1.
    pub forward: f32,
    pub right: f32,
    pub run: bool,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walker {
    pub feet: Vec3,
    pub vel: Vec3,
    /// Heading: 0 looks along +z, turning towards +x.
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
}

impl Walker {
    /// Standing at `feet`, looking along `facing`.
    pub fn at(feet: Vec3, facing: Vec3) -> Self {
        Self { feet, vel: Vec3::ZERO, yaw: facing.x.atan2(facing.z), pitch: 0.0, grounded: false }
    }

    pub fn eye(&self) -> Vec3 {
        self.feet + Vec3::Y * EYE
    }

    /// Where the pilot looks.
    pub fn look(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, cy * cp)
    }

    /// Straight ahead, level.
    pub fn heading(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(sy, 0.0, cy)
    }

    /// Turns the head: `yaw` to the right... to the left (+), `pitch` up (+), radians.
    pub fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw = (self.yaw + yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + pitch).clamp(-PITCH_MAX, PITCH_MAX);
    }

    fn body(feet: Vec3) -> (Vec3, Vec3) {
        (feet + Vec3::new(-HALF, 0.0, -HALF), feet + Vec3::new(HALF, HEIGHT, HALF))
    }

    fn clear<W: Solid + ?Sized>(world: &W, feet: Vec3) -> bool {
        let (a, b) = Self::body(feet);
        !world.hits(a, b)
    }

    /// Moves the body `dt` seconds on among `world`.
    pub fn step<W: Solid + ?Sized>(&mut self, world: &W, s: &Stride, dt: f32) {
        let mut left = dt.clamp(0.0, 0.25);
        while left > 1e-6 {
            let h = left.min(SUBSTEP);
            self.substep(world, s, h);
            left -= h;
        }
    }

    fn substep<W: Solid + ?Sized>(&mut self, bay: &W, s: &Stride, dt: f32) {
        let f = self.heading();
        let right = Vec3::new(-f.z, 0.0, f.x);
        let wish = (f * s.forward.clamp(-1.0, 1.0) + right * s.right.clamp(-1.0, 1.0)).clamp_length_max(1.0)
            * if s.run { RUN } else { WALK };
        let accel = if self.grounded { ACCEL_GROUND } else { ACCEL_AIR };
        let flat = Vec3::new(self.vel.x, 0.0, self.vel.z);
        let change = (wish - flat).clamp_length_max(accel * dt);
        self.vel.x += change.x;
        self.vel.z += change.z;
        if s.jump && self.grounded {
            self.vel.y = JUMP;
            self.grounded = false;
        }
        self.vel.y -= bay.gravity(self.feet) * dt;
        self.vel += bay.push() * dt;
        let stepped_x = self.slide(bay, 0, self.vel.x * dt);
        let stepped_z = self.slide(bay, 2, self.vel.z * dt);
        if stepped_x || stepped_z {
            self.settle(bay, STEP);
        }
        self.fall(bay, self.vel.y * dt);
    }

    /// Moves along a horizontal axis, stepping up a ledge on the way if it can. Whether it did.
    fn slide<W: Solid + ?Sized>(&mut self, bay: &W, axis: usize, d: f32) -> bool {
        if d == 0.0 {
            return false;
        }
        let mut to = self.feet;
        to[axis] += d;
        if Self::clear(bay, to) {
            self.feet = to;
            return false;
        }
        if self.grounded
            && Self::clear(bay, to + Vec3::Y * STEP)
            && Self::clear(bay, self.feet + Vec3::Y * STEP)
        {
            self.feet = to + Vec3::Y * STEP;
            return true;
        }
        // Up against it: as close as it goes, and no further.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..6 {
            let mid = (lo + hi) * 0.5;
            let mut p = self.feet;
            p[axis] += d * mid;
            if Self::clear(bay, p) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.feet[axis] += d * lo;
        self.vel[axis] = 0.0;
        false
    }

    /// Drops the feet onto what's below, at most `most` down (after stepping up).
    fn settle<W: Solid + ?Sized>(&mut self, bay: &W, most: f32) {
        let (mut lo, mut hi) = (0.0f32, most);
        if Self::clear(bay, self.feet - Vec3::Y * hi) {
            return;
        }
        for _ in 0..8 {
            let mid = (lo + hi) * 0.5;
            if Self::clear(bay, self.feet - Vec3::Y * mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.feet.y -= lo;
        self.grounded = true;
        self.vel.y = self.vel.y.max(0.0);
    }

    /// Moves vertically: lands on the floor, bumps its head.
    fn fall<W: Solid + ?Sized>(&mut self, bay: &W, d: f32) {
        let to = self.feet + Vec3::Y * d;
        if Self::clear(bay, to) {
            self.feet = to;
            self.grounded = false;
            return;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..8 {
            let mid = (lo + hi) * 0.5;
            if Self::clear(bay, self.feet + Vec3::Y * d * mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.feet.y += d * lo;
        if d < 0.0 {
            self.grounded = true;
        }
        self.vel.y = 0.0;
    }
}

/// A guide stuck this long (s) gives up.
const GIVE_UP: f32 = 6.0;

/// Walks a pilot along a route of waypoints (from [`Layout::route`]).
#[derive(Clone, Debug, Default)]
pub struct Guide {
    pub route: Vec<Vec3>,
    /// The way to face on arrival.
    pub facing: Option<Vec3>,
    stuck: f32,
    last: Option<Vec3>,
}

impl Guide {
    pub fn new(route: Vec<Vec3>, facing: Option<Vec3>) -> Self {
        Self { route, facing, stuck: 0.0, last: None }
    }

    pub fn arrived(&self) -> bool {
        self.route.is_empty()
    }

    /// Points the walker at the next waypoint and says how to stride there (hands off once it's
    /// arrived, turned to face `facing`).
    pub fn steer(&mut self, w: &mut Walker, dt: f32) -> Stride {
        while let Some(next) = self.route.first().copied() {
            let flat = Vec3::new(next.x - w.feet.x, 0.0, next.z - w.feet.z);
            if flat.length() < 0.35 && (next.y - w.feet.y).abs() < 1.0 {
                self.route.remove(0);
                continue;
            }
            // Turn towards it, not in one snap.
            let want = flat.x.atan2(flat.z);
            let d = (want - w.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            w.turn(d.clamp(-6.0 * dt, 6.0 * dt), -w.pitch * (4.0 * dt).min(1.0));
            // Nudge sideways out of a corner it's caught on.
            let moved = self.last.map_or(1.0, |p| (w.feet - p).length());
            self.last = Some(w.feet);
            self.stuck = if moved < 0.004 { self.stuck + dt } else { 0.0 };
            // Nudged and jumped for long enough without getting anywhere: it gives up (the way is
            // shut: a tram's doors closed on it, say), and whoever asked can ask again.
            if self.stuck > GIVE_UP {
                self.route.clear();
                return Stride::default();
            }
            let facing_it = d.abs() < 0.6;
            return Stride {
                forward: if facing_it { 1.0 } else { 0.25 },
                right: if self.stuck > 0.4 { 0.8 } else { 0.0 },
                run: flat.length() > 4.0,
                jump: self.stuck > 1.2,
            };
        }
        if let Some(f) = self.facing {
            let want = f.x.atan2(f.z);
            let d = (want - w.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            w.turn(d.clamp(-6.0 * dt, 6.0 * dt), -w.pitch * (4.0 * dt).min(1.0));
        }
        Stride::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bay::{CATWALK_Y, SPAWN, Spot};

    const DT: f32 = 1.0 / 60.0;

    fn settle(w: &mut Walker, bay: &Layout) {
        for _ in 0..60 {
            w.step(bay, &Stride::default(), DT);
        }
    }

    #[test]
    fn a_pilot_stands_walks_and_runs_into_walls() {
        let bay = Layout::new();
        let mut w = Walker::at(SPAWN, Vec3::X);
        settle(&mut w, &bay);
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
        let still = w.feet;
        settle(&mut w, &bay);
        assert!((w.feet - still).length() < 1e-4, "no drift");
        // Into the left wall (x = −17): it stops at the wall.
        let mut w = Walker::at(Vec3::new(-10.0, 0.0, 10.0), -Vec3::X);
        for _ in 0..600 {
            w.step(&bay, &Stride { forward: 1.0, run: true, ..Stride::default() }, DT);
        }
        assert!(w.feet.x > -17.0 + 0.29 && w.feet.x < -17.0 + 0.4, "{:?}", w.feet);
        // A jump goes up and comes down.
        let mut w = Walker::at(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);
        settle(&mut w, &bay);
        w.step(&bay, &Stride { jump: true, ..Stride::default() }, DT);
        let mut top: f32 = 0.0;
        for _ in 0..120 {
            w.step(&bay, &Stride::default(), DT);
            top = top.max(w.feet.y);
        }
        assert!(top > 0.8 && top < 1.2 && w.grounded && w.feet.y.abs() < 1e-3, "{top}");
    }

    #[test]
    fn the_guide_walks_from_the_airlock_to_every_place() {
        let bay = Layout::new();
        for spot in Spot::ALL {
            let mut w = Walker::at(SPAWN, Vec3::X);
            settle(&mut w, &bay);
            let (_, facing) = spot.stand();
            let mut g = Guide::new(bay.route(w.feet, spot), Some(facing));
            let mut t = 0.0;
            while !g.arrived() || (w.heading().dot(facing) < 0.95 && t < 60.0) {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
                t += DT;
                assert!(t < 60.0, "{spot:?}: lost at {:?} after {t:.0} s", w.feet);
            }
            for _ in 0..30 {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
            }
            assert_eq!(Layout::spot_in_view(w.eye(), w.look()), Some(spot), "{spot:?} at {:?}", w.feet);
            if spot == Spot::Cockpit {
                assert!((w.feet.y - CATWALK_Y).abs() < 0.05, "up on the catwalk: {:?}", w.feet);
            }
            // And back down, to the fabricator.
            let mut g = Guide::new(bay.route(w.feet, Spot::Fabricator), None);
            let mut t = 0.0;
            while !g.arrived() {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
                t += DT;
                assert!(t < 60.0, "{spot:?} → fabricator: lost at {:?}", w.feet);
            }
        }
    }

    #[test]
    fn a_fall_from_the_catwalk_lands_on_the_floor() {
        let bay = Layout::new();
        let mut w = Walker::at(Vec3::new(4.0, CATWALK_Y, 1.9), -Vec3::Z);
        settle(&mut w, &bay);
        assert!(w.grounded && (w.feet.y - CATWALK_Y).abs() < 1e-3);
        // Over the rail and down.
        w.feet = Vec3::new(4.0, CATWALK_Y + 1.3, 0.2);
        for _ in 0..240 {
            w.step(&bay, &Stride { forward: 1.0, ..Stride::default() }, DT);
        }
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
    }
}
