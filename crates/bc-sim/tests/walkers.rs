//! The city's people (`colony::walkers`): never in anything solid or in each other, moving
//! smoothly, coming round each day, busier downtown and by day, sitting on the avenue's benches and
//! getting on and off trains through their open doors; any area asked finds each once.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::collections::{HashMap, HashSet};

use bc_sim::colony::city::{CITY, HUB_GATE, ROWS, Rect, SITE, Stage, block, grid_x, solid};
use bc_sim::colony::frame::STRIP_WIDTH;
use bc_sim::colony::furniture::{Kind, each_furniture};
use bc_sim::colony::time::DAY_TICKS;
use bc_sim::colony::transit::{
    CAR_WIDTH, DOOR_AT, DOOR_WIDTH, HEADWAY_TICKS, PERIOD_TICKS, PLATFORM_HALF, STATIONS, TRAINS, station_x,
    train,
};
use bc_sim::colony::walkers::{Pose, RADIUS, Walker, each_walker, sitting};
use bc_sim::math::Rng;
use glam::Vec3;

const STAGE: Stage = Stage(0);
const MID: f32 = STRIP_WIDTH * 0.5;

fn people(k: u8, area: &Rect, t: u32, frac: f32) -> Vec<Walker> {
    let mut out = Vec::new();
    each_walker(k, area, STAGE, t, frac, |w| {
        out.push(*w);
        false
    });
    out
}

/// Places worth a look on strip `k`: Hub Gate's square, platforms, downtown, a plaza, a park, the
/// canal, a bank, and some anywhere.
fn places(k: u8, rng: &mut Rng) -> Vec<Rect> {
    let mut out = vec![
        Rect::new(MID - 300.0, MID + 300.0, -16_000.0, -15_700.0),
        Rect::new(MID - 300.0, MID + 300.0, -15_700.0, -15_350.0),
        Rect::new(0.0, 120.0, -14_000.0, -13_700.0),
        Rect::new(STRIP_WIDTH - 120.0, STRIP_WIDTH, -9_000.0, -8_700.0),
    ];
    for i in [0, 1, 4, 8, 10] {
        let x = station_x(i);
        out.push(Rect::new(MID - 45.0, MID + 45.0, x - 70.0, x + 70.0));
    }
    // Blocks of each kind, and their streets round them.
    let mut kinds = HashSet::new();
    for bx in CITY.0..=CITY.1 {
        for row in -ROWS..=ROWS {
            let Some(b) = block(k, bx, row, STAGE) else { continue };
            let kind = std::mem::discriminant(&b.kind);
            if kinds.insert((kind, row.abs() == 1)) || rng.next_u32().is_multiple_of(400) {
                let r = b.rect;
                out.push(Rect::new(r.s0 - 30.0, r.s1 + 30.0, r.x0 - 20.0, r.x1 + 20.0));
            }
        }
    }
    for _ in 0..30 {
        let (s, x) = (rng.next_f32() * STRIP_WIDTH, grid_x(HUB_GATE.0) + rng.next_f32() * 26_000.0);
        out.push(Rect::new(s - 80.0, s + 80.0, x - 80.0, x + 80.0));
    }
    out
}

/// Times worth a look: through a day (noon, the evening, the night), and a week on.
fn times(rng: &mut Rng) -> Vec<(u32, f32)> {
    (0..10)
        .map(|i| (i * (DAY_TICKS / 10) + rng.next_u32() % 5_000 + 600_000 * (i % 3), rng.next_f32()))
        .collect()
}

fn in_something(k: u8, w: &Walker) -> bool {
    if w.pose == Pose::Sit {
        return sitting(w).iter().any(|b| {
            solid(k, Vec3::new(b.rect.x0, b.h0, -b.rect.s1), Vec3::new(b.rect.x1, b.h1, -b.rect.s0), STAGE)
        });
    }
    solid(
        k,
        Vec3::new(w.x - RADIUS, w.h, -w.s - RADIUS),
        Vec3::new(w.x + RADIUS, w.h + 1.8, -w.s + RADIUS),
        STAGE,
    )
}

#[test]
fn nobody_stands_in_anything_or_anybody() {
    let mut rng = Rng::new(17);
    let mut seen = 0usize;
    let mut kinds = HashSet::new();
    for k in 0..3u8 {
        let areas = places(k, &mut rng);
        for (t, frac) in times(&mut rng) {
            for area in &areas {
                let ps = people(k, area, t, frac);
                seen += ps.len();
                for w in &ps {
                    kinds.insert(w.pose as u8);
                    assert!(w.fade > 0.0 && w.fade <= 1.0, "{w:?}");
                    assert!(!in_something(k, w), "strip {k} at {t}: somebody in something: {w:?}");
                }
                for (i, a) in ps.iter().enumerate() {
                    for b in &ps[i + 1..] {
                        let d = ((a.s - b.s).powi(2) + (a.x - b.x).powi(2)).sqrt();
                        assert!(d >= 2.0 * RADIUS - 1e-3, "strip {k} at {t}: {d} m apart: {a:?} {b:?}");
                        assert_ne!(a.id, b.id, "two people with one id");
                    }
                }
            }
        }
    }
    assert!(seen > 20_000, "only {seen} people seen");
    assert_eq!(kinds.len(), 4, "walking, running, standing and sitting: {kinds:?}");
}

/// Each tick for `ticks` from `t0` over `area`: nobody jumps, nor comes or goes but faded.
fn smooth(k: u8, area: &Rect, t0: u32, ticks: u32) {
    let inner = Rect::new(area.s0 + 2.0, area.s1 - 2.0, area.x0 + 2.0, area.x1 - 2.0);
    let mut last: HashMap<u32, Walker> = people(k, area, t0, 0.0).into_iter().map(|w| (w.id, w)).collect();
    for t in t0..t0 + ticks {
        // A tick's end is the next one's start.
        let end = people(k, area, t, 1.0);
        let next: HashMap<u32, Walker> = people(k, area, t + 1, 0.0).into_iter().map(|w| (w.id, w)).collect();
        assert_eq!(end.len(), next.len(), "at {t}");
        for w in &end {
            let n = next[&w.id];
            assert!(
                (w.s - n.s).abs() < 1e-3 && (w.x - n.x).abs() < 1e-3 && (w.fade - n.fade).abs() < 1e-3,
                "{w:?} {n:?}"
            );
        }
        for w in next.values() {
            let inside = inner.contains(w.s, w.x);
            match last.get(&w.id) {
                Some(o) => {
                    let d = ((w.s - o.s).powi(2) + (w.x - o.x).powi(2)).sqrt();
                    assert!(d < 0.12, "a jump of {d} m at {t}: {o:?} {w:?}");
                    assert!((w.h - o.h).abs() <= 0.21, "{o:?} {w:?}");
                    assert!((w.fade - o.fade).abs() < 0.12, "{o:?} {w:?}");
                }
                None if inside => assert!(w.fade < 0.12, "out of nowhere at {t}: {w:?}"),
                None => {}
            }
        }
        for o in last.values() {
            if !next.contains_key(&o.id) && inner.contains(o.s, o.x) {
                assert!(o.fade < 0.12, "gone all at once at {t}: {o:?}");
            }
        }
        last = next;
    }
}

#[test]
fn people_move_smoothly_and_come_and_go_slowly() {
    let mut rng = Rng::new(3);
    for k in 0..3u8 {
        let areas = places(k, &mut rng);
        for area in areas.iter().take(16) {
            smooth(k, area, rng.next_u32() % 2_000_000, 200);
        }
    }
}

#[test]
fn platforms_fill_and_empty_smoothly_at_their_busiest() {
    // A headway at noon at a station on each strip: the queues at their longest, the last to come
    // in their places before the doors open.
    let noon = 14_400 + 3 * DAY_TICKS;
    for (k, i) in [(0u8, 2usize), (1, 7), (2, 9)] {
        let x = station_x(i);
        smooth(k, &Rect::new(MID - 6.0, MID + 6.0, x - 50.0, x + 50.0), noon, HEADWAY_TICKS);
    }
}

#[test]
fn everybody_comes_round() {
    let mut rng = Rng::new(9);
    // Off the platforms, the same each colony day; on them, each week (a day and a headway).
    let week = DAY_TICKS * 7;
    assert_eq!(week % HEADWAY_TICKS, 0);
    assert_eq!(week % PERIOD_TICKS, 0);
    for k in 0..3u8 {
        for area in places(k, &mut rng).iter().step_by(3) {
            let t = rng.next_u32() % 3_000_000;
            let key = |v: Vec<Walker>| {
                let mut v: Vec<_> = v
                    .into_iter()
                    .map(|w| (w.id, w.s.to_bits(), w.x.to_bits(), w.yaw.to_bits(), w.fade.to_bits()))
                    .collect();
                v.sort();
                v
            };
            let off = |v: Vec<Walker>| v.into_iter().filter(|w| (w.s - MID).abs() > 5.0).collect::<Vec<_>>();
            assert_eq!(
                key(off(people(k, area, t, 0.4))),
                key(off(people(k, area, t + DAY_TICKS, 0.4))),
                "{area:?}"
            );
            assert_eq!(key(people(k, area, t, 0.4)), key(people(k, area, t + week, 0.4)), "{area:?}");
        }
    }
}

#[test]
fn tiles_find_everybody_once() {
    let area = Rect::new(MID - 200.0, MID + 200.0, -12_000.0, -11_600.0);
    let t = 123_456;
    let whole: HashSet<u32> = people(0, &area, t, 0.5).iter().map(|w| w.id).collect();
    let mut tiled = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            let s0 = area.s0 + 100.0 * i as f32;
            let x0 = area.x0 + 100.0 * j as f32;
            tiled.extend(people(0, &Rect::new(s0, s0 + 100.0, x0, x0 + 100.0), t, 0.5).iter().map(|w| w.id));
        }
    }
    assert_eq!(tiled.len(), whole.len());
    assert_eq!(tiled.into_iter().collect::<HashSet<_>>(), whole);
}

/// How many people are out in `area` at the hour `t`.
fn count(k: u8, area: &Rect, t: u32) -> usize {
    let mut n = 0;
    each_walker(k, area, STAGE, t, 0.0, |_| {
        n += 1;
        false
    });
    n
}

#[test]
fn busy_downtown_and_by_day_quiet_at_night_and_nobody_on_the_site() {
    // Tick 0 is mid-morning (8 minutes into the light): noon is 12 minutes on, the night's middle
    // 32.
    // The business district (strip 0, blocks 24..40), and a residential one (bx 88..104).
    let downtown = Rect::new(MID - 700.0, MID + 700.0, grid_x(26), grid_x(34));
    let homes = Rect::new(MID - 700.0, MID + 700.0, grid_x(90), grid_x(98));
    let (noon, night) = (14_400, 57_600);
    let (d, n) = (count(0, &downtown, noon), count(0, &downtown, night));
    assert!(d > 1_500 && d > 4 * n, "downtown {d} by day, {n} by night");
    let h = count(0, &homes, noon);
    assert!(h < d && h > 200, "the homes {h}, downtown {d}");
    let square = Rect::new(MID - 300.0, MID + 300.0, -16_000.0, -15_360.0);
    let sq = count(1, &square, noon);
    assert!(sq > 400, "Hub Gate's square: {sq}");
    let site = Rect::new(0.0, STRIP_WIDTH, grid_x(SITE.0) + 200.0, grid_x(SITE.1));
    assert_eq!(count(2, &site, noon), 0, "people on the building site");
}

#[test]
fn the_benches_seats_are_the_furnitures_and_people_sit_on_them() {
    let mut sat = 0;
    for k in 0..3u8 {
        for bx in [9, 30, 77, 150] {
            let area = Rect::new(MID - 40.0, MID + 40.0, grid_x(bx), grid_x(bx + 1));
            let mut benches = Vec::new();
            each_furniture(k, &area, STAGE, |p| {
                if p.kind == Kind::Bench {
                    benches.push(p.solid);
                }
                false
            });
            for t in (0..DAY_TICKS).step_by(1_801) {
                for w in people(k, &area, t, 0.0).iter().filter(|w| w.pose == Pose::Sit) {
                    let on = benches.iter().any(|b| {
                        let (s, x) = ((b.rect.s0 + b.rect.s1) * 0.5, (b.rect.x0 + b.rect.x1) * 0.5);
                        (w.s - s).abs() < 0.6 && (w.x - x).abs() < 0.95
                    });
                    assert!(on, "sitting on nothing: {w:?}");
                    sat += 1;
                }
            }
        }
    }
    assert!(sat > 50, "{sat} people sat");
}

#[test]
fn people_get_on_and_off_trains_through_their_open_doors() {
    let mut through = 0;
    for k in 0..3u8 {
        for i in 0..STATIONS {
            let x = station_x(i);
            let area = Rect::new(MID - 6.0, MID + 6.0, x - 50.0, x + 50.0);
            for t in 0..HEADWAY_TICKS * 3 {
                let t = t + 200_000;
                for w in people(k, &area, t, 0.0) {
                    // Clear of the screens along the platform's edge (their inner face).
                    let c = w.s - MID;
                    if c.abs() + RADIUS <= PLATFORM_HALF - 0.07 {
                        continue;
                    }
                    through += 1;
                    // Past the platform's edge: in a door of a train standing with its doors open.
                    let ok = (0..TRAINS as u8).map(|n| train(k, n, t, 0.0)).any(|tr| {
                        tr.doors
                            && tr.at == Some(i)
                            && (tr.s - MID).signum() == c.signum()
                            && c.abs() < (tr.s - MID).abs()
                            && (0..3).any(|car| {
                                [-DOOR_AT, DOOR_AT].iter().any(|d| {
                                    (w.x - (tr.car_x(car) + d)).abs() + RADIUS <= 0.5 * DOOR_WIDTH + 1e-3
                                })
                            })
                    });
                    assert!(ok, "strip {k} station {i} at {t}: past the edge with no open door: {w:?}");
                    assert!(c.abs() < PLATFORM_HALF + CAR_WIDTH, "{w:?}");
                }
            }
        }
    }
    assert!(through > 100, "{through}");
}

#[test]
fn people_wait_for_their_train_and_are_gone_after_it() {
    // Strip 1's station 4, its out-bound side: more waiting just before the doors open than just
    // after they've shut.
    use bc_sim::colony::transit::{DOOR_MARGIN, DWELL_TICKS, stands};
    let x = station_x(4);
    let area = Rect::new(MID + 0.2, MID + 3.0, x - 45.0, x + 45.0);
    let (mut before, mut after) = (0, 0);
    let at = stands(1, 4, 1.0).unwrap();
    for m in 10..30u32 {
        let open = at + DOOR_MARGIN + m * HEADWAY_TICKS;
        before += people(1, &area, open - 30, 0.0).iter().filter(|w| w.pose == Pose::Stand).count();
        after += people(1, &area, at + m * HEADWAY_TICKS + DWELL_TICKS + 60, 0.0)
            .iter()
            .filter(|w| w.pose == Pose::Stand)
            .count();
    }
    assert!(before > 40 && after == 0, "{before} waiting before, {after} after");
}

/// Every few ticks for a while over `area`: nobody in anything or anybody.
fn sweep(k: u8, area: &Rect, from: u32, ticks: u32, every: usize) -> usize {
    let mut seen = 0;
    for t in (from..from + ticks).step_by(every) {
        let ps = people(k, area, t, 0.5);
        seen += ps.len();
        for (i, a) in ps.iter().enumerate() {
            assert!(!in_something(k, a), "strip {k} at {t}: in something: {a:?}");
            for b in &ps[i + 1..] {
                let d = ((a.s - b.s).powi(2) + (a.x - b.x).powi(2)).sqrt();
                assert!(d >= 2.0 * RADIUS - 1e-3, "strip {k} at {t}: {d} m apart: {a:?} {b:?}");
            }
        }
    }
    seen
}

#[test]
fn sitting_down_and_getting_up_never_bump_anybody() {
    // A colony day on a few stretches of the avenue's pavements, every 4 ticks.
    let mut sat = 0;
    for (k, bx) in [(0u8, 30), (1, 64), (2, 100)] {
        for side in [-1.0f32, 1.0] {
            let (a, b) = (MID + side * 23.0, MID + side * 33.0);
            let area = Rect::new(a.min(b), a.max(b), grid_x(bx), grid_x(bx + 1));
            sweep(k, &area, 0, DAY_TICKS, 4);
            sat += people(k, &area, 30_000, 0.0).iter().filter(|w| w.pose == Pose::Sit).count();
        }
    }
    assert!(sat > 0);
}

#[test]
fn platforms_never_crowd() {
    // Three headways at noon at four stations, every 3 ticks: the queues filling (their longest),
    // getting on, getting off.
    let noon = 14_400 + 2 * DAY_TICKS;
    assert!((bc_sim::colony::time::day(noon, 0.0).phase - 0.417).abs() < 1e-3);
    for (k, i) in [(0u8, 0usize), (0, 5), (1, 10), (2, 3)] {
        let x = station_x(i);
        let area = Rect::new(MID - 5.0, MID + 5.0, x - 48.0, x + 48.0);
        let seen = sweep(k, &area, noon, HEADWAY_TICKS * 3, 3);
        assert!(seen > 2_000, "{seen}");
    }
}

/// How many: per strip at noon, in the evening and at night; per few-hundred-metre query, and what
/// a query costs (`cargo test --release --test walkers -- --ignored --nocapture`).
#[test]
#[ignore]
fn how_many() {
    use std::time::Instant;
    for (name, t) in [("noon", 14_400u32), ("evening", 43_200 + 7_200), ("night", 57_600)] {
        for k in 0..3u8 {
            let (mut n, mut sit, mut stand, mut run) = (0usize, 0, 0, 0);
            let mut x = grid_x(HUB_GATE.0);
            while x < grid_x(SITE.1 + 1) {
                let mut s = 0.0;
                while s < STRIP_WIDTH {
                    each_walker(k, &Rect::new(s, s + 500.0, x, x + 500.0), STAGE, t, 0.0, |w| {
                        n += 1;
                        match w.pose {
                            Pose::Sit => sit += 1,
                            Pose::Stand => stand += 1,
                            Pose::Run => run += 1,
                            Pose::Walk => {}
                        }
                        false
                    });
                    s += 500.0;
                }
                x += 500.0;
            }
            println!("{name}: strip {k}: {n} people ({stand} standing, {sit} sitting, {run} running)");
        }
    }
    let places = [
        ("Hub Gate's square", 0u8, Rect::new(MID - 150.0, MID + 150.0, -15_800.0, -15_500.0)),
        (
            "downtown (business, bx 30)",
            0,
            Rect::new(MID - 150.0, MID + 150.0, grid_x(30) - 150.0, grid_x(30) + 150.0),
        ),
        ("midtown, off the avenue", 0, Rect::new(MID + 300.0, MID + 600.0, grid_x(60), grid_x(60) + 300.0)),
        (
            "residential (Canal strip)",
            1,
            Rect::new(MID - 600.0, MID - 300.0, grid_x(100), grid_x(100) + 300.0),
        ),
        (
            "a station (strip 2, #5)",
            2,
            Rect::new(MID - 150.0, MID + 150.0, station_x(5) - 150.0, station_x(5) + 150.0),
        ),
        (
            "the works (strip 0, bx 180)",
            0,
            Rect::new(MID - 150.0, MID + 150.0, grid_x(180), grid_x(180) + 300.0),
        ),
    ];
    for (name, k, area) in places {
        for (hour, t) in [("noon", 14_400u32), ("night", 57_600)] {
            let start = Instant::now();
            let mut n = 0;
            for i in 0..200u32 {
                n = 0;
                each_walker(k, &area, STAGE, t + i, 0.5, |_| {
                    n += 1;
                    false
                });
            }
            let each = start.elapsed().as_secs_f64() / 200.0 * 1e6;
            println!("{name} at {hour}: {n} people in 300 m x 300 m, {each:.0} us a query");
        }
    }
}

#[test]
fn people_cross_the_narrow_streets_on_their_zebras_and_no_other_road() {
    use bc_sim::colony::city::{AVENUE, STREET, block_index, block_rect, cross_width, row_at};
    let mut crossing = 0;
    let mut rng = Rng::new(21);
    for k in 0..3u8 {
        for _ in 0..60 {
            let (s, x) = (rng.next_f32() * STRIP_WIDTH, grid_x(10) + rng.next_f32() * 24_000.0);
            let area = Rect::new(s - 150.0, s + 150.0, x - 150.0, x + 150.0);
            for w in people(k, &area, rng.next_u32() % 1_000_000, 0.5) {
                let row = row_at(w.s);
                let c = (w.s - MID).abs();
                // On a block, the avenue's pavements or median, a bank: not a road.
                let bx = block_index(w.x);
                let on_block = (bx..=bx + 1)
                    .any(|b| block(k, b, row, STAGE).is_some_and(|bl| bl.rect.contains(w.s, w.x)));
                let on_avenue = row == 0 && (c >= AVENUE * 0.5 - 18.0 || c <= 3.0);
                if on_block || on_avenue || row.abs() == 13 || w.x < grid_x(8) {
                    continue;
                }
                // Anywhere else, it's a narrow cross street's zebra in a row off the avenue.
                let g = ((w.x - grid_x(0)) / 128.0 + 0.5).floor() as i32;
                assert_eq!(cross_width(g), STREET, "on a road that isn't a narrow cross street: {w:?}");
                let r = block_rect(g, row);
                let from_lane = (w.s - r.s0).min(r.s1 - w.s);
                assert!((0.5..=4.5).contains(&from_lane), "off the zebra: {from_lane} m in: {w:?}");
                assert!(row.abs() >= 2, "across a cross street by the avenue: {w:?}");
                // On the street, but for a step off the kerb (its top while a foot's still on it).
                let off_kerb = 0.5 * cross_width(g) - (w.x - grid_x(g)).abs();
                assert_eq!(w.h, if off_kerb >= RADIUS { 0.0 } else { bc_sim::colony::city::KERB }, "{w:?}");
                crossing += 1;
            }
        }
    }
    assert!(crossing > 100, "only {crossing} crossing");
}
