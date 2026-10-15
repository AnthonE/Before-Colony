//! The Almagest's prototype pair, drawn after Gundam Wing's Vayeate and Mercurius (Sunrise's colour
//! and line art, and Bandai's HG 1/144 kit, which builds both on one frame): the AL-13X1
//! Sagittarius, the spear, and the AL-13X2 Corona, the shield. They share everything but the head's
//! hood and its antenna, what rides the back, the weapons and the colours:
//!
//! - a small hooded head sunk between very high, broad shoulders, its square camera in a grey bezel
//!   under the hood's peak, a pale band round the hood's brow; two round sensors on the chest's top
//!   either side of it, over a grey tube handle; a ribbed collar behind;
//! - a blocky chest with a raised inverted-trapezoid panel down its middle, grey louvres in the
//!   side blocks, a round hatch in the back, and a pale band round the waist under it;
//! - shoulders that are pale spheres, each under an angular bracket over its top and outer side
//!   with a round grey port in it;
//! - short upper arms, big boxy gauntlets with stepped cuffs, grey fists;
//! - a round boss in the crotch, long front skirts over the thighs with notched hems, side skirts
//!   ending in hooked spurs behind;
//! - rectangular knee pads, wide flared calves, and tall, heavy boots: a boxy shaft over the ankle
//!   with a thick plate rising up its front, a grey band behind and a big grey disc either side,
//!   on a thick platform with a high heel and a long toe, a narrower block raised along it.
//!
//! The Sagittarius wears a single swept horn and carries the beam cannon cabled to the generator on
//! its right shoulder blade, a ring as tall as its torso with an iris over its core; the Corona wears
//! a wide crescent on a short stalk, ten ward discs (the Mercurius's Planet Defensors) racked on two
//! pylons behind its shoulders (on [`Bone::Props`], so the client can send them out:
//! [`crate::ward_disc`]), the crash shield and a beam gun.
//!
//! The pale band and spheres are mint on the Sagittarius and lilac on the Corona; the inner frame,
//! the hands and the ankles are grey, the weapons, the generator, the wards and the shield a
//! dark slate.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::frames::{
    BODY, EYE, FRAME, GUN, STEEL, TRIM, YELLOW, at, j, nozzle, place, rx, ry, rz, sided, sp, v, v2,
};
use crate::kit::{Light, Paint};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The inner frame, the hands, the ankles' discs and the heels' bands.
const GREY: Paint = Paint::Fixed(paint::HULL_DARK);
/// The cameras' and sensors' glass.
const LENS: Paint = Paint::Fixed(paint::LENS);

/// Local +y along `dir`.
fn toward(dir: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, dir.normalize())
}

/// A closed rectangle in a lathe's (radius, height) plane, for rings and bands.
fn band(r0: f32, r1: f32, y0: f32, y1: f32) -> [(f32, f32); 5] {
    [(r0, y0), (r1, y0), (r1, y1), (r0, y1), (r0, y0)]
}

/// An engraved slot on the face at `xf`'s origin (looking along its +z), along its y.
fn slot(o: &mut On<'_>, len: f32, xf: Affine3A) {
    o.greeble(|o| {
        o.cube(v(0.07, len, 0.05), 0.0, FRAME, xf);
    });
}

/// A round port on the face at `xf`'s origin, looking along its +z: a grey rim round a dark
/// socket and its lens.
fn port(o: &mut On<'_>, r: f32, xf: Affine3A) {
    let face = xf * Affine3A::from_rotation_x(FRAC_PI_2);
    o.lathe(&band(r * 0.62, r, -0.06, 0.08), 18, TRIM, face);
    o.cylinder(r * 0.64, 0.1, 16, FRAME, face);
    o.lathe(&[(0.0, 0.05), (r * 0.42, 0.05), (r * 0.3, 0.1), (0.0, 0.12)], 14, LENS, face);
}

/// Which twin is being drawn, for what they don't share.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Twin {
    Sagittarius,
    Corona,
}

impl Twin {
    /// The pale paint of its spheres and bands.
    fn pale(self) -> Paint {
        match self {
            Twin::Sagittarius => Paint::Fixed(paint::MINT),
            Twin::Corona => Paint::Fixed(paint::LILAC),
        }
    }
}

pub(crate) fn sagittarius(d: &mut Designer) {
    let t = Twin::Sagittarius;
    head(d, t);
    chest(d, t);
    waist(d, t);
    arms(d, t);
    legs(d);
    generator(d);
    cannon(d);
    // It carries no blade, but the client turns the left arm from the saber's rest direction.
    let w = j(Bone::HandL);
    d.sockets.saber =
        (Designer::local(Bone::HandL, v(w.x - 0.1, w.y - 0.55, w.z + 0.2)), v(0.0, 0.62, 0.78).normalize());
}

pub(crate) fn corona(d: &mut Designer) {
    let t = Twin::Corona;
    head(d, t);
    chest(d, t);
    waist(d, t);
    arms(d, t);
    legs(d);
    racks(d);
    beam_gun(d);
    crash_shield(d);
}

// --- Head. ---

/// Heights on the head: the foot of the face block under the chest's collar, the hood's brow band
/// and its crown.
const FACE_FOOT: f32 = 6.05;
const BROW: (f32, f32) = (6.9, 7.05);
const CROWN: f32 = 7.72;
/// The hood's radius at its brow, and how far forward its axis stands.
const HOOD_R: f32 = 0.86;
const HOOD_Z: f32 = 0.0;
/// The square camera's centre, under the hood's peak.
const CAMERA: Vec3 = Vec3::new(0.0, 6.62, 0.97);

fn head(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let mut h = d.on(Bone::Head);
    h.seed(0.06 + if t == Twin::Corona { 0.5 } else { 0.0 });
    // The neck, grey, turning inside the collar.
    h.cylinder(0.45, 0.7, 12, GREY, at(0.0, 5.75, 0.05));
    // The face block: coloured, narrowing toward its chin, its cheeks swept back.
    h.block(v(1.2, 0.95, 1.35), v2(1.0, 1.0), v2(0.0, 0.05), 0.1, BODY, at(0.0, FACE_FOOT + 0.47, 0.25));
    h.block(v(0.7, 0.3, 0.55), v2(1.25, 1.0), Vec2::ZERO, 0.06, BODY, at(0.0, FACE_FOOT + 0.05, 0.62));
    for s in Side::BOTH {
        let cheek = sided(s, place(v(0.62, 6.5, 0.32), ry(0.2)));
        h.block(v(0.22, 0.8, 1.15), v2(1.0, 0.8), v2(0.0, -0.08), 0.06, BODY, cheek);
        slot(&mut h, 0.4, cheek * at(0.12, -0.05, 0.0) * Affine3A::from_rotation_y(FRAC_PI_2));
    }
    // The square camera in its grey bezel: its glass lit in the livery's eye colour.
    h.cube(v(0.68, 0.62, 0.16), 0.04, GREY, at(CAMERA.x, CAMERA.y, CAMERA.z - 0.06));
    h.cube(v(0.5, 0.44, 0.1), 0.02, EYE, at(CAMERA.x, CAMERA.y, CAMERA.z + 0.01));
    // The hood: a dome whose brow is the pale band, its peak coming down over the camera.
    let axis = at(0.0, 0.0, HOOD_Z) * Affine3A::from_scale(v(1.0, 1.0, 1.12));
    h.lathe(&band(HOOD_R - 0.05, HOOD_R + 0.04, BROW.0, BROW.1), 28, pale, axis);
    h.lathe(
        &[
            (0.0, BROW.0 - 0.02),
            (HOOD_R, BROW.0 - 0.02),
            (HOOD_R, BROW.0),
            (HOOD_R + 0.01, BROW.1),
            (HOOD_R, BROW.1 + 0.12),
            (0.72, BROW.1 + 0.3),
            (0.6, BROW.1 + 0.45),
            (0.4, CROWN - 0.04),
            (0.0, CROWN),
        ],
        28,
        BODY,
        axis,
    );
    // The peak: a wedge out of the dome's front, its edge in a shallow V over the camera.
    let (back, tip) = (0.45, 1.06);
    let peak = [
        (-0.62, 6.86, back),
        (0.62, 6.86, back),
        (-0.6, 7.32, back - 0.1),
        (0.6, 7.32, back - 0.1),
        (-0.42, 6.96, tip),
        (0.42, 6.96, tip),
        (-0.36, 7.1, tip - 0.02),
        (0.36, 7.1, tip - 0.02),
    ]
    .map(|(x, y, z)| v(x, y, z));
    h.hexa(peak, 0.05, BODY, Affine3A::IDENTITY);
    h.cube(v(0.86, 0.06, 0.08), 0.0, pale, place(v(0.0, 6.92, tip - 0.05), rx(0.0)));
    match t {
        Twin::Sagittarius => {
            // The horn: one tall blade out of the crown, swept back.
            let horn = [
                v2(0.0, 0.0),
                v2(0.34, 0.02),
                v2(0.12, 1.05),
                v2(-0.42, 1.95),
                v2(-0.36, 1.62),
                v2(-0.22, 0.4),
            ];
            let xf = place(v(0.0, CROWN - 0.22, 0.38), ry(-FRAC_PI_2) * rz(-0.1));
            h.extrude(&horn, 0.16, BODY, xf);
            h.block(v(0.26, 0.3, 0.6), v2(0.6, 0.8), Vec2::ZERO, 0.04, BODY, at(0.0, CROWN - 0.12, 0.2));
        }
        Twin::Corona => {
            // The crescent: a wide gull wing on a short stalk, its tips turned up.
            h.block(v(0.22, 0.42, 0.28), v2(0.8, 0.8), Vec2::ZERO, 0.04, BODY, at(0.0, CROWN - 0.02, 0.42));
            for s in Side::BOTH {
                let wing = [
                    v2(0.0, -0.08),
                    v2(0.55, 0.0),
                    v2(1.0, 0.18),
                    v2(1.28, 0.62),
                    v2(1.18, 0.66),
                    v2(0.86, 0.36),
                    v2(0.4, 0.2),
                    v2(0.0, 0.16),
                ];
                h.extrude(&wing, 0.14, BODY, sided(s, place(v(0.0, CROWN + 0.14, 0.52), rx(-0.2))));
            }
        }
    }
    // The cockpit's camera is the square one.
    d.sockets.eye = Designer::local(Bone::Head, CAMERA + v(0.0, 0.0, 0.08));
    d.head_base = FACE_FOOT + 0.1;
}

// --- Chest, abdomen and back. ---

/// The chest's right half in two tiers, each a hexahedron with flat faces: the lower swells
/// forward to its widest under the louvres, the upper leans back to the collar. Each tier is given
/// by its bottom and top sections: (y, outer x, back z, front z at the middle); the front falls
/// away outboard at `CHEST_FALL` per metre.
const CHEST_TIERS: [[(f32, f32, f32, f32); 2]; 2] = [
    [(3.35, 1.6, -1.25, 1.15), (4.5, 2.2, -1.55, 1.58)],
    [(4.5, 2.2, -1.55, 1.58), (5.95, 2.0, -1.45, 1.12)],
];
const CHEST_FALL: f32 = 0.24;

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

/// The chest's front at height `y` and `x` out (on the right), and its outward normal there.
fn chest_front(x: f32, y: f32) -> (Vec3, Vec3) {
    let tier = CHEST_TIERS.iter().find(|t| y <= t[1].0).unwrap_or(&CHEST_TIERS[1]);
    let [(y0, _, _, f0), (y1, _, _, f1)] = *tier;
    let rise = (f1 - f0) / (y1 - y0);
    let z = f0 + rise * (y - y0) - CHEST_FALL * x;
    (v(x, y, z), v(CHEST_FALL, -rise, 1.0).normalize())
}

/// A frame on the chest's front at (`x`, `y`) (right side), z along its normal, y up the face.
fn on_chest(x: f32, y: f32, lift: f32) -> Affine3A {
    let (p, n) = chest_front(x, y);
    let right = v(1.0, 0.0, -CHEST_FALL).normalize();
    let right = (right - n * right.dot(n)).normalize();
    Affine3A::from_mat3_translation(glam::Mat3::from_cols(right, n.cross(right), n), p + n * lift)
}

/// Half the centre panel's width at height `y`: an inverted trapezoid.
fn panel_half(y: f32) -> f32 {
    0.42 + (y - 3.5) / 1.85 * 0.4
}

fn chest(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    for s in Side::BOTH {
        for tier in CHEST_TIERS {
            c.hexa(chest_tier(tier), 0.14, BODY, sided(s, Affine3A::IDENTITY));
        }
        // The raised centre panel's half on this side, following both tiers' faces.
        for (y0, y1) in [(3.5, 4.5), (4.5, 5.35)] {
            let lift = 0.2;
            let corner = |x: f32, y: f32, out: f32| {
                let (p, n) = chest_front(x, y);
                p + n * out
            };
            let (w0, w1) = (panel_half(y0), panel_half(y1));
            let panel = [
                corner(0.0, y0, -0.08),
                corner(w0, y0, -0.08),
                corner(0.0, y1, -0.08),
                corner(w1, y1, -0.08),
                corner(0.0, y0, lift),
                corner(w0, y0, lift),
                corner(0.0, y1, lift),
                corner(w1, y1, lift),
            ];
            c.hexa(panel, 0.06, BODY, sided(s, Affine3A::IDENTITY));
        }
        // The louvres in the side block's front: a dark recess and five grey slats.
        let vent = sided(s, on_chest(1.55, 4.2, 0.0));
        c.cube(v(0.86, 1.0, 0.12), 0.02, FRAME, vent);
        for k in 0..5 {
            let y = -0.38 + k as f32 * 0.19;
            c.cube(
                v(0.78, 0.09, 0.16),
                0.01,
                TRIM,
                vent * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.35),
            );
        }
        c.block(v(1.0, 0.18, 0.2), v2(1.0, 1.0), Vec2::ZERO, 0.03, BODY, vent * at(0.0, 0.58, 0.04));
        // The round sensors on the chest's top either side of the head, in their housings.
        let sensor = sided(s, place(v(0.8, 5.82, 1.02), ry(0.18)));
        c.block(v(0.62, 0.62, 0.5), v2(0.9, 0.85), Vec2::ZERO, 0.06, BODY, sensor);
        port(&mut c, 0.27, sensor * at(0.0, 0.0, 0.26));
        c.greeble(|c| {
            let lens = sensor * at(0.0, 0.0, 0.39);
            c.cube(v(0.3, 0.05, 0.03), 0.0, FRAME, lens);
            c.cube(v(0.05, 0.3, 0.03), 0.0, FRAME, lens);
        });
        // The shoulder's socket in the chest's side.
        c.cylinder(0.62, 0.45, 14, GREY, sided(s, place(v(2.05, 5.12, 0.0), rz(FRAC_PI_2))));
        // The back's side plates either side of the hatch.
        let plate = sided(s, place(v(1.35, 4.5, -1.55), ry(-0.15)));
        c.block(v(0.85, 1.9, 0.36), v2(0.8, 0.85), Vec2::ZERO, 0.1, BODY, plate);
        slot(&mut c, 0.45, plate * at(0.1, 0.2, -0.19) * Affine3A::from_rotation_y(PI));
    }
    // The grey tube handle across the chest's top, under the face, on its brackets.
    c.cylinder(0.09, 0.9, 10, GREY, place(v(0.0, 5.95, 1.32), rz(FRAC_PI_2)));
    for s in Side::BOTH {
        c.cube(v(0.12, 0.16, 0.36), 0.02, GREY, sided(s, at(0.4, 5.95, 1.18)));
    }
    // The ribbed collar behind the neck.
    let collar = at(0.0, 0.0, 0.0);
    let (from, to) = (FRAC_PI_2 + 0.7, FRAC_PI_2 - 0.7 + TAU);
    for k in 0..3 {
        let y = 5.84 + k as f32 * 0.16;
        c.lathe_arc(&band(0.62, 0.96 - k as f32 * 0.05, y, y + 0.11), 18, from, to, GREY, collar);
    }
    // The round hatch in the middle of the back.
    let hatch = place(v(0.0, 4.65, -1.56), rx(-FRAC_PI_2));
    c.lathe(&band(0.62, 0.92, -0.02, 0.2), 24, BODY, hatch);
    c.lathe(&[(0.0, 0.12), (0.64, 0.12), (0.58, 0.18), (0.0, 0.2)], 20, BODY, hatch);
    c.greeble(|c| {
        for k in 0..4 {
            let a = k as f32 * FRAC_PI_2 + PI / 4.0;
            c.cylinder(0.06, 0.06, 6, GREY, hatch * at(0.76 * a.cos(), 0.21, 0.76 * a.sin()));
        }
    });

    // The abdomen: the grey frame, and the pale band round the waist over it.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.84));
    a.lathe(&[(0.0, 2.45), (1.3, 2.45), (1.36, 2.6), (1.25, 3.4), (0.0, 3.4)], 12, GREY, squash);
    a.lathe(
        &[(1.2, 2.72), (1.46, 2.72), (1.5, 2.8), (1.5, 3.2), (1.46, 3.28), (1.2, 3.28), (1.2, 2.72)],
        20,
        pale,
        squash,
    );
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer, t: Twin) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2 + if t == Twin::Corona { 0.3 } else { 0.0 });
    // The belt, riding over the hips, and the crotch block under it.
    w.block(v(3.4, 0.55, 2.6), v2(0.98, 0.97), Vec2::ZERO, 0.1, BODY, at(0.0, 2.33, 0.0));
    w.block(v(1.15, 1.35, 1.6), v2(1.25, 1.0), Vec2::ZERO, 0.1, BODY, at(0.0, 1.42, 0.3));
    // The round boss in its front: a coloured rim round a grey ball.
    let boss = place(v(0.0, 1.3, 1.1), rx(FRAC_PI_2));
    w.lathe(&band(0.3, 0.5, -0.05, 0.22), 18, BODY, boss);
    w.lathe(&[(0.0, -0.05), (0.32, -0.05), (0.31, 0.12), (0.2, 0.22), (0.0, 0.25)], 14, GREY, boss);
    for s in Side::BOTH {
        // The long front skirts curving over the thighs: an upper plate and a lower one turned
        // further out, a notch in the hem.
        let upper = sided(s, place(v(1.18, 1.62, 1.38), ry(0.24) * rx(-0.1) * rz(0.06)));
        w.block(v(1.6, 1.4, 0.46), v2(0.96, 0.92), Vec2::ZERO, 0.12, BODY, upper);
        let lower = sided(s, place(v(1.36, 0.3, 1.66), ry(0.34) * rx(-0.3) * rz(0.1)));
        w.block(v(1.55, 1.45, 0.42), v2(1.02, 1.0), v2(0.0, 0.04), 0.12, BODY, lower);
        // The hem's notch, a dark step cut into the lower plate's outer corner.
        w.block(v(0.5, 0.42, 0.46), v2(1.0, 1.0), Vec2::ZERO, 0.02, FRAME, lower * at(0.55, -0.55, -0.01));
        w.greeble(|w| {
            w.cube(v(0.08, 0.7, 0.05), 0.0, FRAME, upper * at(-0.48, 0.0, 0.24));
            w.cube(v(1.0, 0.06, 0.05), 0.0, FRAME, lower * at(-0.1, 0.42, 0.22));
        });
        // The side skirts, standing out from the hips, and the hooked spur behind each.
        let side = sided(s, place(v(2.38, 1.62, -0.12), rz(0.16)));
        w.block(v(0.55, 1.8, 2.0), v2(0.85, 0.85), Vec2::ZERO, 0.1, BODY, side);
        slot(&mut w, 0.5, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.1, 0.29));
        // (Its outline runs back along x and up along y; it stands edge-on beside the skirt.)
        let spur = [v2(0.0, 1.1), v2(1.05, 1.0), v2(1.35, 0.1), v2(1.25, -1.0), v2(0.95, -0.2), v2(0.3, 0.3)];
        w.extrude(&spur, 0.3, BODY, sided(s, place(v(2.48, 1.0, -0.6), ry(FRAC_PI_2))));
        // The rear skirt's half, its own spur at the outer corner.
        let rear = sided(s, place(v(0.88, 1.75, -1.5), rx(0.25)));
        w.block(v(1.45, 1.35, 0.6), v2(0.92, 0.85), v2(0.0, 0.05), 0.12, BODY, rear);
        slot(&mut w, 0.35, rear * at(0.2, 0.0, -0.31) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder sphere's centre (right side), high beside the head.
const SPHERE: Vec3 = Vec3::new(3.3, 5.2, 0.0);
const SPHERE_R: f32 = 1.08;

fn arms(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The pale sphere, and the bracket over its top and outer side: a top plate rising
        // outboard to eye level and an outer plate down the side, the port in it.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.sphere(SPHERE_R, 14, pale, sided(s, Affine3A::from_translation(SPHERE)));
        let top = [
            (2.55, 5.92, -1.18),
            (4.3, 6.02, -1.18),
            (2.62, 6.72, -1.05),
            (4.72, 6.98, -1.05),
            (2.55, 5.92, 1.15),
            (4.3, 6.02, 1.15),
            (2.62, 6.72, 0.98),
            (4.72, 6.98, 0.98),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(top, 0.12, BODY, sided(s, Affine3A::IDENTITY));
        let outer = [
            (4.1, 4.3, -1.12),
            (4.8, 4.3, -1.12),
            (4.1, 6.3, -1.15),
            (4.8, 6.95, -1.1),
            (4.1, 4.3, 1.12),
            (4.8, 4.3, 1.12),
            (4.1, 6.3, 1.1),
            (4.8, 6.95, 1.02),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(outer, 0.12, BODY, sided(s, Affine3A::IDENTITY));
        // A lip along the bracket's front edge.
        o.block(
            v(0.4, 2.2, 0.2),
            v2(1.0, 1.0),
            Vec2::ZERO,
            0.04,
            BODY,
            sided(s, place(v(4.45, 5.5, 1.16), rz(0.0))),
        );
        port(&mut o, 0.4, sided(s, place(v(4.82, 5.4, 0.05), ry(FRAC_PI_2))));
        slot(&mut o, 0.4, sided(s, place(v(4.82, 4.7, -0.6), ry(FRAC_PI_2))));
        // A running light at the top plate's outer front corner: red on the suit's own left (+x),
        // green on its right.
        let nav = sided(s, at(4.25, 6.92, 0.96));
        o.cube(v(0.36, 0.16, 0.2), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));
        o.cylinder(0.42, 0.7, 10, GREY, sided(s, place(v(2.35, SPHERE.y, 0.0), rz(FRAC_PI_2))));

        // The short upper arm: a coloured drum on the grey frame, out from under the sphere.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.42, 1.6, 10, GREY, up_xf * at(0.0, 0.9, 0.0));
        u.lathe(
            &[(0.0, 0.55), (0.6, 0.55), (0.68, 0.66), (0.68, 1.15), (0.6, 1.25), (0.0, 1.25)],
            16,
            BODY,
            up_xf,
        );

        // The gauntlet: the elbow's barrel, then a big box stepping out at its cuffs, a dark
        // panel on its outer face.
        let dir = el - wr;
        let fo_xf = sided(s, place(wr, toward(dir)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.52, 1.1, 12, GREY, sided(s, place(el, rz(FRAC_PI_2))));
        f.block(v(1.55, 1.25, 1.62), v2(1.0, 1.0), Vec2::ZERO, 0.16, BODY, fo_xf * at(0.0, 0.92, 0.04));
        f.block(v(1.68, 0.38, 1.76), v2(0.96, 0.96), Vec2::ZERO, 0.08, BODY, fo_xf * at(0.0, 1.7, 0.02));
        f.block(v(1.66, 0.34, 1.72), v2(1.0, 1.0), Vec2::ZERO, 0.08, BODY, fo_xf * at(0.0, 0.2, 0.06));
        f.cube(v(0.06, 0.7, 0.85), 0.02, GREY, fo_xf * at(0.79, 1.05, -0.1));
        f.greeble(|f| {
            f.cube(v(0.05, 0.08, 1.2), 0.0, FRAME, fo_xf * at(0.8, 0.5, 0.05));
            f.cube(v(1.2, 0.08, 0.05), 0.0, FRAME, fo_xf * at(0.0, 0.5, 0.83));
        });
        f.cylinder(0.46, 0.3, 12, GREY, fo_xf);
    }
    crate::frames::hands(d, GREY);
}

// --- Legs. ---

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let k = s.pick(0.0, 0.2);

        // The thigh: grey at the hip, a full drum from under the skirts to the knee, armoured
        // down its front by two plates meeting in a ridge and down its outside by a third.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.8, 6, GREY, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee + v(0.05, 0.0, -0.05), toward(hip - knee)));
        th.lathe(
            &[(0.0, 0.45), (0.95, 0.5), (1.2, 0.9), (1.34, 1.8), (1.38, 2.7), (1.26, 3.4), (0.0, 3.75)],
            14,
            BODY,
            along_thigh * Affine3A::from_scale(v(1.0, 1.0, 1.08)),
        );
        for half in [-1.0f32, 1.0] {
            let (bottom, top) = ((0.85, 1.48, 0.98, 1.05), (3.05, 1.6, 1.08, 1.16));
            th.hexa(ridged(half, bottom, top, 0.42), 0.08, BODY, along_thigh);
        }
        let outside = [
            (1.18, 0.95, -0.95),
            (1.5, 0.95, -0.85),
            (1.22, 3.15, -1.05),
            (1.58, 3.15, -0.95),
            (1.18, 0.95, 0.95),
            (1.5, 0.95, 0.85),
            (1.22, 3.15, 1.05),
            (1.58, 3.15, 0.95),
        ]
        .map(|(x, y, z)| v(x, y, z));
        th.hexa(outside, 0.1, BODY, along_thigh);
        slot(&mut th, 0.55, along_thigh * place(v(0.5, 2.0, 1.36), rx(-0.05)));

        // The shin: a slim core inside heavy greaves. A big knee guard comes to a point below the
        // knee, and the greave's front, two plates meeting in a ridge, flares as it falls toward the
        // boot. Side plates flare out over the boot's shaft, and the calf's block bulges behind,
        // widest a third of the way up.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.62, 1.3, 12, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        let profile = [(0.2, 0.7), (1.2, 0.82), (2.4, 0.92), (3.4, 0.86), (4.4, 0.76), (5.3, 0.7)];
        let outline: Vec<(f32, f32)> = [(0.0, 0.2)]
            .into_iter()
            .chain(profile.iter().map(|&(y, r)| (r, y)))
            .chain([(0.0, 5.32)])
            .collect();
        m.lathe(&outline, 14, BODY, tube);
        // (Each in two runs: swelling from the boot to its widest low on the calf, then narrowing
        // to the knee.)
        let greave = [(1.15, 1.3, 1.0, 0.86), (2.3, 1.4, 1.16, 0.8), (4.55, 0.98, 0.7, 0.5)];
        let flare = [(1.1, 1.02, 1.18, 0.62), (2.25, 1.12, 1.36, 0.66), (3.9, 0.82, 0.98, 0.5)];
        for half in [-1.0f32, 1.0] {
            for run in 0..2 {
                m.hexa(ridged(half, greave[run], greave[run + 1], 0.42), 0.08, BODY, tube);
                // The side plate, standing out over the calf's widest.
                let ((y0, i0, o0, f0), (y1, i1, o1, f1)) = (flare[run], flare[run + 1]);
                let side = [
                    (i0, y0, -0.82),
                    (o0, y0, -0.72),
                    (i1, y1, -0.86),
                    (o1, y1, -0.76),
                    (i0, y0, f0 + 0.1),
                    (o0, y0, f0),
                    (i1, y1, f1 + 0.1),
                    (o1, y1, f1),
                ]
                .map(|(x, y, z)| v(x * half, y, z));
                m.hexa(if half > 0.0 { side } else { swap_x(side) }, 0.08, BODY, tube);
            }
        }
        for (y0, y1, h0, h1, b0, b1) in
            [(1.2, 2.3, 0.86, 1.14, -0.86, -1.45), (2.3, 4.8, 1.14, 0.68, -1.45, -0.68)]
        {
            let calf = [
                (-h0, y0, b0),
                (h0, y0, b0),
                (-h1, y1, b1),
                (h1, y1, b1),
                (-h0, y0, 0.15),
                (h0, y0, 0.15),
                (-h1, y1, 0.15),
                (h1, y1, 0.15),
            ]
            .map(|(x, y, z)| v(x, y, z));
            m.hexa(calf, 0.16, BODY, tube);
        }
        m.greeble(|m| {
            m.cube(v(0.06, 1.6, 0.05), 0.0, FRAME, tube * place(v(0.62, 2.6, 1.03), rx(0.12)));
            m.cube(v(0.6, 0.06, 0.05), 0.0, FRAME, tube * place(v(-0.45, 1.6, 1.13), rx(0.12)));
        });
        // The knee guard: two plates meeting in a ridge, broad over the knee and coming forward
        // to a point below it; a grey inset at its top.
        let pad = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        for half in [-1.0f32, 1.0] {
            m.hexa(ridged(half, (-1.1, 1.62, 0.98, 1.0), (0.9, 1.22, 0.86, 0.82), 0.5), 0.08, BODY, pad);
        }
        m.hexa(
            [
                (-0.3, -1.6, 1.0),
                (0.3, -1.6, 1.0),
                (-0.5, -1.05, 1.0),
                (0.5, -1.05, 1.0),
                (-0.1, -1.6, 1.5),
                (0.1, -1.6, 1.5),
                (-0.4, -1.05, 1.6),
                (0.4, -1.05, 1.6),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.05,
            BODY,
            pad,
        );
        m.block(
            v(0.62, 0.5, 0.12),
            v2(0.8, 1.0),
            Vec2::ZERO,
            0.03,
            TRIM,
            pad * place(v(0.0, 0.42, 1.22), rx(-0.2)),
        );
        slot(&mut m, 0.42, pad * place(v(0.0, -0.45, 1.5), rx(-0.2)));
        // The boot's shaft round the ankle: a box over the foot of the shin, a grey band round
        // its back at the joint, and the big grey disc either side where the shaft meets the foot.
        let x = ankle.x;
        let flat = sided(s, Affine3A::IDENTITY);
        let (y0, y1) = (ankle.y - 0.55, ankle.y + 1.05);
        let shaft = [
            (-1.08, y0, -1.0),
            (1.08, y0, -1.0),
            (-0.96, y1, -0.9),
            (0.96, y1, -0.9),
            (-1.08, y0, 0.85),
            (1.08, y0, 0.85),
            (-0.96, y1, 0.75),
            (0.96, y1, 0.75),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(shaft, 0.12, BODY, flat);
        // Its front: a thick plate rising from over the instep to high on the shin, like a boot's
        // tongue, wider at its foot; a notch near its top.
        let tongue = [
            (-1.02, y0 + 0.15, 0.6),
            (1.02, y0 + 0.15, 0.6),
            (-0.84, y1 + 0.2, 0.68),
            (0.84, y1 + 0.2, 0.68),
            (-1.02, y0 + 0.15, 1.72),
            (1.02, y0 + 0.15, 1.72),
            (-0.84, y1 + 0.2, 1.2),
            (0.84, y1 + 0.2, 1.2),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.14, BODY, flat);
        let slope = v(0.0, y1 + 0.2 - (y0 + 0.15), 1.2 - 1.72).normalize();
        let face = sided(s, place(v(x, y1 - 0.05, 1.29), Quat::from_rotation_arc(Vec3::Y, slope)));
        m.greeble(|m| {
            m.cube(v(0.42, 0.07, 0.05), 0.0, FRAME, face * at(-0.06, 0.0, 0.0));
            m.cube(v(0.07, 0.36, 0.05), 0.0, FRAME, face * at(-0.24, -0.15, 0.0));
            m.cube(v(1.2, 0.06, 0.05), 0.0, FRAME, face * at(0.0, -0.95, 0.04));
        });
        let back = sided(s, at(x, 0.0, ankle.z - 0.08));
        m.lathe_arc(
            &band(1.0, 1.13, ankle.y - 0.3, ankle.y + 0.08),
            18,
            0.2,
            PI - 0.2,
            GREY,
            back * Affine3A::from_rotation_y(PI),
        );
        for (side, r) in [(1.0, 0.6), (-1.0, 0.46)] {
            let hub = sided(s, place(v(x + side * 1.12, ankle.y - 0.2, ankle.z - 0.12), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                16,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }

        // The foot: a thick platform, the heel high at its back, the middle under the shaft's
        // tongue, and the long toe falling away to its tip with a narrower block raised along it;
        // a dark sole under it all.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, GREY, sided(s, Affine3A::from_translation(ankle)));
        let sole = -9.07;
        let base = sole + 0.32;
        // The heel and the middle, one tall block, highest at the back.
        f.hexa(boot(x, base, (-7.5, -7.72), (1.24, 1.2), [-2.05, -1.82, 1.4, 1.4]), 0.18, BODY, flat);
        // The toe box: as wide, narrowing and falling away to a blunt, bevelled tip.
        f.hexa(boot(x, base, (-7.8, -8.04), (1.2, 1.04), [1.35, 1.35, 3.35, 2.92]), 0.28, BODY, flat);
        // The narrower block raised along the toe, stepping down at its front.
        f.hexa(boot(x, -8.1, (-7.6, -7.9), (0.56, 0.48), [1.3, 1.3, 2.8, 2.52]), 0.08, BODY, flat);
        // The thick sole, a little proud of the upper all round, its heel deeper.
        f.hexa(
            boot(x, sole, (base + 0.02, base + 0.02), (1.32, 1.12), [-2.15, -2.12, 3.45, 3.42]),
            0.06,
            FRAME,
            flat,
        );
        f.hexa(
            boot(x, sole, (base + 0.06, base + 0.06), (1.3, 1.3), [-2.15, -2.12, -0.6, -0.6]),
            0.04,
            GREY,
            flat,
        );
        f.greeble(|f| {
            // The upper's seam along each side, the step between the heel and the toe box, and a
            // slot up the heel's back.
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 3.2), 0.0, FRAME, sided(s, at(x + side * 1.12, -8.3, -0.3)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.1, -8.25, 1.38)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.05, -1.98), rx(-0.25))));
        });
    }
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

/// A block of a foot, centred on `x`: its sole level at `base`, narrowing along z from `half.0` at
/// the back to `half.1` at the front, its sides leaning in as they rise; its top falling from
/// `top.0` at the back to `top.1` at the front; `z` its back at the sole and at the top, then its
/// front at the sole and at the top. Each face stays flat (as the Argo's boots: `argo::boot`).
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

/// Half (`half` -1 the inner, 1 the outer) of an armour plate whose two halves meet in a ridge
/// down its middle (x = 0), in a limb's frame (y along it, z forward): at its `bottom` and `top`
/// heights, given as (y, the ridge's z, the edge's x, the edge's z), the plate `depth` thick.
fn ridged(half: f32, bottom: (f32, f32, f32, f32), top: (f32, f32, f32, f32), depth: f32) -> [Vec3; 8] {
    let face = |(y, ridge, edge_x, edge_z): (f32, f32, f32, f32)| {
        let (a, b) = (v(0.0, y, ridge), v(edge_x * half, y, edge_z));
        if half > 0.0 { [a, b] } else { [b, a] }
    };
    let (lo, hi) = (face(bottom), face(top));
    std::array::from_fn(|i| {
        let p = if i & 2 == 0 { lo[i & 1] } else { hi[i & 1] };
        if i & 4 == 0 { p - v(0.0, 0.0, depth) } else { p }
    })
}

/// A hexahedron's corners mirrored across x, reordered so its low-x corners stay the low ones.
fn swap_x(c: [Vec3; 8]) -> [Vec3; 8] {
    std::array::from_fn(|i| c[i ^ 1])
}

// --- The Sagittarius's back: the generator, and its cannon. ---

/// The generator's ring: its centre on the right shoulder blade, facing back, and its radii.
const GEN: Vec3 = Vec3::new(1.2, 5.25, -3.1);
const GEN_R: f32 = 2.35;
const GEN_HOLE: f32 = 1.2;
const GEN_DEPTH: f32 = 1.5;

fn generator(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The mount off the back's hatch, and the struts out to the ring.
    b.block(v(1.6, 1.6, 0.8), v2(0.85, 0.9), Vec2::ZERO, 0.12, GUN, at(0.3, 4.65, -1.98));
    b.block(v(0.7, 2.6, 0.6), v2(1.0, 1.0), Vec2::ZERO, 0.08, GUN, place(v(0.75, 4.9, -2.3), rz(-0.25)));
    // The ring: a deep housing round the core, rounded at its rims.
    let ring = place(GEN, rx(-FRAC_PI_2));
    let h = GEN_DEPTH * 0.5;
    b.lathe(
        &[
            (GEN_HOLE, -h),
            (GEN_R - 0.25, -h),
            (GEN_R, -h + 0.3),
            (GEN_R, h - 0.3),
            (GEN_R - 0.25, h),
            (GEN_HOLE + 0.1, h),
            (GEN_HOLE, h - 0.1),
            (GEN_HOLE, -h),
        ],
        32,
        GUN,
        ring,
    );
    // The amber stripe round its back face, broken at the top where the handle stands.
    b.lathe_arc(&band(1.78, 1.95, h, h + 0.03), 32, FRAC_PI_2 + 0.5, FRAC_PI_2 - 0.5 + TAU, YELLOW, ring);
    // The iris over the core: four petals, a gap between each, round a dark star at the middle.
    for k in 0..4 {
        let a0 = k as f32 * FRAC_PI_2 + 0.05;
        b.lathe_arc(
            &[
                (0.42, h - 0.3),
                (GEN_HOLE + 0.02, h - 0.3),
                (GEN_HOLE + 0.02, h - 0.12),
                (0.7, h - 0.02),
                (0.42, h - 0.06),
                (0.42, h - 0.3),
            ],
            32,
            a0,
            a0 + FRAC_PI_2 - 0.1,
            GUN,
            ring,
        );
    }
    b.cylinder(0.5, 0.2, 12, FRAME, ring * at(0.0, h - 0.32, 0.0));
    b.greeble(|b| {
        // The core's three-way vane behind the iris, and the vent ribs on the ring's upper rim.
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0 + FRAC_PI_2;
            b.cube(
                v(0.12, 0.05, 0.42),
                0.0,
                GREY,
                ring * place(v(0.22 * a.cos(), h - 0.25, 0.22 * a.sin()), ry(-a)),
            );
        }
        for k in 0..9 {
            let a = FRAC_PI_2 + (k as f32 - 4.0) * 0.12;
            let p = v(GEN_R * a.cos(), 0.0, GEN_R * a.sin());
            b.cube(v(0.09, GEN_DEPTH * 0.55, 0.16), 0.0, FRAME, ring * place(p, ry(-a)));
        }
    });
    // The carry handle over the ring.
    let top = GEN + v(0.0, GEN_R, 0.0);
    for x in [-0.75, 0.75] {
        b.cube(v(0.2, 0.75, 0.24), 0.03, GUN, at(top.x + x, top.y + 0.3, top.z));
    }
    b.cube(v(1.7, 0.2, 0.24), 0.03, GUN, at(top.x, top.y + 0.72, top.z));
    // The box under the ring, where the cannon's cable comes out.
    let outlet = GEN_OUTLET;
    b.block(v(1.5, 1.1, 1.1), v2(0.9, 0.85), Vec2::ZERO, 0.1, GUN, at(outlet.x, outlet.y + 0.6, outlet.z));
    b.cylinder(0.2, 0.3, 10, GREY, at(outlet.x, outlet.y + 0.1, outlet.z));
    // The cannon's mount on the left of the back, empty while it's in hand: a pivot hub.
    let hub = place(v(-1.25, 4.4, -2.0), rx(-FRAC_PI_2));
    b.block(v(0.8, 1.2, 0.5), v2(0.9, 0.9), Vec2::ZERO, 0.06, GUN, at(-1.1, 4.4, -1.85));
    b.lathe(&band(0.18, 0.4, 0.0, 0.3), 14, GUN, hub);
    // The main thrusters under the mount.
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.55, 3.3, -2.05)), v(0.0, -0.55, -0.85), 0.42);
    }
}

/// Where the cannon's cable leaves the box under the generator.
const GEN_OUTLET: Vec3 = Vec3::new(GEN.x + 0.6, GEN.y - GEN_R - 0.95, GEN.z + 0.35);

/// The beam cannon, held under the right arm: a long box-section barrel with its amber slot near
/// the octagonal muzzle, a slimmer round rear section with its rows of vents, and the rear block
/// the cable plugs into.
fn cannon(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let along_z = |y: f32, z: f32| place(v(g.x, g.y + y, g.z + z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    let y = 0.78;
    // The grip, and the barrel above it.
    w.cube(v(0.34, 1.0, 0.46), 0.05, GUN, place(v(g.x, g.y - 0.15, g.z), rx(-0.25)));
    w.block(v(0.96, 1.15, 7.6), v2(1.0, 1.0), Vec2::ZERO, 0.2, GUN, at(g.x, g.y + y, g.z + 2.7));
    // The amber slot down its top near the muzzle, and the side's vent grille.
    w.cube(v(0.3, 0.06, 2.8), 0.02, FRAME, at(g.x, g.y + y + 0.58, g.z + 4.6));
    w.cube(v(0.2, 0.06, 2.6), 0.0, YELLOW, at(g.x, g.y + y + 0.6, g.z + 4.6));
    w.greeble(|w| {
        for k in 0..6 {
            w.cube(v(0.06, 0.5, 0.12), 0.0, FRAME, at(g.x + 0.49, g.y + y, g.z + 1.2 + k as f32 * 0.25));
        }
        w.cube(v(1.0, 0.06, 0.06), 0.0, FRAME, at(g.x, g.y + y + 0.2, g.z + 6.2));
    });
    // The sight on top, its round lens.
    w.cube(v(0.26, 0.3, 0.7), 0.03, GUN, at(g.x, g.y + y + 0.72, g.z + 0.6));
    w.cylinder(0.12, 0.08, 10, LENS, place(v(g.x, g.y + y + 0.75, g.z + 0.97), rx(FRAC_PI_2)));
    // The octagonal muzzle and its bore.
    w.cylinder(0.64, 0.7, 8, GUN, along_z(y, 6.75) * Affine3A::from_rotation_y(PI / 8.0));
    w.cylinder(0.4, 0.2, 12, FRAME, along_z(y, 7.05));
    // The rear section: slimmer, round, vented; and the rear block.
    w.cylinder(0.44, 2.6, 14, GUN, along_z(y - 0.1, -2.1));
    w.greeble(|w| {
        for k in 0..4 {
            for a in [-0.6, 0.6] {
                let z = -1.3 - k as f32 * 0.5;
                let p = v(g.x + 0.44 * f32::sin(a), g.y + y - 0.1 + 0.44 * f32::cos(a), g.z + z);
                w.cylinder(
                    0.08,
                    0.06,
                    6,
                    FRAME,
                    place(p, Quat::from_rotation_arc(Vec3::Y, v(f32::sin(a), f32::cos(a), 0.0))),
                );
            }
        }
    });
    w.block(v(0.8, 0.95, 0.8), v2(0.9, 0.9), Vec2::ZERO, 0.08, GUN, at(g.x, g.y + y - 0.05, g.z - 3.6));
    w.cylinder(0.17, 0.25, 10, GREY, along_z(y - 0.05, -4.05));
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + y, g.z + 7.15));
    // The cable from the generator plugs into the back of the rear block.
    let inlet = v(g.x, g.y + y - 0.05, g.z - 4.15);
    d.sockets.cable =
        Some((Designer::local(Bone::Backpack, GEN_OUTLET), Designer::local(Bone::Weapon, inlet)));
}

// --- The Corona's back: its wards on their racks. ---

/// The wards on the right rack (the left's mirror it): each disc's centre, and how far it
/// tilts out. Three high in an arc over and out past the shoulder, two lower and further back,
/// overlapping them.
const RACK: [(Vec3, f32); 5] = [
    (Vec3::new(1.95, 7.7, -2.85), 0.1),
    (Vec3::new(3.5, 7.15, -2.7), 0.25),
    (Vec3::new(4.6, 5.85, -2.55), 0.4),
    (Vec3::new(2.35, 6.1, -3.35), 0.15),
    (Vec3::new(3.75, 4.8, -3.2), 0.35),
];
/// A ward disc's radius and depth.
pub(crate) const DISC_R: f32 = 0.9;
const DISC_DEPTH: f32 = 0.62;
/// Where the right rack's pylon rises from the back.
const PYLON: (Vec3, Vec3) = (Vec3::new(1.15, 4.4, -1.75), Vec3::new(1.65, 6.2, -2.35));

/// One ward disc, its face looking along `xf`'s +z: a drum with a raised rim, three grooves 120°
/// apart across its face round the yellow lens, and a tab with a forked lug off its rim along
/// `xf`'s -y.
pub(crate) fn ward_disc_shape(o: &mut On<'_>, xf: Affine3A) {
    let (r, h) = (DISC_R, DISC_DEPTH * 0.5);
    let face = xf * Affine3A::from_rotation_x(FRAC_PI_2);
    o.lathe(
        &[
            (0.0, -h),
            (r * 0.9, -h),
            (r, -h + 0.08),
            (r, h - 0.02),
            (r * 0.97, h + 0.04),
            (r * 0.87, h + 0.04),
            (r * 0.85, h - 0.03),
            (0.0, h - 0.03),
        ],
        20,
        GUN,
        face,
    );
    o.lathe(&band(0.08, 0.17, h - 0.04, h + 0.02), 12, YELLOW, face);
    o.cylinder(0.09, 0.06, 10, LENS, face * at(0.0, h, 0.0));
    o.greeble(|o| {
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0 + FRAC_PI_2;
            let p = v(0.52 * a.cos(), 0.52 * a.sin(), h - 0.01);
            o.cube(v(0.5, 0.07, 0.04), 0.0, FRAME, xf * place(p, rz(a)));
        }
    });
    // The tab and its forked lug.
    o.cube(v(0.42, 0.42, 0.36), 0.04, GUN, xf * at(0.0, -r - 0.12, -0.05));
    for x in [-0.12, 0.12] {
        o.cube(v(0.1, 0.24, 0.26), 0.0, GUN, xf * at(x, -r - 0.42, -0.05));
    }
}

/// Where each racked ward disc sits, its face looking back (and out), in the suit's frame at rest.
fn racked() -> impl Iterator<Item = Affine3A> {
    Side::BOTH.into_iter().flat_map(|s| {
        RACK.iter().map(move |&(c, out)| {
            // Facing back, tipped out, its tab toward the pylon.
            let to_pylon = (PYLON.1 - c).truncate().normalize_or(Vec2::NEG_Y);
            let spin = (-to_pylon.x).atan2(-to_pylon.y);
            sided(s, place(c, ry(PI - out) * rz(spin)))
        })
    })
}

fn racks(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.52);
    // A small pack low on the back for the main thrusters.
    b.block(v(1.9, 0.75, 0.7), v2(0.9, 0.85), Vec2::ZERO, 0.1, BODY, at(0.0, 3.55, -1.6));
    for s in Side::BOTH {
        // The pylon rising behind the shoulder, and a clamp out to each disc's tab.
        let (lo, hi) = PYLON;
        b.block(
            v(0.5, (hi - lo).length() + 0.4, 0.55),
            v2(0.85, 0.85),
            Vec2::ZERO,
            0.08,
            GUN,
            sided(s, place((lo + hi) * 0.5, toward(hi - lo))),
        );
        for (c, _) in RACK {
            let tab = c + (hi - c).normalize() * (DISC_R + 0.3);
            b.block(
                v(0.22, (hi - tab).length() + 0.1, 0.22),
                v2(1.0, 1.0),
                Vec2::ZERO,
                0.03,
                GUN,
                sided(s, place((tab + hi) * 0.5, toward(tab - hi))),
            );
        }
    }
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.55, 3.15, -1.75)), v(0.0, -0.6, -0.8), 0.36);
    }
    let mut p = d.on(Bone::Props);
    p.seed(0.8);
    let homes: Vec<Affine3A> = racked().collect();
    for (k, xf) in homes.iter().enumerate() {
        p.seed(0.8 + k as f32 * 0.017);
        ward_disc_shape(&mut p, *xf);
    }
    d.sockets.wards = homes
        .iter()
        .map(|xf| {
            (Designer::local(Bone::Props, xf.translation.into()), xf.transform_vector3(Vec3::Z).normalize())
        })
        .collect();
}

// --- The Corona's arms: the beam gun and the crash shield. ---

/// The beam gun: a chunky box pistol, a grip under it, a short barrel and the round green lens
/// over its rear.
fn beam_gun(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let mut w = d.on(Bone::Weapon);
    w.seed(0.62);
    w.cube(v(0.34, 1.0, 0.46), 0.05, GUN, place(v(g.x, g.y - 0.15, g.z), rx(-0.25)));
    w.block(v(0.66, 0.9, 2.5), v2(0.94, 0.94), Vec2::ZERO, 0.1, GUN, at(g.x, g.y + 0.62, g.z + 0.75));
    w.block(v(0.5, 0.42, 1.1), v2(0.9, 0.9), Vec2::ZERO, 0.06, GUN, at(g.x, g.y + 0.9, g.z + 2.4));
    w.cylinder(0.2, 0.5, 10, STEEL, place(v(g.x, g.y + 0.9, g.z + 3.1), rx(FRAC_PI_2)));
    w.cube(v(0.6, 0.3, 0.6), 0.04, GUN, at(g.x, g.y + 0.25, g.z + 1.35));
    w.cube(v(0.28, 0.16, 1.4), 0.02, GUN, at(g.x, g.y + 1.12, g.z + 0.5));
    // The round lens over its rear, lit in the livery's eye colour.
    let lens = place(v(g.x, g.y + 1.12, g.z - 0.35), Quat::IDENTITY);
    w.cylinder(0.2, 0.24, 12, GUN, lens);
    w.cylinder(0.14, 0.06, 12, EYE, lens * at(0.0, 0.14, 0.0));
    w.greeble(|w| {
        for k in 0..3 {
            w.cube(v(0.7, 0.06, 0.12), 0.0, FRAME, at(g.x, g.y + 0.62, g.z + 0.2 + k as f32 * 0.3));
        }
    });
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + 0.9, g.z + 3.4));
}

/// The crash shield on the left forearm, facing out: a dark disc with a raised ring, three bosses
/// 120° apart (its field generators) and the stepped hub whose emitter is the saber's.
fn crash_shield(d: &mut Designer) {
    let c = j(Bone::Shield) + v(-0.45, -0.05, 0.15);
    // Local +y out of its face (-x), turned a little forward.
    let face = place(c, Quat::from_rotation_arc(Vec3::Y, v(-1.0, 0.0, 0.25).normalize()));
    let mut s = d.on(Bone::Shield);
    s.seed(0.71);
    // Its mount on the forearm's outer face.
    s.block(v(0.5, 0.9, 1.0), v2(0.9, 0.9), Vec2::ZERO, 0.06, GREY, at(-4.4, 1.45, 0.85));
    s.lathe(
        &[
            (0.0, -0.22),
            (2.15, -0.22),
            (2.3, -0.08),
            (2.3, 0.08),
            (2.15, 0.2),
            (1.72, 0.22),
            (1.62, 0.34),
            (1.38, 0.34),
            (1.28, 0.22),
            (0.82, 0.22),
            (0.7, 0.36),
            (0.56, 0.36),
            (0.5, 0.58),
            (0.36, 0.62),
            (0.0, 0.62),
        ],
        36,
        GUN,
        face,
    );
    s.cylinder(0.24, 0.1, 12, FRAME, face * at(0.0, 0.64, 0.0));
    for k in 0..3 {
        let a = k as f32 * TAU / 3.0 + FRAC_PI_2;
        let p = v(1.0 * a.cos(), 0.36, 1.0 * a.sin());
        s.lathe(
            &[(0.0, 0.0), (0.3, 0.0), (0.3, 0.22), (0.24, 0.28), (0.0, 0.28)],
            12,
            GUN,
            face * at(p.x, p.y - 0.14, p.z),
        );
    }
    s.greeble(|s| {
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0 + FRAC_PI_2 + PI / 3.0;
            s.cube(
                v(0.6, 0.04, 0.06),
                0.0,
                FRAME,
                face * place(v(1.9 * a.cos(), 0.21, 1.9 * a.sin()), ry(-a)),
            );
        }
    });
    // The saber's blade leaves the hub's emitter, straight out of the face.
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.55, w.z + 0.2);
    let dir = v(0.0, 0.62, 0.78).normalize();
    d.on(Bone::HandL).cylinder(0.2, 1.2, 8, GUN, place(hilt, Quat::from_rotation_arc(Vec3::Y, dir)));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt + dir * 0.6), dir);
}
