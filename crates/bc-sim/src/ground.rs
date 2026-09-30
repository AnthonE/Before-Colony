//! Surface contact: a suit in a body's grip, standing on it, walking, hopping, and letting go.
//!
//! A suit is **Free** (flying in the sector's frame), **Grounded** (standing on a body) or **Aloft**
//! (in the air over a body, in its grip). While it's attached, Grounded or Aloft, what it *is* is an
//! [`Anchor`] in the body's frame. Its world pose is derived from that every tick ([`derive`]), so
//! the spatial hash, lag compensation, weapons and AI see a rider as they see everyone, and nothing
//! drifts however the body moves.
//!
//! - **Grounded** is kinematic. The suit's origin rides [`STANCE`] (crouched, [`CROUCH_STANCE`])
//!   over the surface below it ([`place`]). The stick walks it along the ground toward where it
//!   aims, boost runs, and the suit stands up to the surface, faces the aim and leans back to reach
//!   it. Rounded edges and gentle curves are walked round; an inner corner, or an edge that turns
//!   the ground sharply, is a wall it slides along.
//! - **Aloft** is flight ([`flight::integrate`]) in the body's frame, under a grip gravity of
//!   [`GRIP_ACCEL`] toward the nearest surface, with any descent faster than [`LAND_SPEED_MAX`]
//!   braked for free. Flight assist holds a walk's or a run's speed along the surface, and along the
//!   normal only what the pilot asks for ([`HopAssist`]): it never holds altitude.
//!
//! Grip is opt-in ([`GRIP`]). Armed, a suit that comes in slow and close is caught (Aloft) and
//! lands; Space hops, and held it lifts off; clearing it lets go, with the exact velocity of the
//! surface it left. A suit that never arms it flies exactly as [`flight::step_in`] flies it.
//!
//! [`move_step`] is the one step: the server runs it for every live suit, and the owner's client
//! runs it to predict its own. Every transition is decided by the step's own inputs, so both sides
//! catch, land and let go on the same tick.

use bc_proto::InputCmd;
use bc_proto::buttons::{BOOST, BRAKE, GRIP};
use glam::{Quat, Vec3};

use crate::bodies::{Bodies, Body, BodyPose, Near, Shape};
use crate::content::FrameSpec;
use crate::flight::{self, FlightMods, FlightOut, FlightState, HopAssist};
use crate::math::{
    angle_between, clamp_len, length, look_rotation, normalize_or, quat_rotate_toward, rotation_vector, sqrt,
};

pub use crate::bodies::{GRIP_MIN_AXIS, STANCE};
pub use crate::flight::ROLL_LEVEL_GAIN;

/// How high a crouched suit's origin rides over the ground, m.
pub const CROUCH_STANCE: f32 = 6.0;
/// How far the stance moves a tick, m (5.625 m/s): a sixteenth-metre grid, so it is exact.
pub const STANCE_STEP: f32 = 3.0 / 16.0;
/// Speeds on the ground at full stick: walking, running (boost), crouched, m/s.
pub const WALK_SPEED: f32 = 8.0;
pub const RUN_SPEED: f32 = 16.0;
pub const CROUCH_SPEED: f32 = 3.0;
/// A blade's lunge on the ground is a dash along it, m/s.
pub const LUNGE_GROUND_SPEED: f32 = 28.0;
/// How hard the legs change the suit's speed: walking, lunging, braking, m/s².
pub const GROUND_ACCEL: f32 = 20.0;
pub const LUNGE_ACCEL: f32 = 60.0;
pub const BRAKE_ACCEL: f32 = 40.0;
/// Grip gravity: what pulls a suit Aloft toward the nearest surface, m/s² (0.61 g).
pub const GRIP_ACCEL: f32 = 6.0;
/// Aloft, a descent faster than this is braked, at no cost, m/s.
pub const LAND_SPEED_MAX: f32 = 8.0;
/// A hop's speed off the ground, m/s: an 8.3 m apex, 3.4 s in the air.
pub const JUMP_SPEED: f32 = 10.0;
/// Aloft, flight assist climbs (or dives) this fast along the normal at full stick, m/s.
pub const HOP_CLIMB: f32 = 20.0;
/// Letting go of the ground pushes off at this speed, m/s.
pub const TAKEOFF_SPEED: f32 = 6.0;
/// A suit whose body shatters under it floats off at this speed, m/s.
pub const UNPARK_SPEED: f32 = 1.5;
/// A catch: feet this close to a grippable surface, m, moving no faster than this relative to it,
/// m/s, nor leaving it faster than this, m/s.
pub const CATCH_RANGE: f32 = 25.0;
pub const CATCH_SPEED: f32 = 8.0;
pub const CATCH_LEAVE: f32 = 2.0;
/// Aloft, feet this high, m, or this fast relative to the body, m/s, and the grip is lost: well
/// past a catch's, so a catch is never undone by the fall it starts.
pub const RELEASE_RANGE: f32 = 40.0;
pub const RELEASE_SPEED: f32 = 30.0;
/// With the grip armed, a surface this near, m, and this slow relative to the suit, m/s, rolls the
/// suit's feet toward it.
pub const LEVEL_RANGE: f32 = 150.0;
pub const LEVEL_SPEED: f32 = 40.0;
/// Feet more than this over the ground after a step, m, and the ground is lost: the suit is Aloft.
pub const STEP_DOWN: f32 = 1.2;
/// The ground's normal turning more than this in one step is a wall, rad.
pub const MAX_STEP_TURN: f32 = 0.6;
/// The origin never comes closer than this to any surface of its body (walls, overhangs), m.
pub const BODY_CLEAR: f32 = 5.0;
/// How fast a suit on the ground turns to face its aim, rad/s: with its legs, and without them.
pub const GROUND_TURN_RATE: f32 = 2.5;
pub const LEGLESS_TURN_RATE: f32 = 0.8;
/// The most a suit on the ground leans back (or forward) toward its aim: sin 40°. With it, the
/// arms' 50° cones reach straight overhead.
pub const GROUND_LEAN_SIN: f32 = 0.642_787_6;
/// Snaps [`place`] makes onto the surface.
pub const SNAP_ITERS: u32 = 2;
/// How far off the ground a placed origin's feet are at most, m. A suit on the ground always is:
/// it never steps (or stands up) to where it wouldn't be, which is nearer than its stance to
/// another surface of its body.
pub const PLACE_TOL: f32 = 0.01;
/// `thrust[1]` on the ground: at or below this the suit crouches, at or above this it stands, and
/// at or above this (standing) it hops. Between, it keeps the stance it has.
pub const CROUCH_LEVEL: i8 = -64;
pub const STAND_LEVEL: i8 = 32;
pub const JUMP_LEVEL: i8 = 100;

/// How a suit stands with respect to the bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Footing {
    /// Flying free, in the sector's frame.
    #[default]
    Free = 0,
    /// Standing on a body.
    Grounded = 1,
    /// In the air over a body, in its grip.
    Aloft = 2,
}

/// Where a suit is relative to the body it's on, parked on, or in the grip of.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Anchor {
    pub body: Body,
    /// The suit's origin and orientation in the body's frame.
    pub local: Vec3,
    pub rot: Quat,
    /// The origin's velocity relative to the body, and the suit's angular velocity, both in the
    /// body's frame (m/s, rad/s).
    pub vel: Vec3,
    pub ang_vel: Vec3,
    /// How high the origin rides over the surface, m, on a sixteenth-metre grid; 0 when Free.
    pub stance: f32,
}

/// What [`move_step`] moves: a suit's row on the server, the owner's prediction on its client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mover {
    /// The world state: flown while Free, derived from the anchor while attached.
    pub flight: FlightState,
    pub footing: Footing,
    pub anchor: Anchor,
}

/// What a suit brings to its step besides its command.
#[derive(Clone, Copy, Debug)]
pub struct MoveCtx<'a> {
    pub spec: &'a FrameSpec,
    /// Damage, the arms and a change of form (`roll_level` and `hop` are this step's own).
    pub mods: FlightMods,
    /// It has legs and isn't changing form: it can hold on.
    pub can_grip: bool,
    /// Its legs are there: it can walk, run and hop.
    pub legs_ok: bool,
}

/// What a step did.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveOut {
    pub flight: FlightOut,
    /// It landed, this fast into the surface, m/s.
    pub touchdown: Option<f32>,
    /// It was caught by a surface.
    pub caught: bool,
    /// It let go, or lost its grip.
    pub released: bool,
}

/// Puts an origin `stance` over the surface below `local` (body frame). Returns the origin, the
/// surface's normal there, and how high the feet were over the surface before the snap. Pure in
/// its arguments: it never reads the suit's orientation, so a replayed position is exact. The
/// ground below is along the normal of the surface nearest the origin ([`Shape::normal_from`]),
/// so an origin already placed stays where it is.
pub fn place(shape: &Shape, local: Vec3, stance: f32) -> (Vec3, Vec3, f32) {
    let mut n = shape.normal_from(local);
    let (mut o, mut h0) = (local, 0.0);
    for k in 0..SNAP_ITERS {
        let foot = o - n * stance;
        let pr = shape.probe(foot);
        if k == 0 {
            h0 = pr.dist;
        }
        n = pr.normal;
        o = foot - n * pr.dist + n * stance;
    }
    (o, n, h0)
}

/// The stance `thrust[1]` asks for from `stance`: a crouch, standing, or the stance it has (so a
/// silent client stays crouched). Without legs, a kneel.
pub fn stance_target(ty: i8, stance: f32, legs_ok: bool) -> f32 {
    if !legs_ok || ty <= CROUCH_LEVEL {
        CROUCH_STANCE
    } else if ty >= STAND_LEVEL {
        STANCE
    } else {
        stance
    }
}

/// `x` moved toward `target` by at most `step`.
fn step_toward(x: f32, target: f32, step: f32) -> f32 {
    if x < target { (x + step).min(target) } else { (x - step).max(target) }
}

/// A unit vector square to `n`.
fn any_perp(n: Vec3) -> Vec3 {
    normalize_or(n.cross(if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }), Vec3::X)
}

/// `v` along the plane square to `n`, as a unit vector.
fn tangent_of(v: Vec3, n: Vec3) -> Vec3 {
    normalize_or(v - n * v.dot(n), any_perp(n))
}

/// The surface a suit with its grip armed would be caught by now: the nearest grippable one its
/// feet are within [`CATCH_RANGE`] of, slow enough relative to it. What the HUD's landing ring
/// shows is exactly what the step will do.
pub fn catch_candidate(b: &Bodies, f: &FlightState) -> Option<Near> {
    b.nearest_grippable(f, CATCH_RANGE, CATCH_SPEED, CATCH_LEAVE)
}

/// A suit's world state `f` as an anchor on `body` (posed `pose`), riding `stance` high.
pub fn localize(pose: &BodyPose, f: &FlightState, body: Body, stance: f32) -> Anchor {
    let inv = pose.rot.conjugate();
    let spin = if pose.moving { f.ang_vel - pose.ang_vel } else { f.ang_vel };
    Anchor {
        body,
        local: pose.to_local(f.pos),
        rot: inv * f.rot,
        vel: inv * (f.vel - pose.point_vel(f.pos)),
        ang_vel: inv * spin,
        stance,
    }
}

/// A rider's world state from its anchor on a body posed `p`: where the body carries it, plus its
/// own motion over the body.
pub fn derive(p: &BodyPose, a: &Anchor, f: &mut FlightState) {
    f.pos = p.pos + p.rot * a.local;
    // No renormalisation: both are unit, and it is worked out afresh every tick.
    f.rot = p.rot * a.rot;
    f.vel = p.point_vel(f.pos) + p.rot * a.vel;
    f.ang_vel = if p.moving { p.ang_vel + p.rot * a.ang_vel } else { p.rot * a.ang_vel };
}

/// One tick of a suit under `cmd`, among the bodies: flying free, walking, or aloft in a grip, and
/// the changes between them.
pub fn move_step(b: &Bodies, m: &mut Mover, cmd: &InputCmd, cx: &MoveCtx, dt: f32) -> MoveOut {
    let mut out = MoveOut::default();
    transition(b, m, cmd, cx, &mut out);
    match m.footing {
        Footing::Free => {
            let prev = m.flight.pos;
            let mut mods = cx.mods;
            // Armed, the suit rolls its feet toward a surface it's coming in to.
            if cmd.pressed(GRIP) && cx.can_grip && cmd.roll == 0 {
                mods.roll_level = b
                    .nearest_grippable(&m.flight, LEVEL_RANGE, LEVEL_SPEED, f32::INFINITY)
                    .map(|n| n.n_world);
            }
            out.flight = flight::step_in(b.field, &mut m.flight, cmd, cx.spec, &mods, dt);
            // The landmarks are as solid as the rocks (nothing to do far from them).
            b.collide_landmarks(prev, &mut m.flight, None);
        }
        Footing::Grounded | Footing::Aloft => {
            let body = m.anchor.body;
            let (Some(pose), Some(shape)) = (b.pose(body), b.shape(body)) else {
                unreachable!("an attached suit's body is known (T1 lets go of any other)")
            };
            let prev = m.flight.pos;
            let aim_l = pose.rot.conjugate() * normalize_or(cmd.aim, m.flight.rot * Vec3::Z);
            let release = if m.footing == Footing::Grounded {
                let blackout = m.flight.blackout;
                let acc_l = grounded(&shape, &mut m.anchor, &mut m.footing, cmd, cx, blackout, aim_l, dt);
                // The legs' push is felt as thrust is (contact itself is harmless).
                let accel = pose.rot * acc_l;
                flight::pilot_g(&mut m.flight, accel, cx.mods.g_immune, dt);
                out.flight = FlightOut { boosting: false, accel, throttle: Vec3::ZERO, g_limited: false };
                false
            } else {
                let (fo, touch, rel) =
                    aloft(&shape, &mut m.anchor, &mut m.footing, &mut m.flight, cmd, cx, aim_l, dt);
                out.flight = FlightOut { accel: pose.rot * fo.accel, ..fo };
                out.touchdown = touch;
                rel
            };
            derive(&pose, &m.anchor, &mut m.flight);
            // Everything else stays solid to it: other rocks, other landmarks, the colony and the
            // sector's bounds. (Not its own body: a crouched origin sits inside a rock's collider.)
            let rock = match body {
                Body::Rock(r) => Some(usize::from(r)),
                _ => None,
            };
            let landmark = match body {
                Body::Landmark(k) => Some(k),
                _ => None,
            };
            let moved = b.field.collide_except(prev, &mut m.flight, rock)
                | b.collide_landmarks(prev, &mut m.flight, landmark)
                | crate::world::constrain(&mut m.flight);
            if moved {
                // Back into the body's frame where it was stopped, so the world pose is still its
                // anchor's.
                m.anchor.local = pose.to_local(m.flight.pos);
                m.anchor.vel = pose.rot.conjugate() * (m.flight.vel - pose.point_vel(m.flight.pos));
                derive(&pose, &m.anchor, &mut m.flight);
            }
            if release {
                m.anchor = Anchor::default();
                m.footing = Footing::Free;
                out.released = true;
            }
            debug_assert!(m.footing == Footing::Free || b.alive(m.anchor.body), "I2");
            debug_assert!(
                m.footing == Footing::Free || {
                    let q = m.anchor.stance * 16.0;
                    q == crate::math::floor(q) && (96.0..=146.0).contains(&q)
                },
                "I7"
            );
        }
    }
    out
}

/// The changes decided at the start of a step, from what the suit was and what it's asked: one at
/// most, in priority order. A suit that lets go keeps the world state derived last tick (the
/// surface's velocity where it was, plus its own), plus the push.
fn transition(b: &Bodies, m: &mut Mover, cmd: &InputCmd, cx: &MoveCtx, out: &mut MoveOut) {
    match m.footing {
        Footing::Free => {
            // T7: armed, coming in slow and close, not boosting nor climbing away.
            if cmd.pressed(GRIP)
                && cx.can_grip
                && !cmd.pressed(BOOST)
                && cmd.thrust[1] <= 0
                && let Some(c) = catch_candidate(b, &m.flight)
            {
                m.anchor = localize(&c.pose, &m.flight, c.body, STANCE);
                m.footing = Footing::Aloft;
                out.caught = true;
            }
        }
        Footing::Grounded | Footing::Aloft => {
            let grounded = m.footing == Footing::Grounded;
            let push = if grounded { TAKEOFF_SPEED } else { 0.0 };
            if !b.alive(m.anchor.body) {
                // T1: its rock shattered last tick.
                let_go(b, m, UNPARK_SPEED, out);
            } else if !cx.can_grip || !cmd.pressed(GRIP) {
                // T2 (changing form), T3 (let go): pushing off, from the ground.
                let_go(b, m, push, out);
            } else if grounded && cmd.thrust[1] >= JUMP_LEVEL {
                if cx.legs_ok && m.anchor.stance == STANCE && !m.flight.blackout {
                    // T6: a hop.
                    let Some(shape) = b.shape(m.anchor.body) else { return };
                    let (_, n, _) = place(&shape, m.anchor.local, m.anchor.stance);
                    m.anchor.vel += n * JUMP_SPEED;
                    m.footing = Footing::Aloft;
                } else if !cx.legs_ok {
                    // T6b: no legs to hop with, but the thrusters lift.
                    m.footing = Footing::Aloft;
                }
            }
        }
    }
}

/// Lets go of the body, pushing off along its normal at `push` m/s.
fn let_go(b: &Bodies, m: &mut Mover, push: f32, out: &mut MoveOut) {
    if push != 0.0
        && let (Some(pose), Some(shape)) = (b.pose(m.anchor.body), b.shape(m.anchor.body))
    {
        let (_, n, _) = place(&shape, m.anchor.local, m.anchor.stance);
        m.flight.vel += (pose.rot * n) * push;
    }
    m.anchor = Anchor::default();
    m.footing = Footing::Free;
    out.released = true;
}

/// A tick on the ground, in the body's frame: the stance, the walk, walls, and the attitude.
/// Returns the legs' acceleration (body frame).
#[allow(clippy::too_many_arguments)]
fn grounded(
    shape: &Shape,
    a: &mut Anchor,
    footing: &mut Footing,
    cmd: &InputCmd,
    cx: &MoveCtx,
    blackout: bool,
    aim_l: Vec3,
    dt: f32,
) -> Vec3 {
    let was = a.stance;
    a.stance = step_toward(a.stance, stance_target(cmd.thrust[1], a.stance, cx.legs_ok), STANCE_STEP);
    let (mut o0, mut n0, _) = place(shape, a.local, a.stance);
    if a.stance > was && !placed(shape, o0, a.stance) {
        // No room to stand up here (a wall or an overhang is nearer than the stance would be).
        a.stance = was;
        (o0, n0, _) = place(shape, a.local, a.stance);
    }
    // Forward is the aim along the ground (or the way the suit faces, aiming straight up or down);
    // right is up × forward, as the suit's own axes are.
    let fwd_t = normalize_or(aim_l - n0 * aim_l.dot(n0), tangent_of(a.rot * Vec3::Z, n0));
    let right_t = n0.cross(fwd_t);
    let authority = if blackout { 0.25 } else { 1.0 };
    let speed = if !cx.legs_ok {
        0.0
    } else if a.stance < STANCE {
        CROUCH_SPEED
    } else if cmd.pressed(BOOST) {
        RUN_SPEED
    } else {
        WALK_SPEED
    };
    let stick = clamp_len(Vec3::new(f32::from(cmd.thrust[0]), 0.0, f32::from(cmd.thrust[2])) / 127.0, 1.0);
    let mut target = (right_t * stick.x + fwd_t * stick.z) * (speed * authority);
    let mut cap = GROUND_ACCEL;
    if cx.mods.lunge && cx.legs_ok {
        target = fwd_t * LUNGE_GROUND_SPEED;
        cap = LUNGE_ACCEL;
    }
    if cmd.pressed(BRAKE) {
        target = Vec3::ZERO;
        cap = BRAKE_ACCEL;
    }
    // Nothing along the normal on the ground.
    let v_t = a.vel - n0 * a.vel.dot(n0);
    let dv = clamp_len(target - v_t, cap * dt);
    a.vel = v_t + dv;
    let acc_l = dv / dt;
    let n_now = match step_on(shape, o0, n0, a.vel, a.stance, dt) {
        Stride::On(o1, n1) => {
            a.local = o1;
            a.vel -= n1 * a.vel.dot(n1);
            n1
        }
        Stride::Off(to, n1) => {
            // E2: the ground fell away.
            a.local = to;
            *footing = Footing::Aloft;
            n1
        }
        Stride::Wall(n_block) => {
            // A wall: lose only the speed into it, and go on along it if the rest can.
            let w = normalize_or(n_block - n0 * n_block.dot(n0), -a.vel);
            a.vel -= w * a.vel.dot(w).min(0.0);
            match step_on(shape, o0, n0, a.vel, a.stance, dt) {
                Stride::On(o1, n1) => {
                    a.local = o1;
                    a.vel -= n1 * a.vel.dot(n1);
                    n1
                }
                _ => {
                    a.local = o0;
                    n0
                }
            }
        }
    };
    let rate = if cx.legs_ok { GROUND_TURN_RATE } else { LEGLESS_TURN_RATE };
    surface_attitude(a, aim_l, n_now, rate, dt);
    acc_l
}

/// Where a step on the ground ends.
enum Stride {
    /// On the ground: the origin placed there, and the normal.
    On(Vec3, Vec3),
    /// Off it, the ground falling away faster than the stance can follow: where the origin went,
    /// and the normal below it.
    Off(Vec3, Vec3),
    /// Against a wall, whose normal this is: an inner corner, an edge too sharp to walk round
    /// ([`MAX_STEP_TURN`]), or a surface closer to the origin than [`BODY_CLEAR`].
    Wall(Vec3),
}

/// A step at `vel` for `dt` from `o0` (placed, on ground with normal `n0`).
fn step_on(shape: &Shape, o0: Vec3, n0: Vec3, vel: Vec3, stance: f32, dt: f32) -> Stride {
    let to = o0 + vel * dt;
    let (o1, n1, h0) = place(shape, to, stance);
    if h0 > STEP_DOWN {
        return Stride::Off(to, n1);
    }
    if angle_between(n0, n1) > MAX_STEP_TURN {
        return Stride::Wall(n1);
    }
    // Nearer than the stance to another surface of the body, the origin would be over that one:
    // it's a wall too (an inner corner), and the suit stays where the ground puts it.
    let clear = shape.probe(o1);
    if clear.dist < BODY_CLEAR || !placed(shape, o1, stance) {
        Stride::Wall(clear.normal)
    } else {
        Stride::On(o1, n1)
    }
}

/// Settles an origin `o` just placed (normal `n`) where the suit can stand: in an inner corner,
/// where it's nearer than its stance to another surface of the body, placed again from there (onto
/// that one), up to twice more. What it lands on, or is set down on, is then ground it stays on.
pub fn settle(shape: &Shape, o: Vec3, n: Vec3, stance: f32) -> (Vec3, Vec3) {
    let (mut o, mut n) = (o, n);
    for _ in 0..2 {
        if placed(shape, o, stance) {
            break;
        }
        (o, n, _) = place(shape, o, stance);
    }
    (o, n)
}

/// Whether origin `o` is where [`place`] puts it: its feet within [`PLACE_TOL`] of the ground
/// below the surface nearest it.
fn placed(shape: &Shape, o: Vec3, stance: f32) -> bool {
    shape.probe(o - shape.normal_from(o) * stance).dist.abs() <= PLACE_TOL
}

/// Stands a suit up to the surface's normal `n`, facing its aim and leaning (at most
/// [`GROUND_LEAN_SIN`]) toward it, turning no faster than `rate`.
fn surface_attitude(a: &mut Anchor, aim_l: Vec3, n: Vec3, rate: f32, dt: f32) {
    let s = aim_l.dot(n).clamp(-GROUND_LEAN_SIN, GROUND_LEAN_SIN);
    let h = normalize_or(aim_l - n * aim_l.dot(n), tangent_of(a.rot * Vec3::Z, n));
    let fwd = h * sqrt(1.0 - s * s) + n * s;
    let new = quat_rotate_toward(a.rot, look_rotation(fwd, n), rate * dt);
    a.ang_vel = rotation_vector(new * a.rot.conjugate()) / dt;
    a.rot = new;
}

/// A tick aloft, in the body's frame: flight under grip gravity with the hop's flight assist, the
/// free descent brake, then a touchdown (E1) or a lost grip (T5). Returns what the flight did (body
/// frame), the touchdown's speed into the surface, and whether the grip is lost.
#[allow(clippy::too_many_arguments)]
fn aloft(
    shape: &Shape,
    a: &mut Anchor,
    footing: &mut Footing,
    f: &mut FlightState,
    cmd: &InputCmd,
    cx: &MoveCtx,
    aim_l: Vec3,
    dt: f32,
) -> (FlightOut, Option<f32>, bool) {
    // Down is toward the nearest surface, whichever way that is.
    let (_, n, _) = place(shape, a.local, a.stance);
    let mut l = FlightState {
        pos: a.local,
        vel: a.vel - n * (GRIP_ACCEL * dt),
        rot: a.rot,
        ang_vel: a.ang_vel,
        propellant: f.propellant,
        g_load: f.g_load,
        g_strain: f.g_strain,
        blackout: f.blackout,
    };
    let cmd_l = InputCmd { aim: aim_l, ..*cmd };
    let cruise = if cmd.pressed(BOOST) { RUN_SPEED } else { WALK_SPEED };
    let mods = FlightMods {
        roll_level: (cmd.roll == 0).then_some(n),
        hop: Some(HopAssist { up: l.rot.conjugate() * n, cruise, climb: HOP_CLIMB }),
        ..cx.mods
    };
    let out = flight::integrate(&mut l, &cmd_l, cx.spec, &mods, dt);
    let vn = l.vel.dot(n);
    if vn < -LAND_SPEED_MAX {
        l.vel -= n * (vn + LAND_SPEED_MAX);
    }
    (a.local, a.vel, a.rot, a.ang_vel) = (l.pos, l.vel, l.rot, l.ang_vel);
    (f.propellant, f.g_load, f.g_strain, f.blackout) = (l.propellant, l.g_load, l.g_strain, l.blackout);
    let (o1, n1, h) = place(shape, a.local, a.stance);
    let into = a.vel.dot(n1);
    if h <= 0.0 && into <= 0.0 {
        // E1: down, feet first. The legs take the landing, and any speed along the ground past a
        // lunge's.
        let (o, n) = settle(shape, o1, n1, a.stance);
        a.local = o;
        a.vel = clamp_len(a.vel - n * a.vel.dot(n), LUNGE_GROUND_SPEED);
        *footing = Footing::Grounded;
        (out, Some(-into), false)
    } else {
        // T5: too high, or too fast.
        (out, None, h > RELEASE_RANGE || length(a.vel) > RELEASE_SPEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::landmarks::LANDMARKS;
    use crate::field::Field;
    use crate::math::Rng;

    fn rock_shape(axes: Vec3) -> Shape {
        Shape::ellipsoid(axes)
    }

    /// Foot height after a snap from `p` (outside), on `shape`.
    fn settled(shape: &Shape, p: Vec3, stance: f32) -> f32 {
        let (o, _, _) = place(shape, p, stance);
        place(shape, o, stance).2
    }

    #[test]
    fn place_converges() {
        let mut rng = Rng::new(3);
        let mut worst: f32 = 0.0;
        // Rocks, axes 10 to 100 m, up to 3.05 to 1.
        for _ in 0..400 {
            let min = 10.0 + rng.next_f32() * 20.0;
            let axes = Vec3::new(min, min * (1.0 + 2.05 * rng.next_f32()), min * (1.0 + rng.next_f32()));
            let axes = axes * (1.0 + 2.3 * rng.next_f32()).min(100.0 / axes.max_element());
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            let shape = rock_shape(axes);
            // A point of the surface (the ray's exit), and a step's worth off the stance over it.
            let on = dir / length(dir / axes);
            let n = normalize_or(on / (axes * axes), Vec3::Y);
            for stance in [STANCE, CROUCH_STANCE] {
                let off = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 1.2;
                let p = on + n * stance + off;
                worst = worst.max(settled(&shape, p, stance).abs());
            }
        }
        assert!(worst < 1e-2, "rocks: feet {worst} m off the surface");
        // Hermit, bowls and all, and MO-II's faces, edges and Aft Well.
        let hermit = LANDMARKS[1].shape;
        let mo_ii = LANDMARKS[0].shape;
        let hermit_spots =
            [Vec3::new(0.0, 598.0, 0.0), Vec3::new(20.0, 600.0, 10.0), Vec3::new(0.0, 0.0, -740.0)];
        let mo_ii_spots = [
            Vec3::new(-100.0, 70.0, 0.0),  // core top
            Vec3::new(-100.0, 50.0, 40.0), // core, 40° round
            Vec3::new(0.0, 95.0, 0.0),     // pylon top
            Vec3::new(37.0, 95.0, 20.0),   // pylon, over the rounded edge
            Vec3::new(-270.0, 70.0, 0.0),  // aft module, -X face
            Vec3::new(-250.0, 90.0, 85.0), // aft module's rounded corner
            Vec3::new(-245.0, 0.0, 0.0),   // the Aft Well's floor
            Vec3::new(-252.0, 0.0, 30.0),  // its wall
            Vec3::new(300.0, 12.0, 0.0),   // the mast
        ];
        let mut worst: f32 = 0.0;
        for (shape, spots) in [(hermit, &hermit_spots[..]), (mo_ii, &mo_ii_spots[..])] {
            for &p in spots {
                for stance in [STANCE, CROUCH_STANCE] {
                    worst = worst.max(settled(&shape, p, stance).abs());
                }
            }
        }
        assert!(worst < 1e-2, "landmarks: feet {worst} m off the surface");
        // Hermit's surface all round.
        let mut worst: f32 = 0.0;
        for _ in 0..2_000 {
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            worst = worst.max(settled(&hermit, dir * 960.0, STANCE).abs());
        }
        assert!(worst < 1e-2, "Hermit: feet {worst} m off the surface");
    }

    #[test]
    fn a_placed_origin_stays_put() {
        // Standing still, a suit is placed again every tick: it mustn't creep, on the most
        // lopsided rocks (3.05 to 1, tips and all), on Hermit or on MO-II.
        let mut rng = Rng::new(5);
        let mut worst: f32 = 0.0;
        let mut check = |shape: &Shape, p: Vec3, stance: f32| {
            let (start, _, _) = place(shape, p, stance);
            let mut o = start;
            for _ in 0..300 {
                o = place(shape, o, stance).0;
            }
            worst = worst.max(o.distance(start));
        };
        for _ in 0..200 {
            let min = 10.0 + rng.next_f32() * 20.0;
            let axes = Vec3::new(min, min * 3.05, min * (1.0 + 2.05 * rng.next_f32()));
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed() * 0.2, rng.signed()), Vec3::Y);
            let on = dir / length(dir / axes);
            let n = normalize_or(on / (axes * axes), Vec3::Y);
            for stance in [STANCE, CROUCH_STANCE] {
                check(&rock_shape(axes), on + n * stance, stance);
            }
        }
        for _ in 0..200 {
            let dir = normalize_or(Vec3::new(rng.signed(), rng.signed(), rng.signed()), Vec3::Y);
            check(&LANDMARKS[1].shape, dir * 960.0, STANCE);
        }
        for p in [Vec3::new(-100.0, 70.0, 0.0), Vec3::new(-100.0, 50.0, 40.0), Vec3::new(37.0, 95.0, 20.0)] {
            check(&LANDMARKS[0].shape, p, STANCE);
        }
        assert!(worst < 1e-3, "a still suit crept {worst} m in 10 s");
    }

    #[test]
    fn landing_in_an_inner_corner_settles_clear_of_both_walls() {
        // Where MO-II's +Z pylon rises from its core: set down on the one, an origin can be nearer
        // than its stance to the other. It settles where neither is.
        let mo_ii = LANDMARKS[0].shape;
        let mut rng = Rng::new(8);
        let mut cornered = 0;
        for _ in 0..500 {
            let p = Vec3::new(
                -45.0 + 10.0 * rng.next_f32(),
                25.0 + 12.0 * rng.next_f32(),
                52.0 + 12.0 * rng.next_f32(),
            );
            for stance in [STANCE, CROUCH_STANCE] {
                let (o, n, _) = place(&mo_ii, p, stance);
                cornered += usize::from(!placed(&mo_ii, o, stance));
                let (o, _) = settle(&mo_ii, o, n, stance);
                assert!(placed(&mo_ii, o, stance), "{p} didn't settle");
                assert!(mo_ii.probe(o).dist >= stance - PLACE_TOL, "{p} settled in the corner");
            }
        }
        assert!(cornered > 20, "only {cornered} origins were cornered: the test tests nothing");
    }

    #[test]
    fn stance_stays_on_the_grid_and_is_sticky() {
        let mut rng = Rng::new(11);
        let mut stance = STANCE;
        for k in 0..10_000 {
            let ty = if k % 7 == 0 { 0 } else { (rng.next_u32() & 0xff) as u8 as i8 };
            let legs = k % 97 != 0;
            let before = stance;
            stance = step_toward(stance, stance_target(ty, stance, legs), STANCE_STEP);
            let q = stance * 16.0;
            assert!(q == crate::math::floor(q) && (96.0..=146.0).contains(&q), "{stance} is off the grid");
            if ty == 0 && legs {
                assert_eq!(stance, before, "no stick keeps the stance");
            }
        }
        // Crouched to standing in about 0.55 s, and back.
        let mut s = STANCE;
        let mut n = 0;
        while s > CROUCH_STANCE {
            s = step_toward(s, stance_target(-127, s, true), STANCE_STEP);
            n += 1;
        }
        assert_eq!(n, 17);
    }

    #[test]
    fn derive_and_localize_undo_each_other() {
        let d = &LANDMARKS[0];
        let pose = crate::bodies::landmark_pose(d, 12_345, 0.0);
        let f = FlightState {
            pos: pose.to_world(Vec3::new(-30.0, 75.0, 10.0)),
            vel: Vec3::new(3.0, -1.0, 2.0),
            rot: look_rotation(Vec3::new(1.0, 0.2, 0.3), Vec3::Y),
            ang_vel: Vec3::new(0.1, 0.0, -0.2),
            ..FlightState::default()
        };
        let a = localize(&pose, &f, Body::Landmark(0), STANCE);
        let mut g = f;
        derive(&pose, &a, &mut g);
        assert!(g.pos.distance(f.pos) < 1e-3 && g.vel.distance(f.vel) < 1e-4, "{g:?} vs {f:?}");
        assert!(g.rot.dot(f.rot).abs() > 1.0 - 1e-6 && g.ang_vel.distance(f.ang_vel) < 1e-5);
    }

    /// A Mover standing on the top of a rock of these axes, facing +Z, and the rock's bodies.
    fn standing(field: &Field) -> Mover {
        let rock = field.rocks()[0];
        let shape = Shape::ellipsoid(rock.axes);
        let (o, n, _) = place(&shape, Vec3::Y * (rock.axes.y + STANCE), STANCE);
        let anchor = Anchor {
            body: Body::Rock(0),
            local: o,
            rot: look_rotation(tangent_of(Vec3::Z, n), n),
            stance: STANCE,
            ..Anchor::default()
        };
        let mut flight = FlightState { propellant: 1_000.0, ..FlightState::default() };
        derive(&BodyPose::fixed(rock.pos, rock.rot), &anchor, &mut flight);
        Mover { flight, footing: Footing::Grounded, anchor }
    }

    #[test]
    fn the_ground_falling_away_puts_a_suit_aloft() {
        // No surface at ground speed falls away faster than a step can follow it (a probe's
        // distance changes no faster than the point moves), so only a shove far past any walk's
        // speed can leave the ground behind: the step then carries on aloft.
        let rock = crate::field::Rock {
            pos: Vec3::new(0.0, 900.0, 0.0),
            radius: 12.0,
            axes: Vec3::splat(12.0),
            ..crate::field::Rock::default()
        };
        let field = Field::from_rocks(&[rock]);
        let bodies = Bodies::at(&field, &[], 1);
        let mut m = standing(&field);
        m.anchor.vel = Vec3::Z * 300.0;
        let spec = crate::content::frame(bc_proto::FrameId::Leo);
        let cx = MoveCtx { spec, mods: FlightMods::default(), can_grip: true, legs_ok: true };
        let cmd = InputCmd { buttons: GRIP, ..InputCmd::default() };
        move_step(&bodies, &mut m, &cmd, &cx, crate::config::DT);
        assert_eq!(m.footing, Footing::Aloft, "10 m past the top of a 12 m rock, the ground is gone");
        assert_eq!(m.anchor.body, Body::Rock(0), "and the grip holds on");
    }
}
