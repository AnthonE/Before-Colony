//! Renders the title theme to a stereo WAV file, to listen to without the game:
//!
//! ```sh
//! cargo run -p bc-sound --release --example title -- title.wav
//! ```
//!
//! Once through the loop, then its first eight bars again to hear it come round.

mod wav;

use bc_sound::{Cue, sample_rate, synth, title};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "title.wav".into());
    let buf = synth::render(Cue::MusicTitle);
    let n = buf.len() / 2;
    let (left, right) = buf.split_at(n);
    let frames = n + 8 * n / title::BARS as usize;
    let interleaved: Vec<f32> = (0..frames).flat_map(|i| [left[i % n], right[i % n]]).collect();
    let rate = sample_rate(Cue::MusicTitle);
    wav::write(&path, rate, 2, &interleaved);
    println!("wrote {path}: {:.1} s", frames as f32 / rate as f32);
}
