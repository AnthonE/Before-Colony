//! The sector's landmarks: bodies big enough to land on, walk and hide on, that aren't field rocks.
//!
//! Each is compiled-in content, the same on the server and every client, so a landmark's shape and
//! motion never travel on the wire: its pose at any tick is [`landmark_pose`](crate::bodies::landmark_pose),
//! a closed form in the integer tick. Changing anything here changes the game every client
//! simulates, so it bumps [`LANDMARKS_VERSION`] with the protocol version.
//!
//! - **MO-II**, the resource satellite serving the colony's dock: a 400 m core with a module at
//!   each end, four pylons and a mast. It rolls about its long axis every 320 s and keeps station
//!   on a 200 m circle every 30 min, so its surface never moves faster than 2.87 m/s. The Aft Well,
//!   a bowl in the aft module's end face on the spin axis, is a hide spot.
//! - **Hermit**, a big asteroid 1.8 km long: static, and it can't be mined or broken. A crater bowl
//!   at each of three poles is a hide spot.
//! - **The docking hub** off the colony's −X end cap ([`DOCKING_HUB`]): its spire and the modules
//!   stacked on it, turning with the colony (`colony::hub`). Near its middle its end face moves
//!   slowly enough to land on, and a suit standing on the deck hatch there docks. (The colony draws
//!   it; it's a landmark so that it's solid, and walked like any other.)
//!
//! Hide spots are bowls cut from the shape (exact spheres), so their rims are real cover.

use glam::{Quat, Vec3};

use super::names;
use crate::bodies::{Base, Prim, Shape, SphereCut};
use crate::colony::hub::{SPIRE_RADIUS, SPIRE_TIERS};
use crate::content::salvage::DOCK_HUB_LENGTH;
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_SPIN_PERIOD_TICKS};

/// Bumped with `PROTOCOL_VERSION` on any change to [`LANDMARKS`] (it keys what's saved about them).
pub const LANDMARKS_VERSION: u16 = 2;

/// The docking hub's index among [`LANDMARKS`].
pub const DOCKING_HUB: u8 = 2;

/// A place on a landmark where a suit can hide: crouched still in it, or parked in it, sensors
/// lose it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HideSpot {
    /// For the HUD, ASCII.
    pub name: &'static str,
    /// The bowl's floor, in the body's frame, m.
    pub center: Vec3,
    /// A suit whose origin is this close to `center` is in the spot, m.
    pub radius: f32,
    /// A suit hidden here is seen only this close, m.
    pub visual: f32,
}

/// A landmark: its shape, and how it moves.
#[derive(Clone, Copy, Debug)]
pub struct LandmarkDef {
    /// For the HUD, ASCII.
    pub name: &'static str,
    /// The centre of its station-keeping circle, or where it is if it doesn't move, sector frame, m.
    pub center: Vec3,
    /// Radius of that circle (in the sector's XZ plane), m; 0 for none.
    pub orbit_radius: f32,
    /// Ticks per trip round it.
    pub orbit_period: u32,
    /// Where on it the landmark is at tick 0, ticks.
    pub orbit_phase: u32,
    /// Its orientation at tick 0.
    pub rot0: Quat,
    /// The axis it spins about, in its own frame (unit).
    pub spin_axis: Vec3,
    /// Ticks per turn; 0 for no spin.
    pub spin_period: u32,
    pub shape: Shape,
    /// How far its shape reaches from its origin, m.
    pub bound: f32,
    /// Whether a suit can grip it.
    pub grippable: bool,
    pub hides: &'static [HideSpot],
    /// Part of the colony, which draws it (the client's landmark meshes leave it out).
    pub colony: bool,
}

/// The landmarks, by id. A sector has the first `SimConfig::landmarks` of them.
pub static LANDMARKS: [LandmarkDef; 3] = [MO_II, HERMIT, HUB];

const MO_II: LandmarkDef = LandmarkDef {
    name: names::MO_II,
    center: Vec3::new(-17_500.0, 1_500.0, 3_000.0),
    orbit_radius: 200.0,
    // 30 min round the circle: 0.70 m/s.
    orbit_period: 54_000,
    orbit_phase: 0,
    rot0: Quat::IDENTITY,
    spin_axis: Vec3::X,
    // 320 s a turn: the modules' outer edges, 110.65 m out, move at 2.17 m/s.
    spin_period: 9_600,
    shape: Shape { base: Base::Union(&MO_II_PRIMS), cuts: &MO_II_CUTS },
    // The mast's tip, at x = 334.
    bound: 335.0,
    grippable: true,
    hides: &[HideSpot { name: "AFT WELL", center: Vec3::new(-235.0, 0.0, 0.0), radius: 40.0, visual: 150.0 }],
    colony: false,
};

const MO_II_PRIMS: [Prim; 8] = [
    // The core, 400 m long.
    Prim::CylinderX { c: Vec3::ZERO, half_len: 200.0, r: 60.0, round: 4.0 },
    // The aft module (the Aft Well is cut into its −X face) and the fore module.
    Prim::RoundBox { c: Vec3::new(-230.0, 0.0, 0.0), half: Vec3::new(30.0, 80.0, 80.0), round: 6.0 },
    Prim::RoundBox { c: Vec3::new(230.0, 0.0, 0.0), half: Vec3::new(30.0, 80.0, 80.0), round: 6.0 },
    // Four pylons round the core's middle, their feet sunk in it (at |z| = 24 the core is at y = 55).
    Prim::RoundBox { c: Vec3::new(0.0, 68.0, 0.0), half: Vec3::new(40.0, 18.0, 24.0), round: 3.0 },
    Prim::RoundBox { c: Vec3::new(0.0, -68.0, 0.0), half: Vec3::new(40.0, 18.0, 24.0), round: 3.0 },
    Prim::RoundBox { c: Vec3::new(0.0, 0.0, 68.0), half: Vec3::new(40.0, 24.0, 18.0), round: 3.0 },
    Prim::RoundBox { c: Vec3::new(0.0, 0.0, -68.0), half: Vec3::new(40.0, 24.0, 18.0), round: 3.0 },
    // The mast, 8 m thick.
    Prim::Capsule { a: Vec3::new(260.0, 0.0, 0.0), b: Vec3::new(330.0, 0.0, 0.0), r: 4.0 },
];

/// The Aft Well: a bowl 25 m deep with a 53.6 m rim in the x = −260 face, its floor at x = −235
/// on the spin axis, and 35 m of module behind it.
const MO_II_CUTS: [SphereCut; 1] = [SphereCut { c: Vec3::new(-305.0, 0.0, 0.0), r: 70.0 }];

const HERMIT: LandmarkDef = LandmarkDef {
    name: "HERMIT",
    center: Vec3::new(9_000.0, 5_000.0, -12_000.0),
    orbit_radius: 0.0,
    orbit_period: 0,
    orbit_phase: 0,
    // 0.7 rad about (0.3, 1, 0.2).
    rot0: Quat::from_xyzw(0.096_771_3, 0.322_571_1, 0.064_514_2, 0.939_372_7),
    spin_axis: Vec3::Y,
    spin_period: 0,
    shape: Shape { base: Base::Ellipsoid(Vec3::new(900.0, 620.0, 760.0)), cuts: &HERMIT_CUTS },
    bound: 900.0,
    grippable: true,
    hides: &[
        HideSpot { name: "THE DEEP", center: Vec3::new(0.0, 590.0, 0.0), radius: 45.0, visual: 150.0 },
        HideSpot { name: "KEYHOLE", center: Vec3::new(0.0, 0.0, -730.0), radius: 45.0, visual: 150.0 },
        HideSpot { name: "FAR SIDE", center: Vec3::new(-870.0, 0.0, 0.0), radius: 45.0, visual: 150.0 },
    ],
    colony: false,
};

/// The docking hub's middle: halfway along its spire, on the colony's axis.
const HUB_CENTER: Vec3 =
    Vec3::new(COLONY_CENTER.x - COLONY_HALF_LENGTH - DOCK_HUB_LENGTH * 0.5, COLONY_CENTER.y, COLONY_CENTER.z);
/// Where its end face (the spire's mouth) is, in its frame: x.
pub const HUB_MOUTH_X: f32 = -DOCK_HUB_LENGTH * 0.5;

const HUB: LandmarkDef = LandmarkDef {
    name: "DOCKING HUB",
    center: HUB_CENTER,
    orbit_radius: 0.0,
    orbit_period: 0,
    orbit_phase: 0,
    rot0: Quat::IDENTITY,
    // It turns with the colony, on its clock (`world::colony_spin_angle`).
    spin_axis: Vec3::X,
    spin_period: COLONY_SPIN_PERIOD_TICKS,
    shape: Shape { base: Base::Union(&HUB_PRIMS), cuts: &[] },
    // The widest tier's rim at the cap's end: (450, 560) out.
    bound: 720.0,
    grippable: true,
    hides: &[],
    colony: true,
};

/// The spire, its mouth at x = −450 and its root in the end cap at +450, and the modules stacked
/// on it (`colony::hub::SPIRE_TIERS`, in its frame).
const HUB_PRIMS: [Prim; 5] = [
    Prim::CylinderX { c: Vec3::ZERO, half_len: DOCK_HUB_LENGTH * 0.5, r: SPIRE_RADIUS, round: 6.0 },
    hub_tier(0),
    hub_tier(1),
    hub_tier(2),
    hub_tier(3),
];

const fn hub_tier(k: usize) -> Prim {
    let (x, r, t) = SPIRE_TIERS[k];
    Prim::CylinderX { c: Vec3::new(x - HUB_CENTER.x, 0.0, 0.0), half_len: t * 0.5, r, round: 4.0 }
}

/// A crater bowl at each of three poles (exact surface points, whose normals are the axes): each
/// about 30 m deep with a 60 m rim.
const HERMIT_CUTS: [SphereCut; 3] = [
    SphereCut { c: Vec3::new(0.0, 665.0, 0.0), r: 75.0 },
    SphereCut { c: Vec3::new(0.0, 0.0, -805.0), r: 75.0 },
    SphereCut { c: Vec3::new(-945.0, 0.0, 0.0), r: 75.0 },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::{Bodies, Body, MAX_CUTS, MAX_LANDMARKS, Probe};
    use crate::colony::hub::{BAYS, bay_pose};
    use crate::config::{DT, SECTOR_LIMIT, SimConfig};
    use crate::content::salvage::{DOCK_CENTER, DOCK_HUB_LENGTH, DOCK_RADIUS};
    use crate::field::{FIELD_CENTER, Field, SPAWN_BASES};
    use crate::math::{Rng, angle_between, cos, length, normalize_or, sin, sqrt};
    use crate::sim::Sim;
    use crate::world::COLONY_SPIN_PERIOD_TICKS;
    use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
    use core::f32::consts::{PI, TAU};

    /// A normal jump between steps bigger than this is a wall, not an edge to walk over (the
    /// grounded step's `MAX_STEP_TURN`), rad.
    const MAX_STEP_TURN: f32 = 0.6;

    /// How far `l` is from the line through the origin along `axis`.
    fn off_axis(l: Vec3, axis: Vec3) -> f32 {
        length(l - axis * axis.dot(l))
    }

    /// The farthest a primitive's material reaches from the line through the origin along `axis`,
    /// and from the origin itself: the extreme points of its core, plus its rounding.
    fn reach(prim: &Prim, axis: Vec3) -> (f32, f32) {
        let mut far = (0.0f32, 0.0f32);
        let mut take = |p: Vec3, pad: f32| {
            far = (far.0.max(off_axis(p, axis) + pad), far.1.max(length(p) + pad));
        };
        match *prim {
            Prim::Sphere { c, r } => take(c, r),
            Prim::Capsule { a, b, r } => {
                take(a, r);
                take(b, r);
            }
            Prim::RoundBox { c, half, round } => {
                let core = half - Vec3::splat(round);
                for k in 0..8 {
                    let s = Vec3::new(
                        if k & 1 == 0 { -1.0 } else { 1.0 },
                        if k & 2 == 0 { -1.0 } else { 1.0 },
                        if k & 4 == 0 { -1.0 } else { 1.0 },
                    );
                    take(c + core * s, round);
                }
            }
            Prim::CylinderX { c, half_len, r, round } => {
                for k in 0..3_600 {
                    let a = k as f32 * (TAU / 3_600.0);
                    let rim = Vec3::new(0.0, cos(a), sin(a)) * (r - round);
                    take(c + Vec3::X * (half_len - round) + rim, round);
                    take(c - Vec3::X * (half_len - round) + rim, round);
                }
            }
        }
        far
    }

    /// The farthest `d`'s material is from its spin axis, and from its origin, m.
    fn extremes(d: &LandmarkDef) -> (f32, f32) {
        match d.shape.base {
            Base::Ellipsoid(a) => (a.max_element(), a.max_element()),
            Base::Union(prims) => prims.iter().fold((0.0, 0.0), |(s, o), p| {
                let (ps, po) = reach(p, d.spin_axis);
                (s.max(ps), o.max(po))
            }),
            Base::City => unreachable!("no landmark is the colony's city"),
        }
    }

    #[test]
    fn every_landmark_is_well_formed() {
        assert!(LANDMARKS.len() <= MAX_LANDMARKS);
        for d in &LANDMARKS {
            assert!(!d.name.is_empty() && d.name.is_ascii(), "{:?}", d.name);
            assert!((length(d.spin_axis) - 1.0).abs() < 1e-6, "{}: spin axis {}", d.name, d.spin_axis);
            assert!((d.rot0.length() - 1.0).abs() < 1e-6, "{}: rot0 {}", d.name, d.rot0);
            assert!(d.orbit_radius == 0.0 || d.orbit_period > 0, "{}: an orbit needs a period", d.name);
            assert!(d.shape.cuts.len() <= MAX_CUTS, "{}: {} cuts", d.name, d.shape.cuts.len());
            // The bound reaches past every bit of it.
            assert!(
                extremes(d).1 <= d.bound,
                "{}: material {} m out, bound {}",
                d.name,
                extremes(d).1,
                d.bound
            );
            for h in d.hides {
                assert!(!h.name.is_empty() && h.name.is_ascii(), "{:?}", h.name);
            }
        }
    }

    #[test]
    fn hermit_rot_is_unit() {
        let q = LANDMARKS[1].rot0;
        assert!((q.length() - 1.0).abs() < 1e-6);
        // 0.7 rad about (0.3, 1, 0.2).
        let axis = normalize_or(Vec3::new(0.3, 1.0, 0.2), Vec3::Y);
        let want = crate::math::quat_axis_angle(axis, 0.7);
        assert!(q.dot(want) > 1.0 - 1e-6, "{q} vs {want}");
    }

    #[test]
    fn landmark_surface_speed_within_budget() {
        // (The docking hub turns with the colony: only near its middle is it slow enough to land
        // on, which is where its deck hatch is.)
        for d in LANDMARKS.iter().filter(|d| !d.colony) {
            let spin = if d.spin_period == 0 { 0.0 } else { TAU / (d.spin_period as f32 * DT) };
            let drift =
                if d.orbit_radius == 0.0 { 0.0 } else { d.orbit_radius * TAU / (d.orbit_period as f32 * DT) };
            let worst = drift + spin * extremes(d).0;
            assert!(worst <= 5.0, "{}: its surface moves at up to {worst} m/s", d.name);
        }
        let mo = &LANDMARKS[0];
        let (far, _) = extremes(mo);
        assert!((far - (74.0 * sqrt(2.0) + 6.0)).abs() < 1e-3, "MO-II's modules reach {far} m out");
        let worst =
            mo.orbit_radius * TAU / (mo.orbit_period as f32 * DT) + far * TAU / (mo.spin_period as f32 * DT);
        assert!((worst - 2.87).abs() < 5e-3, "MO-II's surface moves at up to {worst} m/s");
        // The hub's deck hatch: its rim well under a catch's speed, and on the hub's end face.
        let hub = &LANDMARKS[usize::from(DOCKING_HUB)];
        let rim = crate::colony::hub::DECK_HATCH_RADIUS * TAU / (hub.spin_period as f32 * DT);
        assert!(rim < crate::ground::CATCH_SPEED * 0.5, "the deck hatch's rim moves at {rim} m/s");
        let mouth = Vec3::new(HUB_MOUTH_X - crate::ground::STANCE, 20.0, 0.0);
        assert!(crate::colony::hub::on_deck_hatch(mouth));
        assert!((hub.shape.probe(mouth).dist - crate::ground::STANCE).abs() < 0.01);
        assert!(!crate::colony::hub::on_deck_hatch(Vec3::new(HUB_MOUTH_X - 9.0, 60.0, 0.0)));
    }

    #[test]
    fn the_docking_hub_turns_with_the_colony_where_its_drawn() {
        let hub = &LANDMARKS[usize::from(DOCKING_HUB)];
        for t in [0, 1_000, 3_404] {
            let p = crate::bodies::landmark_pose(hub, t, 0.5);
            let want = crate::math::quat_axis_angle(Vec3::X, crate::world::colony_spin_angle(t, 0.5));
            assert!(p.rot.dot(want).abs() > 1.0 - 1e-6, "tick {t}");
            assert_eq!(p.pos, hub.center);
        }
        // Its mouth where the colony draws it, its root in the end cap.
        assert_eq!(hub.center.x + HUB_MOUTH_X, crate::colony::hub::SPIRE_MOUTH_X);
        assert_eq!(hub.center.x - HUB_MOUTH_X, COLONY_CENTER.x - COLONY_HALF_LENGTH);
    }

    #[test]
    fn landmarks_are_clear_of_everything() {
        const CLEAR: f32 = 2_000.0;
        // The field's rocks lie 1.2 to 7.2 km from its centre, with half-axes up to 64 × 1.6 m.
        const FIELD_REACH: f32 = 7_200.0 + 102.4;
        // The colony with its docking hub (off the −X cap) and the axis port (500 m off +X).
        let colony = |p: Vec3| {
            let rel = p - COLONY_CENTER;
            let (lo, hi) = (-COLONY_HALF_LENGTH - DOCK_HUB_LENGTH, COLONY_HALF_LENGTH + 500.0);
            let along = (lo - rel.x).max(rel.x - hi).max(0.0);
            let out = (sqrt(rel.y * rel.y + rel.z * rel.z) - COLONY_RADIUS).max(0.0);
            sqrt(along * along + out * out)
        };
        let fixed = [
            Vec3::new(0.0, 5_000.0, 9_000.0),
            Vec3::new(0.0, 4_000.0, 0.0),
            Vec3::new(0.0, 4_000.0, 8_000.0),
        ];
        // The bays' doors, where suits are thrown out, round the ring as the spin carries them.
        let doors = |c: Vec3| {
            (1..=BAYS as u8)
                .flat_map(|n| (0..4).map(move |q| bay_pose(n, q * COLONY_SPIN_PERIOD_TICKS / 4, 0.0).pos))
                .map(|p| length(c - p))
                .fold(f32::MAX, f32::min)
        };
        // (The docking hub is the colony's own.)
        for (k, d) in LANDMARKS.iter().enumerate().filter(|(_, d)| !d.colony) {
            let swept = d.bound + d.orbit_radius;
            let c = d.center;
            let gaps = [
                ("the field", length(c - FIELD_CENTER) - FIELD_REACH),
                ("the colony", colony(c)),
                ("the bays' doors", doors(c)),
                ("the dock", length(c - DOCK_CENTER) - DOCK_RADIUS),
            ];
            for (what, gap) in gaps {
                assert!(gap - swept >= CLEAR, "{} is {} m from {what}", d.name, gap - swept);
            }
            for p in SPAWN_BASES.iter().chain(&fixed) {
                assert!(
                    length(c - *p) - swept >= CLEAR,
                    "{} is {} m from {p}",
                    d.name,
                    length(c - *p) - swept
                );
            }
            for i in 0..3 {
                assert!(SECTOR_LIMIT - c[i].abs() - swept >= CLEAR, "{} is near the sector's edge", d.name);
            }
            for other in &LANDMARKS[k + 1..] {
                let gap = length(c - other.center) - swept - other.bound - other.orbit_radius;
                assert!(gap >= CLEAR, "{} is {gap} m from {}", d.name, other.name);
            }
        }
    }

    #[test]
    fn hide_spots_sit_on_bowl_floors() {
        let mut rng = Rng::new(6);
        for d in &LANDMARKS {
            let bare = crate::bodies::Shape { base: d.shape.base, cuts: &[] };
            for h in d.hides {
                let floor = d.shape.probe(h.center);
                assert!(
                    floor.dist.abs() < 0.05,
                    "{}: {} is {} m off the surface",
                    d.name,
                    h.name,
                    floor.dist
                );
                // The bowl it's the floor of: the cut whose surface it is on.
                let cut = d.shape.cuts.iter().find(|c| (length(h.center - c.c) - c.r).abs() < 0.05).unwrap();
                let up = floor.normal;
                assert!(length(up - normalize_or(cut.c - h.center, Vec3::Y)) < 1e-5);
                // Round the rim: up the bowl's wall from the floor to where it leaves the base.
                for _ in 0..64 {
                    let side = normalize_or(
                        up.cross(Vec3::new(rng.signed(), rng.signed(), rng.signed())),
                        up.any_orthonormal_vector(),
                    );
                    let wall = |a: f32| cut.c + (-up * cos(a) + side * sin(a)) * cut.r;
                    let (mut lo, mut hi) = (0.0, PI / 2.0);
                    assert!(bare.probe(wall(lo)).dist < 0.0 && bare.probe(wall(hi)).dist > 0.0);
                    for _ in 0..40 {
                        let mid = 0.5 * (lo + hi);
                        if bare.probe(wall(mid)).dist < 0.0 { lo = mid } else { hi = mid }
                    }
                    let rim = wall(lo);
                    let depth = (rim - h.center).dot(up);
                    assert!(depth >= 25.0 - 1e-2, "{}: {} is only {depth} m deep", d.name, h.name);
                    // The rim is a wall: the ground turns more there than a step can take.
                    let turn = angle_between(bare.probe(rim).normal, normalize_or(cut.c - rim, Vec3::Y));
                    assert!(turn > MAX_STEP_TURN, "{}: {}'s rim turns only {turn} rad", d.name, h.name);
                    // And the spot reaches from the floor to where a suit stands on it.
                    assert!(h.radius > crate::bodies::STANCE && length(rim - h.center) > h.radius);
                }
            }
        }
    }

    #[test]
    fn mo_ii_features_are_at_least_6_m_thick() {
        // From anywhere on its surface, straight in, there's 6 m of it: a trace's shortest step
        // (1 m) can't pass through anything, nor can a suit grounded on the far side of a wall.
        let shape = LANDMARKS[0].shape;
        let Base::Union(prims) = shape.base else { unreachable!("MO-II is a union") };
        for prim in prims {
            let thin = match *prim {
                Prim::Sphere { r, .. } | Prim::Capsule { r, .. } => 2.0 * r,
                Prim::RoundBox { half, .. } => 2.0 * half.min_element(),
                Prim::CylinderX { half_len, r, .. } => 2.0 * half_len.min(r),
            };
            assert!(thin >= 6.0, "{prim:?} is {thin} m thick");
        }
        let mut rng = Rng::new(7);
        let mut checked = 0;
        while checked < 20_000 {
            let p = Vec3::new(rng.signed() * 340.0, rng.signed() * 100.0, rng.signed() * 100.0);
            let Probe { dist, normal } = shape.probe(p);
            let on = p - normal * dist;
            if dist.abs() > 30.0 || shape.probe(on).dist.abs() > 1e-3 {
                continue;
            }
            checked += 1;
            let n = shape.probe(on).normal;
            for k in 1..=60 {
                let depth = k as f32 * 0.1;
                assert!(shape.probe(on - n * depth).dist < 0.0, "at {on}, only {depth} m of it");
            }
        }
    }

    #[test]
    fn grippable_rocks_of_the_default_field() {
        let field = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        let bodies = Bodies::at(&field, &LANDMARKS, 0);
        let n = (0..field.len()).filter(|&r| bodies.grippable(Body::Rock(r as u16))).count();
        assert_eq!((field.len(), n), (160, 75));
    }

    #[test]
    fn a_sector_has_the_landmarks_its_config_asks_for() {
        let with = |landmarks| {
            Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, landmarks, ..SimConfig::default() })
        };
        assert_eq!(with(SimConfig::default().landmarks).landmarks().len(), LANDMARKS.len());
        assert_eq!(with(1).landmarks().len(), 1);
        assert_eq!(with(0).landmarks().len(), 0);
        assert_eq!(with(200).landmarks().len(), LANDMARKS.len());
        assert_eq!(with(1).landmarks()[0].name, LANDMARKS[0].name);
    }
}
