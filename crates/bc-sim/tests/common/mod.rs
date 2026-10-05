//! Shared scenario builders for the bc-sim integration tests.
#![allow(dead_code, clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{
    BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, GRIP, JETTISON, MELEE, RCS_SHARP, STOW, THROW,
    ZERO,
};
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind};
use bc_sim::bodies::{Bodies, Body, BodyPose, Probe};
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::field::{Rock, SUIT_CLEARANCE};
use bc_sim::ground::{
    Anchor, Footing, GRIP_MIN_AXIS, LUNGE_GROUND_SPEED, PLACE_TOL, RELEASE_SPEED, derive, place,
};
use bc_sim::math::{hash01, look_rotation};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

/// A busy sector: `humans` player-controlled suits (half Wing Zero with ZERO engaged) and `dolls`
/// Mobile Dolls, spread over a few km so everyone has contacts.
pub fn arena(humans: usize, dolls: usize, seed: u64) -> (Sim, Vec<SuitId>) {
    let cfg = SimConfig { target_dolls: 0, seed, max_suits: 512, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let mut players = Vec::new();
    for k in 0..humans {
        let frame = if k % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        let a = k as f32 * 0.37;
        let pos = Vec3::new(a.cos() * 1_800.0, 800.0 + (k % 5) as f32 * 60.0, a.sin() * 1_800.0);
        let id = sim
            .spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(-pos, Vec3::Y))
            .expect("slot");
        players.push(id);
    }
    for k in 0..dolls {
        let a = k as f32 * 0.61;
        let r = 1_000.0 + (k % 7) as f32 * 150.0;
        let pos = Vec3::new(a.cos() * r, 1_200.0 + (k % 3) as f32 * 80.0, a.sin() * r);
        let frame = if k % 4 == 3 { FrameId::Virgo } else { FrameId::Taurus };
        sim.spawn_at(frame, Faction::Oz, PilotKind::MobileDoll, pos, look_rotation(-pos, Vec3::Y))
            .expect("slot");
    }
    (sim, players)
}

/// Deterministic "human-like" input: weaving thrust, aim sweeping toward the centre, bursts of fire,
/// ZERO engaged on Wing Zeros, occasional boosts and saber swings.
pub fn scripted(sim: &Sim, id: SuitId, tick: u32) -> InputCmd {
    let i = id.idx() as u32;
    let pos = sim.suits.flight[id.idx()].pos;
    let wobble = Vec3::new(
        hash01(tick / 20, i) - 0.5,
        hash01(tick / 20, i + 99) - 0.5,
        hash01(tick / 20, i + 7) - 0.5,
    );
    let aim = (Vec3::new(0.0, 1_100.0, 0.0) - pos).normalize_or(Vec3::Z) + wobble * 0.3;
    let mut buttons = FLIGHT_ASSIST | RCS_SHARP;
    if !(tick / 15 + i).is_multiple_of(3) {
        buttons |= FIRE_PRIMARY;
    }
    if (tick / 7 + i).is_multiple_of(4) {
        buttons |= FIRE_SECONDARY;
    }
    if (tick / 45 + i).is_multiple_of(5) {
        buttons |= BOOST;
    }
    if (tick + i).is_multiple_of(97) {
        buttons |= MELEE;
    }
    if sim.suits.frame[id.idx()] == FrameId::WingZero {
        buttons |= ZERO;
    }
    // Salvage: the free hand reaches for wreckage in stretches; now and then stow, throw, or dump
    // the hold.
    if (tick / 60 + i).is_multiple_of(3) {
        buttons |= GRAB;
    }
    if (tick + i * 7).is_multiple_of(53) {
        buttons |= STOW;
    }
    if (tick + i * 5).is_multiple_of(89) {
        buttons |= THROW;
    }
    if (tick + i * 3).is_multiple_of(149) {
        buttons |= JETTISON;
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        tick,
        view_tick_q4: (tick << 4).saturating_sub(40 + (i % 50)),
        aim: aim.normalize_or(Vec3::Z),
        thrust: [q(wobble.x * 2.0), q(wobble.y * 2.0), q(0.3 + wobble.z)],
        roll: 0,
        buttons: pull(sim, id, buttons),
        lock_target: NO_SLOT,
        shot_seq: (tick / 10) as u8,
        lockon: None,
    }
    .quantized()
}

/// `buttons`, with the primary's trigger pulled only on ticks it would fire if its shot charges (tap
/// fires, hold charges: `bc_sim::arms::charged_pull`). So a script that holds the trigger fires just
/// as it did before charging came, shot for shot.
pub fn pull(sim: &Sim, id: SuitId, buttons: u16) -> u16 {
    let charged = bc_sim::content::frame(sim.suits.frame[id.idx()]).loadout[0]
        .is_some_and(|m| bc_sim::content::weapon(m.weapon).charged.is_some());
    if buttons & FIRE_PRIMARY != 0 && charged && !sim.would_fire(id.idx(), 0) {
        buttons & !FIRE_PRIMARY
    } else {
        buttons
    }
}

/// Steps the arena `ticks` times with scripted input for every player.
pub fn run(sim: &mut Sim, players: &[SuitId], ticks: u32) {
    for _ in 0..ticks {
        let t = sim.next_tick();
        for &id in players {
            let cmd = scripted(sim, id, t);
            sim.set_input(id, cmd);
        }
        sim.step();
    }
}

/// The Gundams' arena: every pilots' frame in pairs (one per side) 12 m apart, spread along x,
/// among `dolls` Mobile Dolls. Returns the sim and the pilots, partners adjacent.
pub fn gundam_arena(dolls: usize, seed: u64) -> (Sim, Vec<SuitId>) {
    let (mut sim, _) = arena(0, dolls, seed);
    // Pairs (Colonies, OZ), 14 m apart. Sandrock meets a Taurus: two Sandrocks parry each other's
    // every stroke.
    let pairs = [
        (FrameId::WingZero, FrameId::WingZero),
        (FrameId::Heavyarms, FrameId::Heavyarms),
        (FrameId::Deathscythe, FrameId::Deathscythe),
        (FrameId::Sandrock, FrameId::Taurus),
        (FrameId::Shenlong, FrameId::Shenlong),
        (FrameId::Leo, FrameId::Leo),
    ];
    let mut pilots = Vec::new();
    for (k, pair) in pairs.iter().enumerate() {
        for (side, (f, faction)) in
            [(pair.0, Faction::Colonies), (pair.1, Faction::Oz)].into_iter().enumerate()
        {
            let pos = Vec3::new(k as f32 * 400.0 - 1_000.0, 1_100.0, side as f32 * 14.0);
            let facing = if side == 0 { Vec3::Z } else { -Vec3::Z };
            let id = sim.spawn_at(f, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y));
            pilots.push(id.expect("slot"));
        }
    }
    (sim, pilots)
}

/// A busy sector of Gundams: `pairs` duels round a ring (the Colonies side cycling through every
/// playable frame, each meeting its own kind, or a Taurus for Sandrock), and `dolls` Mobile Dolls.
pub fn gundam_crowd(pairs: usize, dolls: usize, seed: u64) -> (Sim, Vec<(SuitId, SuitId)>) {
    let (mut sim, _) = arena(0, dolls, seed);
    let mut duels = Vec::new();
    for k in 0..pairs {
        let f = bc_sim::content::PLAYABLE_ORDER[k % bc_sim::content::PLAYABLE_ORDER.len()];
        let foe = if f == FrameId::Sandrock { FrameId::Taurus } else { f };
        let a = k as f32 * 0.43;
        let at = Vec3::new(a.cos() * 2_000.0, 1_000.0 + (k % 4) as f32 * 90.0, a.sin() * 2_000.0);
        let out = at.normalize();
        let mut spawn = |f: FrameId, faction: Faction, pos: Vec3, facing: Vec3| {
            sim.spawn_at(f, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).expect("slot")
        };
        let x = spawn(f, Faction::Colonies, at, out);
        let y = spawn(foe, Faction::Oz, at + out * 300.0, -out);
        duels.push((x, y));
    }
    (sim, duels)
}

/// Deterministic input for a Gundam pilot duelling `foe`: aim at it and designate it, close in,
/// strike with every blade and the special, fire in bursts.
pub fn duel_scripted(sim: &Sim, id: SuitId, foe: SuitId, tick: u32) -> InputCmd {
    let i = id.idx() as u32;
    let me = sim.suits.flight[id.idx()].pos;
    let wobble =
        Vec3::new(hash01(tick / 15, i) - 0.5, hash01(tick / 15, i + 5) - 0.5, hash01(tick / 15, i + 9) - 0.5);
    let to = sim.suits.flight[foe.idx()].pos - me;
    let aim = to.normalize_or(Vec3::Z) + wobble * 0.1;
    let mut buttons = FLIGHT_ASSIST;
    // Blades and the special when the foe is near and ahead, now and then.
    let ahead = (sim.suits.flight[id.idx()].rot * Vec3::Z).dot(to.normalize_or(Vec3::Z)) > 0.8;
    if ahead && to.length() < 16.0 && (tick + i).is_multiple_of(7) {
        buttons |= MELEE;
    }
    if ahead && to.length() < 14.0 && (tick + 3 * i).is_multiple_of(11) {
        buttons |= bc_proto::buttons::SPECIAL;
    }
    // The frame's mode (the Hyper Jammer, Neo-Bird) for stretches of three seconds.
    if (tick / 90 + i).is_multiple_of(2) {
        buttons |= bc_proto::buttons::MODE;
    }
    if (tick / 10 + i).is_multiple_of(3) {
        buttons |= FIRE_PRIMARY;
    }
    if (tick / 7 + i).is_multiple_of(4) {
        buttons |= FIRE_SECONDARY;
    }
    if to.length() > 60.0 && (tick / 30 + i).is_multiple_of(2) {
        buttons |= BOOST;
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    // Flight assist holds the velocity the stick asks for (suit frame): toward the foe, slower as
    // it closes, and a little weave.
    let local = sim.suits.flight[id.idx()].rot.inverse() * to.normalize_or(Vec3::Z);
    let pull = ((to.length() - 6.0) / 300.0).clamp(0.0, 0.5);
    let weave = wobble * 0.05;
    InputCmd {
        tick,
        view_tick_q4: (tick << 4).saturating_sub(40 + (i % 50)),
        aim: aim.normalize_or(Vec3::Z),
        thrust: [q(local.x * pull + weave.x), q(local.y * pull + weave.y), q(local.z * pull + weave.z)],
        roll: 0,
        buttons: self::pull(sim, id, buttons),
        lock_target: foe.idx() as u16,
        shot_seq: (tick / 10) as u8,
        lockon: None,
    }
    .quantized()
}

/// [`duel_scripted`], locked on (`bc_proto::LockOn`): flight assist holds the foe's velocity, the
/// fight's up is the colony's, and the stick, in the fight's axes, closes in and then circles one
/// way and the other, with a burst step now and then.
pub fn locked_scripted(sim: &Sim, id: SuitId, foe: SuitId, tick: u32) -> InputCmd {
    let cmd = duel_scripted(sim, id, foe, tick);
    let (me, it) = (&sim.suits.flight[id.idx()], &sim.suits.flight[foe.idx()]);
    let range = me.pos.distance(it.pos);
    let close = ((range - 20.0) / 200.0).clamp(-0.5, 1.0);
    let circle = if (tick / 90 + id.idx() as u32).is_multiple_of(2) { 0.6 } else { -0.6 };
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    let lockon = bc_proto::LockOn { ref_vel: it.vel, up: bc_sim::world::colony_up(me.pos) };
    // Now and then a burst step, the way it circles.
    if (tick + 13 * id.idx() as u32).is_multiple_of(70) {
        let buttons = cmd.buttons | bc_proto::buttons::BURST;
        return InputCmd { thrust: [q(circle * 2.0), 0, 0], roll: 0, buttons, lockon: Some(lockon), ..cmd }
            .quantized();
    }
    InputCmd { thrust: [q(circle), 0, q(close)], roll: 0, lockon: Some(lockon), ..cmd }.quantized()
}

/// The smallest rock bigger than `min` m with nothing else within `clear` m of it.
pub fn lone_rock(sim: &Sim, min: f32, clear: f32) -> (usize, Rock) {
    let rocks = sim.field.rocks();
    rocks
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            r.radius > min
                && rocks
                    .iter()
                    .enumerate()
                    .all(|(j, o)| j == *i || o.pos.distance(r.pos) > r.radius + o.radius + clear)
        })
        .min_by(|a, b| a.1.radius.total_cmp(&b.1.radius))
        .map(|(i, r)| (i, *r))
        .expect("a lone rock")
}

/// A Colonies Leo at rest against rock `r`, just off its surface and facing away from it, and the
/// way out from the rock there.
pub fn resting_on(sim: &mut Sim, r: &Rock) -> (SuitId, Vec3) {
    resting_as(sim, FrameId::Leo, r, -Vec3::new(1.0, 0.1, 0.3).normalize())
}

/// A Colonies `frame` at rest against rock `r` on its side `out` of it, facing away from it, and
/// the way out from the rock there.
pub fn resting_as(sim: &mut Sim, frame: FrameId, r: &Rock, out: Vec3) -> (SuitId, Vec3) {
    let dir = -out;
    let surface = r.surface(r.pos - dir * (r.radius + 50.0), 0.0);
    let pos = surface + out * (SUIT_CLEARANCE + 0.5);
    let id = sim
        .spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(out, Vec3::Y))
        .expect("slot");
    (id, out)
}

/// Landmark `k` as it is now (whether or not the sector has it): its pose, and the probe of its surface at `p` (sector frame; the
/// normal is turned into the sector's frame too).
pub fn landmark_probe(sim: &Sim, k: u8, p: Vec3) -> (BodyPose, Probe) {
    let bodies = Bodies::at(&sim.field, &LANDMARKS, sim.tick());
    let pose = bodies.pose(Body::Landmark(k)).expect("a landmark");
    let pr = bodies.shape(Body::Landmark(k)).expect("a landmark").probe(pose.to_local(p));
    (pose, Probe { dist: pr.dist, normal: pose.rot * pr.normal })
}

/// The outer surface of landmark `k` straight out from its origin along `dir` (its frame), as it
/// is now (whether or not the sector has it): the point and the outward normal, in the sector's frame.
pub fn landmark_surface(sim: &Sim, k: u8, dir: Vec3) -> (Vec3, Vec3) {
    let bodies = Bodies::at(&sim.field, &LANDMARKS, sim.tick());
    let pose = bodies.pose(Body::Landmark(k)).expect("a landmark");
    let (p, n) = bodies.surface_along(Body::Landmark(k), dir).expect("a surface");
    (pose.to_world(p), pose.rot * n)
}

/// The smallest rock a suit can grip, with nothing else within `clear` m of it.
pub fn grippable_rock(sim: &Sim, clear: f32) -> (usize, Rock) {
    let rocks = sim.field.rocks();
    rocks
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            r.axes.min_element() >= GRIP_MIN_AXIS
                && rocks
                    .iter()
                    .enumerate()
                    .all(|(j, o)| j == *i || o.pos.distance(r.pos) > r.radius + o.radius + clear)
        })
        .min_by(|a, b| a.1.radius.total_cmp(&b.1.radius))
        .map(|(i, r)| (i, *r))
        .expect("a grippable rock")
}

/// A `frame` of `faction` standing on `body`, on its outermost surface straight out along
/// `dir_local` (its frame), gripping.
pub fn standing_on(sim: &mut Sim, frame: FrameId, faction: Faction, body: Body, dir_local: Vec3) -> SuitId {
    let bodies = Bodies::at(&sim.field, &LANDMARKS, sim.tick());
    let at = bodies.pose(body).expect("a body").pos;
    let id = sim
        .spawn_at(frame, faction, PilotKind::Human, at + Vec3::Y * 5_000.0, look_rotation(Vec3::Z, Vec3::Y))
        .expect("slot");
    assert!(sim.place_on(id, body, dir_local), "{frame:?} can't stand on {body:?}");
    id
}

/// A Colonies `frame` standing on MO-II along `dir_local`.
pub fn on_mo_ii(sim: &mut Sim, frame: FrameId, dir_local: Vec3) -> SuitId {
    standing_on(sim, frame, Faction::Colonies, Body::Landmark(0), dir_local)
}

/// A Colonies `frame` standing on Hermit along `dir_local`.
pub fn on_hermit(sim: &mut Sim, frame: FrameId, dir_local: Vec3) -> SuitId {
    standing_on(sim, frame, Faction::Colonies, Body::Landmark(1), dir_local)
}

/// Whether rock `r` broke on the last tick.
fn broke_last_tick(sim: &Sim, r: u16) -> bool {
    (sim.events.oldest_seq()..sim.events.next_seq()).filter_map(|s| sim.events.get(s)).any(|e| {
        matches!(e, bc_proto::events::Event::RockBreak { rock, tick, .. } if *rock == r && *tick == sim.tick())
    })
}

fn same(a: Vec3, b: Vec3) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

/// Asserts the surface invariants (the design's I1 to I7 and I11) for every suit, as a tick
/// leaves them:
/// - I1: a suit on a body (or asleep on one) is exactly where its anchor puts it;
/// - I2: a suit on a body has one, and it's there (or broke this very tick);
/// - I3, I4, I5: a free suit, a wreck and a sleeper resting on a body have the anchors they should;
/// - I6: a suit on its feet stands on the ground, unless something else is in its way;
/// - I7: stances are on the sixteenth-metre grid;
/// - I11: speeds over a body are within the grip's, and positions within the wire's reach.
pub fn check_invariants(sim: &Sim) {
    let bodies = Bodies::at(&sim.field, sim.landmarks(), sim.tick());
    let s = &sim.suits;
    for i in s.used.iter() {
        let (footing, a, f) = (s.footing[i], s.anchor[i], s.flight[i]);
        let asleep = s.sleeping.get(i);
        let t = sim.tick();
        if !s.alive.get(i) {
            assert!(footing == Footing::Free && a == Anchor::default(), "I4: wreck {i} on a body at {t}");
            continue;
        }
        if footing != Footing::Free {
            assert_ne!(a.body, Body::None, "I2: suit {i} on nothing at {t}");
            let there = bodies.alive(a.body) || matches!(a.body, Body::Rock(r) if broke_last_tick(sim, r));
            assert!(there, "I2: suit {i} on {:?}, which is gone, at {t}", a.body);
            let q = a.stance * 16.0;
            assert!(q == q.floor() && (96.0..=146.0).contains(&q), "I7: suit {i}'s stance {}", a.stance);
            let cap = if footing == Footing::Aloft { RELEASE_SPEED } else { LUNGE_GROUND_SPEED };
            assert!(
                a.vel.length() <= cap + 1e-3,
                "I11: suit {i} at {} m/s {footing:?} at {t}",
                a.vel.length()
            );
            let reach = if matches!(a.body, Body::Rock(_)) { 256.0 } else { 1_024.0 };
            assert!(a.local.length() < reach, "I11: suit {i} {} m out on {:?}", a.local.length(), a.body);
        } else if !asleep {
            assert_eq!(a, Anchor::default(), "I3: free suit {i} has an anchor at {t}");
        } else if a.body != Body::None {
            assert_eq!(a.vel, Vec3::ZERO, "I5: parked suit {i} moves over its body");
        }
        if footing != Footing::Free || (asleep && a.body != Body::None) {
            let p = bodies.pose(a.body).expect("a body");
            let mut g = f;
            if asleep && footing != Footing::Aloft {
                g.pos = p.pos + p.rot * a.local;
                g.rot = p.rot * a.rot;
                g.vel = p.point_vel(g.pos);
                g.ang_vel = if p.moving { p.ang_vel } else { Vec3::ZERO };
            } else {
                derive(&p, &a, &mut g);
            }
            let exact = same(g.pos, f.pos)
                && g.rot.to_array().map(f32::to_bits) == f.rot.to_array().map(f32::to_bits)
                && same(g.vel, f.vel)
                && same(g.ang_vel, f.ang_vel);
            assert!(
                exact,
                "I1: suit {i} ({footing:?}) isn't where its anchor puts it at {t}: {f:?} vs {g:?}"
            );
        }
        if footing == Footing::Grounded {
            let shape = bodies.shape(a.body).expect("a shape");
            let h = place(&shape, a.local, a.stance).2;
            let reach = SUIT_CLEARANCE + 0.5;
            let other_rock = sim.field.rocks().iter().enumerate().any(|(r, rock)| {
                Body::Rock(r as u16) != a.body && rock.pos.distance(f.pos) < rock.radius + reach
            });
            let other_landmark = sim.landmarks().iter().enumerate().any(|(k, d)| {
                Body::Landmark(k as u8) != a.body && {
                    let pose = bodies.pose(Body::Landmark(k as u8)).unwrap();
                    d.shape.probe(pose.to_local(f.pos)).dist < reach
                }
            });
            // (The hull keeps suits 12 m off.)
            let hull = bc_sim::world::hull_contact(f.pos, 12.5).is_some();
            let walled = other_rock || other_landmark || hull;
            assert!(h.abs() <= PLACE_TOL || walled, "I6: suit {i}'s feet {h} m off the ground at {t}");
        }
    }
}

/// A crowd on the bodies: 128 suits standing on MO-II, Hermit and the first 16 rocks they can
/// grip (Leos of the Colonies and Heavyarms of the Alliance by turns), 64 OZ Heavyarms each 700 m
/// over one of them, and `dolls` Mobile Dolls. Returns the sim, the riders, the hunters with their
/// prey, and a rock with riders on it.
pub fn rider_crowd(dolls: usize, seed: u64) -> (Sim, Vec<SuitId>, Vec<(SuitId, SuitId)>, usize) {
    let (mut sim, _) = arena(0, dolls, seed);
    let rocks: Vec<u16> = (0..sim.field.len())
        .filter(|&r| sim.field.rocks()[r].axes.min_element() >= GRIP_MIN_AXIS)
        .map(|r| r as u16)
        .collect();
    let mut riders = Vec::new();
    for k in 0..128u32 {
        let body = match k % 3 {
            0 => Body::Landmark(0),
            1 => Body::Landmark(1),
            _ => Body::Rock(rocks[(k as usize / 3) % 16]),
        };
        let dir = Vec3::new(hash01(k, 1) - 0.5, hash01(k, 2) - 0.5, hash01(k, 3) - 0.5).normalize_or(Vec3::Y);
        let (frame, faction) = if k % 2 == 0 {
            (FrameId::Leo, Faction::Colonies)
        } else {
            (FrameId::Heavyarms, Faction::Alliance)
        };
        riders.push(standing_on(&mut sim, frame, faction, body, dir));
    }
    let hunters = (0..64)
        .map(|k| {
            let prey = riders[k * 2];
            let at = sim.suits.flight[prey.idx()].pos + Vec3::new(0.0, 700.0, 300.0 + 20.0 * k as f32);
            let id = sim
                .spawn_at(
                    FrameId::Heavyarms,
                    Faction::Oz,
                    PilotKind::Human,
                    at,
                    look_rotation(-Vec3::Y, Vec3::Z),
                )
                .expect("slot");
            (id, prey)
        })
        .collect();
    // Rider 8 stands on it.
    (sim, riders, hunters, usize::from(rocks[2]))
}

/// A rider's command for tick `t` (rider `k` of the crowd), on a 10 s cycle: it walks (some run),
/// hops, crouches, digs at the ground under it (crouched, blade and rifle), lets go for a third of a
/// second, then arms its grip on flight assist and is caught again. Every fourth never fights: it
/// lies still, crouched, where the others dig.
pub fn rider_scripted(sim: &Sim, id: SuitId, k: usize, t: u32) -> InputCmd {
    let f = sim.suits.flight[id.idx()];
    let (nose, up) = (f.rot * Vec3::Z, f.rot * Vec3::Y);
    let phase = (t + k as u32 * 7) % 300;
    let run = if k.is_multiple_of(3) { BOOST } else { 0 };
    let lurker = k % 4 == 3;
    let (aim, thrust, buttons) = match phase {
        0..120 => (nose, [((k % 5) as i8 - 2) * 40, 0, 127], GRIP | run),
        120 => (nose, [0, 127, 0], GRIP | FLIGHT_ASSIST),
        150..170 => (nose, [0, -127, 0], GRIP),
        200..260 if lurker => (nose, [0; 3], GRIP),
        200..260 => (-up, [0, -127, 0], GRIP | FIRE_PRIMARY | if phase % 45 < 3 { MELEE } else { 0 }),
        260..270 => (nose, [0; 3], FLIGHT_ASSIST),
        _ => (nose, [0; 3], GRIP | FLIGHT_ASSIST),
    };
    let buttons = pull(sim, id, buttons);
    InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() }.quantized()
}

/// A hunter's command for tick `t` (hunter `k`): aim at its prey and designate it, fire the gun,
/// and a salvo of missiles every 2 s.
pub fn hunter_scripted(sim: &Sim, id: SuitId, prey: SuitId, k: usize, t: u32) -> InputCmd {
    let aim = (sim.suits.flight[prey.idx()].pos - sim.suits.flight[id.idx()].pos).normalize_or(Vec3::Z);
    let fire = if (t + k as u32).is_multiple_of(60) { FIRE_SECONDARY } else { FIRE_PRIMARY };
    InputCmd {
        tick: t,
        view_tick_q4: (t << 4).saturating_sub(40),
        aim,
        buttons: pull(sim, id, FLIGHT_ASSIST | fire),
        lock_target: prey.idx() as u16,
        ..InputCmd::default()
    }
    .quantized()
}
