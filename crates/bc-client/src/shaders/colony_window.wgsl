// The colony's window strips. Each window pixel casts its view ray into the (analytic) cylinder
// and draws what it meets on the far side: farmland, rivers, towns and roads on the land strips,
// clouds a kilometre up, and haze over six kilometres of air. Towns light up at night. The glass
// adds a sun glint on top.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
}
#import bc::noise::{hash13, noise3, fbm}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Colony {
    // xyz: centre; w: spin angle (radians, about +X).
    centre: vec4<f32>,
    // x: hull radius, y: half-length, z: first window's centre angle, w: daylight 0..1.
    shape: vec4<f32>,
    // xyz: direction to the Sun; w: sunlight scale (eclipse).
    sun: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> colony: Colony;

const WHITE: f32 = 31830.99;
const SCREEN: f32 = 27800.0;
const TAU: f32 = 6.2831853;

// World → colony space (undo the spin about X).
fn to_local(v: vec3<f32>) -> vec3<f32> {
    let c = cos(-colony.centre.w);
    let s = sin(-colony.centre.w);
    return vec3(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
}

// Angle around the axis, relative to the first window's centre, in [0, TAU).
fn sector_angle(p: vec3<f32>) -> f32 {
    let a = atan2(p.z, p.y) - colony.shape.z;
    return a - floor(a / TAU) * TAU;
}

// Whether a point on the wall is window (true) or land.
fn is_window(p: vec3<f32>) -> bool {
    let a = sector_angle(p) + TAU / 12.0;
    return (i32(floor(a / (TAU / 6.0))) % 2) == 0;
}

struct Ground {
    albedo: vec3<f32>,
    // Where towns and roads light up at night, 0..1.
    lights: f32,
};

// The land at (x along the axis, s around the wall), in metres: fields, woods, a river, towns and
// the roads between them.
fn ground(x: f32, s: f32) -> Ground {
    let q = vec3(x, s, 0.0);
    // Fields: patches a couple of hundred metres across.
    let cell = floor(q / 220.0);
    let crop = hash13(cell + vec3(0.0, 0.0, 3.0));
    var col = mix(vec3(0.12, 0.2, 0.07), vec3(0.3, 0.27, 0.12), crop);
    col = mix(col, vec3(0.2, 0.15, 0.09), step(0.85, crop));
    col *= 0.85 + 0.3 * noise3(q / 40.0);
    // Woods.
    let wood = smoothstep(0.58, 0.64, fbm(q / 900.0 + 7.0, 3));
    col = mix(col, vec3(0.05, 0.1, 0.04), wood);
    // A river meandering along the axis in each land strip.
    let strip = s - floor(s / 3351.0) * 3351.0 - 1675.0;
    let meander = sin(x / 2300.0) * 600.0 + (fbm(vec3(x / 1800.0, 1.3, 0.0), 3) - 0.5) * 900.0;
    let river = 1.0 - smoothstep(18.0, 30.0, abs(strip - meander));
    col = mix(col, vec3(0.04, 0.07, 0.1), river);
    // Towns, and roads along and across the strip.
    let town_cell = floor(q.xy / 1600.0);
    let centre = (town_cell + 0.5) * 1600.0;
    let town = step(0.72, hash13(vec3(town_cell, 11.0))) * (1.0 - smoothstep(250.0, 420.0, length(q.xy - centre)));
    let along = 1.0 - smoothstep(4.0, 7.0, abs(strip + 900.0));
    let across = (1.0 - smoothstep(3.0, 6.0, abs(x - floor(x / 1600.0) * 1600.0 - 800.0))) * 0.6;
    let road = min(along + across, 1.0);
    col = mix(col, vec3(0.33, 0.33, 0.34), max(town, road * 0.8));
    return Ground(col, town * step(0.55, noise3(q / 12.0)) + road * 0.35);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let r_hull = colony.shape.x;
    let half = colony.shape.y;
    let day = colony.shape.w;
    let night = 1.0 - day;
    let world = in.world_position.xyz;
    let v = normalize(world - view.world_position);
    let p = to_local(world - colony.centre.xyz);
    let d = to_local(v);

    // Across the interior to the far wall: |p.yz + t d.yz| = R.
    let a = dot(d.yz, d.yz);
    let b = 2.0 * dot(p.yz, d.yz);
    let c = dot(p.yz, p.yz) - r_hull * r_hull;
    let t = (-b + sqrt(max(b * b - 4.0 * a * c, 0.0))) / (2.0 * max(a, 1e-6));
    let hit = p + d * t;

    // Sunlight thrown in by the mirrors lights the interior.
    let lit = WHITE * 0.55 * day * colony.sun.w;
    var col: vec3<f32>;
    if (abs(hit.x) > half) {
        // An end cap, seen from inside.
        col = vec3(0.2) * lit * 0.4;
    } else if (is_window(hit)) {
        // Through the far window to space.
        col = vec3(0.004, 0.006, 0.012) * WHITE;
    } else {
        let g = ground(hit.x, sector_angle(hit) * r_hull);
        col = g.albedo * lit + vec3(1.0, 0.7, 0.35) * g.lights * night * 0.05 * SCREEN;
    }
    // Clouds about a kilometre above the far wall.
    let rc = r_hull - 1000.0;
    let cc = dot(p.yz, p.yz) - rc * rc;
    let disc = b * b - 4.0 * a * cc;
    if (disc > 0.0) {
        let tc = (-b + sqrt(disc)) / (2.0 * max(a, 1e-6));
        let h = p + d * tc;
        let cloud = smoothstep(0.56, 0.74, fbm(vec3(h.x / 900.0, sector_angle(h) * rc / 900.0, 2.0), 4));
        col = mix(col, vec3(0.8) * lit + vec3(0.02) * WHITE * night, cloud * 0.85);
    }
    // Six kilometres of air.
    let haze = 1.0 - exp(-t / 9000.0);
    col = mix(col, vec3(0.45, 0.6, 0.85) * lit * 0.5, haze);
    // The glass: dimmed a little, with the Sun glinting off it.
    col *= 0.85;
    let n = normalize(in.world_normal);
    let r = reflect(v, n);
    let glint = pow(max(dot(r, colony.sun.xyz), 0.0), 900.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(-v, n), 0.0), 5.0);
    col += vec3(1.0, 0.95, 0.9) * glint * 60.0 * WHITE * colony.sun.w + vec3(0.02, 0.03, 0.05) * fresnel * WHITE;

    var out = vec4(col * view.exposure, 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
