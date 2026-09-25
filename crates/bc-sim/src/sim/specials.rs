//! Frame specials: what they share (a cooldown), each one's step, and whether one is ready.

use super::Sim;
use crate::content::{SpecialKind, frame};
use crate::suits::{MeleePhase, SPECIAL_MOUNT};

impl Sim {
    /// Runs down the specials' cooldowns, and runs the ones that last (the jammer).
    pub(super) fn specials_step(&mut self, _t: u32) {
        for sp in self.suits.special.iter_mut() {
            sp.cooldown = sp.cooldown.saturating_sub(1);
        }
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            if let SpecialKind::HyperJammer { drain, min_energy, .. } = frame(self.suits.frame[i]).special {
                self.jammer_step(i, drain, min_energy);
            }
        }
        self.iter_bits = alive;
    }

    /// Whether suit `i`'s special can be used now.
    pub fn special_ready(&self, i: usize) -> bool {
        match frame(self.suits.frame[i]).special {
            SpecialKind::MeleeMove { .. } => {
                self.suits.melee[i].phase == MeleePhase::Idle && self.melee_ready(i, SPECIAL_MOUNT)
            }
            // On, or enough energy to engage.
            SpecialKind::HyperJammer { min_energy, .. } => {
                self.suits.special[i].active
                    || self.suits.energy[i] >= min_energy * frame(self.suits.frame[i]).energy_cap
            }
            SpecialKind::None | SpecialKind::Transform { .. } | SpecialKind::FullOpen { .. } => false,
        }
    }
}
