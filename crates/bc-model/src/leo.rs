//! OZ-06MS Leo (space type), drawn after Bandai's HG 1/144 Leo (Full Weapon Set): the bucket head
//! with its visor window and ribbed left side, round shoulders in open cuffs, bottle forearms, egg
//! thighs, riveted knee bands, flared ankle guards over wheeled feet, the space type's drum tank,
//! the long beam rifle, and the 105 mm rifle with its drum magazine riding the left forearm. Its
//! inner frame, hands and feet are brown; the wheels and the chest lamps' rims take the trim.
//!
//! The shared skeleton is stockier than the kit (its knee sits lower), so the ankle guards and
//! feet stand tall to keep the Leo's long lower legs.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_8};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::frames::{
    BODY, EYE, FRAME, GLASS, GUN, STEEL, TRIM, at, j, nozzle, place, rx, ry, rz, saber_hilt, sided, sp, v, v2,
};
use crate::kit::Paint;
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The inner frame: neck, joints, hands and feet.
const BROWN: Paint = Paint::Fixed(paint::FRAME_BROWN);

/// Local +y along `dir`.
fn toward(dir: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, dir.normalize())
}

fn deg(a: f32) -> f32 {
    a.to_radians()
}

/// A closed rectangle in a lathe's (radius, height) plane, for rings and ribs.
fn band(r0: f32, r1: f32, y0: f32, y1: f32) -> [(f32, f32); 5] {
    [(r0, y0), (r1, y0), (r1, y1), (r0, y1), (r0, y0)]
}

/// The kit's engraved "U" (a notch in the skirts and shins): three dark bars, opening up, on the
/// face at `xf`'s origin looking along its +z.
fn u_mark(o: &mut On<'_>, w: f32, h: f32, xf: Affine3A) {
    o.greeble(|o| {
        let t = 0.07;
        for x in [-0.5, 0.5] {
            o.cube(v(t, h, 0.05), 0.0, FRAME, xf * at(x * (w - t), 0.0, 0.0));
        }
        o.cube(v(w, t, 0.05), 0.0, FRAME, xf * at(0.0, -(h - t) * 0.5, 0.0));
    });
}

pub(crate) fn leo(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    legs(d);
    backpack(d);
    beam_rifle(d);
    machine_gun(d);
    saber_hilt(d);
}

// --- Head: A1-A3, C7, C17, C18. ---

/// The helmet's profile round its axis: a bucket with a domed crown.
const DOME: [(f32, f32); 9] = [
    (0.0, 6.0),
    (0.84, 6.0),
    (0.91, 6.13),
    (0.93, 7.0),
    (0.9, 7.33),
    (0.78, 7.6),
    (0.55, 7.78),
    (0.28, 7.85),
    (0.0, 7.87),
];
/// The helmet's axis (it sits a little back on the neck).
const HEAD_Z: f32 = 0.05;

fn head(d: &mut Designer) {
    let axis = at(0.0, 0.0, HEAD_Z);
    let mut h = d.on(Bone::Head);
    h.seed(0.07);
    h.cylinder(0.42, 0.8, 12, BROWN, at(0.0, 5.85, HEAD_Z));
    h.lathe(&DOME, 20, BODY, axis);
    // The visor frame down the face; the window in its upper half, the mono-eye behind the glass,
    // and a vent slot under it.
    h.block(v(0.92, 1.42, 0.48), v2(0.92, 0.88), v2(0.0, -0.04), 0.06, BODY, at(0.0, 6.73, 0.8));
    h.cube(v(0.6, 0.52, 0.08), 0.03, GLASS, at(0.0, 7.06, 0.99));
    h.sphere(0.12, 5, EYE, at(0.0, 7.06, 1.0));
    h.cube(v(0.52, 0.18, 0.06), 0.02, FRAME, at(0.0, 6.42, 1.03));
    // The crest: a ridge from the brow over the crown and down the back.
    h.extrude(&crest(), 0.2, BODY, place(v(0.0, 0.0, HEAD_Z), ry(-FRAC_PI_2)));
    // A plain cover on its right; on its left and round the back, the ribbed one.
    h.lathe_arc(&band(0.9, 0.955, 6.25, 7.2), 20, deg(-30.0), deg(50.0), BODY, axis);
    h.lathe_arc(&band(0.9, 0.96, 6.25, 7.2), 20, deg(130.0), deg(330.0), BODY, axis);
    h.greeble(|h| {
        for k in 0..6 {
            let y = 6.32 + k as f32 * 0.15;
            h.lathe_arc(&band(0.94, 1.01, y, y + 0.08), 20, deg(132.0), deg(328.0), BODY, axis);
        }
    });
    // The cockpit's camera is the mono-eye.
    d.sockets.eye = Designer::local(Bone::Head, v(0.0, 7.06, 1.04));
}

/// The crest's outline in the head's (z, y): the dome's silhouette from the brow back, thickened.
fn crest() -> Vec<Vec2> {
    // The silhouette: the crown's front half, then its back half down to the ribs.
    let front: Vec<Vec2> = DOME[5..].iter().map(|&(r, y)| v2(r, y)).collect();
    let back: Vec<Vec2> = DOME[2..DOME.len() - 1].iter().rev().map(|&(r, y)| v2(-r, y)).collect();
    let line: Vec<Vec2> = front.into_iter().chain(back).collect();
    let n = line.len();
    let normal = |i: usize| {
        let a = line[i.saturating_sub(1)];
        let b = line[(i + 1).min(n - 1)];
        let t = (b - a).normalize_or_zero();
        // Outward: the silhouette runs counter-clockwise (front to back over the top).
        v2(-t.y, t.x) * -1.0
    };
    let outer = (0..n).map(|i| line[i] + normal(i) * 0.05);
    let inner: Vec<Vec2> = (0..n).map(|i| line[i] - normal(i) * 0.06).collect();
    outer.chain(inner.into_iter().rev()).collect()
}

// --- Chest (A7, A8, C3) and abdomen (A9, A10). ---

/// The chest's right half in two tiers, each a hexahedron with flat faces: the lower one swells to
/// a prow and tucks in toward the belt, the upper leans back to the collar, so the two halves meet
/// in a crease down the middle and a seam runs across under the lamps. Each tier is given by its
/// bottom and top sections: (y, outer x, back z, front z at the middle); the front falls away
/// outboard at `CHEST_FALL` per metre.
const CHEST_TIERS: [[(f32, f32, f32, f32); 2]; 2] = [
    [(3.55, 1.45, -1.4, 1.42), (4.15, 1.8, -1.55, 1.62)],
    [(4.15, 1.8, -1.55, 1.62), (5.75, 1.7, -1.4, 1.1)],
];
const CHEST_FALL: f32 = 0.257;

/// A chest tier's corners.
fn chest_tier([(y0, x0, b0, f0), (y1, x1, b1, f1)]: [(f32, f32, f32, f32); 2]) -> [Vec3; 8] {
    [
        v(0.0, y0, b0),
        v(x0, y0, b0),
        v(0.0, y1, b1),
        v(x1, y1, b1),
        v(0.0, y0, f0),
        v(x0, y0, f0 - CHEST_FALL * x0),
        v(0.0, y1, f1),
        v(x1, y1, f1 - CHEST_FALL * x1),
    ]
}

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.14);
    for s in Side::BOTH {
        for tier in CHEST_TIERS {
            c.hexa(chest_tier(tier), 0.14, BODY, sided(s, Affine3A::IDENTITY));
        }
    }
    // The collar round the neck, and the belt under the chest with its hatch line.
    c.lathe(&band(0.72, 1.07, 5.62, 6.05), 18, BODY, at(0.0, 0.0, HEAD_Z));
    c.block(v(2.7, 0.5, 2.6), v2(1.0, 1.0), Vec2::ZERO, 0.08, BODY, at(0.0, 3.35, -0.02));
    c.cube(v(1.3, 0.15, 0.1), 0.02, TRIM, at(0.0, 3.35, 1.37));
    for s in Side::BOTH {
        // The sensor lamps flanking the head, in their square housings.
        c.cube(v(0.72, 0.68, 0.6), 0.06, BODY, sided(s, at(0.97, 5.45, 1.02)));
        let lens = sided(s, place(v(0.97, 5.45, 1.34), rx(FRAC_PI_2)));
        c.cylinder(0.24, 0.14, 14, TRIM, lens);
        c.cylinder(0.12, 0.2, 10, GLASS, lens);
        c.greeble(|c| {
            c.cube(v(0.36, 0.03, 0.03), 0.0, FRAME, sided(s, at(0.97, 5.45, 1.44)));
            c.cube(v(0.03, 0.36, 0.03), 0.0, FRAME, sided(s, at(0.97, 5.45, 1.44)));
        });
        // The shoulder's socket in the chest's side.
        c.cylinder(0.7, 0.4, 14, BROWN, sided(s, place(v(1.8, 5.3, -0.05), rz(FRAC_PI_2))));
        // The boxes high on the back, either side of the backpack's mount.
        c.block(v(0.8, 1.1, 0.7), v2(0.9, 0.9), Vec2::ZERO, 0.08, BODY, sided(s, at(1.25, 5.05, -1.55)));
    }
    c.block(v(2.4, 1.7, 0.3), v2(0.95, 0.95), Vec2::ZERO, 0.06, BODY, at(0.0, 4.7, -1.62));

    // The abdomen: a short bell under the belt, flaring wider than the chest where it sits in the
    // waist.
    d.on(Bone::Torso).seed(0.16).lathe(
        &[
            (0.0, 1.85),
            (1.55, 1.85),
            (1.75, 1.98),
            (1.78, 2.15),
            (1.6, 2.45),
            (1.3, 2.8),
            (1.2, 3.15),
            (0.0, 3.15),
        ],
        20,
        BODY,
        Affine3A::from_scale(v(1.0, 1.0, 0.85)),
    );
}

// --- Waist: A13-A17, C1, C2, C19, C20. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    // It rides high, as the kit's does: the abdomen is short.
    w.block(v(3.3, 0.85, 2.5), v2(1.0, 1.0), Vec2::ZERO, 0.1, BODY, at(0.0, 1.65, 0.0));
    // The crotch block and its round fitting.
    w.block(v(0.95, 2.4, 1.35), v2(1.2, 1.0), Vec2::ZERO, 0.1, BODY, at(0.0, 0.35, 0.45));
    let nub = place(v(0.0, -0.15, 1.18), rx(FRAC_PI_2));
    w.cylinder(0.36, 0.42, 14, BODY, nub);
    w.cylinder(0.22, 0.5, 10, FRAME, nub);
    for s in Side::BOTH {
        // Front skirts, angled out over the thighs.
        let front = sided(s, place(v(1.12, 0.9, 1.22), rz(0.08) * rx(-0.12)));
        w.block(v(1.35, 2.0, 0.36), v2(0.95, 0.9), Vec2::ZERO, 0.08, BODY, front);
        u_mark(&mut w, 0.45, 0.42, front * at(0.0, -0.4, 0.19));
        // Side skirts: boxes standing proud of the waist, flaring at the hem.
        let side = sided(s, place(v(2.4, 1.15, 0.0), rz(0.1)));
        w.block(v(0.75, 1.7, 1.9), v2(0.9, 0.85), Vec2::ZERO, 0.1, BODY, side);
        u_mark(&mut w, 0.5, 0.4, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.2, 0.39));
        // Rear skirts.
        let rear = sided(s, place(v(0.8, 0.95, -1.3), rz(0.06) * rx(0.15)));
        w.block(v(1.3, 1.7, 0.36), v2(0.95, 0.9), Vec2::ZERO, 0.08, BODY, rear);
        u_mark(
            &mut w,
            0.42,
            0.4,
            rear * Affine3A::from_rotation_y(std::f32::consts::PI) * at(0.0, -0.2, 0.19),
        );
    }
}

// --- Arms: B3, B6, B7, A11, A12, A18-A20, C10, C11 and the hands. ---

/// The shoulder ball's centre (right side): well above the arm's joint, as the kit's big round
/// shoulders sit high beside the head.
const SHOULDER: Vec3 = Vec3::new(3.05, 5.5, 0.0);
const SHOULDER_R: f32 = 1.15;

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The ball, and the cuff round its top: a short tube tipped outward and back, open toward
        // the head.
        let mut o = d.on(shoulder);
        o.seed(0.25 + k);
        o.sphere(SHOULDER_R, 10, BODY, sided(s, Affine3A::from_translation(SHOULDER)));
        let tilt = rz(-0.38) * rx(-0.22);
        let cuff = sided(s, place(SHOULDER + tilt * v(0.0, SHOULDER_R * 0.25, 0.0), tilt));
        o.lathe_arc(&band(1.17, 1.3, -0.68, 0.68), 22, deg(200.0), deg(520.0), BODY, cuff);
        o.cylinder(0.45, 0.8, 10, BROWN, sided(s, place(v(2.1, SHOULDER.y, 0.0), rz(FRAC_PI_2))));

        // The upper arm: a short drum under the ball, on the frame.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.42, 1.6, 10, BROWN, up_xf * at(0.0, 0.9, 0.0));
        u.lathe(&[(0.0, 0.9), (0.6, 0.9), (0.68, 1.0), (0.68, 1.9), (0.6, 2.0), (0.0, 2.0)], 16, BODY, up_xf);

        // The forearm: an elbow collar with a pad outboard, then the bottle down to the wrist.
        let dir = el - wr;
        let fo_xf = sided(s, place(wr, toward(dir)));
        let collar = sided(s, place(wr + dir.normalize() * 1.95, toward(dir)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.5, 1.05, 12, BROWN, sided(s, place(el, rz(FRAC_PI_2))));
        f.lathe(
            &[
                (0.0, -0.02),
                (0.5, -0.02),
                (0.64, 0.12),
                (0.74, 0.4),
                (0.77, 0.75),
                (0.71, 1.15),
                (0.6, 1.5),
                (0.56, 1.66),
                (0.0, 1.66),
            ],
            18,
            BODY,
            fo_xf,
        );
        f.block(v(1.3, 0.7, 1.3), v2(0.95, 0.95), Vec2::ZERO, 0.1, BODY, collar);
        f.block(v(0.3, 0.85, 1.0), v2(1.0, 0.9), Vec2::ZERO, 0.06, BODY, collar * at(0.75, 0.05, 0.0));
        f.greeble(|f| {
            f.cube(v(0.06, 0.5, 0.6), 0.0, FRAME, collar * at(0.91, 0.05, 0.0));
        });
        f.cylinder(0.48, 0.3, 12, BROWN, fo_xf);
    }
    hands(d);
}

/// Brown fists, each with an armoured guard on its back.
fn hands(d: &mut Designer) {
    let w = j(Bone::HandR);
    for s in Side::BOTH {
        let mut h = d.on(s.pick(Bone::HandL, Bone::HandR));
        h.seed(0.4 + s.pick(0.0, 0.3));
        h.cube(v(0.78, 0.95, 0.92), 0.08, BROWN, sided(s, at(w.x, w.y - 0.45, w.z + 0.1)));
        h.block(
            v(0.2, 0.78, 0.86),
            v2(0.8, 0.9),
            Vec2::ZERO,
            0.05,
            BODY,
            sided(s, at(w.x + 0.46, w.y - 0.42, w.z + 0.12)),
        );
        h.greeble(|h| {
            for f in 0..4 {
                let z = w.z + 0.45 - f as f32 * 0.28;
                h.cube(v(0.3, 0.26, 0.24), 0.04, BROWN, sided(s, place(v(w.x - 0.3, w.y - 1.0, z), rz(0.5))));
            }
            h.cube(
                v(0.26, 0.5, 0.26),
                0.04,
                BROWN,
                sided(s, place(v(w.x - 0.2, w.y - 0.55, w.z + 0.62), rx(0.6))),
            );
        });
    }
}

// --- Legs: A21-A31, B9-B16, C14, C15. ---

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let k = s.pick(0.0, 0.2);

        // The thigh: an egg, fullest high up and swelling outboard, on a brown ball at the hip.
        let mut t = d.on(thigh);
        t.seed(0.31 + k);
        t.sphere(0.75, 6, BROWN, sided(s, Affine3A::from_translation(hip)));
        let egg = sided(
            s,
            place(knee + v(0.08, 0.0, 0.0), toward(hip - knee)) * Affine3A::from_scale(v(0.95, 1.0, 1.08)),
        );
        t.lathe(
            &[
                (0.0, 0.6),
                (0.68, 0.65),
                (0.86, 1.0),
                (1.02, 1.7),
                (1.14, 2.5),
                (1.2, 3.05),
                (1.1, 3.55),
                (0.8, 3.95),
                (0.0, 4.1),
            ],
            18,
            BODY,
            egg,
        );
        t.greeble(|t| {
            t.cube(v(0.08, 0.5, 0.06), 0.0, FRAME, sided(s, place(v(1.75, -1.55, 1.2), rz(-0.55) * ry(0.3))));
            t.cube(
                v(0.08, 0.5, 0.06),
                0.0,
                FRAME,
                sided(s, place(v(0.95, -2.75, 1.3), rz(-0.55) * ry(-0.15))),
            );
        });

        // The shin: knee joint, the riveted band, the tube with a ridge down its front and the
        // booster slot down its back, and the flared guard over the ankle.
        let dir = knee - ankle;
        let tube = sided(s, place(ankle, toward(dir)));
        let at_shin = |h: f32| ankle + dir.normalize() * h;
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.6, 1.3, 12, BROWN, sided(s, place(knee, rz(FRAC_PI_2))));
        m.lathe(
            &[
                (0.0, 0.75),
                (0.74, 0.75),
                (0.76, 1.3),
                (0.8, 2.0),
                (0.88, 2.7),
                (0.95, 3.2),
                (0.95, 3.45),
                (0.0, 3.5),
            ],
            18,
            BODY,
            tube * Affine3A::from_scale(v(0.92, 1.0, 1.0)),
        );
        m.block(
            v(0.4, 1.8, 0.4),
            v2(1.0, 1.0),
            Vec2::ZERO,
            0.05,
            BODY,
            sided(s, place(at_shin(1.7) + v(0.0, 0.0, 0.66), toward(dir) * ry(FRAC_PI_4))),
        );
        // The band sits a little above the joint, where the kit's knee is.
        let band_at = knee + v(0.08, 0.25, 0.05);
        m.cube(v(2.25, 0.9, 2.1), 0.12, BODY, sided(s, Affine3A::from_translation(band_at)));
        m.greeble(|m| {
            for (dx, dy) in [(0.3, 0.2), (0.74, 0.2), (0.3, -0.2), (0.74, -0.2)] {
                let p = band_at + v(dx, dy, 1.07);
                m.cylinder(0.17, 0.12, 10, BODY, sided(s, place(p, rx(FRAC_PI_2))));
            }
        });
        u_mark(&mut m, 0.36, 0.34, sided(s, place(at_shin(2.95) + v(0.0, 0.0, 0.86), rx(-0.06))));
        // The booster hardpoint: a dark slot between two fins.
        let slot = at_shin(1.75) + v(0.0, 0.0, -0.72);
        m.cube(v(0.5, 1.7, 0.3), 0.04, BROWN, sided(s, place(slot, toward(dir))));
        for side in [-1.0, 1.0] {
            m.block(
                v(0.16, 1.9, 0.42),
                v2(1.0, 0.8),
                Vec2::ZERO,
                0.03,
                BODY,
                sided(s, place(slot + v(side * 0.36, 0.0, -0.05), toward(dir) * ry(side * -0.5))),
            );
        }
        m.lathe(
            &[
                (0.0, -6.55),
                (0.86, -6.55),
                (0.92, -6.7),
                (1.1, -7.35),
                (1.2, -7.75),
                (1.16, -7.85),
                (0.0, -7.85),
            ],
            8,
            BODY,
            sided(
                s,
                at(hip.x, 0.0, 0.15)
                    * Affine3A::from_scale(v(1.0, 1.0, 1.12))
                    * Affine3A::from_rotation_y(FRAC_PI_8),
            ),
        );

        // The foot: a brown wedge on a dark sole, a green tongue up into the guard, and a wheel
        // either side of the heel.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, BROWN, sided(s, Affine3A::from_translation(ankle)));
        f.block(
            v(1.6, 0.8, 3.3),
            v2(0.85, 0.6),
            v2(0.0, -0.55),
            0.12,
            BROWN,
            sided(s, at(hip.x, -8.5, 0.45)),
        );
        f.block(v(1.4, 0.7, 0.9), v2(0.9, 0.85), Vec2::ZERO, 0.08, BROWN, sided(s, at(hip.x, -8.35, -0.95)));
        f.cube(v(1.7, 0.2, 3.45), 0.04, FRAME, sided(s, at(hip.x, -8.975, 0.45)));
        f.block(
            v(0.6, 1.0, 0.32),
            v2(0.85, 1.0),
            Vec2::ZERO,
            0.05,
            BODY,
            sided(s, place(v(hip.x, -7.9, 1.25), rx(-0.4))),
        );
        u_mark(&mut f, 0.4, 0.3, sided(s, place(v(hip.x, -8.42, 1.45), rx(-1.0))));
        for side in [-1.0, 1.0] {
            let hub = sided(s, place(v(hip.x + side * 0.9, -8.42, -0.55), rz(FRAC_PI_2)));
            f.cylinder(0.52, 0.24, 16, TRIM, hub);
            f.cylinder(0.2, 0.3, 8, FRAME, hub);
        }
    }
}

// --- The space type's backpack: G1, a propellant drum across the back. ---

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    b.block(v(1.7, 2.3, 1.0), v2(0.9, 0.9), Vec2::ZERO, 0.12, BODY, at(0.0, 4.35, -2.0));
    b.block(v(1.2, 0.8, 0.5), v2(0.9, 0.9), Vec2::ZERO, 0.06, BODY, at(0.0, 5.25, -2.5));
    // The bracket under the drum that carries the main thrusters.
    b.block(v(1.7, 0.5, 0.8), v2(1.0, 1.0), Vec2::ZERO, 0.06, BODY, at(0.0, 2.55, -2.7));
    let drum = place(v(0.0, 3.3, -2.9), rz(FRAC_PI_2));
    b.lathe(
        &[
            (0.0, -3.55),
            (0.42, -3.55),
            (0.7, -3.45),
            (0.84, -3.25),
            (0.86, -3.0),
            (0.86, 3.0),
            (0.84, 3.25),
            (0.7, 3.45),
            (0.42, 3.55),
            (0.0, 3.55),
        ],
        20,
        BODY,
        drum,
    );
    b.lathe(&band(0.86, 0.98, -0.42, 0.42), 20, BODY, drum);
    b.cube(v(0.6, 0.5, 0.3), 0.04, TRIM, at(0.0, 3.3, -3.92));
    for end in [-3.6, 3.6] {
        b.cylinder(0.32, 0.2, 12, GUN, drum * at(0.0, end, 0.0));
    }
    b.greeble(|b| {
        for y in [1.05, 1.75, 2.45] {
            for side in [-1.0, 1.0] {
                let y = side * y;
                b.lathe(&[(0.875, y - 0.03), (0.875, y + 0.03)], 20, FRAME, drum);
            }
        }
    });
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.55, 2.4, -2.75)), v(0.0, -0.6, -0.8), 0.45);
    }
}

// --- Weapons. ---

/// The long type beam rifle (F1 12-13): a boxy receiver with a carry handle, the flat energy pack
/// before the grip, and a ribbed barrel shroud.
fn beam_rifle(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let along_z = |y: f32, z: f32| place(v(g.x, g.y + y, g.z + z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    w.cube(v(0.32, 0.95, 0.42), 0.05, GUN, place(v(g.x, g.y - 0.2, g.z), rx(-0.25)));
    w.block(v(0.64, 0.9, 3.0), v2(0.95, 0.95), Vec2::ZERO, 0.08, GUN, at(g.x, g.y + 0.6, g.z + 0.55));
    w.block(v(0.6, 0.7, 0.6), v2(0.9, 0.9), Vec2::ZERO, 0.06, GUN, at(g.x, g.y + 0.55, g.z - 1.15));
    w.block(v(0.4, 1.35, 1.05), v2(1.0, 1.0), Vec2::ZERO, 0.05, GUN, at(g.x, g.y - 0.05, g.z + 1.3));
    for z in [0.0, 1.2] {
        w.cube(v(0.16, 0.45, 0.18), 0.02, GUN, at(g.x, g.y + 1.25, g.z + z));
    }
    w.cube(v(0.16, 0.14, 1.36), 0.02, GUN, at(g.x, g.y + 1.45, g.z + 0.6));
    w.cylinder(0.34, 2.6, 14, GUN, along_z(0.62, 3.35));
    w.greeble(|w| {
        for k in 0..7 {
            w.cylinder(0.39, 0.12, 14, GUN, along_z(0.62, 2.3 + k as f32 * 0.36));
        }
    });
    w.cylinder(0.18, 1.0, 10, STEEL, along_z(0.62, 5.15));
    w.cylinder(0.27, 0.3, 12, GUN, along_z(0.62, 5.7));
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + 0.62, g.z + 5.85));
}

/// The 105 mm rifle (C4-C6, C12, C13, C16) clamped along the outside of the left forearm: its
/// perforated jacket forward, the drum magazine hung under the receiver. Described on the right
/// and mirrored; it rides low on the forearm, under the hangar's catwalk.
fn machine_gun(d: &mut Designer) {
    let l = |xf: Affine3A| sided(Side::L, xf);
    let along_z = |y: f32, z: f32| l(place(v(4.42, y, z), rx(FRAC_PI_2)));
    let mut f = d.on(Bone::ForearmL);
    f.seed(0.66);
    f.block(v(0.5, 0.6, 2.0), v2(0.95, 0.95), Vec2::ZERO, 0.06, GUN, l(at(4.42, 1.08, 0.95)));
    f.cube(v(0.5, 0.45, 0.7), 0.04, GUN, l(at(4.12, 1.08, 0.9)));
    f.cylinder(0.24, 1.5, 12, GUN, along_z(1.12, 2.7));
    f.greeble(|f| {
        for k in 0..4 {
            f.cube(v(0.06, 0.12, 0.2), 0.0, FRAME, l(at(4.66, 1.12, 2.2 + k as f32 * 0.4)));
        }
    });
    f.cylinder(0.12, 0.55, 8, STEEL, along_z(1.12, 3.7));
    f.cylinder(0.18, 0.25, 10, GUN, along_z(1.12, 4.05));
    f.cylinder(0.58, 0.42, 18, GUN, l(place(v(4.42, 0.5, 1.15), rz(FRAC_PI_2))));
}
