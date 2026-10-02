//! The owner's prediction flies the suit the server flies through strikes and their lunges, busy
//! arms, charged shots (the Twin Buster's, and the beam rifle's held and let go), missile salvos, Full Open and changes of form: seeded from any snapshot, it
//! replays the pilot's commands and its arms keep time with the server's, tick after tick.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, MODE, SPECIAL};
use bc_proto::snapshot::{OwnArms, own_flags};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::arms::phase_to_wire;
use bc_sim::content::{SpecialKind, frame};
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

/// Ticks flown.
const RUN: u32 = 600;
/// How far ahead of each snapshot the prediction flies: a generous lead.
const LEAD: u32 = 10;

/// The own state as the client decodes it.
fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

/// Whether `b` is held at tick `t`, in `(from, to)` windows of a `period`-tick cycle.
fn held(t: u32, period: u32, windows: &[(u32, u32)]) -> bool {
    windows.iter().any(|&(from, to)| (from..to).contains(&(t % period)))
}

/// The pilot's buttons at tick `t` for `f`: every weapon, pressed and held in overlapping rhythms
/// (a strike asked for while another's under way, fire held through a strike), the special, and
/// for Wing Zero a change of form mid-strike and mid-charge.
fn buttons(f: FrameId, t: u32) -> u16 {
    let mut b = FLIGHT_ASSIST;
    let on = |cond: bool, bit: u16| if cond { bit } else { 0 };
    // A short hold, and one long enough for a charged shot (tap fires, hold charges).
    b |= on(held(t, 97, &[(10, 40), (50, 92)]), FIRE_PRIMARY);
    b |= on(held(t, 71, &[(30, 52)]), FIRE_SECONDARY);
    b |= on(held(t, 53, &[(5, 6), (20, 31), (44, 45)]), MELEE);
    b |= on(held(t, 131, &[(60, 62)]), BOOST);
    match f {
        // A strike and a charge dropped by a change of form, then flown as the Neo-Bird a while.
        FrameId::WingZero => b |= on((250..400).contains(&t), MODE),
        // Full Open, and a press while it's cooling down.
        FrameId::Heavyarms => b |= on(t == 100 || t == 300, SPECIAL),
        // The Cross Crusher, twice (the second while it's cooling down), and one mid-strike.
        FrameId::Sandrock => b |= on(t == 80 || t == 200 || t == 400 || t == 457, SPECIAL),
        _ => {}
    }
    b
}

fn command(f: FrameId, t: u32) -> InputCmd {
    let s = t as f32;
    // The aim sweeps, so busy arms and a lunge show in how the suit turns and where it goes.
    let aim = Vec3::new((s * 0.045).sin() * 0.9, (s * 0.031).sin() * 0.4, 1.0).normalize();
    let forward = if held(t, 120, &[(0, 50), (80, 100)]) { 127 } else { 0 };
    let side = if held(t, 90, &[(20, 35)]) { -90 } else { 0 };
    InputCmd {
        tick: t,
        view_tick_q4: t << 4,
        aim,
        thrust: [side, 0, forward],
        buttons: buttons(f, t),
        ..InputCmd::default()
    }
    .quantized()
}

/// What the server did, tick by tick (index = tick).
struct Flown {
    cmds: Vec<InputCmd>,
    owns: Vec<OwnState>,
    /// The server's suit, exactly.
    poses: Vec<(Vec3, Vec3, Quat)>,
}

fn fly(f: FrameId) -> Flown {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let id = sim
        .spawn_at(
            f,
            Faction::Colonies,
            PilotKind::Human,
            Vec3::new(0.0, 4_000.0, 0.0),
            look_rotation(Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let i = id.idx();
    let mut out = Flown {
        cmds: vec![InputCmd::default()],
        owns: vec![over_the_wire(&sim.own_state(i))],
        poses: vec![],
    };
    let s = &sim.suits.flight[i];
    out.poses.push((s.pos, s.vel, s.rot));
    for t in 1..=RUN {
        let cmd = command(f, t);
        sim.set_input(id, cmd);
        sim.step();
        assert_eq!(sim.tick(), t);
        out.cmds.push(cmd);
        out.owns.push(over_the_wire(&sim.own_state(i)));
        let s = &sim.suits.flight[i];
        out.poses.push((s.pos, s.vel, s.rot));
    }
    out
}

/// Whether tick `t` held nothing a client can't foresee: no mount started or stopped waiting on
/// what isn't sent (energy, ammunition, a limb), and no overheat came or went but the lockout
/// after Full Open.
fn foreseeable(f: FrameId, owns: &[OwnState], t: usize) -> bool {
    let (a, b) = (&owns[t - 1], &owns[t]);
    let never = |o: &OwnState| o.arms.wait.map(|w| w == OwnArms::NEVER);
    let hot = |o: &OwnState| o.flags & own_flags::OVERHEAT != 0;
    let opened = |o: &OwnState| o.flags & own_flags::SPECIAL_ACTIVE != 0;
    let full_open_ended = matches!(frame(f).special, SpecialKind::FullOpen { .. }) && opened(a) && !opened(b);
    never(a) == never(b) && (hot(a) == hot(b) || full_open_ended)
}

#[derive(Default, Debug)]
struct Seen {
    checked: u32,
    skipped: u32,
    lunging: u32,
    busy: u32,
    strikes: u32,
    salvo_rounds: u32,
    changes: u32,
    worst_pos: f32,
    worst_turn: f32,
}

fn check(f: FrameId) -> Seen {
    let run = fly(f);
    let mut seen = Seen::default();
    for seed in 1..=RUN - LEAD {
        if !(seed + 1..=seed + LEAD).all(|t| foreseeable(f, &run.owns, t as usize)) {
            seen.skipped += 1;
            continue;
        }
        seen.checked += 1;
        let mut history = InputHistory::default();
        for t in seed.saturating_sub(4)..=seed + LEAD {
            history.push(run.cmds[t as usize]);
        }
        let mut p = Predictor::default();
        p.reconcile(seed, &run.owns[seed as usize], &history);
        for t in seed + 1..=seed + LEAD {
            p.advance(&run.cmds[t as usize], &history);
            let own = &run.owns[t as usize];
            let arms = &p.arms;
            let at = format!("{f:?} seeded at {seed}, tick {t}");
            assert_eq!(p.form.frame, own.frame, "{at}: form");
            assert_eq!(
                (phase_to_wire(arms.phase), arms.timer, arms.slot),
                (own.arms.phase, own.arms.timer, own.arms.slot),
                "{at}: the strike"
            );
            // For the next tick's flight.
            let busy = own.arms.phase != 0 || own.arms.fired_ago < 5;
            assert_eq!(arms.busy(t + 1), busy, "{at}: busy arms");
            let lunge = own.flags & own_flags::LUNGE != 0;
            assert_eq!(arms.lunging(frame(own.frame)), lunge, "{at}: lunge");
            let (pos, vel, rot) = run.poses[t as usize];
            let (dp, dv) = ((p.state.pos - pos).length(), (p.state.vel - vel).length());
            // (Twice the vector part of the difference: `angle_between`'s acos can't resolve this.)
            let turn = 2.0 * (p.state.rot.inverse() * rot).xyz().length();
            assert!(dp < 0.01 && dv < 0.05 && turn < 1e-3, "{at}: off by {dp} m, {dv} m/s, {turn} rad");
            seen.worst_pos = seen.worst_pos.max(dp);
            seen.worst_turn = seen.worst_turn.max(turn);
            seen.lunging += u32::from(lunge);
            seen.busy += u32::from(busy);
        }
    }
    for t in 1..=RUN as usize {
        let (a, b) = (&run.owns[t - 1], &run.owns[t]);
        seen.strikes += u32::from(a.arms.phase == 0 && b.arms.phase == 1);
        seen.salvo_rounds += u32::from(b.arms.fired_ago == 0 && b.arms.salvo != a.arms.salvo);
        seen.changes +=
            u32::from(a.flags & own_flags::TRANSFORMING == 0 && b.flags & own_flags::TRANSFORMING != 0);
    }
    seen
}

#[test]
fn the_owners_arms_keep_time_with_the_servers() {
    for f in [
        FrameId::Leo,
        FrameId::WingZero,
        FrameId::Heavyarms,
        FrameId::Deathscythe,
        FrameId::Sandrock,
        FrameId::Shenlong,
    ] {
        let seen = check(f);
        println!("{f:?}: {seen:?}");
        // Most of the run is checked, strikes and all.
        assert!(seen.checked >= (RUN - LEAD) * 3 / 4, "{f:?}: only {} of the run checked", seen.checked);
        assert!(seen.strikes >= 5 && seen.lunging > 0 && seen.busy > 0, "{f:?}: {seen:?}");
    }
}

#[test]
fn the_scripts_reach_every_arm_the_client_rolls() {
    // Salvos: Sandrock's and Heavyarms' homing missiles.
    for f in [FrameId::Sandrock, FrameId::Heavyarms] {
        let seen = check(f);
        assert!(seen.salvo_rounds >= 4, "{f:?}: {seen:?}");
    }
    // Wing Zero changes form and back.
    assert!(check(FrameId::WingZero).changes >= 2);
}
