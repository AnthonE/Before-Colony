//! The base's buildings stand as Gates' rules would let a player build them, and a pilot can walk
//! them: each building's pieces, placed in the order they're written, pass `pieces::check`; and a
//! walker on the base's deck goes in at its door, to its counter, and up its stairs and down again.

#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use bc_sim::colony::deck::{BASE, DECKS, DeckPos, solid};
use bc_sim::content::base::SITES;
use bc_sim::foot::{DeckGround, Stride, Walker};
use bc_sim::pieces::check::{Placed as PlacedSoFar, check};
use bc_sim::pieces::shape::LEVEL_H_M;
use bc_sim::pieces::sock::PieceAddr;
use bc_sim::pieces::support::Lookup;
use glam::Vec3;

struct Building(Vec<(u8, PieceAddr)>);

impl Lookup for Building {
    fn shape_at(&self, a: &PieceAddr) -> Option<u8> {
        self.0.iter().find(|(_, b)| b == a).map(|(s, _)| *s)
    }
}

impl PlacedSoFar for Building {
    fn each(&self, f: &mut dyn FnMut(u8, &PieceAddr)) {
        for (s, a) in &self.0 {
            f(*s, a);
        }
    }
}

#[test]
fn every_building_goes_up_in_the_order_its_written() {
    for site in &SITES {
        let mut b = Building(Vec::new());
        for (k, p) in site.prefab.pieces.iter().enumerate() {
            let a = p.addr(0);
            if let Err(why) = check(&b, p.shape, &a) {
                panic!("{}: piece {k} ({p:?}) refused: {why:?}", site.prefab.name);
            }
            b.0.push((p.shape, a));
        }
        // Every piece inside what the building says it covers, and the building on its deck.
        let bounds = site.prefab.bounds();
        let d = &DECKS[site.deck as usize];
        let w = site.bounds();
        assert!(w.min.x >= d.x0 && w.max.x <= d.x1, "{}: off its deck's ends", site.prefab.name);
        assert!(w.max.z <= 0.0 && w.min.z >= -d.round(), "{}: across its deck's seam", site.prefab.name);
        assert!(w.max.y <= d.roof_h, "{}: through its deck's ceiling", site.prefab.name);
        for p in site.prefab.pieces {
            for (_, b) in bc_sim::pieces::parts::parts(p.shape, &p.addr(0)).as_slice() {
                assert!(
                    b.min.cmpge(bounds.min - Vec3::splat(1e-3)).all()
                        && b.max.cmple(bounds.max + Vec3::splat(1e-3)).all(),
                    "{}: {p:?} outside its cells",
                    site.prefab.name
                );
            }
        }
    }
}

/// Walks `w` towards `to` (the walker's frame) on the base's deck until it's within `near` m of it
/// in plan, or the time's up; whether it got there.
fn walk_to(w: &mut Walker, ground: &DeckGround, to: Vec3, near: f32, secs: f32) -> bool {
    let dt = 1.0 / 30.0;
    for _ in 0..(secs / dt) as u32 {
        let d = Vec3::new(to.x - w.feet.x, 0.0, to.z - w.feet.z);
        if d.length() < near {
            return true;
        }
        w.yaw = bc_sim::math::atan2(d.x, d.z);
        w.step(ground, &Stride { forward: 1.0, ..Stride::default() }, dt);
    }
    false
}

#[test]
fn a_pilot_walks_into_the_test_hall_to_its_counter_and_up_to_its_gallery() {
    let site = &SITES[0];
    let ground = DeckGround { deck: BASE };
    // Out on the deck in front of its doorway (on the hall's low-z side, cell 1), facing it.
    let door = site.to_walker(Vec3::new(4.5, 0.0, 0.0));
    let outside = site.to_walker(Vec3::new(4.5, 0.0, -6.0));
    let mut w = Walker::at(outside, door - outside);
    for _ in 0..30 {
        w.step(&ground, &Stride::default(), 1.0 / 30.0);
    }
    assert!(w.grounded && w.feet.y.abs() < 0.01, "standing on the deck: {:?}", w.feet);
    // In at the door, to the counter.
    assert!(walk_to(&mut w, &ground, door, 0.3, 10.0), "to the door: stuck at {:?}", w.feet);
    let counter = site.to_walker(site.prefab.rooms[0].counter);
    assert!(walk_to(&mut w, &ground, counter, 0.5, 20.0), "to the counter: stuck at {:?}", w.feet);
    // To the foot of the flight (cell (3, 1), climbing +z), up it to the gallery a storey up.
    let foot = site.to_walker(Vec3::new(10.5, 0.0, 2.6));
    assert!(walk_to(&mut w, &ground, foot, 0.4, 20.0), "to the stairs: stuck at {:?}", w.feet);
    let top = site.to_walker(Vec3::new(10.5, LEVEL_H_M, 7.5));
    assert!(walk_to(&mut w, &ground, top, 0.4, 20.0), "up the stairs: stuck at {:?}", w.feet);
    assert!((w.feet.y - LEVEL_H_M).abs() < 0.05 && w.grounded, "on the gallery: {:?}", w.feet);
    // Along the gallery behind its rail, and down again.
    let along = site.to_walker(Vec3::new(2.0, LEVEL_H_M, 7.5));
    assert!(walk_to(&mut w, &ground, along, 0.4, 20.0), "along the gallery: stuck at {:?}", w.feet);
    assert!(walk_to(&mut w, &ground, top, 0.4, 20.0), "back: stuck at {:?}", w.feet);
    assert!(walk_to(&mut w, &ground, foot, 0.4, 20.0), "down the stairs: stuck at {:?}", w.feet);
    // (At the base's 0.4 g a walker coming down at a walk floats off the treads a while.)
    for _ in 0..60 {
        w.step(&ground, &Stride::default(), 1.0 / 30.0);
    }
    assert!(w.feet.y.abs() < 0.05 && w.grounded, "back down: {:?}", w.feet);
    // Where it ends up is clear, and on the base's deck.
    let (lo, hi) = (w.feet - Vec3::new(0.29, -0.01, 0.29), w.feet + Vec3::new(0.29, 1.79, 0.29));
    assert!(!solid(BASE, lo, hi));
    assert!(bc_sim::colony::deck::deck_at(DeckPos::from_walker(BASE, w.feet).to_colony()).is_some());
}
