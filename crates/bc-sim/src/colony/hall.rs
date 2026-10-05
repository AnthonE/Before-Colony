//! The Blast Hall (`docs/TRAINING.md`), the Proving Ground's room off Hub Gate's square, and its
//! live fire: the colony's law's one exception. A suit in its room may fire; anywhere else inside
//! the colony nothing does. What it fires are training rounds: they never touch a suit, they stop
//! at the hall's walls and at its blast doors (nothing fired in it leaves it), and they score on the
//! hall's targets, holograms hung in its air, some still, some bobbing, some sweeping across it.
//!
//! All of it is closed forms of where a suit is, where a shot goes and the tick, as the rest of the
//! colony is: the server's interior sector and the owner's prediction ask the same questions and get
//! the same answers, and every client draws the targets where the tick has them.
//!
//! The hall has two more things for its pilots:
//! - **The gantry** ([`GANTRY`]): a pad on its floor in the firing line where the Charter Board's
//!   trainers stand. A pilot on foot boards one at its hatch ([`hatch`]) and flies it from there,
//!   and docks it back at rest over the pad ([`in_gantry`]).
//! - **The drill** ([`Drill`]): X-Wing's Maze in the hall. The targets light one at a time in a set
//!   order ([`DRILL`]); striking the first starts a clock, every lit target struck puts time back on
//!   it, and the drill is cleared when the last is struck before it runs out.

use core::f32::consts::TAU;

use glam::Vec3;

use super::city::{KERB, Room, room};
use super::frame::{CityPos, Under, from_colony};
use crate::config::TICK_HZ;
use crate::content::city::PROVING_GROUND;
use crate::math::{normalize_or, sin, sqrt};

/// The hall's targets.
pub const TARGETS: usize = 12;
/// A target's radius, m: a suit's shoulders across. Shots leave a suit's arm a few metres off the
/// line its pilot's crosshair is on (they fly parallel to it), so a crosshair on a target's middle
/// scores from the rifle's arm.
pub const TARGET_RADIUS: f32 = 4.0;

/// The hall's room (`content::city::PLACES`' Proving Ground has one, as its tests hold).
pub fn hall() -> Room {
    room(PROVING_GROUND).expect("the Blast Hall has a room")
}

/// Whether a point of the colony's own frame is in the hall's room, under its roof: where weapons
/// are free.
pub fn in_hall(p: Vec3) -> bool {
    let r = hall();
    match from_colony(p) {
        Under::Land(c) => c.strip == r.strip && r.holds(c.s, c.x, c.h),
        Under::Window { .. } => false,
    }
}

/// Whether a suit at `p` (the colony's own frame) may fire: in the hall's room. A sector in space
/// doesn't ask.
#[inline]
pub fn weapons_free(p: Vec3) -> bool {
    in_hall(p)
}

/// How target `i` moves: still on a stand by the back wall, bobbing over the floor's middle, or
/// sweeping across the hall high up. Its place in the room's own axes: `u` along the front from its
/// middle, `v` in from its face, `h` up, m; and its period, ticks (0: still).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Track {
    u: f32,
    v: f32,
    h: f32,
    /// Bobbing (up and down) or sweeping (across), this far each way, m.
    bob: f32,
    sweep: f32,
    period: u32,
    phase: f32,
}

const fn still(u: f32, v: f32, h: f32) -> Track {
    Track { u, v, h, bob: 0.0, sweep: 0.0, period: 0, phase: 0.0 }
}

const fn bob(u: f32, v: f32, h: f32, period: u32, phase: f32) -> Track {
    Track { u, v, h, bob: 6.0, sweep: 0.0, period, phase }
}

const fn sweep(v: f32, h: f32, period: u32, phase: f32) -> Track {
    Track { u: 0.0, v, h, bob: 0.0, sweep: 30.0, period, phase }
}

/// The targets, all in the hall's back half: its front 40 m, inside the blast doors, is the firing
/// line. Four still by the back wall (two low, two at a standing suit's head), four bobbing over
/// the floor, four sweeping across high up, each on its own beat. However they move, none comes
/// within three radii of another.
pub const FIRING_LINE: f32 = 40.0;
const TRACKS: [Track; TARGETS] = [
    still(-30.0, 78.0, 8.0),
    still(-10.0, 78.0, 20.0),
    still(10.0, 78.0, 8.0),
    still(30.0, 78.0, 20.0),
    bob(-24.0, 50.0, 22.0, 150, 0.0),
    bob(-8.0, 50.0, 30.0, 190, 0.3),
    bob(8.0, 50.0, 22.0, 170, 0.6),
    bob(24.0, 50.0, 30.0, 210, 0.9),
    sweep(63.0, 38.0, 240, 0.0),
    sweep(77.0, 38.0, 300, 0.25),
    sweep(63.0, 54.0, 360, 0.5),
    sweep(77.0, 54.0, 270, 0.75),
];

/// Where target `i` is at tick `t` plus `frac` of the next, in the colony's own frame.
pub fn target(i: usize, t: u32, frac: f32) -> Vec3 {
    let k = &TRACKS[i];
    let w = if k.period == 0 {
        0.0
    } else {
        let p = k.period;
        sin(TAU * (((t % p) as f32 + frac) / p as f32 + k.phase))
    };
    let r = hall();
    let (s, x) = r.front.point(k.u + k.sweep * w, k.v);
    CityPos::new(r.strip, x, s, k.h + k.bob * w).to_colony()
}

/// What stops a training round in the hall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Target `i`: it scores.
    Target(u8),
    /// The hall's walls, floor or roof, or the blast curtain across its doors.
    Wall,
}

/// The first thing a sphere of radius `r` moving from `a` to `b` (the colony's own frame) meets in
/// the hall at tick `t` plus `frac`: a target, or the hall's bounds (its room: walls, floor, roof,
/// and its open doors, which nothing fired inside crosses). How far along (0..1), and which. A
/// round that starts outside the room is stopped where it starts.
pub fn shot_end(a: Vec3, b: Vec3, r: f32, t: u32, frac: f32) -> Option<(f32, Stop)> {
    let mut first = wall_end(a, b, r).map(|f| (f, Stop::Wall));
    let d = b - a;
    let dd = d.dot(d);
    for i in 0..TARGETS {
        let c = target(i, t, frac);
        // The sphere |a + d s − c| = R, first along the move.
        let m = a - c;
        let rr = TARGET_RADIUS + r;
        let (bq, cq) = (m.dot(d), m.dot(m) - rr * rr);
        if cq > 0.0 && bq > 0.0 {
            continue;
        }
        let disc = bq * bq - dd * cq;
        if disc < 0.0 || dd <= 0.0 {
            continue;
        }
        let s = ((-bq - sqrt(disc)) / dd).max(0.0);
        if s <= 1.0 && first.is_none_or(|(f, _)| s < f) {
            first = Some((s, Stop::Target(i as u8)));
        }
    }
    first
}

/// The trainers' gantry: a pad on the hall's floor in its firing line, to one side of the way in
/// from the blast doors. Its middle, `u` along the front from its middle and `v` in from its face,
/// m (`city::Front::point`).
pub const GANTRY: (f32, f32) = (-26.0, 22.0);
/// How far from the pad's middle a trainer may be to dock, m: over the pad, no higher than
/// [`GANTRY_HEIGHT`] and no faster than [`GANTRY_SPEED`] (standing on it, or hovering low).
pub const GANTRY_RADIUS: f32 = 14.0;
pub const GANTRY_HEIGHT: f32 = 30.0;
pub const GANTRY_SPEED: f32 = 4.0;
/// Where a pilot on foot boards a trainer: this far from the pad's middle, toward the hall's, m.
const HATCH: f32 = 14.0;

/// The point `u` along the hall's front and `v` in from its face at height `h`, in the colony's own
/// frame.
fn in_room(u: f32, v: f32, h: f32) -> Vec3 {
    let r = hall();
    let (s, x) = r.front.point(u, v);
    CityPos::new(r.strip, x, s, h).to_colony()
}

/// The gantry's pad: its middle on the hall's floor, in the colony's own frame.
pub fn gantry() -> Vec3 {
    in_room(GANTRY.0, GANTRY.1, 0.0)
}

/// Which way a trainer on the gantry faces: in, toward the targets (the colony's own frame).
pub fn gantry_facing() -> Vec3 {
    normalize_or(in_room(GANTRY.0, GANTRY.1 + 1.0, 0.0) - gantry(), Vec3::X)
}

/// Whether a suit at `pos` moving at `vel` (the colony's own frame) is at rest over the gantry's
/// pad: a trainer there docks.
pub fn in_gantry(pos: Vec3, vel: Vec3) -> bool {
    let r = hall();
    let Under::Land(c) = from_colony(pos) else { return false };
    let (s, x) = r.front.point(GANTRY.0, GANTRY.1);
    let (ds, dx) = (c.s - s, c.x - x);
    c.strip == r.strip
        && ds * ds + dx * dx <= GANTRY_RADIUS * GANTRY_RADIUS
        && c.h <= GANTRY_HEIGHT
        && vel.length_squared() < GANTRY_SPEED * GANTRY_SPEED
}

/// The gantry's hatch, where a pilot on foot boards a trainer (and climbs out of one docked): its
/// spot on the hall's floor, `(s, x)`, and the way they face there, to the pad (a unit `(ds, dx)`).
pub fn hatch() -> ((f32, f32), (f32, f32)) {
    let f = hall().front;
    let at = f.point(GANTRY.0 + HATCH, GANTRY.1);
    let pad = f.point(GANTRY.0, GANTRY.1);
    let (ds, dx) = (pad.0 - at.0, pad.1 - at.1);
    let n = sqrt(ds * ds + dx * dx).max(1e-6);
    (at, (ds / n, dx / n))
}

/// The drill (X-Wing's Maze, in the hall): the targets lit one at a time, in this order. The first
/// starts the clock. The still ones by the back wall come first, then the bobbing, then the
/// sweeping, then a mix, the aim swinging from side to side and never at one target twice running.
pub const DRILL: [u8; 20] = [1, 2, 0, 3, 5, 6, 4, 7, 8, 9, 10, 11, 2, 5, 8, 3, 6, 9, 0, 11];
/// The clock starts with this long on it when the first is struck, s, and every lit target struck
/// after it puts this much back. Whole seconds: the clock runs out on a whole tick.
pub const DRILL_START_S: f64 = 12.0;
pub const DRILL_BONUS_S: f64 = 3.0;
/// The drill's par, s, from the first target struck to the last: a pilot finding each lit target,
/// putting the crosshair on it and scoring in a second and a quarter. (With a machine's perfect aim
/// it takes four seconds: the rest is the pilot's eye and hand.)
pub const DRILL_PAR_S: f64 = 25.0;

/// What a strike on a target, or a tick of the clock, did to a drill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrillEvent {
    /// The first lit target struck: the clock runs.
    Started,
    /// Another struck: how many so far (2 and on).
    Struck(usize),
    /// The last struck before the clock ran out: the time from the first, s.
    Cleared(f64),
    /// The clock ran out: how many had been struck.
    Out(usize),
}

/// A pilot's drill: which of [`DRILL`] is lit, and since when the clock has run. Fed the strikes of
/// the pilot's own rounds on the targets (their `TargetHit` events, by tick) and the clock, it's
/// kept alike by the server's interior sector, which checks the time for the board, and by the
/// pilot's client, for its HUD and to light the target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drill {
    /// The tick the first was struck on, while the clock runs.
    started: Option<u32>,
    /// How many of [`DRILL`] have been struck.
    struck: u8,
}

impl Drill {
    /// The target lit now: the next to strike (the first, before the clock runs).
    pub fn lit(&self) -> u8 {
        DRILL[usize::from(self.struck) % DRILL.len()]
    }

    pub fn running(&self) -> bool {
        self.started.is_some()
    }

    /// How many have been struck.
    pub fn struck(&self) -> usize {
        usize::from(self.struck)
    }

    /// The tick the clock runs out on, while it runs: the start, the time it started with, and what
    /// each target struck after the first put back.
    pub fn deadline(&self) -> Option<u32> {
        let s = self.started?;
        let secs = DRILL_START_S + DRILL_BONUS_S * f64::from(self.struck.saturating_sub(1));
        Some(s + (secs * f64::from(TICK_HZ) + 0.5) as u32)
    }

    /// What's left on the clock at tick `t`, s.
    pub fn left(&self, t: f64) -> Option<f64> {
        self.deadline().map(|d| ((f64::from(d) - t) / f64::from(TICK_HZ)).max(0.0))
    }

    /// How long it has run at tick `t`, s.
    pub fn elapsed(&self, t: f64) -> Option<f64> {
        self.started.map(|s| ((t - f64::from(s)) / f64::from(TICK_HZ)).max(0.0))
    }

    /// One of the pilot's rounds scored on target `k` at tick `t`. Only the lit target counts. A
    /// strike after the clock ran out ends the drill instead (whatever the clock had been told).
    pub fn strike(&mut self, k: u8, t: u32) -> Option<DrillEvent> {
        if let Some(out) = self.tick(f64::from(t)) {
            return Some(out);
        }
        if k != self.lit() {
            return None;
        }
        match self.started {
            None => {
                self.started = Some(t);
                self.struck = 1;
                Some(DrillEvent::Started)
            }
            Some(s) => {
                self.struck += 1;
                if self.struck() < DRILL.len() {
                    return Some(DrillEvent::Struck(self.struck()));
                }
                self.reset();
                Some(DrillEvent::Cleared(f64::from(t.saturating_sub(s)) / f64::from(TICK_HZ)))
            }
        }
    }

    /// The clock at tick `t`: whether it has run out (on the tick after its deadline: a strike on
    /// the deadline's own tick still counts).
    pub fn tick(&mut self, t: f64) -> Option<DrillEvent> {
        if t <= f64::from(self.deadline()?) {
            return None;
        }
        let n = self.struck();
        self.reset();
        Some(DrillEvent::Out(n))
    }

    /// Off: out of the hall's sector, or a new suit.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Where a sphere of radius `r` moving from `a` to `b` leaves the hall's room (0..1), if it does:
/// in city coordinates, which over a room's span are a straight line's to within centimetres.
pub fn wall_end(a: Vec3, b: Vec3, r: f32) -> Option<f32> {
    let room = hall();
    let (Under::Land(ca), Under::Land(cb)) = (from_colony(a), from_colony(b)) else { return Some(0.0) };
    if ca.strip != room.strip || cb.strip != room.strip {
        return Some(0.0);
    }
    let rect = room.rect.inset(r);
    let (h0, h1) = (KERB + r, room.ceiling - r);
    let lo = [rect.s0, rect.x0, h0];
    let hi = [rect.s1, rect.x1, h1];
    let pa = [ca.s, ca.x, ca.h];
    let pb = [cb.s, cb.x, cb.h];
    let mut exit = f32::INFINITY;
    for k in 0..3 {
        if pa[k] < lo[k] || pa[k] > hi[k] {
            return Some(0.0);
        }
        let dk = pb[k] - pa[k];
        if dk > 0.0 && pb[k] > hi[k] {
            exit = exit.min((hi[k] - pa[k]) / dk);
        } else if dk < 0.0 && pb[k] < lo[k] {
            exit = exit.min((lo[k] - pa[k]) / dk);
        }
    }
    (exit <= 1.0).then_some(exit.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::{Stage, place_door, solid_built};
    use crate::content::city::{PLACES, PlaceKind};

    /// The room's middle at height `h`, in the colony's own frame.
    fn middle(h: f32) -> Vec3 {
        let r = hall();
        let (s, x) = r.rect.middle();
        CityPos::new(r.strip, x, s, h).to_colony()
    }

    #[test]
    fn the_hall_is_the_proving_grounds_room() {
        assert_eq!(PLACES[PROVING_GROUND].kind, PlaceKind::Proving);
        let r = hall();
        assert_eq!(usize::from(r.place), PROVING_GROUND);
        assert!(r.ceiling > 50.0 && r.width() > 80.0 && r.depth() > 80.0, "a suit's room: {r:?}");
    }

    #[test]
    fn weapons_are_free_in_the_hall_and_nowhere_else_inside() {
        assert!(weapons_free(middle(20.0)));
        assert!(weapons_free(middle(1.0)));
        assert!(!weapons_free(middle(hall().ceiling + 3.0)), "on its roof");
        // Out on the square, before the blast doors.
        let p = &PLACES[PROVING_GROUND];
        let ((s, x), _) = place_door(p);
        assert!(!weapons_free(CityPos::new(p.strip, x, s, 10.0).to_colony()));
        // The same spot on another strip, the axis, the inner gate.
        let r = hall();
        let (s, x) = r.rect.middle();
        assert!(!weapons_free(CityPos::new((r.strip + 1) % 3, x, s, 20.0).to_colony()));
        assert!(!weapons_free(Vec3::new(x, 0.0, 0.0)));
        assert!(!weapons_free(crate::colony::interior::INNER_GATE));
    }

    #[test]
    fn the_targets_hang_in_the_hall_clear_of_its_walls_and_move_as_they_should() {
        let r = hall();
        for t in (0..3_600u32).step_by(7) {
            for i in 0..TARGETS {
                let p = target(i, t, 0.5);
                let Under::Land(c) = from_colony(p) else { panic!("{p}") };
                let inner = r.rect.inset(TARGET_RADIUS + 2.0);
                assert!(inner.contains(c.s, c.x), "target {i} at {c:?}");
                assert!(c.h > TARGET_RADIUS + 1.0 && c.h < r.ceiling - TARGET_RADIUS - 1.0, "{i}: {c:?}");
                let e = Vec3::splat(TARGET_RADIUS);
                let (min, max) = (Vec3::new(c.x, c.h, -c.s) - e, Vec3::new(c.x, c.h, -c.s) + e);
                assert!(!solid_built(c.strip, min, max, Stage(0)), "target {i} in a wall at {c:?}");
                for j in 0..i {
                    assert!(p.distance(target(j, t, 0.5)) > 3.0 * TARGET_RADIUS, "{i} and {j} at {t}");
                }
            }
        }
        // All beyond the firing line.
        for i in 0..TARGETS {
            let Under::Land(c) = from_colony(target(i, 0, 0.0)) else { panic!() };
            let v = (if r.front.along_s { c.x } else { c.s } - r.front.face) * r.front.sign;
            assert!(v - TARGET_RADIUS > FIRING_LINE, "target {i} {v} m in");
        }
        // The still ones are still; the others move.
        for i in 0..TARGETS {
            let moved = target(i, 0, 0.0).distance(target(i, 61, 0.0));
            assert_eq!(moved == 0.0, i < 4, "target {i} moved {moved} m");
        }
    }

    #[test]
    fn a_round_stops_at_a_target_or_the_halls_bounds_and_never_leaves_it() {
        let t = 1_000;
        let from = middle(20.0);
        // At a target: it's hit, a radius short of its middle.
        for i in 0..TARGETS {
            let c = target(i, t, 0.0);
            let to = from + (c - from).normalize() * 200.0;
            let (f, stop) = shot_end(from, to, 0.2, t, 0.0).expect("stopped");
            if stop == Stop::Target(i as u8) {
                let at = from + (to - from) * f;
                assert!((at.distance(c) - TARGET_RADIUS - 0.2).abs() < 0.05, "{i}: {}", at.distance(c));
            } else {
                // Another target in the way: it's nearer.
                let Stop::Target(j) = stop else { panic!("target {i}: {stop:?}") };
                assert!(from.distance(target(usize::from(j), t, 0.0)) < from.distance(c));
            }
        }
        // Straight up: the roof; straight out through the blast doors: the curtain across them.
        let r = hall();
        let up = middle(r.ceiling + 40.0);
        let (f, stop) = shot_end(from, up, 0.0, t, 0.0).expect("the roof");
        assert_eq!(stop, Stop::Wall);
        let at = from + (up - from) * f;
        let Under::Land(c) = from_colony(at) else { panic!() };
        assert!((c.h - r.ceiling).abs() < 0.2, "{c:?}");
        let p = &PLACES[PROVING_GROUND];
        let ((s, x), _) = place_door(p);
        let out = CityPos::new(p.strip, x, s, 20.0).to_colony();
        let (f, stop) = shot_end(from, out + (out - from), 0.0, t, 0.0).expect("the doors");
        assert_eq!(stop, Stop::Wall);
        let Under::Land(c) = from_colony(from + (out + (out - from) - from) * f) else { panic!() };
        assert!(r.rect.inset(-0.2).contains(c.s, c.x), "stopped at the room's edge: {c:?}");
        // A move inside that meets nothing; one that starts outside stops where it starts.
        let near = middle(20.5);
        assert_eq!(shot_end(from, near, 0.0, t, 0.0), None);
        assert_eq!(shot_end(out, out + Vec3::X, 0.0, t, 0.0), Some((0.0, Stop::Wall)));
    }

    #[test]
    fn the_gantry_is_open_floor_in_the_firing_line_beside_the_way_in() {
        let r = hall();
        let Under::Land(c) = from_colony(gantry()) else { panic!() };
        assert_eq!(c.strip, r.strip);
        // In the room, in the firing line, and a Leo's hull (10 m) and more from its walls.
        assert!(r.rect.inset(12.0).contains(c.s, c.x), "{c:?} in {:?}", r.rect);
        assert!(GANTRY.1 + GANTRY_RADIUS < FIRING_LINE, "short of the targets");
        // Clear of the way in from the blast doors, and nothing stands there for a suit's height.
        assert!(GANTRY.0.abs() - GANTRY_RADIUS > r.door_width() * 0.5 - 10.0);
        let e = Vec3::new(10.0, 0.0, 10.0);
        let at = Vec3::new(c.x, 0.0, -c.s);
        assert!(!solid_built(c.strip, at - e + Vec3::Y * 0.5, at + e + Vec3::Y * 18.0, Stage(0)));
        // A suit at rest over it docks there; one moving, too high, or off it doesn't.
        let up = crate::colony::frame::up_at(gantry());
        assert!(in_gantry(gantry() + up * 9.0, Vec3::ZERO));
        assert!(in_gantry(gantry() + up * 25.0, Vec3::X * 2.0));
        assert!(!in_gantry(gantry() + up * 9.0, Vec3::X * 6.0));
        assert!(!in_gantry(gantry() + up * 40.0, Vec3::ZERO));
        assert!(!in_gantry(middle(9.0), Vec3::ZERO));
        assert!(!in_gantry(crate::colony::interior::INNER_GATE, Vec3::ZERO));
        // Facing in, along the floor.
        let f = gantry_facing();
        assert!(f.dot(up).abs() < 1e-3 && (f.length() - 1.0).abs() < 1e-4);
        let ahead = from_colony(gantry() + f * 30.0);
        let Under::Land(a) = ahead else { panic!() };
        let v = |s: f32, x: f32| (if r.front.along_s { x } else { s } - r.front.face) * r.front.sign;
        assert!((v(a.s, a.x) - v(c.s, c.x) - 30.0).abs() < 0.1, "facing in");
    }

    #[test]
    fn the_hatch_is_on_the_halls_floor_by_the_pad_facing_it() {
        let r = hall();
        let ((s, x), (ds, dx)) = hatch();
        assert!(r.rect.inset(2.0).contains(s, x), "in the room");
        // Clear of the walls a person walks into, and on the floor.
        let feet = Vec3::new(x, 0.5, -s);
        let reach = Vec3::new(0.4, 0.0, 0.4);
        assert!(!crate::colony::city::solid(r.strip, feet - reach, feet + reach + Vec3::Y * 1.6, Stage(0)));
        // Beside the pad, outside a trainer's hull standing on it, and facing its middle.
        let Under::Land(c) = from_colony(gantry()) else { panic!() };
        let d = (c.s - s).hypot(c.x - x);
        assert!(d > 11.0 && d < GANTRY_RADIUS + 1.0, "{d} m from the pad's middle");
        assert!(((c.s - s) / d * ds + (c.x - x) / d * dx - 1.0).abs() < 1e-4);
        assert!(!in_hall(CityPos::new(r.strip, x, s, r.ceiling + 1.0).to_colony()));
        assert!(in_hall(CityPos::new(r.strip, x, s, 1.0).to_colony()));
    }

    #[test]
    fn the_drill_lights_its_targets_in_turn() {
        // Every target of the hall, none twice in a row, starting on one that stands still.
        for (k, t) in DRILL.iter().enumerate() {
            assert!(usize::from(*t) < TARGETS);
            assert_ne!(Some(t), DRILL.get(k + 1), "{k}");
        }
        for i in 0..TARGETS as u8 {
            assert!(DRILL.contains(&i), "target {i} in the drill");
        }
        assert!(TRACKS[usize::from(DRILL[0])].period == 0, "the first stands still");
    }

    #[test]
    fn the_drills_clock_starts_on_the_first_runs_down_and_each_target_puts_time_back() {
        let hz = f64::from(TICK_HZ);
        let mut d = Drill::default();
        assert_eq!(d.lit(), DRILL[0]);
        assert!(!d.running() && d.left(0.0).is_none() && d.tick(1e9).is_none());
        // Anything but the lit target counts for nothing.
        assert_eq!(d.strike(DRILL[1], 100), None);
        assert!(!d.running());
        // The first: the clock runs, with its start on it.
        assert_eq!(d.strike(DRILL[0], 100), Some(DrillEvent::Started));
        assert_eq!(d.lit(), DRILL[1]);
        assert_eq!(d.left(100.0), Some(DRILL_START_S));
        assert_eq!(d.elapsed(130.0), Some(1.0));
        // The second, 4 s on: 3 s back on the clock.
        assert_eq!(d.strike(DRILL[2], 220), None, "not lit yet");
        assert_eq!(d.strike(DRILL[1], 220), Some(DrillEvent::Struck(2)));
        let left = DRILL_START_S + DRILL_BONUS_S - 4.0;
        assert!((d.left(220.0).unwrap() - left).abs() < 1e-9);
        // Every one, a second apart: cleared, the time from the first.
        let mut t = 220;
        for (k, &target) in DRILL.iter().enumerate().skip(2) {
            t += 30;
            let e = d.strike(target, t);
            if k + 1 < DRILL.len() {
                assert_eq!(e, Some(DrillEvent::Struck(k + 1)));
            } else {
                let want = f64::from(t - 100) / hz;
                assert_eq!(e, Some(DrillEvent::Cleared(want)));
            }
        }
        assert!(!d.running() && d.lit() == DRILL[0], "ready to go again");

        // Too slow: the clock runs out on the tick after its deadline (a strike on the deadline's
        // own tick still counts), and a strike after it ends the drill rather than counting.
        let mut d = Drill::default();
        d.strike(DRILL[0], 0);
        let end = d.deadline().unwrap();
        assert_eq!(end, (DRILL_START_S * hz) as u32);
        assert_eq!(d.tick(f64::from(end)), None);
        assert_eq!(d.strike(DRILL[1], end), Some(DrillEvent::Struck(2)));
        let end = d.deadline().unwrap();
        assert_eq!(end, ((DRILL_START_S + DRILL_BONUS_S) * hz) as u32);
        assert_eq!(d.tick(f64::from(end) + 0.5), Some(DrillEvent::Out(2)));
        assert!(!d.running());
        let mut d = Drill::default();
        d.strike(DRILL[0], 0);
        let end = d.deadline().unwrap();
        assert_eq!(d.strike(DRILL[1], end + 1), Some(DrillEvent::Out(1)));
        assert!(!d.running());
    }
}
