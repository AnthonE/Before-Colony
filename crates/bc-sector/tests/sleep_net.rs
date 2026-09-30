//! Pilots leaving and coming back through the sector's queues: a suit put to sleep stays, and the
//! same pilot wakes in it; a sleeper that's gone means a new suit (with the pilot's credits); a
//! wreck can't sleep; sleepers cleared for room are reported to the server.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;

use bc_proto::{Faction, FrameId, MAX_DATAGRAM, PilotKind};
use bc_sector::{Comeback, Control, Outcome, Sector, SectorConfig, SectorShared, SlotState};
use bc_sim::SimConfig;
use bc_sim::sim::Gone;

fn sector(max_sleepers: usize) -> (Sector, Arc<SectorShared>) {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, seed: 5, max_sleepers, ..SimConfig::default() },
        max_clients: 4,
        ..SectorConfig::default()
    };
    let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    (sector, shared)
}

fn join(slot: u16, comeback: Comeback) -> Control {
    Control::Join {
        slot,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback,
        launch: None,
    }
}

/// Sends `msg` and ticks once; the slot's state and outcome after.
fn send(sector: &mut Sector, shared: &SectorShared, slot: u16, msg: Control) -> (SlotState, Outcome) {
    shared.control.push(msg).unwrap();
    sector.tick();
    let st = &shared.slots[slot as usize];
    (st.state(), st.outcome())
}

#[test]
fn a_pilot_sleeps_and_wakes_in_the_same_suit() {
    let (mut sector, shared) = sector(16);
    assert_eq!(
        send(&mut sector, &shared, 0, join(0, Comeback::default())),
        (SlotState::Active, Outcome::Fresh)
    );
    let suit = shared.slots[0].suit_id().expect("a suit");
    sector.sim.suits.credits[usize::from(suit.0)] = 1_234;

    assert_eq!(send(&mut sector, &shared, 0, Control::Sleep { slot: 0 }), (SlotState::Free, Outcome::Asleep));
    assert_eq!(shared.slots[0].suit_id(), Some(suit), "which suit sleeps");
    assert!(sector.sim.is_sleeping(usize::from(suit.0)));
    for _ in 0..30 {
        sector.tick();
    }
    assert_eq!(bc_sector::Metrics::load(&shared.metrics.sleepers), 1);

    // Back, in another slot: the same suit, awake, with what it had.
    let back = Comeback { sleeper: Some(suit), credits: 0 };
    assert_eq!(send(&mut sector, &shared, 2, join(2, back)), (SlotState::Active, Outcome::Woke));
    assert_eq!(shared.slots[2].suit_id(), Some(suit));
    assert!(!sector.sim.is_sleeping(usize::from(suit.0)));
    assert_eq!(sector.sim.suits.credits[usize::from(suit.0)], 1_234);

    // A guest's way out: the suit goes.
    assert_eq!(
        send(&mut sector, &shared, 2, Control::Leave { slot: 2 }),
        (SlotState::Free, Outcome::Released)
    );
    assert!(!sector.sim.suits.used.get(usize::from(suit.0)));
}

#[test]
fn a_sleeper_thats_gone_means_a_new_suit_with_the_pilots_credits() {
    let (mut sector, shared) = sector(16);
    send(&mut sector, &shared, 0, join(0, Comeback::default()));
    let (idx, generation) = shared.slots[0].suit_id().unwrap();
    send(&mut sector, &shared, 0, Control::Sleep { slot: 0 });
    // A handle from before (the generation moved on): no waking a stranger.
    let stale = Comeback { sleeper: Some((idx, generation.wrapping_add(1))), credits: 700 };
    assert_eq!(send(&mut sector, &shared, 1, join(1, stale)), (SlotState::Active, Outcome::Fresh));
    let fresh = shared.slots[1].suit_id().unwrap();
    assert_ne!(fresh.0, idx);
    assert_eq!(sector.sim.suits.credits[usize::from(fresh.0)], 700);
    assert!(sector.sim.is_sleeping(usize::from(idx)), "the real sleeper sleeps on");
}

#[test]
fn a_wreck_cannot_sleep() {
    let (mut sector, shared) = sector(16);
    send(&mut sector, &shared, 0, join(0, Comeback::default()));
    let (idx, _) = shared.slots[0].suit_id().unwrap();
    sector.sim.suits.alive.set(usize::from(idx), false);
    assert_eq!(
        send(&mut sector, &shared, 0, Control::Sleep { slot: 0 }),
        (SlotState::Free, Outcome::Released)
    );
    assert_eq!(shared.slots[0].suit_id(), None);
}

#[test]
fn sleepers_cleared_for_room_are_reported() {
    let (mut sector, shared) = sector(1);
    send(&mut sector, &shared, 0, join(0, Comeback::default()));
    send(&mut sector, &shared, 1, join(1, Comeback::default()));
    let first = shared.slots[0].suit_id().unwrap();
    send(&mut sector, &shared, 0, Control::Sleep { slot: 0 });
    assert!(shared.notes.pop().is_none());
    // A second sleeper, over the cap of one: the first is cleared, and the server hears whose
    // suit (slot and generation) it was.
    send(&mut sector, &shared, 1, Control::Sleep { slot: 1 });
    let note = shared.notes.pop().expect("a note");
    assert_eq!((note.suit, note.generation, note.gone), (first.0, first.1, Gone::Evicted));
    assert!(shared.notes.pop().is_none());
    // Its pilot comes back to nothing: a new suit.
    assert_eq!(
        send(&mut sector, &shared, 0, join(0, Comeback { sleeper: Some(first), credits: 0 })),
        (SlotState::Active, Outcome::Fresh)
    );
}
