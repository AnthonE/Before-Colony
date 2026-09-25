//! Melee: every blade strikes as its table row says (reach, twin blades, the Dragon Fang's thrust,
//! the Cross Crusher as Sandrock's special), clashes need two blades that parry, and every blade
//! mines.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE, SPECIAL};
use bc_proto::events::Event;
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, WeaponKind};
use bc_sim::content::{SpecialKind, WeaponClass, frame, weapon};
use bc_sim::field::{Rock, SUIT_CLEARANCE};
use bc_sim::math::look_rotation;
use bc_sim::suits::MeleePhase;
use bc_sim::{Sim, SimConfig, SuitId};
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

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

/// What strikes with each of `f`'s blades: the button, and the weapon.
fn blades(f: FrameId) -> Vec<(u16, WeaponKind)> {
    let spec = frame(f);
    let mut out = Vec::new();
    for (slot, m) in spec.loadout.iter().enumerate() {
        if let Some(m) = m
            && weapon(m.weapon).class == WeaponClass::Melee
        {
            out.push(([FIRE_PRIMARY, FIRE_SECONDARY, MELEE][slot], m.weapon));
        }
    }
    if let SpecialKind::MeleeMove { .. } = spec.special {
        out.push((SPECIAL, spec.special_mounts[0].unwrap().weapon));
    }
    out
}

/// Hits by `kind` from `a` since event `from`.
fn hits_by(sim: &Sim, from: u32, a: SuitId, kind: WeaponKind) -> usize {
    events_since(sim, from)
        .iter()
        .filter(|e| matches!(e, Event::Hit { shooter, weapon, .. } if *shooter as usize == a.idx() && *weapon == kind))
        .count()
}

/// Hits by `kind` from `f` on a Leo `d` m ahead, facing it, in one strike (the button pressed for a
/// tick, aiming straight ahead).
fn strike_hits(f: FrameId, button: u16, kind: WeaponKind, d: f32) -> usize {
    let mut sim = empty();
    let a = suit(&mut sim, f, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    suit(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, d), -Vec3::Z);
    let from = sim.events.next_seq();
    for k in 0..40 {
        hold(&mut sim, a, if k == 0 { button } else { 0 }, Vec3::Z);
        sim.step();
    }
    hits_by(&sim, from, a, kind)
}

#[test]
fn every_blade_strikes_within_its_reach() {
    let mut n = 0;
    for f in FrameId::ALL {
        for (button, kind) in blades(f) {
            let range = weapon(kind).range;
            assert!(strike_hits(f, button, kind, range) > 0, "{f:?}'s {kind:?} misses a Leo {range} m ahead");
            // The lunge carries a blade some way, but nothing reaches 20 m past its length.
            assert_eq!(strike_hits(f, button, kind, range + 20.0), 0, "{f:?}'s {kind:?} reaches too far");
            n += 1;
        }
    }
    assert_eq!(n, 8, "the saber twice, the knife, scythe, shotels, Cross Crusher, fang and glaive");
    // Reach is the table's: Deathscythe's scythe takes a Leo that a Leo's saber can't.
    assert_eq!(strike_hits(FrameId::Leo, MELEE, WeaponKind::BeamSaber, 20.0), 0);
    assert!(strike_hits(FrameId::Deathscythe, MELEE, WeaponKind::BeamScythe, 20.0) > 0);
    // The Dragon Fang doesn't lunge: its 35 m is all the reach it has.
    assert!(strike_hits(FrameId::Shenlong, FIRE_PRIMARY, WeaponKind::DragonFang, 33.0) > 0);
    assert_eq!(strike_hits(FrameId::Shenlong, FIRE_PRIMARY, WeaponKind::DragonFang, 44.0), 0);
}

#[test]
fn twin_blades_strike_once_each() {
    // Both shotels cut a Leo straight ahead, each once.
    assert_eq!(strike_hits(FrameId::Sandrock, MELEE, WeaponKind::HeatShotel, 8.0), 2);
    // With an arm gone, one blade is left.
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Sandrock, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    suit(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, 8.0), -Vec3::Z);
    sim.suits.part_hp[a.idx()][Part::ArmL as usize] = 0.0;
    let from = sim.events.next_seq();
    for k in 0..40 {
        hold(&mut sim, a, if k == 0 { MELEE } else { 0 }, Vec3::Z);
        sim.step();
    }
    assert_eq!(hits_by(&sim, from, a, WeaponKind::HeatShotel), 1);
    assert!(sim.stats(a.idx()).hits_by_class[WeaponClass::Melee as usize] == 1);
}

#[test]
fn the_cross_crusher_is_sandrocks_special() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Sandrock, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let b = suit(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, 9.0), -Vec3::Z);
    let SpecialKind::MeleeMove { cooldown } = frame(FrameId::Sandrock).special else {
        panic!("a melee move")
    };
    assert!(sim.special_ready(a.idx()));
    assert_ne!(sim.own_state(a.idx()).weapon_ready & 8, 0, "the HUD shows the special ready");
    let from = sim.events.next_seq();
    hold(&mut sim, a, SPECIAL, Vec3::Z);
    sim.step();
    let own = sim.own_state(a.idx());
    assert_ne!(own.flags & own_flags::SPECIAL_ACTIVE, 0);
    assert_eq!(own.weapon_ready & 8, 0);
    assert_eq!(u32::from(own.special_cooldown) * 4, u32::from(cooldown));
    let seen = sim.entity_state(a.idx(), b.idx()).flags;
    assert_eq!(
        seen & (ent_flags::SABER | ent_flags::SPECIAL | ent_flags::MELEE_ALT),
        ent_flags::SABER | ent_flags::SPECIAL
    );
    // It pincers the Leo with both blades.
    for _ in 0..40 {
        hold(&mut sim, a, 0, Vec3::Z);
        sim.step();
    }
    assert_eq!(hits_by(&sim, from, a, WeaponKind::CrossCrusher), 2);
    assert_eq!(sim.stats(a.idx()).specials, 1);
    // Cooling down: SPECIAL does nothing, though the shotels still work.
    let from = sim.events.next_seq();
    hold(&mut sim, a, SPECIAL, Vec3::Z);
    sim.step();
    assert_eq!(sim.suits.melee[a.idx()].phase, MeleePhase::Idle);
    hold(&mut sim, a, MELEE, Vec3::Z);
    sim.step();
    assert_eq!(sim.suits.melee[a.idx()].weapon, WeaponKind::HeatShotel);
    for _ in 0..cooldown {
        hold(&mut sim, a, 0, Vec3::Z);
        sim.step();
    }
    assert!(sim.special_ready(a.idx()), "ready again after {cooldown} ticks");
    assert_eq!(hits_by(&sim, from, a, WeaponKind::CrossCrusher), 0);
    // It needs both arms.
    sim.suits.part_hp[a.idx()][Part::ArmL as usize] = 0.0;
    assert!(!sim.special_ready(a.idx()));
    hold(&mut sim, a, SPECIAL, Vec3::Z);
    sim.step();
    assert_eq!(sim.suits.melee[a.idx()].phase, MeleePhase::Idle);
    assert_eq!(sim.stats(a.idx()).specials, 1);
}

#[test]
fn the_dragon_fang_thrusts_where_shenlong_aims() {
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Shenlong, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    // A Leo 25° off the nose, 25 m out.
    let off = Vec3::new(25f32.to_radians().sin(), 0.0, 25f32.to_radians().cos());
    let b = suit(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, 0.0) + off * 25.0, -off);
    let hand = sim.suits.flight[a.idx()].pos + Vec3::new(3.4, 0.6, 3.0);
    let at = (sim.suits.flight[b.idx()].pos + Vec3::Y * 2.5 - hand).normalize();
    let from = sim.events.next_seq();
    // Held fire strikes again as soon as the fang is back and cooled: a strike and its cooldown
    // take 40 ticks.
    let mut out = 0;
    for _ in 0..100 {
        hold(&mut sim, a, FIRE_PRIMARY, at);
        sim.step();
        if sim.suits.melee[a.idx()].striking() {
            out += 1;
            assert!(!sim.flight_mods(a.idx()).lunge, "the fang doesn't lunge");
            let seen = sim.entity_state(a.idx(), b.idx()).flags;
            assert_eq!(
                seen & (ent_flags::SABER | ent_flags::MELEE_ALT),
                ent_flags::SABER | ent_flags::MELEE_ALT
            );
        }
    }
    assert_eq!(hits_by(&sim, from, a, WeaponKind::DragonFang), 3);
    assert_eq!(out, 3 * 10, "three strikes of 4 + 6 ticks");
    // Straight ahead it misses.
    let mut sim = empty();
    let a = suit(&mut sim, FrameId::Shenlong, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    suit(&mut sim, FrameId::Leo, Faction::Oz, Vec3::new(0.0, 1_000.0, 0.0) + off * 25.0, -off);
    let from = sim.events.next_seq();
    for _ in 0..40 {
        hold(&mut sim, a, FIRE_PRIMARY, Vec3::Z);
        sim.step();
    }
    assert_eq!(hits_by(&sim, from, a, WeaponKind::DragonFang), 0);
}

/// Two suits face to face `d` m apart strike so both blades are out at once. Returns the events
/// and each side's melee state after `ticks`.
fn duel(fa: FrameId, ba: u16, fb: FrameId, bb: u16, d: f32) -> (Sim, SuitId, SuitId, u32) {
    let mut sim = empty();
    let a = suit(&mut sim, fa, Faction::Colonies, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let b = suit(&mut sim, fb, Faction::Oz, Vec3::new(0.0, 1_000.0, d), -Vec3::Z);
    let windup = |f: FrameId, button: u16| {
        let (_, kind) = *blades(f).iter().find(|(b, _)| *b == button).unwrap();
        weapon(kind).melee.unwrap().windup
    };
    // The later windup starts first.
    let (wa, wb) = (windup(fa, ba), windup(fb, bb));
    let from = sim.events.next_seq();
    for k in 0..30u8 {
        let press = |w: u8, other: u8, button: u16| if k + w == other.max(w) { button } else { 0 };
        hold(&mut sim, a, press(wa, wb, ba), Vec3::Z);
        hold(&mut sim, b, press(wb, wa, bb), -Vec3::Z);
        sim.step();
        if events_since(&sim, from).iter().any(|e| matches!(e, Event::Clash { .. })) {
            break;
        }
    }
    (sim, a, b, from)
}

#[test]
fn blades_clash_only_with_blades_that_parry() {
    // A scythe meets a saber: both parried, each recovering for as long as its own blade takes.
    let (sim, a, b, from) = duel(FrameId::Deathscythe, MELEE, FrameId::Leo, MELEE, 11.0);
    assert!(events_since(&sim, from).iter().any(|e| matches!(e, Event::Clash { .. })), "no clash");
    assert!(
        events_since(&sim, from).iter().all(|e| !matches!(e, Event::Hit { .. })),
        "a hit through a parry"
    );
    let (ma, mb) = (sim.suits.melee[a.idx()], sim.suits.melee[b.idx()]);
    assert_eq!((ma.phase, mb.phase), (MeleePhase::Recovery, MeleePhase::Recovery));
    let scythe = weapon(WeaponKind::BeamScythe).melee.unwrap().clash_recovery;
    let saber = weapon(WeaponKind::BeamSaber).melee.unwrap().clash_recovery;
    // (The later of the two in the tick has already counted one tick of it down.)
    assert_eq!(ma.timer + u8::from(a.idx() > b.idx()), scythe);
    assert_eq!(mb.timer + u8::from(b.idx() > a.idx()), saber);

    // Twin shotels parry a saber too.
    let (sim, _, _, from) = duel(FrameId::Sandrock, MELEE, FrameId::Leo, MELEE, 11.0);
    assert!(events_since(&sim, from).iter().any(|e| matches!(e, Event::Clash { .. })));

    // Nothing parries the Dragon Fang: it goes through a saber's swing.
    let (sim, a, _, from) = duel(FrameId::Shenlong, FIRE_PRIMARY, FrameId::Leo, MELEE, 14.0);
    assert!(events_since(&sim, from).iter().all(|e| !matches!(e, Event::Clash { .. })));
    assert!(hits_by(&sim, from, a, WeaponKind::DragonFang) > 0);
}

/// The smallest rock bigger than `min` m with nothing else within `clear` m of it.
fn lone_rock(sim: &Sim, min: f32, clear: f32) -> (usize, Rock) {
    let rocks = sim.field.rocks();
    let mut best: Option<(usize, Rock)> = None;
    for (i, r) in rocks.iter().enumerate() {
        let alone = rocks
            .iter()
            .enumerate()
            .all(|(j, o)| j == i || o.pos.distance(r.pos) > r.radius + o.radius + clear);
        if r.radius > min && alone && best.is_none_or(|(_, b)| r.radius < b.radius) {
            best = Some((i, *r));
        }
    }
    best.expect("a lone rock")
}

#[test]
fn every_blade_mines() {
    for f in FrameId::ALL {
        for (button, kind) in blades(f) {
            let mut sim = Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() });
            let (i, rock) = lone_rock(&sim, 8.0, 60.0);
            let dir = Vec3::new(1.0, 0.1, 0.3).normalize();
            let surface = rock.surface(rock.pos - dir * (rock.radius + 50.0), 0.0);
            let pos = surface - dir * SUIT_CLEARANCE;
            let a = sim
                .spawn_at(f, Faction::Colonies, PilotKind::Human, pos, look_rotation(dir, Vec3::Y))
                .unwrap();
            let hp = sim.rocks.hp[i];
            for k in 0..40 {
                let aim = (rock.pos - sim.suits.flight[a.idx()].pos).normalize();
                hold(&mut sim, a, if k == 0 { button } else { 0 }, aim);
                sim.step();
            }
            let ore = sim
                .chunks
                .alive
                .iter()
                .filter(|&k| sim.chunks.desc[k].kind == ChunkKind::Ore { ore: rock.ore })
                .count();
            assert!(sim.rocks.hp[i] < hp && ore > 0, "{f:?}'s {kind:?} chipped nothing off a rock");
        }
    }
}
