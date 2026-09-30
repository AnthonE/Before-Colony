//! The landmarks are solid, and one ordered test says what stops a shot: free and sleeping suits
//! fetch up against MO-II and Hermit as against rocks; chunks bounce off them relative to their
//! moving surfaces; and shots, missiles and flame meet whatever comes first along their path, a
//! suit, a rock, a landmark or the colony, so a suit skimming the hull is hit and one behind it
//! is not.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

mod common;

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY};
use bc_proto::events::{BurstCause, Event};
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, NO_SLOT, PilotKind, Segment, WeaponKind};
use bc_sim::bodies::{TRACE_EPS, landmark_pose};
use bc_sim::chunks::{self, Motion, segment_pos};
use bc_sim::content::frame;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::BOUNCE;
use bc_sim::field::SUIT_CLEARANCE;
use bc_sim::math::look_rotation;
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS, colony_sweep};
use bc_sim::{Sim, SimConfig, SuitId};
use common::{landmark_probe, landmark_surface};
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
