//! The motor pools: where a pilot on foot in the city takes a car or a scooter. One outside each
//! strip's Hub Gate, and one on the avenue's out-bound road by each tram station's platform.
//! Driving starts only at one of them (the server checks a driver's first pose against them).

use super::city::place_door;
use super::frame::STRIP_WIDTH;
use super::transit::{PLATFORM_LENGTH, STATIONS, station_x};
use crate::content::city::{PLACES, PlaceKind};

/// Pools a strip has: Hub Gate's, then a station's each.
pub const POOLS: usize = 1 + STATIONS;
/// How near a pool a pilot must be to take a vehicle from it, m.
pub const POOL_REACH: f32 = 6.0;

/// Pool `i` of strip `strip`: where it is, `(s, x)`.
pub fn pool(strip: u8, i: usize) -> (f32, f32) {
    let mid = STRIP_WIDTH * 0.5;
    if i == 0 {
        // Beside Hub Gate's door, on the avenue's in-bound road.
        let gate =
            PLACES.iter().find(|p| p.kind == PlaceKind::HubGate && p.strip == strip).unwrap_or(&PLACES[0]);
        let ((_, x), (_, dx)) = place_door(gate);
        return (mid - 16.0, x - dx * 14.0);
    }
    (mid + 16.0, station_x(i - 1) - 0.5 * PLATFORM_LENGTH - 12.0)
}

/// The pool within reach of `(s, x)` on strip `strip`, if there is one.
pub fn pool_near(strip: u8, s: f32, x: f32) -> Option<usize> {
    (0..POOLS).find(|i| {
        let (ps, px) = pool(strip, *i);
        (ps - s) * (ps - s) + (px - x) * (px - x) <= POOL_REACH * POOL_REACH
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::{Stage, solid};
    use glam::Vec3;

    #[test]
    fn every_pool_stands_on_open_road() {
        for strip in 0..3u8 {
            for i in 0..POOLS {
                let (s, x) = pool(strip, i);
                // Room for a car (4.4 × 1.8 m) and then some.
                let clear = !solid(
                    strip,
                    Vec3::new(x - 4.0, 0.4, -s - 2.0),
                    Vec3::new(x + 4.0, 2.0, -s + 2.0),
                    Stage(0),
                );
                assert!(clear, "pool {i} of strip {strip} at ({s}, {x})");
                assert_eq!(pool_near(strip, s + 1.0, x - 1.0), Some(i));
            }
        }
    }
}
