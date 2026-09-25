//! Frame specials: what they share (a cooldown), and whether one is ready.

use super::Sim;
use crate::content::{SpecialKind, frame};
use crate::suits::{MeleePhase, SPECIAL_MOUNT};

impl Sim {
    /// Runs down the specials' cooldowns.
    pub(super) fn specials_step(&mut self, _t: u32) {
        for sp in self.suits.special.iter_mut() {
            sp.cooldown = sp.cooldown.saturating_sub(1);
        }
    }

    /// Whether suit `i`'s special can be used now.
    pub fn special_ready(&self, i: usize) -> bool {
        match frame(self.suits.frame[i]).special {
            SpecialKind::MeleeMove { .. } => {
                self.suits.melee[i].phase == MeleePhase::Idle && self.melee_ready(i, SPECIAL_MOUNT)
            }
            SpecialKind::None
            | SpecialKind::Transform { .. }
            | SpecialKind::HyperJammer { .. }
            | SpecialKind::FullOpen { .. } => false,
        }
    }
}
