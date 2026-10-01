//! Equipment: modules a pilot fits to their suit's parts, each one fixed design with a trade-off.
//!
//! A module rides on a part (the head has one mount, the torso two, the legs and the backpack one
//! each), so a part shot off takes its module with it. A suit carries at most one of each kind.
//! What each does is in [`crate::tuning`]; here are their mounts, masses and names.

use bc_proto::Part;

/// A module's design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ModuleKind {
    /// Head: sees further, and shows a little more.
    SensorArray = 1,
    /// Head: builds missile locks faster.
    FireControlComputer = 2,
    /// Torso: holds more energy, and weighs more.
    CapacitorBank = 3,
    /// Torso: energy comes back faster, and heat goes slower.
    ReactorBooster = 4,
    /// Torso: heat goes faster, and the suit shows more.
    RadiatorPackage = 5,
    /// Torso: takes less damage, and weighs a lot more.
    CompositePlating = 6,
    /// Torso: the pilot bears a g more.
    GSeat = 7,
    /// Torso: restores damaged systems one at a time, drawing energy while it works.
    DamageControl = 8,
    /// Backpack: a bigger tank.
    AuxiliaryTank = 9,
    /// Backpack: more main thrust, burning more for it.
    ThrusterKit = 10,
    /// Legs: more lateral and vertical thrust.
    LegVerniers = 11,
    /// Legs: a bigger hold, and slower turns.
    CargoRack = 12,
}

impl ModuleKind {
    pub const COUNT: usize = 12;
    pub const ALL: [ModuleKind; Self::COUNT] = [
        ModuleKind::SensorArray,
        ModuleKind::FireControlComputer,
        ModuleKind::CapacitorBank,
        ModuleKind::ReactorBooster,
        ModuleKind::RadiatorPackage,
        ModuleKind::CompositePlating,
        ModuleKind::GSeat,
        ModuleKind::DamageControl,
        ModuleKind::AuxiliaryTank,
        ModuleKind::ThrusterKit,
        ModuleKind::LegVerniers,
        ModuleKind::CargoRack,
    ];

    /// The kind with code `v` (0 and unknown codes: none).
    pub fn from_code(v: u32) -> Option<ModuleKind> {
        Self::ALL.into_iter().find(|k| *k as u32 == v)
    }

    /// The part it mounts on.
    pub fn part(self) -> Part {
        match self {
            ModuleKind::SensorArray | ModuleKind::FireControlComputer => Part::Head,
            ModuleKind::AuxiliaryTank | ModuleKind::ThrusterKit => Part::Backpack,
            ModuleKind::LegVerniers | ModuleKind::CargoRack => Part::Legs,
            _ => Part::Torso,
        }
    }

    /// Its mass, kg.
    pub fn mass_kg(self) -> u32 {
        match self {
            ModuleKind::SensorArray => 60,
            ModuleKind::FireControlComputer => 40,
            ModuleKind::CapacitorBank => 350,
            ModuleKind::ReactorBooster => 200,
            ModuleKind::RadiatorPackage => 180,
            ModuleKind::CompositePlating => 600,
            ModuleKind::GSeat => 150,
            ModuleKind::DamageControl => 120,
            ModuleKind::AuxiliaryTank => 200,
            ModuleKind::ThrusterKit => 160,
            ModuleKind::LegVerniers => 150,
            ModuleKind::CargoRack => 250,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            ModuleKind::SensorArray => "sensor_array",
            ModuleKind::FireControlComputer => "fire_control_computer",
            ModuleKind::CapacitorBank => "capacitor_bank",
            ModuleKind::ReactorBooster => "reactor_booster",
            ModuleKind::RadiatorPackage => "radiator_package",
            ModuleKind::CompositePlating => "composite_plating",
            ModuleKind::GSeat => "g_seat",
            ModuleKind::DamageControl => "damage_control",
            ModuleKind::AuxiliaryTank => "auxiliary_tank",
            ModuleKind::ThrusterKit => "thruster_kit",
            ModuleKind::LegVerniers => "leg_verniers",
            ModuleKind::CargoRack => "cargo_rack",
        }
    }

    pub fn from_slug(s: &str) -> Option<ModuleKind> {
        Self::ALL.into_iter().find(|k| k.slug() == s)
    }

    pub fn name(self) -> &'static str {
        match self {
            ModuleKind::SensorArray => "Sensor array",
            ModuleKind::FireControlComputer => "Fire-control computer",
            ModuleKind::CapacitorBank => "Capacitor bank",
            ModuleKind::ReactorBooster => "Reactor booster",
            ModuleKind::RadiatorPackage => "Radiator package",
            ModuleKind::CompositePlating => "Composite plating",
            ModuleKind::GSeat => "G-seat",
            ModuleKind::DamageControl => "Damage control",
            ModuleKind::AuxiliaryTank => "Auxiliary tank",
            ModuleKind::ThrusterKit => "Thruster kit",
            ModuleKind::LegVerniers => "Leg verniers",
            ModuleKind::CargoRack => "Cargo rack",
        }
    }

    /// What it does, and what it costs, in a line.
    pub fn summary(self) -> &'static str {
        match self {
            ModuleKind::SensorArray => "sensor range ×1.35; signature ×1.1",
            ModuleKind::FireControlComputer => "missile locks build 1.5× as fast",
            ModuleKind::CapacitorBank => "energy capacity ×1.5; +350 kg",
            ModuleKind::ReactorBooster => "energy regeneration ×1.35; heat dissipation ×0.85",
            ModuleKind::RadiatorPackage => "heat dissipation ×1.5; signature ×1.15",
            ModuleKind::CompositePlating => "damage taken ×0.88; +600 kg",
            ModuleKind::GSeat => "the pilot bears 1 g more; +150 kg",
            ModuleKind::DamageControl => "restores a damaged system every 25 s, drawing 4 energy/s",
            ModuleKind::AuxiliaryTank => "tank ×1.4; +200 kg",
            ModuleKind::ThrusterKit => "main thrust ×1.15; specific impulse ×0.88",
            ModuleKind::LegVerniers => "lateral and vertical thrust ×1.25; +150 kg",
            ModuleKind::CargoRack => "hold +1,000 kg; AMBAC ×0.9; +250 kg",
        }
    }
}

/// The mounts, in order: the part each is on.
pub const MOUNTS: [Part; 5] = [Part::Head, Part::Torso, Part::Torso, Part::Legs, Part::Backpack];
/// Bits a module's code takes.
pub const MODULE_BITS: u32 = 4;

// What the modules do (see `crate::tuning`).
pub const SENSOR_ARRAY_RANGE: f32 = 1.35;
pub const SENSOR_ARRAY_SIGNATURE: f32 = 1.1;
pub const CAPACITOR_BANK: f32 = 1.5;
pub const REACTOR_BOOSTER_REGEN: f32 = 1.35;
pub const REACTOR_BOOSTER_HEAT: f32 = 0.85;
pub const RADIATOR_PACKAGE_HEAT: f32 = 1.5;
pub const RADIATOR_PACKAGE_SIGNATURE: f32 = 1.15;
pub const COMPOSITE_PLATING: f32 = 0.88;
pub const G_SEAT: f32 = 1.0;
pub const AUXILIARY_TANK: f32 = 1.4;
pub const THRUSTER_KIT_MAIN: f32 = 1.15;
pub const THRUSTER_KIT_ISP: f32 = 0.88;
pub const LEG_VERNIERS: f32 = 1.25;
pub const CARGO_RACK_KG: u32 = 1_000;
pub const CARGO_RACK_AMBAC: f32 = 0.9;
/// Damage control: ticks to restore one damaged system, and the energy it draws meanwhile, /s.
pub const REPAIR_TICKS: u16 = 750;
pub const REPAIR_ENERGY: f32 = 4.0;

/// What's on each mount (a code per [`MOUNTS`] entry, 4 bits each; 0: nothing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modules(pub u32);

impl Modules {
    pub const BITS: u32 = MODULE_BITS * MOUNTS.len() as u32;
    pub const NONE: Modules = Modules(0);

    /// What's on mount `m`.
    pub fn get(self, m: usize) -> Option<ModuleKind> {
        (m < MOUNTS.len()).then(|| ModuleKind::from_code((self.0 >> (MODULE_BITS * m as u32)) & 15)).flatten()
    }

    pub fn set(&mut self, m: usize, kind: Option<ModuleKind>) {
        if m >= MOUNTS.len() {
            return;
        }
        let shift = MODULE_BITS * m as u32;
        self.0 = (self.0 & !(15 << shift)) | (kind.map_or(0, |k| k as u32) << shift);
    }

    /// The modules on parts still on (a bit per [`Part`] in `gone`), with their mounts.
    pub fn fitted(self, gone: u8) -> impl Iterator<Item = (usize, ModuleKind)> {
        (0..MOUNTS.len())
            .filter(move |m| gone & (1 << MOUNTS[*m] as u8) == 0)
            .filter_map(move |m| self.get(m).map(|k| (m, k)))
    }

    /// Whether a module of `kind` is on a part still on.
    pub fn has(self, kind: ModuleKind, gone: u8) -> bool {
        self.fitted(gone).any(|(_, k)| k == kind)
    }

    /// Only modules on the mounts of their own part, and at most one of each kind.
    pub fn clean(self) -> Modules {
        let mut out = Modules::NONE;
        let mut seen = 0u32;
        for (m, part) in MOUNTS.iter().enumerate() {
            if let Some(k) = self.get(m)
                && k.part() == *part
                && seen & (1 << k as u32) == 0
            {
                seen |= 1 << k as u32;
                out.set(m, Some(k));
            }
        }
        out
    }

    /// Without what was on the parts in `gone`.
    pub fn without(self, gone: u8) -> Modules {
        let mut out = self;
        for (m, part) in MOUNTS.iter().enumerate() {
            if gone & (1 << *part as u8) != 0 {
                out.set(m, None);
            }
        }
        out
    }

    /// Mass of the modules on parts still on, kg.
    pub fn mass_kg(self, gone: u8) -> u32 {
        self.fitted(gone).map(|(_, k)| k.mass_kg()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_pack_mount_on_their_parts_and_go_with_them() {
        let mut m = Modules::NONE;
        m.set(0, Some(ModuleKind::SensorArray));
        m.set(1, Some(ModuleKind::GSeat));
        m.set(2, Some(ModuleKind::GSeat));
        m.set(4, Some(ModuleKind::LegVerniers));
        assert!(m.0 < 1 << Modules::BITS);
        let c = m.clean();
        assert_eq!(c.get(0), Some(ModuleKind::SensorArray));
        assert_eq!(c.get(1), Some(ModuleKind::GSeat));
        assert_eq!(c.get(2), None, "one of each kind");
        assert_eq!(c.get(4), None, "leg verniers don't go on the backpack");
        let head = 1 << Part::Head as u8;
        assert!(!c.has(ModuleKind::SensorArray, head));
        assert_eq!(c.without(head).get(0), None);
        assert_eq!(c.mass_kg(0), 60 + 150);
        for k in ModuleKind::ALL {
            assert_eq!(ModuleKind::from_slug(k.slug()), Some(k));
            assert_eq!(ModuleKind::from_code(k as u32), Some(k));
            assert!(MOUNTS.contains(&k.part()));
        }
    }
}
