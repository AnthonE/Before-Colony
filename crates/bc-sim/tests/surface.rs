//! Surfaces. The landmarks are solid, and one ordered test says what stops a shot: free and
//! sleeping suits fetch up against MO-II and Hermit as against rocks; chunks bounce off them
//! relative to their moving surfaces; and shots, missiles and flame meet whatever comes first along
//! their path, a suit, a rock, a landmark or the colony, so a suit skimming the hull is hit and one
//! behind it is not.
//!
//! And suits land on them: with its grip armed, a suit that comes in slow and close is caught and
//! set down on its feet; it walks, runs, crouches and hops, lifts off on its thrusters and lets go
//! with the surface's own velocity; a rider stays solid to everything but its own body; and a pilot
//! who leaves parks on any body, settling first if it was in the air. Every tick of those keeps the
//! surface invariants (`common::check_invariants`).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

mod common;

use bc_proto::buttons::{
    BOOST, BRAKE, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, GRIP, MELEE, MODE, THROW, ZERO,
};
use bc_proto::events::{BurstCause, Event};
use bc_proto::snapshot::{footing, zero_mode};
use bc_proto::{
    BodyRef, ChunkDesc, ChunkKind, EntityState, Faction, FrameId, InputCmd, MAX_DATAGRAM, NO_SLOT, OwnState,
    OwnSurface, Part, PilotKind, RiderOn, Segment, SnapshotHeader, SnapshotReader, SnapshotWriter,
    WeaponKind,
};
use bc_sim::bodies::{Bodies, Body, TRACE_EPS, landmark_pose};
use bc_sim::chunks::{self, Motion, segment_pos};
use bc_sim::content::frame;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::BOUNCE;
use bc_sim::field::{Field, Rock, SUIT_CLEARANCE};
use bc_sim::flight::{self, FlightState};
use bc_sim::ground::{
    Anchor, CATCH_RANGE, CATCH_SPEED, CROUCH_SPEED, CROUCH_STANCE, Footing, GRIP_ACCEL, JUMP_SPEED,
    LAND_SPEED_MAX, LEGLESS_TURN_RATE, RELEASE_RANGE, RELEASE_SPEED, RUN_SPEED, STANCE, TAKEOFF_SPEED,
    UNPARK_SPEED, derive, place,
};
use bc_sim::math::look_rotation;
use bc_sim::rocks::RockStates;
use bc_sim::sim::cover;
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS, colony_sweep};
use bc_sim::{DT, Sim, SimConfig, SuitId};
use common::{
    check_invariants, grippable_rock, landmark_probe, landmark_surface, on_hermit, on_mo_ii, standing_on,
};
use glam::{Quat, Vec3};

/// A sector with no rocks and no dolls, and `landmarks` of the landmarks.
fn sector(landmarks: u8) -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, landmarks, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// `id` holds `buttons`, aiming along `aim`, having seen the world as it was `rewind` ticks ago.
fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3, rewind: u32) {
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: (t - rewind) << 4, aim, buttons, ..InputCmd::default() },
    );
}

/// Hands off: no thrust, no flight assist, the aim straight ahead.
fn hands_off(sim: &mut Sim, id: SuitId) {
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    hold(sim, id, 0, aim, 0);
}

fn hits_on(sim: &Sim, from: u32, target: SuitId) -> usize {
    events_since(sim, from)
        .iter()
        .filter(|e| matches!(e, Event::Hit { target: j, .. } if *j as usize == target.idx()))
        .count()
}

/// A point `up` m over the colony's hull, `deg` degrees round it from the top.
fn over_hull(deg: f32, up: f32) -> Vec3 {
    let a = deg.to_radians();
    COLONY_CENTER + Vec3::new(0.0, a.cos(), a.sin()) * (COLONY_RADIUS + up)
}

/// A Leo at `from` fires one beam at `target`'s centre, having seen the world `rewind` ticks ago,
/// and the sim runs on until it's spent. The hits it landed, and the muzzle it left.
fn one_beam(sim: &mut Sim, leo: SuitId, target: SuitId, rewind: u32) -> (usize, Vec3) {
    // History to rewind into.
    for _ in 0..10 {
        hands_off(sim, leo);
        sim.step();
    }
    let from = sim.events.next_seq();
    let f = sim.suits.flight[leo.idx()];
    let muzzle = f.pos + f.rot * frame(FrameId::Leo).loadout[0].unwrap().arm.muzzle();
    let aim = (sim.suits.flight[target.idx()].pos - muzzle).normalize();
    hold(sim, leo, FIRE_PRIMARY, aim, rewind);
    sim.step();
    for _ in 0..60 {
        hold(sim, leo, 0, aim, rewind);
        sim.step();
    }
    assert!(
        events_since(sim, from).iter().any(|e| matches!(e, Event::BeamSpawn { .. })),
        "the rifle never fired"
    );
    assert!(sim.projectiles.alive.iter().all(|k| sim.projectiles.owner[k] as usize != leo.idx()));
    (hits_on(sim, from, target), muzzle)
}

#[test]
fn colony_no_longer_eats_shots_before_suits() {
    // A Taurus skimming the hull, 12.5 m over it (a suit keeps 12 m off), fired at from above and
    // behind: the path runs through it and on into the hull. The suit is first, so it's hit,
    // whether the shot flies live or is rewound (then the whole path is flown at once).
    for rewind in [0, 8] {
        let mut sim = sector(2);
        let target = suit(&mut sim, FrameId::Taurus, Faction::Oz, over_hull(0.0, 12.5), Vec3::Z);
        let above = over_hull(-4.0, 150.0);
        let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, above, over_hull(0.0, 12.5) - above);
        let (hits, muzzle) = one_beam(&mut sim, leo, target, rewind);
        let at = sim.suits.flight[target.idx()].pos;
        let on = muzzle + (at - muzzle) * 1.5;
        assert!(
            colony_sweep(muzzle, on, 0.6).is_some_and(|s| s > 0.6),
            "the path didn't go on into the hull"
        );
        assert_eq!(hits, 1, "rewound {rewind}: the hull stopped the shot before the suit");
    }

    // Two suits 6 degrees either side of the top, 20 and 12.5 m up: the straight line between them
    // dips 5 m into the hull. It shields the target, live or rewound (a rewind flies the whole
    // 670 m at once, and once went straight through the colony).
    for rewind in [0, 8] {
        let mut sim = sector(2);
        let (a, b) = (over_hull(-6.0, 20.0), over_hull(6.0, 12.5));
        let target = suit(&mut sim, FrameId::Taurus, Faction::Oz, b, a - b);
        let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, a, b - a);
        let (hits, muzzle) = one_beam(&mut sim, leo, target, rewind);
        let at = sim.suits.flight[target.idx()].pos;
        assert!(colony_sweep(muzzle, at, 0.6).is_some_and(|s| s < 0.9), "the hull isn't in the way");
        assert_eq!(hits, 0, "rewound {rewind}: the shot went through the colony");
    }

    // A missile flown blind straight down at the hull bursts where it touches it, not where it was
    // the tick before.
    let mut sim = sector(2);
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, over_hull(0.0, 300.0), -Vec3::Y);
    let from = sim.events.next_seq();
    for k in 0..300 {
        let t = sim.next_tick();
        let buttons = if k < 2 { FIRE_SECONDARY } else { 0 };
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim: -Vec3::Y,
            buttons,
            lock_target: NO_SLOT,
            ..InputCmd::default()
        };
        sim.set_input(ha, cmd);
        sim.step();
    }
    let bursts: Vec<Vec3> = events_since(&sim, from)
        .iter()
        .filter_map(|e| match e {
            Event::MissileBurst { pos, cause: BurstCause::Blocked, .. } => Some(*pos),
            _ => None,
        })
        .collect();
    assert!(!bursts.is_empty(), "no missile met the hull");
    for pos in bursts {
        let rel = pos - COLONY_CENTER;
        let off = (rel.y * rel.y + rel.z * rel.z).sqrt() - COLONY_RADIUS;
        assert!((off - 0.5).abs() < 0.05, "burst {off} m off the hull");
    }
}

/// A Leo `out` m off landmark `k`'s surface straight out from its origin along `dir` (its frame),
/// coasting straight at it at `speed` m/s relative to the surface there.
fn coasting_at(sim: &mut Sim, k: u8, dir: Vec3, out: f32, speed: f32) -> SuitId {
    let (p, n) = landmark_surface(sim, k, dir);
    let id = suit(sim, FrameId::Leo, Faction::Colonies, p + n * out, -n);
    let (pose, _) = landmark_probe(sim, k, p);
    sim.suits.flight[id.idx()].vel = pose.point_vel(p) - n * speed;
    id
}

#[test]
fn free_suits_stop_at_mo_ii_and_hermit() {
    // Pylon tops and sides, the fore module's face, the mast's tip, the Aft Well's floor; Hermit's
    // long and short ends, a flank, and a crater floor.
    let mo_ii = [Vec3::Y, Vec3::Z, Vec3::new(1.0, 0.3, 0.2), Vec3::X, -Vec3::X, Vec3::new(0.3, -1.0, 0.6)];
    let hermit = [Vec3::X, -Vec3::Y, Vec3::new(1.0, 1.0, 1.0), Vec3::Y, Vec3::new(-0.2, 0.4, -1.0)];
    let approaches: Vec<(u8, Vec3)> =
        mo_ii.iter().map(|&d| (0, d)).chain(hermit.iter().map(|&d| (1, d))).collect();
    for landmarks in [2, 0] {
        let mut sim = sector(landmarks);
        // Flying, and asleep: a pilot who left mid-approach drifts in and fetches up the same way.
        let mut suits = Vec::new();
        for (n, &(k, dir)) in approaches.iter().enumerate() {
            let asleep = n % 2 == 1;
            let speed = if asleep { 20.0 } else { 40.0 };
            let id = coasting_at(&mut sim, k, dir, 150.0, speed);
            if asleep {
                assert!(sim.sleep(id));
            }
            suits.push((k, dir, id, f32::INFINITY));
        }
        let mut deepest = f32::INFINITY;
        for _ in 0..300 {
            for &(_, _, id, _) in &suits {
                if !sim.is_sleeping(id.idx()) {
                    hands_off(&mut sim, id);
                }
            }
            sim.step();
            for (k, dir, id, nearest) in &mut suits {
                let (_, pr) = landmark_probe(&sim, *k, sim.suits.flight[id.idx()].pos);
                deepest = deepest.min(pr.dist);
                *nearest = nearest.min(pr.dist);
                if landmarks != 0 {
                    assert!(pr.dist >= SUIT_CLEARANCE - 0.05, "landmark {k} along {dir}: {} m in", pr.dist);
                }
            }
        }
        if landmarks == 0 {
            // Without them, every suit goes straight through where they'd be.
            assert!(deepest < 0.0, "nothing reached a landmark ({deepest} m)");
            continue;
        }
        // Each met it, and came to rest against it: nothing left of its speed into the surface,
        // relative to the surface (MO-II's spin may since have turned it out from under a suit a
        // little).
        for &(k, dir, id, nearest) in &suits {
            let f = &sim.suits.flight[id.idx()];
            let (pose, pr) = landmark_probe(&sim, k, f.pos);
            let vn = (f.vel - pose.point_vel(f.pos)).dot(pr.normal);
            assert!(
                nearest < SUIT_CLEARANCE + TRACE_EPS,
                "landmark {k} along {dir}: never nearer than {nearest}"
            );
            assert!(
                pr.dist < SUIT_CLEARANCE + 3.0 && vn >= -0.05,
                "landmark {k} along {dir}: {} m off, {vn} m/s into it",
                pr.dist
            );
        }
    }
}

/// An ore chunk of 800 kg (1.8 m) at `pos`, moving at `vel`.
fn ore(sim: &mut Sim, pos: Vec3, vel: Vec3) -> usize {
    let t = sim.tick();
    let seg = Segment { t0: t, pos, vel, rot: Quat::IDENTITY, spin: Vec3::ZERO }.quantized();
    let desc = ChunkDesc { kind: ChunkKind::Ore { ore: 8 }, seed: 1, mass_kg: 800 };
    usize::from(sim.chunks.spawn(desc, Motion::Free(seg), t + 100_000, t).unwrap())
}

fn free_segment(sim: &Sim, k: usize) -> Segment {
    let Motion::Free(seg) = sim.chunks.motion[k] else { panic!("chunk {k} was picked up") };
    seg
}

#[test]
fn chunks_bounce_off_landmarks_relative_to_their_surface() {
    let mut sim = sector(2);
    let r = chunks::radius(&ChunkDesc { kind: ChunkKind::Ore { ore: 8 }, seed: 1, mass_kg: 800 });
    // Thrown at MO-II and Hermit from all round, 12 m/s into the surface and 3 m/s across it,
    // relative to it.
    let mut thrown = Vec::new();
    for (n, dir) in [
        Vec3::Y,
        Vec3::Z,
        -Vec3::Z,
        Vec3::new(1.0, 0.3, 0.2),
        -Vec3::X,
        Vec3::new(-0.5, 1.0, 0.7),
        Vec3::new(0.2, -0.4, -1.0),
    ]
    .into_iter()
    .enumerate()
    {
        for k in [0u8, 1] {
            let (p, nrm) = landmark_surface(&sim, k, dir);
            let across = nrm.any_orthonormal_vector() * 3.0;
            let start = p + nrm * (40.0 + n as f32);
            let (pose, _) = landmark_probe(&sim, k, start);
            let c = ore(&mut sim, start, pose.point_vel(start) - nrm * 12.0 + across);
            thrown.push((k, c, free_segment(&sim, c), 0));
        }
    }
    // One at rest just off the face of MO-II's +Y pylon that its spin (and drift) is turning into
    // it; and one at rest just off Hermit, which doesn't move. (A chunk's velocity travels on a
    // grid with no zero: "at rest" is 0.125 m/s on each axis, so the one by Hermit sits where that
    // takes it away.)
    let mo = landmark_pose(&LANDMARKS[0], sim.tick(), 0.0);
    let face = mo.to_world(Vec3::new(0.0, 70.0, 24.0 + r + 0.3));
    let knocked = ore(&mut sim, face, Vec3::ZERO);
    let (p, nrm) = [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z]
        .into_iter()
        .map(|dir| landmark_surface(&sim, 1, dir))
        .max_by(|a, b| a.1.dot(Vec3::ONE).total_cmp(&b.1.dot(Vec3::ONE)))
        .unwrap();
    let resting = ore(&mut sim, p + nrm * (r + 0.5), Vec3::ZERO);
    thrown.push((0, knocked, free_segment(&sim, knocked), 0));
    let before = free_segment(&sim, resting);

    for _ in 0..240 {
        sim.step();
        let t = sim.tick();
        for (k, c, last, bounces) in &mut thrown {
            let seg = free_segment(&sim, *c);
            let (_, pr) = landmark_probe(&sim, *k, segment_pos(&seg, f64::from(t)));
            assert!(pr.dist >= r - 0.25, "chunk {c} is {} m into landmark {k}", r - pr.dist);
            if seg == *last {
                continue;
            }
            // It bounced this tick, where it touched: back out as fast as it went in, less the
            // bounce's loss, and as fast across, both relative to the surface there.
            assert_eq!(seg.t0, t);
            let (pose, pr) = landmark_probe(&sim, *k, seg.pos);
            assert!(pr.dist >= r - 0.05, "chunk {c} bounced {} m into landmark {k}", r - pr.dist);
            let v_s = pose.point_vel(seg.pos);
            let (v_in, v_out) = (last.vel - v_s, seg.vel - v_s);
            let (n_in, n_out) = (v_in.dot(pr.normal), v_out.dot(pr.normal));
            assert!(n_in < 0.0 && n_out > 0.0, "chunk {c}: {n_in} m/s in, {n_out} out");
            assert!((n_out + BOUNCE * n_in).abs() < 0.3, "chunk {c}: {n_in} m/s in, {n_out} out");
            let across = |v: Vec3, vn: f32| v - pr.normal * vn;
            assert!((across(v_out, n_out) - across(v_in, n_in)).length() < 0.3, "chunk {c} skidded");
            *last = seg;
            *bounces += 1;
        }
    }
    for &(k, c, _, bounces) in &thrown {
        assert!(bounces >= 1, "chunk {c} never met landmark {k}");
    }
    // The moving face knocked the resting chunk away faster than it moves; Hermit's never did.
    let seg = free_segment(&sim, knocked);
    let (pose, pr) = landmark_probe(&sim, 0, seg.pos);
    assert!(seg.vel.dot(pr.normal) > pose.point_vel(seg.pos).dot(pr.normal) + 0.5, "{}", seg.vel);
    assert_eq!(free_segment(&sim, resting), before);
}

#[test]
fn landmarks_shield_suits_from_shots_and_flame() {
    for landmarks in [2, 0] {
        // A beam across MO-II's core at a suit beyond it, live and rewound.
        for rewind in [0, 8] {
            let mut sim = sector(landmarks);
            let mo = landmark_pose(&LANDMARKS[0], sim.tick(), 0.0);
            let (a, b) =
                (mo.to_world(Vec3::new(20.0, 400.0, 0.0)), mo.to_world(Vec3::new(-20.0, -150.0, 0.0)));
            let target = suit(&mut sim, FrameId::Taurus, Faction::Oz, b, a - b);
            let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, a, b - a);
            let (hits, _) = one_beam(&mut sim, leo, target, rewind);
            assert_eq!(hits, usize::from(landmarks == 0), "{landmarks} landmarks, rewound {rewind}");
        }
        // Shenlong's flame at a Leo across the mast.
        let mut sim = sector(landmarks);
        let mo = landmark_pose(&LANDMARKS[0], sim.tick(), 0.0);
        let (a, b) = (mo.to_world(Vec3::new(295.0, 0.0, 25.0)), mo.to_world(Vec3::new(295.0, 0.0, -25.0)));
        let shenlong = suit(&mut sim, FrameId::Shenlong, Faction::Colonies, a, b - a);
        let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, b, a - b);
        let from = sim.events.next_seq();
        for _ in 0..30 {
            let aim = (sim.suits.flight[leo.idx()].pos - sim.suits.flight[shenlong.idx()].pos).normalize();
            hold(&mut sim, shenlong, FIRE_SECONDARY, aim, 0);
            sim.step();
        }
        let burns = events_since(&sim, from)
            .iter()
            .filter(|e| matches!(e, Event::Hit { weapon: WeaponKind::Flamethrower, .. }))
            .count();
        assert_eq!(burns > 0, landmarks == 0, "{landmarks} landmarks: {burns} burns");
    }
}

// -------------------------------------------------------------------------------------------------
// On the surface: catching, walking, hopping, letting go
// -------------------------------------------------------------------------------------------------

/// A sector with the default field and both landmarks, and no dolls.
fn grip_sector() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() })
}

fn bodies(sim: &Sim) -> Bodies<'_> {
    Bodies::at(&sim.field, sim.landmarks(), sim.tick())
}

/// `id`'s command for the next tick.
fn drive(sim: &mut Sim, id: SuitId, thrust: [i8; 3], buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() },
    );
}

/// A tick, then the surface invariants.
fn step(sim: &mut Sim) {
    sim.step();
    check_invariants(sim);
}

/// The ground under attached suit `id` as the step finds it: the surface's normal (sector frame)
/// and how high its feet are over it, m.
fn ground(sim: &Sim, id: SuitId) -> (Vec3, f32) {
    let a = sim.suits.anchor[id.idx()];
    let b = bodies(sim);
    let (_, n, h) = place(&b.shape(a.body).unwrap(), a.local, a.stance);
    (b.pose(a.body).unwrap().rot * n, h)
}

/// The outermost surface of `body` straight out from its origin along `dir` (its frame), now: the
/// point and the outward normal, in the sector's frame.
fn surface_of(sim: &Sim, body: Body, dir: Vec3) -> (Vec3, Vec3) {
    let b = bodies(sim);
    let pose = b.pose(body).unwrap();
    let (p, n) = b.surface_along(body, dir).unwrap();
    (pose.to_world(p), pose.rot * n)
}

/// A unit vector along the surface with normal `n`.
fn along(n: Vec3) -> Vec3 {
    (Vec3::Z - n * n.z).normalize_or(Vec3::X - n * n.x)
}

/// A Leo with its feet `up` m over `body`'s surface along `dir`, nose along the surface and body
/// rolled `roll` rad off upright, drifting toward the surface at `sink` m/s relative to it.
fn over(sim: &mut Sim, body: Body, dir: Vec3, up: f32, roll: f32, sink: f32) -> SuitId {
    over_in(sim, FrameId::Leo, body, dir, up, roll, sink)
}

/// [`over`], in `frame`.
fn over_in(sim: &mut Sim, frame: FrameId, body: Body, dir: Vec3, up: f32, roll: f32, sink: f32) -> SuitId {
    let (p, n) = surface_of(sim, body, dir);
    let pos = p + n * (STANCE + up);
    let fwd = along(n);
    let tilted = Quat::from_axis_angle(fwd, roll) * n;
    let id =
        sim.spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(fwd, tilted)).unwrap();
    let surface = bodies(sim).pose(body).unwrap().point_vel(pos);
    sim.suits.flight[id.idx()].vel = surface - n * sink;
    id
}

#[test]
fn grip_catches_a_slow_suit_and_lands_it_feet_first() {
    let mut sim = grip_sector();
    let (r, _) = grippable_rock(&sim, 300.0);
    for (body, dir) in [
        (Body::Rock(r as u16), Vec3::new(0.2, 1.0, 0.1)),
        (Body::Landmark(1), Vec3::new(0.3, 1.0, 0.2)),
        (Body::Landmark(0), Vec3::new(-1.5, 1.0, 0.0)),
    ] {
        // 20 m up, rolled 60° off, sinking at 2 m/s: armed, hands off.
        let id = over(&mut sim, body, dir, 20.0, 1.0, 2.0);
        let i = id.idx();
        let aim = sim.suits.flight[i].rot * Vec3::Z;
        let (mut caught, mut landed, mut fastest, mut upright) = (None, None, 0.0f32, None);
        for k in 0..600 {
            drive(&mut sim, id, [0; 3], GRIP, aim);
            step(&mut sim);
            let footing = sim.footing(i);
            if caught.is_none() && footing != Footing::Free {
                caught = Some(k);
                assert_eq!(sim.suits.anchor[i].body, body);
            }
            if footing == Footing::Aloft {
                let (n, _) = ground(&sim, id);
                let a = sim.suits.anchor[i];
                let pose = bodies(&sim).pose(body).unwrap();
                fastest = fastest.max(-(pose.rot * a.vel).dot(n));
            }
            if landed.is_none() && footing == Footing::Grounded {
                landed = Some(k);
            }
            if let Some(l) = landed {
                assert_eq!(footing, Footing::Grounded, "{body:?}: it stays down");
                let (n, h) = ground(&sim, id);
                assert!(h.abs() < 1e-2, "{body:?}: feet {h} m off the ground");
                if upright.is_none() && (sim.suits.flight[i].rot * Vec3::Y).dot(n) > 0.99 {
                    upright = Some(k - l);
                }
                if k > l + 60 {
                    break;
                }
            }
        }
        assert_eq!(caught, Some(0), "{body:?}: armed, slow and close, it's caught at once");
        assert!(landed.is_some(), "{body:?}: never landed");
        assert!(fastest <= LAND_SPEED_MAX + 1e-3, "{body:?}: came down at {fastest} m/s");
        assert!(upright.is_some_and(|k| k <= 60), "{body:?}: not on its feet a second after landing");
        sim.leave(id);
    }
}

/// A v8 suit's tick: `step_in` with the modifiers the sim uses (and nothing near a landmark).
fn v8_step(sim: &Sim, id: SuitId, f: &mut FlightState) {
    let i = id.idx();
    let mods = sim.flight_mods_at(i, sim.next_tick());
    flight::step_in(&sim.field, f, &sim.suits.input[i], frame(sim.suits.frame[i]), &mods, DT);
}

#[test]
fn without_grip_nothing_attaches_and_flight_is_v8() {
    let mut sim = grip_sector();
    let (_, rock) = grippable_rock(&sim, 300.0);
    // A miner holding 1 m off the rock's collider on flight assist, aiming at it; a suit ramming
    // it at 120 m/s; and one armed with the grip out in the open, weaving: none of them is ever
    // held, and each flies exactly as the flight model always has.
    let face = rock.surface(rock.pos + Vec3::Y * (rock.radius + 50.0), SUIT_CLEARANCE + 1.0);
    let miner = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, face, look_rotation(-Vec3::Y, Vec3::Z))
        .unwrap();
    let dir = Vec3::new(1.0, 0.1, 0.3).normalize();
    let rammer = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            rock.pos - dir * (rock.radius + 300.0),
            look_rotation(dir, Vec3::Y),
        )
        .unwrap();
    sim.suits.flight[rammer.idx()].vel = dir * 120.0;
    let open = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 5_000.0, 9_000.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let mut touched = false;
    for k in 0..900u32 {
        let f = sim.suits.flight[miner.idx()];
        let to_face = f.rot.inverse() * (face - f.pos);
        let q = |v: f32| (v * 127.0 / 220.0).clamp(-127.0, 127.0) as i8;
        let hold = [q(to_face.x * 0.5), q(to_face.y * 0.5), q(to_face.z * 0.5)];
        drive(&mut sim, miner, hold, FLIGHT_ASSIST, (rock.pos - f.pos).normalize());
        drive(&mut sim, rammer, [0; 3], 0, dir);
        let weave = [(k % 90) as i8 - 45, 60, (k % 50) as i8];
        drive(
            &mut sim,
            open,
            weave,
            GRIP | FLIGHT_ASSIST,
            Vec3::new(0.3, (k as f32 * 0.02).sin(), 1.0).normalize(),
        );
        let mut expected = [miner, rammer, open].map(|id| {
            let mut f = sim.suits.flight[id.idx()];
            v8_step(&sim, id, &mut f);
            f
        });
        step(&mut sim);
        for (id, f) in [miner, rammer, open].into_iter().zip(expected.iter_mut()) {
            assert_eq!(sim.footing(id.idx()), Footing::Free);
            assert_eq!(sim.suits.flight[id.idx()], *f, "suit {} left the v8 flight model at {k}", id.idx());
        }
        touched |= rock.touches(sim.suits.flight[rammer.idx()].pos, SUIT_CLEARANCE + 0.1);
    }
    assert!(touched, "the rammer never reached the rock");
    assert!(sim.suits.flight[miner.idx()].pos.distance(face) < 5.0, "the miner didn't hold its spot");
}

#[test]
fn fast_boosting_or_climbing_suits_are_not_caught() {
    let mut sim = grip_sector();
    let hermit = Body::Landmark(1);
    let dirs = [
        Vec3::new(0.3, 1.0, 0.2),
        Vec3::new(0.35, 1.0, 0.2),
        Vec3::new(0.4, 1.0, 0.2),
        Vec3::new(0.45, 1.0, 0.2),
        Vec3::new(0.5, 1.0, 0.2),
    ];
    let ids = dirs.map(|d| over(&mut sim, hermit, d, 15.0, 0.0, 0.0));
    let [fast, boosting, climbing, leaving, slow] = ids;
    let (_, n) = surface_of(&sim, hermit, dirs[1]);
    sim.suits.flight[fast.idx()].vel = along(n) * (CATCH_SPEED + 2.0);
    sim.suits.flight[leaving.idx()].vel = n * 3.0;
    for _ in 0..10 {
        for id in ids {
            let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
            let (thrust, buttons) = match id {
                _ if id == boosting => ([0; 3], GRIP | BOOST),
                _ if id == climbing => ([0, 40, 0], GRIP),
                _ => ([0; 3], GRIP),
            };
            drive(&mut sim, id, thrust, buttons, aim);
        }
        step(&mut sim);
        for id in [fast, boosting, climbing, leaving] {
            assert_eq!(sim.footing(id.idx()), Footing::Free, "suit {} was caught", id.idx());
        }
        assert_ne!(sim.footing(slow.idx()), Footing::Free, "the slow one is caught");
    }
}

#[test]
fn catch_and_release_have_hysteresis() {
    // Up past 40 m on the thrusters, then braked to a stop far up, armed: one hop, one release as
    // its feet pass 40 m, and never caught again.
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    let mut last_aloft = 0.0;
    let mut changes = Vec::new();
    for k in 0..240 {
        let (thrust, buttons) =
            if k < 110 { ([0, 127, 0], GRIP | FLIGHT_ASSIST) } else { ([0; 3], GRIP | BRAKE) };
        drive(&mut sim, id, thrust, buttons, aim);
        let was = sim.footing(i);
        step(&mut sim);
        let now = sim.footing(i);
        if now == Footing::Aloft {
            last_aloft = ground(&sim, id).1;
        }
        if now != was {
            changes.push(now);
        }
    }
    assert_eq!(changes, [Footing::Aloft, Footing::Free]);
    assert!((RELEASE_RANGE - 1.0..=RELEASE_RANGE).contains(&last_aloft), "let go at {last_aloft} m");
    let (_, pr) = landmark_probe(&sim, 1, sim.suits.flight[i].pos);
    assert!(pr.dist - STANCE > CATCH_RANGE + 20.0, "it ends {} m up", pr.dist - STANCE);

    // Too fast along the ground on raw thrust: lost, then caught again once braked slow enough.
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.5, 1.0, 0.2));
    let i = id.idx();
    let (n, _) = ground(&sim, id);
    let fwd = along(n);
    let (mut changes, mut fastest_held, mut caught_at) = (Vec::new(), 0.0f32, None);
    for k in 0..600 {
        let (thrust, buttons) = match k {
            0 => ([0, 127, 0], GRIP),
            1..30 => ([0, 0, 127], GRIP),
            _ => ([0; 3], GRIP | BRAKE),
        };
        drive(&mut sim, id, thrust, buttons, fwd);
        let was = sim.footing(i);
        let rel = {
            let f = sim.suits.flight[i];
            f.vel - bodies(&sim).pose(Body::Landmark(1)).unwrap().point_vel(f.pos)
        };
        step(&mut sim);
        let now = sim.footing(i);
        if now == Footing::Aloft {
            fastest_held = fastest_held.max(sim.suits.anchor[i].vel.length());
        }
        if now != was {
            changes.push(now);
            if was == Footing::Free {
                caught_at = Some(rel.length());
            }
        }
    }
    assert_eq!(changes, [Footing::Aloft, Footing::Free, Footing::Aloft, Footing::Grounded], "{changes:?}");
    assert!(fastest_held <= RELEASE_SPEED, "held at {fastest_held} m/s");
    assert!(caught_at.is_some_and(|v| v <= CATCH_SPEED), "caught again at {caught_at:?} m/s");
}

#[test]
fn roll_level_turns_the_feet_down_without_moving_the_nose() {
    let mut sim = grip_sector();
    let hermit = Body::Landmark(1);
    // 100 m up, rolled onto its side, nose along the surface; one armed, one not.
    let armed = over(&mut sim, hermit, Vec3::new(0.3, 1.0, 0.2), 100.0, 1.5, 0.0);
    let plain = over(&mut sim, hermit, Vec3::new(0.2, 1.0, 0.3), 100.0, 1.5, 0.0);
    let (_, n) = surface_of(&sim, hermit, Vec3::new(0.3, 1.0, 0.2));
    let (_, n_plain) = surface_of(&sim, hermit, Vec3::new(0.2, 1.0, 0.3));
    let aims = [armed, plain].map(|id| sim.suits.flight[id.idx()].rot * Vec3::Z);
    for _ in 0..120 {
        drive(&mut sim, armed, [0; 3], GRIP | FLIGHT_ASSIST, aims[0]);
        drive(&mut sim, plain, [0; 3], FLIGHT_ASSIST, aims[1]);
        step(&mut sim);
        let nose = sim.suits.flight[armed.idx()].rot * Vec3::Z;
        assert!(
            nose.angle_between(aims[0]) < 1e-3,
            "the nose left the aim by {}",
            nose.angle_between(aims[0])
        );
    }
    assert_eq!(sim.footing(armed.idx()), Footing::Free, "too high to be caught");
    let up = |id: SuitId| sim.suits.flight[id.idx()].rot * Vec3::Y;
    assert!(up(armed).dot(n) > 0.99, "armed, its feet turn to the surface: up·n {}", up(armed).dot(n));
    assert!(up(plain).dot(n_plain) < 0.1, "unarmed, it stays as it was");
}

#[test]
fn walks_around_a_rock_keeping_its_stance() {
    let mut sim = grip_sector();
    let (r, rock) = grippable_rock(&sim, 300.0);
    let id = standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Rock(r as u16), Vec3::Y);
    let i = id.idx();
    let mut walked = 0.0;
    let mut normals = Vec::new();
    for _ in 0..600 {
        let before = sim.suits.anchor[i].local;
        let aim = sim.suits.flight[i].rot * Vec3::Z;
        drive(&mut sim, id, [0, 0, 127], GRIP, aim);
        step(&mut sim);
        assert_eq!(sim.footing(i), Footing::Grounded);
        let (n, h) = ground(&sim, id);
        assert!(h.abs() < 1e-2, "feet {h} m off the rock");
        walked += sim.suits.anchor[i].local.distance(before);
        normals.push(n);
    }
    assert!(walked > 150.0, "walked {walked} m in 20 s");
    // All the way round: it stood on every side of the rock.
    let spread = normals.iter().map(|n| n.dot(normals[0])).fold(1.0f32, f32::min);
    assert!(spread < -0.9, "it never got round the {} m rock", rock.radius);
}

/// Aim along MO-II's frame direction `dir` (it spins about its own x, so the aim turns with it).
fn mo_ii_aim(sim: &Sim, dir: Vec3) -> Vec3 {
    bodies(sim).pose(Body::Landmark(0)).unwrap().rot * dir.normalize()
}

#[test]
fn inner_corners_block_rounded_edges_do_not_and_walls_slide() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    // On the core at x = -90, walking +x at the pylon standing on the core at -40..40: its foot is
    // an inner corner, so the suit stops short of it (no nearer than BODY_CLEAR to the wall).
    let id = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(-1.5, 1.0, 0.0));
    let i = id.idx();
    for _ in 0..300 {
        let aim = mo_ii_aim(&sim, Vec3::X);
        drive(&mut sim, id, [0, 0, 127], GRIP, aim);
        step(&mut sim);
        assert_eq!(sim.footing(i), Footing::Grounded);
    }
    let at = sim.suits.anchor[i].local;
    assert!((-50.0..-44.0).contains(&at.x), "stopped at x = {} by the pylon at -40", at.x);
    assert!(ground(&sim, id).0.dot(mo_ii_aim(&sim, Vec3::Y)) > 0.99, "still on the core's top");
    // Into the wall at 45°: it slides along it, keeping what isn't into the wall.
    for _ in 0..60 {
        let aim = mo_ii_aim(&sim, Vec3::new(1.0, 0.0, 1.0));
        drive(&mut sim, id, [0, 0, 127], GRIP, aim);
        step(&mut sim);
    }
    let slid = sim.suits.anchor[i].local;
    assert!(slid.z - at.z > 5.0, "slid {} m along the wall", slid.z - at.z);
    assert!(slid.x < -44.0, "and never into it: x = {}", slid.x);

    // On the pylon's top, walking -x: over its rounded edge and down its side, never stopped.
    let id = on_mo_ii(&mut sim, FrameId::Leo, Vec3::Y);
    let i = id.idx();
    let mut down_the_side = false;
    for _ in 0..240 {
        let aim = sim.suits.flight[i].rot * Vec3::Z;
        let aim = if sim.suits.anchor[i].local.x > -30.0 { mo_ii_aim(&sim, -Vec3::X) } else { aim };
        drive(&mut sim, id, [0, 0, 127], GRIP, aim);
        step(&mut sim);
        assert_eq!(sim.footing(i), Footing::Grounded);
        let (n, _) = ground(&sim, id);
        down_the_side |= n.dot(mo_ii_aim(&sim, -Vec3::X)) > 0.95;
    }
    assert!(down_the_side, "it never walked round the edge onto the pylon's side");
}

#[test]
fn running_off_a_ledge_lands_running() {
    // Running, a hop keeps the run: aloft, flight assist holds run speed along the surface, so the
    // suit comes down still running, and the grip never lets go.
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let (mut aloft, mut landed_at) = (0, None);
    for k in 0..400 {
        let jump = if k == 60 { 127 } else { 0 };
        let (n, _) = ground(&sim, id);
        let aim = sim.suits.flight[i].rot * Vec3::Z;
        let fwd = (aim - n * aim.dot(n)).normalize();
        drive(&mut sim, id, [0, jump, 127], GRIP | BOOST | FLIGHT_ASSIST, fwd);
        let was = sim.footing(i);
        step(&mut sim);
        assert_ne!(sim.footing(i), Footing::Free, "the grip let go at {k}");
        if sim.footing(i) == Footing::Aloft {
            aloft += 1;
        }
        if was == Footing::Aloft && sim.footing(i) == Footing::Grounded {
            let v = sim.suits.anchor[i].vel;
            landed_at = Some(v.length());
            break;
        }
        if k == 59 {
            assert!((sim.suits.anchor[i].vel.length() - RUN_SPEED).abs() < 0.1, "running at 16 m/s");
        }
    }
    assert!(aloft > 60, "in the air {aloft} ticks");
    let v = landed_at.expect("it came down");
    assert!((v - RUN_SPEED).abs() < 1.0, "landed at {v} m/s along the ground");
}

#[test]
fn standing_on_mo_ii_rides_it_round() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let id = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(-1.5, 1.0, 0.0));
    let i = id.idx();
    step(&mut sim);
    let (start, local) = (sim.suits.flight[i], sim.suits.anchor[i].local);
    // A whole spin and a whole circle of its station-keeping at once: 4 hours.
    let period = 432_000;
    let mut moved: f32 = 0.0;
    for k in 0..period {
        sim.step();
        if k % 97 == 0 {
            check_invariants(&sim);
        }
        let f = sim.suits.flight[i];
        let p = bodies(&sim).pose(Body::Landmark(0)).unwrap();
        assert_eq!(f.vel, p.point_vel(f.pos), "moving with the deck at {k}");
        let crept = sim.suits.anchor[i].local.distance(local);
        assert!(crept < 1e-4, "standing still on it, it crept {crept} m by {k}");
        moved = moved.max(f.pos.distance(start.pos));
    }
    assert_eq!(sim.footing(i), Footing::Grounded);
    assert!(moved > 300.0, "carried {moved} m round");
    let off = sim.suits.flight[i].pos.distance(start.pos);
    assert!(off < 1e-2, "and back where it started, but for {off} m");
}

#[test]
fn a_tap_hops_and_holding_space_lifts_off() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    let (n0, _) = ground(&sim, id);
    // A tap: one tick of Space.
    let (mut apex, mut v0, mut airborne) = (0.0f32, 0.0, 0);
    for k in 0..200 {
        drive(&mut sim, id, [0, if k == 0 { 127 } else { 0 }, 0], GRIP | FLIGHT_ASSIST, aim);
        step(&mut sim);
        match sim.footing(i) {
            Footing::Aloft => {
                airborne += 1;
                apex = apex.max(ground(&sim, id).1);
                if k == 0 {
                    v0 =
                        (bodies(&sim).pose(Body::Landmark(1)).unwrap().rot * sim.suits.anchor[i].vel).dot(n0);
                }
            }
            Footing::Grounded if k > 0 => break,
            footing => panic!("{footing:?} at {k}"),
        }
    }
    // Up at 10 m/s (and the tap's own tick of the thrusters), braked coming down.
    assert!((JUMP_SPEED - 0.3..JUMP_SPEED + 0.7).contains(&v0), "hopped at {v0} m/s");
    let want = v0 * v0 / (2.0 * GRIP_ACCEL);
    assert!((apex - want).abs() < 0.3, "apex {apex} m, not {want}");
    assert!((95..=110).contains(&airborne), "{airborne} ticks in the air");
    // Held: it lifts off, and past 40 m it's flying.
    let mut free_at = None;
    for k in 0..120 {
        drive(&mut sim, id, [0, 127, 0], GRIP | FLIGHT_ASSIST, aim);
        step(&mut sim);
        if sim.footing(i) == Footing::Free {
            free_at = Some(k);
            break;
        }
    }
    let k = free_at.expect("never lifted off");
    // Climbing at 20 m/s once flight assist has it there: about 2.5 s to 40 m.
    assert!((60..100).contains(&k), "free after {k} ticks");
}

#[test]
fn crouch_is_sticky_and_space_stands_then_hops() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    for _ in 0..20 {
        drive(&mut sim, id, [0, -127, 0], GRIP, aim);
        step(&mut sim);
    }
    assert_eq!(sim.suits.anchor[i].stance, CROUCH_STANCE);
    // Nothing on the stick keeps the crouch; crouched, it creeps.
    for _ in 0..60 {
        drive(&mut sim, id, [0, 0, 127], GRIP, aim);
        step(&mut sim);
        assert_eq!(sim.suits.anchor[i].stance, CROUCH_STANCE);
        assert!(sim.suits.anchor[i].vel.length() <= CROUCH_SPEED + 1e-4);
    }
    // Space from a crouch: it stands (17 ticks), then hops.
    for k in 0..30 {
        drive(&mut sim, id, [0, 127, 0], GRIP, aim);
        step(&mut sim);
        if k < 17 {
            assert_eq!(sim.footing(i), Footing::Grounded, "hopped from a crouch at {k}");
        } else {
            assert_eq!(sim.footing(i), Footing::Aloft, "stood, and didn't hop at {k}");
            break;
        }
    }
    assert_eq!(sim.suits.anchor[i].stance, STANCE);
}

#[test]
fn grip_off_pushes_off_with_the_surface_velocity() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    // On the aft module's corner, where MO-II's surface moves fastest.
    let id = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(-230.0, 80.0, 80.0));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    for _ in 0..30 {
        drive(&mut sim, id, [0; 3], GRIP, aim);
        step(&mut sim);
    }
    let before = sim.suits.flight[i];
    let a = sim.suits.anchor[i];
    let deck = bodies(&sim).pose(Body::Landmark(0)).unwrap();
    assert!(
        (before.vel - deck.point_vel(before.pos)).length() < 1e-4,
        "standing still, it moves with the deck"
    );
    assert!(before.vel.length() > 2.0, "the deck moves at {} m/s here", before.vel.length());
    drive(&mut sim, id, [0; 3], 0, aim);
    step(&mut sim);
    assert_eq!(sim.footing(i), Footing::Free);
    let (_, n, _) = place(&LANDMARKS[0].shape, a.local, a.stance);
    let n = bodies(&sim).pose(Body::Landmark(0)).unwrap().rot * n;
    let v = sim.suits.flight[i].vel;
    let want = before.vel + n * TAKEOFF_SPEED;
    assert!((v - want).length() < 1e-4, "pushed off at {v}, not {want}");
}

#[test]
fn legless_suits_grip_kneel_and_lift_on_thrusters() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    sim.suits.part_hp[i][Part::Legs as usize] = 0.0;
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    let (n, _) = ground(&sim, id);
    let side = n.cross(aim).normalize();
    for _ in 0..30 {
        drive(&mut sim, id, [0, 0, 127], GRIP, side);
        let before = sim.suits.flight[i].rot;
        step(&mut sim);
        assert_eq!(sim.footing(i), Footing::Grounded);
        assert_eq!(sim.suits.anchor[i].vel.length(), 0.0, "no legs, no walking");
        let turned = sim.suits.flight[i].rot.angle_between(before);
        assert!(turned <= LEGLESS_TURN_RATE * DT + 1e-4, "turned {turned} rad in a tick");
    }
    assert_eq!(sim.suits.anchor[i].stance, CROUCH_STANCE, "it kneels");
    // Space: no hop, but the thrusters lift it off.
    drive(&mut sim, id, [0, 127, 0], GRIP | FLIGHT_ASSIST, side);
    step(&mut sim);
    assert_eq!(sim.footing(i), Footing::Aloft);
    let mut free = false;
    for _ in 0..200 {
        drive(&mut sim, id, [0, 127, 0], GRIP | FLIGHT_ASSIST, side);
        step(&mut sim);
        free |= sim.footing(i) == Footing::Free;
    }
    assert!(free, "the thrusters never got it clear");
    // And legless, it can still be caught.
    let id = over(&mut sim, Body::Landmark(1), Vec3::new(0.5, 1.0, 0.2), 10.0, 0.0, 1.0);
    sim.suits.part_hp[id.idx()][Part::Legs as usize] = 0.0;
    drive(&mut sim, id, [0; 3], GRIP, aim);
    step(&mut sim);
    assert_eq!(sim.footing(id.idx()), Footing::Aloft);
}

#[test]
fn neo_bird_cannot_grip_and_transforming_takes_off() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::WingZero, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    let (n, _) = ground(&sim, id);
    drive(&mut sim, id, [0; 3], GRIP, aim);
    step(&mut sim);
    assert_eq!(sim.footing(i), Footing::Grounded);
    // MODE: the change of form starts, and it takes off.
    drive(&mut sim, id, [0; 3], GRIP | MODE, aim);
    step(&mut sim);
    assert!(sim.suits.form(i).changing());
    assert_eq!(sim.footing(i), Footing::Free);
    assert!((sim.suits.flight[i].vel - n * TAKEOFF_SPEED).length() < 1e-3, "pushed off the static rock");
    // A bird, armed, slow and close: never caught.
    for _ in 0..30 {
        let t = sim.next_tick();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim,
                buttons: GRIP | MODE | FLIGHT_ASSIST,
                ..InputCmd::default()
            },
        );
        step(&mut sim);
    }
    assert_eq!(sim.suits.frame[i], FrameId::WingZeroBird);
    let bird =
        over_in(&mut sim, FrameId::WingZeroBird, Body::Landmark(1), Vec3::new(0.5, 1.0, 0.2), 5.0, 0.0, 1.0);
    let bi = bird.idx();
    for _ in 0..120 {
        drive(&mut sim, bird, [0; 3], GRIP | MODE, aim);
        step(&mut sim);
        assert_eq!(sim.footing(bi), Footing::Free, "a bird was caught");
    }
}

#[test]
fn other_rocks_landmarks_and_the_colony_stay_solid_to_a_rider() {
    let mut sim = grip_sector();
    // Three round rocks of 30 m, each 4 m clear of something else: Hermit, a fourth rock, and the
    // colony's hull. A rider on each walks at what's next to it.
    let (hermit_at, hermit_n) = landmark_surface(&sim, 1, Vec3::new(0.3, 1.0, 0.2));
    let ball = |pos: Vec3| Rock { pos, radius: 30.0, axes: Vec3::splat(30.0), ..Rock::default() };
    let beside_hermit = hermit_at + hermit_n * 34.0;
    let open = Vec3::new(0.0, 5_000.0, 9_000.0);
    let hull_up = Vec3::Y;
    let over_hull = COLONY_CENTER + hull_up * (COLONY_RADIUS + 34.0);
    let rocks = [ball(beside_hermit), ball(open), ball(open + Vec3::X * 64.0), ball(over_hull)];
    sim.field = Field::from_rocks(&rocks);
    sim.rocks = RockStates::new(&sim.field);
    let walkers = [(0usize, -hermit_n), (1, Vec3::X), (3, -hull_up)];
    let ids = walkers.map(|(r, toward)| {
        let side = along(toward);
        standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Rock(r as u16), side)
    });
    let mut closest = [f32::INFINITY; 3];
    for _ in 0..300 {
        for (id, (_, toward)) in ids.iter().zip(walkers) {
            drive(&mut sim, *id, [0, 0, 127], GRIP, toward);
        }
        step(&mut sim);
        for (k, id) in ids.iter().enumerate() {
            assert_eq!(sim.footing(id.idx()), Footing::Grounded, "rider {k} let go");
            let p = sim.suits.flight[id.idx()].pos;
            let gap = match k {
                0 => landmark_probe(&sim, 1, p).1.dist,
                1 => {
                    let r = sim.field.rocks()[2];
                    // Out along the line of centres, the collider (grown by the clearance) is a
                    // sphere: its distance is exact.
                    p.distance(r.pos) - r.radius
                }
                _ => {
                    let rel = p - COLONY_CENTER;
                    Vec3::new(0.0, rel.y, rel.z).length() - COLONY_RADIUS
                }
            };
            closest[k] = closest[k].min(gap);
        }
    }
    // Each met what it walked at (a suit at rest on a surface sits at its clearance) and never got
    // into it.
    assert!((SUIT_CLEARANCE - 0.05..SUIT_CLEARANCE + 1.0).contains(&closest[0]), "Hermit: {}", closest[0]);
    assert!((SUIT_CLEARANCE - 0.05..SUIT_CLEARANCE + 1.0).contains(&closest[1]), "the rock: {}", closest[1]);
    // (The hull keeps suits 12 m off.)
    assert!((12.0 - 0.05..20.0).contains(&closest[2]), "the hull: {}", closest[2]);
}

#[test]
fn grounded_suits_can_aim_overhead() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let (n, _) = ground(&sim, id);
    for _ in 0..60 {
        drive(&mut sim, id, [0; 3], GRIP, n);
        step(&mut sim);
    }
    let lean = (sim.suits.flight[id.idx()].rot * Vec3::Z).dot(n);
    assert!((lean - bc_sim::ground::GROUND_LEAN_SIN).abs() < 1e-3, "leaning back {lean}");
    let from = sim.events.next_seq();
    drive(&mut sim, id, [0; 3], GRIP | FIRE_PRIMARY, n);
    step(&mut sim);
    let shot = events_since(&sim, from)
        .into_iter()
        .find_map(|e| match e {
            Event::BeamSpawn { velocity, .. } => Some(velocity.normalize()),
            _ => None,
        })
        .expect("it fired");
    assert!(
        shot.dot(n) >= 0.99,
        "the rifle reached {:.1}° from straight up",
        shot.angle_between(n).to_degrees()
    );
}

#[test]
fn throw_recoil_moves_a_grounded_suit_along_the_ground() {
    // A rider's world velocity is derived from its anchor every tick, so a throw's push back has to
    // go into the anchor: the suit slides back along the ground and the legs stop it.
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let (n, _) = ground(&sim, id);
    let ahead = sim.suits.flight[i].rot * Vec3::Z;
    // A tonne of ore just off the left hand.
    let desc = ChunkDesc { kind: ChunkKind::Ore { ore: 1 }, seed: 1, mass_kg: 1_000 };
    let f = sim.suits.flight[i];
    let hand = f.pos + f.rot * bc_sim::content::ArmSlot::Left.muzzle();
    let at = hand + f.rot * -Vec3::X * (chunks::radius(&desc) + 0.5);
    let t = sim.tick();
    let seg = Segment { t0: t, pos: at, ..Segment::default() }.quantized();
    let k = sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap() as usize;
    for _ in 0..2 {
        drive(&mut sim, id, [0; 3], GRIP | GRAB, ahead);
        step(&mut sim);
    }
    assert_eq!(sim.held_chunk(i), Some(k), "it took the ore in hand");
    let mods = sim.flight_mods(i);
    let suit_kg =
        frame(FrameId::Leo).mass(sim.suits.flight[i].propellant) + mods.extra_mass_kg as f32 - 1_000.0;
    let (before, start) = (sim.suits.flight[i].vel, sim.suits.anchor[i].local);
    drive(&mut sim, id, [0; 3], GRIP | GRAB | THROW, ahead);
    step(&mut sim);
    let Motion::Free(thrown) = sim.chunks.motion[k] else { panic!("still in hand") };
    assert_eq!(sim.footing(i), Footing::Grounded);
    // Momentum is kept: the push is in the derived world velocity, back and along the ground.
    let kick = sim.suits.flight[i].vel - before;
    let p = (thrown.vel - before) * 1_000.0 + kick * suit_kg;
    assert!(p.length() < 0.1, "momentum not kept: {p:?}");
    assert!(kick.dot(ahead) < -3.0, "pushed back at {kick}");
    // It slides back, keeping its feet, until the legs have stopped it.
    for _ in 0..30 {
        drive(&mut sim, id, [0; 3], GRIP, ahead);
        step(&mut sim);
        assert_eq!(sim.footing(i), Footing::Grounded);
    }
    let pose = bodies(&sim).pose(Body::Landmark(1)).unwrap();
    let moved = pose.rot * (sim.suits.anchor[i].local - start);
    assert!(moved.dot(ahead) < -0.3, "slid {} m back", -moved.dot(ahead));
    assert!(moved.dot(n).abs() < 0.05, "and along the ground, not off it: {}", moved.dot(n));
    assert_eq!(sim.suits.anchor[i].vel, Vec3::ZERO, "the legs stopped it");
}

// -------------------------------------------------------------------------------------------------
// Bodies going away, and sleeping on them
// -------------------------------------------------------------------------------------------------

#[test]
fn shattering_the_rock_underfoot_sets_the_suit_free() {
    let mut sim = grip_sector();
    let (r, _) = grippable_rock(&sim, 300.0);
    let id = standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Rock(r as u16), Vec3::Y);
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    drive(&mut sim, id, [0; 3], GRIP, aim);
    step(&mut sim);
    let (n, _) = ground(&sim, id);
    sim.field.set_dead(r, true);
    drive(&mut sim, id, [0; 3], GRIP, aim);
    step(&mut sim);
    assert_eq!(sim.footing(i), Footing::Free, "free the tick after");
    let v = sim.suits.flight[i].vel;
    assert!((v - n * UNPARK_SPEED).length() < 1e-5, "floating off at {v}");
}

#[test]
fn a_grounded_suit_sleeps_parked_and_wakes_grounded_and_crouched() {
    let mut sim = grip_sector();
    let (r, _) = grippable_rock(&sim, 300.0);
    let id = standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Rock(r as u16), Vec3::Y);
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    for _ in 0..20 {
        drive(&mut sim, id, [0, -127, 0], GRIP, aim);
        step(&mut sim);
    }
    assert_eq!(sim.parkable(i), Some(Body::Rock(r as u16)));
    assert!(sim.sleep(id));
    assert!(sim.is_parked(i));
    assert_eq!(sim.footing(i), Footing::Grounded);
    let at = sim.suits.flight[i].pos;
    for _ in 0..300 {
        step(&mut sim);
        assert_eq!(sim.suits.flight[i].pos, at, "held where it knelt");
        assert_eq!(sim.suits.flight[i].vel, Vec3::ZERO);
    }
    assert!(sim.wake(id));
    assert_eq!(sim.footing(i), Footing::Grounded);
    assert!(sim.suits.input[i].pressed(GRIP), "it wakes gripping");
    // The pilot's client may be slow to speak: its stand-in keeps the grip and the crouch.
    for _ in 0..30 {
        step(&mut sim);
    }
    assert_eq!(sim.footing(i), Footing::Grounded);
    assert_eq!(sim.suits.anchor[i].stance, CROUCH_STANCE, "still crouched");
}

#[test]
fn an_aloft_suit_that_sleeps_settles_then_parks() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    drive(&mut sim, id, [0, 127, 0], GRIP, aim);
    step(&mut sim);
    for _ in 0..10 {
        drive(&mut sim, id, [0, 0, 60], GRIP, aim);
        step(&mut sim);
    }
    assert_eq!(sim.footing(i), Footing::Aloft);
    assert!(sim.sleep(id));
    assert!(sim.is_sleeping(i) && !sim.is_parked(i), "in the air, not parked yet");
    let mut parked_at = None;
    for k in 0..200 {
        step(&mut sim);
        if sim.is_parked(i) {
            parked_at.get_or_insert(k);
            assert_eq!(sim.footing(i), Footing::Grounded);
            assert_eq!(sim.suits.anchor[i].vel, Vec3::ZERO, "down, it stops");
        }
    }
    assert!(parked_at.is_some_and(|k| k < 120), "it never came down: {parked_at:?}");
}

#[test]
fn parked_on_mo_ii_moves_with_it_and_carries_point_velocity() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let deck = Body::Landmark(0);
    // Asleep on its feet on the core, and one that was only resting against the aft module.
    let standing = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(-1.5, 1.0, 0.0));
    let (p, n) = surface_of(&sim, deck, Vec3::new(-1.0, 0.1, 0.05));
    let pos = p + n * (SUIT_CLEARANCE + 0.5);
    let resting = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, look_rotation(n, Vec3::Y))
        .unwrap();
    sim.suits.flight[resting.idx()].vel = bodies(&sim).pose(deck).unwrap().point_vel(pos);
    assert_eq!(sim.parkable(resting.idx()), Some(deck));
    for id in [standing, resting] {
        assert!(sim.sleep(id) && sim.is_parked(id.idx()));
    }
    assert_eq!(sim.footing(resting.idx()), Footing::Free, "resting isn't standing");
    let start = [standing, resting].map(|id| sim.suits.flight[id.idx()].pos);
    for _ in 0..600 {
        step(&mut sim);
        let pose = bodies(&sim).pose(deck).unwrap();
        for id in [standing, resting] {
            let f = sim.suits.flight[id.idx()];
            assert_eq!(f.vel, pose.point_vel(f.pos), "moving with the deck");
            assert_eq!(f.ang_vel, pose.ang_vel, "turning with it");
        }
    }
    for (id, s) in [standing, resting].iter().zip(start) {
        assert!(sim.suits.flight[id.idx()].pos.distance(s) > 10.0, "it went round with MO-II");
    }
    // Awake, the one that rested lets go, moving as the deck does there.
    assert!(sim.wake(resting));
    let f = sim.suits.flight[resting.idx()];
    assert_eq!(sim.suits.anchor[resting.idx()], Default::default());
    assert_eq!(f.vel, bodies(&sim).pose(deck).unwrap().point_vel(f.pos));
}

#[test]
fn sleepers_run_no_specials() {
    let mut sim = grip_sector();
    let (_, rock) = grippable_rock(&sim, 300.0);
    let (zero, _) = common::resting_as(&mut sim, FrameId::WingZero, &rock, Vec3::Y);
    let (scythe, _) = common::resting_as(&mut sim, FrameId::Deathscythe, &rock, -Vec3::Y);
    for _ in 0..40 {
        for id in [zero, scythe] {
            let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
            drive(&mut sim, id, [0; 3], MODE, aim);
        }
        step(&mut sim);
    }
    assert_eq!(sim.suits.frame[zero.idx()], FrameId::WingZeroBird);
    assert!(sim.suits.special[scythe.idx()].active, "the jammer is on");
    for id in [zero, scythe] {
        assert!(sim.sleep(id) && sim.is_parked(id.idx()), "suit {} didn't park", id.idx());
    }
    for _ in 0..600 {
        step(&mut sim);
        assert_eq!(sim.suits.frame[zero.idx()], FrameId::WingZeroBird, "a parked bird stays a bird");
        assert!(!sim.suits.special[scythe.idx()].active, "and a parked jammer stays off");
        assert!(sim.is_parked(zero.idx()) && sim.is_parked(scythe.idx()));
    }
}

#[test]
fn seized_riders_keep_their_grip() {
    let mut sim = grip_sector();
    let id = on_hermit(&mut sim, FrameId::WingZero, Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    drive(&mut sim, id, [0; 3], GRIP | ZERO, aim);
    step(&mut sim);
    // The System seizes the suit (strain at full): it flies it for 3 s, grip and all.
    sim.suits.ai[i].anchor = sim.suits.flight[i].pos;
    sim.suits.zero[i].mode = zero_mode::SEIZED;
    sim.suits.zero[i].timer = 90;
    for k in 0..89 {
        step(&mut sim);
        assert_eq!(sim.suits.zero[i].mode, zero_mode::SEIZED);
        assert!(sim.suits.input[i].pressed(GRIP), "the seizure dropped the grip at {k}");
        assert_ne!(sim.footing(i), Footing::Free, "and let go at {k}");
    }
}

/// What one snapshot to `viewer` says of it and of suit `j`, written and read back.
fn on_the_wire(sim: &Sim, viewer: usize, j: usize) -> (OwnState, EntityState) {
    let mut buf = [0u8; 1500];
    let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
    w.header(&SnapshotHeader { tick: sim.tick(), ..SnapshotHeader::default() });
    w.own(Some(&sim.own_state(viewer)));
    w.zero(None);
    assert!(w.entity(&sim.entity_state(j, viewer), 0));
    let n = w.finish().unwrap();
    let mut r = SnapshotReader::new(&buf[..n]).unwrap();
    (r.own().unwrap().unwrap(), r.next_entity().unwrap().unwrap())
}

/// Where a decoded rider is in the sector, on its body as the snapshot's tick has it.
fn composed(sim: &Sim, body: BodyRef, pos: Vec3, rot: Quat, vel: Vec3) -> FlightState {
    let pose = bodies(sim).pose(Body::from(body)).unwrap();
    let a = Anchor { body: Body::from(body), local: pos, rot, vel, ..Anchor::default() };
    let mut f = FlightState::default();
    derive(&pose, &a, &mut f);
    f
}

#[test]
fn riders_go_on_the_wire_in_their_bodys_frame() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let deck = BodyRef::Landmark(0);
    let rider = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(-1.5, 1.0, 0.0));
    let r = rider.idx();
    let at = sim.suits.flight[r].pos;
    let viewer = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, at + Vec3::Y * 300.0, Quat::IDENTITY)
        .unwrap()
        .idx();
    let aim = sim.suits.flight[r].rot * Vec3::Z;
    for _ in 0..10 {
        drive(&mut sim, rider, [0, 0, 127], GRIP, aim);
        step(&mut sim);
    }
    assert_eq!(sim.footing(r), Footing::Grounded);

    // Its own pilot gets the anchor itself: composed on the deck as the server has it, the world
    // state comes back to the bit (position and velocity are sent whole).
    let (a, f) = (sim.suits.anchor[r], sim.suits.flight[r]);
    let own = sim.own_state(r);
    assert_eq!(own.surface, Some(OwnSurface { footing: footing::GROUNDED, body: deck, stance_q: 146 }));
    assert_eq!((own.pos, own.vel, own.cover), (a.local, a.vel, sim.cover_code(r)));
    let (mine, _) = on_the_wire(&sim, r, viewer);
    assert_eq!(mine.surface, own.surface);
    let back = composed(&sim, deck, mine.pos, a.rot, mine.vel);
    let bits = |v: Vec3| v.to_array().map(f32::to_bits);
    assert_eq!((bits(back.pos), bits(back.vel)), (bits(f.pos), bits(f.vel)));
    assert!(mine.rot.dot(a.rot).abs() > 1.0 - 1e-6);
    // Everyone else gets it on the deck to 1.6 cm, walking at its pace over it.
    let (theirs, e) = on_the_wire(&sim, viewer, r);
    assert_eq!((theirs.surface, theirs.pos), (None, sim.suits.flight[viewer].pos), "flying free");
    assert_eq!(e.on, Some(RiderOn { body: deck, aloft: false }));
    let seen = composed(&sim, deck, e.pos, e.rot, e.vel);
    assert!(seen.pos.distance(f.pos) < 0.016, "{} vs {}", seen.pos, f.pos);
    assert!(seen.vel.distance(f.vel) < 0.06, "{} vs {}", seen.vel, f.vel);
    assert!(e.vel.length() > 3.0, "walking: {}", e.vel);

    // A hop: in the deck's grip, in the air.
    drive(&mut sim, rider, [0, 127, 0], GRIP, aim);
    step(&mut sim);
    hands_off_gripping(&mut sim, rider, aim, 3);
    assert_eq!(sim.footing(r), Footing::Aloft);
    assert_eq!(sim.own_state(r).surface.map(|s| s.footing), Some(footing::ALOFT));
    assert_eq!(on_the_wire(&sim, viewer, r).1.on, Some(RiderOn { body: deck, aloft: true }));

    // Down again and asleep: parked, at rest on the deck however it moves.
    hands_off_gripping(&mut sim, rider, aim, 240);
    assert_eq!(sim.footing(r), Footing::Grounded);
    assert!(sim.sleep(rider) && sim.is_parked(r));
    step(&mut sim);
    let (_, e) = on_the_wire(&sim, viewer, r);
    assert_eq!((e.on, e.vel), (Some(RiderOn { body: deck, aloft: false }), Vec3::ZERO));
    let f = sim.suits.flight[r];
    let seen = composed(&sim, deck, e.pos, e.rot, e.vel);
    assert!(seen.pos.distance(f.pos) < 0.016, "{} vs {}", seen.pos, f.pos);
    // (Moving as the deck does where it's drawn, 1.6 cm off: ω·1.6 cm.)
    assert!(seen.vel.distance(f.vel) < 1e-3, "{} vs {}", seen.vel, f.vel);

    // A suit only resting against the deck parks there too, and goes out on it.
    let (p, n) = surface_of(&sim, Body::Landmark(0), Vec3::new(-1.0, 0.1, 0.05));
    let pos = p + n * (SUIT_CLEARANCE + 0.5);
    let resting = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, look_rotation(n, Vec3::Y))
        .unwrap();
    sim.suits.flight[resting.idx()].vel = bodies(&sim).pose(Body::Landmark(0)).unwrap().point_vel(pos);
    assert!(sim.sleep(resting) && sim.is_parked(resting.idx()));
    assert_eq!(sim.footing(resting.idx()), Footing::Free);
    step(&mut sim);
    let (_, e) = on_the_wire(&sim, viewer, resting.idx());
    assert_eq!((e.on, e.vel), (Some(RiderOn { body: deck, aloft: false }), Vec3::ZERO));
    let seen = composed(&sim, deck, e.pos, e.rot, e.vel);
    assert!(seen.pos.distance(sim.suits.flight[resting.idx()].pos) < 0.016);
}

/// `ticks` ticks of `id` holding its grip, stick idle.
fn hands_off_gripping(sim: &mut Sim, id: SuitId, aim: Vec3, ticks: u32) {
    for _ in 0..ticks {
        drive(sim, id, [0; 3], GRIP, aim);
        step(sim);
    }
}

// -------------------------------------------------------------------------------------------------
// Fighting on and over the surfaces
// -------------------------------------------------------------------------------------------------

#[test]
fn shots_hit_a_suit_standing_on_mo_ii_and_the_hull_shields_one_on_the_far_side() {
    for rewind in [0, 8] {
        // On the +y pylon's top, and a Leo 400 m over it.
        let mut sim = sector(2);
        let target = standing_on(&mut sim, FrameId::Taurus, Faction::Oz, Body::Landmark(0), Vec3::Y);
        let (n, _) = ground(&sim, target);
        let at = sim.suits.flight[target.idx()].pos;
        let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, at + n * 400.0, -n);
        let (hits, _) = one_beam(&mut sim, leo, target, rewind);
        assert_eq!(hits, 1, "rewound {rewind}: a rider in plain sight wasn't hit");
        // On the -y pylon's, with MO-II's core between them.
        let mut sim = sector(2);
        let target = standing_on(&mut sim, FrameId::Taurus, Faction::Oz, Body::Landmark(0), -Vec3::Y);
        let (n, _) = ground(&sim, target);
        let at = sim.suits.flight[target.idx()].pos;
        let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, at - n * 500.0, n);
        let (hits, _) = one_beam(&mut sim, leo, target, rewind);
        assert_eq!(hits, 0, "rewound {rewind}: the shot went through MO-II");
    }
}

#[test]
fn crouched_in_the_deep_the_rim_stops_a_level_shot_and_a_shot_from_above_hits() {
    // Hermit's Deep is 30 m deep: crouched on its floor, a suit is below its rim.
    let hermit = landmark_pose(&LANDMARKS[1], 0, 0.0);
    for (from, hit) in [(Vec3::new(500.0, 600.0, 0.0), false), (Vec3::new(40.0, 1_100.0, 30.0), true)] {
        let mut sim = sector(2);
        let target = standing_on(&mut sim, FrameId::Taurus, Faction::Oz, Body::Landmark(1), Vec3::Y);
        for _ in 0..20 {
            let aim = sim.suits.flight[target.idx()].rot * Vec3::Z;
            drive(&mut sim, target, [0, -127, 0], GRIP, aim);
            step(&mut sim);
        }
        assert_eq!(sim.suits.anchor[target.idx()].stance, CROUCH_STANCE);
        let at = hermit.to_world(from);
        let facing = sim.suits.flight[target.idx()].pos - at;
        let leo = suit(&mut sim, FrameId::Leo, Faction::Colonies, at, facing);
        let (hits, _) = one_beam(&mut sim, leo, target, 0);
        assert_eq!(hits, usize::from(hit), "from {from}: {hits} hits");
    }
}

#[test]
fn a_lag_compensated_shot_from_a_rider_on_mo_ii_leaves_where_it_was_seen() {
    // On the fore module's rim, 120 m off MO-II's axis: the deck there moves 2.9 m/s.
    let mut sim = sector(2);
    let id = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(230.0, 80.0, 80.0));
    let i = id.idx();
    let (n, _) = ground(&sim, id);
    for _ in 0..10 {
        drive(&mut sim, id, [0; 3], GRIP, n);
        step(&mut sim);
    }
    // Its pilot saw the world, its own suit on the deck included, 8 ticks back.
    let rewind = 8;
    let t = sim.next_tick();
    let cmd = InputCmd {
        tick: t,
        view_tick_q4: (t - rewind) << 4,
        aim: n,
        buttons: GRIP | FIRE_PRIMARY,
        ..InputCmd::default()
    };
    sim.set_input(id, cmd);
    let from = sim.events.next_seq();
    step(&mut sim);
    let origin = events_since(&sim, from)
        .into_iter()
        .find_map(|e| match e {
            Event::BeamSpawn { origin, tick, .. } => Some((origin, tick)),
            _ => None,
        })
        .expect("it fired");
    assert_eq!(origin.1, t - rewind);
    let muzzle = frame(FrameId::Leo).loadout[0].unwrap().arm.muzzle();
    let mut seen = sim.suits.flight[i];
    bc_sim::ground::derive(&landmark_pose(&LANDMARKS[0], t - rewind, 0.0), &sim.suits.anchor[i], &mut seen);
    let (seen, now) =
        (seen.pos + seen.rot * muzzle, sim.suits.flight[i].pos + sim.suits.flight[i].rot * muzzle);
    assert!(origin.0.distance(seen) < 1e-3, "left {} m from where it was seen", origin.0.distance(seen));
    assert!(now.distance(seen) > 0.5, "(the deck moved only {} m)", now.distance(seen));
}

#[test]
fn missile_seekers_drop_parked_and_hidden_targets_but_not_ones_in_the_open() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Target {
        Open,
        Hidden,
        Parked,
    }
    // A Leo in Hermit's Deep, and a salvo 1 km over it that only its seekers can bring in: flying
    // blind, it would pass 500 m off.
    let hits = |target: Target| {
        let mut sim = sector(2);
        let id = standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Landmark(1), Vec3::Y);
        let i = id.idx();
        let crouch = if target == Target::Open { 0 } else { -127 };
        for _ in 0..(20 + bc_sim::sim::LURK_SETTLE_TICKS) {
            let aim = sim.suits.flight[i].rot * Vec3::Z;
            let ty = if sim.suits.anchor[i].stance == CROUCH_STANCE { 0 } else { crouch };
            drive(&mut sim, id, [0, ty, 0], GRIP, aim);
            step(&mut sim);
        }
        if target == Target::Parked {
            assert!(sim.sleep(id) && sim.is_parked(i));
            sim.suits.slept_at[i] = sim.tick() - bc_sim::sim::POWER_DOWN_TICKS;
            step(&mut sim);
        }
        let expected = if target == Target::Open { cover::EXPOSED } else { cover::HIDDEN };
        assert_eq!(sim.cover_code(i), expected, "{target:?}");
        let (n, _) = ground(&sim, id);
        let at = sim.suits.flight[i].pos;
        let side = n.cross(Vec3::X).normalize();
        let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Oz, at + n * 3_000.0, -n);
        let from = sim.events.next_seq();
        let spec = bc_sim::content::weapon(WeaponKind::HomingMissile).missile.unwrap();
        for k in 0..4 {
            let pos = at + n * 1_000.0 + side * (k as f32 * 20.0);
            let vel = -n * 200.0 + side * 100.0;
            let target = (i as u16, sim.suits.generation[i]);
            sim.missiles.spawn(
                WeaponKind::HomingMissile,
                ha.idx() as u16,
                Faction::Oz,
                target,
                pos,
                vel,
                spec.dv,
                sim.tick() + 240,
            );
        }
        for _ in 0..240 {
            if target != Target::Parked {
                let aim = sim.suits.flight[i].rot * Vec3::Z;
                drive(&mut sim, id, [0; 3], GRIP, aim);
            }
            step(&mut sim);
        }
        hits_on(&sim, from, id)
    };
    assert!(hits(Target::Open) > 0, "the seekers lost a suit in the open");
    assert_eq!(hits(Target::Hidden), 0, "the seekers kept a hidden suit");
    assert_eq!(hits(Target::Parked), 0, "the seekers kept a parked suit gone dark");
}

/// A Leo standing on top of a big rock (a lone one, 40 m or more across), and the rock's index.
fn on_a_rock(sim: &mut Sim) -> (SuitId, usize) {
    let (r, _) = common::lone_rock(sim, 40.0, 300.0);
    (standing_on(sim, FrameId::Leo, Faction::Colonies, Body::Rock(r as u16), Vec3::Y), r)
}

/// `id` crouches (or stands), then swings its saber aiming `c` (the cosine to the ground's
/// normal: 0 along the ground, -1 straight into it): the rock's structure left.
fn swing_at_the_ground(sim: &mut Sim, id: SuitId, r: usize, c: f32, crouch: bool) -> f32 {
    let (n, _) = ground(sim, id);
    let aim = along(n) * (1.0 - c * c).sqrt() + n * c;
    let ty = if crouch { -127 } else { 64 };
    for k in 0..60 {
        let melee = if k == 20 { MELEE } else { 0 };
        drive(sim, id, [0, ty, 0], GRIP | melee, aim);
        step(sim);
    }
    sim.rocks.hp[r]
}

#[test]
fn melee_does_not_dig_the_rock_underfoot_unless_aimed_down() {
    // Whether a saber swung aiming `c` (crouched or standing) cut the rock its Leo stands on.
    let cuts = |c: f32, crouch: bool| {
        let mut sim = grip_sector();
        let (id, r) = on_a_rock(&mut sim);
        let whole = sim.rocks.hp[r];
        let cut = swing_at_the_ground(&mut sim, id, r, c, crouch) < whole;
        assert_eq!(sim.suits.melee[id.idx()].dig_own, c < -0.5, "aimed {c}");
        assert_eq!(sim.footing(id.idx()), Footing::Grounded);
        cut
    };
    // Crouched, a swing aimed only a little down already goes into the rock: it isn't digging, so
    // the rock is left alone...
    for c in [0.0, -0.2, -0.35, -0.49] {
        assert!(!cuts(c, true), "a swing aimed {c} cut the rock underfoot");
    }
    // ...but aimed down into it, crouched or standing, it digs.
    assert!(cuts(-0.9, true) && cuts(-0.9, false), "a strike down into the rock missed it");
}

#[test]
fn digging_down_can_shatter_it_and_frees_the_suit() {
    let mut sim = grip_sector();
    let (id, r) = on_a_rock(&mut sim);
    let i = id.idx();
    let (n, _) = ground(&sim, id);
    sim.rocks.hp[r] = 1.0;
    let from = sim.events.next_seq();
    let mut broke = None;
    for k in 0..60 {
        let melee = if k == 20 { MELEE } else { 0 };
        drive(&mut sim, id, [0, -127, 0], GRIP | melee, -n);
        step(&mut sim);
        if broke.is_none() && sim.rocks.destroyed.get(r) {
            broke = Some(sim.tick());
            assert_eq!(sim.footing(i), Footing::Grounded, "let go the very tick it broke");
        } else if broke.is_some() {
            break;
        }
    }
    assert!(broke.is_some(), "the dig never shattered it");
    let by = events_since(&sim, from).into_iter().find_map(|e| match e {
        Event::RockBreak { rock, by, .. } if rock as usize == r => Some(by),
        _ => None,
    });
    assert_eq!(by, Some(i as u16));
    // The tick after, it's off the rock (still lunging down into where it was: see
    // `shattering_the_rock_underfoot_sets_the_suit_free` for the float-off itself).
    assert_eq!(sim.footing(i), Footing::Free);
    assert_eq!(sim.suits.anchor[i], Default::default());
}

#[test]
fn melee_between_riders_of_mo_ii_meets_as_seen() {
    // Two Leos on the fore module's top, one facing the other's back `d` m ahead along the deck,
    // both still. The first swings its saber (and lunges) with its pilot's view `rewind` ticks old.
    let hits = |d: f32, rewind: u32| {
        let mut sim = sector(2);
        let a = on_mo_ii(&mut sim, FrameId::Leo, Vec3::new(230.0, 80.0, -40.0));
        let b = standing_on(
            &mut sim,
            FrameId::Leo,
            Faction::Oz,
            Body::Landmark(0),
            Vec3::new(230.0, 80.0, -40.0 + d),
        );
        let (aim_a, aim_b) =
            (sim.suits.flight[a.idx()].rot * Vec3::Z, sim.suits.flight[b.idx()].rot * Vec3::Z);
        let from = sim.events.next_seq();
        for k in 0..40 {
            let t = sim.next_tick();
            let melee = if k == 12 { MELEE } else { 0 };
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: (t - rewind) << 4,
                aim: aim_a,
                buttons: GRIP | melee,
                ..InputCmd::default()
            };
            sim.set_input(a, cmd);
            drive(&mut sim, b, [0; 3], GRIP, aim_b);
            step(&mut sim);
        }
        hits_on(&sim, from, b) > 0
    };
    // Where the live strike stops reaching...
    let (mut near, mut far) = (6.0, 60.0);
    assert!(hits(near, 0) && !hits(far, 0));
    for _ in 0..12 {
        let mid = (near + far) / 2.0;
        if hits(mid, 0) {
            near = mid;
        } else {
            far = mid;
        }
    }
    // ...is where a lagging pilot's does: on the deck, nothing moved while its view lagged. (The deck
    // itself moved 0.5 m in 8 ticks.)
    for rewind in [3, 8] {
        assert!(hits(near - 0.05, rewind), "rewound {rewind}, it fell short at {near} m");
        assert!(!hits(far + 0.05, rewind), "rewound {rewind}, it reached {far} m");
    }
}

#[test]
fn spill_from_a_rider_lands_outside_its_body() {
    // A Leo on Hermit's flank with a full hold, leaning back to aim straight up: behind it is into
    // the ground.
    let spills = |dies: bool| {
        let mut sim = grip_sector();
        let id = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
        let i = id.idx();
        let (n, _) = ground(&sim, id);
        for _ in 0..40 {
            drive(&mut sim, id, [0; 3], GRIP, n);
            step(&mut sim);
        }
        sim.suits.cargo_kg[i] = [300; bc_proto::CARGO_KINDS];
        let before = sim.chunks.alive.iter().collect::<Vec<_>>();
        if dies {
            sim.suits.part_hp[i][bc_proto::Part::Torso as usize] = 1.0;
            let at = sim.suits.flight[i].pos;
            let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, at + n * 300.0, -n);
            one_beam(&mut sim, leo, id, 0);
            assert!(!sim.suits.alive.get(i), "it didn't die");
        } else {
            drive(&mut sim, id, [0; 3], GRIP | bc_proto::buttons::JETTISON, n);
            step(&mut sim);
        }
        let ore: Vec<usize> = sim
            .chunks
            .alive
            .iter()
            .filter(|k| !before.contains(k) && matches!(sim.chunks.desc[*k].kind, ChunkKind::Ore { .. }))
            .collect();
        assert_eq!(ore.len(), bc_proto::CARGO_KINDS, "dies {dies}");
        (sim, ore)
    };
    for dies in [false, true] {
        let (sim, ore) = spills(dies);
        for k in ore {
            let seg = free_segment(&sim, k);
            let at = segment_pos(&seg, f64::from(seg.t0));
            let (_, pr) = landmark_probe(&sim, 1, at);
            let r = chunks::radius(&sim.chunks.desc[k]);
            assert!(pr.dist > r, "dies {dies}: ore {k} spilled {} m into Hermit", r - pr.dist);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Brains and the surfaces
// -------------------------------------------------------------------------------------------------

#[test]
fn dolls_hunt_riders_from_above_and_do_not_push_into_landmarks() {
    use bc_sim::ai::{DOLL, drive as doll_drive};
    use bc_sim::perception::Perception;

    let mut sim = sector(2);
    // A tough Leo on Hermit's flank.
    let rider = on_hermit(&mut sim, FrameId::Leo, Vec3::new(0.3, 1.0, 0.2));
    let r = rider.idx();
    sim.suits.part_hp[r] = [1e9; bc_proto::Part::COUNT];
    let (n, _) = ground(&sim, rider);
    let at = sim.suits.flight[r].pos;
    let side = n.cross(Vec3::X).normalize();
    // One doll level with it 1.4 km off along the ground, and one skimming the rock round the far
    // side of it, 60 m up.
    let spawn = |sim: &mut Sim, pos: Vec3| {
        sim.spawn_at(
            FrameId::Taurus,
            Faction::Oz,
            PilotKind::MobileDoll,
            pos,
            look_rotation(at - pos, Vec3::Y),
        )
        .unwrap()
    };
    let level = spawn(&mut sim, at + side * 1_400.0);
    let (p, m) = landmark_surface(&sim, 1, Vec3::new(-0.3, -0.2, 1.0));
    let skimmer = spawn(&mut sim, p + m * (STANCE + 60.0));
    for &d in &[level, skimmer] {
        let (_, pr) = landmark_probe(&sim, 1, sim.suits.flight[d.idx()].pos);
        assert!(pr.dist > 50.0, "doll {} starts in the rock", d.idx());
    }
    let above = |sim: &Sim, d: SuitId| (sim.suits.flight[d.idx()].pos - at).dot(n);
    let hold = |sim: &mut Sim| {
        let aim = sim.suits.flight[r].rot * Vec3::Z;
        drive(sim, rider, [0; 3], GRIP, aim);
        step(sim);
    };
    hold(&mut sim);
    // The doll level with it is pushed up off the ground's plane there: its brain, given the rider
    // on the ground, and the same rider flying.
    assert!(above(&sim, level).abs() < 1.0);
    let mut p = Perception::default();
    sim.perceive_into(level.idx(), &mut p);
    let c = *p.get(r as u16).expect("the doll sees the rider");
    assert!(c.surface_n.dot(n) > 0.999, "perception has it on the ground: {}", c.surface_n);
    let flying = bc_sim::perception::Contact { surface_n: Vec3::ZERO, ..c };
    let stick_up = |c: &bc_sim::perception::Contact| {
        let mut ai = sim.suits.ai[level.idx()];
        let cmd = doll_drive(&p.me, Some(c), &mut ai, sim.tick(), &DOLL, frame(FrameId::Taurus));
        let [x, y, z] = cmd.thrust.map(|v| f32::from(v) / 127.0);
        (p.me.rot * Vec3::new(x, y, z)).dot(n)
    };
    assert!(stick_up(&c) > stick_up(&flying) + 0.5, "no push up off the ground's plane");
    // Hunting it, neither doll's stick ever drives it into Hermit, and the one that was level with
    // it ends over it.
    let mut hunted = false;
    for _ in 0..900 {
        hold(&mut sim);
        for &d in &[level, skimmer] {
            let f = sim.suits.flight[d.idx()];
            let (_, pr) = landmark_probe(&sim, 1, f.pos);
            if pr.dist - STANCE < 150.0 {
                let [x, y, z] = sim.suits.input[d.idx()].thrust.map(|v| f32::from(v) / 127.0);
                let into = (f.rot * Vec3::new(x, y, z)).dot(pr.normal);
                assert!(into > -0.02, "doll {} pushed into Hermit ({into}) at {}", d.idx(), sim.tick());
            }
            hunted |= sim.suits.ai[d.idx()].target == r as u16;
        }
    }
    assert!(hunted, "the dolls never went for the rider");
    assert!(above(&sim, level) > 60.0, "the doll level with it stayed level ({} m)", above(&sim, level));
}

#[test]
fn zero_never_predicts_a_grounded_contact_into_its_surface() {
    use bc_sim::perception::Perception;
    use bc_sim::zero::hypotheses::{Maneuver, accel, accel_on};
    use bc_sim::zero::local_oracle::{hypothesis_accels, own_maneuver_distribution};
    use bc_sim::zero::rollout::rollout;

    let mut sim = sector(2);
    // A Wing Zero on Hermit's flank, walking, and an OZ one 800 m over it with ZERO engaged.
    let rider = on_hermit(&mut sim, FrameId::WingZero, Vec3::new(0.3, 1.0, 0.2));
    let r = rider.idx();
    let (n, _) = ground(&sim, rider);
    let at = sim.suits.flight[r].pos;
    let watcher = suit(&mut sim, FrameId::WingZero, Faction::Oz, at + n * 800.0 + along(n) * 300.0, -n);
    for _ in 0..30 {
        let aim = (sim.suits.flight[watcher.idx()].pos - sim.suits.flight[r].pos).normalize();
        drive(&mut sim, rider, [0, 0, 127], GRIP, aim);
        let aim = (sim.suits.flight[r].pos - sim.suits.flight[watcher.idx()].pos).normalize();
        drive(&mut sim, watcher, [0; 3], ZERO | FLIGHT_ASSIST, aim);
        step(&mut sim);
    }
    assert_eq!(sim.footing(r), Footing::Grounded);
    let mut p = Perception::default();
    sim.perceive_into(watcher.idx(), &mut p);
    let c = *p.get(r as u16).expect("the watcher sees the rider");
    assert!(c.surface_n.dot(n) > 0.999);
    // Every future it's given stays out of the rock (the origin rides its stance over the ground).
    let height = |q: Vec3| landmark_probe(&sim, 1, q).1.dist;
    for (k, a) in hypothesis_accels(&c).into_iter().enumerate() {
        for q in rollout(c.pos, c.vel, a) {
            assert!(height(q) > 1.0, "{:?} puts it {} m under Hermit", Maneuver::from_index(k), -height(q));
        }
    }
    // Flying, its Down would have gone into it.
    let spec = frame(FrameId::WingZero);
    let down = rollout(c.pos, c.vel, accel(spec, c.rot, Maneuver::Down));
    assert!(down.iter().any(|&q| height(q) < 0.0), "(the flying hypotheses keep out of it anyway)");
    // And the rider, told how to break by its own System, is never told to break into the ground.
    assert_eq!(accel_on(spec, c.rot, c.surface_n, Maneuver::Down), Vec3::ZERO);
    sim.perceive_into(r, &mut p);
    assert!(p.me.surface_n.dot(n) > 0.999);
    let own = own_maneuver_distribution(&p.me, spec, p.contacts());
    assert!(own[Maneuver::Down as usize] < own[Maneuver::Coast as usize], "{own:?}");
}
