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

/// Where a weapon is carried. Losing the arm loses the weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSlot {
    Right,
    Left,
    /// Head/shoulder mounts (Wing Zero's machine cannons): tied to the torso.
    Shoulder,
}

impl ArmSlot {
    pub fn part(self) -> Part {
        match self {
            ArmSlot::Right => Part::ArmR,
            ArmSlot::Left => Part::ArmL,
            ArmSlot::Shoulder => Part::Torso,
        }
    }

    /// Muzzle position in the suit's local frame.
    pub fn muzzle(self) -> Vec3 {
        match self {
            ArmSlot::Right => Vec3::new(3.4, 0.6, 3.0),
            ArmSlot::Left => Vec3::new(-3.4, 0.6, 3.0),
            ArmSlot::Shoulder => Vec3::new(2.0, 5.4, 1.2),
        }
    }

    /// How far off the body axis this mount can aim (radians).
    pub fn cone(self) -> f32 {
        match self {
            ArmSlot::Right | ArmSlot::Left => 50f32.to_radians(),
            ArmSlot::Shoulder => 20f32.to_radians(),
        }
    }
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

const fn thrust_for(g: f32, wet_mass: f32) -> f32 {
    g * G0 * wet_mass
}

const RIGHT: ArmSlot = ArmSlot::Right;
const LEFT: ArmSlot = ArmSlot::Left;

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
    },
];

/// The spec for a frame.
#[inline]
pub fn frame(id: FrameId) -> &'static FrameSpec {
    &FRAMES[id.index()]
}
