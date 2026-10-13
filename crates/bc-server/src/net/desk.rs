//! Which of the city's key places a pilot on foot stands in: the room behind its door, the whole of
//! it (its counter's desk, and the Blast Hall's gantry's hatch). While the colony is open, what
//! only a desk does (`bc_econ::wire::Request::desk`) is done in its room (`docs/COLONY.md`, 2.6).

use bc_sim::colony::city::room_at;
use bc_sim::colony::frame::{Under, from_colony};
use bc_sim::content::city::{PLACES, PlaceKind};
use glam::Vec3;

/// The key place whose room `at` (the colony's own frame: the plaza's last pose of a pilot) is in,
/// under its ceiling. `None` on the street, at Hub Gate (it has no room), or off the land.
pub(super) fn place_at(at: Vec3) -> Option<PlaceKind> {
    match from_colony(at) {
        Under::Land(c) => room_at(c.strip, c.s, c.x, c.h).map(|r| PLACES[usize::from(r.place)].kind),
        Under::Window { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{place_door, room};
    use bc_sim::colony::frame::{CityPos, STRIPS};
    use bc_sim::colony::hall;

    fn at(strip: u8, (s, x): (f32, f32)) -> Vec3 {
        CityPos::new(strip, x, s, 0.0).to_colony()
    }

    #[test]
    fn a_rooms_counter_is_its_own_place_and_just_outside_its_door_is_the_street() {
        let mut rooms = 0;
        for (i, p) in PLACES.iter().enumerate() {
            let Some(r) = room(i) else { continue };
            rooms += 1;
            let (spot, _) = r.counter_spot();
            assert_eq!(place_at(at(r.strip, spot)), Some(p.kind), "{}'s counter", p.name);
            let (outside, _) = r.threshold();
            assert_eq!(place_at(at(r.strip, outside)), None, "outside {}'s door", p.name);
            // The same spot on another strip is somewhere else.
            let other = (r.strip + 1) % STRIPS as u8;
            assert_eq!(place_at(at(other, spot)), None, "{}'s counter on strip {other}", p.name);
        }
        assert_eq!(rooms, 4, "the Arrival, the Exchange floor, the Charter Board and the Blast Hall");
    }

    #[test]
    fn hub_gate_has_no_room_and_the_gantrys_hatch_is_the_proving_grounds() {
        for p in PLACES.iter().filter(|p| p.kind == PlaceKind::HubGate) {
            assert_eq!(place_at(at(p.strip, place_door(p).0)), None, "{}", p.name);
        }
        assert_eq!(place_at(at(hall::hall().strip, hall::hatch().0)), Some(PlaceKind::Proving));
    }
}
