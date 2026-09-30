//! Survival rules: a suit its pilot built launches from the docking hub as it was built (worn,
//! missing parts, weapons fitted or not, what's loaded and in the tank), docks to go home with
//! what it carries, and doesn't come back once it's destroyed. The colony pays for Mobile Dolls.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, MELEE};
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment};
use bc_sim::chunks::Motion;
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_RADIUS, bounty, mass_without};
use bc_sim::content::{Systems, frame};
use bc_sim::math::look_rotation;
use bc_sim::sim::{LAUNCH_GATE, Loadout};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::{Quat, Vec3};

fn survival() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

/// A Leo with a worn torso, no right arm (so no beam rifle), its left arm's machine cannon fitted
/// and 50 rounds loaded, but no saber; half a tank.
fn stripped_leo() -> Loadout {
    Loadout {
        parts: [0.5, 0.8, 1.0, 0.0, 0.25, 1.0],
        mounts: 0b010,
        ammo: [0, 50, 0],
        propellant: 1_200.0,
        systems: Systems::OK,
    }
}

#[test]
fn a_suit_launches_as_it_was_built() {
    let mut sim = survival();
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
    let i = id.idx();
    let spec = frame(FrameId::Leo);
    // Out of the hub's mouth, facing out and moving out.
    let f = sim.suits.flight[i];
    assert!((f.pos.x - LAUNCH_GATE.x).abs() < 1.0 && (f.pos - LAUNCH_GATE).length() < 200.0);
    assert!((f.rot * Vec3::Z).dot(-Vec3::X) > 0.99);
    assert!(f.vel.x < -5.0);
    assert_eq!(f.propellant, 1_200.0);
    assert_eq!(sim.suits.part_hp[i][Part::Torso as usize], spec.part_hp[Part::Torso as usize] * 0.8);
    assert_eq!(sim.suits.part_hp[i][Part::ArmR as usize], 0.0);
    // Missing its right arm, it's lighter by that much, and flies so.
    assert_eq!(sim.suits.gone_mask(i), 1 << Part::ArmR as u8);
    let mods = sim.flight_mods(i);
    assert_eq!(
        mods.extra_mass_kg,
        mass_without(FrameId::Leo, 1 << Part::ArmR as u8) as i32 - mass_without(FrameId::Leo, 0) as i32
    );
    // What it reports to its pilot: the machine cannon ready, nothing else.
    let own = sim.own_state(i);
    assert_eq!(own.weapon_ready & 0b111, 0b010);
    assert_eq!(own.ammo, [0, 50]);
    // No torso, no suit.
    let mut none = stripped_leo();
    none.parts[Part::Torso as usize] = 0.0;
    assert!(sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &none).is_none());
    // Nor a Mobile Doll's frame.
    assert!(
        sim.launch(FrameId::Taurus, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Taurus))
            .is_none()
    );
}

#[test]
fn weapons_that_arent_fitted_dont_fire() {
    let mut sim = survival();
    // Everything fitted but the machine cannon and the saber.
    let loadout = Loadout { mounts: 0b001, ..Loadout::full(FrameId::Leo) };
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &loadout).unwrap();
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    for _ in 0..60 {
        hold(&mut sim, id, FIRE_SECONDARY | MELEE, aim);
        sim.step();
    }
    assert_eq!(sim.stats(id.idx()).shots, 0, "no cannon");
    assert!(sim.suits.melee[id.idx()].phase == bc_sim::suits::MeleePhase::Idle, "no saber");
    for _ in 0..30 {
        hold(&mut sim, id, FIRE_PRIMARY, aim);
        sim.step();
    }
    assert!(sim.stats(id.idx()).shots > 0, "the rifle fires");
}

/// Puts suit `id` at rest in the dock.
fn park_in_dock(sim: &mut Sim, id: SuitId) {
    let f = &mut sim.suits.flight[id.idx()];
    f.pos = DOCK_CENTER + Vec3::new(0.0, 40.0, 0.0);
    f.vel = Vec3::new(3.0, 0.0, 0.0);
}

#[test]
fn a_suit_at_rest_in_the_dock_goes_home_with_what_it_carries() {
    let mut sim = survival();
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
    let i = id.idx();
    // Out in the sector it can't dock.
    sim.suits.flight[i].pos = DOCK_CENTER + Vec3::new(-2_000.0, 0.0, 0.0);
    assert!(sim.dock(id).is_none());
    park_in_dock(&mut sim, id);
    sim.suits.cargo_kg[i] = [900, 0, 120, 30];
    sim.suits.credits[i] = 750;
    // A limb in hand.
    let desc = ChunkDesc {
        kind: ChunkKind::Limb { frame: FrameId::Taurus, faction: Faction::Oz, part: Part::Legs },
        seed: 3,
        mass_kg: 1_170,
    };
    let pos = sim.suits.flight[i].pos + Vec3::new(-3.4, 0.6, 3.0);
    let seg =
        Segment { t0: sim.tick(), pos, vel: sim.suits.flight[i].vel, rot: Quat::IDENTITY, spin: Vec3::ZERO };
    let k = sim.chunks.spawn(desc, Motion::Free(seg.quantized()), sim.tick() + 1_000, sim.tick()).unwrap();
    for _ in 0..3 {
        hold(&mut sim, id, GRAB | FLIGHT_ASSIST, Vec3::NEG_X);
        sim.step();
    }
    assert_eq!(sim.held_chunk(i), Some(k as usize));
    // Survival: the dock doesn't buy the hold.
    assert_eq!(sim.suits.cargo_kg[i], [900, 0, 120, 30]);
    assert!(sim.docked(i));
    let home = sim.dock(id).expect("docked");
    assert_eq!(home.frame, FrameId::Leo);
    assert_eq!(home.parts, stripped_leo().parts);
    assert_eq!(home.mounts, 0b010);
    assert_eq!(home.ammo, [0, 50, 0]);
    assert!(home.propellant > 1_100.0 && home.propellant <= 1_200.0);
    assert_eq!(home.cargo_kg, [900, 0, 120, 30]);
    assert_eq!(home.held, Some(desc));
    assert_eq!(home.bounty, 750);
    // Gone from the sector, with what it held.
    assert!(!sim.suits.valid(id));
    assert!(!sim.chunks.is_alive(k));
    assert!(sim.dock(id).is_none());
}

#[test]
fn too_fast_or_asleep_it_doesnt_dock() {
    let mut sim = survival();
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
    park_in_dock(&mut sim, id);
    sim.suits.flight[id.idx()].vel = Vec3::new(40.0, 0.0, 0.0);
    assert!(sim.dock(id).is_none());
    park_in_dock(&mut sim, id);
    assert!(sim.sleep(id));
    assert!(sim.dock(id).is_none());
    assert!(sim.wake(id));
    assert!(sim.dock(id).is_some());
    // Launches come out inside the dock, a suit's length from its edge at most.
    for _ in 0..12 {
        let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
        assert!((sim.suits.flight[id.idx()].pos - DOCK_CENTER).length() < DOCK_RADIUS - 10.0);
    }
}

/// Shoots suit `target` down with a Wing Zero's Twin Buster Rifle from 300 m. The shooter.
fn shoot_down(sim: &mut Sim, target: SuitId) -> SuitId {
    let at = sim.suits.flight[target.idx()].pos;
    let pos = at + Vec3::new(0.0, 0.0, -300.0);
    let shooter = sim
        .spawn_at(
            FrameId::WingZero,
            Faction::Colonies,
            PilotKind::Human,
            pos,
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    for _ in 0..200 {
        if !sim.is_alive(target.idx()) {
            break;
        }
        let aim = (sim.suits.flight[target.idx()].pos - sim.suits.flight[shooter.idx()].pos).normalize();
        hold(sim, shooter, FIRE_PRIMARY, aim);
        sim.step();
    }
    assert!(!sim.is_alive(target.idx()), "shot down");
    shooter
}

#[test]
fn the_colony_pays_for_mobile_dolls() {
    let mut sim = survival();
    let doll = sim
        .spawn_at(
            FrameId::Virgo,
            Faction::Oz,
            PilotKind::MobileDoll,
            Vec3::new(0.0, 1_500.0, 0.0),
            Quat::IDENTITY,
        )
        .unwrap();
    // A doll that doesn't fight back.
    sim.suits.part_hp[doll.idx()] = [1.0; Part::COUNT];
    let shooter = shoot_down(&mut sim, doll);
    assert_eq!(sim.suits.credits[shooter.idx()], bounty(FrameId::Virgo));
    // Under arcade rules it doesn't.
    let mut arcade = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let doll = arcade
        .spawn_at(
            FrameId::Taurus,
            Faction::Oz,
            PilotKind::MobileDoll,
            Vec3::new(0.0, 1_500.0, 0.0),
            Quat::IDENTITY,
        )
        .unwrap();
    arcade.suits.part_hp[doll.idx()] = [1.0; Part::COUNT];
    let shooter = shoot_down(&mut arcade, doll);
    assert_eq!(arcade.suits.credits[shooter.idx()], 0);
}

#[test]
fn a_pilot_shot_down_stays_down() {
    for survival in [true, false] {
        let mut sim =
            Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival, ..SimConfig::default() });
        let pilot = sim
            .spawn_at(
                FrameId::Leo,
                Faction::Oz,
                PilotKind::Human,
                Vec3::new(0.0, 1_500.0, 0.0),
                Quat::IDENTITY,
            )
            .unwrap();
        sim.suits.part_hp[pilot.idx()] = [1.0; Part::COUNT];
        shoot_down(&mut sim, pilot);
        for _ in 0..(sim.cfg.respawn_secs * 30.0) as u32 + 5 {
            sim.step();
        }
        // Arcade: a new suit at the faction's spawn. Survival: the slot is gone.
        assert_eq!(sim.suits.valid(pilot), !survival, "survival: {survival}");
        assert_eq!(sim.is_alive(pilot.idx()), !survival);
    }
}
