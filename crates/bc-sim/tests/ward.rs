//! The Corona's wards (`sim::ward`): held on MODE they fly out and, once they hold the
//! field, a hostile beam crossing into it stops there for the energy it carried, whoever it was
//! aimed at inside; shots fired from inside go out, solid rounds go through, and the field falls
//! when the energy's gone. The Almagest's last two aces fly the pair, each with the other at its
//! side.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MODE};
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind, WeaponKind};
use bc_sim::content::aces::{ACES, WING_OFFSET};
use bc_sim::content::{SpecialKind, WARD_RADIUS, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_drones: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

/// `id` holds `buttons` this tick, aiming at `at`.
fn hold(sim: &mut Sim, id: SuitId, buttons: u16, at: Vec3) {
    let t = sim.next_tick();
    let aim = (at - sim.suits.flight[id.idx()].pos).normalize();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, lock_target: NO_SLOT, ..InputCmd::default() },
    );
}

fn armour(sim: &Sim, id: SuitId) -> f32 {
    sim.suits.part_hp[id.idx()].iter().sum()
}

fn ward() -> (f32, u8, f32, f32, f32) {
    let SpecialKind::Ward { radius, deploy, drain, absorb, min_energy } = frame(FrameId::Corona).special
    else {
        panic!("the Corona carries wards");
    };
    (radius, deploy, drain, absorb, min_energy)
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);

/// A Corona at `AT` with its field up (MODE held while `extra` ticks run, everyone else idle).
fn field_up(sim: &mut Sim, corona: SuitId) {
    let (_, deploy, ..) = ward();
    for _ in 0..=deploy {
        hold(sim, corona, MODE, AT + Vec3::Z);
        sim.step();
    }
    assert_eq!(sim.ward(corona.idx()), Some(WARD_RADIUS), "the field holds once they're out");
}

#[test]
fn the_wards_fly_out_hold_the_field_and_come_home() {
    let mut sim = empty();
    let corona = suit(&mut sim, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let foe = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT + Vec3::Z * 500.0, -Vec3::Z);
    let (radius, deploy, ..) = ward();
    assert_eq!(radius, WARD_RADIUS);
    hold(&mut sim, corona, MODE, AT + Vec3::Z);
    sim.step();
    assert!(sim.wards_out(corona.idx()), "MODE sends them out");
    assert_eq!(sim.ward(corona.idx()), None, "not holding the field yet");
    // Everyone sees them out.
    assert_ne!(sim.entity_state(corona.idx(), foe.idx()).flags & ent_flags::SPECIAL, 0);
    for _ in 1..deploy {
        hold(&mut sim, corona, MODE, AT + Vec3::Z);
        sim.step();
    }
    assert_eq!(sim.ward(corona.idx()), Some(radius));
    hold(&mut sim, corona, 0, AT + Vec3::Z);
    sim.step();
    assert!(!sim.wards_out(corona.idx()) && sim.ward(corona.idx()).is_none(), "let go, they come home");
    assert_eq!(sim.entity_state(corona.idx(), foe.idx()).flags & ent_flags::SPECIAL, 0);
}

#[test]
fn a_hostile_beam_stops_at_the_field_for_its_energy() {
    let mut sim = empty();
    let corona = suit(&mut sim, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let foe = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT + Vec3::Z * 400.0, -Vec3::Z);
    field_up(&mut sim, corona);
    let (_, _, drain, absorb, _) = ward();
    let (hull, energy) = (armour(&sim, corona), sim.suits.energy[corona.idx()]);
    // One tap of the Argo's beam rifle, at the Corona's middle.
    hold(&mut sim, foe, FIRE_PRIMARY, AT);
    hold(&mut sim, corona, MODE, AT + Vec3::Z);
    sim.step();
    for _ in 0..30 {
        hold(&mut sim, foe, 0, AT);
        hold(&mut sim, corona, MODE, AT + Vec3::Z);
        sim.step();
    }
    assert_eq!(armour(&sim, corona), hull, "the field took the shot");
    assert_eq!(sim.stats(corona.idx()).warded, 1);
    let rifle = weapon(WeaponKind::BeamRifle).damage;
    let regen = frame(FrameId::Corona).energy_regen;
    // What it cost: the shot's energy, on top of the field's drain (and less the regeneration).
    let spent = energy - sim.suits.energy[corona.idx()];
    let expected = absorb * rifle + 31.0 * (drain - regen) / 30.0;
    assert!((spent - expected).abs() < 1.5, "spent {spent}, expected about {expected}");
    // Without the field, the same shot lands.
    let mut bare = empty();
    let corona = suit(&mut bare, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let foe = suit(&mut bare, FrameId::Argo, Faction::Colonies, AT + Vec3::Z * 400.0, -Vec3::Z);
    let hull = armour(&bare, corona);
    hold(&mut bare, foe, FIRE_PRIMARY, AT);
    bare.step();
    for _ in 0..30 {
        hold(&mut bare, foe, 0, AT);
        bare.step();
    }
    assert!(armour(&bare, corona) < hull, "a bare Corona is hit");
}

#[test]
fn the_field_covers_a_wingman_inside_and_lets_shots_out() {
    let mut sim = empty();
    let corona = suit(&mut sim, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let wing = suit(&mut sim, FrameId::Sagittarius, Faction::Almagest, AT + Vec3::X * WING_OFFSET, Vec3::Z);
    let foe = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT + Vec3::Z * 600.0, -Vec3::Z);
    field_up(&mut sim, corona);
    let (wing_hull, foe_hull) = (armour(&sim, wing), armour(&sim, foe));
    let wing_at = sim.suits.flight[wing.idx()].pos;
    let foe_at = sim.suits.flight[foe.idx()].pos;
    // The foe's rifle at the wingman, the Corona's beam gun at the foe.
    hold(&mut sim, foe, FIRE_PRIMARY, wing_at);
    hold(&mut sim, corona, MODE | FIRE_PRIMARY, foe_at);
    sim.step();
    for _ in 0..30 {
        hold(&mut sim, foe, 0, wing_at);
        hold(&mut sim, corona, MODE, foe_at);
        sim.step();
    }
    assert_eq!(armour(&sim, wing), wing_hull, "the field stops a beam aimed at a wingman inside it");
    assert!(armour(&sim, foe) < foe_hull, "the Corona's own beam goes out through it");
}

#[test]
fn solid_rounds_go_through_the_field() {
    let mut sim = empty();
    let corona = suit(&mut sim, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let foe = suit(&mut sim, FrameId::Argo, Faction::Colonies, AT + Vec3::Z * 300.0, -Vec3::Z);
    field_up(&mut sim, corona);
    let hull = armour(&sim, corona);
    // The Argo's machine cannon (its secondary), for a second.
    for _ in 0..30 {
        hold(&mut sim, foe, FIRE_SECONDARY, AT);
        hold(&mut sim, corona, MODE, AT + Vec3::Z);
        sim.step();
    }
    for _ in 0..20 {
        hold(&mut sim, foe, 0, AT);
        hold(&mut sim, corona, MODE, AT + Vec3::Z);
        sim.step();
    }
    assert!(armour(&sim, corona) < hull, "the machine cannon's rounds aren't beams");
    assert_eq!(sim.stats(corona.idx()).warded, 0);
}

#[test]
fn the_field_falls_when_its_energy_is_gone() {
    let mut sim = empty();
    let corona = suit(&mut sim, FrameId::Corona, Faction::Almagest, AT, Vec3::Z);
    let foe = suit(&mut sim, FrameId::Altair, Faction::Colonies, AT + Vec3::Z * 800.0, -Vec3::Z);
    field_up(&mut sim, corona);
    let (_, _, _, _, min_energy) = ward();
    // Nearly spent: one heavy beam takes the rest, and falls with it; its pilot lets go, and the
    // next one gets through.
    sim.suits.energy[corona.idx()] = 30.0;
    let hull = armour(&sim, corona);
    let mut fell = false;
    for _ in 0..400 {
        hold(&mut sim, foe, FIRE_PRIMARY, AT);
        hold(&mut sim, corona, if fell { 0 } else { MODE }, AT + Vec3::Z);
        sim.step();
        fell |= !sim.wards_out(corona.idx());
    }
    assert!(fell, "the field fell");
    assert_eq!(sim.stats(corona.idx()).warded, 1, "it stopped the beam that spent it");
    assert!(armour(&sim, corona) < hull, "and then the heavy beams landed");
    // With MODE still held they go out again once there's the energy for it.
    let cap = frame(FrameId::Corona).energy_cap;
    sim.suits.energy[corona.idx()] = cap * min_energy + 1.0;
    hold(&mut sim, foe, 0, AT);
    hold(&mut sim, corona, MODE, AT + Vec3::Z);
    sim.step();
    assert!(sim.wards_out(corona.idx()));
}

#[test]
fn the_last_two_aces_fly_the_pair_each_with_the_other_at_its_side() {
    let pairs: Vec<_> = ACES.iter().filter(|a| a.frame != FrameId::Argo).collect();
    assert_eq!(pairs.len(), 2);
    for a in &pairs {
        let wing = a.wing.expect("a prototype ace flies with its twin");
        assert_ne!(wing, a.frame);
        assert!(matches!(
            (a.frame, wing),
            (FrameId::Corona, FrameId::Sagittarius) | (FrameId::Sagittarius, FrameId::Corona)
        ));
    }
    // ORION, the last: fielded with its Corona beside it, in its squad.
    let mut sim =
        Sim::new(SimConfig { target_drones: 4, field_rocks: 0, ace_every: 30, ..SimConfig::default() });
    let mut seen = 0;
    for _ in 0..(ACES.len() as u32 + 1) * 40 * 30 {
        sim.step();
        let Some((i, a)) = sim.ace_out() else { continue };
        if usize::from(a) != ACES.len() - 1 {
            // Down it at once, so the next comes.
            sim.strike(i, bc_proto::Part::Torso, 1.0e9, usize::MAX, WeaponKind::BeamRifle);
            continue;
        }
        assert_eq!(sim.suits.frame[i], FrameId::Sagittarius);
        let squad = sim.suits.ai[i].squad;
        let wing = sim
            .suits
            .used
            .iter()
            .find(|&j| j != i && sim.suits.frame[j] == FrameId::Corona && sim.suits.ai[j].squad == squad)
            .expect("its Corona flies with it");
        assert_eq!(sim.ace_of(wing), None, "the wingman is a drone, not another ace");
        let apart = sim.suits.flight[i].pos.distance(sim.suits.flight[wing].pos);
        assert!(apart < WARD_RADIUS, "fielded inside the field's reach: {apart:.0} m");
        seen += 1;
        break;
    }
    assert_eq!(seen, 1, "ORION came out");
}
