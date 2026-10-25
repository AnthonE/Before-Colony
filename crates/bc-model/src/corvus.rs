//! AL-11D Corvus, the Almagest's mass-produced flight suit, flown as a drone: the Argo's stablemate
//! from the same yards, slimmer and sharper and built to fly. Its bulk is in its high, sharply
//! angled shoulders and its long, heavy legs, under a big flight pack:
//!
//! - a small head set low in its collar between the shoulders: a jaw narrowing to a snout under the
//!   dark band that runs round it, the amber visor across the band's front with the pink mono-eye
//!   behind it, as on the Argo; the helmet over it, its brow a peak over the visor, and a blue fin
//!   on a white root along its top, coming forward to a point out over the eye; a round sensor
//!   either side of the band, and swept cheeks with their vents;
//! - a blue chest in two faceted tiers, the lower swelling to a prow with an intake either side
//!   (grey slats in a dark recess), the upper leaning back to the collar under two raised white
//!   plates; white flanks under the shoulders, a white collar round the back and sides of the neck,
//!   the pack's mount and two plates on the back, and white ribbed plates over the brown abdomen;
//! - a blue belt, the crotch block with its red mark, front skirts with a blue panel down their
//!   outer halves, and side and rear skirts;
//! - shoulders of white armour over brown balls, each one's top rising steeply outboard to its high
//!   outer corner and narrowing toward it, a blue plate along the top and a blue flare along the
//!   foot of its outer half; plates standing on its front and back, and another on its outer face
//!   (the unit number on the right, the insignia on the left); a round port in its front and a
//!   vernier in its back;
//! - white upper arms with a blue ring round them, heavy white gauntlets on blue cuffs with an elbow
//!   guard standing out behind, and brown fists;
//! - legs as heavy as the pair's, but plated: thick squared thighs with a plate down the front, the
//!   outside and the back; a knee guard of two plates meeting in a ridge that comes to a point
//!   forward, a blue strip down it; lower legs swelling to full calves, a blue plate down the front
//!   stepping out under the guard and white plates down the sides, two vents in the back of each
//!   calf; and big boots: a white tongue up the front, a brown band behind and a brown hub either
//!   side, a tall heel block, and a blue toe on its own hinge narrowing to a blunt point with a white
//!   ridge along it, a vernier in the sole under the heel and under the toe;
//! - the flight pack: a white body on a blue mount, a jet engine up either side with its intake at
//!   the top and a main thruster under it, the long blue nose cone standing up behind the head with
//!   its red tip, and a blue hatch, a louvre, a grab handle and the red beacon on its back; and the
//!   swept wings (on [`Bone::WingL`] and [`Bone::WingR`], so they flare on boost), canted down at
//!   the back so they show behind the shoulders from the front, a blue panel along each, red tips
//!   carrying the running lights, and a hardpoint under each;
//! - the long beam rifle: a gunmetal receiver with a blue band round it, the energy pack before the
//!   grip, a ribbed shroud, the barrel and an octagonal muzzle with its bore, and the sight with its
//!   lens; the saber's hilt in the left hand.
//!
//! The white, the blue and the red marks are the livery's; the inner frame, the shoulder balls and
//! the hands are the Argo's brown, the weapons gunmetal.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Mat3, Vec2, Vec3};

use crate::detail::{GREY, LENS, band, port, slot, stacked, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, TRIM, at, hands, j, nozzle, place, rx, ry, rz,
    saber_hilt, sided, sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The inner frame, as on the Argo: neck, joints, shoulder balls and hands.
const BROWN: Paint = Paint::Fixed(paint::FRAME_BROWN);
/// The visor's window over the mono-eye.
const VISOR: Paint = Paint::Fixed(paint::VISOR_AMBER);

pub(crate) fn corvus(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    legs(d);
    flight_pack(d);
    wings(d);
    beam_rifle(d);
    saber_hilt(d);
    stencils(d);
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
        14,
        GUN,
        b,
    );
}

/// A louvre on the face at `xf`'s origin (looking along its +z): a dark recess `w` wide and `h`
/// tall, and `n` grey slats across it.
fn louvre(o: &mut On<'_>, w: f32, h: f32, n: u32, xf: Affine3A) {
    o.cube(v(w, h, 0.12), 0.02, FRAME, xf);
    let pitch = h / n as f32;
    for k in 0..n {
        let y = -h * 0.5 + pitch * (k as f32 + 0.5);
        o.cube(
            v(w - 0.06, pitch * 0.5, 0.16),
            0.01,
            GREY,
            xf * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.35),
        );
    }
}

// --- Head. ---

/// The head's axis, upright through the neck, a little ahead of the chest's middle.
const HEAD_Z: f32 = 0.22;
/// Heights on the head: the jaw's foot in the collar, the dark band the visor sits in, the crown.
const JAW: f32 = 6.0;
const BAND: (f32, f32) = (6.54, 6.98);
const CROWN: f32 = 7.58;
/// The band's half width and half depth (an ellipse, so the visor's glass can follow it).
const BAND_R: (f32, f32) = (0.6, 0.8);
/// The cockpit's camera, the mono-eye: where the simulation has the humanoid eye.
const EYE_AT: Vec3 = Vec3::new(0.0, 6.75, 1.1);

/// The jaw under the band, narrowing to its foot.
const JAW_HOOPS: [Hoop; 3] = [
    hoop(JAW, 0.0, 0.12, 0.44, 0.52),
    hoop(6.32, 0.0, 0.2, 0.58, 0.76),
    hoop(BAND.0 + 0.03, 0.0, HEAD_Z, 0.63, 0.83),
];

/// The helmet over the band, its brim just proud of it, closing to the crown.
const CAP_HOOPS: [Hoop; 5] = [
    hoop(BAND.1 - 0.03, 0.0, HEAD_Z, 0.64, 0.85),
    hoop(7.12, 0.0, 0.2, 0.66, 0.86),
    hoop(7.32, 0.0, 0.14, 0.58, 0.76),
    hoop(7.5, 0.0, 0.07, 0.4, 0.52),
    hoop(CROWN, 0.0, 0.03, 0.0, 0.0),
];

/// The fin along the helmet's top (forward along z, up along y): from the back of the helmet up
/// to its peak, then forward and down to a point out over the visor.
const FIN: [Vec2; 10] = [
    Vec2::new(-0.72, 7.18),
    Vec2::new(-0.4, 7.6),
    Vec2::new(-0.02, 8.08),
    Vec2::new(0.28, 8.14),
    Vec2::new(0.6, 7.9),
    Vec2::new(1.62, 7.12),
    Vec2::new(1.4, 7.02),
    Vec2::new(0.85, 7.22),
    Vec2::new(0.3, 7.48),
    Vec2::new(-0.3, 7.36),
];

fn head(d: &mut Designer) {
    let mut h = d.on(Bone::Head);
    h.seed(0.07);
    // The neck, brown, turning inside the collar.
    h.cylinder(0.42, 0.8, 12, BROWN, at(0.0, 5.95, 0.1));
    // The jaw, and the snout under the visor: a wedge narrowing forward to a blunt point.
    h.loft(&JAW_HOOPS, 2.4, 24, BODY, Affine3A::IDENTITY);
    let snout = [
        (-0.42, 6.12, 0.5),
        (0.42, 6.12, 0.5),
        (-0.5, 6.52, 0.5),
        (0.5, 6.52, 0.5),
        (-0.14, 6.16, 1.14),
        (0.14, 6.16, 1.14),
        (-0.22, 6.5, 1.14),
        (0.22, 6.5, 1.14),
    ]
    .map(|(x, y, z)| v(x, y, z));
    h.hexa(snout, 0.06, BODY, Affine3A::IDENTITY);
    h.greeble(|h| {
        for y in [6.26, 6.38] {
            h.cube(v(0.26, 0.05, 0.05), 0.0, FRAME, at(0.0, y, 1.14));
        }
    });
    for s in Side::BOTH {
        // The cheeks, swept back along the jaw, each with its vents.
        h.hexa(
            stacked([(6.12, 0.44, 0.62, -0.3, 0.5), (6.52, 0.52, 0.68, -0.36, 0.8)]),
            0.05,
            BODY,
            sided(s, Affine3A::IDENTITY),
        );
        h.greeble(|h| {
            for k in 0..3 {
                let z = -0.1 + k as f32 * 0.17;
                h.cube(v(0.05, 0.24, 0.06), 0.0, FRAME, sided(s, place(v(0.66, 6.33, z), rz(-0.2))));
            }
        });
        // The ear sensors either side of the band.
        port(&mut h, 0.17, sided(s, place(v(BAND_R.0, 6.76, HEAD_Z), ry(FRAC_PI_2))));
    }
    // The dark band round the head, and the visor's glass across its front with the mono-eye
    // behind it, lit pink.
    let axis = at(0.0, 0.0, HEAD_Z);
    let (w, dz) = BAND_R;
    h.loft(
        &[hoop(BAND.0, 0.0, HEAD_Z, w, dz), hoop(BAND.1, 0.0, HEAD_Z, w, dz)],
        2.0,
        28,
        FRAME,
        Affine3A::IDENTITY,
    );
    let oval = axis * Affine3A::from_scale(v(1.0, 1.0, dz / w));
    h.lathe_arc(&band(w - 0.02, w + 0.015, 6.63, 6.87), 28, FRAC_PI_2 - 0.7, FRAC_PI_2 + 0.7, VISOR, oval);
    let glass = HEAD_Z + (w + 0.015) * dz / w;
    h.sphere(
        0.11,
        6,
        EYE,
        Affine3A::from_translation(v(0.0, EYE_AT.y, glass + 0.01)) * Affine3A::from_scale(v(1.0, 1.0, 0.45)),
    );
    // The helmet, its brow a peak over the visor, and the fin.
    h.loft(&CAP_HOOPS, 2.4, 28, BODY, Affine3A::IDENTITY);
    let brow = [
        (-0.56, 6.97, 0.7),
        (0.56, 6.97, 0.7),
        (-0.56, 7.25, 0.7),
        (0.56, 7.25, 0.7),
        (-0.3, 6.97, 1.18),
        (0.3, 6.97, 1.18),
        (-0.3, 7.04, 1.18),
        (0.3, 7.04, 1.18),
    ]
    .map(|(x, y, z)| v(x, y, z));
    h.hexa(brow, 0.05, BODY, Affine3A::IDENTITY);
    h.extrude(&FIN, 0.18, TRIM, Affine3A::from_rotation_y(-FRAC_PI_2));
    // The fin's white root along the helmet's top, wider than the blade.
    let root = [
        v2(-0.6, 7.2),
        v2(-0.2, 7.62),
        v2(0.4, 7.62),
        v2(1.12, 7.18),
        v2(0.9, 7.08),
        v2(0.3, 7.4),
        v2(-0.3, 7.3),
    ];
    h.extrude(&root, 0.34, BODY, Affine3A::from_rotation_y(-FRAC_PI_2));
    d.sockets.eye = Designer::local(Bone::Head, EYE_AT);
    d.head_base = 6.22;
}

// --- Chest, collar and abdomen. ---

/// The chest's right half in two tiers, each given by its foot and its top as (y, outer x, back z,
/// front z at the middle): the lower swells forward to a prow, the upper leans back to the collar.
/// The front falls away outboard at `CHEST_FALL` per metre.
const CHEST_TIERS: [[(f32, f32, f32, f32); 2]; 2] = [
    [(3.35, 1.5, -1.2, 1.18), (4.45, 1.98, -1.42, 1.6)],
    [(4.45, 1.98, -1.42, 1.6), (5.85, 1.8, -1.32, 1.08)],
];
const CHEST_FALL: f32 = 0.34;

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
    let [(y0, ..), (y1, ..)] = *tier;
    let (f0, f1) = (tier[0].3, tier[1].3);
    let rise = (f1 - f0) / (y1 - y0);
    (v(x, y, f0 + rise * (y - y0) - CHEST_FALL * x), v(CHEST_FALL, -rise, 1.0).normalize())
}

/// A frame on the chest's front at (`x`, `y`), lifted `lift` off it: z out of the face, x level
/// along it.
fn on_chest(x: f32, y: f32, lift: f32) -> Affine3A {
    let (p, n) = chest_front(x, y);
    let across = v(1.0, 0.0, -CHEST_FALL).normalize();
    Affine3A::from_mat3_translation(Mat3::from_cols(across, n.cross(across), n), p + n * lift)
}

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.14);
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
        for tier in CHEST_TIERS {
            c.hexa(chest_tier(tier), 0.16, TRIM, flat);
        }
        // The raised plate on the upper tier either side of the middle, following its face.
        let corner = |x: f32, y: f32, out: f32| {
            let (p, n) = chest_front(x, y);
            p + n * out
        };
        let (lo, hi) = (4.62, 5.6);
        let plate = [
            corner(0.12, lo, -0.06),
            corner(0.95, lo + 0.25, -0.06),
            corner(0.12, hi, -0.06),
            corner(1.5, hi, -0.06),
            corner(0.12, lo, 0.12),
            corner(0.95, lo + 0.25, 0.12),
            corner(0.12, hi, 0.12),
            corner(1.5, hi, 0.12),
        ];
        c.hexa(plate, 0.05, BODY, flat);
        // The intakes on the lower tier: a dark recess and its slats.
        louvre(&mut c, 0.74, 0.6, 4, sided(s, on_chest(1.0, 3.92, 0.0)));
        c.greeble(|c| {
            c.cube(v(0.06, 0.8, 0.05), 0.0, FRAME, sided(s, on_chest(0.42, 3.9, 0.0)));
        });
        // The white flanks under the shoulders.
        let flank = [
            (1.42, 3.55, -1.15),
            (2.12, 3.55, -1.15),
            (1.72, 5.55, -1.3),
            (2.2, 5.55, -1.3),
            (1.42, 3.55, 0.85),
            (2.12, 3.55, 0.85),
            (1.72, 5.55, 0.78),
            (2.2, 5.55, 0.78),
        ]
        .map(|(x, y, z)| v(x, y, z));
        c.hexa(flank, 0.14, BODY, flat);
        // The shoulder's socket in the chest's side.
        c.cylinder(0.6, 0.5, 14, BROWN, sided(s, place(v(2.25, SHOULDER_BALL.y, 0.0), rz(FRAC_PI_2))));
        // The back's side plates either side of the pack's mount.
        let back = sided(s, place(v(1.3, 4.6, -1.55), ry(-0.12)));
        c.block(v(0.8, 1.8, 0.34), v2(0.8, 0.85), Vec2::ZERO, 0.12, BODY, back);
        slot(&mut c, 0.45, back * at(0.1, 0.2, -0.18) * Affine3A::from_rotation_y(PI));
    }
    // The seam across between the tiers.
    c.greeble(|c| {
        for s in Side::BOTH {
            c.cube(v(1.4, 0.06, 0.05), 0.0, FRAME, sided(s, on_chest(0.75, 4.47, 0.0)));
        }
    });
    // The collar: a low ring round the neck, standing up higher behind it.
    let collar = at(0.0, 0.0, 0.05);
    c.lathe(&band(0.6, 0.95, 5.75, 6.06), 24, BODY, collar);
    let (from, to) = (FRAC_PI_2 + 1.2, FRAC_PI_2 - 1.2 + TAU);
    c.lathe_arc(&band(0.62, 1.0, 6.0, 6.32), 24, from, to, BODY, collar);
    // The pack's mount in the middle of the back.
    c.block(v(1.7, 1.9, 0.4), v2(0.85, 0.95), Vec2::ZERO, 0.1, BODY, at(0.0, 4.6, -1.55));

    // The abdomen: the brown frame, and the ribbed plates over its front.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.84));
    a.lathe(&[(0.0, 2.45), (1.25, 2.45), (1.3, 2.6), (1.2, 3.4), (0.0, 3.4)], 14, BROWN, squash);
    for (y0, y1) in [(2.58, 2.95), (2.98, 3.32)] {
        a.lathe_arc(&band(1.08, 1.32, y0, y1), 14, FRAC_PI_2 - 0.85, FRAC_PI_2 + 0.85, BODY, squash);
    }
}

// --- Waist and skirts. ---

/// The belt round the hips.
const BELT: [Hoop; 3] =
    [hoop(2.0, 0.0, 0.0, 1.46, 1.08), hoop(2.1, 0.0, 0.0, 1.54, 1.16), hoop(2.56, 0.0, 0.0, 1.54, 1.16)];

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    w.loft(&BELT, 3.4, 28, TRIM, Affine3A::IDENTITY);
    // The crotch block, and the plate over its front narrowing to its foot, a red mark on it.
    w.block(v(1.05, 1.3, 1.4), v2(1.15, 1.0), v2(0.0, -0.05), 0.12, BODY, at(0.0, 1.42, 0.36));
    let front = [
        (-0.55, 0.9, 0.95),
        (0.55, 0.9, 0.95),
        (-0.62, 2.0, 1.08),
        (0.62, 2.0, 1.08),
        (-0.2, 0.9, 1.15),
        (0.2, 0.9, 1.15),
        (-0.5, 2.0, 1.3),
        (0.5, 2.0, 1.3),
    ]
    .map(|(x, y, z)| v(x, y, z));
    w.hexa(front, 0.06, BODY, Affine3A::IDENTITY);
    w.cube(v(0.34, 0.4, 0.08), 0.02, ACCENT, place(v(0.0, 1.7, 1.24), rx(-0.12)));
    for s in Side::BOTH {
        // The front skirt over the top of the thigh, hung from the belt and turned out round it,
        // its hem rising toward the outside; a blue panel down its outer half.
        let skirt = sided(s, place(v(1.12, 1.5, 1.28), ry(0.36) * rx(-0.15)));
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
        w.hexa(plate(-0.66, 0.66, 0.72, (-0.95, -0.55), -0.18, 0.18), 0.12, BODY, skirt);
        w.hexa(plate(0.08, 0.56, 0.56, (-0.68, -0.52), 0.08, 0.28), 0.05, TRIM, skirt);
        // The hip's side skirt.
        let side = sided(s, place(v(1.98, 1.75, -0.1), rz(0.14)));
        w.block(v(0.5, 1.1, 1.75), v2(0.85, 0.9), Vec2::ZERO, 0.14, BODY, side);
        slot(&mut w, 0.42, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.05, 0.26));
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.82, 1.65, -1.25), rx(0.32)));
        w.block(v(1.4, 1.4, 0.5), v2(0.92, 0.85), v2(0.0, 0.05), 0.16, BODY, rear);
        slot(&mut w, 0.35, rear * at(0.2, 0.0, -0.26) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder's ball (right side), under its armour.
const SHOULDER_BALL: Vec3 = Vec3::new(3.2, 4.75, 0.0);
/// The shoulder armour's section across (x, y), as a block along z: its inner foot, outer foot,
/// inner top and outer top. Its top rises steeply outboard to the high outer corner.
const PAULDRON: [(f32, f32); 4] = [(2.45, 4.55), (4.55, 3.8), (2.45, 5.95), (4.9, 7.3)];
/// Its back and front, at its inner side and at its outer: it narrows outboard.
const PAULDRON_BACK: (f32, f32) = (-1.35, -0.95);
const PAULDRON_FRONT: (f32, f32) = (1.3, 0.85);

/// A block along z from its section across in x-y (inner foot, outer foot, inner top, outer top),
/// its back and its front each running straight from their `.0` at `x.0` to their `.1` at `x.1`:
/// every face flat whatever the section.
fn tapered(section: [(f32, f32); 4], x: (f32, f32), back: (f32, f32), front: (f32, f32)) -> [Vec3; 8] {
    let z = |(a, b): (f32, f32), at: f32| a + (b - a) * (at - x.0) / (x.1 - x.0);
    let [p, q, r, t] = section;
    let c = |(px, py): (f32, f32), zz: (f32, f32)| v(px, py, z(zz, px));
    [c(p, back), c(q, back), c(r, back), c(t, back), c(p, front), c(q, front), c(r, front), c(t, front)]
}

/// The pauldron's own taper, for what stands on it.
fn on_pauldron(section: [(f32, f32); 4], inset: f32) -> [Vec3; 8] {
    let x = (PAULDRON[0].0, PAULDRON[3].0);
    let (b, f) = (PAULDRON_BACK, PAULDRON_FRONT);
    tapered(section, x, (b.0 + inset, b.1 + inset), (f.0 - inset, f.1 - inset))
}

/// A point on the pauldron's edge from corner `a` to corner `b` of its section, `t` of the way
/// along and `out` metres off it, to the edge's right as it runs from `a` to `b`.
fn edge(a: usize, b: usize, t: f32, out: f32) -> (f32, f32) {
    let (p, q) = (Vec2::from(PAULDRON[a]), Vec2::from(PAULDRON[b]));
    let run = q - p;
    let n = Vec2::new(run.y, -run.x).normalize();
    let at = p + run * t + n * out;
    (at.x, at.y)
}

/// The upper arm's drum, up its axis from the elbow.
const UPPER_ARM: [Hoop; 4] = [
    hoop(0.42, 0.0, 0.0, 0.62, 0.64),
    hoop(0.58, 0.0, 0.0, 0.7, 0.72),
    hoop(1.4, 0.0, 0.0, 0.7, 0.72),
    hoop(1.56, 0.0, 0.0, 0.6, 0.62),
];

/// The gauntlet, up the forearm from the cuff (x out, z forward): a heavy box swelling a little
/// below the elbow and rounding in under it.
const GAUNTLET: [Hoop; 5] = [
    hoop(0.38, 0.0, 0.04, 0.78, 0.8),
    hoop(1.1, 0.03, 0.03, 0.86, 0.88),
    hoop(1.55, 0.03, 0.0, 0.8, 0.82),
    hoop(1.85, 0.0, 0.0, 0.62, 0.64),
    hoop(1.97, 0.0, 0.0, 0.46, 0.48),
];
/// The cuff round the wrist, flaring.
const CUFF: [Hoop; 3] =
    [hoop(0.0, 0.0, 0.05, 0.8, 0.84), hoop(0.3, 0.0, 0.05, 0.88, 0.92), hoop(0.42, 0.0, 0.05, 0.82, 0.86)];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);
        let flat = sided(s, Affine3A::IDENTITY);

        // The shoulder: the brown ball, and the armour over it, its top rising steeply to the
        // high outer corner, narrowing outboard; a blue plate along its top, a white one standing
        // on its outer face, a port in its front and a vernier in its back.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.sphere(0.82, 10, BROWN, sided(s, Affine3A::from_translation(SHOULDER_BALL)));
        o.hexa(on_pauldron(PAULDRON, 0.0), 0.22, BODY, flat);
        // (The top's edge runs from the inner top to the outer, so its right is down, into it.)
        let top = [
            edge(2, 3, 0.05, 0.06),
            edge(2, 3, 1.03, 0.06),
            edge(2, 3, 0.05, -0.26),
            edge(2, 3, 1.03, -0.26),
        ];
        o.hexa(on_pauldron(top, 0.18), 0.08, TRIM, flat);
        let outer =
            [edge(1, 3, 0.12, -0.06), edge(1, 3, 0.12, 0.16), edge(1, 3, 0.8, -0.06), edge(1, 3, 0.8, 0.16)];
        o.hexa(on_pauldron(outer, 0.22), 0.07, BODY, flat);
        // The blue flare along the foot of its outer half.
        let hem =
            [edge(0, 1, 0.4, -0.14), edge(0, 1, 1.03, -0.14), edge(0, 1, 0.4, 0.12), edge(0, 1, 1.03, 0.12)];
        o.hexa(on_pauldron(hem, 0.1), 0.06, TRIM, flat);
        // The plates standing on its front and its back, a step in from their edges.
        let face_plate = [(2.72, 4.66), (4.36, 4.08), (2.72, 5.82), (4.6, 6.86)];
        let (b, f) = (PAULDRON_BACK, PAULDRON_FRONT);
        let x = (PAULDRON[0].0, PAULDRON[3].0);
        o.hexa(tapered(face_plate, x, (f.0 - 0.1, f.1 - 0.1), (f.0 + 0.12, f.1 + 0.12)), 0.08, BODY, flat);
        o.hexa(tapered(face_plate, x, (b.0 - 0.12, b.1 - 0.12), (b.0 + 0.1, b.1 + 0.1)), 0.08, BODY, flat);
        let front = |x: f32| f.0 + 0.12 + (f.1 - f.0) * (x - 2.45) / 2.45;
        let face = |x: f32, y: f32| {
            let lean = ((f.1 - f.0) / 2.45).atan();
            sided(s, place(v(x, y, front(x)), ry(lean)))
        };
        port(&mut o, 0.26, face(3.25, 5.05));
        o.greeble(|o| {
            o.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, face(2.95, 5.25));
            o.cube(v(1.2, 0.06, 0.05), 0.0, FRAME, face(3.85, 5.9) * Affine3A::from_rotation_z(0.4));
            o.cube(v(0.5, 0.06, 0.05), 0.0, FRAME, face(3.95, 4.55) * Affine3A::from_rotation_z(-0.27));
        });
        bell(&mut o, v(3.55, 5.3, -1.05), v(0.0, -0.2, -1.0), 0.38, flat);

        // The upper arm: a white drum with a blue ring round it, on the brown frame.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.44, 1.7, 12, BROWN, up_xf * at(0.0, 0.95, 0.0));
        u.loft(&UPPER_ARM, 3.4, 20, BODY, up_xf);
        u.loft(&[hoop(0.86, 0.0, 0.0, 0.74, 0.76), hoop(1.06, 0.0, 0.0, 0.74, 0.76)], 3.4, 20, TRIM, up_xf);

        // The forearm: the elbow's brown barrel, the blue cuff, the white gauntlet over it, and
        // the elbow guard standing out of its outside and back.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.52, 1.1, 12, BROWN, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&CUFF, 3.6, 20, TRIM, fo_xf);
        f.loft(&GAUNTLET, 3.6, 20, BODY, fo_xf);
        let guard = stacked([(0.9, 0.55, 1.02, -0.62, 0.45), (2.15, 0.45, 0.86, -0.9, 0.05)]);
        f.hexa(guard, 0.1, BODY, fo_xf);
        f.greeble(|f| {
            // A seam round the gauntlet's middle, and a dark slot down its front.
            f.cube(v(0.05, 0.62, 0.05), 0.0, FRAME, fo_xf * at(1.04, 1.35, -0.18));
            f.cube(v(0.32, 0.06, 0.05), 0.0, FRAME, fo_xf * at(0.0, 1.0, 0.9));
            f.cube(v(0.06, 0.5, 0.05), 0.0, FRAME, fo_xf * at(0.0, 1.35, 0.9));
        });
        f.cylinder(0.46, 0.3, 12, BROWN, fo_xf);
    }
    hands(d, BROWN);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): thick, squared,
/// fullest a little over halfway up; the plates stand on it.
const THIGH: [Hoop; 6] = [
    hoop(0.45, 0.2, 0.05, 0.95, 0.92),
    hoop(0.9, 0.24, 0.1, 1.15, 1.08),
    hoop(2.2, 0.28, 0.1, 1.24, 1.14),
    hoop(3.1, 0.24, 0.04, 1.16, 1.06),
    hoop(3.55, 0.18, 0.0, 0.95, 0.88),
    hoop(3.82, 0.12, 0.0, 0.62, 0.6),
];

/// The lower leg's core, up its axis from the ankle to the knee: over the boot, swelling back and
/// out to the full calf high up, and closing in behind the knee guard.
const SHIN: [Hoop; 8] = [
    hoop(0.75, 0.0, 0.05, 0.92, 0.95),
    hoop(1.5, 0.03, -0.02, 1.0, 1.05),
    hoop(2.5, 0.06, -0.15, 1.16, 1.2),
    hoop(3.5, 0.08, -0.3, 1.3, 1.35),
    hoop(4.25, 0.08, -0.34, 1.3, 1.34),
    hoop(4.85, 0.06, -0.28, 1.12, 1.15),
    hoop(5.25, 0.04, -0.22, 0.8, 0.82),
    hoop(5.42, 0.02, -0.2, 0.5, 0.5),
];

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.2;

/// A block of a foot ([`frames::boot`](crate::frames::boot)), its sides leaning in [`BOOT_LEAN`].
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    crate::frames::boot(x, base, top, half, z, BOOT_LEAN)
}

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);
        let flat = sided(s, Affine3A::IDENTITY);

        // The thigh: a thick squared core on a brown ball at the hip, a broad plate down its front
        // and another on its outside, a step where they meet; seams across them.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.75, 6, BROWN, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 3.6, 24, BODY, along_thigh);
        th.hexa(
            stacked([(0.75, -0.72, 0.95, 0.75, 1.26), (3.0, -0.82, 1.08, 0.7, 1.24)]),
            0.14,
            BODY,
            along_thigh,
        );
        th.hexa(
            stacked([(0.6, 1.05, 1.44, -0.75, 0.7), (3.3, 1.2, 1.62, -0.75, 0.66)]),
            0.14,
            BODY,
            along_thigh,
        );
        th.hexa(
            stacked([(0.8, -0.7, 0.9, -1.2, -0.78), (3.1, -0.78, 1.0, -1.14, -0.7)]),
            0.12,
            BODY,
            along_thigh,
        );
        th.greeble(|th| {
            th.cube(v(1.5, 0.06, 0.05), 0.0, FRAME, along_thigh * at(0.1, 1.85, 1.26));
            th.cube(v(0.05, 0.06, 1.2), 0.0, FRAME, along_thigh * at(1.53, 1.6, -0.05));
            th.cube(v(0.05, 0.9, 0.06), 0.0, FRAME, along_thigh * at(1.54, 2.6, 0.3));
        });

        // The lower leg: the knee's brown barrel; the core swelling to the calf; a blue plate down
        // its front stepping out under the knee guard, and white plates down its sides; vents in
        // the back of the calf.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.62, 1.3, 12, BROWN, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN, 3.4, 24, BODY, tube);
        m.hexa(stacked([(1.2, -0.62, 0.62, 0.7, 1.1), (4.55, -0.82, 0.82, 0.75, 1.2)]), 0.12, TRIM, tube);
        for out in [1.0, -1.0] {
            let side = tube * Affine3A::from_scale(v(out, 1.0, 1.0));
            m.hexa(stacked([(1.4, 0.75, 1.12, -1.0, 0.5), (4.4, 1.05, 1.46, -1.38, 0.68)]), 0.12, BODY, side);
        }
        m.greeble(|m| {
            m.cube(v(1.3, 0.06, 0.05), 0.0, FRAME, tube * place(v(0.0, 3.3, 1.17), rx(-0.03)));
            for out in [1.0, -1.0] {
                let side = tube * Affine3A::from_scale(v(out, 1.0, 1.0));
                m.cube(v(0.05, 0.06, 1.5), 0.0, FRAME, side * at(1.38, 3.5, -0.3));
            }
        });
        for y in [3.15, 4.05] {
            louvre(&mut m, 0.72, 0.62, 3, tube * place(v(0.08, y, -1.6), ry(PI)));
        }
        // The knee guard: two plates meeting in a ridge that comes to a point forward.
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        let ridge = (-0.1, -0.8, 0.8, 0.42, 1.66);
        m.hexa(stacked([(-1.3, -0.56, 0.56, 0.5, 1.08), ridge]), 0.1, BODY, guard);
        m.hexa(stacked([ridge, (1.15, -0.64, 0.64, 0.22, 1.08)]), 0.12, BODY, guard);
        // A blue strip down the lower plate's face from the ridge.
        let fall = v(0.0, -1.2, 1.08 - 1.66).normalize();
        m.cube(v(0.3, 0.95, 0.1), 0.02, TRIM, guard * place(v(0.0, -0.62, 1.38), toward(-fall)));
        m.greeble(|m| {
            m.cube(v(0.06, 0.8, 0.05), 0.0, FRAME, guard * place(v(0.0, 0.5, 1.37), rx(-0.47)));
        });

        // The boot's shaft round the ankle, its tongue rising up the front, a brown band round
        // its back and the brown hubs either side.
        let x = ankle.x;
        let (y0, y1) = (ankle.y - 0.55, ankle.y + 1.0);
        m.loft(
            &[
                hoop(y0, x, -0.1, 1.18, 1.1),
                hoop(y0 + 0.9, x, -0.12, 1.16, 1.08),
                hoop(y1, x, -0.14, 1.02, 0.95),
                hoop(y1 + 0.1, x, -0.14, 0.88, 0.82),
            ],
            3.4,
            24,
            BODY,
            flat,
        );
        let tongue = [
            (-1.0, y0 + 0.1, 0.55),
            (1.0, y0 + 0.1, 0.55),
            (-0.78, y1 + 0.35, 0.6),
            (0.78, y1 + 0.35, 0.6),
            (-1.0, y0 + 0.1, 1.72),
            (1.0, y0 + 0.1, 1.72),
            (-0.78, y1 + 0.35, 1.16),
            (0.78, y1 + 0.35, 1.16),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.18, BODY, flat);
        m.greeble(|m| {
            let slope = v(0.0, y1 + 0.35 - (y0 + 0.1), 1.16 - 1.72).normalize();
            let face = sided(s, place(v(x, y1 - 0.25, 1.33), toward(slope)));
            m.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, face);
        });
        m.lathe_arc(
            &band(1.12, 1.24, ankle.y - 0.35, ankle.y + 0.05),
            18,
            0.25,
            PI - 0.25,
            BROWN,
            sided(s, at(x, 0.0, ankle.z - 0.12)) * Affine3A::from_rotation_y(PI),
        );
        for side in [-1.0, 1.0] {
            let hub = sided(s, place(v(x + side * 1.15, ankle.y - 0.2, ankle.z - 0.15), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.12), (0.34, -0.12), (0.4, -0.05), (0.4, 0.05), (0.34, 0.12), (0.0, 0.12)],
                16,
                BROWN,
                hub,
            );
            m.cylinder(0.17, 0.3, 6, FRAME, hub);
        }

        // The foot: the heel and the middle in one tall block, highest at the back, and the blue
        // toe on its own hinge, falling and narrowing to a blunt point with a white ridge along
        // it; a dark sole under it all, split at the hinge, a vernier in each half.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, BROWN, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.3;
        f.hexa(boot(x, base, (-7.45, -7.8), (1.22, 1.18), [-2.15, -1.85, 1.28, 1.28]), 0.24, BODY, flat);
        f.greeble(|f| {
            for side in [-1.0, 1.0] {
                let face = sided(s, place(v(x + side * 1.12, -8.4, -1.25), rz(side * BOOT_LEAN.atan())));
                f.cube(v(0.05, 0.5, 0.05), 0.0, FRAME, face);
            }
            f.cube(v(0.6, 0.06, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.0, -2.05), rx(-0.3))));
        });
        let heel =
            Sole { z: (-2.25, 1.26), half: (1.28, 1.24), top: 0.32, pads: BROWN, vernier: -0.72, r: 0.4 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.86, -8.36), (1.2, 0.56), [1.32, 1.32, 3.55, 3.1]), 0.22, TRIM, flat);
        t.hexa(boot(x, -8.1, (-7.74, -8.16), (0.42, 0.26), [1.4, 1.4, 2.9, 2.62]), 0.08, BODY, flat);
        let toes =
            Sole { z: (1.34, 3.62), half: (1.24, 0.62), top: 0.32, pads: BROWN, vernier: 2.25, r: 0.32 };
        sole(d, toe, s, x, toes);
    }
}

// --- The flight pack. ---

/// The pack's middle, and the engines either side of it (x, z of their axes).
const PACK_Z: f32 = -2.75;
const ENGINE: (f32, f32) = (1.3, -2.85);
/// The nose cone's foot on the pack's top, and its tip, standing up behind the head.
const NOSE_FOOT: Vec3 = Vec3::new(0.0, 5.45, -2.8);
const NOSE_TIP: Vec3 = Vec3::new(0.0, 8.95, -3.3);

fn flight_pack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The mount off the back, and the pack's body.
    b.block(v(1.5, 1.6, 0.7), v2(0.9, 0.9), Vec2::ZERO, 0.1, TRIM, at(0.0, 4.5, -1.85));
    b.loft(
        &[
            hoop(2.4, 0.0, PACK_Z, 0.9, 0.7),
            hoop(2.7, 0.0, PACK_Z, 1.05, 0.85),
            hoop(5.2, 0.0, PACK_Z - 0.05, 1.05, 0.88),
            hoop(5.6, 0.0, PACK_Z - 0.05, 0.95, 0.8),
        ],
        3.5,
        24,
        BODY,
        Affine3A::IDENTITY,
    );
    // A blue hatch on its back with a louvre under it.
    let back = place(v(0.0, 4.35, PACK_Z - 0.9), ry(PI));
    b.block(v(1.1, 1.0, 0.16), v2(0.85, 1.0), Vec2::ZERO, 0.04, TRIM, back);
    louvre(&mut b, 0.9, 0.7, 4, place(v(0.0, 3.25, PACK_Z - 0.86), ry(PI)));
    b.greeble(|b| {
        for (x, y) in [(-0.4, 4.0), (0.4, 4.0), (-0.4, 4.7), (0.4, 4.7)] {
            b.cylinder(0.05, 0.06, 6, GREY, back * place(v(x, y - 4.35, 0.1), rx(FRAC_PI_2)));
        }
    });
    // The engines either side, their intakes at the top.
    for s in Side::BOTH {
        let (x, z) = ENGINE;
        let axis = sided(s, at(x, 0.0, z));
        b.loft(
            &[
                hoop(2.05, 0.0, 0.0, 0.5, 0.5),
                hoop(2.3, 0.0, 0.0, 0.64, 0.64),
                hoop(4.7, 0.0, 0.0, 0.66, 0.66),
                hoop(5.0, 0.0, 0.0, 0.6, 0.6),
            ],
            2.0,
            24,
            BODY,
            axis,
        );
        b.lathe(&band(0.44, 0.62, 4.96, 5.1), 20, TRIM, axis);
        b.cylinder(0.46, 0.1, 16, FRAME, axis * at(0.0, 4.98, 0.0));
        b.lathe(&band(0.64, 0.7, 3.3, 3.5), 20, TRIM, axis);
    }
    // The nose cone, blue, standing up behind the head, its tip red.
    let nose = place(NOSE_FOOT, toward(NOSE_TIP - NOSE_FOOT));
    let len = NOSE_FOOT.distance(NOSE_TIP);
    b.lathe(
        &[
            (0.0, 0.0),
            (0.78, 0.0),
            (0.8, 0.3),
            (0.78, 1.0),
            (0.7, 1.75),
            (0.55, 2.45),
            (0.38, 2.95),
            (0.24, 3.22),
            (0.0, 3.22),
        ],
        24,
        TRIM,
        nose,
    );
    b.lathe(&[(0.0, 3.2), (0.25, 3.2), (0.16, 3.38), (0.08, 3.48), (0.0, len)], 16, ACCENT, nose);
    b.lathe(&band(0.8, 0.84, 0.35, 0.6), 24, BODY, nose);
    // The beacon on the pack's back.
    let beacon = v(0.0, 5.1, PACK_Z - 0.9);
    b.cylinder(0.18, 0.12, 10, GUN, place(beacon, rx(-FRAC_PI_2)));
    b.sphere(0.15, 6, Paint::Light(Light::Beacon), at(0.0, beacon.y, beacon.z - 0.08));
    // The grey grab handle across its back under the hatch, on its brackets.
    let grip = PACK_Z - 1.12;
    b.cylinder(0.07, 1.0, 10, GREY, place(v(0.0, 2.75, grip), rz(FRAC_PI_2)));
    for x in [-0.42, 0.42] {
        b.cube(v(0.12, 0.14, 0.36), 0.02, GREY, at(x, 2.75, grip + 0.14));
    }
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(ENGINE.0, 1.95, ENGINE.1 - 0.05)), v(0.0, -1.0, -0.35), 0.52);
    }
}

/// Where the wing meets the pack (right side), how far it rises out to its tip, and how far its
/// chord tips down at the back.
const WING_ROOT: Vec3 = Vec3::new(1.85, 4.6, -2.9);
const DIHEDRAL: f32 = 0.2;
const CANT: f32 = 0.6;
/// The wing's outline (out along the span, forward along the chord): swept back to a cropped
/// tip.
const WING: [Vec2; 6] = [
    Vec2::new(0.0, 1.1),
    Vec2::new(1.0, 0.75),
    Vec2::new(4.5, -0.75),
    Vec2::new(4.7, -1.05),
    Vec2::new(4.5, -1.75),
    Vec2::new(0.0, -1.6),
];

fn wings(d: &mut Designer) {
    let (sn, cs) = DIHEDRAL.sin_cos();
    let span = v(cs, sn, 0.0);
    let chord = v(0.0, CANT.sin(), CANT.cos());
    let chord = (chord - span * chord.dot(span)).normalize();
    let plane = Affine3A::from_mat3_translation(Mat3::from_cols(span, chord, span.cross(chord)), WING_ROOT);
    for s in Side::BOTH {
        let bone = s.pick(Bone::WingL, Bone::WingR);
        let mut w = d.on(bone);
        w.seed(0.55 + s.pick(0.0, 0.2));
        let xf = sided(s, plane);
        w.extrude(&WING, 0.22, BODY, xf);
        // The blue panel along the wing, and the red tip.
        let panel = [v2(0.6, 0.55), v2(3.4, -0.55), v2(3.4, -1.38), v2(0.6, -1.25)];
        w.extrude(&panel, 0.28, TRIM, xf);
        let tip = [v2(3.85, -0.5), v2(4.5, -0.75), v2(4.7, -1.05), v2(4.5, -1.75), v2(3.85, -1.72)];
        w.extrude(&tip, 0.26, ACCENT, xf);
        // The root's fairing into the engine.
        w.block(
            v(0.7, 0.6, 2.6),
            v2(0.8, 0.9),
            Vec2::ZERO,
            0.1,
            BODY,
            xf * at(0.1, -0.25, 0.0) * Affine3A::from_rotation_x(-FRAC_PI_2),
        );
        // A hardpoint under it.
        w.cube(v(0.22, 1.3, 0.34), 0.03, GUN, xf * at(2.4, -0.45, 0.27));
        // The running light at the tip: red on the suit's own left (+x), green on its right.
        let nav = xf * at(4.62, -1.05, 0.0);
        w.cube(v(0.3, 0.34, 0.24), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        w.sphere(0.13, 6, lamp, nav * at(0.12, 0.0, 0.0));
    }
}

// --- The beam rifle. ---

/// The beam rifle, long and slim: a receiver with a blue band round it over the grip, the energy
/// pack before the grip, the stock; a ribbed shroud, the barrel and the octagonal muzzle with its
/// bore; the sight on top with its lens, and a foregrip.
fn beam_rifle(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let y = 0.62;
    let along_z = |z: f32| place(v(g.x, g.y + y, g.z + z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    w.cube(v(0.32, 0.95, 0.42), 0.05, GUN, place(v(g.x, g.y - 0.2, g.z), rx(-0.25)));
    w.block(v(0.62, 0.92, 3.0), v2(0.92, 0.95), Vec2::ZERO, 0.1, GUN, at(g.x, g.y + y, g.z + 0.5));
    w.cube(v(0.68, 0.6, 1.3), 0.04, TRIM, at(g.x, g.y + y - 0.05, g.z + 0.25));
    w.block(v(0.5, 0.95, 0.8), v2(1.0, 0.9), Vec2::ZERO, 0.06, GUN, at(g.x, g.y - 0.1, g.z + 1.4));
    w.cube(v(0.42, 0.12, 0.66), 0.02, STEEL, at(g.x, g.y - 0.6, g.z + 1.4));
    // The stock and its butt.
    w.block(v(0.5, 0.75, 1.7), v2(0.85, 0.7), v2(0.0, -0.1), 0.08, GUN, at(g.x, g.y + y - 0.12, g.z - 1.8));
    w.cube(v(0.56, 1.05, 0.3), 0.05, GUN, at(g.x, g.y + y - 0.25, g.z - 2.7));
    // The shroud, the barrel, and the muzzle with its bore.
    w.cylinder(0.34, 2.2, 14, GUN, along_z(3.05));
    w.cube(v(0.24, 0.7, 0.32), 0.03, GUN, place(v(g.x, g.y + y - 0.6, g.z + 2.6), rx(0.2)));
    w.cylinder(0.2, 2.2, 12, STEEL, along_z(5.2));
    w.cylinder(0.32, 0.55, 8, GUN, along_z(6.5) * Affine3A::from_rotation_y(PI / 8.0));
    w.cylinder(0.17, 0.12, 12, FRAME, along_z(6.78));
    // The sight on top, its lens.
    w.cube(v(0.26, 0.32, 0.9), 0.03, GUN, at(g.x, g.y + y + 0.6, g.z + 0.4));
    w.cylinder(0.12, 0.08, 10, LENS, place(v(g.x, g.y + y + 0.62, g.z + 0.86), rx(FRAC_PI_2)));
    w.greeble(|w| {
        // Vents along the receiver, rings round the shroud, slots in the muzzle.
        for k in 0..5 {
            w.cube(v(0.66, 0.36, 0.08), 0.0, FRAME, at(g.x, g.y + y + 0.05, g.z + 1.15 + k as f32 * 0.16));
        }
        for k in 0..4 {
            w.cylinder(0.37, 0.1, 14, GUN, along_z(2.25 + k as f32 * 0.5));
        }
        for side in [-1.0, 1.0] {
            w.cube(v(0.08, 0.14, 0.3), 0.0, FRAME, at(g.x + side * 0.28, g.y + y, g.z + 6.45));
        }
    });
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + y, g.z + 6.85));
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

/// The right pauldron's outer plate at its middle, as a frame: x running back along it (as one
/// reads it standing beside the suit), y up it, z out of it.
fn pauldron_face() -> Affine3A {
    let up = (Vec2::from(PAULDRON[3]) - Vec2::from(PAULDRON[1])).normalize();
    let (x, y) = edge(1, 3, 0.48, 0.16 + STENCIL.0);
    let cols = Mat3::from_cols(v(0.0, 0.0, -1.0), v(up.x, up.y, 0.0), v(up.y, -up.x, 0.0));
    Affine3A::from_mat3_translation(cols, v(x, y, -0.05))
}

fn stencils(d: &mut Designer) {
    let face = pauldron_face();
    let mut o = d.on(Bone::ShoulderR);
    o.greeble(|o| {
        // The unit number on the right shoulder, reading front to back as one stands beside it
        // (never mirrored: the left wears the insignia).
        let (w, h) = (0.5, 0.85);
        for digit in 0..2u8 {
            let x = (f32::from(digit) - 0.5) * (w + 0.1);
            for (segment, (cx, cy, sw, sh)) in segments(w, h, 0.09).into_iter().enumerate() {
                let paint = Paint::Digit { place: digit, segment: segment as u8 };
                o.cube(v(sw, sh, STENCIL.1), 0.0, paint, face * at(x + cx, cy, 0.0));
            }
        }
    });
    let face = sided(Side::L, face);
    let mut o = d.on(Bone::ShoulderL);
    o.greeble(|o| {
        // The insignia on the left, in the trim: a hexagon round a chevron, as the Argo wears it.
        let r = 0.4;
        let corner = |i: f32| {
            let t = std::f32::consts::FRAC_PI_3 * i + std::f32::consts::FRAC_PI_6;
            v(r * t.cos(), r * t.sin(), 0.0)
        };
        for k in 0..6 {
            let (p, q) = (corner(k as f32), corner(k as f32 + 1.0));
            o.cube(v(0.07, r + 0.035, STENCIL.1), 0.0, TRIM, face * place((p + q) * 0.5, toward(q - p)));
        }
        for side in [-1.0, 1.0] {
            let (p, q) = (v(side * 0.19, 0.1, 0.0), v(0.0, -0.12, 0.0));
            let bar = place((p + q) * 0.5, toward(q - p));
            o.cube(v(0.09, (q - p).length() + 0.06, STENCIL.1), 0.0, TRIM, face * bar);
        }
        o.cube(v(0.3, 0.07, STENCIL.1), 0.0, TRIM, face * at(0.0, 0.2, 0.0));
    });
}
