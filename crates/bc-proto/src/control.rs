//! Control stream: one reliable, ordered, bidirectional WebTransport stream per session.
//!
//! Frame: `[u16 LE payload length][u8 tag][payload]`. Frames are small and rare (handshake, sign-in,
//! roster changes, respawn requests), so they're byte-aligned for simplicity. A [`HANGAR`] frame
//! carries a hangar message instead (JSON, defined by `bc_econ::wire`, up to [`MAX_HANGAR_FRAME`]):
//! read the stream with [`Frame::decode`].

use crate::auth::{Address, Domain, MAX_DOMAIN, NONCE_BYTES, Signature, TOKEN_BYTES};
use crate::types::{Faction, FrameId, PilotKind};
use crate::{DecodeError, PROTOCOL_VERSION};

/// Longest pilot name, in UTF-8 bytes.
pub const MAX_NAME: usize = 16;
/// Largest control frame (prefix included).
pub const MAX_FRAME: usize = 256;
/// The tag of a hangar frame, whose payload is a hangar message (JSON).
pub const HANGAR: u8 = 11;
/// Largest hangar frame (prefix and tag included).
pub const MAX_HANGAR_FRAME: usize = 2 + u16::MAX as usize;

/// [`ControlMsg::Hello`] flags.
pub mod hello_flags {
    /// The pilot will sign in with a wallet: answer with a Challenge.
    pub const SIGN_IN: u8 = 1 << 0;
    /// The pilot signed in earlier and carries a resume token (a reconnect: no signature asked).
    pub const RESUME: u8 = 1 << 1;
}

/// [`ControlMsg::Welcome`] flags.
pub mod welcome_flags {
    /// Signed in: the pilot's suit sleeps when they leave, and wakes when they're back.
    pub const SIGNED_IN: u8 = 1 << 0;
    /// The pilot woke in the suit they left.
    pub const WOKE: u8 = 1 << 1;
    /// Survival rules: the pilot starts in their hangar bay, flies the suit they built, and
    /// launches and docks it with hangar messages. (Otherwise, arcade rules: in a suit at once.)
    pub const SURVIVAL: u8 = 1 << 2;
    /// The colony is open: the bay's airlock leads to the cap lifts, and down to its city.
    pub const COLONY: u8 = 1 << 3;
}

/// [`ControlMsg::Roster`] flags.
pub mod roster_flags {
    /// A signed-in pilot (a wallet), not a guest.
    pub const VERIFIED: u8 = 1 << 0;
    /// Offline: asleep in the cockpit.
    pub const ASLEEP: u8 = 1 << 1;
}

/// [`ControlMsg::Bye`] reasons.
pub mod bye {
    pub const LEAVE: u8 = 0;
    /// The same pilot signed in somewhere else.
    pub const TAKEN_OVER: u8 = 1;
    /// Nothing heard from the client for too long.
    pub const IDLE: u8 = 2;
    pub const SHUTDOWN: u8 = 3;
}

/// [`ControlMsg::Notice`] codes.
pub mod notice {
    /// The pilot's sleeping suit was destroyed while they were away; `name` is by whom.
    pub const SLEEPER_DESTROYED: u8 = 1;
    /// The pilot's sleeping suit is gone (the sector restarted, or it was cleared for room).
    pub const SLEEPER_LOST: u8 = 2;
}

/// A short pilot name stored inline (no allocation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name {
    len: u8,
    bytes: [u8; MAX_NAME],
}

impl Name {
    /// Truncates to [`MAX_NAME`] bytes on a character boundary and drops control characters.
    pub fn new(s: &str) -> Self {
        let mut bytes = [0u8; MAX_NAME];
        let mut len = 0usize;
        for ch in s.chars().filter(|c| !c.is_control()) {
            let mut tmp = [0u8; 4];
            let enc = ch.encode_utf8(&mut tmp).as_bytes();
            if len + enc.len() > MAX_NAME {
                break;
            }
            bytes[len..len + enc.len()].copy_from_slice(enc);
            len += enc.len();
        }
        Self { len: len as u8, bytes }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("?")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for Name {
    fn default() -> Self {
        Self::new("")
    }
}

/// Why the server refused a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RejectReason {
    VersionMismatch = 1,
    ServerFull = 2,
    BadHello = 3,
    /// The frame asked for isn't one pilots may fly (a Mobile Doll's, or a form like Neo-Bird).
    FrameNotAllowed = 4,
    /// The signature didn't prove the address.
    AuthFailed = 5,
    /// This server admits signed-in pilots only.
    AuthRequired = 6,
    /// The resume token is unknown or expired: sign in again.
    ResumeExpired = 7,
    /// No signature came in time.
    AuthTimeout = 8,
}

impl RejectReason {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::VersionMismatch,
            2 => Self::ServerFull,
            4 => Self::FrameNotAllowed,
            5 => Self::AuthFailed,
            6 => Self::AuthRequired,
            7 => Self::ResumeExpired,
            8 => Self::AuthTimeout,
            _ => Self::BadHello,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlMsg {
    /// Client → server, first frame. `pilot` must be honest: agents identify as [`PilotKind::Agent`].
    /// `flags` are [`hello_flags`]; `resume` is the resume token when [`hello_flags::RESUME`] is set.
    Hello {
        version: u16,
        pilot: PilotKind,
        frame: FrameId,
        faction: Faction,
        name: Name,
        flags: u8,
        resume: [u8; TOKEN_BYTES],
    },
    /// Server → client: you're in.
    Welcome {
        version: u16,
        client_slot: u16,
        tick: u32,
        tick_hz: u8,
        sector: u8,
        zero_allowed: bool,
        max_datagram: u16,
        /// The debris field: build it with `bc_sim::field::Field::generate(field_seed, field_rocks)`.
        field_seed: u32,
        field_rocks: u16,
        /// [`welcome_flags`].
        flags: u8,
        /// How many of the compiled landmarks (`bc_sim::content::landmarks::LANDMARKS`) the sector
        /// has: the first this many. Riders name them by index.
        landmarks: u8,
    },
    /// Server → client, then the stream closes.
    Reject { reason: RejectReason },
    /// Server → client: the pilot flying entity `slot` ([`roster_flags`]).
    Roster { slot: u16, pilot: PilotKind, name: Name, flags: u8 },
    /// Client → server: respawn (after death) in `frame`.
    Respawn { frame: FrameId },
    /// Either direction: goodbye ([`bye`]).
    Bye { reason: u8 },
    /// Server → client, after a Hello with [`hello_flags::SIGN_IN`]: sign in to `domain` with this
    /// nonce, at this time (Unix seconds). The text to sign is `bc_auth::siwe_message`'s.
    Challenge { nonce: [u8; NONCE_BYTES], issued_at: u64, domain: Domain },
    /// Client → server: the wallet's address and its signature over the challenge's message.
    Auth { address: Address, signature: Signature },
    /// Server → client, after the Welcome of a signed-in pilot: reconnect with this instead of
    /// signing again.
    Token { token: [u8; TOKEN_BYTES] },
    /// Server → client: something the pilot should know ([`notice`]); `name` is the code's detail.
    Notice { code: u8, name: Name },
}

impl ControlMsg {
    pub fn hello(pilot: PilotKind, frame: FrameId, faction: Faction, name: &str) -> Self {
        ControlMsg::Hello {
            version: PROTOCOL_VERSION,
            pilot,
            frame,
            faction,
            name: Name::new(name),
            flags: 0,
            resume: [0; TOKEN_BYTES],
        }
    }

    /// Encodes one frame (length prefix included) into `buf`; returns its length.
    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        let mut p = Cursor { buf, pos: 2, ok: true };
        match *self {
            ControlMsg::Hello { version, pilot, frame, faction, name, flags, resume } => {
                p.u8(1);
                p.u16(version);
                p.u8(pilot as u8);
                p.u8(frame as u8);
                p.u8(faction as u8);
                p.name(&name);
                p.u8(flags);
                if flags & hello_flags::RESUME != 0 {
                    p.bytes(&resume);
                }
            }
            ControlMsg::Welcome {
                version,
                client_slot,
                tick,
                tick_hz,
                sector,
                zero_allowed,
                max_datagram,
                field_seed,
                field_rocks,
                flags,
                landmarks,
            } => {
                p.u8(2);
                p.u16(version);
                p.u16(client_slot);
                p.u32(tick);
                p.u8(tick_hz);
                p.u8(sector);
                p.u8(u8::from(zero_allowed));
                p.u16(max_datagram);
                p.u32(field_seed);
                p.u16(field_rocks);
                p.u8(flags);
                p.u8(landmarks);
            }
            ControlMsg::Reject { reason } => {
                p.u8(3);
                p.u8(reason as u8);
            }
            ControlMsg::Roster { slot, pilot, name, flags } => {
                p.u8(4);
                p.u16(slot);
                p.u8(pilot as u8);
                p.name(&name);
                p.u8(flags);
            }
            ControlMsg::Respawn { frame } => {
                p.u8(5);
                p.u8(frame as u8);
            }
            ControlMsg::Bye { reason } => {
                p.u8(6);
                p.u8(reason);
            }
            ControlMsg::Challenge { nonce, issued_at, domain } => {
                p.u8(7);
                p.bytes(&nonce);
                p.u64(issued_at);
                p.u8(domain.len);
                p.bytes(&domain.bytes[..domain.len as usize]);
            }
            ControlMsg::Auth { address, signature } => {
                p.u8(8);
                p.bytes(&address.0);
                p.bytes(&signature.0);
            }
            ControlMsg::Token { token } => {
                p.u8(9);
                p.bytes(&token);
            }
            ControlMsg::Notice { code, name } => {
                p.u8(10);
                p.u8(code);
                p.name(&name);
            }
        }
        if !p.ok {
            return None;
        }
        let len = p.pos;
        let payload = (len - 2) as u16;
        p.buf[..2].copy_from_slice(&payload.to_le_bytes());
        Some(len)
    }

    /// Decodes the first complete frame in `buf`. `Ok(None)` means more bytes are needed; on success
    /// returns the message and how many bytes it consumed.
    pub fn decode(buf: &[u8]) -> Result<Option<(Self, usize)>, DecodeError> {
        if buf.len() < 2 {
            return Ok(None);
        }
        let payload = u16::from_le_bytes([buf[0], buf[1]]) as usize;
        if payload == 0 || payload + 2 > MAX_FRAME {
            return Err(DecodeError::Invalid);
        }
        if buf.len() < payload + 2 {
            return Ok(None);
        }
        let mut r = Reader { buf: &buf[2..2 + payload], pos: 0 };
        let msg = match r.u8()? {
            1 => {
                let version = r.u16()?;
                if version == PROTOCOL_VERSION {
                    let pilot = PilotKind::from_bits(u32::from(r.u8()?));
                    let frame = FrameId::from_bits(u32::from(r.u8()?)).ok_or(DecodeError::Invalid)?;
                    let faction = Faction::from_bits(u32::from(r.u8()?));
                    let name = r.name()?;
                    let flags = r.u8()?;
                    let resume = if flags & hello_flags::RESUME != 0 {
                        r.array::<TOKEN_BYTES>()?
                    } else {
                        [0; TOKEN_BYTES]
                    };
                    ControlMsg::Hello { version, pilot, frame, faction, name, flags, resume }
                } else {
                    // Another protocol's Hello may not parse as ours (a frame this build doesn't
                    // know): keep only its version, so the server can answer VersionMismatch.
                    ControlMsg::Hello {
                        version,
                        pilot: PilotKind::Human,
                        frame: FrameId::Leo,
                        faction: Faction::Oz,
                        name: Name::default(),
                        flags: 0,
                        resume: [0; TOKEN_BYTES],
                    }
                }
            }
            2 => ControlMsg::Welcome {
                version: r.u16()?,
                client_slot: r.u16()?,
                tick: r.u32()?,
                tick_hz: r.u8()?,
                sector: r.u8()?,
                zero_allowed: r.u8()? != 0,
                max_datagram: r.u16()?,
                field_seed: r.u32()?,
                field_rocks: r.u16()?,
                flags: r.u8()?,
                landmarks: r.u8()?,
            },
            3 => ControlMsg::Reject { reason: RejectReason::from_u8(r.u8()?) },
            4 => ControlMsg::Roster {
                slot: r.u16()?,
                pilot: PilotKind::from_bits(u32::from(r.u8()?)),
                name: r.name()?,
                flags: r.u8()?,
            },
            5 => ControlMsg::Respawn {
                frame: FrameId::from_bits(u32::from(r.u8()?)).ok_or(DecodeError::Invalid)?,
            },
            6 => ControlMsg::Bye { reason: r.u8()? },
            7 => {
                let nonce = r.array::<NONCE_BYTES>()?;
                let issued_at = r.u64()?;
                let len = r.u8()? as usize;
                if len > MAX_DOMAIN {
                    return Err(DecodeError::Invalid);
                }
                let raw = r.take(len)?;
                let text = core::str::from_utf8(raw).map_err(|_| DecodeError::Invalid)?;
                ControlMsg::Challenge { nonce, issued_at, domain: Domain::new(text) }
            }
            8 => ControlMsg::Auth { address: Address(r.array()?), signature: Signature(r.array()?) },
            9 => ControlMsg::Token { token: r.array()? },
            10 => ControlMsg::Notice { code: r.u8()?, name: r.name()? },
            _ => return Err(DecodeError::WrongKind),
        };
        Ok(Some((msg, payload + 2)))
    }
}

/// One frame off the control stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame<'a> {
    Msg(ControlMsg),
    /// A hangar message's payload (JSON).
    Hangar(&'a [u8]),
}

impl<'a> Frame<'a> {
    /// Decodes the first complete frame in `buf`. `Ok(None)` means more bytes are needed; on
    /// success returns the frame and how many bytes it took.
    pub fn decode(buf: &'a [u8]) -> Result<Option<(Frame<'a>, usize)>, DecodeError> {
        if buf.len() < 3 {
            return Ok(None);
        }
        if buf[2] != HANGAR {
            return ControlMsg::decode(buf).map(|m| m.map(|(msg, used)| (Frame::Msg(msg), used)));
        }
        let payload = u16::from_le_bytes([buf[0], buf[1]]) as usize;
        if payload == 0 {
            return Err(DecodeError::Invalid);
        }
        if buf.len() < payload + 2 {
            return Ok(None);
        }
        Ok(Some((Frame::Hangar(&buf[3..payload + 2]), payload + 2)))
    }
}

/// Writes a hangar frame carrying `payload` into `out`. Its length, or `None` if it's too long for
/// a frame or for `out`.
pub fn encode_hangar(payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let len = payload.len() + 1;
    if len > u16::MAX as usize || out.len() < len + 2 {
        return None;
    }
    out[..2].copy_from_slice(&(len as u16).to_le_bytes());
    out[2] = HANGAR;
    out[3..len + 2].copy_from_slice(payload);
    Some(len + 2)
}

struct Cursor<'a> {
    buf: &'a mut [u8],
    pos: usize,
    ok: bool,
}

impl Cursor<'_> {
    fn bytes(&mut self, b: &[u8]) {
        if !self.ok || self.pos + b.len() > self.buf.len().min(MAX_FRAME) {
            self.ok = false;
            return;
        }
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
    }
    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn name(&mut self, n: &Name) {
        self.u8(n.len);
        self.bytes(&n.bytes[..n.len as usize]);
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        let end = self.pos + n;
        let s = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    fn name(&mut self) -> Result<Name, DecodeError> {
        let len = self.u8()? as usize;
        if len > MAX_NAME {
            return Err(DecodeError::Invalid);
        }
        let raw = self.take(len)?;
        let s = core::str::from_utf8(raw).map_err(|_| DecodeError::Invalid)?;
        Ok(Name::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_split() {
        let msgs = [
            ControlMsg::hello(PilotKind::Agent, FrameId::WingZero, Faction::Colonies, "Heero ユイ"),
            ControlMsg::Welcome {
                version: PROTOCOL_VERSION,
                client_slot: 3,
                tick: 123_456,
                tick_hz: 30,
                sector: 1,
                zero_allowed: true,
                max_datagram: 1100,
                field_seed: 0xDEB12,
                field_rocks: 160,
                flags: welcome_flags::SIGNED_IN | welcome_flags::WOKE,
                landmarks: 2,
            },
            ControlMsg::Reject { reason: RejectReason::FrameNotAllowed },
            ControlMsg::Reject { reason: RejectReason::VersionMismatch },
            ControlMsg::Reject { reason: RejectReason::ResumeExpired },
            ControlMsg::Roster {
                slot: 77,
                pilot: PilotKind::Human,
                name: Name::new("Zechs"),
                flags: roster_flags::VERIFIED | roster_flags::ASLEEP,
            },
            ControlMsg::Respawn { frame: FrameId::Leo },
            ControlMsg::Bye { reason: bye::TAKEN_OVER },
            ControlMsg::Hello {
                version: PROTOCOL_VERSION,
                pilot: PilotKind::Human,
                frame: FrameId::Sandrock,
                faction: Faction::Colonies,
                name: Name::new("Quatre"),
                flags: hello_flags::RESUME,
                resume: [0xAB; TOKEN_BYTES],
            },
            ControlMsg::Challenge {
                nonce: core::array::from_fn(|i| i as u8),
                issued_at: 1_790_000_000,
                domain: Domain::new("127.0.0.1:8080"),
            },
            ControlMsg::Auth { address: Address([0x7E; 20]), signature: Signature([0x1C; 65]) },
            ControlMsg::Token { token: [9; TOKEN_BYTES] },
            ControlMsg::Notice { code: notice::SLEEPER_DESTROYED, name: Name::new("Taurus-03") },
        ];
        let mut stream = [0u8; 2_048];
        let mut len = 0;
        for m in &msgs {
            len += m.encode(&mut stream[len..]).unwrap();
        }
        // Partial frame: needs more bytes.
        assert_eq!(ControlMsg::decode(&stream[..3]).unwrap(), None);
        let mut pos = 0;
        for m in &msgs {
            let (got, used) = ControlMsg::decode(&stream[pos..len]).unwrap().unwrap();
            assert_eq!(&got, m);
            pos += used;
        }
        assert_eq!(pos, len);
    }

    #[test]
    fn welcome_carries_landmarks() {
        let welcome = |landmarks| ControlMsg::Welcome {
            version: PROTOCOL_VERSION,
            client_slot: 0,
            tick: 1,
            tick_hz: 30,
            sector: 1,
            zero_allowed: false,
            max_datagram: 1100,
            field_seed: 7,
            field_rocks: 0,
            flags: 0,
            landmarks,
        };
        let mut buf = [0u8; MAX_FRAME];
        for landmarks in [0, 2, 16, 255] {
            let n = welcome(landmarks).encode(&mut buf).unwrap();
            // Tag, version, slot, tick, tick_hz, sector, zero_allowed, max_datagram, field seed and
            // rocks, flags, and the landmarks' one byte last.
            assert_eq!(n, 2 + 1 + 2 + 2 + 4 + 1 + 1 + 1 + 2 + 4 + 2 + 1 + 1);
            assert_eq!(buf[n - 1], landmarks);
            assert_eq!(ControlMsg::decode(&buf[..n]).unwrap(), Some((welcome(landmarks), n)));
            // A Welcome without it (an older server's) is cut short.
            buf[0] -= 1;
            assert_eq!(ControlMsg::decode(&buf[..n - 1]), Err(DecodeError::Truncated));
        }
    }

    #[test]
    fn another_versions_hello_still_gives_its_version() {
        // Version 99 asking for a frame this build has never heard of.
        let frame = [8u8, 0, 1, 99, 0, 0, 200, 1, 0, 0];
        let (msg, used) = ControlMsg::decode(&frame).unwrap().unwrap();
        assert_eq!(used, frame.len());
        assert!(matches!(msg, ControlMsg::Hello { version: 99, .. }));
        // Our own version still validates the frame.
        let mut ours = frame;
        ours[3..5].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        assert_eq!(ControlMsg::decode(&ours), Err(DecodeError::Invalid));
    }

    #[test]
    fn sign_in_frames_have_their_sizes() {
        let mut buf = [0u8; MAX_FRAME];
        let auth = ControlMsg::Auth { address: Address([1; 20]), signature: Signature([2; 65]) };
        assert_eq!(auth.encode(&mut buf), Some(88));
        assert_eq!(&buf[..3], &[86, 0, 8]);
        let challenge = ControlMsg::Challenge {
            nonce: [3; NONCE_BYTES],
            issued_at: 7,
            domain: Domain::new("127.0.0.1:8080"),
        };
        assert_eq!(challenge.encode(&mut buf), Some(2 + 1 + NONCE_BYTES + 8 + 1 + 14));
        // The longest domain still fits.
        let long = core::str::from_utf8(&[b'd'; MAX_DOMAIN]).unwrap();
        let challenge =
            ControlMsg::Challenge { nonce: [3; NONCE_BYTES], issued_at: 7, domain: Domain::new(long) };
        let n = challenge.encode(&mut buf).unwrap();
        assert_eq!(ControlMsg::decode(&buf[..n]).unwrap().unwrap().0, challenge);
    }

    #[test]
    fn frames_up_to_the_limit_decode_and_past_it_are_refused() {
        let mut big = [0u8; MAX_FRAME + 1];
        big[2] = 6; // a Bye, padded out
        big[..2].copy_from_slice(&((MAX_FRAME - 2) as u16).to_le_bytes());
        assert!(matches!(
            ControlMsg::decode(&big[..MAX_FRAME]),
            Ok(Some((ControlMsg::Bye { .. }, MAX_FRAME)))
        ));
        big[..2].copy_from_slice(&((MAX_FRAME - 1) as u16).to_le_bytes());
        assert_eq!(ControlMsg::decode(&big), Err(DecodeError::Invalid));
    }

    #[test]
    fn a_guest_hello_carries_no_token() {
        let mut buf = [0u8; MAX_FRAME];
        let n = ControlMsg::hello(PilotKind::Human, FrameId::Leo, Faction::Colonies, "")
            .encode(&mut buf)
            .unwrap();
        // Tag, version, pilot, frame, faction, an empty name, flags.
        assert_eq!(n, 2 + 1 + 2 + 3 + 1 + 1);
    }

    #[test]
    fn hangar_frames_ride_between_control_frames() {
        let json = br#"{"t":"craft","item":"mat.steel","batches":3}"#;
        let big = [b'x'; 40_000];
        let mut stream = [0u8; 50_000];
        let mut len = ControlMsg::Bye { reason: bye::LEAVE }.encode(&mut stream).unwrap();
        len += encode_hangar(json, &mut stream[len..]).unwrap();
        len += encode_hangar(&big, &mut stream[len..]).unwrap();
        len += ControlMsg::Respawn { frame: FrameId::Leo }.encode(&mut stream[len..]).unwrap();
        let mut pos = 0;
        let mut frames = 0;
        while let Some((frame, used)) = Frame::decode(&stream[pos..len]).unwrap() {
            match (frames, frame) {
                (0, Frame::Msg(ControlMsg::Bye { .. })) | (3, Frame::Msg(ControlMsg::Respawn { .. })) => {}
                (1, Frame::Hangar(p)) => assert_eq!(p, json),
                (2, Frame::Hangar(p)) => assert_eq!(p, big),
                other => panic!("{other:?}"),
            }
            frames += 1;
            pos += used;
            // Every cut short of the whole frame asks for more.
            if frames == 2 {
                for cut in 0..used {
                    assert_eq!(Frame::decode(&stream[pos - used..pos - used + cut]).unwrap(), None);
                }
            }
        }
        assert_eq!((frames, pos), (4, len));
        // Too long for a frame, or for the buffer.
        assert_eq!(encode_hangar(&[0u8; 70_000], &mut [0u8; 80_000]), None);
        assert_eq!(encode_hangar(json, &mut [0u8; 8]), None);
        // An empty payload is no frame.
        assert_eq!(Frame::decode(&[0, 0, HANGAR]), Err(DecodeError::Invalid));
    }

    #[test]
    fn name_truncates_on_char_boundary() {
        let n = Name::new("ゼクス・マーキス-Zechs");
        assert!(n.as_str().len() <= MAX_NAME);
        assert!(n.as_str().starts_with("ゼクス"));
    }
}
