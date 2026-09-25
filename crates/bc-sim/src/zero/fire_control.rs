//! Fire control: where to aim so a projectile meets a moving (possibly accelerating) target.
//! Projectiles inherit the shooter's velocity, so the problem is solved in the shooter's frame.

use glam::Vec3;

use crate::math::{length, normalize_or, sqrt};

#[derive(Clone, Copy, Debug)]
pub struct Solution {
    /// Unit aim direction.
    pub dir: Vec3,
    /// Time to impact, s.
    pub t: f32,
}

/// Solves `|D + V·t + ½·A·t²| = s·t` for the earliest positive `t`, where `D` and `V` are the
/// target's position and velocity relative to the shooter. The linear case is solved exactly;
/// acceleration is handled with a few fixed-point iterations from that start.
pub fn intercept(
    shooter: Vec3,
    shooter_vel: Vec3,
    speed: f32,
    target: Vec3,
    target_vel: Vec3,
    target_accel: Vec3,
) -> Option<Solution> {
    if speed <= 0.0 {
        return None;
    }
    let d = target - shooter;
    let v = target_vel - shooter_vel;
    let a = v.dot(v) - speed * speed;
    let b = 2.0 * d.dot(v);
    let c = d.dot(d);
    let mut t = if a.abs() < 1e-6 {
        if b.abs() < 1e-6 { return None } else { -c / b }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return None;
        }
        let sq = sqrt(disc);
        let t1 = (-b - sq) / (2.0 * a);
        let t2 = (-b + sq) / (2.0 * a);
        match (t1 > 0.0, t2 > 0.0) {
            (true, true) => t1.min(t2),
            (true, false) => t1,
            (false, true) => t2,
            _ => return None,
        }
    };
    if !(t.is_finite() && t > 0.0) {
        return None;
    }
    if target_accel != Vec3::ZERO {
        for _ in 0..5 {
            t = length(d + v * t + target_accel * (0.5 * t * t)) / speed;
        }
    }
    let aim_point = d + v * t + target_accel * (0.5 * t * t);
    Some(Solution { dir: normalize_or(aim_point, normalize_or(d, Vec3::Z)), t })
}

/// Miss distance of a shot fired along `dir` against a target following
/// `pos + vel·t + ½·accel·t²`: the relative separation at the time of closest approach (exact for
/// constant velocity, Newton-refined under acceleration).
pub fn miss_distance(
    shooter: Vec3,
    shooter_vel: Vec3,
    speed: f32,
    dir: Vec3,
    target: Vec3,
    target_vel: Vec3,
    target_accel: Vec3,
) -> f32 {
    let d = target - shooter;
    let w = target_vel - shooter_vel - dir * speed;
    let ww = w.dot(w);
    let mut t = if ww > 1e-9 { (-d.dot(w) / ww).max(0.0) } else { 0.0 };
    if target_accel != Vec3::ZERO {
        for _ in 0..4 {
            let r = d + w * t + target_accel * (0.5 * t * t);
            let r1 = w + target_accel * t;
            let denom = r1.dot(r1) + r.dot(target_accel);
            if denom.abs() < 1e-9 {
                break;
            }
            t = (t - r.dot(r1) / denom).max(0.0);
        }
    }
    length(d + w * t + target_accel * (0.5 * t * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_constant_velocity_target() {
        let shooter = Vec3::new(0.0, 0.0, 0.0);
        let target = Vec3::new(1_500.0, 200.0, 900.0);
        let tv = Vec3::new(-120.0, 35.0, 80.0);
        let s = intercept(shooter, Vec3::ZERO, 4_000.0, target, tv, Vec3::ZERO).unwrap();
        let hit = target + tv * s.t;
        let shot = s.dir * 4_000.0 * s.t;
        assert!((hit - shot).length() < 0.05, "miss {}", (hit - shot).length());
    }
}
