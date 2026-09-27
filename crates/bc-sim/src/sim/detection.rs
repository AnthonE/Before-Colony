//! What a suit's sensors pick up, what it designates, and Deathscythe's Hyper Jammer.
//!
//! Everything that asks "can `i` see `j`?" asks [`Sim::detects`]: replication (so an undetected
//! suit leaves its enemies' clients), Mobile Doll and ZERO perception, locks. A jamming suit shows
//! its enemies a twentieth of its signature, and their eyes see it only close in. Firing, striking
//! or using a special shows through the jammer for a while.

use bc_proto::Part;
use bc_proto::buttons::MODE;

use super::Sim;
use crate::config::{DT, VISUAL_RANGE};
use crate::content::{SpecialKind, frame};
use crate::sensors;

impl Sim {
    /// Whether `viewer`'s sensors pick up suit `j` (alive or a wreck): within eyesight, or within
    /// its sensor range scaled by `j`'s signature (see `sensors`).
    pub fn detects(&self, viewer: usize, j: usize) -> bool {
        let s = &self.suits;
        let head_ok = s.part_hp[viewer][Part::Head as usize] > 0.0;
        let range = frame(s.frame[viewer]).sensor_range * if head_ok { 1.0 } else { 0.4 };
        let mut sig = sensors::signature(
            frame(s.frame[j]).signature,
            s.boosting[j],
            self.tick().saturating_sub(s.last_fired[j]) < 30,
            !s.alive.get(j),
        );
        let mut visual = VISUAL_RANGE;
        if s.faction[j] != s.faction[viewer]
            && let Some((jam_sig, jam_visual)) = self.jamming(j)
        {
            sig *= jam_sig;
            visual = jam_visual;
        }
        sensors::detects_within(s.flight[viewer].pos, range, s.flight[j].pos, sig, visual)
    }

    /// While suit `j`'s Hyper Jammer is on (and not broken): its signature multiplier and the range
    /// eyes see it within.
    pub fn jamming(&self, j: usize) -> Option<(f32, f32)> {
        let sp = &self.suits.special[j];
        match frame(self.suits.frame[j]).special {
            SpecialKind::HyperJammer { sig, visual, .. }
                if sp.active && self.tick() >= sp.break_until && self.suits.alive.get(j) =>
            {
                Some((sig, visual))
            }
            _ => None,
        }
    }

    /// Whether `viewer` has lost suit `j` to its jammer.
    pub fn jammed_from(&self, viewer: usize, j: usize) -> bool {
        self.jamming(j).is_some()
            && self.suits.faction[j] != self.suits.faction[viewer]
            && !self.detects(viewer, j)
    }

    /// The suit `i` designates (its lock target), if that's a live hostile on `i`'s sensors.
    pub fn designation(&self, i: usize) -> Option<usize> {
        let s = &self.suits;
        let t = usize::from(s.input[i].lock_target);
        (t < s.cap && t != i && s.alive.get(t) && s.faction[t] != s.faction[i] && self.detects(i, t))
            .then_some(t)
    }

    /// Firing, striking or a special shows through suit `i`'s jammer for a while.
    pub(super) fn break_jammer(&mut self, i: usize, t: u32) {
        if let SpecialKind::HyperJammer { break_ticks, .. } = frame(self.suits.frame[i]).special {
            self.suits.special[i].break_until = t + u32::from(break_ticks);
        }
    }

    /// The jammer follows MODE: it engages with enough energy, drains it, and drops when it's gone.
    pub(super) fn jammer_step(&mut self, i: usize, drain: f32, min_energy: f32) {
        let s = &mut self.suits;
        let cap = frame(s.frame[i]).energy_cap;
        let sp = &mut s.special[i];
        if !s.input[i].pressed(MODE) {
            sp.active = false;
        } else if !sp.active && s.energy[i] >= min_energy * cap {
            sp.active = true;
            s.stats[i].specials += 1;
        }
        if sp.active {
            s.energy[i] -= drain * DT;
            if s.energy[i] <= 0.0 {
                s.energy[i] = 0.0;
                sp.active = false;
            }
        }
    }
}
