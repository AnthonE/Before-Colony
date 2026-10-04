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
fn anime_rules_tick_without_allocating() {
    // The same busy sector under anime flight rules: boost gauges drain and fill back up.
    let (mut sim, players) = common::arena(64, 256, 9);
    sim.cfg.flight = bc_sim::tuning::FlightRules::Anime;
    common::run(&mut sim, &players, 120);
    let (mut total, mut refilled) = (0, 0);
    for _ in 0..600 {
        let t = sim.next_tick();
        let mut cmds = [bc_proto::InputCmd::default(); 64];
        for (k, &id) in players.iter().enumerate() {
            cmds[k] = common::scripted(&sim, id, t);
        }
        let before: Vec<f32> = players.iter().map(|id| sim.suits.flight[id.idx()].propellant).collect();
        let ((), n) = bc_alloc::count(|| {
            for (k, &id) in players.iter().enumerate() {
                sim.set_input(id, cmds[k]);
            }
            sim.step();
        });
        total += n;
        refilled += players
            .iter()
            .zip(&before)
            .filter(|(id, b)| sim.suits.alive.get(id.idx()) && sim.suits.flight[id.idx()].propellant > **b)
            .count();
    }
    assert!(refilled > 100, "the gauges should fill back up: {refilled}");
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
fn locked_on_pilots_never_allocate() {
    // Thirty duels, every pilot locked on to its foe, among Mobile Dolls.
    let (mut sim, duels) = common::gundam_crowd(30, 64, 13);
    common::run(&mut sim, &[], 30);
    let mut total = 0;
    for _ in 0..600 {
        let t = sim.next_tick();
        let mut cmds = [bc_proto::InputCmd::default(); 60];
        for (k, &(a, b)) in duels.iter().enumerate() {
            cmds[2 * k] = common::locked_scripted(&sim, a, b, t);
            cmds[2 * k + 1] = common::locked_scripted(&sim, b, a, t);
        }
        let ((), n) = bc_alloc::count(|| {
            for (k, &(a, b)) in duels.iter().enumerate() {
                sim.set_input(a, cmds[2 * k]);
                sim.set_input(b, cmds[2 * k + 1]);
            }
            sim.step();
        });
        total += n;
    }
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

#[test]
fn riders_never_allocate() {
    use bc_proto::InputCmd;
    use bc_sim::ground::Footing;

    // 128 suits standing on MO-II, Hermit and the rocks they can grip, 64 Heavyarms hunting them
    // with guns and missiles, and 256 Mobile Dolls. The riders walk, run, hop, crouch, dig, let go
    // and are caught again; every 10 ticks one falls asleep where it is, every 30 one wakes, and a
    // rock with riders on it is shattered; some of those asleep go dark. None of it may touch the
    // heap.
    let (mut sim, riders, hunters, broken) = common::rider_crowd(256, 13);
    let mut was: Vec<Footing> = riders.iter().map(|id| sim.footing(id.idx())).collect();
    let (mut grounded, mut aloft, mut catches, mut parked) = (0u32, 0u32, 0u32, 0usize);
    let mut hidden = 0;
    let mut total = 0;
    for n in 0..900u32 {
        let t = sim.next_tick();
        let mut rider_cmds = [InputCmd::default(); 128];
        for (k, id) in riders.iter().enumerate() {
            rider_cmds[k] = common::rider_scripted(&sim, *id, k, t);
        }
        let mut hunter_cmds = [InputCmd::default(); 64];
        for (k, &(id, prey)) in hunters.iter().enumerate() {
            hunter_cmds[k] = common::hunter_scripted(&sim, id, prey, k, t);
        }
        let ((), heap) = bc_alloc::count(|| {
            if n.is_multiple_of(10) {
                sim.sleep(riders[(n as usize / 10 * 13) % 128]);
            }
            if n % 30 == 15 {
                sim.wake(riders[(n as usize / 30 * 13) % 128]);
            }
            if n == 450 {
                sim.field.set_dead(broken, true);
            }
            for (k, id) in riders.iter().enumerate() {
                if sim.suits.valid(*id) && !sim.is_sleeping(id.idx()) {
                    sim.set_input(*id, rider_cmds[k]);
                }
            }
            for (k, &(id, _)) in hunters.iter().enumerate() {
                sim.set_input(id, hunter_cmds[k]);
            }
            sim.step();
        });
        total += heap;
        common::check_invariants(&sim);
        for (k, id) in riders.iter().enumerate() {
            let now = sim.footing(id.idx());
            match now {
                Footing::Grounded => grounded += 1,
                Footing::Aloft => aloft += 1,
                Footing::Free => {}
            }
            if was[k] == Footing::Free && now == Footing::Aloft {
                catches += 1;
            }
            was[k] = now;
        }
        parked = parked.max(sim.parked());
        hidden = hidden.max(sim.n_hidden);
    }
    assert!(grounded > 1_000 && aloft > 100, "{grounded} grounded and {aloft} aloft suit-ticks");
    assert!(catches > 10 && parked > 5, "{catches} catches, {parked} parked at once");
    assert!(hidden > 2, "{hidden} hidden at once");
    let missiles: u32 = hunters.iter().map(|(id, _)| sim.stats(id.idx()).missiles).sum();
    assert!(missiles > 0, "the hunters never let a missile go");
    assert!(sim.field.is_dead(broken));
    assert_eq!(total, 0, "heap operations inside the tick: {total}");
}

#[test]
fn the_colony_answers_without_allocating() {
    // The city, its furniture, its day, its frames and its trams are closed forms: asking them anything
    // allocates nothing, so a future sector inside the colony can ask them in its tick.
    use bc_sim::colony::{city, frame, furniture, time, transit};
    let mut rng = bc_sim::math::Rng::new(5);
    let ((hits, pieces), n) = bc_alloc::count(|| {
        let (mut hits, mut pieces) = (0u32, 0u32);
        for i in 0..100_000u32 {
            let k = (i % 3) as u8;
            let p = glam::Vec3::new(
                rng.signed() * 16_000.0,
                rng.next_f32() * 12.0,
                -rng.next_f32() * frame::STRIP_WIDTH,
            );
            let e = glam::Vec3::new(0.3, 0.9, 0.3);
            hits += u32::from(city::solid(k, p - e, p + e, city::Stage(0)));
            let d = time::day(i * 7, 0.0);
            hits += u32::from(time::key_light(k as usize, &d).x > 2.0);
            let c = frame::CityPos::new(k, p.x, -p.z, p.y).to_colony();
            hits += u32::from(matches!(frame::from_colony(c), frame::Under::Window { .. }));
            // The trams, and the walls of a car.
            let t = transit::train(k, (i % transit::TRAINS) as u8, i * 13, 0.5);
            hits += u32::from(t.doors);
            hits += u32::from(transit::car_walls(t.doors, |b| b.h1 > 3.0));
            // The street furniture in a 120 m square round it.
            if i % 16 == 0 {
                let area = city::Rect::new(-p.z - 60.0, -p.z + 60.0, p.x - 60.0, p.x + 60.0);
                furniture::each_furniture(k, &area, city::Stage(0), |_| {
                    pieces += 1;
                    false
                });
            }
        }
        (hits, pieces)
    });
    assert!(hits > 1_000, "{hits}: buildings should be hit");
    assert!(pieces > 1_000, "{pieces}: furniture should be found");
    assert_eq!(n, 0, "heap operations asking the colony: {n}");
}

#[test]
fn the_interior_ticks_without_allocating() {
    // 64 suits flying the colony's inside: low among the towers, landing on roofs and the floor,
    // pressing fire (which the colony's law ignores); every fourth with its grip armed, landing on
    // the city and walking it.
    use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FLIGHT_ASSIST, GRIP};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::colony::frame::CityPos;
    use bc_sim::colony::interior::WorldKind;
    use bc_sim::sim::Loadout;
    use glam::Vec3;
    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig {
        target_dolls: 0,
        field_rocks: 0,
        landmarks: 0,
        survival: true,
        world: WorldKind::Interior,
        ..bc_sim::SimConfig::default()
    });
    let mut ids = Vec::new();
    for k in 0..64 {
        let f = [FrameId::Leo, FrameId::WingZero, FrameId::Heavyarms][k % 3];
        let id = sim.launch(f, Faction::Colonies, PilotKind::Human, &Loadout::full(f)).unwrap();
        // (Those with their grips armed low enough to be caught.)
        let h = if k % 4 == 0 { 25.0 } else { 40.0 };
        let at = CityPos::new((k % 3) as u8, -9_000.0 + k as f32 * 90.0, 300.0 + (k * 37 % 2_800) as f32, h);
        sim.suits.flight[id.idx()].pos = at.to_colony();
        ids.push(id);
    }
    let mut total = 0;
    for n in 0..1_000u32 {
        let t = sim.next_tick();
        let mut cmds = [InputCmd::default(); 64];
        for (k, id) in ids.iter().enumerate() {
            let f = &sim.suits.flight[id.idx()];
            let aim = (f.rot * Vec3::Z + Vec3::new(0.0, 0.1, 0.05 * (k % 5) as f32)).normalize();
            let (buttons, thrust) = if k % 4 == 0 {
                (GRIP | FLIGHT_ASSIST, [0, 0, if (n / 100).is_multiple_of(2) { 127 } else { -127 }])
            } else if (n / 50 + k as u32).is_multiple_of(3) {
                (0, [0, 0, 0])
            } else {
                (FLIGHT_ASSIST | FIRE_PRIMARY | BOOST, [20, 0, 127])
            };
            cmds[k] = InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() };
        }
        let ((), heap) = bc_alloc::count(|| {
            for (k, id) in ids.iter().enumerate() {
                sim.set_input(*id, cmds[k]);
            }
            sim.step();
        });
        total += heap;
    }
    assert_eq!(total, 0, "heap operations inside the interior's tick: {total}");
    let walkers =
        ids.iter().filter(|id| sim.suits.footing[id.idx()] != bc_sim::ground::Footing::Free).count();
    assert!(walkers > 4, "on the city: {walkers}");
}
