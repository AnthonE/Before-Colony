//! Fire control: intercept solutions against moving targets.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_sim::math::Rng;
use bc_sim::zero::fire_control::{intercept, miss_distance};
use glam::Vec3;

#[test]
fn solves_constant_velocity_targets() {
    let mut rng = Rng::new(99);
    let mut hits = 0;
    let n = 2_000;
    for _ in 0..n {
        let dir = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or(Vec3::Z);
        let target = dir * (200.0 + rng.next_f32() * 3_800.0);
        let tv = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 400.0;
        let sv = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 150.0;
        let Some(sol) = intercept(Vec3::ZERO, sv, 4_000.0, target, tv, Vec3::ZERO) else { continue };
        if miss_distance(Vec3::ZERO, sv, 4_000.0, sol.dir, target, tv, Vec3::ZERO) < 0.5 {
            hits += 1;
        }
    }
    assert!(hits as f32 / n as f32 >= 0.99, "{hits}/{n}");
}

#[test]
fn leads_accelerating_targets() {
    // A target burning 30 m/s² sideways at 1.5 km: the linear lead misses, the accelerated one hits.
    let target = Vec3::new(0.0, 0.0, 1_500.0);
    let tv = Vec3::new(80.0, 0.0, 0.0);
    let ta = Vec3::new(30.0, 0.0, 0.0);
    let linear = intercept(Vec3::ZERO, Vec3::ZERO, 4_000.0, target, tv, Vec3::ZERO).unwrap();
    let accel = intercept(Vec3::ZERO, Vec3::ZERO, 4_000.0, target, tv, ta).unwrap();
    assert!(miss_distance(Vec3::ZERO, Vec3::ZERO, 4_000.0, linear.dir, target, tv, ta) > 1.5);
    assert!(miss_distance(Vec3::ZERO, Vec3::ZERO, 4_000.0, accel.dir, target, tv, ta) < 0.5);
}
