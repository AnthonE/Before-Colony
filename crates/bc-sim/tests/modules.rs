//! Equipment: each module does what it says where it counts, weighs what it weighs, and goes with
//! the part it's on.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, FrameId, Part, PilotKind};
use bc_sim::content::modules::{MOUNTS, REPAIR_TICKS};
use bc_sim::content::systems::DAMAGED;
use bc_sim::content::{ModuleKind, Modules, System, Systems, frame};
use bc_sim::sim::Loadout;
use bc_sim::{Sim, SimConfig, SuitId};

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn with(kinds: &[ModuleKind]) -> Modules {
    let mut m = Modules::NONE;
    for k in kinds {
        let slot = (0..MOUNTS.len()).find(|s| MOUNTS[*s] == k.part() && m.get(*s).is_none()).unwrap();
        m.set(slot, Some(*k));
    }
    m
}

fn launch(sim: &mut Sim, kinds: &[ModuleKind], systems: Systems) -> SuitId {
    let mut l = Loadout::full(FrameId::Leo);
    l.modules = with(kinds);
    l.systems = systems;
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap();
    sim.step();
    id
}

#[test]
fn each_module_changes_its_stat() {
    let mut s = sim();
    let bare = launch(&mut s, &[], Systems::OK).idx();
    let t0 = s.tuning(bare);
    let check = |kind: ModuleKind, f: &dyn Fn(&bc_sim::tuning::Tuning, &bc_sim::tuning::Tuning) -> bool| {
        let mut s = sim();
        let id = launch(&mut s, &[kind], Systems::OK).idx();
        let t = s.tuning(id);
        assert!(f(&t0, &t), "{kind:?}: {t:?}");
        assert_eq!(t.module_kg, kind.mass_kg());
    };
    check(ModuleKind::SensorArray, &|a, b| b.sensor > a.sensor && b.signature > a.signature);
    check(ModuleKind::FireControlComputer, &|a, b| b.lock_step > a.lock_step);
    check(ModuleKind::CapacitorBank, &|a, b| b.energy_cap > a.energy_cap);
    let mut s = sim();
    let id = launch(&mut s, &[ModuleKind::CapacitorBank], Systems::OK).idx();
    assert!(s.suits.energy[id] > frame(FrameId::Leo).energy_cap * 1.4, "launched charged full");
    check(ModuleKind::ReactorBooster, &|a, b| b.regen > a.regen && b.heat < a.heat);
    check(ModuleKind::RadiatorPackage, &|a, b| b.heat > a.heat && b.signature > a.signature);
    check(ModuleKind::CompositePlating, &|a, b| b.armor < a.armor);
    check(ModuleKind::GSeat, &|a, b| b.g_tolerance > a.g_tolerance);
    check(ModuleKind::DamageControl, &|_, b| b.repairs);
    check(ModuleKind::AuxiliaryTank, &|a, b| b.tank > a.tank);
    check(ModuleKind::ExtendedTank, &|a, b| b.tank > a.tank);
    check(ModuleKind::IonDrive, &|a, b| b.ion > a.ion);
    check(ModuleKind::ThrusterKit, &|a, b| b.main > a.main && b.isp < a.isp);
    check(ModuleKind::LegVerniers, &|a, b| b.side > a.side);
    check(ModuleKind::CargoRack, &|a, b| b.hold_kg > a.hold_kg && b.ambac < a.ambac);
}

/// The auxiliary tank holds more at launch; modules weigh the suit down.
#[test]
fn a_bigger_tank_fills_and_modules_weigh() {
    let mut s = sim();
    let mut l = Loadout::full(FrameId::Leo);
    l.modules = with(&[ModuleKind::AuxiliaryTank, ModuleKind::CompositePlating]);
    l.propellant = 10_000.0;
    let id = s.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap().idx();
    let cap = frame(FrameId::Leo).propellant_cap;
    assert!((s.suits.flight[id].propellant - cap * 1.4).abs() < 1.0);
    s.step();
    assert_eq!(s.flight_mods(id).extra_mass_kg, 800);
}

/// The extended tank (torso) and the auxiliary tank (backpack) stack: a Leo holds 1.75 tanks.
#[test]
fn the_tanks_stack() {
    let mut s = sim();
    let mut l = Loadout::full(FrameId::Leo);
    l.modules = with(&[ModuleKind::ExtendedTank, ModuleKind::AuxiliaryTank]);
    l.propellant = 10_000.0;
    let id = s.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap().idx();
    assert_eq!(s.suits.flight[id].propellant, 5_250.0);
    assert_eq!(s.tuning(id).tank, 1.75);
    s.step();
    assert_eq!(s.flight_mods(id).extra_mass_kg, 350);
    // The backpack shot off takes the auxiliary tank: the extended one's left.
    s.suits.part_hp[id][Part::Backpack as usize] = 0.0;
    s.step();
    assert_eq!(s.tuning(id).tank, 1.25);
}

/// A part shot off takes its module with it: a sensor array on a head that's gone sees nothing
/// more, and doesn't come home.
#[test]
fn a_module_goes_with_its_part() {
    let mut s = sim();
    let id = launch(&mut s, &[ModuleKind::SensorArray, ModuleKind::GSeat], Systems::OK);
    let i = id.idx();
    assert!(s.tuning(i).sensor > 1.0);
    s.suits.part_hp[i][Part::Head as usize] = 0.0;
    s.step();
    assert_eq!(s.tuning(i).sensor, 0.4, "the sub-camera, and no array");
    assert!(s.tuning(i).g_tolerance > 6.0, "the G-seat is in the torso");
}

/// Damage control mends one damaged system at a time, drawing energy while it works.
#[test]
fn damage_control_mends_damaged_systems() {
    let mut s = sim();
    let faults = Systems::OK.with(System::Gyros, DAMAGED).with(System::Radiators, DAMAGED);
    let i = launch(&mut s, &[ModuleKind::DamageControl], faults).idx();
    assert_eq!(s.suits.status[i].repairing, System::Radiators as u8);
    for _ in 0..u32::from(REPAIR_TICKS) {
        s.step();
    }
    let now = s.suits.systems[i];
    assert_eq!(now.get(System::Radiators), 0, "the first is mended");
    assert_eq!(now.get(System::Gyros), DAMAGED, "the second is under way");
    assert_eq!(s.suits.status[i].repairing, System::Gyros as u8);
    for _ in 0..u32::from(REPAIR_TICKS) + 2 {
        s.step();
    }
    assert!(s.suits.systems[i].is_ok());
    // Without it, nothing mends by itself.
    let mut s = sim();
    let i = launch(&mut s, &[], faults).idx();
    for _ in 0..u32::from(REPAIR_TICKS) + 2 {
        s.step();
    }
    assert_eq!(s.suits.systems[i], faults);
}
