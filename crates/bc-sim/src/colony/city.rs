//! The city: its streets, blocks and buildings as a closed form of where you ask. Nothing is
//! stored. A block is worked out from its strip and its cell on the grid with an integer hash and
//! plain `f32` arithmetic, the same to the bit on every machine, so the server checks a pose
//! against the same walls every client draws and walks.
//!
//! Across a strip from its edge: the window-bank park, twelve rows of 128 m blocks, the avenue
//! (80 m, the tram on its median), twelve rows more, the other bank. Along it: Hub Gate's square at
//! the docking hub's cap (its offices round it), the city's 191 blocks in twelve districts (`content::city`), the
//! building site, and the far cap's foot. Streets run on the grid's lines, 24 m wide and 40 m every
//! fourth; the fourth row past the avenue is the canal, bridged at every street.
//!
//! City coordinates are `frame::CityPos`'s: `s` across, `x` along, `h` up. Rows count out from
//! the avenue (row 0): negative towards the strip's edge where `s` is 0, positive beyond, 13 the
//! banks. The walker's frame is `(x, h, −s)`, and [`solid`] takes its boxes.

use glam::Vec3;

use crate::colony::frame::STRIP_WIDTH;
use crate::content::city::{DISTRICTS, DistrictKind, PLACES, PlaceDef, PlaceKind, SPECIAL, Special};
use crate::math::floor;
use crate::world::COLONY_HALF_LENGTH;

/// A block's cell, m square.
pub const BLOCK: f32 = 128.0;
/// Where the grid's line before block 0 lies along the axis: block `bx` starts at `GRID_X0 + 128 bx`.
pub const GRID_X0: f32 = -16_384.0;
/// The avenue down each strip's middle, m wide; the tram runs on its median.
pub const AVENUE: f32 = 80.0;
pub const MEDIAN: f32 = 16.0;
/// Rows of blocks either side of the avenue, and the banks' row beyond them.
pub const ROWS: i32 = 12;
pub const BANK_ROW: i32 = 13;
/// Streets on the grid's lines: most of them, and every fourth.
pub const STREET: f32 = 24.0;
pub const WIDE_STREET: f32 = 40.0;
/// Pavement round a block, inside its kerb.
pub const SIDEWALK: f32 = 5.0;
pub const KERB: f32 = 0.15;
/// The canal: its row, its channel's width and depth to the bed.
pub const CANAL_ROW: i32 = 4;
pub const CANAL_WIDTH: f32 = 40.0;
pub const CANAL_DEPTH: f32 = 3.0;
/// Railings: along the canal, and at the window banks' glass.
pub const RAILING: f32 = 1.1;
pub const RAIL_THICKNESS: f32 = 0.3;
/// Stretches along the axis, by block: Hub Gate's, the city, the building site, the far foot.
pub const HUB_GATE: (i32, i32) = (3, 7);
/// Hub Gate's square: this many rows either side of the avenue (the rest of its stretch is built).
pub const SQUARE_ROWS: i32 = 2;
pub const CITY: (i32, i32) = (8, 198);
pub const SITE: (i32, i32) = (199, 249);
pub const FAR_FOOT: (i32, i32) = (250, 252);
pub const DISTRICT_BLOCKS: i32 = 16;
/// Nothing stands taller, m.
pub const MAX_HEIGHT: f32 = 240.0;
pub const FLOOR: f32 = 3.6;
pub const GROUND_FLOOR: f32 = 5.0;
/// Hub Gate's terminal at the foot of the end cap, where the cap lift comes down: across the avenue
/// (half-width), and how far out from the cap.
pub const TERMINAL_HALF: f32 = 30.0;
pub const TERMINAL_DEPTH: f32 = 60.0;
pub const TERMINAL_HEIGHT: f32 = 30.0;

/// A rectangle on a strip: `s` across and `x` along, m.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub s0: f32,
    pub s1: f32,
    pub x0: f32,
    pub x1: f32,
}

impl Rect {
    pub const fn new(s0: f32, s1: f32, x0: f32, x1: f32) -> Self {
        Self { s0, s1, x0, x1 }
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        self.s0 < o.s1 && o.s0 < self.s1 && self.x0 < o.x1 && o.x0 < self.x1
    }

    /// Whether it holds all of `o`.
    pub fn holds(&self, o: &Rect) -> bool {
        self.s0 <= o.s0 && o.s1 <= self.s1 && self.x0 <= o.x0 && o.x1 <= self.x1
    }

    pub fn contains(&self, s: f32, x: f32) -> bool {
        self.s0 <= s && s <= self.s1 && self.x0 <= x && x <= self.x1
    }

    /// Smaller by `d` all round.
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.s0 + d, self.s1 - d, self.x0 + d, self.x1 - d)
    }

    pub fn width(&self) -> f32 {
        self.s1 - self.s0
    }

    pub fn length(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn middle(&self) -> (f32, f32) {
        ((self.s0 + self.s1) * 0.5, (self.x0 + self.x1) * 0.5)
    }
}

/// A solid box in city coordinates: a rectangle from `h0` up to `h1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CityBox {
    pub rect: Rect,
    pub h0: f32,
    pub h1: f32,
}

/// How a building is built (and drawn).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Plain,
    /// A tower on a podium.
    Tower,
    /// Long and thin.
    Slab,
    /// A pavilion in a park.
    Pavilion,
    /// A monument or kiosk on a plaza.
    Monument,
    /// A steel frame going up on the site: only its corner columns are solid.
    Frame,
    /// A tower crane's mast.
    Crane,
    /// A key place's hall.
    Hall,
}

/// A building: its base, and a tower on it if it has one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Building {
    pub foot: Rect,
    /// The base's roof, m up from the floor.
    pub height: f32,
    /// A tower on the base: its footprint and its roof.
    pub tower: Option<(Rect, f32)>,
    pub style: Style,
    pub seed: u32,
}

impl Building {
    /// The highest roof.
    pub fn top(&self) -> f32 {
        self.tower.map_or(self.height, |(_, h)| h.max(self.height))
    }

    /// What's solid of it: up to four boxes (a frame's corner columns, or a base and its tower).
    pub fn solids(&self, out: &mut [CityBox; 4]) -> usize {
        match self.style {
            Style::Frame => {
                let f = self.foot;
                let c = 1.2;
                let corners = [
                    Rect::new(f.s0, f.s0 + c, f.x0, f.x0 + c),
                    Rect::new(f.s1 - c, f.s1, f.x0, f.x0 + c),
                    Rect::new(f.s0, f.s0 + c, f.x1 - c, f.x1),
                    Rect::new(f.s1 - c, f.s1, f.x1 - c, f.x1),
                ];
                for (i, r) in corners.into_iter().enumerate() {
                    out[i] = CityBox { rect: r, h0: KERB, h1: self.height };
                }
                4
            }
            _ => {
                out[0] = CityBox { rect: self.foot, h0: KERB, h1: self.height };
                match self.tower {
                    Some((r, h)) => {
                        out[1] = CityBox { rect: r, h0: self.height, h1: h };
                        2
                    }
                    None => 1,
                }
            }
        }
    }
}

/// A block's buildings.
pub const MAX_LOTS: usize = 9;

#[derive(Clone, Copy, Debug)]
pub struct Lots {
    pub n: u8,
    pub items: [Building; MAX_LOTS],
}

impl Lots {
    const EMPTY: Lots = Lots {
        n: 0,
        items: [Building {
            foot: Rect::new(0.0, 0.0, 0.0, 0.0),
            height: 0.0,
            tower: None,
            style: Style::Plain,
            seed: 0,
        }; MAX_LOTS],
    };

    pub fn as_slice(&self) -> &[Building] {
        &self.items[..self.n as usize]
    }

    fn push(&mut self, b: Building) {
        if (self.n as usize) < MAX_LOTS {
            self.items[self.n as usize] = b;
            self.n += 1;
        }
    }
}

/// What a block is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockKind {
    Buildings,
    Park,
    Plaza,
    /// The canal's row: quays either side of the channel.
    Canal,
    /// The building site.
    Site,
    /// A key place: `content::city::PLACES[i]`.
    Place(u8),
    /// One tower taking the block, this tall.
    Tower(f32),
}

/// A block: a cell of the grid inside its streets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockInfo {
    pub strip: u8,
    pub bx: i32,
    pub row: i32,
    /// Its footprint, to the kerb's edge.
    pub rect: Rect,
    pub kind: BlockKind,
    /// Its district (index and kind), in the city's stretch.
    pub district: Option<(u8, DistrictKind)>,
    pub seed: u32,
}

/// How far the building site has been built out: this many of its districts (16 blocks each, from
/// the city's end) are city now. The colony's projects move it on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stage(pub u8);

/// An integer hash of three numbers.
pub fn mix(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

/// A hash's `k`th draw, uniform in [0, 1).
pub fn unit(h: u32, k: u32) -> f32 {
    (mix(h, k, 0x5EED) >> 8) as f32 / (1u32 << 24) as f32
}

/// Across the strip from its middle (the avenue's centre line).
pub fn centred(s: f32) -> f32 {
    s - STRIP_WIDTH * 0.5
}

/// The block whose cell holds `x` along the axis.
pub fn block_index(x: f32) -> i32 {
    floor((x - GRID_X0) / BLOCK) as i32
}

/// Where the grid's line before block `bx` lies along the axis.
pub fn grid_x(bx: i32) -> f32 {
    GRID_X0 + BLOCK * bx as f32
}

/// The street on the grid's line before block `bx`: how wide.
pub fn cross_width(bx: i32) -> f32 {
    if bx.rem_euclid(4) == 0 { WIDE_STREET } else { STREET }
}

/// The street along the axis on the outer edge of row `k` (1..=12, either side): how wide. Row 12's
/// is the bank road.
pub fn lane_width(k: i32) -> f32 {
    if k % 4 == 0 { WIDE_STREET } else { STREET }
}

/// The outer edge of row `k` from the strip's middle (`|centred(s)|`).
fn row_edge(k: i32) -> f32 {
    AVENUE * 0.5 + BLOCK * k as f32
}

/// The row at `s` across: 0 the avenue, ±1..±12 the blocks' rows, ±13 the banks.
pub fn row_at(s: f32) -> i32 {
    let c = centred(s);
    let a = c.abs();
    if a < AVENUE * 0.5 {
        return 0;
    }
    let k = (floor((a - AVENUE * 0.5) / BLOCK) as i32 + 1).min(BANK_ROW);
    if c < 0.0 { -k } else { k }
}

/// A row's whole span across the strip, `s0 < s1`.
pub fn row_span(row: i32) -> (f32, f32) {
    let k = row.abs().min(BANK_ROW);
    let (inner, outer) = match k {
        0 => return (STRIP_WIDTH * 0.5 - AVENUE * 0.5, STRIP_WIDTH * 0.5 + AVENUE * 0.5),
        BANK_ROW => (row_edge(ROWS), STRIP_WIDTH * 0.5),
        _ => (row_edge(k - 1), row_edge(k)),
    };
    let mid = STRIP_WIDTH * 0.5;
    if row < 0 { (mid - outer, mid - inner) } else { (mid + inner, mid + outer) }
}

/// The district at block `bx`, if it's in the city's stretch (or the site's, built out).
pub fn district_of(strip: u8, bx: i32, stage: Stage) -> Option<(u8, DistrictKind)> {
    let built = CITY.1 + DISTRICT_BLOCKS * i32::from(stage.0);
    if bx < HUB_GATE.0 || bx > built.min(SITE.1) {
        return None;
    }
    let d = ((bx - CITY.0).max(0) / DISTRICT_BLOCKS).clamp(0, 11);
    let kind = if bx < CITY.0 {
        // Round Hub Gate's square: the colony's offices.
        DistrictKind::Civic
    } else if bx > CITY.1 {
        // The site, built out: alternate works and homes.
        if (bx - SITE.0) / DISTRICT_BLOCKS % 2 == 0 { DistrictKind::Works } else { DistrictKind::Residential }
    } else {
        DISTRICTS[strip as usize % 3][d as usize]
    };
    Some((d as u8, kind))
}

/// The district at `x` along strip `strip`.
pub fn district_at(strip: u8, x: f32, stage: Stage) -> Option<(u8, DistrictKind)> {
    district_of(strip, block_index(x), stage)
}

/// Whether there's a block in cell (`bx`, `row`): in the city's and the site's stretches off the
/// avenue and the banks, and round Hub Gate's square (its two rows either side of the avenue are
/// the square).
pub fn has_block(bx: i32, row: i32) -> bool {
    let rows = if (HUB_GATE.0..=HUB_GATE.1).contains(&bx) { SQUARE_ROWS + 1..=ROWS } else { 1..=ROWS };
    (HUB_GATE.0..=SITE.1).contains(&bx) && rows.contains(&row.abs())
}

/// The footprint of cell (`bx`, `row`)'s block, inside its streets.
pub fn block_rect(bx: i32, row: i32) -> Rect {
    let x0 = grid_x(bx) + cross_width(bx) * 0.5;
    let x1 = grid_x(bx + 1) - cross_width(bx + 1) * 0.5;
    let k = row.abs();
    let inner = row_edge(k - 1) + if k > 1 { lane_width(k - 1) * 0.5 } else { 0.0 };
    let outer = row_edge(k) - lane_width(k) * 0.5;
    let mid = STRIP_WIDTH * 0.5;
    if row < 0 {
        Rect::new(mid - outer, mid - inner, x0, x1)
    } else {
        Rect::new(mid + inner, mid + outer, x0, x1)
    }
}

/// The canal's channel in a canal block: the water between the quays (beneath a street, the
/// channel runs on under its bridge).
pub fn channel(rect: &Rect) -> Rect {
    let (mid, _) = rect.middle();
    Rect::new(mid - CANAL_WIDTH * 0.5, mid + CANAL_WIDTH * 0.5, rect.x0, rect.x1)
}

/// A key place by its slug: its index and its row.
pub fn place(slug: &str) -> Option<(usize, &'static PlaceDef)> {
    PLACES.iter().enumerate().find(|(_, p)| p.slug == slug)
}

/// Hub Gate's terminal on strip `strip`: its footprint at the foot of the end cap.
pub fn terminal_rect() -> Rect {
    let mid = STRIP_WIDTH * 0.5;
    Rect::new(
        mid - TERMINAL_HALF,
        mid + TERMINAL_HALF,
        -COLONY_HALF_LENGTH,
        -COLONY_HALF_LENGTH + TERMINAL_DEPTH,
    )
}

/// Where a pilot stands at a place's door, and which way they face to go in: `(s, x)` and a unit
/// `(ds, dx)`.
pub fn place_door(p: &PlaceDef) -> ((f32, f32), (f32, f32)) {
    if p.kind == PlaceKind::HubGate {
        let t = terminal_rect();
        return ((STRIP_WIDTH * 0.5, t.x1 + 4.0), (0.0, -1.0));
    }
    let r = block_rect(p.bx, p.row);
    let (ms, mx) = r.middle();
    match p.door_x {
        1 => ((ms, r.x1 + 4.0), (0.0, -1.0)),
        -1 => ((ms, r.x0 - 4.0), (0.0, 1.0)),
        // Towards the avenue.
        _ if p.row < 0 => ((r.s1 + 4.0, mx), (-1.0, 0.0)),
        _ => ((r.s0 - 4.0, mx), (1.0, 0.0)),
    }
}

/// Block (`bx`, `row`) on strip `strip`, if there's one there.
pub fn block(strip: u8, bx: i32, row: i32, stage: Stage) -> Option<BlockInfo> {
    if !has_block(bx, row) {
        return None;
    }
    let seed = mix(u32::from(strip) + 1, bx as u32, (row + 64) as u32);
    let district = district_of(strip, bx, stage);
    let rect = block_rect(bx, row);
    let mut kind = None;
    for (i, p) in PLACES.iter().enumerate() {
        if p.strip == strip && p.bx == bx && p.row == row {
            kind = Some(BlockKind::Place(i as u8));
        }
    }
    if kind.is_none() {
        for (st, b, r, sp) in SPECIAL {
            if st == strip && b == bx && r == row {
                kind = Some(match sp {
                    Special::Park => BlockKind::Park,
                    Special::Plaza => BlockKind::Plaza,
                    Special::Tower(h) => BlockKind::Tower(h),
                });
            }
        }
    }
    let kind = kind.unwrap_or_else(|| {
        if row == CANAL_ROW {
            return BlockKind::Canal;
        }
        let Some((_, d)) = district else { return BlockKind::Site };
        let (park, plaza) = match d {
            DistrictKind::Park => (0.75, 0.05),
            DistrictKind::Civic => (0.1, 0.12),
            DistrictKind::OldTown => (0.05, 0.06),
            DistrictKind::University => (0.15, 0.08),
            _ => (0.07, 0.03),
        };
        let u = unit(seed, 1);
        if u < park {
            BlockKind::Park
        } else if u < park + plaza {
            BlockKind::Plaza
        } else {
            BlockKind::Buildings
        }
    });
    Some(BlockInfo { strip, bx, row, rect, kind, district, seed })
}

/// How a district builds: floors (least, most), lots along and across (least, most), the share of
/// lots with a tower, and the gap between neighbours (m).
struct Builds {
    floors: (u32, u32),
    lots: (u32, u32),
    towers: f32,
    gap: f32,
}

fn builds(d: DistrictKind) -> Builds {
    match d {
        DistrictKind::Business => Builds { floors: (3, 6), lots: (1, 2), towers: 0.65, gap: 6.0 },
        DistrictKind::Midtown => Builds { floors: (7, 21), lots: (2, 3), towers: 0.1, gap: 0.0 },
        DistrictKind::Residential => Builds { floors: (3, 10), lots: (2, 3), towers: 0.0, gap: 4.0 },
        DistrictKind::OldTown => Builds { floors: (2, 4), lots: (3, 3), towers: 0.0, gap: 0.0 },
        DistrictKind::Civic => Builds { floors: (5, 11), lots: (1, 2), towers: 0.1, gap: 8.0 },
        DistrictKind::University => Builds { floors: (2, 7), lots: (2, 2), towers: 0.0, gap: 10.0 },
        DistrictKind::Works => Builds { floors: (3, 9), lots: (1, 2), towers: 0.0, gap: 6.0 },
        DistrictKind::Port => Builds { floors: (2, 7), lots: (1, 2), towers: 0.0, gap: 8.0 },
        DistrictKind::Park => Builds { floors: (1, 2), lots: (1, 1), towers: 0.0, gap: 0.0 },
    }
}

fn floors_height(n: u32) -> f32 {
    GROUND_FLOOR + FLOOR * n.saturating_sub(1) as f32
}

/// A building's height from `lo..=hi` floors with draw `u`.
fn height_of(lo: u32, hi: u32, u: f32) -> f32 {
    let n = lo + ((hi - lo + 1) as f32 * u) as u32;
    floors_height(n.min(hi))
}

/// Block `b`'s buildings.
pub fn lots(b: &BlockInfo) -> Lots {
    let mut out = Lots::EMPTY;
    let area = b.rect.inset(SIDEWALK);
    let seed = b.seed;
    match b.kind {
        BlockKind::Buildings => {
            let Some((_, d)) = b.district else { return out };
            let rule = builds(d);
            let span = rule.lots.1 - rule.lots.0 + 1;
            let nx = rule.lots.0 + (unit(seed, 2) * span as f32) as u32;
            let ns = rule.lots.0 + (unit(seed, 3) * span as f32) as u32;
            let (nx, ns) = (nx.min(3), ns.min(3));
            for i in 0..nx {
                for j in 0..ns {
                    let lot = Rect::new(
                        area.s0 + area.width() * j as f32 / ns as f32,
                        area.s0 + area.width() * (j + 1) as f32 / ns as f32,
                        area.x0 + area.length() * i as f32 / nx as f32,
                        area.x0 + area.length() * (i + 1) as f32 / nx as f32,
                    );
                    let ls = mix(seed, i, j);
                    // Some lots stand empty: a yard, a garden.
                    if unit(ls, 1) < 0.06 {
                        continue;
                    }
                    let foot = lot.inset(rule.gap * 0.5);
                    let height = height_of(rule.floors.0, rule.floors.1, unit(ls, 2));
                    let tower = (unit(ls, 3) < rule.towers && foot.width() > 30.0 && foot.length() > 30.0)
                        .then(|| {
                            let shrink = 0.2 + 0.12 * unit(ls, 4);
                            let t = Rect::new(
                                foot.s0 + foot.width() * shrink,
                                foot.s1 - foot.width() * shrink,
                                foot.x0 + foot.length() * shrink,
                                foot.x1 - foot.length() * shrink,
                            );
                            let u = unit(ls, 5);
                            let top = (80.0 + 160.0 * u * u).min(MAX_HEIGHT);
                            (t, floors_height(((top - GROUND_FLOOR) / FLOOR) as u32 + 1).min(MAX_HEIGHT))
                        });
                    let style = if tower.is_some() {
                        Style::Tower
                    } else if foot.width() > 2.5 * foot.length() || foot.length() > 2.5 * foot.width() {
                        Style::Slab
                    } else {
                        Style::Plain
                    };
                    out.push(Building { foot, height, tower, style, seed: ls });
                }
            }
        }
        BlockKind::Park => {
            // A pavilion or two among the trees.
            let n = (unit(seed, 2) * 2.2) as u32;
            for i in 0..n {
                let ls = mix(seed, 7, i);
                let (w, l) = (8.0 + 8.0 * unit(ls, 1), 10.0 + 10.0 * unit(ls, 2));
                let s = area.s0 + (area.width() - w) * unit(ls, 3);
                let x = area.x0 + (area.length() - l) * (0.5 * i as f32 + 0.45 * unit(ls, 4));
                let foot = Rect::new(s, s + w, x, x + l);
                out.push(Building {
                    foot,
                    height: 4.0 + 3.0 * unit(ls, 5),
                    tower: None,
                    style: Style::Pavilion,
                    seed: ls,
                });
            }
        }
        BlockKind::Plaza => {
            let (s, x) = area.middle();
            let r = 3.0 + 2.0 * unit(seed, 2);
            let foot = Rect::new(s - r, s + r, x - r, x + r);
            out.push(Building {
                foot,
                height: 6.0 + 8.0 * unit(seed, 3),
                tower: None,
                style: Style::Monument,
                seed,
            });
        }
        BlockKind::Tower(h) => {
            let foot = area.inset(10.0);
            let podium = floors_height(4);
            let t = foot.inset((foot.width().min(foot.length()) * 0.18).min(20.0));
            out.push(Building {
                foot,
                height: podium,
                tower: Some((t, h.min(MAX_HEIGHT))),
                style: Style::Tower,
                seed,
            });
        }
        BlockKind::Place(i) => {
            let height = match PLACES[i as usize].kind {
                PlaceKind::Bar => 12.0,
                PlaceKind::Exchange => 24.0,
                PlaceKind::Charter => 36.0,
                PlaceKind::HubGate => TERMINAL_HEIGHT,
            };
            out.push(Building { foot: area, height, tower: None, style: Style::Hall, seed });
        }
        BlockKind::Site => {
            // A frame going up, maybe two, and a crane.
            let n = 1 + (unit(seed, 2) * 1.6) as u32;
            for i in 0..n {
                let ls = mix(seed, 11, i);
                let w = 30.0 + 30.0 * unit(ls, 1);
                let l = 30.0 + 30.0 * unit(ls, 2);
                let s = area.s0 + (area.width() - w).max(0.0) * unit(ls, 3);
                let x = if i == 0 { area.x0 } else { area.x1 - l };
                let foot = Rect::new(s, s + w.min(area.width()), x, x + l.min(area.length()));
                let height = floors_height(5 + (unit(ls, 4) * 28.0) as u32).min(120.0);
                out.push(Building { foot, height, tower: None, style: Style::Frame, seed: ls });
            }
            if unit(seed, 3) < 0.45 {
                let (s, x) = area.middle();
                let foot = Rect::new(s - 1.5, s + 1.5, x - 1.5, x + 1.5);
                out.push(Building {
                    foot,
                    height: 80.0 + 80.0 * unit(seed, 4),
                    tower: None,
                    style: Style::Crane,
                    seed: mix(seed, 13, 0),
                });
            }
        }
        BlockKind::Canal => {}
    }
    out
}

/// How high the ground is at `(s, x)` (the floor, kerbs and the canal's bed), buildings aside.
pub fn ground(strip: u8, s: f32, x: f32, stage: Stage) -> f32 {
    let (bx, row) = (block_index(x), row_at(s));
    let Some(b) = block(strip, bx, row, stage) else { return 0.0 };
    if !b.rect.contains(s, x) {
        return 0.0;
    }
    if b.kind == BlockKind::Canal && channel(&b.rect).contains(s, x) {
        return -CANAL_DEPTH;
    }
    KERB
}

/// The solid boxes near a footprint, for anything that wants them one at a time: buildings, kerbs,
/// railings, Hub Gate's terminal, the end caps' walls, the glass's edge, the tram stations'
/// platforms. Calls `f` with each; stops early when it returns true, and says whether it did.
pub fn each_solid(strip: u8, area: &Rect, stage: Stage, mut f: impl FnMut(&CityBox) -> bool) -> bool {
    const DEEP: f32 = -50.0;
    const SKY: f32 = 4_000.0;
    // The end caps, and past the glass's edge.
    let walls = [
        CityBox { rect: Rect::new(-1e6, 1e6, -1e6, -COLONY_HALF_LENGTH), h0: DEEP, h1: SKY },
        CityBox { rect: Rect::new(-1e6, 1e6, COLONY_HALF_LENGTH, 1e6), h0: DEEP, h1: SKY },
        CityBox { rect: Rect::new(-1e6, 0.0, -1e6, 1e6), h0: DEEP, h1: SKY },
        CityBox { rect: Rect::new(STRIP_WIDTH, 1e6, -1e6, 1e6), h0: DEEP, h1: SKY },
        // The banks' railings at the glass.
        CityBox { rect: Rect::new(0.0, RAIL_THICKNESS, -1e6, 1e6), h0: DEEP, h1: RAILING },
        CityBox {
            rect: Rect::new(STRIP_WIDTH - RAIL_THICKNESS, STRIP_WIDTH, -1e6, 1e6),
            h0: DEEP,
            h1: RAILING,
        },
        CityBox { rect: terminal_rect(), h0: DEEP, h1: TERMINAL_HEIGHT },
    ];
    for w in &walls {
        if w.rect.overlaps(area) && f(w) {
            return true;
        }
    }
    // The tram stations' island platforms, on the avenue.
    if super::transit::platform_solids(area, &mut f) {
        return true;
    }
    let (b0, b1) = (block_index(area.x0), block_index(area.x1));
    let (r0, r1) = (row_at(area.s0), row_at(area.s1));
    let mut boxes = [CityBox::default(); 4];
    for bx in b0..=b1 {
        for row in r0..=r1 {
            let Some(b) = block(strip, bx, row, stage) else { continue };
            if !b.rect.overlaps(area) {
                continue;
            }
            // The kerb: the block stands a step up from the street. In the canal's row the channel
            // cuts it in two, with railings along the quays.
            if b.kind == BlockKind::Canal {
                let ch = channel(&b.rect);
                let quays = [
                    Rect::new(b.rect.s0, ch.s0, b.rect.x0, b.rect.x1),
                    Rect::new(ch.s1, b.rect.s1, b.rect.x0, b.rect.x1),
                ];
                for q in quays {
                    if q.overlaps(area) && f(&CityBox { rect: q, h0: DEEP, h1: KERB }) {
                        return true;
                    }
                }
                let rails = [
                    Rect::new(ch.s0 - RAIL_THICKNESS, ch.s0, b.rect.x0, b.rect.x1),
                    Rect::new(ch.s1, ch.s1 + RAIL_THICKNESS, b.rect.x0, b.rect.x1),
                ];
                for r in rails {
                    if r.overlaps(area) && f(&CityBox { rect: r, h0: KERB, h1: KERB + RAILING }) {
                        return true;
                    }
                }
                if ch.overlaps(area) && f(&CityBox { rect: ch, h0: DEEP, h1: -CANAL_DEPTH }) {
                    return true;
                }
            } else if f(&CityBox { rect: b.rect, h0: DEEP, h1: KERB }) {
                return true;
            }
            for building in lots(&b).as_slice() {
                if !building.foot.overlaps(area) {
                    continue;
                }
                let n = building.solids(&mut boxes);
                for piece in &boxes[..n] {
                    if piece.rect.overlaps(area) && f(piece) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Whether a box in the walker's frame on strip `strip` (`(x, h, −s)`, from `min` to `max`)
/// touches anything solid: the floor, a kerb, a building, a railing, the end caps.
pub fn solid(strip: u8, min: Vec3, max: Vec3, stage: Stage) -> bool {
    let area = Rect::new(-max.z, -min.z, min.x, max.x);
    let (h0, h1) = (min.y, max.y);
    // The floor everywhere, but for the canal's channel.
    if h0 < -CANAL_DEPTH {
        return true;
    }
    if h0 < 0.0 && !in_channel(strip, &area, stage) {
        return true;
    }
    each_solid(strip, &area, stage, |b| b.rect.overlaps(&area) && h0 < b.h1 && b.h0 < h1)
}

/// Whether a footprint lies wholly in the canal's channel (or under one of its bridges' spans).
fn in_channel(strip: u8, area: &Rect, stage: Stage) -> bool {
    let (bx, row) = (block_index((area.x0 + area.x1) * 0.5), row_at((area.s0 + area.s1) * 0.5));
    match block(strip, bx, row, stage) {
        Some(b) if b.kind == BlockKind::Canal => channel(&b.rect).holds(area),
        _ => false,
    }
}

/// One byte-sized summary of a block for the shaders' map of the city: its kind, its district's
/// kind, its tallest roof (2 m steps) and a seed.
pub fn texel(strip: u8, bx: i32, row: i32, stage: Stage) -> [u8; 4] {
    let Some(b) = block(strip, bx, row, stage) else {
        return [0, 0, 0, 0];
    };
    let kind = match b.kind {
        BlockKind::Buildings => 1,
        BlockKind::Park => 2,
        BlockKind::Plaza => 3,
        BlockKind::Canal => 4,
        BlockKind::Site => 5,
        BlockKind::Place(_) => 6,
        BlockKind::Tower(_) => 7,
    };
    let district = b.district.map_or(0, |(_, d)| d as u8 + 1);
    let top = lots(&b).as_slice().iter().fold(0.0f32, |m, x| m.max(x.top()));
    [kind, district, (top * 0.5).min(255.0) as u8, (b.seed & 0xff) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::frame::STRIPS;
    use crate::math::Rng;

    const STAGE: Stage = Stage(0);

    /// A walker's box (0.6 × 1.8 m) standing at `(s, x)` with its feet at `h`, on strip `k`.
    fn walker_at(k: u8, s: f32, x: f32, h: f32) -> bool {
        solid(k, Vec3::new(x - 0.3, h, -s - 0.3), Vec3::new(x + 0.3, h + 1.8, -s + 0.3), STAGE)
    }

    #[test]
    fn the_grid_adds_up() {
        assert!((row_span(BANK_ROW).1 - STRIP_WIDTH).abs() < 1e-3);
        assert!(row_span(-BANK_ROW).0.abs() < 1e-3);
        let bank = row_span(BANK_ROW).1 - row_span(BANK_ROW).0;
        assert!((bank - 99.515).abs() < 0.01, "{bank} m of bank");
        assert_eq!(grid_x(HUB_GATE.0), -COLONY_HALF_LENGTH);
        assert_eq!(grid_x(FAR_FOOT.1 + 1), COLONY_HALF_LENGTH);
        for row in -BANK_ROW..=BANK_ROW {
            let (s0, s1) = row_span(row);
            assert_eq!(row_at((s0 + s1) * 0.5), row);
        }
        let r = block_rect(20, 1);
        assert!((r.width() - (BLOCK - STREET * 0.5)).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn streets_avenues_and_plazas_are_clear_to_walk() {
        for k in 0..STRIPS as u8 {
            // Every cross street, up and down the strip, at a few places across (on the avenue,
            // its road: a tram station's platform may stand on its median).
            for bx in (HUB_GATE.0 + 1)..=FAR_FOOT.1 {
                for s in [60.0, 700.0, STRIP_WIDTH * 0.5 - 20.0, 2_100.0, STRIP_WIDTH - 60.0] {
                    assert!(!walker_at(k, s, grid_x(bx), 0.0), "a street at {bx} on strip {k}, s {s}");
                }
            }
            // The avenue, end to end; Hub Gate's plaza beyond its terminal.
            for i in 0..400 {
                let x = -COLONY_HALF_LENGTH + 70.0 + 31_850.0 * i as f32 / 400.0;
                assert!(!walker_at(k, STRIP_WIDTH * 0.5 + 20.0, x, 0.0), "the avenue at {x}");
            }
            let square = row_edge(SQUARE_ROWS) - lane_width(SQUARE_ROWS) * 0.5 - 1.0;
            for i in 0..=50 {
                let s = STRIP_WIDTH * 0.5 - square + 2.0 * square * i as f32 / 50.0;
                for bx in HUB_GATE.0 + 1..=HUB_GATE.1 {
                    assert!(!walker_at(k, s, grid_x(bx) + 64.0, 0.0), "Hub Gate's square at {s}, {bx}");
                }
            }
            // The lanes along the axis between rows.
            for kk in 1..=ROWS {
                for sign in [-1.0f32, 1.0] {
                    let s = STRIP_WIDTH * 0.5 + sign * row_edge(kk);
                    for bx in [20, 77, 150, 220] {
                        let x = grid_x(bx) + BLOCK * 0.5;
                        assert!(!walker_at(k, s, x, 0.0), "a lane by row {kk} at {bx}");
                    }
                }
            }
        }
    }

    #[test]
    fn buildings_stand_inside_their_blocks_and_under_the_cap() {
        let mut seen = 0;
        for k in 0..STRIPS as u8 {
            for bx in CITY.0..=SITE.1 {
                for row in -ROWS..=ROWS {
                    let Some(b) = block(k, bx, row, STAGE) else { continue };
                    for bd in lots(&b).as_slice() {
                        seen += 1;
                        assert!(b.rect.inset(SIDEWALK - 1e-3).holds(&bd.foot), "{bd:?} spills out of {b:?}");
                        assert!(bd.foot.width() > 0.5 && bd.foot.length() > 0.5);
                        assert!(bd.top() <= MAX_HEIGHT && bd.height > 0.0, "{bd:?}");
                        if let Some((t, h)) = bd.tower {
                            assert!(bd.foot.holds(&t) && h >= bd.height, "{bd:?}");
                        }
                    }
                }
            }
        }
        assert!(seen > 30_000, "{seen} buildings");
    }

    #[test]
    fn solid_is_what_the_boxes_say() {
        // Against a brute force over every box near enough, from random boxes the size of a walker
        // or a car anywhere on a strip.
        let mut rng = Rng::new(7);
        for _ in 0..4_000 {
            let k = (rng.next_u32() % 3) as u8;
            let s = rng.next_f32() * STRIP_WIDTH;
            let x = (rng.signed() * 0.99) * COLONY_HALF_LENGTH;
            let h = rng.next_f32() * 30.0 - 2.0;
            let half =
                Vec3::new(0.1 + 2.0 * rng.next_f32(), 0.5 + rng.next_f32(), 0.1 + 2.0 * rng.next_f32());
            let (min, max) = (Vec3::new(x, h, -s) - half, Vec3::new(x, h, -s) + half);
            let got = solid(k, min, max, STAGE);
            let area = Rect::new(-max.z, -min.z, min.x, max.x);
            let mut want = min.y < -CANAL_DEPTH || (min.y < 0.0 && !in_channel(k, &area, STAGE));
            let wide = Rect::new(area.s0 - BLOCK, area.s1 + BLOCK, area.x0 - BLOCK, area.x1 + BLOCK);
            each_solid(k, &wide, STAGE, |b| {
                want |= b.rect.overlaps(&area) && min.y < b.h1 && b.h0 < max.y;
                false
            });
            assert_eq!(got, want, "{k} {min} {max}");
        }
    }

    #[test]
    fn every_key_places_door_is_on_a_street_facing_it() {
        for p in &PLACES {
            let ((s, x), (ds, dx)) = place_door(p);
            assert!(!walker_at(p.strip, s, x, 0.0), "{}'s door is in a wall", p.name);
            // Further on, the way in, is the building.
            assert!(
                walker_at(p.strip, s + ds * 12.0, x + dx * 12.0, KERB + 0.01),
                "{} has nothing to go into",
                p.name
            );
            if p.kind != PlaceKind::HubGate {
                let b = block(p.strip, p.bx, p.row, STAGE).unwrap();
                assert_eq!(b.kind, BlockKind::Place(PLACES.iter().position(|q| q == p).unwrap() as u8));
            }
            assert!(place(p.slug).is_some());
        }
    }

    #[test]
    fn the_canal_runs_the_city_under_its_bridges() {
        for k in 0..STRIPS as u8 {
            let r = block_rect(40, CANAL_ROW);
            let (ms, _) = channel(&r).middle();
            for bx in CITY.0..=CITY.1 {
                let x = grid_x(bx) + BLOCK * 0.5;
                assert_eq!(ground(k, ms, x, STAGE), -CANAL_DEPTH, "the channel at {bx}");
                // On its bed nothing's in the way; on a bridge, the street.
                assert!(!walker_at(k, ms, x, -CANAL_DEPTH), "the bed at {bx}");
                assert!(!walker_at(k, ms, grid_x(bx), 0.0), "the bridge before {bx}");
                // The quays are railed.
                let q = channel(&block_rect(bx, CANAL_ROW));
                assert!(walker_at(k, q.s0 - 0.5, x, KERB) || walker_at(k, q.s0 - 0.1, x, KERB));
            }
        }
    }

    #[test]
    fn the_glass_is_railed_and_the_caps_are_walls() {
        for k in 0..STRIPS as u8 {
            assert!(walker_at(k, 0.2, 0.0, 0.0) && walker_at(k, STRIP_WIDTH - 0.2, 0.0, 0.0));
            assert!(!walker_at(k, 2.0, 0.0, 0.0) && !walker_at(k, STRIP_WIDTH - 2.0, 0.0, 0.0));
            assert!(walker_at(k, 900.0, -COLONY_HALF_LENGTH - 0.1, 0.0));
            assert!(walker_at(k, 900.0, COLONY_HALF_LENGTH + 0.1, 0.0));
            // Through the floor, never; down into the canal only in its channel.
            assert!(walker_at(k, 900.0, 0.0, -0.5));
        }
    }

    #[test]
    fn districts_build_as_they_should() {
        for k in 0..STRIPS as u8 {
            let mut tall = 0.0f32;
            let mut parks = 0;
            for bx in CITY.0..=CITY.1 {
                for row in -ROWS..=ROWS {
                    let Some(b) = block(k, bx, row, STAGE) else { continue };
                    let top = lots(&b).as_slice().iter().fold(0.0f32, |m, x| m.max(x.top()));
                    tall = tall.max(top);
                    parks += usize::from(b.kind == BlockKind::Park);
                    if let Some((_, DistrictKind::OldTown)) = b.district
                        && b.kind == BlockKind::Buildings
                    {
                        assert!(top <= floors_height(4) + 1e-3, "Old Town stays low: {top}");
                    }
                }
            }
            assert!(tall > 120.0, "a skyline on strip {k}: {tall}");
            assert!(parks > 150, "parks on strip {k}: {parks}");
        }
        // The site is building, and a stage builds it out from the city's end.
        let b = block(0, SITE.0 + 3, 2, Stage(0)).unwrap();
        assert_eq!(b.kind, BlockKind::Site);
        let built = block(0, SITE.0 + 3, 2, Stage(1)).unwrap();
        assert_ne!(built.kind, BlockKind::Site);
    }

    #[test]
    fn the_map_of_the_city_is_byte_sized() {
        let t = texel(0, 30, -2, STAGE);
        assert_eq!(t[0], 7, "the Axis View tower");
        assert_eq!(t[2], 120, "240 m");
        assert_eq!(texel(0, 5, 1, STAGE), [0, 0, 0, 0], "Hub Gate's plaza");
    }
}
