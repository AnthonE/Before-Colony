//! Missile locks, launchers and homing missiles in flight.
//!
//! - **Locks.** A frame with a missile launcher builds a lock on its designation while it holds it
//!   inside the launcher's lock cone and range; losing it counts the lock down twice as fast.
//! - **Salvos.** A trigger pull fires the launcher's salvo, missiles a few ticks apart, each
//!   leaving along the aim with a little spread. With the lock acquired they're guided onto the
//!   locked suit; otherwise they fly blind.
//! - **Guidance.** Proportional navigation: the motor turns the missile against the line of
//!   sight's rotation (`a = N·Vc·(Ω × r̂)`, capped at the motor's acceleration) and spends what's
//!   left of its thrust closing in. Its Δv is a budget: spent, the missile coasts and can't steer.
//!   The seeker checks every third tick that it still sees its target (a jamming one it loses).
//! - **Ends.** A proximity fuse against enemy suits, rocks and the colony, or its life running out:
//!   each is a `MissileBurst` event. There's no lag compensation: missiles fly in the present.

use bc_proto::events::{BurstCause, Event};
use bc_proto::{InputCmd, MissileState, NO_SLOT, Part};
use glam::Vec3;

use super::Sim;
use super::combat::clamp_to_cone;
use crate::collide::{segment_near_point, sweep_capsules};
use crate::config::DT;
use crate::content::{MissileSpec, Mount, SpecialKind, WeaponSpec, frame, weapon};
use crate::math::{angle_between, hash01, length, normalize_or, sqrt};
use crate::sensors;
use crate::suits::WeaponState;
use crate::world::inside_colony;

/// A missile's body, for direct hits and rocks (m).
const BODY_RADIUS: f32 = 0.5;

impl Sim {
    /// Builds, holds or loses each launcher-carrying suit's lock on its designation.
    pub(super) fn lock_step(&mut self) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let Some(spec) = frame(self.suits.frame[i]).lock_spec() else { continue };
            let (me, aim) = (self.suits.flight[i].pos, self.suits.aim[i]);
            let held = self.designation(i).filter(|&j| {
                let to = self.suits.flight[j].pos - me;
                length(to) <= spec.lock_range && angle_between(aim, to) <= spec.lock_cone
            });
            let lock = &mut self.suits.lock[i];
            match held {
                Some(j) if lock.target == j as u16 => {
                    lock.progress = (lock.progress + 1).min(spec.lock_ticks)
                }
                Some(j) if lock.progress == 0 => {
                    lock.target = j as u16;
                    lock.progress = 1;
                }
                _ => {
                    lock.progress = lock.progress.saturating_sub(2);
                    if lock.progress == 0 {
                        lock.target = NO_SLOT;
                    }
                }
            }
        }
        self.iter_bits = alive;
    }

    /// The suit `i` has a missile lock on, once acquired.
    pub fn missile_lock(&self, i: usize) -> Option<usize> {
        let spec = frame(self.suits.frame[i]).lock_spec()?;
        let lock = &self.suits.lock[i];
        let j = usize::from(lock.target);
        (lock.progress >= spec.lock_ticks && self.suits.is_alive(j)).then_some(j)
    }

    /// A launcher's trigger pull: its salvo starts now, and it's cooling down for the next.
    pub(super) fn start_salvo(&mut self, i: usize, ws: &mut WeaponState, w: &WeaponSpec) {
        ws.salvo = w.salvo.max(1);
        ws.gap = 0;
        ws.cooldown = w.cooldown;
        self.suits.heat[i] += w.heat;
        self.suits.energy[i] -= w.energy;
    }

    /// Launches the salvo's next missile when it's due.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn salvo_tick(
        &mut self,
        i: usize,
        slot: usize,
        ws: &mut WeaponState,
        mount: Mount,
        w: &WeaponSpec,
        cmd: &InputCmd,
        t: u32,
    ) {
        if ws.salvo == 0 {
            return;
        }
        if ws.gap > 0 {
            ws.gap -= 1;
            return;
        }
        let Some(spec) = w.missile else { return };
        if ws.ammo == 0 || !self.suits.arm_free(i, mount.arm) {
            ws.salvo = 0;
            return;
        }
        let f = self.suits.flight[i];
        let fwd = f.rot * Vec3::Z;
        let aim = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, mount.arm.cone());
        // Each missile of a salvo leaves a little off the aim, its own way.
        let n = u32::from(w.salvo.max(1) - ws.salvo);
        let seed = (i as u32) * 131 + (slot as u32) * 17 + n;
        let off =
            Vec3::new(hash01(t, seed) - 0.5, hash01(t ^ 0x5A, seed) - 0.5, hash01(t ^ 0xA5, seed) - 0.5);
        let dir = normalize_or(aim + off * (2.0 * w.spread), aim);
        let target = self.missile_lock(i).map_or((NO_SLOT, 0), |j| (j as u16, self.suits.generation[j]));
        let s = &mut self.suits;
        let launched = self.missiles.spawn(
            w.kind,
            i as u16,
            s.faction[i],
            target,
            f.pos + f.rot * mount.arm.muzzle(),
            f.vel + dir * spec.launch_speed,
            spec.dv,
            t + u32::from(spec.life),
        );
        ws.salvo -= 1;
        ws.gap = w.salvo_gap.saturating_sub(1);
        // A full pool swallows the launch (the round isn't spent).
        if launched.is_none() {
            return;
        }
        ws.ammo -= 1;
        s.stats[i].shots += 1;
        s.last_fired[i] = t;
        if slot == 0 {
            s.fired_primary[i] = t;
        } else if slot == 1 {
            s.fired_secondary[i] = t;
        }
        self.break_jammer(i, t);
    }

    /// Whether missile `k`'s seeker still sees its target: alive (the same suit), within the
    /// seeker's cone and its range scaled by the target's signature (a jammer's too).
    fn seeker_holds(&self, k: usize, spec: &MissileSpec) -> bool {
        let (m, s) = (&self.missiles, &self.suits);
        let j = usize::from(m.target[k]);
        if j >= s.cap || !s.alive.get(j) || s.generation[j] != m.target_gen[k] {
            return false;
        }
        let to = s.flight[j].pos - m.pos[k];
        let mut sig = sensors::signature(
            frame(s.frame[j]).signature,
            s.boosting[j],
            self.tick().saturating_sub(s.last_fired[j]) < 30,
            false,
        );
        if s.faction[j] != m.owner_faction[k]
            && let Some((jam, _)) = self.jamming(j)
        {
            sig *= jam;
        }
        let range = spec.seeker_range * sig;
        to.length_squared() <= range * range
            && angle_between(normalize_or(m.vel[k], to), to) <= spec.seeker_cone
    }

    /// Flies every missile a tick: seeker, guidance, motor, then what it meets.
    pub(super) fn missile_step(&mut self, t: u32) {
        for n in self.suits.incoming.iter_mut() {
            *n = 0;
        }
        let mut live = core::mem::take(&mut self.missile_bits);
        live.copy_from(&self.missiles.alive);
        for k in live.iter() {
            let w = weapon(self.missiles.kind[k]);
            let Some(spec) = w.missile else {
                self.missiles.kill(k);
                continue;
            };
            let pos = self.missiles.pos[k];
            if t >= self.missiles.expire[k] {
                self.burst(k, pos, BurstCause::Expired, t);
                continue;
            }
            // The seeker looks every third tick (staggered across the pool).
            if self.missiles.guided(k) && (t + k as u32).is_multiple_of(3) && !self.seeker_holds(k, &spec) {
                self.missiles.target[k] = NO_SLOT;
            }
            let m = &self.missiles;
            let vel = m.vel[k];
            let target = m.guided(k).then(|| usize::from(m.target[k]));
            if let Some(j) = target {
                self.suits.incoming[j] += 1;
            }
            // The motor, while it has Δv: steer by proportional navigation, close with the rest.
            let mut acc = Vec3::ZERO;
            if self.missiles.dv_left[k] > 0.0 {
                let heading = normalize_or(vel, Vec3::Z);
                acc = match target {
                    Some(j) => {
                        let tf = &self.suits.flight[j];
                        let r = tf.pos - pos;
                        let v = tf.vel - vel;
                        let los = normalize_or(r, heading);
                        let omega = r.cross(v) / r.length_squared().max(1.0);
                        let closing = -los.dot(v);
                        let steer =
                            (omega.cross(los) * (spec.nav * closing.max(0.0))).clamp_length_max(spec.accel);
                        let push = sqrt((spec.accel * spec.accel - steer.length_squared()).max(0.0));
                        steer + los * push
                    }
                    None => heading * spec.accel,
                };
                self.missiles.dv_left[k] -= spec.accel * DT;
            }
            let vel = vel + acc * DT;
            let next = pos + vel * DT;
            self.missiles.vel[k] = vel;
            let owner = usize::from(self.missiles.owner[k]);
            let of = self.missiles.owner_faction[k];
            if inside_colony(next) {
                self.burst(k, pos, BurstCause::Blocked, t);
                continue;
            }
            // The first thing along this tick's path: an enemy suit within the fuse, or a rock.
            let (spatial, suits, ff) = (&mut self.spatial, &self.suits, self.cfg.friendly_fire);
            let pad = Vec3::splat(spec.fuse + 14.0);
            let mut best: Option<(f32, usize, usize)> = None;
            spatial.query_box(pos.min(next) - pad, pos.max(next) + pad, |j| {
                if j == owner || (!ff && suits.faction[j] == of) {
                    return;
                }
                let fl = &suits.flight[j];
                let fspec = frame(suits.frame[j]);
                if !segment_near_point(pos, next, fl.pos, fspec.radius + spec.fuse) {
                    return;
                }
                if let Some((s, cap)) =
                    sweep_capsules(pos, next, spec.fuse, &fspec.capsules, fl.pos, fl.rot, suits.gone_mask(j))
                    && best.is_none_or(|(bs, ..)| s < bs)
                {
                    best = Some((s, j, cap));
                }
            });
            let rock = self.field.sweep(pos, next, BODY_RADIUS);
            let dir = normalize_or(vel, Vec3::Z);
            match (best, rock) {
                (Some((s, j, cap)), _) if rock.is_none_or(|(r, _)| s <= r) => {
                    let at = pos + (next - pos) * s;
                    self.queue_damage(j, Part::ALL[cap], w.damage, owner, w.kind, dir);
                    let cause = if target == Some(j) { BurstCause::Hit } else { BurstCause::Proximity };
                    self.burst(k, at, cause, t);
                }
                (_, Some((s, which))) => {
                    let at = pos + (next - pos) * s;
                    self.rock_hit(which, w.damage, w.kind, at, dir, owner, t);
                    self.burst(k, at, BurstCause::Blocked, t);
                }
                _ => self.missiles.pos[k] = next,
            }
        }
        self.missile_bits = live;
        self.peak_missiles = self.peak_missiles.max(self.missiles.count());
    }

    /// Missile `k` bursts at `at`.
    fn burst(&mut self, k: usize, at: Vec3, cause: BurstCause, t: u32) {
        self.events.push(Event::MissileBurst { id: 0, tick: t, missile: k as u16, pos: at, cause });
        self.missiles.kill(k);
    }

    /// Missile `k` as replicated to `viewer`.
    pub fn missile_state(&self, k: usize, viewer: usize) -> MissileState {
        let m = &self.missiles;
        MissileState {
            id: k as u16,
            generation: m.generation[k] & 3,
            kind: m.kind[k],
            guided: m.guided(k),
            targets_you: m.target[k] == viewer as u16,
            friendly: m.owner_faction[k] == self.suits.faction[viewer],
            pos: m.pos[k],
            vel: m.vel[k],
        }
    }

    /// Whether suit `i`'s Full Open Attack is under way.
    pub fn full_open(&self, i: usize) -> bool {
        matches!(frame(self.suits.frame[i]).special, SpecialKind::FullOpen { .. })
            && self.suits.special[i].active
    }
}
