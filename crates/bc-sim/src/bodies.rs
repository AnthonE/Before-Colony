//! Bodies: what a suit can land on, walk on, park on and hide in. Field rocks, and the sector's
//! landmarks (`content::landmarks`).
//!
//! Each body has a frame of its own. Its surface is a [`Shape`] in that frame, queried with
//! [`Shape::probe`] (signed distance and outward normal) and swept with [`Shape::trace`]. Its pose
//! is a closed form in the integer tick ([`landmark_pose`]; rocks don't move), so the server, a
//! client's prediction, lag compensation's rewind and the renderer agree to the bit, and body poses
//! never travel on the wire. [`Bodies`] is the view of them all at one tick.
//!
//! Everything here is fixed work in libm math: nothing allocates, and nothing iterates to
//! convergence.

use core::f32::consts::TAU;

use glam::{Quat, Vec3};

use crate::collide::segment_near_point;
use crate::config::DT;
use crate::content::landmarks::LandmarkDef;
use crate::field::{Field, SUIT_CLEARANCE};
use crate::flight::FlightState;
use crate::math::{cos, length, normalize_or, quat_axis_angle, quat_normalize, sin, sqrt};

/// Landmarks a sector holds at most (the wire could name 16).
pub const MAX_LANDMARKS: usize = 4;
/// Cuts a shape has at most.
pub const MAX_CUTS: usize = 4;
/// The smallest half-axis of a rock a suit can grip, m (75 of the default field's 160 rocks).
pub const GRIP_MIN_AXIS: f32 = 10.0;
/// How high a standing suit's origin rides over the ground, m (its soles are 9.07 m below it).
pub const STANCE: f32 = 9.125;
/// Sphere tracing a union: the most steps it takes, the shortest step, m (no feature is thinner
/// than 6 m, so none is stepped over), and how close counts as touching, m.
pub const TRACE_ITERS: u32 = 48;
pub const TRACE_MIN_STEP: f32 = 1.0;
pub const TRACE_EPS: f32 = 0.02;
/// The longest segment a union trace is sure to skip no feature of, m: its steps must reach `b`
/// within [`TRACE_ITERS`], so on a segment of `len` they can be `len / TRACE_ITERS` long, and this
/// is where that reaches the 6 m of the thinnest feature. Callers split longer segments.
pub const TRACE_MAX_LEN: f32 = TRACE_ITERS as f32 * 6.0;
/// Newton steps [`Shape::normal_from`] takes toward an ellipsoid's nearest point.
pub const NEAREST_ITERS: u32 = 8;
/// The most pieces [`Shape::trace_long`] cuts a segment into (18 km of them; a tick of the
/// fastest shot is 267 m).
pub const TRACE_MAX_PIECES: u32 = 64;

/// What a suit can stand on, fly in the grip of, or park on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Body {
    /// Nothing: it drifts.
    #[default]
    None,
    /// An asteroid of the field, by index.
    Rock(u16),
    /// A landmark (`content::landmarks::LANDMARKS`), by index.
    Landmark(u8),
}

impl Body {
    /// A number for each body, rocks before landmarks: what the state hash records, and how ties
    /// between bodies are broken.
    pub fn code(self) -> u32 {
        match self {
            Body::None => u32::MAX,
            Body::Rock(r) => u32::from(r),
            Body::Landmark(k) => 0x0001_0000 | u32::from(k),
        }
    }
}

/// A body's pose at a (fractional) tick, and how it's moving.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPose {
    pub pos: Vec3,
    pub rot: Quat,
    /// Velocity of its origin, m/s.
    pub vel: Vec3,
    /// Angular velocity, sector frame, rad/s.
    pub ang_vel: Vec3,
    /// Whether it moves at all (a static body's point velocity is exactly zero).
    pub moving: bool,
}

impl BodyPose {
    /// A body that doesn't move.
    pub const fn fixed(pos: Vec3, rot: Quat) -> Self {
        Self { pos, rot, vel: Vec3::ZERO, ang_vel: Vec3::ZERO, moving: false }
    }

    /// A point of the body's frame, in the sector's.
    #[inline]
    pub fn to_world(&self, l: Vec3) -> Vec3 {
        self.pos + self.rot * l
    }

    /// A point of the sector, in the body's frame.
    #[inline]
    pub fn to_local(&self, w: Vec3) -> Vec3 {
        self.rot.conjugate() * (w - self.pos)
    }

    /// The velocity of the body's material at world point `w`, m/s. Exactly `Vec3::ZERO` (positive
    /// zeros) for a body that doesn't move: it is never computed as 0 × r.
    #[inline]
    pub fn point_vel(&self, w: Vec3) -> Vec3 {
        if self.moving { self.vel + self.ang_vel.cross(w - self.pos) } else { Vec3::ZERO }
    }
}

/// What a surface query finds, in the body's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// Signed distance to the surface (negative inside), m.
    pub dist: f32,
    /// The outward unit normal there.
    pub normal: Vec3,
}

/// A body's shape before its cuts, in its own frame.
#[derive(Clone, Copy, Debug)]
pub enum Base {
    /// An ellipsoid of these half-axes about the origin.
    Ellipsoid(Vec3),
    /// Everything inside any of these.
    Union(&'static [Prim]),
}

/// A primitive of a union, axis-aligned in the body's frame.
#[derive(Clone, Copy, Debug)]
pub enum Prim {
    Sphere {
        c: Vec3,
        r: f32,
    },
    /// Everything within `r` of the segment `a`–`b`.
    Capsule {
        a: Vec3,
        b: Vec3,
        r: f32,
    },
    /// A box of half-extents `half` (exactly: the rounding is inside them) with its edges and
    /// corners rounded to `round`.
    RoundBox {
        c: Vec3,
        half: Vec3,
        round: f32,
    },
    /// A cylinder along the body's X axis, `half_len` either side of `c` and `r` in radius, its
    /// rims rounded to `round`.
    CylinderX {
        c: Vec3,
        half_len: f32,
        r: f32,
        round: f32,
    },
}

/// A sphere taken out of a shape: a bowl, or a hollow.
#[derive(Clone, Copy, Debug)]
pub struct SphereCut {
    pub c: Vec3,
    pub r: f32,
}

/// A body's solid shape, in its own frame: the base less its cuts.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub base: Base,
    pub cuts: &'static [SphereCut],
}

/// -1 or 1: the side of zero `x` is on (1 at zero).
#[inline]
fn sign(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

impl Prim {
    /// Exact signed distance and outward normal.
    pub fn probe(&self, p: Vec3) -> Probe {
        match *self {
            Prim::Sphere { c, r } => Probe { dist: length(p - c) - r, normal: normalize_or(p - c, Vec3::Y) },
            Prim::Capsule { a, b, r } => {
                let ab = b - a;
                let l2 = ab.dot(ab);
                let s = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let q = a + ab * s;
                Probe { dist: length(p - q) - r, normal: normalize_or(p - q, Vec3::Y) }
            }
            Prim::RoundBox { c, half, round } => {
                let rel = p - c;
                let q = rel.abs() - (half - Vec3::splat(round));
                let o = q.max(Vec3::ZERO);
                let m = q.max_element();
                let dist = length(o) + m.min(0.0) - round;
                let normal = if m > 0.0 {
                    normalize_or(o * Vec3::new(sign(rel.x), sign(rel.y), sign(rel.z)), Vec3::Y)
                } else if q.x >= q.y && q.x >= q.z {
                    Vec3::X * sign(rel.x)
                } else if q.y >= q.z {
                    Vec3::Y * sign(rel.y)
                } else {
                    Vec3::Z * sign(rel.z)
                };
                Probe { dist, normal }
            }
            Prim::CylinderX { c, half_len, r, round } => {
                // The round box's formula in the plane of the axis and the radius.
                let rel = p - c;
                let yz = Vec3::new(0.0, rel.y, rel.z);
                let rho2 = yz.dot(yz);
                let rho = sqrt(rho2);
                let radial = if rho2 > 1e-12 { yz / rho } else { Vec3::Y };
                let (qa, qr) = (rel.x.abs() - (half_len - round), rho - (r - round));
                let (oa, or) = (qa.max(0.0), qr.max(0.0));
                let m = qa.max(qr);
                let lo = sqrt(oa * oa + or * or);
                let dist = lo + m.min(0.0) - round;
                let normal = if m > 0.0 {
                    Vec3::X * (sign(rel.x) * oa / lo) + radial * (or / lo)
                } else if qa >= qr {
                    Vec3::X * sign(rel.x)
                } else {
                    radial
                };
                Probe { dist, normal }
            }
        }
    }
}

impl Shape {
    /// A plain ellipsoid (a field rock's shape).
    pub const fn ellipsoid(axes: Vec3) -> Self {
        Self { base: Base::Ellipsoid(axes), cuts: &[] }
    }

    /// Signed distance from `p` to the surface (negative inside), and the outward normal there.
    ///
    /// A union is exact outside (the nearest primitive; ties go to the lower index). An ellipsoid
    /// uses Inigo Quilez's first-order distance: exact on the surface, off by O(d²κ) away from it,
    /// so it is millimetre-true within a foot's height of it. A cut is subtracted exactly (ties go
    /// to the base). Normals are analytic, never finite differences.
    pub fn probe(&self, p: Vec3) -> Probe {
        let mut out = match self.base {
            Base::Ellipsoid(a) => {
                let k0 = length(p / a);
                let k1 = length(p / (a * a));
                let dist = if k1 > 1e-9 { k0 * (k0 - 1.0) / k1 } else { -a.min_element() };
                Probe { dist, normal: normalize_or(p / (a * a), Vec3::Y) }
            }
            Base::Union(prims) => {
                let mut best = Probe { dist: f32::INFINITY, normal: Vec3::Y };
                for prim in prims {
                    let pr = prim.probe(p);
                    if pr.dist < best.dist {
                        best = pr;
                    }
                }
                best
            }
        };
        for cut in self.cuts {
            let d = -(length(p - cut.c) - cut.r);
            if d > out.dist {
                out = Probe { dist: d, normal: normalize_or(cut.c - p, Vec3::Y) };
            }
        }
        out
    }

    /// The outward normal where the surface is nearest to `p`, a point outside it: which way is
    /// straight up from the ground below `p`. A union's (and a cut's) is the probe's own. An
    /// ellipsoid's first-order gradient isn't, off its surface: it leans toward the long axes, so a
    /// suit set down along it would creep. There the nearest point `x` is solved for: `p` is
    /// `x + s·x/a²` for the one `s ≥ 0` that puts `x` on the surface (Eberly's equation, by
    /// [`NEAREST_ITERS`] Newton steps from `s = 0`, which never overshoot), and the normal is along
    /// `x/a² = p/(a² + s)`.
    pub fn normal_from(&self, p: Vec3) -> Vec3 {
        let pr = self.probe(p);
        let Base::Ellipsoid(a) = self.base else { return pr.normal };
        let a2 = a * a;
        let q = p * a;
        let k0 = length(p / a);
        let k1 = length(p / a2);
        let base = if k1 > 1e-9 { k0 * (k0 - 1.0) / k1 } else { -a.min_element() };
        if k0 <= 1.0 || self.cuts.iter().any(|c| -(length(p - c.c) - c.r) > base) {
            return pr.normal;
        }
        let mut s = 0.0;
        for _ in 0..NEAREST_ITERS {
            let d = a2 + Vec3::splat(s);
            let v = q / d;
            let f = v.dot(v) - 1.0;
            let df = -2.0 * (v * v / d).dot(Vec3::ONE);
            if f <= 0.0 || df >= 0.0 {
                break;
            }
            s -= f / df;
        }
        normalize_or(p / (a2 + Vec3::splat(s)), pr.normal)
    }

    /// A lower bound on the distance from `p` to the shape, for tracing (the same as the probe's for
    /// a union; for an ellipsoid, `(|p/a| − 1)·min(a)`, since `p ↦ p/a` shrinks no distance by more
    /// than `min(a)`).
    pub fn bound(&self, p: Vec3) -> f32 {
        match self.base {
            Base::Union(_) => self.probe(p).dist,
            Base::Ellipsoid(a) => {
                let mut d = (length(p / a) - 1.0) * a.min_element();
                for cut in self.cuts {
                    d = d.max(-(length(p - cut.c) - cut.r));
                }
                d
            }
        }
    }

    /// How far along `a→b` (0..1, in the body's frame) a sphere of radius `r` first touches the
    /// shape: 0 if it starts touching and moves further in; None if it never touches, or only
    /// leaves (as [`Rock::sweep`](crate::field::Rock::sweep)).
    ///
    /// An ellipsoid's is exact: the ellipsoid grown by `r` less the cuts shrunk by `r`, solved in
    /// closed form. A union's is sphere tracing on its distance, which is conservative: it can
    /// step over a graze shorter than a step, and report a hit up to a step late. A step is at
    /// least [`TRACE_MIN_STEP`], and at least what spreads the rest of the segment over the steps
    /// left, so the trace always reaches `b`; up to [`TRACE_MAX_LEN`] that is at most 6 m.
    pub fn trace(&self, a: Vec3, b: Vec3, r: f32) -> Option<f32> {
        let len = length(b - a);
        if len < 1e-6 {
            return (self.bound(a) < r).then_some(0.0);
        }
        let dir = (b - a) / len;
        let start = self.probe(a);
        let d0 = start.dist - r;
        if d0 <= TRACE_EPS {
            // Touching: going in is a hit at once; going out, or along, isn't.
            return (dir.dot(start.normal) < 0.0).then_some(0.0);
        }
        match self.base {
            Base::Ellipsoid(axes) => self.solve(axes, a, b, r),
            Base::Union(_) => {
                let mut s = d0;
                for i in 0..TRACE_ITERS {
                    if s >= len {
                        break;
                    }
                    let d = self.bound(a + dir * s) - r;
                    if d <= TRACE_EPS {
                        return Some(s / len);
                    }
                    let spread = (len - s) / (TRACE_ITERS - i) as f32;
                    s += d.max(TRACE_MIN_STEP).max(spread);
                }
                (self.bound(b) - r <= 0.0).then_some(1.0)
            }
        }
    }

    /// [`Shape::trace`] for a segment of any length: one longer than [`TRACE_MAX_LEN`] is traced
    /// in equal pieces, in order, so a union's steps never skip a feature however far it goes (up
    /// to [`TRACE_MAX_PIECES`] pieces). As [`Shape::trace`] itself when it is short enough.
    pub fn trace_long(&self, a: Vec3, b: Vec3, r: f32) -> Option<f32> {
        let n = ((length(b - a) / TRACE_MAX_LEN) as u32 + 1).min(TRACE_MAX_PIECES);
        if n == 1 {
            return self.trace(a, b, r);
        }
        let at = |k: u32| k as f32 / n as f32;
        (0..n).find_map(|k| {
            let (s0, s1) = (at(k), at(k + 1));
            self.trace(a + (b - a) * s0, a + (b - a) * s1, r).map(|f| s0 + (s1 - s0) * f)
        })
    }

    /// The ellipsoid's sweep, solved: the segment's interval in the grown ellipsoid, less its
    /// intervals in the shrunk cuts. The hit is the first point of what's left: where it enters
    /// the ellipsoid, or where it leaves a cut inside it.
    fn solve(&self, axes: Vec3, a: Vec3, b: Vec3, r: f32) -> Option<f32> {
        let g = axes + Vec3::splat(r);
        let (p, d) = (a / g, (b - a) / g);
        let (dd, half_b, c) = (d.dot(d), p.dot(d), p.dot(p) - 1.0);
        let disc = half_b * half_b - dd * c;
        if disc < 0.0 {
            return None;
        }
        let sq = sqrt(disc);
        let (e0, e1) = ((-half_b - sq) / dd, (-half_b + sq) / dd);
        if e1 <= 0.0 || e0 > 1.0 {
            return None;
        }
        let mut hollows = [(0.0f32, 0.0f32); MAX_CUTS];
        let mut n = 0;
        let ab = b - a;
        let aa = ab.dot(ab);
        for cut in self.cuts.iter().take(MAX_CUTS) {
            let rk = cut.r - r;
            if rk <= 0.0 {
                continue;
            }
            let m = a - cut.c;
            let (hb, ck) = (m.dot(ab), m.dot(m) - rk * rk);
            let disc = hb * hb - aa * ck;
            if disc < 0.0 {
                continue;
            }
            let sq = sqrt(disc);
            hollows[n] = ((-hb - sq) / aa, (-hb + sq) / aa);
            n += 1;
        }
        let hollows = &hollows[..n];
        let open = |x: f32| !hollows.iter().any(|&(s0, s1)| s0 < x && x < s1);
        let entry = e0.max(0.0);
        let mut hit = None;
        // Starting inside the ellipsoid and leaving it is no hit (it's convex: there's no coming
        // back), unless the start is in a hollow.
        if !(e0 <= 0.0 && half_b >= 0.0) && entry <= 1.0 && open(entry) {
            hit = Some(entry);
        }
        for &(_, s1) in hollows {
            if s1 >= entry && s1 <= e1 && s1 <= 1.0 && open(s1) && hit.is_none_or(|h| s1 < h) {
                hit = Some(s1);
            }
        }
        hit
    }
}

/// A landmark's pose at tick `t` plus `frac` of the next: its station-keeping circle and its spin,
/// each a whole number of ticks round, taken modulo the tick so the phase never drifts.
pub fn landmark_pose(d: &LandmarkDef, t: u32, frac: f32) -> BodyPose {
    let (pos, vel) = if d.orbit_radius == 0.0 {
        (d.center, Vec3::ZERO)
    } else {
        let po = d.orbit_period;
        let k = (t % po + d.orbit_phase % po) % po;
        let b = (k as f32 + frac) * (TAU / po as f32);
        let (s, c) = (sin(b), cos(b));
        let w = TAU / (po as f32 * DT);
        (d.center + Vec3::new(c, 0.0, s) * d.orbit_radius, Vec3::new(-s, 0.0, c) * (d.orbit_radius * w))
    };
    let (rot, ang_vel) = if d.spin_period == 0 {
        (d.rot0, Vec3::ZERO)
    } else {
        let ps = d.spin_period;
        let th = ((t % ps) as f32 + frac) * (TAU / ps as f32);
        (
            quat_normalize(d.rot0 * quat_axis_angle(d.spin_axis, th)),
            d.rot0 * (d.spin_axis * (TAU / (ps as f32 * DT))),
        )
    };
    BodyPose { pos, rot, vel, ang_vel, moving: d.orbit_radius != 0.0 || d.spin_period != 0 }
}

/// The first of `landmarks` a sphere of radius `r` meets moving from `a` to `b`, with each where it
/// was at tick `t` plus `frac` of the next: how far along (0..1), and which. A landmark is posed
/// only if the move comes near where it could be, so a shot nowhere near one costs a distance
/// test each. [`Bodies::sweep_landmarks`] for a caller that has none at hand.
pub fn sweep_landmarks(
    landmarks: &[LandmarkDef],
    a: Vec3,
    b: Vec3,
    r: f32,
    t: u32,
    frac: f32,
) -> Option<(f32, u8)> {
    sweep_posed(landmarks, a, b, r, |_, d| landmark_pose(d, t, frac))
}

/// The first of `landmarks` a sphere of radius `r` at `p` overlaps, with each where it is at tick
/// `t`. What a sweep can't see: a landmark's own motion carries its surface into what it sweeps
/// as though still.
pub fn landmark_touching(landmarks: &[LandmarkDef], p: Vec3, r: f32, t: u32) -> Option<u8> {
    landmarks
        .iter()
        .position(|d| {
            length(p - d.center) <= d.bound + d.orbit_radius + r
                && d.shape.probe(landmark_pose(d, t, 0.0).to_local(p)).dist < r
        })
        .map(|k| k as u8)
}

/// [`sweep_landmarks`], with each landmark posed by `pose` (its index, and it) once the move is
/// near it. The earliest wins; a tie goes to the lower index.
fn sweep_posed(
    landmarks: &[LandmarkDef],
    a: Vec3,
    b: Vec3,
    r: f32,
    pose: impl Fn(usize, &LandmarkDef) -> BodyPose,
) -> Option<(f32, u8)> {
    let mut best: Option<(f32, u8)> = None;
    for (k, d) in landmarks.iter().enumerate() {
        if !segment_near_point(a, b, d.center, d.bound + d.orbit_radius + r) {
            continue;
        }
        let p = pose(k, d);
        if let Some(s) = d.shape.trace_long(p.to_local(a), p.to_local(b), r)
            && best.is_none_or(|(bs, _)| s < bs)
        {
            best = Some((s, k as u8));
        }
    }
    best
}

/// The nearest grippable surface to a suit ([`Bodies::nearest_grippable`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    pub body: Body,
    pub pose: BodyPose,
    /// The suit's origin in the body's frame.
    pub local: Vec3,
    /// The surface's outward normal nearest the suit, in the body's frame and in the sector's.
    pub n_local: Vec3,
    pub n_world: Vec3,
    /// How high the suit's feet are over the surface (standing), m.
    pub h: f32,
    /// The suit's velocity relative to the surface under it, sector frame, m/s.
    pub v_rel: Vec3,
}

/// Every body at one tick: the field's rocks (which don't move) and the sector's landmarks, posed.
pub struct Bodies<'a> {
    pub field: &'a Field,
    pub landmarks: &'static [LandmarkDef],
    pub t: u32,
    now: [BodyPose; MAX_LANDMARKS],
}

impl<'a> Bodies<'a> {
    /// The bodies at tick `t` (at most [`MAX_LANDMARKS`] of `landmarks`).
    pub fn at(field: &'a Field, landmarks: &'static [LandmarkDef], t: u32) -> Self {
        let landmarks = &landmarks[..landmarks.len().min(MAX_LANDMARKS)];
        let mut now = [BodyPose::fixed(Vec3::ZERO, Quat::IDENTITY); MAX_LANDMARKS];
        for (k, d) in landmarks.iter().enumerate() {
            now[k] = landmark_pose(d, t, 0.0);
        }
        Self { field, landmarks, t, now }
    }

    /// Where `b` is now. A shattered rock still has its pose (the seed says where it was).
    pub fn pose(&self, b: Body) -> Option<BodyPose> {
        match b {
            Body::Landmark(k) => (usize::from(k) < self.landmarks.len()).then(|| self.now[usize::from(k)]),
            _ => self.pose_at(b, self.t, 0.0),
        }
    }

    /// Where `b` is at tick `t` plus `frac` of the next.
    pub fn pose_at(&self, b: Body, t: u32, frac: f32) -> Option<BodyPose> {
        match b {
            Body::None => None,
            Body::Rock(r) => {
                self.field.rocks().get(usize::from(r)).map(|rock| BodyPose::fixed(rock.pos, rock.rot))
            }
            Body::Landmark(k) => self.landmarks.get(usize::from(k)).map(|d| landmark_pose(d, t, frac)),
        }
    }

    /// Whether `b` is there to stand on: a rock not shattered, or a landmark of this sector.
    pub fn alive(&self, b: Body) -> bool {
        match b {
            Body::None => false,
            Body::Rock(r) => usize::from(r) < self.field.len() && !self.field.is_dead(usize::from(r)),
            Body::Landmark(k) => usize::from(k) < self.landmarks.len(),
        }
    }

    /// Whether a suit can grip `b`: a rock big enough ([`GRIP_MIN_AXIS`]), or a landmark made to be.
    pub fn grippable(&self, b: Body) -> bool {
        self.alive(b)
            && match b {
                Body::Rock(r) => self.field.rocks()[usize::from(r)].axes.min_element() >= GRIP_MIN_AXIS,
                Body::Landmark(k) => self.landmarks[usize::from(k)].grippable,
                Body::None => false,
            }
    }

    /// The shape of `b`, in its frame.
    pub fn shape(&self, b: Body) -> Option<Shape> {
        match b {
            Body::None => None,
            Body::Rock(r) => self.field.rocks().get(usize::from(r)).map(|rock| Shape::ellipsoid(rock.axes)),
            Body::Landmark(k) => self.landmarks.get(usize::from(k)).map(|d| d.shape),
        }
    }

    /// The nearest surface a suit could grip: its feet within `range` of it, moving no faster than
    /// `max_speed` relative to it, nor leaving it faster than `max_leave`. The nearest wins; a tie
    /// goes to the lower body (rocks before landmarks), whatever order the grid lists them in.
    pub fn nearest_grippable(
        &self,
        f: &FlightState,
        range: f32,
        max_speed: f32,
        max_leave: f32,
    ) -> Option<Near> {
        let reach = range + STANCE;
        let mut best: Option<Near> = None;
        let mut consider = |body: Body, pose: BodyPose, shape: Shape| {
            let local = pose.to_local(f.pos);
            let pr = shape.probe(local);
            let h = pr.dist - STANCE;
            if h > range {
                return;
            }
            let n_world = pose.rot * pr.normal;
            let v_rel = f.vel - pose.point_vel(f.pos);
            if length(v_rel) > max_speed || v_rel.dot(n_world) > max_leave {
                return;
            }
            if best.is_none_or(|b| h < b.h || (h == b.h && body.code() < b.body.code())) {
                best = Some(Near { body, pose, local, n_local: pr.normal, n_world, h, v_rel });
            }
        };
        let pad = Vec3::splat(reach);
        self.field.for_each_in_box(f.pos - pad, f.pos + pad, |i| {
            let rock = &self.field.rocks()[i];
            if !self.field.is_dead(i) && rock.axes.min_element() >= GRIP_MIN_AXIS {
                consider(
                    Body::Rock(i as u16),
                    BodyPose::fixed(rock.pos, rock.rot),
                    Shape::ellipsoid(rock.axes),
                );
            }
        });
        for (k, d) in self.landmarks.iter().enumerate() {
            if d.grippable && length(f.pos - d.center) <= d.bound + d.orbit_radius + reach {
                consider(Body::Landmark(k as u8), self.now[k], d.shape);
            }
        }
        best
    }

    /// The first landmark a sphere of radius `r` meets moving from `a` to `b`, with the landmarks
    /// where they were at tick `t` plus `frac` of the next: how far along (0..1), and which.
    pub fn sweep_landmarks(&self, a: Vec3, b: Vec3, r: f32, t: u32, frac: f32) -> Option<(f32, u8)> {
        let now = t == self.t && frac == 0.0;
        sweep_posed(self.landmarks, a, b, r, |k, d| if now { self.now[k] } else { landmark_pose(d, t, frac) })
    }

    /// Keeps a suit out of the landmarks (all but `except`, the one it rides), as
    /// [`Field::collide`] keeps it out of rocks: its move this tick (from `prev`) is swept, meeting
    /// one stops it there, and ending inside one it was touching slides it out along the surface.
    /// The first contact wins. The speed it had into the surface, relative to the surface, is lost.
    /// Whether it met one.
    pub fn collide_landmarks(&self, prev: Vec3, s: &mut FlightState, except: Option<u8>) -> bool {
        let r = SUIT_CLEARANCE;
        let mut first: Option<(f32, usize, Vec3)> = None;
        for (k, d) in self.landmarks.iter().enumerate() {
            if except == Some(k as u8)
                || !segment_near_point(prev, s.pos, d.center, d.bound + d.orbit_radius + r)
            {
                continue;
            }
            let pose = self.now[k];
            let (a, b) = (pose.to_local(prev), pose.to_local(s.pos));
            let (t, at) = match d.shape.trace(a, b, r) {
                Some(t) if t > 0.0 => (t, a + (b - a) * t),
                _ if d.shape.probe(b).dist < r => (0.0, b),
                _ => continue,
            };
            if first.is_none_or(|(ft, _, _)| t < ft) {
                first = Some((t, k, at));
            }
        }
        let Some((_, k, mut at)) = first else { return false };
        let pose = self.now[k];
        let pr = self.landmarks[k].shape.probe(at);
        if pr.dist < r {
            at += pr.normal * (r - pr.dist);
        }
        s.pos = pose.to_world(at);
        let n = pose.rot * pr.normal;
        let vn = (s.vel - pose.point_vel(s.pos)).dot(n);
        if vn < 0.0 {
            s.vel -= n * vn;
        }
        true
    }

    /// The outermost point of `b`'s surface straight out from its origin along `dir_local`, and
    /// the normal there (both in its frame).
    pub fn surface_along(&self, b: Body, dir_local: Vec3) -> Option<(Vec3, Vec3)> {
        let shape = self.shape(b)?;
        let reach = match b {
            Body::Rock(r) => self.field.rocks()[usize::from(r)].radius,
            Body::Landmark(k) => self.landmarks[usize::from(k)].bound,
            Body::None => return None,
        } + 1.0;
        let dir = normalize_or(dir_local, Vec3::Y);
        let p = match shape.base {
            Base::Ellipsoid(_) => dir * (reach * (1.0 - shape.trace(dir * reach, Vec3::ZERO, 0.0)?)),
            Base::Union(_) => {
                // Sphere tracing in from outside, with no shortest step, never passes the surface.
                // Not in the tick, it takes the steps it needs.
                let mut s = reach;
                for _ in 0..1_024 {
                    let d = shape.bound(dir * s);
                    if d < 1e-4 {
                        break;
                    }
                    s -= d;
                    if s <= 0.0 {
                        return None;
                    }
                }
                dir * s
            }
        };
        Some((p, shape.probe(p).normal))
    }

    /// The first hide spot of `b` (a landmark) that `local`, in its frame, is in.
    pub fn hide_spot_of(&self, b: Body, local: Vec3) -> Option<u8> {
        let Body::Landmark(k) = b else { return None };
        let d = self.landmarks.get(usize::from(k))?;
        d.hides.iter().position(|h| length(local - h.center) <= h.radius).map(|i| i as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::landmarks::LANDMARKS;
    use crate::field::Rock;
    use crate::math::{Rng, angle_between};
    use core::f32::consts::PI;

    fn mo_ii() -> Shape {
        LANDMARKS[0].shape
    }

    fn hermit() -> Shape {
        LANDMARKS[1].shape
    }

    fn mo_ii_prims() -> &'static [Prim] {
        let Base::Union(prims) = mo_ii().base else { unreachable!("MO-II is a union") };
        prims
    }

    fn unit(rng: &mut Rng) -> Vec3 {
        loop {
            let v = Vec3::new(rng.signed(), rng.signed(), rng.signed());
            if (0.01..=1.0).contains(&v.length_squared()) {
                return normalize_or(v, Vec3::Y);
            }
        }
    }

    fn point(rng: &mut Rng, lo: Vec3, hi: Vec3) -> Vec3 {
        lo + (hi - lo) * Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32())
    }

    /// Whether `q` is inside `prim`, from its definition (not its distance).
    fn in_prim(prim: &Prim, q: Vec3) -> bool {
        match *prim {
            Prim::Sphere { c, r } => (q - c).length_squared() < r * r,
            Prim::Capsule { a, b, r } => {
                let ab = b - a;
                let s = ((q - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
                (q - (a + ab * s)).length_squared() < r * r
            }
            Prim::RoundBox { c, half, round } => {
                let e = (q - c).abs() - (half - Vec3::splat(round));
                e.max_element() <= 0.0 || e.max(Vec3::ZERO).length_squared() < round * round
            }
            Prim::CylinderX { c, half_len, r, round } => {
                let rel = q - c;
                let (ea, er) =
                    (rel.x.abs() - (half_len - round), sqrt(rel.y * rel.y + rel.z * rel.z) - (r - round));
                (ea <= 0.0 && er <= 0.0) || ea.max(0.0).powi(2) + er.max(0.0).powi(2) < round * round
            }
        }
    }

    /// Whether `q` is inside `shape`, from its definition.
    fn solid(shape: &Shape, q: Vec3) -> bool {
        let base = match shape.base {
            Base::Ellipsoid(a) => (q / a).length_squared() < 1.0,
            Base::Union(prims) => prims.iter().any(|p| in_prim(p, q)),
        };
        base && !shape.cuts.iter().any(|c| (q - c.c).length_squared() < c.r * c.r)
    }

    /// The box a primitive fills.
    fn extent(prim: &Prim) -> (Vec3, Vec3) {
        match *prim {
            Prim::Sphere { c, r } => (c - Vec3::splat(r), c + Vec3::splat(r)),
            Prim::Capsule { a, b, r } => (a.min(b) - Vec3::splat(r), a.max(b) + Vec3::splat(r)),
            Prim::RoundBox { c, half, .. } => (c - half, c + half),
            Prim::CylinderX { c, half_len, r, .. } => {
                let e = Vec3::new(half_len, r, r);
                (c - e, c + e)
            }
        }
    }

    #[test]
    fn ellipsoid_probe_is_exact_on_the_surface() {
        let mut rng = Rng::new(1);
        for axes in [
            Vec3::new(900.0, 620.0, 760.0),
            Vec3::new(30.0, 12.0, 10.0),
            Vec3::splat(10.0),
            Vec3::new(102.4, 32.0, 64.0),
        ] {
            let shape = Shape::ellipsoid(axes);
            let h = 1e-3 * axes.max_element();
            let d = |p: Vec3| shape.probe(p).dist;
            for _ in 0..500 {
                let p = axes * unit(&mut rng);
                let pr = shape.probe(p);
                assert!(
                    pr.dist.abs() < 1e-4 * axes.max_element(),
                    "{axes}: {} off the surface at {p}",
                    pr.dist
                );
                // The distance's gradient is the normal, and a unit vector: it is a true distance there.
                let grad = Vec3::new(
                    d(p + Vec3::X * h) - d(p - Vec3::X * h),
                    d(p + Vec3::Y * h) - d(p - Vec3::Y * h),
                    d(p + Vec3::Z * h) - d(p - Vec3::Z * h),
                ) / (2.0 * h);
                assert!((length(grad) - 1.0).abs() < 1e-3, "{axes}: |∇d| = {} at {p}", length(grad));
                assert!(length(pr.normal - grad) < 2e-3, "{axes}: normal {} vs ∇d {grad} at {p}", pr.normal);
            }
        }
    }

    #[test]
    fn primitive_sdfs_are_exact_and_lipschitz() {
        let others = [
            Prim::Sphere { c: Vec3::new(3.0, -2.0, 5.0), r: 12.0 },
            Prim::Capsule { a: Vec3::new(-10.0, 0.0, 4.0), b: Vec3::new(10.0, 5.0, -3.0), r: 3.0 },
        ];
        let mut rng = Rng::new(2);
        for prim in mo_ii_prims().iter().chain(&others) {
            let (lo, hi) = extent(prim);
            let pad = Vec3::splat(20.0);
            let mut smooth = 0;
            for _ in 0..10_000 {
                let p = point(&mut rng, lo - pad, hi + pad);
                let q = p + unit(&mut rng) * if rng.next_f32() < 0.5 { 2.0 } else { 40.0 };
                let (a, b) = (prim.probe(p), prim.probe(q));
                if a.dist.abs() > 1e-3 {
                    assert_eq!(a.dist < 0.0, in_prim(prim, p), "{prim:?}: sign wrong at {p} ({})", a.dist);
                }
                assert!(
                    (a.dist - b.dist).abs() <= length(p - q) + 1e-4,
                    "{prim:?}: not 1-Lipschitz at {p}, {q}"
                );
                assert!((length(a.normal) - 1.0).abs() < 1e-5);
                // With that, the nearest point being on the surface makes the distance exact.
                let foot = p - a.normal * a.dist;
                assert!(prim.probe(foot).dist.abs() < 1e-3, "{prim:?}: {p}'s foot {foot} is off the surface");
                // Away from creases (where the one-sided differences disagree), the normal is the
                // distance's gradient.
                let h = 0.02;
                let mut grad = Vec3::ZERO;
                let mut crease = false;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let (f, r) =
                        (prim.probe(p + axis * h).dist - a.dist, a.dist - prim.probe(p - axis * h).dist);
                    crease |= (f - r).abs() > 5e-4;
                    grad += axis * ((f + r) / (2.0 * h));
                }
                if !crease {
                    assert!(
                        length(a.normal - grad) < 1e-2,
                        "{prim:?}: normal {} vs ∇d {grad} at {p}",
                        a.normal
                    );
                    smooth += 1;
                }
            }
            assert!(smooth > 5_000, "{prim:?}: only {smooth} normals checked");
        }
    }

    #[test]
    fn union_ties_go_low_and_cuts_subtract() {
        const TWINS: [Prim; 2] =
            [Prim::Sphere { c: Vec3::ZERO, r: 10.0 }, Prim::Sphere { c: Vec3::new(20.0, 0.0, 0.0), r: 10.0 }];
        const SWAPPED: [Prim; 2] = [TWINS[1], TWINS[0]];
        // Exactly as far from each: the first listed gives the normal.
        let p = Vec3::new(10.0, 5.0, 0.0);
        let a = Shape { base: Base::Union(&TWINS), cuts: &[] }.probe(p);
        let b = Shape { base: Base::Union(&SWAPPED), cuts: &[] }.probe(p);
        assert_eq!(a.dist, b.dist);
        assert_eq!(a.normal, TWINS[0].probe(p).normal);
        assert_eq!(b.normal, TWINS[1].probe(p).normal);
        assert_ne!(a.normal, b.normal);

        const BALL: [Prim; 1] = [Prim::Sphere { c: Vec3::ZERO, r: 5.0 }];
        const BITE: [SphereCut; 1] = [SphereCut { c: Vec3::new(6.0, 0.0, 0.0), r: 5.0 }];
        let bitten = Shape { base: Base::Union(&BALL), cuts: &BITE };
        // Where the ball's surface meets the bite's, the tie goes to the ball.
        assert_eq!(
            bitten.probe(Vec3::new(3.0, 4.0, 0.0)),
            Probe { dist: 0.0, normal: Vec3::new(0.6, 0.8, 0.0) }
        );
        // In the bite, it's outside, and the way out of the ball is into the bite.
        assert_eq!(bitten.probe(Vec3::new(4.0, 0.0, 0.0)), Probe { dist: 3.0, normal: Vec3::X });
        // Clear of the bite, the ball is untouched.
        assert_eq!(bitten.probe(Vec3::new(-8.0, 0.0, 0.0)), BALL[0].probe(Vec3::new(-8.0, 0.0, 0.0)));
        // Hermit's bowls: a floor is on the surface, facing up out of the bowl.
        assert_eq!(hermit().probe(Vec3::new(0.0, 590.0, 0.0)), Probe { dist: 0.0, normal: Vec3::Y });
    }

    #[test]
    fn bound_is_a_lower_bound() {
        let rock = Shape::ellipsoid(Vec3::new(30.0, 10.0, 12.0));
        let bare = Shape::ellipsoid(Vec3::new(900.0, 620.0, 760.0));
        let mut rng = Rng::new(3);
        for (shape, reach) in [(mo_ii(), 335.0), (hermit(), 900.0), (bare, 900.0), (rock, 30.0)] {
            let (mut outside, mut inside) = (0, 0);
            while outside < 400 {
                let p = match (rng.next_u32() % 3, shape.base) {
                    (0, _) => point(&mut rng, Vec3::splat(-reach - 60.0), Vec3::splat(reach + 60.0)),
                    (1, Base::Ellipsoid(a)) => a * unit(&mut rng) * (0.3 + rng.next_f32() * 1.2),
                    (1, Base::Union(prims)) => {
                        let (lo, hi) = extent(&prims[(rng.next_u32() as usize) % prims.len()]);
                        point(&mut rng, lo - Vec3::splat(30.0), hi + Vec3::splat(30.0))
                    }
                    _ if shape.cuts.is_empty() => continue,
                    _ => {
                        let cut = shape.cuts[(rng.next_u32() as usize) % shape.cuts.len()];
                        cut.c + unit(&mut rng) * (rng.next_f32() * 120.0)
                    }
                };
                let bound = shape.bound(p);
                if bound < -1e-3 {
                    assert!(solid(&shape, p), "{:?}: bound {bound} < 0 outside, at {p}", shape.base);
                    inside += 1;
                    continue;
                }
                if !(1e-3..=60.0).contains(&bound) {
                    continue;
                }
                assert!(!solid(&shape, p), "{:?}: bound {bound} > 0 inside, at {p}", shape.base);
                outside += 1;
                // Nothing solid within `bound` of it, marching out along 64 rays.
                for _ in 0..64 {
                    let dir = unit(&mut rng);
                    let mut s = 0.25;
                    while s < bound - 1e-3 {
                        assert!(
                            !solid(&shape, p + dir * s),
                            "{:?}: solid {s} m from {p}, bound {bound}",
                            shape.base
                        );
                        s += 0.5;
                    }
                    let edge = p + dir * (bound - 1e-3);
                    assert!(!solid(&shape, edge), "{:?}: solid at the bound from {p}", shape.base);
                }
            }
            assert!(inside > 20, "{:?}: only {inside} points inside", shape.base);
        }
    }

    /// Where a sphere of radius `r` first touches `shape` moving from `a` to `b` by `probe`, at 5 cm
    /// steps: how far along, m, and for how long it stays touching, m.
    fn sampled_contact(shape: &Shape, a: Vec3, b: Vec3, r: f32) -> Option<(f32, f32)> {
        const STEP: f32 = 0.05;
        let len = length(b - a);
        let n = (len / STEP) as u32 + 1;
        let mut first: Option<f32> = None;
        for k in 0..=n {
            let s = len * k as f32 / n as f32;
            let touching = shape.probe(a + (b - a) * (s / len)).dist - r <= 0.0;
            match (touching, first) {
                (true, None) => first = Some(s),
                (false, Some(f)) => return Some((f, s - f)),
                _ => {}
            }
        }
        first.map(|f| (f, len - f + STEP))
    }

    /// Where along `a→b` the first contact at least `min` long begins, sampled every 5 cm.
    fn sampled_long_contact(shape: &Shape, a: Vec3, b: Vec3, r: f32, min: f32) -> Option<f32> {
        const STEP: f32 = 0.05;
        let len = length(b - a);
        let n = (len / STEP) as u32 + 1;
        let mut run: Option<f32> = None;
        for k in 0..=n {
            let s = len * k as f32 / n as f32;
            let touching = shape.probe(a + (b - a) * (s / len)).dist - r <= 0.0;
            match (touching, run) {
                (true, None) => run = Some(s),
                (true, Some(f)) if s - f >= min => return Some(f),
                (false, Some(_)) => run = None,
                _ => {}
            }
        }
        run.filter(|&f| len - f + STEP >= min)
    }

    /// Whether `q` is in Hermit's model for a sphere of radius `r`: the ellipsoid grown by `r`,
    /// less the cuts shrunk by it (in f64).
    fn in_grown_hermit(q: [f64; 3], r: f64) -> bool {
        let Base::Ellipsoid(a) = hermit().base else { unreachable!() };
        let g = [f64::from(a.x) + r, f64::from(a.y) + r, f64::from(a.z) + r];
        let inside = (0..3).map(|i| (q[i] / g[i]).powi(2)).sum::<f64>() < 1.0;
        inside
            && !hermit().cuts.iter().any(|c| {
                let rk = f64::from(c.r) - r;
                let c = [f64::from(c.c.x), f64::from(c.c.y), f64::from(c.c.z)];
                rk > 0.0 && (0..3).map(|i| (q[i] - c[i]).powi(2)).sum::<f64>() < rk * rk
            })
    }

    #[test]
    fn traces_match_dense_sampling() {
        let mut rng = Rng::new(4);
        let radii = [0.0, 0.5, 2.0, 8.0];
        // MO-II: sphere tracing. Every hit it reports is a contact, never later than a step after
        // the first one, and it misses only grazes shorter than a step.
        let shape = mo_ii();
        let (mut hits, mut grazes) = (0, 0);
        for _ in 0..10_000 {
            let r = radii[(rng.next_u32() % 4) as usize];
            let (a, target) = loop {
                let target = if rng.next_f32() < 0.2 {
                    Vec3::new(-240.0, 0.0, 0.0) + unit(&mut rng) * 60.0
                } else {
                    let (lo, hi) = extent(&mo_ii_prims()[(rng.next_u32() % 8) as usize]);
                    point(&mut rng, lo - Vec3::splat(10.0), hi + Vec3::splat(10.0))
                };
                let a = target + unit(&mut rng) * (5.0 + rng.next_f32() * 50.0);
                if shape.probe(a).dist - r > 0.5 {
                    break (a, target);
                }
            };
            let b = a + normalize_or(target - a, Vec3::X) * (1.0 + rng.next_f32() * 44.0);
            let len = length(b - a);
            let seen = sampled_contact(&shape, a, b, r);
            match (shape.trace(a, b, r), seen) {
                (Some(f), seen) => {
                    hits += 1;
                    let at = a + (b - a) * f;
                    let gap = shape.probe(at).dist - r;
                    assert!(gap <= TRACE_EPS + 1e-3, "{a} → {b} r {r}: hit at {f} is {gap} m clear");
                    if let Some((s, _)) = seen {
                        assert!(
                            f * len <= s + TRACE_MIN_STEP + 0.06,
                            "{a} → {b} r {r}: hit at {} m, touches at {s}",
                            f * len
                        );
                    }
                }
                (None, Some((s, run))) => {
                    assert!(
                        run < TRACE_MIN_STEP + 0.1,
                        "{a} → {b} r {r}: missed {run} m of contact from {s} m"
                    );
                    grazes += 1;
                }
                (None, None) => {}
            }
        }
        assert!(hits > 3_000 && grazes < 200, "MO-II: {hits} hits, {grazes} grazes missed");

        // Long segments, up to TRACE_MAX_LEN (a tick of the fastest shot is 267 m), skimming the
        // core, the pylons and the modules: the steps spread to reach b, so the trace still steps
        // over no contact longer than a step (at most 6 m), and a hit is at most a step late. A
        // graze it steps over can come before the contact it reports, so that is measured from
        // the first contact at least a step long.
        let skims = [
            (Vec3::new(0.0, 44.5, 44.5), Vec3::new(266.7, 44.5, 44.5), 0.5),
            (Vec3::new(50.0, 63.0, 0.0), Vec3::new(338.0, 63.0, 0.0), 0.5),
            (Vec3::new(50.0, 0.0, -65.0), Vec3::new(338.0, 0.0, -65.0), 2.0),
        ];
        let (mut hits, mut far) = (0, 0);
        for k in 0..6_000 {
            let (a, b, r) = if let Some(&skim) = skims.get(k) {
                skim
            } else {
                let r = radii[(rng.next_u32() % 4) as usize];
                let a = loop {
                    let (th, rho) = (rng.signed() * PI, 55.0 + rng.next_f32() * 40.0);
                    let a = Vec3::new(-320.0 + rng.next_f32() * 320.0, rho * cos(th), rho * sin(th));
                    if shape.probe(a).dist - r > 0.5 {
                        break a;
                    }
                };
                let dir = normalize_or(Vec3::X + unit(&mut rng) * 0.15, Vec3::X);
                (a, a + dir * (45.0 + rng.next_f32() * (TRACE_MAX_LEN - 45.0)), r)
            };
            assert!(shape.probe(a).dist - r > 0.5, "{a} r {r} starts touching");
            let len = length(b - a);
            let step = TRACE_MIN_STEP.max(len / TRACE_ITERS as f32);
            let long = sampled_long_contact(&shape, a, b, r, step + 0.1);
            match shape.trace(a, b, r) {
                Some(f) => {
                    hits += 1;
                    let gap = shape.probe(a + (b - a) * f).dist - r;
                    assert!(gap <= TRACE_EPS + 1e-3, "{a} → {b} r {r}: hit at {f} is {gap} m clear");
                    if let Some(s) = long {
                        assert!(
                            f * len <= s + step + 0.06,
                            "{a} → {b} r {r}: hit at {} m, touches for a step at {s}",
                            f * len
                        );
                    }
                    far += usize::from(f * len > 48.0);
                }
                None => assert!(long.is_none(), "{a} → {b} r {r}: missed a contact at {long:?} m"),
            }
        }
        assert!(hits > 2_500 && far > 1_000, "MO-II, long: {hits} hits, {far} past 48 m");

        // Hermit: solved exactly, so it matches the sampled model (refined by bisection) to 1e-4.
        let shape = hermit();
        let Base::Ellipsoid(axes) = shape.base else { unreachable!() };
        let (mut exact, mut into_bowls) = (0, 0);
        for _ in 0..10_000 {
            let r = radii[(rng.next_u32() % 4) as usize];
            let near_bowl = rng.next_f32() < 0.5;
            let (a, target) = loop {
                let (a, target) = if near_bowl {
                    let h = LANDMARKS[1].hides[(rng.next_u32() % 3) as usize];
                    let n = shape.probe(h.center).normal;
                    let a = h.center + n * (10.0 + rng.next_f32() * 40.0) + unit(&mut rng) * 40.0;
                    (a, h.center + unit(&mut rng) * 70.0)
                } else {
                    let on = axes * unit(&mut rng);
                    let a = on + shape.probe(on).normal * (r + rng.next_f32() * 40.0) + unit(&mut rng) * 20.0;
                    (a, on + unit(&mut rng) * 20.0)
                };
                let a64 = [f64::from(a.x), f64::from(a.y), f64::from(a.z)];
                if shape.probe(a).dist - r > 0.5 && !in_grown_hermit(a64, f64::from(r)) {
                    break (a, target);
                }
            };
            let b = a + normalize_or(target - a, Vec3::X) * (1.0 + rng.next_f32() * 44.0);
            let len = length(b - a);
            // The model, sampled every 5 cm, and its first contact bisected.
            let at = |f: f64| {
                let (a, b) = (a.as_dvec3(), b.as_dvec3());
                let q = a + (b - a) * f;
                [q.x, q.y, q.z]
            };
            let n = (len / 0.05) as u32 + 1;
            let first = (0..=n).find(|&k| in_grown_hermit(at(f64::from(k) / f64::from(n)), f64::from(r)));
            let seen = first.map(|k| {
                let (mut lo, mut hi) = (f64::from(k.max(1) - 1) / f64::from(n), f64::from(k) / f64::from(n));
                for _ in 0..50 {
                    let mid = 0.5 * (lo + hi);
                    if in_grown_hermit(at(mid), f64::from(r)) { hi = mid } else { lo = mid }
                }
                let run = (k..=n)
                    .take_while(|&j| in_grown_hermit(at(f64::from(j) / f64::from(n)), f64::from(r)))
                    .count();
                (hi, run as f32 * len / n as f32)
            });
            match (shape.trace(a, b, r), seen) {
                (Some(f), Some((s, run))) => {
                    into_bowls += usize::from(shape.cuts.iter().any(|c| length(a + (b - a) * f - c.c) < c.r));
                    // A graze is ill-conditioned (the roots meet), so only a clean entry is held to 1e-4.
                    exact += usize::from(run >= 1.0);
                    let tol = if run >= 1.0 { 1e-4 } else { 0.5 / len };
                    assert!(
                        (f64::from(f) - s).abs() <= f64::from(tol),
                        "{a} → {b} r {r}: solved {f}, sampled {s}"
                    );
                }
                (None, Some((s, run))) => panic!("{a} → {b} r {r}: missed {run} m of contact from {s}"),
                (Some(f), None) => {
                    // A graze thinner than the sampling: at the model's edge.
                    let q = a + (b - a) * f;
                    let edge = (length(q / (axes + Vec3::splat(r))) - 1.0) * axes.max_element();
                    assert!(edge.abs() < 0.05, "{a} → {b} r {r}: hit at {f}, {edge} m off the model");
                }
                (None, None) => {}
            }
        }
        assert!(exact > 1_500 && into_bowls > 500, "Hermit: {exact} clean hits, {into_bowls} in the bowls");
    }

    #[test]
    fn long_traces_go_in_pieces() {
        let mut rng = Rng::new(9);
        let radii = [0.0, 0.5, 2.0, 8.0];
        // Up to TRACE_MAX_LEN it is the trace itself, bit for bit.
        for shape in [mo_ii(), hermit()] {
            let reach = if matches!(shape.base, Base::Union(_)) { 400.0 } else { 1_000.0 };
            for _ in 0..2_000 {
                let r = radii[(rng.next_u32() % 4) as usize];
                let a = unit(&mut rng) * reach;
                let b = a + unit(&mut rng) * (rng.next_f32() * (TRACE_MAX_LEN - 1.0));
                assert_eq!(
                    shape.trace_long(a, b, r).map(f32::to_bits),
                    shape.trace(a, b, r).map(f32::to_bits),
                    "{a} → {b} r {r}"
                );
            }
        }
        // Longer, skimming MO-II's core, pylons and modules end to end: in pieces of at most
        // TRACE_MAX_LEN, whose steps are at most 6 m, it steps over no contact longer than that,
        // and a hit is at most a step late. In one go its steps would be up to 25 m.
        let shape = mo_ii();
        let (mut hits, mut past) = (0, 0);
        for _ in 0..600 {
            let r = radii[(rng.next_u32() % 4) as usize];
            let a = loop {
                let (th, rho) = (rng.signed() * PI, 55.0 + rng.next_f32() * 40.0);
                let a = Vec3::new(-900.0 + rng.next_f32() * 400.0, rho * cos(th), rho * sin(th));
                if shape.probe(a).dist - r > 0.5 {
                    break a;
                }
            };
            let dir = normalize_or(Vec3::X + unit(&mut rng) * 0.1, Vec3::X);
            let b = a + dir * (TRACE_MAX_LEN + rng.next_f32() * 900.0);
            let len = length(b - a);
            let long = sampled_long_contact(&shape, a, b, r, 6.1);
            match shape.trace_long(a, b, r) {
                Some(f) => {
                    hits += 1;
                    let gap = shape.probe(a + (b - a) * f).dist - r;
                    assert!(gap <= TRACE_EPS + 1e-3, "{a} → {b} r {r}: hit at {f} is {gap} m clear");
                    if let Some(s) = long {
                        assert!(f * len <= s + 6.06, "{a} → {b} r {r}: hit at {} m, touches at {s}", f * len);
                    }
                    past += usize::from(f * len > TRACE_MAX_LEN);
                }
                None => assert!(long.is_none(), "{a} → {b} r {r}: missed a contact at {long:?} m"),
            }
        }
        assert!(hits > 300 && past > 100, "{hits} hits, {past} past the first piece");
    }
    #[test]
    fn a_trace_that_starts_touching_goes_in_or_leaves() {
        // On Hermit's +X pole and on MO-II's +Y pylon, standing off by the sphere's radius.
        for (shape, at, n) in
            [(hermit(), Vec3::new(908.0, 0.0, 0.0), Vec3::X), (mo_ii(), Vec3::new(0.0, 94.0, 0.0), Vec3::Y)]
        {
            let side = n.cross(Vec3::Z);
            assert_eq!(shape.trace(at, at - n * 20.0, 8.0), Some(0.0), "going in");
            assert_eq!(shape.trace(at, at + n * 20.0, 8.0), None, "leaving");
            assert_eq!(shape.trace(at, at + side * 5.0, 8.0), None, "along the surface");
            assert_eq!(shape.trace(at - n * 30.0, at - n * 30.0, 0.0), Some(0.0), "a point inside");
            assert_eq!(shape.trace(at, at, 0.0), None, "a point outside");
        }
    }

    #[test]
    fn landmark_pose_is_periodic_bit_for_bit() {
        let mo = &LANDMARKS[0];
        let lcm = 432_000; // lcm(9_600, 54_000)
        assert_eq!((lcm % mo.spin_period, lcm % mo.orbit_period), (0, 0));
        for t in [0, 1, 777, 9_599, 53_999, 123_456] {
            assert_eq!(landmark_pose(mo, t, 0.0), landmark_pose(mo, t + lcm, 0.0));
            assert_eq!(landmark_pose(mo, t, 0.25), landmark_pose(mo, t + 3 * lcm, 0.25));
        }
        // A tick's fraction runs on into the next tick, over the spin's and the orbit's wrap too.
        for t in [0, 4_321, 9_599, 53_999, 431_999] {
            let (a, b) = (landmark_pose(mo, t, 1.0), landmark_pose(mo, t + 1, 0.0));
            assert!(
                length(a.pos - b.pos) < 5e-3 && a.rot.dot(b.rot).abs() > 1.0 - 1e-6,
                "tick {t}: {a:?} vs {b:?}"
            );
        }
        for d in &LANDMARKS {
            assert_eq!(landmark_pose(d, 0, 0.0).rot, d.rot0, "{}", d.name);
        }
        assert!(!landmark_pose(&LANDMARKS[1], 999, 0.5).moving && landmark_pose(mo, 0, 0.0).moving);
    }

    #[test]
    fn point_velocity_matches_the_motion() {
        // Centred on the origin, so the positions' rounding doesn't swamp a tick's motion.
        let d = LandmarkDef { center: Vec3::ZERO, ..LANDMARKS[0] };
        let points = [Vec3::new(-260.0, 74.0, 74.0), Vec3::new(334.0, 0.0, 0.0), Vec3::new(0.0, 86.0, 20.0)];
        for t in [1, 2_000, 9_599, 27_000, 53_999] {
            let pose = landmark_pose(&d, t, 0.0);
            for l in points {
                let moved = (landmark_pose(&d, t + 1, 0.0).to_world(l)
                    - landmark_pose(&d, t - 1, 0.0).to_world(l))
                    / (2.0 * DT);
                let v = pose.point_vel(pose.to_world(l));
                assert!(length(v - moved) < 2e-3, "tick {t}, {l}: {v} vs {moved}");
            }
        }
    }

    #[test]
    fn the_normal_from_a_point_is_the_nearest_surfaces() {
        // Off an ellipsoid, the nearest point of its surface is where the line down the normal
        // from `p` meets it square: the surface's own normal there is the line's direction. The
        // first-order gradient at `p` leans off that by degrees.
        let mut rng = Rng::new(21);
        let square = |a: Vec3, p: Vec3, n: Vec3| {
            // Where the line p - n·t first meets the ellipsoid (the nearer root), and the angle
            // between the surface's normal there and n.
            let (o, d) = (p / a, -n / a);
            let (qa, qb, qc) = (d.dot(d), 2.0 * o.dot(d), o.dot(o) - 1.0);
            let t = (-qb - sqrt(qb * qb - 4.0 * qa * qc)) / (2.0 * qa);
            let x = p - n * t;
            angle_between(normalize_or(x / (a * a), Vec3::Y), n)
        };
        let (mut worst, mut worst_iq): (f32, f32) = (0.0, 0.0);
        for _ in 0..2_000 {
            let min = 10.0 + rng.next_f32() * 20.0;
            let a = Vec3::new(min, min * (1.0 + 2.05 * rng.next_f32()), min * (1.0 + rng.next_f32()));
            let shape = Shape::ellipsoid(a);
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let p = dir / length(dir / a) + dir * (6.0 + 4.0 * rng.next_f32());
            worst = worst.max(square(a, p, shape.normal_from(p)));
            worst_iq = worst_iq.max(square(a, p, shape.probe(p).normal));
        }
        assert!(worst < 1e-3, "{worst} rad off square to the surface");
        assert!(worst_iq > 0.05, "the gradient was only {worst_iq} rad off: the test tests nothing");
        // A union's and a cut's are the probe's own.
        let p = Vec3::new(-100.0, 70.0, 3.0);
        assert_eq!(mo_ii().normal_from(p), mo_ii().probe(p).normal);
    }

    #[test]
    fn static_point_velocity_is_positive_zero() {
        let rock =
            Rock { pos: Vec3::new(10.0, -3.0, 7.0), rot: quat_axis_angle(Vec3::Y, 1.0), ..Rock::default() };
        let hermit = landmark_pose(&LANDMARKS[1], 12_345, 0.5);
        for pose in [BodyPose::fixed(rock.pos, rock.rot), hermit] {
            for w in [Vec3::ZERO, Vec3::new(-5.0, 1e4, 3.0), Vec3::splat(-0.0)] {
                assert_eq!(pose.point_vel(w).to_array().map(f32::to_bits), [0; 3]);
            }
        }
        assert_eq!(hermit.vel.to_array().map(f32::to_bits), [0; 3]);
        assert_eq!(hermit.ang_vel.to_array().map(f32::to_bits), [0; 3]);
    }

    fn two_rocks() -> Field {
        let big = Rock {
            pos: Vec3::new(100.0, 0.0, 0.0),
            radius: 30.0,
            axes: Vec3::new(30.0, 20.0, 20.0),
            ..Rock::default()
        };
        let twin = Rock { pos: Vec3::new(-100.0, 0.0, 0.0), ..big };
        let small =
            Rock { pos: Vec3::new(0.0, 0.0, 60.0), radius: 9.0, axes: Vec3::splat(9.0), ..Rock::default() };
        Field::from_rocks(&[big, twin, small])
    }

    #[test]
    fn bodies_know_rocks_and_landmarks() {
        let mut field = two_rocks();
        let b = Bodies::at(&field, &LANDMARKS, 0);
        let rock = field.rocks()[0];
        assert_eq!(b.pose(Body::Rock(0)), Some(BodyPose::fixed(rock.pos, rock.rot)));
        assert!(b.grippable(Body::Rock(0)) && b.alive(Body::Rock(2)) && !b.grippable(Body::Rock(2)));
        assert!(
            b.pose(Body::Rock(3)).is_none() && !b.alive(Body::Rock(3)) && b.shape(Body::Rock(3)).is_none()
        );
        assert!(b.pose(Body::None).is_none() && !b.alive(Body::None) && b.shape(Body::None).is_none());
        assert_eq!(b.pose(Body::Landmark(0)), Some(landmark_pose(&LANDMARKS[0], 0, 0.0)));
        assert_eq!(b.pose_at(Body::Landmark(0), 77, 0.5), Some(landmark_pose(&LANDMARKS[0], 77, 0.5)));
        assert!(b.grippable(Body::Landmark(0)) && b.grippable(Body::Landmark(1)));
        assert!(b.pose(Body::Landmark(2)).is_none() && !b.alive(Body::Landmark(2)));
        // A shattered rock is gone, but still posed (the seed says where it was).
        field.set_dead(0, true);
        let b = Bodies::at(&field, &LANDMARKS, 0);
        assert!(!b.alive(Body::Rock(0)) && !b.grippable(Body::Rock(0)));
        assert_eq!(b.pose(Body::Rock(0)), Some(BodyPose::fixed(rock.pos, rock.rot)));
        // A sector without landmarks has none.
        let bare = Bodies::at(&field, &LANDMARKS[..0], 0);
        assert!(!bare.alive(Body::Landmark(0)) && bare.pose(Body::Landmark(0)).is_none());
        assert!(Body::Rock(1_022).code() < Body::Landmark(0).code());
    }

    #[test]
    fn nearest_grippable_is_the_nearest_and_ties_go_to_the_lower_body() {
        let field = two_rocks();
        let b = Bodies::at(&field, &LANDMARKS, 0);
        // Midway between the twins (the small rock is nearer, but too small to grip).
        let mut f = FlightState::default();
        let near = b.nearest_grippable(&f, 100.0, 10.0, 2.0).unwrap();
        assert_eq!(near.body, Body::Rock(0));
        assert!((near.h - (70.0 - STANCE)).abs() < 1e-3, "h {}", near.h);
        assert_eq!((near.n_world, near.v_rel), (-Vec3::X, Vec3::ZERO));
        assert!(b.nearest_grippable(&f, 50.0, 10.0, 2.0).is_none(), "out of range");
        f.vel = Vec3::Z * 20.0;
        assert!(b.nearest_grippable(&f, 100.0, 10.0, 2.0).is_none(), "too fast");
        // Drifting toward the second twin and away from the first.
        f.vel = -Vec3::X * 5.0;
        assert_eq!(b.nearest_grippable(&f, 100.0, 10.0, 2.0).unwrap().body, Body::Rock(1));
        // Over MO-II's +Y pylon, moving with it.
        let t = 1_234;
        let b = Bodies::at(&field, &LANDMARKS, t);
        let pose = b.pose(Body::Landmark(0)).unwrap();
        f.pos = pose.to_world(Vec3::new(0.0, 86.0 + STANCE + 20.0, 0.0));
        f.vel = pose.point_vel(f.pos) + pose.rot * Vec3::Y * 1.0;
        let near = b.nearest_grippable(&f, 100.0, 10.0, 2.0).unwrap();
        assert_eq!(near.body, Body::Landmark(0));
        assert!((near.h - 20.0).abs() < 1e-2 && length(near.v_rel - pose.rot * Vec3::Y) < 1e-3, "{near:?}");
        assert!(length(near.n_world - pose.rot * Vec3::Y) < 1e-5);
        assert!(b.nearest_grippable(&f, 100.0, 10.0, 0.5).is_none(), "leaving too fast");
    }

    #[test]
    fn landmarks_stop_suits_slide_them_and_carry_the_surface() {
        let field = Field::empty();
        let b = Bodies::at(&field, &LANDMARKS, 500);
        let big = b.pose(Body::Landmark(1)).unwrap();
        // Straight at Hermit's +X pole at 600 m/s: stopped on it.
        let mut s = FlightState {
            pos: big.to_world(Vec3::new(880.0, 0.0, 0.0)),
            vel: big.rot * -Vec3::X * 600.0,
            ..FlightState::default()
        };
        let prev = big.to_world(Vec3::new(1_000.0, 0.0, 0.0));
        assert!(b.collide_landmarks(prev, &mut s, None));
        let at = big.to_local(s.pos);
        assert!((at.x - 908.0).abs() < 0.05 && at.y.abs() < 0.05, "stopped at {at}");
        assert!(s.vel.dot(big.rot * Vec3::X) > -1e-3, "still moving in: {}", s.vel);
        // Sliding along it, it keeps going.
        let prev = s.pos;
        s.pos = big.to_world(Vec3::new(907.0, 0.0, 30.0));
        assert!(b.collide_landmarks(prev, &mut s, None));
        let at = big.to_local(s.pos);
        assert!((hermit().probe(at).dist - SUIT_CLEARANCE).abs() < 0.05 && at.z > 29.0, "slid to {at}");
        // Unless it rides it.
        let mut riding = FlightState { pos: big.to_world(Vec3::new(880.0, 0.0, 0.0)), ..s };
        assert!(!b.collide_landmarks(prev, &mut riding, Some(1)));
        // Landing on MO-II's turning pylon at 10 m/s relative: the relative speed in is lost,
        // what it has along the surface is kept.
        let mo = b.pose(Body::Landmark(0)).unwrap();
        let prev = mo.to_world(Vec3::new(0.0, 100.0, 0.0));
        let mut s = FlightState { pos: mo.to_world(Vec3::new(0.0, 90.0, 0.0)), ..FlightState::default() };
        s.vel = mo.point_vel(s.pos) + mo.rot * Vec3::new(3.0, -10.0, 0.0);
        assert!(b.collide_landmarks(prev, &mut s, None));
        let at = mo.to_local(s.pos);
        assert!((at.y - 94.0).abs() < 0.05, "stopped at {at}");
        let rel = mo.rot.conjugate() * (s.vel - mo.point_vel(s.pos));
        assert!(rel.y.abs() < 1e-3 && (rel.x - 3.0).abs() < 1e-2, "relative velocity {rel}");
        // Far from both, nothing happens.
        let mut away = FlightState { pos: Vec3::new(0.0, 4_000.0, 0.0), ..FlightState::default() };
        assert!(!b.collide_landmarks(Vec3::new(0.0, 4_010.0, 0.0), &mut away, None));
        assert_eq!(away.pos, Vec3::new(0.0, 4_000.0, 0.0));
    }

    #[test]
    fn shots_meet_landmarks_where_they_were() {
        let field = Field::empty();
        let b = Bodies::at(&field, &LANDMARKS, 500);
        // Across MO-II's middle, along its local −Z: in through the −Z... +Z pylon's face (z = 86).
        for (t, frac) in [(500, 0.0), (500 + 27_000, 0.5), (3, 0.25)] {
            let mo = landmark_pose(&LANDMARKS[0], t, frac);
            let (a, c) = (mo.to_world(Vec3::new(0.0, 0.0, 300.0)), mo.to_world(Vec3::new(0.0, 0.0, -300.0)));
            let (f, k) = b.sweep_landmarks(a, c, 0.5, t, frac).unwrap();
            assert_eq!(k, 0);
            assert!(
                (f * 600.0 - (300.0 - 86.5)).abs() <= TRACE_MIN_STEP,
                "tick {t}+{frac}: hit {} m in",
                f * 600.0
            );
            assert_eq!(
                Some((f, 0)),
                LANDMARKS[0].shape.trace_long(mo.to_local(a), mo.to_local(c), 0.5).map(|f| (f, 0))
            );
        }
        // A kilometre off, nothing; and nothing where there are no landmarks.
        let mo = landmark_pose(&LANDMARKS[0], 500, 0.0);
        let (a, c) =
            (mo.to_world(Vec3::new(0.0, 1_000.0, 300.0)), mo.to_world(Vec3::new(0.0, 1_000.0, -300.0)));
        assert!(b.sweep_landmarks(a, c, 0.5, 500, 0.0).is_none());
        let (a, c) = (mo.to_world(Vec3::new(0.0, 0.0, 300.0)), mo.to_world(Vec3::new(0.0, 0.0, -300.0)));
        assert!(Bodies::at(&field, &LANDMARKS[..0], 500).sweep_landmarks(a, c, 0.5, 500, 0.0).is_none());
    }

    #[test]
    fn surface_along_finds_the_outer_surface() {
        let field = two_rocks();
        let b = Bodies::at(&field, &LANDMARKS, 0);
        let expect = [
            (Body::Landmark(0), Vec3::Y, Vec3::new(0.0, 86.0, 0.0), Vec3::Y),
            (Body::Landmark(0), -Vec3::X, Vec3::new(-235.0, 0.0, 0.0), -Vec3::X),
            (Body::Landmark(0), Vec3::X, Vec3::new(334.0, 0.0, 0.0), Vec3::X),
            (Body::Landmark(1), Vec3::X, Vec3::new(900.0, 0.0, 0.0), Vec3::X),
            (Body::Landmark(1), Vec3::Y, Vec3::new(0.0, 590.0, 0.0), Vec3::Y),
            (Body::Rock(0), Vec3::Z, Vec3::new(0.0, 0.0, 20.0), Vec3::Z),
        ];
        for (body, dir, at, n) in expect {
            let (p, pn) = b.surface_along(body, dir).unwrap();
            assert!(length(p - at) < 1e-2 && length(pn - n) < 1e-3, "{body:?} along {dir}: {p}, {pn}");
        }
        assert!(
            b.surface_along(Body::None, Vec3::Y).is_none()
                && b.surface_along(Body::Landmark(3), Vec3::Y).is_none()
        );
        // Any way out, on the surface, with nothing further out.
        let mut rng = Rng::new(5);
        for body in [Body::Landmark(0), Body::Landmark(1), Body::Rock(0)] {
            let shape = b.shape(body).unwrap();
            for _ in 0..300 {
                let dir = unit(&mut rng);
                let (p, n) = b.surface_along(body, dir).unwrap();
                assert!(
                    shape.probe(p).dist.abs() < 1e-2 && (length(n) - 1.0).abs() < 1e-5,
                    "{body:?} along {dir}: {p}"
                );
                assert!(angle_between(normalize_or(p, dir), dir) < 1e-3, "{body:?}: {p} isn't along {dir}");
                let mut s = length(p) + 0.1;
                while s < 1_000.0 {
                    assert!(!solid(&shape, dir * s), "{body:?} along {dir}: solid at {s} m, past {p}");
                    s += 0.5;
                }
            }
        }
    }

    #[test]
    fn hide_spots_are_where_a_suit_is() {
        let field = two_rocks();
        let b = Bodies::at(&field, &LANDMARKS, 0);
        let spot = |body, x, y, z| b.hide_spot_of(body, Vec3::new(x, y, z));
        // Standing on the floors.
        assert_eq!(spot(Body::Landmark(0), -235.0 - STANCE, 0.0, 0.0), Some(0));
        assert_eq!(spot(Body::Landmark(1), 0.0, 590.0 + STANCE, 0.0), Some(0));
        assert_eq!(spot(Body::Landmark(1), 0.0, 0.0, -730.0 - STANCE), Some(1));
        assert_eq!(spot(Body::Landmark(1), -870.0 - STANCE, 0.0, 30.0), Some(2));
        // Elsewhere, and on bodies without any.
        assert_eq!(spot(Body::Landmark(0), 0.0, 86.0 + STANCE, 0.0), None);
        assert_eq!(spot(Body::Landmark(1), 0.0, 590.0 + STANCE, 50.0), None);
        assert_eq!(spot(Body::Rock(0), 0.0, 0.0, 0.0), None);
        assert_eq!(spot(Body::Landmark(4), -235.0, 0.0, 0.0), None);
        assert_eq!(spot(Body::None, -235.0, 0.0, 0.0), None);
    }
}
