// Space dust: motes wrapped in a box around the camera, streaking as you fly, so Newtonian speed
// is something you can see. Each mote's position (in [0,1)³) is its seed; the box follows the
// camera, so the motes stay put in the world while the camera moves through them.

#import bevy_pbr::mesh_view_bindings::view
#import bc::glow::glow_out

struct Dust {
    // xyz: the camera's velocity (m/s); w: streak length, as seconds of motion.
    velocity: vec4<f32>,
    // x: box size (m); y: mote radius (m); z: brightness.
    shape: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> dust: Dust;

struct Vertex {
    @location(0) seed: vec3<f32>,
    @location(1) corner: vec2<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
};

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let size = dust.shape.x;
    let cam = view.world_position;
    let p = cam + (fract(v.seed - cam / size + 0.5) - 0.5) * size;
    let rel = p - cam;
    let dist = length(rel);
    out.fade = (1.0 - smoothstep(size * 0.3, size * 0.5, dist)) * smoothstep(3.0, 8.0, dist);
    let radius = dust.shape.y;
    let vel = dust.velocity.xyz;
    let streak = length(vel) * dust.velocity.w;
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    var half_len = radius;
    if (streak > radius) {
        up = normalize(vel);
        // Seen end-on the side is undefined: lean on camera-right so it never goes NaN.
        right = normalize(cross(up, -rel) + view.world_from_view[0].xyz * (1e-3 * dist));
        half_len = streak * 0.5;
    }
    let world = p + right * v.corner.x * radius + up * v.corner.y * half_len;
    out.clip = view.clip_from_world * vec4(world, 1.0);
    out.uv = v.corner;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    // Round when still, tapering at both ends when streaked.
    let along = 1.0 - in.uv.y * in.uv.y;
    let a = exp(-in.uv.x * in.uv.x * 4.0) * along * along * in.fade * dust.shape.z;
    return vec4(glow_out(vec3(0.85, 0.88, 0.95)) * a, 0.0);
}
