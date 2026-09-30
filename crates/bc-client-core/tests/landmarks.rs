//! The owner's prediction fetches up against MO-II and Hermit where the server does: seeded from
//! any snapshot on the way in, at the contact or after it, it replays the pilot's commands into
//! the same stop against the same (moving) surface.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::bodies::{Bodies, Body};
use bc_sim::field::SUIT_CLEARANCE;
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig};
use glam::Vec3;

/// Ticks flown.
const RUN: u32 = 240;
/// How far ahead of each snapshot the prediction flies.
const LEAD: u32 = 10;

/// The own state as the client decodes it.
fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

/// How far a suit at `p` is from landmark `k`'s surface at tick `t`.
fn clear_of(sim: &Sim, k: u8, p: Vec3, t: u32) -> f32 {
    let bodies = Bodies::at(&sim.field, sim.landmarks(), t);
    let pose = bodies.pose(Body::Landmark(k)).unwrap();
    bodies.shape(Body::Landmark(k)).unwrap().probe(pose.to_local(p)).dist
}

#[test]
fn the_prediction_stops_at_a_landmark_where_the_server_does() {
    for (k, dir) in
        [(0u8, Vec3::Y), (0, Vec3::new(1.0, 0.3, 0.2)), (1, Vec3::X), (1, Vec3::new(-0.2, 0.4, -1.0))]
    {
        let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
        let bodies = Bodies::at(&sim.field, sim.landmarks(), 0);
        let pose = bodies.pose(Body::Landmark(k)).unwrap();
        let (p, n) = bodies.surface_along(Body::Landmark(k), dir).unwrap();
        let (p, n) = (pose.to_world(p), pose.rot * n);
        let id = sim
            .spawn_at(
                FrameId::Leo,
                Faction::Colonies,
                PilotKind::Human,
                p + n * 150.0,
                look_rotation(-n, Vec3::Y),
            )
            .unwrap();
        let i = id.idx();
        // Thrust in for a second and a half, then coast into it (no flight assist).
        let mut cmds = vec![InputCmd::default()];
        let mut owns = vec![over_the_wire(&sim.own_state(i))];
        let mut poses = vec![sim.suits.flight[i].pos];
        let mut nearest = f32::INFINITY;
        for t in 1..=RUN {
            let thrust = if t <= 45 { 127 } else { 0 };
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: -n,
                thrust: [0, 0, thrust],
                ..InputCmd::default()
            }
            .quantized();
            sim.set_input(id, cmd);
            sim.step();
            cmds.push(cmd);
            owns.push(over_the_wire(&sim.own_state(i)));
            poses.push(sim.suits.flight[i].pos);
            nearest = nearest.min(clear_of(&sim, k, sim.suits.flight[i].pos, t));
        }
        assert!(nearest < SUIT_CLEARANCE + 0.05, "landmark {k} along {dir}: never nearer than {nearest} m");
        let mut worst: f32 = 0.0;
        for seed in 1..=RUN - LEAD {
            let mut history = InputHistory::default();
            for t in seed.saturating_sub(4)..=seed + LEAD {
                history.push(cmds[t as usize]);
            }
            let mut pr = Predictor::default();
            pr.reconcile(seed, &owns[seed as usize], &history);
            for t in seed + 1..=seed + LEAD {
                pr.advance(&cmds[t as usize], &history);
                let dp = (pr.state.pos - poses[t as usize]).length();
                assert!(dp < 0.01, "landmark {k} along {dir}, seeded at {seed}: off by {dp} m at {t}");
                assert!(
                    clear_of(&sim, k, pr.state.pos, t) >= SUIT_CLEARANCE - 0.05,
                    "landmark {k} along {dir}, seeded at {seed}: predicted into it at {t}"
                );
                worst = worst.max(dp);
            }
        }
        println!("landmark {k} along {dir}: worst {worst} m");
    }
}
