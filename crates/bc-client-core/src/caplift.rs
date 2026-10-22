//! The cap lift's car (`docs/COLONY.md`, "Lift"): a glass car on the end cap's face, ridden in
//! real time between the bay ring, 948 m up, and its landing on the street beside Hub Gate's
//! terminal, with the colony out of its windows all the way. Its ride is a closed form of the time
//! since it set off, so the car drawn, the eye riding in it and its arrival agree; at the landing
//! its doors open on the street and the pilot walks out of it, with no cut.
//!
//! Only the pilot riding it sees it move: everyone else sees each strip's car at its landing.

use bc_sim::colony::city::{Rect, TERMINAL_HALF, terminal_rect};
use bc_sim::colony::frame::STRIP_WIDTH;
use bc_sim::colony::hub::BAY_RADIUS;
use bc_sim::world::COLONY_RADIUS;

/// From the bay ring down to the street, m.
pub const DROP: f32 = COLONY_RADIUS - BAY_RADIUS;
/// How fast it goes, m/s, and how hard it starts and stops, m/s².
pub const TOP_SPEED: f32 = 40.0;
pub const ACCEL: f32 = 2.5;
/// The car: across the strip, out from the cap, and floor to ceiling, m.
pub const WIDTH: f32 = 5.0;
pub const DEPTH: f32 = 4.4;
pub const HEIGHT: f32 = 3.0;
/// Off the terminal's side, and off the cap's face (its rails between), m.
const SIDE_GAP: f32 = 4.0;
const CAP_GAP: f32 = 1.2;
/// The shaft's glass round the car: across (the plumb walls close in a third on the way up, and the
/// car doesn't), and in front, m.
const SHAFT_SIDE: f32 = 2.5;
const SHAFT_FRONT: f32 = 0.6;

/// The car at its landing, on the street beside Hub Gate's terminal (the same on every strip).
pub fn car_rect() -> Rect {
    let t = terminal_rect();
    let s0 = STRIP_WIDTH * 0.5 + TERMINAL_HALF + SIDE_GAP;
    Rect::new(s0, s0 + WIDTH, t.x0 + CAP_GAP, t.x0 + CAP_GAP + DEPTH)
}

/// Its shaft up the cap: its footprint.
pub fn shaft_rect() -> Rect {
    let c = car_rect();
    Rect::new(c.s0 - SHAFT_SIDE, c.s1 + SHAFT_SIDE, terminal_rect().x0, c.x1 + SHAFT_FRONT)
}

/// Its rails on the cap's face, either side of the car.
pub fn rails() -> [Rect; 2] {
    let (c, x0) = (car_rect(), terminal_rect().x0);
    [
        Rect::new(c.s0 + 0.3, c.s0 + 1.1, x0, x0 + CAP_GAP - 0.2),
        Rect::new(c.s1 - 1.1, c.s1 - 0.3, x0, x0 + CAP_GAP - 0.2),
    ]
}

/// Where a rider stands in it, `(s, x)`: its middle.
pub fn stand() -> (f32, f32) {
    car_rect().middle()
}

/// Whether `(s, x)` is inside the car at its landing.
pub fn holds(s: f32, x: f32) -> bool {
    car_rect().contains(s, x)
}

/// The way to it from the street in front of Hub Gate's door, `(s, x)` (the door's as it is, then
/// across in front of the terminal, then in): its last two steps.
pub fn approach() -> [(f32, f32); 2] {
    let (s, x) = stand();
    [(s, terminal_rect().x1 + 4.0), (s, x)]
}

/// How long a ride takes, s (about 40).
pub fn ride_secs() -> f32 {
    let (t, d) = speeding_up();
    2.0 * t + (DROP - 2.0 * d) / TOP_SPEED
}

/// How long it speeds up for, s, and how far it goes doing it, m.
fn speeding_up() -> (f32, f32) {
    let t = TOP_SPEED / ACCEL;
    (t, 0.5 * ACCEL * t * t)
}

/// How far it has gone `t` s into a ride, m: speeding up, at its top speed, and slowing to a stop.
pub fn travelled(t: f32) -> f32 {
    let (ta, da) = speeding_up();
    let total = ride_secs();
    let t = t.clamp(0.0, total);
    if t < ta {
        0.5 * ACCEL * t * t
    } else if t < total - ta {
        da + TOP_SPEED * (t - ta)
    } else {
        let left = total - t;
        DROP - 0.5 * ACCEL * left * left
    }
}

/// How high its floor is over the street `t` s into a ride down (`down`) or up, m.
pub fn floor(t: f32, down: bool) -> f32 {
    let d = travelled(t);
    if down { DROP - d } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{Stage, ground, place_door, solid};
    use bc_sim::colony::frame::{CityPos, STRIPS};
    use bc_sim::content::city::{PLACES, PlaceKind};
    use glam::Vec3;

    #[test]
    fn a_ride_takes_about_forty_seconds_and_keeps_to_its_limits() {
        let total = ride_secs();
        assert!((39.0..41.0).contains(&total), "{total}");
        assert_eq!(floor(0.0, true), DROP);
        assert!(floor(total, true).abs() < 1e-3);
        assert!((floor(total, false) - DROP).abs() < 1e-3);
        let dt = 0.1;
        let mut v0 = 0.0;
        let mut t = 0.0;
        while t < total {
            let v = (travelled(t + dt) - travelled(t)) / dt;
            assert!((0.0..=TOP_SPEED + 0.1).contains(&v), "{v} m/s at {t}");
            assert!(((v - v0) / dt).abs() <= ACCEL + 0.1, "{} m/s² at {t}", (v - v0) / dt);
            v0 = v;
            t += dt;
        }
    }

    #[test]
    fn its_landing_is_clear_and_the_street_runs_out_of_its_doors() {
        let c = car_rect();
        for strip in 0..STRIPS as u8 {
            // Standing anywhere in it, and walking out of its doors 20 m down the colony.
            let mut x = c.x0 + 0.4;
            while x < c.x1 + 20.0 {
                let mut s = c.s0 + 0.4;
                while s < c.s1 - 0.4 {
                    let at = CityPos::new(strip, x, s, 0.0).walker();
                    let (min, max) = (at - Vec3::new(0.3, -0.2, 0.3), at + Vec3::new(0.3, 1.8, 0.3));
                    assert!(!solid(strip, min, max, Stage(0)), "strip {strip}: solid at {s}, {x}");
                    assert_eq!(ground(strip, s, x, Stage(0)), 0.0);
                    s += 0.5;
                }
                x += 0.5;
            }
        }
    }

    #[test]
    fn it_lands_near_hub_gates_door_and_fits_its_shaft_all_the_way_up() {
        let (s, x) = stand();
        for p in PLACES.iter().filter(|p| p.kind == PlaceKind::HubGate) {
            let ((ds, dx), _) = place_door(p);
            // The plaza takes a first pose this near where everyone comes down (150 m).
            assert!((s - ds).hypot(x - dx) < 100.0);
        }
        // The shaft's plumb walls close in towards the axis; the car stays the size it is.
        let narrowest = shaft_rect().width() * BAY_RADIUS / COLONY_RADIUS;
        assert!(narrowest > WIDTH + 1.0, "{narrowest}");
        // Its way in passes the terminal's side, not through it.
        let t = terminal_rect();
        for (s, x) in approach() {
            assert!(!t.contains(s, x));
        }
        assert!(approach()[0].0 > t.s1 + 1.0);
    }
}
