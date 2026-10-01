//! Under anime flight rules the owner's prediction keeps the boost gauge as the server does: it
//! drains only while boosting, stays dry while boost is leaned on, fills back up once it's let go,
//! and an empty gauge still flies. Seeded from any snapshot, the prediction has the server's
//! propellant, and so its mass, and so where the suit goes.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::{BOOST, FLIGHT_ASSIST, RCS_SHARP};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::field::Field;
use bc_sim::math::look_rotation;
use bc_sim::tuning::FlightRules;
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

const LEAD: u32 = 10;

fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

/// What the pilot does at tick `t`: boosts the low gauge dry and keeps leaning on boost, lets go
/// (it fills), turns hard on RCS, boosts again in bursts, brakes, and flies with flight assist off.
fn act(t: u32) -> InputCmd {
    let s = t as f32;
    let aim = Quat::from_rotation_y(s * 0.004) * Quat::from_rotation_x((s * 0.02).sin() * 0.3) * Vec3::Z;
    let (thrust, buttons) = match t {
        0..200 => ([0, 0, 127], FLIGHT_ASSIST | BOOST),
        200..260 => ([30, 0, 127], FLIGHT_ASSIST),
        260..320 => ([0, 60, 127], FLIGHT_ASSIST | RCS_SHARP),
        320..500 if (t / 20).is_multiple_of(2) => ([0, 0, 127], FLIGHT_ASSIST | BOOST),
        320..500 => ([-60, 0, 90], FLIGHT_ASSIST),
        500..560 => ([0, 0, 0], FLIGHT_ASSIST),
        _ => ([0, -40, 127], if (t / 15).is_multiple_of(2) { BOOST } else { 0 }),
    };
    let aim = if (260..320).contains(&t) { Quat::from_rotation_y(s * 0.05) * aim } else { aim };
    InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() }.quantized()
}

#[test]
fn the_boost_gauge_keeps_time_with_the_servers() {
    const TICKS: u32 = 700;
    let cfg =
        SimConfig { target_dolls: 0, field_rocks: 0, flight: FlightRules::Anime, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let start = Vec3::new(0.0, 9_000.0, 0.0);
    let id = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, start, look_rotation(Vec3::Z, Vec3::Y))
        .unwrap();
    let i = id.idx();
    sim.suits.flight[i].propellant = 150.0;
    let mut cmds = vec![InputCmd::default()];
    let mut owns = vec![over_the_wire(&sim.own_state(i))];
    let mut states = vec![sim.suits.flight[i]];
    for t in 1..=TICKS {
        let cmd = act(t);
        sim.set_input(id, cmd);
        sim.step();
        cmds.push(cmd);
        owns.push(over_the_wire(&sim.own_state(i)));
        states.push(sim.suits.flight[i]);
    }
    // The run did what it says: dry while boost was leaned on, full again after, never above.
    let tank = bc_sim::content::frame(FrameId::Leo).propellant_cap;
    let dry = states[..200].iter().filter(|s| s.propellant == 0.0).count();
    assert!(dry > 100, "dry for {dry} ticks of leaning on boost");
    assert!(states[260].propellant > 200.0, "it filled: {}", states[260].propellant);
    assert!(states.iter().all(|s| s.propellant <= tank));
    assert!(states[199].vel.length() > 150.0, "an empty gauge flies: {:?}", states[199].vel);
    let mut checked = 0;
    for seed in 1..=TICKS - LEAD {
        let mut history = InputHistory::default();
        for t in seed.saturating_sub(4)..=seed + LEAD {
            history.push(cmds[t as usize]);
        }
        let mut p = Predictor::default();
        p.set_field(Field::empty());
        p.set_rules(FlightRules::Anime);
        p.reconcile(seed, &owns[seed as usize], &history);
        for t in seed + 1..=seed + LEAD {
            p.advance(&cmds[t as usize], &history);
            let (want, got) = (&states[t as usize], &p.state);
            // To a hair: the snapshot's rotation is quantized, so flight assist's sums can differ
            // in the last bit (as they can under the real rules).
            assert!(
                (got.propellant - want.propellant).abs() < 0.01,
                "seeded at {seed}, tick {t}: {} kg predicted, {} kg on the server",
                got.propellant,
                want.propellant
            );
            let (dp, dv) = ((got.pos - want.pos).length(), (got.vel - want.vel).length());
            assert!(dp < 0.01 && dv < 0.05, "seeded at {seed}, tick {t}: off by {dp} m, {dv} m/s");
        }
        checked += 1;
    }
    assert!(checked > 600);
    // Predicted by the real rules instead, it's wrong: the rules matter, and the Welcome says which.
    let mut history = InputHistory::default();
    for t in 196..=215 {
        history.push(cmds[t as usize]);
    }
    let mut p = Predictor::default();
    p.set_field(Field::empty());
    p.reconcile(200, &owns[200], &history);
    for t in 201..=210 {
        p.advance(&cmds[t as usize], &history);
    }
    assert!((p.state.propellant - states[210].propellant).abs() > 1.0);
}
