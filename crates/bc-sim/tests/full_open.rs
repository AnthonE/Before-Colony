//! Heavyarms' Full Open Attack: a SPECIAL press fires everything along the aim for three seconds,
//! heat or not (both launchers' salvos, the chest gatlings, the beam gatling); then the suit is
//! locked in an overheat, and the attack is ready again only after its cooldown.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, SPECIAL};
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::content::{SpecialKind, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn empty() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn suit(sim: &mut Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16) {
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim: Vec3::Z, buttons, ..InputCmd::default() },
    );
}

const AT: Vec3 = Vec3::new(0.0, 1_000.0, 0.0);

#[test]
fn full_open_fires_everything_then_locks_the_suit_out() {
    let SpecialKind::FullOpen { ticks, lockout, cooldown } = frame(FrameId::Heavyarms).special else {
        panic!("Heavyarms' special is Full Open")
    };
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let watcher = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 800.0, -Vec3::Z);
    assert!(sim.special_ready(ha.idx()));
    assert_ne!(sim.own_state(ha.idx()).weapon_ready & 8, 0);
    hold(&mut sim, ha, SPECIAL);
    sim.step();
    assert!(sim.full_open(ha.idx()));
    assert_eq!(sim.stats(ha.idx()).specials, 1);
    assert_ne!(sim.own_state(ha.idx()).flags & own_flags::SPECIAL_ACTIVE, 0);
    assert_ne!(sim.entity_state(ha.idx(), watcher.idx()).flags & ent_flags::SPECIAL, 0, "the hatches show");
    // Three seconds of everything, with no trigger held.
    let mut peak_missiles = 0;
    for _ in 0..ticks {
        hold(&mut sim, ha, 0);
        sim.step();
        peak_missiles = peak_missiles.max(sim.missiles.count());
    }
    assert!(!sim.full_open(ha.idx()));
    let spec = frame(FrameId::Heavyarms);
    // Every launcher fires a salvo each time it's cooled, as far as its rounds go.
    let missiles: usize = spec
        .loadout
        .iter()
        .chain(spec.special_mounts.iter())
        .flatten()
        .map(|m| weapon(m.weapon))
        .filter(|w| w.missile.is_some())
        .map(|w| {
            (usize::from(ticks).div_ceil(usize::from(w.cooldown)) * usize::from(w.salvo))
                .min(usize::from(w.ammo))
        })
        .sum();
    let shots = sim.stats(ha.idx()).shots as usize;
    assert!(missiles >= 20);
    assert_eq!(peak_missiles, missiles, "missiles in the air at once");
    assert!(shots > missiles + 60, "{shots} shots: the gatlings fired too");
    // Overheated and locked out, however it cools, for the lockout.
    for k in 0..lockout {
        assert!(sim.suits.overheated[ha.idx()], "cooled {k} ticks into the lockout");
        let shots = sim.stats(ha.idx()).shots;
        hold(&mut sim, ha, FIRE_PRIMARY | SPECIAL);
        sim.step();
        assert_eq!(sim.stats(ha.idx()).shots, shots, "fired through the lockout");
    }
    assert_eq!(sim.stats(ha.idx()).specials, 1);
    // Ready again only after the cooldown (counted from the start), once it's cool.
    for _ in 0..u32::from(cooldown) - u32::from(ticks) - u32::from(lockout) - 1 {
        hold(&mut sim, ha, 0);
        sim.step();
        assert!(!sim.special_ready(ha.idx()));
    }
    hold(&mut sim, ha, 0);
    sim.step();
    assert!(sim.special_ready(ha.idx()));
    hold(&mut sim, ha, SPECIAL);
    sim.step();
    assert!(sim.full_open(ha.idx()));
}
