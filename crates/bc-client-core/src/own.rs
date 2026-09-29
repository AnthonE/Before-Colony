//! The own suit as drawn: between the ticks the prediction has flown, a tick behind the input
//! clock as it eases along, with corrections blended in rather than snapped.
//!
//! When a snapshot puts the prediction right, the suit's drawn place and motion are kept and the
//! difference is blended out on a critically damped spring, so a correction bends the suit's
//! path without ever jerking it. Only a new life, or a relocation too big to be a misprediction,
//! cuts.

use bc_proto::FrameId;
use glam::{Quat, Vec3};

use crate::chase::spring;
use crate::predict::{OwnPose, RELOCATION};

/// How fast a correction is blended out (rad/s): about 0.3 s to a tenth of it.
const MEND: f32 = 12.0;
/// The G shown settles over about this long (s).
const G_SMOOTH: f32 = 0.15;
/// Corrections measured longer than this after the last frame (s) aren't blended but cut to: the
/// page wasn't drawing (a background tab), so there's no motion on screen to keep.
const STALE: f64 = 0.25;

/// The own suit as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnView {
    /// The input clock's time drawn (ticks): a tick behind the newest command, as eased.
    pub t: f64,
    pub pos: Vec3,
    pub rot: Quat,
    /// How fast the drawn suit moves, m/s: its flight, the clock's easing and any correction
    /// being blended out. What a camera following it should keep pace with.
    pub vel: Vec3,
    /// The suit's velocity through space (the flight model's), m/s.
    pub flight_vel: Vec3,
    /// Felt acceleration, g, smoothed for reading.
    pub g: f32,
    pub g_strain: f32,
    pub blackout: bool,
    pub boosting: bool,
    /// Thrust applied per local axis, as a fraction of each axis' unboosted maximum.
    pub throttle: Vec3,
    /// Flight assist is holding the pilot's G down.
    pub g_limited: bool,
    /// The form drawn.
    pub frame: FrameId,
    pub alive: bool,
    /// The drawn suit jumped (spawn, respawn, a relocation): a camera should cut, not chase.
    pub cut: bool,
}

/// What carries over between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct Drawn {
    /// Position correction still to blend out, and its rate.
    err: Vec3,
    err_v: Vec3,
    /// Rotation correction still to blend out (scaled axis), and its rate.
    turn: Vec3,
    turn_v: Vec3,
    g: f32,
    /// When the suit was last drawn: the input-clock time drawn, and the local time (s).
    pub last: Option<(f64, f64)>,
    cut: bool,
    pub view: Option<OwnView>,
}

impl Drawn {
    /// The input-clock time last drawn, if it was recent enough at `now` to keep what's on screen.
    pub fn recent(&self, now: f64) -> Option<f64> {
        self.last.filter(|(_, at)| now - at <= STALE).map(|(t, _)| t)
    }

    /// What the suit is drawn from moved under it (`before` → `after`, both at the time last
    /// drawn): keep the drawn suit where it was and as it moved, and blend the difference out.
    pub fn correct(&mut self, before: Option<OwnPose>, after: Option<OwnPose>) {
        match (before, after) {
            (Some(b), Some(a)) if b.pos.distance(a.pos) <= RELOCATION => {
                self.err += b.pos - a.pos;
                self.err_v += b.dpos - a.dpos;
                let q = Quat::from_scaled_axis(self.turn) * b.rot * a.rot.inverse();
                self.turn = if q.w < 0.0 { -q } else { q }.to_scaled_axis();
            }
            (_, Some(_)) => self.cut(),
            _ => {}
        }
    }

    /// Starts over from where the suit is: nothing to blend, and a camera cuts.
    pub fn cut(&mut self) {
        (self.err, self.err_v, self.turn, self.turn_v) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        self.cut = true;
    }

    /// The suit as drawn at `t` from `src`, `dt` s after the last frame, at local time `now`;
    /// `rate`: how fast the clock it's drawn on runs against real time.
    pub fn draw(&mut self, t: f64, now: f64, src: &OwnPose, alive: bool, dt: f32, rate: f32) -> OwnView {
        spring(&mut self.err, &mut self.err_v, Vec3::ZERO, MEND, dt);
        spring(&mut self.turn, &mut self.turn_v, Vec3::ZERO, MEND, dt);
        let cut = std::mem::take(&mut self.cut);
        self.g =
            if cut { src.g_load } else { self.g + (src.g_load - self.g) * (1.0 - (-dt / G_SMOOTH).exp()) };
        let view = OwnView {
            t,
            pos: src.pos + self.err,
            rot: (Quat::from_scaled_axis(self.turn) * src.rot).normalize(),
            vel: src.dpos * rate + self.err_v,
            flight_vel: src.vel,
            g: self.g,
            g_strain: src.g_strain,
            blackout: src.blackout,
            boosting: src.boosting,
            throttle: src.throttle,
            g_limited: src.g_limited,
            frame: src.frame,
            alive,
            cut,
        };
        self.last = Some((t, now));
        self.view = Some(view);
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(pos: Vec3, dpos: Vec3) -> OwnPose {
        OwnPose {
            pos,
            rot: Quat::IDENTITY,
            vel: dpos,
            dpos,
            g_load: 0.0,
            g_strain: 0.0,
            blackout: false,
            boosting: false,
            throttle: Vec3::ZERO,
            g_limited: false,
            frame: FrameId::Leo,
        }
    }

    #[test]
    fn a_correction_bends_the_path_without_a_jerk() {
        // Flying at 300 m/s, drawn at 60 Hz, when the server's news moves the source 3 m back and
        // 20 m/s slower.
        let dt = 1.0 / 60.0;
        let v = Vec3::new(0.0, 0.0, 300.0);
        let old = |t: f32| pose(v * t, v);
        let new = |t: f32| pose(v * t - Vec3::Z * (3.0 + 20.0 * (t - 1.0)), v - Vec3::Z * 20.0);
        let mut d = Drawn::default();
        d.cut();
        let mut t = 0.0f32;
        while t < 1.0 {
            t += dt;
            d.draw(f64::from(t) * 30.0, f64::from(t), &old(t), true, dt, 1.0);
        }
        // At the moment of the news, nothing drawn moves: not where the suit is, nor how it moves.
        let before = d.view.unwrap();
        d.correct(Some(old(t)), Some(new(t)));
        let after = d.draw(before.t, f64::from(t), &new(t), true, 0.0, 1.0);
        assert!(after.pos.distance(before.pos) < 1e-3, "jumped {:?}", after.pos - before.pos);
        assert!(after.vel.distance(before.vel) < 1e-3, "lurched {:?}", after.vel - before.vel);
        // Then it bends onto the new path, never faster than the suit flies.
        let mut prev = after;
        let t0 = t;
        while t < t0 + 0.6 {
            t += dt;
            let now = d.draw(f64::from(t) * 30.0, f64::from(t), &new(t), true, dt, 1.0);
            assert!(
                now.pos.distance(prev.pos) <= v.length() * dt + 1e-3,
                "a step of {:?}",
                now.pos - prev.pos
            );
            prev = now;
        }
        let off = prev.pos.distance(new(t).pos);
        assert!(off < 0.03, "{off} m still to go after 0.6 s");
    }

    #[test]
    fn a_relocation_cuts() {
        let mut d = Drawn::default();
        d.draw(0.0, 0.0, &pose(Vec3::ZERO, Vec3::ZERO), true, 0.016, 1.0);
        d.correct(Some(pose(Vec3::ZERO, Vec3::ZERO)), Some(pose(Vec3::X * 500.0, Vec3::ZERO)));
        let v = d.draw(1.0, 0.016, &pose(Vec3::X * 500.0, Vec3::ZERO), true, 0.016, 1.0);
        assert!(v.cut);
        assert_eq!(v.pos, Vec3::X * 500.0);
        let v = d.draw(2.0, 0.032, &pose(Vec3::X * 500.0, Vec3::ZERO), true, 0.016, 1.0);
        assert!(!v.cut, "a cut lasts a frame");
    }
}
