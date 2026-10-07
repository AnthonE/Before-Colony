//! Small enums shared by the wire format, the simulation and the clients.

use crate::{BitReader, BitWriter, DecodeError, ROCK_BITS};

/// Who is flying a suit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PilotKind {
    /// A person at a keyboard.
    #[default]
    Human = 0,
    /// An external AI agent connected through the Bot SDK. Shown in-game as a Mobile Doll (MD) pilot.
    Agent = 1,
    /// A server-side Mobile Doll NPC.
    MobileDoll = 2,
}

impl PilotKind {
    pub const BITS: u32 = 2;

    pub fn from_bits(v: u32) -> Self {
        match v {
            1 => PilotKind::Agent,
            2 => PilotKind::MobileDoll,
            _ => PilotKind::Human,
        }
    }

    /// Mobile Dolls and agents are both machine pilots (G-immune dolls; agents are labelled MD).
    pub fn is_machine(self) -> bool {
        !matches!(self, PilotKind::Human)
    }
}

/// Mobile suit frame (index into the content tables).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FrameId {
    /// OZ-06MS Leo (space type): the everyman suit.
    #[default]
    Leo = 0,
    /// XXXG-00W0 Wing Gundam Zero: carries the ZERO System.
    WingZero = 1,
    /// OZ-13MS Taurus: transformable Mobile Doll.
    Taurus = 2,
    /// OZ-02MD Virgo: heavy Mobile Doll.
    Virgo = 3,
    /// XXXG-01H Gundam Heavyarms: gatlings and missile volleys; Full Open Attack.
    Heavyarms = 4,
    /// XXXG-01D Gundam Deathscythe: beam scythe and buster shield; Hyper Jammer.
    Deathscythe = 5,
    /// XXXG-01SR Gundam Sandrock: heat shotels and the heaviest armour; Cross Crusher.
    Sandrock = 6,
    /// XXXG-01S Shenlong Gundam: Dragon Fang, flamethrower, beam glaive.
    Shenlong = 7,
    /// Wing Gundam Zero in Neo-Bird form. Not chosen directly: a Wing Zero transforms into it.
    WingZeroBird = 8,
}

impl FrameId {
    pub const BITS: u32 = 4;
    pub const COUNT: usize = 9;
    pub const ALL: [FrameId; Self::COUNT] = [
        FrameId::Leo,
        FrameId::WingZero,
        FrameId::Taurus,
        FrameId::Virgo,
        FrameId::Heavyarms,
        FrameId::Deathscythe,
        FrameId::Sandrock,
        FrameId::Shenlong,
        FrameId::WingZeroBird,
    ];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Short lowercase name for URLs and command lines.
    pub fn slug(self) -> &'static str {
        match self {
            FrameId::Leo => "leo",
            FrameId::WingZero => "wingzero",
            FrameId::Taurus => "taurus",
            FrameId::Virgo => "virgo",
            FrameId::Heavyarms => "heavyarms",
            FrameId::Deathscythe => "deathscythe",
            FrameId::Sandrock => "sandrock",
            FrameId::Shenlong => "shenlong",
            FrameId::WingZeroBird => "neobird",
        }
    }

    /// Parses a [`slug`](Self::slug), ignoring case, spaces, `-` and `_` ("Wing-Zero" works), plus
    /// the short forms `wing` and `zero`. Allocates nothing.
    pub fn from_slug(s: &str) -> Option<Self> {
        fn same(input: &str, slug: &str) -> bool {
            let mut a =
                input.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).map(|c| c.to_ascii_lowercase());
            let mut b = slug.chars();
            loop {
                match (a.next(), b.next()) {
                    (None, None) => return true,
                    (Some(x), Some(y)) if x == y => {}
                    _ => return false,
                }
            }
        }
        const SHORT: [(&str, FrameId); 2] = [("wing", FrameId::WingZero), ("zero", FrameId::WingZero)];
        Self::ALL
            .iter()
            .copied()
            .find(|f| same(s, f.slug()))
            .or_else(|| SHORT.iter().find(|(a, _)| same(s, a)).map(|(_, f)| *f))
    }
}

/// Allegiance. Friendly fire is off between members of the same faction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Faction {
    /// Organization of the Zodiac (and its Mobile Dolls).
    #[default]
    Oz = 0,
    /// Colony resistance: the Gundam pilots' side (Operation Meteor).
    Colonies = 1,
    /// United Earth Sphere Alliance remnants.
    Alliance = 2,
}

impl Faction {
    pub const BITS: u32 = 3;

    pub fn from_bits(v: u32) -> Self {
        match v {
            1 => Faction::Colonies,
            2 => Faction::Alliance,
            _ => Faction::Oz,
        }
    }
}

/// Hit locations. Each has its own armour pool and consequences when destroyed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Part {
    /// Main camera and sensors: losing it cuts sensor range.
    Head = 0,
    /// Cockpit and reactor: losing it destroys the suit.
    Torso = 1,
    /// Left arm: shield and melee.
    ArmL = 2,
    /// Right arm: primary weapon.
    ArmR = 3,
    /// Legs: AMBAC mass and auxiliary thrusters.
    Legs = 4,
    /// Backpack: main thrusters.
    Backpack = 5,
}

impl Part {
    pub const COUNT: usize = 6;
    pub const BITS: u32 = 3;
    pub const ALL: [Part; Self::COUNT] =
        [Part::Head, Part::Torso, Part::ArmL, Part::ArmR, Part::Legs, Part::Backpack];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }
}

/// Weapon archetypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WeaponKind {
    /// Energy projectile, ~4 km/s: dodgeable at range, needs a lead.
    BeamRifle = 0,
    /// Ballistic rounds, high rate of fire, finite ammunition.
    MachineCannon = 1,
    /// Melee arc.
    BeamSaber = 2,
    /// Wing Zero's charged, very thick, very fast beam.
    TwinBusterRifle = 3,
    /// Heavy Mobile Doll beam cannon (Virgo).
    BeamCannon = 4,
    /// Heavyarms' arm-mounted beam gatling: a stream of light bolts.
    BeamGatling = 5,
    /// Shoulder-pod homing missiles, fired in salvos; guided once locked on.
    HomingMissile = 6,
    /// Heavyarms' army knife (melee).
    ArmyKnife = 7,
    /// Heavyarms' chest gatlings (Full Open Attack only).
    ChestGatling = 8,
    /// Heavyarms' leg-pod micro-missiles (Full Open Attack only).
    MicroMissile = 9,
    /// Deathscythe's buster shield: thrown, its beam claws open, slow.
    BusterShield = 10,
    /// Head vulcans.
    HeadVulcan = 11,
    /// Deathscythe's beam scythe: long-reach melee.
    BeamScythe = 12,
    /// Sandrock's beam machine gun.
    BeamMachineGun = 13,
    /// Sandrock's twin heat shotels: two blades at once.
    HeatShotel = 14,
    /// Sandrock's Cross Crusher: both shotels in a pincer.
    CrossCrusher = 15,
    /// Shenlong's Dragon Fang: the right arm extends on its cable to strike ~35 m out.
    DragonFang = 16,
    /// Shenlong's flamethrower: a short cone that burns and heats its target.
    Flamethrower = 17,
    /// Shenlong's beam glaive: an overhead chop.
    BeamGlaive = 18,
    /// The beam rifle's charged shot: twice as fast and as thick, held for and let go.
    BeamRifleCharged = 19,
}

impl WeaponKind {
    /// Room for 32 kinds on the wire.
    pub const BITS: u32 = 5;
    pub const COUNT: usize = 20;
    pub const ALL: [WeaponKind; Self::COUNT] = [
        WeaponKind::BeamRifle,
        WeaponKind::MachineCannon,
        WeaponKind::BeamSaber,
        WeaponKind::TwinBusterRifle,
        WeaponKind::BeamCannon,
        WeaponKind::BeamGatling,
        WeaponKind::HomingMissile,
        WeaponKind::ArmyKnife,
        WeaponKind::ChestGatling,
        WeaponKind::MicroMissile,
        WeaponKind::BusterShield,
        WeaponKind::HeadVulcan,
        WeaponKind::BeamScythe,
        WeaponKind::BeamMachineGun,
        WeaponKind::HeatShotel,
        WeaponKind::CrossCrusher,
        WeaponKind::DragonFang,
        WeaponKind::Flamethrower,
        WeaponKind::BeamGlaive,
        WeaponKind::BeamRifleCharged,
    ];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Landmark ids on the wire use this many bits (a sector could name 16).
pub const LANDMARK_BITS: u32 = 4;
/// Bay numbers on the wire use this many bits (the colony's bay ring has 99).
pub const BAY_BITS: u32 = 7;
/// A rider's velocity over its body is sent over ±this many m/s: more than a suit in a grip can
/// keep (30 m/s lets go) or a blade's ground dash reaches (28 m/s), so it is never clamped.
pub const RIDER_VEL_MAX: f32 = 32.0;
/// Bits per axis of a rider's velocity over its body (6.26 cm/s steps, on a grid where zero is
/// exact: a suit at rest on its body comes out at rest, `quant::quantize_centered`).
pub const RIDER_VEL_BITS: u32 = 10;

/// The body a suit stands on, is in the grip of, or is parked on, as the wire names it. Its pose
/// never travels: rocks come from the Welcome's field and don't move, landmarks and the bays are
/// compiled content whose pose is a closed form in the tick (`bc_sim::bodies`,
/// `bc_sim::colony::hub`), and the colony's city stands still in an interior sector's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyRef {
    /// An asteroid of the field, by index.
    Rock(u16),
    /// A landmark of the sector (`bc_sim::content::landmarks`), by index.
    Landmark(u8),
    /// The colony's inside: its floor, its city and its end caps (only in an interior sector,
    /// whose frame is the colony's own).
    City,
    /// A pilot's bay on the colony's bay ring, by number: a launching suit rides its catapult's
    /// cradle there until its pilot lets go (only outside the colony).
    Bay(u8),
}

impl BodyRef {
    /// The kind: 0 a rock, 1 a landmark, 2 the city, 3 a bay.
    pub const KIND_BITS: u32 = 2;
    /// The most bits [`write`](Self::write) takes (a rock's).
    pub const MAX_BITS: usize = (Self::KIND_BITS + ROCK_BITS) as usize;

    /// Encoded size, in bits: the kind and the id.
    pub fn bits(self) -> usize {
        Self::KIND_BITS as usize
            + match self {
                BodyRef::Rock(_) => ROCK_BITS as usize,
                BodyRef::Landmark(_) => LANDMARK_BITS as usize,
                BodyRef::City => 0,
                BodyRef::Bay(_) => BAY_BITS as usize,
            }
    }

    /// How far from the body's origin a rider can be on each axis, m: past the largest rock's
    /// surface or the largest landmark's, by more than a grip lets a suit fly off; anywhere
    /// between the colony's end caps (16 km from its middle) for the city; well round a bay's
    /// cradle, where its rider stands still.
    pub fn local_max(self) -> f32 {
        match self {
            BodyRef::Rock(_) | BodyRef::Bay(_) => 256.0,
            BodyRef::Landmark(_) => 1_024.0,
            BodyRef::City => 16_384.0,
        }
    }

    /// Bits per axis of a rider's position over [`local_max`](Self::local_max): 1.5625 cm steps
    /// on every kind.
    pub fn local_bits(self) -> u32 {
        match self {
            BodyRef::Rock(_) | BodyRef::Bay(_) => 15,
            BodyRef::Landmark(_) => 17,
            BodyRef::City => 21,
        }
    }

    pub fn write(self, w: &mut BitWriter<'_>) {
        match self {
            BodyRef::Rock(r) => {
                w.write_bits(0, Self::KIND_BITS);
                w.write_bits(u32::from(r).min((1 << ROCK_BITS) - 1), ROCK_BITS);
            }
            BodyRef::Landmark(k) => {
                w.write_bits(1, Self::KIND_BITS);
                w.write_bits(u32::from(k).min((1 << LANDMARK_BITS) - 1), LANDMARK_BITS);
            }
            BodyRef::City => w.write_bits(2, Self::KIND_BITS),
            BodyRef::Bay(n) => {
                w.write_bits(3, Self::KIND_BITS);
                w.write_bits(u32::from(n).min((1 << BAY_BITS) - 1), BAY_BITS);
            }
        }
    }

    /// Reads a body reference. Whether the body exists in this sector (a bay is 1..=99, and
    /// outside the colony) is the client's to check.
    pub fn read(r: &mut BitReader<'_>) -> Result<Self, DecodeError> {
        match r.read_bits(Self::KIND_BITS) {
            0 => Ok(BodyRef::Rock(r.read_bits(ROCK_BITS) as u16)),
            1 => Ok(BodyRef::Landmark(r.read_bits(LANDMARK_BITS) as u8)),
            2 => Ok(BodyRef::City),
            _ => Ok(BodyRef::Bay(r.read_bits(BAY_BITS) as u8)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_refs_round_trip() {
        let refs = [
            BodyRef::Rock(0),
            BodyRef::Rock(1_022),
            BodyRef::Rock(1_023),
            BodyRef::Landmark(0),
            BodyRef::City,
            BodyRef::Bay(1),
            BodyRef::Bay(99),
            BodyRef::Bay(127),
        ];
        for b in refs.into_iter().chain((0..16).map(BodyRef::Landmark)) {
            let mut buf = [0u8; 4];
            let mut w = BitWriter::new(&mut buf);
            b.write(&mut w);
            assert_eq!(w.bits_written(), b.bits(), "{b:?}");
            assert!(b.bits() <= BodyRef::MAX_BITS);
            assert_eq!(BodyRef::read(&mut BitReader::new(&buf)), Ok(b));
        }
        assert_eq!(
            (
                BodyRef::Rock(5).bits(),
                BodyRef::Landmark(5).bits(),
                BodyRef::City.bits(),
                BodyRef::Bay(5).bits()
            ),
            (12, 6, 2, 9)
        );
        // Every kind places a rider to the same 1.5625 cm.
        for b in [BodyRef::Rock(0), BodyRef::Landmark(0), BodyRef::City, BodyRef::Bay(1)] {
            let step = crate::quant::signed_step(b.local_max(), b.local_bits());
            assert!((step - 0.015_625).abs() < 1e-6, "{b:?}: {step}");
        }
    }

    #[test]
    fn slugs_round_trip_and_forgive_spelling() {
        for f in FrameId::ALL {
            assert_eq!(FrameId::from_slug(f.slug()), Some(f));
        }
        assert_eq!(FrameId::from_slug("Wing-Zero"), Some(FrameId::WingZero));
        assert_eq!(FrameId::from_slug("zero"), Some(FrameId::WingZero));
        assert_eq!(FrameId::from_slug("DEATH_SCYTHE"), Some(FrameId::Deathscythe));
        assert_eq!(FrameId::from_slug("leopard"), None);
        assert_eq!(FrameId::from_slug(""), None);
    }
}
