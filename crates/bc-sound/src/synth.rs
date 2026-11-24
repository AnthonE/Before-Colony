//! Every sound, generated from arithmetic at boot: no audio assets (no licences to track, nothing
//! to download). The primitives (noise, one-pole filters, a resonator, envelopes, a small reverb)
//! come from the Gates client's bank (`crates/sound/src/synth.rs` there); the recipes are Before
//! Colony's.
//!
//! Deterministic: every cue is seeded from its own discriminant, so the same build makes the same
//! samples. The tests check shape, not bytes: every cue has energy, doesn't clip, and a one-shot
//! starts and ends near silence while a loop's seam is continuous.

use std::f32::consts::{PI, TAU};

use crate::{Cue, SAMPLE_RATE};

/// Every cue peaks here before its [`crate::CueDef::gain`]: the table alone sets loudness.
pub const PEAK: f32 = 0.9;

const SR: f32 = SAMPLE_RATE as f32;

/// The street's beds and the car's engine are rendered at half the bank's rate
/// ([`crate::sample_rate`]): nothing in them is above a few kHz, and the browser resamples. They
/// cost half as much to build.
pub const BED_RATE: u32 = 24_000;
const BR: f32 = BED_RATE as f32;

/// A cue's samples at [`crate::sample_rate`]`(cue)`, normalized to [`PEAK`]: mono, or for a stereo
/// cue ([`crate::channels`]) the left channel's samples then the right's.
pub fn render(cue: Cue) -> Vec<f32> {
    let mut r = Rng::new(0x9E37_79B9 ^ (cue as u32 + 1).wrapping_mul(0x85EB_CA6B));
    let mut out = match cue {
        Cue::BeamRifle => beam(&mut r, 0.45, 2_600.0, 240.0, 0.11, 0.5),
        Cue::BeamHeavy => {
            let mut b = beam(&mut r, 0.8, 1_500.0, 110.0, 0.2, 0.9);
            mix_in(&mut b, &boom(&mut r, 0.8, 52.0, 0.25), 0, 0.8);
            b
        }
        Cue::HeavyBeamFire => heavy_beam(&mut r),
        Cue::Gun => gun(&mut r),
        Cue::BeamGun => beam(&mut r, 0.16, 1_900.0, 700.0, 0.035, 0.25),
        Cue::Flame => flame(&mut r),
        Cue::MissileLaunch => missile_launch(&mut r),
        Cue::Saber => saber(&mut r),
        Cue::HitFar => impact(&mut r, 0.35, 180.0, 0.08, 0.06, 2_500.0, 300.0),
        Cue::MissileBurst => blast(&mut r, 1.3, 58.0, 0.5),
        Cue::Explosion => blast(&mut r, 3.0, 40.0, 1.0),
        Cue::RockBreak => rock_break(&mut r),
        Cue::Clash => clash(&mut r),
        Cue::HullHit => hull_hit(&mut r),
        Cue::Transform => transform(&mut r),
        Cue::JammerOn => warble(&mut r, 0.7, 1_300.0, 260.0),
        Cue::JammerOff => warble(&mut r, 0.5, 260.0, 1_100.0),
        Cue::Grab => {
            let mut g = impact(&mut r, 0.3, 110.0, 0.07, 0.02, 3_000.0, 600.0);
            mix_in(&mut g, &ring(0.2, &[(1_450.0, 0.05), (2_300.0, 0.03)]), 0, 0.3);
            g
        }
        Cue::Stow => stow(&mut r),
        Cue::Throw => whoosh(&mut r, 0.4, 300.0, 1_900.0),
        Cue::Jettison => jettison(&mut r),
        Cue::Dock => chime(&[(783.99, 0.0, 0.45), (1_046.5, 0.18, 0.7)]),
        Cue::Sale => chime(&[(1_046.5, 0.0, 0.3), (1_318.5, 0.09, 0.3), (1_568.0, 0.18, 0.6)]),
        Cue::LockBeep => beep(0.06, 1_760.0, 0.25),
        Cue::LowFuel => {
            let mut b = beep(0.12, 440.0, 0.35);
            let second = beep(0.12, 440.0, 0.35);
            mix_in(&mut b, &second, samples(0.18), 1.0);
            b
        }
        Cue::RcsPuff => puff(&mut r),
        Cue::OrreryOn => orrery_on(&mut r),
        Cue::Seizure => seizure(&mut r),
        Cue::Destroyed => destroyed(&mut r),
        Cue::Launch => launch(&mut r),
        Cue::Klaxon => klaxon(),
        // Something inside took the blow: a falling two-note chirp.
        Cue::SystemCrit => {
            let mut b = beep(0.07, 1_318.5, 0.4);
            mix_in(&mut b, &beep(0.09, 987.8, 0.4), samples(0.08), 1.0);
            b
        }
        // Something failed: the master caution, three hard square pulses.
        Cue::SystemFail => {
            let mut b = beep(0.11, 660.0, 0.8);
            for k in 1..3 {
                mix_in(&mut b, &beep(0.11, 660.0, 0.8), samples(0.16 * k as f32), 1.0);
            }
            b
        }
        // A holed tank venting: a thin hiss.
        Cue::Leak => whoosh(&mut r, 0.6, 6_800.0, 4_800.0),
        Cue::DoorRumble => door_rumble(&mut r),
        Cue::AirlockHiss => {
            let mut h = whoosh(&mut r, 1.1, 5_200.0, 1_400.0);
            mix_in(&mut h, &impact(&mut r, 0.3, 95.0, 0.08, 0.02, 1_200.0, 150.0), 0, 0.6);
            h
        }
        Cue::UiClick => click(&mut r),
        Cue::UiConfirm => chime(&[(880.0, 0.0, 0.12), (1_318.5, 0.07, 0.2)]),
        Cue::ThrusterLoop => thruster_loop(&mut r),
        Cue::BoostLoop => boost_loop(&mut r),
        Cue::ChargeLoop => charge_loop(),
        Cue::SaberHum => saber_hum(&mut r),
        Cue::LockSolid => tone_loop(0.5, 1_760.0, 0.25),
        Cue::MissileAlarm => missile_alarm(),
        Cue::GStrain => heartbeat(),
        Cue::OrreryDrone => orrery_drone(&mut r),
        Cue::CockpitHum => cockpit_hum(&mut r),
        Cue::MusicCalm => crate::music::calm(),
        Cue::MusicCombat => crate::music::combat(),
        Cue::MusicTitle => crate::title::title(),
        Cue::MagLock => mag_lock(&mut r),
        Cue::MagRelease => mag_release(&mut r),
        Cue::Touchdown => touchdown(&mut r),
        Cue::Footstep => footstep(&mut r, false),
        Cue::FootstepFar => footstep(&mut r, true),
        Cue::PushOff => push_off(&mut r),
        Cue::GoDark => reactor(&mut r, false),
        Cue::PowerUp => reactor(&mut r, true),
        Cue::RadioOpen => squelch(&mut r, false),
        Cue::RadioClose => squelch(&mut r, true),
        Cue::StreetStep => street_step(&mut r),
        Cue::CarDoor => car_door(&mut r),
        Cue::CarPass => car_pass(&mut r),
        Cue::LiftRide => lift_ride(&mut r),
        Cue::TrafficBed => traffic_bed(&mut r),
        Cue::CrowdBed => crowd_bed(&mut r),
        Cue::CarEngine => car_engine(&mut r),
        Cue::BayHum => bay_hum(&mut r),
        Cue::Blow => blow(&mut r),
        Cue::Fall => fall(&mut r),
        Cue::Downed => downed(&mut r),
    };
    normalize(&mut out, PEAK);
    out
}

// ---------------------------------------------------------------------------------------------
// Recipes.
// ---------------------------------------------------------------------------------------------

/// A beam weapon: a bright crack, then a falling zap with a harmonic, over a short recoil thump.
fn beam(r: &mut Rng, dur: f32, from_hz: f32, to_hz: f32, tau: f32, body: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut hp = Lp::new(3_000.0);
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        // Exponential sweep: fast off the top, easing into the low end.
        let hz = to_hz + (from_hz - to_hz) * (-t / (dur * 0.18)).exp();
        phase += TAU * hz / SR;
        let zap = (phase.sin() + 0.35 * (2.0 * phase).sin() + 0.15 * (3.0 * phase).sin()) * (-t / tau).exp();
        let x = r.noise();
        let crack = (x - hp.run(x)) * (-t / 0.004).exp() * 0.8;
        let thump = (TAU * 90.0 * t).sin() * (-t / 0.05).exp() * body;
        out.push((zap * 0.8 + crack + thump) * edges(i, n));
    }
    out
}

/// A low boom: a sine that falls a little as it decays, with rumble.
fn boom(r: &mut Rng, dur: f32, hz: f32, tau: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut lp = Lp::new(140.0);
    let mut lp2 = Lp::new(90.0);
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        phase += TAU * hz * (1.0 - 0.3 * (t / dur)) / SR;
        let rumble = lp2.run(lp.run(r.noise()) * 4.0) * 4.0;
        out.push((phase.sin() + rumble * 0.6) * (-t / tau).exp() * attack(t, 0.002) * edges(i, n));
    }
    out
}

/// The Heavy Beam Rifle: a boom you feel, a roar that falls through the spectrum, and the beam's
/// sizzle riding on top, with a tail.
fn heavy_beam(r: &mut Rng) -> Vec<f32> {
    let dur = 2.4;
    let n = samples(dur);
    let mut out = boom(r, dur, 38.0, 0.7);
    let mut lp_y = 0.0f32;
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let cutoff = 300.0 + 3_500.0 * (-t / 0.5).exp();
        let a = 1.0 - (-TAU * cutoff / SR).exp();
        lp_y += a * (r.noise() - lp_y);
        let roar = lp_y * 2.2 * (-t / 0.9).exp();
        let sizzle = (TAU * 620.0 * t).sin() * (0.6 + 0.4 * (TAU * 93.0 * t).sin()) * (-t / 0.6).exp() * 0.45;
        *v = (*v * 1.1 + roar + sizzle) * edges(i, n);
    }
    reverb(&out, 0.25)
}

/// A ballistic round: a hard transient and a thump.
fn gun(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.14);
    let mut lp = Lp::new(4_500.0);
    let mut hp = Lp::new(700.0);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let low = lp.run(r.noise());
        let band = low - hp.run(low);
        let crack = band * (-t / 0.012).exp() * 1.4;
        let thump = (TAU * 140.0 * t).sin() * (-t / 0.025).exp();
        out.push((crack + thump) * edges(i, n));
    }
    out
}

/// A burst of flame: a roar of low noise with crackle.
fn flame(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.4);
    let mut lp = Lp::new(1_300.0);
    let mut gate = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let env = attack(t, 0.03) * (-t / 0.18).exp();
        if r.unit() < 0.02 {
            gate = 1.0;
        }
        gate *= 0.995;
        let x = r.noise();
        out.push((lp.run(x) * 2.0 + x * gate * 0.3) * env * edges(i, n));
    }
    out
}

/// A missile leaving its rack: an ignition pop, then a hiss that brightens and fades.
fn missile_launch(r: &mut Rng) -> Vec<f32> {
    let dur = 1.0;
    let n = samples(dur);
    let mut lp_y = 0.0f32;
    let mut hp = Lp::new(250.0);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let cutoff = 500.0 + 3_000.0 * (t / 0.25).min(1.0);
        lp_y += (1.0 - (-TAU * cutoff / SR).exp()) * (r.noise() - lp_y);
        let hiss = (lp_y - hp.run(lp_y)) * 1.6 * attack(t, 0.02) * (-t / 0.35).exp();
        let pop = (TAU * 120.0 * t).sin() * (-t / 0.03).exp();
        out.push((hiss + pop) * edges(i, n));
    }
    out
}

/// A beam blade: a swing of noise with the blade's buzz under it.
fn saber(r: &mut Rng) -> Vec<f32> {
    let mut out = whoosh(r, 0.45, 350.0, 2_400.0);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let x = i as f32 / n as f32;
        let buzz = saw(t * 115.0) * 0.35 + saw(t * 172.0) * 0.2;
        *v = (*v + buzz * (PI * x).sin()) * edges(i, n);
    }
    out
}

/// A transient with a body under it: every impact in the bank.
fn impact(
    r: &mut Rng,
    dur: f32,
    body_hz: f32,
    body_tau: f32,
    noise_tau: f32,
    lp_hz: f32,
    hp_hz: f32,
) -> Vec<f32> {
    let n = samples(dur);
    let mut lp = Lp::new(lp_hz);
    let mut hp = Lp::new(hp_hz);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let low = lp.run(r.noise());
        let band = low - hp.run(low);
        let noise = band * 1.5 * (-t / noise_tau).exp();
        let body = (TAU * body_hz * t).sin() * (-t / body_tau).exp();
        out.push((noise + body) * edges(i, n));
    }
    out
}

/// An explosion: crack, boom, roll, then debris.
fn blast(r: &mut Rng, dur: f32, boom_hz: f32, scale: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut crack_lp = Lp::new(6_000.0);
    let mut roll_lp = Lp::new(180.0);
    let mut roll_lp2 = Lp::new(90.0);
    let mut grit_hp = Lp::new(1_500.0);
    let boom_hz = boom_hz * (0.9 + 0.2 * r.unit());
    let mut gate = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let x = r.noise();
        let crack = crack_lp.run(x) * (-t / (0.045 * scale.max(0.4))).exp() * 1.3;
        let boom = (TAU * boom_hz * t).sin() * (-t / (0.45 * scale)).exp() * 1.1;
        let roll = roll_lp2.run(roll_lp.run(x) * 3.0) * 5.0 * (-t / (0.9 * scale)).exp();
        if r.unit() < 0.004 {
            gate = 1.0;
        }
        gate *= 0.998;
        let fall = if t > 0.3 * scale {
            (x - grit_hp.run(x)) * gate * 0.35 * (-(t - 0.3 * scale) / scale).exp()
        } else {
            0.0
        };
        out.push((crack + boom + roll + fall) * attack(t, 0.001) * edges(i, n));
    }
    out
}

/// A rock coming apart: a deep crack and a long grinding rumble.
fn rock_break(r: &mut Rng) -> Vec<f32> {
    let dur = 2.2;
    let n = samples(dur);
    let mut out = blast(r, dur, 34.0, 0.9);
    let mut lp = Lp::new(700.0);
    let mut gate = 0.0f32;
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / SR;
        if r.unit() < 0.006 {
            gate = 1.0;
        }
        gate *= 0.9975;
        let grind = lp.run(r.noise()) * gate * 1.4 * (-t / 0.8).exp();
        *v = (*v + grind) * edges(i, n);
    }
    out
}

/// Blades meeting: an electric crackle and an inharmonic ring.
fn clash(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.8);
    let mut hp = Lp::new(2_000.0);
    let mut gate = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        if r.unit() < 0.02 {
            gate = 1.0;
        }
        gate *= 0.99;
        let x = r.noise();
        let crackle = (x - hp.run(x)) * gate * 1.2 * (-t / 0.25).exp();
        let ring =
            ((TAU * 1_240.0 * t).sin() + 0.7 * (TAU * 1_910.0 * t).sin() + 0.4 * (TAU * 3_020.0 * t).sin())
                * (-t / 0.22).exp()
                * 0.5;
        let spark = x * (-t / 0.006).exp();
        out.push((crackle + ring + spark) * edges(i, n));
    }
    out
}

/// Taking a hit: a clang through the frame, a crunch, and a thump.
fn hull_hit(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.7);
    let mut lp = Lp::new(2_200.0);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let clang = ((TAU * 420.0 * t).sin() * (-t / 0.18).exp()
            + (TAU * 987.0 * t).sin() * (-t / 0.09).exp() * 0.6
            + (TAU * 1_630.0 * t).sin() * (-t / 0.05).exp() * 0.4)
            * 0.6;
        let crunch = lp.run(r.noise()) * 1.8 * (-t / 0.05).exp();
        let thump = (TAU * 68.0 * t).sin() * (-t / 0.12).exp() * 1.2;
        out.push((clang + crunch + thump) * edges(i, n));
    }
    out
}

/// Changing form: servos whining up, clunks at each end.
fn transform(r: &mut Rng) -> Vec<f32> {
    let dur = 1.0;
    let n = samples(dur);
    let mut lp = Lp::new(1_400.0);
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        let x = t / dur;
        phase += (160.0 + 300.0 * x) / SR;
        let servo = lp.run(saw(phase)) * (PI * x).sin() * 0.7;
        out.push(servo);
    }
    let clunk = impact(r, 0.25, 95.0, 0.06, 0.02, 3_000.0, 400.0);
    mix_in(&mut out, &clunk, samples(0.05), 1.0);
    mix_in(&mut out, &clunk, samples(0.72), 1.2);
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A digital warble sweeping from one pitch to another (the Eclipse).
fn warble(r: &mut Rng, dur: f32, from_hz: f32, to_hz: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    let mut held = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        let x = t / dur;
        let hz = from_hz * (to_hz / from_hz).powf(x) * (1.0 + 0.08 * (TAU * 31.0 * t).sin());
        phase += hz / SR;
        // Sample-and-hold every 8 samples: a crushed, digital edge.
        if i % 8 == 0 {
            held = square(phase) * 0.6 + r.noise() * 0.08;
        }
        out.push(held * (PI * x).sin().powf(0.5) * edges(i, n));
    }
    out
}

/// Stowing: a hatch thunk and a short hiss as it seals.
fn stow(r: &mut Rng) -> Vec<f32> {
    let mut out = impact(r, 0.55, 80.0, 0.1, 0.03, 2_000.0, 300.0);
    let n = out.len();
    let mut hp = Lp::new(3_000.0);
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let x = r.noise();
        let hiss = if t > 0.12 { (x - hp.run(x)) * 0.5 * (-(t - 0.12) / 0.1).exp() } else { 0.0 };
        *v = (*v + hiss) * edges(i, n);
    }
    out
}

/// The hold dumped: a latch, then pressurised hiss.
fn jettison(r: &mut Rng) -> Vec<f32> {
    let dur = 1.1;
    let n = samples(dur);
    let mut hp = Lp::new(1_800.0);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let x = r.noise();
        let hiss = (x - hp.run(x)) * attack(t, 0.03) * (-t / 0.4).exp();
        out.push(hiss);
    }
    let latch = impact(r, 0.2, 130.0, 0.04, 0.015, 3_500.0, 500.0);
    mix_in(&mut out, &latch, 0, 1.0);
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A band of noise sweeping up, peaking a third of the way in: a swing, a throw.
fn whoosh(r: &mut Rng, dur: f32, from_hz: f32, to_hz: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut out = Vec::with_capacity(n);
    let mut lp_y = 0.0f32;
    let mut hp_y = 0.0f32;
    for i in 0..n {
        let x = i as f32 / n as f32;
        let centre = from_hz + (to_hz - from_hz) * x;
        let a_lp = 1.0 - (-TAU * (centre * 1.7) / SR).exp();
        let a_hp = 1.0 - (-TAU * (centre * 0.45) / SR).exp();
        lp_y += a_lp.clamp(0.0, 1.0) * (r.noise() - lp_y);
        hp_y += a_hp.clamp(0.0, 1.0) * (lp_y - hp_y);
        let env = (PI * x.powf(0.6)).sin().max(0.0);
        out.push((lp_y - hp_y) * env * edges(i, n));
    }
    out
}

/// Soft sines at (Hz, start s, duration s): the chimes.
fn chime(notes: &[(f32, f32, f32)]) -> Vec<f32> {
    let total = notes.iter().map(|(_, s, d)| s + d).fold(0.0f32, f32::max);
    let n = samples(total);
    let mut out = vec![0.0f32; n];
    for &(hz, start, dur) in notes {
        let from = samples(start);
        for k in 0..samples(dur) {
            let Some(v) = out.get_mut(from + k) else { break };
            let t = k as f32 / SR;
            let env = attack(t, 0.004) * (-t / (dur * 0.45)).exp();
            *v += ((TAU * hz * t).sin() + 0.25 * (TAU * 2.0 * hz * t).sin()) * env;
        }
    }
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// The bay's klaxon: a harsh horn alternating two tones, four times over.
fn klaxon() -> Vec<f32> {
    const TONE: f32 = 0.42;
    let n = samples(TONE * 4.0);
    let mut lp = Lp::new(2_200.0);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let k = (t / TONE) as u32;
            let hz = if k.is_multiple_of(2) { 640.0 } else { 505.0 };
            phase = (phase + hz / SR).fract();
            let saw = 2.0 * phase - 1.0;
            let x = lp.run(0.6 * saw + 0.4 * square(phase));
            // Each tone swells in and cuts off short of the next.
            let u = t - k as f32 * TONE;
            let env = attack(u, 0.03) * ((TONE - 0.03 - u) / 0.03).clamp(0.0, 1.0);
            x * env * edges(i, n)
        })
        .collect()
}

/// The bay doors (or the tunnel's) on the move: a motor's hum and a grinding rumble, with a clank
/// as they start and as they seat.
fn door_rumble(r: &mut Rng) -> Vec<f32> {
    const DUR: f32 = 3.6;
    let n = samples(DUR);
    let mut lp = Lp::new(170.0);
    let mut lp2 = Lp::new(110.0);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let grind = lp2.run(lp.run(r.noise()) * 3.0) * 3.0;
            let motor = (TAU * 55.0 * t).sin() * 0.35 + (TAU * 110.0 * t).sin() * 0.12;
            let swell = attack(t, 0.5) * ((DUR - t) / 0.6).clamp(0.0, 1.0);
            (grind + motor) * swell * edges(i, n)
        })
        .collect();
    mix_in(&mut out, &impact(r, 0.45, 70.0, 0.12, 0.03, 1_800.0, 200.0), 0, 0.8);
    mix_in(&mut out, &impact(r, 0.45, 60.0, 0.14, 0.03, 1_600.0, 180.0), samples(DUR - 0.45), 1.0);
    out
}

/// A plain electronic beep.
fn beep(dur: f32, hz: f32, square_part: f32) -> Vec<f32> {
    let n = samples(dur);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let s = (TAU * hz * t).sin() * (1.0 - square_part) + square(hz * t) * square_part;
            s * attack(t, 0.003) * edges(i, n)
        })
        .collect()
}

/// Decaying partials at (Hz, decay s).
fn ring(dur: f32, partials: &[(f32, f32)]) -> Vec<f32> {
    let n = samples(dur);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let s: f32 = partials.iter().map(|&(hz, tau)| (TAU * hz * t).sin() * (-t / tau).exp()).sum();
            s * edges(i, n)
        })
        .collect()
}

/// An RCS thruster's puff.
fn puff(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.16);
    let mut hp = Lp::new(1_200.0);
    let mut lp = Lp::new(6_000.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = lp.run(r.noise());
            (x - hp.run(x)) * attack(t, 0.004) * (-t / 0.04).exp() * edges(i, n)
        })
        .collect()
}

/// The Orrery engaging: a dark chord swelling out of a single high ping.
fn orrery_on(r: &mut Rng) -> Vec<f32> {
    let dur = 2.6;
    let n = samples(dur);
    let chord = [55.0f32, 82.41, 110.0, 164.81, 220.0 * 1.003];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let x = t / dur;
        let swell = (x / 0.35).min(1.0).powf(1.5) * (1.0 - ((x - 0.35).max(0.0) / 0.65)).max(0.0);
        let pad: f32 = chord.iter().map(|hz| (TAU * hz * t).sin()).sum::<f32>() * 0.25;
        let ping = (TAU * 2_637.0 * t).sin() * (-t / 0.35).exp() * 0.6;
        out.push((pad * swell + ping + r.noise() * 0.02 * swell) * edges(i, n));
    }
    reverb(&out, 0.35)
}

/// An Orrery seizure: the signal tearing: pitch jumps, dropouts, crushed bursts.
fn seizure(r: &mut Rng) -> Vec<f32> {
    let dur = 1.3;
    let n = samples(dur);
    let mut out = Vec::with_capacity(n);
    let mut hz = 400.0f32;
    let mut on = true;
    let mut phase = 0.0f32;
    let mut held = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        if i % samples(0.045) == 0 {
            hz = 120.0 + 1_600.0 * r.unit();
            on = r.unit() > 0.25;
        }
        phase += hz / SR;
        if i % 12 == 0 {
            held = if on { square(phase) * 0.7 + r.noise() * 0.3 } else { 0.0 };
        }
        let sub = (TAU * 45.0 * t).sin() * 0.5;
        out.push((held + sub) * (-t / 0.7).exp() * edges(i, n));
    }
    out
}

/// The own suit destroyed: a falling alarm over a muffled blast.
fn destroyed(r: &mut Rng) -> Vec<f32> {
    let dur = 2.6;
    let mut out = blast(r, dur, 36.0, 0.9);
    let n = out.len();
    let mut lp = Lp::new(800.0);
    let mut phase = 0.0f32;
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / SR;
        phase += (900.0 * (-t / 0.9).exp() + 120.0) / SR;
        let alarm = square(phase) * 0.35 * (-t / 1.1).exp();
        *v = (lp.run(*v) * 1.4 + alarm) * edges(i, n);
    }
    out
}

/// Launching: engines lighting and the suit pushing off.
fn launch(r: &mut Rng) -> Vec<f32> {
    let dur = 1.4;
    let n = samples(dur);
    let mut lp_y = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let x = t / dur;
        let cutoff = 200.0 + 2_500.0 * x;
        lp_y += (1.0 - (-TAU * cutoff / SR).exp()) * (r.noise() - lp_y);
        let env = (x / 0.25).min(1.0) * (1.0 - x).powf(1.5);
        let ignite = (TAU * 70.0 * t).sin() * (-t / 0.15).exp();
        out.push((lp_y * 2.0 * env + ignite) * edges(i, n));
    }
    out
}

/// A suit radio's squelch: a burst of static held to the radio's band (300 Hz to 3.4 kHz), cut
/// off sharply as the carrier quiets it. Opening, a short tick of it; closing, a longer tail with
/// the faint chirp of the carrier dropping.
fn squelch(r: &mut Rng, close: bool) -> Vec<f32> {
    let dur = if close { 0.16 } else { 0.07 };
    let n = samples(dur);
    let (mut lo, mut hi) = (Lp::new(3_400.0), Lp::new(300.0));
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = r.noise();
            let band = lo.run(x) - hi.run(x);
            let body = attack(t, 0.002) * (-t / (dur * 0.5)).exp();
            let chirp = if close {
                phase += TAU * (1_800.0 - 900.0 * t / dur) / SR;
                phase.sin() * 0.2 * (-((t - 0.03).max(0.0)) / 0.04).exp() * attack(t, 0.03)
            } else {
                0.0
            };
            (band * 2.5 * body + chirp) * edges(i, n)
        })
        .collect()
}

/// A menu tick.
fn click(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.05);
    let mut hp = Lp::new(1_500.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = r.noise();
            ((x - hp.run(x)) * 0.5 + (TAU * 2_200.0 * t).sin()) * (-t / 0.008).exp() * edges(i, n)
        })
        .collect()
}

/// The grip arming (or a surface taking hold): magnets seating, a hard clunk with a ring through
/// the frame.
fn mag_lock(r: &mut Rng) -> Vec<f32> {
    let mut out = impact(r, 0.45, 120.0, 0.07, 0.015, 2_600.0, 500.0);
    mix_in(&mut out, &ring(0.4, &[(880.0, 0.08), (1_390.0, 0.05), (2_210.0, 0.03)]), samples(0.01), 0.35);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// The grip letting go: a softer unlatch and the hiss of the field collapsing.
fn mag_release(r: &mut Rng) -> Vec<f32> {
    let mut out = whoosh(r, 0.5, 3_800.0, 900.0);
    mix_in(&mut out, &impact(r, 0.2, 150.0, 0.04, 0.01, 2_000.0, 600.0), 0, 0.6);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// Landing: a low thud through the legs, then the hydraulics taking the weight with a hiss.
fn touchdown(r: &mut Rng) -> Vec<f32> {
    let mut out = boom(r, 0.6, 48.0, 0.14);
    mix_in(&mut out, &impact(r, 0.3, 75.0, 0.08, 0.03, 1_400.0, 160.0), 0, 0.9);
    let mut hiss = whoosh(r, 0.5, 4_500.0, 1_800.0);
    for v in &mut hiss {
        *v *= 0.35;
    }
    mix_in(&mut out, &hiss, samples(0.08), 1.0);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A foot coming down: a thump and a metallic ring through the frame. Heard from another suit
/// through the body (`far`), only the low thump comes through, dulled.
fn footstep(r: &mut Rng, far: bool) -> Vec<f32> {
    let dur = if far { 0.35 } else { 0.3 };
    let mut out = impact(r, dur, 62.0, 0.07, 0.012, if far { 500.0 } else { 2_000.0 }, 120.0);
    if !far {
        mix_in(&mut out, &ring(0.25, &[(610.0, 0.05), (1_475.0, 0.03)]), samples(0.005), 0.2);
    }
    let n = out.len();
    let mut lp = Lp::new(if far { 300.0 } else { 6_000.0 });
    for (i, v) in out.iter_mut().enumerate() {
        *v = lp.run(*v) * edges(i, n);
    }
    out
}

/// Pushing off a surface: the legs' servos slamming out, and a thruster's breath.
fn push_off(r: &mut Rng) -> Vec<f32> {
    let dur = 0.7;
    let n = samples(dur);
    let mut lp = Lp::new(900.0);
    let mut phase = 0.0f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            phase += (220.0 - 120.0 * (t / dur)) / SR;
            lp.run(saw(phase)) * (-t / 0.12).exp() * 0.8
        })
        .collect();
    mix_in(&mut out, &impact(r, 0.25, 90.0, 0.05, 0.02, 2_500.0, 300.0), 0, 0.9);
    mix_in(&mut out, &whoosh(r, 0.55, 600.0, 2_600.0), samples(0.05), 0.5);
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// The reactor going down to idle (`up`: spinning back up): a turbine's whine falling away to a
/// hum, or rising out of one.
fn reactor(r: &mut Rng, up: bool) -> Vec<f32> {
    let dur = 1.6;
    let n = samples(dur);
    let mut lp = Lp::new(400.0);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = if up { t / dur } else { 1.0 - t / dur };
            phase += (90.0 + 900.0 * x * x) / SR;
            let whine = (TAU * phase).sin() * 0.6 + (TAU * phase * 2.01).sin() * 0.2;
            let rumble = lp.run(r.noise()) * 2.0 * (0.3 + 0.7 * x);
            let env = attack(t, 0.08) * ((dur - t) / 0.4).clamp(0.0, 1.0);
            (whine * (0.2 + 0.8 * x) + rumble) * env * edges(i, n)
        })
        .collect()
}

// --- Loops: each ends where it starts ([`loop_seam`]). ---

/// The main engines: a low rumble with some grit.
fn thruster_loop(r: &mut Rng) -> Vec<f32> {
    let secs = 4.0;
    let mut lp = Lp::new(220.0);
    let mut lp2 = Lp::new(160.0);
    let mut band = Lp::new(900.0);
    let mut band_hp = Lp::new(450.0);
    let raw: Vec<f32> = (0..samples(secs + 0.5))
        .map(|i| {
            let t = i as f32 / SR;
            let x = r.noise();
            let rumble = lp2.run(lp.run(x) * 3.0) * 3.0;
            let b = band.run(x);
            let grit = (b - band_hp.run(b)) * 0.35;
            (rumble + grit) * (1.0 + 0.08 * (TAU * 0.5 * t).sin())
        })
        .collect();
    loop_seam(raw, samples(0.5))
}

/// Boost: a brighter, harder roar.
fn boost_loop(r: &mut Rng) -> Vec<f32> {
    let secs = 3.0;
    let mut lp = Lp::new(1_600.0);
    let mut hp = Lp::new(120.0);
    let mut gate = 0.0f32;
    let raw: Vec<f32> = (0..samples(secs + 0.4))
        .map(|_| {
            let x = r.noise();
            if r.unit() < 0.01 {
                gate = 1.0;
            }
            gate *= 0.996;
            let l = lp.run(x);
            (l - hp.run(l)) * 1.5 + x * gate * 0.15
        })
        .collect();
    loop_seam(raw, samples(0.4))
}

/// The Heavy Beam Rifle charging: a whine the player drives upward with playback rate.
fn charge_loop() -> Vec<f32> {
    // Whole cycles of every partial in exactly one second, so the loop needs no seam.
    let n = samples(1.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let vib = 1.0 + 0.004 * (TAU * 6.0 * t).sin();
            (TAU * 440.0 * t * vib).sin() * 0.6
                + (TAU * 880.0 * t).sin() * 0.3
                + (TAU * 1_322.0 * t).sin() * 0.1
        })
        .collect()
}

/// A beam blade held out: a detuned buzz with a slow beat and a little sizzle.
fn saber_hum(r: &mut Rng) -> Vec<f32> {
    let n = samples(2.0);
    let mut hp = Lp::new(4_000.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = r.noise();
            // 110 and 111 Hz beat once a second: whole cycles in two seconds, a seamless loop.
            let hum = saw(t * 110.0) * 0.4 + saw(t * 111.0) * 0.4 + (TAU * 220.0 * t).sin() * 0.2;
            hum + (x - hp.run(x)) * 0.05
        })
        .collect()
}

/// A steady tone (a solid missile lock).
fn tone_loop(secs: f32, hz: f32, square_part: f32) -> Vec<f32> {
    let n = samples(secs);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            (TAU * hz * t).sin() * (1.0 - square_part) + square(hz * t) * square_part
        })
        .collect()
}

/// Missile warning: two tones alternating eight times a second.
fn missile_alarm() -> Vec<f32> {
    let n = samples(0.5);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let hi = ((t * 8.0) as u32).is_multiple_of(2);
            let hz = if hi { 1_200.0 } else { 900.0 };
            let within = (t * 8.0).fract();
            let gate = (within / 0.05).min(1.0) * ((1.0 - within) / 0.05).min(1.0);
            (square(hz * t) * 0.5 + (TAU * hz * t).sin() * 0.5) * gate
        })
        .collect()
}

/// G-strain: the pilot's heartbeat, lub-dub, once a second (the playback rate quickens it).
fn heartbeat() -> Vec<f32> {
    let n = samples(1.0);
    let mut out = vec![0.0f32; n];
    for (start, amp) in [(0.0f32, 1.0f32), (0.28, 0.7)] {
        let from = samples(start);
        for k in 0..samples(0.18) {
            let t = k as f32 / SR;
            if let Some(v) = out.get_mut(from + k) {
                *v += (TAU * 52.0 * t).sin() * amp * attack(t, 0.006) * (-t / 0.05).exp();
            }
        }
    }
    out
}

/// The Orrery running: a low drone with a slow, uneasy beat.
fn orrery_drone(r: &mut Rng) -> Vec<f32> {
    let secs = 6.0;
    let mut lp = Lp::new(300.0);
    let raw: Vec<f32> = (0..samples(secs))
        .map(|i| {
            let t = i as f32 / SR;
            // 55 and 55.5 Hz: whole cycles in 6 s, so the drone loops cleanly; the noise is seamed.
            let drone =
                (TAU * 55.0 * t).sin() * 0.5 + (TAU * 55.5 * t).sin() * 0.5 + (TAU * 110.0 * t).sin() * 0.3;
            let air = lp.run(r.noise()) * 0.5 * (1.0 + 0.5 * (TAU * t / secs).sin());
            drone + air
        })
        .collect();
    let fade = samples(0.3);
    let mut with_tail = raw.clone();
    with_tail.extend_from_slice(&raw[..fade]);
    loop_seam(with_tail, fade)
}

/// The cockpit at rest: an electrical hum and the air handling.
fn cockpit_hum(r: &mut Rng) -> Vec<f32> {
    let secs = 8.0;
    let mut lp = Lp::new(500.0);
    let raw: Vec<f32> = (0..samples(secs + 0.5))
        .map(|i| {
            let t = i as f32 / SR;
            let hum = (TAU * 100.0 * t).sin() * 0.3
                + (TAU * 200.0 * t).sin() * 0.12
                + (TAU * 300.0 * t).sin() * 0.05;
            hum + lp.run(r.noise()) * 0.9
        })
        .collect();
    loop_seam(raw, samples(0.5))
}

// --- On foot: the town, a car, a lift, the bay ([`crate::street`]). ---

/// A shoe on the town's paving: the heel, then the sole rolling down onto it, dry and short, with
/// a little grit; far lighter than a suit's foot.
fn street_step(r: &mut Rng) -> Vec<f32> {
    let n = samples(0.17);
    let mut out = vec![0.0f32; n];
    for (start, amp, tau) in [(0.0f32, 1.0f32, 0.010f32), (0.042, 0.55, 0.018)] {
        let mut lp = Lp::new(4_200.0);
        let mut hp = Lp::new(650.0);
        let from = samples(start);
        let mut phase = 0.0f32;
        for (k, v) in out[from..].iter_mut().enumerate() {
            let t = k as f32 / SR;
            let x = lp.run(r.noise());
            let tick = (x - hp.run(x)) * (-t / tau).exp() * attack(t, 0.0006) * 1.5;
            phase += 150.0 / SR;
            let body = (TAU * phase).sin() * (-t / 0.016).exp() * 0.45;
            let grit = r.noise() * (-t / 0.05).exp() * 0.06;
            *v += (tick + body + grit) * amp;
        }
    }
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A car's door: the handle's click, the door swung to with a solid thunk through the body, and
/// the latch catching.
fn car_door(r: &mut Rng) -> Vec<f32> {
    let mut out = click(r);
    for v in &mut out {
        *v *= 0.35;
    }
    let mut thunk = impact(r, 0.42, 82.0, 0.09, 0.022, 1_500.0, 110.0);
    // The body's panels: a hollow ring under the thunk.
    let mut body = Reso::new(185.0, 7.0, SR);
    for (i, v) in thunk.iter_mut().enumerate() {
        let t = i as f32 / SR;
        *v += body.run(r.noise() * (-t / 0.02).exp()) * 2.5;
    }
    mix_in(&mut out, &thunk, samples(0.11), 1.0);
    mix_in(&mut out, &impact(r, 0.08, 1_900.0, 0.012, 0.004, 6_000.0, 1_500.0), samples(0.125), 0.35);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A car going by: its tyres' roar swelling and falling away, and its motor's note dropping as it
/// passes (Doppler), the air pushed aside with it.
fn car_pass(r: &mut Rng) -> Vec<f32> {
    const DUR: f32 = 1.8;
    const BY: f32 = 0.85;
    let n = samples(DUR);
    let mut roar = Lp::new(900.0);
    let mut roar_hp = Lp::new(140.0);
    let mut air_lp = Lp::new(3_200.0);
    let mut air_hp = Lp::new(900.0);
    let mut motor = Lp::new(520.0);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let u = (t - BY) / 0.33;
            let near = (-u * u).exp();
            // Higher coming, lower going: ±9 %, the turn quick as it's alongside.
            let doppler = 1.0 - 0.09 * ((t - BY) / 0.12).tanh();
            phase = (phase + 88.0 * doppler / SR).fract();
            let x = r.noise();
            let tyres = roar.run(x);
            let tyres = tyres - roar_hp.run(tyres);
            let a = air_lp.run(x);
            let air = (a - air_hp.run(a)) * near * near;
            let note = motor.run(saw(phase) + 0.5 * saw(phase * 2.0));
            (tyres * 2.2 * near + air * 0.6 + note * 0.35 * near) * edges(i, n)
        })
        .collect()
}

/// A lift's ride: the car's motor settling as it slows into the floor, the rails' rumble, a
/// clunk as it stops, the chime and the doors drawing open.
fn lift_ride(r: &mut Rng) -> Vec<f32> {
    const RIDE: f32 = 1.6;
    let n = samples(RIDE);
    let mut rumble = Lp::new(180.0);
    let mut rumble2 = Lp::new(120.0);
    let mut whine_lp = Lp::new(1_400.0);
    let mut phase = 0.0f32;
    let mut hum = 0.0f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = t / RIDE;
            // Slowing: the whine falls as the motor eases off, and dies away into the stop.
            phase = (phase + (430.0 - 170.0 * x) / SR).fract();
            hum = (hum + 50.0 / SR).fract();
            let whine = whine_lp.run(saw(phase)) * 0.25 + (TAU * hum).sin() * 0.35;
            let rails = rumble2.run(rumble.run(r.noise()) * 3.0) * 3.0;
            let env = attack(t, 0.2) * (1.0 - x).max(0.0).powf(0.7);
            (whine + rails) * env
        })
        .collect();
    mix_in(&mut out, &impact(r, 0.3, 68.0, 0.08, 0.02, 1_200.0, 120.0), samples(RIDE - 0.1), 0.7);
    mix_in(&mut out, &chime(&[(659.25, 0.0, 0.5), (987.77, 0.14, 0.75)]), samples(RIDE + 0.05), 0.5);
    let mut doors = whoosh(r, 0.7, 1_100.0, 380.0);
    for v in &mut doors {
        *v *= 0.35;
    }
    mix_in(&mut out, &doors, samples(RIDE + 0.25), 1.0);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// Bed samples at [`BED_RATE`].
fn bed_samples(secs: f32) -> usize {
    (secs * BR).round().max(1.0) as usize
}

/// The town's traffic, as a bed: a low roar of tyres on the roads round about, its hiss on top,
/// each swelling and easing as the traffic does. The swells are whole cycles of the loop (Gates'
/// `bed()`), so the seam joins two stretches of noise at the same level, not a jump.
fn traffic_bed(r: &mut Rng) -> Vec<f32> {
    const LOOP: f32 = 9.0;
    const FADE: f32 = 1.5;
    let f0 = 1.0 / LOOP;
    let mut brown = 0.0f32;
    let mut roar = Lp::at_rate(320.0, BR);
    let mut roar2 = Lp::at_rate(170.0, BR);
    let mut roar_hp = Lp::at_rate(30.0, BR);
    let mut hiss_lp = Lp::at_rate(2_600.0, BR);
    let mut hiss_hp = Lp::at_rate(650.0, BR);
    let mut swell = (1.0f32, 1.0f32);
    let raw: Vec<f32> = (0..bed_samples(LOOP + FADE))
        .map(|i| {
            let t = i as f32 / BR;
            if i % 32 == 0 {
                swell = (
                    0.72 + 0.18 * (TAU * f0 * t).sin() + 0.1 * (TAU * 3.0 * f0 * t + 1.1).sin(),
                    0.6 + 0.25 * (TAU * 2.0 * f0 * t + 2.3).sin() + 0.15 * (TAU * 5.0 * f0 * t + 0.4).sin(),
                );
            }
            let x = r.noise();
            brown = (brown + x * 0.05) * 0.995;
            let low = roar2.run(roar.run(brown));
            let low = low - roar_hp.run(low);
            let h = hiss_lp.run(x);
            let hiss = h - hiss_hp.run(h);
            low * 6.0 * swell.0 + hiss * 0.3 * swell.1
        })
        .collect();
    loop_seam(raw, bed_samples(FADE))
}

/// The voices a vowel is shaped by: its first two formants, Hz.
const VOWELS: [(f32, f32); 6] =
    [(730.0, 1_090.0), (530.0, 1_840.0), (270.0, 2_290.0), (570.0, 840.0), (300.0, 870.0), (640.0, 1_190.0)];

/// The town's people, as a bed: a murmur of a few voices talking, none of them words. Each speaks
/// in phrases of syllables (a buzz through two formants, a vowel each syllable) with pauses
/// between, at its own pitch and distance; a little air under them.
fn crowd_bed(r: &mut Rng) -> Vec<f32> {
    const LOOP: f32 = 7.0;
    const FADE: f32 = 1.0;
    const VOICES: usize = 7;
    let n = bed_samples(LOOP + FADE);
    let mut out = vec![0.0f32; n];
    for v in 0..VOICES {
        let pitch = 95.0 + 140.0 * r.unit();
        let level = 0.35 + 0.65 * r.unit() * if v < 2 { 1.0 } else { 0.6 };
        let mut src = Lp::at_rate(1_100.0, BR);
        let mut phase = r.unit();
        // Where its next syllable starts, samples.
        let mut at = (r.unit() * 0.8 * BR) as usize;
        while at < n {
            // A phrase of a few syllables, then a pause.
            let syllables = 3 + (r.unit() * 7.0) as u32;
            for _ in 0..syllables {
                let len = bed_samples(0.11 + 0.17 * r.unit());
                let amp = level * (0.55 + 0.45 * r.unit());
                let (f1, f2) = VOWELS[(r.unit() * VOWELS.len() as f32) as usize % VOWELS.len()];
                let mut res1 = Reso::new(f1 * (0.9 + 0.2 * r.unit()), 6.0, BR);
                let mut res2 = Reso::new(f2 * (0.9 + 0.2 * r.unit()), 9.0, BR);
                let glide = 0.12 * r.noise();
                for k in 0..len.min(n.saturating_sub(at)) {
                    let u = k as f32 / len as f32;
                    phase = (phase + pitch * (1.0 + glide * u) / BR).fract();
                    let buzz = src.run(saw(phase)) + r.noise() * 0.08;
                    // A syllable swells and falls: no clicks where the vowel changes.
                    let env = 4.0 * u * (1.0 - u);
                    out[at + k] += (res1.run(buzz) + 0.6 * res2.run(buzz)) * env * amp;
                }
                at += len;
            }
            at += bed_samples(0.25 + 0.75 * r.unit());
        }
    }
    // Across a square: a little air, and the edge taken off.
    let mut air = Lp::at_rate(400.0, BR);
    let mut far = Lp::at_rate(2_400.0, BR);
    for v in out.iter_mut() {
        *v = far.run(*v) + air.run(r.noise()) * 0.05;
    }
    loop_seam(out, bed_samples(FADE))
}

/// The car's motor at its own pace (the street's playback rate drives it with speed): a growl on
/// a 45 Hz firing note with its harmonics, a sub under it, a faint electric whine and the
/// intake's breath, through a low-pass. Every note is whole cycles of the 2 s loop, and the
/// filters run a lap before the one kept, so it goes round without a seam.
fn car_engine(r: &mut Rng) -> Vec<f32> {
    const LOOP: f32 = 2.0;
    let n = bed_samples(LOOP);
    let fade = bed_samples(0.4);
    let breath = loop_seam((0..n + fade).map(|_| r.noise()).collect(), fade);
    let tone: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / BR;
            let firing = 0.7 + 0.3 * (TAU * 45.0 * t).sin();
            let growl = saw(45.0 * t) * 0.55 + saw(90.0 * t + 0.3) * 0.22 + (TAU * 22.5 * t).sin() * 0.35;
            let whine = (TAU * 360.0 * t).sin() * 0.05;
            (growl + breath[i] * 0.25) * firing + whine
        })
        .collect();
    let mut lp = Lp::at_rate(650.0, BR);
    let mut lp2 = Lp::at_rate(420.0, BR);
    let mut out = vec![0.0f32; n];
    for lap in 0..2 {
        for (i, x) in tone.iter().enumerate() {
            let y = lp2.run(lp.run(*x));
            if lap == 1 {
                out[i] = y;
            }
        }
    }
    out
}

/// The bay's machinery: the air handlers' fans (their blades' beat in the noise), the mains hum
/// and the deep note of the ring's own works. The notes and the beat are whole cycles of the loop;
/// only the noise is crossfaded at its seam.
fn bay_hum(r: &mut Rng) -> Vec<f32> {
    const LOOP: f32 = 6.0;
    const FADE: f32 = 1.0;
    let n = bed_samples(LOOP);
    let mut fan_lp = Lp::at_rate(1_100.0, BR);
    let mut fan_hp = Lp::at_rate(90.0, BR);
    let mut hiss_hp = Lp::at_rate(3_000.0, BR);
    let raw = (0..bed_samples(LOOP + FADE))
        .map(|_| {
            let x = r.noise();
            let f = fan_lp.run(x);
            (f - fan_hp.run(f)) * 1.4 + (x - hiss_hp.run(x)) * 0.05
        })
        .collect();
    let fans = loop_seam(raw, bed_samples(FADE));
    (0..n)
        .map(|i| {
            let t = i as f32 / BR;
            let beat = 0.85 + 0.15 * (TAU * 14.0 * t).sin();
            let mains = (TAU * 60.0 * t).sin() * 0.3
                + (TAU * 120.0 * t).sin() * 0.14
                + (TAU * 180.0 * t).sin() * 0.05;
            let works = (TAU * 31.0 * t).sin() * 0.18;
            fans[i] * beat + mains + works
        })
        .collect()
}

// --- Fights on foot: a blow, a fall, a pilot downed (named for whoever sounds them). ---

/// A blow landing: a slap on top and a dull thud under it.
fn blow(r: &mut Rng) -> Vec<f32> {
    let mut out = impact(r, 0.28, 92.0, 0.05, 0.007, 4_500.0, 1_100.0);
    mix_in(&mut out, &boom(r, 0.25, 58.0, 0.06), 0, 0.6);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// A body hitting the floor: a heavy thud, the slap of limbs, a smaller bounce and a scuff.
fn fall(r: &mut Rng) -> Vec<f32> {
    let mut out = impact(r, 0.45, 68.0, 0.1, 0.02, 1_600.0, 110.0);
    mix_in(&mut out, &impact(r, 0.12, 240.0, 0.02, 0.008, 5_000.0, 1_200.0), samples(0.035), 0.5);
    mix_in(&mut out, &impact(r, 0.25, 84.0, 0.06, 0.015, 1_400.0, 120.0), samples(0.17), 0.4);
    let mut scuff = whoosh(r, 0.3, 1_300.0, 550.0);
    for v in &mut scuff {
        *v *= 0.3;
    }
    mix_in(&mut out, &scuff, samples(0.2), 1.0);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

/// The pilot's own, downed: they hit the floor, and their ears ring.
fn downed(r: &mut Rng) -> Vec<f32> {
    let mut out = fall(r);
    mix_in(&mut out, &boom(r, 0.9, 44.0, 0.25), 0, 0.5);
    let ring: Vec<f32> = (0..samples(2.0))
        .map(|i| {
            let t = i as f32 / SR;
            let tone = (TAU * 3_150.0 * t).sin() + 0.3 * (TAU * 3_163.0 * t).sin();
            tone * attack(t, 0.12) * (-t / 0.8).exp() * 0.3
        })
        .collect();
    mix_in(&mut out, &ring, samples(0.04), 1.0);
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= edges(i, n);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Primitives.
// ---------------------------------------------------------------------------------------------

/// xorshift32: deterministic and the same everywhere.
pub(crate) struct Rng(u32);

impl Rng {
    pub(crate) fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x1234_5678 } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// White noise in [-1, 1).
    pub(crate) fn noise(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Uniform in [0, 1).
    pub(crate) fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
}

/// A one-pole low-pass (a high-pass is `x - lp(x)`).
pub(crate) struct Lp {
    y: f32,
    a: f32,
}

impl Lp {
    pub(crate) fn new(hz: f32) -> Self {
        Self::at_rate(hz, SR)
    }

    pub(crate) fn at_rate(hz: f32, rate: f32) -> Self {
        Self { y: 0.0, a: (1.0 - (-TAU * hz / rate).exp()).clamp(0.0, 1.0) }
    }

    pub(crate) fn run(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

/// A two-pole resonator: a band `q` wide about `hz` (a formant, a panel's ring).
pub(crate) struct Reso {
    b0: f32,
    a1: f32,
    a2: f32,
    y1: f32,
    y2: f32,
}

impl Reso {
    pub(crate) fn new(hz: f32, q: f32, rate: f32) -> Self {
        let r = (-PI * hz / q.max(0.1) / rate).exp();
        Self { b0: 1.0 - r, a1: 2.0 * r * (TAU * hz / rate).cos(), a2: -r * r, y1: 0.0, y2: 0.0 }
    }

    pub(crate) fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A naive sawtooth in [-1, 1) of phase `cycles` (fine at the low pitches it's used for, and
/// always filtered).
pub(crate) fn saw(cycles: f32) -> f32 {
    2.0 * cycles.fract() - 1.0
}

/// A square wave of phase `cycles`.
pub(crate) fn square(cycles: f32) -> f32 {
    if cycles.fract() < 0.5 { 1.0 } else { -1.0 }
}

pub(crate) fn samples(secs: f32) -> usize {
    (secs * SR).round().max(1.0) as usize
}

/// A linear rise to full over `attack_s`.
pub(crate) fn attack(t: f32, attack_s: f32) -> f32 {
    if attack_s <= 0.0 { 1.0 } else { (t / attack_s).clamp(0.0, 1.0) }
}

/// Fades at a one-shot's ends (0.5 ms in, 4 ms out), so no playback clicks.
pub(crate) fn edges(i: usize, n: usize) -> f32 {
    let out = samples(0.004).min(n / 4).max(1);
    let inn = samples(0.0005).min(n / 4).max(1);
    let head = if i < inn { i as f32 / inn as f32 } else { 1.0 };
    let tail = if i + out >= n { (n - i) as f32 / out as f32 } else { 1.0 };
    head * tail
}

/// Adds `src` into `dst` from `at`, scaled by `gain` (growing `dst` if needed).
pub(crate) fn mix_in(dst: &mut Vec<f32>, src: &[f32], at: usize, gain: f32) {
    if dst.len() < at + src.len() {
        dst.resize(at + src.len(), 0.0);
    }
    for (d, s) in dst[at..].iter_mut().zip(src) {
        *d += s * gain;
    }
}

/// Crossfades the tail into the head (equal power) so the buffer loops without a seam; the loop
/// is `fade` samples shorter than the buffer.
pub(crate) fn loop_seam(mut buf: Vec<f32>, fade: usize) -> Vec<f32> {
    let n = buf.len();
    if fade == 0 || fade * 2 >= n {
        return buf;
    }
    let len = n - fade;
    for i in 0..fade {
        let t = i as f32 / fade as f32;
        let (w_head, w_tail) = ((t * PI / 2.0).sin(), (t * PI / 2.0).cos());
        buf[i] = buf[i] * w_head + buf[len + i] * w_tail;
    }
    buf.truncate(len);
    buf
}

/// A small Schroeder reverb: four combs and two all-passes.
pub(crate) fn reverb(dry: &[f32], wet: f32) -> Vec<f32> {
    const COMBS: [(usize, f32); 4] = [(1_557, 0.88), (1_617, 0.87), (1_491, 0.88), (1_422, 0.87)];
    const ALLPASS: [(usize, f32); 2] = [(225, 0.5), (556, 0.5)];
    let mut acc = vec![0.0f32; dry.len()];
    for (len, fb) in COMBS {
        let mut buf = vec![0.0f32; len];
        let mut i = 0usize;
        for (k, x) in dry.iter().enumerate() {
            let y = buf[i];
            buf[i] = x + y * fb;
            i = (i + 1) % len;
            acc[k] += y * 0.25;
        }
    }
    for (len, g) in ALLPASS {
        let mut buf = vec![0.0f32; len];
        let mut i = 0usize;
        for v in acc.iter_mut() {
            let d = buf[i];
            let y = d - g * *v;
            buf[i] = *v + g * d;
            i = (i + 1) % len;
            *v = y;
        }
    }
    let n = dry.len();
    dry.iter().zip(&acc).enumerate().map(|(i, (d, w))| (d * (1.0 - wet) + w * wet) * edges(i, n)).collect()
}

/// Scales to a peak (a silent buffer is left alone).
pub(crate) fn normalize(buf: &mut [f32], peak: f32) {
    let max = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max > 1e-6 {
        let k = peak / max;
        for v in buf.iter_mut() {
            *v *= k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|v| v * v).sum::<f32>() / b.len().max(1) as f32).sqrt()
    }

    #[test]
    fn every_cue_is_a_sound() {
        for &cue in Cue::ALL {
            if matches!(cue, Cue::MusicCalm | Cue::MusicCombat | Cue::MusicTitle) {
                continue; // long: `music`'s and `title`'s own tests
            }
            let b = render(cue);
            assert!(!b.is_empty(), "{cue:?}");
            assert!(b.iter().all(|v| v.is_finite()), "{cue:?} has NaN");
            let peak = b.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!((peak - PEAK).abs() < 1e-3, "{cue:?} peaks at {peak}");
            assert!(rms(&b) > 0.01, "{cue:?} is nearly silent");
            if cue.def().looped {
                // The seam: the last sample runs into the first no harder than the waveform steps
                // anywhere else (a sawtooth jumps every cycle; a click is a jump nothing else has).
                let jump = (b[b.len() - 1] - b[0]).abs();
                let step = b.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
                assert!(jump <= step * 1.05 + 0.01, "{cue:?} clicks at its loop point ({jump} > {step})");
            } else {
                assert!(b[0].abs() < 0.05 && b[b.len() - 1].abs() < 0.05, "{cue:?} starts or ends on a step");
            }
        }
    }

    #[test]
    fn the_bank_is_deterministic() {
        for cue in [Cue::BeamRifle, Cue::Explosion, Cue::ThrusterLoop] {
            assert_eq!(render(cue), render(cue));
        }
    }

    /// The street's cues are built in the first frames (a new pilot wakes in the town), one whole
    /// cue between two frames' budgets: none may cost much more than the cockpit's hum already
    /// does (about 10 ms native, the slowest of them the crowd's; `examples/cost.rs` lists them
    /// all). Timed only optimised, with room for a busy machine.
    #[test]
    #[cfg(not(debug_assertions))]
    fn the_streets_cues_are_cheap_to_build() {
        // What a cue costs at boot is the sound it renders, so that's what's held: each the length
        // of a short loop at most, and all of them less than the cockpit's few loops together. (Not
        // a stopwatch: a loaded machine would fail it for nothing.)
        let mut total = 0;
        for cue in [
            Cue::StreetStep,
            Cue::CarDoor,
            Cue::CarPass,
            Cue::LiftRide,
            Cue::TrafficBed,
            Cue::CrowdBed,
            Cue::CarEngine,
            Cue::BayHum,
        ] {
            let n = render(cue).len();
            assert!(n <= 5 * crate::SAMPLE_RATE as usize, "{cue:?} renders {n} samples");
            total += n;
        }
        assert!(total <= 20 * crate::SAMPLE_RATE as usize, "the street renders {total} samples");
    }
}
