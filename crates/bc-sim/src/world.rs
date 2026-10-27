//! Static sector geometry: the sector box and the First Colony, a ring (`docs/RING.md`). Shared by
//! the server and the client's prediction, so suits bounce off the colony identically on both.
//!
//! The ring spins for gravity inside, but not in the simulation: its rim moves at 99 m/s, far too
//! fast to walk on or to predict riding, so to suits and shots it is a still, solid ring: a hollow
//! cylinder about the axis, between its ceiling ([`COLONY_INNER_RADIUS`]) and its floor
//! ([`COLONY_RADIUS`]), [`COLONY_HALF_WIDTH`] either side of its middle. Its hub and spokes, in the
//! hole, are a landmark (`content::landmarks`, turning with it). Its clock is still the tick's
//! ([`colony_spin_angle`]), so every client draws it turned the same way at the same tick.

use core::f32::consts::TAU;

use glam::Vec3;

use crate::config::SECTOR_LIMIT;
use crate::flight::FlightState;
use crate::math::{normalize_or, sqrt};

/// The First Colony's middle: on its axis (along X), below the combat zone, where the docking hub
/// has always been.
pub const COLONY_CENTER: Vec3 = Vec3::new(-16_250.0, -4_200.0, 0.0);
/// The ring's floor (and its hull, outside), m from the axis.
pub const COLONY_RADIUS: f32 = 1_000.0;
/// The ring's ceiling, the window lattice the light falls through, m from the axis.
pub const COLONY_INNER_RADIUS: f32 = 700.0;
/// Half the ring's width along the axis, m.
pub const COLONY_HALF_WIDTH: f32 = 300.0;
/// The farthest any of the ring is from its middle, m (its rim's corners).
pub const COLONY_REACH: f32 = 1_044.1;
/// Hull clearance for suits, m.
const HULL_MARGIN: f32 = 12.0;
/// The colony turns once every this many ticks (63.4 s): 1 g at its floor (1.0005 g), which moves
/// at 99 m/s.
pub const COLONY_SPIN_PERIOD_TICKS: u32 = 1_903;
/// How fast the colony turns about its axis, rad/s.
pub const SPIN_RATE: f32 = TAU / (COLONY_SPIN_PERIOD_TICKS as f32 * crate::config::DT);

/// How far the colony has turned about its axis at tick `t` plus `frac` of the next, rad. A whole
/// number of ticks a turn, taken modulo the tick, so it never drifts.
pub fn colony_spin_angle(t: u32, frac: f32) -> f32 {
    TAU * (((t % COLONY_SPIN_PERIOD_TICKS) as f32 + frac) / COLONY_SPIN_PERIOD_TICKS as f32)
}

/// Keeps a suit inside the sector and outside the colony hull (inelastic contact). Whether it had
/// to move it.
pub fn constrain(s: &mut FlightState) -> bool {
    keep_in_sector(s) | keep_off_hull(s)
}

/// Keeps a suit inside the sector: its bounds stop it where it goes out. Whether they did.
pub fn keep_in_sector(s: &mut FlightState) -> bool {
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
    moved
}

/// Keeps a suit outside the colony hull, losing the speed it had into it. Whether it touched.
pub fn keep_off_hull(s: &mut FlightState) -> bool {
    let Some((at, n)) = hull_contact(s.pos, HULL_MARGIN) else { return false };
    s.pos = at;
    let vn = s.vel.dot(n);
    if vn < 0.0 {
        s.vel -= n * vn;
    }
    true
}

/// `p` from the colony's middle: along the axis, the radial offset from it, and how far off it.
fn around(p: Vec3) -> (f32, Vec3, f32) {
    let rel = p - COLONY_CENTER;
    let radial = Vec3::new(0.0, rel.y, rel.z);
    (rel.x, radial, sqrt(radial.length_squared()))
}

/// The unit radial direction of `radial` (`d` its length), or +Y on the axis.
fn outward(radial: Vec3, d: f32) -> Vec3 {
    if d > 1e-3 { radial / d } else { Vec3::Y }
}

/// Whether a sphere of radius `r` at `p` touches the colony: if so, the nearest point out of it (on
/// the hull, the ceiling or a face, the ring grown by `r`) and the outward normal there.
pub fn hull_contact(p: Vec3, r: f32) -> Option<(Vec3, Vec3)> {
    let (x, radial, d) = around(p);
    let (half, out, inn) = (COLONY_HALF_WIDTH + r, COLONY_RADIUS + r, COLONY_INNER_RADIUS - r);
    if x.abs() >= half || d >= out || d <= inn {
        return None;
    }
    let n = outward(radial, d);
    let axis = COLONY_CENTER + Vec3::X * x;
    // Out through the nearest face: the hull, the ceiling (into the hole), or a side.
    let (to_face, to_hull, to_hole) = (half - x.abs(), out - d, d - inn);
    if to_face <= to_hull && to_face <= to_hole {
        let side = if x < 0.0 { -Vec3::X } else { Vec3::X };
        Some((Vec3::new(COLONY_CENTER.x + side.x * half, p.y, p.z), side))
    } else if to_hull <= to_hole {
        Some((axis + n * out, n))
    } else {
        Some((axis + n * inn, -n))
    }
}

/// Up, anywhere in space: +Y, the sector's own. A fight locked on (`bc_proto::LockOn`) takes it for
/// its ground, so the two pilots in it share one, and so does the upright view of the default
/// controls. (The cylinder that went before had sides to stand over; a 1 km ring 16 km from the
/// field has none worth it: `docs/RING.md`.)
pub fn up_at(_p: Vec3) -> Vec3 {
    Vec3::Y
}

/// How high `p` is, along [`up_at`], m: two suits at the same altitude fight on the same floor.
pub fn altitude(p: Vec3) -> f32 {
    p.y
}

/// How far `p` is outside the colony's solid ring (its hull, its ceiling or a side), m; negative
/// inside it.
pub fn colony_clearance(p: Vec3) -> f32 {
    let (x, _, d) = around(p);
    let along = x.abs() - COLONY_HALF_WIDTH;
    let across = (COLONY_INNER_RADIUS - d).max(d - COLONY_RADIUS);
    if along > 0.0 && across > 0.0 { sqrt(along * along + across * across) } else { along.max(across) }
}

/// The nearest point of the colony's solid ring to `p`, on its surface, and the outward normal there.
pub fn colony_surface(p: Vec3) -> (Vec3, Vec3) {
    let (x, radial, d) = around(p);
    let n = outward(radial, d);
    let axis = |x: f32| COLONY_CENTER + Vec3::X * x;
    let (half, out, inn) = (COLONY_HALF_WIDTH, COLONY_RADIUS, COLONY_INNER_RADIUS);
    if colony_clearance(p) > 0.0 {
        // Outside: the nearest point of the ring's cross-section, round the axis where `p` is.
        let at = axis(x.clamp(-half, half)) + n * d.clamp(inn, out);
        return (at, normalize_or(p - at, n));
    }
    // Inside: out through the nearest face.
    let (to_face, to_hull, to_hole) = (half - x.abs(), out - d, d - inn);
    if to_face <= to_hull && to_face <= to_hole {
        let side = if x < 0.0 { -Vec3::X } else { Vec3::X };
        (Vec3::new(COLONY_CENTER.x + side.x * half, p.y, p.z), side)
    } else if to_hull <= to_hole {
        (axis(x) + n * out, n)
    } else {
        (axis(x) + n * inn, -n)
    }
}

/// Whether a point is inside the colony's solid ring.
pub fn inside_colony(p: Vec3) -> bool {
    let (x, _, d) = around(p);
    x.abs() <= COLONY_HALF_WIDTH && (COLONY_INNER_RADIUS..=COLONY_RADIUS).contains(&d)
}

/// How far along `a→b` (0..1) a sphere of radius `r` first meets the colony (its hull, its ceiling or
/// a side): 0 if it starts inside it. The ring is solid through, so this is the ring grown by `r`,
/// square edges and all. It can be met four ways: through the hull from outside, through the
/// ceiling from the hole, or through either side.
pub fn colony_sweep(a: Vec3, b: Vec3, r: f32) -> Option<f32> {
    let (half, out, inn) = (COLONY_HALF_WIDTH + r, COLONY_RADIUS + r, (COLONY_INNER_RADIUS - r).max(0.0));
    let radial = |v: Vec3| v.y * v.y + v.z * v.z;
    let (p, d) = (a - COLONY_CENTER, b - a);
    let solid = |q: Vec3| q.x.abs() <= half && (inn * inn..=out * out).contains(&radial(q));
    if solid(p) {
        return Some(0.0);
    }
    // Starting outside, the first point of the ring's surface along the way is where it's met, so
    // every crossing of a face (within the face) is a candidate, and the earliest wins.
    let mut first: Option<f32> = None;
    let mut meet = |t: f32| {
        if (0.0..=1.0).contains(&t) && first.is_none_or(|f| t < f) {
            first = Some(t);
        }
    };
    // Through the hull or the ceiling: both of a cylinder's crossings, between the sides.
    let dd = d.y * d.y + d.z * d.z;
    if dd > 1e-12 {
        let half_b = p.y * d.y + p.z * d.z;
        for rad in [out, inn] {
            let disc = half_b * half_b - dd * (radial(p) - rad * rad);
            if disc >= 0.0 {
                let root = sqrt(disc);
                for t in [(-half_b - root) / dd, (-half_b + root) / dd] {
                    if (p.x + d.x * t).abs() <= half {
                        meet(t);
                    }
                }
            }
        }
    }
    // Through a side, between the ceiling and the hull.
    if d.x != 0.0 {
        for x in [-half, half] {
            let t = (x - p.x) / d.x;
            if (inn * inn..=out * out).contains(&radial(p + d * t)) {
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
        assert!((g - 1.0).abs() < 1e-3, "{g} g at the floor");
        assert!((w * COLONY_RADIUS - 99.0).abs() < 0.5, "the rim moves at {} m/s", w * COLONY_RADIUS);
        assert!((SPIN_RATE - w).abs() < 1e-7);
        // Whole ticks modulo the period: no drift, and the fraction runs on into the next tick.
        assert_eq!(colony_spin_angle(0, 0.0), 0.0);
        assert_eq!(colony_spin_angle(COLONY_SPIN_PERIOD_TICKS * 1_000 + 5, 0.25), colony_spin_angle(5, 0.25));
        assert!((colony_spin_angle(41, 1.0) - colony_spin_angle(42, 0.0)).abs() < 1e-6);
        assert!((colony_spin_angle(COLONY_SPIN_PERIOD_TICKS - 1, 1.0) - TAU).abs() < 1e-5);
        assert!((colony_spin_angle(1, 0.0) - w * DT).abs() < 1e-7);
        // Its rim's corners are its reach.
        let corner = sqrt(COLONY_RADIUS * COLONY_RADIUS + COLONY_HALF_WIDTH * COLONY_HALF_WIDTH);
        assert!(corner <= COLONY_REACH && COLONY_REACH - corner < 0.5);
    }

    #[test]
    fn up_is_the_sectors_and_altitude_is_height() {
        let mut rng = Rng::new(3);
        for _ in 0..100 {
            let p = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 20_000.0;
            assert_eq!(up_at(p), Vec3::Y);
            assert_eq!(altitude(p + Vec3::Y * 10.0) - altitude(p), 10.0);
        }
    }

    #[test]
    fn the_nearest_point_of_the_ring_is_on_it_and_faces_out() {
        let mut rng = Rng::new(5);
        let mut inside = 0;
        for _ in 0..20_000 {
            let p = COLONY_CENTER
                + Vec3::new(rng.signed() * 1_500.0, rng.signed() * 1_600.0, rng.signed() * 1_600.0);
            let (at, n) = colony_surface(p);
            let c = colony_clearance(p);
            assert!(colony_clearance(at).abs() < 0.05, "{p}: {at} is {} off", colony_clearance(at));
            assert!((length(n) - 1.0).abs() < 1e-4);
            // A step out along the normal clears it; the clearance is the distance to the point.
            assert!(colony_clearance(at + n * 2.0) > 1.9, "{p}: {at} out {n}");
            if c > 0.0 {
                assert!((length(p - at) - c).abs() < 0.05, "{p}: {} vs {c}", length(p - at));
            } else {
                inside += 1;
                assert!(inside_colony(p));
                assert!(hull_contact(p, 0.0).is_some());
            }
            assert_eq!(c < 0.0, inside_colony(p) && c != 0.0);
        }
        assert!(inside > 300, "{inside} inside");
    }

    #[test]
    fn a_suit_is_pushed_out_through_the_nearest_face() {
        let c = COLONY_CENTER;
        // Just under the hull, the ceiling, a side: out each way.
        for (p, want) in [
            (c + Vec3::new(10.0, COLONY_RADIUS - 2.0, 0.0), Vec3::Y),
            (c + Vec3::new(10.0, 0.0, -(COLONY_INNER_RADIUS + 2.0)), Vec3::Z),
            (c + Vec3::new(COLONY_HALF_WIDTH - 1.0, 850.0, 0.0), Vec3::X),
            (c + Vec3::new(-COLONY_HALF_WIDTH + 1.0, -850.0, 0.0), -Vec3::X),
        ] {
            let (at, n) = hull_contact(p, HULL_MARGIN).expect("in it");
            assert!(n.angle_between(want) < 1e-4, "{p}: {n}");
            assert!(hull_contact(at + n * 0.01, HULL_MARGIN).is_none(), "{p}: still in at {at}");
        }
        // In the hole, and well clear of it, nothing.
        assert!(hull_contact(c, HULL_MARGIN).is_none());
        assert!(hull_contact(c + Vec3::Y * 1_200.0, HULL_MARGIN).is_none());
        assert!(hull_contact(c + Vec3::new(400.0, 850.0, 0.0), HULL_MARGIN).is_none());
    }

    /// Whether `q` is in the ring grown by `r` (in f64).
    fn in_colony(q: [f64; 3], r: f64) -> bool {
        let (x, y, z) = (q[0] - f64::from(COLONY_CENTER.x), q[1] - f64::from(COLONY_CENTER.y), q[2]);
        let (out, inn) = (f64::from(COLONY_RADIUS) + r, (f64::from(COLONY_INNER_RADIUS) - r).max(0.0));
        let d2 = y * y + z * z;
        x.abs() <= f64::from(COLONY_HALF_WIDTH) + r && d2 <= out * out && d2 >= inn * inn
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
        let (h, rad, inn) = (COLONY_HALF_WIDTH, COLONY_RADIUS, COLONY_INNER_RADIUS);
        let mut hits = [0; 5];
        for case in 0..5_000 {
            let r = [0.0, 0.5, 12.0][(rng.next_u32() % 3) as usize];
            // A point `x` along the axis, `radius` off it, a turn `k` (0..1) round it.
            let around = |x: f32, radius: f32, k: f32| {
                COLONY_CENTER + Vec3::new(x, cos(k * TAU) * radius, sin(k * TAU) * radius)
            };
            let [u, v, w, z] = [rng.signed(), rng.next_f32(), rng.signed(), rng.next_f32()];
            let k0 = rng.next_f32();
            let [k1, far] = [k0 + rng.signed() * 0.02, 50.0 + rng.next_f32() * 600.0];
            let side = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
            let (a, b) = match case % 5 {
                // At the hull, from outside it.
                0 => {
                    let a = around(u * h * 0.95, rad + r + 10.0 + v * 400.0, k0);
                    let to = around(u * h * 0.95 + w * 100.0, rad * (0.9 + z * 0.15), k1);
                    (a, a + normalize_or(to - a, Vec3::Y) * far)
                }
                // At a side, from beyond it.
                1 => {
                    let off = inn * 0.9 + (u + 1.0) * 0.5 * (rad * 1.05 - inn * 0.9);
                    let a = around(side * (h + r + 10.0 + v * 400.0), off, k0);
                    let to = around(side * (h - 60.0 + z * 80.0), off + w * 120.0, k1);
                    (a, a + normalize_or(to - a, Vec3::X) * far)
                }
                // From inside the ring.
                2 => {
                    let a = around(u * h, inn + v * (rad - inn), k0);
                    (
                        a,
                        a + normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y)
                            * 300.0,
                    )
                }
                // From the hole, out at the ceiling.
                3 => {
                    let a = around(u * h * 0.9, v * inn * 0.9, k0);
                    let to = around(u * h * 0.9 + w * 150.0, rad, k1);
                    (a, a + normalize_or(to - a, Vec3::Y) * (far + 300.0))
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
            // How far a hit is from the grown ring's surface, m.
            let edge = |f: f32| {
                let q = (a + (b - a) * f - COLONY_CENTER).as_dvec3();
                let d = (q.y * q.y + q.z * q.z).sqrt();
                (d - f64::from(rad + r)).max(f64::from(inn - r) - d).max(q.x.abs() - f64::from(h + r))
            };
            match (colony_sweep(a, b, r), sampled(a, b, r)) {
                (Some(0.0), Some((0.0, _))) => hits[case % 5] += 1,
                (Some(f), Some((s, run))) => {
                    hits[case % 5] += 1;
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
