//! Proximity voice: one Opus frame per datagram, both ways ([`PacketKind::Voice`]).
//!
//! Up (client → server) a talker sends their own frame: a sequence number, whether it's the last of
//! one press of the talk key, and the payload. Down (server → client) the server relays it to each
//! listener it picked, saying who is talking and how it came: through the air on foot (the talker is
//! the plaza's id, their session's), or over a suit's radio in space (the talker is their suit's
//! *entity* slot, as the roster names it: never a session's id, which would tie a suit to the pilot
//! on foot). The server does the cull: a client never receives a frame from beyond the reach, so its
//! own falloff is only how it sounds.
//!
//! The payload is opaque here: an Opus packet (the browser's WebCodecs encoder), at most
//! [`VOICE_MAX_BYTES`]. The server checks only its TOC byte ([`is_one_frame`]) so nobody can make a
//! listener's decoder chew on 120 ms at once; it never decodes, keeps or logs one.
//!
//! The layout is whole bytes (the first is the kind and the flags, as the bit writer packs them),
//! so a frame is a header and its payload:
//!
//! | | kind (4) | flags (4) | talker (16) | seq (16) | len (8) | payload |
//! |---|---|---|---|---|---|---|
//! | up | 5 | END | — | ✓ | ✓ | `len` bytes |
//! | down | 5 | END, RADIO | ✓ | ✓ | ✓ | `len` bytes |
//!
//! Nothing here allocates; decoders are total and never panic.

use crate::{DecodeError, MAX_DATAGRAM, MAX_ENTITIES, PacketKind};

/// Longest Opus payload one frame carries. 20 ms at 64 kbps is 160 bytes; the browser encodes near
/// 24 kbps (about 60 B), so this is headroom, not a target.
pub const VOICE_MAX_BYTES: usize = 160;

/// Flags: the last frame of one press of the talk key (a listener's "talking" mark goes out on it).
pub const VOICE_END: u8 = 1;
/// Flags, down only: over a suit's radio (the talker is a suit's entity slot), not through the air.
pub const VOICE_RADIO: u8 = 2;

/// Bytes before the payload, up and down.
pub const VOICE_UP_HEADER: usize = 4;
pub const VOICE_DOWN_HEADER: usize = 6;
/// The longest voice datagram, each way.
pub const VOICE_UP_MAX: usize = VOICE_UP_HEADER + VOICE_MAX_BYTES;
pub const VOICE_DOWN_MAX: usize = VOICE_DOWN_HEADER + VOICE_MAX_BYTES;
const _: () = assert!(VOICE_DOWN_MAX <= MAX_DATAGRAM && VOICE_UP_MAX <= VOICE_DOWN_MAX);

/// How a voice reached the listener.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Via {
    /// Through the air, on foot: from where the talker stands.
    #[default]
    Air,
    /// Over a suit's radio, in space: no position, a radio's sound.
    Radio,
}

/// One voice frame. Fixed size: no allocation, ever.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VoiceFrame {
    /// Down: who's talking. Through the air, the plaza's id (their session's); over the radio, their
    /// suit's entity slot (as the roster names it). Up: 0 (the server knows who sent it).
    pub talker: u16,
    pub via: Via,
    pub seq: u16,
    /// The last frame of one press of the talk key.
    pub end: bool,
    len: u8,
    bytes: [u8; VOICE_MAX_BYTES],
}

impl core::fmt::Debug for VoiceFrame {
    /// Never the payload: what someone said is nobody's log line.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VoiceFrame")
            .field("talker", &self.talker)
            .field("via", &self.via)
            .field("seq", &self.seq)
            .field("end", &self.end)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl Default for VoiceFrame {
    fn default() -> Self {
        Self::EMPTY
    }
}

fn first_byte(flags: u8) -> u8 {
    PacketKind::Voice as u8 | flags << 4
}

impl VoiceFrame {
    pub const EMPTY: Self =
        Self { talker: 0, via: Via::Air, seq: 0, end: false, len: 0, bytes: [0; VOICE_MAX_BYTES] };

    /// A frame round `payload`; `None` when it's empty or over [`VOICE_MAX_BYTES`].
    pub fn new(seq: u16, end: bool, payload: &[u8]) -> Option<Self> {
        if payload.is_empty() || payload.len() > VOICE_MAX_BYTES {
            return None;
        }
        let mut f = Self::EMPTY;
        f.seq = seq;
        f.end = end;
        f.len = payload.len() as u8;
        f.bytes[..payload.len()].copy_from_slice(payload);
        Some(f)
    }

    /// This frame as relayed: from `talker`, `via`.
    pub fn relayed(mut self, talker: u16, via: Via) -> Self {
        self.talker = talker;
        self.via = via;
        self
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    fn write_tail(&self, buf: &mut [u8]) {
        buf[..2].copy_from_slice(&self.seq.to_le_bytes());
        buf[2] = self.len;
        buf[3..3 + self.len as usize].copy_from_slice(self.payload());
    }

    /// Client → server. `None` if `buf` is too short or the frame is empty.
    pub fn encode_up(&self, buf: &mut [u8]) -> Option<usize> {
        let n = VOICE_UP_HEADER + self.len as usize;
        if self.len == 0 || buf.len() < n {
            return None;
        }
        buf[0] = first_byte(if self.end { VOICE_END } else { 0 });
        self.write_tail(&mut buf[1..]);
        Some(n)
    }

    /// Server → client. `None` if `buf` is too short, the frame is empty, or a radio talker isn't an
    /// entity slot.
    pub fn encode_down(&self, buf: &mut [u8]) -> Option<usize> {
        let n = VOICE_DOWN_HEADER + self.len as usize;
        if self.len == 0 || buf.len() < n || (self.via == Via::Radio && self.talker as usize >= MAX_ENTITIES)
        {
            return None;
        }
        let radio = if self.via == Via::Radio { VOICE_RADIO } else { 0 };
        buf[0] = first_byte(if self.end { VOICE_END } else { 0 } | radio);
        buf[1..3].copy_from_slice(&self.talker.to_le_bytes());
        self.write_tail(&mut buf[3..]);
        Some(n)
    }

    /// The kind, the flags and the rest; the flags must be within `allowed`.
    fn head(bytes: &[u8], allowed: u8) -> Result<u8, DecodeError> {
        let first = *bytes.first().ok_or(DecodeError::Truncated)?;
        if first & 0x0f != PacketKind::Voice as u8 {
            return Err(DecodeError::WrongKind);
        }
        let flags = first >> 4;
        if flags & !allowed != 0 {
            return Err(DecodeError::Invalid);
        }
        Ok(flags)
    }

    /// The sequence number, length and payload, from `tail` (just after the talker, or the flags);
    /// the datagram must end with the payload.
    fn read_tail(tail: &[u8]) -> Result<Self, DecodeError> {
        let [s0, s1, len, rest @ ..] = tail else { return Err(DecodeError::Truncated) };
        let len = *len as usize;
        if len == 0 || len > VOICE_MAX_BYTES {
            return Err(DecodeError::Invalid);
        }
        match rest.len().cmp(&len) {
            core::cmp::Ordering::Less => return Err(DecodeError::Truncated),
            core::cmp::Ordering::Greater => return Err(DecodeError::Invalid),
            core::cmp::Ordering::Equal => {}
        }
        let mut f = Self::EMPTY;
        f.seq = u16::from_le_bytes([*s0, *s1]);
        f.len = len as u8;
        f.bytes[..len].copy_from_slice(rest);
        Ok(f)
    }

    /// Total decode of a client's frame (hostile bytes: never panics).
    pub fn decode_up(bytes: &[u8]) -> Result<Self, DecodeError> {
        let flags = Self::head(bytes, VOICE_END)?;
        let mut f = Self::read_tail(&bytes[1..])?;
        f.end = flags & VOICE_END != 0;
        Ok(f)
    }

    /// Total decode of a relayed frame.
    pub fn decode_down(bytes: &[u8]) -> Result<Self, DecodeError> {
        let flags = Self::head(bytes, VOICE_END | VOICE_RADIO)?;
        let [_, t0, t1, tail @ ..] = bytes else { return Err(DecodeError::Truncated) };
        let mut f = Self::read_tail(tail)?;
        f.talker = u16::from_le_bytes([*t0, *t1]);
        f.end = flags & VOICE_END != 0;
        f.via = if flags & VOICE_RADIO != 0 { Via::Radio } else { Via::Air };
        if f.via == Via::Radio && f.talker as usize >= MAX_ENTITIES {
            return Err(DecodeError::Invalid);
        }
        Ok(f)
    }
}

/// Whether an Opus packet is one 20 ms frame (the only shape a talker sends), read off its TOC
/// byte (RFC 6716 §3.1): code 0 (one frame), and a configuration whose frame is 20 ms. Anything
/// else is refused at the server, so a forged packet can't make a listener decode 120 ms at once.
/// (Gates' `crates/voice`, as is.)
pub fn is_one_frame(packet: &[u8]) -> bool {
    let Some(&toc) = packet.first() else {
        return false;
    };
    if toc & 3 != 0 {
        return false;
    }
    let config = toc >> 3;
    match config {
        // SILK: 10, 20, 40, 60 ms by config % 4.
        0..=11 => config % 4 == 1,
        // Hybrid: 10, 20 ms.
        12..=15 => config % 2 == 1,
        // CELT: 2.5, 5, 10, 20 ms.
        _ => config % 4 == 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet_kind;

    #[test]
    fn a_frame_round_trips_both_ways_at_the_edges() {
        let payload = [0x5au8; VOICE_MAX_BYTES];
        let f = VoiceFrame::new(65_535, true, &payload).unwrap();
        let mut buf = [0u8; VOICE_DOWN_MAX];
        let n = f.encode_up(&mut buf).unwrap();
        assert_eq!(n, VOICE_UP_MAX);
        assert_eq!(packet_kind(&buf[..n]), Some(PacketKind::Voice));
        assert_eq!(VoiceFrame::decode_up(&buf[..n]).unwrap(), f);

        for (talker, via) in [(65_534, Via::Air), (1_022, Via::Radio), (0, Via::Radio)] {
            let down = f.relayed(talker, via);
            let n = down.encode_down(&mut buf).unwrap();
            assert_eq!(n, VOICE_DOWN_MAX);
            assert_eq!(packet_kind(&buf[..n]), Some(PacketKind::Voice));
            assert_eq!(VoiceFrame::decode_down(&buf[..n]).unwrap(), down);
        }
        let short = VoiceFrame::new(7, false, &[0x08, 1, 2]).unwrap().relayed(3, Via::Air);
        let n = short.encode_down(&mut buf).unwrap();
        assert_eq!(n, VOICE_DOWN_HEADER + 3);
        let back = VoiceFrame::decode_down(&buf[..n]).unwrap();
        assert_eq!(
            (back.talker, back.via, back.seq, back.end, back.payload()),
            (3, Via::Air, 7, false, &[8, 1, 2][..])
        );
    }

    #[test]
    fn hostile_frames_are_refused() {
        assert!(VoiceFrame::new(0, false, &[]).is_none());
        assert!(VoiceFrame::new(0, false, &[0; VOICE_MAX_BYTES + 1]).is_none());
        assert!(VoiceFrame::EMPTY.encode_up(&mut [0; 8]).is_none());
        // A radio talker past the entity slots isn't one.
        let f = VoiceFrame::new(1, false, &[8; 20]).unwrap();
        assert!(f.relayed(1_024, Via::Radio).encode_down(&mut [0; 64]).is_none());
        let mut buf = [0u8; VOICE_DOWN_MAX + 1];
        let n = f.encode_up(&mut buf).unwrap();
        // Truncated, trailing junk, a forged length, another kind, an unknown flag (RADIO up).
        assert_eq!(VoiceFrame::decode_up(&buf[..n - 1]), Err(DecodeError::Truncated));
        assert_eq!(VoiceFrame::decode_up(&buf[..n + 1]), Err(DecodeError::Invalid));
        assert_eq!(VoiceFrame::decode_up(&buf[..3]), Err(DecodeError::Truncated));
        assert_eq!(VoiceFrame::decode_up(&[]), Err(DecodeError::Truncated));
        let mut forged = buf;
        forged[3] = 0;
        assert_eq!(VoiceFrame::decode_up(&forged[..n]), Err(DecodeError::Invalid));
        forged[3] = 200;
        assert_eq!(VoiceFrame::decode_up(&forged[..n]), Err(DecodeError::Invalid));
        let mut other = buf;
        other[0] = (other[0] & 0xf0) | PacketKind::Pose as u8;
        assert_eq!(VoiceFrame::decode_up(&other[..n]), Err(DecodeError::WrongKind));
        let mut radio_up = buf;
        radio_up[0] |= VOICE_RADIO << 4;
        assert_eq!(VoiceFrame::decode_up(&radio_up[..n]), Err(DecodeError::Invalid));
        let mut flag8 = buf;
        flag8[0] |= 8 << 4;
        assert_eq!(VoiceFrame::decode_up(&flag8[..n]), Err(DecodeError::Invalid));
        // Down: a radio talker past the entity slots, and an up frame read as a down one.
        let n = f.relayed(1_000, Via::Radio).encode_down(&mut buf).unwrap();
        let mut slot = buf;
        slot[1..3].copy_from_slice(&1_024u16.to_le_bytes());
        assert_eq!(VoiceFrame::decode_down(&slot[..n]), Err(DecodeError::Invalid));
        let n = f.encode_up(&mut buf).unwrap();
        assert!(VoiceFrame::decode_down(&buf[..n]).is_err());
        // The payload is never in a log line.
        let mut line = Dbg::default();
        core::fmt::write(&mut line, format_args!("{f:?}")).unwrap();
        assert!(!line.saw_payload, "{:?}", &line.buf[..line.n]);
    }

    /// A tiny `fmt::Write` with no allocation: does the Debug text show the payload's bytes?
    struct Dbg {
        buf: [u8; 256],
        n: usize,
        saw_payload: bool,
    }

    impl Default for Dbg {
        fn default() -> Self {
            Self { buf: [0; 256], n: 0, saw_payload: false }
        }
    }

    impl core::fmt::Write for Dbg {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            self.saw_payload |= s.contains("bytes") || s.contains("[8");
            let b = s.as_bytes();
            let k = b.len().min(self.buf.len() - self.n);
            self.buf[self.n..self.n + k].copy_from_slice(&b[..k]);
            self.n += k;
            Ok(())
        }
    }

    #[test]
    fn only_one_20_ms_frame_passes_the_toc_check() {
        // SILK 20 ms (configs 1, 5, 9), hybrid 20 ms (13, 15), CELT 20 ms (19, 23, 27, 31): code 0.
        for config in [1u8, 5, 9, 13, 15, 19, 23, 27, 31] {
            assert!(is_one_frame(&[config << 3, 0]), "config {config}");
            // Stereo is still one frame of 20 ms.
            assert!(is_one_frame(&[config << 3 | 4]), "config {config} stereo");
            // Codes 1–3: two frames, or a count of them (code 3 up to 120 ms).
            for code in 1..=3u8 {
                assert!(!is_one_frame(&[config << 3 | code]), "config {config} code {code}");
            }
        }
        // 10, 40 and 60 ms SILK; 10 ms hybrid; 2.5, 5, 10 ms CELT.
        for config in [0u8, 2, 3, 4, 6, 7, 8, 10, 11, 12, 14, 16, 17, 18, 20, 30] {
            assert!(!is_one_frame(&[config << 3]), "config {config}");
        }
        assert!(!is_one_frame(&[]));
        // Chrome's WebCodecs Opus at 48 kHz, 20 ms, mono VoIP: CELT FB 20 ms or SILK WB 20 ms.
        assert!(is_one_frame(&[0xf8]));
        assert!(is_one_frame(&[0x48]));
        assert!(is_one_frame(&[0x08]));
    }
}
