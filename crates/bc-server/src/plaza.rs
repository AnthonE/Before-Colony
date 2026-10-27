//! The colony's people on foot ("the plaza"), kept off the sector's tick like the hangar: the pose
//! each pilot in the city last sent, checked against the city before anyone else sees it, and who
//! each pilot sees (the people on their strip near them, nearest first).
//!
//! A pose is taken only if it could be: on the pilot's own strip, within the colony, not inside a
//! wall (`bc_sim::colony::city::solid`, the walls every client walks into), no further from the
//! last one taken than a running pilot (or a car) could go, and, the first, near where they came
//! in: the strip's Hub Gate, where the lift comes down (or the Blast Hall's gantry, climbing out of
//! a trainer); on a tram only from beside its open doors, and in a car (or a Markab) only from a
//! motor pool. Anything else isn't passed on. A Markab, the rocket car, goes where no car does (up
//! walls, along ceilings, through the air), so all that's asked of where it is is that its middle
//! is out of the walls. The homes' buildings are walked like the street (their walls, floors and
//! stairs are `solid`'s), and a lift is a ride from one of a building's lift doors to another
//! (`bc_sim::colony::homes`). Nobody crosses into a home its register (`crate::homes`) says they
//! may not go into: past its shut, locked door, however thin it is.
//!
//! Pilots flying suits inside the colony see the people too: those on the strip under the suit,
//! near it in the air ([`Plaza::fill_around`]).
//!
//! The base's deck in the hub (`docs/RING.md`, "The base") is one more floor ([`BASE_DECK`]): out of
//! a bay's airlock onto Bay Row, the first pose near that airlock, on foot only, out of the deck's
//! walls and its buildings' (`bc_sim::colony::deck::solid`). Its people see only each other.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use bc_proto::presence::{BASE_DECK, MAX_PEOPLE, PersonPose, PlazaWriter, RIDER_S};
use bc_sim::colony::city::{MAX_HEIGHT, Stage, arrival_seats, place_door, solid};
use bc_sim::colony::deck::{self, BASE, DeckPos};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, STRIPS, Under, from_colony, within_caps};
use bc_sim::colony::homes;
use bc_sim::colony::pools::pool_near;
use bc_sim::colony::transit::{self, CAR_WIDTH, TRAIN_LENGTH, TRAINS};
use bc_sim::content::city::{PLACES, PlaceKind};
use glam::Vec3;

/// Faster than anyone runs (the walker's run is 7 m/s), with room for a late packet on top, m/s.
const MAX_SPEED: f32 = 9.0 * 1.5;
/// Faster than anything drives (a car's top speed is 30 m/s), likewise.
const MAX_DRIVE: f32 = 35.0 * 1.3;
/// Faster than a Markab goes (`bc_client_core::markab::TOP_SPEED`, 62 m/s), likewise.
const MAX_MARKAB: f32 = 62.0 * 1.3;
const SLACK: f32 = 2.0;
/// Reach a pilot carries from one pose to the next, s of the fastest they could go, at most. The
/// time since their last pose earns reach (all of it: someone not heard from a minute may be far
/// off), and a move spends it; what's left is carried, up to this. Walking spends less than it
/// earns, so the bank stays full, and covers what a slow page does: one pose repeated between
/// frames, then a frame's walk at once; or a frame's walk sent only once its (seconds-long) render
/// is done, and the next frame's a moment later. Standing still a minute carries no more, and a
/// teleport is refused.
const BANK: f32 = 2.0;
/// A pilot's first pose must be this near where they came in, m: their strip's Hub Gate (everyone
/// comes down its lift), or the Blast Hall's gantry.
const ARRIVAL: f32 = 150.0;
/// Someone not heard from this long isn't shown, s.
const HIDE: f32 = 5.0;
/// How far a pilot sees others, m.
const NEAR: f32 = 1_500.0;
/// How far a suit inside the colony sees people, m (further off, a figure is less than a pixel).
const SUIT_NEAR: f32 = 1_500.0;
/// Getting on or off a tram: how near its body the pilot must be (m), and how far either side of
/// the sector's tick its doors may have been open (ticks: the pilot's clock is their own).
const DOORSTEP: f32 = 8.0;
const DOOR_SLACK: u32 = 90;

/// What became of a pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Taken,
    /// Older than one already taken (reordered).
    Stale,
    /// It couldn't be: not passed on.
    Implausible,
    /// Not in the city.
    Away,
}

struct Person {
    name: String,
    /// Who they are on the register of homes (their trader's key; empty till the session says).
    key: String,
    strip: u8,
    /// Where they came into the city: `(s, x)`.
    arrival: (f32, f32),
    pose: Option<PersonPose>,
    seq: u16,
    heard: Instant,
    /// Reach carried from their last pose, s (at most [`BANK`]).
    bank: f32,
    refused: u32,
    /// When a refusal of theirs was last logged.
    said: Option<Instant>,
}

/// Everyone in the city, by client slot, how many poses were refused, by why; and the register of
/// homes it asks who may go into one.
#[derive(Default)]
pub struct Plaza {
    people: Mutex<HashMap<u16, Person>>,
    refused_by: Mutex<[u32; WHYS.len()]>,
    homes: Option<std::sync::Arc<crate::homes::HomeRegister>>,
}

/// Why a pose wasn't passed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// Not the pilot's strip.
    Strip,
    /// Inside a wall, outside the colony, or (riding) outside the train's cars.
    Wall,
    /// Further from the last one than they could have gone.
    Reach,
    /// A first pose away from Hub Gate.
    Arrival,
    /// On a train without having stood by its open doors.
    Board,
    /// Off a train away from its open doors.
    Alight,
    /// In a vehicle not from a motor pool.
    Pool,
    /// From a train into a vehicle, or one train to another.
    Mode,
    /// Sitting where there's no seat, or moving while seated.
    Seat,
    /// Into a home they may not go into (`crate::homes`).
    Locked,
}

const WHYS: [&str; 10] =
    ["strip", "wall", "reach", "arrival", "board", "alight", "pool", "mode", "seat", "locked"];

/// Where strip `strip`'s Hub Gate lets people out: `(s, x)`.
fn hub_gate(strip: u8) -> (f32, f32) {
    PLACES
        .iter()
        .find(|p| p.kind == PlaceKind::HubGate && p.strip == strip)
        .map_or((STRIP_WIDTH * 0.5, -15_900.0), |p| place_door(p).0)
}

/// How someone gets about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Foot,
    Train(u8),
    Drive(u8),
    Seated,
}

/// The reach `me` has by `now`, s: what they carried from their last pose, and the time since.
fn banked(me: &Person, now: Instant) -> f32 {
    me.bank + now.saturating_duration_since(me.heard).as_secs_f32()
}

fn mode(p: &PersonPose) -> Mode {
    if p.seated() {
        return Mode::Seated;
    }
    match p.riding() {
        Some(k) => Mode::Train(k),
        None if p.driving() => Mode::Drive(p.ride),
        None => Mode::Foot,
    }
}

/// Where someone is on their strip, `(s, x)`, at tick `tick` (a rider: where their train is).
fn place(p: &PersonPose, tick: u32) -> (f32, f32) {
    match p.riding() {
        Some(k) => {
            let t = transit::train(p.strip, k, tick, 0.0);
            (t.s + p.s - RIDER_S, t.x + p.x)
        }
        None => (p.s, p.x),
    }
}

/// Whether train `k` of strip `strip`'s line stood with its doors open near `(s, x)` around
/// `tick`.
fn at_open_doors(strip: u8, k: u8, s: f32, x: f32, tick: u32) -> bool {
    (tick.saturating_sub(DOOR_SLACK)..=tick + DOOR_SLACK).step_by(10).any(|t| {
        let tr = transit::train(strip, k, t, 0.0);
        let dx = ((x - tr.x).abs() - 0.5 * TRAIN_LENGTH).max(0.0);
        let ds = ((s - tr.s).abs() - 0.5 * CAR_WIDTH).max(0.0);
        tr.doors && dx.hypot(ds) <= DOORSTEP
    })
}

/// The land strip a point of the colony's own frame is over (over a window, the nearer of the
/// strips either side of it), and where on it: `(strip, s, x)`.
pub fn strip_under(at: Vec3) -> (u8, f32, f32) {
    match from_colony(at) {
        Under::Land(c) => (c.strip, c.s, c.x),
        // Window `k` lies between strip `k − 1`'s far edge and strip `k`'s near one.
        Under::Window { k, s, .. } if s < STRIP_WIDTH * 0.5 => {
            ((k + STRIPS as u8 - 1) % STRIPS as u8, STRIP_WIDTH, at.x)
        }
        Under::Window { k, .. } => (k, 0.0, at.x),
    }
}

/// Whether a seated pose is on a seat (`bc_sim::colony::city::arrival_seats`).
fn on_seat(p: &PersonPose) -> bool {
    arrival_seats().iter().any(|t| t.strip == p.strip && (t.s - p.s).hypot(t.x - p.x) <= SEATED_SLACK)
}

/// How far a seated pose may be from its seat's spot (rounding on the wire), m.
const SEATED_SLACK: f32 = 0.3;

/// Whether a rider's place in their train could be: inside its cars.
fn aboard(p: &PersonPose) -> bool {
    p.x.abs() <= 0.5 * TRAIN_LENGTH + 0.5
        && (p.s - RIDER_S).abs() <= 0.5 * CAR_WIDTH + 0.3
        && (-0.5..=3.0).contains(&p.h)
        && p.riding().is_some_and(|k| u32::from(k) < TRAINS)
}

/// Whether a pose on the base's deck could be: on foot (nothing is ridden or driven there), a thin
/// core of its body out of the deck's walls and its buildings'.
fn on_the_base(p: &PersonPose) -> bool {
    let e = 0.05;
    mode(p) == Mode::Foot
        && !p.markab()
        && !deck::solid(
            BASE,
            Vec3::new(p.x - e, p.h + 0.4, -p.s - e),
            Vec3::new(p.x + e, p.h + 1.4, -p.s + e),
        )
}

/// Whether a pose could be anyone's: inside the colony, its body out of the walls (a thin core of
/// it: a step's worth of the walker's box may brush one; a Markab's middle, which keeps 0.6 m off
/// any face it's on, whichever way up); on the base's deck, [`on_the_base`].
fn possible(p: &PersonPose) -> bool {
    if p.strip == BASE_DECK {
        return on_the_base(p);
    }
    if p.riding().is_some() {
        return aboard(p);
    }
    let inside =
        within_caps(p.x) && (0.0..=STRIP_WIDTH).contains(&p.s) && (-4.0..=MAX_HEIGHT + 4.0).contains(&p.h);
    let (lo, hi) = if p.markab() { (-0.1, 0.1) } else { (0.4, 1.4) };
    let e = if p.markab() { 0.1 } else { 0.05 };
    inside
        && !solid(
            p.strip,
            Vec3::new(p.x - e, p.h + lo, -p.s - e),
            Vec3::new(p.x + e, p.h + hi, -p.s + e),
            Stage(0),
        )
}

/// Whether a pilot on foot rode a lift from `last` to `now`: both at the lift doors of the same
/// building, on different storeys.
fn lifted(last: &PersonPose, now: &PersonPose) -> bool {
    if last.strip == BASE_DECK || now.strip == BASE_DECK {
        return false;
    }
    let at = |p: &PersonPose| {
        let plan = homes::plan_at(p.strip, p.s, p.x)?;
        Some(((plan.bx, plan.row, plan.lot), plan.at_lift(p.s, p.x, p.h)?))
    };
    matches!((at(last), at(now)), (Some((a, ka)), Some((b, kb))) if a == b && ka != kb)
}

/// The fastest anyone can go getting about as a pose does, m/s: on foot (seated, on a train), in a
/// car or on a scooter, or in a Markab.
fn top(p: &PersonPose) -> f32 {
    match mode(p) {
        Mode::Drive(_) if p.markab() => MAX_MARKAB,
        Mode::Drive(_) => MAX_DRIVE,
        _ => MAX_SPEED,
    }
}

impl Plaza {
    /// A plaza that asks `homes` who may go into a home.
    pub fn with_homes(homes: std::sync::Arc<crate::homes::HomeRegister>) -> Self {
        Self { homes: Some(homes), ..Self::default() }
    }

    /// `id` is `key` on the register of homes (their trader's key).
    pub fn key(&self, id: u16, key: &str) {
        if let Ok(mut all) = self.people.lock()
            && let Some(p) = all.get_mut(&id)
        {
            p.key = key.to_string();
        }
    }

    /// Whether `me` may cross from `last` into where `now` is: into a home only if it's the one
    /// they were in or the register lets them in (its plaza's lock is held: the register's taken
    /// after it, never the other way round).
    fn may_cross(&self, me: &Person, last: &PersonPose, now: &PersonPose) -> bool {
        let Some(reg) = &self.homes else { return true };
        if mode(last) != Mode::Foot || mode(now) != Mode::Foot || now.strip == BASE_DECK {
            return true;
        }
        let home = |p: &PersonPose| homes::home_at(p.strip, p.s, p.x, p.h + 1.0);
        match home(now) {
            Some(to) => home(last) == Some(to) || reg.may_enter(to, &me.key),
            None => true,
        }
    }

    /// `id` came down to strip `strip`'s Hub Gate.
    pub fn enter(&self, id: u16, name: &str, strip: u8) {
        let strip = strip % STRIPS as u8;
        self.enter_at(id, name, strip, hub_gate(strip));
    }

    /// `id` came out of bay `bay`'s airlock onto Bay Row, the base's deck.
    pub fn enter_base(&self, id: u16, name: &str, bay: u8) {
        let out = deck::out_of_the_airlock(bay);
        self.enter_at(id, name, BASE_DECK, (out.s, out.x));
    }

    /// `id` came into strip `strip`'s city at `(s, x)` (out of a trainer, at the Blast Hall's
    /// gantry), or onto the base's deck ([`BASE_DECK`]): their first pose must be near there.
    pub fn enter_at(&self, id: u16, name: &str, strip: u8, arrival: (f32, f32)) {
        if let Ok(mut all) = self.people.lock() {
            let strip = strip.min(BASE_DECK);
            all.insert(
                id,
                Person {
                    name: name.into(),
                    key: String::new(),
                    strip,
                    arrival,
                    pose: None,
                    seq: 0,
                    heard: Instant::now(),
                    bank: 0.0,
                    refused: 0,
                    said: None,
                },
            );
        }
    }

    /// `id` left the city (up the lift, or for good).
    pub fn leave(&self, id: u16) {
        if let Ok(mut all) = self.people.lock() {
            all.remove(&id);
        }
    }

    /// A pose `id` sent, heard at `now`, when the sector was at tick `tick`.
    pub fn accept(&self, id: u16, seq: u16, pose: PersonPose, now: Instant, tick: u32) -> Verdict {
        let Ok(mut all) = self.people.lock() else { return Verdict::Away };
        let Some(me) = all.get_mut(&id) else { return Verdict::Away };
        let why = match me.pose {
            _ if pose.strip != me.strip => Some(Why::Strip),
            _ if !possible(&pose) => Some(Why::Wall),
            Some(_) if (seq.wrapping_sub(me.seq) as i16) <= 0 => return Verdict::Stale,
            Some(last) => {
                let dt = banked(me, now);
                let moved = |speed: f32| {
                    let reach = speed * dt + SLACK;
                    (pose.x - last.x).hypot(pose.s - last.s) <= reach && (pose.h - last.h).abs() <= reach
                };
                let check = |ok: bool, why: Why| (!ok).then_some(why);
                match (mode(&last), mode(&pose)) {
                    // Walking, on the ground or in a car of a train; or a lift ridden.
                    (Mode::Foot, Mode::Foot) => check(moved(MAX_SPEED) || lifted(&last, &pose), Why::Reach),
                    (Mode::Train(a), Mode::Train(b)) => check(a == b && moved(MAX_SPEED), Why::Reach),
                    // Getting on: from beside its open doors.
                    (Mode::Foot, Mode::Train(k)) => {
                        check(at_open_doors(pose.strip, k, last.s, last.x, tick), Why::Board)
                    }
                    // Getting off: out of its open doors.
                    (Mode::Train(k), Mode::Foot) => {
                        check(at_open_doors(pose.strip, k, pose.s, pose.x, tick), Why::Alight)
                    }
                    // Driving: no faster than a car (or a Markab) goes; taken from a motor pool; left
                    // anywhere.
                    (Mode::Drive(a), Mode::Drive(b)) => check(a == b && moved(top(&last)), Why::Reach),
                    (Mode::Foot, Mode::Drive(_)) => {
                        check(pool_near(pose.strip, last.s, last.x).is_some() && moved(MAX_SPEED), Why::Pool)
                    }
                    (Mode::Drive(_), Mode::Foot) => check(moved(top(&last)), Why::Reach),
                    // Sitting down on a seat from beside it, staying put on it, and getting up.
                    (Mode::Foot, Mode::Seated) => check(on_seat(&pose) && moved(MAX_SPEED), Why::Seat),
                    (Mode::Seated, Mode::Seated) => check(
                        on_seat(&pose) && (pose.x - last.x).hypot(pose.s - last.s) <= SEATED_SLACK,
                        Why::Seat,
                    ),
                    (Mode::Seated, Mode::Foot) => check(moved(MAX_SPEED), Why::Reach),
                    // From a train to a car, or one train to another.
                    _ => Some(Why::Mode),
                }
            }
            None => {
                let (s, x) = me.arrival;
                let ok = pose.riding().is_none() && (pose.x - x).hypot(pose.s - s) <= ARRIVAL;
                (!ok).then_some(Why::Arrival)
            }
        };
        let why = why.or_else(|| {
            let crossed = me.pose.is_some_and(|last| !self.may_cross(me, &last, &pose));
            crossed.then_some(Why::Locked)
        });
        if let Some(why) = why {
            // Said now and then (never where: only why, how far and how long since the last).
            if me.said.is_none_or(|t| now.saturating_duration_since(t).as_secs_f32() > 5.0) {
                me.said = Some(now);
                let dt = banked(me, now);
                let moved = me.pose.map_or(0.0, |l| (pose.x - l.x).hypot(pose.s - l.s));
                tracing::info!(
                    slot = id,
                    ?why,
                    moved = moved as u32,
                    dt,
                    refused = me.refused + 1,
                    "a pose not passed on"
                );
            }
            me.refused += 1;
            if let Ok(mut by) = self.refused_by.lock() {
                by[why as usize] += 1;
            }
            return Verdict::Implausible;
        }
        // What the move spent of the bank (where it measures one: on foot, seated, driving, or in
        // one train's car; boarding and alighting are checked at the doors instead).
        me.bank = match me.pose {
            Some(last) => {
                let speed = top(&last).max(top(&pose));
                let same = last.riding() == pose.riding();
                let d = if same { (pose.x - last.x).hypot(pose.s - last.s) } else { 0.0 };
                (banked(me, now) - d / speed).clamp(0.0, BANK)
            }
            None => 0.0,
        };
        me.pose = Some(pose);
        me.seq = seq;
        me.heard = now;
        Verdict::Taken
    }

    /// The people `viewer` sees, into their plaza datagram: on their strip, heard from lately and
    /// near them, nearest first. Their slots go into `shown`.
    pub fn fill(&self, viewer: u16, now: Instant, tick: u32, w: &mut PlazaWriter<'_>, shown: &mut Vec<u16>) {
        shown.clear();
        let Ok(all) = self.people.lock() else { return };
        let Some(me) = all.get(&viewer) else { return };
        let (s, x) = me.pose.map_or(me.arrival, |p| place(&p, tick));
        let d = |ps: f32, px: f32, _: f32| (px - x).hypot(ps - s);
        nearest(&all, Some(viewer), me.strip, now, tick, NEAR, d, w, shown);
    }

    /// The people a pilot flying a suit inside the colony sees from `at` (the colony's own frame),
    /// into their plaza datagram, whose strip is the one under the suit ([`strip_under`]): those on
    /// it within [`SUIT_NEAR`] of the suit, up in the air as it is, nearest first.
    pub fn fill_around(
        &self,
        at: Vec3,
        now: Instant,
        tick: u32,
        w: &mut PlazaWriter<'_>,
        shown: &mut Vec<u16>,
    ) {
        shown.clear();
        let Ok(all) = self.people.lock() else { return };
        let (strip, _, _) = strip_under(at);
        let d = |ps: f32, px: f32, h: f32| CityPos::new(strip, px, ps, h).to_colony().distance(at);
        nearest(&all, None, strip, now, tick, SUIT_NEAR, d, w, shown);
    }

    /// Where `id` was last seen to be (their last pose taken), if they're in the city.
    pub fn pose_of(&self, id: u16) -> Option<PersonPose> {
        self.people.lock().ok()?.get(&id)?.pose
    }

    /// Where `id` is in the colony's own frame at tick `tick` (a rider: where their train has
    /// them); before their first pose, where they came in. `None` if they're not on foot in the
    /// city or on the base's deck.
    pub fn where_is(&self, id: u16, tick: u32) -> Option<Vec3> {
        let all = self.people.lock().ok()?;
        let me = all.get(&id)?;
        let (s, x, h) = match me.pose {
            Some(p) => {
                let (s, x) = place(&p, tick);
                (s, x, p.h.max(0.0))
            }
            None => (me.arrival.0, me.arrival.1, 0.0),
        };
        if me.strip == BASE_DECK {
            return Some(DeckPos::new(BASE, x, s, h).to_colony());
        }
        Some(CityPos::new(me.strip, x, s, h).to_colony())
    }

    /// `id`'s name, as they came down.
    pub fn name(&self, id: u16) -> Option<String> {
        self.people.lock().ok()?.get(&id).map(|p| p.name.clone())
    }

    /// For `/status`: how many poses have been refused, by why.
    pub fn refused_by(&self) -> Vec<(&'static str, u32)> {
        let by = self.refused_by.lock().map(|b| *b).unwrap_or_default();
        WHYS.iter().zip(by).map(|(w, n)| (*w, n)).collect()
    }

    /// For `/status`: how many are in the city, by strip, how many poses have been refused, and how
    /// many are out on the base's deck (no positions).
    pub fn counts(&self) -> (usize, [usize; 3], u32, usize) {
        let Ok(all) = self.people.lock() else { return (0, [0; 3], 0, 0) };
        let mut by_strip = [0; 3];
        let mut base = 0;
        for p in all.values() {
            match by_strip.get_mut(p.strip as usize) {
                Some(n) => *n += 1,
                None => base += 1,
            }
        }
        (all.len() - base, by_strip, all.values().map(|p| p.refused).sum(), base)
    }
}

/// Into `w`, the people on strip `strip` heard from lately (but `skip`) within `range` by `dist`
/// (of where they are across and along it, and how high), nearest first; their slots into `shown`.
#[allow(clippy::too_many_arguments)]
fn nearest(
    all: &HashMap<u16, Person>,
    skip: Option<u16>,
    strip: u8,
    now: Instant,
    tick: u32,
    range: f32,
    dist: impl Fn(f32, f32, f32) -> f32,
    w: &mut PlazaWriter<'_>,
    shown: &mut Vec<u16>,
) {
    let mut near: Vec<(f32, u16, f32, PersonPose)> = all
        .iter()
        .filter(|(id, p)| Some(**id) != skip && p.strip == strip)
        .filter_map(|(id, p)| {
            let pose = p.pose?;
            let age = now.saturating_duration_since(p.heard).as_secs_f32();
            let (ps, px) = place(&pose, tick);
            let d = dist(ps, px, pose.h.max(0.0));
            (age < HIDE && d < range).then_some((d, *id, age, pose))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, id, age, pose) in near.into_iter().take(MAX_PEOPLE) {
        if !w.push(id, age, &pose) {
            break;
        }
        shown.push(id);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use bc_proto::MAX_DATAGRAM;
    use bc_proto::presence::PlazaReader;

    fn at(strip: u8, s: f32, x: f32) -> PersonPose {
        PersonPose { strip, x, s, grounded: true, ..PersonPose::default() }
    }

    fn seen(plaza: &Plaza, viewer: u16, now: Instant) -> Vec<(u16, PersonPose)> {
        seen_at(plaza, viewer, now, 0)
    }

    fn seen_at(plaza: &Plaza, viewer: u16, now: Instant, tick: u32) -> Vec<(u16, PersonPose)> {
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut w = PlazaWriter::new(&mut buf, 1, Some(0));
        let mut shown = Vec::new();
        plaza.fill(viewer, now, tick, &mut w, &mut shown);
        let n = w.finish();
        let mut r = PlazaReader::new(&buf[..n]).unwrap();
        let mut out = Vec::new();
        while let Some((id, _, p)) = r.next_person() {
            out.push((id, p));
        }
        assert_eq!(out.iter().map(|(id, _)| *id).collect::<Vec<_>>(), shown);
        out
    }

    #[test]
    fn a_pilot_walks_into_a_block_of_flats_climbs_its_stairs_and_rides_its_lift() {
        use bc_client_core::city::{CityGround, pose_of};
        use bc_client_core::walker::{Guide, Walker};
        use bc_sim::colony::city::{CITY, KERB, ROWS, block, lots};
        let plan = (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| {
                lots(&b).as_slice().iter().find_map(|bd| {
                    homes::plan(&b, bd)
                        .filter(|p| p.kind == homes::Kind::Flats && p.storeys >= 4 && p.sides != 0)
                })
            })
            .expect("a block of flats");
        let plaza = Plaza::default();
        let door = plan.point(0.0, -homes::SHELL - 1.2);
        plaza.enter_at(1, "Kai", 0, door);
        // In off the street, onto the landing, and up two storeys.
        let mut way = vec![(plan.point(0.0, 0.8), KERB), (plan.point(0.0, homes::NEAR * 0.5), KERB)];
        way.extend(plan.stairs_up(0, 0));
        way.extend(plan.stairs_up(1, 0));
        let ground = CityGround { strip: 0, stage: Stage(0) };
        let mut w = Walker::at(CityPos::new(0, door.1, door.0, KERB + 0.05).walker(), Vec3::X);
        let route = way.iter().map(|&((s, x), h)| CityPos::new(0, x, s, h).walker()).collect();
        let mut g = Guide::new(route, None);
        let t0 = Instant::now();
        let (dt, mut seq, mut frames) = (1.0 / 15.0, 0u16, 0u32);
        while !g.arrived() {
            for _ in 0..4 {
                let stride = g.steer(&mut w, dt / 4.0);
                w.step(&ground, &stride, dt / 4.0);
            }
            seq += 1;
            frames += 1;
            let now = t0 + Duration::from_secs_f32(frames as f32 * dt);
            let pose = pose_of(0, &w);
            assert_eq!(plaza.accept(1, seq, pose, now, 0), Verdict::Taken, "{pose:?}");
            assert!(frames < 15 * 120, "stuck at {pose:?}");
        }
        let up = CityPos::from_walker(0, w.feet);
        assert!((up.h - homes::level(2)).abs() < 0.05, "{up:?}");
        // To the lift's doors, and down in it to the ground floor: a moment, and taken.
        let lift = |k: u32| {
            let ((s, x), h) = plan.lift_spot(k).unwrap();
            PersonPose { strip: 0, s, x, h, grounded: true, ..PersonPose::default() }
        };
        let mut now = t0 + Duration::from_secs_f32(frames as f32 * dt + 2.0);
        for k in [2, 0] {
            seq += 1;
            assert_eq!(plaza.accept(1, seq, lift(k), now, 0), Verdict::Taken, "at the lift on {k}");
            now += Duration::from_millis(500);
        }
        // But never into the lift's own walls.
        seq += 1;
        let (s, x) = plan.point(-homes::SPINE * 0.5 - 1.5, 2.0);
        let in_wall = PersonPose { s, x, ..lift(0) };
        assert_eq!(plaza.accept(1, seq, in_wall, now, 0), Verdict::Implausible, "in the lift's block");
    }

    #[test]
    fn nobody_crosses_into_a_home_its_register_keeps_them_out_of() {
        use bc_econ::Hangar;
        use bc_sim::colony::city::{CITY, ROWS, block, lots};
        let plan = (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| {
                lots(&b)
                    .as_slice()
                    .iter()
                    .find_map(|bd| homes::plan(&b, bd).filter(|p| p.kind == homes::Kind::Flats))
            })
            .expect("a block of flats");
        let flat = plan.home(0, 0).unwrap();
        let register = std::sync::Arc::new(crate::homes::HomeRegister::open(None));
        let mut ada = Hangar { credits: 100_000, ..Hangar::starter() };
        register.with(|h| h.rent(&flat, "0xada", "Ada", 1, &mut ada, 0)).unwrap();
        let plaza = Plaza::with_homes(register.clone());
        let pose = |(s, x): (f32, f32)| PersonPose {
            strip: 0,
            s,
            x,
            h: flat.floor,
            grounded: true,
            ..PersonPose::default()
        };
        let t0 = Instant::now();
        // Each pilot at the door outside, then a step in: Ada's is taken, Bo's isn't, however close.
        for (id, key, name) in [(1, "0xada", "Ada"), (2, "0xb0", "Bo")] {
            plaza.enter_at(id, name, 0, flat.outside);
            plaza.key(id, key);
            assert_eq!(plaza.accept(id, 1, pose(flat.outside), t0, 0), Verdict::Taken);
        }
        let step_in = t0 + Duration::from_millis(400);
        assert_eq!(plaza.accept(1, 2, pose(flat.inside), step_in, 0), Verdict::Taken, "its holder");
        assert_eq!(plaza.accept(2, 2, pose(flat.inside), step_in, 0), Verdict::Implausible, "a stranger");
        // The door opened, anyone may.
        register.with(|h| h.door(flat.id, "0xada", true)).unwrap();
        assert_eq!(
            plaza.accept(2, 3, pose(flat.inside), step_in + Duration::from_millis(200), 0),
            Verdict::Taken
        );
        // Shut behind them, they can still leave.
        register.with(|h| h.door(flat.id, "0xada", false)).unwrap();
        assert_eq!(
            plaza.accept(2, 4, pose(flat.outside), step_in + Duration::from_millis(600), 0),
            Verdict::Taken
        );
        assert_eq!(plaza.refused_by().into_iter().find(|(why, _)| *why == "locked").map(|(_, n)| n), Some(1));
    }

    #[test]
    fn people_come_down_at_hub_gate_walk_and_see_each_other() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Kai", 0);
        plaza.enter(2, "Nico", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        assert_eq!(plaza.accept(2, 1, at(0, s + 3.0, x + 2.0), t0, 0), Verdict::Taken);
        let me = seen(&plaza, 1, t0);
        assert_eq!(me.len(), 1);
        assert_eq!(me[0].0, 2);
        // A second's walk is fine; a reordered packet is dropped.
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(plaza.accept(2, 3, at(0, s + 3.0, x + 9.0), t1, 0), Verdict::Taken);
        assert_eq!(plaza.accept(2, 2, at(0, s + 3.0, x + 5.0), t1, 0), Verdict::Stale);
        assert!((seen(&plaza, 1, t1)[0].1.x - (x + 9.0)).abs() < 0.01);
        assert_eq!(plaza.counts().0, 2);
        plaza.leave(2);
        assert!(seen(&plaza, 1, t1).is_empty());
    }

    #[test]
    fn out_of_a_trainer_a_pilot_comes_in_at_the_blast_halls_gantry() {
        // Climbing out of a trainer docked on the gantry: their first pose is at its hatch, in the
        // hall, not at Hub Gate; one far from there isn't taken.
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let ((s, x), _) = bc_sim::colony::hall::hatch();
        let strip = bc_sim::colony::hall::hall().strip;
        plaza.enter_at(1, "Lin", strip, (s, x));
        let at_hatch = bc_sim::colony::frame::CityPos::new(strip, x, s, 0.0).to_colony();
        assert!(plaza.where_is(1, 0).is_some_and(|p| p.distance(at_hatch) < 0.01));
        assert_eq!(plaza.accept(1, 1, at(strip, s, x + 400.0), t0, 0), Verdict::Implausible);
        assert_eq!(plaza.accept(1, 2, at(strip, s, x + 1.0), t0, 0), Verdict::Taken);
    }

    #[test]
    fn a_slow_client_repeating_its_pose_between_frames_isnt_refused() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Hilde", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x), t0, 0), Verdict::Taken);
        // At under 1 fps: the same pose at 15 Hz for 1.5 s, then the next frame's 7 m run at once.
        let mut seq = 1;
        for i in 1..=22 {
            seq += 1;
            let t = t0 + Duration::from_millis(i * 67);
            assert_eq!(plaza.accept(1, seq, at(0, s, x), t, 0), Verdict::Taken);
        }
        let t1 = t0 + Duration::from_millis(1_550);
        assert_eq!(plaza.accept(1, seq + 1, at(0, s, x + 7.0), t1, 0), Verdict::Taken);
        // Standing still a minute doesn't earn a long jump.
        for i in 1..=900 {
            seq += 1;
            let t = t1 + Duration::from_millis(i * 67);
            assert_eq!(plaza.accept(1, seq + 1, at(0, s, x + 7.0), t, 0), Verdict::Taken);
        }
        let t2 = t1 + Duration::from_millis(900 * 67 + 67);
        assert_eq!(plaza.accept(1, seq + 2, at(0, s, x + 207.0), t2, 0), Verdict::Implausible);
    }

    #[test]
    fn a_page_that_sends_its_walk_late_and_then_at_once_isnt_refused() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Sally", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x), t0, 0), Verdict::Taken);
        // A second's walk each frame, its pose leaving when the frame's render is done: a long
        // render, then a short one, and the two poses arrive a tenth of a second apart.
        let mut seq = 1;
        let mut t = t0;
        let mut ahead = 0.0;
        for k in 0..20 {
            seq += 1;
            ahead += 6.0;
            t += Duration::from_millis(if k % 2 == 0 { 1_900 } else { 120 });
            assert_eq!(plaza.accept(1, seq, at(0, s, x + ahead), t, 0), Verdict::Taken, "frame {k}");
        }
        // But not faster than anyone goes, for long: 20 m a frame at 15 frames a second.
        let refused = (0..30).filter(|_| {
            seq += 1;
            ahead += 20.0;
            t += Duration::from_millis(67);
            plaza.accept(1, seq, at(0, s, x + ahead), t, 0) == Verdict::Implausible
        });
        assert!(refused.count() > 20);
    }

    #[test]
    fn a_teleport_a_wall_or_another_strip_isnt_passed_on() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Tomas", 0);
        // Not down the lift: far from Hub Gate.
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2_000.0), t0, 0), Verdict::Implausible);
        assert_eq!(plaza.accept(1, 2, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        // A kilometre in a tenth of a second.
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(plaza.accept(1, 3, at(0, s, x + 1_000.0), t1, 0), Verdict::Implausible);
        // Another strip.
        assert_eq!(plaza.accept(1, 4, at(1, s, x + 2.0), t1, 0), Verdict::Implausible);
        // Inside Hub Gate's terminal (the end cap's foot).
        let wall = at(0, STRIP_WIDTH * 0.5, -15_975.0);
        assert!(!possible(&wall));
        assert!(!possible(&at(0, s, -16_100.0)), "beyond the end cap");
        assert_eq!(plaza.counts().2, 3);
        // Someone never in the city.
        assert_eq!(plaza.accept(9, 1, at(0, s, x), t1, 0), Verdict::Away);
    }

    #[test]
    fn a_pose_in_a_lamp_post_isnt_possible() {
        use bc_sim::colony::city::block;
        use bc_sim::colony::furniture::{Kind, each_furniture};
        // A street lamp by Hub Gate's offices.
        let b = block(0, 4, 3, Stage(0)).unwrap();
        let mut post = None;
        each_furniture(0, &b.rect, Stage(0), |p| {
            post = Some(*p);
            p.kind == Kind::StreetLamp
        });
        let p = post.filter(|p| p.kind == Kind::StreetLamp).expect("a street lamp");
        assert!(!possible(&PersonPose { h: p.h, ..at(0, p.s, p.x) }), "in {p:?}");
        assert!(possible(&PersonPose { h: p.h, ..at(0, p.s, p.x + 1.0) }), "beside {p:?}");
    }

    #[test]
    fn pilots_ride_the_trams_from_platform_to_platform() {
        use bc_sim::colony::transit::{DOOR_AT, PERIOD_TICKS, PLATFORM_HALF, train};
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        let k = 2;
        // When train `k` stands at station `i` with its doors open.
        let open_at = |i: usize| {
            (0..PERIOD_TICKS)
                .find(|t| train(0, k, *t, 0.0).doors && train(0, k, *t, 0.0).at == Some(i))
                .unwrap()
        };
        let tick = open_at(0);
        let tr = train(0, k, tick, 0.0);
        let door = tr.car_x(0) + DOOR_AT;
        let island = STRIP_WIDTH * 0.5 + tr.dir * (PLATFORM_HALF - 0.3);
        plaza.enter(1, "Tomas", 0);
        plaza.enter(2, "Catherine", 0);
        for id in [1, 2] {
            assert_eq!(plaza.accept(id, 1, at(0, s, x + 2.0), t0, tick), Verdict::Taken);
            let platform = PersonPose { h: 1.0, ..at(0, island, door) };
            assert_eq!(plaza.accept(id, 2, platform, t0 + Duration::from_secs(60), tick), Verdict::Taken);
        }
        // In through the door.
        let rider = |lx: f32| PersonPose { x: lx, s: RIDER_S + 0.4, h: 0.0, ride: k + 1, ..at(0, 0.0, 0.0) };
        assert_eq!(
            plaza.accept(1, 3, rider(door - tr.x), t0 + Duration::from_secs(61), tick),
            Verdict::Taken
        );
        // Seen inside the train, wherever it is.
        let seen_by_2 = seen_at(&plaza, 2, t0 + Duration::from_secs(62), tick);
        assert_eq!(seen_by_2[0].1.riding(), Some(k));
        // Walking down the car as it runs.
        assert_eq!(
            plaza.accept(1, 4, rider(door - tr.x + 3.0), t0 + Duration::from_secs(63), tick + 600),
            Verdict::Taken
        );
        // Out at the next station, onto its platform.
        let next = open_at(1);
        let tr1 = train(0, k, next, 0.0);
        let out = PersonPose { h: 1.0, ..at(0, island, tr1.car_x(0) + DOOR_AT) };
        assert_eq!(plaza.accept(1, 5, out, t0 + Duration::from_secs(150), next), Verdict::Taken);
        // Nobody gets on a train that's running, or one far away.
        let running = (0..PERIOD_TICKS).find(|t| train(0, k, *t, 0.0).speed > 30.0).unwrap();
        assert_eq!(
            plaza.accept(2, 3, rider(0.0), t0 + Duration::from_secs(64), running),
            Verdict::Implausible
        );
        // Nor stands outside its cars while aboard.
        assert_eq!(plaza.accept(2, 4, rider(80.0), t0 + Duration::from_secs(65), tick), Verdict::Implausible);
    }

    #[test]
    fn pilots_sit_only_on_seats_and_stay_put_till_they_stand() {
        use bc_proto::presence::RIDE_SEATED;
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Rafi", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        let seat = arrival_seats()[1];
        let sit = |s: f32, x: f32| PersonPose { ride: RIDE_SEATED, yaw: seat.yaw, ..at(0, s, x) };
        // Not in the middle of the street.
        assert_eq!(
            plaza.accept(1, 2, sit(s, x + 3.0), t0 + Duration::from_millis(100), 0),
            Verdict::Implausible
        );
        // Walked over to The Arrival (a minute later), and sat down on its seat.
        let t1 = t0 + Duration::from_secs(120);
        assert_eq!(plaza.accept(1, 3, at(0, seat.s + 1.0, seat.x), t1, 0), Verdict::Taken);
        assert_eq!(
            plaza.accept(1, 4, sit(seat.s, seat.x), t1 + Duration::from_millis(500), 0),
            Verdict::Taken
        );
        assert_eq!(plaza.accept(1, 5, sit(seat.s, seat.x), t1 + Duration::from_secs(30), 0), Verdict::Taken);
        // No sliding along the bench, seated.
        assert_eq!(
            plaza.accept(1, 6, sit(seat.s, seat.x + 2.0), t1 + Duration::from_secs(31), 0),
            Verdict::Implausible
        );
        // Up again, and off.
        let t2 = t1 + Duration::from_secs(40);
        assert_eq!(plaza.accept(1, 7, at(0, seat.s + 1.0, seat.x), t2, 0), Verdict::Taken);
    }

    #[test]
    fn cars_come_from_the_motor_pools_and_keep_to_a_cars_speed() {
        use bc_proto::presence::RIDE_CAR;
        use bc_sim::colony::pools::pool;
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        let (ps, px) = pool(0, 0);
        plaza.enter(1, "Nico", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        // Not from the middle of the street.
        let car = |s: f32, x: f32| PersonPose { ride: RIDE_CAR, ..at(0, s, x) };
        assert_eq!(
            plaza.accept(1, 2, car(s, x + 3.0), t0 + Duration::from_millis(100), 0),
            Verdict::Implausible
        );
        // From the pool.
        let t1 = t0 + Duration::from_secs(10);
        assert_eq!(plaza.accept(1, 3, at(0, ps, px), t1, 0), Verdict::Taken);
        assert_eq!(plaza.accept(1, 4, car(ps, px + 0.5), t1 + Duration::from_millis(100), 0), Verdict::Taken);
        // 30 m/s for a second, fine; 300 m in one, not.
        let t2 = t1 + Duration::from_millis(1_100);
        assert_eq!(plaza.accept(1, 5, car(ps, px + 30.5), t2, 0), Verdict::Taken);
        assert_eq!(
            plaza.accept(1, 6, car(ps, px + 330.0), t2 + Duration::from_secs(1), 0),
            Verdict::Implausible
        );
        // Out beside it.
        assert_eq!(
            plaza.accept(1, 7, at(0, ps + 1.6, px + 30.5), t2 + Duration::from_secs(2), 0),
            Verdict::Taken
        );
    }

    #[test]
    fn a_markab_comes_from_a_motor_pool_and_goes_up_walls_and_through_the_air_fast() {
        use bc_proto::presence::RIDE_MARKAB;
        use bc_sim::colony::city::terminal_rect;
        use bc_sim::colony::hall::hall;
        use bc_sim::colony::pools::pool;
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        let (ps, px) = pool(0, 0);
        plaza.enter(1, "Lars", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        let markab = |s: f32, x: f32, h: f32| PersonPose { ride: RIDE_MARKAB, h, ..at(0, s, x) };
        // Not from the middle of the street.
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(plaza.accept(1, 2, markab(s, x + 3.0, 0.6), t1, 0), Verdict::Implausible);
        // From the pool.
        let t2 = t1 + Duration::from_secs(10);
        assert_eq!(plaza.accept(1, 3, at(0, ps, px), t2, 0), Verdict::Taken);
        let t3 = t2 + Duration::from_millis(100);
        assert_eq!(plaza.accept(1, 4, markab(ps, px + 0.5, 0.6), t3, 0), Verdict::Taken);
        // 60 m/s for a second (faster than any car), 20 m up; 300 m in one, not.
        let t4 = t3 + Duration::from_millis(1_000);
        assert_eq!(plaza.accept(1, 5, markab(ps, px + 60.5, 20.0), t4, 0), Verdict::Taken);
        assert_eq!(
            plaza.accept(1, 6, markab(ps, px + 360.0, 20.0), t4 + Duration::from_secs(1), 0),
            Verdict::Implausible
        );
        // Hanging from the Blast Hall's ceiling, its middle 0.6 m under it: a Markab's, never a
        // walker's; and never inside a wall.
        let r = hall();
        let (hs, hx) = r.front.point(12.0, 40.0);
        assert!(possible(&markab(hs, hx, r.ceiling - 0.6)));
        assert!(!possible(&PersonPose { h: r.ceiling - 0.6, ..at(0, hs, hx) }));
        let (ts, tx) = terminal_rect().middle();
        assert!(!possible(&markab(ts, tx, 5.0)));
    }

    /// What a suit at `at` (the colony's frame) is shown.
    fn seen_from_a_suit(plaza: &Plaza, at: Vec3, now: Instant) -> Vec<(u16, PersonPose)> {
        let mut buf = [0u8; MAX_DATAGRAM];
        let (strip, _, _) = strip_under(at);
        let mut w = PlazaWriter::new(&mut buf, 1, Some(strip));
        let mut shown = Vec::new();
        plaza.fill_around(at, now, 0, &mut w, &mut shown);
        let n = w.finish();
        let mut r = PlazaReader::new(&buf[..n]).unwrap();
        assert_eq!(r.strip, Some(strip));
        let mut out = Vec::new();
        while let Some((id, _, p)) = r.next_person() {
            out.push((id, p));
        }
        assert_eq!(out.iter().map(|(id, _)| *id).collect::<Vec<_>>(), shown);
        out
    }

    #[test]
    fn a_suit_inside_sees_the_people_on_the_strip_under_it_near_it() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Maren", 0);
        plaza.enter(2, "Pagan", 0);
        plaza.enter(3, "Major Ines", 1);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        assert_eq!(plaza.accept(2, 1, at(0, s + 20.0, x + 60.0), t0, 0), Verdict::Taken);
        let (s1, x1) = hub_gate(1);
        assert_eq!(plaza.accept(3, 1, at(1, s1, x1), t0, 0), Verdict::Taken);
        // A suit 300 m over Hub Gate's door: strip 0 under it, both of its people, the nearer
        // first, and nobody from strip 1.
        let over = CityPos::new(0, x + 2.0, s, 300.0).to_colony();
        assert_eq!(strip_under(over).0, 0);
        let seen = seen_from_a_suit(&plaza, over, t0);
        assert_eq!(seen.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2]);
        // Up by the axis, 3 km over them: too far to make anyone out.
        let high = CityPos::new(0, x + 2.0, s, 3_000.0).to_colony();
        assert!(seen_from_a_suit(&plaza, high, t0).is_empty());
        // Over strip 1: its own.
        let over1 = CityPos::new(1, x1, s1, 120.0).to_colony();
        assert_eq!(
            seen_from_a_suit(&plaza, over1, t0).iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![3]
        );
        // Over a window, the nearer strip's: window 1 lies between strip 0's far edge and strip 1.
        let near0 = CityPos::new(0, x, STRIP_WIDTH + 100.0, 50.0).to_colony();
        let near1 = CityPos::new(1, x, -100.0, 50.0).to_colony();
        assert!(matches!(from_colony(near0), Under::Window { k: 1, .. }), "{:?}", from_colony(near0));
        assert_eq!((strip_under(near0).0, strip_under(near1).0), (0, 1));
        assert_eq!(plaza.pose_of(2).map(|p| p.x), Some(x + 60.0));
        assert_eq!(plaza.pose_of(9), None);
    }

    #[test]
    fn only_the_strip_and_the_near_are_shown() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Rafi", 0);
        plaza.accept(1, 1, at(0, s, x), t0, 0);
        plaza.enter(2, "Lin", 1);
        let (s1, x1) = hub_gate(1);
        plaza.accept(2, 1, at(1, s1, x1), t0, 0);
        assert!(seen(&plaza, 1, t0).is_empty(), "another strip");
        // Quiet for longer than HIDE: not shown.
        plaza.enter(3, "Viktor", 0);
        plaza.accept(3, 1, at(0, s + 1.0, x), t0, 0);
        assert_eq!(seen(&plaza, 1, t0).len(), 1);
        assert!(seen(&plaza, 1, t0 + Duration::from_secs(6)).is_empty());
        assert_eq!(plaza.name(3).as_deref(), Some("Viktor"));
        assert_eq!(plaza.counts().1, [2, 1, 0]);
    }

    #[test]
    fn out_on_bay_row_pilots_walk_the_base_deck_and_see_only_each_other() {
        use bc_sim::content::base::SITES;
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let on_deck = |p: DeckPos| PersonPose { strip: BASE_DECK, ..at(0, p.s, p.x) };
        // Two pilots out of bays 1's and 2's airlocks; one down in the city.
        plaza.enter_base(1, "Maren", 1);
        plaza.enter_base(2, "Pagan", 2);
        plaza.enter(3, "Major Ines", 0);
        assert_eq!(plaza.accept(1, 1, on_deck(deck::out_of_the_airlock(1)), t0, 0), Verdict::Taken);
        // Out of bay 2's airlock, a first pose by bay 40's isn't where they came out.
        assert_eq!(plaza.accept(2, 1, on_deck(deck::out_of_the_airlock(40)), t0, 0), Verdict::Implausible);
        assert_eq!(plaza.accept(2, 2, on_deck(deck::out_of_the_airlock(2)), t0, 0), Verdict::Taken);
        let (s, x) = hub_gate(0);
        assert_eq!(plaza.accept(3, 1, at(0, s, x), t0, 0), Verdict::Taken);
        // A pose in the city from someone on the deck isn't theirs; nor is one riding a tram there.
        let t1 = t0 + Duration::from_millis(200);
        assert_eq!(plaza.accept(1, 2, at(0, s, x), t1, 0), Verdict::Implausible);
        let riding = PersonPose { ride: 1, ..on_deck(deck::out_of_the_airlock(1)) };
        assert_eq!(plaza.accept(1, 3, riding, t1, 0), Verdict::Implausible);
        // Across Bay Row to the test hall's front: taken; into one of its walls: not.
        let site = &SITES[0];
        let front = site.to_walker(Vec3::new(4.5, 0.0, -1.0));
        let t2 = t0 + Duration::from_secs(3);
        assert_eq!(plaza.accept(1, 4, on_deck(DeckPos::from_walker(BASE, front)), t2, 0), Verdict::Taken);
        let mut wall = None;
        site.each_solid(&mut |b| {
            if b.max.y - b.min.y > 2.0 && b.min.y < 0.5 {
                wall.get_or_insert((b.min + b.max) * 0.5);
            }
        });
        let wall = DeckPos::from_walker(BASE, wall.expect("a wall").with_y(0.0));
        let t3 = t2 + Duration::from_millis(800);
        assert_eq!(plaza.accept(1, 5, on_deck(wall), t3, 0), Verdict::Implausible, "in the hall's wall");
        // Maren sees Pagan (on the deck), not Major Ines (in the city).
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut w = PlazaWriter::new(&mut buf, 1, Some(BASE_DECK));
        let mut shown = Vec::new();
        plaza.fill(1, t3, 0, &mut w, &mut shown);
        assert_eq!(shown, vec![2]);
        // Where Maren is, in the colony's own frame, is on the deck by the hall.
        let here = plaza.where_is(1, 0).unwrap();
        let back = deck::deck_at(here).expect("over the deck");
        assert_eq!(back.deck, BASE);
        assert!((back.walker() - front).length() < 0.05, "{back:?}");
        let (city, by_strip, _, base) = plaza.counts();
        assert_eq!((city, by_strip, base), (1, [1, 0, 0], 2));
    }
}
