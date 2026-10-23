// A cockpit monitor's face: its region of the instruments' texture (rendered by the UI each
// frame), glowing, as a pane of glass (`docs/UI.md`): see-through, more or less (`pane.x`: the
// framed cockpit's glass is mostly dark, the holo cockpit's barely there), what it shows lit over
// whatever is behind it; faint scan lines, a soft sheen across it, its colours fringing a little
// towards the edges, a lit rim and a darker band inside it. Coming into view it powers on, a bright
// line sweeping down it. A hit makes it flicker; with the head's main camera gone, it goes grey and
// static bands roll through it.
//
// Drawn with premultiplied alpha (`AlphaMode::Premultiplied`): what's returned is the light added
// over the world, and how much of the world behind it the glass hides.

#import bevy_pbr::forward_io::VertexOutput
#import bc::glow::glow_out

struct Monitor {
    // x: seconds; y: static (0..1); z: flicker (0..1); w: brightness.
    params: vec4<f32>,
};

struct Look {
    // x: seconds since the monitors came on; yzw: the glass's tint (linear), its rim and sheen.
    look: vec4<f32>,
};

struct Pane {
    // x: how much of what's behind the glass it hides (0 clear, 1 opaque).
    pane: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var screen: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var screen_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> monitor: Monitor;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> glass: Look;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> pane: Pane;

fn hash(x: f32) -> f32 {
    return fract(sin(x * 12.9898) * 43758.547);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = vec2(0.5);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    // The face's own coordinates (0,0 its top left), for its rim, its sheen and its sweep.
    var face = uv;
#ifdef VERTEX_UVS_B
    face = in.uv_b;
#endif
    let t = monitor.params.x;
    let noise = monitor.params.y;
    let tint = glass.look.yzw;
    // The picture, its red and blue drawn a hair apart towards the edges (a lens's fringe).
    let out = (face - 0.5) * 0.0024;
    var c = vec3(
        textureSample(screen, screen_sampler, uv + out).r,
        textureSample(screen, screen_sampler, uv).g,
        textureSample(screen, screen_sampler, uv - out).b,
    );
    // The panel's own dark glass under what it shows.
    let dark = vec3(0.012, 0.02, 0.03);
    c = max(c, dark);
    // Scan lines, a darker band inside the rim, and the rim itself lit.
    let lines = 0.9 + 0.1 * sin(in.position.y * 1.8);
    let edge = min(min(face.x, 1.0 - face.x), min(face.y, 1.0 - face.y));
    let inner = smoothstep(0.0, 0.05, edge);
    c *= lines * (0.6 + 0.4 * inner);
    c += tint * 0.08 * (1.0 - smoothstep(0.0, 0.006, edge));
    // The glass: lit a little from above, and a soft band of sheen across it.
    c += tint * 0.012 * (1.0 - face.y);
    let band = face.x * 0.8 - face.y + 0.1;
    c += tint * 0.03 * (1.0 - smoothstep(0.0, 0.18, abs(band)));
    // Static: grey, with bands rolling through.
    if (noise > 0.0) {
        let grey = dot(c, vec3(0.3, 0.59, 0.11));
        let roll = step(0.8, hash(floor(face.y * 40.0) + floor(t * 12.0)));
        let snow = hash(face.x * 91.7 + face.y * 57.3 + floor(t * 30.0));
        c = mix(c, vec3(grey * 0.8 + snow * 0.12 + roll * 0.2), noise);
    }
    c *= 1.0 - monitor.params.z * 0.5 * step(0.5, hash(floor(t * 20.0)));
    // Powering on: nothing below the sweep, a bright line at it, a flicker as it settles.
    let on = glass.look.x;
    if (on < 0.6) {
        let sweep = on / 0.45;
        let shown = step(face.y, sweep);
        let front = 1.0 - smoothstep(0.0, 0.025, abs(face.y - sweep));
        let settle = 0.75 + 0.25 * step(0.4, hash(floor(on * 40.0)));
        c = c * shown * settle + tint * 1.5 * front;
    }
    // As much of the dark glass as hides the world, and what it shows over its dark as light.
    let a = pane.pane.x;
    let lit = c * a + max(c - dark, vec3(0.0)) * (1.0 - a);
    return vec4(glow_out(lit * monitor.params.w), a);
}
