//! What a threat might do next: burn its thrusters along one body axis, or coast.

use glam::{Quat, Vec3};

use crate::content::FrameSpec;

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
