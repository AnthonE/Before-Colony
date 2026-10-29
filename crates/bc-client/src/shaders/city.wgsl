// The city inside the colony (`city.rs`): buildings, pavements, the ground, the canal, trees, the
// street's lamps and benches, the end caps, from `bc_client_core::city_mesh`'s meshes. Each vertex
// says what surface it is (its colour's red, `city_mesh::Surface`), with a seed, ambient occlusion
// and its building's height (or the street's code); walls carry metres along and up, the ground its
// place on the strip (`s`, `x`).
//
// The colony is lit strip by strip: each by the mirror-borne sun in the window over it. On the
// camera's strip that's the scene's one directional light, with Bevy's lighting and shadows; on the
// others (kilometres off, deep in the haze) that strip's own key, sky and ground bounce, coloured by
// the hour (`bc::colony_sky::own_light`). The air between is the sky function's haze, drawn here
// (Bevy's distance fog is off for the city; what keeps it, people and cars, is matched to it), and
// glass and water reflect the sky function.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, calculate_diffuse_color, calculate_F0_dielectric},
    pbr_types::PbrInput,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::{view, lights},
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    lighting,
}
#import bc::noise::{hash13, noise3}
#ifdef CITY_DETAIL
#import bc::city::{city_cell, atlas_texel, city_paint, lantern_burn, streets_at, PLAZA_GAIN, CANAL_WIDTH, row_span, height_texel, HEIGHT_CELL, HEIGHT_S0, BLOCK, GRID_X0, KERB, GROUND_FLOOR, FLOOR}
#else
#import bc::city::{city_cell, atlas_texel, city_sketch, LAMP_MEAN, PLAZA_GAIN, CANAL_WIDTH, KERB, GROUND_FLOOR, FLOOR}
#endif
#import bc::facade::{facade, FacadeIn, city_place, F_MAX_HEIGHT}
#import bc::colony_sky::{Sky, haze, strip_at, strip_up, up_at, height_at, key_dir, own_light, bounce_light, fresnel, sky_reflection, sun_glint}

struct City {
    // xyz: where the render origin is in the colony's frame; w: the camera's strip.
    origin: vec4<f32>,
    // x: daylight 0..1; y: lamps lit 0..1; z: seconds; w: the sun's elevation (rad).
    day: vec4<f32>,
    // x: the key light (lux) on the other strips; y: the sky's light (nits); z: 1 while the camera
    // is in a key place's room (the scene's light is then the room's); w: the rooms' own light
    // (nits on a white wall), by the hour (`city.rs`'s `room_ev`).
    light: vec4<f32>,
    // The hour's air and light (`city.rs`'s colour script).
    sky: Sky,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> city: City;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var atlas: texture_2d<f32>;

const TAU: f32 = 6.2831853;
const PI: f32 = 3.14159265;
const RADIUS: f32 = 3200.0;
// A key place's room is lit by its own lamps (`city.light.w`, nits on a white wall): brighter by day,
// so it reads lit through its door from the street, and the street from in it isn't white.
// The ground's lamps by night: nits at a pool's peak a unit of albedo (`bc::city`'s `Paint::lamps`
// is relative to it), for night's EV 8.5 (`city_hour::EV_NIGHT`; scale by 2^(EV - 8.5) if that
// moves): pools under the lamps, streets that read as lines from the lift without outshining the
// windows. The colony's light strips inlaid in its ground (`Paint::glow`), nits.
const GROUND_LAMPS: f32 = 420.0;
const GROUND_STRIPS: f32 = 1800.0;

#ifdef CITY_DETAIL
// The long shadows (`COLONY_LOOK.md` pass 1.3), past what the shadow cascades reach: the height map
// under the block atlas (`city_atlas::heights`), the tallest roof over each 16 m cell. Steps of the
// march at most, and how far a step moves along the ground where a block's roofs are near the ray.
const LONG_STEPS: i32 = 32;
const LONG_STEP: f32 = 8.0;

// The tallest roof over height cell `c` of strip `strip`, m.
fn roof_cell(strip: i32, c: vec2<i32>) -> f32 {
    let t = height_texel(strip, c);
    return textureLoad(atlas, t.xy, 0)[t.z] * 255.0;
}

// The roofs' height at (s, x) of strip `strip`, between the cells' middles, so a shadow's edge is
// a soft line rather than the cells' steps.
fn roof(strip: i32, s: f32, x: f32) -> f32 {
    let g = vec2((x - GRID_X0) / HEIGHT_CELL - 0.5, (s - HEIGHT_S0) / HEIGHT_CELL - 0.5);
    let c = vec2<i32>(floor(g));
    let f = g - floor(g);
    let a = mix(roof_cell(strip, c), roof_cell(strip, c + vec2(1, 0)), f.x);
    let b = mix(roof_cell(strip, c + vec2(0, 1)), roof_cell(strip, c + vec2(1, 1)), f.x);
    return mix(a, b, f.y);
}

// How much of strip `strip`'s key light (towards `l`) reaches `p` (the colony's frame; unit normal
// `n`), 0..1: marched towards the light through the roofs. A block whose tallest roof (the block
// atlas's) the ray clears is crossed in one step; in the others it steps a few metres at a time.
// It stops above the tallest roof the rules allow, or off the strip.
fn long_shade(p: vec3<f32>, n: vec3<f32>, l: vec3<f32>, strip: i32) -> f32 {
    // Off the surface: off a wall far enough to leave its own cell behind (walls stand anywhere in
    // a cell, whose roof is the building's), and from a roof a cell on, past its own.
    let level = abs(dot(n, up_at(p)));
    let start = p + n * mix(HEIGHT_CELL * 0.75, 0.5, level);
    var t = mix(1.0, HEIGHT_CELL, smoothstep(0.6, 0.9, level) * step(4.0, height_at(p)));
    var shade = 1.0;
    for (var i = 0; i < LONG_STEPS; i++) {
        let q = start + l * t;
        let h = height_at(q);
        let place = city_place(q);
        if (h > F_MAX_HEIGHT || place.x < 0.0 || i32(place.x + 0.5) != strip) {
            break;
        }
        let s = place.y;
        let x = place.z;
        // How fast the ray crosses the strip (s) and runs along it (x), m a metre of ray.
        let r2 = max(dot(q.yz, q.yz), 1.0);
        let ds = RADIUS * (q.y * l.z - q.z * l.y) / r2;
        let dx = l.x;
        let cell = city_cell(s, x);
        let top = textureLoad(atlas, atlas_texel(strip, cell), 0).b * 510.0;
        if (h > top + 1.0) {
            // Clear of the whole block: on to its cell's edge.
            let x0 = GRID_X0 + BLOCK * f32(cell.bx);
            let span = row_span(cell.row);
            let tx = select((x0 + BLOCK - x) / dx, (x0 - x) / dx, dx < 0.0);
            let ts = select((span.y - s) / ds, (span.x - s) / ds, ds < 0.0);
            let along = select(1e6, tx, abs(dx) > 1e-4);
            let across = select(1e6, ts, abs(ds) > 1e-4);
            t += max(min(along, across), 0.0) + 1.0;
        } else {
            // A penumbra that widens with the distance to what casts it, about the sun's own. (The
            // map's heights are rounded up: a metre's grace.)
            let soft = 1.5 + 0.01 * t;
            shade = min(shade, smoothstep(-soft, soft, h + 1.0 - roof(strip, s, x)));
            if (shade < 0.01) {
                break;
            }
            // Longer steps further out, where the penumbra's wider anyway.
            t += (LONG_STEP + 0.04 * t) / max(length(vec2(ds, dx)), 0.25);
        }
    }
    return shade;
}

// The scene's sun (Bevy's one directional light) on a surface, as `apply_pbr_lighting` adds it
// before exposure, without its shadow: what a long shadow takes away.
// The Exchange floor's look (`docs/FLOOR.md`): a dark hall lit by its screens, anime-style neon.
const EX_CYAN: vec3<f32> = vec3(0.3, 0.88, 1.0);
const EX_PINK: vec3<f32> = vec3(1.0, 0.32, 0.82);
const EX_UP: vec3<f32> = vec3(0.3, 1.0, 0.55);
const EX_DOWN: vec3<f32> = vec3(1.0, 0.28, 0.42);

// A candlestick chart filling a panel: `fx` 0..1 across it, `y` m up, between `lo` and `hi` m;
// chart `k` of the room's, moving on a candle every 2 s at `t` s. Its light, in nits.
fn ex_chart(fx: f32, y: f32, lo: f32, hi: f32, k: f32, t: f32) -> vec3<f32> {
    let n = 28.0;
    let i = floor(fx * n) + floor(t / 2.0);
    let mid = 0.5 * (lo + hi);
    let amp = 0.3 * (hi - lo);
    let at = vec2(i, i + 1.0);
    let wave = mid + amp * (0.6 * sin(at * 0.31 + k * 2.1) + 0.3 * sin(at * 1.17 + k) + 0.1 * sin(at * 3.3));
    let o = wave.x;
    let c = wave.y;
    let spread = 0.06 * (hi - lo) * (0.4 + hash13(vec3(i, k, 3.0)));
    let cx = fract(fx * n);
    let body = step(abs(cx - 0.5), 0.3) * step(min(o, c), y) * step(y, max(o, c) + 0.02);
    let wick = step(abs(cx - 0.5), 0.06) * step(min(o, c) - spread, y) * step(y, max(o, c) + spread);
    let tint = select(EX_DOWN, EX_UP, c >= o);
    // The price's level lines, faint.
    let rule = step(abs(fract((y - lo) / ((hi - lo) / 5.0)) - 0.5), 0.015) * 0.06;
    return tint * max(body, wick) * 700.0 + EX_CYAN * rule * 700.0;
}

// Rows of prices on a board: lit cells in green and amber, changing every few seconds.
fn ex_rows(x: f32, y: f32, t: f32) -> vec3<f32> {
    let band = step(fract(y / 0.9), 0.62);
    let cell = vec3(floor(x / 0.7), floor(y / 0.9), floor(t / 3.0));
    let lit = step(0.3, hash13(cell)) * step(0.12, fract(x / 0.7)) * step(fract(x / 0.7), 0.88);
    let tint = mix(EX_UP, vec3(1.0, 0.68, 0.2), step(0.62, hash13(cell + vec3(0.0, 0.0, 5.0))));
    return tint * lit * band * 650.0;
}

fn scene_sun(pbr: PbrInput) -> vec3<f32> {
    let N = pbr.N;
    let V = pbr.V;
    let NdotV = max(dot(N, V), 0.0001);
    let rough = pbr.material.perceptual_roughness;
    var li: lighting::LightingInput;
    li.layers[lighting::LAYER_BASE].NdotV = NdotV;
    li.layers[lighting::LAYER_BASE].N = N;
    li.layers[lighting::LAYER_BASE].R = reflect(-V, N);
    li.layers[lighting::LAYER_BASE].perceptual_roughness = rough;
    li.layers[lighting::LAYER_BASE].roughness = lighting::perceptualRoughnessToRoughness(rough);
    li.P = pbr.world_position.xyz;
    li.V = V;
    li.diffuse_color = calculate_diffuse_color(pbr.material.base_color.rgb, pbr.material.metallic, 0.0, 0.0);
    li.metallic = pbr.material.metallic;
    li.F0_dielectric = calculate_F0_dielectric(pbr.material.reflectance);
    li.F0_metallic = pbr.material.base_color.rgb;
    li.F_ab = lighting::F_AB(rough, NdotV);
    return lighting::directional_light(0u, &li, true);
}
#endif

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
    // Every derivative the facades need, here where control flow is still uniform: below, the
    // shader branches on the vertex's surface, and WebGPU rejects derivatives under such a branch.
    let wp = in.world_position.xyz;
    let dp_dx = dpdx(wp);
    let dp_dy = dpdy(wp);
    let duv_dx = dpdx(uv);
    let duv_dy = dpdy(uv);
    // A pixel's footprint on a level surface, m across the strip and along the axis, which the
    // ground's patterns are filtered by: x is the colony's axis, and on a level surface the
    // pixel's yz step lies in the ground (`fwidth(uv)` would do, but uv.y is x itself, up to
    // 16,000 m, and loses millimetres close up).
    let ground_fp = vec2(length(dp_dx.yz) + length(dp_dy.yz), abs(dp_dx.x) + abs(dp_dy.x));
    let p = wp + city.origin.xyz;
    // The strip this is on (−1 over a window; `kk` its key light's strip either way), and whether
    // the scene's one light is its sun: on the camera's strip, unless the camera's in a room.
    let k = strip_at(p);
    let kk = max(k, 0.0);
    let scene = (k < 0.0 || abs(k - city.origin.w) < 0.5) && city.light.z < 0.5;
    // Towards the eye.
    let v = normalize(view.world_position - in.world_position.xyz);
    let lamps = city.day.y;
    let warm = vec3(1.0, 0.78, 0.5);
    var albedo = vec3(0.5);
    var rough = 0.85;
    var metal = 0.0;
    var glow = vec3(0.0);
    // The mirrors' sun caught in glass and water, where the scene's light doesn't draw it.
    var glint = vec3(0.0);
    var indoor = false;
    // How mirror-like a facade is (glass), for the sky's reflection below.
    var facade_refl = 0.0;

    let ground_cell = city_cell(uv.x, uv.y);
    if (surface == 0u || (surface == 3u && ground_cell.row != 0)) {
        // The ground, from the block atlas: streets, the avenue, parks, yards; and close up (surface
        // 3) the blocks' tops inside their kerbs, painted alike. The canal's channel is left open
        // (its chunk draws the water and its walls). The ground's seed is its strip; a block top's
        // isn't, so that's from where it is.
        let strip = select(i32(kk), i32(seed + 0.5), surface == 0u);
        let t = textureLoad(atlas, atlas_texel(strip, ground_cell), 0);
        if (surface == 0u && i32(t.r * 255.0 + 0.5) == 4) {
            let mid = (ground_cell.rect.x + ground_cell.rect.y) * 0.5;
            if (abs(uv.x - mid) < CANAL_WIDTH * 0.5 && uv.y > ground_cell.rect.z && uv.y < ground_cell.rect.w) {
                discard;
            }
        }
#ifdef CITY_DETAIL
        let g = city_paint(ground_cell, t, strip, false, ground_fp, lamps > 0.0);
#else
        let g = city_sketch(ground_cell, t, strip, false, ground_fp, lamps > 0.0);
#endif
        albedo = g.albedo;
        rough = g.roughness;
        glow = (warm * g.albedo * g.lamps * GROUND_LAMPS + g.glow * GROUND_STRIPS) * lamps
            + g.glow * (GROUND_STRIPS * 0.15);
    } else if (surface == 1u || surface == 2u || surface == 7u || surface == 8u || (surface >= 9u && surface <= 12u)
        || (surface >= 19u && surface <= 21u) || surface == 25u || surface == 26u) {
        // The buildings and the street (`bc::facade`): walls, roofs, the site's steel, the quays'
        // railings, the colony's halls, curtain walls, crowns and roof plant; trees, lamp posts and
        // their lanterns, benches. Their district and block kind from the block atlas.
        let place = city_place(p);
        var district = 0u;
        var kind = 0u;
        if (place.x >= 0.0) {
            let t = textureLoad(atlas, atlas_texel(i32(place.x + 0.5), city_cell(place.y, place.z)), 0);
            kind = u32(t.r * 255.0 + 0.5);
            district = u32(t.g * 255.0 + 0.5);
        }
        var fi: FacadeIn;
        fi.surface = surface;
        fi.seed = seed;
        fi.tall = tall;
        fi.ao = ao;
        fi.uv = uv;
        fi.duv_dx = duv_dx;
        fi.duv_dy = duv_dy;
        fi.p = p;
        fi.dp_dx = dp_dx;
        fi.dp_dy = dp_dy;
        fi.n = normalize(pbr.N);
        fi.v = pbr.V;
        fi.district = district;
        fi.kind = kind;
        fi.strip = u32(max(place.x, 0.0) + 0.5);
        fi.lamps = lamps;
        fi.daylight = city.day.x;
        fi.seconds = city.day.z;
        fi.frag = pbr.frag_coord.xy;
        fi.burn = 0.0;
        if (surface == 20u) {
            // A lantern (its uv: where it hangs, over its pool): as bright as its lamp burns in the
            // ground's paint, from the paint's own rows (`lantern_burn`), so a lamp that's out over a
            // dark pool is out here too; a plaza's always burn. The Low tier paints its rows' mean
            // light (no pools), and its lanterns burn the mean.
#ifdef CITY_DETAIL
            let lt = textureLoad(atlas, atlas_texel(i32(kk), ground_cell), 0);
            let row = lantern_burn(ground_cell, streets_at(ground_cell), i32(lt.r * 255.0 + 0.5), lt.a * 255.0);
#else
            let row = LAMP_MEAN;
#endif
            fi.burn = select(row, PLAZA_GAIN, u32(tall * 255.0 + 0.5) == 1u);
        }
        let fo = facade(fi);
        if (fo.cut > 0.5) {
            discard;
        }
        albedo = fo.albedo;
        rough = fo.rough;
        metal = fo.metal;
        glow = fo.emissive;
        pbr.N = fo.n;
        facade_refl = fo.reflectance;
        ao *= fo.occlusion;
        if (surface == 7u) {
            // Leaves let light through: the sky's all round, and the sun's from behind a crown, so a
            // tree in its own shade or against the light reads as green, not as a black cut-out.
            let behind = max(dot(-normalize(pbr.N), key_dir(kk, city.sky)), 0.0);
            glow += albedo * (city.sky.ambient.rgb * 0.5 + city.sky.key.rgb * (0.08 * behind / PI)) * ao;
        }
        if (facade_refl > 0.0) {
            // The panes mirror the sky function: the strips overhead, the windows' glow, the haze.
            let n = normalize(pbr.N);
            glow += sky_reflection(p, v, n, 0.04, city.sky) * facade_refl * ao;
            glint = sun_glint(reflect(-v, n), key_dir(kk, city.sky), 400.0, city.sky) * fresnel(dot(v, n), 0.04) * facade_refl * ao;
        }
    } else if (surface == 3u) {
        // Pavement slabs.
        let j = min(fract(uv.x / 1.5), fract(uv.y / 1.5));
        albedo = vec3(0.5, 0.5, 0.49) * (0.9 + 0.1 * hash13(vec3(floor(uv / 1.5), 1.0))) * mix(0.8, 1.0, step(0.04, j));
        rough = 0.9;
    } else if (surface == 4u) {
        albedo = vec3(0.6, 0.6, 0.58);
    } else if (surface == 5u) {
        // The canal's water: dark and smooth, mirroring the sky function (the strips overhead, the
        // windows' glow, the haze), the lamps by night, more of it the more glancing the view.
        albedo = vec3(0.03, 0.06, 0.07);
        rough = 0.05;
        let ripple = noise3(vec3(uv * 0.15, city.day.z * 0.3)) - 0.5;
        pbr.N = normalize(pbr.N + vec3(ripple * 0.06, ripple * 0.04, ripple * 0.05));
        let n = normalize(pbr.N);
        let f = fresnel(dot(v, n), 0.02);
        glow = sky_reflection(p, v, n, 0.02, city.sky) + warm * lamps * f * 30.0;
        glint = sun_glint(reflect(-v, n), key_dir(kk, city.sky), 60.0, city.sky) * f;
    } else if (surface == 6u) {
        albedo = vec3(0.42, 0.4, 0.36) * (0.85 + 0.2 * noise3(vec3(uv * 0.5, 2.0)));
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
        // place is: 1 the bar, 2 the Exchange, 3 the Charter Board, 4 the Blast Hall.
        indoor = true;
        let kind = u32(seed + 0.5);
        let bar = kind == 1u;
        let blast = kind == 4u;
        if (surface == 14u && blast) {
            // The Blast Hall's walls: cast concrete in 6 m panels, scorched low down, over a band of
            // yellow and black chevrons a suit's knee high.
            let seam = min(fract(uv.x / 6.0), fract(uv.y / 6.0));
            let panel = hash13(vec3(floor(uv / 6.0), 12.0));
            let concrete = vec3(0.5, 0.5, 0.48) * (0.85 + 0.15 * panel) * (0.9 + 0.12 * noise3(vec3(uv * 0.35, 5.0)));
            let scorch = (1.0 - smoothstep(2.0, 14.0, uv.y)) * (0.35 + 0.4 * noise3(vec3(uv * 0.08, 7.0)));
            albedo = concrete * mix(0.7, 1.0, step(0.015, seam)) * (1.0 - 0.6 * scorch);
            let chevron = step(0.5, fract((uv.x + uv.y) / 1.6));
            let band = step(6.3, uv.y) * step(uv.y, 7.5);
            albedo = mix(albedo, mix(vec3(0.04), vec3(0.95, 0.72, 0.05), chevron), band);
            rough = 0.9;
        } else if (surface == 14u && kind == 2u) {
            // The Exchange's walls: dark lacquered panels between pilasters every 6 m, each lit up its
            // middle; a ticker crawling round the hall between 4.2 and 5 m, prices up in green and
            // down in red; neon trim along the skirting, round the ticker and under the ceiling.
            let bay = fract(uv.x / 6.0);
            let pilaster = step(bay, 0.12);
            let seam = step(0.99, fract(uv.y / 2.4));
            albedo = mix(vec3(0.04, 0.045, 0.07), vec3(0.085, 0.095, 0.13), pilaster) * (1.0 - 0.35 * seam);
            rough = 0.3;
            metal = 0.15;
            glow = EX_CYAN * step(abs(bay - 0.06), 0.008) * step(0.6, uv.y) * step(uv.y, 11.2) * 1100.0;
            let band = step(4.2, uv.y) * step(uv.y, 5.0);
            let crawl = uv.x + city.day.z * 2.5;
            let gx = fract(crawl / 0.32);
            let gy = (uv.y - 4.2) / 0.8;
            let word = floor(crawl / 5.0);
            let mark = step(0.18, gx) * step(gx, 0.82) * step(0.2, gy) * step(gy, 0.8)
                * step(0.28, hash13(vec3(floor(crawl / 0.32), floor(gy * 3.0), 4.0))) * step(fract(crawl / 5.0), 0.78);
            let tint = select(EX_DOWN, EX_UP, hash13(vec3(word, 1.0, 7.0)) > 0.42);
            albedo = mix(albedo, vec3(0.01), band);
            glow = mix(glow, tint * mark * 800.0, band);
            let trim = step(abs(uv.y - 0.35), 0.03) + step(abs(uv.y - 11.45), 0.05)
                + step(abs(uv.y - 4.12), 0.03) + step(abs(uv.y - 5.08), 0.03);
            glow += EX_PINK * min(trim, 1.0) * 900.0;
        } else if (surface == 14u) {
            // Plaster over a dark dado (walls' UVs are metres along, and up from the street).
            let dado = step(uv.y, 1.3);
            let plaster = select(vec3(0.74, 0.72, 0.68), vec3(0.6, 0.45, 0.32), bar);
            albedo = mix(plaster, vec3(0.2, 0.15, 0.11), dado) * (0.94 + 0.08 * noise3(vec3(uv * 0.7, 4.0)));
            rough = 0.8;
        } else if (surface == 15u && blast) {
            // Its floor: slabs scarred by thrusters, ruled in yellow every 10 m for a suit to stand
            // on its marks.
            let slab = hash13(vec3(floor(uv / 5.0), 14.0));
            let joint = min(fract(uv.x / 5.0), fract(uv.y / 5.0));
            albedo = vec3(0.4, 0.4, 0.39) * (0.8 + 0.2 * slab) * mix(0.75, 1.0, step(0.01, joint));
            albedo *= 1.0 - 0.45 * smoothstep(0.55, 0.9, noise3(vec3(uv * 0.05, 3.0)));
            let g = abs(fract(uv / 10.0 + vec2(0.5)) - vec2(0.5)) * 10.0;
            let line = step(min(g.x, g.y), 0.15);
            albedo = mix(albedo, vec3(0.9, 0.7, 0.1), line * 0.85);
            rough = 0.85;
        } else if (surface == 15u) {
            // Polished stone tiles; the bar's boards; the Exchange's dark glass tiles, lanes lit
            // through them every 8 m.
            if (kind == 2u) {
                let j = min(fract(uv.x / 2.0), fract(uv.y / 2.0));
                let tile = hash13(vec3(floor(uv / 2.0), 8.0));
                albedo = vec3(0.05, 0.058, 0.085) * (0.85 + 0.3 * tile) * mix(0.55, 1.0, step(0.015, j));
                rough = 0.12;
                metal = 0.25;
                let g = abs(fract(uv / 8.0 + vec2(0.5)) - vec2(0.5)) * 8.0;
                glow = EX_CYAN * step(min(g.x, g.y), 0.04) * 160.0;
            } else if (bar) {
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
        } else if (surface == 16u && blast) {
            // Its roof, 60 m up: dark steel trusses and big floodlights between them.
            let g = abs(fract(uv / 12.0) - vec2(0.5));
            let flood = step(max(g.x, g.y), 0.12);
            let truss = step(0.47, max(g.x, g.y));
            albedo = mix(vec3(0.25, 0.26, 0.28), vec3(0.12, 0.12, 0.13), truss);
            glow = vec3(0.95, 0.97, 1.0) * flood * 3600.0;
            metal = 0.5;
        } else if (surface == 16u && kind == 2u) {
            // The Exchange's ceiling: dark, long light strips across it every 4 m, pink lines along.
            albedo = vec3(0.05, 0.055, 0.08);
            glow = vec3(0.85, 0.95, 1.0) * step(abs(fract(uv.x / 4.0) - 0.5), 0.035) * 2400.0;
            glow += EX_PINK * step(abs(fract(uv.y / 12.0) - 0.5), 0.008) * 900.0;
        } else if (surface == 16u) {
            // The ceiling and its lamps: panels in a grid (warm in the bar).
            let g = abs(fract(uv / 5.0) - vec2(0.5));
            let panel = step(max(g.x, g.y), 0.14);
            albedo = vec3(0.82, 0.8, 0.77);
            glow = select(vec3(1.0, 0.96, 0.9), warm, bar) * panel * 2600.0;
        } else if (surface == 17u) {
            // The counter: a steel desk; the bar's polished wood. It's a terminal: a row of lit
            // screens across its top (the place's colour: the Board's amber, the hall's cyan, the
            // Exchange's green, the bar's warm), and a light strip along its front edge, so it
            // reads as the thing E uses from across the room.
            albedo = select(vec3(0.2, 0.22, 0.24), vec3(0.34, 0.18, 0.08), bar);
            rough = 0.3;
            metal = select(0.6, 0.0, bar);
            let tint = select(select(select(vec3(1.0, 0.72, 0.3), vec3(0.35, 1.0, 0.5), kind == 2u), vec3(1.0, 0.7, 0.2), kind == 3u), vec3(0.35, 0.95, 1.0), blast);
            let top = dot(normalize(in.world_normal), strip_up(kk)) > 0.7;
            if (top) {
                // Screens a metre and a half apart, each 1.1 m wide, scan lines rolling down them.
                let along = fract(uv.x / 1.5);
                let screen = step(0.12, along) * step(along, 0.85);
                let lines = 0.75 + 0.25 * step(0.5, fract(uv.y * 6.0 - city.day.z * 0.8));
                albedo = mix(albedo, vec3(0.02, 0.03, 0.04), screen);
                glow = tint * screen * lines * 220.0;
            } else {
                // The light strip under the top's edge (sides' UVs: metres along, and up).
                let strip = step(abs(uv.y - 1.18), 0.03);
                glow = tint * strip * 600.0;
            }
        } else {
            // The back wall: what the place is.
            if (blast) {
                // The Proving Ground's board: the course drawn in light on a dark screen from a
                // person's knee to 38 m up, its rings strung along its line with a pulse running down
                // them; at a person's eye, a row of lamps over the desk with a light running along
                // it; yellow and black chevrons at its foot.
                let screen = step(1.6, uv.y) * step(uv.y, 38.0);
                let q = vec2(fract(uv.x / 84.0) * 84.0, uv.y - 16.0);
                let path = 9.0 * sin(q.x * 0.11) + 3.0 * sin(q.x * 0.37);
                let rule = step(abs(fract(uv.x / 2.0) - 0.5), 0.02) + step(abs(fract(uv.y / 2.0) - 0.5), 0.02);
                let along = step(abs(q.y - path), 0.12);
                let c = vec2(floor(q.x / 6.5) * 6.5 + 3.25, 0.0);
                let ring = step(abs(length(vec2(q.x - c.x, q.y - (9.0 * sin(c.x * 0.11) + 3.0 * sin(c.x * 0.37)))) - 1.6), 0.18);
                let pulse = step(fract(q.x / 84.0 - city.day.z * 0.05), 0.08);
                let cell = fract(uv.x / 1.2);
                let lamp = step(2.2, uv.y) * step(uv.y, 2.9) * step(0.15, cell) * step(cell, 0.85);
                let run = step(fract(floor(uv.x / 1.2) / 13.0 - city.day.z * 0.4), 1.0 / 13.0);
                albedo = mix(vec3(0.42, 0.42, 0.4), vec3(0.03, 0.04, 0.05), screen);
                glow = vec3(0.3, 0.9, 1.0) * screen * (min(rule, 1.0) * 25.0 + along * 400.0 + ring * (700.0 + pulse * 900.0));
                glow += vec3(1.0, 0.7, 0.2) * lamp * (80.0 + run * 900.0);
                let chevron = step(0.5, fract((uv.x + uv.y) / 1.6));
                albedo = mix(albedo, mix(vec3(0.04), vec3(0.95, 0.72, 0.05), chevron), step(uv.y, 1.4));
            } else if (kind == 2u) {
                // The Exchange's boards: framed panels 12 m wide, a chart and a board of prices in
                // turn, each in a cyan frame, under a pink header band whose blocks run like a
                // marquee.
                let k = floor(uv.x / 12.0);
                let fx = fract(uv.x / 12.0);
                let framed = step(0.035, fx) * step(fx, 0.965) * step(2.6, uv.y) * step(uv.y, 9.3);
                let core = step(0.045, fx) * step(fx, 0.955) * step(2.7, uv.y) * step(uv.y, 9.2);
                albedo = vec3(0.02, 0.025, 0.04);
                glow = EX_CYAN * (framed - core) * 1100.0;
                if (fract(k * 0.5) < 0.25) {
                    glow += core * ex_chart((fx - 0.045) / 0.91, uv.y, 3.0, 8.8, k, city.day.z);
                } else {
                    glow += core * ex_rows(uv.x, uv.y - 2.7, city.day.z);
                }
                let header = step(9.7, uv.y) * step(uv.y, 11.0);
                let block = step(0.35, hash13(vec3(floor((uv.x + city.day.z * 1.5) / 0.55), floor((uv.y - 9.7) / 0.43), 11.0)));
                glow += EX_PINK * header * (0.25 + 0.75 * block) * 750.0;
            } else if (kind == 5u) {
                // A screen hung over the Exchange floor: a chart in a cyan frame, the uv its own
                // metres along, and up from the street.
                let lo = 6.4;
                let hi = 9.6;
                let fx = fract(uv.x / 12.0);
                let frame = 1.0 - step(0.012, fx) * step(fx, 0.988) * step(lo + 0.12, uv.y) * step(uv.y, hi - 0.12);
                albedo = vec3(0.015, 0.02, 0.035);
                glow = EX_CYAN * frame * 1200.0 + (1.0 - frame) * ex_chart(fx, uv.y, lo + 0.4, hi - 0.4, floor(uv.x / 12.0) + 3.0, city.day.z);
            } else if (kind == 6u) {
                // Its back: dark, a pink line along its foot.
                albedo = vec3(0.03, 0.035, 0.05);
                glow = EX_PINK * step(uv.y, 6.5) * 600.0;
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

    if (surface >= 27u && surface <= 31u) {
        // Inside a home's building (`city_mesh::Surface::HomeWall` to `HomeDoor`), lit as a room is.
        // Walls' and doors' UVs are metres along and up from the street; floors' and ceilings',
        // where they are.
        indoor = true;
        if (surface == 27u) {
            // Painted plaster over a skirting board, along the foot of each storey's walls.
            let up = uv.y - GROUND_FLOOR;
            let above = select(uv.y - KERB, up - floor(up / FLOOR) * FLOOR, up >= 0.0);
            let skirting = step(above, 0.12);
            albedo = vec3(0.8, 0.78, 0.73) * (0.95 + 0.06 * noise3(vec3(uv * 0.9, 21.0)));
            albedo = mix(albedo, vec3(0.32, 0.27, 0.22), skirting);
            rough = 0.85;
        } else if (surface == 28u) {
            // Oak boards, a plank's width across the strip.
            let plank = floor(uv.x / 0.18);
            let joint = step(fract(uv.x / 0.18), 0.05) + step(fract(uv.y / 2.4 + hash13(vec3(plank, 0.0, 22.0))), 0.01);
            albedo = vec3(0.46, 0.33, 0.21) * (0.82 + 0.3 * hash13(vec3(plank, floor(uv.y / 2.4), 23.0)));
            albedo *= mix(1.0, 0.7, min(joint, 1.0));
            rough = 0.5;
        } else if (surface == 29u) {
            // The ceiling: a round lamp every 3 m.
            let g = fract(uv / 3.0) - vec2(0.5);
            let lamp = step(length(g), 0.06);
            albedo = vec3(0.86, 0.85, 0.82);
            glow = vec3(1.0, 0.93, 0.82) * lamp * 1800.0;
        } else if (surface == 30u) {
            // The stairs: terrazzo, a nosing at each tread's edge.
            let chip = step(0.82, hash13(vec3(floor(uv * 40.0), 24.0)));
            albedo = mix(vec3(0.55, 0.54, 0.51), vec3(0.3, 0.3, 0.32), chip);
            rough = 0.45;
        } else {
            // A lift's doors: brushed steel, split down the middle, a lit button beside them.
            let mid = step(abs(fract(uv.x / 1.2) - 0.5), 0.01);
            albedo = vec3(0.55, 0.57, 0.6) * (1.0 - 0.6 * mid);
            metal = 0.8;
            rough = 0.35;
        }
    }

    if (surface == 32u) {
        // A home's furniture (`city_mesh::Surface::Furniture`), lit as the room it stands in: its
        // seed its finish (`city_mesh::FINISHES`).
        indoor = true;
        let finish = u32(seed + 0.5);
        let grain = 0.9 + 0.12 * noise3(vec3(uv * vec2(0.6, 6.0), 31.0));
        if (finish == 0u) {
            albedo = vec3(0.42, 0.28, 0.16) * grain;
            rough = 0.55;
        } else if (finish == 1u) {
            albedo = vec3(0.32, 0.38, 0.46) * (0.92 + 0.1 * noise3(vec3(uv * 9.0, 32.0)));
            rough = 0.95;
        } else if (finish == 2u) {
            albedo = vec3(0.5, 0.52, 0.55);
            metal = 0.8;
            rough = 0.35;
        } else if (finish == 3u) {
            albedo = vec3(0.86, 0.85, 0.82);
            rough = 0.5;
        } else if (finish == 4u) {
            albedo = vec3(0.08, 0.08, 0.09);
            rough = 0.4;
        } else if (finish == 5u) {
            albedo = vec3(0.16, 0.36, 0.14) * (0.7 + 0.5 * noise3(vec3(uv * 4.0, 33.0)));
            rough = 0.9;
        } else if (finish == 6u) {
            albedo = vec3(0.9, 0.88, 0.84);
            rough = 0.9;
        } else {
            // A screen: the colony's news, scrolling.
            albedo = vec3(0.02, 0.03, 0.04);
            let line = step(0.5, fract(uv.y * 8.0 - city.day.z * 0.3));
            glow = vec3(0.35, 0.75, 1.0) * (0.4 + 0.6 * line) * 120.0;
        }
        // Being put down (build mode, `city_mesh::furniture_ghost`): its `tall` says whether it fits
        // there (1, green) or not (2, red).
        if (tall > 0.5) {
            let tint = select(vec3(0.95, 0.2, 0.15), vec3(0.3, 0.95, 0.4), tall < 1.5);
            albedo = mix(albedo, tint, 0.6);
            glow = glow + tint * 25.0;
        }
    }

    let n = normalize(pbr.N);
    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = rough;
    pbr.material.metallic = metal;
    // Bevy's ambient is even all round, so on the scene's strip the ground's bounce on walls and
    // undersides comes in here (the other strips have it in `own_light`), occluded as the ambient is
    // (`diffuse_occlusion`: the screen-space ambient occlusion where it runs, else 1).
    let bounce = albedo * bounce_light(n, kk, city.sky) * ao * pbr.diffuse_occlusion * (1.0 - metal);
    pbr.material.emissive = vec4(glow + select(vec3(0.0), bounce, scene), 1.0);
    pbr.diffuse_occlusion *= ao;
    pbr.specular_occlusion *= ao;

    // On the camera's strip, the scene's sun and shadows; elsewhere, that strip's own sun, sky and
    // bounce, in the hour's colours. From inside a room the scene's light is the room's, so the
    // street through its door is lit by its own as the other strips are (and blazes, at the room's
    // exposure). A room's own surfaces are lit by its lamps.
    // The long shadows: on the scene's strip, past where the cascades reach (Bevy's shadow there is
    // exactly none), or all of it when there are no cascades. (Not the other strips, kilometres off
    // in the haze overhead: there it would cost the most, over most of the picture, and show least.)
    var shade = 1.0;
#ifdef CITY_DETAIL
    let key = key_dir(kk, city.sky);
    if (scene && k >= 0.0 && !indoor && city.sky.ambient.w > 0.0 && dot(n, key) > 0.0) {
        var reach = 0.0;
        if (lights.n_directional_lights > 0u
                && (lights.directional_lights[0u].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
            reach = lights.directional_lights[0u].cascades[lights.directional_lights[0u].num_cascades - 1u].far_bound;
        }
        let vz = view.view_from_world;
        let depth = -dot(vec4(vz[0].z, vz[1].z, vz[2].z, vz[3].z), in.world_position);
        if (depth >= reach) {
            shade = long_shade(p, n, key, i32(kk + 0.5));
        }
    }
#endif
    var out: FragmentOutput;
    var lit = apply_pbr_lighting(pbr);
#ifdef CITY_DETAIL
    if (scene && shade < 1.0) {
        lit = vec4(max(lit.rgb - scene_sun(pbr) * ((1.0 - shade) * view.exposure), vec3(0.0)), lit.a);
    }
#endif
    let own = vec4((own_light(albedo, n, kk, city.sky) * ao + glow + glint) * view.exposure, 1.0);
    out.color = select(own, lit, scene);
    if (indoor) {
        out.color = vec4((albedo * city.light.w * ao + glow) * view.exposure, 1.0);
    } else {
        // The air between the eye and here (`bc::colony_sky`): blue and soft along the street,
        // clear across the core.
        let air = haze(view.world_position + city.origin.xyz, p, city.sky);
        out.color = vec4(out.color.rgb * air.transmittance + air.inscatter * view.exposure, out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
