//! The pilot's body in flight (`content::body`): how fed they are moves the G they bear, on the
//! server's stat sheet and on the owner's, built from the snapshot, alike; a stim stacks on it,
//! and a suit launched again carries a fed pilot until the session says otherwise.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, FrameId, PilotKind};
use bc_sim::content::body::{Fed, fed_g};
use bc_sim::content::kits::STIM_G;
use bc_sim::content::{Kit, Kits};
use bc_sim::sim::Loadout;
use bc_sim::tuning::own_tuning;
use bc_sim::{Sim, SimConfig, SuitId};

fn sim() -> Sim {
    Sim::new(SimConfig { target_drones: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn launch(sim: &mut Sim) -> SuitId {
    let mut l = Loadout::full(FrameId::Argo);
    l.kits = {
        let mut k = Kits::NONE;
        k.set(Kit::Stim, 1);
        k
    };
    let id = sim.launch(FrameId::Argo, Faction::Colonies, PilotKind::Human, &l).unwrap();
    sim.step();
    id
}

#[test]
fn each_condition_moves_the_g_and_the_client_agrees() {
    let mut s = sim();
    let id = launch(&mut s);
    let i = id.idx();
    let base = s.tuning(i).g_tolerance;
    assert_eq!(s.own_state(i).fed, 0, "a launched pilot is fed");
    assert!(!s.set_fed(id, Fed::Fed), "already fed: nothing changes");
    for f in Fed::ALL {
        s.set_fed(id, f);
        let own = s.own_state(i);
        assert_eq!(own.fed, f as u8);
        let g = s.tuning(i).g_tolerance;
        assert_eq!(own_tuning(&own).g_tolerance, g, "{f:?}");
        assert_eq!(g, base + fed_g(f), "{f:?}");
        // It holds tick to tick, the owner's sheet the one the server flies the next tick with.
        s.step();
        assert_eq!(own_tuning(&own).g_tolerance, s.tuning(i).g_tolerance, "{f:?}");
    }
    // A stim stacks on the condition.
    assert!(s.set_fed(id, Fed::Hungry));
    assert!(s.use_kit(id, Kit::Stim));
    s.step();
    let own = s.own_state(i);
    assert_eq!(own_tuning(&own).g_tolerance, s.tuning(i).g_tolerance);
    assert!((s.tuning(i).g_tolerance - (base + fed_g(Fed::Hungry) + STIM_G)).abs() < 1e-6);
    // Asleep, the condition still reaches the suit.
    assert!(s.sleep(id));
    assert!(s.set_fed(id, Fed::Starving));
    assert_eq!(s.own_state(i).fed, Fed::Starving as u8);
    // A released suit takes no word, and the next one launched carries a fed pilot.
    s.leave(id);
    assert!(!s.set_fed(id, Fed::WellFed));
    let again = launch(&mut s);
    assert_eq!(s.suits.status[again.idx()].fed, Fed::Fed);
    assert_eq!(s.own_state(again.idx()).fed, 0);
}
