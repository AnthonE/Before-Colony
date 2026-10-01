//! Tick cost of a busy sector: 64 pilots (32 with ZERO engaged), all firing, plus 256 Mobile Dolls;
//! the same with 64 Gundam pilots duelling in every playable frame (jammers, Neo-Birds, Full
//! Opens, missile salvos); and a crowd on the bodies, 128 suits walking, hopping and digging on
//! MO-II, Hermit and the rocks with 64 hunters over them. Budget at 30 Hz is 33 ms; the target is
//! median < 2 ms and p99 < 4 ms on one core.
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

/// The Gundams' arena: 32 duels in every playable frame, each pilot designating its foe, striking,
/// changing mode and firing its specials, plus 256 dolls. Magazines are refilled every 3 s, so the
/// missile boats keep hundreds of missiles in the air.
fn percentiles_gundams() {
    let (mut sim, duels) = common::gundam_crowd(32, 256, 7);
    let step = |sim: &mut bc_sim::Sim| {
        let t = sim.next_tick();
        if t.is_multiple_of(90) {
            for &(a, b) in &duels {
                for id in [a, b] {
                    let spec = bc_sim::content::frame(sim.suits.frame[id.idx()]);
                    for (k, m) in spec.loadout.iter().enumerate().take(2) {
                        if let Some(m) = m {
                            sim.suits.weapons[id.idx()][k].ammo = bc_sim::content::weapon(m.weapon).ammo;
                        }
                    }
                }
            }
        }
        let cmds: Vec<_> = duels
            .iter()
            .flat_map(|&(a, b)| {
                [(a, common::duel_scripted(sim, a, b, t)), (b, common::duel_scripted(sim, b, a, t))]
            })
            .collect();
        let start = Instant::now();
        for (id, cmd) in cmds {
            sim.set_input(id, cmd);
        }
        sim.step();
        start.elapsed()
    };
    for _ in 0..150 {
        step(&mut sim);
    }
    let mut samples = Vec::with_capacity(2_000);
    let mut missiles = 0;
    for _ in 0..2_000 {
        samples.push(step(&mut sim));
        missiles = missiles.max(sim.missiles.count());
    }
    samples.sort();
    let p = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    println!(
        "tick (64 Gundam pilots + 256 dolls, {missiles} missiles peak): p50 {:?}  p90 {:?}  p99 {:?}  max {:?}",
        p(0.5),
        p(0.9),
        p(0.99),
        samples[samples.len() - 1]
    );
    assert!(p(0.5) < Duration::from_millis(2), "median over budget");
    assert!(p(0.99) < Duration::from_millis(4), "p99 over budget");
}

/// The crowd on the bodies: 128 riders on MO-II, Hermit and the rocks, walking, running, hopping,
/// crouching, digging and letting go (and caught again), 64 Heavyarms firing guns and missiles at
/// them, and 256 dolls; every 10 ticks a rider falls asleep where it stands, and every 30 one wakes.
fn percentiles_riders() {
    let (mut sim, riders, hunters, _) = common::rider_crowd(256, 7);
    let step = |sim: &mut bc_sim::Sim, n: usize| {
        let t = sim.next_tick();
        let mut cmds: Vec<_> = riders
            .iter()
            .enumerate()
            .filter(|(_, id)| sim.suits.valid(**id) && !sim.is_sleeping(id.idx()))
            .map(|(k, &id)| (id, common::rider_scripted(sim, id, k, t)))
            .collect();
        cmds.extend(
            hunters
                .iter()
                .enumerate()
                .map(|(k, &(id, prey))| (id, common::hunter_scripted(sim, id, prey, k, t))),
        );
        let start = Instant::now();
        if n.is_multiple_of(10) {
            sim.sleep(riders[(n / 10 * 13) % 128]);
        }
        if n % 30 == 15 {
            sim.wake(riders[(n / 30 * 13) % 128]);
        }
        for (id, cmd) in cmds {
            sim.set_input(id, cmd);
        }
        sim.step();
        start.elapsed()
    };
    for n in 0..150 {
        step(&mut sim, n);
    }
    let mut samples = Vec::with_capacity(2_000);
    let (mut grounded, mut aloft) = (0, 0);
    for n in 150..2_150 {
        samples.push(step(&mut sim, n));
        for id in &riders {
            match sim.footing(id.idx()) {
                bc_sim::ground::Footing::Grounded => grounded += 1,
                bc_sim::ground::Footing::Aloft => aloft += 1,
                bc_sim::ground::Footing::Free => {}
            }
        }
    }
    samples.sort();
    let p = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    println!(
        "tick (128 riders + 64 hunters + 256 dolls, {:.0} grounded and {:.0} aloft a tick): p50 {:?}  p90 {:?}  p99 {:?}  max {:?}",
        f64::from(grounded) / 2_000.0,
        f64::from(aloft) / 2_000.0,
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
    percentiles_gundams();
    percentiles_riders();
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
