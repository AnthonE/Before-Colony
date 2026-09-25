use bc_proto::{FrameId, Part, WeaponKind};
use glam::Vec3;

use crate::config::G0;

/// A capsule in the suit's local frame (x right, y up, z forward; origin at the torso centre).
#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

const fn cap(a: [f32; 3], b: [f32; 3], r: f32) -> Capsule {
    Capsule { a: Vec3::new(a[0], a[1], a[2]), b: Vec3::new(b[0], b[1], b[2]), r }
}

/// Where a weapon is carried. Losing the part it hangs on loses the weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSlot {
    Right,
    Left,
    /// Head/shoulder mounts (Wing Zero's machine cannons): tied to the torso.
    Shoulder,
    /// In the head (vulcans).
    Head,
    /// Shoulder missile pods.
    Pods,
    /// Chest hatches (Heavyarms' gatlings).
    Chest,
    /// Leg pods (Heavyarms' micro-missiles).
    LegPods,
    /// Both hands at once (twin blades): needs both arms, and is lost with either.
    Both,
    /// Neo-Bird's nose: the Twin Buster Rifles, pointing along the fuselage.
    Nose,
    /// Neo-Bird's nose guns.
    NoseGuns,
}

impl ArmSlot {
    /// The part the mount hangs on. (A [`Both`](ArmSlot::Both) mount needs the left arm too.)
    pub fn part(self) -> Part {
        match self {
            ArmSlot::Right | ArmSlot::Both | ArmSlot::Nose => Part::ArmR,
            ArmSlot::Left => Part::ArmL,
            ArmSlot::Shoulder | ArmSlot::Pods | ArmSlot::Chest | ArmSlot::NoseGuns => Part::Torso,
            ArmSlot::Head => Part::Head,
            ArmSlot::LegPods => Part::Legs,
        }
    }

    /// Muzzle position in the suit's local frame.
    pub fn muzzle(self) -> Vec3 {
        match self {
            ArmSlot::Right | ArmSlot::Both => Vec3::new(3.4, 0.6, 3.0),
            ArmSlot::Left => Vec3::new(-3.4, 0.6, 3.0),
            ArmSlot::Shoulder => Vec3::new(2.0, 5.4, 1.2),
            ArmSlot::Head => Vec3::new(0.0, 7.2, 1.4),
            ArmSlot::Pods => Vec3::new(0.0, 5.8, 1.6),
            ArmSlot::Chest => Vec3::new(0.0, 3.6, 2.6),
            ArmSlot::LegPods => Vec3::new(0.0, -4.0, 1.8),
            ArmSlot::Nose => Vec3::new(0.0, -0.8, 9.0),
            ArmSlot::NoseGuns => Vec3::new(0.0, 0.8, 7.5),
        }
    }

    /// How far off the body axis this mount can aim (radians).
    pub fn cone(self) -> f32 {
        match self {
            ArmSlot::Right | ArmSlot::Left | ArmSlot::Both => 50f32.to_radians(),
            ArmSlot::Shoulder | ArmSlot::Head | ArmSlot::Chest => 20f32.to_radians(),
            // Missiles leave their pods roughly forward and steer from there.
            ArmSlot::Pods | ArmSlot::LegPods => 60f32.to_radians(),
            ArmSlot::Nose => 2f32.to_radians(),
            ArmSlot::NoseGuns => 8f32.to_radians(),
        }
    }
}

/// What a frame's special key (the MODE state or the SPECIAL press) does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecialKind {
    None,
    /// Toggle (MODE): change into frame `to` over `ticks`, with weapons down and main thrust at
    /// `thrust` of normal meanwhile (Wing Zero ↔ Neo-Bird).
    Transform {
        to: FrameId,
        ticks: u8,
        thrust: f32,
    },
    /// Toggle (MODE): the Hyper Jammer. Enemy sensors see the suit at `sig` of its signature, and
    /// eyes only within `visual` m. It drains `drain` energy/s, needs `min_energy` (0..1) to engage,
    /// and firing or striking breaks it for `break_ticks`.
    HyperJammer {
        drain: f32,
        min_energy: f32,
        break_ticks: u16,
        sig: f32,
        visual: f32,
    },
    /// Press (SPECIAL): Full Open Attack. Every ranged weapon (the special mounts too) fires along
    /// the aim for `ticks`, heat ignored; then a forced overheat for `lockout` ticks. Ready again
    /// `cooldown` ticks after it starts.
    FullOpen {
        ticks: u16,
        lockout: u16,
        cooldown: u16,
    },
    /// Press (SPECIAL): a melee move with the special mount's weapon (Cross Crusher), ready again
    /// `cooldown` ticks after it starts.
    MeleeMove {
        cooldown: u16,
    },
}

impl SpecialKind {
    /// Held as the MODE state (a toggle), rather than pressed as SPECIAL.
    pub fn is_toggle(self) -> bool {
        matches!(self, SpecialKind::Transform { .. } | SpecialKind::HyperJammer { .. })
    }
}

/// How the kit-aware AI (agents, the browser autopilot) flies a frame.
#[derive(Clone, Copy, Debug)]
pub struct AiHints {
    /// Preferred fighting distance, m (0: from the primary weapon's range).
    pub engage_range: f32,
    /// Closes to melee rather than shooting from range.
    pub melee_first: bool,
    /// How dangerous the ZERO System rates it (1 = a Leo).
    pub threat: f32,
}

/// A weapon on a mount.
#[derive(Clone, Copy, Debug)]
pub struct Mount {
    pub weapon: WeaponKind,
    pub arm: ArmSlot,
}

/// Everything the simulation needs to know about a mobile-suit frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameSpec {
    pub id: FrameId,
    /// Mass without propellant, kg.
    pub dry_mass: f32,
    pub propellant_cap: f32,
    /// Specific impulse, s. Exhaust velocity is `isp * G0`.
    pub isp: f32,
    /// Main (forward) thrust, N.
    pub main_thrust: f32,
    /// Lateral and vertical thrust, N.
    pub side_thrust: f32,
    /// Retro (backward) thrust, N.
    pub retro_thrust: f32,
    /// Main-thruster multiplier while boosting.
    pub boost_mult: f32,
    /// AMBAC: attitude control by moving limbs. No propellant, limited authority.
    pub ambac_accel: f32,
    pub ambac_rate: f32,
    /// RCS thrusters add this much attitude authority, at a propellant cost.
    pub rcs_accel: f32,
    pub rcs_rate: f32,
    /// Propellant per rad/s of angular velocity change delivered by RCS, kg.
    pub rcs_propellant: f32,
    pub roll_rate: f32,
    /// Flight-assist cruise speed, m/s (boost raises it by 1.8×).
    pub fa_speed: f32,
    /// Bounding radius for broad-phase tests.
    pub radius: f32,
    /// Hit capsules, indexed by [`Part`].
    pub capsules: [Capsule; Part::COUNT],
    /// Armour points per part.
    pub part_hp: [f32; Part::COUNT],
    /// Damage multiplier (gundanium ≈ 0.55).
    pub armor: f32,
    pub sensor_range: f32,
    /// Multiplies other suits' sensor range against this one.
    pub signature: f32,
    pub energy_cap: f32,
    pub energy_regen: f32,
    pub heat_cap: f32,
    pub heat_dissipation: f32,
    /// Primary (LMB), secondary (RMB), melee (F).
    pub loadout: [Option<Mount>; 3],
    /// Carries the ZERO System.
    pub zero: bool,
    /// Pilots and agents may choose it (Mobile Dolls' frames and alternate forms: no).
    pub playable: bool,
    /// What the frame's special key does.
    pub special: SpecialKind,
    /// Weapons only the special uses (Full Open's chest gatlings and micro-missiles, the Cross
    /// Crusher).
    pub special_mounts: [Option<Mount>; 2],
    pub ai: AiHints,
}

impl FrameSpec {
    /// Wet mass for a given propellant load.
    #[inline]
    pub fn mass(&self, propellant: f32) -> f32 {
        self.dry_mass + propellant.max(0.0)
    }

    /// Exhaust velocity, m/s.
    #[inline]
    pub fn exhaust_velocity(&self) -> f32 {
        self.isp * G0
    }

    /// Maximum acceleration along a local axis with a full tank, m/s².
    pub fn max_accel_along(&self, local_dir: Vec3) -> f32 {
        let m = self.mass(self.propellant_cap);
        let f = if local_dir.z > 0.5 {
            self.main_thrust
        } else if local_dir.z < -0.5 {
            self.retro_thrust
        } else {
            self.side_thrust
        };
        f / m
    }
}

/// Human-shaped hitboxes, about 17 m tall.
const HUMANOID: [Capsule; Part::COUNT] = [
    cap([0.0, 6.4, 0.0], [0.0, 7.6, 0.2], 1.3),   // head
    cap([0.0, 0.8, 0.0], [0.0, 4.6, 0.0], 2.5),   // torso
    cap([-3.3, 4.6, 0.0], [-3.5, 0.2, 1.2], 1.1), // left arm
    cap([3.3, 4.6, 0.0], [3.5, 0.2, 1.2], 1.1),   // right arm
    cap([0.0, 0.2, 0.0], [0.0, -8.4, 0.3], 2.1),  // legs
    cap([0.0, 3.0, -2.4], [0.0, 5.2, -2.8], 1.5), // backpack
];

/// Neo-Bird's hitboxes: the folded suit as an aircraft, nose along +z.
const BIRD: [Capsule; Part::COUNT] = [
    cap([0.0, 0.5, 5.5], [0.0, 0.5, 7.5], 1.2),    // head: the nose
    cap([0.0, 0.0, -4.5], [0.0, 0.0, 5.0], 2.3),   // torso: the fuselage
    cap([-1.5, 0.0, 0.0], [-8.0, 0.0, -2.0], 1.0), // left arm: the left wing
    cap([1.5, 0.0, 0.0], [8.0, 0.0, -2.0], 1.0),   // right arm: the right wing and buster rifles
    cap([0.0, 0.0, -4.5], [0.0, 0.5, -10.0], 1.6), // legs: the tail
    cap([0.0, 1.8, -1.0], [0.0, 2.2, -4.0], 1.4),  // backpack: the dorsal thrusters
];

const fn thrust_for(g: f32, wet_mass: f32) -> f32 {
    g * G0 * wet_mass
}

const RIGHT: ArmSlot = ArmSlot::Right;
const LEFT: ArmSlot = ArmSlot::Left;

const fn mount(weapon: WeaponKind, arm: ArmSlot) -> Option<Mount> {
    Some(Mount { weapon, arm })
}

const fn hints(engage_range: f32, melee_first: bool, threat: f32) -> AiHints {
    AiHints { engage_range, melee_first, threat }
}

/// Neo-Bird ↔ Wing Zero: 0.8 s, weapons down, a third of the main thrust.
const TRANSFORM_TICKS: u8 = 24;
const TRANSFORM_THRUST: f32 = 0.3;

/// The frames pilots can choose, in the order of the respawn keys (1–6).
pub const PLAYABLE_ORDER: [FrameId; 6] = [
    FrameId::Leo,
    FrameId::WingZero,
    FrameId::Heavyarms,
    FrameId::Deathscythe,
    FrameId::Sandrock,
    FrameId::Shenlong,
];

static FRAMES: [FrameSpec; FrameId::COUNT] = [
    // OZ-06MS Leo (space): the everyman suit. 3.5 g, 5.6 g on boost.
    FrameSpec {
        id: FrameId::Leo,
        dry_mass: 7_100.0,
        propellant_cap: 2_400.0,
        isp: 900.0,
        main_thrust: thrust_for(3.5, 9_500.0),
        side_thrust: thrust_for(1.75, 9_500.0),
        retro_thrust: thrust_for(2.1, 9_500.0),
        boost_mult: 1.6,
        ambac_accel: 1.4,
        ambac_rate: 1.0,
        rcs_accel: 4.0,
        rcs_rate: 2.5,
        rcs_propellant: 1.2,
        roll_rate: 1.4,
        fa_speed: 220.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [60.0, 220.0, 90.0, 90.0, 120.0, 80.0],
        armor: 1.0,
        sensor_range: 6_000.0,
        signature: 1.0,
        energy_cap: 100.0,
        energy_regen: 12.0,
        heat_cap: 100.0,
        heat_dissipation: 18.0,
        loadout: [
            Some(Mount { weapon: WeaponKind::BeamRifle, arm: RIGHT }),
            Some(Mount { weapon: WeaponKind::MachineCannon, arm: LEFT }),
            Some(Mount { weapon: WeaponKind::BeamSaber, arm: LEFT }),
        ],
        zero: false,
        playable: true,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.0),
    },
    // XXXG-00W0 Wing Gundam Zero: gundanium armour, 8 g (12 g boost: past what a pilot can take),
    // Twin Buster Rifle and the ZERO System.
    FrameSpec {
        id: FrameId::WingZero,
        dry_mass: 8_000.0,
        propellant_cap: 3_000.0,
        isp: 1_200.0,
        main_thrust: thrust_for(8.0, 11_000.0),
        side_thrust: thrust_for(4.4, 11_000.0),
        retro_thrust: thrust_for(5.2, 11_000.0),
        boost_mult: 1.5,
        ambac_accel: 2.0,
        ambac_rate: 1.4,
        rcs_accel: 6.0,
        rcs_rate: 3.5,
        rcs_propellant: 1.6,
        roll_rate: 2.0,
        fa_speed: 300.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [90.0, 380.0, 160.0, 160.0, 200.0, 150.0],
        armor: 0.55,
        sensor_range: 10_000.0,
        signature: 1.1,
        energy_cap: 160.0,
        energy_regen: 20.0,
        heat_cap: 140.0,
        heat_dissipation: 26.0,
        loadout: [
            Some(Mount { weapon: WeaponKind::TwinBusterRifle, arm: RIGHT }),
            Some(Mount { weapon: WeaponKind::MachineCannon, arm: ArmSlot::Shoulder }),
            Some(Mount { weapon: WeaponKind::BeamSaber, arm: LEFT }),
        ],
        zero: true,
        playable: true,
        special: SpecialKind::Transform {
            to: FrameId::WingZeroBird,
            ticks: TRANSFORM_TICKS,
            thrust: TRANSFORM_THRUST,
        },
        special_mounts: [None, None],
        ai: hints(0.0, false, 2.0),
    },
    // OZ-13MS Taurus: agile Mobile Doll, 5 g.
    FrameSpec {
        id: FrameId::Taurus,
        dry_mass: 6_500.0,
        propellant_cap: 2_000.0,
        isp: 900.0,
        main_thrust: thrust_for(5.0, 8_500.0),
        side_thrust: thrust_for(2.75, 8_500.0),
        retro_thrust: thrust_for(3.0, 8_500.0),
        boost_mult: 1.5,
        ambac_accel: 1.8,
        ambac_rate: 1.2,
        rcs_accel: 6.0,
        rcs_rate: 3.0,
        rcs_propellant: 1.0,
        roll_rate: 1.8,
        fa_speed: 260.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [50.0, 170.0, 70.0, 70.0, 90.0, 70.0],
        armor: 1.0,
        sensor_range: 7_000.0,
        signature: 0.9,
        energy_cap: 100.0,
        energy_regen: 14.0,
        heat_cap: 100.0,
        heat_dissipation: 20.0,
        loadout: [Some(Mount { weapon: WeaponKind::BeamRifle, arm: RIGHT }), None, None],
        zero: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.0),
    },
    // OZ-02MD Virgo: heavy Mobile Doll with a beam cannon, 3 g.
    FrameSpec {
        id: FrameId::Virgo,
        dry_mass: 9_500.0,
        propellant_cap: 2_500.0,
        isp: 900.0,
        main_thrust: thrust_for(3.0, 12_000.0),
        side_thrust: thrust_for(1.35, 12_000.0),
        retro_thrust: thrust_for(1.5, 12_000.0),
        boost_mult: 1.3,
        ambac_accel: 1.0,
        ambac_rate: 0.8,
        rcs_accel: 3.0,
        rcs_rate: 2.0,
        rcs_propellant: 1.4,
        roll_rate: 1.0,
        fa_speed: 180.0,
        radius: 11.0,
        capsules: HUMANOID,
        part_hp: [80.0, 300.0, 120.0, 120.0, 150.0, 120.0],
        armor: 0.8,
        sensor_range: 8_000.0,
        signature: 1.3,
        energy_cap: 200.0,
        energy_regen: 25.0,
        heat_cap: 160.0,
        heat_dissipation: 24.0,
        loadout: [Some(Mount { weapon: WeaponKind::BeamCannon, arm: RIGHT }), None, None],
        zero: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.3),
    },
    // XXXG-01H Gundam Heavyarms: a walking arsenal. Gatlings, missile volleys, and the Full Open
    // Attack; gundanium armour, 5.2 g.
    FrameSpec {
        id: FrameId::Heavyarms,
        dry_mass: 8_800.0,
        propellant_cap: 2_600.0,
        isp: 1_100.0,
        main_thrust: thrust_for(5.2, 11_400.0),
        side_thrust: thrust_for(2.6, 11_400.0),
        retro_thrust: thrust_for(3.0, 11_400.0),
        boost_mult: 1.4,
        ambac_accel: 1.5,
        ambac_rate: 1.0,
        rcs_accel: 5.0,
        rcs_rate: 3.0,
        rcs_propellant: 1.5,
        roll_rate: 1.6,
        fa_speed: 240.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [80.0, 360.0, 170.0, 170.0, 200.0, 150.0],
        armor: 0.55,
        sensor_range: 9_000.0,
        signature: 1.25,
        energy_cap: 130.0,
        energy_regen: 16.0,
        heat_cap: 170.0,
        heat_dissipation: 30.0,
        loadout: [
            mount(WeaponKind::BeamGatling, RIGHT),
            mount(WeaponKind::HomingMissile, ArmSlot::Pods),
            mount(WeaponKind::ArmyKnife, LEFT),
        ],
        zero: false,
        playable: false,
        special: SpecialKind::FullOpen { ticks: 90, lockout: 150, cooldown: 900 },
        special_mounts: [
            mount(WeaponKind::ChestGatling, ArmSlot::Chest),
            mount(WeaponKind::MicroMissile, ArmSlot::LegPods),
        ],
        ai: hints(1_800.0, false, 1.8),
    },
    // XXXG-01D Gundam Deathscythe: the god of death. Beam scythe and buster shield, and the Hyper
    // Jammer that hides it from sensors; 7 g.
    FrameSpec {
        id: FrameId::Deathscythe,
        dry_mass: 7_300.0,
        propellant_cap: 2_500.0,
        isp: 1_150.0,
        main_thrust: thrust_for(7.0, 9_800.0),
        side_thrust: thrust_for(4.0, 9_800.0),
        retro_thrust: thrust_for(4.6, 9_800.0),
        boost_mult: 1.6,
        ambac_accel: 2.3,
        ambac_rate: 1.6,
        rcs_accel: 6.5,
        rcs_rate: 3.8,
        rcs_propellant: 1.3,
        roll_rate: 2.2,
        fa_speed: 320.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [80.0, 330.0, 150.0, 150.0, 180.0, 140.0],
        armor: 0.55,
        sensor_range: 8_000.0,
        signature: 0.85,
        energy_cap: 150.0,
        energy_regen: 18.0,
        heat_cap: 120.0,
        heat_dissipation: 24.0,
        loadout: [
            mount(WeaponKind::BusterShield, LEFT),
            mount(WeaponKind::HeadVulcan, ArmSlot::Head),
            mount(WeaponKind::BeamScythe, RIGHT),
        ],
        zero: false,
        playable: false,
        special: SpecialKind::HyperJammer {
            drain: 30.0,
            min_energy: 0.2,
            break_ticks: 60,
            sig: 0.05,
            visual: 400.0,
        },
        special_mounts: [None, None],
        ai: hints(0.0, true, 1.7),
    },
    // XXXG-01SR Gundam Sandrock: built for the desert and the heaviest armour of the five. Beam
    // machine gun, homing missiles, twin heat shotels and the Cross Crusher; 4.6 g.
    FrameSpec {
        id: FrameId::Sandrock,
        dry_mass: 9_600.0,
        propellant_cap: 2_700.0,
        isp: 1_000.0,
        main_thrust: thrust_for(4.6, 12_300.0),
        side_thrust: thrust_for(2.3, 12_300.0),
        retro_thrust: thrust_for(2.7, 12_300.0),
        boost_mult: 1.5,
        ambac_accel: 1.5,
        ambac_rate: 1.0,
        rcs_accel: 5.0,
        rcs_rate: 2.8,
        rcs_propellant: 1.6,
        roll_rate: 1.4,
        fa_speed: 230.0,
        radius: 10.5,
        capsules: HUMANOID,
        part_hp: [100.0, 420.0, 190.0, 190.0, 230.0, 170.0],
        armor: 0.45,
        sensor_range: 8_000.0,
        signature: 1.25,
        energy_cap: 150.0,
        energy_regen: 18.0,
        heat_cap: 150.0,
        heat_dissipation: 28.0,
        loadout: [
            mount(WeaponKind::BeamMachineGun, RIGHT),
            mount(WeaponKind::HomingMissile, ArmSlot::Pods),
            mount(WeaponKind::HeatShotel, ArmSlot::Both),
        ],
        zero: false,
        playable: false,
        special: SpecialKind::MeleeMove { cooldown: 240 },
        special_mounts: [mount(WeaponKind::CrossCrusher, ArmSlot::Both), None],
        ai: hints(1_200.0, false, 1.6),
    },
    // XXXG-01S Shenlong Gundam: a duellist. The Dragon Fang (its right arm, flung out on a cable),
    // a flamethrower in the same arm, and a beam glaive; 7.4 g.
    FrameSpec {
        id: FrameId::Shenlong,
        dry_mass: 7_500.0,
        propellant_cap: 2_700.0,
        isp: 1_100.0,
        main_thrust: thrust_for(7.4, 10_200.0),
        side_thrust: thrust_for(4.5, 10_200.0),
        retro_thrust: thrust_for(5.0, 10_200.0),
        boost_mult: 1.6,
        ambac_accel: 2.2,
        ambac_rate: 1.5,
        rcs_accel: 6.5,
        rcs_rate: 3.8,
        rcs_propellant: 1.3,
        roll_rate: 2.2,
        fa_speed: 330.0,
        radius: 10.0,
        capsules: HUMANOID,
        part_hp: [80.0, 340.0, 150.0, 200.0, 190.0, 140.0],
        armor: 0.55,
        sensor_range: 7_000.0,
        signature: 1.0,
        energy_cap: 130.0,
        energy_regen: 18.0,
        heat_cap: 140.0,
        heat_dissipation: 30.0,
        loadout: [
            mount(WeaponKind::DragonFang, RIGHT),
            mount(WeaponKind::Flamethrower, RIGHT),
            mount(WeaponKind::BeamGlaive, LEFT),
        ],
        zero: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, true, 1.7),
    },
    // Wing Gundam Zero as Neo-Bird: faster in a straight line, clumsy in turns, the Twin Buster
    // Rifles fixed along the nose and no melee. Its armour, energy, heat and tank are Wing Zero's.
    FrameSpec {
        id: FrameId::WingZeroBird,
        dry_mass: 8_000.0,
        propellant_cap: 3_000.0,
        isp: 1_200.0,
        main_thrust: thrust_for(8.5, 11_000.0),
        side_thrust: thrust_for(1.5, 11_000.0),
        retro_thrust: thrust_for(2.0, 11_000.0),
        boost_mult: 1.4,
        ambac_accel: 0.8,
        ambac_rate: 0.6,
        rcs_accel: 4.5,
        rcs_rate: 2.2,
        rcs_propellant: 1.6,
        roll_rate: 3.0,
        fa_speed: 600.0,
        radius: 11.0,
        capsules: BIRD,
        part_hp: [90.0, 380.0, 160.0, 160.0, 200.0, 150.0],
        armor: 0.55,
        sensor_range: 10_000.0,
        signature: 1.0,
        energy_cap: 160.0,
        energy_regen: 20.0,
        heat_cap: 140.0,
        heat_dissipation: 26.0,
        loadout: [
            mount(WeaponKind::TwinBusterRifle, ArmSlot::Nose),
            mount(WeaponKind::MachineCannon, ArmSlot::NoseGuns),
            None,
        ],
        zero: true,
        playable: false,
        special: SpecialKind::Transform {
            to: FrameId::WingZero,
            ticks: TRANSFORM_TICKS,
            thrust: TRANSFORM_THRUST,
        },
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.8),
    },
];

// Every row sits at its frame's index.
const _: () = {
    let mut i = 0;
    while i < FRAMES.len() {
        assert!(FRAMES[i].id as usize == i, "FRAMES row out of order");
        i += 1;
    }
};

/// The spec for a frame.
#[inline]
pub fn frame(id: FrameId) -> &'static FrameSpec {
    &FRAMES[id.index()]
}
