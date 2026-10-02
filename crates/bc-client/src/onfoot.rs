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
//! - Into the colony, when it's open: the airlock leads to the cap lift, which rides down the end
//!   cap's face (the whole city in view) to Hub Gate. There the pilot walks the city's streets
//!   (`bc_client_core::city`), uses its places at their doors, and rides back up from Hub Gate.
//!
//! The server knows none of this: only where the pilot is (`core.hangar.place`) and what they ask
//! for. Space skips a sequence.

use bc_client_core::bay::{CATWALK_Y, HATCH, Layout, SPAWN, SUIT_AT, Spot};
use bc_client_core::city::CityGround;
use bc_client_core::city_nav;
use bc_client_core::tram::{self, CarInside, CityAndTrains, Rider};
use bc_client_core::vehicle::{Drive, Kind, Vehicle};
use bc_client_core::walker::{Guide, Stride, Walker};
use bc_econ::item::thousands;
use bc_econ::wire::{Outcome, Place, Request};
use bc_econ::{Bay, Suit};
use bc_proto::presence::{PersonPose, RIDE_SEATED};
use bc_sim::colony::city::{
    AVENUE as AVENUE_WIDTH, BLOCK, Stage, TERMINAL_HEIGHT, arrival_seats, district_at, grid_x, place_door,
    row_span, seat_near, terminal_rect,
};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, local_frame, up_at};
use bc_sim::colony::hub::BAY_RADIUS;
use bc_sim::colony::pools::{pool, pool_near};
use bc_sim::colony::transit::{
    CAR_WIDTH, CARS, DOOR_AT, FLOOR, PLATFORM_HALF, PLATFORM_LENGTH, STATION_GAP, STATIONS, TRAINS,
    TrainState, station_x, train,
};
use bc_sim::content::city::{DISTRICT_NAMES, PLACES, PlaceDef, PlaceKind, SIGHTS, STRIP_NAMES};
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
/// A sight is named once the pilot is this near its block's middle, m.
const SIGHT_REACH: f32 = 140.0;

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
    pub drive: Option<Vehicle>,
    chase: bool,
    /// The colony's tick the trains are at.
    tick: (u32, f32),
    /// Sitting on one of The Arrival's seats (`bc_sim::colony::city::arrival_seats`).
    pub seat: Option<usize>,
}

/// Seated, the eye is this much lower than standing, m.
const SEATED_DROP: f32 = 0.55;

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
        let feet = CityPos::new(strip, x, s, 0.0).walker();
        let away = CityPos::new(strip, x - dx, s - ds, 0.0).walker() - feet;
        Self {
            strip,
            walker: Walker::at(feet, away.normalize()),
            guide: None,
            focus: None,
            district: None,
            sight: None,
            ride: None,
            trains: std::array::from_fn(|k| train(strip, k as u8, 0, 0.0)),
            stood: None,
            drive: None,
            chase: true,
            tick: (0, 0.0),
            seat: None,
        }
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
            Some(Guide::new(city_nav::route(self.strip, (at.s, at.x), (t.s, t.x)), Some(facing.normalize())));
        true
    }

    /// Takes a vehicle from the motor pool in reach, if there is one.
    fn take(&mut self, kind: Kind) -> bool {
        let at = self.feet();
        let Some(i) = pool_near(self.strip, at.s, at.x) else { return false };
        let (s, x) = pool(self.strip, i);
        // Parked facing up the avenue.
        self.drive = Some(Vehicle::new(kind, self.strip, s, x, std::f32::consts::FRAC_PI_2));
        self.guide = None;
        // From the scooter's deck there's nothing to see but the helmet: from behind.
        self.chase = self.chase || kind == Kind::Scooter;
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
            let mut w = Walker::at(v.door().walker(), Vec3::new(v.yaw.sin(), 0.0, v.yaw.cos()));
            w.pitch = self.walker.pitch;
            self.walker = w;
        }
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
                let world = CityAndTrains { ground: self.ground(), trains: &self.trains };
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
                route = city_nav::route(self.strip, (feet.s, feet.x), (mid - 20.0, foot));
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

    /// What's newly reached: a district's name (with its strip's) or a sight's.
    fn reached(&mut self) -> Option<String> {
        let at = self.feet();
        let k = self.strip as usize % 3;
        let district = district_at(self.strip, at.x, Stage(0)).map(|(d, _)| d);
        let sight = SIGHTS.iter().position(|&(strip, bx, row, _)| {
            let (s0, s1) = row_span(row);
            let (s, x) = ((s0 + s1) * 0.5, grid_x(bx) + BLOCK * 0.5);
            strip == self.strip && (at.s - s).hypot(at.x - x) < SIGHT_REACH
        });
        let mut news = None;
        if district != self.district {
            self.district = district;
            news = district.map(|d| format!("{} · {}", DISTRICT_NAMES[k][d as usize], STRIP_NAMES[k]));
        }
        if sight != self.sight {
            self.sight = sight;
            news = sight.map(|i| SIGHTS[i].3.to_string()).or(news);
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
            return CityPos::new(self.strip, v.x, v.s, v.ground());
        }
        match (self.ride, self.train()) {
            (Some(r), Some(t)) => tram::in_city(&r, self.walker.feet, t),
            _ => CityPos::from_walker(self.strip, self.walker.feet),
        }
    }

    /// The eye, in the colony's frame, and the way it looks.
    fn view(&self) -> (DVec3, Vec3) {
        if let Some(v) = self.drive {
            let (fx, fs) = v.forward();
            let g = v.ground();
            let at = |back: f32, up: f32| {
                colony_point(CityPos::new(self.strip, v.x - fx * back, v.s - fs * back, g + up))
            };
            if self.chase {
                // Behind and above it, looking a little ahead of it.
                let (eye, ahead) = (at(7.5, 2.8), at(-4.0, 1.0));
                return (eye, (ahead - eye).as_vec3().normalize_or(Vec3::X));
            }
            let eye = CityPos::new(self.strip, v.x - fx * 0.3, v.s - fs * 0.3, g + v.spec().eye);
            let look =
                Vec3::new(v.yaw.sin(), self.walker.pitch.clamp(-0.4, 0.3).sin(), v.yaw.cos()).normalize();
            return (colony_point(eye), local_frame(self.strip, eye.s) * look);
        }
        let mut eye = match (self.ride, self.train()) {
            (Some(r), Some(t)) => tram::in_city(&r, self.walker.eye(), t),
            _ => CityPos::from_walker(self.strip, self.walker.eye()),
        };
        if self.seat.is_some() {
            eye.h -= SEATED_DROP;
        }
        (colony_point(eye), local_frame(self.strip, eye.s) * self.walker.look())
    }

    /// The place whose door the pilot stands at, facing it.
    fn door_in_view(&self) -> Option<usize> {
        if self.ride.is_some() || self.drive.is_some() || self.seat.is_some() {
            return None;
        }
        let at = self.feet();
        let h = self.walker.heading();
        // Facing, in city terms: x along is the walker's x, s across its −z.
        let (hs, hx) = (-h.z, h.x);
        PLACES.iter().enumerate().filter(|(_, p)| p.strip == self.strip).find_map(|(i, p)| {
            let ((s, x), (ds, dx)) = place_door(p);
            let near = (at.s - s).hypot(at.x - x) < DOOR_REACH;
            (near && hs * ds + hx * dx > 0.3).then_some(i)
        })
    }

    /// Walks the pilot to a place's door, facing in.
    fn walk_to(&mut self, slug: &str) -> bool {
        let Some((_, p)) = bc_sim::colony::city::place(slug) else { return false };
        if p.strip != self.strip {
            return false;
        }
        let ((s, x), (ds, dx)) = place_door(p);
        let at = self.feet();
        let facing = CityPos::new(self.strip, x + dx, s + ds, 0.0).walker()
            - CityPos::new(self.strip, x, s, 0.0).walker();
        self.guide =
            Some(Guide::new(city_nav::route(self.strip, (at.s, at.x), (s, x)), Some(facing.normalize())));
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

/// What using a place is called on the prompt.
fn verb(spot: Spot, bay: Option<&Bay>, colony: bool) -> String {
    match (spot, bay) {
        (Spot::Cockpit, Some(Bay::Docked { .. })) => "BOARD & LAUNCH".into(),
        (Spot::Cockpit, Some(Bay::Out { .. })) => "COCKPIT: YOUR SUIT IS OUT".into(),
        (Spot::Cockpit, _) => "COCKPIT: THE GANTRY IS EMPTY".into(),
        (Spot::Airlock, _) if colony => "AIRLOCK: THE CAP LIFT, DOWN INTO THE COLONY".into(),
        (Spot::Airlock, _) => "AIRLOCK: LEAVE THE BAY".into(),
        (spot, _) => spot.name().into(),
    }
}

/// What using a place in the city is called on the prompt.
fn city_verb(p: &PlaceDef) -> String {
    match p.kind {
        PlaceKind::HubGate => "THE CAP LIFT, UP TO YOUR BAY".into(),
        _ => p.name.into(),
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
    settings: Res<SettingsRes>,
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
) {
    let now = now_s();
    let dt = time.delta_secs().min(0.1);
    // Walking keeps up with slow frames (up to a second's worth).
    let walk_dt = time.delta_secs().min(1.0);
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
            (Some(Place::Hangar), Some(Place::Space)) if me.seq == Seq::Boarding => {
                me.start(Seq::Venting, now);
            }
            (Some(Place::Space), Some(Place::Hangar)) if me.outcome == Some(Outcome::Docked) => {
                me.climb_out();
                me.start(Seq::Arriving, now);
            }
            (_, Some(Place::Hangar)) => {
                me.city = None;
                me.enter(now);
            }
            // Down in the colony (the lift's already on its way, unless it was sent some other way).
            (_, Some(Place::City)) => {
                if me.city.is_none() {
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
        let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
        own = Stride {
            forward: axis(KeyCode::KeyW, KeyCode::KeyS),
            right: axis(KeyCode::KeyD, KeyCode::KeyA),
            run: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
            jump: keys.pressed(KeyCode::Space),
        };
    }
    let used_key = (live && keys.just_pressed(KeyCode::KeyE)) || cmds.has(&UiCmd::Use);
    if in_city && let Some(c) = me.city.as_mut() {
        for cmd in &cmds.0 {
            if let UiCmd::WalkTo(slug) = cmd {
                match slug.as_str() {
                    "tram" => c.walk_to_tram(),
                    "pool" => c.walk_to_pool(),
                    "seat" => c.walk_to_seat(),
                    _ => c.walk_to(slug),
                };
            }
        }
        if look != Vec2::ZERO {
            c.walker.turn(-look.x, -look.y);
        }
        let mut stride = own;
        if stride != Stride::default() {
            c.guide = None;
        }
        let (tick, frac) = g.core.colony_tick(now);
        c.time(tick, frac);
        // Driving: W/S the throttle and the brake, A/D the wheel, Space the handbrake.
        let mut got_out = false;
        if let Some(v) = c.drive.as_mut() {
            let drive = Drive { throttle: own.forward, steer: -own.right, handbrake: own.jump };
            let mut left = walk_dt;
            while left > 1e-4 {
                let h = left.min(0.05);
                left -= h;
                v.step(&drive, h);
            }
            if live && keys.just_pressed(KeyCode::Tab) && v.kind == Kind::Car {
                c.chase = !c.chase;
            }
            if used_key && v.speed.abs() < 3.0 {
                c.get_out();
                got_out = true;
            }
        }
        // Seated: put, till a step or E gets them up, a step out in front of the seat.
        let mut stood_up = false;
        if c.seat.is_some() && (own != Stride::default() || used_key) {
            c.stand();
            stood_up = true;
        }
        let mut left = if c.drive.is_some() || c.seat.is_some() { 0.0 } else { walk_dt };
        while left > 1e-4 {
            let h = left.min(0.1);
            left -= h;
            if let Some(guide) = c.guide.as_mut() {
                stride = guide.steer(&mut c.walker, h);
                if guide.arrived() && guide.facing.is_none_or(|f| c.walker.heading().dot(f) > 0.97) {
                    c.guide = None;
                }
            }
            if let Some(news) = c.step(&stride, h) {
                ui.toast(news);
            }
        }
        if let Some(name) = c.arrived() {
            ui.toast(name);
        }
        c.focus = c.door_in_view();
        // E by one of The Arrival's seats (no door in view): sit, unless someone's on it.
        if used_key
            && !stood_up
            && !got_out
            && c.focus.is_none()
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
        if let Some(name) = c.reached() {
            ui.toast(name);
        }
        if live && keys.just_pressed(KeyCode::KeyM) {
            ui.map = !ui.map;
        }
        // A motor pool in reach: E a car, Q a scooter (a place's door comes first).
        let pool_here = c.drive.is_none() && c.ride.is_none() && {
            let at = c.feet();
            pool_near(c.strip, at.s, at.x).is_some()
        };
        if pool_here && c.focus.is_none() && !ui.panel_open() && !got_out {
            let scooter = live && keys.just_pressed(KeyCode::KeyQ);
            if (used_key || scooter) && c.take(if scooter { Kind::Scooter } else { Kind::Car }) {
                ui.toast(if scooter { "A SCOOTER FROM THE MOTOR POOL" } else { "A CAR FROM THE MOTOR POOL" });
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
                PlaceKind::Charter => {
                    ui.news("THE CHARTER BOARD · ARRIVALS REGISTER AT THE DESK · NOTICES BY THE DOOR", false)
                }
                PlaceKind::Bar => ui.toast("THE ARRIVAL · A BAR TO MEET IN · QUIET FOR NOW"),
            }
        }
    } else if let Some(c) = me.city.as_mut() {
        c.focus = None;
        c.guide = None;
    }
    if !in_city {
        g.core.set_pose(None);
    }
    let hangar_bay = g.core.hangar.view.as_ref().map(|v| v.bay.clone());
    if on_foot {
        for cmd in &cmds.0 {
            if let UiCmd::WalkTo(slug) = cmd
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
        let mut stride = own;
        if look != Vec2::ZERO {
            me.walker.turn(-look.x, -look.y);
        }
        if stride != Stride::default() {
            // The pilot's own feet take over.
            me.guide = None;
        }
        // In slices, so a slow frame walks as far as a fast one would.
        let mut left = walk_dt;
        while left > 1e-4 {
            let h = left.min(0.1);
            left -= h;
            if let Some(guide) = me.guide.as_mut() {
                stride = guide.steer(&mut me.walker, h);
                if guide.arrived() && guide.facing.is_none_or(|f| me.walker.heading().dot(f) > 0.97) {
                    me.guide = None;
                }
            }
            me.walker.step(&me.layout, &stride, h);
        }
        me.focus = Layout::spot_in_view(me.walker.eye(), me.walker.look());

        // Using what's in view.
        let used = used_key || (autopilot && me.focus == Some(Spot::Cockpit) && me.guide.is_none());
        if used
            && !ui.panel_open()
            && let Some(spot) = me.focus
        {
            match (spot, &hangar_bay) {
                (Spot::Cockpit, Some(Bay::Docked { suit })) => {
                    me.launching = Some(suit.clone());
                    me.from = (me.walker.eye(), me.walker.look());
                    me.start(Seq::Boarding, now);
                    ask(&net, &g, &Request::Launch);
                }
                (Spot::Cockpit, Some(Bay::Out { .. })) => ui.toast("YOUR SUIT IS OUT IN THE SECTOR"),
                (Spot::Cockpit, _) => ui.toast("THE GANTRY IS EMPTY: BUILD A SUIT AT THE FABRICATOR"),
                // Down into the colony, to the Charter strip's Hub Gate.
                (Spot::Airlock, _) if colony => {
                    me.airlock_until = now + 1.5;
                    me.city = Some(CityFoot::at_hub_gate(0));
                    me.start(Seq::LiftDown, now);
                    ask(&net, &g, &Request::EnterCity { strip: 0 });
                }
                (Spot::Airlock, _) => {
                    me.airlock_until = now + 1.5;
                    ui.open_pause();
                }
                (terminal, _) => ui.panel = Panel::Terminal(terminal),
            }
        }
    } else {
        me.focus = None;
        me.guide = None;
    }

    // In the sector: Enter docks.
    let flying = place == Some(Place::Space) && me.seq == Seq::Walking;
    if flying
        && ui.playing()
        && !ui.panel_open()
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter))
    {
        ask(&net, &g, &Request::Dock);
    }
    // Requests from the terminals (launching and docking are the bay's own business).
    for cmd in &cmds.0 {
        if let UiCmd::Hangar(req) = cmd
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
        let a = me.curtain(now);
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
    let city_now = me.city.is_some();
    if city_view.active != city_now {
        *city_view = CityView { active: city_now, sync: false };
    }
    ui.sequence = me.seq != Seq::Walking;
    ui.prompt = match me.seq {
        Seq::Walking if on_foot => {
            me.focus.map_or_else(String::new, |s| format!("E  {}", verb(s, hangar_bay.as_ref(), colony)))
        }
        Seq::Walking if in_city => match me.city.as_ref() {
            Some(c) if c.drive.is_some_and(|v| v.speed.abs() < 3.0) => "E  GET OUT".into(),
            Some(c) if c.drive.is_some() => String::new(),
            Some(c) if c.seat.is_some() => "SEATED · E OR A STEP TO STAND".into(),
            Some(c) if c.focus.is_some() => format!("E  {}", city_verb(&PLACES[c.focus.unwrap_or(0)])),
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
                "E  TAKE A CAR · Q  A SCOOTER".into()
            }
            _ => String::new(),
        },
        s if s.skippable() => "SPACE  SKIP".into(),
        _ => String::new(),
    };
    ui.bay_line = if inside {
        format!("BAY {:02} · {} CR", g.core.hangar.bay, thousands(g.core.hangar.credits()))
    } else {
        String::new()
    };
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
        let (eye, look) = if me.seq == Seq::LiftDown { lift_view(c.strip, me.t(now)) } else { c.view() };
        let mut moved = *origin;
        moved.follow(eye);
        if moved != *origin {
            *origin = moved;
        }
        *tf = Transform::from_translation(origin.place(eye)).looking_to(look, up_at(eye.as_vec3()));
        return;
    }
    let (eye, at) = me.view(now);
    *tf = Transform::from_translation(BAY_ORIGIN + eye).looking_at(BAY_ORIGIN + at, Vec3::Y);
}

/// On foot, for the E2E tests: where the pilot is and what they could use.
pub fn publish_onfoot(me: Res<OnFoot>, ui: Res<Ui>, game: NonSend<GameClient>, mut dev: ResMut<DevStatus>) {
    let g = game.borrow();
    let h = &g.core.hangar;
    dev.set("place", ui.place);
    dev.set("seq", me.seq.name());
    dev.set("focus", me.focus.map_or("", Spot::slug));
    dev.set("terminal", ui.terminal().map_or("", Spot::slug));
    dev.set("walking_to", me.guide.is_some());
    let f = me.walker.feet;
    dev.set("feet", format!("{:.1},{:.1},{:.1}", f.x, f.y, f.z));
    // In the colony: the strip, where on it (along, across, up) and the place at hand.
    match &me.city {
        Some(c) => {
            let at = c.feet();
            dev.set("strip", f64::from(c.strip));
            dev.set("city_feet", format!("{:.1},{:.1},{:.1}", at.x, at.s, at.h));
            dev.set("city_walking_to", c.guide.is_some());
            dev.set("seated", c.seat.map_or(-1.0, |k| k as f64));
            dev.set("riding", c.ride.map_or(-1.0, |r| f64::from(r.k)));
            dev.set("driving", c.drive.map_or("", |v| if v.kind == Kind::Car { "car" } else { "scooter" }));
            dev.set("drive_speed", c.drive.map_or(0.0, |v| f64::from(v.speed)));
            dev.set("station", c.train().and_then(|t| t.at).map_or(-1.0, |i| i as f64));
            dev.set("district", c.district.map_or("", |d| DISTRICT_NAMES[c.strip as usize % 3][d as usize]));
            // The people in view, and where the nearest is from the pilot (m).
            let people = g.core.people(now_s());
            let names: Vec<&str> = people.iter().map(|(_, n, _)| *n).collect();
            dev.set("people", people.len() as u32);
            dev.set("people_names", names.join(","));
            let near =
                people.iter().map(|(_, _, p)| (p.x - at.x).hypot(p.s - at.s)).fold(f32::INFINITY, f32::min);
            dev.set("people_nearest", if near.is_finite() { f64::from(near) } else { -1.0 });
            if me.seq == Seq::Walking {
                dev.set("focus", c.focus.map_or("", |i| PLACES[i].slug));
            }
        }
        None => {
            dev.set("strip", -1.0);
            dev.set("city_feet", "");
            dev.set("city_walking_to", false);
            dev.set("seated", -1.0);
            dev.set("riding", -1.0);
            dev.set("driving", "");
            dev.set("drive_speed", 0.0);
            dev.set("station", -1.0);
            dev.set("district", "");
            dev.set("people", 0u32);
            dev.set("people_names", "");
            dev.set("people_nearest", -1.0);
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
