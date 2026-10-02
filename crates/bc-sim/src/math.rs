//! Deterministic math helpers.
//!
//! The server (native, `glam/scalar-math`) and the browser client (wasm32, scalar) must step the
//! flight model identically. IEEE `+ - * / sqrt` are exact everywhere; transcendental functions are
//! not, so every one we use goes through the pure-Rust `libm` crate here, never `std` or glam's
//! internal choice.

use glam::{Quat, Vec3};

#[inline]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}
#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}
#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}
#[inline]
pub fn acos(x: f32) -> f32 {
    libm::acosf(x.clamp(-1.0, 1.0))
}
#[inline]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}
#[inline]
pub fn floor(x: f32) -> f32 {
    libm::floorf(x)
}
#[inline]
pub fn cbrt(x: f32) -> f32 {
    libm::cbrtf(x)
}

/// Length that never divides by zero.
#[inline]
pub fn length(v: Vec3) -> f32 {
    sqrt(v.dot(v))
}

/// `v / |v|`, or `fallback` for (near-)zero vectors.
#[inline]
pub fn normalize_or(v: Vec3, fallback: Vec3) -> Vec3 {
    let l2 = v.dot(v);
    if l2 > 1e-12 { v / sqrt(l2) } else { fallback }
}

/// Unsigned angle between two unit vectors.
#[inline]
pub fn angle_between(a: Vec3, b: Vec3) -> f32 {
    atan2(length(a.cross(b)), a.dot(b))
}

/// Clamps `dir` into a cone of half-angle `cone` around `axis` (all unit vectors): how far off a
/// suit's nose an arm can point its weapon.
pub fn clamp_to_cone(dir: Vec3, axis: Vec3, cone: f32) -> Vec3 {
    let a = angle_between(axis, dir);
    if a <= cone {
        return dir;
    }
    let perp = normalize_or(dir - axis * axis.dot(dir), Vec3::Y);
    normalize_or(axis * cos(cone) + perp * sin(cone), axis)
}

/// A direction within the cone of half-angle `half` about `dir` (a unit vector), from two numbers
/// in [0, 1): `u` how far off (by the cone's cross-section, so shots fill it evenly rather than
/// bunching on the axis), `v` which way round. A weapon's spread: its shots land within the ring
/// the HUD draws.
pub fn within_cone(dir: Vec3, half: f32, u: f32, v: f32) -> Vec3 {
    let off = half * sqrt(u);
    let around = v * core::f32::consts::TAU;
    let side = normalize_or(dir.cross(if dir.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }), Vec3::X);
    let up = dir.cross(side);
    normalize_or(dir * cos(off) + (side * cos(around) + up * sin(around)) * sin(off), dir)
}

/// Normalizes a quaternion with our sqrt.
#[inline]
pub fn quat_normalize(q: Quat) -> Quat {
    let l2 = q.length_squared();
    if l2 > 1e-12 { q * (1.0 / sqrt(l2)) } else { Quat::IDENTITY }
}

/// Rotation of `angle` radians about unit `axis` (libm trig).
pub fn quat_axis_angle(axis: Vec3, angle: f32) -> Quat {
    let (s, c) = (sin(angle * 0.5), cos(angle * 0.5));
    Quat::from_xyzw(axis.x * s, axis.y * s, axis.z * s, c)
}

/// Integrates orientation by world-frame angular velocity `w` over `dt`.
#[inline]
pub fn integrate_rotation(q: Quat, w: Vec3, dt: f32) -> Quat {
    let wq = Quat::from_xyzw(w.x, w.y, w.z, 0.0);
    let dq = wq * q;
    quat_normalize(Quat::from_xyzw(
        q.x + 0.5 * dt * dq.x,
        q.y + 0.5 * dt * dq.y,
        q.z + 0.5 * dt * dq.z,
        q.w + 0.5 * dt * dq.w,
    ))
}

/// Turns `q` toward `target` by at most `max_angle` radians, the shorter way round: `target` itself
/// if it is that close.
pub fn quat_rotate_toward(q: Quat, target: Quat, max_angle: f32) -> Quat {
    let mut d = target * q.conjugate();
    if d.w < 0.0 {
        d = -d;
    }
    let angle = 2.0 * acos(d.w.min(1.0));
    if angle <= max_angle {
        return quat_normalize(target);
    }
    quat_normalize(quat_axis_angle(normalize_or(Vec3::new(d.x, d.y, d.z), Vec3::Y), max_angle) * q)
}

/// The rotation `dq` as its axis times its angle (rad), the shorter way round.
pub fn rotation_vector(dq: Quat) -> Vec3 {
    let dq = if dq.w < 0.0 { -dq } else { dq };
    let v = Vec3::new(dq.x, dq.y, dq.z);
    let s = length(v);
    if s > 1e-9 { v * (2.0 * atan2(s, dq.w) / s) } else { v * 2.0 }
}

/// `v`, shortened to length `max` if it's longer.
#[inline]
pub fn clamp_len(v: Vec3, max: f32) -> Vec3 {
    let l2 = v.dot(v);
    if l2 > max * max { v * (max / sqrt(l2)) } else { v }
}

/// Rotation that looks along `forward` with `up` as the approximate up vector (+Z forward, +Y up).
pub fn look_rotation(forward: Vec3, up: Vec3) -> Quat {
    let f = normalize_or(forward, Vec3::Z);
    let r = normalize_or(up.cross(f), if f.y.abs() < 0.99 { Vec3::Y.cross(f) } else { Vec3::X });
    let r = normalize_or(r, Vec3::X);
    let u = f.cross(r);
    quat_normalize(Quat::from_mat3(&glam::Mat3::from_cols(r, u, f)))
}

/// Softmax over `scores` (in place) with temperature `temp`.
pub fn softmax(scores: &mut [f32], temp: f32) {
    let mut max = f32::NEG_INFINITY;
    for s in scores.iter() {
        max = max.max(*s);
    }
    let mut sum = 0.0;
    for s in scores.iter_mut() {
        *s = exp((*s - max) / temp.max(1e-3));
        sum += *s;
    }
    if sum > 0.0 {
        for s in scores.iter_mut() {
            *s /= sum;
        }
    }
}

/// Jev's confidence statistic for a distribution: `(n·peak − 1) / (n − 1)` (0 = uniform, 1 = certain).
pub fn confidence(probs: &[f32]) -> f32 {
    let n = probs.len() as f32;
    if n < 2.0 {
        return 1.0;
    }
    let mut peak = 0.0f32;
    for p in probs {
        peak = peak.max(*p);
    }
    ((n * peak - 1.0) / (n - 1.0)).clamp(0.0, 1.0)
}

/// Small deterministic PRNG (xorshift64*), for noise that must match on every machine.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

/// Stateless hash → [0, 1), for per-(tick, entity) noise without carrying RNG state.
pub fn hash01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The angle between two orientations, rad.
    fn apart(a: Quat, b: Quat) -> f32 {
        let d = b * a.conjugate();
        2.0 * atan2(length(Vec3::new(d.x, d.y, d.z)), d.w.abs())
    }

    /// A spread's shots stay within its cone and fill it evenly: half of them in the inner 71% of
    /// its angle (half its cross-section), and as many to each side.
    #[test]
    fn within_cone_fills_the_cone_evenly() {
        let mut rng = Rng::new(0xC0DE);
        for _ in 0..50 {
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Z);
            let half = 0.001 + rng.next_f32() * 0.2;
            let (mut inner, mut right, n) = (0, 0, 2_000);
            let side = normalize_or(dir.cross(Vec3::new(0.3, 0.8, -0.5)), Vec3::X);
            for _ in 0..n {
                let d = within_cone(dir, half, rng.next_f32(), rng.next_f32());
                let off = angle_between(dir, d);
                assert!(off <= half * 1.001 + 1e-4, "{off} outside {half}");
                assert!((length(d) - 1.0).abs() < 1e-5);
                inner += u32::from(off < half * core::f32::consts::FRAC_1_SQRT_2);
                right += u32::from(d.dot(side) > 0.0);
            }
            let share = |k: u32| k as f32 / n as f32;
            assert!((share(inner) - 0.5).abs() < 0.05, "inner {}", share(inner));
            assert!((share(right) - 0.5).abs() < 0.05, "one side {}", share(right));
        }
    }

    fn random_rotation(rng: &mut Rng) -> Quat {
        let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
        quat_axis_angle(axis, rng.next_f32() * core::f32::consts::TAU)
    }

    #[test]
    fn rotate_toward_reaches_a_target_within_max_angle() {
        let mut rng = Rng::new(1);
        for _ in 0..1_000 {
            let q = random_rotation(&mut rng);
            let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let target = quat_normalize(quat_axis_angle(axis, rng.next_f32() * 0.5) * q);
            assert_eq!(quat_rotate_toward(q, target, 0.5 + 1e-3), quat_normalize(target));
        }
    }

    #[test]
    fn rotate_toward_never_overshoots() {
        let mut rng = Rng::new(2);
        for _ in 0..1_000 {
            let (q, target) = (random_rotation(&mut rng), random_rotation(&mut rng));
            let step = 0.01 + rng.next_f32();
            let gap = apart(q, target);
            let r = quat_rotate_toward(q, target, step);
            assert!((r.length() - 1.0).abs() < 1e-5);
            assert!(apart(q, r) <= step.min(gap) + 2e-3, "turned {} for a step of {step}", apart(q, r));
            // Along the way: what's left is what there was less the step.
            assert!(
                (apart(r, target) - (gap - step).max(0.0)).abs() < 2e-3,
                "{gap} less {step} left {}",
                apart(r, target)
            );
        }
    }

    #[test]
    fn rotate_toward_goes_the_short_way_whatever_the_sign() {
        let mut rng = Rng::new(3);
        for _ in 0..1_000 {
            let target = random_rotation(&mut rng);
            // The same orientation with the other sign: already there.
            assert!(apart(quat_rotate_toward(-target, target, 0.1), target) < 1e-3);
            // Near it, with the other sign: closer, not the long way round.
            let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let q = -quat_normalize(quat_axis_angle(axis, 0.8) * target);
            let r = quat_rotate_toward(q, target, 0.1);
            assert!((apart(r, target) - 0.7).abs() < 2e-3, "{} left", apart(r, target));
        }
    }

    #[test]
    fn rotation_vectors_are_axis_times_angle() {
        let mut rng = Rng::new(4);
        for _ in 0..1_000 {
            let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let angle = rng.signed() * 3.1;
            let q = quat_axis_angle(axis, angle);
            assert!((rotation_vector(q) - axis * angle).length() < 1e-4);
            assert!((rotation_vector(-q) - axis * angle).length() < 1e-4, "the sign doesn't matter");
        }
        assert_eq!(rotation_vector(Quat::IDENTITY), Vec3::ZERO);
        let tiny = quat_axis_angle(Vec3::X, 1e-7);
        assert!((rotation_vector(tiny) - Vec3::X * 1e-7).length() < 1e-12);
    }

    #[test]
    fn clamp_len_only_shortens() {
        assert_eq!(clamp_len(Vec3::new(3.0, 4.0, 0.0), 10.0), Vec3::new(3.0, 4.0, 0.0));
        assert!((clamp_len(Vec3::new(3.0, 4.0, 0.0), 1.0) - Vec3::new(0.6, 0.8, 0.0)).length() < 1e-6);
        assert_eq!(clamp_len(Vec3::ZERO, 0.0), Vec3::ZERO);
    }
}
