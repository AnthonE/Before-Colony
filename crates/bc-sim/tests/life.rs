//! The city's two closed forms of life together: nobody the walkers have out is ever touched by a
//! car the traffic has out (the people cross only the narrow cross streets, which no car drives).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_sim::colony::city::{self, AVENUE, BLOCK, Rect, Stage};
use bc_sim::colony::frame::STRIP_WIDTH;
use bc_sim::colony::time::day;
use bc_sim::colony::{traffic, walkers};

/// How far a person's middle is from a car's footprint (negative inside).
fn gap(c: &traffic::Car, s: f32, x: f32) -> f32 {
    let (l, w, _) = c.kind.size();
    let (ds, dx) = (s - c.s, x - c.x);
    let along = ds * c.dir.0 + dx * c.dir.1;
    let across = -ds * c.dir.1 + dx * c.dir.0;
    let (qa, qc) = (along.abs() - 0.5 * l, across.abs() - 0.5 * w);
    let out = (qa.max(0.0).powi(2) + qc.max(0.0).powi(2)).sqrt();
    out + qa.max(qc).min(0.0)
}

fn areas() -> Vec<(u8, Rect)> {
    let mid = STRIP_WIDTH * 0.5;
    let near = |strip: u8, bx: i32, s0: f32, s1: f32| {
        let x = city::grid_x(bx);
        (strip, Rect::new(s0, s1, x - 140.0, x + 140.0))
    };
    // Strip 0: a wide cross street's junctions with the avenue and rows 1 to 3 either side, two
    // narrow ones', the canal's rows, the rows' ends by Hub Gate and the bank road.
    let mut v: Vec<_> = [
        (40, mid - 420.0, mid + 420.0),
        (41, mid + 100.0, mid + 520.0),
        (62, mid - 520.0, mid - 100.0),
        (100, mid + 420.0, mid + 700.0),
        (8, mid - 300.0, mid + 300.0),
        (150, STRIP_WIDTH - 400.0, STRIP_WIDTH),
    ]
    .into_iter()
    .map(|(bx, s0, s1)| near(0, bx, s0, s1))
    .collect();
    // The others: the wide junctions and the bank road.
    for strip in 1..3 {
        v.push(near(strip, 40, mid - 420.0, mid + 420.0));
        v.push(near(strip, 150, STRIP_WIDTH - 400.0, STRIP_WIDTH));
    }
    // The city's last blocks and the building site's first, rows 1 to 3 either side: the rows'
    // traffic runs on to bx 200, past the last of the people (nobody walks the site).
    let rows = AVENUE * 0.5 + 3.0 * BLOCK;
    v.push((0, Rect::new(mid - rows, mid + rows, city::grid_x(196), city::grid_x(204))));
    v
}

#[test]
fn no_car_ever_touches_anybody() {
    let mut worst = f32::MAX;
    let mut closest = None;
    let (mut people, mut pairs) = (0u64, 0u64);
    // Noon, the evening's rush as the dusk starts, the night as the platoons park, and the dawn as
    // they pull out (tick 0 is 8 minutes into the day's light, at phase 0.25).
    for (t0, phase) in [(14_400u32, 0.417), (43_200, 0.75), (53_568, 0.87), (69_120, 0.05)] {
        assert!((day(t0, 0.0).phase - phase).abs() < 1e-3, "tick {t0} isn't phase {phase}");
        // A cycle and a half of the signals, every 6 ticks.
        for (strip, area) in areas() {
            for t in (t0..t0 + 3 * traffic::CYCLE / 2).step_by(6) {
                let mut cars = Vec::new();
                traffic::each_car(strip, &area.inset(-6.0), Stage(0), t, 0.5, |c| {
                    cars.push((*c, c.bounds().inset(-6.0)));
                    false
                });
                walkers::each_walker(strip, &area, Stage(0), t, 0.5, |w| {
                    people += 1;
                    // Only the cars within a few metres need measuring.
                    for (c, near) in &cars {
                        if !near.contains(w.s, w.x) {
                            continue;
                        }
                        let g = gap(c, w.s, w.x);
                        if g < 3.0 {
                            pairs += 1;
                        }
                        if g < worst {
                            worst = g;
                            closest = Some((t, *w, *c));
                        }
                        assert!(
                            g >= walkers::RADIUS + 0.2,
                            "strip {strip} t {t}: {w:?} is {g:.2} m from {c:?}"
                        );
                    }
                    false
                });
            }
        }
    }
    println!("people seen {people}, within 3 m of a car {pairs}, closest {worst:.2} m: {closest:?}");
    assert!(people > 100_000, "{people}");
}
