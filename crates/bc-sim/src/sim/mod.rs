//! The sector simulation: fixed-capacity world state and the per-tick pipeline.
//!
//! [`Sim::step`] never allocates and never blocks. Every buffer it touches was sized in
//! [`Sim::new`]; the `no_alloc` test counts heap operations across thousands of ticks.
//!
//! Pipeline for tick `T`:
//! 1. Mobile Doll AI (and ZERO seizures) write `InputCmd`s (dolls re-plan every 3rd tick, staggered).
//! 2. Flight: AMBAC/RCS attitude, thrust, propellant, G-strain; or, in a body's grip, walking on it
//!    and hopping over it (`ground`); sleepers are held to what they're parked on; wrecks drift;
//!    rocks and landmarks stop them all.
//! 3. Spatial hash rebuild, then lag-compensation history is recorded (`history[T]` = snapshot `T`).
//!    Then cover: who lies still, and in which hide spot (`conceal`).
//! 4. Weapons fire: projectiles spawn and catch up through the history (≤ 8 ticks) for shots fired
//!    by humans/agents; beams emit spawn events.
//! 5. Projectiles sweep against per-part capsules (rocks, landmarks and the colony stop them,
//!    whichever comes first); sabers sweep their arcs.
//! 6. Damage resolves in generation order; parts break; suits die.
//! 7. Heat, energy, ZERO strain, respawns; staggered ZERO rollouts.

use alloc::boxed::Box;

mod combat;
mod conceal;
mod detection;
mod flame;
mod kits;
mod launch;
mod melee;
mod mining;
mod missile;
mod salvage;
mod sleep;
mod specials;
mod wire;
mod zero;

use bc_proto::buttons::{GRAB, GRIP, MODE, ZERO};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, Part, PilotKind, Segment, WeaponKind};
use glam::{Quat, Vec3};

use crate::ai::{self, DOLL, SEIZED};
use crate::arms::{BUSY_FIRE_TICKS, busy_ambac};
use crate::bodies::{Bodies, landmark_pose, landmark_touching, sweep_landmarks};
use crate::chunks::{self, Chunks, Motion, held_pose, segment_pos, segment_rot};
use crate::config::{DT, SECTOR_LIMIT, SimConfig, secs};
use crate::content::landmarks::LandmarkDef;
use crate::content::salvage::{BOUNCE, mass_without};
use crate::content::{frame, weapon};
use crate::events::EventRing;
use crate::field::Field;
use crate::flight::FlightMods;
use crate::ground::{self, MoveCtx, Mover};
use crate::handle::SuitId;
use crate::lagcomp::History;
use crate::math::{Rng, length, look_rotation, normalize_or};
use crate::missiles::{MAX_MISSILES, Missiles};
use crate::perception::{Contact, KitView, Perception, SelfView};
use crate::projectiles::Projectiles;
use crate::rocks::RockStates;
use crate::spatial::SpatialHash;
use crate::storage::{BitSet, FixedVec, boxed};
use crate::suits::{MeleePhase, Suits};
use crate::transform::transform_thrust;
use crate::tuning::{self, Tuning};
use crate::zero::TacticalAdvice;
use crate::zero::strain::StrainEvent;

pub use crate::bodies::Body;
pub use crate::ground::{Anchor, Footing};
pub use crate::suits::Usage;
pub use conceal::{
    COLD_SIG, Conceal, EXPOSE_TICKS, FOUGHT_DARK_TICKS, HIDE_AWAKE_VISUAL_MUL, LURK_SETTLE_TICKS, LURK_STILL,
    POWER_DOWN_TICKS, cover,
};
pub use launch::{Homecoming, LaunchAt, Loadout, ParkRecord};
pub use sleep::{Gone, PARK_SPEED, PARKED_VISUAL, SleeperFate};

/// A pending hit, applied in the damage phase.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DamageEvent {
    pub target: u16,
    pub part: Part,
    pub amount: f32,
    pub shooter: u16,
    pub weapon: WeaponKind,
    /// Which way the blow struck (a part it destroys flies off that way).
    pub dir: Vec3,
}

const MAX_SQUADS: usize = 32;
/// cos(3°): "aiming at me" threshold.
const COS_3_DEG: f32 = 0.998_629_5;
const SQUAD_SIZE: u8 = 4;
/// A brain's stick is kept off a landmark within this of its bounds, m, when its suit's feet are
/// within this of the surface, m (`keep_off_landmarks`).
const PUSH_REACH: f32 = 200.0;
const PUSH_HEIGHT: f32 = 150.0;

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
    /// Homing missiles in flight.
    pub missiles: Missiles,
    /// The debris field (static: rocks are solid to suits and stop shots).
    pub field: Field,
    /// What mining has done to the field's rocks.
    pub rocks: RockStates,
    /// Salvage: loose ore, limbs shot off, hulks.
    pub chunks: Chunks,
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
    /// Live-projectile and live-missile high-water marks (diagnostics).
    pub peak_projectiles: usize,
    pub peak_missiles: usize,
    /// Suits alive after the last tick.
    alive_count: usize,
    /// Scratch: a copy of a membership set to iterate while mutating suits.
    iter_bits: BitSet,
    /// Scratch: spatial query results inside `perceive_into`.
    query_bits: BitSet,
    /// Scratch: live projectiles to iterate while killing some.
    proj_bits: BitSet,
    /// Scratch: live missiles, likewise.
    missile_bits: BitSet,
    /// Scratch: live chunks, likewise.
    chunk_bits: BitSet,
    /// Scratch: the suits `cover_step` goes through (nothing it calls uses it).
    cover_bits: BitSet,
    /// Sleepers lost since the server last asked (`drain_fates`).
    fates: FixedVec<SleeperFate>,
    /// Suits alive on their feet on a body, aloft in a body's grip, and hidden from their enemies'
    /// sensors (parked and dark, or settled in a hide spot), and the sleepers among those hidden, as
    /// of the last tick.
    pub n_grounded: u32,
    pub n_aloft: u32,
    pub n_hidden: u32,
    pub n_hidden_asleep: u32,
}

impl Sim {
    /// Allocates all storage. Nothing grows afterwards.
    pub fn new(cfg: SimConfig) -> Self {
        let cap = cfg.max_suits.min(NO_SLOT as usize);
        let field = Field::generate(cfg.field_seed, cfg.field_rocks);
        let rocks = RockStates::new(&field);
        Self {
            cfg,
            tick: 0,
            suits: Suits::new(cap),
            projectiles: Projectiles::new(cfg.max_projectiles),
            missiles: Missiles::new(),
            field,
            rocks,
            chunks: Chunks::new(),
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
                    dir: Vec3::Z,
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
            peak_missiles: 0,
            alive_count: 0,
            iter_bits: BitSet::new(cap),
            query_bits: BitSet::new(cap),
            proj_bits: BitSet::new(cfg.max_projectiles),
            missile_bits: BitSet::new(MAX_MISSILES),
            chunk_bits: BitSet::new(chunks::MAX_CHUNKS),
            cover_bits: BitSet::new(cap),
            fates: FixedVec::new(64, SleeperFate { suit: 0, generation: 0, gone: Gone::Evicted, tick: 0 }),
            n_grounded: 0,
            n_aloft: 0,
            n_hidden: 0,
            n_hidden_asleep: 0,
        }
    }

    /// The last completed tick.
    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The sector's landmarks (`cfg.landmarks` of them), by id.
    pub fn landmarks(&self) -> &'static [LandmarkDef] {
        self.cfg.landmark_defs()
    }

    /// The tick the next [`step`](Self::step) will simulate (inputs should target it).
    #[inline]
    pub fn next_tick(&self) -> u32 {
        self.tick + 1
    }

    pub fn alive_count(&self) -> usize {
        self.alive_count
    }

    /// Adds a player or agent suit at its faction's spawn point. `None` if the sector is full, or
    /// the frame isn't one pilots may fly.
    pub fn join(&mut self, frame_id: FrameId, faction: Faction, pilot: PilotKind) -> Option<SuitId> {
        if !frame(frame_id).playable {
            return None;
        }
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

    /// Removes a suit (disconnect). What it carried is left behind.
    pub fn leave(&mut self, id: SuitId) {
        if self.suits.valid(id) {
            if self.suits.alive.get(id.idx()) {
                self.spill(id.idx(), self.tick, true);
            }
            self.suits.release(id.idx());
        }
    }

    /// Sets the command a player's suit will use next tick. Inside the colony nothing fires but in
    /// the Blast Hall: elsewhere those buttons never reach the tick ([`Sim::step`]).
    pub fn set_input(&mut self, id: SuitId, cmd: InputCmd) {
        if self.suits.valid(id) {
            self.suits.input[id.idx()] = cmd;
        }
    }

    /// Inside the colony its law holds but in the Blast Hall (`colony::hall::weapons_free`): any
    /// other suit's weapons' buttons are cleared before the tick, from where it is as the tick
    /// starts (as its pilot's prediction clears them).
    fn colony_law(&mut self) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            if !crate::colony::hall::weapons_free(self.suits.flight[i].pos) {
                self.suits.input[i].buttons &= !bc_proto::buttons::FIRE_MASK;
            }
        }
        self.iter_bits = alive;
    }

    /// In its bay's cradle a suit does nothing but wait for the catapult: its weapons' buttons and
    /// a change of form are cleared before the tick, as its pilot's prediction clears them
    /// ([`in_bay`](Self::in_bay)).
    fn bay_law(&mut self) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            if self.in_bay(i) {
                self.suits.input[i].buttons &= !bay_cleared();
            }
        }
        self.iter_bits = alive;
    }

    /// Suit `i` is riding its bay's catapult cradle (`colony::hub`), waiting to be thrown out.
    #[inline]
    pub fn in_bay(&self, i: usize) -> bool {
        i < self.suits.cap
            && self.suits.footing[i] != ground::Footing::Free
            && matches!(self.suits.anchor[i].body, Body::Bay(_))
    }

    /// This sector is the colony's inside (`colony::interior`).
    #[inline]
    pub fn interior(&self) -> bool {
        self.cfg.world == crate::colony::interior::WorldKind::Interior
    }

    /// Sets what a suit's pilot has earned (a signed-in pilot, back in a new suit, keeps theirs).
    pub fn set_credits(&mut self, id: SuitId, credits: u32) {
        if self.suits.valid(id) {
            self.suits.credits[id.idx()] = credits;
        }
    }

    /// Asks for the next respawn to use `frame_id`.
    pub fn set_respawn_frame(&mut self, id: SuitId, frame_id: FrameId) {
        if self.suits.valid(id) && frame(frame_id).playable {
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
        self.spawn_dolls(t);
        if t.is_multiple_of(30) {
            self.squad_logic();
        }
        self.ai_step(t);
        // Inside the colony, weapons are safe by its law but in the Blast Hall: elsewhere there no
        // special, lock, shot, missile or blade starts (their buttons never get this far), and
        // what's fired in the hall stays in it, touching no suit (`colony::hall`).
        if self.interior() {
            self.colony_law();
        } else {
            self.bay_law();
        }
        self.specials_step(t);
        self.flight_step(t);
        self.chunk_step(t);
        self.wrecks_follow_hulks();
        self.spatial_rebuild();
        self.record_history(t);
        self.cover_step(t);
        self.lock_step();
        self.weapons_step(t);
        self.projectile_step(t);
        self.missile_step(t);
        self.melee_step(t);
        self.damage_step(t);
        // (Emptied after, not before: a blow struck between ticks lands with this tick's.)
        self.damage.clear();
        self.salvage_step(t);
        self.status_step(t);
        self.zero_step(t);
        self.field_step(t);
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

    /// Every second: each squad focuses the hostile nearest its anchor (sleepers are left alone).
    fn squad_logic(&mut self) {
        for (s, squad) in self.squads.iter_mut().enumerate() {
            let mut best = (f32::MAX, NO_SLOT);
            for j in self.suits.alive.iter() {
                if self.suits.faction[j] == Faction::Oz || self.suits.sleeping.get(j) {
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
        let range = spec.sensor_range * self.suits.tuning[i].sensor;
        let t = self.tick;
        let mut found = core::mem::take(&mut self.query_bits);
        found.clear();
        self.spatial.query_sphere(me.pos, range * 1.3, |j| found.set(j, true));
        // Mobile Dolls don't hunt sleeping pilots.
        let doll = self.suits.pilot[i] == PilotKind::MobileDoll;
        for j in found.iter() {
            if j == i || !self.suits.alive.get(j) || (doll && self.suits.sleeping.get(j)) {
                continue;
            }
            // Cheap sensor test first; only detected suits get a full contact built.
            let pos = self.suits.flight[j].pos;
            if self.detects(i, j) && out.would_keep(length(pos - me.pos)) {
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
                    && s.fitted(i, slot)
                    && s.arm_free(i, m.arm);
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
            energy: s.energy[i] / (spec.energy_cap * s.tuning[i].energy_cap),
            propellant: f.propellant / tuning::tank_cap(spec, &s.tuning[i]),
            g_strain: f.g_strain,
            ready,
            overheated: s.overheated[i],
            kit: KitView {
                lock_acquired: self.missile_lock(i).is_some(),
                missile_incoming: s.incoming[i] > 0,
                special_ready: self.special_ready(i),
                special_active: s.special[i].active,
                transforming: self.transforming(i),
            },
            surface_n: self.surface_n(i),
            tuning: s.tuning[i],
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
            surface_n: self.surface_n(j),
        }
    }

    /// The ground's normal under suit `i` while it stands on a body, sector frame (`Vec3::ZERO`
    /// otherwise): what perception tells brains of a suit on the ground.
    fn surface_n(&self, i: usize) -> Vec3 {
        if self.suits.footing[i] == ground::Footing::Grounded {
            self.ground_normal(i).unwrap_or(Vec3::ZERO)
        } else {
            Vec3::ZERO
        }
    }

    /// The normal of the ground under suit `i`, sector frame, while it's on a body, aloft in its
    /// grip or parked on it: which way is up off the body where it is.
    pub(crate) fn ground_normal(&self, i: usize) -> Option<Vec3> {
        let a = self.suits.anchor[i];
        let bodies = Bodies::at(&self.field, self.landmarks(), self.tick);
        let (pose, shape) = (bodies.pose(a.body)?, bodies.shape(a.body)?);
        Some(pose.rot * ground::place(&shape, a.local, a.stance).1)
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
            // Its target, followed between thinks, unless it has gone behind a jammer.
            let target = (ai_state.target != NO_SLOT
                && self.suits.is_alive(ai_state.target as usize)
                && !(doll_ignores(self, i, ai_state.target as usize))
                && !self.jammed_from(i, ai_state.target as usize))
            .then(|| self.contact_of(ai_state.target as usize, i, t));
            let me = self.self_view(i);
            let mut cmd = ai::drive(&me, target.as_ref(), &mut ai_state, t, profile, spec);
            self.keep_off_landmarks(i, t, &mut cmd);
            if seized {
                // Keep the System engaged while it holds the controls, the pilot's grip on what's
                // in hand and on the ground, and the frame's mode (a seizure neither transforms the
                // suit, drops its jammer, nor lets go of the surface it stands on).
                cmd.buttons |= ZERO | (self.suits.input[i].buttons & (GRAB | MODE | GRIP));
            }
            self.suits.ai[i] = ai_state;
            self.suits.input[i] = cmd;
        }
        self.iter_bits = alive;
        self.scratch = scratch;
    }

    /// A brain's stick never flies its suit into a landmark: near one (its feet within
    /// `PUSH_HEIGHT` of the surface), the part of the stick into the surface is dropped. Only
    /// for a suit flying free (on the ground, the stick walks).
    fn keep_off_landmarks(&self, i: usize, t: u32, cmd: &mut InputCmd) {
        if self.suits.footing[i] != ground::Footing::Free {
            return;
        }
        let f = &self.suits.flight[i];
        for d in self.landmarks() {
            if length(f.pos - d.center) > d.bound + d.orbit_radius + PUSH_REACH {
                continue;
            }
            let pose = landmark_pose(d, t, 0.0);
            let pr = d.shape.probe(pose.to_local(f.pos));
            if pr.dist - ground::STANCE >= PUSH_HEIGHT {
                continue;
            }
            let n = pose.rot * pr.normal;
            let [x, y, z] = cmd.thrust.map(|v| f32::from(v) / 127.0);
            let stick = f.rot * Vec3::new(x, y, z);
            let into = stick.dot(n);
            if into < 0.0 {
                let local = f.rot.conjugate() * (stick - n * into);
                let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
                cmd.thrust = [q(local.x), q(local.y), q(local.z)];
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // Flight
    // ---------------------------------------------------------------------------------------------

    /// Damage and busy-arm modifiers for the flight model, for this tick's flight.
    pub fn flight_mods(&self, i: usize) -> FlightMods {
        self.flight_mods_at(i, self.tick)
    }

    /// Damage and busy-arm modifiers for the flight of tick `t`, from the arms as they stand. The
    /// owner's client builds the damage's from its snapshot with the same code (`crate::tuning`),
    /// and works out the arms' tick by tick (`crate::arms`).
    pub fn flight_mods_at(&self, i: usize, t: u32) -> FlightMods {
        let s = &self.suits;
        let busy =
            s.melee[i].phase != MeleePhase::Idle || t.saturating_sub(s.last_fired[i]) < BUSY_FIRE_TICKS;
        // Parts shot off lighten the suit; the hold and what's in hand weigh it down.
        let fid = s.frame[i];
        let held = self.held_chunk(i).map_or(0, |k| self.chunks.desc[k].mass_kg);
        let tuned = self.tuning(i);
        let extra_mass_kg = mass_without(fid, s.gone_mask(i)) as i32 - mass_without(fid, 0) as i32
            + (s.cargo_total_kg(i) + held + tuned.module_kg) as i32;
        let doll = s.pilot[i] == PilotKind::MobileDoll;
        let mut mods = tuning::flight_mods(&tuned, self.cfg.flight, doll, extra_mass_kg);
        mods.interior = self.interior();
        mods.main *= tuning::sputter(&tuned, t, i as u16);
        if busy {
            mods.ambac = busy_ambac(mods.ambac);
        }
        mods.lunge = s.melee[i].striking() && weapon(s.melee[i].weapon).melee.is_some_and(|m| m.lunge);
        mods
    }

    /// Suit `i`'s stat sheet, as rebuilt at the top of this tick's flight.
    #[inline]
    pub fn tuning(&self, i: usize) -> Tuning {
        self.suits.tuning[i]
    }

    /// How far off suit `i`'s axis a weapon on `arm` can point, radians: its reach, less what
    /// damaged actuators take.
    #[inline]
    pub fn cone(&self, i: usize, arm: crate::content::ArmSlot) -> f32 {
        tuning::cone(arm, &self.suits.tuning[i])
    }

    /// AMBAC's authority with the arms idle: what the limbs shot off leave of it.
    pub fn idle_ambac(&self, i: usize) -> f32 {
        self.tuning(i).ambac
    }

    /// What suit `i` brings to its step besides its command: its flight modifiers (with a change of
    /// form's cut in thrust: applied here, not in the replicated factor, as the owner's client
    /// applies it the same way as it predicts the change), whether it can hold on to a surface, and
    /// whether it has legs to walk on.
    pub fn move_ctx(&self, i: usize) -> MoveCtx<'static> {
        let mut mods = self.flight_mods(i);
        let form = self.suits.form(i);
        if form.changing() {
            mods.thrust *= transform_thrust(&form);
        }
        let spec = frame(self.suits.frame[i]);
        MoveCtx {
            spec,
            mods,
            can_grip: spec.has_legs() && !form.changing(),
            legs_ok: self.suits.part_hp[i][Part::Legs as usize] > 0.0,
        }
    }

    fn flight_step(&mut self, t: u32) {
        let mut used = core::mem::take(&mut self.iter_bits);
        used.copy_from(&self.suits.used);
        // Every stat sheet from the suits as they stood at the end of the last tick: what their
        // pilots' clients were just told, and fly the next tick with.
        for i in used.iter() {
            self.suits.retune(i);
        }
        // Every body where it is this tick (inside the colony, its city). It borrows only the
        // field, so the suits can move.
        let interior = self.interior();
        let bodies = Bodies::at(&self.field, self.landmarks(), t).inside(interior);
        for i in used.iter() {
            let asleep = self.suits.sleeping.get(i);
            if !self.suits.alive.get(i) {
                // Wrecks drift (and fetch up against rocks and landmarks).
                let f = &mut self.suits.flight[i];
                let prev = f.pos;
                f.pos += f.vel * DT;
                self.field.collide(prev, f);
                bodies.collide_landmarks(prev, f, None);
            } else if asleep && !interior && self.suits.footing[i] != ground::Footing::Aloft {
                // Nobody's flying it (`sleep`): held to its body, or drifting.
                sleep::sleeper_drift(&mut self.suits, &bodies, i);
            } else {
                // Flown, or (asleep in a grip, hands off) settling under it until it's down.
                let cx = self.move_ctx(i);
                let cmd = self.suits.input[i];
                let s = &mut self.suits;
                let mut m = Mover { flight: s.flight[i], footing: s.footing[i], anchor: s.anchor[i] };
                let out = ground::move_step(&bodies, &mut m, &cmd, &cx, DT);
                if asleep && m.footing == ground::Footing::Grounded {
                    // Down: it stays where it landed, parked.
                    m.anchor.vel = Vec3::ZERO;
                    m.anchor.ang_vel = Vec3::ZERO;
                    if let Some(p) = bodies.pose(m.anchor.body) {
                        sleep::hold(&p, &m.anchor, &mut m.flight);
                    }
                }
                (s.flight[i], s.footing[i], s.anchor[i]) = (m.flight, m.footing, m.anchor);
                s.boosting[i] = out.flight.boosting;
                if !asleep {
                    let u = &mut s.usage[i];
                    u.burn += u32::from(out.flight.throttle.z > 0.1);
                    u.boost += u32::from(out.flight.boosting);
                }
                if asleep {
                    sleep::look_ahead(s, i);
                } else {
                    s.aim[i] = normalize_or(cmd.aim, m.flight.rot * Vec3::Z);
                }
            }
        }
        self.iter_bits = used;
    }

    /// Free chunks drift, bounce off the colony, rocks and landmarks, leave the sector or expire.
    /// A landmark's bounce is off its surface as it moves: the chunk leaves it as fast as it came
    /// in, less the bounce's loss, relative to the surface where it struck.
    fn chunk_step(&mut self, t: u32) {
        if self.chunks.count() == 0 {
            return;
        }
        let landmarks = self.landmarks();
        let mut live = core::mem::take(&mut self.chunk_bits);
        live.copy_from(&self.chunks.alive);
        for k in live.iter() {
            let Motion::Free(seg) = self.chunks.motion[k] else { continue };
            let b = segment_pos(&seg, f64::from(t));
            if t >= self.chunks.expire[k] || b.abs().max_element() > SECTOR_LIMIT {
                self.chunks.kill(k);
                continue;
            }
            let a = segment_pos(&seg, f64::from(t - 1));
            let r = chunks::radius(&self.chunks.desc[k]);
            // The first of a rock and a landmark along the way (a tie goes to the rock), else a
            // landmark or the colony where it ends up (a landmark's surface moves into what's
            // still). Each with the velocity of its surface there.
            let rock = self.field.sweep(a, b, r);
            let landmark = sweep_landmarks(landmarks, a, b, r, t, 0.0)
                .or_else(|| landmark_touching(landmarks, b, r, t).map(|m| (1.0, m)));
            let contact = match (rock, landmark) {
                (Some((f, i)), _) if landmark.is_none_or(|(s, _)| f <= s) => {
                    let at = a + (b - a) * f;
                    Some((at, self.field.rocks()[i].normal(at, r), Vec3::ZERO))
                }
                (_, Some((f, m))) => {
                    let d = &landmarks[usize::from(m)];
                    let pose = landmark_pose(d, t, 0.0);
                    let mut local = pose.to_local(a + (b - a) * f);
                    let pr = d.shape.probe(local);
                    if pr.dist < r {
                        local += pr.normal * (r - pr.dist);
                    }
                    let at = pose.to_world(local);
                    Some((at, pose.rot * pr.normal, pose.point_vel(at)))
                }
                _ => crate::world::hull_contact(b, r).map(|(at, n)| (at, n, Vec3::ZERO)),
            };
            let Some((at, n, surface)) = contact else { continue };
            let vn = (seg.vel - surface).dot(n);
            if vn < 0.0 {
                let bounced = Segment {
                    t0: t,
                    pos: at,
                    vel: seg.vel - n * (vn * (1.0 + BOUNCE)),
                    rot: segment_rot(&seg, f64::from(t)),
                    spin: seg.spin * 0.7,
                };
                self.chunks.set_motion(k, Motion::Free(bounced.quantized()));
            }
        }
        self.chunk_bits = live;
    }

    /// Where chunk `k` is now, how it's turned, and how fast it's going.
    pub fn chunk_pose(&self, k: usize) -> (Vec3, Quat, Vec3) {
        match self.chunks.motion[k] {
            Motion::Free(seg) => {
                let t = f64::from(self.tick);
                (segment_pos(&seg, t), segment_rot(&seg, t), seg.vel)
            }
            Motion::Held { holder, right, rot, .. } => {
                let f = &self.suits.flight[holder as usize];
                let (pos, rot) = held_pose(f.pos, f.rot, right, rot, chunks::radius(&self.chunks.desc[k]));
                (pos, rot, f.vel)
            }
        }
    }

    /// A wreck is its hulk: it keeps the hulk's pose, so the suit clients see die and the hulk
    /// they see after it are one object.
    fn wrecks_follow_hulks(&mut self) {
        let mut used = core::mem::take(&mut self.iter_bits);
        used.copy_from(&self.suits.used);
        for i in used.iter() {
            let (h, g) = self.suits.hulk[i];
            if self.suits.alive.get(i) || !self.chunks.is_alive(h) || self.chunks.generation[h as usize] != g
            {
                continue;
            }
            let (pos, rot, vel) = self.chunk_pose(h as usize);
            let f = &mut self.suits.flight[i];
            f.pos = pos;
            f.rot = rot;
            f.vel = vel;
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
                let tuned = s.tuning[i];
                s.heat[i] = (s.heat[i] - spec.heat_dissipation * tuned.heat * DT).max(0.0);
                if s.heat[i] >= spec.heat_cap {
                    if !s.overheated[i] {
                        s.usage[i].overheats = s.usage[i].overheats.saturating_add(1);
                    }
                    s.overheated[i] = true;
                } else if s.overheated[i] && s.heat[i] < spec.heat_cap * 0.5 {
                    s.overheated[i] = false;
                }
                // After Full Open, the weapons stay locked out however fast it cools.
                if s.special[i].lockout > 0 {
                    s.overheated[i] = true;
                }
                // A scrammed reactor gives nothing until it's back.
                let st = &mut s.status[i];
                let regen = if st.scram > 0 { 0.0 } else { spec.energy_regen * tuned.regen };
                st.scram = st.scram.saturating_sub(1);
                st.concussed = st.concussed.saturating_sub(1);
                st.stim = st.stim.saturating_sub(1);
                st.chaff = st.chaff.saturating_sub(1);
                s.energy[i] = (s.energy[i] + regen * DT).min(spec.energy_cap * tuned.energy_cap);
                if tuned.repairs {
                    self.damage_control(i);
                }
                let s = &mut self.suits;
                // Asleep, nobody's there for the System to strain.
                if !s.sleeping.get(i) {
                    let want = s.input[i].pressed(ZERO);
                    let capable = spec.zero || self.cfg.zero_on_all_frames;
                    let g = s.flight[i].g_strain;
                    match s.zero[i].update(want, capable, g, DT) {
                        StrainEvent::Seized => {
                            s.zero[i].out.computed_at = 0;
                            self.events.push(Event::Seizure {
                                id: 0,
                                tick: t,
                                pilot: i as u16,
                                active: true,
                            });
                        }
                        StrainEvent::Released => {
                            self.events.push(Event::Seizure {
                                id: 0,
                                tick: t,
                                pilot: i as u16,
                                active: false,
                            });
                        }
                        StrainEvent::None => {}
                    }
                }
                self.suits.prev_buttons[i] = self.suits.input[i].buttons;
            } else if self.suits.respawn_at[i] != 0 && t >= self.suits.respawn_at[i] {
                // A sleeper destroyed is gone: nobody is there to respawn. Under survival rules
                // nobody respawns: the pilot is back in the hangar, and has to build another suit.
                if self.suits.pilot[i] == PilotKind::MobileDoll
                    || self.suits.sleeping.get(i)
                    || self.cfg.survival
                {
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

    /// Damage control works on suit `i`'s damaged systems one at a time, drawing energy while it
    /// does (it pauses when there isn't enough). It can't mend what's failed.
    fn damage_control(&mut self, i: usize) {
        use crate::content::modules::{REPAIR_ENERGY, REPAIR_TICKS};
        use crate::content::systems::{DAMAGED, OK, System};
        use crate::suits::NO_REPAIR;
        let s = &mut self.suits;
        let gone = s.gone_mask(i);
        let now = s.systems[i];
        let st = &mut s.status[i];
        let working =
            System::from_index(usize::from(st.repairing)).filter(|sys| now.level(*sys, gone) == DAMAGED);
        let Some(sys) = working.or_else(|| System::ALL.into_iter().find(|x| now.level(*x, gone) == DAMAGED))
        else {
            st.repairing = NO_REPAIR;
            st.repair_left = 0;
            return;
        };
        if working.is_none() {
            st.repairing = sys as u8;
            st.repair_left = REPAIR_TICKS;
        }
        let draw = REPAIR_ENERGY * DT;
        if s.energy[i] < draw {
            return;
        }
        s.energy[i] -= draw;
        st.repair_left = st.repair_left.saturating_sub(1);
        if st.repair_left == 0 {
            s.systems[i].set(sys, OK);
            st.repairing = NO_REPAIR;
        }
    }

    /// FNV-1a over the simulation state (determinism tests).
    pub fn state_hash(&self) -> u64 {
        crate::hash::state_hash(self)
    }
}

/// A Mobile Doll's target that's asleep is no target.
fn doll_ignores(sim: &Sim, i: usize, j: usize) -> bool {
    sim.suits.pilot[i] == PilotKind::MobileDoll && sim.suits.sleeping.get(j)
}

/// What a suit in its bay's cradle can't do (`Sim::bay_law`, and its pilot's prediction): fire,
/// use its special, strike, or change its form.
pub const fn bay_cleared() -> u16 {
    bc_proto::buttons::FIRE_MASK | MODE
}
