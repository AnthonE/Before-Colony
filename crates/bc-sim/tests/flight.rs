//! Flight model: Newtonian motion, flight assist, finite delta-v, and pilot G limits.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{BOOST, BRAKE, FLIGHT_ASSIST, RCS_SHARP};
use bc_proto::{FrameId, InputCmd};
use bc_sim::DT;
use bc_sim::config::G0;
use bc_sim::content::frame;
use bc_sim::field::{Field, Rock};
use bc_sim::flight::{
    FA_G_CAP, FA_G_MARGIN, FA_RESPONSE, FlightMods, FlightState, GEnvelope, HUMAN_G_TOLERANCE, LockOnAssist,
    O2_BUFFER, step, step_in, thrusters,
};
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
    let spec = frame(FrameId::Argo);
    let mut s = fresh(FrameId::Argo);
    let go = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    for _ in 0..(10 * 30) {
        step(&mut s, &go, spec, &FlightMods::default(), DT);
    }
    assert!((s.vel.z - spec.fa_speed).abs() < 1.0, "cruise {:?}", s.vel);
    // Retro thrust is 2.1 g on an Argo, so 220 m/s takes ~11 s to kill.
    let stop = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    for _ in 0..(14 * 30) {
        step(&mut s, &stop, spec, &FlightMods::default(), DT);
    }
    assert!(s.vel.length() < 0.5, "should brake to a stop, still {:?}", s.vel);
}

#[test]
fn delta_v_matches_the_rocket_equation() {
    // Burn a full tank; integrated |a|·dt must equal ve·ln(m0/m1).
    let spec = frame(FrameId::Argo);
    let mut s = fresh(FrameId::Argo);
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

/// Flies `cmd` until the pilot blacks out, for at most `secs`: how long it took, s.
fn till_blackout(id: FrameId, s: &mut FlightState, cmd: &InputCmd, mods: &FlightMods, secs: f32) -> f32 {
    let mut n = 0;
    while !s.blackout && (n as f32) < secs / DT {
        step(s, cmd, frame(id), mods, DT);
        n += 1;
    }
    n as f32 * DT
}

/// Thrusting toward their feet drives a pilot's blood to their head: an Altair's side thrusters
/// (4.4 g down) are past the half of their 6 g they bear that way, far enough to cut their
/// brain's blood off, and they black out when its oxygen runs out. Let go, they come round.
#[test]
fn thrusting_down_blacks_a_pilot_out_and_they_recover() {
    let mut s = fresh(FrameId::Altair);
    let dive = InputCmd { aim: Vec3::Z, thrust: [0, -127, 0], ..InputCmd::default() };
    let t = till_blackout(FrameId::Altair, &mut s, &dive, &FlightMods::default(), 30.0);
    assert!(s.blackout, "never blacked out (strain {})", s.g_strain);
    assert!((t - O2_BUFFER).abs() <= 2.0 * DT, "out after {t} s");
    assert!(s.g_load > 4.4, "g load {}", s.g_load);
    let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    for _ in 0..(4 * 30) {
        step(&mut s, &coast, frame(FrameId::Altair), &FlightMods::default(), DT);
    }
    assert!(!s.blackout, "should have recovered (strain {})", s.g_strain);
}

/// An Altair's boost (12 g ahead) presses its pilot back into the seat, which they bear while
/// the tank's full; as it empties the suit lightens, and past 12 g even that's too much: flat out
/// from a full tank, they black out a little before it's dry.
#[test]
fn boost_presses_a_pilot_into_the_seat_till_the_suit_is_light() {
    let mut s = fresh(FrameId::Altair);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let t = till_blackout(FrameId::Altair, &mut s, &burn, &FlightMods::default(), 5.0);
    assert!(!s.blackout && s.g_strain < 0.1, "strain {} after {t} s", s.g_strain);
    let t = t + till_blackout(FrameId::Altair, &mut s, &burn, &FlightMods::default(), 30.0);
    assert!(s.blackout && (12.0..25.0).contains(&t), "out after {t} s");
    assert!(s.g_load > 14.0 && s.propellant > 0.0, "{} g, {} kg left", s.g_load, s.propellant);
}

#[test]
fn drones_have_no_body_to_black_out() {
    let spec = frame(FrameId::Corvus);
    let mut s = fresh(FrameId::Corvus);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let drone = FlightMods { g_immune: true, ..FlightMods::default() };
    for _ in 0..300 {
        step(&mut s, &burn, spec, &drone, DT);
    }
    assert_eq!(s.g_strain, 0.0);
    assert!(!s.blackout);
}

/// The colony's hull is solid, and hitting it at 300 m/s knocks the pilot out.
#[test]
fn the_colony_hull_is_solid() {
    let spec = frame(FrameId::Argo);
    let mut s = FlightState {
        pos: bc_sim::world::COLONY_CENTER + Vec3::Y * (bc_sim::world::COLONY_RADIUS + 200.0),
        vel: Vec3::new(0.0, -300.0, 0.0),
        ..fresh(FrameId::Argo)
    };
    let cmd = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    let mut out_cold = false;
    for _ in 0..300 {
        step(&mut s, &cmd, spec, &FlightMods::default(), DT);
        assert!(!bc_sim::world::inside_colony(s.pos), "went through the hull at {:?}", s.pos);
        out_cold |= s.blackout;
    }
    assert!(out_cold, "the crash didn't knock the pilot out");
}

/// A rock is a crash too: an Argo coasting into one at 40 m/s, face first, knocks its pilot out (the
/// jolt is about 80 g); one drifting onto it at 5 m/s is nothing. The sector's bounds are no wall:
/// stopped at them from 300 m/s, the pilot feels nothing.
#[test]
fn a_rock_is_a_crash_and_the_sectors_bounds_are_not() {
    let spec = frame(FrameId::Argo);
    let rock = Rock {
        pos: Vec3::new(0.0, 2_000.0, 200.0),
        radius: 50.0,
        axes: Vec3::splat(50.0),
        ..Rock::default()
    };
    let field = Field::from_rocks(&[rock]);
    let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    let into = |speed: f32| {
        let mut s = FlightState { vel: Vec3::Z * speed, ..fresh(FrameId::Argo) };
        let mut out_cold = false;
        for _ in 0..(200.0 / speed / DT) as usize + 30 {
            step_in(&field, &mut s, &coast, spec, &FlightMods::default(), DT);
            out_cold |= s.blackout;
        }
        assert!(s.vel.z.abs() < 0.5, "stopped at the rock: {}", s.vel);
        (out_cold, s.g_strain)
    };
    assert!(into(40.0).0, "a crash at 40 m/s");
    assert_eq!(into(5.0), (false, 0.0));
    let edge = bc_sim::config::SECTOR_LIMIT;
    let mut s = FlightState {
        pos: Vec3::new(edge - 5.0, 2_000.0, 0.0),
        vel: Vec3::X * 300.0,
        ..fresh(FrameId::Argo)
    };
    for _ in 0..30 {
        step(&mut s, &coast, spec, &FlightMods::default(), DT);
    }
    assert!(s.pos.x == edge && s.vel.x == 0.0 && s.g_strain == 0.0, "{:?}", s);
}

/// A pilot's flight assist stopping them from cruise: the speed only ever falls, never reverses,
/// and the G tapers off at the end instead of dropping from full thrust to nothing in a tick, on a
/// full tank and a light one. (Braking throws the pilot into the straps, which they bear half as
/// much again as headward.)
#[test]
fn flight_assist_stops_smoothly() {
    for id in [FrameId::Argo, FrameId::Altair, FrameId::Aquila] {
        let spec = frame(id);
        for tank in [1.0, 0.4] {
            let v0 = Vec3::Z * spec.fa_speed;
            let mut s = FlightState { vel: v0, propellant: spec.propellant_cap * tank, ..fresh(id) };
            let stop = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
            let braking =
                (spec.retro_thrust / spec.mass(s.propellant)).min(FA_G_CAP * GEnvelope::HUMAN.back * G0);
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

/// Flight assist holds a Lodestar's pilot under what they bear each way, however hard its thrusters
/// could push and however light its tank: flat out from rest while swinging the aim from side to
/// side, then braking with the velocity off the nose. The strain never builds, though ahead
/// (pressed into the seat) it lets them pull well past the 6 g they bear headward. The Aquila's
/// side thrusters are too weak ever to need holding back.
#[test]
fn flight_assist_spares_the_pilot() {
    for id in [FrameId::Altair, FrameId::Algol, FrameId::Thuban, FrameId::Aquila] {
        let spec = frame(id);
        let (mut limited, mut most) = (0, 0.0f32);
        for tank in [1.0, 0.4] {
            let mut s = FlightState { propellant: spec.propellant_cap * tank, ..fresh(id) };
            for n in 0..(12 * 30) {
                let t = n as f32 * DT;
                let (yaw, forward) = if t < 8.0 { (1.2 * (1.5 * t).sin(), 127) } else { (0.8, 0) };
                let aim = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let cmd =
                    InputCmd { aim, thrust: [0, 0, forward], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
                let out = step(&mut s, &cmd, spec, &FlightMods::default(), DT);
                limited += usize::from(out.g_limited);
                most = most.max(s.g_load);
                assert_eq!(s.g_strain, 0.0, "{id:?} at {:.0}% tank: strain built at {t:.2} s", tank * 100.0);
                assert!(!s.blackout);
            }
        }
        assert!(limited > 0 || id == FrameId::Aquila, "{id:?} never needed holding back");
        assert!(most > 1.4 * HUMAN_G_TOLERANCE, "{id:?} pulled only {most} g");
    }
}

/// Boost is the pilot's own call: flight assist holds nothing back from it, so a boosting Altair
/// pulls its whole 12 g, ahead, which its pilot bears. Climbing on boost as well (4.4 g up, their
/// blood driven to their feet) is past what they bear, and they black out soon after their brain
/// runs short.
#[test]
fn boosting_still_outthrusts_the_pilot() {
    let spec = frame(FrameId::Altair);
    let mut s = fresh(FrameId::Altair);
    let go =
        InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST | BOOST, ..InputCmd::default() };
    let mut most = 0.0f32;
    for _ in 0..60 {
        assert!(!step(&mut s, &go, spec, &FlightMods::default(), DT).g_limited);
        most = most.max(s.g_load);
    }
    assert!(most > 11.0, "pulled only {most} g");
    assert!(s.g_strain < 0.05 && !s.blackout, "strain {}", s.g_strain);
    let mut s = fresh(FrameId::Altair);
    let climb = InputCmd { aim: Vec3::Z, thrust: [0, 127, 127], buttons: BOOST, ..InputCmd::default() };
    let t = till_blackout(FrameId::Altair, &mut s, &climb, &FlightMods::default(), 30.0);
    assert!(s.blackout && t > O2_BUFFER && t < 1.5 * O2_BUFFER, "out after {t} s (strain {})", s.g_strain);
}

/// A pilot holding boost through a blackout (which cuts the boost itself) keeps flight assist's
/// boosted cruise: it carries on toward it at what thrust is left rather than braking.
#[test]
fn a_blackout_mid_boost_doesnt_brake() {
    let spec = frame(FrameId::Altair);
    let mut s = FlightState { vel: Vec3::Z * 400.0, g_strain: 1.0, blackout: true, ..fresh(FrameId::Altair) };
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
    let spec = frame(FrameId::Altair);
    let lunge = FlightMods { lunge: true, ..FlightMods::default() };
    let pull = |buttons: u16| {
        let mut s = fresh(FrameId::Altair);
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
/// without, blacked out or not, from a small turn or from nearly behind, by either rules' agility.
#[test]
fn turns_settle_without_swinging_past_the_aim() {
    for (id, agility) in
        FrameId::ALL.into_iter().flat_map(|id| [(id, 1.0), (id, bc_sim::tuning::ANIME_AGILITY)])
    {
        let spec = frame(id);
        for limb_trim in [1.0, 0.6, 0.18] {
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
                        let mods = FlightMods { limb_trim, agility, ..FlightMods::default() };
                        let mut past = 0.0f32;
                        for _ in 0..(20 * 30) {
                            step(&mut s, &cmd, spec, &mods, DT);
                            let nose = s.rot * Vec3::Z;
                            past = past.max(-nose.x.atan2(nose.z).to_degrees());
                        }
                        let nose = s.rot * Vec3::Z;
                        let off = nose.angle_between(Vec3::Z).to_degrees();
                        let case = format!(
                            "{id:?} agility {agility} limb trim {limb_trim} rcs {} blackout {out} from {start}°",
                            buttons != 0
                        );
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
    let spec = frame(FrameId::Argo);
    let accel = |mods: FlightMods, thrust: [i8; 3]| {
        let mut s = fresh(FrameId::Argo);
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
    let spec = frame(FrameId::Altair);
    let boost = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let run = |mods: FlightMods| {
        let mut s = fresh(FrameId::Altair);
        step(&mut s, &boost, spec, &mods, DT)
    };
    let full = run(FlightMods::default()).accel.length();
    let half = run(FlightMods { boost: 0.5, ..FlightMods::default() });
    let none = run(FlightMods { boost: 0.0, ..FlightMods::default() });
    let plain = {
        let mut s = fresh(FrameId::Altair);
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
    // Under flight assist, failed boosters keep the G guard on: diving, the pilot's held to half
    // what they bear headward.
    let mut s = fresh(FrameId::Altair);
    let fa = InputCmd { thrust: [0, -127, 0], buttons: BOOST | FLIGHT_ASSIST, ..boost };
    let out = step(&mut s, &fa, spec, &FlightMods { boost: 0.0, ..FlightMods::default() }, DT);
    assert!(
        out.g_limited && out.accel.length() / G0 <= 0.5 * FA_G_CAP + 1e-3,
        "{} g",
        out.accel.length() / G0
    );
}

/// A holed tank loses propellant at its rate whether the suit burns or not.
#[test]
fn a_leak_drains_the_tank() {
    let spec = frame(FrameId::Argo);
    let mut s = fresh(FrameId::Argo);
    let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
    let leak = FlightMods { leak_kg_s: 15.0, ..FlightMods::default() };
    for _ in 0..(10 * 30) {
        step(&mut s, &coast, spec, &leak, DT);
    }
    let lost = spec.propellant_cap - s.propellant;
    assert!((lost - 150.0).abs() < 0.5, "lost {lost} kg");
}

/// A hurt pilot bears less: flight assist holds them under their own tolerance (diving, half of
/// it), and G beyond it strains them: the Altair's 4.4 g sideways is past what a pilot of a
/// failed cockpit bears (4 g), not a whole one's. A G-seat lets them bear more.
#[test]
fn a_pilot_bears_their_own_tolerance() {
    let spec = frame(FrameId::Altair);
    let hurt = FlightMods { g_tolerance: 4.0, ..FlightMods::default() };
    let dive = InputCmd { aim: Vec3::Z, thrust: [0, -127, 0], buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    let mut s = fresh(FrameId::Altair);
    let out = step(&mut s, &dive, spec, &hurt, DT);
    let g = out.accel.length() / G0;
    assert!(out.g_limited && (g - 0.5 * (4.0 - FA_G_MARGIN)).abs() < 1e-3, "{g} g");
    let raw = InputCmd { aim: Vec3::Z, thrust: [127, 0, 0], ..InputCmd::default() };
    let (mut a, mut b) = (fresh(FrameId::Altair), fresh(FrameId::Altair));
    for _ in 0..(3 * 30) {
        step(&mut a, &raw, spec, &hurt, DT);
        step(&mut b, &raw, spec, &FlightMods::default(), DT);
    }
    assert!(a.g_strain > 0.05 && b.g_strain == 0.0, "{} vs {}", a.g_strain, b.g_strain);
    let seat = FlightMods { g_tolerance: HUMAN_G_TOLERANCE + 1.0, ..FlightMods::default() };
    let mut c = fresh(FrameId::Altair);
    let out = step(&mut c, &dive, spec, &seat, DT);
    assert!(out.accel.length() / G0 > 0.5 * FA_G_CAP + 0.4, "a G-seat lets flight assist pull harder");
}

/// Anime rules' modifiers for a whole suit (`bc_sim::tuning`, as the server and the owner's client
/// build them).
fn anime() -> FlightMods {
    let t = bc_sim::tuning::Tuning::default();
    bc_sim::tuning::flight_mods(&t, bc_sim::tuning::FlightRules::Anime, false, 0)
}

#[test]
fn under_anime_rules_flying_is_free_and_only_boost_spends_the_gauge() {
    let spec = frame(FrameId::Argo);
    let mods = anime();
    let tank = spec.propellant_cap;
    // Back and forth under flight assist, turning, on RCS: never a kilogram burnt.
    let mut s = fresh(FrameId::Argo);
    for k in 0..(40 * 30) {
        let forward = if (k / 90) % 2 == 0 { 127 } else { -127 };
        let aim = Quat::from_rotation_y(k as f32 * 0.01) * Vec3::Z;
        let buttons = FLIGHT_ASSIST | if k % 200 < 50 { RCS_SHARP } else { 0 };
        let cmd = InputCmd { aim, thrust: [40, -20, forward], buttons, ..InputCmd::default() };
        step(&mut s, &cmd, spec, &mods, DT);
    }
    assert_eq!(s.propellant, tank, "flying burnt the gauge");
    assert!(s.vel.length() > 10.0, "it flew: {:?}", s.vel);
    // Boost spends it, as it always has (all the thrust it gives: half as much again as the real
    // rules' ahead, as the anime's thrusters push).
    let boost = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    let mut real = fresh(FrameId::Argo);
    let mut s = fresh(FrameId::Argo);
    for _ in 0..(5 * 30) {
        step(&mut s, &boost, spec, &mods, DT);
        step(&mut real, &boost, spec, &FlightMods::default(), DT);
    }
    assert!(s.propellant < tank - 200.0, "5 s of boost burnt only {} kg", tank - s.propellant);
    let (anime_kg, real_kg) = (tank - s.propellant, tank - real.propellant);
    assert!((anime_kg - bc_sim::tuning::ANIME_THRUST * real_kg).abs() < 1.0, "{anime_kg} vs {real_kg}");
    // Let go, it fills back up: the whole tank in ANIME_REFILL_SECS.
    let coast = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
    let low = s.propellant;
    for _ in 0..30 {
        step(&mut s, &coast, spec, &mods, DT);
    }
    let per_s = s.propellant - low;
    let want = tank / bc_sim::tuning::ANIME_REFILL_SECS;
    assert!((per_s - want).abs() < want * 0.01, "{per_s} kg/s back, want {want}");
    for _ in 0..(bc_sim::tuning::ANIME_REFILL_SECS as usize * 30) {
        step(&mut s, &coast, spec, &mods, DT);
    }
    assert_eq!(s.propellant, tank, "full again, and no fuller");
}

#[test]
fn under_anime_rules_an_empty_gauge_still_flies_but_cant_boost_until_let_go() {
    let spec = frame(FrameId::Altair);
    let mods = anime();
    let mut s = FlightState { propellant: 0.0, ..fresh(FrameId::Altair) };
    // Leaning on boost with the gauge dry: it flies at its plain cruise, and nothing comes back.
    let held =
        InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST | BOOST, ..InputCmd::default() };
    for _ in 0..(8 * 30) {
        let out = step(&mut s, &held, spec, &mods, DT);
        assert!(!out.boosting);
    }
    assert_eq!(s.propellant, 0.0);
    assert!((s.vel.z - spec.fa_speed).abs() < 1.0, "plain cruise, not boost's: {:?}", s.vel);
    // Let go of boost and it refills; held again, it boosts.
    let go = InputCmd { buttons: FLIGHT_ASSIST, ..held };
    for _ in 0..30 {
        step(&mut s, &go, spec, &mods, DT);
    }
    assert!(s.propellant > 100.0);
    assert!(step(&mut s, &held, spec, &mods, DT).boosting);
    // Under the real rules a dry tank flies nowhere.
    let mut dry = FlightState { propellant: 0.0, ..fresh(FrameId::Altair) };
    for _ in 0..30 {
        step(&mut dry, &held, spec, &FlightMods::default(), DT);
    }
    assert_eq!(dry.vel, Vec3::ZERO);
}

#[test]
fn under_anime_rules_a_holed_tank_refills_slower_and_a_failed_one_not_at_all() {
    use bc_sim::content::systems::{DAMAGED, FAILED};
    use bc_sim::content::{System, Systems, modules::Modules};
    use bc_sim::tuning::{FlightRules, flight_mods, tuning};
    let spec = frame(FrameId::Argo);
    let refill = |level: u8| {
        let t = tuning(0, Systems::OK.with(System::Tank, level), Modules::NONE);
        let mods = flight_mods(&t, FlightRules::Anime, false, 0);
        let mut s = FlightState { propellant: 500.0, ..fresh(FrameId::Argo) };
        let coast = InputCmd { aim: Vec3::Z, ..InputCmd::default() };
        for _ in 0..30 {
            step(&mut s, &coast, spec, &mods, DT);
        }
        s.propellant - 500.0
    };
    let (whole, holed, failed) = (refill(0), refill(DAMAGED), refill(FAILED));
    assert!(holed < whole * 0.6 && holed > 0.0, "{whole} {holed}");
    assert!(failed < 0.0, "a failed tank only leaks: {failed}");
}

#[test]
fn under_anime_rules_a_pilot_bears_twice_the_g() {
    let mods = anime();
    assert_eq!(mods.g_tolerance, HUMAN_G_TOLERANCE * bc_sim::tuning::ANIME_G);
    // Algol's boost (about 11 g) doesn't black its pilot out.
    let spec = frame(FrameId::Algol);
    let mut s = fresh(FrameId::Algol);
    let burn = InputCmd { aim: Vec3::Z, thrust: [0, 0, 127], buttons: BOOST, ..InputCmd::default() };
    for _ in 0..(5 * 30) {
        step(&mut s, &burn, spec, &mods, DT);
    }
    assert!(s.g_load > 10.0 && !s.blackout && s.g_strain == 0.0, "{} g, strain {}", s.g_load, s.g_strain);
    // Nor does an Altair's on a nearly dry gauge (16 g ahead, pressing its pilot into the seat),
    // but diving on it as well (their blood driven to their head) still blacks them out.
    let mut s =
        FlightState { propellant: 0.3 * frame(FrameId::Altair).propellant_cap, ..fresh(FrameId::Altair) };
    let t = till_blackout(FrameId::Altair, &mut s, &burn, &mods, 8.0);
    assert!(
        !s.blackout && s.g_load > 15.0 && s.g_strain == 0.0,
        "{} g, strain {} at {t} s",
        s.g_load,
        s.g_strain
    );
    let mut s =
        FlightState { propellant: 0.3 * frame(FrameId::Altair).propellant_cap, ..fresh(FrameId::Altair) };
    let dive = InputCmd { thrust: [0, -127, 127], ..burn };
    let t = till_blackout(FrameId::Altair, &mut s, &dive, &mods, 8.0);
    assert!(s.blackout && t > O2_BUFFER, "out after {t} s (strain {})", s.g_strain);
}

#[test]
fn drones_fly_by_the_real_rules_whatever_the_sectors() {
    let t = bc_sim::tuning::Tuning::default();
    let drone = bc_sim::tuning::flight_mods(&t, bc_sim::tuning::FlightRules::Anime, true, 0);
    assert!(drone.gauge.is_none(), "a drone burns every newton and runs dry");
    assert_eq!(drone.g_tolerance, HUMAN_G_TOLERANCE);
    assert!(anime().gauge.is_some());
    // And handle as their frames do.
    assert!(drone.agility == 1.0 && drone.push == 1.0 && !drone.vectored, "{drone:?}");
}

/// How long frame `id`, on its RCS (as shooter controls turn it), takes to come round `deg` onto the
/// aim, to within 3°, s.
fn turn_secs(id: FrameId, mods: &FlightMods, deg: f32) -> f32 {
    let mut s = FlightState { rot: Quat::from_rotation_y(deg.to_radians()), ..fresh(id) };
    let cmd = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST | RCS_SHARP, ..InputCmd::default() };
    for n in 1..=20 * 30 {
        step(&mut s, &cmd, frame(id), mods, DT);
        if (s.rot * Vec3::Z).angle_between(Vec3::Z) < 3f32.to_radians() {
            return n as f32 * DT;
        }
    }
    f32::INFINITY
}

/// Under anime rules a suit turns three times as fast, as the show's do, and keeps up with a
/// shooter's mouse: an Argo comes round a quarter turn in under half a second and back the way it
/// came in under one (by the real rules, nearly a second and a half and two).
#[test]
fn under_anime_rules_a_suit_turns_three_times_as_fast() {
    let (real, anime) = (FlightMods::default(), anime());
    assert_eq!(anime.agility, bc_sim::tuning::ANIME_AGILITY);
    for id in FrameId::ALL {
        for deg in [45.0, 90.0, 170.0] {
            let (r, a) = (turn_secs(id, &real, deg), turn_secs(id, &anime, deg));
            // (Whole ticks: a quarter-second turn is eight of them.)
            let ratio = r / a;
            assert!(
                ratio > 2.8 && ratio < 4.0,
                "{id:?} {deg}°: {r:.2} s by the real rules, {a:.2} s by the anime's"
            );
        }
    }
    assert!(turn_secs(FrameId::Argo, &anime, 90.0) < 0.5);
    assert!(turn_secs(FrameId::Argo, &anime, 170.0) < 1.0);
    assert!(turn_secs(FrameId::Argo, &real, 90.0) > 1.2);
}

/// Under anime rules the thrusters swivel: every way pushes as hard as ahead, half as hard again as
/// the frame's own main thrusters, and boost overdrives them all. Aquila, its limbs folded, keeps
/// a fighter's thrusters, only stronger. By the real rules nothing changes.
#[test]
fn under_anime_rules_the_thrusters_push_every_way_as_hard_as_ahead() {
    let ways: [[i8; 3]; 6] =
        [[127, 0, 0], [-127, 0, 0], [0, 127, 0], [0, -127, 0], [0, 0, 127], [0, 0, -127]];
    let accel = |id: FrameId, mods: &FlightMods, thrust: [i8; 3], buttons: u16| {
        let mut s = fresh(id);
        let cmd = InputCmd { aim: Vec3::Z, thrust, buttons, ..InputCmd::default() };
        step(&mut s, &cmd, frame(id), mods, DT).accel.length()
    };
    let k = bc_sim::tuning::ANIME_THRUST;
    for id in
        [FrameId::Argo, FrameId::Altair, FrameId::Antares, FrameId::Algol, FrameId::Canopus, FrameId::Thuban]
    {
        let spec = frame(id);
        let ahead = k * spec.main_thrust / spec.mass(spec.propellant_cap);
        for way in ways {
            let a = accel(id, &anime(), way, 0);
            assert!((a / ahead - 1.0).abs() < 1e-4, "{id:?} {way:?}: {a} m/s², ahead {ahead}");
            let boosted = accel(id, &anime(), way, BOOST);
            assert!(
                (boosted / a - spec.boost_mult).abs() < 1e-4,
                "{id:?} {way:?}: boost gives {}",
                boosted / a
            );
        }
        // By the real rules, side and retro as built, and boost the main thrusters' alone.
        let side = accel(id, &FlightMods::default(), [127, 0, 0], 0);
        assert!((side - spec.side_thrust / spec.mass(spec.propellant_cap)).abs() < 1e-3);
        assert_eq!(accel(id, &FlightMods::default(), [127, 0, 0], BOOST), side);
        // The planners' numbers are the step's.
        let t = thrusters(spec, &anime(), false);
        assert!(t.main == t.side && t.side == t.retro && t.main == spec.main_thrust * k, "{t:?}");
    }
    let bird = frame(FrameId::Aquila);
    let t = thrusters(bird, &anime(), false);
    assert!(t.side < t.main && (t.side - bird.side_thrust * k).abs() < 1.0, "{t:?}");
}

/// Under anime rules W goes where the pilot looks soon after a turn: flying at 100 m/s, the
/// crosshair a quarter turn off (as shooter controls send it: W along the aim), an Argo's
/// velocity swings onto it in under two seconds, where by the real rules it slid on for nearly
/// five.
#[test]
fn under_anime_rules_w_goes_where_the_pilot_looks_soon_after_a_turn() {
    let swung = |mods: &FlightMods| {
        let aim = Vec3::X;
        let lock = LockOnAssist { ref_vel: Vec3::ZERO, right: Vec3::Y.cross(aim), up: Vec3::Y, fwd: aim };
        let mods = FlightMods { lockon: Some(lock), ..*mods };
        let mut s = FlightState { vel: Vec3::Z * 100.0, ..fresh(FrameId::Argo) };
        let cmd =
            InputCmd { aim, thrust: [0, 0, 127], buttons: FLIGHT_ASSIST | RCS_SHARP, ..InputCmd::default() };
        for n in 1..=30 * 30 {
            step(&mut s, &cmd, frame(FrameId::Argo), &mods, DT);
            if s.vel.angle_between(aim) < 10f32.to_radians() && s.vel.length() > 50.0 {
                return n as f32 * DT;
            }
        }
        f32::INFINITY
    };
    let (real, anime) = (swung(&FlightMods::default()), swung(&anime()));
    assert!(anime < 2.0 && real > 4.0, "{anime:.2} s by the anime rules, {real:.2} s by the real ones");
}

/// X brakes as hard as the suit can: at once, every thruster at full with boost's overdrive and
/// nothing held back for the pilot, burning as boost does (the gauge waits meanwhile, and fills
/// again once the suit's at rest). Under anime rules an Argo stops from its cruise in under three
/// seconds, well before letting go of the keys stops it; with the gauge dry it still brakes, at the
/// thrusters' plain push, and the gauge refills.
#[test]
fn the_brake_stops_as_hard_as_the_suit_can() {
    let spec = frame(FrameId::Argo);
    let stop = |mods: &FlightMods, buttons: u16, tank: f32| {
        let mut s = FlightState {
            vel: Vec3::Z * spec.fa_speed,
            propellant: spec.propellant_cap * tank,
            ..fresh(FrameId::Argo)
        };
        let cmd = InputCmd { aim: Vec3::Z, buttons, ..InputCmd::default() };
        for n in 1..=40 * 30 {
            let before = s.propellant;
            let out = step(&mut s, &cmd, spec, mods, DT);
            if s.vel.length() == 0.0 {
                return (n as f32 * DT, s);
            }
            assert!(!out.g_limited, "the brake held back at tick {n}");
            if buttons & BRAKE != 0 && tank > 0.0 {
                assert!(out.boosting && s.propellant < before, "braking hard, it boosts and burns");
            }
        }
        panic!("never stopped");
    };
    let anime = anime();
    let (hard, s) = stop(&anime, FLIGHT_ASSIST | BRAKE, 1.0);
    let (gentle, g) = stop(&anime, FLIGHT_ASSIST, 1.0);
    assert!(hard < 3.0 && gentle > hard + 1.5, "X stops in {hard:.2} s, letting go in {gentle:.2} s");
    assert!(
        s.propellant < spec.propellant_cap - 100.0,
        "X spent only {} kg",
        spec.propellant_cap - s.propellant
    );
    assert_eq!(g.propellant, spec.propellant_cap, "letting go is free");
    // At rest, the overdrive's done: the gauge fills again under X.
    let mut s = s;
    let held = InputCmd { aim: Vec3::Z, buttons: FLIGHT_ASSIST | BRAKE, ..InputCmd::default() };
    let low = s.propellant;
    for _ in 0..30 {
        assert!(!step(&mut s, &held, spec, &anime, DT).boosting);
    }
    assert!(s.propellant > low + 100.0, "{} kg back", s.propellant - low);
    // Its first tick: everything astern, boost's overdrive and all; dry, the thrusters' plain push
    // (and it still stops, the gauge filling meanwhile).
    let first = |tank: f32| {
        let mut s = FlightState {
            vel: Vec3::Z * spec.fa_speed,
            propellant: spec.propellant_cap * tank,
            ..fresh(FrameId::Argo)
        };
        let cmd = InputCmd { aim: Vec3::Z, buttons: BRAKE, ..InputCmd::default() };
        let mass = spec.mass(s.propellant);
        step(&mut s, &cmd, spec, &anime, DT).accel.length() * mass
    };
    let (full, empty) = (first(1.0), first(0.0));
    assert!((full / thrusters(spec, &anime, true).retro - 1.0).abs() < 1e-4, "{full} N");
    assert!((empty / thrusters(spec, &anime, false).retro - 1.0).abs() < 1e-4, "{empty} N");
    let (_, d) = stop(&anime, BRAKE, 0.0);
    assert!(d.propellant > 0.0);
    // By the real rules too, with flight assist off: no easing at the end.
    let (real, _) = stop(&FlightMods::default(), BRAKE, 1.0);
    let retro = spec.retro_thrust / spec.mass(spec.propellant_cap);
    assert!(real < spec.fa_speed / retro + 0.5, "{real:.2} s at {retro} m/s²");
}
