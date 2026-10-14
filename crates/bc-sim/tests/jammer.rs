//! Algol's Eclipse: its enemies lose it (sensors, eyes past 150 m, drones, the Orrery
//! and locks) while its allies still see it; firing or striking shows through it for 2 s; it drains
//! energy and needs a fifth of it to engage. Hidden in a hide spot as well, eyes find it only as
//! close as the hide spot lets them.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_SECONDARY, GRIP, MELEE, MODE, ORRERY};
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind};
use bc_sim::bodies::{Bodies, Body};
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::{SpecialKind, frame};
use bc_sim::math::look_rotation;
use bc_sim::perception::Perception;
use bc_sim::sim::{HIDE_AWAKE_VISUAL_MUL, LURK_SETTLE_TICKS, cover};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_drones: 0, field_rocks: 0, ..SimConfig::default() })
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
    let SpecialKind::Eclipse { drain, min_energy, break_ticks, sig, visual } = frame(FrameId::Algol).special
    else {
        panic!("Algol jams");
    };
    (drain, min_energy, break_ticks, sig, visual)
}

#[test]
fn enemies_lose_a_jamming_algol_and_allies_see_it_shimmer() {
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    let far = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    let (_, _, _, _, visual) = jammer();
    let close = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT + Vec3::X * (visual - 30.0), -Vec3::X);
    let past_eyes = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT - Vec3::X * (visual + 50.0), Vec3::X);
    let ally = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT - Vec3::Z * 2_000.0, Vec3::Z);
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
    assert!(!sim.visible_to(past_eyes.idx(), ds.idx()), "past the jammer's range, eyes lose it");
    assert!(sim.visible_to(close.idx(), ds.idx()), "inside it, eyes still see it");
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
fn drones_and_orrery_go_blind() {
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    let drone = sim
        .spawn_at(
            FrameId::Corvus,
            Faction::Almagest,
            PilotKind::Drone,
            AT + Vec3::Z * 1_200.0,
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    // An Orrery pilot on the other side.
    let altair =
        suit(&mut sim, FrameId::Altair, Faction::Almagest, AT + Vec3::new(900.0, 0.0, 600.0), -Vec3::X);
    let mut p = Perception::default();
    for _ in 0..30 {
        hold(&mut sim, ds, 0);
        hold(&mut sim, altair, ORRERY);
        sim.step();
    }
    assert_eq!(sim.suits.ai[drone.idx()].target, ds.idx() as u16, "the drone hunts the Algol");
    let threat = |sim: &Sim| {
        sim.orrery_info(altair.idx())
            .is_some_and(|z| z.threats[..z.threat_count as usize].iter().any(|t| t.slot == ds.idx() as u16))
    };
    assert!(threat(&sim), "The Orrery rates the Algol");
    for _ in 0..10 {
        hold(&mut sim, ds, MODE);
        hold(&mut sim, altair, ORRERY);
        sim.step();
    }
    sim.perceive_into(drone.idx(), &mut p);
    assert!(p.contacts().iter().all(|c| c.slot != ds.idx() as u16), "the drone still perceives it");
    assert_ne!(sim.suits.ai[drone.idx()].target, ds.idx() as u16, "the drone still hunts it");
    assert!(!threat(&sim), "The Orrery still sees it");
}

#[test]
fn firing_or_striking_shows_through_for_a_while() {
    let (_, _, break_ticks, _, _) = jammer();
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    let enemy = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT + Vec3::Z * 2_000.0, -Vec3::Z);
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
    let spec = frame(FrameId::Algol);
    let mut sim = empty();
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    let enemy = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    for _ in 0..150 {
        hold(&mut sim, ds, MODE);
        sim.step();
    }
    let expected = spec.energy_cap - (drain - spec.energy_regen) * 5.0;
    assert!((sim.suits.energy[ds.idx()] - expected).abs() < 1.0, "{} after 5 s", sim.suits.energy[ds.idx()]);
    // Held until it's flat: it drops, and the Algol shows.
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
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    let argo = suit(&mut sim, FrameId::Argo, Faction::Almagest, AT + Vec3::Z * 2_000.0, -Vec3::Z);
    let friend = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT + Vec3::X * 500.0, Vec3::Z);
    // Each designates the other; the friend designates its own side, which doesn't count.
    let step = |sim: &mut Sim, ds_buttons: u16| {
        hold_locked(sim, ds, ds_buttons, argo.idx() as u16);
        hold_locked(sim, argo, 0, ds.idx() as u16);
        hold_locked(sim, friend, 0, ds.idx() as u16);
        sim.step();
    };
    step(&mut sim, 0);
    assert_eq!(sim.designation(argo.idx()), Some(ds.idx()));
    assert_eq!(sim.designation(friend.idx()), None);
    assert_eq!(sim.own_state(argo.idx()).lock_target, ds.idx() as u16);
    assert_ne!(sim.own_state(argo.idx()).flags & own_flags::LOCKED_ON, 0);
    assert_ne!(sim.own_state(ds.idx()).flags & own_flags::LOCKED_ON, 0);
    step(&mut sim, MODE);
    // The Argo's lock is gone, and it doesn't know the Algol has it locked.
    assert_eq!(sim.designation(argo.idx()), None);
    assert_eq!(sim.own_state(argo.idx()).lock_target, NO_SLOT);
    assert_eq!(sim.own_state(argo.idx()).flags & own_flags::LOCKED_ON, 0);
    assert_eq!(sim.designation(ds.idx()), Some(argo.idx()));
}

#[test]
fn hidden_in_a_hide_spot_and_jamming_it_is_seen_no_further_off_than_either_allows() {
    let (_, _, _, _, jam_visual) = jammer();
    let hidden_visual = LANDMARKS[1].hides[0].visual * HIDE_AWAKE_VISUAL_MUL;
    assert!(jam_visual < hidden_visual);
    let mut sim = empty();
    // An Algol on the floor of Hermit's Deep; enemy eyes between the two ranges, and far off.
    let ds = suit(&mut sim, FrameId::Algol, Faction::Colonies, AT, Vec3::Z);
    assert!(sim.place_on(ds, Body::Landmark(1), Vec3::Y));
    let i = ds.idx();
    let n =
        Bodies::at(&sim.field, sim.landmarks(), sim.tick()).pose(Body::Landmark(1)).unwrap().rot * Vec3::Y;
    let at = sim.suits.flight[i].pos;
    let between =
        suit(&mut sim, FrameId::Argo, Faction::Almagest, at + n * (jam_visual + hidden_visual) / 2.0, -n);
    let far = suit(&mut sim, FrameId::Argo, Faction::Almagest, at + n * 3_000.0, -n);
    let ally = suit(&mut sim, FrameId::Argo, Faction::Colonies, at + n * 3_000.0 + Vec3::X * 50.0, -n);
    let crouch = |sim: &mut Sim, ty: i8, buttons: u16| {
        let t = sim.next_tick();
        let aim = sim.suits.flight[i].rot * Vec3::Z;
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            thrust: [0, ty, 0],
            buttons: GRIP | buttons,
            ..InputCmd::default()
        };
        sim.set_input(ds, cmd);
        sim.step();
    };
    // Crouched, and still for 3 s: hidden, off sensors, and in sight within the hide spot's range.
    for _ in 0..20 {
        crouch(&mut sim, -127, 0);
    }
    for _ in 0..=LURK_SETTLE_TICKS {
        crouch(&mut sim, 0, 0);
    }
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
    assert!(!sim.visible_to(far.idx(), i));
    assert!(sim.visible_to(between.idx(), i), "lost inside the hide spot's eyesight");
    // Jamming as well: eyes find it only within the jammer's range, the nearer.
    crouch(&mut sim, 0, MODE);
    assert!(sim.jamming(i).is_some());
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
    assert!(!sim.visible_to(between.idx(), i), "seen past the jammer's eyesight");
    assert!(sim.visible_to(ally.idx(), i), "its ally lost it");
}
