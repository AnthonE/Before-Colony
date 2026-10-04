//! The city's street furniture (`colony::furniture`): its lamps, trees and benches stand on the
//! pavements, in nothing built, clear of the crossings, the doors and the walks, evenly down every
//! kerb where the ground's paint has them; people bump into them and a suit steps over them; and
//! any area asked finds each piece once.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::collections::HashSet;

use bc_sim::colony::city::{
    self, AVENUE, BANK_ROW, BlockInfo, BlockKind, CANAL_ROW, CITY, FAR_FOOT, HUB_GATE, KERB, RAIL_THICKNESS,
    ROWS, Rect, SIDEWALK, SITE, Stage, block, block_index, block_rect, channel, grid_x, has_block, lots,
    place_door, row_at, row_span, solid, solid_built,
};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH};
use bc_sim::colony::furniture::{self as f, Furniture, Kind, each_furniture};
use bc_sim::colony::interior::ground_under;
use bc_sim::content::city::{DistrictKind, PLACES};
use bc_sim::math::Rng;
use glam::Vec3;

const STAGE: Stage = Stage(0);
const MID: f32 = STRIP_WIDTH * 0.5;

/// Every piece of strip `k` standing in blocks `b0..=b1`, each once (by where it stands).
fn pieces(k: u8, b0: i32, b1: i32) -> Vec<Furniture> {
    let area = Rect::new(0.0, STRIP_WIDTH, grid_x(b0), grid_x(b1 + 1));
    let mut out = Vec::new();
    each_furniture(k, &area, STAGE, |p| {
        if area.x0 <= p.x && p.x < area.x1 {
            out.push(*p);
        }
        false
    });
    out
}

/// Every piece of strip `k` whose solid overlaps `area`, along `x`.
fn pieces_in(k: u8, area: Rect) -> Vec<Furniture> {
    let mut out = Vec::new();
    each_furniture(k, &area, STAGE, |p| {
        out.push(*p);
        false
    });
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.s.total_cmp(&b.s)));
    out
}

/// A walker's box (0.6 × 1.8 m) at `(s, x)`, its feet at `h`.
fn walker(k: u8, s: f32, x: f32, h: f32) -> bool {
    solid(k, Vec3::new(x - 0.3, h, -s - 0.3), Vec3::new(x + 0.3, h + 1.8, -s + 0.3), STAGE)
}

/// `a..b` from `edge` the way `d` points, as a span lowest first.
fn span(edge: f32, d: f32, a: f32, b: f32) -> (f32, f32) {
    let (u, v) = (edge + d * a, edge + d * b);
    (u.min(v), u.max(v))
}

/// The gap between two footprints, m.
fn gap(a: &Rect, b: &Rect) -> f32 {
    let ds = (a.s0 - b.s1).max(b.s0 - a.s1).max(0.0);
    let dx = (a.x0 - b.x1).max(b.x0 - a.x1).max(0.0);
    (ds * ds + dx * dx).sqrt()
}

fn near(got: f32, want: f32, what: &str) {
    assert!((got - want).abs() < 1e-3, "{what}: {got}, not {want}");
}

#[test]
fn furniture_stands_on_the_pavements_and_in_nothing_built() {
    let mut count = [0usize; 6];
    for k in 0..3u8 {
        for p in pieces(k, HUB_GATE.0, SITE.1) {
            let (b, r) = (p.solid, p.solid.rect);
            // In nothing built, and on something.
            let inside = (Vec3::new(r.x0, b.h0 + 0.01, -r.s1), Vec3::new(r.x1, b.h1, -r.s0));
            assert!(!solid_built(k, inside.0, inside.1, STAGE), "in something built: {p:?}");
            let under = (Vec3::new(r.x0, b.h0 - 0.05, -r.s1), Vec3::new(r.x1, b.h0 - 0.005, -r.s0));
            assert!(solid_built(k, under.0, under.1, STAGE), "on nothing: {p:?}");
            assert_eq!(p.h, city::ground(k, p.s, p.x, STAGE), "not on the ground: {p:?}");
            assert_eq!(b.h0, p.h, "{p:?}");
            // Off every road.
            let c = (p.s - MID).abs();
            let row = row_at(p.s);
            match row.abs() {
                0 => assert!(
                    c - 0.3 >= f::ROAD_OUT && c + 0.3 <= AVENUE * 0.5,
                    "off the avenue's pavements: {p:?}"
                ),
                BANK_ROW => assert!(c > row_span(BANK_ROW).0 - MID, "on the bank road: {p:?}"),
                _ => {
                    let bl =
                        block(k, block_index(p.x), row, STAGE).unwrap_or_else(|| panic!("no block: {p:?}"));
                    assert!(bl.rect.inset(0.05).holds(&r), "off its block: {p:?}");
                    if bl.kind == BlockKind::Canal {
                        let water = channel(&bl.rect).inset(-RAIL_THICKNESS - 0.3);
                        assert!(!water.overlaps(&r), "at the water: {p:?}");
                    }
                }
            }
            count[p.kind as usize] += 1;
        }
    }
    assert!(count.iter().all(|&n| n > 0), "every kind: {count:?}");
    assert!(count[Kind::StreetLamp as usize] > 50_000, "{count:?}");
}

#[test]
fn pieces_keep_a_walkers_width_apart() {
    // Business by the avenue, Old Town, the canal, Central Park, Hub Gate's offices, two of the
    // site's stretches.
    for (k, b0, b1) in
        [(0, 24, 30), (0, 136, 140), (1, 40, 46), (0, 104, 110), (2, 3, 9), (1, 150, 154), (2, 230, 236)]
    {
        let mut ps = pieces(k, b0, b1);
        assert!(ps.len() > 200, "{k} {b0}..{b1}: {} pieces", ps.len());
        ps.sort_by(|a, b| a.solid.rect.x0.total_cmp(&b.solid.rect.x0));
        for (i, a) in ps.iter().enumerate() {
            for b in &ps[i + 1..] {
                if b.solid.rect.x0 - a.solid.rect.x1 >= 0.7 {
                    break;
                }
                let d = gap(&a.solid.rect, &b.solid.rect);
                assert!(d >= 0.7, "{d} m between {a:?} and {b:?}");
            }
        }
    }
}

#[test]
fn crossings_doors_and_walks_are_clear() {
    let none = |k: u8, zone: Rect| !each_furniture(k, &zone, STAGE, |_| true);
    for k in 0..3u8 {
        for bx in HUB_GATE.0..=SITE.1 {
            for row in -ROWS..=ROWS {
                let Some(b) = block(k, bx, row, STAGE) else { continue };
                let r = b.rect;
                // At each corner, where the lane's crossing and the cross street's land on its
                // pavement.
                for (s_edge, ds) in [(r.s0, 1.0f32), (r.s1, -1.0)] {
                    for (x_edge, dx) in [(r.x0, 1.0f32), (r.x1, -1.0)] {
                        let ((s0, s1), (x0, x1)) =
                            (span(s_edge, ds, 0.0, SIDEWALK), span(x_edge, dx, 0.5, 4.5));
                        assert!(
                            none(k, Rect::new(s0, s1, x0, x1)),
                            "in the lane's crossing at {s_edge} {x_edge}: {b:?}"
                        );
                        let ((s0, s1), (x0, x1)) =
                            (span(s_edge, ds, 0.5, 4.5), span(x_edge, dx, 0.0, SIDEWALK));
                        assert!(
                            none(k, Rect::new(s0, s1, x0, x1)),
                            "in the cross street's at {s_edge} {x_edge}: {b:?}"
                        );
                    }
                }
                // Down the middle of each side's pavement (but a canal block's cross sides: its
                // railings).
                let h = KERB + 0.01;
                let mut x = r.x0 + 1.0;
                while x < r.x1 - 1.0 {
                    for s in [r.s0 + SIDEWALK * 0.5, r.s1 - SIDEWALK * 0.5] {
                        assert!(!walker(k, s, x, h), "a walker's stopped at {k} {s} {x}: {b:?}");
                    }
                    x += 2.0;
                }
                if b.kind != BlockKind::Canal {
                    let mut s = r.s0 + 1.0;
                    while s < r.s1 - 1.0 {
                        for x in [r.x0 + SIDEWALK * 0.5, r.x1 - SIDEWALK * 0.5] {
                            assert!(!walker(k, s, x, h), "a walker's stopped at {k} {s} {x}: {b:?}");
                        }
                        s += 2.0;
                    }
                }
            }
        }
        // The avenue's walks, and its carriageways' crossings where they land.
        let mut x = grid_x(CITY.0);
        while x < grid_x(FAR_FOOT.0) {
            for side in [-1.0f32, 1.0] {
                assert!(
                    !walker(k, MID + side * 28.0, x, 0.01),
                    "a walker's stopped on the avenue at {k} {side} {x}"
                );
            }
            x += 2.0;
        }
        for bx in CITY.0..FAR_FOOT.0 {
            let r = block_rect(bx, 1);
            for (x_edge, dx) in [(r.x0, 1.0f32), (r.x1, -1.0)] {
                for side in [-1.0f32, 1.0] {
                    let (s0, s1) = span(MID, side, f::ROAD_OUT, f::ROAD_OUT + 5.0);
                    let (x0, x1) = span(x_edge, dx, 0.5, 4.5);
                    assert!(none(k, Rect::new(s0, s1, x0, x1)), "in the avenue's crossing: {k} {bx} {side}");
                }
            }
        }
    }
    // Nothing in a key place's doorway.
    for p in &PLACES {
        let ((s, x), _) = place_door(p);
        let d = f::DOOR_CLEAR;
        each_furniture(p.strip, &Rect::new(s - d, s + d, x - d, x + d), STAGE, |q| {
            let far = ((q.s - s) * (q.s - s) + (q.x - x) * (q.x - x)).sqrt();
            assert!(far > d, "{q:?} stands {far} m from {}'s door", p.name);
            false
        });
    }
}

/// The first block in the city's stretch, any strip, that `pick` likes.
fn find(pick: impl Fn(&BlockInfo) -> bool) -> BlockInfo {
    (0..3u8)
        .flat_map(|k| {
            (CITY.0..=CITY.1)
                .flat_map(move |bx| (-ROWS..=ROWS).filter_map(move |row| block(k, bx, row, STAGE)))
        })
        .find(|b| pick(b))
        .expect("no such block")
}

/// A block's kerbs' lamps: corner to corner down its lanes (but row ±1's avenue side), between the
/// corners down its cross streets (none in the canal's row), each lantern out over the street.
fn kerb_lamps(b: &BlockInfo, got: &[Furniture]) {
    let r = b.rect;
    let lamps = || got.iter().filter(|p| p.kind == Kind::StreetLamp);
    let (len, n) = (r.length(), f::lamp_count(r.length()));
    for (edge, out) in [(r.s0, -1.0f32), (r.s1, 1.0)] {
        let side: Vec<_> = lamps().filter(|p| p.facing == (out, 0.0)).collect();
        if b.row.abs() == 1 && (b.row > 0) == (out < 0.0) {
            assert!(side.is_empty(), "lamps on the avenue's side of {b:?}");
            continue;
        }
        assert_eq!(side.len() as u32, n + 1, "down a lane of {b:?}");
        for (i, p) in side.iter().enumerate() {
            let inset = if i == 0 || i == n as usize { f::LAMP_CORNER } else { f::LAMP_IN };
            near(p.s, edge - out * inset, "in from the kerb");
            let along = (len * i as f32 / n as f32).clamp(f::LAMP_CORNER, len - f::LAMP_CORNER);
            near(p.x, r.x0 + along, "along the kerb");
            let (ls, lx, lh) = p.lantern().unwrap();
            near(ls, edge + out * f::LAMP_OUT, "the lantern over the street");
            near(lx, p.x, "the lantern over the street");
            near(lh, KERB + Kind::StreetLamp.size().2, "the lantern's height");
        }
        for w in side[1..n as usize].windows(2) {
            near(w[1].x - w[0].x, len / n as f32, "evenly");
        }
    }
    let (w, n) = (r.width(), f::lamp_count(r.width()));
    for (edge, out) in [(r.x0, -1.0f32), (r.x1, 1.0)] {
        let mut side: Vec<_> = lamps().filter(|p| p.facing == (0.0, out)).collect();
        side.sort_by(|a, b| a.s.total_cmp(&b.s));
        if b.kind == BlockKind::Canal {
            assert!(side.is_empty(), "lamps over the canal by {b:?}");
            continue;
        }
        assert_eq!(side.len() as u32, n - 1, "down a cross street of {b:?}");
        for (i, p) in side.iter().enumerate() {
            near(p.s, r.s0 + w * (i + 1) as f32 / n as f32, "along the kerb");
            near(p.x, edge - out * f::LAMP_IN, "in from the kerb");
            let (ls, lx, _) = p.lantern().unwrap();
            near(lx, edge + out * f::LAMP_OUT, "the lantern over the street");
            near(ls, p.s, "the lantern over the street");
        }
    }
}

#[test]
fn lamps_stand_evenly_down_every_kerb() {
    let business = find(|b| {
        b.kind == BlockKind::Buildings
            && b.district.is_some_and(|(_, d)| d == DistrictKind::Business)
            && (2..ROWS).contains(&b.row.abs())
            && b.row != CANAL_ROW
    });
    let by_avenue = find(|b| b.kind == BlockKind::Buildings && b.row == 1);
    let canal = find(|b| b.kind == BlockKind::Canal);
    let park = find(|b| b.kind == BlockKind::Park && b.row.abs() > 1);
    let plaza = find(|b| b.kind == BlockKind::Plaza);
    for b in [business, by_avenue, canal, park, plaza] {
        let got = pieces_in(b.strip, b.rect);
        kerb_lamps(&b, &got);
        let others = |kind: Kind| got.iter().filter(move |p| p.kind == kind);
        // Path and plaza lamps hold their lanterns on top.
        for p in got.iter().filter(|p| p.kind == Kind::PathLamp || p.kind == Kind::PlazaLamp) {
            assert_eq!(p.lantern(), Some((p.s, p.x, KERB + 4.0)), "{p:?}");
        }
        match b.kind {
            BlockKind::Plaza => {
                let (ms, mx) = b.rect.middle();
                assert_eq!(others(Kind::PlazaLamp).count(), f::PLAZA_LAMPS, "round {b:?}");
                for p in others(Kind::PlazaLamp) {
                    let r = ((p.s - ms) * (p.s - ms) + (p.x - mx) * (p.x - mx)).sqrt();
                    near(r, f::PLAZA_RING, "on the plaza's ring");
                }
            }
            BlockKind::Park => {
                // Beside the loop, between its corners, every one: the pavilions stand inside them.
                let lr = b.rect.inset(SIDEWALK + f::PARK_LOOP);
                let (nl, nw) = (f::lamp_count(lr.length()), f::lamp_count(lr.width()));
                let mut want = Vec::new();
                for i in 1..nl {
                    let x = lr.x0 + lr.length() * i as f32 / nl as f32;
                    want.extend([(lr.s0 + f::PARK_LAMP, x), (lr.s1 - f::PARK_LAMP, x)]);
                }
                for i in 1..nw {
                    let s = lr.s0 + lr.width() * i as f32 / nw as f32;
                    want.extend([(s, lr.x0 + f::PARK_LAMP), (s, lr.x1 - f::PARK_LAMP)]);
                }
                assert_eq!(want.len() as u32, 2 * (nl - 1) + 2 * (nw - 1));
                for &(s, x) in &want {
                    let here =
                        others(Kind::PathLamp).any(|p| (p.s - s).abs() < 1e-3 && (p.x - x).abs() < 1e-3);
                    assert!(here, "no lamp at {s} {x} in {b:?}");
                }
                assert_eq!(
                    others(Kind::PathLamp).count(),
                    want.len(),
                    "lamps nowhere they should be in {b:?}"
                );
            }
            BlockKind::Canal => {
                // Lamps by the water corner to corner, and a row of trees, on each quay.
                let (water, _) = channel(&b.rect).middle();
                let (len, n) = (b.rect.length(), f::lamp_count(b.rect.length()));
                for side in [-1.0f32, 1.0] {
                    let s = water + side * (city::CANAL_WIDTH * 0.5 + f::QUAY_LAMP);
                    let lamps: Vec<_> = others(Kind::PathLamp).filter(|p| p.s == s).collect();
                    assert_eq!(lamps.len() as u32, n + 1, "down a quay of {b:?}");
                    for (i, p) in lamps.iter().enumerate() {
                        let along = (len * i as f32 / n as f32).clamp(f::LAMP_CORNER, len - f::LAMP_CORNER);
                        near(p.x, b.rect.x0 + along, "along the quay");
                    }
                    let s = water + side * (city::CANAL_WIDTH * 0.5 + f::QUAY_TREE);
                    let trees = others(Kind::Tree).filter(|p| p.s == s).count();
                    assert_eq!(trees as f32, ((len - 9.0) / 8.0).floor(), "trees down a quay of {b:?}");
                }
            }
            _ => assert_eq!(got.iter().filter(|p| p.kind != Kind::StreetLamp).count(), 0, "{b:?}"),
        }
    }
    // The avenue's stretches, 104 m and 96 m long: lamps corner to corner, trees in the pits
    // (none by the crossings) and benches between them.
    for (k, bx) in [(0u8, 25), (1, 24)] {
        let r = block_rect(bx, 1);
        let (x0, len) = (r.x0, r.length());
        assert_eq!(len, if bx % 4 == 1 { 104.0 } else { 96.0 });
        let got = pieces_in(k, Rect::new(MID - AVENUE * 0.5, MID + AVENUE * 0.5, r.x0, r.x1));
        let n = f::lamp_count(len);
        for side in [-1.0f32, 1.0] {
            let on = |kind: Kind| -> Vec<Furniture> {
                got.iter().filter(|p| p.kind == kind && (p.s - MID) * side > 0.0).copied().collect()
            };
            let lamps = on(Kind::AvenueLamp);
            assert_eq!(lamps.len() as u32, n + 1, "the avenue's lamps at {bx}");
            for (i, p) in lamps.iter().enumerate() {
                near(p.s, MID + side * f::AVENUE_LAMP, "the avenue's lamps");
                near(
                    p.x,
                    x0 + (len * i as f32 / n as f32).clamp(f::LAMP_CORNER, len - f::LAMP_CORNER),
                    "along it",
                );
                assert_eq!(p.lantern(), Some((p.s, p.x, 4.0)), "under the trees: {p:?}");
            }
            let trees = on(Kind::Tree);
            assert_eq!(trees.len() as f32, ((len - 9.0) / 8.0).floor(), "the avenue's trees at {bx}");
            for t in &trees {
                near(t.s, MID + side * f::AVENUE_TREE, "in the pits");
                let u = t.x - x0;
                assert!(u >= f::TREE_END && len - u >= f::TREE_END, "by a crossing: {t:?}");
                near((u - f::TREE_FIRST) % f::TREE_PITCH, 0.0, "in a pit");
            }
            let benches = on(Kind::Bench);
            assert_eq!(benches.len() as f32, ((len - 8.0) / 16.0).floor(), "the avenue's benches at {bx}");
            for (j, p) in benches.iter().enumerate() {
                near(p.x - x0, f::BENCH_PITCH * (j + 1) as f32, "between the trees");
                near(p.solid.h1, f::BENCH_HEIGHT, "a step up");
            }
        }
    }
    // The bank road's far kerb, corner to corner, across from row 12's blocks.
    let bx = (CITY.0..).find(|&bx| has_block(bx, ROWS)).unwrap();
    let r = block_rect(bx, ROWS);
    let (len, n) = (r.length(), f::lamp_count(r.length()));
    let edge = row_span(BANK_ROW).0;
    let got = pieces_in(0, Rect::new(edge, STRIP_WIDTH, r.x0, r.x1));
    assert_eq!(got.len() as u32, n + 1, "the bank road's far kerb at {bx}");
    for (i, p) in got.iter().enumerate() {
        assert_eq!(p.kind, Kind::StreetLamp);
        let inset = if i == 0 || i == n as usize { f::LAMP_CORNER } else { f::LAMP_IN };
        near(p.s, edge + inset, "in from the kerb");
        near(p.x, r.x0 + (len * i as f32 / n as f32).clamp(f::LAMP_CORNER, len - f::LAMP_CORNER), "along it");
        near(p.lantern().unwrap().0, edge - f::LAMP_OUT, "the lantern over the road");
    }
}

#[test]
fn every_parks_pavilions_stand_inside_its_lamps() {
    // So every pool the ground's paint has beside a park's loop has its post (`city_lib.wgsl` knows
    // nothing of pavilions): each pavilion a metre clear of the lamps' line, on all three strips.
    let mut pavilions = 0;
    for k in 0..3u8 {
        for bx in HUB_GATE.0..=SITE.1 {
            for row in -ROWS..=ROWS {
                let Some(b) = block(k, bx, row, STAGE).filter(|b| b.kind == BlockKind::Park) else {
                    continue;
                };
                let ring = b.rect.inset(SIDEWALK + f::PARK_LOOP + f::PARK_LAMP + 1.0);
                for q in lots(&b).as_slice() {
                    let r = q.foot;
                    let inside = r.s0 >= ring.s0 - 1e-3
                        && r.s1 <= ring.s1 + 1e-3
                        && r.x0 >= ring.x0 - 1e-3
                        && r.x1 <= ring.x1 + 1e-3;
                    assert!(inside, "a pavilion {r:?} out by the lamps of {b:?}");
                    pavilions += 1;
                }
            }
        }
    }
    assert!(pavilions > 1_000, "{pavilions} pavilions");
}

#[test]
fn suits_step_over_what_people_bump_into() {
    let mut checked = 0;
    for (k, b0, b1) in [(0u8, 24, 26), (1, 40, 42), (0, 104, 106)] {
        for p in pieces(k, b0, b1).iter().step_by(7) {
            let (b, r) = (p.solid, p.solid.rect);
            let (min, max) = (Vec3::new(r.x0, b.h0 + 0.05, -r.s1), Vec3::new(r.x1, b.h1 - 0.05, -r.s0));
            assert!(solid(k, min, max, STAGE), "people walk through {p:?}");
            assert!(!solid_built(k, min, max, STAGE), "a suit meets {p:?}");
            // 10 m over it a suit's ground is the street under it, as 3 m beside it.
            let kerbed = match block(k, block_index(p.x), row_at(p.s), STAGE) {
                Some(bl) => {
                    let q = bl.rect;
                    (p.s - q.s0).min(q.s1 - p.s).min(p.x - q.x0).min(q.x1 - p.x)
                }
                None => f32::MAX,
            };
            if kerbed < 0.6 {
                continue;
            }
            let over = |s: f32, x: f32| ground_under(CityPos::new(k, x, s, p.h + 10.0).to_colony());
            let open = |&(s, x): &(f32, f32)| {
                let clear =
                    (Vec3::new(x - 0.3, p.h + 0.01, -s - 0.3), Vec3::new(x + 0.3, p.h + 12.0, -s + 0.3));
                city::ground(k, s, x, STAGE) == p.h && !solid_built(k, clear.0, clear.1, STAGE)
            };
            let (s, x) = [(3.0, 0.0), (-3.0, 0.0), (0.0, 3.0), (0.0, -3.0)]
                .into_iter()
                .map(|(ds, dx)| (p.s + ds, p.x + dx))
                .find(open)
                .unwrap_or_else(|| panic!("no open ground beside {p:?}"));
            let (here, there) = (over(p.s, p.x), over(s, x));
            assert!(here.is_some_and(|g| (g - 10.0).abs() < 0.01), "{here:?} under a suit over {p:?}");
            assert!(there.is_some_and(|g| (g - 10.0).abs() < 0.01), "{there:?} beside {p:?}");
            checked += 1;
        }
    }
    assert!(checked > 30, "{checked} checked");
}

#[test]
fn furniture_is_found_alike_from_any_area() {
    let key = |p: &Furniture| (p.kind as u8, p.s.to_bits(), p.x.to_bits());
    let mut rng = Rng::new(23);
    let mut total = 0;
    for _ in 0..40 {
        let k = (rng.next_u32() % 3) as u8;
        let s0 = rng.next_f32() * (STRIP_WIDTH - 300.0);
        let x0 = -15_900.0 + rng.next_f32() * 31_000.0;
        let region = Rect::new(s0, s0 + 300.0, x0, x0 + 400.0);
        let mut whole = Vec::new();
        each_furniture(k, &region, STAGE, |p| {
            whole.push(key(p));
            false
        });
        let set: HashSet<_> = whole.iter().copied().collect();
        assert_eq!(set.len(), whole.len(), "found twice in {region:?}");
        let mut tiled = HashSet::new();
        for i in 0..5 {
            for j in 0..5 {
                let (s, x) = (s0 + 60.0 * i as f32, x0 + 80.0 * j as f32);
                let tile = Rect::new(s, s0 + 60.0 * (i + 1) as f32, x, x0 + 80.0 * (j + 1) as f32);
                each_furniture(k, &tile, STAGE, |p| {
                    tiled.insert(key(p));
                    false
                });
            }
        }
        assert_eq!(set, tiled, "tiled, {region:?} finds otherwise");
        // A metre round each piece finds it.
        each_furniture(k, &region, STAGE, |p| {
            let round = Rect::new(p.s - 0.5, p.s + 0.5, p.x - 0.5, p.x + 0.5);
            assert!(each_furniture(k, &round, STAGE, |q| key(q) == key(p)), "a metre round {p:?} misses it");
            false
        });
        total += set.len();
    }
    assert!(total > 1_000, "{total} found");
}
