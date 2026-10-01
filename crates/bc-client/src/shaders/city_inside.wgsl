// The colony's windows seen from inside (`city.rs`): three strips of glass 3.35 km wide, the
// hull's ribs across them every 400 m and beams along them every 200 m. Through the glass, space
// turning with the colony (its stars wheel once every 113 s), and by day the light the mirrors
// throw in, so the glass shines pale. Kilometres of air lie between: the colony's haze, the same
// as the camera's distance fog on everything else.

#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}
#import bc::noise::hash13
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Inside {
    // xyz: where the render origin is in the colony's frame; w: the colony's spin angle (rad).
    origin: vec4<f32>,
    // x: daylight 0..1; y: lamps lit 0..1; z: the mirrors' opening (rad); w: the haze's density (/m).
    day: vec4<f32>,
    // rgb: the haze's colour (nits); a: unused.
    haze: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> inside: Inside;

const RADIUS: f32 = 3200.0;

// Stars in a direction of the sector's frame.
fn stars(d: vec3<f32>) -> f32 {
    let cell = floor(d * 260.0);
    let h = hash13(cell);
    let f = fract(d * 260.0) - 0.5;
    return step(0.985, h) * smoothstep(0.35, 0.0, length(f)) * (0.4 + 2.0 * fract(h * 37.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.world_position.xyz + inside.origin.xyz;
    let to = in.world_position.xyz - view.world_position;
    let dist = length(to);
    let v = to / max(dist, 1e-3);
    let daylight = inside.day.x;

    // Out through the glass: space in the sector's frame (the colony has turned by the spin).
    let c = cos(inside.origin.w);
    let s = sin(inside.origin.w);
    let d = vec3(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
    var col = vec3(stars(d)) * 900.0 * (1.0 - daylight);
    // The mirror outside sends the sun in: by day the glass shines, palest across its middle.
    let across = atan2(p.z, p.y) * RADIUS;
    let mid = abs(fract(across / (RADIUS * 6.2831853 / 6.0)) - 0.5) * 2.0;
    col += vec3(0.55, 0.75, 1.0) * (12000.0 + 5000.0 * (1.0 - mid)) * daylight;

    // The frame: ribs across every 400 m, beams along every 200 m of arc, lit by the city's light.
    let fx = fract(p.x / 400.0);
    let rib = smoothstep(6.0, 4.0, min(fx, 1.0 - fx) * 400.0);
    let fs = fract(across / 200.0);
    let beam = smoothstep(3.5, 2.0, min(fs, 1.0 - fs) * 200.0);
    let frame = max(rib, beam);
    col = mix(col, vec3(0.42, 0.46, 0.52) * (5200.0 * daylight + 20.0), frame);
    // Lamps along the ribs, at night.
    col += vec3(0.9, 1.0, 0.95) * rib * step(0.92, fract(across / 60.0)) * inside.day.y * 2500.0;

    // The air between.
    let haze = 1.0 - exp(-dist * inside.day.w);
    col = mix(col, inside.haze.rgb, haze);
    var out = vec4(col * view.exposure, 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
