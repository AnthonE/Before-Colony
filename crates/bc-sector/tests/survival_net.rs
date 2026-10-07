//! Survival through the sector's queues: a pilot is seated only in a suit they launch (or one they
//! left asleep), docks it to go home with what it carries, and hears at once when it's destroyed;
//! once its wreck is gone the slot is free (they're back in the hangar).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;

use bc_proto::{Faction, FrameId, MAX_DATAGRAM, Part, PilotKind};
use bc_sector::{
    Comeback, Control, Loss, Outcome, Report, Sector, SectorConfig, SectorShared, SlotLease, SlotState,
    TOW_TICKS,
};
use bc_sim::SimConfig;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::sim::{LAUNCH_GATE, Loadout};
use glam::Vec3;

fn sector() -> (Sector, Arc<SectorShared>, SlotLease) {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, field_rocks: 0, seed: 5, survival: true, ..SimConfig::default() },
        max_clients: 2,
        ..SectorConfig::default()
    };
    let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    let lease = shared.leases.pop().unwrap();
    (sector, shared, lease)
}

fn join(slot: u16, launch: Option<Loadout>) -> Control {
    Control::Join {
        slot,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback: Comeback::default(),
        launch,
    }
}

fn send(sector: &mut Sector, shared: &SectorShared, slot: u16, msg: Control) -> (SlotState, Outcome) {
    shared.control.push(msg).unwrap();
    sector.tick();
    let st = &shared.slots[slot as usize];
    (st.state(), st.outcome())
}

#[test]
fn nothing_to_launch_no_suit() {
    let (mut sector, shared, lease) = sector();
    assert_eq!(send(&mut sector, &shared, lease.slot, join(lease.slot, None)).0, SlotState::Refused);
    assert_eq!(sector.sim.suits.used.count(), 0);
}

#[test]
fn launch_dock_and_go_home_with_the_hold() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    let mut built = Loadout::full(FrameId::Leo);
    built.parts[Part::ArmR as usize] = 0.0;
    assert_eq!(send(&mut sector, &shared, s, join(s, Some(built))), (SlotState::Active, Outcome::Fresh));
    let (idx, _) = shared.slots[s as usize].suit_id().unwrap();
    let i = usize::from(idx);
    assert!((sector.sim.suits.flight[i].pos - LAUNCH_GATE).length() < 200.0);
    // Out of the dock: refused, still flying.
    sector.sim.suits.flight[i].pos = DOCK_CENTER + Vec3::new(-2_000.0, 0.0, 0.0);
    assert_eq!(send(&mut sector, &shared, s, Control::Dock { slot: s }).0, SlotState::Active);
    assert_eq!(lease.reports.pop(), Ok(Report::DockRefused));
    // At rest in the dock, with ore aboard.
    sector.sim.suits.flight[i].pos = DOCK_CENTER;
    sector.sim.suits.flight[i].vel = Vec3::ZERO;
    sector.sim.suits.cargo_kg[i] = [600, 200, 0, 0];
    assert_eq!(send(&mut sector, &shared, s, Control::Dock { slot: s }), (SlotState::Free, Outcome::Docked));
    let Ok(Report::Home(home)) = lease.reports.pop() else { panic!("no homecoming") };
    assert_eq!(home.cargo_kg, [600, 200, 0, 0]);
    assert_eq!(home.parts[Part::ArmR as usize], 0.0);
    assert!(!sector.sim.suits.used.get(i), "gone from the sector");
    // And out again.
    assert_eq!(send(&mut sector, &shared, s, join(s, Some(built))).0, SlotState::Active);
}

#[test]
fn a_suit_lost_is_reported_then_its_pilot_goes_home() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    send(&mut sector, &shared, s, join(s, Some(Loadout::full(FrameId::Leo))));
    let (idx, _) = shared.slots[s as usize].suit_id().unwrap();
    let i = usize::from(idx);
    sector.sim.suits.credits[i] = 850;
    // Shot down (as the damage step would leave it).
    sector.sim.suits.alive.set(i, false);
    sector.sim.suits.respawn_at[i] = sector.sim.tick() + 30;
    sector.tick();
    assert_eq!(lease.reports.pop(), Ok(Report::Lost { bounty: 850, how: Loss::Destroyed }));
    assert_eq!(shared.slots[s as usize].state(), SlotState::Active, "watching the wreck");
    for _ in 0..40 {
        sector.tick();
    }
    assert_eq!(
        (shared.slots[s as usize].state(), shared.slots[s as usize].outcome()),
        (SlotState::Free, Outcome::Lost)
    );
    assert!(lease.reports.pop().is_err(), "reported once");
}

#[test]
fn a_pilot_who_ejects_hears_so_and_the_tugs_bring_the_wreck_home() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    send(&mut sector, &shared, s, join(s, Some(Loadout::full(FrameId::Leo))));
    let (idx, _) = shared.slots[s as usize].suit_id().unwrap();
    let i = usize::from(idx);
    // Out of a whole suit.
    send(&mut sector, &shared, s, Control::Eject { slot: s, destruct: false });
    assert!(!sector.sim.is_alive(i));
    assert_eq!(lease.reports.pop(), Ok(Report::Lost { bounty: 0, how: Loss::Ejected }));
    // The tugs take their time: until then the wreck is out there.
    for _ in 0..TOW_TICKS - 2 {
        sector.tick();
    }
    assert!(lease.reports.pop().is_err(), "not yet");
    for _ in 0..3 {
        sector.tick();
    }
    let Ok(Report::Towed { wreck: Some(desc), torso: true }) = lease.reports.pop() else {
        panic!("the tugs brought nothing home")
    };
    assert!(matches!(desc.kind, bc_proto::ChunkKind::Hulk { frame: FrameId::Leo, .. }));
    assert!(
        sector
            .sim
            .chunks
            .alive
            .iter()
            .all(|k| !matches!(sector.sim.chunks.desc[k].kind, bc_proto::ChunkKind::Hulk { .. })),
        "it left the sector"
    );
    assert!(lease.reports.pop().is_err(), "once");
}

#[test]
fn a_wreck_is_towed_at_once_when_its_pilot_goes_and_not_at_all_when_blown_up() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    send(&mut sector, &shared, s, join(s, Some(Loadout::full(FrameId::Leo))));
    send(&mut sector, &shared, s, Control::Eject { slot: s, destruct: false });
    assert!(matches!(lease.reports.pop(), Ok(Report::Lost { how: Loss::Ejected, .. })));
    // The session is going: the tugs bring it in now.
    send(&mut sector, &shared, s, Control::Tow { slot: s });
    assert!(matches!(lease.reports.pop(), Ok(Report::Towed { wreck: Some(_), torso: true })));
    for _ in 0..TOW_TICKS + 5 {
        sector.tick();
    }
    assert!(lease.reports.pop().is_err(), "nothing more to tow");

    // A pilot blowing up their doomed suit leaves nothing for the tugs.
    while shared.slots[s as usize].state() != SlotState::Free {
        sector.tick();
    }
    send(&mut sector, &shared, s, join(s, Some(Loadout::full(FrameId::Leo))));
    let (idx, _) = shared.slots[s as usize].suit_id().unwrap();
    let i = usize::from(idx);
    // Not doomed: refused, still flying.
    send(&mut sector, &shared, s, Control::Eject { slot: s, destruct: true });
    assert!(sector.sim.is_alive(i));
    let torso = sector.sim.suits.part_hp[i][Part::Torso as usize];
    sector.sim.strike(i, Part::Torso, torso + 1.0, i, bc_proto::WeaponKind::BeamRifle);
    sector.tick();
    assert!(sector.sim.doomed(i));
    send(&mut sector, &shared, s, Control::Eject { slot: s, destruct: true });
    assert_eq!(lease.reports.pop(), Ok(Report::Lost { bounty: 0, how: Loss::Blown }));
    for _ in 0..TOW_TICKS + 5 {
        sector.tick();
    }
    assert!(lease.reports.pop().is_err(), "no tugs for a suit blown apart");
}
