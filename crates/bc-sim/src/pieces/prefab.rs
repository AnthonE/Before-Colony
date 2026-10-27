//! Buildings written as data: each a list of pieces placed on its lattice ([`Placed`]), in the
//! order they'd be built, and the rooms in it a pilot uses ([`RoomDef`]). A building stands on a deck
//! at a [`Site`]: where its frame's origin is, and which way it's turned (quarter turns, so every
//! solid stays a box in the deck's coordinates).
//!
//! Nothing here is stored or sent: a building's solids are its pieces' [`parts`](super::parts),
//! placed by its site, worked out where they're asked for, the same on the server and every client.
//! `tests/pieces.rs` places every building's pieces in order through [`check`](super::check), so
//! what's written stands as Gates' rules would let a player build it.

use glam::Vec3;

use super::check::cell;
use super::parts::{Aabb, parts};
use super::shape::*;
use super::sock::{PieceAddr, Sock};

/// One piece of a building: its shape, its level, its footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub shape: u8,
    pub level: u8,
    pub sock: Sock,
}

impl Placed {
    /// Its address in building `bld`.
    pub const fn addr(&self, bld: u16) -> PieceAddr {
        PieceAddr { bld, level: self.level, sock: self.sock }
    }
}

/// A foundation on cell (`i`, `j`).
pub const fn foundation(i: i32, j: i32) -> Placed {
    Placed { shape: FOUNDATION, level: 0, sock: Sock::square_from(cell(i, j), 0) }
}

/// A floor (or a roof, `shape`) over cell (`i`, `j`) at `level`.
pub const fn slab(shape: u8, i: i32, j: i32, level: u8) -> Placed {
    Placed { shape, level, sock: Sock::square_from(cell(i, j), 0) }
}

/// An edge piece of `shape` on the low-z side of cell (`i`, `j`), along +x.
pub const fn edge_x(shape: u8, i: i32, j: i32, level: u8) -> Placed {
    Placed { shape, level, sock: Sock::edge_from(cell(i, j), 0) }
}

/// An edge piece of `shape` on the low-x side of cell (`i`, `j`), along +z.
pub const fn edge_z(shape: u8, i: i32, j: i32, level: u8) -> Placed {
    Placed { shape, level, sock: Sock::edge_from(cell(i, j), 3) }
}

/// A flight on cell (`i`, `j`) at `level`, climbing along `dir` (0 +x, 3 +z, 6 −x, 9 −z).
pub const fn flight(shape: u8, i: i32, j: i32, level: u8, dir: u8) -> Placed {
    Placed { shape, level, sock: Sock::riser(Sock::square_from(cell(i, j), 0).at, dir) }
}

/// A room a pilot uses in a building: what it is, the cells it covers on its storey, and where to
/// stand at its counter (in the building's frame, m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomDef {
    pub name: &'static str,
    /// Its cells, (i0, j0) to (i1, j1) inclusive, on `level`.
    pub cells: (i32, i32, i32, i32),
    pub level: u8,
    pub counter: Vec3,
}

/// A building: its pieces in the order they're built, its rooms, and what it covers (`tests/pieces.rs`
/// holds every piece inside it).
#[derive(Clone, Copy, Debug)]
pub struct Prefab {
    pub name: &'static str,
    pub pieces: &'static [Placed],
    pub rooms: &'static [RoomDef],
    /// The cells it covers, (i0, j0) to (i1, j1) inclusive, and its storeys.
    pub cells: (i32, i32, i32, i32),
    pub storeys: u8,
}

impl Prefab {
    /// What it covers in its own frame, m: its cells and storeys, with a wall's thickness round them.
    pub fn bounds(&self) -> Aabb {
        let (i0, j0, i1, j1) = self.cells;
        let t = super::parts::WALL_THICKNESS;
        Aabb::new(
            Vec3::new(i0 as f32 * CELL_M - t, -super::parts::SLAB, j0 as f32 * CELL_M - t),
            Vec3::new(
                (i1 + 1) as f32 * CELL_M + t,
                self.storeys as f32 * LEVEL_H_M + t,
                (j1 + 1) as f32 * CELL_M + t,
            ),
        )
    }
}

/// Where a building stands: on deck `deck`, its frame's origin at (`x`, `s`) on the deck's floor,
/// turned `turn` quarter turns (anticlockwise seen from above: its +x towards the deck's −s).
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub deck: u8,
    pub x: f32,
    pub s: f32,
    pub turn: u8,
    pub prefab: &'static Prefab,
}

impl Site {
    /// A point of the building's frame in the walker's on its deck, `(x, h, −s)`.
    pub fn to_walker(&self, p: Vec3) -> Vec3 {
        let (x, z) = match self.turn % 4 {
            0 => (p.x, p.z),
            1 => (p.z, -p.x),
            2 => (-p.x, -p.z),
            _ => (-p.z, p.x),
        };
        Vec3::new(self.x + x, p.y, -self.s + z)
    }

    /// A box of the building's frame in the walker's on its deck.
    pub fn box_to_walker(&self, b: &Aabb) -> Aabb {
        let (a, c) = (self.to_walker(b.min), self.to_walker(b.max));
        Aabb::new(a.min(c), a.max(c))
    }

    /// Each of its solids, in the walker's frame on its deck.
    pub fn each_solid(&self, f: &mut impl FnMut(&Aabb)) {
        for p in self.prefab.pieces {
            for (_, b) in parts(p.shape, &p.addr(0)).as_slice() {
                f(&self.box_to_walker(b));
            }
        }
    }

    /// What it covers, in the walker's frame on its deck.
    pub fn bounds(&self) -> Aabb {
        self.box_to_walker(&self.prefab.bounds())
    }

    /// Whether the box `min..max` (the walker's frame on its deck) touches any of its solids.
    pub fn hits(&self, b: &Aabb) -> bool {
        if !self.bounds().overlaps(b) {
            return false;
        }
        let mut hit = false;
        self.each_solid(&mut |s| hit |= s.overlaps(b));
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIECES: [Placed; 3] = [foundation(0, 0), edge_x(WALL, 0, 0, 0), edge_z(DOORWAY, 1, 0, 0)];
    const HUT: Prefab = Prefab { name: "HUT", pieces: &PIECES, rooms: &[], cells: (0, 0, 0, 0), storeys: 1 };

    #[test]
    fn a_site_turns_its_building_by_quarter_turns_onto_its_deck() {
        for turn in 0..4u8 {
            let site = Site { deck: 0, x: 100.0, s: 50.0, turn, prefab: &HUT };
            // Its origin where it was put: x along the deck, −s across it.
            assert_eq!(site.to_walker(Vec3::ZERO), Vec3::new(100.0, 0.0, -50.0));
            // Turned, its boxes stay boxes the size they were.
            let b = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 3.0, 0.24));
            let w = site.box_to_walker(&b);
            let (dx, dz) = (w.max.x - w.min.x, w.max.z - w.min.z);
            let want = if turn % 2 == 0 { (3.0, 0.24) } else { (0.24, 3.0) };
            assert!((dx - want.0).abs() < 1e-4 && (dz - want.1).abs() < 1e-4, "turn {turn}: {w:?}");
            // Every solid inside its bounds.
            let bounds = site.bounds();
            site.each_solid(&mut |s| {
                assert!(
                    s.min.cmpge(bounds.min - Vec3::splat(1e-4)).all()
                        && s.max.cmple(bounds.max + Vec3::splat(1e-4)).all()
                )
            });
            // A walker in its doorway is clear; in its wall, it isn't.
            let door = Aabb::new(Vec3::new(2.7, 0.01, 1.2), Vec3::new(3.3, 1.81, 1.8));
            assert!(!site.hits(&site.box_to_walker(&door)), "turn {turn}");
            let wall = Aabb::new(Vec3::new(1.2, 0.01, -0.3), Vec3::new(1.8, 1.81, 0.3));
            assert!(site.hits(&site.box_to_walker(&wall)), "turn {turn}");
        }
    }
}
