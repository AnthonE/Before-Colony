//! Objectives: what a pilot can set out to do in the sector. The HUD shows one at a time with a
//! waypoint to fly to (◆), and the map (M) lists them all. They're the Charter Board's first jobs
//! for a new Arrival: land on the resource satellite and hide in its well, mine ore and bring it
//! home, then (with the colony open) ride the cap lift down, report to the Proving Ground, find the
//! Exchange floor and sell on it, down a Mobile Doll, reach Hermit, and make a name for yourself.
//!
//! They can be done in any order: each is checked every frame, and the HUD shows the first in the
//! rules' order that isn't done yet. What's done is kept with the settings (a bit per
//! [`Objective`], and how many Dolls the pilot has downed), so it carries over from one visit to
//! the next in the same browser.
//!
//! Everything they're checked against is what the client already knows: where the suit stands and
//! what it holds, its cover, the dock's flag and its kills. None of it is a reward: bounties and
//! sales are the server's, as ever. These point the way to them.

use bc_sim::bodies::Body;
use bc_sim::content::city::{PLACES, PlaceKind};
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::field::FIELD_CENTER;
use glam::Vec3;

use crate::ClientCore;

/// Ore in the hold that counts as having mined, kg.
pub const MINE_KG: u32 = 200;
/// Dolls downed that make a name.
pub const ACE: u32 = 5;
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
    /// Down a Mobile Doll.
    Bounty,
    /// Land on Hermit (the second landmark).
    Hermit,
    /// Down [`ACE`] Mobile Dolls.
    Ace,
    /// Ride a cap lift down from the bays into the colony (the colony open).
    RideDown,
    /// Find the Colony Exchange's floor in the city.
    ExchangeFloor,
    /// Sell something on the Exchange.
    Sell,
    /// Report to the Proving Ground, the Blast Hall off Hub Gate's square (`docs/TRAINING.md`).
    ProvingGround,
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
    /// The nearest Mobile Doll in sight, else their patrols over the field.
    Dolls,
    /// A place in the colony's city (an index into `content::city::PLACES`): flying, the way
    /// home to the dock comes first; on foot, its door.
    Place(u8),
}

impl Objective {
    pub const ALL: [Objective; 11] = [
        Objective::LandStation,
        Objective::Hide,
        Objective::Mine,
        Objective::Deliver,
        Objective::Bounty,
        Objective::Hermit,
        Objective::Ace,
        Objective::RideDown,
        Objective::ExchangeFloor,
        Objective::Sell,
        Objective::ProvingGround,
    ];

    /// Survival's order: a pilot launches at the dock, beside the station, in a worn Leo; the
    /// Dolls are a long way off. The ore brought home goes down the chain: to the colony (its
    /// Proving Ground off the square first), and its Exchange.
    const SURVIVAL: [Objective; 11] = [
        Objective::LandStation,
        Objective::Hide,
        Objective::Mine,
        Objective::Deliver,
        Objective::RideDown,
        Objective::ProvingGround,
        Objective::ExchangeFloor,
        Objective::Sell,
        Objective::Bounty,
        Objective::Hermit,
        Objective::Ace,
    ];
    /// Arcade's: a pilot launches by the field, in whatever suit they like, among the Dolls (and
    /// the colony is closed).
    const ARCADE: [Objective; 11] = [
        Objective::Bounty,
        Objective::Mine,
        Objective::Deliver,
        Objective::LandStation,
        Objective::Hide,
        Objective::Hermit,
        Objective::Ace,
        Objective::RideDown,
        Objective::ProvingGround,
        Objective::ExchangeFloor,
        Objective::Sell,
    ];

    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    /// In the order the rules show them.
    pub fn order(survival: bool) -> &'static [Objective; 11] {
        if survival { &Self::SURVIVAL } else { &Self::ARCADE }
    }

    /// Whether the sector has what it needs (a landmark, rocks, the colony open).
    pub fn available(self, i: &ObjectiveInput) -> bool {
        match self {
            Objective::LandStation | Objective::Hide => i.landmarks >= 1,
            Objective::Hermit => i.landmarks >= 2,
            Objective::Mine => i.rocks,
            Objective::RideDown | Objective::ProvingGround | Objective::ExchangeFloor | Objective::Sell => {
                i.colony
            }
            _ => true,
        }
    }

    /// Done on foot (in the bay or the city), so shown there too.
    pub fn on_foot(self) -> bool {
        matches!(
            self,
            Objective::RideDown | Objective::ProvingGround | Objective::ExchangeFloor | Objective::Sell
        )
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
            Objective::Bounty => format!("DOWN A {}", doll().to_uppercase()),
            Objective::Hermit => format!("LAND ON {}", LANDMARKS[1].name),
            Objective::Ace => format!("DOWN {ACE} {}S", doll().to_uppercase()),
            Objective::RideDown => "RIDE THE CAP LIFT DOWN".into(),
            Objective::ExchangeFloor => "FIND THE EXCHANGE FLOOR".into(),
            Objective::Sell => "SELL ON THE EXCHANGE".into(),
            Objective::ProvingGround => "REPORT TO THE PROVING GROUND".into(),
        }
    }

    /// How to go about it: one plain line.
    pub fn how(self, survival: bool) -> String {
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
                doll()
            ),
            Objective::Hermit => format!(
                "The big asteroid, 15 km out past the field. Its craters hide a suit: {}.",
                LANDMARKS[1].hides.iter().map(|h| h.name).collect::<Vec<_>>().join(", ")
            ),
            Objective::Ace => "Make a name for yourself. Every one downed is a bounty paid.".into(),
            Objective::RideDown => {
                "Home in your bay, walk to the airlock and press E: the cap lift rides down the end cap into the colony."
                    .into()
            }
            Objective::ExchangeFloor => {
                "From Hub Gate, along the avenue: M shows the city, ◆ marks the Exchange's door.".into()
            }
            Objective::Sell => {
                "At the Exchange floor's door, E opens its book: sell your ore to the colony's desk or another pilot."
                    .into()
            }
            Objective::ProvingGround => {
                "The Blast Hall, off Hub Gate's square: M shows the city, ◆ marks its blast doors. Its desk (E) has the course."
                    .into()
            }
        }
    }

    pub fn waypoint(self) -> Waypoint {
        match self {
            Objective::LandStation => Waypoint::Landmark(0),
            Objective::Hide => Waypoint::HideSpot(0, 0),
            Objective::Mine => Waypoint::Rock,
            Objective::Deliver => Waypoint::Dock,
            Objective::Bounty | Objective::Ace => Waypoint::Dolls,
            Objective::Hermit => Waypoint::Landmark(1),
            Objective::RideDown => Waypoint::Place(hub_gate()),
            Objective::ExchangeFloor | Objective::Sell => Waypoint::Place(exchange()),
            Objective::ProvingGround => Waypoint::Place(proving_ground()),
        }
    }

    /// How far along it is, for those that count: (so far, of).
    pub fn progress(self, i: &ObjectiveInput, downed: u32) -> Option<(u32, u32)> {
        match self {
            Objective::Mine => Some((i.cargo_kg.min(MINE_KG), MINE_KG)),
            Objective::Ace => Some((downed.min(ACE), ACE)),
            _ => None,
        }
    }
}

/// Where the browser's cap lift comes down (`PLACES`' index): the first strip's Hub Gate.
fn hub_gate() -> u8 {
    PLACES.iter().position(|p| p.kind == PlaceKind::HubGate && p.strip == 0).unwrap_or(0) as u8
}

/// The Exchange floor's index in `PLACES`.
pub fn exchange() -> u8 {
    PLACES.iter().position(|p| p.kind == PlaceKind::Exchange).unwrap_or(0) as u8
}

/// The Proving Ground's index in `PLACES`.
pub fn proving_ground() -> u8 {
    PLACES.iter().position(|p| p.kind == PlaceKind::Proving).unwrap_or(0) as u8
}

/// What the pilotless suits are called.
fn doll() -> &'static str {
    bc_sim::content::doll_name()
}

/// What the pilot is doing this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectiveInput {
    /// Survival rules (else arcade).
    pub survival: bool,
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
    /// Mobile Dolls the pilot has downed this session (the world's count).
    pub doll_kills: u32,
    /// The colony is open (the Welcome's COLONY flag), the pilot is down in its city, and stands
    /// at the Exchange floor's door.
    pub colony: bool,
    pub in_city: bool,
    pub at_exchange: bool,
    /// At the Proving Ground's doors (or in it).
    pub at_proving: bool,
    /// Sales filled on the Exchange this session (`HangarState::sales`).
    pub sales: u32,
}

/// What the objectives remember between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Objectives {
    /// The session's Doll kills already counted.
    kills_seen: u32,
    /// What was aboard last frame (a sale empties the hold the moment the dock's flag comes on).
    last_cargo: u32,
}

impl Objectives {
    /// Steps once a frame: marks what's done in `done` (bits of [`Objective`]) and counts the
    /// Dolls downed in `downed`. Returns the objective to show: the first in the rules' order that
    /// isn't done and the sector has room for, or none once they all are.
    pub fn step(&mut self, done: &mut u32, downed: &mut u32, i: &ObjectiveInput) -> Option<Objective> {
        // A new session counts from nothing. A kill counts whenever it lands (a missile can bring
        // a Doll down after its shooter is).
        if i.doll_kills < self.kills_seen {
            self.kills_seen = 0;
        }
        *downed = downed.saturating_add(i.doll_kills - self.kills_seen);
        self.kills_seen = i.doll_kills;
        if i.flying {
            let aboard = i.cargo_kg.max(self.last_cargo);
            let mut mark = |o: Objective, now: bool| {
                if now {
                    *done |= o.bit();
                }
            };
            mark(Objective::LandStation, i.landed_on == Some(0));
            mark(Objective::Hide, i.hidden_on == Some(0));
            mark(Objective::Mine, i.cargo_kg >= MINE_KG);
            mark(Objective::Deliver, i.docked && aboard > 0);
            mark(Objective::Hermit, i.landed_on == Some(1));
            self.last_cargo = i.cargo_kg;
        } else {
            self.last_cargo = 0;
        }
        // Down the chain, on foot.
        if i.in_city {
            *done |= Objective::RideDown.bit();
        }
        if i.at_exchange {
            *done |= Objective::ExchangeFloor.bit();
        }
        if i.at_proving {
            *done |= Objective::ProvingGround.bit();
        }
        if i.sales > 0 {
            *done |= Objective::Sell.bit();
        }
        if *downed >= 1 {
            *done |= Objective::Bounty.bit();
        }
        if *downed >= ACE {
            *done |= Objective::Ace.bit();
        }
        Self::current(*done, i)
    }

    /// The first objective in the rules' order not in `done` that the sector has room for.
    pub fn current(done: u32, i: &ObjectiveInput) -> Option<Objective> {
        Objective::order(i.survival).iter().copied().find(|o| done & o.bit() == 0 && o.available(i))
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
        Waypoint::Dolls => {
            let world = &core.world;
            let near = world
                .entities
                .iter()
                .flatten()
                .filter(|tr| {
                    let e = &tr.latest;
                    e.pilot == bc_proto::PilotKind::MobileDoll
                        && e.faction != core.cfg.faction
                        && e.flags & bc_proto::snapshot::ent_flags::WRECK == 0
                })
                .map(|tr| tr.sample(t, &world.bodies).pos)
                .min_by(|a, b| a.distance_squared(from).total_cmp(&b.distance_squared(from)));
            Some(match near {
                Some(p) => (p, doll().to_uppercase()),
                None => (FIELD_CENTER, "PATROLS".to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flying(survival: bool) -> ObjectiveInput {
        ObjectiveInput { survival, flying: true, landmarks: 2, rocks: true, ..Default::default() }
    }

    #[test]
    fn every_objective_has_its_bit_and_both_orders_list_them_all() {
        let bits: u32 = Objective::ALL.iter().map(|o| o.bit()).fold(0, |a, b| a | b);
        assert_eq!(bits.count_ones() as usize, Objective::ALL.len());
        for survival in [true, false] {
            let order = Objective::order(survival);
            assert_eq!(order.iter().map(|o| o.bit()).fold(0, |a, b| a | b), bits);
            for o in order {
                assert!(!o.title(survival).is_empty() && !o.how(survival).is_empty());
                assert_eq!(o.title(survival), o.title(survival).to_uppercase(), "the HUD is all caps");
            }
        }
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

    /// With the colony open, the ore brought home goes down the chain: the cap lift, the Exchange
    /// floor, a sale. With it closed, those don't show; and each counts done on foot, whenever it
    /// happens.
    #[test]
    fn the_chain_runs_down_into_the_colony_and_its_exchange() {
        let (mut o, mut downed) = (Objectives::default(), 0u32);
        let home = Objective::LandStation.bit()
            | Objective::Hide.bit()
            | Objective::Mine.bit()
            | Objective::Deliver.bit();
        let open = ObjectiveInput { colony: true, ..flying(true) };
        let mut done = home;
        assert_eq!(o.step(&mut done, &mut downed, &open), Some(Objective::RideDown));
        assert!(Objective::RideDown.on_foot() && !Objective::Mine.on_foot());
        let mut closed = home;
        assert_eq!(
            o.step(&mut closed, &mut downed, &flying(true)),
            Some(Objective::Bounty),
            "the colony's closed"
        );
        // Down the lift and into the city: on foot, not flying. The Proving Ground is first, off
        // the square.
        let walking = ObjectiveInput { flying: false, in_city: true, ..open };
        assert_eq!(o.step(&mut done, &mut downed, &walking), Some(Objective::ProvingGround));
        assert_eq!(Objective::ProvingGround.waypoint(), Waypoint::Place(proving_ground()));
        assert_eq!(PLACES[usize::from(proving_ground())].kind, PlaceKind::Proving);
        assert!(Objective::ProvingGround.on_foot());
        let reported = ObjectiveInput { at_proving: true, ..walking };
        assert_eq!(o.step(&mut done, &mut downed, &reported), Some(Objective::ExchangeFloor));
        assert_eq!(Objective::ExchangeFloor.waypoint(), Waypoint::Place(exchange()));
        assert_eq!(PLACES[usize::from(exchange())].kind, PlaceKind::Exchange);
        let there = ObjectiveInput { at_exchange: true, ..walking };
        assert_eq!(o.step(&mut done, &mut downed, &there), Some(Objective::Sell));
        let sold = ObjectiveInput { sales: 1, ..there };
        assert_eq!(o.step(&mut done, &mut downed, &sold), Some(Objective::Bounty));
        // A sale made from the bay's terminal before ever going down counts too.
        let mut early = home;
        o.step(&mut early, &mut downed, &ObjectiveInput { sales: 2, ..open });
        assert!(early & Objective::Sell.bit() != 0);
    }

    #[test]
    fn dolls_downed_add_up_across_sessions() {
        let (mut o, mut done, mut downed) = (Objectives::default(), 0u32, 0u32);
        let i = flying(false);
        assert_eq!(o.step(&mut done, &mut downed, &i), Some(Objective::Bounty));
        assert_eq!(
            o.step(&mut done, &mut downed, &ObjectiveInput { doll_kills: 3, ..i }),
            Some(Objective::Mine)
        );
        assert_eq!(downed, 3);
        assert_eq!(done & Objective::Ace.bit(), 0);
        // A new session's world counts from nothing again.
        o.step(&mut done, &mut downed, &ObjectiveInput { doll_kills: 0, ..i });
        o.step(&mut done, &mut downed, &ObjectiveInput { doll_kills: 2, ..i });
        assert_eq!(downed, 5);
        assert!(done & Objective::Ace.bit() != 0);
        // A kill that lands after the pilot is down (a missile still in flight) counts, once.
        o.step(&mut done, &mut downed, &ObjectiveInput { doll_kills: 3, flying: false, ..i });
        o.step(&mut done, &mut downed, &ObjectiveInput { doll_kills: 3, ..i });
        assert_eq!(downed, 6);
    }

    #[test]
    fn a_sector_without_landmarks_or_rocks_skips_what_needs_them() {
        let i = ObjectiveInput { survival: true, flying: true, ..Default::default() };
        assert_eq!(Objectives::current(0, &i), Some(Objective::Deliver));
        let all: u32 = Objective::ALL.iter().map(|o| o.bit()).sum();
        assert_eq!(Objectives::current(all, &flying(true)), None);
    }
}
