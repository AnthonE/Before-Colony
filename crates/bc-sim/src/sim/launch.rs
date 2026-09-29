//! Survival: suits their pilots built, launched from the colony's docking hub and brought home
//! again. (Under arcade rules, [`Sim::join`](super::Sim::join) hands out any frame at its
//! faction's spawn point instead.)
//!
//! - [`Sim::launch`](super::Sim::launch) puts a [`Loadout`] in the sector at the docking hub's
//!   mouth: the parts its pilot fitted (as worn as they are), the weapons fitted on its mounts,
//!   the rounds loaded and the propellant in the tank.
//! - [`Sim::dock`](super::Sim::dock) takes a suit that has come to rest in the dock out of the
//!   sector again, and says what it brings home ([`Homecoming`]): what's left of it, its hold,
//!   whatever it has in hand, and the bounties it earned.

use bc_proto::{CARGO_KINDS, ChunkDesc, Faction, FrameId, NO_CHUNK, Part, PilotKind};
use glam::{Quat, Vec3};

use super::Sim;
use crate::content::salvage::DOCK_HUB_LENGTH;
use crate::content::{frame, weapon};
use crate::handle::SuitId;
use crate::math::{cos, look_rotation, sin};
use crate::suits::ALL_MOUNTS;
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH};

/// Where suits come out: on the docking hub's axis, just off its mouth, inside the dock (a suit
/// that launches can turn round and dock again).
pub const LAUNCH_GATE: Vec3 = Vec3::new(
    COLONY_CENTER.x - COLONY_HALF_LENGTH - DOCK_HUB_LENGTH - 80.0,
    COLONY_CENTER.y,
    COLONY_CENTER.z,
);
/// How fast a suit leaves the hub, m/s (outward, along −X).
pub const LAUNCH_SPEED: f32 = 12.0;
/// Launches are spread round the axis on a ring this wide, m, a place apart each.
const LAUNCH_RING: f32 = 100.0;
const LAUNCH_PLACES: u32 = 12;

/// Where the `n`th launch comes out, facing out of the hub.
fn launch_pose(n: u32) -> (Vec3, Quat) {
    let a = (n % LAUNCH_PLACES) as f32 * core::f32::consts::TAU / LAUNCH_PLACES as f32;
    let pos = LAUNCH_GATE + Vec3::new(0.0, cos(a) * LAUNCH_RING, sin(a) * LAUNCH_RING);
    (pos, look_rotation(-Vec3::X, Vec3::Y))
}

/// A suit as its pilot built it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loadout {
    /// Armour per part, as a fraction of the frame's (0: not fitted). The torso is the suit, so
    /// it must be fitted.
    pub parts: [f32; Part::COUNT],
    /// A bit per loadout slot (primary, secondary, melee): its weapon is fitted.
    pub mounts: u8,
    /// Rounds loaded, per loadout slot.
    pub ammo: [u16; 3],
    /// In the tank, kg.
    pub propellant: f32,
}

impl Loadout {
    /// Everything fitted, new, loaded and full (tests, and arcade frames).
    pub fn full(frame_id: FrameId) -> Self {
        let spec = crate::content::frame(frame_id);
        let mut ammo = [0; 3];
        for (a, m) in ammo.iter_mut().zip(spec.loadout.iter()) {
            if let Some(m) = m {
                *a = crate::content::weapon(m.weapon).ammo;
            }
        }
        Self { parts: [1.0; Part::COUNT], mounts: 0b111, ammo, propellant: spec.propellant_cap }
    }
}

/// What a suit brings home when it docks, or what its pilot gets when it's lost (the bounty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homecoming {
    /// The frame line (Neo-Bird comes home as Wing Zero).
    pub frame: FrameId,
    /// Armour per part as a fraction of the frame's; 0: gone (shot off, or never fitted).
    pub parts: [f32; Part::COUNT],
    pub mounts: u8,
    pub ammo: [u16; 3],
    pub propellant: f32,
    /// The hold, kg per cargo kind.
    pub cargo_kg: [u16; CARGO_KINDS],
    /// Whatever it had in hand (a hulk it towed in, a limb, ore).
    pub held: Option<ChunkDesc>,
    /// Credits the colony pays for the Mobile Dolls it destroyed.
    pub bounty: u32,
}

impl Sim {
    /// Launches `loadout` from the docking hub, flown by `pilot`. `None` if the sector is full, the
    /// frame isn't one pilots fly, or there's no torso.
    pub fn launch(
        &mut self,
        frame_id: FrameId,
        faction: Faction,
        pilot: PilotKind,
        loadout: &Loadout,
    ) -> Option<SuitId> {
        let spec = frame(frame_id);
        if !spec.playable || loadout.parts[Part::Torso as usize] <= 0.0 {
            return None;
        }
        let id = self.suits.allocate(frame_id, faction, pilot)?;
        self.spawn_counter += 1;
        let (pos, rot) = launch_pose(self.spawn_counter);
        let i = id.idx();
        self.suits.place(i, frame_id, pos, rot, self.tick);
        for (hp, (max, f)) in self.suits.part_hp[i].iter_mut().zip(spec.part_hp.iter().zip(loadout.parts)) {
            *hp = max * f.clamp(0.0, 1.0);
        }
        self.suits.mounts[i] = loadout.mounts & ALL_MOUNTS;
        for (slot, ws) in self.suits.weapons[i].iter_mut().enumerate() {
            if let Some(m) = spec.loadout[slot] {
                ws.ammo = loadout.ammo[slot].min(weapon(m.weapon).ammo);
            }
        }
        let f = &mut self.suits.flight[i];
        f.propellant = loadout.propellant.clamp(0.0, spec.propellant_cap);
        f.vel = rot * Vec3::Z * LAUNCH_SPEED;
        Some(id)
    }

    /// Takes suit `id` into the hangar: it must be alive, awake and at rest in the dock. What it
    /// brings home; it's gone from the sector.
    pub fn dock(&mut self, id: SuitId) -> Option<Homecoming> {
        if !self.suits.valid(id) {
            return None;
        }
        let i = id.idx();
        if !self.suits.alive.get(i) || self.suits.sleeping.get(i) || !self.docked(i) {
            return None;
        }
        let held = self.held_chunk(i);
        let s = &self.suits;
        let home = Homecoming {
            frame: match s.frame[i] {
                FrameId::WingZeroBird => FrameId::WingZero,
                f => f,
            },
            parts: s.part_fractions(i),
            mounts: s.mounts[i],
            ammo: [s.weapons[i][0].ammo, s.weapons[i][1].ammo, s.weapons[i][2].ammo],
            propellant: s.flight[i].propellant,
            cargo_kg: s.cargo_kg[i],
            held: held.map(|k| self.chunks.desc[k]),
            bounty: s.credits[i],
        };
        if let Some(k) = held {
            self.chunks.kill(k);
        }
        self.suits.held[i] = (NO_CHUNK, 0, false);
        self.suits.cargo_kg[i] = [0; CARGO_KINDS];
        self.suits.release(i);
        Some(home)
    }
}
