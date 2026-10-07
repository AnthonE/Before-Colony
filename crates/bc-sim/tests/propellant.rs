//! Propellant grades (`content::propellant`): a purer grade goes further (the stat sheet's
//! specific impulse), the owner's snapshot says which so the client's stat sheet is the server's,
//! and it comes home with the suit, or sleeps with it.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::content::modules::MOUNTS;
use bc_sim::content::{Grade, ModuleKind, Modules};
use bc_sim::sim::Loadout;
use bc_sim::tuning::own_tuning;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, survival: true, ..SimConfig::default() })
}

fn launch(sim: &mut Sim, grade: Grade, modules: &[ModuleKind]) -> SuitId {
    let mut l = Loadout::full(FrameId::Leo);
    l.grade = grade;
    let mut m = Modules::NONE;
    for k in modules {
        let slot = (0..MOUNTS.len()).find(|s| MOUNTS[*s] == k.part() && m.get(*s).is_none()).unwrap();
        m.set(slot, Some(*k));
    }
    l.modules = m;
    let id = sim.launch(FrameId::Leo, Faction::Colonies, PilotKind::Human, &l).unwrap();
    sim.step();
    id
}

#[test]
fn a_purer_grade_multiplies_the_specific_impulse() {
    for (grade, isp) in [(Grade::Standard, 1.0), (Grade::Refined, 1.15), (Grade::UltraPure, 1.35)] {
        let mut s = sim();
        let i = launch(&mut s, grade, &[]).idx();
        assert_eq!(s.tuning(i).isp, isp, "{grade:?}");
        assert_eq!(s.suits.grade[i], grade);
        // The owner's snapshot carries it, and the client's stat sheet is the server's.
        let own = s.own_state(i);
        assert_eq!(own.grade, grade as u8);
        assert_eq!(own_tuning(&own), s.tuning(i));
    }
    // A thruster kit's thirstier engines, on ultra-pure.
    let mut s = sim();
    let i = launch(&mut s, Grade::UltraPure, &[ModuleKind::ThrusterKit]).idx();
    assert_eq!(s.tuning(i).isp, 0.88 * 1.35);
}

/// Under the real rules the same burn costs a purer tank less, by its specific impulse.
#[test]
fn the_same_burn_costs_less_of_a_purer_grade() {
    let used = |grade: Grade| {
        let mut s = sim();
        let id = launch(&mut s, grade, &[]);
        let i = id.idx();
        let start = s.suits.flight[i].propellant;
        for _ in 0..60 {
            let t = s.next_tick();
            let aim = s.suits.flight[i].rot * Vec3::Z;
            s.set_input(
                id,
                InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust: [0, 0, 127], ..InputCmd::default() },
            );
            s.step();
        }
        start - s.suits.flight[i].propellant
    };
    let (standard, refined, ultra) = (used(Grade::Standard), used(Grade::Refined), used(Grade::UltraPure));
    assert!(standard > 10.0, "it burned: {standard}");
    assert!((refined * 1.15 / standard - 1.0).abs() < 0.01, "{standard} {refined}");
    assert!((ultra * 1.35 / standard - 1.0).abs() < 0.01, "{standard} {ultra}");
}

/// What a suit brings home (or sleeps with) keeps its grade; a new suit in its slot flies Standard.
#[test]
fn the_grade_comes_home() {
    let mut s = sim();
    let i = launch(&mut s, Grade::Refined, &[]).idx();
    assert_eq!(s.homecoming(i).grade, Grade::Refined);
    s.suits.release(i);
    let j = launch(&mut s, Grade::Standard, &[]).idx();
    assert_eq!(s.suits.grade[j], Grade::Standard);
}
