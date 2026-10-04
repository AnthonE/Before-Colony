//! The city's map for the shaders: one RGBA8 texel a block (`bc_sim::colony::city::texel`), so a
//! shader that paints the city from afar (through the windows from outside, the far strips from
//! inside) paints the streets and blocks the walkers walk, without the layout's rules in WGSL.
//! The shaders' library (`bc-client/src/shaders/city_lib.wgsl`) keeps only the grid's numbers, which
//! a test here checks against the rules'.
//!
//! Laid out `ATLAS_W` blocks along by `ATLAS_ROWS` rows a strip: texel `(bx, strip·27 + row + 13)`
//! is block `bx`, row `row` (−13..=13: the banks, the rows of blocks, the avenue) of strip `strip`.

use bc_sim::colony::city::{BANK_ROW, Stage, texel};
use bc_sim::colony::frame::STRIPS;

/// Blocks along a strip (the grid's cells 0..256 span x from −16,384 to +16,384).
pub const ATLAS_W: u32 = 256;
/// Rows a strip: the banks and the avenue too.
pub const ATLAS_ROWS: u32 = 2 * BANK_ROW as u32 + 1;
pub const ATLAS_H: u32 = ATLAS_ROWS * STRIPS as u32;

/// The map at the site's `stage`, row by row: `ATLAS_W × ATLAS_H` texels of four bytes.
pub fn block_atlas(stage: Stage) -> Vec<u8> {
    let mut out = Vec::with_capacity((ATLAS_W * ATLAS_H * 4) as usize);
    for strip in 0..STRIPS as u8 {
        for r in 0..ATLAS_ROWS as i32 {
            let row = r - BANK_ROW;
            for bx in 0..ATLAS_W as i32 {
                out.extend_from_slice(&texel(strip, bx, row, stage));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{self, CANAL_ROW};
    use bc_sim::colony::frame::STRIP_WIDTH;

    const LIB: &str = include_str!("../../bc-client/src/shaders/city_lib.wgsl");
    const FACADE: &str = include_str!("../../bc-client/src/shaders/city_facade.wgsl");

    fn constant(name: &str) -> f32 {
        let line = LIB
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("const {name}:")))
            .unwrap_or_else(|| panic!("no {name} in city_lib.wgsl"));
        let v = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim();
        v.parse().unwrap_or_else(|_| panic!("{name} = {v}"))
    }

    #[test]
    fn the_shaders_grid_is_the_rules_grid() {
        assert_eq!(constant("BLOCK"), city::BLOCK);
        assert_eq!(constant("GRID_X0"), city::GRID_X0);
        assert_eq!(constant("AVENUE"), city::AVENUE);
        assert_eq!(constant("MEDIAN"), city::MEDIAN);
        assert_eq!(constant("ROWS") as i32, city::ROWS);
        assert_eq!(constant("BANK_ROW") as i32, BANK_ROW);
        assert_eq!(constant("STREET"), city::STREET);
        assert_eq!(constant("WIDE_STREET"), city::WIDE_STREET);
        assert_eq!(constant("SIDEWALK"), city::SIDEWALK);
        assert_eq!(constant("CANAL_ROW") as i32, CANAL_ROW);
        assert_eq!(constant("CANAL_WIDTH"), city::CANAL_WIDTH);
        assert!((constant("STRIP_WIDTH") - STRIP_WIDTH).abs() < 1e-3);
        assert_eq!(constant("ATLAS_ROWS") as u32, ATLAS_ROWS);
        assert_eq!(constant("TRACK_OFFSET"), bc_sim::colony::transit::TRACK_OFFSET);
        assert_eq!(constant("HUB_START") as i32, city::HUB_GATE.0);
        assert_eq!(constant("CITY_START") as i32, city::CITY.0);
        assert_eq!(constant("FOOT_START") as i32, city::FAR_FOOT.0);
        assert_eq!(constant("TERMINAL_FRONT"), -bc_sim::world::COLONY_HALF_LENGTH + city::TERMINAL_DEPTH);
    }

    /// The street's furniture stands where the ground's paint puts its pools and pits, from the
    /// same numbers (`bc_sim::colony::furniture`, `city_lib.wgsl`), and `city_mesh`'s park trees keep
    /// off the paths the paint draws. Both sides count a row's lamps as
    /// `max(1, floor(len / LAMP_GAP + 0.5))` gaps and space them evenly along it, from corner to
    /// corner (the lanes, the avenue, the quays) or between them (the cross streets, a park's loop):
    /// those numbers and that one formula are the contract (the furniture's own tests check its
    /// spacing).
    #[test]
    fn the_furniture_stands_where_the_ground_is_painted_for_it() {
        use crate::city_mesh::{PARK_BEDS, PARK_DIAG, PARK_PATH, PARK_PLAZA};
        use bc_sim::colony::furniture as f;
        for (name, rules) in [
            ("LAMP_GAP", f::LAMP_GAP),
            ("LAMP_OUT", f::LAMP_OUT),
            ("AVENUE_LAMP", f::AVENUE_LAMP),
            ("ROAD_OUT", f::ROAD_OUT),
            ("PARK_LOOP", f::PARK_LOOP),
            ("PARK_LAMP", f::PARK_LAMP),
            ("PLAZA_RING", f::PLAZA_RING),
            ("QUAY_LAMP", f::QUAY_LAMP),
            ("QUAY_TREE", f::QUAY_TREE),
            ("AVENUE_TREE", f::AVENUE_TREE),
            ("TREE_PITCH", f::TREE_PITCH),
            ("TREE_FIRST", f::TREE_FIRST),
            ("TREE_END", f::TREE_END),
            ("PARK_PATH", PARK_PATH),
            ("PARK_DIAG", PARK_DIAG),
            ("PARK_PLAZA", PARK_PLAZA),
            ("PARK_BEDS", PARK_BEDS),
        ] {
            assert_eq!(constant(name), rules, "{name}");
        }
        assert_eq!(constant("PLAZA_LAMPS") as usize, f::PLAZA_LAMPS);
        // The count, down a block's sides (88 to 104 m), a park's loop (74 to 82 m) and shorter.
        for len in [88.0f32, 96.0, 104.0, 74.0, 82.0, 44.0, 30.0, 10.0] {
            let gaps = (len / constant("LAMP_GAP") + 0.5).floor().max(1.0);
            assert_eq!(f::lamp_count(len), gaps as u32, "{len}");
        }
    }

    /// `city_lib.wgsl`: the heavy parts are each called from one place (the GPU's compiler inlines
    /// every call). The lamps' light is worked out in `lamps_near` alone, for the ground's paint; a
    /// lantern asks `lantern_burn`, which repeats only the rows' arithmetic.
    #[test]
    fn the_lamps_light_is_worked_out_in_one_place() {
        const CITY: &str = include_str!("../../bc-client/src/shaders/city.wgsl");
        let calls = |name: &str| {
            let call = format!("{name}(");
            let def = format!("fn {name}(");
            [LIB, FACADE, CITY]
                .iter()
                .flat_map(|src| src.lines())
                .map(|l| l.split("//").next().unwrap_or(""))
                .filter(|l| l.contains(&call) && !l.contains(&def))
                .count()
        };
        assert_eq!(calls("lamp_row"), 1, "lamp_row");
        assert_eq!(calls("lamps_near"), 1, "lamps_near");
        assert_eq!(calls("lantern_burn"), 1, "lantern_burn");
    }

    /// `CLAUDE.md`: in `city.wgsl` every derivative is taken at the top of `fragment()`, before the
    /// surface branch (WebGPU rejects them under it; naga doesn't catch it). Nothing it imports from
    /// the city's own modules may take one.
    #[test]
    fn the_city_shaders_take_their_derivatives_at_the_top() {
        const CITY: &str = include_str!("../../bc-client/src/shaders/city.wgsl");
        let derivative = |l: &str| ["dpdx", "dpdy", "fwidth"].iter().any(|d| l.contains(d));
        let code = |src: &'static str| src.lines().map(|l| l.split("//").next().unwrap_or(""));
        for (name, src) in [("city_lib", LIB), ("city_facade", FACADE)] {
            for l in code(src) {
                assert!(!derivative(l), "{name}: {l}");
            }
        }
        let lines: Vec<&str> = code(CITY).collect();
        let first_branch = lines.iter().position(|l| l.trim_start().starts_with("if (surface")).unwrap();
        for (k, l) in lines.iter().enumerate() {
            assert!(
                !derivative(l) || k < first_branch,
                "city.wgsl line {}: a derivative under the branch",
                k + 1
            );
        }
    }

    #[test]
    fn the_facades_numbers_are_the_rules() {
        let number = |name: &str| -> f32 {
            let line = FACADE
                .lines()
                .find(|l| l.trim_start().starts_with(&format!("const {name}:")))
                .unwrap_or_else(|| panic!("no {name} in city_facade.wgsl"));
            let v = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim();
            v.parse().unwrap_or_else(|_| panic!("{name} = {v}"))
        };
        assert_eq!(number("F_GROUND"), city::GROUND_FLOOR);
        assert_eq!(number("F_STOREY"), city::FLOOR);
        assert_eq!(number("F_KERB"), city::KERB);
        assert_eq!(number("F_MAX_HEIGHT"), city::MAX_HEIGHT);
        assert_eq!(number("F_BLOCK"), city::BLOCK);
        assert_eq!(number("F_RADIUS"), bc_sim::world::COLONY_RADIUS);
        assert_eq!(number("F_FIRST_WINDOW"), bc_sim::colony::frame::FIRST_WINDOW);
    }

    #[test]
    fn the_atlas_holds_every_block_where_the_shader_looks() {
        let atlas = block_atlas(Stage(0));
        assert_eq!(atlas.len(), (ATLAS_W * ATLAS_H * 4) as usize);
        let at = |strip: u32, bx: u32, row: i32| {
            let y = strip * ATLAS_ROWS + (row + BANK_ROW) as u32;
            let i = ((y * ATLAS_W + bx) * 4) as usize;
            [atlas[i], atlas[i + 1], atlas[i + 2], atlas[i + 3]]
        };
        for strip in 0..3u32 {
            for (bx, row) in [(30u32, -2), (77, 5), (150, -11), (210, 3)] {
                assert_eq!(at(strip, bx, row), texel(strip as u8, bx as i32, row, Stage(0)));
            }
        }
        // The Axis View tower, a park and the canal show as themselves.
        assert_eq!(at(0, 30, -2)[0], 7);
        assert_eq!(at(1, 50, CANAL_ROW)[0], 4);
        assert_eq!(at(0, 5, 0), [0; 4], "Hub Gate's plaza");
    }
}
