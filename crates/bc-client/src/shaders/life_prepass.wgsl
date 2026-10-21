// The city's cars and people in the prepass (WebGPU's ambient occlusion, `gfx.rs`): their depth and
// normals, holding back what the main pass (`life.wgsl`) cuts out (one fading in or out, dithered; a
// parked car's driver), which would otherwise leave a hole where the prepass's depth stands in for
// them. Shadow passes (no PREPASS_FRAGMENT) hold nothing back, as before this shader.

#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_functions::prepare_world_normal,
    pbr_prepass_functions::calculate_motion_vector,
    mesh_functions::get_tag,
}
#import bc::life::{field, shown, TAG_FADE, CAR_PARKED, SLOT_DRIVER}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let tag = get_tag(in.instance_index);
    var slot = 0u;
#ifdef VERTEX_COLORS
    slot = u32(in.color.r * 255.0 + 0.5);
#endif
    if (!shown(in.position.xy, field(tag, TAG_FADE, 4u)) || (slot == SLOT_DRIVER && field(tag, CAR_PARKED, 1u) == 1u)) {
        discard;
    }

    var out: FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#ifdef NORMAL_PREPASS
    out.normal = vec4(prepare_world_normal(in.world_normal, false, is_front) * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {}
#endif
