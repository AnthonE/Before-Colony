//! Meshes of the bodies, built from the shapes suits stand on, so what is drawn is what is walked
//! on: every vertex lies on its shape's surface (`bc_sim::bodies::Shape`), and a face sags off it
//! only by its chord (centimetres).
//!
//! - Rounded boxes and the rounded cylinder are built in their flat and rounded zones separately,
//!   with the rounding in equal angles, so the curve is as fine as asked wherever it is.
//! - A box face a sphere is cut from (MO-II's Aft Well) is the bowl, as a polar grid, and the face
//!   round it out to the box's rounded edges.
//! - An ellipsoid (Hermit) is a cube-sphere; where its crater bowls are cut, its triangles are
//!   dropped and the bowl put in, stitched to the hole round its rim.
//!
//! Plain mesh data for the renderer, in the body's frame. Not in the tick: built once.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use bc_sim::bodies::{Base, Prim, Shape, SphereCut};
use glam::Vec3;

/// Positions and normals (outward, unit) with triangles wound counter-clockwise seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    fn vertex(&mut self, p: Vec3, n: Vec3) -> u32 {
        self.positions.push(p.to_array());
        self.normals.push(n.normalize_or(Vec3::Y).to_array());
        (self.positions.len() - 1) as u32
    }

    fn pos(&self, i: u32) -> Vec3 {
        Vec3::from_array(self.positions[i as usize])
    }

    fn nrm(&self, i: u32) -> Vec3 {
        Vec3::from_array(self.normals[i as usize])
    }

    /// A triangle, wound to face where its vertices' normals point; none if it has no area (at a
    /// pole, where a grid's row closes up).
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        let (pa, pb, pc) = (self.pos(a), self.pos(b), self.pos(c));
        let cross = (pb - pa).cross(pc - pa);
        if cross.length_squared() <= 1e-12 * (pb - pa).length_squared().max(1e-6) {
            return;
        }
        if cross.dot(self.nrm(a) + self.nrm(b) + self.nrm(c)) >= 0.0 {
            self.indices.extend([a, b, c]);
        } else {
            self.indices.extend([a, c, b]);
        }
    }

    /// Quads joining a grid of vertices, `rows` × `cols`, laid out row after row from `first`.
    fn grid(&mut self, first: u32, rows: usize, cols: usize, wrap: bool) {
        let at = |r: usize, c: usize| first + (r * cols + c % cols) as u32;
        let across = if wrap { cols } else { cols - 1 };
        for r in 0..rows - 1 {
            for c in 0..across {
                let (a, b, cc, d) = (at(r, c), at(r, c + 1), at(r + 1, c + 1), at(r + 1, c));
                self.tri(a, b, cc);
                self.tri(a, cc, d);
            }
        }
    }

    /// Drops the vertices no triangle uses.
    fn compact(&mut self) {
        let mut to = vec![u32::MAX; self.positions.len()];
        let mut kept = MeshData::default();
        for i in &mut self.indices {
            let k = &mut to[*i as usize];
            if *k == u32::MAX {
                *k = kept.positions.len() as u32;
                kept.positions.push(self.positions[*i as usize]);
                kept.normals.push(self.normals[*i as usize]);
            }
            *i = *k;
        }
        (self.positions, self.normals) = (kept.positions, kept.normals);
    }

    /// Adds `other`'s triangles to these.
    pub fn append(&mut self, other: &MeshData) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

/// The coordinates a face of a rounded box is divided at along one axis (relative to the box's
/// centre): `m` equal angles of rounding at each end, and `flat` equal pieces between.
fn box_ticks(half: f32, round: f32, m: usize, flat: usize) -> Vec<f32> {
    let inner = half - round;
    let mut out = Vec::new();
    for i in (1..=m).rev() {
        out.push(-(inner + round * (FRAC_PI_4 * i as f32 / m as f32).tan()));
    }
    for i in 0..=flat {
        out.push(-inner + 2.0 * inner * i as f32 / flat as f32);
    }
    for i in 1..=m {
        out.push(inner + round * (FRAC_PI_4 * i as f32 / m as f32).tan());
    }
    out
}

/// A face of a box: its axis (0 x, 1 y, 2 z) and side (+1, -1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub axis: usize,
    pub sign: f32,
}

/// A box rounded at its edges and corners by `round`, outer half-extents exactly `half`, centred
/// at `c`; `segs` equal angles to a quarter round.
pub fn round_box(c: Vec3, half: Vec3, round: f32, segs: u32) -> MeshData {
    round_box_open(c, half, round, segs, 1, None)
}

/// [`round_box`], with each flat face in `flat` × `flat` pieces, and the flat of face `open` left
/// out (for [`dished_face`] to fill).
pub fn round_box_open(c: Vec3, half: Vec3, round: f32, segs: u32, flat: u32, open: Option<Face>) -> MeshData {
    let mut mesh = MeshData::default();
    let m = (segs as usize / 2).max(1);
    let flat = flat.max(1) as usize;
    let inner = half - Vec3::splat(round);
    for axis in 0..3 {
        let (j, l) = ((axis + 1) % 3, (axis + 2) % 3);
        let (tj, tl) = (box_ticks(half[j], round, m, flat), box_ticks(half[l], round, m, flat));
        for sign in [1.0f32, -1.0] {
            let first = mesh.positions.len() as u32;
            for &a in &tj {
                for &b in &tl {
                    let mut v = Vec3::ZERO;
                    v[axis] = sign * half[axis];
                    v[j] = a;
                    v[l] = b;
                    let q = v.clamp(-inner, inner);
                    let n = (v - q).normalize();
                    mesh.vertex(c + q + n * round, n);
                }
            }
            if open == Some(Face { axis, sign }) {
                // Every quad but the flat ones.
                let (rows, cols) = (tj.len(), tl.len());
                let flat_j = |r: usize| r >= m && r < m + flat;
                let at = |r: usize, k: usize| first + (r * cols + k) as u32;
                for r in 0..rows - 1 {
                    for k in 0..cols - 1 {
                        if flat_j(r) && flat_j(k) {
                            continue;
                        }
                        let (p, q, s, t) = (at(r, k), at(r, k + 1), at(r + 1, k + 1), at(r + 1, k));
                        mesh.tri(p, q, s);
                        mesh.tri(p, s, t);
                    }
                }
            } else {
                mesh.grid(first, tj.len(), tl.len(), false);
            }
        }
    }
    mesh
}

/// The face `face` of the rounded box (`c`, `half`, `round`) with the sphere `cut` taken out of
/// it: the bowl, `rings` rings deep and `sectors` round, and the flat of the face round it out to
/// where its rounded edges begin, met there by [`round_box_open`] with `sectors / 4` pieces a side.
/// The cut is centred on the face's axis, beyond it.
pub fn dished_face(
    c: Vec3,
    half: Vec3,
    round: f32,
    cut: SphereCut,
    face: Face,
    rings: u32,
    sectors: u32,
) -> MeshData {
    let mut mesh = MeshData::default();
    let (k, j, l) = (face.axis, (face.axis + 1) % 3, (face.axis + 2) % 3);
    let (ek, ej) = (unit(k) * face.sign, unit(j));
    let plane = c[k] + face.sign * half[k];
    // How far beyond the face the cut's centre is, and the bowl's rim on it.
    let out = (cut.c[k] - plane) * face.sign;
    let rim = (cut.r * cut.r - out * out).max(0.0).sqrt();
    let rim_angle = (out / cut.r).clamp(-1.0, 1.0).acos();
    let centre = cut.c - ek * out;
    let inner = half - Vec3::splat(round);
    // Round the square where the flat meets the rounded edges, `per` pieces a side.
    let per = (sectors as usize / 4).max(1);
    // (Each tick as `round_box_open` puts it, so the two meet vertex for vertex.)
    let tick = |h: f32, i: usize| -h + 2.0 * h * i as f32 / per as f32;
    let edge: Vec<Vec3> = (0..4 * per)
        .map(|s| {
            let i = s % per;
            let (a, b) = match s / per {
                0 => (inner[j], tick(inner[l], i)),
                1 => (-tick(inner[j], i), inner[l]),
                2 => (-inner[j], -tick(inner[l], i)),
                _ => (tick(inner[j], i), -inner[l]),
            };
            let mut p = c;
            p[j] += a;
            p[l] += b;
            p[k] = plane;
            p
        })
        .collect();
    let n = edge.len();
    // The bowl: from its floor (on the axis) out to the rim, each sector toward its edge point.
    let first = mesh.positions.len() as u32;
    for r in 0..=rings {
        let alpha = rim_angle * r as f32 / rings as f32;
        for e in &edge {
            let dir = (*e - centre).normalize_or(ej);
            let p = cut.c + (-ek * alpha.cos() + dir * alpha.sin()) * cut.r;
            mesh.vertex(p, cut.c - p);
        }
    }
    mesh.grid(first, rings as usize + 1, n, true);
    // The flat, from the rim out to the edge.
    let ring_out = 8usize;
    let first = mesh.positions.len() as u32;
    for r in 0..=ring_out {
        let u = r as f32 / ring_out as f32;
        for e in &edge {
            let dir = (*e - centre).normalize_or(ej);
            let from = centre + dir * rim;
            mesh.vertex(from.lerp(*e, u), ek);
        }
    }
    mesh.grid(first, ring_out + 1, n, true);
    mesh
}

fn unit(axis: usize) -> Vec3 {
    let mut v = Vec3::ZERO;
    v[axis] = 1.0;
    v
}

/// A cylinder along x rounded at its rims, outer half-length `half_len` and radius `r` exactly,
/// centred at `c`; `radial` sectors round.
pub fn round_cylinder_x(c: Vec3, half_len: f32, r: f32, round: f32, radial: u32) -> MeshData {
    // The profile, (x, ρ) with its normal, from the -x end's centre round to the +x end's.
    let arc = 8;
    let mut profile: Vec<(f32, f32, f32, f32)> = vec![(-half_len, 0.0, -1.0, 0.0)];
    let (ax, ar) = (half_len - round, r - round);
    for i in 0..=arc {
        let phi = FRAC_PI_2 * i as f32 / arc as f32;
        profile.push((-ax - round * phi.cos(), ar + round * phi.sin(), -phi.cos(), phi.sin()));
    }
    let along = ((2.0 * ax / 50.0).ceil() as usize).max(1);
    for i in 1..along {
        profile.push((-ax + 2.0 * ax * i as f32 / along as f32, r, 0.0, 1.0));
    }
    for i in 0..=arc {
        let phi = FRAC_PI_2 * i as f32 / arc as f32;
        profile.push((ax + round * phi.sin(), ar + round * phi.cos(), phi.sin(), phi.cos()));
    }
    profile.push((half_len, 0.0, 1.0, 0.0));
    revolve(c, Vec3::X, Vec3::Y, Vec3::Z, &profile, radial)
}

/// A capsule from `a` to `b`, radius `r`; `segs` equal angles to a quarter round.
pub fn capsule(a: Vec3, b: Vec3, r: f32, segs: u32) -> MeshData {
    let len = a.distance(b);
    let d = (b - a).normalize_or(Vec3::X);
    let e1 = d.any_orthonormal_vector();
    let e2 = d.cross(e1);
    let mut profile = Vec::new();
    let m = segs.max(1) as usize;
    for i in 0..=m {
        let psi = -FRAC_PI_2 + FRAC_PI_2 * i as f32 / m as f32;
        profile.push((r * psi.sin(), r * psi.cos(), psi.sin(), psi.cos()));
    }
    for i in 0..=m {
        let psi = FRAC_PI_2 * i as f32 / m as f32;
        profile.push((len + r * psi.sin(), r * psi.cos(), psi.sin(), psi.cos()));
    }
    revolve(a, d, e1, e2, &profile, 4 * segs)
}

/// A profile `(along, ρ, n_along, n_ρ)` turned about the axis through `o` along `d` (`e1`, `e2`
/// square to it and each other).
fn revolve(o: Vec3, d: Vec3, e1: Vec3, e2: Vec3, profile: &[(f32, f32, f32, f32)], sectors: u32) -> MeshData {
    let mut mesh = MeshData::default();
    let sectors = sectors.max(3) as usize;
    for &(x, rho, nx, nr) in profile {
        for s in 0..sectors {
            let phi = TAU * s as f32 / sectors as f32;
            let radial = e1 * phi.cos() + e2 * phi.sin();
            mesh.vertex(o + d * x + radial * rho, d * nx + radial * nr);
        }
    }
    mesh.grid(0, profile.len(), sectors, true);
    mesh
}

/// An ellipsoid of half-axes `axes` at the origin as a cube-sphere, `n` × `n` a face, with
/// everything inside `cuts` left out (each vertex of a triangle kept is outside every cut, or on
/// one).
pub fn ellipsoid(axes: Vec3, n: u32, cuts: &[SphereCut]) -> MeshData {
    let mut mesh = MeshData::default();
    let n = n.max(1) as usize;
    // Vertices are shared across the cube's seams, so a hole's edge is the only edge with one side.
    let mut seen: HashMap<[u32; 3], u32> = HashMap::new();
    let inside = |p: Vec3| cuts.iter().any(|k| p.distance(k.c) < k.r);
    for axis in 0..3 {
        let (j, l) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [1.0f32, -1.0] {
            let mut ids = Vec::with_capacity((n + 1) * (n + 1));
            for a in 0..=n {
                for b in 0..=n {
                    let mut v = Vec3::ZERO;
                    v[axis] = sign;
                    v[j] = -1.0 + 2.0 * a as f32 / n as f32;
                    v[l] = -1.0 + 2.0 * b as f32 / n as f32;
                    let key = v.to_array().map(f32::to_bits);
                    let id = *seen.entry(key).or_insert_with(|| {
                        let p = axes * v.normalize();
                        mesh.vertex(p, p / (axes * axes))
                    });
                    ids.push(id);
                }
            }
            let at = |a: usize, b: usize| ids[a * (n + 1) + b];
            for a in 0..n {
                for b in 0..n {
                    let quad = [at(a, b), at(a, b + 1), at(a + 1, b + 1), at(a + 1, b)];
                    if quad.iter().any(|&i| inside(mesh.pos(i))) {
                        continue;
                    }
                    mesh.tri(quad[0], quad[1], quad[2]);
                    mesh.tri(quad[0], quad[2], quad[3]);
                }
            }
        }
    }
    mesh
}

/// The bowl `cut` takes out of the ellipsoid of half-axes `axes`: `rings` rings from its floor out
/// to where it meets the ellipsoid, `sectors` round.
pub fn crater_bowl(axes: Vec3, cut: SphereCut, rings: u32, sectors: u32) -> MeshData {
    let mut mesh = MeshData::default();
    let down = (-cut.c).normalize_or(-Vec3::Y);
    let e1 = down.any_orthonormal_vector();
    let e2 = down.cross(e1);
    let at = |alpha: f32, phi: f32| {
        cut.c + (down * alpha.cos() + (e1 * phi.cos() + e2 * phi.sin()) * alpha.sin()) * cut.r
    };
    let outside = |p: Vec3| (p / axes).length() > 1.0;
    for r in 0..=rings {
        for s in 0..sectors {
            let phi = TAU * s as f32 / sectors as f32;
            // The rim, where the bowl comes out of the ellipsoid, by halving.
            let (mut lo, mut hi) = (0.0f32, FRAC_PI_2);
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                if outside(at(mid, phi)) { hi = mid } else { lo = mid }
            }
            let p = at(lo * r as f32 / rings as f32, phi);
            mesh.vertex(p, cut.c - p);
        }
    }
    mesh.grid(0, rings as usize + 1, sectors as usize, true);
    mesh
}

/// Triangles closing the gap between the edge of the hole `cut` left in `holed` (an
/// [`ellipsoid`]) and the rim of its `bowl` ([`crater_bowl`], `sectors` round), both onto `out`.
fn stitch(out: &mut MeshData, holed: &MeshData, bowl: &MeshData, cut: SphereCut, sectors: u32) {
    // The hole's edge: edges with a triangle on one side only, near the cut.
    let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
    for t in holed.indices.chunks(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let near = cut.r * 1.6;
    let mut edge: Vec<u32> = uses
        .iter()
        .filter(|(_, n)| **n == 1)
        .flat_map(|((a, b), _)| [*a, *b])
        .filter(|&i| holed.pos(i).distance(cut.c) < near)
        .collect();
    edge.sort_unstable();
    edge.dedup();
    let down = (-cut.c).normalize_or(-Vec3::Y);
    let e1 = down.any_orthonormal_vector();
    let e2 = down.cross(e1);
    let angle = |p: Vec3| {
        let d = p - cut.c;
        d.dot(e2).atan2(d.dot(e1)).rem_euclid(TAU)
    };
    // Both rings in `out`, each in order round the cut's axis.
    let mut hole: Vec<(f32, u32)> =
        edge.iter().map(|&i| (angle(holed.pos(i)), out.vertex(holed.pos(i), holed.nrm(i)))).collect();
    hole.sort_by(|a, b| a.0.total_cmp(&b.0));
    let rim_first = bowl.positions.len() as u32 - sectors;
    let mut rim: Vec<(f32, u32)> = (0..sectors)
        .map(|s| {
            let i = rim_first + s;
            (angle(bowl.pos(i)), out.vertex(bowl.pos(i), holed_normal(bowl.pos(i))))
        })
        .collect();
    rim.sort_by(|a, b| a.0.total_cmp(&b.0));
    if hole.is_empty() || rim.is_empty() {
        return;
    }
    // Zip the two rings together, always stepping the one whose next point comes first.
    let (h, r) = (hole.len(), rim.len());
    let (mut i, mut k) = (0, 0);
    while i < h || k < r {
        let next_h = hole[(i + 1) % h].0 + if i + 1 >= h { TAU } else { 0.0 };
        let next_r = rim[(k + 1) % r].0 + if k + 1 >= r { TAU } else { 0.0 };
        if k >= r || (i < h && next_h <= next_r) {
            out.tri(hole[i % h].1, hole[(i + 1) % h].1, rim[k % r].1);
            i += 1;
        } else {
            out.tri(hole[i % h].1, rim[(k + 1) % r].1, rim[k % r].1);
            k += 1;
        }
    }

    /// At the rim the surface turns from the ellipsoid into the bowl: the strip faces out.
    fn holed_normal(p: Vec3) -> Vec3 {
        p.normalize_or(Vec3::Y)
    }
}

/// Every mesh of `shape`: one for each primitive of a union (a box face a cut is taken out of
/// dished), or for an ellipsoid, the ellipsoid with its bowls.
pub fn shape_meshes(shape: &Shape) -> Vec<MeshData> {
    match shape.base {
        Base::Ellipsoid(axes) => {
            let holed = ellipsoid(axes, 96, shape.cuts);
            let mut mesh = holed.clone();
            for &cut in shape.cuts {
                let bowl = crater_bowl(axes, cut, 48, 96);
                mesh.append(&bowl);
                stitch(&mut mesh, &holed, &bowl, cut, 96);
            }
            mesh.compact();
            vec![mesh]
        }
        Base::Union(prims) => prims.iter().map(|p| prim_mesh(p, shape.cuts)).collect(),
        // The city's meshes are its streamer's (`city_mesh`).
        Base::City => Vec::new(),
    }
}

/// One primitive's mesh, with any of `cuts` that dishes one of its faces.
fn prim_mesh(p: &Prim, cuts: &[SphereCut]) -> MeshData {
    match *p {
        Prim::Sphere { c, r } => capsule(c, c, r, 16),
        Prim::Capsule { a, b, r } => capsule(a, b, r, 8),
        Prim::CylinderX { c, half_len, r, round } => round_cylinder_x(c, half_len, r, round, 64),
        Prim::RoundBox { c, half, round } => {
            let dish = cuts.iter().find_map(|&cut| dished(c, half, round, cut).map(|f| (cut, f)));
            match dish {
                None => round_box(c, half, round, 16),
                Some((cut, face)) => {
                    let sectors = 128;
                    let mut mesh = round_box_open(c, half, round, 16, sectors / 4, Some(face));
                    mesh.append(&dished_face(c, half, round, cut, face, 64, sectors));
                    // (The open face's flat corners went unused.)
                    mesh.compact();
                    mesh
                }
            }
        }
    }
}

/// The face of the rounded box (`c`, `half`, `round`) that `cut` is a bowl in, if any: centred on
/// the face's axis beyond it, its rim inside the face's flat.
fn dished(c: Vec3, half: Vec3, round: f32, cut: SphereCut) -> Option<Face> {
    let rel = cut.c - c;
    (0..3).find_map(|axis| {
        let (j, l) = ((axis + 1) % 3, (axis + 2) % 3);
        let out = rel[axis].abs() - half[axis];
        let rim = (cut.r * cut.r - out * out).max(0.0).sqrt();
        let flat = (half[j] - round).min(half[l] - round);
        (out > 0.0 && out < cut.r && rel[j].abs() < 1e-3 && rel[l].abs() < 1e-3 && rim < flat)
            .then(|| Face { axis, sign: rel[axis].signum() })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::content::landmarks::LANDMARKS;

    /// How far `p` is off what `mesh` was built from: the primitive (or ellipsoid) less the cuts.
    fn off(base: &Base, prim: Option<&Prim>, cuts: &[SphereCut], p: Vec3) -> f32 {
        let d = match (base, prim) {
            (_, Some(prim)) => prim.probe(p).dist,
            (Base::Ellipsoid(a), None) => Shape::ellipsoid(*a).probe(p).dist,
            (Base::Union(_), None) => unreachable!("a union's meshes are its primitives'"),
            (Base::City, None) => unreachable!("the city's meshes are its streamer's"),
        };
        cuts.iter().fold(d, |d, k| d.max(-(p.distance(k.c) - k.r)))
    }

    fn check(mesh: &MeshData, what: &str, off: impl Fn(Vec3) -> f32) {
        assert!(mesh.triangles() > 0, "{what}: empty");
        let worst = mesh.positions.iter().map(|p| off(Vec3::from_array(*p)).abs()).fold(0.0f32, f32::max);
        assert!(worst <= 0.12, "{what}: a vertex {worst} m off the surface");
        for n in &mesh.normals {
            assert!((Vec3::from_array(*n).length() - 1.0).abs() < 1e-3, "{what}: a normal not unit");
        }
        // Wound outward: every triangle faces the way its corners' normals do.
        for t in mesh.indices.chunks(3) {
            let (a, b, c) = (mesh.pos(t[0]), mesh.pos(t[1]), mesh.pos(t[2]));
            let n = mesh.nrm(t[0]) + mesh.nrm(t[1]) + mesh.nrm(t[2]);
            assert!((b - a).cross(c - a).dot(n) >= 0.0, "{what}: a triangle faces in");
        }
    }

    #[test]
    fn body_meshes_lie_on_their_sdf() {
        for d in &LANDMARKS {
            let meshes = shape_meshes(&d.shape);
            match d.shape.base {
                Base::Union(prims) => {
                    assert_eq!(meshes.len(), prims.len());
                    for (k, (mesh, prim)) in meshes.iter().zip(prims).enumerate() {
                        check(mesh, &format!("{} primitive {k}", d.name), |p| {
                            off(&d.shape.base, Some(prim), d.shape.cuts, p)
                        });
                    }
                }
                Base::Ellipsoid(_) => {
                    check(&meshes[0], d.name, |p| off(&d.shape.base, None, d.shape.cuts, p));
                    // The bowls are in: vertices on each cut's floor.
                    for cut in d.shape.cuts {
                        let floor = cut.c - cut.c.normalize() * cut.r;
                        let nearest = meshes[0]
                            .positions
                            .iter()
                            .map(|p| Vec3::from_array(*p).distance(floor))
                            .fold(f32::INFINITY, f32::min);
                        assert!(nearest < 0.01, "{}: no floor at {floor}", d.name);
                    }
                }
                Base::City => unreachable!("no landmark is the colony's city"),
            }
        }
        // MO-II's Aft Well is dished into its aft module's end face.
        let Base::Union(prims) = LANDMARKS[0].shape.base else { unreachable!("MO-II is a union") };
        let aft = &shape_meshes(&LANDMARKS[0].shape)[1];
        let floor = Vec3::new(-235.0, 0.0, 0.0);
        assert!(aft.positions.iter().any(|p| Vec3::from_array(*p).distance(floor) < 1e-3), "{:?}", prims[1]);
    }

    #[test]
    fn primitives_are_where_their_shapes_are() {
        let c = Vec3::new(3.0, -2.0, 1.0);
        let b = Prim::RoundBox { c, half: Vec3::new(30.0, 80.0, 24.0), round: 6.0 };
        check(&round_box(c, Vec3::new(30.0, 80.0, 24.0), 6.0, 16), "a rounded box", |p| b.probe(p).dist);
        let y = Prim::CylinderX { c, half_len: 200.0, r: 60.0, round: 4.0 };
        check(&round_cylinder_x(c, 200.0, 60.0, 4.0, 64), "a rounded cylinder", |p| y.probe(p).dist);
        let (a, e) = (Vec3::new(260.0, 0.0, 0.0), Vec3::new(330.0, 5.0, -2.0));
        let k = Prim::Capsule { a, b: e, r: 4.0 };
        check(&capsule(a, e, 4.0, 8), "a capsule", |p| k.probe(p).dist);
        // A rounded edge's chords sag no more than they should: round·(1 − cos(π/32)) at 16 to a
        // quarter.
        let mesh = round_box(c, Vec3::splat(20.0), 6.0, 16);
        for t in mesh.indices.chunks(3) {
            let mid = (mesh.pos(t[0]) + mesh.pos(t[1]) + mesh.pos(t[2])) / 3.0;
            assert!(b_box(c, 20.0, 6.0).probe(mid).dist > -0.03, "a face sags into the box");
        }

        fn b_box(c: Vec3, h: f32, r: f32) -> Prim {
            Prim::RoundBox { c, half: Vec3::splat(h), round: r }
        }
    }
}
