//! Lag compensation: a shot resolves against the world the shooter was looking at.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::math::look_rotation;
use bc_sim::zero::fire_control::intercept;
use bc_sim::{DT, Sim, SimConfig};
use glam::Vec3;

/// A Leo fires once at a target crossing at 200 m/s, aiming from a view `view_age` ticks old.
/// Returns whether the shot hit.
fn duel(view_age: u32, compensate: bool) -> bool {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let shooter = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 1_000.0, 0.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let target = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Oz,
            PilotKind::Human,
            Vec3::new(-120.0, 1_000.0, 1_000.0),
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    sim.suits.flight[target.idx()].vel = Vec3::new(200.0, 0.0, 0.0);
    let hold = |sim: &mut Sim| {
        let t = sim.next_tick();
        let aim_t = sim.suits.aim[target.idx()];
        sim.set_input(target, InputCmd { tick: t, view_tick_q4: t << 4, aim: aim_t, ..InputCmd::default() });
        sim.set_input(
            shooter,
            InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, ..InputCmd::default() },
        );
    };
    // The test keeps its own track of the target (the history ring only holds 16 ticks).
    let mut track = Vec::new();
    for _ in 0..30 {
        hold(&mut sim);
        sim.step();
        track.push((sim.tick(), sim.suits.flight[target.idx()].pos));
    }
    // What the shooter's client saw: the target `view_age` ticks ago.
    let now = sim.tick();
    let view = now - view_age;
    let seen_pos = track.iter().find(|(t, _)| *t == view).unwrap().1;
    let seen_vel = Vec3::new(200.0, 0.0, 0.0);
    let s = &sim.suits.flight[shooter.idx()];
    let muzzle = s.pos + s.rot * Vec3::new(3.4, 0.6, 3.0);
    // The client leads from what it saw, advanced to the command's tick (+1): it fires "now".
    let sol = intercept(
        muzzle,
        s.vel,
        4_000.0,
        seen_pos + Vec3::new(0.0, 2.5, 0.0) + seen_vel * DT,
        seen_vel,
        Vec3::ZERO,
    )
    .unwrap();
    let t = sim.next_tick();
    let view_q4 = if compensate { (view + 1) << 4 } else { t << 4 };
    sim.set_input(
        shooter,
        InputCmd {
            tick: t,
            view_tick_q4: view_q4,
            aim: sol.dir,
            buttons: FIRE_PRIMARY,
            ..InputCmd::default()
        },
    );
    let from = sim.events.next_seq();
    sim.step();
    for _ in 0..30 {
        hold(&mut sim);
        sim.step();
    }
    (from..sim.events.next_seq())
        .filter_map(|q| sim.events.get(q).copied())
        .any(|e| matches!(e, Event::Hit { target: tg, .. } if tg as usize == target.idx()))
}

#[test]
fn rewound_shot_hits_what_the_pilot_saw() {
    assert!(duel(6, true), "with lag compensation the shot must land");
    assert!(!duel(6, false), "without it, a 6-tick-old view misses a 200 m/s crosser");
}

#[test]
fn rewind_is_clamped() {
    // A 20-tick-old view is only honoured for 8 ticks: the remaining 12 ticks × 6.7 m miss.
    assert!(!duel(20, true));
    assert!(duel(8, true));
}
