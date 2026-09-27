//! Discrete events carried inside snapshots.
//!
//! Events repeat in every snapshot until the client acks a snapshot that contained them, so an
//! unreliable datagram channel still delivers them. `id` (the low 16 bits of the simulation's event
//! sequence) lets clients de-duplicate the repeats.

use glam::Vec3;

use crate::quant::{self, dequantize_unit, quantize_unit};
use crate::types::{Part, WeaponKind};
use crate::{BitReader, BitWriter, CHUNK_BITS, DecodeError, MISSILE_BITS, ROCK_BITS, SLOT_BITS};

const KIND_BITS: u32 = 3;
/// Kind 7 is an extension: a sub-kind follows (0 = rock break, 1 = missile burst; the rest
/// reserved).
const EXT_BITS: u32 = 3;
const DIR_BITS: u32 = 16;
/// Beam speeds up to 16 384 m/s in 0.25 m/s steps.
const SPEED_BITS: u32 = 16;
const SPEED_MAX: f32 = 16_384.0;
const DAMAGE_BITS: u32 = 10;

/// How a missile's flight ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BurstCause {
    /// It struck a suit.
    Hit = 0,
    /// Its proximity fuse went off beside a suit.
    Proximity = 1,
    /// Its flight time ran out.
    Expired = 2,
    /// It met the colony or a rock.
    Blocked = 3,
}

impl BurstCause {
    pub const BITS: u32 = 2;

    pub fn from_bits(v: u32) -> Self {
        match v {
            0 => BurstCause::Hit,
            1 => BurstCause::Proximity,
            2 => BurstCause::Expired,
            _ => BurstCause::Blocked,
        }
    }
}

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
    /// `victim` was destroyed; its wreck is now the hulk chunk `hulk` (`NO_CHUNK` if none).
    Kill { id: u16, tick: u32, victim: u16, killer: u16, hulk: u16 },
    /// `slot` left this client's sensor coverage. Idempotent, so it needs no `id`.
    Leave { tick: u32, slot: u16 },
    /// Two beam sabers met: both swings were parried.
    Clash { id: u16, tick: u32, a: u16, b: u16 },
    /// A pilot's ZERO System seized (or released) control.
    Seizure { id: u16, tick: u32, pilot: u16, active: bool },
    /// `part` came off suit `source` (or, `from_hulk`, off hulk chunk `source`) and is now the limb
    /// chunk `chunk`.
    Detach { id: u16, tick: u32, source: u16, from_hulk: bool, part: Part, chunk: u16 },
    /// Rock `rock` shattered (its ore scattered as chunks); `by` broke it.
    RockBreak { id: u16, tick: u32, rock: u16, by: u16 },
    /// Missile `missile` (a pool id, see [`MissileState`](crate::MissileState)) ended at `pos`.
    /// Any damage it did arrives as `Hit` events.
    MissileBurst { id: u16, tick: u32, missile: u16, pos: Vec3, cause: BurstCause },
}

impl Event {
    pub fn tick(&self) -> u32 {
        match *self {
            Event::BeamSpawn { tick, .. }
            | Event::Hit { tick, .. }
            | Event::Kill { tick, .. }
            | Event::Leave { tick, .. }
            | Event::Clash { tick, .. }
            | Event::Seizure { tick, .. }
            | Event::Detach { tick, .. }
            | Event::RockBreak { tick, .. }
            | Event::MissileBurst { tick, .. } => tick,
        }
    }

    /// `None` for idempotent events that need no de-duplication.
    pub fn id(&self) -> Option<u16> {
        match *self {
            Event::BeamSpawn { id, .. }
            | Event::Hit { id, .. }
            | Event::Kill { id, .. }
            | Event::Clash { id, .. }
            | Event::Seizure { id, .. }
            | Event::Detach { id, .. }
            | Event::RockBreak { id, .. }
            | Event::MissileBurst { id, .. } => Some(id),
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
            Event::Kill { .. } => 16 + 2 * SLOT_BITS as usize + CHUNK_BITS as usize,
            Event::Clash { .. } => 16 + 2 * SLOT_BITS as usize,
            Event::Leave { .. } => SLOT_BITS as usize,
            Event::Seizure { .. } => 16 + SLOT_BITS as usize + 1,
            Event::Detach { from_hulk, .. } => {
                16 + 1
                    + if *from_hulk { CHUNK_BITS } else { SLOT_BITS } as usize
                    + Part::BITS as usize
                    + CHUNK_BITS as usize
            }
            Event::RockBreak { .. } => EXT_BITS as usize + 16 + ROCK_BITS as usize + SLOT_BITS as usize,
            Event::MissileBurst { .. } => {
                EXT_BITS as usize
                    + 16
                    + MISSILE_BITS as usize
                    + 3 * quant::POS_BITS as usize
                    + BurstCause::BITS as usize
            }
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
            Event::Kill { id, victim, killer, hulk, .. } => {
                w.write_bits(2, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                slot(w, victim);
                slot(w, killer);
                w.write_bits(u32::from(hulk), CHUNK_BITS);
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
            Event::Detach { id, source, from_hulk, part, chunk, .. } => {
                w.write_bits(6, KIND_BITS);
                w.write_u8(age as u8);
                w.write_u16(id);
                w.write_bool(from_hulk);
                w.write_bits(u32::from(source), if from_hulk { CHUNK_BITS } else { SLOT_BITS });
                w.write_bits(part as u32, Part::BITS);
                w.write_bits(u32::from(chunk), CHUNK_BITS);
            }
            Event::RockBreak { id, rock, by, .. } => {
                w.write_bits(7, KIND_BITS);
                w.write_u8(age as u8);
                w.write_bits(0, EXT_BITS);
                w.write_u16(id);
                w.write_bits(u32::from(rock), ROCK_BITS);
                slot(w, by);
            }
            Event::MissileBurst { id, missile, pos, cause, .. } => {
                w.write_bits(7, KIND_BITS);
                w.write_u8(age as u8);
                w.write_bits(1, EXT_BITS);
                w.write_u16(id);
                w.write_bits(u32::from(missile), MISSILE_BITS);
                quant::write_pos(w, pos);
                w.write_bits(cause as u32, BurstCause::BITS);
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
                let hulk = r.read_bits(CHUNK_BITS) as u16;
                Event::Kill { id, tick, victim, killer, hulk }
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
            6 => {
                let id = r.read_u16();
                let from_hulk = r.read_bool();
                let source = r.read_bits(if from_hulk { CHUNK_BITS } else { SLOT_BITS }) as u16;
                let part = Part::from_bits(r.read_bits(Part::BITS)).ok_or(DecodeError::Invalid)?;
                let chunk = r.read_bits(CHUNK_BITS) as u16;
                Event::Detach { id, tick, source, from_hulk, part, chunk }
            }
            7 => match r.read_bits(EXT_BITS) {
                0 => {
                    let id = r.read_u16();
                    let rock = r.read_bits(ROCK_BITS) as u16;
                    Event::RockBreak { id, tick, rock, by: slot(r) }
                }
                1 => {
                    let id = r.read_u16();
                    let missile = r.read_bits(MISSILE_BITS) as u16;
                    let pos = quant::read_pos(r);
                    let cause = BurstCause::from_bits(r.read_bits(BurstCause::BITS));
                    Event::MissileBurst { id, tick, missile, pos, cause }
                }
                _ => return Err(DecodeError::Invalid),
            },
            _ => return Err(DecodeError::Invalid),
        };
        if r.overflowed() { Err(DecodeError::Truncated) } else { Ok(e) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_at_its_stated_size() {
        let all = [
            Event::BeamSpawn {
                id: 1,
                tick: 90,
                shooter: 4,
                weapon: WeaponKind::TwinBusterRifle,
                shot_seq: 9,
                origin: Vec3::ZERO,
                velocity: Vec3::new(0.0, 0.0, 8_000.0),
            },
            Event::Hit {
                id: 2,
                tick: 91,
                target: 5,
                part: Part::Legs,
                shooter: 4,
                weapon: WeaponKind::BeamSaber,
                damage: 1.0,
            },
            Event::Kill { id: 3, tick: 92, victim: 5, killer: 4, hulk: 812 },
            Event::Leave { tick: 93, slot: 1_000 },
            Event::Clash { id: 4, tick: 94, a: 1, b: 2 },
            Event::Seizure { id: 5, tick: 95, pilot: 7, active: true },
            Event::Detach { id: 6, tick: 96, source: 5, from_hulk: false, part: Part::ArmL, chunk: 13 },
            Event::Detach { id: 7, tick: 97, source: 812, from_hulk: true, part: Part::Head, chunk: 14 },
            Event::RockBreak { id: 8, tick: 98, rock: 1_022, by: 4 },
            Event::MissileBurst {
                id: 9,
                tick: 99,
                missile: 1_000,
                pos: Vec3::new(1_000.0, -2_000.0, 3_000.0),
                cause: BurstCause::Proximity,
            },
        ];
        for e in all {
            let mut buf = [0u8; 64];
            let mut w = BitWriter::new(&mut buf);
            e.write(&mut w, 100);
            assert_eq!(w.bits_written(), e.encoded_bits());
            let back = Event::read(&mut BitReader::new(&buf), 100).unwrap();
            if matches!(e, Event::BeamSpawn { .. } | Event::MissileBurst { .. }) {
                assert_eq!((back.id(), back.tick()), (e.id(), e.tick())); // (its vectors are quantized)
            } else {
                assert_eq!(back, e);
            }
        }
    }

    #[test]
    fn unknown_extension_sub_kinds_are_invalid() {
        for sub in 2..8u32 {
            let mut buf = [0u8; 16];
            let mut w = BitWriter::new(&mut buf);
            w.write_bits(7, KIND_BITS);
            w.write_u8(0);
            w.write_bits(sub, EXT_BITS);
            assert_eq!(Event::read(&mut BitReader::new(&buf), 10), Err(DecodeError::Invalid));
        }
    }
}
