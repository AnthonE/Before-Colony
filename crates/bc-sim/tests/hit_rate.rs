//! How often a shot hits: every gun and beam, fired with a perfect *linear* lead (where the target
//! would be if it flew on as it is: the lock-on's ◆, `docs/LOCK.md`) at a Leo coasting across at
//! 150 m/s or jinking under flight assist, from 300 m to 3 km. What it measures is what projectile
//! speed, shot radius and spread are worth; `DESIGN.md` ("Hitting") keeps the table.
//!
//! `cargo test -p bc-sim --release --test hit_rate -- --nocapture` prints it.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind, WeaponKind};
use bc_sim::content::{WeaponClass, frame, weapon};
use bc_sim::math::{hash01, look_rotation};
use bc_sim::zero::fire_control::intercept;
use bc_sim::{Sim, SimConfig};
use glam::Vec3;

const RANGES: [f32; 6] = [300.0, 600.0, 1_000.0, 1_500.0, 2_000.0, 3_000.0];
const TRIALS: u32 = 32;
/// The target's speed across the line of fire, m/s.
const CROSSING: f32 = 150.0;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    /// Flying on, flight assist off.
    Coasting,
    /// Flight assist on, the stick thrown a new way across every 0.4 s, boosting now and then.
    Jinking,
}

/// One shot, fired with a perfect linear lead from `frame_id`'s `slot` (`charged`: its charged
/// shot, the trigger held to a full charge and let go) at a Leo `range` m off: whether it hit.
fn shot(frame_id: FrameId, slot: usize, charged: bool, range: f32, target: Target, seed: u32) -> bool {
    let cfg = SimConfig { max_suits: 8, target_dolls: 0, field_rocks: 0, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    // Each trial's clock starts somewhere of its own, so a gun's spread (drawn from the tick) is
    // sampled afresh, the geometry the same.
    for _ in 0..seed % 32 {
        sim.step();
    }
    let at = Vec3::new(0.0, 2_000.0, 0.0);
    let me = sim.spawn_at(frame_id, Faction::Colonies, PilotKind::Human, at, look_rotation(Vec3::Z, Vec3::Y));
    let it = sim.spawn_at(
        FrameId::Leo,
        Faction::Oz,
        PilotKind::Human,
        at + Vec3::Z * range,
        look_rotation(-Vec3::Z, Vec3::Y),
    );
    let (me, it) = (me.unwrap(), it.unwrap());
    let (i, j) = (me.idx(), it.idx());
    let side = if seed.is_multiple_of(2) { 1.0 } else { -1.0 };
    sim.suits.flight[j].vel = Vec3::X * CROSSING * side;
    let mount = frame(frame_id).loadout[slot].unwrap();
    let charge = weapon(mount.weapon).charged.filter(|_| charged);
    let w = charge.map_or(weapon(mount.weapon), |c| weapon(c.shot));
    let button = if slot == 0 { FIRE_PRIMARY } else { FIRE_SECONDARY };
    let mut fired = None;
    let mut hit = false;
    for tick in 0..200u32 {
        let t = sim.next_tick();
        // The target's stick: across, a new way every 12 ticks.
        let jink = if target == Target::Jinking {
            let k = tick / 12;
            let a = hash01(k, seed) * core::f32::consts::TAU;
            let boost = if hash01(k, seed ^ 0x9e37) < 0.3 { BOOST } else { 0 };
            InputCmd {
                aim: -Vec3::Z,
                thrust: [(a.cos() * 127.0) as i8, (a.sin() * 127.0) as i8, 0],
                buttons: FLIGHT_ASSIST | boost,
                ..InputCmd::default()
            }
        } else {
            InputCmd { aim: -Vec3::Z, ..InputCmd::default() }
        };
        sim.set_input(it, InputCmd { tick: t, view_tick_q4: t << 4, ..jink }.quantized());
        // The shooter leads it, perfectly but linearly, and pulls the trigger once it's settled on
        // the lead (the Twin Buster holds it till the shot leaves; a charged shot is held to a
        // full charge and let go, its tap's shot going first).
        let (s, o) = (sim.suits.flight[i], sim.suits.flight[j]);
        let muzzle = s.pos + s.rot * mount.arm.muzzle();
        let aim = intercept(muzzle, s.vel, w.speed, o.pos, o.vel, Vec3::ZERO)
            .map_or((o.pos - muzzle).normalize(), |x| x.dir);
        let pull = match charge {
            Some(c) => (30..30 + u32::from(c.full())).contains(&tick),
            None => tick >= 30 && fired.is_none(),
        };
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            buttons: FLIGHT_ASSIST | if pull { button } else { 0 },
            ..InputCmd::default()
        };
        sim.set_input(me, cmd.quantized());
        let from = sim.events.next_seq();
        sim.step();
        // Nothing else to judge: armour back as it was, so a big hit doesn't end the trial.
        sim.suits.part_hp[j] = frame(FrameId::Leo).part_hp;
        if fired.is_none() && sim.stats(i).shots > u32::from(charge.is_some()) {
            fired = Some(tick);
        }
        for e in from..sim.events.next_seq() {
            if let Some(Event::Hit { shooter, weapon, .. }) = sim.events.get(e)
                && usize::from(*shooter) == i
                && *weapon == w.kind
            {
                hit = true;
            }
        }
        if fired.is_some_and(|f| tick > f + w.ttl_ticks() + 3) {
            break;
        }
    }
    assert!(fired.is_some(), "{frame_id:?} slot {slot} never fired");
    hit
}

/// The share of [`TRIALS`] shots that hit, %.
fn rate(frame_id: FrameId, slot: usize, charged: bool, range: f32, target: Target) -> f32 {
    let hits = (0..TRIALS).filter(|&k| shot(frame_id, slot, charged, range, target, k * 7 + 1)).count();
    100.0 * hits as f32 / TRIALS as f32
}

#[test]
fn hit_rates_by_weapon_and_range() {
    // Every gun and beam that leaves a muzzle, on the frame that carries it (and charged).
    let guns: [(FrameId, usize, bool); 10] = [
        (FrameId::Leo, 0, false),
        (FrameId::Leo, 0, true),
        (FrameId::Leo, 1, false),
        (FrameId::WingZero, 0, false),
        (FrameId::Virgo, 0, false),
        (FrameId::Heavyarms, 0, false),
        (FrameId::Sandrock, 0, false),
        (FrameId::Deathscythe, 0, false),
        (FrameId::Deathscythe, 1, false),
        (FrameId::Taurus, 0, false),
    ];
    println!(
        "{:<18} {:>6} {:>5} {:>5} |  coasting (%) / jinking (%) at {RANGES:?} m",
        "weapon", "m/s", "r m", "sprd°"
    );
    for (f, slot, charged) in guns {
        let w = weapon(frame(f).loadout[slot].unwrap().weapon);
        let w = if charged { weapon(w.charged.unwrap().shot) } else { w };
        assert!(matches!(w.class, WeaponClass::Beam | WeaponClass::Ballistic));
        let coast: Vec<f32> = RANGES.iter().map(|&r| rate(f, slot, charged, r, Target::Coasting)).collect();
        let jink: Vec<f32> = RANGES.iter().map(|&r| rate(f, slot, charged, r, Target::Jinking)).collect();
        let cells: Vec<String> = coast.iter().zip(&jink).map(|(c, j)| format!("{c:>3.0}/{j:>3.0}")).collect();
        println!(
            "{:<18} {:>6.0} {:>5.2} {:>5.2} |  {}",
            format!("{:?}", w.kind),
            w.speed,
            w.radius,
            w.spread.to_degrees(),
            cells.join("  ")
        );
        // A linear lead is all a coasting target needs: a beam without spread hits it every time
        // to a kilometre.
        if w.spread == 0.0 && w.kind != WeaponKind::BusterShield {
            assert!(
                coast[..3].iter().all(|&c| c >= 90.0),
                "{:?} missed a coasting target: {coast:?}",
                w.kind
            );
        }
        // A target that keeps changing its mind gets harder to hit with range.
        assert!(jink[0] >= jink[5], "{:?}: {jink:?}", w.kind);
    }
}
