//! The modelling kit: procedural shapes for building mobile suits, placed in a bone's own space and
//! baked into one mesh per bone.
//!
//! Every vertex carries three numbers in its colour, which the client's hull shader reads: r, the
//! paint slot ([`Paint`]); g, 1 on a bevel (worn edges catch the light there); b, a panel seed (so
//! neighbouring pieces don't share a plate layout). Normals are flat on every face and bevel, and
//! smooth only round the axis of a turned shape, so edges stay crisp.

use glam::{Affine3A, Mat3, Vec2, Vec3};

/// Which paint a surface takes (decoded by the client's hull shader).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    /// The livery's body, trim and accent colours.
    Body,
    Trim,
    Accent,
    /// Sensor glow, in the livery's eye colour.
    Eye,
    /// A fixed palette entry ([`crate::paint`]): painted, bare metal, or glowing.
    Fixed(u8),
    Metal(u8),
    Glow(u8),
}

impl Paint {
    fn code(self) -> f32 {
        let slot = match self {
            Paint::Body => 0,
            Paint::Trim => 1,
            Paint::Accent => 2,
            Paint::Eye => 3,
            Paint::Fixed(k) => 16 + u32::from(k & 15),
            Paint::Metal(k) => 32 + u32::from(k & 15),
            Paint::Glow(k) => 48 + u32::from(k & 15),
        };
        slot as f32 / 255.0
    }
}

/// A triangle mesh: positions, normals, colours (see the module docs) and indices.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// A mesh being built.
#[derive(Default)]
pub struct Builder {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
    /// Panel seed for the shapes that follow (0..1).
    pub seed: f32,
}

/// A place in the bone: translation, rotation and scale (a negative x scale mirrors).
pub fn at(x: f32, y: f32, z: f32) -> Affine3A {
    Affine3A::from_translation(Vec3::new(x, y, z))
}

/// Mirrors a placement across x (the suit's other side).
pub fn mirrored(xf: Affine3A) -> Affine3A {
    Affine3A::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * xf
}

impl Builder {
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    pub fn triangles(&self) -> usize {
        self.idx.len() / 3
    }

    /// A flat polygon (convex, in order round its edge), wound to face away from `inside`.
    fn poly(&mut self, verts: &[Vec3], inside: Vec3, paint: Paint, bevel: f32) {
        if verts.len() < 3 {
            return;
        }
        let centre = verts.iter().copied().sum::<Vec3>() / verts.len() as f32;
        let mut n = Vec3::ZERO;
        for i in 0..verts.len() {
            n += verts[i].cross(verts[(i + 1) % verts.len()]);
        }
        if n.length_squared() < 1e-12 {
            return; // degenerate (a zero-width bevel)
        }
        let flip = n.dot(centre - inside) < 0.0;
        let n = if flip { -n } else { n }.normalize();
        let base = self.pos.len() as u32;
        let col = [paint.code(), bevel, self.seed, 1.0];
        for v in verts {
            self.pos.push(v.to_array());
            self.nrm.push(n.to_array());
            self.col.push(col);
        }
        for i in 1..verts.len() as u32 - 1 {
            if flip {
                self.idx.extend_from_slice(&[base, base + i + 1, base + i]);
            } else {
                self.idx.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        }
    }

    /// A chamfered hexahedron from its 8 corners, indexed `x + 2y + 4z` (each bit 0 for the low
    /// side, 1 for the high side), already placed. Any convex shape with planar faces works:
    /// boxes, tapers, wedges, sheared plates.
    pub fn hexa(&mut self, c: [Vec3; 8], chamfer: f32, paint: Paint) {
        let centre = c.iter().copied().sum::<Vec3>() / 8.0;
        // The inset vertex of face (axis a) at corner i: moved in along the face's two edges there.
        let inset = |i: usize, a: usize| {
            let mut v = c[i];
            for b in 0..3 {
                if b != a {
                    let d = c[i ^ (1 << b)] - c[i];
                    let len = d.length();
                    v += d / len.max(1e-6) * chamfer.min(len * 0.45);
                }
            }
            v
        };
        let corners_of = |a: usize, s: usize| {
            let (b, d) = ((a + 1) % 3, (a + 2) % 3);
            // Round the face in order: (0,0), (1,0), (1,1), (0,1) over the other two axes.
            [(0, 0), (1, 0), (1, 1), (0, 1)].map(|(u, v)| (s << a) | (u << b) | (v << d))
        };
        for a in 0..3 {
            for s in 0..2 {
                let quad = corners_of(a, s).map(|i| inset(i, a));
                self.poly(&quad, centre, paint, 0.0);
            }
        }
        if chamfer <= 0.0 {
            return;
        }
        // Edges: a strip between the two faces that meet there.
        for e in 0..3 {
            let (b, d) = ((e + 1) % 3, (e + 2) % 3);
            for i in 0..8usize {
                if i & (1 << e) != 0 {
                    continue;
                }
                let j = i | (1 << e);
                let strip = [inset(i, b), inset(j, b), inset(j, d), inset(i, d)];
                self.poly(&strip, centre, paint, 1.0);
            }
        }
        // Corners: a small triangle where three bevels meet.
        for i in 0..8 {
            let tri = [inset(i, 0), inset(i, 1), inset(i, 2)];
            self.poly(&tri, centre, paint, 1.0);
        }
    }

    /// A block of `size` (full extents), its top (+y) face scaled by `top` in x and z and shifted
    /// by `shift`: a box, a taper, a wedge or a sheared plate.
    pub fn block(&mut self, size: Vec3, top: Vec2, shift: Vec2, chamfer: f32, paint: Paint, xf: Affine3A) {
        let h = size * 0.5;
        let mut c = [Vec3::ZERO; 8];
        for (i, corner) in c.iter_mut().enumerate() {
            let sx = if i & 1 != 0 { 1.0 } else { -1.0 };
            let sz = if i & 4 != 0 { 1.0 } else { -1.0 };
            let v = if i & 2 != 0 {
                Vec3::new(sx * h.x * top.x + shift.x, h.y, sz * h.z * top.y + shift.y)
            } else {
                Vec3::new(sx * h.x, -h.y, sz * h.z)
            };
            *corner = xf.transform_point3(v);
        }
        self.hexa(c, chamfer, paint);
    }

    /// A plain chamfered box.
    pub fn cube(&mut self, size: Vec3, chamfer: f32, paint: Paint, xf: Affine3A) {
        self.block(size, Vec2::ONE, Vec2::ZERO, chamfer, paint, xf);
    }

    /// A turned shape round local +y: `profile` runs bottom to top as (radius, height); a radius
    /// of 0 closes that end. Smooth round the axis, faceted along the profile.
    pub fn lathe(&mut self, profile: &[(f32, f32)], segments: u32, paint: Paint, xf: Affine3A) {
        let lin = Mat3::from(xf.matrix3);
        let normal_m = lin.inverse().transpose();
        let mirrored = lin.determinant() < 0.0;
        let seg = segments.max(3);
        for w in profile.windows(2) {
            let ((r0, y0), (r1, y1)) = (w[0], w[1]);
            let (dr, dy) = (r1 - r0, y1 - y0);
            if dr.abs() + dy.abs() < 1e-6 {
                continue;
            }
            let base = self.pos.len() as u32;
            let col = [paint.code(), 0.0, self.seed, 1.0];
            for k in 0..=seg {
                let t = k as f32 / seg as f32 * std::f32::consts::TAU;
                let (s, co) = t.sin_cos();
                let local_n = Vec3::new(dy * co, -dr, dy * s).normalize_or(Vec3::Y);
                let n = (normal_m * local_n).normalize_or(Vec3::Y);
                for (r, y) in [(r0, y0), (r1, y1)] {
                    self.pos.push(xf.transform_point3(Vec3::new(r * co, y, r * s)).to_array());
                    self.nrm.push(n.to_array());
                    self.col.push(col);
                }
            }
            for k in 0..seg {
                // Seen from outside: this angle's bottom and top, then the next angle's top and
                // bottom, run counter-clockwise (a mirror reverses them).
                let a = base + k * 2;
                let (q0, q1, q2, q3) = (a, a + 1, a + 3, a + 2);
                if mirrored {
                    self.idx.extend_from_slice(&[q0, q2, q1, q0, q3, q2]);
                } else {
                    self.idx.extend_from_slice(&[q0, q1, q2, q0, q2, q3]);
                }
            }
        }
    }

    /// A cylinder along local +y, centred, capped.
    pub fn cylinder(&mut self, radius: f32, height: f32, segments: u32, paint: Paint, xf: Affine3A) {
        let h = height * 0.5;
        self.lathe(&[(0.0, -h), (radius, -h), (radius, h), (0.0, h)], segments, paint, xf);
    }

    /// A sphere (or, scaled, an ellipsoid).
    pub fn sphere(&mut self, radius: f32, rings: u32, paint: Paint, xf: Affine3A) {
        let profile: Vec<(f32, f32)> = (0..=rings)
            .map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * k as f32 / rings as f32;
                (radius * a.cos(), radius * a.sin())
            })
            .collect();
        self.lathe(&profile, rings * 2, paint, xf);
    }

    /// A flat plate: `outline` (a simple polygon in local x-y, either winding) extruded `depth`
    /// along z, centred. Its rim counts as bevel.
    pub fn extrude(&mut self, outline: &[Vec2], depth: f32, paint: Paint, xf: Affine3A) {
        let n = outline.len();
        if n < 3 {
            return;
        }
        // Counter-clockwise, for the ear clipping.
        let area: f32 = (0..n).map(|i| outline[i].perp_dot(outline[(i + 1) % n])).sum();
        let ring: Vec<Vec2> =
            if area < 0.0 { outline.iter().rev().copied().collect() } else { outline.to_vec() };
        let h = depth * 0.5;
        let place = |p: Vec2, z: f32| xf.transform_point3(p.extend(z));
        for tri in ear_clip(&ring) {
            let mid = (ring[tri[0]] + ring[tri[1]] + ring[tri[2]]) / 3.0;
            for z in [h, -h] {
                // Facing away from the mid-plane just behind it.
                self.poly(&tri.map(|i| place(ring[i], z)), place(mid, 0.0), paint, 0.0);
            }
        }
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            // Counter-clockwise, so the outside is to the right of each edge.
            let out = Vec2::new(b.y - a.y, a.x - b.x).normalize_or_zero();
            let quad = [place(a, -h), place(b, -h), place(b, h), place(a, h)];
            self.poly(&quad, place((a + b) * 0.5 - out * 0.01, 0.0), paint, 1.0);
        }
    }

    pub fn finish(self) -> MeshData {
        MeshData { positions: self.pos, normals: self.nrm, colors: self.col, indices: self.idx }
    }
}

/// Triangulates a simple counter-clockwise polygon by ear clipping.
fn ear_clip(poly: &[Vec2]) -> Vec<[usize; 3]> {
    let mut left: Vec<usize> = (0..poly.len()).collect();
    let mut tris = Vec::with_capacity(poly.len().saturating_sub(2));
    let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| {
        let d1 = (b - a).perp_dot(p - a);
        let d2 = (c - b).perp_dot(p - b);
        let d3 = (a - c).perp_dot(p - c);
        d1 > 0.0 && d2 > 0.0 && d3 > 0.0
    };
    let mut guard = 0;
    while left.len() > 3 && guard < poly.len() * poly.len() {
        guard += 1;
        let m = left.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if (b - a).perp_dot(c - b) <= 0.0 {
                continue; // reflex
            }
            if left.iter().any(|&j| j != ia && j != ib && j != ic && inside(poly[j], a, b, c)) {
                continue;
            }
            tris.push([ia, ib, ic]);
            left.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            break; // not simple; give up on the rest
        }
    }
    if left.len() == 3 {
        tris.push([left[0], left[1], left[2]]);
    }
    tris
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ear_clip_square_and_concave() {
        let square = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];
        assert_eq!(ear_clip(&square).len(), 2);
        // An L shape: 6 corners, 4 triangles.
        let l = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];
        assert_eq!(ear_clip(&l).len(), 4);
    }

    #[test]
    fn chamfered_box_faces_outward() {
        let mut b = Builder::default();
        b.cube(Vec3::new(2.0, 1.0, 3.0), 0.1, Paint::Body, Affine3A::IDENTITY);
        // 6 faces, 12 bevel strips and 8 corners: 12 + 24 + 8 triangles.
        assert_eq!(b.triangles(), 44);
        for t in b.idx.chunks(3) {
            let [a, bb, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(b.pos[i as usize]));
            let n = (bb - a).cross(c - a);
            assert!(n.dot((a + bb + c) / 3.0) > 0.0, "a triangle faces inward");
        }
    }

    #[test]
    fn mirrored_lathe_faces_outward() {
        for xf in [Affine3A::IDENTITY, mirrored(at(1.0, 0.0, 0.0))] {
            let mut b = Builder::default();
            b.cylinder(1.0, 2.0, 12, Paint::Trim, xf);
            let centre = xf.transform_point3(Vec3::ZERO);
            for t in b.idx.chunks(3) {
                let [a, bb, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(b.pos[i as usize]));
                let n = (bb - a).cross(c - a);
                if n.length() > 1e-6 {
                    assert!(n.dot((a + bb + c) / 3.0 - centre) > 0.0, "a triangle faces inward");
                }
            }
        }
    }
}
