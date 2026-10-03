// The buildings' surfaces, for `city.wgsl`: walls, the colony's halls, curtain walls, roofs, the
// site's steel and the quays' railings. Materials by district and strip, windows with rooms behind
// them, shopfronts with their signs, awnings and shutters, rooftops, and the wear of a city that's
// lived in: rain runs under the sills, grime at the foot of the walls, rust, patches and soot.
//
// Everything is antialiased by the pixel's footprint (the derivatives `city.wgsl` takes at the top
// of its fragment shader, where control flow is still uniform; nothing here takes one): a pattern
// smaller than a pixel fades to its average instead of shimmering, so the same code paints a
// doorway at arm's length and a block kilometres overhead. Nothing here knows the city's layout:
// what a building is comes from its vertex (surface, seed, height) and its block's atlas texel
// (district, kind).
#define_import_path bc::facade

#import bc::noise::{hash13, noise3}

struct FacadeIn {
    // `city_mesh::Surface`: 1 Wall, 2 Roof, 9 Steel, 10 Railing, 11 Hall, 12 Glass.
    surface: u32,
    // The vertex's seed as `city.wgsl` reads it (`in.color.g * 255`, 0..255).
    seed: f32,
    // The building's top over the tallest's (240 m): 0 for an L3 block's bulk.
    tall: f32,
    ao: f32,
    // Walls: metres along and up from the floor. Roofs and steel tops: the strip's (s, x).
    uv: vec2<f32>,
    duv_dx: vec2<f32>,
    duv_dy: vec2<f32>,
    // The point in the colony's frame; the derivatives of the render-relative world position.
    p: vec3<f32>,
    dp_dx: vec3<f32>,
    dp_dy: vec3<f32>,
    // Unit world normal; unit, surface to eye.
    n: vec3<f32>,
    v: vec3<f32>,
    // From the block's atlas texel: its district (byte g: 0 none, else `DistrictKind` + 1) and its
    // kind (byte r: 1 buildings, 2 park, 3 plaza, 4 canal, 5 site, 6 place, 7 tower; 0 none).
    district: u32,
    kind: u32,
    // 0 Charter, 1 Canal, 2 Gardens.
    strip: u32,
    lamps: f32,
    daylight: f32,
    seconds: f32,
    // The pixel's window coordinates (`pbr.frag_coord.xy`): the railing's cut-outs are dithered by
    // them where they're finer than a pixel.
    frag: vec2<f32>,
};

struct FacadeOut {
    albedo: vec3<f32>,
    rough: f32,
    metal: f32,
    // Unexposed nits (lit rooms, signs, lamps).
    emissive: vec3<f32>,
    // The perturbed unit normal (sills, reveals, ribs, balconies, panes).
    n: vec3<f32>,
    // 0..1, how mirror-like: the integrator adds the sky's reflection times Fresnel times this.
    reflectance: f32,
    // 0..1, occlusion on top of the vertex's (reveals, under balconies and awnings).
    occlusion: f32,
    // 1 where the integrator should discard (the gaps in a railing, close up).
    cut: f32,
};

const F_TAU: f32 = 6.2831853;
const F_PI: f32 = 3.14159265;
const F_RADIUS: f32 = 3200.0;
const F_FIRST_WINDOW: f32 = 0.4;
// `bc_sim::colony::city`: the ground floor, a storey, the kerb, the tallest roof, a block.
const F_GROUND: f32 = 5.0;
const F_STOREY: f32 = 3.6;
const F_KERB: f32 = 0.15;
const F_MAX_HEIGHT: f32 = 240.0;
const F_BLOCK: f32 = 128.0;
// A lit room's lamps, nits on a white wall (each room 0.6 to 1.4 times this), and how much brighter
// a city's lit windows read once they're finer than a pixel: the glitter the eye and the bloom make
// of a far strip at night, an anime licence on the average.
const F_ROOM_NITS: f32 = 800.0;
const F_FAR_GLOW: f32 = 2.2;

// Districts, as the atlas has them (`DistrictKind` + 1).
const D_CIVIC: u32 = 1u;
const D_BUSINESS: u32 = 2u;
const D_MIDTOWN: u32 = 3u;
const D_RESIDENTIAL: u32 = 4u;
const D_OLDTOWN: u32 = 5u;
const D_UNIVERSITY: u32 = 6u;
const D_WORKS: u32 = 7u;
const D_PARK: u32 = 8u;
const D_PORT: u32 = 9u;

// What a wall is made of.
const M_RENDER: u32 = 0u;
const M_BRICK: u32 = 1u;
const M_STONE: u32 = 2u;
const M_CONCRETE: u32 = 3u;
const M_TILE: u32 = 4u;
const M_METAL: u32 = 5u;
const M_PANEL: u32 = 6u;
const M_TIMBER: u32 = 7u;

// What a ground floor is.
const G_SHOPS: u32 = 0u;
const G_WORKS: u32 = 1u;
const G_LOBBY: u32 = 2u;
const G_STONE: u32 = 3u;
const G_FLATS: u32 = 4u;
const G_BLANK: u32 = 5u;

// What's behind a window.
const R_HOME: u32 = 0u;
const R_OFFICE: u32 = 1u;
const R_SHOP: u32 = 2u;
const R_HALL: u32 = 3u;
const R_WORKS: u32 = 4u;

// Where a point of the colony's frame is on the city, as `frame::from_colony` has it: (strip, s,
// x), with the strip −1 over a window (and s then across the window).
fn city_place(p: vec3<f32>) -> vec3<f32> {
    let a = atan2(p.z, p.y) - (F_FIRST_WINDOW - F_TAU / 12.0);
    let rel = a - floor(a / F_TAU) * F_TAU;
    let k = min(floor(rel / (F_TAU / 3.0)), 2.0);
    let within = rel - k * (F_TAU / 3.0);
    if (within < F_TAU / 6.0) {
        return vec3(-1.0, within * F_RADIUS, p.x);
    }
    return vec3(k, (within - F_TAU / 6.0) * F_RADIUS, p.x);
}

// ---------------------------------------------------------------------------------- small tools

fn f_h(a: f32, b: f32, c: f32) -> f32 {
    return hash13(vec3(a, b, c));
}

// A pulse train filtered by a box: the share of a footprint `w` wide (in periods) about `x` that
// falls in [a, b] of each unit period (0 <= a < b <= 1). Exact for the box, and it tends to b − a
// as the footprint grows, so a grid seen from afar is its average, not moiré. Worked on the
// fraction, so it keeps its precision far from the origin.
fn f_pulse(x: f32, a: f32, b: f32, w: f32) -> f32 {
    let f = fract(x);
    let h = 0.5 * max(w, 1e-3);
    let hi = floor(f + h) * (b - a) + clamp(fract(f + h), a, b);
    let lo = floor(f - h) * (b - a) + clamp(fract(f - h), a, b);
    return clamp((hi - lo) / (2.0 * h), 0.0, 1.0);
}

// One band [a, b], filtered the same way.
fn f_band(x: f32, a: f32, b: f32, w: f32) -> f32 {
    let h = 0.5 * max(w, 1e-4);
    return (clamp(x + h, a, b) - clamp(x - h, a, b)) / (2.0 * h);
}

// 1 while a feature `size` across spans a few pixels (footprint `w`), 0 once it's about one.
fn f_fade(w: f32, size: f32) -> f32 {
    return 1.0 - smoothstep(0.25, 0.8, w / size);
}

// The same for a wave (ribs, slats, folds) of `period`, and for the normal it ripples: whole while
// a period spans eight pixels or more, gone by three. A wave is aliased well before its period
// shrinks to a pixel (two pixels a period is the limit), and the light it catches through a tilted
// normal is sharper than the wave itself; with `f_fade`'s margins a shutter's slats drew moiré
// arcs at a few metres.
fn f_fade_wave(w: f32, period: f32) -> f32 {
    return 1.0 - smoothstep(0.12, 0.33, w / period);
}

// A rounded rectangle's signed distance (centre `c`, half-size `hs`, corner radius `r`).
fn f_rrect(q: vec2<f32>, c: vec2<f32>, hs: vec2<f32>, r: f32) -> f32 {
    let d = abs(q - c) - hs + vec2(r);
    return length(max(d, vec2(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

// A small lamp seen from anywhere: a disc `r` across that grows to the pixel's footprint when it's
// smaller, dimming as it grows, so it neither vanishes nor sparkles.
fn f_lamp_dot(q: vec2<f32>, r: f32, fw: vec2<f32>) -> f32 {
    let rr = max(r, 0.75 * max(fw.x, fw.y));
    let k = r / rr;
    return (1.0 - smoothstep(rr * 0.5, rr, length(q))) * k * k;
}

// A lamp's colour: 0 warm (2,700 K), 0.5 neutral (4,000 K), 1 cool (6,500 K).
fn f_lamp_colour(k: f32) -> vec3<f32> {
    let warm = vec3(1.0, 0.64, 0.34);
    let neutral = vec3(1.0, 0.8, 0.58);
    let cool = vec3(0.88, 0.9, 1.0);
    return select(mix(neutral, cool, clamp(k * 2.0 - 1.0, 0.0, 1.0)), mix(warm, neutral, clamp(k * 2.0, 0.0, 1.0)), k < 0.5);
}

// The strip's tram line's colour: Charter blue, Canal teal, Gardens green.
fn f_line(strip: u32) -> vec3<f32> {
    var c = vec3(0.12, 0.32, 0.72);
    if (strip == 1u) {
        c = vec3(0.05, 0.5, 0.52);
    }
    if (strip == 2u) {
        c = vec3(0.2, 0.55, 0.22);
    }
    return c;
}

// The same, as a light: its brightest channel 1.
fn f_line_glow(strip: u32) -> vec3<f32> {
    let c = f_line(strip);
    return c / max(max(c.r, c.g), c.b);
}

// ------------------------------------------------------------------------------------ palettes

fn f_render(k: f32) -> vec3<f32> {
    var c = vec3(0.7, 0.62, 0.4);
    c = select(c, vec3(0.66, 0.46, 0.36), k < 0.82);
    c = select(c, vec3(0.48, 0.53, 0.57), k < 0.7);
    c = select(c, vec3(0.44, 0.48, 0.38), k < 0.6);
    c = select(c, vec3(0.68, 0.66, 0.6), k < 0.5);
    c = select(c, vec3(0.6, 0.37, 0.29), k < 0.38);
    c = select(c, vec3(0.62, 0.45, 0.24), k < 0.27);
    c = select(c, vec3(0.66, 0.58, 0.45), k < 0.15);
    return c;
}

fn f_brick(k: f32) -> vec3<f32> {
    var c = vec3(0.16, 0.12, 0.1);
    c = select(c, vec3(0.44, 0.34, 0.2), k < 0.88);
    c = select(c, vec3(0.42, 0.2, 0.1), k < 0.75);
    c = select(c, vec3(0.27, 0.15, 0.09), k < 0.6);
    c = select(c, vec3(0.34, 0.13, 0.075), k < 0.35);
    return c;
}

fn f_stone(k: f32) -> vec3<f32> {
    var c = vec3(0.13, 0.13, 0.135);
    c = select(c, vec3(0.38, 0.37, 0.35), k < 0.85);
    c = select(c, vec3(0.5, 0.4, 0.28), k < 0.65);
    c = select(c, vec3(0.56, 0.52, 0.44), k < 0.4);
    return c;
}

fn f_concrete(k: f32) -> vec3<f32> {
    var c = vec3(0.3, 0.3, 0.3);
    c = select(c, vec3(0.48, 0.45, 0.4), k < 0.8);
    c = select(c, vec3(0.42, 0.41, 0.39), k < 0.5);
    return c;
}

fn f_tile(k: f32) -> vec3<f32> {
    var c = vec3(0.3, 0.4, 0.34);
    c = select(c, vec3(0.33, 0.22, 0.15), k < 0.87);
    c = select(c, vec3(0.34, 0.39, 0.44), k < 0.72);
    c = select(c, vec3(0.58, 0.5, 0.38), k < 0.55);
    c = select(c, vec3(0.66, 0.65, 0.6), k < 0.3);
    return c;
}

fn f_cladding(k: f32) -> vec3<f32> {
    var c = vec3(0.58, 0.42, 0.08);
    c = select(c, vec3(0.6, 0.6, 0.56), k < 0.9);
    c = select(c, vec3(0.36, 0.1, 0.07), k < 0.76);
    c = select(c, vec3(0.14, 0.26, 0.19), k < 0.62);
    c = select(c, vec3(0.12, 0.2, 0.32), k < 0.48);
    c = select(c, vec3(0.4, 0.41, 0.42), k < 0.3);
    return c;
}

fn f_timber(k: f32) -> vec3<f32> {
    var c = vec3(0.22, 0.14, 0.08);
    c = select(c, vec3(0.4, 0.37, 0.33), k < 0.8);
    c = select(c, vec3(0.36, 0.24, 0.13), k < 0.5);
    return c;
}

fn f_frame_colour(k: f32) -> vec3<f32> {
    var c = vec3(0.35, 0.36, 0.37);
    c = select(c, vec3(0.1, 0.22, 0.15), k < 0.87);
    c = select(c, vec3(0.25, 0.16, 0.09), k < 0.75);
    c = select(c, vec3(0.06, 0.06, 0.065), k < 0.6);
    c = select(c, vec3(0.7, 0.7, 0.68), k < 0.35);
    return c;
}

fn f_room_wall(k: f32) -> vec3<f32> {
    var c = vec3(0.35, 0.3, 0.38);
    c = select(c, vec3(0.68, 0.5, 0.4), k < 0.9);
    c = select(c, vec3(0.52, 0.58, 0.64), k < 0.8);
    c = select(c, vec3(0.5, 0.58, 0.5), k < 0.68);
    c = select(c, vec3(0.66, 0.58, 0.44), k < 0.55);
    c = select(c, vec3(0.72, 0.7, 0.64), k < 0.35);
    return c;
}

fn f_furniture(k: f32) -> vec3<f32> {
    var c = vec3(0.6, 0.58, 0.52);
    c = select(c, vec3(0.12, 0.2, 0.3), k < 0.8);
    c = select(c, vec3(0.38, 0.12, 0.08), k < 0.65);
    c = select(c, vec3(0.35, 0.33, 0.3), k < 0.5);
    c = select(c, vec3(0.28, 0.17, 0.09), k < 0.3);
    return c;
}

fn f_curtain_colour(k: f32) -> vec3<f32> {
    var c = vec3(0.65, 0.35, 0.12);
    c = select(c, vec3(0.25, 0.35, 0.2), k < 0.88);
    c = select(c, vec3(0.74, 0.72, 0.68), k < 0.75);
    c = select(c, vec3(0.15, 0.22, 0.4), k < 0.58);
    c = select(c, vec3(0.45, 0.1, 0.07), k < 0.42);
    c = select(c, vec3(0.6, 0.5, 0.36), k < 0.25);
    return c;
}

fn f_sign_colour(k: f32) -> vec3<f32> {
    var c = vec3(0.72, 0.72, 0.7);
    c = select(c, vec3(0.04, 0.25, 0.26), k < 0.87);
    c = select(c, vec3(0.65, 0.5, 0.08), k < 0.75);
    c = select(c, vec3(0.03, 0.03, 0.03), k < 0.64);
    c = select(c, vec3(0.7, 0.66, 0.55), k < 0.52);
    c = select(c, vec3(0.4, 0.05, 0.04), k < 0.4);
    c = select(c, vec3(0.03, 0.05, 0.14), k < 0.28);
    c = select(c, vec3(0.04, 0.12, 0.07), k < 0.15);
    return c;
}

fn f_awning_colour(k: f32) -> vec3<f32> {
    var c = vec3(0.04, 0.3, 0.3);
    c = select(c, vec3(0.25, 0.04, 0.07), k < 0.88);
    c = select(c, vec3(0.65, 0.38, 0.05), k < 0.74);
    c = select(c, vec3(0.04, 0.07, 0.2), k < 0.58);
    c = select(c, vec3(0.05, 0.25, 0.1), k < 0.42);
    c = select(c, vec3(0.45, 0.06, 0.05), k < 0.22);
    return c;
}

fn f_door_colour(k: f32) -> vec3<f32> {
    var c = vec3(0.04, 0.28, 0.28);
    c = select(c, vec3(0.6, 0.45, 0.05), k < 0.82);
    c = select(c, vec3(0.03, 0.03, 0.03), k < 0.7);
    c = select(c, vec3(0.04, 0.06, 0.18), k < 0.55);
    c = select(c, vec3(0.05, 0.2, 0.1), k < 0.38);
    c = select(c, vec3(0.45, 0.05, 0.04), k < 0.2);
    return c;
}

// Neon and backlit lettering, and paint for tags: warm white, red, cyan, pink, green, yellow.
fn f_neon(k: f32) -> vec3<f32> {
    var c = vec3(1.0, 0.8, 0.2);
    c = select(c, vec3(0.3, 1.0, 0.4), k < 0.86);
    c = select(c, vec3(1.0, 0.3, 0.7), k < 0.72);
    c = select(c, vec3(0.2, 0.85, 1.0), k < 0.58);
    c = select(c, vec3(1.0, 0.15, 0.1), k < 0.4);
    c = select(c, vec3(1.0, 0.85, 0.65), k < 0.25);
    return c;
}

// ------------------------------------------------------------------------------- layers, frames

// A surface's look before it's lit: what the facade's parts are blended as.
struct FLayer {
    albedo: vec3<f32>,
    rough: f32,
    metal: f32,
    // The normal's tilt along the surface's tangent and up (added to the normal, then normalised).
    tilt: vec2<f32>,
    emissive: vec3<f32>,
    refl: f32,
    occ: f32,
};

fn f_layer(albedo: vec3<f32>, rough: f32) -> FLayer {
    return FLayer(albedo, rough, 0.0, vec2(0.0), vec3(0.0), 0.0, 1.0);
}

fn f_blend(a: FLayer, b: FLayer, k: f32) -> FLayer {
    return FLayer(
        mix(a.albedo, b.albedo, k),
        mix(a.rough, b.rough, k),
        mix(a.metal, b.metal, k),
        mix(a.tilt, b.tilt, k),
        mix(a.emissive, b.emissive, k),
        mix(a.refl, b.refl, k),
        mix(a.occ, b.occ, k)
    );
}

// A face's frame: along it (the way uv.x grows), up it (the way uv.y grows), out of it; the view
// ray in those terms (z into the face); and the pixel's footprint in uv.
struct FFrame {
    t: vec3<f32>,
    b: vec3<f32>,
    n: vec3<f32>,
    d: vec3<f32>,
    fw: vec2<f32>,
};

// A wall's frame. Walls are plumb, so up is towards the axis; along is up × out, turned to the way
// uv.x grows (which the derivatives say: a wall's u runs either way round the building).
fn f_frame(i: FacadeIn) -> FFrame {
    let up = -normalize(vec3(0.0, i.p.y, i.p.z));
    let n = i.n;
    let c = cross(up, n);
    let cl = length(c);
    // A level face (a steel top) has no along of its own: across the strip will do.
    let a = cross(n, vec3(1.0, 0.0, 0.0));
    var t = select(a / max(length(a), 1e-6), c / max(cl, 1e-6), cl > 1e-3);
    let dp2perp = cross(i.dp_dy, n);
    let dp1perp = cross(n, i.dp_dx);
    let t_uv = dp2perp * i.duv_dx.x + dp1perp * i.duv_dy.x;
    t = t * select(-1.0, 1.0, dot(t_uv, t) >= 0.0);
    let rd = -i.v;
    var d = vec3(dot(rd, t), dot(rd, up), -dot(rd, n));
    // Grazing: keep the ray going in.
    d.z = max(d.z, 0.02);
    return FFrame(t, up, n, d, abs(i.duv_dx) + abs(i.duv_dy));
}

fn f_out(L: FLayer, fr: FFrame) -> FacadeOut {
    var o: FacadeOut;
    o.albedo = clamp(L.albedo, vec3(0.0), vec3(1.0));
    o.rough = clamp(L.rough, 0.04, 1.0);
    o.metal = clamp(L.metal, 0.0, 1.0);
    o.emissive = max(L.emissive, vec3(0.0));
    o.n = normalize(fr.n + fr.t * L.tilt.x + fr.b * L.tilt.y);
    o.reflectance = clamp(L.refl, 0.0, 1.0);
    o.occlusion = clamp(L.occ, 0.0, 1.0);
    o.cut = 0.0;
    return o;
}

// ----------------------------------------------------------------------------------- the looks

// How a building is built, from its district, strip, block kind and seed.
struct FLook {
    mat: u32,
    base: vec3<f32>,
    trim: vec3<f32>,
    frame: vec3<f32>,
    // Its windows: the bay, the opening's width, its sill and head over the storey's floor, and how
    // far the glass is set back, m. No windows when `win_w` is 0.
    bay: f32,
    win_w: f32,
    sill: f32,
    head: f32,
    reveal: f32,
    // Wear 0..1, soot over the windows; balconies (0 none, 1 a bay's, 2 running along the floor);
    // a cornice and string course; the ground floor (`G_*`); the rooms (`R_*`).
    wear: f32,
    soot: f32,
    balcony: u32,
    cornice: bool,
    ground: u32,
    room: u32,
    // At night: the share of rooms lit, of shopfronts lit, and the lamps' colour (0 warm, 1 cool).
    lit: f32,
    shop_lit: f32,
    warmth: f32,
    rough: f32,
    // The share of shopfronts with awnings.
    awning: f32,
};

fn f_look(district: u32, strip: u32, kind: u32, s: f32) -> FLook {
    let h0 = f_h(s, 1.0, 7.3);
    let h1 = f_h(s, 2.0, 7.3);
    let h2 = f_h(s, 3.0, 7.3);
    let h3 = f_h(s, 4.0, 7.3);
    let h4 = f_h(s, 5.0, 7.3);
    let h5 = f_h(s, 6.0, 7.3);
    var lk: FLook;
    lk.mat = M_RENDER;
    lk.base = f_render(h1);
    lk.trim = vec3(0.58, 0.56, 0.52);
    lk.frame = f_frame_colour(h2);
    lk.bay = 3.2;
    lk.win_w = 1.4;
    lk.sill = 0.9;
    lk.head = 2.6;
    lk.reveal = 0.12;
    lk.wear = 0.4;
    lk.soot = 0.0;
    lk.balcony = 0u;
    lk.cornice = false;
    lk.ground = G_SHOPS;
    lk.room = R_HOME;
    lk.lit = 0.38;
    lk.shop_lit = 0.6;
    lk.warmth = 0.35;
    lk.rough = 0.88;
    lk.awning = 0.25;
    var d = district;
    if (kind == 2u) {
        d = D_PARK;
    }
    if (d == 0u) {
        // The colony's own (Hub Gate's surroundings): its white panels.
        lk.mat = M_PANEL;
        lk.base = vec3(0.7, 0.71, 0.72);
        lk.frame = vec3(0.72, 0.73, 0.75);
        lk.ground = G_LOBBY;
        lk.room = R_HALL;
        lk.wear = 0.1;
        lk.warmth = 0.6;
    } else if (d == D_CIVIC) {
        // Stone and white render, tall windows, a cornice.
        if (h0 < 0.6) {
            lk.mat = M_STONE;
            lk.base = f_stone(h1 * 0.64);
        } else {
            lk.base = vec3(0.68, 0.67, 0.63);
        }
        lk.trim = vec3(0.62, 0.59, 0.52);
        lk.frame = vec3(0.07, 0.075, 0.08);
        lk.bay = 4.2;
        lk.win_w = 1.6;
        lk.sill = 0.8;
        lk.head = 3.1;
        lk.reveal = 0.22;
        lk.wear = 0.3;
        lk.cornice = true;
        lk.ground = G_STONE;
        lk.room = R_OFFICE;
        lk.lit = 0.3;
        lk.shop_lit = 0.4;
        lk.warmth = 0.55;
    } else if (d == D_BUSINESS) {
        // Granite, limestone, concrete or the colony's white panels; ribbon windows; lobbies.
        if (h0 < 0.35) {
            lk.mat = M_STONE;
            lk.base = f_stone(select(0.3, 0.95, h1 < 0.5));
        } else if (h0 < 0.6) {
            lk.mat = M_CONCRETE;
            lk.base = f_concrete(h1);
        } else {
            lk.mat = M_PANEL;
            lk.base = mix(vec3(0.7, 0.71, 0.72), vec3(0.52, 0.55, 0.58), h1);
        }
        lk.frame = vec3(0.05, 0.055, 0.06);
        lk.bay = 3.0;
        lk.win_w = 2.5;
        lk.sill = 0.7;
        lk.head = 3.2;
        lk.reveal = 0.08;
        lk.wear = 0.15;
        lk.ground = G_LOBBY;
        lk.room = R_OFFICE;
        lk.lit = 0.42;
        lk.warmth = 0.85;
        lk.rough = 0.6;
    } else if (d == D_MIDTOWN) {
        // Tile and concrete, balconies in rows, shops.
        if (h0 < 0.45) {
            lk.mat = M_TILE;
            lk.base = f_tile(h1);
        } else if (h0 < 0.8) {
            lk.mat = M_CONCRETE;
            lk.base = f_concrete(h1);
        }
        lk.trim = vec3(0.5, 0.49, 0.47);
        lk.bay = 3.4;
        lk.win_w = 1.7;
        lk.sill = 0.9;
        lk.head = 2.7;
        lk.reveal = 0.1;
        lk.balcony = select(0u, select(1u, 2u, h5 < 0.25), h5 < 0.55);
        lk.wear = 0.5;
        lk.lit = 0.4;
        lk.shop_lit = 0.85;
        lk.warmth = 0.45;
        lk.awning = 0.35;
    } else if (d == D_RESIDENTIAL || d == D_PARK) {
        // Render and brick; the Canal's mostly brick, the Gardens' warm render and timber, with
        // balconies.
        if (strip == 1u) {
            if (h0 < 0.6) {
                lk.mat = M_BRICK;
                lk.base = f_brick(h1);
            } else if (h0 < 0.9) {
                lk.base = f_render(h1);
            } else {
                lk.mat = M_CONCRETE;
                lk.base = f_concrete(h1);
            }
        } else if (strip == 2u) {
            if (h0 < 0.55) {
                lk.base = f_render(h1 * 0.4 + select(0.0, 0.6, h1 > 0.6));
            } else if (h0 < 0.75) {
                lk.mat = M_TIMBER;
                lk.base = f_timber(h1);
            } else {
                lk.mat = M_BRICK;
                lk.base = f_brick(h1 * 0.88);
            }
        } else {
            if (h0 < 0.5) {
                lk.base = f_render(h1);
            } else if (h0 < 0.88) {
                lk.mat = M_BRICK;
                lk.base = f_brick(h1);
            } else {
                lk.mat = M_TILE;
                lk.base = f_tile(h1);
            }
        }
        lk.bay = 3.0;
        lk.win_w = 1.3;
        lk.sill = 0.9;
        lk.head = 2.5;
        lk.balcony = select(0u, 1u, h5 < select(0.3, 0.65, strip == 2u));
        lk.wear = 0.45;
        lk.ground = select(G_FLATS, G_SHOPS, h4 < 0.4);
        lk.lit = 0.42;
        lk.shop_lit = 0.55;
        lk.warmth = 0.2;
        lk.awning = select(0.3, 0.55, strip == 2u);
        if (d == D_PARK) {
            // A pavilion in a park: a café front, timber or white render.
            lk.mat = select(M_RENDER, M_TIMBER, h0 < 0.5);
            lk.base = select(vec3(0.68, 0.66, 0.6), f_timber(h1), h0 < 0.5);
            lk.ground = G_SHOPS;
            lk.awning = 0.7;
            lk.wear = 0.3;
            lk.balcony = 0u;
        }
    } else if (d == D_OLDTOWN) {
        // The first streets: brick and sooty stone, narrow tall windows, sills and cornices.
        if (h0 < 0.65) {
            lk.mat = M_BRICK;
            lk.base = f_brick(h1) * 0.88;
        } else {
            lk.mat = M_STONE;
            lk.base = f_stone(h1 * 0.65) * 0.8;
        }
        lk.trim = vec3(0.55, 0.52, 0.46);
        lk.frame = select(vec3(0.7, 0.7, 0.68), f_frame_colour(0.6 + 0.4 * h2), h2 < 0.5);
        lk.bay = 2.7;
        lk.win_w = 1.05;
        lk.sill = 1.0;
        lk.head = 2.85;
        lk.reveal = 0.22;
        lk.wear = 0.85;
        lk.soot = 0.5;
        lk.cornice = true;
        lk.lit = 0.36;
        lk.shop_lit = 0.85;
        lk.warmth = 0.12;
        lk.awning = 0.45;
    } else if (d == D_UNIVERSITY) {
        // Brick and stone, tall windows.
        if (h0 < 0.6) {
            lk.mat = M_BRICK;
            lk.base = f_brick(h1);
        } else {
            lk.mat = M_STONE;
            lk.base = f_stone(h1 * 0.64);
        }
        lk.trim = vec3(0.6, 0.57, 0.5);
        lk.bay = 3.6;
        lk.win_w = 1.5;
        lk.sill = 0.6;
        lk.head = 3.2;
        lk.reveal = 0.18;
        lk.wear = 0.4;
        lk.cornice = h5 < 0.6;
        lk.ground = G_STONE;
        lk.room = R_OFFICE;
        lk.lit = 0.3;
        lk.warmth = 0.5;
    } else {
        // The Works and the Port: cladding, brick, concrete; high strip windows, roller doors.
        if (h0 < select(0.65, 0.55, d == D_PORT)) {
            lk.mat = M_METAL;
            lk.base = f_cladding(h1);
        } else if (h0 < select(0.9, 0.6, d == D_PORT)) {
            lk.mat = M_BRICK;
            lk.base = f_brick(h1);
        } else {
            lk.mat = M_CONCRETE;
            lk.base = f_concrete(h1);
        }
        lk.trim = vec3(0.4, 0.4, 0.39);
        lk.frame = vec3(0.2, 0.21, 0.22);
        lk.bay = 6.0;
        lk.win_w = 3.6;
        lk.sill = 2.0;
        lk.head = 3.1;
        lk.reveal = 0.06;
        lk.wear = select(1.0, 0.9, d == D_PORT);
        lk.soot = 0.3;
        lk.ground = G_WORKS;
        lk.room = R_WORKS;
        lk.lit = 0.15;
        lk.shop_lit = 0.25;
        lk.warmth = 0.75;
        lk.rough = 0.6;
    }
    // Charter is kept, the Canal works hard, the Gardens between; and some buildings are new.
    var sw = 1.0;
    if (strip == 0u) {
        sw = 0.7;
    }
    if (strip == 1u) {
        sw = 1.25;
    }
    if (strip == 2u) {
        sw = 0.9;
    }
    lk.wear = clamp(lk.wear * sw * (0.55 + 0.9 * h3), 0.0, 1.0);
    if (kind == 3u) {
        // A monument on a plaza: dressed stone, no windows.
        lk.mat = M_STONE;
        lk.base = f_stone(h1 * 0.85);
        lk.win_w = 0.0;
        lk.ground = G_BLANK;
        lk.wear = 0.3 * sw;
        lk.cornice = false;
    }
    if (lk.mat == M_BRICK || lk.mat == M_TIMBER) {
        lk.rough = 0.9;
    }
    lk.bay *= 0.92 + 0.16 * h4;
    return lk;
}

// The share of a building's rooms lit tonight.
fn f_lit_share(lit: f32, s: f32, lamps: f32) -> f32 {
    return clamp(lit * (0.45 + 1.1 * f_h(s, 41.0, 3.0)), 0.03, 0.85) * lamps;
}

#ifndef FACADE_LOW
// ------------------------------------------------------------------------------- the materials

// A wall's own material: courses, joints, panels, ribs, boards, and the tone of each piece. All of
// it fades to its average where it's finer than a pixel.
fn f_material(lk: FLook, uv: vec2<f32>, fw: vec2<f32>, s: f32) -> FLayer {
    let fm = max(fw.x, fw.y);
    var c = lk.base;
    // Weathering a few metres across: the stain of a downpipe, a sunnier face.
    let big = noise3(vec3(uv.x * 0.31, uv.y * 0.19, s * 0.37)) - 0.5;
    c *= 1.0 + 0.18 * big * f_fade(fm, 2.5);
    var L = f_layer(c, lk.rough);
    if (lk.mat == M_BRICK) {
        // Stretcher bond: 75 mm courses, 225 mm bricks, each its own.
        let cy = uv.y / 0.075;
        let row = floor(cy);
        let ux = uv.x / 0.225 + 0.5 * (row - 2.0 * floor(row * 0.5));
        let cover = f_pulse(cy, 0.14, 1.0, fw.y / 0.075) * f_pulse(ux, 0.045, 1.0, fw.x / 0.225);
        let tone = (f_h(floor(ux), row, s) - 0.5) * 0.4 * f_fade(fw.y, 0.075);
        let mortar = vec3(0.42, 0.4, 0.37) * mix(1.0, 0.6, lk.wear);
        L.albedo = mix(mortar, c * (1.0 + tone), cover);
    } else if (lk.mat == M_STONE) {
        // Ashlar: courses of half a metre or so, fine joints, each block a shade of its own.
        let ch = 0.5 + 0.2 * f_h(s, 61.0, 1.0);
        let cy = uv.y / ch;
        let row = floor(cy);
        let ux = uv.x / (ch * 2.1) + 0.37 * row;
        let cover = f_pulse(cy, 0.025, 1.0, fw.y / ch) * f_pulse(ux, 0.012, 1.0, fw.x / (ch * 2.1));
        let tone = (f_h(floor(ux), row, s + 0.5) - 0.5) * 0.18 * f_fade(fw.y, ch);
        let grain = (noise3(vec3(uv * 9.0, s)) - 0.5) * 0.08 * f_fade(fm, 0.11);
        L.albedo = mix(c * 0.72, c * (1.0 + tone + grain), cover);
        L.rough = select(0.75, 0.3, lk.base.r < 0.2);
    } else if (lk.mat == M_CONCRETE) {
        // Panels a storey high, their joints, the form-ties' holes.
        let cx = uv.x / 3.0;
        let cy = (uv.y - F_GROUND) / F_STOREY;
        let joint = max(f_pulse(cx, 0.0, 0.0067, fw.x / 3.0), f_pulse(cy, 0.0, 0.0056, fw.y / F_STOREY));
        let tone = (f_h(floor(cx), floor(cy), s) - 0.5) * 0.14 * f_fade(fm, 1.5);
        let tie = f_pulse(uv.x / 0.6, 0.47, 0.53, fw.x / 0.6) * f_pulse(uv.y / 0.6, 0.47, 0.53, fw.y / 0.6);
        L.albedo = c * (1.0 + tone) * (1.0 - 0.45 * joint) * (1.0 - 0.5 * tie);
        L.rough = 0.92;
    } else if (lk.mat == M_TILE) {
        // Small glazed tiles: a sheen, and pale joints.
        let cy = uv.y / 0.1;
        let row = floor(cy);
        let ux = uv.x / 0.2 + 0.5 * (row - 2.0 * floor(row * 0.5));
        let cover = f_pulse(cy, 0.08, 1.0, fw.y / 0.1) * f_pulse(ux, 0.04, 1.0, fw.x / 0.2);
        let tone = (f_h(floor(ux), row, s) - 0.5) * 0.22 * f_fade(fw.y, 0.1);
        L.albedo = mix(vec3(0.6, 0.6, 0.58), c * (1.0 + tone), cover);
        L.rough = 0.4;
    } else if (lk.mat == M_METAL) {
        // Profiled sheet: trapezoidal or corrugated ribs (the normal ripples with them, so the light
        // catches in stripes), lapped every 2.4 m up.
        let pitch = select(0.2, 0.076, f_h(s, 62.0, 1.0) < 0.4);
        let ph = uv.x / pitch * F_TAU;
        let det = f_fade_wave(fw.x, pitch);
        let lap = f_pulse(uv.y / 2.4, 0.0, 0.03, fw.y / 2.4);
        L.tilt = vec2(0.55 * sin(ph) * det, -0.6 * lap * f_fade(fw.y, 0.07));
        L.albedo = c * (1.0 - 0.12 * cos(ph) * det) * (1.0 - 0.35 * lap);
        L.rough = 0.5;
        L.metal = 0.15;
    } else if (lk.mat == M_PANEL) {
        // The colony's panels: white, fine joints.
        let joint = max(f_pulse(uv.x / 1.5, 0.0, 0.0134, fw.x / 1.5), f_pulse(uv.y / 1.2, 0.0, 0.0167, fw.y / 1.2));
        let tone = (f_h(floor(uv.x / 1.5), floor(uv.y / 1.2), s) - 0.5) * 0.05 * f_fade(fm, 1.2);
        L.albedo = c * (1.0 + tone) * (1.0 - 0.6 * joint);
        L.rough = 0.35;
    } else if (lk.mat == M_TIMBER) {
        // Boards up the wall, each its own shade, with the grain.
        let bx = uv.x / 0.14;
        let cover = f_pulse(bx, 0.06, 1.0, fw.x / 0.14);
        let tone = (f_h(floor(bx), s, 3.0) - 0.5) * 0.3 * f_fade(fw.x, 0.14);
        let grain = (noise3(vec3(uv.x * 40.0, uv.y * 1.5, s)) - 0.5) * 0.2 * f_fade(fw.x, 0.03);
        L.albedo = mix(c * 0.35, c * (1.0 + tone + grain), cover);
    } else {
        // Render: smooth, a fine trowelled texture.
        let fine = (noise3(vec3(uv * 3.0, s + 1.0)) - 0.5) * 0.08 * f_fade(fm, 0.33);
        L.albedo = c * (1.0 + fine);
    }
    return L;
}

// ------------------------------------------------------------------------------- rooms, windows

// What a room shows at a ray: its surface lit by daylight through the window (albedo times a
// falloff from the glass), and by its own lamps (albedo times the lamp's falloff, plus the
// fixtures' glow), each per unit of light.
struct FRoomHit {
    day: vec3<f32>,
    lamp: vec3<f32>,
};

// A room behind a window, by interior mapping: the ray enters at `o` (m, in the glass's plane: x
// along, y up, z 0) going `d` (z into the room) and meets the box `lo..hi` (x, y; z from 0 to
// `depth`), or before it a piece of furniture against the back wall (a shop's display at the
// window), or now and then someone standing in the room.
fn f_room(o: vec3<f32>, d: vec3<f32>, lo: vec2<f32>, hi: vec2<f32>, depth: f32, kind: u32, rs: f32, fp: f32) -> FRoomHit {
    let sd = select(vec3(-1.0), vec3(1.0), d >= vec3(0.0));
    let dd = sd * max(abs(d), vec3(1e-4));
    let inv = 1.0 / dd;
    let tx = (select(lo.x, hi.x, dd.x > 0.0) - o.x) * inv.x;
    let ty = (select(lo.y, hi.y, dd.y > 0.0) - o.y) * inv.y;
    let tz = (depth - o.z) * inv.z;
    var t = tz;
    // 0 the back wall, 1 a side wall, 2 the floor, 3 the ceiling, 4 furniture, 5 someone.
    var face = 0u;
    if (tx < t) {
        t = tx;
        face = 1u;
    }
    if (ty < t) {
        t = ty;
        face = select(2u, 3u, dd.y > 0.0);
    }
    let r1 = f_h(rs, 1.0, 3.0);
    let r2 = f_h(rs, 2.0, 3.0);
    let r3 = f_h(rs, 3.0, 3.0);
    // What's in it: a sofa, a desk, a bed against the back wall; a shop's display at the window.
    var b0 = vec3(mix(lo.x, hi.x, 0.1 + 0.4 * r2), lo.y, depth - 0.6 - 0.8 * r1);
    var b1 = vec3(min(b0.x + 0.9 + 1.4 * r3, hi.x - 0.1), lo.y + 0.45 + 0.6 * r3, depth);
    if (kind == R_SHOP) {
        b0 = vec3(lo.x + 0.25, lo.y, 0.15);
        b1 = vec3(hi.x - 0.25, lo.y + 0.75 + 0.3 * r3, 0.85);
    }
    let t0 = (b0 - o) * inv;
    let t1 = (b1 - o) * inv;
    let tn = min(t0, t1);
    let tf = max(t0, t1);
    let tin = max(max(tn.x, tn.y), tn.z);
    let tout = min(min(tf.x, tf.y), tf.z);
    var furniture_top = false;
    if (tin <= tout && tin > 0.0 && tin < t && kind != R_HALL) {
        t = tin;
        face = 4u;
        furniture_top = tn.y >= max(tn.x, tn.z);
    }
    // Now and then someone in a lit room, between the window and the back wall.
    if (r2 > 0.86 && kind != R_WORKS) {
        let zp = depth * (0.3 + 0.4 * r1);
        let tp = (zp - o.z) * inv.z;
        if (tp > 0.0 && tp < t) {
            let q = o + dd * tp;
            let x = abs(q.x - mix(lo.x + 0.4, hi.x - 0.4, r3));
            let y = q.y - lo.y;
            let body = step(x, 0.2) * step(y, 1.42);
            let head = step(length(vec2(x, y - 1.6)), 0.12);
            if (max(body, head) > 0.5) {
                t = tp;
                face = 5u;
            }
        }
    }
    // Never behind the glass: a filtered edge pixel just outside an opening with no reveal (a
    // tower's vision glass, an open roller door) can start outside the box heading away from it,
    // and a negative t would run the daylight's falloff below up to infinity.
    t = max(t, 0.0);
    let hp = o + dd * t;
    // Its colours.
    var wall = f_room_wall(r1);
    var floor_c = select(vec3(0.3, 0.19, 0.1), vec3(0.22, 0.22, 0.24), r2 < 0.4);
    var ceil_c = vec3(0.78, 0.77, 0.74);
    if (kind == R_OFFICE) {
        wall = vec3(0.62, 0.62, 0.6);
        floor_c = vec3(0.18, 0.19, 0.21);
    } else if (kind == R_SHOP) {
        wall = mix(vec3(0.75, 0.74, 0.7), f_room_wall(r1), 0.4);
        floor_c = vec3(0.45, 0.44, 0.42);
    } else if (kind == R_HALL) {
        wall = vec3(0.7, 0.68, 0.64);
        floor_c = vec3(0.5, 0.48, 0.44);
    } else if (kind == R_WORKS) {
        wall = vec3(0.3, 0.31, 0.3);
        floor_c = vec3(0.2, 0.2, 0.19);
        ceil_c = vec3(0.25, 0.25, 0.25);
    }
    var alb = wall;
    if (face == 1u) {
        alb = wall * 0.85;
    } else if (face == 2u) {
        alb = floor_c;
    } else if (face == 3u) {
        alb = ceil_c;
    } else if (face == 4u) {
        alb = f_furniture(r3) * select(0.7, 1.0, furniture_top);
        if (kind == R_SHOP) {
            // Goods on the display, all sorts.
            let goods = mix(vec3(0.5, 0.48, 0.45), f_neon(f_h(floor(hp.x / 0.45), rs, 5.0)) * 0.45, 0.6 * step(0.35, f_h(floor(hp.x / 0.45), rs, 6.0)));
            alb = mix(vec3(0.48, 0.46, 0.43), goods, f_fade(fp, 0.45));
        }
    } else if (face == 5u) {
        alb = vec3(0.025, 0.022, 0.02);
    }
    let yr = hp.y - lo.y;
    if (face <= 1u) {
        if (kind == R_SHOP) {
            // Shelves of goods along the walls.
            let along = select(hp.x, hp.z, face == 1u);
            let shelf = floor(yr / 0.5);
            let item = floor(along / 0.4);
            let goods = mix(vec3(0.42, 0.4, 0.37), f_neon(f_h(item, shelf, rs)) * 0.45, 0.55) * (0.6 + 0.5 * f_h(shelf, item, rs));
            let edge = step(fract(yr / 0.5), 0.1);
            let rows = step(0.3, yr) * step(yr, 2.3);
            let shelves = mix(vec3(0.42, 0.4, 0.38), mix(goods, vec3(0.72), edge), f_fade(fp, 0.25));
            alb = mix(alb, shelves, rows);
        } else if (kind == R_OFFICE) {
            // Desks and partitions along the walls, the odd screen.
            let desk = step(yr, 1.15);
            alb = mix(alb, vec3(0.16, 0.16, 0.17), desk);
        } else if (kind == R_WORKS) {
            // Racking: orange beams, boxes on them.
            let beam = step(fract(yr / 1.5), 0.08);
            let boxes = step(0.5, f_h(floor(select(hp.x, hp.z, face == 1u) / 1.2), floor(yr / 1.5), rs));
            alb = mix(mix(alb, vec3(0.35, 0.25, 0.12), boxes * step(yr, 4.5)), vec3(0.55, 0.22, 0.03), beam);
        } else if (face == 0u && kind == R_HOME) {
            // A door, a picture, a shelf of books on the back wall.
            let dx = hp.x - mix(lo.x + 0.3, hi.x - 1.2, r3);
            let door = step(0.0, dx) * step(dx, 0.85) * step(yr, 2.05);
            let px = hp.x - mix(lo.x + 0.2, hi.x - 1.0, r1);
            let pic = step(0.0, px) * step(px, 0.7) * step(1.35, yr) * step(yr, 1.85);
            alb = mix(alb, alb * 0.6, door);
            alb = mix(alb, f_neon(r2) * 0.35, pic * (1.0 - door));
        }
    }
    // The lamps: a pendant in a home, panels in a grid in an office or a shop, strip lights in a
    // works, big pendants in a hall.
    let lamp_p = vec3((lo.x + hi.x) * 0.5, hi.y - 0.3, depth * 0.45);
    let dl = hp - lamp_p;
    var illum = 0.3 + 1.4 / (1.0 + dot(dl, dl) * 0.3);
    var glow = 0.0;
    if (kind == R_OFFICE || kind == R_SHOP) {
        illum = 0.95;
    }
    if (face == 3u) {
        illum *= 0.55;
        if (kind == R_OFFICE || kind == R_SHOP) {
            let g = abs(fract(vec2(hp.x / 1.8, hp.z / 1.8)) - vec2(0.5));
            glow = step(g.x, 0.17) * step(g.y, 0.33) * 5.0;
        } else if (kind == R_WORKS) {
            glow = step(abs(fract(hp.z / 3.0) - 0.5), 0.04) * 8.0;
        } else {
            glow = (1.0 - smoothstep(0.15, 0.3, length(vec2(dl.x, dl.z)))) * 6.0;
        }
    }
    if (face == 5u) {
        illum = 0.3;
    }
    // Daylight falls off from the glass.
    let dayf = (0.25 + 0.75 * exp(-max(hp.z, 0.0) * 0.3)) * select(1.0, 1.25, face == 2u);
    return FRoomHit(alb * dayf, alb * illum + vec3(glow));
}

// A window: its opening, the reveal round it, the frame, what's drawn across it, and the room.
struct FWin {
    // The opening (m), and how far back in it the glass is.
    size: vec2<f32>,
    reveal: f32,
    // The room's box from the opening's bottom-left (m), its depth, kind and seed.
    lo: vec2<f32>,
    hi: vec2<f32>,
    depth: f32,
    room: u32,
    rs: f32,
    frame: vec3<f32>,
    // The reveal's colour.
    trim: vec3<f32>,
    // Panes across; a transom's height over the sill (0: none).
    panes: f32,
    transom: f32,
    // Blinds drawn down from the head (0..1); curtains drawn in from each side (0..0.5).
    blinds: f32,
    curtain: f32,
    curtain_c: vec3<f32>,
    // The lamps tonight (colour × nits on white × lit), and daylight into the room (nits on white).
    light: vec3<f32>,
    day: f32,
    // What the glass lets through; its own colour and how mirror-like it is; the frame's width
    // (0: none, as in a curtain wall or an open doorway).
    tint: vec3<f32>,
    glass: vec3<f32>,
    refl: f32,
    fwid: f32,
};

// The one opening at a pixel with a room behind it: where the ray meets the wall (m from the
// opening's bottom-left), the window, and how much of its detail to draw. Each facade picks its
// opening and calls `f_window` once, so the room is built once in the shader, not at every kind of
// opening (a software rasteriser, and ANGLE's compilers, inline every call).
struct FOpen {
    on: bool,
    o: vec3<f32>,
    w: FWin,
    dwin: f32,
};

// A window up close, from where the ray `d` meets the wall at `o` (m from the opening's
// bottom-left, z 0), with `fw` the footprint (m).
fn f_window(o: vec3<f32>, d: vec3<f32>, w: FWin, fw: vec2<f32>) -> FLayer {
    var L = f_layer(vec3(0.02), 0.05);
    L.refl = 0.85;
    let sd = select(vec2(-1.0), vec2(1.0), d.xy >= vec2(0.0));
    let dxy = sd * max(abs(d.xy), vec2(1e-4));
    // The reveal: the ray meets a jamb, the sill or the head before it reaches the glass.
    let tg = w.reveal / d.z;
    let tx = (select(0.0, w.size.x, dxy.x > 0.0) - o.x) / dxy.x;
    let ty = (select(0.0, w.size.y, dxy.y > 0.0) - o.y) / dxy.y;
    if (w.reveal > 0.0 && min(tx, ty) < tg) {
        L.albedo = w.trim * 0.85;
        L.rough = 0.85;
        L.refl = 0.0;
        L.occ = 0.7;
        if (tx < ty) {
            // A jamb faces across the opening.
            L.tilt = vec2(-6.0 * sd.x, 0.0);
        } else {
            // The sill faces up, the head down.
            L.tilt = vec2(0.0, -6.0 * sd.y);
        }
    } else {
        L = f_glass(o + d * tg, d, w, fw);
    }
    return L;
}

// The glass of a window at `g` (m from the opening's bottom-left, in the glass's plane): its
// frame, what's drawn across it, and the room behind.
fn f_glass(g: vec3<f32>, d: vec3<f32>, w: FWin, fw: vec2<f32>) -> FLayer {
    var L = f_layer(w.glass, 0.05);
    let fm = max(fw.x, fw.y);
    // The frame round the glass, its mullions and transom.
    let fwid = w.fwid;
    let edge = min(min(g.x, w.size.x - g.x), min(g.y, w.size.y - g.y));
    var bar = select(0.0, 1.0 - smoothstep(fwid, fwid + fm, edge), fwid > 0.0);
    if (w.panes > 1.5) {
        let m = abs(fract(g.x / w.size.x * w.panes + 0.5) - 0.5) * w.size.x / w.panes;
        bar = max(bar, 1.0 - smoothstep(fwid * 0.5, fwid * 0.5 + fw.x, m));
    }
    if (w.transom > 0.0) {
        bar = max(bar, 1.0 - smoothstep(fwid * 0.5, fwid * 0.5 + fw.y, abs(g.y - w.transom)));
    }
    // Blinds and curtains, just behind the glass, lit from both sides.
    let fx = g.x / w.size.x;
    let fy = g.y / w.size.y;
    let blind = clamp((fy - (1.0 - w.blinds)) * w.size.y / max(fw.y, 1e-3) + 0.5, 0.0, 1.0) * step(0.01, w.blinds);
    let slats = 1.0 - 0.35 * f_pulse(g.y / 0.04, 0.0, 0.25, fw.y / 0.04);
    let fwx = fw.x / w.size.x;
    let curtain = max(f_band(fx, -1.0, w.curtain, fwx), f_band(fx, 1.0 - w.curtain, 2.0, fwx)) * step(0.01, w.curtain);
    let folds = 0.75 + 0.25 * sin(g.x * 37.0) * f_fade_wave(fw.x, 0.17);
    // The room.
    let hit = f_room(vec3(g.xy, 0.0), d, w.lo, w.hi, w.depth, w.room, w.rs, fm);
    var em = (hit.lamp * w.light + hit.day * w.day) * w.tint;
    let back = (w.light * 0.3 + vec3(w.day * 0.2)) * w.tint;
    let blind_c = vec3(0.72, 0.7, 0.64);
    em = mix(em, blind_c * back * slats, blind);
    em = mix(em, w.curtain_c * back * folds * 1.4, curtain);
    L.albedo = mix(L.albedo, blind_c * 0.4 * slats, blind);
    L.albedo = mix(L.albedo, w.curtain_c * 0.4 * folds, curtain);
    L.emissive = em * (1.0 - bar);
    L.albedo = mix(L.albedo, w.frame, bar);
    L.rough = mix(0.05, 0.5, bar);
    L.refl = mix(w.refl, 0.05, bar);
    return L;
}

// A window's usual furnishing, from its room's seed.
fn f_win(lk: FLook, size: vec2<f32>, lo: vec2<f32>, hi: vec2<f32>, rs: f32) -> FWin {
    var w: FWin;
    w.size = size;
    w.reveal = lk.reveal;
    w.lo = lo;
    w.hi = hi;
    w.depth = 3.5 + 3.0 * f_h(rs, 5.0, 1.0);
    w.room = lk.room;
    w.rs = rs;
    w.frame = lk.frame;
    w.trim = select(lk.base, lk.trim, lk.cornice);
    w.panes = select(1.0, 2.0, size.x > 1.2) + select(0.0, 1.0, size.x > 2.2);
    w.transom = 0.0;
    w.blinds = select(0.0, 0.1 + 0.7 * f_h(rs, 7.0, 1.0), f_h(rs, 8.0, 1.0) < 0.45);
    w.curtain = select(0.0, 0.12 + 0.2 * f_h(rs, 9.0, 1.0), f_h(rs, 10.0, 1.0) < 0.35 && lk.room == R_HOME);
    w.curtain_c = f_curtain_colour(f_h(rs, 11.0, 1.0));
    w.light = vec3(0.0);
    w.day = 0.0;
    w.tint = vec3(0.78, 0.8, 0.8);
    w.glass = vec3(0.02);
    w.refl = 0.85;
    w.fwid = 0.055;
    return w;
}

#endif

// A window from afar: dark glass, its room's mean light (`light` as in `FWin`).
fn f_far_glass(light: vec3<f32>, day: f32) -> FLayer {
    var L = f_layer(vec3(0.025), 0.08);
    L.refl = 0.7;
    L.emissive = (light * 0.42 + vec3(0.9, 0.95, 1.0) * day * 0.3) * 0.8;
    return L;
}

#ifndef FACADE_LOW
// ------------------------------------------------------------------------ signs and shutters

// A shop's sign on its fascia: a painted board with lettering (blocky glyphs, no real text), lit
// at night on some. `q` is metres along and up the board from its bottom-left.
fn f_sign(q: vec2<f32>, size: vec2<f32>, fw: vec2<f32>, ss: f32, night: f32) -> FLayer {
    let bg = f_sign_colour(f_h(ss, 1.0, 9.0));
    let lum = dot(bg, vec3(0.3, 0.55, 0.15));
    var ink = select(vec3(0.75, 0.72, 0.62), vec3(0.04), lum > 0.35);
    if (lum < 0.1 && f_h(ss, 8.0, 9.0) < 0.4) {
        // Gold leaf on a dark board.
        ink = vec3(0.6, 0.42, 0.12);
    }
    let lh = 0.42 * size.y;
    let lw = 0.62 * lh;
    let pitch = lw * 1.3;
    let n = max(min(3.0 + floor(f_h(ss, 2.0, 9.0) * 8.0), floor((size.x - 0.6) / pitch)), 0.0);
    let x0 = 0.5 * (size.x - n * pitch);
    let y0 = 0.5 * (size.y - lh);
    let tx = (q.x - x0) / pitch;
    let li = floor(tx);
    let lx = fract(tx) * 1.3;
    let ly = (q.y - y0) / lh;
    var code = u32(f_h(li, ss, 3.0) * 127.0);
    if ((code & 7u) == 0u) {
        code = code | 1u;
    }
    if ((code & 56u) == 0u) {
        code = code | 8u;
    }
    let blank = f_h(li, ss, 4.0) < 0.12;
    let sw = 0.18;
    let fx = fw.x / lw;
    let fy = fw.y / lh;
    let full_x = f_band(lx, 0.0, 1.0, fx);
    let full_y = f_band(ly, 0.0, 1.0, fy);
    var g = 0.0;
    if ((code & 1u) != 0u) { g = max(g, f_band(lx, 0.0, sw, fx) * full_y); }
    if ((code & 2u) != 0u) { g = max(g, f_band(lx, 1.0 - sw, 1.0, fx) * full_y); }
    if ((code & 4u) != 0u) { g = max(g, f_band(lx, 0.5 - 0.5 * sw, 0.5 + 0.5 * sw, fx) * full_y); }
    if ((code & 8u) != 0u) { g = max(g, f_band(ly, 1.0 - sw, 1.0, fy) * full_x); }
    if ((code & 16u) != 0u) { g = max(g, f_band(ly, 0.5 - 0.5 * sw, 0.5 + 0.5 * sw, fy) * full_x); }
    if ((code & 32u) != 0u) { g = max(g, f_band(ly, 0.0, sw, fy) * full_x); }
    if ((code & 64u) != 0u) { g = max(g, f_band(lx - ly, -0.6 * sw, 0.6 * sw, fx + fy) * full_x * full_y); }
    let in_text = f_band(q.x, x0, x0 + n * pitch, fw.x) * f_band(q.y, y0, y0 + lh, fw.y) * select(1.0, 0.0, blank);
    // From afar the lettering is a line of its average.
    let det = 1.0 - smoothstep(0.15, 0.5, max(fx, fy) / sw);
    let cov = mix(0.36 * f_band(q.x, x0, x0 + n * pitch, fw.x) * f_band(q.y, y0, y0 + lh, fw.y), g * in_text, det);
    var L = f_layer(mix(bg, ink, cov), 0.5);
    let lit = select(0.0, night, f_h(ss, 5.0, 9.0) < 0.55);
    if (f_h(ss, 7.0, 9.0) < 0.5) {
        // Lettering lit from behind, or neon.
        L.emissive = f_neon(f_h(ss, 6.0, 9.0)) * cov * 900.0 * lit;
    } else {
        // A light box.
        L.emissive = (bg * 0.8 + vec3(0.2)) * (1.0 - cov) * 260.0 * lit;
    }
    let edge = min(min(q.x, size.x - q.x), min(q.y, size.y - q.y));
    L.albedo = mix(L.albedo, bg * 0.5, 1.0 - smoothstep(0.03, 0.03 + max(fw.x, fw.y), edge));
    return L;
}

// A roller shutter, down: slats, dirt at the foot, and on some a tag sprayed across it.
fn f_shutter(q: vec2<f32>, fw: vec2<f32>, c: vec3<f32>, wear: f32, tags: bool, ss: f32) -> FLayer {
    let ph = q.y / 0.09 * F_TAU;
    let det = f_fade_wave(fw.y, 0.09);
    // The groove between slats, box-filtered (so it settles to its average rather than beating
    // against the pixels): the slats still read once their rounded faces have faded.
    let groove = f_pulse(q.y / 0.09, 0.0, 0.16, fw.y / 0.09) * f_fade(fw.y, 0.09);
    var L = f_layer(c * (0.9 + 0.1 * cos(ph) * det) * (1.0 - 0.3 * groove), 0.5);
    L.metal = 0.45;
    L.tilt = vec2(0.0, 0.6 * sin(ph) * det);
    L.albedo *= 1.0 - 0.4 * wear * (1.0 - smoothstep(0.0, 1.2, q.y));
    if (tags) {
        // Tags: a marker's wiggling line at shoulder height, in a patch or two; now and then a
        // filled piece with its outline.
        let fm = max(fw.x, fw.y);
        let band = f_band(q.y, 0.5, 2.1, fw.y) * smoothstep(0.42, 0.55, noise3(vec3(q.x * 0.45, 0.0, ss + 3.0)));
        let wig = noise3(vec3(q.x * 2.6, q.y * 3.4, ss));
        let line = (1.0 - smoothstep(0.012, 0.03 + fm * 2.0, abs(wig - 0.5))) * band * f_fade(fm, 0.04);
        let piece = noise3(vec3(q.x * 2.4, q.y * 3.2, ss + 9.0)) * band;
        let fill = smoothstep(0.64, 0.66, piece) * step(f_h(ss, 3.0, 1.0), 0.3) * f_fade(fm, 0.15);
        let outline = (smoothstep(0.6, 0.62, piece) - smoothstep(0.64, 0.66, piece)) * step(f_h(ss, 3.0, 1.0), 0.3) * f_fade(fm, 0.08);
        L.albedo = mix(L.albedo, f_neon(f_h(ss, 4.0, 1.0)) * 0.55, fill);
        L.albedo = mix(L.albedo, mix(vec3(0.03), f_neon(f_h(ss, 6.0, 1.0)) * 0.5, step(0.5, f_h(ss, 7.0, 1.0))), max(line, outline) * 0.85);
        L.metal *= 1.0 - max(fill, line);
    }
    return L;
}

// ------------------------------------------------------------------------------- ground floors

// The ground floor at a pixel, for `f_wall`: everything but its opening's glass, and the opening
// (a shop's window or door, a roller door, a lobby's glass, a stone base's window, a flat's
// window): how much of the pixel it covers, how it looks from afar (glass, a shutter, a dark bay),
// the room behind it close up, and the shade over it.
struct FGround {
    L: FLayer,
    cov: f32,
    far: FLayer,
    open: FOpen,
    shade: f32,
};

// The ground floor (up to `F_GROUND`), over the wall's own material `wall`: shopfronts with their
// rooms, signs, awnings and shutters; a works' roller doors; an office's lobby; a civic
// building's stone base; flats' front doors and windows.
fn f_ground(i: FacadeIn, fr: FFrame, lk: FLook, s: f32, wall: FLayer) -> FGround {
    let uv = i.uv;
    let fw = fr.fw;
    let fm = max(fw.x, fw.y);
    let h = uv.y;
    let night = i.lamps;
    let day = i.daylight * 900.0;
    var sb = 4.6 + 2.0 * f_h(s, 21.0, 1.0);
    if (lk.ground == G_WORKS) {
        sb = 6.5;
    }
    if (lk.ground == G_STONE || lk.ground == G_FLATS) {
        sb = lk.bay;
    }
    if (lk.ground == G_LOBBY) {
        sb = 6.0;
    }
    let cu = uv.x / sb;
    let c = floor(cu);
    let fu = (cu - c) * sb;
    let bt = f_h(c, s, 23.0);
    let rs = f_h(c, s, 24.0);
    let pil = 0.5;
    let inner = f_pulse(cu, 0.5 * pil / sb, 1.0 - 0.5 * pil / sb, fw.x / sb);
    // Rooms are drawn while their windows span ten pixels or so, and fade out by three.
    let dwin = 1.0 - smoothstep(0.08, 0.3, fm / 1.2);
    var gr: FGround;
    gr.L = wall;
    gr.cov = 0.0;
    gr.far = f_far_glass(vec3(0.0), day);
    gr.open.on = false;
    gr.open.dwin = dwin;
    gr.shade = 1.0;

    if (lk.ground == G_BLANK) {
        // A monument's plain stone.
    } else if (lk.ground == G_SHOPS) {
        // A plinth under the window, the window and its door, the fascia with the sign.
        let dx0 = select(sb - 0.5 * pil - 1.3, 0.5 * pil + 0.1, f_h(c, s, 41.0) < 0.5);
        let in_door = f_band(fu, dx0, dx0 + 1.2, fw.x);
        let c_win = f_band(h, 0.55, 3.0, fw.y) * inner * (1.0 - in_door);
        let c_door = f_band(h, F_KERB, 2.7, fw.y) * in_door;
        let c_plinth = f_band(h, F_KERB, 0.55, fw.y) * inner * (1.0 - in_door);
        let c_fascia = f_band(h, 3.3, 4.25, fw.y) * inner;
        gr.L = f_blend(gr.L, f_layer(lk.trim * 0.55, 0.6), c_plinth);
        // Shuttered shops: closed at night, a few by day too.
        let shuttered = bt < select(0.2, 0.45, lk.mat == M_METAL || i.strip == 1u);
        let closed = shuttered && f_h(c, s, 31.0) < mix(0.25, 0.85, night);
        let lit = select(0.0, 1.0, f_h(c, s, 29.0) < mix(1.0, lk.shop_lit, night)) * select(1.0, 0.0, closed);
        gr.cov = min(c_win + c_door, 1.0);
        if (closed) {
            gr.far = f_shutter(vec2(fu, h), fw, vec3(0.42, 0.43, 0.44), lk.wear, i.strip == 1u, s + c);
        } else {
            let k = f_lamp_colour(clamp(0.5 + (rs - 0.5) * 1.2, 0.0, 1.0));
            let light = k * (520.0 + 330.0 * f_h(rs, 3.0, 2.0)) * lit;
            gr.far = f_far_glass(light, day);
            var o = vec3(fu - 0.5 * pil, h - 0.55, 0.0);
            var w = f_win(lk, vec2(sb - pil, 2.45), vec2(0.0, F_KERB - 0.55), vec2(sb - pil, 3.35), rs);
            w.panes = select(2.0, 3.0, sb > 5.6);
            w.transom = 2.0;
            if (in_door > 0.5) {
                o = vec3(fu - dx0, h - F_KERB, 0.0);
                w.size = vec2(1.2, 2.7 - F_KERB);
                w.lo = vec2(0.5 * pil - dx0, 0.0);
                w.hi = vec2(sb - 0.5 * pil - dx0, 3.75);
                w.panes = 1.0;
                w.transom = 2.1;
            }
            w.reveal = 0.1;
            w.depth = 7.0 + 4.0 * f_h(rs, 4.0, 2.0);
            w.room = R_SHOP;
            w.frame = f_frame_colour(f_h(rs, 5.0, 2.0));
            w.trim = lk.trim;
            w.blinds = 0.0;
            w.curtain = 0.0;
            w.light = light;
            w.day = day;
            w.tint = vec3(0.85);
            gr.open.on = dwin > 0.002;
            gr.open.o = o;
            gr.open.w = w;
        }
        if (c_fascia > 0.002) {
            gr.L = f_blend(gr.L, f_sign(vec2(fu - 0.5 * pil, h - 3.3), vec2(sb - pil, 0.95), fw, f_h(c, s, 43.0), night), c_fascia);
        }
        if (f_h(c, s, 37.0) < lk.awning) {
            // A painted awning, scalloped, over the window; its shade on the glass under it.
            let scallop = 0.05 * abs(sin(fu * 12.566)) * f_fade_wave(fw.x, 0.25);
            let c_aw = f_band(h, 2.85 + scallop, 3.55, fw.y) * inner;
            let stripe = f_pulse(fu / 0.5, 0.0, 0.5, fw.x / 0.5) * step(0.4, f_h(c, s, 49.0));
            var awl = f_layer(mix(f_awning_colour(f_h(c, s, 47.0)), vec3(0.72, 0.7, 0.66), stripe), 0.85);
            awl.tilt = vec2(0.0, 1.2);
            gr.L = f_blend(gr.L, awl, c_aw);
            gr.cov *= 1.0 - c_aw;
            gr.shade = 1.0 - 0.5 * f_band(h, 2.2, 2.85, fw.y) * inner;
            gr.L.occ *= gr.shade;
        }
    } else if (lk.ground == G_WORKS) {
        // Roller doors, a door and a window, or plain wall; a bulkhead lamp over each door.
        let door_c = f_cladding(f_h(s, 91.0, 1.0));
        let sodium = vec3(1.0, 0.55, 0.18);
        if (bt < 0.55) {
            gr.cov = f_band(fu, 0.6, sb - 0.6, fw.x) * f_band(h, F_KERB, 4.3, fw.y);
            // Some stand open a way, the bay inside lit by its strip lights.
            let up = select(0.0, 2.6, f_h(c, s, 93.0) < 0.25);
            if (h < F_KERB + up) {
                let light = f_lamp_colour(0.8) * 420.0 * max(night, 0.3);
                gr.far = f_layer(vec3(0.02), 0.9);
                gr.far.emissive = light * 0.3;
                var w = f_win(lk, vec2(sb - 1.2, up), vec2(0.0), vec2(sb - 1.2, 6.0), rs);
                w.reveal = 0.0;
                w.fwid = 0.0;
                w.panes = 1.0;
                w.depth = 14.0;
                w.room = R_WORKS;
                w.blinds = 0.0;
                w.curtain = 0.0;
                w.light = light;
                w.day = day;
                w.tint = vec3(1.0);
                w.refl = 0.0;
                gr.open.on = dwin > 0.002;
                gr.open.o = vec3(fu - 0.6, h - F_KERB, 0.0);
                gr.open.w = w;
            } else {
                gr.far = f_shutter(vec2(fu, h), fw, door_c, lk.wear, i.strip == 1u || bt < 0.2, s + c);
            }
            gr.L = f_blend(gr.L, f_layer(door_c * 0.6, 0.6), f_band(h, 4.3, 4.7, fw.y) * f_band(fu, 0.45, sb - 0.45, fw.x));
            gr.L.emissive += sodium * f_lamp_dot(vec2(fu - 0.5 * sb, h - 4.95), 0.16, fw) * 3000.0 * night;
        } else if (bt < 0.8) {
            let c_md = f_band(fu, 1.0, 2.0, fw.x) * f_band(h, F_KERB, 2.25, fw.y);
            gr.L = f_blend(gr.L, f_layer(door_c * 0.8, 0.6), c_md);
            let c_sw = f_band(fu, 3.0, 5.2, fw.x) * f_band(h, 1.3, 2.4, fw.y);
            var sw = f_layer(vec3(0.03), 0.1);
            sw.refl = 0.6;
            sw.emissive = f_lamp_colour(0.85) * 160.0 * night * step(f_h(c, s, 95.0), 0.4);
            gr.L = f_blend(gr.L, sw, c_sw);
            gr.L.emissive += sodium * f_lamp_dot(vec2(fu - 1.5, h - 2.6), 0.12, fw) * 2500.0 * night;
        }
    } else if (lk.ground == G_LOBBY) {
        // Glass from the floor to the soffit between piers, the lobby lit through the night.
        gr.cov = f_band(h, F_KERB, 4.5, fw.y) * f_pulse(cu, 0.35 / sb, 1.0 - 0.35 / sb, fw.x / sb);
        let light = f_lamp_colour(0.6) * 700.0;
        gr.far = f_far_glass(light, day);
        var w = f_win(lk, vec2(sb - 0.7, 4.5 - F_KERB), vec2(-0.35 - sb, 0.0), vec2(2.0 * sb, 5.5), f_h(s, 97.0, 1.0));
        w.reveal = 0.06;
        w.depth = 12.0;
        w.room = R_HALL;
        w.panes = floor((sb - 0.7) / 1.5);
        w.blinds = 0.0;
        w.curtain = 0.0;
        w.trim = lk.base;
        w.light = light;
        w.day = day;
        w.tint = vec3(0.8, 0.84, 0.86);
        gr.open.on = dwin > 0.002;
        gr.open.o = vec3(fu - 0.35, h - F_KERB, 0.0);
        gr.open.w = w;
    } else if (lk.ground == G_STONE) {
        // Rusticated stone, deep joints every half metre; tall windows, a door in a bay or two.
        let rj = f_pulse(h / 0.5, 0.0, 0.06, fw.y / 0.5);
        gr.L.albedo *= 1.0 - 0.35 * rj;
        gr.L.occ *= 1.0 - 0.3 * rj;
        let is_door = bt < 0.14;
        let wx = select(0.7, 1.0, is_door);
        let wy0 = select(0.9, F_KERB, is_door);
        let wy1 = select(3.9, 3.5, is_door);
        gr.cov = f_band(fu, 0.5 * sb - wx, 0.5 * sb + wx, fw.x) * f_band(h, wy0, wy1, fw.y);
        let lit = select(step(f_h(c, s, 99.0), lk.lit * 1.4) * night, 1.0, is_door);
        let light = f_lamp_colour(lk.warmth) * F_ROOM_NITS * lit;
        gr.far = f_far_glass(light, day);
        var w = f_win(lk, vec2(2.0 * wx, wy1 - wy0), vec2(wx - 0.5 * sb, F_KERB - wy0), vec2(0.5 * sb + wx, 4.6 - wy0), rs);
        w.reveal = 0.3;
        w.depth = 6.0;
        w.room = select(R_OFFICE, R_HALL, is_door);
        w.panes = 2.0;
        w.transom = (wy1 - wy0) * 0.75;
        w.light = light;
        w.day = day;
        gr.open.on = dwin > 0.002;
        gr.open.o = vec3(fu - (0.5 * sb - wx), h - wy0, 0.0);
        gr.open.w = w;
    } else {
        // Flats: front doors, and windows like the floors over them.
        if (bt < 0.3) {
            let mid = 0.5 * sb;
            let c_d = f_band(fu, mid - 0.5, mid + 0.5, fw.x) * f_band(h, F_KERB, 2.35, fw.y);
            var dl = f_layer(f_door_colour(f_h(c, s, 51.0)), 0.45);
            let pnl = f_band(fu, mid - 0.35, mid + 0.35, fw.x) * (f_band(h, 0.4, 1.1, fw.y) + f_band(h, 1.3, 2.1, fw.y));
            dl.albedo *= 1.0 - 0.25 * pnl * f_fade(fm, 0.1);
            gr.L = f_blend(gr.L, dl, c_d);
            // The fanlight over it, lit when someone's in; a hood over it all; a lamp by it.
            let c_f = f_band(fu, mid - 0.5, mid + 0.5, fw.x) * f_band(h, 2.42, 2.8, fw.y);
            var fl = f_layer(vec3(0.03), 0.08);
            fl.refl = 0.7;
            fl.emissive = f_lamp_colour(0.15) * 350.0 * night * step(f_h(c, s, 53.0), 0.6);
            gr.L = f_blend(gr.L, fl, c_f);
            var hood = f_layer(lk.trim, 0.7);
            hood.tilt = vec2(0.0, 1.0);
            gr.L = f_blend(gr.L, hood, f_band(fu, mid - 0.75, mid + 0.75, fw.x) * f_band(h, 2.85, 3.0, fw.y));
            gr.L.emissive += vec3(1.0, 0.7, 0.4) * f_lamp_dot(vec2(fu - (mid + 0.8), h - 2.2), 0.08, fw) * 2500.0 * night;
        } else {
            let wx0 = 0.5 * (sb - lk.win_w);
            gr.cov = f_band(fu, wx0, wx0 + lk.win_w, fw.x) * f_band(h, 1.0, 2.8, fw.y);
            let lit = step(f_h(c, s, 55.0), f_lit_share(lk.lit, s, night));
            let light = f_lamp_colour(clamp(lk.warmth + (rs - 0.5) * 0.9, 0.0, 1.0)) * F_ROOM_NITS * (0.6 + 0.8 * rs) * lit;
            gr.far = f_far_glass(light, day);
            var w = f_win(lk, vec2(lk.win_w, 1.8), vec2(-wx0, F_KERB - 1.0), vec2(sb - wx0, 4.6 - 1.0), rs);
            w.light = light;
            w.day = day;
            gr.open.on = dwin > 0.002;
            gr.open.o = vec3(fu - wx0, h - 1.0, 0.0);
            gr.open.w = w;
        }
    }
    return gr;
}

// ------------------------------------------------------------------------------------- walls

fn f_wall(i: FacadeIn, fr: FFrame) -> FacadeOut {
    let uv = i.uv;
    let fw = fr.fw;
    let fm = max(fw.x, fw.y);
    let h = uv.y;
    // An L3 block's bulk stands for its buildings: a seed of its own every 32 m along it.
    let bulk = i.tall <= 0.0;
    var s = i.seed;
    if (bulk) {
        s = floor(f_h(i.seed, floor(uv.x / 32.0), 5.0) * 255.0);
    }
    let lk = f_look(i.district, i.strip, i.kind, s);
    let top = select(i.tall * F_MAX_HEIGHT, 1e6, bulk);
    let wr = lk.wear;
    var L = f_material(lk, uv, fw, s);

    // Rain runs down in threads: noise along the wall, slow up it.
    let thread = mix(0.5, noise3(vec3(uv.x * 7.0, uv.y * 0.25, s * 0.13)), f_fade(fw.x, 0.14));
    var dirt = 0.0;
    // Grime at the foot: splash-back and dust, the first few metres.
    let splash = mix(0.5, noise3(vec3(uv.x * 1.3, uv.y * 2.1, s + 4.0)), f_fade(fm, 0.5));
    dirt += (1.0 - smoothstep(0.2, 3.0, h)) * (0.55 + 0.45 * splash) * 0.55;
    // Runs from the roof's edge, and the grime under it.
    if (!bulk) {
        let run = 2.0 + 6.0 * mix(0.5, f_h(floor(uv.x / 0.9), s, 3.3), f_fade(fw.x, 0.9));
        dirt += exp(-max(top - h, 0.0) / run) * (0.2 + 1.2 * thread) * 0.45;
    }

    // The opening at this pixel (an upper floor's window here, the ground floor's below), its room
    // drawn once after the wall's wear.
    var op: FOpen;
    op.on = false;
    op.dwin = 0.0;
    var ocov = 0.0;
    var ofar = f_layer(vec3(0.025), 0.08);
    var oshade = 1.0;
    var wobble = vec2(0.0);
    var grime = 1.0;
    if (h >= F_GROUND && lk.win_w > 0.0) {
        let yv = h - F_GROUND;
        let fy = yv / F_STOREY;
        let fl = floor(fy);
        let fv = yv - fl * F_STOREY;
        let cu = uv.x / lk.bay;
        let col = floor(cu);
        let fu = (cu - col) * lk.bay;
        let wx0 = 0.5 * (lk.bay - lk.win_w);
        let wx1 = wx0 + lk.win_w;
        // Balconies have French windows down to the floor.
        let sill = select(lk.sill, 0.1, lk.balcony > 0u);
        let wh = lk.head - sill;
        let fwx = fw.x / lk.bay;
        let fwy = fw.y / F_STOREY;
        // A storey whose windows wouldn't fit under the roof has none (a park's pavilion is 4 to 7 m
        // tall, not whole storeys); buildings of the rules end on a storey, so theirs all fit.
        let fits = select(0.0, 1.0, F_GROUND + fl * F_STOREY + lk.head <= top - 0.1);
        let cov = f_pulse(cu, wx0 / lk.bay, wx1 / lk.bay, fwx) * f_pulse(fy, sill / F_STOREY, lk.head / F_STOREY, fwy) * fits;
        ocov = cov;
        // The rooms lit tonight: a share of the building's, more on some floors and in some corners;
        // each window its own until they're finer than a pixel, then their floor's, their corner's,
        // and the building's share.
        let q = f_lit_share(lk.lit, s, i.lamps);
        let q_c = clamp(q * (0.25 + 1.5 * f_h(floor(fl / 3.0), floor(col / 4.0), s + 0.3)), 0.0, 1.0);
        let q_f = clamp(q_c * (0.3 + 1.4 * f_h(fl, floor(col / 4.0), s + 0.7)), 0.0, 1.0);
        let lit_w = step(f_h(col, fl, s + 0.37), q_f);
        let a_w = smoothstep(0.5, 1.5, fwx);
        let a_f = smoothstep(0.5, 1.5, fwy);
        let a_c = smoothstep(0.6, 1.8, max(fwx * 0.25, fwy * 0.333));
        let lit = mix(mix(mix(lit_w, q_f, a_w), q_c, a_f), q, a_c);
        // Its lamp: warm, neutral or cool, the odd television.
        let rs = f_h(col, fl, s + 0.11);
        let k_room = clamp(lk.warmth + (f_h(col, fl, s + 0.53) - 0.5) * 0.9, 0.0, 1.0);
        let avg = max(a_w, a_f);
        var lamp = f_lamp_colour(mix(k_room, lk.warmth, avg)) * F_ROOM_NITS * (0.6 + 0.8 * mix(rs, 0.5, avg));
        if (f_h(fl, col, s + 0.71) < 0.07 && avg < 0.5) {
            lamp = vec3(0.45, 0.6, 1.0) * F_ROOM_NITS * 0.35 * (0.55 + 0.45 * noise3(vec3(i.seconds * 4.0, rs * 50.0, 0.0)));
        }
        let light = lamp * lit;
        let day = i.daylight * 900.0;
        ofar = f_far_glass(light * mix(1.0, F_FAR_GLOW, avg), day);
        // Up close, the room behind (while the window spans ten pixels or so; gone by three).
        let dwin = 1.0 - smoothstep(0.1, 0.33, max(fw.x / lk.win_w, fw.y / wh));
        if (cov > 0.002 && dwin > 0.002) {
            var w = f_win(lk, vec2(lk.win_w, wh), vec2(-wx0, -sill), vec2(lk.bay - wx0, F_STOREY - sill - 0.25), rs);
            w.transom = select(0.0, wh * 0.72, f_h(s, 71.0, 1.0) < 0.5);
            w.light = light;
            w.day = day;
            op.on = true;
            op.o = vec3(fu - wx0, fv - sill, 0.0);
            op.w = w;
            op.dwin = dwin;
            // Panes aren't quite flat: each catches the sky a little differently.
            wobble = (vec2(f_h(rs, 12.0, 1.0), f_h(rs, 13.0, 1.0)) - 0.5) * 0.03 * dwin;
        }
        // The windows, a little grimy themselves.
        grime = mix(1.0, 0.85, wr);
        // Rain runs from the sills (and the soot of old stoves over the heads), in each window's
        // column.
        let sx = f_pulse(cu, (wx0 - 0.1) / lk.bay, (wx1 + 0.1) / lk.bay, fwx) * fits;
        let below = select(F_STOREY - fv + sill, sill - fv, fv < sill);
        dirt += sx * exp(-below / (0.6 + 1.6 * wr)) * (0.3 + 1.1 * thread) * 0.6 * (1.0 - cov);
        dirt += sx * lk.soot * exp(-max(fv - lk.head, 0.0) / 0.35) * step(lk.head, fv) * (0.15 + 0.75 * thread) * 0.6;
        // Sills and, on dressed buildings, lintels.
        if (lk.balcony == 0u) {
            let tx = f_pulse(cu, (wx0 - 0.08) / lk.bay, (wx1 + 0.08) / lk.bay, fwx) * fits;
            let c_sill = f_pulse(fy, (sill - 0.12) / F_STOREY, sill / F_STOREY, fwy) * tx;
            let c_shadow = f_pulse(fy, (sill - 0.3) / F_STOREY, (sill - 0.12) / F_STOREY, fwy) * tx;
            var sl = f_layer(lk.trim, 0.8);
            sl.tilt = vec2(0.0, 0.5 * f_fade(fw.y, 0.12));
            L = f_blend(L, sl, c_sill);
            L.occ *= 1.0 - 0.25 * c_shadow;
            if (lk.cornice) {
                let c_lin = f_pulse(fy, lk.head / F_STOREY, min(lk.head + 0.24, F_STOREY) / F_STOREY, fwy) * tx;
                L = f_blend(L, f_layer(lk.trim * 0.95, 0.85), c_lin);
            }
        }
    }

    // Patched panels: a share of cells re-rendered, re-clad or re-pointed, a different tone.
    let pc = floor(uv / vec2(2.3, 1.7));
    let pat = step(f_h(pc.x, pc.y, s + 0.9), 0.14 * wr) * f_pulse(uv.x / 2.3, 0.04, 0.96, fw.x / 2.3) * f_pulse(uv.y / 1.7, 0.05, 0.95, fw.y / 1.7) * f_fade(fm, 1.7);
    L.albedo *= 1.0 + (f_h(pc.y, pc.x, s + 1.3) - 0.45) * 0.24 * pat;
    // The grime, browner than grey.
    L.albedo *= mix(vec3(1.0), vec3(0.52, 0.5, 0.47), clamp(dirt * wr, 0.0, 0.85));
    // Rust bleeding from the laps and fixings of steel cladding (and a works' ironwork).
    if (lk.mat == M_METAL || lk.ground == G_WORKS) {
        // Runs down from each lap, in threads; a band where the water sits at the foot; the odd
        // blot where the paint's gone.
        let below = (1.0 - fract(uv.y / 2.4)) * 2.4;
        let runs = exp(-below / (0.35 + 0.8 * wr)) * smoothstep(0.45, 0.8, thread);
        let foot = (1.0 - smoothstep(0.0, 0.7, h - F_KERB)) * 0.7;
        let blot = smoothstep(0.72, 0.86, noise3(vec3(uv.x * 1.1, uv.y * 0.6, s + 7.0))) * f_fade(fm, 0.4);
        let rust = clamp(runs * 0.9 + foot + blot * 0.6, 0.0, 1.0) * wr * select(0.25, 0.7, lk.mat == M_METAL);
        L.albedo = mix(L.albedo, vec3(0.3, 0.12, 0.05) * (0.7 + 0.6 * thread), rust);
        L.rough = mix(L.rough, 0.95, rust);
        L.metal *= 1.0 - rust;
    }

    if (h < F_GROUND) {
        let gr = f_ground(i, fr, lk, s, L);
        L = gr.L;
        ocov = gr.cov;
        ofar = gr.far;
        op = gr.open;
        oshade = gr.shade;
    }
    if (ocov > 0.0) {
        var glass = ofar;
        if (op.on) {
            glass = f_blend(ofar, f_window(op.o, fr.d, op.w, fw), op.dwin);
        }
        glass.tilt += wobble;
        glass.albedo *= grime;
        glass.occ *= oshade;
        L = f_blend(L, glass, ocov);
    }

    if (h >= F_GROUND && lk.balcony > 0u && lk.win_w > 0.0) {
        // Balconies: the slab's edge, the balustrade (a panel, bars or glass), the shade of the one
        // overhead; plants on them in the Gardens.
        let yv = h - F_GROUND;
        let fy = yv / F_STOREY;
        let cu = uv.x / lk.bay;
        let wx0 = 0.5 * (lk.bay - lk.win_w);
        let fwy = fw.y / F_STOREY;
        let bx = select(f_pulse(cu, (wx0 - 0.35) / lk.bay, (wx0 + lk.win_w + 0.35) / lk.bay, fw.x / lk.bay), 1.0, lk.balcony == 2u);
        let c_slab = f_pulse(fy, 0.0, 0.2 / F_STOREY, fwy) * bx;
        let c_bal = f_pulse(fy, 0.2 / F_STOREY, 1.1 / F_STOREY, fwy) * bx;
        let c_shade = f_pulse(fy, (F_STOREY - 0.7) / F_STOREY, 1.0, fwy) * bx;
        let bt = f_h(s, 81.0, 1.0);
        var bal = f_layer(mix(lk.trim, lk.base, 0.5) * 1.08, 0.8);
        var bal_cov = c_bal;
        if (bt < 0.4) {
            let bars = f_pulse(uv.x / 0.12, 0.0, 0.2, fw.x / 0.12);
            let rail = f_pulse(fy, 1.02 / F_STOREY, 1.1 / F_STOREY, fwy) * bx;
            bal = f_layer(vec3(0.06, 0.065, 0.07), 0.5);
            bal.metal = 0.4;
            bal_cov = min(c_bal * bars + rail, 1.0);
        } else if (bt < 0.65) {
            bal = f_layer(vec3(0.3, 0.35, 0.37), 0.2);
            bal.refl = 0.4;
            bal_cov = c_bal * 0.6;
        }
        L = f_blend(L, bal, bal_cov);
        var slab = f_layer(lk.trim * 1.05, 0.85);
        slab.tilt = vec2(0.0, -0.4);
        L = f_blend(L, slab, c_slab);
        L.occ *= 1.0 - 0.45 * c_shade;
        L.albedo *= 1.0 - 0.3 * c_shade;
        if (i.strip == 2u || lk.mat == M_TIMBER) {
            let pn = noise3(vec3(uv * 2.2, s + 3.0));
            let c_pl = smoothstep(0.55, 0.65, pn) * f_pulse(fy, 0.7 / F_STOREY, 1.45 / F_STOREY, fwy) * bx * f_fade(fm, 0.5);
            L = f_blend(L, f_layer(mix(vec3(0.06, 0.14, 0.04), vec3(0.16, 0.26, 0.06), pn), 0.9), c_pl);
        }
    }

    if (!bulk && lk.win_w > 0.0) {
        if (lk.cornice) {
            // A string course over the ground floor, a cornice on dentils at the top, its shadow.
            var sc = f_layer(lk.trim, 0.8);
            sc.tilt = vec2(0.0, clamp((h - 5.12) / 0.18, -1.0, 1.0) * 1.2 * f_fade(fw.y, 0.35));
            L = f_blend(L, sc, f_band(h, 4.95, 5.3, fw.y));
            var co = f_layer(lk.trim * 1.05, 0.8);
            co.tilt = vec2(0.0, select(-2.0, 1.0, h > top - 0.45) * f_fade(fw.y, 0.4));
            L = f_blend(L, co, f_band(h, top - 0.85, top, fw.y));
            let dent = f_pulse(uv.x / 0.32, 0.0, 0.5, fw.x / 0.32) * f_band(h, top - 1.0, top - 0.85, fw.y);
            L.albedo *= 1.0 - 0.35 * dent;
            L.occ *= 1.0 - 0.35 * f_band(h, top - 1.4, top - 0.85, fw.y);
        } else {
            // A coping along the roof's edge.
            L = f_blend(L, f_layer(vec3(0.4, 0.41, 0.42), 0.5), f_band(h, top - 0.22, top, fw.y));
        }
    }
    return f_out(L, fr);
}

// ------------------------------------------------------------------------------- the colony's

// The colony's own halls (the Charter Board's, the Exchange, Hub Gate): white panels with fine
// joints, a stripe in the strip's line colour with a light strip in it, tall windows with rounded
// ends whose rims glow at night, a glazed ground floor.
fn f_hall(i: FacadeIn, fr: FFrame) -> FacadeOut {
    let s = i.seed;
    let uv = i.uv;
    let fw = fr.fw;
    let fm = max(fw.x, fw.y);
    let h = uv.y;
    let top = select(i.tall * F_MAX_HEIGHT, 1e6, i.tall <= 0.0);
    let line = f_line(i.strip);
    let glow_c = f_line_glow(i.strip);
    let night = i.lamps;
    let day = i.daylight * 900.0;
    let joint = max(f_pulse(uv.x / 2.4, 0.0, 0.0083, fw.x / 2.4), f_pulse(uv.y / 1.8, 0.0, 0.011, fw.y / 1.8));
    let tone = (f_h(floor(uv.x / 2.4), floor(uv.y / 1.8), s) - 0.5) * 0.04 * f_fade(fm, 1.8);
    let upper = f_band(h, 5.5, 1e5, fw.y);
    var L = f_layer(mix(vec3(0.6, 0.62, 0.65), vec3(0.74, 0.75, 0.76), upper) * (1.0 + tone) * (1.0 - 0.55 * joint), 0.32);
    // Dust at the foot even here.
    L.albedo *= 1.0 - 0.18 * (1.0 - smoothstep(0.2, 2.0, h));
    // What its windows need of a look.
    var look: FLook;
    look.reveal = 0.15;
    look.room = R_HALL;
    look.frame = vec3(0.72, 0.73, 0.75);
    look.trim = vec3(0.74, 0.75, 0.76);
    look.base = look.trim;
    look.cornice = false;
    // Its opening at this pixel (the ground floor's glass, or a tall window), drawn once.
    var op: FOpen;
    op.on = false;
    op.dwin = 0.0;
    var ocov = 0.0;
    var ofar = f_far_glass(vec3(0.0), day);
    var rim_glow = vec3(0.0);
    if (h < F_GROUND) {
        // Glass between white piers, the hall lit inside; a skirt in the line's colour.
        let sb = 6.0;
        let cu = uv.x / sb;
        let fu = fract(cu) * sb;
        ocov = f_band(h, F_KERB + 0.35, 4.6, fw.y) * f_pulse(cu, 0.45 / sb, 1.0 - 0.45 / sb, fw.x / sb);
        let light = f_lamp_colour(0.55) * 650.0 * (0.4 + 0.6 * night);
        ofar = f_far_glass(light, day);
        let dwin = 1.0 - smoothstep(0.08, 0.3, fm / 1.2);
        var w = f_win(look, vec2(sb - 0.9, 4.25 - F_KERB), vec2(-0.45 - sb, -0.35), vec2(2.0 * sb, 6.0), f_h(s, 3.0, 5.0));
        w.reveal = 0.08;
        w.depth = 14.0;
        w.panes = 3.0;
        w.blinds = 0.0;
        w.curtain = 0.0;
        w.light = light;
        w.day = day;
        w.tint = vec3(0.78, 0.84, 0.88);
        op.on = ocov > 0.002 && dwin > 0.002;
        op.o = vec3(fu - 0.45, h - F_KERB - 0.35, 0.0);
        op.w = w;
        op.dwin = dwin;
        L = f_blend(L, f_layer(line * 0.9, 0.4), f_band(h, F_KERB, F_KERB + 0.35, fw.y));
    } else {
        // Tall windows with rounded ends, in bays of 4 m, a storey of 7.2 m each, stopping under
        // the top stripe.
        let sb = 4.0;
        let st = 7.2;
        let yv = h - 5.5;
        let fl = floor(yv / st);
        let fv = yv - fl * st;
        let cu = uv.x / sb;
        let col = floor(cu);
        let fu = (cu - col) * sb;
        let wbot = 0.7;
        let wtop = min(st - 0.6, top - 2.4 - (5.5 + fl * st));
        if (yv > 0.0 && wtop > wbot + 1.0) {
            let hs = vec2(0.75, 0.5 * (wtop - wbot));
            let ctr = vec2(0.5 * sb, 0.5 * (wtop + wbot));
            let sdist = f_rrect(vec2(fu, fv), ctr, hs, 0.75);
            let mean = (4.0 * hs.x * hs.y - (4.0 - F_PI) * 0.5625) / (sb * st);
            ocov = mix(mean, clamp(0.5 - sdist / max(fm, 1e-4), 0.0, 1.0), f_fade(fm, 1.5));
            let lit = mix(step(f_h(col, fl, s), 0.7), 0.7, smoothstep(0.5, 1.5, fw.x / sb)) * (0.35 + 0.65 * night);
            let light = f_lamp_colour(0.5) * 600.0 * lit;
            ofar = f_far_glass(light, day);
            ofar.refl = 0.85;
            let dwin = 1.0 - smoothstep(0.1, 0.33, fw.x / 1.5);
            let ox = 0.5 * sb - 0.75;
            var w = f_win(look, vec2(1.5, wtop - wbot), vec2(-ox, -wbot), vec2(sb - ox, st - wbot), f_h(col, fl, s + 2.0));
            w.depth = 12.0;
            w.panes = 1.0;
            w.blinds = 0.0;
            w.curtain = 0.0;
            w.light = light;
            w.day = day;
            w.tint = vec3(0.7, 0.8, 0.86);
            op.on = ocov > 0.002 && dwin > 0.002;
            op.o = vec3(fu - ox, fv - wbot, 0.0);
            op.w = w;
            op.dwin = dwin;
            // The rounded rim, glowing at night in the line's colour.
            let rim = (1.0 - smoothstep(0.04, 0.04 + fm, abs(sdist + 0.04))) * f_fade(fm, 0.2);
            rim_glow = glow_c * rim * (500.0 * night + 20.0);
        }
    }
    if (ocov > 0.0) {
        var glass = ofar;
        if (op.on) {
            glass = f_blend(ofar, f_window(op.o, fr.d, op.w, fw), op.dwin);
        }
        L = f_blend(L, glass, ocov);
    }
    L.emissive += rim_glow;
    // The stripes in the line's colour, over the ground floor and under the top, a light strip in
    // each; a white coping.
    let c_st = max(f_band(h, 4.95, 5.5, fw.y), f_band(h, top - 1.9, top - 1.35, fw.y));
    L = f_blend(L, f_layer(line, 0.3), c_st);
    let c_light = max(f_band(h, 5.18, 5.27, fw.y), f_band(h, top - 1.67, top - 1.58, fw.y));
    L.emissive += glow_c * c_light * (1200.0 * night + 80.0);
    L = f_blend(L, f_layer(vec3(0.78, 0.79, 0.8), 0.3), f_band(h, top - 0.3, top, fw.y));
    return f_out(L, fr);
}

// Hub Gate's lift shaft: a white frame, clear glass, the dark shaft with its rail's light strip.
fn f_lift_glass(i: FacadeIn, fr: FFrame) -> FLayer {
    let fw = fr.fw;
    let uv = i.uv;
    let c_fr = max(f_pulse(uv.x / 2.0, 0.0, 0.05, fw.x / 2.0), f_pulse(uv.y / 3.0, 0.0, 0.04, fw.y / 3.0));
    var L = f_layer(vec3(0.02, 0.025, 0.03), 0.05);
    L.refl = 0.8;
    let d = fr.d;
    let x = uv.x + d.x * (7.0 / d.z);
    let lit = 1.0 - smoothstep(0.3, 0.3 + max(fw.x, 0.02) * 2.0, abs(x - 7.0));
    L.emissive = f_line_glow(i.strip) * lit * (300.0 + 1200.0 * i.lamps) * 0.8;
    L = f_blend(L, f_layer(vec3(0.72, 0.73, 0.75), 0.35), c_fr);
    return L;
}

// A tower's curtain wall: mullions every 1.5 m, a spandrel band at each floor, tinted reflective
// glass (blue, green, bronze, silver), the offices faint behind it and their ceilings' light grids
// at night, whole floors and corners of floors lit.
fn f_curtain(i: FacadeIn, fr: FFrame) -> FacadeOut {
    var L: FLayer;
    if (i.district == 0u) {
        L = f_lift_glass(i, fr);
    } else {
        L = f_tower_glass(i, fr);
    }
    return f_out(L, fr);
}

#endif

// A tower's glass tint and its mullions' colour: blue-green, blue, bronze, silver or green.
fn f_tower_tint(s: f32) -> mat2x3<f32> {
    let tk = f_h(s, 51.0, 2.0);
    var tint = vec3(0.035, 0.065, 0.085);
    var mull = vec3(0.22, 0.24, 0.27);
    if (tk < 0.25) {
        tint = vec3(0.03, 0.05, 0.1);
    } else if (tk < 0.45) {
        tint = vec3(0.075, 0.055, 0.035);
        mull = vec3(0.2, 0.15, 0.1);
    } else if (tk < 0.65) {
        tint = vec3(0.065, 0.07, 0.075);
        mull = vec3(0.45, 0.46, 0.48);
    } else if (tk < 0.8) {
        tint = vec3(0.04, 0.07, 0.05);
    }
    return mat2x3<f32>(tint, mull);
}

#ifndef FACADE_LOW
fn f_tower_glass(i: FacadeIn, fr: FFrame) -> FLayer {
    let s = i.seed;
    let uv = i.uv;
    let fw = fr.fw;
    let h = uv.y;
    let night = i.lamps;
    let day = i.daylight * 900.0;
    let tm = f_tower_tint(s);
    let tint = tm[0];
    var mull = tm[1];
    let trans = mix(vec3(1.0), normalize(tint) * 1.73, 0.35) * 0.65;
    // Three kinds of tower, so the skyline reads: all glass; banded, light spandrel panels at every
    // floor; and finned, a light fin down every mullion.
    let style = f_h(s, 58.0, 2.0);
    let pale = mix(vec3(0.42, 0.43, 0.44), vec3(0.6, 0.58, 0.54), f_h(s, 59.0, 2.0));
    var span_c = mix(tint * 2.2, mull * 0.5, 0.3);
    var span_refl = 0.6;
    var span_rough = 0.12;
    var fin = 0.07;
    if (style >= 0.4 && style < 0.75) {
        span_c = pale;
        span_refl = 0.15;
        span_rough = 0.55;
    } else if (style >= 0.75) {
        fin = 0.32;
        mull = pale;
    }
    let mw = 1.5;
    let cu = uv.x / mw;
    let col = floor(cu);
    let fu = (cu - col) * mw;
    let yv = h - F_GROUND;
    let fy = yv / F_STOREY;
    let fl = floor(fy);
    let fv = yv - fl * F_STOREY;
    let fwy = fw.y / F_STOREY;
    let vis_lo = 0.85;
    let vis_hi = 3.05;
    let c_vis = f_pulse(fy, vis_lo / F_STOREY, vis_hi / F_STOREY, fwy);
    let c_mull = f_pulse(cu, 0.0, fin / mw, fw.x / mw);
    let c_tr = max(f_pulse(fy, (vis_lo - 0.03) / F_STOREY, (vis_lo + 0.03) / F_STOREY, fwy), f_pulse(fy, (vis_hi - 0.03) / F_STOREY, (vis_hi + 0.03) / F_STOREY, fwy));
    // The spandrel: the slab's edge behind fritted glass.
    var L = f_layer(span_c, span_rough);
    L.refl = span_refl;
    // Offices lit tonight, a zone of a floor at a time (6 m of it); more in some corners of the
    // tower than others. Each zone its own until finer than a pixel, then its corner's share, then
    // the tower's.
    let q = (0.15 + 0.3 * f_h(s, 53.0, 1.0)) * night;
    let q_c = clamp(q * (0.2 + 1.6 * f_h(floor(fl / 3.0), floor(col / 8.0), s + 0.4)), 0.0, 1.0);
    let lit_z = step(f_h(fl, floor(col / 4.0), s + 0.2), q_c);
    let a_z = smoothstep(0.5, 1.5, max(fw.x / (4.0 * mw), fwy));
    let a_c = smoothstep(0.6, 1.8, max(fw.x / (8.0 * mw), fwy / 3.0));
    let lit = mix(mix(lit_z, q_c, a_z), q, a_c);
    let lamp = f_lamp_colour(0.45 + 0.55 * f_h(s, 57.0, 1.0)) * F_ROOM_NITS;
    var glass = f_layer(tint * 0.5, 0.04);
    glass.refl = 0.95;
    glass.emissive = (lamp * lit * 0.4 * mix(1.0, F_FAR_GLOW, a_z) + vec3(day * 0.18)) * trans;
    let dwin = 1.0 - smoothstep(0.1, 0.33, max(fw.x / mw, fw.y / (vis_hi - vis_lo)));
    if (c_vis > 0.002 && dwin > 0.002) {
        // The office behind: open plan, a ceiling of light panels, desks along the glass.
        let rx = col - 3.0 * floor(col / 3.0);
        var w: FWin;
        w.size = vec2(mw, vis_hi - vis_lo);
        w.reveal = 0.0;
        w.lo = vec2(-rx * mw, 0.3 - vis_lo);
        w.hi = vec2((3.0 - rx) * mw, vis_hi - vis_lo);
        w.depth = 10.0;
        w.room = R_OFFICE;
        w.rs = f_h(floor(col / 3.0), fl, s);
        w.panes = 1.0;
        w.transom = 0.0;
        w.blinds = 0.0;
        w.curtain = 0.0;
        w.light = lamp * lit;
        w.day = day;
        w.tint = trans;
        w.glass = tint * 0.5;
        w.refl = 0.95;
        w.fwid = 0.0;
        var dg = f_window(vec3(fu, fv - vis_lo, 0.0), fr.d, w, fw);
        // Panes aren't flat: a quilted reflection, each a little off.
        dg.tilt = (vec2(f_h(col, fl, s + 3.0), f_h(fl, col, s + 5.0)) - 0.5) * 0.025;
        glass = f_blend(glass, dg, dwin);
    }
    L = f_blend(L, glass, c_vis);
    var ml = f_layer(mull, 0.35);
    ml.metal = 0.8;
    ml.refl = 0.2;
    L = f_blend(L, ml, max(c_mull, c_tr));
    return L;
}

// ------------------------------------------------------------------------------------- roofs

// A roof: gravel, felt or membrane, pavers, a works' metal, the colony's panels, a garden; plant
// units, solar panels, skylights and planters on it; red lights on the tall ones. An L3 block's
// bulk is split into lots of its own.
fn f_roof(i: FacadeIn) -> FacadeOut {
    var fr: FFrame;
    fr.n = i.n;
    // Level faces: s grows across the strip, x along the axis.
    let across = cross(i.n, vec3(1.0, 0.0, 0.0));
    fr.t = across / max(length(across), 1e-6);
    fr.b = vec3(1.0, 0.0, 0.0);
    fr.d = vec3(0.0, 0.0, 1.0);
    fr.fw = abs(i.duv_dx) + abs(i.duv_dy);
    let fw = fr.fw;
    let fm = max(fw.x, fw.y);
    // The strip's (s, x), x taken within its block so it keeps its precision 16 km out.
    let q = vec2(i.uv.x, i.uv.y - floor(i.uv.y / F_BLOCK) * F_BLOCK);
    let d = i.district;
    var s = i.seed;
    var gap = 0.0;
    let bulk = i.tall <= 0.0 && (i.kind == 1u || i.kind == 7u);
    if (bulk) {
        let lq = q / 42.0;
        s = floor(f_h(floor(lq.x), floor(lq.y), i.seed) * 255.0);
        gap = max(f_pulse(lq.x, 0.0, 0.05, fw.x / 42.0), f_pulse(lq.y, 0.0, 0.05, fw.y / 42.0));
    }
    let h0 = f_h(s, 61.0, 2.0);
    let h1 = f_h(s, 62.0, 2.0);
    // 0 gravel, 1 felt, 2 pale membrane, 3 pavers, 4 metal, 5 the colony's panels, 6 a garden.
    var rt = 0u;
    if (i.kind == 6u || d == 0u) {
        rt = 5u;
    } else if (d == D_WORKS || d == D_PORT) {
        rt = select(1u, 4u, h0 < 0.6);
    } else if (d == D_BUSINESS || d == D_CIVIC) {
        rt = select(select(select(5u, 0u, h0 < 0.85), 1u, h0 < 0.55), 2u, h0 < 0.3);
    } else if (d == D_OLDTOWN) {
        rt = select(1u, 0u, h0 < 0.3);
    } else if (i.strip == 2u && (d == D_RESIDENTIAL || d == D_PARK || d == D_UNIVERSITY || d == D_MIDTOWN)) {
        rt = select(select(0u, 3u, h0 < 0.75), 6u, h0 < 0.45);
    } else {
        rt = select(select(1u, 3u, h0 > 0.85), 0u, h0 < 0.5);
    }
    var L = f_layer(vec3(0.3), 0.92);
    if (rt == 0u) {
        let grain = mix(0.5, noise3(vec3(q * 6.0, s)), f_fade(fm, 0.17));
        let patchy = noise3(vec3(q * 0.35, s + 2.0));
        L.albedo = mix(vec3(0.36, 0.33, 0.28), vec3(0.27, 0.28, 0.3), h1) * (0.78 + 0.3 * grain + 0.2 * patchy);
    } else if (rt == 1u) {
        // Felt in strips, patched, pale rings where the rain ponds.
        let seam = f_pulse(q.x, 0.0, 0.06, fw.x);
        let pn = floor(q / vec2(3.0, 2.0));
        let pt = step(f_h(pn.x, pn.y, s), 0.12) * f_fade(fm, 2.0);
        L.albedo = vec3(0.085, 0.085, 0.09) * (1.0 + 0.5 * seam) * (1.0 + 0.6 * pt * (f_h(pn.y, pn.x, s) - 0.3));
        L.albedo += vec3(0.025, 0.024, 0.022) * smoothstep(0.68, 0.78, noise3(vec3(q * 0.4, s + 7.0)));
        L.rough = 0.75;
    } else if (rt == 2u) {
        // A pale membrane, its seams, dirt washed towards the drains.
        let seam = f_pulse(q.y / 1.8, 0.0, 0.03, fw.y / 1.8);
        L.albedo = vec3(0.52, 0.53, 0.52) * (1.0 - 0.18 * seam) * (0.75 + 0.3 * noise3(vec3(q * 0.18, s + 3.0)));
        L.rough = 0.6;
    } else if (rt == 3u) {
        // Pavers on a terrace, terracotta or concrete.
        let pv = q / 0.6;
        let joint = max(f_pulse(pv.x, 0.0, 0.05, fw.x / 0.6), f_pulse(pv.y, 0.0, 0.05, fw.y / 0.6));
        let tone = (f_h(floor(pv.x), floor(pv.y), s) - 0.5) * 0.2 * f_fade(fm, 0.6);
        L.albedo = select(vec3(0.44, 0.43, 0.4), vec3(0.45, 0.24, 0.14), h1 < 0.5) * (1.0 + tone) * (1.0 - 0.4 * joint);
        L.rough = 0.85;
    } else if (rt == 4u) {
        // A works' metal roof: ribs down its fall, rust.
        let along_x = h1 < 0.5;
        let a = select(q.x, q.y, along_x);
        let ph = a / 0.3 * F_TAU;
        let det = f_fade_wave(select(fw.x, fw.y, along_x), 0.3);
        let rib = 0.5 * sin(ph) * det;
        L.albedo = mix(f_cladding(f_h(s, 63.0, 2.0)), vec3(0.4, 0.41, 0.42), 0.45) * (1.0 - 0.1 * cos(ph) * det);
        L.tilt = select(vec2(rib, 0.0), vec2(0.0, rib), along_x);
        L.metal = 0.2;
        L.rough = 0.5;
        let runs = noise3(vec3(select(q.x * 3.0, q.x * 0.3, along_x), select(q.y * 0.3, q.y * 3.0, along_x), s + 9.0));
        let rust = smoothstep(0.62, 0.85, runs) * smoothstep(0.4, 0.7, noise3(vec3(q * 0.15, s + 4.0))) * 0.55 * f_fade(fm, 0.3) + 0.08;
        L.albedo = mix(L.albedo, vec3(0.3, 0.12, 0.05), rust);
        L.metal *= 1.0 - rust;
    } else if (rt == 5u) {
        // The colony's own: pale panels, skylight strips that glow at night.
        let pj = max(f_pulse(q.x / 3.0, 0.0, 0.01, fw.x / 3.0), f_pulse(q.y / 3.0, 0.0, 0.01, fw.y / 3.0));
        L.albedo = vec3(0.6, 0.62, 0.64) * (1.0 - 0.4 * pj);
        L.rough = 0.4;
        var sl = f_layer(vec3(0.03, 0.04, 0.05), 0.05);
        sl.refl = 0.8;
        sl.emissive = (f_lamp_colour(0.6) * 220.0 + f_line_glow(i.strip) * 80.0) * i.lamps * step(f_h(s, 65.0, 2.0), 0.6);
        L = f_blend(L, sl, f_pulse(q.y / 24.0, 0.47, 0.53, fw.y / 24.0));
    } else {
        // A garden: beds of green and bare soil between paths.
        let bq = q / 4.0;
        let path = max(f_pulse(bq.x, 0.0, 0.22, fw.x / 4.0), f_pulse(bq.y, 0.0, 0.22, fw.y / 4.0));
        let leaf = mix(0.5, noise3(vec3(q * 1.3, s)), f_fade(fm, 0.4));
        let bed = mix(0.6, f_h(floor(bq.x), floor(bq.y), s + 1.0), f_fade(fm, 2.0));
        let green = mix(vec3(0.06, 0.13, 0.04), vec3(0.17, 0.26, 0.07), leaf) * (0.8 + 0.4 * bed);
        let planted = mix(vec3(0.2, 0.14, 0.09), green, smoothstep(0.1, 0.2, bed));
        L.albedo = mix(planted, vec3(0.45, 0.25, 0.15), path);
        L.rough = 0.95;
    }
    // Stains a few metres across.
    L.albedo *= 0.85 + 0.3 * noise3(vec3(q * 0.07, s + 11.0));

    if (rt <= 3u && !bulk && i.tall > 0.0) {
        // What stands on a roof, cell by cell (6 m): plant units, solar panels tipped to the light,
        // skylights, planters. From afar they fade into what they add up to.
        let cq = q / 6.0;
        let cell = floor(cq);
        let fq = (cq - cell) * 6.0;
        let ch = f_h(cell.x, cell.y, s + 0.5);
        let ch2 = f_h(cell.y, cell.x, s + 1.5);
        let det = f_fade(fm, 0.9);
        let p_ac = select(0.12, 0.3, d == D_BUSINESS || d == D_MIDTOWN || d == D_WORKS || d == D_CIVIC);
        let p_solar = select(0.06, 0.35, i.strip == 2u || d == D_WORKS || d == D_RESIDENTIAL) * step(f_h(s, 64.0, 2.0), 0.5);
        let p_sky = 0.08;
        let p_box = select(0.0, 0.25, i.strip == 2u || d == D_RESIDENTIAL);
        let fpx = max(fm, 1e-3);
        var feat = L;
        var fc = 0.0;
        if (ch < p_ac) {
            let c0 = vec2(1.2 + 3.0 * ch2, 1.0 + 3.0 * f_h(ch2, ch, 3.0)) + vec2(0.9, 0.6);
            let r = abs(fq - c0) - vec2(0.9, 0.6);
            fc = clamp(0.5 - max(r.x, r.y) / fpx, 0.0, 1.0);
            let fan = length(vec2(abs(fq.x - c0.x) - 0.45, fq.y - c0.y));
            feat = f_layer(vec3(0.55, 0.56, 0.55) * (1.0 - 0.75 * (1.0 - smoothstep(0.28, 0.32 + fm, fan))), 0.5);
            feat.metal = 0.3;
        } else if (ch < p_ac + p_solar) {
            let pc = f_pulse(fq.x / 1.1, 0.06, 0.94, fw.x / 1.1) * f_pulse(fq.y / 2.0, 0.1, 0.9, fw.y / 2.0);
            fc = pc * f_band(fq.x, 0.4, 5.6, fw.x) * f_band(fq.y, 0.4, 5.6, fw.y);
            feat = f_layer(vec3(0.025, 0.035, 0.07), 0.15);
            feat.refl = 0.5;
            feat.tilt = vec2(0.0, 0.35);
            feat.albedo += vec3(0.04) * max(f_pulse(fq.x / 0.18, 0.0, 0.1, fw.x / 0.18), f_pulse(fq.y / 0.18, 0.0, 0.1, fw.y / 0.18));
        } else if (ch < p_ac + p_solar + p_sky) {
            let r = abs(fq - vec2(3.0)) - vec2(0.7);
            fc = clamp(0.5 - max(r.x, r.y) / fpx, 0.0, 1.0);
            feat = f_layer(vec3(0.03, 0.035, 0.04), 0.06);
            feat.refl = 0.7;
            feat.emissive = f_lamp_colour(ch2) * 380.0 * i.lamps * step(ch2, 0.6);
        } else if (ch < p_ac + p_solar + p_sky + p_box) {
            let r = abs(fq - vec2(3.0)) - vec2(2.2, 0.45);
            fc = clamp(0.5 - max(r.x, r.y) / fpx, 0.0, 1.0);
            let leaf = noise3(vec3(fq * 3.0, s));
            feat = f_layer(mix(vec3(0.05, 0.12, 0.03), vec3(0.15, 0.24, 0.06), leaf), 0.95);
        }
        L = f_blend(L, feat, fc * det);
        L.albedo *= 1.0 - 0.06 * (p_ac + p_solar) * (1.0 - det);
    }
    if (i.tall > 0.3) {
        // Red obstruction lights on the tall roofs (suits fly over the city), flashing together.
        let oc = floor(q / 16.0);
        let oh = f_h(oc.x, oc.y, s + 2.5);
        if (oh < 0.3) {
            let c0 = (oc + vec2(0.3) + 0.4 * vec2(oh * 3.0, f_h(oc.y, oc.x, s))) * 16.0;
            let flash = step(fract(i.seconds * 0.75), 0.4);
            L.emissive += vec3(1.0, 0.06, 0.03) * f_lamp_dot(q - c0, 0.3, fw) * 12000.0 * flash * max(i.lamps, 0.15);
        }
    }
    // The gaps between a bulk's lots: lanes and yards in shadow.
    L.albedo = mix(L.albedo, vec3(0.08, 0.08, 0.085), gap * 0.8);
    return f_out(L, fr);
}

// ------------------------------------------------------------------------------ steel, railings

// The site's steel (primer, galvanised, the cranes' yellow, rusting) and the colony's own (white:
// the stations' canopies, Hub Gate's lift).
fn f_steel(i: FacadeIn, fr: FFrame) -> FacadeOut {
    let s = i.seed;
    let level = abs(dot(i.n, fr.b)) > 0.7;
    var uv = i.uv;
    if (level) {
        uv = vec2(uv.x, uv.y - floor(uv.y / F_BLOCK) * F_BLOCK);
    }
    var L = f_layer(vec3(0.68, 0.7, 0.72), 0.38);
    if (i.kind == 5u) {
        let k = f_h(s, 71.0, 1.0);
        L = f_layer(select(vec3(0.34, 0.1, 0.05), vec3(0.6, 0.42, 0.05), k < 0.3), 0.6);
        if (k > 0.8) {
            L = f_layer(vec3(0.5, 0.51, 0.52), 0.45);
            L.metal = 0.8;
        }
        let rust = smoothstep(0.6, 0.8, noise3(vec3(uv.x * 0.8, uv.y * 0.4, s)));
        L.albedo = mix(L.albedo, vec3(0.28, 0.11, 0.05), rust * 0.7);
        L.metal *= 1.0 - rust;
        L.rough = mix(L.rough, 0.9, rust);
    } else {
        L.albedo *= 0.92 + 0.08 * noise3(vec3(uv * 0.5, s));
    }
    return f_out(L, fr);
}

// A quay's railing: posts every 1.6 m and four rails, painted dark green, rust at the feet. Its
// gaps are cut out (`cut`): exactly up close, and further off, where rails and posts are finer
// than a pixel, as a dither of what they cover, so it stays a see-through railing all the way
// down the quay instead of turning into a solid slab 1.1 m high.
fn f_railing(i: FacadeIn, fr: FFrame) -> FacadeOut {
    let fw = fr.fw;
    var L = f_layer(vec3(0.05, 0.09, 0.07), 0.45);
    L.metal = 0.3;
    let side = abs(dot(i.n, fr.b)) < 0.5 && abs(i.n.x) < 0.5;
    var cut = 0.0;
    if (side) {
        let hr = i.uv.y - F_KERB;
        let rails = max(max(f_band(hr, 0.0, 0.07, fw.y), f_band(hr, 0.36, 0.42, fw.y)), max(f_band(hr, 0.71, 0.77, fw.y), f_band(hr, 1.02, 1.1, fw.y)));
        let posts = f_pulse(i.uv.x / 1.6, 0.0, 0.07 / 1.6, fw.x / 1.6);
        let solid = clamp(rails + posts - rails * posts, 0.0, 1.0);
        // Interleaved gradient noise on the pixel grid: a fine, even screen door (the window
        // coordinates stay in the thousands, so the products keep their precision).
        let dither = fract(52.9829189 * fract(dot(i.frag, vec2(0.06711056, 0.00583715))));
        cut = select(0.0, 1.0, solid <= dither * 0.98 + 0.01);
        let rust = smoothstep(0.62, 0.8, noise3(vec3(i.uv.x * 3.0, hr * 6.0, i.seed))) * (1.0 - smoothstep(0.0, 0.5, hr));
        L.albedo = mix(L.albedo, vec3(0.28, 0.11, 0.05), rust);
    }
    var o = f_out(L, fr);
    o.cut = cut;
    return o;
}

#endif

// ---------------------------------------------------------------------------------- the Low tier

#ifdef FACADE_LOW
// The cheap facade, for the Low tier (`FACADE_LOW`; software rasterisers get Low, CI's
// SwiftShader among them): the same looks, windows and nights, averaged, with no rooms, lettering,
// relief or detailed wear. On SwiftShader a city frame with the full facade takes about ten times
// as long as with today's walls; with this one, about twice.
fn facade_low(i: FacadeIn) -> FacadeOut {
    var fr: FFrame;
    fr.n = i.n;
    fr.t = vec3(1.0, 0.0, 0.0);
    fr.b = vec3(0.0, 1.0, 0.0);
    fr.d = vec3(0.0, 0.0, 1.0);
    fr.fw = abs(i.duv_dx) + abs(i.duv_dy);
    let fw = fr.fw;
    let uv = i.uv;
    let h = uv.y;
    let s = i.seed;
    let night = i.lamps;
    let day = i.daylight * 900.0;
    let top = select(i.tall * F_MAX_HEIGHT, 1e6, i.tall <= 0.0);
    var L = f_layer(vec3(0.5), 0.85);
    var cut = 0.0;
    if (i.surface == 2u) {
        // A roof: its kind's average tone, and stains.
        let k = f_h(s, 61.0, 2.0);
        var c = mix(vec3(0.1, 0.1, 0.105), vec3(0.5, 0.5, 0.49), k);
        if (i.district == D_WORKS || i.district == D_PORT) {
            c = mix(f_cladding(f_h(s, 63.0, 2.0)), vec3(0.4, 0.41, 0.42), 0.45);
        } else if (i.strip == 2u && k < 0.45) {
            c = vec3(0.11, 0.19, 0.07);
        } else if (i.kind == 6u || i.district == 0u) {
            c = vec3(0.6, 0.62, 0.64);
        }
        let q = vec2(uv.x, uv.y - floor(uv.y / F_BLOCK) * F_BLOCK);
        L = f_layer(c * (0.85 + 0.3 * noise3(vec3(q * 0.07, s + 11.0))), 0.9);
    } else if (i.surface == 9u || i.surface == 10u) {
        L = f_layer(select(vec3(0.68, 0.7, 0.72), vec3(0.34, 0.1, 0.05), i.kind == 5u), 0.5);
        if (i.surface == 10u) {
            // A railing: its rails and posts, the gaps cut out as the full facade cuts them.
            L.albedo = vec3(0.05, 0.09, 0.07);
            if (abs(dot(i.n, normalize(-vec3(0.0, i.p.y, i.p.z)))) < 0.5 && abs(i.n.x) < 0.5) {
                let hr = h - F_KERB;
                let rails = max(max(f_band(hr, 0.0, 0.07, fw.y), f_band(hr, 0.36, 0.42, fw.y)), max(f_band(hr, 0.71, 0.77, fw.y), f_band(hr, 1.02, 1.1, fw.y)));
                let posts = f_pulse(uv.x / 1.6, 0.0, 0.07 / 1.6, fw.x / 1.6);
                let solid = clamp(rails + posts - rails * posts, 0.0, 1.0);
                let dither = fract(52.9829189 * fract(dot(i.frag, vec2(0.06711056, 0.00583715))));
                cut = select(0.0, 1.0, solid <= dither * 0.98 + 0.01);
            }
        }
    } else if (i.surface == 11u) {
        // A hall: white panels, its windows, the line's stripes and their light strips.
        let line = f_line(i.strip);
        L = f_layer(mix(vec3(0.6, 0.62, 0.65), vec3(0.74, 0.75, 0.76), f_band(h, 5.5, 1e5, fw.y)), 0.32);
        let yv = h - 5.5;
        let up = f_pulse(uv.x / 4.0, 0.31, 0.69, fw.x / 4.0) * f_pulse(yv / 7.2, 0.1, 0.92, fw.y / 7.2) * f_band(h, 5.5, top - 2.4, fw.y);
        let foot = f_band(h, F_KERB + 0.35, 4.6, fw.y) * f_pulse(uv.x / 6.0, 0.075, 0.925, fw.x / 6.0);
        L = f_blend(L, f_far_glass(f_lamp_colour(0.5) * 600.0 * (0.35 + 0.65 * night), day), max(up, foot));
        L = f_blend(L, f_layer(line, 0.3), max(f_band(h, 4.95, 5.5, fw.y), f_band(h, top - 1.9, top - 1.35, fw.y)));
        L.emissive += f_line_glow(i.strip) * max(f_band(h, 5.18, 5.27, fw.y), f_band(h, top - 1.67, top - 1.58, fw.y)) * (1200.0 * night + 80.0);
    } else if (i.surface == 12u && f_has_towers(i.district, i.kind)) {
        // A tower's curtain wall: spandrels, mullions, its offices' light.
        let tm = f_tower_tint(s);
        let fy = (h - F_GROUND) / F_STOREY;
        let cu = uv.x / 1.5;
        let vis = f_pulse(fy, 0.236, 0.847, fw.y / F_STOREY);
        let q = (0.15 + 0.3 * f_h(s, 53.0, 1.0)) * night;
        let q_c = clamp(q * (0.2 + 1.6 * f_h(floor(floor(fy) / 3.0), floor(floor(cu) / 8.0), s + 0.4)), 0.0, 1.0);
        let lit = mix(q_c, q, smoothstep(0.6, 1.8, max(fw.x / 12.0, fw.y / 10.8)));
        L = f_layer(select(mix(tm[0] * 2.2, tm[1] * 0.5, 0.3), vec3(0.5, 0.5, 0.49), f_h(s, 58.0, 2.0) >= 0.4 && f_h(s, 58.0, 2.0) < 0.75), 0.3);
        L.refl = 0.5;
        var g = f_layer(tm[0] * 0.5, 0.04);
        g.refl = 0.95;
        g.emissive = (f_lamp_colour(0.45 + 0.55 * f_h(s, 57.0, 1.0)) * F_ROOM_NITS * lit * 0.4 * mix(1.0, F_FAR_GLOW, smoothstep(0.5, 1.5, max(fw.x / 6.0, fw.y / F_STOREY))) + vec3(day * 0.18)) * 0.6;
        L = f_blend(L, g, vis);
        L = f_blend(L, f_layer(tm[1], 0.35), f_pulse(cu, 0.0, 0.05, fw.x / 1.5));
    } else {
        // A wall: its material's colour, grime at the foot and under the roof, its windows lit as
        // the full facade lights them, the ground floor's glazing and fascias.
        let bulk = i.tall <= 0.0;
        var ws = s;
        if (bulk) {
            ws = floor(f_h(s, floor(uv.x / 32.0), 5.0) * 255.0);
        }
        let lk = f_look(i.district, i.strip, i.kind, ws);
        var c = lk.base * select(1.0, 0.88, lk.mat == M_BRICK || lk.mat == M_TIMBER);
        c *= 1.0 + 0.16 * (noise3(vec3(uv.x * 0.31, uv.y * 0.19, ws * 0.37)) - 0.5);
        let dirt = (1.0 - smoothstep(0.2, 3.0, h)) * 0.4 + exp(-max(top - h, 0.0) / 4.0) * 0.3;
        c *= mix(vec3(1.0), vec3(0.52, 0.5, 0.47), clamp(dirt * lk.wear, 0.0, 0.85));
        L = f_layer(c, lk.rough);
        if (h >= F_GROUND && lk.win_w > 0.0) {
            let fy = (h - F_GROUND) / F_STOREY;
            let fl = floor(fy);
            let cu = uv.x / lk.bay;
            let col = floor(cu);
            let wx0 = 0.5 * (lk.bay - lk.win_w);
            let sill = select(lk.sill, 0.1, lk.balcony > 0u);
            let fwx = fw.x / lk.bay;
            let fwy = fw.y / F_STOREY;
            let fits = select(0.0, 1.0, F_GROUND + fl * F_STOREY + lk.head <= top - 0.1);
            let cov = f_pulse(cu, wx0 / lk.bay, (wx0 + lk.win_w) / lk.bay, fwx) * f_pulse(fy, sill / F_STOREY, lk.head / F_STOREY, fwy) * fits;
            let q = f_lit_share(lk.lit, ws, night);
            let q_c = clamp(q * (0.25 + 1.5 * f_h(floor(fl / 3.0), floor(col / 4.0), ws + 0.3)), 0.0, 1.0);
            let lit_w = step(f_h(col, fl, ws + 0.37), q_c);
            let lit = mix(mix(lit_w, q_c, smoothstep(0.5, 1.5, max(fwx, fwy))), q, smoothstep(0.6, 1.8, max(fwx * 0.25, fwy * 0.333)));
            let lamp = f_lamp_colour(clamp(lk.warmth + (f_h(col, fl, ws + 0.53) - 0.5) * 0.9, 0.0, 1.0)) * F_ROOM_NITS;
            L = f_blend(L, f_far_glass(lamp * lit * mix(1.0, F_FAR_GLOW, smoothstep(0.5, 1.5, max(fwx, fwy))), day), cov);
            if (lk.balcony > 0u) {
                L = f_blend(L, f_layer(lk.trim, 0.85), f_pulse(fy, 0.0, 0.06, fwy));
            }
        } else if (h < F_GROUND && lk.ground != G_BLANK) {
            let cu = uv.x / 5.5;
            let bay = floor(cu);
            let inner = f_pulse(cu, 0.05, 0.95, fw.x / 5.5);
            let lit = select(0.0, 1.0, f_h(bay, ws, 29.0) < mix(1.0, lk.shop_lit, night));
            var glaze = f_far_glass(f_lamp_colour(0.5) * 650.0 * lit, day);
            if (lk.ground == G_WORKS) {
                glaze = f_layer(f_cladding(f_h(ws, 91.0, 1.0)) * 0.8, 0.5);
            }
            L = f_blend(L, glaze, f_band(h, 0.55, 3.0, fw.y) * inner);
            if (lk.ground == G_SHOPS) {
                var fa = f_layer(f_sign_colour(f_h(bay, ws, 43.0)), 0.5);
                fa.emissive = f_neon(f_h(bay, ws, 46.0)) * 120.0 * night * step(f_h(bay, ws, 45.0), 0.55);
                L = f_blend(L, fa, f_band(h, 3.3, 4.25, fw.y) * inner);
            }
        }
    }
    var o = f_out(L, fr);
    o.cut = cut;
    return o;
}

#endif

// ------------------------------------------------------------------------------------ the entry

// Whether a district builds towers (so its glass is a curtain wall; elsewhere an L3 chunk's
// standing-out building is drawn as its walls).
fn f_has_towers(d: u32, kind: u32) -> bool {
    return kind == 7u || d == 0u || d == D_BUSINESS || d == D_CIVIC || d == D_MIDTOWN;
}

// A building's surface: walls (1), roofs (2), steel (9), railings (10), halls (11), glass (12).
fn facade(fi: FacadeIn) -> FacadeOut {
    // The seed is a byte, but it arrives interpolated, a hair off from pixel to pixel; hashed as
    // is, that hair turns into speckle (a room's depth, whether it's lit). Back to the byte. Not
    // by rounding: Hub Gate's own pieces carry seeds of 0.3, 0.5 and 0.7 (76.5, 127.5, 178.5 in
    // 255ths), right on rounding's edge, where one ulp of interpolation flips the seed from pixel
    // to pixel. A quarter up keeps both the bytes and those halves steady.
    var i = fi;
    i.seed = floor(fi.seed + 0.25);
#ifdef FACADE_LOW
    return facade_low(i);
#else
    // One chain of branches, no early returns: a software rasteriser (SwiftShader, in CI) runs
    // whatever follows a `return` for every pixel, masked, but skips a branch nobody takes.
    var o: FacadeOut;
    if (i.surface == 2u) {
        o = f_roof(i);
    } else {
        let fr = f_frame(i);
        if (i.surface == 9u) {
            o = f_steel(i, fr);
        } else if (i.surface == 10u) {
            o = f_railing(i, fr);
        } else if (i.surface == 11u) {
            o = f_hall(i, fr);
        } else if (i.surface == 12u && f_has_towers(i.district, i.kind)) {
            o = f_curtain(i, fr);
        } else {
            o = f_wall(i, fr);
        }
    }
    return o;
#endif
}
