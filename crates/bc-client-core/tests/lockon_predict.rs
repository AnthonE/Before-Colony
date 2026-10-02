//! Lock-on, flown against the real simulation the way the browser flies it: the pilot locks a
//! Mobile Doll's frame coasting away faster than flight assist's cruise, holds W, and is carried in
//! to just outside a blade's reach, settled onto its level, holding station on it; then holds A and
//! circles it at that range. All the while the owner's prediction, seeded from every snapshot, flies
//! exactly what the server flies: the lock-on travels in the command.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::lockon::{self, Lock};
use bc_client_core::world::World;
use bc_client_core::{InputContext, InputHistory, Predictor};
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::field::Field;
use bc_sim::math::look_rotation;
use bc_sim::tuning::FlightRules;
use bc_sim::world::{colony_altitude, colony_up};
use bc_sim::{Sim, SimConfig};
use glam::Vec3;

fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

#[test]
fn locked_on_a_pilot_closes_settles_and_circles_and_predicts_it_exactly() {
    let cfg =
        SimConfig { target_dolls: 0, field_rocks: 0, flight: FlightRules::Anime, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let face = look_rotation(Vec3::Z, Vec3::Y);
    let me =
        sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 2_000.0, 0.0), face);
    let foe =
        sim.spawn_at(FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(0.0, 2_250.0, 1_500.0), face);
    let (me, foe) = (me.unwrap(), foe.unwrap());
    let (i, j) = (me.idx(), foe.idx());
    // It coasts away and a little across, faster than a Leo's flight assist cruises (220 m/s).
    let pace = Vec3::new(60.0, 0.0, 260.0);
    sim.suits.flight[j].vel = pace;
    let stop = lockon::stop_range(FrameId::Leo);

    let (mut world, mut predict, mut history) =
        (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
    predict.set_field(Field::empty());
    predict.set_rules(FlightRules::Anime);
    let mut lock = Lock::default();
    let mut worst = 0.0f32;
    let (mut closed_at, mut circled) = (None, Vec::new());
    for t in 1..=1_800u32 {
        let own = over_the_wire(&sim.own_state(i));
        world.apply(t - 1, Some(own), None, &[], &[sim.entity_state(j, i)]);
        predict.reconcile(t - 1, &own, &history);
        if t > 2 {
            worst = worst.max(predict.last_error);
        }
        let to = sim.suits.flight[j].pos - sim.suits.flight[i].pos;
        if t == 1 {
            assert!(lock.tap(&world, sim.suits.flight[i].pos, Vec3::Z, 0.0), "nothing to lock");
            assert_eq!(lock.slot(), Some(foe.idx() as u16));
        }
        // W to close in, then A to circle; the mouse on the target all the while.
        let keys = if t <= 1_200 { [0, 0, 127] } else { [-127, 0, 0] };
        let cmd =
            InputCmd { aim: to.normalize(), thrust: keys, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
        let ctx = InputContext {
            tick: t,
            view_tick: f64::from(t - 1),
            resolve_tick: f64::from(t - 1),
            now: 0.0,
            world: &world,
            predict: &predict,
        };
        let cmd =
            InputCmd { tick: t, view_tick_q4: t << 4, ..lockon::shape(cmd, &mut lock, &ctx) }.quantized();
        assert!(cmd.lockon.is_some(), "tick {t}: not flying locked on");
        history.push(cmd);
        sim.set_input(me, cmd);
        sim.step();

        let (a, b) = (sim.suits.flight[i], sim.suits.flight[j]);
        let up = colony_up(a.pos);
        let r = b.pos - a.pos;
        // Its level is its altitude over the colony; the range, along the ground.
        let h = colony_altitude(b.pos) - colony_altitude(a.pos);
        let flat = (r.length_squared() - h * h).max(0.0).sqrt();
        if t <= 1_200 && closed_at.is_none() && (flat - stop).abs() < 3.0 && h.abs() < 3.0 {
            closed_at = Some(t);
        }
        if t == 1_200 {
            // In, level with it, and keeping pace with it.
            assert!((flat - stop).abs() < 3.0, "{flat} m off, not {stop}");
            assert!(h.abs() < 3.0, "{h} m off its level");
            assert!((a.vel - b.vel).length() < 2.0, "{} m/s apart", (a.vel - b.vel).length());
            assert!((a.rot * Vec3::Y).dot(up) > 0.98, "not level with the fight");
        }
        if t > 1_350 {
            circled.push(flat);
        }
    }
    assert!(closed_at.is_some(), "never closed in");
    // Circling held the range it started at, within 5%, and went somewhere.
    let (lo, hi) = circled.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &d| (lo.min(d), hi.max(d)));
    assert!(hi / lo < 1.05, "the circle wandered from {lo} to {hi} m");
    // The owner's prediction flew what the server flew.
    assert!(worst < 0.01, "predicted {worst} m off the server");
}
