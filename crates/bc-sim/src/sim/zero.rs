//! The ZERO System's per-pilot update (staggered every `zero_interval` ticks) and the tactical
//! picture handed to external oracles.

use bc_proto::NO_SLOT;
use glam::Vec3;

use super::Sim;
use crate::content::{frame, weapon};
use crate::math::{confidence, length};
use crate::perception::Perception;
use crate::zero::advice::{EXTERNAL_WEIGHT, PICTURE_THREATS, TacticalPicture, ThreatBrief, blend};
use crate::zero::fire_control::{intercept, miss_distance};
use crate::zero::local_oracle::{
    flanked, hypothesis_accels, maneuver_likelihood, own_maneuver_distribution, posterior,
    target_distribution, threat_levels, threat_score,
};
use crate::zero::{N_HYP, ThreatTrack, ZERO_THREATS, ZeroOut, predictive};

/// Effective hit radius of a suit for the ZERO hit estimate (torso plus limbs), m.
const SUIT_HIT_RADIUS: f32 = 4.0;

fn argmax(v: &[f32]) -> usize {
    let mut b = 0;
    for i in 1..v.len() {
        if v[i] > v[b] {
            b = i;
        }
    }
    b
}

impl Sim {
    pub(super) fn zero_step(&mut self, t: u32) {
        let interval = self.cfg.zero_interval.max(1);
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        let mut scratch = core::mem::take(&mut self.scratch);
        for i in alive.iter() {
            let z = &mut self.suits.zero[i];
            if !z.active() || t < z.next_update {
                continue;
            }
            // Stagger pilots across the interval.
            z.next_update = t + interval;
            self.perceive_into(i, &mut scratch);
            let out = self.compute_zero(i, &scratch, t);
            self.suits.zero[i].out = out;
        }
        self.scratch = scratch;
        self.iter_bits = alive;
    }

    fn compute_zero(&self, i: usize, p: &Perception, t: u32) -> ZeroOut {
        let me = &p.me;
        let spec = frame(me.frame);
        let prev = self.suits.zero[i].out;
        let adv = &self.advice[i];
        let fresh = adv.fresh(t) && adv.pilot == i as u16;
        let mut out = ZeroOut { computed_at: t, ..ZeroOut::default() };

        // --- The two most dangerous threats and their next maneuver. ---
        let mut top: [(f32, usize); ZERO_THREATS] = [(0.0, usize::MAX); ZERO_THREATS];
        for (ci, c) in p.contacts().iter().enumerate() {
            let s = threat_score(c);
            if s > top[1].0 {
                top[1] = (s, ci);
                if top[1].0 > top[0].0 {
                    top.swap(0, 1);
                }
            }
        }
        let mut n = 0;
        for &(score, ci) in &top {
            if ci == usize::MAX || score <= 0.0 {
                continue;
            }
            let c = &p.contacts()[ci];
            let mut prior = [1.0 / N_HYP as f32; N_HYP];
            if let Some(pt) = prev.threats[..prev.n_threats as usize].iter().find(|tr| tr.slot == c.slot) {
                // Transition model between updates (0.1 s): maneuvers mostly persist.
                for (p, q) in prior.iter_mut().zip(pt.post) {
                    *p = 0.9 * q + 0.1 / N_HYP as f32;
                }
            }
            if fresh && let Some(ext) = adv.maneuvers_for(c.slot) {
                blend(&mut prior, ext, EXTERNAL_WEIGHT);
            }
            let cs = frame(c.frame);
            let scale = cs.main_thrust / cs.mass(cs.propellant_cap);
            let lik = maneuver_likelihood(c.accel, &hypothesis_accels(c), scale);
            let post = posterior(&prior, &lik);
            out.threats[n] = ThreatTrack { slot: c.slot, probs: predictive(&post), post };
            n += 1;
        }
        out.n_threats = n as u8;

        // --- Target recommendation. ---
        let mut slots = [NO_SLOT; 4];
        let mut probs = [0.0f32; 4];
        let nt = target_distribution(p.contacts(), &mut slots, &mut probs);
        if fresh && adv.n_targets > 0 && nt > 0 {
            let mut ext = [0.0f32; 4];
            for k in 0..nt {
                ext[k] = (0..adv.n_targets as usize)
                    .find(|&m| adv.target_slots[m] == slots[k])
                    .map_or(0.02, |m| adv.target_probs[m]);
            }
            blend(&mut probs[..nt], &ext[..nt], EXTERNAL_WEIGHT);
        }
        if nt > 0 {
            let b = argmax(&probs[..nt]);
            out.rec_target = slots[b];
            out.rec_target_p = probs[b];
        }

        // --- Firing solution: the aim that connects across the most probability mass. ---
        if let (Some(mount), Some(tc)) = (spec.loadout[0], p.get(out.rec_target)) {
            let w = weapon(mount.weapon);
            let muzzle = me.pos + me.rot * mount.arm.muzzle();
            let accs = hypothesis_accels(tc);
            let probs_t = match out.threats[..n].iter().find(|tr| tr.slot == tc.slot) {
                Some(tr) => tr.probs,
                None => {
                    let cs = frame(tc.frame);
                    predictive(&posterior(
                        &[1.0 / N_HYP as f32; N_HYP],
                        &maneuver_likelihood(tc.accel, &accs, cs.main_thrust / cs.mass(cs.propellant_cap)),
                    ))
                }
            };
            let r_hit = SUIT_HIT_RADIUS + w.radius;
            let mut best = (-1.0f32, me.aim);
            for cand in 0..=N_HYP {
                let dir = if cand < N_HYP {
                    match intercept(muzzle, me.vel, w.speed, tc.pos, tc.vel, accs[cand]) {
                        Some(s) => s.dir,
                        None => continue,
                    }
                } else {
                    me.aim
                };
                let mut hp = 0.0;
                for k in 0..N_HYP {
                    if miss_distance(muzzle, me.vel, w.speed, dir, tc.pos, tc.vel, accs[k]) < r_hit {
                        hp += probs_t[k];
                    }
                }
                if hp > best.0 {
                    best = (hp, dir);
                }
            }
            if best.0 >= 0.0 {
                out.has_solution = true;
                out.solution = best.1;
                out.hit_p = best.0.clamp(0.0, 1.0);
                out.solution_target = tc.slot;
            }
        }

        // --- Own maneuver, threat level, flanking. ---
        let mut own = own_maneuver_distribution(me, spec, p.contacts());
        if fresh && adv.own_action_probs.iter().sum::<f32>() > 0.5 {
            blend(&mut own, &adv.own_action_probs, EXTERNAL_WEIGHT);
        }
        let b = argmax(&own);
        out.rec_maneuver = b as u8;
        out.rec_maneuver_p = own[b];
        let mut lv = threat_levels(p.contacts());
        if fresh && adv.threat_probs.iter().sum::<f32>() > 0.5 {
            blend(&mut lv, &adv.threat_probs, EXTERNAL_WEIGHT);
        }
        let b = argmax(&lv);
        out.threat_level = b as u8;
        out.threat_confidence = confidence(&lv);
        let fl = flanked(me, p.contacts());
        out.flanked = if fresh { 0.5 * fl + 0.5 * adv.flanked } else { fl };
        out.source_jev = fresh && adv.from_jev;
        out.advice_age = if fresh { (t.saturating_sub(adv.computed_at_tick) / 2).min(15) as u8 } else { 0 };
        out
    }

    /// Fills the tactical picture an external oracle (Jev) sees for pilot `i`. Returns `false` when
    /// the pilot isn't flying with ZERO engaged.
    pub fn picture_for(&mut self, i: usize, out: &mut TacticalPicture) -> bool {
        if !self.suits.is_alive(i) || !self.suits.zero[i].active() {
            return false;
        }
        let mut scratch = core::mem::take(&mut self.scratch);
        self.perceive_into(i, &mut scratch);
        let me = scratch.me;
        let z = self.suits.zero[i];
        *out = TacticalPicture {
            pilot: i as u16,
            tick: self.tick(),
            frame: me.frame,
            parts: me.parts,
            heat: me.heat,
            energy: me.energy,
            propellant: me.propellant,
            g_strain: me.g_strain,
            zero_strain: z.strain,
            speed: length(me.vel),
            ..TacticalPicture::default()
        };
        // Top threats by score.
        let mut order: [(f32, usize); PICTURE_THREATS] = [(0.0, usize::MAX); PICTURE_THREATS];
        for (ci, c) in scratch.contacts().iter().enumerate() {
            let s = threat_score(c);
            let mut k = PICTURE_THREATS - 1;
            if s <= order[k].0 {
                continue;
            }
            order[k] = (s, ci);
            while k > 0 && order[k].0 > order[k - 1].0 {
                order.swap(k, k - 1);
                k -= 1;
            }
        }
        let fwd = me.forward();
        for &(s, ci) in &order {
            if ci == usize::MAX || s <= 0.0 {
                continue;
            }
            let c = &scratch.contacts()[ci];
            let to = c.pos - me.pos;
            let dist = length(to).max(1e-3);
            let local_probs = z.out.threats[..z.out.n_threats as usize]
                .iter()
                .find(|tr| tr.slot == c.slot)
                .map_or([1.0 / N_HYP as f32; N_HYP], |tr| tr.probs);
            out.threats[out.n as usize] = ThreatBrief {
                slot: c.slot,
                frame: c.frame,
                pilot: c.pilot,
                distance: dist,
                closing_speed: -(c.vel - me.vel).dot(to / dist),
                bearing_deg: crate::math::angle_between(fwd, to / dist).to_degrees(),
                hull: c.hull,
                firing: c.firing,
                aiming_at_me: c.aiming_at_me,
                accel_g: length(c.accel) / crate::config::G0,
                local_probs,
            };
            out.n += 1;
        }
        self.scratch = scratch;
        let _ = Vec3::ZERO;
        true
    }
}
