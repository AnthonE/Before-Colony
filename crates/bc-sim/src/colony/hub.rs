//! The docking hub on the ring's axis, as built (`docs/RING.md`): the drum standing forward of the
//! ring (−X), whose forward face is the hub's mouth, with the deck hatch in its middle and the
//! pilots' bays' doors round it; the spine through the ring's hole to its far side; and six spokes
//! out to the ring's ceiling. All turn with the colony, and the drum and spine are shapes of
//! revolution about its axis.
//!
//! A suit launches from its pilot's own bay: it rides the bay's catapult cradle in the door
//! ([`Body::Bay`](crate::bodies::Body::Bay), posed by [`bay_pose`]) until its pilot lets go of the
//! grip, and is thrown out of the door along −X ([`BAY_LAUNCH_SPEED`]) with the door's own speed
//! round the axis, about 40 m/s: away from the ring, which is behind the drum. Coming home is on
//! the axis, where nothing moves: at rest in the dock's ring of lights off the mouth, or landed on
//! the deck hatch.

use glam::{Quat, Vec3};

use crate::bodies::{Base, BodyPose, Prim, Shape};
use crate::math::{look_rotation, quat_axis_angle};
use crate::world::{COLONY_CENTER, COLONY_HALF_WIDTH, COLONY_INNER_RADIUS, SPIN_RATE, colony_spin_angle};

/// The drum: its radius, and the x of its faces in the colony's frame (its forward face, at
/// [`DRUM_X`]`.0`, is the hub's mouth).
pub const DRUM_RADIUS: f32 = 440.0;
pub const DRUM_X: (f32, f32) = (-650.0, -350.0);
/// The hub's mouth: the drum's forward face (the dock lies beyond it).
pub const MOUTH_X: f32 = DRUM_X.0;
/// The spine, from the drum's back face through the ring's hole to its far side.
pub const SPINE_RADIUS: f32 = 100.0;
pub const SPINE_X: (f32, f32) = (DRUM_X.1, COLONY_HALF_WIDTH);
/// Spokes from the spine out to the ring's ceiling, in the ring's middle (x 0), each with a lift
/// down to the town in it: how many, and how thick.
pub const SPOKES: u32 = 6;
pub const SPOKE_RADIUS: f32 = 22.0;

/// Spoke `k`'s angle round the axis, from +Y towards +Z, before the colony's spin, rad.
pub fn spoke_angle(k: u32) -> f32 {
    core::f32::consts::TAU * (k as f32 + 0.5) / SPOKES as f32
}

/// The radius the bays' doors are at on the mouth: 0.4 g in the bays.
pub const BAY_RADIUS: f32 = 400.0;
/// The bays round the mouth (a pilot's is `slot % 99 + 1`).
pub const BAYS: u32 = 99;

/// The deck hatch in the middle of the mouth (on the axis), m: a suit standing on it docks, and
/// goes in to its bay (`Sim::docked`). Near the axis the face turns slowly (3 m/s at its rim), so a
/// suit with its grip armed can land by it.
pub const DECK_HATCH_RADIUS: f32 = 30.0;

/// Whether a suit standing on the docking hub at `local` (the hub landmark's frame, centred on the
/// ring's middle) is on its deck hatch.
pub fn on_deck_hatch(local: Vec3) -> bool {
    local.x < MOUTH_X && local.y * local.y + local.z * local.z < DECK_HATCH_RADIUS * DECK_HATCH_RADIUS
}

/// The bays' doors: their outer face, x (each door stands 6 m proud of the mouth).
pub const BAY_DOOR_X: f32 = MOUTH_X - 6.0;
/// Where a suit rides its bay's cradle, in the bay's frame ([`bay_pose`]): in the doorway (the
/// door's 6 m deep, the drum's face behind it), facing out, its head toward the axis (the bay's
/// floor is outward, under the spin).
pub const BAY_RIDE_LOCAL: Vec3 = Vec3::new(3.0, 0.0, 0.0);
/// How fast the catapult throws a suit out of its door, m/s (along −X, on top of the door's own
/// speed round the axis).
pub const BAY_LAUNCH_SPEED: f32 = 40.0;
/// The cradle a suit rides in its bay: a deck under its feet (the frame's +Y is outward, down in
/// the bay). Nothing grips it, collides with it or walks it: it's only where the rider stands.
const BAY_DECK: [Prim; 1] = [Prim::RoundBox {
    c: Vec3::new(BAY_RIDE_LOCAL.x + 8.0, crate::bodies::STANCE + 1.0, 0.0),
    half: Vec3::new(14.0, 1.0, 18.0),
    round: 0.5,
}];

/// The bay of the pilot in sector slot `slot`, 1..=[`BAYS`] (what the hangar calls "BAY nn").
pub const fn bay_of_slot(slot: u16) -> u8 {
    (slot as u32 % BAYS + 1) as u8
}

/// Whether `n` names one of the bays.
pub const fn is_bay(n: u8) -> bool {
    n >= 1 && n as u32 <= BAYS
}

/// Bay `n`'s angle round the mouth, from +Y toward +Z, before the colony's spin, rad (the client
/// draws its door there: `TAU·(n−1)/BAYS`).
pub fn bay_angle(n: u8) -> f32 {
    core::f32::consts::TAU * f32::from(n.saturating_sub(1)) / BAYS as f32
}

/// Bay `n`'s frame at tick `t` plus `frac` of the next, turning with the colony: its origin in the
/// middle of its door's outer face, +X into the drum, +Y outward (the bay's down), +Z the way the
/// spin carries it. A closed form of the tick, as a landmark's pose is.
pub fn bay_pose(n: u8, t: u32, frac: f32) -> BodyPose {
    let rot = quat_axis_angle(Vec3::X, colony_spin_angle(t, frac) + bay_angle(n));
    let pos = COLONY_CENTER + rot * Vec3::new(BAY_DOOR_X, BAY_RADIUS, 0.0);
    let ang_vel = Vec3::X * SPIN_RATE;
    BodyPose { pos, rot, vel: ang_vel.cross(pos - COLONY_CENTER), ang_vel, moving: true }
}

/// How a suit rides its bay's cradle: facing out of the door, its head toward the axis.
pub fn bay_ride_rot() -> Quat {
    look_rotation(-Vec3::X, -Vec3::Y)
}

/// The bays' shape, in a bay's frame: its cradle's deck.
pub const fn bay_shape() -> Shape {
    Shape { base: Base::Union(&BAY_DECK), cuts: &[] }
}

/// What a rider in its bay feels: the deck holding it toward the axis against the spin, m/s².
pub fn bay_g() -> f32 {
    SPIN_RATE * SPIN_RATE * BAY_RADIUS
}

/// The hub as one solid, in its own frame (centred on the ring's middle, turning with it): the drum,
/// the spine, and the spokes (`content::landmarks`' docking hub).
pub const HUB_PRIMS: [Prim; 8] = [
    Prim::CylinderX {
        c: Vec3::new((DRUM_X.0 + DRUM_X.1) * 0.5, 0.0, 0.0),
        half_len: (DRUM_X.1 - DRUM_X.0) * 0.5,
        r: DRUM_RADIUS,
        round: 8.0,
    },
    Prim::CylinderX {
        c: Vec3::new((SPINE_X.0 + SPINE_X.1) * 0.5, 0.0, 0.0),
        half_len: (SPINE_X.1 - SPINE_X.0) * 0.5,
        r: SPINE_RADIUS,
        round: 4.0,
    },
    spoke(SPOKE_DIRS[0]),
    spoke(SPOKE_DIRS[1]),
    spoke(SPOKE_DIRS[2]),
    spoke(SPOKE_DIRS[3]),
    spoke(SPOKE_DIRS[4]),
    spoke(SPOKE_DIRS[5]),
];

/// Each spoke's way out from the axis, (cos, sin) of [`spoke_angle`] (30° + 60°·k).
const SPOKE_DIRS: [(f32, f32); SPOKES as usize] = [
    (0.866_025_4, 0.5),
    (0.0, 1.0),
    (-0.866_025_4, 0.5),
    (-0.866_025_4, -0.5),
    (0.0, -1.0),
    (0.866_025_4, -0.5),
];

/// A spoke, from the spine's skin out to the ceiling.
const fn spoke((c, s): (f32, f32)) -> Prim {
    let (r0, r1) = (SPINE_RADIUS, COLONY_INNER_RADIUS);
    Prim::Capsule { a: Vec3::new(0.0, r0 * c, r0 * s), b: Vec3::new(0.0, r1 * c, r1 * s), r: SPOKE_RADIUS }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::G0;
    use crate::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
    use crate::math::{cos, sin};
    use crate::world::{COLONY_HALF_WIDTH, colony_spin_angle};

    #[test]
    fn the_bays_hang_at_four_tenths_of_a_g_on_the_drums_face() {
        assert!((bay_g() / G0 - 0.4).abs() < 0.005, "{} g", bay_g() / G0);
        // A pilot walks their bay in the pull its deck feels.
        assert!((bay_g() - crate::foot::GRAVITY).abs() < 0.01);
        const { assert!(BAY_RADIUS + 20.0 < DRUM_RADIUS && SPINE_RADIUS < BAY_RADIUS - 20.0) };
        // The drum stands forward of the ring, the doors on its forward face.
        const { assert!(DRUM_X.1 < -COLONY_HALF_WIDTH && BAY_DOOR_X < MOUTH_X) };
        for (k, (c, s)) in SPOKE_DIRS.into_iter().enumerate() {
            let a = spoke_angle(k as u32);
            assert!((cos(a) - c).abs() < 1e-6 && (sin(a) - s).abs() < 1e-6, "spoke {k}");
        }
    }

    #[test]
    fn the_mouth_keeps_clear_of_the_dock() {
        // The dock's sphere is beyond the mouth, and a door isn't in it.
        const { assert!(DOCK_CENTER.x + DOCK_RADIUS < COLONY_CENTER.x + MOUTH_X - 40.0) };
        let door = COLONY_CENTER + Vec3::new(BAY_DOOR_X, BAY_RADIUS, 0.0);
        assert!((door - DOCK_CENTER).length() > DOCK_RADIUS + 200.0);
    }

    #[test]
    fn a_bay_is_its_door_as_drawn_turning_with_the_colony() {
        for (n, t, frac) in [(1u8, 0u32, 0.0f32), (2, 17, 0.5), (50, 1_000, 0.25), (99, 1_902, 0.9)] {
            let p = bay_pose(n, t, frac);
            // Where the client draws door `n − 1`, turned by the colony's spin.
            let a = colony_spin_angle(t, frac) + core::f32::consts::TAU * f32::from(n - 1) / BAYS as f32;
            let want = COLONY_CENTER + Vec3::new(BAY_DOOR_X, BAY_RADIUS * cos(a), BAY_RADIUS * sin(a));
            assert!((p.pos - want).length() < 0.05, "bay {n}: {} vs {want}", p.pos);
            // Moving round the axis the way the frame's +Z points, at about 40 m/s, and its +Y out.
            let v = p.point_vel(p.pos);
            assert!((v.length() - SPIN_RATE * BAY_RADIUS).abs() < 0.01 && (v.length() - 39.6).abs() < 0.2);
            assert!(v.normalize().dot(p.rot * Vec3::Z) > 0.9999);
            assert!((p.rot * Vec3::Y).dot((p.pos - COLONY_CENTER).with_x(0.0).normalize()) > 0.9999);
        }
        // The pilots' bays go round the mouth, a bay each.
        assert_eq!((bay_of_slot(0), bay_of_slot(98), bay_of_slot(99)), (1, 99, 1));
        assert!(is_bay(1) && is_bay(99) && !is_bay(0) && !is_bay(100));
        // The deck the rider stands on is under its feet, the bay's down.
        let pr = bay_shape().probe(BAY_RIDE_LOCAL);
        assert!((pr.dist - crate::bodies::STANCE).abs() < 0.01 && pr.normal.dot(-Vec3::Y) > 0.999, "{pr:?}");
    }
}
