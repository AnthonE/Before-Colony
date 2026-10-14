//! Display names. They are the game's own (`docs/STORY.md`, "Voice and naming"): the Almagest names
//! its lines after the constellations of Ptolemy's catalogue, the colony its Lodestars after stars.

use bc_proto::{FrameId, WeaponKind};

pub fn frame_name(id: FrameId) -> &'static str {
    match id {
        FrameId::Argo => "Argo",
        FrameId::Altair => "Altair",
        FrameId::Corvus => "Corvus",
        FrameId::Cetus => "Cetus",
        FrameId::Antares => "Antares",
        FrameId::Algol => "Algol",
        FrameId::Canopus => "Canopus",
        FrameId::Thuban => "Thuban",
        FrameId::Aquila => "Altair (Aquila)",
    }
}

pub fn frame_designation(id: FrameId) -> &'static str {
    match id {
        FrameId::Argo => "AL-04S",
        FrameId::Altair => "PX-00",
        FrameId::Corvus => "AL-11D",
        FrameId::Cetus => "AL-09D",
        FrameId::Antares => "PX-01",
        FrameId::Algol => "PX-02",
        FrameId::Canopus => "PX-03",
        FrameId::Thuban => "PX-04",
        FrameId::Aquila => "PX-00",
    }
}

pub fn weapon_name(kind: WeaponKind) -> &'static str {
    match kind {
        WeaponKind::BeamRifle => "Beam Rifle",
        WeaponKind::MachineCannon => "Machine Cannon",
        WeaponKind::BeamSaber => "Beam Saber",
        WeaponKind::HeavyBeamRifle => "Heavy Beam Rifle",
        WeaponKind::BeamCannon => "Beam Cannon",
        WeaponKind::BeamGatling => "Beam Gatling",
        WeaponKind::HomingMissile => "Homing Missiles",
        WeaponKind::CombatKnife => "Combat Knife",
        WeaponKind::ChestGatling => "Chest Gatlings",
        WeaponKind::MicroMissile => "Micro-Missiles",
        WeaponKind::ClawShield => "Claw Shield",
        WeaponKind::HeadGun => "Head Guns",
        WeaponKind::BeamScythe => "Beam Scythe",
        WeaponKind::BeamMachineGun => "Beam Machine Gun",
        WeaponKind::HeatBlade => "Heat Blades",
        WeaponKind::PincerStrike => "Pincer Strike",
        WeaponKind::Talon => "Talon",
        WeaponKind::Flamethrower => "Flamethrower",
        WeaponKind::BeamGlaive => "Beam Glaive",
        WeaponKind::BeamRifleCharged => "Charged Beam",
        WeaponKind::Reactor => "Self-Destruct",
    }
}

/// The resource satellite that serves the colony's dock (a landmark).
pub(crate) const FORNAX: &str = "FORNAX";

/// What the pilotless suits are called.
pub fn drone_name() -> &'static str {
    "Drone"
}

/// What a new line of suit, the colony's own, is called in a sentence ("try a new Lodestar").
pub fn prototype_word() -> &'static str {
    "Lodestar"
}
