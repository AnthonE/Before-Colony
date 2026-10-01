//! Pilots leaving and coming back through the sector's queues: a suit put to sleep stays, and the
//! same pilot wakes in it; a sleeper that's gone means a new suit (with the pilot's credits); a
//! wreck can't sleep; sleepers cleared for room are reported to the server; suits on bodies are
//! counted, and one that wakes on a body keeps its grip until its pilot is heard from. Under
//! survival rules, a suit left in a landmark's hide spot is reported to the server, and the
//! server can put it back there after a restart.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::sync::Arc;

use bc_proto::buttons::GRIP;
use bc_proto::{Faction, FrameId, InputCmd, MAX_DATAGRAM, Part, PilotKind};
use bc_sector::{
    Comeback, Control, Outcome, Report, Restored, Sector, SectorConfig, SectorShared, SlotLease, SlotState,
};
use bc_sim::bodies::{Body, landmark_pose};
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::ground::{CROUCH_STANCE, Footing, STANCE};
use bc_sim::handle::Handle;
use bc_sim::sim::{Gone, Loadout, POWER_DOWN_TICKS, ParkRecord};
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

/// Survival: a Leo built new, launched from the hub.
fn launch(slot: u16) -> Control {
    Control::Join {
        slot,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback: Comeback::default(),
        launch: Some(Loadout::full(FrameId::Leo)),
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

/// A sector under survival rules (or arcade), and every slot's lease (its report ring), by slot.
fn sector_with_leases(survival: bool) -> (Sector, Arc<SectorShared>, Vec<SlotLease>) {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, seed: 5, survival, ..SimConfig::default() },
        max_clients: 4,
        ..SectorConfig::default()
    };
    let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    let mut leases: Vec<SlotLease> = std::iter::from_fn(|| shared.leases.pop()).collect();
    leases.sort_by_key(|l| l.slot);
    (sector, shared, leases)
}

/// Seats a Leo in `slot` (launched from the hub, under survival rules) and puts it on `body`
/// along `dir` (if any), standing; a few ticks on, its pilot leaves. The suit.
fn leave_on(sector: &mut Sector, shared: &SectorShared, slot: u16, on: Option<(Body, Vec3)>) -> SuitId {
    let msg = if sector.config().sim.survival { launch(slot) } else { join(slot, Comeback::default()) };
    assert_eq!(send(sector, shared, slot, msg), (SlotState::Active, Outcome::Fresh));
    let (idx, generation) = shared.slots[usize::from(slot)].suit_id().unwrap();
    let id = SuitId(Handle { idx, generation });
    if let Some((body, dir)) = on {
        assert!(sector.sim.place_on(id, body, dir), "on {body:?}");
    }
    for _ in 0..5 {
        sector.tick();
    }
    assert_eq!(send(sector, shared, slot, Control::Sleep { slot }), (SlotState::Free, Outcome::Asleep));
    id
}

/// Down in MO-II's Aft Well.
const AFT_WELL: (Body, Vec3) = (Body::Landmark(0), Vec3::new(-1.0, 0.02, 0.02));

/// Everything the slot's session has been told.
fn reports(lease: &mut SlotLease) -> Vec<Report> {
    std::iter::from_fn(|| lease.reports.pop().ok()).collect()
}

/// A suit crouched in the Aft Well, worn and laden, as its pilot left it: the sector's record.
fn parked_in_the_aft_well() -> ParkRecord {
    let (mut sector, shared, mut leases) = sector_with_leases(true);
    send(&mut sector, &shared, 0, launch(0));
    let (idx, generation) = shared.slots[0].suit_id().unwrap();
    let i = usize::from(idx);
    assert!(sector.sim.place_on(SuitId(Handle { idx, generation }), AFT_WELL.0, AFT_WELL.1));
    let s = &mut sector.sim.suits;
    s.cargo_kg[i] = [120, 0, 35, 4];
    s.credits[i] = 450;
    s.part_hp[i][Part::ArmL as usize] *= 0.5;
    s.weapons[i][1].ammo = 17;
    // Crouched: thrust[1] all the way down, held (the stance is sticky).
    let crouch = InputCmd { thrust: [0, -127, 0], buttons: GRIP, ..s.input[i] };
    for _ in 0..30 {
        let next = sector.sim.next_tick();
        sector.sim.set_input(SuitId(Handle { idx, generation }), InputCmd { tick: next, ..crouch });
        sector.sim.step();
    }
    assert_eq!(sector.sim.suits.anchor[i].stance, CROUCH_STANCE);
    send(&mut sector, &shared, 0, Control::Sleep { slot: 0 });
    match reports(&mut leases[0]).as_slice() {
        [Report::Parked { rec, .. }] => *rec,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_suit_parked_in_a_hide_spot_reports_it() {
    let (mut sector, shared, mut leases) = sector_with_leases(true);
    let id = leave_on(&mut sector, &shared, 0, Some(AFT_WELL));
    let i = id.idx();
    assert!(sector.sim.is_parked(i));
    // On the slot's report ring by the time the slot is free: the session waiting for that finds it.
    let got = reports(&mut leases[0]);
    let [Report::Parked { rec, tick }] = got.as_slice() else { panic!("{got:?}") };
    assert_eq!(*tick, sector.sim.tick() - 1, "recorded as it slept, before the tick");
    let a = sector.sim.suits.anchor[i];
    assert_eq!((rec.landmark, rec.local, rec.rot, rec.stance), (0, a.local, a.rot, a.stance));
    assert_eq!((rec.frame, rec.faction, rec.pilot), (FrameId::Leo, Faction::Colonies, PilotKind::Human));
    assert_eq!(rec.home.held, None);
    assert_eq!(rec.home.parts, sector.sim.suits.part_fractions(i));
    assert!(LANDMARKS[0].hides[0].center.distance(rec.local) <= LANDMARKS[0].hides[0].radius);

    // Parked on Hermit away from its hide spots, or asleep in open space: nothing to keep.
    leave_on(&mut sector, &shared, 1, Some((Body::Landmark(1), Vec3::new(0.3, 0.2, 1.0))));
    leave_on(&mut sector, &shared, 2, None);
    assert!(reports(&mut leases[1]).is_empty());
    assert!(reports(&mut leases[2]).is_empty());

    // Arcade rules: no suit outlives the server.
    let (mut sector, shared, mut leases) = sector_with_leases(false);
    let id = leave_on(&mut sector, &shared, 0, Some(AFT_WELL));
    assert!(sector.sim.park_record(id.idx()).is_some(), "parked in the spot");
    assert!(reports(&mut leases[0]).is_empty());
}

#[test]
fn restore_brings_a_parked_sleeper_back() {
    let rec = parked_in_the_aft_well();
    assert_eq!((rec.home.cargo_kg, rec.home.bounty, rec.stance), ([120, 0, 35, 4], 450, CROUCH_STANCE));

    // The server again, started from nothing: the suit is put back where it was left.
    let (mut sector, shared) = sector(16);
    assert!(sector.sim.suits.used.iter().next().is_none(), "an empty sector");
    shared.control.push(Control::Restore { key: 7, rec }).unwrap();
    sector.tick();
    let back = shared.restored.pop().expect("restored");
    assert!(shared.restored.pop().is_none());
    let Restored { key: 7, suit, generation } = back else { panic!("{back:?}") };
    let i = usize::from(suit);
    let (t, at) = (sector.sim.tick(), sector.sim.suits.slept_at[i]);
    assert_eq!(at, t - 1, "asleep since it came back, before the tick");
    assert!(sector.sim.is_parked(i));
    assert_eq!(sector.sim.footing(i), Footing::Grounded);
    let a = sector.sim.suits.anchor[i];
    assert_eq!((a.body, a.local, a.rot, a.stance), (Body::Landmark(0), rec.local, rec.rot, rec.stance));
    let pose = landmark_pose(&LANDMARKS[0], t, 0.0);
    assert_eq!(sector.sim.suits.flight[i].pos, pose.pos + pose.rot * rec.local, "body(t)∘local");
    assert_eq!(sector.sim.suits.flight[i].vel, pose.point_vel(sector.sim.suits.flight[i].pos));
    // As it was: armour, weapons, rounds, tank, hold and bounty.
    let again = sector.sim.park_record(i).expect("a record of it");
    assert_eq!((again.landmark, again.local, again.rot, again.stance), (0, rec.local, rec.rot, rec.stance));
    let (h, was) = (again.home, rec.home);
    assert_eq!(
        (h.mounts, h.ammo, h.cargo_kg, h.bounty, h.frame),
        (was.mounts, was.ammo, was.cargo_kg, 450, was.frame)
    );
    assert_eq!(h.propellant, was.propellant);
    for (p, q) in h.parts.iter().zip(was.parts) {
        assert!((p - q).abs() < 1e-6, "{:?} vs {:?}", h.parts, was.parts);
    }
    assert!(h.parts[Part::ArmL as usize] < 0.6, "still worn");

    // Like any suit just parked: in sight for the power-down, then dark.
    let count = |c| bc_sector::Metrics::load(c);
    let m = &shared.metrics;
    assert_eq!((count(&m.sleepers), count(&m.parked), count(&m.sleepers_hidden)), (1, 1, 0));
    while sector.sim.tick() < at + POWER_DOWN_TICKS {
        assert!(sector.sim.concealment(i).sig > 0.0, "dark at {}", sector.sim.tick() - at);
        sector.tick();
    }
    assert_eq!(sector.sim.concealment(i).sig, 0.0);
    assert_eq!((count(&m.parked), count(&m.sleepers_hidden)), (1, 1), "dark");

    // Its pilot comes back to it, crouched where it was and still gripping.
    let comeback = Comeback { sleeper: Some((suit, generation)), credits: 0 };
    assert_eq!(send(&mut sector, &shared, 0, join(0, comeback)), (SlotState::Active, Outcome::Woke));
    for _ in 0..30 {
        sector.tick();
        assert_eq!(sector.sim.footing(i), Footing::Grounded);
    }
    let a = sector.sim.suits.anchor[i];
    assert_eq!(a.stance, CROUCH_STANCE);
    assert!(a.local.distance(rec.local) < 1e-3, "moved {} m", a.local.distance(rec.local));

    // Nowhere to put one: a landmark this sector hasn't got, or a spot it isn't in.
    let elsewhere = ParkRecord { local: rec.local + Vec3::new(0.0, 0.0, 200.0), ..rec };
    for (key, bad) in [(1, ParkRecord { landmark: 2, ..rec }), (2, elsewhere)] {
        shared.control.push(Control::Restore { key, rec: bad }).unwrap();
    }
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, landmarks: 0, ..SimConfig::default() },
        max_clients: 1,
        ..SectorConfig::default()
    };
    let (mut bare, bare_shared, _egress, _oracle) = bc_sector::build(cfg);
    bare_shared.control.push(Control::Restore { key: 3, rec }).unwrap();
    for _ in 0..3 {
        sector.tick();
        bare.tick();
    }
    assert!(shared.restored.pop().is_none() && bare_shared.restored.pop().is_none());
    assert_eq!(sector.sim.sleepers() + bare.sim.sleepers(), 0);
}

#[test]
fn a_hidden_sleeper_that_is_hit_reports_what_is_left_of_it() {
    let (mut sector, shared, _leases) = sector_with_leases(true);
    let id = leave_on(&mut sector, &shared, 0, Some(AFT_WELL));
    let i = id.idx();
    // One asleep in open space has nothing to keep, hit or not.
    let open = leave_on(&mut sector, &shared, 1, None);
    sector.tick();
    assert!(shared.reparked.pop().is_none(), "nothing hit");
    // Its left arm shot off (as the damage step leaves a suit it hits).
    let next = sector.sim.next_tick();
    for j in [i, open.idx()] {
        sector.sim.suits.last_hit[j] = next;
    }
    sector.sim.suits.part_hp[i][Part::ArmL as usize] = 0.0;
    sector.tick();
    let r = shared.reparked.pop().expect("reported");
    assert!(shared.reparked.pop().is_none(), "only the hidden one");
    assert_eq!((r.suit, r.generation, r.tick), (id.0.idx, id.0.generation, next));
    assert_eq!(r.rec.home.parts[Part::ArmL as usize], 0.0);
    assert_eq!(Some(r.rec), sector.sim.park_record(i));
    sector.tick();
    assert!(shared.reparked.pop().is_none(), "once a hit");
}

#[test]
fn a_suit_put_back_too_late_is_discarded_quietly() {
    let rec = parked_in_the_aft_well();
    let (mut sector, shared) = sector(16);
    shared.control.push(Control::Restore { key: 1, rec }).unwrap();
    sector.tick();
    let Restored { suit, generation, .. } = shared.restored.pop().expect("restored");
    // The server had stopped waiting, and its record has let the suit go: it goes, with no fate to
    // report and nothing left behind.
    let chunks = sector.sim.chunks.count();
    shared.control.push(Control::Discard { suit, generation }).unwrap();
    sector.tick();
    assert_eq!(sector.sim.sleepers(), 0);
    assert!(!sector.sim.suits.used.get(usize::from(suit)));
    assert!(shared.notes.pop().is_none(), "no fate");
    assert_eq!(sector.sim.chunks.count(), chunks, "nothing spilled");
}
