// The colony's clean language indoors (`StationMaterial`): the Ring Deck's walls, floors, ceilings,
// pillars, doors and markings (`bc_client_core::ring_deck`), painted from what each face carries
// in its vertex colour: r its surface, g its paint (an entry of the palette), b what's painted on
// it (the band and its arrows, chevrons, the suits' lanes), a a seed. UVs are metres across the
// face, the deck's own coordinates where the face is square to them, so panels line up from one
// block to the next. The numbers are `ring_deck`'s (its tests read them here).
//
// White panels with fine joints, a vent or an access hatch here and there; the lines' colours for
// wayfinding; safety yellow and black only where there's a hazard; light in strips.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct Station {
    // rgb: the palette, linear (`ring_deck::PAINT`).
    paint: array<vec4<f32>, 8>,
    // x: seconds.
    time: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> station: Station;

// Surfaces (`ring_deck::Surface`).
const WALL: u32 = 1u;
const FLOOR: u32 = 2u;
const CEILING: u32 = 3u;
const PILLAR: u32 = 4u;
const TRIM: u32 = 5u;
const GLOW: u32 = 6u;
const DOOR: u32 = 7u;
const WINDOW: u32 = 8u;
const HAZARD: u32 = 9u;

// What's painted on a face (its flags).
const F_BAND: u32 = 1u;
const F_ARROWS: u32 = 2u;
const F_BACK: u32 = 4u;
const F_CHEVRONS: u32 = 8u;
const F_LANES: u32 = 16u;

// The palette's safety colours.
const YELLOW: u32 = 4u;
const DARK: u32 = 5u;

const PANEL_W: f32 = 3.0;
const PANEL_H: f32 = 2.0;
const PLATE: f32 = 4.0;
const KICK: f32 = 1.0;
const BAND_Y0: f32 = 27.0;
const BAND_Y1: f32 = 30.0;
const ARROW_EVERY: f32 = 10.0;
const LANE_X0: f32 = -4.0;
const LANE_X1: f32 = 6.0;
const LANE_HALF: f32 = 4.5;
const CHEVRON_EVERY: f32 = 18.0;
const STRIP_EVERY: f32 = 12.0;
const HAZARD_PERIOD: f32 = 1.2;
const COLLAR_Y0: f32 = 6.0;
const COLLAR_Y1: f32 = 7.5;

// A panel's joint, m.
const JOINT: f32 = 0.03;
// The light strips' brightness (straight to the screen: bloom takes it from there).
const STRIP_GLOW: f32 = 7.0;
const FRAME_GLOW: f32 = 4.0;

fn hash2(p: vec2<f32>) -> f32 {
    var q = fract(vec3(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// A pulse train filtered by a box (as the city's `f_pulse`): the share of a footprint `w` wide (in
// periods) about `x` that falls in [a, b] of each unit period. A grid seen from afar is its
// average, not moiré.
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

// How much of a footprint `w` across a shape covers, from its signed distance `d` (negative in it).
fn cover(d: f32, w: f32) -> f32 {
    return clamp(0.5 - d / max(w, 1e-4), 0.0, 1.0);
}

// A grid's joints: cells `size` across, joints `JOINT` wide.
fn joints(uv: vec2<f32>, size: vec2<f32>, fw: vec2<f32>) -> f32 {
    let x = pulse(uv.x / size.x, 0.0, JOINT / size.x, fw.x / size.x);
    let y = pulse(uv.y / size.y, 0.0, JOINT / size.y, fw.y / size.y);
    return max(x, y);
}

// Hazard stripes, diagonal: 1 yellow, 0 black.
fn stripes(uv: vec2<f32>, fw: vec2<f32>) -> f32 {
    return 1.0 - pulse((uv.x + uv.y) / HAZARD_PERIOD, 0.0, 0.5, (fw.x + fw.y) / HAZARD_PERIOD);
}

// A light colour from a paint: its brightest channel 1.
fn as_light(c: vec3<f32>) -> vec3<f32> {
    return c / max(max(c.r, c.g), max(c.b, 1e-3));
}

// The band's arrows: a pair of triangles every `ARROW_EVERY` along `a` (m), pointing along it,
// `v` up from the band's foot. Signed distance, m.
fn arrows(a: f32, v: f32) -> f32 {
    let mid = (BAND_Y1 - BAND_Y0) * 0.5;
    let half = mid * 0.62;
    let len = 1.5;
    let k = half / len;
    let s = sqrt(1.0 + k * k);
    var d = 1e3;
    for (var i = 0; i < 2; i++) {
        let x = a - f32(i) * 1.9;
        let tri = max(-x, (abs(v - mid) - half * (1.0 - x / len)) / s);
        d = min(d, max(tri, x - len));
    }
    return d;
}

// A floor chevron pointing along `a` (m, its tip at 0), `c` across the lane (m). Signed distance.
fn chevron(a: f32, c: f32) -> f32 {
    let reach = 3.0;
    let stroke = 0.55;
    var d = 1e3;
    for (var i = 0; i < 2; i++) {
        let tip = -f32(i) * 1.7;
        let line = abs(a - tip + abs(c)) * 0.7071 - stroke;
        d = min(d, max(line, abs(c) - reach));
    }
    return d;
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
    seed = in.color.a * 255.0;
#endif
    var uv = vec2(0.0);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    // Every derivative here, where control flow is still uniform: below, the shader branches on
    // the face's surface, and WebGPU rejects derivatives under such a branch.
    let fw = max(fwidth(uv), vec2(1e-4));
    let fm = max(fw.x, fw.y);

    let ink = station.paint[min(paint_i, 7u)].rgb;
    let white = station.paint[0].rgb;
    let yellow = station.paint[YELLOW].rgb;
    let dark = station.paint[DARK].rgb;
    var albedo = white;
    var rough = 0.45;
    var metal = 0.0;
    var glow = vec3(0.0);

    if (surface == WALL) {
        let size = vec2(PANEL_W, PANEL_H);
        let cell = floor(uv / size);
        let local = uv - cell * size;
        let h = hash2(cell + vec2(seed * 0.37, seed * 0.11));
        albedo = white * (0.985 + 0.05 * (hash2(cell * 1.7 + 3.1) - 0.5));
        // A vent: slats across the middle of its panel.
        if (h < 0.07) {
            let inset = band(local.x, 0.35, PANEL_W - 0.35, fw.x) * band(local.y, 0.4, PANEL_H - 0.4, fw.y);
            let slats = pulse(local.y / 0.16, 0.0, 0.45, fw.y / 0.16);
            albedo = mix(albedo, mix(white * 0.62, dark * 2.0, slats), inset);
            rough = mix(rough, 0.7, inset);
        } else if (h < 0.15) {
            // An access hatch: a square's outline near a corner.
            let c = vec2(select(0.55, PANEL_W - 0.55, h > 0.11), 0.55);
            let q = abs(local - c) - vec2(0.32);
            let box = max(q.x, q.y);
            albedo *= 1.0 - 0.45 * cover(abs(box) - 0.018, fm);
        }
        albedo *= 1.0 - 0.45 * joints(uv, size, fw);
        // The kick plate at the foot, a rule of the paint over it, or chevrons there.
        let kick = band(uv.y, -1e3, KICK, fw.y);
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), kick);
        } else {
            albedo = mix(albedo, white * 0.32, kick);
        }
        if ((flags & F_BAND) != 0u) {
            albedo = mix(albedo, ink, band(uv.y, KICK, KICK + 0.18, fw.y));
            // The band, a dark rule over and under it, and its arrows in white.
            let b = band(uv.y, BAND_Y0, BAND_Y1, fw.y);
            albedo = mix(albedo, ink, b);
            let rules = band(uv.y, BAND_Y0 - 0.22, BAND_Y0 - 0.1, fw.y) + band(uv.y, BAND_Y1 + 0.1, BAND_Y1 + 0.22, fw.y);
            albedo = mix(albedo, dark, rules);
            if ((flags & F_ARROWS) != 0u) {
                let along = select(uv.x, -uv.x, (flags & F_BACK) != 0u);
                let a = along - floor(along / ARROW_EVERY) * ARROW_EVERY - ARROW_EVERY * 0.5;
                let d = arrows(a, uv.y - BAND_Y0);
                albedo = mix(albedo, white, cover(d, fm) * b);
            }
            rough = mix(rough, 0.35, b);
        }
    } else if (surface == FLOOR) {
        // Deck plates, each a shade its own; the suits' lanes and their chevrons.
        let size = vec2(PLATE);
        let cell = floor(uv / size);
        albedo = vec3(0.46, 0.47, 0.49) * (0.97 + 0.07 * (hash2(cell + vec2(seed)) - 0.5));
        rough = 0.62;
        albedo *= 1.0 - 0.5 * joints(uv, size, fw);
        // Studs at the plates' corners.
        let corner = uv - floor(uv / size + 0.5) * size;
        albedo *= 1.0 - 0.25 * cover(length(corner) - 0.12, fm);
        if ((flags & F_LANES) != 0u) {
            var paint = 0.0;
            for (var k = 0; k < 2; k++) {
                let x = select(LANE_X0, LANE_X1, k == 1);
                let c = uv.y - x;
                // The lane's edges.
                paint = max(paint, band(abs(c), LANE_HALF - 0.4, LANE_HALF, fw.y));
                // Its chevrons, pointing the way it runs (the first towards the lifts, +z).
                let along = select(uv.x, -uv.x, k == 1);
                let a = along - floor(along / CHEVRON_EVERY) * CHEVRON_EVERY - CHEVRON_EVERY * 0.5;
                paint = max(paint, cover(chevron(a, c), fm));
            }
            albedo = mix(albedo, ink * 0.9, paint);
            rough = mix(rough, 0.4, paint);
        }
    } else if (surface == CEILING) {
        let size = vec2(PLATE);
        albedo = white * 0.86 * (1.0 - 0.35 * joints(uv, size, fw));
        rough = 0.6;
        // Light strips along the deck, broken every 8 m.
        let w = 1.0 / STRIP_EVERY;
        let strip = pulse(uv.y / STRIP_EVERY, 0.5 - 0.5 * w, 0.5 + 0.5 * w, fw.y / STRIP_EVERY)
            * pulse(uv.x / 8.0, 0.04, 0.96, fw.x / 8.0);
        albedo = mix(albedo, white, strip);
        glow = vec3(1.0, 0.985, 0.95) * STRIP_GLOW * strip;
    } else if (surface == PILLAR) {
        albedo = vec3(0.34, 0.35, 0.37) * (1.0 - 0.4 * joints(uv, vec2(1.2, 2.0), fw));
        rough = 0.5;
        let collar = band(uv.y, COLLAR_Y0, COLLAR_Y1, fw.y);
        albedo = mix(albedo, ink, collar);
        albedo = mix(albedo, white, band(uv.y, COLLAR_Y0 - 0.25, COLLAR_Y0 - 0.1, fw.y) + band(uv.y, COLLAR_Y1 + 0.1, COLLAR_Y1 + 0.25, fw.y));
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(uv.y, -1e3, KICK * 1.2, fw.y));
        }
    } else if (surface == TRIM) {
        albedo = ink;
        rough = 0.4;
    } else if (surface == GLOW) {
        albedo = ink * 0.4;
        glow = as_light(ink) * FRAME_GLOW;
    } else if (surface == DOOR) {
        // Two leaves meeting at the middle, ribbed across every 2 m; chevrons along the meeting.
        albedo = mix(white * 0.9, vec3(0.6, 0.63, 0.68), 0.35);
        rough = 0.5;
        albedo *= 1.0 - 0.5 * pulse(uv.y / 2.0, 0.0, 0.06, fw.y / 2.0);
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(abs(uv.x), -1.0, 1.1, fw.x));
        }
        albedo = mix(albedo, dark, band(abs(uv.x), -1.0, 0.04, fw.x));
    } else if (surface == WINDOW) {
        // Dark glass; screens lit behind it.
        albedo = vec3(0.012, 0.016, 0.02);
        rough = 0.06;
        let size = vec2(1.6, 1.0);
        let cell = floor(uv / size);
        let local = uv - cell * size;
        let h = hash2(cell + vec2(seed));
        let screen = band(local.x, 0.15, size.x - 0.15, fw.x) * band(local.y, 0.18, size.y - 0.18, fw.y) * step(0.35, h);
        let lines = 0.55 + 0.45 * pulse(local.y / 0.1, 0.0, 0.5, fw.y / 0.1);
        let tint = select(vec3(0.25, 0.85, 1.0), vec3(1.0, 0.7, 0.25), h > 0.9);
        glow = tint * 1.6 * screen * lines;
    } else if (surface == HAZARD) {
        albedo = mix(dark, yellow, stripes(uv, fw));
        rough = 0.55;
    }

    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = clamp(rough, 0.05, 1.0);
    pbr.material.metallic = metal;
    pbr.material.emissive = vec4(glow, 0.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
