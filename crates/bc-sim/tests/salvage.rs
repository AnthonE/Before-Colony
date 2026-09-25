//! Wreckage is real: limbs shot off drift away as chunks (and shots pass where they were),
//! destroyed suits leave hulks, chunks bounce off rocks and the colony, lost parts lighten a suit.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::events::Event;
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, NO_CHUNK, Part, PilotKind, Segment};
use bc_sim::chunks::{self, Motion, segment_pos};
use bc_sim::content::frame;
use bc_sim::content::salvage::{mass_without, part_mass_kg};
use bc_sim::math::look_rotation;
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS};
use bc_sim::{DT, Sim, SimConfig, SuitId};
use glam::{Quat, Vec3};

fn sim(rocks: bool) -> Sim {
    let field_rocks = if rocks { SimConfig::default().field_rocks } else { 0 };
    Sim::new(SimConfig { target_dolls: 0, field_rocks, ..SimConfig::default() })
}

fn human(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

fn idle(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

/// Where part `part` of suit `id` is (its capsule's middle), in the world.
fn part_center(sim: &Sim, id: SuitId, part: Part) -> Vec3 {
    let f = &sim.suits.flight[id.idx()];
    let c = frame(sim.suits.frame[id.idx()]).capsules[part as usize];
    f.pos + f.rot * ((c.a + c.b) * 0.5)
}

/// A Leo 600 m off along `-dir` fires one rifle shot along `dir` through `at`. Returns the
/// events of the next 20 ticks.
fn shoot_through(sim: &mut Sim, at: Vec3, dir: Vec3) -> Vec<Event> {
    let rot = look_rotation(dir, Vec3::Y);
    let muzzle = rot * Vec3::new(3.4, 0.6, 3.0);
    let pos = at - dir * 600.0 - muzzle;
    let leo = sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, rot).unwrap();
    let from = sim.events.next_seq();
    idle(sim, leo, FIRE_PRIMARY, dir);
    sim.step();
    for _ in 0..20 {
        idle(sim, leo, 0, dir);
        sim.step();
    }
    events_since(sim, from)
}

#[test]
fn a_limb_shot_off_drifts_away_as_a_chunk() {
    let mut sim = sim(false);
    let taurus = human(&mut sim, FrameId::Taurus, Faction::Oz, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    sim.suits.part_hp[taurus.idx()][Part::ArmR as usize] = 1.0;
    let arm = part_center(&sim, taurus, Part::ArmR);
    let ev = shoot_through(&mut sim, arm, -Vec3::Z);
    let chunk = ev
        .iter()
        .find_map(|e| match *e {
            Event::Detach { source, from_hulk: false, part: Part::ArmR, chunk, .. }
                if source as usize == taurus.idx() =>
            {
                Some(chunk)
            }
            _ => None,
        })
        .expect("the arm came off");
    assert_ne!(chunk, NO_CHUNK);
    let k = chunk as usize;
    assert!(sim.chunks.alive.get(k));
    assert_eq!(
        sim.chunks.desc[k].kind,
        ChunkKind::Limb { frame: FrameId::Taurus, faction: Faction::Oz, part: Part::ArmR }
    );
    assert_eq!(sim.chunks.desc[k].mass_kg, part_mass_kg(FrameId::Taurus, Part::ArmR));
    let Motion::Free(seg) = sim.chunks.motion[k] else { panic!("held?") };
    // It flies off outward (+X, the right side) and along the shot, and started where the arm was.
    assert!(seg.vel.x > 0.5 && seg.vel.z < -1.0, "{:?}", seg.vel);
    assert!(seg.pos.distance(arm) < 0.1);
    let t = f64::from(sim.tick());
    assert!(segment_pos(&seg, t).distance(arm) > 1.0);
    assert_eq!(sim.suits.gone_mask(taurus.idx()), 1 << Part::ArmR as u8);
    // The suit is lighter by the arm.
    assert_eq!(
        sim.own_state(taurus.idx()).extra_mass_kg,
        -(part_mass_kg(FrameId::Taurus, Part::ArmR) as i32)
    );
}

#[test]
fn shots_pass_where_a_limb_was() {
    for gone in [false, true] {
        let mut sim = sim(false);
        let taurus = human(&mut sim, FrameId::Taurus, Faction::Oz, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
        let arm = part_center(&sim, taurus, Part::ArmR);
        if gone {
            sim.suits.part_hp[taurus.idx()][Part::ArmR as usize] = 0.0;
        }
        // Head-on through the arm: nothing else of the suit is on this line.
        let ev = shoot_through(&mut sim, arm, -Vec3::Z);
        let hits: Vec<Part> = ev
            .iter()
            .filter_map(|e| match *e {
                Event::Hit { target, part, .. } if target as usize == taurus.idx() => Some(part),
                _ => None,
            })
            .collect();
        if gone {
            assert!(hits.is_empty(), "hit {hits:?} through a missing arm");
        } else {
            assert_eq!(hits, [Part::ArmR]);
        }
    }
}

#[test]
fn a_destroyed_suit_leaves_a_hulk() {
    let mut sim = sim(false);
    let taurus = human(&mut sim, FrameId::Taurus, Faction::Oz, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    // Drifting along the line of fire, so the shot still finds it.
    sim.suits.flight[taurus.idx()].vel = Vec3::new(0.0, 0.0, 20.0);
    sim.suits.part_hp[taurus.idx()][Part::ArmL as usize] = 0.0;
    sim.suits.part_hp[taurus.idx()][Part::Torso as usize] = 1.0;
    let torso = part_center(&sim, taurus, Part::Torso);
    let ev = shoot_through(&mut sim, torso, -Vec3::Z);
    let hulk = ev
        .iter()
        .find_map(|e| match *e {
            Event::Kill { victim, hulk, .. } if victim as usize == taurus.idx() => Some(hulk),
            _ => None,
        })
        .expect("killed");
    let k = hulk as usize;
    assert!(sim.chunks.alive.get(k), "no hulk");
    let all: u8 = (1 << Part::COUNT) - 1;
    let lost = 1 << Part::ArmL as u8;
    assert_eq!(
        sim.chunks.desc[k].kind,
        ChunkKind::Hulk { frame: FrameId::Taurus, faction: Faction::Oz, parts: all & !lost }
    );
    assert_eq!(sim.chunks.desc[k].mass_kg, mass_without(FrameId::Taurus, lost));
    let Motion::Free(seg) = sim.chunks.motion[k] else { panic!("held?") };
    // It carries on as the suit was going (within half a step of the wire's 0.25 m/s grid).
    assert!((seg.vel - Vec3::new(0.0, 0.0, 20.0)).abs().max_element() < 0.13, "{:?}", seg.vel);
}

fn free_ore(sim: &mut Sim, pos: Vec3, vel: Vec3) -> usize {
    let t = sim.tick();
    let desc = ChunkDesc { kind: ChunkKind::Ore { ore: 1 }, seed: 3, mass_kg: 800 };
    let seg = Segment { t0: t, pos, vel, rot: Quat::IDENTITY, spin: Vec3::new(0.0, 0.3, 0.0) }.quantized();
    sim.chunks.spawn(desc, Motion::Free(seg), t + 10_000, t).unwrap() as usize
}

fn step_idle(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.step();
    }
}

fn free_seg(sim: &Sim, k: usize) -> Segment {
    match sim.chunks.motion[k] {
        Motion::Free(s) => s,
        Motion::Held { .. } => panic!("held"),
    }
}

#[test]
fn chunks_bounce_off_rocks() {
    let mut sim = sim(true);
    let r = chunks::radius(&ChunkDesc { kind: ChunkKind::Ore { ore: 1 }, seed: 3, mass_kg: 800 });
    // The biggest rock with nothing else on the way in along +X.
    let mut rocks: Vec<_> = sim.field.rocks().to_vec();
    rocks.sort_by(|a, b| b.radius.total_cmp(&a.radius));
    let rock = *rocks
        .iter()
        .find(|k| {
            let from = k.pos - Vec3::X * (k.radius + 200.0);
            sim.field.sweep(from, k.pos, r).is_some_and(|(_, i)| sim.field.rocks()[i] == **k)
        })
        .unwrap();
    let start = rock.pos - Vec3::X * (rock.radius + 80.0);
    let k = free_ore(&mut sim, start, Vec3::X * 60.0);
    let first = free_seg(&sim, k);
    let mut bounced = None;
    for _ in 0..150 {
        sim.step();
        let seg = free_seg(&sim, k);
        let p = segment_pos(&seg, f64::from(sim.tick()));
        assert!(!rock.touches(p, r - 0.05), "inside the rock at {p:?}");
        if bounced.is_none() && seg.t0 != first.t0 {
            bounced = Some(seg);
        }
    }
    let seg = bounced.expect("never bounced");
    // The speed into the rock comes back at 0.4 of what it was (along the normal there).
    let n = rock.normal(seg.pos, r);
    let before = Vec3::X * 60.0;
    let (vn_in, vn_out) = (before.dot(n), seg.vel.dot(n));
    assert!(vn_in < 0.0 && (vn_out + 0.4 * vn_in).abs() < 0.5, "in {vn_in}, out {vn_out}");
}

#[test]
fn chunks_bounce_off_the_colony() {
    let mut sim = sim(false);
    let above = COLONY_CENTER + Vec3::new(500.0, COLONY_RADIUS + 60.0, 0.0);
    let k = free_ore(&mut sim, above, Vec3::new(0.0, -30.0, 2.0));
    step_idle(&mut sim, 120);
    let seg = free_seg(&sim, k);
    assert!(seg.vel.y > 11.0 && seg.vel.y < 13.0, "{:?}", seg.vel);
    let p = segment_pos(&seg, f64::from(sim.tick()));
    assert!((p - COLONY_CENTER).truncate().y > COLONY_RADIUS);
}

#[test]
fn free_chunks_expire() {
    let mut sim = sim(false);
    let t = sim.tick();
    let desc = ChunkDesc { kind: ChunkKind::Ore { ore: 0 }, seed: 0, mass_kg: 50 };
    let seg = Segment { t0: t, pos: Vec3::new(0.0, 900.0, 0.0), ..Segment::default() };
    let k = sim.chunks.spawn(desc, Motion::Free(seg), t + 5, t).unwrap() as usize;
    step_idle(&mut sim, 4);
    assert!(sim.chunks.alive.get(k));
    step_idle(&mut sim, 1);
    assert!(!sim.chunks.alive.get(k));
}

#[test]
fn lost_parts_lighten_the_suit() {
    // Full forward thrust for one tick, flight assist off: a = F / m, with m less the lost legs.
    let accel = |lost: &[Part]| {
        let mut sim = sim(false);
        let id = human(&mut sim, FrameId::Leo, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
        for p in lost {
            sim.suits.part_hp[id.idx()][*p as usize] = 0.0;
        }
        let spec = frame(FrameId::Leo);
        let mods = sim.flight_mods(id.idx());
        let mass = spec.mass(sim.suits.flight[id.idx()].propellant) + mods.extra_mass_kg as f32;
        let t = sim.next_tick();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: Vec3::Z,
                thrust: [0, 0, 127],
                ..InputCmd::default()
            },
        );
        sim.step();
        let got = sim.suits.flight[id.idx()].vel.z / DT;
        (got, spec.main_thrust * mods.thrust / mass, mods.extra_mass_kg)
    };
    let (whole, want_whole, extra_whole) = accel(&[]);
    let (legless, want_legless, extra_legless) = accel(&[Part::Legs]);
    assert_eq!(extra_whole, 0);
    assert_eq!(extra_legless, -(part_mass_kg(FrameId::Leo, Part::Legs) as i32));
    assert!((whole / want_whole - 1.0).abs() < 1e-4, "{whole} vs {want_whole}");
    assert!((legless / want_legless - 1.0).abs() < 1e-4, "{legless} vs {want_legless}");
}
