//! Aquila, the Altair folded into its flight form: the same parts ([`crate::altair`]) laid out as a
//! great white bird with the suit lying on its front, nose along +z. Its bones mean other things
//! here (the simulation's `BIRD` capsules): the head is the nose, the torso, chest and waist the
//! fuselage, the arms the wings, the legs the tail, the backpack the dorsal thrusters.
//!
//! - The nose: the shield laid face up, its yellow point forward and a little down, over a white
//!   keel; the head folded down in front of the chest under it, the V-fin's blades reaching forward
//!   either side of the keel. The cockpit's eye rides over the nose.
//! - The fuselage: the chest face down, the green sensor and the yellow vents under it, the blue
//!   plates of its back on top; the abdomen and the waist behind; a machine cannon turned forward
//!   at each of the chest's front corners as the nose guns.
//! - The wings: each binder's arm reaches forward along the back to its hinge over the arm, and its
//!   wing lies flat, swept back: the lower section's plates fanned out along the span to their yellow
//!   points, the upper section's blue and red plates laid over them and swept back; under each
//!   wing's leading edge the arm reaches out from the shoulder at the root, its wedge forward, the
//!   forearm folded back at the elbow, the fist tucked in behind.
//! - The tail: the legs drawn back and raised a little from the hips, the shins slid up over the
//!   thighs, the red feet turned to trail behind, soles down, their verniers quiet.
//! - The pack rides the back behind the chest, its two main thrusters turned to fire astern.
//! - The Twin Buster Rifle is split in two, a half slung under each wing's root along the nose;
//!   the beam leaves between their muzzles.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::altair::{
    Fold, HEEL_SOLE, HINGE, TOE_SOLE, abdomen, backpack, binder, buster, chest, fist, forearm, head, heel,
    quiet_sole, shield, shin, shoulder_armour, thigh, toe_cap, upper_arm, waist, wing_plane,
};

/// In flight each wing's sections both reach out along the span, the upper laid over the lower
/// and swept back from it.
const SPREAD: Fold = Fold { upper: 2.65, lower: 0.0 };
use crate::detail::band;
use crate::frames::{BODY, FRAME, GUN, TRIM, at, j, nozzle, place, rx, ry, sided, sp, v, v2};
use crate::rig::{Bone, Side};
use crate::{Designer, On};

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
/// Where the right half of the Twin Buster Rifle hangs under the wing's root: over its grip.
const BUSTER: Vec3 = Vec3::new(2.35, -1.2, -1.2);

/// Where each wing's hinge lies in flight (right side), at the root of its leading edge over the
/// arm, and the way its binder's arm reaches it, forward along the back.
const BIRD_HINGE: Vec3 = Vec3::new(2.6, 2.0, 0.6);
const BIRD_ARM: Vec3 = Vec3::new(0.25, -0.1, 0.96);
/// The way each wing reaches out in flight, swept back.
const BIRD_SPAN: Vec3 = Vec3::new(1.0, 0.0, -0.7);

/// Where the binder's arm and its wing lie in flight (right side), as placements of their rest
/// places: the arm forward along the back to the hinge, and the wing laid flat, its foot out and
/// swept back, its chord trailing astern, its outer face up.
fn spread() -> (Affine3A, Affine3A) {
    let root = j(Bone::WingR);
    let (reach, dir) = ((HINGE - root).length(), BIRD_ARM.normalize());
    let arm = place(BIRD_HINGE - dir * reach, Quat::from_rotation_arc((HINGE - root).normalize(), dir))
        * Affine3A::from_translation(-root);
    let w = wing_plane();
    let (chord, up) = (Vec3::from(w.matrix3.x_axis), Vec3::from(w.matrix3.y_axis));
    let from = Mat3::from_cols(-up, chord, chord.cross(up));
    let span = BIRD_SPAN.normalize();
    let aft = v(span.z, 0.0, -span.x);
    let to = Mat3::from_cols(span, aft, span.cross(aft));
    let wing = Affine3A::from_mat3_translation(to * from.transpose(), BIRD_HINGE)
        * Affine3A::from_translation(-HINGE);
    (arm, wing)
}

/// A machine cannon in its housing at `xf` (centred, its barrel along +z): a white box with a blue
/// lid, the barrel out of a dark port in its front, its muzzle ringed.
fn cannon(c: &mut On<'_>, xf: Affine3A) {
    c.block(v(0.78, 0.56, 2.0), v2(0.9, 0.92), Vec2::ZERO, 0.1, BODY, xf);
    c.cube(v(0.6, 0.1, 1.5), 0.03, TRIM, xf * at(0.0, 0.3, -0.12));
    c.cube(v(0.44, 0.36, 0.08), 0.02, FRAME, xf * at(0.0, -0.04, 1.0));
    let barrel = xf * place(v(0.0, -0.04, 1.3), rx(FRAC_PI_2));
    c.cylinder(0.12, 0.7, 10, GUN, barrel);
    c.lathe(&band(0.08, 0.17, 0.25, 0.42), 12, GUN, barrel);
    c.cylinder(0.08, 0.04, 8, FRAME, barrel * at(0.0, 0.43, 0.0));
}

pub(crate) fn aquila(d: &mut Designer) {
    let body = prone(BODY_Z);
    // The head folded down in front of the chest, under the shield, which is the nose: lying face
    // up, its point forward and a little down; a keel under its point.
    let mut h = d.on(Bone::Head);
    head(&mut h, body);
    shield(&mut h, place(v(0.0, 0.75, 6.4), rx(-FRAC_PI_2 + 0.14)));
    let keel = [
        (-0.7, -0.9, 5.0),
        (0.7, -0.9, 5.0),
        (-0.7, 0.55, 5.0),
        (0.7, 0.55, 5.0),
        (-0.15, 0.1, 9.0),
        (0.15, 0.1, 9.0),
        (-0.15, 0.4, 9.0),
        (0.15, 0.4, 9.0),
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
        let arm = place(v(3.0, 1.0, -0.6), ry(-1.273) * rx(FRAC_PI_2))
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

        // The binders' arms forward along the back, their wings spread flat over the arms.
        let (arm, wing) = spread();
        let bone = s.pick(Bone::WingL, Bone::WingR);
        binder(&mut d.on(bone), s, on_side(s, arm), on_side(s, wing), SPREAD);
    }

    // The pack on the back, its main thrusters turned to fire astern.
    let mut b = d.on(Bone::Backpack);
    backpack(&mut b, prone(PACK_Z));
    b.cube(v(1.9, 0.6, 1.0), 0.08, TRIM, at(0.0, 2.1, -2.1));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.62, 2.1, -2.55)), v(0.0, 0.05, -1.0), 0.5);
    }

    // The Twin Buster Rifle split in two, a half slung under each wing's root, along the nose; the
    // beam leaves between their muzzles.
    let mut g = d.on(Bone::Weapon);
    g.seed(0.61);
    for s in Side::BOTH {
        buster(&mut g, 1.0, sided(s, at(BUSTER.x, BUSTER.y, BUSTER.z)));
    }
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(0.0, BUSTER.y, BUSTER.z + 9.05));
    // Nothing is swung in this form; the saber's rest direction still has to be a direction.
    d.sockets.saber = (Vec3::ZERO, Vec3::Z);
}
