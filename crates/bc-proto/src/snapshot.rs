//! Server → client snapshot datagram.
//!
//! Layout, in order (all bit-packed):
//!
//! | Section | Size | Notes |
//! |---|---|---|
//! | header | 116 bits | tick, input ack, input-buffer health, RTT echo, time dilation |
//! | own state | 1 + ~486 bits | full precision: the client reconciles its prediction against it |
//! | ZERO | 1 + ~203 bits | only while the pilot's ZERO System is engaged |
//! | events | `1+n` bits each, `0` ends | repeated until the client acks a snapshot containing them |
//! | entities | `1+204` bits each, `0` ends | as many prioritised contacts as fit |
//!
//! Everything must fit in [`MAX_DATAGRAM`](crate::MAX_DATAGRAM) bytes. The writer checks the budget
//! before every event or entity and never produces a partial item.

use glam::{Quat, Vec3};

use crate::events::Event;
use crate::quant::{self, dequantize_signed, dequantize_unit, quantize_signed, quantize_unit};
use crate::types::{Faction, FrameId, Part, PilotKind};
use crate::{BitReader, BitWriter, DecodeError, PACKET_KIND_BITS, PacketKind, SLOT_BITS};

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
    /// Armour left per [`Part`], 0..1.
    pub parts: [f32; Part::COUNT],
    pub zero_strain: f32,
    pub zero_mode: u8,
    pub flags: u16,
    /// Flight-model modifiers from damage and busy arms (0..1), so prediction matches the server.
    pub ambac_factor: f32,
    pub thrust_factor: f32,
    /// While dead: ticks until respawn, divided by 4.
    pub respawn_in: u8,
}

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

/// Budget-aware snapshot encoder. Call `header`, `own`, `zero`, then `event`s, `end_events`, then
/// `entity`s, then `finish`.
pub struct SnapshotWriter<'a> {
    w: BitWriter<'a>,
    tick: u32,
}

impl<'a> SnapshotWriter<'a> {
    pub fn new(buf: &'a mut [u8], max_bytes: usize) -> Self {
        Self { w: BitWriter::with_limit(buf, max_bytes), tick: 0 }
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
            w.write_bits(quantize_unit(p, 8), 8);
        }
        w.write_bits(quantize_unit(o.zero_strain, 8), 8);
        w.write_bits(u32::from(o.zero_mode & 3), 2);
        w.write_u16(o.flags);
        w.write_bits(quantize_unit(o.ambac_factor, 8), 8);
        w.write_bits(quantize_unit(o.thrust_factor, 8), 8);
        w.write_u8(o.respawn_in);
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

    /// Appends an event if it fits while leaving `keep_free_bits` for what follows. Returns whether
    /// it was written.
    pub fn event(&mut self, e: &Event, keep_free_bits: usize) -> bool {
        // 1 continuation bit + the event + both section terminators must still fit.
        let need = 1 + e.encoded_bits() + 2 + keep_free_bits;
        if self.w.bits_remaining() < need {
            return false;
        }
        self.w.write_bool(true);
        e.write(&mut self.w, self.tick);
        true
    }

    pub fn end_events(&mut self) {
        self.w.write_bool(false);
    }

    /// Appends an entity if it fits. Returns whether it was written.
    pub fn entity(&mut self, e: &EntityState) -> bool {
        if self.w.bits_remaining() < 1 + ENTITY_BITS + 1 {
            return false;
        }
        let w = &mut self.w;
        w.write_bool(true);
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

    /// Bits still free (useful for budgeting).
    pub fn bits_remaining(&self) -> usize {
        self.w.bits_remaining()
    }

    /// Terminates the entity list and returns the datagram length, or `None` on overflow.
    pub fn finish(mut self) -> Option<usize> {
        self.w.write_bool(false);
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
    Entities,
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
        if self.stage < Stage::Events {
            self.zero()?;
        }
        if self.stage != Stage::Events {
            return Ok(None);
        }
        if !self.r.read_bool() {
            self.stage = Stage::Entities;
            self.check()?;
            return Ok(None);
        }
        Event::read(&mut self.r, self.header.tick).map(Some)
    }

    /// Next entity, or `None` at the end of the packet.
    pub fn next_entity(&mut self) -> Result<Option<EntityState>, DecodeError> {
        while self.stage < Stage::Entities {
            if self.stage < Stage::Events {
                self.zero()?;
            } else {
                self.next_event()?;
            }
        }
        if self.stage != Stage::Entities {
            return Ok(None);
        }
        let r = &mut self.r;
        if !r.read_bool() {
            self.stage = Stage::Done;
            self.check()?;
            return Ok(None);
        }
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
