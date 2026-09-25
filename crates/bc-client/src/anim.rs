//! Procedural animation: each suit's bones posed every frame from its [`SuitDrive`].
//! - The arm holding the main weapon aims where the pilot aims (inside the simulation's 50° cone);
//!   the head looks there, the chest turns a little toward it.
//! - AMBAC, as physics: the limbs swing against the suit's rotation (less with busy arms).
//! - Thrust posture: legs trail in forward burns and swing forward when braking, and swing away
//!   from sideways thrust; wing binders flare on boost.
//! - A blade sweeps the simulation's arc on its timing (windup, cut, recovery), both from the
//!   weapon's `MeleeSpec`, carried by the arm that holds it: twin blades mirror each other, and
//!   the Dragon Fang's head flies out along the aim on its cable and back.
//! - Neo-Bird holds its shape: it's an aircraft, not a figure.
//! - Weapons kick when they fire; the Virgo's Planet Defensors circle.
//! - Wrecks go limp.
//!
//! Every joint rides a critically damped spring toward its target, so motion stays smooth
//! whatever the network does. Rotations are kept as scaled axes (radians) in the parent's frame.

use bc_model::Sockets;
use bc_model::rig::{BONES, Bone};
use bc_proto::snapshot::ent_flags;
use bc_proto::{FrameId, WeaponKind};
use bc_sim::config::DT;
use bc_sim::content::{ArmSlot, MeleeSpec, Stroke, frame, weapon};
use bevy::prelude::*;

use crate::damage::Damage;
use crate::model::SuitMeshLib;
use crate::suits_vis::SuitVisual;
use crate::view::{FxEvent, FxEvents, SuitDrive, VisTime};

/// Which hands carry a strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hands {
    Left,
    Right,
    /// Twin blades: the right hand's sweeps the arc, the left's its mirror image.
    Both,
    /// The Dragon Fang: the right hand is the head that flies out.
    Fang,
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
        (Stroke::Thrust, _) => Hands::Fang,
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
            Hands::Fang => false,
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

    /// How far out the Dragon Fang's head is (m): out through the stroke, back in the recovery.
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
    /// The Dragon Fang's head, out from the wrist on its cable (in the forearm's frame).
    pub reach: Vec3,
    prev_rot: Option<Quat>,
    /// Smoothed angular velocity in the suit's frame (rad/s).
    spin: Vec3,
    /// The melee strike under way.
    pub swing: Option<Swing>,
    saber_flag: bool,
    /// The Planet Defensors' angle.
    orbit: f32,
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

    /// Where `bone`'s joint sits in its parent's frame, posed: at rest, but for the Dragon Fang's
    /// head out on its cable.
    pub fn rest(&self, bone: Bone) -> Vec3 {
        if bone == Bone::HandR { bone.rest() + self.reach } else { bone.rest() }
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

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Poses every suit's bones.
pub fn animate_suits(
    time: Res<VisTime>,
    lib: Res<SuitMeshLib>,
    events: Res<FxEvents>,
    mut suits: Query<(&SuitDrive, &SuitVisual, &mut Anim, Option<&Damage>)>,
    mut bones: Query<&mut Transform>,
) {
    let dt = time.dt.min(0.1);
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
        let bird = d.frame == FrameId::WingZeroBird;
        // The arm that holds the main weapon aims it (Deathscythe's buster shield is on the left).
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
            let chest = Vec3::new(0.0, aim.y * 0.25, 0.0);
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

            // AMBAC: limbs swing against the rotation.
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
                    Hands::Fang => {
                        let r = turn(Vec3::Z, sw.dir, 50f32.to_radians());
                        target[Bone::UpperArmR.index()] = (r - chest) * 0.45;
                        target[Bone::ForearmR.index()] = (r - chest) * 0.55;
                        stiff[Bone::UpperArmR.index()] = 30.0;
                        stiff[Bone::ForearmR.index()] = 30.0;
                    }
                }
            }
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
        // The fang's head out along the thrust, in the forearm's frame as it is now posed.
        a.reach = match a.swing {
            Some(sw) if sw.hands == Hands::Fang => {
                let forearm = a.world_rot(Bone::ForearmR);
                forearm.inverse() * sw.dir * sw.reach()
            }
            _ => Vec3::ZERO,
        };
        // The Planet Defensors circle the suit.
        a.orbit = (a.orbit + dt * 0.6) % std::f32::consts::TAU;
        a.rot[Bone::Props.index()] = Vec3::new(0.0, a.orbit, 0.0);

        // The fang's cable, from the wrist out to the head.
        if let Some(cable) = v.cable
            && let Ok(mut tf) = bones.get_mut(cable)
        {
            let len = a.reach.length();
            tf.translation = Bone::HandR.rest() + a.reach * 0.5;
            tf.rotation = Quat::from_rotation_arc(Vec3::Y, a.reach.normalize_or(Vec3::Y));
            tf.scale = Vec3::new(0.25, len.max(0.001), 0.25);
        }
        for (i, e) in v.bones.iter().enumerate() {
            if damage.is_some_and(|dmg| dmg.lost[i]) {
                continue; // broken off: no longer this suit's to pose
            }
            if let Ok(mut tf) = bones.get_mut(*e) {
                tf.rotation = Quat::from_scaled_axis(a.rot[i]);
                if i == Bone::HandR.index() {
                    tf.translation = a.rest(Bone::HandR);
                }
            }
        }
    }
}
