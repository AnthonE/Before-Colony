//! Salvage and mining tunables: what suits are made of, how long wreckage lasts.

use bc_proto::{CARGO_KINDS, ChunkDesc, ChunkKind, FrameId, Part, WeaponKind};
use glam::Vec3;

use crate::config::secs;
use crate::content::frame;
use crate::math::floor;
use crate::world::{COLONY_CENTER, COLONY_HALF_LENGTH};

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

/// How far beyond a chunk's size a hand reaches to grab it, m.
pub const REACH: f32 = 8.0;
/// The fastest a chunk can be moving relative to the hand and still be caught, m/s.
pub const CATCH_SPEED: f32 = 12.0;
/// The heaviest chunk that fits in a hold, kg (anything heavier is towed in hand).
pub const STOW_MAX_KG: u32 = 2_500;
/// A throw's push, N·s, and the fastest it flings a light chunk, m/s.
pub const THROW_IMPULSE: f32 = 60_000.0;
pub const THROW_SPEED_MAX: f32 = 40.0;
/// Jettisoned (or spilled) cargo drifts off at this speed, m/s.
pub const JETTISON_SPEED: f32 = 3.0;
/// How far the docking hub stands out from the colony's −X end cap, m (drawn by the client; nothing
/// collides with it).
pub const DOCK_HUB_LENGTH: f32 = 900.0;
/// The dock: a sphere just off the docking hub's mouth. Arriving slower than `DOCK_SPEED` sells the
/// hold and whatever is in hand, and tops up propellant.
pub const DOCK_CENTER: Vec3 = Vec3::new(
    COLONY_CENTER.x - COLONY_HALF_LENGTH - DOCK_HUB_LENGTH - 350.0,
    COLONY_CENTER.y,
    COLONY_CENTER.z,
);
pub const DOCK_RADIUS: f32 = 300.0;
pub const DOCK_SPEED: f32 = 25.0;
/// Credits per kg of each material: nickel-iron, titanium, volatiles, exotics.
pub const PRICE: [u32; CARGO_KINDS] = [1, 4, 3, 15];

/// How hard each weapon works a rock: sabers cut it, cannon rounds chip it, beams mostly glance off.
pub fn rock_multiplier(kind: WeaponKind) -> f32 {
    match kind {
        WeaponKind::BeamSaber => 2.0,
        WeaponKind::MachineCannon => 1.0,
        _ => 0.3,
    }
}

/// Ore a beam boils off per point of damage, kg.
pub const BEAM_WASTE_KG: f32 = 4.0;
/// Most ore one saber stroke chips off a rock, kg.
pub const CHIP_KG: u32 = 200;
/// A shattered rock grows back only when no suit is this close, m.
pub const REGROW_CLEAR: f32 = 1_000.0;
/// How far past its blade a saber digs into rock, m: a suit keeps 8 m off a rock's surface, and
/// the blade has to reach it.
pub const SABER_DIG: f32 = 2.5;

/// Each frame's hold, kg (Mobile Dolls carry nothing).
pub fn hold_kg(frame_id: FrameId) -> u32 {
    match frame_id {
        FrameId::Leo => 3_000,
        FrameId::WingZero => 1_500,
        FrameId::Taurus | FrameId::Virgo => 0,
    }
}

/// How long loose ore lasts (jettisoned, spilled or mined).
pub fn ore_ttl() -> u32 {
    secs(300.0)
}

/// What a chunk is made of, as cargo: ore is its kind; a Gundam's parts are gundanium (sold with
/// the exotics), everyone else's titanium.
pub fn material(kind: ChunkKind) -> usize {
    match kind {
        ChunkKind::Ore { ore } => usize::from(ore) % CARGO_KINDS,
        ChunkKind::Limb { frame: f, .. } | ChunkKind::Hulk { frame: f, .. } => {
            if f == FrameId::WingZero {
                3
            } else {
                1
            }
        }
    }
}

/// Whether a chunk can go in a hold: loose ore and limbs up to `STOW_MAX_KG` (hulks are towed).
pub fn stowable(desc: &ChunkDesc) -> bool {
    !matches!(desc.kind, ChunkKind::Hulk { .. }) && desc.mass_kg <= STOW_MAX_KG
}

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
