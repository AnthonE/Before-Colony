//! The docking hub at the colony's −X end, as built: the spire on the axis (the hub tube suits
//! dock at, stacked with modules), and the bay ring, the spin ring standing off the end cap where
//! the pilots' bays hang at 0.7 g. Both turn with the colony, and both are shapes of revolution
//! about its axis, so their size is all there is to them here.
//!
//! A suit launches from its pilot's own bay: it rides the bay's catapult cradle in the door
//! ([`Body::Bay`](crate::bodies::Body::Bay), posed by [`bay_pose`]) until its pilot lets go of the
//! grip, and is thrown out of the door ([`BAY_LAUNCH_SPEED`]) with the door's own speed round the
//! axis, about 125 m/s. Coming home is on the axis, where nothing moves: at rest in the dock's ring
//! of lights off the spire's mouth.

use glam::{Quat, Vec3};

use crate::bodies::{Base, BodyPose, Prim, Shape};
use crate::colony::frame::SPIN_RATE;
use crate::content::salvage::DOCK_HUB_LENGTH;
use crate::math::{look_rotation, quat_axis_angle};
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, colony_spin_angle};

/// The spire's tube: the docking hub, from the end cap out to its mouth.
pub const SPIRE_RADIUS: f32 = 340.0;
/// Where the spire's mouth is (the launch gate and the dock lie beyond it).
pub const SPIRE_MOUTH_X: f32 = -COLONY_HALF_LENGTH - DOCK_HUB_LENGTH;

/// The modules stacked on the spire, from the end cap out: (middle x, radius, thickness), m.
pub const SPIRE_TIERS: [(f32, f32, f32); 4] =
    [(-16_120.0, 560.0, 60.0), (-16_330.0, 470.0, 40.0), (-16_560.0, 430.0, 30.0), (-16_820.0, 400.0, 50.0)];

/// The bay ring: inner and outer radius, and the x of its faces (it stands 50 m off the end cap).
pub const BAY_RING_INNER: f32 = 1_950.0;
pub const BAY_RING_OUTER: f32 = 2_650.0;
pub const BAY_RING_X: (f32, f32) = (-COLONY_HALF_LENGTH - 450.0, -COLONY_HALF_LENGTH - 50.0);
/// The radius the bays hang at: 0.7 g.
pub const BAY_RADIUS: f32 = 2_252.0;
/// The bays round the ring (a pilot's is `slot % 99 + 1`).
pub const BAYS: u32 = 99;
/// Spokes from the spire to the bay ring.
pub const SPOKES: u32 = 6;

/// The deck hatch in the middle of the spire's mouth (its end face, on the axis), m: a suit
/// standing on it docks, and goes in to its bay (`Sim::docked`). Near the axis the face turns
/// slowly (2.2 m/s at its rim), so a suit with its grip armed can land by it.
pub const DECK_HATCH_RADIUS: f32 = 40.0;

/// Whether a suit standing on the docking hub at `local` (the hub landmark's frame) is on its deck
/// hatch.
pub fn on_deck_hatch(local: Vec3) -> bool {
    local.x < -DOCK_HUB_LENGTH * 0.5
        && local.y * local.y + local.z * local.z < DECK_HATCH_RADIUS * DECK_HATCH_RADIUS
}

/// The bays' doors: their outer face, x (each door stands 6 m proud of the ring's −X face).
pub const BAY_DOOR_X: f32 = BAY_RING_X.0 - 6.0;
/// Where a suit rides its bay's cradle, in the bay's frame ([`bay_pose`]): in the doorway (the
/// door's 6 m deep, the ring's face behind it), facing out, its head toward the axis (the bay's
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

/// Bay `n`'s angle round the ring, from +Y toward +Z, before the colony's spin, rad (the client
/// draws its door there: `TAU·(n−1)/BAYS`).
pub fn bay_angle(n: u8) -> f32 {
    core::f32::consts::TAU * f32::from(n.saturating_sub(1)) / BAYS as f32
}

/// Bay `n`'s frame at tick `t` plus `frac` of the next, turning with the colony: its origin in the
/// middle of its door's outer face, +X into the ring, +Y outward (the bay's down), +Z the way the
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::frame::gravity;
    use crate::config::G0;
    use crate::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
    use crate::world::COLONY_RADIUS;

    #[test]
    fn the_bays_hang_at_seven_tenths_of_a_g_in_their_ring() {
        let g = gravity(COLONY_RADIUS - BAY_RADIUS) / G0;
        assert!((g - 0.7).abs() < 0.005, "{g}");
        const { assert!(BAY_RING_INNER < BAY_RADIUS && BAY_RADIUS < BAY_RING_OUTER) };
        const { assert!(BAY_RING_X.0 < BAY_RING_X.1 && BAY_RING_X.1 < -COLONY_HALF_LENGTH) };
    }

    #[test]
    fn the_spire_keeps_clear_of_the_launch_gate_and_the_dock() {
        for (x, r, t) in SPIRE_TIERS {
            assert!(x - t / 2.0 > SPIRE_MOUTH_X && x + t / 2.0 < -COLONY_HALF_LENGTH, "a tier off the spire");
            assert!(r > SPIRE_RADIUS && r < BAY_RING_INNER);
        }
        // The dock's sphere is beyond the mouth; the bays' doors stand out of the ring's face.
        const { assert!(DOCK_CENTER.x + DOCK_RADIUS < SPIRE_MOUTH_X - 40.0) };
        const { assert!(BAY_DOOR_X < BAY_RING_X.0) };
    }

    #[test]
    fn a_bay_is_its_door_as_drawn_turning_with_the_colony() {
        use crate::math::{cos, sin};
        use crate::world::{COLONY_CENTER, colony_spin_angle};
        for (n, t, frac) in [(1u8, 0u32, 0.0f32), (2, 17, 0.5), (50, 1_000, 0.25), (99, 3_404, 0.9)] {
            let p = bay_pose(n, t, frac);
            // Where the client draws door `n − 1`, turned by the colony's spin.
            let a = colony_spin_angle(t, frac) + core::f32::consts::TAU * f32::from(n - 1) / BAYS as f32;
            let want = COLONY_CENTER + Vec3::new(BAY_DOOR_X, BAY_RADIUS * cos(a), BAY_RADIUS * sin(a));
            assert!((p.pos - want).length() < 0.05, "bay {n}: {} vs {want}", p.pos);
            // Moving round the axis the way the frame's +Z points, at about 125 m/s, and its +Y out.
            let v = p.point_vel(p.pos);
            assert!((v.length() - SPIN_RATE * BAY_RADIUS).abs() < 0.01 && (v.length() - 124.7).abs() < 0.2);
            assert!(v.normalize().dot(p.rot * Vec3::Z) > 0.9999);
            assert!((p.rot * Vec3::Y).dot((p.pos - COLONY_CENTER).with_x(0.0).normalize()) > 0.9999);
        }
        // The pilots' bays go round the ring, a bay each.
        assert_eq!((bay_of_slot(0), bay_of_slot(98), bay_of_slot(99)), (1, 99, 1));
        assert!(is_bay(1) && is_bay(99) && !is_bay(0) && !is_bay(100));
        // The deck the rider stands on is under its feet, the bay's down.
        let pr = bay_shape().probe(BAY_RIDE_LOCAL);
        assert!((pr.dist - crate::bodies::STANCE).abs() < 0.01 && pr.normal.dot(-Vec3::Y) > 0.999, "{pr:?}");
        assert!((bay_g() / crate::config::G0 - 0.7).abs() < 0.005);
    }
}
