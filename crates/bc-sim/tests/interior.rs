//! Suits inside the colony (`colony::interior`, `docs/SUITS_INSIDE.md`): they launch from the
//! inner gate, hold where they are against the spin's pull on flight assist, fall to the floor
//! (or a roof) and come to rest on it without one, are stopped by the city's buildings, fire
//! nothing (the colony's law), and dock back at the inner gate.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, SPECIAL};
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::colony::city::{Stage, solid_built};
use bc_sim::colony::frame::{CityPos, Under, from_colony};
use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS, WorldKind};
use bc_sim::sim::Loadout;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn interior() -> Sim {
    Sim::new(SimConfig {
        target_dolls: 0,
        field_rocks: 0,
        landmarks: 0,
        survival: true,
        world: WorldKind::Interior,
        ..SimConfig::default()
    })
}

fn launch(sim: &mut Sim, frame: FrameId) -> SuitId {
    sim.launch(frame, Faction::Colonies, PilotKind::Human, &Loadout::full(frame)).unwrap()
}

/// `id` holds `buttons`, aiming where it faces (or at `aim`).
fn hold(sim: &mut Sim, id: SuitId, buttons: u16, thrust: [i8; 3], aim: Option<Vec3>) {
    let t = sim.next_tick();
    let f = &sim.suits.flight[id.idx()];
    let aim = aim.unwrap_or(f.rot * Vec3::Z);
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() },
    );
}

#[test]
fn a_suit_comes_in_at_the_inner_gate_and_holds_there_on_flight_assist() {
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    let i = id.idx();
    assert!(sim.suits.flight[i].pos.distance(INNER_GATE) < 60.0);
    // Flight assist brings it to rest, and holds it against the pull.
    for _ in 0..30 * 8 {
        hold(&mut sim, id, FLIGHT_ASSIST, [0; 3], None);
        sim.step();
    }
    let f = sim.suits.flight[i];
    assert!(f.vel.length() < 1.0, "at rest: {}", f.vel);
    let start = f.pos;
    for _ in 0..30 * 5 {
        hold(&mut sim, id, FLIGHT_ASSIST, [0; 3], None);
        sim.step();
    }
    assert!(
        sim.suits.flight[i].pos.distance(start) < 5.0,
        "held: {}",
        sim.suits.flight[i].pos.distance(start)
    );
    // At rest in the ring: it docks.
    assert!(sim.suits.flight[i].pos.distance(INNER_GATE) < INNER_GATE_RADIUS);
    assert!(sim.dock(id).is_some());
}

#[test]
fn a_suit_holds_at_the_inner_gate_till_its_pilot_is_heard_from() {
    // No command at all for ten seconds (a page still catching up after the launch): flight
    // assist, as the launch leaves it, brings it to rest by the gate rather than the pull taking
    // it down to the floor.
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    for _ in 0..30 * 10 {
        sim.step();
    }
    let f = sim.suits.flight[id.idx()];
    assert!(f.vel.length() < 1.0, "at rest: {}", f.vel);
    assert!(
        f.pos.distance(INNER_GATE) < INNER_GATE_RADIUS,
        "by the gate: {} m off",
        f.pos.distance(INNER_GATE)
    );
}

#[test]
fn weapons_are_safe_inside() {
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    for _ in 0..90 {
        hold(&mut sim, id, FLIGHT_ASSIST | FIRE_PRIMARY | FIRE_SECONDARY | MELEE | SPECIAL, [0; 3], None);
        sim.step();
    }
    assert_eq!(sim.stats(id.idx()).shots, 0);
    assert_eq!(sim.projectiles.count(), 0);
    assert_eq!(sim.suits.input[id.idx()].buttons & (FIRE_PRIMARY | MELEE), 0, "never reaches the tick");
}

#[test]
fn a_suit_without_assist_falls_and_comes_to_rest_on_the_floor_or_a_roof() {
    let mut sim = interior();
    for (k, strip) in [0u8, 1, 2].into_iter().enumerate() {
        let id = launch(&mut sim, FrameId::Leo);
        let i = id.idx();
        let start = CityPos::new(strip, -2_000.0 + k as f32 * 3_000.0, 1_700.0, 150.0);
        let f = &mut sim.suits.flight[i];
        f.pos = start.to_colony();
        f.vel = Vec3::ZERO;
        for _ in 0..30 * 40 {
            hold(&mut sim, id, 0, [0; 3], Some(Vec3::X));
            sim.step();
        }
        let f = sim.suits.flight[i];
        assert!(f.vel.length() < 0.5, "at rest: {}", f.vel);
        let Under::Land(at) = from_colony(f.pos) else { panic!("{}", f.pos) };
        assert!(at.h < 140.0, "it fell: {at:?}");
        // Not in anything: what's under it is solid, where it is isn't.
        let r = 0.8 * 10.0;
        let min = Vec3::new(at.x - r, at.h - r + 2.0, -(at.s + r));
        let max = Vec3::new(at.x + r, at.h + r, -(at.s - r));
        assert!(!solid_built(at.strip, min, max, Stage(0)), "{at:?}");
        let below = Vec3::new(0.0, 20.0, 0.0);
        assert!(solid_built(at.strip, min - below, max - below, Stage(0)), "on something: {at:?}");
    }
}

#[test]
fn buildings_stop_a_suit_flying_into_them() {
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    let i = id.idx();
    // A building's wall across the strip from a point low over a street: the first solid at 30 m.
    let h = 30.0;
    let hit = |x: f32, s: f32| {
        solid_built(
            0,
            Vec3::new(x - 1.0, h - 1.0, -(s + 1.0)),
            Vec3::new(x + 1.0, h + 1.0, -(s - 1.0)),
            Stage(0),
        )
    };
    // Down the middle of a block (not a cross street): somewhere along x with a wall within 600 m
    // across from a clear point.
    let (x, s0, wall) = (0..200)
        .map(|k| -8_000.0 + k as f32 * 7.0)
        .find_map(|x| {
            let s0 = (0..300).map(|j| 300.0 + j as f32 * 5.0).find(|s| !hit(x, *s))?;
            let wall = (0..600).map(|j| s0 + j as f32).find(|s| hit(x, *s))?;
            (wall - s0 > 40.0).then_some((x, s0, wall))
        })
        .expect("a building somewhere across");
    let start = CityPos::new(0, x, s0 + 12.0, h);
    sim.suits.flight[i].pos = start.to_colony();
    let across = (CityPos::new(0, x, wall, h).to_colony() - start.to_colony()).normalize();
    for _ in 0..30 * 10 {
        hold(&mut sim, id, FLIGHT_ASSIST, [0, 0, 127], Some(across));
        sim.step();
        let Under::Land(at) = from_colony(sim.suits.flight[i].pos) else { continue };
        let r = 0.7 * 10.0;
        let min = Vec3::new(at.x - r, (at.h - r).max(0.5), -(at.s + r));
        let max = Vec3::new(at.x + r, at.h + r, -(at.s - r));
        assert!(!solid_built(at.strip, min, max, Stage(0)), "inside something: {at:?}");
    }
    let Under::Land(end) = from_colony(sim.suits.flight[i].pos) else { panic!() };
    // Flown flat out at the wall for 10 s, it's against it (or has slid along or over it, never
    // through it).
    assert!(end.s < wall || end.h > h + 10.0 || (end.x - x).abs() > 10.0, "{end:?} past {wall}");
    assert!(end.s > start.s + 10.0, "it flew: {end:?}");
}
