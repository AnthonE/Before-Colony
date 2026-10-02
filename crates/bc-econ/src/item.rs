//! What can sit in a hangar's stores, be built, and be traded: ores as mined, refined materials,
//! mobile-suit parts for each frame line, and weapons.
//!
//! Ores and materials are bulk goods, counted in kilograms; parts and weapons are counted one by
//! one (each part also has a condition: `crate::stores`). On the wire and in the pilot records an
//! item is its slug (`ore.titanium`, `mat.gundanium`, `part.wingzero.torso`,
//! `weapon.twin_buster_rifle`), so records stay readable and survive reordering.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use bc_proto::{FrameId, Part, WeaponKind};
use bc_sim::content::{ModuleKind, PLAYABLE_ORDER, frame, frame_name, weapon_name};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Ore as it comes out of a rock: the simulation's four cargo kinds, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ore {
    NickelIron,
    Titanium,
    Volatiles,
    Exotics,
}

impl Ore {
    pub const ALL: [Ore; 4] = [Ore::NickelIron, Ore::Titanium, Ore::Volatiles, Ore::Exotics];

    /// The simulation's cargo index (`bc_proto::CARGO_KINDS`).
    pub fn from_cargo(kind: usize) -> Option<Ore> {
        Self::ALL.get(kind).copied()
    }

    pub fn slug(self) -> &'static str {
        match self {
            Ore::NickelIron => "nickel_iron",
            Ore::Titanium => "titanium",
            Ore::Volatiles => "volatiles",
            Ore::Exotics => "exotics",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Ore::NickelIron => "Nickel-iron ore",
            Ore::Titanium => "Titanium ore",
            Ore::Volatiles => "Volatiles",
            Ore::Exotics => "Exotic metals",
        }
    }
}

/// Refined stock, made in the bay's fabricator (gundanium only in the colony's zero-G foundry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    /// Structure: frames, actuators, shells.
    Steel,
    /// Armour plate for everyone who isn't a Gundam.
    TitaniumAlloy,
    /// Reaction mass for the thrusters.
    Propellant,
    /// Sensors, fire control, reactor controls.
    Electronics,
    /// Rounds, missiles and flamethrower fuel.
    Munitions,
    /// The Gundams' armour: it can only be made in zero-G.
    Gundanium,
    /// Machined components (valves, pumps, actuators, bearings): what overhauls and equipment are
    /// built from.
    Components,
}

impl Material {
    pub const ALL: [Material; 7] = [
        Material::Steel,
        Material::TitaniumAlloy,
        Material::Propellant,
        Material::Electronics,
        Material::Munitions,
        Material::Gundanium,
        Material::Components,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Material::Steel => "steel",
            Material::TitaniumAlloy => "ti_alloy",
            Material::Propellant => "propellant",
            Material::Electronics => "electronics",
            Material::Munitions => "munitions",
            Material::Gundanium => "gundanium",
            Material::Components => "components",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Material::Steel => "Steel",
            Material::TitaniumAlloy => "Titanium alloy",
            Material::Propellant => "Propellant",
            Material::Electronics => "Electronics",
            Material::Munitions => "Munitions",
            Material::Gundanium => "Gundanium alloy",
            Material::Components => "Machined components",
        }
    }
}

/// The frame lines a pilot can build: the ones pilots may fly, in the respawn keys' order.
pub const LINES: [FrameId; 6] = PLAYABLE_ORDER;

/// Whether suits of `line` can be built (and their parts fitted).
pub fn is_line(line: FrameId) -> bool {
    LINES.contains(&line)
}

/// A part's slug.
pub fn part_slug(part: Part) -> &'static str {
    match part {
        Part::Head => "head",
        Part::Torso => "torso",
        Part::ArmL => "arm_l",
        Part::ArmR => "arm_r",
        Part::Legs => "legs",
        Part::Backpack => "backpack",
    }
}

/// A part's name.
pub fn part_name(part: Part) -> &'static str {
    match part {
        Part::Head => "head",
        Part::Torso => "torso",
        Part::ArmL => "left arm",
        Part::ArmR => "right arm",
        Part::Legs => "legs",
        Part::Backpack => "backpack",
    }
}

pub fn parse_part(s: &str) -> Option<Part> {
    Part::ALL.into_iter().find(|p| part_slug(*p) == s)
}

/// A weapon's slug.
pub fn weapon_slug(kind: WeaponKind) -> &'static str {
    match kind {
        WeaponKind::BeamRifle => "beam_rifle",
        WeaponKind::MachineCannon => "machine_cannon",
        WeaponKind::BeamSaber => "beam_saber",
        WeaponKind::TwinBusterRifle => "twin_buster_rifle",
        WeaponKind::BeamCannon => "beam_cannon",
        WeaponKind::BeamGatling => "beam_gatling",
        WeaponKind::HomingMissile => "homing_missiles",
        WeaponKind::ArmyKnife => "army_knife",
        WeaponKind::ChestGatling => "chest_gatlings",
        WeaponKind::MicroMissile => "micro_missiles",
        WeaponKind::BusterShield => "buster_shield",
        WeaponKind::HeadVulcan => "head_vulcans",
        WeaponKind::BeamScythe => "beam_scythe",
        WeaponKind::BeamMachineGun => "beam_machine_gun",
        WeaponKind::HeatShotel => "heat_shotels",
        WeaponKind::CrossCrusher => "cross_crusher",
        WeaponKind::DragonFang => "dragon_fang",
        WeaponKind::Flamethrower => "flamethrower",
        WeaponKind::BeamGlaive => "beam_glaive",
        WeaponKind::BeamRifleCharged => "beam_rifle_charged",
    }
}

/// The weapons that are items: every weapon on a buildable line's loadout (a special's own
/// mounts, like Full Open's chest gatlings, come with the suit).
pub fn weapons() -> impl Iterator<Item = WeaponKind> {
    WeaponKind::ALL.into_iter().filter(|w| is_item_weapon(*w))
}

pub fn is_item_weapon(kind: WeaponKind) -> bool {
    LINES.iter().any(|l| frame(*l).loadout.iter().flatten().any(|m| m.weapon == kind))
}

/// Anything a hangar can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Ore(Ore),
    Material(Material),
    /// A part of a suit of this line (a buildable frame).
    Part(FrameId, Part),
    Weapon(WeaponKind),
    /// Equipment fitted to a part (any line's).
    Module(ModuleKind),
}

impl Item {
    /// Every item, in a stable order: ores, materials, each line's parts, weapons.
    pub fn all() -> Vec<Item> {
        let mut v: Vec<Item> = Ore::ALL.into_iter().map(Item::Ore).collect();
        v.extend(Material::ALL.into_iter().map(Item::Material));
        for line in LINES {
            v.extend(Part::ALL.into_iter().map(|p| Item::Part(line, p)));
        }
        v.extend(weapons().map(Item::Weapon));
        v.extend(ModuleKind::ALL.into_iter().map(Item::Module));
        v
    }

    /// A number that orders items as [`Item::all`] does.
    pub fn code(self) -> u16 {
        match self {
            Item::Ore(o) => o as u16,
            Item::Material(m) => 16 + m as u16,
            Item::Part(line, part) => 64 + line as u16 * 8 + part as u16,
            Item::Weapon(w) => 256 + w as u16,
            Item::Module(k) => 512 + k as u16,
        }
    }

    /// Counted in kilograms (ores and materials), not one by one.
    pub fn bulk(self) -> bool {
        matches!(self, Item::Ore(_) | Item::Material(_))
    }

    /// Whether this is a real item (parts of a buildable line, weapons some loadout carries).
    pub fn valid(self) -> bool {
        match self {
            Item::Ore(_) | Item::Material(_) => true,
            Item::Part(line, _) => is_line(line),
            Item::Weapon(w) => is_item_weapon(w),
            Item::Module(_) => true,
        }
    }

    pub fn slug(self) -> String {
        match self {
            Item::Ore(o) => format!("ore.{}", o.slug()),
            Item::Material(m) => format!("mat.{}", m.slug()),
            Item::Part(line, part) => format!("part.{}.{}", line.slug(), part_slug(part)),
            Item::Weapon(w) => format!("weapon.{}", weapon_slug(w)),
            Item::Module(k) => format!("module.{}", k.slug()),
        }
    }

    /// What players read.
    pub fn name(self) -> String {
        match self {
            Item::Ore(o) => o.name().to_string(),
            Item::Material(m) => m.name().to_string(),
            Item::Part(line, part) => format!("{} {}", frame_name(line), part_name(part)),
            Item::Weapon(w) => weapon_name(w).to_string(),
            Item::Module(k) => k.name().to_string(),
        }
    }

    /// A quantity as players read it: "2,400 kg", or "3".
    pub fn amount(self, qty: u64) -> String {
        if self.bulk() { format!("{} kg", thousands(qty)) } else { thousands(qty) }
    }
}

impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        self.code().cmp(&other.code())
    }
}

impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.slug())
    }
}

/// Why a slug isn't an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadItem(pub String);

impl fmt::Display for BadItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no such item: {}", self.0)
    }
}

impl std::error::Error for BadItem {}

impl FromStr for Item {
    type Err = BadItem;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || BadItem(s.to_string());
        let mut it = s.split('.');
        let item = match (it.next(), it.next(), it.next(), it.next()) {
            (Some("ore"), Some(o), None, None) => {
                Item::Ore(Ore::ALL.into_iter().find(|x| x.slug() == o).ok_or_else(bad)?)
            }
            (Some("mat"), Some(m), None, None) => {
                Item::Material(Material::ALL.into_iter().find(|x| x.slug() == m).ok_or_else(bad)?)
            }
            (Some("part"), Some(line), Some(part), None) => {
                let line = LINES.into_iter().find(|l| l.slug() == line).ok_or_else(bad)?;
                Item::Part(line, parse_part(part).ok_or_else(bad)?)
            }
            (Some("weapon"), Some(w), None, None) => {
                Item::Weapon(weapons().find(|x| weapon_slug(*x) == w).ok_or_else(bad)?)
            }
            (Some("module"), Some(k), None, None) => Item::Module(ModuleKind::from_slug(k).ok_or_else(bad)?),
            _ => return Err(bad()),
        };
        Ok(item)
    }
}

impl Serialize for Item {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.slug())
    }
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// `1234567` as `1,234,567`.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Serde for a [`Part`] as its slug.
pub mod part_serde {
    use super::{parse_part, part_slug};
    use bc_proto::Part;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(p: &Part, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(part_slug(*p))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Part, D::Error> {
        let s = String::deserialize(d)?;
        parse_part(&s).ok_or_else(|| serde::de::Error::custom(format!("no such part: {s}")))
    }
}

/// Serde for a [`FrameId`] as its slug (buildable lines only).
pub mod line_serde {
    use super::LINES;
    use bc_proto::FrameId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(f: &FrameId, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(f.slug())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<FrameId, D::Error> {
        let s = String::deserialize(d)?;
        LINES
            .into_iter()
            .find(|l| l.slug() == s)
            .ok_or_else(|| serde::de::Error::custom(format!("no such line: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_round_trips_through_its_slug_in_order() {
        let all = Item::all();
        assert!(all.len() > 40, "{}", all.len());
        for w in all.windows(2) {
            assert!(w[0] < w[1], "{} !< {}", w[0], w[1]);
        }
        for item in all {
            assert!(item.valid());
            assert_eq!(item.slug().parse::<Item>(), Ok(item));
            let json = serde_json::to_string(&item).unwrap();
            assert_eq!(serde_json::from_str::<Item>(&json).unwrap(), item);
        }
        assert_eq!("part.wingzero.torso".parse(), Ok(Item::Part(FrameId::WingZero, Part::Torso)));
        assert!("part.taurus.head".parse::<Item>().is_err(), "a Mobile Doll's line isn't built");
        assert!("weapon.chest_gatlings".parse::<Item>().is_err(), "comes with the suit");
        assert!("ore.titanium.x".parse::<Item>().is_err());
        assert_eq!("module.g_seat".parse(), Ok(Item::Module(ModuleKind::GSeat)));
        assert_eq!("mat.components".parse(), Ok(Item::Material(Material::Components)));
        assert!("".parse::<Item>().is_err());
    }

    #[test]
    fn weapons_are_the_loadouts() {
        let w: Vec<WeaponKind> = weapons().collect();
        assert!(w.contains(&WeaponKind::TwinBusterRifle));
        assert!(w.contains(&WeaponKind::MachineCannon));
        assert!(!w.contains(&WeaponKind::BeamCannon), "the Virgo's");
        assert!(!w.contains(&WeaponKind::CrossCrusher), "Sandrock's special");
    }

    #[test]
    fn numbers_read_with_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(Item::Ore(Ore::Titanium).amount(2_400), "2,400 kg");
    }
}
