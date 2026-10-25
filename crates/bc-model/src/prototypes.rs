//! The Almagest's prototype pair, two suits built on one frame: the AL-13X1 Sagittarius, the
//! spear, and the AL-13X2 Corona, the shield. They share everything but the head's hood and its
//! antenna, what rides the back, the weapons and the colours. Their bulk is in round masses,
//! lofted through rounded sections ([`crate::kit::Builder::loft`]):
//!
//! - a small round hooded head sunk between the chest's side blocks, which stand as high as its
//!   camera: the square camera in a grey bezel under the hood's peak, a pale band round the hood's
//!   brim; two round sensors on the chest's top either side of its chin, over a grey tube handle;
//!   a ribbed collar behind;
//! - a broad chest: the hatch standing forward down its middle with a raised inverted-trapezoid
//!   panel, and the side blocks stacked either side, the lower with grey louvres in its front, the
//!   upper a step back and standing up beside the head; a round hatch in the back, and a pale band
//!   round the waist under it all;
//! - shoulders that are big pale spheres, each under a heavy rounded block down its outside with a
//!   round grey port in its front, and a cap over its top stepping down toward the head;
//! - short round upper arms with a raised ring, big round gauntlets flaring to their cuffs, grey
//!   fists;
//! - a round lens in the crotch, short shield-shaped front skirts, and a plate over each hip;
//! - heavy thighs from under the belt to the knee: a rounded core armoured down its front in two
//!   tiers, the upper a step proud of the lower with a raised panel on it, and down its outside
//!   under a plate where the front turns to the side, coming down past it to a notched point beside
//!   the knee; a tall knee guard standing out of the shin under the thigh; calves swelling back and
//!   out to their widest just under the knee and tapering to the ankle, a ridge down the front and
//!   a plate down the outside; and heavy boots: a rounded shaft over the ankle with a thick plate
//!   rising up its front, a grey band behind and a big grey disc either side, on a thick platform
//!   with a high heel and a long toe on its own hinge, a narrower block raised along it, and a
//!   vernier in the sole under the heel and under the toe.
//!
//! The Sagittarius wears a single swept horn and carries the beam cannon cabled to the generator
//! high on its back behind its head, a ring as tall as its torso with an iris over its core; the
//! Corona wears a wide crescent on a short stalk, ten ward discs racked on two pylons in a crown
//! behind its head and shoulders (on [`Bone::Props`], so the client can send them out:
//! [`crate::ward_disc`]), the disc shield and a beam gun.
//!
//! The pale band and spheres are mint on the Sagittarius and lilac on the Corona; the inner frame,
//! the hands and the ankles are grey, the weapons, the generator, the wards and the shield a
//! dark slate.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::detail::{Cut, GREY, LENS, band, port, slot, stacked, swap_x, toward};
use crate::frames::{
    BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, TRIM, YELLOW, at, j, nozzle, place, rx, ry, rz, sided, sole,
    sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

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

/// Heights on the head: the foot of the face under its brim, the hood's brim band and its crown.
/// The head is small, as the kit's is: the chest's side blocks stand as high as its camera.
const FACE_FOOT: f32 = 6.22;
const BROW: (f32, f32) = (6.86, 6.98);
const CROWN: f32 = 7.52;
/// The hood's radius at its brim, and how far forward its axis stands.
const HOOD_R: f32 = 0.72;
const HOOD_Z: f32 = 0.0;
/// The square camera's centre, under the hood's peak.
const CAMERA: Vec3 = Vec3::new(0.0, 6.62, 0.86);

fn head(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let mut h = d.on(Bone::Head);
    h.seed(0.06 + if t == Twin::Corona { 0.5 } else { 0.0 });
    // The neck, grey, turning inside the collar.
    h.cylinder(0.42, 0.8, 12, GREY, at(0.0, 5.95, 0.05));
    // The face under the brim: a coloured snout, widest under the brim and narrowing to its chin,
    // its cheeks swept back.
    h.block(v(0.7, 0.72, 1.1), v2(1.4, 1.0), v2(0.0, -0.04), 0.1, BODY, at(0.0, FACE_FOOT + 0.36, 0.3));
    h.block(v(0.46, 0.24, 0.45), v2(1.2, 1.0), Vec2::ZERO, 0.05, BODY, at(0.0, FACE_FOOT + 0.04, 0.6));
    for s in Side::BOTH {
        let cheek = sided(s, place(v(0.5, 6.54, 0.26), ry(0.2)));
        h.block(v(0.18, 0.66, 1.0), v2(1.0, 0.8), v2(0.0, -0.08), 0.05, BODY, cheek);
        slot(&mut h, 0.34, cheek * at(0.1, -0.05, 0.0) * Affine3A::from_rotation_y(FRAC_PI_2));
    }
    // The square camera in its grey bezel: its glass lit in the livery's eye colour.
    h.cube(v(0.5, 0.44, 0.16), 0.04, GREY, at(CAMERA.x, CAMERA.y, CAMERA.z - 0.06));
    h.cube(v(0.34, 0.28, 0.1), 0.02, EYE, at(CAMERA.x, CAMERA.y, CAMERA.z + 0.01));
    // The hood: a round dome whose brim is the pale band, its peak coming down over the camera.
    let axis = at(0.0, 0.0, HOOD_Z) * Affine3A::from_scale(v(1.0, 1.0, 1.12));
    h.lathe(&band(HOOD_R - 0.05, HOOD_R + 0.04, BROW.0, BROW.1), 28, pale, axis);
    h.lathe(
        &[
            (0.0, BROW.0 - 0.02),
            (HOOD_R, BROW.0 - 0.02),
            (HOOD_R, BROW.0),
            (HOOD_R + 0.01, BROW.1),
            (HOOD_R, BROW.1 + 0.1),
            (0.62, BROW.1 + 0.26),
            (0.5, BROW.1 + 0.39),
            (0.32, CROWN - 0.04),
            (0.0, CROWN),
        ],
        28,
        BODY,
        axis,
    );
    // The brow: two plates out of the dome's front meeting in a ridge and coming down to a point
    // over the camera, the pale band's front edge along their foot.
    for side in Side::BOTH {
        let brow = [
            (0.0, 6.84, 0.77),
            (0.55, 6.98, 0.53),
            (0.0, 7.13, 0.73),
            (0.55, 7.21, 0.5),
            (0.0, 6.84, 0.98),
            (0.55, 6.98, 0.77),
            (0.0, 7.13, 0.92),
            (0.55, 7.21, 0.72),
        ]
        .map(|(x, y, z)| v(x, y, z));
        let brow = if side == Side::R { brow } else { swap_x(brow.map(|p| v(-p.x, p.y, p.z))) };
        h.hexa(brow, 0.05, BODY, Affine3A::IDENTITY);
        let edge = (v(0.0, 6.83, 0.97) + v(0.54 * side.sign(), 6.97, 0.75)) * 0.5;
        let run = v(0.54 * side.sign(), 0.14, -0.22);
        h.cube(v(0.05, run.length() + 0.04, 0.06), 0.0, pale, place(edge, toward(run)));
    }
    match t {
        Twin::Sagittarius => {
            // The horn: one tall blade rising out of the brow's ridge, swept back over the hood.
            let horn = [
                v2(0.28, 0.0),
                v2(0.53, 0.06),
                v2(0.27, 0.95),
                v2(-0.34, 2.18),
                v2(-0.5, 2.5),
                v2(-0.42, 2.0),
                v2(-0.11, 0.86),
                v2(0.02, 0.21),
            ];
            let xf = place(v(0.0, 6.98, 0.62), ry(-FRAC_PI_2) * rz(-0.06));
            h.extrude(&horn, 0.2, BODY, xf);
        }
        Twin::Corona => {
            // The crescent: a wide gull wing on a short stalk out of the brow's ridge, its tips
            // turned up.
            h.block(
                v(0.2, 0.4, 0.26),
                v2(0.8, 0.8),
                Vec2::ZERO,
                0.04,
                BODY,
                place(v(0.0, 7.26, 0.75), rx(-0.3)),
            );
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
                h.extrude(&wing, 0.14, BODY, sided(s, place(v(0.0, 7.47, 0.69), rx(-0.35))));
            }
        }
    }
    // The cockpit's camera is the square one.
    d.sockets.eye = Designer::local(Bone::Head, CAMERA + v(0.0, 0.0, 0.08));
    d.head_base = FACE_FOOT + 0.1;
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half), the mass behind its blocks: (y, outer x, back z, front z) at
/// its foot and at its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.35, 1.75, -1.3, 0.9), (6.0, 2.05, -1.45, 0.9)];

/// The hatch down the chest's middle (right half), in two tiers, each given by its foot and its
/// top as (y, half width, front z at the middle): it stands forward of the side blocks, furthest
/// forward and widest a little under halfway up, its top leaning back under the sensors. Its
/// front falls away outboard at `CHEST_FALL` per metre.
const HATCH: [[(f32, f32, f32); 2]; 2] =
    [[(3.45, 0.84, 1.38), (4.75, 1.1, 1.62)], [(4.75, 1.1, 1.62), (5.9, 1.14, 1.3)]];
const CHEST_FALL: f32 = 0.24;

/// The side blocks beside the hatch (right side), stacked: the lower, with the louvres in its
/// front, and the upper a step back and in, standing up beside the head as high as its camera.
/// Each given by its foot and its top.
const SIDE_BLOCKS: [[Cut; 2]; 2] = [
    [(3.42, 1.02, 2.32, -1.32, 1.1), (5.0, 1.06, 2.6, -1.5, 1.3)],
    [(5.0, 1.12, 2.54, -1.46, 1.1), (6.72, 1.16, 2.38, -1.28, 0.74)],
];

/// A frame on a block's front (`block` as for [`stacked`]) at (`x`, `y`): z out of the face, y up
/// it.
fn on_front(block: [Cut; 2], x: f32, y: f32, lift: f32) -> Affine3A {
    let [(y0, .., f0), (y1, .., f1)] = block;
    let z = f0 + (f1 - f0) * (y - y0) / (y1 - y0);
    let tilt = ((f1 - f0) / (y1 - y0)).atan();
    place(v(x, y, z + lift), rx(-tilt))
}

/// The hatch's front at height `y` and `x` out (on the right), and its outward normal there.
fn hatch_front(x: f32, y: f32) -> (Vec3, Vec3) {
    let tier = HATCH.iter().find(|t| y <= t[1].0).unwrap_or(&HATCH[1]);
    let [(y0, _, f0), (y1, _, f1)] = *tier;
    let rise = (f1 - f0) / (y1 - y0);
    let z = f0 + rise * (y - y0) - CHEST_FALL * x;
    (v(x, y, z), v(CHEST_FALL, -rise, 1.0).normalize())
}

/// Half the hatch's raised panel's width at height `y`: an inverted trapezoid.
fn panel_half(y: f32) -> f32 {
    0.42 + (y - 3.6) / 1.75 * 0.34
}

fn chest(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
        // The core, and the hatch's half in front of it.
        let [(y0, x0, b0, f0), (y1, x1, b1, f1)] = CORE;
        let core = [
            v(0.0, y0, b0),
            v(x0, y0, b0),
            v(0.0, y1, b1),
            v(x1, y1, b1),
            v(0.0, y0, f0),
            v(x0, y0, f0),
            v(0.0, y1, f1),
            v(x1, y1, f1),
        ];
        c.hexa(core, 0.2, BODY, flat);
        for [(y0, w0, f0), (y1, w1, f1)] in HATCH {
            let tier = [
                v(0.0, y0, 0.4),
                v(w0, y0, 0.4),
                v(0.0, y1, 0.4),
                v(w1, y1, 0.4),
                v(0.0, y0, f0),
                v(w0, y0, f0 - CHEST_FALL * w0),
                v(0.0, y1, f1),
                v(w1, y1, f1 - CHEST_FALL * w1),
            ];
            c.hexa(tier, 0.16, BODY, flat);
        }
        // The hatch's raised panel, following both tiers' faces.
        for (y0, y1) in [(3.6, 4.75), (4.75, 5.35)] {
            let corner = |x: f32, y: f32, out: f32| {
                let (p, n) = hatch_front(x, y);
                p + n * out
            };
            let (w0, w1) = (panel_half(y0), panel_half(y1));
            let panel = [
                corner(0.0, y0, -0.08),
                corner(w0, y0, -0.08),
                corner(0.0, y1, -0.08),
                corner(w1, y1, -0.08),
                corner(0.0, y0, 0.14),
                corner(w0, y0, 0.14),
                corner(0.0, y1, 0.14),
                corner(w1, y1, 0.14),
            ];
            c.hexa(panel, 0.05, BODY, flat);
        }
        // The side blocks, and the louvres in the lower one's front: a dark recess and five grey
        // slats; seams across the upper.
        for block in SIDE_BLOCKS {
            c.hexa(stacked(block), 0.2, BODY, flat);
        }
        let vent = sided(s, on_front(SIDE_BLOCKS[0], 1.74, 4.22, 0.0));
        c.cube(v(0.9, 1.05, 0.12), 0.02, FRAME, vent);
        for k in 0..5 {
            let y = -0.4 + k as f32 * 0.2;
            c.cube(v(0.82, 0.1, 0.16), 0.01, TRIM, vent * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.35));
        }
        c.greeble(|c| {
            for y in [5.75, 6.25] {
                c.cube(v(1.0, 0.06, 0.05), 0.0, FRAME, sided(s, on_front(SIDE_BLOCKS[1], 1.8, y, 0.0)));
            }
            c.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, sided(s, on_front(SIDE_BLOCKS[0], 1.22, 4.4, 0.0)));
        });
        // The round sensors on the hatch's top either side of the chin, in their housings.
        let sensor = sided(s, place(v(0.76, 6.1, 1.0), ry(0.18)));
        c.block(v(0.56, 0.56, 0.52), v2(0.9, 0.85), Vec2::ZERO, 0.08, BODY, sensor);
        port(&mut c, 0.24, sensor * at(0.0, 0.0, 0.27));
        c.greeble(|c| {
            let lens = sensor * at(0.0, 0.0, 0.4);
            c.cube(v(0.26, 0.05, 0.03), 0.0, FRAME, lens);
            c.cube(v(0.05, 0.26, 0.03), 0.0, FRAME, lens);
        });
        // The shoulder's socket in the chest's side.
        c.cylinder(0.62, 0.5, 14, GREY, sided(s, place(v(2.4, SPHERE.y, 0.0), rz(FRAC_PI_2))));
        // The back's side plates either side of the hatch.
        let plate = sided(s, place(v(1.35, 4.5, -1.58), ry(-0.15)));
        c.block(v(0.85, 1.9, 0.36), v2(0.8, 0.85), Vec2::ZERO, 0.14, BODY, plate);
        slot(&mut c, 0.45, plate * at(0.1, 0.2, -0.19) * Affine3A::from_rotation_y(PI));
    }
    // The hatch's seam across, under the panel's top.
    c.greeble(|c| {
        let (p, n) = hatch_front(0.0, 5.55);
        c.cube(v(1.8, 0.06, 0.05), 0.0, FRAME, place(p + n * 0.01, Quat::from_rotation_arc(Vec3::Z, n)));
    });
    // The grey tube handle across the chest's top, under the face, on its brackets.
    c.cylinder(0.09, 0.9, 10, GREY, place(v(0.0, 5.98, 1.3), rz(FRAC_PI_2)));
    for s in Side::BOTH {
        c.cube(v(0.12, 0.16, 0.4), 0.02, GREY, sided(s, at(0.4, 5.98, 1.14)));
    }
    // The ribbed collar behind the neck.
    let collar = at(0.0, 0.0, 0.0);
    let (from, to) = (FRAC_PI_2 + 0.7, FRAC_PI_2 - 0.7 + TAU);
    for k in 0..3 {
        let y = 5.9 + k as f32 * 0.16;
        c.lathe_arc(&band(0.6, 0.94 - k as f32 * 0.05, y, y + 0.11), 18, from, to, GREY, collar);
    }
    // The round hatch in the middle of the back.
    let hatch = place(v(0.0, 4.65, -1.5), rx(-FRAC_PI_2));
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
        &[(1.2, 2.72), (1.48, 2.72), (1.53, 2.8), (1.53, 3.2), (1.48, 3.28), (1.2, 3.28), (1.2, 2.72)],
        20,
        pale,
        squash,
    );
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer, t: Twin) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2 + if t == Twin::Corona { 0.3 } else { 0.0 });
    // The belt round the hips, rounded at its corners, and the crotch block under its front with a
    // box standing out of it over the lens.
    w.loft(
        &[
            hoop(2.02, 0.0, 0.0, 1.62, 1.22),
            hoop(2.12, 0.0, 0.0, 1.72, 1.32),
            hoop(2.6, 0.0, 0.0, 1.72, 1.32),
        ],
        3.2,
        28,
        BODY,
        Affine3A::IDENTITY,
    );
    w.block(v(1.2, 1.3, 1.5), v2(1.2, 1.0), v2(0.0, -0.05), 0.14, BODY, at(0.0, 1.45, 0.42));
    w.block(v(0.62, 0.62, 0.3), v2(0.9, 1.0), Vec2::ZERO, 0.06, BODY, at(0.0, 1.78, 1.2));
    // The round lens in its foot: a coloured rim round a grey ring and the dark glass.
    let boss = place(v(0.0, 1.0, 1.08), rx(FRAC_PI_2));
    w.lathe(&band(0.32, 0.52, -0.05, 0.26), 20, BODY, boss);
    w.lathe(&band(0.22, 0.34, -0.05, 0.2), 18, GREY, boss);
    w.lathe(&[(0.0, -0.05), (0.24, -0.05), (0.22, 0.08), (0.12, 0.14), (0.0, 0.15)], 16, LENS, boss);
    for s in Side::BOTH {
        // The front skirt over the top of the thigh: a shield of a plate hung from the belt and
        // turned out round the thigh's shell, its hem rising toward the outside, a raised panel
        // down its inner half.
        let skirt = sided(s, place(v(1.2, 1.5, 1.42), ry(0.36) * rx(-0.16)));
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
        w.hexa(plate(-0.74, 0.74, 0.8, (-0.98, -0.5), -0.2, 0.2), 0.14, BODY, skirt);
        w.hexa(plate(-0.66, 0.0, 0.62, (-0.72, -0.5), 0.1, 0.32), 0.06, BODY, skirt);
        w.greeble(|w| {
            w.cube(v(0.06, 1.2, 0.05), 0.0, FRAME, skirt * at(0.3, 0.0, 0.21));
        });
        // The hip's side plate under the belt, over the thigh's ball joint.
        let side = sided(s, place(v(2.12, 1.72, -0.12), rz(0.12)));
        w.block(v(0.55, 1.05, 1.95), v2(0.85, 0.9), Vec2::ZERO, 0.16, BODY, side);
        slot(&mut w, 0.42, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.05, 0.29));
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.9, 1.62, -1.38), rx(0.3)));
        w.block(v(1.5, 1.5, 0.55), v2(0.92, 0.85), v2(0.0, 0.05), 0.18, BODY, rear);
        slot(&mut w, 0.35, rear * at(0.2, 0.0, -0.29) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder sphere's centre (right side), high beside the head.
const SPHERE: Vec3 = Vec3::new(3.3, 5.4, 0.0);
const SPHERE_R: f32 = 1.14;

/// The bracket's block down the outside of the sphere, as hoops out along x from inside the
/// sphere, each as high as twice its half width round its middle (5.98 m up) and as deep as
/// twice its half depth: a rounded block, its outer face set in a little round its edge.
const BRACKET: [Hoop; 3] =
    [hoop(3.6, 0.0, 0.0, 1.04, 1.2), hoop(4.48, 0.0, 0.0, 1.04, 1.2), hoop(4.62, 0.0, 0.0, 0.92, 1.08)];

/// The gauntlet, up the forearm from the wrist (x out, z forward): the cuff's flare, a step in,
/// then the box up to the elbow, rounding in over its barrel.
const GAUNTLET: [Hoop; 6] = [
    hoop(0.0, 0.0, 0.05, 0.9, 0.95),
    hoop(0.4, 0.0, 0.05, 0.94, 0.99),
    hoop(0.4, 0.0, 0.05, 0.8, 0.86),
    hoop(1.15, 0.02, 0.04, 0.84, 0.9),
    hoop(1.65, 0.02, 0.02, 0.78, 0.82),
    hoop(1.95, 0.0, 0.0, 0.58, 0.6),
];

fn arms(d: &mut Designer, t: Twin) {
    let pale = t.pale();
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The pale sphere, and the heavy bracket over its top and outer side: a thick block down
        // the outside, its corners cut, with the round port in its front, and a cap over the top
        // stepping down toward the head; the sphere shows inside and under them.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.sphere(SPHERE_R, 16, pale, sided(s, Affine3A::from_translation(SPHERE)));
        let bracket = sided(s, place(v(0.0, 5.98, 0.0), rz(-FRAC_PI_2)));
        o.loft(&BRACKET, 3.4, 28, BODY, bracket);
        let cap = [
            (2.68, 6.22, -1.0),
            (3.9, 6.3, -1.0),
            (2.72, 6.7, -1.0),
            (3.9, 6.94, -1.0),
            (2.68, 6.22, 1.0),
            (3.9, 6.3, 1.0),
            (2.72, 6.7, 1.0),
            (3.9, 6.94, 1.0),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(cap, 0.26, BODY, sided(s, Affine3A::IDENTITY));
        port(&mut o, 0.36, sided(s, at(4.1, 5.5, 1.14)));
        slot(&mut o, 0.5, sided(s, place(v(4.62, 6.1, -0.4), ry(FRAC_PI_2))));
        // A running light at the bracket's outer front corner, high: red on the suit's own left
        // (+x), green on its right.
        let nav = sided(s, at(4.25, 6.8, 0.98));
        o.cube(v(0.34, 0.16, 0.2), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));
        o.cylinder(0.42, 0.7, 10, GREY, sided(s, place(v(2.35, SPHERE.y, 0.0), rz(FRAC_PI_2))));

        // The short upper arm: a coloured drum with a raised ring round it, on the grey frame, out
        // from under the sphere.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.44, 1.7, 12, GREY, up_xf * at(0.0, 0.95, 0.0));
        u.lathe(
            &[
                (0.0, 0.5),
                (0.66, 0.5),
                (0.75, 0.6),
                (0.75, 0.86),
                (0.82, 0.92),
                (0.82, 1.08),
                (0.75, 1.14),
                (0.75, 1.4),
                (0.66, 1.5),
                (0.0, 1.5),
            ],
            18,
            BODY,
            up_xf,
        );

        // The gauntlet: the elbow's barrel, then a big rounded box flaring to its cuff, a grey
        // panel high on its outer face and a dark slot down its front.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.56, 1.15, 14, GREY, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&GAUNTLET, 3.4, 24, BODY, fo_xf);
        f.cube(v(0.08, 0.72, 0.8), 0.02, GREY, fo_xf * at(0.83, 1.18, -0.06));
        f.cube(v(0.36, 0.7, 0.08), 0.02, FRAME, fo_xf * at(0.0, 1.0, 0.89));
        f.greeble(|f| {
            f.cube(v(0.05, 0.08, 1.3), 0.0, FRAME, fo_xf * at(0.87, 0.62, 0.05));
            f.cube(v(1.3, 0.08, 0.05), 0.0, FRAME, fo_xf * at(0.0, 0.62, 0.93));
        });
        f.cylinder(0.46, 0.3, 12, GREY, fo_xf);
    }
    crate::frames::hands(d, GREY);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): a rounded box from
/// just over the knee guard to under the belt, widening out to its broadest a little over halfway
/// up, and closing in under the hip. Its armour stands on it ([`THIGH_FRONT`], [`THIGH_SIDE`]).
const THIGH_SHELL: [Hoop; 7] = [
    hoop(0.45, 0.22, 0.08, 1.0, 0.92),
    hoop(0.95, 0.36, 0.12, 1.36, 1.14),
    hoop(1.7, 0.5, 0.14, 1.58, 1.28),
    hoop(2.45, 0.55, 0.1, 1.62, 1.28),
    hoop(3.15, 0.52, 0.04, 1.48, 1.16),
    hoop(3.6, 0.46, 0.0, 1.2, 1.0),
    hoop(3.88, 0.4, 0.0, 0.76, 0.64),
];

/// The plate down the thigh's front in two tiers, in the core's frame, each by its foot and its
/// top: the lower narrowing to the knee, the upper standing a step proud of it, broadest where
/// they meet and leaning back under the skirt.
const THIGH_FRONT: [[Cut; 2]; 2] = [
    [(0.75, -0.72, 1.05, 0.55, 1.36), (2.05, -0.98, 1.42, 0.65, 1.6)],
    [(2.05, -1.04, 1.5, 0.65, 1.7), (3.3, -0.9, 1.36, 0.55, 1.4)],
];

/// The plate down the thigh's outside, under the fin, by its foot and its top.
const THIGH_SIDE: [Cut; 2] = [(0.95, 1.6, 2.02, -0.9, 0.78), (3.15, 1.72, 2.22, -1.0, 0.84)];

/// The lower leg, up its axis from the ankle to the knee (x out, z forward): slim over the boot,
/// swelling back and out to the calf's widest just under the knee, and closing in behind the knee
/// guard.
const SHIN_SHELL: [Hoop; 9] = [
    hoop(0.75, 0.02, 0.0, 0.78, 0.82),
    hoop(1.5, 0.04, -0.05, 0.85, 0.91),
    hoop(2.5, 0.07, -0.15, 1.01, 1.07),
    hoop(3.4, 0.1, -0.26, 1.22, 1.24),
    hoop(4.15, 0.12, -0.32, 1.35, 1.33),
    hoop(4.75, 0.12, -0.32, 1.35, 1.31),
    hoop(5.25, 0.1, -0.28, 1.19, 1.14),
    hoop(5.6, 0.08, -0.24, 0.86, 0.82),
    hoop(5.75, 0.06, -0.22, 0.5, 0.5),
];

/// The plate down the calf's outside (in the lower leg's frame), from over the boot to the calf's
/// widest, by its foot and its top.
const CALF_SIDE: [Cut; 2] = [(2.4, 1.0, 1.32, -1.0, 0.45), (4.9, 1.2, 1.66, -1.35, 0.4)];

/// The plate on the thigh's shell where its front turns to its outside (its outline running
/// back and out along x, up along y, from where it hangs): its top in the shell, coming down past
/// the shell's foot to a point in front, a notch behind it and a shorter point at the back.
const FIN: [Vec2; 8] = [
    Vec2::new(-0.7, 0.5),
    Vec2::new(-0.82, -0.6),
    Vec2::new(-0.55, -1.5),
    Vec2::new(0.05, -2.1),
    Vec2::new(0.35, -1.4),
    Vec2::new(0.85, -1.75),
    Vec2::new(1.08, -0.5),
    Vec2::new(1.0, 0.55),
];

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);

        // The thigh: a rounded core on a grey ball at the hip, armoured down its front in two
        // tiers and down its outside, and the fin over the outside plate flaring out to its point
        // beside the knee.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.8, 6, GREY, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH_SHELL, 4.0, 28, BODY, along_thigh);
        for tier in THIGH_FRONT {
            th.hexa(stacked(tier), 0.16, BODY, along_thigh);
        }
        th.hexa(stacked(THIGH_SIDE), 0.14, BODY, along_thigh);
        // A raised panel on the upper tier, and a slot down each tier and the side plate.
        let [(y0, .., f0), (y1, .., f1)] = THIGH_FRONT[1];
        let upper = along_thigh * place(v(0.23, 2.68, (f0 + f1) * 0.5), rx(-((f1 - f0) / (y1 - y0)).atan()));
        th.block(v(1.35, 0.9, 0.16), v2(0.85, 0.9), Vec2::ZERO, 0.04, BODY, upper);
        slot(&mut th, 0.5, upper * at(0.42, 0.0, 0.08));
        let [(y0, .., f0), (y1, .., f1)] = THIGH_FRONT[0];
        let lower = along_thigh * place(v(0.15, 1.35, (f0 + f1) * 0.5), rx(-((f1 - f0) / (y1 - y0)).atan()));
        slot(&mut th, 0.6, lower * at(-0.3, 0.0, 0.0));
        slot(&mut th, 0.7, along_thigh * place(v(2.13, 2.0, -0.45), ry(FRAC_PI_2)));
        let fin = along_thigh * place(v(2.0, 1.55, 1.24), ry(0.87) * rx(-0.28));
        th.extrude(&FIN, 0.32, BODY, fin);
        th.greeble(|th| {
            // The seam where the tiers meet, and the notch's dark step.
            th.cube(v(2.4, 0.06, 0.05), 0.0, FRAME, along_thigh * at(0.26, 2.05, 1.66));
            th.cube(v(0.07, 0.6, 0.05), 0.0, FRAME, fin * place(v(-0.2, -1.0, 0.17), rz(0.5)));
        });

        // The shin: the knee's grey barrel, the round shell of the calf, and the knee guard, a
        // tall block standing out of its front with its top tucked up under the thigh's shell and a
        // dark slot down its face.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.62, 1.3, 12, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN_SHELL, 3.4, 28, BODY, tube);
        // The ridge down the shin's front, from over the boot up to the knee guard.
        let ridge = |y: f32, z: f32, out: f32| {
            [v(-0.16, y, z - 0.12), v(0.16, y, z - 0.12), v(-0.1, y, z + out), v(0.1, y, z + out)]
        };
        let (lo, hi) = (ridge(1.45, 0.88, 0.08), ridge(4.3, 1.05, 0.1));
        m.hexa([lo[0], lo[1], hi[0], hi[1], lo[2], lo[3], hi[2], hi[3]], 0.04, BODY, tube);
        // The plate down the calf's outside, a slot in it.
        m.hexa(stacked(CALF_SIDE), 0.14, BODY, tube);
        slot(&mut m, 0.8, tube * place(v(1.53, 3.9, -0.45), ry(FRAC_PI_2)));
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        m.hexa(
            [
                (-0.62, -1.05, 0.4),
                (0.62, -1.05, 0.4),
                (-0.55, 1.15, 0.2),
                (0.55, 1.15, 0.2),
                (-0.55, -1.15, 1.42),
                (0.55, -1.15, 1.42),
                (-0.5, 1.15, 1.02),
                (0.5, 1.15, 1.02),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.16,
            BODY,
            guard,
        );
        // Its foot, a wedge narrowing forward to a blunt point under it.
        m.hexa(
            [
                (-0.5, -1.62, 0.6),
                (0.5, -1.62, 0.6),
                (-0.6, -1.0, 0.45),
                (0.6, -1.0, 0.45),
                (-0.16, -1.62, 1.3),
                (0.16, -1.62, 1.3),
                (-0.26, -1.0, 1.15),
                (0.26, -1.0, 1.15),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.08,
            BODY,
            guard,
        );
        m.block(
            v(0.36, 0.85, 0.12),
            v2(0.85, 1.0),
            Vec2::ZERO,
            0.03,
            FRAME,
            guard * place(v(0.0, 0.15, 1.22), rx(-0.18)),
        );
        m.greeble(|m| {
            m.cube(v(0.06, 1.4, 0.05), 0.0, FRAME, tube * place(v(0.92, 3.4, 0.82), ry(0.75) * rx(0.12)));
        });

        // The boot's shaft round the ankle: a rounded box over the foot of the shin, a grey band
        // round its back at the joint, and the big grey disc either side where it meets the foot.
        let x = ankle.x;
        let flat = sided(s, Affine3A::IDENTITY);
        let (y0, y1) = (ankle.y - 0.55, ankle.y + 1.1);
        m.loft(
            &[
                hoop(y0, x, -0.08, 1.12, 0.98),
                hoop(y0 + 0.9, x, -0.1, 1.1, 0.96),
                hoop(y1, x, -0.12, 0.98, 0.86),
                hoop(y1 + 0.08, x, -0.12, 0.86, 0.74),
            ],
            3.6,
            24,
            BODY,
            flat,
        );
        // Its front: a thick plate rising from over the instep to high on the shin, like a boot's
        // tongue, wider at its foot; a notch near its top.
        let tongue = [
            (-1.02, y0 + 0.15, 0.55),
            (1.02, y0 + 0.15, 0.55),
            (-0.84, y1 + 0.25, 0.62),
            (0.84, y1 + 0.25, 0.62),
            (-1.02, y0 + 0.15, 1.78),
            (1.02, y0 + 0.15, 1.78),
            (-0.84, y1 + 0.25, 1.2),
            (0.84, y1 + 0.25, 1.2),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.2, BODY, flat);
        let slope = v(0.0, y1 + 0.25 - (y0 + 0.15), 1.2 - 1.78).normalize();
        let face = sided(s, place(v(x, y1 - 0.02, 1.3), Quat::from_rotation_arc(Vec3::Y, slope)));
        m.greeble(|m| {
            m.cube(v(0.42, 0.07, 0.05), 0.0, FRAME, face * at(-0.06, 0.0, 0.0));
            m.cube(v(0.07, 0.36, 0.05), 0.0, FRAME, face * at(-0.24, -0.15, 0.0));
            m.cube(v(1.2, 0.06, 0.05), 0.0, FRAME, face * at(0.0, -0.95, 0.04));
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
            let hub = sided(s, place(v(x + side * 1.14, ankle.y - 0.2, ankle.z - 0.12), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                16,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }

        // The foot: a thick platform, the heel high at its back, the middle under the shaft's
        // tongue, and the long toe (on its own hinge) falling away to its tip with a narrower block
        // raised along it; a dark sole under it all, split at the hinge, a vernier in each half,
        // and a grey band round the heel over it.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, GREY, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        // The heel and the middle, one tall block, highest at the back.
        f.hexa(boot(x, base, (-7.5, -7.72), (1.26, 1.22), [-2.05, -1.82, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.36, 1.36), [-2.17, -2.14, -0.6, -0.6]),
            0.05,
            GREY,
            flat,
        );
        f.greeble(|f| {
            // The upper's seam along each side, the step to the toe box, and a slot up the heel's
            // back.
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 3.2), 0.0, FRAME, sided(s, at(x + side * 1.14, -8.3, -0.3)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.12, -8.25, 1.26)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.05, -2.0), rx(-0.25))));
        });
        let heel =
            Sole { z: (-2.15, 1.26), half: (1.34, 1.27), top: 0.34, pads: GREY, vernier: 0.15, r: 0.42 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        // The toe box: as wide, narrowing and falling away to a blunt, rounded tip.
        t.hexa(boot(x, base, (-7.8, -8.04), (1.22, 1.06), [1.32, 1.32, 3.35, 2.92]), 0.34, BODY, flat);
        // The narrower block raised along the toe, stepping down at its front.
        t.hexa(boot(x, -8.1, (-7.6, -7.9), (0.58, 0.5), [1.36, 1.36, 2.8, 2.52]), 0.12, BODY, flat);
        let toes = Sole { z: (1.34, 3.45), half: (1.27, 1.14), top: 0.34, pads: GREY, vernier: 2.3, r: 0.34 };
        sole(d, toe, s, x, toes);
    }
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

/// A block of a foot ([`frames::boot`](crate::frames::boot)), its sides leaning in [`BOOT_LEAN`].
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    crate::frames::boot(x, base, top, half, z, BOOT_LEAN)
}

// --- The Sagittarius's back: the generator, and its cannon. ---

/// The generator's ring: its centre high on the back behind the head, a little to the right,
/// facing back, and its radii.
const GEN: Vec3 = Vec3::new(0.7, 5.8, -3.05);
const GEN_R: f32 = 2.35;
const GEN_HOLE: f32 = 1.2;
const GEN_DEPTH: f32 = 1.5;

fn generator(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The mount off the back's hatch, and the struts out to the ring.
    b.block(v(1.6, 1.6, 0.8), v2(0.85, 0.9), Vec2::ZERO, 0.12, GUN, at(0.3, 4.9, -2.0));
    b.block(v(0.7, 2.4, 0.6), v2(1.0, 1.0), Vec2::ZERO, 0.08, GUN, place(v(0.6, 5.3, -2.3), rz(-0.2)));
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
        b.cube(v(0.2, 0.7, 0.24), 0.03, GUN, at(top.x + x, top.y + 0.25, top.z));
    }
    b.cube(v(1.7, 0.2, 0.24), 0.03, GUN, at(top.x, top.y + 0.62, top.z));
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
const GEN_OUTLET: Vec3 = Vec3::new(GEN.x + 1.15, GEN.y - GEN_R - 0.55, GEN.z + 0.3);

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
/// tilts out. Three in an arc from over the head out past the shoulder, two lower and further
/// back, overlapping them: the two racks crown the head.
const RACK: [(Vec3, f32); 5] = [
    (Vec3::new(1.75, 8.5, -2.85), 0.1),
    (Vec3::new(3.75, 7.85, -2.7), 0.25),
    (Vec3::new(5.0, 6.25, -2.55), 0.4),
    (Vec3::new(2.3, 6.45, -3.45), 0.15),
    (Vec3::new(4.05, 5.25, -3.4), 0.35),
];
/// A ward disc's radius and depth.
pub(crate) const DISC_R: f32 = 1.0;
const DISC_DEPTH: f32 = 0.66;
/// Where the right rack's pylon rises from the back.
const PYLON: (Vec3, Vec3) = (Vec3::new(1.15, 4.4, -1.75), Vec3::new(1.9, 6.8, -2.45));

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

// --- The Corona's arms: the beam gun and the disc shield. ---

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

/// The disc shield on the left forearm, facing out: a dark disc with a raised ring, three bosses
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
