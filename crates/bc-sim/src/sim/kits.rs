//! Using a consumable from the rack (`content::kits`): its pilot's hotbar, at a tick boundary.

use bc_proto::NO_SLOT;

use super::Sim;
use crate::content::System;
use crate::content::kits::{CHAFF_TICKS, CRASH_TICKS, Kit, STIM_TICKS};
use crate::content::systems::{DAMAGED, FAILED, OK};
use crate::handle::SuitId;
use crate::suits::LockState;

impl Sim {
    /// Suit `id`'s pilot uses one `kit` from the rack. Whether it did anything (there was one,
    /// and something for it to do; otherwise it stays in the rack).
    pub fn use_kit(&mut self, id: SuitId, kit: Kit) -> bool {
        if !self.suits.valid(id) {
            return false;
        }
        let i = id.idx();
        let s = &self.suits;
        if !s.alive.get(i) || s.sleeping.get(i) || s.kits[i].get(kit) == 0 {
            return false;
        }
        let used = match kit {
            Kit::Patch => self.patch(i),
            Kit::Coolant => {
                let s = &mut self.suits;
                let hot = s.heat[i] > 0.0;
                s.heat[i] = 0.0;
                if s.special[i].lockout == 0 {
                    s.overheated[i] = false;
                }
                hot
            }
            Kit::Chaff => {
                self.chaff(i);
                true
            }
            Kit::Stim => {
                let st = &mut self.suits.status[i];
                let free = st.stim == 0;
                if free {
                    st.stim = STIM_TICKS + CRASH_TICKS;
                }
                free
            }
        };
        if used {
            self.suits.kits[i].take(kit);
            self.suits.retune(i);
        }
        used
    }

    /// A field repair on suit `i`: a leaking tank first, then the worst-off system it can reach
    /// (a part shot off is beyond it), a level better. Whether there was anything to patch.
    fn patch(&mut self, i: usize) -> bool {
        let s = &mut self.suits;
        let gone = s.gone_mask(i);
        let reach = |sys: System| gone & (1 << sys.part() as u8) == 0;
        let sys = Some(System::Tank)
            .filter(|t| reach(*t) && s.systems[i].get(*t) > OK)
            .or_else(|| System::ALL.into_iter().find(|t| reach(*t) && s.systems[i].get(*t) >= FAILED))
            .or_else(|| System::ALL.into_iter().find(|t| reach(*t) && s.systems[i].get(*t) >= DAMAGED));
        let Some(sys) = sys else { return false };
        let level = s.systems[i].get(sys);
        s.systems[i].set(sys, level - 1);
        if sys == System::Reactor {
            s.status[i].scram = 0;
        }
        // Damage control was on it: it's done for now.
        if s.status[i].repairing == sys as u8 {
            (s.status[i].repairing, s.status[i].repair_left) = (crate::suits::NO_REPAIR, 0);
        }
        true
    }

    /// Chaff from suit `i`: every lock on it breaks, missiles tracking it lose it, and none can be
    /// built on it for a while ([`Sim::chaffed`]).
    fn chaff(&mut self, i: usize) {
        let generation = self.suits.generation[i];
        let m = &mut self.missiles;
        for k in m.alive.iter() {
            if usize::from(m.target[k]) == i && m.target_gen[k] == generation {
                m.target[k] = NO_SLOT;
            }
        }
        for lock in self.suits.lock.iter_mut() {
            if usize::from(lock.target) == i {
                *lock = LockState::default();
            }
        }
        self.suits.status[i].chaff = CHAFF_TICKS;
    }

    /// Chaff still hangs round suit `j`: no lock can be built on it, nor a seeker hold it.
    #[inline]
    pub(super) fn chaffed(&self, j: usize) -> bool {
        self.suits.status[j].chaff > 0
    }
}
