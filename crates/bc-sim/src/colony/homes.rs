//! Homes: the flats and houses inside the city's buildings, each at its own address, as a closed
//! form of the building it's in (`docs/OPEN_CITY.md`, H0). Nothing of them is stored: a building's
//! lobby, stairs, corridors and flats are worked out from its foot and its storeys wherever they're
//! asked for, the same to the bit on the server and in the browser, so the plaza checks a pose
//! against the same walls every client walks. Who holds which home is the register's
//! (`bc_econ::homes`), not this.
//!
//! A **block of flats** ([`Kind::Flats`], a Residential district's buildings of one box): its lobby
//! door in the middle of a street side, inside it the core (the landing; the lift on one side; the
//! stairs on the other, two flights a storey round a landing; a service room either side of those),
//! and from the core to the back wall the corridor with the flats either side of it, each with its
//! door on the corridor. A **row of houses** ([`Kind::Houses`], Old Town's): the building cut along
//! its street side into houses, each with its front door on the street and a straight stair against
//! its party wall.
//!
//! In a building's own frame ([`Plan`]): `u` along its front from the front's middle (`+u` is `+s`
//! or `+x`, as [`Front`] has it), `v` in from the inside of its front wall. Storey `k`'s floor is
//! [`level`]`(k)`; the building's body roof is `level(storeys)`.
//!
//! A building is solid boxes to whatever meets it: a hollow one is hundreds of them, so it hands out
//! only those near what asks ([`Plan::each_box_near`], the city's fine grain), and to a suit or the
//! shaders' map it's still the envelope it was (`city::each_solid`).

use crate::colony::city::{
    BlockInfo, BlockKind, Building, CityBox, FLOOR, Form, Front, GROUND_FLOOR, KERB, Lots, MAX_SOLIDS, Piece,
    Rect, SIDE_S0, SIDE_S1, SIDE_X0, SIDE_X1, SIDEWALK, Stage, Style, block, block_index, floors_height,
    lots_near, row_at, storeys_under,
};
use crate::content::city::DistrictKind;

/// Outer walls, m thick.
pub const SHELL: f32 = 0.3;
/// A floor's slab, m thick (under each storey above the ground, and the roof's).
pub const SLAB: f32 = 0.3;
/// Partitions: between the corridor and a flat, between flats.
pub const PART: f32 = 0.2;
/// Between two houses of a row, m thick.
pub const PARTY: f32 = 0.3;
/// The corridor down the middle of a block of flats (and its landing and lobby), m wide.
pub const SPINE: f32 = 2.4;
/// A flight's width, the wall between a stairwell's two flights, and the stairwell's.
pub const LANE: f32 = 1.2;
pub const WELL_WALL: f32 = 0.2;
pub const WELL: f32 = 2.0 * LANE + WELL_WALL;
/// The stairwell's landing at each floor, m deep (where its flights start and end).
pub const NEAR: f32 = 1.4;
/// The core across the front: the landing, the lift and the stairs, m deep.
pub const CORE: f32 = 6.2;
/// A stair's tread, m deep; its rise, m at most; and how thick each tread's slab is beneath it.
pub const TREAD: f32 = 0.28;
pub const RISER: f32 = 0.2;
pub const STEP_SLAB: f32 = 0.35;
/// A flat's door: m wide and high; the lobby's.
pub const DOOR_WIDTH: f32 = 1.0;
pub const DOOR_HEIGHT: f32 = 2.2;
pub const LOBBY_WIDTH: f32 = 1.8;
pub const LOBBY_HEIGHT: f32 = 2.6;
/// How far along the corridor a flat takes, m (as near as the corridor's length divides).
pub const FLAT_ALONG: f32 = 8.0;
/// The most flats along each side of a corridor.
pub const MAX_ALONG: u32 = 7;
/// A house's front, m (as near as the row divides), and how deep it goes back from the street.
pub const HOUSE_FRONT: f32 = 7.0;
pub const HOUSE_DEPTH: f32 = 14.0;
/// A house's stair: its flight's width, and where it starts in from the front wall, m.
pub const HOUSE_LANE: f32 = 1.0;
pub const HOUSE_STAIR: f32 = 1.2;
/// A house's front door, m wide (as high as a flat's).
pub const HOUSE_DOOR: f32 = 1.2;
/// A banister's height and thickness, m.
pub const RAIL: f32 = 1.0;
pub const RAIL_THICKNESS: f32 = 0.1;
/// Storeys a block of flats has (fewest, most), and a row of houses.
pub const FLATS_STOREYS: (u32, u32) = (2, 10);
pub const HOUSE_STOREYS: (u32, u32) = (2, 4);
/// Where a pilot stands to use a lift, and how near a pose must be to be at it, m.
pub const LIFT_REACH: f32 = 1.2;
/// The windows in a home building's outer walls: one a bay along each wall (as near as the wall
/// divides), this wide, from a sill this high over the floor to a head this high (the ground
/// floor's higher), m. They're glazed: to whatever meets them they're the wall.
pub const BAY: f32 = 3.0;
pub const WINDOW: f32 = 1.5;
pub const SILL: f32 = 0.9;
pub const HEAD: f32 = 2.4;
pub const GROUND_HEAD: f32 = 3.4;

/// What a home building is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Flats,
    Houses,
}

/// Storey `k`'s floor, m up: the street's kerb for the ground floor.
pub fn level(k: u32) -> f32 {
    if k == 0 { KERB } else { floors_height(k) }
}

/// A home, by where it is: its strip, block, lot, storey and unit, in 29 bits. A house's storey is
/// 0 (it has them all).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HomeId(pub u32);

impl HomeId {
    const HOUSE: u32 = 1 << 28;

    pub fn new(strip: u8, bx: i32, row: i32, lot: u8, storey: u32, unit: u32, house: bool) -> Self {
        let row = (row + 12).clamp(0, 31) as u32;
        Self(
            (unit & 15)
                | (storey & 31) << 4
                | u32::from(lot & 15) << 9
                | row << 13
                | (bx.clamp(0, 255) as u32) << 18
                | u32::from(strip & 3) << 26
                | if house { Self::HOUSE } else { 0 },
        )
    }

    pub fn unit(self) -> u32 {
        self.0 & 15
    }

    pub fn storey(self) -> u32 {
        (self.0 >> 4) & 31
    }

    pub fn lot(self) -> u8 {
        ((self.0 >> 9) & 15) as u8
    }

    pub fn row(self) -> i32 {
        ((self.0 >> 13) & 31) as i32 - 12
    }

    pub fn bx(self) -> i32 {
        ((self.0 >> 18) & 255) as i32
    }

    pub fn strip(self) -> u8 {
        ((self.0 >> 26) & 3) as u8
    }

    pub fn house(self) -> bool {
        self.0 & Self::HOUSE != 0
    }
}

/// How a home building is laid out: its frame, its storeys, and how its floors divide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub kind: Kind,
    pub strip: u8,
    pub bx: i32,
    pub row: i32,
    pub lot: u8,
    pub district: DistrictKind,
    pub foot: Rect,
    /// Its body's roof, m up.
    pub height: f32,
    pub storeys: u32,
    pub front: Front,
    /// Inside its outer walls: across its front, and in from it, m.
    pub width: f32,
    pub depth: f32,
    /// How deep the houses go (a row's: what's behind them is solid); a block of flats', its depth.
    pub homes_depth: f32,
    /// Flats along each side of the corridor; or houses in the row.
    pub along: u32,
    /// Its lot's sides on a street.
    pub sides: u8,
}

/// What a point inside a home building is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// The lobby, the landings, the stairs, a corridor.
    Common,
    /// A home.
    Home(HomeId),
}

/// What's across the street from a home's windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Another building, close by.
    Yard,
    Street,
    Plaza,
    Park,
    Canal,
    /// The avenue down the strip's middle, the trams on it.
    Avenue,
    /// The window bank: the park at the glass, and the colony beyond it.
    Window,
}

/// One home: where it is, its floor and its door.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Home {
    pub id: HomeId,
    pub kind: Kind,
    pub strip: u8,
    pub district: DistrictKind,
    /// Its floor, inside its walls.
    pub rect: Rect,
    /// Its lowest floor and its top ceiling, m up.
    pub floor: f32,
    pub ceiling: f32,
    /// Its storeys (a flat has one).
    pub storeys: u32,
    /// Its door: the gap in its wall, and how high it is from its floor.
    pub door: Rect,
    /// Where you stand at its door outside, and just inside: `(s, x)`.
    pub outside: (f32, f32),
    pub inside: (f32, f32),
    /// The way in, through its door: a unit `(ds, dx)`.
    pub inward: (f32, f32),
    /// Its floor's area (all of a house's storeys), m².
    pub area: f32,
    /// A flat on its building's top storey.
    pub top: bool,
    pub view: View,
}

/// A stair's risers from storey `k`'s floor to the next: how many, and how high each is.
fn risers(k: u32) -> (u32, f32) {
    let rise = level(k + 1) - level(k);
    let n = ((rise - 1e-4) / RISER) as u32 + 1;
    (n, rise / n as f32)
}

/// A straight flight's length from storey `k`'s floor to the next (a house's), m.
pub fn run(k: u32) -> f32 {
    (risers(k).0 - 1) as f32 * TREAD
}

/// A switchback's first flight from storey `k` (the larger half of its risers) and its second.
fn halves(k: u32) -> (u32, u32, f32) {
    let (n, r) = risers(k);
    let m1 = n.div_ceil(2);
    (m1, n - m1, r)
}

/// Where a switchback's far landing starts, in from the core's front (storey `k`'s).
fn far_landing(k: u32) -> f32 {
    NEAR + (halves(k).0 - 1) as f32 * TREAD
}

/// Where a switchback's second flight from storey `k` ends: the next floor's landing from there on.
fn second_ends(k: u32) -> f32 {
    let (_, m2, _) = halves(k);
    far_landing(k) - (m2 - 1) as f32 * TREAD
}

/// `r` less `hole`: up to four rectangles, laid in `out`; how many.
fn minus(r: Rect, hole: Rect, out: &mut [Rect; 4]) -> usize {
    if !r.overlaps(&hole) {
        out[0] = r;
        return 1;
    }
    let h = Rect::new(hole.s0.max(r.s0), hole.s1.min(r.s1), hole.x0.max(r.x0), hole.x1.min(r.x1));
    let mut n = 0;
    for q in [
        Rect::new(r.s0, h.s0, r.x0, r.x1),
        Rect::new(h.s1, r.s1, r.x0, r.x1),
        Rect::new(h.s0, h.s1, r.x0, h.x0),
        Rect::new(h.s0, h.s1, h.x1, r.x1),
    ] {
        if q.width() > 1e-3 && q.length() > 1e-3 {
            out[n] = q;
            n += 1;
        }
    }
    n
}

/// A lot's sides on a street, from where its building stands in its block.
fn street_sides(b: &BlockInfo, bd: &Building) -> u8 {
    let area = b.rect.inset(SIDEWALK);
    // A lot's building stands its district's gap's half in from the lot's edge: never more than 5 m.
    let near = |a: f32, c: f32| (a - c).abs() < 5.5;
    let f = bd.foot;
    (if near(f.s0, area.s0) { SIDE_S0 } else { 0 })
        | if near(f.s1, area.s1) { SIDE_S1 } else { 0 }
        | if near(f.x0, area.x0) { SIDE_X0 } else { 0 }
        | if near(f.x1, area.x1) { SIDE_X1 } else { 0 }
}

/// The front on side `side` of `foot` (one of `SIDE_*`).
fn front_on(foot: Rect, side: u8) -> Front {
    let (ms, mx) = foot.middle();
    match side {
        SIDE_X0 => Front { along_s: true, face: foot.x0, middle: ms, sign: 1.0 },
        SIDE_X1 => Front { along_s: true, face: foot.x1, middle: ms, sign: -1.0 },
        SIDE_S0 => Front { along_s: false, face: foot.s0, middle: mx, sign: 1.0 },
        _ => Front { along_s: false, face: foot.s1, middle: mx, sign: -1.0 },
    }
}

/// Building `bd` of block `b`'s homes, if it has any: how it's laid out.
pub fn plan(b: &BlockInfo, bd: &Building) -> Option<Plan> {
    if b.kind != BlockKind::Buildings || bd.room.is_some() || !matches!(bd.style, Style::Plain | Style::Slab)
    {
        return None;
    }
    if !matches!(bd.form, Form::Block | Form::Setback { .. }) {
        return None;
    }
    let (_, district) = b.district?;
    let storeys = storeys_under(bd.height + 0.01);
    if (level(storeys) - bd.height).abs() > 0.01 {
        return None;
    }
    let sides = street_sides(b, bd);
    let foot = bd.foot;
    let long_x = foot.length() >= foot.width();
    let pick = |order: [u8; 4], need_street: bool| {
        order.into_iter().find(|s| sides & s != 0).or(if need_street { None } else { Some(order[0]) })
    };
    let (kind, side) = match district {
        DistrictKind::Residential if (FLATS_STOREYS.0..=FLATS_STOREYS.1).contains(&storeys) => {
            // The corridor the long way: the lobby on a short side.
            let order = if long_x {
                [SIDE_X0, SIDE_X1, SIDE_S0, SIDE_S1]
            } else {
                [SIDE_S0, SIDE_S1, SIDE_X0, SIDE_X1]
            };
            (Kind::Flats, pick(order, false)?)
        }
        DistrictKind::OldTown if (HOUSE_STOREYS.0..=HOUSE_STOREYS.1).contains(&storeys) => {
            // The row along a long side, on its street.
            let order = if long_x {
                [SIDE_S0, SIDE_S1, SIDE_X0, SIDE_X1]
            } else {
                [SIDE_X0, SIDE_X1, SIDE_S0, SIDE_S1]
            };
            (Kind::Houses, pick(order, true)?)
        }
        _ => return None,
    };
    let front = front_on(foot, side);
    let (across, deep) =
        if front.along_s { (foot.width(), foot.length()) } else { (foot.length(), foot.width()) };
    let (width, depth) = (across - 2.0 * SHELL, deep - 2.0 * SHELL);
    let (homes_depth, along) = match kind {
        Kind::Flats => {
            let side_width = width * 0.5 - SPINE * 0.5 - PART;
            if width * 0.5 < SPINE * 0.5 + PART + WELL + 1.0 || side_width < 4.0 || depth < CORE + 5.0 {
                return None;
            }
            let along = (((depth - CORE) / FLAT_ALONG + 0.5) as u32).clamp(1, MAX_ALONG);
            (depth, along)
        }
        Kind::Houses => {
            let along = ((width / HOUSE_FRONT + 0.5) as u32).clamp(1, 15);
            let d = if depth > HOUSE_DEPTH + 2.0 { HOUSE_DEPTH } else { depth };
            if width / along as f32 - PARTY < 4.5 || d < HOUSE_STAIR + run(0) + 1.5 {
                return None;
            }
            (d, along)
        }
    };
    Some(Plan {
        kind,
        strip: b.strip,
        bx: b.bx,
        row: b.row,
        lot: bd.lot,
        district,
        foot,
        height: bd.height,
        storeys,
        front,
        width,
        depth,
        homes_depth,
        along,
        sides,
    })
}

/// The home building at `(s, x)` on strip `strip`, if the point's in one: its plan.
pub fn plan_at(strip: u8, s: f32, x: f32) -> Option<Plan> {
    let b = block(strip, block_index(x), row_at(s), Stage(0))?;
    if b.kind != BlockKind::Buildings || !b.rect.contains(s, x) {
        return None;
    }
    let lots: Lots = lots_near(&b, &Rect::new(s, s, x, x));
    lots.as_slice().iter().find(|bd| bd.foot.contains(s, x)).and_then(|bd| plan(&b, bd))
}

/// What's at `(s, x)`, `h` up on strip `strip`, if it's inside a home building: the building, and
/// the space the point is in (none inside a wall or a floor).
pub fn inside(strip: u8, s: f32, x: f32, h: f32) -> Option<(Plan, Option<Space>)> {
    let p = plan_at(strip, s, x)?;
    (KERB - 0.5..p.height).contains(&h).then(|| (p, p.space_at(s, x, h)))
}

/// Calls `f` with each home building on strip `strip` whose foot comes within `reach` of `(s, x)`.
pub fn each_plan_near(strip: u8, s: f32, x: f32, reach: f32, mut f: impl FnMut(&Plan)) {
    let area = Rect::new(s - reach, s + reach, x - reach, x + reach);
    for bx in block_index(area.x0)..=block_index(area.x1) {
        for row in row_at(area.s0)..=row_at(area.s1) {
            let Some(b) = block(strip, bx, row, Stage(0)) else { continue };
            if b.kind != BlockKind::Buildings || !b.rect.overlaps(&area) {
                continue;
            }
            for bd in lots_near(&b, &area).as_slice() {
                if bd.foot.overlaps(&area)
                    && let Some(p) = plan(&b, bd)
                {
                    f(&p);
                }
            }
        }
    }
}

/// The home at `(s, x)`, `h` up on strip `strip`, if the point's inside one.
pub fn home_at(strip: u8, s: f32, x: f32, h: f32) -> Option<HomeId> {
    match inside(strip, s, x, h)?.1? {
        Space::Home(id) => Some(id),
        Space::Common => None,
    }
}

/// Whether two points of strip `strip` (`(s, x, h)`) are in the same space: both out of the homes'
/// buildings, or in the same building's same storey and the same home (or both in its common parts).
/// Who's on another floor, or behind a home's walls, can't be seen from here.
pub fn same_space(strip: u8, a: (f32, f32, f32), b: (f32, f32, f32)) -> bool {
    let at = |(s, x, h): (f32, f32, f32)| {
        inside(strip, s, x, h).map(|(p, space)| (p.bx, p.row, p.lot, p.storey_at(h), space))
    };
    at(a) == at(b)
}

/// Home `id`, if there's one there.
pub fn home(id: HomeId) -> Option<Home> {
    let b = block(id.strip(), id.bx(), id.row(), Stage(0))?;
    if b.kind != BlockKind::Buildings {
        return None;
    }
    let lots: Lots = lots_near(&b, &b.rect);
    let bd = lots.as_slice().iter().find(|bd| bd.lot == id.lot())?;
    plan(&b, bd)?.home(id.storey(), id.unit())
}

impl Plan {
    /// A box `u0..u1` along the front and `v0..v1` in from the inside of its front wall, `h0..h1` up.
    fn cb(&self, u0: f32, u1: f32, v0: f32, v1: f32, h0: f32, h1: f32) -> CityBox {
        CityBox { rect: self.front.rect(u0, u1, SHELL + v0, SHELL + v1), h0, h1 }
    }

    /// A point `u` along and `v` in, as `(s, x)`.
    pub fn point(&self, u: f32, v: f32) -> (f32, f32) {
        self.front.point(u, SHELL + v)
    }

    /// A point `(s, x)` in its frame, `(u, v)`.
    pub fn local(&self, s: f32, x: f32) -> (f32, f32) {
        let f = &self.front;
        let (along, across) = if f.along_s { (s, x) } else { (x, s) };
        (along - f.middle, (across - f.face) * f.sign - SHELL)
    }

    /// The way in from its front, `(ds, dx)`; and `+u`.
    fn inward(&self) -> (f32, f32) {
        if self.front.along_s { (0.0, self.front.sign) } else { (self.front.sign, 0.0) }
    }

    fn along_u(&self) -> (f32, f32) {
        if self.front.along_s { (1.0, 0.0) } else { (0.0, 1.0) }
    }

    /// Its windows along a wall `a..b` long (`u` or `v`, the wall's own way): each window's middle,
    /// `n` of them; how many.
    pub fn windows(a: f32, b: f32, out: &mut [f32; 32]) -> usize {
        let n = (((b - a - 0.6) / BAY) as usize).min(out.len());
        let margin = (b - a - n as f32 * BAY) * 0.5;
        for (i, w) in out.iter_mut().take(n).enumerate() {
            *w = a + margin + (i as f32 + 0.5) * BAY;
        }
        n
    }

    /// Its doors in its front wall, across the front: `(u0, u1, how high)`; how many.
    pub fn front_doors(&self, out: &mut [(f32, f32, f32); 16]) -> usize {
        match self.kind {
            Kind::Flats => {
                out[0] = (-LOBBY_WIDTH * 0.5, LOBBY_WIDTH * 0.5, LOBBY_HEIGHT);
                1
            }
            Kind::Houses => {
                for i in 0..self.along {
                    let (a, b) = self.house_door(i);
                    out[i as usize] = (a, b, DOOR_HEIGHT);
                }
                self.along as usize
            }
        }
    }

    /// Which storey `h` is on (the top storey for anything above it).
    pub fn storey_at(&self, h: f32) -> u32 {
        if h < GROUND_FLOOR { 0 } else { (((h - GROUND_FLOOR) / FLOOR) as u32 + 1).min(self.storeys - 1) }
    }

    /// The flats' boundaries along the corridor: flat `j` runs from the `j`th to the `j + 1`th.
    fn bound(&self, j: u32) -> f32 {
        CORE + (self.depth - CORE) * j as f32 / self.along as f32
    }

    /// House `i`'s extent across the front, between its party walls' middles.
    fn house_span(&self, i: u32) -> (f32, f32) {
        let hw = self.width / self.along as f32;
        let u0 = -self.width * 0.5 + hw * i as f32;
        (u0, u0 + hw)
    }

    /// House `i`'s floor across the front, inside its party walls.
    fn house_inside(&self, i: u32) -> (f32, f32) {
        let (u0, u1) = self.house_span(i);
        let a = if i > 0 { PARTY * 0.5 } else { 0.0 };
        let b = if i + 1 < self.along { PARTY * 0.5 } else { 0.0 };
        (u0 + a, u1 - b)
    }

    /// Flat `unit`'s floor across the front and in from it (`u0, u1, v0, v1`).
    fn flat_rect(&self, unit: u32) -> (f32, f32, f32, f32) {
        let (side, j) = (unit / self.along, unit % self.along);
        let v0 = self.bound(j) + if j == 0 { PART } else { PART * 0.5 };
        let v1 = self.bound(j + 1) - if j + 1 == self.along { 0.0 } else { PART * 0.5 };
        let (a, b) = (SPINE * 0.5 + PART, self.width * 0.5);
        if side == 0 { (a, b, v0, v1) } else { (-b, -a, v0, v1) }
    }

    /// Flat `unit`'s door in the corridor's wall, along it (`v0, v1`).
    fn flat_door(&self, unit: u32) -> (f32, f32) {
        let v0 = self.bound(unit % self.along) + PART + 0.5;
        (v0, v0 + DOOR_WIDTH)
    }

    /// House `i`'s front door, across the front (`u0, u1`): at the far side from its stair.
    fn house_door(&self, i: u32) -> (f32, f32) {
        let (_, u1) = self.house_inside(i);
        (u1 - 0.4 - HOUSE_DOOR, u1 - 0.4)
    }

    /// House `i`'s stair's flight, across the front (`u0, u1`).
    fn house_lane(&self, i: u32) -> (f32, f32) {
        let (u0, _) = self.house_inside(i);
        (u0, u0 + HOUSE_LANE)
    }

    /// How many homes a storey has (a row of houses: how many houses).
    pub fn homes_per_storey(&self) -> u32 {
        match self.kind {
            Kind::Flats => 2 * self.along,
            Kind::Houses => self.along,
        }
    }

    pub fn id(&self, storey: u32, unit: u32) -> HomeId {
        let house = self.kind == Kind::Houses;
        HomeId::new(self.strip, self.bx, self.row, self.lot, if house { 0 } else { storey }, unit, house)
    }

    /// The space at `(s, x)`, `h` up: a home, the common parts, or nothing (a wall, a floor).
    pub fn space_at(&self, s: f32, x: f32, h: f32) -> Option<Space> {
        let (u, v) = self.local(s, x);
        let w = self.width * 0.5;
        if u.abs() > w || !(0.0..self.depth).contains(&v) {
            return None;
        }
        let k = self.storey_at(h);
        match self.kind {
            Kind::Flats => {
                if u.abs() <= SPINE * 0.5 {
                    return Some(Space::Common);
                }
                let a = SPINE * 0.5 + PART;
                if v < CORE {
                    return (a..a + WELL).contains(&u).then_some(Space::Common);
                }
                let side = u32::from(u < 0.0);
                let j = (((v - CORE) / (self.depth - CORE) * self.along as f32) as u32).min(self.along - 1);
                let unit = side * self.along + j;
                let (u0, u1, v0, v1) = self.flat_rect(unit);
                ((u0..u1).contains(&u) && (v0..v1).contains(&v)).then(|| Space::Home(self.id(k, unit)))
            }
            Kind::Houses => {
                if v >= self.homes_depth {
                    return None;
                }
                (0..self.along)
                    .find(|&i| {
                        let (u0, u1) = self.house_inside(i);
                        (u0..u1).contains(&u)
                    })
                    .map(|i| Space::Home(self.id(0, i)))
            }
        }
    }

    /// Home `unit` on storey `storey` (a house's storey is ignored).
    pub fn home(&self, storey: u32, unit: u32) -> Option<Home> {
        if unit >= self.homes_per_storey() || (self.kind == Kind::Flats && storey >= self.storeys) {
            return None;
        }
        let id = self.id(storey, unit);
        let inward = self.inward();
        let (au, ax) = self.along_u();
        match self.kind {
            Kind::Flats => {
                let (u0, u1, v0, v1) = self.flat_rect(unit);
                let (d0, d1) = self.flat_door(unit);
                let right = u0 > 0.0;
                let (wu0, wu1) = if right { (SPINE * 0.5, u0) } else { (u1, -SPINE * 0.5) };
                let door = self.front.rect(wu0, wu1, SHELL + d0, SHELL + d1);
                let mid = (d0 + d1) * 0.5;
                let (out_u, in_u) =
                    if right { (SPINE * 0.5 - 0.6, u0 + 0.7) } else { (-SPINE * 0.5 + 0.6, u1 - 0.7) };
                let sign = if right { 1.0 } else { -1.0 };
                Some(Home {
                    id,
                    kind: self.kind,
                    strip: self.strip,
                    district: self.district,
                    rect: self.front.rect(u0, u1, SHELL + v0, SHELL + v1),
                    floor: level(storey),
                    ceiling: level(storey + 1) - SLAB,
                    storeys: 1,
                    door,
                    outside: self.point(out_u, mid),
                    inside: self.point(in_u, mid),
                    inward: (au * sign, ax * sign),
                    area: (u1 - u0) * (v1 - v0),
                    top: storey + 1 == self.storeys,
                    view: self.view(if right { 1 } else { -1 }),
                })
            }
            Kind::Houses => {
                let (u0, u1) = self.house_inside(unit);
                let (d0, d1) = self.house_door(unit);
                let mid = (d0 + d1) * 0.5;
                Some(Home {
                    id,
                    kind: self.kind,
                    strip: self.strip,
                    district: self.district,
                    rect: self.front.rect(u0, u1, SHELL, SHELL + self.homes_depth),
                    floor: KERB,
                    ceiling: self.height - SLAB,
                    storeys: self.storeys,
                    door: self.front.rect(d0, d1, 0.0, SHELL),
                    outside: self.point(mid, -SHELL - 0.8),
                    inside: self.point(mid, 0.7),
                    inward,
                    area: (u1 - u0) * self.homes_depth * self.storeys as f32,
                    top: false,
                    view: self.view(0),
                })
            }
        }
    }

    /// What a home's windows look out on: a flat's on its side of the building (`side` +1 or −1
    /// along the front), a house's from its front (0).
    fn view(&self, side: i32) -> View {
        let f = &self.front;
        // The side's bit, and the neighbouring cell that way.
        let bit = match (side, f.along_s) {
            (0, true) => {
                if f.sign > 0.0 {
                    SIDE_X0
                } else {
                    SIDE_X1
                }
            }
            (0, false) => {
                if f.sign > 0.0 {
                    SIDE_S0
                } else {
                    SIDE_S1
                }
            }
            (1, true) => SIDE_S1,
            (_, true) => SIDE_S0,
            (1, false) => SIDE_X1,
            (_, false) => SIDE_X0,
        };
        if self.sides & bit == 0 {
            return View::Yard;
        }
        let (bx, row) = match bit {
            SIDE_S0 => (self.bx, self.row - 1),
            SIDE_S1 => (self.bx, self.row + 1),
            SIDE_X0 => (self.bx - 1, self.row),
            _ => (self.bx + 1, self.row),
        };
        if row == 0 {
            return View::Avenue;
        }
        if row.abs() >= crate::colony::city::BANK_ROW {
            return View::Window;
        }
        match block(self.strip, bx, row, Stage(0)).map(|b| b.kind) {
            Some(BlockKind::Park) => View::Park,
            Some(BlockKind::Canal) => View::Canal,
            Some(BlockKind::Plaza) => View::Plaza,
            _ => View::Street,
        }
    }

    /// The way up the stairs from storey `k`'s floor to the next, as points to walk through: `(s,
    /// x)` and the floor there. A block of flats' switchback from its landing (the first flight in,
    /// across the far landing, the second back out); a house's (`house`) straight flight from its
    /// foot, and back along beside it to the next one's.
    pub fn stairs_up(&self, k: u32, house: u32) -> [((f32, f32), f32); 4] {
        let (lk, lk1) = (level(k), level(k + 1));
        let at = |u: f32, v: f32, h: f32| (self.point(u, v), h);
        match self.kind {
            Kind::Flats => {
                let (a, b) = (SPINE * 0.5 + PART + LANE * 0.5, SPINE * 0.5 + PART + LANE * 1.5 + WELL_WALL);
                let (m1, _, r) = halves(k);
                let far = far_landing(k) + 0.6;
                let hl = lk + m1 as f32 * r;
                [at(a, NEAR * 0.5, lk), at(a, far, hl), at(b, far, hl), at(b, NEAR * 0.5, lk1)]
            }
            Kind::Houses => {
                let (l0, l1) = self.house_lane(house);
                let (mid, top, foot) = ((l0 + l1) * 0.5, HOUSE_STAIR + run(k) + 0.6, HOUSE_STAIR - 0.4);
                [at(mid, foot, lk), at(mid, top, lk1), at(l1 + 0.5, top, lk1), at(l1 + 0.5, foot, lk1)]
            }
        }
    }

    /// Where a pilot stands at the lift on storey `k`: `(s, x)`, and its floor. A block of flats'
    /// only.
    pub fn lift_spot(&self, k: u32) -> Option<((f32, f32), f32)> {
        (self.kind == Kind::Flats && k < self.storeys)
            .then(|| (self.point(-SPINE * 0.5 + 0.6, 1.2), level(k)))
    }

    /// The storey whose lift `(s, x)`, `h` up is at, if it's at one.
    pub fn at_lift(&self, s: f32, x: f32, h: f32) -> Option<u32> {
        let k = self.storey_at(h + 0.3);
        let ((ls, lx), floor) = self.lift_spot(k)?;
        let (ds, dx) = (s - ls, x - lx);
        (ds * ds + dx * dx <= LIFT_REACH * LIFT_REACH && (h - floor).abs() < 0.6).then_some(k)
    }

    /// Calls `f` with each of its outer walls (its doors cut through the front one) and its roof's
    /// slab; stops when `f` says so, and says whether it did.
    pub fn each_shell(&self, f: &mut impl FnMut(&CityBox) -> bool) -> bool {
        let (w, d, top) = (self.width * 0.5, self.depth, self.height);
        let walls = [
            self.cb(-w - SHELL, w + SHELL, d, d + SHELL, KERB, top),
            self.cb(-w - SHELL, -w, -SHELL, d + SHELL, KERB, top),
            self.cb(w, w + SHELL, -SHELL, d + SHELL, KERB, top),
            self.cb(-w, w, 0.0, d, top - SLAB, top),
        ];
        for b in &walls {
            if f(b) {
                return true;
            }
        }
        // The front wall, either side of its doors, and over them.
        let mut doors = [(0.0f32, 0.0f32, 0.0f32); 16];
        let n = self.front_doors(&mut doors);
        let mut from = -w - SHELL;
        for &(a, b, high) in &doors[..n] {
            for piece in
                [self.cb(from, a, -SHELL, 0.0, KERB, top), self.cb(a, b, -SHELL, 0.0, KERB + high, top)]
            {
                if f(&piece) {
                    return true;
                }
            }
            from = b;
        }
        f(&self.cb(from, w + SHELL, -SHELL, 0.0, KERB, top))
    }

    /// Calls `f` with each box of storey `k`: its floor's slab (above the ground), its stairs up,
    /// its partitions, its service rooms; stops when `f` says so, and says whether it did.
    pub fn each_on_storey(&self, k: u32, f: &mut impl FnMut(&CityBox) -> bool) -> bool {
        let (lk, lk1) = (level(k), level(k + 1));
        let w = self.width * 0.5;
        let top = k + 1 == self.storeys;
        let mut emit = |b: CityBox| f(&b);
        match self.kind {
            Kind::Flats => {
                let a0 = SPINE * 0.5 + PART;
                let (a1, b0) = (a0 + LANE, a0 + LANE + WELL_WALL);
                let b1 = b0 + LANE;
                // The floor, round the stairwell's hole (from the landing in); under the second
                // flight up to this floor, as far as that flight comes.
                if k > 0 {
                    let floor = Rect::new(-w, w, 0.0, self.depth);
                    let hole = Rect::new(a0, b1, NEAR, CORE);
                    let mut parts = [Rect::default(); 4];
                    let n = minus(floor, hole, &mut parts);
                    for r in &parts[..n] {
                        if emit(self.cb(r.s0, r.s1, r.x0, r.x1, lk - SLAB, lk)) {
                            return true;
                        }
                    }
                    let ends = second_ends(k - 1);
                    if ends > NEAR && emit(self.cb(b0, b1, NEAR, ends, lk - SLAB, lk)) {
                        return true;
                    }
                }
                // The stairs up: the first flight in from the landing, the far landing, the second
                // flight back out to the next floor's landing.
                if !top {
                    let (m1, m2, r) = halves(k);
                    for j in 1..m1 {
                        let (v0, h) = (NEAR + (j - 1) as f32 * TREAD, lk + j as f32 * r);
                        if emit(self.cb(a0, a1, v0, v0 + TREAD, h - STEP_SLAB, h)) {
                            return true;
                        }
                    }
                    let far = far_landing(k);
                    let hl = lk + m1 as f32 * r;
                    if emit(self.cb(a0, b1, far, CORE, hl - STEP_SLAB, hl)) {
                        return true;
                    }
                    for j in 1..m2 {
                        let (v1, h) = (far - (j - 1) as f32 * TREAD, lk + (m1 + j) as f32 * r);
                        if emit(self.cb(b0, b1, v1 - TREAD, v1, h - STEP_SLAB, h)) {
                            return true;
                        }
                    }
                } else if emit(self.cb(a0, a1, NEAR, NEAR + RAIL_THICKNESS, lk, lk + RAIL)) {
                    // At the top, nothing further up: a rail across the first flight's way.
                    return true;
                }
                let core = [
                    // Between the flights.
                    self.cb(a1, b0, NEAR, CORE.min(far_landing(k.min(self.storeys - 1))), lk, lk1),
                    // Between the landing and the stairs, but at the stairwell's own landing.
                    self.cb(SPINE * 0.5, a0, NEAR, CORE, lk, lk1),
                    // The service room past the stairs, and the lift and its service room.
                    self.cb(b1, w, 0.0, CORE, lk, lk1),
                    self.cb(-w, -SPINE * 0.5, 0.0, CORE, lk, lk1),
                ];
                for b in core {
                    if b.rect.width() > 1e-3 && b.rect.length() > 1e-3 && emit(b) {
                        return true;
                    }
                }
                // The flats: the walls between them and from the core, and the corridor's walls with
                // their doors.
                for side in [1.0f32, -1.0] {
                    let (o0, o1) = if side > 0.0 { (SPINE * 0.5, w) } else { (-w, -SPINE * 0.5) };
                    for j in 0..self.along {
                        let v = self.bound(j);
                        let (c0, c1) = if j == 0 { (v, v + PART) } else { (v - PART * 0.5, v + PART * 0.5) };
                        if emit(self.cb(o0, o1, c0, c1, lk, lk1)) {
                            return true;
                        }
                        let unit = if side > 0.0 { j } else { self.along + j };
                        let (d0, d1) = self.flat_door(unit);
                        let (p0, p1) = if side > 0.0 { (SPINE * 0.5, a0) } else { (-a0, -SPINE * 0.5) };
                        for b in [
                            self.cb(p0, p1, v, d0, lk, lk1),
                            self.cb(p0, p1, d1, self.bound(j + 1), lk, lk1),
                            self.cb(p0, p1, d0, d1, lk + DOOR_HEIGHT, lk1),
                        ] {
                            if emit(b) {
                                return true;
                            }
                        }
                    }
                }
            }
            Kind::Houses => {
                let back = self.homes_depth;
                if self.depth > back + 0.01 && emit(self.cb(-w, w, back, self.depth, lk, lk1)) {
                    return true;
                }
                for i in 0..self.along {
                    let (u0, u1) = self.house_inside(i);
                    let (l0, l1) = self.house_lane(i);
                    if i + 1 < self.along {
                        let (_, s1) = self.house_span(i);
                        if emit(self.cb(s1 - PARTY * 0.5, s1 + PARTY * 0.5, 0.0, back, lk, lk1)) {
                            return true;
                        }
                    }
                    if k > 0 {
                        // The floor, round the stair coming up to it.
                        let hole = Rect::new(l0, l1, HOUSE_STAIR, HOUSE_STAIR + run(k - 1));
                        let mut parts = [Rect::default(); 4];
                        let n = minus(Rect::new(u0, u1, 0.0, back), hole, &mut parts);
                        for r in &parts[..n] {
                            if emit(self.cb(r.s0, r.s1, r.x0, r.x1, lk - SLAB, lk)) {
                                return true;
                            }
                        }
                        // A banister beside the stair's hole, and at the top across its foot too.
                        let from = HOUSE_STAIR + if top { 0.0 } else { 1.0 };
                        let to = HOUSE_STAIR + run(k - 1) - 0.2;
                        if emit(self.cb(l1, l1 + RAIL_THICKNESS, from, to, lk, lk + RAIL)) {
                            return true;
                        }
                        if top
                            && emit(self.cb(l0, l1, HOUSE_STAIR - RAIL_THICKNESS, HOUSE_STAIR, lk, lk + RAIL))
                        {
                            return true;
                        }
                    }
                    if !top {
                        let (n, r) = risers(k);
                        for j in 1..n {
                            let (v0, h) = (HOUSE_STAIR + (j - 1) as f32 * TREAD, lk + j as f32 * r);
                            if emit(self.cb(l0, l1, v0, v0 + TREAD, h - STEP_SLAB, h)) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Calls `f` with every box of it: its walls, its roof's slab, and every storey's.
    pub fn each_box(&self, f: &mut impl FnMut(&CityBox) -> bool) -> bool {
        self.each_shell(f) || (0..self.storeys).any(|k| self.each_on_storey(k, f))
    }

    /// Calls `f` with each of its boxes that could touch `area` between `h0` and `h1` (the storeys
    /// those heights reach, and the walls round them); stops when `f` says so, and says whether it did.
    pub fn each_box_near(&self, area: &Rect, h0: f32, h1: f32, f: &mut impl FnMut(&CityBox) -> bool) -> bool {
        let mut near = |b: &CityBox| b.rect.overlaps(area) && h0 < b.h1 && b.h0 < h1 && f(b);
        if self.each_shell(&mut near) {
            return true;
        }
        if h1 < KERB - 1.0 || h0 > self.height {
            return false;
        }
        // A storey's boxes reach from its slab under its floor to the next floor.
        let k0 = self.storey_at(h0 - STEP_SLAB);
        let k1 = self.storey_at(h1 + SLAB);
        (k0..=k1).any(|k| self.each_on_storey(k, &mut near))
    }
}

/// The boxes of building `bd` of block `b` a person or a car meets near `area`, `h0..h1` up: a home
/// building's walls, floors and stairs ([`Plan::each_box_near`]) and what stands on its roof, or
/// any other building's pieces. Stops when `f` says so, and says whether it did.
pub fn each_fine(
    b: &BlockInfo,
    bd: &Building,
    area: &Rect,
    h0: f32,
    h1: f32,
    f: &mut impl FnMut(&CityBox) -> bool,
) -> Option<bool> {
    let p = plan(b, bd)?;
    if p.each_box_near(area, h0, h1, f) {
        return Some(true);
    }
    // What stands on its body: a tier, the plant on its roof.
    let mut pieces = [Piece::default(); MAX_SOLIDS];
    let n = bd.pieces(&mut pieces);
    Some(pieces[1..n].iter().any(|q| q.b.rect.overlaps(area) && f(&q.b)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::{CITY, ROWS, lots};

    /// Every home building on a strip's stretch of blocks, with its block.
    fn plans(strip: u8, bxs: core::ops::RangeInclusive<i32>, mut f: impl FnMut(&BlockInfo, &Plan)) {
        for bx in bxs {
            for row in -ROWS..=ROWS {
                let Some(b) = block(strip, bx, row, Stage(0)) else { continue };
                for bd in lots(&b).as_slice() {
                    if let Some(p) = plan(&b, bd) {
                        f(&b, &p);
                    }
                }
            }
        }
    }

    #[test]
    fn ids_round_trip() {
        for (strip, bx, row, lot, storey, unit, house) in
            [(0, 3, -12, 0, 0, 0, false), (2, 249, 12, 8, 9, 13, true), (1, 120, -1, 4, 31, 15, false)]
        {
            let id = HomeId::new(strip, bx, row, lot, storey, unit, house);
            assert_eq!(
                (id.strip(), id.bx(), id.row(), id.lot(), id.storey(), id.unit(), id.house()),
                (strip, bx, row, lot, storey, unit, house)
            );
        }
    }

    #[test]
    fn the_residential_and_old_town_districts_have_homes() {
        let (mut flats, mut houses, mut buildings) = (0u32, 0u32, 0u32);
        for strip in 0..3 {
            plans(strip, CITY.0..=CITY.1, |_, p| {
                buildings += 1;
                match p.kind {
                    Kind::Flats => flats += p.homes_per_storey() * p.storeys,
                    Kind::Houses => houses += p.homes_per_storey(),
                }
            });
        }
        assert!(buildings > 2_000, "{buildings} home buildings");
        assert!(flats > 50_000 && houses > 1_000, "{flats} flats, {houses} houses");
    }

    #[test]
    fn every_home_is_inside_its_building_and_found_where_it_is() {
        for strip in 0..3 {
            plans(strip, CITY.0..=CITY.1, |_, p| {
                let inner = p.foot.inset(SHELL - 0.01);
                for k in 0..p.storeys {
                    for unit in 0..p.homes_per_storey() {
                        let home = p.home(k, unit).expect("its home");
                        assert!(inner.holds(&home.rect), "{home:?} in {:?}", p.foot);
                        assert!(home.area > 20.0, "{home:?}");
                        let (ms, mx) = home.rect.middle();
                        let h = home.floor + 1.0;
                        assert_eq!(home_at(strip, ms, mx, h), Some(home.id), "{home:?}");
                        assert_eq!(super::home(home.id), Some(home), "by its id");
                        // Its door's outside is in the common parts (or the street), its inside in it.
                        assert_eq!(home_at(strip, home.inside.0, home.inside.1, h), Some(home.id));
                        assert_ne!(home_at(strip, home.outside.0, home.outside.1, h), Some(home.id));
                    }
                    if p.kind == Kind::Houses {
                        break;
                    }
                }
            });
        }
    }

    #[test]
    fn homes_never_overlap() {
        plans(0, CITY.0..=CITY.0 + 120, |_, p| {
            let storeys = if p.kind == Kind::Houses { 1 } else { p.storeys };
            for k in 0..storeys {
                for a in 0..p.homes_per_storey() {
                    for b in a + 1..p.homes_per_storey() {
                        let (ha, hb) = (p.home(k, a).unwrap(), p.home(k, b).unwrap());
                        assert!(!ha.rect.overlaps(&hb.rect), "{ha:?} and {hb:?}");
                    }
                }
            }
        });
    }

    #[test]
    fn a_home_s_floor_is_clear_and_its_walls_are_solid() {
        // Inside a home, a person's box clear of its walls is clear of every box of the building; a
        // box in the middle of its wall isn't.
        plans(1, CITY.0..=CITY.0 + 60, |_, p| {
            for k in 0..p.storeys.min(3) {
                for unit in 0..p.homes_per_storey() {
                    let home = p.home(k, unit).unwrap();
                    let (ms, mx) = home.rect.middle();
                    let area = Rect::new(ms - 0.3, ms + 0.3, mx - 0.3, mx + 0.3);
                    let (h0, h1) = (home.floor + 0.01, home.floor + 1.8);
                    let hit = p.each_box_near(&area, h0, h1, &mut |_| true);
                    assert!(!hit, "a person in the middle of {home:?}");
                    // Above the ground floor (whose floor is the street's kerb), a slab under it.
                    if k > 0 {
                        let under =
                            p.each_box_near(&area, home.floor - 0.1, home.floor - 0.01, &mut |_| true);
                        assert!(under, "a floor under {home:?}");
                    }
                }
                if p.kind == Kind::Houses {
                    break;
                }
            }
        });
    }
}
