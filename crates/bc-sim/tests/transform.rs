//! Wing Zero ↔ Neo-Bird: MODE held changes the suit's form over a second, with its weapons down and
//! thrust cut; the bird is faster in a straight line; letting go changes it back; ZERO stays
//! engaged through it; a bird that dies comes back as a Wing Zero.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, MODE, ZERO};
use bc_proto::events::Event;
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind};
use bc_sim::content::{SpecialKind, frame};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, thrust: [i8; 3]) {
    let t = sim.next_tick();
    let cmd =
        InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, thrust, buttons, ..InputCmd::default() };
    sim.set_input(id, cmd.quantized());
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);

fn change_ticks() -> u32 {
    let SpecialKind::Transform { ticks, .. } = frame(FrameId::WingZero).special else {
        panic!("it transforms")
    };
    u32::from(ticks)
}

#[test]
fn mode_changes_wing_zero_into_neo_bird_and_back() {
    let ticks = change_ticks();
    let mut sim = empty();
    let wz = suit(&mut sim, FrameId::WingZero, Faction::Colonies, AT, Vec3::Z);
    let watcher = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 900.0, -Vec3::Z);
    // Charging the Twin Buster Rifle, then MODE: the charge is lost, and nothing fires meanwhile.
    for _ in 0..10 {
        hold(&mut sim, wz, FIRE_PRIMARY, [0; 3]);
        sim.step();
    }
    assert!(sim.suits.weapons[wz.idx()][0].charge > 0);
    let from = sim.events.next_seq();
    for k in 0..ticks {
        hold(&mut sim, wz, MODE | FIRE_PRIMARY, [0; 3]);
        sim.step();
        let own = sim.own_state(wz.idx());
        assert_ne!(own.flags & own_flags::TRANSFORMING, 0, "tick {k}");
        assert_eq!(u32::from(own.special_timer), ticks - k);
        assert_eq!(own.frame, FrameId::WingZero);
        assert_eq!(sim.suits.weapons[wz.idx()][0].charge, 0);
        assert_ne!(sim.entity_state(wz.idx(), watcher.idx()).flags & ent_flags::SPECIAL, 0);
    }
    hold(&mut sim, wz, MODE, [0; 3]);
    sim.step();
    let own = sim.own_state(wz.idx());
    assert_eq!(own.frame, FrameId::WingZeroBird);
    assert_eq!(own.flags & own_flags::TRANSFORMING, 0);
    assert!(events_since(&sim, from).iter().all(|e| !matches!(e, Event::BeamSpawn { .. })));
    // Held, it stays a bird; let go, it changes back.
    for _ in 0..30 {
        hold(&mut sim, wz, MODE, [0; 3]);
        sim.step();
    }
    assert_eq!(sim.suits.frame[wz.idx()], FrameId::WingZeroBird);
    for _ in 0..=ticks {
        hold(&mut sim, wz, 0, [0; 3]);
        sim.step();
    }
    assert_eq!(sim.suits.frame[wz.idx()], FrameId::WingZero);
    assert_eq!(sim.stats(wz.idx()).specials, 2);
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// Speed gained over `ticks` of full forward thrust (flight assist off).
fn burn(sim: &mut Sim, id: SuitId, buttons: u16, ticks: u32) -> f32 {
    let before = sim.suits.flight[id.idx()].vel.length();
    for _ in 0..ticks {
        hold(sim, id, buttons, [0, 0, 127]);
        sim.step();
    }
    sim.suits.flight[id.idx()].vel.length() - before
}

#[test]
fn a_change_cuts_thrust_and_the_bird_is_faster() {
    let SpecialKind::Transform { thrust, .. } = frame(FrameId::WingZero).special else { panic!() };
    let ticks = change_ticks();
    // The same burn, changing form or not.
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::WingZero, Faction::Colonies, AT, Vec3::Z);
    let plain = burn(&mut sim, a, 0, ticks - 1);
    let mut sim = empty();
    let b = suit(&mut sim, FrameId::WingZero, Faction::Colonies, AT, Vec3::Z);
    let changing = burn(&mut sim, b, MODE, ticks - 1);
    assert!((changing / plain - thrust).abs() < 0.02, "{changing:.2} m/s changing, {plain:.2} m/s not");
    // With flight assist on and the stick forward, the bird cruises faster.
    let cruise = |f: FrameId| {
        let mut sim = empty();
        let id = suit(&mut sim, FrameId::WingZero, Faction::Colonies, AT, Vec3::Z);
        let mode = if f == FrameId::WingZeroBird { MODE } else { 0 };
        burn(&mut sim, id, mode | FLIGHT_ASSIST, 30 * 20);
        assert_eq!(sim.suits.frame[id.idx()], f);
        sim.suits.flight[id.idx()].vel.length()
    };
    let (wz, bird) = (cruise(FrameId::WingZero), cruise(FrameId::WingZeroBird));
    assert!(bird > wz * 1.5, "Neo-Bird cruises at {bird:.0} m/s, Wing Zero at {wz:.0} m/s");
}

#[test]
fn zero_stays_engaged_and_a_dead_bird_comes_back_as_wing_zero() {
    let ticks = change_ticks();
    let mut sim = empty();
    let wz = suit(&mut sim, FrameId::WingZero, Faction::Colonies, AT, Vec3::Z);
    for _ in 0..ticks + 10 {
        hold(&mut sim, wz, MODE | ZERO, [0; 3]);
        sim.step();
    }
    assert_eq!(sim.suits.frame[wz.idx()], FrameId::WingZeroBird);
    assert_eq!(sim.suits.zero[wz.idx()].mode, zero_mode::ACTIVE);
    // Shot down as a bird.
    let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::new(-3.4, -0.6, 150.0), -Vec3::Z);
    // (Its nose, which takes a shot from ahead, and its fuselage are all but gone.)
    sim.suits.part_hp[wz.idx()][Part::Head as usize] = 1.0;
    sim.suits.part_hp[wz.idx()][Part::Torso as usize] = 1.0;
    let mut dead = false;
    for _ in 0..60 {
        hold(&mut sim, wz, MODE | ZERO, [0; 3]);
        let t = sim.next_tick();
        let lf = sim.suits.flight[leo.idx()];
        let muzzle = lf.pos + lf.rot * frame(FrameId::Leo).loadout[0].unwrap().arm.muzzle();
        let aim = (sim.suits.flight[wz.idx()].pos - muzzle).normalize();
        sim.set_input(
            leo,
            InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons: FIRE_PRIMARY, ..InputCmd::default() },
        );
        sim.step();
        if !sim.suits.alive.get(wz.idx()) {
            dead = true;
            break;
        }
    }
    assert!(dead, "the bird survived");
    for _ in 0..(sim.cfg.respawn_secs * 30.0) as u32 + 5 {
        hold(&mut sim, wz, 0, [0; 3]);
        sim.step();
    }
    assert!(sim.suits.alive.get(wz.idx()));
    assert_eq!(sim.suits.frame[wz.idx()], FrameId::WingZero);
    assert!(!sim.transforming(wz.idx()));
}
