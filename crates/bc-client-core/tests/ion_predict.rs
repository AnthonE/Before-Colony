//! Under the real rules an ion drive carries the first of the thrust on the reactor's power alone,
//! and a dry tank still crawls on it; a scrammed reactor stops it. The owner's prediction burns,
//! crawls and stops as the server does: a Leo with the drive, an extended tank, refined propellant
//! and a damaged reactor (half the drive), burnt dry, crawling, braking and turning, scrammed while
//! it crawls.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::content::modules::MOUNTS;
use bc_sim::content::systems::{DAMAGED, SCRAM_TICKS};
use bc_sim::content::{Grade, ModuleKind, Modules, System, Systems};
use bc_sim::field::Field;
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

const LEAD: u32 = 10;
/// The tick the reactor scrams (after the step): a seed before it can't know.
const SCRAM_AT: u32 = 420;

fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

/// What the pilot does at tick `t`: burns hard until the tank is dry, crawls ahead on the drive,
/// turns, brakes on flight assist, crawls with gentle sideways thrust (all the drive's), and flies
/// on through the scram.
fn act(t: u32) -> InputCmd {
    let s = t as f32;
    let aim = Quat::from_rotation_y(s * 0.003) * Quat::from_rotation_x((s * 0.02).sin() * 0.2) * Vec3::Z;
    let (thrust, buttons) = match t {
        0..250 => ([0, 0, 127], 0),
        250..330 => ([40, -20, 100], 0),
        330..400 => ([0, 0, 0], FLIGHT_ASSIST),
        _ => ([12, 0, 20], 0),
    };
    let aim = if (250..330).contains(&t) { Quat::from_rotation_y(s * 0.04) * aim } else { aim };
    InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() }.quantized()
}

#[test]
fn the_ion_drive_keeps_time_with_the_servers() {
    const TICKS: u32 = 560;
    let cfg = SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let start = Vec3::new(0.0, 9_000.0, 0.0);
    let id = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, start, look_rotation(Vec3::Z, Vec3::Y))
        .unwrap();
    let i = id.idx();
    let mut m = Modules::NONE;
    for k in [ModuleKind::IonDrive, ModuleKind::ExtendedTank] {
        let slot = (0..MOUNTS.len()).find(|s| MOUNTS[*s] == k.part() && m.get(*s).is_none()).unwrap();
        m.set(slot, Some(k));
    }
    sim.suits.modules[i] = m;
    sim.suits.systems[i] = Systems::OK.with(System::Reactor, DAMAGED);
    sim.suits.grade[i] = Grade::Refined;
    sim.suits.retune(i);
    assert_eq!(sim.tuning(i).ion, 0.5, "half the drive on a damaged reactor");
    sim.suits.flight[i].propellant = 200.0;
    let mut cmds = vec![InputCmd::default()];
    let mut owns = vec![over_the_wire(&sim.own_state(i))];
    let mut states = vec![sim.suits.flight[i]];
    for t in 1..=TICKS {
        let cmd = act(t);
        sim.set_input(id, cmd);
        sim.step();
        if t == SCRAM_AT {
            sim.suits.status[i].scram = SCRAM_TICKS;
        }
        cmds.push(cmd);
        owns.push(over_the_wire(&sim.own_state(i)));
        states.push(sim.suits.flight[i]);
    }
    // The run did what it says: dry well before the crawl, moving on the drive alone after it, and
    // not while the reactor's scrammed.
    let dry_from = states.iter().position(|s| s.propellant == 0.0).expect("burnt dry");
    assert!(dry_from < 250, "dry at {dry_from}");
    let (a, b) = (&states[405], &states[SCRAM_AT as usize]);
    assert!((b.vel - a.vel).length() > 0.05, "it crawls: {} → {}", a.vel, b.vel);
    let (a, b) = (&states[SCRAM_AT as usize + 5], &states[SCRAM_AT as usize + 60]);
    assert!((b.vel - a.vel).length() < 1e-3, "scrammed, it coasts: {} → {}", a.vel, b.vel);
    let mut checked = 0;
    for seed in 1..=TICKS - LEAD {
        if (SCRAM_AT - LEAD..SCRAM_AT).contains(&seed) {
            continue;
        }
        let mut history = InputHistory::default();
        for t in seed.saturating_sub(4)..=seed + LEAD {
            history.push(cmds[t as usize]);
        }
        let mut p = Predictor::default();
        p.set_field(Field::empty());
        p.reconcile(seed, &owns[seed as usize], &history);
        for t in seed + 1..=seed + LEAD {
            p.advance(&cmds[t as usize], &history);
            let (want, got) = (&states[t as usize], &p.state);
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
    assert!(checked > 500);
}
