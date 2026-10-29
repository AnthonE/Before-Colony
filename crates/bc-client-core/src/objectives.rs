//! Objectives: what a pilot can set out to do in the sector and the colony. The HUD shows one at a
//! time with a waypoint to fly to (◆), and the map (M) lists them all. They're the Charter Board's
//! first jobs for a new Arrival, after `docs/TRAINING.md`'s first ten minutes: with the colony open,
//! ride the cap lift down, report to the Proving Ground and earn a flight certificate there, then a
//! straight walk down the avenue to the Exchange floor and the Charter Board's desk, standing with
//! the colony, and home to arm the suit with the Board's beam rifle. Then out: land on the resource
//! satellite and hide in its well, mine ore and bring it home, sell it, down a drone, reach
//! Hermit, down five, read the Almagest's Most Wanted at The Arrival and down one of its aces, try a new
//! Lodestar on the test range, and make siderium.
//!
//! They can be done in any order: each is checked every frame, and what's done is kept with the
//! settings (a bit per [`Objective`], and how many drones the pilot has downed), so it carries over
//! from one visit to the next in the same browser. Which one the HUD shows depends on where the
//! pilot is ([`Here`]): flying, the first not done that's done in space (so a pilot who launches
//! first is never pulled home while one is open); on foot, the first not done in the rules' order,
//! passing over one in space the pilot has flown past, and back to the Charter Board's desk while
//! the Board holds something for them there. Each has a [`Realm`], where it's done, and its
//! how-line begins with the way there from where the pilot is ([`Objective::how_from`]).
//!
//! Everything they're checked against is what the client already knows: where the suit stands and
//! what it holds, its cover, the dock's flag and its kills, and the server's word on the hangar,
//! the Charter Board and the Proving Ground. None of it is a reward: bounties, sales, the
//! certificate's rifle are the server's, as ever. These point the way to them.

use bc_proto::FrameId;
use bc_sim::bodies::Body;
use bc_sim::content::aces::ACES;
use bc_sim::content::city::{PLACES, PlaceKind};
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::field::FIELD_CENTER;
use glam::Vec3;

use crate::ClientCore;
use crate::bay::Lift;

/// Ore in the hold that counts as having mined, kg.
pub const MINE_KG: u32 = 200;
/// Drones downed that make a name (and the Almagest's aces are next).
pub const FIVE_DRONES: u32 = 5;
/// A rock this big (its smallest half-axis, m) is the miner's waypoint: it can be landed on, and
/// holds tonnes.
const BIG_ROCK: f32 = 10.0;

/// One objective. Its bit in the done set is its discriminant: new ones go at the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objective {
    /// Land on the resource satellite (the first landmark).
    LandStation,
    /// Hide in its well.
    Hide,
    /// Mine [`MINE_KG`] of ore.
    Mine,
    /// Bring it to the dock (survival: home; arcade: sell it).
    Deliver,
    /// Down a drone.
    Bounty,
    /// Land on Hermit (the second landmark).
    Hermit,
    /// Down [`FIVE_DRONES`] drones.
    FiveDrones,
    /// Ride a cap lift down from the bays into the colony (the colony open).
    RideDown,
    /// Find the Colony Exchange's floor in the city.
    ExchangeFloor,
    /// Sell something on the Exchange.
    Sell,
    /// Report to the Proving Ground, the Blast Hall off Hub Gate's square (`docs/TRAINING.md`).
    ProvingGround,
    /// Earn a flight certificate there: a time on its board, the drill's or the course's (the
    /// server's word). The Charter Board trusts a certified pilot with a beam rifle.
    Certify,
    /// Report to the Charter Board's desk, in its hall on Charter Square.
    CharterDesk,
    /// Earn standing with the colony: deliver to its contracts or its great works.
    Standing,
    /// Arm the bay's suit: every weapon its frame carries fitted (the worn Argo an Arrival is handed
    /// came without its beam rifle).
    Arm,
    /// Read the Almagest's Most Wanted, pinned up behind the bar at The Arrival.
    MostWanted,
    /// Down one of the Almagest's aces (the Charter Board's count).
    AlmagestAce,
    /// Fly a new Lodestar off the test range, from the Blast Hall's gantry.
    TryPrototype,
    /// Make siderium (or have it in the making) at the colony's zero-G foundry.
    Siderium,
}

/// Where an objective is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Realm {
    /// Flying in the sector.
    Space,
    /// On foot in the pilot's own bay.
    Bay,
    /// On foot in the colony's city.
    City,
    /// On foot at any terminal: in the bay, or at a desk in the city.
    Afoot,
}

/// Where the pilot is, for the objectives: flying (or not yet anywhere), on foot in their bay, or
/// on foot on strip `n` of the city.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Here {
    #[default]
    Space,
    Bay,
    City(u8),
}

/// Where an objective's waypoint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waypoint {
    Landmark(u8),
    /// A landmark's hide spot.
    HideSpot(u8, u8),
    /// The nearest big rock.
    Rock,
    Dock,
    /// The nearest drone in sight, else their patrols over the field.
    Drones,
    /// The nearest of the Almagest's aces in sight (by its name on the roster), else as [`Waypoint::Drones`].
    Ace,
    /// A place in the colony's city (an index into `content::city::PLACES`): flying, the way
    /// home to the dock comes first; on foot, its door.
    Place(u8),
}

impl Objective {
    pub const ALL: [Objective; 19] = [
        Objective::LandStation,
        Objective::Hide,
        Objective::Mine,
        Objective::Deliver,
        Objective::Bounty,
        Objective::Hermit,
        Objective::FiveDrones,
        Objective::RideDown,
        Objective::ExchangeFloor,
        Objective::Sell,
        Objective::ProvingGround,
        Objective::Certify,
        Objective::CharterDesk,
        Objective::Standing,
        Objective::Arm,
        Objective::MostWanted,
        Objective::AlmagestAce,
        Objective::TryPrototype,
        Objective::Siderium,
    ];

    /// Survival's order: `docs/TRAINING.md`'s first ten minutes, a new Arrival on foot in their bay
    /// with a worn Argo: down the cap lift, and a straight walk along the avenue from Hub Gate past
    /// the Blast Hall to the Exchange floor and the Charter Board's hall; then home to arm the suit,
    /// and out. The ore brought home is sold; then the drones, and their aces.
    const SURVIVAL: [Objective; 19] = [
        Objective::RideDown,
        Objective::ProvingGround,
        Objective::Certify,
        Objective::ExchangeFloor,
        Objective::CharterDesk,
        Objective::Standing,
        Objective::Arm,
        Objective::LandStation,
        Objective::Hide,
        Objective::Mine,
        Objective::Deliver,
        Objective::Sell,
        Objective::Bounty,
        Objective::Hermit,
        Objective::FiveDrones,
        Objective::MostWanted,
        Objective::AlmagestAce,
        Objective::TryPrototype,
        Objective::Siderium,
    ];
    /// Arcade's: a pilot launches by the field, in whatever suit they like, among the drones (and
    /// the colony is closed: what's the colony's or the bay's doesn't show).
    const ARCADE: [Objective; 19] = [
        Objective::Bounty,
        Objective::Mine,
        Objective::Deliver,
        Objective::LandStation,
        Objective::Hide,
        Objective::Hermit,
        Objective::FiveDrones,
        Objective::RideDown,
        Objective::ProvingGround,
        Objective::ExchangeFloor,
        Objective::Sell,
        Objective::Certify,
        Objective::CharterDesk,
        Objective::Standing,
        Objective::Arm,
        Objective::MostWanted,
        Objective::AlmagestAce,
        Objective::TryPrototype,
        Objective::Siderium,
    ];

    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    /// In the order the rules show them.
    pub fn order(survival: bool) -> &'static [Objective; 19] {
        if survival { &Self::SURVIVAL } else { &Self::ARCADE }
    }

    /// Whether the sector has what it needs (a landmark, rocks, the colony open, a hangar).
    pub fn available(self, i: &ObjectiveInput) -> bool {
        match self {
            Objective::LandStation | Objective::Hide => i.landmarks >= 1,
            Objective::Hermit => i.landmarks >= 2,
            Objective::Mine => i.rocks,
            Objective::RideDown
            | Objective::ProvingGround
            | Objective::ExchangeFloor
            | Objective::Certify
            | Objective::CharterDesk
            | Objective::MostWanted
            | Objective::TryPrototype => i.colony,
            // The bay's terminals have the Exchange's tab, the colony open or not.
            Objective::Sell
            | Objective::Standing
            | Objective::Arm
            | Objective::AlmagestAce
            | Objective::Siderium => i.survival,
            Objective::Deliver | Objective::Bounty | Objective::FiveDrones => true,
        }
    }

    /// Where it's done.
    pub fn realm(self) -> Realm {
        match self {
            Objective::LandStation
            | Objective::Hide
            | Objective::Mine
            | Objective::Deliver
            | Objective::Bounty
            | Objective::Hermit
            | Objective::FiveDrones
            | Objective::AlmagestAce => Realm::Space,
            Objective::RideDown | Objective::Arm | Objective::Siderium => Realm::Bay,
            Objective::ProvingGround
            | Objective::Certify
            | Objective::ExchangeFloor
            | Objective::CharterDesk
            | Objective::MostWanted
            | Objective::TryPrototype => Realm::City,
            Objective::Sell | Objective::Standing => Realm::Afoot,
        }
    }

    /// The HUD's line: terse, all caps.
    pub fn title(self, survival: bool) -> String {
        let station = LANDMARKS[0].name;
        match self {
            Objective::LandStation => format!("LAND ON {station}"),
            Objective::Hide => format!("HIDE IN THE {}", LANDMARKS[0].hides[0].name),
            Objective::Mine => format!("MINE {MINE_KG} KG OF ORE"),
            Objective::Deliver if survival => "BRING THE ORE HOME".into(),
            Objective::Deliver => "SELL ORE AT THE DOCK".into(),
            Objective::Bounty => format!("DOWN A {}", drone().to_uppercase()),
            Objective::Hermit => format!("LAND ON {}", LANDMARKS[1].name),
            Objective::FiveDrones => format!("DOWN {FIVE_DRONES} {}S", drone().to_uppercase()),
            Objective::RideDown => "RIDE THE CAP LIFT DOWN".into(),
            Objective::ExchangeFloor => "FIND THE EXCHANGE FLOOR".into(),
            Objective::Sell => "SELL ON THE EXCHANGE".into(),
            Objective::ProvingGround => "REPORT TO THE PROVING GROUND".into(),
            Objective::Certify => "EARN A FLIGHT CERTIFICATE".into(),
            Objective::CharterDesk => "REPORT TO THE CHARTER BOARD".into(),
            Objective::Standing => "EARN STANDING WITH THE COLONY".into(),
            Objective::Arm => "ARM YOUR SUIT".into(),
            Objective::MostWanted => "READ THE MOST WANTED".into(),
            Objective::AlmagestAce => "DOWN ONE OF THE ALMAGEST'S ACES".into(),
            Objective::TryPrototype => format!("TRY A NEW {} ON THE TEST RANGE", proto().to_uppercase()),
            Objective::Siderium => "MAKE SIDERIUM".into(),
        }
    }

    /// How to go about it: one plain line, under survival or arcade rules, the colony open or not.
    pub fn how(self, survival: bool, colony: bool) -> String {
        let station = LANDMARKS[0].name;
        match self {
            Objective::LandStation => {
                format!("Fly to {station}. L arms the grip: come in slow and close, and it lands you.")
            }
            Objective::Hide => format!(
                "Land in the bowl in {station}'s aft face, then crouch (C) and keep still until sensors lose you."
            ),
            Objective::Mine => {
                "Strike a rock with your blade (F), grab the ore that comes off (G) and stow it (B).".into()
            }
            Objective::Deliver if survival => {
                "Come to rest inside the dock's ring of lights and press Enter: the ore goes to your stores."
                    .into()
            }
            Objective::Deliver => {
                "Come into the dock's ring of lights under 25 m/s: it buys your hold.".into()
            }
            Objective::Bounty => format!(
                "The Consortium's {}s patrol over the field. The Charter Board pays a bounty for each.",
                drone()
            ),
            Objective::Hermit => format!(
                "The big asteroid, 15 km out past the field. Its craters hide a suit: {}.",
                LANDMARKS[1].hides.iter().map(|h| h.name).collect::<Vec<_>>().join(", ")
            ),
            Objective::FiveDrones => {
                "Every one is a bounty paid, and counts toward a patrol you hold. Five, and the Almagest's aces are next."
                    .into()
            }
            Objective::RideDown => {
                "Home in your bay, walk to the airlock and press E: the cap lift rides down the end cap into the colony."
                    .into()
            }
            Objective::ExchangeFloor => {
                "From Hub Gate, along the avenue: M shows the city, ◆ marks the Exchange's door. Its floor is where an order at your own price waits on the book."
                    .into()
            }
            Objective::Sell if !colony => {
                "At your bay's terminals, the EXCHANGE tab: sell what you brought home to the colony's desk or to another pilot."
                    .into()
            }
            Objective::Sell => {
                "At any terminal's EXCHANGE tab, your bay's or the Exchange floor's: sell what you brought home to the colony's desk or to another pilot."
                    .into()
            }
            Objective::ProvingGround => {
                "The Blast Hall, off Hub Gate's square: M shows the city, ◆ marks its blast doors. Its desk (E) has the board; its gantry, a trainer to fly."
                    .into()
            }
            Objective::Certify => format!(
                "In the Blast Hall, the gantry is the amber ring left of the blast doors: E at its hatch boards one of the Board's {}s. Clear the drill (strike the target pulsing cyan) or fly the course from the inner gate. Any time the Board takes is a certificate, and it trusts a certified pilot with a beam rifle.",
                argo()
            ),
            Objective::CharterDesk => {
                "Its hall is on Charter Square, past the Exchange floor: M shows the city, ◆ marks its door. What the Board holds for you is paid out at its desk (E), and contracts are posted, patrols taken and the charter signed there."
                    .into()
            }
            // What the works still need is the board's to say: the colony's needs are met in time,
            // and once the charter's signed its works are done.
            Objective::Standing => {
                "Deliver to one of the colony's contracts or a great work, at the Charter Board's desk or its tab on your bay's terminals: the board lists what each still needs, and while a great work is short of steel the 200 kg you came with will do. Paid over its value, and in standing."
                    .into()
            }
            Objective::Arm if colony => format!(
                "Your {} came without its beam rifle. At the suit's console in your bay (E at its feet), fit one from your stores: the Charter Board's for a flight certificate (paid out at its desk, on Charter Square), or one bought on the Exchange or built at the fabricator.",
                argo()
            ),
            Objective::Arm => format!(
                "Your {} came without its beam rifle. At the suit's console in your bay (E at its feet), fit one from your stores: one bought on the Exchange or built at the fabricator.",
                argo()
            ),
            Objective::MostWanted => {
                "The Almagest's aces are pinned up behind the bar at The Arrival, off Charter Square (E at its counter): who's out, the bounty on each, and your terms for it."
                    .into()
            }
            Objective::AlmagestAce => format!(
                "One flies among the {}s at a time, its callsign on your lock (Y), its ◆ on your HUD. The Charter Board pays its bounty; or, on your terms, the tugs bring its wreck home.",
                drone()
            ),
            Objective::TryPrototype => format!(
                "At the Proving Ground's desk, pick a new {} on THE TEST RANGE and the gantry readies it: board it at the hatch (E) and fly it. Nothing of yours is at stake, and it's what you're building towards.",
                proto()
            ),
            Objective::Siderium => {
                "Only the colony's zero-G foundry makes it, from titanium alloy and exotics, for a fee: queue it at your bay's fabricator. Exotics are in the field's rarest rocks."
                    .into()
            }
        }
    }

    /// How to go about it on foot on strip `strip` of the city (`None`: not down there):
    /// [`Objective::how`], unless its place is on another strip. Then the way there: back up to the
    /// bay, and down that strip's cap lift.
    pub fn how_on(self, survival: bool, colony: bool, strip: Option<u8>) -> String {
        let place = match self.waypoint() {
            Waypoint::Place(k) => PLACES.get(usize::from(k)),
            _ => None,
        };
        match (place, strip) {
            (Some(p), Some(here)) if p.strip != here => {
                let lift = Lift::to(p.strip);
                let down = if lift == Lift::default() {
                    format!("from your airlock E rides lift {} down to it", lift.number())
                } else {
                    format!("at your airlock Q chooses lift {}, and E rides it down", lift.number())
                };
                format!(
                    "It's on the {} strip: ride the lift up from Hub Gate here (E), and {down}.",
                    word(lift.name())
                )
            }
            _ => self.how(survival, colony),
        }
    }

    /// The panel's how-line from where the pilot is: [`Objective::how`] where it's done (on
    /// another strip of the city, [`Objective::how_on`]'s way there), else the way there first.
    pub fn how_from(self, survival: bool, colony: bool, here: Here) -> String {
        let first = match (self.realm(), here) {
            (Realm::Space, Here::Space) | (Realm::Bay | Realm::Afoot, Here::Bay) => {
                return self.how(survival, colony);
            }
            (Realm::City | Realm::Afoot, Here::City(strip)) => {
                return self.how_on(survival, colony, Some(strip));
            }
            (Realm::City, Here::Bay) => "At your airlock, E rides the cap lift down. ",
            (Realm::Bay, Here::City(_)) => "Back up to your bay first: E at Hub Gate's lift. ",
            (Realm::Space, Here::Bay) => "Up the stairs: E at the cockpit's hatch launches. ",
            (Realm::Space, Here::City(_)) => {
                "Back up to your bay (E at Hub Gate's lift), and launch from its cockpit. "
            }
            (Realm::City | Realm::Bay | Realm::Afoot, Here::Space) => "Home first: dock at the ◆. ",
        };
        format!("{first}{}", self.how(survival, colony))
    }

    pub fn waypoint(self) -> Waypoint {
        match self {
            Objective::LandStation => Waypoint::Landmark(0),
            Objective::Hide => Waypoint::HideSpot(0, 0),
            Objective::Mine => Waypoint::Rock,
            Objective::Deliver | Objective::Arm | Objective::Siderium => Waypoint::Dock,
            Objective::Bounty | Objective::FiveDrones => Waypoint::Drones,
            Objective::AlmagestAce => Waypoint::Ace,
            Objective::Hermit => Waypoint::Landmark(1),
            Objective::RideDown => Waypoint::Place(hub_gate()),
            Objective::ExchangeFloor | Objective::Sell => Waypoint::Place(exchange()),
            Objective::ProvingGround | Objective::Certify | Objective::TryPrototype => {
                Waypoint::Place(proving_ground())
            }
            Objective::CharterDesk | Objective::Standing => Waypoint::Place(charter()),
            Objective::MostWanted => Waypoint::Place(bar()),
        }
    }

    /// How far along it is, for those that count: (so far, of).
    pub fn progress(self, i: &ObjectiveInput, downed: u32) -> Option<(u32, u32)> {
        match self {
            Objective::Mine => Some((i.cargo_kg.min(MINE_KG), MINE_KG)),
            Objective::FiveDrones => Some((downed.min(FIVE_DRONES), FIVE_DRONES)),
            _ => None,
        }
    }
}

/// The index in `PLACES` of the first place of kind `kind`.
fn place_of(kind: PlaceKind) -> u8 {
    PLACES.iter().position(|p| p.kind == kind).unwrap_or(0) as u8
}

/// Where the browser's cap lift comes down (`PLACES`' index): the first strip's Hub Gate.
fn hub_gate() -> u8 {
    hub_gate_on(0)
}

/// Strip `strip`'s Hub Gate (`PLACES`' index): its cap lift, the way up to the bays.
pub fn hub_gate_on(strip: u8) -> u8 {
    PLACES.iter().position(|p| p.kind == PlaceKind::HubGate && p.strip == strip).unwrap_or(0) as u8
}

/// The Exchange floor's index in `PLACES`.
pub fn exchange() -> u8 {
    place_of(PlaceKind::Exchange)
}

/// The Proving Ground's index in `PLACES`.
pub fn proving_ground() -> u8 {
    place_of(PlaceKind::Proving)
}

/// The Charter Board's hall's index in `PLACES`.
pub fn charter() -> u8 {
    place_of(PlaceKind::Charter)
}

/// The Arrival's (the bar off Charter Square) index in `PLACES`.
pub fn bar() -> u8 {
    place_of(PlaceKind::Bar)
}

/// A name as it's written in a sentence: "CHARTER" is "Charter".
fn word(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut chars = lower.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// What the pilotless suits are called.
fn drone() -> &'static str {
    bc_sim::content::drone_name()
}

/// What the Charter Board's suits (an Arrival's worn one, its trainers) are called.
fn argo() -> &'static str {
    bc_sim::content::frame_name(FrameId::Argo)
}

/// What a new line of the colony's own is called.
fn proto() -> &'static str {
    bc_sim::content::prototype_word()
}

/// What the pilot is doing this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectiveInput {
    /// Survival rules (else arcade).
    pub survival: bool,
    /// Where the pilot is: flying (or not yet anywhere), in their bay, or down in the city.
    pub here: Here,
    /// Flying in the sector, alive.
    pub flying: bool,
    /// How many landmarks the sector has, and whether it has rocks.
    pub landmarks: u8,
    pub rocks: bool,
    /// Standing on landmark `k`.
    pub landed_on: Option<u8>,
    /// Hidden (the server's word) in a hide spot of landmark `k`.
    pub hidden_on: Option<u8>,
    /// In the hold and in hand, kg.
    pub cargo_kg: u32,
    /// Inside the dock's ring (the own state's flag).
    pub docked: bool,
    /// Drones the pilot has downed this session (the world's count).
    pub drone_kills: u32,
    /// The colony is open (the Welcome's COLONY flag), the pilot is down in its city, and stands
    /// at the Exchange floor's door.
    pub colony: bool,
    pub in_city: bool,
    pub at_exchange: bool,
    /// At the Proving Ground's doors (or in it).
    pub at_proving: bool,
    /// Sales filled on the Exchange this session (`HangarState::sales`).
    pub sales: u32,
    /// The Proving Ground's board holds a time of the pilot's own (`HangarState::has_certificate`).
    pub certified: bool,
    /// At the Charter Board's door, and at The Arrival's.
    pub at_charter: bool,
    pub at_arrival: bool,
    /// Standing with the colony (`HangarState::standing`).
    pub standing: bool,
    /// The bay's suit has every weapon it carries fitted (`HangarState::armed`).
    pub armed: bool,
    /// One of the Almagest's aces downed (`HangarState::aces_downed`).
    pub aces: bool,
    /// Flying a new Lodestar off the test range (a trainer, alive).
    pub tried_prototype: bool,
    /// Siderium in the stores or in the making (`HangarState::smelting`).
    pub smelting: bool,
    /// The Charter Board holds something for the pilot at its desk (`HangarState::board_holds`).
    pub waiting: bool,
}

/// What the objectives remember between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Objectives {
    /// The session's drone kills already counted.
    kills_seen: u32,
    /// What was aboard last frame (a sale empties the hold the moment the dock's flag comes on).
    last_cargo: u32,
}

impl Objectives {
    /// Steps once a frame: marks what's done in `done` (bits of [`Objective`]) and counts the
    /// drones downed in `downed`. Returns the objective to show ([`Objectives::current`]), or none
    /// once they all are done.
    pub fn step(&mut self, done: &mut u32, downed: &mut u32, i: &ObjectiveInput) -> Option<Objective> {
        // A new session counts from nothing. A kill counts whenever it lands (a missile can bring
        // a drone down after its shooter is).
        if i.drone_kills < self.kills_seen {
            self.kills_seen = 0;
        }
        *downed = downed.saturating_add(i.drone_kills - self.kills_seen);
        self.kills_seen = i.drone_kills;
        let mut mark = |o: Objective, now: bool| {
            if now {
                *done |= o.bit();
            }
        };
        if i.flying {
            let aboard = i.cargo_kg.max(self.last_cargo);
            mark(Objective::LandStation, i.landed_on == Some(0));
            mark(Objective::Hide, i.hidden_on == Some(0));
            mark(Objective::Mine, i.cargo_kg >= MINE_KG);
            mark(Objective::Deliver, i.docked && aboard > 0);
            mark(Objective::Hermit, i.landed_on == Some(1));
            self.last_cargo = i.cargo_kg;
        } else {
            self.last_cargo = 0;
        }
        // Down the chain, on foot, and the server's word: each whenever it happens.
        mark(Objective::RideDown, i.in_city);
        mark(Objective::ExchangeFloor, i.at_exchange);
        mark(Objective::ProvingGround, i.at_proving);
        mark(Objective::Sell, i.sales > 0);
        mark(Objective::Certify, i.certified);
        mark(Objective::CharterDesk, i.at_charter);
        mark(Objective::Standing, i.standing);
        mark(Objective::Arm, i.armed);
        mark(Objective::MostWanted, i.at_arrival);
        mark(Objective::AlmagestAce, i.aces);
        mark(Objective::TryPrototype, i.tried_prototype);
        mark(Objective::Siderium, i.smelting);
        if *downed >= 1 {
            *done |= Objective::Bounty.bit();
        }
        if *downed >= FIVE_DRONES {
            *done |= Objective::FiveDrones.bit();
        }
        Self::current(*done, i)
    }

    /// The objective to show, of those not in `done` that the sector has room for. Flying, the
    /// first in the rules' order that's done in space (else the first: its how-line begins with
    /// the way home). On foot, the first in the rules' order, passing over one in space the pilot
    /// has flown past (one done in space comes later in the order): the flight still shows it, and
    /// the chart lists it. On foot, too, the Charter Board's desk is open again while the Board
    /// holds something for the pilot there (a certificate's rifle earned after the walk past it):
    /// its line says it's paid out there, and its `◆` is on the hall's door, not the way home to a
    /// console with no rifle in the stores yet.
    pub fn current(done: u32, i: &ObjectiveInput) -> Option<Objective> {
        let order = Objective::order(i.survival);
        let open = |o: &Objective| done & o.bit() == 0 && o.available(i);
        let space = |o: &Objective| o.realm() == Realm::Space;
        match i.here {
            Here::Space => order
                .iter()
                .copied()
                .find(|o| open(o) && space(o))
                .or_else(|| order.iter().copied().find(open)),
            Here::Bay | Here::City(_) => {
                let due = |o: &Objective| *o == Objective::CharterDesk && i.waiting && o.available(i);
                let flown_past =
                    |k: usize| order[k + 1..].iter().any(|later| space(later) && done & later.bit() != 0);
                order
                    .iter()
                    .enumerate()
                    .find(|&(k, o)| (open(o) && !(space(o) && flown_past(k))) || due(o))
                    .map(|(_, o)| *o)
            }
        }
    }
}

/// Where `w` is at render tick `t`, for a suit at `from`: its point, and its name for the marker.
pub fn waypoint_at(core: &ClientCore, w: Waypoint, from: Vec3, t: f64) -> Option<(Vec3, String)> {
    let bodies = &core.world.bodies;
    match w {
        Waypoint::Landmark(k) => {
            let def = bodies.landmarks().get(usize::from(k))?;
            Some((bodies.pose_at(Body::Landmark(k), t)?.pos, def.name.to_string()))
        }
        Waypoint::HideSpot(k, s) => {
            let spot = bodies.landmarks().get(usize::from(k))?.hides.get(usize::from(s))?;
            Some((bodies.pose_at(Body::Landmark(k), t)?.to_world(spot.center), spot.name.to_string()))
        }
        Waypoint::Rock => {
            let field = &bodies.field;
            field
                .rocks()
                .iter()
                .enumerate()
                .filter(|(i, r)| !field.is_dead(*i) && r.axes.min_element() >= BIG_ROCK)
                .min_by(|a, b| a.1.pos.distance_squared(from).total_cmp(&b.1.pos.distance_squared(from)))
                .map(|(_, r)| (r.pos, "ROCK".to_string()))
        }
        // Flying: home first (on foot the page's map marks the door).
        Waypoint::Dock | Waypoint::Place(_) => Some((DOCK_CENTER, "DOCK".to_string())),
        Waypoint::Drones => {
            let world = &core.world;
            let near = world
                .entities
                .iter()
                .flatten()
                .filter(|tr| {
                    let e = &tr.latest;
                    e.pilot == bc_proto::PilotKind::Drone
                        && e.faction != core.cfg.faction
                        && e.flags & bc_proto::snapshot::ent_flags::WRECK == 0
                })
                .map(|tr| tr.sample(t, &world.bodies).pos)
                .min_by(|a, b| a.distance_squared(from).total_cmp(&b.distance_squared(from)));
            Some(match near {
                Some(p) => (p, drone().to_uppercase()),
                None => (FIELD_CENTER, "PATROLS".to_string()),
            })
        }
        // An ace is a drone the roster names as one of the Almagest's (as `bc-server/tests/aces.rs` tells
        // them); with none in sight, the drones.
        Waypoint::Ace => {
            let world = &core.world;
            let ace = world
                .entities
                .iter()
                .flatten()
                .filter_map(|tr| {
                    let e = &tr.latest;
                    if e.pilot != bc_proto::PilotKind::Drone
                        || e.flags & bc_proto::snapshot::ent_flags::WRECK != 0
                    {
                        return None;
                    }
                    let (name, _) = world.roster.get(&e.slot)?;
                    let ace = ACES.iter().find(|a| a.name == name.as_str())?;
                    Some((tr.sample(t, &world.bodies).pos, ace.name))
                })
                .min_by(|a, b| a.0.distance_squared(from).total_cmp(&b.0.distance_squared(from)));
            match ace {
                Some((p, name)) => Some((p, name.to_string())),
                None => waypoint_at(core, Waypoint::Drones, from, t),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight that came with the desks, in bit order.
    const NEW: [Objective; 8] = [
        Objective::Certify,
        Objective::CharterDesk,
        Objective::Standing,
        Objective::Arm,
        Objective::MostWanted,
        Objective::AlmagestAce,
        Objective::TryPrototype,
        Objective::Siderium,
    ];

    fn flying(survival: bool) -> ObjectiveInput {
        ObjectiveInput { survival, flying: true, landmarks: 2, rocks: true, ..Default::default() }
    }

    /// Survival with the colony open, flying; on foot in the bay; on foot on strip 0 of the city.
    fn open() -> ObjectiveInput {
        ObjectiveInput { colony: true, ..flying(true) }
    }
    fn in_bay(i: ObjectiveInput) -> ObjectiveInput {
        ObjectiveInput { flying: false, here: Here::Bay, ..i }
    }
    fn in_city(i: ObjectiveInput) -> ObjectiveInput {
        ObjectiveInput { flying: false, here: Here::City(0), in_city: true, ..i }
    }

    fn bits(os: &[Objective]) -> u32 {
        os.iter().fold(0, |a, o| a | o.bit())
    }

    #[test]
    fn every_objective_has_its_bit_and_both_orders_list_them_all() {
        let all = bits(&Objective::ALL);
        assert_eq!(all.count_ones() as usize, Objective::ALL.len());
        assert_eq!(Objective::ALL.len(), 19);
        for (k, o) in Objective::ALL.iter().enumerate() {
            assert_eq!(o.bit(), 1 << k, "ALL is in bit order: {o:?}");
        }
        // The bits never move: old settings keep their meanings.
        assert_eq!(Objective::FiveDrones.bit(), 1 << 6);
        assert_eq!(Objective::ProvingGround.bit(), 1 << 10);
        for (k, o) in NEW.iter().enumerate() {
            assert_eq!(o.bit(), 1 << (11 + k), "{o:?}");
        }
        assert_eq!(Objective::Siderium.bit(), 1 << 18);
        for survival in [true, false] {
            let order = Objective::order(survival);
            assert_eq!(bits(order), all);
            for o in order {
                assert!(!o.title(survival).is_empty() && !o.how(survival, true).is_empty());
                assert!(!o.how(survival, false).is_empty());
                assert_eq!(o.title(survival), o.title(survival).to_uppercase(), "the HUD is all caps");
            }
        }
        let titles: Vec<String> = NEW.iter().map(|o| o.title(true)).collect();
        let proto = bc_sim::content::prototype_word().to_uppercase();
        assert_eq!(
            titles,
            [
                "EARN A FLIGHT CERTIFICATE".to_string(),
                "REPORT TO THE CHARTER BOARD".into(),
                "EARN STANDING WITH THE COLONY".into(),
                "ARM YOUR SUIT".into(),
                "READ THE MOST WANTED".into(),
                "DOWN ONE OF THE ALMAGEST'S ACES".into(),
                format!("TRY A NEW {proto} ON THE TEST RANGE"),
                "MAKE SIDERIUM".into(),
            ]
        );
        if bc_sim::content::prototype_word() == "Lodestar" {
            assert_eq!(Objective::TryPrototype.title(true), "TRY A NEW LODESTAR ON THE TEST RANGE");
        }
        assert_eq!(
            Objective::FiveDrones.title(true),
            format!("DOWN 5 {}S", bc_sim::content::drone_name().to_uppercase())
        );
    }

    #[test]
    fn the_first_not_done_shows_and_any_can_be_done_first() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        let i = flying(true);
        assert_eq!(o.step(&mut done, &mut downed, &i), Some(Objective::LandStation));
        // Hiding first counts, though landing is still the one shown.
        let hid = ObjectiveInput { hidden_on: Some(0), ..i };
        assert_eq!(o.step(&mut done, &mut downed, &hid), Some(Objective::LandStation));
        assert!(done & Objective::Hide.bit() != 0);
        let landed = ObjectiveInput { landed_on: Some(0), ..i };
        assert_eq!(o.step(&mut done, &mut downed, &landed), Some(Objective::Mine));
        // Hidden on Hermit isn't the station's well.
        let mut d2 = 0;
        o.step(&mut d2, &mut downed, &ObjectiveInput { hidden_on: Some(1), ..i });
        assert_eq!(d2 & Objective::Hide.bit(), 0);
    }

    #[test]
    fn mining_then_a_sale_at_the_dock_counts_though_the_hold_empties_at_once() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        let i = flying(false);
        o.step(&mut done, &mut downed, &ObjectiveInput { cargo_kg: 150, ..i });
        assert_eq!(done & Objective::Mine.bit(), 0, "150 kg isn't 200");
        assert_eq!(Objective::Mine.progress(&ObjectiveInput { cargo_kg: 150, ..i }, 0), Some((150, MINE_KG)));
        o.step(&mut done, &mut downed, &ObjectiveInput { cargo_kg: 240, ..i });
        assert!(done & Objective::Mine.bit() != 0);
        // The dock's flag comes on with the hold already sold.
        o.step(&mut done, &mut downed, &ObjectiveInput { docked: true, ..i });
        assert!(done & Objective::Deliver.bit() != 0);
        // An empty-handed visit doesn't.
        let (mut o, mut done) = (Objectives::default(), 0u32);
        o.step(&mut done, &mut downed, &i);
        o.step(&mut done, &mut downed, &ObjectiveInput { docked: true, ..i });
        assert_eq!(done & Objective::Deliver.bit(), 0);
    }

    /// A new Arrival, survival with the colony open, on foot in their bay: down the cap lift, the
    /// Proving Ground and its certificate, a straight walk along the avenue to the Exchange floor
    /// and the Charter Board's desk, standing; then home to arm the suit, and out.
    #[test]
    fn a_new_arrivals_chain_walks_the_avenue_then_goes_home_to_arm_and_out() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        let bay = in_bay(open());
        assert_eq!(o.step(&mut done, &mut downed, &bay), Some(Objective::RideDown));
        assert_eq!(Objective::RideDown.how_from(true, true, Here::Bay), Objective::RideDown.how(true, true));
        let city = in_city(open());
        let step = |o: &mut Objectives, done: &mut u32, i: ObjectiveInput| {
            let mut downed = 0;
            o.step(done, &mut downed, &i)
        };
        assert_eq!(step(&mut o, &mut done, city), Some(Objective::ProvingGround));
        assert_eq!(
            step(&mut o, &mut done, ObjectiveInput { at_proving: true, ..city }),
            Some(Objective::Certify)
        );
        assert_eq!(Objective::Certify.waypoint(), Waypoint::Place(proving_ground()));
        assert_eq!(
            step(&mut o, &mut done, ObjectiveInput { certified: true, ..city }),
            Some(Objective::ExchangeFloor)
        );
        assert_eq!(
            step(&mut o, &mut done, ObjectiveInput { at_exchange: true, ..city }),
            Some(Objective::CharterDesk)
        );
        assert_eq!(Objective::CharterDesk.waypoint(), Waypoint::Place(charter()));
        assert_eq!(PLACES[usize::from(charter())].kind, PlaceKind::Charter);
        assert_eq!(
            step(&mut o, &mut done, ObjectiveInput { at_charter: true, ..city }),
            Some(Objective::Standing)
        );
        assert_eq!(step(&mut o, &mut done, ObjectiveInput { standing: true, ..city }), Some(Objective::Arm));
        assert!(
            Objective::Arm.how_from(true, true, Here::City(0)).starts_with("Back up to your bay first"),
            "the rifle's fitted in the bay"
        );
        // Home, and armed: out.
        assert_eq!(
            step(&mut o, &mut done, ObjectiveInput { armed: true, ..bay }),
            Some(Objective::LandStation)
        );
        assert!(Objective::LandStation.how_from(true, true, Here::Bay).starts_with("Up the stairs"));
        assert_eq!(step(&mut o, &mut done, open()), Some(Objective::LandStation));
        assert_eq!(
            Objective::LandStation.how_from(true, true, Here::Space),
            Objective::LandStation.how(true, true)
        );
    }

    /// A pilot who launches first is never pulled home while there's something to do out there;
    /// once there isn't, the first left begins with the way home.
    #[test]
    fn a_pilot_flying_first_is_never_pulled_home_while_a_space_objective_is_open() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        assert_eq!(o.step(&mut done, &mut downed, &open()), Some(Objective::LandStation));
        for space in Objective::order(true).iter().filter(|o| o.realm() == Realm::Space) {
            assert_eq!(Objectives::current(done, &open()), Some(*space));
            done |= space.bit();
        }
        assert_eq!(Objectives::current(done, &open()), Some(Objective::RideDown));
        assert!(
            Objective::RideDown.how_from(true, true, Here::Space).starts_with("Home first: dock at the ◆. ")
        );
    }

    /// One in space skipped (Hide, after Mine): on foot it's passed over for what's next; the
    /// flight still shows it.
    #[test]
    fn a_space_objective_flown_past_waits_for_the_flight() {
        let walked = bits(&[
            Objective::RideDown,
            Objective::ProvingGround,
            Objective::Certify,
            Objective::ExchangeFloor,
            Objective::CharterDesk,
            Objective::Standing,
            Objective::Arm,
        ]);
        let done = walked | bits(&[Objective::LandStation, Objective::Mine, Objective::Deliver]);
        assert_eq!(Objectives::current(done, &in_bay(open())), Some(Objective::Sell));
        assert_eq!(Objectives::current(done, &in_city(open())), Some(Objective::Sell));
        assert_eq!(Objectives::current(done, &open()), Some(Objective::Hide));
        // Not flown past (nothing later in space done): on foot, it's still next.
        let landed = walked | Objective::LandStation.bit();
        assert_eq!(Objectives::current(landed, &in_bay(open())), Some(Objective::Hide));
        // Passing over is space's alone: a city one skipped still shows.
        let walked_past = done & !Objective::ExchangeFloor.bit();
        assert_eq!(Objectives::current(walked_past, &in_city(open())), Some(Objective::ExchangeFloor));
    }

    /// Each of the new ones is done by its own input, and by nothing else.
    #[test]
    fn each_new_objective_is_marked_by_its_own_input_alone() {
        for base in [in_bay(open()), in_city(open()), open()] {
            let base = ObjectiveInput { in_city: false, ..base };
            let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
            o.step(&mut done, &mut downed, &base);
            assert_eq!(done, 0, "nothing's done by being there");
            for (want, i) in [
                (Objective::Certify, ObjectiveInput { certified: true, ..base }),
                (Objective::CharterDesk, ObjectiveInput { at_charter: true, ..base }),
                (Objective::Standing, ObjectiveInput { standing: true, ..base }),
                (Objective::Arm, ObjectiveInput { armed: true, ..base }),
                (Objective::MostWanted, ObjectiveInput { at_arrival: true, ..base }),
                (Objective::AlmagestAce, ObjectiveInput { aces: true, ..base }),
                (Objective::TryPrototype, ObjectiveInput { tried_prototype: true, ..base }),
                (Objective::Siderium, ObjectiveInput { smelting: true, ..base }),
            ] {
                let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
                o.step(&mut done, &mut downed, &i);
                assert_eq!(done, want.bit(), "{want:?}");
                // Sticky: it stays done once the input's gone.
                o.step(&mut done, &mut downed, &base);
                assert_eq!(done, want.bit(), "{want:?} stays done");
            }
        }
    }

    /// Arcade: the colony's closed and there's no hangar, so none of the new ones shows; the drones
    /// come first.
    #[test]
    fn arcade_has_none_of_the_colonys_or_the_bays() {
        let arcade = flying(false);
        for o in NEW {
            assert!(!o.available(&arcade), "{o:?}");
        }
        assert_eq!(Objectives::current(0, &arcade), Some(Objective::Bounty));
        assert_eq!(Objective::order(false)[0], Objective::Bounty);
    }

    /// Survival with the colony closed: what's the colony's doesn't show; the bay's and the
    /// Board's do (the Board's tab is on the bay's terminals).
    #[test]
    fn with_the_colony_closed_the_colonys_objectives_dont_show() {
        let closed = flying(true);
        for o in [Objective::Certify, Objective::CharterDesk, Objective::MostWanted, Objective::TryPrototype]
        {
            assert!(!o.available(&closed), "{o:?}");
        }
        for o in [
            Objective::Sell,
            Objective::Standing,
            Objective::Arm,
            Objective::AlmagestAce,
            Objective::Siderium,
        ] {
            assert!(o.available(&closed), "{o:?}");
        }
        // Sold at the bay's own terminals: no Exchange floor to walk to.
        let sell = Objective::Sell.how(true, false);
        assert!(sell.contains("your bay's terminals") && !sell.contains("floor"), "{sell}");
        assert_eq!(Objectives::current(0, &in_bay(closed)), Some(Objective::Standing));
        // Flying, home done: the drones.
        let home = bits(&[Objective::LandStation, Objective::Hide, Objective::Mine, Objective::Deliver]);
        assert_eq!(Objectives::current(home, &closed), Some(Objective::Bounty));
        // No certificate to earn, so no Board's rifle to fit: the Exchange's or the fabricator's.
        let arm = Objective::Arm.how(true, false);
        assert!(!arm.contains("Charter Board") && arm.contains("bought on the Exchange"), "{arm}");
        assert!(Objective::Arm.how(true, true).contains("the Charter Board's for a flight certificate"));
        // Whatever the Board holds is collected anywhere with the colony shut: no desk to go to.
        let waiting = ObjectiveInput { waiting: true, ..in_bay(closed) };
        assert_eq!(Objectives::current(0, &waiting), Some(Objective::Standing));
    }

    /// The Standing line names no work that may have all it needs (or be done, the charter signed):
    /// what each still needs is the board's to say.
    #[test]
    fn the_standing_line_holds_however_far_the_works_have_come() {
        let how = Objective::Standing.how(true, true);
        assert!(how.contains("the board lists what each still needs"), "{how}");
        assert!(how.contains("while a great work is short of steel"), "{how}");
        assert!(!how.contains("second foundry"), "{how}");
    }

    /// A certificate earned after the walk past the Charter Board's desk: its rifle waits there,
    /// so on foot the desk is next again (not the way home to a console with no rifle in the
    /// stores), until it's handed over. Flying, nothing changes.
    #[test]
    fn a_rifle_waiting_at_the_boards_desk_leads_back_to_it() {
        let walked = bits(&[
            Objective::RideDown,
            Objective::ProvingGround,
            Objective::ExchangeFloor,
            Objective::CharterDesk,
            Objective::Standing,
        ]);
        let (mut o, mut downed) = (Objectives::default(), 0u32);
        let mut done = walked;
        assert_eq!(o.step(&mut done, &mut downed, &in_city(open())), Some(Objective::Certify));
        // The drill cleared: the Board has the rifle at its desk.
        let hall = ObjectiveInput { certified: true, waiting: true, ..in_city(open()) };
        assert_eq!(o.step(&mut done, &mut downed, &hall), Some(Objective::CharterDesk));
        assert!(done & Objective::Certify.bit() != 0);
        let bay = ObjectiveInput { waiting: true, ..in_bay(open()) };
        assert_eq!(o.step(&mut done, &mut downed, &bay), Some(Objective::CharterDesk));
        assert!(
            Objective::CharterDesk
                .how_from(true, true, Here::Bay)
                .starts_with("At your airlock, E rides the cap lift down. ")
        );
        assert!(
            Objective::CharterDesk
                .how(true, true)
                .contains("What the Board holds for you is paid out at its desk")
        );
        // Flying, the sector's own come first, as ever.
        let flying = ObjectiveInput { waiting: true, ..open() };
        assert_eq!(o.step(&mut done, &mut downed, &flying), Some(Objective::LandStation));
        // Handed over on walking into its hall: home to arm.
        assert_eq!(o.step(&mut done, &mut downed, &in_city(open())), Some(Objective::Arm));
        assert_eq!(done & Objective::Arm.bit(), 0);
        // Everything done, a bounty waiting: the desk on foot, nothing in flight.
        let all = bits(&Objective::ALL);
        assert_eq!(Objectives::current(all, &bay), Some(Objective::CharterDesk));
        assert_eq!(Objectives::current(all, &flying), None);
        assert_eq!(Objectives::current(all, &in_bay(open())), None);
    }

    /// The how-line from where the pilot is: the way there first, each a sentence.
    #[test]
    fn the_how_line_begins_with_the_way_there() {
        let charter_strip = "It's on the Charter strip: ride the lift up from Hub Gate here (E), and from your airlock E rides lift 1 down to it.";
        let home = "Home first: dock at the ◆. ";
        for (o, realm) in [
            (Objective::Mine, Realm::Space),
            (Objective::Arm, Realm::Bay),
            (Objective::CharterDesk, Realm::City),
            (Objective::Sell, Realm::Afoot),
        ] {
            assert_eq!(o.realm(), realm);
            let how = o.how(true, true);
            let from = |here| o.how_from(true, true, here);
            let (space, bay, city, other_strip) = match realm {
                Realm::Space => {
                    let up = format!(
                        "Back up to your bay (E at Hub Gate's lift), and launch from its cockpit. {how}"
                    );
                    (
                        how.clone(),
                        format!("Up the stairs: E at the cockpit's hatch launches. {how}"),
                        up.clone(),
                        up,
                    )
                }
                Realm::Bay => {
                    let up = format!("Back up to your bay first: E at Hub Gate's lift. {how}");
                    (format!("{home}{how}"), how.clone(), up.clone(), up)
                }
                Realm::City => (
                    format!("{home}{how}"),
                    format!("At your airlock, E rides the cap lift down. {how}"),
                    how.clone(),
                    charter_strip.to_string(),
                ),
                Realm::Afoot => (format!("{home}{how}"), how.clone(), how.clone(), charter_strip.to_string()),
            };
            assert_eq!(from(Here::Space), space, "{o:?} flying");
            assert_eq!(from(Here::Bay), bay, "{o:?} in the bay");
            assert_eq!(from(Here::City(0)), city, "{o:?} on its strip");
            assert_eq!(from(Here::City(1)), other_strip, "{o:?} on another strip");
        }
        // Wherever the pilot is, the line ends with how it's done.
        for o in Objective::ALL {
            for here in [Here::Space, Here::Bay, Here::City(0)] {
                assert!(o.how_from(true, true, here).ends_with(&o.how(true, true)), "{o:?} from {here:?}");
            }
        }
    }

    /// The key places are on the Charter strip: on another, the objective says how to get there
    /// rather than pointing at a square that strip's Hub Gate doesn't have.
    #[test]
    fn on_another_strip_the_way_to_a_place_goes_up_and_down_its_lift() {
        for o in [
            Objective::ProvingGround,
            Objective::ExchangeFloor,
            Objective::Sell,
            Objective::Certify,
            Objective::CharterDesk,
            Objective::MostWanted,
        ] {
            let Waypoint::Place(k) = o.waypoint() else { panic!("{o:?} has a place") };
            let home = PLACES[usize::from(k)].strip;
            assert_eq!(o.how_on(true, true, Some(home)), o.how(true, true), "{o:?} on its own strip");
            assert_eq!(o.how_on(true, true, None), o.how(true, true), "{o:?} in the bay");
            let other = o.how_on(true, true, Some((home + 1) % 3));
            assert_eq!(home, 0);
            assert_eq!(
                other,
                "It's on the Charter strip: ride the lift up from Hub Gate here (E), and from your airlock E rides lift 1 down to it."
            );
        }
        assert_eq!(PLACES[usize::from(bar())].kind, PlaceKind::Bar);
        for strip in 0..3 {
            let gate = &PLACES[usize::from(hub_gate_on(strip))];
            assert_eq!((gate.kind, gate.strip), (PlaceKind::HubGate, strip));
        }
        // Nothing of the city's in the sector's.
        assert_eq!(Objective::Mine.how_on(true, true, Some(2)), Objective::Mine.how(true, true));
        assert_eq!(word("GARDENS"), "Gardens");
    }

    #[test]
    fn drones_downed_add_up_across_sessions() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        let i = flying(false);
        assert_eq!(o.step(&mut done, &mut downed, &i), Some(Objective::Bounty));
        assert_eq!(
            o.step(&mut done, &mut downed, &ObjectiveInput { drone_kills: 3, ..i }),
            Some(Objective::Mine)
        );
        assert_eq!(downed, 3);
        assert_eq!(done & Objective::FiveDrones.bit(), 0);
        assert_eq!(Objective::FiveDrones.progress(&i, downed), Some((3, FIVE_DRONES)));
        // A new session's world counts from nothing again.
        o.step(&mut done, &mut downed, &ObjectiveInput { drone_kills: 0, ..i });
        o.step(&mut done, &mut downed, &ObjectiveInput { drone_kills: 2, ..i });
        assert_eq!(downed, 5);
        assert!(done & Objective::FiveDrones.bit() != 0);
        // A kill that lands after the pilot is down (a missile still in flight) counts, once.
        o.step(&mut done, &mut downed, &ObjectiveInput { drone_kills: 3, flying: false, ..i });
        o.step(&mut done, &mut downed, &ObjectiveInput { drone_kills: 3, ..i });
        assert_eq!(downed, 6);
    }

    #[test]
    fn a_sector_without_landmarks_or_rocks_skips_what_needs_them() {
        let i = ObjectiveInput { survival: true, flying: true, ..Default::default() };
        assert_eq!(Objectives::current(0, &i), Some(Objective::Deliver));
        let all: u32 = Objective::ALL.iter().map(|o| o.bit()).sum();
        assert_eq!(Objectives::current(all, &flying(true)), None);
        assert_eq!(Objectives::current(all, &in_city(open())), None);
    }

    /// The Almagest's ace is the drone the roster names as one, nearer drones or not; with none out, the
    /// drones'.
    #[test]
    fn the_aces_waypoint_is_the_ace_by_its_name_else_the_drones() {
        use crate::interp::EntityTrack;
        use bc_proto::snapshot::EntityState;
        use bc_proto::{Faction, PilotKind};
        let mut core = crate::ClientCore::new(crate::ClientConfig {
            name: "test".into(),
            pilot: PilotKind::Human,
            frame: FrameId::Argo,
            faction: Faction::Colonies,
        });
        let drone = |slot: u16, pos: Vec3| EntityState {
            slot,
            faction: Faction::Almagest,
            pilot: PilotKind::Drone,
            pos,
            ..EntityState::default()
        };
        let world = &mut core.world;
        world.entities[3] = Some(EntityTrack::new(10, drone(3, Vec3::new(100.0, 0.0, 0.0))));
        world.roster.insert(3, ("Drone 3".into(), PilotKind::Drone));
        let from = Vec3::ZERO;
        let drones = waypoint_at(&core, Waypoint::Drones, from, 10.0);
        assert_eq!(waypoint_at(&core, Waypoint::Ace, from, 10.0), drones, "no ace out: the drones");
        assert_eq!(drones, Some((Vec3::new(100.0, 0.0, 0.0), bc_sim::content::drone_name().to_uppercase())));
        let world = &mut core.world;
        world.entities[5] = Some(EntityTrack::new(10, drone(5, Vec3::new(0.0, 0.0, 900.0))));
        world.roster.insert(5, ("LYRA".into(), PilotKind::Drone));
        assert_eq!(
            waypoint_at(&core, Waypoint::Ace, from, 10.0),
            Some((Vec3::new(0.0, 0.0, 900.0), "LYRA".to_string())),
            "the ace, past a nearer drone"
        );
        // Its wreck is no waypoint.
        let world = &mut core.world;
        let wreck = EntityState {
            flags: bc_proto::snapshot::ent_flags::WRECK,
            ..drone(5, Vec3::new(0.0, 0.0, 900.0))
        };
        world.entities[5] = Some(EntityTrack::new(10, wreck));
        assert_eq!(waypoint_at(&core, Waypoint::Ace, from, 10.0), drones);
    }
}
