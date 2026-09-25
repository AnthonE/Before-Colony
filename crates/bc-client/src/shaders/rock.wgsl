// Asteroids (`RockMaterial`): regolith, strata and bumps in the rock's own space, with specks and
// veins of its ore catching the light, and cracks as it is mined.
//
// Per-rock data rides in the `MeshTag`: bits 0-1 ore kind, 2-9 seed, 10-12 structure left
// (eighths), 13-16 ore left (sixteenths).

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

// How far `p` is from the nearest wall between two Voronoi cells (in cells): near 0 along a
// network of polygonal fractures.
fn fracture(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    var d1 = 9.0;
    var d2 = 9.0;
    for (var z = -1; z <= 1; z += 1) {
        for (var y = -1; y <= 1; y += 1) {
            for (var x = -1; x <= 1; x += 1) {
                let o = vec3(f32(x), f32(y), f32(z));
                let c = i + o;
                let q = o + vec3(hash13(c), hash13(c + 17.31), hash13(c + 41.73)) - f;
                let d = dot(q, q);
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
    }
    return sqrt(d2) - sqrt(d1);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let tag = get_tag(in.instance_index);
    let ore_kind = tag & 3u;
    let seed = f32((tag >> 2u) & 255u);
    let intact = f32((tag >> 10u) & 7u) / 7.0;
    let richness = f32((tag >> 13u) & 15u) / 15.0;

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

    // Veins: thin ridged bands of ore, thicker in a rich rock and gone from a worked-out one.
    let vn = abs(fbm(p * 0.07 + vec3(3.1, 7.7, 1.3), 3) - 0.5);
    let vein = (1.0 - smoothstep(0.004, 0.02 + 0.035 * richness, vn)) * richness;
    albedo = mix(albedo, ore.rgb * 1.1, vein);
    metallic = mix(metallic, ore.a, vein);
    rough = mix(rough, 0.28, vein);
    // The exotics glow faintly in their veins.
    var emissive = select(vec3(0.0), ore.rgb * 0.6 * vein, ore_kind == 3u);

    // Cracks open as the rock is worked: a network of fractures a few metres across, then a finer
    // one as it nears breaking. They're dark, and their fresh cores glow as the rock weakens.
    let damage = 1.0 - intact;
    if (damage > 0.01) {
        let wc = 0.03 + 0.07 * damage;
        let coarse = fracture(p * 0.22) / wc;
        let wf = 0.06 * smoothstep(0.35, 0.9, damage);
        let fine = fracture(p * 0.6 + vec3(5.2, 1.3, 8.8)) / max(wf, 1e-4);
        let crack = max(1.0 - smoothstep(0.4, 1.0, coarse), (1.0 - smoothstep(0.4, 1.0, fine)) * step(1e-3, wf));
        albedo *= 1.0 - 0.85 * crack;
        rough = mix(rough, 1.0, crack);
        let core = 1.0 - smoothstep(0.0, 0.35, coarse);
        emissive += vec3(2.4, 0.7, 0.14) * core * damage * damage;
    }

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
    pbr.material.emissive = vec4(emissive, 0.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
