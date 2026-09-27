//! Display names. Gundam Wing names are © Sotsu · Sunrise and only compiled in with the
//! `canon-names` feature; without it the game ships generic names.

use bc_proto::{FrameId, WeaponKind};

#[cfg(feature = "canon-names")]
pub fn frame_name(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "Leo",
        FrameId::WingZero => "Wing Gundam Zero",
        FrameId::Taurus => "Taurus",
        FrameId::Virgo => "Virgo",
        FrameId::Heavyarms => "Gundam Heavyarms",
        FrameId::Deathscythe => "Gundam Deathscythe",
        FrameId::Sandrock => "Gundam Sandrock",
        FrameId::Shenlong => "Shenlong Gundam",
        FrameId::WingZeroBird => "Wing Zero (Neo-Bird)",
    }
}

#[cfg(feature = "canon-names")]
pub fn frame_designation(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "OZ-06MS",
        FrameId::WingZero => "XXXG-00W0",
        FrameId::Taurus => "OZ-13MS",
        FrameId::Virgo => "OZ-02MD",
        FrameId::Heavyarms => "XXXG-01H",
        FrameId::Deathscythe => "XXXG-01D",
        FrameId::Sandrock => "XXXG-01SR",
        FrameId::Shenlong => "XXXG-01S",
        FrameId::WingZeroBird => "XXXG-00W0",
    }
}

#[cfg(feature = "canon-names")]
pub fn weapon_name(kind: WeaponKind) -> &'static str {
    match kind {
        WeaponKind::BeamRifle => "Beam Rifle",
        WeaponKind::MachineCannon => "Machine Cannon",
        WeaponKind::BeamSaber => "Beam Saber",
        WeaponKind::TwinBusterRifle => "Twin Buster Rifle",
        WeaponKind::BeamCannon => "Beam Cannon",
        WeaponKind::BeamGatling => "Beam Gatling",
        WeaponKind::HomingMissile => "Homing Missiles",
        WeaponKind::ArmyKnife => "Army Knife",
        WeaponKind::ChestGatling => "Chest Gatlings",
        WeaponKind::MicroMissile => "Micro-Missiles",
        WeaponKind::BusterShield => "Buster Shield",
        WeaponKind::HeadVulcan => "Head Vulcans",
        WeaponKind::BeamScythe => "Beam Scythe",
        WeaponKind::BeamMachineGun => "Beam Machine Gun",
        WeaponKind::HeatShotel => "Heat Shotels",
        WeaponKind::CrossCrusher => "Cross Crusher",
        WeaponKind::DragonFang => "Dragon Fang",
        WeaponKind::Flamethrower => "Flamethrower",
        WeaponKind::BeamGlaive => "Beam Glaive",
    }
}

#[cfg(not(feature = "canon-names"))]
pub fn frame_name(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "Line Frame",
        FrameId::WingZero => "Prototype Zero",
        FrameId::Taurus => "Drone T",
        FrameId::Virgo => "Drone V",
        FrameId::Heavyarms => "Prototype Arsenal",
        FrameId::Deathscythe => "Prototype Reaper",
        FrameId::Sandrock => "Prototype Bulwark",
        FrameId::Shenlong => "Prototype Dragon",
        FrameId::WingZeroBird => "Prototype Zero (flight form)",
    }
}

#[cfg(not(feature = "canon-names"))]
pub fn frame_designation(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "LF-06",
        FrameId::WingZero => "PX-00",
        FrameId::Taurus => "MD-13",
        FrameId::Virgo => "MD-02",
        FrameId::Heavyarms => "PX-01A",
        FrameId::Deathscythe => "PX-01R",
        FrameId::Sandrock => "PX-01B",
        FrameId::Shenlong => "PX-01D",
        FrameId::WingZeroBird => "PX-00",
    }
}

#[cfg(not(feature = "canon-names"))]
pub fn weapon_name(kind: WeaponKind) -> &'static str {
    match kind {
        WeaponKind::BeamRifle => "Beam Rifle",
        WeaponKind::MachineCannon => "Autocannon",
        WeaponKind::BeamSaber => "Beam Blade",
        WeaponKind::TwinBusterRifle => "Heavy Beam Rifle",
        WeaponKind::BeamCannon => "Beam Cannon",
        WeaponKind::BeamGatling => "Beam Repeater",
        WeaponKind::HomingMissile => "Homing Missiles",
        WeaponKind::ArmyKnife => "Combat Knife",
        WeaponKind::ChestGatling => "Chest Guns",
        WeaponKind::MicroMissile => "Micro-Missiles",
        WeaponKind::BusterShield => "Claw Shield",
        WeaponKind::HeadVulcan => "Head Guns",
        WeaponKind::BeamScythe => "Beam Reaper",
        WeaponKind::BeamMachineGun => "Beam Carbine",
        WeaponKind::HeatShotel => "Heat Blades",
        WeaponKind::CrossCrusher => "Pincer Strike",
        WeaponKind::DragonFang => "Claw Arm",
        WeaponKind::Flamethrower => "Flamethrower",
        WeaponKind::BeamGlaive => "Beam Polearm",
    }
}
