//! The colony's frames. Three are in play:
//!
//! - The **sector's**, where suits fly: the colony's centre is [`COLONY_CENTER`] and it turns about
//!   +X by [`colony_spin_angle`].
//! - The **colony's own**, turning with it: the same axes at spin angle 0, centred on the axis. Inside
//!   it nothing moves, so the city is drawn and walked in it. An angle round the axis is measured from
//!   +Y towards +Z: the point at angle `a` and radius `r` is `(x, r cos a, r sin a)`.
//! - **City coordinates** ([`CityPos`]): the cylinder unrolled. A land strip is `s` metres across from
//!   its edge (the angle grows with it), `x` along the axis, `h` up from the floor towards the axis.
//!   Plumb is radial, so a building whose walls are plumb is a box in city coordinates; it is bent
//!   onto the cylinder only where it's drawn.
//!
//! Three windows alternate with three land strips round the hull, each 60° wide: window `k` is
//! centred on [`window_centre`]`(k)` and land strip `k` follows it.
//!
//! On foot the walker uses a right-handed frame with Y up: `(x, h, −s)`, so that the axis (+X) and
//! up (+Y) give −s for +Z, as `local_frame` turns it.

use core::f32::consts::{FRAC_PI_3, FRAC_PI_6, PI, TAU};

use glam::{Quat, Vec3};

use crate::config::DT;
use crate::math::{atan2, cos, floor, quat_axis_angle, sin, sqrt};
use crate::world::{
    COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS, COLONY_SPIN_PERIOD_TICKS, colony_spin_angle,
};

/// Land strips (and windows) round the hull.
pub const STRIPS: usize = 3;
/// Centre of the first window, rad round the axis from +Y towards +Z.
pub const FIRST_WINDOW: f32 = 0.4;
/// A strip's (or a window's) arc, rad.
pub const STRIP_ARC: f32 = FRAC_PI_3;
/// A land strip's width at the floor, m (3,351 m).
pub const STRIP_WIDTH: f32 = COLONY_RADIUS * STRIP_ARC;
/// How fast the colony turns, rad/s: 1 g at the floor.
pub const SPIN_RATE: f32 = TAU / (COLONY_SPIN_PERIOD_TICKS as f32 * DT);

/// The centre of window `k`, rad.
pub fn window_centre(k: usize) -> f32 {
    FIRST_WINDOW + k as f32 * (TAU / STRIPS as f32)
}

/// The edge of land strip `k` where `s` is 0 (the far side of window `k`), rad.
pub fn strip_edge(k: usize) -> f32 {
    window_centre(k) + FRAC_PI_6
}

/// The middle of land strip `k`, rad.
pub fn strip_centre(k: usize) -> f32 {
    strip_edge(k) + FRAC_PI_6
}

/// The window opposite land strip `k`, overhead from it: the one its light comes in by.
pub fn window_over(k: usize) -> usize {
    (k + 2) % STRIPS
}

/// A place on a land strip.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CityPos {
    pub strip: u8,
    /// Along the axis, m (the colony's own x: −16,000 at the docking hub's end cap).
    pub x: f32,
    /// Across the strip from its edge, m at the floor: 0..[`STRIP_WIDTH`].
    pub s: f32,
    /// Up from the floor, m.
    pub h: f32,
}

impl CityPos {
    pub const fn new(strip: u8, x: f32, s: f32, h: f32) -> Self {
        Self { strip, x, s, h }
    }

    /// The angle round the axis, rad.
    pub fn angle(&self) -> f32 {
        strip_edge(self.strip as usize) + self.s / COLONY_RADIUS
    }

    /// Where it is in the colony's own frame.
    pub fn to_colony(&self) -> Vec3 {
        let a = self.angle();
        let r = COLONY_RADIUS - self.h;
        Vec3::new(self.x, r * cos(a), r * sin(a))
    }

    /// In the walker's frame on its strip: `(x, h, −s)`.
    pub fn walker(&self) -> Vec3 {
        Vec3::new(self.x, self.h, -self.s)
    }

    /// Back from the walker's frame on strip `strip`.
    pub fn from_walker(strip: u8, w: Vec3) -> Self {
        Self { strip, x: w.x, s: -w.z, h: w.y }
    }
}

/// What lies under a point inside the colony, looking out from the axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Under {
    Land(CityPos),
    /// Window `k`, `s` metres across it (at the hull) from its edge where its angle is least, and `h`
    /// up from the glass.
    Window {
        k: u8,
        s: f32,
        h: f32,
    },
}

/// What's under a point of the colony's own frame, and how far up from it the point is.
pub fn from_colony(p: Vec3) -> Under {
    let r = sqrt(p.y * p.y + p.z * p.z);
    let h = COLONY_RADIUS - r;
    // Round from the first window's near edge: each 120° is a window and then a strip.
    let rel = atan2(p.z, p.y) - (FIRST_WINDOW - FRAC_PI_6);
    let rel = rel - floor(rel / TAU) * TAU;
    let span = TAU / STRIPS as f32;
    let k = (floor(rel / span) as usize).min(STRIPS - 1);
    let within = rel - k as f32 * span;
    if within < STRIP_ARC {
        Under::Window { k: k as u8, s: within * COLONY_RADIUS, h }
    } else {
        Under::Land(CityPos { strip: k as u8, x: p.x, s: (within - STRIP_ARC) * COLONY_RADIUS, h })
    }
}

/// The colony's own frame at the sector's tick `t` plus `frac`: its rotation and centre.
pub fn colony_pose(t: u32, frac: f32) -> (Quat, Vec3) {
    (quat_axis_angle(Vec3::X, colony_spin_angle(t, frac)), COLONY_CENTER)
}

/// A point of the colony's own frame in the sector at tick `t` plus `frac`.
pub fn colony_to_sector(p: Vec3, t: u32, frac: f32) -> Vec3 {
    let (rot, centre) = colony_pose(t, frac);
    centre + rot * p
}

/// A point of the sector in the colony's own frame at tick `t` plus `frac`.
pub fn sector_to_colony(p: Vec3, t: u32, frac: f32) -> Vec3 {
    let (rot, centre) = colony_pose(t, frac);
    rot.conjugate() * (p - centre)
}

/// The colony's pull at `h` metres up from the floor, m/s²: 1 g at the floor, none at the axis.
pub fn gravity(h: f32) -> f32 {
    SPIN_RATE * SPIN_RATE * (COLONY_RADIUS - h)
}

/// How long a metre of `s` is at height `h`, m: the strip narrows towards the axis.
pub fn s_scale(h: f32) -> f32 {
    (COLONY_RADIUS - h) / COLONY_RADIUS
}

/// Turns the walker's frame on strip `strip` at `s` into the colony's own: +X stays the axis, +Y
/// becomes up (towards the axis) there, +Z becomes −s.
pub fn local_frame(strip: u8, s: f32) -> Quat {
    quat_axis_angle(Vec3::X, strip_edge(strip as usize) + s / COLONY_RADIUS + PI)
}

/// Up (towards the axis) at a point of the colony's own frame off the axis.
pub fn up_at(p: Vec3) -> Vec3 {
    let r = sqrt(p.y * p.y + p.z * p.z);
    if r < 1e-6 { Vec3::Y } else { Vec3::new(0.0, -p.y / r, -p.z / r) }
}

/// Whether `x` is between the end caps.
pub fn within_caps(x: f32) -> bool {
    x.abs() <= COLONY_HALF_LENGTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::G0;
    use crate::math::length;

    #[test]
    fn the_spin_is_worlds_and_one_g_at_the_floor() {
        assert!((SPIN_RATE * COLONY_SPIN_PERIOD_TICKS as f32 * DT - TAU).abs() < 1e-5);
        assert!((gravity(0.0) / G0 - 1.0).abs() < 1e-4, "{} g", gravity(0.0) / G0);
        // The bays hang in the docking hub's ring at 0.7 g: 948 m up, r 2,252 m.
        let bays = gravity(COLONY_RADIUS - 2_252.0) / G0;
        assert!((bays - 0.7).abs() < 0.005, "{bays} g in the bay ring");
        assert_eq!(gravity(COLONY_RADIUS), 0.0);
        assert!((STRIP_WIDTH - 3_351.03).abs() < 0.01);
    }

    #[test]
    fn city_positions_go_round_trip_on_every_strip() {
        for k in 0..STRIPS as u8 {
            for i in 0..=20 {
                for h in [-20.0f32, 0.0, 1.62, 45.0, 400.0] {
                    let s = STRIP_WIDTH * i as f32 / 20.0;
                    let p =
                        CityPos::new(k, -15_900.0 + 1_590.0 * i as f32, s.clamp(0.01, STRIP_WIDTH - 0.01), h);
                    let Under::Land(q) = from_colony(p.to_colony()) else {
                        panic!("{p:?} isn't over land");
                    };
                    assert_eq!(q.strip, k);
                    assert!((q.x - p.x).abs() < 1e-3);
                    assert!((q.s - p.s).abs() < 2e-3, "{p:?} → {q:?}");
                    assert!((q.h - p.h).abs() < 2e-3, "{p:?} → {q:?}");
                    let w = CityPos::from_walker(k, p.walker());
                    assert_eq!(w, p);
                }
            }
        }
    }

    #[test]
    fn windows_and_strips_alternate_as_the_window_shader_has_them() {
        for k in 0..STRIPS {
            let mid = window_centre(k);
            let at = |a: f32| Vec3::new(0.0, 3_000.0 * cos(a), 3_000.0 * sin(a));
            assert!(matches!(from_colony(at(mid)), Under::Window { k: w, .. } if w as usize == k));
            assert!(matches!(from_colony(at(strip_centre(k))), Under::Land(p) if p.strip as usize == k));
            // The window overhead is the opposite one.
            let over = window_centre(window_over(k));
            let d = (over - strip_centre(k)).rem_euclid(TAU);
            assert!((d - PI).abs() < 1e-5, "{d}");
        }
    }

    #[test]
    fn walls_are_plumb_and_the_walkers_frame_turns_right_handed() {
        for k in 0..STRIPS as u8 {
            for s in [0.0f32, 900.0, STRIP_WIDTH] {
                let foot = CityPos::new(k, 100.0, s, 0.0).to_colony();
                let roof = CityPos::new(k, 100.0, s, 200.0).to_colony();
                let up = up_at(foot);
                // Straight up the wall is straight towards the axis.
                assert!((roof - foot).normalize().dot(up) > 1.0 - 1e-6);
                let q = local_frame(k, s);
                assert!((q * Vec3::Y - up).length() < 1e-5, "up");
                assert!((q * Vec3::X - Vec3::X).length() < 1e-5, "the axis");
                // +Z is −s: a step across the strip, the other way.
                let across = CityPos::new(k, 100.0, s + 1.0, 0.0).to_colony() - foot;
                assert!((q * Vec3::Z + across / length(across)).length() < 1e-3, "−s");
                // A right-handed turn, not a mirror.
                assert!(((q * Vec3::X).cross(q * Vec3::Y) - q * Vec3::Z).length() < 1e-5);
            }
        }
    }

    #[test]
    fn the_sector_and_the_colonys_frame_agree_with_the_spin() {
        let p = CityPos::new(1, -2_000.0, 1_200.0, 30.0).to_colony();
        for t in [0u32, 1, 1_000, 3_404, 3_405, 999_999] {
            let w = colony_to_sector(p, t, 0.25);
            let back = sector_to_colony(w, t, 0.25);
            assert!((back - p).length() < 5e-3, "{t}: {back} vs {p}");
            // The axis doesn't move.
            assert!(
                (colony_to_sector(Vec3::new(500.0, 0.0, 0.0), t, 0.0) - (COLONY_CENTER + Vec3::X * 500.0))
                    .length()
                    < 1e-3
            );
        }
        // A whole turn later it's where it was, to the bit.
        assert_eq!(colony_to_sector(p, 7, 0.5), colony_to_sector(p, 7 + COLONY_SPIN_PERIOD_TICKS, 0.5));
    }

    #[test]
    fn the_strip_narrows_towards_the_axis() {
        assert_eq!(s_scale(0.0), 1.0);
        assert!((s_scale(320.0) - 0.9).abs() < 1e-6);
        assert!(within_caps(-16_000.0) && !within_caps(16_000.5));
    }
}
