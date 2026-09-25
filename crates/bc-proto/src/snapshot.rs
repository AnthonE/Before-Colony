//! Server → client snapshot datagram.
//!
//! Layout, in order (all bit-packed):
//!
//! | Section | Size | Notes |
//! |---|---|---|
//! | header | 116 bits | tick, input ack, input-buffer health, RTT echo, time dilation |
//! | own state | 1 + 610 bits | full precision: the client reconciles its prediction against it |
//! | ZERO | 1 + ~203 bits | only while the pilot's ZERO System is engaged |
//! | events | `1+n` bits each, `0` ends | repeated until the client acks a snapshot containing them |
//! | rocks | `1+18` bits each, `0` ends | debris-field rocks whose state changed, repeated until acked |
//! | entities | `1+204` bits each, `0` ends | as many prioritised contacts as fit |
//! | objects | `1+12..232` bits each, `0` ends | salvage chunks (ore, limbs, hulks) in range |
//!
//! Everything must fit in [`MAX_DATAGRAM`](crate::MAX_DATAGRAM) bytes. The writer checks the budget
//! before every record, counting the terminators still owed, and never produces a partial item.

use glam::{Quat, Vec3};

use crate::events::Event;
use crate::objects::{ObjectState, ROCK_RECORD_BITS, RockState};
use crate::quant::{self, dequantize_signed, dequantize_unit, quantize_signed, quantize_unit};
use crate::types::{Faction, FrameId, Part, PilotKind};
use crate::{
    BitReader, BitWriter, CARGO_KINDS, CHUNK_BITS, DecodeError, NO_CHUNK, PACKET_KIND_BITS, PacketKind,
    SLOT_BITS,
};

/// Maneuver hypotheses the ZERO System weighs per threat (see `bc_sim::zero`).
pub const ZERO_HYPOTHESES: usize = 7;
/// Threats with full predicted futures in one snapshot.
pub const ZERO_THREATS: usize = 2;
const P_BITS: u32 = 7;

/// Per-snapshot metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SnapshotHeader {
    pub tick: u32,
    /// Newest input tick applied to this client's suit (`u32::MAX` = none yet).
    pub ack_input_tick: u32,
    /// Newest input tick received minus `tick`: how far ahead the client's inputs arrive. Clients
    /// steer their send clock to keep this around +2.
    pub input_health: i8,
    /// The client's `client_time_ms` from its latest input packet...
    pub time_echo_ms: u16,
    /// ...and how long (ms) the server held it before this snapshot went out.
    pub echo_hold_ms: u8,
    /// Time dilation in percent (100 = real time). Reserved for EVE-style TiDi.
    pub tidi_pct: u8,
    pub flags: u8,
}

/// Own-suit flags.
pub mod own_flags {
    pub const BOOSTING: u16 = 1 << 0;
    /// G-strain blackout: control authority reduced.
    pub const BLACKOUT: u16 = 1 << 1;
    pub const OVERHEAT: u16 = 1 << 2;
    /// Twin Buster Rifle charging.
    pub const CHARGING: u16 = 1 << 3;
    pub const SABER_ACTIVE: u16 = 1 << 4;
    /// This frame carries the ZERO System.
    pub const ZERO_CAPABLE: u16 = 1 << 5;
    /// Flight assist engaged (as applied by the server).
    pub const FLIGHT_ASSIST: u16 = 1 << 6;
    /// Something has a weapons lock on you.
    pub const LOCKED_ON: u16 = 1 << 7;
    /// In the colony's dock: cargo, and anything in hand, sells on arrival.
    pub const DOCKED: u16 = 1 << 8;
    /// Beam saber lunge (windup and swing): the flight model drives forward at full thrust.
    pub const LUNGE: u16 = 1 << 9;
}

/// ZERO System state for [`OwnState::zero_mode`].
pub mod zero_mode {
    pub const OFF: u8 = 0;
    pub const ACTIVE: u8 = 1;
    /// ZERO has seized control.
    pub const SEIZED: u8 = 2;
    /// Cooling down after a seizure; cannot be engaged.
    pub const LOCKOUT: u8 = 3;
}

/// The receiving pilot's own suit, precise enough to re-run the flight model from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnState {
    pub slot: u16,
    /// Low 2 bits of the suit's generation (changes on respawn).
    pub generation: u8,
    pub frame: FrameId,
    pub alive: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub ang_vel: Vec3,
    /// Remaining propellant, kg.
    pub propellant: f32,
    /// 0..1; ≥1 means blackout.
    pub g_strain: f32,
    /// 0..1 of the overheat threshold.
    pub heat: f32,
    /// 0..1 of the energy capacitor.
    pub energy: f32,
    /// Rounds left: primary, secondary.
    pub ammo: [u16; 2],
    /// Bits 0..3: primary, secondary, melee ready.
    pub weapon_ready: u8,
    /// 0..1 Twin Buster Rifle charge.
    pub charge: f32,
    /// Armour left per [`Part`], 0..1 (any left at all arrives as more than 0).
    pub parts: [f32; Part::COUNT],
    pub zero_strain: f32,
    pub zero_mode: u8,
    pub flags: u16,
    /// Flight-model modifiers from damage and busy arms (0..1), so prediction matches the server.
    pub ambac_factor: f32,
    pub thrust_factor: f32,
    /// While dead: ticks until respawn, divided by 4.
    pub respawn_in: u8,
    /// Mass beyond the frame's own: cargo and anything in hand, less the parts shot off, kg. The
    /// flight model (and so prediction) uses exactly this.
    pub extra_mass_kg: i32,
    /// The hold's contents per ore kind, kg.
    pub cargo_kg: [u16; CARGO_KINDS],
    /// Credits earned this session.
    pub credits: u32,
    /// The chunk in hand ([`NO_CHUNK`] = none).
    pub held: u16,
}

/// Encoded size of the own state (after its presence bit), in bits.
pub const OWN_BITS: usize = 502 + 18 + 14 * CARGO_KINDS + 24 + CHUNK_BITS as usize;
const EXTRA_MASS_BITS: u32 = 18;
const CARGO_BITS: u32 = 14;
const CREDIT_BITS: u32 = 24;

impl Default for OwnState {
    fn default() -> Self {
        Self {
            slot: 0,
            generation: 0,
            frame: FrameId::Leo,
            alive: false,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            ang_vel: Vec3::ZERO,
            propellant: 0.0,
            g_strain: 0.0,
            heat: 0.0,
            energy: 0.0,
            ammo: [0; 2],
            weapon_ready: 0,
            charge: 0.0,
            parts: [1.0; Part::COUNT],
            zero_strain: 0.0,
            zero_mode: zero_mode::OFF,
            flags: 0,
            ambac_factor: 1.0,
            thrust_factor: 1.0,
            respawn_in: 0,
            extra_mass_kg: 0,
            cargo_kg: [0; CARGO_KINDS],
            credits: 0,
            held: NO_CHUNK,
        }
    }
}

/// Replicated-entity flags.
pub mod ent_flags {
    pub const FIRING_PRIMARY: u16 = 1 << 0;
    pub const FIRING_SECONDARY: u16 = 1 << 1;
    pub const SABER: u16 = 1 << 2;
    pub const BOOST: u16 = 1 << 3;
    pub const CHARGING: u16 = 1 << 4;
    pub const ZERO: u16 = 1 << 5;
    pub const SEIZED: u16 = 1 << 6;
    pub const OVERHEAT: u16 = 1 << 7;
    /// Destroyed: a drifting wreck.
    pub const WRECK: u16 = 1 << 8;
    /// This suit is locked on to the receiving pilot.
    pub const LOCKED_ON_YOU: u16 = 1 << 9;
    pub const BITS: u32 = 10;
}

/// Another suit as seen by the receiving pilot's sensors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub slot: u16,
    pub generation: u8,
    pub frame: FrameId,
    pub faction: Faction,
    pub pilot: PilotKind,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub aim: Vec3,
    pub flags: u16,
    /// Armour per part in eighths (0 = destroyed, 7 = pristine).
    pub parts: [u8; Part::COUNT],
}

impl Default for EntityState {
    fn default() -> Self {
        Self {
            slot: 0,
            generation: 0,
            frame: FrameId::Leo,
            faction: Faction::Oz,
            pilot: PilotKind::Human,
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            aim: Vec3::Z,
            flags: 0,
            parts: [7; Part::COUNT],
        }
    }
}

/// Encoded size of one entity record, in bits.
pub const ENTITY_BITS: usize = SLOT_BITS as usize
    + 2
    + (FrameId::BITS + Faction::BITS + PilotKind::BITS) as usize
    + 3 * quant::POS_BITS as usize
    + 32
    + 3 * quant::VEL_BITS as usize
    + 2 * ENTITY_AIM_BITS as usize
    + ent_flags::BITS as usize
    + 3 * Part::COUNT;
const ENTITY_AIM_BITS: u32 = 9;
const ENTITY_ROT_BITS: u32 = 10;

/// One threat's predicted futures: probability of each maneuver hypothesis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZeroThreat {
    pub slot: u16,
    pub probs: [f32; ZERO_HYPOTHESES],
}

/// What the ZERO System shows its pilot. The client re-runs the same rollouts from its own view to
/// draw the ghost trails, so only IDs and probabilities travel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZeroInfo {
    /// Jev advice was blended into these probabilities.
    pub source_jev: bool,
    /// Age of that advice in ticks/2 (0..15).
    pub advice_age: u8,
    pub threat_count: u8,
    pub threats: [ZeroThreat; ZERO_THREATS],
    /// Recommended target and its probability.
    pub rec_target: u16,
    pub rec_target_p: f32,
    /// Recommended own maneuver (hypothesis index) and its probability.
    pub rec_maneuver: u8,
    pub rec_maneuver_p: f32,
    /// 0 low, 1 moderate, 2 high, 3 lethal.
    pub threat_level: u8,
    pub threat_confidence: f32,
    /// Probability that a threat is flanking from outside the forward arc.
    pub flanked: f32,
    /// Firing solution: aim direction and probability the shot connects.
    pub has_solution: bool,
    pub solution: Vec3,
    pub hit_p: f32,
}

fn write_p(w: &mut BitWriter<'_>, p: f32) {
    w.write_bits(quantize_unit(p, P_BITS), P_BITS);
}

fn read_p(r: &mut BitReader<'_>) -> f32 {
    dequantize_unit(r.read_bits(P_BITS), P_BITS)
}

// ---------------------------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------------------------

/// The lists after the fixed part, in order. Each is closed by a `0` bit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum List {
    Events,
    Rocks,
    Entities,
    Objects,
    Done,
}

impl List {
    fn next(self) -> Self {
        match self {
            List::Events => List::Rocks,
            List::Rocks => List::Entities,
            List::Entities => List::Objects,
            List::Objects | List::Done => List::Done,
        }
    }
}

/// Budget-aware snapshot encoder. Call `header`, `own`, `zero`, then `event`s, `rock`s, `entity`s
/// and `object`s in that order (starting a list closes the ones before it), then `finish`.
pub struct SnapshotWriter<'a> {
    w: BitWriter<'a>,
    tick: u32,
    /// The list being written; earlier ones are closed.
    list: List,
}

impl<'a> SnapshotWriter<'a> {
    pub fn new(buf: &'a mut [u8], max_bytes: usize) -> Self {
        Self { w: BitWriter::with_limit(buf, max_bytes), tick: 0, list: List::Events }
    }

    pub fn header(&mut self, h: &SnapshotHeader) {
        let w = &mut self.w;
        self.tick = h.tick;
        w.write_bits(PacketKind::Snapshot as u32, PACKET_KIND_BITS);
        w.write_u32(h.tick);
        w.write_u32(h.ack_input_tick);
        w.write_i32(i32::from(h.input_health), 8);
        w.write_u16(h.time_echo_ms);
        w.write_u8(h.echo_hold_ms);
        w.write_u8(h.tidi_pct);
        w.write_u8(h.flags);
    }

    pub fn own(&mut self, own: Option<&OwnState>) {
        let w = &mut self.w;
        let Some(o) = own else {
            w.write_bool(false);
            return;
        };
        w.write_bool(true);
        w.write_bits(u32::from(o.slot), SLOT_BITS);
        w.write_bits(u32::from(o.generation & 3), 2);
        w.write_bits(o.frame as u32, FrameId::BITS);
        w.write_bool(o.alive);
        quant::write_vec_f32(w, o.pos);
        quant::write_vec_f32(w, o.vel);
        quant::write_quat(w, o.rot, 16);
        quant::write_vec(w, o.ang_vel, 8.0, 16);
        w.write_f32(o.propellant);
        w.write_bits(quantize_unit(o.g_strain, 16), 16);
        w.write_bits(quantize_unit(o.heat, 10), 10);
        w.write_bits(quantize_unit(o.energy, 10), 10);
        w.write_bits(u32::from(o.ammo[0].min(1023)), 10);
        w.write_bits(u32::from(o.ammo[1].min(1023)), 10);
        w.write_bits(u32::from(o.weapon_ready), 3);
        w.write_bits(quantize_unit(o.charge, 6), 6);
        for p in o.parts {
            // A sliver of armour must not round to "gone".
            let q = quantize_unit(p, 8);
            w.write_bits(if p > 0.0 { q.max(1) } else { 0 }, 8);
        }
        w.write_bits(quantize_unit(o.zero_strain, 8), 8);
        w.write_bits(u32::from(o.zero_mode & 3), 2);
        w.write_u16(o.flags);
        w.write_bits(quantize_unit(o.ambac_factor, 8), 8);
        w.write_bits(quantize_unit(o.thrust_factor, 8), 8);
        w.write_u8(o.respawn_in);
        let reach = (1 << (EXTRA_MASS_BITS - 1)) - 1;
        w.write_i32(o.extra_mass_kg.clamp(-reach, reach), EXTRA_MASS_BITS);
        for c in o.cargo_kg {
            w.write_bits(u32::from(c).min((1 << CARGO_BITS) - 1), CARGO_BITS);
        }
        w.write_bits(o.credits.min((1 << CREDIT_BITS) - 1), CREDIT_BITS);
        w.write_bits(u32::from(o.held.min(NO_CHUNK)), CHUNK_BITS);
    }

    pub fn zero(&mut self, zero: Option<&ZeroInfo>) {
        let w = &mut self.w;
        let Some(z) = zero else {
            w.write_bool(false);
            return;
        };
        w.write_bool(true);
        w.write_bool(z.source_jev);
        w.write_bits(u32::from(z.advice_age.min(15)), 4);
        let n = z.threat_count.min(ZERO_THREATS as u8);
        w.write_bits(u32::from(n), 2);
        for t in &z.threats[..n as usize] {
            w.write_bits(u32::from(t.slot), SLOT_BITS);
            for p in t.probs {
                write_p(w, p);
            }
        }
        w.write_bits(u32::from(z.rec_target), SLOT_BITS);
        write_p(w, z.rec_target_p);
        w.write_bits(u32::from(z.rec_maneuver.min(7)), 3);
        write_p(w, z.rec_maneuver_p);
        w.write_bits(u32::from(z.threat_level.min(3)), 2);
        write_p(w, z.threat_confidence);
        write_p(w, z.flanked);
        w.write_bool(z.has_solution);
        quant::write_dir(w, z.solution, 12);
        write_p(w, z.hit_p);
    }

    /// Terminators still owed: the open list's and every later one's.
    fn owed(&self) -> usize {
        List::Done as usize - self.list as usize
    }

    /// Moves on to `list`, closing the lists before it. False if it's already closed.
    fn open(&mut self, list: List) -> bool {
        if self.list > list {
            return false;
        }
        while self.list < list {
            self.w.write_bool(false);
            self.list = self.list.next();
        }
        true
    }

    /// Opens `list` and starts a record of `bits` if it fits, with `keep_free_bits` to spare
    /// after it and the terminators still owed.
    fn start(&mut self, list: List, bits: usize, keep_free_bits: usize) -> bool {
        if !self.open(list) || self.w.bits_remaining() < 1 + bits + self.owed() + keep_free_bits {
            return false;
        }
        self.w.write_bool(true);
        true
    }

    /// Appends an event if it fits while leaving `keep_free_bits` for what follows. Returns whether
    /// it was written.
    pub fn event(&mut self, e: &Event, keep_free_bits: usize) -> bool {
        if !self.start(List::Events, e.encoded_bits(), keep_free_bits) {
            return false;
        }
        e.write(&mut self.w, self.tick);
        true
    }

    /// Closes the event list (starting a later list does too).
    pub fn end_events(&mut self) {
        self.open(List::Rocks);
    }

    /// Appends a rock's state if it fits while leaving `keep_free_bits`.
    pub fn rock(&mut self, r: &RockState, keep_free_bits: usize) -> bool {
        if !self.start(List::Rocks, ROCK_RECORD_BITS, keep_free_bits) {
            return false;
        }
        r.write(&mut self.w);
        true
    }

    /// Appends an entity if it fits while leaving `keep_free_bits` (for objects). Returns whether
    /// it was written.
    pub fn entity(&mut self, e: &EntityState, keep_free_bits: usize) -> bool {
        if !self.start(List::Entities, ENTITY_BITS, keep_free_bits) {
            return false;
        }
        let w = &mut self.w;
        w.write_bits(u32::from(e.slot), SLOT_BITS);
        w.write_bits(u32::from(e.generation & 3), 2);
        w.write_bits(e.frame as u32, FrameId::BITS);
        w.write_bits(e.faction as u32, Faction::BITS);
        w.write_bits(e.pilot as u32, PilotKind::BITS);
        quant::write_pos(w, e.pos);
        quant::write_quat(w, e.rot, ENTITY_ROT_BITS);
        quant::write_vec(w, e.vel, quant::VEL_MAX, quant::VEL_BITS);
        quant::write_dir(w, e.aim, ENTITY_AIM_BITS);
        w.write_bits(u32::from(e.flags), ent_flags::BITS);
        for p in e.parts {
            w.write_bits(u32::from(p.min(7)), 3);
        }
        true
    }

    /// Appends a salvage object if it fits. Returns whether it was written.
    pub fn object(&mut self, o: &ObjectState) -> bool {
        if !self.start(List::Objects, o.encoded_bits(), 0) {
            return false;
        }
        o.write(&mut self.w, self.tick);
        true
    }

    /// Bits still free (useful for budgeting).
    pub fn bits_remaining(&self) -> usize {
        self.w.bits_remaining()
    }

    /// Closes every list and returns the datagram length, or `None` on overflow.
    pub fn finish(mut self) -> Option<usize> {
        self.open(List::Done);
        (!self.w.overflowed()).then(|| self.w.bytes_written())
    }
}

// ---------------------------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Own,
    Zero,
    Events,
    Rocks,
    Entities,
    Objects,
    Done,
}

/// Streaming snapshot decoder. Sections are read in order; calling a later section's reader skips
/// the earlier ones.
pub struct SnapshotReader<'a> {
    r: BitReader<'a>,
    header: SnapshotHeader,
    stage: Stage,
}

impl<'a> SnapshotReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(bytes);
        if r.read_bits(PACKET_KIND_BITS) != PacketKind::Snapshot as u32 {
            return Err(DecodeError::WrongKind);
        }
        let header = SnapshotHeader {
            tick: r.read_u32(),
            ack_input_tick: r.read_u32(),
            input_health: r.read_i32(8) as i8,
            time_echo_ms: r.read_u16(),
            echo_hold_ms: r.read_u8(),
            tidi_pct: r.read_u8(),
            flags: r.read_u8(),
        };
        if r.overflowed() {
            return Err(DecodeError::Truncated);
        }
        Ok(Self { r, header, stage: Stage::Own })
    }

    pub fn header(&self) -> &SnapshotHeader {
        &self.header
    }

    fn check(&self) -> Result<(), DecodeError> {
        if self.r.overflowed() { Err(DecodeError::Truncated) } else { Ok(()) }
    }

    /// Reads (and drops) whatever comes before `stage`.
    fn skip_to(&mut self, stage: Stage) -> Result<(), DecodeError> {
        while self.stage < stage {
            match self.stage {
                Stage::Own | Stage::Zero => {
                    self.zero()?;
                }
                Stage::Events => {
                    self.next_event()?;
                }
                Stage::Rocks => {
                    self.next_rock()?;
                }
                Stage::Entities => {
                    self.next_entity()?;
                }
                Stage::Objects | Stage::Done => {
                    self.next_object()?;
                }
            }
        }
        Ok(())
    }

    /// Reads the continuation bit of a list: false (moving on to `after`) at its end.
    fn more(&mut self, after: Stage) -> Result<bool, DecodeError> {
        if self.r.read_bool() {
            return Ok(true);
        }
        self.stage = after;
        self.check()?;
        Ok(false)
    }

    pub fn own(&mut self) -> Result<Option<OwnState>, DecodeError> {
        if self.stage != Stage::Own {
            return Err(DecodeError::Invalid);
        }
        self.stage = Stage::Zero;
        let r = &mut self.r;
        if !r.read_bool() {
            self.check()?;
            return Ok(None);
        }
        let mut o = OwnState {
            slot: r.read_bits(SLOT_BITS) as u16,
            generation: r.read_bits(2) as u8,
            frame: FrameId::from_bits(r.read_bits(FrameId::BITS)).ok_or(DecodeError::Invalid)?,
            alive: r.read_bool(),
            pos: quant::read_vec_f32(r),
            vel: quant::read_vec_f32(r),
            rot: quant::read_quat(r, 16),
            ang_vel: quant::read_vec(r, 8.0, 16),
            propellant: r.read_f32(),
            g_strain: dequantize_unit(r.read_bits(16), 16),
            heat: dequantize_unit(r.read_bits(10), 10),
            energy: dequantize_unit(r.read_bits(10), 10),
            ..OwnState::default()
        };
        o.ammo = [r.read_bits(10) as u16, r.read_bits(10) as u16];
        o.weapon_ready = r.read_bits(3) as u8;
        o.charge = dequantize_unit(r.read_bits(6), 6);
        for p in &mut o.parts {
            *p = dequantize_unit(r.read_bits(8), 8);
        }
        o.zero_strain = dequantize_unit(r.read_bits(8), 8);
        o.zero_mode = r.read_bits(2) as u8;
        o.flags = r.read_u16();
        o.ambac_factor = dequantize_unit(r.read_bits(8), 8);
        o.thrust_factor = dequantize_unit(r.read_bits(8), 8);
        o.respawn_in = r.read_u8();
        o.extra_mass_kg = r.read_i32(EXTRA_MASS_BITS);
        for c in &mut o.cargo_kg {
            *c = r.read_bits(CARGO_BITS) as u16;
        }
        o.credits = r.read_bits(CREDIT_BITS);
        o.held = r.read_bits(CHUNK_BITS) as u16;
        self.check()?;
        Ok(Some(o))
    }

    pub fn zero(&mut self) -> Result<Option<ZeroInfo>, DecodeError> {
        if self.stage < Stage::Zero {
            self.own()?;
        }
        if self.stage != Stage::Zero {
            return Err(DecodeError::Invalid);
        }
        self.stage = Stage::Events;
        let r = &mut self.r;
        if !r.read_bool() {
            self.check()?;
            return Ok(None);
        }
        let mut z =
            ZeroInfo { source_jev: r.read_bool(), advice_age: r.read_bits(4) as u8, ..ZeroInfo::default() };
        z.threat_count = (r.read_bits(2) as u8).min(ZERO_THREATS as u8);
        for t in &mut z.threats[..z.threat_count as usize] {
            t.slot = r.read_bits(SLOT_BITS) as u16;
            for p in &mut t.probs {
                *p = read_p(r);
            }
        }
        z.rec_target = r.read_bits(SLOT_BITS) as u16;
        z.rec_target_p = read_p(r);
        z.rec_maneuver = r.read_bits(3) as u8;
        z.rec_maneuver_p = read_p(r);
        z.threat_level = r.read_bits(2) as u8;
        z.threat_confidence = read_p(r);
        z.flanked = read_p(r);
        z.has_solution = r.read_bool();
        z.solution = quant::read_dir(r, 12);
        z.hit_p = read_p(r);
        self.check()?;
        Ok(Some(z))
    }

    /// Next event, or `None` at the end of the event list.
    pub fn next_event(&mut self) -> Result<Option<Event>, DecodeError> {
        self.skip_to(Stage::Events)?;
        if self.stage != Stage::Events || !self.more(Stage::Rocks)? {
            return Ok(None);
        }
        Event::read(&mut self.r, self.header.tick).map(Some)
    }

    /// Next changed rock, or `None` at the end of the rock list.
    pub fn next_rock(&mut self) -> Result<Option<RockState>, DecodeError> {
        self.skip_to(Stage::Rocks)?;
        if self.stage != Stage::Rocks || !self.more(Stage::Entities)? {
            return Ok(None);
        }
        let rock = RockState::read(&mut self.r);
        self.check()?;
        Ok(Some(rock))
    }

    /// Next entity, or `None` at the end of the entity list.
    pub fn next_entity(&mut self) -> Result<Option<EntityState>, DecodeError> {
        self.skip_to(Stage::Entities)?;
        if self.stage != Stage::Entities || !self.more(Stage::Objects)? {
            return Ok(None);
        }
        let r = &mut self.r;
        let mut e = EntityState {
            slot: r.read_bits(SLOT_BITS) as u16,
            generation: r.read_bits(2) as u8,
            frame: FrameId::from_bits(r.read_bits(FrameId::BITS)).ok_or(DecodeError::Invalid)?,
            faction: Faction::from_bits(r.read_bits(Faction::BITS)),
            pilot: PilotKind::from_bits(r.read_bits(PilotKind::BITS)),
            pos: quant::read_pos(r),
            rot: quant::read_quat(r, ENTITY_ROT_BITS),
            vel: quant::read_vec(r, quant::VEL_MAX, quant::VEL_BITS),
            aim: quant::read_dir(r, ENTITY_AIM_BITS),
            flags: r.read_bits(ent_flags::BITS) as u16,
            parts: [0; Part::COUNT],
        };
        for p in &mut e.parts {
            *p = r.read_bits(3) as u8;
        }
        self.check()?;
        Ok(Some(e))
    }

    /// Next salvage object, or `None` at the end of the packet.
    pub fn next_object(&mut self) -> Result<Option<ObjectState>, DecodeError> {
        self.skip_to(Stage::Objects)?;
        if self.stage != Stage::Objects || !self.more(Stage::Done)? {
            return Ok(None);
        }
        let o = ObjectState::read(&mut self.r, self.header.tick)?;
        self.check()?;
        Ok(Some(o))
    }
}

/// Converts armour fractions to the 0..7 buckets used in [`EntityState::parts`].
pub fn part_buckets(parts: &[f32; Part::COUNT]) -> [u8; Part::COUNT] {
    let mut out = [0u8; Part::COUNT];
    for (o, p) in out.iter_mut().zip(parts) {
        *o = if *p <= 0.0 { 0 } else { 1 + (p.clamp(0.0, 1.0) * 6.0) as u8 };
    }
    out
}

/// Precision helpers, exposed for tests and clients.
pub fn entity_pos_step() -> f32 {
    quant::signed_step(crate::SECTOR_HALF_EXTENT, quant::POS_BITS)
}

pub fn round_trip_signed(v: f32, max_abs: f32, bits: u32) -> f32 {
    dequantize_signed(quantize_signed(v, max_abs, bits), max_abs, bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_state_is_exactly_own_bits() {
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.own(Some(&OwnState::default()));
        assert_eq!(w.w.bits_written(), 1 + OWN_BITS);
    }

    #[test]
    fn a_sliver_of_armour_is_not_gone() {
        let own = OwnState { parts: [1e-4, 0.0, 1.0, 0.5, 0.3, 1e-9], ..OwnState::default() };
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.header(&SnapshotHeader::default());
        w.own(Some(&own));
        let n = w.finish().unwrap();
        let back = SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap();
        assert!(back.parts[0] > 0.0 && back.parts[5] > 0.0);
        assert_eq!(back.parts[1], 0.0);
    }

    #[test]
    fn lists_close_in_order_and_skip_cleanly() {
        let mut buf = [0u8; 1100];
        let mut w = SnapshotWriter::new(&mut buf, 1100);
        w.header(&SnapshotHeader { tick: 50, ..SnapshotHeader::default() });
        w.own(None);
        w.zero(None);
        assert!(w.rock(&RockState::new(9, true, 0.0, 0.0), 0));
        assert!(w.object(&ObjectState::Gone { id: 3 }));
        // Lists already closed can't take more.
        assert!(!w.rock(&RockState::default(), 0));
        assert!(!w.entity(&EntityState::default(), 0));
        let n = w.finish().unwrap();
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        // Straight to the objects: everything before is skipped.
        assert_eq!(r.next_object().unwrap(), Some(ObjectState::Gone { id: 3 }));
        assert_eq!(r.next_object().unwrap(), None);
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        assert_eq!(r.next_event().unwrap(), None);
        assert_eq!(r.next_rock().unwrap().map(|x| x.id), Some(9));
        assert_eq!(r.next_rock().unwrap(), None);
        assert_eq!(r.next_entity().unwrap(), None);
        assert_eq!(r.next_object().unwrap().map(|o| o.id()), Some(3));
    }
}
