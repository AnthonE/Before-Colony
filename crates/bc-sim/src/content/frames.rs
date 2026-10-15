use bc_proto::snapshot::ent_flags;
use bc_proto::{FrameId, Part, WeaponKind};
use glam::Vec3;

use super::{MissileSpec, WeaponClass, weapon};
use crate::config::G0;

/// The mount of a frame's special melee move (the Pincer Strike), after the three loadout slots.
pub const SPECIAL_MOUNT: u8 = 3;

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
    /// Head/shoulder mounts (Altair's machine cannons): tied to the torso.
    Shoulder,
    /// In the head (head guns).
    Head,
    /// Shoulder missile pods.
    Pods,
    /// Chest hatches (Antares' gatlings).
    Chest,
    /// Leg pods (Antares' micro-missiles).
    LegPods,
    /// Both hands at once (twin blades): needs both arms, and is lost with either.
    Both,
    /// Aquila's nose: the Heavy Beam Rifles, pointing along the fuselage.
    Nose,
    /// Aquila's nose guns.
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
            ArmSlot::LegPods => Vec3::new(0.0, -0.4, 1.8),
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
    /// `thrust` of normal meanwhile (Altair ↔ Aquila).
    Transform {
        to: FrameId,
        ticks: u8,
        thrust: f32,
    },
    /// Toggle (MODE): the Eclipse. Enemy sensors see the suit at `sig` of its signature, and
    /// eyes only within `visual` m. It drains `drain` energy/s, needs `min_energy` (0..1) to engage,
    /// and firing or striking breaks it for `break_ticks`.
    Eclipse {
        drain: f32,
        min_energy: f32,
        break_ticks: u16,
        sig: f32,
        visual: f32,
    },
    /// Press (SPECIAL): Full Salvo. Every ranged weapon (the special mounts too) fires along
    /// the aim for `ticks`, heat ignored; then a forced overheat for `lockout` ticks. Ready again
    /// `cooldown` ticks after it starts, sooner in a fight (`content::specials`).
    FullSalvo {
        ticks: u16,
        lockout: u16,
        cooldown: u16,
    },
    /// Press (SPECIAL): a melee move with the special mount's weapon (Pincer Strike), ready again
    /// `cooldown` ticks after it starts, sooner in a fight (`content::specials`).
    MeleeMove {
        cooldown: u16,
    },
    /// Toggle (MODE): the Ward (`sim::ward`). The suit's ward discs fly out to a sphere
    /// `radius` m round it over `deploy` ticks, and while they hold it, every hostile beam that
    /// would cross into it from outside stops at it, costing the suit `absorb` energy per armour
    /// point the beam carried. Solid rounds, missiles, blades and flame go through. Up, it drains
    /// `drain` energy/s; it needs `min_energy` (0..1) to deploy, and falls when the energy's gone.
    Ward {
        radius: f32,
        deploy: u8,
        drain: f32,
        absorb: f32,
        min_energy: f32,
    },
}

impl SpecialKind {
    /// Held as the MODE state (a toggle), rather than pressed as SPECIAL.
    pub fn is_toggle(self) -> bool {
        matches!(self, SpecialKind::Transform { .. } | SpecialKind::Eclipse { .. } | SpecialKind::Ward { .. })
    }

    /// The form a transformation changes into.
    pub fn transforms_to(self) -> Option<FrameId> {
        match self {
            SpecialKind::Transform { to, .. } => Some(to),
            _ => None,
        }
    }

    /// Ticks the special takes to charge back by itself after it's used (0: it has no cooldown,
    /// `content::specials`).
    pub fn cooldown(self) -> u16 {
        match self {
            SpecialKind::FullSalvo { cooldown, .. } | SpecialKind::MeleeMove { cooldown } => cooldown,
            SpecialKind::None
            | SpecialKind::Transform { .. }
            | SpecialKind::Eclipse { .. }
            | SpecialKind::Ward { .. } => 0,
        }
    }
}

/// How the kit-aware AI (agents, the browser autopilot) flies a frame.
#[derive(Clone, Copy, Debug)]
pub struct AiHints {
    /// Preferred fighting distance, m (0: from the primary weapon's range).
    pub engage_range: f32,
    /// Closes to melee rather than shooting from range.
    pub melee_first: bool,
    /// How dangerous the Orrery rates it (1 = an Argo).
    pub threat: f32,
}

/// A weapon on a mount.
#[derive(Clone, Copy, Debug)]
pub struct Mount {
    pub weapon: WeaponKind,
    pub arm: ArmSlot,
}

/// Everything the simulation needs to know about a mech frame.
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
    /// Limb trim: attitude control by moving limbs. No propellant, limited authority.
    pub limb_trim_accel: f32,
    pub limb_trim_rate: f32,
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
    /// Damage multiplier (siderium ≈ 0.55).
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
    /// Carries the Orrery.
    pub orrery: bool,
    /// Pilots and agents may choose it (drones' frames and alternate forms: no).
    pub playable: bool,
    /// What the frame's special key does.
    pub special: SpecialKind,
    /// Weapons only the special uses (Full Salvo's chest gatlings and micro-missiles, the Pincer
    /// Strike).
    pub special_mounts: [Option<Mount>; 2],
    pub ai: AiHints,
}

impl FrameSpec {
    /// Wet mass for a given propellant load.
    #[inline]
    pub fn mass(&self, propellant: f32) -> f32 {
        self.dry_mass + propellant.max(0.0)
    }

    /// Where its pilot looks from, in the suit's frame at rest: the head's main camera, which the
    /// cockpit's view is (`bc_model`'s eye socket; its tests hold the two together), and Aquila's
    /// over its nose from the canopy. The crosshair is the line from here along the aim, and guns
    /// aim at the point on it the pilot sees (`crate::aim`).
    pub fn eye(&self) -> Vec3 {
        match self.id {
            FrameId::Aquila => Vec3::new(0.0, 2.4, 4.6),
            FrameId::Argo => ARGO_EYE,
            FrameId::Sagittarius | FrameId::Corona => PROTOTYPE_EYE,
            _ => HUMANOID_EYE,
        }
    }

    /// Whether the frame has legs to stand on: every frame but the Aquila. Without them a suit
    /// can't grip a surface.
    #[inline]
    pub fn has_legs(&self) -> bool {
        self.id != FrameId::Aquila
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

    /// The motor, seeker and lock of the frame's missiles (the first launcher in its loadout or
    /// special mounts): whether it builds missile locks at all.
    pub fn lock_spec(&self) -> Option<MissileSpec> {
        self.loadout.iter().chain(self.special_mounts.iter()).flatten().find_map(|m| weapon(m.weapon).missile)
    }

    /// The melee weapon on mount `slot`: a loadout slot, or [`SPECIAL_MOUNT`] for a melee move.
    pub fn melee_mount(&self, slot: u8) -> Option<Mount> {
        let mount = match slot {
            SPECIAL_MOUNT => match self.special {
                SpecialKind::MeleeMove { .. } => self.special_mounts[0],
                _ => None,
            },
            _ => self.loadout.get(usize::from(slot)).copied().flatten(),
        };
        mount.filter(|m| weapon(m.weapon).class == WeaponClass::Melee)
    }

    /// Which mount a strike seen in entity `flags` comes from: the special's melee move (SPECIAL),
    /// a blade in a gun slot (MELEE_ALT: the Talon), or the melee slot.
    pub fn striking_slot(&self, flags: u16) -> u8 {
        if flags & ent_flags::SPECIAL != 0 && self.melee_mount(SPECIAL_MOUNT).is_some() {
            SPECIAL_MOUNT
        } else if flags & ent_flags::MELEE_ALT != 0 {
            if self.melee_mount(0).is_some() { 0 } else { 1 }
        } else {
            2
        }
    }
}

/// A humanoid head's main camera, between its eyes at its face; the Argo's mono-eye behind its
/// visor; the prototype pair's camera (`bc_model`'s, at rest).
const HUMANOID_EYE: Vec3 = Vec3::new(0.0, 6.75, 1.1);
const ARGO_EYE: Vec3 = Vec3::new(0.0, 6.74, 1.12);
const PROTOTYPE_EYE: Vec3 = Vec3::new(0.0, 6.62, 0.94);

/// Human-shaped hitboxes, about 17 m tall, in the anime's proportions (bc-model's rig): a short
/// torso as wide as its skirts over the hips at 1.55 m, and one capsule round both long legs from
/// the hips down, the knee 2.15 m under the origin. The two overlap at the hips, leaving no seam.
const HUMANOID: [Capsule; Part::COUNT] = [
    cap([0.0, 6.4, 0.0], [0.0, 7.6, 0.2], 1.3),   // head
    cap([0.0, 2.0, 0.0], [0.0, 4.7, 0.0], 2.5),   // torso
    cap([-3.3, 4.6, 0.0], [-3.5, 0.2, 1.2], 1.1), // left arm
    cap([3.3, 4.6, 0.0], [3.5, 0.2, 1.2], 1.1),   // right arm
    cap([0.0, 1.6, 0.0], [0.0, -7.2, 0.2], 2.2),  // legs from the hips, splayed at rest
    cap([0.0, 3.0, -2.4], [0.0, 5.2, -2.8], 1.5), // backpack
];

/// Aquila's hitboxes: the folded suit as an aircraft, nose along +z.
const BIRD: [Capsule; Part::COUNT] = [
    cap([0.0, 0.5, 5.5], [0.0, 0.5, 7.5], 1.2),    // head: the nose
    cap([0.0, 0.0, -4.5], [0.0, 0.0, 5.0], 2.3),   // torso: the fuselage
    cap([-1.5, 0.0, 0.0], [-8.0, 0.0, -2.0], 1.0), // left arm: the left wing
    cap([1.5, 0.0, 0.0], [8.0, 0.0, -2.0], 1.0),   // right arm: the right wing and heavy beam rifles
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

/// Aquila ↔ Altair: 0.8 s, weapons down, a third of the main thrust.
const TRANSFORM_TICKS: u8 = 24;
const TRANSFORM_THRUST: f32 = 0.3;

/// The frames pilots can choose, in the order of the respawn keys (1–6).
pub const PLAYABLE_ORDER: [FrameId; 6] =
    [FrameId::Argo, FrameId::Altair, FrameId::Antares, FrameId::Algol, FrameId::Canopus, FrameId::Thuban];

static FRAMES: [FrameSpec; FrameId::COUNT] = [
    // AL-04S Argo (space): the everyman suit. 3.5 g, 5.6 g on boost.
    FrameSpec {
        id: FrameId::Argo,
        dry_mass: 7_100.0,
        propellant_cap: 3_000.0,
        isp: 900.0,
        main_thrust: thrust_for(3.5, 10_100.0),
        side_thrust: thrust_for(1.75, 10_100.0),
        retro_thrust: thrust_for(2.1, 10_100.0),
        boost_mult: 1.6,
        limb_trim_accel: 1.4,
        limb_trim_rate: 1.0,
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
        orrery: false,
        playable: true,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.0),
    },
    // PX-00 Altair: siderium armour, 8 g (12 g boost: past what a pilot can take),
    // Heavy Beam Rifle and the Orrery.
    FrameSpec {
        id: FrameId::Altair,
        dry_mass: 8_000.0,
        propellant_cap: 3_750.0,
        isp: 1_200.0,
        main_thrust: thrust_for(8.0, 11_750.0),
        side_thrust: thrust_for(4.4, 11_750.0),
        retro_thrust: thrust_for(5.2, 11_750.0),
        boost_mult: 1.5,
        limb_trim_accel: 2.0,
        limb_trim_rate: 1.4,
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
            Some(Mount { weapon: WeaponKind::HeavyBeamRifle, arm: RIGHT }),
            Some(Mount { weapon: WeaponKind::MachineCannon, arm: ArmSlot::Shoulder }),
            Some(Mount { weapon: WeaponKind::BeamSaber, arm: LEFT }),
        ],
        orrery: true,
        playable: true,
        special: SpecialKind::Transform {
            to: FrameId::Aquila,
            ticks: TRANSFORM_TICKS,
            thrust: TRANSFORM_THRUST,
        },
        special_mounts: [None, None],
        ai: hints(0.0, false, 2.0),
    },
    // AL-11D Corvus: agile drone, 5 g.
    FrameSpec {
        id: FrameId::Corvus,
        dry_mass: 6_500.0,
        propellant_cap: 2_500.0,
        isp: 900.0,
        main_thrust: thrust_for(5.0, 9_000.0),
        side_thrust: thrust_for(2.75, 9_000.0),
        retro_thrust: thrust_for(3.0, 9_000.0),
        boost_mult: 1.5,
        limb_trim_accel: 1.8,
        limb_trim_rate: 1.2,
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
        orrery: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.0),
    },
    // AL-09D Cetus: heavy drone with a beam cannon, 3 g.
    FrameSpec {
        id: FrameId::Cetus,
        dry_mass: 9_500.0,
        propellant_cap: 3_100.0,
        isp: 900.0,
        main_thrust: thrust_for(3.0, 12_600.0),
        side_thrust: thrust_for(1.35, 12_600.0),
        retro_thrust: thrust_for(1.5, 12_600.0),
        boost_mult: 1.3,
        limb_trim_accel: 1.0,
        limb_trim_rate: 0.8,
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
        orrery: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.3),
    },
    // PX-01 Antares: a walking arsenal. Gatlings, missile volleys, and the Full Salvo
    // Attack; siderium armour, 5.2 g.
    FrameSpec {
        id: FrameId::Antares,
        dry_mass: 8_800.0,
        propellant_cap: 3_250.0,
        isp: 1_100.0,
        main_thrust: thrust_for(5.2, 12_050.0),
        side_thrust: thrust_for(2.6, 12_050.0),
        retro_thrust: thrust_for(3.0, 12_050.0),
        boost_mult: 1.4,
        limb_trim_accel: 1.5,
        limb_trim_rate: 1.0,
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
            mount(WeaponKind::CombatKnife, LEFT),
        ],
        orrery: false,
        playable: true,
        // 45 s to charge by itself, sooner in a fight (`content::specials`).
        special: SpecialKind::FullSalvo { ticks: 90, lockout: 150, cooldown: 1_350 },
        special_mounts: [
            mount(WeaponKind::ChestGatling, ArmSlot::Chest),
            mount(WeaponKind::MicroMissile, ArmSlot::LegPods),
        ],
        ai: hints(1_800.0, false, 1.8),
    },
    // PX-02 Algol: the demon star. Beam scythe and claw shield, and the
    // Eclipse that hides it from sensors; 7 g.
    FrameSpec {
        id: FrameId::Algol,
        dry_mass: 7_300.0,
        propellant_cap: 3_100.0,
        isp: 1_150.0,
        main_thrust: thrust_for(7.0, 10_400.0),
        side_thrust: thrust_for(4.0, 10_400.0),
        retro_thrust: thrust_for(4.6, 10_400.0),
        boost_mult: 1.6,
        limb_trim_accel: 2.3,
        limb_trim_rate: 1.6,
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
            mount(WeaponKind::ClawShield, LEFT),
            mount(WeaponKind::HeadGun, ArmSlot::Head),
            mount(WeaponKind::BeamScythe, RIGHT),
        ],
        orrery: false,
        playable: true,
        special: SpecialKind::Eclipse {
            drain: 30.0,
            min_energy: 0.2,
            break_ticks: 60,
            sig: 0.02,
            visual: 150.0,
        },
        special_mounts: [None, None],
        ai: hints(0.0, true, 1.7),
    },
    // PX-03 Canopus: built for the desert and the heaviest armour of the five. Beam
    // machine gun, homing missiles, twin heat blades and the Pincer Strike; 4.6 g.
    FrameSpec {
        id: FrameId::Canopus,
        dry_mass: 9_600.0,
        propellant_cap: 3_400.0,
        isp: 1_000.0,
        main_thrust: thrust_for(4.6, 13_000.0),
        side_thrust: thrust_for(2.3, 13_000.0),
        retro_thrust: thrust_for(2.7, 13_000.0),
        boost_mult: 1.5,
        limb_trim_accel: 1.5,
        limb_trim_rate: 1.0,
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
            mount(WeaponKind::HeatBlade, ArmSlot::Both),
        ],
        orrery: false,
        playable: true,
        special: SpecialKind::MeleeMove { cooldown: 240 },
        special_mounts: [mount(WeaponKind::PincerStrike, ArmSlot::Both), None],
        ai: hints(1_200.0, false, 1.6),
    },
    // PX-04 Thuban: a duellist. The Talon (its right arm, flung out on a cable),
    // a flamethrower in the same arm, and a beam glaive; 7.4 g.
    FrameSpec {
        id: FrameId::Thuban,
        dry_mass: 7_500.0,
        propellant_cap: 3_400.0,
        isp: 1_100.0,
        main_thrust: thrust_for(7.4, 10_900.0),
        side_thrust: thrust_for(4.5, 10_900.0),
        retro_thrust: thrust_for(5.0, 10_900.0),
        boost_mult: 1.6,
        limb_trim_accel: 2.2,
        limb_trim_rate: 1.5,
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
            mount(WeaponKind::Talon, RIGHT),
            mount(WeaponKind::Flamethrower, RIGHT),
            mount(WeaponKind::BeamGlaive, LEFT),
        ],
        orrery: false,
        playable: true,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(0.0, true, 1.7),
    },
    // Altair as Aquila: faster in a straight line, clumsy in turns, the heavy beam
    // rifles fixed along the nose and no melee. Its armour, energy, heat and tank are Altair's.
    FrameSpec {
        id: FrameId::Aquila,
        dry_mass: 8_000.0,
        propellant_cap: 3_750.0,
        isp: 1_200.0,
        main_thrust: thrust_for(8.5, 11_750.0),
        side_thrust: thrust_for(1.5, 11_750.0),
        retro_thrust: thrust_for(2.0, 11_750.0),
        boost_mult: 1.4,
        limb_trim_accel: 0.8,
        limb_trim_rate: 0.6,
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
            mount(WeaponKind::HeavyBeamRifle, ArmSlot::Nose),
            mount(WeaponKind::MachineCannon, ArmSlot::NoseGuns),
            None,
        ],
        orrery: true,
        playable: false,
        special: SpecialKind::Transform {
            to: FrameId::Altair,
            ticks: TRANSFORM_TICKS,
            thrust: TRANSFORM_THRUST,
        },
        special_mounts: [None, None],
        ai: hints(0.0, false, 1.8),
    },
    // AL-13X1 Sagittarius: the Almagest's prototype gun, the archer, and its pair's spear (after
    // Gundam Wing's Vayeate, `docs/STORY.md`). Its one weapon is the beam cannon cabled to the
    // generator on its back: no shot limit, a wide beam as heavy as the Altair's, but a charge it
    // can do nothing else through and a draw on the reactor that paces it. No secondary, no blade:
    // close in it has nothing, which is what its Corona is for. Long-range eyes, 4 g.
    FrameSpec {
        id: FrameId::Sagittarius,
        dry_mass: 8_600.0,
        propellant_cap: 3_000.0,
        isp: 1_000.0,
        main_thrust: thrust_for(4.0, 11_600.0),
        side_thrust: thrust_for(2.0, 11_600.0),
        retro_thrust: thrust_for(2.4, 11_600.0),
        boost_mult: 1.5,
        limb_trim_accel: 1.4,
        limb_trim_rate: 1.0,
        rcs_accel: 4.5,
        rcs_rate: 2.6,
        rcs_propellant: 1.3,
        roll_rate: 1.4,
        fa_speed: 230.0,
        radius: 10.5,
        capsules: HUMANOID,
        part_hp: [80.0, 300.0, 130.0, 150.0, 160.0, 140.0],
        armor: 0.8,
        sensor_range: 11_000.0,
        signature: 1.3,
        energy_cap: 240.0,
        energy_regen: 30.0,
        heat_cap: 160.0,
        heat_dissipation: 28.0,
        loadout: [mount(WeaponKind::ReactorCannon, RIGHT), None, None],
        orrery: false,
        playable: false,
        special: SpecialKind::None,
        special_mounts: [None, None],
        ai: hints(3_000.0, false, 2.0),
    },
    // AL-13X2 Corona: the Sagittarius's twin, its shield, the crown at the archer's feet (after
    // the Mercurius). A quick beam gun, the beam saber out of its crash shield, and ten wards
    // racked on its back that fly out into a field no hostile beam crosses, for itself and
    // whoever flies inside it, while its own shots and its friends' go out; 4.6 g.
    FrameSpec {
        id: FrameId::Corona,
        dry_mass: 8_200.0,
        propellant_cap: 3_000.0,
        isp: 1_000.0,
        main_thrust: thrust_for(4.6, 11_200.0),
        side_thrust: thrust_for(2.4, 11_200.0),
        retro_thrust: thrust_for(2.8, 11_200.0),
        boost_mult: 1.5,
        limb_trim_accel: 1.6,
        limb_trim_rate: 1.1,
        rcs_accel: 5.0,
        rcs_rate: 2.8,
        rcs_propellant: 1.3,
        roll_rate: 1.6,
        fa_speed: 240.0,
        radius: 10.5,
        capsules: HUMANOID,
        part_hp: [80.0, 300.0, 160.0, 130.0, 160.0, 150.0],
        armor: 0.8,
        sensor_range: 8_000.0,
        signature: 1.2,
        energy_cap: 220.0,
        energy_regen: 28.0,
        heat_cap: 140.0,
        heat_dissipation: 24.0,
        loadout: [mount(WeaponKind::BeamGun, RIGHT), None, mount(WeaponKind::BeamSaber, LEFT)],
        orrery: false,
        playable: false,
        special: SpecialKind::Ward {
            radius: WARD_RADIUS,
            deploy: 15,
            drain: 6.0,
            absorb: 0.6,
            min_energy: 0.3,
        },
        special_mounts: [None, None],
        ai: hints(1_200.0, false, 1.8),
    },
];

/// How far the Corona's wards hold their field from it, m: round itself and a wingman flying
/// close (`sim::ward`).
pub const WARD_RADIUS: f32 = 26.0;
/// How many wards a Corona carries, five on each of its racks (the field's discs, as drawn).
pub const WARDS: usize = 10;

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
