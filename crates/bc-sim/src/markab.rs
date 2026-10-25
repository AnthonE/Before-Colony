//! The Markab: the colony's rocket car, from any motor pool (`colony::pools`) beside its
//! cars and scooters. The name is α Pegasi's, the saddle of the winged horse. It's a low two-seater
//! with two things no other car in the city has:
//!
//! - **The fan.** Under its floor a fan pulls it onto whatever its wheels are on, at any speed, with
//!   [`FAN`] (2 g; F turns it off and on). Fan on, a Markab driven at a wall rolls up onto it as into
//!   a quarter-pipe ([`FILLET`]) and drives on up it, and one that drives up into a ceiling drives on
//!   along it, upside down: there the fan's 2 g up against the colony's 1 down leaves it 1 g to hang
//!   by, at a standstill if it likes. Over the top of a wall or a roof's edge it rolls on round onto
//!   the next face if it's slow enough for the fan to hold it round ([`EDGE`]), and flies off if not.
//!   Fan off, it's a car: walls stop it, and it falls off any it was on.
//! - **The thrusters.** Ten cold-gas jets on bottles that a compressor tops up while its wheels are
//!   down: four under its floor (Space: a leap off whatever it's on, and held, [`LIFT`] to climb on),
//!   four in its tail (Shift: [`BOOST`], 0 to 100 km/h in a second, pushing where the tyres can't),
//!   and two in its nose that turn it in the air (W/S its pitch, A/D its yaw, Q/E its roll: a barrel
//!   roll takes 1.3 s). Come down on its roof or its side, and they put it back on its wheels.
//!
//! It drives the city's faces in the walker's frame on a strip (`(x, h, −s)`): the boxes of
//! `colony::city::each_solid` and of the street furniture, and the floor round the canal's
//! channels, gathered round it each step ([`Near`]); every face is square to the strip's axes. On a
//! face it's a bicycle model whose tyres hold what the load lets them (the fan's pull and the
//! colony's, into the face), and slide past that. The colony pulls it down −Y by
//! `frame::gravity`, as it does a walker. Nothing of it is checked but where it is and how fast it
//! went (the server's `plaza`), as for any car.
//!
//! Shared by the tick and the driver's own prediction, as `foot` and `car` are: its trig goes
//! through `math` (libm) and glam's (libm too), what it gathers round it is fixed storage
//! ([`Near`], [`MAX_NEAR`]), and its whole state, its private parts too, is a [`MarkabState`], so a
//! Markab made again from one drives on to the bit. [`Markab::step`] drives it a frame's worth (the
//! client); [`Markab::tick`] one tick, in exactly [`SUBSTEPS`] substeps.

use core::f32::consts::{FRAC_PI_2, PI};

use bc_proto::presence::{JET_BOOST, JET_LIFT, PersonPose, RIDE_MARKAB};
use glam::{EulerRot, Quat, Vec3};

use crate::colony::city::{
    BlockKind, CityBox, Rect, Stage, block, block_index, channel, each_solid, row_at, solid,
};
use crate::colony::frame::{CityPos, STRIP_WIDTH, gravity};
use crate::colony::furniture::each_furniture;
use crate::math::{hypot, tan, wrap_tau};

/// Its body, m: long, wide, and high off the face its wheels are on.
pub const LENGTH: f32 = 4.5;
pub const WIDTH: f32 = 2.0;
pub const HEIGHT: f32 = 1.15;
/// Between its axles, and between the wheels of an axle, m.
pub const WHEELBASE: f32 = 2.7;
pub const TRACK: f32 = 1.7;
/// Its middle above the face its wheels are on, m: where it is.
pub const RIDE: f32 = 0.6;
/// The fan's pull into the face it's on, m/s² (2 g).
pub const FAN: f32 = 19.6;
/// What the tyres hold of the load on them, across and along (driving, braking): rolling, and
/// with the handbrake on.
const GRIP: f32 = 1.2;
const HANDBRAKE_GRIP: f32 = 0.45;
/// On its wheels: its top speed and reverse (m/s), its motor's pull and its brakes (m/s²), the
/// most its front wheels turn (rad) and how fast (rad/s).
const TOP: f32 = 34.0;
const REVERSE: f32 = 8.0;
const ACCEL: f32 = 12.0;
const BRAKE: f32 = 11.0;
const STEER: f32 = 0.6;
const STEER_RATE: f32 = 2.2;
/// The air's drag, per (m/s)², and the tyres' rolling, m/s².
const DRAG: f32 = 0.0022;
const ROLLING: f32 = 0.25;
/// It never goes faster, m/s (the server holds a driver to a little over it).
pub const TOP_SPEED: f32 = 62.0;
/// The lift jets' push (1.6 g) and the leap they kick it off a face with (m/s); the tail's push
/// (2.75 g), which fades out by `BOOST_TOP` (m/s).
pub const LIFT: f32 = 15.7;
pub const LEAP: f32 = 7.0;
pub const BOOST: f32 = 27.0;
const BOOST_TOP: f32 = 56.0;
/// In the air, the nose jets turn it at up to these rates (rad/s): pitch, yaw and roll; reached at
/// `TURN_ACCEL` (rad/s²).
const PITCH_RATE: f32 = 3.0;
const YAW_RATE: f32 = 2.2;
const ROLL_RATE: f32 = 4.8;
const TURN_ACCEL: f32 = 16.0;
/// Its bottles (full: 1): what each jet takes of them a second (a leap: at once), and what the
/// compressor puts back while its wheels are down.
const LIFT_GAS: f32 = 0.15;
const LEAP_GAS: f32 = 0.05;
const BOOST_GAS: f32 = 0.2;
const TURN_GAS: f32 = 0.03;
const REFILL: f32 = 0.1;
/// The lift jets fade out between these heights (the colony's ceiling for its traffic, its tallest
/// roofs), and nothing goes higher than [`CEILING`], m.
const LIFT_FADE: (f32, f32) = (200.0, 240.0);
pub const CEILING: f32 = 244.0;
/// The most its wheels step up or down without leaving their face (a kerb), and how far a face may
/// fall away under its middle before it has gone over an edge, m.
const STEP: f32 = 0.45;
const DROP: f32 = 0.7;
/// The quarter-pipe it rolls up a wall by (its radius, m), and the tightest it rolls round over an
/// edge: as fast as `v² ≤ EDGE × load`, m.
pub const FILLET: f32 = 3.0;
pub const EDGE: f32 = 1.2;
/// How long the brake that stopped it holds it before it reverses, s.
const PAUSE: f32 = 0.25;
/// Coming down harder than this, m/s, it loses half its speed.
const HARD: f32 = 14.0;
/// In the air its body is three cubes this big down its length (half a side, m).
const AIR_HALF: f32 = 0.75;
/// Its longest physics step, s; and how many make a tick ([`Markab::tick`]).
pub const SUBSTEP: f32 = 1.0 / 120.0;
pub const SUBSTEPS: u32 = 4;
/// Under the floor, m.
const DEEP: f32 = -50.0;

/// A solid box of the city in the walker's frame, and whether a Markab drives on its faces (all
/// but the glass's edges, which stand over the windows to keep everything on its strip).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    pub min: Vec3,
    pub max: Vec3,
    pub drive: bool,
}

impl Solid {
    /// Nothing, anywhere (what an empty slot of [`Near`] holds).
    pub const NONE: Solid = Solid { min: Vec3::ZERO, max: Vec3::ZERO, drive: false };

    fn of(b: &CityBox) -> Self {
        let r = b.rect;
        Self {
            min: Vec3::new(r.x0, b.h0, -r.s1),
            max: Vec3::new(r.x1, b.h1, -r.s0),
            drive: r.s1 > 0.0 && r.s0 < STRIP_WIDTH,
        }
    }

    fn overlaps(&self, min: Vec3, max: Vec3) -> bool {
        min.cmplt(self.max).all() && self.min.cmplt(max).all()
    }
}

/// The first face a ray meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// How far along the ray, m.
    pub t: f32,
    /// The face's outward normal: one of the strip's axes, either way.
    pub normal: Vec3,
    pub solid: Solid,
}

/// The most solids a [`Near`] holds: twice what the densest stretch of the city puts within a
/// step's reach at the Markab's top speed (29, `tests::the_densest_gather_fits`).
pub const MAX_NEAR: usize = 64;
/// The most pieces the floor round the canal's channels is cut into.
const MAX_FLOOR: usize = 16;

/// What a Markab can touch this step: the city's solids round it, in fixed storage (`full` if
/// there were more than it holds: those past [`MAX_NEAR`] are left out).
#[derive(Clone, Copy, Debug)]
pub struct Near {
    solids: [Solid; MAX_NEAR],
    n: usize,
    full: bool,
}

impl Default for Near {
    fn default() -> Self {
        Self { solids: [Solid::NONE; MAX_NEAR], n: 0, full: false }
    }
}

impl Near {
    /// Strip `strip`'s solids within `reach` of `at` (the walker's frame), and the floor's.
    pub fn gather(strip: u8, at: Vec3, reach: f32) -> Self {
        let (s, x) = (-at.z, at.x);
        let area = Rect::new(s - reach, s + reach, x - reach, x + reach);
        let mut near = Self::default();
        each_solid(strip, &area, Stage(0), |b| {
            near.push(Solid::of(b));
            false
        });
        each_furniture(strip, &area, Stage(0), |p| {
            near.push(Solid::of(&p.solid));
            false
        });
        // The floor everywhere but the canal's channels (their beds are among the boxes).
        let mut floor = [Rect::default(); MAX_FLOOR];
        floor[0] = area;
        let mut pieces = 1;
        for bx in block_index(area.x0)..=block_index(area.x1) {
            for row in row_at(area.s0)..=row_at(area.s1) {
                if let Some(b) = block(strip, bx, row, Stage(0))
                    && b.kind == BlockKind::Canal
                {
                    let ch = channel(&b.rect);
                    let mut next = [Rect::default(); MAX_FLOOR];
                    let mut n = 0;
                    for r in &floor[..pieces] {
                        let mut out = [Rect::default(); 4];
                        let k = cut(r, &ch, &mut out);
                        for p in &out[..k] {
                            if n < MAX_FLOOR {
                                next[n] = *p;
                                n += 1;
                            } else {
                                near.full = true;
                            }
                        }
                    }
                    (floor, pieces) = (next, n);
                }
            }
        }
        for r in &floor[..pieces] {
            near.push(Solid::of(&CityBox { rect: *r, h0: DEEP, h1: 0.0 }));
        }
        near
    }

    fn push(&mut self, s: Solid) {
        match self.solids.get_mut(self.n) {
            Some(slot) => {
                *slot = s;
                self.n += 1;
            }
            None => self.full = true,
        }
    }

    /// What it holds.
    pub fn solids(&self) -> &[Solid] {
        &self.solids[..self.n]
    }

    /// Whether there was more round it than it holds.
    pub fn full(&self) -> bool {
        self.full
    }

    /// The first solid the box `min..max` touches.
    pub fn hits(&self, min: Vec3, max: Vec3) -> Option<&Solid> {
        self.solids().iter().find(|b| b.overlaps(min, max))
    }

    /// The first face a ray from `from` along the unit `dir` comes to within `reach` (a solid it
    /// starts inside doesn't count).
    pub fn ray(&self, from: Vec3, dir: Vec3, reach: f32) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for b in self.solids() {
            if let Some((t, normal)) = slab(from, dir, b)
                && t <= reach
                && best.is_none_or(|h| t < h.t)
            {
                best = Some(Hit { t, normal, solid: *b });
            }
        }
        best
    }
}

/// `r` with `c` cut out of it: up to four pieces, into `out`; how many.
pub fn cut(r: &Rect, c: &Rect, out: &mut [Rect; 4]) -> usize {
    if !r.overlaps(c) {
        out[0] = *r;
        return 1;
    }
    let mut n = 0;
    let mut put = |p: Rect| {
        out[n] = p;
        n += 1;
    };
    if r.s0 < c.s0 {
        put(Rect::new(r.s0, c.s0, r.x0, r.x1));
    }
    if r.s1 > c.s1 {
        put(Rect::new(c.s1, r.s1, r.x0, r.x1));
    }
    let (s0, s1) = (r.s0.max(c.s0), r.s1.min(c.s1));
    if r.x0 < c.x0 {
        put(Rect::new(s0, s1, r.x0, c.x0));
    }
    if r.x1 > c.x1 {
        put(Rect::new(s0, s1, c.x1, r.x1));
    }
    n
}

/// Where a ray from `from` along `dir` comes into box `b` from outside it: how far, and the
/// outward normal of the face it comes in by.
fn slab(from: Vec3, dir: Vec3, b: &Solid) -> Option<(f32, Vec3)> {
    let (mut near, mut far, mut axis) = (f32::NEG_INFINITY, f32::INFINITY, 0);
    for i in 0..3 {
        let (o, d) = (from[i], dir[i]);
        if d.abs() < 1e-9 {
            if o < b.min[i] || o > b.max[i] {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((b.min[i] - o) / d, (b.max[i] - o) / d);
        let (t0, t1) = (t0.min(t1), t0.max(t1));
        if t0 > near {
            (near, axis) = (t0, i);
        }
        far = far.min(t1);
    }
    if !near.is_finite() || near < 0.0 || near > far {
        return None;
    }
    Some((near, unit(axis, -dir[axis].signum())))
}

/// Axis `i`'s unit vector, times `sign`.
fn unit(i: usize, sign: f32) -> Vec3 {
    let mut v = Vec3::ZERO;
    v[i] = sign;
    v
}

/// Which axis a unit vector along one is along.
fn axis_of(n: Vec3) -> usize {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        0
    } else if a.y >= a.z {
        1
    } else {
        2
    }
}

/// The face of `s` a box moving from `from` to `to` (each `(min, max)`) came in by: its outward
/// normal. (Already inside it, the nearest way out.)
fn entry(from: (Vec3, Vec3), to: (Vec3, Vec3), s: &Solid) -> Vec3 {
    let mut best = (f32::NEG_INFINITY, Vec3::ZERO);
    for i in 0..3 {
        let d = to.0[i] - from.0[i];
        // How far along the move it began to overlap on this axis.
        let t = if d > 1e-6 {
            (s.min[i] - from.1[i]) / d
        } else if d < -1e-6 {
            (s.max[i] - from.0[i]) / d
        } else {
            f32::NEG_INFINITY
        };
        if t > best.0 {
            best = (t, unit(i, -d.signum()));
        }
    }
    if best.0.is_finite() { best.1 } else { out_of(to.0, to.1, s).normalize_or(Vec3::Y) }
}

/// The least move that takes the box `min..max` out of `s`.
fn out_of(min: Vec3, max: Vec3, s: &Solid) -> Vec3 {
    let mut best = (f32::INFINITY, Vec3::ZERO);
    for i in 0..3 {
        let (up, down) = (s.max[i] - min[i], max[i] - s.min[i]);
        if up < best.0 {
            best = (up, unit(i, up));
        }
        if down < best.0 {
            best = (down, unit(i, -down));
        }
    }
    best.1
}

/// The motor's or the brakes' pull along its heading at `vf` (m/s) for throttle `t`, m/s².
fn engine(t: f32, vf: f32) -> f32 {
    if t > 0.0 {
        if vf < -0.1 {
            BRAKE * t
        } else {
            let r = vf / TOP;
            ACCEL * t * (1.0 - r * r * r).max(0.0)
        }
    } else if t < 0.0 {
        if vf > 0.1 { BRAKE * t } else { ACCEL * t * (1.0 + vf / REVERSE).max(0.0) }
    } else {
        0.0
    }
}

/// The tail's push at `vf` along its heading, m/s².
fn boost(vf: f32) -> f32 {
    let r = vf.max(0.0) / BOOST_TOP;
    BOOST * (1.0 - r * r * r).max(0.0)
}

/// Its axes from a pose's heading, pitch and roll (rad): turned `yaw` about up (the walker's
/// heading), its nose raised `pitch`, and rolled `roll` about its length (+: its left side up).
pub fn attitude(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, yaw, -pitch, roll)
}

/// A pose's heading, pitch and roll from its axes: [`attitude`]'s inverse, the heading and roll in
/// [0, τ), the pitch within ±90°.
pub fn euler(rot: Quat) -> (f32, f32, f32) {
    let (y, x, z) = rot.to_euler(EulerRot::YXZ);
    (wrap_tau(y), -x, wrap_tau(z))
}

/// What a Markab is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum On {
    /// On its wheels on a face: the face's outward normal (one of the strip's axes, either way), and
    /// the solid it's a face of.
    Face(Vec3, Solid),
    /// Rolling round from one face onto the next.
    Corner(Corner),
    /// In the air.
    Air,
}

/// A roll round from one face onto the next (into a wall, round a quarter-pipe [`FILLET`] from
/// both; over an edge, round the edge): the car turns as one about a line along the corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    /// A point on the line it turns about, and the line's direction: turning about it by +90° takes
    /// the face it came from onto the next.
    pub pivot: Vec3,
    pub axis: Vec3,
    /// How far round it has come, rad (of 90°).
    pub angle: f32,
    /// The faces' outward normals: the one it came from and the next.
    pub from: Vec3,
    pub to: Vec3,
    /// Its speed round (+: on to the next face; −: back) and along the corner, m/s.
    pub round: f32,
    pub along: f32,
}

impl Corner {
    /// Where a car at `pos`, turned `rot`, is once the corner has turned it `angle` further.
    fn turned(&self, pos: Vec3, rot: Quat, angle: f32) -> (Vec3, Quat) {
        let q = Quat::from_axis_angle(self.axis, angle);
        (self.pivot + q * (pos - self.pivot), (q * rot).normalize())
    }
}

/// Which jets fired in its last step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Jets {
    pub lift: bool,
    pub boost: bool,
    pub turn: bool,
}

/// The driver's hands and feet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// W/S: the throttle (+), or the brake and, stopped, reverse; in the air, its nose down (+) or up.
    pub throttle: f32,
    /// A/D: the wheel, + to the left; in the air, its nose to the left (+) or the right.
    pub steer: f32,
    /// Q/E: in the air, a roll to the left (+) or the right.
    pub roll: f32,
    /// Space: the lift jets.
    pub lift: bool,
    /// Shift: the tail's.
    pub boost: bool,
    /// X.
    pub handbrake: bool,
}

/// A Markab on a strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Markab {
    pub strip: u8,
    /// Its middle, in the walker's frame on its strip.
    pub pos: Vec3,
    /// Its axes: x to its left, y up through its roof, z ahead.
    pub rot: Quat,
    pub vel: Vec3,
    /// In the air: how fast it turns about its own axes, rad/s.
    pub spin: Vec3,
    /// How far its front wheels are turned, rad.
    pub wheel: f32,
    pub on: On,
    pub fan: bool,
    /// What's left in its bottles, 0..1.
    pub gas: f32,
    pub jets: Jets,
    /// The lift jets' key was down last step (a leap fires as it goes down).
    lifting: bool,
    /// Stopped by the brakes: how much longer the pedal that stopped it holds it there before it
    /// drives it the other way, s (signed as the pedal was: + the throttle, − the brake).
    stopped: f32,
}

/// Everything a Markab's step reads, its private parts too: a Markab made again from it
/// ([`Markab::from_state`]) drives on to the bit as the one it came from (its face's box as it is,
/// not looked up again).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkabState {
    pub strip: u8,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub spin: Vec3,
    pub wheel: f32,
    pub on: On,
    pub fan: bool,
    pub gas: f32,
    pub jets: Jets,
    pub lifting: bool,
    pub stopped: f32,
}

impl Markab {
    /// Its whole state.
    pub fn state(&self) -> MarkabState {
        let Self { strip, pos, rot, vel, spin, wheel, on, fan, gas, jets, lifting, stopped } = *self;
        MarkabState { strip, pos, rot, vel, spin, wheel, on, fan, gas, jets, lifting, stopped }
    }

    /// The Markab `s` is the state of.
    pub fn from_state(s: &MarkabState) -> Self {
        let MarkabState { strip, pos, rot, vel, spin, wheel, on, fan, gas, jets, lifting, stopped } = *s;
        Self { strip, pos, rot, vel, spin, wheel, on, fan, gas, jets, lifting, stopped }
    }

    /// Standing on whatever is under `(s, x)` on strip `strip`, heading `yaw` (the walker's), its
    /// fan on and its bottles full.
    pub fn new(strip: u8, s: f32, x: f32, yaw: f32) -> Self {
        let near = Near::gather(strip, Vec3::new(x, 0.0, -s), 8.0);
        let top = Vec3::new(x, 3.0, -s);
        let (y, on) = match near.ray(top, Vec3::NEG_Y, 10.0) {
            Some(hit) if hit.normal == Vec3::Y => (top.y - hit.t, On::Face(Vec3::Y, hit.solid)),
            _ => (0.0, On::Air),
        };
        Self {
            strip,
            pos: Vec3::new(x, y + RIDE, -s),
            rot: Quat::from_rotation_y(yaw),
            vel: Vec3::ZERO,
            spin: Vec3::ZERO,
            wheel: 0.0,
            on,
            fan: true,
            gas: 1.0,
            jets: Jets::default(),
            lifting: false,
            stopped: 0.0,
        }
    }

    pub fn up(&self) -> Vec3 {
        self.rot * Vec3::Y
    }

    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::Z
    }

    pub fn left(&self) -> Vec3 {
        self.rot * Vec3::X
    }

    /// On its wheels: on a face, or rolling round onto the next.
    pub fn wheels_down(&self) -> bool {
        !matches!(self.on, On::Air)
    }

    /// The outward normal of the face it's on, if it's on one.
    pub fn face(&self) -> Option<Vec3> {
        match self.on {
            On::Face(n, _) => Some(n),
            _ => None,
        }
    }

    /// How fast it goes, m/s.
    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Along its heading (back: negative), m/s.
    pub fn ahead(&self) -> f32 {
        self.vel.dot(self.forward())
    }

    /// Where it is, in city coordinates.
    pub fn at(&self) -> CityPos {
        CityPos::from_walker(self.strip, self.pos)
    }

    pub fn toggle_fan(&mut self) {
        self.fan = !self.fan;
    }

    /// Drives (or flies) one tick on: exactly [`SUBSTEPS`] substeps of [`SUBSTEP`], among what's
    /// round it at the tick's start.
    pub fn tick(&mut self, c: &Controls) {
        let near = Near::gather(self.strip, self.pos, LENGTH + 2.0 * FILLET + self.speed() * crate::DT + 2.0);
        for _ in 0..SUBSTEPS {
            self.substep(&near, c, SUBSTEP);
        }
    }

    /// Drives (or flies) `dt` seconds on (a frame's worth: the client).
    pub fn step(&mut self, c: &Controls, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        let near = Near::gather(self.strip, self.pos, LENGTH + 2.0 * FILLET + self.speed() * dt + 2.0);
        let mut left = dt;
        while left > 1e-6 {
            let h = left.min(SUBSTEP);
            self.substep(&near, c, h);
            left -= h;
        }
    }

    fn substep(&mut self, near: &Near, c: &Controls, h: f32) {
        let press = c.lift && !self.lifting;
        self.lifting = c.lift;
        self.jets = Jets::default();
        match self.on {
            On::Face(n, under) => self.drive(near, c, n, under, press, h),
            On::Corner(k) => self.corner(near, c, k, h),
            On::Air => self.fly(near, c, h),
        }
        self.vel = self.vel.clamp_length_max(TOP_SPEED);
        if self.pos.y > CEILING {
            self.pos.y = CEILING;
            self.vel.y = self.vel.y.min(0.0);
        }
        if self.wheels_down() {
            self.gas = (self.gas + REFILL * h).min(1.0);
        }
    }

    /// Spends `rate × h` of the bottles if there's anything in them: whether the jet fires.
    fn burn(&mut self, rate: f32, h: f32) -> bool {
        let any = self.gas > 0.0;
        if any {
            self.gas = (self.gas - rate * h).max(0.0);
        }
        any
    }

    /// What holds it onto a face of outward normal `n`, m/s²: the fan's pull, and the colony's into
    /// the face (on a ceiling, out of it).
    fn load(&self, n: Vec3) -> f32 {
        (if self.fan { FAN } else { 0.0 }) + gravity(self.pos.y) * n.y
    }

    fn take_off(&mut self) {
        self.on = On::Air;
        self.spin = Vec3::ZERO;
    }

    /// On its wheels on face `n` of `under`.
    fn drive(&mut self, near: &Near, c: &Controls, n: Vec3, under: Solid, press: bool, h: f32) {
        // Square to its face.
        self.rot = (Quat::from_rotation_arc(self.up(), n) * self.rot).normalize();
        // The lift jets take it off the face (a leap as they fire); nothing holding it, it falls.
        if c.lift && self.gas > 0.0 {
            if press {
                self.gas = (self.gas - LEAP_GAS).max(0.0);
                self.vel += n * LEAP;
            }
            self.take_off();
            return self.fly(near, c, h);
        }
        let load = self.load(n);
        if load <= 0.0 {
            self.take_off();
            return self.fly(near, c, h);
        }
        let (f, l) = (self.forward(), self.left());
        let g = Vec3::NEG_Y * gravity(self.pos.y);
        let along = g - n * g.dot(n);
        let (vf, vl) = (self.vel.dot(f), self.vel.dot(l));
        // The wheel: less lock at speed.
        let lock = STEER / (1.0 + vf.abs() / 14.0);
        let want = c.steer.clamp(-1.0, 1.0) * lock;
        self.wheel += (want - self.wheel).clamp(-STEER_RATE * h, STEER_RATE * h);
        // The tyres: driving or braking it along its heading (or, stopped with nothing pressed,
        // holding it there), and taking out its slide across it and the pull across the face, as
        // far as the load lets them.
        let t = c.throttle.clamp(-1.0, 1.0);
        // Stopped by the brakes, the same pedal holds it a moment before it goes the other way.
        let pause = self.stopped * t > 0.0;
        self.stopped = if pause { (self.stopped.abs() - h).max(0.0) * self.stopped.signum() } else { 0.0 };
        let hold = (t == 0.0 || pause) && vf.abs() < 0.5;
        let lon = if hold {
            -vf / h - along.dot(f)
        } else if c.handbrake {
            (-vf / h).clamp(-BRAKE, BRAKE)
        } else {
            engine(t, vf) - vf.signum() * ROLLING
        };
        let lat = -vl / h - along.dot(l);
        let grip = GRIP * load * if c.handbrake { HANDBRAKE_GRIP } else { 1.0 };
        let total = hypot(lon, lat);
        let k = if total > grip { grip / total } else { 1.0 };
        let mut a = along + f * (lon * k) + l * (lat * k);
        if c.boost && self.burn(BOOST_GAS, h) {
            a += f * boost(vf);
            self.jets.boost = true;
        }
        a -= self.vel * (self.vel.length() * DRAG);
        self.vel += a * h;
        self.vel -= n * self.vel.dot(n);
        // The brakes stop it; they don't run it backwards (that takes another moment on the pedal).
        let after = self.vel.dot(f);
        if vf.abs() > 1e-3 && vf * after < 0.0 && t * vf <= 0.0 && !hold {
            self.vel -= f * after;
            self.stopped = PAUSE * t.signum();
        }
        // The bicycle: it turns about its face as fast as its speed and its wheel say.
        let turn = vf / WHEELBASE * tan(self.wheel);
        self.rot = (Quat::from_axis_angle(n, turn * h) * self.rot).normalize();
        // Into a wall with the fan on: up it.
        let vf = self.vel.dot(self.forward());
        if self.fan
            && vf.abs() > 0.3
            && let Some(k) = self.up_a_wall(near, n, self.forward() * vf.signum(), vf.abs(), h)
        {
            self.on = On::Corner(k);
            return;
        }
        let to = self.pos + self.vel * h;
        if let Some((i, s)) = Self::body_hit(near, to, self.forward(), n) {
            // A knock: what of its way goes into the thing goes, and a little back; it scrapes on
            // along it.
            let m = entry(
                Self::body_on(self.pos, self.forward(), n)[i],
                Self::body_on(to, self.forward(), n)[i],
                &s,
            );
            let into = self.vel.dot(m);
            if into < 0.0 {
                self.vel -= m * (into * 1.2);
            }
            let to = self.pos + self.vel * h;
            if Self::body_hit(near, to, self.forward(), n).is_none() {
                self.pos = to;
            } else {
                self.vel = Vec3::ZERO;
            }
        } else {
            self.pos = to;
        }
        self.follow(near, n, under);
    }

    /// Its body standing on a face of outward normal `n` at `pos`, heading `f`, from a step off the
    /// face to its roof: three boxes down its length.
    fn body_on(pos: Vec3, f: Vec3, n: Vec3) -> [(Vec3, Vec3); 3] {
        let half = 0.5 * WIDTH;
        let (a, b) = (n * (STEP - RIDE), n * (HEIGHT - RIDE));
        let (lo, hi) = (a.min(b), a.max(b));
        let flat = (Vec3::ONE - n.abs()) * half;
        [-1.0f32, 0.0, 1.0].map(|k| {
            let c = pos + f * (k * (0.5 * LENGTH - half));
            (c - flat + lo, c + flat + hi)
        })
    }

    /// Which of its boxes standing on face `n` at `pos` heading `f` touches something, and what.
    fn body_hit(near: &Near, pos: Vec3, f: Vec3, n: Vec3) -> Option<(usize, Solid)> {
        Self::body_on(pos, f, n).iter().enumerate().find_map(|(i, (a, b))| near.hits(*a, *b).map(|s| (i, *s)))
    }

    /// Its body in the air at `pos`, heading `f`, any way up: three cubes down its length.
    fn body_any(pos: Vec3, f: Vec3) -> [(Vec3, Vec3); 3] {
        let e = Vec3::splat(AIR_HALF);
        [-1.0f32, 0.0, 1.0].map(|k| {
            let c = pos + f * (k * (0.5 * LENGTH - AIR_HALF));
            (c - e, c + e)
        })
    }

    /// Whether it could stand at `pos`, turned `rot`, on a face of outward normal `n`: its body
    /// clear of everything, and three of its wheels or more over the face.
    fn fits(near: &Near, pos: Vec3, rot: Quat, n: Vec3) -> bool {
        let f = rot * Vec3::Z;
        if Self::body_hit(near, pos, f, n).is_some() {
            return false;
        }
        let held = [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .iter()
            .filter(|(x, z)| {
                let w = pos + rot * Vec3::new(x * 0.5 * TRACK, 0.0, z * 0.5 * WHEELBASE);
                let from = w - n * (RIDE - STEP);
                near.ray(from, -n, STEP + DROP).is_some_and(|hit| hit.normal.dot(n) > 0.99)
            })
            .count();
        held >= 3
    }

    /// The wall ahead along `dir` (its way along its face of outward normal `n`, at `speed`), if
    /// it's close enough to roll up into, square enough to its way, and has room on it: the roll.
    fn up_a_wall(&self, near: &Near, n: Vec3, dir: Vec3, speed: f32, h: f32) -> Option<Corner> {
        let hit = near.ray(self.pos, dir, 2.0 * FILLET + speed * h)?;
        let m = hit.normal;
        let square = -m.dot(dir);
        if !hit.solid.drive || m.dot(n).abs() > 0.5 || square < 0.5 {
            return None;
        }
        // Its middle's distance from the wall's face.
        let gap = hit.t * square;
        if gap > FILLET {
            return None;
        }
        // The quarter-pipe's middle: FILLET out from the wall, and up from the face.
        let pivot = self.pos + n * (FILLET - RIDE) + m * (FILLET - gap);
        let axis = n.cross(m);
        let k = Corner {
            pivot,
            axis,
            angle: 0.0,
            from: n,
            to: m,
            round: speed * square,
            along: self.vel.dot(axis),
        };
        let (pos, rot) = k.turned(self.pos, self.rot, FRAC_PI_2);
        Self::fits(near, pos, rot, m).then_some(k)
    }

    /// Keeps it on its face (a kerb up or down); with the face gone from under its middle, it's over
    /// an edge: round onto the next face, or off into the air.
    fn follow(&mut self, near: &Near, n: Vec3, under: Solid) {
        let from = self.pos - n * (RIDE - STEP);
        match near.ray(from, -n, STEP + DROP) {
            Some(hit) if hit.normal.dot(n) > 0.99 => {
                self.pos = from - n * hit.t + n * RIDE;
                self.on = On::Face(n, hit.solid);
            }
            _ => match self.over_edge(near, n, under) {
                Some(k) => self.on = On::Corner(k),
                None => self.take_off(),
            },
        }
    }

    /// Over an edge of its face of outward normal `n` on `under`: the roll round it onto the next
    /// face, if the fan can hold it round at its speed and there's room there.
    fn over_edge(&self, near: &Near, n: Vec3, under: Solid) -> Option<Corner> {
        if !self.fan {
            return None;
        }
        // The side of the face it went over: the furthest past.
        let mut past = (0.0, Vec3::ZERO);
        for i in 0..3 {
            if n[i] != 0.0 {
                continue;
            }
            let p = self.pos[i];
            if p - under.max[i] > past.0 {
                past = (p - under.max[i], unit(i, 1.0));
            }
            if under.min[i] - p > past.0 {
                past = (under.min[i] - p, unit(i, -1.0));
            }
        }
        let o = past.1;
        let across = self.vel.dot(o);
        if o == Vec3::ZERO || across <= 0.0 || across * across > EDGE * self.load(n) {
            return None;
        }
        // The edge: on the face's plane, at its side.
        let i = axis_of(o);
        let mut pivot = self.pos - n * RIDE;
        pivot[i] = if o[i] > 0.0 { under.max[i] } else { under.min[i] };
        let axis = n.cross(o);
        let k = Corner { pivot, axis, angle: 0.0, from: n, to: o, round: across, along: self.vel.dot(axis) };
        // Room on the far face for all of it, once it's on (round the edge, half of it still hangs
        // over).
        let (pos, rot) = k.turned(self.pos, self.rot, FRAC_PI_2);
        Self::fits(near, pos - n * (0.5 * WHEELBASE + 0.3), rot, o).then_some(k)
    }

    /// Rolling round a corner.
    fn corner(&mut self, near: &Near, c: &Controls, mut k: Corner, h: f32) {
        let r = self.pos - k.pivot;
        let r = r - k.axis * r.dot(k.axis);
        let radius = r.length().max(0.3);
        let tangent = k.axis.cross(r) / radius;
        let g = Vec3::NEG_Y * gravity(self.pos.y);
        // Going round nose first (+) or tail first.
        let way = self.forward().dot(tangent);
        let mut a = g.dot(tangent) + way * engine(c.throttle.clamp(-1.0, 1.0), k.round * way);
        if c.boost && self.burn(BOOST_GAS, h) {
            a += way * boost(k.round * way);
            self.jets.boost = true;
        }
        k.round += a * h;
        k.along += g.dot(k.axis) * h;
        self.vel = tangent * k.round + k.axis * k.along;
        // Nothing holds it round with the fan off; the lift jets take it off.
        if !self.fan || (c.lift && self.gas > 0.0) {
            self.take_off();
            return;
        }
        let step = k.round / radius * h;
        let (turn, next) = if k.angle + step >= FRAC_PI_2 {
            (FRAC_PI_2 - k.angle, Some(k.to))
        } else if k.angle + step <= 0.0 {
            (-k.angle, Some(k.from))
        } else {
            (step, None)
        };
        let slide = k.axis * (k.along * h);
        let (pos, rot) = k.turned(self.pos, self.rot, turn);
        (self.pos, self.rot) = (pos + slide, rot);
        k.pivot += slide;
        k.angle += turn;
        let Some(n) = next else {
            self.on = On::Corner(k);
            return;
        };
        // On the next face (or back on the last), going as it went round, along it.
        self.vel = Quat::from_axis_angle(k.axis, turn) * self.vel;
        self.vel -= n * self.vel.dot(n);
        let from = self.pos - n * (RIDE - STEP);
        match near.ray(from, -n, STEP + DROP) {
            Some(hit) if hit.normal.dot(n) > 0.99 => {
                self.pos = from - n * hit.t + n * RIDE;
                self.on = On::Face(n, hit.solid);
            }
            _ => self.take_off(),
        }
    }

    /// In the air.
    fn fly(&mut self, near: &Near, c: &Controls, h: f32) {
        let (up, f) = (self.up(), self.forward());
        let mut a = Vec3::NEG_Y * gravity(self.pos.y);
        if c.lift && self.burn(LIFT_GAS, h) {
            let fade = 1.0 - ((self.pos.y - LIFT_FADE.0) / (LIFT_FADE.1 - LIFT_FADE.0)).clamp(0.0, 1.0);
            a += up * (LIFT * fade);
            self.jets.lift = true;
        }
        if c.boost && self.burn(BOOST_GAS, h) {
            a += f * boost(self.vel.dot(f));
            self.jets.boost = true;
        }
        a -= self.vel * (self.vel.length() * DRAG);
        self.vel += a * h;
        // The nose jets: turning it up to the rates asked for; asked for none, it steadies.
        let want = Vec3::new(
            c.throttle.clamp(-1.0, 1.0) * PITCH_RATE,
            c.steer.clamp(-1.0, 1.0) * YAW_RATE,
            -c.roll.clamp(-1.0, 1.0) * ROLL_RATE,
        );
        if want != Vec3::ZERO && self.burn(TURN_GAS, h) {
            self.spin += (want - self.spin).clamp_length_max(TURN_ACCEL * h);
            self.jets.turn = true;
        } else {
            self.spin += (-self.spin).clamp_length_max(0.5 * TURN_ACCEL * h);
        }
        self.rot = (self.rot * Quat::from_scaled_axis(self.spin * h)).normalize();
        self.arrive(near, h);
    }

    /// Moves it `h` seconds on through the air, and down onto whatever it meets.
    fn arrive(&mut self, near: &Near, h: f32) {
        let f = self.forward();
        // Out of anything it has turned into.
        for _ in 0..3 {
            let inside =
                Self::body_any(self.pos, f).iter().find_map(|(a, b)| near.hits(*a, *b).map(|s| (*a, *b, *s)));
            let Some((a, b, s)) = inside else { break };
            self.pos += out_of(a, b, &s) * 1.01;
        }
        let from = Self::body_any(self.pos, f);
        let to_pos = self.pos + self.vel * h;
        let to = Self::body_any(to_pos, f);
        let hit = (0..3).find_map(|i| near.hits(to[i].0, to[i].1).map(|s| (i, *s)));
        match hit {
            None => self.pos = to_pos,
            Some((i, s)) => self.touch(near, entry(from[i], to[i], &s), s, to_pos),
        }
    }

    /// Comes down on face `m` of `s`, on its wheels (put back on them, if it came down any other
    /// way, a little the worse, and shoved along the face to where it has room to stand: nose down
    /// beside a wall, say); or, nothing to hold it there (its fan off, on a wall or a ceiling), or no
    /// room to stand, glances along it.
    fn touch(&mut self, near: &Near, m: Vec3, s: Solid, to: Vec3) {
        let i = axis_of(m);
        let plane = if m[i] > 0.0 { s.max[i] } else { s.min[i] };
        let impact = -self.vel.dot(m);
        let along = self.vel - m * self.vel.dot(m);
        let up = self.up();
        let rot =
            if up.dot(m) < -0.98 { Quat::from_axis_angle(self.forward(), PI) * self.rot } else { self.rot };
        let rot = (Quat::from_rotation_arc(rot * Vec3::Y, m) * rot).normalize();
        let mut pos = to;
        pos[i] = plane + m[i] * RIDE;
        let (f, l) = (rot * Vec3::Z, rot * Vec3::X);
        let room = [0.0f32, 0.6, 1.2, 1.8, 2.4, 3.0]
            .iter()
            .flat_map(|d| [f * *d, -f * *d, l * *d, -l * *d])
            .map(|d| pos + d)
            .find(|p| Self::fits(near, *p, rot, m));
        if s.drive
            && self.load(m) > 0.0
            && let Some(pos) = room
        {
            let rough = impact > HARD || up.dot(m) < 0.5;
            (self.pos, self.rot, self.spin) = (pos, rot, Vec3::ZERO);
            self.vel = if rough { along * 0.5 } else { along };
            self.on = On::Face(m, s);
            return;
        }
        // It glances off: along the face, out of it, and (perched on something it can't stand on)
        // tipping off it.
        self.vel = along;
        let mut p = to;
        p[i] = plane + m[i] * (AIR_HALF + 0.01);
        let clear = Self::body_any(p, self.forward()).iter().all(|(a, b)| near.hits(*a, *b).is_none());
        if clear {
            self.pos = p;
        }
        if along.length() < 0.5 {
            let off = (self.pos - (s.min + s.max) * 0.5) * (Vec3::ONE - m.abs());
            self.vel += off.normalize_or(Vec3::ZERO) * 0.5;
        }
    }

    /// The driver, as the plaza has them: where its middle is, which way it's turned, how fast it
    /// goes, its wheels down (GROUNDED), its fan on (RUNNING), and the jets firing.
    pub fn pose(&self) -> PersonPose {
        let at = self.at();
        let (yaw, pitch, roll) = euler(self.rot);
        PersonPose {
            strip: self.strip,
            x: at.x,
            s: at.s,
            h: at.h,
            yaw,
            pitch,
            roll,
            speed: self.speed(),
            grounded: self.wheels_down(),
            running: self.fan,
            ride: RIDE_MARKAB,
            jets: (if self.jets.lift { JET_LIFT } else { 0 }) | (if self.jets.boost { JET_BOOST } else { 0 }),
        }
    }

    /// Whether its driver can get out: slow, on its wheels on a floor (not a wall or a ceiling).
    pub fn can_leave(&self) -> bool {
        self.face().is_some_and(|n| n.y > 0.99) && self.speed() < 3.0
    }

    /// Where its driver gets out, standing on its floor: beside it on the left, or (something there,
    /// a lamp post, a tree) on the right, or behind it.
    pub fn door(&self) -> CityPos {
        let (f, l) = (self.forward(), self.left());
        let floor = self.pos.y - RIDE;
        let (side, back) = (0.5 * WIDTH + 0.7, 0.5 * LENGTH + 0.7);
        let spots =
            [l * side, -l * side, -f * back].map(|d| Vec3::new(self.pos.x + d.x, floor, self.pos.z + d.z));
        let free = |p: &Vec3| {
            let feet = *p + Vec3::Y * 0.05;
            !solid(self.strip, feet + Vec3::new(-0.3, 0.0, -0.3), feet + Vec3::new(0.3, 1.8, 0.3), Stage(0))
        };
        let at = spots.into_iter().find(free).unwrap_or(spots[0]);
        CityPos::from_walker(self.strip, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::{KERB, Room};
    use crate::colony::hall::hall;
    use crate::math::{atan2, cos, sin};

    const DT: f32 = 1.0 / 60.0;
    const GO: Controls =
        Controls { throttle: 1.0, steer: 0.0, roll: 0.0, lift: false, boost: false, handbrake: false };
    const IDLE: Controls =
        Controls { throttle: 0.0, steer: 0.0, roll: 0.0, lift: false, boost: false, handbrake: false };

    /// Never inside anything: its middle, and its body standing on its face.
    fn clear(m: &Markab) {
        let p = m.pos;
        let e = Vec3::splat(0.1);
        assert!(!solid(m.strip, p - e, p + e, Stage(0)), "its middle is inside a wall: {m:?}");
    }

    fn run(m: &mut Markab, secs: f32, c: Controls) {
        for _ in 0..(secs / DT) as u32 {
            m.step(&c, DT);
            clear(m);
        }
    }

    /// Runs until `done` (at most `secs`): how long it took.
    fn until(m: &mut Markab, secs: f32, c: Controls, done: impl Fn(&Markab) -> bool) -> Option<f32> {
        for k in 0..(secs / DT) as u32 {
            if done(m) {
                return Some(k as f32 * DT);
            }
            m.step(&c, DT);
            clear(m);
        }
        None
    }

    /// The in-bound road on strip 0's avenue, heading +x.
    fn on_the_avenue() -> Markab {
        Markab::new(0, STRIP_WIDTH * 0.5 - 15.0, -12_000.0, FRAC_PI_2)
    }

    /// The Blast Hall, and a place in it: `u` along its front from its middle, `v` in from its face,
    /// `h` up, in the walker's frame.
    fn in_hall(u: f32, v: f32, h: f32) -> (Room, Vec3) {
        let r = hall();
        let (s, x) = r.front.point(u, v);
        (r, Vec3::new(x, h, -s))
    }

    /// Outside the Blast Hall's blast doors, facing in through them.
    fn at_the_doors(u: f32) -> Markab {
        let (r, at) = in_hall(u, -25.0, 0.0);
        let (ds, dx) = r.inward;
        Markab::new(r.strip, -at.z, at.x, atan2(dx, -ds))
    }

    #[test]
    fn the_wheels_stand_on_the_road_and_the_kerb() {
        let m = on_the_avenue();
        assert_eq!(m.face(), Some(Vec3::Y));
        assert!((m.pos.y - RIDE).abs() < 1e-4, "{}", m.pos.y);
        // On a block's pavement, a kerb up.
        let (r, at) = in_hall(0.0, -1.0, 0.0);
        let m = Markab::new(r.strip, -at.z, at.x, 0.0);
        assert!((m.pos.y - RIDE - KERB).abs() < 1e-4, "{}", m.pos.y);
    }

    #[test]
    fn a_pose_turns_round_the_same() {
        for (yaw, pitch, roll) in [(0.3, 0.2, 0.1), (5.0, -1.2, 3.0), (2.0, 1.5, 6.0), (1.0, 0.0, PI)] {
            let q = attitude(yaw, pitch, roll);
            let (y, p, r) = euler(q);
            assert!(attitude(y, p, r).dot(q).abs() > 0.9999, "{yaw} {pitch} {roll} → {y} {p} {r}");
        }
        // Its nose up by the pitch, its left side up by the roll.
        let q = attitude(FRAC_PI_2, 0.4, 0.0);
        assert!((q * Vec3::Z - Vec3::new(cos(0.4), sin(0.4), 0.0)).length() < 1e-5);
        assert!((attitude(0.0, 0.0, 0.3) * Vec3::X).y > 0.29);
    }

    #[test]
    fn it_gets_up_to_speed_and_stops() {
        let mut m = on_the_avenue();
        run(&mut m, 12.0, GO);
        assert!(m.ahead() > 30.0 && m.ahead() <= TOP, "{} m/s", m.ahead());
        assert!((m.at().s - (STRIP_WIDTH * 0.5 - 15.0)).abs() < 0.01, "straight: {:?}", m.at());
        assert_eq!(m.face(), Some(Vec3::Y));
        let stop = until(&mut m, 10.0, Controls { throttle: -1.0, ..IDLE }, |m| m.speed() < 0.05);
        assert!(stop.is_some_and(|t| t < 4.0), "braked to a stop in {stop:?} s");
        // Held, it backs up, slowly.
        run(&mut m, 4.0, Controls { throttle: -1.0, ..IDLE });
        assert!(m.ahead() < -5.0 && m.ahead() >= -REVERSE, "{}", m.ahead());
    }

    #[test]
    fn the_tail_jets_take_it_to_a_hundred_in_a_second() {
        let mut m = on_the_avenue();
        let t = until(&mut m, 3.0, Controls { boost: true, ..GO }, |m| m.ahead() > 27.8).unwrap();
        assert!(t < 1.1, "0 to 100 km/h in {t} s");
        run(&mut m, 4.0, Controls { boost: true, ..GO });
        assert!(m.ahead() > 50.0 && m.speed() <= TOP_SPEED, "{}", m.ahead());
        let low = m.gas;
        assert!(low < 0.7, "the bottles run down faster than the compressor fills them: {low}");
        // It tops them up while it drives.
        run(&mut m, 4.0, GO);
        assert!(m.gas > low + 0.3, "{} {low}", m.gas);
    }

    #[test]
    fn it_turns_the_way_its_wheel_says_and_slides_past_its_grip() {
        let mut m = on_the_avenue();
        m.vel = m.forward() * 8.0;
        run(&mut m, 1.5, Controls { throttle: 0.3, steer: 1.0, ..IDLE });
        // Heading +x, left is +s.
        let (yaw, _, _) = euler(m.rot);
        assert!(yaw > FRAC_PI_2 + 0.5, "it turned left: {yaw}");
        assert!(m.at().s > STRIP_WIDTH * 0.5 - 15.0 + 2.0, "towards +s: {:?}", m.at());
        let slip = m.vel.dot(m.left()).abs();
        assert!(slip < 0.5, "its tyres hold it at 8 m/s: {slip}");
        // The handbrake at speed: the tail slides out.
        let mut m = on_the_avenue();
        m.vel = m.forward() * 25.0;
        let mut most = 0.0f32;
        for _ in 0..60 {
            m.step(&Controls { steer: 1.0, handbrake: true, ..IDLE }, DT);
            most = most.max(m.vel.dot(m.left()).abs());
        }
        assert!(most > 2.0, "it slid: {most}");
    }

    #[test]
    fn with_the_fan_off_a_wall_stops_it() {
        let mut m = at_the_doors(12.0);
        m.fan = false;
        // Pressed on into it, it stays there.
        run(&mut m, 20.0, GO);
        assert_eq!(m.face(), Some(Vec3::Y), "on the floor still: {m:?}");
        let (r, back) = in_hall(12.0, hall().depth(), 0.0);
        let gap = m.pos.distance(Vec3::new(back.x, m.pos.y, back.z));
        assert!(gap < 0.5 * LENGTH + 1.0 && m.speed() < 3.0, "at the wall: {gap} {r:?} {m:?}");
    }

    /// The hall's back wall's face's outward normal, and its ceiling's.
    fn back_wall() -> Vec3 {
        let (ds, dx) = hall().inward;
        -Vec3::new(dx, 0.0, -ds)
    }

    #[test]
    fn the_fan_takes_it_up_the_hall_s_back_wall_and_over_onto_its_ceiling_upside_down() {
        let mut m = at_the_doors(12.0);
        let wall = back_wall();
        let up = until(&mut m, 20.0, GO, |m| m.face() == Some(wall));
        assert!(up.is_some(), "it rolled up onto the wall: {m:?}");
        assert!((m.forward() - Vec3::Y).length() < 0.05, "nose up: {:?}", m.forward());
        let over = until(&mut m, 20.0, GO, |m| m.face() == Some(Vec3::NEG_Y));
        assert!(over.is_some(), "on along the ceiling: {m:?}");
        let r = hall();
        assert!((m.pos.y - (r.ceiling - RIDE)).abs() < 0.01, "hanging from it: {}", m.pos.y);
        assert!(m.up().y < -0.99, "upside down: {:?}", m.up());
        // It stops, and hangs there (the fan's 2 g up, the colony's 1 down).
        until(&mut m, 10.0, Controls { throttle: -1.0, ..IDLE }, |m| m.speed() < 0.05).expect("stopped");
        let at = m.pos;
        run(&mut m, 3.0, IDLE);
        assert_eq!(m.face(), Some(Vec3::NEG_Y));
        assert!(m.pos.distance(at) < 0.05, "it hung there: {:?} {at:?}", m.pos);
        // Back down the wall, tail first, and onto the floor nose first.
        let down = until(&mut m, 30.0, Controls { throttle: -1.0, ..IDLE }, |m| m.face() == Some(Vec3::Y));
        assert!(down.is_some(), "back down on the floor: {m:?}");
        let (ds, dx) = r.inward;
        assert!(
            m.forward().dot(Vec3::new(dx, 0.0, -ds)) > 0.99,
            "facing the wall it came down: {:?}",
            m.forward()
        );
    }

    #[test]
    fn with_the_fan_cut_on_the_ceiling_it_falls_and_lands_on_its_wheels() {
        let mut m = at_the_doors(12.0);
        until(&mut m, 30.0, GO, |m| m.face() == Some(Vec3::NEG_Y)).expect("on the ceiling");
        until(&mut m, 10.0, Controls { throttle: -1.0, ..IDLE }, |m| m.speed() < 0.05).expect("stopped");
        m.toggle_fan();
        run(&mut m, 0.2, IDLE);
        assert_eq!(m.on, On::Air, "it let go");
        let down = until(&mut m, 10.0, IDLE, |m| m.face() == Some(Vec3::Y));
        assert!(down.is_some(), "on the floor: {m:?}");
        assert!(m.up().y > 0.99, "the right way up: {:?}", m.up());
    }

    #[test]
    fn a_leap_and_a_barrel_roll_land_on_its_wheels() {
        let mut m = on_the_avenue();
        run(&mut m, 3.0, Controls { boost: true, ..GO });
        let x0 = m.pos.x;
        // Space: off the road; held a moment, and a roll to the left.
        run(&mut m, 0.8, Controls { lift: true, ..IDLE });
        assert_eq!(m.on, On::Air);
        let mut rolled = 0.0f32;
        let mut last = m.left();
        let mut high = 0.0f32;
        for _ in 0..(1.4 / DT) as u32 {
            m.step(&Controls { roll: 1.0, lift: true, ..IDLE }, DT);
            let l = m.left();
            rolled += last.angle_between(l);
            last = l;
            high = high.max(m.pos.y);
            clear(&m);
        }
        assert!(rolled > 1.8 * PI, "a whole roll: {} deg", rolled.to_degrees());
        assert!(high > 8.0, "over a van and then some: {high} m");
        let down = until(&mut m, 10.0, IDLE, |m| m.face() == Some(Vec3::Y));
        assert!(down.is_some(), "down: {m:?}");
        assert!(m.up().y > 0.99 && m.pos.x > x0 + 30.0, "{m:?}");
    }

    #[test]
    fn backed_from_hub_gate_s_motor_pool_it_climbs_the_terminal_and_holds_there() {
        // The `colony` e2e's run: parked facing up the avenue, Hub Gate's terminal behind it.
        let (s, x) = crate::colony::pools::pool(0, 0);
        let mut m = Markab::new(0, s, x, FRAC_PI_2);
        let back = Controls { throttle: -1.0, ..IDLE };
        let on_wall = |m: &Markab| m.face().is_some_and(|n| n.y.abs() < 0.5);
        until(&mut m, 20.0, back, on_wall).expect("up onto the terminal's face");
        until(&mut m, 30.0, back, |m| m.pos.y > 8.0).expect("up it");
        run(&mut m, 2.0, IDLE);
        let at = m.pos;
        run(&mut m, 2.0, IDLE);
        assert!(on_wall(&m) && m.pos.distance(at) < 0.05, "held there: {m:?}");
        // The fan cut, it falls onto its wheels; a leap, and down again.
        m.toggle_fan();
        until(&mut m, 10.0, IDLE, |m| m.face() == Some(Vec3::Y)).expect("down");
        assert!(m.up().y > 0.99, "{m:?}");
        m.toggle_fan();
        run(&mut m, 0.3, Controls { lift: true, ..IDLE });
        assert_eq!(m.on, On::Air);
        until(&mut m, 10.0, IDLE, |m| m.face() == Some(Vec3::Y) && m.speed() < 0.5).expect("down again");
        assert!(m.can_leave(), "{m:?}");
    }

    #[test]
    fn slow_over_a_wall_s_top_it_rolls_onto_the_roof_and_fast_it_flies() {
        // The Blast Hall's front, outside: up its face beside the blast doors, to its roof.
        let r = hall();
        let roof = (1..400)
            .map(|k| k as f32)
            .find(|h| {
                let (_, p) = in_hall(-35.0, 0.5, *h);
                !solid(r.strip, p - Vec3::splat(0.1), p + Vec3::splat(0.1), Stage(0))
            })
            .expect("a roof");
        let climb = |crawl: bool| {
            let mut m = at_the_doors(-35.0);
            let front = back_wall();
            until(&mut m, 20.0, GO, |m| m.face() == Some(front)).expect("up the front");
            let c = |m: &Markab| {
                let near = m.pos.y > roof - 12.0;
                if !crawl || !near {
                    GO
                } else if m.ahead() > 3.0 {
                    Controls { throttle: -1.0, ..IDLE }
                } else {
                    Controls { throttle: 1.0, ..IDLE }
                }
            };
            for _ in 0..(30.0 / DT) as u32 {
                m.step(&c(&m), DT);
                clear(&m);
                if m.face() == Some(Vec3::Y) || m.on == On::Air {
                    break;
                }
            }
            m
        };
        let slow = climb(true);
        assert_eq!(slow.face(), Some(Vec3::Y), "on the roof: {slow:?}");
        assert!(slow.pos.y > roof - 1.0, "{} vs {roof}", slow.pos.y);
        let fast = climb(false);
        assert_eq!(fast.on, On::Air, "off the top into the air: {fast:?}");
    }

    /// The widest a step gathers round it: at its top speed over the longest step.
    const REACH: f32 = LENGTH + 2.0 * FILLET + TOP_SPEED * 0.1 + 2.0;

    #[test]
    fn the_densest_gather_fits() {
        use crate::colony::city::{CITY, ROWS, block_rect};
        // Every block of every strip, every 16 m across it (the crossings, their lamps and kerbs,
        // the pavements' trees and benches, the buildings), and round every motor pool and the
        // Blast Hall.
        let mut most = 0;
        let mut at = |strip: u8, s: f32, x: f32| {
            let near = Near::gather(strip, Vec3::new(x, 0.0, -s), REACH);
            assert!(!near.full(), "more than {MAX_NEAR} at {strip} {s} {x}");
            most = most.max(near.solids().len());
        };
        for strip in 0..3u8 {
            for i in 0..crate::colony::pools::POOLS {
                let (s, x) = crate::colony::pools::pool(strip, i);
                for (ds, dx) in [(0.0, 0.0), (-20.0, -20.0), (20.0, 20.0), (-20.0, 20.0), (20.0, -20.0)] {
                    at(strip, s + ds, x + dx);
                }
            }
        }
        let r = hall();
        for (u, v) in [(0.0, -20.0), (0.0, 10.0), (-35.0, 0.0), (35.0, 30.0)] {
            let (s, x) = r.front.point(u, v);
            at(r.strip, s, x);
        }
        for strip in 0..3u8 {
            for bx in CITY.0..=CITY.1 {
                for row in -ROWS..=ROWS {
                    let r = block_rect(bx, row);
                    const K: [f32; 9] = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
                    for (u, v) in K.iter().flat_map(|u| K.map(|v| (*u, v))) {
                        at(strip, r.s0 + (r.s1 - r.s0) * u, r.x0 + (r.x1 - r.x0) * v);
                    }
                }
            }
        }
        assert!(most * 2 <= MAX_NEAR && most > 20, "the densest gather: {most} of {MAX_NEAR}");
        // Asked for far more than any step reaches, it's full, and says so.
        let (s, x) = crate::colony::pools::pool(0, 0);
        let all = Near::gather(0, Vec3::new(x, 0.0, -s), 1_000.0);
        assert!(all.full() && all.solids().len() == MAX_NEAR);
    }

    /// Its whole state, written out: equal to the bit (a float's `Debug` is exact).
    #[allow(clippy::disallowed_types, clippy::disallowed_macros)]
    fn written(m: &Markab) -> alloc::string::String {
        alloc::format!("{:?}", m.state())
    }

    /// `m` and a Markab made again from its state, ticked side by side on `c(k)` for 300 ticks:
    /// the same to the bit every tick.
    fn side_by_side(m: &mut Markab, c: impl Fn(u32) -> Controls) {
        let mut again = Markab::from_state(&m.state());
        assert_eq!(written(m), written(&again));
        for k in 0..300 {
            m.tick(&c(k));
            again.tick(&c(k));
            assert_eq!(written(m), written(&again), "tick {k}");
        }
    }

    #[test]
    fn a_markab_made_again_from_its_state_drives_on_to_the_bit() {
        // On a face, just stopped by its brakes (the pedal holding it a moment yet).
        let mut m = on_the_avenue();
        for _ in 0..60 {
            m.tick(&GO);
        }
        let brake = Controls { throttle: -1.0, ..IDLE };
        for _ in 0..300 {
            if m.stopped != 0.0 {
                break;
            }
            m.tick(&brake);
        }
        assert!(m.stopped != 0.0 && m.face().is_some(), "stopped on the road: {m:?}");
        side_by_side(&mut m, |k| if k < 30 { brake } else { Controls { steer: 0.5, ..GO } });
        // Round a corner: rolling up the Blast Hall's back wall.
        let mut m = at_the_doors(12.0);
        for _ in 0..900 {
            if matches!(m.on, On::Corner(_)) {
                break;
            }
            m.tick(&GO);
        }
        assert!(matches!(m.on, On::Corner(_)), "round the corner: {m:?}");
        side_by_side(&mut m, |_| GO);
        // In the air, the lift jets' key held down (let go and pressed again, it leaps).
        let mut m = on_the_avenue();
        for _ in 0..90 {
            m.tick(&Controls { boost: true, ..GO });
        }
        let lift = Controls { lift: true, ..IDLE };
        for _ in 0..3 {
            m.tick(&lift);
        }
        assert!(m.on == On::Air && m.lifting, "aloft, lifting: {m:?}");
        side_by_side(&mut m, |k| match k {
            0..40 => lift,
            40..60 => Controls { roll: 1.0, ..IDLE },
            60..62 => lift,
            _ => Controls { steer: 0.3, ..IDLE },
        });
    }
}
