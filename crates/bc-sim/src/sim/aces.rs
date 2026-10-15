//! The Almagest's aces among the drones (`content::aces`): one out at a time, fielded every
//! `SimConfig::ace_every` while none is, round the list.

use bc_proto::{Faction, NO_SLOT, PilotKind};
use glam::Vec3;

use super::{ANCHORS, MAX_SQUADS, Sim, Squad};
use crate::content::aces::{ACE_ARMOUR, ACES, NO_ACE, WING_OFFSET, ace};
use crate::math::look_rotation;

impl Sim {
    /// The ace out in the sector, if one is: its suit, and which it is. It's out from its fielding
    /// until its slot is let go, flying or (for the few seconds a downed drone stays) not: there's
    /// never more than one.
    pub fn ace_out(&self) -> Option<(usize, u8)> {
        let i = usize::from(self.ace_suit);
        (self.ace_suit != NO_SLOT && self.suits.used.get(i) && self.suits.ace[i] != NO_ACE)
            .then(|| (i, self.suits.ace[i]))
    }

    /// Which ace suit `i` is, if it's one.
    pub fn ace_of(&self, i: usize) -> Option<u8> {
        let a = self.suits.ace[i];
        (a != NO_ACE).then_some(a)
    }

    /// Fields the next ace on the list when one's due and none is out: a squad of its own, high
    /// over one of the drones' anchors, with its wingman beside it if it has one.
    pub(super) fn spawn_ace(&mut self, t: u32) {
        let every = self.cfg.ace_every;
        if every == 0 || self.cfg.target_drones == 0 || t < self.next_ace_at || self.ace_out().is_some() {
            return;
        }
        self.next_ace_at = t + every;
        let squad = self.next_squad % MAX_SQUADS;
        self.next_squad += 1;
        let anchor = ANCHORS[squad % ANCHORS.len()] + Vec3::new(0.0, 700.0, 0.0);
        self.squads[squad] = Squad { anchor, focus: NO_SLOT };
        let rot = look_rotation(-anchor, Vec3::Y);
        let a = ace(self.next_ace);
        let Some(id) = self.spawn_at(a.frame, Faction::Almagest, PilotKind::Drone, anchor, rot) else {
            return;
        };
        let i = id.idx();
        self.suits.ai[i].squad = squad as u8;
        self.suits.ace[i] = self.next_ace;
        for hp in self.suits.part_hp[i].iter_mut() {
            *hp *= ACE_ARMOUR;
        }
        self.ace_suit = i as u16;
        self.next_ace = ((usize::from(self.next_ace) + 1) % ACES.len()) as u8;
        // Its wingman, off its right shoulder.
        if let Some(wing) = a.wing {
            let at = anchor + rot * Vec3::new(WING_OFFSET, 0.0, 0.0);
            if let Some(w) = self.spawn_at(wing, Faction::Almagest, PilotKind::Drone, at, rot) {
                self.suits.ai[w.idx()].squad = squad as u8;
            }
        }
    }
}
