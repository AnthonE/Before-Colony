//! The city's traffic (`colony::traffic`): its cars keep to the roads (off the pavements, the kerbs,
//! the furniture, the tram's median), never touch each other, keep off the junctions while people
//! cross and off the narrow streets' crossings always, move without a jump and never appear or vanish
//! in view, come round exactly, thin out at night, and cost little to ask about.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::collections::HashMap;

use bc_sim::colony::city::{
    AVENUE, BLOCK, MEDIAN, ROWS, Rect, Stage, block_index, cross_width, each_solid, grid_x, lane_width,
};
use bc_sim::colony::frame::STRIP_WIDTH;
use bc_sim::colony::furniture::{ROAD_OUT, each_furniture};
use bc_sim::colony::time::DAY_TICKS;
use bc_sim::colony::traffic::{
    self as tr, CRUISE, CYCLE, Car, FIRST, GO, Light, SIGNAL_EVERY, each_car, last_street, signal,
};

const STAGE: Stage = Stage(0);
/// The rows' last cross street with two of the site's districts built.
const LAST_BUILT: i32 = tr::LAST + 32;
const MID: f32 = STRIP_WIDTH * 0.5;

fn row_edge(k: i32) -> f32 {
    AVENUE * 0.5 + BLOCK * k as f32
}

fn cars(strip: u8, area: &Rect, t: u32, frac: f32) -> Vec<Car> {
    let mut out = Vec::new();
    each_car(strip, area, STAGE, t, frac, |c| {
        out.push(*c);
        false
    });
    out
}

/// Whether two convex quads (corners in order) overlap, each grown by `grow` all round.
fn quads_overlap(a: &[(f32, f32); 4], b: &[(f32, f32); 4], grow: f32) -> bool {
    for poly in [a, b] {
        for i in 0..4 {
            let (p, q) = (poly[i], poly[(i + 1) % 4]);
            let (nx, ny) = (q.1 - p.1, p.0 - q.0);
            let l = (nx * nx + ny * ny).sqrt();
            let (nx, ny) = (nx / l, ny / l);
            let span = |c: &[(f32, f32); 4]| {
                c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                    let d = v.0 * nx + v.1 * ny;
                    (lo.min(d), hi.max(d))
                })
            };
            let ((a0, a1), (b0, b1)) = (span(a), span(b));
            if a1 + grow < b0 - grow || b1 + grow < a0 - grow {
                return false;
            }
        }
    }
    true
}

fn rect_quad(r: &Rect) -> [(f32, f32); 4] {
    [(r.s0, r.x0), (r.s1, r.x0), (r.s1, r.x1), (r.s0, r.x1)]
}

/// The areas the tests watch: where the avenue meets a wide cross street, a district's edge, the
/// canal's rows, the bank road, the rows' ends at Hub Gate's square and on the site.
fn areas() -> Vec<(u8, Rect)> {
    let at = |bx: i32| grid_x(bx);
    vec![
        (0, Rect::new(MID - 330.0, MID + 330.0, at(40) - 280.0, at(40) + 280.0)),
        (1, Rect::new(MID - 200.0, MID + 200.0, at(24) - 300.0, at(24) + 300.0)),
        (1, Rect::new(MID + row_edge(2), MID + row_edge(6), at(100) - 300.0, at(100) + 300.0)),
        (2, Rect::new(MID + row_edge(10), STRIP_WIDTH, at(60) - 300.0, at(60) + 300.0)),
        (2, Rect::new(0.0, MID - row_edge(10), at(60) - 300.0, at(60) + 300.0)),
        (0, Rect::new(MID - 450.0, MID + 450.0, at(FIRST) - 100.0, at(FIRST) + 300.0)),
        (
            0,
            Rect::new(
                MID - 450.0,
                MID + 450.0,
                at(last_street(STAGE)) - 300.0,
                at(last_street(STAGE)) + 100.0,
            ),
        ),
    ]
}

/// Ticks to watch: noon's traffic, the evening's thinning as the lamps come on, and the morning's
/// pulling out (each two signal cycles' worth).
fn moments(step: u32) -> impl Iterator<Item = u32> {
    // The day starts 21,600 ticks into its light (`time::day`'s START): tick 0 is mid-morning.
    [10_000u32, 49_000, 54_600, 70_000]
        .into_iter()
        .flat_map(move |t0| (t0..t0 + 2 * CYCLE).step_by(step as usize))
}

#[test]
fn no_two_cars_ever_touch() {
    let mut pairs = 0u64;
    for (strip, area) in areas() {
        for t in moments(4) {
            let all = cars(strip, &area, t, 0.5);
            let quads: Vec<_> = all.iter().map(|c| (c.corners(), c.bounds())).collect();
            for i in 0..all.len() {
                for j in i + 1..all.len() {
                    let (a, b) = (&quads[i], &quads[j]);
                    if !a.1.inset(-0.5).overlaps(&b.1) {
                        continue;
                    }
                    pairs += 1;
                    assert!(
                        !quads_overlap(&a.0, &b.0, 0.2),
                        "cars touch at {t} on strip {strip}:\n{:?}\n{:?}",
                        all[i],
                        all[j]
                    );
                }
            }
        }
    }
    assert!(pairs > 5_000, "only {pairs} close pairs: not much traffic");
}

#[test]
fn cars_keep_to_the_roads() {
    let mut seen = 0;
    for (strip, area) in areas() {
        for t in moments(29) {
            for c in cars(strip, &area, t, 0.25) {
                seen += 1;
                let q = c.corners();
                let b = c.bounds();
                // Nothing built (kerbs, buildings, platforms, railings) and no furniture.
                let hit = each_solid(strip, &b, STAGE, |s| {
                    s.h1 > 0.05 && s.h0 < 1.5 && quads_overlap(&q, &rect_quad(&s.rect), 0.0)
                });
                assert!(!hit, "a car in something built at {t}: {c:?}");
                let hit =
                    each_furniture(strip, &b, STAGE, |p| quads_overlap(&q, &rect_quad(&p.solid.rect), 0.1));
                assert!(!hit, "a car in the furniture at {t}: {c:?}");
                for (s, x) in q {
                    let a = (s - MID).abs();
                    // Off the tram's median.
                    assert!(a > MEDIAN * 0.5 + 0.3, "a car on the median at {t}: {c:?}");
                    // Off the avenue's pavements, but where a cross street crosses them.
                    if (ROAD_OUT..AVENUE * 0.5).contains(&a) {
                        let g = ((x - grid_x(0)) / BLOCK).round() as i32;
                        assert!(
                            (x - grid_x(g)).abs() < cross_width(g) * 0.5,
                            "a car on the avenue's pavement at {t}: {c:?}"
                        );
                    }
                    // Between Hub Gate's square and the site.
                    assert!(
                        x > grid_x(FIRST) - cross_width(FIRST) * 0.5 && x < grid_x(last_street(STAGE)) + 20.0
                    );
                }
            }
        }
    }
    assert!(seen > 20_000, "{seen}");
}

/// The bits of a junction people cross: its middle and its crossings (0.5 to 4.5 m past each kerb).
fn crossings(bx: i32, k: i32, side: f32) -> [Rect; 2] {
    let (x, w) = (grid_x(bx), cross_width(bx) * 0.5);
    let (lo, hi) = if k == 0 {
        (MEDIAN * 0.5, ROAD_OUT)
    } else {
        (row_edge(k) - lane_width(k) * 0.5, row_edge(k) + lane_width(k) * 0.5)
    };
    // The avenue's crossing over its pavement lies 0.5 m further out.
    let far = if k == 0 { 5.0 } else { 4.5 };
    let span = |a0: f32, a1: f32| if side > 0.0 { (MID + a0, MID + a1) } else { (MID - a1, MID - a0) };
    let (s0, s1) = span(if k == 0 { lo } else { lo - 4.5 }, hi + far);
    let (t0, t1) = span(lo, hi);
    [Rect::new(s0, s1, x - w, x + w), Rect::new(t0, t1, x - w - 4.5, x + w + 4.5)]
}

/// The junctions near a car: their cross streets and along-streets, on its side of the avenue.
fn junctions_near(c: &Car) -> Vec<(i32, i32, f32)> {
    let b = c.bounds();
    let side = (c.s - MID).signum();
    let (a0, a1) = if side > 0.0 { (b.s0 - MID, b.s1 - MID) } else { (MID - b.s1, MID - b.s0) };
    let bx = |x: f32| ((x - grid_x(0)) / BLOCK).round() as i32;
    let k = |a: f32| (((a - AVENUE * 0.5) / BLOCK).round() as i32).clamp(0, ROWS);
    let mut out = Vec::new();
    for g in bx(b.x0 - 30.0)..=bx(b.x1 + 30.0) {
        for kk in k(a0 - 30.0)..=k(a1 + 30.0) {
            out.push((g, kk, side));
        }
    }
    out
}

#[test]
fn people_cross_the_signals_and_the_narrow_streets_clear_of_cars() {
    let (mut held, mut free) = (0, 0);
    for (strip, area) in areas() {
        for t in moments(5) {
            for c in cars(strip, &area, t, 0.0) {
                let q = c.corners();
                for (bx, k, side) in junctions_near(&c) {
                    let zones = crossings(bx, k, side);
                    match signal(strip, bx, k, STAGE, t, 0.0) {
                        Some(sig) if sig.light == Light::Red => {
                            held += 1;
                            for z in &zones {
                                assert!(
                                    !quads_overlap(&q, &rect_quad(z), 0.0),
                                    "a car on junction ({bx}, {k}, {side}) while people cross, at {t}: {c:?}"
                                );
                            }
                        }
                        Some(_) => {}
                        None => {
                            // A narrow street: never a car on its crossings (only in its middle, going
                            // along the strip).
                            free += 1;
                            let (z, mid) = (zones[0], zones[1]);
                            for band in [
                                Rect::new(z.s0, mid.s0 - 0.5, z.x0, z.x1),
                                Rect::new(mid.s1 + 0.5, z.s1, z.x0, z.x1),
                            ] {
                                if band.s1 > band.s0 {
                                    assert!(
                                        !quads_overlap(&q, &rect_quad(&band), 0.0),
                                        "a car on narrow street {bx}'s crossing at {t}: {c:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(held > 100_000 && free > 100_000, "{held} {free}");
}

#[test]
fn the_signals_go_round_every_way_at_once() {
    for strip in 0..3u8 {
        for bx in [FIRST, 40, 44, 100, last_street(STAGE)] {
            for k in [0, 1, 2, 12] {
                let (mut green, mut walk) = (0, 0);
                for t in 0..CYCLE {
                    let s =
                        signal(strip, bx, k, STAGE, t, 0.0).expect("a wide street's junctions have signals");
                    green += u32::from(s.light != Light::Red);
                    walk += u32::from(s.walk);
                    assert!(!(s.walk && s.light != Light::Red));
                    assert!(s.left > 0.0 && s.left <= 40.0);
                }
                assert_eq!(green, GO);
                assert_eq!(walk, GO - tr::ALL_RED);
                // Alternating along the strip and across it.
                let at = |bx: i32, k: i32| signal(strip, bx, k, STAGE, 100, 0.0).unwrap().light == Light::Red;
                if bx + SIGNAL_EVERY <= last_street(STAGE) {
                    assert_ne!(at(bx, k), at(bx + SIGNAL_EVERY, k));
                }
                assert_ne!(at(bx, k), at(bx, k + 1));
            }
        }
        assert!(
            signal(strip, 41, 1, STAGE, 0, 0.0).is_none() && signal(strip, 4, 1, STAGE, 0, 0.0).is_none()
        );
    }
}

#[test]
fn cars_move_smoothly_and_never_appear_or_vanish_in_view() {
    let mut tracked = 0;
    for (strip, outer) in areas() {
        let inner = outer.inset(40.0);
        for t0 in [8_000u32, 48_500, 54_000, 69_500] {
            let mut last: HashMap<u64, Car> = HashMap::new();
            for t in t0..t0 + 2 * CYCLE + 60 {
                let now: HashMap<u64, Car> =
                    cars(strip, &outer, t, 0.0).into_iter().map(|c| (c.id, c)).collect();
                for (id, a) in &last {
                    let inside = inner.contains(a.s, a.x);
                    match now.get(id) {
                        Some(b) => {
                            tracked += 1;
                            let d = ((b.s - a.s).powi(2) + (b.x - a.x).powi(2)).sqrt();
                            assert!(
                                d <= CRUISE / 30.0 + 0.02,
                                "car {id:x} jumped {d} m at {t}: {a:?} → {b:?}"
                            );
                            let turn = (a.dir.0 * b.dir.1 - a.dir.1 * b.dir.0).asin().abs();
                            assert!(
                                turn < 0.05,
                                "car {id:x} turned {turn} rad in a tick at {t}: {a:?} → {b:?}"
                            );
                            assert!(
                                (b.speed - a.speed).abs() < 0.2,
                                "car {id:x}'s speed jumped at {t}: {a:?} → {b:?}"
                            );
                        }
                        None => assert!(!inside, "car {id:x} vanished at {t}: {a:?}"),
                    }
                }
                for (id, b) in &now {
                    if !last.is_empty() && !last.contains_key(id) {
                        assert!(!inner.contains(b.s, b.x), "car {id:x} appeared at {t}: {b:?}");
                    }
                }
                last = now;
            }
        }
    }
    assert!(tracked > 1_000_000, "{tracked}");
}

#[test]
fn traffic_comes_round_exactly() {
    // Track 0 every day, track 1 every lap of its row: all of it every 49 days.
    let period = 49 * DAY_TICKS;
    for (strip, area) in areas() {
        for t in [0u32, 7_777, 50_001] {
            let (a, b) = (cars(strip, &area, t, 0.3), cars(strip, &area, t + period, 0.3));
            assert_eq!(a, b, "strip {strip} at {t}");
        }
    }
    // And track 0 alone, day after day: the cars parked in a wide cross street's bays, where
    // nothing of track 1 stops.
    let leg =
        Rect::new(MID + row_edge(5) + 10.0, MID + row_edge(6) - 10.0, grid_x(40) + 2.0, grid_x(40) + 20.0);
    for t in [3_000u32, 60_000, 80_000] {
        assert_eq!(cars(1, &leg, t, 0.0), cars(1, &leg, t + DAY_TICKS, 0.0));
    }
}

#[test]
fn every_car_is_a_rings_or_a_bays() {
    // `each_car` is `each_ring_car` and `each_bay_car` (bit 63 of its id), each in its own order; the
    // bays' cars are the same at every tick.
    let (mut ringed, mut bayed) = (0, 0);
    for (strip, area) in areas() {
        let mut bays = Vec::new();
        tr::each_bay_car(strip, &area, STAGE, |c| {
            bays.push(*c);
            false
        });
        assert!(bays.iter().all(|c| c.parked && c.id >> 63 == 1), "strip {strip}: {bays:?}");
        assert_eq!(tr::each_bay_car(strip, &area, STAGE, |_| true), !bays.is_empty());
        for t in moments(997) {
            let mut rings = Vec::new();
            tr::each_ring_car(strip, &area, STAGE, t, 0.5, |c| {
                rings.push(*c);
                false
            });
            assert_eq!(tr::each_ring_car(strip, &area, STAGE, t, 0.5, |_| true), !rings.is_empty());
            let (of_bays, of_rings): (Vec<Car>, Vec<Car>) =
                cars(strip, &area, t, 0.5).into_iter().partition(|c| c.id >> 63 == 1);
            assert_eq!(of_rings, rings, "strip {strip} at {t}");
            assert_eq!(of_bays, bays, "strip {strip} at {t}");
            (ringed, bayed) = (ringed + rings.len(), bayed + bays.len());
        }
    }
    assert!(ringed > 1_000 && bayed > 1_000, "{ringed} rings' cars, {bayed} bays'");
}

#[test]
fn a_cars_home_is_where_it_drives() {
    // A ring's car round a stretch of its row keeps to it; a bay's is where it's parked; through
    // traffic has none.
    let (mut stretch, mut through) = (0, 0);
    for (strip, area) in areas() {
        for t in moments(997) {
            for c in cars(strip, &area, t, 0.5) {
                match c.home() {
                    Some(b) if c.id >> 63 == 1 => assert_eq!(b, block_index(c.x), "{c:?}"),
                    Some(b) => {
                        assert_eq!((b - FIRST) % SIGNAL_EVERY, 0, "{c:?}");
                        let x0 = grid_x(b) - 0.5 * cross_width(b);
                        let x1 = grid_x(b + SIGNAL_EVERY) + 0.5 * cross_width(b + SIGNAL_EVERY);
                        assert!((x0..=x1).contains(&c.x), "strip {strip} at {t}: {c:?} not round {b}");
                        stretch += 1;
                    }
                    None => through += 1,
                }
            }
        }
    }
    assert!(stretch > 1_000 && through > 100, "{stretch} round stretches, {through} through");
}

#[test]
fn fewer_out_at_night_and_parked_in_the_bays() {
    let area = Rect::new(MID - 800.0, MID + 800.0, grid_x(60), grid_x(76));
    let count = |t: u32| {
        let (mut moving, mut parked) = (0, 0);
        each_car(0, &area, STAGE, t, 0.0, |c| {
            if c.parked {
                parked += 1;
            } else {
                moving += 1;
            }
            false
        });
        (moving, parked)
    };
    // Mid-morning, and the dead of night (7,200 ticks before the dawn, which tick 64,800 starts).
    let (day, night) = (count(1_000), count(57_000));
    assert!(day.0 > 300, "{day:?}");
    assert!(night.0 * 10 < day.0 * 6, "night {night:?} against day {day:?}");
    assert!(night.1 > day.1, "night {night:?} against day {day:?}");
}

/// The lanes' middles the paint has on along-street `k` (0 the avenue's carriageway, 12 the bank road),
/// out from the strip's middle on one side, and the way their traffic goes there (+1: +x on the +s
/// side, −x on the −s side).
fn lanes(k: i32) -> Vec<(f32, f32)> {
    let (centre, ys): (f32, &[f32]) = match k {
        // The carriageways keep right of the median: −x on the +s side.
        0 => return vec![(17.9, -1.0), (14.3, -1.0), (10.7, -1.0)],
        ROWS => (row_edge(k) - 10.0, &[1.8]),
        _ if lane_width(k) > 30.0 => (row_edge(k), &[10.6, 7.0, 3.4]),
        _ => (row_edge(k), &[5.7, 2.1]),
    };
    ys.iter().flat_map(|y| [(centre - y, 1.0), (centre + y, -1.0)]).collect()
}

#[test]
fn cars_keep_right_in_their_lanes_and_never_cross_a_street_along_the_strip() {
    let mut along = 0;
    for (strip, area) in areas() {
        for t in moments(31) {
            for c in cars(strip, &area, t, 0.0) {
                let side = (c.s - MID).signum();
                let a = (c.s - MID).abs();
                if c.parked {
                    continue;
                }
                if c.dir.1.abs() > 0.9999 {
                    // Going along the strip: in a lane's middle, on the right of the street.
                    along += 1;
                    let k = (((a - AVENUE * 0.5) / BLOCK).round() as i32).clamp(0, ROWS);
                    let way = c.dir.1.signum() * side;
                    let ok = lanes(k).iter().any(|(m, w)| (a - m).abs() < 0.02 && *w == way);
                    assert!(ok, "a car out of its lane at {t} (street {k}, {a} out): {c:?}");
                } else if c.dir.0.abs() > 0.2 {
                    // Crossing: never over a street along the strip's middle line (or the avenue's
                    // inner lane), so never across the other way's traffic.
                    assert!(a > 12.5, "a car across the avenue at {t}: {c:?}");
                    for k in 1..=ROWS {
                        let m = if k == ROWS { row_edge(k) - 10.0 } else { row_edge(k) };
                        assert!((a - m).abs() > 1.0, "a car across street {k}'s middle at {t}: {c:?}");
                    }
                }
            }
        }
    }
    assert!(along > 10_000, "{along}");
}

#[test]
fn no_car_touches_a_tram() {
    use bc_sim::colony::transit::{self, CAR_WIDTH, TRAIN_LENGTH, TRAINS};
    for strip in 0..3u8 {
        for t in (0..transit::PERIOD_TICKS).step_by(97) {
            for k in 0..TRAINS as u8 {
                let tr = transit::train(strip, k, t, 0.0);
                let r =
                    Rect::new(tr.s - CAR_WIDTH, tr.s + CAR_WIDTH, tr.x - TRAIN_LENGTH, tr.x + TRAIN_LENGTH);
                let near = cars(strip, &r.inset(-2.0), t, 0.0);
                assert!(near.is_empty(), "a car by tram {k} at {t}: {:?}", near[0]);
            }
        }
    }
}

#[test]
fn how_many() {
    // Cars a strip has, by day and by night, and what a camera's area and a night's view hold.
    let strip_cars = |t: u32| {
        let mut n = (0, 0);
        for i in 0..24 {
            let x = grid_x(FIRST) + 1_000.0 * i as f32;
            each_car(0, &Rect::new(0.0, STRIP_WIDTH, x, x + 1_000.0), STAGE, t, 0.0, |c| {
                if c.parked {
                    n.1 += 1
                } else {
                    n.0 += 1
                }
                false
            });
        }
        n
    };
    let (day, night) = (strip_cars(1_000), strip_cars(57_000));
    let near =
        cars(0, &Rect::new(MID - 150.0, MID + 150.0, grid_x(42) - 150.0, grid_x(42) + 150.0), 1_000, 0.0)
            .len();
    let wide = cars(
        0,
        &Rect::new(MID - 1_000.0, MID + 1_000.0, grid_x(42) - 1_000.0, grid_x(42) + 1_000.0),
        57_000,
        0.0,
    );
    let lit = wide.iter().filter(|c| !c.parked).count();
    println!("a strip: {} moving and {} parked by day, {} and {} by night", day.0, day.1, night.0, night.1);
    println!("300 m round the camera: {near}; 2 km by night: {} ({lit} lit)", wide.len());
    assert!(day.0 > 10_000 && night.0 > 3_000);
}

#[test]
fn asking_is_cheap() {
    // What the client asks every frame (300 m round the camera), and for the lights at night (2 km).
    for (size, t0) in [(150.0f32, 1_000u32), (1_000.0, 57_000)] {
        let area = Rect::new(MID - size, MID + size, grid_x(42) - size, grid_x(42) + size);
        let start = std::time::Instant::now();
        let mut n = 0;
        for i in 0..100u32 {
            n += cars(0, &area, t0 + i * 37, 0.0).len();
        }
        let each = start.elapsed().as_secs_f64() / 100.0;
        println!("{} m square: {} cars, {:.3} ms a query", 2.0 * size, n / 100, each * 1e3);
        assert!(n > 0);
    }
}

#[test]
fn the_rows_run_on_as_the_site_is_built_out() {
    let stage = Stage(2);
    let end = last_street(stage);
    assert_eq!(end, LAST_BUILT);
    let area = Rect::new(MID - 450.0, MID + 450.0, grid_x(end) - 600.0, grid_x(end) + 100.0);
    let mut seen = 0;
    for t in (0..2 * CYCLE).step_by(13) {
        each_car(1, &area, stage, t, 0.0, |c| {
            seen += 1;
            let q = c.corners();
            let hit = each_solid(1, &c.bounds(), stage, |s| {
                s.h1 > 0.05 && s.h0 < 1.5 && quads_overlap(&q, &rect_quad(&s.rect), 0.0)
            });
            assert!(!hit, "a car in something built at {t}: {c:?}");
            assert!(c.x < grid_x(end) + 20.0);
            false
        });
    }
    assert!(seen > 1_000, "{seen}");
}

#[test]
fn cars_clear_the_junctions_well_before_the_amber() {
    // When in the cars' half the junctions' crossings have cars on them: from as the light turns
    // green to well before it turns amber.
    let (mut first, mut last) = (u32::MAX, 0u32);
    for (strip, area) in areas() {
        for t in moments(3) {
            for c in cars(strip, &area, t, 0.0) {
                for (bx, k, side) in junctions_near(&c) {
                    if tr::signalised(bx, STAGE)
                        && crossings(bx, k, side)
                            .iter()
                            .any(|z| quads_overlap(&c.corners(), &rect_quad(z), 0.0))
                    {
                        let into = (t % CYCLE + CYCLE - tr::phase(strip, bx, k)) % CYCLE;
                        first = first.min(into);
                        last = last.max(into);
                    }
                }
            }
        }
    }
    println!(
        "cars on a junction's crossings from {:.1} s to {:.1} s into its 40 s",
        first as f32 / 30.0,
        last as f32 / 30.0
    );
    assert!(first >= 15 && last + 60 <= GO - tr::AMBER, "{first} {last}");
}
