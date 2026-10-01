//! Sleepers: an offline pilot's suit drifts on what it had, or parks on the rock it rests against
//! and hides there; a shattered rock sets it adrift; Mobile Dolls leave it alone; destroyed, it
//! stays gone and is reported; the longest asleep make room for the living.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

mod common;

use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, MODE};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::math::look_rotation;
use bc_sim::sim::{Body, Gone, SleeperFate};
use bc_sim::{DT, Sim, SimConfig, SuitId};
use common::{lone_rock, resting_on};
use glam::Vec3;

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() })
}

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn leo(sim: &mut Sim, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(FrameId::Leo, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn steps(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.step();
    }
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

fn fates(sim: &mut Sim) -> Vec<SleeperFate> {
    let mut out = Vec::new();
    sim.drain_fates(|f| out.push(f));
    out
}

#[test]
fn a_sleeper_drifts_on_what_it_had() {
    let mut sim = empty();
    let id = leo(&mut sim, Faction::Colonies, Vec3::new(0.0, 5_000.0, 9_000.0), Vec3::Z);
    let i = id.idx();
    // Flying with flight assist on (which would brake it), when the pilot drops.
    let v = Vec3::new(12.0, -3.0, 5.0);
    let spin = Vec3::new(0.0, 0.2, 0.0);
    sim.suits.flight[i].vel = v;
    sim.suits.flight[i].ang_vel = spin;
    hold(&mut sim, id, FLIGHT_ASSIST, Vec3::X);
    assert!(sim.sleep(id));
    assert!(!sim.is_parked(i), "nothing to park on out here");
    let (start, rot, prop) =
        (sim.suits.flight[i].pos, sim.suits.flight[i].rot, sim.suits.flight[i].propellant);
    steps(&mut sim, 300);
    let f = &sim.suits.flight[i];
    let expected = start + v * DT * 300.0;
    // 300 f32 steps at 9 km from the origin round to a few centimetres each way.
    assert!(f.pos.distance(expected) < 0.25, "{} vs {expected}", f.pos);
    assert_eq!(f.vel, v, "no drag, no assist");
    assert_eq!(f.ang_vel, spin, "no attitude hold: it tumbles on");
    assert!(f.rot.dot(rot).abs() < 0.99, "it turned");
    assert_eq!(f.propellant, prop, "nobody's burning anything");
}

#[test]
fn resting_on_a_rock_it_parks_and_hides_there() {
    let mut sim = sim();
    let (r_idx, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    assert_eq!(sim.parkable(i), Some(Body::Rock(r_idx as u16)), "at rest against the rock, it could park");
    assert!(sim.sleep(id));
    assert!(sim.is_parked(i));
    let at = sim.suits.flight[i].pos;
    steps(&mut sim, 300);
    assert!(sim.suits.flight[i].pos.distance(at) < 1e-3, "held where it sat");
    // Cold and still: eyes find it close in, sensors don't, even an enemy's facing it.
    let far = leo(&mut sim, Faction::Alliance, at + out * 1_000.0, -out);
    let near = leo(&mut sim, Faction::Alliance, at + out * 300.0, -out);
    steps(&mut sim, 1);
    assert!(!sim.detects(far.idx(), i), "a parked sleeper is off sensors at 1 km");
    assert!(sim.detects(near.idx(), i), "but in plain sight at 300 m");
    // Awake, it shows at 1 km like anyone.
    assert!(sim.wake(id));
    assert!(sim.detects(far.idx(), i));
}

#[test]
fn a_drifting_sleeper_is_seen_as_ever() {
    let mut sim = empty();
    let id = leo(&mut sim, Faction::Colonies, Vec3::new(0.0, 5_000.0, 9_000.0), Vec3::Z);
    let other = leo(&mut sim, Faction::Alliance, Vec3::new(0.0, 5_000.0, 10_000.0), -Vec3::Z);
    assert!(sim.sleep(id));
    steps(&mut sim, 1);
    assert!(sim.detects(other.idx(), id.idx()));
}

#[test]
fn shattering_the_rock_sets_a_parked_sleeper_adrift() {
    let mut sim = sim();
    let (r_idx, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, _) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    assert!(sim.sleep(id) && sim.is_parked(i));
    let before = sim.suits.flight[i].pos.distance(rock.pos);
    sim.field.set_dead(r_idx, true);
    steps(&mut sim, 60);
    assert!(!sim.is_parked(i));
    assert!(sim.is_sleeping(i), "still asleep, just adrift");
    assert!(sim.suits.flight[i].pos.distance(rock.pos) > before + 1.0, "it floats off where the rock was");
}

#[test]
fn mobile_dolls_leave_sleepers_alone() {
    let mut sim = Sim::new(SimConfig { target_dolls: 4, field_rocks: 0, ..SimConfig::default() });
    steps(&mut sim, 5);
    let doll =
        sim.suits.alive.iter().find(|&d| sim.suits.pilot[d] == PilotKind::MobileDoll).expect("dolls spawned");
    let near = sim.suits.flight[doll].pos + Vec3::new(0.0, 0.0, 700.0);
    let id = leo(&mut sim, Faction::Colonies, near, -Vec3::Z);
    let i = id.idx();
    assert!(sim.sleep(id));
    let hp = sim.suits.part_hp[i];
    let targeted = |sim: &Sim| {
        sim.suits.alive.iter().any(|d| {
            sim.suits.pilot[d] == PilotKind::MobileDoll
                && (sim.suits.ai[d].target == i as u16 || sim.suits.ai[d].order_target == i as u16)
        })
    };
    for _ in 0..600 {
        sim.step();
        assert!(!targeted(&sim), "a doll went for a sleeping pilot");
    }
    assert_eq!(sim.suits.part_hp[i], hp, "and nothing hit it");
    // Awake again, it's fair game.
    assert!(sim.wake(id));
    let mut hunted = false;
    for _ in 0..900 {
        sim.step();
        hunted |= targeted(&sim) || sim.suits.part_hp[i] != hp;
    }
    assert!(hunted, "the dolls ignore an awake pilot too");
}

#[test]
fn a_sleeper_destroyed_is_gone_and_its_pilot_told() {
    let mut sim = empty();
    let zero = sim
        .spawn_at(
            FrameId::WingZero,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 1_000.0, 0.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let sleeper = leo(&mut sim, Faction::Oz, Vec3::new(3.4, 1_000.6, 1_500.0), -Vec3::Z);
    assert!(sim.sleep(sleeper));
    let from = sim.events.next_seq();
    for _ in 0..60 {
        hold(&mut sim, zero, FIRE_PRIMARY, Vec3::Z);
        sim.step();
    }
    let killed = (from..sim.events.next_seq())
        .filter_map(|s| sim.events.get(s).copied())
        .any(|e| matches!(e, Event::Kill { victim, .. } if victim as usize == sleeper.idx()));
    assert!(killed, "the buster should have destroyed it");
    let f = fates(&mut sim);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].suit as usize, sleeper.idx());
    assert_eq!(f[0].gone, Gone::Destroyed { killer: zero.idx() as u16 });
    assert!(fates(&mut sim).is_empty(), "told once");
    // No respawn for a sleeper: nobody's there to fly it. The slot is freed.
    steps(&mut sim, 200);
    assert!(!sim.suits.valid(sleeper));
    assert!(!sim.wake(sleeper));
}

#[test]
fn waking_hands_the_controls_back() {
    let mut sim = empty();
    let id = leo(&mut sim, Faction::Colonies, Vec3::new(0.0, 5_000.0, 9_000.0), Vec3::Z);
    assert!(sim.sleep(id));
    // Asleep, a command is nobody's: it goes nowhere.
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, thrust: [0, 0, 127], ..InputCmd::default() },
    );
    steps(&mut sim, 1);
    assert_eq!(sim.suits.flight[id.idx()].vel, Vec3::ZERO);
    assert!(sim.wake(id));
    assert!(!sim.is_sleeping(id.idx()));
    for _ in 0..30 {
        let t = sim.next_tick();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: Vec3::Z,
                thrust: [0, 0, 127],
                ..InputCmd::default()
            },
        );
        sim.step();
    }
    assert!(sim.suits.flight[id.idx()].vel.z > 5.0, "flying again");
}

#[test]
fn a_destroyed_suit_cannot_sleep() {
    let mut sim = empty();
    let id = leo(&mut sim, Faction::Colonies, Vec3::new(0.0, 5_000.0, 9_000.0), Vec3::Z);
    sim.suits.alive.set(id.idx(), false);
    assert!(!sim.sleep(id));
}

#[test]
fn the_longest_asleep_make_room() {
    let mut sim = Sim::new(SimConfig {
        target_dolls: 0,
        field_rocks: 0,
        max_sleepers: 2,
        max_suits: 4,
        ..SimConfig::default()
    });
    let ids: Vec<SuitId> = (0..3)
        .map(|k| leo(&mut sim, Faction::Colonies, Vec3::new(k as f32 * 100.0, 5_000.0, 9_000.0), Vec3::Z))
        .collect();
    assert!(sim.sleep(ids[0]));
    steps(&mut sim, 1);
    assert!(sim.sleep(ids[1]));
    steps(&mut sim, 1);
    // A third sleeper: over the cap, so the first to fall asleep goes.
    assert!(sim.sleep(ids[2]));
    assert_eq!(sim.sleepers(), 2);
    assert!(!sim.suits.valid(ids[0]));
    let f = fates(&mut sim);
    assert_eq!((f.len(), f[0].suit as usize, f[0].gone), (1, ids[0].idx(), Gone::Evicted));
    // A newcomer needs two slots free of four: one more sleeper goes.
    assert_eq!(sim.suits.free_slots(), 2);
    sim.ensure_free_suits(3);
    assert_eq!(sim.suits.free_slots(), 3);
    assert!(!sim.suits.valid(ids[1]), "the older of the two");
    assert!(sim.suits.valid(ids[2]));
}

#[test]
fn a_sleeping_neo_bird_stays_a_bird() {
    // Asleep, the suit's input keeps MODE (and nobody works the frame's special), so a Neo-Bird
    // parked by its pilot is still a bird when they're back.
    let mut sim = empty();
    let id = sim
        .spawn_at(
            FrameId::WingZero,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 5_000.0, 9_000.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let i = id.idx();
    for _ in 0..30 {
        hold(&mut sim, id, MODE, Vec3::Z);
        sim.step();
    }
    assert_eq!(sim.suits.frame[i], FrameId::WingZeroBird, "MODE held a second makes a bird");
    assert!(sim.sleep(id));
    steps(&mut sim, 600);
    assert_eq!(sim.suits.frame[i], FrameId::WingZeroBird, "the sleeping bird kept its form");
    assert!(!sim.suits.form(i).changing());
    // Awake, its pilot has the mode again: held, it stays a bird.
    assert!(sim.wake(id));
    for _ in 0..30 {
        hold(&mut sim, id, MODE, Vec3::Z);
        sim.step();
    }
    assert_eq!(sim.suits.frame[i], FrameId::WingZeroBird);
}

#[test]
fn a_sleeping_deathscythe_drops_its_jammer() {
    let mut sim = empty();
    let id = sim
        .spawn_at(
            FrameId::Deathscythe,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 5_000.0, 9_000.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let i = id.idx();
    for _ in 0..10 {
        hold(&mut sim, id, MODE, Vec3::Z);
        sim.step();
    }
    assert!(sim.suits.special[i].active, "MODE engages the Hyper Jammer");
    let energy = sim.suits.energy[i];
    assert!(sim.sleep(id));
    steps(&mut sim, 120);
    assert!(!sim.suits.special[i].active, "asleep, the jammer is off");
    assert!(sim.suits.energy[i] > energy, "and draws nothing");
}
