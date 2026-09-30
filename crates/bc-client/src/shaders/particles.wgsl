// Effect particles: sparks, embers, flashes, fireballs, smoke and dust. The CPU simulates them and
// writes one quad per particle into a mesh each frame; this expands each quad into a
// camera-facing billboard, or a streak along the particle's motion.
//
// Blending is premultiplied: glows write alpha 0 (pure addition), puffs write their coverage.
// Puffs (smoke, vapour, rock dust) are lit by the Sun as soft spheres, with the ambient fill on
// their dark side, and their edges break up in noise; fire billows. Both take their seed and age
// from the direction attribute, which round particles don't otherwise use.

#import bevy_pbr::mesh_view_bindings::view
#import bc::glow::glow_out
#import bc::noise::fbm

struct Light {
    // xyz: toward the Sun (unit); w: how much of the Sun reaches here (eclipse, indoors), 0..1.
    sun: vec4<f32>,
    // rgb: fill light on the dark side (raw screen units); w: noise detail, 0 (none) .. 1.
    ambient: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> light: Light;

struct Vertex {
    // The particle's centre, world space.
    @location(0) centre: vec3<f32>,
    // Which corner of the quad, (-1..1, -1..1).
    @location(1) corner: vec2<f32>,
    // rgb: raw HDR colour (a puff's: its albedo); a: opacity.
    @location(2) color: vec4<f32>,
    // x: radius (m); y: streak length (m); z: 0 a glow, 1 a puff (alpha-blended), 2 fire; w: core.
    @location(3) shape: vec4<f32>,
    // A streak's direction (unit, world); a round particle's (seed, life 0..1, 0).
    @location(4) dir: vec3<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: vec4<f32>,
    // x: seed; y: life 0..1.
    @location(3) extra: vec2<f32>,
    @location(4) @interpolate(flat) right: vec3<f32>,
    @location(5) @interpolate(flat) up: vec3<f32>,
    @location(6) @interpolate(flat) to_cam: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let radius = v.shape.x;
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    var half_len = radius;
    let to_cam = normalize(view.world_position - v.centre);
    out.extra = vec2(0.0);
    if (v.shape.y > 0.0 && dot(v.dir, v.dir) > 0.25) {
        // A streak: long along its motion, turned to face the camera.
        up = normalize(v.dir);
        // Seen end-on the side is undefined: lean on camera-right so it never goes NaN.
        right = normalize(cross(up, to_cam) + view.world_from_view[0].xyz * 1e-3);
        half_len = radius + v.shape.y * 0.5;
    } else {
        out.extra = v.dir.xy;
        // Round particles turn by their seed, so no two billows look alike.
        let a = v.dir.x * 6.2831853;
        let c = cos(a);
        let s = sin(a);
        let r0 = right;
        right = r0 * c + up * s;
        up = up * c - r0 * s;
    }
    let world = v.centre + right * v.corner.x * radius + up * v.corner.y * half_len;
    out.clip = view.clip_from_world * vec4(world, 1.0);
    out.uv = v.corner;
    out.color = v.color;
    out.shape = v.shape;
    out.right = right;
    out.up = up;
    out.to_cam = to_cam;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let r2 = dot(in.uv, in.uv);
    if (r2 > 1.0) {
        discard;
    }
    let kind = in.shape.z;
    let detail = light.ambient.w;
    let seed = in.extra.x;
    let age = in.extra.y;
    if (kind > 0.5 && kind < 1.5) {
        // A puff: a soft ball of smoke whose edge frays in noise and churns as it ages.
        var n = 0.5;
        if (detail > 0.0) {
            n = fbm(vec3(in.uv * 1.7 + seed * 41.0, seed * 13.0 + age * 1.3), 3);
        }
        let body = 1.0 - r2;
        // Soft all the way to the middle, its edge frayed by the noise.
        let dens = pow(clamp(body + (n - 0.5) * 0.5 * detail, 0.0, 1.0), 1.4) * (0.8 + 0.4 * n * detail);
        let a = clamp(in.color.a * dens, 0.0, 1.0);
        // Lit as a sphere: the side facing the Sun bright, the far side in the fill.
        let nz = sqrt(max(1.0 - r2, 0.0));
        let normal = normalize(in.right * in.uv.x + in.up * in.uv.y + in.to_cam * (nz + 0.25));
        let ndl = dot(normal, light.sun.xyz);
        // Smoke scatters: even its far side takes some sun.
        let wrap = clamp(ndl * 0.5 + 0.5, 0.0, 1.0);
        let lit = in.color.rgb * (1.14 * wrap * light.sun.w + light.ambient.rgb);
        return vec4(glow_out(lit) * a, a);
    }
    // A glow: a hot core inside a soft halo. Fire billows.
    var halo = (1.0 - r2) * (1.0 - r2);
    if (kind > 1.5 && detail > 0.0) {
        let n = fbm(vec3(in.uv * 2.1 + seed * 29.0, seed * 7.0 + age * 2.0), 3);
        halo *= mix(1.0, 0.55 + 0.9 * n, detail);
    }
    let core = exp(-r2 * 18.0) * in.shape.w;
    let a = in.color.a * (halo + core);
    return vec4(glow_out(in.color.rgb * a), 0.0);
}
