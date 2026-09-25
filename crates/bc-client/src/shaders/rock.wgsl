// Asteroids (`RockMaterial`): regolith, strata and bumps in the rock's own space, with specks of
// its ore catching the light.
//
// Per-rock data rides in the `MeshTag`: bits 0-1 ore kind, 2-9 seed.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions::{get_tag, get_world_from_local, get_local_from_world},
}
#import bc::noise::{hash13, noise3, fbm}

struct Rock {
    // Per ore kind: rgb colour of the ore's specks (linear), a: how metallic they are.
    ore: array<vec4<f32>, 4>,
    // x: detail (octaves, 0..1).
    detail: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> rock: Rock;

fn height(p: vec3<f32>, octaves: i32) -> f32 {
    return fbm(p * 0.35, octaves);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let tag = get_tag(in.instance_index);
    let ore_kind = tag & 3u;
    let seed = f32((tag >> 2u) & 255u);

    // The rock's own space, in metres.
    let m = get_world_from_local(in.instance_index);
    let scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
    let lfw = get_local_from_world(in.instance_index);
    let p = (lfw * vec4(in.world_position.xyz, 1.0)).xyz * scale + seed * 17.0;
    let rot = mat3x3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);
    let octaves = i32(round(mix(2.0, 5.0, rock.detail.x)));

    // Regolith, darker in hollows, banded by old strata along a per-rock axis.
    let h = height(p, octaves);
    let axis = normalize(vec3(hash13(vec3(seed, 1.0, 2.0)), 0.6, hash13(vec3(seed, 3.0, 4.0))) - 0.3);
    let strata = sin(dot(p, axis) * 0.22 + h * 6.0) * 0.5 + 0.5;
    var albedo = mix(vec3(0.075, 0.068, 0.06), vec3(0.2, 0.18, 0.16), h);
    albedo *= 0.8 + 0.3 * strata;
    var rough = 0.88 + 0.1 * noise3(p * 1.7);
    var metallic = 0.0;

    // Ore specks: small, bright and smoother, so they glint in the sun.
    let ore = rock.ore[ore_kind];
    let speck = smoothstep(0.78, 0.86, noise3(p * 2.6)) * smoothstep(0.45, 0.7, fbm(p * 0.12, 2));
    albedo = mix(albedo, ore.rgb, speck);
    metallic = mix(metallic, ore.a, speck);
    rough = mix(rough, 0.35, speck);

    // Bumps: the height field's gradient, by finite differences.
    let e = 0.6;
    let g = vec3(
        height(p + vec3(e, 0.0, 0.0), octaves) - h,
        height(p + vec3(0.0, e, 0.0), octaves) - h,
        height(p + vec3(0.0, 0.0, e), octaves) - h,
    ) / e;
    pbr.N = normalize(pbr.N - rot * g * 1.8);

    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = rough;
    pbr.material.metallic = metallic;

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
