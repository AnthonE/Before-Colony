//! Drone AI: squads find each other and fight to a kill.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, PilotKind};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig};
use glam::Vec3;

#[test]
fn drone_squads_fight_to_a_kill() {
    let mut sim = Sim::new(SimConfig { target_drones: 0, seed: 11, field_rocks: 0, ..SimConfig::default() });
    for k in 0..3 {
        let a = Vec3::new(-1_250.0, 1_000.0 + k as f32 * 60.0, k as f32 * 80.0);
        let b = Vec3::new(1_250.0, 1_000.0 + k as f32 * 60.0, k as f32 * 80.0);
        sim.spawn_at(FrameId::Corvus, Faction::Almagest, PilotKind::Drone, a, look_rotation(b - a, Vec3::Y))
            .unwrap();
        sim.spawn_at(FrameId::Corvus, Faction::Colonies, PilotKind::Drone, b, look_rotation(a - b, Vec3::Y))
            .unwrap();
    }
    let mut first_kill = None;
    for tick in 0..(60 * 30) {
        sim.step();
        let latest = sim.events.next_seq();
        if (latest.saturating_sub(64)..latest)
            .filter_map(|s| sim.events.get(s))
            .any(|e| matches!(e, Event::Kill { .. }))
        {
            first_kill = Some(tick);
            break;
        }
    }
    let t = first_kill.expect("no kill within 60 s");
    println!("first kill after {:.1} s", t as f32 / 30.0);
}

#[test]
fn spawner_keeps_the_patrols_topped_up() {
    let mut sim = Sim::new(SimConfig { target_drones: 12, field_rocks: 0, ..SimConfig::default() });
    for _ in 0..(20 * 30) {
        sim.step();
    }
    let drones = sim.suits.used.iter().filter(|&i| sim.suits.pilot[i] == PilotKind::Drone).count();
    assert_eq!(drones, 12);
}

/// The drones near the squad's anchor, and how far the farthest is from it, m.
fn spread(sim: &Sim, anchor: Vec3) -> (usize, f32) {
    let drones: Vec<usize> = sim.suits.alive.iter().filter(|&i| sim.suits.pilot[i] == PilotKind::Drone).collect();
    let far = drones.iter().map(|&i| (sim.suits.flight[i].pos - anchor).length()).fold(0.0, f32::max);
    (drones.len(), far)
}

#[test]
fn drones_let_a_pilot_who_flees_home_go_and_stay_on_patrol() {
    // One squad, at the first anchor; a pilot near it draws it, then flies off home at 150 m/s,
    // 20 km and more, past the drones' leash. They used to follow, run their tanks dry (they fly
    // by the real rules), coast on to the sector's edge and stay there for good, and the field was
    // left without drones.
    let mut sim = Sim::new(SimConfig { target_drones: 4, seed: 3, field_rocks: 0, ..SimConfig::default() });
    for _ in 0..(5 * 30) {
        sim.step();
    }
    let anchor = Vec3::new(2_800.0, 900.0, 0.0);
    let start = anchor + Vec3::new(-900.0, 0.0, 0.0);
    let pilot = sim
        .spawn_at(FrameId::Argo, Faction::Colonies, PilotKind::Human, start, look_rotation(anchor - start, Vec3::Y))
        .unwrap()
        .idx();
    for _ in 0..(10 * 30) {
        sim.step();
    }
    let home = Vec3::new(-17_000.0, -2_000.0, 0.0);
    for _ in 0..(4 * 60 * 30) {
        if sim.suits.alive.get(pilot) {
            let f = &mut sim.suits.flight[pilot];
            f.vel = if (home - f.pos).length() > 200.0 { (home - f.pos).normalize() * 150.0 } else { Vec3::ZERO };
        }
        sim.step();
    }
    let (n, far) = spread(&sim, anchor);
    println!("{n} drones, the farthest {far:.0} m from the anchor");
    assert!(n >= 1);
    assert!(far < bc_sim::ai::LEASH + 2_000.0, "a drone followed the pilot home: {far:.0} m out");
}

#[test]
fn a_drone_run_dry_at_the_sector_edge_refuels_and_comes_back() {
    let mut sim = Sim::new(SimConfig { target_drones: 4, seed: 5, field_rocks: 0, ..SimConfig::default() });
    for _ in 0..(5 * 30) {
        sim.step();
    }
    let anchor = Vec3::new(2_800.0, 900.0, 0.0);
    let i = sim.suits.alive.iter().find(|&i| sim.suits.pilot[i] == PilotKind::Drone).unwrap();
    // Stranded as the chases left them: against the sector's edge, its tank empty.
    let edge = Vec3::new(-bc_sim::config::SECTOR_LIMIT, 900.0, 0.0);
    sim.suits.flight[i].pos = edge;
    sim.suits.flight[i].vel = Vec3::ZERO;
    sim.suits.flight[i].propellant = 0.0;
    for _ in 0..(5 * 60 * 30) {
        sim.step();
    }
    let d = (sim.suits.flight[i].pos - anchor).length();
    println!("5 min on: {d:.0} m from its anchor, {:.0} kg in its tank", sim.suits.flight[i].propellant);
    assert!(d < 6_000.0, "still out at {d:.0} m");
}
