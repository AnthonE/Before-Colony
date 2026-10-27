//! What each piece is made of, as boxes in its building's frame: the one table both its solids
//! (what a walker meets, and the server's pose check) and its drawing (`bc_client_core::piece_mesh`,
//! which dresses each box inside itself) are taken from, as Gates' `shape_parts` is
//! (`AnthonE/Gates`, MIT, `client/src/render/structures.rs`). What's drawn is what's walked.
//!
//! The first cut is square, as the colony's first buildings are: a piece's footprint is a square
//! with its sides along the frame's axes, an edge along one of them, or a flight climbing along one
//! of them (`Sock::dir` 0, 3, 6 or 9). Triangles and the 30° turns between come later, as prisms.
//!
//! The building's frame is the walker's on its deck turned to the building (`frame::Frame`): x and
//! z across the deck, y up from the building's ground floor.

use glam::Vec3;

use super::lattice::LPt;
use super::shape::*;
use super::sock::{FOOT_EDGE, FOOT_RISER, FOOT_SQUARE, PieceAddr, Sock};

/// A wall's thickness, m.
pub const WALL_THICKNESS: f32 = 0.24;
/// A floor's, a roof's and a foundation's, m.
pub const SLAB: f32 = 0.3;
/// A doorway's posts either side of its opening, m (a 1.2 m opening).
pub const DOOR_POST: f32 = 0.9;
/// The top of a doorway's and a window's opening, m over its storey's floor.
pub const OPENING_HEAD: f32 = 2.2;
/// A window's sill, m over its storey's floor.
pub const WINDOW_SILL: f32 = 1.0;
/// A wall frame's rim, m.
pub const FRAME_RIM: f32 = 0.15;
/// The treads up a storey's flight: each this high and this deep, m (under the walker's 0.45 m
/// step).
pub const TREADS: usize = 8;
pub const TREAD: f32 = LEVEL_H_M / TREADS as f32;

/// The most boxes one piece is made of.
pub const MAX_PARTS: usize = TREADS + 2;

/// A box, in a building's frame (or the walker's on a deck): from `min` to `max`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Whether it and `o` share any volume (touching faces don't).
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x < o.max.x
            && o.min.x < self.max.x
            && self.min.y < o.max.y
            && o.min.y < self.max.y
            && self.min.z < o.max.z
            && o.min.z < self.max.z
    }

    /// It and `o` together.
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
}

/// What a box of a piece is, for its drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// A foundation's, a floor's or a roof's slab.
    Slab,
    /// A wall, a half or low wall, or the solid of a window.
    Wall,
    /// A doorway's post or a frame's rim.
    Post,
    /// The beam over a doorway or a frame.
    Lintel,
    /// One tread of a flight.
    Tread,
}

/// A piece's boxes, and how many.
#[derive(Clone, Copy, Debug)]
pub struct Parts {
    pub boxes: [(Part, Aabb); MAX_PARTS],
    pub n: usize,
}

impl Parts {
    const fn empty() -> Self {
        Self { boxes: [(Part::Slab, Aabb { min: Vec3::ZERO, max: Vec3::ZERO }); MAX_PARTS], n: 0 }
    }

    fn push(&mut self, part: Part, b: Aabb) {
        self.boxes[self.n] = (part, b);
        self.n += 1;
    }

    pub fn as_slice(&self) -> &[(Part, Aabb)] {
        &self.boxes[..self.n]
    }
}

/// A lattice point's (x, z), when it's on the square grid (`b` and `c` naught), m.
const fn grid_xz(p: LPt) -> (f32, f32) {
    (p.a as f32 * 0.5, p.d as f32 * 0.5)
}

/// Whether a footprint is one the square first cut can build.
pub const fn square(sock: &Sock) -> bool {
    sock.at.b == 0
        && sock.at.c == 0
        && match sock.foot {
            FOOT_SQUARE => sock.dir == 0,
            FOOT_EDGE => sock.dir == 0 || sock.dir == 3,
            FOOT_RISER => sock.dir.is_multiple_of(3),
            _ => false,
        }
}

/// The boxes of a piece of `shape` at `a`, in its building's frame. Empty for a footprint the square
/// first cut doesn't build ([`square`]).
pub fn parts(shape: u8, a: &PieceAddr) -> Parts {
    let mut out = Parts::empty();
    if !square(&a.sock) {
        return out;
    }
    let y = level_y(a.level);
    let (cx, cz) = grid_xz(a.sock.at);
    let h = CELL_M * 0.5;
    match a.sock.foot {
        FOOT_SQUARE => {
            let (y0, y1) = match shape {
                FOUNDATION => (-SLAB, 0.0),
                _ => (y - SLAB, y),
            };
            out.push(Part::Slab, Aabb::new(Vec3::new(cx - h, y0, cz - h), Vec3::new(cx + h, y1, cz + h)));
        }
        FOOT_EDGE => {
            // Along x (dir 0) or z (dir 3): `t` along it from its low end, `w` across it.
            let along_x = a.sock.dir == 0;
            let t = WALL_THICKNESS * 0.5;
            let span = |t0: f32, t1: f32, y0: f32, y1: f32| {
                if along_x {
                    Aabb::new(Vec3::new(cx - h + t0, y + y0, cz - t), Vec3::new(cx - h + t1, y + y1, cz + t))
                } else {
                    Aabb::new(Vec3::new(cx - t, y + y0, cz - h + t0), Vec3::new(cx + t, y + y1, cz - h + t1))
                }
            };
            match shape {
                DOORWAY => {
                    out.push(Part::Post, span(0.0, DOOR_POST, 0.0, LEVEL_H_M));
                    out.push(Part::Post, span(CELL_M - DOOR_POST, CELL_M, 0.0, LEVEL_H_M));
                    out.push(Part::Lintel, span(DOOR_POST, CELL_M - DOOR_POST, OPENING_HEAD, LEVEL_H_M));
                }
                FRAME => {
                    out.push(Part::Post, span(0.0, FRAME_RIM, 0.0, LEVEL_H_M));
                    out.push(Part::Post, span(CELL_M - FRAME_RIM, CELL_M, 0.0, LEVEL_H_M));
                    out.push(
                        Part::Lintel,
                        span(FRAME_RIM, CELL_M - FRAME_RIM, LEVEL_H_M - FRAME_RIM, LEVEL_H_M),
                    );
                }
                // A window is solid to a body: its whole edge (its glass is drawn in it).
                _ => out.push(Part::Wall, span(0.0, CELL_M, 0.0, wall_height(shape))),
            }
        }
        FOOT_RISER => {
            // Treads climbing along `dir` (0: +x, 3: +z, 6: −x, 9: −z), each a box from the storey's
            // floor up to its own top.
            let (x0, z0) = (cx - h, cz - h);
            let rise = match shape {
                STAIRS | STAIRS_L | STAIRS_U => LEVEL_H_M,
                _ => LEVEL_H_M * 0.5,
            };
            let base = if shape == FOUNDATION_STEPS { y - rise } else { y };
            let step = CELL_M / TREADS as f32;
            for k in 0..TREADS {
                let top = base + rise * (k + 1) as f32 / TREADS as f32;
                let (u0, u1) = (step * k as f32, step * (k + 1) as f32);
                let b = match a.sock.dir {
                    0 => Aabb::new(Vec3::new(x0 + u0, base, z0), Vec3::new(x0 + u1, top, z0 + CELL_M)),
                    3 => Aabb::new(Vec3::new(x0, base, z0 + u0), Vec3::new(x0 + CELL_M, top, z0 + u1)),
                    6 => Aabb::new(
                        Vec3::new(x0 + CELL_M - u1, base, z0),
                        Vec3::new(x0 + CELL_M - u0, top, z0 + CELL_M),
                    ),
                    _ => Aabb::new(
                        Vec3::new(x0, base, z0 + CELL_M - u1),
                        Vec3::new(x0 + CELL_M, top, z0 + CELL_M - u0),
                    ),
                };
                out.push(Part::Tread, b);
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pieces::check::cell;

    fn at(level: u8, sock: Sock) -> PieceAddr {
        PieceAddr { bld: 0, level, sock }
    }

    #[test]
    fn a_floor_and_its_walls_close_a_room() {
        let body = Sock::square_from(cell(0, 0), 0);
        let floor = parts(FLOOR, &at(1, body));
        assert_eq!(floor.n, 1);
        let (_, slab) = floor.boxes[0];
        assert_eq!((slab.min, slab.max), (Vec3::new(0.0, 2.7, 0.0), Vec3::new(3.0, 3.0, 3.0)));
        let (sides, n) = body.sides();
        assert_eq!(n, 4);
        for e in &sides[..n] {
            let w = parts(WALL, &at(0, *e));
            assert_eq!(w.n, 1, "{e:?}");
            let b = w.boxes[0].1;
            // Each side's wall runs its whole side, a storey tall.
            assert!((b.max.y - b.min.y - LEVEL_H_M).abs() < 1e-6);
            let len = (b.max.x - b.min.x).max(b.max.z - b.min.z);
            assert!((len - CELL_M).abs() < 1e-6, "{e:?}: {b:?}");
        }
    }

    #[test]
    fn a_doorway_lets_a_walker_through_and_a_wall_does_not() {
        let e = Sock::edge_from(cell(2, 0), 3);
        let door = parts(DOORWAY, &at(0, e));
        // A walker's box (0.6 × 1.8 × 0.6) through the middle of it.
        let body = Aabb::new(Vec3::new(5.7, 0.01, 1.2), Vec3::new(6.3, 1.81, 1.8));
        assert!(door.as_slice().iter().all(|(_, b)| !b.overlaps(&body)));
        let wall = parts(WALL, &at(0, e));
        assert!(wall.as_slice().iter().any(|(_, b)| b.overlaps(&body)));
    }

    #[test]
    fn a_flight_climbs_a_storey_in_steps_a_walker_takes() {
        for dir in [0u8, 3, 6, 9] {
            let s = Sock::riser(Sock::square_from(cell(0, 0), 0).at, dir);
            let p = parts(STAIRS, &at(0, s));
            assert_eq!(p.n, TREADS);
            let mut last = 0.0;
            for (_, b) in p.as_slice() {
                assert!(b.max.y - last <= crate::foot::STEP, "dir {dir}");
                last = b.max.y;
            }
            assert!((last - LEVEL_H_M).abs() < 1e-5);
        }
    }
}
