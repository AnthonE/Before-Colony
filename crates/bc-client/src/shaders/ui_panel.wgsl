// The HUD's panels (`ui_panel.rs`): a translucent plate with two corners cut off and a thin edge,
// as the mobile-suit monitor draws its readouts; or (mode 1) hazard stripes, for a caution.

#import bevy_ui::ui_vertex_output::UiVertexOutput

struct Panel {
    // Linear colours, alpha straight.
    fill: vec4<f32>,
    edge: vec4<f32>,
    // x: chamfer (px); y: mode (0 a panel, 1 stripes); z: edge width (px).
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> panel: Panel;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let s = in.size;
    let p = in.uv * s;
    let c = panel.params.x;
    // Distance inside the outline (px): the four sides, and the cut top-right and bottom-left corners.
    let sides = min(min(p.x, s.x - p.x), min(p.y, s.y - p.y));
    let top_right = (p.y - (p.x - (s.x - c))) * 0.7071;
    let bottom_left = ((s.y - p.y) - (c - p.x)) * 0.7071;
    let inside = min(sides, min(top_right, bottom_left));
    let cover = clamp(inside + 0.5, 0.0, 1.0);
    if (panel.params.y > 0.5) {
        let k = fract((p.x + p.y) / 22.0);
        let col = select(panel.fill, panel.edge, k < 0.5);
        return vec4(col.rgb, col.a * cover);
    }
    let w = panel.params.z;
    let edge = 1.0 - smoothstep(w - 0.5, w + 0.5, inside);
    // A little lighter at the top, like a lit glass plate.
    let sheen = 1.0 + 0.25 * (1.0 - in.uv.y);
    var col = vec4(panel.fill.rgb * sheen, panel.fill.a);
    col = mix(col, panel.edge, edge);
    return vec4(col.rgb, col.a * cover);
}
