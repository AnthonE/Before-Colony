//! The colony's day, on the tick's clock so everyone has the same hour. The three mirrors make it:
//! nearly shut at night, opening through dawn, standing wide at noon (the sun straight overhead, in
//! the window across the colony), closing through dusk. A day is 48 minutes: dawn 4, day 32, dusk 4,
//! night 8. A fresh server opens mid-morning.
//!
//! Inside, the sun is drawn as the mirrors throw it: on the +X side of the window overhead, twice
//! the mirrors' opening above the horizon (light along −X off a mirror opened β leaves at 2β).

use core::f32::consts::{FRAC_PI_2, PI};

use glam::Vec3;

use crate::colony::frame::{STRIPS, strip_centre};
use crate::math::{cos, sin};

/// Ticks in a colony day (48 min at 30 Hz).
pub const DAY_TICKS: u32 = 86_400;
const DAWN: u32 = 7_200;
const DAY: u32 = 57_600;
const DUSK: u32 = 7_200;
/// Where tick 0 falls in the day: 8 minutes into the day's light.
const START: u32 = DAWN + 14_400;

/// The mirrors' opening at night, at the end of dawn, and at noon, rad.
pub const MIRROR_SHUT: f32 = 1.0 * PI / 180.0;
pub const MIRROR_NOON: f32 = 45.0 * PI / 180.0;

/// The colony's hour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Day {
    /// Through the day, 0..1 from the start of dawn.
    pub phase: f32,
    /// How much sunlight the mirrors throw in: 0 at night, 0.5 at the end of dawn, 1 at noon.
    pub daylight: f32,
    /// The mirrors' opening, rad.
    pub mirror_beta: f32,
    /// The sun's height above the horizon inside, rad: twice the mirrors' opening.
    pub sun_elev: f32,
    /// How many of the city's lamps are lit, 0..1.
    pub lamps: f32,
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The hour at tick `t` plus `frac` of the next.
pub fn day(t: u32, frac: f32) -> Day {
    let at = ((t % DAY_TICKS + START) % DAY_TICKS) as f32 + frac.clamp(0.0, 1.0);
    let daylight = if at < DAWN as f32 {
        0.5 * smooth(at / DAWN as f32)
    } else if at < (DAWN + DAY) as f32 {
        let s = sin(PI * (at - DAWN as f32) / DAY as f32);
        0.5 + 0.5 * s * s
    } else if at < (DAWN + DAY + DUSK) as f32 {
        0.5 * (1.0 - smooth((at - (DAWN + DAY) as f32) / DUSK as f32))
    } else {
        0.0
    };
    let mirror_beta = MIRROR_SHUT + (MIRROR_NOON - MIRROR_SHUT) * daylight;
    Day {
        phase: at / DAY_TICKS as f32,
        daylight,
        mirror_beta,
        sun_elev: (2.0 * mirror_beta).min(FRAC_PI_2),
        lamps: 1.0 - smooth((daylight - 0.15) / 0.4),
    }
}

/// The way to the sun from land strip `strip`, in the colony's own frame: up from the strip
/// (towards the window overhead), leaning to +X by the sun's elevation.
pub fn key_light(strip: usize, d: &Day) -> Vec3 {
    let a = strip_centre(strip % STRIPS);
    let up = Vec3::new(0.0, -cos(a), -sin(a));
    Vec3::X * cos(d.sun_elev) + up * sin(d.sun_elev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_comes_round_to_the_bit() {
        for t in [0u32, 1, 7_199, 40_000, 86_399, 1_000_000] {
            assert_eq!(day(t, 0.25), day(t + DAY_TICKS, 0.25), "{t}");
        }
    }

    #[test]
    fn the_day_flows_without_jumps() {
        let mut last = day(0, 0.0);
        let (mut night, mut noon) = (false, false);
        for t in 1..=DAY_TICKS {
            let d = day(t, 0.0);
            assert!((d.daylight - last.daylight).abs() < 2e-4, "a jump in daylight at {t}");
            assert!((d.mirror_beta - last.mirror_beta).abs() < 2e-4, "the mirrors jump at {t}");
            assert!((0.0..=1.0).contains(&d.daylight) && (0.0..=1.0).contains(&d.lamps));
            assert!((MIRROR_SHUT..=MIRROR_NOON + 1e-6).contains(&d.mirror_beta));
            night |= d.daylight == 0.0 && d.lamps == 1.0 && d.mirror_beta == MIRROR_SHUT;
            noon |= (d.mirror_beta - MIRROR_NOON).abs() < 1e-4 && (d.sun_elev - FRAC_PI_2).abs() < 1e-3;
            last = d;
        }
        assert!(night && noon, "a day has a night and a noon");
    }

    #[test]
    fn a_fresh_server_opens_in_daylight() {
        let d = day(0, 0.0);
        assert!(d.daylight > 0.5 && d.lamps == 0.0, "{d:?}");
    }

    #[test]
    fn the_sun_stands_over_each_strip_from_the_plus_x_side() {
        let d = day(0, 0.0);
        for k in 0..STRIPS {
            let l = key_light(k, &d);
            assert!((l.length() - 1.0).abs() < 1e-5);
            assert!(l.x > 0.0, "the mirrors throw it from +X");
            // Up, from strip k: towards the axis from its middle.
            let a = strip_centre(k);
            let up = Vec3::new(0.0, -cos(a), -sin(a));
            assert!(l.dot(up) > 0.0);
        }
    }
}
