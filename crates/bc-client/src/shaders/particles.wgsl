// Effect particles: sparks, embers, flashes, fireballs and vapour. The CPU simulates them and
// writes one quad per particle into a mesh each frame; this expands each quad into a
// camera-facing billboard, or a streak along the particle's motion.
//
// Blending is premultiplied: glows write alpha 0 (pure addition), puffs write their coverage.

#import bevy_pbr::mesh_view_bindings::view
#import bc::glow::glow_out

struct Vertex {
    // The particle's centre, world space.
    @location(0) centre: vec3<f32>,
    // Which corner of the quad, (-1..1, -1..1).
    @location(1) corner: vec2<f32>,
    // rgb: raw HDR colour; a: opacity.
    @location(2) color: vec4<f32>,
    // x: radius (m); y: streak length (m); z: 1 for a puff (alpha-blended), 0 for a glow; w: core.
    @location(3) shape: vec4<f32>,
    // Streak direction (unit, world); zero for a round billboard.
    @location(4) dir: vec3<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let radius = v.shape.x;
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    var half_len = radius;
    if (v.shape.y > 0.0 && dot(v.dir, v.dir) > 0.25) {
        // A streak: long along its motion, turned to face the camera.
        let to_cam = normalize(view.world_position - v.centre);
        up = normalize(v.dir);
        // Seen end-on the side is undefined: lean on camera-right so it never goes NaN.
        right = normalize(cross(up, to_cam) + view.world_from_view[0].xyz * 1e-3);
        half_len = radius + v.shape.y * 0.5;
    }
    let world = v.centre + right * v.corner.x * radius + up * v.corner.y * half_len;
    out.clip = view.clip_from_world * vec4(world, 1.0);
    out.uv = v.corner;
    out.color = v.color;
    out.shape = v.shape;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let r2 = dot(in.uv, in.uv);
    if (r2 > 1.0) {
        discard;
    }
    // A hot core inside a soft halo; puffs are just soft.
    let halo = (1.0 - r2) * (1.0 - r2);
    let core = exp(-r2 * 18.0) * in.shape.w;
    let a = in.color.a * (halo + core);
    let rgb = glow_out(in.color.rgb);
    if (in.shape.z > 0.5) {
        return vec4(rgb * a, a);
    }
    return vec4(rgb * a, 0.0);
}
