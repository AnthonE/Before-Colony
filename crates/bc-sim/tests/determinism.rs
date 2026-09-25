//! The same scenario must produce bit-identical state on every machine and on wasm32 (the browser
//! predicts with this code). Native runs need `--features deterministic` (glam scalar math, which is
//! what wasm32 uses too); `cargo test --workspace` gets it through feature unification.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
#![cfg(any(feature = "deterministic", target_arch = "wasm32"))]

mod common;

/// Hash after 600 ticks of the reference scenario (update deliberately when the sim changes).
const GOLDEN: u64 = 0xd10e_d123_9a15_0766;

fn scenario_hash() -> u64 {
    let (mut sim, players) = common::arena(8, 24, 42);
    common::run(&mut sim, &players, 600);
    // The hash covers wreckage too, so the scenario must leave some.
    assert!(sim.chunks.count() > 0, "no limbs or hulks after the battle");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn golden_hash_native() {
    let h = scenario_hash();
    assert_eq!(h, scenario_hash(), "must be reproducible within a process");
    assert_eq!(h, GOLDEN, "state hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn golden_hash_wasm() {
    assert_eq!(scenario_hash(), GOLDEN);
}

/// Hash after 450 ticks of the Gundams duelling in pairs among Mobile Dolls: every blade, the
/// Cross Crusher, the Dragon Fang, the flamethrower and the Gundams' guns (changes deliberately as
/// their mechanics arrive).
const GUNDAMS_GOLDEN: u64 = 0xa22e_cfac_f3a2_f545;

fn gundams_hash() -> u64 {
    use bc_proto::WeaponKind;
    use bc_proto::events::Event;

    let (mut sim, pilots) = common::gundam_arena(12, 7);
    let mut landed = [false; WeaponKind::COUNT];
    let mut clashes = 0;
    for _ in 0..450 {
        let t = sim.next_tick();
        let from = sim.events.next_seq();
        for pair in pilots.chunks(2) {
            let (a, b) = (pair[0], pair[1]);
            let (ca, cb) = (common::duel_scripted(&sim, a, b, t), common::duel_scripted(&sim, b, a, t));
            sim.set_input(a, ca);
            sim.set_input(b, cb);
        }
        sim.step();
        for s in from..sim.events.next_seq() {
            match sim.events.get(s) {
                Some(Event::Hit { weapon, .. }) => landed[*weapon as usize] = true,
                Some(Event::Clash { .. }) => clashes += 1,
                _ => {}
            }
        }
    }
    assert!(clashes > 0, "no blades met in the Gundams' scenario");
    for k in [
        WeaponKind::BeamSaber,
        WeaponKind::ArmyKnife,
        WeaponKind::BeamScythe,
        WeaponKind::HeatShotel,
        WeaponKind::CrossCrusher,
        WeaponKind::DragonFang,
        WeaponKind::BeamGlaive,
        WeaponKind::Flamethrower,
        WeaponKind::BeamGatling,
        WeaponKind::BusterShield,
        WeaponKind::BeamMachineGun,
    ] {
        assert!(landed[k as usize], "no {k:?} hit in the Gundams' scenario");
    }
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn gundams_golden_native() {
    let h = gundams_hash();
    assert_eq!(h, gundams_hash(), "must be reproducible within a process");
    assert_eq!(h, GUNDAMS_GOLDEN, "Gundams' scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn gundams_golden_wasm() {
    assert_eq!(gundams_hash(), GUNDAMS_GOLDEN);
}

/// Hash of the generated debris field (clients build it from the Welcome's seed and count).
const FIELD_GOLDEN: u64 = 0x8631_3a1b_8b14_b993;

fn fnv(h: &mut u64, v: u32) {
    for b in v.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
}

fn field_hash() -> u64 {
    let f =
        bc_sim::field::Field::generate(bc_sim::field::Field::DEFAULT_SEED, bc_sim::field::Field::MAX_ROCKS);
    let mut h = 0xcbf2_9ce4_8422_2325;
    for r in f.rocks() {
        for v in [
            r.pos.x, r.pos.y, r.pos.z, r.radius, r.axes.x, r.axes.y, r.axes.z, r.rot.x, r.rot.y, r.rot.z,
            r.rot.w,
        ] {
            fnv(&mut h, v.to_bits());
        }
        fnv(&mut h, u32::from(r.shape) | u32::from(r.ore) << 8);
    }
    h
}

/// Hash after suits have flown into rocks and fired into them, wearing them down, while a Leo cuts
/// a small one apart with its saber.
const ROCKS_GOLDEN: u64 = 0x72b1_ae0c_faaf_9bc7;

fn rocks_hash() -> u64 {
    use bc_proto::buttons::{FIRE_PRIMARY, MELEE};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::math::{cos, look_rotation, sin};
    use glam::Vec3;

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig { target_dolls: 0, ..bc_sim::SimConfig::default() });
    let rocks: Vec<_> = sim.field.rocks().iter().filter(|r| r.radius > 25.0).take(6).copied().collect();
    let mut ids = Vec::new();
    for (k, r) in rocks.iter().enumerate() {
        // From all round, at speeds up to 2 km/s, firing at whatever is behind the rock.
        let a = k as f32 * 1.1;
        // libm's trig, as in the sim: std's differs between native and wasm32.
        let dir = Vec3::new(cos(a), 0.3 * sin(a), sin(a)).normalize();
        let pos = r.pos - dir * (r.radius + 120.0 + 40.0 * k as f32);
        let frame = if k % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        let id = sim
            .spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(dir, Vec3::Y))
            .unwrap();
        sim.suits.flight[id.idx()].vel = dir * (300.0 + 340.0 * k as f32);
        ids.push((id, dir));
    }
    let k = sim.field.rocks().iter().position(|r| r.radius > 8.0 && r.radius < 12.0).unwrap();
    let small = sim.field.rocks()[k];
    let dir = Vec3::new(1.0, 0.1, 0.3).normalize();
    let face = small.surface(small.pos - dir * (small.radius + 50.0), 0.0);
    let miner = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            face - dir * bc_sim::field::SUIT_CLEARANCE,
            look_rotation(dir, Vec3::Y),
        )
        .unwrap();
    for _ in 0..150 {
        let t = sim.next_tick();
        let cut = if t % 45 < 3 { MELEE } else { 0 };
        let aim = (small.pos - sim.suits.flight[miner.idx()].pos).normalize();
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            thrust: [0, 0, 20],
            buttons: cut,
            ..InputCmd::default()
        };
        sim.set_input(miner, cmd);
        for &(id, dir) in &ids {
            sim.set_input(
                id,
                InputCmd {
                    tick: t,
                    view_tick_q4: t << 4,
                    aim: dir,
                    buttons: FIRE_PRIMARY,
                    ..InputCmd::default()
                },
            );
        }
        sim.step();
    }
    assert!(sim.rocks.destroyed.get(k), "the miner never broke its rock");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn field_and_rocks_golden_native() {
    assert_eq!(field_hash(), FIELD_GOLDEN, "field hash changed: {:#018x}", field_hash());
    let h = rocks_hash();
    assert_eq!(h, rocks_hash(), "must be reproducible within a process");
    assert_eq!(h, ROCKS_GOLDEN, "rocks scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn field_and_rocks_golden_wasm() {
    assert_eq!(field_hash(), FIELD_GOLDEN);
    assert_eq!(rocks_hash(), ROCKS_GOLDEN);
}

/// Hash after a salvage run: pilots gather ore, stow it, tow a hulk, throw, jettison, and sell (and
/// refuel) at the dock.
const SALVAGE_GOLDEN: u64 = 0x10cc_d0c7_aa3c_92a4;

fn salvage_hash() -> u64 {
    use bc_proto::buttons::{FLIGHT_ASSIST, GRAB, JETTISON, STOW, THROW};
    use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment};
    use bc_sim::chunks::{self, Motion};
    use bc_sim::content::ArmSlot;
    use bc_sim::content::salvage::DOCK_CENTER;
    use bc_sim::math::look_rotation;
    use glam::{Quat, Vec3};

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig { target_dolls: 0, ..bc_sim::SimConfig::default() });
    let mut ids = Vec::new();
    for k in 0..4 {
        let pos =
            if k == 3 { DOCK_CENTER + Vec3::X * 40.0 } else { Vec3::new(k as f32 * 150.0, 1_000.0, 0.0) };
        let id = sim
            .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, look_rotation(Vec3::Z, Vec3::Y))
            .unwrap();
        ids.push(id);
    }
    let t = sim.tick();
    let chunk = |sim: &mut bc_sim::Sim, kind: ChunkKind, kg: u32, pos: Vec3, seed: u8| {
        let desc = ChunkDesc { kind, seed, mass_kg: kg };
        let seg =
            Segment { t0: t, pos, vel: Vec3::ZERO, rot: Quat::IDENTITY, spin: Vec3::new(0.0, 0.2, 0.1) }
                .quantized();
        sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap();
    };
    // Ore strung out ahead of each pilot's left hand, and a hulk for the second pilot to tow.
    for (k, id) in ids.iter().enumerate() {
        let f = sim.suits.flight[id.idx()];
        let hand = f.pos + f.rot * ArmSlot::Left.muzzle();
        for n in 0..4u8 {
            let desc =
                ChunkDesc { kind: ChunkKind::Ore { ore: n % 4 }, seed: n, mass_kg: 300 + 100 * u32::from(n) };
            let at = hand + Vec3::new(-chunks::radius(&desc) - 1.0, 0.0, 12.0 * f32::from(n));
            chunk(&mut sim, desc.kind, desc.mass_kg, at, n + 10 * k as u8);
        }
    }
    let hulk = ChunkKind::Hulk { frame: FrameId::Taurus, faction: Faction::Oz, parts: 0b10_1111 };
    let tow = sim.suits.flight[ids[1].idx()].pos + Vec3::new(-12.0, 0.0, 70.0);
    chunk(&mut sim, hulk, 5_600, tow, 99);
    let arm = ChunkKind::Limb { frame: FrameId::WingZero, faction: Faction::Colonies, part: Part::ArmL };
    let f3 = sim.suits.flight[ids[3].idx()];
    chunk(&mut sim, arm, 640, f3.pos + f3.rot * ArmSlot::Left.muzzle() + Vec3::new(-4.0, 0.0, 0.0), 77);

    for _ in 0..360 {
        let t = sim.next_tick();
        for (k, &id) in ids.iter().enumerate() {
            let k = k as u32;
            let mut buttons = FLIGHT_ASSIST | GRAB;
            if (t + k * 7).is_multiple_of(30) && t < 250 {
                buttons |= STOW;
            }
            if k == 0 && t == 300 {
                buttons |= THROW;
            }
            if k == 2 && t == 320 {
                buttons |= JETTISON;
            }
            // Creeping forward (5/127 of a Leo's 220 m/s cruise), slow enough to catch what's ahead.
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: Vec3::Z,
                thrust: [0, 0, 5],
                buttons,
                ..InputCmd::default()
            };
            sim.set_input(id, cmd.quantized());
        }
        sim.step();
    }
    let stowed: u32 = ids.iter().map(|id| sim.suits.cargo_total_kg(id.idx())).sum();
    assert!(stowed > 0, "nothing was stowed");
    assert!(ids.iter().any(|id| sim.held_chunk(id.idx()).is_some()), "nobody is holding anything");
    assert!(sim.suits.credits[ids[3].idx()] > 0, "nothing sold at the dock");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn salvage_golden_native() {
    let h = salvage_hash();
    assert_eq!(h, salvage_hash(), "must be reproducible within a process");
    assert_eq!(h, SALVAGE_GOLDEN, "salvage hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn salvage_golden_wasm() {
    assert_eq!(salvage_hash(), SALVAGE_GOLDEN);
}
