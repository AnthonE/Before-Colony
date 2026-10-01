// A cockpit monitor's face: its region of the instruments' texture (rendered by the UI each
// frame), glowing, with faint scan lines and a darker edge. A hit makes it flicker; with the head's
// main camera gone, it goes grey and static bands roll through it.

#import bevy_pbr::forward_io::VertexOutput
#import bc::glow::glow_out

struct Monitor {
    // x: seconds; y: static (0..1); z: flicker (0..1); w: brightness.
    params: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var screen: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var screen_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> monitor: Monitor;

fn hash(x: f32) -> f32 {
    return fract(sin(x * 12.9898) * 43758.547);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = vec2(0.5);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    // The face's own coordinates, for its rim.
    var face = uv;
#ifdef VERTEX_UVS_B
    face = in.uv_b;
#endif
    let t = monitor.params.x;
    let noise = monitor.params.y;
    var c = textureSample(screen, screen_sampler, uv).rgb;
    // The panel's own dark glass under what it shows.
    c = max(c, vec3(0.012, 0.02, 0.03));
    // Scan lines, and a darker rim.
    let lines = 0.9 + 0.1 * sin(in.position.y * 1.8);
    let rim = smoothstep(0.0, 0.05, min(min(face.x, 1.0 - face.x), min(face.y, 1.0 - face.y)));
    c *= lines * (0.55 + 0.45 * rim);
    // Static: grey, with bands rolling through.
    if (noise > 0.0) {
        let grey = dot(c, vec3(0.3, 0.59, 0.11));
        let band = step(0.8, hash(floor(face.y * 40.0) + floor(t * 12.0)));
        let snow = hash(face.x * 91.7 + face.y * 57.3 + floor(t * 30.0));
        c = mix(c, vec3(grey * 0.8 + snow * 0.12 + band * 0.2), noise);
    }
    c *= 1.0 - monitor.params.z * 0.5 * step(0.5, hash(floor(t * 20.0)));
    return vec4(glow_out(c * monitor.params.w), 1.0);
}
