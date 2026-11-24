//! The looks a pilot puts on their suit at the bay's maintenance console (`docs/ROADMAP.md`,
//! "Liveries"): its body, trim and accent paints, its sensors' glow, and a decal on its right
//! shoulder. Most are bought with credits; the rest come with a cosmetic their wallet holds on Elo
//! Pros (the store's item keys below), read as they sign in.
//!
//! Cosmetic only, and nothing else: no number here reaches `tuning`, the tick or the wire's
//! snapshots (the roster carries what a suit wears). The store sells appearance, never an edge
//! (Elo Pros' no-pay-to-win rule).

use bc_proto::control::Livery;
use bc_proto::{Faction, FrameId};

/// The hull palette's entries a livery can name (`bc_model::paint`; a hull tag has four bits for
/// each of its paints), by index.
pub mod paint {
    pub const WHITE: u8 = 0;
    pub const BLUE: u8 = 1;
    pub const RED: u8 = 2;
    pub const YELLOW: u8 = 3;
    pub const DARK: u8 = 4;
    pub const ALMAGEST_GREEN: u8 = 5;
    pub const ALMAGEST_GREY: u8 = 6;
    pub const CORVUS_WHITE: u8 = 7;
    pub const CORVUS_BLUE: u8 = 8;
    pub const CETUS_OLIVE: u8 = 9;
    pub const ALLIANCE_TAN: u8 = 10;
    pub const HULL: u8 = 11;
    pub const HULL_DARK: u8 = 12;
    pub const MIRROR: u8 = 13;
    pub const GUNMETAL: u8 = 14;
    pub const GLASS: u8 = 15;
}

/// The sensors' glows (the hull shader's order): green, pink, amber, cyan.
pub mod eye {
    pub const GREEN: u8 = 0;
    pub const PINK: u8 = 1;
    pub const AMBER: u8 = 2;
    pub const CYAN: u8 = 3;
}

/// A cosmetic a wallet can hold on Elo Pros (in Before Colony's collection there), by the bit it
/// has in a [`Owned`] set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Cosmetic {
    /// The Pill, on the left shoulder: free for ten minutes of play (the roster's `PILL` bit).
    Pill = 0,
    /// The Coalsack, the dark nebula by the Southern Cross: black glass and gunmetal paints.
    Coalsack = 1,
    /// The Pleiades, the stars in their reflection nebula: a mirror finish.
    Pleiades = 2,
    /// Albireo, the double star, gold and blue: amber and cyan sensors.
    Albireo = 3,
    /// The Lodestar decal: a five-pointed star in a ring.
    LodestarDecal = 4,
    /// The Crux decal: the Southern Cross's four stars and its fifth.
    CruxDecal = 5,
}

impl Cosmetic {
    pub const ALL: [Cosmetic; 6] = [
        Cosmetic::Pill,
        Cosmetic::Coalsack,
        Cosmetic::Pleiades,
        Cosmetic::Albireo,
        Cosmetic::LodestarDecal,
        Cosmetic::CruxDecal,
    ];

    /// Its key in the store's record of the title's items (`/api/items/sol/of/<wallet>`'s `item`).
    pub const fn key(self) -> &'static str {
        match self {
            Cosmetic::Pill => "pill",
            Cosmetic::Coalsack => "coalsack",
            Cosmetic::Pleiades => "pleiades",
            Cosmetic::Albireo => "albireo",
            Cosmetic::LodestarDecal => "lodestar-decal",
            Cosmetic::CruxDecal => "crux-decal",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Cosmetic::Pill => "The Pill",
            Cosmetic::Coalsack => "Coalsack",
            Cosmetic::Pleiades => "Pleiades",
            Cosmetic::Albireo => "Albireo",
            Cosmetic::LodestarDecal => "Lodestar decal",
            Cosmetic::CruxDecal => "Crux decal",
        }
    }

    /// What it puts on a suit, in a line.
    pub const fn summary(self) -> &'static str {
        match self {
            Cosmetic::Pill => "a pill, green and white, on the left shoulder",
            Cosmetic::Coalsack => "black glass and gunmetal paints",
            Cosmetic::Pleiades => "a mirror finish",
            Cosmetic::Albireo => "amber and cyan sensors",
            Cosmetic::LodestarDecal => "a star in a ring, on the right shoulder",
            Cosmetic::CruxDecal => "the Southern Cross, on the right shoulder",
        }
    }

    pub fn from_key(key: &str) -> Option<Cosmetic> {
        Cosmetic::ALL.into_iter().find(|c| c.key() == key)
    }

    pub const fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// The cosmetics a wallet holds, a bit each ([`Cosmetic::bit`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Owned(pub u32);

impl Owned {
    pub const NONE: Owned = Owned(0);

    pub const fn has(self, c: Cosmetic) -> bool {
        self.0 & c.bit() != 0
    }

    pub const fn with(self, c: Cosmetic) -> Owned {
        Owned(self.0 | c.bit())
    }

    /// Whether this unlocks what `needs` (none: anyone may).
    pub const fn allows(self, needs: Option<Cosmetic>) -> bool {
        match needs {
            Some(c) => self.has(c),
            None => true,
        }
    }
}

/// A paint a livery can be chosen from: its index in the hull palette, its slug and name, and the
/// cosmetic that unlocks it (none: credits alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    pub paint: u8,
    pub slug: &'static str,
    pub name: &'static str,
    pub needs: Option<Cosmetic>,
}

const fn swatch(paint: u8, slug: &'static str, name: &'static str, needs: Option<Cosmetic>) -> Swatch {
    Swatch { paint, slug, name, needs }
}

/// Every paint, by its index (`PAINTS[k].paint == k`).
pub const PAINTS: [Swatch; 16] = [
    swatch(paint::WHITE, "white", "White", None),
    swatch(paint::BLUE, "blue", "Blue", None),
    swatch(paint::RED, "red", "Red", None),
    swatch(paint::YELLOW, "yellow", "Yellow", None),
    swatch(paint::DARK, "black", "Black", None),
    swatch(paint::ALMAGEST_GREEN, "field_green", "Field green", None),
    swatch(paint::ALMAGEST_GREY, "grey", "Grey", None),
    swatch(paint::CORVUS_WHITE, "pale_grey", "Pale grey", None),
    swatch(paint::CORVUS_BLUE, "slate_blue", "Slate blue", None),
    swatch(paint::CETUS_OLIVE, "olive", "Olive", None),
    swatch(paint::ALLIANCE_TAN, "tan", "Tan", None),
    swatch(paint::HULL, "hull_grey", "Hull grey", None),
    swatch(paint::HULL_DARK, "service_grey", "Service grey", None),
    swatch(paint::MIRROR, "mirror", "Mirror", Some(Cosmetic::Pleiades)),
    swatch(paint::GUNMETAL, "gunmetal", "Gunmetal", Some(Cosmetic::Coalsack)),
    swatch(paint::GLASS, "black_glass", "Black glass", Some(Cosmetic::Coalsack)),
];

/// Every sensor glow, by its index.
pub const EYES: [Swatch; 4] = [
    swatch(eye::GREEN, "green", "Green", None),
    swatch(eye::PINK, "pink", "Pink", None),
    swatch(eye::AMBER, "amber", "Amber", Some(Cosmetic::Albireo)),
    swatch(eye::CYAN, "cyan", "Cyan", Some(Cosmetic::Albireo)),
];

/// A decal for the right shoulder: its id (on the roster; 0 is none), slug and name, and what
/// unlocks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decal {
    pub id: u8,
    pub slug: &'static str,
    pub name: &'static str,
    pub needs: Option<Cosmetic>,
}

/// Every decal, by id less one (`DECALS[k].id == k + 1`).
pub const DECALS: [Decal; 3] = [
    Decal { id: 1, slug: "ring", name: "The Ring", needs: None },
    Decal { id: 2, slug: "lodestar", name: "Lodestar", needs: Some(Cosmetic::LodestarDecal) },
    Decal { id: 3, slug: "crux", name: "Crux", needs: Some(Cosmetic::CruxDecal) },
];

/// Decal `id`, if there's one (0 is none).
pub fn decal(id: u8) -> Option<&'static Decal> {
    DECALS.get(usize::from(id).wrapping_sub(1))
}

/// Body, trim and accent paint and eye glow for a frame in a faction's livery: how a suit nobody
/// has painted looks.
pub const fn default_livery(frame: FrameId, faction: Faction) -> Livery {
    use paint::*;
    let (b, t, a, e) = match (frame, faction) {
        (FrameId::Altair | FrameId::Aquila, _) => (WHITE, BLUE, RED, eye::GREEN),
        (FrameId::Antares, _) => (WHITE, RED, DARK, eye::GREEN),
        (FrameId::Algol, _) => (DARK, BLUE, RED, eye::GREEN),
        (FrameId::Canopus, _) => (ALLIANCE_TAN, WHITE, RED, eye::GREEN),
        (FrameId::Thuban, _) => (WHITE, ALMAGEST_GREEN, YELLOW, eye::GREEN),
        (FrameId::Corvus, _) => (CORVUS_WHITE, CORVUS_BLUE, RED, eye::PINK),
        (FrameId::Cetus, _) => (CETUS_OLIVE, ALMAGEST_GREY, DARK, eye::PINK),
        // The prototype pair, in the colours they were rolled out in: blue and red, grey vents.
        (FrameId::Sagittarius, _) => (BLUE, HULL, DARK, eye::PINK),
        (FrameId::Corona, _) => (RED, HULL, DARK, eye::PINK),
        (FrameId::Argo, Faction::Almagest) => (ALMAGEST_GREEN, ALMAGEST_GREY, DARK, eye::PINK),
        (FrameId::Argo, Faction::Colonies) => (CORVUS_BLUE, WHITE, RED, eye::GREEN),
        (FrameId::Argo, Faction::Alliance) => (ALLIANCE_TAN, DARK, RED, eye::PINK),
    };
    Livery::new(b, t, a, e)
}

/// What unlocks paint `k`, eye `k` (none: credits alone, or no such thing).
fn paint_needs(k: u8) -> Option<Cosmetic> {
    PAINTS.get(usize::from(k)).and_then(|s| s.needs)
}

fn eye_needs(k: u8) -> Option<Cosmetic> {
    EYES.get(usize::from(k)).and_then(|s| s.needs)
}

/// The cosmetic `livery` needs that `owned` lacks, if there's one: the first, body to eye.
pub fn livery_lacks(livery: Livery, owned: Owned) -> Option<Cosmetic> {
    [paint_needs(livery.body), paint_needs(livery.trim), paint_needs(livery.accent), eye_needs(livery.eye)]
        .into_iter()
        .flatten()
        .find(|c| !owned.has(*c))
}

/// What a suit painted `livery` (none: never painted) with `decal` wears for everyone to see, its
/// pilot holding `owned`: each paint or glow it no longer holds the cosmetic for is the frame's
/// own again (`fallback`, its default livery), and so is the decal (none). The pilot's choice is
/// kept; this is only what's shown.
pub fn worn(livery: Option<Livery>, decal_id: u8, fallback: Livery, owned: Owned) -> bc_proto::control::Look {
    let livery = livery.map(|l| {
        let keep = |k: u8, theirs: u8| if owned.allows(paint_needs(k)) { k } else { theirs };
        Livery::new(
            keep(l.body, fallback.body),
            keep(l.trim, fallback.trim),
            keep(l.accent, fallback.accent),
            if owned.allows(eye_needs(l.eye)) { l.eye } else { fallback.eye },
        )
    });
    let decal_id = match decal(decal_id) {
        Some(d) if owned.allows(d.needs) => d.id,
        _ => 0,
    };
    bc_proto::control::Look { livery: livery.filter(|l| *l != fallback), decal: decal_id }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_in_their_order_and_the_keys_are_the_stores() {
        for (k, s) in PAINTS.iter().enumerate() {
            assert_eq!(usize::from(s.paint), k, "{s:?}");
        }
        for (k, s) in EYES.iter().enumerate() {
            assert_eq!(usize::from(s.paint), k, "{s:?}");
        }
        for (k, d) in DECALS.iter().enumerate() {
            assert_eq!(usize::from(d.id), k + 1, "{d:?}");
            assert_eq!(decal(d.id), Some(d));
        }
        assert_eq!(decal(0), None);
        assert_eq!(decal(DECALS.len() as u8 + 1), None);
        for (k, c) in Cosmetic::ALL.into_iter().enumerate() {
            assert_eq!(c as usize, k);
            assert_eq!(Cosmetic::from_key(c.key()), Some(c));
            // Elo Pros' item ids: `[a-z0-9][a-z0-9_-]{0,39}`.
            let key = c.key().as_bytes();
            assert!(!key.is_empty() && key.len() <= 40, "{c:?}");
            assert!(key[0].is_ascii_lowercase() || key[0].is_ascii_digit(), "{c:?}");
            assert!(
                key.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
            );
        }
        assert_eq!(Cosmetic::from_key("pill"), Some(Cosmetic::Pill));
        assert_eq!(Cosmetic::from_key("nothing"), None);
        // Every premium paint, glow and decal is unlocked by something sold, and the Pill is the
        // left shoulder's alone (the roster's own bit).
        let needs =
            PAINTS.iter().chain(&EYES).filter_map(|s| s.needs).chain(DECALS.iter().filter_map(|d| d.needs));
        for c in needs {
            assert_ne!(c, Cosmetic::Pill);
        }
    }

    #[test]
    fn a_suit_shows_only_what_its_pilot_holds() {
        let fallback = default_livery(FrameId::Argo, Faction::Colonies);
        let black_glass = Livery::new(paint::GLASS, paint::YELLOW, paint::GUNMETAL, eye::CYAN);
        assert_eq!(livery_lacks(black_glass, Owned::NONE), Some(Cosmetic::Coalsack));
        assert_eq!(livery_lacks(black_glass, Owned::NONE.with(Cosmetic::Coalsack)), Some(Cosmetic::Albireo));
        let both = Owned::NONE.with(Cosmetic::Coalsack).with(Cosmetic::Albireo);
        assert_eq!(livery_lacks(black_glass, both), None);
        assert_eq!(worn(Some(black_glass), 2, fallback, both).livery, Some(black_glass));
        // Without them, those channels are the frame's own again; the rest stays as painted.
        let shown = worn(Some(black_glass), 2, fallback, Owned::NONE);
        assert_eq!(
            shown.livery,
            Some(Livery::new(fallback.body, paint::YELLOW, fallback.accent, fallback.eye))
        );
        assert_eq!(shown.decal, 0, "the Lodestar decal needs its cosmetic");
        assert_eq!(worn(None, 1, fallback, Owned::NONE).decal, 1, "the Ring is anyone's");
        assert_eq!(worn(None, 9, fallback, Owned(u32::MAX)).decal, 0, "no such decal");
        // Never painted, or painted back to the frame's own: nothing to send.
        assert_eq!(worn(None, 0, fallback, both), bc_proto::control::Look::default());
        assert_eq!(worn(Some(fallback), 0, fallback, both).livery, None);
        // A paint that falls back all the way is the frame's own too.
        let premium = Livery::new(paint::MIRROR, fallback.trim, fallback.accent, fallback.eye);
        assert_eq!(worn(Some(premium), 0, fallback, Owned::NONE).livery, None);
    }

    #[test]
    fn every_frame_has_a_livery_in_every_side() {
        for f in FrameId::ALL {
            for side in [Faction::Colonies, Faction::Alliance, Faction::Almagest] {
                let l = default_livery(f, side);
                // The frames' own liveries are anyone's: nothing to unlock.
                assert_eq!(livery_lacks(l, Owned::NONE), None, "{f:?} {side:?}");
            }
        }
    }
}
