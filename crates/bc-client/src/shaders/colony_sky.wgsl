// The air and the light inside the colony (`docs/COLONY_LOOK.md`, pass 1.1): one sky function for
// everything drawn inside (`city.wgsl`, `city_inside.wgsl`), in the colony's own frame: x along the
// axis (−16,000 to 16,000 m), the axis at y = z = 0, the floor at radius 3,200 m, up towards the axis.
//
// - **The haze.** The air itself thins only about 17% from the floor to the axis, but haze isn't
//   air: dust, moisture and the city's exhaust sit in a mixed layer over the floor, as aerosols do
//   on Earth. Its density falls off with height above the floor, ρ(h) = ρ₀·exp(−h/H) with
//   h = R − r and H about a kilometre, and the air's own thin, even Rayleigh term (blue) lies on
//   top. Along a segment it's integrated in closed form: split where the segment comes closest to
//   the axis (where h peaks), each side in two runs whose height is taken as linear, on which the
//   exponential's integral is exact. That's within about 6% of the true optical depth for any
//   segment in the colony (transmittance within 0.02). A street's distances go blue and soft; a
//   look across at the other strips spends most of its 6 km in the clean core, so the city
//   overhead stays legible.
// - **Its light.** Each window throws in one beam, the mirrors' sun: a slab as wide as the window,
//   across the colony to the strip opposite (that strip's key light). The haze glows with an even
//   share of the colony's light (blue-white by day, deep blue by night) plus the beams it lies in,
//   scattered forward (Mie, Henyey-Greenstein): bright looking into the light, warm when the light
//   is low. The in-scatter is summed over four runs of the segment, each glowing with the beams at
//   its middle: within a few per cent of a fine march for most rays, 20% at worst (an eye high in
//   the core looking down the axis into a low sun).
// - **The sky.** Along a ray that leaves the air: the glass (the mirrors' glow by day, space by
//   night), the far city over a strip (its average colour, lit by its own sun and the sky, its lights
//   by night), or an end cap, all seen through the haze. Low-frequency only, and cheap: for
//   reflections on glass and water.
//
// All of it is plain arithmetic: no textures and no derivatives, so any of it may be called from
// any branch (the city's shader branches on its non-uniform surface id). Radiances are nits,
// unexposed: multiply by `view.exposure`. Nothing here knows the city's layout: the far city is a
// strip's average, the strips' and windows' angles are the colony's frame (`colony::frame`).
#define_import_path bc::colony_sky

#import bc::noise::noise3

// Everything the sky needs, by the hour (`city.rs`'s colour script fills it; vec4s only, for
// WebGL2's uniform layout).
struct Sky {
    // rgb: the haze's even glow, what a deep enough haze shows looking across the beams (nits);
    // w: its density at the floor (/m, for green).
    haze: vec4<f32>,
    // rgb: the beams' light the haze scatters, as an even phase would (nits; times the Mie phase:
    // about 19 looking straight into the light, 0.1 with it behind); w: the Mie asymmetry g.
    mie: vec4<f32>,
    // rgb: the haze's extinction by channel, relative to its density (green 1); w: its scale height
    // above the floor (m).
    ext: vec4<f32>,
    // rgb: the air's own (Rayleigh) extinction, even all through the colony (/m); w: unused.
    ray: vec4<f32>,
    // rgb: the key light, colour times illuminance on a surface square to it (lux), the same on every
    // strip; w: its elevation above the floor (rad, `Day::sun_elev`).
    key: vec4<f32>,
    // rgb: the sky's light on a surface facing up (nits): the haze, the strips overhead and the
    // windows together; w: daylight 0..1.
    ambient: vec4<f32>,
    // rgb: the windows' glass by day seen from inside, the mirrors' light (nits); w: space through it
    // by night, the stars averaged (nits).
    glow: vec4<f32>,
    // rgb: the city's lit windows and streets seen from afar, averaged (nits, lamps already in);
    // w: lamps lit 0..1.
    night: vec4<f32>,
    // x: the colony's spin angle (rad); y: seconds (wrapped); z: 1 to trace reflections, 0 for the
    // cheap stand-in (the Low tier); w: unused.
    clock: vec4<f32>,
};

// The air between two points: what's left of the light from the far one, and what the air adds.
struct Haze {
    transmittance: vec3<f32>,
    // Nits, unexposed.
    inscatter: vec3<f32>,
};

// `world::COLONY_RADIUS`, `COLONY_HALF_LENGTH`; `colony::frame::FIRST_WINDOW`, `STRIP_ARC`.
const SKY_R: f32 = 3200.0;
const SKY_HALF_LENGTH: f32 = 16000.0;
const SKY_FIRST_WINDOW: f32 = 0.4;
const SKY_ARC: f32 = 1.0471976;
// A window and a strip: 120°.
const SKY_SECTOR: f32 = 2.0943951;
const SKY_TAU: f32 = 6.2831853;
const SKY_PI: f32 = 3.14159265;
// Up at the middle of each land strip, −(0, cos a, sin a) with a = `frame::strip_centre(k)`: towards
// the axis, and on to the window over it.
const SKY_UP0: vec3<f32> = vec3<f32>(0.0, -0.12328432, -0.99237139);
const SKY_UP1: vec3<f32> = vec3<f32>(0.0, 0.92106099, 0.38941834);
const SKY_UP2: vec3<f32> = vec3<f32>(0.0, -0.79777667, 0.60295309);
// A beam's half width (the window's chord, R·sin 30°) and how softly its edge is drawn, m.
const BEAM_HALF: f32 = 1600.0;
const BEAM_SOFT: f32 = 250.0;
// The share of a far strip's sunlight that what's seen of it from afar returns: ground and roofs
// between their shadows.
const CANOPY: f32 = 0.75;
// Each strip from afar, averaged (ground, roofs, trees, water; `STORY.md`): Charter's stone, glass
// and civic white; Canal's brick, corrugated steel, rust and water; Gardens' greens, render and
// timber. Keep them near what `city.wgsl` paints the strips.
const CHARTER_ALBEDO: vec3<f32> = vec3<f32>(0.3, 0.31, 0.32);
const CANAL_ALBEDO: vec3<f32> = vec3<f32>(0.25, 0.21, 0.18);
const GARDENS_ALBEDO: vec3<f32> = vec3<f32>(0.15, 0.22, 0.1);
// An end cap's terraces.
const CAP_ALBEDO: vec3<f32> = vec3<f32>(0.42, 0.43, 0.45);
// The streets, yards and parks, averaged, for the light they throw back up.
const GROUND_ALBEDO: f32 = 0.2;

// The 120° sector a point is in, counted from window 0's near edge (x: k, 0..2), and how far round
// it is (y: rad, 0..2π/3): window k for the first π/3, land strip k for the rest. As
// `frame::from_colony`.
fn arc_at(p: vec3<f32>) -> vec2<f32> {
    let rel = atan2(p.z, p.y) - (SKY_FIRST_WINDOW - SKY_ARC * 0.5);
    let a = rel - floor(rel / SKY_TAU) * SKY_TAU;
    let k = min(floor(a / SKY_SECTOR), 2.0);
    return vec2(k, a - k * SKY_SECTOR);
}

// Which land strip a point is over (0..2), or −1 over a window.
fn strip_at(p: vec3<f32>) -> f32 {
    let s = arc_at(p);
    return select(s.x, -1.0, s.y < SKY_ARC);
}

// Height above the floor, m (negative under it: the canal's water, the glass outside the hull).
fn height_at(p: vec3<f32>) -> f32 {
    return SKY_R - length(p.yz);
}

// Up (towards the axis) at a point off the axis.
fn up_at(p: vec3<f32>) -> vec3<f32> {
    let r = max(length(p.yz), 1e-3);
    return vec3(0.0, -p.y / r, -p.z / r);
}

// Up at the middle of land strip `k` (any k: it comes round every 3), the way to the window over it.
fn strip_up(k: f32) -> vec3<f32> {
    let a = SKY_FIRST_WINDOW + SKY_ARC + k * SKY_SECTOR;
    return -vec3(0.0, cos(a), sin(a));
}

// The way to the key light from land strip `k`: up from it, leaning to +X by the sun's elevation
// (`colony::time::key_light`).
fn key_dir(k: f32, sky: Sky) -> vec3<f32> {
    let e = sky.key.w;
    return vec3(cos(e), 0.0, 0.0) + strip_up(k) * sin(e);
}

// Which way the light runs through the air at `p`, towards its source: over strip k its own key
// light; over window w the beam it lets in, bound for strip w + 1.
fn light_dir_at(p: vec3<f32>, sky: Sky) -> vec3<f32> {
    let s = arc_at(p);
    return key_dir(select(s.x, s.x + 1.0, s.y < SKY_ARC), sky);
}

// Henyey-Greenstein, scaled so that an even phase is 1: (1 − g²) / (1 + g² − 2g·cos θ)^1.5.
fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let d = max(1.0 + g * g - 2.0 * g * cos_theta, 1e-4);
    return (1.0 - g * g) / (d * sqrt(d));
}

// How far into each beam a point is, 0..1 (x: the beam that lands on strip 0, y: strip 1, z: strip 2),
// from its `yz`: each beam a slab within half a window's width of the diameter through its strip's
// middle (and the window over it). One near the floor, all three in the core.
fn beam_covers(yz: vec2<f32>) -> vec3<f32> {
    let off = abs(vec3(
        yz.x * SKY_UP0.z - yz.y * SKY_UP0.y,
        yz.x * SKY_UP1.z - yz.y * SKY_UP1.y,
        yz.x * SKY_UP2.z - yz.y * SKY_UP2.y,
    ));
    return 1.0 - smoothstep(vec3(BEAM_HALF - BEAM_SOFT), vec3(BEAM_HALF + BEAM_SOFT), off);
}

// Each beam's Mie phase for an eye looking along `dir` (as `beam_covers`): its light runs from the
// window towards the strip, so looking towards the window over a strip is looking into its beam.
fn beam_phases(dir: vec3<f32>, sky: Sky) -> vec3<f32> {
    let g = sky.mie.w;
    let x = cos(sky.key.w) * dir.x;
    let s = sin(sky.key.w);
    return vec3(
        mie_phase(x + s * dot(SKY_UP0, dir), g),
        mie_phase(x + s * dot(SKY_UP1, dir), g),
        mie_phase(x + s * dot(SKY_UP2, dir), g),
    );
}

// The beams' light the air at `p` scatters towards an eye looking along `dir` (the Mie phase, summed
// over the beams that cross there).
fn beams(p: vec3<f32>, dir: vec3<f32>, sky: Sky) -> f32 {
    return dot(beam_covers(p.yz), beam_phases(dir, sky));
}

// ∫ exp(−h/H) dl along a run of length `len` whose height goes linearly from h0 to h1, given
// e0 = exp(−h0/H), e1 = exp(−h1/H) and x = (h1 − h0)/H.
fn haze_run(len: f32, e0: f32, e1: f32, x: f32) -> f32 {
    let level = abs(x) < 1e-3;
    return len * select((e0 - e1) / select(x, 1.0, level), 0.5 * (e0 + e1), level);
}

// One more run of air beyond what's been crossed so far: `depth` its ∫ exp(−h/H) dl, `len` its length
// (m), `phase` the beams' phase at its middle. Only the haze's share of its glow scatters forward.
fn haze_step(sofar: Haze, depth: f32, len: f32, phase: f32, sky: Sky) -> Haze {
    let aer = sky.ext.rgb * (sky.haze.w * depth);
    let tau = aer + sky.ray.rgb * len;
    let t = exp(-tau);
    let glow = sky.haze.rgb + sky.mie.rgb * (phase * aer / max(tau, vec3(1e-6)));
    return Haze(sofar.transmittance * t, sofar.inscatter + sofar.transmittance * glow * (1.0 - t));
}

// The air on the segment from `a` to `b` (points of the colony's frame), seen from `a`.
fn haze(a: vec3<f32>, b: vec3<f32>, sky: Sky) -> Haze {
    let ab = b - a;
    let len = length(ab);
    let dir = ab / max(len, 1e-3);
    // Where the segment comes closest to the axis, as a share of the way: its height peaks there.
    // Four runs: from a halfway to the peak, on to the peak, halfway on, and on to b.
    let q = dot(ab.yz, ab.yz);
    let t2 = clamp(-dot(a.yz, ab.yz) / max(q, 1e-6), 0.0, 1.0);
    let t1 = 0.5 * t2;
    let t3 = 0.5 + 0.5 * t2;
    let inv_h = 1.0 / max(sky.ext.w, 1.0);
    let h0 = SKY_R - length(a.yz);
    let h1 = SKY_R - length(a.yz + ab.yz * t1);
    let h2 = SKY_R - length(a.yz + ab.yz * t2);
    let h3 = SKY_R - length(a.yz + ab.yz * t3);
    let h4 = SKY_R - length(b.yz);
    let e0 = exp(-h0 * inv_h);
    let e1 = exp(-h1 * inv_h);
    let e2 = exp(-h2 * inv_h);
    let e3 = exp(-h3 * inv_h);
    let e4 = exp(-h4 * inv_h);
    let la = t1 * len;
    let lb = (0.5 - t1) * len;
    // Each run glows with the beams at its middle (the phases are the ray's, the same all along).
    let phases = beam_phases(dir, sky);
    var air = Haze(vec3(1.0), vec3(0.0));
    air = haze_step(air, haze_run(la, e0, e1, (h1 - h0) * inv_h), la, dot(beam_covers(a.yz + ab.yz * (0.5 * t1)), phases), sky);
    air = haze_step(air, haze_run(la, e1, e2, (h2 - h1) * inv_h), la, dot(beam_covers(a.yz + ab.yz * (t1 + 0.5 * t1)), phases), sky);
    air = haze_step(air, haze_run(lb, e2, e3, (h3 - h2) * inv_h), lb, dot(beam_covers(a.yz + ab.yz * (0.5 * (t2 + t3))), phases), sky);
    air = haze_step(air, haze_run(lb, e3, e4, (h4 - h3) * inv_h), lb, dot(beam_covers(a.yz + ab.yz * (0.5 * (t3 + 1.0))), phases), sky);
    return air;
}

// What `haze` does to a colour seen at `b` from `a` (both nits, or both exposed if `exposure` is
// the view's; pass 1.0 for nits).
fn apply_haze(col: vec3<f32>, a: vec3<f32>, b: vec3<f32>, exposure: f32, sky: Sky) -> vec3<f32> {
    let h = haze(a, b, sky);
    return col * h.transmittance + h.inscatter * exposure;
}

// Each strip from afar.
fn strip_albedo(k: f32) -> vec3<f32> {
    if (k < 0.5) {
        return CHARTER_ALBEDO;
    }
    if (k < 1.5) {
        return CANAL_ALBEDO;
    }
    return GARDENS_ALBEDO;
}

// The light the ground of land strip `k` throws back up (nits, to multiply by a surface's albedo and
// occlusion): its sunlit and skylit streets, on whatever faces sideways or down. Walls in shade at
// noon are lit mostly by it, not by the sky. `n`: the surface's unit normal.
fn bounce_light(n: vec3<f32>, k: f32, sky: Sky) -> vec3<f32> {
    let up = strip_up(k);
    let ground = GROUND_ALBEDO * (sky.key.rgb * (max(dot(up, key_dir(k, sky)), 0.0) / SKY_PI) + sky.ambient.rgb);
    return ground * (0.5 - 0.5 * dot(n, up));
}

// How a surface of land strip `k` is lit (nits, before its ambient occlusion) where the scene's one
// light isn't its sun: that strip's own key light, its sky from above and its ground's bounce from
// below. `n`: the surface's unit normal.
fn own_light(albedo: vec3<f32>, n: vec3<f32>, k: f32, sky: Sky) -> vec3<f32> {
    let sun = sky.key.rgb * (max(dot(n, key_dir(k, sky)), 0.0) / SKY_PI);
    let fill = sky.ambient.rgb * (0.6 + 0.4 * dot(n, strip_up(k)));
    return albedo * (sun + fill + bounce_light(n, k, sky));
}

// Schlick's Fresnel: how much a smooth surface reflects at `cos_v` (the cosine between the view and
// its normal), `f0` face on (0.02 water, 0.04 glass).
fn fresnel(cos_v: f32, f0: f32) -> f32 {
    let c = 1.0 - clamp(cos_v, 0.0, 1.0);
    let c2 = c * c;
    return f0 + (1.0 - f0) * c2 * c2 * c;
}

// Land strip `k` from afar at `p` on its floor (`across`: 0..1 over the strip): its average ground
// and roofs, lit by its own sun and the sky, and its lights by night, brighter in some districts.
// `detail` (0..1) fades the districts out where they'd be smaller than a pixel (far down the axis).
fn far_city(p: vec3<f32>, k: f32, across: f32, detail: f32, sky: Sky) -> vec3<f32> {
    let sun = sky.key.rgb * (max(dot(up_at(p), key_dir(k, sky)), 0.0) * CANOPY / SKY_PI);
    let district = mix(0.5, noise3(vec3(p.x * 0.0011, across * 4.0, k * 3.7)), detail);
    let albedo = strip_albedo(k) * (0.85 + 0.3 * district);
    return albedo * (sun + sky.ambient.rgb) + sky.night.rgb * (0.3 + 1.4 * district);
}

// A window's glass from inside (`across`: 0..1 over it): the mirrors' light by day, palest across its
// middle and a little shaded by its frame; space by night.
fn window_glow(across: f32, sky: Sky) -> vec3<f32> {
    let mid = abs(across - 0.5) * 2.0;
    return sky.glow.rgb * (0.66 + 0.28 * (1.0 - mid)) + sky.glow.w * vec3(0.75, 0.82, 1.0);
}

// An end cap's inner face (`dx`: which way the ray ran along the axis): its terraces, lit by the
// beams running down the colony when the light is low, and their windows by night. (Its axis port
// is left to `city.wgsl`: from afar it would only be a dark spot at the vanishing point.)
fn cap_glow(dx: f32, sky: Sky) -> vec3<f32> {
    // The cap faces back into the colony; every beam leans the same way along the axis.
    let facing = max(-sign(dx) * cos(sky.key.w), 0.0);
    let sun = sky.key.rgb * (facing * CANOPY / SKY_PI);
    return CAP_ALBEDO * (sun + sky.ambient.rgb) + sky.night.rgb * 0.6;
}

// The light seen from `o` along unit `d` as far as the ray leaves the air (the hull or an end cap),
// through the haze on the way. `o` is a point of the colony's frame, anywhere inside (or just under
// the floor, like the canal's water).
fn sky_radiance(o: vec3<f32>, d: vec3<f32>, sky: Sky) -> vec3<f32> {
    // The hull, radius R: the far root of |o + t·d|² = R² across the axis, in whichever form keeps
    // its digits for the way the ray runs.
    let qa = dot(d.yz, d.yz);
    let qb = dot(o.yz, d.yz);
    let qc = dot(o.yz, o.yz) - SKY_R * SKY_R;
    let s = sqrt(max(qb * qb - qa * qc, 0.0));
    var t = select((s - qb) / max(qa, 1e-12), -qc / max(qb + s, 1e-6), qb > 0.0);
    t = select(t, 1e9, qa < 1e-10);
    // The end caps.
    let along = abs(d.x) > 1e-6;
    let tx = (sign(d.x) * SKY_HALF_LENGTH - o.x) / select(1.0, d.x, along);
    let cap = along && tx < t;
    t = max(select(t, tx, cap), 0.0);
    let p = o + d * t;
    var col = vec3(0.0);
    if (cap) {
        col = cap_glow(d.x, sky);
    } else {
        let arc = arc_at(p);
        if (arc.y < SKY_ARC) {
            col = window_glow(arc.y / SKY_ARC, sky);
        } else {
            col = far_city(p, arc.x, (arc.y - SKY_ARC) / SKY_ARC, 1.0 - smoothstep(3000.0, 9000.0, t), sky);
        }
    }
    let h = haze(o, p, sky);
    return col * h.transmittance + h.inscatter;
}

// What glass or water at `p` shows of the sky: the sky function along the view's reflection,
// weighted by Fresnel (Schlick; `f0` the reflectance face on: 0.02 water, 0.04 glass). `v`: unit,
// towards the eye; `n`: the surface's unit normal. A reflection that would dive into the floor (a
// ripple's slope seen at a glancing angle) is bent up to skim it. With reflections off
// (`sky.clock.z`, the Low tier), the haze's and the windows' light stand in, untraced.
fn sky_reflection(p: vec3<f32>, v: vec3<f32>, n: vec3<f32>, f0: f32, sky: Sky) -> vec3<f32> {
    if (sky.clock.z < 0.5) {
        return (sky.haze.rgb + sky.glow.rgb * 0.25) * fresnel(dot(v, n), f0);
    }
    let up = up_at(p);
    var r = reflect(-v, n);
    r = normalize(r + up * max(0.02 - dot(r, up), 0.0));
    return sky_radiance(p, r, sky) * fresnel(dot(v, n), f0);
}

// The mirrors' sun in a reflection along unit `r`, for surfaces the scene's light doesn't reach (the
// far strips; on the camera's strip Bevy's lighting draws the highlight): a normalised lobe round
// `kd`, the key light's direction there (`light_dir_at`). `sharpness`: about 400 for glass, 60 for
// rippled water. Multiply by the surface's Fresnel. Capped, so a far pane can flare but not sparkle.
fn sun_glint(r: vec3<f32>, kd: vec3<f32>, sharpness: f32, sky: Sky) -> vec3<f32> {
    let c = max(dot(r, kd), 1e-4);
    return sky.key.rgb * min(pow(c, sharpness) * (sharpness + 2.0) / (2.0 * SKY_PI), 8.0);
}
