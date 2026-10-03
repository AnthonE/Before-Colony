//! The colony's colour script (`docs/COLONY_LOOK.md`, passes 1.1 and 1.4): the light, the air and the
//! picture inside at every hour, from one table, so the hours are tuned in one place.
//!
//! - **The hour is read from the mirrors.** The light's height (`Day::sun_elev`) sets its colour:
//!   gold when it's low, pale at noon; the morning's keys and the evening's differ (rose dawn and
//!   cool gold, amber afternoon and magenta dusk), split at noon by `Day::phase`. How much light
//!   there is follows what the mirrors throw in (`Day::daylight`), whatever shape the day's curve
//!   takes. Night is where the daylight runs out: its own deep blue, warm windows and the lamps'
//!   glow in the haze.
//! - **Physical units.** Lux for the sun, nits for the sky and the haze (unexposed), EV100 for the
//!   camera, which meters the light as an eye would and then leans darker at the golden hour and
//!   dusk to keep their colour.
//! - **What it drives** (`city::light_city`): the Sun and the ambient, the sky function's uniform
//!   ([`Sky`], `shaders/colony_sky.wgsl`), the camera's exposure, its `DistanceFog` (matched to the
//!   haze near the floor, for what the city's own shader doesn't draw: people, cars, trams), its
//!   grade and its bloom (HDR tiers; the Low tier has neither HDR nor bloom, and grades in its
//!   shaders).

use bc_sim::colony::time::Day;
use bevy::color::Color;
use bevy::math::{Vec3, Vec4};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::render::render_resource::ShaderType;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};

/// Sunlight through the mirrors at noon, lux.
pub const NOON_LUX: f32 = 62_000.0;
/// The sky's light at noon, nits: the haze, the strips overhead and the windows' glow together.
pub const NOON_SKY: f32 = 3_000.0;
/// The camera at noon and at night, EV100.
pub const EV_NOON: f32 = 14.5;
pub const EV_NIGHT: f32 = 8.5;
/// Where noon falls in the day (`Day::phase`; `colony::time`'s dawn plus half its day, which is half
/// its light, dawn to dusk, in pass 1.0's reshaped day too: tick 36,000 of 86,400): the morning's
/// keys before it, the evening's after. Both meet at noon, so the seam doesn't show.
pub const NOON_PHASE: f32 = (7_200.0 + 28_800.0) / 86_400.0;
/// The air's own extinction, even all through the colony: Rayleigh at one atmosphere (/m, at 680,
/// 550 and 440 nm).
pub const RAYLEIGH: Vec3 = Vec3::new(5.8e-6, 13.5e-6, 33.1e-6);
/// The haze's extinction by channel, relative to its density (aerosols: nearly grey).
pub const HAZE_EXT: Vec3 = Vec3::new(0.9, 1.0, 1.12);
/// Below this much daylight the night's look takes over.
const DUSKFALL: f32 = 0.03;

/// The sky function's uniform (`bc::colony_sky::Sky`): see the shader for what each field holds.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct Sky {
    /// rgb: the haze's even glow (nits); w: its density at the floor (/m).
    pub haze: Vec4,
    /// rgb: the beams' light the haze scatters (nits, for an even phase); w: the Mie asymmetry.
    pub mie: Vec4,
    /// rgb: the haze's extinction by channel; w: its scale height (m).
    pub ext: Vec4,
    /// rgb: the air's Rayleigh extinction (/m).
    pub ray: Vec4,
    /// rgb: the key light, colour times lux; w: its elevation (rad).
    pub key: Vec4,
    /// rgb: the sky's light (nits); w: daylight.
    pub ambient: Vec4,
    /// rgb: the windows' glow from inside (nits); w: the stars through them (nits).
    pub glow: Vec4,
    /// rgb: the city's lights from afar (nits); w: lamps lit.
    pub night: Vec4,
    /// x: the colony's spin angle (rad); y: seconds (wrapped); z: 1 to trace reflections.
    pub clock: Vec4,
}

/// A section of the grade (shadows, midtones or highlights).
#[derive(Clone, Copy, Debug)]
struct Tone {
    saturation: f32,
    contrast: f32,
    lift: f32,
}

impl Tone {
    const fn new(saturation: f32, contrast: f32, lift: f32) -> Self {
        Self { saturation, contrast, lift }
    }

    fn mix(self, o: Self, t: f32) -> Self {
        Self {
            saturation: lerp(self.saturation, o.saturation, t),
            contrast: lerp(self.contrast, o.contrast, t),
            lift: lerp(self.lift, o.lift, t),
        }
    }

    fn section(self) -> ColorGradingSection {
        ColorGradingSection {
            saturation: self.saturation,
            contrast: self.contrast,
            lift: self.lift,
            ..Default::default()
        }
    }
}

/// The picture's grade at a key: white balance and its three sections.
#[derive(Clone, Copy, Debug)]
struct Grade {
    temperature: f32,
    tint: f32,
    shadows: Tone,
    midtones: Tone,
    highlights: Tone,
    /// Bloom's intensity (`Bloom::NATURAL` is 0.15).
    bloom: f32,
}

impl Grade {
    fn mix(self, o: Self, t: f32) -> Self {
        Self {
            temperature: lerp(self.temperature, o.temperature, t),
            tint: lerp(self.tint, o.tint, t),
            shadows: self.shadows.mix(o.shadows, t),
            midtones: self.midtones.mix(o.midtones, t),
            highlights: self.highlights.mix(o.highlights, t),
            bloom: lerp(self.bloom, o.bloom, t),
        }
    }
}

/// The haze's make-up at a key.
#[derive(Clone, Copy, Debug)]
struct Air {
    /// Density at the floor (/m, green), scale height (m), Mie asymmetry.
    density: f32,
    height: f32,
    g: f32,
}

impl Air {
    fn mix(self, o: Self, t: f32) -> Self {
        Self {
            density: lerp(self.density, o.density, t),
            height: lerp(self.height, o.height, t),
            g: lerp(self.g, o.g, t),
        }
    }
}

/// One key of the script, at a height of the light. Light is per unit of daylight.
#[derive(Clone, Copy, Debug)]
struct Key {
    /// The light's elevation, degrees.
    elev: f32,
    /// The sun's colour (linear, brightest channel 1) and its share of [`NOON_LUX`].
    sun: Vec3,
    lux: f32,
    /// The sky's colour (linear) and its share of [`NOON_SKY`].
    sky: Vec3,
    sky_share: f32,
    /// The haze's even glow (nits), and the beams' light it scatters (nits, times the sun's colour).
    haze: Vec3,
    mie: f32,
    air: Air,
    /// The windows' glass from inside (nits).
    glow: Vec3,
    /// Leaning the camera off what the light alone would set, EV (negative: darker).
    ev_bias: f32,
    grade: Grade,
}

impl Key {
    fn mix(&self, o: &Self, t: f32) -> Self {
        Self {
            elev: lerp(self.elev, o.elev, t),
            sun: self.sun.lerp(o.sun, t),
            lux: lerp(self.lux, o.lux, t),
            sky: self.sky.lerp(o.sky, t),
            sky_share: lerp(self.sky_share, o.sky_share, t),
            haze: self.haze.lerp(o.haze, t),
            mie: lerp(self.mie, o.mie, t),
            air: self.air.mix(o.air, t),
            glow: self.glow.lerp(o.glow, t),
            ev_bias: lerp(self.ev_bias, o.ev_bias, t),
            grade: self.grade.mix(o.grade, t),
        }
    }
}

/// Noon: pale, hazy blue-white; the light straight down, the windows burning white.
const NOON: Key = Key {
    elev: 90.0,
    sun: Vec3::new(1.0, 0.96, 0.9),
    lux: 1.0,
    sky: Vec3::new(0.68, 0.78, 1.0),
    sky_share: 1.0,
    haze: Vec3::new(5_600.0, 7_700.0, 11_500.0),
    mie: 450.0,
    air: Air { density: 1.5e-4, height: 1_100.0, g: 0.7 },
    glow: Vec3::new(21_000.0, 26_200.0, 32_000.0),
    ev_bias: 0.0,
    grade: Grade {
        temperature: 0.0,
        tint: 0.0,
        shadows: Tone::new(1.02, 1.04, 0.0),
        midtones: Tone::new(1.1, 1.1, 0.0),
        highlights: Tone::new(0.95, 1.0, 0.0),
        bloom: 0.1,
    },
};

/// Late morning: clearing, still a little cool.
const MORNING: Key = Key {
    elev: 40.0,
    sun: Vec3::new(1.0, 0.94, 0.86),
    lux: 0.95,
    sky: Vec3::new(0.7, 0.78, 1.0),
    sky_share: 0.85,
    haze: Vec3::new(5_400.0, 6_700.0, 8_700.0),
    mie: 520.0,
    air: Air { density: 1.65e-4, height: 1_000.0, g: 0.7 },
    glow: Vec3::new(19_600.0, 23_500.0, 28_000.0),
    ev_bias: 0.0,
    grade: Grade {
        temperature: 0.008,
        tint: 0.004,
        shadows: Tone::new(1.03, 1.04, 0.0),
        midtones: Tone::new(1.1, 1.09, 0.0),
        highlights: Tone::new(0.95, 1.0, 0.0),
        bloom: 0.12,
    },
};

/// The morning's golden hour: cool gold through the last of the mist.
const MORNING_GOLD: Key = Key {
    elev: 13.0,
    sun: Vec3::new(1.0, 0.82, 0.6),
    lux: 0.8,
    sky: Vec3::new(0.64, 0.7, 1.0),
    sky_share: 0.62,
    haze: Vec3::new(3_300.0, 3_500.0, 4_400.0),
    mie: 800.0,
    air: Air { density: 2.0e-4, height: 800.0, g: 0.72 },
    glow: Vec3::new(18_000.0, 15_800.0, 13_000.0),
    ev_bias: -0.1,
    grade: Grade {
        temperature: 0.025,
        tint: 0.015,
        shadows: Tone::new(1.1, 1.0, 0.008),
        midtones: Tone::new(1.07, 1.05, 0.0),
        highlights: Tone::new(1.0, 1.0, 0.0),
        bloom: 0.15,
    },
};

/// Dawn: the mirrors cracking open, rose light low along the axis, mist on the floor.
const DAWN: Key = Key {
    elev: 3.0,
    sun: Vec3::new(1.0, 0.64, 0.56),
    lux: 0.55,
    sky: Vec3::new(0.6, 0.65, 1.0),
    sky_share: 0.5,
    haze: Vec3::new(2_000.0, 1_850.0, 2_700.0),
    mie: 700.0,
    air: Air { density: 2.6e-4, height: 600.0, g: 0.72 },
    glow: Vec3::new(9_000.0, 6_300.0, 6_800.0),
    ev_bias: -0.25,
    grade: Grade {
        temperature: -0.015,
        tint: 0.03,
        shadows: Tone::new(1.08, 1.0, 0.012),
        midtones: Tone::new(1.03, 1.03, 0.0),
        highlights: Tone::new(0.95, 1.0, 0.0),
        bloom: 0.18,
    },
};

/// Early afternoon: the light starting to warm.
const AFTERNOON: Key = Key {
    elev: 40.0,
    sun: Vec3::new(1.0, 0.9, 0.78),
    lux: 0.95,
    sky: Vec3::new(0.7, 0.76, 1.0),
    sky_share: 0.85,
    haze: Vec3::new(5_600.0, 6_600.0, 8_300.0),
    mie: 560.0,
    air: Air { density: 1.5e-4, height: 1_150.0, g: 0.72 },
    glow: Vec3::new(20_200.0, 23_200.0, 28_000.0),
    ev_bias: 0.0,
    grade: Grade {
        temperature: 0.015,
        tint: 0.0,
        shadows: Tone::new(1.04, 1.04, 0.0),
        midtones: Tone::new(1.1, 1.09, 0.0),
        highlights: Tone::new(0.95, 1.0, 0.0),
        bloom: 0.12,
    },
};

/// The afternoon's golden hour: long amber light down the axis, violet shadows.
const GOLDEN: Key = Key {
    elev: 15.0,
    sun: Vec3::new(1.0, 0.7, 0.42),
    lux: 0.8,
    sky: Vec3::new(0.6, 0.62, 1.0),
    sky_share: 0.55,
    haze: Vec3::new(3_300.0, 2_700.0, 2_100.0),
    mie: 750.0,
    air: Air { density: 1.6e-4, height: 1_100.0, g: 0.75 },
    glow: Vec3::new(18_000.0, 13_000.0, 7_400.0),
    ev_bias: -0.2,
    grade: Grade {
        temperature: 0.04,
        tint: 0.0,
        shadows: Tone::new(1.15, 1.0, 0.012),
        midtones: Tone::new(1.1, 1.06, 0.0),
        highlights: Tone::new(1.0, 1.0, 0.0),
        bloom: 0.17,
    },
};

/// Dusk: the mirrors closing, magenta haze, the last red light, the lamps coming on.
const DUSK: Key = Key {
    elev: 4.0,
    sun: Vec3::new(1.0, 0.45, 0.32),
    lux: 0.6,
    sky: Vec3::new(0.78, 0.56, 1.0),
    sky_share: 0.5,
    haze: Vec3::new(2_300.0, 1_250.0, 2_200.0),
    mie: 450.0,
    air: Air { density: 1.8e-4, height: 900.0, g: 0.75 },
    glow: Vec3::new(8_000.0, 4_400.0, 5_100.0),
    ev_bias: -0.35,
    grade: Grade {
        temperature: 0.015,
        tint: 0.045,
        shadows: Tone::new(1.15, 1.0, 0.018),
        midtones: Tone::new(1.1, 1.05, 0.0),
        highlights: Tone::new(1.05, 1.0, 0.0),
        bloom: 0.22,
    },
};

/// The morning's keys and the evening's, by the light's elevation (rising).
const MORNING_KEYS: [Key; 4] = [DAWN, MORNING_GOLD, MORNING, NOON];
const EVENING_KEYS: [Key; 4] = [DUSK, GOLDEN, AFTERNOON, NOON];

/// Night: no sun; deep blue, warm windows and cold signs (`city.wgsl`), the lamps' glow low in the
/// haze. These are floors: the day's light adds to them.
const NIGHT_SKY: Vec3 = Vec3::new(0.36, 0.38, 1.0);
const NIGHT_SKY_NITS: f32 = 30.0;
const NIGHT_HAZE: Vec3 = Vec3::new(6.0, 5.5, 22.0);
/// The lamps' light in the haze, and the city's lights seen from afar (nits, all lamps lit).
const LAMP_HAZE: Vec3 = Vec3::new(8.0, 5.5, 3.0);
const CITY_LIGHTS: Vec3 = Vec3::new(34.0, 25.0, 16.0);
/// Space through the glass by night, the stars averaged (nits).
const STARS: f32 = 2.5;
const NIGHT_AIR: Air = Air { density: 1.4e-4, height: 850.0, g: 0.7 };
const NIGHT_GRADE: Grade = Grade {
    temperature: -0.02,
    tint: 0.012,
    shadows: Tone::new(1.1, 1.0, 0.01),
    midtones: Tone::new(1.0, 1.08, 0.0),
    highlights: Tone::new(1.1, 1.0, 0.0),
    bloom: 0.3,
};

/// The colony's look at an hour.
#[derive(Clone, Debug)]
pub struct Look {
    /// The key light: colour (linear, brightest channel 1) and illuminance square to it, lux.
    pub sun: Vec3,
    pub lux: f32,
    /// The sky's light, the ambient: colour (linear) and luminance, nits.
    pub sky: Vec3,
    pub sky_nits: f32,
    /// The haze: its even glow and the beams' light it scatters (nits), its density at the floor
    /// (/m), scale height (m) and Mie asymmetry.
    pub haze: Vec3,
    pub mie: Vec3,
    pub density: f32,
    pub height: f32,
    pub g: f32,
    /// The windows' glass from inside by day (nits), and the stars through it by night (nits).
    pub glow: Vec3,
    pub stars: f32,
    /// The city's lights from afar (nits).
    pub city: Vec3,
    /// The camera's exposure outdoors, EV100.
    pub ev: f32,
    /// The picture's grade (its white balance and sections; `camera::pilot_effects` owns its
    /// exposure and post-saturation) and bloom's intensity.
    pub grading: ColorGrading,
    pub bloom: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The keys either side of `elev` (degrees), eased between (each key holds a while).
fn key_at(keys: &[Key; 4], elev: f32) -> Key {
    if elev <= keys[0].elev {
        return keys[0];
    }
    for w in keys.windows(2) {
        if elev <= w[1].elev {
            return w[0].mix(&w[1], smoothstep(w[0].elev, w[1].elev, elev));
        }
    }
    keys[3]
}

/// How much of the daylight a camera metering the city sees, by the light's height: with it low, the
/// ground is mostly in shade (but not all the way: the walls catch it).
fn metered(elev: f32) -> f32 {
    0.35 + 0.65 * elev.sin().max(0.0)
}

/// The colony's look at hour `d`.
pub fn look(d: &Day) -> Look {
    let elev = d.sun_elev.to_degrees();
    let evening = smoothstep(NOON_PHASE - 0.02, NOON_PHASE + 0.02, d.phase);
    let key = key_at(&MORNING_KEYS, elev).mix(&key_at(&EVENING_KEYS, elev), evening);
    let day = d.daylight.clamp(0.0, 1.0);
    let night = 1.0 - smoothstep(0.0, DUSKFALL, day);
    let lamps = d.lamps.clamp(0.0, 1.0);
    let lux = NOON_LUX * key.lux * day;
    let sky = key.sky * (NOON_SKY * key.sky_share * day) + NIGHT_SKY * NIGHT_SKY_NITS;
    let sky_nits = sky.max_element();
    let air = key.air.mix(NIGHT_AIR, night);
    let grade = key.grade.mix(NIGHT_GRADE, night);
    let floor = 2f32.powf(EV_NIGHT - EV_NOON);
    // The camera never opens up past night's exposure (the colony e2e tells a room by EV under 8.5).
    let ev = (EV_NOON + (day * key.lux * metered(d.sun_elev) + floor).log2() + key.ev_bias * (1.0 - night))
        .max(EV_NIGHT);
    Look {
        sun: key.sun,
        lux,
        sky: sky / sky_nits,
        sky_nits,
        haze: key.haze * day + NIGHT_HAZE + LAMP_HAZE * lamps,
        mie: key.sun * (key.mie * day),
        density: air.density,
        height: air.height,
        g: air.g,
        glow: key.glow * day,
        stars: STARS * night,
        city: CITY_LIGHTS * lamps,
        ev,
        grading: ColorGrading {
            global: ColorGradingGlobal {
                temperature: grade.temperature,
                tint: grade.tint,
                ..Default::default()
            },
            shadows: grade.shadows.section(),
            midtones: grade.midtones.section(),
            highlights: grade.highlights.section(),
        },
        bloom: grade.bloom,
    }
}

/// The haze's density averaged from height `h` (m) down to the floor, as a share of the floor's:
/// what lies between an eye up there and what stands on the ground.
fn mean_density(h: f32, scale: f32) -> f32 {
    if h < 1.0 { 1.0 } else { scale / h * (1.0 - (-h / scale).exp()) }
}

impl Look {
    /// The sky function's uniform at hour `d`, the colony's spin angle (rad) and the clock (s);
    /// `reflections` off (the Low tier) has glass and water show a cheap stand-in instead.
    pub fn sky_params(&self, d: &Day, spin: f32, seconds: f32, reflections: bool) -> Sky {
        Sky {
            haze: self.haze.extend(self.density),
            mie: self.mie.extend(self.g),
            ext: HAZE_EXT.extend(self.height),
            ray: RAYLEIGH.extend(0.0),
            key: (self.sun * self.lux).extend(d.sun_elev),
            ambient: (self.sky * self.sky_nits).extend(d.daylight),
            glow: self.glow.extend(self.stars),
            night: self.city.extend(d.lamps),
            clock: Vec4::new(spin, seconds, if reflections { 1.0 } else { 0.0 }, 0.0),
        }
    }

    /// The camera's distance fog, for what keeps Bevy's own (people, cars, trams; the city's shader
    /// draws its haze itself): the haze as it is between an eye `eye_h` metres up and the floor,
    /// even over the short distances those are seen at, with the key light's forward glow. Colours
    /// exposed by `exposure`.
    pub fn fog(&self, eye_h: f32, exposure: f32) -> DistanceFog {
        let ext = HAZE_EXT * (self.density * mean_density(eye_h.max(0.0), self.height)) + RAYLEIGH;
        // The Mie phase's broad part goes into the fog's colour; its peak, less that, into Bevy's
        // glow round the light (a power of the cosine, about as wide).
        let base = (self.haze + self.mie * 0.25) * exposure;
        let peak = (1.0 + self.g) / ((1.0 - self.g) * (1.0 - self.g)) - 0.25;
        let glow = if self.lux > 1.0 { self.mie.max_element() * peak / self.lux } else { 0.0 };
        DistanceFog {
            color: Color::linear_rgb(base.x, base.y, base.z),
            directional_light_color: Color::linear_rgba(1.0, 1.0, 1.0, glow),
            directional_light_exponent: 10.0,
            falloff: FogFalloff::Atmospheric { extinction: ext, inscattering: ext },
        }
    }
}
