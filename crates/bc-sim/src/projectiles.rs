//! Projectile storage (beams and cannon rounds). Space has no drag or gravity here, so projectiles
//! fly straight at constant velocity until they hit something or time out.

use alloc::boxed::Box;
use bc_proto::{Faction, WeaponKind};
use glam::Vec3;

use crate::storage::{BitSet, FreeList, boxed};

pub struct Projectiles {
    pub cap: usize,
    pub alive: BitSet,
    pub kind: Box<[WeaponKind]>,
    pub owner: Box<[u16]>,
    pub owner_faction: Box<[Faction]>,
    pub pos: Box<[Vec3]>,
    pub vel: Box<[Vec3]>,
    pub expire: Box<[u32]>,
    pub damage: Box<[f32]>,
    pub radius: Box<[f32]>,
    free: FreeList,
    count: usize,
}

impl Projectiles {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            alive: BitSet::new(cap),
            kind: boxed(cap, WeaponKind::BeamRifle),
            owner: boxed(cap, 0u16),
            owner_faction: boxed(cap, Faction::Oz),
            pos: boxed(cap, Vec3::ZERO),
            vel: boxed(cap, Vec3::ZERO),
            expire: boxed(cap, 0u32),
            damage: boxed(cap, 0.0f32),
            radius: boxed(cap, 0.0f32),
            free: FreeList::full(cap),
            count: 0,
        }
    }

    /// Spawns a projectile; `false` if the pool is exhausted (the shot fizzles).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        kind: WeaponKind,
        owner: u16,
        owner_faction: Faction,
        pos: Vec3,
        vel: Vec3,
        expire: u32,
        damage: f32,
        radius: f32,
    ) -> bool {
        let Some(i) = self.free.pop() else { return false };
        let i = i as usize;
        self.alive.set(i, true);
        self.kind[i] = kind;
        self.owner[i] = owner;
        self.owner_faction[i] = owner_faction;
        self.pos[i] = pos;
        self.vel[i] = vel;
        self.expire[i] = expire;
        self.damage[i] = damage;
        self.radius[i] = radius;
        self.count += 1;
        true
    }

    pub fn kill(&mut self, i: usize) {
        if self.alive.get(i) {
            self.alive.set(i, false);
            self.free.push(i as u16);
            self.count -= 1;
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}
