//! Before Colony wire protocol.
//!
//! - `#![no_std]` with **no `alloc`**: nothing here can allocate, by construction. Every encoder
//!   writes into a caller-provided buffer and every decoder reads from a borrowed slice.
//! - Datagrams (unreliable, one QUIC packet each, never fragmented):
//!   - client → server [`InputPacket`]: the last up-to-4 [`InputCmd`]s, so one lost packet costs
//!     nothing.
//!   - server → client snapshot ([`SnapshotWriter`] / [`SnapshotReader`]): header, full-precision
//!     own state, ZERO info, events repeated until acked, then as many prioritised entities as fit
//!     in [`MAX_DATAGRAM`] bytes.
//! - Control stream (reliable, length-prefixed frames): [`control`] handshake and roster messages.
//!
//! Bit layouts are documented in `docs/PROTOCOL.md`.

#![no_std]
#![forbid(unsafe_code)]

pub mod bits;
pub mod control;
pub mod events;
pub mod input;
pub mod quant;
pub mod snapshot;
pub mod types;

pub use bits::{BitReader, BitWriter};
pub use events::Event;
pub use input::{InputCmd, InputPacket, buttons};
pub use snapshot::{EntityState, OwnState, SnapshotHeader, SnapshotReader, SnapshotWriter, ZeroInfo};
pub use types::{Faction, FrameId, Part, PilotKind, WeaponKind};

/// Bumped on any incompatible wire change; the handshake rejects mismatches.
pub const PROTOCOL_VERSION: u16 = 1;

/// Upper bound for every datagram we send. 1200 bytes is the smallest UDP payload QUIC guarantees;
/// the QUIC short header, AEAD tag and HTTP/3 datagram prefix need ~30–40 of those.
pub const MAX_DATAGRAM: usize = 1100;

/// Entity slots on the wire use this many bits.
pub const SLOT_BITS: u32 = 10;
/// Number of addressable entity slots.
pub const MAX_ENTITIES: usize = 1 << SLOT_BITS;
/// "No entity" (e.g. no lock target).
pub const NO_SLOT: u16 = (1 << SLOT_BITS) - 1;

/// Sector-local coordinates span ±this many metres on every axis.
pub const SECTOR_HALF_EXTENT: f32 = 32_768.0;

/// First 4 bits of every datagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketKind {
    Input = 1,
    Snapshot = 2,
}

pub const PACKET_KIND_BITS: u32 = 4;

/// Peeks at a datagram's kind without decoding it.
pub fn packet_kind(bytes: &[u8]) -> Option<PacketKind> {
    match bytes.first()? & 0x0f {
        1 => Some(PacketKind::Input),
        2 => Some(PacketKind::Snapshot),
        _ => None,
    }
}

/// Why a decode failed. Decoders never panic on hostile input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Ran past the end of the buffer.
    Truncated,
    /// Wrong packet kind or message tag.
    WrongKind,
    /// A field held a value outside its domain.
    Invalid,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            DecodeError::Truncated => "truncated packet",
            DecodeError::WrongKind => "wrong packet kind",
            DecodeError::Invalid => "invalid field",
        };
        f.write_str(s)
    }
}
