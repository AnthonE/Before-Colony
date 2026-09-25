//! Melee: beam sabers and every other blade, the Dragon Fang's thrust, and the Cross Crusher.
//!
//! A strike winds up, is out (and can hit) for a few ticks, then recovers. Its motion comes from
//! the weapon's [`MeleeSpec`]: a swing sweeps its blade through an arc, `sub_steps` samples a tick;
//! a thrust drives its head straight out along the aim; a twin weapon strikes with a second,
//! mirrored blade from the other hand.
//!
//! What starts a strike, in priority order: the SPECIAL press for a frame whose special is a melee
//! move (the Cross Crusher), the MELEE press for the melee slot, then fire held on a melee weapon in
//! a gun slot (the Dragon Fang is Shenlong's primary).

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE, SPECIAL};
use bc_proto::events::Event;
use bc_proto::{Part, PilotKind};
use glam::Vec3;

use super::Sim;
use super::combat::clamp_to_cone;
use crate::collide::{capsule_world, segment_segment};
use crate::config::MAX_REWIND_TICKS;
use crate::content::salvage::SABER_DIG;
use crate::content::{ArmSlot, MeleeSpec, Mount, SpecialKind, Stroke, WeaponClass, frame, weapon};
use crate::math::normalize_or;
use crate::suits::{MeleePhase, MeleeState, SECOND_BLADE, SPECIAL_MOUNT};

/// A suit-frame direction mirrored left-right: the second blade of a twin weapon.
fn mirror(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, v.z)
}

impl Sim {
    /// The mount a strike from `slot` (a loadout slot, or [`SPECIAL_MOUNT`]) would use, if it holds
    /// a melee weapon.
    fn melee_mount(&self, i: usize, slot: u8) -> Option<Mount> {
        frame(self.suits.frame[i]).melee_mount(slot)
    }

    /// Whether suit `i` has the arms to strike from `mount`: a twin weapon strikes with the blades
    /// it has an arm for, unless it needs both.
    fn melee_arms_ok(&self, i: usize, mount: Mount, m: &MeleeSpec) -> bool {
        if m.twin && !m.both_arms {
            self.suits.arm_free(i, ArmSlot::Right) || self.suits.arm_free(i, ArmSlot::Left)
        } else {
            self.suits.arm_free(i, mount.arm)
        }
    }

    /// Whether `arm` is out with a strike that takes the arm with it (the Dragon Fang), so the
    /// arm's other weapons wait.
    pub(super) fn arm_blocked(&self, i: usize, arm: ArmSlot) -> bool {
        let st = &self.suits.melee[i];
        st.phase != MeleePhase::Idle
            && weapon(st.weapon).blocks_arm
            && self.melee_mount(i, st.slot).is_some_and(|m| m.arm.part() == arm.part())
    }

    /// Whether a strike from `slot` could start now: arms, cooldown, energy and heat (a strike
    /// already under way aside).
    pub(super) fn melee_ready(&self, i: usize, slot: u8) -> bool {
        let Some(mount) = self.melee_mount(i, slot) else { return false };
        let w = weapon(mount.weapon);
        let Some(m) = w.melee else { return false };
        let s = &self.suits;
        let cooldown =
            if slot == SPECIAL_MOUNT { s.special[i].cooldown } else { s.weapons[i][slot as usize].cooldown };
        self.melee_arms_ok(i, mount, &m) && cooldown == 0 && s.energy[i] >= w.energy && !s.overheated[i]
    }

    pub(super) fn melee_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let spec = frame(self.suits.frame[i]);
            // Blades' cooldowns run here, guns' in `weapons_step`.
            for (slot, mount) in spec.loadout.iter().enumerate() {
                if mount.is_some_and(|m| weapon(m.weapon).class == WeaponClass::Melee) {
                    let ws = &mut self.suits.weapons[i][slot];
                    ws.cooldown = ws.cooldown.saturating_sub(1);
                }
            }
            match self.suits.melee[i].phase {
                MeleePhase::Idle => {
                    let (cmd, prev) = (self.suits.input[i], self.suits.prev_buttons[i]);
                    let edge = |b: u16| cmd.pressed(b) && prev & b == 0;
                    let wants = [
                        (edge(SPECIAL), SPECIAL_MOUNT),
                        (edge(MELEE), 2),
                        (cmd.pressed(FIRE_PRIMARY), 0),
                        (cmd.pressed(FIRE_SECONDARY), 1),
                    ];
                    if let Some(&(_, slot)) =
                        wants.iter().find(|&&(want, slot)| want && self.melee_ready(i, slot))
                    {
                        self.start_strike(i, slot, t);
                    }
                }
                phase => {
                    let st = &mut self.suits.melee[i];
                    let Some(m) = weapon(st.weapon).melee else {
                        st.phase = MeleePhase::Idle;
                        continue;
                    };
                    if phase == MeleePhase::Active {
                        self.melee_sweep(i, t);
                    }
                    let st = &mut self.suits.melee[i];
                    // (A clash may have just sent an active strike into recovery.)
                    if st.phase == phase {
                        st.timer -= 1;
                        if st.timer == 0 {
                            (st.phase, st.timer) = match phase {
                                MeleePhase::Windup => (MeleePhase::Active, m.active),
                                MeleePhase::Active => (MeleePhase::Recovery, m.recovery),
                                _ => (MeleePhase::Idle, 0),
                            };
                        }
                    }
                }
            }
        }
        self.iter_bits = alive;
    }

    /// Starts a strike from `slot` (checked ready).
    fn start_strike(&mut self, i: usize, slot: u8, t: u32) {
        let Some(mount) = self.melee_mount(i, slot) else { return };
        let w = weapon(mount.weapon);
        let Some(m) = w.melee else { return };
        let s = &mut self.suits;
        let cmd = s.input[i];
        // A thrust goes where the pilot aims, as far off the nose as its mount turns.
        let dir = match m.stroke {
            Stroke::Swing => Vec3::Z,
            Stroke::Thrust => {
                let rot = s.flight[i].rot;
                let fwd = rot * Vec3::Z;
                rot.inverse() * clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, mount.arm.cone())
            }
        };
        s.melee[i] = MeleeState {
            phase: MeleePhase::Windup,
            timer: m.windup,
            weapon: w.kind,
            slot,
            dir,
            view_q4: cmd.view_tick_q4,
            ..MeleeState::default()
        };
        s.heat[i] += w.heat;
        s.energy[i] -= w.energy;
        if slot == SPECIAL_MOUNT {
            if let SpecialKind::MeleeMove { cooldown } = frame(s.frame[i]).special {
                s.special[i].cooldown = cooldown;
            }
            s.stats[i].specials += 1;
        } else {
            s.weapons[i][slot as usize].cooldown = w.cooldown + m.duration();
        }
        s.last_fired[i] = t;
    }

    /// Sweeps the strike's blades through this tick (`sub_steps` samples) against rocks, hulks and
    /// suits. Each blade hits a suit at most once a strike; a rock and a hulk are struck once.
    fn melee_sweep(&mut self, i: usize, t: u32) {
        let st = self.suits.melee[i];
        let Some(mount) = self.melee_mount(i, st.slot) else { return };
        let w = weapon(st.weapon);
        let Some(m) = w.melee else { return };
        let f = self.suits.flight[i];
        let rewind = if self.suits.pilot[i] == PilotKind::MobileDoll {
            0
        } else {
            t.saturating_sub(st.view_q4 >> 4).min(MAX_REWIND_TICKS)
        };
        let when = t - rewind;
        let faction = self.suits.faction[i];
        let (active, subs) = (u32::from(m.active), u32::from(m.sub_steps));
        for sub in 0..subs {
            let step = (active - u32::from(st.timer)) * subs + sub;
            for second in [false, true] {
                if second && !m.twin {
                    break;
                }
                // A twin weapon's blades are in the two hands, and a lost arm loses its blade.
                let hand_local = match (m.twin, second) {
                    (false, _) => mount.arm.muzzle(),
                    (true, false) => ArmSlot::Right.muzzle(),
                    (true, true) => ArmSlot::Left.muzzle(),
                };
                if m.twin {
                    let arm = if second { Part::ArmL } else { Part::ArmR };
                    if self.suits.part_hp[i][arm as usize] <= 0.0 {
                        continue;
                    }
                }
                let hand = f.pos + f.rot * hand_local;
                // The blade's span this sample (hand to tip, or the fang's head's travel), and the
                // way a hit drives what it strikes.
                let (a, b, cut) = match m.stroke {
                    Stroke::Swing => {
                        let progress = step as f32 / (active * subs) as f32;
                        let (from, to, cut) = if second {
                            (mirror(m.arc_from), mirror(m.arc_to), mirror(m.cut_dir))
                        } else {
                            (m.arc_from, m.arc_to, m.cut_dir)
                        };
                        let local = normalize_or(from.lerp(to, progress), Vec3::Z);
                        let tip = hand + f.rot * local * w.range;
                        (hand, tip, f.rot * normalize_or(cut, Vec3::X))
                    }
                    Stroke::Thrust => {
                        let dir = f.rot * st.dir;
                        let total = (active * subs) as f32;
                        let near = w.range * step as f32 / total;
                        let far = w.range * (step + 1) as f32 / total;
                        (hand + dir * near, hand + dir * far, dir)
                    }
                };
                // It works a rock, and cuts a hulk, once each a strike.
                if self.suits.melee[i].rock.is_none()
                    && let Some((at, rock)) = self.field.sweep(a, b, w.radius + SABER_DIG)
                {
                    self.suits.melee[i].rock = Some(rock as u16);
                    self.rock_hit(rock, w.damage, w.kind, a + (b - a) * at, cut, i, t);
                }
                if self.suits.melee[i].cut.is_none()
                    && let Some(k) = self.hulk_in_blade(a, b, w.radius)
                {
                    self.suits.melee[i].cut = Some(k as u16);
                    self.cut_hulk(k, a, b, t);
                }
                let key_bit = if second { SECOND_BLADE } else { 0 };
                let mut found: Option<(usize, usize, f32)> = None;
                let (spatial, suits, history, ff) =
                    (&mut self.spatial, &self.suits, &self.history, self.cfg.friendly_fire);
                let hits = &suits.melee[i];
                spatial.query_sphere(hand, w.range + 16.0, |j| {
                    if j == i || (!ff && suits.faction[j] == faction) {
                        return;
                    }
                    if hits.hits[..hits.n_hits as usize].contains(&(j as u16 | key_bit)) {
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
                    let gone = suits.gone_mask(j);
                    for (ci, c) in spec.capsules.iter().enumerate() {
                        if gone & (1 << ci) != 0 {
                            continue;
                        }
                        let (ca, cb, cr) = capsule_world(c, pos, rot);
                        let (_, _, d2) = segment_segment(a, b, ca, cb);
                        let rr = w.radius + cr;
                        if d2 <= rr * rr && found.is_none_or(|(_, _, bd)| d2 < bd) {
                            found = Some((j, ci, d2));
                        }
                    }
                });
                let Some((j, ci, _)) = found else { continue };
                // A clash: both blades out and the target facing us: both parried, no damage.
                let tj = self.suits.melee[j];
                let facing = (self.suits.flight[j].rot * Vec3::Z)
                    .dot(normalize_or(f.pos - self.suits.flight[j].pos, Vec3::Z))
                    > 0.5;
                let theirs = weapon(tj.weapon).melee;
                if tj.phase == MeleePhase::Active
                    && facing
                    && m.clashable
                    && theirs.is_some_and(|o| o.clashable)
                {
                    let their_recovery = theirs.map_or(m.clash_recovery, |o| o.clash_recovery);
                    for (k, recovery) in [(i, m.clash_recovery), (j, their_recovery)] {
                        self.suits.melee[k].phase = MeleePhase::Recovery;
                        self.suits.melee[k].timer = recovery;
                    }
                    self.events.push(Event::Clash { id: 0, tick: t, a: i as u16, b: j as u16 });
                    return;
                }
                let st = &mut self.suits.melee[i];
                if (st.n_hits as usize) < st.hits.len() {
                    st.hits[st.n_hits as usize] = j as u16 | key_bit;
                    st.n_hits += 1;
                }
                self.queue_damage(j, Part::ALL[ci], w.damage, i, w.kind, cut);
            }
        }
    }
}
