//! The Exchange floor as the store's trade page shows it (`?showcase=floor`, `docs/FLOOR.md`): the
//! game's own room drawn by the game, with the people `/floor.json` says stand in it (pilots on
//! foot and the store's traders), put back where they are in the city. The page reads the document
//! (`web/floor.js`) and hands it over as text; this turns it into people to draw.
//!
//! Places in the document are the room's own (`Room::local`: `u` along its front from the door's
//! middle, `v` in from its face; a heading 0 faces the counter), so the page's copy and the game's
//! never disagree about where the room is.

use bc_proto::presence::{PersonPose, VISITOR_BASE, VISITORS};
use bc_sim::colony::city::{KERB, Room, place, room};
use serde::Deserialize;

/// The most people drawn (the server lists at most 200; a crowd past this reads the same).
pub const MAX_DRAWN: usize = 96;

/// Someone in the document.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Person {
    pub name: String,
    #[serde(default)]
    pub agent: bool,
    #[serde(default)]
    pub visitor: bool,
    pub u: f32,
    pub v: f32,
    #[serde(default)]
    pub heading: f32,
    #[serde(default)]
    pub moving: bool,
}

#[derive(Debug, Default, Deserialize)]
struct Doc {
    #[serde(default)]
    open: bool,
    #[serde(default)]
    people: Vec<Person>,
}

/// The Exchange floor's room.
pub fn exchange_room() -> Option<Room> {
    place("exchange_floor").and_then(|(i, _)| room(i))
}

/// Someone at `(u, v)` in `room`'s frame, facing `heading`, as a pose on its strip: standing on its
/// floor (or walking, slowly, so a step between two reads shows as one).
pub fn pose(room: &Room, p: &Person) -> PersonPose {
    let (s, x) = room.front.point(p.u, p.v);
    PersonPose {
        strip: room.strip,
        s,
        x,
        h: KERB,
        yaw: room.yaw(p.heading),
        speed: if p.moving { 1.4 } else { 0.0 },
        grounded: true,
        ..PersonPose::default()
    }
}

/// The people to draw from a `/floor.json` document (or the store's copy of it): slot, name and
/// pose, in the document's order, inside the room's walls. The store's traders take the slots the
/// game gives them (from `VISITOR_BASE` up), so they're drawn as they are in play, in an avatar;
/// pilots take slots from 1. A document that isn't one, or a closed floor, draws nobody.
pub fn crowd(json: &str) -> Vec<(u16, String, PersonPose)> {
    let Some(room) = exchange_room() else { return Vec::new() };
    let Ok(doc) = serde_json::from_str::<Doc>(json) else { return Vec::new() };
    if !doc.open {
        return Vec::new();
    }
    let (w, d) = (room.width() * 0.5 - 0.4, room.depth());
    doc.people
        .iter()
        .filter(|p| p.u.is_finite() && p.v.is_finite() && p.heading.is_finite())
        .filter(|p| p.u.abs() <= w && p.v >= 1.4 && p.v <= d - 0.4)
        .take(MAX_DRAWN)
        .scan((1u16, 0u16), |(pilot, trader), p| {
            let id = if p.visitor && *trader < VISITORS {
                *trader += 1;
                VISITOR_BASE + *trader - 1
            } else {
                *pilot += 1;
                *pilot - 1
            };
            Some((id, p.name.chars().take(24).collect(), pose(&room, p)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_stand_where_the_document_puts_them() {
        let room = exchange_room().unwrap();
        let doc = r#"{"open": true, "people": [
            {"name": "Vega", "u": -4.2, "v": 18.5, "heading": 0.3},
            {"name": "Ledger", "agent": true, "u": 0.0, "v": 31.1, "heading": 0.0, "moving": true},
            {"name": "Mira", "visitor": true, "u": 9.0, "v": 12.0, "heading": -1.2}
        ]}"#;
        let people = crowd(doc);
        assert_eq!(people.len(), 3);
        for ((_, name, pose), (u, v, h)) in
            people.iter().zip([(-4.2, 18.5, 0.3), (0.0, 31.1, 0.0), (9.0, 12.0, -1.2)])
        {
            let (pu, pv) = room.local(pose.s, pose.x);
            assert!((pu - u).abs() < 1e-3 && (pv - v).abs() < 1e-3, "{name}: ({pu}, {pv})");
            assert!((room.heading(pose.yaw) - h).abs() < 1e-4, "{name}");
            assert!(room.holds(pose.s, pose.x, pose.h), "{name} is in the room");
        }
        assert_eq!(people[1].2.speed, 1.4, "one walking shows walking");
        // Pilots from 1; the store's trader in the slots the game gives them.
        assert_eq!([people[0].0, people[1].0, people[2].0], [1, 2, VISITOR_BASE]);
    }

    #[test]
    fn nobody_is_drawn_outside_the_walls_or_from_a_closed_or_broken_floor() {
        let doc = r#"{"open": true, "people": [
            {"name": "Out the door", "u": 0.0, "v": -3.0},
            {"name": "In the wall", "u": 40.0, "v": 10.0},
            {"name": "Here", "u": 1.0, "v": 10.0}
        ]}"#;
        let names: Vec<String> = crowd(doc).into_iter().map(|p| p.1).collect();
        assert_eq!(names, ["Here"]);
        assert!(crowd(r#"{"open": false, "people": [{"name": "x", "u": 0, "v": 10}]}"#).is_empty());
        assert!(crowd("not json").is_empty());
        assert!(crowd(r#"{"open": true}"#).is_empty());
    }
}
