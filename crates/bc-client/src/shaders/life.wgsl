// The city's cars and people (`LifeMaterial`, `life.rs`): a StandardMaterial whose colour comes from
// a 64-entry palette, picked per part by the vertex's slot (its colour's red) and the instance's
// `MeshTag` (`bc::life`), so every car and person shares one material and identical meshes batch.
// Lit as the city's walls are: the scene's light, its sky and the street's bounce (`bounce`).
// Lamps glow in nits; paint wears with the tag's wear, grimier where the occlusion is (low down);
// a figure or car fading in or out (a door, a pilot passing through) is dithered.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions::{get_tag, get_world_from_local},
    mesh_view_bindings::lights,
}
#import bc::life::{field, shown, paint_index, is_lamp, lamp_albedo, car_glow, bounce, TAG_FADE, CAR_PARKED, CAR_WEAR, SLOT_PAINT, SLOT_PAINT2, SLOT_DRIVER}

struct Life {
    // rgb: base colour (linear); a: perceptual roughness.
    palette: array<vec4<f32>, 64>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> life: Life;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let tag = get_tag(in.instance_index);
    var slot = 0u;
    var bevel = 0.0;
    var ao = 1.0;
#ifdef VERTEX_COLORS
    slot = u32(in.color.r * 255.0 + 0.5);
    bevel = in.color.g;
    ao = in.color.a;
#endif
    // Fading, or a parked car's driver gone.
    if (!shown(in.position.xy, field(tag, TAG_FADE, 4u)) || (slot == SLOT_DRIVER && field(tag, CAR_PARKED, 1u) == 1u)) {
        discard;
    }
    let paint = life.palette[paint_index(slot, tag)];
    var albedo = paint.rgb;
    var rough = paint.a;
    var glow = vec3(0.0);
    if (slot == SLOT_PAINT || slot == SLOT_PAINT2) {
        // Wear dulls and greys the paint and lays grime where the light doesn't reach; bevels
        // catch the light.
        let wear = f32(field(tag, CAR_WEAR, 3u)) / 7.0;
        albedo = mix(albedo, vec3(dot(albedo, vec3(0.3, 0.5, 0.2))), wear * 0.35);
        albedo *= 1.0 - wear * 0.5 * (1.0 - ao);
        albedo = mix(albedo, min(albedo * 1.25 + vec3(0.03), vec3(1.0)), bevel * 0.5);
        rough = mix(rough, 0.8, wear * 0.6);
    } else if (is_lamp(slot)) {
        albedo = lamp_albedo(slot);
        rough = 0.15;
        glow = car_glow(slot, tag);
    } else if (slot == SLOT_DRIVER) {
        // Somebody behind the glass.
        albedo = vec3(0.09, 0.07, 0.06);
        rough = 0.8;
    }
    let base = albedo * mix(1.0, ao, 0.5);
    // The street's bounce off the scene's one light (the key, or a room's lamps) and its sky; up is
    // the instance's own (cars and people stand on the floor).
    let up = normalize(get_world_from_local(in.instance_index)[1].xyz);
    var key = vec3(0.0);
    var to_key = up;
    if (lights.n_directional_lights > 0u) {
        key = lights.directional_lights[0].color.rgb;
        to_key = lights.directional_lights[0].direction_to_light;
    }
    glow += base * bounce(normalize(pbr.N), up, to_key, key, lights.ambient_color.rgb) * ao;
    pbr.material.base_color = vec4(base, 1.0);
    pbr.material.perceptual_roughness = clamp(rough, 0.05, 1.0);
    pbr.material.metallic = 0.0;
    pbr.material.emissive = vec4(glow, 1.0);
    pbr.diffuse_occlusion *= ao;
    pbr.specular_occlusion *= ao;

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
