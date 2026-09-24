//! Sensor model. The server only replicates contacts a pilot's sensors can see, so fog of war
//! doubles as wallhack protection. Boosting and firing make a suit easier to detect.

use glam::Vec3;

use crate::config::VISUAL_RANGE;

/// Whether an observer with `sensor_range` detects a target with effective `signature`.
#[inline]
pub fn detects(observer: Vec3, sensor_range: f32, target: Vec3, signature: f32) -> bool {
    let d2 = (target - observer).length_squared();
    let r = sensor_range * signature;
    d2 <= VISUAL_RANGE * VISUAL_RANGE || d2 <= r * r
}

/// Signature multiplier from what a suit is doing.
#[inline]
pub fn signature(base: f32, boosting: bool, fired_recently: bool, wreck: bool) -> f32 {
    let mut s = base;
    if boosting {
        s *= 1.5;
    }
    if fired_recently {
        s *= 1.8;
    }
    if wreck {
        s *= 0.5;
    }
    s
}
