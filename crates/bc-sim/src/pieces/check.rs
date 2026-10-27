//! Whether a building's pieces stand as they're written: each one placed in order answers to the
//! rules Gates' builder does (`AnthonE/Gates`, MIT, `build::check_site`): the spot (its level, its
//! footprint, the lattice, nothing there already, nothing it overlaps on its storey, no partial
//! walls through each other on one edge), and its support. What Gates asks of a player besides
//! (the cost, the reach, a claim, the terrain) has no place in content written by us.

use super::lattice::LPt;
use super::shape::*;
use super::sock::{FOOT_EDGE, FOOT_RISER, FOOT_TRI_RISER, PieceAddr};
use super::support::{Lookup, crosses, foot_of, grounded, on_lattice, supported};

/// Why a piece can't stand where it's written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refuse {
    /// Its level is out of range for its shape (a floor on the ground, a foundation in the air, a
    /// wall or a flight that would top the building's storeys).
    Level,
    /// Its footprint isn't its shape's kind, or isn't on the lattice's vertices.
    Footprint,
    /// Something already stands there, or it overlaps what does on its storey, or a partial wall on
    /// the same edge runs through it.
    Taken,
    /// Nothing holds it up.
    Support,
}

/// Whether a flight's shape is one.
pub const fn is_riser(shape: u8) -> bool {
    matches!(
        shape,
        STAIRS | FOUNDATION_STEPS | RAMP | STAIRS_L | STAIRS_U | STAIRS_SPIRAL | STAIRS_TRI_SPIRAL
    )
}

/// How far a flight climbs, m: a storey for stairs, half for the rest.
pub fn rise(shape: u8) -> f32 {
    match shape {
        STAIRS | STAIRS_L | STAIRS_U => LEVEL_H_M,
        _ => LEVEL_H_M * 0.5,
    }
}

/// What a building being checked has placed so far.
pub trait Placed: Lookup {
    /// Each piece placed, its shape and address, for the overlap rules.
    fn each(&self, f: &mut dyn FnMut(u8, &PieceAddr));
}

/// Whether a piece of `shape` may be placed at `a` among `p`'s.
pub fn check(p: &impl Placed, shape: u8, a: &PieceAddr) -> Result<(), Refuse> {
    let level = a.level;
    let top = MAX_LEVELS as f32 * LEVEL_H_M;
    let level_ok = (level as usize) < MAX_SOCKETS
        && (!is_edge(shape) || level_y(level) + wall_height(shape) <= top)
        && (!grounded(shape) || level == 0)
        && (!matches!(shape, FLOOR | ROOF | TRI_FLOOR | TRI_ROOF | FLOOR_FRAME | TRI_FLOOR_FRAME)
            || level >= 1)
        && (!is_riser(shape) || shape == FOUNDATION_STEPS || level_y(level) + rise(shape) <= top);
    if !level_ok {
        return Err(Refuse::Level);
    }
    if foot_of(shape) != a.sock.foot || !on_lattice(&a.sock) {
        return Err(Refuse::Footprint);
    }
    if p.shape_at(a).is_some() || conflict(p, shape, a) || vertical_edge_clash(p, shape, a) {
        return Err(Refuse::Taken);
    }
    if !supported(p, shape, a) {
        return Err(Refuse::Support);
    }
    Ok(())
}

/// Whether anything on `a`'s storey takes the space it would (Gates' `site_conflict`, in one
/// building): two edges crossing, an edge through a body, bodies overlapping, two flights on one
/// body. A flight stands on a body, and a stepped foundation is one.
fn conflict(p: &impl Placed, shape: u8, a: &PieceAddr) -> bool {
    let riser = |f: u8| f == FOOT_RISER || f == FOOT_TRI_RISER;
    let foot = a.sock.foot;
    let mut hit = false;
    p.each(&mut |rshape, r| {
        if hit || r.level != a.level || r.bld != a.bld {
            return;
        }
        let rfoot = r.sock.foot;
        let steps = |s: u8| s == FOUNDATION_STEPS;
        let meets = crosses(&a.sock, &r.sock);
        hit = match (foot == FOOT_EDGE, rfoot == FOOT_EDGE) {
            (true, true) => meets,
            (true, false) => !riser(rfoot) && meets,
            (false, true) => !riser(foot) && meets,
            (false, false) => match (riser(foot), riser(rfoot)) {
                (true, true) => meets || a.sock.body() == r.sock.body(),
                (true, false) => steps(shape) && meets,
                (false, true) => steps(rshape) && meets,
                (false, false) => meets,
            },
        };
    });
    hit
}

/// Partial walls stack on one edge and may not run through each other: an edge at another level of
/// the same edge whose span meets this one's.
fn vertical_edge_clash(p: &impl Placed, shape: u8, a: &PieceAddr) -> bool {
    if !is_edge(shape) {
        return false;
    }
    let bottom = level_y(a.level);
    let top = bottom + wall_height(shape);
    (0..MAX_SOCKETS as u8).any(|other| {
        other != a.level
            && p.shape_at(&PieceAddr { level: other, ..*a }).is_some_and(|s| {
                let y = level_y(other);
                bottom < y + wall_height(s) && y < top
            })
    })
}

/// A point `i` cells along x and `j` along z from a building's origin (a corner of its lattice).
pub const fn cell(i: i32, j: i32) -> LPt {
    LPt::new(6 * i, 0, 0, 6 * j)
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::pieces::sock::Sock;
    use alloc::vec::Vec;

    #[derive(Default)]
    struct Building(Vec<(u8, PieceAddr)>);

    impl Lookup for Building {
        fn shape_at(&self, a: &PieceAddr) -> Option<u8> {
            self.0.iter().find(|(_, b)| b == a).map(|(s, _)| *s)
        }
    }

    impl Placed for Building {
        fn each(&self, f: &mut dyn FnMut(u8, &PieceAddr)) {
            for (s, a) in &self.0 {
                f(*s, a);
            }
        }
    }

    impl Building {
        fn place(&mut self, shape: u8, level: u8, sock: Sock) -> Result<(), Refuse> {
            let a = PieceAddr { bld: 0, level, sock };
            check(self, shape, &a)?;
            self.0.push((shape, a));
            Ok(())
        }
    }

    fn body(i: i32, j: i32) -> Sock {
        Sock::square_from(cell(i, j), 0)
    }

    #[test]
    fn a_room_goes_up_in_order_and_nothing_stands_on_air() {
        let mut b = Building::default();
        // A wall with nothing beside it, a floor with nothing under it: no.
        assert_eq!(b.place(WALL, 0, Sock::edge_from(cell(0, 0), 0)), Err(Refuse::Support));
        assert_eq!(b.place(FLOOR, 1, body(0, 0)), Err(Refuse::Support));
        // A floor on the ground, a foundation in the air: no.
        assert_eq!(b.place(FLOOR, 0, body(0, 0)), Err(Refuse::Level));
        assert_eq!(b.place(FOUNDATION, 1, body(0, 0)), Err(Refuse::Level));
        // Foundations, then walls round them, then a floor on the walls.
        for (i, j) in [(0, 0), (1, 0)] {
            b.place(FOUNDATION, 0, body(i, j)).unwrap();
        }
        assert_eq!(b.place(FOUNDATION, 0, body(0, 0)), Err(Refuse::Taken));
        for s in
            [Sock::edge_from(cell(0, 0), 0), Sock::edge_from(cell(1, 0), 0), Sock::edge_from(cell(0, 0), 3)]
        {
            b.place(WALL, 0, s).unwrap();
        }
        b.place(DOORWAY, 0, Sock::edge_from(cell(2, 0), 3)).unwrap();
        b.place(FLOOR, 1, body(0, 0)).unwrap();
        b.place(FLOOR, 1, body(1, 0)).unwrap();
        // A half wall under a full one on the same edge runs through it.
        assert_eq!(b.place(HALF_WALL, MAX_LEVELS as u8, Sock::edge_from(cell(0, 0), 0)), Err(Refuse::Taken));
        // A flight on a foundation, and not two on one.
        b.place(STAIRS, 0, Sock::riser(body(1, 0).at, 3)).unwrap();
        assert_eq!(b.place(STAIRS, 0, Sock::riser(body(1, 0).at, 0)), Err(Refuse::Taken));
        // Off the lattice, or the wrong footprint for the shape: no.
        let off = Sock::square_from(LPt::new(3, 0, 0, 0), 0);
        assert_eq!(b.place(FOUNDATION, 0, off), Err(Refuse::Footprint));
        assert_eq!(b.place(WALL, 0, body(2, 0)), Err(Refuse::Footprint));
        // Nothing tops the building's storeys.
        assert_eq!(
            b.place(WALL, (MAX_LEVELS - 1) as u8 + MAX_LEVELS as u8, Sock::edge_from(cell(0, 0), 0)),
            Err(Refuse::Level)
        );
    }
}
