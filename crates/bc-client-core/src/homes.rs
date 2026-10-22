//! Homes on foot (`bc_sim::colony::homes`, `docs/OPEN_CITY.md`): which home's door a pilot stands
//! at, and the doors the register says are shut, as the walls a walker meets in their doorways.
//! The server is the lock (the plaza refuses a pose crossing into a home it mayn't): this only
//! draws the same line where the pilot walks.

use bc_econ::wire::{HomeLine, HomesView};
use bc_sim::colony::city::{CityBox, KERB};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::homes::{self, DOOR_HEIGHT, Home, HomeId, Kind};
use bc_sim::content::homes::{bought, piece_box, price, rent};

/// How near a home's door's spot (inside or out) a pilot stands to use it, m.
pub const DOOR_REACH: f32 = 1.6;

/// A shut door's leaf, in its doorway: what a walker meets there. A flat's is in the corridor's
/// wall, a house's in the street front.
pub fn leaf(h: &Home) -> CityBox {
    let floor = if h.kind == Kind::Houses { KERB } else { h.floor };
    CityBox { rect: h.door, h0: floor, h1: floor + DOOR_HEIGHT }
}

/// The leaves of the shut doors among the held homes `view` has, on strip `strip`.
pub fn shut_doors(view: &HomesView, strip: u8) -> Vec<CityBox> {
    view.homes
        .iter()
        .filter(|l| !l.open && HomeId(l.id).strip() == strip)
        .filter_map(|l| homes::home(HomeId(l.id)))
        .map(|h| leaf(&h))
        .collect()
}

/// The solid pieces of furniture in the held homes `view` has, on strip `strip`: what a walker
/// meets in them (a rug, nothing: it's walked over).
pub fn solid_pieces(view: &HomesView, strip: u8) -> impl Iterator<Item = CityBox> + '_ {
    view.homes
        .iter()
        .filter(move |l| HomeId(l.id).strip() == strip)
        .filter_map(|l| homes::home(HomeId(l.id)).map(|h| (h, l)))
        .flat_map(|(h, l)| {
            l.pieces
                .iter()
                .filter(|p| p.kind.spec().solid)
                .map(move |p| piece_box(&h, p.kind, p.at(), p.turn, u32::from(p.storey)))
        })
}

/// The held line for home `id`, if it's held (`None`: free).
pub fn line(view: &HomesView, id: HomeId) -> Option<&HomeLine> {
    view.homes.iter().find(|l| l.id == id.0)
}

/// The home whose door the pilot stands at (`at`, on its floor), and whether they're inside it.
pub fn door_here(at: CityPos) -> Option<(Home, bool)> {
    let p = homes::plan_at(at.strip, at.s, at.x).or_else(|| {
        // Out in the street at a house's front door: the building whose front they're at.
        let mut near = None;
        homes::each_plan_near(at.strip, at.s, at.x, 3.0, |p| near = near.or(Some(*p)));
        near
    })?;
    let storey = if p.kind == Kind::Flats { p.storey_at(at.h + 0.5) } else { 0 };
    let near = |(s, x): (f32, f32)| (at.s - s).hypot(at.x - x) <= DOOR_REACH;
    (0..p.homes_per_storey()).filter_map(|u| p.home(storey, u)).find_map(|h| {
        let level = (at.h - h.floor).abs() < 1.0 || h.kind == Kind::Houses && at.h < 1.0;
        if !level {
            return None;
        }
        // Inside or out is which side of the door they're on (the two spots are closer than the
        // reach).
        let inside = homes::home_at(at.strip, at.s, at.x, at.h + 1.0) == Some(h.id);
        (near(h.inside) || near(h.outside)).then_some((h, inside))
    })
}

/// What using a home's door is called on the prompt: free, its rent or price; held, whose, and
/// what E does there for this pilot.
pub fn door_verb(h: &Home, held: Option<&HomeLine>) -> String {
    let addr = bc_econ::homes::address(h);
    match held {
        None if bought(h) => format!("{addr} · FOR SALE · {} CR", bc_econ::item::thousands(price(h))),
        None => format!("{addr} · TO LET · {} CR A WEEK", bc_econ::item::thousands(rent(h))),
        Some(l) => {
            let whose = if l.mine { "YOURS".to_string() } else { format!("{}'S", l.name.to_uppercase()) };
            let what = if l.crew || l.guest || !l.locked {
                if l.open { "SHUT IT" } else { "OPEN IT" }
            } else if l.open {
                "OPEN"
            } else {
                "LOCKED · THE KEYPAD"
            };
            format!("{addr} · {whose} · {what}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::CityGround;
    use crate::tram::CityAndTrains;
    use crate::walker::{Guide, Walker};
    use bc_sim::colony::city::{CITY, ROWS, Stage, block, lots};

    fn flats() -> homes::Plan {
        (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| {
                lots(&b)
                    .as_slice()
                    .iter()
                    .find_map(|bd| homes::plan(&b, bd).filter(|p| p.kind == Kind::Flats))
            })
            .unwrap()
    }

    #[test]
    fn a_home_s_door_is_found_from_either_side() {
        let p = flats();
        let h = p.home(0, 0).unwrap();
        let at = |(s, x): (f32, f32)| CityPos::new(0, x, s, h.floor);
        assert_eq!(door_here(at(h.outside)).map(|(d, inside)| (d.id, inside)), Some((h.id, false)));
        assert_eq!(door_here(at(h.inside)).map(|(d, inside)| (d.id, inside)), Some((h.id, true)));
        let (ms, mx) = h.rect.middle();
        assert!(
            door_here(at((ms, mx)))
                .is_none_or(|(d, _)| d.id != h.id || (ms - h.inside.0).hypot(mx - h.inside.1) < DOOR_REACH)
        );
    }

    #[test]
    fn a_shut_door_stops_a_walker_and_an_open_one_lets_them_through() {
        let p = flats();
        let h = p.home(0, 0).unwrap();
        let ground = CityGround { strip: 0, stage: Stage(0) };
        let walk = |doors: &[CityBox]| {
            let world = CityAndTrains { ground, trains: &[], doors };
            let mut w =
                Walker::at(CityPos::new(0, h.outside.1, h.outside.0, h.floor + 0.05).walker(), glam::Vec3::X);
            let (s, x) = h.rect.middle();
            let mut g = Guide::new(
                vec![
                    CityPos::new(0, h.inside.1, h.inside.0, h.floor).walker(),
                    CityPos::new(0, x, s, h.floor).walker(),
                ],
                None,
            );
            for _ in 0..60 * 10 {
                let stride = g.steer(&mut w, 1.0 / 60.0);
                w.step(&world, &stride, 1.0 / 60.0);
            }
            let at = CityPos::from_walker(0, w.feet);
            homes::home_at(0, at.s, at.x, at.h + 1.0)
        };
        assert_eq!(walk(&[]), Some(h.id), "through the open door");
        assert_eq!(walk(&[leaf(&h)]), None, "stopped at the shut one");
    }
}
