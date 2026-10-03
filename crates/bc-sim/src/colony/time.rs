//! The colony's day, on the tick's clock so everyone has the same hour. The three mirrors make it:
//! nearly shut at night, they open through the morning to stand wide at noon (the sun straight
//! overhead, in the window across the colony) and close through the afternoon, on one smooth curve
//! over the whole of the light. They open slowly at first, so the light rakes low along the axis for
//! about the first and last sixth of it (the golden hours: the sun under 25°), and climbs through
//! the late morning. A day is 48 minutes: dawn 4, day 32, dusk 4, night 8. The lamps go out in the
//! dawn and come on again in the dusk. A fresh server opens mid-morning.
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
/// The light, from the start of dawn to the end of dusk (40 minutes).
const LIT: u32 = DAWN + DAY + DUSK;
/// Where tick 0 falls in the day: 8 minutes into the day's light.
const START: u32 = DAWN + 14_400;
/// The daylight that puts the lamps out: what the dawn has a little before its middle.
const LAMPS_OUT: f32 = 0.024;

/// The mirrors' opening at night and at noon, rad.
pub const MIRROR_SHUT: f32 = 1.0 * PI / 180.0;
pub const MIRROR_NOON: f32 = 45.0 * PI / 180.0;

/// The colony's hour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Day {
    /// Through the day, 0..1 from the start of dawn.
    pub phase: f32,
    /// How much sunlight the mirrors throw in: 0 at night, a tenth at the end of dawn, half a
    /// quarter of the way through the light, 1 at noon.
    pub daylight: f32,
    /// The mirrors' opening, rad: from shut to wide as the daylight grows.
    pub mirror_beta: f32,
    /// The sun's height above the horizon inside, rad: twice the mirrors' opening.
    pub sun_elev: f32,
    /// How many of the city's lamps are lit, 0..1: all of them at night, out by the dawn's middle,
    /// coming on again from the dusk's.
    pub lamps: f32,
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The hour at tick `t` plus `frac` of the next.
pub fn day(t: u32, frac: f32) -> Day {
    let at = ((t % DAY_TICKS + START) % DAY_TICKS) as f32 + frac.clamp(0.0, 1.0);
    // Daylight rises and falls as sin² over the light, meeting the night smoothly at both ends, and
    // the mirrors open with it: the sun 10° up at the end of dawn, 32° a fifth of the way through,
    // 46° at a quarter, and straight overhead at noon.
    let daylight = if at < LIT as f32 {
        let s = sin(PI * at / LIT as f32);
        s * s
    } else {
        0.0
    };
    let mirror_beta = MIRROR_SHUT + (MIRROR_NOON - MIRROR_SHUT) * daylight;
    Day {
        phase: at / DAY_TICKS as f32,
        daylight,
        mirror_beta,
        sun_elev: (2.0 * mirror_beta).min(FRAC_PI_2),
        lamps: 1.0 - smooth(daylight / LAMPS_OUT),
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
            assert!((d.lamps - last.lamps).abs() < 1e-3, "the lamps jump at {t}");
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
        assert!(d.sun_elev > 25f32.to_radians(), "mid-morning, past the golden hour: {d:?}");
    }

    /// The hour `a` ticks after the start of dawn.
    fn hour(a: u32) -> Day {
        day((a + DAY_TICKS - START) % DAY_TICKS, 0.0)
    }

    #[test]
    fn the_light_rakes_low_for_a_sixth_of_the_light_at_each_end() {
        let golden = 25f32.to_radians();
        let mut last = hour(0);
        for a in 1..=LIT / 2 {
            let (am, pm) = (hour(a), hour(LIT - a));
            assert!((am.sun_elev - pm.sun_elev).abs() < 1e-5, "the afternoon mirrors the morning at {a}");
            assert!((am.daylight - pm.daylight).abs() < 1e-5, "{a}");
            assert!(am.sun_elev >= last.sun_elev - 1e-6, "the sun climbs all morning, but not at {a}");
            assert!(am.daylight >= last.daylight - 1e-6, "{a}");
            if a <= LIT / 6 {
                assert!(am.sun_elev < golden, "the golden hour is over at {a}: {am:?}");
            }
            last = am;
        }
        // Low for a while, but not for long: the end of dawn has the sun 10° up, not still level.
        let dawn = hour(DAWN);
        assert!(dawn.sun_elev > 8f32.to_radians() && dawn.sun_elev < 13f32.to_radians(), "{dawn:?}");
        let q = hour(LIT / 4);
        assert!(q.daylight >= 0.35, "a quarter of the way in, it reads as day: {q:?}");
        assert!(q.sun_elev > 40f32.to_radians(), "the golden hour is over by a quarter: {q:?}");
        let third = hour(LIT / 3);
        assert!(third.sun_elev > 45f32.to_radians() && third.daylight > 0.7, "{third:?}");
        let noon = hour(LIT / 2);
        assert!((noon.sun_elev - FRAC_PI_2).abs() < 1e-3 && noon.daylight > 0.999, "{noon:?}");
    }

    #[test]
    fn the_lamps_go_out_in_the_dawn_and_come_on_in_the_dusk() {
        assert_eq!(hour(0).lamps, 1.0, "lit as the dawn starts");
        for a in 0..DAY_TICKS {
            let d = hour(a);
            if a >= LIT {
                assert_eq!(d.lamps, 1.0, "the night is lit, but not at {a}");
            } else if (DAWN / 2..=DAWN + DAY + DUSK / 2).contains(&a) {
                assert_eq!(d.lamps, 0.0, "out from the dawn's middle to the dusk's, but not at {a}");
            }
        }
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
