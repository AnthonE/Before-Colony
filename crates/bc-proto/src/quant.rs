//! Quantization: floats ↔ fixed-width integers.
//!
//! Every quantizer rounds to the nearest step, so a round trip is within half a step (½ LSB). The
//! proptest suite checks this.

use glam::{Quat, Vec2, Vec3};

use crate::{BitReader, BitWriter, SECTOR_HALF_EXTENT};

#[inline]
fn max_q(bits: u32) -> u32 {
    if bits >= 32 { u32::MAX } else { (1u32 << bits) - 1 }
}

/// Maps `v ∈ [-max_abs, max_abs]` onto `[0, 2^bits - 1]` (values outside are clamped).
///
/// Computed in f64: at 21 bits over ±32 km, f32 intermediates alone would exceed half a step.
#[inline]
pub fn quantize_signed(v: f32, max_abs: f32, bits: u32) -> u32 {
    let m = f64::from(max_q(bits));
    let max_abs = f64::from(max_abs);
    let t = ((f64::from(v) + max_abs) / (2.0 * max_abs)).clamp(0.0, 1.0);
    libm::round(t * m) as u32
}

#[inline]
pub fn dequantize_signed(q: u32, max_abs: f32, bits: u32) -> f32 {
    let m = f64::from(max_q(bits));
    let max_abs = f64::from(max_abs);
    ((f64::from(q.min(max_q(bits))) / m) * (2.0 * max_abs) - max_abs) as f32
}

/// Step size (LSB) of [`quantize_signed`].
#[inline]
pub fn signed_step(max_abs: f32, bits: u32) -> f32 {
    2.0 * max_abs / max_q(bits) as f32
}

/// Maps `v ∈ [0, 1]` onto `[0, 2^bits - 1]`.
#[inline]
pub fn quantize_unit(v: f32, bits: u32) -> u32 {
    libm::round(f64::from(v.clamp(0.0, 1.0)) * f64::from(max_q(bits))) as u32
}

#[inline]
pub fn dequantize_unit(q: u32, bits: u32) -> f32 {
    (f64::from(q.min(max_q(bits))) / f64::from(max_q(bits))) as f32
}

// ---------------------------------------------------------------------------------------------
// Positions and vectors
// ---------------------------------------------------------------------------------------------

/// Bits per axis for replicated entity positions: 65 536 m / 2²¹ ≈ 3.1 cm.
pub const POS_BITS: u32 = 21;
/// Bits per axis for replicated velocities over ±[`VEL_MAX`] m/s (0.25 m/s steps).
pub const VEL_BITS: u32 = 14;
pub const VEL_MAX: f32 = 2048.0;

pub fn write_pos(w: &mut BitWriter<'_>, p: Vec3) {
    for c in p.to_array() {
        w.write_bits(quantize_signed(c, SECTOR_HALF_EXTENT, POS_BITS), POS_BITS);
    }
}

pub fn read_pos(r: &mut BitReader<'_>) -> Vec3 {
    let mut out = [0.0f32; 3];
    for c in &mut out {
        *c = dequantize_signed(r.read_bits(POS_BITS), SECTOR_HALF_EXTENT, POS_BITS);
    }
    Vec3::from_array(out)
}

pub fn write_vec(w: &mut BitWriter<'_>, v: Vec3, max_abs: f32, bits: u32) {
    for c in v.to_array() {
        w.write_bits(quantize_signed(c, max_abs, bits), bits);
    }
}

pub fn read_vec(r: &mut BitReader<'_>, max_abs: f32, bits: u32) -> Vec3 {
    let mut out = [0.0f32; 3];
    for c in &mut out {
        *c = dequantize_signed(r.read_bits(bits), max_abs, bits);
    }
    Vec3::from_array(out)
}

pub fn write_vec_f32(w: &mut BitWriter<'_>, v: Vec3) {
    for c in v.to_array() {
        w.write_f32(c);
    }
}

pub fn read_vec_f32(r: &mut BitReader<'_>) -> Vec3 {
    Vec3::new(r.read_f32(), r.read_f32(), r.read_f32())
}

// ---------------------------------------------------------------------------------------------
// Unit vectors: octahedral encoding
// ---------------------------------------------------------------------------------------------

#[inline]
fn sign_not_zero(v: f32) -> f32 {
    if v >= 0.0 { 1.0 } else { -1.0 }
}

/// Octahedral map of a unit vector onto `[-1, 1]²`.
pub fn oct_project(n: Vec3) -> Vec2 {
    let l1 = n.x.abs() + n.y.abs() + n.z.abs();
    if l1 <= f32::EPSILON {
        return Vec2::ZERO; // treated as +Z
    }
    let p = Vec2::new(n.x / l1, n.y / l1);
    if n.z < 0.0 {
        Vec2::new((1.0 - p.y.abs()) * sign_not_zero(p.x), (1.0 - p.x.abs()) * sign_not_zero(p.y))
    } else {
        p
    }
}

/// Inverse of [`oct_project`] (returns a normalized vector).
pub fn oct_unproject(e: Vec2) -> Vec3 {
    let mut n = Vec3::new(e.x, e.y, 1.0 - e.x.abs() - e.y.abs());
    let t = (-n.z).clamp(0.0, 1.0);
    n.x += if n.x >= 0.0 { -t } else { t };
    n.y += if n.y >= 0.0 { -t } else { t };
    let len = libm::sqrtf(n.length_squared());
    if len <= f32::EPSILON { Vec3::Z } else { n / len }
}

pub fn write_dir(w: &mut BitWriter<'_>, n: Vec3, bits_per_axis: u32) {
    let e = oct_project(n);
    w.write_bits(quantize_signed(e.x, 1.0, bits_per_axis), bits_per_axis);
    w.write_bits(quantize_signed(e.y, 1.0, bits_per_axis), bits_per_axis);
}

pub fn read_dir(r: &mut BitReader<'_>, bits_per_axis: u32) -> Vec3 {
    let x = dequantize_signed(r.read_bits(bits_per_axis), 1.0, bits_per_axis);
    let y = dequantize_signed(r.read_bits(bits_per_axis), 1.0, bits_per_axis);
    oct_unproject(Vec2::new(x, y))
}

/// Round-trips a direction through the wire encoding, so the client predicts with exactly the value
/// the server will decode.
pub fn quantize_dir(n: Vec3, bits_per_axis: u32) -> Vec3 {
    let e = oct_project(n);
    let x = dequantize_signed(quantize_signed(e.x, 1.0, bits_per_axis), 1.0, bits_per_axis);
    let y = dequantize_signed(quantize_signed(e.y, 1.0, bits_per_axis), 1.0, bits_per_axis);
    oct_unproject(Vec2::new(x, y))
}

// ---------------------------------------------------------------------------------------------
// Rotations: smallest-three
// ---------------------------------------------------------------------------------------------

const SQRT_HALF: f32 = core::f32::consts::FRAC_1_SQRT_2;

/// Writes a unit quaternion as the index of its largest component (2 bits) plus the other three in
/// `[-1/√2, 1/√2]` at `bits` each.
pub fn write_quat(w: &mut BitWriter<'_>, q: Quat, bits: u32) {
    let a = q.to_array();
    let mut largest = 0usize;
    for i in 1..4 {
        if a[i].abs() > a[largest].abs() {
            largest = i;
        }
    }
    let sign = if a[largest] < 0.0 { -1.0 } else { 1.0 };
    w.write_bits(largest as u32, 2);
    for (i, c) in a.iter().enumerate() {
        if i != largest {
            w.write_bits(quantize_signed(c * sign, SQRT_HALF, bits), bits);
        }
    }
}

pub fn read_quat(r: &mut BitReader<'_>, bits: u32) -> Quat {
    let largest = r.read_bits(2) as usize;
    let mut a = [0.0f32; 4];
    let mut sum = 0.0f32;
    for (i, c) in a.iter_mut().enumerate() {
        if i != largest {
            *c = dequantize_signed(r.read_bits(bits), SQRT_HALF, bits);
            sum += *c * *c;
        }
    }
    a[largest] = libm::sqrtf((1.0 - sum).max(0.0));
    let q = Quat::from_array(a);
    let len = libm::sqrtf(q.length_squared());
    if len <= f32::EPSILON { Quat::IDENTITY } else { q / len }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_round_trip_within_half_step() {
        for bits in [8u32, 10, 14, 16, 21] {
            let step = signed_step(100.0, bits);
            let mut v = -100.0f32;
            while v <= 100.0 {
                let back = dequantize_signed(quantize_signed(v, 100.0, bits), 100.0, bits);
                assert!((back - v).abs() <= step * 0.5 + 1e-4, "bits {bits} v {v} back {back}");
                v += 0.37;
            }
        }
    }

    #[test]
    fn oct_round_trip() {
        let dirs = [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::X,
            -Vec3::Y,
            -Vec3::Z,
            Vec3::new(0.3, -0.8, -0.52).normalize(),
        ];
        for d in dirs {
            let back = quantize_dir(d, 16);
            assert!(back.dot(d) > 0.99999, "{d} -> {back}");
        }
    }

    #[test]
    fn quat_round_trip() {
        let q = Quat::from_euler(glam::EulerRot::YXZ, 1.1, -0.4, 2.9);
        let mut buf = [0u8; 16];
        let mut w = BitWriter::new(&mut buf);
        write_quat(&mut w, q, 16);
        let mut r = BitReader::new(&buf);
        let back = read_quat(&mut r, 16);
        assert!(back.dot(q).abs() > 0.99999);
    }
}
