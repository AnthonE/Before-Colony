//! Shooter controls, flown against the real simulation the way the browser flies them: a pilot in
//! a suit left rolled over and facing away looks up and off to the side, and holds W. The suit
//! comes round onto the aim on its RCS, rolls level with the colony's up, and flies up the
//! crosshair at its cruise; then, brought to rest with X each time, D strafes it square to the aim
//! along the ground, and Space lifts it straight up. All the while the owner's prediction, seeded from every snapshot, flies exactly
//! what the server flies: the view's up travels in the command.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::upright;
use bc_client_core::world::World;
use bc_client_core::{InputContext, InputHistory, Predictor};
use bc_proto::buttons::{BRAKE, FLIGHT_ASSIST, RCS_SHARP};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::config::DT;
use bc_sim::content::frame;
use bc_sim::field::Field;
use bc_sim::math::look_rotation;
use bc_sim::tuning::FlightRules;
use bc_sim::world::colony_up;
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

#[test]
fn shooter_controls_fly_where_the_pilot_looks_upright_and_predict_it_exactly() {
    let cfg =
        SimConfig { target_drones: 0, field_rocks: 0, flight: FlightRules::Anime, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let start = Vec3::new(0.0, 2_000.0, 0.0);
    let ground = colony_up(start);
    // Facing away from where the pilot will look, rolled 70° off the colony's up.
    let away = ground.any_orthonormal_vector();
    let face = Quat::from_axis_angle(away, 1.2) * look_rotation(away, ground);
    let me = sim.spawn_at(FrameId::Argo, Faction::Colonies, PilotKind::Human, start, face).unwrap();
    let i = me.idx();
    let spec = frame(FrameId::Argo);
    let propellant = sim.suits.flight[i].propellant;

    // The pilot looks a quarter turn round from the suit's nose and 30° up off the horizon.
    let flat = ground.cross(away).normalize();
    let aim = (flat * 30f32.to_radians().cos() + ground * 30f32.to_radians().sin()).normalize();

    let (mut world, mut predict, mut history) =
        (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
    predict.set_field(Field::empty());
    predict.set_rules(FlightRules::Anime);
    // The view's up starts as the suit's own, and levels out onto the ground's frame by frame.
    let mut up = face * Vec3::Y;
    let mut worst = 0.0f32;
    let (mut on_aim, mut level) = (None, None);
    let mut rcs_ticks = 0;
    for t in 1..=1_770u32 {
        let own = over_the_wire(&sim.own_state(i));
        world.apply(t - 1, Some(own), None, &[], &[]);
        predict.reconcile(t - 1, &own, &history);
        if t > 2 {
            worst = worst.max(predict.last_error);
        }
        up = upright::level(up, upright::ground(&predict, t, false), spec.roll_rate, DT);
        // W; brake; D; brake; Space.
        let (keys, brake) = match t {
            ..=360 => ([0, 0, 127], 0),
            361..=780 => ([0; 3], BRAKE),
            781..=900 => ([127, 0, 0], 0),
            901..=1_320 => ([0; 3], BRAKE),
            _ => ([0, 127, 0], 0),
        };
        let cmd = InputCmd {
            aim: upright::keep(aim, up),
            thrust: keys,
            buttons: FLIGHT_ASSIST | brake,
            ..InputCmd::default()
        };
        let ctx = InputContext {
            tick: t,
            view_tick: f64::from(t - 1),
            resolve_tick: f64::from(t - 1),
            now: 0.0,
            world: &world,
            predict: &predict,
        };
        let cmd = InputCmd { tick: t, view_tick_q4: t << 4, ..upright::shape(cmd, up, &ctx) }.quantized();
        assert!(cmd.lockon.is_some(), "tick {t}: not flying upright");
        if cmd.pressed(RCS_SHARP) {
            rcs_ticks += 1;
        }
        history.push(cmd);
        predict.advance(&cmd, &history);
        sim.set_input(me, cmd);
        sim.step();

        let f = sim.suits.flight[i];
        let ground = colony_up(f.pos);
        let nose = f.rot * Vec3::Z;
        if on_aim.is_none() && nose.angle_between(aim) < 3f32.to_radians() {
            on_aim = Some(t);
        }
        // Level: its wings (its right) flat on the ground, whatever its pitch.
        let wings = (f.rot * Vec3::X).dot(ground).abs();
        if level.is_none() && wings < 0.02 {
            level = Some(t);
        }
        let flat = (aim - ground * aim.dot(ground)).normalize();
        match t {
            360 => {
                // Up the crosshair at the cruise, not along wherever the suit's nose was.
                assert!(
                    f.vel.angle_between(aim) < 0.02,
                    "{}° off the aim",
                    f.vel.angle_between(aim).to_degrees()
                );
                assert!((f.vel.length() - spec.fa_speed).abs() < 3.0, "{} m/s", f.vel.length());
                assert!(wings < 0.02, "not level: {wings}");
            }
            780 | 1_320 => assert!(f.vel.length() < 0.5, "still at {} m/s", f.vel.length()),
            900 => {
                // Strafing: square to the aim, along the ground.
                let right = ground.cross(flat);
                assert!(
                    f.vel.angle_between(right) < 0.03,
                    "{}° off the strafe",
                    f.vel.angle_between(right).to_degrees()
                );
                assert!(f.vel.dot(ground).abs() < 3.0, "{} m/s up or down", f.vel.dot(ground));
            }
            1_770 => {
                // (The thrusters don't hold its heading on the way: the main thrusters bring it to
                // the share of the climb along its pitched-up nose before the side ones do theirs.)
                assert!(
                    f.vel.angle_between(ground) < 0.03,
                    "{}° off straight up",
                    f.vel.angle_between(ground).to_degrees()
                );
            }
            _ => {}
        }
    }
    // A quarter turn and a climb: on the RCS, about a second and a half (on limb trim alone, an
    // Argo's would take nearer three).
    let on_aim = on_aim.expect("never came round onto the aim");
    assert!(on_aim < 60, "on the aim after {on_aim} ticks");
    assert!(level.is_some_and(|t| t < 150), "levelled out at {level:?}");
    // It fired its RCS turning, and not the whole way (only while turning).
    assert!(rcs_ticks > 10 && rcs_ticks < 200, "{rcs_ticks} ticks on RCS");
    // Under the anime rules turning and flying burn nothing.
    assert_eq!(sim.suits.flight[i].propellant, propellant);
    // The owner's prediction flew what the server flew.
    assert!(worst < 0.01, "predicted {worst} m off the server");
}
