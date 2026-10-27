//! The building pieces' shapes and storeys, from Gates (`AnthonE/Gates`, MIT, `sim-core/src/build.rs`):
//! a 3 m lattice of squares and triangles, 3 m storeys, and the 21 shapes a building is made of. Gates
//! grades them twig, wood, stone and metal; ours are the colony's panels and the base's, graded by
//! what's drawn on them, not by what they cost (`docs/RING.md`, "Buildings from pieces").

/// A footprint's side, m: one square's edge, one triangle's.
pub const CELL_M: f32 = 3.0;
/// A storey, m.
pub const LEVEL_H_M: f32 = 3.0;
/// Whole storeys a building has sockets for: codes 0..8. Codes 8..16 sit half a storey above the
/// whole storey they're 8 past.
pub const MAX_LEVELS: usize = 8;
pub const MAX_SOCKETS: usize = 16;

/// Shape codes, as Gates numbers them.
pub const FOUNDATION: u8 = 0;
pub const WALL: u8 = 1;
pub const DOORWAY: u8 = 2;
pub const FLOOR: u8 = 3;
pub const STAIRS: u8 = 4;
pub const ROOF: u8 = 5;
/// An opening that blocks a body but not a shot; [`FRAME`] blocks neither.
pub const WINDOW: u8 = 6;
pub const FRAME: u8 = 7;
pub const TRI_FOUNDATION: u8 = 8;
pub const TRI_FLOOR: u8 = 9;
pub const TRI_ROOF: u8 = 10;
/// A square floor socket with an open centre; its rim bears walls.
pub const FLOOR_FRAME: u8 = 11;
pub const HALF_WALL: u8 = 12;
pub const LOW_WALL: u8 = 13;
pub const FOUNDATION_STEPS: u8 = 14;
pub const RAMP: u8 = 15;
pub const STAIRS_L: u8 = 16;
pub const STAIRS_U: u8 = 17;
pub const STAIRS_SPIRAL: u8 = 18;
pub const STAIRS_TRI_SPIRAL: u8 = 19;
pub const TRI_FLOOR_FRAME: u8 = 20;
/// How many shapes there are.
pub const SHAPES: u8 = 21;

/// Where a socket's level puts its floor, m up from the building's: whole storeys keep their codes,
/// and a code [`MAX_LEVELS`] past one is half a storey above it.
pub fn level_y(level: u8) -> f32 {
    (level % MAX_LEVELS as u8) as f32 * LEVEL_H_M + (level / MAX_LEVELS as u8) as f32 * (LEVEL_H_M * 0.5)
}

/// `level` moved by `halves` half-storeys, or None outside the levels there are.
pub const fn level_step(level: u8, halves: i8) -> Option<u8> {
    let n = (level % MAX_LEVELS as u8) as i16 * 2 + (level / MAX_LEVELS as u8) as i16 + halves as i16;
    if level as usize >= MAX_SOCKETS || n < 0 || n >= MAX_SOCKETS as i16 {
        None
    } else {
        Some((n / 2) as u8 + (n % 2) as u8 * MAX_LEVELS as u8)
    }
}

/// How tall a wall of `shape` stands, m: low walls are cover, half walls bear a socket at their top.
pub fn wall_height(shape: u8) -> f32 {
    match shape {
        HALF_WALL => LEVEL_H_M * 0.5,
        LOW_WALL => LEVEL_H_M / 3.0,
        _ => LEVEL_H_M,
    }
}

/// Whether a shape stands on an edge (the walls' family) and has two faces.
pub const fn is_edge(shape: u8) -> bool {
    matches!(shape, WALL | HALF_WALL | LOW_WALL | DOORWAY | WINDOW | FRAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_storeys_step_between_whole_ones() {
        assert_eq!(level_step(0, 1), Some(8));
        assert_eq!(level_step(8, 1), Some(1));
        assert_eq!(level_step(1, -2), Some(0));
        assert_eq!(level_step(0, -1), None);
        assert_eq!(level_step(7, 1), Some(15));
        assert_eq!(level_step(15, 1), None);
        assert_eq!(level_y(8), 1.5);
        assert_eq!(level_y(3), 9.0);
        for level in 0..MAX_SOCKETS as u8 {
            let up = level_step(level, 1);
            if let Some(u) = up {
                assert!((level_y(u) - level_y(level) - LEVEL_H_M * 0.5).abs() < 1e-6, "{level}");
                assert_eq!(level_step(u, -1), Some(level));
            }
        }
    }
}
