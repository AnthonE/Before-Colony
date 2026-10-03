//! Routes through the city: its streets make a grid, so a route is a walk along a street to the
//! right cross street, along that to the right street, and along that to the door. A walk from a
//! key place's room goes out through its door first. Waypoints are in the walker's frame on the
//! strip (`(x, h, −s)`), for `walker::Guide`.

use bc_sim::colony::city::{BANK_ROW, KERB, ROWS, block_index, grid_x, place_door, room, room_at, row_at};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH};
use bc_sim::content::city::PLACES;
use glam::Vec3;

/// The middle line of the street along the axis nearest `s` across: the avenue's (on its pavement,
/// out of the tram's way), or a lane between rows of blocks.
pub fn street_line(s: f32) -> f32 {
    let mid = STRIP_WIDTH * 0.5;
    let row = row_at(s);
    if row == 0 {
        return mid + if s < mid { -28.0 } else { 28.0 };
    }
    let k = row.abs().min(ROWS);
    let sign = if row < 0 { -1.0 } else { 1.0 };
    // The lanes on either side of this row: its inner (towards the avenue) and outer edges.
    let inner = 40.0 + 128.0 * (k - 1) as f32;
    let outer = 40.0 + 128.0 * k as f32;
    let c = (s - mid).abs();
    let pick = if k == 1 {
        // Row 1's inner side is the avenue's pavement.
        if c - inner < outer - c { inner - 12.0 } else { outer }
    } else if c - inner < outer - c || row.abs() >= BANK_ROW {
        inner
    } else {
        outer
    };
    mid + sign * pick
}

/// The cross street nearest `x` along the axis: its middle line.
pub fn cross_line(x: f32) -> f32 {
    let b = block_index(x);
    let (a, c) = (grid_x(b), grid_x(b + 1));
    if x - a < c - x { a } else { c }
}

/// Where key place `i` is used, and which way the pilot faces to: at the counter of the room behind
/// its door (`bc_sim::colony::city::Room`), or, Hub Gate, at its door. `(s, x)`, a unit `(ds, dx)`.
pub fn use_spot(i: usize) -> ((f32, f32), (f32, f32)) {
    match room(i) {
        Some(r) => r.counter_spot(),
        None => place_door(&PLACES[i]),
    }
}

/// A route on strip `strip` from `at` to where key place `i` is used ([`use_spot`]): along the
/// streets to its door, and in through it to its counter (from inside the room already, straight
/// to it).
pub fn route_to_place(strip: u8, at: CityPos, i: usize) -> Vec<Vec3> {
    let (spot, _) = use_spot(i);
    let Some(r) = room(i) else { return route_from(strip, at, spot) };
    let walk = |(s, x): (f32, f32)| CityPos::new(strip, x, s, KERB).walker();
    let mut out = Vec::new();
    if !r.holds(at.s, at.x, at.h) {
        let (outside, inside) = r.threshold();
        out = route_from(strip, at, place_door(&PLACES[i]).0);
        out.extend([walk(outside), walk(inside)]);
    }
    out.push(walk(spot));
    out
}

/// A route on strip `strip` from `at` to `to` (`(s, x)`) along the streets ([`route`]), out
/// through the door of the room `at` is in first, if it's in one.
pub fn route_from(strip: u8, at: CityPos, to: (f32, f32)) -> Vec<Vec3> {
    let Some(r) = room_at(strip, at.s, at.x, at.h) else { return route(strip, (at.s, at.x), to) };
    let walk = |(s, x): (f32, f32)| CityPos::new(strip, x, s, KERB).walker();
    let (outside, inside) = r.threshold();
    let mut out = vec![walk(inside), walk(outside)];
    out.extend(route(strip, outside, to));
    out
}

/// A route from `from` to `to` on strip `strip`, ground level, as waypoints in the walker's frame.
pub fn route(strip: u8, from: (f32, f32), to: (f32, f32)) -> Vec<Vec3> {
    let (fs, fx) = from;
    let (ts, tx) = to;
    let (a, b) = (street_line(fs), street_line(ts));
    let xc = cross_line(tx);
    let pts = [(a, fx), (a, xc), (b, xc), (b, tx), (ts, tx)];
    let mut out: Vec<Vec3> = Vec::with_capacity(pts.len());
    for (s, x) in pts {
        let w = CityPos::new(strip, x, s, 0.0).walker();
        if out.last().is_none_or(|p| (*p - w).length() > 0.5) {
            out.push(w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::CityGround;
    use crate::walker::{Guide, Stride, Walker};
    use bc_sim::colony::city::{Stage, place, place_door};
    use bc_sim::content::city::PLACES;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn the_guide_walks_from_hub_gate_to_every_place_on_its_strip() {
        for p in PLACES.iter() {
            let ground = CityGround { strip: p.strip, stage: Stage(0) };
            let (gate, _) = place_door(place(&format!("hub_gate_{}", p.strip + 1)).unwrap().1);
            let (door, (ds, dx)) = place_door(p);
            let start = CityPos::new(p.strip, gate.1, gate.0, 0.0).walker();
            let mut w = Walker::at(start, Vec3::X);
            for _ in 0..30 {
                w.step(&ground, &Stride::default(), DT);
            }
            let facing = CityPos::new(p.strip, door.1 + dx, door.0 + ds, 0.0).walker()
                - CityPos::new(p.strip, door.1, door.0, 0.0).walker();
            let mut g = Guide::new(route(p.strip, gate, door), Some(facing.normalize()));
            let mut t = 0.0;
            while !g.arrived() {
                let s = g.steer(&mut w, DT);
                w.step(&ground, &s, DT);
                t += DT;
                assert!(t < 400.0, "lost on the way to {}: at {:?}", p.name, ground.place(w.feet));
            }
            let at = ground.place(w.feet);
            assert!(
                (at.s - door.0).abs() < 1.0 && (at.x - door.1).abs() < 1.0,
                "{}: {at:?} vs {door:?}",
                p.name
            );
        }
    }

    #[test]
    fn the_guide_walks_in_from_hub_gate_to_every_places_counter() {
        for (i, p) in PLACES.iter().enumerate() {
            let ground = CityGround { strip: p.strip, stage: Stage(0) };
            let (gate, _) = place_door(place(&format!("hub_gate_{}", p.strip + 1)).unwrap().1);
            let ((s, x), (ds, dx)) = use_spot(i);
            let start = CityPos::new(p.strip, gate.1, gate.0, 0.0);
            let mut w = Walker::at(start.walker(), Vec3::X);
            for _ in 0..30 {
                w.step(&ground, &Stride::default(), DT);
            }
            let facing = CityPos::new(p.strip, x + dx, s + ds, 0.0).walker()
                - CityPos::new(p.strip, x, s, 0.0).walker();
            let mut g =
                Guide::new(route_to_place(p.strip, ground.place(w.feet), i), Some(facing.normalize()));
            let mut t = 0.0;
            while !g.arrived() {
                let st = g.steer(&mut w, DT);
                w.step(&ground, &st, DT);
                t += DT;
                assert!(t < 400.0, "lost on the way to {}: at {:?}", p.name, ground.place(w.feet));
            }
            let at = ground.place(w.feet);
            assert!((at.s - s).abs() < 1.0 && (at.x - x).abs() < 1.0, "{}: {at:?} vs {:?}", p.name, (s, x));
            // From where it stands, the route there again is just a step.
            assert!(route_to_place(p.strip, at, i).len() <= 1 || room(i).is_none(), "{}", p.name);
        }
    }

    #[test]
    fn the_guide_walks_out_of_every_room_and_back_to_hub_gate() {
        for (i, p) in PLACES.iter().enumerate() {
            let Some(r) = room(i) else { continue };
            let ground = CityGround { strip: p.strip, stage: Stage(0) };
            let ((s, x), _) = r.counter_spot();
            let mut w = Walker::at(CityPos::new(p.strip, x, s, KERB).walker(), Vec3::X);
            for _ in 0..30 {
                w.step(&ground, &Stride::default(), DT);
            }
            let (gate, _) = place_door(place(&format!("hub_gate_{}", p.strip + 1)).unwrap().1);
            let mut g = Guide::new(route_from(p.strip, ground.place(w.feet), gate), None);
            let mut t = 0.0;
            while !g.arrived() {
                let st = g.steer(&mut w, DT);
                w.step(&ground, &st, DT);
                t += DT;
                assert!(t < 600.0, "stuck leaving {}: at {:?}", p.name, ground.place(w.feet));
            }
            let at = ground.place(w.feet);
            assert!((at.s - gate.0).abs() < 1.0 && (at.x - gate.1).abs() < 1.0, "{}: {at:?}", p.name);
        }
    }

    #[test]
    fn routes_keep_to_the_streets() {
        let r = route(0, (STRIP_WIDTH * 0.5, -15_900.0), (2_000.0, -9_000.0));
        assert!(r.len() >= 3 && r.len() <= 5, "{r:?}");
        for w in &r {
            assert_eq!(w.y, 0.0);
        }
    }
}
