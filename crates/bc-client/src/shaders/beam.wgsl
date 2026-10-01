// Beams, tracers and beam-saber blades: a white-hot core inside a coloured glow, with a flicker
// that runs along it, rounded off at the head. Shots fade along the tail; blades (MeshTag bit 8)
// burn full length from the hilt. The Twin Buster Rifle adds a twisting striation.

#import bc::ribbon::{RibbonOut, ribbon_cap, ribbon_vertex}
#import bc::glow::glow_out

struct Beam {
    // rgb: glow colour (raw HDR); a: core brightness.
    color: vec4<f32>,
    // x: core width (fraction of the half-width); y: flicker; z: seconds; w: striation.
    params: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> beam: Beam;

@vertex
fn vertex(@builtin(instance_index) instance: u32, @location(0) position: vec3<f32>) -> RibbonOut {
    // At least 1.2 px either side of the axis, however far.
    return ribbon_vertex(instance, position, 1.2);
}

@fragment
fn fragment(in: RibbonOut) -> @location(0) vec4<f32> {
    let x = in.uv.x;
    let y = in.uv.y;
    let seed = f32(in.tag & 255u);
    let blade = (in.tag & 256u) != 0u;
    // Across the ribbon, squeezed into the cap at the head.
    let xs = x / max(ribbon_cap(in), 0.02);
    let w = beam.params.x;
    let core = exp(-(xs * xs) / (w * w));
    let glow = exp(-xs * xs * 3.5) * max(1.0 - xs * xs, 0.0);
    // Shots: faint at the tail, full at the head. Blades: full from the hilt.
    let along = select(smoothstep(0.0, 0.45, y), smoothstep(0.0, 0.02, y), blade);
    let t = beam.params.z;
    // Flicker and striae run along the beam in metres, so they don't stretch as it grows.
    // (A blade is short and fixed: its ripple keeps the rifle's count along it.)
    let m = y * select(in.size.y, 110.0, blade);
    let flicker = 1.0 + beam.params.y * sin(m * 0.39 - t * 95.0 + seed);
    let striae = 1.0 + beam.params.w * sin(m * 0.146 + xs * 5.0 - t * 60.0 + seed) * glow;
    let rgb = (beam.color.rgb * glow * striae + vec3(1.0, 0.97, 0.94) * core * beam.color.a) * along * flicker;
    return vec4(glow_out(rgb * in.dim), 0.0);
}
