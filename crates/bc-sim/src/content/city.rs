//! The First Colony's city, as written down: each strip's districts from the docking hub's end to
//! the building site, its key places, and the blocks given over to something particular. The rules
//! that turn these into streets and buildings are `colony::city`'s.
//!
//! Any change here changes what every client draws and walks into, and what the server checks
//! poses against, so it moves [`CITY_VERSION`] with the protocol's version.

/// The city's version: bumped, with `bc_proto::PROTOCOL_VERSION`, on any change to the layout (2:
/// the key places' rooms).
pub const CITY_VERSION: u8 = 2;

/// A strip's name, in the colony's own words.
pub const STRIP_NAMES: [&str; 3] = ["CHARTER", "CANAL", "GARDENS"];

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
    University,
    /// Workshops and machine shops.
    Works,
    /// Mostly parkland.
    Park,
    /// Depots and warehouses by the hub.
    Port,
}

/// Each strip's twelve districts, from the docking hub's end (each 16 blocks, 2,048 m).
pub const DISTRICTS: [[DistrictKind; 12]; 3] = {
    use DistrictKind::*;
    [
        [
            Civic,
            Business,
            Business,
            Midtown,
            Midtown,
            Residential,
            Park,
            Residential,
            OldTown,
            University,
            Works,
            Works,
        ],
        [
            Port,
            Residential,
            Midtown,
            OldTown,
            Residential,
            Park,
            Residential,
            Midtown,
            Residential,
            Works,
            Residential,
            Works,
        ],
        [
            Civic,
            Residential,
            Park,
            University,
            Residential,
            Midtown,
            Residential,
            Park,
            Residential,
            Works,
            Residential,
            Works,
        ],
    ]
};

/// The districts' names, by strip and index.
pub const DISTRICT_NAMES: [[&str; 12]; 3] = [
    [
        "CHARTER SQUARE",
        "EXCHANGE ROW",
        "TOWER HILL",
        "MERIDIAN",
        "LANTERN STREET",
        "FIRSTHOMES",
        "CENTRAL PARK",
        "ARRIVAL HEIGHTS",
        "OLD TOWN",
        "THE UNIVERSITY",
        "MACHINE ROW",
        "FOUNDRY LANE",
    ],
    [
        "THE DEPOTS",
        "QUAYSIDE",
        "CANAL CENTRAL",
        "LOCK TOWN",
        "WATERSIDE",
        "THE BASIN",
        "MILLRACE",
        "TWIN BRIDGES",
        "LOWER CANAL",
        "THE YARDS",
        "FAR QUAYS",
        "LAST LOCK",
    ],
    [
        "GARDEN GATE",
        "ORCHARDS",
        "THE ARBORETUM",
        "THE COLLEGES",
        "TERRACES",
        "GREEN MERIDIAN",
        "HILLSIDE",
        "THE MEADOW",
        "VINE STREET",
        "GREENWORKS",
        "FIELDSIDE",
        "SEED HALLS",
    ],
];

/// The places a pilot goes to on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    /// Where the cap lift comes down: a strip's gate to the bays.
    HubGate,
    /// A bar to meet in.
    Bar,
    /// The Colony Exchange's trading floor.
    Exchange,
    /// The Charter Board's hall: the colony's notices.
    Charter,
}

/// A key place: which block it takes (a grid cell: `bx` along, `row` across, + on the far side of
/// the avenue from the strip's edge), and which way its door faces (`door_x` ±1 along the axis, or
/// 0 for across: towards the avenue).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceDef {
    pub kind: PlaceKind,
    pub name: &'static str,
    pub slug: &'static str,
    pub strip: u8,
    pub bx: i32,
    pub row: i32,
    pub door_x: i8,
}

/// The room behind a key place's door, at street level: how deep it runs in from the front, how
/// wide it is along it, and how high its ceiling is, m. Hub Gate's terminal has the lift instead.
pub const fn room_size(kind: PlaceKind) -> Option<(f32, f32, f32)> {
    match kind {
        // A bar: low, warm, a counter along its back.
        PlaceKind::Bar => Some((18.0, 22.0, 5.0)),
        // The trading floor: a hall under a high ceiling, the boards along its back wall.
        PlaceKind::Exchange => Some((36.0, 48.0, 12.0)),
        // The Charter Board's hall: its notices on the back wall.
        PlaceKind::Charter => Some((28.0, 36.0, 9.0)),
        PlaceKind::HubGate => None,
    }
}

/// Every key place. The Hub Gates' terminals stand at the foot of the end cap, on the avenue.
pub const PLACES: [PlaceDef; 6] = [
    PlaceDef {
        kind: PlaceKind::HubGate,
        name: "HUB GATE · CHARTER",
        slug: "hub_gate_1",
        strip: 0,
        bx: 3,
        row: 0,
        door_x: 1,
    },
    PlaceDef {
        kind: PlaceKind::HubGate,
        name: "HUB GATE · CANAL",
        slug: "hub_gate_2",
        strip: 1,
        bx: 3,
        row: 0,
        door_x: 1,
    },
    PlaceDef {
        kind: PlaceKind::HubGate,
        name: "HUB GATE · GARDENS",
        slug: "hub_gate_3",
        strip: 2,
        bx: 3,
        row: 0,
        door_x: 1,
    },
    PlaceDef { kind: PlaceKind::Bar, name: "THE ARRIVAL", slug: "bar", strip: 0, bx: 9, row: -1, door_x: 0 },
    PlaceDef {
        kind: PlaceKind::Exchange,
        name: "THE EXCHANGE FLOOR",
        slug: "exchange_floor",
        strip: 0,
        bx: 10,
        row: 2,
        door_x: -1,
    },
    PlaceDef {
        kind: PlaceKind::Charter,
        name: "THE CHARTER BOARD",
        slug: "charter_board",
        strip: 0,
        bx: 12,
        row: 1,
        door_x: 0,
    },
];

/// What a block is, where the layout says so rather than the district's dice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Special {
    Park,
    Plaza,
    /// One tower taking the block, this tall (m): a landmark on the skyline.
    Tower(f32),
}

/// Blocks given over to something particular: (strip, bx, row, what).
pub const SPECIAL: [(u8, i32, i32, Special); 10] = [
    // The Axis View tower, the tallest thing on any strip: from its top, the whole colony.
    (0, 30, -2, Special::Tower(240.0)),
    // The clock tower over Charter Square.
    (0, 13, -1, Special::Tower(96.0)),
    (0, 11, -1, Special::Plaza),
    (0, 14, 1, Special::Plaza),
    // The canal's basin.
    (1, 88, 3, Special::Plaza),
    (1, 88, 5, Special::Plaza),
    // The meadow's high ground and the arboretum's glasshouse.
    (2, 120, -6, Special::Park),
    (2, 41, 2, Special::Tower(64.0)),
    // The university's quad.
    (0, 156, -3, Special::Plaza),
    // The yards' crane hall.
    (1, 160, -2, Special::Tower(72.0)),
];

/// Sights worth finding, named on the way: (strip, bx, row, name).
pub const SIGHTS: [(u8, i32, i32, &str); 10] = [
    (0, 30, -2, "AXIS VIEW TOWER"),
    (0, 13, -1, "THE CLOCK TOWER"),
    (0, 104, 6, "CENTRAL PARK LAKE"),
    (1, 88, 4, "THE CANAL BASIN"),
    (0, 140, 0, "OLD TOWN GATE"),
    (1, 40, -13, "WINDOW BANK OVERLOOK"),
    (0, 156, -3, "THE UNIVERSITY QUAD"),
    (1, 120, -1, "THE MARKET HALL"),
    (2, 199, 0, "SITE GATE"),
    (2, 248, 0, "FAR CAP VIEW"),
];
