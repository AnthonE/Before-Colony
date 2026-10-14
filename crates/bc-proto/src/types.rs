//! Small enums shared by the wire format, the simulation and the clients.

use crate::{BitReader, BitWriter, DecodeError, ROCK_BITS};

/// Who is flying a suit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PilotKind {
    /// A person at a keyboard.
    #[default]
    Human = 0,
    /// An external AI agent connected through the Bot SDK. Shown in-game as a drone (DR) pilot.
    Agent = 1,
    /// A server-side drone NPC.
    Drone = 2,
}

impl PilotKind {
    pub const BITS: u32 = 2;

    pub fn from_bits(v: u32) -> Self {
        match v {
            1 => PilotKind::Agent,
            2 => PilotKind::Drone,
            _ => PilotKind::Human,
        }
    }

    /// Drones and agents are both machine pilots (G-immune drones; agents are labelled DR).
    pub fn is_machine(self) -> bool {
        !matches!(self, PilotKind::Human)
    }
}

/// Suit frame (index into the content tables).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FrameId {
    /// AL-04S Argo (space type): the everyman suit.
    #[default]
    Argo = 0,
    /// PX-00 Altair: carries the Orrery.
    Altair = 1,
    /// AL-11D Corvus: transformable drone.
    Corvus = 2,
    /// AL-09D Cetus: heavy drone.
    Cetus = 3,
    /// PX-01 Antares: gatlings and missile volleys; Full Salvo.
    Antares = 4,
    /// PX-02 Algol: beam scythe and claw shield; Eclipse.
    Algol = 5,
    /// PX-03 Canopus: heat blades and the heaviest armour; Pincer Strike.
    Canopus = 6,
    /// PX-04 Thuban: Talon, flamethrower, beam glaive.
    Thuban = 7,
    /// Altair in Aquila form. Not chosen directly: an Altair transforms into it.
    Aquila = 8,
}

impl FrameId {
    pub const BITS: u32 = 4;
    pub const COUNT: usize = 9;
    pub const ALL: [FrameId; Self::COUNT] = [
        FrameId::Argo,
        FrameId::Altair,
        FrameId::Corvus,
        FrameId::Cetus,
        FrameId::Antares,
        FrameId::Algol,
        FrameId::Canopus,
        FrameId::Thuban,
        FrameId::Aquila,
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
            FrameId::Argo => "argo",
            FrameId::Altair => "altair",
            FrameId::Corvus => "corvus",
            FrameId::Cetus => "cetus",
            FrameId::Antares => "antares",
            FrameId::Algol => "algol",
            FrameId::Canopus => "canopus",
            FrameId::Thuban => "thuban",
            FrameId::Aquila => "aquila",
        }
    }

    /// Parses a [`slug`](Self::slug), ignoring case, spaces, `-` and `_` ("ALTAIR" works), plus the
    /// slugs the frames had before they took the game's own names ([`OLD_SLUGS`](Self::OLD_SLUGS)),
    /// which saved pilots, settings and links still carry. Allocates nothing.
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
        Self::ALL
            .iter()
            .copied()
            .find(|f| same(s, f.slug()))
            .or_else(|| Self::OLD_SLUGS.iter().find(|(a, _)| same(s, a)).map(|(_, f)| *f))
    }

    /// The slugs before 2026-10-14, read and never written.
    pub const OLD_SLUGS: [(&'static str, FrameId); 11] = [
        ("leo", FrameId::Argo),
        ("wingzero", FrameId::Altair),
        ("wing", FrameId::Altair),
        ("zero", FrameId::Altair),
        ("taurus", FrameId::Corvus),
        ("virgo", FrameId::Cetus),
        ("heavyarms", FrameId::Antares),
        ("deathscythe", FrameId::Algol),
        ("sandrock", FrameId::Canopus),
        ("shenlong", FrameId::Thuban),
        ("neobird", FrameId::Aquila),
    ];
}

/// Allegiance. Friendly fire is off between members of the same faction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Faction {
    /// the Almagest (and its drones).
    #[default]
    Almagest = 0,
    /// Colony resistance: the Lodestar pilots' side (Operation Starfall).
    Colonies = 1,
    /// United Earth-Moon space Alliance remnants.
    Alliance = 2,
}

impl Faction {
    pub const BITS: u32 = 3;

    pub fn from_bits(v: u32) -> Self {
        match v {
            1 => Faction::Colonies,
            2 => Faction::Alliance,
            _ => Faction::Almagest,
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
    /// Legs: limb trim mass and auxiliary thrusters.
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
    /// Altair's charged, very thick, very fast beam.
    HeavyBeamRifle = 3,
    /// Heavy drone beam cannon (Cetus).
    BeamCannon = 4,
    /// Antares' arm-mounted beam gatling: a stream of light bolts.
    BeamGatling = 5,
    /// Shoulder-pod homing missiles, fired in salvos; guided once locked on.
    HomingMissile = 6,
    /// Antares' combat knife (melee).
    CombatKnife = 7,
    /// Antares' chest gatlings (Full Salvo only).
    ChestGatling = 8,
    /// Antares' leg-pod micro-missiles (Full Salvo only).
    MicroMissile = 9,
    /// Algol's claw shield: thrown, its beam claws open, slow.
    ClawShield = 10,
    /// Head guns.
    HeadGun = 11,
    /// Algol's beam scythe: long-reach melee.
    BeamScythe = 12,
    /// Canopus's beam machine gun.
    BeamMachineGun = 13,
    /// Canopus's twin heat blades: two blades at once.
    HeatBlade = 14,
    /// Canopus's Pincer Strike: both blades in a pincer.
    PincerStrike = 15,
    /// Thuban's Talon: the right arm extends on its cable to strike ~35 m out.
    Talon = 16,
    /// Thuban's flamethrower: a short cone that burns and heats its target.
    Flamethrower = 17,
    /// Thuban's beam glaive: an overhead chop.
    BeamGlaive = 18,
    /// The beam rifle's charged shot: twice as fast and as thick, held for and let go.
    BeamRifleCharged = 19,
    /// A suit's own reactor, blown by its pilot in its last seconds (its self-destruct): the blast,
    /// which no suit carries as a weapon (v23).
    Reactor = 20,
}

impl WeaponKind {
    /// Room for 32 kinds on the wire.
    pub const BITS: u32 = 5;
    pub const COUNT: usize = 21;
    pub const ALL: [WeaponKind; Self::COUNT] = [
        WeaponKind::BeamRifle,
        WeaponKind::MachineCannon,
        WeaponKind::BeamSaber,
        WeaponKind::HeavyBeamRifle,
        WeaponKind::BeamCannon,
        WeaponKind::BeamGatling,
        WeaponKind::HomingMissile,
        WeaponKind::CombatKnife,
        WeaponKind::ChestGatling,
        WeaponKind::MicroMissile,
        WeaponKind::ClawShield,
        WeaponKind::HeadGun,
        WeaponKind::BeamScythe,
        WeaponKind::BeamMachineGun,
        WeaponKind::HeatBlade,
        WeaponKind::PincerStrike,
        WeaponKind::Talon,
        WeaponKind::Flamethrower,
        WeaponKind::BeamGlaive,
        WeaponKind::BeamRifleCharged,
        WeaponKind::Reactor,
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
        assert_eq!(FrameId::from_slug("ALTAIR"), Some(FrameId::Altair));
        assert_eq!(FrameId::from_slug("Can-o-pus"), Some(FrameId::Canopus));
        // What a saved pilot or an old link still says.
        assert_eq!(FrameId::from_slug("leo"), Some(FrameId::Argo));
        assert_eq!(FrameId::from_slug("Wing Zero"), Some(FrameId::Altair));
        assert_eq!(FrameId::from_slug("DEATH_SCYTHE"), Some(FrameId::Algol));
        assert_eq!(FrameId::from_slug("neobird"), Some(FrameId::Aquila));
        assert_eq!(FrameId::from_slug("argonaut"), None);
        assert_eq!(FrameId::from_slug(""), None);
    }
}
