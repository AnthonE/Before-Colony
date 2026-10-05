//! A pilot on foot, as others see them: a figure in a flight suit (helmet, visor, a life-support
//! pack), in rigid pieces that turn at their joints, and how they swing as the pilot walks, runs,
//! stands or jumps. Feet at the origin, facing +z (the walker's frame), 1.75 m tall: inside the
//! walker's 0.6 × 1.8 m box.

use glam::{Quat, Vec3};

/// What a piece's surface is: the suit's own colour, its white trim, or the dark visor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Suit = 0,
    Trim = 1,
    Visor = 2,
}

/// The pieces, each turning about its joint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Hips,
    Chest,
    Head,
    Pack,
    UpperArmL,
    ForearmL,
    UpperArmR,
    ForearmR,
    ThighL,
    ShinL,
    ThighR,
    ShinR,
}

impl Piece {
    pub const ALL: [Piece; 12] = [
        Piece::Hips,
        Piece::Chest,
        Piece::Head,
        Piece::Pack,
        Piece::UpperArmL,
        Piece::ForearmL,
        Piece::UpperArmR,
        Piece::ForearmR,
        Piece::ThighL,
        Piece::ShinL,
        Piece::ThighR,
        Piece::ShinR,
    ];

    /// The piece it hangs from (`None`: the figure itself).
    pub fn parent(self) -> Option<Piece> {
        match self {
            Piece::Hips => None,
            Piece::Chest | Piece::ThighL | Piece::ThighR => Some(Piece::Hips),
            Piece::Head | Piece::Pack | Piece::UpperArmL | Piece::UpperArmR => Some(Piece::Chest),
            Piece::ForearmL => Some(Piece::UpperArmL),
            Piece::ForearmR => Some(Piece::UpperArmR),
            Piece::ShinL => Some(Piece::ThighL),
            Piece::ShinR => Some(Piece::ThighR),
        }
    }

    /// Its joint at rest, relative to its parent's joint (the hips' to the feet), m.
    pub fn joint(self) -> Vec3 {
        match self {
            Piece::Hips => Vec3::new(0.0, 0.94, 0.0),
            Piece::Chest => Vec3::new(0.0, 0.1, 0.0),
            Piece::Head => Vec3::new(0.0, 0.46, 0.0),
            Piece::Pack => Vec3::new(0.0, 0.2, -0.17),
            Piece::UpperArmL => Vec3::new(0.21, 0.4, 0.0),
            Piece::UpperArmR => Vec3::new(-0.21, 0.4, 0.0),
            Piece::ForearmL | Piece::ForearmR => Vec3::new(0.0, -0.29, 0.0),
            Piece::ThighL => Vec3::new(0.1, -0.02, 0.0),
            Piece::ThighR => Vec3::new(-0.1, -0.02, 0.0),
            Piece::ShinL | Piece::ShinR => Vec3::new(0.0, -0.44, 0.0),
        }
    }
}

/// A piece's mesh, relative to its joint.
#[derive(Clone, Debug, Default)]
pub struct PieceMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Its paint, per vertex.
    pub paint: Vec<Paint>,
}

impl PieceMesh {
    fn quad(&mut self, p: [Vec3; 4], paint: Paint) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let base = self.positions.len() as u32;
        for v in p {
            self.positions.push(v.to_array());
            self.normals.push(n.to_array());
            self.paint.push(paint);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A box from `min` to `max`, its faces out.
    fn cuboid(&mut self, min: Vec3, max: Vec3, paint: Paint) {
        let c =
            |x: usize, y: usize, z: usize| Vec3::new([min.x, max.x][x], [min.y, max.y][y], [min.z, max.z][z]);
        self.quad([c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)], paint);
        self.quad([c(0, 0, 1), c(0, 1, 1), c(0, 1, 0), c(0, 0, 0)], paint);
        self.quad([c(0, 1, 0), c(0, 1, 1), c(1, 1, 1), c(1, 1, 0)], paint);
        self.quad([c(0, 0, 1), c(0, 0, 0), c(1, 0, 0), c(1, 0, 1)], paint);
        self.quad([c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)], paint);
        self.quad([c(1, 0, 0), c(0, 0, 0), c(0, 1, 0), c(1, 1, 0)], paint);
    }

    /// A tapered limb hanging down `length` from the joint: eight sides, `r0` at the top and `r1`
    /// at the bottom (an ellipse `depth` as deep as it's wide).
    fn limb(&mut self, length: f32, r0: f32, r1: f32, depth: f32, paint: Paint) {
        const SIDES: usize = 8;
        let ring = |y: f32, r: f32, k: usize| {
            let a = k as f32 / SIDES as f32 * std::f32::consts::TAU;
            Vec3::new(a.cos() * r, y, a.sin() * r * depth)
        };
        for k in 0..SIDES {
            let (a, b) = (k, (k + 1) % SIDES);
            self.quad(
                [ring(0.0, r0, a), ring(0.0, r0, b), ring(-length, r1, b), ring(-length, r1, a)],
                paint,
            );
        }
        // Caps.
        let base = self.positions.len() as u32;
        for (y, r, n) in [(0.0, r0, Vec3::Y), (-length, r1, -Vec3::Y)] {
            for k in 0..SIDES {
                self.positions.push(ring(y, r, k).to_array());
                self.normals.push(n.to_array());
                self.paint.push(paint);
            }
        }
        for k in 1..SIDES as u32 - 1 {
            self.indices.extend_from_slice(&[base, base + k + 1, base + k]);
            let b = base + SIDES as u32;
            self.indices.extend_from_slice(&[b, b + k, b + k + 1]);
        }
    }

    /// A ball of radius `r` about `centre`, `rings` × `segs`, its front band (`visor`: between
    /// these heights, facing +z) painted dark.
    fn ball(&mut self, centre: Vec3, r: f32, visor: (f32, f32), paint: Paint) {
        let (rings, segs) = (7usize, 12usize);
        let base = self.positions.len() as u32;
        for i in 0..=rings {
            let v = i as f32 / rings as f32 * std::f32::consts::PI;
            for j in 0..=segs {
                let u = j as f32 / segs as f32 * std::f32::consts::TAU;
                let n = Vec3::new(v.sin() * u.sin(), v.cos(), v.sin() * u.cos());
                self.positions.push((centre + n * r).to_array());
                self.normals.push(n.to_array());
                let front = n.z > 0.45 && (visor.0..visor.1).contains(&n.y);
                self.paint.push(if front { Paint::Visor } else { paint });
            }
        }
        let w = segs as u32 + 1;
        for i in 0..rings as u32 {
            for j in 0..segs as u32 {
                let (a, b) = (base + i * w + j, base + (i + 1) * w + j);
                self.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }

    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Each piece's mesh.
pub fn mesh(piece: Piece) -> PieceMesh {
    let mut m = PieceMesh::default();
    match piece {
        Piece::Hips => {
            m.cuboid(Vec3::new(-0.17, -0.1, -0.11), Vec3::new(0.17, 0.12, 0.11), Paint::Suit);
            // The belt.
            m.cuboid(Vec3::new(-0.175, 0.05, -0.115), Vec3::new(0.175, 0.1, 0.115), Paint::Trim);
        }
        Piece::Chest => {
            m.cuboid(Vec3::new(-0.19, 0.0, -0.12), Vec3::new(0.19, 0.36, 0.12), Paint::Suit);
            // The collar ring and a chest panel.
            m.cuboid(Vec3::new(-0.12, 0.34, -0.09), Vec3::new(0.12, 0.42, 0.09), Paint::Trim);
            m.cuboid(Vec3::new(-0.1, 0.16, 0.12), Vec3::new(0.1, 0.3, 0.135), Paint::Trim);
        }
        Piece::Head => {
            m.ball(Vec3::new(0.0, 0.14, 0.01), 0.135, (-0.35, 0.45), Paint::Trim);
        }
        Piece::Pack => {
            m.cuboid(Vec3::new(-0.15, -0.12, -0.08), Vec3::new(0.15, 0.2, 0.05), Paint::Trim);
        }
        Piece::UpperArmL | Piece::UpperArmR => m.limb(0.29, 0.065, 0.055, 1.0, Paint::Suit),
        Piece::ForearmL | Piece::ForearmR => {
            m.limb(0.22, 0.055, 0.045, 1.0, Paint::Suit);
            // The glove.
            m.cuboid(Vec3::new(-0.04, -0.31, -0.03), Vec3::new(0.04, -0.21, 0.04), Paint::Trim);
        }
        Piece::ThighL | Piece::ThighR => m.limb(0.44, 0.09, 0.07, 1.0, Paint::Suit),
        Piece::ShinL | Piece::ShinR => {
            m.limb(0.36, 0.065, 0.055, 1.0, Paint::Suit);
            // The boot, to the ground.
            m.cuboid(Vec3::new(-0.06, -0.5, -0.06), Vec3::new(0.06, -0.34, 0.16), Paint::Trim);
        }
    }
    m
}

/// How the pieces stand this moment: each one's turn about its joint, by [`Piece::ALL`]'s order.
/// `phase` is the stride's (radians, one step each π), `speed` over the ground (m/s), `pitch` where
/// they look.
pub fn pose(phase: f32, speed: f32, pitch: f32, grounded: bool, running: bool) -> [Quat; 12] {
    // How far the thighs swing: running, more the faster; walking, what a walk's stride
    // (`stride_phase`'s 1.5 m) wants once under way, so the planted foot keeps its place.
    let swing = if running {
        0.75 * (speed / 7.0).clamp(0.0, 1.0)
    } else {
        WALK_SWING * (speed / 1.2).clamp(0.0, 1.0)
    };
    let mut q = walk(phase, swing, running);
    let x = |a: f32| Quat::from_rotation_x(a);
    if !grounded {
        // In the air: knees up.
        set(&mut q, Piece::ThighL, x(-0.6));
        set(&mut q, Piece::ThighR, x(-0.3));
        set(&mut q, Piece::ShinL, x(0.9));
        set(&mut q, Piece::ShinR, x(0.7));
    }
    // The head follows the look.
    set(&mut q, Piece::Head, x(-pitch.clamp(-0.8, 0.8) * 0.6));
    q
}

/// A walk's thighs swing this far either way (rad): about right for its 1.5 m stride.
pub const WALK_SWING: f32 = 0.29;

/// Striding on the ground: the legs, arms and chest at `phase` (radians, one step each π) of a
/// stride whose thighs swing `swing` rad either way ([`WALK_SWING`] walking, 0.75 at a sprint).
/// A leg swings forward (−x turns it to +z) as its other swings back, its knee bending as it
/// goes, so the straight leg is the one the body passes over and its foot keeps its place.
pub fn walk(phase: f32, swing: f32, running: bool) -> [Quat; 12] {
    let mut q = [Quat::IDENTITY; 12];
    let stride = (swing / if running { 0.75 } else { WALK_SWING }).clamp(0.0, 1.0);
    let s = phase.sin();
    let x = |a: f32| Quat::from_rotation_x(a);
    set(&mut q, Piece::ThighL, x(-swing * s));
    set(&mut q, Piece::ThighR, x(swing * s));
    // The knee bends while its thigh swings forward (cos(phase) > 0 for the left), most halfway.
    let knee = |t: f32| (0.15 + 1.5 * swing * t.max(0.0)).min(1.4);
    set(&mut q, Piece::ShinL, x(knee(phase.cos())));
    set(&mut q, Piece::ShinR, x(knee(-phase.cos())));
    // Arms against the legs; bent more running.
    let arm = 0.8 * swing;
    let elbow = if running { -1.3 } else { -0.25 - 0.3 * stride };
    set(&mut q, Piece::UpperArmL, x(arm * s) * Quat::from_rotation_z(0.08));
    set(&mut q, Piece::UpperArmR, x(-arm * s) * Quat::from_rotation_z(-0.08));
    set(&mut q, Piece::ForearmL, x(elbow));
    set(&mut q, Piece::ForearmR, x(elbow));
    // Leaning into a run.
    set(&mut q, Piece::Chest, x(if running { 0.18 } else { 0.04 } * stride));
    q
}

/// Sets piece `p`'s turn in a pose.
fn set(q: &mut [Quat; 12], p: Piece, r: Quat) {
    q[Piece::ALL.iter().position(|x| *x == p).unwrap()] = r;
}

/// Sitting, the hips are this much lower than standing, m: on a bench's seat.
pub const SEAT_DROP: f32 = 0.48;

/// Sitting on a seat: the thighs out level, the shins hanging, the hands on the knees, the head
/// with the look (by [`Piece::ALL`]'s order, as [`pose`]).
pub fn seated(pitch: f32) -> [Quat; 12] {
    let mut q = [Quat::IDENTITY; 12];
    let x = |a: f32| Quat::from_rotation_x(a);
    set(&mut q, Piece::ThighL, x(-1.45) * Quat::from_rotation_z(0.06));
    set(&mut q, Piece::ThighR, x(-1.45) * Quat::from_rotation_z(-0.06));
    set(&mut q, Piece::ShinL, x(1.45));
    set(&mut q, Piece::ShinR, x(1.45));
    set(&mut q, Piece::UpperArmL, x(-0.45) * Quat::from_rotation_z(0.1));
    set(&mut q, Piece::UpperArmR, x(-0.45) * Quat::from_rotation_z(-0.1));
    set(&mut q, Piece::ForearmL, x(-0.7));
    set(&mut q, Piece::ForearmR, x(-0.7));
    set(&mut q, Piece::Chest, x(0.06));
    set(&mut q, Piece::Head, x(-pitch.clamp(-0.8, 0.8) * 0.6));
    q
}

/// The stride's phase moves on by this much for `dist` metres walked (two steps a cycle).
pub fn stride_phase(dist: f32, running: bool) -> f32 {
    dist / if running { 2.6 } else { 1.5 } * std::f32::consts::TAU
}

/// Where every piece's joint is and how it's turned, in the figure's frame, for `turns` ([`pose`]).
pub fn joints(turns: &[Quat; 12]) -> [(Vec3, Quat); 12] {
    let mut out = [(Vec3::ZERO, Quat::IDENTITY); 12];
    for (i, p) in Piece::ALL.iter().enumerate() {
        let (at, rot) = match p.parent() {
            None => (p.joint(), turns[i]),
            Some(parent) => {
                let j = Piece::ALL.iter().position(|x| *x == parent).unwrap();
                let (pa, pr) = out[j];
                (pa + pr * p.joint(), pr * turns[i])
            }
        };
        out[i] = (at, rot);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_is_light_and_fits_the_walker() {
        let tris: usize = Piece::ALL.iter().map(|p| mesh(*p).triangles()).sum();
        assert!(tris < 1_500, "{tris} triangles");
        let at = joints(&pose(0.0, 0.0, 0.0, true, false));
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (i, p) in Piece::ALL.iter().enumerate() {
            let (o, r) = at[i];
            for v in mesh(*p).positions {
                let w = o + r * Vec3::from(v);
                lo = lo.min(w);
                hi = hi.max(w);
            }
        }
        assert!(lo.y.abs() < 0.05, "feet on the ground: {lo:?}");
        assert!((1.7..1.8).contains(&hi.y), "{hi:?} tall");
        assert!(lo.x > -0.3 && hi.x < 0.3 && lo.z > -0.3 && hi.z < 0.3, "{lo:?} {hi:?}");
        // The visor faces the way they look.
        let head = mesh(Piece::Head);
        let visor: Vec<_> =
            head.positions.iter().zip(&head.paint).filter(|(_, p)| **p == Paint::Visor).collect();
        assert!(!visor.is_empty() && visor.iter().all(|(v, _)| v[2] > 0.0));
    }

    #[test]
    fn walking_swings_the_legs_and_standing_still_doesnt() {
        let still = pose(1.0, 0.0, 0.0, true, false);
        let walk = pose(std::f32::consts::FRAC_PI_2, 4.2, 0.0, true, false);
        let thigh = |q: &[Quat; 12], p: Piece| q[Piece::ALL.iter().position(|x| *x == p).unwrap()];
        assert!(thigh(&still, Piece::ThighL).angle_between(Quat::IDENTITY) < 1e-3);
        let (l, r) = (thigh(&walk, Piece::ThighL), thigh(&walk, Piece::ThighR));
        // Forward and back by the same, the left foot ahead.
        assert!((l.angle_between(Quat::IDENTITY) - r.angle_between(Quat::IDENTITY)).abs() < 1e-4);
        assert!((l * Vec3::NEG_Y).z > 0.2 && (r * Vec3::NEG_Y).z < -0.2);
        assert!(stride_phase(1.5, false) > stride_phase(1.5, true));
    }

    /// Each boot's sole's corners, in the figure's frame, for `turns`.
    fn soles(turns: &[Quat; 12]) -> [[Vec3; 4]; 2] {
        let at = joints(turns);
        [Piece::ShinL, Piece::ShinR].map(|p| {
            let (o, r) = at[Piece::ALL.iter().position(|x| *x == p).unwrap()];
            [(-0.06, -0.06), (0.06, -0.06), (-0.06, 0.16), (0.06, 0.16)]
                .map(|(x, z)| o + r * Vec3::new(x, -0.5, z))
        })
    }

    /// How far the lower sole's lowest point moves over the ground while it's the lower one, as a
    /// share of the body's travel, over a stride of `cycle` metres (0: planted; 1: dragged along;
    /// more: sliding on ahead of the body, a moonwalk).
    fn slide(cycle: f32, turns: impl Fn(f32) -> [Quat; 12]) -> f32 {
        let n = 2_000;
        let step = cycle / n as f32;
        let mut moved = 0.0;
        for i in 0..n {
            let at = |i: usize| soles(&turns(i as f32 / n as f32 * std::f32::consts::TAU));
            let (p, q) = (at(i), at(i + 1));
            let lowest = |s: &[Vec3; 4]| (0..4).min_by(|a, b| s[*a].y.total_cmp(&s[*b].y)).unwrap();
            let (l, r) = (lowest(&p[0]), lowest(&p[1]));
            let (side, k) = if p[0][l].y <= p[1][r].y { (0, l) } else { (1, r) };
            // The figure moves on by `step` along +z, the sole by that and its own motion.
            let d = q[side][k] - p[side][k];
            moved += Vec3::new(d.x, 0.0, step + d.z).length();
        }
        moved / cycle
    }

    #[test]
    fn the_planted_foot_stays_put() {
        // The civilians' walk, baked for a 1.55 m stride; a pilot strolling at 1.4 m/s
        // (`stride_phase`'s 1.5 m stride). Measured: 0.36 both (1.15 and 1.08 when the knee bent
        // on the back swing). The rest is the swing's sinusoid and the hips' constant height.
        let civilian = slide(1.55, |a| walk(a, WALK_SWING, false));
        let stroll = slide(1.5, |a| pose(a, 1.4, 0.0, true, false));
        assert!(civilian < 0.4, "the civilian's planted foot slides {civilian:.2} of the body's travel");
        assert!(stroll < 0.5, "a strolling pilot's planted foot slides {stroll:.2} of the body's travel");
    }
}
