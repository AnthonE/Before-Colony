//! People on foot in the colony's inside sector, on the wire (`docs/OPEN_CITY.md`, "The street in
//! the tick"; `docs/PROTOCOL.md`): the pilot's own walker, exactly ([`FootOwn`]), and the people
//! near them ([`PersonRecord`], as codes, [`QPerson`]: whole, or as a delta against a record the
//! client acked, [`PersonDelta`]).
//!
//! The own walker goes as raw `f32` bits, as a suit's own state goes at full precision: the owner's
//! predictor re-seeds from it to the bit and the simulation is never snapped to a grid (a body resting
//! a hair above a floor, rounded down into it, would step up out of it every tick). Other people go
//! on the plaza's grid (`presence::PersonPose`, about 8 mm), which is plenty to draw them by.
//!
//! Both ends hold a person's codes, never floats, so a delta taken on the server is undone on the
//! client to the bit. Nothing here allocates.

use glam::Vec3;

use crate::entity::BACK_BITS;
use crate::presence::{
    H_BITS, JET_BITS, PITCH_BITS, POSE_BITS, PersonPose, PoseCodes, RIDE_BITS, ROLL_BITS, S_BITS, SPEED_BITS,
    SPEED_MAX, STRIP_BITS, STRIPS, X_BITS, YAW_BITS,
};
use crate::{BitReader, BitWriter, DecodeError, SLOT_BITS};

/// What a walker is doing, in [`MODE_BITS`]: on their feet, riding a tram, sitting, riding a
/// lift; the codes past those are kept for kinds still to come (driving).
pub const MODE_BITS: u32 = 3;
const MODE_FOOT: u32 = 0;
const MODE_TRAIN: u32 = 1;
const MODE_SEATED: u32 = 2;
const MODE_LIFT: u32 = 3;
/// A rider's train on their strip's line, and its car.
const TRAIN_BITS: u32 = 4;
const CAR_BITS: u32 = 2;
/// A seat (`colony::city::SEATS`), a lift's storey, and the ticks left of its ride.
const SEAT_BITS: u32 = 6;
const STOREY_BITS: u32 = 5;
const LIFT_TICKS_BITS: u32 = 8;

/// What a walker is doing, and what of it a step reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FootMode {
    /// On their feet.
    Foot,
    /// Riding train `k` of their strip's line, in car `car`, at `local` in the car's frame.
    Train { k: u8, car: u8, local: Vec3 },
    /// Sitting on seat `seat`.
    Seated { seat: u8 },
    /// In a lift, going to storey `storey`, there in `left` ticks.
    Lift { storey: u8, left: u8 },
}

/// The pilot's own walker, as the tick has it, to the bit: its strip, where its feet are and how
/// fast it goes (in the walker's frame on the strip), where it looks, whether it stands on
/// something, and what it's doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootOwn {
    pub strip: u8,
    pub feet: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub mode: FootMode,
}

impl Default for FootOwn {
    fn default() -> Self {
        Self {
            strip: 0,
            feet: Vec3::ZERO,
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            grounded: false,
            mode: FootMode::Foot,
        }
    }
}

/// [`FootOwn`]'s bits on its feet: the strip, the feet and their speed, the look, grounded, the
/// mode.
pub const FOOT_OWN_BITS: usize = STRIP_BITS as usize + 6 * 32 + 2 * 32 + 1 + MODE_BITS as usize;

fn write_vec(w: &mut BitWriter<'_>, v: Vec3) {
    for c in v.to_array() {
        w.write_u32(c.to_bits());
    }
}

fn read_vec(r: &mut BitReader<'_>) -> Vec3 {
    Vec3::new(f32::from_bits(r.read_u32()), f32::from_bits(r.read_u32()), f32::from_bits(r.read_u32()))
}

impl FootOwn {
    /// Bits of the record (its presence bit apart).
    pub fn bits(&self) -> usize {
        FOOT_OWN_BITS
            + match self.mode {
                FootMode::Foot => 0,
                FootMode::Train { .. } => (TRAIN_BITS + CAR_BITS + 3 * 32) as usize,
                FootMode::Seated { .. } => SEAT_BITS as usize,
                FootMode::Lift { .. } => (STOREY_BITS + LIFT_TICKS_BITS) as usize,
            }
    }

    pub(crate) fn write(&self, w: &mut BitWriter<'_>) {
        w.write_bits(u32::from(self.strip.min(STRIPS - 1)), STRIP_BITS);
        write_vec(w, self.feet);
        write_vec(w, self.vel);
        w.write_u32(self.yaw.to_bits());
        w.write_u32(self.pitch.to_bits());
        w.write_bool(self.grounded);
        match self.mode {
            FootMode::Foot => w.write_bits(MODE_FOOT, MODE_BITS),
            FootMode::Train { k, car, local } => {
                w.write_bits(MODE_TRAIN, MODE_BITS);
                w.write_bits(u32::from(k), TRAIN_BITS);
                w.write_bits(u32::from(car), CAR_BITS);
                write_vec(w, local);
            }
            FootMode::Seated { seat } => {
                w.write_bits(MODE_SEATED, MODE_BITS);
                w.write_bits(u32::from(seat), SEAT_BITS);
            }
            FootMode::Lift { storey, left } => {
                w.write_bits(MODE_LIFT, MODE_BITS);
                w.write_bits(u32::from(storey), STOREY_BITS);
                w.write_bits(u32::from(left), LIFT_TICKS_BITS);
            }
        }
    }

    pub(crate) fn read(r: &mut BitReader<'_>) -> Result<Self, DecodeError> {
        let strip = r.read_bits(STRIP_BITS) as u8;
        let feet = read_vec(r);
        let vel = read_vec(r);
        let yaw = f32::from_bits(r.read_u32());
        let pitch = f32::from_bits(r.read_u32());
        let grounded = r.read_bool();
        let mode = match r.read_bits(MODE_BITS) {
            MODE_FOOT => FootMode::Foot,
            MODE_TRAIN => FootMode::Train {
                k: r.read_bits(TRAIN_BITS) as u8,
                car: r.read_bits(CAR_BITS) as u8,
                local: read_vec(r),
            },
            MODE_SEATED => FootMode::Seated { seat: r.read_bits(SEAT_BITS) as u8 },
            MODE_LIFT => FootMode::Lift {
                storey: r.read_bits(STOREY_BITS) as u8,
                left: r.read_bits(LIFT_TICKS_BITS) as u8,
            },
            _ => return Err(DecodeError::Invalid),
        };
        if strip >= STRIPS {
            return Err(DecodeError::Invalid);
        }
        Ok(Self { strip, feet, vel, yaw, pitch, grounded, mode })
    }
}

/// Someone on foot near the pilot: who (the person's id, their sector-0 slot), the low two bits of
/// their life's generation, and where they are as the plaza would have them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PersonRecord {
    pub id: u16,
    pub generation: u8,
    pub pose: PersonPose,
}

/// A person's record as codes: what [`PersonRecord`] says, quantized as the wire carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QPerson {
    pub id: u16,
    pub generation: u8,
    pub strip: u8,
    pub codes: PoseCodes,
}

/// Bits of a whole person's record: the id, the delta bit, the generation, the strip and the pose.
pub const PERSON_RECORD_BITS: usize = (SLOT_BITS + 1 + 2 + STRIP_BITS + POSE_BITS) as usize;

impl QPerson {
    /// The codes a record of `p` travels as.
    pub fn quantize(p: &PersonRecord) -> Self {
        Self {
            id: p.id & ((1 << SLOT_BITS) - 1),
            generation: p.generation & 3,
            strip: p.pose.strip.min(STRIPS - 1),
            codes: PoseCodes::of(&p.pose),
        }
    }

    /// What the codes say.
    pub fn record(&self) -> PersonRecord {
        PersonRecord { id: self.id, generation: self.generation, pose: self.codes.pose(self.strip) }
    }

    /// Whether a delta can be taken against `base`: the same person, life and strip.
    pub fn same_person(&self, base: &QPerson) -> bool {
        self.id == base.id && self.generation == base.generation && self.strip == base.strip
    }

    /// Bits of the whole record, the id and the delta bit included.
    pub fn absolute_bits(&self) -> usize {
        PERSON_RECORD_BITS
    }

    /// Writes the record after its id and delta bit (`0`).
    pub(crate) fn write_absolute(&self, w: &mut BitWriter<'_>) {
        w.write_bits(u32::from(self.generation & 3), 2);
        w.write_bits(u32::from(self.strip), STRIP_BITS);
        self.codes.write(w);
    }

    /// Reads a whole record of person `id`, after its delta bit.
    pub(crate) fn read_absolute(r: &mut BitReader<'_>, id: u16) -> Result<Self, DecodeError> {
        let generation = r.read_bits(2) as u8;
        let strip = r.read_bits(STRIP_BITS) as u8;
        let codes = PoseCodes::read(r);
        if strip >= STRIPS {
            return Err(DecodeError::Invalid);
        }
        Ok(Self { id, generation, strip, codes })
    }
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
    fn from_code(c: u32) -> Self {
        match c & 3 {
            0 => Mode::Same,
            1 => Mode::Small,
            2 => Mode::Medium,
            _ => Mode::Whole,
        }
    }

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

    /// Bits of a group of `n` codes in this mode (its 2-bit mode included), `whole` bits whole.
    fn bits(self, n: usize, small: u32, medium: u32, whole: usize) -> usize {
        2 + match self {
            Mode::Same => 0,
            Mode::Small => n * small as usize,
            Mode::Medium => n * medium as usize,
            Mode::Whole => whole,
        }
    }
}

/// Widths of the small and medium differences: position (from where the heading and speed carried
/// it), heading and pitch.
const POS_SMALL: u32 = 6;
const POS_MEDIUM: u32 = 12;
const YAW_SMALL: u32 = 5;
const YAW_MEDIUM: u32 = 8;
const PITCH_SMALL: u32 = 4;
const PITCH_MEDIUM: u32 = 6;
/// The tick rate a delta's position is carried on at.
const HZ: f32 = 30.0;
/// A metre in position codes (`x`, `s` and `h` are all on a 1/128 m grid).
const PER_METRE: f32 = 128.0;
const _: () = assert!(X_BITS == 22 && S_BITS == 19 && H_BITS == 15, "the grid the carry assumes");

fn max_code(bits: u32) -> i64 {
    (1i64 << bits) - 1
}

/// Rounds half away from zero.
fn round(v: f32) -> i64 {
    if v >= 0.0 { (v + 0.5) as i64 } else { -((-v + 0.5) as i64) }
}

/// A heading code's direction in city terms, `(dx, ds)` (the walker's yaw: 0 faces −s, a quarter
/// turn +x), from polynomials (adds and multiplies only, so both ends agree to the bit without a
/// shared maths library).
fn heading(yaw: u32) -> (f32, f32) {
    let quarter = 1u32 << (YAW_BITS - 2);
    let t = (yaw % quarter) as f32 * (core::f32::consts::FRAC_PI_2 / quarter as f32);
    let t2 = t * t;
    let sin = t * (1.0 - t2 / 6.0 * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0 * (1.0 - t2 / 72.0))));
    let cos = 1.0 - t2 / 2.0 * (1.0 - t2 / 12.0 * (1.0 - t2 / 30.0 * (1.0 - t2 / 56.0)));
    let (s, c) = match (yaw / quarter) & 3 {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    };
    (s, -c)
}

/// Where `base`'s position codes come to after `dt` ticks at speed code `speed` along heading code
/// `yaw` (the record's own, read before its position), clamped to the grid; its height stays.
fn carried(base: &QPerson, yaw: u32, speed: u32, dt: u32) -> [i64; 3] {
    let (dx, ds) = heading(yaw);
    let metres = speed as f32 * (SPEED_MAX / (1u32 << SPEED_BITS) as f32) * dt as f32 / HZ;
    let d = metres * PER_METRE;
    let c = &base.codes;
    [
        (i64::from(c.x) + round(d * dx)).clamp(0, max_code(X_BITS)),
        (i64::from(c.s) + round(d * ds)).clamp(0, max_code(S_BITS)),
        i64::from(c.h),
    ]
}

/// The signed difference between two heading codes, the short way round.
fn turn(cur: u32, base: u32) -> i64 {
    let full = 1i64 << YAW_BITS;
    (i64::from(cur) - i64::from(base) + full / 2).rem_euclid(full) - full / 2
}

/// A person's record as a delta against a baseline, worked out once: what [`bits`](Self::bits)
/// counts is what [`write`](Self::write) writes.
#[derive(Clone, Copy, Debug)]
pub struct PersonDelta {
    back: u8,
    yaw: (Mode, i64),
    pitch: (Mode, i64),
    roll_same: bool,
    speed_same: bool,
    flags_same: bool,
    pos: (Mode, [i64; 3]),
    cur: QPerson,
}

impl PersonDelta {
    /// `cur` against `base`, `back` ticks older (1 to 255). `None` if it can't be: another person,
    /// life or strip, or a baseline too old.
    pub fn plan(cur: &QPerson, base: &QPerson, back: u32) -> Option<Self> {
        if !(1..=255).contains(&back) || !cur.same_person(base) {
            return None;
        }
        let (c, b) = (&cur.codes, &base.codes);
        let dy = turn(c.yaw, b.yaw);
        let dp = i64::from(c.pitch) - i64::from(b.pitch);
        let at = carried(base, c.yaw, c.speed, back);
        let dpos = [i64::from(c.x) - at[0], i64::from(c.s) - at[1], i64::from(c.h) - at[2]];
        let flags = |p: &PoseCodes| (p.grounded, p.running, p.ride, p.jets);
        Some(Self {
            back: back as u8,
            yaw: (Mode::of(&[dy], YAW_SMALL, YAW_MEDIUM), dy),
            pitch: (Mode::of(&[dp], PITCH_SMALL, PITCH_MEDIUM), dp),
            roll_same: c.roll == b.roll,
            speed_same: c.speed == b.speed,
            flags_same: flags(c) == flags(b),
            pos: (Mode::of(&dpos, POS_SMALL, POS_MEDIUM), dpos),
            cur: *cur,
        })
    }

    /// Bits of the record, the id and the delta bit included.
    pub fn bits(&self) -> usize {
        SLOT_BITS as usize
            + 1
            + BACK_BITS as usize
            + self.yaw.0.bits(1, YAW_SMALL, YAW_MEDIUM, YAW_BITS as usize)
            + self.pitch.0.bits(1, PITCH_SMALL, PITCH_MEDIUM, PITCH_BITS as usize)
            + 1
            + if self.roll_same { 0 } else { ROLL_BITS as usize }
            + 1
            + if self.speed_same { 0 } else { SPEED_BITS as usize }
            + 1
            + if self.flags_same { 0 } else { (2 + RIDE_BITS + JET_BITS) as usize }
            + self.pos.0.bits(3, POS_SMALL, POS_MEDIUM, (X_BITS + S_BITS + H_BITS) as usize)
    }

    /// Writes the record after its id and delta bit (`1`).
    pub(crate) fn write(&self, w: &mut BitWriter<'_>) {
        let c = &self.cur.codes;
        w.write_bits(u32::from(self.back), BACK_BITS);
        let one = |w: &mut BitWriter<'_>, (mode, d): (Mode, i64), small, medium, whole: u32, bits| {
            w.write_bits(mode as u32, 2);
            match mode {
                Mode::Same => {}
                Mode::Small => w.write_i32(d as i32, small),
                Mode::Medium => w.write_i32(d as i32, medium),
                Mode::Whole => w.write_bits(whole, bits),
            }
        };
        one(w, self.yaw, YAW_SMALL, YAW_MEDIUM, c.yaw, YAW_BITS);
        one(w, self.pitch, PITCH_SMALL, PITCH_MEDIUM, c.pitch, PITCH_BITS);
        w.write_bool(self.roll_same);
        if !self.roll_same {
            w.write_bits(c.roll, ROLL_BITS);
        }
        w.write_bool(self.speed_same);
        if !self.speed_same {
            w.write_bits(c.speed, SPEED_BITS);
        }
        w.write_bool(self.flags_same);
        if !self.flags_same {
            c.write_flags(w);
        }
        let (mode, d) = self.pos;
        w.write_bits(mode as u32, 2);
        match mode {
            Mode::Same => {}
            Mode::Small => d.iter().for_each(|&d| w.write_i32(d as i32, POS_SMALL)),
            Mode::Medium => d.iter().for_each(|&d| w.write_i32(d as i32, POS_MEDIUM)),
            Mode::Whole => {
                w.write_bits(c.x, X_BITS);
                w.write_bits(c.s, S_BITS);
                w.write_bits(c.h, H_BITS);
            }
        }
    }

    /// Reads a delta record's age, after its id and delta bit.
    pub(crate) fn read_back(r: &mut BitReader<'_>) -> u32 {
        r.read_bits(BACK_BITS)
    }

    /// Reads the rest of a delta record against `base`, `back` ticks older.
    pub(crate) fn read(r: &mut BitReader<'_>, base: &QPerson, back: u32) -> Result<QPerson, DecodeError> {
        let mut q = *base;
        let c = &mut q.codes;
        let b = &base.codes;
        let one = |r: &mut BitReader<'_>, was: u32, small, medium, bits: u32, wrap: bool| {
            let top = max_code(bits);
            let d = match Mode::from_code(r.read_bits(2)) {
                Mode::Same => 0,
                Mode::Small => i64::from(r.read_i32(small)),
                Mode::Medium => i64::from(r.read_i32(medium)),
                Mode::Whole => return r.read_bits(bits),
            };
            let v = i64::from(was) + d;
            (if wrap { v.rem_euclid(top + 1) } else { v.clamp(0, top) }) as u32
        };
        c.yaw = one(r, b.yaw, YAW_SMALL, YAW_MEDIUM, YAW_BITS, true);
        c.pitch = one(r, b.pitch, PITCH_SMALL, PITCH_MEDIUM, PITCH_BITS, false);
        if !r.read_bool() {
            c.roll = r.read_bits(ROLL_BITS);
        }
        if !r.read_bool() {
            c.speed = r.read_bits(SPEED_BITS);
        }
        if !r.read_bool() {
            c.read_flags(r);
        }
        let at = carried(base, c.yaw, c.speed, back);
        let put = |p: i64, bits: u32| p.clamp(0, max_code(bits)) as u32;
        let bits = [X_BITS, S_BITS, H_BITS];
        let pos = match Mode::from_code(r.read_bits(2)) {
            Mode::Same => [0i64; 3],
            Mode::Small => [0; 3].map(|_| i64::from(r.read_i32(POS_SMALL))),
            Mode::Medium => [0; 3].map(|_| i64::from(r.read_i32(POS_MEDIUM))),
            Mode::Whole => {
                let whole = bits.map(|n| i64::from(r.read_bits(n)));
                [whole[0] - at[0], whole[1] - at[1], whole[2] - at[2]]
            }
        };
        c.x = put(at[0] + pos[0], X_BITS);
        c.s = put(at[1] + pos[1], S_BITS);
        c.h = put(at[2] + pos[2], H_BITS);
        Ok(q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::BitReader;

    fn own(mode: FootMode) -> FootOwn {
        FootOwn {
            strip: 2,
            feet: Vec3::new(-12_345.678, 0.150_001, -1_234.567),
            vel: Vec3::new(4.2, -0.000_001, -3.3),
            yaw: 6.25,
            pitch: -1.45,
            grounded: true,
            mode,
        }
    }

    fn modes() -> [FootMode; 4] {
        [
            FootMode::Foot,
            FootMode::Train { k: 11, car: 3, local: Vec3::new(1.5e-7, 0.0, -f32::MIN_POSITIVE) },
            FootMode::Seated { seat: 63 },
            FootMode::Lift { storey: 31, left: 255 },
        ]
    }

    #[test]
    fn the_own_walker_round_trips_to_the_bit_in_every_mode() {
        for mode in modes() {
            let f = own(mode);
            let mut buf = [0u8; 64];
            let mut w = BitWriter::new(&mut buf);
            f.write(&mut w);
            assert_eq!(w.bits_written(), f.bits());
            let n = w.bytes_written();
            let back = FootOwn::read(&mut BitReader::new(&buf[..n])).unwrap();
            let bits = |f: &FootOwn| {
                let local = match f.mode {
                    FootMode::Train { local, .. } => local.to_array().map(f32::to_bits),
                    _ => [0; 3],
                };
                (
                    f.feet.to_array().map(f32::to_bits),
                    f.vel.to_array().map(f32::to_bits),
                    f.yaw.to_bits(),
                    local,
                )
            };
            assert_eq!(back, f);
            assert_eq!(bits(&back), bits(&f));
        }
        assert_eq!(own(FootMode::Foot).bits(), 262);
    }

    #[test]
    fn a_mode_still_to_come_is_refused() {
        let mut buf = [0u8; 64];
        let mut w = BitWriter::new(&mut buf);
        own(FootMode::Foot).write(&mut w);
        let n = w.bytes_written();
        // The mode is the last three bits written: make it 7.
        let at = FOOT_OWN_BITS - MODE_BITS as usize;
        for i in at..FOOT_OWN_BITS {
            buf[i / 8] |= 1 << (i % 8);
        }
        assert_eq!(FootOwn::read(&mut BitReader::new(&buf[..n])), Err(DecodeError::Invalid));
    }

    #[test]
    fn headings_point_where_the_walker_faces() {
        for code in 0..1u32 << YAW_BITS {
            let yaw = code as f32 * core::f32::consts::TAU / (1u32 << YAW_BITS) as f32;
            let (dx, ds) = heading(code);
            assert!((dx - series_sin(yaw)).abs() < 1e-4 && (ds + series_cos(yaw)).abs() < 1e-4, "{code}");
        }
    }

    /// Reference sine and cosine by a long series (tests only).
    fn series_sin(x: f32) -> f32 {
        let x = f64::from(x).rem_euclid(core::f64::consts::TAU);
        let (mut term, mut sum) = (x, x);
        for k in 1..30 {
            term *= -x * x / f64::from((2 * k) * (2 * k + 1));
            sum += term;
        }
        sum as f32
    }

    fn series_cos(x: f32) -> f32 {
        let x = f64::from(x).rem_euclid(core::f64::consts::TAU);
        let (mut term, mut sum) = (1.0f64, 1.0f64);
        for k in 1..30 {
            term *= -x * x / f64::from((2 * k - 1) * (2 * k));
            sum += term;
        }
        sum as f32
    }

    use crate::snapshot::{SnapshotHeader, SnapshotReader, SnapshotWriter, header_flags};
    use crate::{MAX_DATAGRAM, ObjectState};

    const TAU: f32 = core::f32::consts::TAU;

    /// Someone walking: person `i`'s record at tick `t` (a second is 30), round a wide circle at a
    /// walk, now and then running or jumping, and for a moment each minute riding.
    fn walking(i: u16, t: u32) -> PersonRecord {
        const TURN: f32 = 0.05;
        const WALK: f32 = 4.2;
        let f = t as f32 / 30.0;
        let yaw0 = 0.3 * f32::from(i);
        let yaw = yaw0 + TURN * f;
        let r = WALK / TURN;
        let up = t % 50 < 10;
        PersonRecord {
            id: 100 + i,
            generation: (i % 4) as u8,
            pose: PersonPose {
                strip: (i % 3) as u8,
                x: -14_000.0 + 37.0 * f32::from(i) + r * (series_cos(yaw0) - series_cos(yaw)),
                s: 1_700.0 + 11.0 * f32::from(i) - r * (series_sin(yaw) - series_sin(yaw0)),
                h: if up { 0.4 } else { 0.15 },
                yaw: yaw % TAU,
                pitch: -0.2,
                roll: 0.0,
                speed: WALK,
                grounded: !up,
                running: (t / 90 + u32::from(i)).is_multiple_of(4),
                ride: if t % 1_800 > 1_780 { 3 } else { 0 },
                jets: 0,
            },
        }
    }

    /// A walker's snapshot of tick `tick`: an object, their own walker and `people` (each a delta
    /// against `bases`' record of them, `back` ticks older, where one can be taken), read back with
    /// `held` as what the reader holds. What it read, and the bits each record took.
    #[allow(clippy::type_complexity)]
    fn through_the_wire(
        tick: u32,
        own: Option<&FootOwn>,
        people: &[QPerson],
        bases: &[QPerson],
        back: u32,
        held: &[QPerson],
    ) -> (Option<FootOwn>, [Option<Result<QPerson, DecodeError>>; 8], [usize; 8]) {
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&SnapshotHeader { tick, flags: header_flags::ON_FOOT, ..SnapshotHeader::default() });
        w.own(None);
        w.orrery(None);
        assert!(w.object(&ObjectState::Gone { id: 12 }));
        if let Some(f) = own {
            assert!(w.foot_own(f));
            assert!(!w.foot_own(f), "once");
        }
        let mut bits = [0; 8];
        for (k, q) in people.iter().enumerate() {
            let delta = bases.get(k).and_then(|b| PersonDelta::plan(q, b, back));
            let before = w.bits_remaining();
            bits[k] = w.person(q, delta.as_ref(), 0).expect("it fits");
            // The first closes the lists before it: the salvage's, and its own walker's if it isn't.
            let closed = if k == 0 && own.is_none() { 2 } else { 0 };
            assert_eq!(before - w.bits_remaining(), bits[k] + 1 + closed, "the record took the bits it said");
        }
        let n = w.finish().unwrap();
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        assert!(matches!(r.next_object(), Ok(Some(_))));
        let mine = r.foot_own().unwrap();
        let mut out = [None; 8];
        for o in &mut out {
            match r.next_person(|id, at| held.iter().find(|h| h.id == id && at == tick - back).copied()) {
                Ok(None) => break,
                got => *o = Some(got.map(|q| q.unwrap())),
            }
            if matches!(o, Some(Err(_))) {
                break;
            }
        }
        (mine, out, bits)
    }

    #[test]
    fn the_own_walker_and_the_people_near_go_round_in_a_walkers_snapshot() {
        let me = own(FootMode::Train { k: 4, car: 1, local: Vec3::new(3.25, 0.0, -0.75) });
        let people: [QPerson; 3] = core::array::from_fn(|i| QPerson::quantize(&walking(i as u16, 900)));
        let (mine, got, _) = through_the_wire(900, Some(&me), &people, &[], 1, &[]);
        assert_eq!(mine, Some(me));
        for (k, q) in people.iter().enumerate() {
            assert_eq!(got[k], Some(Ok(*q)));
        }
        assert_eq!(got[3], None);
        // Without its own walker (it didn't fit), the people still go.
        let (mine, got, _) = through_the_wire(900, None, &people, &[], 1, &[]);
        assert_eq!((mine, got[2]), (None, Some(Ok(people[2]))));
    }

    #[test]
    fn a_person_goes_round_within_half_a_step() {
        let [dx, ds, dh, dyaw] = PersonPose::STEP;
        for i in 0..64u16 {
            for t in [0u32, 7, 333, 4_000, 99_999] {
                let p = walking(i, t);
                let back = QPerson::quantize(&p).record();
                assert_eq!((back.id, back.generation, back.pose.strip), (p.id, p.generation, p.pose.strip));
                let (a, b) = (&back.pose, &p.pose);
                assert!((a.x - b.x).abs() <= dx * 0.5 + 1e-3, "x {} {}", a.x, b.x);
                assert!((a.s - b.s).abs() <= ds * 0.5 + 1e-4 && (a.h - b.h).abs() <= dh * 0.5 + 1e-4);
                let dy = (a.yaw - b.yaw).rem_euclid(TAU);
                assert!(dy.min(TAU - dy) <= dyaw * 0.5 + 1e-4, "yaw {} {}", a.yaw, b.yaw);
                assert_eq!((a.grounded, a.running, a.ride), (b.grounded, b.running, b.ride));
                // And its codes again are the same codes.
                assert_eq!(QPerson::quantize(&back), QPerson::quantize(&p));
            }
        }
    }

    #[test]
    fn a_delta_rebuilds_a_person_to_the_bit_and_a_walk_is_small() {
        let mut most = 0;
        for back in [1u32, 2, 3, 5, 8, 15, 30, 90, 255] {
            for t in [120u32, 1_000, 3_333] {
                let bases: [QPerson; 8] =
                    core::array::from_fn(|i| QPerson::quantize(&walking(i as u16, t - back)));
                let cur: [QPerson; 8] = core::array::from_fn(|i| QPerson::quantize(&walking(i as u16, t)));
                let (_, got, bits) = through_the_wire(t, None, &cur, &bases, back, &bases);
                for k in 0..8 {
                    assert_eq!(got[k], Some(Ok(cur[k])), "back {back} t {t} person {k}");
                    let planned = PersonDelta::plan(&cur[k], &bases[k], back).unwrap();
                    assert_eq!(bits[k], planned.bits());
                    let (c, b) = (&cur[k].codes, &bases[k].codes);
                    if back <= 8 && (c.ride, c.grounded, c.h) == (b.ride, b.grounded, b.h) {
                        most = most.max(bits[k]);
                    }
                }
            }
        }
        assert!(most <= 60, "a walk's delta: {most} bits");
        // Standing still, it's its id, age and modes; at a steady walk its heading and speed carry it.
        let base = QPerson::quantize(&PersonRecord {
            id: 7,
            generation: 1,
            pose: PersonPose {
                strip: 1,
                x: 100.0,
                s: 900.0,
                h: 0.15,
                yaw: 1.0,
                speed: 4.2,
                grounded: true,
                ..PersonPose::default()
            },
        });
        let still = QPerson { codes: PoseCodes { speed: 0, ..base.codes }, ..base };
        assert_eq!(PersonDelta::plan(&still, &still, 3).unwrap().bits(), 28);
        let (dx, ds) = heading(base.codes.yaw);
        let was = base.record().pose;
        let on = PersonPose { x: was.x + was.speed * 0.1 * dx, s: was.s + was.speed * 0.1 * ds, ..was };
        let cur = QPerson::quantize(&PersonRecord { pose: on, ..base.record() });
        let d = PersonDelta::plan(&cur, &base, 3).unwrap();
        assert!(d.bits() <= 28 + 18, "{} bits", d.bits());
    }

    #[test]
    fn a_delta_without_its_baseline_or_across_people_is_refused() {
        let base = QPerson::quantize(&walking(3, 500));
        let cur = QPerson::quantize(&walking(3, 503));
        // Nothing held: the reader refuses the record.
        let (_, got, _) = through_the_wire(503, None, &[cur], &[base], 3, &[]);
        assert_eq!(got[0], Some(Err(DecodeError::NoBaseline)));
        // Held at another tick: refused too.
        let (_, got, _) = through_the_wire(503, None, &[cur], &[base], 3, &[QPerson { id: 999, ..base }]);
        assert_eq!(got[0], Some(Err(DecodeError::NoBaseline)));
        // No delta across people, lives or strips, or too far back.
        assert!(PersonDelta::plan(&cur, &QPerson { id: 4, ..base }, 3).is_none());
        assert!(PersonDelta::plan(&cur, &QPerson { generation: 2, ..base }, 3).is_none());
        assert!(PersonDelta::plan(&cur, &QPerson { strip: (base.strip + 1) % 3, ..base }, 3).is_none());
        assert!(PersonDelta::plan(&cur, &base, 0).is_none() && PersonDelta::plan(&cur, &base, 256).is_none());
    }

    #[test]
    fn a_snapshot_not_on_foot_has_no_walkers_sections() {
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&SnapshotHeader { tick: 5, ..SnapshotHeader::default() });
        w.own(None);
        w.orrery(None);
        let before = w.bits_remaining();
        assert!(!w.foot_own(&own(FootMode::Foot)));
        assert!(w.person(&QPerson::default(), None, 0).is_none());
        assert_eq!(w.bits_remaining(), before, "nothing written");
        let n = w.finish().unwrap();
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        assert_eq!(r.foot_own(), Ok(None));
        assert_eq!(r.next_person(|_, _| None), Ok(None));
    }

    #[test]
    fn a_full_walkers_snapshot_ends_whole() {
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&SnapshotHeader { tick: 77, flags: header_flags::ON_FOOT, ..SnapshotHeader::default() });
        w.own(None);
        w.orrery(None);
        assert!(w.foot_own(&own(FootMode::Lift { storey: 4, left: 30 })));
        let mut n = 0u16;
        while w.person(&QPerson::quantize(&walking(n, 77)), None, 0).is_some() {
            n += 1;
        }
        assert!(n >= 70, "{n} people");
        let len = w.finish().expect("never overflows");
        let mut r = SnapshotReader::new(&buf[..len]).unwrap();
        assert!(matches!(r.foot_own(), Ok(Some(_))));
        let mut read = 0;
        while let Ok(Some(_)) = r.next_person(|_, _| None) {
            read += 1;
        }
        assert_eq!(read, n);
    }
}
