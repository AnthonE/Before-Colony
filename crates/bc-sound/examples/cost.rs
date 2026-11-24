//! What each cue costs to render, the way the browser builds the bank at boot (`bc-client`'s
//! `audio.rs` renders one whole cue at a time between its frames' budgets): the slowest first.
//!
//! ```sh
//! cargo run -p bc-sound --release --example cost
//! ```
//!
//! Native timings; the browser's wasm runs these a little slower.

use std::time::Instant;

use bc_sound::{Cue, channels, sample_rate, synth};

fn main() {
    let mut rows: Vec<(Cue, f64, f32)> = Cue::ALL
        .iter()
        .map(|&cue| {
            let start = Instant::now();
            let b = synth::render(cue);
            let ms = start.elapsed().as_secs_f64() * 1_000.0;
            let secs = b.len() as f32 / channels(cue) as f32 / sample_rate(cue) as f32;
            (cue, ms, secs)
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = rows.iter().map(|r| r.1).sum();
    for (cue, ms, secs) in &rows {
        println!("{ms:>8.2} ms  {secs:>6.2} s  {cue:?}");
    }
    println!("{total:>8.2} ms  the whole bank");
}
