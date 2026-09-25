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
}

impl WeaponKind {
    /// Room for 32 kinds on the wire.
    pub const BITS: u32 = 5;
    pub const COUNT: usize = 19;
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
    ];

    pub fn from_bits(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
