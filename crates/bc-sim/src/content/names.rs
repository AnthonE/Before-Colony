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
    }
}

#[cfg(feature = "canon-names")]
pub fn frame_designation(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "OZ-06MS",
        FrameId::WingZero => "XXXG-00W0",
        FrameId::Taurus => "OZ-13MS",
        FrameId::Virgo => "OZ-02MD",
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
    }
}

#[cfg(not(feature = "canon-names"))]
pub fn frame_name(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "Line Frame",
        FrameId::WingZero => "Prototype Zero",
        FrameId::Taurus => "Drone T",
        FrameId::Virgo => "Drone V",
    }
}

#[cfg(not(feature = "canon-names"))]
pub fn frame_designation(id: FrameId) -> &'static str {
    match id {
        FrameId::Leo => "LF-06",
        FrameId::WingZero => "PX-00",
        FrameId::Taurus => "MD-13",
        FrameId::Virgo => "MD-02",
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
    }
}
