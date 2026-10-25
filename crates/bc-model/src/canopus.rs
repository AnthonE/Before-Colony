//! PX-03 Canopus, the colonies' brawler: built for the desert, the heaviest armour of the five,
//! and the bulk to wade into a fight with a heat blade in each hand. Heavy and square-shouldered,
//! its masses lofted through rounded sections and plated over:
//!
//! - a small head set low between the shoulders: a white face under a heavy brow, two green eyes
//!   in a dark visor, a mouth of two slots and a red chin, cheek guards swept back to a round
//!   sensor either side, and one tall white crest standing straight up out of the brow;
//! - a deep chest: the hatch standing forward down its middle with a red sensor under the chin,
//!   the upper blocks either side carrying the big heat vents (grey slats in a dark well, framed
//!   in white), the ribs stepping in under them, a collar behind the neck;
//! - very broad shoulders, leaning out a little: round boxes lofted out past the chest and down
//!   over the upper arm, a white band round each hem, a crown of heat-vent fins on top; plated
//!   front, back and outside, the missile hatch on the front plate (a raised door, three launch
//!   tubes along its top over a white band), a round port in the outer plate;
//! - drum upper arms and big gauntlets with white cuffs, grey fists;
//! - a narrow waist over a red crotch block, front skirts with a white panel down each, and side
//!   skirts flaring out over the hips;
//! - long heavy legs: thighs plated front, side and back over a squared core; a white knee guard;
//!   lower legs swelling to full calves behind and flaring to a lipped hem over the boot, plated
//!   all round: the shin in two faceted tiers, the lower standing out, plates down either side
//!   with a heat vent low on the outside, and a plate down the calf with another; heavy boots
//!   under the hem: a shaft with a tongue up its front and a grey hub either side, a tall heel, a
//!   long toe on its own hinge with a red block raised along it, verniers in the soles;
//! - a heavy backpack: a squared pack between two round tanks, fins on its top, a hatch down
//!   the middle of its back between two heat vents, two big bells under it in their housings;
//! - the beam machine gun in the right hand: a deep receiver with the energy drum on its inner
//!   side, a finned jacket round the barrel, a muzzle brake, a sight with its lens, a stock;
//! - two heat blades, crescent shotels on wrapped grips, their inner edges glowing: the left in
//!   the left fist, bowing out; the right clamped under the gun, hanging under the barrel like a
//!   sickle; their glowing edges turned in, so that together they close as a pincer (the Pincer
//!   Strike);
//! - the cross shield on the left forearm, white, a tan spine down it and a red boss where its
//!   arms cross, the slots in its arms where the blades lock to make it a crusher.
//!
//! Tan all over, white on the face, crest, vents' frames, hems, cuffs, knees and the shield; red
//! at the chin, the chest's sensor, the crotch and the toes; the inner frame, the hands and the
//! ankles grey, the weapons gunmetal.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::detail::{Cut, GREY, LENS, band, port, slot, stacked, swap_x, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, TRIM, at, j, nozzle, place, rx, ry, rz, sided, sole,
    sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// A heated blade's edge.
const HOT: Paint = Paint::Glow(paint::RED);

pub(crate) fn canopus(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    legs(d);
    backpack(d);
    machine_gun(d);
    blades(d);
    cross_shield(d);
}

/// A frame at `origin` whose x, y and z run along `x`, `y` and their cross product (`x` is made
/// square to `y`).
fn frame_of(origin: Vec3, x: Vec3, y: Vec3) -> Affine3A {
    let y = y.normalize();
    let x = (x - y * x.dot(y)).normalize();
    Affine3A::from_mat3_translation(Mat3::from_cols(x, y, x.cross(y)), origin)
}

/// A frame on a block's front (`block` as for [`stacked`]) at (`x`, `y`): z out of the face, y up
/// it.
fn on_front(block: [Cut; 2], x: f32, y: f32, lift: f32) -> Affine3A {
    let [(y0, .., f0), (y1, .., f1)] = block;
    let z = f0 + (f1 - f0) * (y - y0) / (y1 - y0);
    let tilt = ((f1 - f0) / (y1 - y0)).atan();
    place(v(x, y, z + lift), rx(-tilt))
}

/// A louvre on the face at `xf`'s origin (looking along its +z): a dark well `w` by `h` with `n`
/// slats across it, tipped down (up close only).
fn louvre(o: &mut On<'_>, w: f32, h: f32, n: u32, slat: Paint, xf: Affine3A) {
    o.cube(v(w, h, 0.12), 0.02, FRAME, xf);
    let pitch = h / n as f32;
    o.greeble(|o| {
        for k in 0..n {
            let y = -h * 0.5 + pitch * (k as f32 + 0.5);
            o.cube(
                v(w - 0.08, pitch * 0.5, 0.16),
                0.0,
                slat,
                xf * at(0.0, y, 0.03) * Affine3A::from_rotation_x(0.4),
            );
        }
    });
}

/// Heat-vent fins on the face at `xf`'s origin (looking along its +y): a dark well `w` by `d`,
/// and `n` fins standing `h` out of it, running along its z.
fn fins(o: &mut On<'_>, w: f32, d: f32, h: f32, n: u32, xf: Affine3A) {
    o.cube(v(w, 0.1, d), 0.02, FRAME, xf);
    let pitch = w / n as f32;
    for k in 0..n {
        let x = -w * 0.5 + pitch * (k as f32 + 0.5);
        o.cube(v(0.07, h, d - 0.12), 0.0, GREY, xf * at(x, h * 0.5, 0.0));
    }
}

// --- Head. ---

/// Heights on the head: the chin's foot, the line of the eyes, the brow's foot and the crown.
const CHIN: f32 = 6.1;
const EYES: f32 = 6.78;
const BROW: f32 = 6.92;
const CROWN: f32 = 7.66;
/// The cockpit's eye: between the eyes, at the face (the simulation's humanoid eye).
const CAMERA: Vec3 = Vec3::new(0.0, 6.75, 1.1);

/// The helmet, up its axis: from the jaw round the back, swelling a little at the brow and
/// closing over the crown.
const HELMET: [Hoop; 6] = [
    hoop(6.3, 0.0, -0.02, 0.68, 0.76),
    hoop(6.9, 0.0, -0.02, 0.78, 0.86),
    hoop(7.3, 0.0, -0.05, 0.74, 0.82),
    hoop(7.52, 0.0, -0.08, 0.58, 0.66),
    hoop(CROWN - 0.04, 0.0, -0.1, 0.32, 0.38),
    hoop(CROWN, 0.0, -0.1, 0.0, 0.0),
];

fn head(d: &mut Designer) {
    let mut h = d.on(Bone::Head);
    h.seed(0.05);
    // The neck, grey, turning inside the collar.
    h.cylinder(0.42, 0.8, 12, GREY, at(0.0, 5.95, 0.05));
    h.loft(&HELMET, 2.6, 28, BODY, Affine3A::IDENTITY);
    // The visor: a dark band across the face under the brow, and the two eyes in it, angled up
    // toward the outside.
    h.block(v(1.0, 0.34, 0.5), v2(1.0, 1.0), Vec2::ZERO, 0.04, FRAME, at(0.0, EYES, 0.84));
    for s in Side::BOTH {
        let eye = [
            (0.06, EYES - 0.09, 1.04),
            (0.46, EYES - 0.03, 1.02),
            (0.06, EYES + 0.06, 1.04),
            (0.46, EYES + 0.15, 1.02),
            (0.06, EYES - 0.09, 1.125),
            (0.46, EYES - 0.03, 1.105),
            (0.06, EYES + 0.06, 1.125),
            (0.46, EYES + 0.15, 1.105),
        ]
        .map(|(x, y, z)| v(x, y, z));
        h.hexa(eye, 0.015, EYE, sided(s, Affine3A::IDENTITY));
    }
    // The face under the visor: a white plate narrowing to the chin, the mouth's two slots in it,
    // and the red chin under it.
    h.block(v(0.6, 0.46, 0.46), v2(1.32, 1.0), v2(0.0, 0.03), 0.05, TRIM, at(0.0, 6.4, 0.86));
    h.greeble(|h| {
        for x in [-0.1, 0.1] {
            h.cube(v(0.06, 0.24, 0.04), 0.0, FRAME, place(v(x, 6.4, 1.1), rx(-0.07)));
        }
    });
    h.block(v(0.44, 0.2, 0.42), v2(0.8, 1.0), v2(0.0, 0.02), 0.04, ACCENT, at(0.0, CHIN + 0.1, 0.9));
    for s in Side::BOTH {
        // The cheek guards, swept back from the face to the sensor on either side of the helmet.
        let cheek = sided(s, place(v(0.6, 6.48, 0.3), ry(0.22) * rz(-0.06)));
        h.block(v(0.2, 0.8, 1.1), v2(1.0, 0.82), v2(0.0, 0.06), 0.05, BODY, cheek);
        slot(&mut h, 0.4, cheek * at(0.11, -0.05, 0.1) * Affine3A::from_rotation_y(FRAC_PI_2));
        // A vent in the helmet's side, over the sensor: a housing standing out of it, slotted.
        let vent = sided(s, place(v(0.66, 7.1, -0.32), ry(FRAC_PI_2) * rz(0.12)));
        h.block(v(0.62, 0.34, 0.24), v2(0.9, 0.8), Vec2::ZERO, 0.04, BODY, vent);
        h.greeble(|h| {
            for k in 0..3 {
                h.cube(v(0.48, 0.05, 0.05), 0.0, FRAME, vent * at(0.0, k as f32 * 0.09 - 0.09, 0.12));
            }
        });
        port(&mut h, 0.2, sided(s, place(v(0.8, 6.68, -0.12), ry(FRAC_PI_2))));
        // The brow: two plates out of the helmet's front meeting in a ridge over the visor.
        let brow = [
            (0.0, BROW, 0.84),
            (0.56, BROW + 0.06, 0.64),
            (0.0, BROW + 0.26, 0.78),
            (0.56, BROW + 0.32, 0.58),
            (0.0, BROW, 1.14),
            (0.56, BROW + 0.06, 0.94),
            (0.0, BROW + 0.26, 0.98),
            (0.56, BROW + 0.32, 0.78),
        ]
        .map(|(x, y, z)| v(x, y, z));
        let brow = if s == Side::R { brow } else { swap_x(brow.map(|p| v(-p.x, p.y, p.z))) };
        h.hexa(brow, 0.05, BODY, Affine3A::IDENTITY);
    }
    // The ridge from the brow back over the crown, and the crest: one tall white blade standing
    // up out of the brow in front of it, leaning back a little, a red sensor at its foot.
    let ridge = [
        v2(0.32, 0.06),
        v2(0.04, 0.56),
        v2(-0.38, 0.72),
        v2(-0.88, 0.7),
        v2(-1.23, 0.52),
        v2(-1.43, 0.26),
        v2(-1.28, 0.16),
        v2(-0.08, -0.04),
    ];
    h.extrude(&ridge, 0.28, BODY, place(v(0.0, BROW + 0.12, 0.58), ry(-FRAC_PI_2)));
    let crest = [
        v2(0.5, 0.0),
        v2(0.56, 0.16),
        v2(0.26, 0.9),
        v2(0.0, 1.62),
        v2(-0.18, 1.76),
        v2(-0.3, 1.7),
        v2(-0.24, 1.2),
        v2(-0.32, 0.5),
        v2(-0.5, 0.18),
        v2(-0.42, 0.0),
    ];
    h.extrude(&crest, 0.16, TRIM, place(v(0.0, BROW + 0.12, 0.58), ry(-FRAC_PI_2)));
    h.cube(v(0.2, 0.14, 0.12), 0.03, ACCENT, at(0.0, BROW + 0.14, 1.12));
    // The back of the helmet: a grey vent under its crown.
    h.greeble(|h| {
        for k in 0..3 {
            let y = 6.75 + k as f32 * 0.16;
            h.cube(v(0.7, 0.07, 0.05), 0.0, FRAME, place(v(0.0, y, -0.86), rx(0.2)));
        }
    });
    d.sockets.eye = Designer::local(Bone::Head, CAMERA);
    d.head_base = CHIN + 0.1;
}

// --- Chest and abdomen. ---

/// The chest's core (right half), the mass behind its blocks: (y, outer x, back z, front z) at
/// its foot and at its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.3, 1.9, -1.35, 0.95), (6.0, 2.2, -1.5, 0.7)];

/// The hatch down the chest's middle (right half), in two tiers, each given by its foot and its
/// top as (y, half width, front z at the middle): furthest forward a little under halfway up, its
/// top leaning back under the chin. Its front falls away outboard at `CHEST_FALL` per metre.
const HATCH: [[(f32, f32, f32); 2]; 2] =
    [[(3.4, 0.86, 1.42), (4.55, 1.04, 1.74)], [(4.55, 1.04, 1.74), (5.7, 0.96, 1.3)]];
const CHEST_FALL: f32 = 0.22;

/// The blocks either side of the hatch (right side): the ribs, and over them the upper blocks
/// that carry the heat vents, standing out over the ribs and leaning back to the collar. Each
/// given by its foot and its top.
const RIBS: [Cut; 2] = [(3.35, 0.9, 2.2, -1.32, 1.12), (4.3, 0.95, 2.42, -1.42, 1.36)];
const UPPER: [Cut; 2] = [(4.3, 0.98, 2.56, -1.46, 1.54), (6.22, 1.04, 2.42, -1.38, 0.98)];

/// The hatch's front at height `y` and `x` out (on the right), and its outward normal there.
fn hatch_front(x: f32, y: f32) -> (Vec3, Vec3) {
    let tier = HATCH.iter().find(|t| y <= t[1].0).unwrap_or(&HATCH[1]);
    let [(y0, _, f0), (y1, _, f1)] = *tier;
    let rise = (f1 - f0) / (y1 - y0);
    let z = f0 + rise * (y - y0) - CHEST_FALL * x;
    (v(x, y, z), v(CHEST_FALL, -rise, 1.0).normalize())
}

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
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
        // A raised panel down the hatch's side, over both tiers.
        for (y0, y1) in [(3.62, 4.55), (4.55, 5.4)] {
            let corner = |x: f32, y: f32, out: f32| {
                let (p, n) = hatch_front(x, y);
                p + n * out
            };
            let panel = [
                corner(0.34, y0, -0.08),
                corner(0.78, y0, -0.08),
                corner(0.34, y1, -0.08),
                corner(0.76, y1, -0.08),
                corner(0.34, y0, 0.1),
                corner(0.78, y0, 0.1),
                corner(0.34, y1, 0.1),
                corner(0.76, y1, 0.1),
            ];
            c.hexa(panel, 0.04, BODY, flat);
        }
        c.hexa(stacked(RIBS), 0.18, BODY, flat);
        c.hexa(stacked(UPPER), 0.22, BODY, flat);
        // The heat vent in the upper block's front: grey slats in a dark well, framed in white.
        let vent = sided(s, on_front(UPPER, 1.76, 5.18, 0.0));
        c.cube(v(1.3, 1.42, 0.16), 0.04, TRIM, vent);
        louvre(&mut c, 1.08, 1.2, 6, GREY, vent * at(0.0, 0.0, 0.04));
        c.greeble(|c| {
            // Seams across the ribs, and down the upper block's side.
            for y in [3.7, 4.0] {
                c.cube(v(1.0, 0.06, 0.05), 0.0, FRAME, sided(s, on_front(RIBS, 1.6, y, 0.0)));
            }
            c.cube(v(0.05, 1.2, 0.06), 0.0, FRAME, sided(s, place(v(2.5, 5.2, 0.2), ry(FRAC_PI_2))));
            c.cube(v(0.05, 0.06, 2.2), 0.0, FRAME, sided(s, at(2.47, 5.95, -0.2)));
        });
        // The shoulder's socket in the chest's side.
        c.cylinder(0.7, 0.5, 14, GREY, sided(s, place(v(2.5, 4.55, 0.0), rz(FRAC_PI_2))));
        // The back's side plates either side of the backpack.
        let plate = sided(s, place(v(1.5, 4.6, -1.6), ry(-0.15)));
        c.block(v(0.9, 2.0, 0.38), v2(0.82, 0.85), Vec2::ZERO, 0.14, BODY, plate);
        slot(&mut c, 0.5, plate * at(0.1, 0.25, -0.2) * Affine3A::from_rotation_y(PI));
    }
    // The hatch's seam across, under its top, and the red sensor under the chin in its housing.
    c.greeble(|c| {
        let (p, n) = hatch_front(0.0, 5.52);
        c.cube(v(1.7, 0.06, 0.05), 0.0, FRAME, place(p + n * 0.01, Quat::from_rotation_arc(Vec3::Z, n)));
        let (p, n) = hatch_front(0.0, 4.0);
        c.cube(v(0.06, 0.7, 0.05), 0.0, FRAME, place(p + n * 0.01, Quat::from_rotation_arc(Vec3::Z, n)));
    });
    c.block(v(0.8, 0.42, 0.6), v2(0.85, 0.8), Vec2::ZERO, 0.06, BODY, at(0.0, 5.78, 1.0));
    c.cube(v(0.5, 0.22, 0.12), 0.03, ACCENT, at(0.0, 5.8, 1.3));
    // The collar behind the neck: a raised rim round the back, grey ribs inside it.
    let (from, to) = (FRAC_PI_2 + 0.95, FRAC_PI_2 - 0.95 + TAU);
    c.lathe_arc(&band(0.82, 1.12, 5.9, 6.42), 22, from, to, BODY, Affine3A::IDENTITY);
    for k in 0..3 {
        let y = 5.92 + k as f32 * 0.15;
        c.lathe_arc(&band(0.55, 0.84, y, y + 0.1), 18, from, to, GREY, Affine3A::IDENTITY);
    }
    // The backpack's mount in the middle of the back.
    c.block(v(1.8, 1.9, 0.4), v2(0.85, 0.95), Vec2::ZERO, 0.1, BODY, at(0.0, 4.6, -1.62));

    // The abdomen: the grey frame, ribbed, and a plate over its front.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.82));
    a.lathe(&[(0.0, 2.45), (1.26, 2.45), (1.32, 2.6), (1.22, 3.45), (0.0, 3.45)], 14, GREY, squash);
    for k in 0..2 {
        let y = 2.72 + k as f32 * 0.3;
        a.lathe(&band(1.1, 1.36, y, y + 0.14), 16, GREY, squash);
    }
    a.block(v(1.2, 0.72, 0.4), v2(0.86, 1.0), v2(0.0, 0.04), 0.1, BODY, at(0.0, 3.0, 0.98));
    a.greeble(|a| {
        a.cube(v(0.9, 0.06, 0.05), 0.0, FRAME, at(0.0, 3.0, 1.2));
    });
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    // The belt round the hips, narrow under the chest, and the crotch block under its front with
    // the red plate over it.
    w.loft(
        &[hoop(2.0, 0.0, 0.0, 1.68, 1.26), hoop(2.1, 0.0, 0.0, 1.78, 1.36), hoop(2.62, 0.0, 0.0, 1.78, 1.36)],
        3.4,
        28,
        BODY,
        Affine3A::IDENTITY,
    );
    w.block(v(1.1, 0.46, 0.24), v2(0.9, 0.9), Vec2::ZERO, 0.06, BODY, at(0.0, 2.34, 1.36));
    w.greeble(|w| {
        w.cube(v(0.6, 0.07, 0.05), 0.0, FRAME, at(0.0, 2.34, 1.49));
        for s in Side::BOTH {
            w.cube(v(0.05, 0.4, 0.05), 0.0, FRAME, sided(s, place(v(1.3, 2.34, 1.0), ry(0.9))));
        }
    });
    w.block(v(1.15, 1.3, 1.5), v2(1.25, 1.0), v2(0.0, -0.05), 0.14, BODY, at(0.0, 1.45, 0.4));
    w.block(v(0.62, 0.86, 0.26), v2(1.4, 1.0), Vec2::ZERO, 0.06, ACCENT, place(v(0.0, 1.5, 1.2), rx(-0.1)));
    for s in Side::BOTH {
        // The front skirt hung from the belt over the thigh, turned out round it, its hem rising
        // toward the outside, a white panel down its inner half.
        let skirt = sided(s, place(v(1.16, 1.48, 1.44), ry(0.3) * rx(-0.16)));
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
        w.hexa(plate(-0.72, 0.72, 0.82, (-0.82, -0.46), -0.2, 0.2), 0.14, BODY, skirt);
        w.hexa(plate(-0.62, -0.05, 0.64, (-0.62, -0.46), 0.1, 0.3), 0.05, TRIM, skirt);
        w.greeble(|w| {
            w.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, skirt * at(0.3, 0.12, 0.21));
        });
        // The side skirt over the hip, flaring out and down from the belt, its louvre low on it.
        let side: [Cut; 2] = [(0.55, 2.26, 2.8, -1.24, 1.18), (2.5, 1.92, 2.4, -1.06, 1.02)];
        w.hexa(stacked(side), 0.18, BODY, sided(s, Affine3A::IDENTITY));
        let [(y0, _, o0, ..), (y1, _, o1, ..)] = side;
        let lean = ((o0 - o1) / (y1 - y0)).atan();
        let y = 1.15;
        let face =
            sided(s, place(v(o0 + (o1 - o0) * (y - y0) / (y1 - y0), y, 0.0), rz(-lean) * ry(FRAC_PI_2)));
        louvre(&mut w, 1.3, 0.6, 3, GREY, face);
        slot(&mut w, 0.5, face * at(-0.5, 0.75, 0.06));
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.88, 1.6, -1.4), rx(0.3)));
        w.block(v(1.5, 1.55, 0.55), v2(0.92, 0.85), v2(0.0, 0.05), 0.18, BODY, rear);
        slot(&mut w, 0.35, rear * at(0.2, 0.0, -0.29) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder armour (right), up the suit's y: a broad round box hanging out past the chest,
/// widest a third of the way up and rounding over to a flat top inboard; its hem hides the top of
/// the upper arm. Hoops as (y, centre x, centre z, half width, half depth).
const PAULDRON: [Hoop; 6] = [
    hoop(3.8, 4.12, 0.0, 1.66, 1.5),
    hoop(4.0, 4.12, 0.0, 1.74, 1.56),
    hoop(5.0, 4.04, 0.0, 1.78, 1.6),
    hoop(5.85, 3.92, 0.0, 1.62, 1.52),
    hoop(6.35, 3.8, 0.0, 1.26, 1.26),
    hoop(6.6, 3.76, 0.0, 0.92, 0.94),
];
/// The white band round its hem, and the dark hollow under it.
const HEM: [Hoop; 2] = [hoop(3.66, 4.12, 0.0, 1.7, 1.55), hoop(3.98, 4.12, 0.0, 1.8, 1.64)];
const HOLLOW: [Hoop; 2] = [hoop(3.62, 4.12, 0.0, 1.56, 1.42), hoop(3.65, 4.12, 0.0, 1.56, 1.42)];
/// The plate over its front (as for [`stacked`]), and the plate down its outside.
const SHOULDER_FRONT: [Cut; 2] = [(3.95, 2.72, 5.48, 0.9, 1.76), (5.95, 2.86, 5.2, 0.9, 1.64)];
const SHOULDER_SIDE: [Cut; 2] = [(4.05, 5.6, 5.98, -1.1, 1.1), (5.85, 5.4, 5.74, -1.0, 0.98)];
/// The missile hatch's centre on the front plate.
const HATCH_AT: (f32, f32) = (4.1, 4.95);
/// How far the shoulder armour leans out and down about the shoulder's joint (radians).
const SHOULDER_LEAN: f32 = 0.1;

/// The shoulder armour's lean (right side), about the shoulder's joint.
fn shoulder_lean() -> Affine3A {
    let p = j(Bone::ShoulderR);
    Affine3A::from_translation(p) * Affine3A::from_rotation_z(-SHOULDER_LEAN) * Affine3A::from_translation(-p)
}

/// The gauntlet, up the forearm from the wrist (x out, z forward): the white cuff's flare, then
/// a step in to the big box up to the elbow, rounding in over its barrel.
const CUFF: [Hoop; 2] = [hoop(0.0, 0.0, 0.05, 1.0, 1.04), hoop(0.42, 0.0, 0.05, 1.05, 1.09)];
const GAUNTLET: [Hoop; 5] = [
    hoop(0.4, 0.0, 0.05, 0.92, 0.96),
    hoop(1.2, 0.03, 0.04, 0.98, 1.0),
    hoop(1.72, 0.03, 0.02, 0.9, 0.9),
    hoop(1.98, 0.0, 0.0, 0.66, 0.64),
    hoop(2.04, 0.0, 0.0, 0.4, 0.4),
];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The shoulder armour: the round box, its white hem and the dark hollow under it, the
        // grey socket it turns on.
        let lean = |xf: Affine3A| sided(s, shoulder_lean() * xf);
        let flat_lean = lean(Affine3A::IDENTITY);
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.loft(&PAULDRON, 2.8, 32, BODY, flat_lean);
        o.loft(&HEM, 2.8, 32, TRIM, flat_lean);
        o.loft(&HOLLOW, 2.8, 24, FRAME, flat_lean);
        o.cylinder(0.5, 0.8, 12, GREY, sided(s, place(v(2.7, sh.y + 0.25, 0.0), rz(FRAC_PI_2))));
        // The heat-vent fins on its flat top, in a raised housing.
        let top = lean(place(v(3.98, 6.62, -0.08), rz(-0.08)));
        o.block(v(1.56, 0.26, 1.8), v2(0.92, 0.94), Vec2::ZERO, 0.08, BODY, top);
        fins(&mut o, 1.32, 1.52, 0.36, 6, top * at(0.0, 0.11, 0.0));
        // The plate over its front, and on it the missile hatch: a raised door with three launch
        // tubes along its top, a white band under them.
        o.hexa(stacked(SHOULDER_FRONT), 0.16, BODY, flat_lean);
        let (hx, hy) = HATCH_AT;
        let hatch = lean(on_front(SHOULDER_FRONT, hx, hy, 0.0));
        o.block(v(1.7, 1.1, 0.16), v2(0.94, 0.92), Vec2::ZERO, 0.06, BODY, hatch);
        o.cube(v(1.5, 0.14, 0.06), 0.01, TRIM, hatch * at(0.0, -0.1, 0.09));
        for x in [-0.5, 0.0, 0.5] {
            let tube = hatch * place(v(x, 0.22, 0.06), rx(FRAC_PI_2));
            o.lathe(&band(0.15, 0.22, 0.0, 0.08), 14, GREY, tube);
            o.cylinder(0.16, 0.06, 12, FRAME, tube * at(0.0, 0.05, 0.0));
            o.greeble(|o| {
                o.lathe(&[(0.0, 0.0), (0.11, 0.0), (0.09, 0.07), (0.0, 0.11)], 10, GREY, tube);
            });
        }
        o.greeble(|o| {
            for x in [-0.25, 0.25] {
                o.cube(v(0.05, 0.36, 0.04), 0.0, FRAME, hatch * at(x, -0.36, 0.09));
            }
            o.cube(v(1.5, 0.05, 0.04), 0.0, FRAME, hatch * at(0.0, -0.2, 0.09));
        });
        let launch = hatch.transform_point3(v(0.0, 0.22, 0.16));
        d.sockets.missiles.push((shoulder, Designer::local(shoulder, launch)));
        let mut o = d.on(shoulder);
        o.greeble(|o| {
            let face = lean(on_front(SHOULDER_FRONT, 4.1, 5.62, 0.0));
            o.cube(v(2.0, 0.06, 0.05), 0.0, FRAME, face);
            o.cube(v(0.06, 0.5, 0.05), 0.0, FRAME, lean(on_front(SHOULDER_FRONT, 3.2, 4.4, 0.0)));
        });
        // A plate over its back, slotted.
        let rear: [Cut; 2] = [(4.1, 3.0, 5.2, -1.72, -1.0), (5.6, 3.05, 5.0, -1.62, -1.0)];
        o.hexa(stacked(rear), 0.14, BODY, flat_lean);
        slot(&mut o, 0.6, lean(place(v(4.5, 4.85, -1.72), ry(PI))));
        slot(&mut o, 0.6, lean(place(v(3.6, 4.85, -1.72), ry(PI))));
        // The plate down its outside, a round port in it, and the running light at its outer
        // front corner, high: red on the suit's own left (+x), green on its right.
        o.hexa(stacked(SHOULDER_SIDE), 0.12, BODY, flat_lean);
        port(&mut o, 0.32, lean(place(v(5.94, 4.85, 0.35), ry(FRAC_PI_2))));
        slot(&mut o, 0.7, lean(place(v(5.92, 5.0, -0.5), ry(FRAC_PI_2))));
        let nav = lean(at(4.95, 6.24, 1.12));
        o.cube(v(0.34, 0.18, 0.22), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.09) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));

        // The upper arm: a drum with a raised ring round it, on the grey frame, out from under
        // the shoulder.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.46, 1.8, 12, GREY, up_xf * at(0.0, 1.0, 0.0));
        u.lathe(
            &[
                (0.0, 0.45),
                (0.72, 0.45),
                (0.82, 0.56),
                (0.82, 0.84),
                (0.9, 0.9),
                (0.9, 1.08),
                (0.82, 1.14),
                (0.82, 1.42),
                (0.72, 1.5),
                (0.0, 1.5),
            ],
            20,
            BODY,
            up_xf,
        );

        // The gauntlet: the elbow's barrel, the white cuff, the big box with a plate down its
        // outside and a guard over the elbow.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.58, 1.2, 14, GREY, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&CUFF, 3.4, 28, TRIM, fo_xf);
        f.loft(&GAUNTLET, 3.4, 28, BODY, fo_xf);
        let plate: [Cut; 2] = [(0.62, 0.7, 1.08, -0.72, 0.72), (1.62, 0.72, 1.06, -0.66, 0.66)];
        f.hexa(stacked(plate), 0.1, BODY, fo_xf);
        f.greeble(|f| {
            f.cube(v(0.05, 0.08, 1.2), 0.0, FRAME, fo_xf * at(1.07, 1.12, 0.0));
            f.cube(v(0.36, 0.08, 0.05), 0.0, FRAME, fo_xf * at(0.0, 1.0, 0.99));
            f.cube(v(0.06, 0.62, 0.05), 0.0, FRAME, fo_xf * at(0.4, 1.2, 0.98));
        });
        let guard = sided(s, place(el + v(0.15, 0.1, -0.5), rx(-0.5)));
        f.block(v(0.95, 0.9, 0.5), v2(0.7, 0.6), v2(0.0, -0.06), 0.12, BODY, guard);
        f.cylinder(0.46, 0.3, 12, GREY, fo_xf);
    }
    crate::frames::hands(d, GREY);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): squared, from over
/// the knee guard to under the belt, swelling out and forward, closing in under the hip.
const THIGH: [Hoop; 6] = [
    hoop(0.4, 0.15, 0.05, 0.96, 0.96),
    hoop(0.95, 0.22, 0.08, 1.24, 1.18),
    hoop(2.5, 0.3, 0.06, 1.32, 1.22),
    hoop(3.3, 0.26, 0.0, 1.18, 1.08),
    hoop(3.72, 0.2, 0.0, 0.86, 0.76),
    hoop(3.86, 0.18, 0.0, 0.5, 0.42),
];

/// The thigh's plates, along its axis (x out, z forward), each by its foot and its top: the
/// front plate, wide and thick, and the plate down its outside.
const THIGH_FRONT: [Cut; 2] = [(0.7, -0.9, 1.28, 0.6, 1.44), (3.25, -0.96, 1.36, 0.7, 1.38)];
const THIGH_SIDE: [Cut; 2] = [(0.6, 1.2, 1.76, -1.02, 1.1), (3.35, 1.24, 1.68, -0.98, 1.0)];

/// The lower leg's core, up its axis from the ankle to the knee (x out, z forward): a lip round
/// its hem over the boot, slimmer over the ankle, swelling back to the full calf under the knee
/// and closing in behind the knee guard.
const SHIN: [Hoop; 8] = [
    hoop(-0.12, 0.05, 0.04, 1.2, 1.3),
    hoop(0.04, 0.05, 0.04, 1.28, 1.38),
    hoop(0.2, 0.05, 0.02, 1.16, 1.26),
    hoop(1.4, 0.05, -0.06, 1.08, 1.2),
    hoop(3.3, 0.08, -0.24, 1.12, 1.34),
    hoop(4.4, 0.1, -0.28, 1.04, 1.16),
    hoop(5.1, 0.08, -0.22, 0.86, 0.86),
    hoop(5.42, 0.06, -0.2, 0.5, 0.5),
];

/// The plates down the outside and the inside of the lower leg, by foot and top: flaring out to
/// the hem, set in from the core's front and back so the calf shows round behind them.
const SHIN_OUT: [Cut; 2] = [(-0.14, 1.06, 1.6, -1.04, 1.08), (2.7, 0.96, 1.3, -0.9, 0.86)];
const SHIN_IN: [Cut; 2] = [(-0.12, -1.3, -1.02, -0.95, 1.0), (2.2, -1.22, -0.96, -0.9, 0.86)];

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);

        // The thigh: the squared core on a grey ball at the hip, the thick front plate and the
        // plate down its outside, a step between them.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.82, 6, GREY, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 4.0, 28, BODY, along_thigh);
        th.hexa(stacked(THIGH_FRONT), 0.18, BODY, along_thigh);
        th.hexa(stacked(THIGH_SIDE), 0.16, BODY, along_thigh);
        // The plate over its back, its face narrower than its foot.
        let back = [
            (-0.86, 0.9, -0.6),
            (1.16, 0.9, -0.6),
            (-0.9, 3.3, -0.55),
            (1.2, 3.3, -0.55),
            (-0.64, 0.9, -1.36),
            (0.94, 0.9, -1.36),
            (-0.7, 3.3, -1.3),
            (1.0, 3.3, -1.3),
        ]
        .map(|(x, y, z)| v(x, y, z));
        th.hexa(back, 0.14, BODY, along_thigh);
        th.greeble(|th| {
            // Seams across the front plate and down the side plate, and a slot in the core's back.
            th.cube(v(1.7, 0.06, 0.05), 0.0, FRAME, along_thigh * place(v(0.2, 2.3, 1.43), rx(0.02)));
            th.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, along_thigh * at(0.55, 1.4, 1.44));
            th.cube(v(0.05, 1.6, 0.06), 0.0, FRAME, along_thigh * at(1.75, 2.0, 0.3));
            th.cube(v(0.6, 0.06, 0.05), 0.0, FRAME, along_thigh * at(0.3, 2.2, -1.2));
        });

        // The lower leg: the knee's grey barrel, the core with its full calf, the plates down
        // either side flaring at the hem, the faceted shin plate down the front and the white
        // knee guard over it all.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.64, 1.35, 12, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN, 3.0, 28, BODY, tube);
        m.hexa(stacked(SHIN_OUT), 0.16, BODY, tube);
        m.hexa(stacked(SHIN_IN), 0.14, BODY, tube);
        // The shin plates: the lower standing out over the boot, the upper stepping back from it
        // up to the knee guard, each with its face narrower than its foot, so its sides fall away
        // in facets.
        let shin_plate = |y0: f32, y1: f32, w0: (f32, f32), w1: (f32, f32), f0: f32, f1: f32| {
            [
                v(-w0.0, y0, 0.55),
                v(w0.0 + 0.1, y0, 0.55),
                v(-w1.0, y1, 0.5),
                v(w1.0 + 0.1, y1, 0.5),
                v(-w0.1, y0, f0),
                v(w0.1 + 0.1, y0, f0),
                v(-w1.1, y1, f1),
                v(w1.1 + 0.1, y1, f1),
            ]
        };
        m.hexa(shin_plate(-0.12, 2.3, (1.02, 0.42), (0.94, 0.38), 1.66, 1.46), 0.14, BODY, tube);
        m.hexa(shin_plate(2.3, 4.2, (0.9, 0.36), (0.82, 0.32), 1.36, 1.2), 0.12, BODY, tube);
        // The plate down the back of the calf, faceted like the shin's, a vent in it.
        let calf = [
            (-0.92, 1.0, -0.5),
            (1.02, 1.0, -0.5),
            (-0.86, 4.5, -0.6),
            (0.96, 4.5, -0.6),
            (-0.5, 1.0, -1.42),
            (0.6, 1.0, -1.42),
            (-0.46, 4.5, -1.6),
            (0.56, 4.5, -1.6),
        ]
        .map(|(x, y, z)| v(x, y, z));
        m.hexa(calf, 0.14, BODY, tube);
        let slope = ((1.6f32 - 1.42) / 3.5).atan();
        louvre(&mut m, 0.7, 0.9, 3, GREY, tube * place(v(0.05, 2.0, -1.47), ry(PI) * rx(slope)));
        // The heat vent low down the outside plate.
        let [(y0, _, o0, ..), (y1, _, o1, ..)] = SHIN_OUT;
        let out_at = |y: f32| o0 + (o1 - o0) * (y - y0) / (y1 - y0);
        let lean = ((o0 - o1) / (y1 - y0)).atan();
        let vent = tube * place(v(out_at(0.85), 0.85, 0.04), rz(lean) * ry(FRAC_PI_2));
        louvre(&mut m, 1.3, 1.1, 4, GREY, vent);
        m.greeble(|m| {
            m.cube(v(0.05, 0.9, 0.06), 0.0, FRAME, tube * place(v(out_at(1.9), 1.9, -0.62), rz(lean)));
            m.cube(v(0.06, 1.2, 0.05), 0.0, FRAME, tube * place(v(0.1, 1.0, 1.57), rx(-0.083)));
            m.cube(v(0.06, 0.8, 0.05), 0.0, FRAME, tube * place(v(0.1, 3.1, 1.29), rx(-0.084)));
        });
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        m.hexa(
            [
                (-0.68, -1.1, 0.45),
                (0.68, -1.1, 0.45),
                (-0.6, 1.15, 0.25),
                (0.6, 1.15, 0.25),
                (-0.6, -1.2, 1.5),
                (0.6, -1.2, 1.5),
                (-0.54, 1.15, 1.08),
                (0.54, 1.15, 1.08),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.16,
            TRIM,
            guard,
        );
        m.hexa(
            [
                (-0.54, -1.7, 0.62),
                (0.54, -1.7, 0.62),
                (-0.62, -1.05, 0.5),
                (0.62, -1.05, 0.5),
                (-0.16, -1.7, 1.36),
                (0.16, -1.7, 1.36),
                (-0.26, -1.05, 1.2),
                (0.26, -1.05, 1.2),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.08,
            TRIM,
            guard,
        );
        m.block(
            v(0.38, 0.85, 0.12),
            v2(0.85, 1.0),
            Vec2::ZERO,
            0.03,
            FRAME,
            guard * place(v(0.0, 0.15, 1.28), rx(-0.18)),
        );

        // The boot's shaft round the ankle, under the hem: a rounded box, the tongue rising up
        // its front from over the instep, and a big grey hub either side where it meets the foot.
        let x = ankle.x;
        let flat = sided(s, Affine3A::IDENTITY);
        m.loft(
            &[
                hoop(-8.32, x, -0.1, 1.12, 1.02),
                hoop(-7.9, x, -0.12, 1.1, 1.0),
                hoop(-7.55, x, -0.12, 1.0, 0.92),
            ],
            3.6,
            24,
            BODY,
            flat,
        );
        let tongue = crate::frames::boot(x, -8.62, (-7.7, -7.7), (0.98, 0.84), [0.5, 0.5, 1.92, 1.36], 0.06);
        m.hexa(tongue, 0.16, BODY, flat);
        m.greeble(|m| {
            let slope = v(0.0, 0.92, 1.36 - 1.92).normalize();
            let face = sided(s, place(v(x, -8.05, 1.575), Quat::from_rotation_arc(Vec3::Y, slope)));
            m.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, face);
            m.cube(v(0.06, 0.32, 0.05), 0.0, FRAME, face * at(0.3, 0.2, 0.0));
        });
        for (side, r) in [(1.0, 0.56), (-1.0, 0.46)] {
            let hub = sided(s, place(v(x + side * 1.14, ankle.y - 0.36, ankle.z - 0.15), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                16,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }

        // The foot: a grey ball at the ankle, a thick platform with its heel standing tall behind
        // the shaft, a grey band round the heel over the dark sole and its vernier.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.52, 5, GREY, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        f.hexa(boot(x, base, (-7.62, -7.86), (1.36, 1.32), [-2.05, -1.8, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.46, 1.46), [-2.17, -2.14, -0.6, -0.6]),
            0.05,
            GREY,
            flat,
        );
        f.greeble(|f| {
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 2.9), 0.0, FRAME, sided(s, at(x + side * 1.26, -8.42, -0.3)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.24, -8.3, 1.26)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.1, -1.92), rx(-0.22))));
        });
        let heel =
            Sole { z: (-2.15, 1.26), half: (1.44, 1.38), top: 0.34, pads: GREY, vernier: -0.4, r: 0.42 };
        sole(d, foot, s, x, heel);

        // The toe: as wide, falling away and narrowing to a blunt tip, with the red block raised
        // along it.
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.95, -8.35), (1.32, 1.08), [1.32, 1.32, 3.32, 2.92]), 0.34, BODY, flat);
        t.hexa(boot(x, -8.5, (-7.87, -8.2), (0.56, 0.46), [1.42, 1.42, 2.85, 2.6]), 0.08, ACCENT, flat);
        t.greeble(|t| {
            t.cube(v(1.9, 0.05, 0.05), 0.0, FRAME, sided(s, at(x, -8.3, 1.42)));
        });
        let toes = Sole { z: (1.34, 3.36), half: (1.38, 1.1), top: 0.34, pads: GREY, vernier: 2.3, r: 0.34 };
        sole(d, toe, s, x, toes);
    }
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

/// A block of a foot ([`frames::boot`](crate::frames::boot)), its sides leaning in [`BOOT_LEAN`].
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    crate::frames::boot(x, base, top, half, z, BOOT_LEAN)
}

// --- Backpack. ---

/// The pack, up the suit's y: a squared box behind the chest.
const PACK: [Hoop; 4] = [
    hoop(2.95, 0.0, -2.4, 1.3, 0.66),
    hoop(3.2, 0.0, -2.46, 1.5, 0.82),
    hoop(5.25, 0.0, -2.5, 1.5, 0.82),
    hoop(5.55, 0.0, -2.46, 1.26, 0.64),
];

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.5);
    b.loft(&PACK, 4.0, 28, BODY, Affine3A::IDENTITY);
    // A raised panel down the middle of its back, a round hatch in it, a heat vent either side
    // of it, and a grab handle over it.
    let back = |x: f32, y: f32| place(v(x, y, -3.32), ry(PI));
    b.block(v(1.3, 2.1, 0.3), v2(0.9, 0.92), Vec2::ZERO, 0.1, BODY, back(0.0, 4.3) * at(0.0, 0.0, 0.1));
    let hatch = back(0.0, 4.55) * place(v(0.0, 0.0, 0.25), rx(FRAC_PI_2));
    b.lathe(&band(0.3, 0.46, -0.04, 0.1), 18, BODY, hatch);
    b.cylinder(0.31, 0.08, 14, FRAME, hatch);
    for s in Side::BOTH {
        louvre(&mut b, 0.62, 1.7, 6, GREY, back(s.sign() * 1.04, 4.25));
    }
    b.greeble(|b| {
        b.cube(v(0.9, 0.06, 0.05), 0.0, FRAME, back(0.0, 3.6) * at(0.0, 0.0, 0.26));
        b.cylinder(0.06, 0.8, 8, GREY, back(0.0, 5.32) * place(v(0.0, 0.0, 0.22), rz(FRAC_PI_2)));
        for x in [-0.4, 0.4] {
            b.cube(v(0.1, 0.1, 0.2), 0.0, GREY, back(x, 5.32) * at(0.0, 0.0, 0.12));
        }
    });
    // The heat-vent fins on its top.
    fins(&mut b, 2.2, 1.1, 0.32, 8, at(0.0, 5.55, -2.5));
    for s in Side::BOTH {
        // A round tank either side, domed at both ends, two grey bands round it, a small bell
        // under it.
        let tank = sided(s, at(1.72, 0.0, -2.5));
        b.lathe(
            &[
                (0.0, 2.75),
                (0.32, 2.8),
                (0.5, 2.98),
                (0.58, 3.25),
                (0.58, 5.05),
                (0.5, 5.32),
                (0.32, 5.5),
                (0.0, 5.55),
            ],
            16,
            BODY,
            tank,
        );
        for y in [3.45, 4.85] {
            b.lathe(&band(0.5, 0.62, y, y + 0.16), 16, GREY, tank);
        }
        b.lathe(&[(0.14, 2.86), (0.18, 2.74), (0.3, 2.5), (0.26, 2.48), (0.12, 2.7)], 12, STEEL, tank);
        // The thrusters' housing, a block over each bell.
        let housing = sided(s, place(v(0.72, 3.12, -2.78), rx(-0.5)));
        b.block(v(1.05, 0.55, 1.05), v2(0.9, 0.9), Vec2::ZERO, 0.08, GUN, housing);
        b.lathe(&band(0.5, 0.6, -0.36, -0.26), 16, GREY, housing);
    }
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.72, 2.88, -2.78)), v(0.0, -0.5, -0.86), 0.56);
    }
}

// --- The beam machine gun. ---

/// The barrel's axis, over the grip.
const BORE_Y: f32 = 0.62;

/// The beam machine gun, in the right hand: a deep receiver, the energy drum on its inner side, a
/// finned jacket round the barrel, the muzzle brake and its bore, a sight with its lens, a stock;
/// and the clamp under its front that the right blade's grip locks into.
fn machine_gun(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let y = BORE_Y;
    let along_z = |y: f32, z: f32| place(v(g.x, g.y + y, g.z + z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    w.cube(v(0.34, 1.0, 0.46), 0.05, GUN, place(v(g.x, g.y - 0.15, g.z), rx(-0.25)));
    // The receiver, its spine and its red stripe.
    w.block(v(0.8, 1.05, 2.8), v2(0.92, 0.95), Vec2::ZERO, 0.14, GUN, at(g.x, g.y + y, g.z + 0.5));
    w.cube(v(0.36, 0.16, 2.2), 0.04, GUN, at(g.x, g.y + y + 0.58, g.z + 0.6));
    w.cube(v(0.04, 0.14, 1.4), 0.0, ACCENT, at(g.x + 0.41, g.y + y + 0.22, g.z + 0.7));
    w.greeble(|w| {
        for k in 0..5 {
            let z = g.z + 0.9 + k as f32 * 0.22;
            w.cube(v(0.84, 0.36, 0.08), 0.0, FRAME, at(g.x, g.y + y - 0.12, z));
        }
    });
    // The sight on the spine, its lenses fore and aft.
    w.cube(v(0.32, 0.36, 0.8), 0.04, GUN, at(g.x, g.y + y + 0.84, g.z - 0.1));
    for (z, r) in [(0.31, 0.12), (-0.51, 0.1)] {
        w.cylinder(r, 0.06, 12, LENS, along_z(y + 0.86, z - 0.1));
    }
    // The energy drum on the receiver's inner side, a white ring round its face.
    let drum = place(v(g.x - 0.62, g.y + y - 0.12, g.z + 0.75), rz(FRAC_PI_2));
    w.lathe(
        &[(0.0, -0.24), (0.6, -0.24), (0.68, -0.16), (0.68, 0.16), (0.6, 0.24), (0.0, 0.24)],
        20,
        GUN,
        drum,
    );
    w.lathe(&band(0.36, 0.5, 0.2, 0.27), 18, TRIM, drum);
    w.cylinder(0.2, 0.56, 10, GREY, drum);
    // The barrel's jacket: a sleeve with cooling fins round it.
    let mut jacket = vec![(0.0, 0.0), (0.36, 0.0)];
    for k in 0..4 {
        let h = 0.3 + k as f32 * 0.55;
        jacket.extend([(0.36, h), (0.46, h + 0.04), (0.46, h + 0.16), (0.36, h + 0.2)]);
    }
    jacket.extend([(0.36, 2.6), (0.0, 2.6)]);
    w.lathe(&jacket, 16, GUN, along_z(y, 1.9));
    w.cylinder(0.2, 0.8, 12, STEEL, along_z(y, 4.85));
    // The muzzle brake: octagonal, ported either side, and its bore.
    w.cylinder(0.36, 0.72, 8, GUN, along_z(y, 5.42) * Affine3A::from_rotation_y(PI / 8.0));
    w.greeble(|w| {
        for x in [-0.34, 0.34] {
            for z in [5.3, 5.55] {
                w.cube(v(0.06, 0.24, 0.12), 0.0, FRAME, at(g.x + x, g.y + y, g.z + z));
            }
        }
    });
    w.cylinder(0.2, 0.1, 12, FRAME, along_z(y, 5.75));
    // The stock, its grey butt.
    w.block(v(0.6, 0.9, 1.5), v2(0.9, 0.8), v2(0.0, -0.08), 0.1, GUN, at(g.x, g.y + y - 0.1, g.z - 1.6));
    w.cube(v(0.64, 0.95, 0.14), 0.03, GREY, at(g.x, g.y + y - 0.1, g.z - 2.4));
    // The clamp under the receiver's front for the right blade's grip.
    w.block(v(0.44, 0.48, 0.8), v2(0.9, 0.9), Vec2::ZERO, 0.06, GUN, at(g.x, g.y + 0.02, g.z + 1.3));
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + y, g.z + 5.8));
}

// --- The heat blades. ---

/// How long a blade is past its guard, and how far its spine bows out at its middle; how wide it
/// is at its guard.
const BLADE_LEN: f32 = 3.5;
const BLADE_BOW: f32 = 0.95;
const BLADE_INNER: f32 = 0.5;
const BLADE_ROOT: f32 = 0.13;

fn blades(d: &mut Designer) {
    // The left in the left fist, forward and up, bowing out to the left.
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.5, w.z + 0.3);
    let dir = v(-0.05, 0.6, 0.8).normalize();
    shotel(d, Bone::HandL, hilt, dir, Vec3::NEG_X);
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, hilt), dir));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt), dir);
    // The right clamped under the gun, forward and down, bowing down and out under the barrel.
    let g = j(Bone::Weapon);
    let hilt = v(g.x, g.y - 0.1, g.z + 1.75);
    let dir = v(0.06, -0.32, 0.95).normalize();
    shotel(d, Bone::Weapon, hilt, dir, v(0.5, -0.85, 0.0));
    d.sockets.blade_right = Some((Designer::local(Bone::Weapon, hilt), dir));
}

/// A heat blade on `bone`: its wrapped grip back from `hilt` along -`dir` to a pommel, a guard,
/// and the crescent out along `dir`, bowing toward `bow` and coming back to its point, its inner
/// edge glowing.
fn shotel(d: &mut Designer, bone: Bone, hilt: Vec3, dir: Vec3, bow: Vec3) {
    let xf = frame_of(hilt, bow, dir);
    let mut o = d.on(bone);
    o.seed(0.66);
    o.cylinder(0.16, 1.1, 10, GUN, xf * at(0.0, -0.55, 0.0));
    o.lathe(&[(0.0, -1.32), (0.18, -1.3), (0.22, -1.18), (0.2, -1.08), (0.0, -1.08)], 10, GUN, xf);
    o.greeble(|o| {
        for k in 0..4 {
            let y = -0.95 + k as f32 * 0.22;
            o.lathe(&band(0.15, 0.19, y, y + 0.1), 10, GREY, xf);
        }
    });
    o.block(v(0.62, 0.22, 0.4), v2(0.8, 0.8), Vec2::ZERO, 0.05, GUN, xf * at(0.12, 0.0, 0.0));
    // The crescent: its outer edge and its inner, as far out at `u` along it.
    let outer = |u: f32| BLADE_BOW * (PI * u).sin() + BLADE_ROOT * (1.0 - u);
    let inner = |u: f32| BLADE_INNER * (PI * u).sin() - BLADE_ROOT * (1.0 - u);
    let n = 12;
    let at_u = |k: u32| k as f32 / n as f32;
    let mut shape: Vec<Vec2> = (0..=n).map(|k| v2(outer(at_u(k)), BLADE_LEN * at_u(k) + 0.1)).collect();
    shape.extend((1..n).rev().map(|k| v2(inner(at_u(k)), BLADE_LEN * at_u(k) + 0.1)));
    shape.push(v2(inner(0.0), 0.1));
    o.extrude(&shape, 0.13, STEEL, xf);
    // The glowing edge along the inside of the curve, standing just proud of the blade's faces.
    let (u0, u1) = (0.06, 0.97);
    let along = |k: u32| u0 + (u1 - u0) * k as f32 / n as f32;
    let mut edge: Vec<Vec2> =
        (0..=n).map(|k| v2(inner(along(k)) - 0.035, BLADE_LEN * along(k) + 0.1)).collect();
    edge.extend((0..=n).rev().map(|k| v2(inner(along(k)) + 0.1, BLADE_LEN * along(k) + 0.1)));
    o.extrude(&edge, 0.17, HOT, xf);
    // The fuller, a dark groove down the blade's middle.
    o.greeble(|o| {
        let mid = |u: f32| (outer(u) + inner(u)) * 0.5 + 0.05;
        let m = 8;
        let along = |k: u32| 0.14 + 0.62 * k as f32 / m as f32;
        let mut groove: Vec<Vec2> = (0..=m).map(|k| v2(mid(along(k)) + 0.04, BLADE_LEN * along(k))).collect();
        groove.extend((0..=m).rev().map(|k| v2(mid(along(k)) - 0.04, BLADE_LEN * along(k))));
        o.extrude(&groove, 0.15, FRAME, xf);
    });
}

// --- The cross shield. ---

/// How much larger the cross shield is drawn than its outline below, across its face.
const SHIELD_SCALE: f32 = 1.15;

/// The cross shield on the left forearm, facing out: a long white bar down the forearm, pointed
/// at both ends, crossed near its top by a shorter bar whose arms are where the blades lock in;
/// a tan spine down its middle, a red boss where the arms cross, and the grey mount behind.
fn cross_shield(d: &mut Designer) {
    let (el, wr) = (sp(Side::L, j(Bone::ForearmR)), sp(Side::L, j(Bone::HandR)));
    let out = v(-1.0, 0.0, 0.1).normalize();
    let run = el - wr;
    let up = (run - out * run.dot(out)).normalize();
    let centre = (el + wr) * 0.5 + out * 1.3 + up * 0.2;
    // Local x across the shield, y up the forearm toward the elbow, z out of its face; its outline
    // below is drawn larger by `SHIELD_SCALE` across its face.
    let face = frame_of(centre, up.cross(out), up) * Affine3A::from_scale(v(SHIELD_SCALE, SHIELD_SCALE, 1.0));
    let mut s = d.on(Bone::Shield);
    s.seed(0.71);
    s.block(v(0.9, 1.6, 0.62), v2(0.9, 0.9), Vec2::ZERO, 0.06, GREY, face * at(0.0, 0.2, -0.28));
    let corners = |c: [(f32, f32, f32); 8]| c.map(|(x, y, z)| v(x, y, z));
    // The long bar: its middle, and its two pointed ends.
    s.hexa(
        corners([
            (-0.62, -1.7, 0.0),
            (0.62, -1.7, 0.0),
            (-0.62, 1.4, 0.0),
            (0.62, 1.4, 0.0),
            (-0.5, -1.7, 0.3),
            (0.5, -1.7, 0.3),
            (-0.5, 1.4, 0.3),
            (0.5, 1.4, 0.3),
        ]),
        0.08,
        TRIM,
        face,
    );
    s.hexa(
        corners([
            (-0.14, -2.95, 0.0),
            (0.14, -2.95, 0.0),
            (-0.62, -1.7, 0.0),
            (0.62, -1.7, 0.0),
            (-0.02, -2.95, 0.3),
            (0.02, -2.95, 0.3),
            (-0.5, -1.7, 0.3),
            (0.5, -1.7, 0.3),
        ]),
        0.08,
        TRIM,
        face,
    );
    s.hexa(
        corners([
            (-0.62, 1.4, 0.0),
            (0.62, 1.4, 0.0),
            (-0.3, 2.3, 0.0),
            (0.3, 2.3, 0.0),
            (-0.5, 1.4, 0.3),
            (0.5, 1.4, 0.3),
            (-0.18, 2.3, 0.3),
            (0.18, 2.3, 0.3),
        ]),
        0.08,
        TRIM,
        face,
    );
    // The cross's arms, narrowing to their ends, a dark slot along each where a blade locks in.
    for side in Side::BOTH {
        let arm = corners([
            (0.5, 0.2, 0.0),
            (1.85, 0.43, 0.0),
            (0.5, 1.3, 0.0),
            (1.85, 1.07, 0.0),
            (0.5, 0.32, 0.3),
            (1.73, 0.55, 0.3),
            (0.5, 1.18, 0.3),
            (1.73, 0.95, 0.3),
        ]);
        let arm = if side == Side::R { arm } else { swap_x(arm.map(|p| v(-p.x, p.y, p.z))) };
        s.hexa(arm, 0.08, TRIM, face);
        s.cube(v(0.9, 0.16, 0.08), 0.02, FRAME, face * at(side.sign() * 1.2, 0.75, 0.3));
    }
    // The spine, and the red boss.
    s.hexa(
        corners([
            (-0.24, -2.35, 0.2),
            (0.24, -2.35, 0.2),
            (-0.24, 1.95, 0.2),
            (0.24, 1.95, 0.2),
            (-0.16, -2.35, 0.44),
            (0.16, -2.35, 0.44),
            (-0.16, 1.95, 0.44),
            (0.16, 1.95, 0.44),
        ]),
        0.05,
        BODY,
        face,
    );
    let boss = face * place(v(0.0, 0.75, 0.3), rx(FRAC_PI_2));
    s.lathe(&[(0.0, 0.0), (0.5, 0.0), (0.5, 0.14), (0.4, 0.26), (0.0, 0.28)], 20, ACCENT, boss);
    s.lathe(&band(0.16, 0.24, 0.26, 0.32), 14, GREY, boss);
    s.greeble(|s| {
        for y in [-1.2, 1.7] {
            s.cube(v(0.9, 0.05, 0.05), 0.0, FRAME, face * at(0.0, y, 0.31));
        }
    });
}
