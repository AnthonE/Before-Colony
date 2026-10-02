//! Missile locks and homing missiles: a lock builds while the designation is held in the lock cone
//! and falls apart twice as fast; a guided salvo runs down a crossing target; a target faster than
//! the motor's Δv outruns it; a jammer breaks the seeker's hold, and so does a target parking and
//! going dark (but not before it has, nor to its friends); missiles pass friends; a missile that
//! finds nothing bursts at the end of its life; a full pool swallows launches; chaff throws a salvo
//! off and keeps the next lock from building.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

mod common;

use bc_proto::buttons::{FIRE_SECONDARY, MODE};
use bc_proto::events::{BurstCause, Event};
use bc_proto::snapshot::own_flags;
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind, WeaponKind};
use bc_sim::content::{Kit, WeaponClass, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::missiles::MAX_MISSILES;
use bc_sim::{Sim, SimConfig, SuitId};
use common::{lone_rock, resting_on};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// `id` aims at `at` (a suit) and designates it, with `buttons`.
fn press(sim: &mut Sim, id: SuitId, buttons: u16, at: SuitId) {
    let t = sim.next_tick();
    let to = sim.suits.flight[at.idx()].pos - sim.suits.flight[id.idx()].pos;
    let cmd = InputCmd {
        tick: t,
        view_tick_q4: t << 4,
        aim: to.normalize(),
        buttons,
        lock_target: at.idx() as u16,
        ..InputCmd::default()
    };
    sim.set_input(id, cmd);
}

/// Keeps `id` coasting at `vel` (flight assist off, no thrust).
fn coast(sim: &mut Sim, id: SuitId, vel: Vec3) {
    sim.suits.flight[id.idx()].vel = vel;
    let t = sim.next_tick();
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, lock_target: NO_SLOT, ..InputCmd::default() },
    );
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);
const LOCK_TICKS: u32 = 15;

fn missile_hits(sim: &Sim, from: u32, target: SuitId) -> usize {
    events_since(sim, from)
        .iter()
        .filter(|e| {
            matches!(e, Event::Hit { target: t, weapon: WeaponKind::HomingMissile, .. } if *t as usize == target.idx())
        })
        .count()
}

#[test]
fn a_lock_builds_while_held_and_falls_apart_twice_as_fast() {
    let spec = frame(FrameId::Heavyarms).lock_spec().expect("a launcher");
    assert_eq!(u32::from(spec.lock_ticks), LOCK_TICKS);
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    for k in 1..=LOCK_TICKS {
        press(&mut sim, ha, 0, leo);
        sim.step();
        let own = sim.own_state(ha.idx());
        assert_eq!(own.flags & own_flags::LOCK_ACQUIRED != 0, k == LOCK_TICKS, "tick {k}");
    }
    assert_eq!(sim.own_state(ha.idx()).lock_progress, 15);
    let theirs = sim.own_state(leo.idx()).flags;
    assert_ne!(theirs & own_flags::MISSILE_LOCK, 0, "the Leo knows it's locked");
    // Aimed away from it (the designation held), the lock goes in half the time.
    let mut ticks = 0;
    while sim.suits.lock[ha.idx()].target != NO_SLOT {
        let t = sim.next_tick();
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim: Vec3::X,
            lock_target: leo.idx() as u16,
            ..InputCmd::default()
        };
        sim.set_input(ha, cmd);
        sim.step();
        ticks += 1;
        assert!(ticks < 30);
    }
    assert_eq!(ticks, LOCK_TICKS.div_ceil(2));
    assert_eq!(sim.own_state(leo.idx()).flags & own_flags::MISSILE_LOCK, 0);
    // Past the lock's range it never builds.
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let far = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * (spec.lock_range + 100.0), -Vec3::Z);
    for _ in 0..30 {
        press(&mut sim, ha, 0, far);
        sim.step();
    }
    assert_eq!(sim.suits.lock[ha.idx()].progress, 0);
}

/// Heavyarms locks a Leo 1.5 km ahead coasting at `leo_vel`, fires one salvo, and watches for
/// `secs`; `each` runs every tick.
fn salvo_at(
    target: FrameId,
    leo_vel: Vec3,
    secs: u32,
    each: &mut dyn FnMut(&mut Sim, SuitId, u32),
) -> (Sim, SuitId, u32) {
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let leo = suit(&mut sim, target, Faction::Oz, AT + Vec3::Z * 1_500.0, -Vec3::Z);
    for _ in 0..LOCK_TICKS {
        press(&mut sim, ha, 0, leo);
        coast(&mut sim, leo, Vec3::ZERO);
        sim.step();
    }
    assert!(sim.missile_lock(ha.idx()).is_some());
    let from = sim.events.next_seq();
    for k in 0..secs * 30 {
        press(&mut sim, ha, if k == 0 { FIRE_SECONDARY } else { 0 }, leo);
        coast(&mut sim, leo, leo_vel);
        each(&mut sim, leo, k);
        sim.step();
    }
    (sim, leo, from)
}

#[test]
fn a_guided_salvo_runs_down_a_crossing_target() {
    let (sim, leo, from) = salvo_at(FrameId::Leo, Vec3::X * 120.0, 8, &mut |_, _, _| {});
    let hits = missile_hits(&sim, from, leo);
    let salvo = usize::from(weapon(WeaponKind::HomingMissile).salvo);
    assert!(hits >= salvo - 1, "{hits} of {salvo} hit");
    assert!(
        events_since(&sim, from)
            .iter()
            .any(|e| matches!(e, Event::MissileBurst { cause: BurstCause::Hit, .. }))
    );
    assert_eq!(sim.stats(0).hits_by_class[WeaponClass::Missile as usize] as usize, hits);
}

#[test]
fn chaff_throws_a_salvo_off_and_keeps_locks_off() {
    let (sim, leo, from) = salvo_at(FrameId::Leo, Vec3::X * 120.0, 8, &mut |sim, leo, k| {
        if k == 0 {
            sim.suits.kits[leo.idx()].set(Kit::Chaff, 1);
        }
        // The salvo is on its way: chaff.
        if k == 20 {
            assert!(sim.suits.incoming[leo.idx()] > 0);
            assert!(sim.use_kit(leo, Kit::Chaff));
            assert!(!sim.use_kit(leo, Kit::Chaff), "the rack is empty");
        }
    });
    assert_eq!(missile_hits(&sim, from, leo), 0);
    assert_eq!(sim.suits.kits[leo.idx()].get(Kit::Chaff), 0);
    // A lock started under the chaff doesn't build until it clears.
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 1_500.0, -Vec3::Z);
    sim.suits.kits[leo.idx()].set(Kit::Chaff, 1);
    for k in 0..LOCK_TICKS {
        press(&mut sim, ha, 0, leo);
        coast(&mut sim, leo, Vec3::ZERO);
        sim.step();
        if k == LOCK_TICKS / 2 {
            assert!(sim.suits.lock[ha.idx()].progress > 0);
            assert!(sim.use_kit(leo, Kit::Chaff));
            assert_eq!(sim.suits.lock[ha.idx()].progress, 0, "broken at once");
        }
    }
    assert!(sim.missile_lock(ha.idx()).is_none());
    for _ in 0..u32::from(bc_sim::content::kits::CHAFF_TICKS) + LOCK_TICKS {
        press(&mut sim, ha, 0, leo);
        coast(&mut sim, leo, Vec3::ZERO);
        sim.step();
    }
    assert!(sim.missile_lock(ha.idx()).is_some(), "once it clears, the lock builds again");
}

#[test]
fn a_target_faster_than_the_motor_outruns_it() {
    let spec = weapon(WeaponKind::HomingMissile).missile.unwrap();
    let fast = spec.launch_speed + spec.dv + 300.0;
    let (sim, leo, from) = salvo_at(FrameId::Leo, Vec3::Z * fast, 9, &mut |_, _, _| {});
    assert_eq!(missile_hits(&sim, from, leo), 0);
    let expired = events_since(&sim, from)
        .iter()
        .filter(|e| matches!(e, Event::MissileBurst { cause: BurstCause::Expired, .. }))
        .count();
    assert_eq!(expired, usize::from(weapon(WeaponKind::HomingMissile).salvo));
}

#[test]
fn a_jammer_breaks_the_seekers_hold() {
    // Deathscythe jams a second after the launch and breaks the other way.
    let mut jamming = |sim: &mut Sim, ds: SuitId, k: u32| {
        if k >= 30 {
            coast(sim, ds, Vec3::X * -150.0);
            let t = sim.next_tick();
            let aim = sim.suits.flight[ds.idx()].rot * Vec3::Z;
            sim.set_input(
                ds,
                InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons: MODE, ..InputCmd::default() },
            );
        }
    };
    let (sim, ds, from) = salvo_at(FrameId::Deathscythe, Vec3::X * 150.0, 8, &mut jamming);
    assert_eq!(missile_hits(&sim, from, ds), 0, "the missiles still found it");
    // Without the jammer, the same break doesn't save it.
    let mut breaking = |sim: &mut Sim, ds: SuitId, k: u32| {
        if k >= 30 {
            coast(sim, ds, Vec3::X * -150.0);
        }
    };
    let (sim, ds, from) = salvo_at(FrameId::Deathscythe, Vec3::X * 150.0, 8, &mut breaking);
    assert!(missile_hits(&sim, from, ds) > 0);
}

#[test]
fn missiles_pass_friends_and_burst_at_the_end_of_their_life() {
    let spec = weapon(WeaponKind::HomingMissile).missile.unwrap();
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    // A friend right in the way, and nothing else.
    let friend = suit(&mut sim, FrameId::Leo, Faction::Colonies, AT + Vec3::new(0.0, 5.8, 200.0), -Vec3::Z);
    let from = sim.events.next_seq();
    let mut launched_at = None;
    for k in 0..u32::from(spec.life) + 30 {
        let t = sim.next_tick();
        let buttons = if k == 0 { FIRE_SECONDARY } else { 0 };
        sim.set_input(
            ha,
            InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, buttons, ..InputCmd::default() },
        );
        coast(&mut sim, friend, Vec3::ZERO);
        sim.step();
        if launched_at.is_none() && sim.missiles.count() > 0 {
            launched_at = Some(sim.tick());
        }
    }
    let ev = events_since(&sim, from);
    assert!(ev.iter().all(|e| !matches!(e, Event::Hit { .. })), "a missile hit a friend");
    let bursts: Vec<u32> = ev
        .iter()
        .filter_map(|e| match e {
            Event::MissileBurst { tick, cause: BurstCause::Expired, .. } => Some(*tick),
            _ => None,
        })
        .collect();
    assert_eq!(bursts.len(), usize::from(weapon(WeaponKind::HomingMissile).salvo));
    assert_eq!(bursts[0], launched_at.unwrap() + u32::from(spec.life));
    assert_eq!(sim.missiles.count(), 0);
}

#[test]
fn a_full_pool_swallows_launches() {
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    while sim
        .missiles
        .spawn(
            WeaponKind::HomingMissile,
            0,
            Faction::Oz,
            (NO_SLOT, 0),
            AT + Vec3::Y * 500.0,
            Vec3::ZERO,
            0.0,
            1_000_000,
        )
        .is_some()
    {}
    assert_eq!(sim.missiles.count(), MAX_MISSILES);
    let ammo = sim.suits.weapons[ha.idx()][1].ammo;
    for k in 0..30 {
        let t = sim.next_tick();
        let buttons = if k == 0 { FIRE_SECONDARY } else { 0 };
        sim.set_input(
            ha,
            InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, buttons, ..InputCmd::default() },
        );
        sim.step();
    }
    assert_eq!(sim.suits.weapons[ha.idx()][1].ammo, ammo, "rounds spent on missiles that never left");
    assert_eq!(sim.missiles.count(), MAX_MISSILES);
}

/// Heavyarms locks a Leo resting against a lone rock 1.5 km off, crossing at 150 m/s so that
/// only homing brings its missiles in, and lets a salvo go; once it's away, the Leo parks there,
/// asleep, and `parked` runs (on the sim, the Leo, and the tick it slept). The hits it took.
fn salvo_at_a_suit_that_parks(parked: impl FnOnce(&mut Sim, SuitId, u32)) -> usize {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() });
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (leo, out) = resting_on(&mut sim, &rock);
    // Ten seconds on the clock, so its sleep can be backdated by that much.
    for _ in 0..300 {
        sim.step();
    }
    let at = sim.suits.flight[leo.idx()].pos;
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Oz, at + out * 1_500.0, -out);
    let across = out.cross(Vec3::Y).normalize() * 150.0;
    for _ in 0..LOCK_TICKS {
        sim.suits.flight[ha.idx()].vel = across;
        press(&mut sim, ha, 0, leo);
        sim.step();
    }
    assert!(sim.missile_lock(ha.idx()).is_some());
    let salvo = usize::from(weapon(WeaponKind::HomingMissile).salvo);
    let from = sim.events.next_seq();
    let mut k = 0;
    while sim.missiles.count() < salvo {
        press(&mut sim, ha, if k == 0 { FIRE_SECONDARY } else { 0 }, leo);
        sim.step();
        k += 1;
        assert!(k < 30, "the salvo never left");
    }
    // The whole salvo away and homing, its target parks.
    assert!(sim.sleep(leo) && sim.is_parked(leo.idx()));
    let t = sim.tick();
    parked(&mut sim, leo, t);
    for _ in 0..8 * 30 {
        sim.step();
    }
    missile_hits(&sim, from, leo)
}

#[test]
fn a_guided_missile_loses_a_suit_that_parks_and_goes_dark() {
    // As if it had slept for 10 s: its reactor has idled down, and the seekers lose it on their
    // next look.
    let hits = salvo_at_a_suit_that_parks(|sim, leo, t| sim.suits.slept_at[leo.idx()] = t - 300);
    assert_eq!(hits, 0, "the seekers kept a parked suit gone dark");
}

#[test]
fn a_guided_missile_keeps_a_suit_that_parks_while_it_powers_down() {
    // Just asleep, it shows for 8 s yet (POWER_DOWN_TICKS): the salvo is in by then.
    let hits = salvo_at_a_suit_that_parks(|_, _, _| {});
    assert!(hits > 0, "the seekers lost a parked suit before it went dark");
}

#[test]
fn a_guided_missile_keeps_a_friend_s_parked_suit_in_its_sights() {
    // Concealment is from enemies only: an OZ salvo that locked an OZ sleeper (friendly fire on)
    // runs it down however dark it is to the Colonies.
    let mut sim = Sim::new(SimConfig { target_dolls: 0, friendly_fire: true, ..SimConfig::default() });
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (leo, out) = resting_on(&mut sim, &rock);
    sim.suits.faction[leo.idx()] = Faction::Oz;
    for _ in 0..300 {
        sim.step();
    }
    let at = sim.suits.flight[leo.idx()].pos;
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Oz, at + out * 1_500.0, -out);
    let across = out.cross(Vec3::Y).normalize() * 150.0;
    let salvo = usize::from(weapon(WeaponKind::HomingMissile).salvo);
    // An ally is no designation: the salvo is given its target by hand, as a lock would.
    let from = sim.events.next_seq();
    let mut k = 0;
    while sim.missiles.count() < salvo {
        sim.suits.flight[ha.idx()].vel = across;
        press(&mut sim, ha, if k == 0 { FIRE_SECONDARY } else { 0 }, leo);
        sim.step();
        for m in sim.missiles.alive.iter() {
            sim.missiles.target[m] = leo.idx() as u16;
            sim.missiles.target_gen[m] = sim.suits.generation[leo.idx()];
        }
        k += 1;
        assert!(k < 30, "the salvo never left");
    }
    assert!(sim.sleep(leo) && sim.is_parked(leo.idx()));
    sim.suits.slept_at[leo.idx()] = sim.tick() - 300;
    assert_eq!(sim.concealment(leo.idx()).sig, 0.0, "dark to its enemies");
    for _ in 0..8 * 30 {
        sim.step();
    }
    assert!(missile_hits(&sim, from, leo) > 0, "an ally's seeker lost it");
}
