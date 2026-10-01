//! Flight model: Newtonian motion, flight assist, finite delta-v, and pilot G limits.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{BOOST, FLIGHT_ASSIST, RCS_SHARP};
use bc_proto::{FrameId, InputCmd};
use bc_sim::DT;
use bc_sim::config::G0;
use bc_sim::content::frame;
use bc_sim::flight::{FA_G_CAP, FA_RESPONSE, FlightMods, FlightState, HUMAN_G_TOLERANCE, step};
use glam::{Quat, Vec3};

fn fresh(f: FrameId) -> FlightState {
    FlightState {
        pos: Vec3::new(0.0, 2_000.0, 0.0),
        propellant: frame(f).propellant_cap,
        ..FlightState::default()
    }
}

#[test]
fn flight_assist_cruises_then_stops() {
    let spec = frame(FrameId::Leo);
    let mut s = fresh(FrameId::Leo);
    let go = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    for _ in 0..(10 * 30) {
        step(&mut s, &go, spec, &FlightMods::default(), DT);
    }
    assert!((s.vel.z - spec.fa_speed).abs() < 1.0, "cruise {:?}", s.vel);
    // Retro thrust is 2.1 g on a Leo, so 220 m/s takes ~11 s to kill.
    let stop = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    for _ in 0..(14 * 30) {
        step(&mut s, &stop, spec, &FlightMods::default(), DT);
    }
    assert!(s.vel.length() < 0.5, "should brake to a stop, still {:?}", s.vel);
}

#[test]
fn delta_v_matches_the_rocket_equation() {
    // Burn a full tank; integrated |a|·dt must equal ve·ln(m0/m1).
    let spec = frame(FrameId::Leo);
    let mut s = fresh(FrameId::Leo);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], ..InputCmd::default() };
    let mut dv = 0.0f64;
    for _ in 0..(200 * 30) {
        let out = step(&mut s, &burn, spec, &FlightMods::default(), DT);
        dv += f64::from(out.accel.length() * DT);
        if s.propellant <= 0.0 {
            break;
        }
    }
    let ideal = f64::from(spec.exhaust_velocity())
        * (f64::from(spec.mass(spec.propellant_cap)) / f64::from(spec.dry_mass)).ln();
    assert!((dv - ideal).abs() / ideal < 0.01, "dv {dv:.1} vs ideal {ideal:.1}");
}

#[test]
fn over_boost_blacks_a_human_out_and_they_recover() {
    // Wing Zero on boost pulls 12 g: well past what a pilot's body takes.
    let spec = frame(FrameId::WingZero);
    let mut s = fresh(FrameId::WingZero);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let mut ticks = 0;
    while !s.blackout && ticks < 300 {
        step(&mut s, &burn, spec, &FlightMods::default(), DT);
        ticks += 1;
    }
    assert!(s.blackout, "never blacked out (strain {})", s.g_strain);
    assert!(s.g_load > 11.0, "g load {}", s.g_load);
    assert!(ticks < 60, "blackout took {ticks} ticks");
    let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    for _ in 0..(4 * 30) {
        step(&mut s, &coast, spec, &FlightMods::default(), DT);
    }
    assert!(!s.blackout, "should have recovered (strain {})", s.g_strain);
}

#[test]
fn mobile_dolls_have_no_body_to_black_out() {
    let spec = frame(FrameId::Taurus);
    let mut s = fresh(FrameId::Taurus);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let doll = FlightMods { g_immune: true, ..FlightMods::default() };
    for _ in 0..300 {
        step(&mut s, &burn, spec, &doll, DT);
    }
    assert_eq!(s.g_strain, 0.0);
    assert!(!s.blackout);
}

#[test]
fn the_colony_hull_is_solid() {
    let spec = frame(FrameId::Leo);
    let mut s = FlightState {
        pos: Vec3::new(0.0, -800.0, 0.0),
        vel: Vec3::new(0.0, -300.0, 0.0),
        ..fresh(FrameId::Leo)
    };
    let cmd = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    for _ in 0..300 {
        step(&mut s, &cmd, spec, &FlightMods::default(), DT);
        assert!(!bc_sim::world::inside_colony(s.pos), "went through the hull at {:?}", s.pos);
    }
}

/// A pilot's flight assist stopping them from cruise: the speed only ever falls, never reverses,
/// and the G tapers off at the end instead of dropping from full thrust to nothing in a tick, on a
/// full tank and a light one.
#[test]
fn flight_assist_stops_smoothly() {
    for id in [FrameId::Leo, FrameId::WingZero, FrameId::WingZeroBird] {
        let spec = frame(id);
        for tank in [1.0, 0.4] {
            let v0 = Vec3::Z * spec.fa_speed;
            let mut s = FlightState { vel: v0, propellant: spec.propellant_cap * tank, ..fresh(id) };
            let stop = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
            let braking = (spec.retro_thrust / spec.mass(s.propellant)).min(FA_G_CAP * G0);
            let within = (spec.fa_speed / braking / DT) as usize + 45;
            let (mut prev_speed, mut prev_g) = (v0.length(), 0.0f32);
            for n in 0..within + 60 {
                step(&mut s, &stop, spec, &FlightMods::default(), DT);
                let speed = s.vel.length();
                assert!(speed <= prev_speed + 1e-4, "{id:?} sped up braking at tick {n}");
                assert!(s.vel.dot(v0) >= -1e-3, "{id:?} went backwards at tick {n}");
                if prev_g > 0.05 {
                    let least = prev_g * (1.0 - DT / FA_RESPONSE) - 0.02;
                    assert!(s.g_load >= least, "{id:?}: G fell from {prev_g} to {} at tick {n}", s.g_load);
                }
                (prev_speed, prev_g) = (speed, s.g_load);
                if n == within {
                    assert!(speed < 1e-3, "{id:?} still at {speed} m/s after {:.1} s", n as f32 * DT);
                }
            }
            assert!(s.g_load < 0.01, "{id:?} still pulling {} g at rest", s.g_load);
        }
    }
}

/// Flight assist holds a Gundam's pilot under their G tolerance, however hard its thrusters could
/// push and however light its tank: flat out from rest while swinging the aim from side to side,
/// then braking with the velocity off the nose. The strain never builds.
#[test]
fn flight_assist_spares_the_pilot() {
    for id in [FrameId::WingZero, FrameId::Deathscythe, FrameId::Shenlong, FrameId::WingZeroBird] {
        let spec = frame(id);
        for tank in [1.0, 0.4] {
            let mut s = FlightState { propellant: spec.propellant_cap * tank, ..fresh(id) };
            let mut limited = 0;
            for n in 0..(12 * 30) {
                let t = n as f32 * DT;
                let (yaw, forward) = if t < 8.0 { (1.2 * (1.5 * t).sin(), 127) } else { (0.8, 0) };
                let aim = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let cmd =
                    InputCmd { aim, thrust: [0, 0, forward], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
                let out = step(&mut s, &cmd, spec, &FlightMods::default(), DT);
                limited += usize::from(out.g_limited);
                assert!(
                    s.g_load < HUMAN_G_TOLERANCE,
                    "{id:?} at {:.0}% tank pulled {} g at {t:.2} s",
                    tank * 100.0,
                    s.g_load
                );
                assert_eq!(s.g_strain, 0.0, "{id:?}: strain built at {t:.2} s");
                assert!(!s.blackout);
            }
            assert!(limited > 0, "{id:?} never needed holding back");
        }
    }
}

/// Boost is the pilot's own call: with flight assist a boosting Wing Zero still pulls 12 g, and
/// blacks its pilot out.
#[test]
fn boosting_still_outthrusts_the_pilot() {
    let spec = frame(FrameId::WingZero);
    let mut s = fresh(FrameId::WingZero);
    let go =
        InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST | BOOST, ..InputCmd::default() };
    let mut most = 0.0f32;
    for _ in 0..60 {
        step(&mut s, &go, spec, &FlightMods::default(), DT);
        most = most.max(s.g_load);
    }
    assert!(most > 11.0, "pulled only {most} g");
    assert!(s.blackout, "strain {}", s.g_strain);
}

/// A pilot holding boost through a blackout (which cuts the boost itself) keeps flight assist's
/// boosted cruise: it carries on toward it at what thrust is left rather than braking.
#[test]
fn a_blackout_mid_boost_doesnt_brake() {
    let spec = frame(FrameId::WingZero);
    let mut s =
        FlightState { vel: Vec3::Z * 400.0, g_strain: 1.0, blackout: true, ..fresh(FrameId::WingZero) };
    let go =
        InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST | BOOST, ..InputCmd::default() };
    let mut prev = s.vel.z;
    for n in 0..(3 * 30) {
        step(&mut s, &go, spec, &FlightMods::default(), DT);
        assert!(s.vel.z >= prev - 1e-3, "slowed from {prev} to {} at tick {n}", s.vel.z);
        prev = s.vel.z;
    }
    assert!(prev > 400.0);
}

/// A blade's lunge drives at one and a half times full main thrust, boost or not.
#[test]
fn a_lunge_is_never_boosted() {
    let spec = frame(FrameId::WingZero);
    let lunge = FlightMods { lunge: true, ..FlightMods::default() };
    let pull = |buttons: u16| {
        let mut s = fresh(FrameId::WingZero);
        let cmd = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons, ..InputCmd::default() };
        let out = step(&mut s, &cmd, spec, &lunge, DT);
        (s.g_load, out.throttle.z)
    };
    let (plain, throttle) = pull(0);
    assert_eq!(pull(BOOST), (plain, throttle));
    assert!((throttle - 1.5).abs() < 1e-4, "throttle {throttle}");
    assert!(plain < 12.5, "{plain} g");
}

/// Turning onto the aim, a suit settles on it rather than swinging past, whatever its turning
/// authority: every frame, with its arms free, busy (firing or striking) or shot away, with RCS or
/// without, blacked out or not, from a small turn or from nearly behind.
#[test]
fn turns_settle_without_swinging_past_the_aim() {
    for id in FrameId::ALL {
        let spec = frame(id);
        for ambac in [1.0, 0.6, 0.18] {
            for buttons in [0, RCS_SHARP] {
                for out in [false, true] {
                    for start in [10.0f32, 45.0, 90.0, 170.0] {
                        let mut s = FlightState {
                            rot: Quat::from_rotation_y(start.to_radians()),
                            g_strain: if out { 1.2 } else { 0.0 },
                            blackout: out,
                            ..fresh(id)
                        };
                        let cmd = InputCmd { aim: Vec3::Z, buttons, ..InputCmd::default() };
                        let mods = FlightMods { ambac, ..FlightMods::default() };
                        let mut past = 0.0f32;
                        for _ in 0..(20 * 30) {
                            step(&mut s, &cmd, spec, &mods, DT);
                            let nose = s.rot * Vec3::Z;
                            past = past.max(-nose.x.atan2(nose.z).to_degrees());
                        }
                        let nose = s.rot * Vec3::Z;
                        let off = nose.angle_between(Vec3::Z).to_degrees();
                        let case =
                            format!("{id:?} ambac {ambac} rcs {} blackout {out} from {start}°", buttons != 0);
                        assert!(past <= 0.5, "{case}: swung {past:.2}° past the aim");
                        assert!(off < 1.0, "{case}: still {off:.2}° off after 20 s");
                    }
                }
            }
        }
    }
}

/// Each axis' thrusters are only as strong as what's left of them, and flight assist asks for no
/// more than they can give.
#[test]
fn damaged_thrusters_push_less_on_their_own_axis() {
    let spec = frame(FrameId::Leo);
    let accel = |mods: FlightMods, thrust: [i8; 3]| {
        let mut s = fresh(FrameId::Leo);
        let cmd = InputCmd { aim: Vec3::Z, thrust, ..InputCmd::default() };
        step(&mut s, &cmd, spec, &mods, DT).accel.length()
    };
    let whole = FlightMods::default();
    let weak_main = FlightMods { main: 0.5, ..whole };
    let weak_side = FlightMods { side: 0.5, ..whole };
    let fwd = [0, 0, 127];
    let right = [127, 0, 0];
    assert!((accel(weak_main, fwd) / accel(whole, fwd) - 0.5).abs() < 1e-3);
    assert_eq!(accel(weak_main, right), accel(whole, right), "the main engines don't strafe");
    assert!((accel(weak_side, right) / accel(whole, right) - 0.5).abs() < 1e-3);
    assert_eq!(accel(weak_side, fwd), accel(whole, fwd));
}

/// Damaged boosters give part of boost's extra thrust; failed ones give none, and then holding
/// Shift neither boosts nor lifts flight assist's G guard or its cruise.
#[test]
fn boosters_give_what_they_have_left() {
    let spec = frame(FrameId::WingZero);
    let boost = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let run = |mods: FlightMods| {
        let mut s = fresh(FrameId::WingZero);
        step(&mut s, &boost, spec, &mods, DT)
    };
    let full = run(FlightMods::default()).accel.length();
    let half = run(FlightMods { boost: 0.5, ..FlightMods::default() });
    let none = run(FlightMods { boost: 0.0, ..FlightMods::default() });
    let plain = {
        let mut s = fresh(FrameId::WingZero);
        let cmd = InputCmd { buttons: 0, ..boost };
        step(&mut s, &cmd, spec, &FlightMods::default(), DT).accel.length()
    };
    let expect_half = plain * (1.0 + (spec.boost_mult - 1.0) * 0.5);
    assert!(
        (half.accel.length() - expect_half).abs() / expect_half < 1e-3,
        "{} vs {expect_half}",
        half.accel.length()
    );
    assert!(half.boosting && half.accel.length() < full);
    assert!(!none.boosting, "failed boosters don't boost");
    assert_eq!(none.accel.length(), plain);
    // Under flight assist, failed boosters keep the G guard on.
    let mut s = fresh(FrameId::WingZero);
    let fa = InputCmd { buttons: BOOST | FLIGHT_ASSIST, ..boost };
    let out = step(&mut s, &fa, spec, &FlightMods { boost: 0.0, ..FlightMods::default() }, DT);
    assert!(out.accel.length() / G0 <= FA_G_CAP + 1e-3, "{} g", out.accel.length() / G0);
}

/// A holed tank loses propellant at its rate whether the suit burns or not.
#[test]
fn a_leak_drains_the_tank() {
    let spec = frame(FrameId::Leo);
    let mut s = fresh(FrameId::Leo);
    let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    let leak = FlightMods { leak_kg_s: 15.0, ..FlightMods::default() };
    for _ in 0..(10 * 30) {
        step(&mut s, &coast, spec, &leak, DT);
    }
    let lost = spec.propellant_cap - s.propellant;
    assert!((lost - 150.0).abs() < 0.5, "lost {lost} kg");
}

/// A hurt pilot bears less: flight assist holds them under their own tolerance, and G beyond it
/// strains them.
#[test]
fn a_pilot_bears_their_own_tolerance() {
    let spec = frame(FrameId::WingZero);
    let hurt = FlightMods { g_tolerance: 5.0, ..FlightMods::default() };
    let go = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    let mut s = fresh(FrameId::WingZero);
    let out = step(&mut s, &go, spec, &hurt, DT);
    assert!(out.g_limited && out.accel.length() / G0 < 4.98, "{} g", out.accel.length() / G0);
    // Unassisted at ~8 g: the hurt pilot strains faster than a whole one.
    let raw = InputCmd { buttons: 0, ..go };
    let (mut a, mut b) = (fresh(FrameId::WingZero), fresh(FrameId::WingZero));
    for _ in 0..30 {
        step(&mut a, &raw, spec, &hurt, DT);
        step(&mut b, &raw, spec, &FlightMods::default(), DT);
    }
    assert!(a.g_strain > b.g_strain + 0.1, "{} vs {}", a.g_strain, b.g_strain);
    let seat = FlightMods { g_tolerance: HUMAN_G_TOLERANCE + 1.0, ..FlightMods::default() };
    let mut c = fresh(FrameId::WingZero);
    let out = step(&mut c, &go, spec, &seat, DT);
    assert!(out.accel.length() / G0 > FA_G_CAP + 0.5, "a G-seat lets flight assist pull harder");
}
