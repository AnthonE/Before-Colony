//! The ion drive (`content::modules`, `flight::ion_thrust`): under the real rules the first of each
//! tick's thrust is the drive's, on the reactor's power, and burns nothing; a dry tank crawls on
//! it; a scrammed reactor stops it. Under anime rules it fills the gauge faster. It takes from the
//! reactor's regeneration while it works.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind};
use bc_sim::content::modules::{ION_DRIVE_G, MOUNTS};
use bc_sim::content::systems::{DAMAGED, FAILED};
use bc_sim::content::{ModuleKind, Modules, System, Systems, frame};
use bc_sim::flight::ion_thrust;
use bc_sim::math::look_rotation;
use bc_sim::tuning::{FlightRules, tuning};
use bc_sim::{Sim, SimConfig, SuitId, config::G0};
use glam::Vec3;

fn sim(flight: FlightRules) -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, flight, ..SimConfig::default() })
}

fn with(kinds: &[ModuleKind]) -> Modules {
    let mut m = Modules::NONE;
    for k in kinds {
        let slot = (0..MOUNTS.len()).find(|s| MOUNTS[*s] == k.part() && m.get(*s).is_none()).unwrap();
        m.set(slot, Some(*k));
    }
    m
}

/// A Leo flying free, with `kinds` fitted, its systems at `systems`, `propellant` in its tank.
fn leo(sim: &mut Sim, kinds: &[ModuleKind], systems: Systems, propellant: f32) -> SuitId {
    let at = Vec3::new(0.0, 9_000.0, 0.0);
    let id = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, at, look_rotation(Vec3::Z, Vec3::Y))
        .unwrap();
    let i = id.idx();
    sim.suits.modules[i] = with(kinds);
    sim.suits.systems[i] = systems;
    sim.suits.retune(i);
    sim.suits.flight[i].propellant = propellant;
    id
}

/// `n` ticks of thrust `forward` (of 127) straight ahead, flight assist off.
fn thrust(sim: &mut Sim, id: SuitId, forward: i8, n: u32) {
    for _ in 0..n {
        let t = sim.next_tick();
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim: Vec3::Z,
            thrust: [0, 0, forward],
            ..InputCmd::default()
        };
        sim.set_input(id, cmd);
        sim.step();
    }
}

#[test]
fn the_drive_runs_on_the_reactor() {
    let drive = with(&[ModuleKind::IonDrive]);
    assert_eq!(tuning(0, Systems::OK, Modules::NONE).ion, 0.0);
    assert_eq!(tuning(0, Systems::OK, drive).ion, 1.0);
    assert_eq!(tuning(0, Systems::OK.with(System::Reactor, DAMAGED), drive).ion, 0.5);
    assert_eq!(tuning(0, Systems::OK.with(System::Reactor, FAILED), drive).ion, 0.15);
    // The backpack shot off takes the drive with it.
    assert_eq!(tuning(1 << Part::Backpack as u8, Systems::OK, drive).ion, 0.0);
    let leo = frame(FrameId::Leo);
    assert_eq!(ion_thrust(leo), ION_DRIVE_G * G0 * leo.mass(leo.propellant_cap));
}

/// Gentle thrust, under the drive's, burns nothing; full thrust burns all but the drive's share.
#[test]
fn the_drive_s_share_burns_nothing() {
    let spec = frame(FrameId::Leo);
    let f_ion = ion_thrust(spec);
    // 3/127 of a Leo's main thrust is under the drive's.
    assert!(spec.main_thrust * 3.0 / 127.0 < f_ion);
    let mut s = sim(FlightRules::Real);
    let id = leo(&mut s, &[ModuleKind::IonDrive], Systems::OK, 1_000.0);
    thrust(&mut s, id, 3, 30);
    let f = s.suits.flight[id.idx()];
    assert_eq!(f.propellant, 1_000.0, "nothing burnt");
    assert!(f.vel.z > 0.3, "it moved: {}", f.vel);
    let mut s = sim(FlightRules::Real);
    let id = leo(&mut s, &[], Systems::OK, 1_000.0);
    thrust(&mut s, id, 3, 30);
    assert!(s.suits.flight[id.idx()].propellant < 1_000.0, "without the drive it burns");
    // Full thrust: the drive's share comes off the burn.
    let burnt = |kinds: &[ModuleKind]| {
        let mut s = sim(FlightRules::Real);
        let id = leo(&mut s, kinds, Systems::OK, 1_000.0);
        thrust(&mut s, id, 127, 1);
        1_000.0 - s.suits.flight[id.idx()].propellant
    };
    let (with_drive, without) = (burnt(&[ModuleKind::IonDrive]), burnt(&[]));
    let want = (spec.main_thrust - f_ion) / spec.main_thrust;
    assert!((with_drive / without - want).abs() < 1e-3, "{with_drive} of {without}: want ×{want}");
}

/// Dry, a suit with the drive crawls on it (and with none it doesn't move); a scrammed reactor
/// stops it until it's back.
#[test]
fn a_dry_tank_crawls_home_on_the_drive() {
    let spec = frame(FrameId::Leo);
    let mut s = sim(FlightRules::Real);
    let id = leo(&mut s, &[ModuleKind::IonDrive], Systems::OK, 0.0);
    let i = id.idx();
    thrust(&mut s, id, 127, 30);
    let mass = spec.mass(0.0) + ModuleKind::IonDrive.mass_kg() as f32;
    let want = ion_thrust(spec) / mass;
    let got = s.suits.flight[i].vel.z;
    assert!((got / want - 1.0).abs() < 0.02, "{got} m/s after a second, want {want}");
    assert_eq!(s.suits.flight[i].propellant, 0.0);
    // Scrammed, it coasts.
    s.suits.status[i].scram = 20;
    let before = s.suits.flight[i].vel;
    thrust(&mut s, id, 127, 20);
    assert_eq!(s.suits.flight[i].vel, before, "no power, no thrust");
    thrust(&mut s, id, 127, 5);
    assert!(s.suits.flight[i].vel.z > before.z, "and back on once the reactor is");
    // No drive: dry is dry.
    let mut s = sim(FlightRules::Real);
    let id = leo(&mut s, &[], Systems::OK, 0.0);
    thrust(&mut s, id, 127, 30);
    assert_eq!(s.suits.flight[id.idx()].vel, Vec3::ZERO);
}

/// Under anime rules the gauge fills faster with a drive: half again, or a quarter on a damaged
/// reactor.
#[test]
fn the_drive_fills_the_gauge_faster() {
    let filled = |kinds: &[ModuleKind], systems: Systems| {
        let mut s = sim(FlightRules::Anime);
        let id = leo(&mut s, kinds, systems, 0.0);
        thrust(&mut s, id, 0, 30);
        s.suits.flight[id.idx()].propellant
    };
    let none = filled(&[], Systems::OK);
    assert!(none > 0.0);
    let full = filled(&[ModuleKind::IonDrive], Systems::OK);
    assert!((full / none - 1.5).abs() < 1e-3, "{full} {none}");
    // (A damaged reactor's tank refills as fast; only the drive is down.)
    let damaged = filled(&[ModuleKind::IonDrive], Systems::OK.with(System::Reactor, DAMAGED));
    assert!((damaged / none - 1.25).abs() < 1e-3, "{damaged} {none}");
}

/// Working flat out, the drive takes half the reactor's regeneration.
#[test]
fn the_drive_draws_on_the_reactor() {
    let regen = |kinds: &[ModuleKind]| {
        let mut s = sim(FlightRules::Real);
        let id = leo(&mut s, kinds, Systems::OK, 0.0);
        s.suits.energy[id.idx()] = 0.0;
        thrust(&mut s, id, 127, 30);
        s.suits.energy[id.idx()]
    };
    let (with_drive, without) = (regen(&[ModuleKind::IonDrive]), regen(&[]));
    assert!(without > 0.0);
    assert!((with_drive / without - 0.5).abs() < 1e-3, "{with_drive} {without}");
}
