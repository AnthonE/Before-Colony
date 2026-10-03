// The colony's windows seen from inside (`city.rs`): three strips of glass 3.35 km wide, the
// hull's ribs across them every 400 m and beams along them every 200 m. Through the glass, space
// turning with the colony (its stars wheel once every 113 s), and by day the light the mirrors
// throw in, so the glass shines (`bc::colony_sky`'s glow, by the hour). Kilometres of air lie
// between: the sky function's haze, the same as on everything else inside.

#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}
#import bc::noise::hash13
#import bc::colony_sky::{Sky, haze}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Inside {
    // xyz: where the render origin is in the colony's frame; w: the colony's spin angle (rad).
    origin: vec4<f32>,
    // x: daylight 0..1; y: lamps lit 0..1; z: the mirrors' opening (rad); w: the haze's density (/m).
    day: vec4<f32>,
    // rgb: the haze's colour (nits); a: unused. (Not `haze`: naga_oil renames every bare identifier
    // that matches an import, struct fields included.)
    haze_rgb: vec4<f32>,
    // The hour's air and light (`city.rs`'s colour script).
    sky: Sky,
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

// A line `half` metres either side of each multiple of `period`, at `x` metres, drawn over the
// pixel's footprint `w` (m): solid up close, thinning to its share of the glass far off instead of
// shimmering.
fn line(x: f32, period: f32, half: f32, w: f32) -> f32 {
    let f = fract(x / period);
    let d = min(f, 1.0 - f) * period;
    // The share of the pixel's footprint, d ± w/2, that the line's −half..half covers.
    let cover = max(min(half, d + 0.5 * w) - max(-half, d - 0.5 * w), 0.0) / w;
    return mix(cover, 2.0 * half / period, smoothstep(0.25, 1.0, w / period));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.world_position.xyz + inside.origin.xyz;
    let to = in.world_position.xyz - view.world_position;
    let dist = length(to);
    let v = to / max(dist, 1e-3);
    let daylight = inside.sky.ambient.w;
    // Across the glass, m of arc. (The angle's seam at ±180° lies over a strip, not a window.)
    let across = atan2(p.z, p.y) * RADIUS;
    // How much glass a pixel covers, m: derivatives first, in uniform control flow, of the
    // render-relative position (small numbers).
    let w_along = max(fwidth(in.world_position.x), 1e-3);
    let w_across = max(fwidth(across), 1e-3);

    // Out through the glass: space in the sector's frame (the colony has turned by the spin).
    let c = cos(inside.origin.w);
    let s = sin(inside.origin.w);
    let d = vec3(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
    var col = vec3(stars(d)) * 900.0 * (1.0 - daylight);
    // The mirror outside sends the sun in: by day the glass shines, palest across its middle. `mid`
    // is 0 on a window's centre line (`frame::window_centre`: 0.4 rad + k·120°) and 1 at its edges.
    let mid = abs(fract((across / RADIUS - 0.4) * 0.95492966 + 0.5) - 0.5) * 2.0;
    col += inside.sky.glow.rgb * (0.7 + 0.3 * (1.0 - mid));

    // The frame: ribs across every 400 m, beams along every 200 m of arc, lit by the colony's light.
    let rib = line(p.x, 400.0, 5.0, w_along);
    let beam = line(across, 200.0, 2.75, w_across);
    let frame = max(rib, beam);
    col = mix(col, vec3(0.45, 0.48, 0.53) * (inside.sky.ambient.rgb * 1.9 + vec3(20.0)), frame);
    // Lamps along the ribs, at night.
    col += vec3(0.9, 1.0, 0.95) * rib * step(0.92, fract(across / 60.0)) * inside.sky.night.w * 2500.0;

    // The air between.
    let air = haze(view.world_position + inside.origin.xyz, p, inside.sky);
    col = col * air.transmittance + air.inscatter;
    var out = vec4(col * view.exposure, 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
