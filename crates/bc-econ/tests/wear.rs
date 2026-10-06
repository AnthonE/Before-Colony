//! Wear and tear in the hangar: what's broken inside a suit's parts comes home and goes with the
//! parts, overhauls restore it, equipment fits on its parts and comes off with them, and records
//! written before any of this load as they were.

use bc_econ::exchange::Exchange;
use bc_econ::faults::overhaul_cost;
use bc_econ::hangar::{Bay, Hangar};
use bc_econ::{Faults, Item, Material, PartUnit, Side, Slot, Suit};
use bc_proto::{FrameId, Part};
use bc_sim::content::systems::{DAMAGED, FAILED, OK};
use bc_sim::content::{ModuleKind, System};
use bc_sim::sim::Homecoming;

fn components() -> Item {
    Item::Material(Material::Components)
}

fn suit(h: &Hangar) -> &Suit {
    h.suit().expect("a suit in the bay")
}

fn home_of(s: &Suit) -> Homecoming {
    let l = s.loadout();
    Homecoming {
        frame: FrameId::Leo,
        parts: l.parts,
        mounts: l.mounts,
        ammo: l.ammo,
        propellant: l.propellant,
        systems: l.systems,
        modules: l.modules,
        kits: l.kits,
        usage: Default::default(),
        cargo_kg: [0; 4],
        held: None,
        bounty: 0,
    }
}

/// A new pilot's Leo is second-hand inside too; an overhaul puts it right, if the stores have
/// the machined components and electronics for it.
#[test]
fn the_starter_leo_needs_an_overhaul() {
    let mut h = Hangar::starter();
    assert_eq!(suit(&h).faults.level(System::Radiators), DAMAGED);
    assert_eq!(suit(&h).loadout().systems.get(System::Radiators), DAMAGED, "it launches as it is");
    assert!(h.overhaul(None).is_err(), "nothing in the stores to do it with");
    h.stores.add_all(&overhaul_cost(FrameId::Leo, DAMAGED), 1);
    let note = h.overhaul(Some(Part::Torso)).unwrap();
    assert!(note.contains("RADIATORS"), "{note}");
    assert!(suit(&h).faults.is_empty());
    assert!(h.overhaul(None).is_err(), "nothing left to overhaul");
}

/// What broke out there comes home; a part shot off takes its faults and its equipment with it.
#[test]
fn faults_and_equipment_come_home_with_their_parts() {
    let mut h = Hangar::starter();
    h.stores.add(Item::Module(ModuleKind::SensorArray), 1);
    h.stores.add(Item::Module(ModuleKind::GSeat), 1);
    h.fit(Item::Module(ModuleKind::SensorArray)).unwrap();
    h.fit(Item::Module(ModuleKind::GSeat)).unwrap();
    let l = h.launch().unwrap();
    assert!(l.modules.has(ModuleKind::SensorArray, 0) && l.modules.has(ModuleKind::GSeat, 0));
    let Bay::Out { suit: out } = &h.bay else { panic!("out") };
    let mut home = home_of(out);
    home.parts[Part::Head as usize] = 0.0;
    home.modules = home.modules.without(1 << Part::Head as u8);
    home.systems.set(System::Tank, FAILED);
    home.systems.set(System::Sensors, DAMAGED);
    h.came_home(&home);
    let s = suit(&h);
    assert_eq!(s.parts[Part::Head as usize], None);
    assert_eq!(s.faults.level(System::Tank), FAILED);
    assert_eq!(s.faults.level(System::Sensors), OK, "the head went with its sensors");
    assert!(s.modules.contains(&Some(ModuleKind::GSeat)));
    assert!(!s.modules.contains(&Some(ModuleKind::SensorArray)), "it was on the head");
}

/// Stripping a part takes its faults to the shelf with it (a faulted part isn't new: it doesn't
/// sell or count as an ingredient), and its equipment to the stores; fitting it brings them back.
#[test]
fn faults_travel_with_parts_and_equipment_comes_off_with_them() {
    let mut h = Hangar::starter();
    h.stores.add(Item::Module(ModuleKind::CompositePlating), 1);
    h.fit(Item::Module(ModuleKind::CompositePlating)).unwrap();
    // Only one of each kind, and only on its own part.
    h.stores.add(Item::Module(ModuleKind::CompositePlating), 1);
    assert!(h.fit(Item::Module(ModuleKind::CompositePlating)).is_err());
    assert_eq!(suit(&h).slot_for(Item::Module(ModuleKind::GSeat)), Some(Slot::Module { module: 2 }));
    // Strip everything but the torso, then the torso: its radiators' fault goes with it.
    h.strip(Slot::Module { module: 1 }).unwrap();
    assert_eq!(h.stores.get(Item::Module(ModuleKind::CompositePlating)), 2);
    h.dismantle().unwrap();
    assert_eq!(h.bay, Bay::Empty);
    assert_eq!(h.stores.get(Item::Part(FrameId::Leo, Part::Torso)), 0, "worn and faulted: not new");
    h.fit(Item::Part(FrameId::Leo, Part::Torso)).unwrap();
    assert_eq!(suit(&h).faults.level(System::Radiators), DAMAGED, "back in with it");
    // A new part is new; one with faults isn't, whatever its armour.
    let mut s = bc_econ::Stores::default();
    s.add_part(PartUnit {
        faults: Faults::all(Part::Head, DAMAGED),
        ..PartUnit::new(FrameId::Leo, Part::Head)
    });
    assert_eq!(s.get(Item::Part(FrameId::Leo, Part::Head)), 0);
    s.add_part(PartUnit::new(FrameId::Leo, Part::Head));
    assert_eq!(s.get(Item::Part(FrameId::Leo, Part::Head)), 1);
    let best = s.take_best_part(FrameId::Leo, Part::Head).unwrap();
    assert!(best.faults.is_empty(), "the sound one first");
}

/// An auxiliary tank makes the tank bigger, so the launch fills more.
#[test]
fn an_auxiliary_tank_holds_more() {
    let mut s = Suit::complete(FrameId::Leo);
    let base = s.tank();
    s.modules[4] = Some(ModuleKind::AuxiliaryTank);
    assert!(s.tank() > base);
    assert!(s.loadout().modules.has(ModuleKind::AuxiliaryTank, 0));
    s.parts[Part::Backpack as usize] = None;
    assert_eq!(s.tank(), base, "no backpack, nothing to carry it");
}

/// Records written before suits had faults or equipment load as they were.
#[test]
fn old_records_load() {
    let mut v = serde_json::to_value(Hangar::starter()).unwrap();
    let suit = v["bay"]["suit"].as_object_mut().unwrap();
    suit.remove("faults");
    suit.remove("modules");
    let h: Hangar = serde_json::from_value(v).unwrap();
    let s = h.suit().unwrap();
    assert!(s.faults.is_empty() && s.modules.iter().all(|m| m.is_none()));
    // And today's round-trip.
    let mut h = Hangar::starter();
    h.stores.add(Item::Module(ModuleKind::CargoRack), 1);
    h.fit(Item::Module(ModuleKind::CargoRack)).unwrap();
    let json = serde_json::to_string(&h).unwrap();
    assert!(json.contains(r#""faults":{"radiators":"damaged"}"#), "{json}");
    assert!(json.contains("cargo_rack"));
    assert_eq!(serde_json::from_str::<Hangar>(&json).unwrap(), h);
}

/// The colony deals in components and ordinary equipment; an exchange kept from before they
/// existed opens their desks at the price it would have had all along.
#[test]
fn the_colony_trades_components_and_equipment() {
    let fresh = Exchange::new();
    for item in [components(), Item::Module(ModuleKind::GSeat)] {
        assert!(fresh.colony_stock(item).is_some(), "{item}");
    }
    // An old exchange: no desk records for the new items.
    let mut v = serde_json::to_value(&fresh).unwrap();
    let colony = v["colony"].as_object_mut().unwrap();
    colony.retain(|k, _| !k.starts_with("module.") && k != "mat.components");
    let mut old: Exchange = serde_json::from_value(v).unwrap();
    assert!(old.colony_stock(components()).is_none());
    assert!(old.seed_missing() >= 13);
    assert_eq!(old.mark(components()), fresh.mark(components()));
    assert_eq!(old.seed_missing(), 0);
    // A pilot can buy a module off the colony's shelf.
    let mut h = Hangar::starter();
    let ask = fresh.mark(Item::Module(ModuleKind::GSeat)).unwrap() * 2;
    let mut ex = Exchange::new();
    h.trade(&mut ex, "pilot", Item::Module(ModuleKind::GSeat), Side::Buy, ask, 1, false).unwrap();
    assert_eq!(h.stores.get(Item::Module(ModuleKind::GSeat)), 1);
}

/// The suit's stat sheet follows what's fitted and what's broken: an auxiliary tank buys delta-v
/// and costs acceleration, a G-seat lets the pilot bear more, failed boosters don't boost.
#[test]
fn the_stat_sheet_follows_the_suit() {
    let whole = Suit::complete(FrameId::Leo).stats();
    assert!((whole.delta_v - 2_600.0).abs() < 300.0, "{whole:?}");
    assert!(whole.boost_g > whole.accel_g);
    let mut s = Suit::complete(FrameId::Leo);
    s.modules[4] = Some(ModuleKind::AuxiliaryTank);
    s.modules[1] = Some(ModuleKind::GSeat);
    let modded = s.stats();
    assert!(modded.delta_v > whole.delta_v * 1.2 && modded.accel_g < whole.accel_g, "{modded:?}");
    assert_eq!(modded.g_tolerance, whole.g_tolerance + 1.0);
    s.faults.set(System::Boosters, FAILED);
    let broken = s.stats();
    assert_eq!(broken.boost_g, broken.accel_g);
}

/// The rack takes up to three of each consumable from the stores at launch; what's left comes back
/// when the suit docks (or is towed in), and nothing when it's lost.
#[test]
fn the_rack_loads_from_the_stores_and_comes_home() {
    use bc_sim::content::Kit;
    let mut h = Hangar::starter();
    h.stores.add(Item::Kit(Kit::Chaff), 5);
    h.stores.add(Item::Kit(Kit::Stim), 1);
    let l = h.launch().unwrap();
    assert_eq!((l.kits.get(Kit::Chaff), l.kits.get(Kit::Stim), l.kits.get(Kit::Patch)), (3, 1, 0));
    assert_eq!(h.stores.get(Item::Kit(Kit::Chaff)), 2);
    let Bay::Out { suit } = &h.bay else { panic!() };
    let mut home = home_of(suit);
    home.kits.take(Kit::Chaff);
    home.kits.take(Kit::Stim);
    h.came_home(&home);
    assert_eq!((h.stores.get(Item::Kit(Kit::Chaff)), h.stores.get(Item::Kit(Kit::Stim))), (4, 0));
    // Towed in, it brings its rack as it went out.
    h.launch().unwrap();
    assert_eq!(h.stores.get(Item::Kit(Kit::Chaff)), 1);
    assert!(h.recover());
    assert_eq!(h.stores.get(Item::Kit(Kit::Chaff)), 4);
    // Lost, it's gone.
    h.launch().unwrap();
    h.lost(0);
    assert_eq!(h.stores.get(Item::Kit(Kit::Chaff)), 1);
}

/// Consumables are made at the fabricator and sold by the colony.
#[test]
fn consumables_are_made_and_traded() {
    use bc_econ::catalogue::{desk, recipe, value};
    use bc_sim::content::Kit;
    for kit in Kit::ALL {
        let item = Item::Kit(kit);
        assert_eq!(item.slug().parse::<Item>().unwrap(), item);
        assert!(recipe(item).is_some(), "{kit:?}");
        assert!(value(item) > 0, "{kit:?}");
        assert!(desk(item).is_some_and(|d| d.buys && d.sells), "{kit:?}");
    }
}

/// Wear from use: a sortie's thruster burn, rounds and overheats add up, a system past its
/// service life comes home damaged, an overhaul restores it, and a worn one can be serviced first.
#[test]
fn systems_wear_from_use_and_are_serviced() {
    use bc_econ::wear::{MAIN_S, OVERHEATS};
    use bc_sim::TICK_HZ;
    use bc_sim::sim::Usage;
    let mut h = Hangar::starter();
    h.stores.add(Item::Material(Material::Components), 10_000);
    h.stores.add(Item::Material(Material::Electronics), 10_000);
    h.overhaul(None).unwrap();
    let fly = |h: &mut Hangar, usage: Usage| {
        h.launch().unwrap();
        let Bay::Out { suit } = &h.bay else { panic!() };
        let mut home = home_of(suit);
        home.usage = usage;
        h.came_home(&home)
    };
    // Half a service life of burn and a few overheats: nothing yet, but it can be serviced.
    let half =
        Usage { burn: MAIN_S * TICK_HZ / 2 + 1, overheats: (OVERHEATS / 2) as u16, ..Usage::default() };
    let note = fly(&mut h, half);
    assert!(!note.contains("WORN"), "{note}");
    assert!(suit(&h).faults.is_empty());
    let text = h.overhaul(None).unwrap();
    assert!(text.contains("MAIN THRUSTERS (SERVICED)"), "{text}");
    assert!(suit(&h).wear.is_none(), "{:?}", suit(&h).wear);
    // A whole life of it: the main thrusters come home damaged, and an overhaul puts them right.
    let note = fly(&mut h, Usage { burn: MAIN_S * TICK_HZ, ..Usage::default() });
    assert!(note.contains("MAIN THRUSTERS WORN"), "{note}");
    assert_eq!(suit(&h).faults.level(System::MainThrusters), DAMAGED);
    let text = h.overhaul(None).unwrap();
    assert!(text.contains("MAIN THRUSTERS"), "{text}");
    assert!(suit(&h).faults.is_empty());
    // Nothing worn, nothing broken: nothing to do.
    assert!(h.overhaul(None).is_err());
}

/// The paint earns its look: the starter Leo comes scuffed (it's second-hand), every sortie adds
/// to its weathering, a hard one more than a quiet one, and servicing what's inside doesn't take
/// it back.
#[test]
fn sorties_weather_the_paint() {
    let mut h = Hangar::starter();
    let start = suit(&h).weathering;
    assert_eq!(start.level(), 2, "second-hand: {start:?}");
    let mut home = home_of(suit(&h));
    h.launch().expect("it launches");
    home.usage.burn = 6 * 60 * bc_sim::TICK_HZ;
    // An arm shot off, the torso knocked about.
    home.parts[Part::ArmL as usize] = 0.0;
    home.parts[Part::Torso as usize] *= 0.5;
    h.came_home(&home);
    let after = suit(&h).weathering;
    assert_eq!(after.sorties, start.sorties + 1);
    assert_eq!(after.thrust, start.thrust + home.usage.burn);
    assert!(after.damage > start.damage + 55, "{after:?}");
    assert!(after.points() > start.points());
    h.stores.add_all(&overhaul_cost(FrameId::Leo, DAMAGED), 1);
    let _ = h.overhaul(Some(Part::Torso));
    assert_eq!(suit(&h).weathering, after, "an overhaul is about what's inside");
}
