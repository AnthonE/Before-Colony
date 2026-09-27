//! Renders a reel of the sound bank to a WAV file, to listen to without the game:
//!
//! ```sh
//! cargo run -p bc-sound --release --example reel -- reel.wav
//! ```
//!
//! The title music, then a fight: engines, beams and guns, missiles, hits, a kill, the cockpit's
//! alarms and the ZERO System, then the salvage run home.

use bc_sound::{Cue, SAMPLE_RATE, sample_rate, synth};

/// A cue's samples at the reel's rate (the score is rendered at a lower one).
fn at_reel_rate(cue: Cue) -> Vec<f32> {
    let src = synth::render(cue);
    let ratio = sample_rate(cue) as f32 / SAMPLE_RATE as f32;
    if (ratio - 1.0).abs() < 1e-6 {
        return src;
    }
    let n = (src.len() as f32 / ratio) as usize;
    (0..n)
        .map(|i| {
            let x = i as f32 * ratio;
            let k = x as usize;
            let f = x - k as f32;
            let a = src.get(k).copied().unwrap_or(0.0);
            let b = src.get(k + 1).copied().unwrap_or(a);
            a + (b - a) * f
        })
        .collect()
}

struct Reel {
    out: Vec<f32>,
}

impl Reel {
    /// Adds `cue` at `t` s, at `gain` (loops for `hold` s).
    fn add(&mut self, cue: Cue, t: f32, gain: f32, hold: f32) {
        let s = at_reel_rate(cue);
        let from = (t * SAMPLE_RATE as f32) as usize;
        let len = if cue.def().looped { (hold * SAMPLE_RATE as f32) as usize } else { s.len() };
        if self.out.len() < from + len {
            self.out.resize(from + len, 0.0);
        }
        let fade = (0.05 * SAMPLE_RATE as f32) as usize;
        for k in 0..len {
            let env = if cue.def().looped { (k.min(len - 1 - k) as f32 / fade as f32).min(1.0) } else { 1.0 };
            self.out[from + k] += s[k % s.len()] * gain * cue.def().gain * env;
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "reel.wav".into());
    let mut r = Reel { out: Vec::new() };
    // The title screen.
    r.add(Cue::MusicCalm, 0.0, 0.8, 12.0);
    r.add(Cue::UiClick, 9.0, 1.0, 0.0);
    r.add(Cue::UiConfirm, 10.5, 1.0, 0.0);
    // Launch and cruise.
    r.add(Cue::Launch, 12.0, 1.0, 0.0);
    r.add(Cue::CockpitHum, 12.0, 1.0, 26.0);
    r.add(Cue::ThrusterLoop, 12.5, 0.8, 25.0);
    r.add(Cue::BoostLoop, 15.0, 1.0, 2.5);
    r.add(Cue::RcsPuff, 18.0, 1.0, 0.0);
    r.add(Cue::RcsPuff, 18.15, 1.0, 0.0);
    // The fight.
    r.add(Cue::MusicCombat, 19.0, 0.6, 19.0);
    r.add(Cue::MissileAlarm, 19.5, 1.0, 2.0);
    for k in 0..6 {
        r.add(Cue::LockBeep, 19.5 + k as f32 * (0.4 - k as f32 * 0.05), 1.0, 0.0);
    }
    r.add(Cue::LockSolid, 21.6, 1.0, 1.0);
    r.add(Cue::MissileLaunch, 22.6, 1.0, 0.0);
    r.add(Cue::MissileLaunch, 22.8, 1.0, 0.0);
    r.add(Cue::BeamRifle, 23.5, 1.0, 0.0);
    r.add(Cue::BeamRifle, 24.1, 0.8, 0.0);
    for k in 0..10 {
        r.add(Cue::Gun, 24.8 + k as f32 * 0.08, 0.8, 0.0);
    }
    r.add(Cue::HitFar, 25.3, 1.0, 0.0);
    r.add(Cue::MissileBurst, 25.6, 0.9, 0.0);
    r.add(Cue::HullHit, 26.4, 1.0, 0.0);
    r.add(Cue::Saber, 27.2, 1.0, 0.0);
    r.add(Cue::Clash, 27.5, 1.0, 0.0);
    r.add(Cue::ChargeLoop, 28.5, 1.0, 1.5);
    r.add(Cue::BusterFire, 30.0, 1.0, 0.0);
    r.add(Cue::Explosion, 31.0, 1.0, 0.0);
    r.add(Cue::GStrain, 32.0, 1.0, 2.0);
    r.add(Cue::ZeroOn, 33.5, 1.0, 0.0);
    r.add(Cue::ZeroDrone, 34.0, 0.8, 3.0);
    r.add(Cue::Seizure, 36.0, 1.0, 0.0);
    // Salvage and the dock.
    r.add(Cue::RockBreak, 38.5, 1.0, 0.0);
    r.add(Cue::Grab, 40.5, 1.0, 0.0);
    r.add(Cue::Stow, 41.3, 1.0, 0.0);
    r.add(Cue::LowFuel, 42.3, 1.0, 0.0);
    r.add(Cue::Dock, 43.5, 1.0, 0.0);
    r.add(Cue::Sale, 44.4, 1.0, 0.0);
    r.add(Cue::MusicCalm, 44.0, 0.6, 6.0);

    // Normalize the mix and write 16-bit mono WAV.
    let peak = r.out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let k = 0.9 / peak;
    let data: Vec<u8> = r
        .out
        .iter()
        .flat_map(|v| (((v * k).clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes())
        .collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(&path, wav).expect("write the reel");
    println!("wrote {path}: {:.1} s", r.out.len() as f32 / SAMPLE_RATE as f32);
}
