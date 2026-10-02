//! The Earth Sphere as seen from Sector L1: where the Sun, Earth, the Moon and the five Lagrange
//! points are, in the sector's own frame and metres (its origin is the L1 point, where the First
//! Colony keeps station between Earth and the Moon).
//!
//! The sky (`bc-client`'s `sky.rs`) draws Earth, the Moon and the Sun in these directions, and the
//! chart (`nav`, `bc-client`'s `chart.rs`) places them at these distances, so the Earth in the
//! window is where the chart says it is. Distances are true (the Earth–Moon system's: 384,400 km
//! apart, L1 85% of the way to the Moon); the sky enlarges the discs to read at a glance.
//!
//! The sector's frame doesn't turn with the Moon's month: like the sky, the chart holds the Earth
//! Sphere where the sector's first survey put it.

use glam::{Quat, Vec3};

/// Direction to the Sun (from anywhere in the sector: it is 1 AU away). The colony's axis points
/// roughly at it (its mirrors are at the sunward +X cap), far enough off-axis that the hull catches
/// the light.
pub const SUN_DIR: Vec3 = Vec3::new(0.84788, 0.34913, 0.39900);
/// Direction from L1 to Earth: the Earth–Moon line. (The sky's first survey put Earth and the Moon
/// 4.5° short of opposite; they now sit on the line, each within 2.3° of where it was.)
pub const EARTH_DIR: Vec3 = Vec3::new(-0.32986, -0.52482, 0.78470);
/// Direction from L1 to the Moon: exactly opposite Earth, since L1 lies on the line between them.
pub const MOON_DIR: Vec3 = Vec3::new(0.32986, 0.52482, -0.78470);
/// Normal of the Milky Way's plane.
pub const GALAXY_NORMAL: Vec3 = Vec3::new(0.4703, 0.7705, -0.4303);

/// Kilometres, in metres.
pub const KM: f32 = 1_000.0;
/// One astronomical unit, m.
pub const AU: f32 = 1.495_978_7e11;
/// Earth's and the Moon's mean radii, m.
pub const EARTH_RADIUS: f32 = 6_371.0 * KM;
pub const MOON_RADIUS: f32 = 1_737.4 * KM;
/// The Moon's mean distance from Earth, m.
pub const EARTH_MOON: f32 = 384_400.0 * KM;
/// The Moon's share of the Earth–Moon system's mass.
pub const MU: f64 = 0.012_150_585_6;
/// From L1 to Earth's centre and to the Moon's, m (the restricted three-body problem's collinear
/// point for [`MU`]; `tests::the_lagrange_points_balance`).
pub const L1_TO_EARTH: f32 = 326_380.86 * KM;
pub const L1_TO_MOON: f32 = 58_019.14 * KM;
/// L2, beyond the Moon, from its centre, m.
pub const MOON_TO_L2: f32 = 64_514.91 * KM;
/// L3, beyond Earth on the far side from the Moon, from Earth's centre, m.
pub const EARTH_TO_L3: f32 = 381_675.4 * KM;

/// Earth's centre, m.
pub fn earth() -> Vec3 {
    EARTH_DIR * L1_TO_EARTH
}

/// The Moon's centre, m.
pub fn moon() -> Vec3 {
    MOON_DIR * L1_TO_MOON
}

/// The Earth–Moon system's barycentre, which the Moon goes round, m (inside Earth, 4,671 km from
/// its centre).
pub fn barycentre() -> Vec3 {
    earth() + (moon() - earth()) * MU as f32
}

/// The normal of the Moon's orbit (its angular momentum's direction): the plane holds the
/// Earth–Moon line and, within a few degrees as the ecliptic does, the Sun. The sector's up is its
/// north.
pub fn orbit_normal() -> Vec3 {
    let n = EARTH_DIR.cross(SUN_DIR).normalize();
    if n.y < 0.0 { -n } else { n }
}

/// One of the five Lagrange points of the Earth–Moon system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lagrange {
    /// Between Earth and the Moon: the First Colony's, and Sector L1's.
    L1,
    /// Beyond the Moon.
    L2,
    /// Beyond Earth, opposite the Moon.
    L3,
    /// 60° ahead of the Moon on its orbit.
    L4,
    /// 60° behind it.
    L5,
}

impl Lagrange {
    pub const ALL: [Lagrange; 5] = [Lagrange::L1, Lagrange::L2, Lagrange::L3, Lagrange::L4, Lagrange::L5];

    pub fn name(self) -> &'static str {
        match self {
            Lagrange::L1 => "L1",
            Lagrange::L2 => "L2",
            Lagrange::L3 => "L3",
            Lagrange::L4 => "L4",
            Lagrange::L5 => "L5",
        }
    }

    /// Where it is, m (L1 is the origin).
    pub fn pos(self) -> Vec3 {
        let (e, m) = (earth(), moon());
        let trailing = |deg: f32| e + Quat::from_axis_angle(orbit_normal(), deg.to_radians()) * (m - e);
        match self {
            Lagrange::L1 => Vec3::ZERO,
            Lagrange::L2 => m + MOON_DIR * MOON_TO_L2,
            Lagrange::L3 => e + EARTH_DIR * EARTH_TO_L3,
            Lagrange::L4 => trailing(60.0),
            Lagrange::L5 => trailing(-60.0),
        }
    }

    /// What the colonies know of it, in the world's voice (`docs/STORY.md`, "The calendar and the
    /// eras"): only L1 has a colony; the rest open with the expeditions of the Cluster era.
    pub fn about(self) -> &'static str {
        match self {
            Lagrange::L1 => "Sector L1: the First Colony and its field. You are here.",
            Lagrange::L2 => {
                "Beyond the Moon, in its shadow half the month. No colony yet: the Cluster's expeditions will chart it."
            }
            Lagrange::L3 => "Behind Earth, always out of sight of the Moon. Nobody has been.",
            Lagrange::L4 => {
                "Stable ground 60° ahead of the Moon, where the next cylinders are meant to go. Unopened."
            }
            Lagrange::L5 => {
                "Stable ground 60° behind the Moon. The Consortium holds claims here it hasn't worked. Unopened."
            }
        }
    }
}

/// A point on the Moon's orbit (round the barycentre, as near enough as a chart shows), `a` radians
/// on from where the Moon is now.
pub fn moon_orbit_at(a: f32) -> Vec3 {
    let c = barycentre();
    c + Quat::from_axis_angle(orbit_normal(), a) * (moon() - c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pull toward each Lagrange point's place in the rotating frame: zero at a true one.
    fn pull(x: f64) -> f64 {
        let mu = MU;
        x - (1.0 - mu) * (x + mu) / (x + mu).abs().powi(3)
            - mu * (x - 1.0 + mu) / (x - 1.0 + mu).abs().powi(3)
    }

    #[test]
    fn the_lagrange_points_balance() {
        // In units of the Earth–Moon distance, from the barycentre (Earth at −μ, the Moon at 1 − μ).
        let d = f64::from(EARTH_MOON);
        let l1 = f64::from(L1_TO_EARTH) / d - MU;
        let l2 = 1.0 - MU + f64::from(MOON_TO_L2) / d;
        let l3 = -MU - f64::from(EARTH_TO_L3) / d;
        for (name, x) in [("L1", l1), ("L2", l2), ("L3", l3)] {
            assert!(pull(x).abs() < 1e-6, "{name}: {}", pull(x));
        }
        // L1 is on the line, the two distances add up.
        assert!((L1_TO_EARTH + L1_TO_MOON - EARTH_MOON).abs() < 1.0 * KM);
    }

    #[test]
    fn the_sphere_is_where_the_sky_puts_it() {
        for d in [SUN_DIR, EARTH_DIR, MOON_DIR, GALAXY_NORMAL] {
            assert!((d.length() - 1.0).abs() < 1e-4, "{d}");
        }
        // Within 2.3° of the first survey's Earth and Moon.
        let old_earth = Vec3::new(-0.29987, -0.54975, 0.77965).normalize();
        let old_moon = Vec3::new(0.35934, 0.49908, -0.78854).normalize();
        assert!(EARTH_DIR.angle_between(old_earth).to_degrees() < 2.3);
        assert!(MOON_DIR.angle_between(old_moon).to_degrees() < 2.3);
        assert_eq!(MOON_DIR, -EARTH_DIR);
        // L1 between them; the Moon's orbit passes through it and through L4 and L5, at the Moon's
        // distance from Earth (the triangles are equilateral).
        let (e, m) = (earth(), moon());
        assert!((e.distance(m) - EARTH_MOON).abs() < 2.0 * KM);
        for l in [Lagrange::L4, Lagrange::L5] {
            let p = l.pos();
            assert!((p.distance(e) - EARTH_MOON).abs() < 2.0 * KM, "{l:?}");
            assert!((p.distance(m) - EARTH_MOON).abs() < 2.0 * KM, "{l:?}");
            assert!((p - e).dot(orbit_normal()).abs() < 1.0 * KM, "{l:?} off the orbit's plane");
        }
        // L4 leads: the Moon moves toward it.
        let ahead = orbit_normal().cross(m - barycentre());
        assert!(ahead.dot(Lagrange::L4.pos() - m) > 0.0 && ahead.dot(Lagrange::L5.pos() - m) < 0.0);
        // The Sun lies in the orbit's plane, near enough.
        assert!(orbit_normal().dot(SUN_DIR).abs() < 1e-4);
        assert!((moon_orbit_at(0.0) - m).length() < 1.0 * KM);
        assert!((moon_orbit_at(1.0).distance(barycentre()) - m.distance(barycentre())).abs() < 1.0 * KM);
    }
}
