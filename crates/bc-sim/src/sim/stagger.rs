//! Stagger (`content::stagger`, after Armored Core VI): every blow's impact builds on a suit's
//! attitude control, and drains away once the suit is left alone. Past its frame's stability the
//! suit is staggered for [`STAGGER_TICKS`]:
//! - it tumbles with the spin the blow knocked into it: its attitude control does nothing, and its
//!   thrusters give a quarter of their thrust (`flight::FlightMods::staggered`; on its feet, its
//!   legs neither walk nor turn it);
//! - its weapons are down, and a strike under way is lost;
//! - blows that land on it meanwhile are direct hits ([`DIRECT_HIT`]).
//!
//! Then its impact starts again from nothing. The owner's client flies the stagger as the server
//! does, from the ticks its own state carries.

use bc_proto::WeaponKind;
use bc_proto::events::Event;
use glam::Vec3;

use super::Sim;
use crate::config::DT;
use crate::content::stagger::{RECOVER_AFTER, RECOVER_RATE, STAGGER_SPIN, STAGGER_TICKS, impact, stability};
use crate::ground::Footing;
use crate::math::{hash01, normalize_or};
use crate::suits::MeleeState;

impl Sim {
    /// Whether suit `i` is staggered.
    #[inline]
    pub fn staggered(&self, i: usize) -> bool {
        self.suits.stagger[i] > 0
    }

    /// A blow of `raw` armour points (before the target's armour) from `weapon`, struck along
    /// `dir`, on suit `j`: what it adds to the suit's impact, and the stagger once that's past what
    /// the suit stands. A staggered suit takes none (it starts again once it's steady).
    pub(super) fn impact(&mut self, j: usize, raw: f32, weapon: WeaponKind, dir: Vec3, t: u32) {
        if self.suits.stagger[j] > 0 {
            return;
        }
        let limit = stability(self.suits.frame[j]);
        let built = self.suits.impact[j] + raw * impact(weapon);
        if built < limit {
            self.suits.impact[j] = built;
            return;
        }
        self.suits.impact[j] = 0.0;
        self.suits.stagger[j] = STAGGER_TICKS;
        // Its arms are flung out: a strike under way is lost.
        self.suits.melee[j] = MeleeState::default();
        // The blow knocks it spinning, about an axis square to the blow (on its feet, it's braced:
        // it only stumbles).
        if self.suits.footing[j] == Footing::Free {
            let salt = j as u32 ^ 0x57A6;
            let any =
                Vec3::new(hash01(t, salt) - 0.5, hash01(t ^ 0x1D, salt) - 0.5, hash01(t ^ 0xE2, salt) - 0.5);
            let axis = normalize_or(dir.cross(any), normalize_or(any, Vec3::X));
            self.suits.flight[j].ang_vel += axis * STAGGER_SPIN;
        }
        self.events.push(Event::Staggered { id: 0, tick: t, suit: j as u16 });
    }

    /// Staggers run out; impact drains from suits left alone for a while.
    pub(super) fn stagger_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let s = &mut self.suits;
            if s.stagger[i] > 0 {
                s.stagger[i] -= 1;
                continue;
            }
            if s.impact[i] > 0.0 && t.saturating_sub(s.last_hit[i]) >= RECOVER_AFTER {
                let drain = stability(s.frame[i]) * RECOVER_RATE * DT;
                s.impact[i] = (s.impact[i] - drain).max(0.0);
            }
        }
        self.iter_bits = alive;
    }

    /// Suit `i`'s impact as a share of what it stands, 0..1 (1 while it's staggered).
    pub fn impact_share(&self, i: usize) -> f32 {
        if self.suits.stagger[i] > 0 {
            1.0
        } else {
            (self.suits.impact[i] / stability(self.suits.frame[i])).clamp(0.0, 1.0)
        }
    }
}
