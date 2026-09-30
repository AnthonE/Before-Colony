//! What's inside the parts: blows that get through thinning armour damage the systems behind it,
//! and damaged or failed systems make the suit worse at what they do.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind, WeaponKind};
use bc_sim::content::systems::{DAMAGED, FAILED, SCRAM_TICKS};
use bc_sim::content::{System, Systems, frame};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, f: FrameId, pilot: PilotKind, faction: Faction, pos: Vec3) -> SuitId {
    sim.spawn_at(f, faction, pilot, pos, look_rotation(Vec3::Z, Vec3::Y)).unwrap()
}

fn system_hits(sim: &Sim, from: u32, target: usize) -> Vec<(u8, u8)> {
    (from..sim.events.next_seq())
        .filter_map(|s| match sim.events.get(s) {
            Some(Event::SystemHit { target: j, system, level, .. }) if *j as usize == target => {
                Some((*system, *level))
            }
            _ => None,
        })
        .collect()
}

/// How many times, on average, blows of `amount` reach a system in `f`'s torso before the torso
/// would be gone.
fn crits_per_life(f: FrameId, pilot: PilotKind, amount: f32, lives: usize) -> f32 {
    let mut sim = empty();
    let j = suit(&mut sim, f, pilot, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
    let spec = frame(f);
    let max = spec.part_hp[Part::Torso as usize];
    let mut total = 0usize;
    for _ in 0..lives {
        sim.suits.part_hp[j] = spec.part_hp;
        sim.suits.systems[j] = Systems::OK;
        let from = sim.events.next_seq();
        // Stop short of the blow that would destroy it (that one reaches nothing).
        while sim.suits.part_hp[j][Part::Torso as usize] > amount * spec.armor + 1e-3 {
            sim.strike(j, Part::Torso, amount, usize::MAX, WeaponKind::BeamRifle);
            sim.step();
        }
        total += system_hits(&sim, from, j).len();
        assert!(sim.suits.part_hp[j][Part::Torso as usize] <= max);
    }
    total as f32 / lives as f32
}

/// A part's armour protects what's inside it: fresh, blows seldom get through; thinned, they
/// do. Over a part's life about two blows reach a system, whatever the weapon.
#[test]
fn blows_through_thin_armour_reach_the_systems() {
    for (what, amount) in [("machine cannon", 6.0), ("beam rifle", 45.0), ("beam saber", 90.0)] {
        let pilot = crits_per_life(FrameId::Leo, PilotKind::Human, amount, 120);
        let doll = crits_per_life(FrameId::Leo, PilotKind::MobileDoll, amount, 120);
        println!("{what}: {pilot:.2} systems hit per torso's life (a Mobile Doll's: {doll:.2})");
        assert!((0.9..=2.6).contains(&pilot), "{what}: {pilot}");
        assert!(doll < pilot, "{what}: dolls are built simply ({doll} vs {pilot})");
    }
    // Fresh armour stops a small blow outright.
    let mut sim = empty();
    let j = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
    let from = sim.events.next_seq();
    sim.strike(j, Part::Torso, 1.0, usize::MAX, WeaponKind::MachineCannon);
    sim.step();
    assert!(system_hits(&sim, from, j).is_empty());
    assert!(sim.suits.systems[j].is_ok());
}

/// The same blows reach the same systems on every run.
#[test]
fn criticals_are_the_same_every_time() {
    let run = || {
        let mut sim = empty();
        let j =
            suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
        for _ in 0..4 {
            sim.strike(j, Part::Torso, 45.0, usize::MAX, WeaponKind::BeamRifle);
            sim.step();
        }
        (sim.suits.systems[j], sim.state_hash())
    };
    assert_eq!(run(), run());
}

/// A struck reactor scrams: no energy comes back for four seconds, and after that it comes back
/// at what the damaged reactor gives.
#[test]
fn a_scrammed_reactor_gives_nothing_for_a_while() {
    let mut sim = empty();
    let j = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
    sim.suits.systems[j] = Systems::OK.with(System::Reactor, DAMAGED);
    sim.suits.status[j].scram = SCRAM_TICKS;
    sim.suits.energy[j] = 0.0;
    for _ in 0..u32::from(SCRAM_TICKS) - 2 {
        sim.step();
    }
    assert_eq!(sim.suits.energy[j], 0.0, "scrammed");
    for _ in 0..30 {
        sim.step();
    }
    let regen = frame(FrameId::Leo).energy_regen;
    let got = sim.suits.energy[j];
    assert!(got > 0.0 && got < regen * 0.6, "back at half: {got}");
}

/// A holed tank loses propellant, and the worse the hole the faster.
#[test]
fn a_holed_tank_leaks() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
    let b = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(500.0, 1_500.0, 0.0)).idx();
    sim.suits.systems[a] = Systems::OK.with(System::Tank, DAMAGED);
    sim.suits.systems[b] = Systems::OK.with(System::Tank, FAILED);
    let before = (sim.suits.flight[a].propellant, sim.suits.flight[b].propellant);
    for _ in 0..301 {
        sim.step();
    }
    let lost = (before.0 - sim.suits.flight[a].propellant, before.1 - sim.suits.flight[b].propellant);
    assert!((lost.0 - 30.0).abs() < 1.0 && (lost.1 - 150.0).abs() < 1.0, "{lost:?}");
}

/// Damaged sensors see less far: a suit 5 km off is on a whole Leo's sensors (6 km), not on one
/// whose sensors are damaged (4.2 km).
#[test]
fn damaged_sensors_see_less_far() {
    let mut sim = empty();
    let a =
        suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Colonies, Vec3::new(0.0, 1_500.0, -2_500.0));
    let b = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 2_500.0));
    // (Past the first second, when everyone reads as having just fired.)
    for _ in 0..40 {
        sim.step();
    }
    assert!(sim.detects(a.idx(), b.idx()));
    sim.suits.systems[a.idx()] = Systems::OK.with(System::Sensors, DAMAGED);
    sim.step();
    assert!(!sim.detects(a.idx(), b.idx()));
}

/// Struck actuators jam the arm's weapons for a while, and failed ones narrow where they point
/// and leave the hand unable to hold anything.
#[test]
fn struck_actuators_jam_and_failed_ones_let_go() {
    let mut sim = empty();
    let j = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 0.0)).idx();
    let spec = frame(FrameId::Leo);
    let mut jammed = false;
    for _ in 0..200 {
        sim.suits.part_hp[j] = spec.part_hp;
        sim.suits.systems[j] = Systems::OK;
        sim.suits.weapons[j][0].cooldown = 0;
        let from = sim.events.next_seq();
        sim.strike(j, Part::ArmR, 60.0, usize::MAX, WeaponKind::BeamSaber);
        sim.step();
        if system_hits(&sim, from, j).iter().any(|(s, _)| *s == System::ActuatorR as u8) {
            // The Leo's beam rifle is in its right hand.
            assert!(sim.suits.weapons[j][0].cooldown >= 80, "jammed: {}", sim.suits.weapons[j][0].cooldown);
            jammed = true;
            break;
        }
    }
    assert!(jammed, "a blow reached the right arm's actuators");
    sim.suits.systems[j] = Systems::OK.with(System::ActuatorL, FAILED);
    sim.step();
    assert_eq!(sim.suits.grab_hand(j), Some(true), "the left hand can't grip: the right grabs");
    let arm = bc_sim::content::ArmSlot::Left;
    assert!(sim.cone(j, arm) < arm.cone() * 0.5);
}

/// Without fire control no missile lock builds; damaged, one builds at half the rate.
#[test]
fn fire_control_builds_the_lock() {
    let lock_after = |fcs: u8, ticks: u32| {
        let mut sim = empty();
        let me = suit(
            &mut sim,
            FrameId::Heavyarms,
            PilotKind::Human,
            Faction::Colonies,
            Vec3::new(0.0, 1_500.0, 0.0),
        );
        let foe = suit(&mut sim, FrameId::Leo, PilotKind::Human, Faction::Oz, Vec3::new(0.0, 1_500.0, 800.0));
        sim.suits.systems[me.idx()] = Systems::OK.with(System::FireControl, fcs);
        for _ in 0..ticks {
            let t = sim.next_tick();
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: Vec3::Z,
                lock_target: foe.idx() as u16,
                ..InputCmd::default()
            };
            sim.set_input(me, cmd);
            sim.step();
        }
        sim.missile_lock(me.idx()).is_some()
    };
    let lock_ticks = u32::from(frame(FrameId::Heavyarms).lock_spec().unwrap().lock_ticks);
    assert!(lock_after(0, lock_ticks + 3), "whole fire control locks in its time");
    assert!(!lock_after(DAMAGED, lock_ticks + 3), "damaged, it takes longer");
    assert!(lock_after(DAMAGED, 2 * lock_ticks + 4));
    assert!(!lock_after(FAILED, 4 * lock_ticks), "failed, never");
}

/// A suit launched with damaged systems has them out there, and brings its faults home.
#[test]
fn faults_launch_and_come_home() {
    use bc_sim::sim::Loadout;
    let mut sim =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() });
    let mut l = Loadout::full(FrameId::Leo);
    l.systems = Systems::OK.with(System::Radiators, DAMAGED).with(System::Boosters, FAILED);
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap();
    sim.step();
    assert_eq!(sim.suits.systems[id.idx()], l.systems);
    assert_eq!(sim.tuning(id.idx()).boost, 0.0);
    assert!(sim.tuning(id.idx()).heat < 1.0);
}
