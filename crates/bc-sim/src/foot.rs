//! On foot: a first-person body (a box 0.6 m across and 1.8 m tall) under the gravity of where it
//! is. It slides along what it runs into, steps up stairs and kerbs, jumps, and falls. What it
//! walks among is a [`Solid`]: the colony's city ([`CityGround`]), the bay's layout and the inside
//! of a car it rides (the client's).
//!
//! The same stepper for the tick and for the pilot's own prediction, as `ground::move_step` is for
//! suits: its trig goes through `math` (libm), so a walker steps to the same bits on the server and
//! in the browser. [`Walker::step`] moves it a frame's worth (the bay, the client's own walk);
//! [`Walker::tick`] moves it one tick, in exactly [`SUBSTEPS`] substeps.

use glam::Vec3;

use crate::colony::city::{Stage, solid};
use crate::colony::frame::{CityPos, gravity};
use crate::math::{atan2, cos, sin, wrap_tau};

/// The pull where nothing says otherwise, m/s² (the bay's, at 0.7 g).
pub const GRAVITY: f32 = 6.9;

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

/// Eye height above the feet, m.
pub const EYE: f32 = 1.62;
/// Half the body's width, m.
pub const HALF: f32 = 0.3;
/// The body's height, m.
pub const HEIGHT: f32 = 1.8;
/// The highest ledge a stride steps up onto, m: a kerb, a stair, a bench in the street.
pub const STEP: f32 = 0.45;
/// Walking speed, m/s.
pub const WALK: f32 = 4.2;
/// Running speed, m/s.
pub const RUN: f32 = 7.0;
/// A jump's speed off the ground, m/s.
pub const JUMP: f32 = 3.6;
/// How fast the legs change the body's speed on the ground, m/s².
pub const ACCEL_GROUND: f32 = 40.0;
/// And in the air, m/s².
pub const ACCEL_AIR: f32 = 6.0;
/// Longest physics step, s.
pub const SUBSTEP: f32 = 1.0 / 120.0;
/// Substeps in a tick ([`Walker::tick`]): `SUBSTEPS × SUBSTEP` is the tick's `DT`.
pub const SUBSTEPS: u32 = 4;
/// Pitch stops short of straight up or down.
pub const PITCH_MAX: f32 = 1.45;

const _: () =
    assert!((SUBSTEPS as f32 * SUBSTEP - crate::DT) * (SUBSTEPS as f32 * SUBSTEP - crate::DT) < 1e-12);

/// What the pilot's legs are asked to do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stride {
    /// Forward (+) or back, and right (+) or left, each −1..1.
    pub forward: f32,
    pub right: f32,
    pub run: bool,
    pub jump: bool,
}

/// A body on its feet: everything a step reads, so a copy steps on exactly as the original.
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
        Self { feet, vel: Vec3::ZERO, yaw: atan2(facing.x, facing.z), pitch: 0.0, grounded: false }
    }

    pub fn eye(&self) -> Vec3 {
        self.feet + Vec3::Y * EYE
    }

    /// Where the pilot looks.
    pub fn look(&self) -> Vec3 {
        let (sy, cy) = (sin(self.yaw), cos(self.yaw));
        let (sp, cp) = (sin(self.pitch), cos(self.pitch));
        Vec3::new(sy * cp, sp, cy * cp)
    }

    /// Straight ahead, level.
    pub fn heading(&self) -> Vec3 {
        Vec3::new(sin(self.yaw), 0.0, cos(self.yaw))
    }

    /// Turns the head: `yaw` to the right... to the left (+), `pitch` up (+), radians.
    pub fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw = wrap_tau(self.yaw + yaw);
        self.pitch = (self.pitch + pitch).clamp(-PITCH_MAX, PITCH_MAX);
    }

    /// The body's box at `feet`.
    pub fn body(feet: Vec3) -> (Vec3, Vec3) {
        (feet + Vec3::new(-HALF, 0.0, -HALF), feet + Vec3::new(HALF, HEIGHT, HALF))
    }

    fn clear<W: Solid + ?Sized>(world: &W, feet: Vec3) -> bool {
        let (a, b) = Self::body(feet);
        !world.hits(a, b)
    }

    /// Moves the body `dt` seconds on among `world`, in substeps of at most [`SUBSTEP`] (a frame's
    /// worth: the bay, the client's own walk).
    pub fn step<W: Solid + ?Sized>(&mut self, world: &W, s: &Stride, dt: f32) {
        let mut left = dt.clamp(0.0, 0.25);
        while left > 1e-6 {
            let h = left.min(SUBSTEP);
            self.substep(world, s, h);
            left -= h;
        }
    }

    /// Moves the body one tick on among `world`: exactly [`SUBSTEPS`] substeps of [`SUBSTEP`]
    /// (not `step(DT)`, whose subtracting loop could leave a residue for a fifth).
    pub fn tick<W: Solid + ?Sized>(&mut self, world: &W, s: &Stride) {
        for _ in 0..SUBSTEPS {
            self.substep(world, s, SUBSTEP);
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

/// A strip of the city, to walk: `city::solid` under the colony's gravity, which weakens with
/// height (1 g at the floor), in the walker's frame on the strip, `(x, h, −s)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityGround {
    pub strip: u8,
    pub stage: Stage,
}

impl Solid for CityGround {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        solid(self.strip, min, max, self.stage)
    }

    fn gravity(&self, feet: Vec3) -> f32 {
        gravity(feet.y)
    }
}

impl CityGround {
    /// Where `feet` (in the walker's frame) are in city coordinates.
    pub fn place(&self, feet: Vec3) -> CityPos {
        CityPos::from_walker(self.strip, feet)
    }
}
