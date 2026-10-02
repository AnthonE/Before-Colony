// The chart's backdrop (`holo.rs`): deep space behind the holograms, drawn at infinity as the
// sky is (`sky.wgsl`): faint stars, the band of the Milky Way, and the Sun where the sky has it,
// so the chart reads as a window onto the same space, dimmed to let the chart's light carry it.

#import bevy_pbr::mesh_view_bindings::view
#import bc::glow::glow_out

struct ChartSky {
    // xyz: direction to the Sun; w: seconds.
    sun: vec4<f32>,
    // xyz: the galaxy's plane normal; w: brightness.
    galaxy: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: ChartSky;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) dir: vec3<f32>,
};

@vertex
fn vertex(@location(0) position: vec3<f32>) -> Out {
    var out: Out;
    // Centred on the camera and pinned to the far plane (depth 0 with reverse-Z).
    let world = view.world_position + position * 1000.0;
    var clip = view.clip_from_world * vec4<f32>(world, 1.0);
    clip.z = 0.0;
    out.clip = clip;
    out.dir = position;
    return out;
}

fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn stars(d: vec3<f32>, cells: f32, density: f32) -> f32 {
    let g = d * cells;
    let i = floor(g);
    let h = hash13(i);
    if (h > density) {
        return 0.0;
    }
    let c = i + 0.5 + (vec3(hash13(i + 7.1), hash13(i + 3.3), hash13(i + 5.9)) - 0.5) * 0.7;
    let r = length(g - c);
    return exp(-r * r * 60.0) * (0.3 + 0.7 * hash13(i + 1.7));
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let d = normalize(in.dir);
    let b = sky.galaxy.w;
    // A navy deep, a shade lighter toward the galaxy's band.
    let band = exp(-pow(dot(d, sky.galaxy.xyz) * 5.0, 2.0));
    var col = vec3(0.004, 0.009, 0.02) + vec3(0.02, 0.03, 0.06) * band;
    col += vec3(0.7, 0.8, 1.0) * (stars(d, 90.0, 0.06) * 0.5 + stars(d, 260.0, 0.04 + band * 0.08) * 0.35) * b;
    // The Sun: a small hot disc and a wide glow.
    let s = max(dot(d, sky.sun.xyz), 0.0);
    col += vec3(1.0, 0.9, 0.7) * (pow(s, 6000.0) * 6.0 + pow(s, 60.0) * 0.08) * b;
    return vec4(glow_out(col), 1.0);
}
