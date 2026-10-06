//! The modelling kit: procedural shapes for building mobile suits, placed in a bone's own space and
//! baked into one mesh per bone.
//!
//! Every vertex carries four numbers in its colour, which the client's hull shader reads: r, the
//! paint slot ([`Paint`]: 0-3 the livery's, 16+ fixed, 32+ bare metal, 48+ glowing, 64+ the
//! palette's second bank, 80+ the running lights, 96+ a unit number's segments); g, 1 on a bevel (worn edges catch the light there); b, a panel seed (so
//! neighbouring pieces don't share a plate layout); a, its ambient occlusion (1 open, less where
//! other pieces crowd it: see [`crate::ao`]). Normals are flat on every face and bevel, and smooth
//! on a turned shape round its axis and along its profile's gentle bends (a dome, an egg, a ball),
//! so edges stay crisp and curves read as curves.
//!
//! Each shape also leaves a simple stand-in for its volume (a [`Proxy`]) for the occlusion bake.

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
    /// A fixed palette entry ([`crate::paint`]): painted, bare metal, or glowing. Only painted
    /// takes the second bank (entries 16 and up); bare metal and glow take the first 16.
    Fixed(u8),
    Metal(u8),
    Glow(u8),
    /// One of the suit's running lights, lit by the hull shader in its own colour and rhythm; dark
    /// on a wreck and while its pilot sleeps.
    Light(Light),
    /// A segment of a stencilled unit number: `place` 0 the tens, 1 the units; `segment` 0-6 the
    /// seven-segment digit's a to g. The hull shader paints it where the suit's own number lights
    /// it and leaves it the body's paint elsewhere, so one mesh carries every suit's number.
    Digit {
        place: u8,
        segment: u8,
    },
}

/// A suit's running lights (the hull shader's `light()` draws each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    /// Steady red on the suit's own left, as on an aircraft or a ship: +x, since the suit faces +z
    /// with y up.
    Port,
    /// Steady green on its right (-x).
    Starboard,
    /// White, two quick flashes every 1.6 s.
    Strobe,
    /// Red anti-collision beacon, a pulse every 1.2 s.
    Beacon,
    /// A lamp lit steady and soft, warm white: sensor lamps, floodlights.
    Lamp,
}

impl Paint {
    fn code(self) -> f32 {
        let slot = match self {
            Paint::Body => 0,
            Paint::Trim => 1,
            Paint::Accent => 2,
            Paint::Eye => 3,
            // The second bank of fixed paints (16..32: [`crate::paint`]) codes from 64.
            Paint::Fixed(k) if k >= 16 => 64 + u32::from(k & 15),
            Paint::Fixed(k) => 16 + u32::from(k),
            Paint::Metal(k) => 32 + u32::from(k & 15),
            Paint::Glow(k) => 48 + u32::from(k & 15),
            Paint::Light(l) => 80 + l as u32,
            Paint::Digit { place, segment } => 96 + u32::from(place.min(1)) * 7 + u32::from(segment.min(6)),
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

/// A shape's volume, roughly, for the occlusion bake: a box (centre, orthonormal axes and half
/// extents) or a capsule (a segment and a radius).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Proxy {
    Box { centre: Vec3, axes: [Vec3; 3], half: Vec3 },
    Capsule { a: Vec3, b: Vec3, r: f32 },
}

impl Proxy {
    /// Signed distance from `p` to its surface (negative inside).
    pub fn distance(&self, p: Vec3) -> f32 {
        match *self {
            Proxy::Box { centre, axes, half } => {
                let d = p - centre;
                let q = Vec3::new(axes[0].dot(d).abs(), axes[1].dot(d).abs(), axes[2].dot(d).abs()) - half;
                q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
            }
            Proxy::Capsule { a, b, r } => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                (p - (a + ab * t)).length() - r
            }
        }
    }

    /// A sphere round it: its centre and radius.
    pub fn bounds(&self) -> (Vec3, f32) {
        match *self {
            Proxy::Box { centre, half, .. } => (centre, half.length()),
            Proxy::Capsule { a, b, r } => ((a + b) * 0.5, (b - a).length() * 0.5 + r),
        }
    }

    /// Moved by `offset`.
    pub fn shifted(self, offset: Vec3) -> Self {
        match self {
            Proxy::Box { centre, axes, half } => Proxy::Box { centre: centre + offset, axes, half },
            Proxy::Capsule { a, b, r } => Proxy::Capsule { a: a + offset, b: b + offset, r },
        }
    }

    /// The box round 8 corners indexed `x + 2y + 4z` (a hexahedron's), with axes along its mean
    /// edges.
    fn from_corners(c: &[Vec3; 8]) -> Option<Self> {
        let centre = c.iter().copied().sum::<Vec3>() / 8.0;
        let edge =
            |bit: usize| (0..8).filter(|i| i & bit == 0).map(|i| c[i | bit] - c[i]).sum::<Vec3>() / 4.0;
        let (ex, ey) = (edge(1), edge(2));
        let x = ex.try_normalize()?;
        let y = (ey - x * ey.dot(x)).try_normalize()?;
        let z = x.cross(y);
        let axes = [x, y, z];
        let mut half = Vec3::ZERO;
        for p in c {
            let d = *p - centre;
            half = half.max(Vec3::new(x.dot(d).abs(), y.dot(d).abs(), z.dot(d).abs()));
        }
        Some(Proxy::Box { centre, axes, half })
    }
}

/// Where a turned shape's profile bends by less than this (radians), the segments either side
/// share their normal, so the curve shades smooth; a sharper corner (a cylinder's rim, a band's
/// edge) stays crisp.
const SMOOTH_BEND: f32 = 0.6;

/// A mesh being built.
#[derive(Default)]
pub struct Builder {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
    /// Each shape's first vertex and its volume, in order.
    prims: Vec<(usize, Proxy)>,
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
    /// Moves everything built so far by `d`.
    pub fn translate(&mut self, d: Vec3) {
        if d == Vec3::ZERO {
            return;
        }
        for p in &mut self.pos {
            *p = (Vec3::from(*p) + d).to_array();
        }
        for (_, proxy) in &mut self.prims {
            *proxy = proxy.shifted(d);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    pub fn triangles(&self) -> usize {
        self.idx.len() / 3
    }

    /// The shapes' volumes, in the bone's space.
    pub fn proxies(&self) -> impl Iterator<Item = Proxy> + '_ {
        self.prims.iter().map(|(_, p)| *p)
    }

    /// Which shape each vertex belongs to (an index into [`Builder::proxies`]).
    pub fn vertex_shapes(&self) -> Vec<usize> {
        let mut out = vec![0; self.pos.len()];
        for (k, w) in self.prims.iter().enumerate() {
            let end = self.prims.get(k + 1).map_or(self.pos.len(), |n| n.0);
            for s in &mut out[w.0..end] {
                *s = k;
            }
        }
        out
    }

    /// Every vertex's position and normal.
    pub fn vertices(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        self.pos.iter().zip(&self.nrm).map(|(p, n)| (Vec3::from(*p), Vec3::from(*n)))
    }

    /// Sets each vertex's ambient occlusion (the colour's alpha).
    pub fn set_occlusion(&mut self, ao: &[f32]) {
        for (c, a) in self.col.iter_mut().zip(ao) {
            c[3] = *a;
        }
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
        if let Some(proxy) = Proxy::from_corners(&c) {
            self.prims.push((self.pos.len(), proxy));
        }
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
        let profile = &facing_out(profile)[..];
        let lin = Mat3::from(xf.matrix3);
        if let (Some(lo), Some(hi)) =
            (profile.iter().map(|p| p.1).reduce(f32::min), profile.iter().map(|p| p.1).reduce(f32::max))
        {
            // A capsule down the axis, as wide as the shape's widest (scaled as placed).
            let r = profile.iter().map(|p| p.0).fold(0.0, f32::max);
            let scale = (lin.x_axis.length() + lin.z_axis.length()) * 0.5;
            let (a, b) = (xf.transform_point3(Vec3::Y * lo), xf.transform_point3(Vec3::Y * hi));
            // The capsule's caps round off past the ends: pull them in by the radius.
            let dir = (b - a).normalize_or_zero();
            let r = r * scale;
            let inset = (r).min((b - a).length() * 0.5);
            self.prims.push((self.pos.len(), Proxy::Capsule { a: a + dir * inset, b: b - dir * inset, r }));
        }
        self.sweep(profile, segments.max(3), 0.0, std::f32::consts::TAU, paint, xf);
    }

    /// Part of a turned shape: `profile` swept round local +y from angle `from` to `to` (radians:
    /// 0 along +x, a quarter turn along +z). A closed profile (its last point its first) is capped
    /// at both ends, for ribs, cuffs and collars that wrap only part of the way round.
    pub fn lathe_arc(
        &mut self,
        profile: &[(f32, f32)],
        segments: u32,
        from: f32,
        to: f32,
        paint: Paint,
        xf: Affine3A,
    ) {
        if profile.len() < 2 {
            return;
        }
        let profile = &facing_out(profile)[..];
        let ring = |r: f32, y: f32, t: f32| xf.transform_point3(Vec3::new(r * t.cos(), y, r * t.sin()));
        // An arc wraps round empty space, so it occludes as a speck at its middle (it's no convex
        // volume a neighbour could hide under).
        let n = profile.len() as f32;
        let (cr, cy) = profile.iter().fold((0.0, 0.0), |(r, y), p| (r + p.0 / n, y + p.1 / n));
        let mid = ring(cr, cy, (from + to) * 0.5);
        self.prims.push((self.pos.len(), Proxy::Capsule { a: mid, b: mid, r: 0.01 }));
        self.sweep(profile, segments.max(1), from, to, paint, xf);
        let closed = profile.len() > 3 && profile[0] == profile[profile.len() - 1];
        if !closed {
            return;
        }
        // The profile's outline, counter-clockwise in (r, y), clipped into triangles for each end.
        let outline: Vec<Vec2> = profile[..profile.len() - 1].iter().map(|&(r, y)| Vec2::new(r, y)).collect();
        let m = outline.len();
        let area: f32 = (0..m).map(|i| outline[i].perp_dot(outline[(i + 1) % m])).sum();
        let ccw: Vec<Vec2> = if area < 0.0 { outline.iter().rev().copied().collect() } else { outline };
        let tris = ear_clip(&ccw);
        let step = (to - from).signum() * 0.01;
        for (t, inward) in [(from, step), (to, -step)] {
            let inside = ring(cr, cy, t + inward);
            for tri in &tris {
                let verts = tri.map(|i| ring(ccw[i].x, ccw[i].y, t));
                self.poly(&verts, inside, paint, 0.0);
            }
        }
    }

    /// The surface of a turned shape from angle `from` to `to`, in `segments` steps.
    fn sweep(&mut self, profile: &[(f32, f32)], seg: u32, from: f32, to: f32, paint: Paint, xf: Affine3A) {
        let lin = Mat3::from(xf.matrix3);
        let normal_m = lin.inverse().transpose();
        // A mirror, or a sweep running backwards, turns the faces round.
        let flip = (lin.determinant() < 0.0) != (to < from);
        // Each segment's normal in the profile's (radius, height) plane, None where it has no length.
        let normals: Vec<Option<Vec2>> = profile
            .windows(2)
            .map(|w| Vec2::new(w[1].1 - w[0].1, -(w[1].0 - w[0].0)).try_normalize())
            .collect();
        let closed = profile.len() > 3 && profile[0] == profile[profile.len() - 1];
        // The nearest segment with a length before or after segment `i` (round the loop if closed).
        let neighbour = |i: usize, step: isize| {
            let n = normals.len() as isize;
            let mut j = i as isize;
            for _ in 1..n {
                j += step;
                if closed {
                    j = j.rem_euclid(n);
                } else if !(0..n).contains(&j) {
                    return None;
                }
                if let Some(m) = normals[j as usize] {
                    return Some(m);
                }
            }
            None
        };
        // A segment's normal at one of its ends: shared with the next segment over a gentle bend.
        let at_end = |own: Vec2, other: Option<Vec2>| match other {
            Some(o) if own.angle_to(o).abs() < SMOOTH_BEND => (own + o).normalize_or(own),
            _ => own,
        };
        // On the axis, a gently domed cap looks straight along it (a cone's point keeps its own).
        let on_axis = |r: f32, m: Vec2| {
            let axis = Vec2::new(0.0, m.y.signum());
            if r.abs() < 1e-6 && m.angle_to(axis).abs() < SMOOTH_BEND { axis } else { m }
        };
        for (i, w) in profile.windows(2).enumerate() {
            let ((r0, y0), (r1, y1)) = (w[0], w[1]);
            let Some(own) = normals[i] else { continue };
            let ends =
                [on_axis(r0, at_end(own, neighbour(i, -1))), on_axis(r1, at_end(own, neighbour(i, 1)))];
            let base = self.pos.len() as u32;
            let col = [paint.code(), 0.0, self.seed, 1.0];
            for k in 0..=seg {
                let t = from + k as f32 / seg as f32 * (to - from);
                let (s, co) = t.sin_cos();
                for ((r, y), m) in [(r0, y0), (r1, y1)].into_iter().zip(ends) {
                    let local_n = Vec3::new(m.x * co, m.y, m.x * s).normalize_or(Vec3::Y);
                    let n = (normal_m * local_n).normalize_or(Vec3::Y);
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
                if flip {
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
        // Its volume: the outline's bounds, the plate's depth.
        let (lo, hi) = ring.iter().fold((ring[0], ring[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let corners = [0usize, 1, 2, 3, 4, 5, 6, 7].map(|i| {
            let x = if i & 1 != 0 { hi.x } else { lo.x };
            let y = if i & 2 != 0 { hi.y } else { lo.y };
            place(Vec2::new(x, y), if i & 4 != 0 { h } else { -h })
        });
        if let Some(proxy) = Proxy::from_corners(&corners) {
            self.prims.push((self.pos.len(), proxy));
        }
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

/// A turned shape's profile, run the way that faces its surface out: one closed on the axis at both
/// ends from bottom to top, a closed loop counter-clockwise in (radius, height). Any other runs as
/// given, its direction saying which side is outside (a bell's lip turns back down its inside).
fn facing_out(profile: &[(f32, f32)]) -> std::borrow::Cow<'_, [(f32, f32)]> {
    let (Some(&first), Some(&last)) = (profile.first(), profile.last()) else {
        return profile.into();
    };
    let backwards = if profile.len() > 3 && first == last {
        let n = profile.len() - 1;
        let area: f32 =
            (0..n).map(|i| profile[i].0 * profile[i + 1].1 - profile[i + 1].0 * profile[i].1).sum();
        area < 0.0
    } else {
        first.0 == 0.0 && last.0 == 0.0 && first.1 > last.1
    };
    if backwards { profile.iter().rev().copied().collect::<Vec<_>>().into() } else { profile.into() }
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

    /// A closed profile swept part of the way round is a closed solid, faced outward: its signed
    /// volume (the divergence theorem over its triangles) is the swept ring's, whichever way it
    /// runs and mirrored or not.
    #[test]
    fn a_capped_arc_is_closed_and_faces_outward() {
        let ring = [(1.0, -0.1), (1.2, -0.1), (1.2, 0.1), (1.0, 0.1), (1.0, -0.1)];
        let half = std::f32::consts::PI;
        // Half an annulus 0.2 thick (its 32-sided polygon falls short of it by a fraction of a
        // per cent).
        let want = 0.5 * half * (1.2f32.powi(2) - 1.0) * 0.2;
        for (from, to) in [(0.0, half), (half, 0.0), (1.0, 1.0 + half)] {
            for xf in [Affine3A::IDENTITY, mirrored(at(1.0, 2.0, 0.0))] {
                let mut b = Builder::default();
                b.lathe_arc(&ring, 32, from, to, Paint::Body, xf);
                let volume: f32 = b
                    .idx
                    .chunks(3)
                    .map(|t| {
                        let [a, bb, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(b.pos[i as usize]));
                        a.dot(bb.cross(c)) / 6.0
                    })
                    .sum();
                assert!((volume / want - 1.0).abs() < 0.02, "{from}..{to}: volume {volume}, want {want}");
            }
        }
    }

    /// A ball shades round along its profile as well as round its axis (every normal off its pole
    /// points out from its centre), while a cylinder keeps its rims crisp: its wall's normals
    /// level, its caps' upright.
    #[test]
    fn turned_shapes_are_smooth_over_gentle_bends_only() {
        let mut ball = Builder::default();
        ball.sphere(1.0, 10, Paint::Body, at(0.0, 2.0, 0.0));
        for (p, n) in ball.vertices() {
            let d = p - Vec3::Y * 2.0;
            if d.x.hypot(d.z) > 1e-3 {
                assert!(n.angle_between(d.normalize()) < 0.02, "{p}: {n}");
            }
        }
        let mut can = Builder::default();
        can.cylinder(1.0, 2.0, 12, Paint::Body, Affine3A::IDENTITY);
        for (p, n) in can.vertices() {
            assert!(n.y.abs() < 1e-4 || (n.y.abs() - 1.0).abs() < 1e-4, "{p}: {n}");
        }
    }

    /// However a closed profile is written, top down or clockwise, its solid faces out: its signed
    /// volume is positive.
    #[test]
    fn closed_profiles_face_out_either_way() {
        let lens = [(0.0, 0.1), (0.28, 0.1), (0.26, 0.17), (0.0, 0.24)];
        let ring = [(1.0, -0.1), (1.2, -0.1), (1.2, 0.1), (1.0, 0.1), (1.0, -0.1)];
        let volume = |b: &Builder| -> f32 {
            b.idx
                .chunks(3)
                .map(|t| {
                    let [a, bb, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(b.pos[i as usize]));
                    a.dot(bb.cross(c)) / 6.0
                })
                .sum()
        };
        for profile in [lens.to_vec(), ring.to_vec()] {
            let backwards: Vec<_> = profile.iter().rev().copied().collect();
            for p in [profile, backwards] {
                let mut b = Builder::default();
                b.lathe(&p, 24, Paint::Body, Affine3A::IDENTITY);
                assert!(volume(&b) > 0.0, "{p:?} faces in");
            }
        }
    }

    #[test]
    fn the_second_bank_codes_from_64() {
        let slot = |p: Paint| (p.code() * 255.0).round() as u32;
        assert_eq!(slot(Paint::Fixed(crate::paint::GLASS)), 16 + 15);
        assert_eq!(slot(Paint::Fixed(crate::paint::FRAME_BROWN)), 64);
        // The lights code from 80, in the order the hull shader's `light()` takes them.
        assert_eq!(slot(Paint::Light(Light::Port)), 80);
        assert_eq!(slot(Paint::Light(Light::Lamp)), 84);
        // A unit number's segments from 96: the tens' a to g, then the units'.
        assert_eq!(slot(Paint::Digit { place: 0, segment: 0 }), 96);
        assert_eq!(slot(Paint::Digit { place: 1, segment: 6 }), 109);
    }
}
