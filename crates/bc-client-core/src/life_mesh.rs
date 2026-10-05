//! The city's life as meshes (`bc_sim::colony::traffic`'s cars, `walkers`' people): the vehicles at
//! two levels of detail, and the civilians as a bank of posed figures, a mesh for each frame of each
//! gait, so a figure is posed by swapping its mesh (no skinning, no vertex shader).
//!
//! Every mesh faces +z with its feet or wheels on the ground (`y` 0), its middle at the origin: a
//! vehicle inside its kind's footprint (`Kind::size`), a figure inside the walkers' radius. What a
//! vertex is rides in its colour, for `shaders/life.wgsl`: r its slot ([`slot`], over 255), g 1 on
//! bevels, b a panel seed, a baked occlusion.

use std::f32::consts::{PI, TAU};

use bc_sim::colony::furniture::BENCH_HEIGHT;
use bc_sim::colony::traffic::Kind;
use bc_sim::colony::walkers::Pose;
use glam::{Quat, Vec3};

use crate::figure::{self, Piece};
use crate::life::DECK;

/// What a part is, for the material (`life_lib.wgsl` keeps the same numbers): a vehicle's below 16,
/// a figure's from 16.
pub mod slot {
    /// The livery's paint, and its second (a stripe, a roof, the odd door).
    pub const PAINT: u8 = 0;
    pub const PAINT2: u8 = 1;
    /// Bumpers, sills, black plastic.
    pub const TRIM: u8 = 2;
    pub const GLASS: u8 = 3;
    pub const TYRE: u8 = 4;
    pub const HEAD: u8 = 5;
    pub const TAIL: u8 = 6;
    pub const BLINK_L: u8 = 7;
    pub const BLINK_R: u8 = 8;
    /// The driver's shape on the glass, or a far scooter's rider (gone when it's parked).
    pub const DRIVER: u8 = 9;
    /// A taxi's roof sign.
    pub const SIGN: u8 = 10;
    /// A civilian's clothes, skin, hair and shoes.
    pub const TOP: u8 = 16;
    pub const BOTTOM: u8 = 17;
    pub const SKIN: u8 = 18;
    pub const HAIR: u8 = 19;
    pub const SHOES: u8 = 20;
    /// Every slot there is.
    pub const ALL: [u8; 16] = [
        PAINT, PAINT2, TRIM, GLASS, TYRE, HEAD, TAIL, BLINK_L, BLINK_R, DRIVER, SIGN, TOP, BOTTOM, SKIN,
        HAIR, SHOES,
    ];
}

/// A mesh of the city's life, ready to become a Bevy `Mesh` (positions, normals, colours, indices).
#[derive(Clone, Debug, Default)]
pub struct LifeMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// How a part is coloured: its slot, whether it's a bevel, its panel's seed.
#[derive(Clone, Copy, Debug)]
struct Ink {
    slot: u8,
    bevel: f32,
    panel: f32,
}

impl LifeMesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// The slot of vertex `i`.
    pub fn slot(&self, i: usize) -> u8 {
        (self.colors[i][0] * 255.0).round() as u8
    }

    /// The box round it, `(min, max)`.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p)))
        })
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, ink: Ink, ao: f32) -> u32 {
        self.positions.push(p.to_array());
        self.normals.push(n.to_array());
        self.colors.push([f32::from(ink.slot) / 255.0, ink.bevel, ink.panel, ao]);
        self.positions.len() as u32 - 1
    }

    /// A flat polygon (convex, its corners in order either way round), facing away from `inside`.
    fn face(&mut self, p: &[Vec3], inside: Vec3, ink: Ink, ao: impl Fn(Vec3, Vec3) -> f32) {
        let centre = p.iter().copied().sum::<Vec3>() / p.len() as f32;
        let mut n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let flip = n.dot(centre - inside) < 0.0;
        if flip {
            n = -n;
        }
        let base = self.positions.len() as u32;
        for v in p {
            self.vertex(*v, n, ink, ao(*v, n));
        }
        for k in 1..p.len() as u32 - 1 {
            if flip {
                self.indices.extend_from_slice(&[base, base + k + 1, base + k]);
            } else {
                self.indices.extend_from_slice(&[base, base + k, base + k + 1]);
            }
        }
    }

    /// A box from `min` to `max`, its faces out but those `skip` says (−y, +y, −x, +x, −z, +z).
    fn cuboid(
        &mut self,
        min: Vec3,
        max: Vec3,
        inks: [Ink; 6],
        skip: [bool; 6],
        ao: impl Fn(Vec3, Vec3) -> f32,
    ) {
        let c =
            |x: usize, y: usize, z: usize| Vec3::new([min.x, max.x][x], [min.y, max.y][y], [min.z, max.z][z]);
        let inside = (min + max) * 0.5;
        let faces = [
            [c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)],
            [c(0, 1, 0), c(1, 1, 0), c(1, 1, 1), c(0, 1, 1)],
            [c(0, 0, 0), c(0, 1, 0), c(0, 1, 1), c(0, 0, 1)],
            [c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)],
            [c(0, 0, 0), c(1, 0, 0), c(1, 1, 0), c(0, 1, 0)],
            [c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)],
        ];
        for (i, f) in faces.iter().enumerate() {
            if !skip[i] {
                self.face(f, inside, inks[i], &ao);
            }
        }
    }

    /// A side profile (`(z, y)` corners of a convex polygon, in order) run across from −`hw` to
    /// `hw`, or tapering to `top_hw` at the profile's highest corner. Edges `inks` says are drawn
    /// in its slot (one per edge, from corner `i` to `i + 1`); the sides in `side`. Faces facing
    /// down aren't drawn.
    fn prism(
        &mut self,
        profile: &[(f32, f32)],
        hw: f32,
        top_hw: f32,
        inks: &[Ink],
        side: Ink,
        ao: impl Fn(Vec3, Vec3) -> f32,
    ) {
        let (y0, y1) = profile.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let width = |y: f32| hw + (top_hw - hw) * ((y - y0) / (y1 - y0).max(1e-3));
        let (zc, yc) = profile.iter().fold((0.0, 0.0), |(a, b), p| (a + p.0, b + p.1));
        let inside = Vec3::new(0.0, yc / profile.len() as f32, zc / profile.len() as f32);
        let at = |i: usize, s: f32| {
            let (z, y) = profile[i % profile.len()];
            Vec3::new(s * width(y), y, z)
        };
        for (i, ink) in inks.iter().enumerate().take(profile.len()) {
            let quad = [at(i, -1.0), at(i + 1, -1.0), at(i + 1, 1.0), at(i, 1.0)];
            let n = (quad[1] - quad[0]).cross(quad[2] - quad[0]);
            let centre = quad.iter().copied().sum::<Vec3>() * 0.25;
            let out = if n.dot(centre - inside) < 0.0 { -n } else { n };
            if out.normalize_or_zero().y < -0.7 {
                continue;
            }
            self.face(&quad, inside, *ink, &ao);
        }
        for s in [-1.0, 1.0] {
            let side_pts: Vec<Vec3> = (0..profile.len()).map(|i| at(i, s)).collect();
            self.face(&side_pts, inside, side, &ao);
        }
    }

    /// A wheel: a six-sided tyre about the x axis, `r` round to its flats and `width` wide, its
    /// lowest flat on the ground.
    fn wheel(&mut self, x: f32, z: f32, r: f32, width: f32) {
        let ink = Ink { slot: slot::TYRE, bevel: 0.0, panel: 0.0 };
        let cy = r * (PI / 6.0).cos();
        let rim = |k: usize, dx: f32| {
            let a = TAU * k as f32 / 6.0 + PI / 6.0;
            Vec3::new(x + dx, cy - r * a.cos(), z + r * a.sin())
        };
        let inside = Vec3::new(x, cy, z);
        let hw = width * 0.5;
        for k in 0..6 {
            self.face(&[rim(k, -hw), rim(k + 1, -hw), rim(k + 1, hw), rim(k, hw)], inside, ink, |_, _| 1.0);
        }
        for dx in [-hw, hw] {
            let pts: Vec<Vec3> = (0..6).map(|k| rim(k, dx)).collect();
            self.face(&pts, inside, ink, |_, _| 1.0);
        }
    }

    /// A lamp: a flat rectangle `half` (x, y) about `centre`, facing +z (`dir` 1) or −z (−1).
    fn lamp(&mut self, centre: Vec3, half: (f32, f32), dir: f32, slot: u8) {
        let ink = Ink { slot, bevel: 0.0, panel: 0.0 };
        let (hx, hy) = half;
        let p = [
            centre + Vec3::new(-hx, -hy, 0.0),
            centre + Vec3::new(hx, -hy, 0.0),
            centre + Vec3::new(hx, hy, 0.0),
            centre + Vec3::new(-hx, hy, 0.0),
        ];
        self.face(&p, centre - Vec3::Z * dir, ink, |_, _| 1.0);
    }

    /// A tapered limb hanging down `length` from the origin: `sides` sides, `r0` round at the top
    /// and `r1` at the bottom, capped at both ends.
    fn limb(&mut self, length: f32, r0: f32, r1: f32, sides: usize, ink: Ink) {
        let ring = |y: f32, r: f32, k: usize| {
            let a = k as f32 / sides as f32 * TAU;
            Vec3::new(a.cos() * r, y, a.sin() * r)
        };
        let inside = Vec3::new(0.0, -0.5 * length, 0.0);
        for k in 0..sides {
            let q = [ring(0.0, r0, k), ring(0.0, r0, k + 1), ring(-length, r1, k + 1), ring(-length, r1, k)];
            self.face(&q, inside, ink, |_, _| 1.0);
        }
        for (y, r) in [(0.0, r0), (-length, r1)] {
            let cap: Vec<Vec3> = (0..sides).map(|k| ring(y, r, k)).collect();
            self.face(&cap, inside, ink, |_, _| 1.0);
        }
    }

    /// A skirt: a flared, open-ended cone round the y axis, from `r0` at `y0` to `r1` at `y1`.
    fn skirt(&mut self, y0: f32, r0: f32, y1: f32, r1: f32, sides: usize, ink: Ink) {
        let ring = |y: f32, r: f32, k: usize| {
            let a = (k as f32 + 0.5) / sides as f32 * TAU;
            Vec3::new(a.cos() * r, y, a.sin() * r)
        };
        let inside = Vec3::new(0.0, 0.5 * (y0 + y1), 0.0);
        for k in 0..sides {
            let q = [ring(y0, r0, k), ring(y0, r0, k + 1), ring(y1, r1, k + 1), ring(y1, r1, k)];
            self.face(&q, inside, ink, |_, _| 1.0);
        }
    }

    /// A ball of radius `r` about `centre` in `rings` bands top to bottom and `segs` round, or its
    /// top `upto` bands alone (a cap).
    fn ball(&mut self, centre: Vec3, r: f32, rings: usize, upto: usize, segs: usize, ink: Ink) {
        let at = |i: usize, j: usize| {
            let v = i as f32 / rings as f32 * PI;
            let u = j as f32 / segs as f32 * TAU;
            centre + Vec3::new(v.sin() * u.sin(), v.cos(), v.sin() * u.cos()) * r
        };
        for i in 0..upto.min(rings) {
            for j in 0..segs {
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
                // The poles' bands are triangles.
                let n = |p: Vec3| (p - centre).normalize_or_zero();
                let base = self.positions.len() as u32;
                if i == 0 {
                    for p in [a, d, c] {
                        self.vertex(p, n(p), ink, 1.0);
                    }
                    self.indices.extend_from_slice(&[base, base + 1, base + 2]);
                } else if i == rings - 1 {
                    for p in [a, d, b] {
                        self.vertex(p, n(p), ink, 1.0);
                    }
                    self.indices.extend_from_slice(&[base, base + 1, base + 2]);
                } else {
                    for p in [a, d, c, b] {
                        self.vertex(p, n(p), ink, 1.0);
                    }
                    self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }

    /// Adds `other`, turned by `rot` and moved to `at`.
    pub fn append(&mut self, other: &LifeMesh, at: Vec3, rot: Quat) {
        let base = self.positions.len() as u32;
        for (p, n) in other.positions.iter().zip(&other.normals) {
            self.positions.push((at + rot * Vec3::from(*p)).to_array());
            self.normals.push((rot * Vec3::from(*n)).to_array());
        }
        self.colors.extend_from_slice(&other.colors);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

fn ink(slot: u8, panel: f32) -> Ink {
    Ink { slot, bevel: 0.0, panel }
}

/// Darker low down, under the sills and in the arches, and on faces turned to the ground.
fn body_ao(p: Vec3, n: Vec3) -> f32 {
    let low = (0.55 + 0.45 * (p.y / 0.7)).clamp(0.55, 1.0);
    if n.y < -0.5 { low * 0.6 } else { low }
}

// ---- Vehicles -----------------------------------------------------------------------------------

/// What's driven, as drawn: the traffic's kinds, a car either a runabout or a hatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    /// Phantasy Star Online's colony car: small, rounded, clean, in pale paints.
    Runabout,
    /// Rust's older five-door: boxier, faded, worn.
    Hatch,
    /// A runabout lengthened, with a roof sign.
    Taxi,
    /// A boxy panel van.
    Van,
    /// The pilots' stand-up scooter.
    Scooter,
}

impl Body {
    pub const ALL: [Body; 5] = [Body::Runabout, Body::Hatch, Body::Taxi, Body::Van, Body::Scooter];

    /// The traffic's kind it's drawn for.
    pub fn kind(self) -> Kind {
        match self {
            Body::Runabout | Body::Hatch => Kind::Car,
            Body::Taxi => Kind::Taxi,
            Body::Van => Kind::Van,
            Body::Scooter => Kind::Scooter,
        }
    }

    pub fn index(self) -> usize {
        Body::ALL.iter().position(|b| *b == self).unwrap()
    }
}

/// A body's mesh: near (full, a few hundred triangles) or mid (two boxes and its lamps' faces).
pub fn vehicle_mesh(body: Body, near: bool) -> LifeMesh {
    let mut m = LifeMesh::default();
    match (body, near) {
        (Body::Scooter, true) => scooter(&mut m),
        (Body::Scooter, false) => scooter_mid(&mut m),
        (Body::Van, true) => van(&mut m),
        (Body::Van, false) => van_mid(&mut m),
        (b, true) => car(&mut m, b),
        (b, false) => car_mid(&mut m, b),
    }
    m
}

/// A car's dimensions: half its length and width, its height, how far back its cabin's glass
/// starts and ends (from the nose and the tail), its wheels' radius and how far in from its ends.
struct CarShape {
    hl: f32,
    hw: f32,
    roof: f32,
    belt: f32,
    screen: (f32, f32),
    back: (f32, f32),
    wheel: f32,
    axle: f32,
}

fn car_shape(body: Body) -> CarShape {
    match body {
        Body::Hatch => CarShape {
            hl: 2.08,
            hw: 0.86,
            roof: 1.44,
            belt: 0.88,
            screen: (0.95, 1.6),
            back: (0.06, 0.18),
            wheel: 0.31,
            axle: 0.66,
        },
        Body::Taxi => CarShape {
            hl: 2.22,
            hw: 0.85,
            roof: 1.47,
            belt: 0.84,
            screen: (0.95, 1.75),
            back: (0.3, 0.6),
            wheel: 0.3,
            axle: 0.68,
        },
        _ => CarShape {
            hl: 1.86,
            hw: 0.84,
            roof: 1.47,
            belt: 0.84,
            screen: (0.9, 1.6),
            back: (0.28, 0.55),
            wheel: 0.29,
            axle: 0.6,
        },
    }
}

/// A runabout, a hatch or a taxi.
fn car(m: &mut LifeMesh, body: Body) {
    let c = car_shape(body);
    let (hl, hw) = (c.hl, c.hw);
    let paint = ink(slot::PAINT, 0.1);
    let bevel = Ink { bevel: 1.0, ..paint };
    // The lower body: a vertical nose and tail, the bonnet sloping up to the screen, the boot.
    let hatch = body == Body::Hatch;
    let lower = [
        (-hl + 0.01, 0.22),
        (hl - 0.01, 0.22),
        (hl - 0.01, 0.56),
        (hl - if hatch { 0.06 } else { 0.16 }, if hatch { 0.76 } else { 0.7 }),
        (hl - c.screen.0, c.belt - 0.02),
        (-hl + c.back.0 + 0.02, c.belt),
        (-hl + 0.01, if hatch { 0.84 } else { 0.66 }),
    ];
    m.prism(&lower, hw, hw - 0.06, &[paint, paint, bevel, paint, paint, bevel, paint], paint, body_ao);
    // The cabin's glass all round, narrower towards the roof.
    let glass = ink(slot::GLASS, 0.3);
    let roof_y = c.roof - 0.08;
    let cabin = [
        (hl - c.screen.0, c.belt - 0.03),
        (hl - c.screen.1, roof_y),
        (-hl + c.back.1, roof_y),
        (-hl + c.back.0, c.belt - 0.01),
    ];
    m.prism(&cabin, hw - 0.1, hw - 0.24, &[glass; 4], glass, |_, _| 1.0);
    // The roof.
    let top = ink(slot::PAINT2, 0.5);
    m.cuboid(
        Vec3::new(-hw + 0.25, roof_y - 0.02, -hl + c.back.1 - 0.02),
        Vec3::new(hw - 0.25, c.roof, hl - c.screen.1 + 0.02),
        [top; 6],
        [true, false, false, false, false, false],
        body_ao,
    );
    // Bumpers and sills.
    let trim = ink(slot::TRIM, 0.7);
    for dir in [-1.0f32, 1.0] {
        let (z0, z1) = if dir > 0.0 { (hl - 0.12, hl) } else { (-hl, -hl + 0.12) };
        m.cuboid(
            Vec3::new(-hw + 0.04, 0.2, z0),
            Vec3::new(hw - 0.04, 0.36, z1),
            [trim; 6],
            [true, false, false, false, false, false],
            body_ao,
        );
        let x = dir * hw;
        m.cuboid(
            Vec3::new(x.min(x - dir * 0.04), 0.2, -hl + c.axle + c.wheel + 0.05),
            Vec3::new(x.max(x - dir * 0.04), 0.3, hl - c.axle - c.wheel - 0.05),
            [trim; 6],
            [true, false, false, false, false, false],
            body_ao,
        );
    }
    // A taxi's band and roof sign; a hatch's odd door: just proud of the lower body's sides, which
    // lean in from `hw` at its foot to `hw` − 0.06 at its top.
    let top = lower.iter().fold(0.0f32, |a, p| a.max(p.1));
    let side = |y: f32| hw - 0.06 * (y - 0.22) / (top - 0.22) + 0.003;
    if body == Body::Taxi {
        let band = ink(slot::PAINT2, 0.9);
        for dir in [-1.0f32, 1.0] {
            let (a, b) = (dir * side(0.46), dir * side(0.56));
            m.face(
                &[
                    Vec3::new(a, 0.46, -hl + 0.3),
                    Vec3::new(a, 0.46, hl - 0.3),
                    Vec3::new(b, 0.56, hl - 0.3),
                    Vec3::new(b, 0.56, -hl + 0.3),
                ],
                Vec3::new(0.0, 0.5, 0.0),
                band,
                |_, _| 1.0,
            );
        }
        m.cuboid(
            Vec3::new(-0.3, c.roof, -0.35),
            Vec3::new(0.3, 1.55, 0.05),
            [ink(slot::SIGN, 0.0); 6],
            [true, false, false, false, false, false],
            |_, _| 1.0,
        );
    }
    if hatch {
        let (a, b) = (side(0.32), side(c.belt - 0.04));
        m.face(
            &[
                Vec3::new(a, 0.32, -hl + 0.95),
                Vec3::new(a, 0.32, -0.05),
                Vec3::new(b, c.belt - 0.04, -0.05),
                Vec3::new(b, c.belt - 0.04, -hl + 0.95),
            ],
            Vec3::new(0.0, 0.5, 0.0),
            ink(slot::PAINT2, 0.9),
            body_ao,
        );
    }
    for (x, z) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        m.wheel(x * (hw - 0.13), z * (hl - c.axle), c.wheel, 0.2);
    }
    // Lamps: heads and indicators on the nose, tails and indicators on the tail (left is +x).
    let (front, rear) = (hl - 0.004, -hl + 0.004);
    for s in [-1.0f32, 1.0] {
        m.lamp(Vec3::new(s * (hw - 0.3), 0.47, front), (0.16, 0.05), 1.0, slot::HEAD);
        m.lamp(
            Vec3::new(s * (hw - 0.3), if hatch { 0.68 } else { 0.56 }, rear),
            (0.14, 0.07),
            -1.0,
            slot::TAIL,
        );
        let blink = if s > 0.0 { slot::BLINK_L } else { slot::BLINK_R };
        m.lamp(Vec3::new(s * (hw - 0.075), 0.47, front), (0.05, 0.04), 1.0, blink);
        m.lamp(Vec3::new(s * (hw - 0.09), if hatch { 0.68 } else { 0.56 }, rear), (0.05, 0.05), -1.0, blink);
    }
    driver(m, &cabin, hw - 0.1, hw - 0.24);
}

/// The driver's shape (head and shoulders, on the left: traffic keeps right), on the screen and
/// on the side glass beside them, just proud of the glass prism `cabin` (`hw` to `top_hw` across).
fn driver(m: &mut LifeMesh, cabin: &[(f32, f32); 4], hw: f32, top_hw: f32) {
    let ink = ink(slot::DRIVER, 0.0);
    let (y0, y1) = (cabin[0].1, cabin[1].1);
    let across = |y: f32| hw + (top_hw - hw) * (y - y0) / (y1 - y0);
    // On the screen: from its foot (z0, y0) up to its top (z1, y1).
    let (z0, z1) = (cabin[0].0, cabin[1].0);
    let screen = |u: f32| (z0 + (z1 - z0) * u, y0 + (y1 - y0) * u);
    let n = Vec3::new(0.0, z1 - z0, -(y1 - y0)).normalize() * -1.0;
    let mut on_screen = |u0: f32, u1: f32, x0: f32, x1: f32| {
        let (za, ya) = screen(u0);
        let (zb, yb) = screen(u1);
        let off = n * 0.006;
        let p = [
            Vec3::new(x0, ya, za) + off,
            Vec3::new(x1, ya, za) + off,
            Vec3::new(x1, yb, zb) + off,
            Vec3::new(x0, yb, zb) + off,
        ];
        let inside = p[0] - n;
        m.face(&p, inside, ink, |_, _| 1.0);
    };
    on_screen(0.3, 0.62, 0.12, 0.6);
    on_screen(0.62, 0.9, 0.26, 0.46);
    // On the side glass, behind the screen's foot.
    let zc = z0 - 0.75;
    let mut on_side = |ya: f32, yb: f32, za: f32, zb: f32| {
        let p = [
            Vec3::new(across(ya) + 0.006, ya, za),
            Vec3::new(across(ya) + 0.006, ya, zb),
            Vec3::new(across(yb) + 0.006, yb, zb),
            Vec3::new(across(yb) + 0.006, yb, za),
        ];
        m.face(&p, Vec3::new(0.0, 0.5 * (ya + yb), zc), ink, |_, _| 1.0);
    };
    let (ya, yb) = (y0 + 0.3 * (y1 - y0), y0 + 0.62 * (y1 - y0));
    on_side(ya, yb, zc - 0.22, zc + 0.24);
    on_side(yb, y0 + 0.9 * (y1 - y0), zc - 0.1, zc + 0.12);
}

/// A panel van.
fn van(m: &mut LifeMesh) {
    let (hl, hw) = (2.58, 0.97);
    let paint = ink(slot::PAINT, 0.1);
    let glass = ink(slot::GLASS, 0.3);
    let profile = [
        (-hl + 0.01, 0.3),
        (hl - 0.01, 0.3),
        (hl - 0.01, 0.8),
        (hl - 0.6, 1.1),
        (hl - 1.15, 1.95),
        (-hl + 0.06, 2.0),
        (-hl + 0.01, 1.92),
    ];
    let bevel = Ink { bevel: 1.0, ..paint };
    m.prism(&profile, hw, hw - 0.04, &[paint, paint, bevel, glass, paint, bevel, paint], paint, body_ao);
    // The cab's side windows and the roof's rack.
    for s in [-1.0f32, 1.0] {
        let x = s * (hw + 0.003);
        m.face(
            &[
                Vec3::new(x, 1.15, hl - 1.62),
                Vec3::new(x, 1.15, hl - 0.75),
                Vec3::new(x, 1.78, hl - 1.13),
                Vec3::new(x, 1.78, hl - 1.62),
            ],
            Vec3::new(0.0, 1.4, hl - 1.2),
            glass,
            |_, _| 1.0,
        );
        m.cuboid(
            Vec3::new(s * 0.6 - 0.03, 2.0, -hl + 0.4),
            Vec3::new(s * 0.6 + 0.03, 2.07, hl - 1.3),
            [ink(slot::TRIM, 0.7); 6],
            [true, false, false, false, false, false],
            |_, _| 1.0,
        );
    }
    let trim = ink(slot::TRIM, 0.7);
    for (z0, z1) in [(hl - 0.14, hl), (-hl, -hl + 0.14)] {
        m.cuboid(
            Vec3::new(-hw + 0.04, 0.26, z0),
            Vec3::new(hw - 0.04, 0.46, z1),
            [trim; 6],
            [true, false, false, false, false, false],
            body_ao,
        );
    }
    for (x, z) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        m.wheel(x * (hw - 0.15), z * (hl - 0.72), 0.34, 0.22);
    }
    let (front, rear) = (hl - 0.004, -hl + 0.004);
    for s in [-1.0f32, 1.0] {
        m.lamp(Vec3::new(s * (hw - 0.3), 0.64, front), (0.17, 0.06), 1.0, slot::HEAD);
        m.lamp(Vec3::new(s * (hw - 0.1), 0.98, rear), (0.07, 0.17), -1.0, slot::TAIL);
        let blink = if s > 0.0 { slot::BLINK_L } else { slot::BLINK_R };
        m.lamp(Vec3::new(s * (hw - 0.08), 0.64, front), (0.05, 0.05), 1.0, blink);
        m.lamp(Vec3::new(s * (hw - 0.1), 1.24, rear), (0.07, 0.05), -1.0, blink);
    }
    // The screen's lower part, beside the cab's side windows.
    let cab = [(hl - 0.6, 1.1), (hl - 1.066, 1.82), (hl - 1.6, 1.82), (hl - 1.6, 1.1)];
    driver(m, &cab, hw, hw);
}

/// The stand-up scooter (near, its rider is a figure of their own, on its deck 0.4 m up:
/// `life::Choice`).
fn scooter(m: &mut LifeMesh) {
    let paint = ink(slot::PAINT, 0.1);
    let trim = ink(slot::TRIM, 0.7);
    let none = [false; 6];
    m.cuboid(
        Vec3::new(-0.2, 0.25, -0.78),
        Vec3::new(0.2, 0.4, 0.6),
        [paint; 6],
        [true, false, false, false, false, false],
        body_ao,
    );
    m.cuboid(Vec3::new(-0.045, 0.3, 0.57), Vec3::new(0.045, 1.14, 0.67), [trim; 6], none, |_, _| 1.0);
    m.cuboid(Vec3::new(-0.3, 1.12, 0.58), Vec3::new(0.3, 1.18, 0.66), [trim; 6], none, |_, _| 1.0);
    m.face(
        &[
            Vec3::new(-0.16, 0.92, 0.68),
            Vec3::new(0.16, 0.92, 0.68),
            Vec3::new(0.16, 1.08, 0.7),
            Vec3::new(-0.16, 1.08, 0.7),
        ],
        Vec3::new(0.0, 1.0, 0.6),
        ink(slot::GLASS, 0.3),
        |_, _| 1.0,
    );
    m.wheel(0.0, 0.72, 0.22, 0.12);
    m.wheel(0.0, -0.72, 0.22, 0.12);
    m.lamp(Vec3::new(0.0, 0.84, 0.674), (0.05, 0.04), 1.0, slot::HEAD);
    m.lamp(Vec3::new(0.0, 0.33, -0.784), (0.12, 0.04), -1.0, slot::TAIL);
    m.lamp(Vec3::new(0.27, 1.15, 0.664), (0.03, 0.025), 1.0, slot::BLINK_L);
    m.lamp(Vec3::new(-0.27, 1.15, 0.664), (0.03, 0.025), 1.0, slot::BLINK_R);
}

/// Faces of a mid-level box: no bottom (and no top where `top` is false).
fn mid_box(m: &mut LifeMesh, min: Vec3, max: Vec3, slot: u8, top: bool) {
    m.cuboid(min, max, [ink(slot, 0.2); 6], [true, !top, false, false, false, false], body_ao);
}

fn car_mid(m: &mut LifeMesh, body: Body) {
    let c = car_shape(body);
    let (hl, hw) = (c.hl, c.hw);
    // The wheels as a dark band, the body over it, the cabin.
    m.cuboid(
        Vec3::new(-hw + 0.06, 0.0, -hl + 0.3),
        Vec3::new(hw - 0.06, 0.3, hl - 0.3),
        [ink(slot::TYRE, 0.0); 6],
        [true, true, false, false, false, false],
        |_, _| 1.0,
    );
    mid_box(m, Vec3::new(-hw, 0.22, -hl + 0.01), Vec3::new(hw, c.belt, hl - 0.01), slot::PAINT, true);
    mid_box(
        m,
        Vec3::new(-hw + 0.17, c.belt, -hl + 0.5 * (c.back.0 + c.back.1)),
        Vec3::new(hw - 0.17, c.roof, hl - 0.5 * (c.screen.0 + c.screen.1)),
        slot::GLASS,
        true,
    );
    let (front, rear) = (hl - 0.004, -hl + 0.004);
    for s in [-1.0f32, 1.0] {
        m.lamp(Vec3::new(s * (hw - 0.3), 0.47, front), (0.16, 0.05), 1.0, slot::HEAD);
        m.lamp(Vec3::new(s * (hw - 0.3), 0.6, rear), (0.14, 0.07), -1.0, slot::TAIL);
    }
    if body == Body::Taxi {
        for dir in [-1.0f32, 1.0] {
            m.lamp(Vec3::new(0.0, 0.5 * (c.roof + 1.55), -0.15 + dir * 0.2), (0.3, 0.04), dir, slot::SIGN);
        }
    }
}

fn van_mid(m: &mut LifeMesh) {
    let (hl, hw) = (2.58, 0.97);
    m.cuboid(
        Vec3::new(-hw + 0.06, 0.0, -hl + 0.35),
        Vec3::new(hw - 0.06, 0.34, hl - 0.35),
        [ink(slot::TYRE, 0.0); 6],
        [true, true, false, false, false, false],
        |_, _| 1.0,
    );
    mid_box(m, Vec3::new(-hw, 0.3, -hl + 0.01), Vec3::new(hw, 2.0, hl - 0.01), slot::PAINT, true);
    m.lamp(Vec3::new(0.0, 1.5, hl - 0.004), (hw - 0.12, 0.35), 1.0, slot::GLASS);
    let (front, rear) = (hl - 0.004, -hl + 0.004);
    for s in [-1.0f32, 1.0] {
        m.lamp(Vec3::new(s * (hw - 0.3), 0.64, front), (0.17, 0.06), 1.0, slot::HEAD);
        m.lamp(Vec3::new(s * (hw - 0.1), 0.98, rear), (0.07, 0.17), -1.0, slot::TAIL);
    }
}

fn scooter_mid(m: &mut LifeMesh) {
    mid_box(m, Vec3::new(-0.2, 0.0, -0.9), Vec3::new(0.2, DECK, 0.9), slot::PAINT, true);
    mid_box(m, Vec3::new(-0.05, DECK, 0.56), Vec3::new(0.05, 1.18, 0.66), slot::TRIM, false);
    // Its rider, as tall as a figure standing on the deck: a body and a head.
    let top = DECK + mid_figure(Gait::Stand, 0.0).bounds().1.y;
    mid_box(m, Vec3::new(-0.17, DECK, -0.1), Vec3::new(0.17, top - 0.22, 0.1), slot::DRIVER, false);
    mid_box(m, Vec3::new(-0.09, top - 0.22, -0.09), Vec3::new(0.09, top, 0.1), slot::DRIVER, true);
    m.lamp(Vec3::new(0.0, 0.84, 0.664), (0.05, 0.04), 1.0, slot::HEAD);
    m.lamp(Vec3::new(0.0, 0.3, -0.904), (0.12, 0.04), -1.0, slot::TAIL);
}

// ---- Civilians ----------------------------------------------------------------------------------

/// What a figure's doing, as baked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gait {
    Walk,
    Run,
    Stand,
    Sit,
}

impl Gait {
    pub const ALL: [Gait; 4] = [Gait::Walk, Gait::Run, Gait::Stand, Gait::Sit];

    pub fn index(self) -> usize {
        Gait::ALL.iter().position(|g| *g == self).unwrap()
    }

    /// What's baked for one of the city's people doing `pose`.
    pub fn of(pose: Pose) -> Gait {
        match pose {
            Pose::Walk => Gait::Walk,
            Pose::Run => Gait::Run,
            Pose::Stand => Gait::Stand,
            Pose::Sit => Gait::Sit,
        }
    }

    /// Its frames baked at each level.
    pub fn frames(self, near: bool) -> usize {
        match (self, near) {
            (Gait::Walk, true) => 16,
            (Gait::Walk, false) => 8,
            (Gait::Run, true) => 12,
            (Gait::Run, false) => 6,
            _ => 1,
        }
    }

    /// The pieces' turns and how far the hips are raised (m) at `u` (0..1) through its cycle.
    pub fn pose(self, u: f32) -> ([Quat; 12], f32) {
        let phase = u * TAU;
        match self {
            Gait::Walk => (figure::walk(phase, figure::WALK_SWING, false), 0.0),
            Gait::Run => (figure::pose(phase, 4.0, 0.0, true, true), 0.0),
            Gait::Stand => (figure::pose(0.0, 0.0, 0.0, true, false), 0.0),
            // The hips' underside on the bench's seat, leaning forward a little, hands on the knees.
            Gait::Sit => {
                let mut q = figure::seated(0.0);
                let mut set = |p: Piece, r: Quat| q[Piece::ALL.iter().position(|x| *x == p).unwrap()] = r;
                set(Piece::Chest, Quat::from_rotation_x(0.24));
                // Elbows in, within the seat's width.
                set(Piece::UpperArmL, Quat::from_rotation_x(-0.45) * Quat::from_rotation_z(0.02));
                set(Piece::UpperArmR, Quat::from_rotation_x(-0.45) * Quat::from_rotation_z(-0.02));
                (q, BENCH_HEIGHT + 0.1 - Piece::Hips.joint().y)
            }
        }
    }
}

/// The civilians' cuts: trousers and a top, or a skirt and short sleeves.
pub const CUTS: usize = 2;

/// How far back the hair's cap is tipped (rad): its brim 5 cm over the head's middle on the brow,
/// as far under it at the nape.
const HAIRLINE: f32 = 0.5;

/// Frames a level's bank holds for one cut: every gait's, one after another.
pub fn bank_frames(near: bool) -> usize {
    Gait::ALL.iter().map(|g| g.frames(near)).sum()
}

/// Where gait `g`'s frame for `stride` (0..1 through its cycle) is in a level's bank (one cut's).
pub fn bank_frame(g: Gait, near: bool, stride: f32) -> usize {
    let first: usize = Gait::ALL[..g.index()].iter().map(|g| g.frames(near)).sum();
    let n = g.frames(near);
    first + ((stride.rem_euclid(1.0) * n as f32) as usize).min(n - 1)
}

/// Every frame of a level's bank, in order: `(gait, u)` (near: by cut, the cuts one after another).
pub fn bank(near: bool) -> Vec<(Gait, f32)> {
    let mut out = Vec::new();
    for g in Gait::ALL {
        let n = g.frames(near);
        for f in 0..n {
            out.push((g, f as f32 / n as f32));
        }
    }
    out
}

/// A civilian piece's mesh, about its joint: `figure`'s pieces with a bare head and hair, no pack,
/// shoes, and clothes (`cut` 0: trousers and long sleeves; 1: a skirt to the knee and short
/// sleeves).
fn civilian_piece(piece: Piece, cut: usize, seated: bool) -> LifeMesh {
    let mut m = LifeMesh::default();
    let skirt = cut == 1;
    let none = [false; 6];
    let one = |s: u8| [ink(s, 0.0); 6];
    let flat = |_: Vec3, _: Vec3| 1.0;
    match piece {
        Piece::Hips => {
            m.cuboid(Vec3::new(-0.15, -0.1, -0.1), Vec3::new(0.15, 0.1, 0.1), one(slot::BOTTOM), none, flat);
            if skirt && seated {
                // Over the lap, to the knees.
                m.cuboid(
                    Vec3::new(-0.19, -0.1, -0.11),
                    Vec3::new(0.19, 0.03, 0.44),
                    one(slot::BOTTOM),
                    none,
                    flat,
                );
            } else if skirt {
                m.skirt(0.06, 0.17, -0.44, 0.25, 8, ink(slot::BOTTOM, 0.0));
            }
        }
        Piece::Chest => {
            m.cuboid(Vec3::new(-0.17, 0.0, -0.1), Vec3::new(0.17, 0.36, 0.1), one(slot::TOP), none, flat);
            m.cuboid(
                Vec3::new(-0.045, 0.34, -0.045),
                Vec3::new(0.045, 0.44, 0.045),
                one(slot::SKIN),
                none,
                flat,
            );
        }
        Piece::Head => {
            m.ball(Vec3::new(0.0, 0.11, 0.01), 0.105, 6, 6, 10, ink(slot::SKIN, 0.0));
            // The hair: a cap to its brim, tipped back so its line is high on the brow, over the
            // ears, and down to the nape (a block at the back read as a visor side on).
            let mut cap = LifeMesh::default();
            cap.ball(Vec3::ZERO, 0.113, 6, 3, 10, ink(slot::HAIR, 0.0));
            m.append(&cap, Vec3::new(0.0, 0.118, -0.004), Quat::from_rotation_x(-HAIRLINE));
        }
        Piece::Pack => {}
        Piece::UpperArmL | Piece::UpperArmR => m.limb(0.29, 0.055, 0.045, 6, ink(slot::TOP, 0.0)),
        Piece::ForearmL | Piece::ForearmR => {
            m.limb(0.22, 0.045, 0.038, 6, ink(if skirt { slot::SKIN } else { slot::TOP }, 0.0));
            m.cuboid(
                Vec3::new(-0.035, -0.31, -0.025),
                Vec3::new(0.035, -0.22, 0.035),
                one(slot::SKIN),
                none,
                flat,
            );
        }
        Piece::ThighL | Piece::ThighR => {
            m.limb(0.44, 0.08, 0.062, 6, ink(if skirt { slot::SKIN } else { slot::BOTTOM }, 0.0))
        }
        Piece::ShinL | Piece::ShinR => {
            m.limb(0.42, 0.058, 0.045, 6, ink(if skirt { slot::SKIN } else { slot::BOTTOM }, 0.0));
            m.cuboid(
                Vec3::new(-0.05, -0.5, -0.055),
                Vec3::new(0.05, -0.42, 0.13),
                one(slot::SHOES),
                none,
                flat,
            );
        }
    }
    m
}

/// A civilian of cut `cut` at `u` (0..1) through gait `g`'s cycle, as one mesh.
pub fn civilian_mesh(cut: usize, g: Gait, u: f32) -> LifeMesh {
    let (turns, lift) = g.pose(u);
    let at = figure::joints(&turns);
    let mut m = LifeMesh::default();
    for (i, p) in Piece::ALL.iter().enumerate() {
        let (o, r) = at[i];
        m.append(&civilian_piece(*p, cut, g == Gait::Sit), o + Vec3::Y * lift, r);
    }
    m
}

/// A civilian far off: boxes for the head, the body, the arms, the thighs and the shins, on the same
/// joints, without the faces they hide.
pub fn mid_figure(g: Gait, u: f32) -> LifeMesh {
    let (turns, lift) = g.pose(u);
    let at = figure::joints(&turns);
    let mut m = LifeMesh::default();
    let flat = |_: Vec3, _: Vec3| 1.0;
    let mut part = |p: Piece, min: Vec3, max: Vec3, inks: [Ink; 6], skip: [bool; 6]| {
        let (o, r) = at[Piece::ALL.iter().position(|x| *x == p).unwrap()];
        let mut b = LifeMesh::default();
        b.cuboid(min, max, inks, skip, flat);
        m.append(&b, o + Vec3::Y * lift, r);
    };
    let (hair, skin, top, bottom) =
        (ink(slot::HAIR, 0.0), ink(slot::SKIN, 0.0), ink(slot::TOP, 0.0), ink(slot::BOTTOM, 0.0));
    // Each box's hidden faces: its bottom (−y), or its top (+y) where it meets its parent.
    let (no_bottom, no_top) =
        ([true, false, false, false, false, false], [false, true, false, false, false, false]);
    part(
        Piece::Head,
        Vec3::new(-0.085, 0.0, -0.09),
        Vec3::new(0.085, 0.21, 0.1),
        [hair, hair, hair, hair, hair, skin],
        no_bottom,
    );
    part(Piece::Chest, Vec3::new(-0.17, -0.2, -0.1), Vec3::new(0.17, 0.44, 0.1), [top; 6], no_bottom);
    for (thigh, shin, arm) in
        [(Piece::ThighL, Piece::ShinL, Piece::UpperArmL), (Piece::ThighR, Piece::ShinR, Piece::UpperArmR)]
    {
        part(thigh, Vec3::new(-0.075, -0.46, -0.075), Vec3::new(0.075, 0.0, 0.08), [bottom; 6], no_top);
        part(shin, Vec3::new(-0.06, -0.5, -0.06), Vec3::new(0.06, -0.02, 0.1), [bottom; 6], no_top);
        part(arm, Vec3::new(-0.045, -0.56, -0.045), Vec3::new(0.045, 0.0, 0.045), [top; 6], no_top);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{Rect, Stage};
    use bc_sim::colony::frame::STRIP_WIDTH;
    use bc_sim::colony::walkers::{self, RADIUS};

    fn all_vehicles() -> Vec<(Body, bool, LifeMesh)> {
        Body::ALL.iter().flat_map(|b| [true, false].map(|near| (*b, near, vehicle_mesh(*b, near)))).collect()
    }

    fn all_figures() -> Vec<(String, LifeMesh)> {
        let mut out = Vec::new();
        for (g, u) in bank(true) {
            for cut in 0..CUTS {
                out.push((format!("{g:?} {u} cut {cut}"), civilian_mesh(cut, g, u)));
            }
        }
        for (g, u) in bank(false) {
            out.push((format!("{g:?} {u} mid"), mid_figure(g, u)));
        }
        out
    }

    #[test]
    fn vehicles_fit_their_footprints() {
        for (body, near, m) in all_vehicles() {
            let (l, w, h) = body.kind().size();
            let (lo, hi) = m.bounds();
            let e = 1e-3;
            assert!(
                lo.x >= -0.5 * w - e && hi.x <= 0.5 * w + e,
                "{body:?} {near}: {lo:?} {hi:?} in {w} wide"
            );
            assert!(
                lo.z >= -0.5 * l - e && hi.z <= 0.5 * l + e,
                "{body:?} {near}: {lo:?} {hi:?} in {l} long"
            );
            // A far scooter carries its rider (a figure's height on its deck).
            let h = if body == Body::Scooter && !near { DECK + 1.9 } else { h };
            assert!(hi.y <= h + e, "{body:?} {near}: {hi:?} under {h}");
            assert!(lo.y.abs() < e, "{body:?} {near}: wheels on the ground, {lo:?}");
        }
    }

    #[test]
    fn vehicles_and_figures_are_light() {
        for (body, near, m) in all_vehicles() {
            let most = if near { 300 } else { 40 };
            assert!(m.triangles() <= most, "{body:?} {near}: {} triangles", m.triangles());
        }
        for (name, m) in all_figures() {
            let most = if name.ends_with("mid") { 80 } else { 800 };
            assert!(m.triangles() <= most, "{name}: {} triangles", m.triangles());
        }
        assert_eq!(bank_frames(true), 30);
        assert_eq!(bank_frames(false), 16);
        assert_eq!(bank(true).len() * CUTS + bank(false).len(), 76);
    }

    #[test]
    fn lamps_face_their_ends() {
        for (body, near, m) in all_vehicles() {
            let (l, _, _) = body.kind().size();
            let mut heads = 0;
            for i in 0..m.positions.len() {
                let (n, z) = (Vec3::from(m.normals[i]), m.positions[i][2]);
                match m.slot(i) {
                    slot::HEAD => {
                        heads += 1;
                        assert!(
                            n.z > 0.95 && z > 0.3 * l,
                            "{body:?} {near}: a head lamp at {z} facing {n:?}"
                        );
                    }
                    slot::TAIL => {
                        assert!(n.z < -0.95 && z < -0.3 * l, "{body:?} {near}: a tail at {z} facing {n:?}")
                    }
                    slot::BLINK_L | slot::BLINK_R => {
                        let left = m.slot(i) == slot::BLINK_L;
                        assert!(
                            n.z.abs() > 0.95 && (m.positions[i][0] > 0.0) == left,
                            "{body:?} {near}: indicator"
                        );
                    }
                    _ => {}
                }
            }
            assert!(heads >= 4, "{body:?} {near}: its head lamps");
        }
    }

    #[test]
    fn every_vertex_has_a_known_slot_and_faces_out() {
        let check = |name: &str, m: &LifeMesh| {
            assert_eq!(m.positions.len(), m.normals.len());
            assert_eq!(m.positions.len(), m.colors.len());
            for (i, c) in m.colors.iter().enumerate() {
                let s = c[0] * 255.0;
                assert!((s - s.round()).abs() < 1e-3 && slot::ALL.contains(&m.slot(i)), "{name}: slot {s}");
                assert!((0.0..=1.0).contains(&c[3]), "{name}: occlusion {}", c[3]);
            }
            for t in m.indices.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize]));
                let n = (b - a).cross(c - a);
                // A triangle's slot is its corners' (the shader reads it interpolated).
                assert!(
                    m.slot(t[0] as usize) == m.slot(t[1] as usize)
                        && m.slot(t[1] as usize) == m.slot(t[2] as usize)
                );
                if n.length() > 1e-7 {
                    let vn = Vec3::from(m.normals[t[0] as usize]);
                    assert!(n.normalize().dot(vn) > 0.3, "{name}: a face turned in");
                }
            }
        };
        for (body, near, m) in all_vehicles() {
            check(&format!("{body:?} {near}"), &m);
        }
        for (name, m) in all_figures() {
            check(&name, &m);
        }
    }

    /// The hair's line is high on the brow and low at the nape, and nothing of it stands in front
    /// of the face or off the head.
    #[test]
    fn hair_sits_on_the_head() {
        let head = civilian_piece(Piece::Head, 0, false);
        let hair: Vec<Vec3> = (0..head.positions.len())
            .filter(|i| head.slot(*i) == slot::HAIR)
            .map(|i| Vec3::from(head.positions[i]))
            .collect();
        let brim = |front: bool| {
            hair.iter()
                .filter(|p| p.x.abs() < 0.02 && (p.z > 0.0) == front)
                .map(|p| p.y)
                .fold(f32::MAX, f32::min)
        };
        let centre = 0.118;
        // All of it on the cap's round: no flat-sided block to stand off the head.
        let cap = Vec3::new(0.0, centre, -0.004);
        for p in &hair {
            assert!((p.distance(cap) - 0.113).abs() < 1e-3, "hair off the cap at {p:?}");
        }
        assert!(brim(true) > centre + 0.04, "brow at {}", brim(true));
        assert!(brim(false) < centre - 0.04, "nape at {}", brim(false));
        let (lo, hi) = head.bounds();
        let skin = |i: usize| head.slot(i) == slot::SKIN;
        let face = (0..head.positions.len())
            .filter(|i| skin(*i))
            .map(|i| head.positions[i][2])
            .fold(f32::MIN, f32::max);
        assert!(hi.z <= face + 1e-4, "the face is in front: {hi:?}");
        assert!(lo.z > -0.125 && hi.x < 0.12 && lo.x > -0.12, "a head's size: {lo:?} {hi:?}");
    }

    #[test]
    fn a_civilian_fits_the_walkers_radius() {
        // The widest build: as wide as baked, 8% taller (`life::build`).
        for (name, m) in all_figures() {
            if name.starts_with("Sit") {
                continue;
            }
            let (lo, hi) = m.bounds();
            assert!(lo.x > -RADIUS && hi.x < RADIUS, "{name}: {lo:?} {hi:?}");
            // A runner is off the ground a moment each step.
            let lift = if name.starts_with("Run") { 0.06 } else { 0.02 };
            assert!(lo.y > -0.08 && lo.y < lift, "{name}: feet on the ground, {lo:?}");
            assert!(hi.y * 1.08 < 1.95, "{name}: {hi:?}");
        }
    }

    #[test]
    fn a_sitter_fits_the_walkers_boxes() {
        // Somebody on one of the avenue's benches at noon.
        let mid = STRIP_WIDTH * 0.5;
        let area = Rect::new(mid + 20.0, mid + 30.0, -14_400.0, -13_600.0);
        let mut sitter = None;
        walkers::each_walker(0, &area, Stage(0), 14_400, 0.0, |w| {
            if w.pose == Pose::Sit {
                sitter = Some(*w);
            }
            sitter.is_some()
        });
        let w = sitter.expect("somebody sitting on the avenue at noon");
        let boxes = walkers::sitting(&w);
        let (sy, cy) = w.yaw.sin_cos();
        // In the sitter's frame: along the bench (its +x) and forward (its +z), from the seat.
        let frame = |s: f32, x: f32| ((s - w.s) * sy + (x - w.x) * cy, -(s - w.s) * cy + (x - w.x) * sy);
        let (mut f0, mut f1) = (f32::MAX, f32::MIN);
        for b in &boxes {
            for (s, x) in [
                (b.rect.s0, b.rect.x0),
                (b.rect.s1, b.rect.x0),
                (b.rect.s0, b.rect.x1),
                (b.rect.s1, b.rect.x1),
            ] {
                let (_, f) = frame(s, x);
                (f0, f1) = (f0.min(f), f1.max(f));
            }
        }
        let top = boxes.iter().map(|b| b.h1).fold(f32::MIN, f32::max);
        for cut in 0..CUTS {
            for m in [civilian_mesh(cut, Gait::Sit, 0.0), mid_figure(Gait::Sit, 0.0)] {
                for p in &m.positions {
                    let (s, x, h) = (w.s + p[0] * sy - p[2] * cy, w.x + p[0] * cy + p[2] * sy, w.h + p[1]);
                    let (along, ahead) = frame(s, x);
                    // From the seat to the shins' box's far side, over and in front of the bench; along it,
                    // within the walkers' radius (everybody's middle is twice that from anybody else's).
                    assert!(
                        ahead > f0 - 0.01 && ahead < f1 + 0.01,
                        "cut {cut}: {ahead} m ahead, not in {f0}..{f1}"
                    );
                    assert!(along.abs() < RADIUS, "cut {cut}: {along} m along the bench");
                    assert!(h > w.h - 0.06 && h < top, "cut {cut}: {} m up", h - w.h);
                }
            }
        }
    }
}
