//! The hot-path contract: after construction, a busy sector ticks without a single heap operation.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
#![cfg(not(target_arch = "wasm32"))]

mod common;

use bc_alloc::CountingAlloc;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn busy_sector_ticks_without_allocating() {
    // 64 pilots (32 Wing Zeros with ZERO engaged, all firing, boosting, swinging sabers) and
    // 256 Mobile Dolls.
    let (mut sim, players) = common::arena(64, 256, 7);
    // Warm up outside the measured region (first contacts, first deaths, respawn paths).
    common::run(&mut sim, &players, 120);
    let mut total = 0;
    let mut ticks = 0;
    for _ in 0..1_000 {
        let t = sim.next_tick();
        let mut cmds = [bc_proto::InputCmd::default(); 64];
        for (k, &id) in players.iter().enumerate() {
            cmds[k] = common::scripted(&sim, id, t);
        }
        let ((), n) = bc_alloc::count(|| {
            for (k, &id) in players.iter().enumerate() {
                sim.set_input(id, cmds[k]);
            }
            sim.step();
        });
        total += n;
        ticks += 1;
    }
    assert_eq!(ticks, 1_000);
    assert!(sim.projectiles.count() > 0 || sim.peak_projectiles > 100, "the fight should be real");
    assert!(sim.events.next_seq() > 1_000, "events should flow: {}", sim.events.next_seq());
    assert!(sim.chunks.count() > 10, "wreckage should pile up: {} chunks", sim.chunks.count());
    assert_eq!(total, 0, "heap operations inside the tick: {total}");
}

#[test]
fn gundams_duel_without_allocating() {
    // Every Gundam's blades, twin blades, the fang and the Cross Crusher, among Mobile Dolls.
    let (mut sim, pilots) = common::gundam_arena(64, 11);
    let mut total = 0;
    for _ in 0..600 {
        let t = sim.next_tick();
        let mut cmds = [bc_proto::InputCmd::default(); 12];
        for (k, pair) in pilots.chunks(2).enumerate() {
            cmds[2 * k] = common::duel_scripted(&sim, pair[0], pair[1], t);
            cmds[2 * k + 1] = common::duel_scripted(&sim, pair[1], pair[0], t);
        }
        let ((), n) = bc_alloc::count(|| {
            for (k, &id) in pilots.iter().enumerate() {
                sim.set_input(id, cmds[k]);
            }
            sim.step();
        });
        total += n;
    }
    let melee = bc_sim::content::WeaponClass::Melee as usize;
    let melee_hits: u32 = pilots.iter().map(|id| sim.stats(id.idx()).hits_by_class[melee]).sum();
    assert!(melee_hits > 0, "the blades should connect");
    assert_eq!(total, 0, "heap operations inside the tick: {total}");
}

#[test]
fn sleepers_never_allocate() {
    // 64 pilots among 256 Mobile Dolls. Every tick a pilot falls asleep, and every third tick a
    // sleeper wakes, well past the cap on sleepers (so the longest asleep are cleared); sleepers
    // are shot down among the fighting; pilots whose suits are gone come back in new ones; the
    // fates are read out every tick. None of it may touch the heap.
    let (mut sim, mut players) = common::arena(64, 256, 9);
    sim.cfg.max_sleepers = 12;
    common::run(&mut sim, &players, 120);
    let mut total = 0;
    let (mut slept, mut woke, mut rejoined, mut fates) = (0u32, 0u32, 0u32, 0u32);
    for n in 0..900usize {
        let t = sim.next_tick();
        let mut cmds = [bc_proto::InputCmd::default(); 64];
        for (k, &id) in players.iter().enumerate() {
            cmds[k] = common::scripted(&sim, id, t);
        }
        let ((), heap) = bc_alloc::count(|| {
            if sim.sleep(players[(n * 7) % 64]) {
                slept += 1;
            }
            if n % 3 == 0 && sim.wake(players[(n * 13) % 64]) {
                woke += 1;
            }
            for (k, id) in players.iter_mut().enumerate() {
                if !sim.suits.valid(*id) {
                    sim.ensure_free_suits(1);
                    if let Some(new) = sim.join(
                        bc_proto::FrameId::Leo,
                        bc_proto::Faction::Colonies,
                        bc_proto::PilotKind::Human,
                    ) {
                        *id = new;
                        rejoined += 1;
                    }
                } else if !sim.is_sleeping(id.idx()) {
                    sim.set_input(*id, cmds[k]);
                }
            }
            sim.step();
            sim.drain_fates(|_| fates += 1);
        });
        total += heap;
    }
    assert!(slept > 300 && woke > 30, "slept {slept}, woke {woke}");
    assert!(fates > 100 && rejoined > 100, "{fates} fates, {rejoined} back in new suits");
    assert!(sim.sleepers() <= 12);
    assert_eq!(total, 0, "heap operations inside the tick: {total}");
}
