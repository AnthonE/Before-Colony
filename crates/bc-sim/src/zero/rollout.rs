//! Forward simulation of maneuver hypotheses (constant acceleration over the horizon).

use glam::Vec3;

pub const STEPS: usize = 15;
pub const STEP_DT: f32 = 0.1;
/// Prediction horizon, s.
pub const HORIZON: f32 = STEPS as f32 * STEP_DT;

/// Position at time `t` under constant acceleration.
#[inline]
pub fn position_at(pos: Vec3, vel: Vec3, accel: Vec3, t: f32) -> Vec3 {
    pos + vel * t + accel * (0.5 * t * t)
}

/// Positions at `STEP_DT, 2·STEP_DT, …, HORIZON`.
pub fn rollout(pos: Vec3, vel: Vec3, accel: Vec3) -> [Vec3; STEPS] {
    let mut out = [Vec3::ZERO; STEPS];
    for (k, o) in out.iter_mut().enumerate() {
        *o = position_at(pos, vel, accel, (k + 1) as f32 * STEP_DT);
    }
    out
}
