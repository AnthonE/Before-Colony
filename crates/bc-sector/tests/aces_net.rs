//! Zodiac's aces through the sector (`docs/DESIGN.md`, "Aces"): which one is out goes in the
//! shared word for the server; downed by a pilot, their session hears of it with its wreck; and
//! claimed as the bounty, the tugs bring that wreck home `TOW_TICKS` on, as the ace's.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use bc_proto::{ChunkKind, Faction, FrameId, MAX_DATAGRAM, NO_CHUNK, Part, PilotKind, WeaponKind};
use bc_sector::{
    Comeback, Control, Report, Sector, SectorConfig, SectorShared, SlotLease, TOW_TICKS, ace_of_word,
    ace_word,
};
use bc_sim::SimConfig;
use bc_sim::content::aces::ACE_FRAME;
use bc_sim::sim::Loadout;

fn sector() -> (Sector, Arc<SectorShared>, SlotLease) {
    let cfg = SectorConfig {
        sim: SimConfig {
            target_dolls: 1,
            field_rocks: 0,
            seed: 5,
            survival: true,
            ace_every: 30,
            ..SimConfig::default()
        },
        max_clients: 2,
        ..SectorConfig::default()
    };
    let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    let lease = shared.leases.pop().unwrap();
    (sector, shared, lease)
}

fn launch(sector: &mut Sector, shared: &SectorShared, slot: u16) -> usize {
    shared
        .control
        .push(Control::Join {
            slot,
            pilot: PilotKind::Human,
            frame: FrameId::Leo,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
            comeback: Comeback::default(),
            launch: Some(Loadout::full(FrameId::Leo)),
        })
        .unwrap();
    sector.tick();
    usize::from(shared.slots[slot as usize].suit_id().unwrap().0)
}

fn word(shared: &SectorShared) -> Option<(u16, u8, bool)> {
    ace_of_word(shared.ace.load(Ordering::Acquire))
}

/// The first report of `slot`'s that `pick` takes, of those there are now.
fn find<T>(lease: &mut SlotLease, pick: impl Fn(Report) -> Option<T>) -> Option<T> {
    while let Ok(r) = lease.reports.pop() {
        if let Some(t) = pick(r) {
            return Some(t);
        }
    }
    None
}

#[test]
fn the_word_round_trips() {
    for (suit, ace, flying) in [(0, 0, true), (511, 8, false), (u16::MAX >> 1, 254, true)] {
        assert_eq!(ace_of_word(ace_word(suit, ace, flying)), Some((suit, ace, flying)));
    }
    assert_eq!(ace_of_word(0), None);
}

#[test]
fn an_ace_downed_by_a_pilot_is_theirs_and_its_wreck_theirs_to_claim() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    let me = launch(&mut sector, &shared, s);
    assert_eq!(word(&shared), None, "none out yet");
    for _ in 0..40 {
        sector.tick();
    }
    let (suit, ace, flying) = word(&shared).expect("an ace out");
    assert_eq!((ace, flying), (0, true));
    let i = usize::from(suit);
    assert_eq!(sector.sim.ace_out(), Some((i, 0)));
    assert_eq!(sector.sim.suits.frame[i], ACE_FRAME);
    // The pilot downs it: their session hears, with its wreck.
    sector.sim.strike(i, Part::Torso, 1.0e9, me, WeaponKind::BeamRifle);
    sector.tick();
    let (hulk, generation) = find(&mut lease, |r| match r {
        Report::AceDown { ace: 0, hulk, generation } => Some((hulk, generation)),
        _ => None,
    })
    .expect("the pilot hears they downed it");
    assert_ne!(hulk, NO_CHUNK, "a beam leaves a wreck");
    // Downed, it's out still (named) but no longer flies.
    assert_eq!(word(&shared), Some((suit, 0, false)));
    // Their terms say salvage: the tugs bring its wreck home, as the ace's.
    shared.control.push(Control::Claim { slot: s, hulk, generation, ace: 0 }).unwrap();
    for _ in 0..TOW_TICKS + 2 {
        sector.tick();
    }
    let (wreck, torso) = find(&mut lease, |r| match r {
        Report::Towed { wreck, torso, ace: Some(0) } => Some((wreck, torso)),
        _ => None,
    })
    .expect("the tugs came back with the ace's");
    assert!(!torso, "an ace's wreck is salvage, not a pilot's own torso");
    let desc = wreck.expect("nobody else got to it");
    assert!(matches!(desc.kind, ChunkKind::Hulk { frame, .. } if frame == ACE_FRAME));
    // Its slot long let go: none out (until the next is due).
    assert_ne!(word(&shared).map(|(_, a, _)| a), Some(0));
}

#[test]
fn an_ace_nobody_here_downed_is_nobody_s() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    let _me = launch(&mut sector, &shared, s);
    for _ in 0..40 {
        sector.tick();
    }
    let (suit, _, _) = word(&shared).expect("an ace out");
    // Downed by no pilot of the sector's.
    sector.sim.strike(usize::from(suit), Part::Torso, 1.0e9, usize::MAX, WeaponKind::BeamRifle);
    sector.tick();
    assert_eq!(find(&mut lease, |r| matches!(r, Report::AceDown { .. }).then_some(())), None);
}

#[test]
fn no_ace_without_dolls() {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, field_rocks: 0, ace_every: 30, ..SimConfig::default() },
        max_clients: 1,
        ..SectorConfig::default()
    };
    let (mut sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    for _ in 0..90 {
        sector.tick();
    }
    assert_eq!(word(&shared), None);
}
