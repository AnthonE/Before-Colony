//! Cone weapons: Shenlong's flamethrower.
//!
//! While fire is held the flame is lit (the slot's firing flag shows it), and every `interval`
//! ticks it burns what's in its cone: each suit there takes the weapon's damage on the part
//! nearest the flame's axis, and its heat rises, which can overheat it. Each burn costs a round.
//! Rocks shield what's behind them. There's no lag compensation: a flame lingers.

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY};
use bc_proto::{InputCmd, Part};
use glam::Vec3;

use super::Sim;
use super::combat::clamp_to_cone;
use crate::collide::{capsule_world, segment_segment};
use crate::content::{Mount, WeaponSpec, frame};
use crate::math::{cos, normalize_or, sin};

/// Suits one burn can reach.
const MAX_BURNT: usize = 8;

impl Sim {
    pub(super) fn flame(
        &mut self,
        i: usize,
        slot: usize,
        mount: Mount,
        w: &WeaponSpec,
        cmd: &InputCmd,
        t: u32,
    ) {
        let Some(cone) = w.cone else { return };
        let button = if slot == 0 { FIRE_PRIMARY } else { FIRE_SECONDARY };
        let arm_ok = self.suits.arm_free(i, mount.arm) && !self.arm_blocked(i, mount.arm);
        let s = &mut self.suits;
        let mut ws = s.weapons[i][slot];
        ws.cooldown = ws.cooldown.saturating_sub(1);
        let lit = cmd.pressed(button) && arm_ok && ws.ammo > 0 && !s.overheated[i] && s.energy[i] >= w.energy;
        let burn = lit && ws.cooldown == 0;
        if lit {
            if slot == 0 {
                s.fired_primary[i] = t;
            } else {
                s.fired_secondary[i] = t;
            }
            s.last_fired[i] = t;
        }
        if burn {
            ws.cooldown = u16::from(cone.interval);
            ws.ammo -= 1;
            s.heat[i] += w.heat;
            s.energy[i] -= w.energy;
            s.stats[i].shots += 1;
        }
        s.weapons[i][slot] = ws;
        if !burn {
            return;
        }
        self.break_jammer(i, t);

        let s = &self.suits;
        let f = s.flight[i];
        let fwd = f.rot * Vec3::Z;
        let dir = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, mount.arm.cone());
        let nozzle = f.pos + f.rot * mount.arm.muzzle();
        let tip = nozzle + dir * w.range;
        // The cone's radius per metre along it.
        let spread = sin(cone.half_angle) / cos(cone.half_angle);
        let faction = s.faction[i];
        let mut burnt = [(0usize, Part::Torso, Vec3::ZERO); MAX_BURNT];
        let mut n = 0;
        let (spatial, suits, ff) = (&mut self.spatial, &self.suits, self.cfg.friendly_fire);
        spatial.query_sphere(nozzle, w.range + 16.0, |j| {
            if j == i || (!ff && suits.faction[j] == faction) || n == MAX_BURNT {
                return;
            }
            let fl = &suits.flight[j];
            let spec = frame(suits.frame[j]);
            let gone = suits.gone_mask(j);
            // The part nearest the flame's axis, if the cone takes any of it: its point nearest the
            // axis, within reach and within the cone's radius there.
            let mut best: Option<(f32, usize, Vec3)> = None;
            for (ci, c) in spec.capsules.iter().enumerate() {
                if gone & (1 << ci) != 0 {
                    continue;
                }
                let (ca, cb, cr) = capsule_world(c, fl.pos, fl.rot);
                let (_, on, _) = segment_segment(nozzle, tip, ca, cb);
                let p = ca + (cb - ca) * on;
                let x = (p - nozzle).dot(dir);
                let off2 = (p - nozzle - dir * x).length_squared();
                let reach = x.max(0.0) * spread + cr;
                if x > -cr
                    && x <= w.range + cr
                    && off2 <= reach * reach
                    && best.is_none_or(|(bd, ..)| off2 < bd)
                {
                    best = Some((off2, ci, p));
                }
            }
            if let Some((_, ci, at)) = best {
                burnt[n] = (j, Part::ALL[ci], at);
                n += 1;
            }
        });
        for &(j, part, at) in &burnt[..n] {
            if self.field.sweep(nozzle, at, 0.0).is_some() {
                continue;
            }
            self.queue_damage(j, part, w.damage, i, w.kind, dir);
            self.suits.heat[j] += cone.target_heat;
        }
    }
}
