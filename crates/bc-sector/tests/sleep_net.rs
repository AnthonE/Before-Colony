//! Pilots leaving and coming back through the sector's queues: a suit put to sleep stays, and the
//! same pilot wakes in it; a sleeper that's gone means a new suit (with the pilot's credits); a
//! wreck can't sleep; sleepers cleared for room are reported to the server; suits on bodies are
//! counted, and one that wakes on a body keeps its grip until its pilot is heard from.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;

use bc_proto::{Faction, FrameId, MAX_DATAGRAM, PilotKind};
use bc_sector::{Comeback, Control, Outcome, Sector, SectorConfig, SectorShared, SlotState};
use bc_sim::bodies::Body;
use bc_sim::ground::{Footing, STANCE};
use bc_sim::handle::Handle;
use bc_sim::sim::{Gone, POWER_DOWN_TICKS};
use bc_sim::{SimConfig, SuitId};
use glam::Vec3;

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

#[test]
fn suits_on_bodies_are_counted_and_the_welcome_names_the_landmarks() {
    let (mut sector, shared) = sector(16);
    assert_eq!(shared.landmarks, 2, "MO-II and Hermit");
    send(&mut sector, &shared, 0, join(0, Comeback::default()));
    let (idx, generation) = shared.slots[0].suit_id().unwrap();
    // Standing on Hermit (out of its hide spots) as its pilot leaves: parked on its feet.
    let id = SuitId(Handle { idx, generation });
    assert!(sector.sim.place_on(id, Body::Landmark(1), Vec3::new(0.3, 0.2, 1.0)));
    assert_eq!(send(&mut sector, &shared, 0, Control::Sleep { slot: 0 }), (SlotState::Free, Outcome::Asleep));
    let count = |c| bc_sector::Metrics::load(c);
    let m = &shared.metrics;
    assert_eq!((count(&m.grounded), count(&m.aloft), count(&m.parked)), (1, 0, 1));
    assert_eq!((count(&m.hidden), count(&m.sleepers_hidden)), (0, 0), "still powering down");
    for _ in 0..POWER_DOWN_TICKS {
        sector.tick();
    }
    assert_eq!((count(&m.grounded), count(&m.hidden), count(&m.sleepers_hidden)), (1, 1, 1), "dark");

    // A sector of fewer landmarks says so; one asking for more than there are has them all.
    for (asked, has) in [(0, 0), (1, 1), (9, 2)] {
        let cfg = SectorConfig {
            sim: SimConfig { target_dolls: 0, field_rocks: 0, landmarks: asked, ..SimConfig::default() },
            max_clients: 1,
            ..SectorConfig::default()
        };
        let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
        assert_eq!((shared.landmarks, sector.sim.landmarks().len()), (has, usize::from(has)));
    }
}

#[test]
fn a_rider_that_wakes_keeps_its_grip_until_its_pilot_is_heard_from() {
    let (mut sector, shared) = sector(16);
    send(&mut sector, &shared, 0, join(0, Comeback::default()));
    let (idx, generation) = shared.slots[0].suit_id().unwrap();
    let i = usize::from(idx);
    // On its feet on Hermit as its pilot leaves.
    assert!(sector.sim.place_on(
        SuitId(Handle { idx, generation }),
        Body::Landmark(1),
        Vec3::new(0.3, 0.2, 1.0)
    ));
    send(&mut sector, &shared, 0, Control::Sleep { slot: 0 });
    for _ in 0..30 {
        sector.tick();
    }
    let parked = sector.sim.suits.anchor[i];
    // Back: the suit wakes where it stood, and is flown on stand-ins until the client's first
    // command arrives (a round trip and the client's lead later, or a slow page's first frame).
    // Those carry on as the sector left the suit, gripping, not as a client that never said
    // anything would: it doesn't let go and push off its body.
    let back = Comeback { sleeper: Some((idx, generation)), credits: 0 };
    assert_eq!(send(&mut sector, &shared, 1, join(1, back)), (SlotState::Active, Outcome::Woke));
    for _ in 0..60 {
        sector.tick();
        assert_eq!(sector.sim.footing(i), Footing::Grounded, "let go at {}", sector.sim.tick());
    }
    let a = sector.sim.suits.anchor[i];
    assert_eq!((a.body, a.stance), (Body::Landmark(1), STANCE), "still standing on Hermit");
    assert!(a.local.distance(parked.local) < 1e-3, "moved {} m", a.local.distance(parked.local));
}
