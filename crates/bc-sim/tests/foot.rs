//! The walker (`bc_sim::foot`), shared by the tick and the pilot's own prediction: it stands without
//! drifting, stops at walls, steps up kerbs, benches and stairs, jumps as high as the colony's
//! gravity lets it and lands from a fall; and a tick is exactly four substeps.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_sim::DT;
use bc_sim::colony::city::{self, KERB, Rect, Stage, block, block_rect, lots, place, place_door};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH};
use bc_sim::colony::furniture::{AVENUE_TREE, BENCH_HEIGHT, Furniture, Kind, each_furniture};
use bc_sim::colony::homes;
use bc_sim::foot::{CityGround, SUBSTEP, Stride, Walker};
use bc_sim::math::atan2;
use glam::Vec3;

const CITY: CityGround = CityGround { strip: 0, stage: Stage(0) };
const FORWARD: Stride = Stride { forward: 1.0, right: 0.0, run: false, jump: false };

/// A walker put down at `(s, x, h)` on strip 0 and let settle for three seconds.
fn at(s: f32, x: f32, h: f32, facing: Vec3) -> Walker {
    let mut w = Walker::at(CityPos::new(0, x, s, h).walker(), facing);
    for _ in 0..90 {
        w.tick(&CITY, &Stride::default());
    }
    w
}

fn walk(w: &mut Walker, secs: f32, s: Stride) {
    for _ in 0..(secs / DT) as u32 {
        w.tick(&CITY, &s);
    }
}

/// Walks `w` through `points` (`(s, x)` and the floor there) on `ground`, facing each in turn:
/// whether it got to the last.
fn through(ground: &CityGround, w: &mut Walker, points: &[((f32, f32), f32)], most: u32) -> bool {
    let mut next = 0;
    for _ in 0..most {
        let Some(&((s, x), h)) = points.get(next) else { return true };
        let to = CityPos::new(ground.strip, x, s, h).walker();
        let flat = Vec3::new(to.x - w.feet.x, 0.0, to.z - w.feet.z);
        if flat.length() < 0.35 && (to.y - w.feet.y).abs() < 1.0 {
            next += 1;
            continue;
        }
        w.yaw = atan2(flat.x, flat.z);
        w.tick(ground, &FORWARD);
    }
    next == points.len()
}

/// Strip 0's furniture on `area` that `pick` likes, along `x`.
fn furniture(area: Rect, pick: impl Fn(&Furniture) -> bool) -> Vec<Furniture> {
    let mut out = Vec::new();
    each_furniture(0, &area, Stage(0), |p| {
        if pick(p) {
            out.push(*p);
        }
        false
    });
    out.sort_by(|a, b| a.x.total_cmp(&b.x));
    out
}

/// The first block of flats on strip `strip` with at least `storeys` storeys.
fn flats(strip: u8, storeys: u32) -> homes::Plan {
    for bx in city::CITY.0..city::CITY.1 {
        for row in 1..=city::ROWS {
            let Some(b) = block(strip, bx, row, Stage(0)) else { continue };
            for bd in lots(&b).as_slice() {
                if let Some(p) = homes::plan(&b, bd)
                    && p.kind == homes::Kind::Flats
                    && p.storeys >= storeys
                    && p.sides != 0
                {
                    return p;
                }
            }
        }
    }
    panic!("no flats of {storeys} storeys on strip {strip}");
}

#[test]
fn a_walker_stands_on_the_avenue_without_drifting() {
    let ((s, x), _) = place_door(place("hub_gate_1").unwrap().1);
    let mut w = at(s, x, 0.5, Vec3::X);
    assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
    let still = w;
    for _ in 0..300 {
        w.tick(&CITY, &Stride::default());
    }
    assert_eq!(w.feet, still.feet, "no drift");
    assert!(w.grounded);
}

#[test]
fn a_walker_steps_up_a_kerb_and_stops_at_a_wall() {
    let b = block(0, 20, -3, Stage(0)).unwrap();
    let all = lots(&b);
    let building = all.as_slice().iter().max_by(|a, c| a.foot.s1.total_cmp(&c.foot.s1)).unwrap();
    let (_, mx) = building.foot.middle();
    let mut w = at(b.rect.s1 + 8.0, mx, 0.2, Vec3::Z);
    walk(&mut w, 8.0, FORWARD);
    let p = CITY.place(w.feet);
    assert!((p.h - KERB).abs() < 1e-3, "up on the pavement: {p:?}");
    assert!(
        p.s > building.foot.s1 && p.s < building.foot.s1 + 0.5,
        "at the wall: {p:?} vs {:?}",
        building.foot
    );
    assert_eq!(w.vel.z, 0.0, "stopped");
}

#[test]
fn a_walker_steps_up_onto_a_bench_and_off_it() {
    let r = block_rect(30, 1);
    let s = STRIP_WIDTH * 0.5 + AVENUE_TREE;
    let row = Rect::new(s - 0.5, s + 0.5, r.x0, r.x1);
    let bench = furniture(row, |p| p.kind == Kind::Bench)[0];
    let trunk = furniture(row, |p| p.kind == Kind::Tree && p.x > bench.x)[0];
    let mut w = at(s, bench.x - 3.0, 0.2, Vec3::X);
    let mut top: f32 = 0.0;
    for _ in 0..(4.0 / DT) as u32 {
        w.tick(&CITY, &FORWARD);
        top = top.max(w.feet.y);
    }
    let p = CITY.place(w.feet);
    assert!((top - BENCH_HEIGHT).abs() < 0.02, "up on the bench: {top}");
    assert!(w.grounded && p.h.abs() < 1e-3, "and down off it: {p:?}");
    assert!((trunk.x - p.x - 0.5).abs() < 0.02, "stopped at the trunk: {p:?} vs {trunk:?}");
}

#[test]
fn a_walker_climbs_a_block_of_flats_stairs() {
    let p = flats(0, 4);
    let mut way = vec![
        (p.point(0.0, -homes::SHELL - 1.2), KERB),
        (p.point(0.0, 0.8), KERB),
        (p.point(0.0, homes::NEAR * 0.5), KERB),
    ];
    let ((s, x), h) = way[0];
    let mut w = Walker::at(CityPos::new(0, x, s, h + 0.05).walker(), Vec3::X);
    for k in 0..p.storeys - 1 {
        way.extend(p.stairs_up(k, 0));
    }
    assert!(through(&CITY, &mut w, &way, 30 * 30 * p.storeys), "let go at {:?}", CITY.place(w.feet));
    let top = homes::level(p.storeys - 1);
    assert!((CITY.place(w.feet).h - top).abs() < 0.05, "on the top storey: {:?}", CITY.place(w.feet));
}

#[test]
fn a_jump_peaks_as_the_colony_s_gravity_says() {
    let ((s, x), _) = place_door(place("hub_gate_1").unwrap().1);
    let mut w = at(s, x, 0.5, Vec3::X);
    w.tick(&CITY, &Stride { jump: true, ..Stride::default() });
    let mut top: f32 = 0.0;
    for _ in 0..60 {
        w.tick(&CITY, &Stride::default());
        top = top.max(w.feet.y);
    }
    // v² / 2g at 1 g: 0.66 m.
    assert!(top > 0.6 && top < 0.7, "jumped {top} m");
    assert!(w.grounded && w.feet.y.abs() < 1e-3, "and landed: {:?}", w.feet);
}

#[test]
fn a_fall_lands_on_the_street() {
    let ((s, x), _) = place_door(place("hub_gate_1").unwrap().1);
    let mut w = Walker::at(CityPos::new(0, x, s, 12.0).walker(), Vec3::X);
    let mut fell = false;
    for _ in 0..120 {
        w.tick(&CITY, &Stride::default());
        fell |= w.vel.y < -10.0;
    }
    assert!(fell, "it fell");
    assert!(w.grounded && w.feet.y.abs() < 1e-3 && w.vel.y == 0.0, "{w:?}");
}

#[test]
fn a_tick_is_four_substeps_to_the_bit() {
    let ((s, x), _) = place_door(place("hub_gate_1").unwrap().1);
    let mut a = Walker::at(CityPos::new(0, x, s, 0.4).walker(), Vec3::X);
    let mut b = a;
    for n in 0..600u32 {
        let st = Stride {
            forward: [1.0, 0.5, -0.3, 0.0][(n / 40 % 4) as usize],
            right: [0.0, 0.7, -1.0][(n / 70 % 3) as usize],
            run: n % 200 > 120,
            jump: n.is_multiple_of(90),
        };
        if n.is_multiple_of(50) {
            a.turn(0.4, 0.05);
            b.turn(0.4, 0.05);
        }
        a.tick(&CITY, &st);
        for _ in 0..4 {
            b.step(&CITY, &st, SUBSTEP);
        }
        assert_eq!(
            (a.feet.to_array().map(f32::to_bits), a.vel.to_array().map(f32::to_bits), a.grounded),
            (b.feet.to_array().map(f32::to_bits), b.vel.to_array().map(f32::to_bits), b.grounded),
            "tick {n}"
        );
    }
    assert!(CITY.place(a.feet).x != x, "it went somewhere");
}
