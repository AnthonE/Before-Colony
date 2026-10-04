// The city's grid for the shaders that paint it from afar (`bc_sim::colony::city`, with its
// numbers checked against these by `bc_client_core::city_atlas`'s tests). What each block holds
// comes from the block atlas, one texel a block; this paints inside a block what the rules would
// have built there: kerbs and pavements, roofs, parks, the canal, the site, the streets with their
// paint and their wear, and by night the light their lamps throw on them.
//
// Everything here is filtered by the footprint its caller passes: how many metres a pixel covers
// across the strip (s) and along it (x), from `fwidth` taken in uniform control flow. Stripes,
// joints and markings narrower than a pixel fade to their average, noise detail fades out (and is
// skipped once it has), and the lamps' pools widen and soften as the pixel grows while each keeps
// the light it throws, so a street seen from a kilometre off is a line of light, not a row of dots.
//
// The three layers of `docs/COLONY_LOOK.md`: the streets and yards are used (tyre tracks, oil,
// patches, cracks, dust in the gutters, puddles where it dips), dirtier where the city works; the
// colony's own ground (Hub Gate's square, the tram's median, the banks' promenades) is pale stone
// with the strip's line colour inlaid, glowing by night; and each strip has its own materials:
// Charter granite, Canal brick and setts, Gardens buff stone and timber.
//
// Two ways in. `city_paint` is the ground in full, for Medium and up: a point looks its noise up
// once, at three scales (`Grain`), and each heavy part (a road, a row of lamps, a run of flags) is
// called from one place, since the compiler inlines every call. `city_sketch` is its colours and
// main lines and its lamps' average light, small, for the Low tier and for the city seen from
// outside: a software rasteriser (SwiftShader, which the Low tier is for and CI runs on) runs every
// branch of a shader for every pixel, so there a pixel costs the whole shader's size.
#define_import_path bc::city

#import bc::noise::hash13

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
// The tram's tracks: each one's middle from the avenue's.
const TRACK_OFFSET: f32 = 4.5;
const STRIP_WIDTH: f32 = 3351.0322;
const ATLAS_ROWS: i32 = 27;

// Where the stretches along the axis begin, by block (`colony::city`'s `HUB_GATE.0`, `CITY.0` and
// `FAR_FOOT.0`): Hub Gate's square is the cells with no block between the cap and `CITY_START`
// (and the avenue there), the far cap's foot from `FOOT_START`. Hub Gate's terminal's front, x
// (`-COLONY_HALF_LENGTH + TERMINAL_DEPTH`): its square's rings centre on its door.
const HUB_START: i32 = 3;
const CITY_START: i32 = 8;
const FOOT_START: i32 = 250;
const TERMINAL_FRONT: f32 = -15940.0;
// The avenue's carriageways, across from its middle line: from the median's kerb (MEDIAN / 2) to
// the pavements' (18 m of pavement beyond, to AVENUE / 2).
const ROAD_OUT: f32 = 22.0;
// The streets' lamps, where their posts stand (`bc_sim::colony::furniture`): along every kerb, about
// `LAMP_GAP` apart and evenly from corner to corner of each block's side (the corners' lamps are the
// streets along x's), each pool's middle `LAMP_OUT` out over the street from its kerb. A pool is a
// core `LAMP_CORE` wide (σ, m) in a soft halo; seen from afar each widens `LAMP_SPREAD` times faster
// than the pixel along its row (the eye's glare: past a few hundred metres a street's lamps run
// together into a line).
// The city's lamps burn LAMP_MEAN on average (some dim, one in sixteen out).
const LAMP_GAP: f32 = 30.0;
const LAMP_OUT: f32 = 1.5;
const LAMP_CORE: f32 = 5.0;
const LAMP_HALO: f32 = 10.0;
const LAMP_HALO_GAIN: f32 = 0.07;
const LAMP_SPREAD: f32 = 2.5;
const LAMP_MEAN: f32 = 0.88;
// The avenue's lamps: across from its middle line (its pavements' kerb side); the median's are the
// colony's (no posts yet).
const AVENUE_LAMP: f32 = 22.7;
// The street furniture's numbers (`bc_sim::colony::furniture`, checked by `city_atlas`'s tests): a
// park's lamps beside the loop of its path (PARK_LOOP in from its pavement, PARK_LAMP inside it,
// between its corners), a plaza's ring of lamps and their gain (they're the colony's: all burn), a
// quay's lamps and its row of trees back from the water, the avenue's trees out from its middle line;
// their pits TREE_PITCH apart from TREE_FIRST, none within TREE_END of a cross street (where the
// crossings land). And the park's paths, which `city_mesh`'s trees keep off: the loop's half-width,
// the diagonals', the round plaza, its beds' outer edge.
const PARK_LOOP: f32 = 6.0;
const PARK_LAMP: f32 = 2.4;
const PARK_PATH: f32 = 1.5;
const PARK_DIAG: f32 = 1.2;
const PARK_PLAZA: f32 = 11.0;
const PARK_BEDS: f32 = 14.0;
const PLAZA_RING: f32 = 24.0;
const PLAZA_LAMPS: f32 = 8.0;
const PLAZA_GAIN: f32 = 0.8;
const QUAY_LAMP: f32 = 2.2;
const QUAY_TREE: f32 = 11.0;
const AVENUE_TREE: f32 = 24.3;
const TREE_PITCH: f32 = 8.0;
const TREE_FIRST: f32 = 4.0;
const TREE_END: f32 = 5.0;
// Which side the traffic keeps to (stop lines go on the half that drives into a crossing): +1 the
// right, −1 the left. Traffic (`COLONY_LOOK.md` pass 4.1) must keep to the same.
const KEEP_RIGHT: f32 = 1.0;
const SQRT_TAU: f32 = 2.5066283;
const PI: f32 = 3.14159265;

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
    // By night. On the ground (`from_afar` false): the lamps' light falling here, relative to the
    // peak under one (the caller multiplies by the albedo and its lamps' luminance). From afar:
    // where lamps or lit windows show, already weighted by what they light (an emissive mask).
    lamps: f32,
    // How tall what's here stands, m (for shading from afar).
    height: f32,
    roughness: f32,
    // Where water stands when it's wet, 0..1: puddles in dips, ruts, gutters and joints (the canal's
    // water is 1). The weather (`COLONY_LOOK.md` pass 3.2) multiplies it by how wet it is.
    wet: f32,
    // Self-lit inlays: the colony's light strips in the line colour, linear colour × strength. The
    // caller scales it to nits.
    glow: vec3<f32>,
};

// Distance inside a rectangle's edge (negative outside).
fn inside(r: vec4<f32>, p: vec2<f32>) -> f32 {
    return min(min(p.x - r.x, r.y - p.x), min(p.y - r.z, r.w - p.y));
}

// ---- Filtering and noise -------------------------------------------------------------------------

fn mk(albedo: vec3<f32>, rough: f32) -> Paint {
    return Paint(albedo, 0.0, 0.0, rough, 0.0, vec3(0.0));
}

// How much of a pixel `w` wide at `x` the band |x| < hw covers (a box filter: a band narrower than
// the pixel fades to its share of it).
fn band(x: f32, hw: f32, w: f32) -> f32 {
    let f = max(w, 1e-3);
    return clamp((min(x + 0.5 * f, hw) - max(x - 0.5 * f, -hw)) / f, 0.0, 1.0);
}

// The same for a < x < b.
fn span(x: f32, a: f32, b: f32, w: f32) -> f32 {
    return band(x - 0.5 * (a + b), 0.5 * (b - a), w);
}

// How much of the pixel lies past `edge` (a step, filtered).
fn past(x: f32, edge: f32, w: f32) -> f32 {
    return clamp((x - edge) / max(w, 1e-3) + 0.5, 0.0, 1.0);
}

// Stripes: the part [0, on) of every `period` from x = 0, box-filtered by a pixel `w` wide. `x`
// should be local (tens or hundreds of metres), never a colony coordinate.
fn stripes(x: f32, period: f32, on: f32, w: f32) -> f32 {
    let f = max(w, 1e-3);
    let a = x - 0.5 * f;
    let b = x + 0.5 * f;
    let fa = floor(a / period);
    let fb = floor(b / period);
    let ia = fa * on + min(a - fa * period, on);
    let ib = fb * on + min(b - fb * period, on);
    return clamp((ib - ia) / f, 0.0, 1.0);
}

// Lines `width` wide centred on the multiples of `period`.
fn lines(x: f32, period: f32, width: f32, w: f32) -> f32 {
    return stripes(x + 0.5 * width, period, width, w);
}

// The joints of a grid of slabs `size` along u.x and u.y, `width` wide.
fn joints(u: vec2<f32>, size: vec2<f32>, width: f32, fp: vec2<f32>) -> f32 {
    let a = lines(u.x, size.x, width, fp.x);
    let b = lines(u.y, size.y, width, fp.y);
    return 1.0 - (1.0 - a) * (1.0 - b);
}

// 1 while detail `size` m across is resolved, fading to 0 as the pixel grows past it (0 from
// 1.5 × size on, where it's skipped).
fn fade(w: f32, size: f32) -> f32 {
    return 1.0 - smoothstep(0.35 * size, 1.5 * size, w);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

fn hash12(p: vec2<f32>) -> f32 {
    var q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// Smooth value noise on the plane, 0..1 (four hashes: the ground is flat, so half `noise3`'s cost).
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash12(i), hash12(i + vec2(1.0, 0.0)), u.x);
    let b = mix(hash12(i + vec2(0.0, 1.0)), hash12(i + vec2(1.0, 1.0)), u.x);
    return mix(a, b, u.y);
}

// x along the strip from the last grid line on a multiple of `period` (a whole number of metres, a
// multiple of BLOCK or of every pattern's period it's used for), without the colony coordinate's
// magnitude: `p.y - GRID_X0` itself rounds to 4 mm at the far cap, and a box filter's `x ± w / 2`
// with it, so stripes seen through a pixel of a few mm covered the wrong share of it, or none at all
// (joints and gaps vanished at your feet). The line's x (a whole number, exact) is taken off `p.y`
// instead, which keeps `p.y`'s own millimetre.
fn along(px: f32, period: f32) -> f32 {
    return px - (GRID_X0 + period * floor((px - GRID_X0) / period));
}

// Noise at `f` per metre over the strip. The colony is 32 km long, and noise of a colony coordinate
// times a frequency loses its precision, so x is wrapped first (at every 32nd grid line: the seams
// fall in wide streets).
fn nz(p: vec2<f32>, f: f32, z: f32) -> f32 {
    return vnoise(vec2(p.x, along(p.y, 4096.0)) * f + vec2(z * 17.31, z * 7.97));
}

// The angle of `q` from +x (its second component) towards its first, safe at the origin (atan2(0, 0)
// is undefined in WGSL and GLSL, and a NaN there would spread through the bloom).
fn heading(q: vec2<f32>) -> f32 {
    return atan2(q.x, select(q.y, 1e-6, abs(q.x) + abs(q.y) < 1e-6));
}

// A point's noise at three scales, looked up once and shared by everything there: about 16 m, 2 m
// and 0.4 m across (the last fading to 0.5 once a pixel's past it, and not looked up then).
struct Grain {
    lo: f32,
    mid: f32,
    hi: f32,
};

fn grain_at(p: vec2<f32>, fi: f32) -> Grain {
    var hi = 0.5;
    if (fi < 0.55) {
        hi = mix(0.5, nz(p, 2.7, 3.0), fade(fi, 0.37));
    }
    return Grain(nz(p, 0.06, 1.0), nz(p, 0.45, 2.0), hi);
}

// Flags (slabs, bricks or setts) `size` along u.x and across u.y in a running bond, each a shade of
// its own, joints `jw` wide. `fp`: the footprint along u.x and u.y. `id` tells patterns apart.
fn flags(u: vec2<f32>, size: vec2<f32>, base: vec3<f32>, jw: f32, fp: vec2<f32>, id: f32) -> vec3<f32> {
    let row = floor(u.y / size.y);
    let x = u.x + 0.5 * size.x * (row - 2.0 * floor(row * 0.5));
    let jr = lines(u.y, size.y, jw, fp.y);
    // Once the rows run together in a pixel their joints' offsets do too: their average.
    let jc = mix(lines(x, size.x, jw, fp.x), jw / size.x, smoothstep(0.5 * size.y, size.y, fp.y));
    let j = 1.0 - (1.0 - jr) * (1.0 - jc);
    let h = mix(0.5, hash13(vec3(floor(x / size.x), row, id)), fade(max(fp.x, fp.y), max(size.x, size.y)));
    return base * (0.9 + 0.2 * h) * (1.0 - 0.35 * j);
}

// A kerb stone's top: pale granite, a joint every metre.
fn kerb_stone(u: f32, fu: f32, n: f32) -> vec3<f32> {
    return vec3(0.4, 0.395, 0.38) * (0.88 + 0.2 * n) * (1.0 - 0.4 * lines(u, 1.0, 0.02, fu));
}

// The strips' line colours (the trams' and the colony's own signs): Charter blue, Canal teal,
// Gardens green.
fn line_colour(strip: i32) -> vec3<f32> {
    if (strip == 1) {
        return vec3(0.05, 0.5, 0.52);
    }
    if (strip == 2) {
        return vec3(0.2, 0.55, 0.22);
    }
    return vec3(0.12, 0.32, 0.72);
}

// How worn the ground is, 0..1, by district (the texel's second byte: 0 none, else
// `DistrictKind` + 1) and strip: Charter cleanest, Canal the working town.
fn wear_of(district: i32, strip: i32) -> f32 {
    var w = 0.7;
    switch district {
        case 1: { w = 0.2; }
        case 2: { w = 0.25; }
        case 3: { w = 0.55; }
        case 4: { w = 0.45; }
        case 5: { w = 0.6; }
        case 6: { w = 0.35; }
        case 7: { w = 0.95; }
        case 8: { w = 0.3; }
        case 9: { w = 0.9; }
        default: { w = 0.7; }
    }
    let s = select(select(1.0, 1.2, strip == 1), 0.8, strip == 0);
    return clamp(w * s, 0.0, 1.0);
}

// Grass in drifts, lusher and drier, its tufts close up.
fn grass(g: Grain) -> vec3<f32> {
    let c = mix(vec3(0.06, 0.13, 0.035), vec3(0.15, 0.17, 0.07), smoothstep(0.3, 0.75, 1.0 - g.lo));
    return c * (0.8 + 0.4 * g.hi);
}

// Darker under the trees: blotches about 10 m across.
fn tree_shade(g: Grain) -> f32 {
    return smoothstep(0.58, 0.74, 0.65 * g.lo + 0.35 * g.mid);
}

// ---- Lamps --------------------------------------------------------------------------------------

// How bright the city's lamp `i` of row `id` burns: each its own, and one in sixteen out (nobody's
// changed it). The colony's own lamps (the square's, the banks') all burn.
fn lamp_burns(i: f32, id: f32) -> f32 {
    let h = hash13(vec3(i, id, 17.0));
    return select(0.7 + 0.45 * h, 0.0, h < 0.0625);
}

// A row's ends, softened by `s`: 1 between lo and hi.
fn ends(u: f32, lo: f32, hi: f32, s: f32) -> f32 {
    return clamp((u - lo) / (2.0 * s) + 0.5, 0.0, 1.0) * clamp((hi - u) / (2.0 * s) + 0.5, 0.0, 1.0);
}

// Along a row of lamps `gap` apart (lamp i at u = i·gap for i0 ≤ i ≤ i1): the light of pools `s0`
// wide (σ) through a pixel `fu` long, each keeping its integral (s0·√τ) as it widens. Near, the two
// lamps either side (each as bright as it burns); far, their average along the row.
fn lamps_along(u: f32, gap: f32, i0: f32, i1: f32, s0: f32, fu: f32, id: f32) -> f32 {
    let su = sqrt(s0 * s0 + LAMP_SPREAD * LAMP_SPREAD * fu * fu);
    let far = smoothstep(0.3, 0.55, su / gap);
    var near = 0.0;
    if (far < 1.0) {
        let w = u / gap;
        let a = floor(w);
        let da = (w - a) * gap;
        let db = gap - da;
        let k = -0.5 / (su * su);
        near = step(i0, a) * step(a, i1) * lamp_burns(a, id) * exp(k * da * da)
            + step(i0, a + 1.0) * step(a + 1.0, i1) * lamp_burns(a + 1.0, id) * exp(k * db * db);
    }
    let avg = LAMP_MEAN * su * SQRT_TAU / gap * ends(u, (i0 - 0.5) * gap, (i1 + 0.5) * gap, su);
    return s0 / su * mix(near, avg, far);
}

// The same along an endless row of the colony's own lamps, `u` wrapped by the caller into [0, gap).
fn lamps_periodic(u: f32, gap: f32, s0: f32, fu: f32) -> f32 {
    let su = sqrt(s0 * s0 + LAMP_SPREAD * LAMP_SPREAD * fu * fu);
    let k = -0.5 / (su * su);
    let v = gap - u;
    let near = exp(k * u * u) + exp(k * v * v);
    return s0 / su * mix(near, su * SQRT_TAU / gap, smoothstep(0.3, 0.55, su / gap));
}

// Across a row of lamps, `v` from its line: a pool `s0` wide through a pixel `fv` wide.
fn lamps_across(v: f32, s0: f32, fv: f32) -> f32 {
    let sv = sqrt(s0 * s0 + 0.5 * fv * fv);
    return s0 / sv * exp(-0.5 * v * v / (sv * sv));
}

// A row of lamps along a kerb from u = 0 to `len`, `v` across from the line of its pools' middles:
// spaced evenly, about LAMP_GAP apart, from corner to corner (`corners`) or between them. `id`
// names the row (the same from either side of its street) for its lamps' brightness.
fn lamp_row(u: f32, v: f32, len: f32, corners: bool, fu: f32, fv: f32, id: f32) -> f32 {
    let n = max(1.0, floor(len / LAMP_GAP + 0.5));
    let gap = len / n;
    let i0 = select(1.0, 0.0, corners);
    let i1 = select(n - 1.0, n, corners);
    let halo_across = lamps_across(v, LAMP_HALO, fv);
    if (i1 < i0 || halo_across < 0.002) {
        return 0.0;
    }
    let sh = sqrt(LAMP_HALO * LAMP_HALO + LAMP_SPREAD * LAMP_SPREAD * fu * fu);
    var l = LAMP_HALO_GAIN * halo_across * LAMP_MEAN * LAMP_HALO * SQRT_TAU / gap
        * ends(u, (i0 - 0.5) * gap, (i1 + 0.5) * gap, sh);
    let core_across = lamps_across(v, LAMP_CORE, fv);
    if (core_across > 0.002) {
        l += core_across * lamps_along(u, gap, i0, i1, LAMP_CORE, fu, id);
    }
    return l;
}

// A few floodlights at random in a block (the site's work lights): one in about three of its 32 m
// squares, 6 m wide (σ); `q` from the block's corner.
fn floods(q: vec2<f32>, seed: f32, fp: vec2<f32>) -> f32 {
    let s0 = 6.0;
    let sx = sqrt(s0 * s0 + LAMP_SPREAD * LAMP_SPREAD * fp.x * fp.x);
    let sy = sqrt(s0 * s0 + LAMP_SPREAD * LAMP_SPREAD * fp.y * fp.y);
    let base = floor(q / 32.0 - 0.5);
    var l = 0.0;
    for (var i = 0; i < 2; i++) {
        for (var j = 0; j < 2; j++) {
            let c = base + vec2(f32(i), f32(j));
            let h = hash13(vec3(c, seed + 6.0));
            let at = c * 32.0 + 10.0 + 12.0 * vec2(fract(h * 7.31), fract(h * 3.77));
            let d = q - at;
            l += step(h, 0.3) * exp(-0.5 * (d.x * d.x / (sx * sx) + d.y * d.y / (sy * sy)));
        }
    }
    let avg = 0.3 * 6.2831853 * s0 * s0 / 1024.0;
    return mix(l * s0 * s0 / (sx * sy), avg, smoothstep(0.3, 0.6, max(sx, sy) / 32.0));
}

// ---- The streets --------------------------------------------------------------------------------
//
// The heavy parts (a road, a row of lamps, a run of flags) are each called from one place: the
// GPU's compiler inlines every call, and a shader this size slows down with every copy.

// The streets round a point: the nearest cross street (along s, on a grid line) and the nearest
// street along x (on a row's edge; the bank road is the half of row 12's that's in row 12), and the
// sides of the block whose cell it is, for the lamps along them.
struct Streets {
    // x from the cross street's middle line; its half-width; which grid line it's on.
    dx: f32,
    hx: f32,
    gi: i32,
    // s from the street along x's middle line (positive away from the avenue); its half-width;
    // which row's outer edge it runs on (0: the nearest is the avenue).
    ds: f32,
    hs: f32,
    k: i32,
    // The block's sides: x0, x1 along (between the cross streets), s0, s1 across.
    seg_x: vec2<f32>,
    seg_s: vec2<f32>,
};

fn streets_at(cell: Cell) -> Streets {
    let p = cell.p;
    let i = i32(floor((p.y - GRID_X0) / BLOCK + 0.5));
    let gx = GRID_X0 + BLOCK * f32(i);
    let c = s_of_avenue(p.x);
    let a = abs(c);
    let side = select(1.0, -1.0, c < 0.0);
    let k = clamp(i32(floor((a - AVENUE * 0.5) / BLOCK + 0.5)), 0, ROWS);
    var mid = row_edge(k);
    var hs = lane_width(k) * 0.5;
    if (k == ROWS) {
        // The bank road: its row 12 half.
        hs = lane_width(k) * 0.25;
        mid = row_edge(k) - hs;
    }
    if (k == 0) {
        mid = 0.0;
        hs = AVENUE * 0.5;
    }
    return Streets(p.y - gx, cross_width(i) * 0.5, i, side * (a - mid), hs, k, cell.rect.zw, cell.rect.xy);
}

// A road at a point.
struct Road {
    // Across: from its middle line (a two-way street) or from the median's kerb (one of the avenue's
    // one-way carriageways, 14 m). Its half-width (0: no road here).
    v: f32,
    hw: f32,
    // Along it (local metres), and how far past the crossing street's kerb (negative: in the
    // crossing, where nothing is painted and the kerbs fall away).
    u: f32,
    e: f32,
    // Whether this half drives into that crossing (its stop line); whether it's one way.
    inbound: bool,
    oneway: bool,
    district: i32,
    wear: f32,
    // The footprint across and along it.
    fv: f32,
    fu: f32,
    // 0 plain, 1 a bridge over the canal, 2 a level crossing over the tram's tracks.
    over: i32,
    // Which street (or which block's stretch of it) this is, so that its patches and manholes are
    // its own and not every block's the same.
    id: f32,
};

fn no_road() -> Road {
    return Road(0.0, 0.0, 0.0, -1.0, false, false, 0, 0.0, 1.0, 1.0, 0, 0.0);
}

// The ids: a street along x by its block, its row edge and its side of the avenue; a cross street
// (its `u` runs the strip's width) by its grid line.
fn along_id(bx: i32, k: i32, c: f32) -> f32 {
    return f32(bx) * 1.7 + f32(k) * 13.1 + select(7.9, -7.9, c < 0.0);
}

fn cross_id(gi: i32) -> f32 {
    return f32(gi) * 1.3 + 100.0;
}

// A block row's street (a point outside its block's kerb): along x (and the box where it meets a
// cross street), or a cross street along s (over the canal, a bridge).
fn street_road(cell: Cell, st: Streets, kind: i32, district: i32, wear: f32, fp: vec2<f32>) -> Road {
    let p = cell.p;
    if (st.k > 0 && abs(st.ds) < st.hs) {
        return Road(st.ds, st.hs, p.y - st.seg_x.x, abs(st.dx) - st.hx, st.ds * st.dx * KEEP_RIGHT > 0.0, false,
            district, wear, fp.x, fp.y, 0, along_id(cell.bx, st.k, s_of_avenue(p.x)));
    }
    // Into the avenue, the crossing is its pavement's zebra (ROAD_OUT + 0.5 to + 4.5).
    let e = select(abs(st.ds) - st.hs, abs(s_of_avenue(p.x)) - (ROAD_OUT + 0.5), st.k == 0);
    return Road(st.dx, st.hx, p.x, e, st.dx * st.ds * KEEP_RIGHT < 0.0, false, district, wear, fp.y, fp.x,
        select(0, 1, kind == 4), cross_id(st.gi));
}

// The avenue's roads: its carriageways, and where a cross street crosses it (over the pavement, a
// zebra and a stop line; over the carriageway, a junction; over the tracks, a level crossing). No
// road on its median and its pavements.
fn avenue_road(p: vec2<f32>, bx: i32, st: Streets, wear: f32, fp: vec2<f32>) -> Road {
    let c = s_of_avenue(p.x);
    let a = abs(c);
    let way = along_id(bx, 0, c);
    let e = abs(st.dx) - st.hx;
    let u = p.y - st.seg_x.x;
    if (e < 0.0) {
        if (a >= ROAD_OUT + 0.5) {
            return Road(st.dx, st.hx, p.x, a - ROAD_OUT - 0.5, st.dx * c * KEEP_RIGHT < 0.0, false, 0, wear,
                fp.y, fp.x, 0, cross_id(st.gi));
        }
        if (a >= MEDIAN * 0.5) {
            return Road(a - MEDIAN * 0.5, 7.0, u, -1.0, false, true, 0, wear, fp.x, fp.y, 0, way);
        }
        return Road(st.dx, st.hx, p.x, -1.0, false, false, 0, wear, fp.y, fp.x, 2, cross_id(st.gi));
    }
    if (a >= MEDIAN * 0.5 && a < ROAD_OUT) {
        return Road(a - MEDIAN * 0.5, 7.0, u, e, c * st.dx * KEEP_RIGHT > 0.0, true, 0, wear, fp.x, fp.y, 0, way);
    }
    return no_road();
}

// A road's surface and its wear, `u` along it (local metres) and `y` across. `lane`: across, the
// nearest lane's middle (tyre tracks either side of it, oil down it), `in_lanes` 1 in the running
// lanes; `kerb` how far from the nearest kerb (its gutter, dust and shadow; large: none); `wait`
// 0..1 where the traffic queues; `pid` tells halves apart. Asphalt, or granite setts in the Old
// Town, concrete panels in the works and the port.
fn road_surface(u: f32, y: f32, lane: f32, in_lanes: f32, kerb: f32, wait: f32, pid: f32,
                district: i32, wear: f32, p: vec2<f32>, g: Grain, fv: f32, fu: f32) -> Paint {
    let fi = max(fv, fu);
    var col = vec3(0.105, 0.105, 0.11);
    var rough = 0.88;
    let setts = district == 5;
    let panels = district == 7 || district == 9;
    if (setts || panels) {
        // Setts in rows across the street, offset row to row; or concrete panels by the lanes.
        let size = select(vec2(3.6, 5.0), vec2(0.3, 0.2), setts);
        let row = floor(u / size.y);
        let off = select(0.0, 0.5 * size.x * (row - 2.0 * floor(row * 0.5)), setts);
        let j = joints(vec2(y + off, u), size, select(0.04, 0.035, setts), vec2(fv, fu));
        let h = mix(0.5, hash13(vec3(floor((y + off) / size.x), row, 7.0 + pid)), fade(fi, max(size.x, size.y)));
        col = select(vec3(0.33, 0.32, 0.3) * (0.85 + 0.25 * h) * (1.0 - 0.55 * j),
            vec3(0.23, 0.215, 0.2) * (0.8 + 0.4 * h) * (1.0 - 0.45 * j), setts);
        rough = select(0.9, 0.72, setts);
    }
    // The aggregate (its grain close up), and grime in drifts: blotchier the harder the street works.
    var grain = 0.5;
    if (fi < 0.075) {
        grain = mix(0.5, nz(p, 19.0, 3.5), fade(fi, 0.05));
    }
    col *= (0.9 + 0.2 * g.hi) * (0.84 + 0.32 * grain);
    col *= 1.0 - 0.28 * wear * smoothstep(0.35, 0.8, g.lo);
    // Tyre tracks either side of each lane's middle: darker, polished (their average from afar).
    let st = sqrt(0.1225 + 0.25 * fv * fv);
    let dv = y - lane;
    let kt = -0.5 / (st * st);
    let tr = 0.35 / st * (exp(kt * (dv - 0.85) * (dv - 0.85)) + exp(kt * (dv + 0.85) * (dv + 0.85)));
    let tracks = in_lanes * mix(tr, 0.49, smoothstep(0.8, 2.0, st));
    col *= 1.0 - 0.22 * tracks * (0.4 + 0.6 * wear);
    rough -= 0.14 * tracks;
    // Oil down the lanes' middles, thickest where the traffic waits.
    let so = sqrt(0.2 + 0.25 * fv * fv);
    let oil = in_lanes * smoothstep(0.56, 0.78, g.mid) * (0.45 / so) * exp(-0.5 * dv * dv / (so * so))
        * (0.35 + 0.65 * wait) * (0.3 + 0.7 * wear);
    col *= 1.0 - 0.5 * oil;
    rough = mix(rough, 0.35, oil);
    // Patches where it's been dug up and filled: newer, darker, a sealed seam round each.
    var mended = 0.0;
    let cu = floor(u / 12.0);
    let ph = hash13(vec3(cu, floor(lane * 0.3) + pid * 7.0, 5.0));
    let odds = select(0.04 + 0.16 * wear, 0.0, setts || panels);
    if (ph < odds) {
        let q = ph / odds;
        let a0 = cu * 12.0 + 0.5 + 4.0 * fract(q * 7.31);
        let a1 = a0 + 1.2 + 6.0 * fract(q * 13.7);
        let v0 = lane - 1.6 + 0.8 * fract(q * 17.3);
        let v1 = v0 + 1.0 + 2.0 * fract(q * 23.9);
        mended = span(u, a0, a1, fu) * span(y, v0, v1, fv);
        let seam = mended * (1.0 - span(u, a0 + 0.1, a1 - 0.1, fu) * span(y, v0 + 0.1, v1 - 0.1, fv));
        col = mix(col, vec3(0.07, 0.07, 0.075) * (0.9 + 0.2 * g.hi), mended * 0.7);
        col = mix(col, vec3(0.03), 0.5 * seam * fade(fi, 0.2));
        rough = mix(rough, 0.4, seam);
    }
    // Cracks, where it's worn (close up only: past a few metres they're part of its grain).
    if (fi < 0.15 && !setts) {
        let nc = 0.7 * nz(p + vec2(31.0, 17.0), 0.55, 4.0) + 0.3 * nz(p, 3.1, 5.0);
        let crack = band(abs(nc - 0.5) / 0.75, 0.004 + 0.005 * wear, fi)
            * smoothstep(0.85 - 0.45 * wear, 0.97 - 0.3 * wear, g.lo) * fade(fi, 0.1) * (1.0 - mended);
        col *= 1.0 - 0.5 * crack;
    }
    // By the kerb: the gutter's concrete channel, dust and leaves gathering, the kerb's shadow,
    // a drain every 20 m.
    let gut = 1.0 - past(kerb, 0.45, fv);
    col = mix(col, vec3(0.25, 0.245, 0.235) * (0.8 + 0.3 * g.hi), gut);
    rough = mix(rough, 0.8, gut);
    let dust = (1.0 - smoothstep(0.1, 1.6, kerb)) * (0.3 + 0.7 * wear) * (0.45 + 0.55 * g.mid);
    col = mix(col, vec3(0.3, 0.27, 0.22), 0.55 * dust);
    col *= mix(0.62, 1.0, smoothstep(0.0, 0.35, kerb));
    let drain = span(kerb, 0.07, 0.47, fv) * stripes(u - 9.6, 20.0, 0.8, fu);
    col = mix(col, vec3(0.025) + vec3(0.05) * mix(0.5, lines(u, 0.08, 0.035, fu), fade(fu, 0.1)), drain);
    var out = mk(col, clamp(rough, 0.05, 1.0));
    // Puddles: in the gutters, in the ruts where it dips, between the setts.
    let dips = smoothstep(0.66, 0.8, g.lo) * (0.35 + 0.65 * min(tracks, 1.0));
    out.wet = clamp(max(gut * smoothstep(0.42, 0.62, g.mid), dips) * (1.0 - 0.6 * mended) + select(0.0, 0.15, setts), 0.0, 1.0);
    return out;
}

// A manhole cover in a lane every 32 m or so (some go without): how much of the pixel it covers.
fn manhole(u: f32, y: f32, lane: f32, pid: f32, fi: f32) -> f32 {
    let k = floor(u / 32.0);
    let h = hash13(vec3(k, floor(lane * 0.3) + pid * 7.0, 9.0));
    let d = length(vec2(u - (k * 32.0 + 8.0 + 16.0 * h), y - lane - 0.9 * (fract(h * 9.7) - 0.5)));
    return step(h, 0.7) * band(d, 0.33, fi);
}

// Markings worn through, most where the traffic's heaviest: how much of the paint is left.
fn paint_left(wear: f32, g: Grain) -> f32 {
    return 1.0 - (0.1 + 0.5 * wear) * smoothstep(0.25, 0.75, g.hi);
}

// A road: its surface, and its paint. A two-way street: the centre (a double amber line; a hatched
// island on the wide streets; a dashed line on the bank road), lanes of 3.6 m (solid near the
// crossing), the edge line, parking bays by the kerb. One of the avenue's carriageways: kerb stones
// either side, an amber edge line on the median's side, three lanes, a cycle lane by the pavement.
// At a crossing, a zebra and (on the half driving into it) a stop line.
fn paint_road(r: Road, p: vec2<f32>, g: Grain) -> Paint {
    let fv = r.fv;
    let fu = r.fu;
    let fi = max(fv, fu);
    let wide = r.hw > 15.0;
    let narrow = r.hw < 11.0;
    let c0 = select(select(0.3, 1.6, wide), 0.0, narrow);
    let nl = select(select(2.0, 3.0, wide), 1.0, narrow);
    let edge = c0 + 3.6 * nl;
    var y = abs(r.v);
    var lane = c0 + 1.8 + 3.6 * clamp(floor((y - c0) / 3.6), 0.0, nl - 1.0);
    var in_lanes = 1.0 - past(y, edge, fv);
    var kerb = r.hw - y;
    var pid = r.id + select(1.0, 2.0, r.v < 0.0);
    if (r.oneway) {
        y = r.v;
        lane = 2.7 + 3.6 * clamp(floor((y - 0.9) / 3.6), 0.0, 2.0);
        in_lanes = span(y, 0.9, 11.7, fv);
        kerb = min(y - 0.3, 13.7 - y);
        pid = r.id + 3.0;
    }
    let kerbed = past(r.e, 0.0, fu);
    var out = road_surface(r.u, y, lane, in_lanes, select(1e3, kerb, r.e > 0.0), 1.0 - smoothstep(6.0, 30.0, r.e),
        pid, r.district, r.wear, p, g, fv, fu);
    var col = mix(out.albedo, vec3(0.08, 0.075, 0.07) + vec3(0.05, 0.02, 0.0) * r.wear, manhole(r.u, y, lane, pid, fi));
    let pst = past(r.e, 6.4, fu);
    let solid = 1.0 - past(r.e, 24.0, fu);
    let dash = stripes(r.u, 9.0, 3.0, fu);
    let zebra = span(r.e, 0.5, 4.5, fu);
    let stop = select(0.0, 1.0, r.inbound) * span(r.e, 5.8, 6.3, fu);
    let edged = past(r.e, 0.5, fu);
    var white = 0.0;
    var amber = 0.0;
    var ks = 0.0;
    if (r.oneway) {
        col = mix(col, vec3(0.2, 0.085, 0.06) * (0.85 + 0.3 * g.mid), 0.85 * span(y, 11.95, 13.25, fv) * kerbed);
        amber = band(y - 0.825, 0.075, fv) * edged;
        white = (band(y - 4.5, 0.075, fv) + band(y - 8.1, 0.075, fv)) * mix(dash, 1.0, solid) * pst
            + band(y - 11.775, 0.075, fv) * edged
            + zebra * stripes(y, 1.0, 0.5, fv) * span(y, 0.6, 13.4, fv)
            + stop * span(y, 0.9, 11.7, fv);
        ks = (1.0 - past(y, 0.3, fv) + past(y, 13.7, fv)) * kerbed;
    } else {
        if (narrow) {
            white = band(y, 0.075, fv) * mix(dash, 1.0, solid) * pst;
        } else if (wide) {
            white = (band(y - 1.45, 0.075, fv) + band(y, 1.3, fv) * lines(r.u + y, 3.0, 0.45, 0.7 * (fu + fv))) * pst;
        } else {
            amber = band(y - 0.18, 0.06, fv) * pst;
        }
        let k = clamp(floor((y - c0) / 3.6 + 0.5), 1.0, max(nl - 1.0, 1.0));
        white += step(1.5, nl) * band(y - c0 - 3.6 * k, 0.075, fv) * mix(dash, 1.0, solid) * pst
            + band(y - edge - 0.075, 0.075, fv) * edged
            + span(y, edge + 0.15, edge + 2.4, fv) * lines(r.u, 6.0, 0.12, fu) * past(r.e, 10.0, fu)
            + zebra * stripes(r.v + r.hw, 1.0, 0.5, fv) * band(r.v, r.hw - 0.6, fv)
            + stop * span(y, 0.3, r.hw - 0.6, fv);
    }
    let keep = paint_left(r.wear, g);
    col = mix(col, vec3(0.7, 0.7, 0.68), min(white, 1.0) * keep);
    col = mix(col, vec3(0.6, 0.4, 0.07), min(amber, 1.0) * keep);
    col = mix(col, kerb_stone(r.u, fu, g.hi), ks);
    let marked = min((white + amber) * keep + ks, 1.0);
    out.albedo = col;
    out.roughness = mix(out.roughness, 0.6, marked);
    out.wet *= 1.0 - marked;
    return out;
}

// Over the canal, a bridge: its deck, a joint where it meets each quay, its parapets. Over the
// tram's tracks, a level crossing: the rails set in the road, an amber grid to keep it clear.
fn road_over(o: Paint, r: Road, cell: Cell, st: Streets) -> Paint {
    var out = o;
    let p = cell.p;
    if (r.over == 1) {
        let y = abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) - CANAL_WIDTH * 0.5;
        let deck = 1.0 - past(y, 0.3, r.fu);
        var col = mix(out.albedo, out.albedo * 0.85 + vec3(0.03), deck);
        col = mix(col, vec3(0.03), band(y - 0.3, 0.05, r.fu));
        let parapet = deck * (1.0 - past(st.hx - abs(st.dx), 0.6, r.fv));
        out.albedo = mix(col, vec3(0.42, 0.41, 0.39), parapet);
        out.wet *= 1.0 - parapet;
    } else if (r.over == 2) {
        let c = s_of_avenue(p.x);
        let t = abs(abs(c) - TRACK_OFFSET);
        let rails = band(t - 0.75, 0.036, r.fu);
        var col = mix(out.albedo, vec3(0.03), band(t - 0.69, 0.02, r.fu));
        col = mix(col, vec3(0.58, 0.58, 0.6), rails);
        let fd = r.fu + r.fv;
        let grid = max(lines(st.dx + c, 2.5, 0.15, fd), lines(st.dx - c, 2.5, 0.15, fd))
            * band(st.dx, st.hx - 1.0, r.fv) * band(c, MEDIAN * 0.5 - 0.6, r.fu);
        out.albedo = mix(col, vec3(0.6, 0.45, 0.06), 0.85 * grid);
        out.roughness = mix(out.roughness, 0.25, rails);
    }
    return out;
}

// One of the city's lamp rows round a point (`lamps_near` sums them, `lantern_burn` finds a
// lantern's own): `u` along it from its start, `v` across from the line of its pools' middles (1e4:
// no row here), how long it is and whether its lamps stand corner to corner or between the corners,
// the pixel's footprint along and across it, which row it is (for its lamps' brightness,
// `lamp_burns`) and its gain.
struct LampLine {
    u: f32,
    v: f32,
    len: f32,
    corners: bool,
    fu: f32,
    fv: f32,
    id: f32,
    gain: f32,
};

// Row `i` of the seven round a point: the kerbs of the street along x and of the cross street round
// it, the avenue's pavements and median, a park's loop, a quay's edge.
fn lamp_line(i: i32, cell: Cell, st: Streets, kind: i32, seed: f32, fp: vec2<f32>) -> LampLine {
    let p = cell.p;
    let r = cell.rect;
    let c = s_of_avenue(p.x);
    let side = select(1.0, -1.0, c < 0.0);
    let rows = abs(cell.row) >= 1 && abs(cell.row) <= ROWS;
    var l = LampLine(0.0, 1e4, 1.0, true, fp.y, fp.x, 0.0, 1.0);
    switch i {
        case 0, 1: {
            // The kerbs of the street along x (and the bank road's).
            if (st.k > 0) {
                let kerb = select(-1.0, 1.0, i == 0);
                l.u = p.y - st.seg_x.x;
                l.len = st.seg_x.y - st.seg_x.x;
                l.v = st.ds - kerb * (st.hs - LAMP_OUT);
                l.id = f32(cell.bx) * 1.7 + f32(st.k) * 13.1 + side * 7.9 + kerb * 3.3;
            }
        }
        case 2, 3: {
            // The kerbs of the cross street, between its corners (none in the canal's row: its
            // posts would stand over the water).
            if (rows && kind != 4) {
                let kerb = select(-1.0, 1.0, i == 2);
                l.u = p.x - st.seg_s.x;
                l.len = st.seg_s.y - st.seg_s.x;
                l.v = st.dx - kerb * (st.hx - LAMP_OUT);
                l.corners = false;
                l.fu = fp.x;
                l.fv = fp.y;
                l.id = f32(st.gi) * 1.3 + f32(cell.row) * 11.7 + 100.0 + kerb * 4.1;
            }
        }
        case 4, 5: {
            // The avenue's: on its pavements' kerb side, and (dimmer) on the tram's posts.
            if (st.k == 0) {
                l.u = p.y - st.seg_x.x;
                l.len = st.seg_x.y - st.seg_x.x;
                l.v = select(c, abs(c) - AVENUE_LAMP, i == 4);
                l.gain = select(0.55, 1.0, i == 4);
                l.id = f32(cell.bx) * 1.7 + 500.0 + select(200.0, side * 7.9, i == 4);
            }
        }
        default: {
            if (rows && kind == 2) {
                // A park's, beside the loop of its path (between its corners: its diagonals run
                // to them).
                let inset = SIDEWALK + PARK_LOOP;
                let lr = vec4(r.x + inset, r.y - inset, r.z + inset, r.w - inset);
                let near_s = min(abs(p.x - lr.x), abs(p.x - lr.y)) < min(abs(p.y - lr.z), abs(p.y - lr.w));
                l.u = select(p.x - lr.x, p.y - lr.z, near_s);
                l.len = select(lr.y - lr.x, lr.w - lr.z, near_s);
                l.v = inside(lr, p) - PARK_LAMP;
                l.corners = false;
                l.fu = select(fp.x, fp.y, near_s);
                l.fv = max(fp.x, fp.y);
                l.id = seed + select(0.0, 50.0, near_s);
                l.gain = 0.55;
            } else if (rows && kind == 4) {
                // A quay's, along the water's edge.
                l.u = p.y - r.z;
                l.len = r.w - r.z;
                l.v = abs(p.x - 0.5 * (r.x + r.y)) - CANAL_WIDTH * 0.5 - QUAY_LAMP;
                l.id = f32(cell.bx) * 1.7 + select(900.0, 950.0, p.x < 0.5 * (r.x + r.y));
                l.gain = 0.85;
            }
        }
    }
    return l;
}

// The city's lamp rows near a point, worked out one at a time in a loop (one copy of the work). A
// row's pools fade out within about 25 m of it, so rows further off are passed over.
fn lamps_near(cell: Cell, st: Streets, kind: i32, seed: f32, fp: vec2<f32>) -> f32 {
    var l = 0.0;
    for (var i = 0; i < 7; i++) {
        let row = lamp_line(i, cell, st, kind, seed, fp);
        if (abs(row.v) < 26.0 + 2.0 * row.fv) {
            l += row.gain * lamp_row(row.u, row.v, row.len, row.corners, row.fu, row.fv, row.id);
        }
    }
    return l;
}

// How bright a lamp's lantern burns, hung over the middle of its pool at the cell's point: as its
// own lamp burns in the paint (`lamp_burns` times its row's gain: out if that's out), without the
// halo or its neighbours' light. Only the rows' arithmetic is repeated for it: the heavy part
// (`lamp_row`) is called from `lamps_near` alone. Away from any pool's middle (it isn't asked
// there): the lamps' mean.
fn lantern_burn(cell: Cell, st: Streets, kind: i32, seed: f32) -> f32 {
    var best = lamp_line(0, cell, st, kind, seed, vec2(0.0));
    for (var i = 1; i < 7; i++) {
        let row = lamp_line(i, cell, st, kind, seed, vec2(0.0));
        if (abs(row.v) < abs(best.v)) {
            best = row;
        }
    }
    let n = max(1.0, floor(best.len / LAMP_GAP + 0.5));
    let a = floor(best.u * n / best.len + 0.5);
    let i0 = select(1.0, 0.0, best.corners);
    let i1 = select(n - 1.0, n, best.corners);
    if (abs(best.v) > 0.5 || a < i0 || a > i1) {
        return LAMP_MEAN;
    }
    return best.gain * lamp_burns(a, best.id);
}

// ---- The avenue's median and pavements ----------------------------------------------------------

// The tram's median (`a` < MEDIAN / 2): ballast between and round the tracks (in the Gardens they
// run in grass), the catenary's footings down the middle, each track's sleepers and rails, short
// grass outside them, kerb stones at its edges with the colony's light line along them in the
// strip's colour. At each crossing, a paved way over the tracks.
fn median(a: f32, u: f32, e: f32, strip: i32, g: Grain, fp: vec2<f32>) -> Paint {
    let fv = fp.x;
    let fu = fp.y;
    let t = abs(a - TRACK_OFFSET);
    let gravel = vec3(0.25, 0.24, 0.225) * (0.75 + 0.5 * g.hi);
    let turf = vec3(0.07, 0.14, 0.04) * (0.75 + 0.5 * g.mid);
    let ballast = select(gravel, turf, strip == 2);
    var col = ballast;
    col = mix(col, vec3(0.36, 0.35, 0.33), band(a, 0.4, fv) * stripes(u - 14.6, 30.0, 0.8, fu));
    var rough = 0.95;
    let verge = past(a, TRACK_OFFSET + 1.6, fv);
    col = mix(col, vec3(0.09, 0.16, 0.05) * (0.75 + 0.5 * g.mid), verge);
    let bed = 1.0 - past(t, 1.6, fv);
    let sleepers = lines(u, 0.65, 0.24, fu) * (1.0 - past(t, 1.3, fv));
    var trk = mix(ballast, select(vec3(0.42, 0.41, 0.39), vec3(0.3, 0.29, 0.27), strip == 2), sleepers);
    // Brake dust, rust-brown, between the rails.
    trk = mix(trk, vec3(0.17, 0.11, 0.07), 0.3 * (1.0 - past(t, 0.7, fv)));
    let rails = band(t - 0.75, 0.036, fv);
    trk = mix(trk, vec3(0.58, 0.58, 0.6), rails);
    col = mix(col, trk, bed);
    rough = mix(rough, 0.25, rails);
    let xing = span(e, 0.5, 4.5, fu);
    var xc = vec3(0.42, 0.41, 0.39) * (1.0 - 0.3 * joints(vec2(a, u), vec2(0.6), 0.02, fp));
    xc = mix(xc, vec3(0.03), band(t - 0.69, 0.02, fv));
    xc = mix(xc, vec3(0.58, 0.58, 0.6), rails);
    col = mix(col, xc, xing);
    rough = mix(rough, 0.7, xing * (1.0 - rails));
    let ks = past(a, MEDIAN * 0.5 - 0.3, fv);
    col = mix(col, kerb_stone(u, fu, g.hi), ks);
    var out = mk(col, rough);
    out.glow = line_colour(strip) * band(a - (MEDIAN * 0.5 - 0.45), 0.05, fv) * (1.0 - xing);
    out.wet = 0.1 * bed;
    return out;
}

// One of the avenue's pavements (`a` from ROAD_OUT to AVENUE / 2), where the people walk: the kerb,
// a furniture zone of dark setts with a tree pit every 8 m (and the lamps), a walk, a row of
// planting beds with gaps to cross by, another walk. Charter granite flags, Canal brick with
// granite bands, Gardens buff flags. Tactile paving where the crossings land.
fn avenue_pavement(a: f32, u: f32, e: f32, strip: i32, wear: f32, g: Grain, fp: vec2<f32>) -> Paint {
    let fv = fp.x;
    let fu = fp.y;
    let fi = max(fv, fu);
    let y = a - ROAD_OUT;
    let furniture = y < 4.3;
    var base = vec3(0.42, 0.41, 0.395);
    var size = vec2(1.2, 0.6);
    var jw = 0.012;
    var rough = 0.6;
    if (furniture) {
        base = vec3(0.2, 0.195, 0.19);
        size = vec2(0.15, 0.15);
        jw = 0.02;
        rough = 0.8;
    } else if (strip == 1) {
        base = vec3(0.3, 0.155, 0.1);
        size = vec2(0.22, 0.11);
        rough = 0.82;
    } else if (strip == 2) {
        base = vec3(0.4, 0.355, 0.275);
        size = vec2(0.6, 0.6);
        jw = 0.015;
        rough = 0.85;
    }
    var col = flags(vec2(u, y), size, base, jw, vec2(fu, fv), f32(strip) + select(31.0, 34.0, furniture));
    if (strip == 1) {
        col = mix(col, vec3(0.42, 0.41, 0.39), lines(u, 4.8, 0.4, fu) * past(y, 4.3, fv));
    }
    // Tree pits: a granite frame round an iron grille (Charter), gravel (Canal), grass (Gardens); a
    // tree in each (`bc_sim::colony::furniture`), none where the crossings land.
    let pit_y = AVENUE_TREE - ROAD_OUT;
    let keep = past(e, TREE_END, fu);
    let pit = keep * stripes(u - (TREE_FIRST - 0.8), TREE_PITCH, 1.6, fu) * span(y, pit_y - 0.8, pit_y + 0.8, fv);
    let pit_in = keep * stripes(u - (TREE_FIRST - 0.68), TREE_PITCH, 1.36, fu)
        * span(y, pit_y - 0.68, pit_y + 0.68, fv);
    var pit_col = vec3(0.1, 0.09, 0.08) * (1.0 - 0.6 * mix(0.5, lines(u, 0.1, 0.05, fu), fade(fu, 0.1)));
    if (strip == 1) {
        pit_col = vec3(0.3, 0.27, 0.23) * (0.8 + 0.3 * g.hi);
    } else if (strip == 2) {
        pit_col = vec3(0.08, 0.15, 0.05) * (0.8 + 0.4 * g.hi);
    }
    col = mix(col, vec3(0.44, 0.43, 0.41), pit - pit_in);
    col = mix(col, pit_col, pit_in);
    // The planting beds: low shrubs in a stone edging (in the Gardens, flowers among them).
    let bed = stripes(u - 1.0, 8.0, 6.0, fu) * span(y, 9.5, 11.5, fv);
    let bed_in = stripes(u - 1.15, 8.0, 5.7, fu) * span(y, 9.65, 11.35, fv);
    var plants = mix(vec3(0.045, 0.09, 0.03), vec3(0.1, 0.17, 0.05), g.mid);
    if (strip == 2) {
        let bloom = smoothstep(0.62, 0.78, g.hi) * fade(fi, 0.5);
        plants = mix(plants, mix(vec3(0.55, 0.12, 0.2), vec3(0.62, 0.5, 0.1), step(0.5, g.lo)), bloom);
    }
    col = mix(col, vec3(0.44, 0.43, 0.41), bed - bed_in);
    col = mix(col, plants, bed_in);
    rough = mix(rough, 0.95, min(bed_in + pit_in, 1.0));
    col = mix(col, kerb_stone(u, fu, g.hi), 1.0 - past(y, 0.3, fv));
    // Tactile paving where the zebra lands (the zebra is `avenue_road`'s at e 0.5 to 4.5 past the
    // carriageway's kerb + 0.5: y 1 to 5).
    let tact = (1.0 - past(e, 0.8, fu)) * span(y, 1.0, 5.0, fv);
    let dots = mix(0.5, lines(u, 0.06, 0.025, fu) * lines(y, 0.06, 0.025, fv), fade(fi, 0.06));
    col = mix(col, vec3(0.55, 0.42, 0.07) * (0.85 + 0.3 * dots), tact);
    // Grime in drifts (trodden in where the crowds go, some even on Charter's), stains, and dust
    // along the kerb.
    col *= 1.0 - 0.18 * wear * smoothstep(0.4, 0.8, g.mid);
    col *= 1.0 - (0.06 + 0.16 * wear) * smoothstep(0.3, 0.85, g.lo);
    col *= 1.0 - (0.08 + 0.2 * wear) * smoothstep(0.7, 0.82, g.hi);
    col = mix(col, vec3(0.28, 0.26, 0.22), 0.4 * wear * (1.0 - smoothstep(0.3, 1.2, y)));
    col *= 0.9 + 0.2 * g.hi;
    var out = mk(col, rough);
    out.wet = smoothstep(0.7, 0.85, g.lo) * (1.0 - bed_in) * 0.7;
    return out;
}

// ---- The colony's own ground --------------------------------------------------------------------

// Hub Gate's square (and the far cap's foot): pale stone in 2 m flags, a grid of darker bands every
// 32 m with the strip's line colour inlaid down every other one, glowing by night, lamps down the
// inlaid bands; before the terminal, rings round its door and spokes out from it, two rings of
// lamps; the tram's way through it in darker stone with the rails set in, light lines along it.
fn paint_square(p: vec2<f32>, strip: i32, lit: bool, g: Grain, fp: vec2<f32>) -> Paint {
    let c = s_of_avenue(p.x);
    let hub = p.y < 0.0;
    let cap = GRID_X0 + f32(HUB_START) * BLOCK;
    let u = vec2(c, abs(p.y - select(-cap, cap, hub)));
    let fi = max(fp.x, fp.y);
    let accent = line_colour(strip);
    var col = flags(u.yx, vec2(2.0, 2.0), vec3(0.5, 0.495, 0.475), 0.012, fp.yx, 70.0);
    var grid_on = 1.0;
    var glow = vec3(0.0);
    var ring_lamps = 0.0;
    let q = vec2(c, p.y - TERMINAL_FRONT);
    let r = length(q);
    if (hub && q.y > -fi && r < 220.0 + fi) {
        let fore = (1.0 - past(r, 220.0, fi)) * past(q.y, 0.0, fp.y);
        grid_on = 1.0 - fore;
        let rings = lines(r, 24.0, 0.8, fi) * step(20.0, r);
        let ang = heading(q);
        let sp = PI / 12.0;
        let spoke = band(r * abs(sin(ang - sp * floor(ang / sp + 0.5))), 0.15, fi) * step(24.0, r);
        let ring_inlay = min(band(r - 48.0, 0.15, fi) + band(r - 144.0, 0.15, fi), 1.0);
        col = mix(col, vec3(0.3, 0.3, 0.31), fore * max(rings, spoke));
        col = mix(col, accent, fore * ring_inlay);
        glow += accent * fore * ring_inlay;
        if (lit) {
            // Thirty-six lamps on the ring 96 m out, and as many 192 m out.
            let near_ring = select(192.0, 96.0, r < 144.0);
            let gap = 2.0 * PI * near_ring / 36.0;
            let ua = (ang + PI) * near_ring;
            ring_lamps = fore * lamps_across(r - near_ring, LAMP_CORE, fi)
                * lamps_periodic(ua - gap * floor(ua / gap), gap, LAMP_CORE, fi);
        }
    }
    let bands = max(lines(u.x - 16.0, 32.0, 1.6, fp.x), lines(u.y - 16.0, 32.0, 1.6, fp.y));
    let inlay = max(lines(u.x - 16.0, 64.0, 0.2, fp.x), lines(u.y - 16.0, 64.0, 0.2, fp.y));
    col = mix(col, vec3(0.33, 0.33, 0.34), bands * grid_on);
    col = mix(col, accent, inlay * grid_on);
    glow += accent * inlay * grid_on;
    var rough = 0.5;
    if (hub && abs(c) < MEDIAN * 0.5 + 1.0) {
        let a = abs(c);
        let way = 1.0 - past(a, MEDIAN * 0.5, fp.x);
        let t = abs(a - TRACK_OFFSET);
        var w = vec3(0.3, 0.3, 0.31) * (1.0 - 0.3 * joints(u, vec2(1.0), 0.015, fp));
        w = mix(w, vec3(0.03), band(t - 0.69, 0.02, fp.x));
        let rails = band(t - 0.75, 0.036, fp.x);
        w = mix(w, vec3(0.58, 0.58, 0.6), rails);
        col = mix(col, w, way);
        rough = mix(rough, 0.25, rails);
        let edge = band(a - (MEDIAN * 0.5 - 0.3), 0.06, fp.x);
        col = mix(col, accent, edge);
        glow = mix(glow, accent * edge, way);
    }
    // Swept every night: only a little grime, in drifts.
    col *= 1.0 - 0.08 * smoothstep(0.45, 0.85, g.lo);
    var out = mk(col, rough);
    out.glow = glow;
    out.wet = 0.5 * smoothstep(0.74, 0.86, 1.0 - g.lo);
    if (lit) {
        // Lamps down the inlaid bands (every 64 m), 32 m apart along them: the square's lines of
        // light, dotted rather than solid, so it reads as a square and not a lit grid.
        let gg = u - vec2(16.0);
        let gb = gg - 64.0 * floor(gg / 64.0 + 0.5);
        let ga = gg - 32.0 * floor(gg / 32.0);
        out.lamps = 0.4 * (lamps_across(gb.x, LAMP_CORE, fp.x) * lamps_periodic(ga.y, 32.0, LAMP_CORE, fp.y)
            + lamps_across(gb.y, LAMP_CORE, fp.y) * lamps_periodic(ga.x, 32.0, LAMP_CORE, fp.x)) * grid_on
            + 0.5 * ring_lamps + 0.015;
    }
    return out;
}

// The window bank's park (row 13), `edge` in from the glass: the railing's plinth, the colony's
// light strip along its foot, a promenade (Charter granite, Canal concrete with steel edging,
// Gardens timber decking), lamps, a hedge with gaps, the park (paths along it and across it where
// the cross streets end), and a footpath along the bank road.
fn paint_bank(p: vec2<f32>, strip: i32, lit: bool, g: Grain, fp: vec2<f32>) -> Paint {
    let edge = min(p.x, STRIP_WIDTH - p.x);
    let fv = fp.x;
    let fu = fp.y;
    let fi = max(fv, fu);
    // Along the bank, exact to the millimetre anywhere on the strip: every period below (BLOCK, 25,
    // 0.6, 2 and 3 m) divides 9600 m, so the wrap shows no seam.
    let x = along(p.y, 9600.0);
    let accent = line_colour(strip);
    var col = grass(g);
    col = mix(col, vec3(0.04, 0.075, 0.03), 0.75 * tree_shade(g));
    let path = max(band(edge - 52.0, 1.5, fv), lines(x, BLOCK, 3.0, fu) * span(edge, 15.0, 96.5, fv));
    col = mix(col, vec3(0.4, 0.36, 0.29) * (0.85 + 0.3 * g.hi), path);
    let hedge = span(edge, 12.0, 15.0, fv) * (1.0 - lines(x - 12.5, 25.0, 2.5, fu));
    col = mix(col, vec3(0.04, 0.09, 0.03) * (0.7 + 0.6 * g.mid), hedge);
    let promenade = 1.0 - past(edge, 12.0, fv);
    let foot = past(edge, 96.5, fv);
    var rough = 0.7;
    if (promenade + foot > 0.0) {
        // The footpath's concrete flags, or the promenade's.
        var base = vec3(0.34, 0.34, 0.325);
        var size = vec2(0.6, 0.6);
        var jw = 0.015;
        if (promenade > 0.0) {
            base = select(vec3(0.33, 0.325, 0.31), vec3(0.46, 0.455, 0.44), strip == 0);
            size = select(vec2(3.0, 3.0), vec2(2.0, 1.0), strip == 0);
            jw = select(0.02, 0.01, strip == 0);
            rough = select(0.85, 0.55, strip == 0);
        }
        var floor_col = flags(vec2(x, edge), size, base, jw, vec2(fu, fv), 63.0 + f32(strip));
        if (promenade > 0.0 && strip == 1) {
            floor_col = mix(floor_col, vec3(0.3, 0.17, 0.1), 0.3 * smoothstep(0.6, 0.8, g.mid));
            floor_col = mix(floor_col, vec3(0.2, 0.2, 0.21), band(edge - 1.15, 0.08, fv) + band(edge - 11.85, 0.08, fv));
        } else if (promenade > 0.0 && strip == 2) {
            // Boards along the glass, their ends staggered.
            let boards = lines(edge, 0.15, 0.012, fv);
            let row = floor(edge / 0.15);
            let ends_j = mix(0.004, lines(x + 1.7 * fract(row * 0.618) * 3.0, 3.0, 0.012, fu), fade(fv, 0.15));
            let h = mix(0.5, hash13(vec3(row, floor(x / 3.0), 68.0)), fade(fi, 0.3));
            floor_col = vec3(0.32, 0.21, 0.13) * (0.8 + 0.35 * h) * (1.0 - 0.5 * max(boards, ends_j));
            rough = 0.8;
        }
        col = mix(col, floor_col, promenade + foot);
    }
    let strip_line = band(edge - 0.6, 0.12, fv);
    col = mix(col, mix(vec3(0.85), accent, 0.6), strip_line);
    col = mix(col, vec3(0.16, 0.16, 0.17), 1.0 - past(edge, 0.45, fv));
    var out = mk(col, mix(0.93, rough, promenade + foot));
    out.glow = accent * strip_line;
    out.wet = smoothstep(0.72, 0.86, g.lo) * 0.6 * (promenade + foot + path);
    if (lit) {
        let xu = x - 25.0 * floor(x / 25.0);
        out.lamps = 0.8 * (lamps_across(edge - 11.0, LAMP_CORE, fv) * lamps_periodic(xu, 25.0, LAMP_CORE, fu)
            + LAMP_HALO_GAIN * lamps_across(edge - 11.0, LAMP_HALO, fv) * LAMP_HALO * SQRT_TAU / 25.0);
    }
    return out;
}

// ---- Blocks -------------------------------------------------------------------------------------

// The pavement inside a block's kerb (the outer SIDEWALK metres): the kerb stone, then flags by
// district and strip, dirtier towards the walls' feet, a utility cover here and there.
fn paint_sidewalk(cell: Cell, d: f32, district: i32, strip: i32, wear: f32, g: Grain, fp: vec2<f32>) -> Paint {
    let p = cell.p;
    let r = cell.rect;
    let along_x = min(p.x - r.x, r.y - p.x) < min(p.y - r.z, r.w - p.y);
    let u = select(p.x - r.x, p.y - r.z, along_x);
    let fu = select(fp.x, fp.y, along_x);
    let fv = select(fp.y, fp.x, along_x);
    var base = vec3(0.34, 0.34, 0.325);
    var size = vec2(0.6, 0.6);
    var jw = 0.012;
    var rough = 0.85;
    switch district {
        case 1, 2: {
            base = select(vec3(0.41, 0.4, 0.385), vec3(0.44, 0.435, 0.42), strip == 0);
            size = vec2(1.2, 0.8);
            jw = 0.008;
            rough = 0.62;
        }
        case 5: {
            base = vec3(0.36, 0.34, 0.31);
            size = vec2(0.9, 0.6);
            jw = 0.02;
        }
        case 7, 9: {
            base = vec3(0.32, 0.31, 0.295);
            size = vec2(3.0, 3.0);
            jw = 0.02;
            rough = 0.9;
        }
        default: {
            if (strip == 1) {
                base = vec3(0.31, 0.16, 0.105);
                size = vec2(0.22, 0.11);
            } else if (strip == 2) {
                base = vec3(0.4, 0.355, 0.275);
            }
        }
    }
    var col = flags(vec2(u, d), size, base, jw, vec2(fu, fv), 40.0 + f32(district));
    col = mix(col, kerb_stone(u, fu, g.hi), 1.0 - past(d, 0.3, fv));
    col *= 1.0 - 0.18 * wear * smoothstep(0.4, 0.85, g.mid);
    col *= 1.0 - (0.06 + 0.16 * wear) * smoothstep(0.3, 0.85, g.lo);
    col *= 1.0 - (0.08 + 0.2 * wear) * smoothstep(0.7, 0.82, g.hi);
    let foot = smoothstep(3.6, 5.0, d);
    col *= mix(1.0, 0.78, foot * (0.4 + 0.6 * wear));
    if (strip == 2) {
        // Moss at the walls' feet.
        col = mix(col, vec3(0.1, 0.14, 0.06), 0.3 * foot * g.mid);
    }
    if (district == 7 || district == 9) {
        // Rust run off the works' steel, in streaks along the kerb.
        col = mix(col, vec3(0.32, 0.13, 0.05), 0.2 * smoothstep(0.6, 0.85, g.lo) * (1.0 - smoothstep(0.5, 3.0, d)));
    }
    let k = floor(u / 12.0);
    let h = hash13(vec3(k, select(1.0, 2.0, along_x) + 3.0 * f32(cell.row), 44.0 + f32(cell.bx)));
    let cover = step(h, 0.3) * span(u - k * 12.0 - 3.0 - 6.0 * h, -0.3, 0.3, fu) * span(d, 1.6, 2.2, fv);
    col = mix(col, vec3(0.09, 0.085, 0.08), cover);
    col *= 0.92 + 0.16 * g.hi;
    var out = mk(col, rough);
    out.wet = smoothstep(0.72, 0.86, g.lo) * 0.6 * (0.5 + 0.5 * wear);
    return out;
}

// A park: grass in drifts, mown in stripes, darker under the trees; a gravel loop 6 m in from the
// pavement, the two diagonals and a round plaza in the middle ringed with flower beds (the lamps
// along the loop are `lamps_near`'s).
fn paint_park(cell: Cell, seed: f32, g: Grain, fp: vec2<f32>) -> Paint {
    let p = cell.p;
    let r = cell.rect;
    let fi = max(fp.x, fp.y);
    var col = grass(g);
    col *= 1.0 + 0.07 * (2.0 * lines(p.y - r.z, 4.0, 2.0, fp.y) - 1.0) * fade(fp.y, 2.0);
    col = mix(col, vec3(0.04, 0.075, 0.03), 0.8 * tree_shade(g));
    let inset = SIDEWALK + PARK_LOOP;
    let dl = inside(vec4(r.x + inset, r.y - inset, r.z + inset, r.w - inset), p);
    let loop_path = band(dl, PARK_PATH, fi);
    let q = p - vec2(0.5 * (r.x + r.y), 0.5 * (r.z + r.w));
    let dir = normalize(vec2(r.y - r.x, r.w - r.z) - 2.0 * inset);
    let dg = min(abs(q.x * dir.y - q.y * dir.x), abs(q.x * dir.y + q.y * dir.x));
    let rc = length(q);
    let diag = band(dg, PARK_DIAG, fi) * step(0.0, dl);
    let plaza = 1.0 - past(rc, PARK_PLAZA, fi);
    let path = max(max(loop_path, diag), plaza);
    // Worn grass where people cut the corners.
    col = mix(col, vec3(0.2, 0.17, 0.11), 0.45 * band(dl, 2.8, fi) * (1.0 - loop_path));
    col = mix(col, vec3(0.4, 0.36, 0.29) * (0.85 + 0.3 * g.hi), path);
    if (rc < PARK_BEDS + fi && fi < 3.0) {
        // Flower beds round the plaza, a colour each.
        let ang = heading(q);
        let fh = hash13(vec3(floor((ang + PI) / (PI / 6.0)), seed, 5.0));
        let flower = mix(mix(vec3(0.5, 0.06, 0.05), vec3(0.62, 0.48, 0.05), step(0.33, fh)), vec3(0.28, 0.1, 0.38),
            step(0.66, fh));
        // (The beds' arcs at no less than the ring's radius: a period shrinking to 0 at the middle
        // would make NaNs there, which no zero weight can cancel.)
        let rb = max(rc, PARK_PLAZA);
        let beds = span(rc, PARK_PLAZA + 0.8, PARK_BEDS, fi) * stripes((ang + PI) * rb, PI / 6.0 * rb, PI / 6.0 * rb - 1.5, fi);
        col = mix(col, mix(vec3(0.05, 0.1, 0.03), flower, smoothstep(0.35, 0.75, g.mid)), beds * fade(fi, 2.0));
    }
    var out = mk(col, mix(0.95, 0.9, path));
    out.wet = path * smoothstep(0.7, 0.85, g.lo) * 0.8;
    out.height = 6.0;
    return out;
}

// A plaza: slabs in a diagonal checker, rings round its monument, a band round it (the line colour
// on the civic plazas, glowing), eight lamps on a ring.
fn paint_plaza(cell: Cell, district: i32, strip: i32, lit: bool, g: Grain, fp: vec2<f32>) -> Paint {
    let p = cell.p;
    let r = cell.rect;
    let fi = max(fp.x, fp.y);
    let q = p - vec2(0.5 * (r.x + r.y), 0.5 * (r.z + r.w));
    let rc = length(q);
    let civic = district == 1 || district == 2;
    var base = select(vec3(0.36, 0.345, 0.32), vec3(0.46, 0.455, 0.44), civic);
    if (district == 5) {
        base = vec3(0.34, 0.2, 0.14);
    }
    let dq = vec2(q.x + q.y, q.y - q.x) * 0.70710678;
    let fd = vec2(0.70710678 * (fp.x + fp.y));
    let cq = floor(dq / 1.8);
    let checker = mix(0.5, (cq.x + cq.y) - 2.0 * floor((cq.x + cq.y) * 0.5), fade(fd.x, 1.8));
    var col = base * (0.88 + 0.2 * checker) * (1.0 - 0.4 * joints(dq, vec2(1.8), 0.02, fd));
    // From the ring at 6 m to the one at 36 m (the cuts fall between rings, not across one).
    let rings = lines(rc, 6.0, 0.35, fi) * step(3.0, rc) * (1.0 - past(rc, 39.0, fi));
    col = mix(col, base * 0.55, rings);
    let ring_band = span(rc, 7.0, 9.0, fi);
    col = mix(col, select(base * 0.6, line_colour(strip) * 0.8, civic), ring_band);
    col *= 1.0 - 0.15 * smoothstep(0.5, 0.85, g.lo);
    var out = mk(col, select(0.75, 0.5, civic));
    out.glow = select(vec3(0.0), line_colour(strip) * band(rc - 8.0, 0.12, fi), civic);
    out.wet = 0.5 * smoothstep(0.74, 0.86, g.lo);
    if (lit && abs(rc - PLAZA_RING) < 20.0 + fi) {
        let gap = 2.0 * PI * PLAZA_RING / PLAZA_LAMPS;
        let um = (heading(q) + PI) * PLAZA_RING;
        out.lamps = PLAZA_GAIN * lamps_across(rc - PLAZA_RING, LAMP_CORE, fi)
            * lamps_periodic(um - gap * floor(um / gap), gap, LAMP_CORE, fi);
    }
    return out;
}

// The canal's quays (the channel itself is water): granite coping at the water's edge, a yellow
// line, mooring bollards, setts on the Canal strip (concrete elsewhere) with a quay crane's rails,
// a row of tree pits, paving to the street (the lamps along the edge are `lamps_near`'s).
fn paint_canal(cell: Cell, strip: i32, wear: f32, g: Grain, fp: vec2<f32>) -> Paint {
    let p = cell.p;
    let r = cell.rect;
    let fv = fp.x;
    let fu = fp.y;
    let fi = max(fv, fu);
    let y = abs(p.x - 0.5 * (r.x + r.y)) - CANAL_WIDTH * 0.5;
    let u = p.y - r.z;
    let working = strip == 1;
    let street_side = y > 13.0;
    var base = vec3(0.33, 0.325, 0.31);
    var size = vec2(1.5, 1.5);
    var jw = 0.02;
    if (street_side) {
        base = vec3(0.34, 0.335, 0.32);
        size = vec2(0.6, 0.6);
        jw = 0.012;
    } else if (working) {
        base = vec3(0.24, 0.225, 0.21);
        size = vec2(0.3, 0.2);
        jw = 0.03;
    }
    let id = select(52.0, 51.0, working) + select(0.0, 2.0, street_side);
    var col = flags(vec2(u, y), size, base, jw, vec2(fu, fv), id);
    if (working) {
        // A quay crane's rails, rust round them.
        col = mix(col, vec3(0.2, 0.1, 0.05), 0.6 * (span(y, 2.8, 3.2, fv) + span(y, 8.8, 9.2, fv)));
        col = mix(col, vec3(0.45, 0.4, 0.36), band(y - 3.0, 0.04, fv) + band(y - 9.0, 0.04, fv));
    }
    // A row of trees along the quay, a pit each (as the avenue's), none within TREE_END of its ends.
    let qkeep = past(min(u, (r.w - r.z) - u), TREE_END, fu);
    let qpit = qkeep * stripes(u - (TREE_FIRST - 0.8), TREE_PITCH, 1.6, fu) * span(y, QUAY_TREE - 0.8, QUAY_TREE + 0.8, fv);
    let qpit_in = qkeep * stripes(u - (TREE_FIRST - 0.68), TREE_PITCH, 1.36, fu)
        * span(y, QUAY_TREE - 0.68, QUAY_TREE + 0.68, fv);
    var qcol = vec3(0.1, 0.09, 0.08);
    if (strip == 1) {
        qcol = vec3(0.3, 0.27, 0.23) * (0.8 + 0.3 * g.hi);
    } else if (strip == 2) {
        qcol = vec3(0.08, 0.15, 0.05) * (0.8 + 0.4 * g.hi);
    }
    col = mix(col, vec3(0.44, 0.43, 0.41), qpit - qpit_in);
    col = mix(col, qcol, qpit_in);
    col *= 1.0 - 0.3 * wear * smoothstep(0.4, 0.85, g.mid);
    let coping = 1.0 - past(y, 1.2, fv);
    col = mix(col, vec3(0.44, 0.435, 0.42) * (1.0 - 0.4 * lines(u, 1.5, 0.02, fu)) * (0.9 + 0.2 * g.hi), coping);
    col = mix(col, vec3(0.6, 0.45, 0.06), band(y - 1.4, 0.06, fv) * select(0.6, 1.0, working));
    let bm = u - 18.0 * floor(u / 18.0) - 9.0;
    let bollard = band(length(vec2(bm, y - 0.75)), 0.22, fi);
    col = mix(col, select(vec3(0.08), vec3(0.17, 0.07, 0.03), working), bollard);
    col = mix(col, vec3(0.12, 0.12, 0.13), 1.0 - past(y, 0.3, fv));
    var out = mk(col, select(0.85, 0.75, working));
    out.wet = clamp(0.6 * coping * g.mid + select(0.0, 0.2, working) * (1.0 - past(y, 13.0, fv))
        + 0.5 * smoothstep(0.74, 0.86, g.lo), 0.0, 1.0);
    return out;
}

// The building site: soil and gravel, haul roads rutted by the trucks (water standing in the
// ruts), concrete pads poured for the frames; the work lights' pools by night.
fn paint_site(cell: Cell, seed: f32, height: f32, lit: bool, g: Grain, fp: vec2<f32>) -> Paint {
    let q = cell.p - cell.rect.xz;
    var col = vec3(0.22, 0.16, 0.1) * (0.75 + 0.4 * g.lo) * (0.85 + 0.3 * g.mid);
    let gravel = smoothstep(0.55, 0.66, 0.6 * (1.0 - g.lo) + 0.4 * g.mid);
    col = mix(col, vec3(0.3, 0.28, 0.25) * (0.85 + 0.3 * g.hi), gravel);
    let haul = lines(q.x - 16.0, 32.0, 4.0, fp.x);
    let ruts = min(lines(q.x - 16.95, 32.0, 0.56, fp.x) + lines(q.x - 15.05, 32.0, 0.56, fp.x), 1.0);
    col = mix(col, vec3(0.26, 0.22, 0.17), 0.6 * haul);
    col = mix(col, vec3(0.12, 0.09, 0.06), 0.8 * ruts);
    let cp = floor(q / 32.0);
    let h = hash13(vec3(cp, seed + 4.0));
    let lo = cp * 32.0 + 3.0 + 4.0 * vec2(fract(h * 7.0), fract(h * 5.0));
    let hi = cp * 32.0 + 29.0 - 4.0 * vec2(fract(h * 11.0), fract(h * 3.0));
    let pad = step(h, 0.3) * span(q.x, lo.x, hi.x, fp.x) * span(q.y, lo.y, hi.y, fp.y);
    col = mix(col, vec3(0.36, 0.35, 0.33) * (1.0 - 0.3 * joints(q, vec2(6.0), 0.03, fp)), pad);
    var out = mk(col, 0.97 - 0.2 * pad);
    out.wet = clamp(ruts + smoothstep(0.62, 0.75, g.mid) * (1.0 - pad), 0.0, 1.0);
    out.height = height;
    if (lit) {
        out.lamps = floods(q, seed, fp);
    }
    return out;
}

// The ground between a block's buildings, seen at the street (or past the near chunks, where no
// kerb stands on it): a tower's forecourt; yards by district.
fn paint_yard(cell: Cell, kind: i32, district: i32, seed: f32, wear: f32, g: Grain, fp: vec2<f32>) -> Paint {
    let q = cell.p - cell.rect.xz;
    let h = hash13(vec3(floor(q / 32.0), seed + 51.0));
    let tower = kind == 7;
    // Flags: a tower's forecourt, the civic districts', the Old Town's setts, the works' slabs.
    var paved = tower;
    var base = vec3(0.4, 0.395, 0.38);
    var size = vec2(1.5, 1.5);
    var jw = 0.008;
    var rough = 0.88;
    var wet = 0.0;
    if (!tower) {
        switch district {
            case 1, 2: {
                paved = true;
                base = vec3(0.42, 0.41, 0.395);
                size = vec2(1.2, 1.2);
                jw = 0.01;
                rough = 0.6;
            }
            case 5: {
                paved = true;
                base = vec3(0.25, 0.23, 0.21);
                size = vec2(0.3, 0.2);
                jw = 0.03;
                wet = 0.2;
            }
            case 7: {
                paved = true;
                base = vec3(0.3, 0.295, 0.28);
                size = vec2(6.0, 6.0);
                jw = 0.04;
                wet = 0.5;
            }
            default: {}
        }
    }
    var col = vec3(0.31, 0.305, 0.29) * (0.85 + 0.3 * g.mid);
    if (paved) {
        col = flags(q.yx, size, base, jw, fp.yx, 57.0 + f32(district));
    }
    if (tower) {
        col = mix(col, vec3(0.2, 0.2, 0.21), 0.8 * max(lines(q.x, 6.0, 0.5, fp.x), lines(q.y, 6.0, 0.5, fp.y)));
        rough = 0.4;
    } else if (district == 4 || district == 8) {
        // Gardens behind the houses, and paved yards.
        if (h < 0.6) {
            col = grass(g);
            rough = 0.95;
        }
    } else if (district == 6) {
        // The colleges' lawns, and their paths.
        col = mix(grass(g), vec3(0.4, 0.36, 0.29), max(lines(q.x, 32.0, 2.5, fp.x), lines(q.y, 32.0, 2.5, fp.y)));
        rough = 0.93;
    } else if (district == 7) {
        // Oil and rust on the works' yards.
        col *= 1.0 - 0.22 * smoothstep(0.5, 0.85, g.mid);
        col = mix(col, vec3(0.3, 0.13, 0.05), 0.15 * smoothstep(0.65, 0.9, g.lo));
    } else if (district == 9) {
        // The port's yards: asphalt, its bays marked out for containers.
        col = vec3(0.12, 0.12, 0.125) * (0.85 + 0.3 * g.mid);
        let bays = max(lines(q.y, 12.5, 0.12, fp.y), lines(q.x, 2.6, 0.12, fp.x));
        col = mix(col, vec3(0.6, 0.45, 0.06), 0.8 * bays * (1.0 - 0.5 * wear));
        wet = 0.4;
    } else if (!paved && h < 0.5) {
        col = vec3(0.12, 0.12, 0.125) * (0.85 + 0.3 * g.mid);
    }
    col *= 1.0 - 0.14 * wear * smoothstep(0.45, 0.85, g.lo);
    var out = mk(col, rough);
    out.wet = select(wet * smoothstep(0.6, 0.8, g.lo), 0.4 * smoothstep(0.76, 0.88, g.lo), tower);
    return out;
}

// A block's roofs from afar (through the windows from outside), lit windows by night.
fn roofs(cell: Cell, kind: i32, height: f32, seed: f32, g: Grain, fi: f32) -> Paint {
    let p = cell.p;
    let q = (p - cell.rect.xz) / BLOCK;
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
    roof = mix(vec3(0.37, 0.38, 0.39) * 0.6, roof, smoothstep(0.0, 0.04, gap));
    let lit_windows = mix(step(0.5, hash13(vec3(floor(p * 0.4), seed))), 0.5, smoothstep(1.2, 5.0, fi));
    var out = mk(roof * (0.85 + 0.25 * g.mid), 0.8);
    out.lamps = lit_windows * clamp(height / 40.0, 0.3, 1.0);
    out.height = height;
    return out;
}

// What a block looks like from its atlas texel (bytes as 0..1: kind, district, height in 2 m steps,
// seed), on strip `strip`, through a pixel covering `fp` metres (across, along). From afar (through
// the windows from outside) its buildings are roofs; from inside the colony, where the buildings
// stand on it, the ground in a block is its yards and pavements. Its streets, the avenue, the banks
// and Hub Gate's square are painted from the cell's place on the grid, with their lamps' light
// when `lit` (the lamps are on: their light is worked out only then).
fn city_paint(cell: Cell, t: vec4<f32>, strip: i32, from_afar: bool, fp: vec2<f32>, lit: bool) -> Paint {
    let kind = i32(t.r * 255.0 + 0.5);
    let district = i32(t.g * 255.0 + 0.5);
    let height = t.b * 255.0 * 2.0;
    let seed = t.a * 255.0;
    let p = cell.p;
    let f = max(fp, vec2(1e-3));
    let g = grain_at(p, max(f.x, f.y));
    let st = streets_at(cell);
    let wear = wear_of(district, strip);
    let d = inside(cell.rect, p);
    let has_block = abs(cell.row) >= 1 && abs(cell.row) <= ROWS && kind != 0;
    var out = mk(vec3(0.3), 0.9);
    var road = no_road();
    // The city's lamp rows light it here (the colony's own ground has its own).
    var city_lamps = true;
    if (cell.row == 0) {
        if (cell.bx < CITY_START || cell.bx >= FOOT_START) {
            out = paint_square(p, strip, lit, g, f);
            city_lamps = false;
        } else {
            // The avenue's wear (its row has no district in the atlas): by the strip.
            let a = abs(s_of_avenue(p.x));
            let avenue_wear = select(select(0.45, 0.6, strip == 1), 0.3, strip == 0);
            road = avenue_road(p, cell.bx, st, avenue_wear, f);
            if (road.hw == 0.0) {
                let e = abs(st.dx) - st.hx;
                let u = p.y - st.seg_x.x;
                if (a < MEDIAN * 0.5) {
                    out = median(a, u, e, strip, g, f);
                } else {
                    out = avenue_pavement(a, u, e, strip, avenue_wear, g, f);
                }
            }
        }
    } else if (abs(cell.row) >= BANK_ROW) {
        out = paint_bank(p, strip, lit, g, f);
        // (The bank road, and its lamps, end with row 12's blocks.)
        city_lamps = cell.bx < FOOT_START;
    } else if (!has_block) {
        out = paint_square(p, strip, lit, g, f);
        city_lamps = false;
    } else if (d < 0.0) {
        road = street_road(cell, st, kind, district, wear, f);
    } else if (kind == 4 && abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) < CANAL_WIDTH * 0.5) {
        // The canal's water (from afar: inside, its chunk draws it and the ground leaves it open).
        out = mk(vec3(0.03, 0.06, 0.07), 0.04);
        out.wet = 1.0;
        return out;
    } else if (d < SIDEWALK && (kind != 4 || abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) > CANAL_WIDTH * 0.5 + 13.0)) {
        // (A quay's pavement only on its street side: its coping runs on to the bridges.)
        out = paint_sidewalk(cell, d, district, strip, wear, g, f);
    } else if (kind == 2) {
        out = paint_park(cell, seed, g, f);
    } else if (kind == 3) {
        out = paint_plaza(cell, district, strip, lit, g, f);
    } else if (kind == 4) {
        out = paint_canal(cell, strip, wear, g, f);
    } else if (kind == 5) {
        out = paint_site(cell, seed, height, lit, g, f);
    } else if (from_afar) {
        return roofs(cell, kind, height, seed, g, max(f.x, f.y));
    } else {
        out = paint_yard(cell, kind, district, seed, wear, g, f);
    }
    if (road.hw > 0.0) {
        out = road_over(paint_road(road, p, g), road, cell, st);
    }
    if (lit && city_lamps) {
        out.lamps += lamps_near(cell, st, kind, seed, f);
    }
    if (from_afar) {
        out.lamps *= luma(out.albedo);
    }
    return out;
}

// The ground sketched: `city_paint`'s colours and its main lines, without the detail or the wear,
// and its lamps as their rows' average light (lines of light, no pools). For the Low tier and the
// city seen from outside through the windows. A software rasteriser (SwiftShader) runs every branch
// of a shader for every pixel, so what a pixel costs there is the whole shader's size: this stays
// about the size of the paint before the detail came.
fn city_sketch(cell: Cell, t: vec4<f32>, strip: i32, from_afar: bool, fp: vec2<f32>, lit: bool) -> Paint {
    let kind = i32(t.r * 255.0 + 0.5);
    let district = i32(t.g * 255.0 + 0.5);
    let height = t.b * 255.0 * 2.0;
    let seed = t.a * 255.0;
    let p = cell.p;
    let f = max(fp, vec2(1e-3));
    let fi = max(f.x, f.y);
    let st = streets_at(cell);
    let c = s_of_avenue(p.x);
    let a = abs(c);
    let d = inside(cell.rect, p);
    let n = nz(p, 0.06, 1.0);
    let accent = line_colour(strip);
    let has_block = abs(cell.row) >= 1 && abs(cell.row) <= ROWS && kind != 0;
    let square = (cell.row == 0 && (cell.bx < CITY_START || cell.bx >= FOOT_START))
        || (abs(cell.row) >= 1 && abs(cell.row) <= ROWS && kind == 0);
    let asphalt = vec3(0.1, 0.1, 0.105) * (0.85 + 0.3 * n);
    let paving = select(select(vec3(0.42, 0.41, 0.395), vec3(0.31, 0.17, 0.11), strip == 1), vec3(0.4, 0.355, 0.275),
        strip == 2);
    let green = mix(vec3(0.06, 0.13, 0.035), vec3(0.15, 0.17, 0.07), 1.0 - n);
    var col = paving * (0.9 + 0.2 * n);
    var rough = 0.85;
    var glow = vec3(0.0);
    var wet = 0.5 * smoothstep(0.68, 0.82, n);
    var h = 0.0;
    // A lamp row's light, spread along it: its pools' average.
    let line = LAMP_MEAN * LAMP_CORE * SQRT_TAU / LAMP_GAP;
    var lamps = 0.0;
    if (square) {
        // Hub Gate's square: pale stone, the darker bands, the line colour inlaid and lit, the lamps
        // down the inlaid bands; the tram's way through it. Measured from its cap, as `paint_square`
        // does (small numbers at the far foot too, and the same lines).
        let cap = GRID_X0 + f32(HUB_START) * BLOCK;
        let u = vec2(c, abs(p.y - select(-cap, cap, p.y < 0.0)));
        col = vec3(0.5, 0.495, 0.475) * (0.95 + 0.1 * n);
        let bands = max(lines(u.x - 16.0, 32.0, 1.6, f.x), lines(u.y - 16.0, 32.0, 1.6, f.y));
        col = mix(col, vec3(0.33, 0.33, 0.34), bands);
        let inlay = max(lines(u.x - 16.0, 64.0, 0.2, f.x), lines(u.y - 16.0, 64.0, 0.2, f.y));
        col = mix(col, accent, inlay);
        glow = accent * inlay;
        let gb = u - vec2(16.0) - 64.0 * floor((u - vec2(16.0)) / 64.0 + 0.5);
        lamps = 0.5 * (LAMP_CORE * SQRT_TAU / 16.0)
            * (lamps_across(gb.x, LAMP_CORE, f.x) + lamps_across(gb.y, LAMP_CORE, f.y));
        col = mix(col, vec3(0.3, 0.3, 0.31), select(0.0, 1.0 - past(a, MEDIAN * 0.5, f.x), cell.row == 0 && p.y < 0.0));
        rough = 0.5;
    } else if (cell.row == 0) {
        let e = abs(st.dx) - st.hx;
        if (e < 0.0 || (a >= MEDIAN * 0.5 && a < ROAD_OUT)) {
            // The carriageways and where the cross streets cross: asphalt, the lane lines (their
            // dashes' average), the cycle lane; the kerbs' stones.
            let y = a - MEDIAN * 0.5;
            col = asphalt;
            if (e >= 0.0) {
                col = mix(col, vec3(0.2, 0.085, 0.06), 0.85 * span(y, 11.95, 13.25, f.x));
                let marks = band(y - 0.825, 0.075, f.x) + band(y - 11.775, 0.075, f.x)
                    + (band(y - 4.5, 0.075, f.x) + band(y - 8.1, 0.075, f.x)) * 0.4;
                col = mix(col, vec3(0.6, 0.55, 0.45), min(marks, 1.0));
                col = mix(col, vec3(0.4, 0.395, 0.38), 1.0 - past(y, 0.3, f.x) + past(y, 13.7, f.x));
            }
        } else if (a < MEDIAN * 0.5) {
            // The median: ballast (grass in the Gardens) and rails, the light lines at its kerbs.
            col = select(vec3(0.25, 0.24, 0.225), vec3(0.07, 0.14, 0.04), strip == 2);
            col = mix(col, vec3(0.58, 0.58, 0.6), band(abs(a - TRACK_OFFSET) - 0.75, 0.036, f.x));
            col = mix(col, vec3(0.09, 0.16, 0.05), past(a, TRACK_OFFSET + 1.6, f.x));
            glow = accent * band(a - (MEDIAN * 0.5 - 0.45), 0.05, f.x);
        } else {
            // The pavements: the furniture zone's dark setts, the planting beds.
            let y = a - ROAD_OUT;
            col = mix(col, vec3(0.2, 0.195, 0.19), span(y, 0.3, 4.3, f.x));
            col = mix(col, vec3(0.07, 0.13, 0.04), 0.75 * span(y, 9.5, 11.5, f.x));
        }
    } else if (abs(cell.row) >= BANK_ROW) {
        // The bank: grass, a path, the promenade, the light strip along the glass.
        let edge = min(p.x, STRIP_WIDTH - p.x);
        col = mix(green, vec3(0.4, 0.36, 0.29), band(edge - 52.0, 1.5, f.x));
        let prom = select(select(vec3(0.46, 0.455, 0.44), vec3(0.33, 0.325, 0.31), strip == 1), vec3(0.32, 0.21, 0.13),
            strip == 2);
        col = mix(col, prom, 1.0 - past(edge, 12.0, f.x));
        col = mix(col, vec3(0.34, 0.34, 0.325), past(edge, 96.5, f.x));
        let strip_line = band(edge - 0.6, 0.12, f.x);
        col = mix(col, accent, strip_line);
        glow = accent * strip_line;
        lamps = 0.8 * (LAMP_CORE * SQRT_TAU / 25.0) * lamps_across(edge - 11.0, LAMP_CORE, f.x);
        rough = 0.9;
    } else if (d < 0.0) {
        // A street: asphalt, its centre line, its edge lines, the gutters.
        let along_x = st.k > 0 && abs(st.ds) < st.hs;
        let v = abs(select(st.dx, st.ds, along_x));
        let hw = select(st.hx, st.hs, along_x);
        let fv = select(f.y, f.x, along_x);
        col = asphalt * select(1.0, 2.6, district == 7 || district == 9);
        col = mix(col, vec3(0.6, 0.45, 0.15), band(v, 0.25, fv) * 0.7);
        // The edge lines where `paint_road` has them: 3.6 m out on the bank road, 7.5 m on a street,
        // 12.4 m on a wide one.
        let edge = select(select(7.5, 12.4, hw > 15.0), 3.6, hw < 11.0);
        col = mix(col, vec3(0.6, 0.58, 0.55), band(v - edge - 0.075, 0.075, fv));
        col = mix(col, vec3(0.25, 0.245, 0.235), 1.0 - past(hw - v, 0.45, fv));
        wet = max(wet, (1.0 - past(hw - v, 0.45, fv)) * 0.6 * n);
    } else if (kind == 4 && abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) < CANAL_WIDTH * 0.5) {
        col = vec3(0.03, 0.06, 0.07);
        rough = 0.04;
        wet = 1.0;
    } else if (d < SIDEWALK && (kind != 4 || abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) > CANAL_WIDTH * 0.5 + 13.0)) {
        col = mix(col * 0.85, vec3(0.4, 0.395, 0.38), 1.0 - past(d, 0.3, f.x + f.y));
    } else if (kind == 2) {
        // A park: grass, the loop of its path, its trees' shade.
        let inset = SIDEWALK + 6.0;
        let dl = inside(vec4(cell.rect.x + inset, cell.rect.y - inset, cell.rect.z + inset, cell.rect.w - inset), p);
        col = mix(green * (1.0 - 0.5 * smoothstep(0.6, 0.8, n)), vec3(0.4, 0.36, 0.29), band(dl, 1.5, fi));
        rough = 0.95;
        h = 6.0;
    } else if (kind == 3) {
        let m = vec2(0.5 * (cell.rect.x + cell.rect.y), 0.5 * (cell.rect.z + cell.rect.w));
        col = mix(col * 0.9, col * 0.6, lines(length(p - m), 6.0, 0.35, fi));
    } else if (kind == 4) {
        col = mix(vec3(0.33, 0.325, 0.31), vec3(0.44, 0.435, 0.42),
            1.0 - past(abs(p.x - 0.5 * (cell.rect.x + cell.rect.y)) - CANAL_WIDTH * 0.5, 1.2, f.x));
    } else if (kind == 5) {
        col = mix(vec3(0.22, 0.16, 0.1), vec3(0.3, 0.28, 0.25), smoothstep(0.4, 0.7, n));
        rough = 0.97;
        h = height;
    } else if (from_afar) {
        return roofs(cell, kind, height, seed, Grain(n, n, 0.5), fi);
    } else if (district == 4 || district == 6 || district == 8) {
        col = green;
        rough = 0.95;
    } else if (district == 7 || district == 9) {
        col = vec3(0.3, 0.295, 0.28) * (0.8 + 0.3 * n);
    }
    if (lit && !square && abs(cell.row) < BANK_ROW) {
        // The rows along both kerbs of the streets round here, and the avenue's.
        if (st.k > 0) {
            lamps += line * (lamps_across(abs(st.ds) - st.hs + LAMP_OUT, LAMP_CORE, f.x)
                + lamps_across(abs(st.ds) + st.hs - LAMP_OUT, LAMP_CORE, f.x));
        } else {
            lamps += line * (lamps_across(a - AVENUE_LAMP, LAMP_CORE, f.x) + 0.55 * lamps_across(c, LAMP_CORE, f.x));
        }
        if (cell.row != 0) {
            lamps += line * (lamps_across(abs(st.dx) - st.hx + LAMP_OUT, LAMP_CORE, f.y)
                + lamps_across(abs(st.dx) + st.hx - LAMP_OUT, LAMP_CORE, f.y));
        }
    } else if (lit && abs(cell.row) >= BANK_ROW) {
        lamps += line * lamps_across(abs(st.ds) - st.hs + LAMP_OUT, LAMP_CORE, f.x);
    }
    var out = mk(col, rough);
    out.lamps = select(0.0, lamps, lit);
    out.height = h;
    out.wet = wet;
    out.glow = glow;
    if (from_afar) {
        out.lamps *= luma(col);
    }
    return out;
}

// Across from the avenue's centre line, for a point `s` of the strip.
fn s_of_avenue(s: f32) -> f32 {
    return s - STRIP_WIDTH * 0.5;
}
