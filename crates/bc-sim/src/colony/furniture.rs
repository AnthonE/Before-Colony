//! The city's street furniture: the lamps whose light `city_lib.wgsl` paints on the ground, the
//! trees in the avenue's pits and along the quays, and the benches between the avenue's trees.
//! Like the rest of the city it's a closed form of where you ask: [`each_furniture`] works out the
//! few pieces near a footprint from the block or the stretch of street they stand by. Nothing is
//! stored or sent.
//!
//! People and their cars bump into it ([`super::city::solid`]); a mobile suit steps over it, so
//! what a suit meets ([`super::city::each_solid`], [`super::city::solid_built`], `interior`) leaves
//! it out.
//!
//! Each piece stands where the ground's paint has it (`city_lib.wgsl`: `lamps_near`'s rows,
//! `paint_plaza`'s ring, the pits of `avenue_pavement` and `paint_canal`), from the same numbers,
//! which `bc_client_core::city_atlas`'s tests check: a post under every pool of the city's lamps, a
//! trunk in every pit. A post stands on the pavement and leans its lantern out over the street; at
//! a corner, right by the kerb's corner, before the crossings that land there (0.5 to 4.5 m on).
//! Nothing stands in a crossing, a key place's doorway or the walks. The colony's own lamps (Hub
//! Gate's square, the banks' promenades, the tram's median) are its paint alone for now.

use core::f32::consts::FRAC_1_SQRT_2;

use super::city::{
    AVENUE, BANK_ROW, BlockInfo, BlockKind, CANAL_WIDTH, CITY, CityBox, FAR_FOOT, HUB_GATE, KERB, ROWS, Rect,
    SIDEWALK, SITE, Stage, block, block_index, channel, cross_width, grid_x, has_block, mix, place_door,
    room, row_at, row_span, unit,
};
use super::frame::STRIP_WIDTH;
use crate::content::city::PLACES;
use crate::math::floor;
use crate::world::COLONY_HALF_LENGTH;

/// The street lamps (`city_lib.wgsl`'s `LAMP_GAP`, `LAMP_OUT`): about this far apart down a kerb,
/// spaced evenly from corner to corner; each lantern this far out over the street from its kerb.
pub const LAMP_GAP: f32 = 30.0;
pub const LAMP_OUT: f32 = 1.5;
/// A post stands this far in from its kerb, its arm reaching out to its lantern; at a corner, this
/// far in from both kerbs, before the crossings (which land 0.5 m on).
pub const LAMP_IN: f32 = 0.8;
pub const LAMP_CORNER: f32 = 0.25;
/// The avenue's carriageways reach this far from its middle line; its pavements are the rest. Its
/// lamps stand on them this far out, lanterns under the trees.
pub const ROAD_OUT: f32 = AVENUE * 0.5 - 18.0;
pub const AVENUE_LAMP: f32 = 22.7;
/// A park's lamps: beside the loop of its path (`PARK_LOOP` in from its pavement), `PARK_LAMP`
/// inside it, between its corners (its diagonals run to them).
pub const PARK_LOOP: f32 = 6.0;
pub const PARK_LAMP: f32 = 2.4;
/// A plaza's lamps on a ring round its monument.
pub const PLAZA_RING: f32 = 24.0;
pub const PLAZA_LAMPS: usize = 8;
/// A quay's lamps and trees, this far back from the water's edge.
pub const QUAY_LAMP: f32 = 2.2;
pub const QUAY_TREE: f32 = 11.0;
/// The avenue's trees, in pits down its pavements this far out from its middle line: one every
/// `TREE_PITCH` from `TREE_FIRST` along each stretch between cross streets, none within `TREE_END`
/// of one (where the crossings land). The quays' the same way.
pub const AVENUE_TREE: f32 = 24.3;
pub const TREE_PITCH: f32 = 8.0;
pub const TREE_FIRST: f32 = 4.0;
pub const TREE_END: f32 = 5.0;
/// A trunk is solid this high: above it, the crown, out of anyone's reach.
pub const TRUNK: f32 = 3.0;
/// Benches between the avenue's trees: every `BENCH_PITCH` along a stretch, none within
/// `BENCH_END` of its far end; under a stride's step (`walker::STEP`), so a walker steps up on one.
pub const BENCH_PITCH: f32 = 16.0;
pub const BENCH_END: f32 = 8.0;
pub const BENCH_HEIGHT: f32 = 0.42;
/// No post stands this near a key place's door (its spot on the street, `city::place_door`).
pub const DOOR_CLEAR: f32 = 6.0;
/// How far a piece's solid reaches from where it stands (a bench's half-length and more): cells
/// this near a footprint are asked.
const REACH: f32 = 2.0;

/// What a piece of furniture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On a block's pavement by a street (or across the bank road from one): its arm reaches out
    /// over the street to its lantern.
    StreetLamp,
    /// On the avenue's pavements, under the trees: its lantern on top.
    AvenueLamp,
    /// Beside a park's loop, along a quay: a lantern on a short post.
    PathLamp,
    /// On a plaza's ring: as a path lamp, but always lit (the colony's).
    PlazaLamp,
    /// A tree: its trunk is solid, its crown isn't.
    Tree,
    /// A slab of a bench between the avenue's trees.
    Bench,
}

impl Kind {
    /// Its solid's half-widths (across `s`, along `x`) and height over where it stands, m.
    pub const fn size(self) -> (f32, f32, f32) {
        match self {
            Kind::StreetLamp => (0.12, 0.12, 7.0),
            Kind::AvenueLamp => (0.12, 0.12, 4.0),
            Kind::PathLamp | Kind::PlazaLamp => (0.08, 0.08, 4.0),
            Kind::Tree => (0.2, 0.2, TRUNK),
            Kind::Bench => (0.25, 0.9, BENCH_HEIGHT),
        }
    }

    /// Whether it's a lamp (it has a lantern).
    pub const fn lamp(self) -> bool {
        matches!(self, Kind::StreetLamp | Kind::AvenueLamp | Kind::PathLamp | Kind::PlazaLamp)
    }
}

/// A piece of the street furniture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Furniture {
    pub kind: Kind,
    /// Where it stands, `(s, x)`, and the ground under it (m up: a kerb's top, or the floor).
    pub s: f32,
    pub x: f32,
    pub h: f32,
    /// The way it faces, a unit `(ds, dx)`: a street lamp's arm reaches this way.
    pub facing: (f32, f32),
    /// A street lamp's arm, from its post to its lantern over the pool, m (anything else: 0).
    pub reach: f32,
    /// What people and their vehicles bump into (a suit steps over it).
    pub solid: CityBox,
    pub seed: u32,
}

impl Furniture {
    fn new(kind: Kind, (s, x, h): (f32, f32, f32), facing: (f32, f32), reach: f32, seed: u32) -> Self {
        let (hs, hx, tall) = kind.size();
        let solid = CityBox { rect: Rect::new(s - hs, s + hs, x - hx, x + hx), h0: h, h1: h + tall };
        Self { kind, s, x, h, facing, reach, solid, seed }
    }

    /// A lamp's lantern: out along its arm over its pool (or on top of its post), `(s, x, h)` with
    /// `h` its post's top.
    pub fn lantern(&self) -> Option<(f32, f32, f32)> {
        let (s, x) = (self.s + self.facing.0 * self.reach, self.x + self.facing.1 * self.reach);
        self.kind.lamp().then_some((s, x, self.solid.h1))
    }
}

/// How big a tree grows (its size, m, as `city_mesh`'s trees draw it): its trunk stays [`TRUNK`]
/// solid whatever its size.
pub fn tree_size(seed: u32) -> f32 {
    8.0 + 4.0 * unit(seed, 1)
}

/// How many gaps a row of lamps `len` long has (`city_lib.wgsl`'s `lamp_row`): about `LAMP_GAP`.
pub fn lamp_count(len: f32) -> u32 {
    (floor(len / LAMP_GAP + 0.5) as u32).max(1)
}

/// Where lamp `i` of a row of `n` gaps `len` long stands along it: evenly, its ends `LAMP_CORNER`
/// in from the corners.
fn lamp_at(i: u32, n: u32, len: f32) -> f32 {
    (len * i as f32 / n as f32).clamp(LAMP_CORNER, len - LAMP_CORNER)
}

/// A stretch of street along the axis between cross streets `bx` and `bx + 1`: where it starts,
/// and how long it is (a block's length).
fn stretch(bx: i32) -> (f32, f32) {
    let x0 = grid_x(bx) + cross_width(bx) * 0.5;
    (x0, grid_x(bx + 1) - cross_width(bx + 1) * 0.5 - x0)
}

/// Whether a row of pieces along `x` at `s` (reaching `half` either side of it) can touch `near`.
fn line(near: &Rect, s: f32, half: f32) -> bool {
    near.s0 <= s + half && s - half <= near.s1
}

/// The same for a row along `s` at `x`.
fn line_x(near: &Rect, x: f32, half: f32) -> bool {
    near.x0 <= x + half && x - half <= near.x1
}

/// The street furniture near a footprint, for anything that wants it one piece at a time: calls
/// `f` with each whose solid overlaps `area`; stops early when it returns true, and says whether it
/// did. Each piece belongs to the cell of the grid it stands in, so a walk over the cells finds it
/// once.
pub fn each_furniture(strip: u8, area: &Rect, stage: Stage, mut f: impl FnMut(&Furniture) -> bool) -> bool {
    let near = Rect::new(area.s0 - REACH, area.s1 + REACH, area.x0 - REACH, area.x1 + REACH);
    if near.s1 < 0.0 || near.s0 > STRIP_WIDTH || near.x1 < -COLONY_HALF_LENGTH || near.x0 > COLONY_HALF_LENGTH
    {
        return false;
    }
    let mut give = |p: Furniture| p.solid.rect.overlaps(area) && f(&p);
    // Blocks stand from Hub Gate's to the site's end; the avenue's stretches and the bank road no
    // further.
    let b0 = block_index(near.x0.max(-COLONY_HALF_LENGTH)).max(HUB_GATE.0);
    let b1 = block_index(near.x1.min(COLONY_HALF_LENGTH)).min(SITE.1);
    let (r0, r1) = (row_at(near.s0), row_at(near.s1));
    for bx in b0..=b1 {
        for row in r0..=r1 {
            let (s0, s1) = row_span(row);
            if s1 < near.s0 || s0 > near.s1 {
                continue;
            }
            let done = match block(strip, bx, row, stage) {
                Some(b) => by_block(&b, &near, &mut give),
                None if row == 0 && (CITY.0..FAR_FOOT.0).contains(&bx) => avenue(strip, bx, &near, &mut give),
                None if row.abs() == BANK_ROW => bank(strip, bx, row, &near, &mut give),
                None => false,
            };
            if done {
                return true;
            }
        }
    }
    false
}

/// A block's: its kerbs' lamps, and in it a park's or a plaza's lamps, or a canal's quays.
fn by_block(b: &BlockInfo, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let r = b.rect;
    // Everything of a block's stands inside its kerb.
    if !r.overlaps(near) {
        return false;
    }
    // A key place's door: no post in its way, across the whole of the Blast Hall's.
    let door = match b.kind {
        BlockKind::Place(i) => room(usize::from(i))
            .map(|r| (place_door(&PLACES[usize::from(i)]).0, DOOR_CLEAR.max(r.door_width() * 0.5 + 2.0))),
        _ => None,
    };
    let clear =
        |s: f32, x: f32| door.is_none_or(|((ds, dx), c)| (s - ds) * (s - ds) + (x - dx) * (x - dx) > c * c);
    // Down its kerbs along the lanes (row 12's outer one is the bank road's), corner to corner.
    // Row ±1's inner side is the avenue's pavement, whose lamps are the avenue's.
    let (len, n) = (r.length(), lamp_count(r.length()));
    for (k, (edge, out)) in [(r.s0, -1.0f32), (r.s1, 1.0)].into_iter().enumerate() {
        let avenue_side = (b.row > 0) == (out < 0.0);
        if (avenue_side && b.row.abs() == 1) || !line(near, edge, 1.0) {
            continue;
        }
        for i in 0..=n {
            let inset = if i == 0 || i == n { LAMP_CORNER } else { LAMP_IN };
            let (s, x) = (edge - out * inset, r.x0 + lamp_at(i, n, len));
            let p = Furniture::new(
                Kind::StreetLamp,
                (s, x, KERB),
                (out, 0.0),
                inset + LAMP_OUT,
                mix(b.seed, 0xF1 + k as u32, i),
            );
            if clear(s, x) && give(p) {
                return true;
            }
        }
    }
    // Down its kerbs along the cross streets, between the corners; none in the canal's row, where
    // they'd stand over its water (`city_lib.wgsl` paints no pools there either).
    if b.kind != BlockKind::Canal {
        let (w, n) = (r.width(), lamp_count(r.width()));
        for (k, (edge, out)) in [(r.x0, -1.0f32), (r.x1, 1.0)].into_iter().enumerate() {
            if !line_x(near, edge, 1.0) {
                continue;
            }
            for i in 1..n {
                let (s, x) = (r.s0 + w * i as f32 / n as f32, edge - out * LAMP_IN);
                let p = Furniture::new(
                    Kind::StreetLamp,
                    (s, x, KERB),
                    (0.0, out),
                    LAMP_IN + LAMP_OUT,
                    mix(b.seed, 0xF3 + k as u32, i),
                );
                if clear(s, x) && give(p) {
                    return true;
                }
            }
        }
    }
    match b.kind {
        BlockKind::Park => park(b, near, give),
        BlockKind::Plaza => plaza(b, near, give),
        BlockKind::Canal => quays(b, near, give),
        _ => false,
    }
}

/// A park's lamps beside the loop of its path, between its corners (its pavilions stand inside
/// them, `city::lots`).
fn park(b: &BlockInfo, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let lr = b.rect.inset(SIDEWALK + PARK_LOOP);
    if !lr.overlaps(near) {
        return false;
    }
    let n = lamp_count(lr.length());
    for (k, (s, out)) in [(lr.s0 + PARK_LAMP, -1.0f32), (lr.s1 - PARK_LAMP, 1.0)].into_iter().enumerate() {
        for i in 1..n {
            let x = lr.x0 + lr.length() * i as f32 / n as f32;
            let p = Furniture::new(
                Kind::PathLamp,
                (s, x, KERB),
                (out, 0.0),
                0.0,
                mix(b.seed, 0xF5 + k as u32, i),
            );
            if give(p) {
                return true;
            }
        }
    }
    let n = lamp_count(lr.width());
    for (k, (x, out)) in [(lr.x0 + PARK_LAMP, -1.0f32), (lr.x1 - PARK_LAMP, 1.0)].into_iter().enumerate() {
        for i in 1..n {
            let s = lr.s0 + lr.width() * i as f32 / n as f32;
            let p = Furniture::new(
                Kind::PathLamp,
                (s, x, KERB),
                (0.0, out),
                0.0,
                mix(b.seed, 0xF7 + k as u32, i),
            );
            if give(p) {
                return true;
            }
        }
    }
    false
}

/// The plaza's ring of lamps, heading by heading from −π (`paint_plaza`'s `heading`, from +x
/// towards +s): `(ds, dx)` from its middle.
const RING: [(f32, f32); PLAZA_LAMPS] = [
    (0.0, -1.0),
    (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (0.0, 1.0),
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (1.0, 0.0),
    (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
];

fn plaza(b: &BlockInfo, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let (ms, mx) = b.rect.middle();
    let reach = PLAZA_RING + 1.0;
    if !near.overlaps(&Rect::new(ms - reach, ms + reach, mx - reach, mx + reach)) {
        return false;
    }
    for (k, (ds, dx)) in RING.into_iter().enumerate() {
        let at = (ms + PLAZA_RING * ds, mx + PLAZA_RING * dx, KERB);
        if give(Furniture::new(Kind::PlazaLamp, at, (ds, dx), 0.0, mix(b.seed, 0xF9, k as u32))) {
            return true;
        }
    }
    false
}

/// Both quays of a canal block: lamps by the water, corner to corner, and a row of trees.
fn quays(b: &BlockInfo, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let r = b.rect;
    let (water, _) = channel(&r).middle();
    let (len, n) = (r.length(), lamp_count(r.length()));
    for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let s = water + side * (CANAL_WIDTH * 0.5 + QUAY_LAMP);
        if line(near, s, 0.2) {
            for i in 0..=n {
                let at = (s, r.x0 + lamp_at(i, n, len), KERB);
                let p =
                    Furniture::new(Kind::PathLamp, at, (-side, 0.0), 0.0, mix(b.seed, 0xFB + k as u32, i));
                if give(p) {
                    return true;
                }
            }
        }
        let s = water + side * (CANAL_WIDTH * 0.5 + QUAY_TREE);
        if line(near, s, 0.3) && trees(s, r.x0, len, KERB, mix(b.seed, 0xFD, k as u32), near, give) {
            return true;
        }
    }
    false
}

/// A row of trees down a stretch `len` long from `x0`, at `s`, on ground `h`: every `TREE_PITCH`
/// from `TREE_FIRST`, none within `TREE_END` of its ends.
fn trees(
    s: f32,
    x0: f32,
    len: f32,
    h: f32,
    seed: u32,
    near: &Rect,
    give: &mut impl FnMut(Furniture) -> bool,
) -> bool {
    let last = floor((len - TREE_END - TREE_FIRST) / TREE_PITCH) as i32;
    for k in 0..=last {
        let u = TREE_FIRST + TREE_PITCH * k as f32;
        if u < TREE_END || !line_x(near, x0 + u, 0.3) {
            continue;
        }
        if give(Furniture::new(Kind::Tree, (s, x0 + u, h), (1.0, 0.0), 0.0, mix(seed, 0x7E, k as u32))) {
            return true;
        }
    }
    false
}

/// The avenue's stretch between cross streets `bx` and `bx + 1`: on each pavement its lamps
/// (corner to corner), its trees and the benches between them.
fn avenue(strip: u8, bx: i32, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let (x0, len) = stretch(bx);
    let (mid, n) = (STRIP_WIDTH * 0.5, lamp_count(len));
    let seed = mix(u32::from(strip) + 1, bx as u32, 0xA0);
    for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let s = mid + side * AVENUE_LAMP;
        if line(near, s, 0.2) {
            for i in 0..=n {
                let at = (s, x0 + lamp_at(i, n, len), 0.0);
                let p = Furniture::new(Kind::AvenueLamp, at, (side, 0.0), 0.0, mix(seed, 1 + k as u32, i));
                if give(p) {
                    return true;
                }
            }
        }
        let s = mid + side * AVENUE_TREE;
        if line(near, s, 0.3) {
            if trees(s, x0, len, 0.0, mix(seed, 3 + k as u32, 0), near, give) {
                return true;
            }
            let last = floor((len - BENCH_END) / BENCH_PITCH) as i32;
            for j in 1..=last {
                let x = x0 + BENCH_PITCH * j as f32;
                let p = Furniture::new(
                    Kind::Bench,
                    (s, x, 0.0),
                    (side, 0.0),
                    0.0,
                    mix(seed, 5 + k as u32, j as u32),
                );
                if line_x(near, x, 1.0) && give(p) {
                    return true;
                }
            }
        }
    }
    false
}

/// A window bank's cell: across the bank road from row ±12's blocks, its far kerb's lamps, on the
/// bank's footpath.
fn bank(strip: u8, bx: i32, row: i32, near: &Rect, give: &mut impl FnMut(Furniture) -> bool) -> bool {
    let side = if row < 0 { -1.0f32 } else { 1.0 };
    let edge = if row < 0 { row_span(-BANK_ROW).1 } else { row_span(BANK_ROW).0 };
    if !has_block(bx, ROWS * row.signum()) || !line(near, edge + side * LAMP_IN, 1.0) {
        return false;
    }
    let (x0, len) = stretch(bx);
    let n = lamp_count(len);
    for i in 0..=n {
        let inset = if i == 0 || i == n { LAMP_CORNER } else { LAMP_IN };
        let at = (edge + side * inset, x0 + lamp_at(i, n, len), 0.0);
        let seed = mix(u32::from(strip) + 1, bx as u32, 0xBB + i);
        if give(Furniture::new(Kind::StreetLamp, at, (-side, 0.0), inset + LAMP_OUT, seed)) {
            return true;
        }
    }
    false
}
