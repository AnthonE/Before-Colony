//! The sector simulation: fixed-capacity world state and the per-tick pipeline.
//!
//! [`Sim::step`] never allocates and never blocks. Every buffer it touches was sized in
//! [`Sim::new`]; the `no_alloc` test counts heap operations across thousands of ticks.
//!
//! Pipeline for tick `T`:
//! 1. Mobile Doll AI (and ZERO seizures) write `InputCmd`s (dolls re-plan every 3rd tick, staggered).
//! 2. Flight: AMBAC/RCS attitude, thrust, propellant, G-strain; wrecks drift.
//! 3. Spatial hash rebuild, then lag-compensation history is recorded (`history[T]` = snapshot `T`).
//! 4. Weapons fire: projectiles spawn and catch up through the history (≤ 8 ticks) for shots fired
//!    by humans/agents; beams emit spawn events.
//! 5. Projectiles sweep against per-part capsules; sabers sweep their arcs.
//! 6. Damage resolves in generation order; parts break; suits die.
//! 7. Heat, energy, ZERO strain, respawns; staggered ZERO rollouts.

use alloc::boxed::Box;

mod combat;
mod wire;
mod zero;

use bc_proto::buttons::ZERO;
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, Part, PilotKind, WeaponKind};
use glam::{Quat, Vec3};

use crate::ai::{self, DOLL, SEIZED};
use crate::config::{DT, SimConfig, secs};
use crate::content::{frame, weapon};
use crate::events::EventRing;
use crate::flight::{self, FlightMods};
use crate::handle::SuitId;
use crate::lagcomp::History;
use crate::math::{Rng, length, look_rotation, normalize_or};
use crate::perception::{Contact, Perception, SelfView};
use crate::projectiles::Projectiles;
use crate::sensors;
use crate::spatial::SpatialHash;
use crate::storage::{BitSet, FixedVec, boxed};
use crate::suits::{SaberPhase, Suits};
use crate::zero::TacticalAdvice;
use crate::zero::strain::StrainEvent;

/// A pending hit, applied in the damage phase.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DamageEvent {
    pub target: u16,
    pub part: Part,
    pub amount: f32,
    pub shooter: u16,
    pub weapon: WeaponKind,
}

const MAX_SQUADS: usize = 32;
/// cos(3°): "aiming at me" threshold.
const COS_3_DEG: f32 = 0.998_629_5;
const SQUAD_SIZE: u8 = 4;

#[derive(Clone, Copy, Debug)]
struct Squad {
    anchor: Vec3,
    focus: u16,
}

/// Patrol anchors for Mobile Doll squads: a ring above the colony.
const ANCHORS: [Vec3; 8] = [
    Vec3::new(2_800.0, 900.0, 0.0),
    Vec3::new(1_980.0, 1_300.0, 1_980.0),
    Vec3::new(0.0, 700.0, 2_800.0),
    Vec3::new(-1_980.0, 1_500.0, 1_980.0),
    Vec3::new(-2_800.0, 1_000.0, 0.0),
    Vec3::new(-1_980.0, 600.0, -1_980.0),
    Vec3::new(0.0, 1_400.0, -2_800.0),
    Vec3::new(1_980.0, 800.0, -1_980.0),
];

/// Faction spawn points (players and agents).
fn spawn_point(faction: Faction, n: u32) -> (Vec3, Quat) {
    let base = match faction {
        Faction::Colonies => Vec3::new(-4_200.0, 700.0, -3_200.0),
        Faction::Alliance => Vec3::new(4_200.0, 700.0, -3_200.0),
        Faction::Oz => Vec3::new(0.0, 2_200.0, 4_200.0),
    };
    let k = (n % 16) as f32;
    let offset = Vec3::new((k % 4.0) * 60.0 - 90.0, crate::math::floor(k / 4.0) * 40.0, 0.0);
    let pos = base + offset;
    (pos, look_rotation(Vec3::new(0.0, 900.0, 0.0) - pos, Vec3::Y))
}

pub struct Sim {
    pub cfg: SimConfig,
    tick: u32,
    pub suits: Suits,
    pub projectiles: Projectiles,
    pub history: History,
    spatial: SpatialHash,
    pub events: EventRing,
    damage: FixedVec<DamageEvent>,
    squads: [Squad; MAX_SQUADS],
    advice: Box<[TacticalAdvice]>,
    scratch: Perception,
    next_doll_spawn: u32,
    next_squad: usize,
    spawn_counter: u32,
    rng: Rng,
    /// Live-projectile high-water mark (diagnostics).
    pub peak_projectiles: usize,
    /// Suits alive after the last tick.
    alive_count: usize,
    /// Scratch: a copy of a membership set to iterate while mutating suits.
    iter_bits: BitSet,
    /// Scratch: spatial query results inside `perceive_into`.
    query_bits: BitSet,
    /// Scratch: live projectiles to iterate while killing some.
    proj_bits: BitSet,
}

impl Sim {
    /// Allocates all storage. Nothing grows afterwards.
    pub fn new(cfg: SimConfig) -> Self {
        let cap = cfg.max_suits.min(NO_SLOT as usize);
        Self {
            cfg,
            tick: 0,
            suits: Suits::new(cap),
            projectiles: Projectiles::new(cfg.max_projectiles),
            history: History::new(cap),
            spatial: SpatialHash::new(cap),
            events: EventRing::new(cfg.max_events),
            damage: FixedVec::new(
                2_048,
                DamageEvent {
                    target: 0,
                    part: Part::Torso,
                    amount: 0.0,
                    shooter: 0,
                    weapon: WeaponKind::BeamRifle,
                },
            ),
            squads: [Squad { anchor: Vec3::ZERO, focus: NO_SLOT }; MAX_SQUADS],
            advice: boxed(cap, TacticalAdvice::default()),
            scratch: Perception::default(),
            next_doll_spawn: 0,
            next_squad: 0,
            spawn_counter: 0,
            rng: Rng::new(cfg.seed),
            peak_projectiles: 0,
            alive_count: 0,
            iter_bits: BitSet::new(cap),
            query_bits: BitSet::new(cap),
            proj_bits: BitSet::new(cfg.max_projectiles),
        }
    }

    /// The last completed tick.
    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The tick the next [`step`](Self::step) will simulate (inputs should target it).
    #[inline]
    pub fn next_tick(&self) -> u32 {
        self.tick + 1
    }

    pub fn alive_count(&self) -> usize {
        self.alive_count
    }

    /// Adds a player or agent suit at its faction's spawn point.
    pub fn join(&mut self, frame_id: FrameId, faction: Faction, pilot: PilotKind) -> Option<SuitId> {
        let id = self.suits.allocate(frame_id, faction, pilot)?;
        self.spawn_counter += 1;
        let (pos, rot) = spawn_point(faction, self.spawn_counter);
        self.suits.place(id.idx(), frame_id, pos, rot, self.tick);
        Some(id)
    }

    /// Adds a suit at an explicit pose (tests, scenarios).
    pub fn spawn_at(
        &mut self,
        frame_id: FrameId,
        faction: Faction,
        pilot: PilotKind,
        pos: Vec3,
        rot: Quat,
    ) -> Option<SuitId> {
        let id = self.suits.allocate(frame_id, faction, pilot)?;
        self.suits.place(id.idx(), frame_id, pos, rot, self.tick);
        if pilot == PilotKind::MobileDoll {
            let i = id.idx();
            self.suits.ai[i].anchor = pos;
            self.suits.ai[i].rng = self.rng.next_u32() | 1;
            self.suits.ai[i].think_at = self.tick + (i as u32 % self.cfg.doll_think_interval.max(1));
        }
        Some(id)
    }

    /// Removes a suit (disconnect).
    pub fn leave(&mut self, id: SuitId) {
        if self.suits.valid(id) {
            self.suits.release(id.idx());
        }
    }

    /// Sets the command a player's suit will use next tick.
    pub fn set_input(&mut self, id: SuitId, cmd: InputCmd) {
        if self.suits.valid(id) {
            self.suits.input[id.idx()] = cmd;
        }
    }

    /// Asks for the next respawn to use `frame_id`.
    pub fn set_respawn_frame(&mut self, id: SuitId, frame_id: FrameId) {
        if self.suits.valid(id) {
            self.suits.respawn_frame[id.idx()] = frame_id;
        }
    }

    /// Stores external oracle advice for its pilot.
    pub fn apply_advice(&mut self, advice: &TacticalAdvice) {
        let i = advice.pilot as usize;
        if i < self.suits.cap && self.suits.alive.get(i) {
            self.advice[i] = *advice;
        }
    }

    /// Advances one tick.
    pub fn step(&mut self) {
        self.tick += 1;
        let t = self.tick;
        self.damage.clear();
        self.spawn_dolls(t);
        if t.is_multiple_of(30) {
            self.squad_logic();
        }
        self.ai_step(t);
        self.flight_step(t);
        self.spatial_rebuild();
        self.record_history(t);
        self.weapons_step(t);
        self.projectile_step(t);
        self.melee_step(t);
        self.damage_step(t);
        self.status_step(t);
        self.zero_step(t);
        self.peak_projectiles = self.peak_projectiles.max(self.projectiles.count());
    }

    // ---------------------------------------------------------------------------------------------
    // Spawning
    // ---------------------------------------------------------------------------------------------

    fn doll_count(&mut self) -> u32 {
        let mut n = 0;
        for i in self.suits.used.iter() {
            if self.suits.pilot[i] == PilotKind::MobileDoll {
                n += 1;
            }
        }
        n
    }

    fn spawn_dolls(&mut self, t: u32) {
        if t < self.next_doll_spawn || self.cfg.target_dolls == 0 {
            return;
        }
        self.next_doll_spawn = t + secs(4.0);
        let have = self.doll_count();
        if have >= self.cfg.target_dolls {
            return;
        }
        let squad = self.next_squad % MAX_SQUADS;
        self.next_squad += 1;
        let anchor =
            ANCHORS[squad % ANCHORS.len()] + Vec3::new(0.0, (squad / ANCHORS.len()) as f32 * 350.0, 0.0);
        self.squads[squad] = Squad { anchor, focus: NO_SLOT };
        let n = (self.cfg.target_dolls - have).min(u32::from(SQUAD_SIZE));
        for k in 0..n {
            let frame_id = if k == 3 { FrameId::Virgo } else { FrameId::Taurus };
            let pos = anchor + Vec3::new((k as f32 - 1.5) * 90.0, (k % 2) as f32 * 40.0, 0.0);
            let rot = look_rotation(-anchor, Vec3::Y);
            if let Some(id) = self.spawn_at(frame_id, Faction::Oz, PilotKind::MobileDoll, pos, rot) {
                self.suits.ai[id.idx()].squad = squad as u8;
            }
        }
    }

    /// Every second: each squad focuses the hostile nearest its anchor.
    fn squad_logic(&mut self) {
        for (s, squad) in self.squads.iter_mut().enumerate() {
            let mut best = (f32::MAX, NO_SLOT);
            for j in self.suits.alive.iter() {
                if self.suits.faction[j] == Faction::Oz {
                    continue;
                }
                let d = length(self.suits.flight[j].pos - squad.anchor);
                if d < 8_000.0 && d < best.0 {
                    best = (d, j as u16);
                }
            }
            squad.focus = best.1;
            let _ = s;
        }
        for i in self.suits.alive.iter() {
            if self.suits.pilot[i] == PilotKind::MobileDoll {
                let sq = self.suits.ai[i].squad as usize % MAX_SQUADS;
                self.suits.ai[i].order_target = self.squads[sq].focus;
                self.suits.ai[i].anchor = self.squads[sq].anchor;
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // Perception
    // ---------------------------------------------------------------------------------------------

    /// Builds what suit `i` perceives into `out`.
    pub fn perceive_into(&mut self, i: usize, out: &mut Perception) {
        let me = self.self_view(i);
        out.reset(me);
        let spec = frame(self.suits.frame[i]);
        let head_ok = self.suits.part_hp[i][Part::Head as usize] > 0.0;
        let range = spec.sensor_range * if head_ok { 1.0 } else { 0.4 };
        let t = self.tick;
        let mut found = core::mem::take(&mut self.query_bits);
        found.clear();
        self.spatial.query_sphere(me.pos, range * 1.3, |j| found.set(j, true));
        for j in found.iter() {
            if j == i || !self.suits.alive.get(j) {
                continue;
            }
            // Cheap sensor test first; only detected suits get a full contact built.
            let sig = sensors::signature(
                frame(self.suits.frame[j]).signature,
                self.suits.boosting[j],
                t.saturating_sub(self.suits.last_fired[j]) < 30,
                false,
            );
            let pos = self.suits.flight[j].pos;
            if sensors::detects(me.pos, range, pos, sig) && out.would_keep(length(pos - me.pos)) {
                out.offer(self.contact_of(j, i, t));
            }
        }
        self.query_bits = found;
    }

    pub(crate) fn self_view(&self, i: usize) -> SelfView {
        let s = &self.suits;
        let spec = frame(s.frame[i]);
        let f = &s.flight[i];
        let mut ready = [false; 3];
        for (slot, r) in ready.iter_mut().enumerate() {
            if let Some(m) = spec.loadout[slot] {
                let w = weapon(m.weapon);
                let ws = &s.weapons[i][slot];
                *r = ws.cooldown == 0
                    && s.energy[i] >= w.energy
                    && (w.ammo == 0 || ws.ammo > 0)
                    && s.part_hp[i][m.arm.part() as usize] > 0.0;
            }
        }
        SelfView {
            slot: i as u16,
            frame: s.frame[i],
            faction: s.faction[i],
            pos: f.pos,
            vel: f.vel,
            rot: f.rot,
            aim: s.aim[i],
            parts: s.part_fractions(i),
            heat: s.heat[i] / spec.heat_cap,
            energy: s.energy[i] / spec.energy_cap,
            propellant: f.propellant / spec.propellant_cap,
            g_strain: f.g_strain,
            ready,
            overheated: s.overheated[i],
        }
    }

    /// Suit `j` as seen by observer `i`.
    pub(crate) fn contact_of(&self, j: usize, i: usize, t: u32) -> Contact {
        let s = &self.suits;
        let me = s.flight[i].pos;
        let f = &s.flight[j];
        let to_me = normalize_or(me - f.pos, Vec3::Z);
        let accel = self.history.accel_estimate(t, j, DT).unwrap_or(Vec3::ZERO);
        Contact {
            slot: j as u16,
            frame: s.frame[j],
            faction: s.faction[j],
            pilot: s.pilot[j],
            pos: f.pos,
            vel: f.vel,
            rot: f.rot,
            aim: s.aim[j],
            accel,
            hull: s.hull_fraction(j),
            dist: length(f.pos - me),
            firing: t.saturating_sub(s.last_fired[j]) < 10,
            aiming_at_me: s.aim[j].dot(to_me) > COS_3_DEG,
            locked_on_me: s.input[j].lock_target == i as u16,
            hostile: s.faction[j] != s.faction[i],
        }
    }

    // ---------------------------------------------------------------------------------------------
    // AI
    // ---------------------------------------------------------------------------------------------

    fn ai_step(&mut self, t: u32) {
        let interval = self.cfg.doll_think_interval.max(1);
        let mut scratch = core::mem::take(&mut self.scratch);
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let seized = self.suits.zero[i].mode == bc_proto::snapshot::zero_mode::SEIZED;
            if self.suits.pilot[i] != PilotKind::MobileDoll && !seized {
                continue;
            }
            let profile = if seized { &SEIZED } else { &DOLL };
            let spec = frame(self.suits.frame[i]);
            let range = spec.loadout[0].map_or(3_000.0, |m| weapon(m.weapon).range);
            let mut ai_state = self.suits.ai[i];
            if t >= ai_state.think_at {
                self.perceive_into(i, &mut scratch);
                if seized {
                    ai_state.order_target = self.suits.zero[i].out.rec_target;
                }
                ai::think(&scratch, &mut ai_state, t, profile, range);
                ai_state.think_at = t + interval;
            }
            let target = (ai_state.target != NO_SLOT && self.suits.is_alive(ai_state.target as usize))
                .then(|| self.contact_of(ai_state.target as usize, i, t));
            let me = self.self_view(i);
            let mut cmd = ai::drive(&me, target.as_ref(), &mut ai_state, t, profile, spec);
            if seized {
                cmd.buttons |= ZERO; // keep the System engaged while it holds the controls
            }
            self.suits.ai[i] = ai_state;
            self.suits.input[i] = cmd;
        }
        self.iter_bits = alive;
        self.scratch = scratch;
    }

    // ---------------------------------------------------------------------------------------------
    // Flight
    // ---------------------------------------------------------------------------------------------

    /// Damage and busy-arm modifiers for the flight model (also replicated to the owner).
    pub fn flight_mods(&self, i: usize) -> FlightMods {
        let s = &self.suits;
        let hp = &s.part_hp[i];
        let dead = |p: Part| hp[p as usize] <= 0.0;
        let mut ambac: f32 = 1.0;
        if dead(Part::ArmL) {
            ambac -= 0.2;
        }
        if dead(Part::ArmR) {
            ambac -= 0.2;
        }
        if dead(Part::Legs) {
            ambac -= 0.3;
        }
        let busy = s.saber[i].phase != SaberPhase::Idle || self.tick.saturating_sub(s.last_fired[i]) < 6;
        if busy {
            ambac *= 0.6;
        }
        let mut thrust: f32 = if dead(Part::Backpack) { 0.35 } else { 1.0 };
        if dead(Part::Legs) {
            thrust *= 0.9;
        }
        FlightMods {
            ambac: ambac.max(0.1),
            thrust,
            g_immune: s.pilot[i] == PilotKind::MobileDoll,
            lunge: matches!(s.saber[i].phase, SaberPhase::Windup | SaberPhase::Active),
        }
    }

    fn flight_step(&mut self, _t: u32) {
        let mut used = core::mem::take(&mut self.iter_bits);
        used.copy_from(&self.suits.used);
        for i in used.iter() {
            if self.suits.alive.get(i) {
                let mods = self.flight_mods(i);
                let spec = frame(self.suits.frame[i]);
                let cmd = self.suits.input[i];
                let out = flight::step(&mut self.suits.flight[i], &cmd, spec, &mods, DT);
                self.suits.boosting[i] = out.boosting;
                self.suits.aim[i] = normalize_or(cmd.aim, self.suits.flight[i].rot * Vec3::Z);
            } else {
                // Wrecks drift.
                let f = &mut self.suits.flight[i];
                f.pos += f.vel * DT;
            }
        }
        self.iter_bits = used;
    }

    fn spatial_rebuild(&mut self) {
        let suits = &self.suits;
        self.spatial.build(suits.alive.iter().map(|i| (i, suits.flight[i].pos)));
        self.alive_count = suits.alive.count();
    }

    fn record_history(&mut self, t: u32) {
        let suits = &self.suits;
        self.history.record(t, &suits.alive, |i| (suits.flight[i].pos, suits.flight[i].rot));
    }

    // ---------------------------------------------------------------------------------------------
    // Status: heat, energy, ZERO strain, respawns
    // ---------------------------------------------------------------------------------------------

    fn status_step(&mut self, t: u32) {
        let mut used = core::mem::take(&mut self.iter_bits);
        used.copy_from(&self.suits.used);
        for i in used.iter() {
            let spec = frame(self.suits.frame[i]);
            if self.suits.alive.get(i) {
                let s = &mut self.suits;
                s.heat[i] = (s.heat[i] - spec.heat_dissipation * DT).max(0.0);
                if s.heat[i] >= spec.heat_cap {
                    s.overheated[i] = true;
                } else if s.overheated[i] && s.heat[i] < spec.heat_cap * 0.5 {
                    s.overheated[i] = false;
                }
                s.energy[i] = (s.energy[i] + spec.energy_regen * DT).min(spec.energy_cap);
                let want = s.input[i].pressed(ZERO);
                let capable = spec.zero || self.cfg.zero_on_all_frames;
                let g = s.flight[i].g_strain;
                match s.zero[i].update(want, capable, g, DT) {
                    StrainEvent::Seized => {
                        s.zero[i].out.computed_at = 0;
                        self.events.push(Event::Seizure { id: 0, tick: t, pilot: i as u16, active: true });
                    }
                    StrainEvent::Released => {
                        self.events.push(Event::Seizure { id: 0, tick: t, pilot: i as u16, active: false });
                    }
                    StrainEvent::None => {}
                }
                self.suits.prev_buttons[i] = self.suits.input[i].buttons;
            } else if self.suits.respawn_at[i] != 0 && t >= self.suits.respawn_at[i] {
                if self.suits.pilot[i] == PilotKind::MobileDoll {
                    self.suits.release(i);
                } else {
                    self.spawn_counter += 1;
                    let (pos, rot) = spawn_point(self.suits.faction[i], self.spawn_counter);
                    let f = self.suits.respawn_frame[i];
                    self.suits.place(i, f, pos, rot, t);
                }
            }
        }
        self.iter_bits = used;
    }

    /// FNV-1a over the simulation state (determinism tests).
    pub fn state_hash(&self) -> u64 {
        crate::hash::state_hash(self)
    }
}
