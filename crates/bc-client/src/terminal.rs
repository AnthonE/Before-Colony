//! The hangar's terminals (survival rules), on the page: the fabricator, the stores, the suit's
//! maintenance console and the Colony Exchange are HTML panels (`web/ui.js`), opened by walking up
//! to them in the bay (`onfoot.rs`). This sends them what they show, as JSON:
//! - the catalogue, once: every item by slug (its name, whether it's counted in kilograms, what
//!   the colony values it at, whether it's Gundam technology), every recipe, and each frame
//!   line's tank and weapon mounts;
//! - the pilot's hangar, with what the suit's console works out from it (what could be fitted,
//!   what repairs would take, whether the suit would launch), the exchange and the book they're
//!   watching, whenever the server's word on them changes, and the log of what the server said.
//!
//! What the pilot asks for comes back as `UiCmd::Hangar` (a `bc_econ::Request`), and `onfoot.rs`
//! sends it on. The server decides everything; the page only shows its word.

use std::collections::VecDeque;

use bc_client_core::hangar::HangarState;
use bc_econ::catalogue::{desk, gundam_tech, munitions_per_load, recipes, rounds_per_load, tank_kg, value};
use bc_econ::exchange::FEE_BP;
use bc_econ::item::{LINES, part_name, part_slug};
use bc_econ::suit::repair_cost;
use bc_econ::wire::HangarView;
use bc_econ::{Bay, Hangar, Item};
use bc_proto::Part;
use bc_sim::content::{frame, frame_name, weapon_name};
use bevy::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsValue;

use crate::net::GameClient;
use crate::page::{call_ui, get};

/// What the server said, oldest first (the terminals show the last few).
#[derive(Resource, Default)]
pub struct TerminalLog {
    pub lines: VecDeque<(String, bool)>,
    /// Bumped by every line.
    pub seq: u64,
}

impl TerminalLog {
    const KEEP: usize = 12;

    pub fn push(&mut self, text: String, ok: bool) {
        if self.lines.len() == Self::KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back((text, ok));
        self.seq += 1;
    }
}

fn amounts(list: &[(Item, u64)]) -> Vec<Value> {
    list.iter().map(|(i, q)| json!([i.slug(), q])).collect()
}

/// Everything that can be owned, made and fitted.
fn catalogue() -> Value {
    let items: Vec<Value> = Item::all()
        .into_iter()
        .filter(|i| i.valid())
        .map(|i| {
            let (kind, line, part) = match i {
                Item::Ore(_) => ("ore", None, None),
                Item::Material(_) => ("material", None, None),
                Item::Part(l, p) => ("part", Some(l.slug()), Some(part_slug(p))),
                Item::Weapon(_) => ("weapon", None, None),
            };
            let d = desk(i);
            json!({
                "slug": i.slug(),
                "name": i.name(),
                "kind": kind,
                "bulk": i.bulk(),
                "value": value(i),
                "gundam": gundam_tech(i),
                "line": line,
                "part": part,
                "colony_buys": d.is_some_and(|d| d.buys),
                "colony_sells": d.is_some_and(|d| d.sells),
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
                "gundam": gundam_tech(Item::Part(l, Part::Torso)),
                "tank": tank_kg(l),
                "mounts": mounts,
            })
        })
        .collect();
    let parts: Vec<Value> =
        Part::ALL.iter().map(|&p| json!({ "slug": part_slug(p), "name": part_name(p) })).collect();
    json!({ "items": items, "recipes": recipes, "lines": lines, "parts": parts, "fee_bp": FEE_BP })
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
        return json!({ "launch": launch, "repairs": [], "fits": fits });
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
    json!({ "launch": launch, "repairs": repairs, "fits": fits })
}

fn hangar_json(h: &HangarState, log: &TerminalLog) -> Value {
    json!({
        "bay": h.bay,
        "view": h.view,
        "market": h.market,
        "book": h.book.as_ref().map(|(depth, history)| json!({ "depth": depth, "history": history })),
        "console": h.view.as_ref().map(console),
        "log": log.lines,
    })
}

/// Last thing each frame (survival rules): the catalogue once the page can take it, then the
/// hangar whenever the server's word on it, or the log, moved.
pub fn publish_terminal(
    game: NonSend<GameClient>,
    log: Res<TerminalLog>,
    mut catalogued: Local<bool>,
    mut sent: Local<Option<(u64, u64)>>,
) {
    let g = game.borrow();
    let h = &g.core.hangar;
    if h.place.is_none() {
        return;
    }
    let Some(w) = web_sys::window() else { return };
    if get(&get(&w, "bcUi"), "hangar").is_undefined() {
        return; // the page script isn't in yet: next frame
    }
    if !*catalogued {
        *catalogued = true;
        call_ui("catalogue", &JsValue::from_str(&catalogue().to_string()));
    }
    let now = (h.version, log.seq);
    if *sent == Some(now) {
        return;
    }
    *sent = Some(now);
    call_ui("hangar", &JsValue::from_str(&hangar_json(h, &log).to_string()));
}
