// Shared by the effect shaders (beams, plumes, particles, shells, dust, flare), which write raw
// HDR colour. With HDR on, the camera's tonemapping pass maps it; on LDR tiers there is no such
// pass, so it is tonemapped here instead of clipping to white.
#define_import_path bc::glow

#import bevy_pbr::mesh_view_bindings::view
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

fn glow_out(rgb: vec3<f32>) -> vec3<f32> {
#ifdef TONEMAP_IN_SHADER
    return tone_mapping(vec4(rgb, 1.0), view.color_grading).rgb;
#else
    return rgb;
#endif
}
