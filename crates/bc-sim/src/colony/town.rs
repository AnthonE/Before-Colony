//! The ring's town (`docs/FREE_ROAM.md`, T1): its streets, blocks and buildings as a closed form of
//! where you ask, as the cylinder's city was. Nothing is stored. A block is worked out from its cell
//! round the ring and its column across it with an integer hash and plain `f32` arithmetic, the same
//! to the bit on every machine, so the server checks a pose against the same walls every client draws
//! and walks.
//!
//! **Round the ring** (`t`, m along the floor from Hub Gate the way the deck's `s` grows, 0 to
//! [`ROUND`]): twelve districts (`content::town::DISTRICTS`) of twelve cells each, [`CELL`] long. A
//! cross street runs at every cell's edge, and an avenue twice as wide between districts. A spoke's lift
//! comes down at the start of every even district, in the boulevard's middle ([`LIFTS`]), its foot a
//! portal the boulevard runs under, with a square round it; Hub Gate's is the first.
//!
//! **Across the ring** (`x`, the colony's own frame, from the hub's side, fore, to aft), a band at a
//! time ([`band_at`]): the fore rim walk along the ring's face, the fore rim road, seven columns of
//! blocks, the boulevard down the middle (the tram on its median), four columns, the canal, two
//! columns of depots on its far quay, the aft rim road and the aft rim walk.
//!
//! **A block** is its cell and column ([`BlockId`]), inset from them by its streets' halves; what
//! stands on it ([`block_use`]) is a square, a park, a key place's site, the building site's frames or
//! the buildings its district's kind and its nearness to a lift build ([`each_block_box`]). The canal's
//! railings fence its water from its quays and its bridges.
//!
//! The walker's frame on the ring's deck is `(x, h, −s)` (`deck::DeckPos::walker`), and [`solid`]
//! takes its boxes: `deck::solid` asks it for the ring's deck.
//!
//! The seam (`deck::Deck::seam`) is still a bulkhead to the deck, at the avenue between Quayside and
//! Lock Town: it opens when walkers wrap round the ring (`docs/FREE_ROAM.md`, P1). The town itself is
//! periodic round the ring and has no seam.

use core::f32::consts::TAU;

use glam::Vec3;

use crate::colony::deck::{DECKS, RING};
use crate::colony::hub::{SPOKES, spoke_angle};
use crate::content::town::{DISTRICTS, DistrictDef, DistrictKind, PLACES, PlaceKind};
use crate::math::{floor, sqrt};
use crate::world::{COLONY_HALF_WIDTH, COLONY_INNER_RADIUS, COLONY_RADIUS};

/// Round the ring at its floor, m.
pub const ROUND: f32 = TAU * COLONY_RADIUS;
/// Districts round the ring, and cells round each.
pub const DISTRICT_COUNT: u16 = 12;
pub const CELLS_PER_DISTRICT: u16 = 12;
/// Cells round the ring.
pub const CELLS: u16 = DISTRICT_COUNT * CELLS_PER_DISTRICT;
/// A cell's length round the ring, m (130.9).
pub const CELL: f32 = ROUND / CELLS as f32;
/// A district's length round the ring, m (1,571).
pub const DISTRICT: f32 = ROUND / DISTRICT_COUNT as f32;
/// The ceiling over the floor, m: nothing stands taller than [`MAX_HEIGHT`] under it.
pub const ROOF: f32 = COLONY_RADIUS - COLONY_INNER_RADIUS;
pub const MAX_HEIGHT: f32 = 200.0;
/// A building's storeys: the ground floor's, and each one over it, m.
pub const GROUND_FLOOR: f32 = 5.0;
pub const FLOOR: f32 = 4.0;

/// Where the town starts round the ring's deck (its `s`, m): the first spoke's lift, Hub Gate.
pub const HUB_GATE_S: f32 = ROUND * 7.0 / 12.0;

/// Streets: a cross street, an avenue between districts, m wide.
pub const STREET: f32 = 20.0;
pub const AVENUE: f32 = 40.0;
/// Pavement between a block and the boulevard or a rim road, and the quay between a block and the
/// canal, m.
pub const PAVEMENT: f32 = 6.0;
pub const QUAY: f32 = 14.0;
/// Between buildings on one block, m (half each side of a lot's line).
pub const ALLEY: f32 = 4.0;

/// The bands across the ring, m along its axis.
pub const RIM_WALK: f32 = 60.0;
pub const RIM_ROAD: f32 = 30.0;
/// The boulevard down the middle: its half width, and its median's (the tram's).
pub const BOULEVARD_HALF: f32 = 32.0;
pub const MEDIAN_HALF: f32 = 8.0;
/// The canal's quays: its fore and aft edges, m along the axis.
pub const CANAL: (f32, f32) = (560.0, 600.0);
/// Where the block columns are: the fore band, the middle band, the aft band (each from, to, and how
/// many columns).
const FORE: (f32, f32, u8) = (-(COLONY_HALF_WIDTH - RIM_WALK - RIM_ROAD), -BOULEVARD_HALF, 7);
const MID: (f32, f32, u8) = (BOULEVARD_HALF, CANAL.0, 4);
const AFT: (f32, f32, u8) = (CANAL.1, COLONY_HALF_WIDTH - RIM_WALK - RIM_ROAD, 2);
/// Columns of blocks across the ring.
pub const COLUMNS: u8 = FORE.2 + MID.2 + AFT.2;
/// The columns either side of the boulevard: the lift squares'.
pub const BOULEVARD_COLUMNS: (u8, u8) = (FORE.2 - 1, FORE.2);

/// The canal's railings, m high and thick.
pub const RAILING: f32 = 1.1;
pub const RAIL_THICKNESS: f32 = 0.3;

/// A lift's foot: a portal over the boulevard, the shaft going up from its lintel to the ceiling.
/// Its half widths across and round, its lintel's height, and its piers' size, m.
pub const LIFT_HALF_X: f32 = BOULEVARD_HALF + 8.0;
pub const LIFT_HALF_T: f32 = 14.0;
pub const LIFT_LINTEL: f32 = 24.0;
pub const LIFT_PIER: f32 = 6.0;
/// The lifts: one down each spoke, at the start of every even district.
pub const LIFTS: u16 = SPOKES as u16;

/// A box of the town in its own coordinates: `x` across (the colony's), `t` round from Hub Gate, `h`
/// up from the floor, m.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TownBox {
    pub x0: f32,
    pub x1: f32,
    pub t0: f32,
    pub t1: f32,
    pub h0: f32,
    pub h1: f32,
}

impl TownBox {
    pub const fn new(x0: f32, x1: f32, t0: f32, t1: f32, h0: f32, h1: f32) -> Self {
        Self { x0, x1, t0, t1, h0, h1 }
    }

    /// Moved round the ring by `dt`.
    fn round_by(self, dt: f32) -> Self {
        Self { t0: self.t0 + dt, t1: self.t1 + dt, ..self }
    }

    /// Whether it and `o` share any volume.
    pub fn overlaps(&self, o: &TownBox) -> bool {
        self.x0 < o.x1
            && o.x0 < self.x1
            && self.t0 < o.t1
            && o.t0 < self.t1
            && self.h0 < o.h1
            && o.h0 < self.h1
    }
}

/// A block: its cell round the ring (0 to [`CELLS`]) and its column across it (0 to [`COLUMNS`],
/// fore to aft).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId {
    pub cell: u16,
    pub column: u8,
}

impl BlockId {
    pub const fn new(cell: u16, column: u8) -> Self {
        Self { cell: cell % CELLS, column }
    }

    /// Its district (0 to 11).
    pub const fn district(self) -> u16 {
        self.cell / CELLS_PER_DISTRICT
    }
}

/// What stands on a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockUse {
    /// The buildings its district builds.
    Built,
    /// Grass and trees, a pocket park or a park district's.
    Park,
    /// A lift's square.
    Square,
    /// A key place's site, built from pieces.
    Place(PlaceKind),
    /// The building site's frames.
    Frames,
}

/// What lies across the ring at `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    RimWalk,
    RimRoad,
    /// A column of blocks (and the streets between them).
    Blocks(u8),
    Boulevard,
    Canal,
    /// Past the ring's faces.
    Outside,
}

/// What a point of the floor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// On a block, inside its kerbs.
    Block(BlockId),
    /// A road's carriageway: the boulevard's either side of its median, a rim road, a cross street or
    /// an avenue between blocks.
    Road,
    /// The boulevard's median, where the tram runs.
    Median,
    /// Pavement or a quay: along a road between it and the blocks, or along the canal.
    Pavement,
    /// The canal's water, between its railings.
    Water,
    /// The rim walks along the faces.
    RimWalk,
    /// Past the ring's faces.
    Outside,
}

/// A column's ends across the ring, and its insets from them (pavement, quay or half a street), m.
pub fn column(c: u8) -> (f32, f32, f32, f32) {
    let (band, k) = if c < FORE.2 {
        (FORE, c)
    } else if c < FORE.2 + MID.2 {
        (MID, c - FORE.2)
    } else {
        (AFT, c - FORE.2 - MID.2)
    };
    let w = (band.1 - band.0) / f32::from(band.2);
    let x0 = band.0 + w * f32::from(k);
    let x1 = if k + 1 == band.2 { band.1 } else { x0 + w };
    // A band's first and last columns face the road or the canal beyond it; the rest a street.
    let edge = |x: f32| if x == CANAL.0 || x == CANAL.1 { QUAY } else { PAVEMENT };
    let lo = if k == 0 { edge(band.0) } else { STREET * 0.5 };
    let hi = if k + 1 == band.2 { edge(band.1) } else { STREET * 0.5 };
    (x0, x1, lo, hi)
}

/// What's across the ring at `x`.
pub fn band_at(x: f32) -> Band {
    let a = x.abs();
    if a > COLONY_HALF_WIDTH {
        return Band::Outside;
    }
    if a > COLONY_HALF_WIDTH - RIM_WALK {
        return Band::RimWalk;
    }
    if a > COLONY_HALF_WIDTH - RIM_WALK - RIM_ROAD {
        return Band::RimRoad;
    }
    if a < BOULEVARD_HALF {
        return Band::Boulevard;
    }
    if (CANAL.0..CANAL.1).contains(&x) {
        return Band::Canal;
    }
    let (band, first) = if x < 0.0 {
        (FORE, 0)
    } else if x < CANAL.0 {
        (MID, FORE.2)
    } else {
        (AFT, FORE.2 + MID.2)
    };
    let w = (band.1 - band.0) / f32::from(band.2);
    let k = (floor((x - band.0) / w) as i32).clamp(0, i32::from(band.2) - 1) as u8;
    Band::Blocks(first + k)
}

/// `t` brought round into `0..ROUND`.
pub fn wrap(t: f32) -> f32 {
    let w = t - floor(t / ROUND) * ROUND;
    if w >= ROUND { 0.0 } else { w }
}

/// The town's `t` of a point on the ring's deck at `s` (`deck::DeckPos::s`).
pub fn town_t(s: f32) -> f32 {
    wrap(s - HUB_GATE_S)
}

/// The cell round the ring at `t` (any `t`: it wraps).
pub fn cell_at(t: f32) -> u16 {
    ((floor(wrap(t) / CELL) as i32).clamp(0, i32::from(CELLS) - 1)) as u16
}

/// District `d`'s definition.
pub fn district(d: u16) -> &'static DistrictDef {
    &DISTRICTS[usize::from(d % DISTRICT_COUNT)]
}

/// Lift `k`'s foot round the ring, `t`: the start of district `2k`.
pub fn lift_t(k: u16) -> f32 {
    DISTRICT * f32::from(2 * (k % LIFTS))
}

/// How far `t` is round the ring from the nearest lift's foot, m.
pub fn from_lift(t: f32) -> f32 {
    let p = ROUND / f32::from(LIFTS);
    let d = wrap(t) - floor(wrap(t) / p) * p;
    d.min(p - d)
}

/// How far a cell's ends are inset for the streets at them: an avenue's half where a district starts
/// or ends, a street's elsewhere.
fn cell_insets(cell: u16) -> (f32, f32) {
    let k = cell % CELLS_PER_DISTRICT;
    let lo = if k == 0 { AVENUE * 0.5 } else { STREET * 0.5 };
    let hi = if k + 1 == CELLS_PER_DISTRICT { AVENUE * 0.5 } else { STREET * 0.5 };
    (lo, hi)
}

/// A block's ground inside its kerbs: across (`x0..x1`) and round (`t0..t1`), m, with `h` 0.
pub fn block_rect(id: BlockId) -> TownBox {
    let (x0, x1, xl, xh) = column(id.column);
    let (tl, th) = cell_insets(id.cell);
    let t0 = CELL * f32::from(id.cell);
    TownBox::new(x0 + xl, x1 - xh, t0 + tl, t0 + CELL - th, 0.0, 0.0)
}

/// The Watch's station in district `d`: halfway round it, aft of the boulevard.
pub const fn watch_station(d: u16) -> BlockId {
    BlockId::new((d % DISTRICT_COUNT) * CELLS_PER_DISTRICT + CELLS_PER_DISTRICT / 2, FORE.2)
}

/// Whether a block is one of a lift's square's four, either side of the boulevard and of the avenue
/// at an even district's start.
pub fn is_square(id: BlockId) -> bool {
    let k = id.cell % CELLS_PER_DISTRICT;
    let d = id.district();
    let at_lift =
        (k == 0 && d.is_multiple_of(2)) || (k + 1 == CELLS_PER_DISTRICT && (d + 1).is_multiple_of(2));
    at_lift && (id.column == BOULEVARD_COLUMNS.0 || id.column == BOULEVARD_COLUMNS.1)
}

/// The key place on a block, if one is.
pub fn place_at(id: BlockId) -> Option<PlaceKind> {
    if watch_station(id.district()) == id {
        return Some(PlaceKind::WatchStation);
    }
    PLACES.iter().find(|p| p.cell == id.cell && p.column == id.column).map(|p| p.kind)
}

/// What stands on a block.
pub fn block_use(id: BlockId) -> BlockUse {
    if is_square(id) {
        return BlockUse::Square;
    }
    if let Some(p) = place_at(id) {
        return BlockUse::Place(p);
    }
    match district(id.district()).kind {
        DistrictKind::Park => BlockUse::Park,
        DistrictKind::Site => BlockUse::Frames,
        // Pocket parks, one block in about sixteen where people live and work.
        DistrictKind::Residential | DistrictKind::Midtown | DistrictKind::OldTown
            if unit(hash(id), 1) < 0.0625 =>
        {
            BlockUse::Park
        }
        _ => BlockUse::Built,
    }
}

/// Most boxes a block stands as.
pub const MAX_BLOCK_BOXES: usize = 8;

/// Every box block `id` stands as (none for a square, a park or a key place's site), in the town's
/// coordinates round its own cell.
pub fn each_block_box(id: BlockId, f: &mut impl FnMut(TownBox)) {
    let r = block_rect(id);
    let h = hash(id);
    let kind = district(id.district()).kind;
    // Nearer a lift, taller: up to 2.5× for the money and the middle of town, 1.5× elsewhere.
    let mx = 0.5 * (r.x0 + r.x1);
    let mt = 0.5 * (r.t0 + r.t1);
    let near = (1.0 - sqrt(mx * mx + from_lift(mt) * from_lift(mt)) / 700.0).max(0.0);
    let tall = |storeys: f32, gain: f32| {
        let n = floor(storeys * (1.0 + gain * near)).max(1.0);
        (GROUND_FLOOR + FLOOR * (n - 1.0)).min(MAX_HEIGHT)
    };
    let span = |a: f32, b: f32, k: u32| a + (b - a) * unit(h, k);
    match block_use(id) {
        BlockUse::Square | BlockUse::Park | BlockUse::Place(_) => {}
        BlockUse::Frames => {
            // The corner columns of a frame going up, and a crane's mast on one block in three.
            let top = tall(span(6.0, 20.0, 2), 0.0);
            let c = 1.2;
            let s = r.inset(4.0);
            for (x, t) in [(s.x0, s.t0), (s.x1 - c, s.t0), (s.x0, s.t1 - c), (s.x1 - c, s.t1 - c)] {
                f(TownBox::new(x, x + c, t, t + c, 0.0, top));
            }
            if unit(h, 3) < 1.0 / 3.0 {
                let (x, t) = (0.5 * (s.x0 + s.x1), 0.5 * (s.t0 + s.t1));
                f(TownBox::new(x - 2.0, x + 2.0, t - 2.0, t + 2.0, 0.0, (top + 30.0).min(MAX_HEIGHT)));
            }
        }
        BlockUse::Built => match kind {
            DistrictKind::Business => {
                // A podium over the whole block, a tower on its middle.
                let p = r.inset(2.0);
                f(TownBox::new(p.x0, p.x1, p.t0, p.t1, 0.0, tall(span(3.0, 5.0, 2), 0.0)));
                let tw = p.inset(0.2 * (p.x1 - p.x0).min(p.t1 - p.t0));
                f(TownBox::new(tw.x0, tw.x1, tw.t0, tw.t1, 0.0, tall(span(15.0, 40.0, 3), 1.5)));
            }
            DistrictKind::Civic => {
                let p = r.inset(4.0);
                f(TownBox::new(p.x0, p.x1, p.t0, p.t1, 0.0, tall(span(3.0, 8.0, 2), 0.5)));
            }
            DistrictKind::Works => {
                // A shed over the block, or two.
                let p = r.inset(3.0);
                let top = tall(span(2.0, 3.0, 2), 0.0);
                if unit(h, 3) < 0.5 {
                    f(TownBox::new(p.x0, p.x1, p.t0, p.t1, 0.0, top));
                } else {
                    lots(p, 2, 1, h, &mut |b, k| f(TownBox { h1: tall(span(2.0, 3.0, 4 + k), 0.0), ..b }));
                }
            }
            DistrictKind::OldTown => {
                lots(r, 2, 2, h, &mut |b, k| f(TownBox { h1: tall(span(2.0, 4.0, 4 + k), 0.2), ..b }));
            }
            DistrictKind::Midtown => {
                lots(r, 2, 1, h, &mut |b, k| f(TownBox { h1: tall(span(6.0, 18.0, 4 + k), 1.5), ..b }));
            }
            DistrictKind::Residential => {
                let n = if unit(h, 3) < 0.5 { 1 } else { 2 };
                lots(r, 2, n, h, &mut |b, k| f(TownBox { h1: tall(span(4.0, 12.0, 4 + k), 0.5), ..b }));
            }
            DistrictKind::Park | DistrictKind::Site => {}
        },
    }
}

/// Block `r` cut into lots, `along` its longer side and `across` the other, each with an alley's half
/// round it; `f` takes each lot's footprint (its `h1` for the caller to set) and its number.
fn lots(r: TownBox, along: u32, across: u32, h: u32, f: &mut impl FnMut(TownBox, u32)) {
    let (lx, lt) = if r.x1 - r.x0 >= r.t1 - r.t0 { (along, across) } else { (across, along) };
    let a = ALLEY * 0.5;
    // Each line between lots is off its even place by up to a tenth of the block, by the hash.
    let cut = |lo: f32, hi: f32, n: u32, i: u32, k: u32| {
        if i == 0 {
            lo
        } else if i == n {
            hi
        } else {
            let even = lo + (hi - lo) * i as f32 / n as f32;
            even + (unit(h, 16 + k * 4 + i) - 0.5) * 0.2 * (hi - lo) / n as f32
        }
    };
    let mut k = 0;
    for i in 0..lx {
        for j in 0..lt {
            let (x0, x1) = (cut(r.x0, r.x1, lx, i, 0), cut(r.x0, r.x1, lx, i + 1, 0));
            let (t0, t1) = (cut(r.t0, r.t1, lt, j, 1), cut(r.t0, r.t1, lt, j + 1, 1));
            f(TownBox::new(x0 + a, x1 - a, t0 + a, t1 - a, 0.0, 0.0), k);
            k += 1;
        }
    }
}

impl TownBox {
    /// Smaller by `d` across and round (not up).
    pub fn inset(&self, d: f32) -> TownBox {
        TownBox { x0: self.x0 + d, x1: self.x1 - d, t0: self.t0 + d, t1: self.t1 - d, ..*self }
    }
}

/// Lift `k`'s foot: its four piers and the shaft over its lintel, up to the ceiling.
pub fn each_lift_box(k: u16, f: &mut impl FnMut(TownBox)) {
    let t = lift_t(k);
    let (hx, ht, p) = (LIFT_HALF_X, LIFT_HALF_T, LIFT_PIER);
    for (x0, t0) in [(-hx, t - ht), (hx - p, t - ht), (-hx, t + ht - p), (hx - p, t + ht - p)] {
        f(TownBox::new(x0, x0 + p, t0, t0 + p, 0.0, LIFT_LINTEL));
    }
    f(TownBox::new(-hx, hx, t - ht, t + ht, LIFT_LINTEL, ROOF));
}

/// The canal's railings in cell `cell`'s length: along both its quays, and along both sides of the
/// bridges at the cell's ends (a cross street's or an avenue's).
pub fn each_rail(cell: u16, f: &mut impl FnMut(TownBox)) {
    let (lo, hi) = cell_insets(cell);
    let t0 = CELL * f32::from(cell % CELLS) + lo;
    let t1 = CELL * f32::from(cell % CELLS + 1) - hi;
    let (c0, c1, r) = (CANAL.0, CANAL.1, RAIL_THICKNESS);
    f(TownBox::new(c0, c0 + r, t0, t1, 0.0, RAILING));
    f(TownBox::new(c1 - r, c1, t0, t1, 0.0, RAILING));
    f(TownBox::new(c0, c1, t0 - r, t0, 0.0, RAILING));
    f(TownBox::new(c0, c1, t1, t1 + r, 0.0, RAILING));
}

/// Every box of the town that might reach into `x0..x1` across and `t0..t1` round (any `t`, either
/// side of Hub Gate: it wraps, and the boxes come back in the same turn of the ring as asked). Blocks,
/// lifts and the canal's railings.
pub fn each_box(x0: f32, x1: f32, t0: f32, t1: f32, f: &mut impl FnMut(TownBox)) {
    let (c0, c1) = (floor(t0 / CELL) as i32, floor(t1 / CELL) as i32);
    let cells = i32::from(CELLS);
    // The block columns the span crosses.
    let first = match band_at(x0.max(-COLONY_HALF_WIDTH)) {
        Band::Blocks(c) => c,
        _ if x0 < 0.0 => 0,
        _ if x0 < CANAL.1 => FORE.2,
        _ => FORE.2 + MID.2,
    };
    for cu in c0..=c1 {
        let cell = cu.rem_euclid(cells) as u16;
        let dt = (cu - i32::from(cell)) as f32 * CELL;
        for c in first..COLUMNS {
            let (cx0, _, _, _) = column(c);
            if cx0 > x1 {
                break;
            }
            each_block_box(BlockId::new(cell, c), &mut |b| f(b.round_by(dt)));
        }
        if x0 < CANAL.1 && x1 > CANAL.0 {
            each_rail(cell, &mut |b| f(b.round_by(dt)));
        }
    }
    // The lifts' feet, standing across the avenues at even districts' starts, in reach of the span.
    if x0 < LIFT_HALF_X && x1 > -LIFT_HALF_X {
        let p = ROUND / f32::from(LIFTS);
        let lifts = i32::from(LIFTS);
        for i in -(floor(-(t0 - LIFT_HALF_T) / p) as i32)..=floor((t1 + LIFT_HALF_T) / p) as i32 {
            let k = i.rem_euclid(lifts);
            each_lift_box(k as u16, &mut |b| f(b.round_by((i - k) as f32 * p)));
        }
    }
}

/// Whether the box `min..max` (the walker's frame on the ring's deck, `(x, h, −s)`) touches anything of
/// the town standing on its floor.
pub fn solid(min: Vec3, max: Vec3) -> bool {
    let t0 = min_t(-max.z);
    let q = TownBox::new(min.x, max.x, t0, t0 + (max.z - min.z), min.y, max.y);
    let mut hit = false;
    each_box(q.x0, q.x1, q.t0, q.t1, &mut |b| hit |= b.overlaps(&q));
    hit
}

/// The town's `t` for the deck's `s`, kept in the same turn as `s` (not wrapped): the start of a
/// query's span, whose end is its length on.
fn min_t(s: f32) -> f32 {
    s - HUB_GATE_S
}

/// What the floor is at `x` across and `t` round (any `t`: it wraps).
pub fn ground_at(x: f32, t: f32) -> Ground {
    let t = wrap(t);
    match band_at(x) {
        Band::Outside => Ground::Outside,
        Band::RimWalk => Ground::RimWalk,
        Band::RimRoad => Ground::Road,
        Band::Boulevard => {
            if x.abs() < MEDIAN_HALF {
                Ground::Median
            } else {
                Ground::Road
            }
        }
        Band::Canal => {
            let cell = cell_at(t);
            let (lo, hi) = cell_insets(cell);
            let c0 = CELL * f32::from(cell);
            if t < c0 + lo || t > c0 + CELL - hi { Ground::Road } else { Ground::Water }
        }
        Band::Blocks(c) => {
            let id = BlockId::new(cell_at(t), c);
            let r = block_rect(id);
            if (r.x0..=r.x1).contains(&x) && (r.t0..=r.t1).contains(&t) {
                return Ground::Block(id);
            }
            // Off the block: round it, a cross street or an avenue; across it, a street between columns,
            // or the pavement or quay along its band's edge.
            if t < r.t0 || t > r.t1 {
                return Ground::Road;
            }
            let (_, _, xl, xh) = column(c);
            let half = if x < r.x0 { xl } else { xh };
            if half == STREET * 0.5 { Ground::Road } else { Ground::Pavement }
        }
    }
}

/// Where on the ring's deck (its `s`, m) the town's `t` is.
pub fn deck_s(t: f32) -> f32 {
    let s = t + HUB_GATE_S;
    let round = DECKS[RING as usize].round();
    s - floor(s / round) * round
}

/// Lift `k`'s spoke's angle round the axis, from +Y towards +Z, before the spin (`hub::spoke_angle`).
pub fn lift_angle(k: u16) -> f32 {
    spoke_angle(u32::from(k % LIFTS))
}

fn hash(id: BlockId) -> u32 {
    mix(u32::from(id.cell), u32::from(id.column), 0x70_57)
}

/// An integer hash, the same on every machine.
pub fn mix(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

/// A number in `0..1` from `h` and `k`.
pub fn unit(h: u32, k: u32) -> f32 {
    (mix(h, k, 0x5EED) >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
mod tests {
    use super::*;
    use crate::colony::deck::DeckPos;
    use crate::math::Rng;
    use crate::world::COLONY_HALF_WIDTH as HALF;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Every box of every block, with its block.
    fn all_block_boxes(mut f: impl FnMut(BlockId, TownBox)) {
        for cell in 0..CELLS {
            for c in 0..COLUMNS {
                let id = BlockId::new(cell, c);
                each_block_box(id, &mut |b| f(id, b));
            }
        }
    }

    #[test]
    fn hub_gate_and_every_lift_are_under_their_spokes() {
        let d = &DECKS[RING as usize];
        for k in 0..LIFTS {
            let a = lift_angle(k) - d.seam;
            let s = (a - floor(a / TAU) * TAU) * d.floor_r;
            assert!((deck_s(lift_t(k)) - s).abs() < 0.05, "lift {k}: {} vs {s}", deck_s(lift_t(k)));
            // At the boulevard's middle, the foot's on the floor there.
            let p = DeckPos::new(RING, 0.0, deck_s(lift_t(k)), 0.0).angle();
            let want = lift_angle(k).rem_euclid(TAU);
            assert!((p.rem_euclid(TAU) - want).abs() < 1e-4, "lift {k}");
        }
        assert_eq!(deck_s(0.0), HUB_GATE_S);
        assert!(town_t(HUB_GATE_S) < 1e-3 && (town_t(HUB_GATE_S - 1.0) - (ROUND - 1.0)).abs() < 2e-3);
        // A lift's foot at every other district's start.
        for k in 0..LIFTS {
            assert_eq!(cell_at(lift_t(k) + 1.0) % CELLS_PER_DISTRICT, 0);
            assert_eq!((cell_at(lift_t(k) + 1.0) / CELLS_PER_DISTRICT) % 2, 0);
        }
    }

    #[test]
    fn the_columns_and_cells_tile_the_ring() {
        assert!((CELL * f32::from(CELLS) - ROUND).abs() < 0.01);
        assert!((DISTRICT - 1_570.8).abs() < 0.1 && (CELL - 130.9).abs() < 0.1);
        assert_eq!(DISTRICTS.len(), usize::from(DISTRICT_COUNT));
        let mut last = -HALF + RIM_WALK + RIM_ROAD;
        for c in 0..COLUMNS {
            let (x0, x1, lo, hi) = column(c);
            // Each column starts where the last ended, or across the boulevard or the canal.
            let gap = x0 - last;
            assert!(
                gap.abs() < 1e-3
                    || (gap - 2.0 * BOULEVARD_HALF).abs() < 1e-3
                    || (gap - (CANAL.1 - CANAL.0)).abs() < 1e-3,
                "column {c}: {gap}"
            );
            last = x1;
            assert!(x1 - x0 - lo - hi > 90.0, "column {c} is {} m of block", x1 - x0 - lo - hi);
            assert_eq!(band_at(0.5 * (x0 + x1)), Band::Blocks(c));
        }
        assert!((last - (HALF - RIM_WALK - RIM_ROAD)).abs() < 1e-3);
        assert_eq!(band_at(0.0), Band::Boulevard);
        assert_eq!(band_at(580.0), Band::Canal);
        assert_eq!(band_at(-HALF + 30.0), Band::RimWalk);
        assert_eq!(band_at(HALF - 75.0), Band::RimRoad);
        assert_eq!(band_at(HALF + 1.0), Band::Outside);
        for t in [-1.0f32, 0.0, CELL - 0.01, CELL, ROUND - 0.01, ROUND, ROUND + CELL + 1.0] {
            let c = cell_at(t);
            let w = wrap(t);
            assert!(c < CELLS && (0.0..ROUND).contains(&w), "{t}");
            assert!(CELL * f32::from(c) <= w + 1e-2 && w < CELL * f32::from(c + 1) + 1e-2, "{t}: cell {c}");
        }
    }

    #[test]
    fn every_building_stands_on_its_block_under_the_ceiling() {
        let (mut n, mut built, mut tallest) = (0, 0, 0.0f32);
        let mut per = [[0usize; COLUMNS as usize]; CELLS as usize];
        all_block_boxes(|id, b| {
            let r = block_rect(id);
            assert!(
                b.x0 >= r.x0 - 1e-3 && b.x1 <= r.x1 + 1e-3 && b.t0 >= r.t0 - 1e-3 && b.t1 <= r.t1 + 1e-3,
                "{id:?}: {b:?} off {r:?}"
            );
            assert!(
                b.h0 == 0.0 && b.h1 > b.h0 && b.h1 <= MAX_HEIGHT && b.x1 > b.x0 && b.t1 > b.t0,
                "{id:?}: {b:?}"
            );
            per[usize::from(id.cell)][usize::from(id.column)] += 1;
            n += 1;
            tallest = tallest.max(b.h1);
        });
        for cell in 0..CELLS {
            for c in 0..COLUMNS {
                let k = per[usize::from(cell)][usize::from(c)];
                let id = BlockId::new(cell, c);
                assert!(k <= MAX_BLOCK_BOXES, "{id:?}: {k} boxes");
                match block_use(id) {
                    BlockUse::Built | BlockUse::Frames => {
                        assert!(k > 0, "{id:?} stands empty");
                        built += 1;
                    }
                    _ => assert_eq!(k, 0, "{id:?}: {:?} has buildings", block_use(id)),
                }
            }
        }
        assert!(built > 1_400 && n > 3_000, "{built} blocks built, {n} boxes");
        assert!(tallest > 150.0 && MAX_HEIGHT < ROOF - 200.0, "the tallest is {tallest} m");
    }

    #[test]
    fn towers_rise_round_the_lifts() {
        // The money's towers within 700 m of a lift stand taller than the rest of its blocks.
        let (mut near, mut far) = ((0.0f32, 0u32), (0.0f32, 0u32));
        all_block_boxes(|id, b| {
            if district(id.district()).kind != DistrictKind::Business
                && district(id.district()).kind != DistrictKind::Midtown
            {
                return;
            }
            let r = block_rect(id);
            let (mx, mt) = (0.5 * (r.x0 + r.x1), 0.5 * (r.t0 + r.t1));
            let d = sqrt(mx * mx + from_lift(mt) * from_lift(mt));
            let acc = if d < 400.0 {
                &mut near
            } else if d > 800.0 {
                &mut far
            } else {
                return;
            };
            acc.0 += b.h1;
            acc.1 += 1;
        });
        let (near, far) = (near.0 / near.1 as f32, far.0 / far.1 as f32);
        assert!(near > far * 1.5, "near the lifts {near} m, away {far} m");
    }

    #[test]
    fn the_squares_and_the_places_have_their_blocks() {
        let mut squares = 0;
        for cell in 0..CELLS {
            for c in 0..COLUMNS {
                squares += u32::from(block_use(BlockId::new(cell, c)) == BlockUse::Square);
            }
        }
        assert_eq!(squares, 4 * u32::from(LIFTS));
        for p in &PLACES {
            let id = BlockId::new(p.cell, p.column);
            assert_eq!(block_use(id), BlockUse::Place(p.kind), "{p:?}");
            // Round Hub Gate's square.
            let r = block_rect(id);
            let t = 0.5 * (r.t0 + r.t1);
            assert!(t.min(ROUND - t) < 400.0, "{p:?} is {t} m round");
        }
        for d in 0..DISTRICT_COUNT {
            let id = watch_station(d);
            assert_eq!(id.district(), d);
            assert_eq!(block_use(id), BlockUse::Place(PlaceKind::WatchStation), "district {d}");
            // On the boulevard, so its cars are out on the main road at once.
            assert_eq!(column(id.column).0, BOULEVARD_HALF);
        }
        // No two places share a block.
        for (i, a) in PLACES.iter().enumerate() {
            for b in &PLACES[i + 1..] {
                assert!((a.cell, a.column) != (b.cell, b.column));
            }
            assert_ne!(place_at(BlockId::new(a.cell, a.column)), Some(PlaceKind::WatchStation));
        }
    }

    #[test]
    fn the_lifts_stand_clear_of_the_boulevard_and_the_blocks() {
        for k in 0..LIFTS {
            each_lift_box(k, &mut |b| {
                // Under its lintel, the boulevard and its pavements run on.
                if b.h0 < LIFT_LINTEL {
                    assert!(b.x0.abs().min(b.x1.abs()) > BOULEVARD_HALF, "lift {k}: a pier on the road");
                }
                assert!(b.h1 <= ROOF);
                all_block_boxes(|id, o| {
                    for turn in [-ROUND, 0.0, ROUND] {
                        assert!(!b.overlaps(&o.round_by(turn)), "lift {k} meets {id:?}");
                    }
                });
            });
        }
    }

    #[test]
    fn the_floor_is_what_the_plan_says() {
        let id = BlockId::new(30, 3);
        let r = block_rect(id);
        assert_eq!(ground_at(0.5 * (r.x0 + r.x1), 0.5 * (r.t0 + r.t1)), Ground::Block(id));
        // The same block a turn round, or a turn back.
        assert_eq!(ground_at(0.5 * (r.x0 + r.x1), 0.5 * (r.t0 + r.t1) + ROUND), Ground::Block(id));
        assert_eq!(ground_at(0.5 * (r.x0 + r.x1), r.t0 - 3.0), Ground::Road);
        assert_eq!(ground_at(r.x1 + 3.0, 0.5 * (r.t0 + r.t1)), Ground::Road);
        assert_eq!(ground_at(0.0, 500.0), Ground::Median);
        assert_eq!(ground_at(20.0, 500.0), Ground::Road);
        assert_eq!(ground_at(-BOULEVARD_HALF - 3.0, 500.0), Ground::Pavement);
        assert_eq!(ground_at(580.0, CELL * 30.5), Ground::Water);
        assert_eq!(ground_at(580.0, CELL * 30.0 + 1.0), Ground::Road);
        assert_eq!(ground_at(CANAL.1 + 5.0, CELL * 30.5), Ground::Pavement);
        assert_eq!(ground_at(-HALF + 10.0, 0.0), Ground::RimWalk);
        assert_eq!(ground_at(HALF - 75.0, 0.0), Ground::Road);
        assert_eq!(ground_at(HALF + 10.0, 0.0), Ground::Outside);
    }

    /// The floor on a grid `step` m square, round the whole ring: whether a person can stand there
    /// (off the water, out of every box), and whether it's a road a car may drive.
    fn grid(step: f32) -> (usize, usize, Vec<bool>, Vec<bool>) {
        let nx = (2.0 * HALF / step) as usize;
        let nt = (ROUND / step) as usize;
        let (mut walk, mut road) = (vec![false; nx * nt], vec![false; nx * nt]);
        for j in 0..nt {
            let t = (j as f32 + 0.5) * step;
            for i in 0..nx {
                let x = -HALF + (i as f32 + 0.5) * step;
                let g = ground_at(x, t);
                let q = TownBox::new(x - 0.3, x + 0.3, t - 0.3, t + 0.3, 0.1, 1.8);
                let mut free = true;
                each_box(q.x0, q.x1, q.t0, q.t1, &mut |b| free &= !b.overlaps(&q));
                walk[j * nx + i] = free && !matches!(g, Ground::Water | Ground::Outside);
                road[j * nx + i] = free && g == Ground::Road;
            }
        }
        (nx, nt, walk, road)
    }

    /// Everything reached from `start` over `ok`, four ways, wrapping round the ring.
    fn flood(nx: usize, nt: usize, ok: &[bool], start: usize) -> Vec<bool> {
        let mut seen = vec![false; ok.len()];
        let mut todo = vec![start];
        seen[start] = ok[start];
        while let Some(k) = todo.pop() {
            let (i, j) = (k % nx, k / nx);
            let mut go = |n: usize| {
                if ok[n] && !seen[n] {
                    seen[n] = true;
                    todo.push(n);
                }
            };
            if i > 0 {
                go(k - 1);
            }
            if i + 1 < nx {
                go(k + 1);
            }
            go(((j + nt - 1) % nt) * nx + i);
            go(((j + 1) % nt) * nx + i);
        }
        seen
    }

    #[test]
    fn a_walker_and_a_car_reach_every_district_from_hub_gate() {
        let step = 4.0;
        let (nx, nt, walk, road) = grid(step);
        let at = |x: f32, t: f32| ((wrap(t) / step) as usize % nt) * nx + ((x + HALF) / step) as usize;
        // From the boulevard at Hub Gate, under the lift's lintel.
        let start = at(20.0, 30.0);
        assert!(walk[start] && road[start]);
        let (on_foot, by_car) = (flood(nx, nt, &walk, start), flood(nx, nt, &road, start));
        for d in 0..DISTRICT_COUNT {
            // Every block's kerb on foot, and the road outside the Watch's station by car.
            for cell in d * CELLS_PER_DISTRICT..(d + 1) * CELLS_PER_DISTRICT {
                for c in 0..COLUMNS {
                    let r = block_rect(BlockId::new(cell, c));
                    let kerb = at(0.5 * (r.x0 + r.x1), r.t0 - 2.0);
                    assert!(on_foot[kerb], "district {d}: block ({cell}, {c})'s kerb");
                }
            }
            let w = block_rect(watch_station(d));
            let out = at(BOULEVARD_HALF - 12.0, 0.5 * (w.t0 + w.t1));
            assert!(by_car[out], "district {d}: the road outside the Watch's station");
        }
        // Most of the floor that can be walked is reached.
        let (reached, open) = (on_foot.iter().filter(|r| **r).count(), walk.iter().filter(|w| **w).count());
        assert!(reached as f32 > open as f32 * 0.98, "{reached} of {open}");
    }

    #[test]
    fn solid_is_every_box_it_touches_and_wraps_round_the_ring() {
        let mut every = Vec::new();
        each_box(-HALF, HALF, 0.0, ROUND, &mut |b| every.push(b));
        let mut rng = Rng::new(11);
        let mut hits = 0;
        for k in 0..6_000 {
            // A walker's box anywhere on the floor, some astride Hub Gate (s either side of it).
            let s = if k % 3 == 0 { HUB_GATE_S + rng.signed() * 60.0 } else { rng.next_f32() * ROUND };
            let x = rng.signed() * HALF;
            let h = rng.next_f32() * 30.0;
            let (min, max) = (Vec3::new(x - 0.3, h, -s - 0.3), Vec3::new(x + 0.3, h + 1.8, -s + 0.3));
            let t0 = town_t(s - 0.3);
            let q = TownBox::new(min.x, max.x, t0, t0 + 0.6, min.y, max.y);
            let want = every.iter().any(|b| [-ROUND, 0.0, ROUND].iter().any(|d| b.round_by(*d).overlaps(&q)));
            assert_eq!(solid(min, max), want, "{min} {max}");
            hits += u32::from(want);
        }
        assert!(hits > 500, "{hits} hits");
        // The same boxes a turn round.
        let (mut a, mut b) = (Vec::new(), Vec::new());
        each_box(-HALF, HALF, 1_000.0, 1_400.0, &mut |x| a.push(x));
        each_box(-HALF, HALF, 1_000.0 + ROUND, 1_400.0 + ROUND, &mut |x| b.push(x.round_by(-ROUND)));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x.t0 - y.t0).abs() < 0.01 && x.x0 == y.x0 && x.h1 == y.h1, "{x:?} vs {y:?}");
        }
    }
}
