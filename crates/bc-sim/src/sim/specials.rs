//! Frame specials: what they share (a cooldown, which the fight charges: `content::specials`),
//! each one's step, and whether one is ready.

use bc_proto::buttons::{MODE, SPECIAL};

use super::Sim;
use crate::content::{SpecialKind, frame};
use crate::suits::{MeleePhase, MeleeState, SPECIAL_MOUNT};
use crate::transform::transform_step;

impl Sim {
    /// Runs down the specials' cooldowns, and runs the ones that last (the jammer).
    pub(super) fn specials_step(&mut self, t: u32) {
        for sp in self.suits.special.iter_mut() {
            sp.cooldown = sp.cooldown.saturating_sub(1);
        }
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            // Asleep, nobody works the frame's special: a bird stays a bird, a jammer stays off.
            if self.suits.sleeping.get(i) {
                continue;
            }
            match frame(self.suits.frame[i]).special {
                SpecialKind::HyperJammer { drain, min_energy, .. } => self.jammer_step(i, drain, min_energy),
                SpecialKind::FullOpen { ticks, lockout, cooldown } => {
                    self.full_open_step(i, ticks, lockout, cooldown, t)
                }
                SpecialKind::Transform { .. } => self.transform(i),
                SpecialKind::None | SpecialKind::MeleeMove { .. } => {}
            }
        }
        self.iter_bits = alive;
    }

    /// The suit's form follows MODE (see `transform`). A change drops any strike or charge under
    /// way; at its end the suit is the other frame.
    fn transform(&mut self, i: usize) {
        let s = &mut self.suits;
        let mut form = s.form(i);
        if transform_step(&mut form, s.input[i].pressed(MODE)) {
            s.melee[i] = MeleeState::default();
            for ws in s.weapons[i].iter_mut() {
                ws.charge = 0;
            }
            s.stats[i].specials += 1;
        }
        s.special[i].timer = form.timer;
        s.frame[i] = form.frame;
    }

    /// Whether suit `i` is changing form.
    pub fn transforming(&self, i: usize) -> bool {
        self.suits.form(i).changing()
    }

    /// Full Open Attack: a SPECIAL press opens every hatch for `ticks` (the weapons fire on their
    /// own, heat or not); then the suit is forced into an overheat it can't fire through for
    /// `lockout` ticks. Ready again `cooldown` ticks after it starts, sooner in a fight.
    fn full_open_step(&mut self, i: usize, ticks: u16, lockout: u16, cooldown: u16, t: u32) {
        let s = &mut self.suits;
        let cap = frame(s.frame[i]).heat_cap;
        let sp = &mut s.special[i];
        sp.lockout = sp.lockout.saturating_sub(1);
        if sp.active {
            sp.timer = sp.timer.saturating_sub(1);
            if sp.timer == 0 {
                sp.active = false;
                sp.lockout = lockout;
                s.heat[i] = s.heat[i].max(cap);
            }
            return;
        }
        let pressed = s.input[i].pressed(SPECIAL) && s.prev_buttons[i] & SPECIAL == 0;
        if pressed && sp.cooldown == 0 && !s.overheated[i] && s.stagger[i] == 0 {
            sp.active = true;
            sp.timer = ticks;
            sp.cooldown = cooldown;
            s.stats[i].specials += 1;
            self.break_jammer(i, t);
        }
    }

    /// The fight charges suit `i`'s special (`content::specials`): `share` of its whole cooldown
    /// comes off what's left of it, unless it's under way or Full Open's lockout lasts.
    pub(super) fn charge_special(&mut self, i: usize, share: f32) {
        let full = frame(self.suits.frame[i]).special.cooldown();
        let sp = &mut self.suits.special[i];
        if full == 0 || sp.active || sp.lockout > 0 || sp.cooldown == 0 || share <= 0.0 {
            return;
        }
        sp.cooldown = sp.cooldown.saturating_sub((share * f32::from(full)) as u16);
    }

    /// How charged suit `i`'s special is, in 255ths (255: charged, or it has no cooldown).
    pub fn special_charge(&self, i: usize) -> u8 {
        let full = u32::from(frame(self.suits.frame[i]).special.cooldown());
        let left = u32::from(self.suits.special[i].cooldown).min(full);
        ((full - left) * 255).checked_div(full).map_or(255, |c| c as u8)
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
            SpecialKind::FullOpen { .. } => {
                let sp = &self.suits.special[i];
                !sp.active && sp.cooldown == 0 && !self.suits.overheated[i]
            }
            SpecialKind::Transform { .. } => !self.transforming(i),
            SpecialKind::None => false,
        }
    }
}
