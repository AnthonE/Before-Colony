//! Navigation: the places a pilot can pick on the chart, the course to one round whatever is in
//! the way, and the auto-nav that flies it.
//!
//! - **Places** ([`Place`]) are everything the chart names: the colony, the dock, the debris field,
//!   the landmarks and their hide spots, the field's rocks, the suits in sight, a point marked on
//!   the chart, and beyond the sector the Earth Sphere (`sphere`): Earth, the Moon, the Sun and
//!   the Lagrange points, which can be found but not yet flown to.
//! - **The course** ([`plot`]) is the shortest way from the suit to where it's going that keeps
//!   off the colony and the landmarks: straight there when nothing is in the way, else through a
//!   ring of points round the colony's hull and round each landmark (a visibility graph, searched
//!   shortest first). The chart draws it, the HUD lays it out in space ahead of the suit, and the
//!   auto-nav flies it.
//! - **The auto-nav** ([`AutoNav`]) flies the suit along the course with flight assist, braking to
//!   arrive at rest just off what it was sent to (or alongside it, if it moves), and sidesteps
//!   rocks on the way. It is the pilot's own stick: the same command a pilot sends, judged by the
//!   same server. It hands the stick back on arrival, or the moment the pilot flies.

use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, PilotKind};
use bc_sim::bodies::Body;
use bc_sim::config::{G0, SECTOR_LIMIT};
use bc_sim::content::landmarks::LandmarkDef;
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
use bc_sim::content::{FrameSpec, doll_name, frame, frame_name};
use bc_sim::field::FIELD_CENTER;
use bc_sim::flight::{FA_G_CAP, FlightMods, ion_thrust};
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS, colony_sweep};
use glam::Vec3;

use crate::InputContext;
use crate::sphere::{self, Lagrange};
use crate::surface::BodySet;
use crate::world::World;

/// How far off the colony's hull the course's ring of points stands, m.
pub const COLONY_CLEAR: f32 = 900.0;
/// A leg of the course keeps this far off the colony's hull and caps, m (less where it starts or
/// ends closer).
pub const COLONY_LEG_CLEAR: f32 = 300.0;
/// How far past a landmark's reach the course's points round it stand, and how far its legs keep
/// off its surface, m.
pub const LANDMARK_CLEAR: f32 = 160.0;
pub const LANDMARK_LEG_CLEAR: f32 = 60.0;
/// The course keeps this far inside the sector's limit, m.
const EDGE: f32 = 500.0;
/// The auto-nav arrives this far off a landmark's or a rock's surface, m, and this far over a hide
/// spot's floor.
pub const STANDOFF: f32 = 150.0;
pub const OVER_SPOT: f32 = 120.0;
/// And this far off a suit it was sent to, alongside it.
pub const ALONGSIDE: f32 = 300.0;
/// And this far off the colony's hull (it can't be landed on: its hull moves at 177 m/s).
pub const OFF_HULL: f32 = 800.0;
/// The auto-nav cruises no faster than this, m/s, whatever the frame's flight assist allows: fast
/// enough to cross the sector in two minutes, slow enough to turn round the colony's ring.
pub const NAV_CRUISE: f32 = 300.0;
/// It plans to brake with this share of the weakest of its thrusters, so it arrives without
/// leaning on all of them, whichever way the pilot has the suit turned (and of an ion drive's
/// thrust, which shares itself among the axes: along any line it gives at least 1/√3 of it).
const BRAKE_SHARE: f32 = 0.5;
/// It's there when this close to the arrival point (m) and this slow over it (m/s).
pub const ARRIVE_RANGE: f32 = 40.0;
pub const ARRIVE_SPEED: f32 = 2.5;
/// A rock is sidestepped when the suit would pass this close to its surface within
/// [`ROCK_LOOKAHEAD`] seconds, m.
const ROCK_CLEAR: f32 = 45.0;
const ROCK_LOOKAHEAD: f32 = 4.0;
/// The course is plotted again every this many ticks while flown.
const REPLOT_TICKS: u32 = 10;
/// The share of the room round a turn the auto-nav lets itself cut into.
const TURN_ROOM: f32 = 0.35;
/// Where the way ahead (the next few seconds of it) runs close to something, it goes no faster
/// than this share of the room there a second, m/s, down to [`NEAR_SPEED`].
const AHEAD_SECONDS: f32 = 4.0;
const ROOM_SPEED: f32 = 0.6;
const NEAR_SPEED: f32 = 12.0;
/// It brakes once it would stop within this of something along the way it's moving, m.
const BACKSTOP: f32 = 40.0;

/// Somewhere on the chart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// The First Colony.
    Colony,
    /// The dock, off the docking hub's mouth.
    Dock,
    /// The debris field's middle.
    Field,
    /// A landmark, by id.
    Landmark(u8),
    /// A landmark's hide spot.
    HideSpot(u8, u8),
    /// A rock of the field, by index.
    Rock(u16),
    /// A suit in sight, by slot and generation.
    Suit(u16, u8),
    /// A point marked on the chart.
    Point(Vec3),
    Earth,
    Moon,
    Sun,
    Lagrange(Lagrange),
}

/// Where a place is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Located {
    /// Its middle, m.
    pub pos: Vec3,
    /// How fast it moves, m/s.
    pub vel: Vec3,
    /// How far it reaches from its middle, m (0 for a point).
    pub reach: f32,
}

/// Where the auto-nav brings the suit to rest, going to a place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrival {
    pub point: Vec3,
    /// The arrival point's own velocity (on a moving body, a moving suit), m/s.
    pub vel: Vec3,
    /// The body it is on or by, if any: the course may come close to it where it ends.
    pub body: Option<Body>,
}

impl Place {
    /// Whether a course can be set to it: everything in the sector. The Earth Sphere's places are
    /// charted, but no lane leaves L1 yet.
    pub fn reachable(self) -> bool {
        !matches!(self, Place::Earth | Place::Moon | Place::Sun | Place::Lagrange(_))
    }

    /// In the Earth Sphere, beyond the sector.
    pub fn beyond(self) -> bool {
        !self.reachable()
    }

    /// What it's called, for labels and the HUD (ASCII, caps).
    pub fn name(self, world: &World) -> String {
        match self {
            Place::Colony => "THE FIRST COLONY".into(),
            Place::Dock => "DOCK".into(),
            Place::Field => "DEBRIS FIELD".into(),
            Place::Landmark(k) => landmark(world, k).map_or("LANDMARK".into(), |d| d.name.to_string()),
            Place::HideSpot(k, s) => landmark(world, k)
                .and_then(|d| d.hides.get(usize::from(s)))
                .map_or("HIDE SPOT".into(), |h| h.name.to_string()),
            Place::Rock(i) => format!("ROCK {i}"),
            Place::Suit(slot, _) => suit_name(world, slot),
            Place::Point(_) => "NAV POINT".into(),
            Place::Earth => "EARTH".into(),
            Place::Moon => "THE MOON".into(),
            Place::Sun => "THE SUN".into(),
            Place::Lagrange(l) => l.name().into(),
        }
    }

    /// What kind of place it is (caps).
    pub fn kind(self) -> &'static str {
        match self {
            Place::Colony => "O'NEILL CYLINDER",
            Place::Dock => "DOCK",
            Place::Field => "DEBRIS FIELD",
            Place::Landmark(_) => "LANDMARK",
            Place::HideSpot(..) => "HIDE SPOT",
            Place::Rock(_) => "ROCK",
            Place::Suit(..) => "MOBILE SUIT",
            Place::Point(_) => "NAV POINT",
            Place::Earth => "PLANET",
            Place::Moon => "MOON",
            Place::Sun => "STAR",
            Place::Lagrange(_) => "LAGRANGE POINT",
        }
    }

    /// A line or two about it, in the world's voice (`docs/STORY.md`).
    pub fn about(self, world: &World, survival: bool) -> String {
        match self {
            Place::Colony => "The First Colony, L1-01 on the Consortium's papers: 32 km long, turning once every 113 s for a \
                 g at the rim. Its hull moves at 177 m/s, so nothing lands on it. The dock is off the docking \
                 hub at its -X end."
                .into(),
            Place::Dock if survival => "The dock's ring of amber lights. Come to rest inside it and press Enter to \
                 dock: the hold goes to the stores, and the suit to its bay."
                .into(),
            Place::Dock => "The dock's ring of amber lights. Come in slower than 25 m/s and the dock buys the hold \
                 and tops up propellant."
                .into(),
            Place::Field => "The debris of the colony's construction: asteroids hauled in for their metal. The \
                 Consortium's Dolls patrol over it, and shoot claim-jumpers."
                .into(),
            Place::Landmark(k) => landmark(world, k).map_or_else(String::new, about_landmark),
            Place::HideSpot(k, s) => match landmark(world, k).and_then(|d| Some((d, d.hides.get(usize::from(s))?))) {
                Some((d, h)) => format!(
                    "A bowl in {}. Crouched still in it, sensors lose you: you're seen only within {:.0} m. \
                     Leave your suit parked here and it's hidden while you're away.",
                    d.name, h.visual
                ),
                None => String::new(),
            },
            Place::Rock(i) => world.bodies.field.rocks().get(usize::from(i)).map_or_else(String::new, |r| {
                let ore = ["nickel-iron", "titanium", "volatiles", "exotic metals"][usize::from(r.ore.min(3))];
                let size = r.axes.max_element() * 2.0;
                if r.axes.min_element() >= bc_sim::bodies::GRIP_MIN_AXIS {
                    format!("{size:.0} m of {ore}. Big enough to land on (L), and to mine.")
                } else {
                    format!("{size:.0} m of {ore}. Too small to land on; a blade or a beam breaks ore off it.")
                }
            }),
            Place::Suit(slot, _) => suit_about(world, slot),
            Place::Point(_) => "A point you marked on the chart.".into(),
            Place::Earth => "Home of the Unified Earth Alignment, and of most of humanity. No lane runs down \
                 the well to Earth orbit yet."
                .into(),
            Place::Moon => "Lunar orbit opens when the Cluster's expeditions reach it. Its mass and Earth's \
                 balance at L1, which is why the colony is where it is."
                .into(),
            Place::Sun => "One astronomical unit away. The colony's mirrors turn its light in through the windows.".into(),
            Place::Lagrange(l) => l.about().into(),
        }
    }

    /// Where it is at render tick `t` (the Earth Sphere's places, where the chart puts them).
    pub fn locate(self, world: &World, t: f64) -> Option<Located> {
        let still = |pos: Vec3, reach: f32| Some(Located { pos, vel: Vec3::ZERO, reach });
        let bodies = &world.bodies;
        match self {
            Place::Colony => still(COLONY_CENTER, COLONY_HALF_LENGTH),
            Place::Dock => still(DOCK_CENTER, DOCK_RADIUS),
            Place::Field => still(FIELD_CENTER, 0.0),
            Place::Landmark(k) => {
                let pose = bodies.pose_at(Body::Landmark(k), t)?;
                Some(Located { pos: pose.pos, vel: pose.vel, reach: landmark(world, k)?.bound })
            }
            Place::HideSpot(k, s) => {
                let spot = landmark(world, k)?.hides.get(usize::from(s))?;
                let pose = bodies.pose_at(Body::Landmark(k), t)?;
                let p = pose.to_world(spot.center);
                Some(Located { pos: p, vel: pose.point_vel(p), reach: spot.radius })
            }
            Place::Rock(i) => {
                let field = &bodies.field;
                let r = field.rocks().get(usize::from(i)).filter(|_| !field.is_dead(usize::from(i)))?;
                still(r.pos, r.axes.max_element())
            }
            Place::Suit(slot, generation) => {
                let tr = world.entities.get(usize::from(slot))?.as_ref()?;
                if tr.latest.generation != generation {
                    return None;
                }
                let p = tr.sample(t, bodies);
                Some(Located { pos: p.pos, vel: p.vel, reach: frame(tr.latest.frame).radius })
            }
            Place::Point(p) => still(p, 0.0),
            Place::Earth => still(sphere::earth(), sphere::EARTH_RADIUS),
            Place::Moon => still(sphere::moon(), sphere::MOON_RADIUS),
            Place::Sun => still(sphere::SUN_DIR * sphere::AU, 696_000.0 * sphere::KM),
            Place::Lagrange(l) => still(l.pos(), 0.0),
        }
    }

    /// Where the auto-nav stops, going there from `from`: off its near side, over a hide spot's
    /// bowl, alongside a suit. None beyond the sector, or for what can't be found now.
    pub fn arrival(self, world: &World, t: f64, from: Vec3) -> Option<Arrival> {
        if !self.reachable() {
            return None;
        }
        let at = self.locate(world, t)?;
        let toward = |c: Vec3| (from - c).normalize_or(Vec3::Y);
        let still = |point: Vec3, body: Option<Body>| Some(Arrival { point, vel: Vec3::ZERO, body });
        match self {
            Place::Colony => {
                // Off the hull (or a cap) where it's nearest.
                let rel = from - COLONY_CENTER;
                let x = rel.x.clamp(-COLONY_HALF_LENGTH, COLONY_HALF_LENGTH);
                let radial = Vec3::new(0.0, rel.y, rel.z).normalize_or(Vec3::Y);
                let hull = COLONY_CENTER + Vec3::new(x, 0.0, 0.0) + radial * COLONY_RADIUS;
                let point = if rel.x.abs() > COLONY_HALF_LENGTH
                    && Vec3::new(0.0, rel.y, rel.z).length() < COLONY_RADIUS
                {
                    COLONY_CENTER + Vec3::new(rel.x.signum() * (COLONY_HALF_LENGTH + OFF_HULL), rel.y, rel.z)
                } else {
                    hull + radial * OFF_HULL
                };
                still(point, None)
            }
            Place::Dock => still(DOCK_CENTER, None),
            Place::Field | Place::Point(_) => still(at.pos, None),
            Place::Landmark(k) => {
                let body = Body::Landmark(k);
                let pose = world.bodies.pose_at(body, t)?;
                let shape = world.bodies.shape(body)?;
                // Out from its middle toward the suit, past its surface there.
                let dir = pose.rot.conjugate() * toward(at.pos);
                let surface = surface_along(&shape, dir, at.reach);
                let point = pose.to_world(surface + dir * STANDOFF);
                Some(Arrival { point, vel: pose.point_vel(point), body: Some(body) })
            }
            Place::HideSpot(k, s) => {
                let def = landmark(world, k)?;
                let spot = def.hides.get(usize::from(s))?;
                let body = Body::Landmark(k);
                let pose = world.bodies.pose_at(body, t)?;
                let n = def.shape.probe(spot.center).normal;
                let point = pose.to_world(spot.center + n * OVER_SPOT);
                Some(Arrival { point, vel: pose.point_vel(point), body: Some(body) })
            }
            Place::Rock(i) => {
                let r = world.bodies.field.rocks().get(usize::from(i))?;
                still(at.pos + toward(at.pos) * (r.axes.max_element() + STANDOFF), Some(Body::Rock(i)))
            }
            Place::Suit(..) => {
                Some(Arrival { point: at.pos + toward(at.pos) * ALONGSIDE, vel: at.vel, body: None })
            }
            Place::Earth | Place::Moon | Place::Sun | Place::Lagrange(_) => None,
        }
    }
}

fn landmark(world: &World, k: u8) -> Option<&'static LandmarkDef> {
    world.bodies.landmarks().get(usize::from(k))
}

/// How a landmark moves and what's on it.
fn about_landmark(d: &LandmarkDef) -> String {
    let motion = match (d.spin_period, d.orbit_radius > 0.0) {
        (0, false) => "Still.".to_string(),
        (spin, orbit) => {
            let mut s = String::new();
            if spin > 0 {
                s += &format!("Rolls once every {:.0} s", spin as f32 / bc_sim::TICK_HZ as f32);
            }
            if orbit {
                s += if spin > 0 { " and keeps" } else { "Keeps" };
                s += &format!(
                    " station on a {:.0} m circle every {:.0} min",
                    d.orbit_radius,
                    d.orbit_period as f32 / bc_sim::TICK_HZ as f32 / 60.0
                );
            }
            s + "."
        }
    };
    let size = d.bound * 2.0;
    let grip = if d.grippable { " Land on it with the grip (L)." } else { "" };
    let hides = if d.hides.is_empty() {
        String::new()
    } else {
        format!(" Hide spots: {}.", d.hides.iter().map(|h| h.name).collect::<Vec<_>>().join(", "))
    };
    format!("{size:.0} m across. {motion}{grip}{hides}")
}

/// Where a shape's surface is straight out from its middle along `dir` (its frame), searching
/// inward from `reach`.
fn surface_along(shape: &bc_sim::bodies::Shape, dir: Vec3, reach: f32) -> Vec3 {
    let from = dir * (reach + 10.0);
    match shape.trace_long(from, Vec3::ZERO, 0.0) {
        Some(s) => from * (1.0 - s),
        None => Vec3::ZERO,
    }
}

/// A suit's name as the chart labels it: its pilot's callsign, or what it is.
fn suit_name(world: &World, slot: u16) -> String {
    let Some(tr) = world.entities.get(usize::from(slot)).and_then(|t| t.as_ref()) else {
        return "SUIT".into();
    };
    let e = &tr.latest;
    match world.roster.get(&slot) {
        Some((name, _)) if e.pilot != PilotKind::MobileDoll => name.to_uppercase(),
        _ if e.pilot == PilotKind::MobileDoll => {
            format!("{} {}", frame_name(e.frame), doll_name()).to_uppercase()
        }
        _ => frame_name(e.frame).to_uppercase(),
    }
}

fn suit_about(world: &World, slot: u16) -> String {
    let Some(tr) = world.entities.get(usize::from(slot)).and_then(|t| t.as_ref()) else {
        return String::new();
    };
    let e = &tr.latest;
    let side = if e.faction == world.faction { "Friendly" } else { "Hostile" };
    let who = match e.pilot {
        PilotKind::MobileDoll => "no pilot: a Mobile Doll",
        PilotKind::Agent => "flown by an Arrival without a body (MD)",
        PilotKind::Human => "a pilot",
    };
    let faction = match e.faction {
        Faction::Oz => "the Consortium's security arm",
        Faction::Colonies => "the colonies' cause",
        Faction::Alliance => "the Unified Earth Alignment",
    };
    if e.flags & ent_flags::WRECK != 0 {
        return format!("The wreck of a {}. What's left of it can be salvaged.", frame_name(e.frame));
    }
    format!("{side}. A {}, {who}, flying for {faction}.", frame_name(e.frame))
}

/// The way from one place to another: the suit's position first, the arrival point last, and the
/// turns between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Course {
    pub points: Vec<Vec3>,
}

impl Course {
    /// How far it is, all the way, m.
    pub fn length(&self) -> f32 {
        self.points.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    /// Where it ends.
    pub fn end(&self) -> Option<Vec3> {
        self.points.last().copied()
    }

    /// The next point to fly at (the first turn, or the end).
    pub fn next(&self) -> Option<Vec3> {
        self.points.get(1).copied()
    }

    /// Points `step` m apart along it from its start, at most `n`, with the way it runs there.
    pub fn marks(&self, step: f32, n: usize) -> Vec<(Vec3, Vec3)> {
        self.marks_from(step, step, n)
    }

    /// Points `step` m apart along it, the first `first` m from its start (a chart's chevrons,
    /// flowing as `first` grows), at most `n`, with the way it runs there.
    pub fn marks_from(&self, first: f32, step: f32, n: usize) -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::new();
        if step <= 0.0 {
            return out;
        }
        let mut carry = first.max(0.0);
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = a.distance(b);
            let dir = (b - a).normalize_or(Vec3::Z);
            let mut s = carry;
            while s <= len {
                if out.len() >= n {
                    return out;
                }
                out.push((a + dir * s, dir));
                s += step;
            }
            carry = s - len;
        }
        out
    }
}

/// How far `p` is outside the colony's solid (its hull or a cap), m; negative inside.
pub fn colony_clearance(p: Vec3) -> f32 {
    let rel = p - COLONY_CENTER;
    let radial = Vec3::new(0.0, rel.y, rel.z).length() - COLONY_RADIUS;
    let along = rel.x.abs() - COLONY_HALF_LENGTH;
    if radial > 0.0 && along > 0.0 { (radial * radial + along * along).sqrt() } else { radial.max(along) }
}

/// What a course must keep off: the colony, and the landmarks where they are at a moment.
struct Obstacles {
    /// Each landmark's pose, shape and reach.
    landmarks: Vec<(bc_sim::bodies::BodyPose, bc_sim::bodies::Shape, f32)>,
}

impl Obstacles {
    fn new(bodies: &BodySet, t: f64) -> Self {
        let landmarks = (0..bodies.landmarks().len() as u8)
            .filter_map(|k| {
                let b = Body::Landmark(k);
                Some((bodies.pose_at(b, t)?, bodies.shape(b)?, bodies.landmarks()[usize::from(k)].bound))
            })
            .collect();
        Self { landmarks }
    }

    /// How far `p` is off landmark `k`'s surface, m.
    fn landmark_clearance(&self, k: usize, p: Vec3) -> f32 {
        let (pose, shape, bound) = &self.landmarks[k];
        let local = pose.to_local(p);
        // Far off, the bounding sphere says enough.
        let d = local.length() - bound;
        if d > LANDMARK_CLEAR * 2.0 { d } else { shape.probe(local).dist }
    }

    /// How far a suit at `from` can go along `dir` (unit), at most `max` m, before it meets the
    /// colony or a landmark (keeping a little off it).
    fn free_along(&self, from: Vec3, dir: Vec3, max: f32) -> f32 {
        let margin = (self.clearance(from) * 0.5).clamp(0.0, 20.0);
        let to = from + dir * max;
        let mut s = colony_sweep(from, to, margin).unwrap_or(1.0);
        for (pose, shape, bound) in &self.landmarks {
            if pose.pos.distance(from) > bound + max + margin {
                continue;
            }
            if let Some(hit) = shape.trace_long(pose.to_local(from), pose.to_local(to), margin) {
                s = s.min(hit);
            }
        }
        s * max
    }

    /// How far `p` is off the colony and every landmark, m.
    fn clearance(&self, p: Vec3) -> f32 {
        (0..self.landmarks.len()).map(|k| self.landmark_clearance(k, p)).fold(colony_clearance(p), f32::min)
    }

    /// Whether the leg `a → b` keeps clear of everything: the colony by [`COLONY_LEG_CLEAR`] and
    /// each landmark by [`LANDMARK_LEG_CLEAR`], or by most of what either end has, where an end is
    /// closer than that.
    fn clear(&self, a: Vec3, b: Vec3) -> bool {
        let margin = |full: f32, ca: f32, cb: f32| full.min(ca * 0.8).min(cb * 0.8).max(0.0);
        let m = margin(COLONY_LEG_CLEAR, colony_clearance(a), colony_clearance(b));
        if colony_sweep(a, b, m).is_some() {
            return false;
        }
        for (k, (pose, shape, bound)) in self.landmarks.iter().enumerate() {
            // A quick miss: the leg passes wide of its bounding sphere.
            let c = pose.pos;
            let ab = b - a;
            let s = ((c - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            if (a + ab * s).distance(c) > bound + LANDMARK_LEG_CLEAR {
                continue;
            }
            let m = margin(LANDMARK_LEG_CLEAR, self.landmark_clearance(k, a), self.landmark_clearance(k, b));
            if shape.trace_long(pose.to_local(a), pose.to_local(b), m).is_some() {
                return false;
            }
        }
        true
    }

    /// The points a course may turn at: a ring round the colony at five stations along it, and
    /// fourteen round each landmark (along its axes and its diagonals).
    fn waypoints(&self) -> Vec<Vec3> {
        let mut out = Vec::new();
        let r = COLONY_RADIUS + COLONY_CLEAR;
        let ends = COLONY_HALF_LENGTH + COLONY_CLEAR;
        for x in [-ends, -COLONY_HALF_LENGTH * 0.5, 0.0, COLONY_HALF_LENGTH * 0.5, ends] {
            for k in 0..8 {
                let a = k as f32 * core::f32::consts::TAU / 8.0;
                out.push(COLONY_CENTER + Vec3::new(x, r * a.cos(), r * a.sin()));
            }
        }
        for (pose, _, bound) in &self.landmarks {
            let d = bound + LANDMARK_CLEAR;
            let diag = d / 3f32.sqrt() * 1.15;
            for v in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
                out.push(pose.to_world(v * d));
            }
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    for sz in [-1.0, 1.0] {
                        out.push(pose.to_world(Vec3::new(sx, sy, sz) * diag));
                    }
                }
            }
        }
        out.retain(|p| p.abs().max_element() <= SECTOR_LIMIT - EDGE);
        out
    }
}

/// The course from `from` to `to`, keeping off the colony and the landmarks posed at render tick
/// `t`: straight there if the way is clear, else the shortest way through the points round them.
/// If nothing gets there (an end inside something), straight there.
pub fn plot(bodies: &BodySet, t: f64, from: Vec3, to: Vec3) -> Course {
    let obstacles = Obstacles::new(bodies, t);
    if obstacles.clear(from, to) {
        return Course { points: vec![from, to] };
    }
    // Dijkstra over the visibility graph: 0 is `from`, 1 is `to`, then the waypoints. Legs are
    // tested as they're reached, so most of the graph is never built.
    let mut nodes = vec![from, to];
    nodes.extend(obstacles.waypoints());
    let n = nodes.len();
    let mut dist = vec![f32::INFINITY; n];
    let mut prev = vec![usize::MAX; n];
    let mut done = vec![false; n];
    dist[0] = 0.0;
    let nearest = |done: &[bool], dist: &[f32]| {
        (0..n).filter(|&i| !done[i] && dist[i].is_finite()).min_by(|&a, &b| dist[a].total_cmp(&dist[b]))
    };
    while let Some(u) = nearest(&done, &dist) {
        if u == 1 {
            break;
        }
        done[u] = true;
        for v in 1..n {
            if done[v] || v == u {
                continue;
            }
            let d = dist[u] + nodes[u].distance(nodes[v]);
            if d < dist[v] && obstacles.clear(nodes[u], nodes[v]) {
                dist[v] = d;
                prev[v] = u;
            }
        }
    }
    if !dist[1].is_finite() {
        return Course { points: vec![from, to] };
    }
    let mut points = vec![to];
    let mut at = 1;
    while prev[at] != usize::MAX {
        at = prev[at];
        points.push(nodes[at]);
    }
    points.reverse();
    Course { points }
}

/// How hard the auto-nav plans to brake, m/s², on a trip of `dist` m by a suit of `spec` with
/// `propellant` kg in its tank, flying with `mods` (its stat sheet under the sector's rules, as its
/// prediction flies it: `Predictor::mods`). It's a share of the suit's weakest thrusters (with what
/// it carries), within what flight assist will pull on its pilot; but under the real rules a dry
/// tank brakes on an ion drive alone, a share of its thrust (on nothing without one), and a tank
/// too low for the trip runs dry on the way in and leaves the rest to the drive: then it's the
/// braking that stops the suit, from the fastest it flies the trip, in the distance the tank's
/// burn and the drive's crawl take together.
pub fn planned_braking(spec: &FrameSpec, mods: &FlightMods, propellant: f32, dist: f32) -> f32 {
    let extra = (mods.extra_mass_kg as f32).max(0.0);
    let mass = spec.mass(propellant) + extra;
    let weakest = spec.main_thrust.min(spec.side_thrust).min(spec.retro_thrust);
    let thrusters = (weakest / mass * BRAKE_SHARE).clamp(1.0, FA_G_CAP * G0 * BRAKE_SHARE);
    // Under anime rules flying burns nothing: an empty gauge brakes as a full one does.
    if mods.gauge.is_some() {
        return thrusters;
    }
    let drive = ion_thrust(spec) * mods.ion / (spec.mass(0.0) + extra) * BRAKE_SHARE;
    if propellant <= 0.0 {
        return drive;
    }
    // Without a drive the tank is all it has, as ever.
    if drive <= 0.0 {
        return thrusters;
    }
    // What the tank takes off before it's dry, by the rocket equation (counting none of the
    // drive's share, which burns nothing); and the fastest the trip is flown: as fast as the suit
    // can still stop from in `dist`, on the tank and then the drive, and no faster than its cruise.
    let tank = spec.exhaust_velocity() * mods.isp * (mass / (mass - propellant)).ln();
    let d = dist.max(0.0);
    let top = if 2.0 * thrusters * d <= tank * tank {
        (2.0 * thrusters * d).sqrt()
    } else {
        // (top² − rest²)/2a + rest²/2b = d, braking at a on the tank and b on the drive, with
        // rest = top − tank.
        let k = tank / thrusters;
        tank + drive * (-k + (k * k - (tank * k - 2.0 * d) / drive).sqrt())
    };
    let top = top.min(spec.fa_speed.min(NAV_CRUISE));
    // The tank takes all of it off, as ever; or the drive takes the rest.
    let rest = top - tank;
    if rest <= 0.0 {
        return thrusters;
    }
    let v2 = top * top;
    v2 / ((v2 - rest * rest) / thrusters + rest * rest / drive)
}

/// How long to cover `dist` m starting at `speed` m/s toward it (negative: away), cruising at
/// `cruise` and braking at `brake` m/s² to stop at the end (and speeding up as hard), s: never,
/// with nothing to brake with (a dry tank and no drive).
pub fn eta(dist: f32, speed: f32, cruise: f32, brake: f32) -> f32 {
    if dist <= 0.0 {
        return 0.0;
    }
    if brake <= 0.0 {
        return f32::INFINITY;
    }
    let mut t = 0.0;
    let mut v = speed;
    // Turn round first if it's heading away.
    if v < 0.0 {
        t += -v / brake;
        let back = v * v / (2.0 * brake);
        return t + eta(dist + back, 0.0, cruise, brake);
    }
    // Already too fast to stop in time: it overshoots and comes back.
    let stop = v * v / (2.0 * brake);
    if stop > dist {
        t += v / brake;
        return t + eta(stop - dist, 0.0, cruise, brake);
    }
    v = v.min(cruise);
    // Speed up to the peak the distance allows (no higher than the cruise), then brake.
    let peak = ((2.0 * brake * dist + v * v) / 2.0).sqrt().min(cruise);
    let up = (peak - v) / brake;
    let up_dist = (peak * peak - v * v) / (2.0 * brake);
    let down = peak / brake;
    let down_dist = peak * peak / (2.0 * brake);
    let level = (dist - up_dist - down_dist).max(0.0) / peak.max(1e-3);
    t + up + level + down
}

/// About how much propellant (kg) a suit of `spec` carrying `propellant` burns going `dist` m by
/// the real flight rules: up to the speed the trip allows (no higher than `cruise`), and down
/// again (braking at `brake`), by the rocket equation. `isp` is its stat sheet's specific impulse
/// (the propellant's grade, a thruster kit).
pub fn burn_estimate(
    spec: &bc_sim::content::FrameSpec,
    isp: f32,
    propellant: f32,
    dist: f32,
    cruise: f32,
    brake: f32,
) -> f32 {
    let peak = (brake * dist.max(0.0)).sqrt().min(cruise);
    let mass = spec.mass(propellant);
    (mass * (1.0 - (-2.0 * peak / (spec.exhaust_velocity() * isp)).exp())).min(propellant.max(0.0))
}

/// What a pilot flying by hand is told about getting there: speed up, hold it, or brake now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// Closing too slowly (or going away).
    Burn,
    /// Closing about right.
    Coast,
    /// Closing too fast to stop in time: brake.
    Brake,
}

/// The cue for `dist` m to go, closing at `closing` m/s, braking at `brake` m/s².
pub fn cue(dist: f32, closing: f32, brake: f32) -> Cue {
    if closing <= 0.0 {
        return Cue::Burn;
    }
    let stop = closing * closing / (2.0 * brake);
    if stop >= dist * 0.85 {
        Cue::Brake
    } else if closing < (2.0 * brake * dist).sqrt().min(NAV_CRUISE) * 0.5 {
        Cue::Burn
    } else {
        Cue::Coast
    }
}

/// How an auto-nav run is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavState {
    /// On the way.
    Flying,
    /// At rest at the arrival point: the stick is the pilot's again.
    Arrived,
    /// What it was going to can't be found (a suit gone from sight, a rock shattered).
    Lost,
}

/// The stick, as the auto-nav holds it this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavCmd {
    /// Thrust per axis of the suit, as the command carries it (flight assist: a velocity asked
    /// for, as a share of the frame's cruise).
    pub thrust: [i8; 3],
    /// Which way it would have the suit look: where it's going.
    pub aim: Vec3,
    /// Buttons it holds: flight assist.
    pub buttons: u16,
}

/// The auto-nav: flies the suit to a place along its course.
#[derive(Clone, Debug)]
pub struct AutoNav {
    pub place: Place,
    pub state: NavState,
    course: Course,
    plotted: Option<u32>,
    /// The arrival point, as last worked out.
    pub arrival: Option<Arrival>,
    /// It turns the suit to look where it's going (until the pilot takes the aim back with the
    /// mouse: it keeps flying the course, whichever way the suit looks).
    pub look: bool,
}

impl AutoNav {
    pub fn new(place: Place) -> Self {
        Self {
            place,
            state: NavState::Flying,
            course: Course::default(),
            plotted: None,
            arrival: None,
            look: true,
        }
    }

    /// The course it's flying.
    pub fn course(&self) -> &Course {
        &self.course
    }

    /// The command for the tick `ctx` is for, or None once it's arrived or lost what it was after.
    pub fn decide(&mut self, ctx: &InputContext) -> Option<NavCmd> {
        if self.state != NavState::Flying {
            return None;
        }
        let own = ctx.world.own.filter(|o| o.alive)?;
        let s = &ctx.predict.state;
        let t = f64::from(ctx.tick);
        let Some(arrival) = self.place.arrival(ctx.world, t, s.pos) else {
            self.state = NavState::Lost;
            return None;
        };
        self.arrival = Some(arrival);
        let rel = s.vel - arrival.vel;
        if s.pos.distance(arrival.point) < ARRIVE_RANGE && rel.length() < ARRIVE_SPEED {
            self.state = NavState::Arrived;
            return None;
        }
        if self.plotted.is_none_or(|at| ctx.tick >= at + REPLOT_TICKS) || self.course.points.is_empty() {
            self.course = plot(&ctx.world.bodies, t, s.pos, arrival.point);
            self.plotted = Some(ctx.tick);
        } else {
            self.course.points[0] = s.pos;
            if let Some(end) = self.course.points.last_mut() {
                *end = arrival.point;
            }
        }
        let spec = frame(own.frame);
        let left = self.course.length();
        let brake = planned_braking(spec, ctx.predict.mods(), s.propellant, left);
        let cruise = spec.fa_speed.min(NAV_CRUISE);
        let next = self.course.next().unwrap_or(arrival.point);
        // As fast as it can still stop from at the end, easing in over the last few metres.
        let mut speed =
            (2.0 * brake * (left - ARRIVE_RANGE * 0.25).max(0.0)).sqrt().min(left * 0.6).min(cruise);
        let obstacles = Obstacles::new(&ctx.world.bodies, t);
        // Slow enough at the next turn to make it without cutting in on what it turns round: the
        // corner cut, v²(1 − cos ½θ)/a, is kept to a share of the room there.
        if let [_, via, after, ..] = self.course.points[..] {
            let half = (via - s.pos).angle_between(after - via) * 0.5;
            let room = TURN_ROOM * obstacles.clearance(via);
            let bend = 1.0 - half.cos();
            let turn = if bend > 1e-4 { (room * brake * 2.0 / bend).sqrt() } else { cruise };
            speed = speed.min((turn * turn + 2.0 * brake * s.pos.distance(via)).sqrt());
        }
        // Slow where the way ahead runs close to something, so it holds to the course there.
        let ahead = s.vel.length().max(speed) * AHEAD_SECONDS;
        let room = self
            .course
            .marks(ahead / 8.0, 8)
            .into_iter()
            .map(|(p, _)| obstacles.clearance(p))
            .fold(obstacles.clearance(s.pos), f32::min);
        speed = speed.min((room * ROOM_SPEED).max(NEAR_SPEED));
        let dir = (next - s.pos).normalize_or(Vec3::ZERO);
        // Near the end, keep pace with where it's going.
        let pace = if left < 2_000.0 { arrival.vel } else { Vec3::ZERO };
        let mut want = pace + dir * speed;
        want = sidestep_rocks(ctx, s.pos, want);
        // And never carry on toward something it couldn't stop short of: whatever the course
        // says, it adds nothing to its way toward it, and flight assist brakes. (With nothing to
        // brake with, it can't stop short of anything.)
        let moving = s.vel - pace;
        if let Some(way) = moving.try_normalize() {
            let v = moving.length();
            let stop = v * v / (2.0 * brake * 2.0);
            if !stop.is_finite() || stop + BACKSTOP > obstacles.free_along(s.pos, way, stop + 300.0) {
                want -= way * want.dot(way).max(0.0);
            }
        }
        let aim = if (want - pace).length() > 5.0 {
            (want - pace).normalize()
        } else {
            (arrival.point - s.pos).normalize_or(s.rot * Vec3::Z)
        };
        let stick = s.rot.conjugate() * want / spec.fa_speed;
        let q = |x: f32| (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
        Some(NavCmd { thrust: [q(stick.x), q(stick.y), q(stick.z)], aim, buttons: FLIGHT_ASSIST })
    }
}

/// Bends `want` (a velocity, m/s) round any rock the suit at `pos` would pass too close to in the
/// next few seconds.
fn sidestep_rocks(ctx: &InputContext, pos: Vec3, want: Vec3) -> Vec3 {
    let speed = want.length();
    if speed < 1.0 {
        return want;
    }
    let reach = speed * ROCK_LOOKAHEAD;
    let field = &ctx.world.bodies.field;
    let (lo, hi) = (pos.min(pos + want * ROCK_LOOKAHEAD), pos.max(pos + want * ROCK_LOOKAHEAD));
    let pad = Vec3::splat(ROCK_CLEAR + 200.0);
    let dir = want / speed;
    let mut worst: Option<(f32, Vec3)> = None;
    field.for_each_in_box(lo - pad, hi + pad, |i| {
        if field.is_dead(i) {
            return;
        }
        let r = &field.rocks()[i];
        let rel = r.pos - pos;
        let along = rel.dot(dir);
        if along < 0.0 || along > reach {
            return;
        }
        let side = rel - dir * along;
        let gap = side.length() - r.radius - ROCK_CLEAR;
        if gap < 0.0 && worst.is_none_or(|(a, _)| along < a) {
            worst = Some((along, side));
        }
    });
    let Some((_, side)) = worst else { return want };
    // Away from it, across the way: as hard as the way is fast.
    let away = (-side).try_normalize().unwrap_or_else(|| dir.any_orthonormal_vector());
    (dir + away * 0.8).normalize() * speed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputHistory, Predictor};
    use bc_proto::{FrameId, InputCmd, OwnState, SnapshotHeader, SnapshotReader, SnapshotWriter};
    use bc_sim::content::modules::MOUNTS;
    use bc_sim::content::systems::DAMAGED;
    use bc_sim::content::{ModuleKind, Modules, System, Systems};
    use bc_sim::tuning::{FlightRules, flight_mods, tuning};
    use bc_sim::{Sim, SimConfig};
    use std::sync::Arc;

    fn bodies(rocks: u16) -> BodySet {
        let field = bc_sim::field::Field::generate(bc_sim::field::Field::DEFAULT_SEED, rocks);
        BodySet::new(Arc::new(field), 2)
    }

    #[test]
    fn a_clear_way_is_straight() {
        let b = bodies(0);
        let (a, z) = (Vec3::new(-2_000.0, 900.0, 0.0), Vec3::new(3_000.0, 1_500.0, 2_000.0));
        assert_eq!(plot(&b, 0.0, a, z).points, vec![a, z]);
    }

    /// Every leg of `c` keeps off the colony and the landmarks.
    fn keeps_off(b: &BodySet, t: f64, c: &Course) {
        for w in c.points.windows(2) {
            let n = (w[0].distance(w[1]) / 20.0).ceil() as usize;
            for k in 0..=n {
                let p = w[0].lerp(w[1], k as f32 / n as f32);
                // Where the ends are themselves close, the legs keep most of what they have.
                let need = 0.5f32.min(colony_clearance(w[0]) * 0.5).min(colony_clearance(w[1]) * 0.5);
                assert!(colony_clearance(p) > need.min(100.0), "{p} in the colony on {:?}", c.points);
                for (k, def) in b.landmarks().iter().enumerate() {
                    let pose = b.pose_at(Body::Landmark(k as u8), t).unwrap();
                    let d = def.shape.probe(pose.to_local(p)).dist;
                    let ends = |q: Vec3| def.shape.probe(pose.to_local(q)).dist;
                    let need = (LANDMARK_LEG_CLEAR * 0.5).min(ends(w[0]) * 0.5).min(ends(w[1]) * 0.5);
                    assert!(d > need - 1.0, "{p} {d} m off {} on {:?}", def.name, c.points);
                }
            }
        }
    }

    #[test]
    fn courses_go_round_the_colony_and_the_landmarks() {
        let b = bodies(0);
        let hermit = b.pose_at(Body::Landmark(1), 0.0).unwrap().pos;
        let mo = b.pose_at(Body::Landmark(0), 0.0).unwrap().pos;
        let cases = [
            // Over the colony to beneath it.
            (Vec3::new(0.0, 2_000.0, 0.0), Vec3::new(1_000.0, -11_000.0, 500.0)),
            // End to end along its side.
            (Vec3::new(-19_000.0, -4_000.0, 4_000.0), Vec3::new(19_000.0, -4_200.0, -4_100.0)),
            // The field to the dock, round the −X end.
            (Vec3::new(-4_000.0, 800.0, -3_000.0), DOCK_CENTER),
            // Through Hermit, and through MO-II.
            (hermit + Vec3::new(-3_000.0, 0.0, 0.0), hermit + Vec3::new(3_000.0, 100.0, 0.0)),
            (mo + Vec3::new(-1_500.0, 50.0, 0.0), mo + Vec3::new(1_500.0, 0.0, 30.0)),
        ];
        for (from, to) in cases {
            let c = plot(&b, 0.0, from, to);
            assert!(c.points.len() > 2, "straight through: {:?}", c.points);
            assert_eq!((c.points[0], *c.points.last().unwrap()), (from, to));
            keeps_off(&b, 0.0, &c);
            // Not a long way round: within 1.8× the crow's flight.
            assert!(
                c.length() < from.distance(to) * 1.8 + 3_000.0,
                "{} m for {}",
                c.length(),
                from.distance(to)
            );
        }
    }

    #[test]
    fn courses_between_many_places_keep_off_everything() {
        let b = bodies(0);
        let mut rng = bc_sim::math::Rng::new(7);
        let mut r = || rng.signed();
        for _ in 0..120 {
            let from = Vec3::new(r() * 25_000.0, r() * 25_000.0, r() * 25_000.0);
            let to = Vec3::new(r() * 25_000.0, r() * 25_000.0, r() * 25_000.0);
            if colony_clearance(from) < 50.0 || colony_clearance(to) < 50.0 {
                continue;
            }
            let c = plot(&b, 0.0, from, to);
            keeps_off(&b, 0.0, &c);
        }
    }

    #[test]
    fn eta_and_cues_add_up() {
        // From rest, 10 km at 300 m/s braking at 10 m/s²: 30 s up, 30 s down (9 km), 1 km level.
        let t = eta(10_000.0, 0.0, 300.0, 10.0);
        assert!((t - (30.0 + 30.0 + 1_000.0 / 300.0)).abs() < 0.01, "{t}");
        // Short hops never reach the cruise.
        let t = eta(500.0, 0.0, 300.0, 10.0);
        assert!((t - 2.0 * (500.0f32 / 10.0).sqrt()).abs() < 0.01, "{t}");
        // Heading away costs the turn.
        assert!(eta(1_000.0, -50.0, 300.0, 10.0) > eta(1_000.0, 0.0, 300.0, 10.0) + 5.0);
        assert_eq!(cue(1_000.0, 200.0, 10.0), Cue::Brake);
        assert_eq!(cue(10_000.0, 10.0, 10.0), Cue::Burn);
        assert_eq!(cue(10_000.0, 250.0, 10.0), Cue::Coast);
        assert_eq!(cue(10_000.0, -5.0, 10.0), Cue::Burn);
        // Up to 220 m/s and back down costs a Leo what the rocket equation says, and a short hop
        // less than a long one.
        let leo = bc_sim::content::frame(bc_proto::FrameId::Leo);
        let burn = burn_estimate(leo, 1.0, 2_000.0, 20_000.0, 220.0, 8.0);
        let want = leo.mass(2_000.0) * (1.0 - (-440.0 / leo.exhaust_velocity()).exp());
        assert!((burn - want).abs() < 1.0, "{burn} {want}");
        assert!(burn_estimate(leo, 1.0, 2_000.0, 500.0, 220.0, 8.0) < burn);
        // A purer propellant burns less for the same trip.
        assert!(burn_estimate(leo, 1.35, 2_000.0, 20_000.0, 220.0, 8.0) < burn * 0.8);
    }

    #[test]
    fn course_marks_are_evenly_spaced() {
        let c = Course { points: vec![Vec3::ZERO, Vec3::X * 250.0, Vec3::new(250.0, 0.0, 250.0)] };
        let m = c.marks(100.0, 10);
        assert_eq!(m.len(), 5);
        assert!((m[2].0 - Vec3::new(250.0, 0.0, 50.0)).length() < 1e-3, "{:?}", m[2]);
        assert_eq!(c.marks(100.0, 2).len(), 2);
    }

    /// The own state as the client decodes it.
    fn over_the_wire(own: &OwnState) -> OwnState {
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.header(&SnapshotHeader::default());
        w.own(Some(own));
        let n = w.finish().unwrap();
        SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
    }

    /// Flies `frame_id` from `start` to `place` on the auto-nav, as its client would, for at most
    /// `ticks`: the tick it arrived (if it did), and the closest it came to the colony's hull and
    /// to each landmark's surface, and how many rocks it touched.
    fn fly(
        frame_id: FrameId,
        rocks: u16,
        start: Vec3,
        place: Place,
        ticks: u32,
    ) -> (Option<u32>, f32, f32, u32) {
        fly_by(FlightRules::Real, frame_id, rocks, start, place, ticks)
    }

    /// [`fly`], by these flight rules.
    fn fly_by(
        rules: FlightRules,
        frame_id: FrameId,
        rocks: u16,
        start: Vec3,
        place: Place,
        ticks: u32,
    ) -> (Option<u32>, f32, f32, u32) {
        let trip = fly_fitted(rules, frame_id, rocks, start, place, ticks, |_, _| {});
        (trip.arrived, trip.hull, trip.landmark, trip.touched)
    }

    /// What a trip on the auto-nav came to: the tick it arrived (if it did), the closest it came
    /// to the colony's hull and to each landmark's surface, how many rocks it touched, and where
    /// it was after each tick.
    struct Trip {
        arrived: Option<u32>,
        hull: f32,
        landmark: f32,
        touched: u32,
        path: Vec<Vec3>,
    }

    /// [`fly_by`], with the suit fitted out by `fit` (the sim, and its index there) before it sets
    /// off.
    fn fly_fitted(
        rules: FlightRules,
        frame_id: FrameId,
        rocks: u16,
        start: Vec3,
        place: Place,
        ticks: u32,
        fit: impl FnOnce(&mut Sim, usize),
    ) -> Trip {
        let mut sim = Sim::new(SimConfig {
            target_dolls: 0,
            field_rocks: rocks,
            flight: rules,
            ..SimConfig::default()
        });
        let id = sim
            .spawn_at(
                frame_id,
                Faction::Colonies,
                PilotKind::Human,
                start,
                bc_sim::math::look_rotation(Vec3::Z, Vec3::Y),
            )
            .unwrap();
        let i = id.idx();
        fit(&mut sim, i);
        let (mut world, mut predict, mut history) =
            (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
        world.bodies = BodySet::new(Arc::new(sim.field.clone()), sim.landmarks().len() as u8);
        predict.set_field(sim.field.clone());
        predict.set_landmarks(sim.landmarks().len() as u8);
        predict.set_rules(rules);
        let mut nav = AutoNav::new(place);
        let mut trip = Trip {
            arrived: None,
            hull: f32::INFINITY,
            landmark: f32::INFINITY,
            touched: 0,
            path: Vec::new(),
        };
        for t in 1..=ticks {
            let own = over_the_wire(&sim.own_state(i));
            world.apply(t - 1, Some(own), None, &[], &[]);
            predict.reconcile(t - 1, &own, &history);
            let ctx = InputContext {
                tick: t,
                view_tick: f64::from(t),
                resolve_tick: f64::from(t),
                now: 0.0,
                world: &world,
                predict: &predict,
            };
            let Some(cmd) = nav.decide(&ctx) else {
                trip.arrived = (nav.state == NavState::Arrived).then_some(t);
                return trip;
            };
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: cmd.aim,
                thrust: cmd.thrust,
                buttons: cmd.buttons,
                ..InputCmd::default()
            }
            .quantized();
            history.push(cmd);
            sim.set_input(id, cmd);
            sim.step();
            let p = sim.suits.flight[i].pos;
            trip.path.push(p);
            trip.hull = trip.hull.min(colony_clearance(p));
            let b = bc_sim::bodies::Bodies::at(&sim.field, sim.landmarks(), sim.tick());
            for (k, def) in sim.landmarks().iter().enumerate() {
                let pose = b.pose(Body::Landmark(k as u8)).unwrap();
                trip.landmark = trip.landmark.min(def.shape.probe(pose.to_local(p)).dist);
            }
            let radius = frame(frame_id).radius;
            trip.touched += u32::from(
                sim.field
                    .rocks()
                    .iter()
                    .enumerate()
                    .any(|(k, r)| !sim.field.is_dead(k) && r.touches(p, radius)),
            );
        }
        trip
    }

    #[test]
    fn the_auto_nav_flies_round_the_colony_to_the_dock_and_stops_in_it() {
        // From over the field, the far side of the colony from the dock.
        let (arrived, hull, landmark, _) =
            fly(FrameId::Leo, 0, Vec3::new(6_000.0, -2_000.0, 6_500.0), Place::Dock, 30 * 240);
        let t = arrived.expect("never arrived");
        assert!(hull > 150.0, "came within {hull} m of the hull");
        assert!(landmark > 30.0, "came within {landmark} m of a landmark");
        assert!(t < 30 * 200, "took {} s", t / 30);
    }

    #[test]
    fn the_auto_nav_comes_to_rest_over_a_hide_spot_on_a_rolling_station() {
        // From Hermit to over MO-II's Aft Well, which faces away.
        let (arrived, _, landmark, _) = fly(
            FrameId::WingZero,
            0,
            Vec3::new(-14_000.0, 2_200.0, 4_500.0),
            Place::HideSpot(0, 0),
            30 * 180,
        );
        assert!(arrived.is_some(), "never arrived");
        assert!(landmark > 20.0, "came within {landmark} m of a landmark");
    }

    #[test]
    fn the_auto_nav_threads_the_field_without_touching_a_rock() {
        // Across the field's middle, rocks and all, to Hermit.
        let (arrived, _, landmark, touched) =
            fly(FrameId::Heavyarms, 160, Vec3::new(-6_000.0, 700.0, 3_000.0), Place::Landmark(1), 30 * 240);
        assert!(arrived.is_some(), "never arrived");
        assert_eq!(touched, 0, "touched rocks");
        assert!(landmark > 60.0, "came within {landmark} m of a landmark");
    }

    #[test]
    fn every_frame_flies_the_auto_nav_everywhere_without_touching_anything() {
        let mo = Vec3::new(-17_500.0, 1_500.0, 3_000.0);
        let trips = [
            // Out of the dock to MO-II's Aft Well, round the station.
            (DOCK_CENTER + Vec3::new(-200.0, 0.0, 0.0), Place::HideSpot(0, 0)),
            // From MO-II across the field to Hermit's KEYHOLE.
            (mo + Vec3::new(600.0, 200.0, 0.0), Place::HideSpot(1, 1)),
            // From under the colony to a rock in the field.
            (Vec3::new(3_000.0, -12_000.0, -2_000.0), Place::Rock(40)),
            // From Hermit's far side home to the dock.
            (Vec3::new(9_000.0, 5_000.0, -14_500.0), Place::Dock),
            // A point marked high over the field.
            (Vec3::new(-5_000.0, 900.0, 5_000.0), Place::Point(Vec3::new(6_000.0, 9_000.0, -4_000.0))),
        ];
        let mut report = String::new();
        for rules in [FlightRules::Real, FlightRules::Anime] {
            for &f in bc_sim::content::PLAYABLE_ORDER.iter() {
                for (k, &(start, place)) in trips.iter().enumerate() {
                    let (arrived, hull, landmark, touched) = fly_by(rules, f, 160, start, place, 30 * 300);
                    report += &format!(
                        "{rules:?} {f:?} trip {k}: {:?} s, hull {hull:.0} m, landmarks {landmark:.0} m, rocks {touched}\n",
                        arrived.map(|t| t / 30)
                    );
                    assert!(arrived.is_some(), "{f:?} never arrived on trip {k}\n{report}");
                    assert!(hull > 150.0 && landmark > 20.0 && touched == 0, "{f:?} trip {k}\n{report}");
                }
            }
        }
        eprintln!("{report}");
    }

    /// A suit's modules with an ion drive on its backpack.
    fn ion_drive_fitted() -> Modules {
        let mut m = Modules::NONE;
        let slot = MOUNTS.iter().position(|p| *p == ModuleKind::IonDrive.part()).unwrap();
        m.set(slot, Some(ModuleKind::IonDrive));
        m
    }

    /// Fits suit `i` with an ion drive and leaves `propellant` kg in its tank.
    fn ion_drive(sim: &mut Sim, i: usize, propellant: f32) {
        sim.suits.modules[i] = ion_drive_fitted();
        sim.suits.retune(i);
        sim.suits.flight[i].propellant = propellant;
    }

    #[test]
    fn the_braking_planned_is_what_the_suit_has_to_brake_with() {
        let leo = frame(FrameId::Leo);
        let weakest = leo.main_thrust.min(leo.side_thrust).min(leo.retro_thrust);
        let thrusters =
            |kg: f32, extra: f32| (weakest / (leo.mass(kg) + extra) * 0.5).clamp(1.0, FA_G_CAP * G0 * 0.5);
        let mods = |systems: Systems, modules: Modules, rules: FlightRules| {
            flight_mods(&tuning(0, systems, modules), rules, false, modules.mass_kg(0) as i32)
        };
        let plain = mods(Systems::OK, Modules::NONE, FlightRules::Real);
        let ion = mods(Systems::OK, ion_drive_fitted(), FlightRules::Real);
        let drive = ion_thrust(leo) / (leo.mass(0.0) + 250.0) * 0.5;
        for d in [100.0, 5_000.0, 50_000.0] {
            // Without a drive, a share of the weakest thrusters as ever, and nothing on a dry tank.
            for kg in [1.0, 300.0, 3_000.0] {
                assert_eq!(planned_braking(leo, &plain, kg, d), thrusters(kg, 0.0));
            }
            assert_eq!(planned_braking(leo, &plain, 0.0, d), 0.0);
            // With one: a full tank brakes as ever, a dry one on half the drive.
            assert_eq!(planned_braking(leo, &ion, 3_000.0, d), thrusters(3_000.0, 250.0));
            assert_eq!(planned_braking(leo, &ion, 0.0, d), drive);
            // Under anime rules flying burns nothing: an empty gauge brakes as a full one.
            for m in [Modules::NONE, ion_drive_fitted()] {
                let anime = mods(Systems::OK, m, FlightRules::Anime);
                let extra = m.mass_kg(0) as f32;
                assert_eq!(planned_braking(leo, &anime, 0.0, d), thrusters(0.0, extra));
            }
        }
        // Half the drive on a damaged reactor.
        let damaged = mods(Systems::OK.with(System::Reactor, DAMAGED), ion_drive_fitted(), FlightRules::Real);
        assert_eq!(planned_braking(leo, &damaged, 0.0, 5_000.0), drive * 0.5);
        // A low tank (60 kg, some 70 m/s): the thrusters' on a short hop it covers, and less the
        // further the trip, down toward the drive's; all but empty, the drive's.
        let low = |d: f32| planned_braking(leo, &ion, 60.0, d);
        assert_eq!(low(100.0), thrusters(60.0, 250.0));
        assert!(low(100.0) > low(500.0) && low(500.0) > low(5_000.0) && low(5_000.0) > low(20_000.0));
        assert!(low(20_000.0) > drive, "{} {drive}", low(20_000.0));
        assert!((planned_braking(leo, &ion, 0.01, 5_000.0) - drive).abs() < 0.01 * drive);
    }

    /// How far a trip from `from` went past `to`, along the way between them, m.
    fn past(trip: &Trip, from: Vec3, to: Vec3) -> f32 {
        let way = (to - from).normalize();
        trip.path.iter().map(|p| (*p - to).dot(way)).fold(f32::MIN, f32::max)
    }

    /// High over the field, 5 km to a point marked on the chart, with nothing in between.
    const CRAWL_FROM: Vec3 = Vec3::new(-4_000.0, 7_000.0, 6_000.0);
    const CRAWL_TO: Vec3 = Vec3::new(-1_000.0, 8_000.0, 2_000.0);

    #[test]
    fn a_dry_suit_crawls_on_its_ion_drive_to_where_it_was_sent_and_stops_there() {
        // A Leo with an ion drive and a dry tank: a little over 0.1 g, all the way.
        let (from, to) = (CRAWL_FROM, CRAWL_TO);
        let trip =
            fly_fitted(FlightRules::Real, FrameId::Leo, 0, from, Place::Point(to), 30 * 400, |sim, i| {
                ion_drive(sim, i, 0.0)
            });
        let past = past(&trip, from, to);
        let t = trip.arrived.unwrap_or_else(|| panic!("never arrived (went {past:.0} m past it)"));
        assert!(past < ARRIVE_RANGE, "went {past:.0} m past it");
        assert!(t < 30 * 200, "took {} s", t / 30);
    }

    #[test]
    fn a_tank_that_runs_dry_on_the_way_in_leaves_the_rest_to_the_drive() {
        // The same Leo on a low tank: heading in at 150 m/s with 60 kg left (some 70 m/s of
        // braking), and setting off from rest with 300 kg (enough to get up to its cruise, not to
        // stop from it as well).
        let (from, to) = (CRAWL_FROM, CRAWL_TO);
        for (kg, speed) in [(60.0, 150.0), (300.0, 0.0)] {
            let trip =
                fly_fitted(FlightRules::Real, FrameId::Leo, 0, from, Place::Point(to), 30 * 400, |sim, i| {
                    ion_drive(sim, i, kg);
                    let way = (to - from).normalize();
                    let f = &mut sim.suits.flight[i];
                    (f.vel, f.rot) = (way * speed, bc_sim::math::look_rotation(way, Vec3::Y));
                });
            let past = past(&trip, from, to);
            let t = trip
                .arrived
                .unwrap_or_else(|| panic!("{kg} kg at {speed} m/s never arrived (went {past:.0} m past it)"));
            assert!(past < ARRIVE_RANGE, "{kg} kg at {speed} m/s went {past:.0} m past it");
            assert!(t < 30 * 200, "{kg} kg at {speed} m/s took {} s", t / 30);
        }
    }

    #[test]
    fn places_are_named_found_and_arrived_at() {
        let mut world = World::new(Faction::Colonies);
        world.bodies = bodies(160);
        let from = Vec3::new(2_000.0, 3_000.0, -1_000.0);
        let mut seen = std::collections::HashSet::new();
        for place in [
            Place::Colony,
            Place::Dock,
            Place::Field,
            Place::Landmark(0),
            Place::Landmark(1),
            Place::HideSpot(0, 0),
            Place::HideSpot(1, 2),
            Place::Rock(3),
            Place::Point(Vec3::new(1.0, 2.0, 3.0)),
        ] {
            let name = place.name(&world);
            assert!(name.is_ascii() && name == name.to_uppercase() && !name.is_empty(), "{name}");
            assert!(seen.insert(name.clone()), "{name} twice");
            assert!(
                !place.about(&world, true).is_empty() && !place.about(&world, false).is_empty(),
                "{name}"
            );
            let at = place.locate(&world, 100.0).unwrap();
            let arrive = place.arrival(&world, 100.0, from).unwrap();
            // Arrival points are out in the open: off the colony, off the landmarks.
            assert!(colony_clearance(arrive.point) > 500.0, "{name}: {}", arrive.point);
            for (k, def) in world.bodies.landmarks().iter().enumerate() {
                let pose = world.bodies.pose_at(Body::Landmark(k as u8), 100.0).unwrap();
                assert!(def.shape.probe(pose.to_local(arrive.point)).dist > 60.0, "{name} by {}", def.name);
            }
            assert!(at.pos.distance(arrive.point) < at.reach + 1_000.0, "{name}");
        }
        for place in [Place::Earth, Place::Moon, Place::Sun, Place::Lagrange(Lagrange::L4)] {
            assert!(!place.reachable() && place.arrival(&world, 0.0, from).is_none());
            assert!(place.locate(&world, 0.0).unwrap().pos.length() > 50_000_000.0);
            assert!(!place.about(&world, false).is_empty());
        }
    }
}
