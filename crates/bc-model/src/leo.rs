//! OZ-06MS Leo (space type), drawn after its line art and Bandai's HG 1/144 and Robot Spirits
//! figures: the round head sunk in its chest, under a domed cap whose peak shades the amber visor
//! and the mono-eye behind it, red ribs wrapped round its sides and back; the two big lamps either
//! side of it; brown shoulder balls under green armour; boxy forearms with their elbow guards and
//! brown fists; the grey ring in the crotch and the two thrusters under the rear skirts; egg thighs,
//! knee blocks with four rivets, long shins swelling to the calf and pinched in over the ankle,
//! tall two-tier ankle guards with a tab up the front and a wheel either side, and brown feet in
//! three blocks (the heel, the instep and a toe falling away to its tip) under the green instep
//! armour. The space type's propellant drum rides across its back; it carries the long beam rifle,
//! and the 105 mm rifle with its drum magazine rides the left forearm.
//!
//! Its inner frame, shoulders, hands and feet are brown; the wheels, the crotch ring, the hatch's
//! handle and the lamps' rims take the trim.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::frames::{
    BODY, EYE, FRAME, GUN, STEEL, TRIM, at, j, nozzle, place, rx, ry, rz, saber_hilt, sided, sp, v, v2,
};
use crate::kit::{Light, Paint};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The inner frame: neck, joints, shoulder balls, hands and feet.
const BROWN: Paint = Paint::Fixed(paint::FRAME_BROWN);
const RED: Paint = Paint::Fixed(paint::RIB_RED);
const VISOR: Paint = Paint::Fixed(paint::VISOR_AMBER);

/// Local +y along `dir`.
fn toward(dir: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, dir.normalize())
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

/// A short engraved slot on the face at `xf`'s origin (looking along its +z), along its y.
fn slot(o: &mut On<'_>, len: f32, xf: Affine3A) {
    o.greeble(|o| {
        o.cube(v(0.07, len, 0.05), 0.0, FRAME, xf);
    });
}

/// A wheel (or a ring) facing along `xf`'s +y: a grey tyre round a dark hub.
fn wheel(o: &mut On<'_>, r: f32, width: f32, xf: Affine3A) {
    o.lathe(
        &[
            (r * 0.45, -width * 0.5),
            (r * 0.9, -width * 0.5),
            (r, -width * 0.38),
            (r, width * 0.38),
            (r * 0.9, width * 0.5),
            (r * 0.45, width * 0.5),
        ],
        16,
        TRIM,
        xf,
    );
    o.cylinder(r * 0.46, width * 0.9, 10, FRAME, xf);
}

/// A thruster bell that's not one of the main engines (no socket): its mouth along `dir`.
fn bell(o: &mut On<'_>, pos: Vec3, dir: Vec3, r: f32, xf: Affine3A) {
    let b = xf * place(pos, toward(dir));
    o.lathe(
        &[
            (r * 0.62, -r * 0.35),
            (r * 0.7, -r * 0.2),
            (r * 0.9, r * 0.45),
            (r, r * 0.95),
            (r * 0.86, r * 1.0),
            (r * 0.55, r * 0.15),
            (0.0, r * 0.1),
        ],
        16,
        GUN,
        b,
    );
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
    stencils(d);
}

// --- Head. ---

/// The head's axis, upright through the neck (a little ahead of the chest's middle).
const HEAD_Z: f32 = 0.1;
/// Heights on the head: the jaw under the chest's collar, the band of ribs and the visor, and the
/// cap's rim.
const JAW: f32 = 6.0;
const RIBS: (f32, f32) = (6.4, 7.03);
const CAP: f32 = 7.0;
/// The ribs' outer radius, and the cap's rim just proud of them.
const RIB_R: f32 = 0.97;
/// Half the visor's width, either side of the front (radians round the head).
const VISOR_HALF: f32 = 0.38;

fn head(d: &mut Designer) {
    let axis = at(0.0, 0.0, HEAD_Z);
    let mut h = d.on(Bone::Head);
    h.seed(0.07);
    // The neck, and the jaw: what shows of it stays under the chest's collar, round so the head
    // turns inside it.
    h.cylinder(0.45, 0.7, 12, BROWN, at(0.0, 5.75, HEAD_Z));
    h.lathe(
        &[(0.0, JAW), (0.6, JAW), (0.72, JAW + 0.1), (0.76, RIBS.0 + 0.05), (0.0, RIBS.0 + 0.05)],
        24,
        BODY,
        axis,
    );
    // The ribbed band round the sides and back: dark between red ribs, open at the front for the
    // visor.
    let (from, to) = (FRAC_PI_2 + VISOR_HALF, FRAC_PI_2 - VISOR_HALF + 2.0 * PI);
    h.lathe(&band(0.6, 0.9, RIBS.0, RIBS.1), 30, FRAME, axis);
    let n = 5;
    let pitch = (RIBS.1 - RIBS.0) / n as f32;
    for k in 0..n {
        let y = RIBS.0 + k as f32 * pitch + 0.025;
        h.lathe_arc(&band(0.88, RIB_R, y, y + pitch - 0.035), 30, from, to, RED, axis);
    }
    // The visor: a dark frame filling the gap in the ribs, flat across its face, the amber window
    // in it and the mono-eye behind the glass.
    let half_w = RIB_R * VISOR_HALF.sin() + 0.06;
    let face = HEAD_Z + RIB_R * VISOR_HALF.cos() + 0.06;
    h.block(
        v(half_w * 2.0, RIBS.1 - RIBS.0 + 0.04, 0.6),
        v2(1.0, 1.0),
        Vec2::ZERO,
        0.04,
        FRAME,
        at(0.0, (RIBS.0 + RIBS.1) * 0.5, face - 0.3),
    );
    let eye_y = 6.74;
    h.cube(v(half_w * 1.22, 0.56, 0.06), 0.02, VISOR, at(0.0, eye_y + 0.02, face));
    h.sphere(
        0.1,
        6,
        EYE,
        Affine3A::from_translation(v(0.0, eye_y, face + 0.01)) * Affine3A::from_scale(v(1.0, 1.0, 0.45)),
    );
    // A green chin under the window.
    h.block(
        v(half_w * 1.6, 0.2, 0.3),
        v2(1.0, 0.8),
        Vec2::ZERO,
        0.04,
        BODY,
        at(0.0, RIBS.0 + 0.1, face - 0.12),
    );
    // The cap: a dome whose rim stands just proud of the ribs, and its peak over the visor.
    h.lathe(
        &[
            (0.0, CAP),
            (RIB_R + 0.03, CAP),
            (RIB_R + 0.04, CAP + 0.07),
            (RIB_R, CAP + 0.24),
            (0.92, CAP + 0.44),
            (0.79, CAP + 0.61),
            (0.6, CAP + 0.73),
            (0.33, CAP + 0.8),
            (0.0, CAP + 0.82),
        ],
        30,
        BODY,
        axis,
    );
    // The peak: a wedge running forward and down out of the dome, its underside flat over the
    // window.
    let (back, tip) = (face - 0.6, face + 0.2);
    let peak = [
        (-1.0, -0.02, back),
        (1.0, -0.02, back),
        (-1.0, 0.55, back),
        (1.0, 0.55, back),
        (-0.85, -0.02, tip),
        (0.85, -0.02, tip),
        (-0.85, 0.13, tip),
        (0.85, 0.13, tip),
    ]
    .map(|(x, y, z)| v(x * half_w * 1.1, CAP + y, z));
    h.hexa(peak, 0.05, BODY, Affine3A::IDENTITY);
    // The cockpit's camera is the mono-eye.
    d.sockets.eye = Designer::local(Bone::Head, v(0.0, eye_y, face + 0.06));
    // What shows of the head starts at its ribs.
    d.head_base = RIBS.0;
}

// --- Chest and abdomen. ---

/// The chest's right half in two tiers, each a hexahedron with flat faces: the lower one swells to
/// a prow and tucks in toward the belt, the upper leans back to the collar, so the two halves meet
/// in a crease down the middle and a seam runs across under the lamps. Each tier is given by its
/// bottom and top sections: (y, outer x, back z, front z at the middle); the front falls away
/// outboard at `CHEST_FALL` per metre.
const CHEST_TIERS: [[(f32, f32, f32, f32); 2]; 2] =
    [[(3.5, 1.7, -1.35, 1.55), (4.3, 2.2, -1.6, 1.9)], [(4.3, 2.2, -1.6, 1.9), (5.75, 2.1, -1.5, 1.38)]];
const CHEST_FALL: f32 = 0.26;

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

/// Where a chest lamp's lens sits (right side), and the way it looks: forward, a little out.
const LAMP: Vec3 = Vec3::new(1.14, 5.92, 1.46);
const LAMP_TURN: f32 = 0.32;

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.14);
    for s in Side::BOTH {
        for tier in CHEST_TIERS {
            c.hexa(chest_tier(tier), 0.14, BODY, sided(s, Affine3A::IDENTITY));
        }
    }
    // The belt under the chest.
    c.block(v(3.1, 0.5, 2.75), v2(1.0, 1.0), Vec2::ZERO, 0.08, BODY, at(0.0, 3.32, 0.04));
    // The hatch's handle at the foot of the chest: a bar between two grey brackets.
    for s in Side::BOTH {
        c.cube(v(0.18, 0.42, 0.42), 0.04, TRIM, sided(s, place(v(0.5, 3.72, 1.66), rx(-0.3))));
    }
    c.cylinder(0.08, 1.0, 8, BROWN, place(v(0.0, 3.82, 1.82), rz(FRAC_PI_2)));
    for s in Side::BOTH {
        // The lamps flanking the head: a housing on the chest's shoulder, its square socket and
        // the big round lens in it, looking forward and a little out.
        let look = sided(s, place(LAMP, ry(LAMP_TURN)));
        c.block(v(0.98, 0.95, 1.0), v2(0.92, 0.85), v2(0.0, -0.05), 0.1, BODY, look * at(0.0, 0.0, -0.45));
        c.cube(v(0.76, 0.74, 0.1), 0.03, FRAME, look * at(0.0, 0.0, 0.06));
        let lens = look * Affine3A::from_rotation_x(FRAC_PI_2);
        c.cylinder(0.33, 0.12, 16, TRIM, lens * at(0.0, 0.1, 0.0));
        // The lens lit soft and warm (the lamps are dark when nobody's aboard).
        c.lathe(&[(0.0, 0.1), (0.28, 0.1), (0.26, 0.17), (0.0, 0.24)], 16, Paint::Light(Light::Lamp), lens);
        // The shoulder's socket in the chest's side.
        c.cylinder(0.72, 0.4, 14, BROWN, sided(s, place(v(1.95, 5.3, -0.05), rz(FRAC_PI_2))));
        // The back's side plates, either side of the backpack's mount, cut away at the top.
        let plate = sided(s, at(1.45, 4.85, -1.72));
        c.block(v(0.95, 1.7, 0.42), v2(0.7, 0.9), v2(-0.12, 0.0), 0.1, BODY, plate);
        slot(&mut c, 0.35, plate * at(0.0, 0.3, -0.22) * Affine3A::from_rotation_y(PI));
        slot(&mut c, 0.35, plate * at(0.2, -0.25, -0.22) * Affine3A::from_rotation_y(PI));
    }
    // Under the visor, a chin block over the hatch, with a dark vent under it.
    c.block(v(0.95, 0.75, 0.6), v2(0.9, 0.6), v2(0.0, -0.12), 0.08, BODY, at(0.0, 5.62, 1.42));
    c.cube(v(0.6, 0.12, 0.2), 0.02, FRAME, at(0.0, 5.2, 1.52));
    // Behind the head, the collar it turns in.
    c.block(v(1.9, 0.95, 0.55), v2(0.85, 0.7), v2(0.0, 0.05), 0.12, BODY, at(0.0, 5.95, -1.2));
    // The backpack's mount in the middle of the back: a square plate and its round port.
    c.block(v(1.8, 1.9, 0.4), v2(0.85, 0.95), Vec2::ZERO, 0.1, BODY, at(0.0, 4.7, -1.72));
    let port = place(v(0.0, 4.75, -1.95), rx(-FRAC_PI_2));
    c.lathe(&band(0.26, 0.5, -0.06, 0.1), 16, BODY, port);
    c.cylinder(0.27, 0.1, 12, FRAME, port);

    // The abdomen: a short, square-ish drum under the belt, flaring where it sits in the waist.
    d.on(Bone::Torso).seed(0.16).lathe(
        &[(0.0, 2.45), (1.45, 2.45), (1.55, 2.6), (1.5, 2.85), (1.3, 3.1), (1.25, 3.2), (0.0, 3.2)],
        8,
        BODY,
        Affine3A::from_rotation_y(PI / 8.0) * Affine3A::from_scale(v(1.0, 1.0, 0.82)),
    );
}

// --- Waist. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    // It rides high, over the hips: the abdomen is short.
    w.block(v(3.45, 0.7, 2.55), v2(0.97, 0.97), Vec2::ZERO, 0.12, BODY, at(0.0, 2.3, 0.0));
    // The crotch block, and the grey ring in its front.
    w.block(v(1.0, 1.35, 1.6), v2(1.25, 1.0), Vec2::ZERO, 0.1, BODY, at(0.0, 1.35, 0.3));
    let ring = place(v(0.0, 1.22, 1.12), rx(FRAC_PI_2));
    w.lathe(&band(0.27, 0.47, -0.05, 0.3), 18, TRIM, ring);
    w.cylinder(0.28, 0.3, 12, FRAME, ring * at(0.0, 0.05, 0.0));
    for s in Side::BOTH {
        // Front skirts: thick blocks angled out over the thighs, the bottom kicked forward.
        let front = sided(s, place(v(1.2, 1.58, 1.28), ry(0.3) * rx(-0.1) * rz(0.06)));
        w.block(v(1.4, 1.6, 0.66), v2(0.95, 0.85), v2(0.0, -0.04), 0.12, BODY, front);
        // A bolt on the right one, a square hatch on the left.
        w.greeble(|w| {
            let face = front * at(0.1, 0.15, 0.31);
            if s == Side::R {
                w.cylinder(0.17, 0.08, 10, BODY, face * Affine3A::from_rotation_x(FRAC_PI_2));
            } else {
                w.cube(v(0.32, 0.32, 0.05), 0.02, FRAME, face);
                w.cube(v(0.24, 0.24, 0.08), 0.02, BODY, face);
            }
        });
        // Side skirts: boxes standing proud of the hips, flaring at the hem.
        let side = sided(s, place(v(2.35, 1.72, -0.05), rz(0.14)));
        w.block(v(0.62, 1.6, 1.8), v2(0.9, 0.85), Vec2::ZERO, 0.12, BODY, side);
        slot(&mut w, 0.4, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.2, 0.32));
        // Rear skirts, each hooding a thruster that blows down and back.
        let rear = sided(s, place(v(0.86, 1.8, -1.55), rx(0.3)));
        w.block(v(1.45, 1.4, 0.85), v2(0.9, 0.8), v2(0.0, 0.05), 0.14, BODY, rear);
        u_mark(&mut w, 0.42, 0.36, rear * Affine3A::from_rotation_y(PI) * at(0.0, 0.0, 0.43));
        bell(&mut w, v(0.86, 1.05, -1.85), v(0.0, -0.4, -1.0), 0.54, sided(s, Affine3A::IDENTITY));
    }
}

// --- Arms. ---

/// The shoulder ball's centre (right side): above the arm's joint, as the big round shoulders sit
/// high beside the head.
const SHOULDER: Vec3 = Vec3::new(3.1, 5.3, 0.0);
const SHOULDER_R: f32 = 1.18;

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The brown ball, and the green armour over its top and outer side: a thick block tipped
        // outward, rounded at its edges, the ball showing under it and toward the chest.
        let mut o = d.on(shoulder);
        o.seed(0.25 + k);
        o.sphere(SHOULDER_R, 10, BROWN, sided(s, Affine3A::from_translation(SHOULDER)));
        let pad = sided(s, place(SHOULDER + v(0.55, 0.45, 0.0), rz(-0.22)));
        o.block(v(1.75, 1.75, 2.5), v2(0.9, 0.9), v2(-0.05, 0.0), 0.34, BODY, pad);
        o.greeble(|o| {
            o.cube(v(0.5, 0.42, 0.06), 0.02, FRAME, pad * at(0.15, 0.2, 1.25));
            o.cube(v(0.42, 0.34, 0.08), 0.02, BODY, pad * at(0.15, 0.2, 1.25));
        });
        slot(&mut o, 0.45, pad * at(0.0, 0.0, -1.25) * Affine3A::from_rotation_y(PI));
        // The running light on the pad's outer face, high and forward: red on the suit's own left
        // (+x), green on its right.
        let nav = pad * at(0.78, 0.5, 0.82);
        o.cube(v(0.16, 0.34, 0.42), 0.04, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.15, 6, lamp, nav * at(0.08, 0.0, 0.0) * Affine3A::from_scale(v(0.6, 1.0, 1.0)));
        o.cylinder(0.45, 0.8, 10, BROWN, sided(s, place(v(2.1, SHOULDER.y, 0.0), rz(FRAC_PI_2))));

        // The upper arm: a drum under the ball, its collar wider, on the frame.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.42, 1.6, 10, BROWN, up_xf * at(0.0, 0.9, 0.0));
        u.lathe(
            &[
                (0.0, 0.62),
                (0.58, 0.62),
                (0.64, 0.72),
                (0.64, 1.25),
                (0.72, 1.3),
                (0.72, 1.85),
                (0.62, 1.95),
                (0.0, 1.95),
            ],
            16,
            BODY,
            up_xf,
        );
        u.greeble(|u| {
            u.cube(v(0.26, 0.26, 0.08), 0.02, FRAME, up_xf * at(0.0, 1.58, 0.72));
        });

        // The forearm: the elbow's barrel, a rounded box swelling below the elbow and narrowing
        // to the wrist's cuff, and the elbow guard standing up over the joint outboard.
        let dir = el - wr;
        let fo_xf = sided(s, place(wr, toward(dir)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.5, 1.05, 12, BROWN, sided(s, place(el, rz(FRAC_PI_2))));
        f.block(v(1.42, 1.6, 1.48), v2(1.22, 1.18), Vec2::ZERO, 0.32, BODY, fo_xf * at(0.0, 0.96, 0.05));
        f.lathe(&band(0.48, 0.68, 0.0, 0.26), 14, BODY, fo_xf);
        let guard = fo_xf * at(0.86, 1.85, -0.2);
        f.block(v(0.36, 1.3, 1.25), v2(1.0, 0.55), v2(0.0, -0.25), 0.08, BODY, guard);
        slot(&mut f, 0.45, guard * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.1, -0.1, 0.19));
        f.cylinder(0.48, 0.3, 12, BROWN, fo_xf);
    }
    hands(d);
}

/// Brown fists.
fn hands(d: &mut Designer) {
    let w = j(Bone::HandR);
    for s in Side::BOTH {
        let mut h = d.on(s.pick(Bone::HandL, Bone::HandR));
        h.seed(0.4 + s.pick(0.0, 0.3));
        h.cube(v(0.82, 1.0, 0.98), 0.1, BROWN, sided(s, at(w.x, w.y - 0.45, w.z + 0.1)));
        h.greeble(|h| {
            for f in 0..4 {
                let z = w.z + 0.47 - f as f32 * 0.29;
                h.cube(
                    v(0.32, 0.28, 0.26),
                    0.05,
                    BROWN,
                    sided(s, place(v(w.x - 0.3, w.y - 1.02, z), rz(0.5))),
                );
            }
            h.cube(
                v(0.28, 0.52, 0.28),
                0.05,
                BROWN,
                sided(s, place(v(w.x - 0.22, w.y - 0.55, w.z + 0.64), rx(0.6))),
            );
        });
    }
}

// --- Legs. ---

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let k = s.pick(0.0, 0.2);

        // The thigh: an egg, fullest high up and swelling outboard, from the knee block up under
        // the skirts, on a brown ball at the hip.
        let mut t = d.on(thigh);
        t.seed(0.31 + k);
        t.sphere(0.75, 6, BROWN, sided(s, Affine3A::from_translation(hip)));
        let egg = sided(
            s,
            place(knee + v(0.08, 0.0, 0.0), toward(hip - knee)) * Affine3A::from_scale(v(0.96, 1.0, 1.1)),
        );
        t.lathe(
            &[
                (0.0, 0.45),
                (0.92, 0.5),
                (1.16, 0.85),
                (1.3, 1.5),
                (1.38, 2.25),
                (1.37, 2.85),
                (1.24, 3.35),
                (0.92, 3.78),
                (0.0, 3.95),
            ],
            20,
            BODY,
            egg,
        );
        for (p, turn) in
            [(v(0.45, 2.3, 1.28), rz(-0.5) * ry(0.3)), (v(-0.4, 1.35, 1.2), rz(-0.5) * ry(-0.15))]
        {
            slot(&mut t, 0.5, sided(s, place(knee + p, turn)));
        }

        // The shin: the knee block with its four rivets, then the long shin, narrow under the
        // knee, swelling to the calf behind two-fifths of the way down and pinched in again over
        // the ankle, with a ridge down its front; and the guard flaring out round the ankle, with
        // its wheels.
        let dir = knee - ankle;
        let tube = sided(s, place(ankle, toward(dir)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.6, 1.3, 12, BROWN, sided(s, place(knee, rz(FRAC_PI_2))));
        // Its radius up from the ankle, along the shin.
        let profile = [
            (0.25, 0.62),
            (0.7, 0.64),
            (1.3, 0.76),
            (2.1, 0.87),
            (2.7, 0.88),
            (3.6, 0.8),
            (4.4, 0.7),
            (5.0, 0.66),
            (5.25, 0.64),
        ];
        let outline: Vec<(f32, f32)> = [(0.0, 0.25)]
            .into_iter()
            .chain(profile.iter().map(|&(y, r)| (r, y)))
            .chain([(0.0, 5.3)])
            .collect();
        m.lathe(&outline, 20, BODY, tube);
        m.sphere(1.0, 10, BODY, tube * at(0.0, 3.0, -0.36) * Affine3A::from_scale(v(0.72, 1.7, 0.84)));
        // The ridge, in two runs following the front's curve.
        for pair in [[profile[6], profile[3]], [profile[3], profile[1]]] {
            let [(y0, r0), (y1, r1)] = pair;
            let run = v(0.0, y1 - y0, r1 - r0);
            let mid = v(0.0, (y0 + y1) * 0.5, (r0 + r1) * 0.5 - 0.06);
            let xf = tube * place(mid, toward(run)) * Affine3A::from_rotation_y(FRAC_PI_4);
            m.cube(v(0.22, run.length() + 0.1, 0.22), 0.03, BODY, xf);
        }
        u_mark(&mut m, 0.36, 0.34, tube * at(0.0, 4.55, 0.7) * Affine3A::from_rotation_x(-0.1));
        // The knee block wraps the front and the sides, its front sloping back at the top (it
        // stays behind the hangar's brace); behind, the joint shows.
        let knee_block = knee + v(0.02, 0.05, 0.14);
        m.block(
            v(2.3, 1.25, 1.6),
            v2(0.96, 0.84),
            v2(0.0, -0.1),
            0.14,
            BODY,
            sided(s, Affine3A::from_translation(knee_block)),
        );
        m.greeble(|m| {
            for (dx, dy) in [(-0.6, 0.22), (0.6, 0.22), (-0.6, -0.24), (0.6, -0.24)] {
                let p = knee_block + v(dx, dy - 0.08, 0.78);
                m.cylinder(0.19, 0.14, 12, BODY, sided(s, place(p, rx(FRAC_PI_2))));
            }
        });
        // The ankle guard: a tall collar round the foot of the shin in two tiers, flared below a
        // ridge and narrower above it, its front rising in a tab up the shin; a slot behind and a
        // wheel either side, low and toward the heel.
        let x = ankle.x;
        let cuff = sided(s, at(x, ankle.y, ankle.z - 0.1) * Affine3A::from_scale(v(1.0, 1.0, 1.1)));
        m.lathe(
            &[
                (0.0, -0.55),
                (0.92, -0.55),
                (1.02, -0.43),
                (1.05, -0.1),
                (0.97, 0.0),
                (0.95, 0.3),
                (0.92, 0.36),
                (0.88, 0.85),
                (0.82, 1.08),
                (0.62, 1.16),
                (0.0, 1.16),
            ],
            18,
            BODY,
            cuff,
        );
        m.hexa(
            shoe(x, (-7.5, -6.22), (0.5, 0.34), (0.45, 1.14), (0.4, 0.92)),
            0.1,
            BODY,
            sided(s, Affine3A::IDENTITY),
        );
        slot(&mut m, 0.45, sided(s, place(v(x, -7.05, ankle.z - 1.12), ry(PI) * rz(FRAC_PI_2))));
        for side in [-1.0, 1.0] {
            let hub = sided(s, place(v(x + side * 1.04, -7.92, ankle.z - 0.37), rz(FRAC_PI_2)));
            wheel(&mut m, 0.44, 0.3, hub);
        }

        // The foot, in three brown blocks seamed together, their sides leaning in: the heel under
        // the guard, its back slanting in, the instep, and the long toe falling away and narrowing
        // to its tip; the dark sole under them, and the green instep armour lying down the foot
        // out of the guard.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, BROWN, sided(s, Affine3A::from_translation(ankle)));
        let (sole, flat) = (-9.07, sided(s, Affine3A::IDENTITY));
        let base = sole + 0.12;
        f.hexa(boot(x, base, (-8.1, -8.1), (0.9, 0.96), [-1.5, -1.3, -0.36, -0.36]), 0.12, BROWN, flat);
        f.hexa(boot(x, base, (-8.04, -8.2), (0.98, 0.96), [-0.33, -0.33, 1.2, 1.2]), 0.12, BROWN, flat);
        f.hexa(boot(x, base, (-8.22, -8.52), (0.96, 0.78), [1.23, 1.23, 2.82, 2.42]), 0.12, BROWN, flat);
        f.cube(v(1.7, 0.14, 4.2), 0.03, FRAME, sided(s, at(x, sole + 0.07, 0.64)));
        f.greeble(|f| {
            // A seam across the toe, and the panel lines down the instep's and heel's sides.
            f.cube(v(1.3, 0.03, 0.06), 0.0, FRAME, sided(s, place(v(x, -8.36, 2.0), rx(0.19))));
            for (z, lean) in [(0.45, 0.5), (-0.9, -0.4)] {
                for side in [-1.0, 1.0] {
                    let tilt = rz(side * BOOT_LEAN.atan()) * ry(side * FRAC_PI_2) * rz(lean);
                    let face = sided(s, place(v(x + side * 0.88, -8.55, z), tilt));
                    f.cube(v(0.05, 0.55, 0.04), 0.0, FRAME, face);
                }
            }
        });
        f.hexa(shoe(x, (-8.24, -7.45), (0.6, 0.48), (0.3, 1.72), (0.3, 0.92)), 0.08, BODY, flat);
        u_mark(&mut f, 0.36, 0.32, sided(s, place(v(x, -8.02, 1.53), rx(-0.79))));
    }
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.22;

/// A block of a foot, centred on `x`, in the suit's frame: its sole level at `base`, narrowing
/// along z from `half.0` at the back to `half.1` at the front, its sides leaning in as they rise
/// ([`BOOT_LEAN`]); its top falling from `top.0` at the back to `top.1` at the front; `z` its back
/// at the sole and at the top, then its front at the sole and at the top (a face slants where they
/// differ). Each side lies in one plane (its half-width is linear in z and in height), so every
/// face stays flat.
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    let [back, back_top, front, front_top] = z;
    let h =
        |at: f32, y: f32| half.0 + (half.1 - half.0) * (at - back) / (front - back) - BOOT_LEAN * (y - base);
    std::array::from_fn(|i| {
        let (zz, y) = match (i & 4 != 0, i & 2 != 0) {
            (false, false) => (back, base),
            (false, true) => (back_top, top.0),
            (true, false) => (front, base),
            (true, true) => (front_top, top.1),
        };
        v(x + if i & 1 != 0 { h(zz, y) } else { -h(zz, y) }, y, zz)
    })
}

/// A block of a boot, centred on `x`, in the suit's frame: its bottom and top level (heights `y`),
/// its sides leaning in from `half.0` wide at the bottom to `half.1` at the top, running from back
/// to front along z over `bottom_z` at the bottom and `top_z` at the top (a slanted face where
/// they differ). Every face stays flat.
fn shoe(x: f32, y: (f32, f32), half: (f32, f32), bottom_z: (f32, f32), top_z: (f32, f32)) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        let (h, (z0, z1), height) = if i & 2 != 0 { (half.1, top_z, y.1) } else { (half.0, bottom_z, y.0) };
        v(x + if i & 1 != 0 { h } else { -h }, height, if i & 4 != 0 { z1 } else { z0 })
    })
}

// --- Stencils: thin plates just proud of the paint, up close only. ---

/// How far a stencil stands off its face, and how thick it is.
const STENCIL: (f32, f32) = (0.016, 0.03);

/// A seven-segment digit's segments, a to g: (centre x, centre y, width, height) in a digit `w`
/// wide and `h` tall, strokes `t` thick, with stencil gaps at the corners.
fn segments(w: f32, h: f32, t: f32) -> [(f32, f32, f32, f32); 7] {
    let gap = 0.02;
    let (across, up) = (w - 2.0 * t - 2.0 * gap, h * 0.5 - t - 2.0 * gap);
    let (side, row) = (w * 0.5 - t * 0.5, h * 0.25);
    [
        (0.0, h * 0.5 - t * 0.5, across, t),
        (side, row, t, up),
        (side, -row, t, up),
        (0.0, -h * 0.5 + t * 0.5, across, t),
        (-side, -row, t, up),
        (-side, row, t, up),
        (0.0, 0.0, across, t),
    ]
}

fn stencils(d: &mut Designer) {
    let mut o = d.on(Bone::ShoulderR);
    o.greeble(|o| {
        // The unit number on the right shoulder pad's outer face, reading front to back as one
        // stands beside it (never mirrored: the left pad wears the insignia instead).
        let pad = place(SHOULDER + v(0.55, 0.45, 0.0), rz(-0.22));
        // The face leans in toward its top (the pad's top is drawn narrower).
        let (up, out) = (v(-0.1375, 1.75, 0.0).normalize(), v(1.75, 0.1375, 0.0).normalize());
        let face = pad
            * Affine3A::from_mat3_translation(
                glam::Mat3::from_cols(v(0.0, 0.0, -1.0), up, out),
                v(0.806, -0.05, 0.0) + out * STENCIL.0,
            );
        let (w, h) = (0.55, 0.95);
        for place in 0..2u8 {
            let x = (f32::from(place) - 0.5) * (w + 0.1);
            for (segment, (cx, cy, sw, sh)) in segments(w, h, 0.095).into_iter().enumerate() {
                let paint = Paint::Digit { place, segment: segment as u8 };
                o.cube(v(sw, sh, STENCIL.1), 0.0, paint, face * at(x + cx, cy, 0.0));
            }
        }
    });
    let mut o = d.on(Bone::ShoulderL);
    o.greeble(|o| {
        // The insignia on the left pad: a hexagon round a chevron, the same for every faction.
        let pad = sided(Side::L, place(SHOULDER + v(0.55, 0.45, 0.0), rz(-0.22)));
        let (up, out) = (v(-0.1375, 1.75, 0.0).normalize(), v(1.75, 0.1375, 0.0).normalize());
        let face = pad
            * Affine3A::from_mat3_translation(
                glam::Mat3::from_cols(v(0.0, 0.0, -1.0), up, out),
                v(0.806, -0.05, 0.0) + out * STENCIL.0,
            );
        let white = Paint::Fixed(paint::WHITE);
        let r = 0.42;
        for k in 0..6 {
            let (a, b) = (k as f32, k as f32 + 1.0);
            let corner = |i: f32| {
                let t = std::f32::consts::FRAC_PI_3 * i + std::f32::consts::FRAC_PI_6;
                v(r * t.cos(), r * t.sin(), 0.0)
            };
            let (p, q) = (corner(a), corner(b));
            let bar = place((p + q) * 0.5, toward(q - p));
            o.cube(v(0.07, r + 0.035, STENCIL.1), 0.0, white, face * bar);
        }
        for side in [-1.0, 1.0] {
            let (p, q) = (v(side * 0.2, 0.1, 0.0), v(0.0, -0.13, 0.0));
            o.cube(
                v(0.09, (q - p).length() + 0.06, STENCIL.1),
                0.0,
                white,
                face * place((p + q) * 0.5, toward(q - p)),
            );
        }
        o.cube(v(0.32, 0.07, STENCIL.1), 0.0, white, face * at(0.0, 0.2, 0.0));
    });
    let mut c = d.on(Bone::Chest);
    c.greeble(|c| {
        // Hazard stripes across the belt under the hatch.
        let belt = at(0.0, 3.32, 0.04 + 1.375 + STENCIL.0);
        let (yellow, dark) = (Paint::Fixed(paint::YELLOW), Paint::Fixed(paint::DARK));
        let (w, h) = (0.17, 0.2);
        for k in -4..4 {
            let x = k as f32 * w;
            let stripe = [v2(x, -h * 0.5), v2(x + w, -h * 0.5), v2(x + w + h, h * 0.5), v2(x + h, h * 0.5)]
                .map(|p| v2(p.x - h * 0.5, p.y));
            c.extrude(&stripe, STENCIL.1, if k % 2 == 0 { yellow } else { dark }, belt);
        }
        // The rescue mark by the hatch, on the lower chest's left: a red triangle with its
        // exclamation, on the face as it leans.
        let (y0, _, _, f0) = CHEST_TIERS[0][0];
        let (y1, _, _, f1) = CHEST_TIERS[0][1];
        let rise = (f1 - f0) / (y1 - y0);
        let (x, y): (f32, f32) = (-0.95, 3.92);
        let z = f0 + rise * (y - y0) - CHEST_FALL * x.abs();
        let n = v(-CHEST_FALL, -rise, 1.0).normalize();
        let right = v(1.0, 0.0, CHEST_FALL).normalize();
        let mark = Affine3A::from_mat3_translation(
            glam::Mat3::from_cols(right, n.cross(right), n),
            v(x, y, z) + n * STENCIL.0,
        );
        let s = 0.42;
        let tri = [v2(0.0, s * 0.55), v2(-s * 0.5, -s * 0.32), v2(s * 0.5, -s * 0.32)];
        c.extrude(&tri, STENCIL.1, Paint::Fixed(paint::RED), mark);
        let white = Paint::Fixed(paint::WHITE);
        c.cube(v(0.05, 0.14, STENCIL.1 * 1.4), 0.0, white, mark * at(0.0, 0.03, 0.0));
        c.cube(v(0.05, 0.045, STENCIL.1 * 1.4), 0.0, white, mark * at(0.0, -0.1, 0.0));
    });
}

// --- The space type's backpack: a propellant drum across the back. ---

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The mount on the back's port, and the strut down to the drum.
    b.block(v(1.4, 1.5, 0.7), v2(0.85, 0.9), Vec2::ZERO, 0.12, BODY, at(0.0, 4.55, -2.25));
    b.block(v(1.0, 1.4, 0.6), v2(1.0, 1.0), Vec2::ZERO, 0.08, BODY, at(0.0, 3.65, -2.55));
    // The bracket under the drum that carries the main thrusters.
    b.block(v(1.8, 0.45, 0.85), v2(1.0, 1.0), Vec2::ZERO, 0.06, BODY, at(0.0, 2.5, -2.85));
    let drum = place(v(0.0, 3.25, -2.95), rz(FRAC_PI_2));
    b.lathe(
        &[
            (0.0, -3.2),
            (0.42, -3.2),
            (0.7, -3.1),
            (0.84, -2.9),
            (0.86, -2.65),
            (0.86, 2.65),
            (0.84, 2.9),
            (0.7, 3.1),
            (0.42, 3.2),
            (0.0, 3.2),
        ],
        20,
        BODY,
        drum,
    );
    b.lathe(&band(0.86, 0.98, -0.45, 0.45), 20, BODY, drum);
    b.cube(v(0.6, 0.5, 0.3), 0.04, TRIM, at(0.0, 3.25, -3.98));
    for end in [-3.25, 3.25] {
        b.cylinder(0.32, 0.2, 12, GUN, drum * at(0.0, end, 0.0));
        // A white strobe on top of each end of the drum.
        b.cylinder(
            0.17,
            0.12,
            10,
            GUN,
            drum * at(0.86, end * 0.9, 0.0) * Affine3A::from_rotation_z(FRAC_PI_2),
        );
        b.sphere(0.14, 6, Paint::Light(Light::Strobe), drum * at(0.94, end * 0.9, 0.0));
    }
    // The red beacon on top of the mount.
    b.cylinder(0.2, 0.12, 10, GUN, at(0.0, 5.34, -2.3));
    b.sphere(0.16, 6, Paint::Light(Light::Beacon), at(0.0, 5.42, -2.3));
    b.greeble(|b| {
        for y in [1.05, 1.7, 2.35] {
            for side in [-1.0, 1.0] {
                let y = side * y;
                b.lathe(&[(0.875, y - 0.03), (0.875, y + 0.03)], 20, FRAME, drum);
            }
        }
    });
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.6, 2.35, -2.8)), v(0.0, -0.6, -0.8), 0.45);
    }
}

// --- Weapons. ---

/// The long type beam rifle: a boxy receiver with a carry handle, the flat energy pack before the
/// grip, and a ribbed barrel shroud.
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

/// The 105 mm rifle clamped along the outside of the left forearm: the receiver, its perforated
/// jacket forward to the muzzle brake and the front sight, and the drum magazine hung under it.
/// Described on the right and mirrored; it rides low on the forearm, under the hangar's catwalk.
fn machine_gun(d: &mut Designer) {
    let l = |xf: Affine3A| sided(Side::L, xf);
    let x = 4.4;
    let along_z = |y: f32, z: f32| l(place(v(x, y, z), rx(FRAC_PI_2)));
    let mut f = d.on(Bone::ForearmL);
    f.seed(0.66);
    // The clamp onto the forearm, and the receiver.
    f.cube(v(0.45, 0.5, 0.8), 0.05, GUN, l(at(x - 0.32, 1.25, 1.0)));
    f.block(v(0.5, 0.62, 2.1), v2(0.95, 0.95), Vec2::ZERO, 0.06, GUN, l(at(x, 1.15, 0.95)));
    f.cube(v(0.18, 0.14, 1.1), 0.02, GUN, l(at(x, 1.52, 0.8)));
    // The jacket, its slots, and the barrel out of it.
    f.cylinder(0.24, 1.6, 12, GUN, along_z(1.12, 2.75));
    f.greeble(|f| {
        for k in 0..3 {
            for side in [-1.0, 1.0] {
                f.cube(v(0.05, 0.1, 0.32), 0.0, FRAME, l(at(x + side * 0.235, 1.12, 2.3 + k as f32 * 0.45)));
            }
        }
    });
    f.cylinder(0.12, 0.5, 8, STEEL, along_z(1.12, 3.75));
    f.cylinder(0.17, 0.3, 10, GUN, along_z(1.12, 4.1));
    // The drum magazine under the receiver.
    f.cylinder(0.48, 0.5, 18, GUN, l(at(x, 0.55, 1.1)));
    f.cylinder(0.2, 0.56, 10, FRAME, l(at(x, 0.55, 1.1)));
}
