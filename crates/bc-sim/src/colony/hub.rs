//! The docking hub at the colony's −X end, as built: the spire on the axis (the hub tube the suits
//! launch from and dock at, stacked with modules), and the bay ring, the spin ring standing off the
//! end cap where the pilots' bays hang at 0.7 g. Both turn with the colony, and both are shapes of
//! revolution about its axis, so their size is all there is to them here.

use crate::content::salvage::DOCK_HUB_LENGTH;
use crate::world::COLONY_HALF_LENGTH;

/// The spire's tube: the docking hub, from the end cap out to its mouth.
pub const SPIRE_RADIUS: f32 = 340.0;
/// Where the spire's mouth is (the launch gate and the dock lie beyond it).
pub const SPIRE_MOUTH_X: f32 = -COLONY_HALF_LENGTH - DOCK_HUB_LENGTH;

/// The modules stacked on the spire, from the end cap out: (middle x, radius, thickness), m.
pub const SPIRE_TIERS: [(f32, f32, f32); 4] =
    [(-16_120.0, 560.0, 60.0), (-16_330.0, 470.0, 40.0), (-16_560.0, 430.0, 30.0), (-16_820.0, 400.0, 50.0)];

/// The bay ring: inner and outer radius, and the x of its faces (it stands 50 m off the end cap).
pub const BAY_RING_INNER: f32 = 1_950.0;
pub const BAY_RING_OUTER: f32 = 2_650.0;
pub const BAY_RING_X: (f32, f32) = (-COLONY_HALF_LENGTH - 450.0, -COLONY_HALF_LENGTH - 50.0);
/// The radius the bays hang at: 0.7 g.
pub const BAY_RADIUS: f32 = 2_252.0;
/// The bays round the ring (a pilot's is `slot % 99 + 1`).
pub const BAYS: u32 = 99;
/// Spokes from the spire to the bay ring.
pub const SPOKES: u32 = 6;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::frame::gravity;
    use crate::config::G0;
    use crate::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
    use crate::sim::LAUNCH_GATE;
    use crate::world::COLONY_RADIUS;

    #[test]
    fn the_bays_hang_at_seven_tenths_of_a_g_in_their_ring() {
        let g = gravity(COLONY_RADIUS - BAY_RADIUS) / G0;
        assert!((g - 0.7).abs() < 0.005, "{g}");
        const { assert!(BAY_RING_INNER < BAY_RADIUS && BAY_RADIUS < BAY_RING_OUTER) };
        const { assert!(BAY_RING_X.0 < BAY_RING_X.1 && BAY_RING_X.1 < -COLONY_HALF_LENGTH) };
    }

    #[test]
    fn the_spire_keeps_clear_of_the_launch_gate_and_the_dock() {
        for (x, r, t) in SPIRE_TIERS {
            assert!(x - t / 2.0 > SPIRE_MOUTH_X && x + t / 2.0 < -COLONY_HALF_LENGTH, "a tier off the spire");
            assert!(r > SPIRE_RADIUS && r < BAY_RING_INNER);
        }
        // The launch places ring the gate 100 m out, beyond the mouth; the dock's sphere too.
        const { assert!(LAUNCH_GATE.x < SPIRE_MOUTH_X - 50.0) };
        const { assert!(DOCK_CENTER.x + DOCK_RADIUS < SPIRE_MOUTH_X - 40.0) };
    }
}
