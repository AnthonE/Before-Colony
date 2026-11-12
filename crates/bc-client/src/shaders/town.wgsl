// The ring's town (`TownMaterial`, `bc_client_core::town_mesh`; `docs/FREE_ROAM.md`, V1): its
// buildings, streets, canal, parks, lifts, faces and ceiling, painted from what each face carries in
// its vertex colour, as the station language's are: r its surface, g its paint (a facade's district
// and tint, a road's class), b its flags (a facade's style), a a seed. UVs are metres.
//
// The station language at a town's scale: light blue-grey panels with fine joints, each quarter's own
// colour in its bands and fins, glass with a painted streak across it, yellow and white on the
// roads; lit like the anime (`bc::anime`), and washed out with distance in painted layers of haze.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import bc::anime::paint_bands

struct Town {
    // rgb: the palette, linear (`station::PAINT`).
    paint: array<vec4<f32>, 11>,
    // x: seconds; y: daylight, 0 to 1; z: lamps lit, 0 to 1; w: 1 to paint it in bands.
    day: vec4<f32>,
    // rgb: the haze, as it reaches the screen; w: its density, /m.
    haze: vec4<f32>,
    // rgb: the sky's light in the ceiling's glass, as it reaches the screen; w: the picture's gain
    // (the camera's grade opens the picture up before its tonemapper: this takes that back out, so
    // the town's paints come out the colours they're given).
    sky: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> town: Town;

// Surfaces (`town_mesh::Surface`).
const FACADE: u32 = 1u;
const ROOF: u32 = 2u;
const ROAD: u32 = 3u;
const JUNCTION: u32 = 4u;
const PAVEMENT: u32 = 5u;
const MEDIAN: u32 = 6u;
const WATER: u32 = 7u;
const GRASS: u32 = 8u;
const SQUARE: u32 = 9u;
const LOT: u32 = 10u;
const TRIM: u32 = 11u;
const PORTAL: u32 = 12u;
const GLOW: u32 = 13u;
const RIM: u32 = 14u;
const SKY: u32 = 15u;
const FOLIAGE: u32 = 16u;
const SITE: u32 = 17u;
const DOOR: u32 = 18u;

// A facade's flags: its style in the low bits, and a tower's.
const TOWER: u32 = 8u;
// A road's class (`town_mesh::Road`) and half its width, m.
const R_STREET: u32 = 0u;
const R_AVENUE: u32 = 1u;
const R_BOULEVARD: u32 = 2u;
const R_RIM: u32 = 3u;
const H_STREET: f32 = 10.0;
const H_AVENUE: f32 = 20.0;
const H_BOULEVARD: f32 = 12.0;
const H_RIM: f32 = 15.0;
// A building's storeys, m (`colony::town`).
const GROUND_FLOOR: f32 = 5.0;
const STOREY: f32 = 4.0;
// A cell round the ring, m, and the ceiling's frames across it; a parking bay along a kerb.
const CELL: f32 = 130.9;
const BAY: f32 = 6.2333;
const SKY_FRAME: f32 = 125.0;

// The palette's entries the town paints with (`station`).
const WHITE: u32 = 0u;
const YELLOW: u32 = 4u;
const DARK: u32 = 5u;
const PAINTS: u32 = 11u;

fn hash2(p: vec2<f32>) -> f32 {
    var q = fract(vec3(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// A pulse train filtered by a box: the share of a footprint `w` wide (in periods) about `x` that
// falls in [a, b] of each unit period. A grid seen from afar is its average, not moiré.
fn pulse(x: f32, a: f32, b: f32, w: f32) -> f32 {
    let f = fract(x);
    let h = 0.5 * max(w, 1e-3);
    let hi = floor(f + h) * (b - a) + clamp(fract(f + h), a, b);
    let lo = floor(f - h) * (b - a) + clamp(fract(f - h), a, b);
    return clamp((hi - lo) / (2.0 * h), 0.0, 1.0);
}

// One band [a, b], filtered the same way.
fn band(x: f32, a: f32, b: f32, w: f32) -> f32 {
    let h = 0.5 * max(w, 1e-4);
    return (clamp(x + h, a, b) - clamp(x - h, a, b)) / (2.0 * h);
}

// A grid's joints: cells `size` across, joints `joint` wide.
fn joints(uv: vec2<f32>, size: vec2<f32>, joint: f32, fw: vec2<f32>) -> f32 {
    let x = pulse(uv.x / size.x, 0.0, joint / size.x, fw.x / size.x);
    let y = pulse(uv.y / size.y, 0.0, joint / size.y, fw.y / size.y);
    return max(x, y);
}

// Hazard stripes, diagonal: 1 yellow, 0 black.
fn stripes(uv: vec2<f32>, fw: vec2<f32>) -> f32 {
    return 1.0 - pulse((uv.x + uv.y) / 1.2, 0.0, 0.5, (fw.x + fw.y) / 1.2);
}

// A light colour from a paint: its brightest channel 1.
fn as_light(c: vec3<f32>) -> vec3<f32> {
    return c / max(max(c.r, c.g), max(c.b, 1e-3));
}

// A quarter's own colour (Charter blue, Canal teal, Gardens green; the Site's safety yellow).
fn quarter_paint(q: u32) -> vec3<f32> {
    if (q == 3u) {
        return town.paint[YELLOW].rgb;
    }
    return town.paint[1u + min(q, 2u)].rgb;
}

// A district's panels: the station's light blue-grey, warm sand, a deeper blue-grey or a rose clay by
// its tint, each with a little of its quarter's colour in it.
fn panel_paint(paint: u32) -> vec3<f32> {
    var tints = array<vec3<f32>, 4>(
        vec3(0.47, 0.52, 0.60),
        vec3(0.55, 0.50, 0.42),
        vec3(0.36, 0.44, 0.55),
        vec3(0.52, 0.42, 0.38),
    );
    return mix(tints[paint & 3u], quarter_paint(paint / 4u), 0.07);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    var surface = 0u;
    var paint_i = 0u;
    var flags = 0u;
    var seed = 0.0;
#ifdef VERTEX_COLORS
    surface = u32(in.color.r * 255.0 + 0.5);
    paint_i = u32(in.color.g * 255.0 + 0.5);
    flags = u32(in.color.b * 255.0 + 0.5);
    seed = floor(in.color.a * 255.0 + 0.5);
#endif
    var uv = vec2(0.0);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    // Every derivative here, where control flow is still uniform: below, the shader branches on
    // the face's surface, and WebGPU rejects derivatives under such a branch.
    let fw = max(fwidth(uv), vec2(1e-4));
    let fm = max(fw.x, fw.y);

    let time = town.day.x;
    let daylight = town.day.y;
    let lamps = town.day.z;
    let white = town.paint[WHITE].rgb;
    let yellow = town.paint[YELLOW].rgb;
    let dark = town.paint[DARK].rgb;
    let ink = town.paint[min(paint_i, PAINTS - 1u)].rgb;
    let quarter = quarter_paint(paint_i / 4u);
    let u = uv.x;
    let v = uv.y;
    var albedo = white;
    var rough = 0.5;
    var glow = vec3(0.0);

    if (surface == FACADE) {
        let style = flags & 7u;
        let tower = (flags & TOWER) != 0u;
        // By style: a bay's width, a window's margin either side in it, its sill and its head over
        // the storey's floor (m). Civic, the money's podium, midtown, homes, Old Town, sheds.
        var bays = array<f32, 6>(6.0, 3.0, 4.0, 3.6, 2.6, 8.0);
        var margins = array<f32, 6>(1.7, 0.12, 0.7, 0.8, 0.6, 1.2);
        var sills = array<f32, 6>(0.7, 1.0, 1.0, 1.0, 1.1, 2.5);
        var heads = array<f32, 6>(3.5, 3.2, 3.0, 2.8, 2.9, 3.3);
        let st = min(style, 5u);
        let bay = select(bays[st], 1.5, tower);
        let margin = select(margins[st], 0.06, tower);
        let sill = select(sills[st], 0.75, tower);
        let head = select(heads[st], 3.9, tower);
        let shade = 0.94 + 0.12 * hash2(vec2(seed, 7.0));
        let panel = panel_paint(paint_i) * shade;
        let bay_i = floor(u / bay);
        let bu = u - bay_i * bay;
        let up = v - GROUND_FLOOR;
        let storey_i = floor(up / STOREY);
        let sv = up - storey_i * STOREY;
        let above = step(GROUND_FLOOR, v);
        // The upper storeys: panels with joints at every bay and storey, and a window in each.
        albedo = panel * (1.0 - 0.3 * joints(vec2(u, up), vec2(bay, STOREY), 0.06, fw));
        albedo *= 1.0 - 0.12 * joints(vec2(u, up), vec2(bay * 0.5, STOREY * 0.5), 0.03, fw);
        var window = band(bu, margin, bay - margin, fw.x) * band(sv, sill, head, fw.y) * above;
        // Sheds have a window only in some bays; a civic hall's are tall between its piers.
        let h_bay = hash2(vec2(bay_i, seed));
        if (st == 5u) {
            window *= step(0.45, h_bay);
            // Ribbed cladding.
            albedo *= 1.0 - 0.1 * pulse(u / 0.6, 0.0, 0.5, fw.x / 0.6);
        }
        // Homes: a balcony's front under every other bay's window, in the quarter's colour.
        if (st == 3u) {
            let balcony = band(bu, margin - 0.4, bay - margin + 0.4, fw.x) * band(sv, sill - 0.9, sill - 0.05, fw.y)
                * step(0.5, fract(bay_i * 0.5 + storey_i * 0.5)) * above;
            albedo = mix(albedo, mix(panel * 0.8, quarter, 0.55), balcony);
        }
        // A tower's fins every fourth bay, and the money's podium's every bay, in the quarter's colour.
        let fins = pulse(u / (bay * 4.0), 0.0, 0.2 / (bay * 4.0), fw.x / (bay * 4.0)) * above;
        if (tower || st == 2u) {
            albedo = mix(albedo, quarter * 0.9, fins * 0.85);
            window *= 1.0 - fins;
        }
        // The ground floor: a dark base course, glazing between piers (a shed's doors, a hall's
        // porch), and a fascia over it in the quarter's colour.
        let gbay = max(bay, 3.0) * select(1.0, 2.0, bay < 2.0);
        let gi = floor(u / gbay);
        let gu = u - gi * gbay;
        let h_g = hash2(vec2(gi, seed + 3.0));
        var front = band(gu, 0.35, gbay - 0.35, fw.x) * band(v, 0.5, 3.7, fw.y) * (1.0 - above);
        var door_dark = 0.0;
        if (st == 5u) {
            // A roller door in some bays, ribbed, hazard stripes at its foot; else a blank wall.
            front *= step(0.55, h_g);
            door_dark = front;
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(v, -1.0, 0.6, fw.y) * (1.0 - above));
        } else if (st == 3u || st == 4u) {
            // Homes and Old Town: a door in every third bay, windows in the rest.
            let is_door = step(h_g, 0.34);
            door_dark = front * is_door * band(gu, gbay * 0.3, gbay * 0.7, fw.x) * band(v, 0.0, 2.6, fw.y);
            front *= (1.0 - is_door) * band(v, 1.1, 3.2, fw.y);
        }
        let fascia = band(v, 4.15, 4.8, fw.y);
        albedo = mix(albedo, panel * 0.35, band(v, -1.0, 0.45, fw.y));
        albedo = mix(albedo, mix(quarter, panel, select(0.25, 0.75, st == 0u)), fascia);
        if (st == 5u) {
            albedo = mix(albedo, vec3(0.2, 0.21, 0.23) * (1.0 - 0.35 * pulse(v / 0.35, 0.0, 0.3, fw.y / 0.35)), door_dark);
        } else {
            albedo = mix(albedo, dark * 3.0, door_dark);
            window = max(window, front * (1.0 - door_dark));
        }
        // Glass: a painted sky in it, a streak across each pane; lit from inside by night.
        let pane = vec2((bu - margin) / max(bay - 2.0 * margin, 0.1), (sv - sill) / max(head - sill, 0.1));
        let d = pane.x * 0.6 + pane.y;
        let streak = (band(d, 0.95, 1.12, fm) + band(d, 1.2, 1.26, fm)) * above;
        var glass = mix(vec3(0.02, 0.05, 0.13), vec3(0.09, 0.22, 0.42), clamp(pane.y, 0.0, 1.0) * above + 0.25 * (1.0 - above));
        glass = mix(glass, vec3(0.42, 0.6, 0.78), streak * 0.5) * (0.25 + 0.75 * daylight);
        // Mullions.
        let mull = select(1.5, 3.0, st == 0u || st == 5u);
        glass *= 1.0 - 0.6 * pulse(u / mull, 0.0, 0.08 / mull, fw.x / mull);
        albedo = mix(albedo, glass, window);
        rough = mix(0.55, 0.12, window);
        // Who's in: a share of the windows by the lamps' hour, each its own warmth.
        let h_w = hash2(vec2(bay_i + storey_i * 17.0, seed + storey_i));
        let share = select(0.42, 0.7, st == 1u && !tower);
        let lit = step(h_w, share * lamps) * window * lamps;
        let warm = mix(vec3(1.0, 0.72, 0.38), vec3(0.75, 0.9, 1.0), step(0.8, fract(h_w * 13.0)));
        glow = warm * lit * (0.5 + 0.5 * above) * 1.8;
    } else if (surface == ROOF) {
        let cell = floor(uv / 4.0);
        let h = hash2(cell + vec2(seed));
        albedo = panel_paint(paint_i) * 0.5 * (0.92 + 0.16 * (h - 0.5));
        albedo *= 1.0 - 0.35 * joints(uv, vec2(4.0), 0.08, fw);
        // A plant unit here and there.
        let local = uv - cell * 4.0;
        let plant = band(local.x, 0.6, 3.4, fw.x) * band(local.y, 0.6, 3.4, fw.y) * step(h, 0.07);
        albedo = mix(albedo, vec3(0.1, 0.105, 0.115), plant);
        rough = 0.7;
    } else if (surface == ROAD || surface == JUNCTION) {
        // Dark deck plates, each a shade its own.
        let cell = floor(uv / 4.0);
        albedo = vec3(0.052, 0.054, 0.06) * (0.94 + 0.14 * (hash2(cell) - 0.5));
        albedo *= 1.0 - 0.25 * joints(uv, vec2(4.0), 0.05, fw);
        rough = 0.72;
        if (surface == ROAD) {
            var halves = array<f32, 4>(H_STREET, H_AVENUE, H_BOULEVARD, H_RIM);
            var lanes = array<f32, 4>(3.5, 3.5, 6.0, 3.75);
            var counts = array<f32, 4>(1.0, 3.0, 1.0, 2.0);
            let cls = min(paint_i, 3u);
            let half = halves[cls];
            let lane = lanes[cls];
            let one_way = cls == R_BOULEVARD;
            let au = abs(u);
            let paint_white = vec3(0.5, 0.52, 0.54);
            // The lanes' dashes: a line every lane's width out from the middle (a one-way's middle is
            // one too), 4 m of paint in every 12.
            let k = floor(au / lane + 0.5);
            let on_line = (k >= select(1.0, 0.0, one_way)) && (k <= counts[cls]);
            let dash = band(abs(au - k * lane), -1.0, 0.08, fw.x) * pulse(v / 12.0, 0.0, 0.34, fw.y / 12.0);
            albedo = mix(albedo, paint_white, dash * select(0.0, 1.0, on_line));
            // Two ways: a double line down the middle, in yellow.
            let centre = band(au, 0.1, 0.24, fw.x) * select(1.0, 0.0, one_way);
            albedo = mix(albedo, yellow * 0.9, centre);
            // The edge line, and the kerb outside it; on the streets, parking bays against it.
            let parked = (cls == R_STREET || cls == R_AVENUE);
            let inner = half - 2.7;
            if (parked) {
                let bay_line = band(au, inner - 0.06, inner + 0.06, fw.x);
                let ticks = band(au, inner, half - 0.5, fw.x) * pulse(v / BAY, 0.0, 0.02, fw.y / BAY);
                albedo = mix(albedo, paint_white, max(bay_line, ticks) * 0.85);
            }
            albedo = mix(albedo, paint_white, band(au, half - 0.62, half - 0.5, fw.x) * select(1.0, 0.0, parked));
            albedo = mix(albedo, vec3(0.3, 0.31, 0.33), band(au, half - 0.3, half + 1.0, fw.x));
        }
    } else if (surface == PAVEMENT || surface == LOT) {
        // Paving, lighter than the road: a quay's or a pavement's slabs, the rim walk's wider ones
        // with a guide strip, a lot's plain plates.
        let walk = surface == PAVEMENT && paint_i == 1u;
        let size = select(2.0, 3.0, walk || surface == LOT);
        let cell = floor(uv / size);
        var base = select(vec3(0.25, 0.26, 0.28), vec3(0.17, 0.18, 0.195), surface == LOT);
        base = select(base, vec3(0.3, 0.275, 0.24), walk);
        albedo = base * (0.94 + 0.12 * (hash2(cell + vec2(3.0)) - 0.5));
        albedo *= 1.0 - 0.3 * joints(uv, vec2(size), 0.04, fw);
        if (walk) {
            albedo = mix(albedo, yellow * 0.8, pulse(u / 30.0, 0.49, 0.5, fw.x / 30.0));
        }
        rough = 0.75;
    } else if (surface == MEDIAN) {
        // The tram's two tracks on dark ballast, a verge either side.
        let au = abs(u);
        let track = abs(au - 2.4);
        albedo = vec3(0.04, 0.04, 0.042) * (0.9 + 0.2 * hash2(floor(uv * 3.0)));
        let sleepers = pulse(v / 0.75, 0.0, 0.34, fw.y / 0.75) * band(track, -1.0, 1.15, fw.x);
        albedo = mix(albedo, vec3(0.13, 0.13, 0.135), sleepers);
        let rails = band(abs(track - 0.72), -1.0, 0.045, fw.x);
        albedo = mix(albedo, vec3(0.45, 0.48, 0.52), rails);
        let verge = band(au, 5.6, 9.0, fw.x);
        albedo = mix(albedo, vec3(0.06, 0.19, 0.05) * (0.9 + 0.2 * hash2(floor(uv * 0.7))), verge);
        albedo = mix(albedo, vec3(0.3, 0.31, 0.33), band(au, 5.35, 5.6, fw.x) + band(au, 7.75, 8.5, fw.x));
        rough = mix(0.8, 0.3, rails);
    } else if (surface == WATER) {
        // The canal: flat teal, painted glints sliding along it.
        let w1 = sin(v * 0.55 + time * 0.8 + sin(u * 0.45) * 1.6);
        let w2 = sin(u * 1.1 - time * 0.35 + v * 0.13);
        let glint = smoothstep(0.82, 0.92, w1 * w2 * 0.5 + 0.5 + 0.25 * sin(v * 2.3 - time * 1.4));
        albedo = mix(vec3(0.03, 0.17, 0.22), vec3(0.06, 0.28, 0.34), 0.5 + 0.5 * w2);
        albedo = mix(albedo, vec3(0.5, 0.72, 0.8), glint * 0.6 * (0.3 + 0.7 * daylight));
        rough = 0.12;
    } else if (surface == GRASS) {
        // Lawn in two greens, mown in bands; gravel paths through its middle each way.
        let mow = pulse((u + v) / 9.0, 0.0, 0.5, (fw.x + fw.y) / 9.0);
        let h = hash2(floor(uv / 7.0) + vec2(seed));
        albedo = mix(vec3(0.05, 0.17, 0.045), vec3(0.075, 0.23, 0.055), mow) * (0.9 + 0.2 * h);
        let path = max(band(abs(u), -1.0, 1.6, fw.x), band(abs(v), -1.0, 1.6, fw.y));
        albedo = mix(albedo, vec3(0.3, 0.27, 0.22), path);
        rough = 0.9;
    } else if (surface == SQUARE) {
        // A lift's square: big light plates, lines of the quarter's colour ruling it.
        albedo = vec3(0.33, 0.34, 0.365) * (0.94 + 0.12 * (hash2(floor(uv / 6.0)) - 0.5));
        albedo *= 1.0 - 0.35 * joints(uv, vec2(6.0), 0.06, fw);
        let rule = joints(uv + vec2(0.25), vec2(24.0), 0.5, fw);
        albedo = mix(albedo, quarter * 0.9, rule * 0.8);
        rough = 0.6;
    } else if (surface == SITE) {
        if (paint_i == 0u) {
            // A place's site, kept clear for it: plates hatched in yellow.
            albedo = vec3(0.17, 0.175, 0.19) * (1.0 - 0.3 * joints(uv, vec2(3.0), 0.04, fw));
            let hatch = pulse((u + v) / 6.0, 0.0, 0.06, (fw.x + fw.y) / 6.0);
            albedo = mix(albedo, yellow * 0.8, hatch * 0.7);
        } else {
            // The building site's ground: bare, tracked.
            let h = hash2(floor(uv / 3.0) + vec2(seed));
            albedo = vec3(0.17, 0.14, 0.11) * (0.85 + 0.3 * h);
            albedo *= 1.0 - 0.15 * pulse(u / 2.4, 0.0, 0.2, fw.x / 2.4);
        }
        rough = 0.9;
    } else if (surface == TRIM) {
        albedo = ink;
        rough = 0.45;
    } else if (surface == PORTAL) {
        // A lift's foot and its shaft: panels, the band in Charter blue high on its piers, chevrons
        // at their feet; rows of small lights up the shaft; under its lintel, light strips.
        let panel = vec2(3.0, 2.0);
        albedo = white * (0.985 + 0.05 * (hash2(floor(uv / panel) + vec2(seed)) - 0.5));
        albedo *= 1.0 - 0.4 * joints(uv, panel, 0.04, fw);
        if ((flags & 1u) != 0u) {
            let strip = pulse(v / 7.0, 0.46, 0.54, fw.y / 7.0) * pulse(u / 10.0, 0.05, 0.95, fw.x / 10.0);
            albedo = mix(albedo * 0.86, white, strip);
            glow = vec3(1.0, 0.985, 0.95) * 4.0 * strip;
        } else {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(v, -1.0, 1.2, fw.y));
            let blue = town.paint[1].rgb;
            albedo = mix(albedo, blue, band(v, 18.0, 21.0, fw.y));
            albedo = mix(albedo, dark, band(v, 17.6, 17.8, fw.y) + band(v, 21.2, 21.4, fw.y));
            let lights = pulse(v / 36.0, 0.0, 0.012, fw.y / 36.0) * pulse(u / 5.0, 0.3, 0.5, fw.x / 5.0) * step(30.0, v);
            glow = vec3(1.0, 0.95, 0.85) * 3.0 * lights * (0.35 + 0.65 * lamps);
        }
        rough = 0.45;
    } else if (surface == GLOW) {
        // A door's frame, lit always; a lamp's head (its flag), by the lamps' hour.
        let on = select(1.0, lamps, (flags & 1u) != 0u);
        albedo = mix(ink, white, 0.3) * 0.5;
        glow = mix(as_light(ink), vec3(1.0, 0.9, 0.72), select(0.0, 1.0, (flags & 1u) != 0u)) * 3.2 * on;
    } else if (surface == DOOR) {
        // Two leaves meeting at the middle, ribbed; chevrons along the meeting.
        albedo = mix(white * 0.9, vec3(0.6, 0.63, 0.68), 0.35);
        albedo *= 1.0 - 0.5 * pulse(v / 1.0, 0.0, 0.06, fw.y);
        albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(abs(u), -1.0, 0.3, fw.x));
        albedo = mix(albedo, dark, band(abs(u), -1.0, 0.02, fw.x));
        rough = 0.5;
    } else if (surface == RIM) {
        // The ring's faces: great panels, a band of the quarter's colour over the rim walk, and decks
        // of small windows all the way up, lit by night.
        let panel = vec2(12.0, 8.0);
        albedo = panel_paint(paint_i) * 0.8 * (0.97 + 0.06 * (hash2(floor(uv / panel)) - 0.5));
        albedo *= 1.0 - 0.3 * joints(uv, panel, 0.15, fw);
        albedo = mix(albedo, quarter * 0.85, band(v, 20.0, 24.0, fw.y));
        let deck = floor(v / 16.0);
        let wv = v - deck * 16.0;
        let wu = floor(u / 4.0);
        let h = hash2(vec2(wu, deck));
        let win = band(wv, 9.0, 11.0, fw.y) * pulse(u / 4.0, 0.15, 0.85, fw.x / 4.0) * step(32.0, v);
        albedo = mix(albedo, vec3(0.08, 0.14, 0.24) * (0.3 + 0.7 * daylight), win);
        glow = vec3(1.0, 0.8, 0.5) * win * step(h, 0.45 * lamps) * 1.6;
        rough = 0.5;
    } else if (surface == SKY) {
        // The ceiling: the sky's light in its glass, behind a lattice of frames across the ring and
        // round it, and finer bars between them.
        let across = pulse(u / SKY_FRAME, 0.0, 7.0 / SKY_FRAME, fw.x / SKY_FRAME);
        let ring_frame = pulse(v / (CELL * 0.5), 0.0, 6.0 / (CELL * 0.5), fw.y / (CELL * 0.5));
        let fine = max(pulse(u / 25.0, 0.0, 0.04, fw.x / 25.0), pulse(v / 21.817, 0.0, 0.045, fw.y / 21.817));
        let frame = max(max(across, ring_frame), fine * 0.55);
        albedo = vec3(0.02);
        glow = mix(town.sky.rgb, town.sky.rgb * 0.22, frame);
    } else if (surface == FOLIAGE) {
        let h = hash2(vec2(seed, 1.0));
        let top = clamp(pbr.N.y * 0.5 + 0.5, 0.0, 1.0);
        albedo = mix(vec3(0.03, 0.11, 0.035), vec3(0.09, 0.27, 0.06), top) * (0.8 + 0.4 * h);
        rough = 0.9;
    }

    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = clamp(rough, 0.05, 1.0);
    pbr.material.metallic = 0.0;
    pbr.material.emissive = vec4(glow, 0.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    let to_eye = view.world_position.xyz - in.world_position.xyz;
    let dist = length(to_eye);
    if (town.day.w > 0.5) {
        out.color = vec4(paint_bands(out.color.rgb, albedo, glow, pbr.N, to_eye / max(dist, 1e-3)), out.color.a);
    }
    // The haze, in painted layers: each plane of the town a step lighter and cooler than the last,
    // until the floor rising ahead is lost in it.
    let fog = 1.0 - exp(-dist * town.haze.w);
    let q = fog * 7.0;
    let layered = (floor(q) + smoothstep(0.3, 0.7, fract(q))) / 7.0;
    let hazed = mix(fog, layered, 0.55);
    out.color = vec4(mix(out.color.rgb, town.haze.rgb, hazed) * town.sky.w, out.color.a);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
