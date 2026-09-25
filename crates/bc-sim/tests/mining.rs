//! Mining: sabers chip ore off rocks and shatter them without waste, beams waste ore, a shattered
//! rock stops colliding and grows back once nobody is near, and a saber cuts limbs off hulks.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, MELEE};
use bc_proto::events::Event;
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment};
use bc_sim::chunks::Motion;
use bc_sim::field::{Rock, SUIT_CLEARANCE};
use bc_sim::math::look_rotation;
use bc_sim::rocks::max_ore_kg;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() })
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

/// A Leo `gap` metres past the clearance it keeps from rock `r`'s surface, facing it.
fn miner(sim: &mut Sim, r: &Rock, gap: f32) -> SuitId {
    let dir = Vec3::new(1.0, 0.1, 0.3).normalize();
    let surface = r.surface(r.pos - dir * (r.radius + 50.0), 0.0);
    let pos = surface - dir * (SUIT_CLEARANCE + gap);
    sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, look_rotation(dir, Vec3::Y)).unwrap()
}

fn events_since(sim: &Sim, from: u32) -> Vec<Event> {
    (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect()
}

/// Steps with `buttons` pressed on the ticks where `press(tick)` says so, aiming at `at` and
/// pressing gently toward it.
fn work(sim: &mut Sim, id: SuitId, at: Vec3, ticks: u32, press: impl Fn(u32) -> u16) {
    for _ in 0..ticks {
        let t = sim.next_tick();
        let f = &sim.suits.flight[id.idx()];
        let aim = (at - f.pos).normalize();
        sim.set_input(
            id,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim,
                thrust: [0, 0, 20],
                buttons: press(t),
                ..InputCmd::default()
            },
        );
        sim.step();
    }
}

/// Loose ore of kind `ore` in the world, kg.
fn loose_ore(sim: &Sim, ore: u8) -> u32 {
    sim.chunks
        .alive
        .iter()
        .filter(|&k| sim.chunks.desc[k].kind == ChunkKind::Ore { ore })
        .map(|k| sim.chunks.desc[k].mass_kg)
        .sum()
}

#[test]
fn sabers_chip_ore_and_shatter_rocks_without_waste() {
    let mut sim = sim();
    let (i, rock) = lone_rock(&sim, 8.0, 60.0);
    let ore = sim.rocks.ore_kg[i];
    assert_eq!(ore, max_ore_kg(&rock));
    let leo = miner(&mut sim, &rock, 0.0);
    let from = sim.events.next_seq();
    // A stroke every 1.5 s (a swing and its cooldown take 1.2 s); the blade reaches into the rock.
    work(&mut sim, leo, rock.pos, 30, |t| if t % 45 < 3 { MELEE } else { 0 });
    let chipped = loose_ore(&sim, rock.ore);
    assert!(chipped > 0, "the first stroke chipped nothing off");
    assert!(sim.rocks.hp[i] < rocks_max(&rock), "the rock took no damage");
    work(&mut sim, leo, rock.pos, 30 * 30, |t| if t % 45 < 3 { MELEE } else { 0 });
    assert!(sim.rocks.destroyed.get(i), "the rock never shattered (hp {})", sim.rocks.hp[i]);
    assert!(sim.field.is_dead(i));
    assert!(
        events_since(&sim, from)
            .iter()
            .any(|e| matches!(e, Event::RockBreak { rock, .. } if *rock as usize == i))
    );
    // Every kilogram is loose now: chipped, or scattered when it shattered (to the 10 kg the chunks
    // are counted in).
    let loose = loose_ore(&sim, rock.ore);
    let broke = events_since(&sim, from).iter().find_map(|e| match e {
        Event::RockBreak { tick, .. } => Some(*tick),
        _ => None,
    });
    println!(
        "a {:.1} m rock ({ore} kg, {:.0} hp): {chipped} kg off the first stroke, shattered at tick {broke:?}, {loose} kg loose",
        rock.radius,
        rocks_max(&rock),
    );
    assert!(ore - loose <= 90, "{ore} kg in the rock, {loose} kg loose");
    assert_eq!(sim.rocks.ore_kg[i], 0);
}

fn rocks_max(r: &Rock) -> f32 {
    bc_sim::rocks::max_hp(r)
}

#[test]
fn beams_waste_ore() {
    let mut sim = sim();
    let (i, rock) = lone_rock(&sim, 8.0, 60.0);
    let ore = sim.rocks.ore_kg[i];
    let leo = miner(&mut sim, &rock, 200.0);
    work(&mut sim, leo, rock.pos, 30 * 90, |_| FIRE_PRIMARY);
    assert!(sim.rocks.destroyed.get(i), "the rifle never broke it (hp {})", sim.rocks.hp[i]);
    let loose = loose_ore(&sim, rock.ore);
    assert!(loose + 200 < ore, "beams should boil ore off: {loose} of {ore} kg recovered");
}

#[test]
fn a_shattered_rock_lets_suits_through_and_grows_back_when_left_alone() {
    let mut sim = sim();
    let (i, rock) = lone_rock(&sim, 8.0, 60.0);
    // One stroke shatters it.
    sim.rocks.hp[i] = 1.0;
    let leo = miner(&mut sim, &rock, 0.0);
    work(&mut sim, leo, rock.pos, 40, |t| if t % 30 < 3 { MELEE } else { 0 });
    assert!(sim.rocks.destroyed.get(i));
    // The miner holds station where it is.
    sim.suits.flight[leo.idx()].vel = Vec3::ZERO;
    let t = sim.next_tick();
    sim.set_input(
        leo,
        InputCmd { tick: t, view_tick_q4: t << 4, buttons: FLIGHT_ASSIST, ..InputCmd::default() },
    );
    // Straight through where it was.
    let from = rock.pos - Vec3::X * (rock.radius + 40.0);
    let mut s = bc_sim::flight::FlightState { pos: from, vel: Vec3::X * 300.0, ..Default::default() };
    for _ in 0..30 {
        let prev = s.pos;
        s.pos += s.vel * bc_sim::DT;
        sim.field.collide(prev, &mut s);
    }
    assert!(s.pos.x > rock.pos.x + rock.radius, "stopped by a rock that isn't there: {:?}", s.pos);
    // Ten minutes on, with the miner still there, it hasn't grown back...
    for _ in 0..(10 * 60 * 30 + 60) {
        sim.step();
    }
    assert!(sim.rocks.destroyed.get(i), "grew back under the miner's nose");
    // ...until the miner leaves.
    sim.leave(leo);
    for _ in 0..(40 * 30) {
        sim.step();
    }
    assert!(!sim.rocks.destroyed.get(i), "never grew back");
    assert!(!sim.field.is_dead(i));
    assert_eq!(sim.rocks.ore_kg[i], max_ore_kg(&rock));
}

#[test]
fn a_saber_cuts_limbs_off_a_hulk() {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let leo = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 1_000.0, 0.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let t = sim.tick();
    let all: u8 = (1 << Part::COUNT) - 1;
    let desc = ChunkDesc {
        kind: ChunkKind::Hulk { frame: FrameId::Taurus, faction: Faction::Oz, parts: all },
        seed: 3,
        mass_kg: bc_sim::content::salvage::mass_without(FrameId::Taurus, 0),
    };
    // Right in front, within the blade's reach.
    let at = sim.suits.flight[leo.idx()].pos + Vec3::new(-2.0, 1.0, 12.0);
    let seg = Segment { t0: t, pos: at, ..Segment::default() }.quantized();
    let k = sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap() as usize;
    let from = sim.events.next_seq();
    work(&mut sim, leo, at, 40, |t| if t % 30 < 3 { MELEE } else { 0 });
    let cut = events_since(&sim, from).into_iter().find_map(|e| match e {
        Event::Detach { source, from_hulk: true, part, chunk, .. } if source as usize == k => {
            Some((part, chunk))
        }
        _ => None,
    });
    let (part, limb) = cut.expect("the saber cut nothing off the hulk");
    let ChunkKind::Hulk { parts, .. } = sim.chunks.desc[k].kind else { panic!("not a hulk") };
    assert_eq!(parts, all & !(1 << part as u8));
    assert_eq!(
        sim.chunks.desc[limb as usize].kind,
        ChunkKind::Limb { frame: FrameId::Taurus, faction: Faction::Oz, part }
    );
    assert_eq!(
        sim.chunks.desc[k].mass_kg + sim.chunks.desc[limb as usize].mass_kg,
        desc.mass_kg,
        "mass went missing"
    );
}
