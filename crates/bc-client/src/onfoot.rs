//! On foot in the hangar bay (survival rules): the pilot's body and eyes, what they can reach and
//! use, and the sequences between the bay and space.
//!
//! - The pilot walks the bay's layout (`bc_client_core::walker`): W A S D, Shift to run, Space to
//!   jump; the mouse looks. What's in view and in reach can be used with E: a terminal opens its
//!   panel on the page, the airlock the menu (the way out of the bay is through it), and the
//!   cockpit boards the suit and launches it.
//! - Launching: the pilot climbs in and flight control is asked; the bay vents, its doors open
//!   and the catapult throws the suit down the launch tunnel into space, where the flight camera
//!   (chasing, or the cockpit) picks it up.
//! - Coming home (Enter, at rest inside the dock's ring of lights): the suit glides in down the
//!   tunnel, the doors shut behind it and the pilot climbs out onto the catwalk. After a suit is
//!   lost the pilot comes back in through the airlock, to an empty gantry.
//! - Into the colony, when it's open: the airlock leads to the cap lifts, one to each land strip
//!   (`bay::Lift`: E rides the Charter strip's, Q chooses the next), which ride down the end cap's
//!   face (the whole city in view) to that strip's Hub Gate. There the pilot walks the city's
//!   streets (`bc_client_core::city`), uses its places at their doors, and rides back up from Hub
//!   Gate.
//! - In the Blast Hall (the Proving Ground, `docs/TRAINING.md`): E at its desk opens its board,
//!   and E at its gantry's hatch boards one of the Charter Board's trainers there. Docked back on
//!   the gantry, the pilot climbs out at the hatch.
//!
//! The server knows none of this: only where the pilot is (`core.hangar.place`) and what they ask
//! for. Space skips a sequence.

use bc_client_core::bay::{CATWALK_Y, HATCH, Layout, Lift, SPAWN, SUIT_AT, Spot};
use bc_client_core::city::CityGround;
use bc_client_core::city_nav;
use bc_client_core::dock;
use bc_client_core::homes as homes_on_foot;
use bc_client_core::markab::{self, Chase, Markab};
use bc_client_core::sights;
use bc_client_core::tram::{self, CarInside, CityAndTrains, Rider};
use bc_client_core::vehicle::{Drive, Kind, Vehicle};
use bc_client_core::walker::{Guide, Stride, Walker};
use bc_econ::item::thousands;
use bc_econ::wire::{HomesView, Outcome, Place, Request};
use bc_econ::{Bay, Suit};
use bc_proto::presence::{PersonPose, RIDE_SEATED};
use bc_sim::colony::city::{
    AVENUE as AVENUE_WIDTH, CityBox, KERB, Stage, TERMINAL_HEIGHT, arrival_seats, district_at, place_door,
    seat_near, terminal_rect,
};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, local_frame, up_at};
use bc_sim::colony::hall;
use bc_sim::colony::homes::{self, Home, HomeId};
use bc_sim::colony::hub::BAY_RADIUS;
use bc_sim::colony::pools::{pool, pool_near};
use bc_sim::colony::transit::{
    CAR_WIDTH, CARS, DOOR_AT, FLOOR, PLATFORM_HALF, PLATFORM_LENGTH, STATION_GAP, STATIONS, TRAINS,
    TrainState, station_x, train,
};
use bc_sim::content::city::{DISTRICT_NAMES, PLACES, PlaceDef, PlaceKind, STRIP_NAMES};
use bc_sim::content::homes::Furniture;
use bc_sim::content::{Kit, Kits};
use bc_sim::world::COLONY_RADIUS;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::city::{CityView, RenderOrigin, colony_point};
use crate::dev_hooks::DevStatus;
use crate::hangar::{BAY_ORIGIN, BayState, DOOR_SECS, Indoors};
use crate::net::{GameClient, NetState, now_s};
use crate::page::{Panel, Ui, UiCmd, UiCmds};
use crate::pointer::PointerRes;
use crate::settings::SettingsRes;
use crate::terminal::TerminalLog;
use crate::view::ViewPrefs;

const SENSITIVITY: f32 = 0.0022;
/// Climbing into the cockpit, s (then the screen waits, dark, on flight control).
const BOARD_SECS: f32 = 1.0;
/// How long flight control has to answer, s.
const ANSWER_SECS: f32 = 8.0;
/// The bay vents and its doors open, s.
const VENT_SECS: f32 = DOOR_SECS + 1.3;
/// The klaxon sounds this long before the doors move, s.
const KLAXON_SECS: f32 = 0.8;
/// The catapult's run down the tunnel, s, and how hard it throws, m/s².
const CATAPULT_SECS: f32 = 3.2;
const CATAPULT_ACCEL: f32 = 49.0;
/// The suit lets go of its bay's cradle this long before the catapult's throw ends on screen (the
/// round trip to the server, so it's out of the door as the view cuts to it).
const RELEASE_LEAD: f32 = 0.35;
/// A homecoming: the suit glides in from this far down the tunnel, m, taking this long, s; the
/// doors start to close once it's through; the pilot climbs out at the end.
const ARRIVE_FROM: f32 = 170.0;
const GLIDE_SECS: f32 = 4.2;
const ARRIVE_SECS: f32 = 7.0;
/// Coming in through the airlock, s.
const ENTER_SECS: f32 = 1.6;
/// The screen fades back in over this long, s.
const REVEAL_SECS: f32 = 0.5;
/// The cap lift's ride down from the bay ring to Hub Gate, s (skippable), and how long the pilot
/// waits at Hub Gate for the lift back up before giving up, s.
const LIFT_SECS: f32 = 12.0;
const LIFT_UP_SECS: f32 = 6.0;
/// How near a place's door the pilot must stand to use it, m.
const DOOR_REACH: f32 = 3.5;
/// On foot in the city: walks the pilot to where the objective is.
pub const WALK_THERE_KEY: KeyCode = KeyCode::KeyG;
/// How near the gantry's hatch a pilot boards from, m (or anywhere round the pad).
const HATCH_REACH: f32 = 8.0;
/// How much walking a slow frame catches up, s. The pilot's own legs: a second's worth (no further,
/// blind, after a hitch). A guide's (the dev hooks' errands, the autopilot's): more, so its walk
/// across the city keeps to the clock on a page drawing it in software at a few seconds a frame
/// (held to a second a frame, a walk took several times as long, and a tram's open doors shut
/// before it reached them); and less than the shortest errand (Hub Gate's door to its motor pool,
/// 5.4 s), so none ends in the frame that begins it.
const OWN_CATCH_UP: f32 = 1.0;
const GUIDE_CATCH_UP: f32 = 4.0;

/// Where the pilot is in the bay's sequences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seq {
    /// On their own feet (or flying: nothing going on).
    #[default]
    Walking,
    /// Climbing into the cockpit, waiting on flight control.
    Boarding,
    /// The bay vents and its doors open.
    Venting,
    /// The catapult throws the suit down the tunnel.
    Catapult,
    /// Home: the suit glides in and the doors close behind it.
    Arriving,
    /// Coming in through the airlock.
    Entering,
    /// Riding the cap lift down the end cap into the colony.
    LiftDown,
    /// Riding the cap lift back up to the bay.
    LiftUp,
}

impl Seq {
    pub fn name(self) -> &'static str {
        match self {
            Seq::Walking => "walking",
            Seq::Boarding => "boarding",
            Seq::Venting => "venting",
            Seq::Catapult => "catapult",
            Seq::Arriving => "arriving",
            Seq::Entering => "entering",
            Seq::LiftDown => "lift_down",
            Seq::LiftUp => "lift_up",
        }
    }

    /// The view is in the bay while this goes on, wherever the pilot is.
    fn indoors(self) -> bool {
        self != Seq::Walking
    }

    fn skippable(self) -> bool {
        matches!(self, Seq::Venting | Seq::Catapult | Seq::Arriving | Seq::LiftDown)
    }
}

/// On foot in the colony's city: the strip, the pilot's body in its frame (`(x, h, −s)`), where
/// they're being walked to, and the place whose door they're at.
#[derive(Clone, Debug)]
pub struct CityFoot {
    pub strip: u8,
    pub walker: Walker,
    guide: Option<Guide>,
    pub focus: Option<usize>,
    /// The district they're in, and the sight they're at (both named on the way in).
    district: Option<u8>,
    sight: Option<usize>,
    /// Riding a tram (the walker is then in its car's frame), the trains of the strip's line
    /// this frame, and the station the pilot's train last stood at.
    pub ride: Option<Rider>,
    trains: [TrainState; TRAINS as usize],
    stood: Option<usize>,
    /// Driving (from a motor pool), and seen from behind (else from the driver's seat).
    pub drive: Option<Wheels>,
    chase: bool,
    /// Behind a Markab, the chase camera.
    cam: Option<Chase>,
    /// The colony's tick the trains are at.
    tick: (u32, f32),
    /// Sitting on one of The Arrival's seats (`bc_sim::colony::city::arrival_seats`).
    pub seat: Option<usize>,
    /// At the Blast Hall's gantry's hatch, facing the pad: a trainer can be boarded.
    pub gantry: bool,
    /// Riding a home building's lift (`bc_sim::colony::homes`): a cut, not a moving room.
    pub lift: Option<LiftRide>,
    /// The homes' shut doors near the pilot (the register's word, `bc_client_core::homes`): their
    /// leaves, solid in their doorways. And a door of theirs they've just worked, ahead of the
    /// register's word: which, open or shut, and when (Gates' `predict_door`).
    pub doors: Vec<CityBox>,
    door_ahead: Option<(u32, bool, f64)>,
    /// What the walker meets of the homes: the shut doors' leaves and the solid furniture.
    blocks: Vec<CityBox>,
    /// Build mode in a home of theirs (H2): the piece being put down, and where it would go.
    pub build: Option<Build>,
}

/// A piece of furniture being put down: which, how many quarter turns, and where it would go (the
/// home, its middle in dm from the home's floor's −s −x corner, the storey) and whether it fits
/// there (why not, if it doesn't).
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub kind: Furniture,
    pub turn: u8,
    pub at: Option<(HomeId, (u16, u16), u8)>,
    pub why: Option<&'static str>,
}

/// How far in front of the pilot's feet a piece being put down goes, m.
const BUILD_AHEAD: f32 = 1.8;

/// A ride in a home building's lift: to which storey, and since when (s, the page's clock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiftRide {
    pub to: u32,
    at: f64,
    there: bool,
}

/// A lift's ride: the screen goes dark, the pilot is put on the other floor halfway, and it comes
/// up again, s.
const LIFT_RIDE: f32 = 1.2;
const LIFT_PUT: f32 = 0.55;

/// Seated, the eye is this much lower than standing, m.
const SEATED_DROP: f32 = 0.55;
/// Taking a Markab from a motor pool.
const MARKAB_NEWS: &str = "A MARKAB FROM THE MOTOR POOL · ITS FAN HOLDS IT TO WALLS AND CEILINGS (F) · SPACE LIFT · SHIFT BOOST · Q/E ROLL IN THE AIR";

/// What a pilot drives in the city: a car or a scooter, or a Markab, the rocket car
/// (`bc_client_core::markab`).
#[derive(Clone, Copy, Debug)]
pub enum Wheels {
    Road(Vehicle),
    Markab(Markab),
}

impl Wheels {
    /// For the page and the dev hooks: "car", "scooter" or "markab".
    pub fn name(&self) -> &'static str {
        match self {
            Wheels::Road(v) if v.kind == Kind::Car => "car",
            Wheels::Road(_) => "scooter",
            Wheels::Markab(_) => "markab",
        }
    }

    /// How fast it goes, m/s.
    pub fn speed(&self) -> f32 {
        match self {
            Wheels::Road(v) => v.speed.abs(),
            Wheels::Markab(m) => m.speed(),
        }
    }

    /// Whether its driver can get out: slow (a Markab: and on a floor, not a wall or a ceiling).
    fn can_leave(&self) -> bool {
        match self {
            Wheels::Road(v) => v.speed.abs() < 3.0,
            Wheels::Markab(m) => m.can_leave(),
        }
    }

    /// The driver, as the plaza has them.
    pub fn pose(&self, pitch: f32) -> PersonPose {
        match self {
            Wheels::Road(v) => v.pose(pitch),
            Wheels::Markab(m) => m.pose(),
        }
    }

    /// Where its driver gets out, and the way they face.
    fn door(&self) -> (CityPos, Vec3) {
        match self {
            Wheels::Road(v) => (v.door(), Vec3::new(v.yaw.sin(), 0.0, v.yaw.cos())),
            Wheels::Markab(m) => {
                let f = m.forward();
                (m.door(), Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z))
            }
        }
    }

    /// Where it stands on strip `strip` (a Markab: the face under its middle).
    fn at(&self, strip: u8) -> CityPos {
        match self {
            Wheels::Road(v) => CityPos::new(strip, v.x, v.s, v.ground()),
            Wheels::Markab(m) => CityPos::from_walker(strip, m.pos - m.up() * markab::RIDE),
        }
    }
}

/// The line under the view while driving a Markab: its fan, its bottles, its speed, which way up.
fn markab_line(m: &Markab) -> String {
    let way = if !m.wheels_down() {
        " · IN THE AIR"
    } else if m.up().y < -0.5 {
        " · UPSIDE DOWN"
    } else if m.up().y < 0.5 {
        " · ON THE WALL"
    } else {
        ""
    };
    let out = if m.can_leave() { "E  GET OUT · " } else { "" };
    let fan = if m.fan { "ON" } else { "OFF" };
    // In fives, so the line doesn't change every frame.
    let five = |v: f32| (v / 5.0).round() * 5.0;
    format!("{out}FAN {fan} · GAS {:.0}% · {:.0} KM/H{way}", five(m.gas * 100.0), five(m.speed() * 3.6))
}

/// For the dev hooks: what a Markab's wheels are on.
fn markab_on(m: &Markab) -> &'static str {
    match m.on {
        markab::On::Air => "air",
        markab::On::Corner(_) => "corner",
        markab::On::Face(n, _) if n.y > 0.5 => "floor",
        markab::On::Face(n, _) if n.y < -0.5 => "ceiling",
        markab::On::Face(..) => "wall",
    }
}

/// Station `i` of strip `strip`'s line, by name: Hub Gate's, then the district it's in.
fn station_name(strip: u8, i: usize) -> String {
    if i == 0 {
        return "HUB GATE".into();
    }
    let k = strip as usize % 3;
    district_at(strip, station_x(i), Stage(0))
        .map_or_else(|| "THE BUILDING SITE".into(), |(d, _)| DISTRICT_NAMES[k][d as usize].to_string())
}

/// Strip `strip`'s Hub Gate.
fn hub_gate(strip: u8) -> &'static PlaceDef {
    PLACES.iter().find(|p| p.kind == PlaceKind::HubGate && p.strip == strip).unwrap_or(&PLACES[0])
}

impl CityFoot {
    /// Out of Hub Gate's terminal, facing down the colony.
    fn at_hub_gate(strip: u8) -> Self {
        let ((s, x), (ds, dx)) = place_door(hub_gate(strip));
        Self::at(strip, (s, x), (-ds, -dx))
    }

    /// Out of a trainer docked on the Blast Hall's gantry: at its hatch, facing the pad.
    fn at_hatch() -> Self {
        let (at, facing) = hall::hatch();
        Self::at(hall::hall().strip, at, facing)
    }

    /// Up from asleep in a bed at home (`Update::Place`'s wake): beside it, on its floor, facing
    /// into the home.
    fn at_bed(w: bc_econ::wire::Wake) -> Option<Self> {
        let home = bc_sim::colony::homes::home(HomeId(w.home))?;
        let (s, x) = (home.rect.s0 + f32::from(w.s) / 10.0, home.rect.x0 + f32::from(w.x) / 10.0);
        let (ms, mx) = home.rect.middle();
        let floor = bc_sim::content::homes::floor_of(&home, u32::from(w.storey));
        Some(Self::at_height(home.strip, (s, x), floor, (ms - s, mx - x)))
    }

    /// Standing at `(s, x)` on strip `strip`, facing `(ds, dx)`.
    fn at(strip: u8, at: (f32, f32), facing: (f32, f32)) -> Self {
        Self::at_height(strip, at, 0.0, facing)
    }

    /// Standing at `(s, x)` on strip `strip`, `h` up (on a home's floor), facing `(ds, dx)`.
    fn at_height(strip: u8, (s, x): (f32, f32), h: f32, (ds, dx): (f32, f32)) -> Self {
        let feet = CityPos::new(strip, x, s, h).walker();
        let away = CityPos::new(strip, x + dx, s + ds, h).walker() - feet;
        Self {
            strip,
            walker: Walker::at(feet, away.try_normalize().unwrap_or(Vec3::X)),
            guide: None,
            focus: None,
            district: None,
            sight: None,
            ride: None,
            trains: std::array::from_fn(|k| train(strip, k as u8, 0, 0.0)),
            stood: None,
            drive: None,
            chase: true,
            cam: None,
            tick: (0, 0.0),
            seat: None,
            gantry: false,
            lift: None,
            doors: Vec::new(),
            blocks: Vec::new(),
            door_ahead: None,
            build: None,
        }
    }

    /// Whether the pilot stands at the Blast Hall's gantry, by its hatch or round its pad, facing
    /// the pad (the trainer standing on it): E boards.
    fn hatch_in_view(&self) -> bool {
        if self.ride.is_some()
            || self.drive.is_some()
            || self.seat.is_some()
            || self.strip != hall::hall().strip
        {
            return false;
        }
        let ((s, x), _) = hall::hatch();
        let (ps, px) = hall::hall().front.point(hall::GANTRY.0, hall::GANTRY.1);
        let at = self.feet();
        let h = self.walker.heading();
        // Facing, in city terms: x along is the walker's x, s across its −z.
        let (hs, hx) = (-h.z, h.x);
        let (to_s, to_x) = (ps - at.s, px - at.x);
        let to_pad = to_s.hypot(to_x).max(1e-3);
        let near = (at.s - s).hypot(at.x - x) < HATCH_REACH || to_pad < hall::GANTRY_RADIUS + 4.0;
        near && (hs * to_s + hx * to_x) / to_pad > 0.2
    }

    /// The homes' doors as the register last said, near the pilot: the shut ones' leaves, and the
    /// pilot's own door worked a moment ago as they worked it, until the register agrees (or two
    /// seconds pass: it didn't).
    fn shut_doors(&mut self, view: &HomesView, now: f64) {
        if let Some((id, open, at)) = self.door_ahead {
            let agreed = homes_on_foot::line(view, HomeId(id)).is_some_and(|l| l.open == open);
            if agreed || now - at > 2.0 {
                self.door_ahead = None;
            }
        }
        self.doors = homes_on_foot::shut_doors(view, self.strip);
        if let Some((id, open, _)) = self.door_ahead
            && let Some(h) = bc_sim::colony::homes::home(HomeId(id))
        {
            let leaf = homes_on_foot::leaf(&h);
            self.doors.retain(|b| *b != leaf);
            if !open {
                self.doors.push(leaf);
            }
        }
        self.blocks.clear();
        self.blocks.extend_from_slice(&self.doors);
        self.blocks.extend(homes_on_foot::solid_pieces(view, self.strip));
    }

    /// The home whose door the pilot stands at (and whether they're in it): on foot, nothing else
    /// in hand.
    fn home_door(&self) -> Option<(Home, bool)> {
        if self.ride.is_some() || self.drive.is_some() || self.seat.is_some() || self.lift.is_some() {
            return None;
        }
        homes_on_foot::door_here(self.feet())
    }

    /// Build mode: where the piece would go, a little ahead of the pilot on the floor they stand
    /// on in a home of theirs, on the decimetre, and whether it fits there (`content::homes::fits`,
    /// the server's own rule). Out of the home, build mode ends.
    fn place_ahead(&mut self, view: &HomesView) {
        let Some(mut b) = self.build.clone() else { return };
        let Some(id) = self.home_in() else {
            self.build = None;
            return;
        };
        let (Some(home), Some(line)) = (bc_sim::colony::homes::home(id), homes_on_foot::line(view, id))
        else {
            self.build = None;
            return;
        };
        let at = self.feet();
        let h = self.walker.heading();
        // Ahead, in city terms: x along is the walker's x, s across its −z.
        let (s, x) = (at.s - h.z * BUILD_AHEAD, at.x + h.x * BUILD_AHEAD);
        let dm = |v: f32| (v * 10.0).round().clamp(0.0, f32::from(u16::MAX)) as u16;
        let (ds, dx) = (dm(s - home.rect.s0), dm(x - home.rect.x0));
        let storey = if home.kind == bc_sim::colony::homes::Kind::Houses {
            bc_sim::colony::homes::plan_at(at.strip, at.s, at.x).map_or(0, |p| p.storey_at(at.h + 0.5)) as u8
        } else {
            0
        };
        b.at = Some((id, (ds, dx), storey));
        let at_m = (f32::from(ds) / 10.0, f32::from(dx) / 10.0);
        let bx = bc_sim::content::homes::piece_box(&home, b.kind, at_m, b.turn, u32::from(storey));
        let (ms, mx) = home.rect.middle();
        b.why = match bc_sim::colony::homes::plan_at(home.strip, ms, mx) {
            Some(plan) => {
                let others = line.pieces.iter().map(|p| {
                    let o =
                        bc_sim::content::homes::piece_box(&home, p.kind, p.at(), p.turn, u32::from(p.storey));
                    (o, p.kind.spec().solid)
                });
                bc_sim::content::homes::fits(&home, &plan, b.kind, &bx, u32::from(storey), others).err()
            }
            None => Some("there's no home there"),
        };
        self.build = Some(b);
    }

    /// The home the pilot stands in, if they do.
    fn home_in(&self) -> Option<HomeId> {
        if self.ride.is_some() || self.drive.is_some() {
            return None;
        }
        let at = self.feet();
        bc_sim::colony::homes::home_at(at.strip, at.s, at.x, at.h + 1.0)
    }

    /// The home building whose lift the pilot stands at, and the storey they're on.
    fn lift_here(&self) -> Option<(homes::Plan, u32)> {
        if self.ride.is_some() || self.drive.is_some() || self.seat.is_some() || self.lift.is_some() {
            return None;
        }
        let at = self.feet();
        let plan = homes::plan_at(self.strip, at.s, at.x)?;
        Some((plan, plan.at_lift(at.s, at.x, at.h)?))
    }

    /// Rides the lift where the pilot stands to storey `to`, if there's one and it's another.
    fn ride_lift(&mut self, to: u32, now: f64) -> bool {
        match self.lift_here() {
            Some((plan, k)) if to != k && to < plan.storeys => {
                self.lift = Some(LiftRide { to, at: now, there: false });
                self.guide = None;
                true
            }
            _ => false,
        }
    }

    /// A lift's ride `now`: halfway, the pilot is on its floor `to`, out of its doors; then it's done.
    fn lifting(&mut self, now: f64) {
        let Some(mut r) = self.lift else { return };
        let t = (now - r.at) as f32;
        if !r.there && t >= LIFT_PUT {
            let at = self.feet();
            if let Some(((s, x), h)) = homes::plan_at(self.strip, at.s, at.x).and_then(|p| p.lift_spot(r.to))
            {
                self.walker.feet = CityPos::new(self.strip, x, s, h).walker();
                self.walker.vel = Vec3::ZERO;
            }
            r.there = true;
        }
        self.lift = (t < LIFT_RIDE).then_some(r);
    }

    /// How dark the lift's ride has the screen, 0..1.
    fn lift_dark(&self, now: f64) -> f32 {
        self.lift.map_or(0.0, |r| {
            let t = (now - r.at) as f32;
            smoothstep(0.0, LIFT_PUT - 0.1, t).min(1.0 - smoothstep(LIFT_PUT + 0.1, LIFT_RIDE, t))
        })
    }

    /// Walks the pilot to the Blast Hall's gantry's hatch (in through its blast doors), facing the
    /// pad.
    fn walk_to_gantry(&mut self) -> bool {
        let r = hall::hall();
        if self.strip != r.strip {
            return false;
        }
        let ((s, x), (ds, dx)) = hall::hatch();
        let at = self.feet();
        let walk = |(s, x): (f32, f32)| CityPos::new(r.strip, x, s, KERB).walker();
        let mut route = Vec::new();
        if !r.holds(at.s, at.x, at.h + 1.0) {
            let (outside, inside) = r.threshold();
            route = city_nav::route_from(r.strip, at, outside);
            route.push(walk(inside));
        }
        route.push(walk((s, x)));
        let facing = walk((s + ds, x + dx)) - walk((s, x));
        self.guide = Some(Guide::new(route, Some(facing.normalize())));
        true
    }

    /// Sits on seat `k`, facing out from it.
    fn sit(&mut self, k: usize) {
        let t = arrival_seats()[k];
        self.walker.feet = CityPos::new(self.strip, t.x, t.s, self.feet().h).walker();
        self.walker.vel = Vec3::ZERO;
        self.walker.yaw = t.yaw;
        self.guide = None;
        self.seat = Some(k);
    }

    /// Stands up off the seat, a step out in front of it.
    fn stand(&mut self) {
        if let Some(k) = self.seat.take() {
            let t = arrival_seats()[k];
            let (fs, fx) = (-t.yaw.cos(), t.yaw.sin());
            self.walker.feet =
                CityPos::new(self.strip, t.x + fx * 0.9, t.s + fs * 0.9, self.feet().h).walker();
        }
    }

    /// Walks the pilot to a home's door on the floor they're on (a dev hook's errand: `home:<id>`),
    /// out of the home they're in through its own door first, and on in through it to its middle
    /// (`home:<id>:in`): a neighbour's door along the corridor, not across the city.
    fn walk_to_home(&mut self, spot: &str) -> bool {
        let mut parts = spot.split(':').skip(1);
        let Some(to) = parts.next().and_then(|id| id.parse().ok()).and_then(|id| homes::home(HomeId(id)))
        else {
            return false;
        };
        let inside = parts.next() == Some("in");
        if to.strip != self.strip {
            return false;
        }
        let floor = |h: &Home| if h.kind == homes::Kind::Houses { KERB } else { h.floor };
        let at = |h: &Home, (s, x): (f32, f32)| CityPos::new(h.strip, x, s, floor(h)).walker();
        let mut route = Vec::new();
        if let Some(from) = self.home_in().and_then(homes::home) {
            route.extend([at(&from, from.inside), at(&from, from.outside)]);
        }
        route.push(at(&to, to.outside));
        if inside {
            route.extend([at(&to, to.inside), at(&to, to.rect.middle())]);
        }
        self.guide = Some(Guide::new(route, None));
        true
    }

    /// Walks the pilot to one of The Arrival's seats (a dev hook's errand), to sit on it.
    fn walk_to_seat(&mut self) -> bool {
        let t = arrival_seats()[0];
        if t.strip != self.strip {
            return false;
        }
        let at = self.feet();
        let facing = CityPos::new(self.strip, t.x + t.yaw.sin(), t.s - t.yaw.cos(), 0.0).walker()
            - CityPos::new(self.strip, t.x, t.s, 0.0).walker();
        self.guide =
            Some(Guide::new(city_nav::route_from(self.strip, at, (t.s, t.x)), Some(facing.normalize())));
        true
    }

    /// Takes a car or a scooter (`kind`), or a Markab (`None`), from the motor pool in reach, if
    /// there is one.
    fn take(&mut self, kind: Option<Kind>) -> bool {
        let at = self.feet();
        let Some(i) = pool_near(self.strip, at.s, at.x) else { return false };
        let (s, x) = pool(self.strip, i);
        // Parked facing up the avenue.
        let yaw = std::f32::consts::FRAC_PI_2;
        self.drive = Some(match kind {
            Some(kind) => Wheels::Road(Vehicle::new(kind, self.strip, s, x, yaw)),
            None => Wheels::Markab(Markab::new(self.strip, s, x, yaw)),
        });
        self.guide = None;
        self.cam = None;
        // From the scooter's deck there's nothing to see but the helmet: from behind.
        self.chase = self.chase || kind == Some(Kind::Scooter);
        true
    }

    /// Walks the pilot to the nearest motor pool along the avenue (the dev hooks').
    fn walk_to_pool(&mut self) -> bool {
        if self.drive.is_some() || self.ride.is_some() {
            return false;
        }
        let at = self.feet();
        let Some((s, x)) = (0..bc_sim::colony::pools::POOLS)
            .map(|i| pool(self.strip, i))
            .min_by(|a, b| (a.1 - at.x).abs().total_cmp(&(b.1 - at.x).abs()))
        else {
            return false;
        };
        let route = vec![
            CityPos::new(self.strip, at.x, s, 0.0).walker(),
            CityPos::new(self.strip, x, s, 0.0).walker(),
        ];
        self.guide = Some(Guide::new(route, None));
        true
    }

    /// Gets out of the vehicle, beside it.
    fn get_out(&mut self) {
        if let Some(v) = self.drive.take() {
            let (door, facing) = v.door();
            let mut w = Walker::at(door.walker(), facing);
            w.pitch = self.walker.pitch;
            self.walker = w;
            self.cam = None;
        }
    }

    /// Eases the chase camera behind a Markab `dt` seconds on (`markab::Chase`).
    fn follow_cam(&mut self, dt: f32) {
        let Some(Wheels::Markab(m)) = self.drive else {
            self.cam = None;
            return;
        };
        self.cam.get_or_insert_with(|| Chase::new(&m)).follow(&m, dt);
    }

    /// The line's trains at the colony's tick.
    fn time(&mut self, tick: u32, frac: f32) {
        self.trains = std::array::from_fn(|k| train(self.strip, k as u8, tick, frac));
        self.tick = (tick, frac);
    }

    /// The train the pilot rides, this frame.
    fn train(&self) -> Option<&TrainState> {
        self.ride.map(|r| &self.trains[r.k as usize])
    }

    /// Steps the pilot's body `h` seconds on: among the city's walls and its standing trains, or
    /// in their car. Getting on and off; what to tell them if they did.
    fn step(&mut self, stride: &Stride, h: f32) -> Option<String> {
        match (self.ride, self.train().copied()) {
            (Some(r), Some(t)) => {
                self.walker.step(&CarInside { open: t.doors, accel: t.accel }, stride, h);
                let out = tram::alighting(&r, self.walker.feet, &t)?;
                self.ride = None;
                self.walker.feet = out.walker();
                self.stood = None;
                None
            }
            _ => {
                let world =
                    CityAndTrains { ground: self.ground(), trains: &self.trains, doors: &self.blocks };
                self.walker.step(&world, stride, h);
                if let Some(back) = tram::rescue(self.feet()) {
                    self.walker.feet = back.walker();
                    self.walker.vel = Vec3::ZERO;
                    self.guide = None;
                }
                let (r, local) = tram::boarding(self.feet(), &self.trains)?;
                let t = self.trains[r.k as usize];
                self.ride = Some(r);
                self.walker.feet = local;
                self.guide = None;
                self.stood = t.at;
                let to = if t.dir > 0.0 { "THE BUILDING SITE" } else { "HUB GATE" };
                Some(format!("{} LINE · TO {to}", STRIP_NAMES[self.strip as usize % 3]))
            }
        }
    }

    /// Riding: the station the train has just pulled into, by name.
    fn arrived(&mut self) -> Option<String> {
        let at = self.train()?.at;
        if at == self.stood {
            return None;
        }
        self.stood = at;
        at.map(|i| station_name(self.strip, i))
    }

    /// Where the pilot is, as the plaza has them.
    fn pose(&self) -> PersonPose {
        if let Some(k) = self.seat {
            let t = arrival_seats()[k];
            let p = bc_client_core::city::pose_of(self.strip, &self.walker);
            return PersonPose {
                s: t.s,
                x: t.x,
                yaw: t.yaw,
                speed: 0.0,
                running: false,
                ride: RIDE_SEATED,
                ..p
            };
        }
        match (self.ride, self.drive) {
            (Some(r), _) => tram::pose_riding(&r, &self.walker, self.strip),
            (None, Some(v)) => v.pose(self.walker.pitch),
            (None, None) => bc_client_core::city::pose_of(self.strip, &self.walker),
        }
    }

    /// Walks the pilot onto the nearest station's platform and in through the nearest open door
    /// of a train standing there (or, none standing, to the platform's middle, to wait); riding,
    /// out of the nearest door onto the platform. False if there's nowhere to go.
    fn walk_to_tram(&mut self) -> bool {
        let mid = STRIP_WIDTH * 0.5;
        let at = |x: f32, s: f32, h: f32| CityPos::new(self.strip, x, s, h).walker();
        if let Some(t) = self.train().copied() {
            if !t.doors {
                return false;
            }
            // Out of the nearest door, to the platform's side (the avenue's middle).
            let x = self.walker.feet.x;
            let door = if (x - DOOR_AT).abs() < (x + DOOR_AT).abs() { DOOR_AT } else { -DOOR_AT };
            let out = t.dir * (0.5 * CAR_WIDTH + 1.2);
            let route = vec![Vec3::new(door, 0.0, 0.0), Vec3::new(door, 0.0, out)];
            self.guide = Some(Guide::new(route, None));
            return true;
        }
        let feet = self.feet();
        let i = ((feet.x - station_x(0)) / STATION_GAP).round().clamp(0.0, (STATIONS - 1) as f32) as usize;
        let sx = station_x(i);
        let half = 0.5 * PLATFORM_LENGTH;
        let on_platform = (feet.x - sx).abs() < half && (feet.s - mid).abs() < PLATFORM_HALF && feet.h > 0.5;
        let mut route = Vec::new();
        if !on_platform {
            // Along the avenue's middle to the nearer end of the platform, and up its steps.
            let end = if feet.x < sx { -1.0 } else { 1.0 };
            let foot = sx + end * (half + 2.0);
            if (feet.s - mid).abs() < 0.5 * AVENUE_WIDTH {
                route.push(at(feet.x, mid, 0.0));
            } else {
                route = city_nav::route_from(self.strip, feet, (mid - 20.0, foot));
            }
            route.push(at(foot, mid, 0.0));
            route.push(at(sx + end * (half - 5.0), mid, FLOOR));
        }
        let from = route.last().map_or(feet.x, |p| p.x);
        // A train whose doors will still be open by the time the pilot gets there (6 s on).
        let (tick, frac) = self.tick;
        let standing = self
            .trains
            .iter()
            .find(|t| t.doors && t.at == Some(i) && train(self.strip, t.k, tick + 180, frac).doors);
        match standing {
            Some(t) => {
                // The door nearest, and in through it.
                let door = (0..CARS)
                    .flat_map(|c| [t.car_x(c) - DOOR_AT, t.car_x(c) + DOOR_AT])
                    .min_by(|a, b| (a - from).abs().total_cmp(&(b - from).abs()))
                    .unwrap_or(t.x);
                route.push(at(door, mid, FLOOR));
                route.push(at(door, t.s, FLOOR));
            }
            None => route.push(at(sx, mid, FLOOR)),
        }
        self.guide = Some(Guide::new(route, None));
        true
    }

    /// What's newly reached: a district (its name, with its strip's), and a sight (`sights`).
    fn reached(&mut self) -> (Option<String>, Option<usize>) {
        let at = self.feet();
        let k = self.strip as usize % 3;
        let district = district_at(self.strip, at.x, Stage(0)).map(|(d, _)| d);
        let sight = sights::reached(self.strip, at.s, at.x);
        let mut news = (None, None);
        if district != self.district {
            self.district = district;
            news.0 = district.map(|d| format!("{} · {}", DISTRICT_NAMES[k][d as usize], STRIP_NAMES[k]));
        }
        if sight != self.sight {
            self.sight = sight;
            news.1 = sight;
        }
        news
    }

    fn ground(&self) -> CityGround {
        CityGround { strip: self.strip, stage: Stage(0) }
    }

    /// Where the pilot stands, in city coordinates (riding: where their car has them; driving:
    /// where the vehicle is).
    pub fn feet(&self) -> CityPos {
        if let Some(v) = self.drive {
            return v.at(self.strip);
        }
        match (self.ride, self.train()) {
            (Some(r), Some(t)) => tram::in_city(&r, self.walker.feet, t),
            _ => CityPos::from_walker(self.strip, self.walker.feet),
        }
    }

    /// The eye, in the colony's frame, the way it looks, and its up.
    fn view(&self) -> (DVec3, Vec3, Vec3) {
        let upright = |eye: DVec3| up_at(eye.as_vec3());
        if let Some(Wheels::Markab(m)) = self.drive {
            return self.markab_view(&m);
        }
        if let Some(Wheels::Road(v)) = self.drive {
            let (fx, fs) = v.forward();
            let g = v.ground();
            let at = |back: f32, up: f32| {
                colony_point(CityPos::new(self.strip, v.x - fx * back, v.s - fs * back, g + up))
            };
            if self.chase {
                // Behind and above it, looking a little ahead of it.
                let (eye, ahead) = (at(7.5, 2.8), at(-4.0, 1.0));
                return (eye, (ahead - eye).as_vec3().normalize_or(Vec3::X), upright(eye));
            }
            let eye = CityPos::new(self.strip, v.x - fx * 0.3, v.s - fs * 0.3, g + v.spec().eye);
            let look =
                Vec3::new(v.yaw.sin(), self.walker.pitch.clamp(-0.4, 0.3).sin(), v.yaw.cos()).normalize();
            let frame = local_frame(self.strip, eye.s);
            let eye = colony_point(eye);
            return (eye, frame * look, upright(eye));
        }
        let mut eye = match (self.ride, self.train()) {
            (Some(r), Some(t)) => tram::in_city(&r, self.walker.eye(), t),
            _ => CityPos::from_walker(self.strip, self.walker.eye()),
        };
        if self.seat.is_some() {
            eye.h -= SEATED_DROP;
        }
        let at = colony_point(eye);
        (at, local_frame(self.strip, eye.s) * self.walker.look(), upright(at))
    }

    /// A Markab's view: from behind it (`follow_cam`), upright to the face it's on; or from its
    /// seat, turning with it, upside down on a ceiling and round in a roll.
    fn markab_view(&self, m: &Markab) -> (DVec3, Vec3, Vec3) {
        let point = |p: Vec3| colony_point(CityPos::from_walker(self.strip, p));
        let frame = local_frame(self.strip, -m.pos.z);
        if self.chase {
            let (eye, at, up) = self.cam.unwrap_or_else(|| Chase::new(m)).view(m);
            let (eye, at) = (point(eye), point(at));
            return (eye, (at - eye).as_vec3().normalize_or(Vec3::X), frame * up);
        }
        let (above, back) = markab::EYE;
        let eye = point(m.pos + m.up() * above - m.forward() * back);
        let pitch = self.walker.pitch.clamp(-0.4, 0.3);
        let look = Quat::from_axis_angle(m.left(), -pitch) * m.forward();
        (eye, frame * look, frame * m.up())
    }

    /// The place the pilot can use where they stand, facing it: at its counter, in the room behind
    /// its door (`bc_sim::colony::city::Room`), or at Hub Gate's door.
    fn door_in_view(&self) -> Option<usize> {
        if self.ride.is_some() || self.drive.is_some() || self.seat.is_some() {
            return None;
        }
        let at = self.feet();
        let h = self.walker.heading();
        // Facing, in city terms: x along is the walker's x, s across its −z.
        let (hs, hx) = (-h.z, h.x);
        PLACES.iter().enumerate().filter(|(_, p)| p.strip == self.strip).find_map(|(i, _)| {
            let ((s, x), (ds, dx)) = city_nav::use_spot(i);
            let near = (at.s - s).hypot(at.x - x) < DOOR_REACH;
            (near && hs * ds + hx * dx > 0.3).then_some(i)
        })
    }

    /// Walks the pilot to a place's door, facing in; or (`sight_<i>`) to where sight `i` is seen
    /// from the street; or (`gantry`) to the Blast Hall's gantry's hatch.
    fn walk_to(&mut self, slug: &str) -> bool {
        if slug == "gantry" {
            return self.walk_to_gantry();
        }
        if let Some(i) = slug.strip_prefix("sight_").and_then(|i| i.parse::<usize>().ok()) {
            if i >= bc_sim::content::city::SIGHTS.len() || sights::sight_at(i).0 != self.strip {
                return false;
            }
            let at = self.feet();
            self.guide = Some(Guide::new(city_nav::route_from(self.strip, at, sights::stand(i)), None));
            return true;
        }
        let Some((i, p)) = bc_sim::colony::city::place(slug) else { return false };
        if p.strip != self.strip {
            return false;
        }
        let ((s, x), (ds, dx)) = city_nav::use_spot(i);
        let facing = CityPos::new(self.strip, x + dx, s + ds, 0.0).walker()
            - CityPos::new(self.strip, x, s, 0.0).walker();
        let route = city_nav::route_to_place(self.strip, self.feet(), i);
        self.guide = Some(Guide::new(route, Some(facing.normalize())));
        true
    }
}

/// The cap lift's car `t` seconds into its ride down strip `strip`'s end cap: the eye in the
/// colony's frame, and the way it looks (down the colony, tipping up as it nears the floor).
fn lift_view(strip: u8, t: f32) -> (DVec3, Vec3) {
    let u = smoothstep(0.0, LIFT_SECS, t);
    let (top, bottom) = (COLONY_RADIUS - BAY_RADIUS, TERMINAL_HEIGHT + 6.0);
    let h = top + (bottom - top) * u;
    let x = terminal_rect().x0 + 7.0;
    let s = STRIP_WIDTH * 0.5;
    let pitch = (-0.5 + 0.38 * u).clamp(-0.6, 0.0);
    let look = Vec3::new(pitch.cos(), pitch.sin(), 0.0);
    (colony_point(CityPos::new(strip, x, s, h)), local_frame(strip, s) * look)
}

/// The pilot on foot.
#[derive(Resource)]
pub struct OnFoot {
    layout: Layout,
    pub walker: Walker,
    /// Walking the pilot somewhere (a dev hook, or the autopilot).
    guide: Option<Guide>,
    /// What the pilot could use, where they stand and look.
    pub focus: Option<Spot>,
    pub seq: Seq,
    /// When the sequence began (the page's clock, s).
    since: f64,
    /// Where the pilot was, as last seen.
    place: Option<Place>,
    /// How the last sortie ended: it decides how the pilot comes home.
    outcome: Option<Outcome>,
    /// The suit being launched (the bay says it's out as soon as it is).
    launching: Option<Suit>,
    /// The eye and where it looked when boarding began (the camera eases away from them).
    from: (Vec3, Vec3),
    /// The screen fades in from black from this moment.
    reveal: Option<f64>,
    /// The airlock's door stands open until then.
    airlock_until: f64,
    /// The cap lift E at the airlock rides down (Q chooses the next strip's).
    lift: Lift,
    /// On foot in the colony (or riding down to it).
    pub city: Option<CityFoot>,
}

impl Default for OnFoot {
    fn default() -> Self {
        let layout = Layout::new();
        Self {
            layout,
            walker: Self::at_the_airlock(),
            guide: None,
            focus: None,
            seq: Seq::Walking,
            since: 0.0,
            place: None,
            outcome: None,
            launching: None,
            from: (Vec3::ZERO, Vec3::Z),
            reveal: None,
            airlock_until: 0.0,
            lift: Lift::default(),
            city: None,
        }
    }
}

impl OnFoot {
    /// Just in from the airlock, looking at the suit.
    fn at_the_airlock() -> Walker {
        Walker::at(SPAWN, (SUIT_AT - SPAWN).with_y(0.0).normalize())
    }

    fn start(&mut self, seq: Seq, now: f64) {
        self.seq = seq;
        self.since = now;
        self.guide = None;
    }

    /// Out of the sequence, on foot or flying, the screen fading back in.
    fn done(&mut self, now: f64) {
        self.seq = Seq::Walking;
        self.reveal = Some(now);
        self.launching = None;
    }

    fn enter(&mut self, now: f64) {
        self.walker = Self::at_the_airlock();
        self.start(Seq::Entering, now);
        self.airlock_until = now + f64::from(ENTER_SECS) * 0.5;
    }

    /// Out of the cockpit onto the catwalk, turned towards the stairs down.
    fn climb_out(&mut self) {
        self.walker = Walker::at(Vec3::new(0.8, CATWALK_Y, 1.9), -Vec3::X);
    }

    /// Until when the airlock's door stands open (it opens when the pilot comes in or leaves).
    pub fn airlock_until(&self) -> f64 {
        self.airlock_until
    }

    fn t(&self, now: f64) -> f32 {
        (now - self.since) as f32
    }

    /// Where the suit is in a sequence, from its place in the gantry, and how fast it's going.
    fn suit_motion(&self, now: f64) -> (Vec3, Vec3) {
        let t = self.t(now);
        match self.seq {
            Seq::Catapult => (Vec3::Z * (-0.5 * CATAPULT_ACCEL * t * t), Vec3::Z * (-CATAPULT_ACCEL * t)),
            Seq::Arriving => {
                // Easing out: the retros burn it down to rest in the gantry.
                let u = (t / GLIDE_SECS).clamp(0.0, 1.0);
                let left = (1.0 - u).powi(3);
                let speed = 3.0 * (1.0 - u).powi(2) * ARRIVE_FROM / GLIDE_SECS;
                (Vec3::Z * (-ARRIVE_FROM * left), Vec3::Z * speed)
            }
            _ => (Vec3::ZERO, Vec3::ZERO),
        }
    }

    /// The camera, in the bay's frame: where it is, and what it looks at.
    fn view(&self, now: f64) -> (Vec3, Vec3) {
        let t = self.t(now);
        let eye = self.walker.eye();
        let first_person = (eye, eye + self.walker.look());
        // The whole bay from its back corner, the suit below and the doors beyond, pushing in.
        let corner = |t: f32| {
            (Vec3::new(12.5, 21.0, 22.0) - Vec3::new(0.35, 0.2, 0.6) * t, Vec3::new(-1.5, 7.0, -14.0))
        };
        let (offset, _) = self.suit_motion(now);
        let suit = SUIT_AT + offset;
        match self.seq {
            // (The lifts are seen from the colony, `onfoot_camera`.)
            Seq::Walking | Seq::Entering | Seq::LiftDown | Seq::LiftUp => first_person,
            Seq::Boarding => {
                let u = smoothstep(0.0, BOARD_SECS, t);
                let (from, look) = self.from;
                let to = HATCH + Vec3::new(0.0, 0.2, 0.6);
                (from.lerp(to, u), (from + look).lerp(to + Vec3::Z, u))
            }
            Seq::Venting => corner(t),
            Seq::Catapult => {
                // From the corner to chasing it down the tunnel.
                let u = smoothstep(0.0, 1.0, t);
                let chase = (suit + Vec3::new(2.5, 5.5, 17.0), suit + Vec3::new(0.0, 3.0, -30.0));
                let (eye, at) = corner(VENT_SECS);
                (eye.lerp(chase.0, u), at.lerp(chase.1, u))
            }
            // By the exchange terminal, watching it come in through the doors.
            Seq::Arriving => (Vec3::new(-13.5, 2.5, -21.0), suit + Vec3::Y * 4.0),
        }
    }

    /// How dark the screen is, 0..1.
    fn curtain(&self, now: f64) -> f32 {
        let t = self.t(now);
        let dark = match self.seq {
            Seq::Walking => 0.0,
            Seq::Boarding => smoothstep(BOARD_SECS * 0.55, BOARD_SECS, t),
            Seq::Venting => 1.0 - smoothstep(0.0, REVEAL_SECS, t),
            Seq::Catapult => smoothstep(CATAPULT_SECS - 0.35, CATAPULT_SECS, t),
            Seq::Arriving => smoothstep(ARRIVE_SECS - 0.7, ARRIVE_SECS, t),
            Seq::Entering => 1.0 - smoothstep(0.0, ENTER_SECS * 0.6, t),
            // Out of the airlock into the car (a cut), and out of the car at Hub Gate.
            Seq::LiftDown => (1.0 - smoothstep(0.3, 1.1, t)).max(smoothstep(LIFT_SECS - 0.5, LIFT_SECS, t)),
            Seq::LiftUp => smoothstep(0.0, 0.6, t),
        };
        let reveal = self.reveal.map_or(0.0, |at| 1.0 - smoothstep(0.0, REVEAL_SECS, (now - at) as f32));
        dark.max(reveal)
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A full-screen black veil over the game (and under the page) for the sequences' cuts.
#[derive(Component)]
pub struct Curtain;

pub fn setup_onfoot(mut commands: Commands) {
    commands.spawn((
        Curtain,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(100),
    ));
}

/// Sends a request to the hangar.
fn ask(net: &NetState, game: &crate::net::Game, req: &Request) {
    if let Some(t) = net.get() {
        t.send_control(game.core.request(req));
    }
}

/// Whether a launch sequence at `t` seconds into `seq` still holds the suit in its bay's cradle:
/// until the catapult's throw ends, a moment before the cut to space.
fn holds_the_bay(seq: Seq, t: f32) -> bool {
    match seq {
        Seq::Boarding | Seq::Venting => true,
        Seq::Catapult => t < CATAPULT_SECS - RELEASE_LEAD,
        _ => false,
    }
}

/// What using a place is called on the prompt (`lift`: the cap lift the airlock's E rides).
fn verb(spot: Spot, bay: Option<&Bay>, colony: bool, lift: Lift) -> String {
    match (spot, bay) {
        (Spot::Cockpit, Some(Bay::Docked { .. })) if colony => {
            "LAUNCH INTO SPACE · Q  LAUNCH INTO THE COLONY, BY THE INNER GATE".into()
        }
        (Spot::Cockpit, Some(Bay::Docked { .. })) => "BOARD & LAUNCH INTO SPACE".into(),
        (Spot::Cockpit, Some(Bay::Out { .. })) => "COCKPIT: YOUR SUIT IS OUT".into(),
        (Spot::Cockpit, _) => "COCKPIT: THE GANTRY IS EMPTY".into(),
        (Spot::Airlock, _) if colony => lift.prompt(),
        (Spot::Airlock, _) => "AIRLOCK: LEAVE THE BAY · THE CAP LIFTS ARE CLOSED".into(),
        (spot, _) => spot.name().into(),
    }
}

/// What using a place in the city is called on the prompt: its name, and the one thing only its
/// desk does.
fn city_verb(p: &PlaceDef) -> String {
    match p.kind {
        PlaceKind::HubGate => "THE CAP LIFT, UP TO YOUR BAY".into(),
        PlaceKind::Exchange => "THE EXCHANGE FLOOR · ORDERS THAT WAIT ON THE BOOK".into(),
        PlaceKind::Charter => "THE CHARTER BOARD · CONTRACTS, PATROLS AND THE CHARTER".into(),
        PlaceKind::Bar => "THE ARRIVAL · MEALS AND THE MOST WANTED".into(),
        PlaceKind::Proving => "THE PROVING GROUND · THE BOARD AND THE TEST RANGE".into(),
    }
}

/// Once a frame, in game mode: the hangar's news, where the pilot is, their legs and eyes, what
/// they use, and the sequences; then what the bay and the page show.
#[allow(clippy::too_many_arguments)]
pub fn drive_onfoot(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    cmds: Res<UiCmds>,
    pointer: Res<PointerRes>,
    mut settings: ResMut<SettingsRes>,
    time: Res<Time<Real>>,
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    mut ui: ResMut<Ui>,
    mut me: ResMut<OnFoot>,
    mut indoors: ResMut<Indoors>,
    mut bay: ResMut<BayState>,
    mut log: ResMut<TerminalLog>,
    mut curtain: Query<&mut BackgroundColor, With<Curtain>>,
    mut city_view: ResMut<CityView>,
    (map, enter): (Res<crate::map::MapOpen>, Res<crate::chat::Enter>),
) {
    let now = now_s();
    let dt = time.delta_secs().min(0.1);
    // Walking keeps up with slow frames (`OWN_CATCH_UP`, a guide's `GUIDE_CATCH_UP`).
    let walk_dt = |guided: bool| time.delta_secs().min(if guided { GUIDE_CATCH_UP } else { OWN_CATCH_UP });
    let mut g = game.borrow_mut();
    let autopilot = g.autopilot;

    // What the server had to say.
    let mut refused = false;
    for (text, ok) in g.core.hangar.notes.drain(..) {
        refused |= !ok;
        ui.toast(if ok { text.clone() } else { format!("CAN'T: {}", text.to_uppercase()) });
        log.push(text, ok);
    }
    for (outcome, text) in g.core.hangar.sorties.drain(..) {
        me.outcome = Some(outcome);
        ui.news(text.clone(), outcome == Outcome::Lost);
        log.push(text, outcome != Outcome::Lost);
    }
    // Its payout sheet, under the news, and line by line in the terminals' log.
    for d in g.core.hangar.debriefs.drain(..) {
        ui.debrief(&d);
        let lines: Vec<String> = d.lines.iter().map(|l| format!("{} {:+} CR", l.what, l.cr)).collect();
        log.push(format!("DEBRIEF · {} · NET {:+} CR", lines.join(" · "), d.net()), d.net() >= 0);
    }
    for text in g.core.hangar.news.drain(..) {
        ui.news(text.clone(), false);
        log.push(text, true);
    }

    // Where the pilot is.
    let place = g.core.hangar.place;
    if place != me.place {
        let before = me.place;
        match (before, place) {
            // A new session (or the link was reset): start afresh.
            (_, None) => *me = OnFoot::default(),
            // Aboard a trainer at the Blast Hall's gantry (the one the test range picked): flying it,
            // off the street.
            (Some(Place::City), Some(Place::Space)) if g.core.hangar.trainer => {
                me.city = None;
                me.done(now);
                let name = g.core.hangar.trainer_name();
                ui.news(
                    format!(
                        "ABOARD A TRAINER · {name} · WEAPONS FREE IN THE HALL · DOCK AT REST ON ITS GANTRY"
                    ),
                    false,
                );
                ui.toast(
                    "TRAINING · THE BLAST HALL: put the crosshair on the target pulsing cyan (its ◆) and fire, LMB the rifle, RMB the cannon. The first strike starts the clock.",
                );
            }
            // Its trainer docked on the gantry: out at the hatch, on foot in the hall.
            (Some(Place::Space), Some(Place::City)) if g.core.hangar.off_a_trainer() => {
                me.city = Some(CityFoot::at_hatch());
                me.done(now);
            }
            (Some(Place::Hangar), Some(Place::Space)) if me.seq == Seq::Boarding => {
                me.start(Seq::Venting, now);
                if g.core.welcome.is_some_and(|w| w.interior) {
                    ui.news(
                        "INTO THE COLONY · WEAPONS SAFE BY ITS LAW · DOCK AT REST IN THE INNER GATE'S RING",
                        false,
                    );
                }
            }
            (Some(Place::Space), Some(Place::Hangar)) if me.outcome == Some(Outcome::Docked) => {
                me.climb_out();
                me.start(Seq::Arriving, now);
            }
            (_, Some(Place::Hangar)) => {
                me.city = None;
                me.enter(now);
            }
            // Down in the colony (the lift's already on its way, unless it was sent some other way);
            // or back, waking at home beside the bed they were asleep in.
            (_, Some(Place::City)) => {
                if let Some(w) = g.core.hangar.wake.take()
                    && let Some(c) = CityFoot::at_bed(w)
                {
                    me.city = Some(c);
                    me.done(now);
                    ui.news("AWAKE AT HOME", false);
                } else if me.city.is_none() {
                    let strip = g.core.hangar.strip.unwrap_or(0);
                    me.city = Some(CityFoot::at_hub_gate(strip));
                }
            }
            // Woke in the suit they'd left out there, or launched by other means: flying.
            (_, Some(Place::Space)) => {
                if me.seq.indoors() {
                    me.done(now);
                }
            }
        }
        me.place = place;
    }

    // The sequences.
    let t = me.t(now);
    let skip =
        (keys.just_pressed(KeyCode::Space) && ui.playing() && !ui.panel_open()) || cmds.has(&UiCmd::Skip);
    match me.seq {
        Seq::Boarding if refused => me.seq = Seq::Walking,
        Seq::Boarding if t > ANSWER_SECS => {
            me.seq = Seq::Walking;
            ui.toast("FLIGHT CONTROL DIDN'T ANSWER");
        }
        s if s.skippable() && skip => {
            if s == Seq::Arriving {
                me.climb_out();
            }
            me.done(now);
        }
        Seq::Venting if t > VENT_SECS => me.start(Seq::Catapult, now),
        Seq::Catapult if t > CATAPULT_SECS => me.done(now),
        Seq::Arriving if t > ARRIVE_SECS => me.done(now),
        Seq::Entering if t > ENTER_SECS => me.seq = Seq::Walking,
        Seq::LiftDown if t > LIFT_SECS => me.done(now),
        Seq::LiftDown if refused => {
            me.city = None;
            me.done(now);
        }
        Seq::LiftUp if t > LIFT_UP_SECS || refused => {
            if !refused {
                ui.toast("THE LIFT DIDN'T COME");
            }
            me.done(now);
        }
        _ => {}
    }
    // The catapult fires as its throw ends on screen (or the launch is skipped, refused, or over):
    // the suit lets go of its bay's cradle and is thrown out of the door.
    if g.bay_hold && !holds_the_bay(me.seq, me.t(now)) {
        g.bay_hold = false;
    }
    let inside = me.seq.indoors() || matches!(place, Some(Place::Hangar | Place::City));
    if indoors.0 != inside {
        indoors.0 = inside;
    }

    // On foot: legs, eyes and hands.
    let on_foot = me.seq == Seq::Walking && place == Some(Place::Hangar) && me.city.is_none();
    let in_city = me.seq == Seq::Walking && place == Some(Place::City) && me.city.is_some();
    let live = (on_foot || in_city) && ui.playing() && !ui.panel_open();
    let colony = g.core.welcome.as_ref().is_some_and(|w| w.colony);
    // The pilot's own legs and eyes this frame: keys and the mouse.
    let mut own = Stride::default();
    let mut look = Vec2::ZERO;
    if live {
        if pointer.0.flying() && motion.delta != Vec2::ZERO {
            look = motion.delta * SENSITIVITY * settings.0.sensitivity;
            if settings.0.invert_y {
                look.y = -look.y;
            }
        }
        // The dev hook's turn, as the mouse's.
        look += cmds.look();
        let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
        own = Stride {
            forward: axis(KeyCode::KeyW, KeyCode::KeyS),
            right: axis(KeyCode::KeyD, KeyCode::KeyA),
            run: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
            jump: keys.pressed(KeyCode::Space),
        };
    }
    let used_key = (live && keys.just_pressed(KeyCode::KeyE)) || cmds.has(&UiCmd::Use);
    let mut board_trainer = false;
    if in_city && let Some(c) = me.city.as_mut() {
        // This frame's trains first, so a walk to the tram sees which doors are open now (at a few
        // frames a second, the last frame's can have shut since).
        let (tick, frac) = g.core.colony_tick(now);
        c.time(tick, frac);
        for cmd in &cmds.0 {
            if let UiCmd::WalkTo { spot: slug } = cmd {
                match slug.as_str() {
                    "tram" => c.walk_to_tram(),
                    "pool" => c.walk_to_pool(),
                    "seat" => c.walk_to_seat(),
                    _ if slug.starts_with("home:") => c.walk_to_home(slug),
                    _ => c.walk_to(slug),
                };
            }
        }
        // G: walked to where the objective is (its place's counter), on this strip.
        if live
            && keys.just_pressed(WALK_THERE_KEY)
            && let Some(goal) = ui.map_goal
        {
            if c.walk_to(goal) {
                ui.toast("WALKING YOU THERE · any move key takes your feet back");
            } else {
                ui.toast("THAT'S ON ANOTHER STRIP: ride Hub Gate's cap lift up and take that strip's (Q at the airlock)");
            }
        }
        if look != Vec2::ZERO {
            c.walker.turn(-look.x, -look.y);
        }
        if own != Stride::default() {
            c.guide = None;
        }
        // Driving: W/S the throttle and the brake, A/D the wheel, Space the handbrake. A Markab:
        // Space its lift jets, Shift its tail's, Q/E a roll in the air, X the handbrake, F its fan.
        let mut got_out = false;
        if let Some(w) = c.drive.as_mut() {
            let mut left = walk_dt(false);
            match w {
                Wheels::Road(v) => {
                    let drive = Drive { throttle: own.forward, steer: -own.right, handbrake: own.jump };
                    while left > 1e-4 {
                        let h = left.min(0.05);
                        left -= h;
                        v.step(&drive, h);
                    }
                }
                Wheels::Markab(m) => {
                    let held = |k: KeyCode| live && keys.pressed(k);
                    let ctl = markab::Controls {
                        throttle: own.forward,
                        steer: -own.right,
                        roll: (held(KeyCode::KeyQ) as i32 - held(KeyCode::KeyE) as i32) as f32,
                        lift: own.jump,
                        boost: own.run,
                        handbrake: held(KeyCode::KeyX),
                    };
                    if live && keys.just_pressed(KeyCode::KeyF) {
                        m.toggle_fan();
                        ui.toast(if m.fan {
                            "THE FAN ON · IT HOLDS TO WHATEVER ITS WHEELS ARE ON: WALLS, CEILINGS"
                        } else {
                            "THE FAN OFF · A CAR AGAIN: WALLS ARE WALLS, AND NOTHING HOLDS IT TO ONE"
                        });
                    }
                    while left > 1e-4 {
                        let h = left.min(0.05);
                        left -= h;
                        m.step(&ctl, h);
                    }
                }
            }
            if live && keys.just_pressed(KeyCode::Tab) && w.name() != "scooter" {
                c.chase = !c.chase;
            }
            if used_key && w.can_leave() {
                c.get_out();
                got_out = true;
            }
        }
        c.follow_cam(dt);
        // Seated: put, till a step or E gets them up, a step out in front of the seat.
        let mut stood_up = false;
        if c.seat.is_some() && (own != Stride::default() || used_key) {
            c.stand();
            stood_up = true;
        }
        // A lift's ride: put on the other floor halfway through it, and still till it's over.
        c.lifting(now);
        let still = c.drive.is_some() || c.seat.is_some() || c.lift.is_some();
        let mut left = if still { 0.0 } else { walk_dt(c.guide.is_some()) };
        while left > 1e-4 {
            let h = left.min(0.1);
            left -= h;
            // The guide's stride while it walks them; once it's done, or a tram's door has taken
            // them aboard (and the guide with it), their own: not the guide's last, which would
            // walk them on across the car and out of its far door for the rest of a long frame.
            let stride = match c.guide.as_mut() {
                Some(guide) => {
                    let stride = guide.steer(&mut c.walker, h);
                    if guide.arrived() && guide.facing.is_none_or(|f| c.walker.heading().dot(f) > 0.97) {
                        c.guide = None;
                    }
                    stride
                }
                None => own,
            };
            if let Some(news) = c.step(&stride, h) {
                ui.toast(news);
            }
        }
        if let Some(name) = c.arrived() {
            ui.toast(name);
        }
        c.focus = c.door_in_view();
        // At a home building's lift: E a floor up, Q one down, a digit that floor (0 the ground).
        // (`rode`: E went to the lift or, below, to a home's door or panel.)
        let mut rode = false;
        if c.focus.is_none()
            && !got_out
            && !stood_up
            && let Some((plan, k)) = c.lift_here()
        {
            let digits = [
                KeyCode::Digit0,
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9,
            ];
            let mut to = digits.iter().position(|d| live && keys.just_pressed(*d)).map(|i| i as u32);
            for cmd in &cmds.0 {
                if let UiCmd::Lift { storey: n } = cmd {
                    to = Some(*n);
                }
            }
            if used_key {
                to = Some(if k + 1 < plan.storeys { k + 1 } else { 0 });
            } else if live && keys.just_pressed(KeyCode::KeyQ) && k > 0 {
                to = Some(k - 1);
            }
            if let Some(to) = to {
                rode = c.ride_lift(to, now);
                if !rode && to >= plan.storeys {
                    ui.toast(format!("THIS LIFT GOES NO HIGHER THAN FLOOR {}", plan.storeys - 1));
                }
            }
        }
        // The homes: the doors the register says are shut, and E at a home's door (works it, if its
        // lock lets the pilot by; else its panel: the keypad and a knock, or what it'd cost) or in a
        // home of theirs (its panel).
        c.shut_doors(&g.core.hangar.homes, now);
        // Build mode (from a home's panel): R turns the piece, E puts it down where it fits, Q stops.
        for cmd in &cmds.0 {
            if let UiCmd::Place { furniture: slug } = cmd
                && let Some(kind) = Furniture::from_slug(slug)
            {
                c.build = Some(Build { kind, turn: 0, at: None, why: None });
            }
        }
        c.place_ahead(&g.core.hangar.homes);
        if let Some(b) = c.build.as_mut() {
            if live && keys.just_pressed(KeyCode::KeyR) {
                b.turn = (b.turn + 1) % 4;
            }
            if live && keys.just_pressed(KeyCode::KeyQ) {
                c.build = None;
            } else if used_key {
                match (b.at, b.why) {
                    (Some((id, (s, x), storey)), None) => {
                        let furniture = b.kind.spec().slug.to_string();
                        ask(
                            &net,
                            &g,
                            &Request::Furnish { home: id.0, furniture, s, x, turn: b.turn, storey },
                        );
                        c.build = None;
                    }
                    (_, Some(why)) => ui.toast(format!("CAN'T: {}", why.to_uppercase())),
                    _ => {}
                }
            }
            rode = true;
        }
        if used_key && c.focus.is_none() && !got_out && !stood_up && !rode && !ui.panel_open() {
            let view = &g.core.hangar.homes;
            if let Some((h, _)) = c.home_door() {
                match homes_on_foot::line(view, h.id) {
                    Some(l) if l.crew || l.guest || !l.locked => {
                        let open = !l.open;
                        c.door_ahead = Some((h.id.0, open, now));
                        c.shut_doors(view, now);
                        ask(&net, &g, &Request::Door { home: h.id.0, open });
                    }
                    _ => ui.panel = Panel::Home(h.id.0),
                }
                rode = true;
            } else if let Some(id) = c.home_in()
                && homes_on_foot::line(view, id).is_some_and(|l| l.crew)
            {
                ui.panel = Panel::Home(id.0);
                rode = true;
            }
        }
        // At the Blast Hall's gantry's hatch: E boards the trainer the test range picked.
        c.gantry = c.focus.is_none() && c.hatch_in_view();
        board_trainer =
            used_key && c.gantry && !ui.panel_open() && c.drive.is_none() && !got_out && !stood_up;
        // E by one of The Arrival's seats (no door in view): sit, unless someone's on it.
        if used_key
            && !stood_up
            && !got_out
            && !rode
            && c.focus.is_none()
            && !c.gantry
            && c.seat.is_none()
            && c.drive.is_none()
            && c.ride.is_none()
        {
            let at = c.feet();
            if let Some(k) = seat_near(c.strip, at.s, at.x) {
                let t = arrival_seats()[k];
                let taken = g
                    .core
                    .people(now)
                    .iter()
                    .any(|(_, _, p)| p.seated() && (p.s - t.s).hypot(p.x - t.x) < 0.5);
                if taken {
                    ui.toast("SOMEONE'S SITTING THERE");
                } else {
                    c.sit(k);
                }
            }
        }
        g.core.set_pose(Some(c.pose()));
        // Sent here, the frame it's taken, if one's due (and nowhere else, so a stale one never
        // takes its turn): after a slow frame the walk has caught up by up to a second's worth (a
        // guide's, `GUIDE_CATCH_UP`), and a pose a frame late would reach the server only a moment
        // after the one before it, too far for that moment.
        if let (Some(t), Some(p)) = (net.get(), g.core.poll_pose(now)) {
            t.send_datagram(&p);
        }
        // A sight goes on the found-list the first time (and is named every time); else a
        // district's name.
        match c.reached() {
            (_, Some(i)) => {
                let found = settings.0.sights_found;
                ui.toast(sights::news(i, found));
                if found & sights::bit(i) == 0 {
                    settings.0.sights_found |= sights::bit(i);
                }
            }
            (Some(district), None) => ui.toast(district),
            (None, None) => {}
        }
        if live && keys.just_pressed(KeyCode::KeyM) {
            ui.map = !ui.map;
        }
        // A motor pool in reach: E a car, Q a scooter, R a Markab (a place's door comes first).
        let pool_here = c.drive.is_none() && c.ride.is_none() && {
            let at = c.feet();
            pool_near(c.strip, at.s, at.x).is_some()
        };
        if pool_here && c.focus.is_none() && !c.gantry && !ui.panel_open() && !got_out {
            let scooter = live && keys.just_pressed(KeyCode::KeyQ);
            let rocket = live && keys.just_pressed(KeyCode::KeyR);
            let kind = match (rocket, scooter) {
                (true, _) => None,
                (_, true) => Some(Kind::Scooter),
                _ => Some(Kind::Car),
            };
            if (used_key || scooter || rocket) && c.take(kind) {
                ui.toast(match kind {
                    None => MARKAB_NEWS,
                    Some(Kind::Scooter) => "A SCOOTER FROM THE MOTOR POOL",
                    Some(Kind::Car) => "A CAR FROM THE MOTOR POOL",
                });
            }
        }
        if used_key
            && !ui.panel_open()
            && c.drive.is_none()
            && !got_out
            && let Some(i) = c.focus
        {
            match PLACES[i].kind {
                PlaceKind::HubGate => {
                    me.start(Seq::LiftUp, now);
                    ask(&net, &g, &Request::LeaveCity);
                }
                PlaceKind::Exchange => ui.panel = Panel::Terminal(Spot::Exchange),
                PlaceKind::Charter => ui.panel = Panel::Board,
                // Behind the bar: the Almagest's Most Wanted, and the pilot's terms.
                PlaceKind::Bar => ui.panel = Panel::Arrival,
                // The instructor's desk: the board, the pars, and the pilot's own bests.
                PlaceKind::Proving => ui.panel = Panel::Proving,
            }
        }
    } else if let Some(c) = me.city.as_mut() {
        c.focus = None;
        c.guide = None;
        c.gantry = false;
    }
    if board_trainer {
        me.start(Seq::Boarding, now);
        ask(&net, &g, &Request::BoardTrainer);
    }
    if !in_city {
        g.core.set_pose(None);
    }
    let hangar_bay = g.core.hangar.view.as_ref().map(|v| v.bay.clone());
    if on_foot {
        for cmd in &cmds.0 {
            if let UiCmd::WalkTo { spot: slug } = cmd
                && let Some(spot) = Spot::from_slug(slug)
            {
                let (_, facing) = spot.stand();
                let route = me.layout.route(me.walker.feet, spot);
                me.guide = Some(Guide::new(route, Some(facing)));
            }
        }
        // The autopilot walks to the cockpit and launches whatever stands in the gantry.
        if autopilot
            && me.guide.is_none()
            && me.focus != Some(Spot::Cockpit)
            && matches!(hangar_bay, Some(Bay::Docked { .. }))
        {
            let (_, facing) = Spot::Cockpit.stand();
            let route = me.layout.route(me.walker.feet, Spot::Cockpit);
            me.guide = Some(Guide::new(route, Some(facing)));
        }
        let me = &mut *me;
        if look != Vec2::ZERO {
            me.walker.turn(-look.x, -look.y);
        }
        if own != Stride::default() {
            // The pilot's own feet take over.
            me.guide = None;
        }
        // In slices, so a slow frame walks as far as a fast one would (the guide's stride while it
        // walks them, then their own).
        let mut left = walk_dt(me.guide.is_some());
        while left > 1e-4 {
            let h = left.min(0.1);
            left -= h;
            let stride = match me.guide.as_mut() {
                Some(guide) => {
                    let stride = guide.steer(&mut me.walker, h);
                    if guide.arrived() && guide.facing.is_none_or(|f| me.walker.heading().dot(f) > 0.97) {
                        me.guide = None;
                    }
                    stride
                }
                None => own,
            };
            me.walker.step(&me.layout, &stride, h);
        }
        me.focus = Layout::spot_in_view(me.walker.eye(), me.walker.look());
        // At the airlock (the colony open), Q chooses the next strip's cap lift; out of its view,
        // the choice goes back to the Charter strip's.
        if me.focus != Some(Spot::Airlock) {
            me.lift = Lift::default();
        } else if colony && live && keys.just_pressed(KeyCode::KeyQ) {
            me.lift = me.lift.next();
        }

        // Using what's in view.
        // Q at the cockpit (the colony open): launch into the colony, by the inner gate.
        let inward = colony && live && keys.just_pressed(KeyCode::KeyQ) && me.focus == Some(Spot::Cockpit);
        let used = used_key || inward || (autopilot && me.focus == Some(Spot::Cockpit) && me.guide.is_none());
        if used
            && !ui.panel_open()
            && let Some(spot) = me.focus
        {
            match (spot, &hangar_bay) {
                (Spot::Cockpit, Some(Bay::Docked { suit })) => {
                    me.launching = Some(suit.clone());
                    me.from = (me.walker.eye(), me.walker.look());
                    me.start(Seq::Boarding, now);
                    // Out into space, the suit rides the bay's cradle in its door until the
                    // catapult fires at the end of the sequence (`release_bay`).
                    g.bay_hold = !inward;
                    ask(&net, &g, if inward { &Request::LaunchInside } else { &Request::Launch });
                }
                (Spot::Cockpit, Some(Bay::Out { .. })) => ui.toast("YOUR SUIT IS OUT IN THE SECTOR"),
                (Spot::Cockpit, _) => ui.toast("THE GANTRY IS EMPTY: BUILD A SUIT AT THE FABRICATOR"),
                // Down into the colony, to the chosen strip's Hub Gate (the Charter strip's, unless
                // Q chose another).
                (Spot::Airlock, _) if colony => {
                    let strip = me.lift.strip();
                    me.airlock_until = now + 1.5;
                    me.city = Some(CityFoot::at_hub_gate(strip));
                    me.start(Seq::LiftDown, now);
                    ask(&net, &g, &Request::EnterCity { strip });
                }
                // The colony closed (arcade rules, or `--no-colony`): the airlock only leads out.
                (Spot::Airlock, _) => {
                    me.airlock_until = now + 1.5;
                    ui.toast("THE CAP LIFTS ARE CLOSED: THIS SERVER KEEPS THE COLONY SHUT");
                    ui.open_pause();
                }
                (terminal, _) => ui.panel = Panel::Terminal(terminal),
            }
        }
    } else {
        me.focus = None;
        me.guide = None;
    }

    // In the sector: Enter docks where the suit is docked or coming in to (`dock::enter_docks`,
    // where the HUD's marker says so); anywhere else it opens the radio, and on the chart it sets a
    // course. The radio (`chat.rs`) decided where this press went, earlier in the frame.
    let flying = place == Some(Place::Space) && me.seq == Seq::Walking;
    if flying && ui.playing() && !ui.panel_open() && !map.0 && enter.0 == dock::EnterKey::Dock {
        ask(&net, &g, &Request::Dock);
    }
    // In the sector: 1-4 use the rack's consumables (the hotbar), while the suit is alive.
    if flying && ui.playing() && !ui.panel_open() && !map.0 && g.core.world.own.is_some_and(|o| o.alive) {
        let keys_kits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
        for (key, kit) in keys_kits.into_iter().zip(Kit::ALL) {
            if keys.just_pressed(key) {
                let have = g.core.world.own.is_some_and(|o| Kits(o.kits).get(kit) > 0);
                if have {
                    ask(&net, &g, &Request::UseKit { kit });
                } else {
                    ui.toast(format!("NO {} IN THE RACK", kit.name().to_uppercase()));
                }
            }
        }
    }
    // Requests from the terminals (launching and docking are the bay's own business).
    for cmd in &cmds.0 {
        if let UiCmd::Hangar { req } = cmd
            && !matches!(req, Request::Launch | Request::Dock)
        {
            ask(&net, &g, req);
        }
    }

    // What the bay shows.
    let t = me.t(now);
    let (offset, vel) = me.suit_motion(now);
    bay.suit = match me.seq {
        Seq::Boarding | Seq::Venting | Seq::Catapult => me.launching.clone(),
        _ => match &hangar_bay {
            Some(Bay::Docked { suit }) => Some(suit.clone()),
            _ => None,
        },
    };
    bay.boarded = match me.seq {
        Seq::Boarding | Seq::Venting | Seq::Catapult => true,
        Seq::Arriving => t < ARRIVE_SECS - 1.0,
        _ => false,
    };
    bay.doors = match me.seq {
        Seq::Venting => ((t - KLAXON_SECS) / DOOR_SECS).clamp(0.0, 1.0),
        Seq::Catapult => 1.0,
        Seq::Arriving => 1.0 - ((t - GLIDE_SECS * 0.72) / DOOR_SECS).clamp(0.0, 1.0),
        _ => 0.0,
    };
    // The tunnel's outer doors open after the bay's, and shut behind a suit coming in.
    bay.outer = match me.seq {
        Seq::Venting => ((t - KLAXON_SECS - 0.5) / DOOR_SECS).clamp(0.0, 1.0),
        Seq::Catapult => 1.0,
        Seq::Arriving => 1.0 - ((t - 0.3) / DOOR_SECS).clamp(0.0, 1.0),
        _ => 0.0,
    };
    bay.alarm =
        matches!(me.seq, Seq::Venting | Seq::Catapult) || (me.seq == Seq::Arriving && bay.doors > 0.0);
    bay.offset = offset;
    bay.vel = vel;
    (bay.thrust, bay.boost) = match me.seq {
        Seq::Catapult => (Vec3::Z, true),
        Seq::Arriving if t < GLIDE_SECS => (-Vec3::Z * (1.0 - t / GLIDE_SECS), false),
        _ => (Vec3::ZERO, false),
    };
    // The airlock's door opens as the pilot comes in (or goes to leave), and closes after them.
    let open = (me.seq == Seq::Entering && t < ENTER_SECS * 0.8) || now < me.airlock_until;
    bay.airlock = if open { (bay.airlock + dt * 2.5).min(1.0) } else { (bay.airlock - dt * 1.5).max(0.0) };
    if let Ok(mut c) = curtain.single_mut() {
        let a = me.curtain(now).max(me.city.as_ref().map_or(0.0, |c| c.lift_dark(now)));
        let want = Color::srgba(0.0, 0.0, 0.0, a);
        if c.0 != want {
            c.0 = want;
        }
    }

    // What the page shows.
    ui.place = match place {
        Some(Place::Hangar) => "hangar",
        Some(Place::Space) => "space",
        Some(Place::City) => "city",
        None => "",
    };
    ui.on_foot = on_foot || in_city;
    ui.driving = me.city.as_ref().and_then(|c| c.drive).filter(|_| in_city).map_or("", |w| w.name());
    // The map (M), and where the pilot stands on it.
    ui.map &= me.city.is_some();
    let map_at = me.city.as_ref().filter(|_| ui.map).map(|c| {
        let at = c.feet();
        let h = c.walker.heading();
        let q = |v: f32, k: f32| (v * k).round() / k;
        [f32::from(c.strip), q(at.x, 2.0), q(at.s, 2.0), q(h.z.atan2(h.x), 20.0)]
    });
    if ui.map_at != map_at {
        ui.map_at = map_at;
    }
    if ui.map_found != settings.0.sights_found {
        ui.map_found = settings.0.sights_found;
    }
    // On foot in the city, or flying a suit inside the colony: the camera sees its city.
    let flying_inside = place == Some(Place::Space) && g.core.welcome.is_some_and(|w| w.interior);
    let city_now = me.city.is_some() || flying_inside;
    if city_view.active != city_now {
        *city_view = CityView { active: city_now, sync: false };
    }
    ui.sequence = me.seq != Seq::Walking;
    ui.prompt = match me.seq {
        Seq::Walking if on_foot => me
            .focus
            .map_or_else(String::new, |s| format!("E  {}", verb(s, hangar_bay.as_ref(), colony, me.lift))),
        Seq::Walking if in_city => match me.city.as_ref() {
            Some(CityFoot { drive: Some(Wheels::Markab(m)), .. }) => markab_line(m),
            Some(c) if c.drive.is_some_and(|w| w.can_leave()) => "E  GET OUT".into(),
            Some(c) if c.drive.is_some() => String::new(),
            Some(c) if c.seat.is_some() => "SEATED · E OR A STEP TO STAND".into(),
            Some(c) if c.focus.is_some() => format!("E  {}", city_verb(&PLACES[c.focus.unwrap_or(0)])),
            Some(c) if c.gantry => format!("E  BOARD A TRAINER · {}", g.core.hangar.trainer_name()),
            Some(CityFoot { build: Some(b), .. }) => {
                let what = b.kind.spec().name.to_uppercase();
                match b.why {
                    None => format!("PUTTING DOWN {what} · E  PUT IT HERE · R  TURN IT · Q  STOP"),
                    Some(why) => {
                        format!("PUTTING DOWN {what} · {} · R  TURN IT · Q  STOP", why.to_uppercase())
                    }
                }
            }
            Some(c) if let Some((h, _)) = c.home_door() => {
                format!(
                    "E  {}",
                    homes_on_foot::door_verb(&h, homes_on_foot::line(&g.core.hangar.homes, h.id))
                )
            }
            Some(c)
                if let Some(id) = c.home_in()
                    && homes_on_foot::line(&g.core.hangar.homes, id).is_some_and(|l| l.crew) =>
            {
                "E  ITS PANEL · CODES, CREW AND RENT".into()
            }
            Some(c) if let Some((plan, k)) = c.lift_here() => {
                let floor = if k == 0 { "THE GROUND FLOOR".to_string() } else { format!("FLOOR {k}") };
                format!("THE LIFT · {floor} · E  UP · Q  DOWN · 0–{}  A FLOOR", plan.storeys - 1)
            }
            Some(c)
                if c.ride.is_none() && {
                    let at = c.feet();
                    seat_near(c.strip, at.s, at.x).is_some()
                } =>
            {
                "E  SIT".into()
            }
            Some(c)
                if c.ride.is_none() && {
                    let at = c.feet();
                    pool_near(c.strip, at.s, at.x).is_some()
                } =>
            {
                "E  TAKE A CAR · Q  A SCOOTER · R  A MARKAB".into()
            }
            _ => String::new(),
        },
        s if s.skippable() => "SPACE  SKIP".into(),
        _ => String::new(),
    };
    // Indoors (the bay, the city, a sequence there): the bay's line, and how fed the pilot is once
    // the server has said. Flying, the HUD has the corner.
    ui.bay_line = if inside {
        let fed = g.core.hangar.fed().map_or(String::new(), |f| format!(" · {}", f.tag()));
        format!("BAY {:02} · {} CR{fed}", g.core.hangar.bay, thousands(g.core.hangar.credits()))
    } else {
        String::new()
    };
    // Leaving from inside the colony, the suit is brought home (`page`'s menu says so).
    ui.inside = flying_inside;
    ui.trainer = flying_inside && g.core.hangar.trainer;
}

/// The camera at the pilot's eyes (or where a sequence puts it), after the chase camera has had
/// its turn.
pub fn onfoot_camera(
    me: Res<OnFoot>,
    indoors: Res<Indoors>,
    prefs: Res<ViewPrefs>,
    mut origin: ResMut<RenderOrigin>,
    mut cam: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut tf, mut projection)) = cam.single_mut() else { return };
    // Walls come close on foot (in the city, a little less close: the far wall is 6 km off).
    let near = match (&me.city, indoors.0) {
        (Some(_), true) => 0.15,
        (None, true) => 0.05,
        _ => 0.5,
    };
    if let Projection::Perspective(p) = &mut *projection {
        if p.near != near {
            p.near = near;
        }
        if indoors.0 {
            p.fov = prefs.fov.clamp(40.0, 120.0).to_radians();
        }
    }
    if !indoors.0 {
        return;
    }
    let now = now_s();
    // In the colony: drawn in its frame, relative to the render origin, upright to its floor.
    if let Some(c) = &me.city {
        let (eye, look, up) = if me.seq == Seq::LiftDown {
            let (eye, look) = lift_view(c.strip, me.t(now));
            (eye, look, up_at(eye.as_vec3()))
        } else {
            c.view()
        };
        let mut moved = *origin;
        moved.follow(eye);
        if moved != *origin {
            *origin = moved;
        }
        *tf = Transform::from_translation(origin.place(eye)).looking_to(look, up);
        return;
    }
    let (eye, at) = me.view(now);
    *tf = Transform::from_translation(BAY_ORIGIN + eye).looking_at(BAY_ORIGIN + at, Vec3::Y);
}

/// On foot, for the E2E tests: where the pilot is and what they could use.
pub fn publish_onfoot(
    me: Res<OnFoot>,
    ui: Res<Ui>,
    settings: Res<SettingsRes>,
    game: NonSend<GameClient>,
    mut dev: ResMut<DevStatus>,
) {
    let g = game.borrow();
    let h = &g.core.hangar;
    dev.set("place", ui.place);
    // The colony's sights found on foot (the found-list, kept in the settings).
    dev.set("sights_found", sights::count(settings.0.sights_found) as u32);
    // The objectives done (a bit per `bc_client_core::objectives::Objective`).
    dev.set("objectives_done", settings.0.objectives_done);
    dev.set("seq", me.seq.name());
    dev.set("focus", me.focus.map_or("", Spot::slug));
    dev.set("terminal", ui.terminal_tab());
    dev.set("walking_to", me.guide.is_some());
    let f = me.walker.feet;
    dev.set("feet", format!("{:.1},{:.1},{:.1}", f.x, f.y, f.z));
    // The people in view (on foot in the city, or round a suit flying inside it): how many, their
    // names, and where each is ("name@x,s,h", `;` between them).
    let people = g.core.people(now_s());
    dev.set("people", people.len() as u32);
    dev.set("people_names", people.iter().map(|(_, n, _)| *n).collect::<Vec<_>>().join(","));
    let where_ = people.iter().map(|(_, n, p)| format!("{n}@{:.1},{:.1},{:.1}", p.x, p.s, p.h));
    dev.set("people_at", where_.collect::<Vec<_>>().join(";"));
    // In the colony: the strip, where on it (along, across, up) and the place at hand.
    match &me.city {
        Some(c) => {
            let at = c.feet();
            dev.set("strip", f64::from(c.strip));
            dev.set("city_feet", format!("{:.1},{:.1},{:.1}", at.x, at.s, at.h));
            dev.set("city_walking_to", c.guide.is_some());
            dev.set("seated", c.seat.map_or(-1.0, |k| k as f64));
            dev.set("riding", c.ride.map_or(-1.0, |r| f64::from(r.k)));
            dev.set("driving", c.drive.map_or("", |w| w.name()));
            dev.set("drive_speed", c.drive.map_or(0.0, |w| f64::from(w.speed())));
            // A Markab: what its wheels are on (floor, wall, ceiling, corner, air), how high its middle
            // is, its bottles and its fan.
            let rocket = match c.drive {
                Some(Wheels::Markab(m)) => Some(m),
                _ => None,
            };
            dev.set("markab", rocket.as_ref().map_or("", markab_on));
            dev.set("markab_h", rocket.map_or(-1.0, |m| f64::from(m.pos.y)));
            dev.set("markab_gas", rocket.map_or(-1.0, |m| f64::from(m.gas)));
            dev.set("markab_fan", rocket.is_some_and(|m| m.fan));
            dev.set("station", c.train().and_then(|t| t.at).map_or(-1.0, |i| i as f64));
            dev.set("district", c.district.map_or("", |d| DISTRICT_NAMES[c.strip as usize % 3][d as usize]));
            // Where the nearest of the people in view is from the pilot (m).
            let near =
                people.iter().map(|(_, _, p)| (p.x - at.x).hypot(p.s - at.s)).fold(f32::INFINITY, f32::min);
            dev.set("people_nearest", if near.is_finite() { f64::from(near) } else { -1.0 });
            // The suits inside the colony that they watch, how far off the nearest is, and how many
            // stand on its city.
            let eye = colony_point(at).as_vec3();
            let t = g.core.render_tick(now_s());
            let suits: Vec<_> = (0..g.core.world.entities.len() as u16)
                .filter_map(|slot| g.core.world.pose(slot, t))
                .collect();
            dev.set("watched", suits.len() as u32);
            let near = suits.iter().map(|p| p.pos.distance(eye)).fold(f32::INFINITY, f32::min);
            dev.set("watched_nearest", if near.is_finite() { f64::from(near) } else { -1.0 });
            let standing = suits
                .iter()
                .filter(|p| p.ground.is_some_and(|g| g.body == bc_sim::bodies::Body::City && !g.aloft))
                .count();
            dev.set("watched_standing", standing as u32);
            if me.seq == Seq::Walking {
                let focus = if c.gantry { "gantry" } else { c.focus.map_or("", |i| PLACES[i].slug) };
                dev.set("focus", focus);
            }
            // The homes: the one they stand in (its id; 0, none), the one whose door they're at,
            // the held homes in view, their pieces, who's asleep in their beds, and a piece being
            // put down ("sofa:fits", "sofa:no").
            let held = &h.homes.homes;
            dev.set("home", c.home_in().map_or(0.0, |id| f64::from(id.0)));
            dev.set("home_door", c.home_door().map_or(0.0, |(d, _)| f64::from(d.id.0)));
            dev.set("homes_held", held.len() as u32);
            dev.set("home_pieces", held.iter().map(|l| l.pieces.len() as u32).sum::<u32>());
            let asleep = held.iter().flat_map(|l| l.asleep.iter().map(|(_, name)| name.as_str()));
            dev.set("home_asleep", asleep.collect::<Vec<_>>().join(","));
            let build = c
                .build
                .as_ref()
                .map(|b| format!("{}:{}", b.kind.spec().slug, if b.why.is_none() { "fits" } else { "no" }));
            dev.set("build", build.unwrap_or_default());
        }
        None => {
            dev.set("home", 0.0);
            dev.set("home_door", 0.0);
            dev.set("homes_held", 0u32);
            dev.set("home_pieces", 0u32);
            dev.set("home_asleep", "");
            dev.set("build", "");
            dev.set("strip", -1.0);
            dev.set("city_feet", "");
            dev.set("city_walking_to", false);
            dev.set("seated", -1.0);
            dev.set("riding", -1.0);
            dev.set("driving", "");
            dev.set("drive_speed", 0.0);
            dev.set("markab", "");
            dev.set("markab_h", -1.0);
            dev.set("markab_gas", -1.0);
            dev.set("markab_fan", false);
            dev.set("station", -1.0);
            dev.set("district", "");
            dev.set("people_nearest", -1.0);
            dev.set("watched", 0u32);
            dev.set("watched_nearest", -1.0);
            dev.set("watched_standing", 0u32);
        }
    }
    dev.set("hangar_credits", h.credits() as f64);
    dev.set("hangar_version", h.version as f64);
    let (bay, line) = match h.view.as_ref().map(|v| &v.bay) {
        Some(Bay::Docked { suit }) => ("docked", suit.line.slug()),
        Some(Bay::Out { suit }) => ("out", suit.line.slug()),
        Some(Bay::Empty) => ("empty", ""),
        None => ("", ""),
    };
    dev.set("bay", bay);
    dev.set("bay_line", line);
    // The pilot's bay on the ring, by number: their suit launches out of its door.
    dev.set("bay_no", u32::from(h.bay));
    // What's broken inside the suit in the bay, and what it carries.
    let suit = h.view.as_ref().and_then(|v| match &v.bay {
        Bay::Docked { suit } | Bay::Out { suit } => Some(suit),
        Bay::Empty => None,
    });
    dev.set("bay_faults", suit.map_or(0, |s| s.faults.count() as u32));
    dev.set("bay_modules", suit.map_or(0, |s| s.modules.iter().flatten().count() as u32));
    dev.set("jobs", h.view.as_ref().map_or(0, |v| v.jobs.len() as u32));
    dev.set("orders", h.market.as_ref().map_or(0, |m| m.orders.len() as u32));
}
