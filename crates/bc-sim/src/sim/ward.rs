//! The Corona's Ward (`content::SpecialKind::Ward`; `docs/DESIGN.md`, "The prototype pair"): MODE
//! sends its ward discs out to a sphere round the suit, and once they hold it, every hostile
//! beam that would cross into it from outside stops at it, and the energy it carried comes off the
//! suit's. A shot fired from inside goes out as ever (the Corona's own, a wingman's, an enemy's
//! who has closed in), and solid rounds, missiles, blades and flame go through: the field is
//! energy against energy.

use bc_proto::Faction;
use bc_proto::buttons::MODE;
use glam::Vec3;

use super::Sim;
use crate::collide::sphere_entry;
use crate::config::DT;
use crate::content::{SpecialKind, frame};

/// The most fields that stand at once (more Coronas' wards fly out but hold none: the
/// Almagest has two prototypes in the sky at the most).
pub const MAX_WARDS: usize = 8;

/// The fields standing this tick, by suit (rebuilt as the specials step).
#[derive(Clone, Copy, Debug)]
pub(crate) struct WardList {
    pub n: u8,
    pub suit: [u16; MAX_WARDS],
}

impl WardList {
    pub const NONE: Self = Self { n: 0, suit: [0; MAX_WARDS] };

    fn push(&mut self, i: usize) {
        if usize::from(self.n) < MAX_WARDS {
            self.suit[usize::from(self.n)] = i as u16;
            self.n += 1;
        }
    }
}

impl Sim {
    /// The wards follow MODE: with enough energy they fly out, and `deploy` ticks later they
    /// hold the field. Out, they drain `drain` energy/s, and fall back when it's gone; let go of
    /// MODE, they come home.
    pub(super) fn ward_step(&mut self, i: usize, deploy: u8, drain: f32, min_energy: f32) {
        let s = &mut self.suits;
        let cap = frame(s.frame[i]).energy_cap * s.tuning[i].energy_cap;
        let sp = &mut s.special[i];
        if !s.input[i].pressed(MODE) {
            sp.active = false;
        } else if !sp.active && s.energy[i] >= min_energy * cap {
            sp.active = true;
            sp.timer = 0;
            s.stats[i].specials += 1;
        }
        if sp.active {
            sp.timer = (sp.timer + 1).min(u16::from(deploy));
            s.energy[i] -= drain * DT;
            if s.energy[i] <= 0.0 {
                s.energy[i] = 0.0;
                sp.active = false;
            }
        }
        if !sp.active {
            sp.timer = 0;
        }
        if self.ward(i).is_some() {
            self.wards.push(i);
        }
    }

    /// Whether suit `i`'s wards are out (flying to their places, or holding the field).
    pub fn wards_out(&self, i: usize) -> bool {
        matches!(frame(self.suits.frame[i]).special, SpecialKind::Ward { .. })
            && self.suits.special[i].active
            && self.suits.alive.get(i)
            && !self.suits.sleeping.get(i)
    }

    /// The radius of the field suit `i`'s wards hold, if they hold one.
    pub fn ward(&self, i: usize) -> Option<f32> {
        match frame(self.suits.frame[i]).special {
            SpecialKind::Ward { radius, deploy, .. }
                if self.wards_out(i) && self.suits.special[i].timer >= u16::from(deploy) =>
            {
                Some(radius)
            }
            _ => None,
        }
    }

    /// The first field hostile to `faction` that a beam moving from `a` to `b` would cross into
    /// from outside: how far along (0..1), and whose. With `when` (a tick and a fraction of the
    /// next), each field stands where its suit was then (lag compensation).
    pub(super) fn ward_sweep(
        &self,
        a: Vec3,
        b: Vec3,
        faction: Faction,
        when: Option<(u32, f32)>,
    ) -> Option<(f32, usize)> {
        let mut first: Option<(f32, usize)> = None;
        for &j in &self.wards.suit[..usize::from(self.wards.n)] {
            let j = usize::from(j);
            if self.suits.faction[j] == faction {
                continue;
            }
            // (It may have fallen since the list was made.)
            let Some(r) = self.ward(j) else { continue };
            let now = self.suits.flight[j].pos;
            let c = when.and_then(|(t, frac)| self.history.pose_lerp(t, frac, j)).map_or(now, |(p, _)| p);
            if let Some(s) = sphere_entry(a, b, c, r)
                && first.is_none_or(|(f, _)| s < f)
            {
                first = Some((s, j));
            }
        }
        first
    }

    /// A beam carrying `damage` stopped at suit `j`'s field: its energy comes off the suit's, and
    /// the field falls when there's none left.
    pub(super) fn ward_takes(&mut self, j: usize, damage: f32) {
        let SpecialKind::Ward { absorb, .. } = frame(self.suits.frame[j]).special else { return };
        let e = &mut self.suits.energy[j];
        *e -= absorb * damage;
        if *e <= 0.0 {
            *e = 0.0;
            let sp = &mut self.suits.special[j];
            sp.active = false;
            sp.timer = 0;
        }
        let st = &mut self.suits.stats[j];
        st.warded = st.warded.saturating_add(1);
    }
}
