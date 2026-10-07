//! Losing a suit the way the mech games teach it (`docs/PEERS.md`, "Mech games"): Titanfall's
//! Titans are doomed before they're destroyed and their pilots eject, Steel Battalion makes
//! ejecting the pilot's own job, and Heero blows up his Gundam rather than leave it to anyone.
//!
//! - **Doom.** A pilot's suit whose torso is breached isn't lost at once: for [`DOOM_TICKS`] it
//!   flies and fights on while its reactor goes. Every blow that lands meanwhile takes time off it
//!   ([`DOOM_PER_TORSO`]: a blow of a sixth of the torso's armour, a second). Then it's destroyed
//!   as any suit is, credited to whoever breached it. A Mobile Doll has no pilot to save and a
//!   sleeper's can't wake in time: theirs go at once.
//! - **Eject** ([`Sim::eject`]). The pilot leaves, doomed or not (never inside the colony, where
//!   nothing strikes a suit): the suit is destroyed at once and their capsule is thrown clear
//!   ([`Event::Eject`]). Its hulk is the pilot's claim, which the sector's tugs bring home
//!   (`bc_sector`), its torso whole if it wasn't doomed.
//! - **Self-destruct.** Doomed, the pilot can blow the reactor with themselves aboard: hostile suits
//!   within the blast's reach ([`WeaponKind::Reactor`]) take its damage to the torso, less with
//!   distance, and nothing of the suit is left to salvage ([`Event::Blast`]).

use bc_proto::events::Event;
use bc_proto::{NO_CHUNK, NO_SLOT, Part, PilotKind, WeaponKind};
use glam::Vec3;

use super::Sim;
use crate::config::secs;
use crate::content::{frame, weapon};
use crate::handle::SuitId;
use crate::math::{length, normalize_or};

/// How long a doomed suit lasts, ticks, unless blows cut it short.
pub const DOOM_TICKS: u16 = secs(3.0) as u16;
/// Ticks a blow takes off a doom per whole torso's armour it carries: a sixth of the torso, a
/// second.
pub const DOOM_PER_TORSO: f32 = 180.0;
/// How fast the pilot's capsule leaves the suit, m/s: up out of it, and a little back.
pub const EJECT_SPEED: f32 = 25.0;

/// A suit's doom: ticks until its reactor goes (0: it isn't doomed), and who breached it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Doom {
    pub left: u16,
    pub by: u16,
}

impl Doom {
    pub const NONE: Doom = Doom { left: 0, by: NO_SLOT };
}

impl Default for Doom {
    fn default() -> Self {
        Self::NONE
    }
}

/// What a pilot leaving their suit ([`Sim::eject`]) left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ejected {
    /// The pilot is out. The suit's wreck is the hulk chunk `hulk` of `generation` ([`NO_CHUNK`]
    /// if the sector had no room for one); `torso`: it wasn't doomed, so its torso is whole.
    Out { hulk: u16, generation: u8, torso: bool },
    /// The pilot blew the reactor, aboard: nothing is left.
    Blown,
}

impl Sim {
    /// Whether suit `i` is doomed.
    #[inline]
    pub fn doomed(&self, i: usize) -> bool {
        self.suits.doom[i].left > 0
    }

    /// Whether a breach dooms suit `i` rather than destroying it: someone is aboard, awake.
    pub(super) fn doomable(&self, i: usize) -> bool {
        self.suits.pilot[i] != PilotKind::MobileDoll && !self.suits.sleeping.get(i)
    }

    /// A blow of `amount` (armour points, after armour) on suit `j`'s breached torso, from `shooter`:
    /// it dooms the suit, cuts its doom short, or (no one aboard to save) destroys it.
    pub(super) fn breach(&mut self, j: usize, amount: f32, shooter: u16, t: u32) {
        let doom = self.suits.doom[j];
        if doom.left > 0 {
            let torso = frame(self.suits.frame[j]).part_hp[Part::Torso as usize].max(1.0);
            let cut = (amount / torso * DOOM_PER_TORSO) as u16;
            let left = doom.left.saturating_sub(cut);
            self.suits.doom[j].left = left;
            if left == 0 {
                self.destroy(j, doom.by, t, true);
            }
        } else if self.doomable(j) {
            self.suits.doom[j] = Doom { left: DOOM_TICKS, by: shooter };
            self.events.push(Event::Doomed { id: 0, tick: t, suit: j as u16 });
        } else {
            self.destroy(j, shooter, t, true);
        }
    }

    /// Doomed suits' reactors run down; each is destroyed when its doom is up.
    pub(super) fn doom_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let d = self.suits.doom[i];
            if d.left == 0 {
                continue;
            }
            self.suits.doom[i].left = d.left - 1;
            if d.left == 1 {
                self.destroy(i, d.by, t, true);
            }
        }
        self.iter_bits = alive;
    }

    /// The pilot of suit `id` leaves it: they eject (`destruct` false), or, doomed, blow its
    /// reactor with themselves aboard. `None` if they can't: no such suit alive, nobody awake aboard,
    /// inside the colony (nothing strikes a suit there), or a self-destruct not doomed.
    pub fn eject(&mut self, id: SuitId, destruct: bool) -> Option<Ejected> {
        if !self.suits.valid(id) {
            return None;
        }
        let i = id.idx();
        if !self.suits.alive.get(i) || !self.doomable(i) || self.interior() {
            return None;
        }
        let t = self.tick;
        let doom = self.suits.doom[i];
        if destruct {
            if doom.left == 0 {
                return None;
            }
            self.blast(i, t);
            self.destroy(i, doom.by, t, false);
            return Some(Ejected::Blown);
        }
        let f = self.suits.flight[i];
        let top = frame(self.suits.frame[i]).capsules[Part::Torso as usize];
        let (up, back) = (f.rot * Vec3::Y, f.rot * -Vec3::Z);
        let pos = f.pos + f.rot * (top.a + top.b) * 0.5 + up * 2.0;
        let vel = f.vel + normalize_or(up * 4.0 + back, up) * EJECT_SPEED;
        self.events.push(Event::Eject { id: 0, tick: t, suit: i as u16, pos, vel });
        let by = if doom.left > 0 { doom.by } else { NO_SLOT };
        let hulk = self.destroy(i, by, t, true);
        let generation = if hulk == NO_CHUNK { 0 } else { self.chunks.generation[usize::from(hulk)] };
        Some(Ejected::Out { hulk, generation, torso: doom.left == 0 })
    }

    /// Suit `i`'s reactor blows: every hostile suit within the blast's reach takes its damage to the
    /// torso, falling off to nothing at the edge (measured from the blast to the suit's own bounds).
    fn blast(&mut self, i: usize, t: u32) {
        let at = self.suits.flight[i].pos;
        self.events.push(Event::Blast { id: 0, tick: t, suit: i as u16, pos: at });
        let w = weapon(WeaponKind::Reactor);
        let faction = self.suits.faction[i];
        let mut near = core::mem::take(&mut self.query_bits);
        near.clear();
        // (Suits' bounds are well under 20 m.)
        self.spatial.query_sphere(at, w.range + 20.0, |j| near.set(j, true));
        for j in near.iter() {
            let s = &self.suits;
            if j == i || !s.alive.get(j) || (!self.cfg.friendly_fire && s.faction[j] == faction) {
                continue;
            }
            let to = s.flight[j].pos - at;
            let gap = (length(to) - frame(s.frame[j]).radius).max(0.0);
            if gap >= w.range {
                continue;
            }
            let amount = w.damage * (1.0 - gap / w.range);
            self.queue_damage(j, Part::Torso, amount, i, WeaponKind::Reactor, normalize_or(to, Vec3::Y));
        }
        self.query_bits = near;
    }
}
