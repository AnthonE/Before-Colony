// The city inside the colony (`city.rs`): buildings, pavements, the ground, the canal, trees, the
// end caps, from `bc_client_core::city_mesh`'s meshes. Each vertex says what surface it is (its
// colour's red, `city_mesh::Surface`), with a seed, ambient occlusion and its building's height;
// walls carry metres along and up, the ground its place on the strip (`s`, `x`).
//
// The colony is lit strip by strip: each by the mirror-borne sun in the window over it. On the
// camera's strip that's the scene's one directional light, with Bevy's lighting and shadows; on the
// others (kilometres off, deep in the haze) a simple key-and-sky light in their own frame. The
// haze is the camera's distance fog, so everything else inside fades the same.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import bc::noise::{hash13, noise3}
#import bc::city::{city_cell, atlas_texel, city_paint, CANAL_WIDTH}

struct City {
    // xyz: where the render origin is in the colony's frame; w: the camera's strip.
    origin: vec4<f32>,
    // x: daylight 0..1; y: lamps lit 0..1; z: seconds; w: the sun's elevation (rad).
    day: vec4<f32>,
    // x: the key light (lux) on the other strips; y: the sky's light (nits); z: 1 while the camera
    // is in a key place's room (the scene's light is then the room's); w: unused.
    light: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> city: City;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var atlas: texture_2d<f32>;

const TAU: f32 = 6.2831853;
const PI: f32 = 3.14159265;
const FIRST_WINDOW: f32 = 0.4;
const RADIUS: f32 = 3200.0;
// A key place's room is lit by its own lamps (nits on a white wall): exposed at EV 8 inside, it's
// dim seen from the sunlit street through its door, as rooms are.
const INDOOR: f32 = 300.0;

// Which land strip a point of the colony's frame is over (−1: a window).
fn strip_of(p: vec3<f32>) -> f32 {
    let a = atan2(p.z, p.y) - (FIRST_WINDOW - TAU / 12.0);
    let rel = a - floor(a / TAU) * TAU;
    let k = floor(rel / (TAU / 3.0));
    if (rel - k * (TAU / 3.0) < TAU / 6.0) {
        return -1.0;
    }
    return k;
}

// Up (towards the axis), and the way to the sun, over strip `k`.
fn strip_up(k: f32) -> vec3<f32> {
    let a = FIRST_WINDOW + k * (TAU / 3.0) + TAU / 6.0;
    return -vec3(0.0, cos(a), sin(a));
}

fn key_light(k: f32) -> vec3<f32> {
    let e = city.day.w;
    return vec3(1.0, 0.0, 0.0) * cos(e) + strip_up(k) * sin(e);
}

// A wall's windows, by floor and bay: whether here's glass (x), and whether it's lit tonight (y).
fn windows(uv: vec2<f32>, bay: f32, storey: f32, ground: f32, seed: f32) -> vec2<f32> {
    let up = uv.y - ground;
    if (up < 0.0) {
        // The ground floor: shopfronts, glazed between pillars, lit more often than not.
        let fx = fract(uv.x / 4.2);
        let glass = step(0.08, fx) * step(fx, 0.92) * step(0.7, uv.y) * step(uv.y, ground - 0.8);
        let lit = step(hash13(vec3(floor(uv.x / 4.2), 0.0, seed)), 0.75);
        return vec2(glass, lit);
    }
    let fl = floor(up / storey);
    let fy = fract(up / storey);
    let col = floor(uv.x / bay);
    let fx = fract(uv.x / bay);
    let glass = step(0.2, fy) * step(fy, 0.75) * step(0.14, fx) * step(fx, 0.86);
    let lit = step(hash13(vec3(fl, col, seed)), city.day.y * 0.6);
    return vec2(glass, lit);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    var surface = 0u;
    var seed = 0.0;
    var ao = 1.0;
    var tall = 0.0;
#ifdef VERTEX_COLORS
    surface = u32(in.color.r * 255.0 + 0.5);
    seed = in.color.g * 255.0;
    ao = in.color.b;
    tall = in.color.a;
#endif
    var uv = vec2(0.0);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    let p = in.world_position.xyz + city.origin.xyz;
    let lamps = city.day.y;
    let warm = vec3(1.0, 0.78, 0.5);
    var albedo = vec3(0.5);
    var rough = 0.85;
    var metal = 0.0;
    var glow = vec3(0.0);
    var indoor = false;

    if (surface == 0u) {
        // The ground, from the block atlas: streets, the avenue, parks, yards. The canal's channel is
        // left open (its chunk draws the water and its walls).
        let strip = i32(seed + 0.5);
        let cell = city_cell(uv.x, uv.y);
        let t = textureLoad(atlas, atlas_texel(strip, cell), 0);
        if (i32(t.r * 255.0 + 0.5) == 4) {
            let mid = (cell.rect.x + cell.rect.y) * 0.5;
            if (abs(uv.x - mid) < CANAL_WIDTH * 0.5 && uv.y > cell.rect.z && uv.y < cell.rect.w) {
                discard;
            }
        }
        let g = city_paint(cell, t, false);
        albedo = g.albedo;
        rough = 0.92;
        glow = warm * g.lamps * lamps * 900.0;
    } else if (surface == 1u || surface == 11u) {
        // Walls: plaster, concrete, brick, stone, with their windows.
        let tint = hash13(vec3(seed, 3.0, 7.0));
        var base = vec3(0.55, 0.53, 0.5);
        if (tint < 0.2) {
            base = vec3(0.62, 0.55, 0.45);
        } else if (tint < 0.35) {
            base = vec3(0.45, 0.29, 0.21);
        } else if (tint < 0.55) {
            base = vec3(0.72, 0.71, 0.67);
        } else if (tint < 0.7) {
            base = vec3(0.33, 0.34, 0.36);
        } else if (tint < 0.8) {
            base = vec3(0.4, 0.47, 0.46);
        }
        let hall = surface == 11u;
        let w = windows(uv, select(3.2, 6.0, hall), select(3.6, 7.2, hall), 5.0, seed);
        albedo = mix(base * (0.88 + 0.2 * noise3(vec3(uv * 0.4, seed))), vec3(0.04, 0.05, 0.06), w.x);
        rough = mix(0.85, 0.12, w.x);
        glow = warm * w.x * w.y * lamps * 420.0 * (0.6 + 0.8 * hash13(vec3(floor(uv / 3.0), seed)));
    } else if (surface == 12u) {
        // Curtain walls of glass on towers (and the lift's shaft).
        let fx = fract(uv.x / 1.6);
        let fy = fract(uv.y / 3.6);
        let mullion = 1.0 - step(0.06, fx) * step(fx, 0.94) * step(0.05, fy) * step(fy, 0.95);
        albedo = mix(vec3(0.11, 0.16, 0.19), vec3(0.3, 0.32, 0.34), mullion);
        rough = mix(0.08, 0.5, mullion);
        metal = mix(0.5, 0.8, mullion);
        let lit = step(hash13(vec3(floor(uv.y / 3.6), floor(uv.x / 6.4), seed)), lamps * 0.55);
        glow = warm * lit * (1.0 - mullion) * 300.0;
    } else if (surface == 2u) {
        // Roofs: gravel, plant, a darker patch or two.
        albedo = vec3(0.3, 0.31, 0.33) * (0.75 + 0.35 * noise3(vec3(uv * 0.2, seed)));
        rough = 0.95;
    } else if (surface == 3u) {
        // Pavement slabs.
        let j = min(fract(uv.x / 1.5), fract(uv.y / 1.5));
        albedo = vec3(0.5, 0.5, 0.49) * (0.9 + 0.1 * hash13(vec3(floor(uv / 1.5), 1.0))) * mix(0.8, 1.0, step(0.04, j));
        rough = 0.9;
    } else if (surface == 4u) {
        albedo = vec3(0.6, 0.6, 0.58);
    } else if (surface == 5u) {
        // The canal's water: dark, smooth, catching the sky (there's no sky to reflect but the
        // haze's light, so that's added: more of it the more glancing the view).
        albedo = vec3(0.03, 0.06, 0.07);
        rough = 0.05;
        let ripple = noise3(vec3(uv * 0.15, city.day.z * 0.3)) - 0.5;
        pbr.N = normalize(pbr.N + vec3(ripple * 0.06, ripple * 0.04, ripple * 0.05));
        let v = normalize(view.world_position - in.world_position.xyz);
        let fresnel = 0.03 + 0.97 * pow(1.0 - max(dot(v, normalize(pbr.N)), 0.0), 5.0);
        glow = vec3(0.6, 0.72, 0.9) * city.light.y * 2.5 * fresnel + warm * lamps * fresnel * 30.0;
    } else if (surface == 6u) {
        albedo = vec3(0.42, 0.4, 0.36) * (0.85 + 0.2 * noise3(vec3(uv * 0.5, 2.0)));
    } else if (surface == 7u) {
        // Leaves.
        let v = hash13(vec3(floor(seed * 7.0), 2.0, 9.0));
        albedo = mix(vec3(0.1, 0.22, 0.07), vec3(0.2, 0.3, 0.09), v) * (0.8 + 0.4 * noise3(in.world_position.xyz * 0.6));
        rough = 0.95;
    } else if (surface == 8u) {
        albedo = vec3(0.25, 0.18, 0.12);
    } else if (surface == 9u) {
        // The site's steel, in primer.
        albedo = vec3(0.55, 0.22, 0.1);
        rough = 0.6;
        metal = 0.3;
    } else if (surface == 10u) {
        albedo = vec3(0.15, 0.16, 0.17);
        rough = 0.4;
        metal = 0.7;
    } else if (surface == 13u) {
        // An end cap's inner face: terraces stepping in towards the axis port, its lamps by night;
        // the port dark in a ring of lights.
        let r = length(p.yz);
        let band = fract(r / 160.0);
        let terrace = step(0.82, band);
        albedo = mix(vec3(0.5, 0.52, 0.55), vec3(0.28, 0.3, 0.33), terrace);
        let a = atan2(p.z, p.y);
        let windows_lit = step(0.6, band) * step(band, 0.78) * step(hash13(vec3(floor(r / 160.0), floor(a * 400.0), 3.0)), lamps * 0.5);
        glow = warm * windows_lit * 260.0;
        if (r < 360.0) {
            albedo = vec3(0.05, 0.05, 0.06);
            glow += vec3(0.8, 0.9, 1.0) * step(abs(r - 345.0), 4.0) * step(0.5, fract(a * 24.0 / TAU)) * 600.0;
        }
        rough = 0.8;
    }

    if (surface >= 14u && surface <= 18u) {
        // Inside a key place's room (`city_mesh::Surface::Interior` to `Display`), its seed what the
        // place is: 1 the bar, 2 the Exchange, 3 the Charter Board.
        indoor = true;
        let kind = u32(seed + 0.5);
        let bar = kind == 1u;
        if (surface == 14u) {
            // Plaster over a dark dado (walls' UVs are metres along, and up from the street).
            let dado = step(uv.y, 1.3);
            let plaster = select(vec3(0.74, 0.72, 0.68), vec3(0.6, 0.45, 0.32), bar);
            albedo = mix(plaster, vec3(0.2, 0.15, 0.11), dado) * (0.94 + 0.08 * noise3(vec3(uv * 0.7, 4.0)));
            rough = 0.8;
        } else if (surface == 15u) {
            // Polished stone tiles; the bar's boards.
            if (bar) {
                let plank = fract(uv.y / 0.25);
                albedo = vec3(0.32, 0.2, 0.11) * (0.8 + 0.3 * hash13(vec3(floor(uv.y / 0.25), floor(uv.x / 2.4), 6.0)));
                albedo *= mix(0.7, 1.0, step(0.06, plank));
                rough = 0.55;
            } else {
                let j = min(fract(uv.x / 1.2), fract(uv.y / 1.2));
                let tile = hash13(vec3(floor(uv / 1.2), 8.0));
                albedo = mix(vec3(0.62, 0.6, 0.56), vec3(0.42, 0.42, 0.44), step(0.5, tile)) * mix(0.75, 1.0, step(0.03, j));
                rough = 0.25;
            }
        } else if (surface == 16u) {
            // The ceiling and its lamps: panels in a grid (warm in the bar).
            let g = abs(fract(uv / 5.0) - vec2(0.5));
            let panel = step(max(g.x, g.y), 0.14);
            albedo = vec3(0.82, 0.8, 0.77);
            glow = select(vec3(1.0, 0.96, 0.9), warm, bar) * panel * 2600.0;
        } else if (surface == 17u) {
            // The counter: a steel desk; the bar's polished wood.
            albedo = select(vec3(0.2, 0.22, 0.24), vec3(0.34, 0.18, 0.08), bar);
            rough = 0.3;
            metal = select(0.6, 0.0, bar);
        } else {
            // The back wall: what the place is.
            if (kind == 2u) {
                // The Exchange's boards: rows of prices, green and amber, changing every few seconds.
                let band = step(3.0, uv.y) * step(fract((uv.y - 3.0) / 1.1), 0.7);
                let cell = vec3(floor(uv.x / 0.9), floor((uv.y - 3.0) / 1.1), floor(city.day.z / 3.0));
                let lit = step(0.25, hash13(cell));
                let tint = mix(vec3(0.25, 1.0, 0.45), vec3(1.0, 0.62, 0.15), step(0.6, hash13(cell.xyz + vec3(0.0, 0.0, 5.0))));
                albedo = vec3(0.04, 0.045, 0.05);
                glow = tint * lit * band * 900.0;
            } else if (kind == 3u) {
                // The Charter Board: notices pinned on cork, lit by lamps over them.
                let fx = fract(uv.x / 1.4);
                let fy = fract((uv.y - 1.4) / 1.0);
                let cell = vec3(floor(uv.x / 1.4), floor((uv.y - 1.4) / 1.0), 9.0);
                let paper = step(0.1, fx) * step(fx, 0.85) * step(0.1, fy) * step(fy, 0.9)
                    * step(1.4, uv.y) * step(uv.y, 6.4) * step(hash13(cell), 0.75);
                albedo = mix(vec3(0.36, 0.25, 0.15), vec3(0.9, 0.88, 0.8), paper);
                glow = warm * paper * 60.0;
            } else {
                // The bar's shelves: bottles catching the lamps.
                let rows = step(1.2, uv.y) * step(uv.y, 3.6);
                let shelf = step(fract((uv.y - 1.2) / 0.6), 0.08);
                let neck = step(0.2, fract(uv.x / 0.16)) * step(fract(uv.x / 0.16), 0.7) * step(0.15, fract((uv.y - 1.2) / 0.6));
                let bottle = rows * neck * (1.0 - shelf);
                let hue = hash13(vec3(floor(uv.x / 0.16), floor((uv.y - 1.2) / 0.6), 2.0));
                albedo = mix(vec3(0.28, 0.16, 0.08), mix(vec3(0.1, 0.3, 0.12), vec3(0.45, 0.25, 0.05), hue), bottle);
                glow = warm * bottle * 140.0;
                rough = mix(0.6, 0.15, bottle);
            }
        }
    }

    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = rough;
    pbr.material.metallic = metal;
    pbr.material.emissive = vec4(glow, 1.0);
    pbr.diffuse_occlusion *= ao;
    pbr.specular_occlusion *= ao;

    // On the camera's strip, the scene's sun and shadows; elsewhere, that strip's own sun and sky.
    // From inside a room the scene's light is the room's, so the street through its door is lit by
    // its own sun and sky as the other strips are (and blazes, at the room's exposure). A room's
    // own surfaces are lit by its lamps.
    var out: FragmentOutput;
    let lit = apply_pbr_lighting(pbr);
    let k = strip_of(p);
    let n = normalize(pbr.N);
    let key = max(dot(n, key_light(max(k, 0.0))), 0.0) * city.light.x / PI;
    let sky = city.light.y * (0.6 + 0.4 * dot(n, strip_up(max(k, 0.0))));
    let own = vec4((albedo * (key + sky) * ao) * view.exposure + glow * view.exposure, 1.0);
    let scene = (k < 0.0 || abs(k - city.origin.w) < 0.5) && city.light.z < 0.5;
    out.color = select(own, lit, scene);
    if (indoor) {
        out.color = vec4((albedo * INDOOR * ao + glow) * view.exposure, 1.0);
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
