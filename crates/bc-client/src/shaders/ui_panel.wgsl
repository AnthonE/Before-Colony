// The HUD's panels (`ui_panel.rs`), in the page's look (`docs/UI.md`): dark glass with fine
// scanlines and four corner ticks where a box would have its border (mode 0); or hazard stripes,
// for a caution (mode 1); or a vital's arc beside the crosshair (mode 2); or an instrument's gauge
// of lit segments, a dial's ring (mode 3) or a bar (mode 4). The corners can still be cut (the
// chamfer), for a plate that wants it.

#import bevy_ui::ui_vertex_output::UiVertexOutput

struct Panel {
    // Linear colours, alpha straight: the glass, and its ticks (the stripes' two colours).
    fill: vec4<f32>,
    edge: vec4<f32>,
    // x: chamfer (px); y: mode (0 glass, 1 stripes, 2 an arc, 3 a dial, 4 a bar); z: the ticks'
    // width (px); w: their length (px). (An arc's and a gauge's are their own: below.)
    params: vec4<f32>,
    // A gauge's: x its mark (0..1, below 0 none), y where its red begins (0..1, past 1 none).
    more: vec4<f32>,
    // A gauge's red.
    alert: vec4<f32>,
};

// A gauge's segment `k` of `n` (its middle at `u`): lit to `params.x` in `fill`, else its track in
// `edge`; past `more.y` red, lit or not.
fn segment(k: f32, n: f32) -> vec4<f32> {
    let u = (k + 0.5) / n;
    let lit = u < panel.params.x;
    let red = u > panel.more.y;
    if (lit) {
        return select(panel.fill, panel.alert, red);
    }
    return select(panel.edge, vec4(panel.alert.rgb, panel.edge.a * 1.6), red);
}

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
    let mark = panel.more.x;
    if (panel.params.y > 3.5) {
        // A bar (mode 4): z segments left to right, each with a gap after it a fifth of its width.
        let n = panel.params.z;
        let w = s.x / n;
        let k = floor(p.x / w);
        let gap = max(1.0, w * 0.2);
        let on = step(p.x - k * w, w - gap);
        var col = segment(k, n);
        col.a *= on;
        // The mark: a line across it.
        if (mark >= 0.0 && abs(p.x - mark * s.x) < 0.75) {
            col = vec4(panel.edge.rgb, 0.95);
        }
        return col;
    }
    if (panel.params.y > 2.5) {
        // A dial (mode 3): a ring of z segments three quarters round, from the bottom left over the
        // top to the bottom right, a fine ring inside it, and the mark a tick across both.
        let d = p - s * 0.5;
        let outer = min(s.x, s.y) * 0.5 - 1.0;
        let th = outer * 0.16;
        let r = outer - th * 0.5 - outer * 0.08;
        let len = length(d);
        let span = 4.712389;
        var a = atan2(-d.x, d.y);
        if (a < 0.0) {
            a += 6.2831853;
        }
        let u = (a - (6.2831853 - span) * 0.5) / span;
        let in_span = step(0.0, u) * step(u, 1.0);
        let n = panel.params.z;
        let k = floor(u * n);
        // Gaps a fifth of a segment, as a bar's.
        let gap = step(fract(u * n), 0.8);
        let on_ring = clamp(0.5 - (abs(len - r) - th * 0.5), 0.0, 1.0);
        var col = segment(k, n);
        col.a *= on_ring * in_span * gap;
        // The fine ring inside, the track's colour, faint.
        let fine = clamp(0.5 - (abs(len - (r - th * 0.5 - outer * 0.1)) - 0.5), 0.0, 1.0) * in_span;
        col = mix(col, vec4(panel.edge.rgb, panel.edge.a * 1.4), fine * (1.0 - col.a));
        // The mark, across the ring and out past it.
        if (mark >= 0.0) {
            let at = (6.2831853 - span) * 0.5 + mark * span;
            let dir = vec2(-sin(at), cos(at));
            let along = dot(d, dir);
            let across = abs(d.x * dir.y - d.y * dir.x);
            if (across < 0.9 && along > r - th && along < outer) {
                col = vec4(panel.edge.rgb, 0.95);
            }
        }
        return col;
    }
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
