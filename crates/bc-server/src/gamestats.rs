//! Stats, achievements and boards on Elo Pros (`meter/gamestats.py` in AnthonE/scry-forge).
//!
//! The game declares a set once: [`declaration`], committed as the `stats` block of `elo.json` at
//! the repo root (`tests/elo_json.rs` holds the two equal). The server reports what each signed-in
//! pilot did, and Elo Pros adds it up: a stat by its `agg`, an achievement unlocking itself when its
//! stat reaches its mark, a board keeping each wallet's best. So the server keeps no record of its
//! own: a session counts what it sees into a [`Tally`] and hands it over every second it has
//! something ([`Reporter::report`]); the reporter merges them by pilot and posts them every
//! [`FLUSH_EVERY`], signed with the server's key (`crate::serverkey`), at most [`MAX_REPORTS`] a
//! call. With no key (`--elo-key`) there is no reporter, and nothing is counted.
//!
//! Only what the ring server makes today, and only against the colony's own side (PvE): sorties,
//! ore and salvage brought home (the Almagest's wrecks, never a pilot's), the colony's contracts
//! and great works delivered to, the Almagest's aces downed, and fills on the Colony Exchange. **No
//! pilot kills, ever**: suits fight pilots away from the colony and anyone on foot anywhere, and a
//! number that counts kills of pilots is a printer for alts. A guest is nobody and is never
//! reported; an agent is reported as one, and never on a board. Under arcade rules (free suits,
//! free respawns) nothing is.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bc_proto::auth::PlayerId;
use bc_proto::{ChunkDesc, ChunkKind, Faction};
use bc_sim::sim::Homecoming;
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, watch};

use crate::net::NetStats;
use crate::serverkey::Signed;

/// How a stat's reports add up on Elo Pros.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agg {
    Sum,
    Max,
}

impl Agg {
    fn slug(self) -> &'static str {
        match self {
            Agg::Sum => "sum",
            Agg::Max => "max",
        }
    }
}

pub struct Stat {
    pub id: &'static str,
    pub name: &'static str,
    pub agg: Agg,
}

const fn sum(id: &'static str, name: &'static str) -> Stat {
    Stat { id, name, agg: Agg::Sum }
}

const fn max(id: &'static str, name: &'static str) -> Stat {
    Stat { id, name, agg: Agg::Max }
}

/// The stats a report draws from. Never rename one with values on record: Elo Pros drops a stat a
/// new set leaves out, with its values.
pub const STATS: &[Stat] = &[
    sum("sorties", "Sorties flown"),
    sum("sorties_home", "Sorties brought home"),
    sum("ore_kg", "Ore brought home, kg"),
    max("best_haul_kg", "Most ore from one sortie, kg"),
    sum("salvage", "Salvage brought home"),
    sum("deliveries", "Charter deliveries"),
    sum("aces_downed", "Aces downed"),
    sum("fills", "Exchange fills"),
];

/// An achievement unlocks itself on Elo Pros when its stat reaches `max`, so a report never names
/// one. Never remove or rename a row someone holds: Elo Pros keeps one a new set leaves out as
/// archived on the profiles that have it.
pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub stat: &'static str,
    pub max: u64,
}

const fn ach(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    stat: &'static str,
    max: u64,
) -> Achievement {
    Achievement { id, name, desc, stat, max }
}

pub const ACHIEVEMENTS: &[Achievement] = &[
    ach("FIRST_SORTIE", "Off the rail", "Launch from your bay", "sorties", 1),
    ach("SORTIES_25", "Regular on the rail", "Fly 25 sorties", "sorties", 25),
    ach("SORTIES_100", "Old hand", "Fly 100 sorties", "sorties", 100),
    ach("FIRST_HOME", "Home again", "Bring your suit home and dock it", "sorties_home", 1),
    ach("HOME_50", "Safe pair of hands", "Bring 50 sorties home", "sorties_home", 50),
    ach("ORE_1T", "A tonne of rock", "Bring home 1,000 kg of ore", "ore_kg", 1_000),
    ach("ORE_100T", "Prospector", "Bring home 100 tonnes of ore", "ore_kg", 100_000),
    ach("HAUL_3T", "Full hold", "Bring home 3,000 kg of ore from one sortie", "best_haul_kg", 3_000),
    ach("FIRST_SALVAGE", "Scrapper", "Bring home a piece of an Almagest wreck", "salvage", 1),
    ach("SALVAGE_50", "Breaker's yard", "Bring home 50 pieces of Almagest wrecks", "salvage", 50),
    ach(
        "FIRST_DELIVERY",
        "On contract",
        "Deliver to one of the colony's contracts or great works",
        "deliveries",
        1,
    ),
    ach("DELIVERIES_50", "The colony's supplier", "Make 50 Charter deliveries", "deliveries", 50),
    ach("FIRST_ACE", "Most Wanted", "Down one of the Almagest's aces", "aces_downed", 1),
    ach("ACES_9", "Every name on the list", "Down nine of the Almagest's aces", "aces_downed", 9),
    ach("FIRST_FILL", "On the books", "Trade on the Colony Exchange", "fills", 1),
    ach("FILLS_100", "A name on the floor", "Fill 100 trades on the Colony Exchange", "fills", 100),
];

/// A board: one score per wallet, its best (highest first). People only.
pub struct Board {
    pub id: &'static str,
    pub name: &'static str,
}

pub const BOARDS: &[Board] = &[Board { id: "haul_kg", name: "Biggest ore haul, kg" }];

/// The set as Elo Pros declares it (`elo.json`'s `stats`, the dev desk's `gamestats` act), in its
/// canonical shape: every field filled in, as `meter/gamestats.py` `validate_defs` writes it.
pub fn declaration() -> Value {
    let stats: Vec<Value> =
        STATS.iter().map(|s| json!({ "id": s.id, "name": s.name, "agg": s.agg.slug() })).collect();
    let achievements: Vec<Value> = ACHIEVEMENTS
        .iter()
        .map(|a| {
            json!({
                "id": a.id, "name": a.name, "desc": a.desc, "hidden": false, "icon": null,
                "progress": { "stat": a.stat, "max": a.max },
            })
        })
        .collect();
    let boards: Vec<Value> = BOARDS
        .iter()
        .map(|b| json!({ "id": b.id, "name": b.name, "sort": "desc", "keep": "best", "format": "number" }))
        .collect();
    json!({ "stats": stats, "achievements": achievements, "boards": boards })
}

/// What one pilot did since their last report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub sorties: u64,
    pub sorties_home: u64,
    pub ore_kg: u64,
    /// The most ore one sortie brought home (a max: merged as one).
    pub best_haul_kg: u64,
    pub salvage: u64,
    pub deliveries: u64,
    pub aces_downed: u64,
    pub fills: u64,
}

/// Whether `desc` is a piece of an Almagest wreck (a drone's or an ace's): salvage, as opposed to
/// a pilot's suit (never counted) or loose ore (counted as ore).
fn almagest_wreck(desc: &ChunkDesc) -> bool {
    match desc.kind {
        ChunkKind::Limb { faction, .. } | ChunkKind::Hulk { faction, .. } => faction == Faction::Almagest,
        ChunkKind::Ore { .. } => false,
    }
}

impl Tally {
    pub fn is_empty(&self) -> bool {
        *self == Tally::default()
    }

    /// A sortie docked: what it brought home in its hold and in hand.
    pub fn home(&mut self, home: &Homecoming) {
        let mut ore: u64 = home
            .cargo_kg
            .iter()
            .enumerate()
            .filter(|(kind, _)| bc_econ::item::Ore::from_cargo(*kind).is_some())
            .map(|(_, kg)| u64::from(*kg))
            .sum();
        if let Some(desc) = &home.held {
            match desc.kind {
                ChunkKind::Ore { ore: kind }
                    if bc_econ::item::Ore::from_cargo(usize::from(kind)).is_some() =>
                {
                    ore += u64::from(desc.mass_kg);
                }
                _ if almagest_wreck(desc) => self.salvage += 1,
                _ => {}
            }
        }
        self.sorties_home += 1;
        self.ore_kg += ore;
        self.best_haul_kg = self.best_haul_kg.max(ore);
    }

    /// The colony's tugs brought a wreck home: an ace's, whose rights were the pilot's terms, is
    /// salvage (their own suit's wreck isn't).
    pub fn towed(&mut self, wreck: Option<&ChunkDesc>, ace: Option<u8>) {
        if ace.is_some() && wreck.is_some() {
            self.salvage += 1;
        }
    }

    /// Folds a later tally in.
    pub fn merge(&mut self, o: &Tally) {
        self.sorties += o.sorties;
        self.sorties_home += o.sorties_home;
        self.ore_kg += o.ore_kg;
        self.best_haul_kg = self.best_haul_kg.max(o.best_haul_kg);
        self.salvage += o.salvage;
        self.deliveries += o.deliveries;
        self.aces_downed += o.aces_downed;
        self.fills += o.fills;
    }

    /// Its stats, by id, those that moved.
    fn stats(&self) -> Map<String, Value> {
        let all = [
            ("sorties", self.sorties),
            ("sorties_home", self.sorties_home),
            ("ore_kg", self.ore_kg),
            ("best_haul_kg", self.best_haul_kg),
            ("salvage", self.salvage),
            ("deliveries", self.deliveries),
            ("aces_downed", self.aces_downed),
            ("fills", self.fills),
        ];
        all.into_iter().filter(|(_, v)| *v > 0).map(|(k, v)| (k.to_string(), json!(v))).collect()
    }

    /// Its board scores, by id.
    fn scores(&self) -> Map<String, Value> {
        let mut m = Map::new();
        if self.best_haul_kg > 0 {
            m.insert("haul_kg".into(), json!(self.best_haul_kg));
        }
        m
    }
}

/// One pilot's report: who (a wallet, or an account's `acct_` id, as Elo Pros spells them), whether
/// an agent flies them, and what they did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub player: PlayerId,
    pub agent: bool,
    pub tally: Tally,
}

/// `POST /api/gamestats/<slug>/report`'s body: `{"reports": [{wallet, agent, stats, scores}]}`.
/// An agent's row carries no scores: the boards are people's.
pub fn report_body(rows: &[Row]) -> String {
    let reports: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = json!({ "wallet": r.player.as_str(), "agent": r.agent, "stats": r.tally.stats() });
            let scores = r.tally.scores();
            if !r.agent && !scores.is_empty() {
                v["scores"] = Value::Object(scores);
            }
            v
        })
        .collect();
    json!({ "reports": reports }).to_string()
}

/// Reports a call takes at most (`meter/gamestats.py` `MAX_REPORTS`).
pub const MAX_REPORTS: usize = 500;
/// How often what the sessions handed over is posted.
pub const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// Rows waiting to be merged at most; past it a session's row is dropped (counted).
const QUEUE: usize = 4_096;

/// Takes the sessions' tallies and posts them to Elo Pros, merged by pilot.
pub struct Reporter {
    tx: mpsc::Sender<Row>,
    stats: Arc<NetStats>,
}

impl Reporter {
    /// The reporter's task, posting with `signed` every `every`, and once more when `stop` says so.
    pub fn start(signed: Signed, stats: Arc<NetStats>, every: Duration, stop: watch::Receiver<bool>) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE);
        tokio::spawn(run(rx, Arc::new(signed), every, stop));
        Self { tx, stats }
    }

    /// Hands `row` over (never waits). A full queue drops it, counted (`net.elo_dropped`).
    pub fn report(&self, row: Row) {
        if row.tally.is_empty() {
            return;
        }
        if self.tx.try_send(row).is_err() {
            self.stats.elo_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn run(
    mut rx: mpsc::Receiver<Row>,
    signed: Arc<Signed>,
    every: Duration,
    mut stop: watch::Receiver<bool>,
) {
    tokio::spawn({
        let signed = signed.clone();
        async move { signed.check().await }
    });
    let mut held: BTreeMap<PlayerId, (bool, Tally)> = BTreeMap::new();
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let last = tokio::select! {
            row = rx.recv() => match row {
                Some(row) => {
                    let e = held.entry(row.player).or_insert((row.agent, Tally::default()));
                    e.0 = row.agent;
                    e.1.merge(&row.tally);
                    // A whole call's worth: sent now.
                    if held.len() < MAX_REPORTS {
                        continue;
                    }
                    false
                }
                None => true,
            },
            _ = tick.tick() => false,
            _ = stop.changed() => true,
        };
        if !held.is_empty() {
            let rows: Vec<Row> = std::mem::take(&mut held)
                .into_iter()
                .map(|(player, (agent, tally))| Row { player, agent, tally })
                .collect();
            let signed = signed.clone();
            let send = async move {
                let path = format!("/gamestats/{}/report", bc_auth::GAME);
                for chunk in rows.chunks(MAX_REPORTS) {
                    if let Err(why) = signed.post(&path, &report_body(chunk)).await {
                        tracing::warn!(pilots = chunk.len(), "stats not reported to Elo Pros: {why}");
                    }
                }
            };
            if last {
                send.await;
            } else {
                tokio::spawn(send);
            }
        }
        if last {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_ok(id: &str, upper: bool) -> bool {
        (1..=64).contains(&id.len())
            && id.bytes().all(|c| {
                c.is_ascii_digit()
                    || c == b'_'
                    || if upper {
                        c.is_ascii_uppercase()
                    } else {
                        c.is_ascii_lowercase() || c == b'.' || c == b':'
                    }
            })
    }

    /// The rules `meter/gamestats.py` checks a set against: ids' shapes, names' lengths, no id a
    /// board ranks money by (`leaderboard.REFUSED_KEYS`), no board named like a row's own field
    /// (`ROW_FIELDS`), and every achievement over a declared stat that counts up.
    #[test]
    fn the_set_is_one_elo_pros_takes() {
        const REFUSED: &[&str] = &[
            "meter",
            "meter_score",
            "meter_number",
            "profile",
            "scry",
            "elo",
            "reserve",
            "balance",
            "holdings",
        ];
        const ROW_FIELDS: &[&str] =
            &["wallet", "at", "agent", "rank", "name", "place", "harness", "model", "key"];
        let mut seen = std::collections::BTreeSet::new();
        for s in STATS {
            assert!(id_ok(s.id, false) && !REFUSED.contains(&s.id) && seen.insert(s.id), "{}", s.id);
            assert!(!s.name.is_empty() && s.name.chars().count() <= 48, "{}", s.name);
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in ACHIEVEMENTS {
            assert!(id_ok(a.id, true) && seen.insert(a.id), "{}", a.id);
            assert!(!a.name.is_empty() && a.name.chars().count() <= 48 && a.desc.chars().count() <= 200);
            assert!(STATS.iter().any(|s| s.id == a.stat) && a.max >= 1, "{}", a.id);
        }
        let mut seen = std::collections::BTreeSet::new();
        for b in BOARDS {
            assert!(
                id_ok(b.id, false) && !REFUSED.contains(&b.id) && !ROW_FIELDS.contains(&b.id),
                "{}",
                b.id
            );
            assert!(seen.insert(b.id) && b.name.chars().count() <= 48);
        }
        // Nothing that counts pilots downed, on a stat, an achievement or a board.
        let words = STATS.iter().map(|s| s.id).chain(BOARDS.iter().map(|b| b.id));
        for w in words {
            assert!(!w.contains("kill") && !w.contains("pilot") && !w.contains("pvp"), "{w}");
        }
    }

    /// What a report says is only ever what the set declares.
    #[test]
    fn a_report_names_only_declared_ids() {
        let all = Tally {
            sorties: 1,
            sorties_home: 1,
            ore_kg: 2,
            best_haul_kg: 2,
            salvage: 1,
            deliveries: 1,
            aces_downed: 1,
            fills: 1,
        };
        let stats = all.stats();
        assert_eq!(stats.len(), STATS.len(), "every stat has a field");
        assert!(stats.keys().all(|k| STATS.iter().any(|s| s.id == k)));
        assert!(all.scores().keys().all(|k| BOARDS.iter().any(|b| b.id == k)));
    }

    #[test]
    fn people_are_on_the_boards_and_agents_are_not() {
        let human = PlayerId::parse("0x1111111111111111111111111111111111111111").unwrap();
        let agent = PlayerId::parse("acct_0123456789abcdef0123").unwrap();
        let haul = Tally { sorties_home: 1, ore_kg: 900, best_haul_kg: 900, ..Tally::default() };
        let body: Value = serde_json::from_str(&report_body(&[
            Row { player: human, agent: false, tally: haul },
            Row { player: agent, agent: true, tally: haul },
        ]))
        .unwrap();
        let r = &body["reports"];
        assert_eq!(r[0]["wallet"], "0x1111111111111111111111111111111111111111");
        assert_eq!((r[0]["agent"].as_bool(), &r[0]["scores"]["haul_kg"]), (Some(false), &json!(900)));
        assert_eq!(r[0]["stats"], json!({ "sorties_home": 1, "ore_kg": 900, "best_haul_kg": 900 }));
        assert_eq!(r[1]["wallet"], "acct_0123456789abcdef0123");
        assert_eq!(r[1]["agent"], true);
        assert!(r[1].get("scores").is_none(), "{body}");
    }

    #[test]
    fn tallies_merge_by_their_rules() {
        let mut a = Tally { sorties: 1, ore_kg: 500, best_haul_kg: 500, ..Tally::default() };
        a.merge(&Tally { sorties: 2, ore_kg: 300, best_haul_kg: 300, fills: 4, ..Tally::default() });
        assert_eq!(a, Tally { sorties: 3, ore_kg: 800, best_haul_kg: 500, fills: 4, ..Tally::default() });
        assert!(Tally::default().is_empty() && !a.is_empty());
    }

    #[test]
    fn salvage_is_the_almagests_wrecks_never_a_pilots() {
        use bc_proto::{FrameId, Part};
        let desc = |kind| ChunkDesc { kind, seed: 0, mass_kg: 400 };
        let drone = desc(ChunkKind::Hulk { frame: FrameId::Corvus, faction: Faction::Almagest, parts: 3 });
        let pilot = desc(ChunkKind::Hulk { frame: FrameId::Argo, faction: Faction::Colonies, parts: 3 });
        let limb =
            desc(ChunkKind::Limb { frame: FrameId::Cetus, faction: Faction::Almagest, part: Part::ArmL });
        let rock = desc(ChunkKind::Ore { ore: 1 });
        assert!(almagest_wreck(&drone) && almagest_wreck(&limb));
        assert!(!almagest_wreck(&pilot) && !almagest_wreck(&rock));
        let mut t = Tally::default();
        t.towed(Some(&drone), None);
        assert_eq!(t.salvage, 0, "their own wreck, towed home");
        t.towed(Some(&drone), Some(2));
        t.towed(None, Some(2));
        assert_eq!(t.salvage, 1, "an ace's wreck, when there was one");
    }
}
