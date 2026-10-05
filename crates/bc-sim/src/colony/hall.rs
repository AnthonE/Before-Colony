//! The Blast Hall (`docs/TRAINING.md`), the Proving Ground's room off Hub Gate's square, and its
//! live fire: the colony's law's one exception. A suit in its room may fire; anywhere else inside
//! the colony nothing does. What it fires are training rounds: they never touch a suit, they stop
//! at the hall's walls and at its blast doors (nothing fired in it leaves it), and they score on the
//! hall's targets, holograms hung in its air, some still, some bobbing, some sweeping across it.
//!
//! All of it is closed forms of where a suit is, where a shot goes and the tick, as the rest of the
//! colony is: the server's interior sector and the owner's prediction ask the same questions and get
//! the same answers, and every client draws the targets where the tick has them.

use core::f32::consts::TAU;

use glam::Vec3;

use super::city::{KERB, Room, room};
use super::frame::{CityPos, Under, from_colony};
use crate::content::city::PROVING_GROUND;
use crate::math::{sin, sqrt};

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

/// The targets: four still by the back wall (two low, two at a standing suit's head), four bobbing
/// over the middle of the floor, four sweeping across high up, each on its own beat. However they
/// move, none comes within three radii of another.
const TRACKS: [Track; TARGETS] = [
    still(-30.0, 74.0, 8.0),
    still(-10.0, 74.0, 20.0),
    still(10.0, 74.0, 8.0),
    still(30.0, 74.0, 20.0),
    bob(-24.0, 40.0, 22.0, 150, 0.0),
    bob(-8.0, 40.0, 30.0, 190, 0.3),
    bob(8.0, 40.0, 22.0, 170, 0.6),
    bob(24.0, 40.0, 30.0, 210, 0.9),
    sweep(56.0, 38.0, 240, 0.0),
    sweep(70.0, 38.0, 300, 0.25),
    sweep(56.0, 54.0, 360, 0.5),
    sweep(70.0, 54.0, 270, 0.75),
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
}
