//! The louvres that light the ring (`docs/RING.md`): a band of plates round its spine in the hole,
//! on the sun's side of the spokes (+X), each hinged along its middle and opened with the day
//! (`time::day`). Shut (β 0) a plate lies edge-on to the sunlight coming along −X and lets it by;
//! opened β it turns the light 2β, so at noon (45°) it throws it straight out from the axis, through
//! the ring's ceiling and down onto the town.
//!
//! They turn with the colony and nothing touches them: to suits and shots they aren't there. A test
//! keeps them in the hole, clear of the spine, the spokes and the ring.

use glam::Vec3;

use crate::math::{cos, sin};

/// Plates round the spine.
pub const LOUVRES: u32 = 24;
/// Where each plate's middle is: along the axis, and out from it, m (colony frame).
pub const LOUVRE_X: f32 = 160.0;
pub const LOUVRE_R: f32 = 420.0;
/// Along a plate (across the sunlight when it's open), m.
pub const LOUVRE_LENGTH: f32 = 260.0;
/// Round the axis, m.
pub const LOUVRE_WIDTH: f32 = 100.0;
pub const LOUVRE_THICKNESS: f32 = 3.0;

/// Plate `k` opened `beta`, in the colony's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Louvre {
    /// Its middle.
    pub centre: Vec3,
    /// Along it (unit): along the axis shut, turned out towards the axis's side as it opens.
    pub along: Vec3,
    /// Round the axis (unit), the way the angle grows.
    pub across: Vec3,
    /// Out of its face, towards the sun and the axis's outside (unit): `along × across`.
    pub face: Vec3,
}

impl Louvre {
    pub fn new(k: u32, beta: f32) -> Self {
        let a = louvre_angle(k);
        let (ca, sa) = (cos(a), sin(a));
        let radial = Vec3::new(0.0, ca, sa);
        let across = Vec3::new(0.0, -sa, ca);
        // Its face turns from radial (shut, edge-on to light along −X) towards +X, so light along
        // −X leaves it at 2β from −X, outward.
        let face = Vec3::X * sin(beta) + radial * cos(beta);
        let along = Vec3::X * cos(beta) - radial * sin(beta);
        Self { centre: Vec3::new(LOUVRE_X, LOUVRE_R * ca, LOUVRE_R * sa), along, across, face }
    }

    /// Where sunlight coming along −X goes off its face (unit).
    pub fn throws(&self) -> Vec3 {
        let d = -Vec3::X;
        d - self.face * (2.0 * d.dot(self.face))
    }

    /// Its four corners.
    pub fn corners(&self) -> [Vec3; 4] {
        let (l, w) = (self.along * (LOUVRE_LENGTH * 0.5), self.across * (LOUVRE_WIDTH * 0.5));
        [self.centre - l - w, self.centre - l + w, self.centre + l - w, self.centre + l + w]
    }
}

/// Plate `k`'s angle round the axis, before the colony's spin, rad.
pub fn louvre_angle(k: u32) -> f32 {
    core::f32::consts::TAU * k as f32 / LOUVRES as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::hub::{SPINE_RADIUS, SPOKE_RADIUS};
    use crate::colony::time::{MIRROR_NOON, MIRROR_SHUT};
    use crate::world::{COLONY_HALF_WIDTH, COLONY_INNER_RADIUS};

    #[test]
    fn at_noon_they_throw_the_sun_out_onto_the_ceiling_and_shut_they_let_it_by() {
        for k in 0..LOUVRES {
            let open = Louvre::new(k, MIRROR_NOON);
            let out = (open.centre - Vec3::X * open.centre.x).normalize();
            assert!(open.throws().dot(out) > 0.999, "plate {k} throws {}", open.throws());
            let shut = Louvre::new(k, MIRROR_SHUT);
            assert!(shut.throws().dot(-Vec3::X) > 0.99, "plate {k} shut throws {}", shut.throws());
        }
    }

    #[test]
    fn they_stay_in_the_hole_clear_of_the_spine_the_spokes_and_the_ring() {
        for k in 0..LOUVRES {
            for beta in [MIRROR_SHUT, 0.3, MIRROR_NOON] {
                for c in Louvre::new(k, beta).corners() {
                    let r = c.y.hypot(c.z);
                    assert!(r > SPINE_RADIUS + 50.0 && r < COLONY_INNER_RADIUS - 50.0, "plate {k}: r {r}");
                    // On the sun's side of the spokes (at x 0), inside the ring's width.
                    assert!(
                        c.x > SPOKE_RADIUS + 5.0 && c.x < COLONY_HALF_WIDTH - 5.0,
                        "plate {k}: x {}",
                        c.x
                    );
                }
            }
        }
    }
}
