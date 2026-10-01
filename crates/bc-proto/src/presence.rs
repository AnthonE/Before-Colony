//! The colony's people on foot ("the plaza"): where each pilot walking the city is, off the
//! sector's tick. Pilots in the city send their own pose ([`PosePacket`], about 15 a second); the
//! session task relays the people near each of them ([`PlazaWriter`] / [`PlazaReader`], 10 a
//! second, and twice a second in the bay as a heartbeat that carries the sector's tick).
//!
//! Poses are in a strip's city coordinates (`bc_sim::colony::frame::CityPos`: `x` along, `s` across
//! from the strip's edge, `h` up), where the city stands still: about 8 mm steps over the whole
//! colony. Nothing here allocates.

use crate::{BitReader, BitWriter, DecodeError, MAX_DATAGRAM, PACKET_KIND_BITS, PacketKind, SLOT_BITS};

/// The colony's land strips.
pub const STRIPS: u8 = 3;
const STRIP_BITS: u32 = 2;
/// `x` along: [−`X_HALF`, `X_HALF`) m.
const X_HALF: f32 = 16_384.0;
const X_BITS: u32 = 22;
/// `s` across: [0, `S_SPAN`) m.
const S_SPAN: f32 = 4_096.0;
const S_BITS: u32 = 19;
/// `h` up: [`H_MIN`, `H_MIN` + `H_SPAN`) m (a canal's bed to the tallest roof).
const H_MIN: f32 = -8.0;
const H_SPAN: f32 = 256.0;
const H_BITS: u32 = 15;
const YAW_BITS: u32 = 10;
const PITCH_BITS: u32 = 8;
/// Speed over the ground: [0, `SPEED_MAX`] m/s.
const SPEED_MAX: f32 = 12.6;
const SPEED_BITS: u32 = 6;
/// What a pilot rides ([`PersonPose::ride`]).
const RIDE_BITS: u32 = 4;
const POSE_BITS: u32 = X_BITS + S_BITS + H_BITS + YAW_BITS + PITCH_BITS + SPEED_BITS + 2 + RIDE_BITS;
/// Trains on a strip's line (`bc_sim::colony::transit::TRAINS`).
pub const MAX_TRAINS: u8 = 12;
/// [`PersonPose::ride`] for a pilot driving a car, and riding a scooter.
pub const RIDE_CAR: u8 = 13;
pub const RIDE_SCOOTER: u8 = 14;
/// A rider's `s` is from their train's track, offset by this so it stays positive, m.
pub const RIDER_S: f32 = 2_048.0;
/// How long before the plaza's tick the server heard a person's pose, in 10 ms steps.
const AGE_BITS: u32 = 6;
/// What one person takes in a plaza datagram, bits.
pub const PERSON_BITS: u32 = SLOT_BITS + AGE_BITS + POSE_BITS;
/// The most people a plaza datagram carries.
pub const MAX_PEOPLE: usize = 48;
const PLAZA_HEADER_BITS: u32 = PACKET_KIND_BITS + 32 + 1 + STRIP_BITS;

/// One pilot on foot in the city.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PersonPose {
    pub strip: u8,
    /// Along the colony, m.
    pub x: f32,
    /// Across the strip from its edge, m.
    pub s: f32,
    /// Feet above the ground, m.
    pub h: f32,
    /// Heading, rad: the walker's yaw (0 faces −s, across the strip; τ/4 faces +x, along it).
    pub yaw: f32,
    /// Looking up (+) or down, rad.
    pub pitch: f32,
    /// Over the ground, m/s.
    pub speed: f32,
    pub grounded: bool,
    pub running: bool,
    /// What they ride: 0 nothing (on foot); `k` + 1 train `k` of the strip's line, when a rider's
    /// `x`, `s` and `h` are from the train's middle, from its track's middle (plus [`RIDER_S`])
    /// and from its floor, so they're drawn inside it wherever each screen has it; [`RIDE_CAR`] or
    /// [`RIDE_SCOOTER`] driving one (where it is, its heading, its speed).
    pub ride: u8,
}

impl PersonPose {
    /// Driving a car or riding a scooter.
    pub fn driving(&self) -> bool {
        matches!(self.ride, RIDE_CAR | RIDE_SCOOTER)
    }

    /// Riding train `k`.
    pub fn riding(&self) -> Option<u8> {
        self.ride.checked_sub(1).filter(|k| *k < MAX_TRAINS)
    }
}

fn unit(v: f32, lo: f32, span: f32, bits: u32) -> u32 {
    let max = (1u32 << bits) - 1;
    let q = ((v - lo) / span * (1u32 << bits) as f32 + 0.5) as i64;
    q.clamp(0, i64::from(max)) as u32
}

fn ununit(q: u32, lo: f32, span: f32, bits: u32) -> f32 {
    lo + q as f32 * span / (1u32 << bits) as f32
}

const TAU: f32 = core::f32::consts::TAU;
const HALF_PI: f32 = core::f32::consts::FRAC_PI_2;

/// An angle in [0, τ).
fn wrap(a: f32) -> f32 {
    let r = a % TAU;
    if r < 0.0 { r + TAU } else { r }
}

impl PersonPose {
    /// How far a pose may move on the wire (half a step), per field: x, s, h (m) and yaw (rad).
    pub const STEP: [f32; 4] = [
        2.0 * X_HALF / (1u32 << X_BITS) as f32,
        S_SPAN / (1u32 << S_BITS) as f32,
        H_SPAN / (1u32 << H_BITS) as f32,
        TAU / (1u32 << YAW_BITS) as f32,
    ];

    fn write_body(&self, w: &mut BitWriter<'_>) {
        w.write_bits(unit(self.x, -X_HALF, 2.0 * X_HALF, X_BITS), X_BITS);
        w.write_bits(unit(self.s, 0.0, S_SPAN, S_BITS), S_BITS);
        w.write_bits(unit(self.h, H_MIN, H_SPAN, H_BITS), H_BITS);
        // Round the circle: a heading just short of τ goes as 0.
        let yaw = (wrap(self.yaw) / TAU * (1u32 << YAW_BITS) as f32 + 0.5) as u32;
        w.write_bits(yaw & ((1 << YAW_BITS) - 1), YAW_BITS);
        w.write_bits(unit(self.pitch, -HALF_PI, 2.0 * HALF_PI, PITCH_BITS), PITCH_BITS);
        w.write_bits(unit(self.speed, 0.0, SPEED_MAX, SPEED_BITS), SPEED_BITS);
        w.write_bool(self.grounded);
        w.write_bool(self.running);
        w.write_bits(u32::from(self.ride.min((1 << RIDE_BITS) - 1)), RIDE_BITS);
    }

    fn read_body(r: &mut BitReader<'_>, strip: u8) -> Self {
        Self {
            strip,
            x: ununit(r.read_bits(X_BITS), -X_HALF, 2.0 * X_HALF, X_BITS),
            s: ununit(r.read_bits(S_BITS), 0.0, S_SPAN, S_BITS),
            h: ununit(r.read_bits(H_BITS), H_MIN, H_SPAN, H_BITS),
            yaw: ununit(r.read_bits(YAW_BITS), 0.0, TAU, YAW_BITS),
            pitch: ununit(r.read_bits(PITCH_BITS), -HALF_PI, 2.0 * HALF_PI, PITCH_BITS),
            speed: ununit(r.read_bits(SPEED_BITS), 0.0, SPEED_MAX, SPEED_BITS),
            grounded: r.read_bool(),
            running: r.read_bool(),
            ride: r.read_bits(RIDE_BITS) as u8,
        }
    }
}

/// Client → server: the pilot's own pose in the city. `seq` counts up (wrapping), so the server
/// keeps only the newest of reordered packets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PosePacket {
    pub seq: u16,
    pub pose: PersonPose,
}

impl PosePacket {
    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        let mut w = BitWriter::new(buf);
        w.write_bits(PacketKind::Pose as u32, PACKET_KIND_BITS);
        w.write_u16(self.seq);
        w.write_bits(u32::from(self.pose.strip.min(STRIPS - 1)), STRIP_BITS);
        self.pose.write_body(&mut w);
        (!w.overflowed()).then(|| w.bytes_written())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(bytes);
        if r.read_bits(PACKET_KIND_BITS) != PacketKind::Pose as u32 {
            return Err(DecodeError::WrongKind);
        }
        let seq = r.read_u16();
        let strip = r.read_bits(STRIP_BITS) as u8;
        let pose = PersonPose::read_body(&mut r, strip);
        if r.overflowed() {
            return Err(DecodeError::Truncated);
        }
        if strip >= STRIPS {
            return Err(DecodeError::Invalid);
        }
        Ok(Self { seq, pose })
    }
}

/// Server → client: the sector's tick as this was sent, and the people on the viewer's strip near
/// them (none in the bay, or alone). People follow the header until the bits run out.
pub struct PlazaWriter<'a> {
    w: BitWriter<'a>,
    count: usize,
}

impl<'a> PlazaWriter<'a> {
    /// `tick`: the sector's last tick; `strip`: the viewer's strip, if they're in the city.
    pub fn new(buf: &'a mut [u8], tick: u32, strip: Option<u8>) -> Self {
        let mut w = BitWriter::with_limit(buf, MAX_DATAGRAM);
        w.write_bits(PacketKind::Plaza as u32, PACKET_KIND_BITS);
        w.write_u32(tick);
        w.write_bool(strip.is_some());
        w.write_bits(u32::from(strip.unwrap_or(0).min(STRIPS - 1)), STRIP_BITS);
        Self { w, count: 0 }
    }

    /// Adds a person (their client slot, how long ago their pose was heard, s): false once full.
    pub fn push(&mut self, id: u16, age: f32, pose: &PersonPose) -> bool {
        if self.count >= MAX_PEOPLE || self.w.bits_remaining() < PERSON_BITS as usize {
            return false;
        }
        self.w.write_bits(u32::from(id), SLOT_BITS);
        self.w.write_bits(unit(age, 0.0, 0.64, AGE_BITS), AGE_BITS);
        pose.write_body(&mut self.w);
        self.count += 1;
        true
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The datagram's length.
    pub fn finish(self) -> usize {
        self.w.bytes_written()
    }
}

/// Reads a plaza datagram: its header, then [`PlazaReader::next_person`] until `None`.
pub struct PlazaReader<'a> {
    r: BitReader<'a>,
    pub tick: u32,
    pub strip: Option<u8>,
}

impl<'a> PlazaReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(bytes);
        if r.read_bits(PACKET_KIND_BITS) != PacketKind::Plaza as u32 {
            return Err(DecodeError::WrongKind);
        }
        let tick = r.read_u32();
        let city = r.read_bool();
        let strip = r.read_bits(STRIP_BITS) as u8;
        if r.overflowed() {
            return Err(DecodeError::Truncated);
        }
        if strip >= STRIPS {
            return Err(DecodeError::Invalid);
        }
        Ok(Self { r, tick, strip: city.then_some(strip) })
    }

    /// The next person: their client slot, how long before the tick their pose was heard (s),
    /// and the pose.
    pub fn next_person(&mut self) -> Option<(u16, f32, PersonPose)> {
        let strip = self.strip?;
        if self.r.bits_remaining() < PERSON_BITS as usize {
            return None;
        }
        let id = self.r.read_bits(SLOT_BITS) as u16;
        let age = ununit(self.r.read_bits(AGE_BITS), 0.0, 0.64, AGE_BITS);
        let pose = PersonPose::read_body(&mut self.r, strip);
        (!self.r.overflowed()).then_some((id, age, pose))
    }
}

const _: () = assert!(PLAZA_HEADER_BITS as usize + MAX_PEOPLE * PERSON_BITS as usize <= MAX_DATAGRAM * 8);

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(i: u32) -> PersonPose {
        let f = i as f32;
        PersonPose {
            strip: (i % 3) as u8,
            x: -16_000.0 + f * 611.37,
            s: 12.5 + f * 63.11,
            h: -2.0 + (f * 4.7) % 200.0,
            yaw: (f * 0.73) % TAU,
            pitch: -1.2 + (f * 0.05) % 2.4,
            speed: (f * 0.37) % 12.0,
            grounded: i.is_multiple_of(2),
            running: i.is_multiple_of(3),
            ride: (i % 15) as u8,
        }
    }

    fn close(a: &PersonPose, b: &PersonPose) {
        let [dx, ds, dh, dyaw] = PersonPose::STEP;
        assert!((a.x - b.x).abs() <= dx * 0.5 + 1e-3, "x {} {}", a.x, b.x);
        assert!((a.s - b.s).abs() <= ds * 0.5 + 1e-4, "s {} {}", a.s, b.s);
        assert!((a.h - b.h).abs() <= dh * 0.5 + 1e-4, "h {} {}", a.h, b.h);
        let dy = wrap(a.yaw - b.yaw);
        assert!(dy.min(TAU - dy) <= dyaw * 0.5 + 1e-4, "yaw {} {}", a.yaw, b.yaw);
        assert!((a.pitch - b.pitch).abs() < 0.007);
        assert!((a.speed - b.speed).abs() < 0.11);
        assert_eq!((a.grounded, a.running, a.ride), (b.grounded, b.running, b.ride));
    }

    #[test]
    fn a_pose_goes_round_within_half_a_step() {
        let mut buf = [0u8; 64];
        for i in 0..50 {
            let p = PosePacket { seq: i as u16 * 977, pose: pose(i) };
            let n = p.encode(&mut buf).unwrap();
            assert_eq!(n, 14, "a pose is 14 bytes");
            let q = PosePacket::decode(&buf[..n]).unwrap();
            assert_eq!((q.seq, q.pose.strip), (p.seq, p.pose.strip));
            close(&q.pose, &p.pose);
        }
    }

    #[test]
    fn forty_eight_people_fit_one_datagram() {
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut w = PlazaWriter::new(&mut buf, 123_456, Some(2));
        for i in 0..MAX_PEOPLE as u32 {
            assert!(w.push(i as u16 * 7, i as f32 * 0.01, &PersonPose { strip: 2, ..pose(i) }));
        }
        assert!(!w.push(1, 0.0, &pose(0)), "no more than {MAX_PEOPLE}");
        let n = w.finish();
        assert!(n <= MAX_DATAGRAM, "{n} B");
        let mut r = PlazaReader::new(&buf[..n]).unwrap();
        assert_eq!((r.tick, r.strip), (123_456, Some(2)));
        let mut k = 0u32;
        while let Some((id, age, p)) = r.next_person() {
            assert_eq!(id, k as u16 * 7);
            assert!((age - k as f32 * 0.01).abs() <= 0.006, "{age}");
            close(&p, &PersonPose { strip: 2, ..pose(k) });
            k += 1;
        }
        assert_eq!(k, MAX_PEOPLE as u32);
    }

    #[test]
    fn the_heartbeat_in_the_bay_carries_only_the_tick() {
        let mut buf = [0u8; 64];
        let w = PlazaWriter::new(&mut buf, 77, None);
        let n = w.finish();
        let mut r = PlazaReader::new(&buf[..n]).unwrap();
        assert_eq!((r.tick, r.strip), (77, None));
        assert!(r.next_person().is_none());
    }

    #[test]
    fn decoders_never_panic() {
        let mut seed = 0x9e37_79b9u32;
        let mut buf = [0u8; 256];
        for len in 0..256 {
            for b in buf.iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *b = seed as u8;
            }
            buf[0] = (buf[0] & 0xf0) | if len % 2 == 0 { 3 } else { 4 };
            let _ = PosePacket::decode(&buf[..len]);
            if let Ok(mut r) = PlazaReader::new(&buf[..len]) {
                while r.next_person().is_some() {}
            }
        }
    }
}
