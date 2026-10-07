//! The hangar's messages: what a pilot asks for ([`Request`]) and what the server tells them
//! ([`Update`]). They ride the control stream as JSON, in frames of their own
//! (`bc_proto::control::HANGAR`): rare, a few kilobytes at most, and easy to read for people and
//! for agents alike.

use bc_proto::Part;
use bc_sim::content::Kit;
use serde::{Deserialize, Serialize};

use crate::catalogue::Station;
use crate::charter::{Board, CharterView, Work};
use crate::exchange::{Depth, Exchange, Quote, Side, Trader};
use crate::hangar::{Bay, Done, Hangar, Rules};
use crate::item::Item;
use crate::proving::BoardView;
use crate::stores::PartUnit;
use crate::suit::Slot;

/// Pilot → server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Request {
    /// Queue `batches` of what makes `item`.
    Craft {
        item: Item,
        batches: u32,
    },
    CancelJob {
        station: Station,
        index: usize,
    },
    /// Fit `item` from the stores onto the suit in the bay.
    Fit {
        item: Item,
    },
    /// Take what's in `slot` off the suit.
    Strip {
        slot: Slot,
    },
    /// Strip the suit bare, torso and all.
    Dismantle,
    /// Repair a part (none: all of them) as far as the stores allow.
    Repair {
        #[serde(default, with = "opt_part")]
        part: Option<Part>,
    },
    /// Restore the damaged and failed systems inside a part (none: every part) as far as the
    /// stores allow.
    Overhaul {
        #[serde(default, with = "opt_part")]
        part: Option<Part>,
    },
    /// Melt one `item` down.
    Scrap {
        item: Item,
    },
    /// An order on the exchange: rest what doesn't fill at once (`rest`), or hand it back.
    Order {
        item: Item,
        side: Side,
        price: u64,
        qty: u64,
        rest: bool,
    },
    CancelOrder {
        id: u64,
    },
    /// Send the book of `item` (and keep sending it as it changes), or stop.
    Watch {
        item: Option<Item>,
    },
    /// Board the suit in the bay and launch it.
    Launch,
    /// Board the suit in the bay and launch it into the colony, through the inner gate (the colony
    /// open): its own sector, weapons safe. `dock` brings it back to the bay from the inner gate.
    LaunchInside,
    /// Take the suit into the bay (it must be resting in the dock).
    Dock,
    /// Ride a cap lift down from the bays into the colony, to Hub Gate on land strip `strip`.
    EnterCity {
        strip: u8,
    },
    /// Ride the cap lift back up to the bay.
    LeaveCity,
    /// On foot at the Blast Hall's gantry in the colony's city: board one of the Charter Board's
    /// trainers and fly it from there (the inside's sector, weapons free in the hall). Nothing of
    /// the pilot's own is taken; `dock`, at rest on the gantry, puts them back on foot there.
    BoardTrainer,
    /// At the Blast Hall's desk: what the gantry readies for the pilot to board (`proving::Trainer`:
    /// the Board's Leo, the build in their bay, or a new suit of any line). Answered with a note
    /// and the board.
    Trainer {
        build: crate::proving::Trainer,
    },
    /// The Most Wanted: how the pilot takes an ace's bounty (`salvage`: the rights to its wreck,
    /// which the tugs bring home; else pay).
    AceTerms {
        salvage: bool,
    },
    /// The Charter Board: post a supply contract (its reward goes into escrow), take one down,
    /// deliver to one from the stores, take or give up a patrol.
    Post {
        item: Item,
        qty: u64,
        reward: u64,
        hours: u64,
    },
    Withdraw {
        id: u64,
    },
    Deliver {
        id: u64,
        qty: u64,
    },
    TakePatrol {
        id: u64,
    },
    DropPatrol {
        id: u64,
    },
    /// Deliver toward one of the colony's great works.
    Contribute {
        work: Work,
        item: Item,
        qty: u64,
    },
    /// Sign the charter.
    Sign,
    /// Send the Charter Board (and keep sending it as it changes), or stop.
    WatchBoard {
        on: bool,
    },
    /// In flight: use a consumable from the suit's rack (the hotbar). Nothing answers: the own
    /// snapshot shows the rack, and what it did.
    UseKit {
        #[serde(with = "kit_serde")]
        kit: Kit,
    },
    /// In flight: eject from the suit (any time; never inside the colony), or, `destruct`, blow up
    /// the doomed suit with the pilot aboard (`docs/DESIGN.md`, "Doom and ejecting"). Nothing
    /// answers but what happens: the suit's loss, and the tugs' word on its wreck.
    Eject {
        #[serde(default)]
        destruct: bool,
    },
    /// Say something on the colony's radio, to everyone connected (any rules): at most
    /// [`SAY_MAX_CHARS`] of it, cleaned ([`clean_line`]).
    Say {
        text: String,
    },
}

/// The longest line the colony's radio carries, characters.
pub const SAY_MAX_CHARS: usize = 160;

/// A line fit for the radio: control characters gone, runs of space made one, trimmed, and cut to
/// [`SAY_MAX_CHARS`]. `None` if nothing's left.
pub fn clean_line(text: &str) -> Option<String> {
    let mut out = String::new();
    for c in text.chars() {
        let c = if c.is_whitespace() { ' ' } else { c };
        if c.is_control() || (c == ' ' && (out.is_empty() || out.ends_with(' '))) {
            continue;
        }
        if out.chars().count() == SAY_MAX_CHARS {
            break;
        }
        out.push(c);
    }
    let out = out.trim_end().to_string();
    (!out.is_empty()).then_some(out)
}

/// Where the pilot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// On foot in the hangar bay.
    Hangar,
    /// In the cockpit, in the sector.
    Space,
    /// On foot inside the colony, in its city.
    City,
}

/// How a sortie ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Docked,
    Lost,
    /// The suit was out when the sector lost it (a restart, cleared for room): towed in.
    Recovered,
}

/// Server → pilot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Update {
    /// Where the pilot is now, and their bay's number.
    Place {
        place: Place,
        bay: u8,
        /// In the city: which land strip.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strip: Option<u8>,
        /// Flying one of the Charter Board's trainers (from the Blast Hall's gantry, back to it).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        trainer: bool,
    },
    Hangar(HangarView),
    Market(MarketView),
    /// The Charter Board.
    Charter(CharterView),
    /// The watched item's book and price history.
    Book {
        depth: Depth,
        history: Vec<u64>,
    },
    /// Something to tell the pilot (`ok`: done; else refused, and why).
    Note {
        text: String,
        ok: bool,
    },
    /// A sortie ended, and (survival) its payout sheet.
    Sortie {
        outcome: Outcome,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debrief: Option<crate::debrief::Debrief>,
    },
    /// News for the pilot (a first arrival; the colony's announcements).
    News {
        text: String,
    },
    /// In the city: the names of people the pilot sees for the first time (by client slot, as the
    /// plaza's datagrams name them).
    People {
        people: Vec<Person>,
    },
    /// A line on the colony's radio: who said it (their callsign), and what.
    Said {
        from: String,
        text: String,
    },
    /// The Proving Ground's board (in the colony, whenever it changes): the day's best times, the
    /// records, and the pilot's own.
    Proving(BoardView),
}

/// Someone in the city, by the slot the plaza's datagrams know them by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: u16,
    pub name: String,
}

/// A job, as the pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobView {
    pub station: Station,
    pub item: Item,
    pub batches: u32,
    pub done: u32,
    /// Seconds a batch takes, and until the job is done.
    pub secs: u32,
    pub secs_left: u64,
}

/// A hangar, as its pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HangarView {
    pub credits: u64,
    /// Bulk goods (kg) and weapons (pieces).
    pub stock: Vec<(Item, u64)>,
    pub parts: Vec<PartUnit>,
    pub bay: Bay,
    pub jobs: Vec<JobView>,
}

impl HangarView {
    pub fn of(h: &Hangar, now: u64) -> Self {
        let mut jobs = Vec::new();
        for station in Station::ALL {
            let q = h.works.get(station);
            let mut ahead = 0u64;
            for (k, job) in q.jobs.iter().enumerate() {
                let own = if k == 0 {
                    job.secs_left(now)
                } else {
                    u64::from(job.batches - job.done) * u64::from(job.secs)
                };
                ahead += own;
                jobs.push(JobView {
                    station,
                    item: job.recipe,
                    batches: job.batches,
                    done: job.done,
                    secs: job.secs,
                    secs_left: ahead,
                });
            }
        }
        Self {
            credits: h.credits,
            stock: h.stores.stock().collect(),
            parts: h.stores.parts().to_vec(),
            bay: h.bay.clone(),
            jobs,
        }
    }
}

/// An open order, as its trader sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderView {
    pub id: u64,
    pub item: Item,
    pub side: Side,
    pub price: u64,
    pub qty: u64,
}

/// The exchange, as a trader sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketView {
    pub quotes: Vec<Quote>,
    pub orders: Vec<OrderView>,
    /// The seller's fee, basis points.
    pub fee_bp: u64,
}

impl MarketView {
    pub fn of(ex: &Exchange, trader: &Trader) -> Self {
        Self {
            quotes: ex.quotes(),
            orders: ex
                .orders(trader)
                .map(|o| OrderView { id: o.id, item: o.item, side: o.side, price: o.price, qty: o.qty })
                .collect(),
            fee_bp: crate::exchange::FEE_BP,
        }
    }
}

/// Does what `req` asks of the hangar (and the exchange), unless it's the server's to do
/// (launching, docking, watching a book): `None` then.
pub fn apply(
    req: &Request,
    hangar: &mut Hangar,
    exchange: &mut Exchange,
    trader: &str,
    now: u64,
    rules: &Rules,
) -> Option<Done> {
    Some(match req {
        Request::Post { .. }
        | Request::Withdraw { .. }
        | Request::Deliver { .. }
        | Request::TakePatrol { .. }
        | Request::DropPatrol { .. }
        | Request::Contribute { .. }
        | Request::AceTerms { .. }
        | Request::Sign => return None,
        Request::Craft { item, batches } => hangar.craft(*item, *batches, now, rules),
        Request::CancelJob { station, index } => hangar.cancel_job(*station, *index, now),
        Request::Fit { item } => hangar.fit(*item),
        Request::Strip { slot } => hangar.strip(*slot),
        Request::Dismantle => hangar.dismantle(),
        Request::Repair { part } => hangar.repair(*part),
        Request::Overhaul { part } => hangar.overhaul(*part),
        Request::Scrap { item } => hangar.scrap(*item),
        Request::Order { item, side, price, qty, rest } => {
            hangar.trade(exchange, trader, *item, *side, *price, *qty, *rest)
        }
        Request::CancelOrder { id } => {
            if exchange.cancel(trader, *id) {
                hangar.collect(exchange, trader);
                Ok(format!("ORDER #{id} CANCELLED"))
            } else {
                Err("no such order".into())
            }
        }
        Request::Watch { .. }
        | Request::Launch
        | Request::LaunchInside
        | Request::BoardTrainer
        | Request::Trainer { .. }
        | Request::Dock
        | Request::EnterCity { .. }
        | Request::LeaveCity
        | Request::WatchBoard { .. }
        | Request::UseKit { .. }
        | Request::Eject { .. }
        | Request::Say { .. } => return None,
    })
}

/// Does what `req` asks of the Charter Board, if it's the board's to do: `None` otherwise.
/// `name` is what the pilot goes by on the board.
#[allow(clippy::too_many_arguments)]
pub fn apply_charter(
    req: &Request,
    hangar: &mut Hangar,
    board: &mut Board,
    exchange: &mut Exchange,
    trader: &str,
    name: &str,
    now: u64,
) -> Option<Done> {
    Some(match *req {
        Request::Post { item, qty, reward, hours } => {
            board.post(hangar, trader, name, item, qty, reward, hours, now)
        }
        Request::Withdraw { id } => board.withdraw(hangar, trader, id),
        Request::Deliver { id, qty } => board.deliver(hangar, exchange, trader, name, id, qty, now),
        Request::TakePatrol { id } => board.take(trader, name, id, now),
        Request::DropPatrol { id } => board.drop_patrol(trader, id, now),
        // What's delivered is built into the work: it's gone from the economy.
        Request::Contribute { work, item, qty } => {
            board.contribute(hangar, trader, name, work, item, qty, now)
        }
        Request::Sign => board.sign(trader, name, now),
        Request::AceTerms { salvage } => board.set_terms(trader, salvage),
        _ => return None,
    })
}

/// JSON, for the wire.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    serde_json::to_vec(msg).unwrap_or_default()
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    serde_json::from_slice(bytes).ok()
}

/// Serde for a consumable, as its slug (`patch_kit`, `coolant`, `chaff`, `stim`).
mod kit_serde {
    use bc_sim::content::Kit;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(k: &Kit, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(k.slug())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Kit, D::Error> {
        let s = String::deserialize(d)?;
        Kit::ALL
            .into_iter()
            .find(|k| k.slug() == s)
            .ok_or_else(|| serde::de::Error::custom(format!("no such consumable: {s}")))
    }
}

/// Serde for an optional part, as its slug or null.
mod opt_part {
    use bc_proto::Part;
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::item::{parse_part, part_slug};

    pub fn serialize<S: Serializer>(p: &Option<Part>, s: S) -> Result<S::Ok, S::Error> {
        match p {
            Some(p) => s.serialize_some(part_slug(*p)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Part>, D::Error> {
        match Option::<String>::deserialize(d)? {
            None => Ok(None),
            Some(s) => {
                parse_part(&s).map(Some).ok_or_else(|| serde::de::Error::custom(format!("no such part: {s}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Material, Ore};
    use bc_proto::{FrameId, WeaponKind};

    #[test]
    fn requests_read_as_json_a_page_can_write() {
        let cases: &[(&str, Request)] = &[
            (
                r#"{"t":"craft","item":"mat.steel","batches":3}"#,
                Request::Craft { item: Item::Material(Material::Steel), batches: 3 },
            ),
            (
                r#"{"t":"fit","item":"weapon.beam_rifle"}"#,
                Request::Fit { item: Item::Weapon(WeaponKind::BeamRifle) },
            ),
            (
                r#"{"t":"strip","slot":{"kind":"part","part":"arm_l"}}"#,
                Request::Strip { slot: Slot::Part { part: Part::ArmL } },
            ),
            (
                r#"{"t":"strip","slot":{"kind":"mount","mount":2}}"#,
                Request::Strip { slot: Slot::Mount { mount: 2 } },
            ),
            (r#"{"t":"repair"}"#, Request::Repair { part: None }),
            (r#"{"t":"repair","part":"torso"}"#, Request::Repair { part: Some(Part::Torso) }),
            (
                r#"{"t":"order","item":"ore.titanium","side":"sell","price":3600,"qty":2000,"rest":false}"#,
                Request::Order {
                    item: Item::Ore(Ore::Titanium),
                    side: Side::Sell,
                    price: 3_600,
                    qty: 2_000,
                    rest: false,
                },
            ),
            (
                r#"{"t":"cancel_job","station":"foundry","index":0}"#,
                Request::CancelJob { station: Station::Foundry, index: 0 },
            ),
            (
                r#"{"t":"watch","item":"part.leo.torso"}"#,
                Request::Watch { item: Some(Item::Part(FrameId::Leo, Part::Torso)) },
            ),
            (r#"{"t":"watch","item":null}"#, Request::Watch { item: None }),
            (r#"{"t":"launch"}"#, Request::Launch),
            (r#"{"t":"launch_inside"}"#, Request::LaunchInside),
            (r#"{"t":"board_trainer"}"#, Request::BoardTrainer),
            (
                r#"{"t":"trainer","build":{"kind":"bay"}}"#,
                Request::Trainer { build: crate::proving::Trainer::Bay },
            ),
            (r#"{"t":"ace_terms","salvage":true}"#, Request::AceTerms { salvage: true }),
            (r#"{"t":"dock"}"#, Request::Dock),
            (r#"{"t":"use_kit","kit":"chaff"}"#, Request::UseKit { kit: Kit::Chaff }),
            (r#"{"t":"eject"}"#, Request::Eject { destruct: false }),
            (r#"{"t":"eject","destruct":true}"#, Request::Eject { destruct: true }),
            (r#"{"t":"say","text":"o7"}"#, Request::Say { text: "o7".into() }),
        ];
        for (json, req) in cases {
            assert_eq!(decode::<Request>(json.as_bytes()).as_ref(), Some(req), "{json}");
            assert_eq!(decode::<Request>(&encode(req)).as_ref(), Some(req));
        }
        assert_eq!(decode::<Request>(br#"{"t":"fit","item":"part.virgo.head"}"#), None);
        assert_eq!(decode::<Request>(b"not json"), None);
    }

    #[test]
    fn a_place_says_when_its_a_trainer_and_the_proving_grounds_board_goes_round() {
        // An old page's place, and one that isn't a trainer's, say nothing of trainers.
        let city = Update::Place { place: Place::City, bay: 3, strip: Some(0), trainer: false };
        assert!(!String::from_utf8(encode(&city)).unwrap().contains("trainer"));
        let back: Update = decode(br#"{"t":"place","place":"space","bay":3}"#).unwrap();
        assert_eq!(back, Update::Place { place: Place::Space, bay: 3, strip: None, trainer: false });
        let flying = Update::Place { place: Place::Space, bay: 3, strip: None, trainer: true };
        assert_eq!(decode::<Update>(&encode(&flying)), Some(flying));
        let mut b = crate::proving::Board::default();
        b.record(crate::proving::Feat::Course, "me", "Heero", 118_000, 1_000_000);
        let board = Update::Proving(b.view("me", 1_000_000));
        assert_eq!(decode::<Update>(&encode(&board)), Some(board));
    }

    #[test]
    fn a_line_for_the_radio_is_cleaned() {
        assert_eq!(clean_line("  hello\tthere \n "), Some("hello there".into()));
        assert_eq!(clean_line("\u{7}\u{1b}[31mred"), Some("[31mred".into()), "control characters go");
        assert_eq!(clean_line(" \n\t "), None);
        let long = "é".repeat(SAY_MAX_CHARS + 40);
        assert_eq!(clean_line(&long).unwrap().chars().count(), SAY_MAX_CHARS);
        // The longest line still fits a frame, as the server sends it.
        let said = Update::Said { from: "W".repeat(16), text: "\u{1F600}".repeat(SAY_MAX_CHARS) };
        let json = encode(&said);
        let mut out = vec![0u8; json.len() + 3];
        assert!(bc_proto::control::encode_hangar(&json, &mut out).is_some(), "{} bytes", json.len());
    }

    #[test]
    fn a_hangar_and_a_market_fit_in_a_frame() {
        let mut h = Hangar::starter();
        let mut ex = Exchange::new();
        for item in Item::all().into_iter().filter(|i| i.bulk()) {
            h.stores.add(item, 123_456);
        }
        h.craft(Item::Material(Material::Munitions), 2, 0, &Rules::default()).unwrap();
        for _ in 0..20 {
            h.stores.add(Item::Part(FrameId::WingZero, Part::ArmL), 1);
        }
        let _ = apply(
            &Request::Order {
                item: Item::Ore(Ore::Titanium),
                side: Side::Sell,
                price: 9_999,
                qty: 100,
                rest: true,
            },
            &mut h,
            &mut ex,
            "me",
            1,
            &Rules::default(),
        );
        let hangar = encode(&Update::Hangar(HangarView::of(&h, 5)));
        let market = encode(&Update::Market(MarketView::of(&ex, &"me".to_string())));
        assert!(hangar.len() < 8_000, "{}", hangar.len());
        assert!(market.len() < 16_000, "{}", market.len());
        let back: Update = decode(&market).unwrap();
        let Update::Market(m) = back else { panic!() };
        assert_eq!(m.orders.len(), 1);
    }
}
