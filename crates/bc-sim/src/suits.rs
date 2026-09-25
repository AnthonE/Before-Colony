//! Suit storage: structure-of-arrays, fixed capacity, generational slots.

use alloc::boxed::Box;
use bc_proto::{CARGO_KINDS, Faction, FrameId, InputCmd, NO_CHUNK, NO_SLOT, Part, PilotKind, WeaponKind};
use glam::{Quat, Vec3};

use crate::ai::AiState;
use crate::content::{ArmSlot, frame};
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
    /// Missiles still to leave in the salvo under way, and ticks until the next.
    pub salvo: u8,
    pub gap: u8,
}

/// A missile lock being built on the suit's designation.
#[derive(Clone, Copy, Debug)]
pub struct LockState {
    /// The suit being locked (`NO_SLOT`: none).
    pub target: u16,
    /// Ticks it has been held (up to the launcher's `lock_ticks`; losing it counts down twice as
    /// fast).
    pub progress: u8,
}

impl Default for LockState {
    fn default() -> Self {
        Self { target: NO_SLOT, progress: 0 }
    }
}

/// Phases of a melee strike (a saber swing, a scythe's reap, the Dragon Fang's thrust).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeleePhase {
    #[default]
    Idle,
    Windup,
    Active,
    Recovery,
}

pub use crate::content::SPECIAL_MOUNT;
/// Weapon slots from here on are the special mounts' (see [`Suits::weapon_state`]).
pub const SPECIAL_SLOTS: usize = 3;
/// Marks a hit by a twin weapon's second blade in [`MeleeState::hits`].
pub const SECOND_BLADE: u16 = 1 << 15;

#[derive(Clone, Copy, Debug)]
pub struct MeleeState {
    pub phase: MeleePhase,
    pub timer: u8,
    /// The weapon striking, and its mount: a loadout slot, or [`SPECIAL_MOUNT`].
    pub weapon: WeaponKind,
    pub slot: u8,
    /// A thrust's direction, suit frame.
    pub dir: Vec3,
    /// Suits already hit this strike, each at most once per blade (a second blade's hits carry
    /// [`SECOND_BLADE`]).
    pub hits: [u16; 4],
    pub n_hits: u8,
    /// Lag-compensation view of the strike (1/16 ticks).
    pub view_q4: u32,
    /// The rock, and the hulk, this strike has struck (each at most once).
    pub rock: Option<u16>,
    pub cut: Option<u16>,
}

impl Default for MeleeState {
    fn default() -> Self {
        Self {
            phase: MeleePhase::Idle,
            timer: 0,
            weapon: WeaponKind::BeamSaber,
            slot: 2,
            dir: Vec3::Z,
            hits: [0; 4],
            n_hits: 0,
            view_q4: 0,
            rock: None,
            cut: None,
        }
    }
}

impl MeleeState {
    /// Windup or the stroke itself (not recovering).
    pub fn striking(&self) -> bool {
        matches!(self.phase, MeleePhase::Windup | MeleePhase::Active)
    }
}

/// A frame's special (see [`SpecialKind`](crate::content::SpecialKind)).
#[derive(Clone, Copy, Debug, Default)]
pub struct SpecialState {
    /// Engaged: the jammer on, Full Open firing.
    pub active: bool,
    /// Ticks left in what's under way: a transformation, Full Open.
    pub timer: u16,
    /// Ticks until the special can be used again.
    pub cooldown: u16,
    /// Ticks of forced overheat left after Full Open.
    pub lockout: u16,
    /// The jammer is broken (by firing, striking) until this tick.
    pub break_until: u32,
}

/// Per-suit combat statistics (for `/status` and the kill feed).
#[derive(Clone, Copy, Debug, Default)]
pub struct SuitStats {
    pub shots: u32,
    pub hits: u32,
    pub kills: u32,
    pub deaths: u32,
    pub damage_dealt: f32,
    /// Hits landed, by weapon class (beam, ballistic, missile, melee, cone).
    pub hits_by_class: [u32; 5],
    /// Specials used: transformations, Full Open Attacks, Cross Crushers, jammer engagements.
    pub specials: u32,
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
    pub melee: Box<[MeleeState]>,
    pub special: Box<[SpecialState]>,
    /// The special mounts' weapons (Full Open's chest gatlings and micro-missiles).
    pub special_weapons: Box<[[WeaponState; 2]]>,
    pub lock: Box<[LockState]>,
    /// Guided missiles tracking the suit (counted each tick).
    pub incoming: Box<[u16]>,
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
    /// The chunk in hand (id, generation) and which hand holds it (the right if true).
    pub held: Box<[(u16, u8, bool)]>,
    /// The hold's contents, kg per ore kind.
    pub cargo_kg: Box<[[u16; CARGO_KINDS]]>,
    /// Credits earned this session (kept across respawns).
    pub credits: Box<[u32]>,
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
            melee: boxed(cap, MeleeState::default()),
            special: boxed(cap, SpecialState::default()),
            special_weapons: boxed(cap, [WeaponState::default(); 2]),
            lock: boxed(cap, LockState::default()),
            incoming: boxed(cap, 0u16),
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
            held: boxed(cap, (NO_CHUNK, 0u8, false)),
            cargo_kg: boxed(cap, [0u16; CARGO_KINDS]),
            credits: boxed(cap, 0u32),
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
        self.credits[idx] = 0;
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
        let mut sw = [WeaponState::default(); 2];
        for (w, m) in sw.iter_mut().zip(spec.special_mounts.iter()) {
            if let Some(m) = m {
                w.ammo = crate::content::weapon(m.weapon).ammo;
            }
        }
        self.special_weapons[idx] = sw;
        self.melee[idx] = MeleeState::default();
        self.special[idx] = SpecialState::default();
        self.lock[idx] = LockState::default();
        self.incoming[idx] = 0;
        self.part_hp[idx] = spec.part_hp;
        self.zero[idx] = ZeroState::default();
        self.respawn_at[idx] = 0;
        self.hulk[idx] = (NO_CHUNK, 0);
        self.held[idx] = (NO_CHUNK, 0, false);
        self.cargo_kg[idx] = [0; CARGO_KINDS];
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

    /// The hand that grabs: the left, unless it's gone.
    pub fn grab_hand(&self, idx: usize) -> Option<bool> {
        let hp = &self.part_hp[idx];
        if hp[Part::ArmL as usize] > 0.0 {
            Some(false)
        } else if hp[Part::ArmR as usize] > 0.0 {
            Some(true)
        } else {
            None
        }
    }

    /// The state of weapon `slot`: a loadout slot (0..3), or a special mount (from
    /// [`SPECIAL_SLOTS`]).
    pub fn weapon_state(&mut self, idx: usize, slot: usize) -> &mut WeaponState {
        match slot.checked_sub(SPECIAL_SLOTS) {
            Some(k) => &mut self.special_weapons[idx][k],
            None => &mut self.weapons[idx][slot],
        }
    }

    /// Whether a mount's weapons can be used: its arm (or other part) is there, and not holding
    /// anything. A two-handed mount needs both arms, and both free.
    pub fn arm_free(&self, idx: usize, arm: ArmSlot) -> bool {
        let (chunk, _, right) = self.held[idx];
        let holding = chunk != NO_CHUNK;
        let busy = holding
            && match arm {
                ArmSlot::Left => !right,
                ArmSlot::Right | ArmSlot::Nose => right,
                ArmSlot::Both => true,
                ArmSlot::Shoulder
                | ArmSlot::Head
                | ArmSlot::Pods
                | ArmSlot::Chest
                | ArmSlot::LegPods
                | ArmSlot::NoseGuns => false,
            };
        let both = arm != ArmSlot::Both || self.part_hp[idx][Part::ArmL as usize] > 0.0;
        self.part_hp[idx][arm.part() as usize] > 0.0 && both && !busy
    }

    /// What's in the hold, kg.
    pub fn cargo_total_kg(&self, idx: usize) -> u32 {
        self.cargo_kg[idx].iter().map(|kg| u32::from(*kg)).sum()
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
