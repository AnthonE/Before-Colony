// ZERO's view, on the HDR picture before tonemapping: a faint echo of the picture along a slow
// drift, as if seeing a split second ahead; a magenta tint and glow at the edges that deepens with
// ZERO's strain, and a slow scan line; in a seizure, a torn echo and a flicker.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;

struct ZeroVision {
    engaged: f32,
    strain: f32,
    seizure: f32,
    time: f32,
};

@group(0) @binding(2) var<uniform> zero: ZeroVision;

fn hash(x: f32) -> f32 {
    return fract(sin(x * 12.9898) * 43758.547);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = zero.time;
    let on = zero.engaged;
    let seizure = zero.seizure;
    // The echo drifts slowly, further as the strain builds; in a seizure, bands of it tear sideways.
    let drift = vec2(sin(t * 0.7), cos(t * 0.53)) * (0.003 + 0.01 * zero.strain);
    let band = step(0.55, hash(floor(uv.y * 24.0) + floor(t * 11.0)));
    let tear = seizure * band * 0.025 * sin(t * 31.0 + uv.y * 17.0);
    let here = textureSample(screen, screen_sampler, uv).rgb;
    let echo = textureSample(screen, screen_sampler, uv + drift + vec2(tear, 0.0)).rgb;
    var c = mix(here, max(here, echo), 0.4 * on + 0.5 * seizure);
    // Magenta at the edges: a tint on what's there, and a glow over black space.
    let d = uv - 0.5;
    let edge = smoothstep(0.05, 0.4, dot(d, d));
    let tint = edge * (on * (0.3 + 0.4 * zero.strain) + 0.5 * seizure);
    c = mix(c, c * vec3(1.3, 0.45, 1.0), tint);
    c += vec3(0.35, 0.02, 0.22) * tint * 0.25;
    // A faint line scanning down the view.
    let scan = exp(-pow((uv.y - fract(t * 0.35)) * 40.0, 2.0));
    c += vec3(0.4, 0.08, 0.35) * scan * 0.05 * on;
    // A seizure flickers.
    let flicker = 1.0 - seizure * 0.3 * step(0.6, hash(floor(t * 17.0)));
    return vec4(c * flicker, 1.0);
}
