//! The owner's prediction lands, walks, hops, crouches, lifts off and lets go where the server does,
//! on a rock, on Hermit and on the rolling MO-II: seeded from any snapshot (in the body's frame, on
//! one), it replays the pilot's commands through the same catches, touchdowns and releases on the
//! same ticks, and stands where the server stands, in the body's frame, tick after tick. A rock
//! dug out from underfoot lets go of its rider on the tick the server says, however late the news.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FLIGHT_ASSIST, GRIP, MELEE};
use bc_proto::snapshot::{OwnArms, own_flags};
use bc_proto::{
    Faction, FrameId, InputCmd, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter,
};
use bc_sim::bodies::{Bodies, Body};
use bc_sim::field::Field;
use bc_sim::ground::{Anchor, Footing, GRIP_MIN_AXIS, STANCE};
use bc_sim::math::{look_rotation, quat_axis_angle};
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

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

/// What the pilot does at tick `t` of a run: comes in with the grip armed and lands, walks
/// (turning, and leaning to aim up and down), runs, hops on the run, crouches and walks crouched,
/// stands, strafes and hops again; then lets go and is caught again; then either digs the rock
/// out from under itself, or holds Space and lifts off.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Act {
    thrust: [i8; 3],
    buttons: u16,
    /// How far round the ground the aim has turned (rad), and how far up it pitches (rad).
    heading: f32,
    pitch: f32,
}

fn act(t: u32, dig: bool) -> Act {
    let mut a = Act { thrust: [0; 3], buttons: FLIGHT_ASSIST | GRIP, heading: 0.0, pitch: 0.0 };
    let s = t as f32;
    match t {
        // Caught, falling in the grip, landing.
        0..150 => {}
        // Walking, turning as it goes, the aim swinging up and down.
        150..230 => {
            a.thrust[2] = 127;
            a.heading = (s - 150.0) * 0.02;
            a.pitch = ((s - 150.0) * 0.1).sin() * 0.9;
        }
        // Running, hopping on the run.
        230..300 => {
            (a.thrust[2], a.buttons) = (127, a.buttons | BOOST);
            a.heading = 1.6 - (s - 230.0) * 0.01;
            if t == 260 {
                a.thrust[1] = 127;
            }
        }
        // Crouching, and walking crouched; then the crouch held with nothing on the stick.
        300..380 => {
            a.thrust = [0, -127, if t < 350 { 90 } else { 0 }];
            a.heading = 0.9;
        }
        380..400 => a.heading = 0.9,
        // Standing up, strafing, and a hop from a stand.
        400..450 => {
            a.thrust = [if t >= 420 { 127 } else { 0 }, 64, 0];
            a.heading = 0.9;
        }
        450 => a.thrust[1] = 127,
        // Letting go (a push off the ground), then coming back into its grip.
        500 => a.buttons = FLIGHT_ASSIST,
        _ if !dig => {
            // Holding Space: off the ground, climbing in the grip, and past its reach, free.
            if t >= 640 {
                a.thrust[1] = 127;
            }
        }
        _ => {
            // Crouched, aimed straight down, cutting and shooting until the rock gives.
            if t >= 640 {
                a.thrust[1] = -127;
                a.pitch = -1.5;
                if t.is_multiple_of(2) {
                    a.buttons |= MELEE;
                }
                a.buttons |= FIRE_PRIMARY;
            }
        }
    }
    a
}

/// What the server did, tick by tick (index = tick).
struct Run {
    cmds: Vec<InputCmd>,
    owns: Vec<OwnState>,
    /// The server's suit: its world state, footing and anchor.
    poses: Vec<(Vec3, Vec3, Quat, Footing, Anchor)>,
    /// Rocks the server broke, and the tick each broke on.
    breaks: Vec<(u16, u32)>,
    /// The field the client would generate from the Welcome.
    field: (u32, u16),
}

/// A Leo `height` m over `body` along `dir_local` (its frame), moving with its surface, flown by
/// [`act`] for `ticks`.
fn fly(cfg: SimConfig, body: Body, dir_local: Vec3, height: f32, ticks: u32, dig: bool) -> Run {
    fly_by(cfg, body, dir_local, height, ticks, |t| act(t, dig), None)
}

/// A Leo `height` m over `body` along `dir_local` (its frame), moving with its surface, flown by
/// `act` for `ticks`; aimed toward `toward` on the body (its frame), or a fixed way along it.
fn fly_by(
    cfg: SimConfig,
    body: Body,
    dir_local: Vec3,
    height: f32,
    ticks: u32,
    act: impl Fn(u32) -> Act,
    toward: Option<Vec3>,
) -> Run {
    let field = (cfg.field_seed, cfg.field_rocks);
    let mut sim = Sim::new(cfg);
    let bodies = Bodies::at(&sim.field, sim.landmarks(), 0);
    let pose = bodies.pose(body).unwrap();
    let (at, n) = bodies.surface_along(body, dir_local).unwrap();
    let start = pose.to_world(at + n * (STANCE + height));
    let id = sim
        .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, start, look_rotation(Vec3::Z, Vec3::Y))
        .unwrap();
    let i = id.idx();
    sim.suits.flight[i].vel = pose.point_vel(start);
    if let Body::Rock(r) = body {
        // Soft enough to dig through in a few seconds.
        sim.rocks.hp[usize::from(r)] = 400.0;
    }
    let s = &sim.suits.flight[i];
    let mut run = Run {
        cmds: vec![InputCmd::default()],
        owns: vec![over_the_wire(&sim.own_state(i))],
        poses: vec![(s.pos, s.vel, s.rot, sim.footing(i), sim.suits.anchor[i])],
        breaks: Vec::new(),
        field,
    };
    let dead = |sim: &Sim| (0..sim.field.len()).filter(|&r| sim.field.is_dead(r)).collect::<Vec<_>>();
    let mut was_dead = dead(&sim);
    for t in 1..=ticks {
        let a = act(t);
        // The aim, in the ground's plane where the suit is (its heading round it from a fixed
        // way along it), pitched up or down.
        let b = Bodies::at(&sim.field, sim.landmarks(), t - 1);
        let (p, shape) = (b.pose(body).unwrap(), b.shape(body).unwrap());
        let local = p.to_local(sim.suits.flight[i].pos);
        let up = shape.probe(local).normal;
        let along = match toward {
            // Walking at a place on the body.
            Some(to) => (to - local - up * (to - local).dot(up)).normalize_or(Vec3::X),
            None => up.cross(if up.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }).normalize(),
        };
        let flat = quat_axis_angle(up, a.heading) * along;
        let aim = p.rot * (flat * a.pitch.cos() + up * a.pitch.sin()).normalize();
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            thrust: a.thrust,
            buttons: a.buttons,
            ..InputCmd::default()
        }
        .quantized();
        sim.set_input(id, cmd);
        sim.step();
        assert_eq!(sim.tick(), t);
        run.cmds.push(cmd);
        run.owns.push(over_the_wire(&sim.own_state(i)));
        let s = &sim.suits.flight[i];
        run.poses.push((s.pos, s.vel, s.rot, sim.footing(i), sim.suits.anchor[i]));
        let now_dead = dead(&sim);
        for r in now_dead.iter().filter(|r| !was_dead.contains(r)) {
            run.breaks.push((*r as u16, t));
        }
        was_dead = now_dead;
    }
    run
}

/// Whether tick `t` held nothing a client can't foresee: no mount started or stopped waiting on
/// what isn't sent (energy, ammunition, a limb), and no overheat came or went.
fn foreseeable(owns: &[OwnState], t: usize) -> bool {
    let (a, b) = (&owns[t - 1], &owns[t]);
    let never = |o: &OwnState| o.arms.wait.map(|w| w == OwnArms::NEVER);
    let hot = |o: &OwnState| o.flags & own_flags::OVERHEAT != 0;
    never(a) == never(b) && hot(a) == hot(b)
}

#[derive(Default, Debug)]
struct Seen {
    checked: u32,
    skipped: u32,
    /// Footing changes the server made: caught, landed, hopped (or walked off), let go.
    caught: u32,
    landed: u32,
    hopped: u32,
    released: u32,
    /// Ticks crouched on the ground, and running on it.
    crouched: u32,
    running: u32,
    worst_pos: f32,
    worst_vel: f32,
    worst_turn: f32,
}

fn check(name: &str, run: &Run) -> Seen {
    let mut seen = Seen::default();
    let ticks = run.cmds.len() as u32 - 1;
    for t in 1..=ticks as usize {
        let (a, b) = (run.poses[t - 1].3, run.poses[t].3);
        seen.caught += u32::from(a == Footing::Free && b == Footing::Aloft);
        seen.landed += u32::from(a == Footing::Aloft && b == Footing::Grounded);
        seen.hopped += u32::from(a == Footing::Grounded && b == Footing::Aloft);
        seen.released += u32::from(a != Footing::Free && b == Footing::Free);
        let anchor = run.poses[t].4;
        if b == Footing::Grounded {
            seen.crouched += u32::from(anchor.stance < STANCE);
            seen.running += u32::from(anchor.vel.length() > 12.0);
        }
    }
    for seed in 1..=ticks - LEAD {
        // A rock breaking after the snapshot, before the prediction's done, is news it hasn't had.
        let unheard = run.breaks.iter().any(|&(_, b)| b > seed && b < seed + LEAD);
        if unheard || !(seed + 1..=seed + LEAD).all(|t| foreseeable(&run.owns, t as usize)) {
            seen.skipped += 1;
            continue;
        }
        seen.checked += 1;
        let mut history = InputHistory::default();
        for t in seed.saturating_sub(4)..=seed + LEAD {
            history.push(run.cmds[t as usize]);
        }
        let mut p = Predictor::default();
        p.set_field(Field::generate(run.field.0, run.field.1));
        // What the client has heard by the snapshot: which rocks broke, and when.
        for &(r, b) in run.breaks.iter().filter(|(_, b)| *b <= seed) {
            p.note_rock_break(r, b + 1);
            p.set_rock_dead(usize::from(r), true);
        }
        p.reconcile(seed, &run.owns[seed as usize], &history);
        for t in seed + 1..=seed + LEAD {
            p.advance(&run.cmds[t as usize], &history);
            let at = format!("{name} seeded at {seed}, tick {t}");
            let (pos, vel, rot, footing, anchor) = run.poses[t as usize];
            let m = p.mover();
            assert_eq!(m.footing, footing, "{at}: footing");
            assert_eq!(m.anchor.body, anchor.body, "{at}: body");
            assert_eq!(m.anchor.stance, anchor.stance, "{at}: stance");
            // On a body, in its frame; free, in the sector's.
            let (dp, dv, turn) = if footing == Footing::Free {
                (
                    (m.flight.pos - pos).length(),
                    (m.flight.vel - vel).length(),
                    2.0 * (m.flight.rot.inverse() * rot).xyz().length(),
                )
            } else {
                let a = &m.anchor;
                (
                    (a.local - anchor.local).length(),
                    (a.vel - anchor.vel).length(),
                    2.0 * (a.rot.inverse() * anchor.rot).xyz().length(),
                )
            };
            assert!(
                dp < 0.01 && dv < 0.05 && turn < 1e-3,
                "{at} ({footing:?}): off by {dp} m, {dv} m/s, {turn} rad"
            );
            seen.worst_pos = seen.worst_pos.max(dp);
            seen.worst_vel = seen.worst_vel.max(dv);
            seen.worst_turn = seen.worst_turn.max(turn);
        }
    }
    seen
}

/// The smallest rock a suit can grip, well clear of the others.
fn grippable_rock(cfg: &SimConfig) -> u16 {
    let field = Field::generate(cfg.field_seed, cfg.field_rocks);
    let rocks = field.rocks();
    let (i, _) = rocks
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            r.axes.min_element() >= GRIP_MIN_AXIS + 2.0
                && rocks
                    .iter()
                    .enumerate()
                    .all(|(j, o)| j == *i || o.pos.distance(r.pos) > r.radius + o.radius + 150.0)
        })
        .min_by(|a, b| a.1.radius.total_cmp(&b.1.radius))
        .expect("a grippable rock");
    i as u16
}

#[test]
fn the_owners_feet_keep_time_with_the_servers() {
    let rocks = SimConfig { target_dolls: 0, ..SimConfig::default() };
    let rock = grippable_rock(&rocks);
    let open = SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() };
    let runs = [
        ("a rock", fly(rocks, Body::Rock(rock), Vec3::new(0.2, 1.0, 0.3), 18.0, 900, true)),
        ("Hermit", fly(open, Body::Landmark(1), Vec3::new(0.3, 1.0, 0.2), 20.0, 900, false)),
        ("MO-II", fly(open, Body::Landmark(0), Vec3::new(1.0, 0.6, 0.6), 20.0, 900, false)),
    ];
    for (name, run) in &runs {
        let seen = check(name, run);
        println!("{name}: {seen:?}");
        assert!(seen.checked >= 700, "{name}: only {} seeds checked", seen.checked);
        assert!(seen.caught >= 2 && seen.landed >= 3 && seen.hopped >= 2, "{name}: {seen:?}");
        assert!(seen.crouched > 30 && seen.running > 20, "{name}: {seen:?}");
        // Let go (and pushed off), and at the end either dug out or lifted off.
        assert!(seen.released >= 2, "{name}: {seen:?}");
    }
    // Across MO-II's aft face to the Aft Well: its rim is a wall, so the suit stops at it, hops
    // in, and crouches on the bowl's floor.
    let open = SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() };
    let well = Vec3::new(-235.0, 0.0, 0.0);
    let hop_in = |t: u32| Act {
        thrust: [
            0,
            match t {
                230 => 127,
                330.. => -127,
                _ => 0,
            },
            if (150..330).contains(&t) { 127 } else { 0 },
        ],
        buttons: FLIGHT_ASSIST | GRIP,
        heading: 0.0,
        pitch: 0.0,
    };
    let run = fly_by(open, Body::Landmark(0), Vec3::new(-260.0, 0.0, 66.0), 15.0, 450, hop_in, Some(well));
    let seen = check("the Aft Well", &run);
    println!("the Aft Well: {seen:?}");
    let at_rim = run.poses[225].4;
    assert!(
        at_rim.local.z > 53.0 && at_rim.vel.length() > 7.0,
        "held at the rim, walking into it: {at_rim:?}"
    );
    assert!(seen.hopped == 1 && seen.landed == 2 && seen.crouched > 60, "{seen:?}");
    let (.., footing, end) = *run.poses.last().unwrap();
    let inside = end.local.x > -260.0 && Vec3::new(0.0, end.local.y, end.local.z).length() < 53.0;
    assert!(footing == Footing::Grounded && inside, "{footing:?} at {}", end.local);
    assert!(seen.checked >= 430, "{seen:?}");
    // The rock rider dug its rock out from under itself, and was let go the tick after.
    let (_, rock_run) = &runs[0];
    let &(r, broke) = rock_run.breaks.first().expect("the rock was dug out");
    assert_eq!(r, rock);
    assert_ne!(rock_run.poses[broke as usize].3, Footing::Free, "let go the very tick it broke");
    assert_eq!(rock_run.poses[broke as usize + 1].3, Footing::Free);
}
