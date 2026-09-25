//! Control stream: one reliable, ordered, bidirectional WebTransport stream per session.
//!
//! Frame: `[u16 LE payload length][u8 tag][payload]`. Frames are tiny and rare (handshake, roster
//! changes, respawn requests), so they're byte-aligned for simplicity.

use crate::types::{Faction, FrameId, PilotKind};
use crate::{DecodeError, PROTOCOL_VERSION};

/// Longest pilot name, in UTF-8 bytes.
pub const MAX_NAME: usize = 16;
/// Largest control frame (prefix included).
pub const MAX_FRAME: usize = 64;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlMsg {
    /// Client → server, first frame. `pilot` must be honest: agents identify as [`PilotKind::Agent`].
    Hello { version: u16, pilot: PilotKind, frame: FrameId, faction: Faction, name: Name },
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
    },
    /// Server → client, then the stream closes.
    Reject { reason: RejectReason },
    /// Server → client: the pilot flying entity `slot`.
    Roster { slot: u16, pilot: PilotKind, name: Name },
    /// Client → server: respawn (after death) in `frame`.
    Respawn { frame: FrameId },
    /// Either direction: goodbye.
    Bye { reason: u8 },
}

impl ControlMsg {
    pub fn hello(pilot: PilotKind, frame: FrameId, faction: Faction, name: &str) -> Self {
        ControlMsg::Hello { version: PROTOCOL_VERSION, pilot, frame, faction, name: Name::new(name) }
    }

    /// Encodes one frame (length prefix included) into `buf`; returns its length.
    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        let mut p = Cursor { buf, pos: 2, ok: true };
        match *self {
            ControlMsg::Hello { version, pilot, frame, faction, name } => {
                p.u8(1);
                p.u16(version);
                p.u8(pilot as u8);
                p.u8(frame as u8);
                p.u8(faction as u8);
                p.name(&name);
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
            }
            ControlMsg::Reject { reason } => {
                p.u8(3);
                p.u8(reason as u8);
            }
            ControlMsg::Roster { slot, pilot, name } => {
                p.u8(4);
                p.u16(slot);
                p.u8(pilot as u8);
                p.name(&name);
            }
            ControlMsg::Respawn { frame } => {
                p.u8(5);
                p.u8(frame as u8);
            }
            ControlMsg::Bye { reason } => {
                p.u8(6);
                p.u8(reason);
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
            1 => ControlMsg::Hello {
                version: r.u16()?,
                pilot: PilotKind::from_bits(u32::from(r.u8()?)),
                frame: FrameId::from_bits(u32::from(r.u8()?)).ok_or(DecodeError::Invalid)?,
                faction: Faction::from_bits(u32::from(r.u8()?)),
                name: r.name()?,
            },
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
            },
            3 => ControlMsg::Reject {
                reason: match r.u8()? {
                    1 => RejectReason::VersionMismatch,
                    2 => RejectReason::ServerFull,
                    _ => RejectReason::BadHello,
                },
            },
            4 => ControlMsg::Roster {
                slot: r.u16()?,
                pilot: PilotKind::from_bits(u32::from(r.u8()?)),
                name: r.name()?,
            },
            5 => ControlMsg::Respawn {
                frame: FrameId::from_bits(u32::from(r.u8()?)).ok_or(DecodeError::Invalid)?,
            },
            6 => ControlMsg::Bye { reason: r.u8()? },
            _ => return Err(DecodeError::WrongKind),
        };
        Ok(Some((msg, payload + 2)))
    }
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
            },
            ControlMsg::Roster { slot: 77, pilot: PilotKind::Human, name: Name::new("Zechs") },
            ControlMsg::Respawn { frame: FrameId::Leo },
            ControlMsg::Bye { reason: 0 },
        ];
        let mut stream = [0u8; 512];
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
    fn name_truncates_on_char_boundary() {
        let n = Name::new("ゼクス・マーキス-Zechs");
        assert!(n.as_str().len() <= MAX_NAME);
        assert!(n.as_str().starts_with("ゼクス"));
    }
}
