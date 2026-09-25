//! The debris field around the colony: rocks (and the ore in them), generated from a seed so that
//! the server and every client build the identical field. Storage is allocated once, here.
//!
//! The simulation does not collide with the field yet: today it is the client's backdrop. Sharing
//! the generator is what will let salvage and mining make the rocks solid without moving one.

use alloc::boxed::Box;

use glam::{Quat, Vec3};

use crate::math::{Rng, normalize_or, quat_axis_angle};
use crate::storage::boxed;
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};

/// Ore kinds: nickel-iron (common), titanium, volatiles (ices), and exotic metals, the feedstock of
/// zero-G alloys (rare).
pub const ORE_KINDS: u8 = 4;
/// Mesh variants the client draws rocks with.
pub const SHAPES: u8 = 12;

/// Centre of the field (the combat zone above the colony).
pub const FIELD_CENTER: Vec3 = Vec3::new(0.0, 900.0, 0.0);
/// Keep-out zones: the faction spawn bases (see `sim::spawn_point`), and room around the colony.
const SPAWN_BASES: [Vec3; 3] = [
    Vec3::new(-4_200.0, 700.0, -3_200.0),
    Vec3::new(4_200.0, 700.0, -3_200.0),
    Vec3::new(0.0, 2_200.0, 4_200.0),
];
const SPAWN_CLEARANCE: f32 = 500.0;
const COLONY_CLEARANCE: f32 = 150.0;

/// One rock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rock {
    pub pos: Vec3,
    /// Collider radius: the rock's mesh fits inside this sphere.
    pub radius: f32,
    /// Half-axes of the irregular ellipsoid the mesh is stretched to (the largest equals `radius`).
    pub axes: Vec3,
    pub rot: Quat,
    /// Mesh variant, `0..SHAPES`.
    pub shape: u8,
    /// Ore kind, `0..ORE_KINDS`.
    pub ore: u8,
}

impl Default for Rock {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, radius: 1.0, axes: Vec3::ONE, rot: Quat::IDENTITY, shape: 0, ore: 0 }
    }
}

/// The field: `len()` rocks.
pub struct Field {
    rocks: Box<[Rock]>,
    n: usize,
}

impl Field {
    /// The L1 debris field.
    pub const DEFAULT_SEED: u32 = 0xDEB12;
    pub const DEFAULT_ROCKS: u16 = 160;

    /// Generates up to `count` rocks from `seed`. Candidates inside a keep-out zone are rejected
    /// (at most `10 × count` are drawn), so the field may hold slightly fewer.
    pub fn generate(seed: u32, count: u16) -> Self {
        let mut rocks = boxed(count as usize, Rock::default());
        let mut rng = Rng::new(u64::from(seed));
        let mut n = 0;
        for _ in 0..count as usize * 10 {
            if n == count as usize {
                break;
            }
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed() * 0.5, rng.signed()), Vec3::X);
            let pos = FIELD_CENTER + dir * (1_200.0 + rng.next_f32() * 6_000.0);
            let s = 4.0 + rng.next_f32() * rng.next_f32() * 60.0;
            let axes = Vec3::new(s * (0.6 + rng.next_f32()), s * (0.5 + rng.next_f32() * 0.6), s);
            let radius = axes.max_element();
            let axis = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let rot = quat_axis_angle(axis, rng.next_f32() * core::f32::consts::TAU);
            let shape = (rng.next_u32() % u32::from(SHAPES)) as u8;
            let ore = match rng.next_f32() {
                x if x < 0.55 => 0,
                x if x < 0.8 => 1,
                x if x < 0.95 => 2,
                _ => 3,
            };
            if Self::keep_out(pos, radius) {
                continue;
            }
            rocks[n] = Rock { pos, radius, axes, rot, shape, ore };
            n += 1;
        }
        Self { rocks, n }
    }

    /// Whether a rock of `radius` at `pos` would sit inside a keep-out zone.
    fn keep_out(pos: Vec3, radius: f32) -> bool {
        let rel = pos - COLONY_CENTER;
        let radial = crate::math::sqrt(rel.y * rel.y + rel.z * rel.z);
        let colony = rel.x.abs() <= COLONY_HALF_LENGTH + COLONY_CLEARANCE + radius
            && radial <= COLONY_RADIUS + COLONY_CLEARANCE + radius;
        colony || SPAWN_BASES.iter().any(|b| b.distance(pos) < SPAWN_CLEARANCE + radius)
    }

    pub fn rocks(&self) -> &[Rock] {
        &self.rocks[..self.n]
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_clear_of_keep_out_zones() {
        let a = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        let b = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
        assert_eq!(a.rocks(), b.rocks());
        assert!(a.len() > 140, "only {} rocks", a.len());
        for r in a.rocks() {
            assert!(!Field::keep_out(r.pos, r.radius));
            assert_eq!(r.radius, r.axes.max_element());
            assert!(r.shape < SHAPES && r.ore < ORE_KINDS);
            assert!((r.rot.length() - 1.0).abs() < 1e-4);
        }
        let other = Field::generate(1, Field::DEFAULT_ROCKS);
        assert_ne!(a.rocks()[0], other.rocks()[0]);
    }

    #[test]
    fn empty_field() {
        assert!(Field::generate(7, 0).is_empty());
    }
}
