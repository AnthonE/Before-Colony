//! Concealment. A suit parked on a body stays in sight for 8 s after its pilot leaves (60 s after
//! its last fight), then its enemies' sensors lose it and their eyes find it only close in; one
//! that never fought hasn't. Awake, a suit crouched still on a body settles in 3 s: in a hide spot
//! it's off its enemies' sensors and seen within 225 m, anywhere else it runs cold at half its
//! signature; firing or a hit shows it for 5 s, in the very tick; one that slept hidden wakes
//! hidden. Allies see it all. A shattered rock grows back past sleepers that aren't in its way, and
//! a sleeper left in a hide spot is the last cleared to make room.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

mod common;

use bc_proto::buttons::{FIRE_PRIMARY, GRIP};
use bc_proto::events::Event;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::bodies::{Bodies, Body};
use bc_sim::content::frame;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::{REGROW_CLEAR, REGROW_SLEEPER_CLEAR};
use bc_sim::field::SUIT_CLEARANCE;
use bc_sim::ground::{CROUCH_STANCE, place};
use bc_sim::math::look_rotation;
use bc_sim::sim::{
    COLD_SIG, EXPOSE_TICKS, FOUGHT_DARK_TICKS, Gone, HIDE_AWAKE_VISUAL_MUL, LURK_SETTLE_TICKS, PARKED_VISUAL,
    POWER_DOWN_TICKS, cover,
};
use bc_sim::{Sim, SimConfig, SuitId};
use common::{check_invariants, lone_rock, resting_on, standing_on};
use glam::Vec3;

fn with_rocks() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() })
}

/// No rocks, no dolls: just the landmarks.
fn landmarks_only() -> Sim {
    Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() })
}

fn leo(sim: &mut Sim, faction: Faction, pos: Vec3, facing: Vec3) -> SuitId {
    sim.spawn_at(FrameId::Leo, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
}

fn steps(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.step();
    }
}

/// `id`'s command for the next tick: `thrust`, `buttons`, aiming along `aim`.
fn drive(sim: &mut Sim, id: SuitId, thrust: [i8; 3], buttons: u16, aim: Vec3) {
    let t = sim.next_tick();
    sim.set_input(
        id,
        InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() },
    );
}

/// The ground's normal under attached suit `id`, sector frame.
fn up(sim: &Sim, id: SuitId) -> Vec3 {
    let a = sim.suits.anchor[id.idx()];
    let b = Bodies::at(&sim.field, sim.landmarks(), sim.tick());
    b.pose(a.body).unwrap().rot * place(&b.shape(a.body).unwrap(), a.local, a.stance).1
}

/// A Colonies Leo standing on `body` along `dir` crouches and then lies still, gripping, until it's
/// crouched: the tick its stillness starts from.
fn crouched_on(sim: &mut Sim, body: Body, dir: Vec3) -> (SuitId, u32) {
    let id = standing_on(sim, FrameId::Leo, Faction::Colonies, body, dir);
    crouch(sim, id)
}

/// `id` crouches where it stands: the tick it's down.
fn crouch(sim: &mut Sim, id: SuitId) -> (SuitId, u32) {
    let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
    for _ in 0..40 {
        drive(sim, id, [0, -127, 0], GRIP, aim);
        sim.step();
        check_invariants(sim);
        if sim.suits.anchor[id.idx()].stance == CROUCH_STANCE {
            return (id, sim.tick());
        }
    }
    panic!("never crouched");
}

/// Holds `id` still (crouched: the stance keeps) for `n` ticks.
fn lie_still(sim: &mut Sim, id: SuitId, n: u32) {
    for _ in 0..n {
        let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
        drive(sim, id, [0; 3], GRIP, aim);
        sim.step();
    }
}

#[test]
fn parked_suits_power_down_before_they_hide() {
    let mut sim = with_rocks();
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    let at = sim.suits.flight[i].pos;
    let far = leo(&mut sim, Faction::Alliance, at + out * 1_000.0, -out);
    let near = leo(&mut sim, Faction::Alliance, at + out * (PARKED_VISUAL - 50.0), -out);
    assert!(sim.sleep(id) && sim.is_parked(i));
    let slept = sim.tick();
    // Its reactor idles down: for 8 s it's on sensors as ever...
    while sim.tick() < slept + POWER_DOWN_TICKS {
        assert!(sim.detects(far.idx(), i), "gone dark {} ticks after sleeping", sim.tick() - slept);
        sim.step();
    }
    // ...then it's off them, and eyes find it only close in.
    assert_eq!(sim.tick(), slept + POWER_DOWN_TICKS);
    assert!(!sim.detects(far.idx(), i), "still on sensors at 1 km");
    assert!(sim.detects(near.idx(), i), "not in sight inside the parked range");
    assert_eq!((sim.n_hidden, sim.n_hidden_asleep), (1, 1));
    assert!(sim.is_still(i), "a parked suit tells its viewers nothing new");
}

#[test]
fn a_suit_that_fought_stays_visible_60s() {
    // It fired at t0, and slept 10 s later.
    let mut sim = with_rocks();
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    let at = sim.suits.flight[i].pos;
    let far = leo(&mut sim, Faction::Alliance, at + out * 1_000.0, -out);
    let side = out.cross(Vec3::Y).normalize();
    drive(&mut sim, id, [0; 3], FIRE_PRIMARY, side);
    sim.step();
    let t0 = sim.tick();
    assert_eq!(sim.suits.last_fired[i], t0, "it fired");
    for _ in 0..300 {
        drive(&mut sim, id, [0; 3], 0, side);
        sim.step();
    }
    assert_eq!(sim.suits.last_fired[i], t0, "once");
    assert!(sim.sleep(id) && sim.is_parked(i));
    while sim.tick() < t0 + FOUGHT_DARK_TICKS {
        assert!(sim.detects(far.idx(), i), "dark {} ticks after its shot", sim.tick() - t0);
        sim.step();
    }
    assert!(!sim.detects(far.idx(), i), "a minute on, it should be dark");

    // Hit rather than firing, likewise: the minute runs from the hit.
    let mut sim = with_rocks();
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    let at = sim.suits.flight[i].pos;
    let shooter = leo(&mut sim, Faction::Alliance, at + out * 300.0, -out);
    let from = sim.events.next_seq();
    let mut hit = None;
    for _ in 0..60 {
        let aim = (sim.suits.flight[i].pos - sim.suits.flight[shooter.idx()].pos).normalize();
        let fire = if hit.is_none() { FIRE_PRIMARY } else { 0 };
        drive(&mut sim, shooter, [0; 3], fire, aim);
        sim.step();
        hit = hit.or_else(|| {
            (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s)).find_map(|e| match *e {
                Event::Hit { target, tick, .. } if target as usize == i => Some(tick),
                _ => None,
            })
        });
    }
    assert!(hit.is_some(), "the shot never hit it");
    let hit = sim.suits.last_hit[i];
    let far = leo(&mut sim, Faction::Alliance, at + out * 1_000.0, -out);
    assert!(sim.sleep(id) && sim.is_parked(i));
    while sim.tick() < hit + FOUGHT_DARK_TICKS {
        assert!(sim.detects(far.idx(), i), "dark {} ticks after it was hit", sim.tick() - hit);
        sim.step();
    }
    assert!(!sim.detects(far.idx(), i));
}

#[test]
fn never_having_fired_is_not_having_fought() {
    // A suit that never fired nor was hit has 0 for both: that is "never", not "at tick 0", so it
    // goes dark on the power-down alone, well before a minute is up.
    let mut sim = with_rocks();
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    let at = sim.suits.flight[i].pos;
    let far = leo(&mut sim, Faction::Alliance, at + out * 1_000.0, -out);
    assert_eq!((sim.suits.last_fired[i], sim.suits.last_hit[i]), (0, 0));
    assert!(sim.sleep(id));
    let slept = sim.tick();
    assert!(slept + POWER_DOWN_TICKS < FOUGHT_DARK_TICKS);
    steps(&mut sim, POWER_DOWN_TICKS);
    assert!(!sim.detects(far.idx(), i), "a suit that never fought is dark after the power-down");
}

#[test]
fn allies_see_parked_and_hidden_suits() {
    let mut sim = with_rocks();
    let (_, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    steps(&mut sim, 5);
    let at = sim.suits.flight[i].pos;
    let friend = leo(&mut sim, Faction::Colonies, at + out * 2_000.0, -out);
    let enemy = leo(&mut sim, Faction::Alliance, at - out * 2_000.0, out);
    assert!(sim.sleep(id));
    steps(&mut sim, POWER_DOWN_TICKS + 1);
    assert!(!sim.detects(enemy.idx(), i));
    assert!(sim.detects(friend.idx(), i), "an ally lost a parked suit");
    assert!(sim.visible_to(friend.idx(), i));

    // Hidden in the Deep, awake.
    let mut sim = landmarks_only();
    let (id, _) = crouched_on(&mut sim, Body::Landmark(1), Vec3::Y);
    let i = id.idx();
    lie_still(&mut sim, id, LURK_SETTLE_TICKS + 1);
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
    let (at, n) = (sim.suits.flight[i].pos, up(&sim, id));
    let friend = leo(&mut sim, Faction::Colonies, at + n * 2_000.0, -n);
    let enemy = leo(&mut sim, Faction::Oz, at + n * 2_000.0 + Vec3::X * 50.0, -n);
    lie_still(&mut sim, id, 1);
    assert!(!sim.detects(enemy.idx(), i));
    assert!(sim.detects(friend.idx(), i), "an ally lost a hidden suit");
}

#[test]
fn crouched_still_in_a_hide_spot_goes_dark_in_3s_seen_only_within_225m() {
    let mut sim = landmarks_only();
    let id = standing_on(&mut sim, FrameId::Leo, Faction::Colonies, Body::Landmark(1), Vec3::Y);
    let i = id.idx();
    lie_still(&mut sim, id, 1);
    assert_eq!(sim.suits.hide_spot[i], 0, "it stands in THE DEEP");
    assert_eq!(sim.cover_code(i), cover::EXPOSED, "standing, it's in the open");
    let (at, n) = (sim.suits.flight[i].pos, up(&sim, id));
    let visual = LANDMARKS[1].hides[0].visual * HIDE_AWAKE_VISUAL_MUL;
    assert_eq!(visual, 225.0);
    let [far, inside, outside] =
        [2_000.0, visual - 25.0, visual + 25.0].map(|d| leo(&mut sim, Faction::Oz, at + n * d, -n));
    let (_, down) = crouch(&mut sim, id);
    assert_eq!(sim.cover_code(i), cover::SETTLING, "crouched still, it's settling");
    assert_eq!(sim.suits.still_since[i], down);
    while sim.tick() < down + LURK_SETTLE_TICKS {
        assert!(sim.detects(far.idx(), i), "hidden {} ticks after it lay still", sim.tick() - down);
        lie_still(&mut sim, id, 1);
    }
    assert_eq!(sim.cover_code(i), cover::HIDDEN, "3 s still, it's hidden");
    assert!(!sim.detects(far.idx(), i), "on sensors at 2 km");
    assert!(!sim.detects(outside.idx(), i), "seen at {} m", visual + 25.0);
    assert!(sim.detects(inside.idx(), i), "not seen at {} m", visual - 25.0);
    lie_still(&mut sim, id, 1);
    assert_eq!((sim.n_grounded, sim.n_hidden, sim.n_hidden_asleep), (1, 1, 0), "awake, it's no sleeper");
    assert!(sim.is_still(i));
    // Moving breaks it.
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    drive(&mut sim, id, [0, 0, 127], GRIP, aim);
    sim.step();
    assert_eq!(sim.cover_code(i), cover::EXPOSED);
    assert!(sim.detects(far.idx(), i));
}

#[test]
fn firing_exposes_for_5s_in_the_same_tick() {
    let mut sim = landmarks_only();
    let (id, _) = crouched_on(&mut sim, Body::Landmark(1), Vec3::Y);
    let i = id.idx();
    lie_still(&mut sim, id, LURK_SETTLE_TICKS);
    let (at, n) = (sim.suits.flight[i].pos, up(&sim, id));
    let far = leo(&mut sim, Faction::Oz, at + n * 2_000.0, -n);
    lie_still(&mut sim, id, 1);
    assert!(!sim.detects(far.idx(), i));
    let since = sim.suits.still_since[i];
    // One shot, up and away: in the snapshot of the very tick it fires, it's seen.
    let away = (n + Vec3::new(n.y, -n.x, 0.0).normalize()).normalize();
    drive(&mut sim, id, [0; 3], GRIP | FIRE_PRIMARY, away);
    sim.step();
    let fired = sim.tick();
    assert_eq!(sim.suits.last_fired[i], fired);
    assert!(sim.detects(far.idx(), i), "the shot didn't show it");
    assert_eq!(sim.cover_code(i), cover::SETTLING, "shown, it's settling again");
    assert_eq!(sim.suits.still_since[i], since, "a shot doesn't move it");
    lie_still(&mut sim, id, 1);
    assert!(!sim.is_still(i), "just after a shot, it's news");
    while sim.tick() < fired + EXPOSE_TICKS {
        assert!(sim.detects(far.idx(), i), "dark again {} ticks after the shot", sim.tick() - fired);
        lie_still(&mut sim, id, 1);
    }
    assert!(!sim.detects(far.idx(), i), "5 s on, it should be hidden again");
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
}

#[test]
fn cold_crouched_still_anywhere_halves_signature() {
    let mut sim = landmarks_only();
    // On Hermit's flank, nowhere near a hide spot.
    let (id, down) = crouched_on(&mut sim, Body::Landmark(1), Vec3::new(0.3, 1.0, 0.2));
    let i = id.idx();
    assert_eq!(sim.suits.hide_spot[i], bc_sim::suits::NO_SPOT);
    let (at, n) = (sim.suits.flight[i].pos, up(&sim, id));
    // A Leo's sensors reach 6 km for a Leo; cold, half that.
    let range = frame(FrameId::Leo).sensor_range * frame(FrameId::Leo).signature;
    let [beyond, within] = [range * COLD_SIG + 500.0, range * COLD_SIG - 500.0]
        .map(|d| leo(&mut sim, Faction::Oz, at + n * d, -n));
    let settling = down + LURK_SETTLE_TICKS - sim.tick() - 1;
    lie_still(&mut sim, id, settling);
    assert!(sim.detects(beyond.idx(), i), "cold before it settled");
    lie_still(&mut sim, id, 1);
    assert_eq!(sim.cover_code(i), cover::COLD);
    assert_eq!(sim.concealment(i).sig, COLD_SIG);
    assert!(!sim.detects(beyond.idx(), i), "seen past half its sensor range");
    assert!(sim.detects(within.idx(), i), "lost inside half its sensor range");
    assert_eq!(sim.n_hidden, 0, "cold is not hidden");
}

#[test]
fn regrowth_ignores_distant_sleepers_but_not_one_in_the_way() {
    // A rock shattered, and one suit left `off` m from where it was: asleep (or awake). Whether
    // it grew back 11 minutes on.
    let regrown = |off: f32, asleep: bool| {
        let mut sim = with_rocks();
        let (i, rock) = lone_rock(&sim, 8.0, 60.0);
        sim.rocks.hp[i] = 1.0;
        let shooter = leo(&mut sim, Faction::Colonies, rock.pos + Vec3::Z * 300.0, -Vec3::Z);
        for _ in 0..30 {
            drive(&mut sim, shooter, [0; 3], FIRE_PRIMARY, -Vec3::Z);
            sim.step();
        }
        assert!(sim.rocks.destroyed.get(i), "the rock didn't shatter");
        sim.leave(shooter);
        let at = rock.pos + Vec3::new(0.3, 1.0, -0.2).normalize() * off;
        let id = leo(&mut sim, Faction::Colonies, at, Vec3::Z);
        if asleep {
            assert!(sim.sleep(id));
        }
        steps(&mut sim, 11 * 60 * 30);
        assert!(sim.suits.flight[id.idx()].pos.distance(at) < 1.0, "it drifted");
        !sim.rocks.destroyed.get(i)
    };
    let (_, rock) = lone_rock(&with_rocks(), 8.0, 60.0);
    let in_the_way = rock.radius + SUIT_CLEARANCE + REGROW_SLEEPER_CLEAR - 5.0;
    assert!(regrown(900.0, true), "a sleeper 900 m off held it back");
    assert!(regrown(in_the_way + 10.0, true), "a sleeper just clear of it held it back");
    assert!(!regrown(in_the_way, true), "it grew back over a sleeper");
    assert!(!regrown(REGROW_CLEAR - 100.0, false), "it grew back with a pilot awake 900 m off");
}

#[test]
fn hidden_sleepers_are_evicted_last() {
    let mut sim =
        Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, max_sleepers: 2, ..SimConfig::default() });
    // The first asleep, in THE DEEP; then two more out in the open.
    let (hidden, _) = crouched_on(&mut sim, Body::Landmark(1), Vec3::Y);
    lie_still(&mut sim, hidden, 1);
    assert_eq!(sim.suits.hide_spot[hidden.idx()], 0);
    let open: Vec<SuitId> = (0..2)
        .map(|k| leo(&mut sim, Faction::Colonies, Vec3::new(k as f32 * 100.0, 5_000.0, 9_000.0), Vec3::Z))
        .collect();
    assert!(sim.sleep(hidden));
    steps(&mut sim, 1);
    assert!(sim.sleep(open[0]));
    steps(&mut sim, 1);
    // Over the cap: the longest asleep is in a hide spot, so the next longest goes.
    assert!(sim.sleep(open[1]));
    assert!(sim.suits.valid(hidden), "the hidden sleeper was cleared first");
    assert!(!sim.suits.valid(open[0]));
    let mut f = Vec::new();
    sim.drain_fates(|x| f.push((x.suit as usize, x.gone)));
    assert_eq!(f, [(open[0].idx(), Gone::Evicted)]);
    // Room for two newcomers: the open one goes, and then, there being no other, the hidden one.
    let free = sim.suits.free_slots();
    sim.ensure_free_suits(free + 1);
    assert!(sim.suits.valid(hidden) && !sim.suits.valid(open[1]));
    sim.ensure_free_suits(free + 2);
    assert!(!sim.suits.valid(hidden));
}

#[test]
fn a_suit_that_slept_hidden_wakes_hidden() {
    let mut sim = landmarks_only();
    let (id, _) = crouched_on(&mut sim, Body::Landmark(1), Vec3::Y);
    let i = id.idx();
    lie_still(&mut sim, id, LURK_SETTLE_TICKS + 1);
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
    let (at, n) = (sim.suits.flight[i].pos, up(&sim, id));
    let far = leo(&mut sim, Faction::Oz, at + n * 2_000.0, -n);
    lie_still(&mut sim, id, 1);
    assert!(!sim.detects(far.idx(), i));
    // Its pilot leaves: the suit kneels there, and powers down in plain sight like any other.
    assert!(sim.sleep(id) && sim.is_parked(i));
    sim.step();
    assert!(sim.detects(far.idx(), i), "logging off doesn't hide it any sooner");
    steps(&mut sim, POWER_DOWN_TICKS);
    assert!(!sim.detects(far.idx(), i));
    assert_eq!(sim.suits.hide_spot[i], 0, "parked in THE DEEP");
    // Back, still crouched and still: hidden from the first tick, until it moves.
    assert!(sim.wake(id));
    lie_still(&mut sim, id, 1);
    assert_eq!(sim.cover_code(i), cover::HIDDEN);
    assert!(!sim.detects(far.idx(), i), "it woke in plain sight");
    let aim = sim.suits.flight[i].rot * Vec3::Z;
    drive(&mut sim, id, [0, 0, 127], GRIP, aim);
    sim.step();
    assert!(sim.detects(far.idx(), i));
}

#[test]
fn what_a_pilot_could_park_on_is_worked_out_once_a_tick() {
    use bc_proto::snapshot::own_flags::PARKABLE;
    let mut sim = with_rocks();
    let (r, rock) = lone_rock(&sim, 20.0, 1_500.0);
    let (id, out) = resting_on(&mut sim, &rock);
    let i = id.idx();
    let doll = sim
        .spawn_at(
            FrameId::Taurus,
            Faction::Oz,
            PilotKind::MobileDoll,
            rock.surface(rock.pos - out * (rock.radius + 50.0), 0.0) - out * (SUIT_CLEARANCE + 0.5),
            look_rotation(-out, Vec3::Y),
        )
        .unwrap();
    assert_eq!(sim.own_state(i).flags & PARKABLE, 0, "(nothing's worked out before the first tick)");
    sim.step();
    assert_eq!(sim.suits.parkable[i], Body::Rock(r as u16));
    assert_ne!(sim.own_state(i).flags & PARKABLE, 0);
    assert_eq!(sim.suits.parkable[doll.idx()], Body::None, "a doll never parks: nobody asks");
    assert!(sim.sleep(id));
    sim.step();
    assert_eq!(sim.suits.parkable[i], Body::None, "asleep, it's parked already");
}
