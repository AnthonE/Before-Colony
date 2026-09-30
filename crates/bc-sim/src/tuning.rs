//! A suit's stat sheet: what its damage (and, soon, its systems and equipment) make of its frame.
//!
//! [`Tuning`] is a handful of multipliers on the frame's own numbers. The server builds it for
//! every suit at the top of each tick's flight, from the suit's state at the end of the last tick;
//! the owner's client builds the same one from the snapshot that carries that state. Both run this
//! code, and it's nothing but products and branches (no `mul_add`, no transcendentals), so the
//! two get the same bits and the prediction flies the server's suit.

use bc_proto::Part;
use bc_proto::quant::{dequantize_unit, quantize_unit};

use crate::flight::{FlightMods, HUMAN_G_TOLERANCE};

/// A multiplier rounded to 8 bits, as part-loss factors always have been (so a suit flies exactly
/// as it did before these were computed here).
pub fn wire(x: f32) -> f32 {
    dequantize_unit(quantize_unit(x, 8), 8)
}

/// What a suit's frame is multiplied by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// AMBAC's authority with the arms idle.
    pub ambac: f32,
    /// Thrust on every axis, for parts shot off.
    pub thrust: f32,
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
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            ambac: 1.0,
            thrust: 1.0,
            main: 1.0,
            side: 1.0,
            retro: 1.0,
            boost: 1.0,
            isp: 1.0,
            g_tolerance: HUMAN_G_TOLERANCE,
            leak_kg_s: 0.0,
        }
    }
}

/// Whether `part` is in `gone` (a bit per [`Part`]).
#[inline]
pub fn is_gone(gone: u8, part: Part) -> bool {
    gone & (1 << part as u8) != 0
}

/// The stat sheet of a suit whose parts in `gone` were shot off.
pub fn tuning(gone: u8) -> Tuning {
    let lost = |p: Part| is_gone(gone, p);
    let mut ambac: f32 = 1.0;
    if lost(Part::ArmL) {
        ambac -= 0.2;
    }
    if lost(Part::ArmR) {
        ambac -= 0.2;
    }
    if lost(Part::Legs) {
        ambac -= 0.3;
    }
    let mut thrust: f32 = if lost(Part::Backpack) { 0.35 } else { 1.0 };
    if lost(Part::Legs) {
        thrust *= 0.9;
    }
    Tuning { ambac: wire(ambac.max(0.1)), thrust: wire(thrust), ..Tuning::default() }
}

/// The flight model's modifiers from a suit's stat sheet, before what the arms are doing (busy
/// arms, a lunge) and a change of form, which the caller applies tick by tick.
pub fn flight_mods(t: &Tuning, g_immune: bool, extra_mass_kg: i32) -> FlightMods {
    FlightMods {
        ambac: t.ambac,
        thrust: t.thrust,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_suit_flies_its_frame_as_it_is() {
        let t = tuning(0);
        assert_eq!(t, Tuning::default());
        let m = flight_mods(&t, false, 0);
        let d = FlightMods::default();
        assert_eq!(
            (m.ambac, m.thrust, m.main, m.side, m.retro, m.boost, m.isp, m.g_tolerance, m.leak_kg_s),
            (d.ambac, d.thrust, d.main, d.side, d.retro, d.boost, d.isp, d.g_tolerance, d.leak_kg_s)
        );
    }

    #[test]
    fn parts_shot_off_weaken_it_as_they_always_have() {
        let all = (1 << Part::ArmR as u8) | (1 << Part::Legs as u8) | (1 << Part::Backpack as u8);
        let t = tuning(all);
        assert!(t.thrust < 0.5 && t.ambac < 0.75, "{t:?}");
    }
}
