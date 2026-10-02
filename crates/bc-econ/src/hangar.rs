//! A pilot's hangar bay: their credits, their stores, the suit standing in the bay (or out on a
//! sortie), and their two stations' queues. Every change a pilot asks for is a method here that
//! either does all of it or refuses with a sentence (nothing half-done), so the server can apply
//! requests as they come and the tests can play whole careers.

use bc_proto::{ChunkDesc, ChunkKind, FrameId, Part};
use bc_sim::content::frame_name;
use bc_sim::content::salvage::{is_gundam, part_mass_kg};
use bc_sim::sim::{Homecoming, Loadout};
use serde::{Deserialize, Serialize};

use crate::catalogue::{MUNITIONS_ITEM, PROPELLANT_ITEM, Station, munitions_per_load, recipe};
use crate::exchange::{Exchange, Side};
use crate::fab::{MAX_JOBS, Works};
use crate::faults::{Faults, overhaul_cost};
use crate::item::{Item, Material, Ore, is_line, part_name};
use crate::stores::{PartUnit, Stores};
use crate::suit::{MODULE_MOUNTS, Slot, Suit, line_of, repair_cost, scrap_yield};
use bc_sim::content::System;
use bc_sim::content::modules::MOUNTS;
use bc_sim::content::systems::{DAMAGED, FAILED, OK};

/// A part towed home loose (it was shot off) comes back this worn, %.
pub const SALVAGED_LIMB: u8 = 15;
/// A part still on a hulk towed home, %.
pub const SALVAGED_HULK: u8 = 40;
/// The most batches one job may ask for.
pub const MAX_BATCHES: u32 = 500;

/// Where the pilot's suit is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Bay {
    /// Nothing standing in the bay.
    #[default]
    Empty,
    /// Standing in the gantry.
    Docked { suit: Suit },
    /// Out in the sector, as it launched (flying, or asleep with its pilot away).
    Out { suit: Suit },
}

/// A pilot's hangar.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hangar {
    pub credits: u64,
    pub stores: Stores,
    pub bay: Bay,
    pub works: Works,
}

/// How a request went: what to tell the pilot.
pub type Done = Result<String, String>;

fn refuse<T>(why: impl Into<String>) -> Result<T, String> {
    Err(why.into())
}

/// Server knobs that shape the economy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Jobs run this many times faster than their recipes say (testing, events).
    pub craft_speed: f64,
    /// The foundry's fee, percent of the recipe's, and how many times faster it works (the
    /// colony's great works: `charter::Effects`).
    pub foundry_fee_pct: u64,
    pub foundry_speed: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Self { craft_speed: 1.0, foundry_fee_pct: 100, foundry_speed: 1.0 }
    }
}

impl Rules {
    /// These rules, with what the colony's finished works do.
    pub fn with(self, e: &crate::charter::Effects) -> Self {
        Self { foundry_fee_pct: e.foundry_fee_pct, foundry_speed: e.foundry_speed, ..self }
    }
}

impl Hangar {
    /// What a new pilot starts with: a worn, second-hand Leo (no beam rifle: that's the first
    /// thing to buy or build), a little propellant and munitions, and enough credits to get going.
    pub fn starter() -> Self {
        let line = FrameId::Leo;
        let mut suit = Suit::complete(line);
        suit.parts = [Some(70), Some(60), Some(55), Some(65), Some(60), Some(70)];
        suit.mounts[0] = false;
        suit.ammo[0] = 0;
        suit.propellant = suit.tank() / 2;
        // Second-hand, and it shows inside too: its radiators are tired.
        suit.faults.set(System::Radiators, DAMAGED);
        let mut stores = Stores::default();
        stores.add(PROPELLANT_ITEM, 600);
        stores.add(MUNITIONS_ITEM, 100);
        stores.add(Item::Material(Material::Steel), 200);
        Self { credits: 2_000, stores, bay: Bay::Docked { suit }, works: Works::default() }
    }

    /// The suit standing in the bay.
    pub fn suit(&self) -> Option<&Suit> {
        match &self.bay {
            Bay::Docked { suit } => Some(suit),
            _ => None,
        }
    }

    fn suit_mut(&mut self) -> Result<&mut Suit, String> {
        match &mut self.bay {
            Bay::Docked { suit } => Ok(suit),
            Bay::Out { .. } => refuse("your suit is out in the sector"),
            Bay::Empty => refuse("there's no suit in the bay"),
        }
    }

    /// Runs the stations' clocks to `now`, delivering what they finished; what was delivered.
    pub fn settle(&mut self, now: u64) -> Vec<(Item, u64)> {
        let done = self.works.settle(now);
        for (item, qty) in &done {
            self.stores.add(*item, *qty);
        }
        done
    }

    /// Queues `batches` of the recipe that makes `item`.
    pub fn craft(&mut self, item: Item, batches: u32, now: u64, rules: &Rules) -> Done {
        let Some(r) = recipe(item) else { return refuse(format!("{} can't be made", item.name())) };
        if batches == 0 || batches > MAX_BATCHES {
            return refuse("that's not a number of batches");
        }
        self.settle(now);
        let queue = self.works.queue(r.station);
        if queue.is_full() {
            return refuse(format!(
                "the {} has {MAX_JOBS} jobs queued already",
                r.station.name().to_lowercase()
            ));
        }
        let n = u64::from(batches);
        let fee = r.fee * n * rules.foundry_fee_pct / 100;
        if self.credits < fee {
            return refuse(format!("the foundry wants {fee} cr for that"));
        }
        if !self.stores.has_all(&r.inputs, n) {
            let short: Vec<String> = r
                .inputs
                .iter()
                .filter(|(i, q)| !self.stores.has(*i, q * n))
                .map(|(i, q)| format!("{} {}", i.amount(q * n - self.stores.get(*i)), i.name()))
                .collect();
            return refuse(format!("short of {}", short.join(", ")));
        }
        let _ = self.stores.take_all(&r.inputs, n);
        self.credits -= fee;
        let speed = rules.craft_speed * if r.station == Station::Foundry { rules.foundry_speed } else { 1.0 };
        let secs = (f64::from(r.secs) / speed.max(1e-3)).round().max(1.0) as u32;
        self.works.queue(r.station).push(item, batches, secs, now, rules.foundry_fee_pct);
        Ok(format!("QUEUED {} × {}", batches, item.name().to_uppercase()))
    }

    /// Cancels job `index` at `station`: what its unmade batches would have used comes back.
    pub fn cancel_job(&mut self, station: Station, index: usize, now: u64) -> Done {
        self.settle(now);
        let Some((refund, fee)) = self.works.queue(station).cancel(index, now) else {
            return refuse("no such job");
        };
        self.stores.add_all(&refund, 1);
        self.credits += fee;
        Ok("JOB CANCELLED".into())
    }

    /// Fits `item` from the stores onto the suit in the bay (a torso into an empty bay builds a
    /// new suit on it). A part goes on at the best condition there is.
    pub fn fit(&mut self, item: Item) -> Done {
        match (item, &self.bay) {
            (Item::Part(line, Part::Torso), Bay::Empty) => {
                let Some(unit) = self.stores.take_best_part(line, Part::Torso) else {
                    return refuse(format!("no {} in the stores", item.name()));
                };
                self.bay = Bay::Docked { suit: Suit::on(unit) };
                return Ok(format!("A NEW {} STANDS IN THE BAY", item.name().to_uppercase()));
            }
            (_, Bay::Empty) => return refuse("fit a torso first: it's the cockpit and the reactor"),
            _ => {}
        }
        let (line, slot) = {
            let suit = self.suit_mut()?;
            (suit.line, suit.slot_for(item))
        };
        let Some(slot) = slot else {
            return refuse(match item {
                Item::Part(other, _) if other != line => {
                    format!("{} parts don't fit a {}", frame_name(other), frame_name(line))
                }
                Item::Part(..) | Item::Weapon(_) => format!("there's no free place for a {}", item.name()),
                Item::Module(k) => format!(
                    "there's no free place for a {} (it goes on the {}, one to a suit)",
                    item.name(),
                    part_name(k.part())
                ),
                _ => format!("{} isn't something you fit", item.name()),
            });
        };
        match slot {
            Slot::Part { part } => {
                let Some(unit) = self.stores.take_best_part(line, part) else {
                    return refuse(format!("no {} in the stores", item.name()));
                };
                let suit = self.suit_mut()?;
                suit.parts[part as usize] = Some(unit.condition);
                suit.faults = suit.faults.with_part(part, unit.faults);
                let inside =
                    if unit.faults.is_empty() { String::new() } else { format!(" · {}", unit.faults) };
                Ok(format!(
                    "FITTED {} ({}%){}",
                    item.name().to_uppercase(),
                    unit.condition,
                    inside.to_uppercase()
                ))
            }
            Slot::Module { module } => {
                if !self.stores.take(item, 1) {
                    return refuse(format!("no {} in the stores", item.name()));
                }
                let Item::Module(kind) = item else { return refuse("that isn't equipment") };
                self.suit_mut()?.modules[usize::from(module)] = Some(kind);
                Ok(format!("FITTED {}", item.name().to_uppercase()))
            }
            Slot::Mount { mount } => {
                if !self.stores.take(item, 1) {
                    return refuse(format!("no {} in the stores", item.name()));
                }
                let suit = self.suit_mut()?;
                suit.mounts[usize::from(mount)] = true;
                suit.ammo[usize::from(mount)] = 0;
                Ok(format!("FITTED {}", item.name().to_uppercase()))
            }
        }
    }

    /// Takes what's in `slot` off the suit into the stores. The torso can't come off while
    /// anything else is on it; taking it off leaves the bay empty.
    pub fn strip(&mut self, slot: Slot) -> Done {
        let suit = self.suit_mut()?;
        let line = suit.line;
        match slot {
            Slot::Part { part: Part::Torso } => {
                if suit.fitted().count() > 1 || suit.mounts.iter().any(|m| *m) {
                    return refuse("strip everything else first");
                }
                if suit.modules.iter().any(|m| m.is_some()) {
                    return refuse("strip everything else first");
                }
                let condition = suit.parts[Part::Torso as usize].unwrap_or(1);
                let propellant = suit.propellant;
                let faults = suit.faults.of_part(Part::Torso);
                self.stores.add_part(PartUnit { line, part: Part::Torso, condition, faults });
                self.stores.add(PROPELLANT_ITEM, u64::from(propellant));
                self.bay = Bay::Empty;
                Ok("THE BAY IS EMPTY".into())
            }
            Slot::Part { part } => {
                let Some(condition) = suit.parts[part as usize].take() else {
                    return refuse(format!("there's no {} fitted", part_name(part)));
                };
                let faults = suit.faults.of_part(part);
                suit.faults = suit.faults.with_part(part, Faults::NONE);
                // Its equipment comes off with it, into the stores.
                let mut gear = Vec::new();
                for (k, m) in suit.modules.iter_mut().enumerate() {
                    if MOUNTS[k] == part
                        && let Some(kind) = m.take()
                    {
                        gear.push(kind);
                    }
                }
                // Its weapons come off with it.
                let mut notes = vec![format!("{} OFF", part_name(part).to_uppercase())];
                let mut off = Vec::new();
                for m in 0..3 {
                    if suit.mounts[m] && !suit.mount_has_its_part(m) {
                        off.push(m);
                    }
                }
                for m in off {
                    notes.push(self.unmount(m));
                }
                for kind in gear {
                    self.stores.add(Item::Module(kind), 1);
                    notes.push(format!("{} OFF", kind.name().to_uppercase()));
                }
                self.stores.add_part(PartUnit { line, part, condition, faults });
                Ok(notes.join(" · "))
            }
            Slot::Module { module } => {
                let m = usize::from(module);
                let Some(kind) = suit.modules.get_mut(m).and_then(|k| k.take()) else {
                    return refuse("there's no equipment on that mount");
                };
                self.stores.add(Item::Module(kind), 1);
                Ok(format!("{} OFF", kind.name().to_uppercase()))
            }
            Slot::Mount { mount } => {
                let m = usize::from(mount);
                if m >= 3 || !suit.mounts[m] {
                    return refuse("there's no weapon on that mount");
                }
                Ok(self.unmount(m))
            }
        }
    }

    /// Takes the weapon off mount `m` (fitted), unloading its rounds.
    fn unmount(&mut self, m: usize) -> String {
        let Bay::Docked { suit } = &mut self.bay else { return String::new() };
        let Some(w) = suit.weapon_on(m) else { return String::new() };
        suit.mounts[m] = false;
        let rounds = std::mem::take(&mut suit.ammo[m]);
        let full = suit.full_load(m);
        if full > 0 {
            self.stores.add(MUNITIONS_ITEM, u64::from(rounds) * munitions_per_load(w) / u64::from(full));
        }
        self.stores.add(Item::Weapon(w), 1);
        format!("{} OFF", Item::Weapon(w).name().to_uppercase())
    }

    /// Strips the suit bare, torso and all: everything to the stores.
    pub fn dismantle(&mut self) -> Done {
        self.suit_mut()?;
        for m in 0..3 {
            let _ = self.strip(Slot::Mount { mount: m });
        }
        for m in 0..MODULE_MOUNTS as u8 {
            let _ = self.strip(Slot::Module { module: m });
        }
        for part in Part::ALL {
            if part != Part::Torso {
                let _ = self.strip(Slot::Part { part });
            }
        }
        self.strip(Slot::Part { part: Part::Torso })
    }

    /// Repairs `part` on the suit in the bay (or every part) as far as the stores allow.
    pub fn repair(&mut self, part: Option<Part>) -> Done {
        let suit = self.suit_mut()?.clone();
        let parts: Vec<Part> = match part {
            Some(p) => vec![p],
            None => suit.fitted().map(|(p, _)| p).collect(),
        };
        let mut fixed = Vec::new();
        let mut short = false;
        for p in parts {
            let Some(from) = suit.parts[p as usize] else { continue };
            // The furthest it can go with what's in the stores.
            let mut to = 100;
            while to > from && !self.stores.has_all(&repair_cost(suit.line, p, from, to), 1) {
                to -= 1;
            }
            if to < 100 {
                short = true;
            }
            if to > from {
                let _ = self.stores.take_all(&repair_cost(suit.line, p, from, to), 1);
                self.suit_mut()?.parts[p as usize] = Some(to);
                fixed.push(format!("{} {from}→{to}%", part_name(p).to_uppercase()));
            }
        }
        match (fixed.is_empty(), short) {
            (true, true) => refuse("not enough in the stores to repair that"),
            (true, false) => refuse("nothing needs repairing"),
            (false, _) => Ok(format!("REPAIRED {}", fixed.join(" · "))),
        }
    }

    /// Restores the damaged and failed systems inside `part` (or every part) as far as the stores
    /// allow, one system at a time.
    pub fn overhaul(&mut self, part: Option<Part>) -> Done {
        let suit = self.suit_mut()?.clone();
        let mut fixed = Vec::new();
        let mut short = false;
        for (sys, level) in suit.faults.iter() {
            if part.is_some_and(|p| p != sys.part()) || suit.parts[sys.part() as usize].is_none() {
                continue;
            }
            if !self.stores.take_all(&overhaul_cost(suit.line, level), 1) {
                short = true;
                continue;
            }
            self.suit_mut()?.faults.set(sys, OK);
            fixed.push(sys.name().to_uppercase());
        }
        match (fixed.is_empty(), short) {
            (true, true) => {
                refuse("not enough in the stores to overhaul that (machined components, electronics)")
            }
            (true, false) => refuse("nothing inside needs overhauling"),
            (false, _) => Ok(format!("OVERHAULED {}", fixed.join(" · "))),
        }
    }

    /// Melts down one `item` from the stores (a part: the worst there is) for half its materials.
    pub fn scrap(&mut self, item: Item) -> Done {
        let condition = match item {
            Item::Part(line, part) => match self.stores.take_worst_part(line, part) {
                Some(u) => u.condition,
                None => return refuse(format!("no {} in the stores", item.name())),
            },
            Item::Weapon(_) | Item::Module(_) if self.stores.take(item, 1) => 100,
            _ => return refuse(format!("{} can't be scrapped", item.name())),
        };
        let back = scrap_yield(item, condition);
        self.stores.add_all(&back, 1);
        let what: Vec<String> = back.iter().map(|(i, q)| format!("{} {}", i.amount(*q), i.name())).collect();
        Ok(format!("SCRAPPED {}: {}", item.name().to_uppercase(), what.join(", ")))
    }

    /// Readies the suit in the bay for launch: tops up its tank and reloads its guns from the
    /// stores, and sends it out. What the simulation launches.
    pub fn launch(&mut self) -> Result<Loadout, String> {
        let suit = match &self.bay {
            Bay::Docked { suit } => suit.clone(),
            Bay::Out { .. } => return refuse("your suit is already out"),
            Bay::Empty => return refuse("there's no suit in the bay: build one"),
        };
        let mut suit = suit;
        let want = u64::from(suit.tank().saturating_sub(suit.propellant));
        suit.propellant += self.stores.take_up_to(PROPELLANT_ITEM, want) as u32;
        if suit.propellant == 0 {
            return refuse("the tank is dry and there's no propellant in the stores");
        }
        for m in 0..3 {
            let Some(w) = suit.weapon_on(m) else { continue };
            let full = suit.full_load(m);
            if !suit.mounts[m] || full == 0 {
                continue;
            }
            let per_load = munitions_per_load(w);
            let need = u64::from(full - suit.ammo[m].min(full));
            let kg = (need * per_load).div_ceil(u64::from(full));
            let have = self.stores.get(MUNITIONS_ITEM);
            let (kg, rounds) =
                if have >= kg { (kg, need) } else { (have, have * u64::from(full) / per_load) };
            let _ = self.stores.take(MUNITIONS_ITEM, kg);
            suit.ammo[m] = (u64::from(suit.ammo[m]) + rounds).min(u64::from(full)) as u16;
        }
        let loadout = suit.loadout();
        self.bay = Bay::Out { suit };
        Ok(loadout)
    }

    /// The suit came home: what's left of it stands in the bay again, its hold and whatever it
    /// towed in go to the stores, and its bounties are paid. What to tell the pilot.
    pub fn came_home(&mut self, home: &Homecoming) -> String {
        let mut suit = match std::mem::take(&mut self.bay) {
            Bay::Out { suit } | Bay::Docked { suit } => suit,
            Bay::Empty => Suit::complete(line_of(home.frame)),
        };
        suit.came_home(home);
        self.bay = Bay::Docked { suit };
        let mut notes = vec!["DOCKED".to_string()];
        for (kind, kg) in home.cargo_kg.iter().enumerate() {
            if let (Some(ore), true) = (Ore::from_cargo(kind), *kg > 0) {
                self.stores.add(Item::Ore(ore), u64::from(*kg));
                notes.push(format!("{} {}", Item::Ore(ore).amount(u64::from(*kg)), ore.name()));
            }
        }
        if let Some(desc) = home.held {
            notes.extend(self.salvage(&desc));
        }
        if home.bounty > 0 {
            self.credits += u64::from(home.bounty);
            notes.push(format!("BOUNTY {} CR", home.bounty));
        }
        notes.join(" · ")
    }

    /// The suit was destroyed: the bay stays empty; the bounties it earned are still paid.
    pub fn lost(&mut self, bounty: u32) -> String {
        self.bay = Bay::Empty;
        self.credits += u64::from(bounty);
        if bounty > 0 { format!("SUIT LOST · BOUNTY {bounty} CR") } else { "SUIT LOST".into() }
    }

    /// The suit was out when the sector lost track of it (a server restart, cleared for room):
    /// the colony's tugs bring it in as it launched.
    pub fn recover(&mut self) -> bool {
        match std::mem::take(&mut self.bay) {
            Bay::Out { suit } => {
                self.bay = Bay::Docked { suit };
                true
            }
            other => {
                self.bay = other;
                false
            }
        }
    }

    /// What something towed home becomes: ore, a worn part (of a line pilots build), or scrap
    /// (the simulation's own cargo kinds for what a part is made of).
    fn salvage(&mut self, desc: &ChunkDesc) -> Vec<String> {
        let mut notes = Vec::new();
        match desc.kind {
            ChunkKind::Ore { ore } => {
                if let Some(o) = Ore::from_cargo(usize::from(ore)) {
                    self.stores.add(Item::Ore(o), u64::from(desc.mass_kg));
                    notes.push(format!("{} {}", Item::Ore(o).amount(u64::from(desc.mass_kg)), o.name()));
                }
            }
            ChunkKind::Limb { frame, part, .. } => {
                let line = line_of(frame);
                if is_line(line) {
                    // It was shot off: everything inside it failed.
                    let faults = Faults::all(part, FAILED);
                    self.stores.add_part(PartUnit { line, part, condition: SALVAGED_LIMB, faults });
                    notes.push(format!(
                        "SALVAGED {} ({SALVAGED_LIMB}%)",
                        Item::Part(line, part).name().to_uppercase()
                    ));
                } else {
                    notes.push(self.scrap_metal(line, desc.mass_kg));
                }
            }
            ChunkKind::Hulk { frame, parts, .. } => {
                let line = line_of(frame);
                for part in Part::ALL {
                    if parts & (1 << part as u8) == 0 {
                        continue;
                    }
                    if is_line(line) && part != Part::Torso {
                        let faults = Faults::all(part, DAMAGED);
                        self.stores.add_part(PartUnit { line, part, condition: SALVAGED_HULK, faults });
                        notes.push(format!(
                            "SALVAGED {} ({SALVAGED_HULK}%)",
                            Item::Part(line, part).name().to_uppercase()
                        ));
                    } else {
                        // A torso that died is only good for scrap, as is a Mobile Doll.
                        notes.push(self.scrap_metal(line, part_mass_kg(frame, part)));
                    }
                }
            }
        }
        notes
    }

    /// Scrap from a suit of `line`, as the simulation's cargo counts it: a Gundam's gundanium goes
    /// with the exotics, everyone else's metal with the titanium.
    fn scrap_metal(&mut self, line: FrameId, kg: u32) -> String {
        let ore = if is_gundam(line) { Ore::Exotics } else { Ore::Titanium };
        self.stores.add(Item::Ore(ore), u64::from(kg));
        format!("{} {} (SCRAP)", Item::Ore(ore).amount(u64::from(kg)), ore.name())
    }

    /// Collects what the exchange owes this pilot. Its notes.
    pub fn collect(&mut self, exchange: &mut Exchange, trader: &str) -> Vec<String> {
        let a = exchange.collect(trader);
        self.credits += a.credits;
        for (item, qty) in a.goods {
            self.stores.add(item, qty);
        }
        a.notes
    }

    /// Places an order on the exchange, handing over its escrow from this hangar, and collects
    /// whatever filled at once.
    #[allow(clippy::too_many_arguments)]
    pub fn trade(
        &mut self,
        exchange: &mut Exchange,
        trader: &str,
        item: Item,
        side: Side,
        price: u64,
        qty: u64,
        rest: bool,
    ) -> Done {
        if !item.valid() {
            return refuse("no such item");
        }
        let escrow = match side {
            Side::Sell => {
                if !self.stores.take(item, qty) {
                    return refuse(match item {
                        Item::Part(..) => {
                            format!("you have {} new {} to sell", self.stores.get(item), item.name())
                        }
                        _ => {
                            format!("you have {} {} to sell", item.amount(self.stores.get(item)), item.name())
                        }
                    });
                }
                0
            }
            Side::Buy => {
                let e = Exchange::escrow_for(item, price, qty);
                if self.credits < e {
                    return refuse(format!("that needs {e} cr, and you have {}", self.credits));
                }
                self.credits -= e;
                e
            }
        };
        match exchange.place(trader, item, side, price, qty, escrow, rest) {
            Ok(placed) => {
                let mut notes = self.collect(exchange, trader);
                if let Some(id) = placed.resting {
                    let left = exchange.orders(trader).find(|o| o.id == id).map_or(0, |o| o.qty);
                    notes.push(format!("ORDER #{id} WAITING: {} {}", item.amount(left), item.name()));
                } else if placed.fills.is_empty() {
                    notes.push("NOTHING MATCHED".into());
                }
                Ok(notes.join(" · "))
            }
            Err(e) => {
                // Hand the escrow straight back.
                match side {
                    Side::Sell => self.stores.add(item, qty),
                    Side::Buy => self.credits += escrow,
                }
                refuse(e.text())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::{Faction, WeaponKind};

    fn home_as(suit: &Suit) -> Homecoming {
        let l = suit.loadout();
        Homecoming {
            frame: suit.line,
            parts: l.parts,
            mounts: l.mounts,
            ammo: l.ammo,
            propellant: l.propellant,
            systems: l.systems,
            modules: l.modules,
            cargo_kg: [0; 4],
            held: None,
            bounty: 0,
        }
    }

    #[test]
    fn a_new_pilot_can_launch_straight_away() {
        let mut h = Hangar::starter();
        let l = h.launch().unwrap();
        assert!(l.parts.iter().all(|p| *p > 0.0 && *p < 1.0), "worn but whole");
        assert_eq!(l.mounts, 0b110, "no beam rifle");
        assert_eq!(l.propellant, 1_800.0, "the stores topped the tank up as far as they could");
        assert_eq!(l.ammo[1], 400);
        assert!(matches!(h.bay, Bay::Out { .. }));
        assert!(h.launch().is_err());
    }

    #[test]
    fn a_tank_comes_home_no_fuller_than_it_went_out() {
        // Under anime rules the tank refills in flight: what it made out there isn't the stores'.
        let mut h = Hangar::starter();
        let l = h.launch().unwrap();
        let Bay::Out { suit } = h.bay.clone() else { panic!() };
        let mut home = home_as(&suit);
        home.propellant = suit.tank() as f32;
        h.came_home(&home);
        assert_eq!(h.suit().unwrap().propellant, l.propellant as u32, "launched with 1.8 t, home with 1.8 t");
        // Burnt down, it comes home with what's left, as ever.
        let _ = h.launch().unwrap();
        let Bay::Out { suit } = h.bay.clone() else { panic!() };
        let mut home = home_as(&suit);
        home.propellant = 700.0;
        h.came_home(&home);
        assert_eq!(h.suit().unwrap().propellant, 700);
    }

    #[test]
    fn a_sortie_brings_home_ore_salvage_and_bounty() {
        let mut h = Hangar::starter();
        let _ = h.launch().unwrap();
        let Bay::Out { suit } = h.bay.clone() else { panic!() };
        let mut home = home_as(&suit);
        home.cargo_kg = [1_200, 300, 0, 40];
        home.propellant = 900.0;
        home.parts[Part::ArmR as usize] = 0.0;
        home.bounty = 250;
        // Towing in a Leo hulk that still has its head, torso, right arm and legs.
        home.held = Some(ChunkDesc {
            kind: ChunkKind::Hulk { frame: FrameId::Leo, faction: Faction::Oz, parts: 0b01_1011 },
            seed: 0,
            mass_kg: 5_000,
        });
        let note = h.came_home(&home);
        assert!(note.contains("BOUNTY 250"), "{note}");
        let s = h.suit().unwrap();
        assert_eq!(s.parts[Part::ArmR as usize], None);
        assert_eq!(s.propellant, 900);
        assert_eq!(h.stores.get(Item::Ore(Ore::NickelIron)), 1_200);
        // The hulk: its head, left arm and legs as worn parts; its torso as scrap titanium.
        let salvaged: Vec<(Part, u8)> = h.stores.parts().iter().map(|u| (u.part, u.condition)).collect();
        assert_eq!(salvaged, [(Part::Head, 40), (Part::ArmR, 40), (Part::Legs, 40)]);
        assert_eq!(
            h.stores.get(Item::Ore(Ore::Titanium)),
            300 + u64::from(part_mass_kg(FrameId::Leo, Part::Torso))
        );
        assert_eq!(h.credits, 2_250);
        // Our right arm was shot off: the hulk's goes on in its place.
        h.fit(Item::Part(FrameId::Leo, Part::ArmR)).unwrap();
        assert_eq!(h.suit().unwrap().parts[Part::ArmR as usize], Some(40));
        assert!(h.fit(Item::Part(FrameId::Leo, Part::Head)).is_err(), "it has a head");
    }

    #[test]
    fn building_a_suit_from_nothing() {
        let mut h = Hangar { credits: 10_000, ..Hangar::default() };
        assert!(h.fit(Item::Part(FrameId::Leo, Part::Head)).is_err(), "no torso yet");
        h.stores.add(Item::Part(FrameId::Leo, Part::Torso), 1);
        h.stores.add(Item::Part(FrameId::Leo, Part::ArmL), 1);
        h.stores.add(Item::Weapon(WeaponKind::BeamSaber), 1);
        h.stores.add(Item::Weapon(WeaponKind::MachineCannon), 1);
        assert!(h.fit(Item::Weapon(WeaponKind::BeamSaber)).is_err(), "no bay suit yet");
        h.fit(Item::Part(FrameId::Leo, Part::Torso)).unwrap();
        h.fit(Item::Part(FrameId::Leo, Part::ArmL)).unwrap();
        h.fit(Item::Weapon(WeaponKind::BeamSaber)).unwrap();
        h.fit(Item::Weapon(WeaponKind::MachineCannon)).unwrap();
        assert!(h.launch().is_err(), "a dry tank");
        h.stores.add(PROPELLANT_ITEM, 500);
        h.stores.add(MUNITIONS_ITEM, 50);
        let l = h.launch().unwrap();
        assert_eq!(l.parts.map(|p| p > 0.0), [false, true, true, false, false, false]);
        assert_eq!(l.mounts, 0b110);
        assert_eq!(l.ammo[1], 200, "half the munitions a full load takes");
        assert_eq!(l.propellant, 500.0);
    }

    #[test]
    fn stripping_takes_weapons_with_their_part_and_the_torso_last() {
        let mut h = Hangar::starter();
        assert!(h.strip(Slot::Part { part: Part::Torso }).is_err());
        let note = h.strip(Slot::Part { part: Part::ArmL }).unwrap();
        for w in [WeaponKind::MachineCannon, WeaponKind::BeamSaber] {
            assert!(note.contains(&format!("{} OFF", Item::Weapon(w).name().to_uppercase())), "{note}");
        }
        assert_eq!(h.stores.get(Item::Weapon(WeaponKind::MachineCannon)), 1);
        // Its 400 rounds went back as 100 kg of munitions.
        assert_eq!(h.stores.get(MUNITIONS_ITEM), 200);
        h.dismantle().unwrap();
        assert_eq!(h.bay, Bay::Empty);
        assert_eq!(h.stores.parts().len(), 6);
        h.fit(Item::Part(FrameId::Leo, Part::Torso)).unwrap();
        assert_eq!(h.suit().unwrap().parts[Part::Torso as usize], Some(60));
    }

    #[test]
    fn repairs_go_as_far_as_the_stores_allow() {
        let mut h = Hangar::starter();
        assert!(h.repair(Some(Part::Head)).is_err(), "no titanium alloy");
        h.stores.add(Item::Material(Material::TitaniumAlloy), 10_000);
        h.stores.add(Item::Material(Material::Steel), 10_000);
        h.stores.add(Item::Material(Material::Electronics), 3);
        let note = h.repair(Some(Part::Head)).unwrap();
        let head = h.suit().unwrap().parts[Part::Head as usize].unwrap();
        assert!(head > 70 && head < 100, "{note}");
        h.stores.add(Item::Material(Material::Electronics), 1_000);
        h.repair(None).unwrap();
        assert!(h.suit().unwrap().fitted().all(|(_, c)| c == 100));
        assert!(h.repair(None).is_err(), "nothing to repair");
    }

    #[test]
    fn crafting_takes_its_inputs_and_delivers_in_time() {
        let mut h = Hangar::default();
        let steel = Item::Material(Material::Steel);
        assert!(h.craft(steel, 2, 0, &Rules::default()).is_err());
        h.stores.add(Item::Ore(Ore::NickelIron), 250);
        let err = h.craft(steel, 3, 0, &Rules::default()).unwrap_err();
        assert!(err.contains("50 kg Nickel-iron ore"), "{err}");
        h.craft(steel, 2, 0, &Rules { craft_speed: 2.0, ..Rules::default() }).unwrap();
        assert_eq!(h.stores.get(Item::Ore(Ore::NickelIron)), 50);
        assert_eq!(h.settle(10), [(steel, 80)]);
        assert_eq!(h.settle(20), [(steel, 80)]);
        assert_eq!(h.stores.get(steel), 160);
        // The foundry charges.
        let g = Item::Material(Material::Gundanium);
        h.stores.add(Item::Material(Material::TitaniumAlloy), 400);
        h.stores.add(Item::Ore(Ore::Exotics), 80);
        assert!(h.craft(g, 2, 30, &Rules::default()).unwrap_err().contains("800 cr"));
        h.credits = 1_000;
        h.craft(g, 2, 30, &Rules::default()).unwrap();
        assert_eq!(h.credits, 200);
        h.cancel_job(Station::Foundry, 0, 40).unwrap();
        assert_eq!(h.credits, 1_000);
        assert_eq!(h.stores.get(Item::Ore(Ore::Exotics)), 80);
    }

    #[test]
    fn trading_through_the_hangar_keeps_escrow_honest() {
        let mut ex = Exchange::new();
        let mut a = Hangar::starter();
        let mut b = Hangar { credits: 100_000, ..Hangar::default() };
        let ti = Item::Ore(Ore::Titanium);
        a.stores.add(ti, 1_000);
        assert!(a.trade(&mut ex, "a", ti, Side::Sell, 1, 2_000, false).is_err(), "only 1,000 kg");
        // Rest it above the colony's bid, so it waits for a pilot.
        a.trade(&mut ex, "a", ti, Side::Sell, 5_000, 1_000, true).unwrap();
        assert_eq!(a.stores.get(ti), 0);
        let credits = b.credits;
        let note = b.trade(&mut ex, "b", ti, Side::Buy, 6_000, 500, false).unwrap();
        assert!(note.contains("BOUGHT 500 kg"), "{note}");
        assert_eq!(b.stores.get(ti), 500);
        assert_eq!(b.credits, credits - 2_500, "at the resting price");
        let before = a.credits;
        a.collect(&mut ex, "a");
        assert_eq!(a.credits, before + 2_500 - 50);
        // Too poor: refused, and nothing taken.
        let mut c = Hangar::default();
        assert!(c.trade(&mut ex, "c", ti, Side::Buy, 6_000, 500, false).is_err());
        assert_eq!(c, Hangar::default());
    }

    #[test]
    fn it_round_trips_as_json() {
        let mut h = Hangar::starter();
        h.stores.add(Item::Ore(Ore::Volatiles), 100);
        h.craft(Item::Material(Material::Munitions), 1, 5, &Rules::default()).unwrap();
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(serde_json::from_str::<Hangar>(&json).unwrap(), h);
    }
}
