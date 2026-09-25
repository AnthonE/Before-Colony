//! Conversions from simulation state to the wire structs in `bc-proto`.

use bc_proto::snapshot::{
    ZERO_THREATS as WIRE_THREATS, ZeroThreat, ent_flags, own_flags, part_buckets, zero_mode,
};
use bc_proto::{EntityState, ObjectState, OwnState, RockState, ZeroInfo};

use super::Sim;
use crate::chunks::Motion;
use crate::content::{SpecialKind, WeaponClass, frame, weapon};
use crate::rocks::{max_hp, max_ore_kg};
use crate::suits::{MeleePhase, SPECIAL_MOUNT, SuitStats};

impl Sim {
    pub fn is_used(&self, i: usize) -> bool {
        i < self.suits.cap && self.suits.used.get(i)
    }

    pub fn is_alive(&self, i: usize) -> bool {
        self.suits.is_alive(i)
    }

    pub fn stats(&self, i: usize) -> SuitStats {
        self.suits.stats[i]
    }

    /// Whether `viewer`'s sensors pick up suit `j` (alive or a fresh wreck).
    pub fn visible_to(&self, viewer: usize, j: usize) -> bool {
        let s = &self.suits;
        if !s.used.get(j) || j == viewer {
            return false;
        }
        let wreck = !s.alive.get(j);
        if wreck
            && s.respawn_at[j] != 0
            && self.tick() + 60 < s.respawn_at[j] + 60
            && s.pilot[j] != bc_proto::PilotKind::MobileDoll
        {
            // Dead pilots waiting to respawn vanish after their wreck has been seen for a moment.
            if self.tick() > s.respawn_at[j].saturating_sub(crate::config::secs(self.cfg.respawn_secs) - 60) {
                return false;
            }
        }
        self.detects(viewer, j)
    }

    /// Full-precision state of suit `i` for its own pilot.
    pub fn own_state(&self, i: usize) -> OwnState {
        let s = &self.suits;
        let spec = frame(s.frame[i]);
        let f = &s.flight[i];
        let mods = self.flight_mods(i);
        let t = self.tick();
        let mut ready = 0u8;
        for slot in 0..3 {
            if let Some(m) = spec.loadout[slot] {
                let w = weapon(m.weapon);
                let ws = &s.weapons[i][slot];
                let ok = if w.class == WeaponClass::Melee {
                    self.melee_ready(i, slot as u8)
                } else {
                    ws.cooldown == 0
                        && s.energy[i] >= w.energy
                        && (w.ammo == 0 || ws.ammo > 0)
                        && !s.overheated[i]
                        && s.arm_free(i, m.arm)
                        && !self.arm_blocked(i, m.arm)
                };
                if ok {
                    ready |= 1 << slot;
                }
            }
        }
        if self.special_ready(i) {
            ready |= 1 << 3;
        }
        let charge = spec.loadout[0]
            .map(|m| weapon(m.weapon))
            .filter(|w| w.charge_ticks > 0)
            .map_or(0.0, |w| f32::from(s.weapons[i][0].charge) / f32::from(w.charge_ticks));
        let mut flags = 0u16;
        if s.boosting[i] {
            flags |= own_flags::BOOSTING;
        }
        if f.blackout {
            flags |= own_flags::BLACKOUT;
        }
        if s.overheated[i] {
            flags |= own_flags::OVERHEAT;
        }
        if charge > 0.0 {
            flags |= own_flags::CHARGING;
        }
        let melee = &s.melee[i];
        if melee.phase != MeleePhase::Idle {
            flags |= own_flags::SABER_ACTIVE;
            if melee.slot == SPECIAL_MOUNT {
                flags |= own_flags::SPECIAL_ACTIVE;
            }
        }
        if mods.lunge {
            flags |= own_flags::LUNGE;
        }
        if s.alive.get(i) && self.docked(i) {
            flags |= own_flags::DOCKED;
        }
        if spec.zero || self.cfg.zero_on_all_frames {
            flags |= own_flags::ZERO_CAPABLE;
        }
        if s.input[i].pressed(bc_proto::buttons::FLIGHT_ASSIST) {
            flags |= own_flags::FLIGHT_ASSIST;
        }
        // Locked on by someone it can see (a jamming suit's lock goes unnoticed), perhaps with a
        // missile lock acquired.
        for j in s.alive.iter() {
            if s.input[j].lock_target == i as u16 && self.designation(j) == Some(i) && !self.jammed_from(i, j)
            {
                flags |= own_flags::LOCKED_ON;
                if self.missile_lock(j) == Some(i) {
                    flags |= own_flags::MISSILE_LOCK;
                }
            }
        }
        if s.incoming[i] > 0 {
            flags |= own_flags::MISSILE_INCOMING;
        }
        if self.missile_lock(i).is_some() {
            flags |= own_flags::LOCK_ACQUIRED;
        }
        let lock_progress = spec.lock_spec().map_or(0, |m| {
            let l = &s.lock[i];
            if l.target == bc_proto::NO_SLOT {
                0
            } else {
                (u32::from(l.progress) * 15 / u32::from(m.lock_ticks)) as u8
            }
        });
        if s.special[i].active {
            flags |= own_flags::SPECIAL_ACTIVE;
        }
        // The special's timer: a change of form, Full Open, or (the jammer) the break until it
        // hides the suit again.
        let special_timer = match spec.special {
            SpecialKind::HyperJammer { .. } => s.special[i].break_until.saturating_sub(t),
            _ => u32::from(s.special[i].timer),
        };
        let respawn_in =
            if s.alive.get(i) { 0 } else { (s.respawn_at[i].saturating_sub(t) / 4).min(255) as u8 };
        OwnState {
            slot: i as u16,
            generation: (s.generation[i] & 3) as u8,
            frame: s.frame[i],
            alive: s.alive.get(i),
            pos: f.pos,
            vel: f.vel,
            rot: f.rot,
            ang_vel: f.ang_vel,
            propellant: f.propellant,
            g_strain: f.g_strain,
            heat: (s.heat[i] / spec.heat_cap).clamp(0.0, 1.0),
            energy: (s.energy[i] / spec.energy_cap).clamp(0.0, 1.0),
            ammo: [s.weapons[i][0].ammo, s.weapons[i][1].ammo],
            weapon_ready: ready,
            charge,
            parts: s.part_fractions(i),
            zero_strain: s.zero[i].strain,
            zero_mode: s.zero[i].mode,
            flags,
            ambac_factor: mods.ambac,
            thrust_factor: mods.thrust,
            respawn_in,
            extra_mass_kg: mods.extra_mass_kg,
            cargo_kg: s.cargo_kg[i],
            credits: s.credits[i],
            held: self.held_chunk(i).map_or(bc_proto::NO_CHUNK, |k| k as u16),
            lock_target: self.designation(i).map_or(bc_proto::NO_SLOT, |j| j as u16),
            lock_progress,
            special_timer: special_timer.min(255) as u8,
            special_cooldown: s.special[i].cooldown.div_ceil(4).min(255) as u8,
        }
    }

    /// Suit `j` as replicated to `viewer`.
    pub fn entity_state(&self, j: usize, viewer: usize) -> EntityState {
        let s = &self.suits;
        let f = &s.flight[j];
        let t = self.tick();
        let mut flags = 0u16;
        if t.saturating_sub(s.fired_primary[j]) < 4 && s.fired_primary[j] != 0 {
            flags |= ent_flags::FIRING_PRIMARY;
        }
        if t.saturating_sub(s.fired_secondary[j]) < 4 && s.fired_secondary[j] != 0 {
            flags |= ent_flags::FIRING_SECONDARY;
        }
        let melee = &s.melee[j];
        if melee.striking() {
            flags |= ent_flags::SABER;
            if melee.slot < 2 {
                flags |= ent_flags::MELEE_ALT;
            } else if melee.slot == SPECIAL_MOUNT {
                flags |= ent_flags::SPECIAL;
            }
        }
        if s.boosting[j] {
            flags |= ent_flags::BOOST;
        }
        if s.weapons[j][0].charge > 0 {
            flags |= ent_flags::CHARGING;
        }
        if s.zero[j].active() {
            flags |= ent_flags::ZERO;
        }
        if s.zero[j].mode == zero_mode::SEIZED {
            flags |= ent_flags::SEIZED;
        }
        if s.overheated[j] {
            flags |= ent_flags::OVERHEAT;
        }
        if !s.alive.get(j) {
            flags |= ent_flags::WRECK;
        }
        if s.input[j].lock_target == viewer as u16 && self.designation(j) == Some(viewer) {
            flags |= ent_flags::LOCKED_ON_YOU;
        }
        // Allies see a jamming suit's shimmer; its enemies (close enough to see it at all) don't.
        // Full Open shows to everyone.
        if (self.jamming(j).is_some() && s.faction[j] == s.faction[viewer]) || self.full_open(j) {
            flags |= ent_flags::SPECIAL;
        }
        EntityState {
            slot: j as u16,
            generation: (s.generation[j] & 3) as u8,
            frame: s.frame[j],
            faction: s.faction[j],
            pilot: s.pilot[j],
            pos: f.pos,
            rot: f.rot,
            vel: f.vel,
            aim: s.aim[j],
            flags,
            parts: part_buckets(&s.part_fractions(j)),
        }
    }

    /// Chunk `k` as replicated.
    pub fn object_state(&self, k: usize) -> ObjectState {
        let c = &self.chunks;
        let (id, generation, desc) = (k as u16, c.generation[k] & 3, c.desc[k]);
        match c.motion[k] {
            Motion::Free(seg) => ObjectState::Free { id, generation, desc, seg },
            Motion::Held { holder, right, rot, since } => {
                ObjectState::Held { id, generation, desc, holder, right, rot, since }
            }
        }
    }

    /// Rock `i` as replicated.
    pub fn rock_state(&self, i: usize) -> RockState {
        let r = &self.field.rocks()[i];
        let s = &self.rocks;
        let ore = s.ore_kg[i] as f32 / max_ore_kg(r).max(1) as f32;
        RockState::new(i as u16, s.destroyed.get(i), s.hp[i] / max_hp(r), ore)
    }

    /// What the ZERO System shows pilot `i` (only while engaged).
    pub fn zero_info(&self, i: usize) -> Option<ZeroInfo> {
        let z = &self.suits.zero[i];
        if !z.active() || z.out.computed_at == 0 {
            return None;
        }
        let o = &z.out;
        let mut info = ZeroInfo {
            source_jev: o.source_jev,
            advice_age: o.advice_age,
            threat_count: o.n_threats.min(WIRE_THREATS as u8),
            rec_target: o.rec_target,
            rec_target_p: o.rec_target_p,
            rec_maneuver: o.rec_maneuver,
            rec_maneuver_p: o.rec_maneuver_p,
            threat_level: o.threat_level,
            threat_confidence: o.threat_confidence,
            flanked: o.flanked,
            has_solution: o.has_solution,
            solution: o.solution,
            hit_p: o.hit_p,
            ..ZeroInfo::default()
        };
        for k in 0..info.threat_count as usize {
            info.threats[k] = ZeroThreat { slot: o.threats[k].slot, probs: o.threats[k].probs };
        }
        Some(info)
    }
}
