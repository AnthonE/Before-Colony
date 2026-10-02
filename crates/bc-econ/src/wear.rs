//! Wear from use: a suit's systems wear down between fights as well as in them. Thruster hours,
//! barrel wear and reactor cycles add up sortie by sortie (`bc_sim::sim::Usage`, what each one
//! brought home), and once a system has had its service life, it comes home a level worse
//! (working to damaged, damaged to failed). An overhaul restores it and starts its life again;
//! servicing a system before then is cheaper than an overhaul after. So keeping a suit flying is a
//! steady trade in machined components.
//!
//! | What's counted | Wears | Service life |
//! |---|---|---|
//! | the main thrusters burning | the main thrusters | [`MAIN_S`] s |
//! | boosting | the boosters | [`BOOST_S`] s |
//! | rounds and shots from a mount | the actuators of the arm it hangs on (fire control, for the head's, shoulders' and chest's) | [`ROUNDS`] |
//! | the suit overheating | the reactor | [`OVERHEATS`] times |

use bc_proto::Part;
use bc_sim::TICK_HZ;
use bc_sim::content::{ArmSlot, System, frame};
use bc_sim::sim::Usage;
use serde::{Deserialize, Serialize};

use crate::faults::{Faults, overhaul_cost};
use crate::item::Item;

/// Service lives: seconds of main burn, seconds of boost, rounds or shots from a mount, overheats.
pub const MAIN_S: u32 = 1_800;
pub const BOOST_S: u32 = 300;
pub const ROUNDS: u32 = 1_500;
pub const OVERHEATS: u32 = 6;
/// A system this far through its life can be serviced (its life starts again), for a part of
/// what overhauling a damaged one takes ([`service_cost`]).
pub const SERVICE_FROM: f32 = 0.25;

/// What a suit's systems have been through since they were last overhauled or serviced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wear {
    /// Ticks the main thrusters burnt, and boosted.
    #[serde(default)]
    pub main: u32,
    #[serde(default)]
    pub boost: u32,
    /// Rounds or shots per loadout mount.
    #[serde(default)]
    pub rounds: [u32; 3],
    #[serde(default)]
    pub overheats: u32,
}

/// The system a loadout mount's firing wears: its arm's actuators, or the fire control.
pub fn mount_system(line: bc_proto::FrameId, m: usize) -> System {
    match frame(line).loadout.get(m).copied().flatten().map(|mount| mount.arm) {
        Some(ArmSlot::Left) => System::ActuatorL,
        Some(ArmSlot::Right | ArmSlot::Both) => System::ActuatorR,
        _ => System::FireControl,
    }
}

impl Wear {
    pub fn is_none(&self) -> bool {
        *self == Wear::default()
    }

    /// Adds a sortie's use.
    pub fn add(&mut self, u: &Usage) {
        self.main = self.main.saturating_add(u.burn);
        self.boost = self.boost.saturating_add(u.boost);
        for (r, n) in self.rounds.iter_mut().zip(u.shots) {
            *r = r.saturating_add(u32::from(n));
        }
        self.overheats = self.overheats.saturating_add(u32::from(u.overheats));
    }

    /// Each worn system, how far through its service life (1.0: it's due), in [`System`] order
    /// (a system worn by two mounts takes the more worn).
    pub fn systems(&self, line: bc_proto::FrameId) -> Vec<(System, f32)> {
        let tick = TICK_HZ as f32;
        let mut v: Vec<(System, f32)> = vec![
            (System::MainThrusters, self.main as f32 / tick / MAIN_S as f32),
            (System::Boosters, self.boost as f32 / tick / BOOST_S as f32),
            (System::Reactor, self.overheats as f32 / OVERHEATS as f32),
        ];
        for (m, r) in self.rounds.iter().enumerate() {
            let sys = mount_system(line, m);
            let f = *r as f32 / ROUNDS as f32;
            match v.iter_mut().find(|(s, _)| *s == sys) {
                Some(e) => e.1 = e.1.max(f),
                None => v.push((sys, f)),
            }
        }
        v.retain(|(_, f)| *f > 0.0);
        v.sort_by_key(|(s, _)| *s);
        v
    }

    /// Starts `sys`'s service life again.
    pub fn reset(&mut self, sys: System, line: bc_proto::FrameId) {
        match sys {
            System::MainThrusters => self.main = 0,
            System::Boosters => self.boost = 0,
            System::Reactor => self.overheats = 0,
            _ => {}
        }
        for m in 0..3 {
            if mount_system(line, m) == sys {
                self.rounds[m] = 0;
            }
        }
    }

    /// The systems past their service life: each comes home a level worse, and its life starts
    /// again. What wore out (only systems in parts that are `fitted`).
    pub fn wear_out(
        &mut self,
        faults: &mut Faults,
        line: bc_proto::FrameId,
        fitted: impl Fn(Part) -> bool,
    ) -> Vec<System> {
        let mut out = Vec::new();
        for (sys, f) in self.systems(line) {
            if f < 1.0 || !fitted(sys.part()) {
                continue;
            }
            let level = faults.level(sys);
            faults.set(sys, (level + 1).min(bc_sim::content::systems::FAILED));
            self.reset(sys, line);
            out.push(sys);
        }
        out
    }
}

/// What servicing a worn system takes: half of what overhauling it damaged would.
pub fn service_cost(line: bc_proto::FrameId) -> Vec<(Item, u64)> {
    overhaul_cost(line, bc_sim::content::systems::DAMAGED)
        .into_iter()
        .map(|(i, q)| (i, q.div_ceil(2)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::FrameId;

    #[test]
    fn systems_wear_out_after_their_service_life_and_start_again() {
        let mut w = Wear::default();
        let tick = TICK_HZ;
        let sortie = Usage { burn: MAIN_S * tick / 2, boost: 0, shots: [0, 800, 0], overheats: 2 };
        w.add(&sortie);
        let mut faults = Faults::NONE;
        assert!(w.wear_out(&mut faults, FrameId::Leo, |_| true).is_empty());
        w.add(&sortie);
        let worn = w.wear_out(&mut faults, FrameId::Leo, |_| true);
        assert!(worn.contains(&System::MainThrusters), "{worn:?}");
        assert!(worn.contains(&mount_system(FrameId::Leo, 1)), "{worn:?}");
        assert!(!worn.contains(&System::Reactor));
        assert_eq!(faults.level(System::MainThrusters), bc_sim::content::systems::DAMAGED);
        assert_eq!(w.main, 0, "its life starts again");
        // The reactor, after six overheats; a part not fitted doesn't wear.
        w.add(&Usage { overheats: 2, ..Usage::default() });
        assert!(w.wear_out(&mut faults, FrameId::Leo, |p| p != Part::Torso).is_empty());
        assert!(w.systems(FrameId::Leo).iter().any(|(s, f)| *s == System::Reactor && *f >= 1.0));
    }
}
