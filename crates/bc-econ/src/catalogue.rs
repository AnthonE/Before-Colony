//! What everything is made of, how long it takes, and what the colony thinks it's worth.
//!
//! - **Refining** (the bay's fabricator): ore into steel, titanium alloy, propellant, electronics
//!   and munitions.
//! - **Gundanium** can only be made in zero-G: the colony's foundry at the docking hub makes it
//!   for a fee, slowly.
//! - **Parts** of a suit weigh what the simulation says they weigh (`part_mass_kg`), and are made
//!   of structure (steel), armour (titanium alloy, or gundanium for a Gundam) and electronics; a
//!   Gundam's torso carries its reactor and its system (the ZERO System, the Hyper Jammer).
//! - **Weapons** each have a recipe; the ones that fire rounds need munitions to load.
//! - **Machined components** (steel, a little electronics and titanium alloy) are what overhauls
//!   restore a suit's systems with, and what equipment **modules** are mostly made of.
//!
//! A Gundam costs about six Leos, most of it exotics and foundry time: getting one is the game.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use bc_proto::{FrameId, Part, WeaponKind};
use bc_sim::content::salvage::{is_gundam, part_mass_kg};
use bc_sim::content::{ModuleKind, frame, weapon};
use serde::{Deserialize, Serialize};

use crate::item::{Item, LINES, Material, Ore, weapons};

/// Where a recipe is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Station {
    /// The bay's own fabricator.
    Fabricator,
    /// The colony's zero-G foundry at the docking hub (a fee a batch).
    Foundry,
}

impl Station {
    pub const ALL: [Station; 2] = [Station::Fabricator, Station::Foundry];

    pub fn name(self) -> &'static str {
        match self {
            Station::Fabricator => "Fabricator",
            Station::Foundry => "Zero-G foundry",
        }
    }
}

/// One way to make something: a batch takes `inputs` (and `fee` credits) and `secs`, and gives
/// `output`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub output: Item,
    /// How much a batch makes (kg for bulk goods, else a count).
    pub makes: u64,
    pub inputs: Vec<(Item, u64)>,
    pub station: Station,
    pub secs: u32,
    pub fee: u64,
}

/// Every recipe, one per thing that can be made.
pub fn recipes() -> &'static [Recipe] {
    static ALL: OnceLock<Vec<Recipe>> = OnceLock::new();
    ALL.get_or_init(build_recipes)
}

/// The recipe that makes `item`, if it can be made.
pub fn recipe(item: Item) -> Option<&'static Recipe> {
    recipes().iter().find(|r| r.output == item)
}

const fn ore(o: Ore) -> Item {
    Item::Ore(o)
}

const fn mat(m: Material) -> Item {
    Item::Material(m)
}

const STEEL: Item = mat(Material::Steel);
const TI_ALLOY: Item = mat(Material::TitaniumAlloy);
pub(crate) const ELECTRONICS: Item = mat(Material::Electronics);
const GUNDANIUM: Item = mat(Material::Gundanium);
const MUNITIONS: Item = mat(Material::Munitions);
pub(crate) const EXOTICS: Item = ore(Ore::Exotics);
pub(crate) const COMPONENTS: Item = mat(Material::Components);

/// Rounded to the nearest 10 kg (and at least 10).
fn tens(kg: f32) -> u64 {
    ((kg / 10.0).round() as u64).max(1) * 10
}

fn refine(output: Material, makes: u64, inputs: &[(Item, u64)], secs: u32) -> Recipe {
    Recipe { output: mat(output), makes, inputs: inputs.to_vec(), station: Station::Fabricator, secs, fee: 0 }
}

fn build_recipes() -> Vec<Recipe> {
    let mut v = vec![
        refine(Material::Steel, 80, &[(ore(Ore::NickelIron), 100)], 20),
        refine(Material::TitaniumAlloy, 80, &[(ore(Ore::Titanium), 100), (ore(Ore::Volatiles), 10)], 30),
        refine(Material::Propellant, 100, &[(ore(Ore::Volatiles), 100)], 15),
        refine(Material::Electronics, 20, &[(EXOTICS, 20), (STEEL, 40)], 40),
        refine(Material::Munitions, 60, &[(STEEL, 50), (ore(Ore::Volatiles), 10)], 20),
        Recipe {
            output: GUNDANIUM,
            makes: 100,
            inputs: vec![(TI_ALLOY, 200), (EXOTICS, 40)],
            station: Station::Foundry,
            secs: 180,
            fee: 400,
        },
        refine(Material::Components, 40, &[(STEEL, 40), (ELECTRONICS, 5), (TI_ALLOY, 5)], 30),
    ];
    for line in LINES {
        for part in Part::ALL {
            v.push(part_recipe(line, part));
        }
    }
    for w in weapons() {
        v.push(weapon_recipe(w));
    }
    for k in ModuleKind::ALL {
        v.push(module_recipe(k));
    }
    v
}

/// An equipment module: mostly machined components, with what its job needs.
fn module_recipe(k: ModuleKind) -> Recipe {
    use ModuleKind::*;
    let (inputs, secs): (&[(Item, u64)], u32) = match k {
        SensorArray => (&[(COMPONENTS, 30), (ELECTRONICS, 25), (TI_ALLOY, 20)], 90),
        FireControlComputer => (&[(COMPONENTS, 20), (ELECTRONICS, 40), (EXOTICS, 10)], 120),
        CapacitorBank => (&[(COMPONENTS, 60), (ELECTRONICS, 30), (STEEL, 200)], 120),
        ReactorBooster => (&[(COMPONENTS, 80), (ELECTRONICS, 20), (EXOTICS, 20)], 150),
        RadiatorPackage => (&[(COMPONENTS, 60), (TI_ALLOY, 100)], 90),
        CompositePlating => (&[(TI_ALLOY, 400), (STEEL, 200), (COMPONENTS, 20)], 120),
        GSeat => (&[(COMPONENTS, 60), (ELECTRONICS, 10), (STEEL, 60)], 60),
        DamageControl => (&[(COMPONENTS, 100), (ELECTRONICS, 30)], 180),
        AuxiliaryTank => (&[(COMPONENTS, 40), (TI_ALLOY, 120)], 90),
        ThrusterKit => (&[(COMPONENTS, 80), (TI_ALLOY, 60), (EXOTICS, 10)], 150),
        LegVerniers => (&[(COMPONENTS, 60), (TI_ALLOY, 60)], 90),
        CargoRack => (&[(COMPONENTS, 30), (STEEL, 200)], 60),
    };
    Recipe {
        output: Item::Module(k),
        makes: 1,
        inputs: inputs.to_vec(),
        station: Station::Fabricator,
        secs,
        fee: 0,
    }
}

/// A part of `line`: structure, armour and electronics in proportion to its mass, plus what its
/// systems need.
fn part_recipe(line: FrameId, part: Part) -> Recipe {
    let m = part_mass_kg(line, part) as f32;
    let gundam = is_gundam(line);
    let (armour, armour_share, steel_share, wiring) =
        if gundam { (GUNDANIUM, 0.5, 0.4, 2.0) } else { (TI_ALLOY, 0.35, 0.6, 1.0) };
    let electronics = wiring
        * match part {
            Part::Head => 0.08,
            Part::Torso => 0.04,
            Part::Backpack => 0.03,
            Part::ArmL | Part::ArmR | Part::Legs => 0.01,
        };
    let mut need: BTreeMap<Item, u64> = BTreeMap::new();
    let mut add = |item: Item, kg: f32| {
        if kg > 0.0 {
            *need.entry(item).or_default() += tens(kg);
        }
    };
    add(armour, m * armour_share);
    add(STEEL, m * steel_share);
    add(ELECTRONICS, m * electronics);
    // A Gundam's reactor, and each Gundam's own system.
    if gundam && part == Part::Torso {
        add(EXOTICS, m * 0.05);
    }
    match (line, part) {
        // The ZERO System.
        (FrameId::WingZero, Part::Torso) => {
            add(ELECTRONICS, 200.0);
            add(EXOTICS, 150.0);
        }
        // The wings, and the frame that folds into Neo-Bird.
        (FrameId::WingZero, Part::Backpack) => {
            add(GUNDANIUM, 100.0);
            add(ELECTRONICS, 60.0);
        }
        // The Hyper Jammer.
        (FrameId::Deathscythe, Part::Torso) => {
            add(ELECTRONICS, 150.0);
            add(EXOTICS, 100.0);
        }
        // Full Open's chest gatlings and micro-missile pods.
        (FrameId::Heavyarms, Part::Torso) => {
            add(STEEL, 300.0);
            add(ELECTRONICS, 40.0);
        }
        (FrameId::Heavyarms, Part::Legs) => {
            add(STEEL, 200.0);
            add(ELECTRONICS, 30.0);
        }
        // The arms that close the Cross Crusher.
        (FrameId::Sandrock, Part::ArmL | Part::ArmR) => add(GUNDANIUM, 50.0),
        _ => {}
    }
    if line == FrameId::Leo && part == Part::Torso {
        add(ELECTRONICS, 40.0);
    }
    let secs = (10.0 + m / 40.0) * if gundam { 1.5 } else { 1.0 };
    Recipe {
        output: Item::Part(line, part),
        makes: 1,
        inputs: need.into_iter().collect(),
        station: Station::Fabricator,
        secs: secs.round() as u32,
        fee: 0,
    }
}

fn weapon_recipe(w: WeaponKind) -> Recipe {
    use WeaponKind::*;
    let (inputs, secs): (&[(Item, u64)], u32) = match w {
        BeamRifle => (&[(TI_ALLOY, 120), (ELECTRONICS, 40), (EXOTICS, 20)], 60),
        MachineCannon => (&[(STEEL, 150), (ELECTRONICS, 10)], 40),
        BeamSaber => (&[(TI_ALLOY, 30), (ELECTRONICS, 20), (EXOTICS, 10)], 40),
        TwinBusterRifle => (&[(GUNDANIUM, 300), (ELECTRONICS, 150), (EXOTICS, 300)], 300),
        BeamGatling => (&[(TI_ALLOY, 200), (ELECTRONICS, 60), (EXOTICS, 40)], 120),
        HomingMissile => (&[(STEEL, 250), (ELECTRONICS, 60)], 90),
        ArmyKnife => (&[(STEEL, 60), (TI_ALLOY, 20)], 20),
        BusterShield => (&[(GUNDANIUM, 150), (ELECTRONICS, 40), (EXOTICS, 40)], 150),
        HeadVulcan => (&[(STEEL, 60), (ELECTRONICS, 10)], 30),
        BeamScythe => (&[(GUNDANIUM, 80), (ELECTRONICS, 60), (EXOTICS, 80)], 180),
        BeamMachineGun => (&[(TI_ALLOY, 150), (ELECTRONICS, 50), (EXOTICS, 30)], 90),
        HeatShotel => (&[(GUNDANIUM, 200), (ELECTRONICS, 30)], 150),
        DragonFang => (&[(GUNDANIUM, 150), (STEEL, 200), (ELECTRONICS, 40)], 150),
        Flamethrower => (&[(STEEL, 150), (ELECTRONICS, 20)], 90),
        BeamGlaive => (&[(GUNDANIUM, 60), (ELECTRONICS, 50), (EXOTICS, 70)], 150),
        // Not items (Mobile Dolls' guns, a special's own mounts, a charged shot): never asked for.
        BeamCannon | ChestGatling | MicroMissile | CrossCrusher | BeamRifleCharged => (&[(STEEL, 100)], 60),
    };
    Recipe {
        output: Item::Weapon(w),
        makes: 1,
        inputs: inputs.to_vec(),
        station: Station::Fabricator,
        secs,
        fee: 0,
    }
}

/// Munitions (kg) a full load of `w`'s rounds takes (0: it fires energy, or it's a blade).
pub fn munitions_per_load(w: WeaponKind) -> u64 {
    match w {
        WeaponKind::MachineCannon => 100,
        WeaponKind::HomingMissile => 120,
        WeaponKind::HeadVulcan => 30,
        WeaponKind::Flamethrower => 150,
        _ => 0,
    }
}

/// Rounds in a full load of `w`.
pub fn rounds_per_load(w: WeaponKind) -> u16 {
    weapon(w).ammo
}

/// The munitions item.
pub const MUNITIONS_ITEM: Item = MUNITIONS;
/// The propellant item.
pub const PROPELLANT_ITEM: Item = mat(Material::Propellant);

/// A suit's tank, kg.
pub fn tank_kg(line: FrameId) -> u32 {
    frame(line).propellant_cap as u32
}

/// How the colony values things: raw ore at fixed prices; everything else at what went into it,
/// plus a quarter for the work. Credits per tonne for bulk goods, per piece otherwise.
pub fn value(item: Item) -> u64 {
    static VALUES: OnceLock<BTreeMap<Item, u64>> = OnceLock::new();
    VALUES.get_or_init(build_values).get(&item).copied().unwrap_or(0)
}

/// The value of `qty` of `item` at `price` (credits per tonne for bulk goods, per piece
/// otherwise), rounded down.
pub fn worth(item: Item, price: u64, qty: u64) -> u64 {
    let v = u128::from(price) * u128::from(qty);
    let v = if item.bulk() { v / 1_000 } else { v };
    u64::try_from(v).unwrap_or(u64::MAX)
}

fn ore_value(o: Ore) -> u64 {
    match o {
        Ore::NickelIron => 1_000,
        Ore::Titanium => 4_000,
        Ore::Volatiles => 3_000,
        Ore::Exotics => 15_000,
    }
}

fn build_values() -> BTreeMap<Item, u64> {
    let mut v: BTreeMap<Item, u64> = BTreeMap::new();
    for o in Ore::ALL {
        v.insert(Item::Ore(o), ore_value(o));
    }
    // Recipes list inputs before what's made from them, except gundanium (made from titanium
    // alloy, listed after it): a few passes settle everything.
    for _ in 0..3 {
        for r in recipes() {
            let Some(cost) = r
                .inputs
                .iter()
                .map(|(item, qty)| v.get(item).map(|price| worth(*item, *price, *qty)))
                .sum::<Option<u64>>()
            else {
                continue;
            };
            let per = (cost + r.fee) as f64 * 1.25 / r.makes as f64;
            let price = if r.output.bulk() { per * 1_000.0 } else { per };
            v.insert(r.output, (price / 10.0).round() as u64 * 10);
        }
    }
    v
}

/// Gundam technology: gundanium, anything made with it, and the Gundams' own parts. The colony
/// won't touch it (it's what OZ is hunting for); only pilots trade it.
pub fn gundam_tech(item: Item) -> bool {
    match item {
        Item::Material(Material::Gundanium) => true,
        Item::Part(line, _) => is_gundam(line),
        Item::Ore(_) | Item::Material(_) => false,
        Item::Weapon(_) => recipe(item).is_some_and(|r| r.inputs.iter().any(|(i, _)| gundam_tech(*i))),
        Item::Module(_) => false,
    }
}

/// How the colony trades an item, if it does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Desk {
    /// Its price with the colony's stock at `target`.
    pub base: u64,
    /// The stock it wants to hold (kg, or pieces).
    pub target: u64,
    /// It buys, and it sells.
    pub buys: bool,
    pub sells: bool,
    /// How long its stock takes to settle back towards `target`, hours (its consumption when it
    /// has too much, its imports when it has too little).
    pub settle_hours: f64,
}

/// The colony's desk for `item`, if it trades it.
pub fn desk(item: Item) -> Option<Desk> {
    if gundam_tech(item) {
        return None;
    }
    let base = value(item);
    let d = |target: u64, buys: bool, sells: bool, settle_hours: f64| Desk {
        base,
        target,
        buys,
        sells,
        settle_hours,
    };
    Some(match item {
        // Raw ore: the colony's smelters take all they can get; it sells none.
        Item::Ore(Ore::Exotics) => d(5_000, true, false, 3.0),
        Item::Ore(Ore::NickelIron) => d(60_000, true, false, 3.0),
        Item::Ore(_) => d(30_000, true, false, 3.0),
        // Propellant is cracked from the colony's own water and sold cheap.
        Item::Material(Material::Propellant) => Desk { base: 1_000, ..d(60_000, true, true, 1.0) },
        Item::Material(Material::Electronics) => d(2_000, true, true, 2.0),
        Item::Material(Material::Munitions) => d(10_000, true, true, 2.0),
        Item::Material(Material::Steel) => d(30_000, true, true, 2.0),
        // The colony's machine shops turn them out, and its own repair crews use them up.
        Item::Material(Material::Components) => d(8_000, true, true, 2.0),
        Item::Material(_) => d(15_000, true, true, 2.0),
        // The militia's Leos, and ordinary weapons.
        Item::Part(..) | Item::Weapon(_) => d(3, true, true, 6.0),
        // Ordinary equipment, a couple of each on the shelf.
        Item::Module(_) => d(2, true, true, 6.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    /// Raw ore (kg) in what `item` is made of, all the way down.
    fn raw(item: Item, qty: f64, out: &mut BTreeMap<Item, f64>) {
        match recipe(item) {
            Some(r) if !matches!(item, Item::Ore(_)) => {
                for (i, q) in &r.inputs {
                    raw(*i, *q as f64 * qty / r.makes as f64, out);
                }
            }
            _ => *out.entry(item).or_default() += qty,
        }
    }

    fn suit_raw(line: FrameId) -> BTreeMap<Item, f64> {
        let mut out = BTreeMap::new();
        for p in Part::ALL {
            raw(Item::Part(line, p), 1.0, &mut out);
        }
        for m in frame(line).loadout.iter().flatten() {
            raw(Item::Weapon(m.weapon), 1.0, &mut out);
        }
        out
    }

    #[test]
    fn everything_but_ore_has_one_recipe_and_its_inputs_can_be_had() {
        for item in Item::all() {
            let n = recipes().iter().filter(|r| r.output == item).count();
            assert_eq!(n, usize::from(!matches!(item, Item::Ore(_))), "{item}");
        }
        for r in recipes() {
            assert!(r.makes > 0 && r.secs > 0 && !r.inputs.is_empty(), "{}", r.output);
            for (i, q) in &r.inputs {
                assert!(*q > 0 && i.bulk(), "{} takes {q} {i}", r.output);
            }
            assert!(value(r.output) > 0, "{} has no value", r.output);
        }
        assert_eq!(recipe(GUNDANIUM).unwrap().station, Station::Foundry);
    }

    #[test]
    fn a_gundam_is_several_leos_and_mostly_exotics() {
        let leo = suit_raw(FrameId::Leo);
        let exotics = |m: &BTreeMap<Item, f64>| m.get(&EXOTICS).copied().unwrap_or(0.0);
        let total =
            |m: &BTreeMap<Item, f64>| m.iter().map(|(i, kg)| value(*i) as f64 * kg / 1_000.0).sum::<f64>();
        for line in [
            FrameId::WingZero,
            FrameId::Heavyarms,
            FrameId::Deathscythe,
            FrameId::Sandrock,
            FrameId::Shenlong,
        ] {
            let g = suit_raw(line);
            assert!(total(&g) > 3.0 * total(&leo), "{line:?}: {} vs a Leo's {}", total(&g), total(&leo));
            assert!(exotics(&g) > 6.0 * exotics(&leo), "{line:?}");
            assert!(exotics(&g) > 1_500.0, "{line:?}: {} kg of exotics", exotics(&g));
        }
        // A Leo is a few mining trips' worth of ore (a hold is 3 t).
        let ore: f64 = leo.values().sum();
        assert!((6_000.0..14_000.0).contains(&ore), "{ore}");
    }

    #[test]
    fn the_colony_values_work_and_keeps_out_of_gundam_tech() {
        assert_eq!(value(Item::Ore(Ore::Titanium)), 4_000);
        // Refining adds value.
        assert!(value(STEEL) > value(Item::Ore(Ore::NickelIron)));
        assert!(value(GUNDANIUM) > value(TI_ALLOY));
        assert!(gundam_tech(GUNDANIUM));
        assert!(gundam_tech(Item::Part(FrameId::Deathscythe, Part::Head)));
        assert!(gundam_tech(Item::Weapon(WeaponKind::TwinBusterRifle)));
        assert!(!gundam_tech(Item::Weapon(WeaponKind::BeamRifle)));
        assert!(!gundam_tech(Item::Part(FrameId::Leo, Part::Torso)));
        assert!(desk(GUNDANIUM).is_none());
        assert!(desk(Item::Part(FrameId::Leo, Part::ArmL)).is_some_and(|d| d.buys && d.sells));
        assert!(desk(Item::Ore(Ore::Exotics)).is_some_and(|d| d.buys && !d.sells));
        // A full Leo tank bought from the colony is a trip's running cost, not a fortune.
        let tank =
            worth(PROPELLANT_ITEM, desk(PROPELLANT_ITEM).unwrap().base, u64::from(tank_kg(FrameId::Leo)));
        assert!((1_000..5_000).contains(&tank), "{tank}");
    }

    #[test]
    fn worth_rounds_down_per_tonne_and_per_piece() {
        assert_eq!(worth(STEEL, 1_560, 999), 1_558);
        assert_eq!(worth(Item::Weapon(WeaponKind::BeamRifle), 3_000, 2), 6_000);
        assert_eq!(worth(STEEL, u64::MAX, u64::MAX), u64::MAX);
    }
}
