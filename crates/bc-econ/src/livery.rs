//! A suit's paint (`docs/ROADMAP.md`, "Liveries"): body, trim and accent paints, its sensors'
//! glow and a decal on its right shoulder, chosen at the bay's maintenance console from
//! `bc_sim::content::cosmetics`' swatches and paid for in credits (a sink). Some swatches and decals
//! come only with a cosmetic the pilot's wallet holds on Elo Pros ([`Hangar::cosmetics`], the
//! server's read). The paint is the suit's: it stays through strip and refit, and goes with the
//! torso (a new suit is the frame's own again).
//!
//! Cosmetic only: nothing here reaches the stat sheet or the simulation.

use bc_proto::control::Livery;
use bc_sim::content::cosmetics::{self, Cosmetic, EYES, Owned, PAINTS, decal, livery_lacks};
use serde::{Deserialize, Serialize};

use crate::hangar::{Bay, Done, Hangar};

/// What a suit's paint costs, credits, for each thing changed: the body, the trim, the accent, the
/// sensors' glow, the decal (taking one off is free).
pub const PAINT_CR: Costs = Costs { body: 400, trim: 200, accent: 150, eye: 100, decal: 250 };

/// What painting each part of a livery costs, credits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Costs {
    pub body: u64,
    pub trim: u64,
    pub accent: u64,
    pub eye: u64,
    pub decal: u64,
}

/// A livery and decal as a pilot chose them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paint {
    #[serde(with = "paint_slug")]
    pub body: u8,
    #[serde(with = "paint_slug")]
    pub trim: u8,
    #[serde(with = "paint_slug")]
    pub accent: u8,
    #[serde(with = "eye_slug")]
    pub eye: u8,
    /// `cosmetics::DECALS`' id (0: none).
    #[serde(default, skip_serializing_if = "is_none", with = "decal_slug")]
    pub decal: u8,
}

fn is_none(d: &u8) -> bool {
    *d == 0
}

impl Paint {
    pub const fn of(l: Livery, decal: u8) -> Self {
        Self { body: l.body, trim: l.trim, accent: l.accent, eye: l.eye, decal }
    }

    pub const fn livery(&self) -> Livery {
        Livery::new(self.body, self.trim, self.accent, self.eye)
    }

    /// What going from `self` to `to` costs ([`PAINT_CR`]).
    pub fn cost_to(&self, to: &Paint) -> u64 {
        let c = PAINT_CR;
        let moved = |a: u8, b: u8, cr: u64| if a == b { 0 } else { cr };
        moved(self.body, to.body, c.body)
            + moved(self.trim, to.trim, c.trim)
            + moved(self.accent, to.accent, c.accent)
            + moved(self.eye, to.eye, c.eye)
            + if to.decal != 0 && to.decal != self.decal { c.decal } else { 0 }
    }
}

macro_rules! slug_serde {
    ($m:ident, $table:expr, $what:literal) => {
        mod $m {
            use serde::{Deserialize, Deserializer, Serializer};

            pub fn serialize<S: Serializer>(k: &u8, s: S) -> Result<S::Ok, S::Error> {
                match $table.get(usize::from(*k)) {
                    Some(sw) => s.serialize_str(sw.slug),
                    None => Err(serde::ser::Error::custom(concat!("no such ", $what))),
                }
            }

            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
                let slug = String::deserialize(d)?;
                $table.iter().find(|sw| sw.slug == slug).map(|sw| sw.paint).ok_or_else(|| {
                    serde::de::Error::custom(format!(concat!("no such ", $what, ": {}"), slug))
                })
            }
        }
    };
}

slug_serde!(paint_slug, bc_sim::content::cosmetics::PAINTS, "paint");
slug_serde!(eye_slug, bc_sim::content::cosmetics::EYES, "sensor glow");

mod decal_slug {
    use bc_sim::content::cosmetics::{DECALS, decal};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &u8, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(decal(*id).map_or("none", |d| d.slug))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
        let slug = String::deserialize(d)?;
        if slug == "none" {
            return Ok(0);
        }
        DECALS
            .iter()
            .find(|d| d.slug == slug)
            .map(|d| d.id)
            .ok_or_else(|| serde::de::Error::custom(format!("no such decal: {slug}")))
    }
}

/// Why `c` stops a pilot who lacks it.
fn lacks(what: &str, c: Cosmetic) -> String {
    format!("{what} comes with {}, from Elo Pros ({})", c.name(), c.summary())
}

impl Hangar {
    /// The cosmetics the pilot's wallet holds, as last read.
    pub fn owned(&self) -> Owned {
        Owned(self.cosmetics)
    }

    /// The suit in the bay's paint as it stands (its frame's own livery, `default`, never painted).
    pub fn paint_of(&self, default: Livery) -> Option<Paint> {
        match &self.bay {
            Bay::Docked { suit } | Bay::Out { suit } => Some(suit.paint.unwrap_or(Paint::of(default, 0))),
            Bay::Empty => None,
        }
    }

    /// What the suit in the bay (or out flying) wears for everyone to see, flown for `side`: its
    /// paint, but what the pilot's wallet no longer holds the cosmetic for is its frame's own
    /// (`cosmetics::worn`). None painted: nothing beyond the frame's livery.
    pub fn look(&self, side: bc_proto::Faction) -> bc_proto::control::Look {
        match &self.bay {
            Bay::Docked { suit } | Bay::Out { suit } => {
                let default = cosmetics::default_livery(suit.line, side);
                let p = suit.paint;
                cosmetics::worn(p.map(|p| p.livery()), p.map_or(0, |p| p.decal), default, self.owned())
            }
            Bay::Empty => bc_proto::control::Look::default(),
        }
    }

    /// Paints the suit standing in the bay `want`, its frame's own livery being `default`: for
    /// the credits each change costs ([`PAINT_CR`]), and only in what the pilot's wallet holds the
    /// cosmetic for. Painted back to the frame's own with no decal, it's unpainted again.
    pub fn paint(&mut self, want: Paint, default: Livery) -> Done {
        let owned = self.owned();
        let credits = self.credits;
        let suit = match &mut self.bay {
            Bay::Docked { suit } => suit,
            Bay::Out { .. } => return Err("your suit is out in the sector: paint it in the bay".into()),
            Bay::Empty => return Err("there's no suit in the bay to paint".into()),
        };
        let l = want.livery();
        if Paint::of(l, want.decal) != want {
            return Err("there's no such paint".into());
        }
        if let Some(c) = livery_lacks(l, owned) {
            let what = [want.body, want.trim, want.accent]
                .into_iter()
                .filter_map(|k| PAINTS.get(usize::from(k)))
                .find(|s| s.needs == Some(c))
                .map(|s| s.name)
                .or_else(|| EYES.get(usize::from(want.eye)).filter(|s| s.needs == Some(c)).map(|s| s.name))
                .unwrap_or("that paint");
            return Err(lacks(&what.to_lowercase(), c));
        }
        if want.decal != 0 {
            let Some(d) = decal(want.decal) else { return Err("there's no such decal".into()) };
            if let Some(c) = d.needs.filter(|c| !owned.has(*c)) {
                return Err(lacks(&format!("the {} decal", d.name), c));
            }
        }
        let was = suit.paint.unwrap_or(Paint::of(default, 0));
        if was == want {
            return Err("that's how it's painted already".into());
        }
        let cost = was.cost_to(&want);
        if cost > credits {
            return Err(format!("that paint costs {cost} CR, and you have {credits} CR"));
        }
        suit.paint = (want != Paint::of(default, 0)).then_some(want);
        self.credits -= cost;
        let mut said = Vec::new();
        let name = |k: u8| PAINTS.get(usize::from(k)).map_or("?", |s| s.name).to_uppercase();
        for (what, a, b) in [
            ("BODY", was.body, want.body),
            ("TRIM", was.trim, want.trim),
            ("ACCENT", was.accent, want.accent),
        ] {
            if a != b {
                said.push(format!("{what} {}", name(b)));
            }
        }
        if was.eye != want.eye {
            let glow = EYES.get(usize::from(want.eye)).map_or("?", |s| s.name).to_uppercase();
            said.push(format!("SENSORS {glow}"));
        }
        if was.decal != want.decal {
            said.push(match cosmetics::decal(want.decal) {
                Some(d) => format!("DECAL {}", d.name.to_uppercase()),
                None => "DECAL OFF".into(),
            });
        }
        let paid = if cost > 0 { format!(" · {cost} CR") } else { String::new() };
        Ok(format!("PAINTED {}{paid}", said.join(" · ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::suit::{Slot, Suit};
    use bc_proto::{Faction, FrameId, Part};
    use bc_sim::content::cosmetics::{default_livery, eye, paint};

    fn argo() -> Livery {
        default_livery(FrameId::Argo, Faction::Colonies)
    }

    fn hangar() -> Hangar {
        Hangar {
            credits: 5_000,
            bay: Bay::Docked { suit: Suit::complete(FrameId::Argo) },
            ..Hangar::default()
        }
    }

    fn painted(h: &Hangar) -> Option<Paint> {
        h.suit().and_then(|s| s.paint)
    }

    #[test]
    fn paint_costs_credits_for_what_changes() {
        let mut h = hangar();
        let red = Paint { body: paint::RED, ..Paint::of(argo(), 0) };
        assert_eq!(h.paint(red, argo()), Ok("PAINTED BODY RED · 400 CR".into()));
        assert_eq!((h.credits, painted(&h)), (4_600, Some(red)));
        assert_eq!(h.paint(red, argo()), Err("that's how it's painted already".into()));
        // The trim and a decal: 200 and 250.
        let ringed = Paint { trim: paint::YELLOW, decal: 1, ..red };
        assert!(h.paint(ringed, argo()).is_ok());
        assert_eq!(h.credits, 4_600 - 450);
        // Taking the decal off is free; back to the frame's own is unpainted.
        assert!(h.paint(Paint { decal: 0, ..ringed }, argo()).is_ok());
        assert_eq!(h.credits, 4_150);
        assert!(h.paint(Paint::of(argo(), 0), argo()).is_ok());
        assert_eq!((h.credits, painted(&h)), (4_150 - 600, None));
        // Not enough credits: nothing changes.
        h.credits = 100;
        assert_eq!(h.paint(red, argo()), Err("that paint costs 400 CR, and you have 100 CR".into()));
        assert_eq!((h.credits, painted(&h)), (100, None));
    }

    #[test]
    fn premium_paint_needs_the_cosmetic_the_wallet_holds() {
        let mut h = hangar();
        let glass = Paint { body: paint::GLASS, ..Paint::of(argo(), 0) };
        let why = h.paint(glass, argo()).unwrap_err();
        assert!(why.contains("Coalsack") && why.contains("black glass"), "{why}");
        let cyan = Paint { eye: eye::CYAN, ..Paint::of(argo(), 0) };
        assert!(h.paint(cyan, argo()).unwrap_err().contains("Albireo"));
        let star = Paint { decal: 2, ..Paint::of(argo(), 0) };
        assert!(h.paint(star, argo()).unwrap_err().contains("Lodestar decal"));
        assert_eq!(h.credits, 5_000);
        h.cosmetics = Owned::NONE.with(Cosmetic::Coalsack).with(Cosmetic::LodestarDecal).0;
        assert!(h.paint(glass, argo()).is_ok());
        assert!(h.paint(Paint { decal: 2, ..glass }, argo()).is_ok());
        assert!(h.paint(Paint { eye: eye::CYAN, ..glass }, argo()).is_err(), "no Albireo");
        assert_eq!(h.credits, 5_000 - 400 - 250);
        // Everyone sees it while the wallet holds them; the paint is kept when it doesn't.
        let look = h.look(Faction::Colonies);
        assert_eq!((look.livery.map(|l| l.body), look.decal), (Some(paint::GLASS), 2));
        h.cosmetics = 0;
        assert_eq!(h.look(Faction::Colonies), bc_proto::control::Look::default());
        assert_eq!(painted(&h), Some(Paint { decal: 2, ..glass }));
    }

    #[test]
    fn paint_stays_through_strip_and_refit_and_goes_with_the_torso() {
        let mut h = hangar();
        let p = Paint { body: paint::YELLOW, trim: paint::DARK, decal: 1, ..Paint::of(argo(), 0) };
        h.paint(p, argo()).unwrap();
        h.strip(Slot::Part { part: Part::ArmL }).unwrap();
        h.fit(Item::Part(FrameId::Argo, Part::ArmL)).unwrap();
        h.strip(Slot::Mount { mount: 0 }).unwrap();
        h.fit(Item::Weapon(bc_proto::WeaponKind::BeamRifle)).unwrap();
        assert_eq!(painted(&h), Some(p));
        // Launched and home, it's still painted.
        let out = h.launch().unwrap();
        let l = out;
        let home = bc_sim::sim::Homecoming {
            frame: FrameId::Argo,
            parts: l.parts,
            mounts: l.mounts,
            ammo: l.ammo,
            propellant: l.propellant,
            grade: l.grade,
            systems: l.systems,
            modules: l.modules,
            kits: l.kits,
            usage: Default::default(),
            cargo_kg: [0; 4],
            held: None,
            bounty: 0,
        };
        h.came_home(&home);
        assert_eq!(painted(&h), Some(p));
        // Dismantled and built again on the torso: a new suit, the frame's own.
        h.dismantle().unwrap();
        h.fit(Item::Part(FrameId::Argo, Part::Torso)).unwrap();
        assert_eq!(painted(&h), None);
    }

    #[test]
    fn a_suit_out_or_none_is_not_painted_and_paint_reads_back() {
        let mut h = Hangar { credits: 5_000, ..Hangar::default() };
        assert!(h.paint(Paint::of(argo(), 1), argo()).is_err());
        h.bay = Bay::Out { suit: Suit::complete(FrameId::Argo) };
        assert!(h.paint(Paint::of(argo(), 1), argo()).unwrap_err().contains("out in the sector"));
        // As slugs, and an old suit (no paint) reads as never painted.
        let p =
            Paint { body: paint::MIRROR, trim: paint::GLASS, accent: paint::RED, eye: eye::AMBER, decal: 3 };
        let json = serde_json::to_value(p).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"body": "mirror", "trim": "black_glass", "accent": "red", "eye": "amber", "decal": "crux"})
        );
        assert_eq!(serde_json::from_value::<Paint>(json).unwrap(), p);
        let plain = serde_json::to_value(Paint::of(argo(), 0)).unwrap();
        assert!(plain.get("decal").is_none(), "{plain}");
        assert!(
            serde_json::from_value::<Paint>(
                serde_json::json!({"body": "pink", "trim": "red", "accent": "red", "eye": "green"})
            )
            .is_err()
        );
        let old = serde_json::to_value(Suit::complete(FrameId::Argo)).unwrap();
        assert!(old.get("paint").is_none(), "{old}");
        assert_eq!(serde_json::from_value::<Suit>(old).unwrap().paint, None);
    }
}
