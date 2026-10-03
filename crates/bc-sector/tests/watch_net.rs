//! Spectators through the sector's queues: a pilot on foot in the colony's city watches its inside
//! sector's suits. Their slot gets snapshots marked as a spectator's, with no suit of their own,
//! of the suits near where they watch from; a suit leaving their view is said to have left; and
//! `Leave` ends it. Watching costs the tick nothing on the heap.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_alloc::CountingAlloc;
use bc_proto::events::Event;
use bc_proto::snapshot::header_flags;
use bc_proto::{Faction, FrameId, MAX_DATAGRAM, PilotKind, SnapshotReader};
use bc_sector::{Comeback, Control, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::colony::interior::{INNER_GATE, WorldKind};
use bc_sim::sim::Loadout;
use glam::Vec3;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// What a spectator's snapshot says: whether it's marked theirs and has no own suit, the suits in
/// it, and the slots it says have left.
fn read(bytes: &[u8]) -> (bool, Vec<(u16, Vec3)>, Vec<u16>) {
    let mut r = SnapshotReader::new(bytes).expect("a snapshot");
    let spectator = r.header().flags & header_flags::SPECTATOR != 0;
    let own = r.own().expect("own section");
    let _ = r.zero().expect("zero section");
    let mut left = Vec::new();
    while let Ok(Some(e)) = r.next_event() {
        if let Event::Leave { slot, .. } = e {
            left.push(slot);
        }
    }
    while let Ok(Some(_)) = r.next_rock() {}
    while let Ok(Some(_)) = r.next_missile() {}
    let mut suits = Vec::new();
    while let Ok(Some(e)) = r.next_entity() {
        suits.push((e.slot, e.pos));
    }
    (spectator && own.is_none(), suits, left)
}

#[test]
fn a_pilot_on_foot_watches_the_suits_near_them_inside() {
    let cfg = SectorConfig {
        sim: SimConfig {
            target_dolls: 0,
            field_rocks: 0,
            landmarks: 0,
            max_sleepers: 0,
            survival: true,
            world: WorldKind::Interior,
            ..SimConfig::default()
        },
        max_clients: 2,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let (pilot, watcher) = (shared.leases.pop().unwrap(), shared.leases.pop().unwrap());
    shared
        .control
        .push(Control::Join {
            slot: pilot.slot,
            pilot: PilotKind::Human,
            frame: FrameId::Leo,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
            comeback: Comeback::default(),
            launch: Some(Loadout::full(FrameId::Leo)),
        })
        .unwrap();
    // Watching from under the inner gate, 600 m off.
    let near = INNER_GATE + Vec3::new(0.0, 600.0, 0.0);
    let watch = |at| Control::Watch { slot: watcher.slot, at, max_datagram: MAX_DATAGRAM as u16 };
    shared.control.push(watch(near)).unwrap();
    sector.tick();
    assert_eq!(shared.slots[watcher.slot as usize].state(), SlotState::Active);
    assert_eq!(shared.slots[watcher.slot as usize].suit_id(), None, "no suit of their own");
    let (suit, _) = shared.slots[pilot.slot as usize].suit_id().expect("the pilot's suit");
    let mut buf = [0u8; MAX_DATAGRAM];
    let ring = &mut egress.rings[watcher.slot as usize];
    let mut last = None;
    while let Some(n) = read_packet(ring, &mut buf) {
        last = Some(read(&buf[..n]));
    }
    let (theirs, suits, _) = last.expect("a spectator's snapshot");
    assert!(theirs, "marked a spectator's, with no own suit");
    assert_eq!(suits.len(), 1);
    assert_eq!(suits[0].0, suit);
    assert!(suits[0].1.distance(INNER_GATE) < 200.0, "{}", suits[0].1);

    // Watching from far down the colony: the suit leaves their view.
    shared.control.push(watch(INNER_GATE + Vec3::new(8_000.0, 2_000.0, 0.0))).unwrap();
    sector.tick();
    let n = read_packet(ring, &mut buf).expect("a snapshot");
    let (_, suits, left) = read(&buf[..n]);
    assert!(suits.is_empty());
    assert_eq!(left, vec![suit]);
    // Said for a while, then no more (a spectator acks nothing).
    for _ in 0..30 {
        sector.tick();
    }
    let mut left = vec![0];
    while let Some(n) = read_packet(ring, &mut buf) {
        left = read(&buf[..n]).2;
    }
    assert!(left.is_empty(), "{left:?}");

    // Back near, and watching costs the tick nothing on the heap.
    shared.control.push(watch(near)).unwrap();
    let ((), n) = bc_alloc::count(|| {
        for _ in 0..300 {
            sector.tick();
        }
    });
    assert_eq!(n, 0, "heap operations while watching");
    while read_packet(ring, &mut buf).is_some() {}

    // Leave: no more snapshots, and the slot is free.
    shared.control.push(Control::Leave { slot: watcher.slot }).unwrap();
    sector.tick();
    assert_eq!(shared.slots[watcher.slot as usize].state(), SlotState::Free);
    sector.tick();
    assert!(read_packet(ring, &mut buf).is_none(), "nothing after leaving");
    assert_eq!(shared.slots[pilot.slot as usize].state(), SlotState::Active, "the pilot flies on");
}
