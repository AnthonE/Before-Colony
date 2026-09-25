//! Tick cost of a busy sector: 64 pilots (32 with ZERO engaged), all firing, plus 256 Mobile Dolls.
//! Budget at 30 Hz is 33 ms; the target is median < 2 ms and p99 < 4 ms on one core.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};

fn percentiles() {
    let (mut sim, players) = common::arena(64, 256, 7);
    common::run(&mut sim, &players, 150);
    let mut samples = Vec::with_capacity(2_000);
    for _ in 0..2_000 {
        let t = sim.next_tick();
        let cmds: Vec<_> = players.iter().map(|&id| common::scripted(&sim, id, t)).collect();
        let start = Instant::now();
        for (&id, cmd) in players.iter().zip(cmds) {
            sim.set_input(id, cmd);
        }
        sim.step();
        samples.push(start.elapsed());
    }
    samples.sort();
    let p = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    println!(
        "tick (64 pilots + 256 dolls, {} live projectiles peak): p50 {:?}  p90 {:?}  p99 {:?}  max {:?}",
        sim.peak_projectiles,
        p(0.5),
        p(0.9),
        p(0.99),
        samples[samples.len() - 1]
    );
    assert!(p(0.5) < Duration::from_millis(2), "median over budget");
    assert!(p(0.99) < Duration::from_millis(4), "p99 over budget");
}

fn bench(c: &mut Criterion) {
    percentiles();
    let (mut sim, players) = common::arena(64, 256, 7);
    common::run(&mut sim, &players, 150);
    c.bench_function("sector tick: 64 pilots + 256 dolls", |b| {
        b.iter(|| {
            let t = sim.next_tick();
            for &id in &players {
                let cmd = common::scripted(&sim, id, t);
                sim.set_input(id, cmd);
            }
            sim.step();
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(60).measurement_time(Duration::from_secs(8));
    targets = bench
}
criterion_main!(benches);
