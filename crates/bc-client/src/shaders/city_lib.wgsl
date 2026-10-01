// The city's grid for the shaders that paint it from afar (`bc_sim::colony::city`, with its
// numbers checked against these by `bc_client_core::city_atlas`'s tests). What each block holds
// comes from the block atlas, one texel a block; this paints inside a block what the rules would
// have built there: kerbs and pavements, roofs, parks, the canal, the site, the streets' lamps.
#define_import_path bc::city

#import bc::noise::{hash13, noise3}

const BLOCK: f32 = 128.0;
const GRID_X0: f32 = -16384.0;
const AVENUE: f32 = 80.0;
const MEDIAN: f32 = 16.0;
const ROWS: i32 = 12;
const BANK_ROW: i32 = 13;
const STREET: f32 = 24.0;
const WIDE_STREET: f32 = 40.0;
const SIDEWALK: f32 = 5.0;
const CANAL_ROW: i32 = 4;
const CANAL_WIDTH: f32 = 40.0;
const STRIP_WIDTH: f32 = 3351.0322;
const ATLAS_ROWS: i32 = 27;

// Where a point of a strip falls on the grid.
struct Cell {
    bx: i32,
    row: i32,
    // Its block's footprint (s0, s1, x0, x1), when there's a block there.
    rect: vec4<f32>,
    // The point: s across, x along.
    p: vec2<f32>,
};

fn cross_width(bx: i32) -> f32 {
    return select(STREET, WIDE_STREET, (bx & 3) == 0);
}

fn lane_width(k: i32) -> f32 {
    return select(STREET, WIDE_STREET, (k % 4) == 0);
}

fn row_edge(k: i32) -> f32 {
    return AVENUE * 0.5 + BLOCK * f32(k);
}

fn row_at(s: f32) -> i32 {
    let c = s - STRIP_WIDTH * 0.5;
    let a = abs(c);
    if (a < AVENUE * 0.5) {
        return 0;
    }
    let k = min(i32(floor((a - AVENUE * 0.5) / BLOCK)) + 1, BANK_ROW);
    return select(k, -k, c < 0.0);
}

fn block_rect(bx: i32, row: i32) -> vec4<f32> {
    let x0 = GRID_X0 + BLOCK * f32(bx) + cross_width(bx) * 0.5;
    let x1 = GRID_X0 + BLOCK * f32(bx + 1) - cross_width(bx + 1) * 0.5;
    let k = abs(row);
    let inner = row_edge(k - 1) + select(0.0, lane_width(k - 1) * 0.5, k > 1);
    let outer = row_edge(k) - lane_width(k) * 0.5;
    let mid = STRIP_WIDTH * 0.5;
    if (row < 0) {
        return vec4(mid - outer, mid - inner, x0, x1);
    }
    return vec4(mid + inner, mid + outer, x0, x1);
}

fn city_cell(s: f32, x: f32) -> Cell {
    let bx = i32(floor((x - GRID_X0) / BLOCK));
    let row = row_at(s);
    return Cell(bx, row, block_rect(bx, row), vec2(s, x));
}

// The block atlas's texel for a cell of strip `strip`.
fn atlas_texel(strip: i32, cell: Cell) -> vec2<i32> {
    return vec2(clamp(cell.bx, 0, 255), strip * ATLAS_ROWS + cell.row + BANK_ROW);
}

struct Paint {
    albedo: vec3<f32>,
    // Where lamps or lit windows would show at night, 0..1.
    lamps: f32,
    // How tall what's here stands, m (for shading from afar).
    height: f32,
};

// Distance inside a rectangle's edge (negative outside).
fn inside(r: vec4<f32>, p: vec2<f32>) -> f32 {
    return min(min(p.x - r.x, r.y - p.x), min(p.y - r.z, r.w - p.y));
}

// What a block looks like from afar, from its atlas texel (bytes as 0..1: kind, district, height
// in 2 m steps, seed).
fn city_paint(cell: Cell, t: vec4<f32>) -> Paint {
    let kind = i32(t.r * 255.0 + 0.5);
    let height = t.b * 255.0 * 2.0;
    let seed = t.a * 255.0;
    let p = cell.p;
    let asphalt = vec3(0.16, 0.16, 0.17);
    let paving = vec3(0.42, 0.40, 0.37);
    let grass = vec3(0.13, 0.24, 0.08);
    // The streets: asphalt, with the lamps along their edges.
    let d = inside(cell.rect, p);
    let has_block = abs(cell.row) >= 1 && abs(cell.row) <= ROWS && kind != 0;
    if (!has_block || d < 0.0) {
        let along = fract(p.y / 30.0);
        let lamp_line = smoothstep(1.5, 0.0, abs(-d - 2.5)) * step(0.85, along);
        if (cell.row == 0) {
            // The avenue: its tree-lined pavements, the roads, the tram's median.
            let a = abs(s_of_avenue(p.x));
            var col = asphalt;
            col = mix(col, vec3(0.2, 0.26, 0.14), step(MEDIAN * 0.5, a) * (1.0 - step(MEDIAN * 0.5 + 1.0, a)));
            col = mix(col, vec3(0.08, 0.16, 0.06) * (0.7 + 0.6 * noise3(vec3(p * 0.3, 1.0))), step(AVENUE * 0.5 - 18.0, a));
            return Paint(col, smoothstep(1.5, 0.0, abs(a - (AVENUE * 0.5 - 17.0))) * step(0.8, along), 0.0);
        }
        if (abs(cell.row) >= BANK_ROW) {
            // The window bank's park, and the promenade along the glass.
            let edge = min(p.x, STRIP_WIDTH - p.x);
            var col = grass * (0.7 + 0.6 * noise3(vec3(p * 0.08, 3.0)));
            col = mix(paving, col, smoothstep(10.0, 14.0, edge));
            return Paint(col, smoothstep(1.0, 0.0, abs(edge - 8.0)) * step(0.82, along), 0.0);
        }
        if (cell.bx < 8 || cell.bx > 249) {
            // Hub Gate's plaza and the far cap's foot.
            return Paint(paving * (0.9 + 0.1 * noise3(vec3(p * 0.2, 5.0))), step(0.9, fract(p.x / 40.0)) * step(0.9, along), 0.0);
        }
        return Paint(asphalt, lamp_line, 0.0);
    }
    // The pavement round the block.
    if (d < SIDEWALK && kind != 4) {
        return Paint(paving, 0.0, 0.0);
    }
    let q = (p - cell.rect.xz) / BLOCK;
    if (kind == 2) {
        // A park: trees in the grass, a path or two.
        let trees = smoothstep(0.45, 0.7, noise3(vec3(p * 0.12, seed)));
        var col = mix(grass, vec3(0.05, 0.11, 0.04), trees);
        let path = smoothstep(2.5, 0.5, abs(fract(q.x * 2.0 + seed * 0.1) - 0.5) * BLOCK * 0.5);
        col = mix(col, paving, path * 0.7);
        return Paint(col, path * step(0.9, fract(p.y / 25.0)) * 0.6, 6.0);
    }
    if (kind == 3) {
        // A plaza.
        return Paint(paving * (0.85 + 0.15 * hash13(vec3(floor(p / 6.0), seed))), step(0.92, hash13(vec3(floor(p / 9.0), seed))), 0.0);
    }
    if (kind == 4) {
        // The canal: quays either side of the water.
        let mid = (cell.rect.x + cell.rect.y) * 0.5;
        let water = step(abs(p.x - mid), CANAL_WIDTH * 0.5);
        let col = mix(paving * 0.9, vec3(0.03, 0.07, 0.08), water);
        let lamps = (1.0 - water) * smoothstep(1.0, 0.0, abs(abs(p.x - mid) - CANAL_WIDTH * 0.5 - 2.0)) * step(0.85, fract(p.y / 25.0));
        return Paint(col, lamps, 0.0);
    }
    if (kind == 5) {
        // The building site: bare ground, frames going up, lamps on the cranes.
        let soil = vec3(0.32, 0.25, 0.17) * (0.8 + 0.3 * noise3(vec3(p * 0.1, seed)));
        let frame = step(0.92, fract(q.x * 6.0)) + step(0.92, fract(q.y * 6.0));
        return Paint(mix(soil, vec3(0.45, 0.44, 0.42), min(frame, 1.0) * 0.6), step(0.97, hash13(vec3(floor(p / 12.0), seed))), height);
    }
    // Buildings: roofs over a grid of lots, lit windows by night.
    let lots = select(2.0, 3.0, hash13(vec3(seed, 1.0, 2.0)) > 0.5);
    let lot = floor(q * lots);
    let h = hash13(vec3(lot, seed));
    var roof = mix(vec3(0.34, 0.33, 0.32), vec3(0.5, 0.44, 0.38), h);
    roof = mix(roof, vec3(0.2, 0.2, 0.22), step(0.7, h));
    if (kind == 6) {
        roof = vec3(0.25, 0.42, 0.36);
    }
    if (kind == 7) {
        roof = vec3(0.12, 0.15, 0.2);
    }
    // Gaps between the lots' buildings.
    let gap = min(fract(q.x * lots), fract(q.y * lots));
    roof = mix(paving * 0.6, roof, smoothstep(0.0, 0.04, gap));
    let windows = step(0.5, hash13(vec3(floor(p * 0.4), seed))) * clamp(height / 40.0, 0.3, 1.0);
    return Paint(roof * (0.85 + 0.25 * noise3(vec3(p * 0.5, seed))), windows, height);
}

// Across from the avenue's centre line, for a point `s` of the strip.
fn s_of_avenue(s: f32) -> f32 {
    return s - STRIP_WIDTH * 0.5;
}
