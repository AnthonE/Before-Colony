//! Procedural animation: each suit's bones posed every frame from its [`SuitDrive`].
//! - The arm holding the main weapon aims where the pilot aims (inside the simulation's 50° cone);
//!   the head looks there, the chest turns a little toward it. A gun in the right hand comes up to
//!   the ready in flight or as it fires: the upper arm forward, the forearm near level and the fist
//!   in line with it, the gun turned back in the fist so it keeps the aim (as a pistol's grip
//!   stands in a hand held out level). Parked, asleep, kneeling, in the bay or walking quietly on
//!   a body, it hangs at the side.
//! - Limb trim, as physics: the limbs swing against the suit's rotation (less with busy arms).
//! - Thrust posture: legs trail in forward burns and swing forward when braking, and swing away
//!   from sideways thrust; wing binders flare on boost.
//! - A blade sweeps the simulation's arc on its timing (windup, cut, recovery), both from the
//!   weapon's `MeleeSpec`, carried by the arm that holds it: twin blades mirror each other, and
//!   the Talon's head flies out along the aim on its cable and back.
//! - Aquila holds its shape: it's an aircraft, not a figure.
//! - Weapons kick when they fire; the Cetus's shield pods circle (a Corona's wards stay racked:
//!   `suits_vis` flies them out).
//! - Wrecks go limp.
//! - On a body, the suit walks (`bc_client_core::gait`): each foot planted on the surface where
//!   the body's shape has it, held there in the body's frame, the legs reaching it by two-bone IK
//!   (`bc_model::ik`), the soles turned flat to the ground. Each foot rolls onto its toes as it
//!   pushes off, its heel rising about their hinge while they stay flat (the gait's `push`), and
//!   kneeling, the back foot's toes are tucked under it. The hips drop as far as a stride needs,
//!   the knees fold with a crouch and the chest leans into it; a landing squashes the legs and they
//!   spring back. A pilot asleep in the cockpit, or a suit with its legs shot off, kneels. Limb trim and
//!   the thrust posture are the air's, not the ground's.
//!
//! Every joint rides a critically damped spring toward its target, so motion stays smooth
//! whatever the network does. Rotations are kept as scaled axes (radians) in the parent's frame.

use bc_client_core::gait::{Gait, Stand};
use bc_model::Sockets;
use bc_model::ik::{ANKLE_TO_SOLE, SHIN, THIGH, two_bone};
use bc_model::rig::{self, BONES, Bone};
use bc_proto::snapshot::ent_flags;
use bc_proto::{FrameId, Part, WeaponKind};
use bc_sim::config::DT;
use bc_sim::content::{ArmSlot, MeleeSpec, SpecialKind, Stroke, WeaponClass, frame, weapon};
use bc_sim::ground::{CROUCH_STANCE, STANCE};
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::damage::Damage;
use crate::model::SuitMeshLib;
use crate::suits_vis::SuitVisual;
use crate::view::{FxEvent, FxEvents, SuitDrive, SuitGround, VisTime};

/// The deepest a stride drops the hips, and a landing squashes the legs (m).
const MAX_BOB: f32 = 2.5;
/// A landing squashes the legs `SQUASH_PER_SPEED` m for each m/s it came down at (at most
/// [`MAX_BOB`]), and they spring back at `SQUASH_SPRING` rad/s.
const SQUASH_PER_SPEED: f32 = 0.12;
const SQUASH_SPRING: f32 = 10.0;
/// How fast the hips follow a stride, a crouch or a kneel (rad/s).
const DROP_SPRING: f32 = 8.0;
/// How high the origin rides kneeling (m): one knee on the ground, the thigh over it nearly
/// upright (the hips are 1.55 m over the origin).
const KNEEL_HEIGHT: f32 = 2.9;
/// A stride keeps the legs this much short of straight.
const REACH: f32 = 0.985;
/// How far the chest leans into a crouch (rad).
const CROUCH_LEAN: f32 = 0.2;
/// The legs' joints ride stiffer springs on the ground, so a planted foot stays put (rad/s).
const LEG_STIFF: f32 = 40.0;
/// How far a foot rolls onto its toes as it leaves the ground (rad): its heel up about their
/// hinge.
const TOE_ROLL: f32 = 0.55;
/// Kneeling, the back foot stands on its tucked toes, its heel up this far (rad), its ankle this
/// high over the ground (m): the toes' hinge on the ground under it.
const KNEEL_ROLL: f32 = 1.4;
const KNEEL_ANKLE: f32 = 1.73;

/// Which hands carry a strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hands {
    Left,
    Right,
    /// Twin blades: the right hand's sweeps the arc, the left's its mirror image.
    Both,
    /// The Talon: the right hand is the head that flies out.
    Talon,
}

/// A melee strike being drawn.
#[derive(Clone, Copy, Debug)]
pub struct Swing {
    /// Seconds since it began.
    pub t: f32,
    pub spec: MeleeSpec,
    pub weapon: WeaponKind,
    pub hands: Hands,
    /// Where a thrust drives, suit frame (the aim when it began).
    dir: Vec3,
}

/// The melee weapon a suit's flags say it is striking with, and the hands that carry it.
pub fn striking(d: &SuitDrive) -> Option<(WeaponKind, MeleeSpec, Hands)> {
    let spec = frame(d.frame);
    let m = spec.melee_mount(spec.striking_slot(d.flags))?;
    let melee = weapon(m.weapon).melee?;
    let hands = match (melee.stroke, m.arm) {
        (Stroke::Thrust, _) => Hands::Talon,
        _ if melee.twin || m.arm == ArmSlot::Both => Hands::Both,
        (_, ArmSlot::Right) => Hands::Right,
        _ => Hands::Left,
    };
    Some((m.weapon, melee, hands))
}

impl Hands {
    /// Whether the left or the right hand carries a blade.
    pub fn holds(self, right: bool) -> bool {
        match self {
            Hands::Left => !right,
            Hands::Right => right,
            Hands::Both => true,
            Hands::Talon => false,
        }
    }
}

impl Swing {
    /// The strike a suit's flags say has begun, by the frame's mounts.
    fn begin(d: &SuitDrive, aim_local: Vec3) -> Option<Self> {
        let (weapon, spec, hands) = striking(d)?;
        Some(Self { t: 0.0, spec, weapon, hands, dir: aim_local.normalize_or(Vec3::Z) })
    }

    /// How far into the stroke the blade is: its direction along the arc from `rest`, suit frame.
    fn blade(&self, rest: Vec3, mirror: bool) -> Vec3 {
        let m = &self.spec;
        let (windup, cut, recovery) =
            (f32::from(m.windup) * DT, f32::from(m.active) * DT, f32::from(m.recovery) * DT);
        let flip = |v: Vec3| if mirror { Vec3::new(-v.x, v.y, v.z) } else { v };
        let (from, to) = (flip(m.arc_from.normalize()), flip(m.arc_to.normalize()));
        let s = self.t;
        if s < windup {
            rest.lerp(from, smooth(0.0, windup, s))
        } else if s < windup + cut {
            from.lerp(to, (s - windup) / cut)
        } else {
            to.lerp(rest, smooth(0.0, recovery, s - windup - cut))
        }
    }

    /// How far out the Talon's head is (m): out through the stroke, back in the recovery.
    fn reach(&self) -> f32 {
        let m = &self.spec;
        let (windup, cut, recovery) =
            (f32::from(m.windup) * DT, f32::from(m.active) * DT, f32::from(m.recovery) * DT);
        let range = weapon(self.weapon).range;
        let s = self.t;
        if s < windup {
            0.0
        } else if s < windup + cut {
            range * smooth(0.0, cut, s - windup)
        } else {
            range * (1.0 - smooth(0.0, recovery, s - windup - cut))
        }
    }
}

/// The way a hand's blade points at rest, suit frame: the beam saber from its hilt, other blades
/// from their sockets.
pub fn blade_rest(sockets: &Sockets, weapon: WeaponKind, right: bool) -> Vec3 {
    let (_, saber) = sockets.saber;
    match (right, weapon) {
        (true, _) => sockets.blade_right.map_or(Vec3::new(-saber.x, saber.y, saber.z), |b| b.1),
        (false, WeaponKind::BeamSaber) => saber,
        (false, _) => sockets.blade_left.map_or(saber, |b| b.1),
    }
}

/// A suit's pose and what it's doing, between frames.
#[derive(Component)]
pub struct Anim {
    /// Each bone's rotation from rest (scaled axis, parent's frame) and its spring velocity.
    pub rot: [Vec3; BONES],
    vel: [Vec3; BONES],
    /// The Talon's head, out from the wrist on its cable (in the forearm's frame).
    pub reach: Vec3,
    prev_rot: Option<Quat>,
    /// Smoothed angular velocity in the suit's frame (rad/s).
    spin: Vec3,
    /// The melee strike under way.
    pub swing: Option<Swing>,
    saber_flag: bool,
    /// The shield pods' angle.
    orbit: f32,
    /// The walk on a body (the feet, in its frame).
    pub gait: Gait,
    /// How far the hips have dropped below the origin for a stride, a crouch or a kneel, and its
    /// spring's rate (m, m/s); the stride's share of it (the cockpit's eye takes a little).
    drop: f32,
    drop_v: f32,
    pub bob: f32,
    /// A landing's squash, and its rate (m, m/s).
    squash: f32,
    squash_v: f32,
    /// How fast it was coming down onto the surface last frame, in the air in the body's grip
    /// (m/s); none standing or flying free.
    falling: Option<f32>,
    /// Where the torso sits from the suit's origin (suit frame): dropped along the surface's
    /// normal.
    pub lift: Vec3,
    /// How far the gun arm is up at the ready (0 at the side, 1 up), and how long it stays up
    /// after the gun last fired or charged (s).
    ready: f32,
    ready_hold: f32,
    /// How far a Corona's wards are out of their racks (0 racked, 1 holding the field), and
    /// how far round the field has turned (rad).
    ward: f32,
    ward_spin: f32,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            rot: [Vec3::ZERO; BONES],
            vel: [Vec3::ZERO; BONES],
            reach: Vec3::ZERO,
            prev_rot: None,
            spin: Vec3::ZERO,
            swing: None,
            saber_flag: false,
            orbit: 0.0,
            gait: Gait::default(),
            drop: 0.0,
            drop_v: 0.0,
            bob: 0.0,
            squash: 0.0,
            squash_v: 0.0,
            falling: None,
            lift: Vec3::ZERO,
            ready: 0.0,
            ready_hold: 0.0,
            ward: 0.0,
            ward_spin: 0.0,
        }
    }
}

impl Anim {
    /// Where a point on `bone` is in the world, posed.
    pub fn point(&self, d: &SuitDrive, bone: Bone, local: Vec3) -> Vec3 {
        let mut p = local;
        let mut b = Some(bone);
        while let Some(k) = b {
            p = Quat::from_scaled_axis(self.rot[k.index()]) * p + self.rest(k);
            b = k.def().parent;
        }
        d.pos + d.rot * p
    }

    /// Where `bone`'s joint sits in its parent's frame, posed: at rest, but for the Talon's
    /// head out on its cable, and the torso dropped on a body.
    pub fn rest(&self, bone: Bone) -> Vec3 {
        match bone {
            Bone::HandR => bone.rest() + self.reach,
            Bone::Torso => self.lift,
            _ => bone.rest(),
        }
    }

    /// How `bone` is turned relative to the suit, posed.
    pub fn world_rot(&self, bone: Bone) -> Quat {
        let mut q = Quat::from_scaled_axis(self.rot[bone.index()]);
        let mut b = bone.def().parent;
        while let Some(k) = b {
            q = Quat::from_scaled_axis(self.rot[k.index()]) * q;
            b = k.def().parent;
        }
        q
    }
}

/// The gun arm at the ready: how far the upper arm swings forward and the forearm bends up from
/// it (rad, about the suit's x), how fast it comes up or goes down (fractions a second), and how
/// long it stays up after the gun last fired (s).
const READY_UPPER: f32 = -0.35;
const READY_FORE: f32 = -0.73;
const READY_RATE: f32 = 3.0;
const READY_HOLD: f32 = 4.0;

/// How many pieces each arc between a Corona's ward discs is drawn in (kinked where they meet).
pub const ARC_KINKS: usize = 3;

/// A cheap hash of `x` into 0..1 (the arcs' flicker and kinks).
fn noise(x: f64) -> f32 {
    ((x.sin() * 43_758.545).fract().abs()) as f32
}

/// How fast a Corona's field turns its wards round it (rad/s).
const WARD_SPIN: f32 = 0.35;

/// Where ward `k` of `n` holds the field, in the suit's frame: two rings of them on the
/// field's sphere, above and below the suit's middle, each a half step round from the other.
fn ward_spot(k: usize, n: usize, spin: f32) -> Vec3 {
    let per = n.div_ceil(2).max(1);
    let (ring, at) = (k % 2, k / 2);
    let lat = if ring == 0 { 0.34f32 } else { -0.34 };
    let lon = spin + std::f32::consts::TAU * (at as f32 + 0.5 * ring as f32) / per as f32;
    let r = bc_sim::content::WARD_RADIUS * 0.94;
    Vec3::new(lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()) * r
}

fn show(v: &mut Visibility, on: bool) {
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
}

/// The rotation taking `from` to `to`, as a scaled axis, limited to `max` radians.
fn turn(from: Vec3, to: Vec3, max: f32) -> Vec3 {
    let q = Quat::from_rotation_arc(from.normalize_or(Vec3::Z), to.normalize_or(Vec3::Z));
    q.to_scaled_axis().clamp_length_max(max)
}

/// A critically damped spring's exact step toward `target` (stable at any frame time).
fn spring(x: &mut Vec3, v: &mut Vec3, target: Vec3, w: f32, dt: f32) {
    let y = *x - target;
    let j = *v + y * w;
    let e = (-w * dt).exp();
    *x = target + (y + j * dt) * e;
    *v = (*v - j * (w * dt)) * e;
}

/// [`spring`], for one number.
fn spring1(x: &mut f32, v: &mut f32, target: f32, w: f32, dt: f32) {
    let (mut xv, mut vv) = (Vec3::X * *x, Vec3::X * *v);
    spring(&mut xv, &mut vv, Vec3::X * target, w, dt);
    (*x, *v) = (xv.x, vv.x);
}

/// The legs of a suit on a body, posed this frame: each leg's thigh, shin, foot and toe rotations
/// (scaled axes, parents' frames), and how far the hips must drop for the feet to be reached (m).
struct Legs {
    rot: [[Vec3; 4]; 2],
    drop: f32,
}

/// The bones of the left and the right leg.
const LEG_BONES: [[Bone; 4]; 2] = [
    [Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL],
    [Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR],
];

/// The legs reaching for `ankles`, their knees bending toward `poles` (both in the suit's frame,
/// from the torso), the soles turned flat to `up` (suit frame) but for each foot's heel rolled up
/// `rolls` (rad) about its toes, which stay flat.
fn reach_legs(ankles: [Vec3; 2], poles: [Vec3; 2], up: Vec3, rolls: [f32; 2]) -> [[Vec3; 4]; 2] {
    // The rig's legs aren't quite straight at rest: each bone's rotation is taken from its own
    // rest line, not the IK's straight-down one.
    let rest_line = |b: Bone| Quat::from_rotation_arc(b.rest().normalize(), Vec3::NEG_Y);
    let flat = Quat::from_rotation_arc(Vec3::Y, up);
    [0, 1].map(|side| {
        let [thigh, shin, foot, _] = LEG_BONES[side];
        let hip = thigh.def().joint;
        let (tq, sq) = two_bone(hip, poles[side], ankles[side], THIGH, SHIN);
        let (a, b) = (rest_line(shin), rest_line(foot));
        let thigh_rot = tq * a;
        let shin_rot = a.inverse() * sq * b;
        let foot_rot = (thigh_rot * shin_rot).inverse() * flat * Quat::from_rotation_x(rolls[side]);
        let toe_rot = Quat::from_rotation_x(-rolls[side]);
        [thigh_rot, shin_rot, foot_rot, toe_rot].map(Quat::to_scaled_axis)
    })
}

/// Standing on a body: where each foot is (its gait, or a kneel), how the legs reach them, and how
/// far the hips drop to do it.
fn ground_legs(d: &SuitDrive, g: &SuitGround, gait: &Gait, lift: Vec3, kneel: bool) -> Legs {
    let inv = d.rot.inverse();
    let up = (inv * g.up).normalize_or(Vec3::Y);
    let fwd = (Vec3::Z - up * up.z).normalize_or(Vec3::Z);
    // Suit frame, from the origin.
    let ground = -up * g.height;
    let hip = |side: usize| LEG_BONES[side][0].def().joint;
    let on_ground = |side: usize| {
        let h = hip(side);
        h - up * (h.dot(up) - ground.dot(up))
    };
    let (mut ankles, poles, rolls) = if kneel {
        // The right foot planted ahead, its knee up by the chest (the shin is long), the left knee
        // down with its shin along the ground behind and its toes tucked under it.
        (
            [on_ground(0) - fwd * 4.8 + up * KNEEL_ANKLE, on_ground(1) + fwd * 3.2 + up * ANKLE_TO_SOLE],
            [hip(0) + fwd * 5.0 - up * 4.0, hip(1) + fwd * 10.0],
            [KNEEL_ROLL, 0.0],
        )
    } else {
        let ankle = |side: usize| inv * (g.pose.to_world(gait.feet[side].at) - d.pos) + up * ANKLE_TO_SOLE;
        let roll = |side: usize| gait.feet[side].push * TOE_ROLL;
        ([ankle(0), ankle(1)], [hip(0) + fwd * 10.0, hip(1) + fwd * 10.0], [roll(0), roll(1)])
    };
    // A planted foot rolling onto its toes lifts its ankle about their hinge.
    let flat = Quat::from_rotation_arc(Vec3::Y, up);
    for side in 0..2 {
        if !kneel && gait.feet[side].planted && rolls[side] > 0.0 {
            let hinge = ankles[side] + flat * LEG_BONES[side][3].rest();
            let heel_up = Quat::from_axis_angle(flat * Vec3::X, rolls[side]);
            ankles[side] = hinge + heel_up * (ankles[side] - hinge);
        }
    }
    // How far the hips must drop for the further foot to be in reach.
    let reach = (THIGH + SHIN) * REACH;
    let drop = [0, 1]
        .map(|side| {
            let v = ankles[side] - hip(side);
            let b = v.dot(up);
            let disc = b * b - v.length_squared() + reach * reach;
            if disc >= 0.0 { -b - disc.sqrt() } else { -b }
        })
        .into_iter()
        .fold(0.0f32, f32::max)
        .clamp(0.0, MAX_BOB);
    Legs { rot: reach_legs(ankles.map(|a| a - lift), poles.map(|p| p - lift), up, rolls), drop }
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Poses every suit's bones.
pub fn animate_suits(
    time: Res<VisTime>,
    lib: Res<SuitMeshLib>,
    mut events: ResMut<FxEvents>,
    mut suits: Query<(&SuitDrive, &SuitVisual, &mut Anim, Option<&Damage>)>,
    mut bones: Query<&mut Transform>,
    mut shown: Query<&mut Visibility>,
    mut tags: Query<&mut MeshTag>,
) {
    let dt = time.dt.min(0.1);
    let mut touchdowns = Vec::new();
    for (d, v, mut a, damage) in &mut suits {
        let a = &mut *a;
        let has = |f: u16| d.flags & f != 0;
        let wreck = has(ent_flags::WRECK);

        // Angular velocity in the suit's frame, from how its rotation changed.
        if let Some(prev) = a.prev_rot
            && dt > 0.0
        {
            let w = (prev.inverse() * d.rot).to_scaled_axis() / dt;
            let w = if w.length() < 20.0 { w } else { Vec3::ZERO };
            a.spin += (w - a.spin) * (1.0 - (-dt * 10.0).exp());
        }
        a.prev_rot = Some(d.rot);

        let aim_local = d.rot.inverse() * d.aim;
        // A melee strike starts on the flag's rising edge.
        let saber = has(ent_flags::SABER) && !wreck;
        if saber && !a.saber_flag {
            a.swing = Swing::begin(d, aim_local);
        }
        a.saber_flag = saber;
        if let Some(sw) = &mut a.swing {
            sw.t += dt;
            if sw.t > f32::from(sw.spec.duration()) * DT {
                a.swing = None;
            }
        }

        let mut target = [Vec3::ZERO; BONES];
        let mut stiff = [9.0f32; BONES];
        let bird = d.frame == FrameId::Aquila;
        // On a body: standing on it (walking, crouched or kneeling), or in the air in its grip.
        let ground = d.ground.filter(|_| !wreck && !bird);
        let standing = ground.filter(|g| !g.aloft);
        let kneel = standing.is_some() && (has(ent_flags::ASLEEP) || d.parts[Part::Legs as usize] == 0);
        match standing {
            Some(g) if !kneel => {
                let inv = g.pose.rot.conjugate();
                let stand = Stand {
                    body: g.body,
                    shape: &g.shape,
                    local: g.pose.to_local(d.pos),
                    rot: inv * d.rot,
                    vel: inv * g.rel_vel,
                    up: inv * g.up,
                    stance: g.height,
                };
                a.gait.step(&stand, dt);
            }
            // Off the ground (or kneeling) the feet start over where they land.
            _ => a.gait.body = None,
        }
        // A landing: the legs squash with how hard it came down, and the ground puffs.
        if let Some(g) = standing
            && let Some(v) = a.falling
        {
            a.squash_v += (SQUASH_PER_SPEED * v).min(MAX_BOB) * SQUASH_SPRING * std::f32::consts::E;
            let pos = d.pos - g.up * g.height;
            touchdowns.push(FxEvent::Touchdown {
                pos,
                vel: g.pose.point_vel(pos),
                normal: g.up,
                speed: v,
                rock: matches!(g.shape.base, bc_sim::bodies::Base::Ellipsoid(_)),
            });
        }
        a.falling = ground.filter(|g| g.aloft).map(|g| (-g.rel_vel.dot(g.up)).max(0.0));
        let up = ground.map_or(Vec3::Y, |g| (d.rot.inverse() * g.up).normalize_or(Vec3::Y));
        let mut drop = 0.0;
        // The arm that holds the main weapon aims it (Algol's claw shield is on the left).
        let aim_left = frame(d.frame).loadout[0].is_some_and(|m| m.arm == ArmSlot::Left);
        let (aim_up, aim_fore) =
            if aim_left { (Bone::UpperArmL, Bone::ForearmL) } else { (Bone::UpperArmR, Bone::ForearmR) };
        let arms_busy = has(ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY | ent_flags::CHARGING)
            || a.swing.is_some()
            || d.holding.is_some();

        if bird {
            // An aircraft: every joint holds, stiffly.
            stiff = [30.0; BONES];
        } else if wreck {
            // Limp: every joint drifts to a loose pose of its own.
            let seed = f32::from(d.slot) * 1.7;
            for (i, t) in target.iter_mut().enumerate() {
                let k = seed + i as f32 * 2.3;
                *t = Vec3::new(k.sin(), (k * 1.3).cos(), (k * 0.7).sin()) * 0.5;
            }
            stiff = [2.0; BONES];
        } else {
            // Aim: the chest turns a little toward it, and the right arm brings the weapon the rest
            // of the way, shoulder and elbow sharing the turn; the head looks there too.
            let aim = turn(Vec3::Z, aim_local, 50f32.to_radians());
            // Standing, the chest leans into a crouch, and further kneeling.
            let lean = standing.map_or(0.0, |g| {
                let crouch = ((STANCE - g.height) / (STANCE - CROUCH_STANCE)).clamp(0.0, 1.0);
                CROUCH_LEAN * crouch + if kneel { 0.25 } else { 0.0 }
            });
            let chest = Vec3::new(lean, aim.y * 0.25, 0.0);
            target[Bone::Chest.index()] = chest;
            // An arm holding a chunk holds still in the suit's frame, undoing the chest's turn: the
            // chunk rides the suit, just in front of the hand.
            if let Some(right) = d.holding {
                let arm = if right { Bone::UpperArmR } else { Bone::UpperArmL };
                target[arm.index()] = -chest;
                stiff[arm.index()] = 20.0;
            }
            if d.holding != Some(!aim_left) {
                target[aim_up.index()] = (aim - chest) * 0.45;
                target[aim_fore.index()] = (aim - chest) * 0.55;
            }
            target[Bone::Head.index()] = turn(Vec3::Z, aim_local, 1.0) * 0.7 - chest;
            stiff[aim_up.index()] = 14.0;
            stiff[aim_fore.index()] = 14.0;

            if let Some(g) = standing {
                // On the ground: the legs reach the feet where they're planted (or kneel), and
                // the hips drop as far as that takes.
                let legs = ground_legs(d, &g, &a.gait, a.lift, kneel);
                for (side, bones) in LEG_BONES.iter().enumerate() {
                    for (k, b) in bones.iter().enumerate() {
                        target[b.index()] = legs.rot[side][k];
                        stiff[b.index()] = LEG_STIFF;
                    }
                }
                drop = if kneel { (g.height - KNEEL_HEIGHT).max(0.0) } else { legs.drop };
            } else {
                // Limb trim: limbs swing against the rotation.
                let reaction = (-a.spin * 0.22).clamp_length_max(0.6);
                for b in [Bone::ThighL, Bone::ThighR] {
                    target[b.index()] += reaction;
                }
                if !arms_busy {
                    let free = if aim_left { Bone::UpperArmR } else { Bone::UpperArmL };
                    target[free.index()] += reaction * 0.8;
                }

                // Thrust posture: legs trail forward burns, swing forward braking, and away from
                // sideways thrust; knees follow.
                let t = d.thrust;
                let trail = Vec3::new(0.4 * t.z, 0.0, -0.25 * t.x);
                for (thigh, shin) in [(Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR)] {
                    target[thigh.index()] += trail;
                    target[shin.index()] += Vec3::new(0.35 * t.z.abs() + 0.1, 0.0, 0.0);
                }
            }
            // Wings flare on boost.
            let flare = if has(ent_flags::BOOST) { 0.35 } else { 0.0 };
            target[Bone::WingR.index()] = Vec3::new(0.0, 0.0, -flare);
            target[Bone::WingL.index()] = Vec3::new(0.0, 0.0, flare);

            // The blade's arc, carried by the arm (or arms) that hold it; the fang flies out.
            if let Some(sw) = a.swing {
                let sockets = lib.sockets(d.frame);
                let mut carry = |right: bool, mirror: bool| {
                    let rest = blade_rest(sockets, sw.weapon, right);
                    let r = turn(rest, sw.blade(rest, mirror), 2.6);
                    let (up, fore) = if right {
                        (Bone::UpperArmR, Bone::ForearmR)
                    } else {
                        (Bone::UpperArmL, Bone::ForearmL)
                    };
                    target[up.index()] = r * 0.55;
                    target[fore.index()] = r * 0.45;
                    stiff[up.index()] = 40.0;
                    stiff[fore.index()] = 40.0;
                };
                match sw.hands {
                    Hands::Left => carry(false, false),
                    Hands::Right => carry(true, false),
                    Hands::Both => {
                        carry(true, false);
                        carry(false, true);
                    }
                    // The arm points the head at where it's going.
                    Hands::Talon => {
                        let r = turn(Vec3::Z, sw.dir, 50f32.to_radians());
                        target[Bone::UpperArmR.index()] = (r - chest) * 0.45;
                        target[Bone::ForearmR.index()] = (r - chest) * 0.55;
                        stiff[Bone::UpperArmR.index()] = 30.0;
                        stiff[Bone::ForearmR.index()] = 30.0;
                    }
                }
            }
        }

        // A gun in the right hand at the ready: in flight, or for a while after it fires or as it
        // charges; never parked, asleep, kneeling or in the bay. Not while the right arm strikes
        // or holds something.
        let gun = frame(d.frame).loadout[0].is_some_and(|m| {
            m.arm == ArmSlot::Right
                && matches!(weapon(m.weapon).class, WeaponClass::Beam | WeaponClass::Ballistic)
        });
        if has(ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY | ent_flags::CHARGING) {
            a.ready_hold = READY_HOLD;
        } else {
            a.ready_hold = (a.ready_hold - dt).max(0.0);
        }
        let right_busy = d.holding == Some(true)
            || a.swing.is_some_and(|sw| matches!(sw.hands, Hands::Right | Hands::Both | Hands::Talon));
        let resting = has(ent_flags::ASLEEP) || kneel || wreck || bird || d.slot == crate::hangar::BAY_SLOT;
        let want = if !gun || resting || right_busy {
            0.0
        } else if a.ready_hold > 0.0 || ground.is_none() {
            1.0
        } else {
            0.0
        };
        a.ready += (want - a.ready).clamp(-READY_RATE * dt, READY_RATE * dt);
        if a.ready > 0.0 && !right_busy {
            let (up, fore) = (Bone::UpperArmR.index(), Bone::ForearmR.index());
            let lift = Quat::from_rotation_x(READY_UPPER * a.ready);
            let bend = Quat::from_rotation_x(READY_FORE * a.ready);
            let (aim_up, aim_fore) =
                (Quat::from_scaled_axis(target[up]), Quat::from_scaled_axis(target[fore]));
            target[up] = (aim_up * lift).to_scaled_axis();
            target[fore] = (aim_fore * bend).to_scaled_axis();
            // The gun turns back in the fist by what the arm's bend added, so it keeps the aim.
            target[Bone::Weapon.index()] =
                (bend.inverse() * aim_fore.inverse() * lift.inverse() * aim_fore).to_scaled_axis();
            stiff[Bone::Weapon.index()] = stiff[fore];
        }

        // Weapons kick up when they fire.
        for ev in &events.0 {
            if let FxEvent::Muzzle { shooter: Some(slot), weapon, .. } = *ev
                && slot == d.slot
            {
                let main =
                    frame(d.frame).loadout[0].is_some_and(|m| m.weapon == weapon && m.arm == ArmSlot::Right);
                if main {
                    a.vel[Bone::ForearmR.index()] += Vec3::new(-3.0, 0.0, 0.0);
                }
            }
        }

        // Critically damped springs toward the targets.
        for i in 0..BONES {
            spring(&mut a.rot[i], &mut a.vel[i], target[i], stiff[i], dt);
        }
        // The hips drop along the surface's normal: a stride, a crouch or a kneel, and a landing's
        // squash.
        spring1(&mut a.drop, &mut a.drop_v, drop, DROP_SPRING, dt);
        spring1(&mut a.squash, &mut a.squash_v, 0.0, SQUASH_SPRING, dt);
        a.bob = if standing.is_some() && !kneel { a.drop } else { 0.0 };
        a.lift = -up * (a.drop + a.squash).clamp(-MAX_BOB, MAX_BOB + STANCE - KNEEL_HEIGHT);
        // The fang's head out along the thrust, in the forearm's frame as it is now posed.
        a.reach = match a.swing {
            Some(sw) if sw.hands == Hands::Talon => {
                let forearm = a.world_rot(Bone::ForearmR);
                forearm.inverse() * sw.dir * sw.reach()
            }
            _ => Vec3::ZERO,
        };
        // The shield pods circle the suit (a Corona's wards stay in their racks).
        if !matches!(frame(d.frame).special, SpecialKind::Ward { .. }) {
            a.orbit = (a.orbit + dt * 0.6) % std::f32::consts::TAU;
            a.rot[Bone::Props.index()] = Vec3::new(0.0, a.orbit, 0.0);
        }

        // The fang's cable, from the wrist out to the head.
        if let Some(cable) = v.cable
            && let Ok(mut tf) = bones.get_mut(cable)
        {
            let len = a.reach.length();
            tf.translation = Bone::HandR.rest() + a.reach * 0.5;
            tf.rotation = Quat::from_rotation_arc(Vec3::Y, a.reach.normalize_or(Vec3::Y));
            tf.scale = Vec3::new(0.25, len.max(0.001), 0.25);
        }
        // The Sagittarius's power cable, from its generator to its cannon as they're posed (in the
        // suit's frame), while the arm that holds the cannon is on.
        if let (Some(cable), Some((from, to))) = (v.power, lib.sockets(d.frame).cable) {
            let inv = d.rot.inverse();
            let p = inv * (a.point(d, Bone::Backpack, from) - d.pos);
            let q = inv * (a.point(d, Bone::Weapon, to) - d.pos);
            if let Ok(mut tf) = bones.get_mut(cable) {
                tf.translation = (p + q) * 0.5;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, (q - p).normalize_or(Vec3::Y));
                tf.scale = Vec3::new(0.16, (q - p).length().max(0.001), 0.16);
            }
            let on = !wreck && d.parts[Part::ArmR as usize] > 0 && d.parts[Part::Backpack as usize] > 0;
            if let Ok(mut s) = shown.get_mut(cable) {
                show(&mut s, on);
            }
        }
        // A Corona's wards: out of their racks while its special is up (taking its deploy
        // time to fly out, as the simulation does, and as long to come home), turning round it in
        // two rings on its field's sphere; its racks empty meanwhile, and the field's sheet glows
        // once they hold it.
        if let SpecialKind::Ward { deploy, .. } = frame(d.frame).special {
            let on = has(ent_flags::SPECIAL) && !wreck;
            let rate = 1.0 / (f32::from(deploy.max(1)) * DT);
            a.ward = (a.ward + if on { rate * dt } else { -rate * dt }).clamp(0.0, 1.0);
            a.ward_spin = (a.ward_spin + dt * WARD_SPIN) % std::f32::consts::TAU;
            let ease = a.ward * a.ward * (3.0 - 2.0 * a.ward);
            let racks = &lib.sockets(d.frame).wards;
            let mut at = [Vec3::ZERO; bc_sim::content::WARDS];
            for (k, (&e, &(home, face))) in v.wards.iter().zip(racks).enumerate() {
                let spot = ward_spot(k, racks.len(), a.ward_spin);
                if let Some(p) = at.get_mut(k) {
                    *p = (home + a.lift).lerp(spot, ease);
                }
                if let Ok(mut tf) = bones.get_mut(e) {
                    tf.translation = (home + a.lift).lerp(spot, ease);
                    let (racked, held) = (
                        Quat::from_rotation_arc(Vec3::Z, face),
                        Quat::from_rotation_arc(Vec3::Z, spot.normalize()),
                    );
                    tf.rotation = racked.slerp(held, ease);
                }
                if let Ok(mut s) = shown.get_mut(e) {
                    show(&mut s, a.ward > 0.0);
                }
            }
            // (A rack broken off with the suit's torso stays gone.)
            let props = Bone::Props.index();
            if !damage.is_some_and(|dmg| dmg.lost[props])
                && let Ok(mut s) = shown.get_mut(v.bones[props])
            {
                show(&mut s, a.ward <= 0.0);
            }
            // Arcs jump between each disc and the next round its ring while they hold the field,
            // each flickering on and off on its own beat, kinked where it jumps anew each time.
            let n = racks.len().min(at.len());
            for (k, pieces) in v.arcs.chunks(ARC_KINKS).enumerate().take(n) {
                let (p, q) = (at[k], at[(k + 2) % n]);
                let beat = time.now * 23.0 + k as f64 * 7.3;
                let lit = a.ward >= 1.0 && on && noise(beat) > 0.45;
                // Its kinks: off the straight line, square to it, by up to a tenth of its length.
                let along = (q - p).normalize_or(Vec3::X);
                let (u, w) = (along.any_orthonormal_vector(), along.cross(along.any_orthonormal_vector()));
                let jump = (q - p).length() * 0.1;
                let jag = |i: usize| {
                    if i == 0 || i == ARC_KINKS {
                        return if i == 0 { p } else { q };
                    }
                    let (s, t) =
                        (noise(beat.floor() + i as f64 * 1.7), noise(beat.floor() * 1.3 + i as f64 * 3.1));
                    p.lerp(q, i as f32 / ARC_KINKS as f32) + (u * (s - 0.5) + w * (t - 0.5)) * 2.0 * jump
                };
                for (i, &piece) in pieces.iter().enumerate() {
                    if lit && let Ok(mut tf) = bones.get_mut(piece) {
                        let (from, to) = (jag(i), jag(i + 1));
                        let len = (to - from).length().max(0.01);
                        crate::beams::place_ribbon(&mut tf, to, (to - from) / len, len, 0.18);
                    }
                    if let Ok(mut s) = shown.get_mut(piece) {
                        show(&mut s, lit);
                    }
                }
            }
            if let Some(f) = v.field {
                let held = a.ward >= 1.0 && on;
                if let Ok(mut s) = shown.get_mut(f) {
                    show(&mut s, held);
                }
                if held && let Ok(mut t) = tags.get_mut(f) {
                    let shimmer = 0.75 + 0.25 * ((time.now * 5.0 + f64::from(d.slot)).sin() as f32);
                    *t = MeshTag((120.0 * shimmer) as u32);
                }
            }
        }
        for (i, e) in v.bones.iter().enumerate() {
            if damage.is_some_and(|dmg| dmg.lost[i]) {
                continue; // broken off: no longer this suit's to pose
            }
            if let Ok(mut tf) = bones.get_mut(*e) {
                tf.rotation = Quat::from_scaled_axis(a.rot[i]);
                if i == Bone::HandR.index() || i == Bone::Torso.index() {
                    tf.translation = a.rest(rig::ALL[i]);
                }
            }
        }
    }
    events.0.extend(touchdowns);
}
