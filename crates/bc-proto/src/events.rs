//! Discrete events carried inside snapshots.
//!
//! Events repeat in every snapshot until the client acks a snapshot that contained them, so an
//! unreliable datagram channel still delivers them. `id` (the low 16 bits of the simulation's event
//! sequence) lets clients de-duplicate the repeats.

use glam::Vec3;

use crate::quant::{self, dequantize_unit, quantize_unit};
use crate::types::{Part, WeaponKind};
use crate::{BitReader, BitWriter, DecodeError, SLOT_BITS};

const KIND_BITS: u32 = 3;
const DIR_BITS: u32 = 16;
/// Beam speeds up to 16 384 m/s in 0.25 m/s steps.
const SPEED_BITS: u32 = 16;
const SPEED_MAX: f32 = 16_384.0;
const DAMAGE_BITS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A beam left a muzzle. Beams fly straight at constant velocity in space, so this one event
    /// lets every client draw the whole flight.
    BeamSpawn {
        id: u16,
        /// Effective spawn tick (the shooter's lag-compensated view time).
        tick: u32,
        shooter: u16,
        weapon: WeaponKind,
        shot_seq: u8,
        origin: Vec3,
        velocity: Vec3,
    },
    /// A hit landed on `target`'s `part`. `damage` is a fraction of that part's full armour.
    Hit { id: u16, tick: u32, target: u16, part: Part, shooter: u16, weapon: WeaponKind, damage: f32 },
    /// `victim` was destroyed.
    Kill { id: u16, tick: u32, victim: u16, killer: u16 },
    /// `slot` left this client's sensor coverage. Idempotent, so it needs no `id`.
    Leave { tick: u32, slot: u16 },
    /// Two beam sabers met: both swings were parried.
    Clash { id: u16, tick: u32, a: u16, b: u16 },
    /// A pilot's ZERO System seized (or released) control.
    Seizure { id: u16, tick: u32, pilot: u16, active: bool },
}

impl Event {
    pub fn tick(&self) -> u32 {
        match *self {
            Event::BeamSpawn { tick, .. }
            | Event::Hit { tick, .. }
            | Event::Kill { tick, .. }
            | Event::Leave { tick, .. }
            | Event::Clash { tick, .. }
            | Event::Seizure { tick, .. } => tick,
        }
    }

    /// `None` for idempotent events that need no de-duplication.
    pub fn id(&self) -> Option<u16> {
        match *self {
            Event::BeamSpawn { id, .. }
            | Event::Hit { id, .. }
            | Event::Kill { id, .. }
            | Event::Clash { id, .. }
            | Event::Seizure { id, .. } => Some(id),
            Event::Leave { .. } => None,
        }
    }

    /// Exact encoded size in bits (used for budgeting before writing).
    pub fn encoded_bits(&self) -> usize {
        let head = (KIND_BITS + 8) as usize; // kind + tick age
        head + match self {
            Event::BeamSpawn { .. } => {
                16 + SLOT_BITS as usize
                    + WeaponKind::BITS as usize
                    + 8
                    + 3 * quant::POS_BITS as usize
                    + 2 * DIR_BITS as usize
                    + SPEED_BITS as usize
            }
            Event::Hit { .. } => {
                16 + 2 * SLOT_BITS as usize
                    + Part::BITS as usize
                    + WeaponKind::BITS as usize
                    + DAMAGE_BITS as usize
            }
            Event::Kill { .. } | Event::Clash { .. } => 16 + 2 * SLOT_BITS as usize,
            Event::Leave { .. } => SLOT_BITS as usize,
            Event::Seizure { .. } => 16 + SLOT_BITS as usize + 1,
        }
    }

    /// Writes the event relative to `snapshot_tick` (events are at most 255 ticks old).
    pub fn write(&self, w: &mut BitWriter<'_>, snapshot_tick: u32) {
        let age = snapshot_tick.saturating_sub(self.tick()).min(255);
        let slot = |w: &mut BitWriter<'_>, s: u16| w.write_bits(u32::from(s), SLOT_BITS);
        match *self {
            Event::BeamSpawn { id, shooter, weapon, shot_seq, origin, velocity, .. } => {
                w.write_bits(0, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, shooter);
                w.write_bits(weapon as u32, WeaponKind::BITS);
                w.write_u8(shot_seq);
                quant::write_pos(w, origin);
                let speed = velocity.length();
                let dir = if speed > 1e-3 { velocity / speed } else { Vec3::Z };
                quant::write_dir(w, dir, DIR_BITS);
                w.write_bits(quantize_unit(speed / SPEED_MAX, SPEED_BITS), SPEED_BITS);
            }
            Event::Hit { id, target, part, shooter, weapon, damage, .. } => {
                w.write_bits(1, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, target);
                w.write_bits(part as u32, Part::BITS);
                slot(w, shooter);
                w.write_bits(weapon as u32, WeaponKind::BITS);
                w.write_bits(quantize_unit(damage, DAMAGE_BITS), DAMAGE_BITS);
            }
            Event::Kill { id, victim, killer, .. } => {
                w.write_bits(2, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, victim);
                slot(w, killer);
            }
            Event::Leave { slot: s, .. } => {
                w.write_bits(3, KIND_BITS);
                w.write_u8(age as u8);
                slot(w, s);
            }
            Event::Clash { id, a, b, .. } => {
                w.write_bits(4, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, a);
                slot(w, b);
            }
            Event::Seizure { id, pilot, active, .. } => {
                w.write_bits(5, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, pilot);
                w.write_bool(active);
            }
        }
    }

    pub fn read(r: &mut BitReader<'_>, snapshot_tick: u32) -> Result<Self, DecodeError> {
        let kind = r.read_bits(KIND_BITS);
        let tick = snapshot_tick.wrapping_sub(u32::from(r.read_u8()));
        let slot = |r: &mut BitReader<'_>| r.read_bits(SLOT_BITS) as u16;
        let e = match kind {
            0 => {
                let id = r.read_u16();
                let shooter = slot(r);
                let weapon =
                    WeaponKind::from_bits(r.read_bits(WeaponKind::BITS)).ok_or(DecodeError::Invalid)?;
                let shot_seq = r.read_u8();
                let origin = quant::read_pos(r);
                let dir = quant::read_dir(r, DIR_BITS);
                let speed = dequantize_unit(r.read_bits(SPEED_BITS), SPEED_BITS) * SPEED_MAX;
                Event::BeamSpawn { id, tick, shooter, weapon, shot_seq, origin, velocity: dir * speed }
            }
            1 => {
                let id = r.read_u16();
                let target = slot(r);
                let part = Part::from_bits(r.read_bits(Part::BITS)).ok_or(DecodeError::Invalid)?;
                let shooter = slot(r);
                let weapon =
                    WeaponKind::from_bits(r.read_bits(WeaponKind::BITS)).ok_or(DecodeError::Invalid)?;
                let damage = dequantize_unit(r.read_bits(DAMAGE_BITS), DAMAGE_BITS);
                Event::Hit { id, tick, target, part, shooter, weapon, damage }
            }
            2 => {
                let id = r.read_u16();
                let victim = slot(r);
                let killer = slot(r);
                Event::Kill { id, tick, victim, killer }
            }
            3 => Event::Leave { tick, slot: slot(r) },
            4 => {
                let id = r.read_u16();
                let a = slot(r);
                let b = slot(r);
                Event::Clash { id, tick, a, b }
            }
            5 => {
                let id = r.read_u16();
                let pilot = slot(r);
                let active = r.read_bool();
                Event::Seizure { id, tick, pilot, active }
            }
            _ => return Err(DecodeError::Invalid),
        };
        if r.overflowed() { Err(DecodeError::Truncated) } else { Ok(e) }
    }
}
