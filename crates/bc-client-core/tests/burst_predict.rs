//! The burst step, flown against the real simulation the way the browser flies it: a pilot
//! double-taps its way about, free and then locked on to a Taurus (where a step goes along the
//! fight's axes), each step 36 m/s along the keys. All the while the owner's prediction, seeded
//! from snapshots a few ticks old, flies exactly what the server flies: the step's state travels in
//! the own snapshot, and its press in the command.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::lockon::{self, Lock};
use bc_client_core::world::World;
use bc_client_core::{InputContext, InputHistory, Predictor};
use bc_proto::buttons::{BURST, FLIGHT_ASSIST};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::field::Field;
use bc_sim::flight::{BURST_COOLDOWN, BURST_TICKS};
use bc_sim::math::look_rotation;
use bc_sim::tuning::FlightRules;
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

/// How old each snapshot is when the client hears it, ticks.
const LAG: u32 = 5;
/// A step every this many ticks.
const EVERY: u32 = 50;

#[test]
fn steps_free_and_locked_on_are_predicted_exactly() {
    let cfg =
        SimConfig { target_dolls: 0, field_rocks: 0, flight: FlightRules::Anime, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let face = look_rotation(Vec3::Z, Vec3::Y);
    let me =
        sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 2_000.0, 0.0), face);
    let foe =
        sim.spawn_at(FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(300.0, 2_000.0, 600.0), face);
    let (me, foe) = (me.unwrap(), foe.unwrap());
    let (i, j) = (me.idx(), foe.idx());

    let (mut world, mut predict, mut history) =
        (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
    predict.set_field(Field::empty());
    predict.set_rules(FlightRules::Anime);
    let mut lock = Lock::default();
    let mut snapshots: Vec<(OwnState, bc_proto::EntityState)> = Vec::new();
    let mut worst = 0.0f32;
    let (mut free_steps, mut locked_steps) = (0, 0);
    // Right, left, up, back, forward, down: each key in turn.
    let keys: [[i8; 3]; 6] =
        [[127, 0, 0], [-127, 0, 0], [0, 127, 0], [0, 0, -127], [0, 0, 127], [0, -127, 0]];
    for t in 1..=900u32 {
        snapshots.push((over_the_wire(&sim.own_state(i)), sim.entity_state(j, i)));
        if let Some(k) = (t - 1).checked_sub(LAG) {
            let (own, them) = snapshots[k as usize];
            world.apply(k, Some(own), None, &[], &[them]);
            predict.reconcile(k, &own, &history);
            if t > LAG + 2 {
                worst = worst.max(predict.last_error);
            }
        }
        let to = sim.suits.flight[j].pos - sim.suits.flight[i].pos;
        if t == 450 {
            assert!(
                lock.tap(&world, sim.suits.flight[i].pos, to.normalize(), f64::from(t - 1)),
                "nothing to lock"
            );
        }
        // A double tap's second press: BURST held for two ticks, with the key.
        let n = t / EVERY;
        let pressing = t % EVERY < 2 && t > EVERY;
        let (thrust, buttons) =
            if pressing { (keys[n as usize % 6], FLIGHT_ASSIST | BURST) } else { ([0; 3], FLIGHT_ASSIST) };
        let cmd = InputCmd { aim: to.normalize(), thrust, buttons, ..InputCmd::default() };
        let ctx = InputContext {
            tick: t,
            view_tick: f64::from(t.saturating_sub(LAG + 1)),
            resolve_tick: f64::from(t.saturating_sub(LAG + 1)),
            now: 0.0,
            world: &world,
            predict: &predict,
        };
        let cmd =
            InputCmd { tick: t, view_tick_q4: t << 4, ..lockon::shape(cmd, &mut lock, &ctx) }.quantized();
        history.push(cmd);
        // Flown ahead as the client flies it, to be checked against the server's word of it.
        predict.advance(&cmd, &history);
        sim.set_input(me, cmd);
        let before = sim.suits.flight[i].vel;
        sim.step();
        let b = sim.suits.flight[i].burst;
        if b.left == BURST_TICKS - 1 {
            // A step began: its press's direction, in the suit's axes or the fight's.
            assert_eq!(b.cooldown, BURST_COOLDOWN);
            if cmd.lockon.is_some() {
                locked_steps += 1;
            } else {
                free_steps += 1;
            }
            let dv = sim.suits.flight[i].vel - before;
            assert!(dv.length() > 3.0, "tick {t}: the step's first tick gave {dv}");
        }
    }
    println!("{free_steps} steps free, {locked_steps} locked on; prediction ≤ {worst:.5} m off");
    assert!(free_steps >= 6 && locked_steps >= 6, "{free_steps} free, {locked_steps} locked on");
    assert!(worst < 0.01, "predicted {worst} m off the server");
}
