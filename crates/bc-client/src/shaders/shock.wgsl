// Glowing shells, lit only where they graze the line of sight: an explosion's shock front (a
// sphere), the ZERO System's aura, and — on a flat disc, MeshTag bit 8 — the ring a Twin Buster
// shot throws off round the muzzle. Fade (255 fresh .. 0 gone) rides in the MeshTag's low byte.

#import bevy_pbr::{forward_io::VertexOutput, mesh_functions::get_tag, mesh_view_bindings::view}
#import bc::glow::glow_out

struct Shock {
    // rgb: raw HDR colour; a: how sharply the rim falls off (a sphere's rim exponent).
    color: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> shock: Shock;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tag = get_tag(in.instance_index);
    let fade = f32(tag & 255u) / 255.0;
    var glow = 0.0;
    if ((tag & 256u) != 0u) {
#ifdef VERTEX_UVS_A
        // A ring: a bright band near the disc's edge inside a fainter halo.
        let r = length(in.uv * 2.0 - 1.0);
        glow = exp(-pow((r - 0.82) / 0.06, 2.0)) + 0.2 * exp(-pow((r - 0.6) / 0.2, 2.0));
#endif
    } else {
        let v = normalize(view.world_position - in.world_position.xyz);
        glow = pow(1.0 - abs(dot(normalize(in.world_normal), v)), shock.color.a);
    }
    return vec4(glow_out(shock.color.rgb * glow * fade * fade), 0.0);
}
