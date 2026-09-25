use bc_proto::WeaponKind;
use glam::Vec3;

use super::melee::{ConeSpec, MeleeSpec, MissileSpec, Stroke};

/// What a weapon is, which decides how the simulation resolves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponClass {
    /// Energy projectile flying straight.
    Beam,
    /// Solid rounds flying straight.
    Ballistic,
    /// Homing missiles with a motor and seeker.
    Missile,
    /// A blade or claw swept through the suits near it.
    Melee,
    /// A short cone of fire, applied at intervals while held.
    Cone,
}

/// How clients learn about a weapon's shots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replication {
    /// One `BeamSpawn` event per shot: it flies straight, so every client draws its whole flight.
    PerShot,
    /// Too many shots for events: clients draw a stream of tracers while the firing flag is up.
    Stream,
    /// In the snapshot's missile list while it flies.
    List,
    /// Nothing flies: clients animate the strike or the flame from the suit's flags.
    Anim,
}

/// Weapon parameters.
#[derive(Clone, Copy, Debug)]
pub struct WeaponSpec {
    pub kind: WeaponKind,
    pub class: WeaponClass,
    pub replication: Replication,
    /// Armour points per hit, before the target's armour multiplier (per blade, per missile, per
    /// flame application).
    pub damage: f32,
    /// Muzzle speed relative to the shooter, m/s (projectiles inherit the shooter's velocity).
    pub speed: f32,
    /// Projectile radius, m (blade radius for melee weapons).
    pub radius: f32,
    /// Max range, m (blade length or claw reach for melee weapons, the cone's length for flame).
    pub range: f32,
    /// Ticks between shots (for a salvo, between salvos).
    pub cooldown: u16,
    pub heat: f32,
    pub energy: f32,
    /// Magazine size (0 = energy weapon, unlimited rounds).
    pub ammo: u16,
    /// Ticks of charge before the shot leaves (Twin Buster Rifle); visible to everyone.
    pub charge_ticks: u16,
    /// Random cone half-angle, radians.
    pub spread: f32,
    /// A hit swallows the whole suit: it lands on the torso whatever it touched first (the Twin
    /// Buster Rifle's beam is wider than a suit).
    pub engulfs: bool,
    /// Shots per trigger pull, and ticks between them.
    pub salvo: u8,
    pub salvo_gap: u8,
    /// While this weapon's strike is out, the other weapons on its arm wait (the Dragon Fang is
    /// the arm).
    pub blocks_arm: bool,
    pub melee: Option<MeleeSpec>,
    pub missile: Option<MissileSpec>,
    pub cone: Option<ConeSpec>,
}

impl WeaponSpec {
    /// Projectile lifetime in ticks.
    pub fn ttl_ticks(&self) -> u32 {
        if self.speed <= 0.0 { 0 } else { crate::config::secs(self.range / self.speed) }
    }
}

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A weapon with the defaults the rows below override.
const BASE: WeaponSpec = WeaponSpec {
    kind: WeaponKind::BeamRifle,
    class: WeaponClass::Beam,
    replication: Replication::PerShot,
    damage: 0.0,
    speed: 0.0,
    radius: 0.0,
    range: 0.0,
    cooldown: 0,
    heat: 0.0,
    energy: 0.0,
    ammo: 0,
    charge_ticks: 0,
    spread: 0.0,
    engulfs: false,
    salvo: 1,
    salvo_gap: 0,
    blocks_arm: false,
    melee: None,
    missile: None,
    cone: None,
};

const STREAM: WeaponSpec = WeaponSpec { replication: Replication::Stream, ..BASE };
const GUN: WeaponSpec = WeaponSpec { class: WeaponClass::Ballistic, ..STREAM };
const BLADE: WeaponSpec = WeaponSpec { class: WeaponClass::Melee, replication: Replication::Anim, ..BASE };

/// The beam saber's swing: from the right shoulder across to the left hip.
const SABER: MeleeSpec = MeleeSpec {
    stroke: Stroke::Swing,
    windup: 4,
    active: 6,
    recovery: 8,
    clash_recovery: 10,
    arc_from: v(0.75, 0.65, 0.35),
    arc_to: v(-0.75, -0.45, 0.55),
    sub_steps: 3,
    twin: false,
    both_arms: false,
    lunge: true,
    clashable: true,
};

const HOMING: MissileSpec = MissileSpec {
    launch_speed: 120.0,
    accel: 18.0 * crate::config::G0,
    dv: 1_100.0,
    nav: 3.5,
    life: 240,
    fuse: 4.0,
    seeker_range: 3_200.0,
    seeker_cone: core::f32::consts::FRAC_PI_3, // 60°
    lock_range: 2_800.0,
    lock_cone: 0.349_065_85, // 20°
    lock_ticks: 15,
};

static WEAPONS: [WeaponSpec; WeaponKind::COUNT] = [
    WeaponSpec {
        kind: WeaponKind::BeamRifle,
        damage: 45.0,
        speed: 4_000.0,
        radius: 0.6,
        range: 5_000.0,
        cooldown: 20,
        heat: 14.0,
        energy: 16.0,
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::MachineCannon,
        damage: 6.0,
        speed: 1_200.0,
        radius: 0.25,
        range: 2_500.0,
        cooldown: 3,
        heat: 2.0,
        ammo: 400,
        spread: 0.004,
        ..GUN
    },
    WeaponSpec {
        kind: WeaponKind::BeamSaber,
        damage: 90.0,
        radius: 0.8,
        range: 9.0,
        cooldown: 18,
        heat: 10.0,
        energy: 8.0,
        melee: Some(SABER),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::TwinBusterRifle,
        damage: 220.0,
        speed: 8_000.0,
        radius: 5.0,
        range: 12_000.0,
        cooldown: 150,
        heat: 60.0,
        energy: 70.0,
        charge_ticks: 18,
        engulfs: true,
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::BeamCannon,
        damage: 70.0,
        speed: 3_500.0,
        radius: 1.2,
        range: 6_000.0,
        cooldown: 36,
        heat: 20.0,
        energy: 30.0,
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::BeamGatling,
        damage: 7.0,
        speed: 3_000.0,
        radius: 0.35,
        range: 2_400.0,
        cooldown: 3,
        heat: 2.5,
        energy: 2.0,
        spread: 0.006,
        ..STREAM
    },
    WeaponSpec {
        kind: WeaponKind::HomingMissile,
        class: WeaponClass::Missile,
        replication: Replication::List,
        damage: 32.0,
        speed: 120.0,
        radius: 0.5,
        range: 2_800.0,
        cooldown: 60,
        heat: 2.0,
        ammo: 24,
        spread: 0.05,
        salvo: 4,
        salvo_gap: 3,
        missile: Some(HOMING),
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::ArmyKnife,
        damage: 55.0,
        radius: 0.6,
        range: 5.0,
        cooldown: 12,
        heat: 6.0,
        melee: Some(MeleeSpec {
            windup: 3,
            active: 4,
            recovery: 6,
            clash_recovery: 8,
            arc_from: v(0.6, 0.5, 0.6),
            arc_to: v(-0.6, -0.3, 0.75),
            ..SABER
        }),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::ChestGatling,
        damage: 4.0,
        speed: 1_200.0,
        radius: 0.25,
        range: 1_800.0,
        cooldown: 1,
        ammo: 300,
        spread: 0.012,
        ..GUN
    },
    WeaponSpec {
        kind: WeaponKind::MicroMissile,
        class: WeaponClass::Missile,
        replication: Replication::List,
        damage: 16.0,
        speed: 150.0,
        radius: 0.3,
        range: 1_100.0,
        cooldown: 45,
        ammo: 16,
        spread: 0.14,
        salvo: 8,
        salvo_gap: 1,
        missile: Some(MissileSpec {
            launch_speed: 150.0,
            accel: 14.0 * crate::config::G0,
            dv: 800.0,
            nav: 3.0,
            life: 90,
            fuse: 3.0,
            seeker_range: 1_500.0,
            lock_range: 1_100.0,
            lock_cone: 0.436_332_3, // 25°
            ..HOMING
        }),
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::BusterShield,
        damage: 80.0,
        speed: 450.0,
        radius: 1.2,
        range: 700.0,
        cooldown: 150,
        heat: 12.0,
        energy: 25.0,
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::HeadVulcan,
        damage: 3.0,
        speed: 1_000.0,
        radius: 0.2,
        range: 1_500.0,
        cooldown: 2,
        heat: 1.0,
        ammo: 300,
        spread: 0.008,
        ..GUN
    },
    WeaponSpec {
        kind: WeaponKind::BeamScythe,
        damage: 120.0,
        radius: 1.0,
        range: 12.0,
        cooldown: 24,
        heat: 14.0,
        energy: 12.0,
        melee: Some(MeleeSpec {
            windup: 6,
            active: 6,
            recovery: 10,
            clash_recovery: 12,
            arc_from: v(-0.6, 0.8, 0.3),
            arc_to: v(0.6, -0.5, 0.6),
            sub_steps: 4,
            ..SABER
        }),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::BeamMachineGun,
        damage: 12.0,
        speed: 3_500.0,
        radius: 0.45,
        range: 3_500.0,
        cooldown: 5,
        heat: 4.5,
        energy: 5.0,
        ..STREAM
    },
    WeaponSpec {
        kind: WeaponKind::HeatShotel,
        damage: 70.0,
        radius: 0.7,
        range: 8.0,
        cooldown: 20,
        heat: 16.0,
        melee: Some(MeleeSpec {
            windup: 5,
            active: 6,
            recovery: 8,
            clash_recovery: 10,
            arc_from: v(0.9, 0.4, 0.3),
            arc_to: v(-0.2, -0.3, 0.95),
            twin: true,
            ..SABER
        }),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::CrossCrusher,
        damage: 100.0,
        radius: 1.6,
        range: 9.0,
        heat: 30.0,
        energy: 20.0,
        melee: Some(MeleeSpec {
            windup: 8,
            active: 5,
            recovery: 15,
            clash_recovery: 15,
            arc_from: v(1.0, 0.25, 0.25),
            arc_to: v(0.1, 0.0, 1.0),
            twin: true,
            both_arms: true,
            ..SABER
        }),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::DragonFang,
        damage: 85.0,
        radius: 1.4,
        range: 35.0,
        cooldown: 20,
        heat: 10.0,
        energy: 6.0,
        blocks_arm: true,
        melee: Some(MeleeSpec {
            stroke: Stroke::Thrust,
            windup: 4,
            active: 6,
            recovery: 10,
            clash_recovery: 10,
            arc_from: v(0.0, 0.0, 1.0),
            arc_to: v(0.0, 0.0, 1.0),
            sub_steps: 4,
            lunge: false,
            clashable: false,
            ..SABER
        }),
        ..BLADE
    },
    WeaponSpec {
        kind: WeaponKind::Flamethrower,
        class: WeaponClass::Cone,
        replication: Replication::Anim,
        damage: 7.0,
        range: 70.0,
        cooldown: 6,
        heat: 1.5,
        ammo: 150,
        cone: Some(ConeSpec { half_angle: 0.209_439_51, interval: 6, target_heat: 10.0 }), // 12°
        ..BASE
    },
    WeaponSpec {
        kind: WeaponKind::BeamGlaive,
        damage: 110.0,
        radius: 0.9,
        range: 13.0,
        cooldown: 22,
        heat: 12.0,
        energy: 10.0,
        melee: Some(MeleeSpec {
            windup: 5,
            active: 6,
            recovery: 10,
            clash_recovery: 10,
            arc_from: v(0.1, 1.0, 0.1),
            arc_to: v(0.0, -0.5, 0.85),
            sub_steps: 4,
            ..SABER
        }),
        ..BLADE
    },
];

// Every row sits at its kind's index, and each class has the parameters it needs.
const _: () = {
    let mut i = 0;
    while i < WEAPONS.len() {
        let w = &WEAPONS[i];
        assert!(w.kind as usize == i, "WEAPONS row out of order");
        match w.class {
            WeaponClass::Melee => assert!(w.melee.is_some() && w.speed == 0.0),
            WeaponClass::Missile => {
                assert!(w.missile.is_some() && matches!(w.replication, Replication::List))
            }
            WeaponClass::Cone => assert!(w.cone.is_some()),
            WeaponClass::Beam | WeaponClass::Ballistic => assert!(w.speed > 0.0 && w.range > 0.0),
        }
        i += 1;
    }
};

#[inline]
pub fn weapon(kind: WeaponKind) -> &'static WeaponSpec {
    &WEAPONS[kind as usize]
}
