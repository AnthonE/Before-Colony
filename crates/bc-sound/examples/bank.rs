//! Writes every cue in the bank to its own WAV file, for the trailer's mix (`scripts/trailer`) and
//! anything else that wants the game's sounds outside it:
//!
//! ```sh
//! cargo run -p bc-sound --release --example bank -- out/bank
//! ```
//!
//! Each file is named for its cue (`beam_rifle.wav`), at the cue's own rate and channels. A loop
//! is written going round for [`LOOP_S`] seconds (the score for [`MUSIC_S`]), so a cut can take
//! as much of it as it needs.

mod wav;

use bc_sound::{Cue, channels, sample_rate, synth};

/// How long a looped cue is written, s.
const LOOP_S: f32 = 12.0;
/// How long the score is written, s: twice round the title theme.
const MUSIC_S: f32 = 125.0;

/// `BeamRifle` → `beam_rifle`.
fn file_name(cue: Cue) -> String {
    let mut out = String::new();
    for (i, ch) in format!("{cue:?}").chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "bank".into());
    std::fs::create_dir_all(&dir).expect("make the bank's directory");
    for &cue in Cue::ALL {
        let src = synth::render(cue);
        let (rate, ch) = (sample_rate(cue), channels(cue));
        let n = src.len() / ch;
        let music = matches!(cue, Cue::MusicTitle | Cue::MusicCalm | Cue::MusicCombat);
        let frames = if cue.def().looped {
            ((if music { MUSIC_S } else { LOOP_S }) * rate as f32) as usize
        } else {
            n
        };
        // Planar (the title theme is left then right) to interleaved, going round a loop.
        let samples: Vec<f32> =
            (0..frames).flat_map(|i| (0..ch).map(move |c| (i, c))).map(|(i, c)| src[c * n + i % n]).collect();
        let path = format!("{dir}/{}.wav", file_name(cue));
        wav::write(&path, rate, ch as u16, &samples);
    }
    println!("wrote {} cues to {dir}", Cue::ALL.len());
}
