//! The score: two generated loops, and a director that crossfades them with the fight.
//!
//! - **Calm** (the title screen, cruising, mining): slow pads through Am9, Fmaj7, Cmaj7, Em7, six
//!   seconds each, with a few soft plucks, drenched in reverb.
//! - **Combat**: 120 BPM, a pulsing bass ostinato, drums on the beat, a tense pad under it.
//!
//! Each loop is rendered a little long and its tail folded back onto its head, so it repeats
//! seamlessly (the reverb tail of the last bar rings into the first, as it would live). Oscillators
//! read a precomputed wavetable, so the whole score builds in a fraction of a second, in wasm too.

use std::f32::consts::TAU;

use crate::synth::{Lp, Rng, reverb};

/// The score's sample rate: pads and a bass line need no more, and it halves the build time and
/// the memory (the browser resamples).
pub const MUSIC_RATE: u32 = 24_000;
const SR: f32 = MUSIC_RATE as f32;
/// The calm loop's length, s.
pub const CALM_SECS: f32 = 24.0;
/// The combat loop's length, s (8 bars at 120 BPM).
pub const COMBAT_SECS: f32 = 16.0;

/// One cycle of a waveform, read with a phase accumulator.
struct Table(Vec<f32>);

const TABLE: usize = 4_096;

impl Table {
    /// Harmonics `1..=k` at amplitude `1/h^tilt`: `tilt` 1 is a soft sawtooth, higher is duller.
    fn harmonics(k: usize, tilt: f32) -> Self {
        let mut t: Vec<f32> = (0..TABLE)
            .map(|i| {
                let x = i as f32 / TABLE as f32;
                (1..=k).map(|h| (TAU * h as f32 * x).sin() / (h as f32).powf(tilt)).sum()
            })
            .collect();
        let peak = t.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
        t.iter_mut().for_each(|v| *v /= peak);
        Self(t)
    }

    #[inline]
    fn at(&self, phase: f32) -> f32 {
        self.0[((phase.fract() * TABLE as f32) as usize).min(TABLE - 1)]
    }
}

/// A note's frequency from its MIDI number.
fn midi(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Adds a held pad chord: every note two slightly detuned voices, swelling in and out over `dur`.
fn pad(out: &mut [f32], table: &Table, notes: &[f32], start: f32, dur: f32, amp: f32) {
    let from = (start * SR) as usize;
    let len = (dur * SR) as usize;
    // Chords overlap by their fades, so the swell of one covers the ebb of the last.
    let fade = (0.35 * dur).min(2.5);
    for &n in notes {
        let hz = midi(n);
        for detune in [0.997f32, 1.003] {
            let step = hz * detune / SR;
            let mut phase = (n * 0.37 + detune).fract();
            for k in 0..len {
                let Some(v) = out.get_mut(from + k) else { break };
                let t = k as f32 / SR;
                let env = (t / fade).min(1.0).min((dur - t) / fade).max(0.0);
                *v += table.at(phase) * env * amp;
                phase += step;
            }
        }
    }
}

/// Adds a plucked note: a bright attack that dulls as it decays.
fn pluck(out: &mut [f32], bright: &Table, dull: &Table, n: f32, start: f32, decay: f32, amp: f32) {
    let from = (start * SR) as usize;
    let len = (decay * 5.0 * SR) as usize;
    let step = midi(n) / SR;
    let mut phase = 0.0f32;
    for k in 0..len {
        let Some(v) = out.get_mut(from + k) else { break };
        let t = k as f32 / SR;
        let mix = (-t / (decay * 0.5)).exp();
        let s = bright.at(phase) * mix + dull.at(phase) * (1.0 - mix);
        *v += s * (t / 0.004).min(1.0) * (-t / decay).exp() * amp;
        phase += step;
    }
}

/// Folds everything past `len` back onto the start: a seamless loop of periodic music.
fn fold(mut buf: Vec<f32>, len: usize) -> Vec<f32> {
    for i in len..buf.len() {
        let v = buf[i];
        buf[i % len] += v;
    }
    buf.truncate(len);
    buf
}

/// The calm loop.
pub fn calm() -> Vec<f32> {
    let len = (CALM_SECS * SR) as usize;
    let tail = (5.0 * SR) as usize;
    let mut out = vec![0.0f32; len + tail];
    let soft = Table::harmonics(6, 1.6);
    let bright = Table::harmonics(10, 1.0);
    let dull = Table::harmonics(3, 2.0);
    // Am9, Fmaj7, Cmaj7, Em7 (voiced low and open).
    let chords: [&[f32]; 4] = [
        &[45.0, 52.0, 59.0, 60.0, 64.0],
        &[41.0, 48.0, 52.0, 57.0, 64.0],
        &[36.0, 43.0, 47.0, 52.0, 55.0],
        &[40.0, 47.0, 50.0, 55.0, 59.0],
    ];
    for (k, notes) in chords.iter().enumerate() {
        // Each chord rings on into the next (the last into the first, when the loop folds).
        pad(&mut out, &soft, notes, k as f32 * 6.0, 8.5, 0.05);
    }
    // A few plucks from A minor pentatonic, placed by a fixed seed.
    let scale = [69.0f32, 72.0, 74.0, 76.0, 79.0, 81.0, 84.0];
    let mut r = Rng::new(0xCA1B);
    let mut t = 1.5f32;
    while t < CALM_SECS {
        let n = scale[(r.unit() * scale.len() as f32) as usize % scale.len()];
        pluck(&mut out, &bright, &dull, n, t, 0.6, 0.05);
        t += 1.5 + 2.5 * r.unit();
    }
    let wet = reverb(&out, 0.45);
    fold(wet, len)
}

/// The combat loop.
pub fn combat() -> Vec<f32> {
    let len = (COMBAT_SECS * SR) as usize;
    let tail = (2.0 * SR) as usize;
    let mut out = vec![0.0f32; len + tail];
    let bass = Table::harmonics(8, 1.2);
    let soft = Table::harmonics(6, 1.6);
    let beat = 0.5f32; // 120 BPM
    // Two bars each: A, F, G, E (bass roots), eighth-note pulses with an accent on the beat.
    let roots = [33.0f32, 29.0, 31.0, 28.0];
    for (bar2, &root) in roots.iter().enumerate() {
        let base = bar2 as f32 * 4.0;
        for e in 0..16 {
            let start = base + e as f32 * beat * 0.5;
            let accent = if e % 2 == 0 { 1.0 } else { 0.6 };
            // An octave jump on the last eighth of each bar pushes it forward.
            let n = if e % 8 == 7 { root + 12.0 } else { root };
            let from = (start * SR) as usize;
            let step = midi(n) / SR;
            let mut phase = 0.0f32;
            let mut lp = Lp::at_rate(900.0, SR);
            for k in 0..(0.22 * SR) as usize {
                let Some(v) = out.get_mut(from + k) else { break };
                let t = k as f32 / SR;
                *v += lp.run(bass.at(phase)) * (t / 0.003).min(1.0) * (-t / 0.09).exp() * 0.22 * accent;
                phase += step;
            }
        }
        // The tense pad: root, fifth, ninth.
        pad(&mut out, &soft, &[root + 24.0, root + 31.0, root + 38.0], base, 4.2, 0.035);
    }
    // Drums: a low thump on 1 and 3, a noise snap on 2 and 4.
    let mut r = Rng::new(0xD2);
    for b in 0..(COMBAT_SECS / beat) as usize {
        let from = (b as f32 * beat * SR) as usize;
        if b % 2 == 0 {
            let mut phase = 0.0f32;
            for k in 0..(0.35 * SR) as usize {
                let t = k as f32 / SR;
                phase += (40.0 + 50.0 * (-t / 0.04).exp()) / SR;
                if let Some(v) = out.get_mut(from + k) {
                    *v += (TAU * phase).sin() * (-t / 0.12).exp() * 0.5;
                }
            }
        } else {
            let mut hp = Lp::at_rate(1_800.0, SR);
            for k in 0..(0.2 * SR) as usize {
                let t = k as f32 / SR;
                let x = r.noise();
                if let Some(v) = out.get_mut(from + k) {
                    *v += (x - hp.run(x)) * (-t / 0.05).exp() * 0.18;
                }
            }
        }
    }
    let wet = reverb(&out, 0.2);
    fold(wet, len)
}

/// What the music director hears each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct MusicIn {
    /// In the world (not the title screen).
    pub in_world: bool,
    /// Combat this frame, as energy: hits given and taken, kills, missiles, explosions near.
    pub heat: f32,
    /// Under fire right now (a missile lock or one inbound).
    pub threatened: bool,
}

/// The two loops' gains.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicOut {
    pub calm: f32,
    pub combat: f32,
}

/// Follows the fight: intensity rises with combat and ebbs over tens of seconds, and the loops
/// crossfade on it slowly (music that flips with every shot is noise).
#[derive(Clone, Copy, Debug, Default)]
pub struct Director {
    pub intensity: f32,
    out: MusicOut,
}

impl Director {
    pub fn frame(&mut self, dt: f32, i: &MusicIn) -> MusicOut {
        let dt = dt.clamp(0.0, 0.25);
        self.intensity += i.heat;
        if i.threatened {
            self.intensity += 0.3 * dt;
        }
        // Half-life of 12 s.
        self.intensity = (self.intensity * (0.5f32).powf(dt / 12.0)).clamp(0.0, 1.5);
        let fight = ((self.intensity - 0.25) / 0.35).clamp(0.0, 1.0);
        let fight = fight * fight * (3.0 - 2.0 * fight);
        let target = if i.in_world {
            MusicOut { calm: 0.7 * (1.0 - fight), combat: fight }
        } else {
            // The title screen: the calm loop, full.
            MusicOut { calm: 1.0, combat: 0.0 }
        };
        // Glide toward the target over a few seconds.
        let k = 1.0 - (-dt / 2.5).exp();
        self.out.calm += (target.calm - self.out.calm) * k;
        self.out.combat += (target.combat - self.out.combat) * k;
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Director, secs: f32, i: MusicIn) -> MusicOut {
        let mut out = MusicOut::default();
        let mut t = 0.0;
        while t < secs {
            out = d.frame(1.0 / 30.0, &i);
            t += 1.0 / 30.0;
        }
        out
    }

    #[test]
    fn the_loops_have_their_lengths_and_no_clipping() {
        for (buf, secs) in [(calm(), CALM_SECS), (combat(), COMBAT_SECS)] {
            assert_eq!(buf.len(), (secs * SR) as usize);
            assert!(buf.iter().all(|v| v.is_finite()));
            let rms = (buf.iter().map(|v| v * v).sum::<f32>() / buf.len() as f32).sqrt();
            assert!(rms > 0.005, "silent");
        }
    }

    #[test]
    fn a_fight_brings_the_combat_loop_in_and_it_ebbs_after() {
        let mut d = Director::default();
        let calm = run(&mut d, 5.0, MusicIn { in_world: true, ..Default::default() });
        assert!(calm.calm > 0.5 && calm.combat < 0.05);
        let fight = run(&mut d, 8.0, MusicIn { in_world: true, heat: 0.05, threatened: true });
        assert!(fight.combat > 0.8 && fight.calm < 0.2, "{fight:?}");
        let after = run(&mut d, 60.0, MusicIn { in_world: true, ..Default::default() });
        assert!(after.combat < 0.1 && after.calm > 0.5, "{after:?}");
    }

    #[test]
    fn the_title_plays_the_calm_loop() {
        let mut d = Director::default();
        let out = run(&mut d, 10.0, MusicIn::default());
        assert!(out.calm > 0.95 && out.combat < 0.01);
    }
}
