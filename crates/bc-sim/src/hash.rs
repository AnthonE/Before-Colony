//! Deterministic digest of the simulation state (FNV-1a), for golden tests across targets.

use crate::chunks::Motion;
use crate::sim::Sim;

struct Fnv(u64);

impl Fnv {
    fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
}

pub fn state_hash(sim: &Sim) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    h.u32(sim.tick());
    let s = &sim.suits;
    for i in s.used.iter() {
        h.u32(i as u32);
        h.u32(u32::from(s.alive.get(i)));
        let f = &s.flight[i];
        for v in [f.pos, f.vel, f.ang_vel] {
            h.f32(v.x);
            h.f32(v.y);
            h.f32(v.z);
        }
        for c in f.rot.to_array() {
            h.f32(c);
        }
        h.f32(f.propellant);
        h.f32(f.g_strain);
        for p in s.part_hp[i] {
            h.f32(p);
        }
        h.f32(s.heat[i]);
        h.f32(s.energy[i]);
    }
    let p = &sim.projectiles;
    for k in p.alive.iter() {
        h.u32(k as u32);
        h.f32(p.pos[k].x);
        h.f32(p.pos[k].y);
        h.f32(p.pos[k].z);
    }
    let c = &sim.chunks;
    for k in c.alive.iter() {
        h.u32(k as u32);
        h.u32(u32::from(c.generation[k]) | u32::from(c.version[k]) << 8 | u32::from(c.desc[k].seed) << 16);
        h.u32(c.desc[k].mass_kg);
        h.u32(c.expire[k]);
        match c.motion[k] {
            Motion::Free(s) => {
                h.u32(s.t0);
                for v in [s.pos, s.vel, s.spin] {
                    h.f32(v.x);
                    h.f32(v.y);
                    h.f32(v.z);
                }
                for q in s.rot.to_array() {
                    h.f32(q);
                }
            }
            Motion::Held { holder, right, rot, since } => {
                h.u32(u32::from(holder) | u32::from(right) << 16);
                h.u32(since);
                for q in rot.to_array() {
                    h.f32(q);
                }
            }
        }
    }
    let r = &sim.rocks;
    for i in 0..r.version.len() {
        if r.version[i] != 0 {
            h.u32(i as u32 | u32::from(r.version[i]) << 16 | u32::from(r.destroyed.get(i)) << 24);
            h.f32(r.hp[i]);
            h.u32(r.ore_kg[i]);
            h.u32(r.regrow_at[i]);
        }
    }
    h.u32(sim.events.next_seq());
    h.0
}
