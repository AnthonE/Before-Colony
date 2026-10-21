// The city's prepass (WebGPU's ambient occlusion, `gfx.rs`): its depth and normals, holding back
// what the main pass (`city.wgsl`) cuts out. The main pass tests against the prepass's depth, so
// anything drawn here that `city.wgsl` discards would be a hole: the canal's channel in the ground
// over the water, and the quays' railings, whose gaps are dithered (left out here whole: they cast
// no occlusion, and the main pass still draws them).
//
// Shadow passes (no PREPASS_FRAGMENT) hold nothing back, as before this shader.

#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_functions::prepare_world_normal,
    pbr_prepass_functions::calculate_motion_vector,
}
#import bc::city::{city_cell, atlas_texel, CANAL_WIDTH}

@group(#{MATERIAL_BIND_GROUP}) @binding(101) var atlas: texture_2d<f32>;

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var surface = 0u;
    var seed = 0.0;
#ifdef VERTEX_COLORS
    surface = u32(in.color.r * 255.0 + 0.5);
    seed = in.color.g * 255.0;
#endif
    var uv = vec2(0.0);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    if (surface == 10u) {
        discard;
    }
    if (surface == 0u) {
        // The ground over the canal's channel, as `city.wgsl` finds it (the ground's seed is its strip).
        let cell = city_cell(uv.x, uv.y);
        let t = textureLoad(atlas, atlas_texel(i32(seed + 0.5), cell), 0);
        if (i32(t.r * 255.0 + 0.5) == 4) {
            let mid = (cell.rect.x + cell.rect.y) * 0.5;
            if (abs(uv.x - mid) < CANAL_WIDTH * 0.5 && uv.y > cell.rect.z && uv.y < cell.rect.w) {
                discard;
            }
        }
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
