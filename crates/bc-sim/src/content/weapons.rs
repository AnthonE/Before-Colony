use bc_proto::WeaponKind;

/// Weapon parameters.
#[derive(Clone, Copy, Debug)]
pub struct WeaponSpec {
    pub kind: WeaponKind,
    /// Armour points per hit, before the target's armour multiplier.
    pub damage: f32,
    /// Muzzle speed relative to the shooter, m/s (projectiles inherit the shooter's velocity).
    pub speed: f32,
    /// Projectile radius, m (blade radius for sabers).
    pub radius: f32,
    /// Max range, m (blade length for sabers).
    pub range: f32,
    /// Ticks between shots.
    pub cooldown: u16,
    pub heat: f32,
    pub energy: f32,
    /// Magazine size (0 = energy weapon, unlimited rounds).
    pub ammo: u16,
    /// Ticks of charge before the shot leaves (Twin Buster Rifle); visible to everyone.
    pub charge_ticks: u16,
    /// Random cone half-angle, radians.
    pub spread: f32,
}

impl WeaponSpec {
    /// Projectile lifetime in ticks.
    pub fn ttl_ticks(&self) -> u32 {
        if self.speed <= 0.0 { 0 } else { crate::config::secs(self.range / self.speed) }
    }
}

static WEAPONS: [WeaponSpec; 5] = [
    WeaponSpec {
        kind: WeaponKind::BeamRifle,
        damage: 45.0,
        speed: 4_000.0,
        radius: 0.6,
        range: 5_000.0,
        cooldown: 20,
        heat: 14.0,
        energy: 16.0,
        ammo: 0,
        charge_ticks: 0,
        spread: 0.0,
    },
    WeaponSpec {
        kind: WeaponKind::MachineCannon,
        damage: 6.0,
        speed: 1_200.0,
        radius: 0.25,
        range: 2_500.0,
        cooldown: 3,
        heat: 2.0,
        energy: 0.0,
        ammo: 400,
        charge_ticks: 0,
        spread: 0.004,
    },
    WeaponSpec {
        kind: WeaponKind::BeamSaber,
        damage: 90.0,
        speed: 0.0,
        radius: 0.8,
        range: 9.0,
        cooldown: 18,
        heat: 10.0,
        energy: 8.0,
        ammo: 0,
        charge_ticks: 0,
        spread: 0.0,
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
        ammo: 0,
        charge_ticks: 18,
        spread: 0.0,
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
        ammo: 0,
        charge_ticks: 0,
        spread: 0.0,
    },
];

#[inline]
pub fn weapon(kind: WeaponKind) -> &'static WeaponSpec {
    &WEAPONS[kind as usize]
}
