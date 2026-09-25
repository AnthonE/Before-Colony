//! Narrow-phase geometry: segment–segment distance and swept tests against suit capsules.

use glam::{Quat, Vec3};

use crate::content::Capsule;

/// Closest points between segments `p1q1` and `p2q2`. Returns `(s, t, dist²)` with `s`, `t` the
/// parameters along each segment (Ericson, *Real-Time Collision Detection* §5.1.9).
pub fn segment_segment(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (f32, f32, f32) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    const EPS: f32 = 1e-9;
    let (s, t);
    if a <= EPS && e <= EPS {
        return (0.0, 0.0, r.dot(r));
    }
    if a <= EPS {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > EPS { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    let c1 = p1 + d1 * s;
    let c2 = p2 + d2 * t;
    let d = c1 - c2;
    (s, t, d.dot(d))
}

/// A capsule transformed to world space.
#[inline]
pub fn capsule_world(c: &Capsule, pos: Vec3, rot: Quat) -> (Vec3, Vec3, f32) {
    (pos + rot * c.a, pos + rot * c.b, c.r)
}

/// First contact of a swept sphere (segment `a→b`, radius `r`) with a set of capsules posed at
/// `pos`/`rot`. Returns `(param along the segment, capsule index)`. The closest-approach parameter
/// is used as the contact point, which is accurate for thin, fast projectiles.
pub fn sweep_capsules(
    a: Vec3,
    b: Vec3,
    r: f32,
    caps: &[Capsule],
    pos: Vec3,
    rot: Quat,
) -> Option<(f32, usize)> {
    let mut best: Option<(f32, usize)> = None;
    for (i, c) in caps.iter().enumerate() {
        let (ca, cb, cr) = capsule_world(c, pos, rot);
        let (s, _, d2) = segment_segment(a, b, ca, cb);
        let rr = r + cr;
        if d2 <= rr * rr && best.is_none_or(|(bs, _)| s < bs) {
            best = Some((s, i));
        }
    }
    best
}

/// Where a shot along `dir`, on the line through `through`, strikes a capsule (`a`–`b`, radius
/// `r`), and the surface normal there. A line that misses grazes the nearest point of the surface
/// instead (hits are decided by the server; this only places the sparks).
pub fn capsule_impact(through: Vec3, dir: Vec3, a: Vec3, b: Vec3, r: f32) -> (Vec3, Vec3) {
    const REACH: f32 = 1_000.0;
    let (p, q) = (through - dir * REACH, through + dir * REACH);
    let (s, t, d2) = segment_segment(p, q, a, b);
    let on_line = p + (q - p) * s;
    let on_axis = a + (b - a) * t;
    let toward = if d2 < r * r {
        // Back up the line from its closest approach to where it enters the armour.
        on_line - dir * crate::math::sqrt(r * r - d2) - on_axis
    } else {
        on_line - on_axis
    };
    let n = crate::math::normalize_or(toward, -dir);
    (on_axis + n * r, n)
}

/// Whether segment `a→b` passes within `radius` of point `p`.
pub fn segment_near_point(a: Vec3, b: Vec3, p: Vec3, radius: f32) -> bool {
    let (_, _, d2) = segment_segment(a, b, p, p);
    d2 <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_on_a_capsule() {
        let (a, b, r) = (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 1.0);
        // Straight in from +z through the middle: the front of the capsule.
        let (p, n) = capsule_impact(Vec3::ZERO, Vec3::NEG_Z, a, b, r);
        assert!((p - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-4 && (n - Vec3::Z).length() < 1e-4);
        // A line passing wide grazes the nearest side.
        let (p, n) = capsule_impact(Vec3::new(3.0, 0.5, 0.0), Vec3::NEG_Z, a, b, r);
        assert!((p - Vec3::new(1.0, 0.5, 0.0)).length() < 1e-4 && (n - Vec3::X).length() < 1e-4);
        // Off-centre but through: enters on the near face, the normal leaning toward the line.
        let (p, n) = capsule_impact(Vec3::new(0.5, 0.0, 0.0), Vec3::NEG_Z, a, b, r);
        assert!((p.length() - 1.0).abs() < 1e-4 && p.z > 0.8 && n.x > 0.4);
    }

    #[test]
    fn crossing_segments() {
        let (s, t, d2) = segment_segment(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        );
        assert!((s - 0.5).abs() < 1e-5 && (t - 0.5).abs() < 1e-5);
        assert!((d2 - 1.0).abs() < 1e-5);
    }
}
