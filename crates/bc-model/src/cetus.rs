//! AL-09D Cetus, the Almagest's heavy drone and the Argo's stablemate: a heavy, rounded body
//! under high, broad shoulders, its small head set low between them, on long, heavy legs. Its
//! bulk is in rounded masses lofted through rounded sections ([`crate::kit::Builder::loft`]),
//! plated with faceted armour:
//!
//! - a small domed head sunk in its collar, the mono-eye on its rail behind a visor slit wrapping
//!   round the front: the slit's glass dark, a brown rail along its top and its foot, and the
//!   pink eye in a dark bezel on its brown carriage (the cockpit's camera); a peak swept back from
//!   a ridge shading the slit, a low grey crest along the dome, a faceted chin with its vents
//!   under the slit, a round sensor housing either side for an ear, and a rear camera;
//! - a rounded barrel of a chest, its prow two tiers meeting in a crease down the middle: a panel
//!   raised on each upper tier, a louvred vent in each lower, a round sensor at the top of the
//!   crease under the chin and a grey band round its foot; a collar round the back and sides of
//!   the head, and plates either side of the backpack;
//! - shoulders that are heavy boxes rounded at their edges, as high as the visor slit, on brown
//!   balls: a grey band round each near its outer end, a lid on its top sloping down outboard, a
//!   raised plate on its outer face with a round port (the unit number stencilled on the right
//!   one's, the insignia on the left's), and a running light high on its front;
//! - short round upper arms with a raised ring, heavy gauntlets with grey cuffs, a plate on their
//!   outer faces and an elbow guard, and brown fists;
//! - a narrow belt over the hips, the grey ring and its lens in the crotch, thick front skirts
//!   with a grey panel bolted to each, side skirts, and rear skirts each hooding a thruster;
//! - heavy legs plated over lofted cores: the thigh's front plate with a ridge down it, standing
//!   proud of a plate down its outside, and a plate on its back; a knee guard standing out of the
//!   shin with a wedge rising over it and a grey plate with four rivets on its face; the shin's
//!   front plate with its ridge, the calf swelling back and out under the knee, a plate stepping
//!   out of its outside and two tiers of plate down its back over a louvred vent; and boots: a
//!   rounded shaft over the ankle with a thick tongue up its front and a wheel either side, on a
//!   thick platform with a high heel and a long toe on its own hinge, a vernier in the sole under
//!   each.
//!
//! The backpack is a rounded box with a handle and louvred vents on its back, a grille and the
//! red beacon on its top, a banded propellant tank standing either side with a vernier on it, and
//! the main thrusters in their housing under it. The beam cannon is carried under the right arm:
//! the olive receiver over the fist with its grip and sight, the ribbed condenser drum riding on
//! its back and out over the wrist, cabled to a vented box shroud, the barrel with its cooling
//! rings, and the flared emitter round the bore. Held as [`crate::hold`] has it, the forearm lies
//! under the condenser at the ready, and carried muzzle down it hangs clear in front of the leg.
//! Four shield pods circle the suit (on [`Bone::Props`], which the client turns about
//! the upright axis): flat grey hexagonal plates, each with an olive panel raised round its field
//! generator, the lens ringed in pink, and an emitter at either point.
//!
//! The armour is olive and the trim grey; as on the Argo, the inner frame, the hands and the
//! shoulder balls are brown. The weapons are gunmetal and the slit's glass dark.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::detail::{GREY, LENS, band, port, slot, toward};
use crate::frames::{
    BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, TRIM, at, j, nozzle, place, rx, ry, rz, saber_hilt, sided,
    sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The inner frame, as the Argo's: neck, joints, shoulder balls, hands.
const BROWN: Paint = Paint::Fixed(paint::FRAME_BROWN);
/// The visor slit's glass, dark behind the mono-eye.
const SMOKE: Paint = Paint::Fixed(paint::GLASS);

pub(crate) fn cetus(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    arms(d);
    legs(d);
    backpack(d);
    cannon(d);
    saber_hilt(d);
    pods(d);
    stencils(d);
}

// --- Head. ---

/// The head's axis, upright through the neck, and how much wider than deep the helmet is.
const HEAD_Z: f32 = 0.12;
const HEAD_WIDE: f32 = 1.12;
/// Heights on the head: the jaw's foot inside the collar, the visor slit, and the dome's crown.
const JAW: f32 = 6.0;
const SLIT: (f32, f32) = (6.57, 6.93);
const CROWN: f32 = 7.84;
/// The helmet's radius round the slit, and the slit's back wall the rail runs along.
const HELM_R: f32 = 1.0;
const SLIT_BACK: f32 = 0.8;
/// Half the slit's sweep round the front, radians.
const SLIT_HALF: f32 = 1.15;
/// The mono-eye, at rest at the rail's middle: the cockpit's camera, where the simulation's eye is.
const EYE_AT: Vec3 = Vec3::new(0.0, 6.75, 1.1);

fn head(d: &mut Designer) {
    let axis = at(0.0, 0.0, HEAD_Z) * Affine3A::from_scale(v(HEAD_WIDE, 1.0, 1.0));
    let mut h = d.on(Bone::Head);
    h.seed(0.08);
    // The neck, brown, turning inside the collar.
    h.cylinder(0.5, 0.8, 12, BROWN, at(0.0, 5.85, HEAD_Z));
    // The jaw: the helmet's lower half, round so it turns in the collar, its lip standing out
    // under the slit.
    h.lathe(
        &[
            (0.0, JAW),
            (0.8, JAW),
            (0.9, JAW + 0.1),
            (0.97, 6.3),
            (HELM_R, SLIT.0 - 0.07),
            (HELM_R + 0.02, SLIT.0 - 0.02),
            (SLIT_BACK, SLIT.0),
            (0.0, SLIT.0),
        ],
        24,
        BODY,
        axis,
    );
    // The slit: the dark glass of its back wall, and the helmet's band round the sides and back.
    h.lathe(&[(0.0, SLIT.0), (SLIT_BACK, SLIT.0), (SLIT_BACK, SLIT.1), (0.0, SLIT.1)], 24, SMOKE, axis);
    let (front0, front1) = (FRAC_PI_2 - SLIT_HALF, FRAC_PI_2 + SLIT_HALF);
    h.lathe_arc(&band(SLIT_BACK, HELM_R, SLIT.0, SLIT.1), 24, front1, front0 + TAU, BODY, axis);
    // The rail the eye runs along, top and bottom of the slit, and the eye's carriage on it: a
    // brown block, a dark bezel and the pink lens.
    for y in [SLIT.0 + 0.06, SLIT.1 - 0.06] {
        h.lathe_arc(
            &band(SLIT_BACK - 0.02, SLIT_BACK + 0.07, y - 0.025, y + 0.025),
            16,
            front0,
            front1,
            BROWN,
            axis,
        );
    }
    let face = HEAD_Z + SLIT_BACK;
    h.cube(v(0.4, 0.3, 0.16), 0.03, BROWN, at(0.0, EYE_AT.y, face + 0.04));
    let lens = place(v(0.0, EYE_AT.y, face + 0.1), rx(FRAC_PI_2));
    h.lathe(&band(0.1, 0.17, -0.02, 0.06), 12, FRAME, lens);
    h.sphere(0.12, 6, EYE, lens * Affine3A::from_scale(v(1.0, 0.5, 1.0)));
    // The dome, its rim standing just proud of the slit.
    h.lathe(
        &[
            (0.0, SLIT.1),
            (HELM_R + 0.03, SLIT.1),
            (HELM_R + 0.05, SLIT.1 + 0.05),
            (HELM_R + 0.05, SLIT.1 + 0.14),
            (0.99, 7.22),
            (0.9, 7.44),
            (0.74, 7.62),
            (0.48, 7.77),
            (0.0, CROWN),
        ],
        24,
        BODY,
        axis,
    );
    // The peak: a wedge out of the dome's front shading the slit, flat underneath, its front
    // swept back from a ridge down its middle.
    for s in Side::BOTH {
        let peak = [
            (0.0, SLIT.1 - 0.01, 0.47),
            (0.85, SLIT.1 - 0.01, 0.47),
            (0.0, 7.5, 0.47),
            (0.8, 7.4, 0.47),
            (0.0, SLIT.1 - 0.01, 1.3),
            (0.78, SLIT.1 + 0.01, 1.1),
            (0.0, 7.12, 1.3),
            (0.78, 7.03, 1.1),
        ]
        .map(|(x, y, z)| v(x, y, z));
        h.hexa(peak, 0.05, BODY, sided(s, Affine3A::IDENTITY));
    }
    // The crest: a low fin along the dome's top from the brow to the back.
    let crest = [
        v2(1.0, 7.16),
        v2(0.8, 7.6),
        v2(0.25, 7.96),
        v2(-0.45, 7.96),
        v2(-0.85, 7.66),
        v2(-0.8, 7.4),
        v2(-0.2, 7.7),
        v2(0.5, 7.5),
        v2(0.85, 7.08),
    ];
    h.extrude(&crest, 0.16, TRIM, place(v(0.0, 0.0, HEAD_Z), ry(-FRAC_PI_2)));
    // The chin under the slit: a faceted muzzle with its vents.
    let chin = [
        (-0.5, 6.12, 0.55),
        (0.5, 6.12, 0.55),
        (-0.56, SLIT.0 - 0.03, 0.6),
        (0.56, SLIT.0 - 0.03, 0.6),
        (-0.36, 6.12, 1.08),
        (0.36, 6.12, 1.08),
        (-0.48, SLIT.0 - 0.03, 1.2),
        (0.48, SLIT.0 - 0.03, 1.2),
    ]
    .map(|(x, y, z)| v(x, y, z));
    h.hexa(chin, 0.06, BODY, Affine3A::IDENTITY);
    h.greeble(|h| {
        for k in 0..3 {
            let y = 6.24 + k as f32 * 0.1;
            h.cube(v(0.6, 0.04, 0.05), 0.0, FRAME, place(v(0.0, y, 1.1 + (y - 6.12) * 0.27), rx(-0.27)));
        }
    });
    // The ears: a round sensor housing either side, behind the slit's ends, and a vent slot in
    // the dome behind each.
    for s in Side::BOTH {
        let x = HELM_R * HEAD_WIDE - 0.05;
        let ear = sided(s, place(v(x, 6.72, HEAD_Z - 0.12), rz(-FRAC_PI_2)));
        h.lathe(&[(0.0, 0.0), (0.44, 0.0), (0.44, 0.12), (0.36, 0.2), (0.0, 0.2)], 14, BODY, ear);
        port(&mut h, 0.24, sided(s, place(v(x + 0.2, 6.72, HEAD_Z - 0.12), ry(FRAC_PI_2))));
        slot(&mut h, 0.4, sided(s, place(v(0.62, 7.3, HEAD_Z - 0.52), ry(2.3) * rx(-0.5))));
    }
    // The rear camera, in a housing low on the back of the dome.
    let rear = place(v(0.0, 7.12, HEAD_Z - 1.0), ry(PI));
    h.block(v(0.5, 0.36, 0.3), v2(0.85, 0.8), Vec2::ZERO, 0.05, BODY, rear);
    port(&mut h, 0.13, rear * at(0.0, 0.0, 0.15));
    d.sockets.eye = Designer::local(Bone::Head, EYE_AT);
    d.head_base = 6.3;
}

// --- Chest, abdomen and back. ---

/// The chest's core, a rounded barrel from the abdomen to the collar.
const CORE: [Hoop; 6] = [
    hoop(3.2, 0.0, -0.15, 1.95, 1.4),
    hoop(3.6, 0.0, -0.15, 2.3, 1.6),
    hoop(4.6, 0.0, -0.2, 2.5, 1.72),
    hoop(5.45, 0.0, -0.25, 2.42, 1.66),
    hoop(5.95, 0.0, -0.3, 2.05, 1.45),
    hoop(6.12, 0.0, -0.3, 1.5, 1.1),
];

/// The chest's prow (right half), two tiers meeting in a crease down the middle, each given by
/// its foot and top as (y, outer x, back z, front z at the middle); the front falls away
/// outboard at `CHEST_FALL` per metre.
const PROW: [[(f32, f32, f32, f32); 2]; 2] =
    [[(3.4, 1.8, 0.2, 1.5), (4.55, 2.2, 0.2, 1.95)], [(4.55, 2.2, 0.2, 1.95), (5.85, 1.9, 0.2, 1.45)]];
const CHEST_FALL: f32 = 0.3;

/// A prow tier's corners.
fn prow_tier([(y0, x0, b0, f0), (y1, x1, b1, f1)]: [(f32, f32, f32, f32); 2]) -> [Vec3; 8] {
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

/// The prow's front at height `y`, `x` out (on the right), and its outward normal.
fn prow_front(x: f32, y: f32) -> (Vec3, Vec3) {
    let tier = PROW.iter().find(|t| y <= t[1].0).unwrap_or(&PROW[1]);
    let [(y0, _, _, f0), (y1, _, _, f1)] = *tier;
    let rise = (f1 - f0) / (y1 - y0);
    (v(x, y, f0 + rise * (y - y0) - CHEST_FALL * x), v(CHEST_FALL, -rise, 1.0).normalize())
}

/// A frame on the prow's face at (`x`, `y`), `lift` out of it: z out of the face, y up it.
fn on_prow(x: f32, y: f32, lift: f32) -> Affine3A {
    let (p, n) = prow_front(x, y);
    let up = (Vec3::Y - n * n.y).normalize();
    Affine3A::from_mat3_translation(glam::Mat3::from_cols(up.cross(n), up, n), p + n * lift)
}

/// The shoulder ball's centre (right side), high beside the collar.
const SHOULDER: Vec3 = Vec3::new(3.0, 5.2, 0.0);

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    c.loft(&CORE, 3.0, 28, BODY, Affine3A::IDENTITY);
    for s in Side::BOTH {
        let flat = sided(s, Affine3A::IDENTITY);
        for tier in PROW {
            c.hexa(prow_tier(tier), 0.12, BODY, flat);
        }
        // A panel raised on the upper tier, following its face, and the vent in the lower: a dark
        // recess and four grey slats.
        let corner = |x: f32, y: f32, out: f32| {
            let (p, n) = prow_front(x, y);
            p + n * out
        };
        let (y0, y1) = (4.72, 5.62);
        let panel = [
            corner(0.5, y0, -0.06),
            corner(1.85, y0, -0.06),
            corner(0.45, y1, -0.06),
            corner(1.62, y1, -0.06),
            corner(0.5, y0, 0.1),
            corner(1.85, y0, 0.1),
            corner(0.45, y1, 0.1),
            corner(1.62, y1, 0.1),
        ];
        c.hexa(panel, 0.04, BODY, flat);
        let vent = sided(s, on_prow(1.0, 3.95, 0.0));
        c.cube(v(1.05, 0.8, 0.14), 0.02, FRAME, vent);
        for k in 0..4 {
            let y = -0.28 + k as f32 * 0.19;
            c.cube(v(0.95, 0.09, 0.16), 0.0, TRIM, vent * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.4));
        }
        c.greeble(|c| {
            c.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, sided(s, on_prow(1.9, 5.0, 0.0)));
            c.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, sided(s, on_prow(1.05, 4.6, 0.0)));
        });
        // The shoulder's socket in the chest's side.
        c.cylinder(0.75, 0.6, 14, BROWN, sided(s, place(v(2.45, SHOULDER.y, 0.0), rz(FRAC_PI_2))));
        // The back's side plates, either side of the backpack.
        let plate = sided(s, place(v(1.65, 4.5, -1.75), ry(-0.25)));
        c.block(v(1.0, 2.0, 0.4), v2(0.8, 0.85), Vec2::ZERO, 0.14, BODY, plate);
        slot(&mut c, 0.5, plate * at(0.15, 0.2, -0.21) * Affine3A::from_rotation_y(PI));
    }
    // The sensor at the top of the crease, in its housing, and the grey band round the chest's
    // foot under the prow.
    let sensor = on_prow(0.0, 5.45, 0.0);
    c.block(v(0.62, 0.56, 0.4), v2(0.85, 0.85), Vec2::ZERO, 0.06, BODY, sensor);
    port(&mut c, 0.22, sensor * at(0.0, 0.0, 0.2));
    c.loft(
        &[hoop(3.12, 0.0, -0.15, 2.04, 1.5), hoop(3.42, 0.0, -0.15, 2.08, 1.54)],
        3.0,
        28,
        TRIM,
        Affine3A::IDENTITY,
    );
    // The collar round the back and sides of the head.
    let collar = at(0.0, 0.0, HEAD_Z);
    let (from, to) = (FRAC_PI_2 + 1.0, FRAC_PI_2 - 1.0 + TAU);
    c.lathe_arc(
        &[(1.2, 5.9), (1.62, 5.9), (1.62, 6.25), (1.5, 6.48), (1.3, 6.5), (1.2, 6.4), (1.2, 5.9)],
        28,
        from,
        to,
        BODY,
        collar,
    );

    // The abdomen: the brown frame, and two olive hoops round it.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.84));
    a.lathe(&[(0.0, 2.4), (1.45, 2.4), (1.5, 2.55), (1.42, 3.35), (0.0, 3.35)], 14, BROWN, squash);
    for y in [2.6, 2.94] {
        a.lathe(
            &[
                (1.4, y),
                (1.62, y),
                (1.67, y + 0.05),
                (1.67, y + 0.2),
                (1.62, y + 0.25),
                (1.4, y + 0.25),
                (1.4, y),
            ],
            22,
            BODY,
            squash,
        );
    }
}

// --- Waist and skirts. ---

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.21);
    // The belt, narrow under the chest, and the crotch block under its front with the grey ring
    // and its lens.
    w.loft(
        &[hoop(1.95, 0.0, 0.0, 1.7, 1.4), hoop(2.05, 0.0, 0.0, 1.8, 1.5), hoop(2.62, 0.0, 0.0, 1.8, 1.5)],
        3.4,
        28,
        BODY,
        Affine3A::IDENTITY,
    );
    w.block(v(1.25, 1.35, 1.6), v2(1.25, 1.0), v2(0.0, -0.05), 0.14, BODY, at(0.0, 1.4, 0.4));
    let ring = place(v(0.0, 1.2, 1.18), rx(FRAC_PI_2));
    w.lathe(&band(0.26, 0.46, -0.05, 0.28), 18, TRIM, ring);
    w.lathe(&[(0.0, -0.05), (0.27, -0.05), (0.25, 0.1), (0.14, 0.16), (0.0, 0.17)], 16, LENS, ring);
    w.greeble(|w| {
        w.cube(v(1.2, 0.06, 0.05), 0.0, FRAME, at(0.0, 2.3, 1.5));
    });
    for s in Side::BOTH {
        // The front skirts: thick plates hung from the belt and turned out over the thighs, their
        // hems rising outboard, a grey panel raised on each with two bolts.
        let front = sided(s, place(v(1.18, 1.45, 1.4), ry(0.32) * rx(-0.14)));
        let plate = [
            (-0.75, -0.95, -0.25),
            (0.75, -0.6, -0.25),
            (-0.75, 0.75, -0.25),
            (0.75, 0.75, -0.25),
            (-0.75, -0.95, 0.25),
            (0.75, -0.6, 0.25),
            (-0.75, 0.75, 0.25),
            (0.75, 0.75, 0.25),
        ]
        .map(|(x, y, z)| v(x, y, z));
        w.hexa(plate, 0.16, BODY, front);
        let panel = front * at(-0.1, 0.05, 0.28);
        w.block(v(0.8, 0.95, 0.12), v2(0.85, 1.0), Vec2::ZERO, 0.03, TRIM, panel);
        w.greeble(|w| {
            for x in [-0.22, 0.22] {
                w.cylinder(0.07, 0.06, 8, GREY, panel * place(v(x, 0.32, 0.07), rx(FRAC_PI_2)));
            }
        });
        // The side skirts over the hips, flaring at the hem.
        let side = sided(s, place(v(2.2, 1.65, -0.1), rz(0.14)));
        w.block(v(0.6, 1.5, 2.0), v2(0.85, 0.9), Vec2::ZERO, 0.16, BODY, side);
        slot(&mut w, 0.5, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, -0.1, 0.31));
        slot(&mut w, 0.5, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.35, -0.1, 0.31));
        // The rear skirts, each hooding a thruster that blows down and back.
        let rear = sided(s, place(v(0.95, 1.65, -1.52), rx(0.3)));
        w.block(v(1.55, 1.45, 0.75), v2(0.92, 0.85), v2(0.0, 0.05), 0.18, BODY, rear);
        slot(&mut w, 0.4, rear * at(0.2, 0.0, -0.38) * Affine3A::from_rotation_y(PI));
        bell(&mut w, v(0.95, 0.85, -1.75), v(0.0, -0.4, -1.0), 0.48, sided(s, Affine3A::IDENTITY));
    }
}

// --- Arms. ---

/// The shoulder's block, hoops out along x from inside the chest's side: a heavy box rounded at
/// its edges, stepping in to its outer face.
const SHELL: [Hoop; 6] = [
    hoop(2.55, 0.0, 0.0, 1.2, 1.38),
    hoop(2.95, -0.04, 0.0, 1.42, 1.58),
    hoop(4.7, -0.04, 0.0, 1.42, 1.58),
    hoop(5.15, -0.02, 0.0, 1.32, 1.48),
    hoop(5.42, 0.0, 0.0, 1.1, 1.26),
    hoop(5.56, 0.0, 0.0, 0.8, 0.94),
];
/// The height of the block's middle: its top stands as high as the visor slit.
const SHELL_Y: f32 = 5.45;

/// The gauntlet, up the forearm from the wrist: the grey cuff's flare, then the heavy box up to
/// the elbow, rounding in over its barrel.
const CUFF: [Hoop; 2] = [hoop(0.0, 0.0, 0.05, 0.95, 1.0), hoop(0.42, 0.0, 0.05, 1.0, 1.05)];
const GAUNTLET: [Hoop; 4] = [
    hoop(0.4, 0.0, 0.05, 0.88, 0.94),
    hoop(1.2, 0.03, 0.03, 0.94, 1.0),
    hoop(1.7, 0.03, 0.0, 0.86, 0.9),
    hoop(2.0, 0.0, 0.0, 0.6, 0.62),
];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (shoulder, up, fore) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR),
        );
        let k = s.pick(0.0, 0.4);

        // The brown ball, and the block over it: a grey band round it near its outer end, a lid
        // on its top sloping down outboard, and a raised plate on its outer face with a round port.
        let mut o = d.on(shoulder);
        o.seed(0.24 + k);
        o.sphere(1.05, 10, BROWN, sided(s, Affine3A::from_translation(SHOULDER)));
        let shell = sided(s, place(v(0.0, SHELL_Y, 0.0), rz(-FRAC_PI_2)));
        o.loft(&SHELL, 5.0, 28, BODY, shell);
        o.loft(
            &[hoop(4.2, -0.04, 0.0, 1.47, 1.63), hoop(4.48, -0.04, 0.0, 1.47, 1.63)],
            5.0,
            28,
            TRIM,
            shell,
        );
        let lid = [
            (2.95, 6.82, -1.42),
            (5.05, 6.82, -1.42),
            (3.05, 7.1, -1.3),
            (4.95, 6.98, -1.3),
            (2.95, 6.82, 1.42),
            (5.05, 6.82, 1.42),
            (3.05, 7.1, 1.3),
            (4.95, 6.98, 1.3),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(lid, 0.1, BODY, sided(s, Affine3A::IDENTITY));
        let outer = [
            (5.4, 4.6, -1.0),
            (5.62, 4.6, -0.96),
            (5.4, 6.3, -1.0),
            (5.62, 6.3, -0.96),
            (5.4, 4.6, 1.0),
            (5.62, 4.6, 0.96),
            (5.4, 6.3, 1.0),
            (5.62, 6.3, 0.96),
        ]
        .map(|(x, y, z)| v(x, y, z));
        o.hexa(outer, 0.1, BODY, sided(s, Affine3A::IDENTITY));
        port(&mut o, 0.24, sided(s, place(v(5.63, 5.95, 0.6), ry(FRAC_PI_2))));
        slot(&mut o, 0.6, sided(s, place(v(5.63, 5.1, -0.78), ry(FRAC_PI_2))));
        slot(&mut o, 0.6, sided(s, place(v(3.6, 5.6, 1.59), Quat::IDENTITY)));
        // A running light high on its front: red on the suit's own left (+x), green on its right.
        let nav = sided(s, at(4.85, 6.62, 1.5));
        o.cube(v(0.36, 0.18, 0.22), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));

        // The upper arm: a drum with a raised ring, on the brown frame.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.cylinder(0.46, 1.7, 12, BROWN, up_xf * at(0.0, 0.95, 0.0));
        u.lathe(
            &[
                (0.0, 0.45),
                (0.7, 0.45),
                (0.8, 0.55),
                (0.8, 0.8),
                (0.88, 0.86),
                (0.88, 1.06),
                (0.8, 1.12),
                (0.8, 1.35),
                (0.7, 1.45),
                (0.0, 1.45),
            ],
            18,
            BODY,
            up_xf,
        );

        // The gauntlet: the elbow's barrel, the heavy box, an elbow guard over the joint.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.58, 1.2, 14, BROWN, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&CUFF, 3.4, 24, TRIM, fo_xf);
        f.loft(&GAUNTLET, 3.4, 24, BODY, fo_xf);
        // A plate raised on its outer face, and a dark slot down its front.
        let outer = [
            (0.86, 0.55, -0.6),
            (1.06, 0.55, -0.6),
            (0.86, 1.6, -0.55),
            (1.04, 1.6, -0.55),
            (0.86, 0.55, 0.62),
            (1.06, 0.55, 0.62),
            (0.86, 1.6, 0.55),
            (1.04, 1.6, 0.55),
        ]
        .map(|(x, y, z)| v(x, y, z));
        f.hexa(outer, 0.06, BODY, fo_xf);
        f.cube(v(0.36, 0.7, 0.08), 0.02, FRAME, fo_xf * at(0.0, 1.05, 0.99));
        let guard = fo_xf * at(0.9, 1.75, -0.2);
        f.block(v(0.36, 1.3, 1.3), v2(1.0, 0.6), v2(0.0, -0.22), 0.08, BODY, guard);
        slot(&mut f, 0.45, guard * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.1, -0.1, 0.19));
        f.greeble(|f| {
            f.cube(v(1.3, 0.08, 0.05), 0.0, FRAME, fo_xf * at(0.0, 0.66, 0.98));
        });
        f.cylinder(0.48, 0.3, 12, BROWN, fo_xf);
    }
    crate::frames::hands(d, BROWN);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): a heavy rounded
/// block from over the knee guard to under the skirts.
const THIGH: [Hoop; 5] = [
    hoop(0.5, 0.1, -0.1, 1.0, 0.95),
    hoop(1.0, 0.15, -0.08, 1.28, 1.18),
    hoop(2.7, 0.2, -0.05, 1.38, 1.26),
    hoop(3.4, 0.15, -0.1, 1.22, 1.12),
    hoop(3.85, 0.1, -0.1, 0.82, 0.75),
];

/// The lower leg's core, up its axis from the ankle to the knee: slim over the boot, swelling
/// back and out to the calf's widest just under the knee, closing in behind the knee guard.
const SHIN: [Hoop; 8] = [
    hoop(0.6, 0.0, -0.05, 0.92, 0.95),
    hoop(1.5, 0.03, -0.1, 1.04, 1.08),
    hoop(2.6, 0.08, -0.26, 1.26, 1.32),
    hoop(3.6, 0.12, -0.4, 1.44, 1.48),
    hoop(4.3, 0.12, -0.42, 1.46, 1.48),
    hoop(4.9, 0.1, -0.36, 1.3, 1.3),
    hoop(5.3, 0.06, -0.28, 0.98, 0.95),
    hoop(5.55, 0.04, -0.25, 0.6, 0.6),
];

/// A ridged plate's section across at one height: (y, half width, front z at the ridge).
type Span = (f32, f32, f32);

/// Half of a plate with a ridge down its middle (at x 0), out to its edge: its foot and its
/// top as (y, half width, front z at the ridge), its back flat at `back`; its front falls away
/// toward the edge at `fall` per metre. Every face stays flat.
fn ridged(foot: Span, top: Span, back: f32, fall: f32) -> [Vec3; 8] {
    let ((y0, w0, f0), (y1, w1, f1)) = (foot, top);
    [
        v(0.0, y0, back),
        v(w0, y0, back),
        v(0.0, y1, back),
        v(w1, y1, back),
        v(0.0, y0, f0),
        v(w0, y0, f0 - fall * w0),
        v(0.0, y1, f1),
        v(w1, y1, f1 - fall * w1),
    ]
}

/// Mirrors across a limb's own middle (its local x).
fn flip() -> Affine3A {
    Affine3A::from_scale(v(-1.0, 1.0, 1.0))
}

/// The thigh's front plate (each half, outer then inner, from the ridge), and how its face
/// falls away from the ridge.
const THIGH_PLATE: [[Span; 2]; 2] =
    [[(0.95, 1.25, 1.56), (3.25, 1.3, 1.6)], [(0.95, 1.12, 1.56), (3.25, 1.18, 1.6)]];
const THIGH_FALL: f32 = 0.26;

/// The shin's front plate (each half, outer then inner), up to its top under the knee guard.
const SHIN_PLATE: [[Span; 2]; 2] =
    [[(1.0, 0.86, 1.18), (4.35, 1.06, 1.3)], [(1.0, 0.78, 1.18), (4.35, 0.96, 1.3)]];
const SHIN_FALL: f32 = 0.3;
/// How the calf's back plate falls away from its ridge.
const CALF_FALL: f32 = 0.4;

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);

        // The thigh: the brown ball at the hip, the heavy core, and its armour: a front plate
        // with a ridge down its middle, standing proud of a side plate down its outside.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.85, 6, BROWN, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 4.0, 28, BODY, along_thigh);
        let ridge = along_thigh * at(0.2, 0.0, 0.0);
        for (half, [foot, top]) in THIGH_PLATE.into_iter().enumerate() {
            let xf = if half == 0 { ridge } else { ridge * flip() };
            th.hexa(ridged(foot, top, 0.45, THIGH_FALL), 0.16, BODY, xf);
        }
        let side = [
            (1.42, 1.05, -1.0),
            (1.74, 1.05, -0.96),
            (1.42, 3.15, -0.95),
            (1.72, 3.15, -0.92),
            (1.42, 1.05, 0.72),
            (1.74, 1.05, 0.62),
            (1.42, 3.15, 0.85),
            (1.72, 3.15, 0.76),
        ]
        .map(|(x, y, z)| v(x, y, z));
        th.hexa(side, 0.1, BODY, along_thigh);
        // A plate on its back, under the rear skirt.
        let back = along_thigh * at(0.2, 0.0, -0.05) * Affine3A::from_rotation_y(PI);
        for (half, w) in [(0, 1.0), (1, 1.05)] {
            let xf = if half == 0 { back } else { back * flip() };
            th.hexa(ridged((0.85, w, 1.36), (3.0, w * 1.05, 1.4), 0.6, THIGH_FALL), 0.1, BODY, xf);
        }
        slot(&mut th, 0.7, along_thigh * place(v(1.75, 2.1, -0.3), ry(FRAC_PI_2)));
        th.greeble(|th| {
            // A seam across the front plate, and bolts at its top corners.
            for (half, w) in [(0, 1.0), (1, 0.9)] {
                let xf = if half == 0 { ridge } else { ridge * flip() };
                let run = v(w, 0.0, -THIGH_FALL * w);
                th.cube(
                    v(run.length(), 0.06, 0.05),
                    0.0,
                    FRAME,
                    xf * place(v(w * 0.5, 2.35, 1.59 - THIGH_FALL * w * 0.5), ry(THIGH_FALL.atan())),
                );
            }
            for x in [-0.75, 0.95] {
                let z = 1.6 - THIGH_FALL * f32::abs(x);
                th.cylinder(0.1, 0.08, 8, GREY, ridge * place(v(x, 3.0, z), rx(FRAC_PI_2)));
            }
        });

        // The shin: the knee's brown barrel, the core swelling to the calf, and its armour: the
        // knee guard standing out of its top, a front plate with a ridge down it, and a plate on
        // the calf's outside stepping out of the core.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.66, 1.4, 12, BROWN, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN, 3.2, 28, BODY, tube);
        let ridge = tube * at(0.06, 0.0, 0.0);
        for (half, [foot, top]) in SHIN_PLATE.into_iter().enumerate() {
            let xf = if half == 0 { ridge } else { ridge * flip() };
            m.hexa(ridged(foot, top, 0.45, SHIN_FALL), 0.12, BODY, xf);
        }
        let calf = [
            (1.3, 2.35, -1.05),
            (1.62, 2.35, -1.0),
            (1.4, 4.65, -1.55),
            (1.74, 4.65, -1.5),
            (1.3, 2.35, 0.3),
            (1.62, 2.35, 0.22),
            (1.4, 4.65, 0.7),
            (1.74, 4.65, 0.6),
        ]
        .map(|(x, y, z)| v(x, y, z));
        m.hexa(calf, 0.1, BODY, tube);
        for z in [-0.7, -0.4] {
            slot(&mut m, 0.7, tube * place(v(1.71, 3.75, z), ry(FRAC_PI_2) * rx(0.12)));
        }
        // The calf's back: a plate in two tiers following its swell, a ridge down its middle,
        // and a louvred vent in the lower.
        let back = tube * at(0.1, 0.0, 0.0) * Affine3A::from_rotation_y(PI);
        for (half, w) in [(0, 0.95), (1, 0.85)] {
            let xf = if half == 0 { back } else { back * flip() };
            m.hexa(ridged((2.25, w * 0.8, 1.62), (3.75, w, 2.02), 1.1, CALF_FALL), 0.1, BODY, xf);
            m.hexa(ridged((3.75, w, 2.02), (4.8, w * 0.9, 1.86), 1.1, CALF_FALL), 0.1, BODY, xf);
        }
        let vent = back * place(v(0.0, 3.0, 1.83), rx(-0.26));
        m.cube(v(0.7, 0.75, 0.12), 0.02, FRAME, vent);
        for k in 0..3 {
            m.cube(
                v(0.62, 0.09, 0.14),
                0.0,
                TRIM,
                vent * at(0.0, -0.22 + k as f32 * 0.22, 0.02) * Affine3A::from_rotation_x(0.4),
            );
        }
        m.greeble(|m| {
            // The seam across the shin plate's foot, and one down the calf's inside.
            for half in 0..2 {
                let xf = if half == 0 { ridge } else { ridge * flip() };
                m.cube(
                    v(0.8, 0.06, 0.05),
                    0.0,
                    FRAME,
                    xf * place(v(0.4, 1.7, 1.2 - SHIN_FALL * 0.4), ry(SHIN_FALL.atan())),
                );
            }
            m.cube(v(0.06, 1.6, 0.05), 0.0, FRAME, tube * place(v(-1.36, 3.6, -0.4), ry(-FRAC_PI_2)));
        });
        // The knee guard: a heavy block standing out of the shin's top, its face leaning back, a
        // grey plate on it with four rivets; a wedge rising to a ridge over it, tucked under the
        // thigh.
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        let block = [
            (-0.84, -1.3, 0.45),
            (0.84, -1.3, 0.45),
            (-0.74, 0.95, 0.25),
            (0.74, 0.95, 0.25),
            (-0.78, -1.3, 1.62),
            (0.78, -1.3, 1.62),
            (-0.66, 0.95, 1.22),
            (0.66, 0.95, 1.22),
        ]
        .map(|(x, y, z)| v(x, y, z));
        m.hexa(block, 0.2, BODY, guard);
        let cap = [
            (-0.74, 0.95, 0.25),
            (0.74, 0.95, 0.25),
            (-0.2, 1.45, 0.2),
            (0.2, 1.45, 0.2),
            (-0.66, 0.95, 1.22),
            (0.66, 0.95, 1.22),
            (-0.2, 1.45, 0.9),
            (0.2, 1.45, 0.9),
        ]
        .map(|(x, y, z)| v(x, y, z));
        m.hexa(cap, 0.1, BODY, guard);
        let lean = (0.4f32 / 2.25).atan();
        let face = guard * place(v(0.0, -0.2, 1.42 + 0.04), rx(-lean));
        m.block(v(1.05, 1.5, 0.12), v2(0.8, 0.92), Vec2::ZERO, 0.04, TRIM, face);
        m.greeble(|m| {
            for (dx, dy) in [(-0.3, 0.45), (0.3, 0.45), (-0.36, -0.5), (0.36, -0.5)] {
                m.cylinder(0.09, 0.08, 6, GREY, face * place(v(dx, dy, 0.07), rx(FRAC_PI_2)));
            }
        });

        // The boot's shaft round the ankle: a rounded box over the shin's foot, a thick plate
        // rising up its front like a boot's tongue, a brown band round its back, and a wheel either
        // side where it meets the foot.
        let x = ankle.x;
        let flat = sided(s, Affine3A::IDENTITY);
        let (y0, y1) = (ankle.y - 0.55, ankle.y + 1.15);
        m.loft(
            &[
                hoop(y0, x, -0.08, 1.22, 1.06),
                hoop(y0 + 0.9, x, -0.1, 1.18, 1.02),
                hoop(y1, x, -0.12, 1.04, 0.92),
                hoop(y1 + 0.08, x, -0.12, 0.92, 0.8),
            ],
            3.6,
            24,
            BODY,
            flat,
        );
        let tongue = [
            (-1.08, y0 + 0.15, 0.55),
            (1.08, y0 + 0.15, 0.55),
            (-0.88, y1 + 0.3, 0.62),
            (0.88, y1 + 0.3, 0.62),
            (-1.08, y0 + 0.15, 1.82),
            (1.08, y0 + 0.15, 1.82),
            (-0.88, y1 + 0.3, 1.22),
            (0.88, y1 + 0.3, 1.22),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.2, BODY, flat);
        let slope = v(0.0, y1 + 0.3 - (y0 + 0.15), 1.22 - 1.82).normalize();
        let face = sided(s, place(v(x, y1 - 0.1, 1.32), toward(slope)));
        m.greeble(|m| {
            m.cube(v(1.3, 0.06, 0.05), 0.0, FRAME, face * at(0.0, -0.9, 0.04));
            m.cube(v(0.07, 0.5, 0.05), 0.0, FRAME, face * at(-0.3, 0.05, 0.0));
            m.cube(v(0.07, 0.5, 0.05), 0.0, FRAME, face * at(0.3, 0.05, 0.0));
        });
        m.lathe_arc(
            &band(1.04, 1.18, ankle.y - 0.3, ankle.y + 0.1),
            18,
            0.2,
            PI - 0.2,
            BROWN,
            sided(s, at(x, 0.0, ankle.z - 0.1)) * Affine3A::from_rotation_y(PI),
        );
        for side in [-1.0, 1.0] {
            let hub = sided(s, place(v(x + side * 1.2, ankle.y - 0.25, ankle.z - 0.15), rz(FRAC_PI_2)));
            wheel(&mut m, if side > 0.0 { 0.58 } else { 0.46 }, 0.32, hub);
        }

        // The foot: a thick platform, the heel high at its back and the instep under the
        // tongue, and the long toe (on its own hinge) falling away to its tip with a narrower block
        // raised along it; a dark sole under it all, split at the hinge, a vernier in each half,
        // and a brown band round the heel over it.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.5, 5, BROWN, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        f.hexa(boot(x, base, (-7.45, -7.7), (1.34, 1.3), [-2.15, -1.9, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.42, 1.42), [-2.27, -2.24, -0.7, -0.7]),
            0.05,
            BROWN,
            flat,
        );
        f.greeble(|f| {
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 3.2), 0.0, FRAME, sided(s, at(x + side * 1.2, -8.3, -0.35)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.17, -8.25, 0.9)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.05, -2.07), rx(-0.25))));
        });
        let heel =
            Sole { z: (-2.25, 1.26), half: (1.42, 1.34), top: 0.34, pads: BROWN, vernier: 0.1, r: 0.44 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.76, -8.04), (1.3, 1.1), [1.32, 1.32, 3.4, 2.95]), 0.34, BODY, flat);
        t.hexa(boot(x, -8.1, (-7.58, -7.9), (0.62, 0.52), [1.38, 1.38, 2.85, 2.55]), 0.12, BODY, flat);
        t.greeble(|t| {
            t.cube(v(1.5, 0.05, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.28, 2.55), rx(0.2))));
        });
        let toes =
            Sole { z: (1.34, 3.48), half: (1.34, 1.16), top: 0.34, pads: BROWN, vernier: 2.35, r: 0.36 };
        sole(d, toe, s, x, toes);
    }
}

/// A wheel (or a ring) facing along `xf`'s +y, as the Argo's: a grey tyre round a dark hub.
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
        14,
        TRIM,
        xf,
    );
    o.cylinder(r * 0.46, width * 0.9, 8, FRAME, xf);
}

/// How far a foot block's sides lean in, per metre up.
const BOOT_LEAN: f32 = 0.18;

/// A block of a foot ([`frames::boot`](crate::frames::boot)), its sides leaning in [`BOOT_LEAN`].
fn boot(x: f32, base: f32, top: (f32, f32), half: (f32, f32), z: [f32; 4]) -> [Vec3; 8] {
    crate::frames::boot(x, base, top, half, z, BOOT_LEAN)
}

// --- Stencils: thin plates just proud of the paint, up close only. ---

/// How far a stencil stands off its face, and how thick it is.
const STENCIL: (f32, f32) = (0.016, 0.03);

/// A seven-segment digit's segments, a to g, as the Argo's: (centre x, centre y, width, height)
/// in a digit `w` wide and `h` tall, strokes `t` thick, with stencil gaps at the corners.
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

/// The unit number on the right shoulder's outer plate, reading front to back as one stands
/// beside it, and the insignia on the left's (a hexagon round a chevron, as the Argo wears).
fn stencils(d: &mut Designer) {
    // The plate's outer face: x along it toward the back, y up, z out of it.
    let face = Affine3A::from_mat3_translation(
        glam::Mat3::from_cols(v(0.0, 0.0, -1.0), Vec3::Y, Vec3::X),
        v(5.62 + STENCIL.0, 5.2, 0.0),
    );
    let mut o = d.on(Bone::ShoulderR);
    o.greeble(|o| {
        let (w, h) = (0.42, 0.74);
        for place in 0..2u8 {
            let x = (f32::from(place) - 0.5) * (w + 0.1);
            for (segment, (cx, cy, sw, sh)) in segments(w, h, 0.085).into_iter().enumerate() {
                let paint = Paint::Digit { place, segment: segment as u8 };
                o.cube(v(sw, sh, STENCIL.1), 0.0, paint, face * at(x + cx, cy, 0.0));
            }
        }
    });
    let mut o = d.on(Bone::ShoulderL);
    o.greeble(|o| {
        let face = sided(Side::L, face);
        let white = Paint::Fixed(paint::WHITE);
        let r = 0.4;
        let corner = |i: f32| {
            let t = std::f32::consts::FRAC_PI_3 * i + std::f32::consts::FRAC_PI_6;
            v(r * t.cos(), r * t.sin(), 0.0)
        };
        for k in 0..6 {
            let (p, q) = (corner(k as f32), corner(k as f32 + 1.0));
            o.cube(v(0.07, r + 0.035, STENCIL.1), 0.0, white, face * place((p + q) * 0.5, toward(q - p)));
        }
        for side in [-1.0, 1.0] {
            let (p, q) = (v(side * 0.19, 0.1, 0.0), v(0.0, -0.12, 0.0));
            o.cube(
                v(0.09, (q - p).length() + 0.06, STENCIL.1),
                0.0,
                white,
                face * place((p + q) * 0.5, toward(q - p)),
            );
        }
        o.cube(v(0.3, 0.07, STENCIL.1), 0.0, white, face * at(0.0, 0.19, 0.0));
    });
}

// --- Backpack. ---

/// The pack on the back: a heavy box rounded at its edges, up from the thrusters' housing to its
/// top, and how far back its back face stands.
const PACK: [Hoop; 5] = [
    hoop(2.85, 0.0, -2.5, 1.3, 0.75),
    hoop(3.1, 0.0, -2.55, 1.5, 0.9),
    hoop(5.2, 0.0, -2.5, 1.55, 0.92),
    hoop(5.55, 0.0, -2.42, 1.35, 0.8),
    hoop(5.7, 0.0, -2.38, 1.0, 0.55),
];
const PACK_BACK: f32 = -3.42;

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    b.loft(&PACK, 5.0, 28, BODY, Affine3A::IDENTITY);
    // Its back: a raised plate down the middle with a grey handle, and a louvred vent either
    // side of it.
    let plate = [
        (-0.72, 3.45, PACK_BACK + 0.1),
        (0.72, 3.45, PACK_BACK + 0.1),
        (-0.62, 5.2, PACK_BACK + 0.1),
        (0.62, 5.2, PACK_BACK + 0.1),
        (-0.72, 3.45, PACK_BACK - 0.16),
        (0.72, 3.45, PACK_BACK - 0.16),
        (-0.62, 5.2, PACK_BACK - 0.12),
        (0.62, 5.2, PACK_BACK - 0.12),
    ]
    .map(|(x, y, z)| v(x, y, z));
    b.hexa(plate, 0.06, BODY, Affine3A::IDENTITY);
    b.cylinder(0.08, 0.9, 8, GREY, place(v(0.0, 4.95, PACK_BACK - 0.36), rz(FRAC_PI_2)));
    for x in [-0.36, 0.36] {
        b.cube(v(0.12, 0.14, 0.32), 0.02, GREY, at(x, 4.95, PACK_BACK - 0.24));
    }
    b.greeble(|b| {
        b.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, at(0.0, 4.3, PACK_BACK - 0.17));
        b.cube(v(0.06, 0.6, 0.05), 0.0, FRAME, at(0.0, 3.85, PACK_BACK - 0.17));
    });
    for s in Side::BOTH {
        let vent = sided(s, place(v(1.12, 4.25, PACK_BACK + 0.02), ry(PI)));
        b.cube(v(0.46, 1.4, 0.1), 0.02, FRAME, vent);
        for k in 0..6 {
            let y = -0.55 + k as f32 * 0.22;
            b.cube(v(0.4, 0.08, 0.14), 0.0, TRIM, vent * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.45));
        }
    }
    // The grille on its top, and the red beacon behind it.
    b.cube(v(1.5, 0.1, 0.7), 0.02, FRAME, at(0.0, 5.66, -2.3));
    b.greeble(|b| {
        for k in 0..5 {
            b.cube(v(1.4, 0.06, 0.08), 0.0, TRIM, at(0.0, 5.71, -2.56 + k as f32 * 0.13));
        }
    });
    b.cylinder(0.2, 0.14, 10, GUN, at(0.0, 5.66, -2.85));
    b.sphere(0.16, 6, Paint::Light(Light::Beacon), at(0.0, 5.75, -2.85));
    // The propellant tanks standing either side, banded, a vernier on each's outside; and the
    // housing of the main thrusters under the pack.
    for s in Side::BOTH {
        let tank = sided(s, at(1.78, 0.0, -2.62));
        b.lathe(
            &[
                (0.0, 2.75),
                (0.32, 2.75),
                (0.5, 2.95),
                (0.54, 3.2),
                (0.54, 5.0),
                (0.5, 5.25),
                (0.32, 5.45),
                (0.0, 5.45),
            ],
            16,
            TRIM,
            tank,
        );
        for y in [3.35, 4.85] {
            b.lathe(&band(0.53, 0.6, y - 0.1, y + 0.1), 16, GUN, tank);
        }
        b.greeble(|b| {
            b.lathe(&band(0.53, 0.57, 4.07, 4.13), 16, FRAME, tank);
        });
        bell(&mut b, v(0.52, 4.1, 0.0), v(1.0, -0.1, -0.2), 0.26, tank);
    }
    b.block(v(2.3, 0.6, 1.3), v2(0.92, 0.9), Vec2::ZERO, 0.1, GUN, at(0.0, 2.95, -2.75));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.68, 2.85, -3.2)), v(0.0, -0.5, -0.85), 0.56);
    }
}

/// A thruster bell that's not one of the main engines (no socket), its mouth along `dir`.
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
        12,
        GUN,
        b,
    );
}

// --- The beam cannon. ---

/// The beam cannon, along its bore from the grip (as [`crate::hold`] has it held): how high the
/// bore runs over the grip, and where its sections run along it: the receiver over the fist, the
/// vented shroud, the barrel and the muzzle. The condenser rides on the receiver's back, high over
/// the wrist: its axis's height, and where it runs. Nothing else lies behind the grip, and nothing
/// ahead of it hangs below the receiver's foot, so at the ready the forearm lies under the
/// condenser, and carried muzzle down the cannon hangs clear in front of the leg.
const BORE: f32 = 0.95;
const RECEIVER: (f32, f32) = (0.05, 2.8);
const SHROUD: (f32, f32) = (2.75, 4.75);
const MUZZLE: (f32, f32) = (6.2, 7.05);
const CONDENSER_Y: f32 = 1.95;
const CONDENSER: (f32, f32) = (-1.4, 1.75);

/// The beam cannon, carried under the right arm: the olive receiver over the fist with its grip
/// and sight, the ribbed condenser drum riding along its back and out over the wrist, cabled to
/// the vented box shroud ahead, the barrel with its cooling rings, and the flared emitter round the
/// bore.
fn cannon(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let along_z = |y: f32, z: f32| place(v(g.x, g.y + y, g.z + z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    // The grip, upright in the fist ahead of the wrist, and its guard running forward to the
    // receiver.
    w.cube(v(0.34, 1.0, 0.46), 0.05, GUN, at(g.x, g.y - 0.15, g.z + 0.27));
    w.cube(v(0.12, 0.1, 0.5), 0.02, GUN, at(g.x, g.y - 0.12, g.z + 0.72));
    w.cube(v(0.12, 0.5, 0.1), 0.02, GUN, at(g.x, g.y + 0.1, g.z + 0.95));
    // The receiver, a grey stripe down each side and its panel lines.
    let (r0, r1) = RECEIVER;
    let receiver = at(g.x, g.y + BORE, g.z + (r0 + r1) * 0.5);
    w.block(v(1.05, 1.2, r1 - r0), v2(0.86, 0.94), Vec2::ZERO, 0.16, BODY, receiver);
    for side in [-1.0, 1.0] {
        w.cube(v(0.06, 0.24, r1 - r0 - 0.5), 0.02, TRIM, receiver * at(side * 0.53, -0.18, 0.0));
    }
    w.greeble(|w| {
        for z in [-0.75, 0.55] {
            for side in [-1.0, 1.0] {
                w.cube(v(0.05, 0.75, 0.06), 0.0, FRAME, receiver * at(side * 0.535, 0.12, z));
            }
        }
    });
    // The sight on its inboard side, and its lens.
    let sight = receiver * at(-0.66, 0.2, 0.75);
    w.cube(v(0.28, 0.38, 0.8), 0.03, GUN, sight);
    w.cylinder(0.11, 0.08, 10, LENS, sight * place(v(0.0, 0.04, 0.41), rx(FRAC_PI_2)));
    // The condenser: a drum along the receiver's back, out over the wrist, ribbed, its end capped
    // round a socket; two clamps hold it down.
    let (c0, c1) = CONDENSER;
    w.lathe(
        &[(0.0, c0), (0.32, c0), (0.46, c0 + 0.1), (0.5, c0 + 0.3), (0.5, c1 - 0.25), (0.4, c1), (0.0, c1)],
        16,
        GUN,
        along_z(CONDENSER_Y, 0.0),
    );
    for k in 0..3 {
        let z = c0 + 0.6 + k as f32 * 0.42;
        w.lathe(&band(0.47, 0.56, z - 0.08, z + 0.08), 16, BODY, along_z(CONDENSER_Y, 0.0));
    }
    w.cylinder(0.18, 0.1, 12, FRAME, along_z(CONDENSER_Y, c0 - 0.02));
    for z in [0.75, 1.4] {
        w.block(
            v(0.7, 0.42, 0.24),
            v2(0.75, 1.0),
            Vec2::ZERO,
            0.04,
            GUN,
            at(g.x, g.y + BORE + 0.68, g.z + z),
        );
    }
    // The cable from the condenser's front down into the shroud's outboard side.
    let (from, to) =
        (v(g.x + 0.3, g.y + CONDENSER_Y - 0.28, g.z + c1 - 0.1), v(g.x + 0.36, g.y + BORE + 0.3, g.z + 3.2));
    w.cylinder(0.1, (to - from).length() + 0.2, 8, BROWN, place((from + to) * 0.5, toward(to - from)));
    // The shroud: a box section with vent slots down its sides.
    let (s0, s1) = SHROUD;
    let shroud = at(g.x, g.y + BORE, g.z + (s0 + s1) * 0.5);
    w.block(v(0.82, 0.86, s1 - s0), v2(1.0, 1.0), Vec2::ZERO, 0.1, GUN, shroud);
    w.greeble(|w| {
        for k in 0..5 {
            for side in [-1.0, 1.0] {
                let z = -0.7 + k as f32 * 0.35;
                w.cube(v(0.06, 0.5, 0.12), 0.0, FRAME, shroud * at(side * 0.41, 0.0, z));
            }
        }
    });
    // The barrel, its cooling rings, and the flared emitter round the bore.
    let (m0, m1) = MUZZLE;
    w.cylinder(0.36, m0 - s1 + 0.1, 16, GUN, along_z(BORE, (s1 + m0) * 0.5));
    for k in 0..4 {
        let z = s1 + 0.25 + k as f32 * 0.3;
        w.lathe(&band(0.34, 0.46, z - 0.06, z + 0.06), 12, GUN, along_z(BORE, 0.0));
    }
    w.lathe(
        &[(0.3, m0), (0.5, m0), (0.56, m0 + 0.2), (0.68, m1 - 0.12), (0.66, m1), (0.4, m1)],
        18,
        GUN,
        along_z(BORE, 0.0),
    );
    w.cylinder(0.4, 0.2, 14, FRAME, along_z(BORE, m1 - 0.08));
    w.cylinder(0.16, 0.24, 10, STEEL, along_z(BORE, m1 - 0.12));
    w.greeble(|w| {
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0 + FRAC_PI_2;
            let p = v(g.x + 0.52 * a.cos(), g.y + BORE + 0.52 * a.sin(), g.z + m0 + 0.45);
            w.cube(v(0.08, 0.2, 0.7), 0.01, GUN, place(p, rz(a - FRAC_PI_2)));
        }
    });
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + BORE, g.z + m1 + 0.05));
}

// --- The shield pods. ---

/// The shield pods' orbit round the suit's upright axis: how far out each rides, and each one's
/// bearing (radians from straight ahead, toward +x), height and tilt in at its top.
const ORBIT: f32 = 6.6;
const PODS: [(f32, f32, f32); 4] =
    [(1.05, 2.1, 0.12), (2.09, 2.1, 0.12), (4.19, 2.1, 0.12), (5.24, 2.1, 0.12)];

fn pods(d: &mut Designer) {
    let mut p = d.on(Bone::Props);
    for (k, &(bearing, y, tilt)) in PODS.iter().enumerate() {
        p.seed(0.8 + k as f32 * 0.05);
        let xf = place(v(ORBIT * bearing.sin(), y, ORBIT * bearing.cos()), ry(bearing) * rx(-tilt));
        pod(&mut p, xf);
    }
}

/// A flat hexagon `w` half wide and `h` half tall, its slanted edges `n` high, in x-y.
fn hexagon(w: f32, h: f32, n: f32) -> [Vec2; 6] {
    [v2(0.0, h), v2(-w, h - n), v2(-w, -h + n), v2(0.0, -h), v2(w, -h + n), v2(w, h - n)]
}

/// One shield pod, its outer face looking along `xf`'s +z, its long axis along y: a grey plate
/// with an olive panel raised on its face round the field generator (a dark drum, a pink ring
/// round its lens), the emitters at its points, and a dark back with a small thruster.
fn pod(o: &mut On<'_>, xf: Affine3A) {
    o.extrude(&hexagon(0.82, 1.8, 0.62), 0.32, TRIM, xf);
    o.extrude(&hexagon(0.62, 1.42, 0.46), 0.12, BODY, xf * at(0.0, 0.0, 0.2));
    o.extrude(&hexagon(0.66, 1.5, 0.5), 0.08, FRAME, xf * at(0.0, 0.0, -0.18));
    let boss = xf * at(0.0, 0.0, 0.26) * Affine3A::from_rotation_x(FRAC_PI_2);
    o.lathe(&band(0.2, 0.4, 0.0, 0.18), 14, GUN, boss);
    o.lathe(&band(0.2, 0.27, 0.12, 0.2), 12, EYE, boss);
    o.cylinder(0.22, 0.24, 12, LENS, boss);
    for end in [-1.0, 1.0] {
        let tip = xf * place(v(0.0, end * 1.5, 0.05), rz(if end > 0.0 { 0.0 } else { PI }));
        o.cube(v(0.3, 0.36, 0.44), 0.04, GUN, tip);
        o.cube(v(0.16, 0.06, 0.46), 0.0, EYE, tip * at(0.0, 0.18, 0.0));
    }
    o.greeble(|o| {
        for side in [-1.0, 1.0] {
            o.cube(v(0.05, 0.9, 0.05), 0.0, FRAME, xf * at(side * 0.4, 0.0, 0.27));
        }
    });
    bell(o, v(0.0, -0.6, -0.22), v(0.0, -0.3, -1.0), 0.2, xf);
}
