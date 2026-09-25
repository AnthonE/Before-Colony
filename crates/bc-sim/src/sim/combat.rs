//! Weapons, projectiles, beam sabers and damage.

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE};
use bc_proto::events::Event;
use bc_proto::{InputCmd, NO_CHUNK, Part, PilotKind, WeaponKind};
use glam::Vec3;

use super::{DamageEvent, Sim};
use crate::collide::{capsule_world, segment_near_point, segment_segment, sweep_capsules};
use crate::config::{DT, MAX_REWIND_TICKS, secs};
use crate::content::{Mount, WeaponSpec, frame, weapon};
use crate::math::{angle_between, cos, hash01, normalize_or, sin};
use crate::suits::{SaberPhase, WeaponState};
use crate::world::inside_colony;

/// ZERO fire-time magnetism: shots this close to the ZERO firing solution snap to it.
pub const MAGNET_ANGLE: f32 = 0.026; // 1.5°

/// Beams at least this wide engulf a suit and always hit the torso, m.
const WIDE_BEAM_RADIUS: f32 = 3.0;

/// Clamps `dir` into a cone of half-angle `cone` around `axis`.
fn clamp_to_cone(dir: Vec3, axis: Vec3, cone: f32) -> Vec3 {
    let a = angle_between(axis, dir);
    if a <= cone {
        return dir;
    }
    let perp = normalize_or(dir - axis * axis.dot(dir), Vec3::Y);
    normalize_or(axis * cos(cone) + perp * sin(cone), axis)
}

impl Sim {
    pub(super) fn queue_damage(
        &mut self,
        target: usize,
        part: Part,
        amount: f32,
        shooter: usize,
        weapon: WeaponKind,
    ) {
        let _ = self.damage.try_push(DamageEvent {
            target: target as u16,
            part,
            amount,
            shooter: shooter as u16,
            weapon,
        });
    }

    pub(super) fn weapons_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let spec = frame(self.suits.frame[i]);
            let cmd = self.suits.input[i];
            for slot in 0..2 {
                let Some(mount) = spec.loadout[slot] else { continue };
                let w = weapon(mount.weapon);
                let button = if slot == 0 { FIRE_PRIMARY } else { FIRE_SECONDARY };
                let mut ws: WeaponState = self.suits.weapons[i][slot];
                ws.cooldown = ws.cooldown.saturating_sub(1);
                let arm_ok = self.suits.part_hp[i][mount.arm.part() as usize] > 0.0;
                let ready = ws.cooldown == 0
                    && !self.suits.overheated[i]
                    && self.suits.energy[i] >= w.energy
                    && (w.ammo == 0 || ws.ammo > 0)
                    && arm_ok;
                let wants = cmd.pressed(button);
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
                self.suits.weapons[i][slot] = ws;
                if fire {
                    self.fire(i, slot, mount, w, &cmd, t);
                }
            }
        }
        self.iter_bits = alive;
    }

    fn fire(&mut self, i: usize, slot: usize, mount: Mount, w: &WeaponSpec, cmd: &InputCmd, t: u32) {
        let f = self.suits.flight[i];
        let fwd = f.rot * Vec3::Z;
        let mut dir = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, mount.arm.cone());
        let z = &self.suits.zero[i];
        if z.active()
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
        let muzzle = f.pos + f.rot * mount.arm.muzzle();
        let vel = f.vel + dir * w.speed;

        let s = &mut self.suits;
        let ws = &mut s.weapons[i][slot];
        ws.cooldown = w.cooldown;
        if w.ammo > 0 {
            ws.ammo -= 1;
        }
        s.heat[i] += w.heat;
        s.energy[i] -= w.energy;
        s.last_fired[i] = t;
        s.stats[i].shots += 1;
        if slot == 0 {
            s.fired_primary[i] = t;
        } else {
            s.fired_secondary[i] = t;
        }

        // Lag compensation for remote pilots: fly the shot through the world as they saw it.
        let rewind = if s.pilot[i] == PilotKind::MobileDoll {
            0
        } else {
            t.saturating_sub(cmd.view_tick_q4 >> 4).min(MAX_REWIND_TICKS)
        };
        let frac = if rewind > 0 { (cmd.view_tick_q4 & 15) as f32 / 16.0 } else { 0.0 };
        let spawn_tick = t - rewind;
        let faction = s.faction[i];
        let mut p = muzzle;
        let mut hit = None;
        let mut blocked = false;
        for k in 0..rewind {
            let b = p + vel * DT;
            let rock = self.field.sweep(p, b, w.radius).map(|(t, _)| t);
            match self.sweep_history(p, b, w.radius, i, faction, spawn_tick + k, frac) {
                Some((s, j, part)) if rock.is_none_or(|t| s <= t) => {
                    hit = Some((j, part));
                    break;
                }
                _ if rock.is_some() => {
                    blocked = true;
                    break;
                }
                _ => {}
            }
            p = b;
        }
        if w.kind.is_beam() {
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
        match hit {
            Some((target, part)) => self.queue_damage(target, part, w.damage, i, w.kind),
            None if !blocked => {
                let ttl = w.ttl_ticks().saturating_sub(rewind);
                if ttl > 0 {
                    self.projectiles.spawn(w.kind, i as u16, faction, p, vel, t + ttl, w.damage, w.radius);
                }
            }
            None => {} // a rock took it
        }
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
            if let Some((s, cap)) = sweep_capsules(a, b, r, &spec.capsules, pos, rot)
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
            if inside_colony(b) {
                self.projectiles.kill(k);
                continue;
            }
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
                if let Some((s, cap)) = sweep_capsules(a, b, r, &spec.capsules, fl.pos, fl.rot)
                    && best.is_none_or(|(bs, _, _)| s < bs)
                {
                    best = Some((s, j, cap));
                }
            });
            // Rocks stop shots: whichever is met first along this tick's path.
            let rock = self.field.sweep(a, b, r).map(|(t, _)| t);
            match best {
                Some((s, j, cap)) if rock.is_none_or(|t| s <= t) => {
                    let kind = self.projectiles.kind[k];
                    let dmg = self.projectiles.damage[k];
                    self.queue_damage(j, Part::ALL[cap], dmg, owner, kind);
                    self.projectiles.kill(k);
                }
                _ if rock.is_some() => self.projectiles.kill(k),
                _ => self.projectiles.pos[k] = b,
            }
        }
        self.proj_bits = live;
    }

    pub(super) fn melee_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let spec = frame(self.suits.frame[i]);
            let Some(mount) = spec.loadout[2] else { continue };
            let w = weapon(mount.weapon);
            let s = &mut self.suits;
            s.weapons[i][2].cooldown = s.weapons[i][2].cooldown.saturating_sub(1);
            let mut st = s.saber[i];
            let cmd = s.input[i];
            let pressed = cmd.pressed(MELEE) && s.prev_buttons[i] & MELEE == 0;
            match st.phase {
                SaberPhase::Idle => {
                    let arm_ok = s.part_hp[i][mount.arm.part() as usize] > 0.0;
                    if pressed
                        && arm_ok
                        && s.weapons[i][2].cooldown == 0
                        && s.energy[i] >= w.energy
                        && !s.overheated[i]
                    {
                        st.phase = SaberPhase::Windup;
                        st.timer = 4;
                        st.n_hits = 0;
                        st.view_q4 = cmd.view_tick_q4;
                        s.heat[i] += w.heat;
                        s.energy[i] -= w.energy;
                        s.weapons[i][2].cooldown = w.cooldown + 18;
                        s.last_fired[i] = t;
                    }
                }
                SaberPhase::Windup => {
                    st.timer -= 1;
                    if st.timer == 0 {
                        st.phase = SaberPhase::Active;
                        st.timer = 6;
                    }
                }
                SaberPhase::Active => {
                    s.saber[i] = st;
                    self.saber_sweep(i, mount, w, t);
                    st = self.suits.saber[i];
                    if st.phase == SaberPhase::Active {
                        st.timer -= 1;
                        if st.timer == 0 {
                            st.phase = SaberPhase::Recovery;
                            st.timer = 8;
                        }
                    }
                }
                SaberPhase::Recovery => {
                    st.timer -= 1;
                    if st.timer == 0 {
                        st.phase = SaberPhase::Idle;
                    }
                }
            }
            self.suits.saber[i] = st;
        }
        self.iter_bits = alive;
    }

    /// Sweeps the blade arc (3 sub-steps this tick) against nearby suits.
    fn saber_sweep(&mut self, i: usize, mount: Mount, w: &WeaponSpec, t: u32) {
        let f = self.suits.flight[i];
        let st = self.suits.saber[i];
        let hand = f.pos + f.rot * mount.arm.muzzle();
        let rewind = if self.suits.pilot[i] == PilotKind::MobileDoll {
            0
        } else {
            t.saturating_sub(st.view_q4 >> 4).min(MAX_REWIND_TICKS)
        };
        let when = t - rewind;
        let faction = self.suits.faction[i];
        for sub in 0..3u32 {
            let progress = ((6 - u32::from(st.timer)) * 3 + sub) as f32 / 18.0;
            let local = normalize_or(
                Vec3::new(0.75, 0.65, 0.35).lerp(Vec3::new(-0.75, -0.45, 0.55), progress),
                Vec3::Z,
            );
            let tip = hand + f.rot * local * w.range;
            let mut found: Option<(usize, usize, f32)> = None;
            let (spatial, suits, history, ff) =
                (&mut self.spatial, &self.suits, &self.history, self.cfg.friendly_fire);
            spatial.query_sphere(hand, w.range + 16.0, |j| {
                if j == i || (!ff && suits.faction[j] == faction) {
                    return;
                }
                let already = suits.saber[i].hits[..suits.saber[i].n_hits as usize].contains(&(j as u16));
                if already {
                    return;
                }
                let (pos, rot) = if rewind > 0 {
                    match history.pose(when, j) {
                        Some(p) => p,
                        None => return,
                    }
                } else {
                    (suits.flight[j].pos, suits.flight[j].rot)
                };
                let spec = frame(suits.frame[j]);
                for (ci, c) in spec.capsules.iter().enumerate() {
                    let (ca, cb, cr) = capsule_world(c, pos, rot);
                    let (_, _, d2) = segment_segment(hand, tip, ca, cb);
                    let rr = w.radius + cr;
                    if d2 <= rr * rr && found.is_none_or(|(_, _, bd)| d2 < bd) {
                        found = Some((j, ci, d2));
                    }
                }
            });
            let Some((j, ci, _)) = found else { continue };
            // Saber clash: both blades live and the target faces us: parried, no damage.
            let tj = self.suits.saber[j];
            let facing = (self.suits.flight[j].rot * Vec3::Z)
                .dot(normalize_or(f.pos - self.suits.flight[j].pos, Vec3::Z))
                > 0.5;
            if tj.phase == SaberPhase::Active && facing {
                for k in [i, j] {
                    self.suits.saber[k].phase = SaberPhase::Recovery;
                    self.suits.saber[k].timer = 10;
                }
                self.events.push(Event::Clash { id: 0, tick: t, a: i as u16, b: j as u16 });
                return;
            }
            let st = &mut self.suits.saber[i];
            if (st.n_hits as usize) < st.hits.len() {
                st.hits[st.n_hits as usize] = j as u16;
                st.n_hits += 1;
            }
            self.queue_damage(j, Part::ALL[ci], w.damage, i, WeaponKind::BeamSaber);
        }
    }

    pub(super) fn damage_step(&mut self, t: u32) {
        for k in 0..self.damage.len() {
            let d = self.damage.as_slice()[k];
            let j = d.target as usize;
            if !self.suits.alive.get(j) {
                continue;
            }
            let spec = frame(self.suits.frame[j]);
            let mut part = d.part;
            let mut amount = d.amount * spec.armor;
            // A beam wider than a limb engulfs the whole suit: it lands on the torso.
            if weapon(d.weapon).radius >= WIDE_BEAM_RADIUS {
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
            if self.suits.part_hp[j][Part::Torso as usize] <= 0.0 {
                self.suits.alive.set(j, false);
                self.suits.stats[j].deaths += 1;
                if shooter < self.suits.cap && shooter != j {
                    self.suits.stats[shooter].kills += 1;
                }
                self.events.push(Event::Kill {
                    id: 0,
                    tick: t,
                    victim: j as u16,
                    killer: d.shooter,
                    hulk: NO_CHUNK,
                });
                let wait = if self.suits.pilot[j] == PilotKind::MobileDoll {
                    secs(3.0)
                } else {
                    secs(self.cfg.respawn_secs)
                };
                self.suits.respawn_at[j] = t + wait.max(1);
                self.suits.zero[j] = Default::default();
            }
        }
    }
}
