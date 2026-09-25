//! Small enums shared by the wire format, the simulation and the clients.

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
}

impl FrameId {
    pub const BITS: u32 = 4;
    pub const COUNT: usize = 4;
    pub const ALL: [FrameId; Self::COUNT] =
        [FrameId::Leo, FrameId::WingZero, FrameId::Taurus, FrameId::Virgo];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
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
}

impl WeaponKind {
    /// Room for 32 kinds on the wire.
    pub const BITS: u32 = 5;
    pub const COUNT: usize = 5;
    pub const ALL: [WeaponKind; Self::COUNT] = [
        WeaponKind::BeamRifle,
        WeaponKind::MachineCannon,
        WeaponKind::BeamSaber,
        WeaponKind::TwinBusterRifle,
        WeaponKind::BeamCannon,
    ];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Beam weapons are replicated as spawn events; cannon rounds are drawn from firing flags.
    pub fn is_beam(self) -> bool {
        matches!(self, WeaponKind::BeamRifle | WeaponKind::TwinBusterRifle | WeaponKind::BeamCannon)
    }
}
