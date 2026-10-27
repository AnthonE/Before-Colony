//! The hangar's terminals as the page takes them (survival rules), as JSON: the catalogue, once
//! (every item by slug: its name, whether it's counted in kilograms, what the colony values it at,
//! whether it's Lodestar technology; every recipe; each frame line's tank and weapon mounts; the
//! staff's dishes at The Arrival, and how much food a pilot holds), and
//! the pilot's hangar, with what the suit's console works out from it (what could be fitted, what
//! repairs and overhauls would take, the suit's stat sheet, whether it would launch), the exchange
//! and the book they're watching, the Charter Board, the home whose door or panel is open, and the
//! log of what the server said. The browser's `web/ui.js` draws them; the server decides
//! everything, and the page only shows its word.

use std::collections::VecDeque;

use bc_econ::body::{FULL_S, PECKISH_S};
use bc_econ::catalogue::{
    desk, lodestar_tech, munitions_per_load, propellant_item, recipes, rounds_per_load, tank_kg, value,
};
use bc_econ::exchange::FEE_BP;
use bc_econ::faults::overhaul_cost;
use bc_econ::food::Dish;
use bc_econ::item::{LINES, part_name, part_slug};
use bc_econ::suit::{Slot, Stats, Suit, repair_cost};
use bc_econ::wear::{SERVICE_FROM, service_cost};
use bc_econ::wire::HangarView;
use bc_econ::{Bay, Hangar, Item};
use bc_proto::Part;
use bc_sim::TICK_HZ;
use bc_sim::content::modules::MOUNTS;
use bc_sim::content::systems::FAILED;
use bc_sim::content::{
    Grade, System, WeaponClass, frame, frame_designation, frame_name, weapon, weapon_name,
};
use serde_json::{Value, json};

use crate::hangar::HangarState;
use crate::page::special_name;

/// What fires each loadout mount (`bc-client`'s `input.rs`): primary, secondary, melee.
const MOUNT_KEYS: [&str; 3] = ["LMB", "RMB", "F"];

fn weapon_class(c: WeaponClass) -> &'static str {
    match c {
        WeaponClass::Beam => "beam",
        WeaponClass::Ballistic => "ballistic",
        WeaponClass::Missile => "missile",
        WeaponClass::Melee => "melee",
        WeaponClass::Cone => "flame",
    }
}

fn amounts(list: &[(Item, u64)]) -> Vec<Value> {
    list.iter().map(|(i, q)| json!([i.slug(), q])).collect()
}

/// Everything that can be owned, made and fitted.
pub fn catalogue() -> Value {
    let items: Vec<Value> = Item::all()
        .into_iter()
        .filter(|i| i.valid())
        .map(|i| {
            let (kind, line, part) = match i {
                Item::Ore(_) => ("ore", None, None),
                Item::Material(_) => ("material", None, None),
                Item::Part(l, p) => ("part", Some(l.slug()), Some(part_slug(p))),
                Item::Weapon(_) => ("weapon", None, None),
                Item::Module(k) => ("module", None, Some(part_slug(k.part()))),
                Item::Kit(_) => ("kit", None, None),
            };
            let d = desk(i);
            json!({
                "slug": i.slug(),
                "name": i.name(),
                "kind": kind,
                "bulk": i.bulk(),
                "value": value(i),
                "lodestar": lodestar_tech(i),
                "line": line,
                "part": part,
                "colony_buys": d.is_some_and(|d| d.buys),
                "colony_sells": d.is_some_and(|d| d.sells),
                "summary": match i {
                    Item::Module(k) => Some(k.summary()),
                    Item::Kit(k) => Some(k.summary()),
                    _ => None,
                },
                "mass": match i { Item::Module(k) => Some(k.mass_kg()), _ => None },
            })
        })
        .collect();
    let recipes: Vec<Value> = recipes()
        .iter()
        .map(|r| {
            json!({
                "output": r.output.slug(),
                "makes": r.makes,
                "inputs": amounts(&r.inputs),
                "station": r.station,
                "station_name": r.station.name(),
                "secs": r.secs,
                "fee": r.fee,
            })
        })
        .collect();
    let lines: Vec<Value> = LINES
        .iter()
        .map(|&l| {
            let spec = frame(l);
            let mounts: Vec<Value> = (0..3)
                .map(|m| match spec.loadout.get(m).copied().flatten() {
                    Some(mount) => {
                        let loads = munitions_per_load(mount.weapon) > 0;
                        let w = weapon(mount.weapon);
                        // A trigger held: a salvo's shots every cooldown.
                        let per_s = f64::from(TICK_HZ) / f64::from(w.cooldown.max(1));
                        let tenths = |x: f32| (f64::from(x) * per_s * 10.0).round() / 10.0;
                        json!({
                            "weapon": Item::Weapon(mount.weapon).slug(),
                            "name": weapon_name(mount.weapon),
                            "rounds": if loads { rounds_per_load(mount.weapon) } else { 0 },
                            "munitions": munitions_per_load(mount.weapon),
                            "class": weapon_class(w.class),
                            // The part it hangs on: losing it loses the weapon.
                            "part": part_slug(mount.arm.part()),
                            "key": MOUNT_KEYS[m],
                            "damage_s": (f64::from(w.damage * f32::from(w.salvo.max(1))) * per_s).round(),
                            "range": f64::from(w.range).round(),
                            "heat_s": tenths(w.heat),
                            "energy_s": tenths(w.energy),
                        })
                    }
                    None => Value::Null,
                })
                .collect();
            json!({
                "slug": l.slug(),
                "name": frame_name(l),
                "designation": frame_designation(l),
                "lodestar": lodestar_tech(Item::Part(l, Part::Torso)),
                "tank": tank_kg(l),
                "mounts": mounts,
                "special": special_name(spec.special),
            })
        })
        .collect();
    let parts: Vec<Value> =
        Part::ALL.iter().map(|&p| json!({ "slug": part_slug(p), "name": part_name(p) })).collect();
    let systems: Vec<Value> = System::ALL
        .iter()
        .map(|s| json!({ "slug": s.slug(), "name": s.name(), "tag": s.tag(), "part": part_slug(s.part()) }))
        .collect();
    let mounts: Vec<&str> = MOUNTS.iter().map(|p| part_slug(*p)).collect();
    // The Arrival's kitchen: the colony's staff cook every dish.
    let dishes: Vec<Value> = Dish::ALL
        .iter()
        .map(|d| json!({ "slug": d.slug(), "name": d.name(), "food_s": d.food_s(), "price": d.price() }))
        .collect();
    json!({
        "items": items,
        "recipes": recipes,
        "lines": lines,
        "parts": parts,
        "systems": systems,
        "module_mounts": mounts,
        "fee_bp": FEE_BP,
        "dishes": dishes,
        "body_full_s": FULL_S,
        "peckish_s": PECKISH_S,
    })
}

/// The hangar as the server last told it, rebuilt (to ask it what a launch would say).
fn rebuilt(v: &HangarView) -> Hangar {
    let mut h = Hangar { credits: v.credits, bay: v.bay.clone(), ..Hangar::default() };
    for (item, qty) in &v.stock {
        h.stores.add(*item, *qty);
    }
    for unit in &v.parts {
        h.stores.add_part(*unit);
    }
    h
}

/// The suit's stat sheet once `strip` has come off and `fit` gone on, as the hangar would do it;
/// `None` where the hangar would refuse (or the bay ends up empty).
fn after(h: &Hangar, strip: Option<Slot>, fit: Option<Item>) -> Option<Stats> {
    let mut h = h.clone();
    if let Some(slot) = strip {
        h.strip(slot).ok()?;
    }
    if let Some(item) = fit {
        h.fit(item).ok()?;
    }
    match &h.bay {
        Bay::Docked { suit } => Some(suit.stats()),
        _ => None,
    }
}

/// The refit, slot by slot (`part.<part>`, `mount.<0..3>`, `module.<0..5>`): the stat sheet
/// without what's in it (`null` where it can't come off), and with each thing the stores hold
/// that could go there instead, what's there taken off first; and the spare parts of the suit's
/// line in the stores, by part (how many, the best condition).
fn refit(v: &HangarView, suit: &Suit) -> (Value, Value) {
    let h = rebuilt(v);
    let held = |item: Item| v.stock.iter().any(|(i, q)| *i == item && *q > 0);
    let mut slots = serde_json::Map::new();
    let mut add = |key: String, fitted: bool, slot: Slot, candidates: Vec<Item>| {
        let strip = fitted.then_some(slot);
        let with: serde_json::Map<String, Value> = candidates
            .into_iter()
            .filter_map(|i| after(&h, strip, Some(i)).map(|st| (i.slug(), json!(st))))
            .collect();
        let without = if fitted { after(&h, strip, None) } else { None };
        slots.insert(key, json!({ "without": without, "with": with }));
    };
    let mut spares = serde_json::Map::new();
    for p in Part::ALL {
        let units: Vec<u8> =
            v.parts.iter().filter(|u| u.line == suit.line && u.part == p).map(|u| u.condition).collect();
        let item = Item::Part(suit.line, p);
        let fitted = suit.parts[p as usize].is_some();
        add(
            format!("part.{}", part_slug(p)),
            fitted,
            Slot::Part { part: p },
            if units.is_empty() { Vec::new() } else { vec![item] },
        );
        if !units.is_empty() {
            let best = units.iter().copied().max().unwrap_or(0);
            spares.insert(part_slug(p).to_string(), json!({ "count": units.len(), "best": best }));
        }
    }
    for m in 0..3 {
        let Some(w) = suit.weapon_on(m) else { continue };
        let item = Item::Weapon(w);
        let candidates = if held(item) { vec![item] } else { Vec::new() };
        add(format!("mount.{m}"), suit.mounts[m], Slot::Mount { mount: m as u8 }, candidates);
    }
    for (k, part) in MOUNTS.iter().enumerate() {
        // Equipment for this part's mounts, one of a kind to a suit.
        let candidates: Vec<Item> = v
            .stock
            .iter()
            .filter_map(|(i, q)| match i {
                Item::Module(kind)
                    if *q > 0
                        && kind.part() == *part
                        && !suit.modules.iter().enumerate().any(|(j, on)| j != k && *on == Some(*kind)) =>
                {
                    Some(*i)
                }
                _ => None,
            })
            .collect();
        add(format!("module.{k}"), suit.modules[k].is_some(), Slot::Module { module: k as u8 }, candidates);
    }
    (Value::Object(slots), Value::Object(spares))
}

/// The suit's console: what could be fitted from the stores, what repairs would take, and
/// whether the suit would launch (and why not).
fn console(v: &HangarView) -> Value {
    let launch = rebuilt(v).launch().err();
    let Bay::Docked { suit } = &v.bay else {
        // An empty bay: a torso from the stores starts a new suit.
        let mut torsos: Vec<Item> =
            v.parts.iter().filter(|u| u.part == Part::Torso).map(|u| u.item()).collect();
        torsos.sort();
        torsos.dedup();
        let fits: Vec<String> = torsos.into_iter().map(Item::slug).collect();
        return json!({ "launch": launch, "repairs": [], "overhauls": [], "fits": fits });
    };
    let repairs: Vec<Value> = suit
        .fitted()
        .filter(|(_, c)| *c < 100)
        .map(|(p, c)| {
            json!({
                "part": part_slug(p),
                "condition": c,
                "cost": amounts(&repair_cost(suit.line, p, c, 100)),
            })
        })
        .collect();
    let mut fits: Vec<Item> = v
        .stock
        .iter()
        .map(|(i, _)| *i)
        .chain(v.parts.iter().map(|u| u.item()))
        .filter(|i| suit.slot_for(*i).is_some())
        .collect();
    fits.sort();
    fits.dedup();
    let fits: Vec<String> = fits.into_iter().map(Item::slug).collect();
    // What's broken inside the parts fitted, and what restoring each takes.
    let overhauls: Vec<Value> = suit
        .faults
        .iter()
        .filter(|(sys, _)| suit.parts[sys.part() as usize].is_some())
        .map(|(sys, level)| {
            json!({
                "system": sys.slug(),
                "part": part_slug(sys.part()),
                "level": if level >= FAILED { "failed" } else { "damaged" },
                "cost": amounts(&overhaul_cost(suit.line, level)),
            })
        })
        .collect();
    // Wear from use: each worn system, how far through its service life.
    let wear: Vec<Value> = suit
        .wear
        .systems(suit.line)
        .into_iter()
        .filter(|(sys, _)| suit.parts[sys.part() as usize].is_some())
        .map(|(sys, used)| json!({ "system": sys.slug(), "used": used }))
        .collect();
    // Each grade of propellant: what the stores hold of it, and how far the suit would go on it.
    let grades: Vec<Value> = Grade::ALL
        .into_iter()
        .map(|g| {
            let st = Suit { grade: g, ..suit.clone() }.stats();
            let item = propellant_item(g);
            json!({
                "grade": g.slug(),
                "name": g.name(),
                "item": item.slug(),
                "stores": v.stock.iter().find(|(i, _)| *i == item).map_or(0, |(_, q)| *q),
                "delta_v": st.delta_v,
                "boost_s": st.boost_s,
            })
        })
        .collect();
    let (refit, spares) = refit(v, suit);
    json!({
        "launch": launch,
        "repairs": repairs,
        "overhauls": overhauls,
        "fits": fits,
        "refit": refit,
        "spares": spares,
        "grade": suit.grade.slug(),
        "grades": grades,
        "stats": suit.stats(),
        "wear": wear,
        "service_cost": amounts(&service_cost(suit.line)),
        "service_from_pct": (SERVICE_FROM * 100.0).round() as u32,
    })
}

/// The home whose door or panel is open, as the page shows it (`docs/OPEN_CITY.md`): its address,
/// what it is and what it costs (the city's own numbers), the register's word on it if it's held,
/// and whether the pilot's at its door or in it.
fn home_json(h: &HangarState, id: u32, inside: bool) -> Value {
    use bc_sim::colony::homes::{HomeId, Kind, View, home};
    use bc_sim::content::homes::{FURNITURE, FURNITURE_BACK_PCT, MAX_PIECES, bought, price, rent, sell_back};
    let Some(home) = home(HomeId(id)) else { return Value::Null };
    let view = match home.view {
        View::Yard => "a yard",
        View::Street => "the street",
        View::Plaza => "a plaza",
        View::Park => "a park",
        View::Canal => "the canal",
        View::Avenue => "the avenue",
        View::Window => "the window bank",
    };
    let what = match home.kind {
        Kind::Houses => format!("A HOUSE OF {} STOREYS", home.storeys),
        Kind::Flats if home.top => "A FLAT ON ITS BUILDING'S TOP STOREY".to_string(),
        Kind::Flats => "A FLAT".to_string(),
    };
    json!({
        "id": id,
        "address": bc_econ::homes::address(&home),
        "about": format!("{what} · {:.0} M² · LOOKING ONTO {}", home.area, view.to_uppercase()),
        "bought": bought(&home),
        "rent": rent(&home),
        "price": price(&home),
        "sell_back": sell_back(&home),
        "line": crate::homes::line(&h.homes, home.id),
        "at": if inside { "inside" } else { "door" },
        "furniture": FURNITURE
            .iter()
            .map(|f| json!({ "slug": f.slug, "name": f.name, "price": f.price, "holds": f.holds }))
            .collect::<Vec<_>>(),
        "max_pieces": MAX_PIECES,
        "furniture_back_pct": FURNITURE_BACK_PCT,
    })
}

/// The hangar as the page takes it; `colony`: the colony's open (the Welcome's COLONY flag), so the
/// bay's terminals point at the desks in the city for what only they do; `home`: the home whose
/// door or panel is open, and whether the pilot's in it.
pub fn hangar_json(
    h: &HangarState,
    log: &VecDeque<(String, bool)>,
    colony: bool,
    home: Option<(u32, bool)>,
) -> Value {
    json!({
        "home": home.map(|(id, inside)| home_json(h, id, inside)),
        "bay": h.bay,
        "colony": colony,
        "view": h.view,
        "market": h.market,
        "charter": h.charter,
        "proving": h.proving,
        "book": h.book.as_ref().map(|(depth, history)| json!({ "depth": depth, "history": history })),
        "console": h.view.as_ref().map(console),
        "body": h.body,
        "log": log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_lists_every_dish_once_with_its_price() {
        let c = catalogue();
        let dishes = c["dishes"].as_array().expect("the dishes");
        assert_eq!(dishes.len(), Dish::COUNT);
        for d in Dish::ALL {
            let listed: Vec<&Value> = dishes.iter().filter(|v| v["slug"] == d.slug()).collect();
            assert_eq!(listed.len(), 1, "{d:?}");
            assert_eq!(listed[0]["price"], d.price());
            assert_eq!(listed[0]["food_s"], d.food_s());
            assert_eq!(listed[0]["name"], d.name());
        }
        assert_eq!((c["body_full_s"].as_u64(), c["peckish_s"].as_u64()), (Some(8 * 3_600), Some(3_600)));
    }
    #[test]
    fn the_refit_says_what_each_choice_would_make_of_the_suit() {
        use bc_econ::stores::PartUnit;
        use bc_sim::content::ModuleKind;
        let mut h = Hangar::starter();
        h.stores.add(Item::Module(ModuleKind::CompositePlating), 1);
        h.stores.add_part(PartUnit {
            line: bc_proto::FrameId::Argo,
            part: Part::Head,
            condition: 90,
            faults: Default::default(),
        });
        let con = console(&HangarView::of(&h, 0));
        let mass = |v: &Value| v["mass_kg"].as_u64().expect("a mass");
        let now = mass(&con["stats"]);
        let refit = &con["refit"];
        // Plating on the torso's first mount weighs; it takes less damage.
        let plated = &refit["module.1"]["with"]["module.composite_plating"];
        assert!(mass(plated) > now);
        assert!(plated["armour"].as_f64() < con["stats"]["armour"].as_f64());
        // Nothing's on the mount to take off; the head comes off lighter, and the spare can go on.
        assert!(refit["module.1"]["without"].is_null());
        assert!(mass(&refit["part.head"]["without"]) < now);
        assert!(refit["part.head"]["with"]["part.argo.head"].is_object());
        assert_eq!(con["spares"]["head"]["best"], 90);
        // The torso can't come off with the rest on it.
        assert!(refit["part.torso"]["without"].is_null());
        // The beam rifle the starter came without isn't in the stores to fit.
        assert_eq!(refit["mount.0"]["with"], json!({}));
    }

    #[test]
    fn each_mount_says_what_fires_it_and_what_it_does() {
        let c = catalogue();
        let argo = c["lines"].as_array().unwrap().iter().find(|l| l["slug"] == "argo").unwrap();
        let keys: Vec<&str> =
            argo["mounts"].as_array().unwrap().iter().map(|m| m["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["LMB", "RMB", "F"]);
        for m in argo["mounts"].as_array().unwrap() {
            assert!(m["damage_s"].as_f64().unwrap() > 0.0 && m["range"].as_f64().unwrap() > 0.0, "{m}");
        }
    }
}
