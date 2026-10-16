//! Shots go where the pilot's crosshair is (`bc_sim::aim`): a command that says how far along the
//! aim the crosshair's point is (`InputCmd::converge`) fires its gun at that point, the shot's path
//! crossing it however the suit drifts; one that doesn't fires along the aim, as every shot once did.
//! (The machine cannon's rounds, from the Blast Hall's gantry: `interior.rs`,
//! `a_crosshair_on_a_target_scores_from_the_pilots_eye`.)
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::content::frame;
use bc_sim::sim::Loadout;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn space() -> Sim {
    Sim::new(SimConfig { target_drones: 0, field_rocks: 0, landmarks: 0, ..SimConfig::default() })
}

/// The shots `id` fires over `ticks` with its trigger(s) pulled every third tick, aiming `aim` with
/// the crosshair's point `range` m along it (none: 0), drifting at `vel`: each one's origin and
/// velocity, with the suit's pose (position, rotation) the shot left it at.
fn fire(
    sim: &mut Sim,
    id: SuitId,
    aim: Vec3,
    range: Option<f32>,
    vel: Vec3,
    buttons: u16,
) -> Vec<(Vec3, Vec3, Vec3, glam::Quat)> {
    let i = id.idx();
    let mut out = Vec::new();
    for n in 0..90 {
        let t = sim.next_tick();
        sim.suits.flight[i].vel = vel;
        let pull = if n % 3 == 0 { buttons } else { 0 };
        let converge = range.map_or(0, bc_sim::aim::encode);
        sim.set_input(
            id,
            InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons: pull, converge, ..InputCmd::default() },
        );
        let from = sim.events.next_seq();
        sim.step();
        let f = sim.suits.flight[i];
        for s in from..sim.events.next_seq() {
            if let Some(Event::BeamSpawn { shooter, origin, velocity, .. }) = sim.events.get(s).copied()
                && shooter == i as u16
            {
                out.push((origin, velocity, f.pos, f.rot));
            }
        }
    }
    out
}

/// How far the line from `origin` along `vel` passes from `p`, m.
fn miss(origin: Vec3, vel: Vec3, p: Vec3) -> f32 {
    let d = vel.normalize();
    let rel = p - origin;
    (rel - d * rel.dot(d)).length()
}

#[test]
fn a_shot_crosses_the_crosshairs_point_near_and_far_and_strafing() {
    let spec = frame(FrameId::Argo);
    for (range, drift) in [
        (30.0, Vec3::ZERO),
        (100.0, Vec3::ZERO),
        (1_000.0, Vec3::ZERO),
        (100.0, Vec3::new(150.0, 0.0, 0.0)),
        (1_000.0, Vec3::new(-60.0, 120.0, 40.0)),
    ] {
        let mut sim = space();
        let id = sim
            .launch(FrameId::Argo, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Argo))
            .unwrap();
        let rot = sim.suits.flight[id.idx()].rot;
        // A little off the nose, within the arm's reach.
        let aim = (rot * Vec3::new(0.1, -0.05, 1.0)).normalize();
        let shots = fire(&mut sim, id, aim, Some(range), drift, FIRE_PRIMARY);
        assert!(!shots.is_empty(), "it fired at {range} m");
        for (origin, vel, pos, rot) in shots {
            let p = pos + rot * spec.eye() + aim * range;
            let off = miss(origin, vel, p);
            // (The rifle scatters its shots a little: within its spread at that range.)
            let spread = bc_sim::content::weapon(bc_proto::WeaponKind::BeamRifle).spread;
            let allow = 0.02 + (p - origin).length() * spread.tan() * 1.01;
            assert!(
                off <= allow,
                "at {range} m drifting {drift}: misses the crosshair's point by {off:.3} m (allowed {allow:.3})"
            );
        }
    }
}

#[test]
fn a_command_without_a_range_fires_along_the_aim() {
    let mut sim = space();
    let id = sim
        .launch(FrameId::Argo, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Argo))
        .unwrap();
    let rot = sim.suits.flight[id.idx()].rot;
    let aim = (rot * Vec3::new(0.05, 0.02, 1.0)).normalize();
    let spread = bc_sim::content::weapon(bc_proto::WeaponKind::BeamRifle).spread;
    for (_, vel, ..) in fire(&mut sim, id, aim, None, Vec3::ZERO, FIRE_PRIMARY) {
        assert!(vel.normalize().angle_between(aim) <= spread * 1.01 + 1e-4);
    }
}
