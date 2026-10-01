//! What's inside each part: the systems a blow can reach once it's through the armour, and what
//! each does to the suit damaged or failed.
//!
//! A system is working, damaged or failed. A part shot off takes its systems with it: they count
//! as failed while it's gone (nothing stores that), so losing the backpack is the main thrusters
//! and boosters failing, and losing the head is the sensors failing.

use bc_proto::Part;

/// A system inside a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum System {
    /// The head's main camera and sensors: how far the suit sees.
    Sensors = 0,
    /// The head's fire control: missile locks, and the ZERO System's firing solution.
    FireControl = 1,
    /// The torso's reactor: how fast energy comes back.
    Reactor = 2,
    /// The torso's propellant tank.
    Tank = 3,
    /// The torso's radiators: how fast heat goes.
    Radiators = 4,
    /// The torso's gyros: AMBAC's turning.
    Gyros = 5,
    /// The cockpit, and the pilot in it.
    Cockpit = 6,
    /// The left arm's actuators: where its weapons can point, and its hand.
    ActuatorL = 7,
    /// The right arm's.
    ActuatorR = 8,
    /// The legs' verniers: lateral, vertical and retro thrust.
    LegThrusters = 9,
    /// The backpack's main thrusters.
    MainThrusters = 10,
    /// The backpack's boosters.
    Boosters = 11,
}

/// Levels: working, damaged, failed.
pub const OK: u8 = 0;
pub const DAMAGED: u8 = 1;
pub const FAILED: u8 = 2;

impl System {
    pub const COUNT: usize = 12;
    pub const ALL: [System; Self::COUNT] = [
        System::Sensors,
        System::FireControl,
        System::Reactor,
        System::Tank,
        System::Radiators,
        System::Gyros,
        System::Cockpit,
        System::ActuatorL,
        System::ActuatorR,
        System::LegThrusters,
        System::MainThrusters,
        System::Boosters,
    ];

    pub fn from_index(i: usize) -> Option<System> {
        Self::ALL.get(i).copied()
    }

    /// The part it's in.
    pub fn part(self) -> Part {
        match self {
            System::Sensors | System::FireControl => Part::Head,
            System::Reactor | System::Tank | System::Radiators | System::Gyros | System::Cockpit => {
                Part::Torso
            }
            System::ActuatorL => Part::ArmL,
            System::ActuatorR => Part::ArmR,
            System::LegThrusters => Part::Legs,
            System::MainThrusters | System::Boosters => Part::Backpack,
        }
    }

    /// How likely a blow through its part's armour finds it, against the part's other systems.
    pub fn weight(self) -> u32 {
        match self {
            System::Reactor => 3,
            System::Sensors | System::Tank | System::Radiators | System::Gyros | System::MainThrusters => 2,
            _ => 1,
        }
    }

    /// Its slug (the pilot records, the terminals).
    pub fn slug(self) -> &'static str {
        match self {
            System::Sensors => "sensors",
            System::FireControl => "fire_control",
            System::Reactor => "reactor",
            System::Tank => "tank",
            System::Radiators => "radiators",
            System::Gyros => "gyros",
            System::Cockpit => "cockpit",
            System::ActuatorL => "actuators_l",
            System::ActuatorR => "actuators_r",
            System::LegThrusters => "leg_thrusters",
            System::MainThrusters => "main_thrusters",
            System::Boosters => "boosters",
        }
    }

    pub fn from_slug(s: &str) -> Option<System> {
        Self::ALL.into_iter().find(|x| x.slug() == s)
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            System::Sensors => "sensors",
            System::FireControl => "fire control",
            System::Reactor => "reactor",
            System::Tank => "propellant tank",
            System::Radiators => "radiators",
            System::Gyros => "gyros",
            System::Cockpit => "cockpit",
            System::ActuatorL | System::ActuatorR => "actuators",
            System::LegThrusters => "leg thrusters",
            System::MainThrusters => "main thrusters",
            System::Boosters => "boosters",
        }
    }

    /// Its three-letter tag on the HUD.
    pub fn tag(self) -> &'static str {
        match self {
            System::Sensors => "SNS",
            System::FireControl => "FCS",
            System::Reactor => "RCT",
            System::Tank => "TNK",
            System::Radiators => "RAD",
            System::Gyros => "GYR",
            System::Cockpit => "CPT",
            System::ActuatorL | System::ActuatorR => "ACT",
            System::LegThrusters => "VRN",
            System::MainThrusters => "THR",
            System::Boosters => "BST",
        }
    }

    /// The systems in `part`.
    pub fn of_part(part: Part) -> impl Iterator<Item = System> {
        Self::ALL.into_iter().filter(move |s| s.part() == part)
    }
}

/// Every system's level as stored: 2 bits each, in [`System`] order. (A part shot off counts as
/// failed on top of this: see [`Systems::level`].)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Systems(pub u32);

impl Systems {
    /// Bits a [`Systems`] takes on the wire.
    pub const BITS: u32 = 2 * System::COUNT as u32;

    /// Everything working.
    pub const OK: Systems = Systems(0);

    /// The level stored for `sys`.
    #[inline]
    pub fn get(self, sys: System) -> u8 {
        ((self.0 >> (2 * sys as u32)) & 3).min(u32::from(FAILED)) as u8
    }

    pub fn set(&mut self, sys: System, level: u8) {
        let shift = 2 * sys as u32;
        self.0 = (self.0 & !(3 << shift)) | (u32::from(level.min(FAILED)) << shift);
    }

    pub fn with(mut self, sys: System, level: u8) -> Systems {
        self.set(sys, level);
        self
    }

    /// `sys`'s level with the parts in `gone` (a bit per [`Part`]) shot off: failed if its part is.
    #[inline]
    pub fn level(self, sys: System, gone: u8) -> u8 {
        if gone & (1 << sys.part() as u8) != 0 { FAILED } else { self.get(sys) }
    }

    /// Nothing stored damaged or failed.
    pub fn is_ok(self) -> bool {
        self.0 == 0
    }

    /// The worst level among the systems of the parts still on.
    pub fn worst(self, gone: u8) -> u8 {
        System::ALL
            .into_iter()
            .filter(|s| gone & (1 << s.part() as u8) == 0)
            .map(|s| self.get(s))
            .max()
            .unwrap_or(OK)
    }

    /// The levels, sanitised (a stray 3 reads as failed).
    pub fn clean(self) -> Systems {
        let mut out = Systems::OK;
        for s in System::ALL {
            out.set(s, self.get(s));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Criticals: when a blow gets through to a part's systems.
// ---------------------------------------------------------------------------------------------

/// The most likely a single blow is to reach a system.
pub const CRIT_CAP: f32 = 0.75;
/// A blow this big (of the part's full armour) is as likely as its thinned armour allows.
pub const CRIT_BLOW: f32 = 0.25;
/// A blow this big knocks a system down two levels.
pub const CRIT_DOUBLE: f32 = 0.35;
/// Mobile Dolls are built simply: their systems are reached this much less often.
pub const DOLL_CRIT: f32 = 0.6;

/// How likely a blow of `blow` (of the part's full armour) that leaves `armour_left` (0..1) is to
/// reach one of the part's systems. Thin armour lets more through, and so does a bigger blow.
pub fn crit_chance(blow: f32, armour_left: f32, doll: bool) -> f32 {
    let p = ((1.0 - armour_left.clamp(0.0, 1.0)) * (blow / CRIT_BLOW).min(1.0)).min(CRIT_CAP);
    if doll { p * DOLL_CRIT } else { p }
}

// ---------------------------------------------------------------------------------------------
// What each level does. (Indexed by level: working, damaged, failed.)
// ---------------------------------------------------------------------------------------------

/// Sensor range.
pub const SENSORS: [f32; 3] = [1.0, 0.7, 0.4];
/// Missile lock progress a tick (a lock is acquired at twice the launcher's `lock_ticks`).
pub const LOCK_STEP: [u8; 3] = [2, 1, 0];
/// And how fast it falls apart when lost.
pub const LOCK_DECAY: [u8; 3] = [4, 8, 8];
/// Energy regeneration.
pub const REACTOR: [f32; 3] = [1.0, 0.5, 0.15];
/// Propellant lost, kg/s.
pub const LEAK_KG_S: [f32; 3] = [0.0, 3.0, 15.0];
/// Under anime rules, how fast the boost gauge fills back up: a holed tank can't hold what it's
/// given, and a failed one nothing at all (it still flies, but it can't boost for long).
pub const TANK_REFILL: [f32; 3] = [1.0, 0.5, 0.0];
/// Heat dissipation.
pub const RADIATORS: [f32; 3] = [1.0, 0.6, 0.25];
/// AMBAC authority.
pub const GYROS: [f32; 3] = [1.0, 0.75, 0.5];
/// Sustained G the pilot bears.
pub const COCKPIT_G: [f32; 3] = [6.0, 5.0, 4.0];
/// How far off the body's axis an arm's weapons point, of its full reach.
pub const ACTUATORS: [f32; 3] = [1.0, 0.7, 0.3];
/// Lateral, vertical and retro thrust.
pub const LEG_THRUSTERS: [f32; 3] = [1.0, 0.8, 0.6];
/// Main thrust.
pub const MAIN_THRUSTERS: [f32; 3] = [1.0, 0.7, 0.35];
/// Boost's extra thrust.
pub const BOOSTERS: [f32; 3] = [1.0, 0.5, 0.0];

/// Damaged main thrusters cough: this share of 8-tick windows, at this much thrust.
pub const SPUTTER_CHANCE: f32 = 0.2;
pub const SPUTTER_THRUST: f32 = 0.3;
/// Ticks in a sputter window (a cough lasts about a quarter of a second).
pub const SPUTTER_SHIFT: u32 = 3;

/// A reactor struck scrams: no energy at all for this long, ticks.
pub const SCRAM_TICKS: u8 = 120;
/// A pilot whose cockpit is struck is concussed for this long, ticks.
pub const CONCUSSION_TICKS: u8 = 90;
/// How far a concussed pilot's shots wander, radians (1.5°).
pub const CONCUSSION_WOBBLE: f32 = 0.026;
/// An arm whose actuators are struck jams its weapons for at least this long, ticks.
pub const JAM_TICKS: u16 = 90;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_pack_and_parts_shot_off_fail_their_systems() {
        let mut s = Systems::OK;
        s.set(System::Reactor, DAMAGED);
        s.set(System::Boosters, FAILED);
        assert_eq!(s.get(System::Reactor), DAMAGED);
        assert_eq!(s.get(System::Boosters), FAILED);
        assert_eq!(s.get(System::Tank), OK);
        assert!(s.0 < 1 << Systems::BITS);
        let head = 1 << Part::Head as u8;
        assert_eq!(s.level(System::Sensors, head), FAILED);
        assert_eq!(s.level(System::Sensors, 0), OK);
        assert_eq!(s.worst(0), FAILED);
        assert_eq!(s.worst(1 << Part::Backpack as u8), DAMAGED);
        for sys in System::ALL {
            assert_eq!(System::from_slug(sys.slug()), Some(sys));
            assert!(System::of_part(sys.part()).any(|x| x == sys));
        }
    }

    #[test]
    fn thin_armour_and_big_blows_let_more_through() {
        assert_eq!(crit_chance(0.2, 1.0, false), 0.0, "whole armour stops it");
        assert!(crit_chance(0.05, 0.5, false) < crit_chance(0.2, 0.5, false));
        assert!(crit_chance(0.2, 0.8, false) < crit_chance(0.2, 0.3, false));
        assert_eq!(crit_chance(1.0, 0.0, false), CRIT_CAP);
        assert!(crit_chance(0.3, 0.2, true) < crit_chance(0.3, 0.2, false));
    }
}
