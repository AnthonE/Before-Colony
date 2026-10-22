//! The hangar's terminals as the page takes them (survival rules), as JSON: the catalogue, once
//! (every item by slug: its name, whether it's counted in kilograms, what the colony values it at,
//! whether it's Lodestar technology; every recipe; each frame line's tank and weapon mounts), and
//! the pilot's hangar, with what the suit's console works out from it (what could be fitted, what
//! repairs and overhauls would take, the suit's stat sheet, whether it would launch), the exchange
//! and the book they're watching, the Charter Board, the home whose door or panel is open, and the
//! log of what the server said. The browser's `web/ui.js` draws them; the server decides
//! everything, and the page only shows its word.

use std::collections::VecDeque;

use bc_econ::catalogue::{
    desk, lodestar_tech, munitions_per_load, propellant_item, recipes, rounds_per_load, tank_kg, value,
};
use bc_econ::exchange::FEE_BP;
use bc_econ::faults::overhaul_cost;
use bc_econ::item::{LINES, part_name, part_slug};
use bc_econ::suit::{Suit, repair_cost};
use bc_econ::wear::{SERVICE_FROM, service_cost};
use bc_econ::wire::HangarView;
use bc_econ::{Bay, Hangar, Item};
use bc_proto::Part;
use bc_sim::content::modules::MOUNTS;
use bc_sim::content::systems::FAILED;
use bc_sim::content::{Grade, System, frame, frame_name, weapon_name};
use serde_json::{Value, json};

use crate::hangar::HangarState;

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
                        json!({
                            "weapon": Item::Weapon(mount.weapon).slug(),
                            "name": weapon_name(mount.weapon),
                            "rounds": if loads { rounds_per_load(mount.weapon) } else { 0 },
                            "munitions": munitions_per_load(mount.weapon),
                        })
                    }
                    None => Value::Null,
                })
                .collect();
            json!({
                "slug": l.slug(),
                "name": frame_name(l),
                "lodestar": lodestar_tech(Item::Part(l, Part::Torso)),
                "tank": tank_kg(l),
                "mounts": mounts,
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
    json!({
        "items": items,
        "recipes": recipes,
        "lines": lines,
        "parts": parts,
        "systems": systems,
        "module_mounts": mounts,
        "fee_bp": FEE_BP,
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
    json!({
        "launch": launch,
        "repairs": repairs,
        "overhauls": overhauls,
        "fits": fits,
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
        "log": log,
    })
}
