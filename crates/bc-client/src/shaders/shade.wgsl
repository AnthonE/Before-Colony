// A contact shadow (`shade.rs`): a rectangle on the floor, darkest inside, its edge fading out over
// a margin. Drawn by multiplying what's behind by the colour written here. The MeshTag carries its
// strength (bits 0-7, of 255) and its margin (bits 8-15, in decimetres).

#import bevy_pbr::{forward_io::VertexOutput, mesh_functions::{get_tag, get_world_from_local}}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tag = get_tag(in.instance_index);
    let strength = f32(tag & 255u) / 255.0;
    let margin = max(f32((tag >> 8u) & 255u) * 0.1, 0.1);
    let m = get_world_from_local(in.instance_index);
    let half = vec2(length(m[0].xyz), length(m[2].xyz)) * 0.5;
    var uv = vec2(0.5);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    let p = (uv - 0.5) * 2.0 * half;
    // Distance out from the inner rectangle, in margins: 0 inside, 1 at the quad's edge.
    let inner = max(half - vec2(margin), vec2(0.0));
    let d = length(max(abs(p) - inner, vec2(0.0))) / margin;
    let fall = 1.0 - smoothstep(0.0, 1.0, d);
    let a = fall * fall * strength;
    return vec4(vec3(1.0 - a), 1.0);
}
