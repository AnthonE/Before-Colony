//! Homes on foot (`bc_sim::colony::homes`, `docs/OPEN_CITY.md`): which home's door a pilot stands
//! at, and the doors the register says are shut and the furniture in the homes, as the walls a
//! walker meets (the homes' overlay).
//! The server is the lock (the plaza refuses a pose crossing into a home it mayn't): this only
//! draws the same line where the pilot walks.

use bc_econ::wire::{HomeLine, HomesView};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::homes::{self, Home, HomeId, Kind, Overlay, OverlayHome};
use bc_sim::content::homes::{bought, price, rent};

/// How near a home's door's spot (inside or out) a pilot stands to use it, m.
pub const DOOR_REACH: f32 = 1.6;

pub use bc_sim::colony::homes::leaf;

/// How many held homes the client's overlay has room for: more than a view ever holds.
pub const VIEW_HOMES: usize = 256;

/// The held homes `view` has, as everyone's walker meets them (`bc_sim::colony::homes::Overlay`):
/// their shut doors and solid furniture, built as the server builds its own from the register.
pub fn overlay_from(view: &HomesView, overlay: &mut Overlay) {
    overlay.clear();
    for l in &view.homes {
        let Some(h) = homes::home(HomeId(l.id)) else { continue };
        let pieces = l.pieces.iter().map(|p| (p.kind, p.at(), p.turn, u32::from(p.storey)));
        overlay.set(&OverlayHome::of(&h, l.open, pieces));
    }
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
        let walk = |doors: &Overlay| {
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
        let mut overlay = Overlay::new(4);
        overlay.set(&OverlayHome::new(h.id, false));
        assert_eq!(walk(&overlay), Some(h.id), "through the open door");
        overlay.set(&OverlayHome::new(h.id, true));
        assert_eq!(walk(&overlay), None, "stopped at the shut one");
        // A sofa across the way in stops them too.
        let (s, x) = h.inside;
        let mut sofa = OverlayHome::new(h.id, false);
        sofa.add(bc_sim::colony::city::CityBox {
            rect: bc_sim::colony::city::Rect::new(s - 1.5, s + 1.5, x - 1.5, x + 1.5),
            h0: h.floor,
            h1: h.floor + 0.9,
        });
        overlay.set(&sofa);
        assert_eq!(walk(&overlay), None, "stopped at the sofa");
    }
}
