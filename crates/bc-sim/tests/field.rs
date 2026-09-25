//! The debris field is solid: suits stop at rocks instead of passing through, even at speed, and
//! slide along them; shots stop at them.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::{Faction, FrameId, InputCmd, Part, PilotKind};
use bc_sim::field::{Rock, SUIT_CLEARANCE};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn sim(rocks: bool) -> Sim {
    let field_rocks = if rocks { SimConfig::default().field_rocks } else { 0 };
    Sim::new(SimConfig { target_dolls: 0, field_rocks, ..SimConfig::default() })
}

/// The biggest rocks, each with a clear line of approach along `dir` (no other rock on the way in
/// from `reach` metres out).
fn big_rocks(sim: &Sim, dir: Vec3, reach: f32) -> Vec<Rock> {
    let mut rocks: Vec<Rock> = sim.field.rocks().to_vec();
    rocks.sort_by(|a, b| b.radius.total_cmp(&a.radius));
    rocks
        .into_iter()
        .filter(|r| {
            let from = r.pos - dir * (r.radius + reach);
            let to = r.pos - dir * (r.radius + SUIT_CLEARANCE * 0.5);
            sim.field.sweep(from, to, SUIT_CLEARANCE).is_none_or(|(_, i)| sim.field.rocks()[i] == *r)
        })
        .take(4)
        .collect()
}

fn human(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

#[test]
fn suits_stop_at_rocks_even_at_two_km_per_second() {
    let mut sim = sim(true);
    for rock in big_rocks(&sim, Vec3::X, 400.0) {
        let start = rock.pos - Vec3::X * (rock.radius + 300.0);
        let id = human(&mut sim, FrameId::WingZero, Faction::Colonies, start, Vec3::X);
        sim.suits.flight[id.idx()].vel = Vec3::X * 2_000.0;
        let mut met = false;
        for _ in 0..30 {
            let t = sim.next_tick();
            sim.set_input(
                id,
                InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::X, ..InputCmd::default() },
            );
            sim.step();
            let f = &sim.suits.flight[id.idx()];
            assert!(!rock.touches(f.pos, SUIT_CLEARANCE - 0.1), "inside the rock at {:?}", f.pos);
            if rock.touches(f.pos, SUIT_CLEARANCE + 1.0) {
                // At the surface: no speed left into it (what's along it carries the suit off).
                met = true;
                let n = rock.normal(f.pos, SUIT_CLEARANCE);
                assert!(f.vel.dot(n) > -1.0, "still driving into the rock: {:?}", f.vel);
            }
        }
        assert!(met, "never reached the rock");
        sim.leave(id);
    }
}

#[test]
fn rocks_stop_shots() {
    // A Leo fires its rifle at a Taurus straight through a rock; with the rock there, nothing lands.
    let damage = |rocks: bool| {
        let mut sim = sim(true);
        let rock = big_rocks(&sim, Vec3::X, 400.0)[0];
        if !rocks {
            sim = sim_without_rocks();
        }
        let gap = rock.radius + 150.0;
        let leo = human(&mut sim, FrameId::Leo, Faction::Colonies, rock.pos - Vec3::X * gap, Vec3::X);
        let taurus = human(&mut sim, FrameId::Taurus, Faction::Oz, rock.pos + Vec3::X * gap, -Vec3::X);
        for _ in 0..90 {
            let (me, them) = (sim.suits.flight[leo.idx()], sim.suits.flight[taurus.idx()]);
            let muzzle = me.pos + me.rot * Vec3::new(3.4, 0.6, 3.0);
            let aim = (them.pos + Vec3::new(0.0, 2.5, 0.0) - muzzle).normalize();
            let t = sim.next_tick();
            sim.set_input(
                leo,
                InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons: FIRE_PRIMARY, ..InputCmd::default() },
            );
            sim.step();
        }
        let hp = sim.suits.part_hp[taurus.idx()];
        Part::ALL.iter().map(|p| hp[*p as usize]).sum::<f32>()
    };
    let full = damage(true);
    let open = damage(false);
    assert!(open < full, "the rifle should hit with the rock gone ({open} vs {full})");
    let pristine = {
        let spec = bc_sim::content::frame(FrameId::Taurus);
        spec.part_hp.iter().sum::<f32>()
    };
    assert_eq!(full, pristine, "a shot got through the rock");
}

fn sim_without_rocks() -> Sim {
    sim(false)
}
