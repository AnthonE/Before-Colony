//! The ring's town, as written down (`docs/FREE_ROAM.md`, T1): its twelve districts round the rim from
//! Hub Gate, the quarter each belongs to (`docs/STORY.md`, "Inside"), and the blocks given over to a
//! place of the colony's own. The rules that turn these into streets and buildings are `colony::town`'s.
//!
//! Any change here changes what every client draws and walks into, and what the server checks poses
//! against, so it moves [`TOWN_VERSION`] with the protocol's version.

/// The town's version: bumped, with `bc_proto::PROTOCOL_VERSION`, on any change to the layout.
pub const TOWN_VERSION: u8 = 2;

use glam::Vec3;

use crate::pieces::prefab::{Placed, Prefab, RoomDef, edge_x, edge_z, foundation, slab};
use crate::pieces::shape::{CELL_M, DOORWAY, FRAME, LOW_WALL, ROOF, WALL, WINDOW};

/// The quarters the town is in, each a run of districts round the rim (`docs/STORY.md`). Each has its
/// tram line's colour for its wayfinding (Charter blue, Canal teal, Gardens green; the building site's
/// safety yellow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quarter {
    /// The colony's offices, its money, its first streets.
    Charter,
    /// A working quarter along the canal that carries its freight round the ring.
    Canal,
    /// Orchards, terraces, homes.
    Gardens,
    /// The arc still open frames and cranes, where the colony is still being built.
    Site,
}

/// What a district is, which sets how it's built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistrictKind {
    /// Halls and offices of the colony's institutions.
    Civic,
    /// Towers on podiums: the money.
    Business,
    Midtown,
    Residential,
    /// The first streets built: narrow lots, low.
    OldTown,
    /// Depots, workshops and warehouses.
    Works,
    /// Mostly parkland.
    Park,
    /// Frames and cranes: nothing finished.
    Site,
}

/// A district: its name on the signs, its quarter and its kind, and which of its quarter's panel
/// tints the station language paints it in (0 to 3: `docs/FREE_ROAM.md`, the station language with a
/// colour per district).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DistrictDef {
    pub name: &'static str,
    pub quarter: Quarter,
    pub kind: DistrictKind,
    pub tint: u8,
}

const fn district(name: &'static str, quarter: Quarter, kind: DistrictKind, tint: u8) -> DistrictDef {
    DistrictDef { name, quarter, kind, tint }
}

/// The twelve districts round the rim, from Hub Gate the way `s` grows, each 30° of the ring (1,571
/// m). A spoke's lift comes down at the start of every even one (`colony::town::LIFTS`); Hub Gate's
/// square is round the first. Names from `docs/STORY.md`'s names in waiting.
pub const DISTRICTS: [DistrictDef; 12] = {
    use DistrictKind::*;
    use Quarter as Q;
    [
        district("CHARTER SQUARE", Q::Charter, Civic, 0),
        district("EXCHANGE ROW", Q::Charter, Business, 1),
        district("TOWER HILL", Q::Charter, Business, 2),
        district("OLD TOWN", Q::Charter, OldTown, 3),
        district("QUAYSIDE", Q::Canal, Works, 0),
        district("LOCK TOWN", Q::Canal, Residential, 1),
        district("THE BASIN", Q::Canal, Midtown, 2),
        district("THE YARDS", Q::Canal, Works, 3),
        district("GARDEN GATE", Q::Gardens, Park, 0),
        district("TERRACES", Q::Gardens, Residential, 1),
        district("GREEN MERIDIAN", Q::Gardens, Midtown, 2),
        district("THE BUILDING SITE", Q::Site, Site, 0),
    ]
};

/// A place of the colony's own on a block of the town, built from pieces (`bc_sim::pieces`) rather
/// than by the town's rules: its block stands empty for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    /// The Colony Exchange's hall.
    ExchangeFloor,
    /// The bar where pilots meet.
    TheArrival,
    /// The Charter Board's hall, where its notices go up.
    CharterHall,
    /// A station of the Watch: its hounds, its cars and its people (one in every district,
    /// `colony::town::watch_station`).
    WatchStation,
}

/// A key place: its block (`colony::town::BlockId`'s cell round the ring and column across it) and
/// what it is. The Watch's stations aren't listed: there's one in every district, at
/// `colony::town::watch_station`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceDef {
    pub cell: u16,
    pub column: u8,
    pub kind: PlaceKind,
}

/// The key places, round Hub Gate's square: the Exchange floor and The Arrival facing each other
/// across the boulevard just past it, the Charter Board's hall just before it.
pub const PLACES: [PlaceDef; 3] = [
    PlaceDef { cell: 1, column: 6, kind: PlaceKind::ExchangeFloor },
    PlaceDef { cell: 1, column: 7, kind: PlaceKind::TheArrival },
    PlaceDef { cell: 142, column: 6, kind: PlaceKind::CharterHall },
];

/// How many pieces a [`hall`] `w` cells along its front and `d` deep takes.
pub const fn hall_pieces(w: i32, d: i32) -> usize {
    // Foundations and roof; two storeys of walls round it; a colonnade of frames under every other
    // line of its roof, both storeys; the counter across its back.
    (2 * w * d + 2 * (2 * w + 2 * d) + 2 * w * ((d - 1) / 2) + (w - 2)) as usize
}

/// A key place's hall, `w` cells along its front and `d` deep (even): two storeys high under one
/// roof, its doorway in the middle of its front with a window either side, windows down its sides
/// and along its upper front, a colonnade of frames under every other line of the roof (a roof
/// slab stands on what's under one of its sides), and a counter of low walls across it a cell
/// short of its back wall, to be served at.
pub const fn hall<const N: usize>(w: i32, d: i32) -> [Placed; N] {
    assert!(N == hall_pieces(w, d) && d % 2 == 0 && w >= 4 && d >= 4);
    let mut out = [foundation(0, 0); N];
    let mut k = 0;
    let mut j = 0;
    while j < d {
        let mut i = 0;
        while i < w {
            out[k] = foundation(i, j);
            k += 1;
            i += 1;
        }
        j += 1;
    }
    let mut level = 0u8;
    while level < 2 {
        // The front: the doorway and its windows below, windows along it above.
        let mut i = 0;
        while i < w {
            let end = i == 0 || i == w - 1;
            let shape = if level == 0 {
                if i == w / 2 {
                    DOORWAY
                } else if i == w / 2 - 2 || i == w / 2 + 2 {
                    WINDOW
                } else {
                    WALL
                }
            } else if end {
                WALL
            } else {
                WINDOW
            };
            out[k] = edge_x(shape, i, 0, level);
            out[k + 1] = edge_x(WALL, i, d, level);
            k += 2;
            i += 1;
        }
        // The sides: a window every other cell.
        let mut j = 0;
        while j < d {
            let shape = if j % 2 == 1 { WINDOW } else { WALL };
            out[k] = edge_z(shape, 0, j, level);
            out[k + 1] = edge_z(shape, w, j, level);
            k += 2;
            j += 1;
        }
        // The colonnade.
        let mut j = 2;
        while j < d {
            let mut i = 0;
            while i < w {
                out[k] = edge_x(FRAME, i, j, level);
                k += 1;
                i += 1;
            }
            j += 2;
        }
        level += 1;
    }
    // The counter, open at either end.
    let mut i = 1;
    while i < w - 1 {
        out[k] = edge_x(LOW_WALL, i, d - 1, 0);
        k += 1;
        i += 1;
    }
    let mut j = 0;
    while j < d {
        let mut i = 0;
        while i < w {
            out[k] = slab(ROOF, i, j, 2);
            k += 1;
            i += 1;
        }
        j += 1;
    }
    assert!(k == N);
    out
}

/// The one room of a [`hall`], named, with its counter's near side to stand at.
///
/// Its doorway's middle, in the hall's frame, is [`hall_door`].
/// The middle of a [`hall`]'s doorway, in its frame, m.
pub const fn hall_door(prefab: &Prefab) -> Vec3 {
    Vec3::new(((prefab.cells.2 + 1) / 2) as f32 * CELL_M + 0.5 * CELL_M, 0.0, 0.0)
}

const fn hall_room(name: &'static str, w: i32, d: i32) -> [RoomDef; 1] {
    [RoomDef {
        name,
        cells: (0, 0, w - 1, d - 1),
        level: 0,
        // (In line with the doorway's middle, so the way in is straight between the colonnade's posts.)
        counter: Vec3::new((w / 2) as f32 * CELL_M + 0.5 * CELL_M, 0.0, (d - 1) as f32 * CELL_M - 0.9),
    }]
}

const EXCHANGE: (i32, i32) = (12, 8);
const ARRIVAL: (i32, i32) = (8, 6);
const CHARTER: (i32, i32) = (10, 8);
const WATCH: (i32, i32) = (10, 6);
const EXCHANGE_PIECES: [Placed; hall_pieces(EXCHANGE.0, EXCHANGE.1)] = hall(EXCHANGE.0, EXCHANGE.1);
const ARRIVAL_PIECES: [Placed; hall_pieces(ARRIVAL.0, ARRIVAL.1)] = hall(ARRIVAL.0, ARRIVAL.1);
const CHARTER_PIECES: [Placed; hall_pieces(CHARTER.0, CHARTER.1)] = hall(CHARTER.0, CHARTER.1);
const WATCH_PIECES: [Placed; hall_pieces(WATCH.0, WATCH.1)] = hall(WATCH.0, WATCH.1);
const EXCHANGE_ROOM: [RoomDef; 1] = hall_room("THE EXCHANGE FLOOR", EXCHANGE.0, EXCHANGE.1);
const ARRIVAL_ROOM: [RoomDef; 1] = hall_room("THE ARRIVAL", ARRIVAL.0, ARRIVAL.1);
const CHARTER_ROOM: [RoomDef; 1] = hall_room("THE CHARTER BOARD", CHARTER.0, CHARTER.1);
const WATCH_ROOM: [RoomDef; 1] = hall_room("THE WATCH", WATCH.0, WATCH.1);

const fn place_hall(
    name: &'static str,
    wd: (i32, i32),
    pieces: &'static [Placed],
    rooms: &'static [RoomDef],
) -> Prefab {
    Prefab { name, pieces, rooms, cells: (0, 0, wd.0 - 1, wd.1 - 1), storeys: 2 }
}

/// The key places' buildings, from Gates' pieces as the base's are (`docs/RING.md`, "Buildings
/// from pieces"), each standing on its block with its doorway on the boulevard
/// (`colony::town::place_site`).
pub const EXCHANGE_FLOOR: Prefab =
    place_hall("THE EXCHANGE FLOOR", EXCHANGE, &EXCHANGE_PIECES, &EXCHANGE_ROOM);
pub const THE_ARRIVAL: Prefab = place_hall("THE ARRIVAL", ARRIVAL, &ARRIVAL_PIECES, &ARRIVAL_ROOM);
pub const CHARTER_HALL: Prefab = place_hall("THE CHARTER BOARD", CHARTER, &CHARTER_PIECES, &CHARTER_ROOM);
pub const WATCH_STATION: Prefab = place_hall("THE WATCH", WATCH, &WATCH_PIECES, &WATCH_ROOM);

impl PlaceKind {
    /// Its building.
    pub const fn prefab(self) -> &'static Prefab {
        match self {
            PlaceKind::ExchangeFloor => &EXCHANGE_FLOOR,
            PlaceKind::TheArrival => &THE_ARRIVAL,
            PlaceKind::CharterHall => &CHARTER_HALL,
            PlaceKind::WatchStation => &WATCH_STATION,
        }
    }

    /// Its name over its door and on the map.
    pub const fn name(self) -> &'static str {
        self.prefab().name
    }
}

/// The names the town's people go by (`colony::census`): a given name and a family name each, from
/// every part of the Earth the colony's builders came from, and none from another studio's show or
/// game (`docs/STORY.md`, "Voice and naming"). A hundred of each, so a street's crowd rarely holds the
/// same person twice. (They aren't the town's layout: changing them doesn't move [`TOWN_VERSION`],
/// but it does change who everybody is.)
pub const GIVEN_NAMES: [&str; 100] = [
    "Ada", "Agnes", "Aino", "Aksel", "Alba", "Amara", "Anouk", "Ansel", "Arlo", "Asha", "Aurel", "Ayo",
    "Bea", "Bram", "Brielle", "Bruno", "Calla", "Cass", "Clara", "Coralie", "Dara", "Dov", "Eamon", "Edda",
    "Efe", "Elin", "Elio", "Emre", "Esme", "Farah", "Femi", "Ffion", "Gil", "Greta", "Hana", "Hugo", "Ida",
    "Ilka", "Ilse", "Imre", "Ines", "Ingrid", "Isak", "Ivo", "Jae", "Joss", "Kasia", "Kenji", "Kofi", "Lark",
    "Leif", "Lena", "Linnea", "Lior", "Lotte", "Luca", "Maeve", "Malik", "Marek", "Maren", "Mateo", "Mei",
    "Milo", "Mira", "Nell", "Nils", "Noor", "Nuno", "Odile", "Olu", "Oren", "Orla", "Otto", "Petra", "Priya",
    "Quinn", "Rafa", "Ravi", "Rhea", "Rosa", "Rui", "Sami", "Sanne", "Saoirse", "Selma", "Sven", "Taavi",
    "Tamsin", "Teo", "Thea", "Tobin", "Tomas", "Ulla", "Uma", "Una", "Vesna", "Vida", "Wren", "Yusuf",
    "Zaid",
];

pub const FAMILY_NAMES: [&str; 100] = [
    "Abara",
    "Achterberg",
    "Adeyemi",
    "Amsel",
    "Asante",
    "Baines",
    "Bakker",
    "Barlow",
    "Bergstrom",
    "Birch",
    "Bonsu",
    "Brandt",
    "Cardoso",
    "Carvalho",
    "Castell",
    "Chandra",
    "Coombe",
    "Corrigan",
    "Dagher",
    "Dalby",
    "Delgado",
    "Dunmore",
    "Ekwueme",
    "Elgin",
    "Engstrom",
    "Fairweather",
    "Falk",
    "Fennick",
    "Ferreira",
    "Fofana",
    "Galloway",
    "Garrow",
    "Haddad",
    "Hartley",
    "Hayward",
    "Hoffmann",
    "Holm",
    "Huang",
    "Ibarra",
    "Iqbal",
    "Ishida",
    "Jaramillo",
    "Jovanovic",
    "Kalnins",
    "Kapoor",
    "Kaur",
    "Keane",
    "Kiplagat",
    "Kowalczyk",
    "Lachance",
    "Lam",
    "Larkin",
    "Lindqvist",
    "Lund",
    "Marchetti",
    "Mensah",
    "Merriman",
    "Moreno",
    "Mwangi",
    "Nakashima",
    "Navarro",
    "Nowak",
    "Nyberg",
    "Obi",
    "Okafor",
    "Ortega",
    "Osei",
    "Oyelaran",
    "Pellegrini",
    "Penrose",
    "Quintero",
    "Rahman",
    "Rasku",
    "Rautio",
    "Renwick",
    "Rocha",
    "Sadeghi",
    "Salo",
    "Sandoval",
    "Sato",
    "Selby",
    "Shah",
    "Sorensen",
    "Strand",
    "Takahashi",
    "Tamura",
    "Tanaka",
    "Thorne",
    "Tiwari",
    "Valdes",
    "Varga",
    "Vos",
    "Wainwright",
    "Weller",
    "Whitlock",
    "Wilde",
    "Xu",
    "Yilmaz",
    "Zhou",
    "Zielinski",
];
