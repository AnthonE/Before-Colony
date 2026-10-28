//! The Exchange floor, seen from outside the game (`/floor.json`): who is standing in the Colony
//! Exchange's trading floor now, where in the room and which way they face, and what its boards
//! show. The Elo Pros trade terminal draws it from above, so a trader on the store's page sees the
//! same room a pilot walks into (`docs/FLOOR.md`). Off the hot path: it reads the plaza and the
//! market, like `/status`.
//!
//! Places are the room's own: `u` metres along its front from the door's middle (+ to the right
//! looking in), `v` metres in from the front face, towards the counter and the boards on its back
//! wall. A heading is radians in that frame: 0 faces the boards, τ/4 faces +u.
//!
//! Only people are listed, never the colony's crowd (nobody of it walks in here): everyone on it is
//! a pilot on foot, by the callsign other pilots already see over their head, and whether an agent
//! flies them. No wallet, no key.

use std::time::Instant;

use bc_econ::Exchange;
use bc_econ::exchange::Quote;
use bc_proto::presence::PersonPose;
use bc_sim::colony::city::{COUNTER_DEPTH, COUNTER_GAP, Room, WALL, room};
use bc_sim::content::city::{PLACES, PlaceKind};

use crate::pilots::unix_now;
use crate::plaza::Plaza;

/// The most people the document lists (the room is 48 m by 36 m: a crowd, not a census).
pub const MAX_LISTED: usize = 200;
/// The most items on the boards: the busiest first.
pub const BOARD_ROWS: usize = 24;

/// The Exchange floor's room.
pub fn exchange_room() -> Option<Room> {
    PLACES.iter().position(|p| p.kind == PlaceKind::Exchange).and_then(room)
}

/// A point of the room's strip, `(s, x)`, in the room's own frame: `(u, v)`.
pub fn local(room: &Room, s: f32, x: f32) -> (f32, f32) {
    let f = &room.front;
    if f.along_s { (s - f.middle, (x - f.face) * f.sign) } else { (x - f.middle, (s - f.face) * f.sign) }
}

/// A walker's yaw (0 faces −s, τ/4 faces +x) as a heading in the room's frame (0 faces the boards,
/// τ/4 faces +u).
pub fn heading(room: &Room, yaw: f32) -> f32 {
    let (ds, dx) = (-yaw.cos(), yaw.sin());
    let f = &room.front;
    let (du, dv) = if f.along_s { (ds, dx * f.sign) } else { (dx, ds * f.sign) };
    du.atan2(dv)
}

/// Rounded to the centimetre (or the hundredth of a radian): the page needs no more.
fn r2(v: f32) -> f64 {
    (f64::from(v) * 100.0).round() / 100.0
}

/// The room's shape, in its own frame.
pub fn room_json(room: &Room) -> serde_json::Value {
    let depth = room.depth();
    let near = depth - COUNTER_GAP - COUNTER_DEPTH;
    serde_json::json!({
        "width": r2(room.width()),
        "depth": r2(depth),
        "wall": r2(WALL),
        "door": r2(room.door_width()),
        "counter": { "width": r2(room.counter_width()), "near": r2(near), "far": r2(depth - COUNTER_GAP) },
        "ceiling": r2(room.ceiling),
    })
}

/// One person on the floor.
pub fn person_json(room: &Room, name: &str, agent: bool, pose: &PersonPose) -> serde_json::Value {
    let (u, v) = local(room, pose.s, pose.x);
    serde_json::json!({
        "name": name,
        "agent": agent,
        "u": r2(u),
        "v": r2(v),
        "heading": r2(heading(room, pose.yaw)),
        "moving": pose.speed > 0.3,
        "seated": pose.seated(),
    })
}

/// What the boards show: the items traded most, then those the colony deals in, at most
/// [`BOARD_ROWS`]. Prices are credits (a tonne of bulk goods, else a piece).
pub fn board_json(quotes: Vec<Quote>) -> Vec<serde_json::Value> {
    let mut quotes = quotes;
    quotes.retain(|q| q.bid.is_some() || q.ask.is_some() || q.last.is_some());
    quotes.sort_by(|a, b| b.volume.cmp(&a.volume).then(b.colony.cmp(&a.colony)).then(a.item.cmp(&b.item)));
    quotes
        .into_iter()
        .take(BOARD_ROWS)
        .map(|q| {
            serde_json::json!({
                "item": q.item.slug(),
                "name": q.item.name(),
                "per": if q.item.bulk() { "t" } else { "piece" },
                "bid": q.bid,
                "ask": q.ask,
                "last": q.last,
                "volume": q.volume,
            })
        })
        .collect()
}

/// The whole document: `{at, place, open, room, people, board}`. `agent` says whether a client
/// slot is flown by an agent. With no colony there is no floor: `open` is false and nothing else
/// is filled in.
pub fn json(
    plaza: Option<&Plaza>,
    exchange: Option<&Exchange>,
    agent: impl Fn(u16) -> bool,
) -> serde_json::Value {
    let (Some(room), Some(plaza)) = (exchange_room(), plaza) else {
        return serde_json::json!({ "at": unix_now(), "open": false });
    };
    let mut people: Vec<serde_json::Value> = plaza
        .in_room(&room, Instant::now())
        .into_iter()
        .map(|(slot, name, pose)| person_json(&room, &name, agent(slot), &pose))
        .collect();
    let listed = people.len();
    people.truncate(MAX_LISTED);
    let place = PLACES.iter().find(|p| p.kind == PlaceKind::Exchange).map_or("", |p| p.name);
    serde_json::json!({
        "at": unix_now(),
        "open": true,
        "place": place,
        "room": room_json(&room),
        "here": listed,
        "people": people,
        "board": exchange.map(|ex| board_json(ex.quotes())).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::COUNTER_STAND;

    #[test]
    fn the_room_frame_puts_the_door_at_zero_and_the_counter_at_the_back() {
        let room = exchange_room().expect("the Exchange floor has a room");
        let (outside, _) = room.threshold();
        let (u, v) = local(&room, outside.0, outside.1);
        assert!(u.abs() < 0.01, "the threshold is the door's middle: {u}");
        assert!((v - -2.0).abs() < 0.01, "and 2 m out in front of its face: {v}");
        let ((s, x), (ds, dx)) = room.counter_spot();
        let (u, v) = local(&room, s, x);
        let want = room.depth() - COUNTER_GAP - COUNTER_DEPTH - COUNTER_STAND;
        assert!(u.abs() < 0.01 && (v - want).abs() < 0.01, "the counter spot: ({u}, {v})");
        // Facing the counter is heading 0, whichever way the hall's front runs.
        let yaw = dx.atan2(-ds);
        assert!(heading(&room, yaw).abs() < 1e-4, "{}", heading(&room, yaw));
    }

    #[test]
    fn nobody_is_on_an_empty_floor_and_the_boards_carry_the_colonys_desks() {
        let plaza = Plaza::default();
        let ex = Exchange::new();
        let doc = json(Some(&plaza), Some(&ex), |_| false);
        assert_eq!(doc["open"], true);
        assert_eq!(doc["place"], "THE EXCHANGE FLOOR");
        assert_eq!(doc["people"], serde_json::json!([]));
        let board = doc["board"].as_array().unwrap();
        assert!(!board.is_empty() && board.len() <= BOARD_ROWS);
        assert!(board.iter().all(|r| r["bid"].is_u64() || r["ask"].is_u64()), "{board:?}");
        assert_eq!(doc["room"]["width"], 48.0);
    }

    #[test]
    fn a_pilot_walking_in_is_on_the_floor_and_one_outside_is_not() {
        let room = exchange_room().unwrap();
        let plaza = Plaza::default();
        let (outside, _) = room.threshold();
        // Two pilots come down the lift; each is put straight at a spot by a first pose there.
        let ((cs, cx), _) = room.counter_spot();
        plaza.enter_at(1, "Vega", room.strip, (cs, cx));
        plaza.enter_at(2, "Rook", room.strip, outside);
        let now = Instant::now();
        let pose =
            |s: f32, x: f32| PersonPose { strip: room.strip, s, x, grounded: true, ..PersonPose::default() };
        assert_eq!(plaza.accept(1, 1, pose(cs, cx), now, 0), crate::plaza::Verdict::Taken);
        assert_eq!(plaza.accept(2, 1, pose(outside.0, outside.1), now, 0), crate::plaza::Verdict::Taken);
        let doc = json(Some(&plaza), None, |slot| slot == 1);
        let people = doc["people"].as_array().unwrap();
        assert_eq!(people.len(), 1, "{people:?}");
        assert_eq!(people[0]["name"], "Vega");
        assert_eq!(people[0]["agent"], true);
        assert!(people[0]["v"].as_f64().unwrap() > 20.0);
        assert_eq!(doc["board"], serde_json::json!([]));
    }

    #[test]
    fn no_colony_no_floor() {
        let doc = json(None, None, |_| false);
        assert_eq!(doc["open"], false);
        assert!(doc.get("people").is_none());
    }
}
