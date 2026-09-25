//! ZERO strain: the System feeds its pilot futures faster than a mind can take. Strain builds while
//! it's engaged (faster under G); at 100 % ZERO seizes the suit for 3 s, then locks out for 10 s.

use bc_proto::snapshot::zero_mode;

use super::ZeroOut;
use crate::config::secs;

/// Strain per second while engaged (≈28 s from zero to seizure at 1 g).
const BUILD: f32 = 0.035;
/// Recovery per second while disengaged.
const DECAY: f32 = 0.05;

#[derive(Clone, Copy, Debug, Default)]
pub struct ZeroState {
    pub mode: u8,
    pub strain: f32,
    /// Ticks left in a seizure or lockout.
    pub timer: u32,
    pub out: ZeroOut,
    /// Staggers rollouts across pilots.
    pub next_update: u32,
}

/// Result of one strain update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrainEvent {
    None,
    Seized,
    Released,
}

impl ZeroState {
    pub fn active(&self) -> bool {
        self.mode == zero_mode::ACTIVE || self.mode == zero_mode::SEIZED
    }

    /// Advances one tick. `want_on` is the pilot's ZERO switch state.
    pub fn update(&mut self, want_on: bool, capable: bool, g_strain: f32, dt: f32) -> StrainEvent {
        match self.mode {
            zero_mode::OFF => {
                self.strain = (self.strain - DECAY * dt).max(0.0);
                if want_on && capable {
                    self.mode = zero_mode::ACTIVE;
                }
                StrainEvent::None
            }
            zero_mode::ACTIVE => {
                if !want_on || !capable {
                    self.mode = zero_mode::OFF;
                    return StrainEvent::None;
                }
                self.strain += BUILD * (1.0 + g_strain) * dt;
                if self.strain >= 1.0 {
                    self.strain = 1.0;
                    self.mode = zero_mode::SEIZED;
                    self.timer = secs(3.0);
                    return StrainEvent::Seized;
                }
                StrainEvent::None
            }
            zero_mode::SEIZED => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.mode = zero_mode::LOCKOUT;
                    self.timer = secs(10.0);
                    self.strain = 0.3;
                    return StrainEvent::Released;
                }
                StrainEvent::None
            }
            _ => {
                self.strain = (self.strain - DECAY * dt).max(0.0);
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.mode = zero_mode::OFF;
                }
                StrainEvent::None
            }
        }
    }
}
