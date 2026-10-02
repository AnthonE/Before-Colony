//! Consumables (`content::kits`): a patch kit seals a leak and mends the worst-off system a level,
//! a coolant flush dumps heat, a stim lifts the G a pilot bears and then crashes it (and the
//! owner's stat sheet, built from the snapshot, agrees with the server's); a rack goes out with
//! its loadout and comes home with what's left. (Chaff is with the missiles: `missiles.rs`.)
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, FrameId, PilotKind};
use bc_sim::content::kits::{CRASH_G, CRASH_TICKS, STIM_G, STIM_TICKS};
use bc_sim::content::systems::{DAMAGED, FAILED, OK};
use bc_sim::content::{Kit, Kits, System, Systems};
use bc_sim::sim::Loadout;
use bc_sim::tuning::own_tuning;
use bc_sim::{Sim, SimConfig, SuitId};

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn rack(n: u8) -> Kits {
    let mut k = Kits::NONE;
    for kit in Kit::ALL {
        k.set(kit, n);
    }
    k
}

fn launch(sim: &mut Sim, systems: Systems, kits: Kits) -> SuitId {
    let mut l = Loadout::full(FrameId::Leo);
    l.systems = systems;
    l.kits = kits;
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap();
    sim.step();
    id
}

#[test]
fn a_patch_kit_seals_the_tank_first_then_the_worst_off() {
    let mut s = sim();
    let worn =
        Systems::OK.with(System::Tank, DAMAGED).with(System::Gyros, FAILED).with(System::Sensors, DAMAGED);
    let id = launch(&mut s, worn, rack(3));
    let i = id.idx();
    assert!(s.tuning(i).leak_kg_s > 0.0);
    assert!(s.use_kit(id, Kit::Patch));
    assert_eq!(s.suits.systems[i].get(System::Tank), OK);
    assert_eq!(s.tuning(i).leak_kg_s, 0.0, "the leak is sealed");
    assert!(s.use_kit(id, Kit::Patch));
    assert_eq!(s.suits.systems[i].get(System::Gyros), DAMAGED, "failed before damaged");
    assert!(s.use_kit(id, Kit::Patch));
    assert_eq!(s.suits.kits[i].get(Kit::Patch), 0);
    assert!(!s.use_kit(id, Kit::Patch), "the rack is empty");
    // With nothing to patch, a kit stays in the rack.
    let mut s = sim();
    let id = launch(&mut s, Systems::OK, rack(1));
    assert!(!s.use_kit(id, Kit::Patch));
    assert_eq!(s.suits.kits[id.idx()].get(Kit::Patch), 1);
}

#[test]
fn a_coolant_flush_dumps_the_heat() {
    let mut s = sim();
    let id = launch(&mut s, Systems::OK, rack(1));
    let i = id.idx();
    assert!(!s.use_kit(id, Kit::Coolant), "cold already: kept");
    s.suits.heat[i] = 1_000.0;
    s.suits.overheated[i] = true;
    assert!(s.use_kit(id, Kit::Coolant));
    assert_eq!((s.suits.heat[i], s.suits.overheated[i]), (0.0, false));
}

#[test]
fn a_stim_lifts_the_g_then_crashes_and_the_client_agrees() {
    let mut s = sim();
    let id = launch(&mut s, Systems::OK, rack(2));
    let i = id.idx();
    let base = s.tuning(i).g_tolerance;
    assert!(s.use_kit(id, Kit::Stim));
    assert!(!s.use_kit(id, Kit::Stim), "one at a time");
    let mut seen = (false, false);
    for _ in 0..u32::from(STIM_TICKS + CRASH_TICKS) + 2 {
        // The owner's stat sheet, from this tick's snapshot, is the one the server flies the
        // next tick with.
        let own = s.own_state(i);
        s.step();
        let g = s.tuning(i).g_tolerance;
        assert_eq!(own_tuning(&own).g_tolerance, g, "stim {}", own.stim);
        if (g - (base + STIM_G)).abs() < 1e-6 {
            seen.0 = true;
        }
        if (g - (base + CRASH_G)).abs() < 1e-6 {
            seen.1 = true;
            assert!(seen.0, "the crash comes after");
        }
    }
    assert!(seen.0 && seen.1);
    assert_eq!(s.tuning(i).g_tolerance, base);
    assert!(s.use_kit(id, Kit::Stim), "and another, once it's over");
}

#[test]
fn a_rack_goes_out_and_what_is_left_comes_home() {
    let mut s = sim();
    let mut kits = Kits::NONE;
    kits.set(Kit::Chaff, 2);
    kits.set(Kit::Stim, 1);
    let id = launch(&mut s, Systems::OK, kits);
    assert_eq!(s.own_state(id.idx()).kits, kits.0);
    assert!(s.use_kit(id, Kit::Chaff));
    assert!(!s.use_kit(id, Kit::Patch), "none in the rack");
    let home = s.homecoming(id.idx());
    assert_eq!((home.kits.get(Kit::Chaff), home.kits.get(Kit::Stim)), (1, 1));
}
