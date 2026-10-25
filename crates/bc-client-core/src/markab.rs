//! The Markab, the colony's rocket car (`bc_sim::markab`, shared with the tick: its fan, its
//! thrusters, the faces it drives and the state it drives on from), as the pilot sees it: from the
//! driver's seat ([`EYE`]) or the chase camera behind it ([`Chase`]).

use glam::Vec3;

pub use bc_sim::markab::*;

/// The driver's eye: above its middle, and back from it, m.
pub const EYE: (f32, f32) = (0.42, 0.3);

/// The chase camera behind a Markab, eased: upright to the face its wheels are on (in the air, to
/// the colony's floor), looking the way it heads (so a roll doesn't swing it round), and pulled in
/// short of anything between it and the car. The game's and the showcase's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chase {
    pub up: Vec3,
    pub ahead: Vec3,
    /// How far back it is, m.
    pub reach: f32,
}

impl Chase {
    /// How far back from the car it sits, and how far up.
    pub const BACK: f32 = 7.0;
    pub const UP: f32 = 2.4;

    /// Right behind `m`.
    pub fn new(m: &Markab) -> Self {
        Self {
            up: if m.wheels_down() { m.up() } else { Vec3::Y },
            ahead: m.forward(),
            reach: Self::BACK.hypot(Self::UP),
        }
    }

    /// Eases it `dt` seconds on after `m`.
    pub fn follow(&mut self, m: &Markab, dt: f32) {
        let want_up = if m.wheels_down() { m.up() } else { Vec3::Y };
        let ease = |rate: f32| 1.0 - (-dt * rate).exp();
        let up = self.up.lerp(want_up, ease(5.0)).normalize_or(want_up);
        let f = m.forward();
        let flat = f - up * f.dot(up);
        let ahead =
            if flat.length() > 0.2 { self.ahead.lerp(flat.normalize(), ease(5.0)) } else { self.ahead };
        let ahead = (ahead - up * ahead.dot(up)).normalize_or(self.ahead);
        let full = Self::BACK.hypot(Self::UP);
        let dir = (up * Self::UP - ahead * Self::BACK).normalize_or(Vec3::Y);
        let near = Near::gather(m.strip, m.pos, full + 1.0);
        let room = near.ray(m.pos, dir, full).map_or(full, |hit| (hit.t - 0.4).max(1.0));
        let reach = if room < self.reach { room } else { self.reach + (room - self.reach) * ease(2.0) };
        *self = Self { up, ahead, reach };
    }

    /// Where it looks from and at, and its up, in the walker's frame on `m`'s strip.
    pub fn view(&self, m: &Markab) -> (Vec3, Vec3, Vec3) {
        let dir = (self.up * Self::UP - self.ahead * Self::BACK).normalize_or(Vec3::Y);
        (m.pos + dir * self.reach, m.pos + self.ahead * 3.0 + self.up * 0.6, self.up)
    }
}
