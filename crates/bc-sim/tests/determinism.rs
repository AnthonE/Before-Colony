//! The same scenario must produce bit-identical state on every machine and on wasm32 (the browser
//! predicts with this code). Native runs need `--features deterministic` (glam scalar math, which is
//! what wasm32 uses too); `cargo test --workspace` gets it through feature unification.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
#![cfg(any(feature = "deterministic", target_arch = "wasm32"))]

mod common;

/// Hash after 600 ticks of the reference scenario (update deliberately when the sim changes).
const GOLDEN: u64 = 0x06e1_3c01_bfae_0563;

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

/// Hash after suits have flown into rocks and fired through them.
const ROCKS_GOLDEN: u64 = 0x15ca_6f25_2a91_5b07;

fn rocks_hash() -> u64 {
    use bc_proto::buttons::FIRE_PRIMARY;
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
    for _ in 0..150 {
        let t = sim.next_tick();
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
