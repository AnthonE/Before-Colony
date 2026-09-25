//! What has happened to the debris field's rocks: the field itself (`crate::field`) never
//! changes, so this is their mutable side. Nothing mines them yet; clients are sent whatever has
//! changed.

use alloc::boxed::Box;

use crate::field::{Field, Rock};
use crate::storage::{BitSet, boxed};

/// A rock's structure, by its size.
pub fn max_hp(r: &Rock) -> f32 {
    60.0 + 25.0 * r.radius
}

/// The ore a rock holds, by its size (kg, a multiple of 10).
pub fn max_ore_kg(r: &Rock) -> u32 {
    (r.radius * 20.0) as u32 * 10
}

pub struct RockStates {
    pub hp: Box<[f32]>,
    pub ore_kg: Box<[u32]>,
    /// Shattered, until it regrows.
    pub destroyed: BitSet,
    /// Bumped on every change clients must hear of.
    pub version: Box<[u8]>,
    /// Tick a shattered rock grows back.
    pub regrow_at: Box<[u32]>,
}

impl RockStates {
    pub fn new(field: &Field) -> Self {
        let n = field.len();
        let mut s = Self {
            hp: boxed(n, 0.0f32),
            ore_kg: boxed(n, 0u32),
            destroyed: BitSet::new(n.max(1)),
            version: boxed(n, 0u8),
            regrow_at: boxed(n, 0u32),
        };
        for (i, r) in field.rocks().iter().enumerate() {
            s.hp[i] = max_hp(r);
            s.ore_kg[i] = max_ore_kg(r);
        }
        s
    }

    /// Marks rock `i` changed.
    pub fn touch(&mut self, i: usize) {
        self.version[i] = self.version[i].wrapping_add(1);
    }
}
