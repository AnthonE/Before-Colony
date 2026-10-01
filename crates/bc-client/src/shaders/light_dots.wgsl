// Small lights in their thousands (`dots.rs`): the rows of lamps along the colony's window frames,
// rings, spokes, spire and mirrors. Each is a quad turned to face the camera round its centre (in
// the entity's space, so the lights turn with what they're on), never smaller on screen than the
// material's least size, dimmed to match as it's spread wider: a far row of lamps stays a dotted
// line of the same light instead of shimmering away. Some blink.

#import bevy_pbr::{mesh_functions::get_world_from_local, mesh_view_bindings::view}
#import bc::glow::glow_out

struct Dots {
    // x: seconds; y: the least diameter on screen (px); z: brightness; w: unused.
    p: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> dots: Dots;

struct Vertex {
    @builtin(instance_index) instance: u32,
    // The light's centre, in the entity's space.
    @location(0) centre: vec3<f32>,
    // Which corner of the quad, (-1..1, -1..1).
    @location(1) corner: vec2<f32>,
    // rgb: raw HDR colour.
    @location(2) color: vec4<f32>,
    // x: radius (m); y: blink period (s; 0 steady); z: phase (0..1); w: the part of it lit.
    @location(3) shape: vec4<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let m = get_world_from_local(v.instance);
    let centre = (m * vec4(v.centre, 1.0)).xyz;
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    // Pixels per metre where it is.
    let depth = max((view.clip_from_world * vec4(centre, 1.0)).w, 1e-3);
    let ppm = view.viewport.w * 0.5 * view.clip_from_view[1][1] / depth;
    let least = dots.p.y * 0.5 / max(ppm, 1e-6);
    let r = max(v.shape.x, least);
    let spread = v.shape.x / r;
    var on = 1.0;
    if (v.shape.y > 0.0) {
        on = select(0.0, 1.0, fract(dots.p.x / v.shape.y + v.shape.z) < v.shape.w);
    }
    let world = centre + (right * v.corner.x + up * v.corner.y) * r;
    out.clip = view.clip_from_world * vec4(world, 1.0);
    out.uv = v.corner;
    out.color = v.color.rgb * max(spread * spread, 0.2) * on * dots.p.z;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let r2 = dot(in.uv, in.uv);
    if (r2 > 1.0) {
        discard;
    }
    // A bright core in a soft halo.
    let a = exp(-r2 * 5.0) + exp(-r2 * 40.0);
    return vec4(glow_out(in.color * a), 0.0);
}
