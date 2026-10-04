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
use crate::colony::furniture::{PARK_LAMP, PARK_LOOP};
use crate::content::city::{DISTRICTS, DistrictKind, PLACES, PlaceDef, PlaceKind, SPECIAL, Special};
use crate::math::{atan2, floor};
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

/// A building: its foot and its body's roof, how it's massed over and beside its body, and what's
/// on its roof. Its boxes are [`Building::pieces`], every one inside its foot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Building {
    pub foot: Rect,
    /// Its body's roof (a tower's podium's), m up from the floor.
    pub height: f32,
    pub form: Form,
    pub roof: Rooftop,
    pub style: Style,
    pub seed: u32,
    /// A key place's hall: the room behind its door.
    pub room: Option<Room>,
}

/// What a piece of a building is, for how it's drawn (`city_mesh`): to the rules, and to whatever
/// walks or flies into it, every piece is a solid box alike.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Part {
    /// What stands on the street: a body, a podium, a wing, a shed's bay.
    #[default]
    Body,
    /// Storeys set back on what's below: a tower's shaft, a terrace, an attic, a campanile.
    Tier,
    /// A tower's crown, a lantern, a belfry: no storeys of windows.
    Crown,
    /// What stands on a roof: plant rooms, lift overruns, water tanks, chimneys, rooflights.
    Plant,
    /// A spire, a mast, a stack: thin and tall.
    Mast,
}

/// A piece of a building: a solid box, and what it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Piece {
    pub b: CityBox,
    pub part: Part,
}

/// A lot's sides on a street (its block's edges), a building's wings, a shed's office's corner:
/// bits for its −s, +s, −x and +x sides.
pub const SIDE_S0: u8 = 1;
pub const SIDE_S1: u8 = 2;
pub const SIDE_X0: u8 = 4;
pub const SIDE_X1: u8 = 8;
pub const SIDES: u8 = 15;

/// How a tower is crowned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Crown {
    /// Its shaft's roof, with plant on it.
    #[default]
    Flat,
    /// A box a little in from the shaft's top: its mechanical floors.
    Hat,
    /// Two boxes stepping in: a stepped top.
    Stepped,
    /// A narrow box in the middle: a lantern.
    Lantern,
    /// A penthouse at one end of the roof.
    Offset,
}

/// What stands on a building's highest roof.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rooftop {
    #[default]
    Bare,
    /// A lift overrun, and a plant room on a big roof (on a slab, an overrun at either end).
    Plant,
    /// A lift overrun and a water tank or two.
    Tanks,
    /// Chimney stacks on the party walls `walls` (the sides not on a street).
    Chimneys { walls: u8 },
    /// A lantern stepping up in the middle: a civic building's.
    Lantern,
}

/// How a works' shed is roofed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShedRoof {
    /// Rooflights in rows across it: a sawtooth's steps.
    #[default]
    Sawtooth,
    /// A monitor down its length.
    Monitor,
    /// Bays along it, each its own height.
    Bays,
}

/// How a building is massed ([`Building::pieces`]): its body, and what stands on it and beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Form {
    /// One box: a pavilion, the site's frames and cranes, a key place's hall.
    #[default]
    Block,
    /// Its upper storeys set back `step` m from its sides `sides`, `tiers` times, up to `top`: a
    /// terrace steps back from one side, an attic from all four.
    Setback { sides: u8, tiers: u8, step: f32, top: f32 },
    /// Wings `depth` m deep along its sides `wings`, round a yard: an L, a U, an H or a court. The
    /// wings along `x` stand at its height, those across at `low`; a campanile on some.
    Court { wings: u8, depth: f32, low: f32, campanile: bool },
    /// A works' shed: its roof, an office at a street corner (`office`: that corner's sides; 0
    /// for none), a stack on some.
    Shed { roof: ShedRoof, office: u8, stack: bool },
    /// A tower on its podium: its shaft (`shaft` its foot) stepping in up to `tiers` times, its
    /// crown on top up to `top`, and a mast on some up to `mast` (0 for none).
    Tower { shaft: Rect, tiers: u8, crown: Crown, top: f32, mast: f32 },
    /// A monument on a plaza: a plinth, its shaft on it.
    Monument,
}

/// The most solid boxes a building is ([`Building::solids`]): a hall round its room, a tower.
pub const MAX_SOLIDS: usize = 8;

/// A shed's monitor and rooflights, m high.
pub const MONITOR: f32 = 4.0;
const ROOFLIGHT: f32 = 3.2;

/// The storeys under a roof `h` m up: the most whose roof is no higher.
pub fn storeys_under(h: f32) -> u32 {
    if h < GROUND_FLOOR { 0 } else { ((h - GROUND_FLOOR) / FLOOR + 1e-3) as u32 + 1 }
}

/// `r` moved in by `d` on its sides `sides`.
pub fn set_back(r: Rect, sides: u8, d: f32) -> Rect {
    let k = |bit: u8| if sides & bit != 0 { d } else { 0.0 };
    Rect::new(r.s0 + k(SIDE_S0), r.s1 - k(SIDE_S1), r.x0 + k(SIDE_X0), r.x1 - k(SIDE_X1))
}

/// A `ws` × `lx` rectangle inside `area`, `u` of the way across what's left over and `v` of the
/// way along it, if it fits.
fn place_in(area: Rect, ws: f32, lx: f32, u: f32, v: f32) -> Option<Rect> {
    let (fs, fx) = (area.width() - ws, area.length() - lx);
    (fs >= 0.0 && fx >= 0.0).then(|| {
        let (s, x) = (area.s0 + fs * u, area.x0 + fx * v);
        Rect::new(s, s + ws, x, x + lx)
    })
}

/// The `i`th of `n` equal slices of `r` along its length (`along_x`) or across it.
fn slice(r: Rect, along_x: bool, i: u32, n: u32) -> Rect {
    let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
    if along_x {
        Rect::new(r.s0, r.s1, r.x0 + r.length() * a, r.x0 + r.length() * b)
    } else {
        Rect::new(r.s0 + r.width() * a, r.s0 + r.width() * b, r.x0, r.x1)
    }
}

fn min_side(r: &Rect) -> f32 {
    r.width().min(r.length())
}

/// A building's pieces as they're laid, bottom up.
struct Laying<'a> {
    out: &'a mut [Piece; MAX_SOLIDS],
    n: usize,
}

impl Laying<'_> {
    /// Lays a piece, if there's room for it and it's a box at all.
    fn lay(&mut self, rect: Rect, h0: f32, h1: f32, part: Part) -> bool {
        if self.n >= MAX_SOLIDS || h1 < h0 + 0.5 || rect.width() < 0.8 || rect.length() < 0.8 {
            return false;
        }
        self.out[self.n] = Piece { b: CityBox { rect, h0, h1 }, part };
        self.n += 1;
        true
    }

    fn left(&self) -> usize {
        MAX_SOLIDS - self.n
    }
}

impl Building {
    /// A draw of its own for its pieces (`lots` draws 1 to 15 of the seed).
    fn draw(&self, k: u32) -> f32 {
        unit(self.seed, 64 + k)
    }

    /// The highest roof (a mast's top, if it has one).
    pub fn top(&self) -> f32 {
        let mut p = [Piece::default(); MAX_SOLIDS];
        let n = self.pieces(&mut p);
        p[..n].iter().fold(self.height, |m, q| m.max(q.b.h1))
    }

    /// The height of a box over its bodies' footprint that holds what its bodies and tiers do:
    /// what it is from afar (`city_mesh`'s coarser levels).
    pub fn bulk(&self) -> f32 {
        let mut p = [Piece::default(); MAX_SOLIDS];
        let n = self.pieces(&mut p);
        let (mut area, mut volume) = (0.0f32, 0.0f32);
        for q in &p[..n] {
            let a = q.b.rect.width() * q.b.rect.length();
            match q.part {
                Part::Body => {
                    area += a;
                    volume += a * (q.b.h1 - q.b.h0);
                }
                Part::Tier => volume += a * (q.b.h1 - q.b.h0),
                _ => {}
            }
        }
        if area > 0.0 { KERB + volume / area } else { self.height }
    }

    /// What's solid of it: its pieces' boxes.
    pub fn solids(&self, out: &mut [CityBox; MAX_SOLIDS]) -> usize {
        if let Some(room) = &self.room {
            return room.hall_solids(self.foot, self.height, out);
        }
        let mut p = [Piece::default(); MAX_SOLIDS];
        let n = self.pieces(&mut p);
        for (o, q) in out.iter_mut().zip(&p[..n]) {
            *o = q.b;
        }
        n
    }

    /// Its pieces, bottom up, each standing on the street or on the roof of one laid before it,
    /// none inside another, all inside its foot: a frame's corner columns, a hall round the room
    /// behind its door, or its body and what its form and its rooftop build on and beside it.
    pub fn pieces(&self, out: &mut [Piece; MAX_SOLIDS]) -> usize {
        let mut p = Laying { out, n: 0 };
        let f = self.foot;
        if let Some(room) = &self.room {
            let mut boxes = [CityBox::default(); MAX_SOLIDS];
            let n = room.hall_solids(f, self.height, &mut boxes);
            for b in &boxes[..n] {
                p.lay(b.rect, b.h0, b.h1, Part::Body);
            }
            return p.n;
        }
        match self.form {
            Form::Block if self.style == Style::Frame => {
                let c = 1.2;
                for r in [
                    Rect::new(f.s0, f.s0 + c, f.x0, f.x0 + c),
                    Rect::new(f.s1 - c, f.s1, f.x0, f.x0 + c),
                    Rect::new(f.s0, f.s0 + c, f.x1 - c, f.x1),
                    Rect::new(f.s1 - c, f.s1, f.x1 - c, f.x1),
                ] {
                    p.lay(r, KERB, self.height, Part::Body);
                }
            }
            Form::Block => {
                p.lay(f, KERB, self.height, Part::Body);
                self.rooftop(&mut p, f, self.height);
            }
            Form::Monument => {
                let plinth = KERB + (self.height * 0.15).clamp(1.0, 2.0);
                p.lay(f, KERB, plinth, Part::Body);
                p.lay(f.inset(min_side(&f) * 0.3), plinth, self.height, Part::Tier);
            }
            Form::Setback { sides, tiers, step, top } => {
                p.lay(f, KERB, self.height, Part::Body);
                let (sb, st) = (storeys_under(self.height + 0.01), storeys_under(top + 0.01));
                let (mut r, mut h) = (f, self.height);
                for k in 1..=u32::from(tiers) {
                    let next = set_back(r, sides, step);
                    let hk = floors_height(sb + (st.saturating_sub(sb) * k).div_ceil(u32::from(tiers)));
                    if min_side(&next) < 8.0 || hk < h + 1.0 {
                        break;
                    }
                    p.lay(next, h, hk, Part::Tier);
                    (r, h) = (next, hk);
                }
                self.rooftop(&mut p, r, h);
            }
            Form::Court { wings, depth, low, campanile } => {
                let (mut s0, mut s1) = (f.s0, f.s1);
                let across = wings & (SIDE_S0 | SIDE_S1) != 0;
                let mut first = None;
                // The wings along x (on the −s and +s sides), at its height.
                for (bit, r) in [
                    (SIDE_S0, Rect::new(f.s0, f.s0 + depth, f.x0, f.x1)),
                    (SIDE_S1, Rect::new(f.s1 - depth, f.s1, f.x0, f.x1)),
                ] {
                    if wings & bit != 0 && p.lay(r, KERB, self.height, Part::Body) {
                        if bit == SIDE_S0 {
                            s0 = r.s1
                        } else {
                            s1 = r.s0
                        }
                        first = first.or(Some(r));
                    }
                }
                // The wings across (on the −x and +x sides), between those.
                let h = if across { low } else { self.height };
                for (bit, x0, x1) in [(SIDE_X0, f.x0, f.x0 + depth), (SIDE_X1, f.x1 - depth, f.x1)] {
                    let r = Rect::new(s0, s1, x0, x1);
                    if wings & bit != 0 && p.lay(r, KERB, h, Part::Body) {
                        first = first.or(Some(r));
                    }
                }
                let Some(w) = first else { return p.n };
                let wh = if across { self.height } else { h };
                if campanile {
                    // At an end of the first wing: its shaft, a belfry, a spire.
                    let c = (min_side(&w) - 1.0).min(7.0);
                    let x = if self.draw(1) < 0.5 { w.x0 + 0.5 } else { w.x1 - 0.5 - c };
                    let s = if w.s0 <= f.s0 { w.s0 + 0.5 } else { w.s1 - 0.5 - c };
                    let t = Rect::new(s, s + c, x, x + c);
                    let up = floors_height(storeys_under(wh + 0.01) + 3 + (self.draw(2) * 3.0) as u32);
                    if p.lay(t, wh, up, Part::Tier) && p.lay(t.inset(0.6), up, up + 4.5, Part::Crown) {
                        let (ms, mx) = t.middle();
                        p.lay(
                            Rect::new(ms - 0.6, ms + 0.6, mx - 0.6, mx + 0.6),
                            up + 4.5,
                            up + 13.0,
                            Part::Mast,
                        );
                    }
                } else {
                    self.rooftop(&mut p, w, wh);
                }
            }
            Form::Shed { roof, office, stack } => {
                let along_x = f.length() >= f.width();
                let mut shed = f;
                if office != 0 {
                    // The office at its street corner, a few storeys over the shed; the shed
                    // beside it and beyond it.
                    let (ow, ol) = ((f.width() * 0.4).min(18.0), (f.length() * 0.4).min(22.0));
                    let lo_s = office & SIDE_S1 == 0;
                    let lo_x = office & SIDE_X1 == 0;
                    let (os0, os1) = if lo_s { (f.s0, f.s0 + ow) } else { (f.s1 - ow, f.s1) };
                    let (ox0, ox1) = if lo_x { (f.x0, f.x0 + ol) } else { (f.x1 - ol, f.x1) };
                    let oh =
                        floors_height(storeys_under(self.height + 0.01) + 2 + (self.draw(2) * 2.0) as u32);
                    p.lay(Rect::new(os0, os1, ox0, ox1), KERB, oh.min(MAX_HEIGHT), Part::Body);
                    let beside =
                        if lo_s { Rect::new(os1, f.s1, ox0, ox1) } else { Rect::new(f.s0, os0, ox0, ox1) };
                    shed = if lo_x {
                        Rect::new(f.s0, f.s1, ox1, f.x1)
                    } else {
                        Rect::new(f.s0, f.s1, f.x0, ox0)
                    };
                    p.lay(beside, KERB, self.height, Part::Body);
                    p.lay(shed, KERB, self.height, Part::Body);
                } else if roof == ShedRoof::Bays {
                    // Two or three bays along it, the first the tallest.
                    let nb = 2 + u32::from(self.draw(3) < 0.5);
                    let n = storeys_under(self.height + 0.01);
                    for i in 0..nb {
                        let share = if i == 0 { 1.0 } else { 0.55 + 0.35 * self.draw(4 + i) };
                        let h = floors_height(((n as f32 * share) as u32).max(2));
                        p.lay(slice(f, along_x, i, nb), KERB, h, Part::Body);
                    }
                    shed = slice(f, along_x, 0, nb);
                } else {
                    p.lay(f, KERB, self.height, Part::Body);
                }
                let h = self.height;
                let long_x = shed.length() >= shed.width();
                let (short, long) =
                    if long_x { (shed.width(), shed.length()) } else { (shed.length(), shed.width()) };
                // Across the shed at `a..b` along it, `inset` in from its long sides.
                let band = |a: f32, b: f32, inset: f32| {
                    if long_x {
                        Rect::new(shed.s0 + inset, shed.s1 - inset, shed.x0 + a, shed.x0 + b)
                    } else {
                        Rect::new(shed.s0 + a, shed.s0 + b, shed.x0 + inset, shed.x1 - inset)
                    }
                };
                let stack_rect = || {
                    // In the corner of its first bay, clear of the rooflights.
                    let (a, b) = (1.5, 4.7);
                    if long_x {
                        Rect::new(shed.s1 - b, shed.s1 - a, shed.x0 + a, shed.x0 + b)
                    } else {
                        Rect::new(shed.s0 + a, shed.s0 + b, shed.x1 - b, shed.x1 - a)
                    }
                };
                let stacks = usize::from(stack && short > 14.0);
                match roof {
                    ShedRoof::Sawtooth if short > 12.0 => {
                        let k = (((long / 14.0 + 0.5) as usize).clamp(3, 6)).min(p.left() - stacks);
                        let pitch = long / k as f32;
                        for i in 0..k {
                            let a = pitch * i as f32;
                            p.lay(
                                band(a + 0.45 * pitch, a + pitch - 0.5, 1.0),
                                h,
                                h + ROOFLIGHT,
                                Part::Plant,
                            );
                        }
                    }
                    ShedRoof::Monitor | ShedRoof::Bays if short > 12.0 => {
                        let w = (short * 0.3).clamp(5.0, 14.0);
                        let m = band(4.0, long - 4.0, (short - w) * 0.5);
                        p.lay(m, h, h + MONITOR, Part::Plant);
                    }
                    _ => {}
                }
                if stacks > 0 {
                    p.lay(stack_rect(), h, (h + 16.0 + 14.0 * self.draw(8)).min(MAX_HEIGHT), Part::Mast);
                }
            }
            Form::Tower { shaft, tiers, crown, top, mast } => {
                p.lay(f, KERB, self.height, Part::Body);
                // The shaft's tiers, each stepping in from the one below while it stays slender
                // enough to.
                let mut rects = [shaft; 3];
                let mut t = 1;
                while t < usize::from(tiers).min(3) {
                    let r = rects[t - 1];
                    let step = (0.12 * min_side(&r)).clamp(2.5, 6.0);
                    // A third tier on some steps in only on its long sides.
                    let next = if t == 2 && self.draw(1) < 0.5 {
                        set_back(
                            r,
                            if r.length() >= r.width() { SIDE_S0 | SIDE_S1 } else { SIDE_X0 | SIDE_X1 },
                            step,
                        )
                    } else {
                        r.inset(step)
                    };
                    if min_side(&next) < 12.0 {
                        break;
                    }
                    rects[t] = next;
                    t += 1;
                }
                // The crown takes the top of it (none over a shaft too short for it).
                let crown_h = match crown {
                    Crown::Flat => 0.0,
                    Crown::Hat | Crown::Offset => 2.0 * FLOOR + 1.0,
                    Crown::Lantern => 3.0 * FLOOR,
                    Crown::Stepped => 4.0 * FLOOR,
                };
                let sb = storeys_under(self.height + 0.01);
                let mut st = storeys_under(top - crown_h + 0.01);
                let mut crown = crown;
                if st < sb + 2 * t as u32 {
                    (crown, st) = (Crown::Flat, storeys_under(top + 0.01));
                }
                let shaft_top = if crown == Crown::Flat { top } else { floors_height(st) };
                let t = t.min(st.saturating_sub(sb) as usize).max(1);
                let fracs: [f32; 3] = match t {
                    1 => [1.0, 1.0, 1.0],
                    2 => [0.6 + 0.12 * self.draw(2), 1.0, 1.0],
                    _ => [0.48 + 0.1 * self.draw(2), 0.76 + 0.08 * self.draw(3), 1.0],
                };
                let mut h = self.height;
                let mut last = sb;
                for k in 0..t {
                    let hk = if k + 1 == t {
                        shaft_top
                    } else {
                        let sk = (sb + ((st - sb) as f32 * fracs[k] + 0.5) as u32)
                            .max(last + 1)
                            .min(st - (t - 1 - k) as u32);
                        last = sk;
                        floors_height(sk)
                    };
                    p.lay(rects[k], h, hk, Part::Tier);
                    h = hk;
                }
                let r = rects[t - 1];
                // What the mast stands on.
                let mut cap = (r, shaft_top);
                match crown {
                    Crown::Flat => {}
                    Crown::Hat => {
                        let c = r.inset(1.5);
                        if p.lay(c, shaft_top, top, Part::Crown) {
                            cap = (c, top);
                        }
                    }
                    Crown::Lantern => {
                        let c = r.inset(0.3 * min_side(&r));
                        if p.lay(c, shaft_top, top, Part::Crown) {
                            cap = (c, top);
                        }
                    }
                    Crown::Stepped => {
                        let c1 = r.inset(0.14 * min_side(&r));
                        let c2 = c1.inset(0.14 * min_side(&r));
                        let mid = shaft_top + 0.55 * (top - shaft_top);
                        if p.lay(c1, shaft_top, mid, Part::Crown) && p.lay(c2, mid, top, Part::Crown) {
                            cap = (c2, top);
                        }
                    }
                    Crown::Offset => {
                        let c = r.inset(1.5);
                        let c = if c.length() >= c.width() {
                            let l = c.length() * 0.55;
                            if self.draw(4) < 0.5 {
                                Rect::new(c.s0, c.s1, c.x0, c.x0 + l)
                            } else {
                                Rect::new(c.s0, c.s1, c.x1 - l, c.x1)
                            }
                        } else {
                            let w = c.width() * 0.55;
                            if self.draw(4) < 0.5 {
                                Rect::new(c.s0, c.s0 + w, c.x0, c.x1)
                            } else {
                                Rect::new(c.s1 - w, c.s1, c.x0, c.x1)
                            }
                        };
                        if p.lay(c, shaft_top, top, Part::Crown) {
                            cap = (c, top);
                        }
                    }
                }
                if mast > cap.1 + 6.0 {
                    let w = (1.4 + 0.04 * (mast - cap.1)).min(3.2).min(min_side(&cap.0) - 1.0);
                    let (ms, mx) = cap.0.middle();
                    p.lay(
                        Rect::new(ms - w * 0.5, ms + w * 0.5, mx - w * 0.5, mx + w * 0.5),
                        cap.1,
                        mast,
                        Part::Mast,
                    );
                } else if crown == Crown::Flat {
                    self.rooftop(&mut p, r, shaft_top);
                }
            }
        }
        p.n
    }

    /// What stands on its roof `r`, `h` up.
    fn rooftop(&self, p: &mut Laying, r: Rect, h: f32) {
        let long_x = r.length() >= r.width();
        let (short, long) = if long_x { (r.width(), r.length()) } else { (r.length(), r.width()) };
        // Half of the roof along its length, 2.5 m in from its edges.
        let half = |i: u32| slice(r, long_x, i, 2).inset(2.5);
        // Something `a` across by `b` along the roof, in half `i`.
        let fit = |i: u32, a: f32, b: f32, k: u32| {
            let (ws, lx) = if long_x { (a, b) } else { (b, a) };
            place_in(half(i), ws, lx, self.draw(k), self.draw(k + 1))
        };
        match self.roof {
            Rooftop::Bare => {}
            Rooftop::Plant | Rooftop::Tanks => {
                if short < 10.0 || long < 12.0 {
                    return;
                }
                // A lift overrun, a storey and its parapet tall.
                let (a, b) = (4.0 + 2.0 * self.draw(10), 5.5 + 3.0 * self.draw(11));
                if let Some(o) = fit(0, a, b, 12) {
                    p.lay(o, h, h + FLOOR + 0.6, Part::Plant);
                }
                let second = if self.style == Style::Slab {
                    fit(1, a, b, 14).map(|o| (o, FLOOR + 0.6))
                } else if self.roof == Rooftop::Tanks {
                    fit(1, 3.4, 3.4, 16).map(|o| (o, 4.0))
                } else if short >= 20.0 {
                    fit(1, 5.0 + 3.0 * self.draw(18), 6.0 + 5.0 * self.draw(19), 20).map(|o| (o, 3.0))
                } else {
                    None
                };
                if let Some((o, up)) = second {
                    p.lay(o, h, h + up, Part::Plant);
                }
            }
            Rooftop::Chimneys { walls } => {
                // On the party walls (any two sides, if none is), a third of the way along.
                let walls = if walls == 0 { SIDES } else { walls };
                let mut n = 0;
                for bit in [SIDE_X0, SIDE_X1, SIDE_S0, SIDE_S1] {
                    if walls & bit == 0 || n == 2 {
                        continue;
                    }
                    let at = 0.25 + 0.5 * self.draw(22 + u32::from(bit));
                    let c = if bit & (SIDE_X0 | SIDE_X1) != 0 {
                        let s = r.s0 + (r.width() - 2.2) * at;
                        let x = if bit == SIDE_X0 { r.x0 } else { r.x1 - 0.9 };
                        Rect::new(s, s + 2.2, x, x + 0.9)
                    } else {
                        let x = r.x0 + (r.length() - 2.2) * at;
                        let s = if bit == SIDE_S0 { r.s0 } else { r.s1 - 0.9 };
                        Rect::new(s, s + 0.9, x, x + 2.2)
                    };
                    if p.lay(c, h, h + 2.4, Part::Plant) {
                        n += 1;
                    }
                }
            }
            Rooftop::Lantern => {
                if min_side(&r) < 30.0 {
                    return;
                }
                let c1 = r.inset(min_side(&r) * 0.275);
                let c2 = c1.inset(min_side(&c1) * 0.2);
                if p.lay(c1, h, h + 5.4, Part::Crown) {
                    p.lay(c2, h + 5.4, h + 9.4, Part::Crown);
                }
            }
        }
    }
}

/// Walls round a key place's room, m thick.
pub const WALL: f32 = 1.0;
/// Its door: the gap in the front wall, m wide and high.
pub const DOOR_WIDTH: f32 = 4.0;
pub const DOOR_HEIGHT: f32 = 3.5;
/// Its counter, against the back wall: how high, how deep, how far out from the wall (m), and
/// how much of the room's width it takes.
pub const COUNTER_HEIGHT: f32 = 1.1;
pub const COUNTER_DEPTH: f32 = 1.2;
pub const COUNTER_GAP: f32 = 2.5;
pub const COUNTER_SHARE: f32 = 0.5;
/// Where a pilot stands to use a counter: this far out in front of it, m.
pub const COUNTER_STAND: f32 = 1.2;

/// The room behind a key place's door (`content::city::room_size`), at street level: through the
/// door in its front wall, a floor under a ceiling, and against its back wall the counter the
/// place is used at (the Exchange's terminal, the Charter Board, the bar). The hall stands solid
/// round it ([`Building::solids`]). A closed form of the place, like everything in the city.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    /// Which place it is (`content::city::PLACES`), and its strip.
    pub place: u8,
    pub strip: u8,
    /// Its floor, inside its walls.
    pub rect: Rect,
    /// Its ceiling, m up from the street.
    pub ceiling: f32,
    /// The door: the gap through the front wall.
    pub door: Rect,
    /// The way in, through the door (a unit `(ds, dx)`).
    pub inward: (f32, f32),
    /// The counter against the back wall.
    pub counter: Rect,
    /// The hall's frame: its front face, where along it the door is, and which way is in.
    pub front: Front,
}

/// Where a hall's front is: across (`along_s`: the front runs along `s`, its face at an `x`; else
/// along `x`, its face at an `s`), the face itself, the middle of the front along it (the door's
/// middle), and which way is in (+1 or −1). A place in it is `u` along the front from its middle
/// and `v` in from its face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Front {
    pub along_s: bool,
    pub face: f32,
    pub middle: f32,
    pub sign: f32,
}

impl Front {
    /// What's `u0..u1` along the front from its middle and `v0..v1` in from its face.
    pub fn rect(&self, u0: f32, u1: f32, v0: f32, v1: f32) -> Rect {
        let (a, b) = (self.face + self.sign * v0, self.face + self.sign * v1);
        let (lo, hi) = (a.min(b), a.max(b));
        if self.along_s {
            Rect::new(self.middle + u0, self.middle + u1, lo, hi)
        } else {
            Rect::new(lo, hi, self.middle + u0, self.middle + u1)
        }
    }

    /// The point `u` along the front and `v` in: `(s, x)`.
    pub fn point(&self, u: f32, v: f32) -> (f32, f32) {
        let w = self.face + self.sign * v;
        if self.along_s { (self.middle + u, w) } else { (w, self.middle + u) }
    }
}

impl Room {
    /// The hall round the room, of foot `foot` and roof `height`: what's over the ceiling, what's
    /// either side of the room and behind it, the front wall either side of the door and its
    /// lintel, and the counter. Eight boxes.
    fn hall_solids(&self, foot: Rect, height: f32, out: &mut [CityBox; MAX_SOLIDS]) -> usize {
        let f = &self.front;
        // The hall's extent along its front and in from it, about the front's middle.
        let (u0, u1, depth) = if f.along_s {
            (foot.s0 - f.middle, foot.s1 - f.middle, foot.length())
        } else {
            (foot.x0 - f.middle, foot.x1 - f.middle, foot.width())
        };
        let w = self.width() * 0.5;
        let d = self.depth();
        let (c, low) = (self.ceiling, KERB);
        let door = DOOR_WIDTH * 0.5;
        let cw = self.width() * COUNTER_SHARE * 0.5;
        let boxes = [
            CityBox { rect: foot, h0: c, h1: height },
            CityBox { rect: f.rect(u0, -w, 0.0, depth), h0: low, h1: c },
            CityBox { rect: f.rect(w, u1, 0.0, depth), h0: low, h1: c },
            CityBox { rect: f.rect(-w, w, d, depth), h0: low, h1: c },
            CityBox { rect: f.rect(-w, -door, 0.0, WALL), h0: low, h1: c },
            CityBox { rect: f.rect(door, w, 0.0, WALL), h0: low, h1: c },
            CityBox { rect: f.rect(-door, door, 0.0, WALL), h0: low + DOOR_HEIGHT, h1: c },
            CityBox {
                rect: f.rect(-cw, cw, d - COUNTER_GAP - COUNTER_DEPTH, d - COUNTER_GAP),
                h0: low,
                h1: low + COUNTER_HEIGHT,
            },
        ];
        out.copy_from_slice(&boxes);
        MAX_SOLIDS
    }

    /// How wide it is along its front, and how deep from the front face to the back wall, m.
    pub fn width(&self) -> f32 {
        if self.front.along_s { self.rect.width() } else { self.rect.length() }
    }

    pub fn depth(&self) -> f32 {
        WALL + if self.front.along_s { self.rect.length() } else { self.rect.width() }
    }

    /// Whether a point of its strip is in it, under its ceiling: `(s, x)`, `h` up.
    pub fn holds(&self, s: f32, x: f32, h: f32) -> bool {
        self.rect.contains(s, x) && (-1.0..self.ceiling).contains(&h)
    }

    /// Where a pilot stands to use its counter, and which way they face: `(s, x)` and a unit
    /// `(ds, dx)`.
    pub fn counter_spot(&self) -> ((f32, f32), (f32, f32)) {
        let v = self.depth() - COUNTER_GAP - COUNTER_DEPTH - COUNTER_STAND;
        (self.front.point(0.0, v), self.inward)
    }

    /// Just inside its door, and just outside it: a walk in goes from the one to the other.
    pub fn threshold(&self) -> ((f32, f32), (f32, f32)) {
        (self.front.point(0.0, -2.0), self.front.point(0.0, WALL + 2.0))
    }
}

/// Place `i`'s room behind its door, if it has one (Hub Gate's terminal doesn't).
pub fn room(i: usize) -> Option<Room> {
    let p = PLACES.get(i)?;
    let (depth, width, ceiling) = crate::content::city::room_size(p.kind)?;
    let foot = block_rect(p.bx, p.row).inset(SIDEWALK);
    let (ms, mx) = foot.middle();
    let front = match p.door_x {
        1 => Front { along_s: true, face: foot.x1, middle: ms, sign: -1.0 },
        -1 => Front { along_s: true, face: foot.x0, middle: ms, sign: 1.0 },
        _ if p.row < 0 => Front { along_s: false, face: foot.s1, middle: mx, sign: -1.0 },
        _ => Front { along_s: false, face: foot.s0, middle: mx, sign: 1.0 },
    };
    let (w, d) = (width * 0.5, depth);
    let cw = width * COUNTER_SHARE * 0.5;
    let inward = if front.along_s { (0.0, front.sign) } else { (front.sign, 0.0) };
    Some(Room {
        place: i as u8,
        strip: p.strip,
        rect: front.rect(-w, w, WALL, d),
        ceiling: KERB + ceiling,
        door: front.rect(-DOOR_WIDTH * 0.5, DOOR_WIDTH * 0.5, 0.0, WALL),
        inward,
        counter: front.rect(-cw, cw, d - COUNTER_GAP - COUNTER_DEPTH, d - COUNTER_GAP),
        front,
    })
}

/// The room a point of strip `strip` is in (`(s, x)`, `h` up), if any.
pub fn room_at(strip: u8, s: f32, x: f32, h: f32) -> Option<Room> {
    (0..PLACES.len()).filter_map(room).find(|r| r.strip == strip && r.holds(s, x, h))
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
            form: Form::Block,
            roof: Rooftop::Bare,
            style: Style::Plain,
            seed: 0,
            room: None,
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

/// A seat in the city: on a bench against a place's front, facing out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    pub strip: u8,
    /// Across the strip and along the colony, m.
    pub s: f32,
    pub x: f32,
    /// The way a pilot sitting on it faces: the walker's yaw (0 faces −s, τ/4 faces +x).
    pub yaw: f32,
}

/// How many seats The Arrival puts out.
pub const ARRIVAL_SEATS: usize = 4;
/// How far along its front from the door the seats are, m: two benches of two, either side.
const SEAT_OFFSETS: [f32; ARRIVAL_SEATS] = [-5.5, -4.5, 4.5, 5.5];
/// A pilot this close to a seat can sit on it, m.
pub const SEAT_REACH: f32 = 1.6;

/// The Arrival's seats: two benches either side of its door, a metre out from its front, facing
/// the avenue. A closed form of the place's door, like everything in the city.
pub fn arrival_seats() -> [Seat; ARRIVAL_SEATS] {
    let bar = PLACES.iter().find(|p| p.kind == PlaceKind::Bar).unwrap_or(&PLACES[0]);
    let ((s, x), (ds, dx)) = place_door(bar);
    // The door's spot is 4 m out from the front; the benches 1 m out, along it either side.
    let (fs, fx) = (s + ds * 3.0, x + dx * 3.0);
    let (along_s, along_x) = (-dx, ds);
    let yaw = atan2(-dx, ds);
    SEAT_OFFSETS.map(|k| Seat { strip: bar.strip, s: fs + along_s * k, x: fx + along_x * k, yaw })
}

/// The seat within [`SEAT_REACH`] of `(s, x)` on `strip`, nearest first.
pub fn seat_near(strip: u8, s: f32, x: f32) -> Option<usize> {
    let d = |t: &Seat| (t.s - s) * (t.s - s) + (t.x - x) * (t.x - x);
    arrival_seats()
        .iter()
        .enumerate()
        .filter(|(_, t)| t.strip == strip && d(t) <= SEAT_REACH * SEAT_REACH)
        .min_by(|a, b| d(a.1).total_cmp(&d(b.1)))
        .map(|(k, _)| k)
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

/// Block `b`'s buildings.
pub fn lots(b: &BlockInfo) -> Lots {
    lots_near(b, &Rect::new(f32::MIN, f32::MAX, f32::MIN, f32::MAX))
}

/// Block `b`'s buildings that stand on `near` (on a block of lots, the rest aren't worked out).
fn lots_near(b: &BlockInfo, near: &Rect) -> Lots {
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
                    if !foot.overlaps(near) {
                        continue;
                    }
                    // Its sides on a street (the block's edges).
                    let sides = if j == 0 { SIDE_S0 } else { 0 }
                        | if j == ns - 1 { SIDE_S1 } else { 0 }
                        | if i == 0 { SIDE_X0 } else { 0 }
                        | if i == nx - 1 { SIDE_X1 } else { 0 };
                    // On the streets of homes and shops, a corner stands a storey taller.
                    let corner = sides & (SIDE_S0 | SIDE_S1) != 0 && sides & (SIDE_X0 | SIDE_X1) != 0;
                    let lifts = matches!(
                        d,
                        DistrictKind::Midtown | DistrictKind::Residential | DistrictKind::OldTown
                    );
                    let n = storeys_of(rule.floors.0, rule.floors.1, unit(ls, 2));
                    let n = (n + u32::from(corner && lifts)).min(rule.floors.1);
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
                    let (height, form, roof) = mass(d, b.strip, foot, sides, n, tower, style, ls);
                    out.push(Building { foot, height, form, roof, style, seed: ls, room: None });
                }
            }
        }
        BlockKind::Park => {
            // A pavilion or two among the trees, inside the lamps beside the loop of its path
            // (`furniture::park`) and a metre clear of them.
            let area = b.rect.inset(SIDEWALK + PARK_LOOP + PARK_LAMP + 1.0);
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
                    form: Form::Block,
                    roof: Rooftop::Bare,
                    style: Style::Pavilion,
                    seed: ls,
                    room: None,
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
                form: Form::Monument,
                roof: Rooftop::Bare,
                style: Style::Monument,
                seed,
                room: None,
            });
        }
        BlockKind::Tower(h) => {
            // A landmark: its top is `h` (a mast's, on a tower), its form its district's.
            let foot = area.inset(10.0);
            let h = h.min(MAX_HEIGHT);
            let shaft = foot.inset((min_side(&foot) * 0.18).min(20.0));
            let (height, form) = match b.district.map(|(_, d)| d) {
                Some(DistrictKind::Works | DistrictKind::Port) => {
                    (h - MONITOR, Form::Shed { roof: ShedRoof::Monitor, office: 0, stack: false })
                }
                Some(DistrictKind::Park | DistrictKind::Residential | DistrictKind::University) => (
                    floors_height(4),
                    Form::Tower { shaft, tiers: 3, crown: Crown::Lantern, top: h, mast: 0.0 },
                ),
                _ => {
                    let tall = h >= 160.0;
                    let top = h - (0.12 * h).max(10.0);
                    let crown = if tall { Crown::Stepped } else { Crown::Hat };
                    (
                        floors_height(4),
                        Form::Tower { shaft, tiers: if tall { 3 } else { 1 }, crown, top, mast: h },
                    )
                }
            };
            out.push(Building {
                foot,
                height,
                form,
                roof: Rooftop::Bare,
                style: Style::Tower,
                seed,
                room: None,
            });
        }
        BlockKind::Place(i) => {
            let height = match PLACES[i as usize].kind {
                PlaceKind::Bar => 12.0,
                PlaceKind::Exchange => 24.0,
                PlaceKind::Charter => 36.0,
                PlaceKind::HubGate => TERMINAL_HEIGHT,
            };
            out.push(Building {
                foot: area,
                height,
                form: Form::Block,
                roof: Rooftop::Bare,
                style: Style::Hall,
                seed,
                room: room(usize::from(i)),
            });
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
                out.push(Building {
                    foot,
                    height,
                    form: Form::Block,
                    roof: Rooftop::Bare,
                    style: Style::Frame,
                    seed: ls,
                    room: None,
                });
            }
            if unit(seed, 3) < 0.45 {
                let (s, x) = area.middle();
                let foot = Rect::new(s - 1.5, s + 1.5, x - 1.5, x + 1.5);
                out.push(Building {
                    foot,
                    height: 80.0 + 80.0 * unit(seed, 4),
                    form: Form::Block,
                    roof: Rooftop::Bare,
                    style: Style::Crane,
                    seed: mix(seed, 13, 0),
                    room: None,
                });
            }
        }
        BlockKind::Canal => {}
    }
    out
}

/// A building's height from `lo..=hi` floors with draw `u`: its storeys.
fn storeys_of(lo: u32, hi: u32, u: f32) -> u32 {
    (lo + ((hi - lo + 1) as f32 * u) as u32).min(hi)
}

/// How a building on a lot is massed, by its district and strip: its body's roof, its form and
/// what's on its roof. `n` is its storeys, `sides` the lot's sides on a street, `tower` its tower
/// if it has one (its shaft's foot and top).
#[allow(clippy::too_many_arguments)]
fn mass(
    d: DistrictKind,
    strip: u8,
    foot: Rect,
    sides: u8,
    n: u32,
    tower: Option<(Rect, f32)>,
    style: Style,
    ls: u32,
) -> (f32, Form, Rooftop) {
    let u = |k: u32| unit(ls, k);
    let height = floors_height(n);
    let small = min_side(&foot);
    // Plant, or water tanks on the Canal's roofs.
    let plant = if strip == 1 && u(15) < 0.5 { Rooftop::Tanks } else { Rooftop::Plant };
    if let Some((shaft, top)) = tower {
        // Taller towers step in more often, and wear masts.
        let tiers = ((1.0 + u(6) * (1.6 + (top - 80.0) / 60.0)) as u8).clamp(1, 3);
        let c = u(7);
        let crown = match d {
            DistrictKind::Business => {
                if c < 0.15 {
                    Crown::Flat
                } else if c < 0.45 {
                    Crown::Hat
                } else if c < 0.7 {
                    Crown::Stepped
                } else if c < 0.85 {
                    Crown::Lantern
                } else {
                    Crown::Offset
                }
            }
            _ => {
                if c < 0.3 {
                    Crown::Flat
                } else if c < 0.7 {
                    Crown::Hat
                } else {
                    Crown::Offset
                }
            }
        };
        let masted = top >= 120.0 && u(8) < if d == DistrictKind::Business { 0.4 } else { 0.15 };
        let mast = if masted { (top + 15.0 + 30.0 * u(9)).min(MAX_HEIGHT) } else { 0.0 };
        let tiers = if d == DistrictKind::Business { tiers } else { tiers.min(2) };
        return (height, Form::Tower { shaft, tiers, crown, top, mast }, Rooftop::Plant);
    }
    // Its top `k` storeys set back from `sides`.
    let setback = |k: u32, sides: u8, step: f32| {
        let k = k.min(n.saturating_sub(1));
        (floors_height(n - k), Form::Setback { sides, tiers: k.min(2) as u8, step, top: height })
    };
    let street = if sides == 0 { SIDES } else { sides };
    let block = |roof| (height, Form::Block, roof);
    match d {
        DistrictKind::Business | DistrictKind::Civic if style == Style::Slab => block(Rooftop::Plant),
        DistrictKind::Business => {
            if u(6) < 0.5 {
                let (h, f) = setback(1, SIDES, 3.0);
                (h, f, Rooftop::Plant)
            } else {
                block(Rooftop::Plant)
            }
        }
        DistrictKind::Civic => {
            if u(6) < 0.7 {
                // An attic storey, and on the big ones a lantern over it.
                let (h, f) = setback(1, SIDES, 2.0);
                (h, f, if small >= 36.0 && u(7) < 0.5 { Rooftop::Lantern } else { Rooftop::Plant })
            } else {
                block(Rooftop::Plant)
            }
        }
        DistrictKind::Midtown => {
            if n >= 9 && u(6) < 0.65 {
                // The street wall to three fifths of it or so, then one or two steps back.
                let body = ((n as f32 * (0.6 + 0.15 * u(9))) as u32).max(5);
                let tiers = 1 + u32::from(u(7) < 0.45);
                (
                    floors_height(body),
                    Form::Setback { sides: street, tiers: tiers as u8, step: 3.0 + 1.5 * u(8), top: height },
                    plant,
                )
            } else {
                block(plant)
            }
        }
        DistrictKind::Residential => {
            let k = u(6);
            let gardens = strip == 2;
            if gardens && n >= 4 && sides != 0 && k < 0.45 {
                // The Gardens' terraces, stepping down to one of its streets.
                let face = [SIDE_S0, SIDE_S1, SIDE_X0, SIDE_X1]
                    .into_iter()
                    .filter(|b| sides & b != 0)
                    .nth((u(7) * sides.count_ones() as f32) as usize)
                    .unwrap_or(SIDE_S0);
                let tiers = 2 + u32::from(n >= 7 && u(8) < 0.5);
                (
                    floors_height((n / 2).max(2)),
                    Form::Setback { sides: face, tiers: tiers as u8, step: 4.0 + u(9), top: height },
                    Rooftop::Bare,
                )
            } else if small >= 26.0 && sides != 0 && k < if gardens { 0.7 } else { 0.3 } {
                court(foot, sides, n, false, ls, plant)
            } else if n >= 4 && k < if gardens { 0.85 } else { 0.55 } {
                // A penthouse storey set back from the street.
                let (h, f) = setback(1, street, 2.5);
                (h, f, plant)
            } else if u(10) < 0.25 {
                block(Rooftop::Bare)
            } else {
                block(plant)
            }
        }
        DistrictKind::OldTown => {
            if n >= 4 && sides != 0 {
                // A mansard's attic, under the old town's eaves.
                let (h, f) = setback(1, sides, 1.8);
                (h, f, Rooftop::Bare)
            } else if n >= 4 {
                block(Rooftop::Bare)
            } else {
                block(Rooftop::Chimneys { walls: SIDES & !sides })
            }
        }
        DistrictKind::University => {
            if small >= 26.0 && sides != 0 && u(6) < 0.75 {
                court(foot, sides, n, u(7) < 0.25, ls, Rooftop::Plant)
            } else {
                block(Rooftop::Plant)
            }
        }
        DistrictKind::Works | DistrictKind::Port => {
            let r = u(6);
            let port = d == DistrictKind::Port;
            let roof = if style == Style::Slab || (port && r < 0.5) || (!port && (0.4..0.7).contains(&r)) {
                ShedRoof::Monitor
            } else if r < if port { 0.75 } else { 0.4 } {
                ShedRoof::Sawtooth
            } else {
                ShedRoof::Bays
            };
            let corner = sides & (SIDE_S0 | SIDE_S1) != 0 && sides & (SIDE_X0 | SIDE_X1) != 0;
            let office = if port && corner && roof != ShedRoof::Bays && u(7) < 0.6 { sides } else { 0 };
            // Keep the office's corner to one s side and one x side.
            let office = if office & SIDE_S0 != 0 { office & !SIDE_S1 } else { office };
            let office = if office & SIDE_X0 != 0 { office & !SIDE_X1 } else { office };
            (height, Form::Shed { roof, office, stack: !port && u(8) < 0.35 }, Rooftop::Bare)
        }
        DistrictKind::Park => block(Rooftop::Bare),
    }
}

/// An L, a U, an H or a court on a lot with streets on its sides `sides`: wings along them (a lot
/// on one street gets wings back from it too), round a yard.
fn court(foot: Rect, sides: u8, n: u32, campanile: bool, ls: u32, roof: Rooftop) -> (f32, Form, Rooftop) {
    let u = |k: u32| unit(ls, k);
    let depth = (0.4 * min_side(&foot)).clamp(10.0, 14.0);
    let mut wings = sides;
    if wings.count_ones() == 1 {
        // Back from the street: two wings if it's long enough for a yard between, else one.
        let along_x = wings & (SIDE_S0 | SIDE_S1) != 0;
        let (len, a, b) =
            if along_x { (foot.length(), SIDE_X0, SIDE_X1) } else { (foot.width(), SIDE_S0, SIDE_S1) };
        wings |= if len >= 3.0 * depth + 8.0 && u(11) < 0.6 {
            a | b
        } else if u(12) < 0.5 {
            a
        } else {
            b
        };
    }
    // Room for a yard between opposite wings.
    if wings & SIDE_S0 != 0 && wings & SIDE_S1 != 0 && foot.width() < 2.0 * depth + 8.0 {
        wings &= !SIDE_S1;
    }
    if wings & SIDE_X0 != 0 && wings & SIDE_X1 != 0 && foot.length() < 2.0 * depth + 8.0 {
        wings &= !SIDE_X1;
    }
    // A yard is always open on a side: nobody's walled in.
    if wings == SIDES {
        wings &= !SIDE_X1;
    }
    let low = floors_height(n.saturating_sub(u32::from(u(13) < 0.5)).max(2));
    (floors_height(n), Form::Court { wings, depth, low, campanile }, roof)
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
/// Not the street furniture (`furniture::each_furniture`): a suit steps over it.
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
    let mut boxes = [CityBox::default(); MAX_SOLIDS];
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
            for building in lots_near(&b, area).as_slice() {
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
/// touches anything solid to a person or a car: the built city ([`solid_built`]) and its street
/// furniture (`furniture`: lamp posts, trees' trunks, benches).
pub fn solid(strip: u8, min: Vec3, max: Vec3, stage: Stage) -> bool {
    if solid_built(strip, min, max, stage) {
        return true;
    }
    let area = Rect::new(-max.z, -min.z, min.x, max.x);
    super::furniture::each_furniture(strip, &area, stage, |p| min.y < p.solid.h1 && p.solid.h0 < max.y)
}

/// Whether the box touches the built city: the floor, a kerb, a building, a railing, a platform,
/// the end caps. That's [`solid`] without the street furniture, which a mobile suit steps over:
/// what a suit's hull meets (`interior`, through [`each_solid`]).
pub fn solid_built(strip: u8, min: Vec3, max: Vec3, stage: Stage) -> bool {
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

    /// The Arrival's seats stand clear of its walls, beside its door and either side of it, facing
    /// away from the bar; one is found from where a pilot stands by it, none from the door itself.
    #[test]
    fn the_arrivals_seats_are_out_front_facing_the_avenue() {
        let bar = PLACES.iter().find(|p| p.kind == PlaceKind::Bar).unwrap();
        let ((s, x), (ds, dx)) = place_door(bar);
        let seats = arrival_seats();
        for (k, seat) in seats.iter().enumerate() {
            assert_eq!(seat.strip, bar.strip);
            // Seated (a box from the seat's height up), clear of the walls.
            assert!(
                !solid(
                    seat.strip,
                    Vec3::new(seat.x - 0.2, 0.5, -seat.s - 0.2),
                    Vec3::new(seat.x + 0.2, 1.3, -seat.s + 0.2),
                    STAGE
                ),
                "seat {k} is in a wall"
            );
            let off = (seat.s - s).hypot(seat.x - x);
            assert!((3.0..8.0).contains(&off), "seat {k} is {off} m from the door");
            // Facing out: the way the door goes in, turned round (yaw 0 faces −s, τ/4 faces +x).
            let face = (-crate::math::cos(seat.yaw), crate::math::sin(seat.yaw));
            assert!((face.0 + ds).abs() < 1e-4 && (face.1 + dx).abs() < 1e-4, "seat {k} faces {face:?}");
            assert_eq!(seat_near(seat.strip, seat.s + 0.5, seat.x + 0.3), Some(k));
        }
        assert_eq!(seat_near(bar.strip, s, x), None, "the door's spot isn't a seat");
        assert_eq!(seat_near((bar.strip + 1) % STRIPS as u8, seats[0].s, seats[0].x), None, "another strip");
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
        let mut pieces = [Piece::default(); MAX_SOLIDS];
        let mut boxes = [CityBox::default(); MAX_SOLIDS];
        for k in 0..STRIPS as u8 {
            for bx in HUB_GATE.0..=SITE.1 {
                for row in -ROWS..=ROWS {
                    let Some(b) = block(k, bx, row, STAGE) else { continue };
                    for bd in lots(&b).as_slice() {
                        seen += 1;
                        assert!(b.rect.inset(SIDEWALK - 1e-3).holds(&bd.foot), "{bd:?} spills out of {b:?}");
                        assert!(bd.foot.width() > 0.5 && bd.foot.length() > 0.5);
                        assert!(bd.top() <= MAX_HEIGHT && bd.height > 0.0, "{bd:?}");
                        let n = bd.pieces(&mut pieces);
                        let ps = &pieces[..n];
                        assert!(n > 0, "{bd:?} is nothing");
                        // Its solids are its pieces' boxes; its top, the highest of them.
                        assert_eq!(bd.solids(&mut boxes), n);
                        assert!(ps.iter().zip(&boxes).all(|(q, b)| q.b == *b));
                        let top = ps.iter().fold(0.0f32, |m, q| m.max(q.b.h1));
                        assert!((bd.top() - top).abs() < 1e-4, "{bd:?}: top {} vs {top}", bd.top());
                        for (i, q) in ps.iter().enumerate() {
                            let (r, h0, h1) = (q.b.rect, q.b.h0, q.b.h1);
                            assert!(bd.foot.holds(&r), "{bd:?}: piece {i} {q:?} out of its foot");
                            assert!(h0 >= KERB - 1e-4 && h1 <= MAX_HEIGHT && h1 >= h0 + 0.5, "{bd:?}: {q:?}");
                            assert!(r.width() >= 0.8 && r.length() >= 0.8, "{bd:?}: {q:?} too thin");
                            if bd.room.is_some() {
                                continue;
                            }
                            // Nothing floats or overhangs: off the street, a piece stands on the roof
                            // of one laid before it, which holds all of it.
                            if h0 > KERB + 1e-3 {
                                assert!(
                                    ps[..i].iter().any(|o| (o.b.h1 - h0).abs() < 1e-3 && o.b.rect.holds(&r)),
                                    "{bd:?}: piece {i} {q:?} stands on nothing"
                                );
                            }
                            // And no piece is inside another (no hidden faces, no fighting ones).
                            for o in &ps[..i] {
                                let apart =
                                    !o.b.rect.overlaps(&r) || o.b.h1 <= h0 + 1e-4 || h1 <= o.b.h0 + 1e-4;
                                assert!(apart, "{bd:?}: {q:?} is inside {o:?}");
                            }
                        }
                    }
                }
            }
        }
        assert!(seen > 30_000, "{seen} buildings");
    }

    #[test]
    fn the_skyline_has_crowns_setbacks_and_spires() {
        // Across the three strips, the districts mass as they should: towers step in and are
        // crowned, the tall ones wear masts; midtown steps back; homes have plant and tanks on their
        // roofs and wings round yards; the Gardens terrace; the old town has chimneys and attics; the
        // works have rooflights and stacks; the university its quads and campaniles.
        let mut pieces = [Piece::default(); MAX_SOLIDS];
        let mut count = |d: DistrictKind, test: &dyn Fn(&Building, &[Piece]) -> bool| {
            let (mut all, mut hits) = (0, 0);
            for k in 0..STRIPS as u8 {
                for bx in CITY.0..=CITY.1 {
                    for row in -ROWS..=ROWS {
                        let Some(b) = block(k, bx, row, STAGE) else { continue };
                        if b.kind != BlockKind::Buildings || b.district.map(|x| x.1) != Some(d) {
                            continue;
                        }
                        for bd in lots(&b).as_slice() {
                            let n = bd.pieces(&mut pieces);
                            all += 1;
                            hits += usize::from(test(bd, &pieces[..n]));
                        }
                    }
                }
            }
            hits as f32 / all.max(1) as f32
        };
        let has = |ps: &[Piece], part: Part| ps.iter().filter(|q| q.part == part).count();
        use DistrictKind::*;
        let towers = count(Business, &|bd, _| bd.style == Style::Tower);
        let stepped = count(Business, &|bd, ps| bd.style == Style::Tower && has(ps, Part::Tier) >= 2);
        let crowned = count(Business, &|bd, ps| bd.style == Style::Tower && has(ps, Part::Crown) >= 1);
        let masted = count(Business, &|_, ps| has(ps, Part::Mast) >= 1);
        assert!(
            towers > 0.5 && stepped > 0.25 * towers && crowned > 0.6 * towers,
            "{towers} {stepped} {crowned}"
        );
        assert!(masted > 0.05 && masted < 0.3, "{masted} of the business district masted");
        let setbacks = count(Midtown, &|_, ps| has(ps, Part::Tier) >= 1);
        assert!(setbacks > 0.3, "midtown sets back: {setbacks}");
        let plant = count(Residential, &|_, ps| has(ps, Part::Plant) >= 1);
        let courts = count(Residential, &|_, ps| has(ps, Part::Body) >= 2);
        assert!(plant > 0.3 && courts > 0.08, "homes: {plant} with plant, {courts} round yards");
        let chimneys = count(OldTown, &|_, ps| has(ps, Part::Plant) >= 1);
        let attics = count(OldTown, &|_, ps| has(ps, Part::Tier) >= 1);
        assert!(chimneys > 0.3 && attics > 0.15, "the old town: {chimneys} chimneys, {attics} attics");
        let lights = count(Works, &|_, ps| has(ps, Part::Plant) >= 1 || has(ps, Part::Body) >= 2);
        let stacks = count(Works, &|_, ps| has(ps, Part::Mast) >= 1);
        assert!(lights > 0.7 && stacks > 0.15, "the works: {lights} roofed, {stacks} stacks");
        let quads = count(University, &|_, ps| has(ps, Part::Body) >= 2);
        let campaniles = count(University, &|_, ps| has(ps, Part::Mast) >= 1);
        assert!(quads > 0.5 && campaniles > 0.08, "the university: {quads} quads, {campaniles} campaniles");
        // Towers narrow as they go up: each tier on the one below, and the crown on the top tier.
        for k in 0..STRIPS as u8 {
            for bx in CITY.0..=CITY.1 {
                for row in -ROWS..=ROWS {
                    let Some(b) = block(k, bx, row, STAGE) else { continue };
                    for bd in lots(&b).as_slice().iter().filter(|bd| bd.style == Style::Tower) {
                        let n = bd.pieces(&mut pieces);
                        let mut under = bd.foot;
                        for q in pieces[..n].iter().filter(|q| matches!(q.part, Part::Tier | Part::Crown)) {
                            assert!(under.holds(&q.b.rect), "{bd:?}: {q:?} is wider than what's under it");
                            under = q.b.rect;
                        }
                    }
                }
            }
        }
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
            crate::colony::furniture::each_furniture(k, &wide, STAGE, |p| {
                want |= p.solid.rect.overlaps(&area) && min.y < p.solid.h1 && p.solid.h0 < max.y;
                false
            });
            assert_eq!(got, want, "{k} {min} {max}");
        }
    }

    #[test]
    fn every_key_places_door_is_on_a_street_facing_it() {
        for (i, p) in PLACES.iter().enumerate() {
            let ((s, x), (ds, dx)) = place_door(p);
            assert!(!walker_at(p.strip, s, x, 0.0), "{}'s door is in a wall", p.name);
            // Further on, the way in, is the building: its front wall beside the door (and through
            // the door, its room), or Hub Gate's terminal.
            let aside = if room(i).is_some() { DOOR_WIDTH } else { 0.0 };
            let (a, b) = (s + ds * 9.5 + dx * aside, x + dx * 9.5 + ds * aside);
            assert!(walker_at(p.strip, a, b, KERB + 0.01), "{} has nothing to go into", p.name);
            if p.kind != PlaceKind::HubGate {
                let b = block(p.strip, p.bx, p.row, STAGE).unwrap();
                assert_eq!(b.kind, BlockKind::Place(PLACES.iter().position(|q| q == p).unwrap() as u8));
            }
            assert!(place(p.slug).is_some());
        }
    }

    #[test]
    fn the_key_places_rooms_are_walked_into_and_used_at_their_counters() {
        let mut rooms = 0;
        for (i, p) in PLACES.iter().enumerate() {
            let Some(room) = room(i) else {
                assert_eq!(p.kind, PlaceKind::HubGate, "{} has no room", p.name);
                continue;
            };
            rooms += 1;
            let b = block(p.strip, p.bx, p.row, STAGE).unwrap();
            let hall = lots(&b).as_slice()[0];
            assert_eq!(hall.room, Some(room));
            assert!(hall.foot.holds(&room.rect) && room.ceiling + 1.0 < hall.height, "{}", p.name);
            // From the door's spot on the street, through the door, to the counter: nothing in the
            // way of a walker.
            let (door, _) = place_door(p);
            let (outside, inside) = room.threshold();
            let (spot, (fs, fx)) = room.counter_spot();
            for (a, b) in [(door, outside), (outside, inside), (inside, spot)] {
                for k in 0..=200 {
                    let t = k as f32 / 200.0;
                    let (s, x) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    assert!(!walker_at(p.strip, s, x, KERB + 0.01), "{}: blocked at ({s}, {x})", p.name);
                }
            }
            // The counter is just in front of the spot, and the way the spot faces is in.
            assert_eq!((fs, fx), room.inward);
            assert!(
                walker_at(p.strip, spot.0 + fs * 1.5, spot.1 + fx * 1.5, KERB + 0.01),
                "{}: no counter",
                p.name
            );
            assert!(room.rect.holds(&room.counter) && room.rect.contains(spot.0, spot.1));
            // The front wall stands either side of the door, the back wall behind the counter, the
            // side walls and the ceiling round the room: nobody walks out but by the door.
            let (ms, mx) = room.door.middle();
            let (us, ux) = (fx.abs(), fs.abs());
            for side in [-1.0f32, 1.0] {
                let (s, x) =
                    (ms + us * side * (DOOR_WIDTH * 0.5 + 0.8), mx + ux * side * (DOOR_WIDTH * 0.5 + 0.8));
                assert!(walker_at(p.strip, s, x, KERB + 0.01), "{}: no front wall at ({s}, {x})", p.name);
            }
            let (rs, rx) = room.rect.middle();
            let half_w = room.width() * 0.5 + 0.5;
            assert!(
                walker_at(p.strip, rs + us * half_w, rx + ux * half_w, KERB + 0.01),
                "{}: no side wall",
                p.name
            );
            let back = room.depth() + 0.5;
            let (bs, bx) = (ms + fs * back, mx + fx * back);
            assert!(walker_at(p.strip, bs, bx, KERB + 0.01), "{}: no back wall at ({bs}, {bx})", p.name);
            assert!(walker_at(p.strip, rs, rx, room.ceiling - 1.0), "{}: no ceiling", p.name);
            assert!(!walker_at(p.strip, rs, rx, room.ceiling - 2.0), "{}: the ceiling's too low", p.name);
            // Who's in it, and who isn't.
            assert_eq!(room_at(p.strip, spot.0, spot.1, KERB).map(|r| r.place), Some(i as u8));
            assert_eq!(room_at(p.strip, door.0, door.1, KERB), None, "{}'s street isn't its room", p.name);
            assert_eq!(room_at(p.strip, spot.0, spot.1, room.ceiling + 2.0), None, "over the ceiling");
            assert_eq!(room_at((p.strip + 1) % STRIPS as u8, spot.0, spot.1, KERB), None);
        }
        assert_eq!(rooms, 3, "the bar, the Exchange floor and the Charter Board");
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
