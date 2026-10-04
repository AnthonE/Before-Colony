//! The city showcase's script (`?showcase=city`), testable: its named pilots and two vehicles on its
//! own clock, and where its street-level cameras stand. The pilots are some of the city's own people
//! (`bc_sim::colony::walkers`) given names and flight suits, so they walk where the walkers do and
//! nobody walks through them (the life drawn skips the civilians they borrow); the vehicles keep to
//! the avenue's inner lanes, which the traffic (`traffic`) never drives.

use bc_proto::presence::{PersonPose, RIDE_CAR, RIDE_SCOOTER};
use bc_sim::TICK_HZ;
use bc_sim::colony::city::{CANAL_ROW, KERB, Rect, Stage, block_rect, channel};
use bc_sim::colony::frame::STRIP_WIDTH;
use bc_sim::colony::transit::{FLOOR, station_x};
use bc_sim::colony::walkers::{Pose, each_walker};

/// The pilots' names, as the showcase has always had them.
pub const NAMES: [&str; 16] = [
    "Heero",
    "Duo",
    "Trowa",
    "Quatre",
    "Wufei",
    "Relena",
    "Zechs",
    "Noin",
    "Sally",
    "Hilde",
    "Catherine",
    "Dorothy",
    "Lady Une",
    "Treize",
    "Howard",
    "Rashid",
];

/// The third camera's place along the avenue: 20 m short of a narrow cross street, in the half of
/// the signals' 512 m stretch where both ways' platoons are about (`the_third_camera_sees_life`).
const THIRD: f32 = -13_716.0;
/// The stretch of the avenue by the third camera where the pilots are found: along, and out from
/// the middle either side.
const STRETCH: (f32, f32) = (THIRD - 40.0, THIRD + 200.0);
const OUT: (f32, f32) = (22.0, 40.0);
/// The scripted vehicles' lanes, out from the middle: the carriageways' inner lanes.
pub const INNER_LANE: f32 = 10.7;

/// The script at `t` seconds: the people to draw (slot, name, pose), and which of the city's people
/// they are (`walkers::Walker::id`), for the life drawn to leave out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub people: Vec<(u16, String, PersonPose)>,
    pub borrowed: Vec<u32>,
}

/// The showcase's tick and the fraction of the next at `t` seconds.
pub fn clock(t: f64) -> (u32, f32) {
    let ticks = t.max(0.0) * f64::from(TICK_HZ);
    (ticks.floor() as u32, (ticks - ticks.floor()) as f32)
}

/// The showcase's pilots and vehicles at `t` seconds.
pub fn crowd(t: f64) -> Script {
    let (tick, frac) = clock(t);
    let mid = STRIP_WIDTH * 0.5;
    let mut found = Vec::new();
    for side in [-1.0f32, 1.0] {
        let (a, b) = (mid + side * OUT.0, mid + side * OUT.1);
        let area = Rect::new(a.min(b), a.max(b), STRETCH.0, STRETCH.1);
        each_walker(0, &area, Stage(0), tick, frac, |w| {
            if w.pose == Pose::Walk && w.fade >= 1.0 && w.id % 5 == 0 {
                found.push(*w);
            }
            false
        });
    }
    found.sort_by_key(|w| w.id);
    let mut script = Script::default();
    let mut taken = [false; 16];
    for w in found {
        let k = (w.id >> 3) as usize % NAMES.len();
        if taken[k] {
            continue;
        }
        taken[k] = true;
        let pose = PersonPose {
            strip: 0,
            x: w.x,
            s: w.s,
            h: w.h,
            yaw: w.yaw,
            pitch: 0.0,
            speed: w.speed,
            grounded: true,
            running: false,
            ride: 0,
        };
        script.people.push((k as u16, NAMES[k].to_string(), pose));
        script.borrowed.push(w.id);
    }
    // A car up the avenue's +x carriageway (the −s side: traffic keeps right) and a scooter down the
    // other, each in its inner lane, on its own loop.
    let ts = t as f32;
    for (k, (name, ride, dir, s, speed)) in [
        ("Noin-car", RIDE_CAR, 1.0f32, mid - INNER_LANE, 11.0),
        ("Hilde", RIDE_SCOOTER, -1.0, mid + INNER_LANE, 8.0),
    ]
    .into_iter()
    .enumerate()
    {
        let pose = PersonPose {
            strip: 0,
            x: vehicle_x(k, dir, speed, ts),
            s,
            h: 0.0,
            yaw: if dir > 0.0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 },
            pitch: 0.0,
            speed,
            grounded: true,
            running: false,
            ride,
        };
        script.people.push((100 + k as u16, name.to_string(), pose));
    }
    script
}

/// Where scripted vehicle `k` is along at `t` seconds: round a 220 m loop at `speed` m/s (over two
/// narrow cross streets, which no car drives and nobody crosses by the avenue).
fn vehicle_x(k: usize, dir: f32, speed: f32, t: f32) -> f32 {
    THIRD + (dir * speed * t + 40.0 * k as f32).rem_euclid(220.0)
}

/// The street-level cameras' eyes, `(strip, s, x, h)`: the avenue's pavement downtown (the third
/// camera), the window bank (the fourth), the canal's quay (the fifth) and a tram platform (the
/// seventh). Each stands where nobody walks and no car drives.
pub fn street_eyes() -> [(u8, f32, f32, f32); 4] {
    let mid = STRIP_WIDTH * 0.5;
    let quay = channel(&block_rect(40, CANAL_ROW)).s0;
    [
        // In the planting beds' band between the avenue's two walks.
        (0, mid + 32.5, THIRD, 1.65),
        (0, 60.0, -12_000.0, 1.65),
        // By the water, inside the quay's walk, just past the cross street (its bays).
        (0, quay - 1.5, -10_988.0, KERB + 1.65),
        // Mid-platform, between the middle car's doors and their queues.
        (0, mid - 1.0, station_x(5), FLOOR + 1.65),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{BAYS, CARS_MID, CARS_NEAR, Choice, Clears, Eye, PEOPLE_MID, PEOPLE_NEAR, TIERS};
    use crate::vehicle::{Kind as Vehicle, spec};
    use bc_sim::colony::traffic::{each_bay_car, each_car, each_ring_car};
    use bc_sim::colony::walkers::{RADIUS, Walker};
    use glam::Vec3;

    /// The review set's hours (s), each looked at ±20 s.
    const HOURS: [f64; 6] = [6.0, 480.0, 1350.0, 1600.0, 1900.0, 2620.0];

    fn times() -> impl Iterator<Item = f64> {
        HOURS.into_iter().flat_map(|h| (0..400).map(move |i| (h - 20.0 + i as f64 * 0.1).max(0.0)))
    }

    fn walkers(area: &Rect, t: f64) -> Vec<Walker> {
        let (tick, frac) = clock(t);
        let mut out = Vec::new();
        each_walker(0, area, Stage(0), tick, frac, |w| {
            out.push(*w);
            false
        });
        out
    }

    /// The gap between two convex footprints, `(s, x)` corners in order round (0 if they meet).
    fn gap(a: &[(f32, f32); 4], b: &[(f32, f32); 4]) -> f32 {
        let inside = |p: (f32, f32), q: &[(f32, f32); 4]| {
            let side = |i: usize| {
                let (u, v) = (q[i], q[(i + 1) % 4]);
                (v.0 - u.0) * (p.1 - u.1) - (v.1 - u.1) * (p.0 - u.0)
            };
            let s: Vec<f32> = (0..4).map(side).collect();
            s.iter().all(|x| *x >= 0.0) || s.iter().all(|x| *x <= 0.0)
        };
        if a.iter().any(|p| inside(*p, b)) || b.iter().any(|p| inside(*p, a)) {
            return 0.0;
        }
        let mut best = f32::MAX;
        for i in 0..4 {
            for j in 0..4 {
                for (p, (u, v)) in [(a[i], (b[j], b[(j + 1) % 4])), (b[j], (a[i], a[(i + 1) % 4]))] {
                    best = best.min(point_segment(p, u, v));
                }
            }
        }
        best
    }

    fn point_segment(p: (f32, f32), u: (f32, f32), v: (f32, f32)) -> f32 {
        let (dx, dy) = (v.0 - u.0, v.1 - u.1);
        let l2 = dx * dx + dy * dy;
        let k = if l2 > 0.0 { (((p.0 - u.0) * dx + (p.1 - u.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        ((p.0 - u.0 - k * dx).powi(2) + (p.1 - u.1 - k * dy).powi(2)).sqrt()
    }

    /// A scripted vehicle's footprint, `(s, x)` corners round.
    fn footprint(p: &PersonPose) -> [(f32, f32); 4] {
        let sp = spec(if p.ride == RIDE_CAR { Vehicle::Car } else { Vehicle::Scooter });
        let (hl, hw) = (0.5 * sp.length, 0.5 * sp.width);
        [(p.s - hw, p.x - hl), (p.s + hw, p.x - hl), (p.s + hw, p.x + hl), (p.s - hw, p.x + hl)]
    }

    #[test]
    fn the_named_pilots_are_the_citys_people() {
        let mid = STRIP_WIDTH * 0.5;
        let mut named = 0;
        for t in times().step_by(5) {
            let script = crowd(t);
            let pilots: Vec<_> = script.people.iter().filter(|(k, _, _)| *k < 100).collect();
            assert_eq!(pilots.len(), script.borrowed.len());
            assert!(pilots.len() <= NAMES.len());
            let all = walkers(&Rect::new(mid - 41.0, mid + 41.0, STRETCH.0 - 1.0, STRETCH.1 + 1.0), t);
            for ((k, name, p), id) in pilots.iter().zip(&script.borrowed) {
                let w = all.iter().find(|w| w.id == *id).expect("a borrowed walker");
                assert_eq!((p.x, p.s, p.h, p.yaw), (w.x, w.s, w.h, w.yaw), "{name} is walker {id}");
                assert_eq!(NAMES[*k as usize], name);
            }
            let mut names: Vec<&String> = pilots.iter().map(|(_, n, _)| n).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), pilots.len(), "a name twice");
            named += pilots.len();
        }
        assert!(named > 100, "only {named} named pilots seen");
    }

    #[test]
    fn the_scripted_vehicles_keep_out_of_the_way() {
        let mid = STRIP_WIDTH * 0.5;
        let mut closest = f32::MAX;
        for t in times() {
            let script = crowd(t);
            let (tick, frac) = clock(t);
            for (_, name, p) in script.people.iter().filter(|(_, _, p)| p.driving()) {
                let mine = footprint(p);
                let area = Rect::new(p.s - 12.0, p.s + 12.0, p.x - 12.0, p.x + 12.0);
                each_car(0, &area, Stage(0), tick, frac, |c| {
                    let g = gap(&mine, &c.corners());
                    closest = closest.min(g);
                    assert!(g >= 0.3, "{name} at {t} s: {g} m from {c:?}");
                    false
                });
                for w in walkers(&area, t) {
                    let corners = [(w.s, w.x); 4];
                    let g = gap(&mine, &corners) - RADIUS;
                    assert!(g >= 0.3, "{name} at {t} s: {g} m from {w:?}");
                }
                assert!((p.s - mid).abs() > 8.0 + 1.0, "{name} off the median");
            }
        }
        println!("the scripted vehicles' closest car: {closest} m");
    }

    #[test]
    fn nobody_walks_or_drives_through_the_street_cameras() {
        let mut nearest = [(f32::MAX, f32::MAX); 4];
        for t in times().step_by(3) {
            let (tick, frac) = clock(t);
            for (i, (strip, s, x, h)) in street_eyes().into_iter().enumerate() {
                let area = Rect::new(s - 4.0, s + 4.0, x - 4.0, x + 4.0);
                let mut people = Vec::new();
                each_walker(strip, &area, Stage(0), tick, frac, |w| {
                    people.push(*w);
                    false
                });
                for w in people {
                    // Their head and shoulders, about the eye's height (a seated one's lower).
                    if (h - w.h) < 2.2 {
                        let d = (w.s - s).hypot(w.x - x);
                        nearest[i].0 = nearest[i].0.min(d);
                    }
                }
                each_car(strip, &area, Stage(0), tick, frac, |c| {
                    let d = gap(&[(s, x); 4], &c.corners());
                    nearest[i].1 = nearest[i].1.min(d);
                    false
                });
            }
        }
        for (i, (people, cars)) in nearest.iter().enumerate() {
            println!("eye {i}: nearest person {people} m, car {cars} m");
            assert!(*people >= 1.2, "eye {i}: somebody walks {people} m from it");
            assert!(*cars >= 2.0, "eye {i}: a car {cars} m from it");
        }
    }

    #[test]
    fn the_third_camera_sees_life() {
        let (_, s, x, _) = street_eyes()[0];
        // Cars moving on the avenue up ahead at every hour of the review: the platoons both ways
        // pass a 400 m stretch together, so one looked up from the middle of a signals' stretch
        // (blocks 4n+1 and 4n+2) has none a quarter of the time.
        assert!(matches!(bc_sim::colony::city::block_index(x).rem_euclid(4), 0 | 3));
        let mid = STRIP_WIDTH * 0.5;
        for t in HOURS {
            let (tick, frac) = clock(t);
            let mut moving = 0;
            let ahead = Rect::new(mid - 22.0, mid + 22.0, x, x + 400.0);
            each_ring_car(0, &ahead, Stage(0), tick, frac, |c| {
                moving += usize::from(!c.parked && c.speed > 0.5 && (c.s - mid).abs() < 22.0 && c.x >= x);
                false
            });
            assert!(moving > 0, "at {t} s: no car moving on the avenue ahead");
        }
        for t in [6.0, 480.0] {
            let (tick, frac) = clock(t);
            let mut people = 0;
            each_walker(0, &Rect::new(s - 60.0, s + 60.0, x - 60.0, x + 60.0), Stage(0), tick, frac, |w| {
                people += usize::from((w.s - s).hypot(w.x - x) < 60.0);
                false
            });
            let mut cars = 0;
            let near = Rect::new(s - 150.0, s + 150.0, x - 150.0, x + 150.0);
            let mut count = |c: &bc_sim::colony::traffic::Car| {
                cars += usize::from((c.s - s).hypot(c.x - x) < 150.0);
                false
            };
            each_ring_car(0, &near, Stage(0), tick, frac, &mut count);
            each_bay_car(0, &near, Stage(0), &mut count);
            println!("at {t} s: {people} people within 60 m, {cars} cars within 150 m");
            assert!(people > 10 && cars > 5, "at {t} s: {people} people, {cars} cars");
            // And every tier draws some of each there, looking up the avenue as the camera does.
            let (_, _, _, h) = street_eyes()[0];
            let eye = Eye { strip: 0, s, x, h, forward: Vec3::X };
            for (k, tier) in TIERS.iter().enumerate() {
                let mut c = Choice::default();
                c.choose(&eye, tier, tick, frac, &Clears::default());
                let people = c.picks[PEOPLE_NEAR].len() + c.picks[PEOPLE_MID].len();
                let cars = c.picks[CARS_NEAR].len() + c.picks[CARS_MID].len() + c.picks[BAYS].len();
                assert!(people > 0 && cars > 0, "tier {k} at {t} s: {people} people and {cars} cars drawn");
            }
        }
    }
}
