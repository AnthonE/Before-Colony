//! Aquila, the Altair folded into its flight form: the same parts ([`crate::altair`]) laid out as a
//! great white bird with the suit lying on its front, nose along +z. Its bones mean other things
//! here (the simulation's `BIRD` capsules): the head is the nose, the torso, chest and waist the
//! fuselage, the arms the wings, the legs the tail, the backpack the dorsal thrusters.
//!
//! - The nose: the shield laid face up, its point forward and a little down, over a white keel; the
//!   head folded down in front of the chest under it, the V-fin's blades reaching forward either
//!   side of the keel. The cockpit's eye rides over the nose.
//! - The fuselage: the chest face down, the yellow vents under it, the blue plates of its back on
//!   top; the abdomen and the waist behind; the machine cannons turned forward at the chest's front
//!   corners as the nose guns.
//! - The wings: the binders on the pack spread their feathered wings flat, each upper wing's spar
//!   out to its wrist and swept a little back, the long feathers trailing from the wrist and the
//!   spar, the lower wing back along the body before the tail. Under each wing's leading edge the
//!   arm reaches out from the shoulder at the root, the shoulder's outer plate forward; the
//!   forearm folds back at the elbow, the fist tucked in behind.
//! - The tail: the legs drawn back and raised a little from the hips, the shins slid up over the
//!   thighs, the feet turned to trail behind, soles down, their verniers quiet.
//! - The pack rides the back behind the chest, its two main thrusters turned to fire astern.
//! - The Heavy Beam Rifle is slung whole under the fuselage, along the nose, and fires between
//!   its muzzles.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Affine3A, Quat, Vec3};

use crate::Designer;
use crate::altair::{
    Fan, HEEL_SOLE, TOE_SOLE, abdomen, backpack, binder, cannon, chest, fist, forearm, head, heel,
    quiet_sole, shield, shin, shoulder_armour, thigh, toe_cap, twin_rifle, upper_arm, waist,
};
use crate::frames::{BODY, TRIM, at, j, nozzle, place, rx, ry, rz, sided, sp, v};
use crate::rig::{Bone, Side};

/// The Altair lying on its front with its head forward, `dz` along z: its up along +z, its front
/// down. (x, y, z) goes to (x, -z, y + dz).
fn prone(dz: f32) -> Affine3A {
    place(v(0.0, 0.0, dz), rx(FRAC_PI_2))
}

/// A placement described for the right side, on side `s`: mirrored across x both before and after.
fn on_side(s: Side, xf: Affine3A) -> Affine3A {
    if s == Side::L {
        let m = Affine3A::from_scale(v(-1.0, 1.0, 1.0));
        m * xf * m
    } else {
        xf
    }
}

/// Where the body lies: the chest, the abdomen, the waist and the head folded down in front of the
/// chest.
const BODY_Z: f32 = -2.4;
/// The legs, drawn back as the tail: the thighs from under the waist, the shins slid up over them.
const THIGH_Z: f32 = -3.35;
const SHIN_Z: f32 = -0.85;
/// How far the tail rises from the hips, radians.
const TAIL_RISE: f32 = 0.08;
/// The pack on the back, behind the chest.
const PACK_Z: f32 = -5.0;

/// The upper wing in flight: its spar out to the wrist, swept a little back, the long feathers
/// trailing from the wrist and the spar.
const BIRD_UPPER: Fan = Fan {
    spar: (-8.0, 5.0),
    primaries: &[
        (1.0, 2.0, 3.2),
        (0.96, -14.0, 3.9),
        (0.9, -30.0, 4.5),
        (0.82, -47.0, 4.9),
        (0.72, -63.0, 5.0),
        (0.6, -77.0, 4.9),
        (0.46, -88.0, 4.6),
        (0.32, -96.0, 4.1),
        (0.18, -102.0, 3.5),
    ],
    coverts: &[
        (0.9, -38.0, 2.4),
        (0.72, -60.0, 2.6),
        (0.55, -76.0, 2.5),
        (0.38, -88.0, 2.2),
        (0.22, -97.0, 1.9),
    ],
};

/// The lower wing in flight, under the upper and back along the body, a fan before the tail.
const BIRD_LOWER: Fan = Fan {
    spar: (-70.0, 2.2),
    primaries: &[(1.0, -55.0, 3.4), (0.85, -68.0, 3.8), (0.7, -80.0, 3.8), (0.55, -92.0, 3.4)],
    coverts: &[(0.8, -75.0, 1.8)],
};

pub(crate) fn aquila(d: &mut Designer) {
    let body = prone(BODY_Z);
    // The head folded down in front of the chest, under the shield, which is the nose: lying face
    // up, its point forward and a little down; a keel under its point.
    let mut h = d.on(Bone::Head);
    head(&mut h, body);
    shield(&mut h, place(v(0.0, 0.95, 6.2), rx(-FRAC_PI_2 + 0.12)));
    let keel = [
        (-0.7, -0.9, 5.0),
        (0.7, -0.9, 5.0),
        (-0.7, 0.75, 5.0),
        (0.7, 0.75, 5.0),
        (-0.15, 0.3, 8.6),
        (0.15, 0.3, 8.6),
        (-0.15, 0.7, 8.6),
        (0.15, 0.7, 8.6),
    ]
    .map(|(x, y, z)| v(x, y, z));
    h.hexa(keel, 0.1, BODY, Affine3A::IDENTITY);
    // The cockpit view looks out from over the nose.
    d.sockets.eye = Designer::local(Bone::Head, v(0.0, 2.4, 4.6));
    d.sockets.eye_in_head = false;

    // The fuselage: the chest face down, the abdomen and the waist behind it; the machine cannons
    // turned forward at the chest's front corners as the nose guns.
    let mut c = d.on(Bone::Chest);
    chest(&mut c, body);
    for s in Side::BOTH {
        cannon(&mut c, sided(s, at(1.7, 0.6, 3.7)));
    }
    abdomen(&mut d.on(Bone::Torso), body);
    waist(&mut d.on(Bone::Waist), body);

    for s in Side::BOTH {
        // The arms out along the wings' leading edges, under the feathers: the shoulders at their
        // roots, their outer plates forward; the forearms folded back under the wings at the elbow,
        // the fists tucked in behind.
        let arm = place(v(2.7, 1.2, -0.2), ry(-1.273) * rx(FRAC_PI_2))
            * Affine3A::from_translation(-j(Bone::UpperArmR));
        let elbow = arm.transform_point3(j(Bone::ForearmR));
        let fold = Quat::from_rotation_arc(v(0.956, 0.0, -0.294), v(0.3, -0.1, -0.95).normalize());
        let folded = place(elbow, fold) * Affine3A::from_translation(-elbow) * arm;
        let (arm, folded) = (on_side(s, arm), on_side(s, folded));
        let (shoulder, up, fore, hand) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL, Bone::HandL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR, Bone::HandR),
        );
        shoulder_armour(&mut d.on(shoulder), s, arm);
        upper_arm(&mut d.on(up), s, arm);
        forearm(&mut d.on(fore), s, folded);
        fist(&mut d.on(hand), s, folded);

        // The tail: the legs drawn back and raised a little from the hips, the shins slid up over
        // the thighs, the feet turned to trail behind, soles down.
        let (thigh_bone, shin_bone, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let hip = prone(THIGH_Z).transform_point3(j(Bone::ThighR));
        let tail = |xf: Affine3A| place(hip, rx(TAIL_RISE)) * Affine3A::from_translation(-hip) * xf;
        thigh(&mut d.on(thigh_bone), s, on_side(s, tail(prone(THIGH_Z))));
        shin(&mut d.on(shin_bone), s, on_side(s, tail(prone(SHIN_Z))));
        let ankle = j(Bone::FootR);
        let heel_end =
            place(prone(SHIN_Z).transform_point3(ankle), ry(PI)) * Affine3A::from_translation(-ankle);
        let turned = on_side(s, tail(heel_end));
        let x = ankle.x;
        let mut f = d.on(foot);
        heel(&mut f, s, turned);
        quiet_sole(&mut f, s, x, HEEL_SOLE, turned);
        let mut t = d.on(toe);
        toe_cap(&mut t, s, turned);
        quiet_sole(&mut t, s, x, TOE_SOLE, turned);

        // The binders on the pack, their wings spread flat as the bird's.
        let root = prone(PACK_Z).transform_point3(j(Bone::WingR));
        let spread = on_side(
            s,
            place(root + v(0.0, -0.9, 0.0), rz(-0.3)) * Affine3A::from_translation(-root) * prone(PACK_Z),
        );
        binder(&mut d.on(s.pick(Bone::WingL, Bone::WingR)), s, spread, &BIRD_UPPER, &BIRD_LOWER);
    }

    // The pack on the back, its main thrusters turned to fire astern.
    let mut b = d.on(Bone::Backpack);
    backpack(&mut b, prone(PACK_Z));
    b.cube(v(1.9, 0.6, 1.0), 0.08, TRIM, at(0.0, 2.1, -2.1));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.62, 2.1, -2.55)), v(0.0, 0.05, -1.0), 0.5);
    }

    // The Heavy Beam Rifle slung under the fuselage, along the nose.
    let muzzle = twin_rifle(&mut d.on(Bone::Weapon), place(v(0.0, -2.55, -0.9), Quat::IDENTITY));
    d.sockets.muzzle = Designer::local(Bone::Weapon, muzzle);
    // Nothing is swung in this form; the saber's rest direction still has to be a direction.
    d.sockets.saber = (Vec3::ZERO, Vec3::Z);
}
