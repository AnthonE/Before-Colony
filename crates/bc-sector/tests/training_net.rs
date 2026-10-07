//! The Proving Ground through the sector's queues (`docs/TRAINING.md`): a pilot boards one of the
//! Charter Board's trainers at the Blast Hall's gantry, clears the drill from there and docks back;
//! another flies the course from the inner gate. The interior sector keeps each pilot's run and
//! drill in its tick and tells their session the times on the slot's report ring, the same times
//! the pilot's own client reads from the same flight; and keeping them costs the tick nothing on
//! the heap.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_alloc::CountingAlloc;
use bc_proto::buttons::{FIRE_SECONDARY, FLIGHT_ASSIST, GRIP};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PilotKind, WeaponKind};
use bc_sector::{Comeback, Control, InputMsg, Outcome, Report, Sector, SectorConfig, SlotLease, SlotState};
use bc_sim::bodies::Body;
use bc_sim::colony::course::{self, GATES, PAD, Run, STRIP, centre, on_pad, way};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::hall::{self, DRILL, Drill, DrillEvent};
use bc_sim::colony::interior::WorldKind;
use bc_sim::ground::Footing;
use bc_sim::sim::Loadout;
use bc_sim::tuning::FlightRules;
use bc_sim::{SimConfig, TICK_HZ};
use glam::Vec3;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn inside() -> (Sector, std::sync::Arc<bc_sector::SectorShared>, SlotLease) {
    let cfg = SectorConfig {
        sim: SimConfig {
            target_dolls: 0,
            field_rocks: 0,
            landmarks: 0,
            max_sleepers: 0,
            survival: true,
            flight: FlightRules::Anime,
            world: WorldKind::Interior,
            ..SimConfig::default()
        },
        max_clients: 2,
        ..SectorConfig::default()
    };
    let (sector, shared, _egress, _oracle) = bc_sector::build(cfg);
    let lease = shared.leases.pop().unwrap();
    (sector, shared, lease)
}

/// Sends `cmd` for the tick the sector simulates next, and ticks it (counting heap operations).
fn fly(sector: &mut Sector, lease: &mut SlotLease, cmd: InputCmd) -> u64 {
    let next = sector.sim.next_tick();
    let mut p = InputPacket {
        ack_snapshot: next.saturating_sub(2),
        client_time_ms: next as u16,
        count: 1,
        ..InputPacket::default()
    };
    p.cmds[0] = InputCmd { tick: next, view_tick_q4: next << 4, ..cmd }.quantized();
    let _ = lease.input.push(InputMsg { packet: p, recv_us: u64::from(next) * 33_333 });
    let ((), n) = bc_alloc::count(|| sector.tick());
    n
}

#[test]
fn a_trainer_boarded_at_the_gantry_clears_the_drill_and_docks_back() {
    let (mut sector, shared, mut lease) = inside();
    let s = lease.slot;
    shared
        .control
        .push(Control::Board {
            slot: s,
            pilot: PilotKind::Human,
            frame: FrameId::Leo,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
            loadout: Loadout::full(FrameId::Leo),
        })
        .unwrap();
    sector.tick();
    assert_eq!(shared.slots[s as usize].state(), SlotState::Active);
    let (idx, _) = shared.slots[s as usize].suit_id().expect("a trainer");
    let i = usize::from(idx);
    assert!(sector.sim.suits.trainer.get(i));
    assert!(hall::in_gantry(sector.sim.suits.flight[i].pos, Vec3::ZERO), "on the gantry");
    assert_eq!(sector.sim.suits.footing[i], Footing::Grounded);

    // On the gantry, the machine cannon's trigger held on each lit target in turn (where it will
    // be when the rounds get there). The test keeps a drill of its own, fed from the sim's events
    // as the pilot's client is fed them, to check the sector's time against.
    let speed = bc_sim::content::weapon(WeaponKind::MachineCannon).speed;
    let muzzle = bc_sim::content::frame(FrameId::Leo).loadout[1].map_or(Vec3::ZERO, |m| m.arm.muzzle());
    let mut mine = Drill::default();
    let (mut seen, mut cleared, mut heap) = (sector.sim.events.next_seq(), None, 0);
    for _ in 0..30 * 60 {
        let t = sector.sim.next_tick();
        let lit = usize::from(mine.lit());
        let f = sector.sim.suits.flight[i];
        let ticks = hall::target(lit, t, 0.0).distance(f.pos) / speed * TICK_HZ as f32;
        let at = hall::target(lit, t + ticks.round() as u32, 0.0);
        let aim = (at - (f.pos + f.rot * muzzle)).normalize();
        heap += fly(
            &mut sector,
            &mut lease,
            InputCmd { aim, buttons: FLIGHT_ASSIST | GRIP | FIRE_SECONDARY, ..InputCmd::default() },
        );
        for seq in seen..sector.sim.events.next_seq() {
            if let Some(&Event::TargetHit { tick, target, shooter, .. }) = sector.sim.events.get(seq)
                && usize::from(shooter) == i
                && let Some(DrillEvent::Cleared(secs)) = mine.strike(target, tick)
            {
                cleared = Some(secs);
            }
        }
        seen = sector.sim.events.next_seq();
        if cleared.is_some() {
            break;
        }
    }
    let secs = cleared.unwrap_or_else(|| panic!("cleared: {} of {} struck", mine.struck(), DRILL.len()));
    println!("the drill, through the sector: {secs:.3} s");
    assert_eq!(lease.reports.pop(), Ok(Report::Drill { ms: (secs * 1_000.0).round() as u32 }));
    assert_eq!(heap, 0, "heap operations while drilling");

    // Docked back on its gantry: the pilot is out of it, and the trainer gone.
    shared.control.push(Control::Dock { slot: s }).unwrap();
    sector.tick();
    assert_eq!(
        (shared.slots[s as usize].state(), shared.slots[s as usize].outcome()),
        (SlotState::Free, Outcome::Docked)
    );
    assert!(matches!(lease.reports.pop(), Ok(Report::Home(_))));
    assert!(!sector.sim.suits.used.get(i));
    // Outside the colony there's no gantry: nobody's seated.
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() },
        max_clients: 1,
        ..SectorConfig::default()
    };
    let (mut space, shared, _e, _o) = bc_sector::build(cfg);
    let lease = shared.leases.pop().unwrap();
    let board = Control::Board {
        slot: lease.slot,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        loadout: Loadout::full(FrameId::Leo),
    };
    shared.control.push(board).unwrap();
    space.tick();
    assert_eq!(shared.slots[lease.slot as usize].state(), SlotState::Refused);
}

/// The test range (`bc_econ::proving::Trainer`): the gantry readies any build the pilot asks for,
/// carrying what its loadout says.
#[test]
fn any_build_boards_at_the_gantry() {
    let (mut sector, shared, lease) = inside();
    let s = lease.slot;
    // A Heavyarms without its missile pods' rounds and with its right arm gone, as asked.
    let mut loadout = Loadout::full(FrameId::Heavyarms);
    loadout.ammo[1] = 7;
    loadout.parts[bc_proto::Part::ArmR as usize] = 0.0;
    let board = Control::Board {
        slot: s,
        pilot: PilotKind::Human,
        frame: FrameId::Heavyarms,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        loadout,
    };
    shared.control.push(board).unwrap();
    sector.tick();
    assert_eq!(shared.slots[s as usize].state(), SlotState::Active);
    let (idx, _) = shared.slots[s as usize].suit_id().expect("a trainer");
    let i = usize::from(idx);
    assert!(sector.sim.suits.trainer.get(i));
    assert_eq!(sector.sim.suits.frame[i], FrameId::Heavyarms);
    assert_eq!(sector.sim.suits.weapons[i][1].ammo, 7);
    assert_eq!(sector.sim.suits.part_hp[i][bc_proto::Part::ArmR as usize], 0.0);
    assert!(hall::in_gantry(sector.sim.suits.flight[i].pos, Vec3::ZERO), "on the gantry");
}

/// The command flying a suit on flight assist toward `to` at up to `top` m/s, easing in over the
/// last stretch when `stop` (as `bc_client_core::course`'s test flies the course).
fn toward(f: &bc_sim::flight::FlightState, to: Vec3, top: f32, stop: bool) -> InputCmd {
    let d = to - f.pos;
    let speed = if stop { (d.length() * 0.3).min(top) } else { top };
    let want = d.normalize_or_zero() * speed;
    let local = f.rot.conjugate() * (want - f.vel);
    let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
    InputCmd {
        buttons: FLIGHT_ASSIST,
        thrust: [q(local.x), q(local.y), q(local.z)],
        aim: d.normalize_or(Vec3::X),
        ..InputCmd::default()
    }
}

#[test]
fn the_course_flown_through_the_sector_is_timed_as_its_pilot_times_it() {
    // From the inner gate by rote through every ring's middle, and down onto the pad with the grip.
    // The sector's run of it is stepped with where the suit stands at the end of each tick, and so
    // is the test's (as the pilot's prediction has it): the same time, to the millisecond.
    let (mut sector, shared, mut lease) = inside();
    let s = lease.slot;
    let join = Control::Join {
        slot: s,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback: Comeback::default(),
        launch: Some(Loadout::full(FrameId::Leo)),
    };
    shared.control.push(join).unwrap();
    sector.tick();
    let (idx, _) = shared.slots[s as usize].suit_id().expect("launched");
    let i = usize::from(idx);
    let over_pad = CityPos::new(STRIP, PAD.0, PAD.1, 30.0).to_colony();
    let (mut mine, mut flown, mut landing, mut heap) = (Run::default(), None, false, 0);
    for _ in 0..TICK_HZ * 400 {
        let f = sector.sim.suits.flight[i];
        let next = mine.next().unwrap_or(0);
        let cmd = if next < GATES.len() {
            toward(&f, centre(next) + way(next) * 20.0, 80.0, false)
        } else if !landing && (f.pos.distance(over_pad) > 3.0 || f.vel.length() > 2.0) {
            toward(&f, over_pad, 80.0, true)
        } else {
            landing = true;
            InputCmd { buttons: FLIGHT_ASSIST | GRIP, aim: Vec3::X, ..InputCmd::default() }
        };
        heap += fly(&mut sector, &mut lease, cmd);
        let f = sector.sim.suits.flight[i];
        let standing = sector.sim.suits.footing[i] == Footing::Grounded
            && sector.sim.suits.anchor[i].body == Body::City
            && on_pad(f.pos);
        if let Some(course::Event::Finished(secs)) = mine.step(f.pos, f64::from(sector.sim.tick()), standing)
        {
            flown = Some(secs);
            break;
        }
    }
    let secs = flown.expect("the course flown");
    println!("the course, through the sector: {secs:.3} s");
    assert_eq!(lease.reports.pop(), Ok(Report::Course { ms: (secs * 1_000.0).round() as u32 }));
    assert!(lease.reports.pop().is_err(), "once");
    assert_eq!(heap, 0, "heap operations flying the course");
}
