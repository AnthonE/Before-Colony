//! The hangar's messages: what a pilot asks for ([`Request`]) and what the server tells them
//! ([`Update`]). They ride the control stream as JSON, in frames of their own
//! (`bc_proto::control::HANGAR`): rare, a few kilobytes at most, and easy to read for people and
//! for agents alike.

use bc_proto::Part;
use bc_sim::colony::watch::{Offence, UnitKind};
use bc_sim::content::city::PlaceKind;
use bc_sim::content::{Grade, Kit};
use serde::{Deserialize, Serialize};

use crate::catalogue::Station;
use crate::charter::{Board, CharterView, Work};
use crate::exchange::{Depth, Exchange, Quote, Side, Trader};
use crate::hangar::{Bay, Done, Hangar, Rules};
use crate::item::Item;
use crate::proving::BoardView;
use crate::stores::PartUnit;
use crate::suit::{Slot, grade_serde};

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
    /// Fuel the suit in the bay with propellant of `grade` (`standard`, `refined`, `ultra`): what
    /// it has of another grade is pumped back to the stores first.
    Fuel {
        #[serde(with = "grade_serde")]
        grade: Grade,
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
    /// Out through the bay's airlock onto Bay Row, the base's deck in the hub (`docs/RING.md`, "The
    /// base"): on foot among the other pilots and the Muster's buildings.
    EnterBase,
    /// Back in through the bay's airlock from Bay Row (standing at its door).
    LeaveBase,
    /// Down a spoke's lift from the bay to the ring's town (`docs/FREE_ROAM.md`): on foot at Hub
    /// Gate's lift, in its square.
    EnterTown,
    /// Back up the lift to the bay, from the door of any of the town's lifts.
    LeaveTown,
    /// From the door of one of the town's lifts, on foot: up its spoke, across the hub and down
    /// lift `to`'s spoke to its square (the town's quick way round the ring). Answered with
    /// [`Update::Across`], or refused with a `note`.
    RideAcross {
        to: u16,
    },
    /// At the wheel in the ring's town, the pilot's car hit one of its people on foot
    /// (`bc_sim::colony::street`'s person `person`): the server checks where that person was, and
    /// everyone in the town sees them down ([`Update::Street`]).
    Hit {
        person: u32,
    },
    /// At the wheel in the ring's town, stopped by someone standing in a lift's square
    /// (`bc_sim::colony::street`'s person `person`): take them as a fare. The server says where to
    /// and what it pays ([`Update::Fare`]).
    TakeFare {
        person: u32,
    },
    /// Stopped in the square the fare asked for: set them down, and be paid.
    DropFare,
    /// On foot in the ring's town, at the side of a car parked at a kerb
    /// (`bc_sim::colony::street::kerb`): take it. The server checks the car is there, near the
    /// pilot and not already taken; everyone near sees the kerb empty ([`Update::Street`]), and the
    /// Watch minds (it's somebody's). Refused with a `note`.
    TakeParked {
        kerb: u32,
    },
    /// On foot in the ring's town, at the side of a car of the traffic stopped for them
    /// (`bc_sim::colony::street::Car::id`): take it from its driver. The server checks its lane runs
    /// beside the pilot with its place no further on than a held car falls behind
    /// (`street::PATIENCE`); everyone sees it gone from the traffic, and the Watch minds.
    Jack {
        car: u32,
    },
    /// Out of the car the server has them holding (one taken from a kerb, the traffic or where it
    /// was left), standing where it is: its middle on the ring's floor, across it and round it (the
    /// deck's `s`), cm, and the way it faces (the walker's yaw), mrad. Left there for anyone
    /// ([`LeftCar`]); within a few metres of where the pilot is, or refused.
    LeaveCar {
        x_cm: i32,
        s_cm: i32,
        yaw_mrad: i32,
    },
    /// On foot in the ring's town, at the side of a car left about ([`LeftCar::id`]): take it.
    TakeLeft {
        id: u32,
    },
    /// Stopped by the Watch in the ring's town (one of its cars at their side): the fine of the heat
    /// the server holds on them ([`watch_fine`]) is paid from the bay's credits, as much of it as
    /// there is, and the heat is cleared ([`Update::Heat`]). Nothing on them: refused.
    Stopped,
    /// In the ring's town, the pilot did something the Watch minds that the server can't see for
    /// itself (`bc_sim::colony::watch::Offence::reported`: a car taken from a kerb or its driver,
    /// one of the Watch's own, a knock against another car). It can only add heat: whether the
    /// Watch saw it is the server's to judge, and the same report is taken at most once a second.
    Offence {
        #[serde(with = "offence_serde")]
        what: Offence,
    },
    /// In the ring's town, one of the Watch's cars after the pilot has them in sight: their heat
    /// doesn't cool while it does. Taken at most once a second; it never adds or takes heat.
    Sighted,
    /// On foot at the Blast Hall's gantry in the colony's city: board one of the Charter Board's
    /// trainers and fly it from there (the inside's sector, weapons free in the hall). Nothing of
    /// the pilot's own is taken; `dock`, at rest on the gantry, puts them back on foot there.
    BoardTrainer,
    /// At the Blast Hall's desk: what the gantry readies for the pilot to board (`proving::Trainer`:
    /// the Board's Argo, the build in their bay, or a new suit of any line). Answered with a note
    /// and the board.
    Trainer {
        build: crate::proving::Trainer,
    },
    /// The Most Wanted: how the pilot takes an ace's bounty (`salvage`: the rights to its wreck,
    /// which the tugs bring home; else pay).
    AceTerms {
        salvage: bool,
    },
    /// At The Arrival's counter: a dish from the colony's staff (`food::Dish`, by its slug), paid
    /// for from the bay's credits (`body::eat_at_counter`). Never in the cockpit.
    Eat {
        dish: crate::food::Dish,
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
    /// A home (`bc_sim::colony::homes::HomeId`, `docs/OPEN_CITY.md`), at its door: rent it for
    /// `weeks` (paid ahead into its panel), buy it, give it up (a lease ended, its rent back; a
    /// house sold back to the colony).
    Rent {
        home: u32,
        weeks: u64,
    },
    Buy {
        home: u32,
    },
    GiveUp {
        home: u32,
    },
    /// At a home's door: a code on its keypad, open or shut it, knock.
    EnterCode {
        home: u32,
        code: u16,
    },
    Door {
        home: u32,
        open: bool,
    },
    Knock {
        home: u32,
    },
    /// At a home's panel, inside it, on its crew: set its main code (or its guest code: `guest`;
    /// none clears it), lock or unlock its door, leave its crew, clear its crew; and anyone there,
    /// pay rent into it; its crew, take some back.
    SetCode {
        home: u32,
        guest: bool,
        code: Option<u16>,
    },
    Lock {
        home: u32,
        locked: bool,
    },
    LeaveCrew {
        home: u32,
    },
    ClearCrew {
        home: u32,
    },
    Feed {
        home: u32,
        credits: u64,
    },
    TakeRent {
        home: u32,
        credits: u64,
    },
    /// In a home, on its crew: buy a piece of furniture (`furniture`: its slug) and put it down,
    /// its middle `(s, x)` dm from the home's floor's −s −x corner, `turn` quarter turns, on
    /// `storey` (a house's); move one; sell one back; put things from the bay into a chest or a
    /// safe, or take them back out.
    Furnish {
        home: u32,
        furniture: String,
        s: u16,
        x: u16,
        turn: u8,
        #[serde(default)]
        storey: u8,
    },
    MovePiece {
        home: u32,
        piece: u16,
        s: u16,
        x: u16,
        turn: u8,
        #[serde(default)]
        storey: u8,
    },
    SellPiece {
        home: u32,
        piece: u16,
    },
    Stash {
        home: u32,
        piece: u16,
        item: Item,
        qty: u64,
    },
    Unstash {
        home: u32,
        piece: u16,
        item: Item,
        qty: u64,
    },
    /// On foot (`docs/OPEN_CITY.md`, "Heat and the Watch"; `bc_sim::colony::brawl`): a blow thrown at
    /// pilot `pilot` (their session's id, as the plaza's datagrams know them), from where the pilot
    /// says they stood: across (or along) and round their floor, cm, and the way they faced (the
    /// walker's yaw), mrad. The server checks that against the poses it took of them lately, then
    /// whether it lands (`brawl::lands`) on any pose of the other's of the last moment (everyone is
    /// drawn a little late); refused with a `note`.
    Strike {
        pilot: u16,
        x_cm: i32,
        s_cm: i32,
        yaw_mrad: i32,
    },
    /// At the wheel, the pilot's car reached pilot `pilot` on foot: the server checks it from the
    /// poses it took of both (fast enough, near enough, a moment ago) and knocks them down.
    RunDown {
        pilot: u16,
    },
    /// On foot beside pilot `pilot`, who is down: start lifting them up (`on`), or stop. The server
    /// lifts them once it has had the two of them near enough together long enough
    /// (`brawl::HELP_TICKS`, `HELP_REACH`).
    Help {
        pilot: u16,
        on: bool,
    },
}

/// Where a request about a home is made: at its door (outside it, or just in), or inside it, at its
/// panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtHome {
    Door,
    Inside,
}

impl Request {
    /// The desk in the colony's city that alone does what this asks, while the colony is open
    /// (`docs/COLONY.md`, 2.6 "Key places"): an order at the pilot's own price that waits on the book is
    /// placed on the Exchange floor; contracts are posted, patrols taken and the charter signed at
    /// the Charter Board's desk; terms for an ace's bounty are settled, and meals eaten, at The Arrival; the test
    /// range is picked at the Proving Ground's desk. `None`: any terminal does it (the bay's
    /// included). Every request is listed, so a new one has to choose.
    pub fn desk(&self) -> Option<PlaceKind> {
        match self {
            Request::Order { rest: true, .. } => Some(PlaceKind::Exchange),
            Request::Post { .. } | Request::TakePatrol { .. } | Request::Sign => Some(PlaceKind::Charter),
            Request::AceTerms { .. } | Request::Eat { .. } => Some(PlaceKind::Bar),
            Request::Trainer { .. } => Some(PlaceKind::Proving),
            Request::Order { rest: false, .. }
            | Request::Craft { .. }
            | Request::CancelJob { .. }
            | Request::Fit { .. }
            | Request::Strip { .. }
            | Request::Dismantle
            | Request::Repair { .. }
            | Request::Overhaul { .. }
            | Request::Scrap { .. }
            | Request::Fuel { .. }
            | Request::CancelOrder { .. }
            | Request::Watch { .. }
            | Request::Launch
            | Request::LaunchInside
            | Request::Dock
            | Request::EnterCity { .. }
            | Request::LeaveCity
            | Request::EnterBase
            | Request::LeaveBase
            | Request::EnterTown
            | Request::LeaveTown
            | Request::RideAcross { .. }
            | Request::Stopped
            | Request::Offence { .. }
            | Request::Sighted
            | Request::Hit { .. }
            | Request::TakeFare { .. }
            | Request::DropFare
            | Request::TakeParked { .. }
            | Request::Jack { .. }
            | Request::LeaveCar { .. }
            | Request::TakeLeft { .. }
            | Request::BoardTrainer
            | Request::Withdraw { .. }
            | Request::Deliver { .. }
            | Request::DropPatrol { .. }
            | Request::Contribute { .. }
            | Request::WatchBoard { .. }
            | Request::UseKit { .. }
            | Request::Eject { .. }
            | Request::Say { .. }
            | Request::Strike { .. }
            | Request::RunDown { .. }
            | Request::Help { .. } => None,
            // Its own door or panel (`Request::home`), not a desk.
            Request::Rent { .. }
            | Request::Buy { .. }
            | Request::GiveUp { .. }
            | Request::EnterCode { .. }
            | Request::Door { .. }
            | Request::Knock { .. }
            | Request::SetCode { .. }
            | Request::Lock { .. }
            | Request::LeaveCrew { .. }
            | Request::ClearCrew { .. }
            | Request::Feed { .. }
            | Request::TakeRent { .. }
            | Request::Furnish { .. }
            | Request::MovePiece { .. }
            | Request::SellPiece { .. }
            | Request::Stash { .. }
            | Request::Unstash { .. } => None,
        }
    }

    /// The home this asks about, and where the pilot must be to ask it: at its door, or in it.
    pub fn home(&self) -> Option<(bc_sim::colony::homes::HomeId, AtHome)> {
        use bc_sim::colony::homes::HomeId;
        let (home, at) = match *self {
            Request::Rent { home, .. }
            | Request::Buy { home }
            | Request::EnterCode { home, .. }
            | Request::Door { home, .. }
            | Request::Knock { home } => (home, AtHome::Door),
            Request::GiveUp { home }
            | Request::SetCode { home, .. }
            | Request::Lock { home, .. }
            | Request::LeaveCrew { home }
            | Request::ClearCrew { home }
            | Request::Feed { home, .. }
            | Request::TakeRent { home, .. }
            | Request::Furnish { home, .. }
            | Request::MovePiece { home, .. }
            | Request::SellPiece { home, .. }
            | Request::Stash { home, .. }
            | Request::Unstash { home, .. } => (home, AtHome::Inside),
            _ => return None,
        };
        Some((HomeId(home), at))
    }
}

/// Why a request that only desk `k` does was refused elsewhere (shown as `CAN'T: ` and the text
/// in upper case): where to go.
pub fn desk_refusal(k: PlaceKind) -> &'static str {
    match k {
        PlaceKind::Exchange => {
            "a price that waits on the book is placed at the exchange floor's counter, on charter square"
        }
        PlaceKind::Charter => {
            "that's done in person at the charter board's desk, in its hall on charter square"
        }
        PlaceKind::Bar => {
            "meals and terms for an ace's bounty are at the arrival's counter, off charter square"
        }
        PlaceKind::Proving => "the test range is picked at the proving ground's desk, in the blast hall",
        // `Request::desk` never names it.
        PlaceKind::HubGate => "that's done at hub gate",
    }
}

/// What a trade's note gains when an order meant to wait on the book was placed away from the
/// Exchange floor: it traded at once, and what didn't fill came back.
pub const FLOOR_ONLY: &str =
    "WHAT DIDN'T FILL CAME BACK: AN ORDER WAITS ON THE BOOK ONLY FROM THE EXCHANGE FLOOR, ON CHARTER SQUARE";

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

/// One of the town's people knocked down by a pilot's car (`docs/FREE_ROAM.md`, the first of the
/// promoted): who (`bc_sim::colony::street::Person::id`), where they lie on the ring's floor
/// (across it, round it, m) and the way they faced, and the sector's tick they get up at. Till
/// then no client draws them walking.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Down {
    pub id: u32,
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    pub until: u32,
    /// Not lying there but gone from the street: a fare in a pilot's car.
    #[serde(default)]
    pub away: bool,
}

/// A car a pilot took from the ring's town's street and left about (`docs/FREE_ROAM.md`, P2's taken
/// set): where it stands on the ring's floor (across it, round it in the deck's `s`, m) and the way it
/// faces, where it came from (its look is that one's, on every screen: the kerb's car, or the
/// traffic's car `street::car` names), who left it (their session's id, as the plaza's datagrams name
/// them) and the sector's tick they did. Anyone may take it ([`Request::TakeLeft`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeftCar {
    pub id: u32,
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    /// From a kerb's bay (`bc_sim::colony::street::kerb`), or out of the traffic
    /// (`street::Car::id`): one of the two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kerb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub car: Option<u32>,
    pub by: u16,
    pub at: u32,
}

/// One of the ring's town's districts (`content::town::DISTRICTS`, 0 to 11) as the server holds what
/// has been taken out of its crowd: its people down (or away in a pilot's car), its kerbs whose cars
/// have been driven off (`bc_sim::colony::street::kerb`), and the cars left about in it; and its
/// version, which moves whenever any of them does.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakenDistrict {
    pub district: u16,
    pub version: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub down: Vec<Down>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kerbs: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<LeftCar>,
}

/// What a fare pays for each kilometre round the ring between its two squares, credits.
pub const FARE_PER_KM: u64 = 40;

/// The Watch's fine for a pilot caught wanted at `heat` (1 to 5), credits (`bc_sim::colony::watch`'s).
pub fn watch_fine(heat: u8) -> u64 {
    bc_sim::colony::watch::fine(heat)
}

/// Something the Watch is answering in the ring's town (`docs/FREE_ROAM.md`, 4; `docs/OPEN_CITY.md`,
/// "A huge deal"): where, how bad, what raised it, when, and who it's about. Everyone down in the
/// town is told them all ([`Update::Incidents`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    pub id: u32,
    /// Where on the ring's floor: across it, round it (the deck's own `s`), m.
    pub x: f32,
    pub s: f32,
    /// How bad, 1 to 5: the heat it rose to.
    pub level: u8,
    /// What raised it, last.
    #[serde(with = "offence_serde")]
    pub cause: Offence,
    /// The sector's ticks it was raised at and is over at (its last rise's time on from then).
    pub at: u32,
    pub until: u32,
    /// Who it's about: their session's id, as the plaza's datagrams name them (0: nobody's).
    #[serde(default)]
    pub offender: u16,
}

/// One of the Watch's units out in the ring's town (`docs/FREE_ROAM.md`, 4: the server's, sent out
/// of its stations by road): which (its number, the same as long as the server runs), what (a car,
/// a Sirius hound), where on the ring's floor (across it, round it: the deck's own `s`, m), the way
/// it faces (the walker's yaw: 0 faces −s, τ/4 faces +x) and what it's doing. Everyone down in the
/// town is told those near them ([`Update::Units`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub id: u16,
    #[serde(with = "unit_kind_serde")]
    pub kind: UnitKind,
    pub x: f32,
    pub s: f32,
    pub heading: f32,
    pub state: UnitState,
}

/// What one of the Watch's units is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    /// On its way to an incident, or after a wanted pilot.
    Driving,
    /// Holding a cordon round an incident's place.
    Cordon,
    /// Back to its station, where it's free again.
    Returning,
}

/// One of the ring's town's people out of its crowd (`docs/FREE_ROAM.md`, 3: the promoted tier, the
/// server's from something done by them till they've settled out of everyone's sight): who
/// (`bc_sim::colony::street::Person::id`: their look is still `street::seed_of`'s), where on the
/// ring's floor (across it, round it: the deck's own `s`, m), the way they face (the walker's yaw),
/// how fast they go (m/s: none, standing about) and whether they're running scared. Everyone down
/// in the town is told those near them ([`Update::Crowd`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Promoted {
    pub id: u32,
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    pub speed: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scared: bool,
}

/// One of the town's people out of its crowd further off ([`Update::Crowd`]'s middle tier): who,
/// across the ring in decimetres, round it in metres, and the way they face in 256ths of a turn.
pub type Placed = (u32, i16, u16, u8);

impl Promoted {
    /// As the middle tier has them.
    pub fn placed(&self) -> Placed {
        let turn = self.yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        (
            self.id,
            (self.x * 10.0).round().clamp(-32_767.0, 32_767.0) as i16,
            self.s.round().clamp(0.0, 65_535.0) as u16,
            ((turn * 256.0).round() as u32 % 256) as u8,
        )
    }

    /// One of the middle tier's, standing (it carries no more).
    pub fn from_placed((id, x, s, yaw): Placed) -> Promoted {
        let yaw = f32::from(yaw) / 256.0 * std::f32::consts::TAU;
        Promoted { id, x: f32::from(x) / 10.0, s: f32::from(s), yaw, speed: 0.0, scared: false }
    }
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
    /// On foot on the base's deck in the hub, out of the bay's airlock (`docs/RING.md`).
    Base,
    /// On foot (or driving) in the ring's town, down a spoke's lift (`docs/FREE_ROAM.md`).
    Town,
}

/// Where a pilot waking at home gets up: the home, and the spot beside its bed (dm from the home's
/// floor's −s −x corner, `content::homes::wake_spot`) on its storey (a house's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    pub home: u32,
    pub s: u16,
    pub x: u16,
    #[serde(default)]
    pub storey: u8,
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
        /// Into the city by waking at home (logged out asleep in a bed there, H3): where they get up.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wake: Option<Wake>,
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
    /// The held homes of the buildings near the pilot in the city (whenever the register or the
    /// buildings change): the rest are free, at the prices the city's own numbers give.
    Homes(HomesView),
    /// The pilot's body (survival): how fed they are, and for how long. On entering, after a meal,
    /// when the condition changes, and each minute on foot.
    Body(crate::body::BodyView),
    /// The ring's town's street as the server holds it (`docs/FREE_ROAM.md`, P2's taken set): what's
    /// been taken out of the crowd in the districts round the pilot (their own, by where the plaza
    /// has them, and the one either side: `near`; the client forgets any other), each of `districts`
    /// whole, sent when its version moves or the pilot comes near it; and the traffic's cars taken
    /// out of their lanes anywhere round the ring (a lane's places go all the way round), when that
    /// changes (absent: as last sent).
    Street {
        near: Vec<u16>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        districts: Vec<TakenDistrict>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lanes: Option<Vec<u32>>,
    },
    /// The fare in the pilot's car (the ring's town): the lift whose square they asked for, and
    /// what they pay there; none, set down (with what they paid) or lost (paying nothing).
    Fare {
        to: Option<u16>,
        pay: u64,
    },
    /// How much the Watch wants the pilot (`docs/OPEN_CITY.md`, "Heat and the Watch"), 0 to 5, as
    /// the server holds it: on entering and whenever it changes. Risen, what raised it.
    Heat {
        level: u8,
        #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_offence_serde")]
        cause: Option<Offence>,
    },
    /// Down lift `to`'s spoke into its square, across the hub from another's (`Request::RideAcross`):
    /// the pilot's first pose is out of its door.
    Across {
        to: u16,
    },
    /// What the Watch made of something the pilot did in the ring's town, each time (before the
    /// heat it leaves them at): the offence as it was taken (worse in the sight of its cars), and
    /// whether anybody saw it. Unseen, it added nothing (`docs/OPEN_CITY.md`: seen, not done).
    Watched {
        #[serde(with = "offence_serde")]
        what: Offence,
        seen: bool,
        /// Their level after it.
        level: u8,
    },
    /// The incidents the Watch is answering in the ring's town, to everyone down in it whenever
    /// they change.
    Incidents {
        incidents: Vec<Incident>,
    },
    /// The Watch's units out near the pilot in the ring's town, nearest first, a few times a second
    /// while any are (and once more, empty, when none are left).
    Units {
        units: Vec<Unit>,
    },
    /// The town's people out of its crowd round the pilot (`docs/FREE_ROAM.md`, 3, in tiers): those
    /// near them whole, nearest first; past those, out to as far as any screen draws the crowd, each
    /// placed in a few numbers; and how many more there are in the districts round them. A few times
    /// a second while there are any (and once more, empty, when there aren't): a screen draws its
    /// crowd's people but these where these are.
    Crowd {
        near: Vec<Promoted>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mid: Vec<Placed>,
        #[serde(default, skip_serializing_if = "is_zero")]
        far: u32,
    },
    /// The pilot's health on foot (`bc_sim::colony::brawl`), whenever it changes and on entering a
    /// floor: what's left of `brawl::MAX_HP`; how long till they're up if they're down, s (0: up);
    /// who did it, if somebody just did; and who's lifting them up, while somebody is.
    Hurt {
        hp: u8,
        #[serde(default, skip_serializing_if = "is_zero_u16")]
        down_s: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        helper: Option<String>,
    },
}

fn is_zero_u16(n: &u16) -> bool {
    *n == 0
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The held homes near a pilot, as they see them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomesView {
    pub homes: Vec<HomeLine>,
}

/// A held home, as a pilot near it sees it: whose (the name on its door), whether its door's open
/// and locked, and what the pilot may do there. Its panel's rent and lists are its crew's to see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeLine {
    pub id: u32,
    pub name: String,
    pub open: bool,
    pub locked: bool,
    /// The pilot's own: they hold it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    /// On its crew (full rights), or a guest (its door).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub crew: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub guest: bool,
    /// Its crew's to see: the rent on its panel (none for a home bought), how many are on its crew,
    /// and whether it has a main code and a guest code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelView>,
    /// Its furniture, as everyone who comes in sees it (what a chest or a safe holds, its crew's
    /// alone).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<crate::homes::Piece>,
    /// Who's asleep in its beds (logged out at home): each bed (its piece), and the name on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asleep: Vec<(u16, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelView {
    pub bank: Option<u64>,
    pub crew: u8,
    pub coded: bool,
    pub guest_coded: bool,
}

impl HomesView {
    /// The held homes among `ids`, as `trader` sees them.
    pub fn of(reg: &crate::homes::Homes, ids: impl Iterator<Item = u32>, trader: &str) -> Self {
        use crate::homes::Tenure;
        let homes = ids
            .filter_map(|id| {
                let h = reg.get(bc_sim::colony::homes::HomeId(id))?;
                let crew = h.lock.is_crew(trader);
                Some(HomeLine {
                    id,
                    name: h.name.clone(),
                    open: h.open,
                    locked: h.lock.locked,
                    mine: h.holder == trader,
                    crew,
                    guest: !crew && h.lock.guests.iter().any(|g| g == trader),
                    pieces: h
                        .pieces
                        .iter()
                        .map(|p| crate::homes::Piece {
                            holds: if crew { p.holds.clone() } else { Vec::new() },
                            ..p.clone()
                        })
                        .collect(),
                    asleep: h.asleep.iter().map(|a| (a.piece, a.name.clone())).collect(),
                    panel: crew.then(|| PanelView {
                        bank: match h.tenure {
                            Tenure::Lease { bank, .. } => Some(bank),
                            Tenure::Deed { .. } => None,
                        },
                        crew: h.lock.crew.len() as u8,
                        coded: h.lock.code.is_some(),
                        guest_coded: h.lock.guest_code.is_some(),
                    }),
                })
            })
            .collect();
        Self { homes }
    }
}

/// Someone in the city, by the session id the plaza's datagrams know them by.
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
        // The session's: the body is on the pilot's record (`body::eat_at_counter`).
        Request::Eat { .. } => return None,
        Request::Craft { item, batches } => hangar.craft(*item, *batches, now, rules),
        Request::CancelJob { station, index } => hangar.cancel_job(*station, *index, now),
        Request::Fit { item } => hangar.fit(*item),
        Request::Strip { slot } => hangar.strip(*slot),
        Request::Dismantle => hangar.dismantle(),
        Request::Repair { part } => hangar.repair(*part),
        Request::Overhaul { part } => hangar.overhaul(*part),
        Request::Scrap { item } => hangar.scrap(*item),
        Request::Fuel { grade } => hangar.fuel(*grade),
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
        | Request::EnterBase
        | Request::LeaveBase
        | Request::EnterTown
        | Request::LeaveTown
        | Request::RideAcross { .. }
        | Request::Stopped
        | Request::Offence { .. }
        | Request::Sighted
        | Request::Hit { .. }
        | Request::TakeFare { .. }
        | Request::DropFare
        | Request::TakeParked { .. }
        | Request::Jack { .. }
        | Request::LeaveCar { .. }
        | Request::TakeLeft { .. }
        | Request::WatchBoard { .. }
        | Request::UseKit { .. }
        | Request::Eject { .. }
        | Request::Say { .. }
        | Request::Strike { .. }
        | Request::RunDown { .. }
        | Request::Help { .. } => return None,
        // The register's (`apply_homes`).
        Request::Rent { .. }
        | Request::Buy { .. }
        | Request::GiveUp { .. }
        | Request::EnterCode { .. }
        | Request::Door { .. }
        | Request::Knock { .. }
        | Request::SetCode { .. }
        | Request::Lock { .. }
        | Request::LeaveCrew { .. }
        | Request::ClearCrew { .. }
        | Request::Feed { .. }
        | Request::TakeRent { .. }
        | Request::Furnish { .. }
        | Request::MovePiece { .. }
        | Request::SellPiece { .. }
        | Request::Stash { .. }
        | Request::Unstash { .. } => return None,
    })
}

/// Does what `req` asks of the register of homes, if it's the register's: `None` otherwise. The
/// caller has checked where the pilot is (`Request::home`). `name` is what they go by.
pub fn apply_homes(
    req: &Request,
    hangar: &mut Hangar,
    homes: &mut crate::homes::Homes,
    trader: &str,
    name: &str,
    now: u64,
) -> Option<Done> {
    use crate::homes::Entered;
    use bc_sim::colony::homes::{HomeId, home};
    let (id, _) = req.home()?;
    let Some(h) = home(id) else { return Some(Err("there's no home there".into())) };
    let addr = crate::homes::address(&h);
    Some(match *req {
        Request::Rent { weeks, .. } => homes.rent(&h, trader, name, weeks, hangar, now),
        Request::Buy { .. } => homes.buy(&h, trader, name, hangar, now),
        Request::GiveUp { .. } => homes.give_up(id, trader, hangar),
        Request::EnterCode { code, .. } => match homes.enter_code(id, trader, code, now) {
            Ok(Entered::Crew) => Ok(format!("{addr} · THE CODE IS RIGHT · ON ITS CREW")),
            Ok(Entered::Guest) => Ok(format!("{addr} · THE GUEST CODE · ITS DOOR WORKS FOR YOU")),
            Ok(Entered::Wrong { shock, shut: false }) => {
                Err(format!("wrong code: the keypad shocks you ({shock})"))
            }
            Ok(Entered::Wrong { shut: true, .. }) => {
                Err("wrong code: the keypad shuts for 15 minutes".into())
            }
            Ok(Entered::Shut) => Err("the keypad is shut".into()),
            Err(e) => Err(e),
        },
        Request::Door { open, .. } => homes.door(id, trader, open).map(|s| format!("{addr} · {s}")),
        Request::Knock { .. } => homes.knock(id, name),
        Request::SetCode { guest, code, .. } => homes.set_code(id, trader, guest, code),
        Request::Lock { locked, .. } => homes.set_locked(id, trader, locked),
        Request::LeaveCrew { .. } => homes.leave_crew(id, trader),
        Request::ClearCrew { .. } => homes.clear_crew(id, trader),
        Request::Feed { credits, .. } => homes.feed(id, credits, hangar),
        Request::TakeRent { credits, .. } => homes.withdraw(id, trader, credits, hangar),
        Request::Furnish { ref furniture, s, x, turn, storey, .. } => {
            match bc_sim::content::homes::Furniture::from_slug(furniture) {
                Some(kind) => homes.furnish(id, trader, kind, (s, x), turn, storey, hangar),
                None => Err("there's no such furniture".into()),
            }
        }
        Request::MovePiece { piece, s, x, turn, storey, .. } => {
            homes.move_piece(id, trader, piece, (s, x), turn, storey)
        }
        Request::SellPiece { piece, .. } => homes.sell_piece(id, trader, piece, hangar),
        Request::Stash { piece, item, qty, .. } => homes.stash(id, trader, piece, item, qty, hangar),
        Request::Unstash { piece, item, qty, .. } => homes.unstash(id, trader, piece, item, qty, hangar),
        _ => {
            let _: HomeId = id;
            return None;
        }
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
mod offence_serde {
    use bc_sim::colony::watch::Offence;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(o: &Offence, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(o.slug())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Offence, D::Error> {
        let s = String::deserialize(d)?;
        Offence::from_slug(&s).ok_or_else(|| serde::de::Error::custom(format!("no such offence: {s}")))
    }
}

mod unit_kind_serde {
    use bc_sim::colony::watch::UnitKind;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(k: &UnitKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(k.slug())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<UnitKind, D::Error> {
        let s = String::deserialize(d)?;
        UnitKind::from_slug(&s).ok_or_else(|| serde::de::Error::custom(format!("no such unit: {s}")))
    }
}

mod opt_offence_serde {
    use bc_sim::colony::watch::Offence;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(o: &Option<Offence>, s: S) -> Result<S::Ok, S::Error> {
        match o {
            Some(o) => s.serialize_some(o.slug()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Offence>, D::Error> {
        match Option::<String>::deserialize(d)? {
            None => Ok(None),
            Some(s) => Offence::from_slug(&s)
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom(format!("no such offence: {s}"))),
        }
    }
}

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
            (r#"{"t":"fuel","grade":"refined"}"#, Request::Fuel { grade: Grade::Refined }),
            (r#"{"t":"fuel","grade":"ultra"}"#, Request::Fuel { grade: Grade::UltraPure }),
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
                r#"{"t":"watch","item":"part.argo.torso"}"#,
                Request::Watch { item: Some(Item::Part(FrameId::Argo, Part::Torso)) },
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
            (r#"{"t":"eat","dish":"noodles"}"#, Request::Eat { dish: crate::food::Dish::Noodles }),
            (r#"{"t":"dock"}"#, Request::Dock),
            (r#"{"t":"use_kit","kit":"chaff"}"#, Request::UseKit { kit: Kit::Chaff }),
            (r#"{"t":"eject"}"#, Request::Eject { destruct: false }),
            (r#"{"t":"eject","destruct":true}"#, Request::Eject { destruct: true }),
            (r#"{"t":"say","text":"o7"}"#, Request::Say { text: "o7".into() }),
            (r#"{"t":"stopped"}"#, Request::Stopped),
            (r#"{"t":"offence","what":"took_a_car"}"#, Request::Offence { what: Offence::TookACar }),
            (r#"{"t":"sighted"}"#, Request::Sighted),
            (r#"{"t":"take_parked","kerb":131077}"#, Request::TakeParked { kerb: 131_077 }),
            (r#"{"t":"jack","car":65539}"#, Request::Jack { car: 65_539 }),
            (
                r#"{"t":"leave_car","x_cm":-4210,"s_cm":1100250,"yaw_mrad":3142}"#,
                Request::LeaveCar { x_cm: -4_210, s_cm: 1_100_250, yaw_mrad: 3_142 },
            ),
            (r#"{"t":"take_left","id":9}"#, Request::TakeLeft { id: 9 }),
        ];
        for (json, req) in cases {
            assert_eq!(decode::<Request>(json.as_bytes()).as_ref(), Some(req), "{json}");
            assert_eq!(decode::<Request>(&encode(req)).as_ref(), Some(req));
        }
        assert_eq!(decode::<Request>(br#"{"t":"fit","item":"part.cetus.head"}"#), None);
        assert_eq!(decode::<Request>(b"not json"), None);
        assert_eq!(decode::<Request>(br#"{"t":"offence","what":"loitering"}"#), None);
        // The old fine, with the heat on the client's word, is no request now.
        assert_eq!(decode::<Request>(br#"{"t":"fine","heat":0}"#), None);
    }

    /// Each desk in the city does its one thing (the colony open), and any terminal the rest:
    /// every request, one of each.
    #[test]
    fn each_desk_does_its_one_thing_and_any_terminal_the_rest() {
        use PlaceKind::{Bar, Charter, Exchange as Floor, Proving};
        let steel = Item::Material(Material::Steel);
        let order = |rest| Request::Order { item: steel, side: Side::Sell, price: 1, qty: 1, rest };
        let cases: Vec<(Request, Option<PlaceKind>)> = vec![
            (order(true), Some(Floor)),
            (order(false), None),
            (Request::Post { item: steel, qty: 10, reward: 50, hours: 1 }, Some(Charter)),
            (Request::TakePatrol { id: 1 }, Some(Charter)),
            (Request::Sign, Some(Charter)),
            (Request::AceTerms { salvage: true }, Some(Bar)),
            (Request::AceTerms { salvage: false }, Some(Bar)),
            (Request::Eat { dish: crate::food::Dish::Noodles }, Some(Bar)),
            (Request::Trainer { build: crate::proving::Trainer::Board }, Some(Proving)),
            (Request::Craft { item: steel, batches: 1 }, None),
            (Request::CancelJob { station: Station::Foundry, index: 0 }, None),
            (Request::Fit { item: steel }, None),
            (Request::Strip { slot: Slot::Mount { mount: 0 } }, None),
            (Request::Dismantle, None),
            (Request::Repair { part: None }, None),
            (Request::Overhaul { part: None }, None),
            (Request::Scrap { item: steel }, None),
            (Request::Fuel { grade: Grade::Standard }, None),
            (Request::CancelOrder { id: 1 }, None),
            (Request::Watch { item: None }, None),
            (Request::Launch, None),
            (Request::LaunchInside, None),
            (Request::Dock, None),
            (Request::EnterCity { strip: 0 }, None),
            (Request::LeaveCity, None),
            (Request::EnterBase, None),
            (Request::LeaveBase, None),
            (Request::EnterTown, None),
            (Request::LeaveTown, None),
            (Request::RideAcross { to: 3 }, None),
            (Request::Stopped, None),
            (Request::Offence { what: Offence::Knock }, None),
            (Request::Sighted, None),
            (Request::Hit { person: 7 }, None),
            (Request::TakeFare { person: 7 }, None),
            (Request::DropFare, None),
            (Request::TakeParked { kerb: 1 }, None),
            (Request::Jack { car: 1 }, None),
            (Request::LeaveCar { x_cm: 0, s_cm: 0, yaw_mrad: 0 }, None),
            (Request::TakeLeft { id: 1 }, None),
            (Request::BoardTrainer, None),
            (Request::Withdraw { id: 1 }, None),
            (Request::Deliver { id: 1, qty: 1 }, None),
            (Request::DropPatrol { id: 1 }, None),
            (Request::Contribute { work: Work::SecondFoundry, item: steel, qty: 1 }, None),
            (Request::WatchBoard { on: true }, None),
            (Request::UseKit { kit: Kit::Chaff }, None),
            (Request::Eject { destruct: false }, None),
            (Request::Say { text: "o7".into() }, None),
        ];
        for (req, desk) in &cases {
            assert_eq!(req.desk(), *desk, "{req:?}");
        }
        let tags: std::collections::BTreeSet<String> = cases
            .iter()
            .map(|(r, _)| serde_json::to_value(r).unwrap()["t"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(tags.len(), 47, "every request: {tags:?}");
        // Each refusal says where to go, as the server's notes and the e2e read them.
        for (k, place) in [
            (Floor, "the exchange floor's counter"),
            (Charter, "the charter board's desk"),
            (Bar, "the arrival's counter"),
            (Proving, "the proving ground's desk"),
            (PlaceKind::HubGate, "hub gate"),
        ] {
            assert!(desk_refusal(k).contains(place), "{k:?}: {}", desk_refusal(k));
        }
        assert!(desk_refusal(Bar).contains("meals"), "The Arrival's counter serves them too");
        assert_eq!(FLOOR_ONLY, FLOOR_ONLY.to_uppercase());
    }

    #[test]
    fn a_place_says_when_its_a_trainer_and_the_proving_grounds_board_goes_round() {
        // An old page's place, and one that isn't a trainer's, say nothing of trainers.
        let city = Update::Place { place: Place::City, bay: 3, strip: Some(0), trainer: false, wake: None };
        assert!(!String::from_utf8(encode(&city)).unwrap().contains("trainer"));
        let back: Update = decode(br#"{"t":"place","place":"space","bay":3}"#).unwrap();
        assert_eq!(
            back,
            Update::Place { place: Place::Space, bay: 3, strip: None, trainer: false, wake: None }
        );
        // Out on Bay Row: the page reads it as `base`.
        let base = Update::Place { place: Place::Base, bay: 3, strip: None, trainer: false, wake: None };
        assert!(String::from_utf8(encode(&base)).unwrap().contains(r#""place":"base""#));
        let flying = Update::Place { place: Place::Space, bay: 3, strip: None, trainer: true, wake: None };
        assert_eq!(decode::<Update>(&encode(&flying)), Some(flying));
        let mut b = crate::proving::Board::default();
        b.record(crate::proving::Feat::Course, "me", "Kai", 118_000, 1_000_000);
        let board = Update::Proving(b.view("me", 1_000_000));
        assert_eq!(decode::<Update>(&encode(&board)), Some(board));
    }

    #[test]
    fn the_watchs_heat_and_incidents_go_round() {
        let cold = Update::Heat { level: 0, cause: None };
        assert_eq!(String::from_utf8(encode(&cold)).unwrap(), r#"{"t":"heat","level":0}"#);
        assert_eq!(decode::<Update>(&encode(&cold)), Some(cold));
        let hot = Update::Heat { level: 3, cause: Some(Offence::HitSomeone) };
        let json = String::from_utf8(encode(&hot)).unwrap();
        assert_eq!(json, r#"{"t":"heat","level":3,"cause":"hit_someone"}"#);
        assert_eq!(decode::<Update>(json.as_bytes()), Some(hot));
        let unseen = Update::Watched { what: Offence::TookACar, seen: false, level: 0 };
        let json = String::from_utf8(encode(&unseen)).unwrap();
        assert_eq!(json, r#"{"t":"watched","what":"took_a_car","seen":false,"level":0}"#);
        assert_eq!(decode::<Update>(json.as_bytes()), Some(unseen));
        let one = Incident {
            id: 7,
            x: -120.5,
            s: 11_002.25,
            level: 4,
            cause: Offence::TookTheWatchs,
            at: 9_000,
            until: 14_400,
            offender: 3,
        };
        let all = Update::Incidents { incidents: vec![one, Incident { id: 8, offender: 0, ..one }] };
        let json = String::from_utf8(encode(&all)).unwrap();
        assert!(json.contains(r#""cause":"took_the_watchs""#), "{json}");
        assert_eq!(decode::<Update>(json.as_bytes()), Some(all));
        // Nobody's: the offender left out reads as 0.
        let bare = r#"{"t":"incidents","incidents":[{"id":1,"x":0.0,"s":5.0,"level":5,"cause":"reckless","at":1,"until":2}]}"#;
        let Some(Update::Incidents { incidents }) = decode::<Update>(bare.as_bytes()) else { panic!() };
        assert_eq!((incidents[0].offender, incidents[0].cause), (0, Offence::Reckless));
        assert_eq!((watch_fine(1), watch_fine(3), watch_fine(7)), (150, 1_350, 3_750));
    }

    #[test]
    fn the_streets_taken_set_goes_round() {
        let down = Down { id: 0x51, x: -40.5, s: 10_995.5, yaw: 1.5, until: 900, away: false };
        let left =
            LeftCar { id: 3, x: 12.0, s: 11_020.0, yaw: 0.0, kerb: Some(77), car: None, by: 2, at: 450 };
        let one = TakenDistrict {
            district: 0,
            version: 4,
            down: vec![down],
            kerbs: vec![77, 78],
            left: vec![left],
        };
        let empty = TakenDistrict { district: 11, version: 1, ..TakenDistrict::default() };
        let all =
            Update::Street { near: vec![11, 0, 1], districts: vec![one, empty], lanes: Some(vec![65_539]) };
        let json = String::from_utf8(encode(&all)).unwrap();
        assert!(json.contains(r#"{"district":11,"version":1}"#), "an empty district is short: {json}");
        assert!(json.contains(r#""kerb":77,"by":2"#) && !json.contains(r#""car":null"#), "{json}");
        assert_eq!(decode::<Update>(json.as_bytes()), Some(all));
        // Nothing changed but where they are: only which districts are theirs now.
        let moved = Update::Street { near: vec![0, 1, 2], districts: vec![], lanes: None };
        assert_eq!(String::from_utf8(encode(&moved)).unwrap(), r#"{"t":"street","near":[0,1,2]}"#);
        assert_eq!(decode::<Update>(&encode(&moved)), Some(moved));
    }

    #[test]
    fn the_watchs_units_go_round() {
        let car = Unit {
            id: 12,
            kind: UnitKind::Car,
            x: 29.0,
            s: 10_500.5,
            heading: 1.5,
            state: UnitState::Driving,
        };
        let hound = Unit { id: 13, kind: UnitKind::Hound, state: UnitState::Cordon, ..car };
        let all = Update::Units { units: vec![car, hound] };
        let json = String::from_utf8(encode(&all)).unwrap();
        assert!(json.starts_with(r#"{"t":"units","units":[{"id":12,"kind":"car","#), "{json}");
        assert!(json.contains(r#""kind":"hound""#) && json.contains(r#""state":"cordon""#), "{json}");
        assert_eq!(decode::<Update>(json.as_bytes()), Some(all));
        let none = Update::Units { units: vec![] };
        assert_eq!(String::from_utf8(encode(&none)).unwrap(), r#"{"t":"units","units":[]}"#);
        let odd = r#"{"t":"units","units":[{"id":1,"kind":"tank","x":0.0,"s":0.0,"heading":0.0,"state":"driving"}]}"#;
        assert_eq!(decode::<Update>(odd.as_bytes()), None);
    }

    #[test]
    fn the_body_goes_round() {
        let body = Update::Body(crate::body::BodyView {
            fed: bc_sim::content::body::Fed::Peckish,
            food_s: 2_400,
            well_fed_s: 0,
        });
        let json = String::from_utf8(encode(&body)).unwrap();
        assert_eq!(json, r#"{"t":"body","fed":"peckish","food_s":2400,"well_fed_s":0}"#);
        assert_eq!(decode::<Update>(json.as_bytes()), Some(body));
    }

    #[test]
    fn a_fight_on_foot_goes_round() {
        let whole = Update::Hurt { hp: 100, down_s: 0, by: None, helper: None };
        assert_eq!(String::from_utf8(encode(&whole)).unwrap(), r#"{"t":"hurt","hp":100}"#);
        let down = Update::Hurt { hp: 0, down_s: 25, by: Some("Kai".into()), helper: Some("Ines".into()) };
        let json = String::from_utf8(encode(&down)).unwrap();
        assert_eq!(json, r#"{"t":"hurt","hp":0,"down_s":25,"by":"Kai","helper":"Ines"}"#);
        assert_eq!(decode::<Update>(json.as_bytes()), Some(down));
        for req in [
            Request::Strike { pilot: 7, x_cm: -1_250, s_cm: 1_100_025, yaw_mrad: 1_571 },
            Request::RunDown { pilot: 9 },
            Request::Help { pilot: 9, on: true },
        ] {
            assert_eq!(decode::<Request>(&encode(&req)), Some(req.clone()));
            assert_eq!(req.desk(), None);
            assert!(req.home().is_none());
        }
        let strike: Request =
            decode(br#"{"t":"strike","pilot":3,"x_cm":10,"s_cm":20,"yaw_mrad":0}"#).unwrap();
        assert_eq!(strike, Request::Strike { pilot: 3, x_cm: 10, s_cm: 20, yaw_mrad: 0 });
        // The Watch's word of a blow, by its slug.
        let seen = Update::Watched { what: Offence::Assault, seen: true, level: 1 };
        assert!(String::from_utf8(encode(&seen)).unwrap().contains(r#""what":"assault""#));
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
            h.stores.add(Item::Part(FrameId::Altair, Part::ArmL), 1);
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
