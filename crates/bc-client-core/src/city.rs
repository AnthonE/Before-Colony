//! On foot in the colony's city: the walker among `bc_sim::colony::city`'s streets and buildings,
//! in its frame on a strip, `(x, h, −s)` (`bc_sim::colony::frame`), under the colony's gravity,
//! which weakens with height (1 g at the floor).

use bc_sim::colony::city::{Stage, solid};
use bc_sim::colony::frame::{CityPos, gravity};
use glam::Vec3;

use crate::walker::Solid;

/// A strip of the city, to walk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityGround {
    pub strip: u8,
    pub stage: Stage,
}

impl Solid for CityGround {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        solid(self.strip, min, max, self.stage)
    }

    fn gravity(&self, feet: Vec3) -> f32 {
        gravity(feet.y)
    }
}

impl CityGround {
    /// Where `feet` (in the walker's frame) are in city coordinates.
    pub fn place(&self, feet: Vec3) -> CityPos {
        CityPos::from_walker(self.strip, feet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::{Stride, Walker};
    use bc_sim::colony::city::{
        BLOCK, CANAL_ROW, KERB, SIDEWALK, block, block_rect, channel, grid_x, lots, place, place_door,
    };

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
}
