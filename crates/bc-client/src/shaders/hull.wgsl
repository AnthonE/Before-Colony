// Painted armour and hull plating (`HullMaterial`): a StandardMaterial whose colour, roughness,
// seams, grime and battle damage are procedural, in the piece's own space (in metres), so plates
// stay put as a part moves.
//
// Everything that differs per piece rides in its `MeshTag`, so every suit shares one material
// and identical pieces batch:
//   bits 0-3 paint (palette index), 4-6 armour left (7 pristine .. 0 destroyed), 7-12 seed,
//   13-15 heat (recent hits glow), 16-18 weathering (0 factory fresh .. 7 a veteran's), 19 dark
//   (nobody at the controls), 20 wreck, 21 bare metal, 22-25 trim paint, 26-29 accent paint,
//   30-31 eye colour.
// Merged suit meshes (bc_model) also carry per-vertex data in their colour: r the paint slot
// (0 body, 1 trim, 2 accent, 3 eye glow, 16+ a fixed paint, 32+ bare metal, 48+ glowing, 64+ a
// fixed paint from the palette's second bank, entries 16 and up, 80+ a running light), g 1 on
// bevels, b a panel seed, a ambient occlusion baked from the whole suit.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions::{get_tag, get_world_from_local, get_local_from_world},
}
#import bc::noise::{hash13, noise3, fbm}

struct Hull {
    // rgb: base colour (linear); a: perceptual roughness.
    palette: array<vec4<f32>, 32>,
    // x: plate size (m); y: seam width (m); z: seam depth (normal tilt); w: grime.
    panel: vec4<f32>,
    // x: seconds (embers flicker).
    time: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> hull: Hull;

// Sensor glow, by the tag's eye colour: green, pink, amber, cyan.
const EYES: array<vec3<f32>, 4> = array<vec3<f32>, 4>(
    vec3(0.2, 6.0, 1.2), vec3(6.0, 0.4, 2.2), vec3(6.0, 3.0, 0.3), vec3(0.4, 3.5, 6.0),
);

// Seven-segment digits, segments a to g in bits 0 to 6, for a suit's stencilled unit number.
const DIGITS: array<u32, 10> = array<u32, 10>(
    0x3Fu, 0x06u, 0x5Bu, 0x4Fu, 0x66u, 0x6Du, 0x7Du, 0x07u, 0x7Fu, 0x6Fu,
);

// A running light (bc_model::kit::Light, from slot 80): port red and starboard green steady, the
// white strobe's double flash, the red beacon's pulse, a lamp's soft warm white. `phase` (0..1)
// staggers one suit's flashes from another's.
fn light(kind: u32, t: f32, phase: f32) -> vec3<f32> {
    switch kind {
        case 0u: { return vec3(9.0, 0.3, 0.15); }
        case 1u: { return vec3(0.2, 8.0, 1.2); }
        case 2u: {
            let c = fract(t / 1.6 + phase);
            let flash = step(c, 0.035) + step(0.12, c) * step(c, 0.155);
            return vec3(0.2) + vec3(16.0, 16.0, 18.0) * flash;
        }
        case 3u: {
            let c = fract(t / 1.2 + phase);
            return vec3(0.3, 0.02, 0.0) + vec3(11.0, 0.7, 0.3) * exp(-c * 8.0);
        }
        default: { return vec3(2.4, 2.1, 1.7); }
    }
}

// Brick-like plates on a plane: distance to the nearest seam (m), a hash per plate, and the
// direction from that seam into the plate.
fn plates(uv: vec2<f32>, size: f32, seed: f32) -> vec4<f32> {
    let q = uv / size;
    let row = floor(q.y);
    let shifted = q.x + hash13(vec3(row, seed, 3.7)) * 0.7;
    let cell = vec2(floor(shifted), row);
    let f = vec2(fract(shifted), fract(q.y));
    // Some plates are split in two, so the layout never looks like a grid.
    let split = hash13(vec3(cell, seed + 1.3)) > 0.62;
    let fx = select(f.x, fract(f.x * 2.0), split);
    let wx = select(1.0, 0.5, split);
    let dx = min(fx, 1.0 - fx) * wx * size;
    let dy = min(f.y, 1.0 - f.y) * size;
    let h = hash13(vec3(cell + select(0.0, floor(f.x * 2.0) * 0.37, split), seed));
    if (dx < dy) {
        return vec4(dx, h, sign(0.5 - fx), 0.0);
    }
    return vec4(dy, h, 0.0, sign(0.5 - f.y));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let tag = get_tag(in.instance_index);
    var slot = 0u;
    var bevel = 0.0;
    var vseed = 0.0;
    var ao = 1.0;
#ifdef VERTEX_COLORS
    slot = u32(in.color.r * 255.0 + 0.5);
    bevel = in.color.g;
    vseed = in.color.b * 97.0;
    ao = in.color.a;
#endif
    var index = tag & 15u;
    var bare_metal = ((tag >> 21u) & 1u) == 1u;
    var glow = vec3(0.0);
    if (slot == 1u) {
        index = (tag >> 22u) & 15u;
    } else if (slot == 2u) {
        index = (tag >> 26u) & 15u;
    } else if (slot == 3u) {
        index = 15u;
        glow = EYES[(tag >> 30u) & 3u];
    } else if (slot >= 96u) {
        // A segment of the suit's stencilled unit number (its seed, 0..63; bc_model's
        // Paint::Digit: the tens' a to g, then the units'): stencil white where the digit lights
        // it, the body's paint where it doesn't.
        let k = slot - 96u;
        let number = (tag >> 7u) & 63u;
        let digit = select(number / 10u, number % 10u, k >= 7u);
        if (((DIGITS[digit] >> (k % 7u)) & 1u) == 1u) {
            index = 0u;
        }
    } else if (slot >= 80u) {
        // A light: its lens (the palette's LENS), lit.
        index = 19u;
        glow = light(slot - 80u, hull.time.x, fract(f32((tag >> 7u) & 63u) * 0.618));
    } else if (slot >= 64u) {
        index = slot - 48u;
    } else if (slot >= 48u) {
        index = slot - 48u;
        glow = hull.palette[index].rgb * 5.0;
    } else if (slot >= 32u) {
        index = slot - 32u;
        bare_metal = true;
    } else if (slot >= 16u) {
        index = slot - 16u;
    }
    let paint = hull.palette[index];
    let armour = f32((tag >> 4u) & 7u) / 7.0;
    let seed = f32((tag >> 7u) & 63u) + vseed;
    let heat = f32((tag >> 13u) & 7u) / 7.0;
    let worn = f32((tag >> 16u) & 7u) / 7.0;
    let dark = ((tag >> 19u) & 1u) == 1u;
    let wreck = ((tag >> 20u) & 1u) == 1u;

    // The piece's own space, in metres (its transform's scale taken back out).
    let m = get_world_from_local(in.instance_index);
    let scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
    let lfw = get_local_from_world(in.instance_index);
    let p = (lfw * vec4(in.world_position.xyz, 1.0)).xyz * scale;
    let n = normalize((lfw * vec4(in.world_normal, 0.0)).xyz * scale);
    let rot = mat3x3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);

    // Plates projected along the dominant axis: crisp on flat armour, a natural seam where a
    // curved surface turns.
    let a = abs(n);
    var uv = p.xy;
    var tu = vec3(1.0, 0.0, 0.0);
    var tv = vec3(0.0, 1.0, 0.0);
    if (a.x >= a.y && a.x >= a.z) {
        uv = p.zy;
        tu = vec3(0.0, 0.0, 1.0);
    } else if (a.y >= a.z) {
        uv = p.xz;
        tv = vec3(0.0, 0.0, 1.0);
    }
    let size = hull.panel.x;
    let ps = p / size;
    let pl = plates(uv + vec2(seed * 3.13, seed * 1.71), size, seed);
    // Metres per pixel here: seams and fine noise fade as they shrink below a pixel, instead of
    // shimmering.
    let fp = max(length(fwidth(p)), 1e-4);
    let seam_w = hull.panel.y;
    let seam = (1.0 - smoothstep(0.0, seam_w + fp * 0.5, pl.x)) * clamp(seam_w * 2.0 / fp, 0.0, 1.0);
    let detail = clamp(size * 0.25 / fp, 0.0, 1.0);

    var albedo = paint.rgb * (0.9 + 0.16 * pl.y);
    var rough = paint.a * (0.92 + 0.16 * mix(0.5, noise3(ps * 1.7 + seed), detail));
    var metallic = select(0.0, 0.85, bare_metal);
    // Bevels catch the light: worn a shade brighter and smoother.
    albedo = mix(albedo, min(albedo * 1.35 + vec3(0.04), vec3(1.0)), bevel * 0.55);
    rough = mix(rough, rough * 0.7, bevel);
    albedo *= 1.0 - 0.4 * seam;
    rough = mix(rough, 0.8, seam);
    // Grime in broad, soft patches a few plates across.
    let grime = fbm(ps * 0.35 + seed, 3);
    albedo *= 1.0 - hull.panel.w * 0.7 * smoothstep(0.5, 0.9, grime);

    // Weathering: what the suit's life has done to its paint (`worn`, from its pilot's roster
    // entry: 0 factory fresh .. 1 a veteran's). Sunlight fades it paler and greyer; grime settles
    // where the occlusion is and runs down the plates; the edges (the bevels the model marks) chip
    // to the red primer and then to bare metal, and on an old suit flakes come off the faces;
    // scratches cut through it, and fights long over leave their scorch marks. Lights, glass and
    // bare metal keep as they are.
    let painted = !bare_metal && slot != 3u && (slot < 48u || (slot >= 64u && slot < 80u) || slot >= 96u);
    if (worn > 0.0 && painted) {
        let grey = dot(albedo, vec3(0.3, 0.59, 0.11));
        albedo = mix(albedo, vec3(grey * 1.06 + 0.02), 0.3 * worn);
        let run = noise3(vec3(p.x * 2.2, p.y * 0.25, p.z * 2.2) + seed * 1.3);
        let dirt = (1.0 - ao) * 1.4 + smoothstep(0.55, 0.85, run) * 0.7;
        albedo = mix(albedo, vec3(0.045, 0.04, 0.03), clamp(dirt * worn * 0.55, 0.0, 0.65));
        rough = mix(rough, 0.85, clamp(dirt * worn * 0.4, 0.0, 0.4));
        let n_chip = fbm(p * 2.7 + seed * 1.7, 3);
        let t_edge = 0.82 - 0.2 * worn;
        let t_face = 0.8 - 0.1 * worn;
        let faces = smoothstep(0.6, 0.85, worn);
        let primer = max(
            bevel * smoothstep(t_edge, t_edge + 0.02, n_chip),
            faces * smoothstep(t_face, t_face + 0.02, n_chip),
        );
        let bare = max(
            bevel * smoothstep(t_edge + 0.05, t_edge + 0.07, n_chip),
            faces * smoothstep(t_face + 0.05, t_face + 0.07, n_chip),
        );
        albedo = mix(albedo, vec3(0.2, 0.085, 0.055), primer);
        albedo = mix(albedo, vec3(0.5, 0.5, 0.48), bare);
        metallic = mix(metallic, 0.9, bare);
        rough = mix(rough, 0.4, max(primer * 0.3, bare));
        // Scratches: thin lines where a stretched noise crosses its middle, in loose clusters,
        // fading out before they'd shimmer.
        let s = noise3(p * vec3(1.7, 15.0, 1.7) + seed * 2.3);
        let line = 1.0 - smoothstep(0.0, 0.03, abs(s - 0.5));
        let cluster = smoothstep(0.55, 0.8, noise3(p * 0.6 + seed * 0.7));
        let scratch = line * cluster * worn * clamp(0.015 / fp, 0.0, 1.0);
        albedo = mix(albedo, vec3(0.6, 0.6, 0.58), scratch * 0.6);
        metallic = mix(metallic, 0.8, scratch * 0.4);
        let burn = smoothstep(0.68, 0.86, fbm(p * 0.35 + seed * 4.1, 3)) * smoothstep(0.45, 1.0, worn);
        albedo = mix(albedo, vec3(0.03, 0.027, 0.024), burn * 0.8);
        rough = mix(rough, 0.92, burn * 0.7);
    }

    // Battle damage grows as armour runs out: scorching, then burnt-through paint showing the
    // bare frame. Heat makes fresh scorch edges glow.
    let hurt = 1.0 - armour;
    // A wreck's sensors are dark, and so are those of a suit whose pilot is asleep.
    var emissive = select(glow, vec3(0.0), wreck || dark);
    // A fresh hit leaves hot spots on the plate, cooling, even where the armour's still whole.
    if (heat > 0.0) {
        let spots = smoothstep(0.64, 0.82, fbm(p * 0.9 + seed * 5.3, 2));
        emissive += vec3(6.0, 1.6, 0.3) * spots * heat * heat;
    }
    if (hurt > 0.01 || wreck) {
        let reach = select(hurt, 1.0, wreck);
        let n_scorch = fbm(p * 0.45 + seed * 3.1, 4);
        let threshold = 1.0 - reach * 0.85;
        let scorch = smoothstep(threshold, threshold + 0.1, n_scorch);
        albedo = mix(albedo, vec3(0.03, 0.026, 0.022), scorch);
        rough = mix(rough, 0.95, scorch);
        let bare = scorch * smoothstep(0.55, 0.9, reach) * step(0.52, noise3(p * 2.1 + seed));
        albedo = mix(albedo, vec3(0.42, 0.43, 0.45), bare);
        metallic = mix(metallic, 1.0, bare);
        rough = mix(rough, 0.45, bare);
        let edge = scorch * (1.0 - scorch) * 4.0;
        emissive += vec3(6.0, 1.6, 0.3) * edge * heat;
        if (wreck) {
            let flicker = 0.6 + 0.4 * sin(hull.time.x * 7.0 + seed + p.x * 3.0);
            let embers = smoothstep(0.7, 0.9, fbm(p * 1.3 + seed, 3)) * scorch;
            emissive += vec3(3.0, 0.8, 0.15) * embers * flicker;
            albedo *= 0.6;
        }
    }

    // Seams are grooves: tilt the normal toward the seam line.
    let tilt = rot * (tu * pl.z + tv * pl.w) * seam * hull.panel.z;
    pbr.N = normalize(pbr.N - tilt);
    pbr.material.perceptual_roughness = clamp(rough, 0.05, 1.0);
    pbr.material.metallic = metallic;
    pbr.material.emissive = vec4(emissive, 0.0);
    // Crevices hold shadow: the sky's light doesn't reach into them, and a little of the Sun's
    // doesn't either (grime collects there too).
    pbr.diffuse_occlusion *= ao;
    pbr.specular_occlusion *= ao;
    pbr.material.base_color = vec4(albedo * mix(1.0, ao, 0.45), 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
