//! Zodiac's aces (`content::aces`): one out among the Dolls at a time, a Leo standing half as much
//! again as a Doll's, fielded round the list every `ace_every` while none is out.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, Part, PilotKind, WeaponKind};
use bc_sim::content::aces::{ACE_ARMOUR, ACE_FRAME, ACES, ace};
use bc_sim::content::frame;
use bc_sim::{Sim, SimConfig};

fn sim(ace_every: u32) -> Sim {
    Sim::new(SimConfig { target_dolls: 4, field_rocks: 0, ace_every, ..SimConfig::default() })
}

fn run(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        sim.step();
    }
}

#[test]
fn an_ace_is_fielded_one_at_a_time_round_the_list() {
    let mut sim = sim(30);
    run(&mut sim, 29);
    assert_eq!(sim.ace_out(), None, "not due yet");
    run(&mut sim, 2);
    let (i, a) = sim.ace_out().expect("the first ace");
    assert_eq!(a, 0);
    assert_eq!(ace(a).name, ACES[0].name);
    assert_eq!(sim.ace_of(i), Some(0));
    assert_eq!(
        (sim.suits.frame[i], sim.suits.faction[i], sim.suits.pilot[i]),
        (ACE_FRAME, Faction::Oz, PilotKind::MobileDoll)
    );
    let torso = frame(ACE_FRAME).part_hp[Part::Torso as usize] * ACE_ARMOUR;
    assert_eq!(sim.suits.part_hp[i][Part::Torso as usize], torso, "it stands more than a Doll");
    // While it's out, no other.
    run(&mut sim, 90);
    assert_eq!(sim.ace_out(), Some((i, 0)));
    let aces = sim.suits.used.iter().filter(|&j| sim.ace_of(j).is_some()).count();
    assert_eq!(aces, 1);
    // Downed, it's still out (and still the ace, for whoever looks at its kill) until its slot is
    // let go, a Doll's few seconds on...
    sim.strike(i, Part::Torso, torso * 10.0, usize::MAX, WeaponKind::BeamRifle);
    run(&mut sim, 1);
    assert!(!sim.is_alive(i));
    assert_eq!(sim.ace_out(), Some((i, 0)));
    assert_eq!(sim.ace_of(i), Some(0));
    let mut waited = 0;
    while sim.ace_out() == Some((i, 0)) {
        run(&mut sim, 1);
        waited += 1;
        assert!(waited < 200, "a downed ace's slot is let go");
    }
    assert!(waited >= 60, "named a while after: {waited} ticks");
    assert_eq!(sim.ace_out(), None);
    // ...and then the next on the list comes, as it's due.
    run(&mut sim, 1);
    let (j, next) = sim.ace_out().expect("the next ace");
    assert_eq!(next, 1);
    assert_eq!(sim.ace_of(j), Some(1));
}

#[test]
fn a_slot_an_ace_had_is_nobody_s_ace_after() {
    // Every Doll fielded after an ace downed in its slot is a plain Doll (but the next ace).
    let mut sim = sim(30);
    run(&mut sim, 31);
    let (i, _) = sim.ace_out().expect("the first ace");
    sim.strike(i, Part::Torso, 1.0e9, usize::MAX, WeaponKind::BeamRifle);
    run(&mut sim, 600);
    for j in sim.suits.used.iter() {
        if let Some(a) = sim.ace_of(j) {
            assert_eq!(sim.ace_out(), Some((j, a)), "only the ace out is an ace");
        }
    }
}

#[test]
fn no_aces_unless_asked_and_none_without_dolls() {
    let mut sim = sim(0);
    run(&mut sim, 120);
    assert_eq!(sim.ace_out(), None);
    let mut sim =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ace_every: 30, ..SimConfig::default() });
    run(&mut sim, 120);
    assert_eq!(sim.ace_out(), None);
    // The default fields one every five minutes.
    assert_eq!(SimConfig::default().ace_every, bc_sim::content::aces::ACE_EVERY);
}
