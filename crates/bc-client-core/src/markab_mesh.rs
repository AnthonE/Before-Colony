//! The Markab as meshes (`markab`): a low wedge of a two-seater, faceted, in its driver's colour,
//! under a glass canopy, on four wheels in its fenders, with the fan's skirt lit round its floor and
//! its ten jets' nozzles (four under its floor, four in its tail, two in its nose). In its own frame:
//! `x` to its left, `y` up, `z` ahead, from its middle (its wheels stand `markab::RIDE` under it).
//! Every part fits inside its body's box, as its tests hold.

use glam::Vec3;

use crate::markab::{LENGTH, RIDE, TRACK, WHEELBASE};

/// What a part is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    /// The driver's colour.
    Body,
    Glass,
    Tyre,
    /// The dark trim: the skirt, the nozzles, the grille.
    Trim,
    /// The fan's skirt light, lit while the fan runs.
    Skirt,
    /// The headlamps and the tail lamps.
    Lamp,
    Tail,
}

/// Triangles, flat-shaded: each its own three vertices.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl Shape {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let base = self.positions.len() as u32;
        for p in [a, b, c] {
            self.positions.push(p.to_array());
            self.normals.push(n.to_array());
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// A quad `a b c d` going round counter-clockwise seen from outside.
    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3) {
        self.tri(a, b, c);
        self.tri(a, c, d);
    }

    /// The quad `q`, turned to face away from `inside`.
    fn quad_from(&mut self, inside: Vec3, q: [Vec3; 4]) {
        let n = (q[1] - q[0]).cross(q[2] - q[0]);
        let c = (q[0] + q[1] + q[2] + q[3]) * 0.25;
        if n.dot(c - inside) >= 0.0 {
            self.quad(q[0], q[1], q[2], q[3])
        } else {
            self.quad(q[3], q[2], q[1], q[0])
        }
    }

    /// Rings of points round a line through their middles, joined ring to ring (round the whole way
    /// if `closed`), and the ends closed if `caps`: every face turned out.
    fn loft(&mut self, rings: &[Vec<Vec3>], closed: bool, caps: bool) {
        let middle = |r: &[Vec3]| r.iter().copied().sum::<Vec3>() / r.len() as f32;
        let whole = rings.iter().map(|r| middle(r)).sum::<Vec3>() / rings.len() as f32;
        for w in rings.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let inside = (middle(a) + middle(b)) * 0.5;
            let n = a.len();
            let last = if closed { n } else { n - 1 };
            for i in 0..last {
                let j = (i + 1) % n;
                self.quad_from(inside, [a[i], a[j], b[j], b[i]]);
            }
        }
        if caps {
            for ring in [&rings[0], &rings[rings.len() - 1]] {
                let mid = middle(ring);
                for i in 0..ring.len() {
                    let j = (i + 1) % ring.len();
                    let n = (ring[i] - mid).cross(ring[j] - mid);
                    if n.dot(mid - whole) >= 0.0 {
                        self.tri(mid, ring[i], ring[j])
                    } else {
                        self.tri(mid, ring[j], ring[i])
                    }
                }
            }
        }
    }

    /// A box from `min` to `max`.
    fn cuboid(&mut self, min: Vec3, max: Vec3) {
        let c =
            |x: usize, y: usize, z: usize| Vec3::new([min.x, max.x][x], [min.y, max.y][y], [min.z, max.z][z]);
        self.quad(c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1));
        self.quad(c(1, 0, 0), c(0, 0, 0), c(0, 1, 0), c(1, 1, 0));
        self.quad(c(1, 0, 1), c(1, 0, 0), c(1, 1, 0), c(1, 1, 1));
        self.quad(c(0, 0, 0), c(0, 0, 1), c(0, 1, 1), c(0, 1, 0));
        self.quad(c(0, 1, 1), c(1, 1, 1), c(1, 1, 0), c(0, 1, 0));
        self.quad(c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1));
    }

    /// A cylinder of radius `r` round the axis from `a` to `b`, `sides` faceted, its ends closed.
    fn cylinder(&mut self, a: Vec3, b: Vec3, r: f32, sides: usize) {
        let axis = (b - a).normalize();
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);
        let ring = |c: Vec3| -> Vec<Vec3> {
            (0..sides)
                .map(|k| {
                    let t = k as f32 / sides as f32 * std::f32::consts::TAU;
                    c + (u * t.cos() + v * t.sin()) * r
                })
                .collect()
        };
        self.loft(&[ring(a), ring(b)], true, true);
    }

    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Up from the face its wheels are on to its own frame's `y`.
fn y(h: f32) -> f32 {
    h - RIDE
}

/// The body's sections, tail to nose: where along it (m), half its width, its floor and its top
/// (m up from the face its wheels are on).
const SECTIONS: [(f32, f32, f32, f32); 8] = [
    (-2.25, 0.9, 0.24, 0.84),
    (-1.95, 0.98, 0.16, 0.9),
    (-1.25, 1.0, 0.13, 0.88),
    (-0.5, 0.98, 0.13, 0.82),
    (0.4, 0.98, 0.13, 0.75),
    (1.2, 0.97, 0.13, 0.62),
    (1.85, 0.92, 0.15, 0.5),
    (2.25, 0.78, 0.19, 0.36),
];

/// The canopy's, tail to nose: where, half its width at its foot and its top.
const CANOPY: [(f32, f32, f32); 5] =
    [(-1.05, 0.6, 0.88), (-0.55, 0.7, 1.1), (0.1, 0.7, 1.12), (0.65, 0.64, 1.0), (1.05, 0.5, 0.7)];

/// The body's top at `z` along it, from its sections.
fn top_at(z: f32) -> f32 {
    let k = SECTIONS.iter().position(|s| s.0 >= z).unwrap_or(SECTIONS.len() - 1).max(1);
    let (a, b) = (SECTIONS[k - 1], SECTIONS[k]);
    let u = ((z - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    a.3 + (b.3 - a.3) * u
}

/// The ten jets' nozzles: where each is in its own frame, and the way it blows. The lift's four
/// under its floor, the tail's four, the nose's two (one either side).
pub fn jets() -> [(Vec3, Vec3); 10] {
    let (x, z) = (0.62, 1.55);
    let under = |sx: f32, sz: f32| (Vec3::new(sx * x, y(0.12), sz * z), Vec3::NEG_Y);
    let tail = |sx: f32, h: f32| (Vec3::new(sx, y(h), -0.5 * LENGTH + 0.02), Vec3::NEG_Z);
    [
        under(1.0, 1.0),
        under(-1.0, 1.0),
        under(1.0, -1.0),
        under(-1.0, -1.0),
        tail(0.55, 0.42),
        tail(-0.55, 0.42),
        tail(0.2, 0.32),
        tail(-0.2, 0.32),
        (Vec3::new(0.75, y(0.3), 2.0), Vec3::X),
        (Vec3::new(-0.75, y(0.3), 2.0), Vec3::NEG_X),
    ]
}

/// The Markab, part by part.
pub fn shapes() -> Vec<(Paint, Shape)> {
    let mut body = Shape::default();
    let rings: Vec<Vec<Vec3>> = SECTIONS
        .iter()
        .map(|&(z, w, lo, hi)| {
            let at = |x: f32, h: f32| Vec3::new(x, y(h), z);
            let d = hi - lo;
            // Round from its floor's left, over its roof, to its floor's right.
            vec![
                at(-w * 0.9, lo),
                at(-w, lo + 0.25 * d),
                at(-w, lo + 0.62 * d),
                at(-w * 0.8, hi),
                at(w * 0.8, hi),
                at(w, lo + 0.62 * d),
                at(w, lo + 0.25 * d),
                at(w * 0.9, lo),
            ]
        })
        .collect();
    body.loft(&rings, true, true);

    let mut glass = Shape::default();
    let rings: Vec<Vec<Vec3>> = CANOPY
        .iter()
        .map(|&(z, w, hi)| {
            let foot = top_at(z) - 0.02;
            vec![
                Vec3::new(-w, y(foot), z),
                Vec3::new(-w * 0.72, y(hi), z),
                Vec3::new(w * 0.72, y(hi), z),
                Vec3::new(w, y(foot), z),
            ]
        })
        .collect();
    glass.loft(&rings, false, true);

    let mut tyre = Shape::default();
    let r = 0.36;
    for (sx, sz) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let c = Vec3::new(sx * 0.5 * TRACK, y(r), sz * 0.5 * WHEELBASE);
        tyre.cylinder(c - Vec3::X * 0.14, c + Vec3::X * 0.14, r, 14);
    }

    let mut trim = Shape::default();
    // The fan's skirt round its floor, and its intake's grille in the nose.
    trim.cuboid(Vec3::new(-0.86, y(0.06), -1.9), Vec3::new(0.86, y(0.14), 1.9));
    trim.cuboid(Vec3::new(-0.5, y(0.22), 2.12), Vec3::new(0.5, y(0.3), 2.2));
    for (at, dir) in jets() {
        trim.cylinder(at - dir * 0.08, at + dir * 0.01, 0.07, 8);
    }

    let mut skirt = Shape::default();
    for side in [1.0f32, -1.0] {
        skirt.cuboid(
            Vec3::new(side * 0.87 - 0.02, y(0.07), -1.85),
            Vec3::new(side * 0.87 + 0.02, y(0.11), 1.85),
        );
    }
    skirt.cuboid(Vec3::new(-0.85, y(0.07), 1.88), Vec3::new(0.85, y(0.11), 1.92));
    skirt.cuboid(Vec3::new(-0.85, y(0.07), -1.92), Vec3::new(0.85, y(0.11), -1.88));

    let mut lamp = Shape::default();
    for side in [1.0f32, -1.0] {
        lamp.cuboid(Vec3::new(side * 0.6 - 0.16, y(0.38), 2.08), Vec3::new(side * 0.6 + 0.16, y(0.43), 2.16));
    }
    let mut tail = Shape::default();
    tail.cuboid(Vec3::new(-0.82, y(0.62), -2.25), Vec3::new(0.82, y(0.68), -2.19));

    vec![
        (Paint::Body, body),
        (Paint::Glass, glass),
        (Paint::Tyre, tyre),
        (Paint::Trim, trim),
        (Paint::Skirt, skirt),
        (Paint::Lamp, lamp),
        (Paint::Tail, tail),
    ]
}

/// A jet's plume, blowing along +z from its nozzle: a cone of cold gas, `length` m long.
pub fn plume(length: f32) -> Shape {
    let mut s = Shape::default();
    let sides = 8;
    let ring: Vec<Vec3> = (0..sides)
        .map(|k| {
            let t = k as f32 / sides as f32 * std::f32::consts::TAU;
            Vec3::new(t.cos(), t.sin(), 0.0)
        })
        .collect();
    let (r0, r1) = (0.06, 0.28);
    let tip = Vec3::Z * length;
    for i in 0..sides {
        let j = (i + 1) % sides;
        let (a, b) = (ring[i] * r0, ring[j] * r0);
        let (c, d) = (ring[j] * r1 + tip * 0.7, ring[i] * r1 + tip * 0.7);
        s.quad(a, b, c, d);
        s.tri(d, c, tip);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markab::{HEIGHT, WIDTH};

    #[test]
    fn it_all_fits_inside_its_body_s_box_on_its_wheels() {
        let (lo, hi) = (
            Vec3::new(-0.5 * WIDTH, -RIDE, -0.5 * LENGTH),
            Vec3::new(0.5 * WIDTH, HEIGHT - RIDE, 0.5 * LENGTH),
        );
        for (paint, s) in shapes() {
            assert!(s.triangles() > 0, "{paint:?}");
            for p in &s.positions {
                let p = Vec3::from_array(*p);
                assert!(p.cmpge(lo - 1e-4).all() && p.cmple(hi + 1e-4).all(), "{paint:?}: {p:?}");
            }
            for n in &s.normals {
                assert!((Vec3::from_array(*n).length() - 1.0).abs() < 1e-3, "{paint:?}");
            }
        }
        // Its wheels touch the face it stands on.
        let tyres = shapes().into_iter().find(|(p, _)| *p == Paint::Tyre).unwrap().1;
        let low = tyres.positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        assert!((low + RIDE).abs() < 0.01, "{low}");
    }

    #[test]
    fn its_roof_looks_up_and_its_nose_ahead() {
        let body = shapes().into_iter().find(|(p, _)| *p == Paint::Body).unwrap().1;
        let face = |at: Vec3| {
            let k = body
                .positions
                .chunks(3)
                .map(|t| (Vec3::from_array(t[0]) + Vec3::from_array(t[1]) + Vec3::from_array(t[2])) / 3.0)
                .enumerate()
                .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                .unwrap()
                .0;
            Vec3::from_array(body.normals[3 * k])
        };
        assert!(face(Vec3::new(0.0, y(0.9), -1.5)).y > 0.9, "the roof");
        assert!(face(Vec3::new(0.0, y(0.0), 0.0)).y < -0.9, "the floor");
        assert!(face(Vec3::new(0.0, y(0.27), 2.3)).z > 0.9, "the nose");
        assert!(face(Vec3::new(1.1, y(0.5), 0.0)).x > 0.9, "its left side");
    }

    #[test]
    fn it_is_light() {
        let n: usize = shapes().iter().map(|(_, s)| s.triangles()).sum();
        assert!(n < 2_000, "{n} triangles");
    }
}
