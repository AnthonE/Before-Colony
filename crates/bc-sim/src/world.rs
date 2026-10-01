//! Static sector geometry: the sector box and the L1 colony cylinder. Shared by the server and the
//! client's prediction, so suits bounce off the colony hull identically on both.
//!
//! The colony spins for gravity inside, but not in the simulation: its hull's surface moves at
//! 177 m/s, far too fast to walk on or to predict riding, so to suits and shots it is a still,
//! solid cylinder. Its clock is still the tick's ([`colony_spin_angle`]), so every client draws it
//! turned the same way at the same tick, and a later hull walker has it to hand.

use core::f32::consts::TAU;

use glam::Vec3;

use crate::config::SECTOR_LIMIT;
use crate::flight::FlightState;
use crate::math::sqrt;

/// O'Neill cylinder "L1 Colony Cluster, Colony 03": axis along X, below the combat zone.
pub const COLONY_CENTER: Vec3 = Vec3::new(0.0, -4_200.0, 0.0);
pub const COLONY_RADIUS: f32 = 3_200.0;
pub const COLONY_HALF_LENGTH: f32 = 16_000.0;
/// Hull clearance for suits, m.
const HULL_MARGIN: f32 = 12.0;
/// The colony turns once every this many ticks (113.5 s): 1 g at the hull (0.999994 g), which
/// moves at 177 m/s.
pub const COLONY_SPIN_PERIOD_TICKS: u32 = 3_405;

/// How far the colony has turned about its axis at tick `t` plus `frac` of the next, rad. A whole
/// number of ticks a turn, taken modulo the tick, so it never drifts.
pub fn colony_spin_angle(t: u32, frac: f32) -> f32 {
    TAU * (((t % COLONY_SPIN_PERIOD_TICKS) as f32 + frac) / COLONY_SPIN_PERIOD_TICKS as f32)
}

/// Keeps a suit inside the sector and outside the colony hull (inelastic contact). Whether it had
/// to move it.
pub fn constrain(s: &mut FlightState) -> bool {
    let mut moved = false;
    for i in 0..3 {
        if s.pos[i] > SECTOR_LIMIT {
            s.pos[i] = SECTOR_LIMIT;
            s.vel[i] = s.vel[i].min(0.0);
            moved = true;
        } else if s.pos[i] < -SECTOR_LIMIT {
            s.pos[i] = -SECTOR_LIMIT;
            s.vel[i] = s.vel[i].max(0.0);
            moved = true;
        }
    }
    if let Some((at, n)) = hull_contact(s.pos, HULL_MARGIN) {
        s.pos = at;
        let vn = s.vel.dot(n);
        if vn < 0.0 {
            s.vel -= n * vn;
        }
        moved = true;
    }
    moved
}

/// Whether a sphere of radius `r` at `p` touches the colony: if so, the nearest point out of it (on
/// the hull or an end cap, grown by `r`) and the outward normal there.
pub fn hull_contact(p: Vec3, r: f32) -> Option<(Vec3, Vec3)> {
    let rel = p - COLONY_CENTER;
    let cap = COLONY_HALF_LENGTH + r;
    if rel.x.abs() >= cap {
        return None;
    }
    let radial = Vec3::new(0.0, rel.y, rel.z);
    let r2 = radial.length_squared();
    let limit = COLONY_RADIUS + r;
    if r2 >= limit * limit {
        return None;
    }
    let d = crate::math::sqrt(r2);
    // Out through the nearer face: the curved hull, or an end cap.
    if cap - rel.x.abs() < limit - d {
        let n = if rel.x < 0.0 { -Vec3::X } else { Vec3::X };
        return Some((Vec3::new(COLONY_CENTER.x + n.x * cap, p.y, p.z), n));
    }
    let n = if d > 1e-3 { radial / d } else { Vec3::Y };
    Some((COLONY_CENTER + Vec3::new(rel.x, 0.0, 0.0) + n * limit, n))
}

/// Whether a point is inside the colony's solid hull.
pub fn inside_colony(p: Vec3) -> bool {
    let rel = p - COLONY_CENTER;
    rel.x.abs() <= COLONY_HALF_LENGTH && rel.y * rel.y + rel.z * rel.z <= COLONY_RADIUS * COLONY_RADIUS
}

/// How far along `a→b` (0..1) a sphere of radius `r` first meets the colony (its hull, or an end
/// cap): 0 if it starts inside it. The colony is solid through, so this is the cylinder grown by
/// `r`, flat ends and all.
pub fn colony_sweep(a: Vec3, b: Vec3, r: f32) -> Option<f32> {
    let (half, rad) = (COLONY_HALF_LENGTH + r, COLONY_RADIUS + r);
    let r2 = rad * rad;
    let radial = |v: Vec3| v.y * v.y + v.z * v.z;
    let (p, d) = (a - COLONY_CENTER, b - a);
    if p.x.abs() <= half && radial(p) <= r2 {
        return Some(0.0);
    }
    let mut first: Option<f32> = None;
    let mut meet = |t: f32| {
        if (0.0..=1.0).contains(&t) && first.is_none_or(|f| t < f) {
            first = Some(t);
        }
    };
    // Through the curved hull, between the caps.
    let dd = d.y * d.y + d.z * d.z;
    if dd > 1e-12 {
        let half_b = p.y * d.y + p.z * d.z;
        let disc = half_b * half_b - dd * (radial(p) - r2);
        if disc >= 0.0 {
            let t = (-half_b - sqrt(disc)) / dd;
            if (p.x + d.x * t).abs() <= half {
                meet(t);
            }
        }
    }
    // Through an end cap.
    if d.x != 0.0 {
        for x in [-half, half] {
            let t = (x - p.x) / d.x;
            if radial(p + d * t) <= r2 {
                meet(t);
            }
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DT, G0};
    use crate::math::{Rng, cos, length, normalize_or, sin};

    #[test]
    fn colony_spin_period_is_one_g() {
        let w = TAU / (COLONY_SPIN_PERIOD_TICKS as f32 * DT);
        let g = w * w * COLONY_RADIUS / G0;
        assert!((g - 1.0).abs() < 1e-4, "{g} g at the hull");
        assert!((w * COLONY_RADIUS - 177.0).abs() < 0.5, "the hull moves at {} m/s", w * COLONY_RADIUS);
        // Whole ticks modulo the period: no drift, and the fraction runs on into the next tick.
        assert_eq!(colony_spin_angle(0, 0.0), 0.0);
        assert_eq!(colony_spin_angle(COLONY_SPIN_PERIOD_TICKS * 1_000 + 5, 0.25), colony_spin_angle(5, 0.25));
        assert!((colony_spin_angle(41, 1.0) - colony_spin_angle(42, 0.0)).abs() < 1e-6);
        assert!((colony_spin_angle(COLONY_SPIN_PERIOD_TICKS - 1, 1.0) - TAU).abs() < 1e-5);
        assert!((colony_spin_angle(1, 0.0) - w * DT).abs() < 1e-7);
    }

    /// Whether `q` is in the colony grown by `r` (in f64).
    fn in_colony(q: [f64; 3], r: f64) -> bool {
        let (x, y, z) = (q[0] - f64::from(COLONY_CENTER.x), q[1] - f64::from(COLONY_CENTER.y), q[2]);
        let rad = f64::from(COLONY_RADIUS) + r;
        x.abs() <= f64::from(COLONY_HALF_LENGTH) + r && y * y + z * z <= rad * rad
    }

    /// Where a sphere of radius `r` moving `a→b` first enters the colony, sampled every 25 cm and
    /// bisected: how far along (0..1), and for how long it's inside, m.
    fn sampled(a: Vec3, b: Vec3, r: f32) -> Option<(f64, f64)> {
        let (a, b, r) = (a.as_dvec3(), b.as_dvec3(), f64::from(r));
        let at = |f: f64| {
            let q = a + (b - a) * f;
            [q.x, q.y, q.z]
        };
        let len = (b - a).length();
        let n = (len / 0.25) as u32 + 1;
        let k = (0..=n).find(|&k| in_colony(at(f64::from(k) / f64::from(n)), r))?;
        if k == 0 {
            return Some((0.0, len));
        }
        let (mut lo, mut hi) = (f64::from(k - 1) / f64::from(n), f64::from(k) / f64::from(n));
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if in_colony(at(mid), r) { hi = mid } else { lo = mid }
        }
        let run = (k..=n).take_while(|&j| in_colony(at(f64::from(j) / f64::from(n)), r)).count();
        Some((hi, run as f64 * len / f64::from(n)))
    }

    #[test]
    fn colony_sweep_matches_dense_sampling() {
        let mut rng = Rng::new(7);
        let (h, rad) = (COLONY_HALF_LENGTH, COLONY_RADIUS);
        let mut hits = [0; 4];
        for case in 0..4_000 {
            let r = [0.0, 0.5, 12.0][(rng.next_u32() % 3) as usize];
            // A point `x` along the axis, `radius` off it, a turn `k` (0..1) round it.
            let around = |x: f32, radius: f32, k: f32| {
                COLONY_CENTER + Vec3::new(x, cos(k * TAU) * radius, sin(k * TAU) * radius)
            };
            let [u, v, w, z] = [rng.signed(), rng.next_f32(), rng.signed(), rng.next_f32()];
            let k0 = rng.next_f32();
            let [k1, far] = [k0 + rng.signed() * 0.02, 50.0 + rng.next_f32() * 600.0];
            let side = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
            let (a, b) = match case % 4 {
                // At the hull, from outside it.
                0 => {
                    let a = around(u * h * 0.95, rad + r + 10.0 + v * 400.0, k0);
                    let to = around(u * h * 0.95 + w * 300.0, rad * (0.9 + z * 0.15), k1);
                    (a, a + normalize_or(to - a, Vec3::Y) * far)
                }
                // At an end cap, from beyond it.
                1 => {
                    let off = (u + 1.0) * 0.5 * rad * 1.05;
                    let a = around(side * (h + r + 10.0 + v * 400.0), off, k0);
                    let to = around(side * (h - 60.0 + z * 80.0), off + w * 200.0, k1);
                    (a, a + normalize_or(to - a, Vec3::X) * far)
                }
                // From inside.
                2 => {
                    let a = around(u * h, v * rad, k0);
                    (
                        a,
                        a + normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y)
                            * 300.0,
                    )
                }
                // Grazing the hull: a chord passing within half a metre of it, either side.
                _ => {
                    let x = rng.signed() * h * 0.9;
                    let pass = rad + r + rng.signed() * 0.5;
                    let a = COLONY_CENTER + Vec3::new(x, pass, -400.0);
                    (a, a + Vec3::new(rng.signed() * 5.0, 0.0, 800.0))
                }
            };
            let len = f64::from(length(b - a));
            // How far a hit is from the grown colony's surface, m.
            let edge = |f: f32| {
                let q = (a + (b - a) * f - COLONY_CENTER).as_dvec3();
                ((q.y * q.y + q.z * q.z).sqrt() - f64::from(rad + r)).max(q.x.abs() - f64::from(h + r))
            };
            match (colony_sweep(a, b, r), sampled(a, b, r)) {
                (Some(0.0), Some((0.0, _))) => hits[case % 4] += 1,
                (Some(f), Some((s, run))) => {
                    hits[case % 4] += 1;
                    // On the surface, and where the sampling says along the line. How far along is
                    // ill-conditioned for a graze (the roots meet): the hull's radius over half the
                    // chord times a rounding of ~1e-4 m, so the tolerance grows as the chord shrinks.
                    assert!(
                        edge(f).abs() < 0.02,
                        "case {case}: {a} → {b} r {r}: hit at {f}, {} m off",
                        edge(f)
                    );
                    let tol = 0.02 + 2.0 / run;
                    assert!(
                        (f64::from(f) - s).abs() * len < tol,
                        "case {case}: {a} → {b} r {r}: swept {f}, sampled {s}"
                    );
                }
                (None, Some((s, run))) => {
                    assert!(run < 0.5, "case {case}: {a} → {b} r {r}: missed {run} m from {s}")
                }
                (Some(f), None) => assert!(
                    edge(f).abs() < 0.02,
                    "case {case}: {a} → {b} r {r}: hit at {f}, {} m off",
                    edge(f)
                ),
                (None, None) => {}
            }
        }
        assert!(hits.iter().all(|&n| n > 300), "hits by case: {hits:?}");
    }
}
