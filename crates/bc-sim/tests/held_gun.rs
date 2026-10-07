//! The enemy's gun (`content::salvage::held_gun`, after Daemon X Machina): a suit that grabs an arm
//! shot off a suit with a gun in its hand fires that gun from the hand that holds it, on the
//! secondary's trigger, with what was left in it; let go, its own secondary is back.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_SECONDARY, GRAB};
use bc_proto::events::Event;
use bc_proto::snapshot::OwnArms;
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment, WeaponKind};
use bc_sim::chunks::{self, Motion};
use bc_sim::content::salvage::{HELD_ROUNDS, held_gun, part_mass_kg};
use bc_sim::content::{ArmSlot, weapon};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn leo(sim: &mut Sim) -> SuitId {
    let at = Vec3::new(0.0, 1_000.0, 0.0);
    sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, at, look_rotation(Vec3::Z, Vec3::Y))
        .unwrap()
}

fn limb(frame: FrameId, part: Part) -> ChunkDesc {
    ChunkDesc {
        kind: ChunkKind::Limb { frame, faction: Faction::Oz, part },
        seed: 1,
        mass_kg: part_mass_kg(frame, part),
    }
}

/// `desc` floating at rest just by suit `id`'s left hand.
fn by_the_left_hand(sim: &mut Sim, id: SuitId, desc: ChunkDesc) -> usize {
    let f = &sim.suits.flight[id.idx()];
    let hand = f.pos + f.rot * ArmSlot::Left.muzzle();
    let t = sim.tick();
    let pos = hand - Vec3::X * (chunks::radius(&desc) + 0.5);
    let seg = Segment { t0: t, pos, ..Segment::default() }.quantized();
    sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap() as usize
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, ticks: u32) {
    for _ in 0..ticks {
        let t = sim.next_tick();
        let cmd = InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, buttons, ..InputCmd::default() };
        sim.set_input(id, cmd);
        sim.step();
    }
}

fn shots_of(sim: &Sim, from: u32, id: SuitId) -> Vec<(WeaponKind, Vec3)> {
    (from..sim.events.next_seq())
        .filter_map(|s| sim.events.get(s).copied())
        .filter_map(|e| match e {
            Event::BeamSpawn { shooter, weapon, origin, .. } if shooter as usize == id.idx() => {
                Some((weapon, origin))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn which_limbs_carry_a_gun_a_hand_can_fire() {
    let gun = |f, p| held_gun(&limb(f, p));
    assert_eq!(gun(FrameId::Taurus, Part::ArmR), Some(WeaponKind::BeamRifle));
    assert_eq!(gun(FrameId::Leo, Part::ArmL), Some(WeaponKind::MachineCannon));
    assert_eq!(gun(FrameId::Heavyarms, Part::ArmR), Some(WeaponKind::BeamGatling));
    // A blade, nothing, a gun that charges: no.
    assert_eq!(gun(FrameId::Heavyarms, Part::ArmL), None, "the army knife");
    assert_eq!(gun(FrameId::Taurus, Part::ArmL), None);
    assert_eq!(gun(FrameId::WingZero, Part::ArmR), None, "the Twin Buster Rifle");
    assert_eq!(gun(FrameId::Leo, Part::Head), None);
    let ore = ChunkDesc { kind: ChunkKind::Ore { ore: 0 }, seed: 0, mass_kg: 400 };
    assert_eq!(held_gun(&ore), None);
}

#[test]
fn a_taurus_rifle_in_hand_fires_from_the_hand() {
    let mut sim = sim();
    let me = leo(&mut sim);
    let i = me.idx();
    let k = by_the_left_hand(&mut sim, me, limb(FrameId::Taurus, Part::ArmR));
    hold(&mut sim, me, GRAB, 2);
    assert_eq!(sim.held_chunk(i), Some(k));
    assert_eq!(sim.gun_in_hand(i), Some((WeaponKind::BeamRifle, false)));
    assert_ne!(sim.own_state(i).weapon_ready & 0b010, 0, "the secondary's trigger is the rifle's");
    // RMB: the rifle in the left hand, not the Leo's machine cannon (which it holds the arm of).
    let (cannon, from) = (sim.suits.weapons[i][1].ammo, sim.events.next_seq());
    hold(&mut sim, me, GRAB | FIRE_SECONDARY, 1);
    let shots = shots_of(&sim, from, me);
    assert_eq!(shots.len(), 1, "{shots:?}");
    assert_eq!(shots[0].0, WeaponKind::BeamRifle);
    let f = &sim.suits.flight[i];
    let hand = f.pos + f.rot * ArmSlot::Left.muzzle();
    assert!((shots[0].1 - hand).length() < 1.0, "from the hand: {} vs {hand}", shots[0].1);
    assert_eq!(sim.suits.weapons[i][1].ammo, cannon, "the machine cannon fired");
    // Its own cooldown, which the pilot's client rolls on from the own state.
    let cooldown = weapon(WeaponKind::BeamRifle).cooldown;
    assert_eq!(sim.own_state(i).arms.wait[1], OwnArms::wait(cooldown));
    hold(&mut sim, me, GRAB | FIRE_SECONDARY, u32::from(cooldown) - 1);
    assert_eq!(shots_of(&sim, from, me).len(), 1, "it fired before its cooldown was out");
    hold(&mut sim, me, GRAB | FIRE_SECONDARY, 1);
    assert_eq!(shots_of(&sim, from, me).len(), 2);
    // Let go: the hand's own machine cannon again.
    hold(&mut sim, me, 0, 2);
    assert_eq!(sim.gun_in_hand(i), None);
    hold(&mut sim, me, FIRE_SECONDARY, 10);
    assert!(sim.suits.weapons[i][1].ammo < cannon, "the machine cannon is back");
}

#[test]
fn a_machine_cannon_picked_up_has_half_a_load_left() {
    let mut sim = sim();
    let me = leo(&mut sim);
    let i = me.idx();
    by_the_left_hand(&mut sim, me, limb(FrameId::Leo, Part::ArmL));
    hold(&mut sim, me, GRAB, 2);
    let load = weapon(WeaponKind::MachineCannon).ammo;
    let left = (f32::from(load) * HELD_ROUNDS) as u16;
    assert_eq!(sim.own_state(i).ammo[1], left);
    assert_eq!(sim.own_state(i).ammo[1], sim.suits.held_gun[i].ammo);
    hold(&mut sim, me, GRAB | FIRE_SECONDARY, 1);
    assert_eq!(sim.own_state(i).ammo[1], left - 1);
    // Fired dry, it waits for good, and the suit's own cannon doesn't take over while it's held.
    sim.suits.held_gun[i].ammo = 0;
    hold(&mut sim, me, GRAB | FIRE_SECONDARY, 10);
    let own = sim.own_state(i);
    assert_eq!((own.weapon_ready & 0b010, own.arms.wait[1]), (0, OwnArms::NEVER));
}
