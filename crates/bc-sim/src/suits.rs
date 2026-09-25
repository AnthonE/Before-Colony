//! Suit storage: structure-of-arrays, fixed capacity, generational slots.

use alloc::boxed::Box;
use bc_proto::{Faction, FrameId, InputCmd, NO_CHUNK, NO_SLOT, Part, PilotKind};
use glam::{Quat, Vec3};

use crate::ai::AiState;
use crate::content::frame;
use crate::flight::FlightState;
use crate::handle::{Handle, SuitId};
use crate::storage::{BitSet, FreeList, boxed};
use crate::zero::ZeroState;

/// Per-weapon runtime state.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeaponState {
    /// Ticks until it can fire again.
    pub cooldown: u16,
    /// Rounds left (ammo weapons).
    pub ammo: u16,
    /// Ticks spent charging (Twin Buster Rifle), 0 = not charging.
    pub charge: u16,
}

/// Beam saber swing phases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaberPhase {
    #[default]
    Idle,
    Windup,
    Active,
    Recovery,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SaberState {
    pub phase: SaberPhase,
    pub timer: u8,
    /// Suits already hit this swing (each at most once).
    pub hits: [u16; 4],
    pub n_hits: u8,
    /// Lag-compensation view of the swing (1/16 ticks).
    pub view_q4: u32,
}

/// Per-suit combat statistics (for `/status` and the kill feed).
#[derive(Clone, Copy, Debug, Default)]
pub struct SuitStats {
    pub shots: u32,
    pub hits: u32,
    pub kills: u32,
    pub deaths: u32,
    pub damage_dealt: f32,
}

pub struct Suits {
    pub cap: usize,
    pub generation: Box<[u16]>,
    /// Slot occupied (alive or a wreck / waiting to respawn).
    pub used: BitSet,
    /// Alive and flying.
    pub alive: BitSet,
    pub frame: Box<[FrameId]>,
    pub faction: Box<[Faction]>,
    pub pilot: Box<[PilotKind]>,
    pub flight: Box<[FlightState]>,
    pub aim: Box<[Vec3]>,
    pub input: Box<[InputCmd]>,
    pub prev_buttons: Box<[u16]>,
    pub boosting: Box<[bool]>,
    pub heat: Box<[f32]>,
    pub overheated: Box<[bool]>,
    pub energy: Box<[f32]>,
    pub weapons: Box<[[WeaponState; 3]]>,
    pub saber: Box<[SaberState]>,
    pub part_hp: Box<[[f32; Part::COUNT]]>,
    pub zero: Box<[ZeroState]>,
    pub ai: Box<[AiState]>,
    /// Tick the suit last fired (signature bloom, "firing" flags).
    pub last_fired: Box<[u32]>,
    /// Tick of the last primary/secondary shot, for replication flags.
    pub fired_primary: Box<[u32]>,
    pub fired_secondary: Box<[u32]>,
    /// While a wreck: tick it disappears (dolls) or respawns (pilots).
    pub respawn_at: Box<[u32]>,
    /// Frame to respawn in.
    pub respawn_frame: Box<[FrameId]>,
    pub stats: Box<[SuitStats]>,
    /// The hulk a dead suit became, and its generation: the wreck moves as the hulk does.
    pub hulk: Box<[(u16, u8)]>,
    free: FreeList,
}

impl Suits {
    pub fn new(cap: usize) -> Self {
        let cap = cap.min(NO_SLOT as usize);
        Self {
            cap,
            generation: boxed(cap, 0u16),
            used: BitSet::new(cap),
            alive: BitSet::new(cap),
            frame: boxed(cap, FrameId::Leo),
            faction: boxed(cap, Faction::Oz),
            pilot: boxed(cap, PilotKind::Human),
            flight: boxed(cap, FlightState::default()),
            aim: boxed(cap, Vec3::Z),
            input: boxed(cap, InputCmd::default()),
            prev_buttons: boxed(cap, 0u16),
            boosting: boxed(cap, false),
            heat: boxed(cap, 0.0f32),
            overheated: boxed(cap, false),
            energy: boxed(cap, 0.0f32),
            weapons: boxed(cap, [WeaponState::default(); 3]),
            saber: boxed(cap, SaberState::default()),
            part_hp: boxed(cap, [0.0f32; Part::COUNT]),
            zero: boxed(cap, ZeroState::default()),
            ai: boxed(cap, AiState::default()),
            last_fired: boxed(cap, 0u32),
            fired_primary: boxed(cap, 0u32),
            fired_secondary: boxed(cap, 0u32),
            respawn_at: boxed(cap, 0u32),
            respawn_frame: boxed(cap, FrameId::Leo),
            stats: boxed(cap, SuitStats::default()),
            hulk: boxed(cap, (NO_CHUNK, 0u8)),
            free: FreeList::full(cap),
        }
    }

    /// Claims a slot. Returns `None` when full.
    pub fn allocate(&mut self, frame_id: FrameId, faction: Faction, pilot: PilotKind) -> Option<SuitId> {
        let idx = self.free.pop()? as usize;
        self.generation[idx] = self.generation[idx].wrapping_add(1).max(1);
        self.used.set(idx, true);
        self.alive.set(idx, false);
        self.frame[idx] = frame_id;
        self.respawn_frame[idx] = frame_id;
        self.faction[idx] = faction;
        self.pilot[idx] = pilot;
        self.stats[idx] = SuitStats::default();
        self.ai[idx] = AiState::default();
        Some(SuitId(Handle { idx: idx as u16, generation: self.generation[idx] }))
    }

    /// Puts a (re)spawned suit in the world with a full tank and pristine armour.
    pub fn place(&mut self, idx: usize, frame_id: FrameId, pos: Vec3, rot: Quat, tick: u32) {
        let spec = frame(frame_id);
        self.frame[idx] = frame_id;
        self.alive.set(idx, true);
        self.flight[idx] =
            FlightState { pos, rot, propellant: spec.propellant_cap, ..FlightState::default() };
        self.aim[idx] = rot * Vec3::Z;
        self.input[idx] =
            InputCmd { tick, view_tick_q4: tick << 4, aim: rot * Vec3::Z, ..InputCmd::default() };
        self.prev_buttons[idx] = 0;
        self.heat[idx] = 0.0;
        self.overheated[idx] = false;
        self.energy[idx] = spec.energy_cap;
        let mut ws = [WeaponState::default(); 3];
        for (w, m) in ws.iter_mut().zip(spec.loadout.iter()) {
            if let Some(m) = m {
                w.ammo = crate::content::weapon(m.weapon).ammo;
            }
        }
        self.weapons[idx] = ws;
        self.saber[idx] = SaberState::default();
        self.part_hp[idx] = spec.part_hp;
        self.zero[idx] = ZeroState::default();
        self.respawn_at[idx] = 0;
        self.hulk[idx] = (NO_CHUNK, 0);
    }

    /// Frees a slot entirely (disconnect, or a Mobile Doll wreck clearing).
    pub fn release(&mut self, idx: usize) {
        if self.used.get(idx) {
            self.used.set(idx, false);
            self.alive.set(idx, false);
            self.generation[idx] = self.generation[idx].wrapping_add(1).max(1);
            self.free.push(idx as u16);
        }
    }

    #[inline]
    pub fn is_alive(&self, idx: usize) -> bool {
        idx < self.cap && self.alive.get(idx)
    }

    #[inline]
    pub fn valid(&self, id: SuitId) -> bool {
        let i = id.idx();
        i < self.cap && self.used.get(i) && self.generation[i] == id.0.generation
    }

    pub fn free_slots(&self) -> usize {
        self.free.len()
    }

    /// Torso armour fraction (how close to destruction).
    pub fn hull_fraction(&self, idx: usize) -> f32 {
        let spec = frame(self.frame[idx]);
        (self.part_hp[idx][Part::Torso as usize] / spec.part_hp[Part::Torso as usize]).clamp(0.0, 1.0)
    }

    /// Parts shot off (a bit per [`Part`]; the torso is the suit, so never).
    pub fn gone_mask(&self, idx: usize) -> u8 {
        let mut m = 0;
        for p in crate::content::salvage::DETACHABLE {
            if self.part_hp[idx][p as usize] <= 0.0 {
                m |= 1 << p as u8;
            }
        }
        m
    }

    /// Armour fraction per part.
    pub fn part_fractions(&self, idx: usize) -> [f32; Part::COUNT] {
        let spec = frame(self.frame[idx]);
        let mut out = [0.0; Part::COUNT];
        for (i, o) in out.iter_mut().enumerate() {
            *o = (self.part_hp[idx][i] / spec.part_hp[i]).clamp(0.0, 1.0);
        }
        out
    }
}
