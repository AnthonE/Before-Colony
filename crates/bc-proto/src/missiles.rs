//! Missiles in snapshots.
//!
//! A beam flies straight, so one spawn event lets every client draw its whole flight. A homing
//! missile steers after its target, so it is replicated while it flies instead: each snapshot lists
//! the missiles near the client (those tracking it first), and clients extrapolate between them.
//! A missile's end arrives as a `MissileBurst` event.

use glam::Vec3;

use crate::quant;
use crate::types::WeaponKind;
use crate::{BitReader, BitWriter, DecodeError, MISSILE_BITS};

/// Missile velocities span ±this many m/s on every axis (launch speed and motor Δv on top of a
/// fast suit's own speed)...
pub const MISSILE_VEL_MAX: f32 = 4_096.0;
/// ...in 2 m/s steps: enough to extrapolate one snapshot's worth of flight.
pub const MISSILE_VEL_BITS: u32 = 12;

/// Encoded size of a missile record, in bits.
pub const MISSILE_RECORD_BITS: usize = MISSILE_BITS as usize
    + 2
    + WeaponKind::BITS as usize
    + 3
    + 3 * quant::POS_BITS as usize
    + 3 * MISSILE_VEL_BITS as usize;

/// One missile in flight, as the receiving pilot sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileState {
    /// The simulation's pool index: reused once a missile is gone, so `generation` tells the
    /// missiles on one id apart.
    pub id: u16,
    /// Low 2 bits of the pool slot's generation.
    pub generation: u8,
    pub kind: WeaponKind,
    /// Its seeker holds a target; otherwise it flies on without steering.
    pub guided: bool,
    /// It is tracking the receiving pilot.
    pub targets_you: bool,
    /// Fired by the receiving pilot's side.
    pub friendly: bool,
    pub pos: Vec3,
    pub vel: Vec3,
}

impl Default for MissileState {
    fn default() -> Self {
        Self {
            id: 0,
            generation: 0,
            kind: WeaponKind::BeamRifle,
            guided: false,
            targets_you: false,
            friendly: false,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
        }
    }
}

impl MissileState {
    pub(crate) fn write(&self, w: &mut BitWriter<'_>) {
        w.write_bits(u32::from(self.id), MISSILE_BITS);
        w.write_bits(u32::from(self.generation & 3), 2);
        w.write_bits(self.kind as u32, WeaponKind::BITS);
        w.write_bool(self.guided);
        w.write_bool(self.targets_you);
        w.write_bool(self.friendly);
        quant::write_pos(w, self.pos);
        quant::write_vec(w, self.vel, MISSILE_VEL_MAX, MISSILE_VEL_BITS);
    }

    pub(crate) fn read(r: &mut BitReader<'_>) -> Result<Self, DecodeError> {
        let id = r.read_bits(MISSILE_BITS) as u16;
        let generation = r.read_bits(2) as u8;
        let kind = WeaponKind::from_bits(r.read_bits(WeaponKind::BITS)).ok_or(DecodeError::Invalid)?;
        Ok(Self {
            id,
            generation,
            kind,
            guided: r.read_bool(),
            targets_you: r.read_bool(),
            friendly: r.read_bool(),
            pos: quant::read_pos(r),
            vel: quant::read_vec(r, MISSILE_VEL_MAX, MISSILE_VEL_BITS),
        })
    }

    /// The same record after a round trip through the wire format.
    pub fn quantized(&self) -> Self {
        let mut buf = [0u8; MISSILE_RECORD_BITS.div_ceil(8)];
        let mut w = BitWriter::new(&mut buf);
        self.write(&mut w);
        Self::read(&mut BitReader::new(&buf)).unwrap_or(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_exactly_its_stated_size_and_round_trips() {
        let m = MissileState {
            id: 1_023,
            generation: 3,
            kind: WeaponKind::BeamCannon,
            guided: true,
            targets_you: false,
            friendly: true,
            pos: Vec3::new(-31_000.0, 12.5, 4_000.0),
            vel: Vec3::new(1_200.0, -3_900.0, 0.0),
        };
        let mut buf = [0u8; 32];
        let mut w = BitWriter::new(&mut buf);
        m.write(&mut w);
        assert_eq!(w.bits_written(), MISSILE_RECORD_BITS);
        let back = MissileState::read(&mut BitReader::new(&buf)).unwrap();
        assert_eq!((back.id, back.generation, back.kind), (m.id, m.generation, m.kind));
        assert_eq!((back.guided, back.targets_you, back.friendly), (true, false, true));
        assert!((back.pos - m.pos).abs().max_element() < 0.02);
        assert!((back.vel - m.vel).abs().max_element() <= 1.01);
        assert_eq!(back, m.quantized());
    }
}
