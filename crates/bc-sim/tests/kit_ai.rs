//! The kit-aware pilot (agents and the browser autopilot) flies every Gundam's kit: each fights
//! three Taurus and a Virgo for a minute, deterministically, and shows its mechanics at work.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, PilotKind};
use bc_sim::ai::{self, AiState, DollProfile, PILOT};
use bc_sim::content::{WeaponClass, frame, weapon};
use bc_sim::math::look_rotation;
use bc_sim::perception::Perception;
use bc_sim::tuning::FlightRules;
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

/// What a minute's fight showed.
#[derive(Debug, Default)]
struct Evidence {
    hits_by_class: [u32; 5],
    specials: u32,
    missiles: usize,
    kills: u32,
    frames: Vec<FrameId>,
}

/// `f` (Colonies) starts `gap` m from three Taurus and a Virgo (Oz), and fights them for `secs`.
fn fight(f: FrameId, gap: f32, secs: u32) -> Evidence {
    fight_by(FlightRules::Real, f, gap, secs)
}

/// [`fight`], under the given flight rules.
fn fight_by(flight: FlightRules, f: FrameId, gap: f32, secs: u32) -> Evidence {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, seed: 21, flight, ..SimConfig::default() });
    let home = Vec3::new(-2_000.0, 1_200.0, 3_000.0);
    let me =
        sim.spawn_at(f, Faction::Colonies, PilotKind::Human, home, look_rotation(Vec3::X, Vec3::Y)).unwrap();
    for k in 0..4 {
        let pos = home + Vec3::new(gap, (k as f32 - 1.5) * 120.0, (k as f32 - 1.5) * 200.0);
        let frame = if k == 3 { FrameId::Virgo } else { FrameId::Taurus };
        sim.spawn_at(frame, Faction::Oz, PilotKind::MobileDoll, pos, look_rotation(-Vec3::X, Vec3::Y))
            .unwrap();
    }
    let mut ev = Evidence::default();
    let mut brain = AiState { rng: 7, anchor: home + Vec3::X * gap, ..AiState::default() };
    let mut p = Perception::default();
    let mut from = sim.events.next_seq();
    for _ in 0..secs * 30 {
        let cmd = pilot(&mut sim, me, &mut brain, &mut p);
        sim.set_input(me, cmd);
        sim.step();
        ev.missiles = ev.missiles.max(sim.missiles.count());
        let fr = sim.suits.frame[me.idx()];
        if ev.frames.last() != Some(&fr) {
            ev.frames.push(fr);
        }
        for s in from..sim.events.next_seq() {
            if let Some(Event::Kill { killer, .. }) = sim.events.get(s)
                && *killer as usize == me.idx()
            {
                ev.kills += 1;
            }
        }
        from = sim.events.next_seq();
    }
    let st = sim.stats(me.idx());
    (ev.hits_by_class, ev.specials) = (st.hits_by_class, st.specials);
    ev
}

/// One tick of the kit-aware pilot on the suit's own (server-side) perception.
fn pilot(sim: &mut Sim, me: SuitId, brain: &mut AiState, p: &mut Perception) -> bc_proto::InputCmd {
    let t = sim.next_tick();
    sim.perceive_into(me.idx(), p);
    let spec = frame(sim.suits.frame[me.idx()]);
    let range = spec.loadout[0].map_or(3_000.0, |m| weapon(m.weapon).range);
    let profile = match spec.ai.engage_range {
        r if r > 0.0 => DollProfile { preferred_range: r, ..PILOT },
        _ => PILOT,
    };
    if t >= brain.think_at {
        ai::think(p, brain, t, &profile, range);
        brain.think_at = t + 3;
    }
    let target = p.get(brain.target).copied();
    ai::drive(&p.me, target.as_ref(), brain, t, &profile, spec)
}

fn hits(ev: &Evidence, class: WeaponClass) -> u32 {
    ev.hits_by_class[class as usize]
}

#[test]
fn heavyarms_locks_on_and_opens_fire() {
    let ev = fight(FrameId::Heavyarms, 2_500.0, 60);
    println!("Heavyarms: {ev:?}");
    assert!(ev.specials >= 1, "no Full Open");
    assert!(ev.missiles >= 4, "missiles in the air: {}", ev.missiles);
    assert!(hits(&ev, WeaponClass::Missile) >= 1 && hits(&ev, WeaponClass::Beam) >= 1);
}

#[test]
fn deathscythe_closes_unseen_and_reaps() {
    let ev = fight(FrameId::Deathscythe, 2_500.0, 90);
    println!("Deathscythe: {ev:?}");
    assert!(ev.specials >= 1, "never jammed");
    assert!(hits(&ev, WeaponClass::Melee) >= 1, "the scythe never landed");
}

#[test]
fn sandrock_fires_missiles_and_cuts() {
    let ev = fight(FrameId::Sandrock, 2_500.0, 60);
    println!("Sandrock: {ev:?}");
    assert!(ev.missiles >= 2, "missiles in the air: {}", ev.missiles);
    assert!(hits(&ev, WeaponClass::Missile) + hits(&ev, WeaponClass::Melee) >= 1);
}

#[test]
fn shenlong_fights_with_fang_and_flame() {
    let ev = fight(FrameId::Shenlong, 2_500.0, 90);
    println!("Shenlong: {ev:?}");
    assert!(hits(&ev, WeaponClass::Melee) + hits(&ev, WeaponClass::Cone) >= 1);
}

#[test]
fn wing_zero_flies_out_as_neo_bird_and_fights_as_itself() {
    let ev = fight(FrameId::WingZero, 6_000.0, 60);
    println!("Wing Zero: {ev:?}");
    let changes = ev.frames.len() - 1;
    assert!(changes >= 2, "forms flown: {:?}", ev.frames);
    assert!(ev.specials >= 2);
    assert!(hits(&ev, WeaponClass::Beam) >= 1, "no hits");
}

/// Under anime rules the tank is a boost gauge that fills only once boost is let go: a pilot run
/// dry on a long pursuit lets go of boost (rather than crawl after its prey at the plain cruise on
/// an empty gauge that never fills), and boosts again once it's half full.
#[test]
fn under_anime_rules_a_dry_gauge_is_let_go_until_half_full() {
    let cfg = SimConfig { target_dolls: 0, seed: 21, flight: FlightRules::Anime, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let home = Vec3::new(-2_000.0, 1_200.0, 3_000.0);
    let me = sim
        .spawn_at(
            FrameId::Shenlong,
            Faction::Colonies,
            PilotKind::Human,
            home,
            look_rotation(Vec3::X, Vec3::Y),
        )
        .unwrap();
    let far = home + Vec3::new(5_000.0, 0.0, 0.0);
    sim.spawn_at(FrameId::Taurus, Faction::Oz, PilotKind::MobileDoll, far, look_rotation(-Vec3::X, Vec3::Y))
        .unwrap();
    sim.suits.flight[me.idx()].propellant = 0.0;
    let mut brain = AiState { rng: 7, anchor: far, ..AiState::default() };
    let mut p = Perception::default();
    let tank = frame(FrameId::Shenlong).propellant_cap;
    let (mut held_dry, mut boosted_again) = (0, false);
    for _ in 0..20 * 30 {
        let cmd = pilot(&mut sim, me, &mut brain, &mut p);
        let gauge = sim.suits.flight[me.idx()].propellant / tank;
        let boost = cmd.pressed(bc_proto::buttons::BOOST);
        if boost && gauge < 0.45 && !boosted_again {
            held_dry += 1;
        }
        boosted_again |= boost && gauge > 0.45;
        sim.set_input(me, cmd);
        sim.step();
    }
    assert_eq!(held_dry, 0, "it leaned on an empty gauge");
    assert!(boosted_again, "it never boosted again");
}

/// Every frame still shows its kit under anime rules, from further off.
#[test]
fn under_anime_rules_the_kit_pilot_minds_its_boost_gauge() {
    let shenlong = fight_by(FlightRules::Anime, FrameId::Shenlong, 6_000.0, 90);
    println!("Shenlong: {shenlong:?}");
    assert!(hits(&shenlong, WeaponClass::Melee) + hits(&shenlong, WeaponClass::Cone) >= 1);
    let deathscythe = fight_by(FlightRules::Anime, FrameId::Deathscythe, 6_000.0, 90);
    println!("Deathscythe: {deathscythe:?}");
    assert!(hits(&deathscythe, WeaponClass::Melee) >= 1, "the scythe never landed");
    let heavyarms = fight_by(FlightRules::Anime, FrameId::Heavyarms, 2_500.0, 60);
    assert!(hits(&heavyarms, WeaponClass::Missile) >= 1 && hits(&heavyarms, WeaponClass::Beam) >= 1);
}
