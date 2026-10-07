//! Losing a suit (`bc_sim::sim::doom`): a pilot's breached suit is doomed and fights on, blows cut
//! its doom short, and its pilot can eject (its wreck left for the tugs) or, doomed, blow it up.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::ChunkKind;
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, NO_CHUNK, NO_SLOT, Part, PilotKind, WeaponKind};
use bc_sim::chunks::Motion;
use bc_sim::colony::interior::WorldKind;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::content::{frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::sim::{DOOM_PER_TORSO, DOOM_TICKS, EJECT_SPEED, Ejected, Loadout};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pilot: PilotKind, pos: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, pilot, pos, look_rotation(Vec3::Z, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// Steps with every suit in `ids` holding still, `ticks` times.
fn idle(sim: &mut Sim, ids: &[SuitId], ticks: u32) {
    for _ in 0..ticks {
        let t = sim.next_tick();
        for &id in ids {
            let aim = sim.suits.aim[id.idx()];
            sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, ..InputCmd::default() });
        }
        sim.step();
    }
}

/// A blow that breaches `target`'s torso, from `by`.
fn breach(sim: &mut Sim, target: SuitId, by: SuitId) {
    let torso = sim.suits.part_hp[target.idx()][Part::Torso as usize];
    let armour = frame(sim.suits.frame[target.idx()]).armor;
    sim.strike(target.idx(), Part::Torso, torso / armour + 1.0, by.idx(), WeaponKind::BeamRifle);
}

fn kills_of(ev: &[Event], victim: SuitId) -> Vec<(u16, u16)> {
    ev.iter()
        .filter_map(|e| match *e {
            Event::Kill { victim: v, killer, hulk, .. } if v as usize == victim.idx() => Some((killer, hulk)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_pilots_breached_suit_is_doomed_then_lost_to_whoever_breached_it() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let doll =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, Vec3::new(0.0, 1_000.0, 500.0));
    let from = sim.events.next_seq();
    breach(&mut sim, leo, doll);
    idle(&mut sim, &[leo], 1);
    // Breached, but still flying: doomed.
    assert!(sim.is_alive(leo.idx()) && sim.doomed(leo.idx()));
    assert!(
        events_since(&sim, from)
            .iter()
            .any(|e| matches!(e, Event::Doomed { suit, .. } if *suit as usize == leo.idx()))
    );
    assert_eq!(u16::from(sim.own_state(leo.idx()).doom), DOOM_TICKS - 1, "its pilot sees the clock");
    idle(&mut sim, &[leo], u32::from(DOOM_TICKS) - 2);
    assert!(sim.is_alive(leo.idx()), "it lasts its doom out");
    idle(&mut sim, &[leo], 1);
    assert!(!sim.is_alive(leo.idx()), "then its reactor goes");
    let kills = kills_of(&events_since(&sim, from), leo);
    assert_eq!(kills.len(), 1);
    assert_eq!(kills[0].0 as usize, doll.idx(), "credited to the Doll that breached it");
    assert_ne!(kills[0].1, NO_CHUNK, "a hulk is left");
    assert_eq!(sim.stats(doll.idx()).kills, 1);
}

#[test]
fn blows_on_a_doomed_suit_cut_its_doom_short() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let a = suit(&mut sim, FrameId::Leo, Faction::Oz, PilotKind::Human, Vec3::new(0.0, 1_000.0, 400.0));
    breach(&mut sim, leo, a);
    idle(&mut sim, &[leo, a], 1);
    let left = sim.suits.doom[leo.idx()].left;
    // A blow of a sixth of its torso: a second off.
    let torso = frame(FrameId::Leo).part_hp[Part::Torso as usize];
    let armour = frame(FrameId::Leo).armor;
    sim.strike(leo.idx(), Part::Torso, torso / 6.0 / armour, a.idx(), WeaponKind::MachineCannon);
    idle(&mut sim, &[leo, a], 1);
    let cut = (DOOM_PER_TORSO / 6.0) as u16;
    let now = sim.suits.doom[leo.idx()].left;
    assert!(now + 1 + cut >= left && now + cut <= left, "{left} → {now}, a cut of about {cut}");
    // Blows enough end it at once, credited to the one who breached it.
    let from = sim.events.next_seq();
    sim.strike(leo.idx(), Part::Torso, torso * 2.0, a.idx(), WeaponKind::BeamCannon);
    idle(&mut sim, &[leo, a], 1);
    assert!(!sim.is_alive(leo.idx()));
    assert_eq!(kills_of(&events_since(&sim, from), leo)[0].0 as usize, a.idx());
}

#[test]
fn a_doll_and_a_sleeper_have_nobody_to_save() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let doll =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, Vec3::new(0.0, 1_000.0, 500.0));
    let sleeper = suit(&mut sim, FrameId::Leo, Faction::Oz, PilotKind::Human, Vec3::new(300.0, 1_000.0, 0.0));
    assert!(sim.sleep(sleeper));
    let from = sim.events.next_seq();
    breach(&mut sim, doll, leo);
    breach(&mut sim, sleeper, leo);
    idle(&mut sim, &[leo], 1);
    assert!(!sim.is_alive(doll.idx()) && !sim.is_alive(sleeper.idx()), "no doom for either");
    assert!(!events_since(&sim, from).iter().any(|e| matches!(e, Event::Doomed { .. })));
}

#[test]
fn a_doomed_pilot_ejects_and_the_wreck_is_theirs() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let doll =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, Vec3::new(0.0, 1_000.0, 500.0));
    sim.suits.flight[leo.idx()].vel = Vec3::new(10.0, 0.0, 0.0);
    breach(&mut sim, leo, doll);
    idle(&mut sim, &[leo], 1);
    let from = sim.events.next_seq();
    let out = sim.eject(leo, false).expect("a doomed pilot ejects");
    let Ejected::Out { hulk, generation, torso } = out else { panic!("{out:?}") };
    assert!(!torso, "a doomed suit's torso is gone");
    assert!(!sim.is_alive(leo.idx()));
    assert!(sim.chunks.is_alive(hulk) && sim.chunks.generation[usize::from(hulk)] == generation);
    let ev = events_since(&sim, from);
    let pod = ev.iter().find_map(|e| match *e {
        Event::Eject { suit, pos, vel, .. } if suit as usize == leo.idx() => Some((pos, vel)),
        _ => None,
    });
    let (pos, vel) = pod.expect("the capsule is thrown clear");
    let f = sim.suits.flight[leo.idx()];
    assert!((pos - f.pos).length() < 15.0, "from the cockpit");
    assert!(((vel - Vec3::new(10.0, 0.0, 0.0)).length() - EJECT_SPEED).abs() < 0.01, "{vel}");
    assert_eq!(kills_of(&ev, leo), vec![(doll.idx() as u16, hulk)], "credited to the Doll, a hulk left");
    // The tugs take it.
    let desc = sim.tow(hulk, generation).expect("there to take");
    assert!(matches!(desc.kind, ChunkKind::Hulk { frame: FrameId::Leo, .. }));
    assert!(!sim.chunks.is_alive(hulk));
    assert_eq!(sim.tow(hulk, generation), None, "once");
}

#[test]
fn ejecting_from_a_whole_suit_leaves_its_torso_whole_and_nobody_credited() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Agent, Vec3::new(0.0, 1_000.0, 0.0));
    idle(&mut sim, &[leo], 1);
    let from = sim.events.next_seq();
    let Some(Ejected::Out { hulk, torso, .. }) = sim.eject(leo, false) else { panic!("ejects") };
    assert!(torso);
    assert_eq!(kills_of(&events_since(&sim, from), leo), vec![(NO_SLOT, hulk)]);
    assert_eq!(sim.eject(leo, false), None, "nobody left aboard");
}

#[test]
fn a_held_wreck_is_not_the_tugs_to_take() {
    let mut sim = empty();
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 0.0));
    let Some(Ejected::Out { hulk, generation, .. }) = sim.eject(leo, false) else { panic!("ejects") };
    // Someone has it in hand.
    let other =
        suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 1_000.0, 30.0));
    let rot = sim.chunk_pose(usize::from(hulk)).1;
    sim.chunks.set_motion(
        usize::from(hulk),
        Motion::Held { holder: other.idx() as u16, right: false, rot, since: 1 },
    );
    assert_eq!(sim.tow(hulk, generation), None);
    assert!(sim.chunks.is_alive(hulk));
}

#[test]
fn a_doomed_suit_blown_up_hurts_its_enemies_and_leaves_nothing() {
    let mut sim =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() });
    let at = Vec3::new(0.0, 1_000.0, 0.0);
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, at);
    let close =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, at + Vec3::new(0.0, 0.0, 20.0));
    let edge =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, at + Vec3::new(0.0, 0.0, 55.0));
    let far =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, at + Vec3::new(0.0, 0.0, 300.0));
    let friend =
        suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, at + Vec3::new(0.0, 0.0, -20.0));
    idle(&mut sim, &[leo, friend], 1);
    // Not doomed: no self-destruct.
    assert_eq!(sim.eject(leo, true), None);
    breach(&mut sim, leo, far);
    idle(&mut sim, &[leo, friend], 1);
    let torso = |sim: &Sim, id: SuitId| sim.suits.part_hp[id.idx()][Part::Torso as usize];
    let before = [close, edge, far, friend].map(|id| torso(&sim, id));
    let from = sim.events.next_seq();
    assert_eq!(sim.eject(leo, true), Some(Ejected::Blown));
    idle(&mut sim, &[friend], 1);
    let ev = events_since(&sim, from);
    assert!(ev.iter().any(|e| matches!(e, Event::Blast { suit, .. } if *suit as usize == leo.idx())));
    assert_eq!(
        kills_of(&ev, leo),
        vec![(far.idx() as u16, NO_CHUNK)],
        "nothing left, credited to the breacher"
    );
    let lost = |k: usize, id: SuitId| before[k] - torso(&sim, id);
    // The Doll beside it is downed (the blast outweighs a Taurus's torso), the one at the edge
    // only scorched, the far one and the friend untouched.
    assert!(!sim.is_alive(close.idx()), "the close Doll is downed");
    assert!(lost(1, edge) > 0.0 && sim.is_alive(edge.idx()), "the edge Doll is hurt: {}", lost(1, edge));
    assert_eq!(lost(2, far), 0.0);
    assert_eq!(lost(3, friend), 0.0);
    let reactor = ev.iter().any(|e| matches!(e, Event::Hit { weapon: WeaponKind::Reactor, .. }));
    assert!(reactor, "its blows are the reactor's");
    // Its pilot is paid for the Doll it took with it.
    assert_eq!(sim.stats(leo.idx()).kills, 1);
    assert!(sim.suits.credits[leo.idx()] > 0);
    // And nobody sees anything of it.
    assert!(!sim.visible_to(friend.idx(), leo.idx()));
    assert!(weapon(WeaponKind::Reactor).range > 55.0 - frame(FrameId::Taurus).radius);
}

#[test]
fn nobody_ejects_inside_the_colony_nor_docks_doomed() {
    let mut sim = Sim::new(SimConfig {
        target_dolls: 0,
        field_rocks: 0,
        landmarks: 0,
        world: WorldKind::Interior,
        survival: true,
        ..SimConfig::default()
    });
    let id =
        sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Leo)).unwrap();
    assert_eq!(sim.eject(id, false), None, "inside the colony nothing strikes a suit");

    let mut sim =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() });
    let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, PilotKind::Human, DOCK_CENTER);
    let doll =
        suit(&mut sim, FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, Vec3::new(0.0, 1_000.0, 0.0));
    idle(&mut sim, &[leo], 2);
    assert!(sim.docked(leo.idx()), "at rest in the dock");
    breach(&mut sim, leo, doll);
    idle(&mut sim, &[leo], 1);
    assert!(sim.doomed(leo.idx()));
    assert!(sim.dock(leo).is_none(), "a doomed suit's reactor is going: no docking it");
}
