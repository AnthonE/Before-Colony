// The sky of the L1 Colony Cluster, drawn at infinity behind everything: procedural stars and the
// Milky Way, the Sun, Earth (continents, oceans, clouds, city lights, atmosphere) and the Moon.
//
// Earth, the Moon and the Sun are in physical units (cd/m², lit by a 100,000 lux sun) and pass
// through the camera's exposure like every lit surface, so they sit correctly against the suits.
// Stars and the galaxy are artistic: a camera exposed for sunlight would see none.

#import bevy_pbr::mesh_view_bindings::view
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Sky {
    // xyz: direction to the body; w: its angular radius (radians).
    sun: vec4<f32>,
    earth: vec4<f32>,
    moon: vec4<f32>,
    // xyz: normal of the Milky Way's plane; w: seconds (cloud drift).
    galaxy: vec4<f32>,
    // x: star brightness; y: detail, 0 (cheapest) to 1; z: sunlight scale (colony eclipse); w: spare.
    params: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: Sky;

const PI: f32 = 3.14159265;
// Sunlight at Earth's distance, lux.
const SUN_LUX: f32 = 100000.0;
// Radiance of a white Lambertian surface in full sunlight, cd/m².
const WHITE: f32 = 31830.99;
// About 1.0 on screen at the default exposure (EV100 14.5); scales the artistic parts.
const SCREEN: f32 = 27800.0;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) dir: vec3<f32>,
};

@vertex
fn vertex(@location(0) position: vec3<f32>) -> SkyOut {
    var out: SkyOut;
    // Centred on the camera, whatever the mesh's transform, and pinned to the far plane (depth 0
    // with reverse-Z), so it is behind everything and never clipped.
    let world = view.world_position + position * 1000.0;
    var clip = view.clip_from_world * vec4<f32>(world, 1.0);
    clip.z = 0.0;
    out.clip = clip;
    out.dir = position;
    return out;
}

// --- Noise. -------------------------------------------------------------------------------------

fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn hash23(p: vec3<f32>) -> vec2<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash13(i), hash13(i + vec3(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash13(i + vec3(0.0, 1.0, 0.0)), hash13(i + vec3(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash13(i + vec3(0.0, 0.0, 1.0)), hash13(i + vec3(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash13(i + vec3(0.0, 1.0, 1.0)), hash13(i + vec3(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

fn fbm(p: vec3<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < octaves; i++) {
        sum += amp * noise3(q);
        q = q * 2.03 + vec3(1.7, 9.2, 3.1);
        amp *= 0.5;
    }
    return sum / (1.0 - amp * 2.0);
}

fn octaves(most: i32) -> i32 {
    return max(2, i32(round(mix(2.0, f32(most), sky.params.y))));
}

// --- Stars and the galaxy. ------------------------------------------------------------------------

// Face coordinates in [-1, 1]² and a face id.
fn cube_uv(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    if (a.x >= a.y && a.x >= a.z) {
        return vec3(d.y / a.x, d.z / a.x, select(1.0, 0.0, d.x > 0.0));
    }
    if (a.y >= a.z) {
        return vec3(d.x / a.y, d.z / a.y, select(3.0, 2.0, d.y > 0.0));
    }
    return vec3(d.x / a.z, d.y / a.z, select(5.0, 4.0, d.z > 0.0));
}

// Approximate colour of a star by temperature: red dwarfs, the Sun, white and blue giants.
fn star_tint(t: f32) -> vec3<f32> {
    if (t < 0.12) { return vec3(1.0, 0.62, 0.38); }
    if (t < 0.35) { return vec3(1.0, 0.86, 0.68); }
    if (t < 0.8) { return vec3(1.0, 0.98, 0.95); }
    return vec3(0.72, 0.82, 1.0);
}

// The direction of face coordinates `uv` on cube face `face` (the inverse of `cube_uv`).
fn face_dir(uv: vec2<f32>, face: f32) -> vec3<f32> {
    if (face < 0.5) { return normalize(vec3(1.0, uv.x, uv.y)); }
    if (face < 1.5) { return normalize(vec3(-1.0, uv.x, uv.y)); }
    if (face < 2.5) { return normalize(vec3(uv.x, 1.0, uv.y)); }
    if (face < 3.5) { return normalize(vec3(uv.x, -1.0, uv.y)); }
    if (face < 4.5) { return normalize(vec3(uv.x, uv.y, 1.0)); }
    return normalize(vec3(uv.x, uv.y, -1.0));
}

// One layer of stars, one candidate per cell of a `cells`² grid on each cube face. `pix` is the
// pixel's angular size, so every star is drawn about a pixel wide (anti-aliased, round, whatever
// the resolution). Brightness follows a power law, as real stars do: a handful bright, most faint.
fn star_layer(d: vec3<f32>, pix: f32, cells: f32, density: f32, seed: f32) -> vec3<f32> {
    let c = cube_uv(d);
    let grid = (c.xy * 0.5 + 0.5) * cells;
    let cell = floor(grid);
    let p = max(pix, 1e-5);
    var col = vec3(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let id = cell + vec2(f32(i), f32(j));
            let key = vec3(id, c.z * 17.0 + seed);
            if (hash13(key) > density) {
                continue;
            }
            let centre = (id + hash23(key + 5.0)) / cells * 2.0 - 1.0;
            let r = length(d - face_dir(centre, c.z)) / p;
            // Pareto-distributed brightness (N(>L) ∝ L^-1.2), on-screen units.
            let h = hash13(key + 11.0);
            let flux = min(0.002 * pow(1.0 - h * 0.9999, -1.0 / 1.2), 3.0);
            col += star_tint(hash13(key + 23.0)) * flux * exp(-r * r * 1.4);
        }
    }
    return col;
}

fn stars(d: vec3<f32>, pix: f32) -> vec3<f32> {
    // A hundred or so that catch the eye in any view, thousands more that only add texture.
    var col = star_layer(d, pix, 90.0, 0.2, 0.0);
    if (sky.params.y > 0.3) {
        col += star_layer(d, pix, 200.0, 0.25, 7.0) * 0.15;
    }
    if (sky.params.y > 0.7) {
        col += star_layer(d, pix, 420.0, 0.25, 13.0) * 0.06;
    }
    return col * sky.params.x * SCREEN;
}

fn galaxy(d: vec3<f32>) -> vec3<f32> {
    let n = sky.galaxy.xyz;
    let h = dot(d, n);
    let band = exp(-h * h * 14.0);
    if (band < 0.002) {
        return vec3(0.0);
    }
    // Brighter toward the galactic centre, a direction in the plane.
    let centre = normalize(cross(n, vec3(0.0, 0.0, 1.0)));
    let bulge = 0.35 + 0.65 * pow(max(dot(d, centre) * 0.5 + 0.5, 0.0), 3.0);
    let o = octaves(5);
    let clumps = fbm(d * 3.1, o);
    let dust = smoothstep(0.42, 0.72, fbm(d * 6.3 + 17.0, o)) * exp(-h * h * 120.0);
    let lum = band * bulge * (0.25 + 0.75 * clumps * clumps) * (1.0 - 0.8 * dust);
    let tint = mix(vec3(0.78, 0.82, 1.0), vec3(1.0, 0.86, 0.72), bulge * 0.8);
    return tint * lum * 0.05 * sky.params.x * SCREEN;
}

// --- Bodies. -------------------------------------------------------------------------------------

// Where a view ray meets a distant sphere of angular radius `ang` in direction `c`: the surface
// normal (xyz) and 1 in w, or 0 in w on a miss. The sphere is at distance 1.
fn hit_sphere(d: vec3<f32>, c: vec3<f32>, ang: f32) -> vec4<f32> {
    let r = sin(ang);
    let b = dot(d, c);
    let disc = b * b - (1.0 - r * r);
    if (disc < 0.0 || b < 0.0) {
        return vec4(0.0);
    }
    let t = b - sqrt(disc);
    return vec4((d * t - c) / r, 1.0);
}

// Earth's own axis (tilted), for latitude.
const EARTH_POLE: vec3<f32> = vec3<f32>(0.21, 0.96, 0.18);

fn earth(d: vec3<f32>, sun: vec3<f32>) -> vec4<f32> {
    let c = sky.earth.xyz;
    let ang = sky.earth.w;
    let hit = hit_sphere(d, c, ang);
    let o = octaves(6);
    if (hit.w == 0.0) {
        // Just off the limb: the atmosphere, glowing where the Sun lights it.
        let theta = acos(clamp(dot(d, c), -1.0, 1.0));
        let above = (theta - ang) / (ang * 0.022);
        if (above > 6.0) {
            return vec4(0.0);
        }
        let up = normalize(d - c * dot(d, c));
        let lit = smoothstep(-0.35, 0.25, dot(up, sun));
        let glow = exp(-above) * lit;
        let sunset = vec3(1.0, 0.45, 0.2) * smoothstep(0.3, -0.1, abs(dot(up, sun)));
        let col = (vec3(0.25, 0.5, 1.0) + sunset * 0.6) * glow * 0.9 * WHITE;
        return vec4(col, 0.0);
    }
    let n = hit.xyz;
    let v = -d;
    let ndl = dot(n, sun);
    let day = smoothstep(-0.08, 0.12, ndl);
    let lat = dot(n, EARTH_POLE);
    let drift = sky.galaxy.w;

    // Continents, climate and ice.
    let height = fbm(n * 2.3 + vec3(4.0, 1.0, 7.0), o);
    let land = smoothstep(0.52, 0.56, height);
    let dry = fbm(n * 5.1 + 3.3, max(o - 2, 2));
    let forest = vec3(0.07, 0.12, 0.05);
    let desert = vec3(0.34, 0.27, 0.16);
    var albedo = mix(vec3(0.015, 0.035, 0.09), mix(forest, desert, smoothstep(0.45, 0.62, dry)), land);
    let ice = smoothstep(0.78, 0.84, abs(lat) + 0.06 * (dry - 0.5));
    albedo = mix(albedo, vec3(0.75), ice);

    // Clouds, drifting slowly.
    let clouds_n = fbm(n * 4.2 + vec3(drift * 0.004, 0.0, drift * 0.002), o);
    let clouds = smoothstep(0.6, 0.8, clouds_n) * 0.85;
    albedo = mix(albedo, vec3(0.82), clouds);

    var col = albedo * max(ndl, 0.0) * WHITE;

    // Sun glint on open ocean.
    let h = normalize(sun + v);
    let glint = pow(max(dot(n, h), 0.0), 220.0) * (1.0 - land) * (1.0 - clouds) * step(0.0, ndl);
    col += vec3(1.0, 0.95, 0.85) * glint * 0.9 * WHITE;

    // Cities on the night side.
    let cities = smoothstep(0.62, 0.8, fbm(n * 34.0, max(o - 2, 2))) * land * (1.0 - ice) * (1.0 - clouds);
    col += vec3(1.0, 0.62, 0.28) * cities * (1.0 - day) * 0.03 * SCREEN;

    // Atmosphere: blue haze toward the limb, orange along the terminator.
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    let haze = vec3(0.3, 0.55, 1.0) * rim * smoothstep(-0.2, 0.4, ndl) * 0.25 * WHITE;
    let dusk = vec3(1.0, 0.42, 0.15) * exp(-ndl * ndl * 300.0) * 0.02 * WHITE;
    col += haze + dusk * day;
    return vec4(col, 1.0);
}

fn moon(d: vec3<f32>, sun: vec3<f32>) -> vec4<f32> {
    let hit = hit_sphere(d, sky.moon.xyz, sky.moon.w);
    if (hit.w == 0.0) {
        return vec4(0.0);
    }
    let n = hit.xyz;
    let o = octaves(5);
    let maria = smoothstep(0.5, 0.62, fbm(n * 1.7 + 2.0, 3));
    let grit = fbm(n * 9.0, o);
    let albedo = mix(0.16, 0.08, maria) * (0.8 + 0.4 * grit);
    let ndl = max(dot(n, sun), 0.0);
    // Earthshine on the night side.
    let earthshine = max(dot(n, sky.earth.xyz), 0.0) * 0.002;
    return vec4(vec3(albedo) * (ndl + earthshine) * WHITE, 1.0);
}

fn sun_disc(d: vec3<f32>, sun: vec3<f32>) -> vec3<f32> {
    let rs = sky.sun.w;
    // Accurate for small angles (unlike acos near 1).
    let ang = 2.0 * asin(clamp(length(d - sun) * 0.5, 0.0, 1.0));
    // The disc carries the whole sunlight (E = L · πr²), with limb darkening.
    let l = SUN_LUX / (PI * rs * rs);
    var col = vec3(0.0);
    if (ang < rs) {
        let mu = sqrt(1.0 - (ang / rs) * (ang / rs));
        col = vec3(1.0, 0.97, 0.92) * l * (0.35 + 0.65 * mu);
    }
    // A soft corona; bloom turns it into glare.
    let corona = l * 0.00004 * pow(rs / max(ang, rs), 3.0);
    col += vec3(1.0, 0.9, 0.75) * corona;
    return col * sky.params.z;
}

@fragment
fn fragment(in: SkyOut) -> @location(0) vec4<f32> {
    let d = normalize(in.dir);
    let sun = sky.sun.xyz;
    let pix = length(fwidth(d));
    var col = stars(d, pix) + galaxy(d);
    let m = moon(d, sun);
    col = mix(col, m.rgb, m.w);
    let e = earth(d, sun);
    col = mix(col + e.rgb * (1.0 - e.w), e.rgb, e.w);
    if (e.w == 0.0 && m.w == 0.0) {
        col += sun_disc(d, sun);
    }
    var out = vec4(col * view.exposure, 1.0);
    // Keep the half-float target finite.
    out = vec4(min(out.rgb, vec3(30000.0)), 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
