// The HUD's panels (`ui_panel.rs`), in the page's look (`docs/UI.md`): dark glass with fine
// scanlines and four corner ticks where a box would have its border (mode 0); or hazard stripes,
// for a caution (mode 1); or a vital's arc beside the crosshair (mode 2). The corners can still be
// cut (the chamfer), for a plate that wants it.

#import bevy_ui::ui_vertex_output::UiVertexOutput

struct Panel {
    // Linear colours, alpha straight: the glass, and its ticks (the stripes' two colours).
    fill: vec4<f32>,
    edge: vec4<f32>,
    // x: chamfer (px); y: mode (0 glass, 1 stripes, 2 an arc); z: the ticks' width (px); w: their
    // length (px). (An arc's are its own: below.)
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
    var inside = sides;
    if (c > 0.0) {
        let top_right = (p.y - (p.x - (s.x - c))) * 0.7071;
        let bottom_left = ((s.y - p.y) - (c - p.x)) * 0.7071;
        inside = min(sides, min(top_right, bottom_left));
    }
    let cover = clamp(inside + 0.5, 0.0, 1.0);
    if (panel.params.y > 1.5) {
        // A vital's gauge beside the crosshair (mode 2, `hud::update_vitals`): a quarter of a ring on
        // one side of the node's middle (z: -1 left, +1 right), ten segments lit from the bottom up to
        // x of it, w px thick; lit in `fill`, the rest a track in `edge`.
        let d = p - s * 0.5;
        let th = panel.params.w;
        let radius = min(s.x, s.y) * 0.5 - th;
        let out = d.x * panel.params.z;
        let half = 0.7854;
        let u = (half - atan2(d.y, out)) / (2.0 * half);
        let on_ring = clamp(0.5 - (abs(length(d) - radius) - th * 0.5), 0.0, 1.0);
        let in_span = step(0.0, out) * step(0.0, u) * step(u, 1.0);
        let gap = step(fract(u * 10.0), 0.82);
        let arc = select(panel.edge, panel.fill, u <= panel.params.x);
        return vec4(arc.rgb, arc.a * on_ring * in_span * gap);
    }
    if (panel.params.y > 0.5) {
        let k = fract((p.x + p.y) / 22.0);
        let col = select(panel.fill, panel.edge, k < 0.5);
        return vec4(col.rgb, col.a * cover);
    }
    // The glass, a scanline every third pixel (the page's `--scan`).
    var col = panel.fill;
    let line = 1.0 - step(1.0, fract(p.y / 3.0) * 3.0);
    col = vec4(col.rgb + vec3(0.012, 0.020, 0.028) * line, col.a);
    // The ticks: the outermost `w` px of the outline, within `len` px of a corner.
    let w = panel.params.z;
    let len = panel.params.w;
    let near_x = min(p.x, s.x - p.x) < len;
    let near_y = min(p.y, s.y - p.y) < len;
    let on_x_edge = min(p.y, s.y - p.y) < w && near_x;
    let on_y_edge = min(p.x, s.x - p.x) < w && near_y;
    if (on_x_edge || on_y_edge) {
        col = panel.edge;
    }
    return vec4(col.rgb, col.a * cover);
}
