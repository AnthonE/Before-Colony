//! Small CPU noise functions for procedural geometry (rocks, the colony). Mirrors the hash and
//! value noise the shaders use, though nothing needs them to match bit for bit.

use bevy::math::Vec3;

/// A hash of a 3D point to [0, 1).
pub fn hash13(p: Vec3) -> f32 {
    let mut q = (p * 0.1031).fract();
    q += q.dot(Vec3::new(q.z, q.y, q.x) + 31.32);
    ((q.x + q.y) * q.z).fract()
}

/// Smooth value noise in [0, 1].
pub fn noise3(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let h = |x: f32, y: f32, z: f32| hash13(i + Vec3::new(x, y, z));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let a = lerp(h(0.0, 0.0, 0.0), h(1.0, 0.0, 0.0), u.x);
    let b = lerp(h(0.0, 1.0, 0.0), h(1.0, 1.0, 0.0), u.x);
    let c = lerp(h(0.0, 0.0, 1.0), h(1.0, 0.0, 1.0), u.x);
    let d = lerp(h(0.0, 1.0, 1.0), h(1.0, 1.0, 1.0), u.x);
    lerp(lerp(a, b, u.y), lerp(c, d, u.y), u.z)
}

/// Fractal value noise, normalised to about [0, 1].
pub fn fbm(p: Vec3, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut q) = (0.0, 0.5, 0.0, p);
    for _ in 0..octaves {
        sum += amp * noise3(q);
        norm += amp;
        q = q * 2.03 + Vec3::new(1.7, 9.2, 3.1);
        amp *= 0.5;
    }
    sum / norm
}
