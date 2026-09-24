//! The same scenario must produce bit-identical state on every machine and on wasm32 (the browser
//! predicts with this code). Native runs need `--features deterministic` (glam scalar math, which is
//! what wasm32 uses too); `cargo test --workspace` gets it through feature unification.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
#![cfg(any(feature = "deterministic", target_arch = "wasm32"))]

mod common;

/// Hash after 600 ticks of the reference scenario (update deliberately when the sim changes).
const GOLDEN: u64 = 0x7a7c_d27c_401a_eb97;

fn scenario_hash() -> u64 {
    let (mut sim, players) = common::arena(8, 24, 42);
    common::run(&mut sim, &players, 600);
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
