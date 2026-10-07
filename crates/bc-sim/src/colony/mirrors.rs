//! The three mirrors that light the colony: a plate over each window, hinged at the docking hub's
//! (−X) end of the hull and opened towards the sun at +X, as an Island-3 colony's are. Light coming
//! along −X leaves a mirror opened β at 2β and crosses the colony through its window to the land
//! strip opposite. They open with the day (`time::day`).
//!
//! They turn with the colony, so their far edges sweep round faster than anything may move under a
//! suit: to suits and shots they aren't there. A content test keeps the volume they sweep clear of
//! the field, the landmarks, the spawn bases, the launch gate and the dock.

use glam::Vec3;

use crate::colony::frame::window_centre;
use crate::math::{cos, sin};
use crate::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};

/// From the hinge to the far edge, m.
pub const MIRROR_LENGTH: f32 = 7_000.0;
/// Across, m: the window's chord.
pub const MIRROR_WIDTH: f32 = 3_200.0;
pub const MIRROR_THICKNESS: f32 = 12.0;
/// The hinges: along the −X end of the hull, standing off it.
pub const HINGE_X: f32 = -COLONY_HALF_LENGTH;
pub const HINGE_R: f32 = COLONY_RADIUS + 60.0;

/// Mirror `k` (over window `k`) opened `beta`, in the colony's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mirror {
    /// The middle of its hinge.
    pub hinge: Vec3,
    /// From the hinge to the far edge (unit).
    pub along: Vec3,
    /// Across it, the way the angle round the axis grows (unit).
    pub across: Vec3,
    /// Out of its back, away from the window (unit): `across × along`.
    pub back: Vec3,
}

impl Mirror {
    pub fn new(k: usize, beta: f32) -> Self {
        let w = window_centre(k);
        let (cw, sw) = (cos(w), sin(w));
        let radial = Vec3::new(0.0, cw, sw);
        let across = Vec3::new(0.0, -sw, cw);
        let along = Vec3::X * cos(beta) + radial * sin(beta);
        Self {
            hinge: Vec3::new(HINGE_X, HINGE_R * cw, HINGE_R * sw),
            along,
            across,
            back: across.cross(along),
        }
    }

    /// The far edge's corners: least angle first.
    pub fn tips(&self) -> [Vec3; 2] {
        let tip = self.hinge + self.along * MIRROR_LENGTH;
        let half = self.across * (MIRROR_WIDTH * 0.5);
        [tip - half, tip + half]
    }

    /// Its middle (the centre of the plate).
    pub fn centre(&self) -> Vec3 {
        self.hinge + self.along * (MIRROR_LENGTH * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::hub::{BAYS, bay_pose};
    use crate::colony::time::{MIRROR_NOON, MIRROR_SHUT};
    use crate::config::{SECTOR_LIMIT, SimConfig};
    use crate::content::landmarks::LANDMARKS;
    use crate::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
    use crate::field::{FIELD_CENTER, Field, SPAWN_BASES};
    use crate::math::sqrt;
    use crate::world::COLONY_CENTER;
    use crate::world::COLONY_SPIN_PERIOD_TICKS;

    /// How far a point of the sector is from anything the mirrors sweep at any opening, m. They
    /// turn with the colony, so this is in its meridian half-plane: along the axis, and out from it.
    fn gap(p: Vec3) -> f32 {
        let rel = p - COLONY_CENTER;
        meridian_gap(rel.x, sqrt(rel.y * rel.y + rel.z * rel.z))
    }

    fn meridian_gap(x: f32, r: f32) -> f32 {
        let mut best = f32::MAX;
        let steps = 88;
        for b in 0..=steps {
            let beta = MIRROR_SHUT + (MIRROR_NOON - MIRROR_SHUT) * b as f32 / steps as f32;
            for i in 0..=140 {
                let u = MIRROR_LENGTH * i as f32 / 140.0;
                let mx = HINGE_X + u * cos(beta);
                let rho = HINGE_R + u * sin(beta);
                let (lo, hi) = (rho, sqrt(rho * rho + (MIRROR_WIDTH * 0.5).powi(2)));
                let dr = if r < lo {
                    lo - r
                } else if r > hi {
                    r - hi
                } else {
                    0.0
                };
                let dx = x - mx;
                best = best.min(sqrt(dx * dx + dr * dr));
            }
        }
        // The sampling (50 m along) and the plate's thickness.
        best - 50.0 - MIRROR_THICKNESS
    }

    #[test]
    fn a_mirror_is_a_plate_over_its_window_opening_to_plus_x() {
        for k in 0..3 {
            for beta in [MIRROR_SHUT, 0.3, MIRROR_NOON] {
                let m = Mirror::new(k, beta);
                for v in [m.along, m.across, m.back] {
                    assert!((v.length() - 1.0).abs() < 1e-5);
                }
                assert!(m.along.dot(m.across).abs() < 1e-5 && m.back.dot(m.along).abs() < 1e-5);
                assert!(m.along.x > 0.0, "it opens towards the sun");
                // Its front, the side away from `back`, faces the window and the sun.
                assert!(m.back.x < 0.0 || beta < 0.02);
                let w = window_centre(k);
                let out = Vec3::new(0.0, cos(w), sin(w));
                assert!(m.back.dot(out) > 0.0);
            }
        }
    }

    #[test]
    fn light_down_the_axis_leaves_a_mirror_into_its_window() {
        // A mirror opened β sends light arriving along −X back into the colony at 2β.
        for beta in [0.2f32, MIRROR_NOON] {
            let m = Mirror::new(0, beta);
            let front = -m.back;
            let d = -Vec3::X;
            let out = d - front * (2.0 * d.dot(front));
            let w = window_centre(0);
            let inward = -Vec3::new(0.0, cos(w), sin(w));
            assert!(out.dot(inward) > 0.0, "into the window");
            assert!((out.dot(-Vec3::X) - cos(2.0 * beta)).abs() < 1e-4);
        }
    }

    #[test]
    fn what_the_mirrors_sweep_is_clear_of_everything() {
        // The field's rocks lie 1.2 to 7.2 km from its centre, with half-axes up to 64 × 1.6 m: all of
        // that reach, whatever the seed, and the default field rock by rock.
        let rel = FIELD_CENTER - COLONY_CENTER;
        let (fx, fr) = (rel.x, sqrt(rel.y * rel.y + rel.z * rel.z));
        let mut reach = f32::MAX;
        for i in 0..=400 {
            let a = core::f32::consts::PI * i as f32 / 400.0;
            let (x, r) = (fx + 7_302.4 * cos(a), (fr + 7_302.4 * sin(a)).max(0.0));
            reach = reach.min(meridian_gap(x, r));
        }
        assert!(reach >= 1_000.0, "the mirrors pass {reach} m from the field's reach");
        let c = SimConfig::default();
        let field = Field::generate(c.field_seed, c.field_rocks);
        for rock in field.rocks() {
            assert!(gap(rock.pos) - rock.radius >= 1_000.0, "a rock at {}", rock.pos);
        }
        for d in &LANDMARKS {
            let g = gap(d.center) - d.bound - d.orbit_radius;
            assert!(g >= 2_000.0, "{} is {g} m from the mirrors", d.name);
        }
        for p in SPAWN_BASES {
            assert!(gap(p) >= 2_000.0, "a spawn base is {} m from the mirrors", gap(p));
        }
        // The bays' doors, where suits are thrown out, wherever the spin has them: on the bay
        // ring by the end cap, which the mirrors clear by a kilometre.
        for n in 1..=BAYS as u8 {
            for q in 0..4 {
                let door = bay_pose(n, q * COLONY_SPIN_PERIOD_TICKS / 4, 0.0).pos;
                assert!(gap(door) >= 1_000.0, "bay {n}'s door is {} m from the mirrors", gap(door));
            }
        }
        assert!(gap(DOCK_CENTER) - DOCK_RADIUS >= 2_000.0);
        // And they stay in the sector.
        for k in 0..3 {
            for t in Mirror::new(k, MIRROR_NOON).tips() {
                let p = COLONY_CENTER + t;
                assert!(p.abs().max_element() < SECTOR_LIMIT, "{p}");
            }
        }
    }
}
