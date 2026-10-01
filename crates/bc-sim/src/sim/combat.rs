//! Guns, projectiles and damage (blades are in `melee`).

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY};
use bc_proto::events::Event;
use bc_proto::{ChunkDesc, ChunkKind, InputCmd, NO_CHUNK, Part, PilotKind, Segment, WeaponKind};
use glam::Vec3;

use super::{DamageEvent, Sim};
use crate::bodies::{Body, landmark_pose, sweep_landmarks};
use crate::chunks::Motion;
use crate::collide::{segment_near_point, sweep_capsules};
use crate::config::{DT, MAX_REWIND_TICKS, secs};
use crate::content::salvage::{DETACH_PUSH, DETACH_SPEED, bounty, mass_without, part_mass_kg, wreck_ttl};
use crate::content::systems::{self, FAILED, System};
use crate::content::{ArmSlot, Mount, Replication, WeaponClass, WeaponSpec, frame, weapon};
use crate::flight::FlightState;
use crate::ground::{Footing, derive};
use crate::math::{angle_between, clamp_to_cone, hash01, normalize_or};
use crate::suits::{SPECIAL_SLOTS, WeaponState};
use crate::tuning;
use crate::world::colony_sweep;

/// What stops a shot, a missile or a flame short of a suit ([`Sim::first_blocker`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blocker {
    /// An asteroid of the field, by index (a shot works it: `rock_hit`).
    Rock(usize),
    /// A landmark, by index.
    Landmark(u8),
    /// The colony's hull, or an end cap.
    Colony,
}

/// ZERO fire-time magnetism: shots this close to the ZERO firing solution snap to it.
pub const MAGNET_ANGLE: f32 = 0.026; // 1.5°

impl Sim {
    /// Queues a hit on `target`'s `part`, struck along `dir`.
    pub(super) fn queue_damage(
        &mut self,
        target: usize,
        part: Part,
        amount: f32,
        shooter: usize,
        weapon: WeaponKind,
        dir: Vec3,
    ) {
        let _ = self.damage.try_push(DamageEvent {
            target: target as u16,
            part,
            amount,
            shooter: shooter as u16,
            weapon,
            dir,
        });
    }

    /// A blow from nowhere in particular: `amount` on `target`'s `part`, as if `shooter`'s `weapon`
    /// had landed it, resolved with this tick's other hits (tools and tests).
    pub fn strike(&mut self, target: usize, part: Part, amount: f32, shooter: usize, weapon: WeaponKind) {
        self.queue_damage(target, part, amount, shooter, weapon, Vec3::Z);
    }

    pub(super) fn weapons_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let spec = frame(self.suits.frame[i]);
            let cmd = self.suits.input[i];
            // Weapons are down while the suit changes form.
            if self.transforming(i) {
                continue;
            }
            // Full Open: everything fires along the aim, heat or not.
            let full_open = self.full_open(i);
            for slot in 0..2 {
                let Some(mount) = spec.loadout[slot] else { continue };
                let button = if slot == 0 { FIRE_PRIMARY } else { FIRE_SECONDARY };
                let wants = full_open || cmd.pressed(button);
                self.trigger(i, slot, mount, wants, full_open, &cmd, t);
            }
            if full_open {
                for (k, mount) in spec.special_mounts.iter().enumerate() {
                    if let Some(mount) = *mount {
                        self.trigger(i, SPECIAL_SLOTS + k, mount, true, true, &cmd, t);
                    }
                }
            }
        }
        self.iter_bits = alive;
    }

    /// One weapon's tick: `slot` is a loadout slot (0, 1) or a special mount (from
    /// [`SPECIAL_SLOTS`]). Its cooldown runs down; if `wants` and it's ready, a gun fires and a
    /// launcher starts a salvo. `heedless` fires through an overheat (Full Open).
    #[allow(clippy::too_many_arguments)]
    fn trigger(
        &mut self,
        i: usize,
        slot: usize,
        mount: Mount,
        wants: bool,
        heedless: bool,
        cmd: &InputCmd,
        t: u32,
    ) {
        let w = weapon(mount.weapon);
        match w.class {
            WeaponClass::Beam | WeaponClass::Ballistic | WeaponClass::Missile => {}
            WeaponClass::Cone => return self.flame(i, slot, mount, w, cmd, t),
            // Blades strike in `melee_step` (the Dragon Fang is a primary).
            WeaponClass::Melee => return,
        }
        let mut ws: WeaponState = *self.suits.weapon_state(i, slot);
        ws.cooldown = ws.cooldown.saturating_sub(1);
        let arm_ok = self.suits.fitted(i, slot)
            && self.suits.arm_free(i, mount.arm)
            && !self.arm_blocked(i, mount.arm);
        let ready = ws.cooldown == 0
            && (heedless || !self.suits.overheated[i])
            && self.suits.energy[i] >= w.energy
            && (w.ammo == 0 || ws.ammo > 0)
            && arm_ok;
        if w.class == WeaponClass::Missile {
            if wants && ready && ws.salvo == 0 {
                self.start_salvo(i, &mut ws, w);
            }
            self.salvo_tick(i, slot, &mut ws, mount, w, cmd, t);
            *self.suits.weapon_state(i, slot) = ws;
            return;
        }
        let fire = if w.charge_ticks > 0 {
            // Charged weapons: hold to charge, fires automatically when full; releasing early
            // cancels. The charge glow is replicated, so everyone sees it coming.
            if wants && ready {
                ws.charge += 1;
                if ws.charge >= w.charge_ticks {
                    ws.charge = 0;
                    true
                } else {
                    false
                }
            } else {
                ws.charge = 0;
                false
            }
        } else {
            wants && ready
        };
        *self.suits.weapon_state(i, slot) = ws;
        if fire {
            self.fire(i, slot, mount, w, cmd, t);
        }
    }

    fn fire(&mut self, i: usize, slot: usize, mount: Mount, w: &WeaponSpec, cmd: &InputCmd, t: u32) {
        // Lag compensation for remote pilots: fly the shot through the world as they saw it, but
        // no further back than MAX_REWIND_TICKS (a view older than that counts as exactly that old).
        let view_q4 = cmd.view_tick_q4.max(t.saturating_sub(MAX_REWIND_TICKS) << 4);
        let rewind =
            if self.suits.pilot[i] == PilotKind::MobileDoll { 0 } else { t.saturating_sub(view_q4 >> 4) };
        let frac = if rewind > 0 { (view_q4 & 15) as f32 / 16.0 } else { 0.0 };
        let spawn_tick = t - rewind;
        let mut f = self.suits.flight[i];
        // On a body, the shot leaves where its pilot saw the suit: on the body as it was then.
        if rewind > 0 {
            self.as_seen_on_its_body(i, spawn_tick, frac, &mut f);
        }
        let fwd = f.rot * Vec3::Z;
        let mut dir = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, self.cone(i, mount.arm));
        let z = &self.suits.zero[i];
        // ZERO's pull needs the fire control that aims by it.
        if self.suits.tuning[i].magnetism
            && z.active()
            && z.out.has_solution
            && t.saturating_sub(z.out.computed_at) <= 6
            && angle_between(dir, z.out.solution) <= MAGNET_ANGLE
        {
            dir = z.out.solution;
        }
        if w.spread > 0.0 {
            let seed = (i as u32) * 7 + slot as u32;
            let n =
                Vec3::new(hash01(t, seed) - 0.5, hash01(t ^ 0x55, seed) - 0.5, hash01(t ^ 0xAA, seed) - 0.5);
            dir = normalize_or(dir + n * (2.0 * w.spread), dir);
        }
        // A concussed pilot's hands shake.
        if self.suits.status[i].concussed > 0 {
            dir = tuning::wobble(dir, t, i as u16, slot);
        }
        let muzzle = f.pos + f.rot * mount.arm.muzzle();
        let vel = f.vel + dir * w.speed;

        let s = &mut self.suits;
        let ws = s.weapon_state(i, slot);
        ws.cooldown = w.cooldown;
        if w.ammo > 0 {
            ws.ammo -= 1;
        }
        s.heat[i] += w.heat;
        s.energy[i] -= w.energy;
        s.last_fired[i] = t;
        s.stats[i].shots += 1;
        // (The special mounts fire only in Full Open, which shows by itself.)
        if slot == 0 {
            s.fired_primary[i] = t;
        } else if slot == 1 {
            s.fired_secondary[i] = t;
        }
        self.break_jammer(i, t);
        let faction = self.suits.faction[i];
        // Rocks, landmarks and the colony stop it too, the landmarks where they were then.
        let mut p = muzzle;
        let mut hit = None;
        let mut blocked = None;
        for k in 0..rewind {
            let b = p + vel * DT;
            let when = spawn_tick + k;
            let blocker = self.first_blocker(p, b, w.radius, when, frac);
            match self.sweep_history(p, b, w.radius, i, faction, when, frac) {
                Some((s, j, part)) if blocker.is_none_or(|(t, _)| s <= t) => {
                    hit = Some((j, part));
                    break;
                }
                _ if blocker.is_some() => {
                    blocked = blocker.map(|(f, what)| (what, p + (b - p) * f));
                    break;
                }
                _ => {}
            }
            p = b;
        }
        if w.replication == Replication::PerShot {
            self.events.push(Event::BeamSpawn {
                id: 0,
                tick: spawn_tick,
                shooter: i as u16,
                weapon: w.kind,
                shot_seq: cmd.shot_seq,
                origin: muzzle,
                velocity: vel,
            });
        }
        match (hit, blocked) {
            (Some((target, part)), _) => self.queue_damage(target, part, w.damage, i, w.kind, dir),
            (None, Some((Blocker::Rock(rock), at))) => self.rock_hit(rock, w.damage, w.kind, at, dir, i, t),
            // A landmark or the colony just takes it.
            (None, Some(_)) => {}
            (None, None) => {
                let ttl = w.ttl_ticks().saturating_sub(rewind);
                if ttl > 0 {
                    self.projectiles.spawn(w.kind, i as u16, faction, p, vel, t + ttl, w.damage, w.radius);
                }
            }
        }
    }

    /// Where suit `i`'s pilot saw it at tick `t` plus `frac` of the next, into `f` (its world
    /// state now): on its body, if it's on one that moves, its anchor as it is on the body as the
    /// body was then. The pilot's view drew it there, glued to the deck it saw, and saw everyone
    /// else as they were then (lag compensation), so a shot or a blade leaves from there. Free, or
    /// on a body that doesn't move, it's where it is.
    pub(super) fn as_seen_on_its_body(&self, i: usize, t: u32, frac: f32, f: &mut FlightState) {
        if self.suits.footing[i] == Footing::Free {
            return;
        }
        let a = self.suits.anchor[i];
        let Body::Landmark(k) = a.body else { return };
        let Some(d) = self.landmarks().get(usize::from(k)) else { return };
        let pose = landmark_pose(d, t, frac);
        if pose.moving {
            derive(&pose, &a, f);
        }
    }

    /// The first thing other than a suit that a sphere of radius `r` meets moving from `a` to `b`:
    /// a rock, a landmark as it was at tick `t` plus `frac` of the next, or the colony. How far
    /// along (0..1), and which. A tie goes to the rock, then the landmark. Shots, missiles and
    /// flame all stop at it; a suit before it, or level with it, is hit.
    pub(crate) fn first_blocker(
        &self,
        a: Vec3,
        b: Vec3,
        r: f32,
        t: u32,
        frac: f32,
    ) -> Option<(f32, Blocker)> {
        let mut first = self.field.sweep(a, b, r).map(|(s, i)| (s, Blocker::Rock(i)));
        let mut meet = |s: f32, what: Blocker| {
            if first.is_none_or(|(f, _)| s < f) {
                first = Some((s, what));
            }
        };
        if let Some((s, k)) = sweep_landmarks(self.landmarks(), a, b, r, t, frac) {
            meet(s, Blocker::Landmark(k));
        }
        if let Some(s) = colony_sweep(a, b, r) {
            meet(s, Blocker::Colony);
        }
        first
    }

    /// Swept test against suits as they were at `when + frac` (lag compensation).
    #[allow(clippy::too_many_arguments)]
    fn sweep_history(
        &mut self,
        a: Vec3,
        b: Vec3,
        r: f32,
        shooter: usize,
        faction: bc_proto::Faction,
        when: u32,
        frac: f32,
    ) -> Option<(f32, usize, Part)> {
        let (spatial, suits, history, ff) =
            (&mut self.spatial, &self.suits, &self.history, self.cfg.friendly_fire);
        // Suits move ≲ 150 m within the rewind window; widen the broad phase by that.
        let pad = Vec3::splat(r + 170.0);
        let mut best: Option<(f32, usize, usize)> = None;
        spatial.query_box(a.min(b) - pad, a.max(b) + pad, |j| {
            if j == shooter || (!ff && suits.faction[j] == faction) {
                return;
            }
            let Some((pos, rot)) = history.pose_lerp(when, frac, j) else { return };
            let spec = frame(suits.frame[j]);
            if !segment_near_point(a, b, pos, spec.radius + r) {
                return;
            }
            if let Some((s, cap)) = sweep_capsules(a, b, r, &spec.capsules, pos, rot, suits.gone_mask(j))
                && best.is_none_or(|(bs, _, _)| s < bs)
            {
                best = Some((s, j, cap));
            }
        });
        best.map(|(s, j, cap)| (s, j, Part::ALL[cap]))
    }

    pub(super) fn projectile_step(&mut self, t: u32) {
        let mut live = core::mem::take(&mut self.proj_bits);
        live.copy_from(&self.projectiles.alive);
        for k in live.iter() {
            if t >= self.projectiles.expire[k] {
                self.projectiles.kill(k);
                continue;
            }
            let a = self.projectiles.pos[k];
            let b = a + self.projectiles.vel[k] * DT;
            let r = self.projectiles.radius[k];
            let owner = self.projectiles.owner[k] as usize;
            let of = self.projectiles.owner_faction[k];
            let (spatial, suits, ff) = (&mut self.spatial, &self.suits, self.cfg.friendly_fire);
            let pad = Vec3::splat(r + 14.0);
            let mut best: Option<(f32, usize, usize)> = None;
            spatial.query_box(a.min(b) - pad, a.max(b) + pad, |j| {
                if j == owner || (!ff && suits.faction[j] == of) {
                    return;
                }
                let fl = &suits.flight[j];
                let spec = frame(suits.frame[j]);
                if !segment_near_point(a, b, fl.pos, spec.radius + r) {
                    return;
                }
                if let Some((s, cap)) =
                    sweep_capsules(a, b, r, &spec.capsules, fl.pos, fl.rot, suits.gone_mask(j))
                    && best.is_none_or(|(bs, _, _)| s < bs)
                {
                    best = Some((s, j, cap));
                }
            });
            // Rocks, landmarks and the colony stop shots (rocks are worked by them): whichever is
            // met first along this tick's path, a suit included. A suit skimming the hull is hit.
            let blocker = self.first_blocker(a, b, r, t, 0.0);
            let shot = |p: &crate::projectiles::Projectiles| {
                (p.kind[k], p.damage[k], normalize_or(p.vel[k], Vec3::Z))
            };
            match (best, blocker) {
                (Some((s, j, cap)), _) if blocker.is_none_or(|(t, _)| s <= t) => {
                    let (kind, dmg, dir) = shot(&self.projectiles);
                    self.queue_damage(j, Part::ALL[cap], dmg, owner, kind, dir);
                    self.projectiles.kill(k);
                }
                (_, Some((f, what))) => {
                    if let Blocker::Rock(which) = what {
                        let (kind, dmg, dir) = shot(&self.projectiles);
                        self.rock_hit(which, dmg, kind, a + (b - a) * f, dir, owner, t);
                    }
                    self.projectiles.kill(k);
                }
                _ => self.projectiles.pos[k] = b,
            }
        }
        self.proj_bits = live;
    }

    pub(super) fn damage_step(&mut self, t: u32) {
        for k in 0..self.damage.len() {
            let d = self.damage.as_slice()[k];
            let j = d.target as usize;
            if !self.suits.alive.get(j) {
                continue;
            }
            self.suits.last_hit[j] = t;
            let spec = frame(self.suits.frame[j]);
            let mut part = d.part;
            let mut amount = d.amount * spec.armor * self.suits.tuning[j].armor;
            // A beam wider than a limb engulfs the whole suit: it lands on the torso.
            if weapon(d.weapon).engulfs {
                part = Part::Torso;
            }
            // Hits on a destroyed limb carry through to the torso at half strength.
            if self.suits.part_hp[j][part as usize] <= 0.0 && part != Part::Torso {
                part = Part::Torso;
                amount *= 0.5;
            }
            let hp = &mut self.suits.part_hp[j][part as usize];
            let before = *hp;
            *hp = (*hp - amount).max(0.0);
            let mut dealt = before - *hp;
            let on_part = dealt;
            if part != Part::Torso && before > 0.0 && *hp <= 0.0 {
                self.detach(j, part, d.dir, t);
            }
            // Damage that blows a limb off spills half its excess into the torso.
            let excess = amount - dealt;
            if part != Part::Torso && excess > 0.0 {
                let torso = &mut self.suits.part_hp[j][Part::Torso as usize];
                let spill = (excess * 0.5).min(*torso);
                *torso -= spill;
                dealt += spill;
            }
            let shooter = d.shooter as usize;
            if shooter < self.suits.cap {
                self.suits.stats[shooter].hits += 1;
                self.suits.stats[shooter].hits_by_class[weapon(d.weapon).class as usize] += 1;
                self.suits.stats[shooter].damage_dealt += dealt;
            }
            self.events.push(Event::Hit {
                id: 0,
                tick: t,
                target: j as u16,
                part,
                shooter: d.shooter,
                weapon: d.weapon,
                damage: (amount / spec.part_hp[part as usize]).clamp(0.0, 1.0),
            });
            // Through thinned armour, a blow can reach what's inside the part.
            if self.suits.part_hp[j][Part::Torso as usize] > 0.0 {
                self.critical(j, part, on_part, k, t);
            }
            if self.suits.part_hp[j][Part::Torso as usize] <= 0.0 {
                self.suits.alive.set(j, false);
                self.suits.stats[j].deaths += 1;
                if shooter < self.suits.cap && shooter != j {
                    self.suits.stats[shooter].kills += 1;
                    // Survival: the colony pays for every Mobile Doll a pilot brings down.
                    if self.cfg.survival
                        && self.suits.pilot[j] == PilotKind::MobileDoll
                        && self.suits.pilot[shooter] != PilotKind::MobileDoll
                    {
                        let c = &mut self.suits.credits[shooter];
                        *c = c.saturating_add(bounty(self.suits.frame[j]));
                    }
                }
                // What it spills goes off the body it stood on (the wreck is off it).
                let up = self.spill_up(j);
                let hulk = self.wreck(j, t);
                self.spill_over(j, t, true, up);
                self.events.push(Event::Kill { id: 0, tick: t, victim: j as u16, killer: d.shooter, hulk });
                let wait = if self.suits.pilot[j] == PilotKind::MobileDoll {
                    secs(3.0)
                } else {
                    secs(self.cfg.respawn_secs)
                };
                self.suits.respawn_at[j] = t + wait.max(1);
                self.suits.zero[j] = Default::default();
                if self.suits.sleeping.get(j) {
                    self.note_fate(j, super::Gone::Destroyed { killer: d.shooter }, t);
                }
            }
        }
    }

    /// Whether the blow `on_part` (armour points) that the `k`th hit of this tick dealt to suit
    /// `j`'s `part` got through to one of the part's systems, and if so what it did.
    fn critical(&mut self, j: usize, part: Part, on_part: f32, k: usize, t: u32) {
        let max = frame(self.suits.frame[j]).part_hp[part as usize];
        let left = self.suits.part_hp[j][part as usize];
        if left <= 0.0 || max <= 0.0 || on_part <= 0.0 {
            return;
        }
        let doll = self.suits.pilot[j] == PilotKind::MobileDoll;
        let blow = on_part / max;
        let salt = (j as u32) << 11 | k as u32;
        if hash01(t ^ 0xC417, salt) >= systems::crit_chance(blow, left / max, doll) {
            return;
        }
        // Which of the part's working systems it finds (a Mobile Doll has no pilot to hurt).
        let now = self.suits.systems[j];
        let open = |s: &System| now.get(*s) < FAILED && !(doll && *s == System::Cockpit);
        let total: u32 = System::of_part(part).filter(open).map(System::weight).sum();
        if total == 0 {
            return;
        }
        let mut pick = ((hash01(t ^ 0x7E1D, salt) * total as f32) as u32).min(total - 1);
        let Some(sys) = System::of_part(part).filter(open).find(|s| {
            let w = s.weight();
            if pick < w {
                true
            } else {
                pick -= w;
                false
            }
        }) else {
            return;
        };
        let level = (now.get(sys) + if blow >= systems::CRIT_DOUBLE { 2 } else { 1 }).min(FAILED);
        self.suits.systems[j].set(sys, level);
        let st = &mut self.suits.status[j];
        match sys {
            System::Reactor => st.scram = systems::SCRAM_TICKS,
            System::Cockpit => st.concussed = systems::CONCUSSION_TICKS,
            System::ActuatorL | System::ActuatorR => self.jam(j, sys == System::ActuatorR),
            _ => {}
        }
        self.events.push(Event::SystemHit { id: 0, tick: t, target: j as u16, system: sys as u8, level });
    }

    /// The weapons on an arm whose actuators were struck jam for a while.
    fn jam(&mut self, j: usize, right: bool) {
        let spec = frame(self.suits.frame[j]);
        for (slot, mount) in spec.loadout.iter().enumerate() {
            let Some(m) = mount else { continue };
            let on = match m.arm {
                ArmSlot::Left => !right,
                ArmSlot::Right | ArmSlot::Nose => right,
                ArmSlot::Both => true,
                _ => false,
            };
            if on {
                let ws = &mut self.suits.weapons[j][slot];
                ws.cooldown = ws.cooldown.max(systems::JAM_TICKS);
            }
        }
    }

    /// A seed for a chunk's look, the same on every machine.
    fn chunk_seed(&self, j: usize, salt: u32, t: u32) -> u8 {
        (hash01(t, j as u32 * 8 + salt) * 255.0) as u8
    }

    /// Part `part` of suit `j`, destroyed by a blow along `dir`, comes off as a drifting limb.
    fn detach(&mut self, j: usize, part: Part, dir: Vec3, t: u32) {
        let fid = self.suits.frame[j];
        let f = self.suits.flight[j];
        let c = frame(fid).capsules[part as usize];
        let local = (c.a + c.b) * 0.5;
        let out = normalize_or(f.rot * local, dir);
        let spin_axis = normalize_or(
            Vec3::new(
                hash01(t, j as u32) - 0.5,
                hash01(t ^ 0x5A, j as u32) - 0.5,
                hash01(t ^ 0xA5, j as u32) - 0.5,
            ),
            Vec3::X,
        );
        let seg = Segment {
            t0: t,
            pos: f.pos + f.rot * local,
            vel: f.vel + out * DETACH_SPEED + dir * DETACH_PUSH,
            rot: f.rot,
            spin: spin_axis * 1.5,
        }
        .quantized();
        let desc = ChunkDesc {
            kind: ChunkKind::Limb { frame: fid, faction: self.suits.faction[j], part },
            seed: self.chunk_seed(j, part as u32, t),
            mass_kg: part_mass_kg(fid, part),
        };
        let doll = self.suits.pilot[j] == PilotKind::MobileDoll;
        let chunk = self.chunks.spawn(desc, Motion::Free(seg), t + wreck_ttl(doll), t).unwrap_or(NO_CHUNK);
        self.events.push(Event::Detach { id: 0, tick: t, source: j as u16, from_hulk: false, part, chunk });
    }

    /// Suit `j` was destroyed: what's left of it drifts on as a hulk. Returns its chunk id.
    fn wreck(&mut self, j: usize, t: u32) -> u16 {
        let fid = self.suits.frame[j];
        let f = self.suits.flight[j];
        // Off whatever it stood on: the hulk drifts on the world velocity it had there.
        self.suits.footing[j] = Footing::Free;
        self.suits.anchor[j] = crate::ground::Anchor::default();
        let gone = self.suits.gone_mask(j);
        let lim = Vec3::splat(bc_proto::objects::SPIN_MAX);
        let seg = Segment { t0: t, pos: f.pos, vel: f.vel, rot: f.rot, spin: f.ang_vel.clamp(-lim, lim) }
            .quantized();
        let desc = ChunkDesc {
            kind: ChunkKind::Hulk {
                frame: fid,
                faction: self.suits.faction[j],
                parts: !gone & ((1 << Part::COUNT) - 1),
            },
            seed: self.chunk_seed(j, 7, t),
            mass_kg: mass_without(fid, gone),
        };
        let doll = self.suits.pilot[j] == PilotKind::MobileDoll;
        let Some(hulk) = self.chunks.spawn(desc, Motion::Free(seg), t + wreck_ttl(doll), t) else {
            return NO_CHUNK;
        };
        self.suits.hulk[j] = (hulk, self.chunks.generation[hulk as usize]);
        // From now on the wreck is where its hulk is (on the wire's grid).
        let f = &mut self.suits.flight[j];
        (f.pos, f.rot, f.vel) = (seg.pos, seg.rot, seg.vel);
        hulk
    }
}
