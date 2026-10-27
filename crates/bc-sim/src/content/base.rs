//! The Muster's buildings (`docs/RING.md`, "The base"), built from Gates' pieces
//! (`bc_sim::pieces`) and standing on the base's deck in the hub's drum. Bumped with the protocol
//! ([`BASE_VERSION`]) on any change, as the city's content was: what's walked is worked out from it
//! on every screen and on the server alike.
//!
//! The first is a test hall, to walk and to check the pieces by: a room of 4 × 3 cells with a
//! doorway, a flight up to a gallery along its back, a skylight, and a counter to stand at. The base's real
//! buildings follow it one at a time.

use glam::Vec3;

use crate::colony::deck::BASE;
use crate::pieces::prefab::{Placed, Prefab, RoomDef, Site, edge_x, edge_z, flight, foundation, slab};
use crate::pieces::shape::*;

/// Bumped with `PROTOCOL_VERSION` on any change to the base's buildings.
pub const BASE_VERSION: u16 = 1;

/// The test hall's pieces, in the order they'd be built: its foundations, its walls (a doorway in
/// the middle of its front), a flight in its back corner up to the gallery over its back half, the
/// gallery's floor and its low wall, and its roof.
const TEST_HALL_PIECES: [Placed; 58] = [
    // Foundations, 4 × 3.
    foundation(0, 0),
    foundation(1, 0),
    foundation(2, 0),
    foundation(3, 0),
    foundation(0, 1),
    foundation(1, 1),
    foundation(2, 1),
    foundation(3, 1),
    foundation(0, 2),
    foundation(1, 2),
    foundation(2, 2),
    foundation(3, 2),
    // The front (low z), its doorway second from the left.
    edge_x(WALL, 0, 0, 0),
    edge_x(DOORWAY, 1, 0, 0),
    edge_x(WALL, 2, 0, 0),
    edge_x(WALL, 3, 0, 0),
    // The back (high z).
    edge_x(WALL, 0, 3, 0),
    edge_x(WALL, 1, 3, 0),
    edge_x(WALL, 2, 3, 0),
    edge_x(WALL, 3, 3, 0),
    // The sides.
    edge_z(WALL, 0, 0, 0),
    edge_z(WINDOW, 0, 1, 0),
    edge_z(WALL, 0, 2, 0),
    edge_z(WALL, 4, 0, 0),
    edge_z(WINDOW, 4, 1, 0),
    edge_z(WALL, 4, 2, 0),
    // The flight in the back right corner, climbing towards the back.
    flight(STAIRS, 3, 1, 0, 3),
    // The upper storey's walls on the lower's.
    edge_x(WALL, 0, 0, 1),
    edge_x(WINDOW, 1, 0, 1),
    edge_x(WALL, 2, 0, 1),
    edge_x(WALL, 3, 0, 1),
    edge_x(WALL, 0, 3, 1),
    edge_x(WALL, 1, 3, 1),
    edge_x(WALL, 2, 3, 1),
    edge_x(WALL, 3, 3, 1),
    edge_z(WALL, 0, 0, 1),
    edge_z(WALL, 0, 1, 1),
    edge_z(WALL, 0, 2, 1),
    edge_z(WALL, 4, 0, 1),
    edge_z(WALL, 4, 1, 1),
    edge_z(WALL, 4, 2, 1),
    // The gallery over the back row, the flight coming up at its right end.
    slab(FLOOR, 0, 2, 1),
    slab(FLOOR, 1, 2, 1),
    slab(FLOOR, 2, 2, 1),
    slab(FLOOR, 3, 2, 1),
    // Its rail along its front edge, but over the flight's head.
    edge_x(LOW_WALL, 0, 2, 1),
    edge_x(LOW_WALL, 1, 2, 1),
    edge_x(LOW_WALL, 2, 2, 1),
    // The roof, on the walls round it: over the middle two cells, which no wall bears, a skylight.
    slab(ROOF, 0, 0, 2),
    slab(ROOF, 1, 0, 2),
    slab(ROOF, 2, 0, 2),
    slab(ROOF, 3, 0, 2),
    slab(ROOF, 0, 1, 2),
    slab(ROOF, 3, 1, 2),
    slab(ROOF, 0, 2, 2),
    slab(ROOF, 1, 2, 2),
    slab(ROOF, 2, 2, 2),
    slab(ROOF, 3, 2, 2),
];

const TEST_HALL_ROOMS: [RoomDef; 1] =
    [RoomDef { name: "TEST HALL", cells: (0, 0, 3, 2), level: 0, counter: Vec3::new(1.5, 0.0, 6.0) }];

/// The test hall.
pub const TEST_HALL: Prefab = Prefab {
    name: "TEST HALL",
    pieces: &TEST_HALL_PIECES,
    rooms: &TEST_HALL_ROOMS,
    cells: (0, 0, 3, 2),
    storeys: 2,
};

/// Where the base's buildings stand.
pub const SITES: [Site; 1] = [Site { deck: BASE, x: -540.0, s: 400.0, turn: 0, prefab: &TEST_HALL }];
