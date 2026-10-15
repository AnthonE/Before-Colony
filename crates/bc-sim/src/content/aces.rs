//! The Almagest's aces (`docs/DESIGN.md`, "Aces"; Armored Core VI's Arena, `docs/PEERS.md`): named
//! drone units of the Consortium's security arm, each with a bounty on the Charter Board's Most
//! Wanted list. One is out among the drones at a time, the next on the list fielded every
//! [`ACE_EVERY`] while none is (`Sim`'s `spawn_ace`). Each flies a suit with its drone system tuned
//! by hand, so it stands more than a drone does and its wreck is worth salvaging; a pilot who downs
//! one takes its bounty as pay, or as the rights to its wreck (the server's to settle). Most fly
//! the Argo; the last two on the list fly the Almagest's prototype pair, the Sagittarius and the
//! Corona, each with the other half of the pair at its side as a drone.

use bc_proto::FrameId;

use crate::config::secs;

/// An ace on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ace {
    /// Its callsign, as the roster and the board give it.
    pub name: &'static str,
    /// What the colony pays for it, as pay, cr.
    pub bounty: u32,
    /// What it flies.
    pub frame: FrameId,
    /// The drone fielded at its side in its squad, if any (a plain drone, no ace).
    pub wing: Option<FrameId>,
}

const fn argo(name: &'static str, bounty: u32) -> Ace {
    Ace { name, bounty, frame: FrameId::Argo, wing: None }
}

/// The list, in the order they're fielded (and fielded again, round and round).
pub const ACES: [Ace; 9] = [
    argo("LYRA", 1_500),
    argo("SAGITTA", 1_800),
    argo("DELPHINUS", 2_000),
    argo("CYGNUS", 2_200),
    argo("AURIGA", 2_500),
    argo("PERSEUS", 2_800),
    argo("CENTAURUS", 3_200),
    // The strongman holds up the sky: the Corona's field, a Sagittarius firing from inside it.
    Ace { name: "HERCULES", bounty: 3_600, frame: FrameId::Corona, wing: Some(FrameId::Sagittarius) },
    // The hunter: the Sagittarius's cannon, a Corona keeping the field round it.
    Ace { name: "ORION", bounty: 4_000, frame: FrameId::Sagittarius, wing: Some(FrameId::Corona) },
];

/// [`crate::suits::Suits::ace`] of a suit that's no ace.
pub const NO_ACE: u8 = u8::MAX;
/// An ace is fielded at most this often, ticks, and only while none is out
/// (`SimConfig::ace_every`'s default).
pub const ACE_EVERY: u32 = secs(300.0);
/// How much more an ace stands than a drone in its suit: its parts' armour, times this.
pub const ACE_ARMOUR: f32 = 1.5;
/// How far from its ace a wingman is fielded, m: inside a Corona's field
/// (`content::WARD_RADIUS`).
pub const WING_OFFSET: f32 = 18.0;

/// Ace `ace` on the list (round and round).
pub fn ace(ace: u8) -> &'static Ace {
    &ACES[usize::from(ace) % ACES.len()]
}

// An ace's index fits beside a suit slot (`bc_sector`'s shared word) and isn't `NO_ACE`.
const _: () = assert!(ACES.len() < NO_ACE as usize);
// A wingman starts inside the field.
const _: () = assert!(WING_OFFSET < crate::content::WARD_RADIUS);
