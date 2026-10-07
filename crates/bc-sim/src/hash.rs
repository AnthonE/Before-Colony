//! Deterministic digest of the simulation state (FNV-1a), for golden tests across targets.

use crate::chunks::Motion;
use crate::ground::Footing;
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
        h.u32(s.systems[i].0);
        h.u32(s.modules[i].0);
        let st = &s.status[i];
        h.u32(u32::from(st.scram) | u32::from(st.concussed) << 8 | u32::from(st.repairing) << 16);
        h.u32(u32::from(st.repair_left));
        // Consumables (only once there are any, so a suit without hashes as it always has).
        if !s.kits[i].is_empty() || st.stim > 0 || st.chaff > 0 {
            h.u32(u32::from(s.kits[i].0) | u32::from(st.chaff) << 8 | u32::from(st.stim) << 16);
        }
        let (k, g, right) = s.held[i];
        h.u32(u32::from(k) | u32::from(g) << 16 | u32::from(right) << 24);
        // Doomed, or blown apart (only then: a suit never breached hashes as it always has).
        let d = s.doom[i];
        if d.left > 0 || s.blown.get(i) {
            h.u32(0xD00E_0000 | u32::from(d.left) | u32::from(s.blown.get(i)) << 15);
            h.u32(u32::from(d.by));
        }
        for kg in s.cargo_kg[i] {
            h.u32(u32::from(kg));
        }
        h.u32(s.credits[i]);
        // When it last fought: what decides how long it takes to go dark.
        let quiet_from = s.last_fired[i].max(s.last_hit[i]);
        let a = &s.anchor[i];
        if s.sleeping.get(i) {
            h.u32(s.slept_at[i]);
            h.u32(a.body.code());
            for v in [a.local.x, a.local.y, a.local.z] {
                h.f32(v);
            }
            for c in a.rot.to_array() {
                h.f32(c);
            }
            h.u32(quiet_from);
            h.u32(u32::from(s.hide_spot[i]));
        }
        // On a body (only then: a suit that never grips hashes as it always has).
        if s.footing[i] != Footing::Free {
            h.u32(0xF007_0000 | s.footing[i] as u32);
            h.u32(a.body.code());
            for v in a.local.to_array().into_iter().chain(a.rot.to_array()) {
                h.f32(v);
            }
            for v in [a.vel, a.ang_vel] {
                h.f32(v.x);
                h.f32(v.y);
                h.f32(v.z);
            }
            h.f32(a.stance);
            if !s.sleeping.get(i) {
                h.u32(quiet_from);
                h.u32(s.still_since[i]);
                h.u32(u32::from(s.hide_spot[i]));
            }
        }
    }
    let p = &sim.projectiles;
    for k in p.alive.iter() {
        h.u32(k as u32);
        h.f32(p.pos[k].x);
        h.f32(p.pos[k].y);
        h.f32(p.pos[k].z);
    }
    let m = &sim.missiles;
    for k in m.alive.iter() {
        h.u32(k as u32 | u32::from(m.target[k]) << 16);
        for v in [m.pos[k], m.vel[k]] {
            h.f32(v.x);
            h.f32(v.y);
            h.f32(v.z);
        }
        h.f32(m.dv_left[k]);
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
