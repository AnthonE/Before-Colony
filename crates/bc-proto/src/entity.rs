//! A suit's record on the wire as the integer codes it travels as ([`QEntity`]): whole, or as a
//! delta against an earlier record of the same suit that the client acked (Gaffer On Games'
//! snapshot compression; `docs/NETCODE.md`, `docs/PROTOCOL.md`).
//!
//! Both ends hold codes, never floats, so a delta taken on the server is undone on the client to
//! the bit: what the client rebuilds is exactly what a whole record would have said.

use crate::quant::{self, dequantize_centered, dequantize_signed, quantize_centered, quantize_signed};
use crate::snapshot::{ENTITY_AIM_BITS, ENTITY_ROT_BITS, EntityState, RiderOn, ent_flags};
use crate::types::{BodyRef, Faction, FrameId, Part, PilotKind, RIDER_VEL_BITS, RIDER_VEL_MAX};
use crate::{BitReader, BitWriter, DecodeError, SECTOR_HALF_EXTENT, SLOT_BITS};

/// Bits of a delta record's age: how many ticks before the snapshot's its baseline is (1 to 255).
pub const BACK_BITS: u32 = 8;
/// How many records of each suit a client keeps (the newest it took), and so how far back a
/// baseline may be: a sector takes a delta only against a record it sent fewer than this many
/// records of the suit ago. The client takes a stream's datagrams in order and each holds a suit
/// once, so what it took since is fewer still, and the baseline is among those it kept.
pub const KEPT: usize = 16;
/// The tick rate a delta's position is carried on at. Both ends work it out alike from the codes,
/// so a different rate would cost bits, never exactness.
const HZ: i64 = 30;

/// A suit's record as codes: what [`EntityState`] says, quantized as the wire carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QEntity {
    pub slot: u16,
    /// The low two bits of the suit's generation.
    pub generation: u8,
    pub frame: FrameId,
    pub faction: Faction,
    pub pilot: PilotKind,
    pub on: Option<RiderOn>,
    /// Position codes: [`quant::POS_BITS`] over the sector, or a rider's body's `local_bits` over
    /// its `local_max` (`quant::quantize_signed`'s grid either way).
    pub pos: [u32; 3],
    /// Velocity codes: [`quant::VEL_BITS`] on the signed grid, or a rider's [`RIDER_VEL_BITS`]
    /// centered (two's complement).
    pub vel: [i32; 3],
    /// Rotation: the largest component's index and the other three ([`quant::quat_code`]).
    pub rot_largest: u8,
    pub rot: [u32; 3],
    /// Aim: its octahedral code ([`quant::dir_code`]).
    pub aim: [u32; 2],
    pub flags: u16,
    pub parts: [u8; Part::COUNT],
}

impl Default for QEntity {
    fn default() -> Self {
        Self::quantize(&EntityState::default())
    }
}

fn pos_bits(on: Option<RiderOn>) -> u32 {
    on.map_or(quant::POS_BITS, |on| on.body.local_bits())
}

fn vel_bits(on: Option<RiderOn>) -> u32 {
    if on.is_some() { RIDER_VEL_BITS } else { quant::VEL_BITS }
}

fn max_code(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

impl QEntity {
    /// The codes a record of `e` travels as.
    pub fn quantize(e: &EntityState) -> Self {
        let (pos, vel) = match e.on {
            None => (
                e.pos.to_array().map(|c| quantize_signed(c, SECTOR_HALF_EXTENT, quant::POS_BITS)),
                e.vel.to_array().map(|c| quantize_signed(c, quant::VEL_MAX, quant::VEL_BITS) as i32),
            ),
            Some(on) => (
                e.pos.to_array().map(|c| quantize_signed(c, on.body.local_max(), on.body.local_bits())),
                e.vel.to_array().map(|c| quantize_centered(c, RIDER_VEL_MAX, RIDER_VEL_BITS)),
            ),
        };
        let (rot_largest, rot) = quant::quat_code(e.rot, ENTITY_ROT_BITS);
        Self {
            slot: e.slot,
            generation: e.generation & 3,
            frame: e.frame,
            faction: e.faction,
            pilot: e.pilot,
            on: e.on,
            pos,
            vel,
            rot_largest,
            rot,
            aim: quant::dir_code(e.aim, ENTITY_AIM_BITS),
            flags: e.flags,
            parts: e.parts.map(|p| p.min(7)),
        }
    }

    /// What the codes say.
    pub fn state(&self) -> EntityState {
        let (pos, vel) = match self.on {
            None => (
                self.pos.map(|c| dequantize_signed(c, SECTOR_HALF_EXTENT, quant::POS_BITS)),
                self.vel.map(|c| dequantize_signed(c as u32, quant::VEL_MAX, quant::VEL_BITS)),
            ),
            Some(on) => (
                self.pos.map(|c| dequantize_signed(c, on.body.local_max(), on.body.local_bits())),
                self.vel.map(|c| dequantize_centered(c, RIDER_VEL_MAX, RIDER_VEL_BITS)),
            ),
        };
        EntityState {
            slot: self.slot,
            generation: self.generation,
            frame: self.frame,
            faction: self.faction,
            pilot: self.pilot,
            on: self.on,
            pos: pos.into(),
            rot: quant::quat_from_code(self.rot_largest, self.rot, ENTITY_ROT_BITS),
            vel: vel.into(),
            aim: quant::dir_from_code(self.aim, ENTITY_AIM_BITS),
            flags: self.flags,
            parts: self.parts,
        }
    }

    /// Whether a delta can be taken against `base`: the same suit, life and body.
    pub fn same_suit(&self, base: &QEntity) -> bool {
        self.slot == base.slot
            && self.generation == base.generation
            && self.frame == base.frame
            && self.faction == base.faction
            && self.pilot == base.pilot
            && self.on.map(|on| on.body) == base.on.map(|on| on.body)
    }

    /// Bits of the whole record, the slot and the delta bit included.
    pub fn absolute_bits(&self) -> usize {
        crate::snapshot::whole_record_bits(self.on)
    }

    /// Writes the record after its slot and delta bit (`0`): everything, in codes.
    pub(crate) fn write_absolute(&self, w: &mut BitWriter<'_>) {
        w.write_bits(u32::from(self.generation & 3), 2);
        w.write_bits(self.frame as u32, FrameId::BITS);
        w.write_bits(self.faction as u32, Faction::BITS);
        w.write_bits(self.pilot as u32, PilotKind::BITS);
        w.write_bool(self.on.is_some());
        if let Some(on) = self.on {
            on.body.write(w);
            w.write_bool(on.aloft);
        }
        let bits = pos_bits(self.on);
        for c in self.pos {
            w.write_bits(c.min(max_code(bits)), bits);
        }
        write_rot(w, self.rot_largest, self.rot);
        write_vel(w, self.on, self.vel);
        for c in self.aim {
            w.write_bits(c, ENTITY_AIM_BITS);
        }
        w.write_bits(u32::from(self.flags), ent_flags::BITS);
        for p in self.parts {
            w.write_bits(u32::from(p.min(7)), 3);
        }
    }

    /// Reads a whole record of `slot`, after its delta bit.
    pub(crate) fn read_absolute(r: &mut BitReader<'_>, slot: u16) -> Result<Self, DecodeError> {
        let generation = r.read_bits(2) as u8;
        let frame = FrameId::from_bits(r.read_bits(FrameId::BITS)).ok_or(DecodeError::Invalid)?;
        let faction = Faction::from_bits(r.read_bits(Faction::BITS));
        let pilot = PilotKind::from_bits(r.read_bits(PilotKind::BITS));
        let on = if r.read_bool() {
            Some(RiderOn { body: BodyRef::read(r)?, aloft: r.read_bool() })
        } else {
            None
        };
        let bits = pos_bits(on);
        let pos = [r.read_bits(bits), r.read_bits(bits), r.read_bits(bits)];
        let (rot_largest, rot) = read_rot(r);
        let vel = read_vel(r, on);
        let aim = [r.read_bits(ENTITY_AIM_BITS), r.read_bits(ENTITY_AIM_BITS)];
        let flags = r.read_bits(ent_flags::BITS) as u16;
        let mut parts = [0u8; Part::COUNT];
        for p in &mut parts {
            *p = r.read_bits(3) as u8;
        }
        Ok(Self {
            slot,
            generation,
            frame,
            faction,
            pilot,
            on,
            pos,
            vel,
            rot_largest,
            rot,
            aim,
            flags,
            parts,
        })
    }
}

fn write_rot(w: &mut BitWriter<'_>, largest: u8, codes: [u32; 3]) {
    w.write_bits(u32::from(largest & 3), 2);
    for c in codes {
        w.write_bits(c, ENTITY_ROT_BITS);
    }
}

fn read_rot(r: &mut BitReader<'_>) -> (u8, [u32; 3]) {
    let largest = r.read_bits(2) as u8;
    (largest, [r.read_bits(ENTITY_ROT_BITS), r.read_bits(ENTITY_ROT_BITS), r.read_bits(ENTITY_ROT_BITS)])
}

fn write_vel(w: &mut BitWriter<'_>, on: Option<RiderOn>, vel: [i32; 3]) {
    for c in vel {
        match on {
            None => w.write_bits(c as u32, quant::VEL_BITS),
            Some(_) => w.write_i32(c, RIDER_VEL_BITS),
        }
    }
}

fn read_vel(r: &mut BitReader<'_>, on: Option<RiderOn>) -> [i32; 3] {
    let mut out = [0i32; 3];
    for c in &mut out {
        *c = match on {
            None => r.read_bits(quant::VEL_BITS) as i32,
            Some(_) => r.read_i32(RIDER_VEL_BITS),
        };
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Deltas
// ---------------------------------------------------------------------------------------------

/// How a delta carries a group of codes: unchanged, as small or medium differences, or whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Same,
    Small,
    Medium,
    Whole,
}

impl Mode {
    fn code(self) -> u32 {
        self as u32
    }

    fn from_code(c: u32) -> Self {
        match c & 3 {
            0 => Mode::Same,
            1 => Mode::Small,
            2 => Mode::Medium,
            _ => Mode::Whole,
        }
    }

    /// The narrowest mode `diffs` fit, given the small and medium widths in bits.
    fn of(diffs: &[i64], small: u32, medium: u32) -> Self {
        let fits = |bits: u32| {
            let lim = 1i64 << (bits - 1);
            diffs.iter().all(|&d| (-lim..lim).contains(&d))
        };
        if diffs.iter().all(|&d| d == 0) {
            Mode::Same
        } else if fits(small) {
            Mode::Small
        } else if fits(medium) {
            Mode::Medium
        } else {
            Mode::Whole
        }
    }
}

/// Widths of the small and medium differences: velocity, position (from where it was carried to),
/// rotation and aim.
const VEL_SMALL: u32 = 5;
const VEL_MEDIUM: u32 = 9;
const POS_SMALL: u32 = 6;
const POS_MEDIUM: u32 = 12;
const ROT_SMALL: u32 = 5;
const ROT_MEDIUM: u32 = 8;
const AIM_SMALL: u32 = 5;
const AIM_MEDIUM: u32 = 7;

/// Integer division rounding half away from zero.
fn div_round(n: i64, d: i64) -> i64 {
    if n >= 0 { (n + d / 2) / d } else { -((-n + d / 2) / d) }
}

/// Where `base`'s position codes come to after `dt` ticks at the mean of its velocity and `vel`'s
/// (exact for a steady acceleration), clamped to the grid. Integers only, alike on both ends.
fn carried(base: &QEntity, vel: [i32; 3], dt: u32) -> [i64; 3] {
    let dt = i64::from(dt);
    let bits = pos_bits(base.on);
    let top = i64::from(max_code(bits));
    let mut out = [0i64; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let (num, den) = match base.on {
            // v = (2c − m_v)/m_v · VEL_MAX; a code is 2·HALF/m_p metres.
            None => {
                let mv = i64::from(max_code(quant::VEL_BITS));
                let mp = i64::from(max_code(quant::POS_BITS));
                let sum = i64::from(base.vel[k]) + i64::from(vel[k]) - mv;
                let vmax = quant::VEL_MAX as i64;
                let half = SECTOR_HALF_EXTENT as i64;
                (sum * vmax * dt * mp, mv * HZ * 2 * half)
            }
            // v = c/m_c · RIDER_VEL_MAX (centered); a code is 2·L/m_p metres.
            Some(on) => {
                let mc = i64::from(max_code(RIDER_VEL_BITS - 1));
                let mp = i64::from(max_code(bits));
                let sum = i64::from(base.vel[k]) + i64::from(vel[k]);
                let vmax = RIDER_VEL_MAX as i64;
                let l = on.body.local_max() as i64;
                (sum * vmax * dt * mp, mc * HZ * 2 * 2 * l)
            }
        };
        *o = (i64::from(base.pos[k]) + div_round(num, den)).clamp(0, top);
    }
    out
}

/// A record's delta against a baseline, worked out once: what [`bits`](Self::bits) counts is what
/// [`write`](Self::write) writes.
#[derive(Clone, Copy, Debug)]
pub struct Delta {
    back: u8,
    rider: bool,
    aloft: bool,
    vel: (Mode, [i64; 3]),
    pos: (Mode, [i64; 3]),
    rot: (Mode, [i64; 3]),
    aim: (Mode, [i64; 2]),
    cur: QEntity,
    flags_same: bool,
    parts_same: bool,
}

impl Delta {
    /// `cur` against `base`, `back` ticks older (1 to 255). `None` if it can't be: another suit,
    /// life or body, or a baseline too old.
    pub fn plan(cur: &QEntity, base: &QEntity, back: u32) -> Option<Self> {
        if !(1..=255).contains(&back) || !cur.same_suit(base) {
            return None;
        }
        let d3 = |a: [i64; 3], b: [i64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let wide = |a: [u32; 3]| a.map(i64::from);
        let dv = d3(cur.vel.map(i64::from), base.vel.map(i64::from));
        let vel = (Mode::of(&dv, VEL_SMALL, VEL_MEDIUM), dv);
        let dp = d3(wide(cur.pos), carried(base, cur.vel, back));
        let pos = (Mode::of(&dp, POS_SMALL, POS_MEDIUM), dp);
        let dr = d3(wide(cur.rot), wide(base.rot));
        let rot = if cur.rot_largest == base.rot_largest {
            (Mode::of(&dr, ROT_SMALL, ROT_MEDIUM), dr)
        } else {
            (Mode::Whole, dr)
        };
        let da =
            [i64::from(cur.aim[0]) - i64::from(base.aim[0]), i64::from(cur.aim[1]) - i64::from(base.aim[1])];
        let aim = (Mode::of(&da, AIM_SMALL, AIM_MEDIUM), da);
        Some(Self {
            back: back as u8,
            rider: cur.on.is_some(),
            aloft: cur.on.is_some_and(|on| on.aloft),
            vel,
            pos,
            rot,
            aim,
            cur: *cur,
            flags_same: cur.flags == base.flags,
            parts_same: cur.parts == base.parts,
        })
    }

    /// Bits of the record, the slot and the delta bit included.
    pub fn bits(&self) -> usize {
        let on = self.cur.on;
        let group = |mode: Mode, n: usize, small: u32, medium: u32, whole: usize| {
            2 + match mode {
                Mode::Same => 0,
                Mode::Small => n * small as usize,
                Mode::Medium => n * medium as usize,
                Mode::Whole => whole,
            }
        };
        SLOT_BITS as usize
            + 1
            + BACK_BITS as usize
            + usize::from(self.rider)
            + group(self.vel.0, 3, VEL_SMALL, VEL_MEDIUM, 3 * vel_bits(on) as usize)
            + group(self.pos.0, 3, POS_SMALL, POS_MEDIUM, 3 * pos_bits(on) as usize)
            + group(self.rot.0, 3, ROT_SMALL, ROT_MEDIUM, 2 + 3 * ENTITY_ROT_BITS as usize)
            + group(self.aim.0, 2, AIM_SMALL, AIM_MEDIUM, 2 * ENTITY_AIM_BITS as usize)
            + 1
            + if self.flags_same { 0 } else { ent_flags::BITS as usize }
            + 1
            + if self.parts_same { 0 } else { 3 * Part::COUNT }
    }

    /// Writes the record after its slot and delta bit (`1`).
    pub(crate) fn write(&self, w: &mut BitWriter<'_>) {
        let c = &self.cur;
        w.write_bits(u32::from(self.back), BACK_BITS);
        if self.rider {
            w.write_bool(self.aloft);
        }
        w.write_bits(self.vel.0.code(), 2);
        match self.vel.0 {
            Mode::Same => {}
            Mode::Small => self.vel.1.iter().for_each(|&d| w.write_i32(d as i32, VEL_SMALL)),
            Mode::Medium => self.vel.1.iter().for_each(|&d| w.write_i32(d as i32, VEL_MEDIUM)),
            Mode::Whole => write_vel(w, c.on, c.vel),
        }
        w.write_bits(self.pos.0.code(), 2);
        match self.pos.0 {
            Mode::Same => {}
            Mode::Small => self.pos.1.iter().for_each(|&d| w.write_i32(d as i32, POS_SMALL)),
            Mode::Medium => self.pos.1.iter().for_each(|&d| w.write_i32(d as i32, POS_MEDIUM)),
            Mode::Whole => {
                let bits = pos_bits(c.on);
                c.pos.iter().for_each(|&p| w.write_bits(p.min(max_code(bits)), bits));
            }
        }
        w.write_bits(self.rot.0.code(), 2);
        match self.rot.0 {
            Mode::Same => {}
            Mode::Small => self.rot.1.iter().for_each(|&d| w.write_i32(d as i32, ROT_SMALL)),
            Mode::Medium => self.rot.1.iter().for_each(|&d| w.write_i32(d as i32, ROT_MEDIUM)),
            Mode::Whole => write_rot(w, c.rot_largest, c.rot),
        }
        w.write_bits(self.aim.0.code(), 2);
        match self.aim.0 {
            Mode::Same => {}
            Mode::Small => self.aim.1.iter().for_each(|&d| w.write_i32(d as i32, AIM_SMALL)),
            Mode::Medium => self.aim.1.iter().for_each(|&d| w.write_i32(d as i32, AIM_MEDIUM)),
            Mode::Whole => c.aim.iter().for_each(|&a| w.write_bits(a, ENTITY_AIM_BITS)),
        }
        w.write_bool(self.flags_same);
        if !self.flags_same {
            w.write_bits(u32::from(c.flags), ent_flags::BITS);
        }
        w.write_bool(self.parts_same);
        if !self.parts_same {
            for p in c.parts {
                w.write_bits(u32::from(p.min(7)), 3);
            }
        }
    }

    /// Reads a delta record's age, after its slot and delta bit.
    pub(crate) fn read_back(r: &mut BitReader<'_>) -> u32 {
        r.read_bits(BACK_BITS)
    }

    /// Reads the rest of a delta record against `base`, `back` ticks older.
    pub(crate) fn read(r: &mut BitReader<'_>, base: &QEntity, back: u32) -> Result<QEntity, DecodeError> {
        let mut q = *base;
        if let Some(on) = &mut q.on {
            on.aloft = r.read_bool();
        }
        let signed = |r: &mut BitReader<'_>, bits: u32| i64::from(r.read_i32(bits));
        match Mode::from_code(r.read_bits(2)) {
            Mode::Same => {}
            Mode::Small => q.vel.iter_mut().for_each(|v| *v = (i64::from(*v) + signed(r, VEL_SMALL)) as i32),
            Mode::Medium => {
                q.vel.iter_mut().for_each(|v| *v = (i64::from(*v) + signed(r, VEL_MEDIUM)) as i32)
            }
            Mode::Whole => q.vel = read_vel(r, q.on),
        }
        let at = carried(base, q.vel, back);
        let bits = pos_bits(q.on);
        let put = |p: i64| p.clamp(0, i64::from(max_code(bits))) as u32;
        match Mode::from_code(r.read_bits(2)) {
            Mode::Same => q.pos = at.map(put),
            Mode::Small => {
                for (p, a) in q.pos.iter_mut().zip(at) {
                    *p = put(a + signed(r, POS_SMALL));
                }
            }
            Mode::Medium => {
                for (p, a) in q.pos.iter_mut().zip(at) {
                    *p = put(a + signed(r, POS_MEDIUM));
                }
            }
            Mode::Whole => q.pos = [r.read_bits(bits), r.read_bits(bits), r.read_bits(bits)],
        }
        let rot = |r: &mut BitReader<'_>, q: &mut QEntity, bits: u32| {
            for c in &mut q.rot {
                *c = (i64::from(*c) + signed(r, bits)).clamp(0, i64::from(max_code(ENTITY_ROT_BITS))) as u32;
            }
        };
        match Mode::from_code(r.read_bits(2)) {
            Mode::Same => {}
            Mode::Small => rot(r, &mut q, ROT_SMALL),
            Mode::Medium => rot(r, &mut q, ROT_MEDIUM),
            Mode::Whole => (q.rot_largest, q.rot) = read_rot(r),
        }
        let aim = |r: &mut BitReader<'_>, q: &mut QEntity, bits: u32| {
            for c in &mut q.aim {
                *c = (i64::from(*c) + signed(r, bits)).clamp(0, i64::from(max_code(ENTITY_AIM_BITS))) as u32;
            }
        };
        match Mode::from_code(r.read_bits(2)) {
            Mode::Same => {}
            Mode::Small => aim(r, &mut q, AIM_SMALL),
            Mode::Medium => aim(r, &mut q, AIM_MEDIUM),
            Mode::Whole => q.aim = [r.read_bits(ENTITY_AIM_BITS), r.read_bits(ENTITY_AIM_BITS)],
        }
        if !r.read_bool() {
            q.flags = r.read_bits(ent_flags::BITS) as u16;
        }
        if !r.read_bool() {
            for p in &mut q.parts {
                *p = r.read_bits(3) as u8;
            }
        }
        Ok(q)
    }
}
