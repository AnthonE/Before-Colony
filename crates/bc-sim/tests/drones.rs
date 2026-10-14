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
