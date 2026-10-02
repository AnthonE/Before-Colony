// The chart's holograms (`holo.rs`): bodies drawn as light, not matter. Each is a translucent
// shell, brightest at its rim where the view grazes it, with scan bands across it, added over
// whatever is behind (no depth written, so the chart's lines show through).
//
// Kinds (params.x):
//   0  plain: a landmark, the docking hub.
//   1  the colony: its three windows bright along their length, its land strips ruled across,
//      turning with the colony (uv.x runs round it, uv.y along it).
//   2  Earth: continents from the sky's own noise (`sky.wgsl`), ruled in contour lines, the day
//      side lit by the Sun and the night side pricked with cities, an atmosphere at the limb.
//   3  the Moon: its maria, the night side dark.
// The colour is raw HDR (it blooms where the tier has bloom, and is tonemapped here where not).

#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}
#import bc::glow::glow_out

struct Holo {
    // rgb: the fill's colour (raw HDR); a: how much of it fills the shell.
    color: vec4<f32>,
    // rgb: the rim's colour; a: how tight the rim is (a power).
    rim: vec4<f32>,
    // x: kind; y: seconds; z: brightness (fades with the chart's scale); w: scan band spacing (m).
    params: vec4<f32>,
    // xyz: direction to the Sun; w: the first window's angle (the colony).
    sun: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> holo: Holo;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.2831853;

fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
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

fn fbm(p: vec3<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        sum += amp * noise3(q);
        q = q * 2.03 + vec3(1.7, 9.2, 3.1);
        amp *= 0.5;
    }
    return sum / (1.0 - amp * 2.0);
}

// A thin line where `x` crosses a whole number, `w` of a step wide, kept a pixel or so on screen.
fn rule(x: f32, w: f32) -> f32 {
    let d = abs(fract(x + 0.5) - 0.5);
    let px = max(fwidth(x), 1e-5);
    return 1.0 - smoothstep(w * 0.5, w * 0.5 + px * 1.5, d);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let kind = i32(round(holo.params.x));
    let t = holo.params.y;
    let n = normalize(in.world_normal) * select(-1.0, 1.0, front);
    let v = normalize(view.world_position - in.world_position.xyz);
    let facing = clamp(abs(dot(n, v)), 0.0, 1.0);
    let rim = pow(1.0 - facing, holo.rim.w);
    var col = holo.color.rgb * holo.color.a * (0.35 + 0.65 * facing) + holo.rim.rgb * rim;
    let p = in.world_position.xyz;
    let sun = holo.sun.xyz;

    if (kind == 0) {
        // Scan bands across it, drifting up.
        let bands = rule(p.y / holo.params.w - t * 0.15, 0.12);
        col += holo.rim.rgb * bands * 0.35;
        // The lit side a touch brighter.
        col *= 0.75 + 0.35 * max(dot(n, sun), 0.0);
    } else if (kind == 1) {
        let around = in.uv.x * TAU;
        let along = in.uv.y;
        // Windows: a third of the way round each, centred on its angle.
        let k = (around - holo.sun.w) / (TAU / 3.0);
        let off = abs(fract(k + 0.5) - 0.5);
        let window = 1.0 - smoothstep(0.155, 0.17, off);
        // Ribs round the hull every kilometre, and the strips ruled along their length.
        let ribs = rule(along * 32.0, 0.06);
        let ruled = rule(k * 6.0, 0.05) * (1.0 - window);
        col += holo.rim.rgb * (ribs * 0.5 + ruled * 0.3);
        // The windows glow with the city behind them.
        let city = 0.5 + 0.5 * noise3(vec3(along * 400.0, k * 30.0, 0.0));
        col += vec3(0.9, 0.85, 0.55) * window * (0.25 + 0.35 * city) * holo.color.a;
    } else if (kind == 2) {
        let ndl = dot(n, sun);
        let day = smoothstep(-0.12, 0.18, ndl);
        let h = fbm(n * 2.3 + vec3(4.0, 1.0, 7.0));
        let land = smoothstep(0.52, 0.56, h);
        // Oceans dim, the land bright, ruled in contours; the night side darker, with cities.
        let contours = rule(h * 22.0, 0.08) * land;
        let coast = rule((h - 0.54) * 1.0, 0.004);
        var body = mix(vec3(0.02, 0.09, 0.22), vec3(0.08, 0.42, 0.55), land);
        body += vec3(0.2, 0.75, 0.9) * (contours * 0.45 + coast * 0.8);
        body *= 0.25 + 0.95 * day;
        let cities = smoothstep(0.66, 0.82, fbm(n * 30.0)) * land * (1.0 - day);
        body += vec3(1.0, 0.62, 0.28) * cities * 1.4;
        // Lines of latitude and longitude, faint.
        let lat = asin(clamp(n.y, -1.0, 1.0)) / PI * 12.0;
        let lon = atan2(n.z, n.x) / PI * 12.0;
        body += vec3(0.25, 0.6, 0.8) * (rule(lat, 0.04) + rule(lon, 0.04)) * 0.25;
        // The atmosphere at the limb, lit where the Sun is.
        let haze = vec3(0.35, 0.65, 1.0) * rim * (0.35 + 0.9 * smoothstep(-0.3, 0.3, ndl));
        col = body * holo.color.a * 2.0 + haze * 1.6 + holo.rim.rgb * rim * 0.4;
    } else if (kind == 3) {
        let ndl = dot(n, sun);
        let day = smoothstep(-0.1, 0.15, ndl);
        let maria = smoothstep(0.5, 0.62, fbm(n * 1.7 + 2.0));
        let grit = fbm(n * 9.0);
        var body = mix(vec3(0.45, 0.5, 0.58), vec3(0.18, 0.22, 0.3), maria) * (0.7 + 0.5 * grit);
        body += vec3(0.4, 0.7, 0.9) * rule(grit * 9.0, 0.05) * 0.25;
        body *= 0.08 + 0.92 * day;
        col = body * holo.color.a * 1.4 + holo.rim.rgb * rim * 0.6;
    }
    col *= holo.params.z;
    return vec4(glow_out(col), 0.0);
}
