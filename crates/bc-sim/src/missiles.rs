//! Missile storage: homing missiles in flight. Unlike beams and rounds they steer, so each one
//! carries its target and what's left of its motor's Δv.

use alloc::boxed::Box;
use bc_proto::{Faction, NO_SLOT, WeaponKind};
use glam::Vec3;

use crate::storage::{BitSet, FreeList, boxed};

/// Missiles in flight at once, sector-wide (their wire ids are 10 bits).
pub const MAX_MISSILES: usize = 1 << bc_proto::MISSILE_BITS;

pub struct Missiles {
    pub alive: BitSet,
    /// Bumped each time a pool slot is reused, so clients tell the missiles on one id apart.
    pub generation: Box<[u8]>,
    pub kind: Box<[WeaponKind]>,
    pub owner: Box<[u16]>,
    pub owner_faction: Box<[Faction]>,
    /// The suit it's guided onto and that suit's generation, or `NO_SLOT` (flying blind).
    pub target: Box<[u16]>,
    pub target_gen: Box<[u16]>,
    pub pos: Box<[Vec3]>,
    pub vel: Box<[Vec3]>,
    /// The motor's remaining Δv, m/s.
    pub dv_left: Box<[f32]>,
    pub expire: Box<[u32]>,
    free: FreeList,
    count: usize,
}

impl Default for Missiles {
    fn default() -> Self {
        Self::new()
    }
}

impl Missiles {
    pub fn new() -> Self {
        let cap = MAX_MISSILES;
        Self {
            alive: BitSet::new(cap),
            generation: boxed(cap, 0u8),
            kind: boxed(cap, WeaponKind::HomingMissile),
            owner: boxed(cap, 0u16),
            owner_faction: boxed(cap, Faction::Oz),
            target: boxed(cap, NO_SLOT),
            target_gen: boxed(cap, 0u16),
            pos: boxed(cap, Vec3::ZERO),
            vel: boxed(cap, Vec3::ZERO),
            dv_left: boxed(cap, 0.0f32),
            expire: boxed(cap, 0u32),
            free: FreeList::full(cap),
            count: 0,
        }
    }

    /// Launches a missile; `None` if the pool is exhausted.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        kind: WeaponKind,
        owner: u16,
        owner_faction: Faction,
        target: (u16, u16),
        pos: Vec3,
        vel: Vec3,
        dv: f32,
        expire: u32,
    ) -> Option<usize> {
        let k = self.free.pop()? as usize;
        self.alive.set(k, true);
        self.generation[k] = self.generation[k].wrapping_add(1);
        self.kind[k] = kind;
        self.owner[k] = owner;
        self.owner_faction[k] = owner_faction;
        (self.target[k], self.target_gen[k]) = target;
        self.pos[k] = pos;
        self.vel[k] = vel;
        self.dv_left[k] = dv;
        self.expire[k] = expire;
        self.count += 1;
        Some(k)
    }

    pub fn kill(&mut self, k: usize) {
        if self.alive.get(k) {
            self.alive.set(k, false);
            self.free.push(k as u16);
            self.count -= 1;
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn guided(&self, k: usize) -> bool {
        self.target[k] != NO_SLOT
    }
}
