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
        Cue::BusterFire => buster(&mut r),
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
        Cue::ZeroOn => zero_on(&mut r),
        Cue::Seizure => seizure(&mut r),
        Cue::Destroyed => destroyed(&mut r),
        Cue::Launch => launch(&mut r),
        Cue::Klaxon => klaxon(),
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
        Cue::ZeroDrone => zero_drone(&mut r),
        Cue::CockpitHum => cockpit_hum(&mut r),
        Cue::MusicCalm => crate::music::calm(),
        Cue::MusicCombat => crate::music::combat(),
        Cue::MusicTitle => crate::title::title(),
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

/// The Twin Buster Rifle: a boom you feel, a roar that falls through the spectrum, and the beam's
/// sizzle riding on top, with a tail.
fn buster(r: &mut Rng) -> Vec<f32> {
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

/// A digital warble sweeping from one pitch to another (the Hyper Jammer).
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

/// The ZERO System engaging: a dark chord swelling out of a single high ping.
fn zero_on(r: &mut Rng) -> Vec<f32> {
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

/// A ZERO seizure: the signal tearing: pitch jumps, dropouts, crushed bursts.
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

/// The Twin Buster Rifle charging: a whine the player drives upward with playback rate.
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

/// The ZERO System running: a low drone with a slow, uneasy beat.
fn zero_drone(r: &mut Rng) -> Vec<f32> {
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
}
