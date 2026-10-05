//! Suits inside the colony (`colony::interior`, `docs/SUITS_INSIDE.md`): they launch from the
//! inner gate, hold where they are against the spin's pull on flight assist, fall to the floor
//! (or a roof) and come to rest on it without one, are stopped by the city's buildings, fire
//! nothing (the colony's law), and dock back at the inner gate.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRIP, MELEE, SPECIAL};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::bodies::{Body, STANCE};
use bc_sim::colony::city::{Stage, place, place_door, room, solid_built};
use bc_sim::colony::course::Class;
use bc_sim::colony::frame::{CityPos, Under, from_colony};
use bc_sim::colony::hall::{self, DRILL, DRILL_PAR_S, Drill, DrillEvent, GANTRY, hall};
use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS, WorldKind, probe};
use bc_sim::content::city::{PLACES, PROVING_GROUND};
use bc_sim::ground::Footing;
use bc_sim::sim::{LaunchAt, Loadout};
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

/// One of the Charter Board's trainers, boarded at the Blast Hall's gantry.
fn trainer(sim: &mut Sim) -> SuitId {
    let leo = FrameId::Leo;
    sim.launch_at(leo, Faction::Colonies, PilotKind::Human, &Loadout::full(leo), LaunchAt::Gantry).unwrap()
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

/// A command flying `id` toward `to` on flight assist, no faster than `top` m/s (as `bc-server`'s
/// inside test flies).
fn toward(sim: &mut Sim, id: SuitId, to: Vec3, top: f32, buttons: u16) {
    let f = sim.suits.flight[id.idx()];
    let d = to - f.pos;
    let want = d.normalize_or_zero() * (d.length() * 0.3).min(top);
    let local = f.rot.conjugate() * (want - f.vel);
    let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim: d.normalize_or(Vec3::X),
            thrust: [q(local.x), q(local.y), q(local.z)],
            buttons: FLIGHT_ASSIST | buttons,
            ..InputCmd::default()
        },
    );
}

#[test]
fn a_suit_flies_in_through_the_blast_halls_doors_and_lands_on_its_floor() {
    // The Blast Hall (the Proving Ground, `docs/TRAINING.md`): from over Hub Gate's square, 60 m
    // out from its blast doors and 20 m up, straight in on flight assist to the middle of its room,
    // and down onto its floor with the grip armed. Never in its walls on the way.
    let (i, p) = place("proving_ground").expect("the Proving Ground");
    let room = room(i).expect("its room");
    let ((s, x), (ds, dx)) = place_door(p);
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    let k = id.idx();
    sim.suits.flight[k].pos = CityPos::new(p.strip, x - dx * 60.0, s - ds * 60.0, 20.0).to_colony();
    sim.suits.flight[k].vel = Vec3::ZERO;
    let (ms, mx) = room.rect.middle();
    let middle = CityPos::new(p.strip, mx, ms, 20.0).to_colony();
    let clear = |sim: &Sim| {
        let Under::Land(at) = from_colony(sim.suits.flight[k].pos) else { return false };
        let r = 0.7 * 10.0;
        let min = Vec3::new(at.x - r, (at.h - r).max(0.5), -(at.s + r));
        let max = Vec3::new(at.x + r, at.h + r, -(at.s - r));
        !solid_built(at.strip, min, max, Stage(0))
    };
    for _ in 0..30 * 30 {
        let f = sim.suits.flight[k];
        if f.pos.distance(middle) < 3.0 && f.vel.length() < 1.0 {
            break;
        }
        toward(&mut sim, id, middle, 40.0, 0);
        sim.step();
        assert!(clear(&sim), "in a wall at {}", sim.suits.flight[k].pos);
    }
    let in_room = |sim: &Sim| match from_colony(sim.suits.flight[k].pos) {
        Under::Land(at) => room.holds(at.s, at.x, at.h.min(room.ceiling - 1.0)) && at.strip == p.strip,
        Under::Window { .. } => false,
    };
    assert!(sim.suits.flight[k].pos.distance(middle) < 3.0, "in: {}", sim.suits.flight[k].pos);
    assert!(in_room(&sim));
    // The grip armed: down onto the hall's floor, on its feet.
    for _ in 0..30 * 20 {
        if sim.suits.footing[k] == Footing::Grounded {
            break;
        }
        toward(&mut sim, id, middle, 0.0, GRIP);
        sim.step();
    }
    assert_eq!(sim.suits.footing[k], Footing::Grounded);
    assert!(in_room(&sim), "standing in the hall: {}", sim.suits.flight[k].pos);
}

/// The middle of the Blast Hall's room, `h` up, in the colony's own frame.
fn in_the_hall(h: f32) -> Vec3 {
    let r = hall();
    let (s, x) = r.rect.middle();
    CityPos::new(r.strip, x, s, h).to_colony()
}

/// The way from `id`'s rifle's muzzle to `at`: where a pilot's crosshair on `at` aims it.
fn from_the_muzzle(sim: &Sim, id: SuitId, at: Vec3) -> Vec3 {
    from_the_muzzle_of(sim, id, 0, at)
}

/// The way from the muzzle of `id`'s weapon in loadout slot `slot` to `at`.
fn from_the_muzzle_of(sim: &Sim, id: SuitId, slot: usize, at: Vec3) -> Vec3 {
    let f = &sim.suits.flight[id.idx()];
    let muzzle = bc_sim::content::frame(FrameId::Leo).loadout[slot].map_or(Vec3::ZERO, |m| m.arm.muzzle());
    (at - (f.pos + f.rot * muzzle)).normalize()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// Puts `id` at `at`, at rest, and holds it there on flight assist aiming at `aim` for `ticks`,
/// pulling `buttons` for a tick in every three (a rifle's tap fires; held, it charges), checking
/// each tick with `each`.
fn hold_at(
    sim: &mut Sim,
    id: SuitId,
    at: Vec3,
    aim: Vec3,
    buttons: u16,
    ticks: u32,
    mut each: impl FnMut(&Sim),
) {
    let f = &mut sim.suits.flight[id.idx()];
    f.pos = at;
    f.vel = Vec3::ZERO;
    for n in 0..ticks {
        let to = from_the_muzzle(sim, id, aim);
        let pull = if n % 3 == 0 { buttons } else { 0 };
        hold(sim, id, FLIGHT_ASSIST | pull, [0; 3], Some(to));
        sim.step();
        each(sim);
    }
}

#[test]
fn in_the_blast_hall_weapons_are_free_and_its_rounds_score_on_its_targets() {
    // The colony's law's one exception (`docs/TRAINING.md`): a Leo in the hall turns to a target
    // on a stand by the back wall and fires its rifle. Its rounds score on the target, every client
    // near is told, and the shooter counts them.
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    let (k, t0) = (0, sim.tick());
    let aim = hall::target(k, t0, 0.0);
    // Turned to it first, then the trigger held.
    hold_at(&mut sim, id, in_the_hall(20.0), aim, 0, 60, |_| {});
    let from = sim.events.next_seq();
    let at = sim.suits.flight[id.idx()].pos;
    hold_at(&mut sim, id, at, aim, FIRE_PRIMARY, 30 * 4, |_| {});
    let events = events_since(&sim, from);
    let fired = events.iter().filter(|e| matches!(e, Event::BeamSpawn { .. })).count();
    let scored = events
        .iter()
        .filter(|e| matches!(e, Event::TargetHit { target, shooter, .. } if usize::from(*target) == k && *shooter == id.idx() as u16))
        .count();
    assert!(fired >= 3, "it fired: {fired}");
    assert!(scored * 2 >= fired, "most of {fired} scored: {scored}");
    assert_eq!(sim.stats(id.idx()).targets as usize, scored);
}

#[test]
fn training_rounds_touch_no_suit_and_never_leave_the_hall() {
    // A Leo fires at a target through another suit standing in the way, then out at the blast
    // doors: the suit in the way takes nothing, the rounds score beyond it, and nothing fired in
    // the hall is ever outside it.
    let mut sim = interior();
    let a = launch(&mut sim, FrameId::Leo);
    let b = launch(&mut sim, FrameId::Leo);
    let t0 = sim.tick();
    let aim = hall::target(1, t0, 0.0);
    let from_a = in_the_hall(20.0);
    // `b` halfway along the line of fire, held there.
    let between = from_a + (aim - from_a) * 0.5;
    sim.suits.flight[b.idx()].pos = between;
    let hp = sim.suits.part_hp[b.idx()];
    let from = sim.events.next_seq();
    let outside = |sim: &Sim| {
        for k in sim.projectiles.alive.iter() {
            assert!(
                hall::in_hall(sim.projectiles.pos[k]),
                "a round out of the hall at {}",
                sim.projectiles.pos[k]
            );
        }
    };
    hold_at(&mut sim, a, from_a, aim, 0, 60, |_| {});
    let at = sim.suits.flight[a.idx()].pos;
    let keep_b = |sim: &mut Sim| {
        let f = &mut sim.suits.flight[b.idx()];
        f.pos = between;
        f.vel = Vec3::ZERO;
    };
    for n in 0..30 * 3 {
        keep_b(&mut sim);
        let to = from_the_muzzle(&sim, a, aim);
        let pull = if n % 3 == 0 { FIRE_PRIMARY | FIRE_SECONDARY } else { FIRE_SECONDARY };
        hold(&mut sim, a, FLIGHT_ASSIST | pull, [0; 3], Some(to));
        sim.step();
        outside(&sim);
    }
    assert_eq!(sim.suits.part_hp[b.idx()], hp, "the suit in the way took nothing");
    let events = events_since(&sim, from);
    assert!(!events.iter().any(|e| matches!(e, Event::Hit { .. })), "no hits on suits");
    assert!(events.iter().any(|e| matches!(e, Event::TargetHit { target: 1, .. })), "scored beyond it");
    // Out at the blast doors, the square beyond them: stopped at the curtain across them.
    let p = &PLACES[PROVING_GROUND];
    let ((s, x), _) = place_door(p);
    let square = CityPos::new(p.strip, x, s - 60.0 * hall().inward.0, 20.0).to_colony();
    hold_at(&mut sim, a, at, square, 0, 60, |_| {});
    let at = sim.suits.flight[a.idx()].pos;
    let shots = sim.stats(a.idx()).shots;
    hold_at(&mut sim, a, at, square, FIRE_PRIMARY | FIRE_SECONDARY, 30 * 3, outside);
    assert!(sim.stats(a.idx()).shots > shots + 5, "it fired at the doors");
}

#[test]
fn out_through_the_blast_doors_weapons_are_safe_again() {
    // On the square before the doors, and over the city: the trigger does nothing.
    let mut sim = interior();
    let id = launch(&mut sim, FrameId::Leo);
    let p = &PLACES[PROVING_GROUND];
    let ((s, x), _) = place_door(p);
    let square = CityPos::new(p.strip, x, s - 30.0 * hall().inward.0, 20.0).to_colony();
    let from = sim.events.next_seq();
    hold_at(&mut sim, id, square, in_the_hall(20.0), FIRE_PRIMARY | FIRE_SECONDARY | MELEE, 30 * 3, |sim| {
        assert_eq!(sim.projectiles.count(), 0);
    });
    assert_eq!(sim.stats(id.idx()).shots, 0);
    assert!(!events_since(&sim, from).iter().any(|e| matches!(e, Event::BeamSpawn { .. })));
    // Nor is anything shown ready to fire there; in the hall, it is.
    assert_eq!(sim.own_state(id.idx()).weapon_ready, 0);
    hold_at(&mut sim, id, in_the_hall(20.0), in_the_hall(20.0) + Vec3::X * 50.0, 0, 30, |_| {});
    assert_ne!(sim.own_state(id.idx()).weapon_ready, 0);
}

#[test]
fn a_trainer_stands_on_the_gantry_till_its_pilot_is_heard_from_and_docks_only_there() {
    // There's no gantry in space.
    let mut space =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() });
    let leo = Loadout::full(FrameId::Leo);
    assert!(
        space.launch_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, &leo, LaunchAt::Gantry).is_none()
    );
    // In the colony, a trainer comes in on its feet on the gantry's pad, a stance over the floor,
    // facing in toward the targets.
    let mut sim = interior();
    let id = trainer(&mut sim);
    let i = id.idx();
    assert_eq!((sim.suits.footing[i], sim.suits.anchor[i].body), (Footing::Grounded, Body::City));
    assert!(sim.suits.trainer.get(i));
    let at = sim.suits.flight[i].pos;
    assert!(hall::in_hall(at) && hall::in_gantry(at, Vec3::ZERO), "{at}");
    assert!((probe(at).dist - STANCE).abs() < 0.05, "{} m up", probe(at).dist);
    assert!((sim.suits.flight[i].rot * Vec3::Z).dot(hall::gantry_facing()) > 0.999);
    // Ten seconds without a word from its pilot: it stands there, gripping, docked.
    for _ in 0..30 * 10 {
        sim.step();
    }
    assert_eq!(sim.suits.footing[i], Footing::Grounded);
    assert!(sim.suits.flight[i].pos.distance(at) < 0.01, "{}", sim.suits.flight[i].pos.distance(at));
    assert!(sim.docked(i));
    // A suit from the bays at rest on the gantry isn't docked there (it docks at the inner gate),
    // and a trainer at the inner gate isn't either.
    let other = launch(&mut sim, FrameId::Leo);
    let f = &mut sim.suits.flight[other.idx()];
    f.pos = at;
    f.vel = Vec3::ZERO;
    assert!(!sim.docked(other.idx()));
    assert!(sim.dock(other).is_none());
    let t = trainer(&mut sim);
    let f = &mut sim.suits.flight[t.idx()];
    (f.pos, f.vel) = (INNER_GATE, Vec3::ZERO);
    sim.suits.footing[t.idx()] = Footing::Free;
    assert!(!sim.docked(t.idx()));
    // The one on the gantry docks: gone from the sector.
    assert!(sim.dock(id).is_some());
    assert!(!sim.suits.used.get(i));
}

#[test]
fn a_trainer_flies_out_through_the_blast_doors_and_back_and_docks_on_its_gantry() {
    let mut sim = interior();
    let id = trainer(&mut sim);
    let k = id.idx();
    let r = hall();
    let p = |u: f32, v: f32, h: f32| {
        let (s, x) = r.front.point(u, v);
        CityPos::new(r.strip, x, s, h).to_colony()
    };
    let clear = |sim: &Sim| {
        let Under::Land(at) = from_colony(sim.suits.flight[k].pos) else { return false };
        let r = 0.7 * 10.0;
        let min = Vec3::new(at.x - r, (at.h - r).max(0.5), -(at.s + r));
        let max = Vec3::new(at.x + r, at.h + r, -(at.s - r));
        !solid_built(at.strip, min, max, Stage(0))
    };
    // Up off the pad (letting go of it), out through the middle of the blast doors to the square
    // beyond them, and back in over the pad: never in a wall.
    let way = [
        p(GANTRY.0, GANTRY.1, 22.0),
        p(0.0, 12.0, 22.0),
        p(0.0, -40.0, 22.0),
        p(0.0, 12.0, 22.0),
        p(GANTRY.0, GANTRY.1, 22.0),
    ];
    let mut out = false;
    for w in way {
        for _ in 0..30 * 30 {
            let f = sim.suits.flight[k];
            if f.pos.distance(w) < 2.0 && f.vel.length() < 1.0 {
                break;
            }
            toward(&mut sim, id, w, 25.0, 0);
            sim.step();
            assert!(clear(&sim), "in a wall at {:?}", from_colony(sim.suits.flight[k].pos));
            out |= !hall::in_hall(sim.suits.flight[k].pos);
        }
        assert!(sim.suits.flight[k].pos.distance(w) < 2.0, "{} m short", sim.suits.flight[k].pos.distance(w));
    }
    assert!(out, "out through the doors");
    // The grip armed, down onto the pad: on its feet, docked.
    for _ in 0..30 * 20 {
        if sim.suits.footing[k] == Footing::Grounded {
            break;
        }
        toward(&mut sim, id, p(GANTRY.0, GANTRY.1, 0.0), 0.0, GRIP);
        sim.step();
    }
    assert_eq!(sim.suits.footing[k], Footing::Grounded);
    for _ in 0..30 {
        hold(&mut sim, id, FLIGHT_ASSIST | GRIP, [0; 3], None);
        sim.step();
    }
    assert!(sim.docked(k));
    assert!(sim.dock(id).is_some());
}

#[test]
fn a_trainer_on_the_gantry_clears_the_drill() {
    // X-Wing's Maze in the hall (`docs/TRAINING.md`): the trainer stands on its pad and turns to
    // each lit target in turn, its machine cannon's trigger held, aiming where the target will be
    // when the rounds get there. The drill (fed its strikes as the board's sector feeds it) starts
    // on the first and is cleared on the last, the clock never near running out. With a machine's
    // aim it takes a few seconds, well within par: everything a pilot takes past that is aiming.
    let mut sim = interior();
    let id = trainer(&mut sim);
    let k = id.idx();
    let mut drill = Drill::default();
    let (mut seen, mut cleared, mut struck) = (sim.events.next_seq(), None, 0);
    let speed = bc_sim::content::weapon(bc_proto::WeaponKind::MachineCannon).speed;
    for _ in 0..30 * 120 {
        let t = sim.next_tick();
        let lit = usize::from(drill.lit());
        let from = sim.suits.flight[k].pos;
        let flight = hall::target(lit, t, 0.0).distance(from) / speed * bc_sim::TICK_HZ as f32;
        let at = hall::target(lit, t + flight.round() as u32, 0.0);
        let to = from_the_muzzle_of(&sim, id, 1, at);
        hold(&mut sim, id, FLIGHT_ASSIST | GRIP | FIRE_SECONDARY, [0; 3], Some(to));
        sim.step();
        for e in events_since(&sim, seen) {
            if let Event::TargetHit { tick, target, shooter, .. } = e
                && shooter == k as u16
            {
                match drill.strike(target, tick) {
                    Some(DrillEvent::Cleared(secs)) => cleared = Some(secs),
                    Some(DrillEvent::Out(n)) => panic!("the clock ran out at {n}"),
                    Some(_) => struck += 1,
                    None => {}
                }
            }
        }
        seen = sim.events.next_seq();
        assert_eq!(drill.tick(f64::from(sim.tick())), None);
        if cleared.is_some() {
            break;
        }
    }
    let secs = cleared.unwrap_or_else(|| panic!("cleared: {struck} of {} struck", DRILL.len()));
    assert_eq!(struck + 1, DRILL.len());
    assert_eq!(sim.suits.footing[k], Footing::Grounded, "it stood on the pad throughout");
    println!("the drill, by rote from the gantry: {secs:.1} s, {}", Class::against(secs, DRILL_PAR_S).name());
    assert!(secs < DRILL_PAR_S * 0.5, "{secs} s");
}
