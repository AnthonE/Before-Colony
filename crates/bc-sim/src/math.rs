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
