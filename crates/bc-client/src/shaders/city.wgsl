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
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import bc::noise::{hash13, noise3}
#ifdef CITY_DETAIL
#import bc::city::{city_cell, atlas_texel, city_paint, lantern_burn, streets_at, PLAZA_GAIN, CANAL_WIDTH}
#else
#import bc::city::{city_cell, atlas_texel, city_sketch, LAMP_MEAN, PLAZA_GAIN, CANAL_WIDTH}
#endif
#import bc::facade::{facade, FacadeIn, city_place}
#import bc::colony_sky::{Sky, haze, strip_at, strip_up, key_dir, own_light, bounce_light, fresnel, sky_reflection, sun_glint}

struct City {
    // xyz: where the render origin is in the colony's frame; w: the camera's strip.
    origin: vec4<f32>,
    // x: daylight 0..1; y: lamps lit 0..1; z: seconds; w: the sun's elevation (rad).
    day: vec4<f32>,
    // x: the key light (lux) on the other strips; y: the sky's light (nits); z: 1 while the camera
    // is in a key place's room (the scene's light is then the room's); w: unused.
    light: vec4<f32>,
    // The hour's air and light (`city.rs`'s colour script).
    sky: Sky,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> city: City;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var atlas: texture_2d<f32>;

const TAU: f32 = 6.2831853;
const PI: f32 = 3.14159265;
const RADIUS: f32 = 3200.0;
// A key place's room is lit by its own lamps (nits on a white wall): exposed at EV 8 inside, it's
// dim seen from the sunlit street through its door, as rooms are.
const INDOOR: f32 = 300.0;
// The ground's lamps by night: nits at a pool's peak a unit of albedo (`bc::city`'s `Paint::lamps`
// is relative to it), for night's EV 8.5 (`city_hour::EV_NIGHT`; scale by 2^(EV - 8.5) if that
// moves): pools under the lamps, streets that read as lines from the lift without outshining the
// windows. The colony's light strips inlaid in its ground (`Paint::glow`), nits.
const GROUND_LAMPS: f32 = 420.0;
const GROUND_STRIPS: f32 = 1800.0;

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

    let n = normalize(pbr.N);
    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = rough;
    pbr.material.metallic = metal;
    // Bevy's ambient is even all round, so on the scene's strip the ground's bounce on walls and
    // undersides comes in here (the other strips have it in `own_light`).
    let bounce = albedo * bounce_light(n, kk, city.sky) * ao * (1.0 - metal);
    pbr.material.emissive = vec4(glow + select(vec3(0.0), bounce, scene), 1.0);
    pbr.diffuse_occlusion *= ao;
    pbr.specular_occlusion *= ao;

    // On the camera's strip, the scene's sun and shadows; elsewhere, that strip's own sun, sky and
    // bounce, in the hour's colours. From inside a room the scene's light is the room's, so the
    // street through its door is lit by its own as the other strips are (and blazes, at the room's
    // exposure). A room's own surfaces are lit by its lamps.
    var out: FragmentOutput;
    let lit = apply_pbr_lighting(pbr);
    let own = vec4((own_light(albedo, n, kk, city.sky) * ao + glow + glint) * view.exposure, 1.0);
    out.color = select(own, lit, scene);
    if (indoor) {
        out.color = vec4((albedo * INDOOR * ao + glow) * view.exposure, 1.0);
    } else {
        // The air between the eye and here (`bc::colony_sky`): blue and soft along the street,
        // clear across the core.
        let air = haze(view.world_position + city.origin.xyz, p, city.sky);
        out.color = vec4(out.color.rgb * air.transmittance + air.inscatter * view.exposure, out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
