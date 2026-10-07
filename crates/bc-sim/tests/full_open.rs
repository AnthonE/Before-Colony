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

/// Titanfall's Core (`content::specials`): Full Open charges back by itself over its cooldown, and
/// faster from the fight, from blows dealt and from blows taken, though not from its own barrage.
#[test]
fn the_fight_charges_full_open_back() {
    use bc_proto::{Part, WeaponKind};
    use bc_sim::content::specials::{DEALT_FULL, TAKEN_FULL};
    let SpecialKind::FullOpen { ticks, lockout, cooldown } = frame(FrameId::Heavyarms).special else {
        panic!("Heavyarms' special is Full Open")
    };
    let mut sim = empty();
    let ha = suit(&mut sim, FrameId::Heavyarms, Faction::Colonies, AT, Vec3::Z);
    let leo = suit(&mut sim, FrameId::Leo, Faction::Oz, AT + Vec3::Z * 800.0, -Vec3::Z);
    for id in [ha, leo] {
        sim.suits.part_hp[id.idx()][Part::Torso as usize] = 1.0e6;
    }
    assert_eq!(sim.own_state(ha.idx()).special_charge, 255, "it launches charged");
    // Its barrage, then its lockout: the Leo takes everything, and the attack charges nothing.
    hold(&mut sim, ha, SPECIAL);
    sim.step();
    for _ in 0..ticks + lockout {
        hold(&mut sim, ha, 0);
        sim.step();
    }
    assert!(sim.stats(ha.idx()).damage_dealt > 100.0, "the barrage hit");
    let left = sim.suits.special[ha.idx()].cooldown;
    assert_eq!(left, cooldown - ticks - lockout, "only the time gone by");
    // A blow dealt: its share of the whole comes off (a Leo's armour takes all of it).
    let i = ha.idx();
    sim.strike(leo.idx(), Part::Torso, 90.0, i, WeaponKind::BeamGatling);
    hold(&mut sim, ha, 0);
    sim.step();
    let dealt = (90.0 / DEALT_FULL * f32::from(cooldown)) as u16;
    assert_eq!(sim.suits.special[i].cooldown, left - 1 - dealt);
    // A blow taken counts for more (gundanium turns most of it).
    let left = sim.suits.special[i].cooldown;
    let armour = frame(FrameId::Heavyarms).armor;
    sim.strike(i, Part::ArmL, 100.0, leo.idx(), WeaponKind::BeamRifle);
    hold(&mut sim, ha, 0);
    sim.step();
    let taken = (100.0 * armour / TAKEN_FULL * f32::from(cooldown)) as u16;
    assert_eq!(sim.suits.special[i].cooldown, left - 1 - taken);
    // Its pilot sees it charging...
    let charge = sim.own_state(i).special_charge;
    let want = (u32::from(cooldown - sim.suits.special[i].cooldown) * 255 / u32::from(cooldown)) as u8;
    assert_eq!(charge, want);
    assert!(charge > 0 && charge < 255 && sim.own_state(i).weapon_ready & 8 == 0);
    // ...and a hard fight charges it in full, long before its time.
    for _ in 0..4 {
        sim.strike(leo.idx(), Part::Torso, 120.0, i, WeaponKind::BeamGatling);
        hold(&mut sim, ha, 0);
        sim.step();
    }
    assert_eq!(sim.own_state(i).special_charge, 255);
    assert!(sim.special_ready(i));
}
