//! The ring's town, as written down (`docs/FREE_ROAM.md`, T1): its twelve districts round the rim from
//! Hub Gate, the quarter each belongs to (`docs/STORY.md`, "Inside"), and the blocks given over to a
//! place of the colony's own. The rules that turn these into streets and buildings are `colony::town`'s.
//!
//! Any change here changes what every client draws and walks into, and what the server checks poses
//! against, so it moves [`TOWN_VERSION`] with the protocol's version.

/// The town's version: bumped, with `bc_proto::PROTOCOL_VERSION`, on any change to the layout.
pub const TOWN_VERSION: u8 = 1;

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
