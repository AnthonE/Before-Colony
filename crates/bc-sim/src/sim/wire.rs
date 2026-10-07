//! Conversions from simulation state to the wire structs in `bc-proto`.

use bc_proto::snapshot::{
    OwnArms, ZERO_THREATS as WIRE_THREATS, ZeroThreat, ent_flags, footing, own_flags, part_buckets, zero_mode,
};
use bc_proto::{EntityState, ObjectState, OwnState, OwnSurface, RiderOn, RockState, ZeroInfo};
use glam::Vec3;

use super::Sim;
use crate::arms::phase_to_wire;
use crate::bodies::Body;
use crate::chunks::Motion;
use crate::content::systems::{DAMAGED, FAILED, OK, System};
use crate::content::{SpecialKind, WeaponClass, frame, weapon};
use crate::ground::Footing;
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

    /// Full-precision state of suit `i` for its own pilot. On a body it is the state the suit moves
    /// in there: its anchor, in the body's frame.
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
                        && s.fitted(i, slot)
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
        // Inside the colony nothing is ready to fire but in the Blast Hall.
        if self.interior() && !crate::colony::hall::weapons_free(f.pos) {
            ready = 0;
        }
        let charge = spec.loadout[0]
            .map(|m| weapon(m.weapon))
            .filter(|w| w.charge_span() > 0)
            .map_or(0.0, |w| f32::from(s.weapons[i][0].charge) / f32::from(w.charge_span()));
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
        if spec.loadout[0].is_some_and(|m| crate::arms::charging(s.weapons[i][0].charge, weapon(m.weapon))) {
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
        // (What it could park on is worked out once a tick, in `cover_step`.)
        if s.parkable[i] != Body::None {
            flags |= own_flags::PARKABLE;
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
                (u32::from(l.progress) * 15 / u32::from(super::missile::lock_full(&m).max(1))) as u8
            }
        });
        if s.special[i].active {
            flags |= own_flags::SPECIAL_ACTIVE;
        }
        if self.transforming(i) {
            flags |= own_flags::TRANSFORMING;
        }
        // The special's timer: a change of form, Full Open, or (the jammer) the break until it
        // hides the suit again.
        let special_timer = match spec.special {
            SpecialKind::HyperJammer { .. } => s.special[i].break_until.saturating_sub(t),
            _ => u32::from(s.special[i].timer),
        };
        let respawn_in =
            if s.alive.get(i) { 0 } else { (s.respawn_at[i].saturating_sub(t) / 4).min(255) as u8 };
        let a = &s.anchor[i];
        let on = match s.footing[i] {
            Footing::Free => None,
            Footing::Grounded => Some(footing::GROUNDED),
            Footing::Aloft => Some(footing::ALOFT),
        };
        let surface = on.zip(a.body.to_wire()).map(|(footing, body)| OwnSurface {
            footing,
            body,
            // (On the sixteenth-metre grid: exact.)
            stance_q: (a.stance * 16.0 + 0.5) as u8,
        });
        let (pos, vel, rot, ang_vel) = match surface {
            Some(_) => (a.local, a.vel, a.rot, a.ang_vel),
            None => (f.pos, f.vel, f.rot, f.ang_vel),
        };
        OwnState {
            slot: i as u16,
            generation: (s.generation[i] & 3) as u8,
            frame: s.frame[i],
            alive: s.alive.get(i),
            pos,
            vel,
            rot,
            ang_vel,
            propellant: f.propellant,
            g_strain: f.g_strain,
            heat: (s.heat[i] / spec.heat_cap).clamp(0.0, 1.0),
            energy: (s.energy[i] / (spec.energy_cap * s.tuning[i].energy_cap)).clamp(0.0, 1.0),
            ammo: [s.weapons[i][0].ammo, s.weapons[i][1].ammo],
            weapon_ready: ready,
            charge,
            parts: s.part_fractions(i),
            zero_strain: s.zero[i].strain,
            zero_mode: s.zero[i].mode,
            flags,
            systems: s.systems[i].0,
            modules: s.modules[i].0,
            grade: s.grade[i] as u8,
            scram: s.status[i].scram,
            concussed: s.status[i].concussed,
            repairing: s.status[i].repairing,
            repair_left: s.status[i].repair_left.div_ceil(8).min(127) as u8,
            respawn_in,
            kits: s.kits[i].0,
            stim: s.status[i].stim,
            extra_mass_kg: mods.extra_mass_kg,
            cargo_kg: s.cargo_kg[i],
            credits: s.credits[i],
            held: self.held_chunk(i).map_or(bc_proto::NO_CHUNK, |k| k as u16),
            lock_target: self.designation(i).map_or(bc_proto::NO_SLOT, |j| j as u16),
            lock_progress,
            special_timer: special_timer.min(255) as u8,
            special_cooldown: s.special[i].cooldown.div_ceil(4).min(255) as u8,
            arms: self.own_arms(i),
            burst: f.burst,
            surface,
            cover: self.cover_code(i),
        }
    }

    /// Suit `i`'s arms for its own pilot's client, which rolls them on tick by tick
    /// (`crate::arms::ArmsClock`). A mount waits for its cooldown; one that can't fire or strike
    /// for a reason the client can't see run out (an arm gone, energy or rounds short) waits
    /// [`OwnArms::NEVER`]. Heat goes in the flags.
    fn own_arms(&self, i: usize) -> OwnArms {
        let s = &self.suits;
        let spec = frame(s.frame[i]);
        let mut arms = OwnArms {
            phase: phase_to_wire(s.melee[i].phase),
            timer: s.melee[i].timer,
            slot: s.melee[i].slot,
            fired_ago: self.tick().saturating_sub(s.last_fired[i]).min(u32::from(OwnArms::LONG_AGO)) as u8,
            ..OwnArms::default()
        };
        for (slot, mount) in spec.loadout.iter().enumerate() {
            let Some(mount) = *mount else { continue };
            let w = weapon(mount.weapon);
            let ws = &s.weapons[i][slot];
            let arm = s.arm_free(i, mount.arm);
            let can = s.energy[i] >= w.energy
                && s.fitted(i, slot)
                && match w.class {
                    WeaponClass::Melee => w.melee.is_some_and(|m| self.melee_arms_ok(i, mount, &m)),
                    // A flame is lit while fire is held, whatever its cooldown.
                    WeaponClass::Cone => arm && ws.ammo > 0,
                    WeaponClass::Beam | WeaponClass::Ballistic | WeaponClass::Missile => {
                        arm && (w.ammo == 0 || ws.ammo > 0)
                    }
                };
            if can {
                arms.wait[slot] = if w.class == WeaponClass::Cone { 0 } else { OwnArms::wait(ws.cooldown) };
            }
            // A salvo under way runs on until it's out of rounds or the arm's gone.
            if w.class == WeaponClass::Missile && slot < 2 && arm && ws.ammo > 0 {
                (arms.salvo[slot], arms.salvo_gap[slot]) = (ws.salvo, ws.gap);
            }
        }
        let cooldown = OwnArms::wait(s.special[i].cooldown);
        arms.wait[3] = match spec.special {
            SpecialKind::MeleeMove { .. } => spec
                .melee_mount(SPECIAL_MOUNT)
                .map(|mount| (mount, weapon(mount.weapon)))
                .filter(|(mount, w)| {
                    s.energy[i] >= w.energy && w.melee.is_some_and(|m| self.melee_arms_ok(i, *mount, &m))
                })
                .map_or(OwnArms::NEVER, |_| cooldown),
            SpecialKind::FullOpen { .. } => cooldown,
            SpecialKind::HyperJammer { .. } | SpecialKind::Transform { .. } | SpecialKind::None => {
                OwnArms::NEVER
            }
        };
        arms
    }

    /// Suit `j` as replicated to `viewer`. On a body (standing on it, in its grip, or parked on it)
    /// it goes in the body's frame, a parked suit at rest there; otherwise in the sector's.
    pub fn entity_state(&self, j: usize, viewer: usize) -> EntityState {
        self.entity_state_for(j, Some(viewer))
    }

    /// Suit `j` as a spectator sees it, who has no suit (a pilot on foot watching the colony's
    /// inside): as [`Sim::entity_state`], but nothing is locked on to them, and they're nobody's
    /// ally.
    pub fn entity_state_watched(&self, j: usize) -> EntityState {
        self.entity_state_for(j, None)
    }

    fn entity_state_for(&self, j: usize, viewer: Option<usize>) -> EntityState {
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
        if frame(s.frame[j]).loadout[0]
            .is_some_and(|m| crate::arms::charging(s.weapons[j][0].charge, weapon(m.weapon)))
        {
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
        if s.sleeping.get(j) {
            flags |= ent_flags::ASLEEP;
        }
        // What's broken inside shows: sparks, smoke, a vapour trail (not on a wreck).
        if s.alive.get(j) {
            let gone = s.gone_mask(j);
            flags |= match s.systems[j].worst(gone) {
                FAILED => ent_flags::SMOKING,
                DAMAGED => ent_flags::SPARKING,
                _ => 0,
            };
            if s.systems[j].level(System::Tank, gone) != OK {
                flags |= ent_flags::VENTING;
            }
        }
        if let Some(viewer) = viewer
            && s.input[j].lock_target == viewer as u16
            && self.designation(j) == Some(viewer)
        {
            flags |= ent_flags::LOCKED_ON_YOU;
        }
        // Allies see a jamming suit's shimmer; its enemies (close enough to see it at all) don't.
        // Full Open shows to everyone.
        if (self.jamming(j).is_some() && viewer.is_some_and(|v| s.faction[j] == s.faction[v]))
            || self.full_open(j)
            || self.transforming(j)
        {
            flags |= ent_flags::SPECIAL;
        }
        let a = &s.anchor[j];
        let parked = self.is_parked(j);
        let on = a
            .body
            .to_wire()
            .filter(|_| parked || s.footing[j] != Footing::Free)
            .map(|body| RiderOn { body, aloft: s.footing[j] == Footing::Aloft });
        let (pos, rot, vel) = match on {
            Some(_) => (a.local, a.rot, if parked { Vec3::ZERO } else { a.vel }),
            None => (f.pos, f.rot, f.vel),
        };
        EntityState {
            slot: j as u16,
            generation: (s.generation[j] & 3) as u8,
            frame: s.frame[j],
            faction: s.faction[j],
            pilot: s.pilot[j],
            on,
            pos,
            rot,
            vel,
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
