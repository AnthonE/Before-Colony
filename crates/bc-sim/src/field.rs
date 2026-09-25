//! The debris field around the colony: rocks (and the ore in them), generated from a seed so that
//! the server and every client build the identical field. Storage is allocated once, here.
//!
//! Rocks are solid: a suit's flight is swept against them ([`Field::collide`]), and shots stop at
//! them ([`Field::sweep`]). Each rock's collider is its ellipsoid (the client's mesh fits inside
//! it), grown by the suit's or shot's radius. A static hashed grid finds the rocks near anything.

use alloc::boxed::Box;

use glam::{Quat, Vec3};

use crate::flight::FlightState;
use crate::math::{Rng, floor, length, normalize_or, quat_axis_angle, sqrt};
use crate::storage::{BitSet, boxed};
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};

/// Ore kinds: nickel-iron (common), titanium, volatiles (ices), and exotic metals, the feedstock of
/// zero-G alloys (rare).
pub const ORE_KINDS: u8 = 4;
/// Mesh variants the client draws rocks with.
pub const SHAPES: u8 = 12;
/// How close a suit's centre comes to a rock's surface, m.
pub const SUIT_CLEARANCE: f32 = 8.0;

/// Centre of the field (the combat zone above the colony).
pub const FIELD_CENTER: Vec3 = Vec3::new(0.0, 900.0, 0.0);
/// Keep-out zones: the faction spawn bases (see `sim::spawn_point`), and room around the colony.
const SPAWN_BASES: [Vec3; 3] = [
    Vec3::new(-4_200.0, 700.0, -3_200.0),
    Vec3::new(4_200.0, 700.0, -3_200.0),
    Vec3::new(0.0, 2_200.0, 4_200.0),
];
const SPAWN_CLEARANCE: f32 = 500.0;
const COLONY_CLEARANCE: f32 = 150.0;

/// Broad-phase grid: cell edge (m) and hash buckets.
const CELL: f32 = 256.0;
const BUCKETS: usize = 2_048;

/// One rock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rock {
    pub pos: Vec3,
    /// Bounding radius: the largest half-axis.
    pub radius: f32,
    /// Half-axes of the ellipsoid the rock fills (its collider; the largest equals `radius`).
    pub axes: Vec3,
    pub rot: Quat,
    /// Mesh variant, `0..SHAPES`.
    pub shape: u8,
    /// Ore kind, `0..ORE_KINDS`.
    pub ore: u8,
}

impl Default for Rock {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, radius: 1.0, axes: Vec3::ONE, rot: Quat::IDENTITY, shape: 0, ore: 0 }
    }
}

impl Rock {
    /// `p` in the rock's frame, scaled so that its surface grown by `r` is the unit sphere.
    fn unit(&self, p: Vec3, r: f32) -> Vec3 {
        (self.rot.conjugate() * (p - self.pos)) / (self.axes + Vec3::splat(r))
    }

    /// How far along `a→b` (0..1) a sphere of radius `r` first touches the rock: 0 if it starts
    /// touching and moves further in (a hair's breadth counts, so a suit resting on a rock slides
    /// instead of sticking); None if it never touches, or only leaves (a shot fired away from a
    /// rock its muzzle is against goes on its way).
    pub fn sweep(&self, a: Vec3, b: Vec3, r: f32) -> Option<f32> {
        let p = self.unit(a, r);
        let d = self.unit(b, r) - p;
        let half_b = p.dot(d);
        if half_b >= 0.0 {
            return None; // not closing on it
        }
        let c = p.dot(p) - 1.0;
        if c <= 1e-3 {
            return Some(0.0);
        }
        let dd = d.dot(d);
        let disc = half_b * half_b - dd * c;
        if disc < 0.0 {
            return None;
        }
        let t = (-half_b - sqrt(disc)) / dd;
        (t <= 1.0).then_some(t)
    }

    /// Whether a sphere of radius `r` at `p` overlaps the rock.
    pub fn touches(&self, p: Vec3, r: f32) -> bool {
        let u = self.unit(p, r);
        u.dot(u) < 1.0
    }

    /// The point on the rock's surface (grown by `r`) straight out from `p`.
    pub fn surface(&self, p: Vec3, r: f32) -> Vec3 {
        let u = normalize_or(self.unit(p, r), Vec3::Y);
        self.pos + self.rot * (u * (self.axes + Vec3::splat(r)))
    }

    /// The outward normal of the rock's surface (grown by `r`) near `p`.
    pub fn normal(&self, p: Vec3, r: f32) -> Vec3 {
        let s = self.axes + Vec3::splat(r);
        normalize_or(self.rot * (self.unit(p, r) / s), Vec3::Y)
    }
}

#[inline]
fn cell_of(p: Vec3) -> (i32, i32, i32) {
    let inv = 1.0 / CELL;
    (floor(p.x * inv) as i32, floor(p.y * inv) as i32, floor(p.z * inv) as i32)
}

#[inline]
fn bucket(x: i32, y: i32, z: i32) -> usize {
    let h = (x as u32).wrapping_mul(73_856_093)
        ^ (y as u32).wrapping_mul(19_349_663)
        ^ (z as u32).wrapping_mul(83_492_791);
    h as usize & (BUCKETS - 1)
}

/// The field: `len()` rocks, and a grid listing each in every cell its bounds touch.
#[derive(Clone)]
pub struct Field {
    rocks: Box<[Rock]>,
    n: usize,
    bucket_start: Box<[u32]>,
    items: Box<[u16]>,
    /// Rocks shattered (by mining): nothing meets them until they grow back.
    dead: BitSet,
}

impl core::fmt::Debug for Field {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Field").field("rocks", &self.n).finish()
    }
}

impl Field {
    /// The L1 debris field.
    pub const DEFAULT_SEED: u32 = 0xDEB12;
    pub const DEFAULT_ROCKS: u16 = 160;
    /// The most rocks a field holds (their ids fit the wire's 10 bits).
    pub const MAX_ROCKS: u16 = 1_023;

    /// No rocks at all.
    pub fn empty() -> Self {
        Self::generate(0, 0)
    }

    /// Generates up to `count` rocks from `seed`. Candidates inside a keep-out zone are rejected
    /// (at most `10 × count` are drawn), so the field may hold slightly fewer.
    pub fn generate(seed: u32, count: u16) -> Self {
        let count = count.min(Self::MAX_ROCKS);
        let mut rocks = boxed(count as usize, Rock::default());
        let mut rng = Rng::new(u64::from(seed));
        let mut n = 0;
        for _ in 0..count as usize * 10 {
            if n == count as usize {
                break;
            }
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed() * 0.5, rng.signed()), Vec3::X);
            let pos = FIELD_CENTER + dir * (1_200.0 + rng.next_f32() * 6_000.0);
            let s = 4.0 + rng.next_f32() * rng.next_f32() * 60.0;
            let axes = Vec3::new(s * (0.6 + rng.next_f32()), s * (0.5 + rng.next_f32() * 0.6), s);
            let radius = axes.max_element();
            let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let rot = quat_axis_angle(axis, rng.next_f32() * core::f32::consts::TAU);
            let shape = (rng.next_u32() % u32::from(SHAPES)) as u8;
            let ore = match rng.next_f32() {
                x if x < 0.55 => 0,
                x if x < 0.8 => 1,
                x if x < 0.95 => 2,
                _ => 3,
            };
            if Self::keep_out(pos, radius) {
                continue;
            }
            rocks[n] = Rock { pos, radius, axes, rot, shape, ore };
            n += 1;
        }
        Self::index(rocks, n)
    }

    /// A field of exactly these rocks (at most `MAX_ROCKS` of them).
    pub fn from_rocks(given: &[Rock]) -> Self {
        let n = given.len().min(Self::MAX_ROCKS as usize);
        let mut rocks = boxed(n, Rock::default());
        rocks.copy_from_slice(&given[..n]);
        Self::index(rocks, n)
    }

    /// Builds the grid over the first `n` rocks.
    fn index(rocks: Box<[Rock]>, n: usize) -> Self {
        // In three passes: count each bucket's entries, place the starts, fill.
        let cells =
            |r: &Rock| (cell_of(r.pos - Vec3::splat(r.radius)), cell_of(r.pos + Vec3::splat(r.radius)));
        let mut bucket_start = boxed(BUCKETS + 1, 0u32);
        for r in &rocks[..n] {
            let (lo, hi) = cells(r);
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        bucket_start[bucket(x, y, z) + 1] += 1;
                    }
                }
            }
        }
        for b in 0..BUCKETS {
            bucket_start[b + 1] += bucket_start[b];
        }
        let mut fill = boxed(BUCKETS, 0u32);
        fill.copy_from_slice(&bucket_start[..BUCKETS]);
        let mut items = boxed(bucket_start[BUCKETS] as usize, 0u16);
        for (i, r) in rocks[..n].iter().enumerate() {
            let (lo, hi) = cells(r);
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        let b = bucket(x, y, z);
                        items[fill[b] as usize] = i as u16;
                        fill[b] += 1;
                    }
                }
            }
        }
        Self { rocks, n, bucket_start, items, dead: BitSet::new(n.max(1)) }
    }

    /// Marks rock `i` shattered (or grown back).
    pub fn set_dead(&mut self, i: usize, dead: bool) {
        if i < self.n {
            self.dead.set(i, dead);
        }
    }

    pub fn is_dead(&self, i: usize) -> bool {
        i < self.n && self.dead.get(i)
    }

    /// Whether a rock of `radius` at `pos` would sit inside a keep-out zone.
    fn keep_out(pos: Vec3, radius: f32) -> bool {
        let rel = pos - COLONY_CENTER;
        let radial = sqrt(rel.y * rel.y + rel.z * rel.z);
        let colony = rel.x.abs() <= COLONY_HALF_LENGTH + COLONY_CLEARANCE + radius
            && radial <= COLONY_RADIUS + COLONY_CLEARANCE + radius;
        colony || SPAWN_BASES.iter().any(|b| length(*b - pos) < SPAWN_CLEARANCE + radius)
    }

    pub fn rocks(&self) -> &[Rock] {
        &self.rocks[..self.n]
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Calls `f` once for each rock whose bounds overlap the box `[min, max]`.
    pub fn for_each_in_box(&self, min: Vec3, max: Vec3, mut f: impl FnMut(usize)) {
        if self.n == 0 {
            return;
        }
        let (lo, hi) = (cell_of(min), cell_of(max));
        let cells = (hi.0 - lo.0 + 1) as i64 * (hi.1 - lo.1 + 1) as i64 * (hi.2 - lo.2 + 1) as i64;
        let overlaps = |r: &Rock| {
            let e = Vec3::splat(r.radius);
            (r.pos - e).cmple(max).all() && (r.pos + e).cmpge(min).all()
        };
        if cells > BUCKETS as i64 {
            // Bigger than the grid is worth: check every rock once.
            for (i, r) in self.rocks().iter().enumerate() {
                if overlaps(r) {
                    f(i);
                }
            }
            return;
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    let b = bucket(x, y, z);
                    for k in self.bucket_start[b]..self.bucket_start[b + 1] {
                        let i = self.items[k as usize] as usize;
                        let r = &self.rocks[i];
                        // Each rock once: from the cell holding the low corner of where its bounds
                        // and the box meet (unrelated rocks that share the bucket fail the overlap).
                        if overlaps(r) && cell_of((r.pos - Vec3::splat(r.radius)).max(min)) == (x, y, z) {
                            f(i);
                        }
                    }
                }
            }
        }
    }

    /// The first rock a sphere of radius `r` meets moving from `a` to `b`: how far along (0..1),
    /// and which.
    pub fn sweep(&self, a: Vec3, b: Vec3, r: f32) -> Option<(f32, usize)> {
        let pad = Vec3::splat(r);
        let mut best: Option<(f32, usize)> = None;
        self.for_each_in_box(a.min(b) - pad, a.max(b) + pad, |i| {
            if self.dead.get(i) {
                return;
            }
            if let Some(t) = self.rocks[i].sweep(a, b, r)
                && best.is_none_or(|(bt, _)| t < bt)
            {
                best = Some((t, i));
            }
        });
        best
    }

    /// Keeps a suit out of the rocks. Its move this tick (from `prev`) is swept: meeting a rock
    /// stops it there; ending inside one it was already touching, it slides along the surface. The
    /// first contact wins (a rock the suit is leaving is none). Either way the speed into the rock
    /// is lost (inelastic, like the colony's hull).
    pub fn collide(&self, prev: Vec3, s: &mut FlightState) {
        let r = SUIT_CLEARANCE;
        let pad = Vec3::splat(r);
        let end = s.pos;
        let mut first: Option<(f32, usize)> = None;
        self.for_each_in_box(prev.min(end) - pad, prev.max(end) + pad, |i| {
            if self.dead.get(i) {
                return;
            }
            let rock = &self.rocks[i];
            let t = match rock.sweep(prev, end, r) {
                Some(t) if t > 0.0 => t,
                _ if rock.touches(end, r) => 0.0,
                _ => return,
            };
            if first.is_none_or(|(ft, _)| t < ft) {
                first = Some((t, i));
            }
        });
        let Some((t, i)) = first else { return };
        let rock = &self.rocks[i];
        let at = if t > 0.0 { prev + (end - prev) * t } else { rock.surface(end, r) };
        let n = rock.normal(at, r);
        s.pos = at;
        let vn = s.vel.dot(n);
        if vn < 0.0 {
            s.vel -= n * vn;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_clear_of_keep_out_zones() {
        let a = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        let b = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        assert_eq!(a.rocks(), b.rocks());
        assert!(a.len() > 140, "only {} rocks", a.len());
        for r in a.rocks() {
            assert!(!Field::keep_out(r.pos, r.radius));
            assert_eq!(r.radius, r.axes.max_element());
            assert!(r.shape < SHAPES && r.ore < ORE_KINDS);
            assert!((r.rot.length() - 1.0).abs() < 1e-4);
        }
        let other = Field::generate(1, Field::DEFAULT_ROCKS);
        assert_ne!(a.rocks()[0], other.rocks()[0]);
    }

    #[test]
    fn empty_field() {
        assert!(Field::generate(7, 0).is_empty());
        assert!(Field::empty().sweep(Vec3::ZERO, Vec3::X * 1e4, 1.0).is_none());
    }

    #[test]
    fn the_grid_finds_what_brute_force_finds() {
        let f = Field::generate(Field::DEFAULT_SEED, 600);
        let mut rng = Rng::new(9);
        for _ in 0..400 {
            let c = FIELD_CENTER + Vec3::new(rng.signed(), rng.signed() * 0.5, rng.signed()) * 7_000.0;
            let e = Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32()) * 900.0;
            let (min, max) = (c - e, c + e);
            let mut seen = [0u8; 1_024];
            f.for_each_in_box(min, max, |i| seen[i] += 1);
            for (i, r) in f.rocks().iter().enumerate() {
                let rr = Vec3::splat(r.radius);
                let want = u8::from((r.pos - rr).cmple(max).all() && (r.pos + rr).cmpge(min).all());
                assert_eq!(seen[i], want, "rock {i}");
            }
        }
    }

    #[test]
    fn sweeps_meet_the_ellipsoid() {
        let rock =
            Rock { pos: Vec3::ZERO, radius: 20.0, axes: Vec3::new(20.0, 10.0, 5.0), ..Rock::default() };
        // Along x, the surface is at 20 (+1 for the shot's radius).
        let t = rock.sweep(Vec3::new(-100.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), 1.0).unwrap();
        assert!((t * 100.0 - 79.0).abs() < 1e-3);
        // Along z it is only 5 deep: a line at z = 7 misses; at z = 5 it meets it.
        assert!(rock.sweep(Vec3::new(-100.0, 0.0, 7.0), Vec3::new(100.0, 0.0, 7.0), 1.0).is_none());
        assert!(rock.sweep(Vec3::new(-100.0, 0.0, 5.0), Vec3::new(100.0, 0.0, 5.0), 1.0).is_some());
        // Starting inside and going through: at once. Leaving, from inside or from the surface: no.
        assert_eq!(rock.sweep(Vec3::X * -15.0, Vec3::X * 50.0, 1.0), Some(0.0));
        assert!(rock.sweep(Vec3::X * 15.0, Vec3::X * 60.0, 1.0).is_none());
        assert!(rock.sweep(Vec3::X * 21.0, Vec3::X * 60.0, 1.0).is_none());
        assert!(rock.sweep(Vec3::X * 30.0, Vec3::X * 60.0, 1.0).is_none());
    }

    #[test]
    fn leaving_one_rock_for_another_stops_at_the_second() {
        // A suit on a big rock's surface heads off at 2 km/s straight through a small one nearby.
        let a = Rock { pos: Vec3::ZERO, radius: 20.0, axes: Vec3::splat(20.0), ..Rock::default() };
        let b = Rock { pos: Vec3::X * 60.0, radius: 10.0, axes: Vec3::splat(10.0), ..Rock::default() };
        let f = Field::from_rocks(&[a, b]);
        let start = Vec3::X * (20.0 + SUIT_CLEARANCE);
        let mut s = FlightState { pos: start, vel: Vec3::X * 2_000.0, ..FlightState::default() };
        s.pos += s.vel * crate::config::DT;
        f.collide(start, &mut s);
        assert!((s.pos.x - (60.0 - 10.0 - SUIT_CLEARANCE)).abs() < 1e-3, "went on to {:?}", s.pos);
        assert!(s.vel.x.abs() < 1e-3);
        // Resting inside one rock's shell (after a push into an overlapping neighbour), it's put out.
        let mut s = FlightState { pos: Vec3::X * 5.0, ..FlightState::default() };
        f.collide(s.pos, &mut s);
        assert!(!a.touches(s.pos, SUIT_CLEARANCE - 1e-3), "left inside at {:?}", s.pos);
    }

    #[test]
    fn suits_stop_at_rocks_and_slide_along_them() {
        let f = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        let rock = f.rocks()[0];
        // Straight at it at 2 km/s: one tick carries the suit 67 m, well past the surface.
        let dir = normalize_or(rock.pos - FIELD_CENTER, Vec3::X);
        let start = rock.pos - dir * (rock.radius + SUIT_CLEARANCE + 30.0);
        let mut s = FlightState { pos: start, vel: dir * 2_000.0, ..FlightState::default() };
        let prev = s.pos;
        s.pos += s.vel * crate::config::DT;
        f.collide(prev, &mut s);
        assert!(!rock.touches(s.pos + (s.pos - rock.pos) * 1e-4, SUIT_CLEARANCE), "tunnelled into the rock");
        assert!(s.vel.dot(rock.normal(s.pos, SUIT_CLEARANCE)) >= -1e-3, "still moving into it");
        // Sideways along the surface, it keeps going.
        let n = rock.normal(s.pos, SUIT_CLEARANCE);
        let side = normalize_or(n.cross(Vec3::Y), Vec3::X);
        s.vel = side * 100.0 - n * 5.0;
        let before = s.pos;
        for _ in 0..10 {
            let prev = s.pos;
            s.pos += s.vel * crate::config::DT;
            f.collide(prev, &mut s);
        }
        assert!(length(s.pos - before) > 20.0, "stuck to the rock");
    }
}
