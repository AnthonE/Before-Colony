//! Weapons, hit detection, part damage and sabers.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind};
use bc_sim::collide::sweep_capsules;
use bc_sim::content::Capsule;
use bc_sim::math::{Rng, look_rotation};
use bc_sim::{DT, Sim, SimConfig, SuitId};
use glam::{Quat, Vec3};

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() })
}

fn human(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

#[test]
fn fast_shots_never_tunnel() {
    // 10 000 random 3 km/s shots (100 m per tick) at a 1 m-radius capsule, stepped tick by tick.
    let caps = [Capsule { a: Vec3::new(0.0, -2.0, 0.0), b: Vec3::new(0.0, 2.0, 0.0), r: 1.0 }];
    let mut rng = Rng::new(3);
    for n in 0..10_000 {
        let d = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or(Vec3::X);
        // Aim at a random point inside the capsule.
        let axis_p = Vec3::new(0.0, rng.signed() * 2.0, 0.0);
        let off = Vec3::new(rng.signed(), 0.0, rng.signed()).normalize_or(Vec3::X) * rng.next_f32() * 0.9;
        let q = axis_p + off;
        let mut p = q - d * (50.0 + rng.next_f32() * 3_000.0);
        let step = d * 3_000.0 * DT;
        let mut hit = false;
        for _ in 0..40 {
            if sweep_capsules(p, p + step, 0.1, &caps, Vec3::ZERO, Quat::IDENTITY).is_some() {
                hit = true;
                break;
            }
            p += step;
        }
        assert!(hit, "shot {n} tunneled");
    }
}

#[test]
fn beam_rifle_hits_and_breaks_parts() {
    let mut sim = empty();
    let leo = human(&mut sim, FrameId::Leo, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let target = human(&mut sim, FrameId::Taurus, Faction::Oz, Vec3::new(0.0, 1_002.0, 800.0), -Vec3::Z);
    let from = sim.events.next_seq();
    for _ in 0..90 {
        let muzzle =
            sim.suits.flight[leo.idx()].pos + sim.suits.flight[leo.idx()].rot * Vec3::new(3.4, 0.6, 3.0);
        let aim = (sim.suits.flight[target.idx()].pos + Vec3::new(0.0, 2.5, 0.0) - muzzle).normalize();
        hold(&mut sim, leo, FIRE_PRIMARY, aim);
        sim.step();
    }
    let ev = events_since(&sim, from);
    let spawns = ev.iter().filter(|e| matches!(e, Event::BeamSpawn { .. })).count();
    let hits = ev
        .iter()
        .filter(|e| matches!(e, Event::Hit { target: t, .. } if *t as usize == target.idx()))
        .count();
    assert!(spawns >= 3, "beam spawns: {spawns}");
    assert!(hits >= 3, "hits: {hits}");
    assert!(sim.suits.part_hp[target.idx()][Part::Torso as usize] < 170.0);
}

#[test]
fn losing_an_arm_loses_its_weapon() {
    let mut sim = empty();
    let leo = human(&mut sim, FrameId::Leo, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    sim.suits.part_hp[leo.idx()][Part::ArmR as usize] = 0.0; // rifle arm gone
    let from = sim.events.next_seq();
    for _ in 0..60 {
        hold(&mut sim, leo, FIRE_PRIMARY | FIRE_SECONDARY, Vec3::Z);
        sim.step();
    }
    let beams = events_since(&sim, from).iter().filter(|e| matches!(e, Event::BeamSpawn { .. })).count();
    assert_eq!(beams, 0, "beam rifle fired without its arm");
    assert!(sim.stats(leo.idx()).shots > 10, "the machine cannon (left arm) should still fire");
}

#[test]
fn twin_buster_rifle_charges_then_one_shots_a_leo() {
    let mut sim = empty();
    let zero = human(&mut sim, FrameId::WingZero, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let leo = human(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(3.4, 1_000.6, 1_500.0), -Vec3::Z);
    let from = sim.events.next_seq();
    let mut fired_at = None;
    for k in 0..60 {
        hold(&mut sim, zero, FIRE_PRIMARY, Vec3::Z);
        sim.step();
        if fired_at.is_none() && events_since(&sim, from).iter().any(|e| matches!(e, Event::BeamSpawn { .. }))
        {
            fired_at = Some(k);
        }
    }
    assert!(fired_at.unwrap() >= 17, "fired before charging: tick {fired_at:?}");
    assert!(
        events_since(&sim, from)
            .iter()
            .any(|e| matches!(e, Event::Kill { victim, .. } if *victim as usize == leo.idx())),
        "the Twin Buster Rifle should destroy a Leo outright"
    );
}

#[test]
fn saber_cuts_and_clashes() {
    let mut sim = empty();
    let a = human(&mut sim, FrameId::Leo, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let b = human(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(-2.0, 1_000.0, 9.0), -Vec3::Z);
    let from = sim.events.next_seq();
    hold(&mut sim, a, MELEE, Vec3::Z);
    for _ in 0..20 {
        sim.step();
        hold(&mut sim, a, 0, Vec3::Z);
        let tb = sim.next_tick();
        sim.set_input(b, InputCmd { tick: tb, view_tick_q4: tb << 4, aim: -Vec3::Z, ..InputCmd::default() });
    }
    let hits = events_since(&sim, from)
        .iter()
        .filter(|e| matches!(e, Event::Hit { weapon: bc_proto::WeaponKind::BeamSaber, .. }))
        .count();
    assert!(hits >= 1, "saber should connect at 9 m");

    // Both swing at once, face to face: parried.
    let mut sim = empty();
    let a = human(&mut sim, FrameId::Leo, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let b = human(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, 10.0), -Vec3::Z);
    let from = sim.events.next_seq();
    hold(&mut sim, a, MELEE, Vec3::Z);
    hold(&mut sim, b, MELEE, -Vec3::Z);
    for _ in 0..20 {
        sim.step();
        hold(&mut sim, a, 0, Vec3::Z);
        hold(&mut sim, b, 0, -Vec3::Z);
    }
    assert!(events_since(&sim, from).iter().any(|e| matches!(e, Event::Clash { .. })), "expected a clash");
}
