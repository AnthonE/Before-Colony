//! The ZERO System: strain and seizure, fire-time magnetism, and the quality of its predictions.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, ZERO};
use bc_proto::events::Event;
use bc_proto::snapshot::zero_mode;
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind};
use bc_sim::content::frame;
use bc_sim::math::{Rng, look_rotation};
use bc_sim::zero::hypotheses::{self, Maneuver};
use bc_sim::zero::{N_HYP, ThreatTrack};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::{Quat, Vec3};

fn sim() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn zero_pilot(sim: &mut Sim, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(FrameId::WingZero, Faction::Colonies, PilotKind::Human, pos, look_rotation(facing, Vec3::Y))
        .unwrap()
}

fn hold(sim: &mut Sim, id: SuitId, buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, aim, buttons, ..InputCmd::default() });
}

#[test]
fn strain_builds_seizes_then_locks_out() {
    let mut sim = sim();
    let pilot = zero_pilot(&mut sim, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let from = sim.events.next_seq();
    let mut seized_at = None;
    let mut released_at = None;
    for k in 0..(50 * 30) {
        hold(&mut sim, pilot, ZERO, Vec3::Z);
        sim.step();
        let z = sim.suits.zero[pilot.idx()];
        if seized_at.is_none() && z.mode == zero_mode::SEIZED {
            seized_at = Some(k);
        }
        if seized_at.is_some() && released_at.is_none() && z.mode == zero_mode::LOCKOUT {
            released_at = Some(k);
        }
    }
    let seized = seized_at.expect("never seized");
    let released = released_at.expect("never released");
    assert!((25 * 30..32 * 30).contains(&seized), "seized after {:.1} s", seized as f32 / 30.0);
    assert_eq!(released - seized, 90, "a seizure lasts 3 s");
    let events: Vec<_> = (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s).copied()).collect();
    assert!(events.iter().any(|e| matches!(e, Event::Seizure { active: true, .. })));
    assert!(events.iter().any(|e| matches!(e, Event::Seizure { active: false, .. })));
    // Locked out for 10 s after release, then it can be engaged again.
    for _ in 0..(10 * 30 + 5) {
        hold(&mut sim, pilot, ZERO, Vec3::Z);
        sim.step();
    }
    assert_eq!(sim.suits.zero[pilot.idx()].mode, zero_mode::ACTIVE);
}

#[test]
fn magnetism_snaps_near_misses_onto_the_solution() {
    let mut sim = sim();
    let pilot = zero_pilot(&mut sim, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let target = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Oz,
            PilotKind::Human,
            Vec3::new(200.0, 1_050.0, 1_400.0),
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    sim.suits.flight[target.idx()].vel = Vec3::new(-60.0, 0.0, 0.0);
    // Engage ZERO and let it compute a solution.
    for _ in 0..20 {
        hold(&mut sim, pilot, ZERO | FLIGHT_ASSIST, Vec3::Z);
        let t = sim.next_tick();
        sim.set_input(
            target,
            InputCmd { tick: t, view_tick_q4: t << 4, aim: -Vec3::Z, ..InputCmd::default() },
        );
        sim.step();
    }
    let out = sim.suits.zero[pilot.idx()].out;
    assert!(out.has_solution && out.solution_target == target.idx() as u16, "{out:?}");
    // Hold the aim 1° off the (moving) solution while the Twin Buster Rifle charges; the shot
    // snaps onto the solution in force at the firing tick.
    let from = sim.events.next_seq();
    let mut used = Vec::new();
    for _ in 0..25 {
        let sol = sim.suits.zero[pilot.idx()].out.solution;
        used.push((sim.next_tick(), sol, sim.suits.flight[pilot.idx()].vel));
        let off = Quat::from_axis_angle(Vec3::Y, 1f32.to_radians()) * sol;
        hold(&mut sim, pilot, ZERO | FIRE_PRIMARY, off);
        let t = sim.next_tick();
        sim.set_input(
            target,
            InputCmd { tick: t, view_tick_q4: t << 4, aim: -Vec3::Z, ..InputCmd::default() },
        );
        sim.step();
    }
    let (tick, v) = (from..sim.events.next_seq())
        .filter_map(|s| sim.events.get(s).copied())
        .find_map(|e| match e {
            Event::BeamSpawn { tick, velocity, .. } => Some((tick, velocity)),
            _ => None,
        })
        .expect("no shot");
    let (_, sol, shooter_vel) = used.iter().find(|(t, _, _)| *t == tick).copied().unwrap();
    let dir = (v - shooter_vel).normalize();
    assert!(
        dir.angle_between(sol) < 0.05f32.to_radians(),
        "shot not magnetized: {:.2}°",
        dir.angle_between(sol).to_degrees()
    );
}

/// The maneuver a suit actually flew over `[t, t+15]`, classified to the nearest hypothesis.
fn realized_label(spec_frame: FrameId, rot: Quat, p0: Vec3, v0: Vec3, p1: Vec3) -> usize {
    let h = 0.5;
    let a = (p1 - p0 - v0 * h) * (2.0 / (h * h));
    let spec = frame(spec_frame);
    (0..N_HYP)
        .min_by(|&x, &y| {
            let ax = hypotheses::accel(spec, rot, Maneuver::from_index(x));
            let ay = hypotheses::accel(spec, rot, Maneuver::from_index(y));
            (a - ax).length().total_cmp(&(a - ay).length())
        })
        .unwrap()
}

fn argmax(p: &[f32; N_HYP]) -> usize {
    (0..N_HYP).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap()
}

#[test]
fn predicts_mobile_dolls_better_than_chance() {
    // A Wing Zero with ZERO engaged against four Taurus dolls that are hunting it.
    let mut sim = sim();
    let pilot = zero_pilot(&mut sim, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    for k in 0..4 {
        let pos = Vec3::new(-600.0 + k as f32 * 400.0, 1_100.0, 1_800.0);
        sim.spawn_at(
            FrameId::Taurus,
            Faction::Oz,
            PilotKind::MobileDoll,
            pos,
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    }
    struct Sample {
        t: u32,
        slot: usize,
        probs: [f32; N_HYP],
        rot: Quat,
        p0: Vec3,
        v0: Vec3,
    }
    let mut samples: Vec<Sample> = Vec::new();
    let mut positions: Vec<Vec<Vec3>> = Vec::new();
    let mut last = 0;
    for _ in 0..(60 * 30) {
        hold(&mut sim, pilot, ZERO | FLIGHT_ASSIST, Vec3::Z);
        // Keep the observer alive so the sample stays long (armour top-up is test-only).
        sim.suits.part_hp[pilot.idx()] = frame(FrameId::WingZero).part_hp;
        sim.suits.zero[pilot.idx()].strain = 0.0;
        sim.step();
        let t = sim.tick();
        positions.push((0..sim.suits.cap).map(|i| sim.suits.flight[i].pos).collect());
        if !sim.is_alive(pilot.idx()) {
            break;
        }
        let out = sim.suits.zero[pilot.idx()].out;
        if out.computed_at == t && t > 60 && t != last {
            last = t;
            for tr in &out.threats[..out.n_threats as usize] {
                let ThreatTrack { slot, probs, .. } = *tr;
                if slot == NO_SLOT || !sim.is_alive(slot as usize) {
                    continue;
                }
                let s = slot as usize;
                samples.push(Sample {
                    t,
                    slot: s,
                    probs,
                    rot: sim.suits.flight[s].rot,
                    p0: sim.suits.flight[s].pos,
                    v0: sim.suits.flight[s].vel,
                });
            }
        }
    }
    let t0 = positions.len() as u32;
    let base_tick = sim.tick() + 1 - t0;
    let (mut hit, mut n, mut coast_hits, mut brier, mut brier_uniform) = (0, 0, 0, 0.0f32, 0.0f32);
    for s in &samples {
        let later = s.t + 15;
        let Some(frame_pos) = later.checked_sub(base_tick).and_then(|k| positions.get(k as usize)) else {
            continue;
        };
        if !sim.is_used(s.slot) {
            continue;
        }
        let label = realized_label(FrameId::Taurus, s.rot, s.p0, s.v0, frame_pos[s.slot]);
        n += 1;
        if argmax(&s.probs) == label {
            hit += 1;
        }
        if label == Maneuver::Coast as usize {
            coast_hits += 1;
        }
        for k in 0..N_HYP {
            let y = if k == label { 1.0 } else { 0.0 };
            brier += (s.probs[k] - y) * (s.probs[k] - y);
            brier_uniform += (1.0 / N_HYP as f32 - y) * (1.0 / N_HYP as f32 - y);
        }
    }
    assert!(n > 200, "too few samples: {n}");
    let acc = hit as f32 / n as f32;
    let coast = coast_hits as f32 / n as f32;
    println!(
        "ZERO vs dolls: top-1 {:.1}% over {n} predictions (always-coast {:.1}%, chance 14.3%), Brier {:.3} vs uniform {:.3}",
        acc * 100.0,
        coast * 100.0,
        brier / n as f32,
        brier_uniform / n as f32
    );
    // Dolls run the same deterministic brain the System models, so ZERO should read them well.
    assert!(acc >= 0.80, "top-1 {acc:.3} (coast baseline {coast:.3})");
    assert!(brier < brier_uniform * 0.8, "no better than uniform");
}

#[test]
fn probabilities_are_calibrated_against_scripted_maneuvers() {
    // A human-flown Leo that re-picks a random maneuver every 0.5 s (75 % chance of keeping it).
    let mut sim = sim();
    let pilot = zero_pilot(&mut sim, Vec3::new(0.0, 1_000.0, 0.0), Vec3::Z);
    let target = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Oz,
            PilotKind::Human,
            Vec3::new(0.0, 1_000.0, 1_200.0),
            look_rotation(-Vec3::Z, Vec3::Y),
        )
        .unwrap();
    let mut rng = Rng::new(5);
    let mut current = Maneuver::Coast;
    // Per predicted-probability bin: (sum of predictions, hits, count).
    let mut bins = [(0.0f32, 0.0f32, 0u32); 10];
    let mut pending: Vec<(u32, [f32; N_HYP])> = Vec::new();
    let mut flown: Vec<(u32, Maneuver)> = Vec::new();
    for _ in 0..(120 * 30) {
        let t = sim.next_tick();
        if t.is_multiple_of(15) && rng.next_f32() > 0.75 {
            current = Maneuver::from_index((rng.next_u32() % N_HYP as u32) as usize);
        }
        // Keep the target near the pilot: steer back if it strays.
        let tp = sim.suits.flight[target.idx()].pos;
        let man =
            if (tp - Vec3::new(0.0, 1_000.0, 1_200.0)).length() > 900.0 { Maneuver::Coast } else { current };
        flown.push((t, man));
        let d = man.local_dir();
        let q = |v: f32| (v * 127.0) as i8;
        let aim = sim.suits.aim[target.idx()];
        sim.set_input(
            target,
            InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim,
                thrust: [q(d.x), q(d.y), q(d.z)],
                ..InputCmd::default()
            },
        );
        let look = (tp - sim.suits.flight[pilot.idx()].pos).normalize();
        hold(&mut sim, pilot, ZERO, look);
        sim.step();
        let out = sim.suits.zero[pilot.idx()].out;
        if out.computed_at == sim.tick()
            && let Some(tr) =
                out.threats[..out.n_threats as usize].iter().find(|tr| tr.slot == target.idx() as u16)
        {
            pending.push((sim.tick(), tr.probs));
        }
    }
    let label_at = |t: u32| -> Option<Maneuver> {
        // The maneuver flown for most of the next 0.5 s.
        let mut counts = [0u32; N_HYP];
        for &(ft, m) in &flown {
            if ft > t && ft <= t + 15 {
                counts[m as usize] += 1;
            }
        }
        let total: u32 = counts.iter().sum();
        (total >= 15).then(|| Maneuver::from_index((0..N_HYP).max_by_key(|&k| counts[k]).unwrap()))
    };
    let mut n = 0;
    for (t, probs) in &pending {
        let Some(label) = label_at(*t) else { continue };
        n += 1;
        for (k, &p) in probs.iter().enumerate() {
            let b = ((p * 10.0) as usize).min(9);
            bins[b].0 += p;
            bins[b].1 += if k == label as usize { 1.0 } else { 0.0 };
            bins[b].2 += 1;
        }
    }
    assert!(n > 500, "samples {n}");
    let total: u32 = bins.iter().map(|b| b.2).sum();
    let mut ece = 0.0;
    for (i, (sp, hits, c)) in bins.iter().enumerate() {
        if *c == 0 {
            continue;
        }
        let mean_p = sp / *c as f32;
        let freq = hits / *c as f32;
        ece += (*c as f32 / total as f32) * (mean_p - freq).abs();
        println!(
            "bin {:.1}-{:.1}: predicted {:.2}, observed {:.2} (n={c})",
            i as f32 / 10.0,
            (i + 1) as f32 / 10.0,
            mean_p,
            freq
        );
    }
    println!("expected calibration error: {ece:.3} over {n} predictions");
    assert!(ece < 0.10, "ECE {ece:.3}");
}
