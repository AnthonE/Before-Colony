//! A suit's stat sheet: what its damage, its systems (and, soon, its equipment) make of its frame.
//!
//! [`Tuning`] is a handful of multipliers on the frame's own numbers. The server builds it for
//! every suit at the top of each tick's flight, from the suit's state at the end of the last tick;
//! the owner's client builds the same one from the snapshot that carries that state. Both run this
//! code, and it's nothing but products and branches (no `mul_add`, no transcendentals), so the
//! two get the same bits and the prediction flies the server's suit.

use bc_proto::Part;
use bc_proto::quant::{dequantize_unit, quantize_unit};
use glam::Vec3;

use crate::content::systems::{self as sys, FAILED, System, Systems};
use crate::content::{ArmSlot, FrameSpec};
use crate::flight::FlightMods;
use crate::math::{hash01, normalize_or};

/// A multiplier rounded to 8 bits, as part-loss factors always have been.
pub fn wire(x: f32) -> f32 {
    dequantize_unit(quantize_unit(x, 8), 8)
}

/// What a suit's frame is multiplied by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// AMBAC's authority with the arms idle.
    pub ambac: f32,
    /// Main, side and retro thrusters' authority.
    pub main: f32,
    pub side: f32,
    pub retro: f32,
    /// How much of boost's extra thrust the boosters give.
    pub boost: f32,
    /// Specific impulse.
    pub isp: f32,
    /// Sustained G the pilot bears, g.
    pub g_tolerance: f32,
    /// Propellant lost from the tank, kg/s.
    pub leak_kg_s: f32,
    /// The main thrusters cough (see [`sputter`]).
    pub sputter: bool,
    /// Sensor range, and the signature others see.
    pub sensor: f32,
    pub signature: f32,
    /// Energy capacity and regeneration.
    pub energy_cap: f32,
    pub regen: f32,
    /// Heat dissipation.
    pub heat: f32,
    /// Damage taken.
    pub armor: f32,
    /// The tank's size.
    pub tank: f32,
    /// Hold capacity beyond the frame's, kg.
    pub hold_kg: u32,
    /// Missile lock progress a tick (0: no locks), and how fast a lost one falls apart.
    pub lock_step: u8,
    pub lock_decay: u8,
    /// The ZERO System's firing solution pulls shots onto it.
    pub magnetism: bool,
    /// How far each arm's weapons reach off the body's axis, of their full reach.
    pub cone_l: f32,
    pub cone_r: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        tuning(0, Systems::OK)
    }
}

/// Whether `part` is in `gone` (a bit per [`Part`]).
#[inline]
pub fn is_gone(gone: u8, part: Part) -> bool {
    gone & (1 << part as u8) != 0
}

/// The stat sheet of a suit whose parts in `gone` were shot off and whose systems stand at
/// `systems`.
pub fn tuning(gone: u8, systems: Systems) -> Tuning {
    let lost = |p: Part| is_gone(gone, p);
    let level = |s: System| usize::from(systems.level(s, gone));
    // Limbs swing to turn the suit (AMBAC): each one gone takes its share.
    let mut limbs: f32 = 1.0;
    if lost(Part::ArmL) {
        limbs -= 0.2;
    }
    if lost(Part::ArmR) {
        limbs -= 0.2;
    }
    if lost(Part::Legs) {
        limbs -= 0.3;
    }
    let legs = sys::LEG_THRUSTERS[level(System::LegThrusters)];
    Tuning {
        ambac: wire(limbs.max(0.1)) * sys::GYROS[level(System::Gyros)],
        main: sys::MAIN_THRUSTERS[level(System::MainThrusters)],
        side: legs,
        retro: legs,
        boost: sys::BOOSTERS[level(System::Boosters)],
        isp: 1.0,
        g_tolerance: sys::COCKPIT_G[level(System::Cockpit)],
        leak_kg_s: sys::LEAK_KG_S[level(System::Tank)],
        sputter: level(System::MainThrusters) == usize::from(sys::DAMAGED),
        sensor: sys::SENSORS[level(System::Sensors)],
        signature: 1.0,
        energy_cap: 1.0,
        regen: sys::REACTOR[level(System::Reactor)],
        heat: sys::RADIATORS[level(System::Radiators)],
        armor: 1.0,
        tank: 1.0,
        hold_kg: 0,
        lock_step: sys::LOCK_STEP[level(System::FireControl)],
        lock_decay: sys::LOCK_DECAY[level(System::FireControl)],
        magnetism: level(System::FireControl) != usize::from(FAILED),
        cone_l: sys::ACTUATORS[level(System::ActuatorL)],
        cone_r: sys::ACTUATORS[level(System::ActuatorR)],
    }
}

/// Parts shot off, from an own state (any armour at all left arrives as more than 0).
pub fn own_gone(own: &bc_proto::OwnState) -> u8 {
    let mut gone = 0;
    for p in crate::content::salvage::DETACHABLE {
        if own.parts[p as usize] <= 0.0 {
            gone |= 1 << p as u8;
        }
    }
    gone
}

/// The owner's client's copy of its suit's stat sheet, from the snapshot: the same as the one the
/// server flies the next tick with.
pub fn own_tuning(own: &bc_proto::OwnState) -> Tuning {
    tuning(own_gone(own), Systems(own.systems))
}

/// The flight model's modifiers from a suit's stat sheet, before what the arms are doing (busy
/// arms, a lunge), a change of form and a sputter, which the caller applies tick by tick.
pub fn flight_mods(t: &Tuning, g_immune: bool, extra_mass_kg: i32) -> FlightMods {
    FlightMods {
        ambac: t.ambac,
        thrust: 1.0,
        main: t.main,
        side: t.side,
        retro: t.retro,
        boost: t.boost,
        isp: t.isp,
        g_tolerance: t.g_tolerance,
        leak_kg_s: t.leak_kg_s,
        g_immune,
        lunge: false,
        extra_mass_kg,
    }
}

/// What damaged main thrusters give on tick `tick` of suit `slot`: now and then, for a quarter of
/// a second, they cough. The owner's client knows both numbers, so it coughs on the same ticks.
pub fn sputter(t: &Tuning, tick: u32, slot: u16) -> f32 {
    if t.sputter && hash01(tick >> sys::SPUTTER_SHIFT, u32::from(slot) ^ 0x5B17) < sys::SPUTTER_CHANCE {
        sys::SPUTTER_THRUST
    } else {
        1.0
    }
}

/// How far off the body's axis a weapon on `arm` can point, radians.
pub fn cone(arm: ArmSlot, t: &Tuning) -> f32 {
    let k = match arm {
        ArmSlot::Left => t.cone_l,
        ArmSlot::Right | ArmSlot::Nose => t.cone_r,
        ArmSlot::Both => t.cone_l.min(t.cone_r),
        ArmSlot::Shoulder
        | ArmSlot::Head
        | ArmSlot::Pods
        | ArmSlot::Chest
        | ArmSlot::LegPods
        | ArmSlot::NoseGuns => 1.0,
    };
    arm.cone() * k
}

/// A concussed pilot's shot from mount `mount` on tick `tick`: `dir`, wandering (at most
/// [`CONCUSSION_WOBBLE`](sys::CONCUSSION_WOBBLE)). The shooter's client draws it the same way.
pub fn wobble(dir: Vec3, tick: u32, slot: u16, mount: usize) -> Vec3 {
    let seed = u32::from(slot) * 11 + mount as u32 + 0x0C0C;
    let n =
        Vec3::new(hash01(tick, seed) - 0.5, hash01(tick ^ 0x3C, seed) - 0.5, hash01(tick ^ 0xC3, seed) - 0.5);
    normalize_or(dir + (n * (2.0 * sys::CONCUSSION_WOBBLE)).clamp_length_max(sys::CONCUSSION_WOBBLE), dir)
}

/// The tank's size, kg.
pub fn tank_cap(spec: &FrameSpec, t: &Tuning) -> f32 {
    spec.propellant_cap * t.tank
}

/// Whether hand `right` (the right, or the left) can hold anything: its arm is on and its
/// actuators work.
pub fn hand_works(gone: u8, systems: Systems, right: bool) -> bool {
    let s = if right { System::ActuatorR } else { System::ActuatorL };
    systems.level(s, gone) != FAILED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::systems::DAMAGED;

    #[test]
    fn a_whole_suit_flies_its_frame_as_it_is() {
        let t = tuning(0, Systems::OK);
        let m = flight_mods(&t, false, 0);
        let d = FlightMods::default();
        assert_eq!(
            (m.ambac, m.thrust, m.main, m.side, m.retro, m.boost, m.isp, m.g_tolerance, m.leak_kg_s),
            (d.ambac, d.thrust, d.main, d.side, d.retro, d.boost, d.isp, d.g_tolerance, d.leak_kg_s)
        );
        assert_eq!((t.sensor, t.regen, t.heat, t.cone_l, t.cone_r), (1.0, 1.0, 1.0, 1.0, 1.0));
        assert_eq!(t.lock_step, 2);
        assert!(!t.sputter && t.magnetism);
    }

    #[test]
    fn parts_shot_off_fail_their_systems() {
        let all = (1 << Part::ArmR as u8) | (1 << Part::Legs as u8) | (1 << Part::Backpack as u8);
        let t = tuning(all, Systems::OK);
        assert_eq!(t.main, sys::MAIN_THRUSTERS[2]);
        assert_eq!(t.boost, 0.0);
        assert_eq!(t.side, sys::LEG_THRUSTERS[2]);
        assert!(t.ambac < 0.75, "{t:?}");
        assert_eq!(t.cone_r, sys::ACTUATORS[2]);
        assert_eq!(tuning(1 << Part::Head as u8, Systems::OK).sensor, 0.4, "the sub-camera, as ever");
    }

    #[test]
    fn each_level_does_what_its_table_says() {
        for level in [DAMAGED, FAILED] {
            let l = usize::from(level);
            let t = |s: System| tuning(0, Systems::OK.with(s, level));
            assert_eq!(t(System::Sensors).sensor, sys::SENSORS[l]);
            assert_eq!(t(System::Reactor).regen, sys::REACTOR[l]);
            assert_eq!(t(System::Tank).leak_kg_s, sys::LEAK_KG_S[l]);
            assert_eq!(t(System::Radiators).heat, sys::RADIATORS[l]);
            assert_eq!(t(System::Gyros).ambac, sys::GYROS[l]);
            assert_eq!(t(System::Cockpit).g_tolerance, sys::COCKPIT_G[l]);
            assert_eq!(t(System::ActuatorL).cone_l, sys::ACTUATORS[l]);
            assert_eq!(t(System::LegThrusters).side, sys::LEG_THRUSTERS[l]);
            assert_eq!(t(System::MainThrusters).main, sys::MAIN_THRUSTERS[l]);
            assert_eq!(t(System::Boosters).boost, sys::BOOSTERS[l]);
            assert_eq!(t(System::FireControl).lock_step, sys::LOCK_STEP[l]);
        }
        assert!(!tuning(0, Systems::OK.with(System::FireControl, FAILED)).magnetism);
    }

    #[test]
    fn damaged_thrusters_cough_now_and_then_the_same_way_every_time() {
        let t = tuning(0, Systems::OK.with(System::MainThrusters, DAMAGED));
        let coughs: usize = (0..30_000u32).filter(|k| sputter(&t, *k, 7) < 1.0).count();
        let share = coughs as f32 / 30_000.0;
        assert!((share - sys::SPUTTER_CHANCE).abs() < 0.03, "{share}");
        // In windows: a cough lasts its whole window.
        for k in (0..800u32).step_by(8) {
            let first = sputter(&t, k, 7);
            assert!((k..k + 8).all(|j| sputter(&t, j, 7) == first));
        }
        assert_eq!(sputter(&tuning(0, Systems::OK), 5, 7), 1.0);
    }

    #[test]
    fn a_concussed_shot_wanders_but_not_far() {
        for k in 0..500u32 {
            let d = wobble(Vec3::Z, k, 3, 0);
            let off = crate::math::angle_between(d, Vec3::Z);
            assert!(off <= sys::CONCUSSION_WOBBLE + 1e-4, "{off}");
        }
    }
}
