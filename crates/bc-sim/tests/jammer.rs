//! Deathscythe's Hyper Jammer: its enemies lose it (sensors, eyes past 400 m, Mobile Dolls, ZERO
//! and locks) while its allies still see it; firing or striking shows through it for 2 s; it drains
//! energy and needs a fifth of it to engage.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_SECONDARY, MELEE, MODE, ZERO};
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind};
use bc_sim::content::{SpecialKind, frame};
use bc_sim::math::look_rotation;
use bc_sim::perception::Perception;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16) {
    hold_locked(sim, id, buttons, NO_SLOT);
}

fn hold_locked(sim: &mut Sim, id: SuitId, buttons: u16, lock_target: u16) {
    let t = sim.next_tick();
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, lock_target, ..InputCmd::default() },
    );
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);

fn jammer() -> (f32, f32, u16, f32, f32) {
    let SpecialKind::HyperJammer { drain, min_energy, break_ticks, sig, visual } =
        frame(FrameId::Deathscythe).special
    else {
        panic!("Deathscythe jams");
    };
    (drain, min_energy, break_ticks, sig, visual)
}

#[test]
fn enemies_lose_a_jamming_deathscythe_and_allies_see_it_shimmer() {
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let far = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    let close = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::X * 350.0, -Vec3::X);
    let past_eyes = suit(&mut sim, FrameId::Leo, Faction::Oz, AT - Vec3::X * 450.0, Vec3::X);
    let ally = suit(&mut sim, FrameId::Leo, Faction::Colonies, AT - Vec3::Z * 2_000.0, Vec3::Z);
    // (Past the first second, when every suit still counts as having just fired.)
    for _ in 0..40 {
        hold(&mut sim, ds, 0);
        sim.step();
    }
    for viewer in [far, close, past_eyes, ally] {
        assert!(sim.visible_to(viewer.idx(), ds.idx()));
    }
    hold(&mut sim, ds, MODE);
    sim.step();
    assert!(!sim.visible_to(far.idx(), ds.idx()), "2 km off, a jammer is gone");
    assert!(!sim.visible_to(past_eyes.idx(), ds.idx()), "450 m off, eyes lose it");
    assert!(sim.visible_to(close.idx(), ds.idx()), "350 m off, eyes still see it");
    assert!(sim.visible_to(ally.idx(), ds.idx()), "allies' sensors aren't jammed");
    assert_ne!(
        sim.entity_state(ds.idx(), ally.idx()).flags & ent_flags::SPECIAL,
        0,
        "allies see the shimmer"
    );
    assert_eq!(
        sim.entity_state(ds.idx(), close.idx()).flags & ent_flags::SPECIAL,
        0,
        "enemies just see a suit"
    );
    assert_ne!(sim.own_state(ds.idx()).flags & own_flags::SPECIAL_ACTIVE, 0);
    assert_eq!(sim.stats(ds.idx()).specials, 1);
    // Held, it stays on; let go, it's off.
    for _ in 0..30 {
        hold(&mut sim, ds, MODE);
        sim.step();
    }
    assert!(!sim.visible_to(far.idx(), ds.idx()));
    assert_eq!(sim.stats(ds.idx()).specials, 1);
    hold(&mut sim, ds, 0);
    sim.step();
    assert!(sim.visible_to(far.idx(), ds.idx()));
}

#[test]
fn dolls_and_zero_go_blind() {
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let doll = sim
        .spawn_at(
            FrameId::Taurus,
            Faction::Oz,
            PilotKind::MobileDoll,
            AT + Vec3::Z * 1_200.0,
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    // A ZERO pilot on the other side.
    let zero = suit(&mut sim, FrameId::WingZero, Faction::Oz, AT + Vec3::new(900.0, 0.0, 600.0), -Vec3::X);
    let mut p = Perception::default();
    for _ in 0..30 {
        hold(&mut sim, ds, 0);
        hold(&mut sim, zero, ZERO);
        sim.step();
    }
    assert_eq!(sim.suits.ai[doll.idx()].target, ds.idx() as u16, "the doll hunts the Deathscythe");
    let threat = |sim: &Sim| {
        sim.zero_info(zero.idx())
            .is_some_and(|z| z.threats[..z.threat_count as usize].iter().any(|t| t.slot == ds.idx() as u16))
    };
    assert!(threat(&sim), "ZERO rates the Deathscythe");
    for _ in 0..10 {
        hold(&mut sim, ds, MODE);
        hold(&mut sim, zero, ZERO);
        sim.step();
    }
    sim.perceive_into(doll.idx(), &mut p);
    assert!(p.contacts().iter().all(|c| c.slot != ds.idx() as u16), "the doll still perceives it");
    assert_ne!(sim.suits.ai[doll.idx()].target, ds.idx() as u16, "the doll still hunts it");
    assert!(!threat(&sim), "ZERO still sees it");
}

#[test]
fn firing_or_striking_shows_through_for_a_while() {
    let (_, _, break_ticks, _, _) = jammer();
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let enemy = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    for press in [FIRE_SECONDARY, MELEE] {
        for _ in 0..90 {
            hold(&mut sim, ds, MODE);
            sim.step();
        }
        assert!(!sim.visible_to(enemy.idx(), ds.idx()));
        hold(&mut sim, ds, MODE | press);
        sim.step();
        let mut seen = 0;
        while sim.visible_to(enemy.idx(), ds.idx()) {
            seen += 1;
            assert!(seen < 200, "the jammer never came back");
            hold(&mut sim, ds, MODE);
            sim.step();
        }
        assert_eq!(seen, u32::from(break_ticks), "{press:#x} broke it for {seen} ticks");
    }
}

#[test]
fn the_jammer_drains_energy_and_needs_a_fifth_of_it() {
    let (drain, min_energy, _, _, _) = jammer();
    let spec = frame(FrameId::Deathscythe);
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let enemy = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    for _ in 0..150 {
        hold(&mut sim, ds, MODE);
        sim.step();
    }
    let expected = spec.energy_cap - (drain - spec.energy_regen) * 5.0;
    assert!((sim.suits.energy[ds.idx()] - expected).abs() < 1.0, "{} after 5 s", sim.suits.energy[ds.idx()]);
    // Held until it's flat: it drops, and the Deathscythe shows.
    let mut ticks = 150;
    while sim.suits.special[ds.idx()].active {
        hold(&mut sim, ds, MODE);
        sim.step();
        ticks += 1;
        assert!(ticks < 30 * 30, "it never ran out");
    }
    let secs = ticks as f32 / 30.0;
    assert!((secs - spec.energy_cap / (drain - spec.energy_regen)).abs() < 0.5, "ran out after {secs:.1} s");
    assert!(sim.visible_to(enemy.idx(), ds.idx()));
    // Still held, it comes back once there's a fifth of the energy again.
    let mut off = 0;
    while !sim.suits.special[ds.idx()].active {
        hold(&mut sim, ds, MODE);
        sim.step();
        off += 1;
        assert!(off < 30 * 10, "it never re-engaged");
    }
    let refill = min_energy * spec.energy_cap / spec.energy_regen;
    assert!((off as f32 / 30.0 - refill).abs() < 0.2, "re-engaged after {off} ticks");
    assert_eq!(sim.stats(ds.idx()).specials, 2);
}

#[test]
fn locks_need_sensors_that_see() {
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Deathscythe, Faction::Colonies, AT, Vec3::Z);
    let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    let friend = suit(&mut sim, FrameId::Leo, Faction::Colonies, AT + Vec3::X * 500.0, Vec3::Z);
    // Each designates the other; the friend designates its own side, which doesn't count.
    let step = |sim: &mut Sim, ds_buttons: u16| {
        hold_locked(sim, ds, ds_buttons, leo.idx() as u16);
        hold_locked(sim, leo, 0, ds.idx() as u16);
        hold_locked(sim, friend, 0, ds.idx() as u16);
        sim.step();
    };
    step(&mut sim, 0);
    assert_eq!(sim.designation(leo.idx()), Some(ds.idx()));
    assert_eq!(sim.designation(friend.idx()), None);
    assert_eq!(sim.own_state(leo.idx()).lock_target, ds.idx() as u16);
    assert_ne!(sim.own_state(leo.idx()).flags & own_flags::LOCKED_ON, 0);
    assert_ne!(sim.own_state(ds.idx()).flags & own_flags::LOCKED_ON, 0);
    step(&mut sim, MODE);
    // The Leo's lock is gone, and it doesn't know the Deathscythe has it locked.
    assert_eq!(sim.designation(leo.idx()), None);
    assert_eq!(sim.own_state(leo.idx()).lock_target, NO_SLOT);
    assert_eq!(sim.own_state(leo.idx()).flags & own_flags::LOCKED_ON, 0);
    assert_eq!(sim.designation(ds.idx()), Some(leo.idx()));
}
