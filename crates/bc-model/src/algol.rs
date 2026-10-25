//! PX-02 Algol, the colonies' infiltrator: a dark suit edged in points, under two great binders
//! spread like a bat's wings, the beam scythe in its right hand and the claw shield on its left
//! forearm. Its armour is faceted plates over lofted cores ([`crate::kit::Builder::loft`]):
//!
//! - a small head set low between the shoulders: a pale mask like a skull's, slit teeth over a red
//!   chin, green eyes slanting up under a heavy brow, cheek guards coming down past the chin to
//!   points; a crest over the helmet from the brow to a spike behind, the long yellow V-fin swept
//!   back with the forehead's camera at its root, and an ear either side with a round sensor in it
//!   and a head gun over it;
//! - a chest whose breastplates come down to points either side of the blue keel, each in two
//!   facets meeting in a ridge across it, red-slatted louvres in the upper; a blue yoke behind them
//!   rising to the ribbed grey collar; a narrow grey abdomen under two dark lames;
//! - a narrow belt with a red V for its buckle; front skirts falling to points outboard, a blue
//!   panel down each; side skirts slanting down to points at the front, and the rear skirt swept
//!   back over the seat;
//! - high, broad shoulders: a rounded block over a grey ball, its cap rising outboard to a faceted
//!   horn, a blue plate down its outside coming to a point with a round port in it, a raised panel
//!   with a vent on its front and a running light at the cap's front corner;
//! - slim upper arms with a blue band round them, gauntlets with blue cuffs and a spike standing
//!   back off the elbow, grey fists;
//! - long, heavy legs in plates: the thigh a squared core under a front plate stepped halfway down,
//!   a plate down its outside and another behind; the blue knee guard, its face rising to a point
//!   in front of the thigh; the lower leg full in the calf, a plate with a ridge down the shin, a
//!   plate over the calf's outside with a fin standing back off it, and two plates meeting in a
//!   ridge down its back; the boot's shaft round the ankle with its tongue and big grey hubs; and
//!   long feet: a high heel with a spur standing back off it, the toe (on its own hinge) narrowing
//!   to a point with a blue ridge along it, a vernier in the sole under the heel and under the toe.
//!
//! On its back, a narrow pack with a blue spine of louvres, the two main thrusters under it, and
//! the binders on hinges at its top corners (on `WingL` and `WingR`, which the client flares about
//! z at their roots on boost). Each binder's arm rises to a knuckle and on to a claw high over the
//! shoulder, and five plates hang from it in a fan, each tapering to a point, so that their feet
//! make the scalloped edge of a bat's wing: blue on their inner faces, the Eclipse's emitters in the
//! three nearest the body, dark behind.
//!
//! The beam scythe stands upright in front of the hanging arm, held as a scythe is, by its nib: a
//! short grey handle standing back from the shaft into the fist. The shaft runs from a spike by the
//! knee to its head high over the shoulder, with a grey grip for the other hand and blue collars;
//! the head is a housing with a spike up and a hook swept back over the pauldron, its red beak
//! reaching forward and down to the emitter the blade leaves. The claw shield is a long plate
//! pointed at the elbow, set down the forearm clear of the pauldron, its blue face in two facets
//! either side of a dark spine, three red talons round the emitter at its tip.
//!
//! The body is the livery's dark, its trim blue and its accents red; the inner frame, the joints and
//! the hands are grey, the mask a paler grey, the weapons gunmetal, the V-fin yellow.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::detail::{GREY, LENS, band, port, slot, stacked, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, Sole, THROAT, TRIM, YELLOW, along, at, hands, j, nozzle, place, rx,
    ry, rz, sided, sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The face's pale grey, the skull under the helmet.
const PALE: Paint = Paint::Fixed(paint::HULL);

pub(crate) fn algol(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    legs(d);
    backpack(d);
    binders(d);
    scythe(d);
    claw_shield(d);
}

// --- Shapes. ---

/// A plane facing forward: `z` forward at (`x`, `y`), falling back `fall.x` per metre out along x
/// and `fall.y` per metre up.
#[derive(Clone, Copy)]
struct Front {
    z: f32,
    at: Vec2,
    fall: Vec2,
}

impl Front {
    const fn new(z: f32, x: f32, y: f32, fall_x: f32, fall_y: f32) -> Self {
        Self { z, at: Vec2::new(x, y), fall: Vec2::new(fall_x, fall_y) }
    }

    fn z(self, x: f32, y: f32) -> f32 {
        self.z - self.fall.x * (x - self.at.x) - self.fall.y * (y - self.at.y)
    }

    /// A plate from its outline's corners in x and y (the low x's foot, the high x's foot, then
    /// their tops), its back flat at `back` and its front on this plane: every face flat.
    fn plate(self, xy: [(f32, f32); 4], back: f32) -> [Vec3; 8] {
        std::array::from_fn(|i| {
            let (x, y) = xy[i & 3];
            v(x, y, if i & 4 != 0 { self.z(x, y) } else { back })
        })
    }

    /// A frame on the plane at (`x`, `y`), `lift` out of it: z out of the face, y up it.
    fn frame(self, x: f32, y: f32, lift: f32) -> Affine3A {
        let n = v(self.fall.x, self.fall.y, 1.0).normalize();
        place(v(x, y, self.z(x, y)) + n * lift, Quat::from_rotation_arc(Vec3::Z, n))
    }
}

/// A slab `half` thick either side of x = 0, from its outline in (z, y): the back's foot and top,
/// then the front's.
fn slab(half: f32, zy: [(f32, f32); 4]) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        let (z, y) = zy[i >> 1];
        v(if i & 1 != 0 { half } else { -half }, y, z)
    })
}

/// An engraved line on the face at `xf`'s origin (looking along its +z), along its y: grey, as a
/// kit's panel lines are drawn on dark plastic.
fn line(o: &mut On<'_>, len: f32, xf: Affine3A) {
    o.greeble(|o| {
        o.cube(v(0.06, len, 0.05), 0.0, GREY, xf);
    });
}

/// A small round lens on the face at `xf`'s origin, looking along its +z: a grey rim round its
/// glass (lighter than [`port`], for the small ones).
fn lens(o: &mut On<'_>, r: f32, xf: Affine3A) {
    let face = xf * Affine3A::from_rotation_x(FRAC_PI_2);
    o.lathe(&band(r * 0.62, r, -0.04, 0.07), 12, GREY, face);
    o.lathe(&[(0.0, 0.0), (r * 0.64, 0.0), (r * 0.5, 0.07), (0.0, 0.09)], 10, LENS, face);
}

/// Placed in the plane of a side view: an outline's x runs forward (z), its y up, and its depth
/// across x.
fn profile(p: Vec3) -> Affine3A {
    place(p, ry(-FRAC_PI_2))
}

// --- Head. ---

/// Heights on the head: the chin's foot, the brow over the eyes, and the crown.
const CHIN: f32 = 6.2;
const BROW: f32 = 6.9;
const CROWN: f32 = 7.36;
/// The cockpit's camera, between the eyes at the face (the simulation's humanoid eye).
const CAMERA: Vec3 = Vec3::new(0.0, 6.75, 1.1);
/// The brow's front (right half): forward of the eyes at the middle, swept back outboard and up.
const BROW_FRONT: Front = Front::new(1.16, 0.0, 6.84, 0.3, 0.3);

fn head(d: &mut Designer) {
    let flat = Affine3A::IDENTITY;
    let mut h = d.on(Bone::Head);
    h.seed(0.05);
    // The neck, grey, turning inside the collar.
    h.cylinder(0.4, 0.8, 12, GREY, at(0.0, 5.98, 0.08));
    // The helmet: a tapered skull, and the forehead sloping back from the brow to the crown.
    h.hexa(stacked([(6.3, -0.6, 0.6, -0.5, 0.6), (CROWN - 0.1, -0.48, 0.48, -0.4, 0.45)]), 0.16, BODY, flat);
    h.hexa(stacked([(BROW, -0.5, 0.5, 0.2, 0.96), (CROWN, -0.36, 0.36, 0.1, 0.5)]), 0.1, BODY, flat);
    // The mask, pale, narrowing to the chin under the brow, and the eyes' dark sockets in it.
    h.hexa(
        stacked([(CHIN + 0.1, -0.2, 0.2, 0.3, 0.9), (BROW + 0.02, -0.5, 0.5, 0.3, 0.98)]),
        0.05,
        PALE,
        flat,
    );
    // The mouth: a raised grille between the cheeks, its slits the skull's teeth; the red chin
    // under it, coming to a point.
    h.hexa(stacked([(6.3, -0.18, 0.18, 0.6, 0.98), (6.6, -0.3, 0.3, 0.6, 1.0)]), 0.03, PALE, flat);
    h.greeble(|h| {
        for k in -2..=2 {
            let x = k as f32 * 0.1;
            h.cube(v(0.035, 0.22, 0.04), 0.0, FRAME, place(v(x, 6.46, 0.99), rz(x * 0.4)));
        }
    });
    h.hexa(stacked([(CHIN, -0.06, 0.06, 0.55, 0.88), (6.32, -0.18, 0.18, 0.55, 1.0)]), 0.03, ACCENT, flat);
    // The ridge between the eyes.
    h.hexa(stacked([(6.6, -0.05, 0.05, 0.8, 1.02), (BROW, -0.07, 0.07, 0.8, 1.05)]), 0.02, PALE, flat);
    for s in Side::BOTH {
        let side = sided(s, flat);
        // The eye: a dark socket and the green glass in it, slanting up outboard under the brow.
        let socket = Front::new(1.0, 0.0, 6.7, 0.12, 0.0);
        h.hexa(socket.plate([(0.05, 6.65), (0.5, 6.73), (0.05, 6.84), (0.53, 6.9)], 0.8), 0.02, FRAME, side);
        let eye = [v2(0.09, 6.69), v2(0.44, 6.76), v2(0.48, 6.845), v2(0.12, 6.785)];
        h.extrude(&eye, 0.05, EYE, sided(s, place(v(0.0, 0.0, 1.0), ry(-0.12))));
        // The brow over it, heavy, coming down to a point over the ridge.
        let brow = BROW_FRONT.plate([(0.0, 6.84), (0.6, 6.95), (0.0, 7.06), (0.6, 7.1)], 0.5);
        h.hexa(brow, 0.05, BODY, side);
        // The cheek guards, framing the mask and coming down past the chin to points.
        h.hexa(stacked([(6.08, 0.3, 0.44, 0.4, 0.86), (6.66, 0.46, 0.66, 0.1, 0.7)]), 0.05, BODY, side);
        // The ears: a block either side, a round sensor in its face and a head gun over it.
        h.hexa(stacked([(6.4, 0.54, 0.76, -0.38, 0.5), (7.04, 0.5, 0.7, -0.3, 0.42)]), 0.08, BODY, side);
        lens(&mut h, 0.17, sided(s, place(v(0.76, 6.68, 0.02), ry(FRAC_PI_2))));
        let gun = sided(s, at(0.6, 7.04, 0.62));
        h.block(v(0.32, 0.26, 0.6), v2(0.85, 0.85), Vec2::ZERO, 0.04, GUN, gun);
        h.cylinder(0.075, 0.4, 10, GUN, gun * place(v(0.0, 0.0, 0.4), rx(FRAC_PI_2)));
        h.cylinder(0.045, 0.06, 8, FRAME, gun * place(v(0.0, 0.0, 0.6), rx(FRAC_PI_2)));
        // The V-fin: a long yellow blade swept up and back from the crest's foot.
        let fin = [
            v2(0.0, -0.06),
            v2(0.3, -0.02),
            v2(1.0, 0.5),
            v2(1.78, 1.02),
            v2(1.05, 0.72),
            v2(0.3, 0.3),
            v2(0.0, 0.24),
        ];
        h.extrude(&fin, 0.09, YELLOW, sided(s, place(v(0.03, 7.12, 0.98), ry(0.32) * rx(-0.42))));
    }
    // The crest: a ridge over the helmet from the brow to the nape, ending in a point behind.
    let crest = [
        v2(1.0, 7.0),
        v2(0.78, 7.36),
        v2(0.4, 7.45),
        v2(-0.3, 7.4),
        v2(-0.7, 7.08),
        v2(-1.2, 6.48),
        v2(-0.5, 6.72),
        v2(-0.2, 7.1),
        v2(0.6, 7.0),
    ];
    h.extrude(&crest, 0.2, BODY, profile(Vec3::ZERO));
    // The V-fin's root, yellow, and the forehead's camera in it.
    h.hexa(stacked([(7.0, -0.14, 0.14, 0.8, 1.06), (7.3, -0.08, 0.08, 0.7, 0.92)]), 0.03, YELLOW, flat);
    h.cube(v(0.12, 0.09, 0.05), 0.01, EYE, place(v(0.0, 7.12, 1.03), rx(-0.4)));
    d.sockets.eye = Designer::local(Bone::Head, CAMERA);
    d.head_base = 6.25;
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half), the mass behind its plates: (y, outer x, back z, front z) at its
/// foot and at its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.3, 1.45, -1.15, 0.8), (5.95, 1.9, -1.35, 0.7)];
/// The breastplate (right) in two facets meeting in a ridge across it (`RIDGE` up), the lower
/// coming down to a point at the middle: each one's outline in x and y, and its front.
const RIDGE: f32 = 4.85;
const BREAST_LOW: [(f32, f32); 4] = [(0.18, 4.0), (2.0, 4.55), (0.18, RIDGE), (2.0, RIDGE)];
const BREAST_HIGH: [(f32, f32); 4] = [(0.18, RIDGE), (2.0, RIDGE), (0.18, 5.82), (2.0, 5.98)];
const LOW_FRONT: Front = Front::new(1.42, 0.18, RIDGE, 0.18, -0.25);
const BREAST_FRONT: Front = Front::new(1.42, 0.18, RIDGE, 0.18, 0.28);
/// The yoke over the breastplate (right): its outline, rising from behind the breastplate's top
/// toward the neck, and its front, steep, leaning back to the collar.
const YOKE: [(f32, f32); 4] = [(0.55, 5.72), (1.98, 5.88), (0.62, 6.22), (1.85, 6.34)];
const YOKE_FRONT: Front = Front::new(1.06, 0.55, 5.72, 0.15, 1.6);

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
        let [(y0, x0, b0, f0), (y1, x1, b1, f1)] = CORE;
        c.hexa(stacked([(y0, 0.0, x0, b0, f0), (y1, 0.0, x1, b1, f1)]), 0.2, BODY, flat);
        // The breastplate, and the louvre in its upper facet: a dark recess and red slats.
        c.hexa(LOW_FRONT.plate(BREAST_LOW, 0.3), 0.12, BODY, flat);
        c.hexa(BREAST_FRONT.plate(BREAST_HIGH, 0.3), 0.12, BODY, flat);
        let vent = sided(s, BREAST_FRONT.frame(1.12, 5.32, 0.0));
        c.cube(v(0.98, 0.7, 0.12), 0.02, FRAME, vent);
        for k in 0..4 {
            let y = -0.24 + k as f32 * 0.16;
            c.cube(
                v(0.88, 0.08, 0.14),
                0.01,
                ACCENT,
                vent * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.4),
            );
        }
        line(&mut c, 0.5, sided(s, BREAST_FRONT.frame(1.85, 5.35, 0.0)));
        line(&mut c, 0.6, sided(s, LOW_FRONT.frame(1.0, 4.5, 0.0) * Affine3A::from_rotation_z(1.3)));
        // The yoke, a step back from the breastplate's top, leaning back to the collar.
        c.hexa(YOKE_FRONT.plate(YOKE, -0.7), 0.12, TRIM, flat);
        line(&mut c, 0.4, sided(s, YOKE_FRONT.frame(1.3, 6.0, 0.0) * Affine3A::from_rotation_z(FRAC_PI_2)));
        // The flank under the shoulder, and the shoulder's socket in it.
        c.hexa(stacked([(3.55, 1.5, 2.25, -1.05, 0.65), (5.35, 1.7, 2.35, -1.2, 0.55)]), 0.14, BODY, flat);
        c.cylinder(0.6, 0.5, 14, GREY, sided(s, place(v(2.4, BALL.y, 0.0), rz(FRAC_PI_2))));
        // The back's plate either side of the pack.
        let back = sided(s, place(v(1.2, 4.6, -1.42), ry(-0.15)));
        c.block(v(0.9, 2.0, 0.34), v2(0.8, 0.85), Vec2::ZERO, 0.12, BODY, back);
        line(&mut c, 0.5, back * at(0.15, 0.1, -0.18) * Affine3A::from_rotation_y(PI));
    }
    // The keel down the middle, blue, standing out between the breastplates and coming to a point
    // under them.
    c.hexa(
        stacked([(3.7, -0.07, 0.07, 0.5, 1.56), (5.6, -0.36, 0.36, 0.5, 1.36)]),
        0.06,
        TRIM,
        Affine3A::IDENTITY,
    );
    c.greeble(|c| {
        c.cube(v(0.42, 0.06, 0.05), 0.0, FRAME, place(v(0.0, 5.1, 1.46), rx(-0.1)));
    });
    // The ribbed collar behind the neck.
    let (from, to) = (FRAC_PI_2 + 0.75, FRAC_PI_2 - 0.75 + 2.0 * PI);
    for k in 0..3 {
        let y = 5.88 + k as f32 * 0.15;
        c.lathe_arc(&band(0.55, 0.9 - k as f32 * 0.05, y, y + 0.1), 18, from, to, GREY, Affine3A::IDENTITY);
    }

    // The abdomen: the grey frame, narrow, and two lames over its front.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.82));
    a.lathe(&[(0.0, 2.4), (1.15, 2.4), (1.2, 2.6), (1.12, 3.45), (0.0, 3.45)], 14, GREY, squash);
    for (y0, y1) in [(2.55, 2.98), (2.98, 3.42)] {
        a.hexa(
            stacked([(y0, -0.7, 0.7, 0.3, 0.98), (y1, -0.84, 0.84, 0.3, 1.06)]),
            0.08,
            BODY,
            Affine3A::IDENTITY,
        );
    }
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.21);
    // The belt, narrow, rounded at its corners.
    w.loft(
        &[
            hoop(1.95, 0.0, 0.0, 1.42, 1.12),
            hoop(2.05, 0.0, 0.0, 1.52, 1.2),
            hoop(2.5, 0.0, 0.0, 1.52, 1.2),
            hoop(2.58, 0.0, 0.0, 1.44, 1.12),
        ],
        3.2,
        28,
        BODY,
        Affine3A::IDENTITY,
    );
    // The buckle, a red V, and the crotch block under it with a blue plate down its front.
    w.hexa(
        stacked([(1.62, -0.06, 0.06, 0.9, 1.3), (2.5, -0.5, 0.5, 0.95, 1.36)]),
        0.05,
        ACCENT,
        Affine3A::IDENTITY,
    );
    w.hexa(
        stacked([(0.7, -0.4, 0.4, -0.4, 0.95), (1.95, -0.62, 0.62, -0.5, 1.12)]),
        0.14,
        BODY,
        Affine3A::IDENTITY,
    );
    w.hexa(
        stacked([(0.82, -0.12, 0.12, 0.6, 1.1), (1.58, -0.4, 0.4, 0.6, 1.24)]),
        0.04,
        TRIM,
        Affine3A::IDENTITY,
    );
    for s in Side::BOTH {
        // The front skirt: a long plate hung from the belt over the thigh, its hem falling to a
        // point outboard, a blue panel raised down its inner half.
        let skirt = sided(s, place(v(1.12, 1.38, 1.32), ry(0.34) * rx(-0.14)));
        let plate = |x0: f32, x1: f32, top: f32, hem: (f32, f32), back: f32, front: f32| {
            [
                v(x0, hem.0, back),
                v(x1, hem.1, back),
                v(x0, top, back),
                v(x1, top, back),
                v(x0, hem.0, front),
                v(x1, hem.1, front),
                v(x0, top, front),
                v(x1, top, front),
            ]
        };
        w.hexa(plate(-0.7, 0.72, 0.75, (-0.68, -1.2), -0.2, 0.2), 0.12, BODY, skirt);
        w.hexa(plate(-0.6, 0.05, 0.6, (-0.5, -0.72), 0.1, 0.3), 0.05, TRIM, skirt);
        line(&mut w, 0.8, skirt * place(v(0.38, -0.2, 0.2), rz(0.3)));
        // The side skirt over the hip, its foot slanting down to a point at the front.
        let side = sided(s, place(v(2.12, 1.4, -0.08), rz(0.14)));
        w.hexa(slab(0.2, [(-1.05, -0.3), (-1.05, 0.95), (1.0, -1.15), (0.95, 0.95)]), 0.12, BODY, side);
        w.hexa(
            slab(0.08, [(-0.75, -0.08), (-0.75, 0.7), (0.62, -0.65), (0.6, 0.7)]),
            0.03,
            TRIM,
            side * at(0.2, 0.0, 0.0),
        );
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.78, 1.55, -1.4), rx(0.3)));
        w.block(v(1.4, 1.6, 0.5), v2(0.9, 0.85), v2(0.0, 0.05), 0.16, BODY, rear);
        line(&mut w, 0.6, rear * at(0.2, 0.0, -0.26) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder's grey ball (right).
const BALL: Vec3 = Vec3::new(3.25, 4.62, 0.0);
/// The pauldron's rounded block, as hoops out along x from the chest's side, each as high as
/// twice its half width round its middle (`PAULDRON_Y` up) and as deep as twice its half depth.
const PAULDRON_Y: f32 = 5.15;
const PAULDRON: [Hoop; 4] = [
    hoop(2.2, 0.0, 0.0, 0.82, 1.08),
    hoop(2.6, -0.06, 0.0, 1.0, 1.3),
    hoop(4.25, -0.02, 0.0, 1.04, 1.34),
    hoop(4.62, 0.06, 0.0, 0.84, 1.12),
];

/// The upper arm's sleeve, up its axis from the elbow.
const SLEEVE: [Hoop; 4] = [
    hoop(0.42, 0.0, 0.0, 0.62, 0.64),
    hoop(0.62, 0.0, 0.0, 0.7, 0.72),
    hoop(1.35, 0.0, 0.0, 0.68, 0.7),
    hoop(1.55, 0.0, 0.0, 0.56, 0.58),
];

/// The gauntlet, up the forearm from the wrist: the blue cuff's flare, then the box up to the
/// elbow, rounding in over its barrel.
const CUFF: [Hoop; 3] =
    [hoop(0.0, 0.0, 0.05, 0.8, 0.84), hoop(0.36, 0.0, 0.05, 0.86, 0.9), hoop(0.42, 0.0, 0.05, 0.8, 0.84)];
const GAUNTLET: [Hoop; 4] = [
    hoop(0.4, 0.0, 0.04, 0.74, 0.78),
    hoop(1.15, 0.02, 0.03, 0.8, 0.82),
    hoop(1.62, 0.02, 0.0, 0.76, 0.78),
    hoop(1.92, 0.0, 0.0, 0.54, 0.56),
];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);
        let flat = sided(s, Affine3A::IDENTITY);

        // The pauldron: a rounded block over the grey ball, a cap over its top rising outboard to a
        // horn swept up and out, and a blue plate down its outside coming to a point under it.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.sphere(0.86, 12, GREY, sided(s, Affine3A::from_translation(BALL)));
        o.loft(&PAULDRON, 3.4, 28, BODY, sided(s, place(v(0.0, PAULDRON_Y, 0.0), rz(-FRAC_PI_2))));
        let cap = [
            (2.45, 5.95, -1.2),
            (4.7, 6.1, -1.45),
            (2.45, 6.25, -1.2),
            (4.7, 6.55, -1.45),
            (2.45, 5.95, 1.2),
            (4.7, 6.1, 1.45),
            (2.45, 6.25, 1.2),
            (4.7, 6.55, 1.45),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(cap, 0.16, BODY, flat);
        line(&mut o, 1.6, sided(s, place(v(3.6, 6.4, 0.5), rz(0.133) * rx(FRAC_PI_2) * rz(-FRAC_PI_2))));
        // The horn: a faceted spike out of the cap, tapering to its point.
        let (root, tip) = (v(4.05, 6.2, -0.3), v(4.75, 7.75, -0.5));
        o.block(
            v(0.95, root.distance(tip), 0.8),
            v2(0.12, 0.2),
            v2(0.0, 0.0),
            0.1,
            BODY,
            sided(s, along(root, tip)),
        );
        // A raised panel on the pauldron's front, a grey vent in it.
        let panel = sided(s, at(3.55, 5.0, 1.28));
        o.block(v(1.5, 1.35, 0.24), v2(0.85, 0.9), Vec2::ZERO, 0.08, BODY, panel);
        o.cube(v(0.8, 0.5, 0.1), 0.02, FRAME, panel * at(0.0, -0.1, 0.1));
        o.greeble(|o| {
            for k in 0..3 {
                o.cube(v(0.72, 0.07, 0.12), 0.0, GREY, panel * at(0.0, -0.26 + k as f32 * 0.16, 0.12));
            }
        });
        o.hexa(stacked([(3.6, 4.48, 4.76, -0.6, 0.2), (5.7, 4.52, 4.82, -1.15, 1.15)]), 0.1, TRIM, flat);
        port(&mut o, 0.3, sided(s, place(v(4.82, 5.0, 0.15), ry(FRAC_PI_2))));
        // A running light at the cap's outer front corner: red on the suit's own left (+x), green
        // on its right.
        let nav = sided(s, at(4.3, 6.42, 1.08));
        o.cube(v(0.34, 0.16, 0.22), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.07, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));

        // The upper arm: a slim dark sleeve with a blue band round it, on the grey frame.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.42, 1.7, 12, GREY, up_xf * at(0.0, 0.95, 0.0));
        u.loft(&SLEEVE, 3.2, 20, BODY, up_xf);
        u.loft(&[hoop(0.85, 0.0, 0.0, 0.73, 0.75), hoop(1.05, 0.0, 0.0, 0.73, 0.75)], 3.2, 20, TRIM, up_xf);

        // The forearm: the elbow's grey barrel, the gauntlet over it with a blue cuff, and a spike
        // standing back off the elbow.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.54, 1.1, 14, GREY, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&CUFF, 3.2, 24, TRIM, fo_xf);
        f.loft(&GAUNTLET, 3.2, 24, BODY, fo_xf);
        let spike = [v2(-0.3, 0.8), v2(-0.62, 1.2), v2(-1.2, 2.15), v2(-0.5, 1.8), v2(-0.1, 1.5)];
        f.extrude(&spike, 0.3, BODY, fo_xf * Affine3A::from_rotation_y(-FRAC_PI_2));
        f.cube(v(0.08, 0.7, 0.7), 0.02, GREY, fo_xf * at(0.8, 1.1, 0.0));
        line(&mut f, 1.0, fo_xf * place(v(0.0, 1.0, 0.8), Quat::IDENTITY));
        f.cylinder(0.44, 0.3, 12, GREY, fo_xf);
    }
    hands(d, GREY);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): broad and squared,
/// fullest a little over halfway up.
const THIGH: [Hoop; 6] = [
    hoop(0.35, 0.3, 0.05, 1.0, 1.0),
    hoop(0.9, 0.38, 0.1, 1.22, 1.14),
    hoop(1.9, 0.42, 0.12, 1.32, 1.2),
    hoop(2.8, 0.4, 0.08, 1.3, 1.16),
    hoop(3.35, 0.32, 0.0, 1.1, 1.0),
    hoop(3.75, 0.2, 0.0, 0.7, 0.7),
];

/// The lower leg's core, up its axis from the ankle to the knee: full in the calf, swelling back
/// and out to its widest a little under the knee.
const SHIN: [Hoop; 8] = [
    hoop(0.8, 0.02, 0.0, 0.88, 0.9),
    hoop(1.6, 0.05, -0.1, 0.98, 1.02),
    hoop(2.7, 0.1, -0.28, 1.16, 1.24),
    hoop(3.7, 0.12, -0.38, 1.3, 1.38),
    hoop(4.4, 0.12, -0.38, 1.3, 1.36),
    hoop(4.95, 0.1, -0.3, 1.12, 1.14),
    hoop(5.35, 0.08, -0.25, 0.8, 0.8),
    hoop(5.5, 0.06, -0.22, 0.5, 0.5),
];

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);

        // The thigh: a squared core on a grey ball at the hip, a plate down its front standing
        // proud of it with a step where its lower half meets the upper, a plate down its outside
        // and another behind.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.8, 8, GREY, sided(s, Affine3A::from_translation(hip)));
        let up = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 4.0, 28, BODY, up);
        th.hexa(stacked([(1.75, -0.6, 1.1, 0.7, 1.4), (3.05, -0.66, 1.16, 0.6, 1.3)]), 0.14, BODY, up);
        th.hexa(stacked([(0.6, -0.48, 0.95, 0.6, 1.28), (1.72, -0.58, 1.08, 0.7, 1.36)]), 0.12, BODY, up);
        th.hexa(
            slab(0.16, [(-0.9, 1.0), (-0.95, 3.1), (0.85, 0.55), (0.95, 3.1)]),
            0.12,
            BODY,
            up * at(1.72, 0.0, 0.1),
        );
        th.hexa(stacked([(1.15, -0.5, 1.05, -1.36, -0.8), (2.95, -0.58, 1.12, -1.3, -0.8)]), 0.12, BODY, up);
        line(&mut th, 0.9, up * place(v(0.25, 2.4, 1.36), Quat::IDENTITY));
        line(&mut th, 1.2, up * place(v(1.88, 2.0, 0.1), ry(FRAC_PI_2)));
        line(&mut th, 1.0, up * place(v(0.3, 2.0, -1.33), ry(PI)));

        // The shin: the knee's grey barrel and the full core of the calf; a plate down its front
        // with a ridge, a plate over the calf's outside and a fin standing back off it; and the
        // blue knee guard, its face rising to a point in front of the thigh.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.62, 1.4, 12, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN, 3.5, 28, BODY, tube);
        let shin_front = Front::new(1.12, 0.1, 1.5, 0.0, -0.04);
        m.hexa(
            shin_front.plate([(-0.75, 1.45), (0.95, 1.45), (-0.85, 4.45), (1.05, 4.45)], 0.4),
            0.14,
            BODY,
            tube,
        );
        let ridge = |y: f32, z: f32| {
            [v(-0.14, y, z - 0.1), v(0.34, y, z - 0.1), v(-0.06, y, z + 0.1), v(0.26, y, z + 0.1)]
        };
        let (lo, hi) = (ridge(1.7, 1.12), ridge(4.25, 1.24));
        m.hexa([lo[0], lo[1], hi[0], hi[1], lo[2], lo[3], hi[2], hi[3]], 0.04, BODY, tube);
        m.hexa(
            slab(0.16, [(-1.45, 2.4), (-1.6, 4.6), (0.4, 2.1), (0.5, 4.75)]),
            0.12,
            BODY,
            tube * at(1.32, 0.0, 0.0),
        );
        // The calf's back: two plates meeting in a ridge, standing proud of it at their feet.
        for (lo, hi, fall) in [(0.12, 1.12, -0.48), (-0.88, 0.12, 0.48)] {
            let back = Front::new(-1.9, 0.12, 2.9, fall, 0.0);
            m.hexa(back.plate([(lo, 2.9), (hi, 2.9), (lo, 4.6), (hi, 4.6)], -1.0), 0.1, BODY, tube);
        }
        line(&mut m, 1.0, tube * place(v(0.62, 3.75, -1.7), ry(PI - 0.45)));
        let fin = [v2(0.0, 4.5), v2(0.35, 4.25), v2(0.78, 2.7), v2(1.05, 1.95), v2(0.0, 2.7)];
        m.extrude(&fin, 0.2, BODY, tube * place(v(0.75, 0.0, -1.1), ry(0.8)));
        line(&mut m, 1.4, tube * place(v(0.55, 3.0, 1.18), Quat::IDENTITY));
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        m.hexa(slab(0.5, [(0.45, -1.1), (0.3, 0.5), (1.36, -1.15), (1.15, 0.5)]), 0.14, TRIM, guard);
        let face = [v2(-0.5, -1.05), v2(0.5, -1.05), v2(0.52, 0.35), v2(0.0, 1.15), v2(-0.52, 0.35)];
        m.extrude(&face, 0.2, TRIM, guard * place(v(0.0, 0.0, 1.36), rx(-0.12)));
        slot(&mut m, 0.7, guard * place(v(0.0, -0.25, 1.46), rx(-0.12)));

        // The boot's shaft round the ankle: a rounded box over the foot of the shin, its tongue
        // rising up the front with its seams, a grey band round its back and a big grey hub either
        // side.
        let x = ankle.x;
        let flat = sided(s, Affine3A::IDENTITY);
        let (y0, y1) = (ankle.y - 0.55, ankle.y + 1.1);
        m.loft(
            &[
                hoop(y0, x, -0.08, 1.14, 1.0),
                hoop(y0 + 0.9, x, -0.1, 1.12, 0.98),
                hoop(y1, x, -0.12, 1.0, 0.88),
                hoop(y1 + 0.08, x, -0.12, 0.88, 0.76),
            ],
            3.6,
            24,
            BODY,
            flat,
        );
        let tongue = [
            (-1.0, y0 + 0.15, 0.55),
            (1.0, y0 + 0.15, 0.55),
            (-0.8, y1 + 0.3, 0.62),
            (0.8, y1 + 0.3, 0.62),
            (-1.0, y0 + 0.15, 1.8),
            (1.0, y0 + 0.15, 1.8),
            (-0.8, y1 + 0.3, 1.2),
            (0.8, y1 + 0.3, 1.2),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.2, BODY, flat);
        let rise = v(0.0, y1 + 0.3 - (y0 + 0.15), 1.2 - 1.8).normalize();
        let face = sided(s, place(v(x, y1 - 0.1, 1.36), Quat::from_rotation_arc(Vec3::Y, rise)));
        m.greeble(|m| {
            m.cube(v(1.3, 0.06, 0.05), 0.0, GREY, face * at(0.0, -0.9, 0.0));
            m.cube(v(0.06, 0.6, 0.05), 0.0, GREY, face * at(-0.3, 0.1, 0.0));
        });
        let back = sided(s, at(x, 0.0, ankle.z - 0.1));
        m.lathe_arc(
            &band(1.0, 1.14, ankle.y - 0.3, ankle.y + 0.08),
            18,
            0.2,
            PI - 0.2,
            GREY,
            back * Affine3A::from_rotation_y(PI),
        );
        for (side, r) in [(1.0, 0.62), (-1.0, 0.48)] {
            let hub = sided(s, place(v(x + side * 1.16, ankle.y - 0.2, ankle.z - 0.12), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                16,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }

        // The foot: a thick platform, high at the heel with a spur standing back off it, and the
        // long toe (on its own hinge) falling and narrowing to its point; a dark sole under it all,
        // split at the hinge, a vernier in each half, and a grey band round the heel over it.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 6, GREY, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        f.hexa(boot(x, base, (-7.5, -7.72), (1.24, 1.2), [-2.05, -1.82, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.34, 1.34), [-2.17, -2.14, -0.6, -0.6]),
            0.05,
            GREY,
            flat,
        );
        let spur = [v2(-1.7, -8.5), v2(-1.7, -7.7), v2(-2.05, -7.8), v2(-2.85, -8.42), v2(-2.15, -8.48)];
        f.extrude(&spur, 0.5, BODY, sided(s, profile(v(x, 0.0, 0.0))));
        line(&mut f, 1.6, sided(s, place(v(x + 1.12, -8.3, -0.4), ry(FRAC_PI_2) * rz(FRAC_PI_2))));
        let heel =
            Sole { z: (-2.15, 1.26), half: (1.32, 1.25), top: 0.34, pads: GREY, vernier: 0.15, r: 0.42 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.8, -8.36), (1.2, 0.5), [1.32, 1.32, 3.35, 2.92]), 0.34, BODY, flat);
        t.hexa(boot(x, -8.1, (-7.62, -8.0), (0.5, 0.2), [1.36, 1.36, 2.9, 2.6]), 0.1, TRIM, flat);
        let toes =
            Sole { z: (1.34, 3.42), half: (1.25, 0.52), top: 0.34, pads: GREY, vernier: 2.15, r: 0.32 };
        sole(d, toe, s, x, toes);
    }
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

/// A block of a foot ([`frames::boot`](crate::frames::boot)), its sides leaning in [`BOOT_LEAN`].
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    crate::frames::boot(x, base, top, half, z, BOOT_LEAN)
}

// --- The back: the pack and its binders. ---

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The pack between the shoulder blades, narrowing to its foot, and the blue spine down its
    // back with its louvres.
    b.hexa(
        stacked([(3.2, -0.95, 0.95, -2.65, -1.35), (5.7, -1.15, 1.15, -2.8, -1.45)]),
        0.2,
        BODY,
        Affine3A::IDENTITY,
    );
    b.hexa(
        stacked([(3.5, -0.38, 0.38, -2.95, -2.5), (5.4, -0.42, 0.42, -3.0, -2.6)]),
        0.06,
        TRIM,
        Affine3A::IDENTITY,
    );
    b.greeble(|b| {
        for k in 0..5 {
            let y = 3.9 + k as f32 * 0.28;
            b.cube(v(0.6, 0.08, 0.1), 0.0, FRAME, at(0.0, y, -3.0));
        }
    });
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
        // The binder's hinge out of the pack's top corner: a housing round a grey pin.
        b.hexa(stacked([(4.25, 0.7, 1.6, -3.0, -2.2), (5.6, 0.75, 1.5, -2.95, -2.3)]), 0.12, BODY, flat);
        b.cylinder(0.44, 0.5, 14, GREY, sided(s, place(v(1.2, 4.9, -2.9), rx(FRAC_PI_2))));
        // A side thruster low on the pack.
        b.lathe(
            &[(0.18, 0.0), (0.24, 0.1), (0.32, 0.4), (0.28, 0.42), (0.16, 0.1)],
            12,
            GUN,
            sided(s, place(v(1.05, 3.5, -2.2), rz(-FRAC_PI_2))),
        );
    }
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.58, 3.2, -2.3)), v(0.0, -0.6, -0.8), 0.42);
    }
}

/// The binder's arm (right): up and out from the hinge (the rig's wing joint) to the knuckle
/// where it bends, then on to the claw's root.
const KNUCKLE: Vec3 = Vec3::new(2.3, 7.9, -3.35);
const CLAW: Vec3 = Vec3::new(3.7, 9.6, -3.75);
/// How far a binder's plates sweep back as they go out (radians about y).
const SWEEP: f32 = 0.16;
/// The binder's plates (right), inner to outer: how far along the arm each hangs (0 at the
/// hinge, 1 at the knuckle, 2 at the claw), how far it leans out from straight down, and how long
/// and wide it is.
const PLATES: [(f32, f32, f32, f32); 5] = [
    (0.25, 0.08, 4.8, 1.5),
    (0.7, 0.28, 5.6, 1.6),
    (1.1, 0.5, 5.8, 1.6),
    (1.55, 0.75, 5.4, 1.5),
    (1.95, 1.0, 4.2, 1.2),
];

/// A plate's outline, hanging from its top: wide under the arm, tapering to a point.
fn feather(len: f32, w: f32) -> [Vec2; 6] {
    [
        v2(-0.5 * w, 0.3),
        v2(0.5 * w, 0.3),
        v2(0.56 * w, -0.32 * len),
        v2(0.22 * w, -0.82 * len),
        v2(0.05 * w, -len),
        v2(-0.42 * w, -0.5 * len),
    ]
}

fn binders(d: &mut Designer) {
    let hinge = j(Bone::WingR);
    let arm = |t: f32| if t <= 1.0 { hinge.lerp(KNUCKLE, t) } else { KNUCKLE.lerp(CLAW, t - 1.0) };
    let back = v(0.0, 0.0, -0.2);
    for s in Side::BOTH {
        let mut w = d.on(s.pick(Bone::WingL, Bone::WingR));
        w.seed(0.56 + s.pick(0.0, 0.2));
        for (k, &(t, lean, len, wide)) in PLATES.iter().enumerate() {
            let top = arm(t) + v(0.0, 0.0, 0.0 - 0.06 * k as f32);
            let xf = sided(s, place(top, ry(SWEEP) * rz(lean)));
            w.seed(0.6 + k as f32 * 0.04);
            let outline = feather(len, wide);
            w.extrude(&outline, 0.2, BODY, xf);
            let centre = v2(0.0, -0.45 * len);
            let inset: Vec<Vec2> = outline.iter().map(|p| centre + (*p - centre) * 0.76).collect();
            w.extrude(&inset, 0.06, TRIM, xf * at(0.0, 0.0, 0.11));
            line(&mut w, len * 0.5, xf * place(v(0.0, -0.45 * len, -0.11), ry(PI)));
            if k < 3 {
                w.greeble(|w| lens(w, 0.2, xf * at(0.0, -1.0, 0.14)));
            }
        }
        // The arm over the plates' tops, behind them, its knuckles grey.
        let (a, b, c) = (hinge + back, KNUCKLE + back, CLAW + back);
        w.block(v(0.66, a.distance(b), 0.5), v2(0.8, 0.85), Vec2::ZERO, 0.12, BODY, sided(s, along(a, b)));
        w.block(v(0.56, b.distance(c), 0.44), v2(0.75, 0.8), Vec2::ZERO, 0.1, BODY, sided(s, along(b, c)));
        for (p, r) in [(a, 0.36), (b, 0.3)] {
            w.lathe(&band(r * 0.4, r, -0.36, 0.36), 14, GREY, sided(s, place(p, rx(FRAC_PI_2))));
            w.cylinder(r * 0.45, 0.6, 10, TRIM, sided(s, place(p, rx(FRAC_PI_2))));
        }
        // The claw at its tip, swept up and out, and a thumb's spike off the knuckle.
        let claw = [v2(-0.32, -0.3), v2(0.32, -0.4), v2(0.45, 0.5), v2(0.18, 1.6), v2(-0.15, 0.6)];
        w.extrude(&claw, 0.4, BODY, sided(s, place(c, ry(SWEEP) * rz(-0.45))));
        let thumb = [v2(-0.2, 0.0), v2(0.22, 0.0), v2(0.05, 0.9)];
        w.extrude(&thumb, 0.3, BODY, sided(s, place(b, ry(SWEEP) * rz(0.5))));
    }
}

// --- The beam scythe. ---

/// The scythe's shaft at rest, from its butt to its head: upright in front of the hanging arm,
/// leaning a little forward, clear of the gauntlet's cuff, the pauldron and the leg.
const BUTT: Vec3 = Vec3::new(3.55, -3.55, 2.15);
const HEAD: Vec3 = Vec3::new(3.5, 6.95, 2.4);

fn scythe(d: &mut Designer) {
    // The nib's line through the fist, where the fingers close, a little under the wrist's grey
    // ring; and how far up the shaft the nib leaves it.
    let g = j(Bone::Weapon) + v(0.0, -0.25, 0.0);
    let len = BUTT.distance(HEAD);
    let shaft = place(BUTT, toward(HEAD - BUTT));
    let nib = (g.y - BUTT.y) / (HEAD.y - BUTT.y) * len;
    let mut w = d.on(Bone::Weapon);
    w.seed(0.62);
    // The shaft; the nib, a short grey handle standing back from it into the fist (the grip runs
    // along it, as a scythe's is held), clamped to the shaft; a grey grip higher up for the other
    // hand, and blue collars along it.
    w.cylinder(0.15, len, 12, GUN, shaft * at(0.0, len * 0.5, 0.0));
    let socket = shaft.transform_point3(v(0.0, nib, 0.0));
    let (heel, toe) = (v(g.x, g.y, g.z - 0.05), v(g.x, g.y, socket.z));
    w.cylinder(0.17, heel.distance(toe), 12, GREY, along(heel, toe));
    w.block(v(0.4, 0.62, 0.42), v2(1.0, 1.0), Vec2::ZERO, 0.05, GUN, shaft * at(0.0, nib, -0.12));
    w.greeble(|w| {
        for k in 1..4 {
            let z = g.z + 0.12 + k as f32 * 0.2;
            w.cylinder(0.2, 0.07, 10, GREY, place(v(g.x, g.y, z), rx(FRAC_PI_2)));
        }
    });
    let (y0, y1) = (nib + 2.0, nib + 3.1);
    w.cylinder(0.21, y1 - y0, 12, GREY, shaft * at(0.0, (y0 + y1) * 0.5, 0.0));
    w.greeble(|w| {
        for k in 1..4 {
            let y = y0 + (y1 - y0) * k as f32 / 4.0;
            w.cylinder(0.24, 0.09, 10, GREY, shaft * at(0.0, y, 0.0));
        }
    });
    for y in [0.4, nib - 0.9, nib + 0.5, len - 1.6] {
        w.lathe(&band(0.12, 0.26, y, y + 0.22), 12, TRIM, shaft);
    }
    // The butt, a spike.
    w.lathe(
        &[(0.0, -0.9), (0.08, -0.7), (0.22, -0.05), (0.26, 0.0), (0.26, 0.3), (0.0, 0.3)],
        10,
        GUN,
        shaft,
    );
    // The head: a collar round the shaft's top and a housing over it, a spike up out of it and a
    // hook swept back over the pauldron; its red beak reaching forward and down, the emitter in its
    // tip looking where the blade leaves it.
    let hd = shaft * at(0.0, len, 0.0);
    let side_on = hd * Affine3A::from_rotation_y(-FRAC_PI_2);
    w.lathe(&[(0.0, -0.7), (0.24, -0.7), (0.32, -0.6), (0.32, -0.1), (0.26, 0.0), (0.0, 0.0)], 12, GUN, hd);
    w.hexa(slab(0.3, [(-0.55, -0.25), (-0.4, 1.15), (0.55, -0.35), (0.3, 1.15)]), 0.08, GUN, hd);
    w.extrude(&[v2(-0.32, 1.1), v2(0.22, 1.1), v2(0.1, 1.6), v2(-0.05, 2.3)], 0.32, GUN, side_on);
    w.extrude(
        &[v2(-0.45, 0.95), v2(-0.5, 0.2), v2(-1.05, 0.0), v2(-1.55, -0.28), v2(-0.95, 0.45)],
        0.24,
        GUN,
        side_on,
    );
    line(&mut w, 0.7, hd * place(v(0.31, 0.45, 0.0), ry(FRAC_PI_2)));
    let blade = v(0.0, -0.35, 1.0).normalize();
    let root = hd.transform_point3(v(0.0, 0.7, 0.15));
    let emitter = root + blade * 1.6;
    let beak = place((root + emitter) * 0.5, toward(blade));
    w.block(v(0.56, 1.6, 0.7), v2(0.68, 0.7), v2(0.0, -0.08), 0.06, ACCENT, beak);
    w.block(v(0.36, 1.1, 0.12), v2(0.8, 1.0), Vec2::ZERO, 0.02, GUN, beak * at(0.0, -0.1, 0.37));
    let mouth = place(emitter, toward(blade));
    w.lathe(
        &[(0.1, -0.2), (0.2, -0.2), (0.24, -0.05), (0.24, 0.04), (0.14, 0.04), (0.1, -0.2)],
        14,
        GUN,
        mouth,
    );
    w.cylinder(0.12, 0.06, 10, THROAT, mouth * at(0.0, -0.06, 0.0));
    d.sockets.blade_right = Some((Designer::local(Bone::Weapon, emitter), blade));
    d.sockets.muzzle = Designer::local(Bone::Weapon, emitter);
    // The saber socket keeps the left arm's rest (the scythe is the right's).
    let wr = j(Bone::HandL);
    d.sockets.saber =
        (Designer::local(Bone::HandL, v(wr.x, wr.y - 0.5, wr.z + 0.4)), v(0.0, 0.62, 0.78).normalize());
}

// --- The claw shield. ---

fn claw_shield(d: &mut Designer) {
    let (el, wr) = (j(Bone::ForearmL), j(Bone::HandL));
    // Its frame: y down the forearm toward the claw, z out of its face (outboard), x across.
    let y = (wr - el).normalize();
    let z = (Vec3::NEG_X - y * y.dot(Vec3::NEG_X)).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(y.cross(z), y, z));
    let mid = (el + wr) * 0.5 + z * 0.95 + y * 0.55;
    let xf = place(mid, rot);
    let mut s = d.on(Bone::Shield);
    s.seed(0.71);
    // Its mount on the forearm's outer face.
    s.cube(v(0.7, 1.5, 0.5), 0.06, GREY, xf * at(0.0, -0.2, -0.38));
    // The body: a long dark plate pointed at the elbow, its blue face in two facets meeting in a
    // ridge down its middle, a dark spine along the ridge.
    let body = [
        v2(0.0, -2.45),
        v2(0.62, -2.0),
        v2(0.95, -1.2),
        v2(0.98, 0.9),
        v2(0.72, 1.95),
        v2(-0.72, 1.95),
        v2(-0.98, 0.9),
        v2(-0.95, -1.2),
        v2(-0.62, -2.0),
    ];
    s.extrude(&body, 0.3, BODY, xf);
    for (lo, hi, fall) in [(0.0, 0.8, 0.36), (-0.8, 0.0, -0.36)] {
        let face = Front::new(0.52, 0.0, 0.0, fall, 0.0);
        s.hexa(face.plate([(lo, -1.75), (hi, -1.75), (lo, 1.55), (hi, 1.55)], 0.1), 0.06, TRIM, xf);
        let tail = [(lo * 0.1, -2.15), (hi * 0.1, -2.15), (lo, -1.78), (hi, -1.78)];
        s.hexa(face.plate(tail, 0.1), 0.05, TRIM, xf);
    }
    s.hexa(stacked([(-2.05, -0.1, 0.1, 0.4, 0.62), (1.65, -0.16, 0.16, 0.4, 0.66)]), 0.04, BODY, xf);
    lens(&mut s, 0.2, xf * at(0.0, -1.0, 0.68));
    s.greeble(|s| {
        for k in 0..3 {
            let y = -0.2 + k as f32 * 0.4;
            for x in [-0.55, 0.55] {
                s.cube(
                    v(0.3, 0.05, 0.05),
                    0.0,
                    FRAME,
                    xf * place(v(x, y, 0.52 - 0.36 * 0.55), ry(0.35 * x.signum())),
                );
            }
        }
    });
    // The claw: three red talons out of its tip round the beam's emitter, the outer two curving
    // in to meet the middle one's point.
    s.block(v(1.3, 0.4, 0.5), v2(0.85, 0.85), Vec2::ZERO, 0.06, GUN, xf * at(0.0, 1.95, 0.15));
    s.cylinder(0.2, 0.08, 12, THROAT, xf * at(0.0, 2.16, 0.15));
    let talon = |s: &mut On<'_>, a: Vec3, b: Vec3, w: f32, tip: f32| {
        s.block(v(w, a.distance(b), w * 0.9), v2(tip, tip), Vec2::ZERO, 0.04, ACCENT, xf * along(a, b));
    };
    talon(&mut s, v(0.0, 2.0, 0.2), v(0.0, 3.5, 0.15), 0.3, 0.1);
    for x in [-1.0, 1.0] {
        let (a, b, c) = (v(x * 0.48, 1.95, 0.15), v(x * 0.78, 2.85, 0.12), v(x * 0.32, 3.55, 0.1));
        talon(&mut s, a, b, 0.34, 0.85);
        talon(&mut s, b, c, 0.3, 0.12);
    }
}
