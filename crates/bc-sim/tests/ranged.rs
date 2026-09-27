//! Guns by class: the flamethrower burns and overheats what's in its cone, the Dragon Fang takes its
//! arm (and the flamethrower on it) along, stream weapons fire without spawn events, and the buster
//! shield is a slow beam.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY};
use bc_proto::events::Event;
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind, WeaponKind};
use bc_sim::content::{WeaponClass, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::suits::MeleePhase;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

fn hits_by(sim: &Sim, from: u32, a: SuitId, kind: WeaponKind) -> usize {
    events_since(sim, from)
        .iter()
        .filter(|e| matches!(e, Event::Hit { shooter, weapon, .. } if *shooter as usize == a.idx() && *weapon == kind))
        .count()
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);

/// Shenlong flames a Leo placed at `offset` for `ticks`, aiming straight ahead. Returns the burns
/// that landed and the Leo.
fn flame_at(offset: Vec3, ticks: u32) -> (Sim, SuitId, SuitId, usize) {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Shenlong, Faction::Colonies, AT, Vec3::Z);
    let b = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + offset, -Vec3::Z);
    let from = sim.events.next_seq();
    for _ in 0..ticks {
        hold(&mut sim, a, FIRE_SECONDARY, Vec3::Z);
        sim.step();
    }
    let burns = hits_by(&sim, from, a, WeaponKind::Flamethrower);
    (sim, a, b, burns)
}

#[test]
fn the_flamethrower_burns_what_is_in_its_cone() {
    let w = weapon(WeaponKind::Flamethrower);
    let cone = w.cone.unwrap();
    // Every `interval` ticks, starting at once: 10 burns in 60 ticks, a round each.
    let (sim, a, b, burns) = flame_at(Vec3::Z * 40.0, 60);
    assert_eq!(burns, 60 / usize::from(cone.interval));
    assert_eq!(u32::from(sim.suits.weapons[a.idx()][1].ammo), u32::from(w.ammo) - burns as u32);
    assert_ne!(sim.entity_state(a.idx(), b.idx()).flags & ent_flags::FIRING_SECONDARY, 0, "the flame shows");
    assert_eq!(sim.stats(a.idx()).hits_by_class[WeaponClass::Cone as usize], burns as u32);
    // It reaches a Leo at its range, but not 30° off the flame, nor past its reach.
    assert!(flame_at(Vec3::Z * w.range, 60).3 > 0);
    assert_eq!(flame_at(Vec3::new(40.0 * 0.5, 0.0, 40.0 * 0.866), 60).3, 0);
    assert_eq!(flame_at(Vec3::Z * (w.range + 15.0), 60).3, 0);
}

#[test]
fn the_flame_overheats_its_target() {
    let leo = frame(FrameId::Leo);
    let (sim, _, b, _) = flame_at(Vec3::Z * 30.0, 30);
    assert!(sim.suits.heat[b.idx()] > 30.0, "heat {}", sim.suits.heat[b.idx()]);
    let (sim, _, b, _) = flame_at(Vec3::Z * 30.0, 120);
    assert!(sim.suits.overheated[b.idx()], "heat {} of {}", sim.suits.heat[b.idx()], leo.heat_cap);
}

#[test]
fn the_dragon_fang_takes_its_arm_along() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Shenlong, Faction::Colonies, AT, Vec3::Z);
    suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 30.0, -Vec3::Z);
    let mut strikes = 0;
    let mut burns_while_out = 0;
    let mut burns = 0;
    for _ in 0..120 {
        let before = sim.suits.weapons[a.idx()][1].ammo;
        let out = sim.suits.melee[a.idx()].phase != MeleePhase::Idle;
        hold(&mut sim, a, FIRE_PRIMARY | FIRE_SECONDARY, Vec3::Z);
        sim.step();
        let burnt = sim.suits.weapons[a.idx()][1].ammo < before;
        burns += usize::from(burnt);
        burns_while_out += usize::from(burnt && out);
        if !out && sim.suits.melee[a.idx()].phase != MeleePhase::Idle {
            strikes += 1;
        }
    }
    assert_eq!(strikes, 3, "the fang strikes every 40 ticks");
    assert_eq!(burns_while_out, 0, "the flamethrower burned while its arm was out");
    assert!(burns >= 6, "between strikes it burns: {burns}");
}

#[test]
fn stream_weapons_fire_without_events() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let b = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::new(3.4, 0.6, 400.0), -Vec3::Z);
    let from = sim.events.next_seq();
    for _ in 0..60 {
        hold(&mut sim, a, FIRE_PRIMARY, Vec3::Z);
        sim.step();
    }
    assert!(hits_by(&sim, from, a, WeaponKind::BeamGatling) >= 5);
    assert!(events_since(&sim, from).iter().all(|e| !matches!(e, Event::BeamSpawn { .. })));
    assert_ne!(sim.entity_state(a.idx(), b.idx()).flags & ent_flags::FIRING_PRIMARY, 0);
}

#[test]
fn the_buster_shield_is_a_slow_beam() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let d = 400.0;
    suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::new(-3.4, 0.6, d), -Vec3::Z);
    let from = sim.events.next_seq();
    let w = weapon(WeaponKind::BusterShield);
    let mut spawned = None;
    let mut hit = None;
    for k in 0..60u32 {
        hold(&mut sim, a, if k == 0 { FIRE_PRIMARY } else { 0 }, Vec3::Z);
        sim.step();
        for e in events_since(&sim, from) {
            match e {
                Event::BeamSpawn { weapon: WeaponKind::BusterShield, .. } => spawned = spawned.or(Some(k)),
                Event::Hit { weapon: WeaponKind::BusterShield, .. } => hit = hit.or(Some(k)),
                _ => {}
            }
        }
    }
    assert_eq!(spawned, Some(0), "every shot is an event");
    let flight = hit.expect("a hit") - spawned.unwrap();
    let expected = d / w.speed * 30.0;
    assert!((flight as f32 - expected).abs() <= 3.0, "{flight} ticks in flight, {expected:.0} expected");
}
