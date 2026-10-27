// The colony's station language (`StationMaterial`, `bc_client_core::station`): walls, floors,
// ceilings, pillars, doors and markings, painted from what each face carries in its vertex colour:
// r its surface, g its paint (an entry of the palette), b what's painted on it (the band and its
// arrows, chevrons, the lanes), a a seed. UVs are metres across the face, so panels line up from one
// face to the next. The sizes it paints at are its material's (`station::Scale`: a suit's deck, a
// person's halls), in the uniform; the surfaces, flags and safety paints are `station`'s (its tests
// read them here).
//
// White panels with fine joints, a vent or an access hatch here and there; the lines' colours for
// wayfinding; safety yellow and black only where there's a hazard; light in strips.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import bc::anime::paint_bands

// The palette's size (`station::PAINTS`).
const PAINTS: u32 = 11u;

struct Station {
    // rgb: the palette, linear (`station::PAINT`).
    paint: array<vec4<f32>, 11>,
    // x: seconds.
    time: vec4<f32>,
    // The sizes it paints at, m (`station::Scale::uniform`). x, y: a wall panel; z: a deck plate;
    // w: a joint.
    size: vec4<f32>,
    // x, y: the band round the walls; z: its arrows' spacing; w: the kick plate's height.
    band: vec4<f32>,
    // x, y: the lanes' middles across a floor; z: half a lane's width; w: their chevrons' spacing.
    lanes: vec4<f32>,
    // x: the ceiling's light strips' spacing; y: the hazard stripes' period; z, w: a pillar's collar.
    marks: vec4<f32>,
    // x: 1 to paint it (`?look=anime`, `bc::anime`); y: the kick plate's paint.
    look: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> station: Station;

// Surfaces (`station::Surface`).
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

// The palette's white and safety colours.
const WHITE: u32 = 0u;
const YELLOW: u32 = 4u;
const DARK: u32 = 5u;

// The light strips' brightness (straight to the screen: bloom takes it from there).
const STRIP_GLOW: f32 = 7.0;
const FRAME_GLOW: f32 = 1.4;

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

// A grid's joints: cells `size` across, joints `joint` wide.
fn joints(uv: vec2<f32>, size: vec2<f32>, joint: f32, fw: vec2<f32>) -> f32 {
    let x = pulse(uv.x / size.x, 0.0, joint / size.x, fw.x / size.x);
    let y = pulse(uv.y / size.y, 0.0, joint / size.y, fw.y / size.y);
    return max(x, y);
}

// Hazard stripes, diagonal: 1 yellow, 0 black.
fn stripes(uv: vec2<f32>, fw: vec2<f32>) -> f32 {
    let period = station.marks.y;
    return 1.0 - pulse((uv.x + uv.y) / period, 0.0, 0.5, (fw.x + fw.y) / period);
}

// A light colour from a paint: its brightest channel 1.
fn as_light(c: vec3<f32>) -> vec3<f32> {
    return c / max(max(c.r, c.g), max(c.b, 1e-3));
}

// The band's arrows: a pair of triangles along `a` (m, the first's tip at 0), pointing along it,
// `v` up from the band's foot, the band `bh` high. Signed distance, m.
fn arrows(a: f32, v: f32, bh: f32) -> f32 {
    let mid = bh * 0.5;
    let half = mid * 0.62;
    let len = bh * 0.5;
    let k = half / len;
    let s = sqrt(1.0 + k * k);
    var d = 1e3;
    for (var i = 0; i < 2; i++) {
        let x = a - f32(i) * bh * 0.63;
        let tri = max(-x, (abs(v - mid) - half * (1.0 - x / len)) / s);
        d = min(d, max(tri, x - len));
    }
    return d;
}

// A floor chevron pointing along `a` (m, its tip at 0), `c` across the lane (m), the lane `half`
// wide each side. Signed distance.
fn chevron(a: f32, c: f32, half: f32) -> f32 {
    let reach = half * 0.75;
    let stroke = half * 0.1375;
    var d = 1e3;
    for (var i = 0; i < 2; i++) {
        let tip = -f32(i) * half * 0.425;
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

    let panel = station.size.xy;
    let plate = station.size.z;
    let joint = station.size.w;
    let b0 = station.band.x;
    let b1 = station.band.y;
    let bh = b1 - b0;
    let kick_h = station.band.w;
    let ink = station.paint[min(paint_i, PAINTS - 1u)].rgb;
    let white = station.paint[WHITE].rgb;
    let yellow = station.paint[YELLOW].rgb;
    let dark = station.paint[DARK].rgb;
    let kick_c = station.paint[min(u32(station.look.y + 0.5), PAINTS - 1u)].rgb;
    var albedo = white;
    var rough = 0.45;
    var metal = 0.0;
    var glow = vec3(0.0);

    if (surface == WALL) {
        let cell = floor(uv / panel);
        let local = uv - cell * panel;
        let h = hash2(cell + vec2(seed * 0.37, seed * 0.11));
        albedo = white * (0.985 + 0.05 * (hash2(cell * 1.7 + 3.1) - 0.5));
        // A vent: slats across the middle of its panel.
        if (h < 0.07) {
            let mx = panel.x * 0.117;
            let my = panel.y * 0.2;
            let inset = band(local.x, mx, panel.x - mx, fw.x) * band(local.y, my, panel.y - my, fw.y);
            let slat = panel.y * 0.08;
            let slats = pulse(local.y / slat, 0.0, 0.45, fw.y / slat);
            albedo = mix(albedo, mix(white * 0.62, dark * 2.0, slats), inset);
            rough = mix(rough, 0.7, inset);
        } else if (h < 0.15) {
            // An access hatch: a square's outline near a corner.
            let inx = panel.x * 0.183;
            let c = vec2(select(inx, panel.x - inx, h > 0.11), panel.y * 0.275);
            let q = abs(local - c) - vec2(panel.y * 0.16);
            let box = max(q.x, q.y);
            albedo *= 1.0 - 0.45 * cover(abs(box) - joint * 0.6, fm);
        }
        albedo *= 1.0 - 0.45 * joints(uv, panel, joint, fw);
        // The kick plate at the foot, a rule of the paint over it, or chevrons there.
        let kick = band(uv.y, -1e3, kick_h, fw.y);
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), kick);
        } else {
            albedo = mix(albedo, kick_c, kick);
        }
        if ((flags & F_BAND) != 0u) {
            albedo = mix(albedo, ink, band(uv.y, kick_h, kick_h + bh * 0.06, fw.y));
            // The band, a dark rule over and under it, and its arrows in white.
            let b = band(uv.y, b0, b1, fw.y);
            albedo = mix(albedo, ink, b);
            let rules = band(uv.y, b0 - bh * 0.073, b0 - bh * 0.033, fw.y) + band(uv.y, b1 + bh * 0.033, b1 + bh * 0.073, fw.y);
            albedo = mix(albedo, dark, rules);
            if ((flags & F_ARROWS) != 0u) {
                let every = station.band.z;
                let along = select(uv.x, -uv.x, (flags & F_BACK) != 0u);
                let a = along - floor(along / every) * every - every * 0.5;
                let d = arrows(a, uv.y - b0, bh);
                albedo = mix(albedo, white, cover(d, fm) * b);
            }
            rough = mix(rough, 0.35, b);
        }
    } else if (surface == FLOOR) {
        // Deck plates, each a shade its own; the lanes and their chevrons.
        let size = vec2(plate);
        let cell = floor(uv / size);
        albedo = vec3(0.46, 0.47, 0.49) * (0.97 + 0.07 * (hash2(cell + vec2(seed)) - 0.5));
        rough = 0.62;
        albedo *= 1.0 - 0.5 * joints(uv, size, joint, fw);
        // Studs at the plates' corners.
        let corner = uv - floor(uv / size + 0.5) * size;
        albedo *= 1.0 - 0.25 * cover(length(corner) - plate * 0.03, fm);
        if ((flags & F_LANES) != 0u) {
            let half = station.lanes.z;
            let every = station.lanes.w;
            var paint = 0.0;
            for (var k = 0; k < 2; k++) {
                let x = select(station.lanes.x, station.lanes.y, k == 1);
                let c = uv.y - x;
                // The lane's edges.
                paint = max(paint, band(abs(c), half * 0.9, half, fw.y));
                // Its chevrons, pointing the way it runs (the first along +u, the second back).
                let along = select(uv.x, -uv.x, k == 1);
                let a = along - floor(along / every) * every - every * 0.5;
                paint = max(paint, cover(chevron(a, c, half), fm));
            }
            albedo = mix(albedo, ink * 0.9, paint);
            rough = mix(rough, 0.4, paint);
        }
    } else if (surface == CEILING) {
        let size = vec2(plate);
        albedo = white * 0.86 * (1.0 - 0.35 * joints(uv, size, joint, fw));
        rough = 0.6;
        // Light strips along the face, broken every so often.
        let every = station.marks.x;
        let gap = every * 0.667;
        let w = 1.0 / 12.0;
        let strip = pulse(uv.y / every, 0.5 - 0.5 * w, 0.5 + 0.5 * w, fw.y / every)
            * pulse(uv.x / gap, 0.04, 0.96, fw.x / gap);
        albedo = mix(albedo, white, strip);
        glow = vec3(1.0, 0.985, 0.95) * STRIP_GLOW * strip;
    } else if (surface == PILLAR) {
        albedo = vec3(0.34, 0.35, 0.37) * (1.0 - 0.4 * joints(uv, vec2(panel.x * 0.4, panel.y), joint, fw));
        rough = 0.5;
        let c0 = station.marks.z;
        let c1 = station.marks.w;
        let ch = c1 - c0;
        albedo = mix(albedo, ink, band(uv.y, c0, c1, fw.y));
        albedo = mix(albedo, white, band(uv.y, c0 - ch * 0.167, c0 - ch * 0.067, fw.y) + band(uv.y, c1 + ch * 0.067, c1 + ch * 0.167, fw.y));
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(uv.y, -1e3, kick_h * 1.2, fw.y));
        }
    } else if (surface == TRIM) {
        albedo = ink;
        rough = 0.4;
    } else if (surface == GLOW) {
        albedo = ink * 0.4;
        glow = as_light(ink) * FRAME_GLOW;
    } else if (surface == DOOR) {
        // Two leaves meeting at the middle, ribbed across every panel; chevrons along the meeting.
        albedo = mix(white * 0.9, vec3(0.6, 0.63, 0.68), 0.35);
        rough = 0.5;
        albedo *= 1.0 - 0.5 * pulse(uv.y / panel.y, 0.0, 0.06, fw.y / panel.y);
        if ((flags & F_CHEVRONS) != 0u) {
            albedo = mix(albedo, mix(dark, yellow, stripes(uv, fw)), band(abs(uv.x), -1.0, panel.x * 0.37, fw.x));
        }
        albedo = mix(albedo, dark, band(abs(uv.x), -1.0, joint * 1.33, fw.x));
    } else if (surface == WINDOW) {
        // Dark glass; screens lit behind it.
        albedo = vec3(0.012, 0.016, 0.02);
        rough = 0.06;
        let size = vec2(panel.x * 0.533, panel.y * 0.5);
        let cell = floor(uv / size);
        let local = uv - cell * size;
        let h = hash2(cell + vec2(seed));
        let screen = band(local.x, size.x * 0.094, size.x * 0.906, fw.x) * band(local.y, size.y * 0.18, size.y * 0.82, fw.y) * step(0.35, h);
        let line = size.y * 0.1;
        let lines = 0.55 + 0.45 * pulse(local.y / line, 0.0, 0.5, fw.y / line);
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
    if (station.look.x > 0.5) {
        let v = normalize(view.world_position.xyz - in.world_position.xyz);
        out.color = vec4(paint_bands(out.color.rgb, albedo, glow, pbr.N, v), out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
