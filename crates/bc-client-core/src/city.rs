//! On foot in the colony's city: the walker among `bc_sim::colony::city`'s streets and buildings,
//! in its frame on a strip, `(x, h, −s)` (`bc_sim::colony::frame`), under the colony's gravity,
//! which weakens with height (1 g at the floor).

use glam::Vec3;

use crate::walker::Walker;
use bc_proto::presence::PersonPose;
use bc_sim::colony::frame::CityPos;

/// A strip of the city, to walk (`bc_sim::foot`'s, shared with the tick).
pub use bc_sim::foot::CityGround;

/// A walker on strip `strip`, as the plaza has them (`bc_proto::presence`).
pub fn pose_of(strip: u8, w: &Walker) -> PersonPose {
    let at = CityPos::from_walker(strip, w.feet);
    let flat = Vec3::new(w.vel.x, 0.0, w.vel.z).length();
    PersonPose {
        strip,
        x: at.x,
        s: at.s,
        h: at.h,
        yaw: w.yaw,
        pitch: w.pitch,
        roll: 0.0,
        speed: flat,
        grounded: w.grounded,
        running: flat > 5.0,
        ride: 0,
        jets: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::{Guide, Stride};
    use bc_sim::colony::city::{
        BLOCK, CANAL_ROW, KERB, Rect, SIDEWALK, Stage, block, block_rect, channel, grid_x, lots, place,
        place_door,
    };
    use bc_sim::colony::frame::STRIP_WIDTH;
    use bc_sim::colony::furniture::{AVENUE_TREE, BENCH_HEIGHT, Furniture, Kind, each_furniture};
    use bc_sim::colony::homes;

    const DT: f32 = 1.0 / 60.0;
    const CITY: CityGround = CityGround { strip: 0, stage: Stage(0) };

    fn at(s: f32, x: f32, h: f32, facing: Vec3) -> Walker {
        let mut w = Walker::at(CityPos::new(0, x, s, h).walker(), facing);
        for _ in 0..90 {
            w.step(&CITY, &Stride::default(), DT);
        }
        w
    }

    fn walk(w: &mut Walker, secs: f32, s: Stride) {
        for _ in 0..(secs / DT) as u32 {
            w.step(&CITY, &s, DT);
        }
    }

    #[test]
    fn a_pilot_stands_on_the_avenue_and_walks_it() {
        // Just out of Hub Gate's terminal, facing down the colony (+X).
        let ((s, x), _) = place_door(place("hub_gate_1").unwrap().1);
        let mut w = at(s, x, 0.5, Vec3::X);
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
        walk(&mut w, 10.0, Stride { forward: 1.0, run: true, ..Stride::default() });
        let p = CITY.place(w.feet);
        assert!((p.x - x - 70.0).abs() < 3.0, "ran {} m", p.x - x);
        assert!(w.grounded && p.h.abs() < 1e-3);
        // A jump at 1 g: lower than in the bay at 0.7 g.
        w.step(&CITY, &Stride { jump: true, ..Stride::default() }, DT);
        let mut top: f32 = 0.0;
        for _ in 0..120 {
            w.step(&CITY, &Stride::default(), DT);
            top = top.max(w.feet.y);
        }
        assert!(top > 0.55 && top < 0.75, "jumped {top} m");
    }

    #[test]
    fn kerbs_are_stepped_and_walls_stop_you() {
        // Find a block with a building near its edge, and walk at it across the street.
        let (bx, row) = (20, -3);
        let b = block(0, bx, row, Stage(0)).unwrap();
        // The building nearest the avenue's side of the block (larger s), walked at from the street
        // on that side, facing −s: +Z.
        let all = lots(&b);
        let building = all.as_slice().iter().max_by(|a, c| a.foot.s1.total_cmp(&c.foot.s1)).unwrap();
        let (_, mx) = building.foot.middle();
        let mut w = at(b.rect.s1 + 8.0, mx, 0.2, Vec3::Z);
        walk(&mut w, 8.0, Stride { forward: 1.0, ..Stride::default() });
        let p = CITY.place(w.feet);
        assert!((p.h - KERB).abs() < 1e-3, "up on the pavement: {p:?}");
        assert!(
            p.s > building.foot.s1 && p.s < building.foot.s1 + 0.5,
            "at the wall: {p:?} vs {:?}",
            building.foot
        );
        assert!(building.foot.s1 <= b.rect.s1 - SIDEWALK + 1e-3, "the pavement's clear");
    }

    /// Strip 0's pieces of the furniture on `area` that `pick` likes, along `x`.
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

    #[test]
    fn a_pilot_walks_into_a_lamp_post_and_the_guide_takes_them_round() {
        // A street lamp in the middle of a lane's kerb, walked at down the pavement.
        let b = block(0, 30, 3, Stage(0)).unwrap();
        let post = furniture(b.rect, |p| p.kind == Kind::StreetLamp && p.facing.0 != 0.0 && p.reach > 2.0)[0];
        let mut w = at(post.s, post.x - 5.0, KERB + 0.2, Vec3::X);
        walk(&mut w, 3.0, Stride { forward: 1.0, ..Stride::default() });
        let p = CITY.place(w.feet);
        let short = Kind::StreetLamp.size().1 + 0.3;
        assert!((post.x - p.x - short).abs() < 0.02, "stopped at the post: {p:?} vs {post:?}");
        // The guide sidesteps it.
        let mut g = Guide::new(vec![CityPos::new(0, post.x + 10.0, post.s, KERB).walker()], None);
        let mut t = 0.0;
        while !g.arrived() {
            let s = g.steer(&mut w, DT);
            w.step(&CITY, &s, DT);
            t += DT;
            assert!(t < 4.0, "caught on the post at {:?}", CITY.place(w.feet));
        }
    }

    /// Guides a pilot standing at `from` (`(s, x, h)`) to `to`: how long it took, or where the
    /// guide let go of them.
    fn guided(from: (f32, f32, f32), to: (f32, f32, f32)) -> Result<f32, CityPos> {
        let ds = to.0 - from.0;
        let mut w = at(from.0, from.1, from.2, Vec3::new(0.0, 0.0, -ds.signum()));
        let mut g = Guide::new(vec![CityPos::new(0, to.1, to.0, to.2).walker()], None);
        let mut t = 0.0;
        while !g.arrived() && t < 20.0 {
            let s = g.steer(&mut w, DT);
            w.step(&CITY, &s, DT);
            t += DT;
        }
        let p = CITY.place(w.feet);
        let home = (p.s - to.0).hypot(p.x - to.1) < 0.5;
        if g.arrived() && home { Ok(t) } else { Err(p) }
    }

    #[test]
    fn the_guide_crosses_rows_of_lamps_and_trees_wherever_it_meets_them() {
        // Straight across the avenue's lamps and trees (and benches) from its road to the walk
        // under them and back, and from a lane onto its pavement past the kerb's lamps and back,
        // every 3 cm along: whichever side of a post or a trunk it catches on, the guide takes the
        // pilot round.
        let mut slowest = 0.0f32;
        let mut cross = |from: (f32, f32, f32), to: (f32, f32, f32)| match guided(from, to) {
            Ok(t) => slowest = slowest.max(t),
            Err(p) => panic!("from {from:?} to {to:?}: let go at {p:?}"),
        };
        let mid = STRIP_WIDTH * 0.5;
        // Caught on a trunk's near edge (at c = −24.3, x = −13,280), the short way round is left.
        cross((mid - 15.0, -13_279.53, 0.0), (mid - 28.0, -13_279.53, 0.0));
        let r = block_rect(30, 1);
        let trees = Rect::new(mid - AVENUE_TREE - 0.5, mid - AVENUE_TREE + 0.5, r.x0, r.x1);
        let lamps = Rect::new(mid - 23.0, mid - 22.4, r.x0, r.x1);
        let mut posts: Vec<f32> =
            furniture(lamps, |p| p.kind == Kind::AvenueLamp).iter().map(|p| p.x).collect();
        posts.extend(furniture(trees, |p| p.kind == Kind::Tree).iter().take(4).map(|p| p.x));
        assert!(posts.len() > 5, "{posts:?}");
        for &x0 in &posts {
            for k in 0..80 {
                let x = x0 - 1.2 + 0.03 * k as f32;
                cross((mid - 15.0, x, 0.0), (mid - 28.0, x, 0.0));
                cross((mid - 28.0, x, 0.0), (mid - 15.0, x, 0.0));
            }
        }
        // A lane's kerb, its lamps 0.8 m in from it on the pavement (the corners' 0.25 m).
        let b = block(0, 30, 3, Stage(0)).unwrap();
        let edge = b.rect.s1;
        let kerb = Rect::new(edge - 1.0, edge, b.rect.x0, b.rect.x1);
        let posts = furniture(kerb, |p| p.kind == Kind::StreetLamp);
        assert!(posts.len() >= 3, "{posts:?}");
        for p in posts {
            for k in 0..80 {
                let x = p.x - 1.2 + 0.03 * k as f32;
                cross((edge + 6.0, x, 0.0), (edge - 3.0, x, KERB));
                cross((edge - 3.0, x, KERB), (edge + 6.0, x, 0.0));
            }
        }
        assert!(slowest < 8.0, "{slowest} s");
    }

    #[test]
    fn a_pilot_steps_up_onto_a_bench_and_off_it() {
        // Down the avenue's row of trees, over a bench to the trunk after it.
        let r = block_rect(30, 1);
        let s = STRIP_WIDTH * 0.5 + AVENUE_TREE;
        let row = Rect::new(s - 0.5, s + 0.5, r.x0, r.x1);
        let bench = furniture(row, |p| p.kind == Kind::Bench)[0];
        let trunk = furniture(row, |p| p.kind == Kind::Tree && p.x > bench.x)[0];
        assert!((trunk.x - bench.x - 4.0).abs() < 1e-3, "{bench:?} {trunk:?}");
        let mut w = at(s, bench.x - 3.0, 0.2, Vec3::X);
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
        let mut top: f32 = 0.0;
        for _ in 0..(4.0 / DT) as u32 {
            w.step(&CITY, &Stride { forward: 1.0, ..Stride::default() }, DT);
            top = top.max(w.feet.y);
        }
        let p = CITY.place(w.feet);
        assert!((top - BENCH_HEIGHT).abs() < 0.02, "up on the bench: {top}");
        assert!(w.grounded && p.h.abs() < 1e-3, "and down off it: {p:?}");
        assert!((trunk.x - p.x - 0.5).abs() < 0.02, "stopped at the trunk: {p:?} vs {trunk:?}");
    }

    #[test]
    fn the_glass_is_railed() {
        let mut w = at(30.0, 1_000.0, 0.2, Vec3::Z);
        walk(&mut w, 10.0, Stride { forward: 1.0, run: true, ..Stride::default() });
        let p = CITY.place(w.feet);
        assert!(p.s > 0.3 && p.s < 0.7, "at the railing: {p:?}");
        // And a jump doesn't clear it.
        walk(&mut w, 2.0, Stride { forward: 1.0, jump: true, ..Stride::default() });
        assert!(CITY.place(w.feet).s > 0.3);
    }

    #[test]
    fn the_canal_is_crossed_by_its_bridges() {
        // Along a cross street (constant x, on the street's middle line), across the canal's row.
        let r = block_rect(40, CANAL_ROW);
        let ch = channel(&r);
        let x = grid_x(41);
        let mut w = at(ch.s0 - 30.0, x, 0.2, -Vec3::Z);
        assert!(w.grounded);
        walk(&mut w, 15.0, Stride { forward: 1.0, run: true, ..Stride::default() });
        let p = CITY.place(w.feet);
        assert!(p.s > ch.s1 + 20.0 && p.h.abs() < 1e-3, "across on the bridge: {p:?}");
        // Along a quay you can't walk off into the water.
        let mut w = at(ch.s0 - 3.0, r.x0 + BLOCK * 0.3, KERB + 0.2, -Vec3::Z);
        walk(&mut w, 5.0, Stride { forward: 1.0, run: true, ..Stride::default() });
        let p = CITY.place(w.feet);
        assert!(p.s < ch.s0 && (p.h - KERB).abs() < 1e-3, "railed off: {p:?}");
    }

    /// Guides a pilot through `points` (`(s, x)` and the floor there) on strip `strip`, from
    /// standing at the first: where they end up, or where the guide let go of them.
    fn through(strip: u8, points: &[((f32, f32), f32)]) -> Result<CityPos, CityPos> {
        let ground = CityGround { strip, stage: Stage(0) };
        let ((s, x), h) = points[0];
        let mut w = Walker::at(CityPos::new(strip, x, s, h + 0.05).walker(), Vec3::X);
        let route = points[1..].iter().map(|&((s, x), h)| CityPos::new(strip, x, s, h).walker()).collect();
        let mut g = Guide::new(route, None);
        let mut t = 0.0;
        while !g.arrived() && t < 30.0 * points.len() as f32 {
            let st = g.steer(&mut w, DT);
            w.step(&ground, &st, DT);
            t += DT;
        }
        let p = ground.place(w.feet);
        let ((s, x), h) = points[points.len() - 1];
        let there = (p.s - s).hypot(p.x - x) < 0.6 && (p.h - h).abs() < 0.1;
        if g.arrived() && there { Ok(p) } else { Err(p) }
    }

    /// The first home building of `kind` on strip `strip` with at least `storeys` storeys.
    fn home_building(strip: u8, kind: homes::Kind, storeys: u32) -> homes::Plan {
        for bx in bc_sim::colony::city::CITY.0..bc_sim::colony::city::CITY.1 {
            for row in 1..=bc_sim::colony::city::ROWS {
                let Some(b) = block(strip, bx, row, Stage(0)) else { continue };
                for bd in lots(&b).as_slice() {
                    if let Some(p) = homes::plan(&b, bd)
                        && p.kind == kind
                        && p.storeys >= storeys
                        && p.sides != 0
                    {
                        return p;
                    }
                }
            }
        }
        panic!("no {kind:?} of {storeys} storeys on strip {strip}");
    }

    #[test]
    fn a_pilot_walks_in_off_the_street_and_up_every_storey_of_a_block_of_flats() {
        let p = home_building(0, homes::Kind::Flats, 6);
        // From the pavement in at the lobby door, onto the landing.
        let mut way = vec![
            (p.point(0.0, -homes::SHELL - 1.2), KERB),
            (p.point(0.0, 0.8), KERB),
            (p.point(0.0, homes::NEAR * 0.5), KERB),
        ];
        for k in 0..p.storeys - 1 {
            way.extend(p.stairs_up(k, 0));
            let reached =
                through(0, &way).unwrap_or_else(|at| panic!("to storey {}: let go at {at:?}", k + 1));
            assert!((reached.h - homes::level(k + 1)).abs() < 0.05, "{reached:?}");
        }
        // Along the top storey's corridor to a flat's door, and in.
        let top = p.storeys - 1;
        let flat = p.home(top, 1).unwrap();
        way.push((p.point(0.0, homes::NEAR * 0.5), homes::level(top)));
        way.push((flat.outside, flat.floor));
        way.push((flat.inside, flat.floor));
        way.push((flat.rect.middle(), flat.floor));
        let at = through(0, &way).unwrap_or_else(|at| panic!("into {flat:?}: let go at {at:?}"));
        assert_eq!(homes::home_at(0, at.s, at.x, at.h + 1.0), Some(flat.id));
    }

    #[test]
    fn a_pilot_walks_in_at_a_house_s_front_door_and_up_its_stairs() {
        let p = home_building(0, homes::Kind::Houses, 3);
        let house = p.home(0, 0).unwrap();
        let mut way = vec![(house.outside, KERB), (house.inside, KERB)];
        for k in 0..p.storeys - 1 {
            way.extend(p.stairs_up(k, 0));
            let reached =
                through(0, &way).unwrap_or_else(|at| panic!("to storey {}: let go at {at:?}", k + 1));
            assert!((reached.h - homes::level(k + 1)).abs() < 0.05, "{reached:?}");
        }
        let (s, x) = house.rect.middle();
        assert_eq!(homes::home_at(0, s, x, homes::level(p.storeys - 1) + 1.0), Some(house.id));
    }

    #[test]
    fn a_home_s_walls_keep_a_pilot_out_of_the_next_one() {
        // Walking from a flat straight at its neighbour, they stop at the wall between.
        let p = home_building(0, homes::Kind::Flats, 3);
        let (a, b) = (p.home(1, 0).unwrap(), p.home(1, 1).unwrap());
        let way = [(a.rect.middle(), a.floor), (b.rect.middle(), b.floor)];
        let at = through(0, &way).expect_err("walked through a wall");
        assert_eq!(homes::home_at(0, at.s, at.x, at.h + 1.0), Some(a.id), "{at:?}");
    }
}
