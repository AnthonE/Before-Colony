//! Salvage and mining tunables: what suits are made of, how long wreckage lasts.

use bc_proto::{FrameId, Part};

use crate::config::secs;
use crate::content::frame;
use crate::math::floor;

/// Share of a frame's dry mass in each part (the torso holds the rest).
const PART_SHARE: [f32; Part::COUNT] = [0.04, 0.52, 0.08, 0.08, 0.18, 0.10];

/// A part's mass, kg: a multiple of 10, like every chunk's.
pub fn part_mass_kg(frame_id: FrameId, part: Part) -> u32 {
    let tens = frame(frame_id).dry_mass * PART_SHARE[part as usize] / 10.0;
    floor(tens + 0.5) as u32 * 10
}

/// Mass of what's still on a suit whose parts in `lost` (a bit per [`Part`]) came off, kg.
pub fn mass_without(frame_id: FrameId, lost: u8) -> u32 {
    Part::ALL.iter().filter(|p| lost & (1 << **p as u8) == 0).map(|p| part_mass_kg(frame_id, *p)).sum()
}

/// How long wreckage drifts before it's cleared: Mobile Dolls' and pilots'.
pub fn wreck_ttl(doll: bool) -> u32 {
    if doll { secs(60.0) } else { secs(180.0) }
}

/// Parts that come off when destroyed (the torso is the suit).
pub const DETACHABLE: [Part; 5] = [Part::Head, Part::ArmL, Part::ArmR, Part::Legs, Part::Backpack];

/// How fast a part flies off, m/s: outward from the body, and along the blow.
pub const DETACH_SPEED: f32 = 4.0;
pub const DETACH_PUSH: f32 = 3.0;
/// Coefficient of restitution when a chunk meets the colony or a rock.
pub const BOUNCE: f32 = 0.4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_add_up_to_the_frame() {
        for f in FrameId::ALL {
            let all = mass_without(f, 0);
            let dry = frame(f).dry_mass;
            assert!((all as f32 - dry).abs() <= 30.0, "{f:?}: {all} vs {dry}");
            assert_eq!(all % 10, 0);
        }
        let leo = FrameId::Leo;
        assert_eq!(
            mass_without(leo, 1 << Part::ArmL as u8),
            mass_without(leo, 0) - part_mass_kg(leo, Part::ArmL)
        );
    }
}
