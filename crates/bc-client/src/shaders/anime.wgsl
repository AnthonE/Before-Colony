// The painted look on trial (`docs/RING.md`, "The look"; `?look=anime`), shared by the hull shader
// and the station's: what a surface was lit laid down in a few bands. It takes no derivatives (the
// station shader calls it under its surface branch; `bc_client_core::station`'s tests hold it).

#define_import_path bc::anime

// The light a surface took laid down in a few flat bands with soft painted edges, the way a 1990s
// anime's backgrounds and its machines are coloured: violet-blue in shadow, a mid tone, the lit face
// warm, and a cool rim where the view grazes it. What the surface glows keeps (its lamps); a little of the physical light is
// kept under the bands, so plates and seams still read.
fn paint_bands(lit: vec3<f32>, albedo: vec3<f32>, glow: vec3<f32>, n: vec3<f32>, v: vec3<f32>) -> vec3<f32> {
    let lum = vec3(0.2126, 0.7152, 0.0722);
    let a = max(dot(albedo, lum), 0.02);
    // How much light it took, per unit of its own colour (already exposed).
    let took = max(dot(max(lit - glow, vec3(0.0)), lum), 0.0) / a;
    let soft = 0.03;
    let mid = smoothstep(0.06 - soft, 0.06 + soft, took);
    let high = smoothstep(0.38 - soft, 0.38 + soft, took);
    // The shadow side a cool violet and never black (the side turned from the sun still reads),
    // the half-lit a cool grey, the lit side warm.
    let shadow = vec3(0.40, 0.40, 0.66) * 0.42;
    let middle = vec3(0.78, 0.80, 0.98) * 0.72;
    let light = vec3(1.06, 1.0, 0.9);
    var level = mix(shadow, middle, mid);
    level = mix(level, light, high);
    // The brightest catch a highlight band above the light, as a painter's last stroke.
    let shine = smoothstep(1.4, 1.5, took);
    level += vec3(0.35) * shine;
    let rim = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 3.0) * 0.5 * (0.3 + 0.7 * mid);
    let painted = albedo * level + vec3(0.55, 0.7, 1.0) * rim * a + glow;
    return mix(painted, lit, 0.06);
}
