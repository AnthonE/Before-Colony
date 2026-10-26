//! The city's map for the shaders: one RGBA8 texel a block (`bc_sim::colony::city::texel`), so a
//! shader that paints the city from afar (through the windows from outside, the far strips from
//! inside) paints the streets and blocks the walkers walk, without the layout's rules in WGSL.
//! The shaders' library (`bc-client/src/shaders/city_lib.wgsl`) keeps only the grid's numbers, which
//! a test here checks against the rules'.
//!
//! Laid out `ATLAS_W` blocks along by `ATLAS_ROWS` rows a strip: texel `(bx, strip·27 + row + 13)`
//! is block `bx`, row `row` (−13..=13: the banks, the rows of blocks, the avenue) of strip `strip`.
//!
//! Under the blocks, in the same image (one texture binding, which WebGL2 has few of), the heights
//! the long shadows are marched through (`COLONY_LOOK.md` pass 1.3, [`heights`]): the tallest roof
//! over each 16 m cell, four cells a texel.

use bc_sim::colony::city::{BANK_ROW, BLOCK, GRID_X0, MAX_HEIGHT, Rect, Stage, each_solid, texel};
use bc_sim::colony::frame::{STRIP_WIDTH, STRIPS};

/// Blocks along a strip (the grid's cells 0..256 span x from −16,384 to +16,384).
pub const ATLAS_W: u32 = 256;
/// Rows a strip: the banks and the avenue too.
pub const ATLAS_ROWS: u32 = 2 * BANK_ROW as u32 + 1;
pub const ATLAS_H: u32 = ATLAS_ROWS * STRIPS as u32;

/// The height map's cell, m: eight to a block, on the grid's lines.
pub const HEIGHT_CELL: f32 = 16.0;
/// Where the height map's first row starts across a strip (s, m): a little before the strip, so
/// that the avenue's edges, and every row of blocks', fall on its cells' edges.
pub const HEIGHT_S0: f32 = -12.483_92;
/// The height map's cells along a strip (the block grid's 32 km) and across it.
pub const HEIGHT_COLS: u32 = 2048;
pub const HEIGHT_ROWS: u32 = 211;
/// The atlas image: the blocks at its top left, the heights under them, four cells a texel.
pub const IMAGE_W: u32 = HEIGHT_COLS / 4;
pub const IMAGE_H: u32 = ATLAS_H + HEIGHT_ROWS * STRIPS as u32;

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

/// The tallest roof over each [`HEIGHT_CELL`] cell of every strip (m above the floor, one byte),
/// `HEIGHT_COLS` along by `HEIGHT_ROWS` a strip: cell `(c, strip·211 + j)` spans x from
/// `GRID_X0 + 16c` and s from `HEIGHT_S0 + 16j`. A solid counts where it covers at least half a
/// cell, so a mast, a crane's lattice or a frame's columns cast no shadow kilometres long, 16 m wide.
pub fn heights(stage: Stage) -> Vec<u8> {
    let (cols, rows) = (HEIGHT_COLS as usize, HEIGHT_ROWS as usize);
    let mut out = vec![0u8; cols * rows * STRIPS];
    let half = HEIGHT_CELL * HEIGHT_CELL * 0.5;
    let cell = |v: f32, from: f32| (v - from) / HEIGHT_CELL;
    for (strip, grid) in out.chunks_exact_mut(cols * rows).enumerate() {
        for bx in 0..ATLAS_W {
            let x0 = GRID_X0 + BLOCK * bx as f32;
            let area = Rect::new(0.0, STRIP_WIDTH, x0, x0 + BLOCK);
            each_solid(strip as u8, &area, stage, |b| {
                // Not the end caps' walls, nor what's underfoot.
                if b.h1 > MAX_HEIGHT || b.h1 < 1.0 {
                    return false;
                }
                let r = &b.rect;
                let (c0, c1) =
                    (cell(r.x0, GRID_X0).floor().max(0.0), cell(r.x1, GRID_X0).ceil().min(cols as f32));
                let (j0, j1) =
                    (cell(r.s0, HEIGHT_S0).floor().max(0.0), cell(r.s1, HEIGHT_S0).ceil().min(rows as f32));
                let top = b.h1.ceil() as u8;
                for j in j0 as usize..j1.max(j0) as usize {
                    let s = HEIGHT_S0 + HEIGHT_CELL * j as f32;
                    let across = (r.s1.min(s + HEIGHT_CELL) - r.s0.max(s)).max(0.0);
                    for c in c0 as usize..c1.max(c0) as usize {
                        let x = GRID_X0 + HEIGHT_CELL * c as f32;
                        let along = (r.x1.min(x + HEIGHT_CELL) - r.x0.max(x)).max(0.0);
                        if across * along >= half {
                            let h = &mut grid[j * cols + c];
                            *h = (*h).max(top);
                        }
                    }
                }
                false
            });
        }
    }
    out
}

/// The atlas image at the site's `stage`, `IMAGE_W × IMAGE_H` texels of four bytes: the block
/// map ([`block_atlas`]) at the top left, and the height map ([`heights`]) under it, its cells
/// `4t..4t + 3` along in texel `t`'s r, g, b and a.
pub fn atlas(stage: Stage) -> Vec<u8> {
    let w = IMAGE_W as usize;
    let mut out = vec![0u8; w * IMAGE_H as usize * 4];
    let blocks = block_atlas(stage);
    let row = ATLAS_W as usize * 4;
    for (y, src) in blocks.chunks_exact(row).enumerate() {
        out[y * w * 4..y * w * 4 + row].copy_from_slice(src);
    }
    let top = ATLAS_H as usize * w * 4;
    out[top..].copy_from_slice(&heights(stage));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{self, AVENUE, CANAL_ROW};
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
        assert_eq!(constant("KERB"), city::KERB);
        assert_eq!(constant("GROUND_FLOOR"), city::GROUND_FLOOR);
        assert_eq!(constant("FLOOR"), city::FLOOR);
        assert_eq!(constant("CANAL_ROW") as i32, CANAL_ROW);
        assert_eq!(constant("CANAL_WIDTH"), city::CANAL_WIDTH);
        assert!((constant("STRIP_WIDTH") - STRIP_WIDTH).abs() < 1e-3);
        assert_eq!(constant("ATLAS_ROWS") as u32, ATLAS_ROWS);
        assert_eq!(constant("TRACK_OFFSET"), bc_sim::colony::transit::TRACK_OFFSET);
        assert_eq!(constant("HUB_START") as i32, city::HUB_GATE.0);
        assert_eq!(constant("CITY_START") as i32, city::CITY.0);
        assert_eq!(constant("FOOT_START") as i32, city::FAR_FOOT.0);
        assert_eq!(constant("TERMINAL_FRONT"), -bc_sim::colony::frame::HALF_LENGTH + city::TERMINAL_DEPTH);
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

    /// What `city.wgsl` uses of the city's own module's constants (`city_lib.wgsl`, `bc::city`), it
    /// imports: in the detailed city's import, the sketched one's, or both, as where it's used is
    /// compiled (naga_oil finds a missing one only in the browser, as a shader that won't build).
    #[test]
    fn the_city_shader_imports_the_constants_it_uses() {
        const CITY: &str = include_str!("../../bc-client/src/shaders/city.wgsl");
        let ident = |l: &'static str| l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
        let consts = |src: &'static str| -> Vec<&'static str> {
            src.lines()
                .filter_map(|l| l.trim_start().strip_prefix("const "))
                .filter_map(|l| ident(l).next())
                .collect()
        };
        let (lib, own) = (consts(LIB), consts(CITY));
        // Each line's code, and whether CITY_DETAIL is on (Some(true)), off (Some(false)) or either.
        let mut stack: Vec<(bool, bool)> = Vec::new();
        let mut lines = Vec::new();
        let (mut detail, mut sketch) = ("", "");
        for l in CITY.lines() {
            let t = l.trim_start();
            if let Some(name) = t.strip_prefix("#ifdef ") {
                stack.push((name.trim() == "CITY_DETAIL", true));
            } else if t.starts_with("#else") {
                if let Some(top) = stack.last_mut() {
                    top.1 = !top.1;
                }
            } else if t.starts_with("#endif") {
                stack.pop();
            } else {
                let on = stack.iter().rev().find(|(city, _)| *city).map(|(_, on)| *on);
                if t.starts_with("#import bc::city::") {
                    match on {
                        Some(true) => detail = t,
                        Some(false) => sketch = t,
                        None => (detail, sketch) = (t, t),
                    }
                } else if !t.starts_with('#') {
                    lines.push((l.split("//").next().unwrap_or(""), on));
                }
            }
        }
        for c in lib.iter().filter(|c| !own.contains(c)) {
            for (l, on) in &lines {
                if !ident(l).any(|w| w == *c) {
                    continue;
                }
                let need: &[&str] = match on {
                    Some(true) => &[detail],
                    Some(false) => &[sketch],
                    None => &[detail, sketch],
                };
                for import in need {
                    assert!(ident(import).any(|w| w == *c), "city.wgsl uses {c} without importing it: {l}");
                }
            }
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
        assert_eq!(number("F_RADIUS"), bc_sim::colony::frame::RADIUS);
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

    /// The height map's cells sit on the grid: eight to a block along the strip, and across it
    /// every row's edges (the avenue's ± 40 m, then each block's 128 m) on a cell's edge.
    #[test]
    fn the_height_map_is_on_the_grid() {
        assert_eq!(BLOCK / HEIGHT_CELL, 8.0);
        assert_eq!(HEIGHT_COLS as f32 * HEIGHT_CELL, ATLAS_W as f32 * BLOCK);
        let edge = STRIP_WIDTH * 0.5 + AVENUE * 0.5 - HEIGHT_S0;
        assert!((edge / HEIGHT_CELL - (edge / HEIGHT_CELL).round()).abs() < 1e-4, "{edge}");
        const {
            assert!(HEIGHT_S0 <= 0.0 && HEIGHT_S0 > -HEIGHT_CELL);
            assert!(HEIGHT_S0 + HEIGHT_CELL * HEIGHT_ROWS as f32 >= STRIP_WIDTH);
            assert!(HEIGHT_S0 + HEIGHT_CELL * ((HEIGHT_ROWS - 1) as f32) < STRIP_WIDTH);
        }
        // The shader's copy of the numbers.
        assert_eq!(constant("HEIGHT_CELL"), HEIGHT_CELL);
        assert!((constant("HEIGHT_S0") - HEIGHT_S0).abs() < 1e-3);
        assert_eq!(constant("HEIGHT_COLS") as u32, HEIGHT_COLS);
        assert_eq!(constant("HEIGHT_ROWS") as u32, HEIGHT_ROWS);
        assert_eq!(constant("HEIGHT_Y0") as u32, ATLAS_H);
    }

    /// The heights are the city's: the Axis View's tower stands tall where it is, a park and the
    /// avenue are open, nothing's taller than the rules allow, and the image keeps the blocks.
    #[test]
    fn the_height_map_holds_the_roofs() {
        let image = atlas(Stage(0));
        assert_eq!(image.len(), (IMAGE_W * IMAGE_H * 4) as usize);
        let blocks = block_atlas(Stage(0));
        for y in 0..ATLAS_H as usize {
            let row = ATLAS_W as usize * 4;
            let at = y * IMAGE_W as usize * 4;
            assert_eq!(&image[at..at + row], &blocks[y * row..(y + 1) * row]);
        }
        let h = heights(Stage(0));
        assert!(h.iter().all(|&v| f32::from(v) <= MAX_HEIGHT));
        let (cols, rows) = (HEIGHT_COLS as usize, HEIGHT_ROWS as usize);
        let at = |strip: usize, s: f32, x: f32| {
            let j = ((s - HEIGHT_S0) / HEIGHT_CELL).floor() as usize;
            let c = ((x - GRID_X0) / HEIGHT_CELL).floor() as usize;
            h[strip * cols * rows + j * cols + c]
        };
        // The Axis View: block (30, −2) of strip 0, a tower (`texel`'s kind 7) with its height there.
        let r = city::block_rect(30, -2);
        let tallest = (0..8)
            .flat_map(|i| (0..8).map(move |k| (i, k)))
            .map(|(i, k)| at(0, r.s0 + 8.0 + 12.0 * i as f32, r.x0 + 8.0 + 12.0 * k as f32))
            .max()
            .unwrap();
        assert!(f32::from(tallest) >= 80.0, "the Axis View: {tallest}");
        // The avenue down the middle of every strip, away from Hub Gate.
        for strip in 0..3 {
            assert_eq!(at(strip, STRIP_WIDTH * 0.5, 0.0), 0);
        }
        // And a good share of the city under roofs.
        let roofed = h.iter().filter(|&&v| v > 4).count() as f32 / h.len() as f32;
        assert!(roofed > 0.15 && roofed < 0.7, "{roofed}");
    }
}
