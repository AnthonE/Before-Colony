//! Zodiac's aces (`docs/DESIGN.md`, "Aces"; Armored Core VI's Arena, `docs/PEERS.md`): named Mobile
//! Doll units of the Consortium's security arm, each with a bounty on the Charter Board's Most
//! Wanted list. One is out among the Dolls at a time, the next on the list fielded every
//! [`ACE_EVERY`] while none is (`Sim`'s `spawn_ace`). Each flies a Leo with its Doll system tuned
//! by hand, so it stands more than a Doll does and its wreck is worth salvaging; a pilot who downs
//! one takes its bounty as pay, or as the rights to its wreck (the server's to settle).

use bc_proto::FrameId;

use crate::config::secs;

/// An ace on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ace {
    /// Its callsign, as the roster and the board give it.
    pub name: &'static str,
    /// What the colony pays for it, as pay, cr.
    pub bounty: u32,
}

/// The list, in the order they're fielded (and fielded again, round and round).
pub const ACES: [Ace; 9] = [
    Ace { name: "ARIES", bounty: 1_500 },
    Ace { name: "GEMINI", bounty: 1_800 },
    Ace { name: "CANCER", bounty: 2_000 },
    Ace { name: "LIBRA", bounty: 2_200 },
    Ace { name: "SCORPIO", bounty: 2_500 },
    Ace { name: "SAGITTARIUS", bounty: 2_800 },
    Ace { name: "CAPRICORN", bounty: 3_200 },
    Ace { name: "AQUARIUS", bounty: 3_600 },
    Ace { name: "PISCES", bounty: 4_000 },
];

/// [`crate::suits::Suits::ace`] of a suit that's no ace.
pub const NO_ACE: u8 = u8::MAX;
/// An ace is fielded at most this often, ticks, and only while none is out
/// (`SimConfig::ace_every`'s default).
pub const ACE_EVERY: u32 = secs(300.0);
/// What an ace flies...
pub const ACE_FRAME: FrameId = FrameId::Leo;
/// ...and how much more it stands than a Doll in one: its parts' armour, times this.
pub const ACE_ARMOUR: f32 = 1.5;

/// Ace `ace` on the list (round and round).
pub fn ace(ace: u8) -> &'static Ace {
    &ACES[usize::from(ace) % ACES.len()]
}

// An ace's index fits beside a suit slot (`bc_sector`'s shared word) and isn't `NO_ACE`.
const _: () = assert!(ACES.len() < NO_ACE as usize);
