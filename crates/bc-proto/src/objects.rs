//! Rocks and salvage objects in snapshots.
//!
//! - **Rocks.** Clients generate the debris field from the Welcome's seed, so snapshots carry only
//!   the rocks whose state has changed (mined, shattered, regrown), repeated until acked.
//! - **Chunks**: loose ore, limbs blown off suits, and hulks (what's left of destroyed suits). A free
//!   chunk moves on a closed-form [`Segment`] (constant velocity and spin from its start tick; space
//!   has no drag), so one record describes its motion until something changes it. A held chunk
//!   rides its holder's hand.

use glam::{Quat, Vec3};

use crate::quant;
use crate::types::{FrameId, Part};
use crate::{BitReader, BitWriter, CARGO_KINDS, CHUNK_BITS, DecodeError, ROCK_BITS, SLOT_BITS};

/// One rock's state (sent when it changes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RockState {
    pub id: u16,
    /// Shattered: gone (no collisions, nothing to draw) until it regrows.
    pub destroyed: bool,
    /// Structure left in eighths (0 = none, 7 = intact).
    pub hp: u8,
    /// Ore left, 0..15 of what it started with.
    pub ore: u8,
}

/// Encoded size of a rock record, in bits.
pub const ROCK_RECORD_BITS: usize = ROCK_BITS as usize + 1 + 3 + 4;

impl RockState {
    /// From fractions of the rock's full structure and ore: any left at all shows as at least 1.
    pub fn new(id: u16, destroyed: bool, hp: f32, ore: f32) -> Self {
        let grade = |f: f32, top: u8| if f <= 0.0 { 0 } else { 1 + (f.min(1.0) * f32::from(top - 1)) as u8 };
        Self { id, destroyed, hp: grade(hp, 7), ore: grade(ore, 15) }
    }

    pub(crate) fn write(&self, w: &mut BitWriter<'_>) {
        w.write_bits(u32::from(self.id), ROCK_BITS);
        w.write_bool(self.destroyed);
        w.write_bits(u32::from(self.hp.min(7)), 3);
        w.write_bits(u32::from(self.ore.min(15)), 4);
    }

    pub(crate) fn read(r: &mut BitReader<'_>) -> Self {
        Self {
            id: r.read_bits(ROCK_BITS) as u16,
            destroyed: r.read_bool(),
            hp: r.read_bits(3) as u8,
            ore: r.read_bits(4) as u8,
        }
    }
}

/// What a chunk is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkKind {
    /// Loose ore of kind `ore` (`0..CARGO_KINDS`).
    Ore { ore: u8 },
    /// A limb blown off a suit.
    Limb { frame: FrameId, part: Part },
    /// What's left of a destroyed suit: `parts` has a bit per [`Part`] still on it.
    Hulk { frame: FrameId, parts: u8 },
}

/// A chunk's identity and build: what it is, a seed for its look, and its mass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkDesc {
    pub kind: ChunkKind,
    pub seed: u8,
    /// kg, in 10 kg steps up to 40.95 t (the simulation's chunk masses are multiples of 10 kg).
    pub mass_kg: u32,
}

impl Default for ChunkDesc {
    fn default() -> Self {
        Self { kind: ChunkKind::Ore { ore: 0 }, seed: 0, mass_kg: 0 }
    }
}

const MASS_BITS: u32 = 12;
const MASS_STEP: u32 = 10;

impl ChunkDesc {
    pub fn encoded_bits(&self) -> usize {
        2 + match self.kind {
            ChunkKind::Ore { .. } => 2,
            ChunkKind::Limb { .. } => (FrameId::BITS + Part::BITS) as usize,
            ChunkKind::Hulk { .. } => FrameId::BITS as usize + Part::COUNT,
        } + 8
            + MASS_BITS as usize
    }

    fn write(&self, w: &mut BitWriter<'_>) {
        match self.kind {
            ChunkKind::Ore { ore } => {
                w.write_bits(0, 2);
                w.write_bits(u32::from(ore) % CARGO_KINDS as u32, 2);
            }
            ChunkKind::Limb { frame, part } => {
                w.write_bits(1, 2);
                w.write_bits(frame as u32, FrameId::BITS);
                w.write_bits(part as u32, Part::BITS);
            }
            ChunkKind::Hulk { frame, parts } => {
                w.write_bits(2, 2);
                w.write_bits(frame as u32, FrameId::BITS);
                w.write_bits(u32::from(parts), Part::COUNT as u32);
            }
        }
        w.write_u8(self.seed);
        w.write_bits((self.mass_kg / MASS_STEP).min((1 << MASS_BITS) - 1), MASS_BITS);
    }

    fn read(r: &mut BitReader<'_>) -> Result<Self, DecodeError> {
        let frame = |r: &mut BitReader<'_>| {
            FrameId::from_bits(r.read_bits(FrameId::BITS)).ok_or(DecodeError::Invalid)
        };
        let kind = match r.read_bits(2) {
            0 => ChunkKind::Ore { ore: r.read_bits(2) as u8 },
            1 => {
                let frame = frame(r)?;
                ChunkKind::Limb {
                    frame,
                    part: Part::from_bits(r.read_bits(Part::BITS)).ok_or(DecodeError::Invalid)?,
                }
            }
            2 => ChunkKind::Hulk { frame: frame(r)?, parts: r.read_bits(Part::COUNT as u32) as u8 },
            _ => return Err(DecodeError::Invalid),
        };
        Ok(Self { kind, seed: r.read_u8(), mass_kg: r.read_bits(MASS_BITS) * MASS_STEP })
    }
}

/// A free chunk's motion from tick `t0` on: constant velocity and spin. The simulation moves
/// chunks on exactly the [`quantized`](Segment::quantized) segment it sends, and evaluates it at
/// whole ticks with the same arithmetic as the clients (`bc_sim::chunks`), so both agree to the bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub t0: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    /// World-frame angular velocity, rad/s.
    pub spin: Vec3,
}

impl Default for Segment {
    fn default() -> Self {
        Self { t0: 0, pos: Vec3::ZERO, vel: Vec3::ZERO, rot: Quat::IDENTITY, spin: Vec3::ZERO }
    }
}

/// Chunks spin at most this fast on the wire, rad/s.
pub const SPIN_MAX: f32 = 4.0;
const SPIN_BITS: u32 = 10;
const SEGMENT_ROT_BITS: u32 = 10;
/// A segment's start travels as its age in ticks (up to ~36 minutes).
const AGE_BITS: u32 = 16;
const SEGMENT_BITS: usize = AGE_BITS as usize
    + 3 * quant::POS_BITS as usize
    + 3 * quant::VEL_BITS as usize
    + 2
    + 3 * SEGMENT_ROT_BITS as usize
    + 3 * SPIN_BITS as usize;

impl Segment {
    fn write_motion(&self, w: &mut BitWriter<'_>) {
        quant::write_pos(w, self.pos);
        quant::write_vec(w, self.vel, quant::VEL_MAX, quant::VEL_BITS);
        quant::write_quat(w, self.rot, SEGMENT_ROT_BITS);
        quant::write_vec(w, self.spin, SPIN_MAX, SPIN_BITS);
    }

    fn read_motion(r: &mut BitReader<'_>, t0: u32) -> Self {
        Self {
            t0,
            pos: quant::read_pos(r),
            vel: quant::read_vec(r, quant::VEL_MAX, quant::VEL_BITS),
            rot: quant::read_quat(r, SEGMENT_ROT_BITS),
            spin: quant::read_vec(r, SPIN_MAX, SPIN_BITS),
        }
    }

    fn write(&self, w: &mut BitWriter<'_>, snapshot_tick: u32) {
        let age = snapshot_tick.saturating_sub(self.t0).min((1 << AGE_BITS) - 1);
        w.write_bits(age, AGE_BITS);
        self.write_motion(w);
    }

    fn read(r: &mut BitReader<'_>, snapshot_tick: u32) -> Self {
        let t0 = snapshot_tick.wrapping_sub(r.read_bits(AGE_BITS));
        Self::read_motion(r, t0)
    }

    /// The same segment after a round trip through the wire format.
    pub fn quantized(&self) -> Self {
        let mut buf = [0u8; 32];
        let mut w = BitWriter::new(&mut buf);
        self.write_motion(&mut w);
        debug_assert!(!w.overflowed());
        Self::read_motion(&mut BitReader::new(&buf), self.t0)
    }
}

/// A held chunk's rotation relative to its holder travels at this precision.
const HELD_ROT_BITS: u32 = 9;

/// One chunk as the receiving pilot sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectState {
    /// Forget chunk `id`: it's gone, or out of this pilot's range.
    Gone { id: u16 },
    /// Drifting free.
    Free { id: u16, generation: u8, desc: ChunkDesc, seg: Segment },
    /// In suit `holder`'s hand (the right one if `right`, else the left), turned by `rot`
    /// relative to the holder.
    Held { id: u16, generation: u8, desc: ChunkDesc, holder: u16, right: bool, rot: Quat },
}

impl ObjectState {
    pub fn id(&self) -> u16 {
        match *self {
            ObjectState::Gone { id } | ObjectState::Free { id, .. } | ObjectState::Held { id, .. } => id,
        }
    }

    /// Exact encoded size in bits (without the continuation bit).
    pub fn encoded_bits(&self) -> usize {
        2 + CHUNK_BITS as usize
            + match self {
                ObjectState::Gone { .. } => 0,
                ObjectState::Free { desc, .. } => 2 + desc.encoded_bits() + SEGMENT_BITS,
                ObjectState::Held { desc, .. } => {
                    2 + desc.encoded_bits() + SLOT_BITS as usize + 1 + 2 + 3 * HELD_ROT_BITS as usize
                }
            }
    }

    /// The largest record (a free hulk).
    pub const MAX_BITS: usize = 2 + CHUNK_BITS as usize + 2 + (2 + 4 + 6 + 8 + 12) + SEGMENT_BITS;

    pub(crate) fn write(&self, w: &mut BitWriter<'_>, snapshot_tick: u32) {
        let id = |w: &mut BitWriter<'_>, id: u16| w.write_bits(u32::from(id), CHUNK_BITS);
        match *self {
            ObjectState::Gone { id: i } => {
                w.write_bits(0, 2);
                id(w, i);
            }
            ObjectState::Free { id: i, generation, desc, seg } => {
                w.write_bits(1, 2);
                id(w, i);
                w.write_bits(u32::from(generation & 3), 2);
                desc.write(w);
                seg.write(w, snapshot_tick);
            }
            ObjectState::Held { id: i, generation, desc, holder, right, rot } => {
                w.write_bits(2, 2);
                id(w, i);
                w.write_bits(u32::from(generation & 3), 2);
                desc.write(w);
                w.write_bits(u32::from(holder), SLOT_BITS);
                w.write_bool(right);
                quant::write_quat(w, rot, HELD_ROT_BITS);
            }
        }
    }

    pub(crate) fn read(r: &mut BitReader<'_>, snapshot_tick: u32) -> Result<Self, DecodeError> {
        let kind = r.read_bits(2);
        let id = r.read_bits(CHUNK_BITS) as u16;
        Ok(match kind {
            0 => ObjectState::Gone { id },
            1 => {
                let generation = r.read_bits(2) as u8;
                let desc = ChunkDesc::read(r)?;
                ObjectState::Free { id, generation, desc, seg: Segment::read(r, snapshot_tick) }
            }
            2 => {
                let generation = r.read_bits(2) as u8;
                let desc = ChunkDesc::read(r)?;
                let holder = r.read_bits(SLOT_BITS) as u16;
                let right = r.read_bool();
                ObjectState::Held {
                    id,
                    generation,
                    desc,
                    holder,
                    right,
                    rot: quant::read_quat(r, HELD_ROT_BITS),
                }
            }
            _ => return Err(DecodeError::Invalid),
        })
    }
}

/// Held rotations, as the holder's client will decode them.
pub fn quantize_held_rot(q: Quat) -> Quat {
    let mut buf = [0u8; 8];
    let mut w = BitWriter::new(&mut buf);
    quant::write_quat(&mut w, q, HELD_ROT_BITS);
    quant::read_quat(&mut BitReader::new(&buf), HELD_ROT_BITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_object(o: &ObjectState, tick: u32) -> ObjectState {
        let mut buf = [0u8; 64];
        let mut w = BitWriter::new(&mut buf);
        o.write(&mut w, tick);
        assert_eq!(w.bits_written(), o.encoded_bits());
        ObjectState::read(&mut BitReader::new(&buf), tick).unwrap()
    }

    #[test]
    fn records_round_trip() {
        let desc = ChunkDesc {
            kind: ChunkKind::Hulk { frame: FrameId::Virgo, parts: 0b10_1011 },
            seed: 200,
            mass_kg: 9_310,
        };
        let seg = Segment {
            t0: 9_000,
            pos: Vec3::new(-1_234.5, 800.25, 17.0),
            vel: Vec3::new(3.0, -0.5, 40.25),
            rot: Quat::from_rotation_y(1.0),
            spin: Vec3::new(0.2, -1.0, 3.9),
        }
        .quantized();
        let free = ObjectState::Free { id: 1_022, generation: 3, desc, seg };
        assert_eq!(round_trip_object(&free, 9_500), free);
        assert_eq!(free.encoded_bits(), ObjectState::MAX_BITS);
        let held = ObjectState::Held {
            id: 4,
            generation: 1,
            desc: ChunkDesc {
                kind: ChunkKind::Limb { frame: FrameId::WingZero, part: Part::ArmR },
                seed: 7,
                mass_kg: 720,
            },
            holder: 311,
            right: false,
            rot: quantize_held_rot(Quat::from_rotation_x(0.4)),
        };
        assert_eq!(round_trip_object(&held, 9_500), held);
        let gone = ObjectState::Gone { id: 77 };
        assert_eq!(round_trip_object(&gone, 1), gone);
        let ore = ChunkDesc { kind: ChunkKind::Ore { ore: 3 }, seed: 1, mass_kg: 150 };
        assert_eq!(ore.encoded_bits(), 2 + 2 + 8 + 12);
    }

    #[test]
    fn quantized_segments_are_fixed_points() {
        let seg = Segment {
            t0: 5,
            pos: Vec3::new(1.0, 2.0, 3.0),
            vel: Vec3::splat(-7.3),
            rot: Quat::from_rotation_z(2.0),
            spin: Vec3::X,
        }
        .quantized();
        assert_eq!(seg.quantized(), seg);
    }

    #[test]
    fn rock_grades() {
        assert_eq!(RockState::new(3, false, 1.0, 1.0), RockState { id: 3, destroyed: false, hp: 7, ore: 15 });
        let scraped = RockState::new(3, false, 0.001, 0.001);
        assert_eq!((scraped.hp, scraped.ore), (1, 1));
        assert_eq!(RockState::new(3, true, 0.0, 0.0).hp, 0);
    }
}
