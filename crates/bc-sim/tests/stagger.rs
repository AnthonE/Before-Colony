//! Stagger (`bc_sim::sim::stagger`, after Armored Core VI): blows build impact on a suit's attitude
//! control; past its frame's stability it's staggered, tumbling with its thrust cut and its weapons
//! down, and blows meanwhile are direct hits; impact drains away from a suit left alone.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind, WeaponKind};
use bc_sim::content::frame;
use bc_sim::content::stagger::{DIRECT_HIT, RECOVER_AFTER, STAGGER_THRUST, STAGGER_TICKS, impact, stability};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pilot: PilotKind, pos: Vec3) -> SuitId {
    let id = sim.spawn_at(frame, faction, pilot, pos, look_rotation(Vec3::Z, Vec3::Y)).unwrap();
    // A torso nothing here gets through: these tests are about the push, not the damage.
    sim.suits.part_hp[id.idx()][Part::Torso as usize] = 1.0e6;
    id
}

fn fly(sim: &mut Sim, id: SuitId, buttons: u16, thrust: [i8; 3], aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() },
    );
    sim.step();
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// A blow of exactly what tips suit `id` over, by beam (impact 1 a point).
fn push(sim: &mut Sim, id: SuitId, by: SuitId, points: f32) {
    sim.strike(id.idx(), Part::Torso, points, by.idx(), WeaponKind::BeamRifle);
}

#[test]
fn blows_build_impact_until_the_suit_is_staggered() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let foe = suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(0.0, 1_000.0, 500.0));
    let limit = stability(FrameId::Leo);
    // Most of the way: impact, no stagger.
    push(&mut sim, leo, foe, limit * 0.6);
    fly(&mut sim, leo, FLIGHT_ASSIST, [0; 3], Vec3::Z);
    assert!(!sim.staggered(leo.idx()));
    assert!((sim.impact_share(leo.idx()) - 0.6).abs() < 1e-3, "{}", sim.impact_share(leo.idx()));
    assert_eq!(sim.own_state(leo.idx()).impact, 4, "in sixths on the wire, rounded");
    // A blade's stroke pushes harder than a beam's.
    assert!(impact(WeaponKind::BeamSaber) > impact(WeaponKind::BeamRifle));
    // Over the top.
    let from = sim.events.next_seq();
    push(&mut sim, leo, foe, limit * 0.5);
    fly(&mut sim, leo, FLIGHT_ASSIST, [0; 3], Vec3::Z);
    assert!(sim.staggered(leo.idx()));
    assert!(
        events_since(&sim, from)
            .iter()
            .any(|e| matches!(e, Event::Staggered { suit, .. } if *suit as usize == leo.idx()))
    );
    let own = sim.own_state(leo.idx());
    assert_eq!(own.stagger, STAGGER_TICKS - 1, "its pilot's client flies on from this");
    assert_eq!(own.weapon_ready, 0, "nothing is ready");
    // The blow knocked it spinning, free as it is.
    assert!(sim.suits.flight[leo.idx()].ang_vel.length() > 1.0);
    // Staggered for its ticks, then steady again with nothing built up.
    for _ in 0..STAGGER_TICKS - 1 {
        assert!(sim.staggered(leo.idx()));
        fly(&mut sim, leo, FLIGHT_ASSIST, [0; 3], Vec3::Z);
    }
    assert!(!sim.staggered(leo.idx()));
    assert_eq!(sim.impact_share(leo.idx()), 0.0);
}

#[test]
fn a_staggered_suit_tumbles_cannot_fire_and_takes_direct_hits() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let foe = suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(0.0, 1_000.0, 500.0));
    // Steady, flown flat out forward, aim held: it holds its heading and fires.
    for _ in 0..10 {
        fly(&mut sim, leo, FLIGHT_ASSIST | FIRE_SECONDARY, [0, 0, 127], Vec3::Z);
    }
    let shots = sim.stats(leo.idx()).shots;
    assert!(shots > 0, "it fires steady");
    let v0 = sim.suits.flight[leo.idx()].vel;
    push(&mut sim, leo, foe, stability(FrameId::Leo) + 1.0);
    fly(&mut sim, leo, FLIGHT_ASSIST | FIRE_SECONDARY, [0, 0, 127], Vec3::Z);
    assert!(sim.staggered(leo.idx()));
    let spin = sim.suits.flight[leo.idx()].ang_vel;
    let v1 = sim.suits.flight[leo.idx()].vel;
    for _ in 0..10 {
        fly(&mut sim, leo, FLIGHT_ASSIST | FIRE_PRIMARY | FIRE_SECONDARY, [0, 0, 127], Vec3::Z);
    }
    // No shots, the spin the blow left (no attitude control to stop it), and a quarter of the
    // thrust: it gains a quarter as much speed as it did steady (its cruise caps that too).
    assert_eq!(sim.stats(leo.idx()).shots, shots, "it fired staggered");
    assert!(
        (sim.suits.flight[leo.idx()].ang_vel - spin).length() < 1e-4,
        "its attitude control did something"
    );
    let gained = (sim.suits.flight[leo.idx()].vel - v1).length();
    let steady = (v1 - v0).length().max(1e-3);
    assert!(
        gained > 0.0 && gained <= steady * 10.0 * STAGGER_THRUST * 1.2 + 0.5,
        "gained {gained} m/s, steady {steady}"
    );
    // A blow on it now is a direct hit.
    let (arm, armour) = (Part::ArmL as usize, frame(FrameId::Leo).armor);
    let before = sim.suits.part_hp[leo.idx()][arm];
    sim.strike(leo.idx(), Part::ArmL, 10.0, foe.idx(), WeaponKind::MachineCannon);
    fly(&mut sim, leo, FLIGHT_ASSIST, [0; 3], Vec3::Z);
    let lost = before - sim.suits.part_hp[leo.idx()][arm];
    assert!((lost - 10.0 * armour * DIRECT_HIT).abs() < 1e-3, "lost {lost}");
}

#[test]
fn impact_drains_from_a_suit_left_alone_and_dolls_stagger_too() {
    let mut sim = empty();
    let doll =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, Vec3::new(0.0, 1_000.0, 0.0));
    let leo =
        suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 900.0));
    let limit = stability(FrameId::Taurus);
    push(&mut sim, doll, leo, limit * 0.8);
    sim.step();
    let built = sim.suits.impact[doll.idx()];
    // Left alone a while, it settles.
    for _ in 0..RECOVER_AFTER - 2 {
        sim.step();
    }
    assert_eq!(sim.suits.impact[doll.idx()], built, "not yet");
    for _ in 0..60 {
        sim.step();
    }
    assert!(sim.suits.impact[doll.idx()] < built * 0.5, "{} of {built}", sim.suits.impact[doll.idx()]);
    // A Mobile Doll is staggered like anyone.
    push(&mut sim, doll, leo, limit);
    sim.step();
    assert!(sim.staggered(doll.idx()));
}

#[test]
fn a_suit_on_its_feet_stumbles_to_a_stop() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() });
    let id = common_standing(&mut sim);
    let foe =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(9_000.0, 9_000.0, 9_000.0));
    let i = id.idx();
    let aim = sim.suits.aim[i];
    for _ in 0..30 {
        fly(&mut sim, id, FLIGHT_ASSIST | bc_proto::buttons::GRIP, [0, 0, 127], aim);
    }
    let walking = sim.suits.anchor[i].vel.length();
    assert!(walking > 3.0, "walking at {walking} m/s");
    push(&mut sim, id, foe, stability(FrameId::Leo) + 1.0);
    for _ in 0..STAGGER_TICKS - 2 {
        fly(&mut sim, id, FLIGHT_ASSIST | bc_proto::buttons::GRIP, [0, 0, 127], aim);
    }
    assert_eq!(sim.suits.footing[i], bc_sim::ground::Footing::Grounded, "it stays on its feet");
    assert!(sim.suits.anchor[i].vel.length() < 0.5, "still walking at {}", sim.suits.anchor[i].vel.length());
}

/// A Leo standing on the biggest grippable rock (as `common::resting_on` puts it).
fn common_standing(sim: &mut Sim) -> SuitId {
    let (_, rock) = common::grippable_rock(sim, 200.0);
    let (id, _) = common::resting_on(sim, &rock);
    sim.suits.part_hp[id.idx()][Part::Torso as usize] = 1.0e6;
    // Grip it, and let it settle on its feet.
    for _ in 0..90 {
        let aim = sim.suits.aim[id.idx()];
        fly(sim, id, FLIGHT_ASSIST | bc_proto::buttons::GRIP, [0; 3], aim);
    }
    assert_eq!(sim.suits.footing[id.idx()], bc_sim::ground::Footing::Grounded, "standing");
    id
}

mod common;
