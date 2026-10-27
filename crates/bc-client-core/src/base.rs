//! On foot on the base's deck in the hub (`docs/RING.md`, "The base"): out of the bay's airlock onto
//! Bay Row, the walker on `bc_sim::foot::DeckGround` in the deck's walker frame, `(x, h, −s)`
//! (`bc_sim::colony::deck`), under the deck's 0.4 g; its pose as the plaza has it; the airlock's door
//! to go back in by; and the routes a guide walks it along (the dev hooks, the browser's autopilot).

use glam::Vec3;

use crate::walker::Walker;
use bc_proto::presence::{BASE_DECK, PersonPose};
use bc_sim::colony::deck::{self, BASE, DeckPos};
use bc_sim::content::base::SITES;

pub use bc_sim::foot::DeckGround;

/// The base's deck, to walk.
pub const GROUND: DeckGround = DeckGround { deck: BASE };
/// How near its doorstep a pilot uses their bay's airlock from, looking at its door, m (the server
/// takes them back in within 6 m of it).
pub const AIRLOCK_USE: f32 = 3.0;

/// Just out of bay `bay`'s airlock onto Bay Row, facing across the row (+x, into the deck).
pub fn out_of_the_airlock(bay: u8) -> Walker {
    let mut w = Walker::at(deck::out_of_the_airlock(bay).walker(), Vec3::X);
    w.grounded = true;
    w
}

/// A walker on the base's deck, as the plaza has them (`bc_proto::presence`, [`BASE_DECK`]).
pub fn pose_of(w: &Walker) -> PersonPose {
    let at = DeckPos::from_walker(BASE, w.feet);
    let flat = Vec3::new(w.vel.x, 0.0, w.vel.z).length();
    PersonPose {
        strip: BASE_DECK,
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

/// Whether a walker can use bay `bay`'s airlock: by its doorstep, looking at its door.
pub fn at_the_airlock(w: &Walker, bay: u8) -> bool {
    let step = deck::out_of_the_airlock(bay).walker();
    let door = deck::airlock(bay).walker() + Vec3::Y * 1.5;
    let near = (w.feet - step).with_y(0.0).length() <= AIRLOCK_USE;
    near && w.look().dot((door - w.eye()).normalize_or_zero()) > 0.6
}

/// Where a guide walks a pilot on the deck to from `from` (their feet), by name (the dev hooks'
/// `walk_to`): `airlock`, back to bay `bay`'s; `hall`, in at the test hall's door to its counter.
/// The row is open between the airlocks and the buildings' doors; from inside the hall the way out
/// is its door. The route, and the way to face at its end.
pub fn route(spot: &str, bay: u8, from: Vec3) -> Option<(Vec<Vec3>, Vec3)> {
    let site = &SITES[0];
    let outside = site.to_walker(Vec3::new(4.5, 0.0, -1.5));
    let inside = site.to_walker(Vec3::new(4.5, 0.0, 1.0));
    let b = site.bounds();
    let in_hall = (b.min.x..=b.max.x).contains(&from.x) && (b.min.z..=b.max.z).contains(&from.z);
    let mut route = Vec::new();
    match spot {
        "airlock" => {
            if in_hall {
                route.extend([inside, outside]);
            }
            route.push(deck::out_of_the_airlock(bay).walker());
            Some((route, -Vec3::X))
        }
        "hall" => {
            let counter = site.to_walker(site.prefab.rooms[0].counter);
            if !in_hall {
                route.extend([outside, inside]);
            }
            route.push(counter);
            Some((route, (counter - inside).with_y(0.0).normalize()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::{Guide, Stride};
    use bc_sim::colony::hub::BAYS;

    fn walk(w: &mut Walker, guide: &mut Guide, secs: f32) -> bool {
        for _ in 0..(secs * 60.0) as u32 {
            let stride = guide.steer(w, 1.0 / 60.0);
            w.step(&GROUND, &stride, 1.0 / 60.0);
            if guide.arrived() {
                return true;
            }
        }
        false
    }

    #[test]
    fn out_of_every_airlock_a_pilot_stands_on_bay_row_and_can_turn_and_go_back_in() {
        for bay in 1..=BAYS as u8 {
            let mut w = out_of_the_airlock(bay);
            let start = w.feet;
            for _ in 0..60 {
                w.step(&GROUND, &Stride::default(), 1.0 / 60.0);
            }
            assert!(w.grounded && (w.feet - start).length() < 0.02, "bay {bay}: {:?}", w.feet);
            assert!(!at_the_airlock(&w, bay), "bay {bay}: facing away from its door");
            w.turn(std::f32::consts::PI, 0.0);
            assert!(at_the_airlock(&w, bay), "bay {bay}: turned to its door");
            assert!(!at_the_airlock(&w, bay % BAYS as u8 + 1), "bay {bay}: not the next bay's door");
            let p = pose_of(&w);
            let at = deck::out_of_the_airlock(bay);
            assert_eq!(p.strip, BASE_DECK);
            assert!((p.x - at.x).abs() < 1e-3 && (p.s - at.s).abs() < 1e-3 && p.h.abs() < 0.02, "{p:?}");
        }
    }

    #[test]
    fn a_pilot_crosses_bay_row_into_the_hall_and_back_to_their_airlock() {
        let bay = 1;
        let mut w = out_of_the_airlock(bay);
        let (route_in, facing) = route("hall", bay, w.feet).unwrap();
        let mut guide = Guide::new(route_in, Some(facing));
        assert!(walk(&mut w, &mut guide, 30.0), "to the hall's counter: stuck at {:?}", w.feet);
        // From the counter: out of its door, then along the row.
        let (back, facing) = route("airlock", bay, w.feet).unwrap();
        assert_eq!(back.len(), 3, "out by the door");
        let mut guide = Guide::new(back, Some(facing));
        assert!(walk(&mut w, &mut guide, 30.0), "back to the airlock: stuck at {:?}", w.feet);
        // Turned to its door.
        for _ in 0..60 {
            let stride = guide.steer(&mut w, 1.0 / 60.0);
            w.step(&GROUND, &stride, 1.0 / 60.0);
        }
        assert!(at_the_airlock(&w, bay), "at {:?} looking {:?}", w.feet, w.look());
        // Out on the row, the hall is in at its door; the airlock straight along.
        assert_eq!(route("hall", bay, w.feet).unwrap().0.len(), 3);
        assert_eq!(route("airlock", bay, w.feet).unwrap().0.len(), 1);
    }
}
