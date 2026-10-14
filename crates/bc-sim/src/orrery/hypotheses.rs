//! What a threat might do next: burn its thrusters along one body axis, or coast. On the ground,
//! its legs move it along the ground, and up is a hop.

use glam::{Quat, Vec3};

use crate::content::FrameSpec;
use crate::ground::{GROUND_ACCEL, JUMP_SPEED};
use crate::math::clamp_len;

/// A hop off the ground, as a maneuver: its take-off speed reached over this long, s.
const HOP_SECS: f32 = 1.0;

pub const N_HYP: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Maneuver {
    Coast = 0,
    Forward = 1,
    Back = 2,
    Left = 3,
    Right = 4,
    Up = 5,
    Down = 6,
}

impl Maneuver {
    pub const ALL: [Maneuver; N_HYP] = [
        Maneuver::Coast,
        Maneuver::Forward,
        Maneuver::Back,
        Maneuver::Left,
        Maneuver::Right,
        Maneuver::Up,
        Maneuver::Down,
    ];

    pub fn from_index(i: usize) -> Maneuver {
        Self::ALL[i.min(N_HYP - 1)]
    }

    /// Unit thrust direction in the suit's local frame.
    pub fn local_dir(self) -> Vec3 {
        match self {
            Maneuver::Coast => Vec3::ZERO,
            Maneuver::Forward => Vec3::Z,
            Maneuver::Back => Vec3::NEG_Z,
            Maneuver::Left => Vec3::NEG_X,
            Maneuver::Right => Vec3::X,
            Maneuver::Up => Vec3::Y,
            Maneuver::Down => Vec3::NEG_Y,
        }
    }

    /// Short HUD label.
    pub fn label(self) -> &'static str {
        match self {
            Maneuver::Coast => "COAST",
            Maneuver::Forward => "FWD",
            Maneuver::Back => "BACK",
            Maneuver::Left => "LEFT",
            Maneuver::Right => "RIGHT",
            Maneuver::Up => "UP",
            Maneuver::Down => "DOWN",
        }
    }

    /// Label for the pilot's own evasive recommendation.
    pub fn own_label(self) -> &'static str {
        match self {
            Maneuver::Coast => "HOLD",
            Maneuver::Forward => "PUSH",
            Maneuver::Back => "BREAK-BACK",
            Maneuver::Left => "BREAK-LEFT",
            Maneuver::Right => "BREAK-RIGHT",
            Maneuver::Up => "BREAK-HIGH",
            Maneuver::Down => "BREAK-LOW",
        }
    }
}

/// World-frame acceleration of maneuver `m` at full thrust for a suit with orientation `rot`.
pub fn accel(spec: &FrameSpec, rot: Quat, m: Maneuver) -> Vec3 {
    let d = m.local_dir();
    if d == Vec3::ZERO {
        return Vec3::ZERO;
    }
    rot * d * spec.max_accel_along(d)
}

/// [`accel`] for a suit standing on a body, the ground's normal under it `surface_n` (sector frame;
/// `Vec3::ZERO` for a suit off the ground, which gets [`accel`]'s). Every maneuver is along the
/// ground, no harder than its legs push ([`GROUND_ACCEL`]); down is into the ground, so it's no
/// maneuver at all, and up is a hop.
pub fn accel_on(spec: &FrameSpec, rot: Quat, surface_n: Vec3, m: Maneuver) -> Vec3 {
    if surface_n == Vec3::ZERO {
        return accel(spec, rot, m);
    }
    match m {
        Maneuver::Coast | Maneuver::Down => Vec3::ZERO,
        Maneuver::Up => surface_n * (JUMP_SPEED / HOP_SECS),
        _ => {
            let a = accel(spec, rot, m);
            clamp_len(a - surface_n * a.dot(surface_n), GROUND_ACCEL)
        }
    }
}
