//! Survival through the sector's queues: a pilot is seated only in a suit they launch (or one they
//! left asleep), docks it to go home with what it carries, and hears at once when it's destroyed;
//! once its wreck is gone the slot is free (they're back in the hangar).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;

use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::{Faction, FrameId, MAX_DATAGRAM, Part, PilotKind};
use bc_sector::{
    Comeback, Control, Outcome, Report, Sector, SectorConfig, SectorShared, SlotLease, SlotState,
};
use bc_sim::SimConfig;
use bc_sim::bodies::Body;
use bc_sim::colony::hub::{BAY_RIDE_LOCAL, bay_of_slot, bay_pose};
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::sim::Loadout;
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
    // In its pilot's own bay's cradle, at the door.
    assert_eq!(sector.sim.suits.anchor[i].body, Body::Bay(bay_of_slot(s)));
    let door = bay_pose(bay_of_slot(s), sector.sim.tick(), 0.0);
    assert!((sector.sim.suits.flight[i].pos - door.to_world(BAY_RIDE_LOCAL)).length() < 0.01);
    // Its pilot lets go: thrown out of the door.
    sector.sim.suits.input[i].buttons = FLIGHT_ASSIST;
    sector.tick();
    assert!(!sector.sim.in_bay(i));
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
    assert_eq!(lease.reports.pop(), Ok(Report::Lost { bounty: 850 }));
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
fn a_pilot_who_leaves_before_the_catapult_fires_keeps_the_suit_in_the_bay() {
    let (mut sector, shared, mut lease) = sector();
    let s = lease.slot;
    send(&mut sector, &shared, s, join(s, Some(Loadout::full(FrameId::Leo))));
    let (idx, _) = shared.slots[s as usize].suit_id().unwrap();
    let i = usize::from(idx);
    assert!(sector.sim.in_bay(i));
    // Signed in, they'd sleep in a suit out there; in the bay it goes back in instead.
    assert_eq!(send(&mut sector, &shared, s, Control::Sleep { slot: s }), (SlotState::Free, Outcome::Docked));
    let Ok(Report::Home(home)) = lease.reports.pop() else { panic!("no homecoming") };
    assert_eq!(home.frame, FrameId::Leo);
    assert!(!sector.sim.suits.used.get(i), "gone from the sector");
}
