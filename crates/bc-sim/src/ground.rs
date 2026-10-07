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
//! surface it left. A suit that never arms it flies exactly as [`flight::step_in`] flies it (inside
//! the colony, as `colony::interior::step` does).
//!
//! The colony's city ([`Body::City`]) has a down, the spin's: in its grip a suit falls that way,
//! under the colony's pull rather than the grip's, and stands only on ground that faces up
//! ([`CITY_FOOTING_COS`]). A wall is never stood on: it stops a suit walking into it and keeps one
//! in the air off it, and a roof's edge is stepped off, not walked round.
//!
//! [`move_step`] is the one step: the server runs it for every live suit, and the owner's client
//! runs it to predict its own. Every transition is decided by the step's own inputs, so both sides
//! catch, land and let go on the same tick.

use bc_proto::InputCmd;
use bc_proto::buttons::{BOOST, BRAKE, GRIP};
use glam::{Quat, Vec3};

use crate::bodies::{Base, Bodies, Body, BodyPose, Near, Shape};
use crate::colony::frame::{gravity, up_at};
use crate::colony::hub::{BAY_LAUNCH_SPEED, BAY_RIDE_LOCAL, bay_g, bay_ride_rot};
use crate::content::FrameSpec;
use crate::flight::{self, FA_BOOST_CRUISE, FlightMods, FlightOut, FlightState, HopAssist, LockOnAssist};
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
/// On the city, ground whose normal is further than this from the spin's up (cos 30°) isn't
/// stood on: walked into, it's a wall; walked out over (a roof's edge), the ground fell away; come
/// down on, it keeps the suit off and doesn't take it.
pub const CITY_FOOTING_COS: f32 = 0.866_025_4;
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

/// Whether `shape` is the colony's city, which has a down.
fn is_city(shape: &Shape) -> bool {
    matches!(shape.base, Base::City)
}

/// On the city: up at `p` (the spin's) and how hard the colony pulls there, m/s².
fn city_down(p: Vec3) -> (Vec3, f32) {
    let r = sqrt(p.y * p.y + p.z * p.z);
    (up_at(p), gravity(crate::world::COLONY_RADIUS - r))
}

/// Whether ground with normal `n` at `o` can be stood on: anywhere but on the city, where it must
/// face up ([`CITY_FOOTING_COS`]).
fn standable(shape: &Shape, o: Vec3, n: Vec3) -> bool {
    !is_city(shape) || n.dot(up_at(o)) >= CITY_FOOTING_COS
}

/// A unit vector square to `n`.
fn any_perp(n: Vec3) -> Vec3 {
    normalize_or(n.cross(if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }), Vec3::X)
}

/// `v` along the plane square to `n`, as a unit vector.
fn tangent_of(v: Vec3, n: Vec3) -> Vec3 {
    normalize_or(v - n * v.dot(n), any_perp(n))
}

/// What flight assist flies by when `cmd` is locked on to a target (`bc_proto::LockOn`): the
/// fight's axes, levelled to its up, forward the aim laid flat on its ground (else the nose), and
/// the target's velocity, capped at the frame's boosted cruise so a lock can't pace anything for
/// free. Pure in the command and the suit's own state, so the server and the owner's prediction
/// agree to the bit; a reference that isn't a number (an agent's own, unquantized) is no reference.
pub fn lockon_assist(cmd: &InputCmd, f: &FlightState, spec: &FrameSpec) -> Option<LockOnAssist> {
    let l = cmd.lockon?;
    let up = normalize_or(l.up, Vec3::Y);
    let nose = f.rot * Vec3::Z;
    let aim = normalize_or(cmd.aim, nose);
    let fwd = normalize_or(aim - up * aim.dot(up), tangent_of(nose, up));
    let r = if l.ref_vel.is_finite() { l.ref_vel } else { Vec3::ZERO };
    Some(LockOnAssist {
        ref_vel: clamp_len(r, spec.fa_speed * FA_BOOST_CRUISE),
        right: up.cross(fwd),
        up,
        fwd,
    })
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
            if b.interior {
                // Inside the colony: its pull, its air, its hull and its city.
                out.flight = crate::colony::interior::step(&mut m.flight, cmd, cx.spec, &mods, dt);
            } else {
                // Locked on: flight assist flies by the fight's axes, and the suit rolls level
                // with them, unless the pilot rolls or the grip is bringing it down onto a surface.
                mods.lockon = lockon_assist(cmd, &m.flight, cx.spec);
                if let Some(l) = mods.lockon
                    && mods.roll_level.is_none()
                    && cmd.roll == 0
                {
                    mods.roll_level = Some(l.up);
                }
                out.flight = flight::step_in(b.field, &mut m.flight, cmd, cx.spec, &mods, dt);
                // The landmarks are as solid as the rocks (nothing to do far from them), and as
                // hard to hit.
                let v = m.flight.vel;
                b.collide_landmarks(prev, &mut m.flight, None);
                flight::crash(&mut m.flight, v, &mods);
            }
        }
        Footing::Grounded | Footing::Aloft => {
            let body = m.anchor.body;
            let (Some(pose), Some(shape)) = (b.pose(body), b.shape(body)) else {
                unreachable!("an attached suit's body is known (T1 lets go of any other)")
            };
            let prev = m.flight.pos;
            let aim_l = pose.rot.conjugate() * normalize_or(cmd.aim, m.flight.rot * Vec3::Z);
            let in_bay = matches!(body, Body::Bay(_));
            let release = if in_bay {
                // Held in the cradle, standing still in the door: the pilot feels the deck
                // holding the suit toward the axis against the spin.
                ride_bay(&mut m.anchor, &mut m.flight, cx, dt);
                out.flight = FlightOut::default();
                false
            } else if m.footing == Footing::Grounded {
                let blackout = m.flight.blackout;
                let (acc_l, n) =
                    grounded(&shape, &mut m.anchor, &mut m.footing, cmd, cx, blackout, aim_l, dt);
                // The legs' push is felt as thrust is, and so is the ground holding the suit up
                // against what it would fall under there: in the city, its pilot feels its g.
                let held = match m.footing {
                    Footing::Grounded if is_city(&shape) => {
                        let (up, g) = city_down(m.anchor.local);
                        up * g
                    }
                    Footing::Grounded => n * GRIP_ACCEL,
                    _ => Vec3::ZERO,
                };
                flight::pilot_g(&mut m.flight, m.anchor.rot.conjugate() * (acc_l + held), &cx.mods, dt);
                let accel = pose.rot * acc_l;
                // Legs burn nothing, so under anime rules the boost gauge fills here too (an ion
                // drive working at it).
                let before = m.flight.propellant;
                flight::refill(&mut m.flight, cx.spec, &cx.mods, dt);
                let ion = if cx.mods.ion > 0.0 && m.flight.propellant > before { 1.0 } else { 0.0 };
                out.flight =
                    FlightOut { boosting: false, accel, throttle: Vec3::ZERO, g_limited: false, ion };
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
            // Inside the colony the city is all there is, and it's the body.
            let rock = match body {
                Body::Rock(r) => Some(usize::from(r)),
                _ => None,
            };
            let landmark = match body {
                Body::Landmark(k) => Some(k),
                _ => None,
            };
            let moved = !b.interior
                && !in_bay
                && (b.field.collide_except(prev, &mut m.flight, rock)
                    | b.collide_landmarks(prev, &mut m.flight, landmark)
                    | crate::world::constrain(&mut m.flight));
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
        // In its bay's cradle: only its pilot letting go of the grip lets it go, thrown out of the
        // door (nothing else it's asked, legs or none, changes that: the bay's law clears the rest).
        Footing::Grounded | Footing::Aloft if matches!(m.anchor.body, Body::Bay(_)) => {
            if !b.alive(m.anchor.body) {
                let_go(b, m, UNPARK_SPEED, out);
            } else if !cmd.pressed(GRIP) {
                let_go(b, m, BAY_LAUNCH_SPEED, out);
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

/// Lets go of the body, pushing off along its normal at `push` m/s (out of a bay, out of its
/// door).
fn let_go(b: &Bodies, m: &mut Mover, push: f32, out: &mut MoveOut) {
    if push != 0.0
        && let (Some(pose), Some(shape)) = (b.pose(m.anchor.body), b.shape(m.anchor.body))
    {
        let n = match m.anchor.body {
            Body::Bay(_) => -Vec3::X,
            _ => place(&shape, m.anchor.local, m.anchor.stance).1,
        };
        m.flight.vel += (pose.rot * n) * push;
    }
    m.anchor = Anchor::default();
    m.footing = Footing::Free;
    out.released = true;
}

/// A tick in a bay's cradle: the suit stands still in the door as the bay carries it round, its
/// pilot held toward the axis by the deck at the spin's 0.7 g, the boost gauge filling as it does
/// on any ground.
fn ride_bay(a: &mut Anchor, f: &mut FlightState, cx: &MoveCtx, dt: f32) {
    *a = Anchor {
        body: a.body,
        local: BAY_RIDE_LOCAL,
        rot: bay_ride_rot(),
        stance: STANCE,
        ..Anchor::default()
    };
    flight::pilot_g(f, a.rot.conjugate() * (-Vec3::Y * bay_g()), &cx.mods, dt);
    flight::refill(f, cx.spec, &cx.mods, dt);
}

/// A tick on the ground, in the body's frame: the stance, the walk, walls, and the attitude.
/// Returns the legs' acceleration and the normal of the ground it ends on (body frame).
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
) -> (Vec3, Vec3) {
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
                    // Blocked both ways (a rim turning away too sharply, an inner corner): it
                    // doesn't move, so it has no speed over the ground to keep.
                    a.local = o0;
                    a.vel = Vec3::ZERO;
                    n0
                }
            }
        }
    };
    let rate = if cx.legs_ok { GROUND_TURN_RATE } else { LEGLESS_TURN_RATE };
    surface_attitude(a, aim_l, n_now, rate, dt);
    (acc_l, n_now)
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
    // The city has a down: ground leaning further from it than a suit stands on is a wall walked
    // into or, walked out over (a roof's edge), ground that fell away.
    if !standable(shape, o1, n1) {
        return if n1.dot(vel) < 0.0 { Stride::Wall(n1) } else { Stride::Off(to, n1) };
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
/// frame), the touchdown's speed into the surface, and whether the grip is lost. Over the city,
/// down is the spin's and the pull is the colony's, and what can't be stood on is a wall that
/// keeps the suit off it.
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
    // Down is toward the nearest surface, whichever way that is; over the city, the spin's.
    let city = is_city(shape);
    let (n, pull) = if city { city_down(a.local) } else { (place(shape, a.local, a.stance).1, GRIP_ACCEL) };
    let mut l = FlightState {
        pos: a.local,
        vel: a.vel - n * (pull * dt),
        rot: a.rot,
        ang_vel: a.ang_vel,
        propellant: f.propellant,
        g_load: f.g_load,
        g_strain: f.g_strain,
        blackout: f.blackout,
        burst: f.burst,
    };
    let cmd_l = InputCmd { aim: aim_l, ..*cmd };
    let cruise = if cmd.pressed(BOOST) { RUN_SPEED } else { WALK_SPEED };
    let mods = FlightMods {
        roll_level: (cmd.roll == 0).then_some(n),
        hop: Some(HopAssist { up: l.rot.conjugate() * n, cruise, climb: HOP_CLIMB }),
        // The pull is the one above (the grip's, or the colony's own over its city), not again.
        interior: false,
        ..cx.mods
    };
    let out = flight::integrate(&mut l, &cmd_l, cx.spec, &mods, dt);
    let vn = l.vel.dot(n);
    if vn < -LAND_SPEED_MAX {
        l.vel -= n * (vn + LAND_SPEED_MAX);
    }
    (a.local, a.vel, a.rot, a.ang_vel) = (l.pos, l.vel, l.rot, l.ang_vel);
    (f.propellant, f.g_load, f.g_strain, f.blackout) = (l.propellant, l.g_load, l.g_strain, l.blackout);
    f.burst = l.burst;
    let (o1, n1, h) = place(shape, a.local, a.stance);
    let into = a.vel.dot(n1);
    if h <= 0.0 && into <= 0.0 {
        // E1: down, feet first. The legs take the landing, and any speed along the ground past a
        // lunge's.
        let (o, n) = settle(shape, o1, n1, a.stance);
        if !standable(shape, o1, n1) || !standable(shape, o, n) {
            // A wall of the city (or nowhere it fits to stand, as a lane narrower than a suit):
            // it keeps the suit off, and takes the speed into it.
            a.local = o1;
            a.vel -= n1 * into;
            return (out, None, false);
        }
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

    /// Standing still on a rock, a pilot feels the ground holding them up against the grip's pull
    /// (0.61 g, headward); walking off, the legs' push comes on top.
    #[test]
    fn standing_a_pilot_feels_the_ground_hold_them_up() {
        let rock = crate::field::Rock {
            pos: Vec3::new(0.0, 900.0, 0.0),
            radius: 40.0,
            axes: Vec3::splat(40.0),
            ..crate::field::Rock::default()
        };
        let field = Field::from_rocks(&[rock]);
        let bodies = Bodies::at(&field, &[], 1);
        let mut m = standing(&field);
        let spec = crate::content::frame(bc_proto::FrameId::Leo);
        let cx = MoveCtx { spec, mods: FlightMods::default(), can_grip: true, legs_ok: true };
        let stand = InputCmd { aim: Vec3::Z, buttons: GRIP, ..InputCmd::default() };
        for _ in 0..10 {
            move_step(&bodies, &mut m, &stand, &cx, crate::config::DT);
        }
        assert_eq!(m.footing, Footing::Grounded);
        let held = GRIP_ACCEL / crate::config::G0;
        assert!((m.flight.g_load - held).abs() < 1e-3, "{} g, want {held}", m.flight.g_load);
        let walk = InputCmd { thrust: [0, 0, 127], ..stand };
        move_step(&bodies, &mut m, &walk, &cx, crate::config::DT);
        assert!(m.flight.g_load > held + 0.5, "{} g", m.flight.g_load);
        assert_eq!(m.flight.g_strain, 0.0);
    }

    #[test]
    fn a_lockon_flies_by_the_fights_axes_capped_and_sane() {
        let spec = crate::content::frame(bc_proto::FrameId::Leo);
        let f = FlightState::default();
        let lock = |ref_vel: Vec3, up: Vec3, aim: Vec3| {
            let cmd = InputCmd { aim, lockon: Some(bc_proto::LockOn { ref_vel, up }), ..InputCmd::default() };
            lockon_assist(&cmd, &f, spec).unwrap()
        };
        // Forward is the aim laid flat on the fight's ground; the axes are right-handed and square.
        let l = lock(Vec3::X * 100.0, Vec3::Y, Vec3::new(0.0, 0.6, 0.8));
        assert!(l.fwd.distance(Vec3::Z) < 1e-6 && l.right.distance(Vec3::X) < 1e-6, "{l:?}");
        assert!(l.ref_vel == Vec3::X * 100.0);
        let l = lock(Vec3::ZERO, Vec3::new(1.0, 2.0, -0.5).normalize(), Vec3::new(0.3, -0.2, 0.9));
        assert!(l.up.dot(l.fwd).abs() < 1e-6 && l.up.dot(l.right).abs() < 1e-6);
        assert!((l.right.cross(l.up) - l.fwd).length() < 1e-5);
        // Aiming straight along the up: forward falls back to the nose laid flat.
        let l = lock(Vec3::ZERO, Vec3::X, Vec3::X);
        assert!(l.fwd.distance(Vec3::Z) < 1e-6, "{l:?}");
        // No faster than the frame's boosted cruise, and nothing that isn't a number.
        let l = lock(Vec3::X * 5_000.0, Vec3::Y, Vec3::Z);
        assert!((l.ref_vel.length() - spec.fa_speed * FA_BOOST_CRUISE).abs() < 1e-3);
        assert_eq!(lock(Vec3::new(f32::NAN, 1.0, 0.0), Vec3::Y, Vec3::Z).ref_vel, Vec3::ZERO);
        assert!(lockon_assist(&InputCmd::default(), &f, spec).is_none());
    }

    #[test]
    fn a_lockon_flies_free_suits_only() {
        let rock = crate::field::Rock {
            pos: Vec3::new(0.0, 900.0, 0.0),
            radius: 12.0,
            axes: Vec3::splat(12.0),
            ..crate::field::Rock::default()
        };
        let field = Field::from_rocks(&[rock]);
        let bodies = Bodies::at(&field, &[], 1);
        let spec = crate::content::frame(bc_proto::FrameId::Leo);
        let cx = MoveCtx { spec, mods: FlightMods::default(), can_grip: true, legs_ok: true };
        let lockon = Some(bc_proto::LockOn { ref_vel: Vec3::new(150.0, 0.0, -40.0), up: Vec3::X });
        let walk = InputCmd { aim: Vec3::Z, thrust: [40, 0, 127], buttons: GRIP, ..InputCmd::default() };
        // On its feet, the legs walk as ever.
        let (mut a, mut b) = (standing(&field), standing(&field));
        for _ in 0..30 {
            move_step(&bodies, &mut a, &walk, &cx, crate::config::DT);
            move_step(&bodies, &mut b, &InputCmd { lockon, ..walk }, &cx, crate::config::DT);
        }
        assert_eq!(a, b);
        // Flying free, the suit rolls level with the fight's up and keeps pace with the target.
        let mut m = Mover {
            flight: FlightState {
                pos: Vec3::new(0.0, 2_000.0, 0.0),
                propellant: 1_000.0,
                ..FlightState::default()
            },
            footing: Footing::Free,
            anchor: Anchor::default(),
        };
        let fly = InputCmd {
            aim: Vec3::Z,
            buttons: bc_proto::buttons::FLIGHT_ASSIST,
            lockon,
            ..InputCmd::default()
        };
        for _ in 0..300 {
            move_step(&bodies, &mut m, &fly, &cx, crate::config::DT);
        }
        assert!((m.flight.rot * Vec3::Y).dot(Vec3::X) > 0.999, "{}", m.flight.rot * Vec3::Y);
        assert!((m.flight.vel - Vec3::new(150.0, 0.0, -40.0)).length() < 0.05, "{}", m.flight.vel);
    }

    mod city {
        use super::super::*;
        use crate::colony::city::{Stage, solid_built};
        use crate::colony::frame::{CityPos, STRIP_WIDTH, Under, from_colony};
        use crate::colony::interior::{ground_under, probe};
        use crate::colony::transit::station_x;
        use crate::config::DT;
        use crate::field::Field;
        use crate::math::Rng;
        use bc_proto::buttons::FLIGHT_ASSIST;

        /// The colony's inside, its city a body.
        fn inside(field: &Field) -> Bodies<'_> {
            Bodies::at(field, &[], 1).inside(true)
        }

        fn ctx() -> MoveCtx<'static> {
            let spec = crate::content::frame(bc_proto::FrameId::Leo);
            MoveCtx {
                spec,
                mods: FlightMods { interior: true, ..FlightMods::default() },
                can_grip: true,
                legs_ok: true,
            }
        }

        /// A Leo upright at `c`, nose along the axis.
        fn at(c: CityPos) -> Mover {
            let pos = c.to_colony();
            Mover {
                flight: FlightState {
                    pos,
                    rot: look_rotation(Vec3::X, up_at(pos)),
                    propellant: 2_000.0,
                    ..FlightState::default()
                },
                footing: Footing::Free,
                anchor: Anchor::default(),
            }
        }

        /// Where the avenue's lane is, up from the floor `h`: on strip 0 between two stations,
        /// clear of their platforms.
        fn avenue(h: f32) -> CityPos {
            CityPos::new(0, (station_x(2) + station_x(3)) * 0.5, STRIP_WIDTH * 0.5 + 24.0, h)
        }

        /// Steps `m` under `cmd` up to `ticks` times, until `done`; the ticks it took.
        fn run(b: &Bodies, m: &mut Mover, cmd: InputCmd, ticks: u32, done: impl Fn(&Mover) -> bool) -> u32 {
            let cx = ctx();
            for t in 0..ticks {
                if done(m) {
                    return t;
                }
                move_step(b, m, &cmd, &cx, DT);
            }
            ticks
        }

        /// How high the suit's origin is over the floor.
        fn height(m: &Mover) -> f32 {
            match from_colony(m.flight.pos) {
                Under::Land(c) => c.h,
                Under::Window { h, .. } => h,
            }
        }

        /// Flight assist on and the grip armed, aiming up the axis.
        fn armed() -> InputCmd {
            InputCmd { aim: Vec3::X, buttons: GRIP | FLIGHT_ASSIST, ..InputCmd::default() }
        }

        #[test]
        fn a_suit_lands_on_the_avenue_walks_it_and_lifts_off_smoothly() {
            let field = Field::empty();
            let b = inside(&field);
            let mut m = at(avenue(30.0));
            // Armed, 21 m over the street: caught, brought down no faster than the brake, and on
            // its feet, a stance over the floor.
            let mut fastest = 0.0f32;
            let cx = ctx();
            for _ in 0..600 {
                if m.footing == Footing::Grounded {
                    break;
                }
                move_step(&b, &mut m, &armed(), &cx, DT);
                fastest = fastest.max(m.flight.vel.length());
            }
            assert_eq!((m.footing, m.anchor.body), (Footing::Grounded, Body::City));
            assert!(fastest <= LAND_SPEED_MAX + 0.5, "came down at {fastest} m/s");
            assert!((height(&m) - STANCE).abs() < 0.02, "{}", height(&m));
            // Standing, its pilot feels the colony's g.
            for _ in 0..10 {
                move_step(&b, &mut m, &armed(), &cx, DT);
            }
            let g = crate::colony::frame::gravity(STANCE) / crate::config::G0;
            assert!((m.flight.g_load - g).abs() < 0.01 && g > 0.99, "{} g, want {g}", m.flight.g_load);
            // Up the avenue at a walk, on the ground all the way.
            let x0 = m.flight.pos.x;
            let walk = InputCmd { thrust: [0, 0, 127], ..armed() };
            for _ in 0..90 {
                move_step(&b, &mut m, &walk, &cx, DT);
                assert_eq!(m.footing, Footing::Grounded);
                assert!((height(&m) - STANCE).abs() < 0.02, "{}", height(&m));
            }
            let walked = m.flight.pos.x - x0;
            assert!(walked > 15.0 && walked < 25.0, "walked {walked} m in 3 s");
            // Letting go: flying again, from where it stood, with no jump.
            let stood = m.flight.pos;
            let free = InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() };
            for _ in 0..3 {
                move_step(&b, &mut m, &free, &cx, DT);
            }
            assert_eq!(m.footing, Footing::Free);
            assert!(m.flight.pos.distance(stood) < 1.0, "a jump of {} m", m.flight.pos.distance(stood));
        }

        #[test]
        fn a_suit_on_the_city_is_kept_off_its_walls_and_steps_off_a_roof() {
            let field = Field::empty();
            let b = inside(&field);
            let cx = ctx();
            let mut m = at(avenue(20.0));
            run(&b, &mut m, armed(), 600, |m| m.footing == Footing::Grounded);
            assert_eq!(m.footing, Footing::Grounded);
            // Across the strip into the blocks: stopped short of the building there (its wall 8 m
            // in from the kerb, nearer than a suit's stance), never nearer anything than it stands
            // high.
            let c = avenue(0.0);
            let across = (CityPos { s: c.s + 1.0, ..c }.to_colony() - c.to_colony()).normalize();
            let walk = InputCmd { aim: across, thrust: [0, 0, 127], ..armed() };
            let s0 = c.s;
            let mut furthest = 0.0f32;
            for _ in 0..600 {
                move_step(&b, &mut m, &walk, &cx, DT);
                assert_eq!(m.footing, Footing::Grounded, "{:?}", from_colony(m.flight.pos));
                let clear = probe(m.flight.pos).dist;
                assert!(clear > STANCE - 0.02, "{clear} m from the city at {:?}", from_colony(m.flight.pos));
                if let Under::Land(c) = from_colony(m.flight.pos) {
                    furthest = furthest.max(c.s - s0);
                }
            }
            assert!(furthest > 10.0, "toward the blocks: {furthest} m");
            assert!(m.anchor.vel.length() < 0.5, "stopped at the wall: {} m/s", m.anchor.vel.length());

            // A roof, wide enough to stand on: dropped onto it, armed, it lands there.
            let flat = |c: CityPos| {
                let top = c.h - ground_under(c.to_colony())?;
                let same = [(12.0, 0.0), (-12.0, 0.0), (0.0, 12.0), (0.0, -12.0)].iter().all(|(dx, ds)| {
                    let p = CityPos { x: c.x + dx, s: c.s + ds, ..c }.to_colony();
                    ground_under(p).is_some_and(|g| (c.h - g - top).abs() < 0.01)
                });
                // Clear of what stands on it: nothing nearer a suit standing there than its roof.
                let clear = probe(CityPos { h: top + STANCE, ..c }.to_colony()).dist > STANCE - 0.05;
                (same && clear && top > 25.0 && top < 150.0).then_some(top)
            };
            let (spot, top) = (0..2_000)
                .find_map(|k| {
                    let c = CityPos::new(
                        0,
                        avenue(0.0).x + (k % 100) as f32 * 9.0,
                        s0 + 60.0 + (k / 100) as f32 * 11.0,
                        400.0,
                    );
                    flat(c).map(|top| (c, top))
                })
                .expect("a roof to stand on");
            let mut m = at(CityPos { h: top + 20.0, ..spot });
            run(&b, &mut m, armed(), 600, |m| m.footing == Footing::Grounded);
            assert_eq!(m.footing, Footing::Grounded);
            assert!((height(&m) - (top + STANCE)).abs() < 0.05, "{} over a {top} m roof", height(&m));
            // Walked out over its edge, the suit falls (the colony's pull, braked), kept off the
            // walls it passes, and stands again lower down.
            let fell = [Vec3::X, -Vec3::X, across, -across].into_iter().any(|dir| {
                let walk = InputCmd { aim: dir, thrust: [0, 0, 127], ..armed() };
                let mut left = false;
                for _ in 0..900 {
                    move_step(&b, &mut m, &walk, &cx, DT);
                    if m.footing == Footing::Aloft {
                        left = true;
                        assert!(probe(m.flight.pos).dist > STANCE - 0.1, "kept off the walls");
                    }
                    if left && m.footing == Footing::Grounded {
                        return true;
                    }
                }
                false
            });
            assert!(fell, "it never stepped off the roof");
            assert!(height(&m) < top, "down from the roof: {}", height(&m));
            let under = ground_under(m.flight.pos).unwrap();
            // (On a kerb's rounded edge, up to its height more.)
            assert!(
                under > STANCE - 0.05 && under < STANCE + crate::colony::city::KERB + 0.05,
                "standing on what's under it: {under}"
            );
        }

        #[test]
        fn the_citys_probe_agrees_with_its_solids_and_finds_the_ground_under() {
            let mut rng = Rng::new(11);
            let (mut inside_n, mut outside_n) = (0, 0);
            for _ in 0..4_000 {
                let c = CityPos::new(
                    (rng.next_u32() % 3) as u8,
                    -15_000.0 + rng.next_f32() * 30_000.0,
                    40.0 + rng.next_f32() * (STRIP_WIDTH - 80.0),
                    0.5 + rng.next_f32() * 250.0,
                );
                let pr = probe(c.to_colony());
                let e = 0.02;
                let here = solid_built(
                    c.strip,
                    Vec3::new(c.x - e, c.h - e, -(c.s + e)),
                    Vec3::new(c.x + e, c.h + e, -(c.s - e)),
                    Stage(0),
                );
                // (Past the rounding of a box's edges and corners, which the solids don't have.)
                if pr.dist > 0.1 {
                    assert!(!here, "{c:?} is {} m out of the city but solid", pr.dist);
                    outside_n += 1;
                } else if pr.dist < -0.1 {
                    assert!(here, "{c:?} is {} m into the city but clear", -pr.dist);
                    inside_n += 1;
                }
                // Straight down, past the nearest surface's distance at most.
                if let Some(g) = ground_under(c.to_colony()) {
                    // (Less a box's rounding, which the boxes under don't have.)
                    assert!(g + 0.5 >= pr.dist.min(PROBE_REACH_CHECK), "{c:?}: {g} under, {} away", pr.dist);
                }
            }
            assert!(inside_n > 40 && outside_n > 2_000, "{inside_n} in, {outside_n} out");
            // Over the avenue it's the floor; under the floor, or in a building, nothing.
            let c = avenue(50.0);
            assert!((ground_under(c.to_colony()).unwrap() - 50.0).abs() < 1e-3);
            assert!(ground_under(CityPos { h: -5.0, ..c }.to_colony()).is_none());
        }

        /// What [`probe`] caps its distance at.
        const PROBE_REACH_CHECK: f32 = crate::colony::interior::PROBE_REACH;

        #[test]
        fn an_armed_suit_over_the_city_is_shown_the_ground_straight_under_it() {
            let field = Field::empty();
            let b = inside(&field);
            let m = at(avenue(40.0));
            let n = b.nearest_grippable(&m.flight, CATCH_RANGE + 20.0, CATCH_SPEED, CATCH_LEAVE).unwrap();
            assert_eq!(n.body, Body::City);
            assert!((n.h - (40.0 - STANCE)).abs() < 1e-3, "{}", n.h);
            assert!(n.n_world.distance(up_at(m.flight.pos)) < 1e-6);
            // Out in space there's no city.
            assert!(Bodies::at(&field, &[], 1).nearest_grippable(&m.flight, 100.0, 100.0, 100.0).is_none());
            assert!(!Bodies::at(&field, &[], 1).alive(Body::City));
        }
    }
}
