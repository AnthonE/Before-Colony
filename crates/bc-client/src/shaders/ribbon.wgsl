// A ribbon along an entity's local +Y, turned about that axis to face the camera: beams, tracers
// and thruster plumes. The mesh is a strip with x in {-1, 1} (across) and y in [0, 1] (along);
// the entity's transform gives the ribbon's start (translation), direction and length (+Y
// column) and half-width (+X column's length).
#define_import_path bc::ribbon

#import bevy_pbr::{mesh_functions::{get_world_from_local, get_tag}, mesh_view_bindings::view}

struct RibbonOut {
    @builtin(position) clip: vec4<f32>,
    // x: across (-1..1); y: along (0 at the start, 1 at the end).
    @location(0) uv: vec2<f32>,
    // The entity's MeshTag, for per-instance parameters.
    @location(1) @interpolate(flat) tag: u32,
    // x: half-width; y: length (m).
    @location(2) @interpolate(flat) size: vec2<f32>,
};

fn ribbon_vertex(instance: u32, position: vec3<f32>) -> RibbonOut {
    var out: RibbonOut;
    let m = get_world_from_local(instance);
    let axis = m[1].xyz;
    let dir = normalize(axis);
    let half_width = length(m[0].xyz);
    let on_axis = m[3].xyz + axis * position.y;
    let to_cam = view.world_position - on_axis;
    // Seen end-on the side is undefined: lean on camera-right so it never goes NaN.
    let side = normalize(cross(dir, to_cam) + view.world_from_view[0].xyz * (1e-3 * length(to_cam)));
    let world = on_axis + side * position.x * half_width;
    out.clip = view.clip_from_world * vec4(world, 1.0);
    out.uv = position.xy;
    out.tag = get_tag(instance);
    out.size = vec2(half_width, length(axis));
    return out;
}

// The ribbon's width at this fragment, as a fraction of its half-width: 1 along the body, closing
// to 0 over a semicircular cap at the head (y = 1).
fn ribbon_cap(in: RibbonOut) -> f32 {
    let cap = min(in.size.x / max(in.size.y, 1e-3), 0.5);
    let u = clamp((in.uv.y - (1.0 - cap)) / cap, 0.0, 1.0);
    return sqrt(1.0 - u * u);
}
