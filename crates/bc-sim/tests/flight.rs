//! Flight model: Newtonian motion, flight assist, finite delta-v, and pilot G limits.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{BOOST, FLIGHT_ASSIST};
use bc_proto::{FrameId, InputCmd};
use bc_sim::DT;
use bc_sim::content::frame;
use bc_sim::flight::{FlightMods, FlightState, step};
use glam::Vec3;

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
