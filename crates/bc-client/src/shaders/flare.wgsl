// The Sun's lens flare: ghosts strung along the line from the Sun through the centre of the
// screen, drawn in screen space over everything. The CPU works out how visible the Sun is.

#import bevy_pbr::mesh_view_bindings::view
#import bc::glow::glow_out

struct Flare {
    // xyz: direction to the Sun; w: intensity 0..1.
    sun: vec4<f32>,
    // x: height / width of the viewport.
    aspect: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> flare: Flare;

// Per ghost: position along the Sun→centre line (1 at the Sun), size, and colour.
const GHOSTS: u32 = 7u;
const T: array<f32, 7> = array<f32, 7>(1.0, 0.62, 0.35, 0.12, -0.25, -0.55, -0.95);
const SIZE: array<f32, 7> = array<f32, 7>(0.28, 0.05, 0.09, 0.03, 0.12, 0.06, 0.2);
const COLOR: array<vec3<f32>, 7> = array<vec3<f32>, 7>(
    vec3(1.0, 0.85, 0.6), vec3(0.4, 0.8, 1.0), vec3(1.0, 0.5, 0.25), vec3(0.6, 1.0, 0.6),
    vec3(0.5, 0.45, 1.0), vec3(1.0, 0.7, 0.4), vec3(0.35, 0.6, 1.0),
);

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) ghost: u32,
};

@vertex
fn vertex(@location(0) position: vec3<f32>) -> Out {
    var out: Out;
    let g = min(u32(position.z), GHOSTS - 1u);
    // The Sun at infinity, in clip space.
    let sun = view.clip_from_world * vec4(flare.sun.xyz, 0.0);
    if (sun.w <= 0.0 || flare.sun.w <= 0.001) {
        out.clip = vec4(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let centre = sun.xy / sun.w * T[g];
    let xy = centre + position.xy * SIZE[g] * vec2(flare.aspect.x, 1.0);
    // Nearest depth, drawn over everything (depth test off, no depth writes).
    out.clip = vec4(xy, 1.0, 1.0);
    out.uv = position.xy;
    out.ghost = g;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let r = length(in.uv);
    if (r > 1.0) {
        discard;
    }
    var a: f32;
    if (in.ghost == 0u) {
        // Glare round the Sun itself.
        a = pow(1.0 - r, 3.0) * 0.6;
    } else if (in.ghost % 3u == 1u) {
        // A faint ring.
        a = exp(-pow((r - 0.8) * 12.0, 2.0)) * 0.06;
    } else {
        // A soft disc, brightest at its rim.
        a = smoothstep(1.0, 0.6, r) * (0.3 + 0.7 * r * r) * 0.035;
    }
    let rgb = COLOR[in.ghost] * a * flare.sun.w * 3.0;
    return vec4(glow_out(rgb), 0.0);
}
