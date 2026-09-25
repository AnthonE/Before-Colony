// Thruster plumes: a blue-white core with shock diamonds near the nozzle, a bluer glow flaring
// out behind, and a flicker. Intensity (0-255) rides in the MeshTag; length and width are the
// entity's scale.

#import bc::ribbon::{RibbonOut, ribbon_vertex}
#import bc::glow::glow_out

struct Plume {
    // rgb: core colour; a: unused.
    core: vec4<f32>,
    // rgb: outer glow colour; a: seconds.
    glow: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> plume: Plume;

@vertex
fn vertex(@builtin(instance_index) instance: u32, @location(0) position: vec3<f32>) -> RibbonOut {
    return ribbon_vertex(instance, position);
}

@fragment
fn fragment(in: RibbonOut) -> @location(0) vec4<f32> {
    let power = f32(in.tag & 255u) / 255.0;
    let seed = f32((in.tag >> 8u) & 255u);
    let x = in.uv.x;
    let y = in.uv.y;
    let t = plume.glow.a;
    // The jet widens as it leaves the nozzle.
    let spread = mix(0.35, 1.0, y);
    let r = x / spread;
    let core = exp(-r * r * 16.0) * pow(1.0 - y, 1.5);
    let diamonds = 0.65 + 0.35 * cos(y * 34.0) * exp(-y * 3.0);
    let glow = exp(-r * r * 3.0) * (1.0 - y) * (1.0 - y);
    let flicker = 0.85 + 0.15 * sin(t * 57.0 + seed + y * 9.0);
    let rgb = (plume.core.rgb * core * diamonds + plume.glow.rgb * glow) * flicker * power;
    return vec4(glow_out(rgb), 0.0);
}
