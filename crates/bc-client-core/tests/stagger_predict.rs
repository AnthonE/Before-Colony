//! Stagger, flown against the real simulation the way the browser flies it: a pilot flies about a
//! Taurus, firing, while blows stagger their suit again and again (a Heavyarms pressing for its Full
//! Open all the while, staggered or not). The owner's prediction, seeded from snapshots a few ticks
//! old, flies exactly what the server flies: the stagger's ticks travel in the own snapshot, and
//! while they run the suit tumbles with its thrust cut and its weapons and special down.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::world::World;
use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, SPECIAL};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, Part, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
    WeaponKind,
};
use bc_sim::content::stagger::{STAGGER_TICKS, stability};
use bc_sim::field::Field;
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
/// A blow that staggers every this many ticks.
const EVERY: u32 = 70;

/// Flies `frame` for 900 ticks under `rules`; returns how many times it was staggered, how many of
/// those it pressed SPECIAL through, and the prediction's worst miss, m.
fn fly(frame: FrameId, rules: FlightRules) -> (u32, u32, f32) {
    let cfg = SimConfig { target_dolls: 0, field_rocks: 0, flight: rules, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let face = look_rotation(Vec3::Z, Vec3::Y);
    let me = sim.spawn_at(frame, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 2_000.0, 0.0), face);
    let foe =
        sim.spawn_at(FrameId::Taurus, Faction::Oz, PilotKind::Human, Vec3::new(300.0, 2_000.0, 900.0), face);
    let (me, foe) = (me.unwrap(), foe.unwrap());
    let (i, j) = (me.idx(), foe.idx());
    // Torsos nothing here gets through: this is about the push, not the damage.
    sim.suits.part_hp[i][Part::Torso as usize] = 1.0e6;
    sim.suits.part_hp[j][Part::Torso as usize] = 1.0e6;

    let (mut world, mut predict, mut history) =
        (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
    predict.set_field(Field::empty());
    predict.set_rules(rules);
    let mut snapshots: Vec<(OwnState, bc_proto::EntityState)> = Vec::new();
    let (mut worst, mut staggers, mut pressed_through) = (0.0f32, 0, 0);
    // Thrust about: forward, right, up, back, left, down.
    let keys: [[i8; 3]; 6] =
        [[0, 0, 127], [127, 0, 0], [0, 127, 0], [0, 0, -127], [-127, 0, 0], [0, -127, 0]];
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
        // The blow, landing in this tick's damage (after its flight).
        if t % EVERY == EVERY / 2 {
            sim.strike(i, Part::Torso, stability(frame) + 1.0, j, WeaponKind::BeamRifle);
        }
        let was = sim.staggered(i);
        // Aim off the foe and back, firing in bursts, the special pressed every so often (it
        // lands both staggered and steady).
        let to = sim.suits.flight[j].pos - sim.suits.flight[i].pos;
        let wobble = Vec3::new((t as f32 * 0.05).sin() * 0.4, (t as f32 * 0.03).cos() * 0.3, 0.0);
        let mut buttons = FLIGHT_ASSIST;
        if t % 40 < 25 {
            buttons |= FIRE_PRIMARY | FIRE_SECONDARY;
        }
        if t % 23 == 0 {
            buttons |= SPECIAL;
            if was {
                pressed_through += 1;
            }
        }
        let thrust = keys[(t / 25) as usize % keys.len()];
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t.saturating_sub(LAG + 1) << 4,
            aim: (to.normalize() + wobble).normalize(),
            thrust,
            buttons,
            ..InputCmd::default()
        }
        .quantized();
        history.push(cmd);
        // Flown ahead as the client flies it, to be checked against the server's word of it.
        predict.advance(&cmd, &history);
        sim.set_input(me, cmd);
        sim.step();
        if !was && sim.staggered(i) {
            staggers += 1;
            assert_eq!(sim.own_state(i).stagger, STAGGER_TICKS - 1);
        }
    }
    (staggers, pressed_through, worst)
}

#[test]
fn a_staggered_leo_is_predicted_exactly() {
    for rules in [FlightRules::Anime, FlightRules::Real] {
        let (staggers, _, worst) = fly(FrameId::Leo, rules);
        println!("{rules:?}: {staggers} staggers; prediction ≤ {worst:.5} m off");
        assert!(staggers >= 10, "{staggers} staggers");
        assert!(worst < 0.01, "{rules:?}: predicted {worst} m off the server");
    }
}

#[test]
fn a_staggered_heavyarms_pressing_for_its_full_open_is_predicted_exactly() {
    let (staggers, pressed, worst) = fly(FrameId::Heavyarms, FlightRules::Anime);
    println!("{staggers} staggers, SPECIAL pressed {pressed} times staggered; prediction ≤ {worst:.5} m off");
    assert!(staggers >= 10 && pressed >= 3, "{staggers} staggers, {pressed} presses staggered");
    assert!(worst < 0.01, "predicted {worst} m off the server");
}
