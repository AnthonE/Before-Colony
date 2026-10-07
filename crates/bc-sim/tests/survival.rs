//! Survival rules: a suit its pilot built launches from its bay as it was built (worn, missing
//! parts, weapons fitted or not, what's loaded and in the tank): it rides the bay's catapult
//! cradle in its door, turning with the colony, until its pilot lets go, and is thrown out. It
//! docks to go home with what it carries, and doesn't come back once it's destroyed. The colony
//! pays for Mobile Dolls.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, GRIP, MELEE, MODE};
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment};
use bc_sim::bodies::Body;
use bc_sim::chunks::Motion;
use bc_sim::colony::hub::{BAY_LAUNCH_SPEED, BAY_RADIUS, BAY_RIDE_LOCAL, bay_pose};
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_RADIUS, bounty, mass_without};
use bc_sim::content::{Systems, frame};
use bc_sim::ground::Footing;
use bc_sim::math::look_rotation;
use bc_sim::sim::Loadout;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::{Quat, Vec3};

fn survival() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

/// Lets suit `id` go of its bay's cradle: thrown out of the door, flying free.
fn fly_out(sim: &mut Sim, id: SuitId) {
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    hold(sim, id, FLIGHT_ASSIST, aim);
    sim.step();
    assert!(!sim.in_bay(id.idx()) && sim.footing(id.idx()) == Footing::Free);
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
        modules: Default::default(),
        kits: Default::default(),
    }
}

#[test]
fn a_suit_launches_as_it_was_built() {
    let mut sim = survival();
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
    let i = id.idx();
    let spec = frame(FrameId::Leo);
    // In its bay's cradle, standing in the door facing out of it, carried round the axis with
    // the bay at its 125 m/s, its head toward the axis.
    let Body::Bay(n) = sim.suits.anchor[i].body else { panic!("in its bay") };
    assert!(sim.in_bay(i) && sim.footing(i) == Footing::Grounded);
    let door = bay_pose(n, sim.tick(), 0.0);
    let f = sim.suits.flight[i];
    assert!((f.pos - door.to_world(BAY_RIDE_LOCAL)).length() < 0.01);
    assert!((f.rot * Vec3::Z).dot(-Vec3::X) > 0.99);
    assert!((f.vel - door.point_vel(f.pos)).length() < 0.01 && (f.vel.length() - 124.7).abs() < 1.0);
    assert!((f.rot * Vec3::Y).dot(-(door.rot * Vec3::Y)) > 0.99);
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
fn a_suit_rides_its_bay_until_its_pilot_lets_go_and_is_thrown_out_of_the_door() {
    let mut sim = survival();
    let id =
        sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Leo)).unwrap();
    let i = id.idx();
    let Body::Bay(n) = sim.suits.anchor[i].body else { panic!("in its bay") };
    // Held, it goes round with the colony in its door, whatever else it's asked: no shot, no
    // change of form, no thrust moves it, and nothing hurts it.
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    for _ in 0..90 {
        let t = sim.next_tick();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim,
                thrust: [127, 127, 127],
                buttons: GRIP | FLIGHT_ASSIST | FIRE_PRIMARY | MODE,
                ..InputCmd::default()
            },
        );
        sim.step();
        let door = bay_pose(n, sim.tick(), 0.0);
        assert!((sim.suits.flight[i].pos - door.to_world(BAY_RIDE_LOCAL)).length() < 0.01);
    }
    assert_eq!(sim.stats(i).shots, 0);
    assert!(sim.in_bay(i));
    // It's still in its bay: it can go back in.
    assert!(sim.docked(i));
    // Let go: out of the door at the door's speed and the catapult's.
    let door = bay_pose(n, sim.tick(), 0.0);
    let v_door = door.point_vel(sim.suits.flight[i].pos);
    hold(&mut sim, id, FLIGHT_ASSIST, aim);
    sim.step();
    assert!(!sim.in_bay(i) && sim.footing(i) == Footing::Free);
    let out = door.rot * -Vec3::X;
    let v = sim.suits.flight[i].vel;
    assert!((v.dot(out) - BAY_LAUNCH_SPEED).abs() < 3.0, "{v} out {}", v.dot(out));
    assert!((v - out * v.dot(out) - v_door).length() < 3.0, "{v} vs the door's {v_door}");
    // On its way out of the door, away from the colony, and no longer docked.
    assert!(sim.suits.flight[i].pos.x < door.pos.x + BAY_RIDE_LOCAL.x - 1.0 && !sim.docked(i));
    // A bay the ring doesn't have launches nothing.
    let leo = Loadout::full(FrameId::Leo);
    use bc_sim::sim::LaunchAt;
    for bad in [0, 100] {
        assert!(
            sim.launch_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, &leo, LaunchAt::Bay(bad))
                .is_none()
        );
    }
    // Flight assist brakes the spin's speed off, in time.
    for _ in 0..(30 * 20) {
        hold(&mut sim, id, FLIGHT_ASSIST, aim);
        sim.step();
    }
    assert!(sim.suits.flight[i].vel.length() < 1.0, "{}", sim.suits.flight[i].vel);
    let r = (sim.suits.flight[i].pos - bc_sim::world::COLONY_CENTER).with_x(0.0).length();
    assert!((r - BAY_RADIUS).abs() < 2_000.0, "{r}");
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
    fly_out(&mut sim, id);
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
    fly_out(&mut sim, id);
    park_in_dock(&mut sim, id);
    sim.suits.flight[id.idx()].vel = Vec3::new(40.0, 0.0, 0.0);
    assert!(sim.dock(id).is_none());
    park_in_dock(&mut sim, id);
    assert!(sim.sleep(id));
    assert!(sim.dock(id).is_none());
    assert!(sim.wake(id));
    assert!(sim.dock(id).is_some());
    // Launches come out of their bays' doors, each its own, round the ring.
    let mut bays = Vec::new();
    for _ in 0..12 {
        let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &stripped_leo()).unwrap();
        let Body::Bay(n) = sim.suits.anchor[id.idx()].body else { panic!("in its bay") };
        let door = bay_pose(n, sim.tick(), 0.0);
        assert!((sim.suits.flight[id.idx()].pos - door.to_world(BAY_RIDE_LOCAL)).length() < 0.01);
        assert!((sim.suits.flight[id.idx()].pos - DOCK_CENTER).length() > DOCK_RADIUS + 1_000.0);
        assert!(!bays.contains(&n));
        bays.push(n);
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

#[test]
fn a_suit_lands_on_the_docking_hub_walks_onto_its_deck_hatch_and_docks() {
    use bc_sim::bodies::Bodies;
    use bc_sim::content::landmarks::{DOCKING_HUB, HUB_MOUTH_X};
    let mut sim = survival();
    let id =
        sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &Loadout::full(FrameId::Leo)).unwrap();
    let i = id.idx();
    fly_out(&mut sim, id);
    let hub = Body::Landmark(DOCKING_HUB);
    let pose = |sim: &Sim| Bodies::at(&sim.field, sim.landmarks(), sim.tick()).pose(hub).unwrap();
    // At rest 20 m off the hub's mouth, 70 m off its axis, facing it, its grip armed: caught by the
    // face (which moves there at under 4 m/s), it lands.
    let p = pose(&sim);
    let f = &mut sim.suits.flight[i];
    f.pos = p.to_world(Vec3::new(HUB_MOUTH_X - 20.0, 70.0, 0.0));
    f.vel = Vec3::ZERO;
    f.ang_vel = Vec3::ZERO;
    f.rot = look_rotation(Vec3::X, Vec3::Y);
    for _ in 0..600 {
        hold(&mut sim, id, FLIGHT_ASSIST | GRIP, Vec3::X);
        sim.step();
        if sim.footing(i) == Footing::Grounded {
            break;
        }
    }
    assert_eq!((sim.footing(i), sim.suits.anchor[i].body), (Footing::Grounded, hub), "landed on the hub");
    assert!(!sim.docked(i), "not on the hatch yet");
    // It walks in toward the middle of the face, to the deck hatch, and docks there.
    let mut on = false;
    for _ in 0..30 * 30 {
        let p = pose(&sim);
        let hatch = p.to_world(Vec3::new(HUB_MOUTH_X, 0.0, 0.0));
        let aim = (hatch - sim.suits.flight[i].pos).normalize();
        let t = sim.next_tick();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim,
                thrust: [0, 0, 127],
                buttons: FLIGHT_ASSIST | GRIP,
                ..InputCmd::default()
            },
        );
        sim.step();
        if sim.on_deck_hatch(i) {
            on = true;
            break;
        }
    }
    assert!(on, "walked onto the hatch: {:?}", sim.suits.anchor[i].local);
    assert!(sim.docked(i));
    let home = sim.dock(id).expect("docked on the hatch");
    assert_eq!(home.frame, FrameId::Leo);
}
