//! The colony's reach (`bc_sim::colony::reach`) under `Law::OpenSpace`: out in open space pilots
//! of one side trade hits by every weapon (beams through the lag-compensated sweep and as rounds,
//! the cannon, a locked missile, a blade, the flamethrower, a reactor's blast); in the reach, or
//! across its line either way, nothing a pilot does lands on a pilot, and a round passes on to a
//! drone behind; drones fight by side wherever they are; a ward stops an open-space pilot's beam;
//! the lock follows the law; and under `Law::Sides` a same-side pilot is never hit.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE, MODE};
use bc_proto::events::{BurstCause, Event};
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, Part, PilotKind, WeaponKind};
use bc_sim::colony::reach::{Law, REACH_M, in_reach, past_line};
use bc_sim::content::{WARD_RADIUS, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::sim::Ejected;
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

/// Open space, by the field.
const OUT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);
/// In the reach: 1.5 km off the ring's hull, on the side away from Fornax.
const IN: Vec3 = Vec3::new(COLONY_CENTER.x, COLONY_CENTER.y, COLONY_CENTER.z - COLONY_RADIUS - 1_500.0);

fn sector(law: Law) -> Sim {
    Sim::new(SimConfig { target_drones: 0, field_rocks: 0, law, ..SimConfig::default() })
}

fn suit(
    sim: &mut Sim,
    frame: FrameId,
    faction: Faction,
    pilot: PilotKind,
    pos: Vec3,
    facing: Vec3,
) -> SuitId {
    sim.spawn_at(frame, faction, pilot, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn pilot(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    suit(sim, frame, faction, PilotKind::Human, pos, facing)
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// Hits `by` landed on `on` since `from`, by any weapon.
fn hits(sim: &Sim, from: u32, by: SuitId, on: SuitId) -> usize {
    events_since(sim, from)
        .iter()
        .filter(|e| {
            matches!(e, Event::Hit { shooter, target, .. }
                if *shooter as usize == by.idx() && *target as usize == on.idx())
        })
        .count()
}

/// `id` holds `buttons` this tick, aiming at `at`, its view `lag` ticks behind (lag compensation:
/// a beam then resolves through the history's sweep), designating `lock`.
fn press(sim: &mut Sim, id: SuitId, buttons: u16, at: Vec3, lag: u32, lock: u16) {
    let t = sim.next_tick();
    let aim = (at - sim.suits.flight[id.idx()].pos).normalize_or(Vec3::Z);
    let view = t.saturating_sub(lag);
    let cmd =
        InputCmd { tick: t, view_tick_q4: view << 4, aim, buttons, lock_target: lock, ..InputCmd::default() };
    sim.set_input(id, cmd);
}

/// Holds `id` still where it is.
fn still(sim: &mut Sim, id: SuitId) {
    sim.suits.flight[id.idx()].vel = Vec3::ZERO;
    let at = sim.suits.flight[id.idx()].pos + sim.suits.aim[id.idx()];
    press(sim, id, 0, at, 0, NO_SLOT);
}

/// `a` fires `buttons` at `b` (`d` m ahead of `at`, the way `dir` points) for `ticks`, tapping
/// whenever it's ready, its view `lag` ticks behind: the hits it landed.
#[allow(clippy::too_many_arguments)]
fn volley(
    law: Law,
    frame_a: FrameId,
    at: Vec3,
    dir: Vec3,
    d: f32,
    buttons: u16,
    ticks: u32,
    lag: u32,
) -> usize {
    let mut sim = sector(law);
    let a = pilot(&mut sim, frame_a, Faction::Colonies, at, dir);
    let b = pilot(&mut sim, FrameId::Argo, Faction::Colonies, at + dir * d, -dir);
    for _ in 0..4 {
        still(&mut sim, a);
        still(&mut sim, b);
        sim.step();
    }
    let from = sim.events.next_seq();
    for _ in 0..ticks {
        let target = sim.suits.flight[b.idx()].pos;
        let slot = if buttons & FIRE_SECONDARY != 0 { 1 } else { 0 };
        let pull = if buttons & MELEE != 0 || sim.would_fire(a.idx(), slot) { buttons } else { 0 };
        press(&mut sim, a, pull, target, lag, NO_SLOT);
        still(&mut sim, b);
        sim.step();
    }
    hits(&sim, from, a, b)
}

#[test]
fn the_test_places_are_where_they_say() {
    assert!(!in_reach(OUT));
    assert!(in_reach(IN));
    assert!((past_line(IN) + REACH_M - 1_500.0).abs() < 1.0);
}

#[test]
fn out_in_open_space_one_side_trades_beams_rounds_and_blows() {
    // The beam rifle through the lag-compensated sweep, and as a round in flight.
    assert!(
        volley(Law::OpenSpace, FrameId::Argo, OUT, Vec3::Z, 800.0, FIRE_PRIMARY, 90, 3) >= 2,
        "beams, swept"
    );
    assert!(
        volley(Law::OpenSpace, FrameId::Argo, OUT, Vec3::Z, 800.0, FIRE_PRIMARY, 90, 0) >= 2,
        "beams, flown"
    );
    // The machine cannon's rounds.
    assert!(volley(Law::OpenSpace, FrameId::Argo, OUT, Vec3::Z, 400.0, FIRE_SECONDARY, 60, 0) >= 2, "rounds");
    // A blade at 9 m.
    assert!(volley(Law::OpenSpace, FrameId::Argo, OUT, Vec3::Z, 9.0, MELEE, 20, 0) >= 1, "a blade");
    // The flamethrower at 40 m.
    assert!(volley(Law::OpenSpace, FrameId::Thuban, OUT, Vec3::Z, 40.0, FIRE_SECONDARY, 30, 0) >= 1, "flame");
}

#[test]
fn under_sides_a_same_side_pilot_is_never_hit() {
    for (frame_a, d, buttons, lag) in [
        (FrameId::Argo, 800.0, FIRE_PRIMARY, 3),
        (FrameId::Argo, 800.0, FIRE_PRIMARY, 0),
        (FrameId::Argo, 400.0, FIRE_SECONDARY, 0),
        (FrameId::Argo, 9.0, MELEE, 0),
        (FrameId::Thuban, 40.0, FIRE_SECONDARY, 0),
    ] {
        assert_eq!(
            volley(Law::Sides, frame_a, OUT, Vec3::Z, d, buttons, 60, lag),
            0,
            "{frame_a:?} {buttons}"
        );
    }
}

#[test]
fn in_the_reach_nothing_a_pilot_does_lands_on_a_pilot() {
    let dir = -Vec3::X;
    for (frame_a, d, buttons, lag) in [
        (FrameId::Argo, 800.0, FIRE_PRIMARY, 3),
        (FrameId::Argo, 800.0, FIRE_PRIMARY, 0),
        (FrameId::Argo, 400.0, FIRE_SECONDARY, 0),
        (FrameId::Argo, 9.0, MELEE, 0),
        (FrameId::Thuban, 40.0, FIRE_SECONDARY, 0),
    ] {
        let n = volley(Law::OpenSpace, frame_a, IN, dir, d, buttons, 60, lag);
        assert_eq!(n, 0, "{frame_a:?} {buttons}");
    }
}

#[test]
fn across_the_line_nothing_lands_either_way() {
    // The line off the hull, on −Z: 150 m out and 150 m in, 300 m apart.
    let line = COLONY_CENTER - Vec3::Z * (COLONY_RADIUS + REACH_M);
    let (out, inside) = (line - Vec3::Z * 150.0, line + Vec3::Z * 150.0);
    assert!(!in_reach(out) && in_reach(inside));
    for (from, to) in [(out, inside), (inside, out)] {
        let dir = (to - from).normalize();
        for lag in [0, 3] {
            assert_eq!(volley(Law::OpenSpace, FrameId::Argo, from, dir, 300.0, FIRE_PRIMARY, 60, lag), 0);
        }
        assert_eq!(volley(Law::OpenSpace, FrameId::Argo, from, dir, 300.0, FIRE_SECONDARY, 60, 0), 0);
    }
}

#[test]
fn a_round_passes_a_pilot_in_the_reach_on_to_a_drone_behind() {
    let mut sim = sector(Law::OpenSpace);
    let dir = -Vec3::X;
    let a = pilot(&mut sim, FrameId::Argo, Faction::Colonies, IN, dir);
    let b = pilot(&mut sim, FrameId::Argo, Faction::Colonies, IN + dir * 400.0, -dir);
    let post = IN + dir * 700.0;
    let drone = suit(&mut sim, FrameId::Corvus, Faction::Almagest, PilotKind::Drone, post, -dir);
    let from = sim.events.next_seq();
    for _ in 0..90 {
        // Along the line through both: the muzzle's offset keeps it on the pilot ahead too.
        let f = &mut sim.suits.flight[drone.idx()];
        (f.pos, f.vel) = (post, Vec3::ZERO);
        let pull = if sim.would_fire(a.idx(), 0) { FIRE_PRIMARY } else { 0 };
        press(&mut sim, a, pull, post, 0, NO_SLOT);
        sim.suits.flight[a.idx()].vel = Vec3::ZERO;
        still(&mut sim, b);
        sim.step();
    }
    assert_eq!(hits(&sim, from, a, b), 0);
    assert!(hits(&sim, from, a, drone) >= 2, "the drone behind is hit");
}

#[test]
fn drones_fight_by_side_in_the_reach() {
    let mut sim = sector(Law::OpenSpace);
    let human = pilot(&mut sim, FrameId::Argo, Faction::Colonies, IN, Vec3::Z);
    let post = IN - Vec3::X * 700.0;
    let drone = suit(&mut sim, FrameId::Corvus, Faction::Almagest, PilotKind::Drone, post, Vec3::X);
    let from = sim.events.next_seq();
    for _ in 0..30 * 20 {
        // Its brain flies and fires; it's held on its post.
        let f = &mut sim.suits.flight[drone.idx()];
        (f.pos, f.vel) = (post, Vec3::ZERO);
        let at = sim.suits.flight[drone.idx()].pos;
        let pull = if sim.would_fire(human.idx(), 0) { FIRE_PRIMARY } else { 0 };
        press(&mut sim, human, pull, at, 0, NO_SLOT);
        sim.suits.flight[human.idx()].vel = Vec3::ZERO;
        sim.step();
    }
    assert!(hits(&sim, from, drone, human) >= 1, "a drone still harms a pilot in the reach");
    assert!(hits(&sim, from, human, drone) >= 1, "and a pilot there still harms a drone");
}

#[test]
fn a_round_on_the_hull_harms_nothing() {
    let mut sim = sector(Law::OpenSpace);
    let a = pilot(&mut sim, FrameId::Argo, Faction::Colonies, IN, Vec3::Z);
    let from = sim.events.next_seq();
    for _ in 0..90 {
        let pull = if sim.would_fire(a.idx(), 0) { FIRE_PRIMARY | FIRE_SECONDARY } else { FIRE_SECONDARY };
        press(&mut sim, a, pull, COLONY_CENTER, 0, NO_SLOT);
        sim.step();
    }
    assert!(sim.stats(a.idx()).shots > 2);
    assert!(!events_since(&sim, from).iter().any(|e| matches!(e, Event::Hit { .. })));
}

/// Antares (Colonies) holds `target`'s designation at `at` for the lock's time under `law`: whether
/// it's designated and locked.
fn lock_on(law: Law, at: Vec3, faction: Faction) -> (bool, bool) {
    let mut sim = sector(law);
    let ha = pilot(&mut sim, FrameId::Antares, Faction::Colonies, at, Vec3::Z);
    let b = pilot(&mut sim, FrameId::Argo, faction, at + Vec3::Z * 1_500.0, -Vec3::Z);
    let spec = frame(FrameId::Antares).lock_spec().unwrap();
    for _ in 0..=spec.lock_ticks {
        let to = sim.suits.flight[b.idx()].pos;
        press(&mut sim, ha, 0, to, 0, b.idx() as u16);
        still(&mut sim, b);
        sim.step();
    }
    (sim.designation(ha.idx()) == Some(b.idx()), sim.missile_lock(ha.idx()).is_some())
}

#[test]
fn the_lock_follows_the_law() {
    assert_eq!(lock_on(Law::OpenSpace, OUT, Faction::Colonies), (true, true), "open space: fair game");
    assert_eq!(lock_on(Law::OpenSpace, IN, Faction::Colonies), (false, false), "the reach");
    assert_eq!(lock_on(Law::OpenSpace, IN, Faction::Alliance), (false, false), "every side, in the reach");
    assert_eq!(lock_on(Law::Sides, OUT, Faction::Colonies), (false, false), "sides: an ally");
    assert_eq!(lock_on(Law::Sides, IN, Faction::Alliance), (true, true), "sides: another side, anywhere");
}

#[test]
fn a_locked_salvo_runs_down_a_same_side_pilot_in_open_space() {
    let mut sim = sector(Law::OpenSpace);
    let ha = pilot(&mut sim, FrameId::Antares, Faction::Colonies, OUT, Vec3::Z);
    let b = pilot(&mut sim, FrameId::Argo, Faction::Colonies, OUT + Vec3::Z * 1_500.0, -Vec3::Z);
    let spec = frame(FrameId::Antares).lock_spec().unwrap();
    for _ in 0..=spec.lock_ticks {
        let to = sim.suits.flight[b.idx()].pos;
        press(&mut sim, ha, 0, to, 0, b.idx() as u16);
        still(&mut sim, b);
        sim.step();
    }
    assert!(sim.missile_lock(ha.idx()).is_some());
    let from = sim.events.next_seq();
    for k in 0..30 * 8 {
        let to = sim.suits.flight[b.idx()].pos;
        press(&mut sim, ha, if k == 0 { FIRE_SECONDARY } else { 0 }, to, 0, b.idx() as u16);
        still(&mut sim, b);
        sim.step();
    }
    let missiles = events_since(&sim, from)
        .iter()
        .filter(|e| matches!(e, Event::Hit { weapon: WeaponKind::HomingMissile, target, .. } if *target as usize == b.idx()))
        .count();
    assert!(missiles >= 1, "the salvo lands");
}

#[test]
fn a_reactor_blast_in_open_space_hurts_its_own_side_and_in_the_reach_nobody() {
    for (at, hurt) in [(OUT, true), (IN, false)] {
        let mut sim = Sim::new(SimConfig {
            target_drones: 0,
            field_rocks: 0,
            survival: true,
            law: Law::OpenSpace,
            ..SimConfig::default()
        });
        let argo = pilot(&mut sim, FrameId::Argo, Faction::Colonies, at, Vec3::Z);
        let friend = pilot(&mut sim, FrameId::Argo, Faction::Colonies, at + Vec3::Z * 20.0, -Vec3::Z);
        still(&mut sim, argo);
        still(&mut sim, friend);
        sim.step();
        let torso = sim.suits.part_hp[argo.idx()][Part::Torso as usize];
        let armour = frame(FrameId::Argo).armor;
        sim.strike(argo.idx(), Part::Torso, torso / armour + 1.0, friend.idx(), WeaponKind::BeamRifle);
        still(&mut sim, argo);
        still(&mut sim, friend);
        sim.step();
        let before = sim.suits.part_hp[friend.idx()][Part::Torso as usize];
        assert_eq!(sim.eject(argo, true), Some(Ejected::Blown));
        still(&mut sim, friend);
        sim.step();
        let lost = before - sim.suits.part_hp[friend.idx()][Part::Torso as usize];
        assert_eq!(lost > 0.0, hurt, "{at}: {lost}");
    }
}

/// A Colonies Corona at `at` raises its field; a Colonies Argo 600 m off fires its beam rifle
/// into it under `law`. How many beams the field stopped.
fn warded(law: Law, at: Vec3) -> u32 {
    let mut sim = sector(law);
    let corona = pilot(&mut sim, FrameId::Corona, Faction::Colonies, at, -Vec3::Z);
    let argo = pilot(&mut sim, FrameId::Argo, Faction::Colonies, at + Vec3::Z * 600.0, -Vec3::Z);
    let mut up = false;
    for _ in 0..30 * 6 {
        let me = sim.suits.flight[corona.idx()].pos;
        press(&mut sim, corona, MODE, me - Vec3::Z * 10.0, 0, NO_SLOT);
        sim.suits.flight[corona.idx()].vel = Vec3::ZERO;
        up |= sim.ward(corona.idx()) == Some(WARD_RADIUS);
        let pull = if up && sim.would_fire(argo.idx(), 0) { FIRE_PRIMARY } else { 0 };
        press(&mut sim, argo, pull, me, 0, NO_SLOT);
        sim.suits.flight[argo.idx()].vel = Vec3::ZERO;
        sim.step();
    }
    assert!(up, "the field held");
    sim.stats(corona.idx()).warded
}

#[test]
fn a_ward_stops_an_open_space_pilots_beam_and_never_an_allys() {
    assert!(warded(Law::OpenSpace, OUT) >= 1, "open space: the field stops it");
    assert_eq!(warded(Law::OpenSpace, IN), 0, "in the reach the beam is nobody's harm");
    assert_eq!(warded(Law::Sides, OUT), 0, "sides: an ally's beam");
}

/// Antares (Colonies) fires a salvo at an Alliance Argo with a Colonies Argo 30 m beside it, whose
/// machine cannon is on the nearest missile, under `law`: how the missiles ended.
fn point_defence(law: Law) -> Vec<BurstCause> {
    let mut sim = sector(law);
    let ha = pilot(&mut sim, FrameId::Antares, Faction::Colonies, OUT, Vec3::Z);
    let argo = pilot(&mut sim, FrameId::Argo, Faction::Alliance, OUT + Vec3::Z * 1_500.0, -Vec3::Z);
    let gunner =
        pilot(&mut sim, FrameId::Argo, Faction::Colonies, OUT + Vec3::new(30.0, 0.0, 1_500.0), -Vec3::Z);
    assert!(weapon(frame(FrameId::Argo).loadout[1].unwrap().weapon).shoots_down);
    let lock = u32::from(frame(FrameId::Antares).lock_spec().unwrap().lock_ticks);
    for _ in 0..=lock {
        let to = sim.suits.flight[argo.idx()].pos;
        press(&mut sim, ha, 0, to, 0, argo.idx() as u16);
        still(&mut sim, argo);
        still(&mut sim, gunner);
        sim.step();
    }
    assert!(sim.missile_lock(ha.idx()).is_some());
    let from = sim.events.next_seq();
    for k in 0..6 * 30 {
        let to = sim.suits.flight[argo.idx()].pos;
        press(&mut sim, ha, if k == 0 { FIRE_SECONDARY } else { 0 }, to, 0, argo.idx() as u16);
        still(&mut sim, argo);
        let me = sim.suits.flight[gunner.idx()].pos;
        sim.suits.flight[gunner.idx()].vel = Vec3::ZERO;
        let nearest = sim
            .missiles
            .alive
            .iter()
            .map(|m| sim.missiles.pos[m])
            .min_by(|a, b| a.distance_squared(me).total_cmp(&b.distance_squared(me)));
        match nearest {
            Some(p) => press(&mut sim, gunner, FIRE_SECONDARY, p, 0, NO_SLOT),
            None => still(&mut sim, gunner),
        }
        sim.step();
    }
    events_since(&sim, from)
        .iter()
        .filter_map(|e| match e {
            Event::MissileBurst { cause, .. } => Some(*cause),
            _ => None,
        })
        .collect()
}

#[test]
fn point_defence_goes_by_the_law() {
    let sides = point_defence(Law::Sides);
    assert!(!sides.is_empty() && !sides.contains(&BurstCause::ShotDown), "sides: its own side's: {sides:?}");
    let open = point_defence(Law::OpenSpace);
    assert!(open.contains(&BurstCause::ShotDown), "open space: anyone's: {open:?}");
}
