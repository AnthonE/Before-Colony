//! PX-04 Thuban, the colonies' duellist: a martial artist's upright build, blocky and sturdy, a
//! small head set low between high, broad shoulders, a narrow waist and long, heavy legs; the
//! dragon on its right arm (the Talon: its head flies out on a cable the client draws, the
//! flamethrower in its mouth) and the twin-bladed beam glaive in its left hand. Part by part:
//!
//! - the head: a white helmet domed over the brow, the faceplate's two halves meeting in a ridge
//!   down to a green chin with a vent either side, two sharp green eyes in a dark visor under the
//!   brow's plates, cheek guards with a round port in each; a green block on the forehead with its
//!   lens, the green crest rising out of it, the yellow V-fin swept up and back, and a white horn
//!   swept back off either side of the helmet, the dragon's crown;
//! - a broad chest: the green breastplate in two halves meeting in a crease, standing forward at
//!   its foot and leaning back to the collar, an intake with yellow slats high on each half; white
//!   blocks a step back either side of the head, and the yellow collar round the neck in two
//!   steps, its tabs at the throat; the white belly plate under the breastplate with a yellow coin
//!   on it, its square hole through the middle; white blocks under the arms; and the narrow grey
//!   waist, ribbed, over the belt;
//! - a narrow white belt with a yellow buckle, the crotch with a green V on it, front skirts
//!   turned out over the thighs with a green panel raised on their lower halves and yellow studs,
//!   side plates over the hips, and the rear skirt;
//! - shoulders that are big squarish white blocks, each under a green roof whose eave turns up at
//!   its outer end as a temple's does, a ridge along its top; a raised panel on the front with a
//!   yellow cloud head on it, as on a ruyi sceptre, a port in the outer end and the running light
//!   at its front corner; white upper arms with a green ring; on the left, a white bracer flaring
//!   to its cuff with a green wrist guard studded in yellow, and a grey fist;
//! - on the right, the dragon: its neck down the forearm in green rings, each flared over the one
//!   below like a telescope's tubes and finned in yellow down its back, and its head on the hand:
//!   a boxy green skull falling to a blunt nose, heavy brows over green eyes, white cheeks, yellow
//!   horns swept back along the neck, a white mane behind the cheeks and yellow whiskers trailing
//!   from the snout; the fangs, the dark throat with the flamethrower's nozzle in it, and the lower
//!   jaw (on the weapon's bone: the mouth is what fires) hanging open on its hinge, a white beard
//!   under its chin;
//! - long, heavy legs, plated: each thigh a squarish core under a front plate in two halves
//!   meeting in a ridge, and an outer plate with a green panel raised on it; green knee guards
//!   with a ridge down their faces and a wedge under them; full calves under a two-tier shin guard
//!   with a chevron engraved on it, a green plate with its vents on the outside of each calf and
//!   white plates inside and behind; and heavy boots: a squarish shaft over the ankle, its tongue
//!   up the front with a green strip on it, a grey band behind and a big grey disc either side,
//!   on a thick platform with a high heel and a long toe on its own hinge, a green block raised
//!   along it, and a vernier in the sole under the heel and under the toe;
//! - on the back, a white pack with green sides, its louvres and a row of yellow spikes along its
//!   top, the two main thrusters under it, and the dragon's tail: white segments tapering down and
//!   back from the pack's foot, each with a flared green rim and a yellow fin, to a green spade;
//! - the glaive: a gunmetal staff through the fist, its grip wrapped, yellow collars and
//!   hexagonal shafts either side, and an emitter at each end between two prongs; the blade leaves
//!   the upper one.
//!
//! White, green and yellow are the livery's body, trim and accent. The inner frame, the fist,
//! the ankles' discs and the soles are grey, the glaive gunmetal, the teeth ivory.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::detail::{GREY, LENS, band, port, slot, swap_x, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, THROAT, TRIM, at, j, nozzle, place, rx, ry, rz, sided,
    sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

pub(crate) fn thuban(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    talon(d);
    legs(d);
    backpack(d);
    glaive(d);
}

/// Teeth and fangs.
const IVORY: Paint = Paint::Fixed(paint::WHITE);

/// A hexahedron's corners described for the right side, on side `s` (mirrored and reordered on
/// the left, so they stay in the kit's order).
fn on_side(s: Side, c: [Vec3; 8]) -> [Vec3; 8] {
    if s == Side::R { c } else { swap_x(c.map(|p| v(-p.x, p.y, p.z))) }
}

/// Corners from tuples.
fn corners(c: [(f32, f32, f32); 8]) -> [Vec3; 8] {
    c.map(|(x, y, z)| v(x, y, z))
}

/// A frame at `origin` whose local y runs along `down` and local z along `out` (made square to
/// it): for parts laid along a line, faced one way.
fn frame_along(origin: Vec3, down: Vec3, out: Vec3) -> Affine3A {
    let y = down.normalize();
    let z = (out - y * y.dot(out)).normalize();
    Affine3A::from_mat3_translation(Mat3::from_cols(y.cross(z), y, z), origin)
}

/// A louvre on a face at `xf` (looking along its +z): a dark recess `w` by `h` and `n` slats
/// across it, tipped to shed downward.
fn louvre(o: &mut On<'_>, w: f32, h: f32, n: usize, slats: Paint, xf: Affine3A) {
    o.cube(v(w, h, 0.12), 0.02, FRAME, xf);
    let pitch = h / n as f32;
    for k in 0..n {
        let y = -h * 0.5 + pitch * (k as f32 + 0.5);
        o.cube(
            v(w - 0.08, pitch * 0.55, 0.16),
            0.01,
            slats,
            xf * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.35),
        );
    }
}

// --- Head. ---

/// The head's axis, upright through the neck, a little ahead of the chest's middle.
const HEAD_Z: f32 = 0.2;
/// Heights on the head: the chin's foot, the brow over the eyes, and the crown. The head is
/// small and sits low, between the shoulders' caps.
const CHIN: f32 = 6.26;
const BROW: f32 = 6.92;
const CROWN: f32 = 7.56;
/// The cockpit's camera, between the eyes: where the simulation has a humanoid's.
const CAMERA: Vec3 = Vec3::new(0.0, 6.75, 1.1);

/// The V-fin's right blade, in its own plane: out along x from the crest's foot, up along y.
const V_FIN: [Vec2; 9] = [
    Vec2::new(0.0, -0.08),
    Vec2::new(0.3, 0.04),
    Vec2::new(0.9, 0.42),
    Vec2::new(1.5, 0.98),
    Vec2::new(1.66, 1.2),
    Vec2::new(1.4, 1.04),
    Vec2::new(0.8, 0.64),
    Vec2::new(0.24, 0.3),
    Vec2::new(0.0, 0.18),
];

/// The crest, rising out of the forehead behind the V-fin: forward along x, up along y.
const CREST: [Vec2; 6] = [
    Vec2::new(0.14, 0.0),
    Vec2::new(0.06, 0.34),
    Vec2::new(-0.16, 0.62),
    Vec2::new(-0.42, 0.78),
    Vec2::new(-0.3, 0.42),
    Vec2::new(-0.24, 0.0),
];

/// A horn back off the helmet's side, the dragon's crown: back along x, up along y.
const CROWN_HORN: [Vec2; 6] = [
    Vec2::new(0.0, -0.06),
    Vec2::new(0.4, -0.04),
    Vec2::new(0.95, 0.2),
    Vec2::new(1.25, 0.52),
    Vec2::new(0.8, 0.36),
    Vec2::new(0.2, 0.26),
];

fn head(d: &mut Designer) {
    let mut h = d.on(Bone::Head);
    h.seed(0.05);
    let axis = at(0.0, 0.0, HEAD_Z);
    // The neck, grey, turning in the collar.
    h.cylinder(0.42, 0.85, 12, GREY, at(0.0, 5.95, HEAD_Z - 0.05));
    // The head's core under the helmet, round, its front behind the face.
    h.lathe(
        &[(0.0, CHIN + 0.06), (0.58, CHIN + 0.06), (0.7, CHIN + 0.2), (0.74, BROW), (0.0, BROW)],
        20,
        BODY,
        axis * Affine3A::from_scale(v(1.0, 1.0, 1.05)),
    );
    // The face mask under the eyes: two halves meeting in a ridge down its front, narrowing to
    // the green chin, a vent either side of it.
    for s in Side::BOTH {
        let mask = corners([
            (0.0, CHIN, 0.55),
            (0.4, CHIN, 0.55),
            (0.0, 6.62, 0.6),
            (0.56, 6.62, 0.6),
            (0.0, CHIN, 1.1),
            (0.34, CHIN + 0.04, 0.9),
            (0.0, 6.62, 1.17),
            (0.5, 6.62, 1.0),
        ]);
        h.hexa(on_side(s, mask), 0.04, BODY, Affine3A::IDENTITY);
        h.greeble(|h| {
            for x in [0.13, 0.25] {
                let z = 1.14 - 0.42 * x;
                h.cube(v(0.05, 0.2, 0.05), 0.0, FRAME, sided(s, place(v(x, 6.44, z), ry(0.4))));
            }
        });
    }
    h.block(v(0.36, 0.16, 0.32), v2(1.0, 0.8), Vec2::ZERO, 0.04, TRIM, at(0.0, CHIN + 0.02, 0.96));
    // The eyes: two sharp green slits in a dark visor under the brow.
    h.cube(v(1.0, 0.32, 0.4), 0.03, FRAME, at(0.0, 6.76, 0.88));
    let eye = [v2(0.07, -0.09), v2(0.42, -0.05), v2(0.49, 0.1), v2(0.1, 0.06)];
    for s in Side::BOTH {
        h.extrude(&eye, 0.06, EYE, sided(s, place(v(0.0, 6.76, 1.1), ry(0.12))));
    }
    // The helmet's cheek guards either side of the face, a round port in each.
    for s in Side::BOTH {
        let cheek = corners([
            (0.6, 6.32, -0.3),
            (0.8, 6.32, -0.3),
            (0.62, 6.96, -0.42),
            (0.88, 6.96, -0.42),
            (0.5, 6.32, 0.86),
            (0.78, 6.32, 0.74),
            (0.5, 6.96, 1.1),
            (0.86, 6.96, 1.0),
        ]);
        h.hexa(on_side(s, cheek), 0.05, BODY, Affine3A::IDENTITY);
        port(&mut h, 0.16, sided(s, place(v(0.86, 6.62, 0.2), ry(FRAC_PI_2))));
    }
    // The dome over it all, from the brow up.
    h.lathe(
        &[
            (0.0, BROW),
            (0.84, BROW),
            (0.87, BROW + 0.1),
            (0.85, 7.16),
            (0.77, 7.32),
            (0.6, 7.46),
            (0.34, CROWN - 0.02),
            (0.0, CROWN),
        ],
        24,
        BODY,
        axis * Affine3A::from_scale(v(1.0, 1.0, 1.08)),
    );
    // The brow: two plates meeting in a ridge over the eyes.
    for s in Side::BOTH {
        let brow = corners([
            (0.0, 6.88, 0.5),
            (0.66, 6.9, 0.5),
            (0.0, 7.06, 0.5),
            (0.62, 7.08, 0.5),
            (0.0, 6.88, 1.22),
            (0.66, 6.9, 0.98),
            (0.0, 7.06, 1.17),
            (0.62, 7.08, 0.95),
        ]);
        h.hexa(on_side(s, brow), 0.04, BODY, Affine3A::IDENTITY);
    }
    // The forehead's block, green, its lens, and the crest rising out of it.
    h.hexa(
        corners([
            (-0.2, 6.94, 0.75),
            (0.2, 6.94, 0.75),
            (-0.15, 7.34, 0.75),
            (0.15, 7.34, 0.75),
            (-0.2, 6.94, 1.27),
            (0.2, 6.94, 1.27),
            (-0.15, 7.34, 1.06),
            (0.15, 7.34, 1.06),
        ]),
        0.04,
        TRIM,
        Affine3A::IDENTITY,
    );
    h.cylinder(0.08, 0.06, 10, LENS, place(v(0.0, 7.08, 1.24), rx(1.1)));
    h.extrude(&CREST, 0.12, TRIM, place(v(0.0, 7.3, 1.0), ry(-FRAC_PI_2)));
    // The V-fin, its blades swept up and back out of the forehead.
    for s in Side::BOTH {
        h.extrude(&V_FIN, 0.09, ACCENT, sided(s, place(v(0.06, 7.1, 1.2), rx(-0.36))));
        // The dragon's crown: a horn swept back off each side of the helmet.
        h.extrude(&CROWN_HORN, 0.1, BODY, sided(s, place(v(0.8, 7.08, -0.05), ry(FRAC_PI_2 - 0.25))));
    }
    h.greeble(|h| {
        for s in Side::BOTH {
            slot(h, 0.3, sided(s, place(v(0.42, 7.2, -0.75), ry(PI - 0.5) * rx(-0.4))));
        }
    });
    d.sockets.eye = Designer::local(Bone::Head, CAMERA);
    d.head_base = CHIN + 0.04;
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half) behind its armour: (y, outer x, back z, front z) at its foot
/// and its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.3, 1.5, -1.25, 0.9), (5.9, 2.0, -1.45, 0.8)];

/// The armour on the chest's front (right half), each given by its foot and its top as (y, half
/// width, front z at the middle): the belly plate, and the green breastplate over it standing
/// forward at its foot and leaning back to the collar. Their fronts fall away outboard at
/// `CHEST_FALL` per metre, so the two halves meet in a crease down the middle.
const BELLY: [(f32, f32, f32); 2] = [(3.25, 1.3, 1.18), (4.3, 1.62, 1.4)];
const BREAST: [(f32, f32, f32); 2] = [(4.24, 1.98, 1.66), (5.9, 2.1, 1.12)];
const CHEST_FALL: f32 = 0.28;

/// A tier of the chest's front armour's corners (right half), its back at `back`.
fn tier([(y0, w0, f0), (y1, w1, f1)]: [(f32, f32, f32); 2], back: f32) -> [Vec3; 8] {
    [
        v(0.0, y0, back),
        v(w0, y0, back),
        v(0.0, y1, back),
        v(w1, y1, back),
        v(0.0, y0, f0),
        v(w0, y0, f0 - CHEST_FALL * w0),
        v(0.0, y1, f1),
        v(w1, y1, f1 - CHEST_FALL * w1),
    ]
}

/// A frame on a tier's front at (`x`, `y`), `lift` out of it: z out of the face.
fn on_tier(t: [(f32, f32, f32); 2], x: f32, y: f32, lift: f32) -> Affine3A {
    let [(y0, _, f0), (y1, _, f1)] = t;
    let rise = (f1 - f0) / (y1 - y0);
    let n = v(CHEST_FALL, -rise, 1.0).normalize();
    let p = v(x, y, f0 + rise * (y - y0) - CHEST_FALL * x);
    place(p + n * lift, Quat::from_rotation_arc(Vec3::Z, n))
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
        c.hexa(tier(BELLY, 0.3), 0.14, BODY, flat);
        c.hexa(tier(BREAST, 0.0), 0.16, TRIM, flat);
        // The chest's shoulder, white, a step back, standing up beside the head.
        c.hexa(
            corners([
                (1.05, 5.6, -0.9),
                (2.2, 5.6, -0.9),
                (1.1, 6.5, -0.85),
                (2.15, 6.5, -0.85),
                (1.05, 5.6, 1.18),
                (2.2, 5.6, 0.9),
                (1.1, 6.5, 0.62),
                (2.15, 6.5, 0.5),
            ]),
            0.14,
            BODY,
            flat,
        );
        // The intake high on the breastplate: a dark recess and yellow slats.
        louvre(&mut c, 0.82, 0.62, 4, ACCENT, sided(s, on_tier(BREAST, 1.12, 5.0, 0.0)));
        c.greeble(|c| {
            // The seam under the intake, and one down the breastplate's outer edge.
            c.cube(v(0.9, 0.05, 0.05), 0.0, FRAME, sided(s, on_tier(BREAST, 1.12, 4.55, 0.0)));
            c.cube(v(0.05, 1.1, 0.05), 0.0, FRAME, sided(s, on_tier(BREAST, 1.78, 5.05, 0.0)));
            c.cube(v(0.05, 0.55, 0.05), 0.0, FRAME, sided(s, on_tier(BELLY, 0.95, 3.75, 0.0)));
        });
        // The side blocks under the arms.
        let side = sided(s, place(v(2.05, 4.45, -0.15), rz(-0.06)));
        c.block(v(0.8, 2.0, 1.85), v2(0.9, 0.9), Vec2::ZERO, 0.16, BODY, side);
        slot(&mut c, 0.6, side * at(0.41, 0.1, 0.3) * Affine3A::from_rotation_y(FRAC_PI_2));
        // The shoulder's socket in the chest's side.
        c.cylinder(0.62, 0.5, 14, GREY, sided(s, place(v(2.45, SHOULDER.y, 0.0), rz(FRAC_PI_2))));
        // The back's side plates either side of the pack.
        let plate = sided(s, place(v(1.35, 4.55, -1.55), ry(-0.15)));
        c.block(v(0.95, 2.1, 0.4), v2(0.8, 0.85), Vec2::ZERO, 0.14, BODY, plate);
        slot(&mut c, 0.5, plate * at(0.12, 0.2, -0.21) * Affine3A::from_rotation_y(PI));
    }
    // The coin on the belly, where the breastplate's halves meet over it: a yellow disc, its
    // raised rim, the square hole through its middle.
    let coin = on_tier(BELLY, 0.0, 3.86, 0.0) * Affine3A::from_rotation_x(FRAC_PI_2);
    c.lathe(&[(0.0, -0.06), (0.42, -0.06), (0.44, 0.06), (0.4, 0.12), (0.0, 0.08)], 24, ACCENT, coin);
    c.lathe(&band(0.3, 0.38, 0.06, 0.14), 24, ACCENT, coin);
    c.cube(v(0.2, 0.12, 0.2), 0.02, FRAME, coin * at(0.0, 0.1, 0.0));
    c.greeble(|c| {
        // The seam across the belly under the coin.
        c.cube(v(2.2, 0.05, 0.05), 0.0, FRAME, on_tier(BELLY, 0.0, 3.42, 0.0));
    });
    // The yellow collar round the back and sides of the neck, two steps, and its tabs either
    // side of the throat.
    let (from, to) = (FRAC_PI_2 + 0.8, FRAC_PI_2 - 0.8 + TAU);
    let collar = at(0.0, 0.0, HEAD_Z - 0.1);
    c.lathe_arc(
        &[(0.62, 5.8), (1.12, 5.8), (1.08, 6.02), (0.62, 6.06), (0.62, 5.8)],
        24,
        from,
        to,
        ACCENT,
        collar,
    );
    c.lathe_arc(
        &[(0.62, 6.0), (0.92, 6.0), (0.86, 6.24), (0.62, 6.26), (0.62, 6.0)],
        24,
        from,
        to,
        ACCENT,
        collar,
    );
    for s in Side::BOTH {
        let tab = corners([
            (0.48, 5.72, 0.5),
            (1.0, 5.72, 0.5),
            (0.5, 6.04, 0.45),
            (0.95, 6.04, 0.45),
            (0.48, 5.72, 1.08),
            (1.0, 5.72, 1.02),
            (0.5, 6.04, 0.92),
            (0.95, 6.04, 0.86),
        ]);
        c.hexa(tab, 0.06, ACCENT, sided(s, Affine3A::IDENTITY));
    }

    // The abdomen: the grey frame, narrow, between the belly and the belt.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    a.lathe(
        &[(0.0, 2.4), (1.15, 2.4), (1.22, 2.55), (1.1, 3.4), (0.0, 3.4)],
        14,
        GREY,
        Affine3A::from_scale(v(1.0, 1.0, 0.86)),
    );
    a.greeble(|a| {
        for k in 0..3 {
            let y = 2.75 + k as f32 * 0.2;
            a.lathe(&band(1.12, 1.19, y, y + 0.07), 14, FRAME, Affine3A::from_scale(v(1.0, 1.0, 0.86)));
        }
    });
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.21);
    // The belt, narrow: a martial artist's waist.
    w.loft(
        &[hoop(1.98, 0.0, 0.0, 1.48, 1.1), hoop(2.08, 0.0, 0.0, 1.6, 1.2), hoop(2.62, 0.0, 0.0, 1.58, 1.18)],
        3.4,
        28,
        BODY,
        Affine3A::IDENTITY,
    );
    // The buckle: a yellow block at the belt's front with a boss in it.
    w.block(v(0.8, 0.56, 0.3), v2(0.86, 0.8), Vec2::ZERO, 0.06, ACCENT, at(0.0, 2.3, 1.24));
    w.cylinder(0.16, 0.1, 12, GUN, place(v(0.0, 2.3, 1.42), rx(FRAC_PI_2)));
    // The crotch block, and the green V standing on its front.
    w.block(v(1.1, 1.3, 1.45), v2(1.25, 1.0), v2(0.0, -0.05), 0.14, BODY, at(0.0, 1.38, 0.38));
    w.hexa(
        corners([
            (-0.14, 0.82, 1.0),
            (0.14, 0.82, 1.0),
            (-0.56, 1.98, 1.0),
            (0.56, 1.98, 1.0),
            (-0.14, 0.82, 1.18),
            (0.14, 0.82, 1.18),
            (-0.56, 1.98, 1.22),
            (0.56, 1.98, 1.22),
        ]),
        0.05,
        TRIM,
        Affine3A::IDENTITY,
    );
    for s in Side::BOTH {
        // The front skirt: a plate hung from the belt over the thigh, turned out round it, its
        // hem rising toward the outside; a green panel raised on its lower half, a seam across
        // above it, and two yellow studs at its top.
        let skirt = sided(s, place(v(1.04, 1.5, 1.36), ry(0.36) * rx(-0.14)));
        let plate = |x0: f32, x1: f32, top: f32, hem: (f32, f32), back: f32, front: (f32, f32)| {
            corners([
                (x0, hem.0, back),
                (x1, hem.1, back),
                (x0, top, back),
                (x1, top, back),
                (x0, hem.0, front.0),
                (x1, hem.1, front.0),
                (x0, top, front.1),
                (x1, top, front.1),
            ])
        };
        w.hexa(plate(-0.6, 0.6, 0.74, (-0.8, -0.52), -0.2, (0.2, 0.17)), 0.12, BODY, skirt);
        w.hexa(plate(-0.48, 0.44, -0.08, (-0.66, -0.44), 0.0, (0.3, 0.26)), 0.06, TRIM, skirt);
        w.greeble(|w| {
            w.cube(v(1.04, 0.05, 0.05), 0.0, FRAME, skirt * place(v(0.0, 0.18, 0.2), rx(-0.02)));
            for x in [-0.4, 0.4] {
                w.cylinder(0.08, 0.08, 10, ACCENT, skirt * place(v(x, 0.5, 0.2), rx(FRAC_PI_2)));
            }
        });
        // The hip's side plate under the belt, over the thigh's ball, a raised panel on it.
        let side = sided(s, place(v(2.08, 1.7, -0.12), rz(0.14)));
        w.block(v(0.5, 1.2, 1.9), v2(0.85, 0.9), Vec2::ZERO, 0.14, BODY, side);
        w.block(v(0.12, 0.9, 1.5), v2(1.0, 0.88), Vec2::ZERO, 0.04, TRIM, side * at(0.28, -0.05, 0.0));
        slot(&mut w, 0.42, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(-0.72, -0.05, 0.27));
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.86, 1.62, -1.34), rx(0.3)));
        w.block(v(1.45, 1.45, 0.5), v2(0.92, 0.85), v2(0.0, 0.05), 0.16, BODY, rear);
        slot(&mut w, 0.35, rear * at(0.2, 0.0, -0.26) * Affine3A::from_rotation_y(PI));
    }
}

// --- Arms. ---

/// The shoulder block's middle (right side): high and broad, its top above the head's chin.
const SHOULDER: Vec3 = Vec3::new(3.45, 5.18, 0.0);

/// The shoulder block, as hoops out along x from inside the chest's side, each as high as twice
/// its half width round the block's middle and as deep as twice its half depth: a squarish
/// block, its outer end stepped in.
const PAULDRON: [Hoop; 5] = [
    hoop(2.45, 0.0, 0.0, 0.82, 1.0),
    hoop(2.7, 0.0, 0.0, 1.02, 1.2),
    hoop(4.15, 0.0, 0.0, 1.06, 1.24),
    hoop(4.3, 0.0, 0.0, 1.06, 1.24),
    hoop(4.5, 0.0, 0.0, 0.9, 1.06),
];

/// The roof over each shoulder, as the front sees it (out along x from its inner end, up along
/// y): its eave turning up to a point at the outside, as a temple's does.
const EAVE: [Vec2; 9] = [
    Vec2::new(-1.05, 0.0),
    Vec2::new(0.55, 0.0),
    Vec2::new(1.0, 0.1),
    Vec2::new(1.32, 0.34),
    Vec2::new(1.5, 0.66),
    Vec2::new(1.32, 0.6),
    Vec2::new(0.95, 0.4),
    Vec2::new(0.5, 0.32),
    Vec2::new(-1.05, 0.32),
];

/// A cloud head, the sceptre's (across along x, up along y): three lobes over a point.
const RUYI: [Vec2; 14] = [
    Vec2::new(0.0, -0.5),
    Vec2::new(0.25, -0.28),
    Vec2::new(0.48, -0.05),
    Vec2::new(0.5, 0.15),
    Vec2::new(0.38, 0.3),
    Vec2::new(0.22, 0.22),
    Vec2::new(0.18, 0.42),
    Vec2::new(0.0, 0.5),
    Vec2::new(-0.18, 0.42),
    Vec2::new(-0.22, 0.22),
    Vec2::new(-0.38, 0.3),
    Vec2::new(-0.5, 0.15),
    Vec2::new(-0.48, -0.05),
    Vec2::new(-0.25, -0.28),
];

/// The left forearm's bracer, up from the wrist (x out, z forward): the cuff's flare, a step
/// in, then the box up to the elbow, rounding in over its barrel.
const BRACER: [Hoop; 6] = [
    hoop(0.0, 0.0, 0.05, 0.8, 0.84),
    hoop(0.36, 0.0, 0.05, 0.84, 0.88),
    hoop(0.36, 0.0, 0.05, 0.74, 0.78),
    hoop(1.15, 0.02, 0.04, 0.8, 0.84),
    hoop(1.6, 0.02, 0.02, 0.76, 0.78),
    hoop(1.92, 0.0, 0.0, 0.56, 0.58),
];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up) = s.pick((Bone::ShoulderL, Bone::UpperArmL), (Bone::ShoulderR, Bone::UpperArmR));
        let k = s.pick(0.0, 0.4);

        // The shoulder: a big squarish block, the green roof over it with its eave turned up at
        // the outside and a ridge along its top; a port in the block's outer end, raised panels
        // on its front and back, and the running light on its outer front corner: red on the
        // suit's own left (+x), green on its right.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        let block = sided(s, place(v(0.0, SHOULDER.y, 0.0), rz(-FRAC_PI_2)));
        o.loft(&PAULDRON, 3.8, 28, BODY, block);
        o.extrude(&EAVE, 2.7, TRIM, sided(s, at(SHOULDER.x, SHOULDER.y + 1.0, 0.0)));
        o.hexa(
            corners([
                (2.62, 6.48, -1.05),
                (4.2, 6.48, -1.05),
                (2.78, 6.78, -0.8),
                (3.98, 6.78, -0.8),
                (2.62, 6.48, 1.05),
                (4.2, 6.48, 1.05),
                (2.78, 6.78, 0.8),
                (3.98, 6.78, 0.8),
            ]),
            0.1,
            TRIM,
            sided(s, Affine3A::IDENTITY),
        );
        port(&mut o, 0.36, sided(s, place(v(4.5, 5.05, 0.15), ry(FRAC_PI_2))));
        // The front's panel, its lower inner corner cut.
        o.hexa(
            corners([
                (2.95, 4.3, 1.1),
                (4.02, 4.45, 1.1),
                (2.9, 5.85, 1.1),
                (4.02, 5.85, 1.1),
                (2.95, 4.3, 1.32),
                (4.02, 4.45, 1.32),
                (2.9, 5.85, 1.34),
                (4.02, 5.85, 1.34),
            ]),
            0.07,
            BODY,
            sided(s, Affine3A::IDENTITY),
        );
        // The same on its back, plainer.
        o.hexa(
            corners([
                (2.95, 4.45, -1.32),
                (4.02, 4.45, -1.32),
                (2.95, 5.85, -1.32),
                (4.02, 5.85, -1.32),
                (2.95, 4.45, -1.1),
                (4.02, 4.45, -1.1),
                (2.95, 5.85, -1.1),
                (4.02, 5.85, -1.1),
            ]),
            0.07,
            BODY,
            sided(s, Affine3A::IDENTITY),
        );
        o.greeble(|o| {
            // The seam round the block where its outer end steps in; the yellow cloud head on
            // the front panel, as on a sceptre's; slots on the back.
            o.cube(v(0.05, 1.9, 0.05), 0.0, FRAME, sided(s, at(4.22, 5.18, 1.25)));
            o.extrude(
                &RUYI,
                0.04,
                ACCENT,
                sided(s, at(3.5, 5.15, 1.35) * Affine3A::from_scale(Vec3::splat(0.85))),
            );
            for x in [3.2, 3.8] {
                slot(o, 0.8, sided(s, place(v(x, 5.15, -1.33), ry(PI))));
            }
        });
        let nav = sided(s, at(4.3, 5.92, 1.12));
        o.cube(v(0.34, 0.2, 0.24), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.0, 0.1) * Affine3A::from_scale(v(1.0, 0.7, 1.0)));
        o.cylinder(0.44, 0.7, 10, GREY, sided(s, place(v(2.3, SHOULDER.y - 0.3, 0.0), rz(FRAC_PI_2))));

        // The upper arm: a white drum with a green ring round it, on the grey frame, out from
        // under the shoulder.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.44, 1.7, 12, GREY, up_xf * at(0.0, 0.95, 0.0));
        u.lathe(
            &[(0.0, 0.5), (0.64, 0.5), (0.72, 0.6), (0.72, 1.42), (0.64, 1.5), (0.0, 1.5)],
            18,
            BODY,
            up_xf,
        );
        u.lathe(&band(0.7, 0.78, 0.88, 1.06), 18, TRIM, up_xf);
    }

    // The left forearm: the elbow's barrel, the bracer flaring to its cuff with the green guard
    // round its wrist and its studs, and the elbow's guard.
    let fo_xf = sided(Side::L, place(wr, toward(el - wr)));
    let mut f = d.on(Bone::ForearmL);
    f.seed(0.33);
    f.cylinder(0.56, 1.15, 14, GREY, sided(Side::L, place(el, rz(FRAC_PI_2))));
    f.loft(&BRACER, 3.4, 24, BODY, fo_xf);
    f.loft(
        &[
            hoop(0.44, 0.0, 0.05, 0.78, 0.82),
            hoop(0.5, 0.0, 0.05, 0.86, 0.9),
            hoop(0.86, 0.0, 0.05, 0.88, 0.92),
            hoop(0.92, 0.0, 0.05, 0.82, 0.86),
        ],
        3.4,
        24,
        TRIM,
        fo_xf,
    );
    f.greeble(|f| {
        for k in 0..3 {
            let y = 0.68;
            let x = -0.4 + k as f32 * 0.4;
            f.cylinder(0.07, 0.08, 8, ACCENT, fo_xf * place(v(x, y, 0.97), rx(FRAC_PI_2)));
            f.cylinder(0.07, 0.08, 8, ACCENT, fo_xf * place(v(0.93, y, x), rz(-FRAC_PI_2)));
        }
        f.cube(v(0.05, 0.08, 1.3), 0.0, FRAME, fo_xf * at(0.82, 1.35, 0.05));
    });
    let guard = fo_xf * at(0.72, 1.7, -0.3);
    f.block(v(0.34, 1.15, 1.15), v2(1.0, 0.55), v2(0.0, -0.25), 0.08, BODY, guard);
    f.cylinder(0.46, 0.3, 12, GREY, fo_xf);
    fist(d);
}

/// The left fist, grey, closed round the glaive.
fn fist(d: &mut Designer) {
    let s = Side::L;
    let w = j(Bone::HandR);
    let mut h = d.on(Bone::HandL);
    h.seed(0.4);
    h.cube(v(0.8, 1.0, 0.95), 0.08, GREY, sided(s, at(w.x, w.y - 0.45, w.z + 0.1)));
    h.greeble(|h| {
        for f in 0..4 {
            let z = w.z + 0.45 - f as f32 * 0.28;
            h.cube(v(0.3, 0.26, 0.24), 0.04, GREY, sided(s, place(v(w.x - 0.3, w.y - 1.0, z), rz(0.5))));
        }
        h.cube(
            v(0.26, 0.5, 0.26),
            0.04,
            GREY,
            sided(s, place(v(w.x - 0.2, w.y - 0.55, w.z + 0.62), rx(0.6))),
        );
    });
    h.cylinder(0.42, 0.4, 10, GREY, sided(s, at(w.x, w.y + 0.1, w.z)));
}

// --- The Talon: the dragon's neck down the right forearm, its head on the right hand. ---

/// The dragon's neck's rings, up the right forearm from the wrist: each one's foot (along the
/// forearm) and its half width and depth, wider toward the elbow, each one's flared foot lying
/// over the top of the one below, as a telescope's tubes do.
const NECK: [(f32, f32, f32); 5] =
    [(0.05, 0.6, 0.64), (0.41, 0.65, 0.69), (0.77, 0.7, 0.74), (1.13, 0.75, 0.79), (1.49, 0.8, 0.84)];

/// How much bigger the dragon's head is drawn than its sections give, about the wrist.
const TALON_SCALE: f32 = 1.15;
/// How far its jaw hangs open, radians.
const JAW_OPEN: f32 = 0.12;

/// The skull and upper jaw, as sections forward along z: (z, top, foot, half width).
const SKULL: [(f32, f32, f32, f32); 8] = [
    (0.55, 1.22, 0.62, 0.5),
    (0.85, 1.56, 0.46, 0.72),
    (1.5, 1.66, 0.45, 0.8),
    (2.2, 1.46, 0.5, 0.66),
    (2.9, 1.3, 0.53, 0.56),
    (3.5, 1.24, 0.56, 0.5),
    (3.86, 1.18, 0.6, 0.44),
    (4.02, 1.02, 0.66, 0.3),
];

/// The lower jaw, as sections forward along z: (z, top, foot, half width).
const JAW: [(f32, f32, f32, f32); 6] = [
    (0.9, 0.52, -0.05, 0.5),
    (1.5, 0.44, -0.3, 0.64),
    (2.4, 0.34, -0.18, 0.54),
    (3.3, 0.3, -0.06, 0.46),
    (3.75, 0.28, 0.02, 0.38),
    (3.92, 0.24, 0.08, 0.24),
];

/// Sections forward along z, as hoops for a loft placed by [`forward`].
fn sections(t: &[(f32, f32, f32, f32)]) -> Vec<Hoop> {
    t.iter().map(|&(z, top, foot, w)| hoop(z, 0.0, -(top + foot) * 0.5, w, (top - foot) * 0.5)).collect()
}

/// A loft's placing for [`sections`] at `x`: its axis forward along z.
fn forward(x: f32) -> Affine3A {
    place(v(x, 0.0, 0.0), rx(FRAC_PI_2))
}

/// A horn of the dragon's, in its own plane: back along x from where it leaves the skull, up
/// along y.
const HORN: [Vec2; 8] = [
    Vec2::new(0.0, -0.08),
    Vec2::new(0.35, -0.06),
    Vec2::new(0.95, 0.08),
    Vec2::new(1.55, 0.34),
    Vec2::new(2.05, 0.8),
    Vec2::new(1.42, 0.56),
    Vec2::new(0.8, 0.36),
    Vec2::new(0.2, 0.26),
];

/// The mane behind each cheek: three locks swept back (back along x, up along y).
const MANE: [Vec2; 8] = [
    Vec2::new(0.0, 0.05),
    Vec2::new(0.8, -0.18),
    Vec2::new(0.36, 0.26),
    Vec2::new(1.05, 0.36),
    Vec2::new(0.4, 0.6),
    Vec2::new(0.92, 0.98),
    Vec2::new(0.12, 0.84),
    Vec2::new(0.0, 0.74),
];

/// A dorsal fin down the neck (out along x from its back, up the forearm along y).
const NECK_FIN: [Vec2; 4] =
    [Vec2::new(0.0, 0.0), Vec2::new(0.14, 0.0), Vec2::new(0.46, 0.44), Vec2::new(0.0, 0.34)];

/// The beard under the chin (back along x, down along -y), in jags.
const BEARD: [Vec2; 7] = [
    Vec2::new(0.0, 0.05),
    Vec2::new(0.16, -0.36),
    Vec2::new(0.4, -0.08),
    Vec2::new(0.64, -0.44),
    Vec2::new(0.86, -0.1),
    Vec2::new(1.12, -0.36),
    Vec2::new(1.32, 0.05),
];

fn talon(d: &mut Designer) {
    let (el, wr) = (j(Bone::ForearmR), j(Bone::HandR));
    // The neck: the elbow's barrel, then green rings down to the wrist, a yellow fin down the
    // back of each; the grey drum the cable pays out of at the wrist.
    let fo_xf = place(wr, toward(el - wr));
    let mut f = d.on(Bone::ForearmR);
    f.seed(0.35);
    f.cylinder(0.56, 1.15, 14, GREY, place(el, rz(FRAC_PI_2)));
    for (y0, w, dd) in NECK {
        let flare = 0.07;
        f.loft(
            &[
                hoop(y0, 0.0, 0.0, w + flare, dd + flare),
                hoop(y0 + 0.09, 0.0, 0.0, w + flare, dd + flare),
                hoop(y0 + 0.14, 0.0, 0.0, w, dd),
                hoop(y0 + 0.46, 0.0, 0.0, w - 0.04, dd - 0.04),
            ],
            2.6,
            24,
            TRIM,
            fo_xf,
        );
        f.extrude(&NECK_FIN, 0.1, ACCENT, fo_xf * place(v(0.0, y0 + 0.14, -(dd - 0.06)), ry(FRAC_PI_2)));
    }
    f.cylinder(0.42, 0.3, 12, GREY, fo_xf);

    // The head, drawn about the wrist and scaled up from there: the skull and the upper jaw in
    // one, falling to the snout; heavy green brows over the eyes, a ridge down the snout to the
    // nostrils, white cheeks, the yellow horns swept back along the neck and the white mane
    // behind the cheeks; the fangs, the dark throat and the flamethrower's nozzle in it; and the
    // whiskers trailing back from the snout.
    let x = wr.x;
    let dragon = Affine3A::from_translation(wr)
        * Affine3A::from_scale(Vec3::splat(TALON_SCALE))
        * at(-wr.x, -wr.y, -wr.z);
    let mut h = d.on(Bone::HandR);
    h.seed(0.45);
    h.loft(&sections(&SKULL), 3.2, 28, TRIM, dragon * forward(x));
    for s in Side::BOTH {
        let flat = dragon * at(x, 0.0, 0.0) * sided(s, Affine3A::IDENTITY);
        h.hexa(
            corners([
                (0.3, 1.42, 1.1),
                (0.88, 1.34, 1.1),
                (0.3, 1.74, 1.1),
                (0.88, 1.62, 1.1),
                (0.3, 1.38, 2.2),
                (0.72, 1.32, 2.08),
                (0.3, 1.56, 2.24),
                (0.68, 1.48, 2.14),
            ]),
            0.06,
            TRIM,
            flat,
        );
        h.hexa(
            corners([
                (0.6, 0.52, 0.95),
                (0.86, 0.56, 0.95),
                (0.62, 1.24, 0.95),
                (0.84, 1.2, 0.95),
                (0.56, 0.56, 2.1),
                (0.72, 0.58, 2.0),
                (0.56, 1.1, 2.0),
                (0.7, 1.08, 1.95),
            ]),
            0.05,
            BODY,
            flat,
        );
        h.cube(v(0.08, 0.15, 0.4), 0.02, EYE, flat * place(v(0.72, 1.3, 1.9), ry(0.45)));
        h.extrude(&HORN, 0.14, ACCENT, flat * place(v(0.42, 1.5, 1.05), ry(FRAC_PI_2 - 0.35)));
        h.extrude(&MANE, 0.1, BODY, flat * place(v(0.62, 0.48, 1.05), ry(FRAC_PI_2 - 0.55)));
        // The fangs at the front corners of the mouth, and the teeth along it.
        h.block(
            v(0.13, 0.38, 0.14),
            v2(0.15, 0.15),
            Vec2::ZERO,
            0.0,
            IVORY,
            flat * place(v(0.3, 0.4, 3.72), rx(PI)),
        );
        h.greeble(|h| {
            for z in [2.55, 2.95, 3.3] {
                h.block(
                    v(0.08, 0.18, 0.09),
                    v2(0.2, 0.2),
                    Vec2::ZERO,
                    0.0,
                    IVORY,
                    flat * place(v(0.4, 0.45, z), rx(PI)),
                );
            }
            h.cylinder(0.09, 0.06, 8, FRAME, flat * place(v(0.2, 1.02, 4.04), rx(1.25)));
            // The whiskers, in three runs back from beside the nostrils.
            let runs = [v(0.42, 0.86, 3.7), v(0.8, 0.7, 3.1), v(1.08, 0.44, 2.4), v(1.24, 0.06, 1.7)];
            for (k, pair) in runs.windows(2).enumerate() {
                let (a, b) = (pair[0], pair[1]);
                let r = 0.065 - k as f32 * 0.012;
                h.cylinder(r, a.distance(b) + 0.04, 6, ACCENT, flat * place((a + b) * 0.5, toward(b - a)));
            }
        });
    }
    // The ridge down the snout, and the nose's pad at its end, the nostrils in its front.
    h.hexa(
        corners([
            (-0.22, 1.4, 2.1),
            (0.22, 1.4, 2.1),
            (-0.15, 1.58, 2.1),
            (0.15, 1.58, 2.1),
            (-0.2, 1.2, 3.5),
            (0.2, 1.2, 3.5),
            (-0.14, 1.34, 3.5),
            (0.14, 1.34, 3.5),
        ]),
        0.04,
        TRIM,
        dragon * at(x, 0.0, 0.0),
    );
    h.hexa(
        corners([
            (-0.46, 1.0, 3.4),
            (0.46, 1.0, 3.4),
            (-0.38, 1.36, 3.4),
            (0.38, 1.36, 3.4),
            (-0.4, 0.86, 4.08),
            (0.4, 0.86, 4.08),
            (-0.3, 1.16, 3.98),
            (0.3, 1.16, 3.98),
        ]),
        0.1,
        TRIM,
        dragon * at(x, 0.0, 0.0),
    );
    // The throat, dark, and the flamethrower's nozzle in it.
    h.cube(v(0.72, 0.4, 1.9), 0.03, FRAME, dragon * at(x, 0.36, 2.7));
    let nozzle_xf = dragon * place(v(x, 0.4, 3.25), rx(FRAC_PI_2));
    h.lathe(
        &[(0.1, -0.3), (0.13, -0.3), (0.2, 0.1), (0.22, 0.2), (0.17, 0.2), (0.1, -0.1)],
        14,
        GUN,
        nozzle_xf,
    );
    h.cylinder(0.11, 0.04, 10, THROAT, nozzle_xf);
    // The socket the cable leaves from, at the back of the skull.
    h.lathe(&band(0.2, 0.34, 0.0, 0.14), 12, GREY, dragon * place(v(x, 0.92, 0.56), rx(-FRAC_PI_2)));
    let snout = dragon.transform_point3(v(x, 0.95, 4.1));
    let mouth = dragon.transform_point3(v(x, 0.4, 3.48));
    d.sockets.talon = Some((Designer::local(Bone::HandR, snout), Vec3::Z));
    d.sockets.flame = Some((Designer::local(Bone::HandR, mouth), Vec3::Z));

    // The lower jaw, on the weapon's bone (the mouth fires), dropped open about its hinge: the
    // hinge, the jaw, the fangs up out of it, and the white beard under its chin.
    let hinge = v(x, 0.42, 1.2);
    let jaw = dragon
        * Affine3A::from_translation(hinge)
        * Affine3A::from_rotation_x(JAW_OPEN)
        * at(-x, -0.42, -1.2);
    let mut w = d.on(Bone::Weapon);
    w.seed(0.63);
    w.cylinder(0.22, 1.32, 12, GREY, jaw * place(hinge, rz(FRAC_PI_2)));
    w.loft(&sections(&JAW), 3.2, 24, TRIM, jaw * forward(x));
    for s in Side::BOTH {
        let flat = jaw * at(x, 0.0, 0.0) * sided(s, Affine3A::IDENTITY);
        w.block(v(0.11, 0.32, 0.12), v2(0.15, 0.15), Vec2::ZERO, 0.0, IVORY, flat * at(0.24, 0.4, 3.55));
        w.greeble(|w| {
            for z in [2.5, 2.9] {
                w.block(v(0.07, 0.16, 0.08), v2(0.2, 0.2), Vec2::ZERO, 0.0, IVORY, flat * at(0.36, 0.37, z));
            }
        });
    }
    w.extrude(&BEARD, 0.12, BODY, jaw * place(v(x, 0.0, 3.72), ry(FRAC_PI_2)));
    d.sockets.muzzle = Designer::local(Bone::Weapon, jaw.transform_point3(v(x, 0.3, 3.6)));
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): squarish, full,
/// closing in under the hip; the plates stand on it.
const THIGH: [Hoop; 5] = [
    hoop(0.45, 0.1, 0.05, 1.04, 1.02),
    hoop(1.1, 0.2, 0.08, 1.28, 1.18),
    hoop(2.2, 0.26, 0.06, 1.38, 1.24),
    hoop(3.1, 0.22, 0.0, 1.26, 1.14),
    hoop(3.62, 0.16, 0.0, 0.88, 0.8),
];

/// The lower leg's core, up its axis from the ankle to the knee (x out, z forward): slim over the
/// boot, swelling back and out to a full calf under the knee.
const SHIN: [Hoop; 8] = [
    hoop(0.7, 0.0, -0.02, 0.86, 0.86),
    hoop(1.6, 0.04, -0.08, 0.96, 0.98),
    hoop(2.7, 0.08, -0.2, 1.12, 1.16),
    hoop(3.7, 0.1, -0.3, 1.28, 1.3),
    hoop(4.4, 0.1, -0.32, 1.3, 1.3),
    hoop(4.95, 0.08, -0.26, 1.14, 1.1),
    hoop(5.35, 0.06, -0.2, 0.82, 0.78),
    hoop(5.55, 0.05, -0.18, 0.5, 0.5),
];

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

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

        // The thigh: the core on a grey ball at the hip, a front plate in two halves meeting in
        // a ridge, and the outer plate with a raised panel on it, a step between them.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.8, 6, GREY, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 3.4, 28, BODY, along_thigh);
        for half in [-1.0f32, 1.0] {
            let c = |x: f32, y: f32, z: f32| v(0.2 + half * x, y, z);
            let plate = [
                c(0.0, 0.7, 0.9),
                c(1.0, 0.75, 0.82),
                c(0.0, 3.2, 0.86),
                c(1.06, 3.15, 0.78),
                c(0.0, 0.7, 1.46),
                c(1.0, 0.75, 1.14),
                c(0.0, 3.2, 1.4),
                c(1.06, 3.15, 1.12),
            ];
            let plate = if half > 0.0 { plate } else { swap_x(plate) };
            th.hexa(plate, 0.12, BODY, along_thigh);
        }
        let outer = corners([
            (1.32, 0.95, -0.86),
            (1.74, 0.95, -0.86),
            (1.32, 3.3, -0.82),
            (1.7, 3.3, -0.82),
            (1.32, 0.95, 0.56),
            (1.74, 0.95, 0.5),
            (1.32, 3.3, 0.9),
            (1.7, 3.3, 0.84),
        ]);
        th.hexa(outer, 0.12, BODY, along_thigh);
        th.hexa(
            corners([
                (1.62, 1.4, -0.52),
                (1.84, 1.4, -0.52),
                (1.6, 2.85, -0.48),
                (1.8, 2.85, -0.48),
                (1.62, 1.4, 0.3),
                (1.84, 1.4, 0.26),
                (1.6, 2.85, 0.58),
                (1.8, 2.85, 0.54),
            ]),
            0.06,
            TRIM,
            along_thigh,
        );
        th.greeble(|th| {
            // The seam where the front plate meets the side, and slots on the plates.
            th.cube(v(0.05, 2.2, 0.05), 0.0, FRAME, along_thigh * place(v(1.3, 2.0, 1.04), ry(0.5)));
            slot(th, 0.5, along_thigh * place(v(1.75, 2.45, -0.66), ry(FRAC_PI_2)));
            th.cube(v(0.7, 0.05, 0.05), 0.0, FRAME, along_thigh * place(v(0.66, 1.4, 1.3), ry(-0.25)));
            // A seam round the back, and a slot under it.
            th.cube(v(1.5, 0.05, 0.05), 0.0, FRAME, along_thigh * at(0.24, 1.9, -1.2));
            slot(th, 0.6, along_thigh * place(v(0.6, 1.3, -1.12), ry(PI)));
        });

        // The knee: its grey barrel, and the green guard standing out of the shin's front, its
        // top tucked up under the thigh's plates, a ridge down its face and a wedge under it.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.64, 1.4, 12, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        let guard = sided(s, at(knee.x + 0.04, knee.y, knee.z));
        m.hexa(
            corners([
                (-0.66, -0.85, 0.45),
                (0.66, -0.85, 0.45),
                (-0.56, 0.8, 0.25),
                (0.56, 0.8, 0.25),
                (-0.62, -0.85, 1.44),
                (0.62, -0.85, 1.44),
                (-0.48, 0.8, 1.0),
                (0.48, 0.8, 1.0),
            ]),
            0.16,
            TRIM,
            guard,
        );
        m.hexa(
            corners([
                (-0.13, -0.78, 1.36),
                (0.13, -0.78, 1.36),
                (-0.09, 0.66, 1.0),
                (0.09, 0.66, 1.0),
                (-0.09, -0.8, 1.58),
                (0.09, -0.8, 1.58),
                (-0.05, 0.66, 1.12),
                (0.05, 0.66, 1.12),
            ]),
            0.03,
            TRIM,
            guard,
        );
        m.hexa(
            corners([
                (-0.16, -1.42, 0.62),
                (0.16, -1.42, 0.62),
                (-0.62, -0.85, 0.45),
                (0.62, -0.85, 0.45),
                (-0.1, -1.42, 1.18),
                (0.1, -1.42, 1.18),
                (-0.6, -0.85, 1.42),
                (0.6, -0.85, 1.42),
            ]),
            0.08,
            TRIM,
            guard,
        );

        // The lower leg: the core, the shin's guard down its front in two tiers, each in two
        // halves meeting in a ridge, the upper standing out over the lower's top; the green plate
        // on the calf's outside with its vents, white plates on its inside and down its back.
        m.loft(&SHIN, 3.4, 28, BODY, tube);
        let tiers = [
            [(1.0, 0.86, 0.45, 1.02, 0.78), (3.02, 0.96, 0.45, 1.12, 0.84)],
            [(2.9, 0.98, 0.5, 1.2, 0.9), (4.4, 1.06, 0.5, 1.26, 0.94)],
        ];
        for [(y0, w0, b0, r0, e0), (y1, w1, b1, r1, e1)] in tiers {
            for half in [-1.0f32, 1.0] {
                let c = |x: f32, y: f32, z: f32| v(0.06 + half * x, y, z);
                let plate = [
                    c(0.0, y0, b0),
                    c(w0, y0, b0 - 0.05),
                    c(0.0, y1, b1),
                    c(w1, y1, b1 - 0.05),
                    c(0.0, y0, r0),
                    c(w0, y0, e0),
                    c(0.0, y1, r1),
                    c(w1, y1, e1),
                ];
                let plate = if half > 0.0 { plate } else { swap_x(plate) };
                m.hexa(plate, 0.12, BODY, tube);
            }
        }
        let calf = corners([
            (1.02, 2.3, -1.25),
            (1.36, 2.3, -1.1),
            (1.08, 4.95, -1.45),
            (1.48, 4.9, -1.3),
            (1.02, 2.3, 0.3),
            (1.36, 2.3, 0.2),
            (1.08, 4.95, 0.62),
            (1.48, 4.9, 0.5),
        ]);
        m.hexa(calf, 0.14, TRIM, tube);
        louvre(&mut m, 0.6, 1.1, 4, GREY, tube * place(v(1.43, 3.75, -0.35), ry(FRAC_PI_2) * rz(-0.03)));
        let inner = corners([
            (-1.2, 2.6, -1.1),
            (-0.98, 2.6, -1.2),
            (-1.3, 4.7, -1.2),
            (-1.06, 4.7, -1.35),
            (-1.2, 2.6, 0.2),
            (-0.98, 2.6, 0.3),
            (-1.3, 4.7, 0.4),
            (-1.06, 4.7, 0.5),
        ]);
        m.hexa(inner, 0.1, BODY, tube);
        m.greeble(|m| {
            // The seams beside the shin's guard and across it, and slots up the calf's back.
            for x in [-0.98, 1.06] {
                m.cube(v(0.05, 2.6, 0.05), 0.0, FRAME, tube * place(v(x, 3.0, 0.62), ry(x.signum() * 0.9)));
            }
            m.cube(v(1.2, 0.05, 0.05), 0.0, FRAME, tube * at(0.06, 2.0, 1.08));
            for x in [-0.15, 0.35] {
                slot(m, 0.8, tube * place(v(x, 3.7, -1.7), ry(PI) * rx(-0.12)));
            }
            // A chevron engraved down the upper tier, either side of its ridge.
            for side in [-1.0f32, 1.0] {
                let face = place(v(0.06 + side * 0.5, 3.6, 1.09), ry(side * 0.3) * rz(-side * 0.7));
                m.cube(v(0.06, 0.7, 0.05), 0.0, FRAME, tube * face);
            }
        });
        // A plate down the calf's back.
        m.hexa(
            corners([
                (-0.45, 2.5, -1.48),
                (0.65, 2.5, -1.48),
                (-0.38, 4.6, -1.74),
                (0.58, 4.6, -1.74),
                (-0.45, 2.5, -1.2),
                (0.65, 2.5, -1.2),
                (-0.38, 4.6, -1.45),
                (0.58, 4.6, -1.45),
            ]),
            0.1,
            BODY,
            tube,
        );

        // The boot's shaft round the ankle: a squarish box over the foot of the shin, its tongue
        // up its front with a green strip on it, a grey band round its back and a grey disc
        // either side.
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
            3.8,
            24,
            BODY,
            flat,
        );
        let tongue = [
            (-1.02, y0 + 0.15, 0.55),
            (1.02, y0 + 0.15, 0.55),
            (-0.84, y1 + 0.3, 0.62),
            (0.84, y1 + 0.3, 0.62),
            (-1.02, y0 + 0.15, 1.8),
            (1.02, y0 + 0.15, 1.8),
            (-0.84, y1 + 0.3, 1.2),
            (0.84, y1 + 0.3, 1.2),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.2, BODY, flat);
        let strip = [
            (-0.3, y0 + 0.5, 1.5),
            (0.3, y0 + 0.5, 1.5),
            (-0.26, y1 + 0.2, 1.08),
            (0.26, y1 + 0.2, 1.08),
            (-0.3, y0 + 0.5, 1.82),
            (0.3, y0 + 0.5, 1.82),
            (-0.26, y1 + 0.2, 1.34),
            (0.26, y1 + 0.2, 1.34),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(strip, 0.05, TRIM, flat);
        let slope = v(0.0, y1 + 0.3 - (y0 + 0.15), 1.2 - 1.8).normalize();
        let face = sided(s, place(v(x, y1 - 0.05, 1.32), Quat::from_rotation_arc(Vec3::Y, slope)));
        m.greeble(|m| {
            m.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, face * at(0.0, -0.9, 0.04));
            m.cube(v(0.07, 0.6, 0.05), 0.0, FRAME, face * at(0.0, 0.0, 0.0));
        });
        let back = sided(s, at(x, 0.0, ankle.z - 0.1));
        m.lathe_arc(
            &band(1.02, 1.16, ankle.y - 0.3, ankle.y + 0.08),
            18,
            0.2,
            PI - 0.2,
            GREY,
            back * Affine3A::from_rotation_y(PI),
        );
        for (side, r) in [(1.0, 0.62), (-1.0, 0.5)] {
            let hub = sided(s, place(v(x + side * 1.16, ankle.y - 0.2, ankle.z - 0.12), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                16,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }

        // The foot: a thick platform, the heel high at its back, a grey band round its foot, and
        // the long toe (on its own hinge) falling away to its tip with a green block raised along
        // it; a dark sole under it all, split at the hinge, a vernier in each half.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, GREY, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        f.hexa(boot(x, base, (-7.5, -7.74), (1.28, 1.22), [-2.1, -1.86, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.38, 1.38), [-2.2, -2.18, -0.6, -0.6]),
            0.05,
            GREY,
            flat,
        );
        f.greeble(|f| {
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 3.2), 0.0, FRAME, sided(s, at(x + side * 1.16, -8.3, -0.3)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.14, -8.25, 1.26)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.05, -2.05), rx(-0.25))));
        });
        let heel = Sole { z: (-2.2, 1.26), half: (1.36, 1.28), top: 0.34, pads: GREY, vernier: 0.1, r: 0.42 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.8, -8.06), (1.24, 1.06), [1.32, 1.32, 3.4, 2.96]), 0.34, BODY, flat);
        t.hexa(boot(x, -8.1, (-7.62, -7.92), (0.58, 0.48), [1.36, 1.36, 2.84, 2.56]), 0.12, TRIM, flat);
        let toes = Sole { z: (1.34, 3.5), half: (1.28, 1.12), top: 0.34, pads: GREY, vernier: 2.3, r: 0.34 };
        sole(d, toe, s, x, toes);
    }
}

// --- The back: the pack and the dragon's tail. ---

/// The tail's spine, from its root in the pack's foot down to its tip, the segments between: it
/// hangs from the back, sweeping back from the body as it falls.
const TAIL: [Vec3; 6] = [
    Vec3::new(0.0, 3.2, -2.95),
    Vec3::new(0.0, 2.55, -3.5),
    Vec3::new(0.0, 1.88, -3.95),
    Vec3::new(0.0, 1.2, -4.28),
    Vec3::new(0.0, 0.62, -4.42),
    Vec3::new(0.0, 0.18, -4.52),
];

/// The spade at the tail's end (across along x, down along y).
const SPADE: [Vec2; 7] = [
    Vec2::new(-0.3, -0.1),
    Vec2::new(0.3, -0.1),
    Vec2::new(0.46, 0.12),
    Vec2::new(0.14, 0.3),
    Vec2::new(0.0, 0.46),
    Vec2::new(-0.14, 0.3),
    Vec2::new(-0.46, 0.12),
];

/// A fin down the back of a tail's segment (out along x, down the tail along y).
const TAIL_FIN: [Vec2; 4] =
    [Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.5), Vec2::new(0.42, 0.7), Vec2::new(0.12, 0.16)];

/// The spikes along the pack's top, the dragon's back (back along x, up along y).
const SPINE: [Vec2; 7] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(0.15, 0.42),
    Vec2::new(0.3, 0.1),
    Vec2::new(0.5, 0.52),
    Vec2::new(0.64, 0.1),
    Vec2::new(0.88, 0.6),
    Vec2::new(1.0, 0.0),
];

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    // The mount off the back, and the pack: a faceted block, narrower at its top, its back face
    // leaning in; green panels on its sides, the louvres in its back, and the spikes along its top.
    b.block(v(1.9, 1.9, 0.7), v2(0.9, 0.9), Vec2::ZERO, 0.12, BODY, at(0.0, 4.5, -1.85));
    b.hexa(
        corners([
            (-1.2, 3.0, -3.0),
            (1.2, 3.0, -3.0),
            (-1.0, 5.45, -2.85),
            (1.0, 5.45, -2.85),
            (-1.3, 2.95, -2.1),
            (1.3, 2.95, -2.1),
            (-1.1, 5.55, -2.1),
            (1.1, 5.55, -2.1),
        ]),
        0.2,
        BODY,
        Affine3A::IDENTITY,
    );
    for s in Side::BOTH {
        b.hexa(
            corners([
                (1.12, 3.3, -2.92),
                (1.4, 3.3, -2.92),
                (1.0, 5.1, -2.82),
                (1.24, 5.1, -2.82),
                (1.12, 3.3, -2.25),
                (1.4, 3.3, -2.25),
                (1.0, 5.1, -2.25),
                (1.24, 5.1, -2.25),
            ]),
            0.06,
            TRIM,
            sided(s, Affine3A::IDENTITY),
        );
    }
    louvre(&mut b, 1.3, 1.0, 5, GREY, place(v(0.0, 4.3, -2.93), ry(PI) * rx(0.06)));
    b.extrude(&SPINE, 0.16, ACCENT, place(v(0.0, 5.46, -2.15), ry(FRAC_PI_2)));
    b.greeble(|b| {
        for s in Side::BOTH {
            slot(b, 0.6, sided(s, place(v(0.8, 3.6, -2.99), ry(PI))));
        }
        b.cube(v(1.8, 0.05, 0.05), 0.0, FRAME, place(v(0.0, 5.1, -2.88), ry(PI) * rx(0.06)));
    });
    // The tail: a chain of segments tapering down from the pack's foot, each a white drum with a
    // flared green rim over the next and a yellow fin down its back, on a grey hinge at its root,
    // and the green spade at its end.
    b.cylinder(0.36, 1.0, 12, GREY, place(TAIL[0], rz(FRAC_PI_2)));
    for (k, pair) in TAIL.windows(2).enumerate() {
        let (a, c) = (pair[0], pair[1]);
        let len = a.distance(c);
        let r = 0.56 - 0.06 * k as f32;
        let seg = frame_along(a, c - a, Vec3::NEG_Z);
        b.loft(
            &[hoop(-0.1, 0.0, 0.0, r - 0.06, r - 0.08), hoop(len - 0.16, 0.0, 0.0, r, r - 0.02)],
            2.8,
            20,
            BODY,
            seg,
        );
        b.loft(
            &[hoop(len - 0.24, 0.0, 0.0, r + 0.11, r + 0.09), hoop(len, 0.0, 0.0, r + 0.07, r + 0.05)],
            2.8,
            20,
            TRIM,
            seg,
        );
        let fin = TAIL_FIN.map(|p| p * (r / 0.5));
        b.extrude(&fin, 0.1, ACCENT, seg * at(0.0, 0.0, r - 0.06) * Affine3A::from_rotation_y(-FRAC_PI_2));
    }
    let (a, c) = (TAIL[4], TAIL[5]);
    let tip = frame_along(c, c - a, Vec3::NEG_Z);
    b.extrude(&SPADE.map(|p| p * 1.15), 0.18, TRIM, tip);
    b.extrude(&SPADE.map(|p| p * 0.7), 0.28, TRIM, tip * at(0.0, -0.04, 0.0));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.82, 2.95, -2.6)), v(0.0, -0.55, -0.85), 0.42);
    }
}

// --- The beam glaive, in the left hand. ---

/// The way the glaive's staff points through the fist: its front blade forward and up.
const GLAIVE_DIR: Vec3 = Vec3::new(0.0, 0.62, 0.78);
/// How far each emitter's tip stands from the grip along the staff, back and front.
const GLAIVE_ENDS: (f32, f32) = (3.0, 3.05);

/// A prong either side of an emitter, the trident's (out along x, along the staff along y).
const PRONG: [Vec2; 6] = [
    Vec2::new(0.2, 0.0),
    Vec2::new(0.4, 0.0),
    Vec2::new(0.6, 0.3),
    Vec2::new(0.56, 0.78),
    Vec2::new(0.42, 0.4),
    Vec2::new(0.24, 0.24),
];

fn glaive(d: &mut Designer) {
    let wl = j(Bone::HandL);
    let dir = GLAIVE_DIR.normalize();
    let grip = v(wl.x - 0.05, wl.y - 0.55, wl.z + 0.2);
    let staff = place(grip, toward(dir));
    let mut g = d.on(Bone::HandL);
    g.seed(0.66);
    // The staff, its grip wrapped, its yellow collars, and the hexagonal shafts either side.
    let (back, front) = GLAIVE_ENDS;
    let len = back + front - 0.8;
    g.cylinder(0.15, len, 12, GUN, staff * at(0.0, (front - back) * 0.5, 0.0));
    g.cylinder(0.19, 1.3, 12, GREY, staff);
    g.greeble(|g| {
        for k in 0..5 {
            g.lathe(&band(0.18, 0.22, -0.5 + k as f32 * 0.24, -0.42 + k as f32 * 0.24), 12, FRAME, staff);
        }
    });
    // Each end alike, the lower turned round.
    for (turn, reach) in [(0.0, front), (PI, back)] {
        let flip = staff * Affine3A::from_rotation_z(turn);
        g.lathe(&band(0.14, 0.26, 0.66, 0.86), 12, ACCENT, flip);
        g.cylinder(0.21, 1.2, 6, STEEL, flip * at(0.0, 1.5, 0.0));
        g.lathe(&band(0.14, 0.27, 2.1, 2.28), 12, ACCENT, flip);
        // The emitter: a housing flaring to its head, a prong either side, its glowing tip.
        let head = flip * at(0.0, reach - 0.75, 0.0);
        g.lathe(
            &[(0.0, 0.0), (0.22, 0.0), (0.36, 0.25), (0.36, 0.5), (0.26, 0.62), (0.14, 0.74), (0.0, 0.74)],
            12,
            GUN,
            head,
        );
        for s in [1.0f32, -1.0] {
            g.extrude(&PRONG, 0.14, GUN, head * at(0.0, 0.32, 0.0) * Affine3A::from_scale(v(s, 1.0, 1.0)));
        }
        g.cylinder(0.1, 0.08, 8, THROAT, head * at(0.0, 0.76, 0.0));
    }
    let tip = grip + dir * front;
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, tip), dir));
    d.sockets.saber = (Designer::local(Bone::HandL, tip), dir);
}
