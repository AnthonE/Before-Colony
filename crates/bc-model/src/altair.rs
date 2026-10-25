//! PX-00 Altair, the colonies' hero suit, after its first design: plated wing binders rising
//! behind the shoulders, huge sweeping shoulders, a broad blue chest with its great green sensor,
//! a narrow waist, long angular legs flaring to red feet, and the Twin Buster Rifle, two rifles
//! clamped together, as long as the leg. Built as a model kit of its design is. Part by part:
//!
//! - a small white head: a rounded helmet with a ridge over the crown, the brow overhanging green
//!   eyes that slant up to their outer corners, the tall yellow V-fin with a shorter blade inside
//!   each of its own swept back out of a red crest with its green camera, a white mask with two
//!   vents over the red chin, cheek guards either side and a round ear on each side;
//! - a broad blue chest in two tiers, the lower swelling forward and coming down to a point over
//!   the abdomen, the upper leaning back to the collar: the great round sensor in its middle, its
//!   green glass domed in a yellow ring; a yellow louvred vent either side high up, red plates down
//!   its lower flanks, a white gorget under the chin and the collar round the back of the neck;
//!   blue plates down the back beside the pack;
//! - the shoulders: wide white blocks, their tops sweeping up and out under a blue cap that runs
//!   out to a red point, a red plate across the front holding the big round yellow hatch with the
//!   machine cannon's muzzle in its middle, a plate with a blue inset on the outer face, and the
//!   running lights at the outer front corners;
//! - white upper arms with a raised ring; heavy gauntlets with blue cuffs, a blue panel outside
//!   and a guard over the elbow; grey fists;
//! - the abdomen's white plates on the grey frame, red down its sides; a white belt with its red
//!   buckle, the blue crotch with its red V, and the white skirts: front, side and rear;
//! - long, angular legs, armour plated over lofted cores: slim thighs with two plates down the
//!   front, broken above the knee, a dark vent high on them, and a plate down the outside; a sharp
//!   knee guard rising to a point with a grey panel in its face; the lower legs slim under the knee
//!   and flaring broad to the ankle, plated down the front in two, down the outside (a grey vent in
//!   it) and down the calf, a dark slot over the foot; and the red feet: a tall heel with a spur up
//!   its back, the instep falling to the toes' hinge, the toe cap on its own bone with a white plate
//!   along it, and under both the soles with a vernier under the heel and under the toes;
//! - the pack on the back, its two thrusters under a grey skirt and a grille in its top, and on its
//!   top corners the wing binders ([`Bone::WingL`], [`Bone::WingR`], which the client flares on
//!   boost): each a white arm up and out to a grey hinge behind the shoulder's top, a pair of
//!   thruster bells at its root, and the wing hanging from the hinge in solid plates stepping like a
//!   folded wing: the main wing, its top sweeping up and out to a blue tip over a red band, high
//!   beside the head, a raised plate down its middle with a vent in it and a red stripe down its
//!   leading edge, falling to a point by the knee; and the smaller plate stepping out behind it,
//!   edged in blue;
//! - the Twin Buster Rifle, held as [`crate::hold`] has it: two complete rifles side by side, each
//!   a long grey receiver banded yellow with a light deck along its top, its energy cartridge riding
//!   high over its rear (over the forearm at the ready, ahead of the wrist carried), a shroud, a long
//!   round barrel and a stepped muzzle; light brackets clamp them together between the receivers,
//!   between the cartridges and round the barrels, the grip and its guard under the join. It is as
//!   long as the rifle can hang muzzle down from the fist and stay clear of the floor;
//! - the long shield on the left forearm: white, its red face raised in it, a ridge down its
//!   middle, its yellow point reaching down toward the knee with a green light; and the beam
//!   saber's hilt in the left hand.
//!
//! The livery paints the body white, the trim blue and the accent red, the eyes, the sensor and the
//! crest's camera green; the vents, the V-fin, the hatches and the rifle's bands are yellow. The
//! inner frame, the fists and the hinges are grey, the weapons gunmetal, their brackets light grey.
//!
//! Each part is drawn through a placement in the suit's frame, so the Aquila ([`crate::aquila`])
//! folds the very same parts into its flight form.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::detail::{GREY, band, slot, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, THROAT, TRIM, YELLOW, along, at, boot, j, nozzle,
    place, rx, ry, rz, sided, sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On, paint};

/// The light grey of the rifle's brackets and decks.
const LIGHT: Paint = Paint::Fixed(paint::HULL);

pub(crate) fn altair(d: &mut Designer) {
    let rest = Affine3A::IDENTITY;
    head(&mut d.on(Bone::Head), rest);
    d.head_base = FACE_FOOT + 0.08;
    chest(&mut d.on(Bone::Chest), rest);
    abdomen(&mut d.on(Bone::Torso), rest);
    waist(&mut d.on(Bone::Waist), rest);
    for s in Side::BOTH {
        let (shoulder, up, fore, hand) = s.pick(
            (Bone::ShoulderL, Bone::UpperArmL, Bone::ForearmL, Bone::HandL),
            (Bone::ShoulderR, Bone::UpperArmR, Bone::ForearmR, Bone::HandR),
        );
        shoulder_armour(&mut d.on(shoulder), s, rest);
        upper_arm(&mut d.on(up), s, rest);
        forearm(&mut d.on(fore), s, rest);
        fist(&mut d.on(hand), s, rest);
        let (thigh_bone, shin_bone, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        thigh(&mut d.on(thigh_bone), s, rest);
        shin(&mut d.on(shin_bone), s, rest);
        heel(&mut d.on(foot), s, rest);
        toe_cap(&mut d.on(toe), s, rest);
        let x = j(Bone::FootR).x;
        sole(d, foot, s, x, HEEL_SOLE);
        sole(d, toe, s, x, TOE_SOLE);
        binder(&mut d.on(s.pick(Bone::WingL, Bone::WingR)), s, rest, rest);
    }
    backpack(&mut d.on(Bone::Backpack), rest);
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.6, 3.0, -2.55)), v(0.0, -0.6, -0.8), 0.48);
    }
    let muzzle = twin_rifle(&mut d.on(Bone::Weapon), Affine3A::from_translation(j(Bone::Weapon)));
    d.sockets.muzzle = Designer::local(Bone::Weapon, muzzle);
    shield(&mut d.on(Bone::Shield), shield_place());
    saber(d);
}

/// A hexahedron from its corners as (x, y, z) tuples.
fn hexa_of(c: [(f32, f32, f32); 8]) -> [Vec3; 8] {
    c.map(|(x, y, z)| v(x, y, z))
}

/// A louvred vent on a face at `xf` (looking along its +z): a frame round a dark recess, slats
/// across it, `w` wide and `h` high.
fn vent(o: &mut On<'_>, w: f32, h: f32, slats: usize, frame: Paint, slat: Paint, xf: Affine3A) {
    let t = 0.12;
    o.cube(v(w - t, h - t, 0.1), 0.01, FRAME, xf);
    for y in [-0.5, 0.5] {
        o.cube(v(w + t, t, 0.18), 0.03, frame, xf * at(0.0, y * h, 0.02));
    }
    for x in [-0.5, 0.5] {
        o.cube(v(t, h, 0.18), 0.03, frame, xf * at(x * w, 0.0, 0.02));
    }
    let pitch = (h - t) / slats as f32;
    for k in 0..slats {
        let y = -0.5 * (h - t) + pitch * (k as f32 + 0.5);
        o.cube(
            v(w - t, pitch * 0.55, 0.12),
            0.01,
            slat,
            xf * at(0.0, y, 0.02) * Affine3A::from_rotation_x(0.4),
        );
    }
}

/// A small thruster bell at `xf` (its mouth along local +y), steel round a dark throat.
fn bell(o: &mut On<'_>, r: f32, xf: Affine3A) {
    o.lathe(
        &[
            (r * 0.5, -r * 0.1),
            (r * 0.62, 0.0),
            (r * 0.8, r * 0.55),
            (r, r * 1.25),
            (r * 0.9, r * 1.28),
            (r * 0.5, r * 0.1),
        ],
        12,
        STEEL,
        xf,
    );
    o.cylinder(r * 0.52, 0.04, 10, FRAME, xf * at(0.0, r * 0.12, 0.0));
}

// --- Head. ---

/// The foot of the face: the red chin's underside.
pub(crate) const FACE_FOOT: f32 = 6.08;

/// The helmet, up from behind the jaw to the crown (centred on the head's axis, z forward): a
/// rounded box, its front set back behind the face and the brow.
const HELMET: [Hoop; 6] = [
    hoop(6.3, 0.0, -0.1, 0.58, 0.6),
    hoop(6.55, 0.0, -0.04, 0.7, 0.72),
    hoop(6.95, 0.0, -0.02, 0.74, 0.76),
    hoop(7.28, 0.0, -0.04, 0.68, 0.72),
    hoop(7.47, 0.0, -0.08, 0.52, 0.58),
    hoop(7.56, 0.0, -0.1, 0.28, 0.34),
];

/// One blade of the V-fin (the right), in its own plane from its root on the forehead: rising
/// out and up, narrowing to its point.
const V_FIN: [Vec2; 7] = [
    Vec2::new(0.0, -0.06),
    Vec2::new(0.24, -0.06),
    Vec2::new(1.42, 1.48),
    Vec2::new(1.52, 1.72),
    Vec2::new(1.3, 1.58),
    Vec2::new(0.12, 0.22),
    Vec2::new(0.0, 0.2),
];

/// The shorter, steeper blade inside each of the V-fin's.
const INNER_FIN: [Vec2; 5] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(0.14, 0.0),
    Vec2::new(0.52, 0.86),
    Vec2::new(0.44, 0.92),
    Vec2::new(0.0, 0.18),
];

/// The head at `xf` (its rest place in the suit's frame).
pub(crate) fn head(h: &mut On<'_>, xf: Affine3A) {
    h.seed(0.05);
    // The neck, grey, turning inside the collar.
    h.cylinder(0.4, 0.75, 12, GREY, xf * at(0.0, 5.98, 0.0));
    h.loft(&HELMET, 3.2, 28, BODY, xf);
    // The dark band the eyes are set in, and between them the face's ridge up to the brow.
    h.cube(v(1.0, 0.3, 0.45), 0.03, FRAME, xf * at(0.0, 6.77, 0.86));
    h.block(v(0.12, 0.28, 0.12), v2(0.8, 1.0), Vec2::ZERO, 0.02, BODY, xf * at(0.0, 6.76, 1.06));
    // The mask under the eyes, narrowing to the red chin, two vents in its front.
    h.block(v(0.48, 0.4, 0.5), v2(1.75, 1.0), Vec2::ZERO, 0.05, BODY, xf * at(0.0, 6.44, 0.86));
    let chin = xf * at(0.0, FACE_FOOT + 0.08, 0.9);
    h.block(v(0.34, 0.16, 0.44), v2(1.25, 1.0), Vec2::ZERO, 0.04, ACCENT, chin);
    h.greeble(|h| {
        for x in [-0.1, 0.1] {
            h.cube(v(0.05, 0.22, 0.04), 0.0, FRAME, xf * at(x, 6.44, 1.11));
        }
    });
    for s in Side::BOTH {
        let flat = xf * sided(s, Affine3A::IDENTITY);
        // The eyes: green, slanting up to their outer corners.
        let eye = [v2(0.09, 6.69), v2(0.38, 6.71), v2(0.47, 6.86), v2(0.1, 6.8)];
        h.extrude(&eye, 0.06, EYE, flat * at(0.0, 0.0, 1.1));
        // The brow: a plate over the eyes from the middle out, sweeping back, its foot overhanging
        // them.
        let brow = hexa_of([
            (0.0, 6.88, 0.7),
            (0.62, 6.94, 0.6),
            (0.0, 7.12, 0.7),
            (0.62, 7.18, 0.6),
            (0.0, 6.88, 1.22),
            (0.62, 6.94, 1.02),
            (0.0, 7.12, 1.12),
            (0.62, 7.18, 0.92),
        ]);
        h.hexa(brow, 0.04, BODY, flat);
        // The cheek guards either side of the face, their fronts raking back to the jaw.
        let cheek = hexa_of([
            (0.5, 6.27, 0.3),
            (0.76, 6.27, 0.3),
            (0.5, 6.95, 0.2),
            (0.72, 6.95, 0.2),
            (0.5, 6.27, 0.88),
            (0.76, 6.27, 0.88),
            (0.5, 6.95, 1.1),
            (0.72, 6.95, 1.1),
        ]);
        h.hexa(cheek, 0.05, BODY, flat);
        // The ears: a round drum either side, a dark ring in its face round a grey boss.
        let ear = flat * place(v(0.72, 6.72, -0.04), rz(-FRAC_PI_2));
        h.lathe(
            &[(0.0, 0.0), (0.34, 0.0), (0.36, 0.05), (0.36, 0.12), (0.3, 0.17), (0.0, 0.17)],
            18,
            BODY,
            ear,
        );
        h.cylinder(0.24, 0.06, 14, FRAME, ear * at(0.0, 0.17, 0.0));
        h.cylinder(0.12, 0.12, 10, GREY, ear * at(0.0, 0.19, 0.0));
        slot(h, 0.38, flat * place(v(0.7, 7.12, 0.3), ry(FRAC_PI_2)));
        // The V-fin, leaning back from the forehead, and a shorter blade inside each of its own.
        h.extrude(&V_FIN, 0.1, YELLOW, flat * place(v(0.04, 7.2, 1.06), rx(-0.3)));
        h.extrude(&INNER_FIN, 0.08, YELLOW, flat * place(v(0.06, 7.34, 1.0), rx(-0.42)));
    }
    // The crest on the forehead, red, its camera lit green.
    let crest = xf * place(v(0.0, 7.24, 1.0), rx(-0.3));
    h.block(v(0.26, 0.3, 0.34), v2(0.7, 0.75), v2(0.0, -0.03), 0.04, ACCENT, crest);
    h.cube(v(0.12, 0.1, 0.06), 0.02, EYE, xf * place(v(0.0, 7.2, 1.17), rx(-0.3)));
    // A ridge over the crown from the crest back, and down the back of the helmet.
    let ridge = xf * place(v(0.0, 7.53, 0.12), rx(0.12));
    h.block(v(0.28, 0.14, 1.1), v2(0.8, 0.9), Vec2::ZERO, 0.04, BODY, ridge);
    let back = xf * place(v(0.0, 6.95, -0.74), rx(0.15));
    h.block(v(0.34, 0.9, 0.3), v2(0.7, 0.9), Vec2::ZERO, 0.06, BODY, back);
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half), behind its plate: (y, outer x, back z, front z) at its foot and
/// its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.3, 1.5, -1.25, 0.6), (5.85, 1.95, -1.4, 0.6)];

/// The blue chest plate's two tiers (right half), each its corners in front view (x, y): its foot
/// inside and outside, its top inside and outside. The lower tier's foot rises outward from the
/// middle, so the plate comes down to a point over the abdomen.
const PLATE: [[(f32, f32); 4]; 2] = [
    [(0.0, 3.35), (1.75, 3.85), (0.0, 4.55), (2.2, 4.55)],
    [(0.0, 4.55), (2.2, 4.55), (0.0, 5.85), (2.35, 5.85)],
];

/// Each tier's front, a plane: z = z0 + rise (y - y0) - fall x, as (y0, z0, rise, fall). The
/// lower swells forward as it rises, the upper leans back to the collar.
const PLATE_FRONT: [(f32, f32, f32, f32); 2] = [(3.35, 1.2, 0.28, 0.2), (4.55, 1.536, -0.42, 0.2)];

/// The chest plate's front on tier `k` at (`x`, `y`), and its outward normal.
fn plate_front(k: usize, x: f32, y: f32) -> (Vec3, Vec3) {
    let (y0, z0, rise, fall) = PLATE_FRONT[k];
    (v(x, y, z0 + rise * (y - y0) - fall * x), v(fall, -rise, 1.0).normalize())
}

/// A frame on the chest plate's front (tier `k`) at (`x`, `y`): z out of the face, y up it.
fn on_plate(k: usize, x: f32, y: f32) -> Affine3A {
    let (p, n) = plate_front(k, x, y);
    place(p, Quat::from_rotation_arc(Vec3::Z, n))
}

/// The great round sensor in the middle of the chest.
const SENSOR: Vec3 = Vec3::new(0.0, 4.6, 1.42);

/// The chest at `xf`: the blue core and its plate in two tiers, the great green sensor in its
/// middle ringed in yellow, the yellow vents either side, red plates down its lower flanks, the
/// shoulders' sockets, the plates on its back, the gorget under the chin and the collar round the
/// neck.
pub(crate) fn chest(c: &mut On<'_>, xf: Affine3A) {
    c.seed(0.13);
    for s in Side::BOTH {
        let flat = xf * sided(s, Affine3A::IDENTITY);
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
        c.hexa(core, 0.2, TRIM, flat);
        for (k, tier) in PLATE.iter().enumerate() {
            let back = tier.map(|(x, y)| v(x, y, 0.2));
            let front = tier.map(|(x, y)| plate_front(k, x, y).0);
            let corners = [back[0], back[1], back[2], back[3], front[0], front[1], front[2], front[3]];
            c.hexa(corners, 0.18, TRIM, flat);
        }
        // The yellow vents in the upper tier, either side of the sensor, and seams across the lower.
        vent(
            c,
            0.95,
            0.72,
            4,
            YELLOW,
            YELLOW,
            flat * on_plate(1, 1.55, 5.18) * Affine3A::from_rotation_z(0.12),
        );
        // A red plate down each lower flank, where the chest comes down to the waist.
        let flank = flat * on_plate(0, 1.5, 3.98) * Affine3A::from_rotation_z(-0.28);
        c.block(v(0.5, 0.95, 0.12), v2(0.8, 1.0), Vec2::ZERO, 0.03, ACCENT, flank);
        c.greeble(|c| {
            c.cube(v(0.06, 0.6, 0.05), 0.0, FRAME, flat * on_plate(0, 1.1, 4.0));
            c.cube(v(0.9, 0.06, 0.05), 0.0, FRAME, flat * on_plate(1, 1.35, 4.72));
        });
        // The shoulder's socket in the chest's side, and the back's plates beside the backpack.
        c.cylinder(0.6, 0.6, 14, GREY, flat * place(v(2.3, 4.5, 0.0), rz(FRAC_PI_2)));
        let plate = flat * place(v(1.4, 4.6, -1.5), ry(-0.2));
        c.block(v(0.9, 2.0, 0.34), v2(0.85, 0.85), Vec2::ZERO, 0.14, TRIM, plate);
        slot(c, 0.5, plate * at(0.15, 0.2, -0.18) * Affine3A::from_rotation_y(PI));
    }
    // The sensor: a blue housing, a yellow ring and the green glass domed in it.
    let face = xf * place(SENSOR, rx(FRAC_PI_2));
    c.lathe(&[(0.0, -0.1), (0.98, -0.1), (1.0, 0.08), (0.9, 0.14), (0.0, 0.14)], 28, TRIM, face);
    c.lathe(&band(0.6, 0.82, 0.08, 0.24), 28, YELLOW, face);
    c.cylinder(0.62, 0.1, 24, FRAME, face * at(0.0, 0.16, 0.0));
    let dome = [(0.0, 0.16), (0.58, 0.16), (0.55, 0.28), (0.44, 0.4), (0.24, 0.48), (0.0, 0.5)];
    c.lathe(&dome, 24, EYE, face);
    // The gorget across the chest's top under the chin, and the collar round the back of the neck.
    c.block(v(1.7, 0.36, 0.7), v2(0.9, 0.8), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 5.92, 0.68));
    let (from, to) = (FRAC_PI_2 + 0.8, FRAC_PI_2 - 0.8 + TAU);
    c.lathe_arc(&band(0.55, 0.95, 5.8, 6.06), 20, from, to, BODY, xf);
    c.lathe_arc(&band(0.5, 0.78, 6.06, 6.2), 20, from, to, GREY, xf);
}

/// The abdomen at `xf`: the grey frame, white plates across its front, red down its sides.
pub(crate) fn abdomen(a: &mut On<'_>, xf: Affine3A) {
    a.seed(0.17);
    let squash = xf * Affine3A::from_scale(v(1.0, 1.0, 0.85));
    a.lathe(&[(0.0, 2.3), (1.12, 2.3), (1.2, 2.5), (1.1, 3.5), (0.0, 3.5)], 14, GREY, squash);
    a.block(v(1.5, 0.42, 0.5), v2(0.94, 0.9), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 2.71, 0.8));
    a.block(v(1.36, 0.38, 0.5), v2(0.94, 0.9), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 3.16, 0.8));
    for s in Side::BOTH {
        let side = xf * sided(s, place(v(1.0, 2.9, 0.1), rz(0.1)));
        a.block(v(0.4, 0.9, 1.3), v2(0.8, 0.9), Vec2::ZERO, 0.08, ACCENT, side);
    }
}

// --- Waist and skirts. ---

/// The belt round the hips.
const BELT: [Hoop; 3] =
    [hoop(1.9, 0.0, 0.0, 1.48, 1.1), hoop(2.0, 0.0, 0.0, 1.58, 1.18), hoop(2.45, 0.0, 0.0, 1.58, 1.18)];

/// The waist at `xf`: the belt, its red buckle, the blue crotch block with its red V, and the
/// white skirts: front, side and rear.
pub(crate) fn waist(w: &mut On<'_>, xf: Affine3A) {
    w.seed(0.2);
    w.loft(&BELT, 3.2, 28, BODY, xf);
    let buckle = [v2(-0.55, 0.22), v2(0.55, 0.22), v2(0.0, -0.3)];
    w.extrude(&buckle, 0.16, ACCENT, xf * at(0.0, 2.24, 1.22));
    w.block(v(0.95, 1.2, 1.3), v2(1.3, 1.0), v2(0.0, -0.05), 0.14, TRIM, xf * at(0.0, 1.38, 0.45));
    let vee = [v2(-0.44, 0.42), v2(0.44, 0.42), v2(0.44, 0.02), v2(0.0, -0.46), v2(-0.44, 0.02)];
    w.extrude(&vee, 0.12, ACCENT, xf * at(0.0, 1.42, 1.13));
    for s in Side::BOTH {
        // The front skirt over the thigh, its hem rising outboard; a raised panel down its inner
        // half and a stepped band along its hem.
        let skirt = xf * sided(s, place(v(1.08, 1.55, 1.32), ry(0.3) * rx(-0.16)));
        let (x0, x1) = (-0.62, 0.68);
        let hem = |x: f32| -1.3 + (x - x0) / (x1 - x0) * 0.42;
        let plate = |a: f32, b: f32, lo: f32, hi: Option<f32>, top: f32, back: f32, front: f32| {
            let (ya, yb) = (hem(a) + lo, hem(b) + lo);
            let (ta, tb) = hi.map_or((top, top), |h| (hem(a) + h, hem(b) + h));
            [
                v(a, ya, back),
                v(b, yb, back),
                v(a, ta, back),
                v(b, tb, back),
                v(a, ya, front),
                v(b, yb, front),
                v(a, ta, front),
                v(b, tb, front),
            ]
        };
        w.hexa(plate(x0, x1, 0.0, None, 0.55, -0.16, 0.16), 0.12, BODY, skirt);
        w.hexa(plate(-0.52, 0.05, 0.22, None, 0.4, 0.08, 0.24), 0.05, BODY, skirt);
        w.hexa(plate(x0 - 0.02, x1 + 0.02, -0.02, Some(0.3), 0.0, -0.19, 0.2), 0.06, BODY, skirt);
        slot(w, 0.7, skirt * at(0.3, -0.25, 0.17));
        // The side skirt over the hip, a grey slot in it; the rear skirt's half, swept back.
        let side = xf * sided(s, place(v(2.42, 1.5, -0.1), rz(0.18)));
        w.block(v(0.42, 1.4, 1.9), v2(0.85, 0.9), Vec2::ZERO, 0.14, BODY, side);
        w.cube(v(0.1, 0.6, 0.5), 0.03, GREY, side * at(0.22, -0.1, 0.3));
        let rear = xf * sided(s, place(v(0.8, 1.5, -1.32), rx(0.3)));
        w.block(v(1.45, 1.5, 0.42), v2(0.92, 0.85), v2(0.0, 0.05), 0.16, BODY, rear);
        slot(w, 0.4, rear * at(0.2, 0.0, -0.22) * Affine3A::from_rotation_y(PI));
    }
}

// --- Shoulders and arms. ---

/// The shoulder armour's block (right side): its corners, inside and out, foot and top, back and
/// front. Wide, its top sweeping up toward its outer end.
const SHOULDER: [(f32, f32, f32); 8] = [
    (2.45, 3.85, -1.3),
    (5.1, 3.95, -1.3),
    (2.45, 5.95, -1.3),
    (5.1, 6.85, -1.3),
    (2.45, 3.85, 1.2),
    (5.1, 3.95, 1.2),
    (2.45, 5.95, 1.2),
    (5.1, 6.85, 1.2),
];

/// The height of the shoulder's top at `x` out (right side), sweeping up outward.
fn shoulder_top(x: f32) -> f32 {
    5.95 + (x - 2.45) * 0.9 / 2.65
}

/// Where the round hatch over the machine cannon sits on the shoulder's front (right side).
const HATCH: Vec3 = Vec3::new(3.55, 5.55, 1.34);

/// The shoulder armour on side `s` at `xf`: the wide white block, its top sweeping up and out under
/// a blue cap that runs out to a red point; a red plate across its front holding the big round
/// yellow hatch, the machine cannon's muzzle in its middle; a plate on its outer face with a blue
/// inset; a running light at its outer front corner; and the grey socket into the chest.
pub(crate) fn shoulder_armour(o: &mut On<'_>, s: Side, xf: Affine3A) {
    o.seed(0.24 + s.pick(0.0, 0.4));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    o.hexa(hexa_of(SHOULDER), 0.32, BODY, flat);
    // The blue cap over the swept top, overhanging it, and the red point at its outer end.
    let (a, b) = (2.55, 5.3);
    let cap = [
        v(a, shoulder_top(a) - 0.1, -1.42),
        v(b, shoulder_top(b) - 0.1, -1.42),
        v(a, shoulder_top(a) + 0.24, -1.42),
        v(b, shoulder_top(b) + 0.24, -1.42),
        v(a, shoulder_top(a) - 0.1, 1.34),
        v(b, shoulder_top(b) - 0.1, 1.34),
        v(a, shoulder_top(a) + 0.24, 1.34),
        v(b, shoulder_top(b) + 0.24, 1.34),
    ];
    o.hexa(cap, 0.1, TRIM, flat);
    let point = [
        v2(4.8, shoulder_top(4.8) - 0.12),
        v2(5.38, 6.45),
        v2(5.45, 7.22),
        v2(5.1, shoulder_top(5.1) + 0.3),
        v2(4.8, shoulder_top(4.8) + 0.26),
    ];
    o.extrude(&point, 0.5, ACCENT, flat * at(0.0, 0.0, 1.0));
    o.extrude(&point, 0.36, TRIM, flat * at(0.0, 0.0, -1.1));
    // The red plate across the front, along the swept top, and the hatch on it.
    let (a, b) = (2.7, 5.0);
    let front = [
        v(a, shoulder_top(a) - 1.5, 1.16),
        v(b, shoulder_top(b) - 1.5, 1.16),
        v(a, shoulder_top(a) - 0.14, 1.16),
        v(b, shoulder_top(b) - 0.14, 1.16),
        v(a, shoulder_top(a) - 1.5, 1.36),
        v(b, shoulder_top(b) - 1.5, 1.36),
        v(a, shoulder_top(a) - 0.14, 1.36),
        v(b, shoulder_top(b) - 0.14, 1.36),
    ];
    o.hexa(front, 0.05, ACCENT, flat);
    let hatch = flat * place(HATCH, rx(FRAC_PI_2));
    o.lathe(&[(0.0, 0.0), (0.62, 0.0), (0.64, 0.06), (0.6, 0.14), (0.0, 0.14)], 24, YELLOW, hatch);
    o.lathe(&band(0.4, 0.5, 0.12, 0.17), 20, FRAME, hatch);
    o.lathe(&[(0.0, 0.12), (0.38, 0.12), (0.36, 0.2), (0.0, 0.22)], 20, YELLOW, hatch);
    o.cube(v(0.3, 0.12, 0.16), 0.02, GREY, hatch * at(0.0, 0.1, -0.66));
    // The machine cannon's muzzle, out of the hatch's middle.
    o.cylinder(0.12, 0.36, 10, GUN, hatch * at(0.0, 0.36, 0.0));
    o.cylinder(0.08, 0.04, 8, FRAME, hatch * at(0.0, 0.55, 0.0));
    // The outer face's raised plate and its blue inset.
    o.cube(v(0.2, 2.0, 2.0), 0.08, BODY, flat * at(5.16, 5.05, -0.05));
    o.cube(v(0.1, 1.0, 0.8), 0.03, TRIM, flat * at(5.28, 4.9, -0.35));
    slot(o, 0.8, flat * place(v(5.27, 5.0, 0.5), ry(FRAC_PI_2)));
    slot(o, 1.2, flat * place(v(3.9, 4.3, 1.21), rz(FRAC_PI_2)));
    // A running light: red on the suit's own left (+x), green on its right.
    let nav = flat * at(4.85, 6.4, 1.25);
    o.cube(v(0.34, 0.16, 0.2), 0.03, GUN, nav);
    let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
    o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));
    o.cylinder(0.45, 0.7, 10, GREY, flat * place(v(2.45, 4.5, 0.0), rz(FRAC_PI_2)));
}

/// The upper arm, up from the elbow.
const UPPER_ARM: [Hoop; 4] = [
    hoop(0.35, 0.0, 0.0, 0.66, 0.66),
    hoop(0.5, 0.0, 0.0, 0.78, 0.78),
    hoop(1.35, 0.0, 0.0, 0.78, 0.78),
    hoop(1.55, 0.0, 0.0, 0.66, 0.66),
];

/// The upper arm on side `s` at `xf`: a white drum with a raised ring, on the grey frame, out from
/// under the shoulder.
pub(crate) fn upper_arm(u: &mut On<'_>, s: Side, xf: Affine3A) {
    let (sh, el) = (j(Bone::UpperArmR), j(Bone::ForearmR));
    u.seed(0.28 + s.pick(0.0, 0.4));
    let up = xf * sided(s, place(el, toward(sh - el)));
    u.sphere(0.6, 8, GREY, xf * sided(s, Affine3A::from_translation(sh)));
    u.cylinder(0.42, 1.6, 12, GREY, up * at(0.0, 1.0, 0.0));
    u.loft(&UPPER_ARM, 2.6, 20, BODY, up);
    u.lathe(&band(0.7, 0.84, 0.85, 1.02), 20, BODY, up);
}

/// The gauntlet, up the forearm from the wrist (x out, z forward): the blue cuff, a step in, then
/// the white box up to the elbow, rounding in over its barrel.
const CUFF: [Hoop; 2] = [hoop(0.0, 0.0, 0.05, 0.86, 0.92), hoop(0.38, 0.0, 0.05, 0.88, 0.94)];
const GAUNTLET: [Hoop; 4] = [
    hoop(0.38, 0.0, 0.05, 0.8, 0.86),
    hoop(1.2, 0.04, 0.03, 0.86, 0.9),
    hoop(1.6, 0.03, 0.0, 0.78, 0.8),
    hoop(1.92, 0.0, 0.0, 0.54, 0.56),
];

/// The forearm on side `s` at `xf`: the elbow's barrel, the gauntlet with its blue cuff and a blue
/// panel on its outer face, and the elbow's guard.
pub(crate) fn forearm(f: &mut On<'_>, s: Side, xf: Affine3A) {
    let (el, wr) = (j(Bone::ForearmR), j(Bone::HandR));
    f.seed(0.33 + s.pick(0.0, 0.4));
    let fo = xf * sided(s, place(wr, toward(el - wr)));
    f.cylinder(0.5, 1.0, 14, GREY, xf * sided(s, place(el, rz(FRAC_PI_2))));
    f.loft(&CUFF, 3.4, 24, TRIM, fo);
    f.loft(&GAUNTLET, 3.4, 24, BODY, fo);
    f.cube(v(0.1, 0.75, 0.8), 0.03, TRIM, fo * at(0.86, 1.05, 0.02));
    let guard = fo * place(v(0.0, 1.75, -0.7), rx(0.25));
    f.block(v(0.9, 0.9, 0.32), v2(0.6, 0.8), Vec2::ZERO, 0.08, BODY, guard);
    f.greeble(|f| {
        f.cube(v(0.05, 0.08, 1.2), 0.0, FRAME, fo * at(0.88, 0.6, 0.05));
        f.cube(v(1.3, 0.08, 0.05), 0.0, FRAME, fo * at(0.0, 0.6, 0.94));
        f.cube(v(0.36, 0.6, 0.05), 0.0, FRAME, fo * at(0.0, 1.2, 0.93));
    });
    f.cylinder(0.46, 0.3, 12, GREY, fo);
}

/// The fist on side `s` at `xf`, grey: a palm and four fingers curled round a grip, and a thumb.
pub(crate) fn fist(h: &mut On<'_>, s: Side, xf: Affine3A) {
    let w = j(Bone::HandR);
    h.seed(0.4 + s.pick(0.0, 0.3));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    h.cube(v(0.8, 1.0, 0.95), 0.08, GREY, flat * at(w.x, w.y - 0.45, w.z + 0.1));
    h.greeble(|h| {
        for f in 0..4 {
            let z = w.z + 0.45 - f as f32 * 0.28;
            h.cube(v(0.3, 0.26, 0.24), 0.04, GREY, flat * place(v(w.x - 0.3, w.y - 1.0, z), rz(0.5)));
        }
        h.cube(v(0.26, 0.5, 0.26), 0.04, GREY, flat * place(v(w.x - 0.2, w.y - 0.55, w.z + 0.62), rx(0.6)));
    });
    h.cylinder(0.42, 0.4, 10, FRAME, flat * at(w.x, w.y + 0.1, w.z));
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): slim and squared off.
const THIGH: [Hoop; 5] = [
    hoop(0.35, 0.04, 0.04, 0.82, 0.82),
    hoop(0.8, 0.08, 0.06, 0.97, 0.93),
    hoop(2.4, 0.1, 0.05, 1.02, 0.97),
    hoop(3.2, 0.08, 0.0, 0.94, 0.9),
    hoop(3.62, 0.05, -0.02, 0.7, 0.68),
];

/// The lower leg's core, up its axis from the ankle to the knee: slim under the knee, its armour
/// flaring broad toward the ankle and over the top of the foot.
const SHIN: [Hoop; 8] = [
    hoop(-0.25, 0.04, 0.14, 1.28, 1.42),
    hoop(0.55, 0.05, 0.12, 1.3, 1.44),
    hoop(1.4, 0.05, 0.06, 1.16, 1.28),
    hoop(2.5, 0.04, -0.04, 1.0, 1.1),
    hoop(3.7, 0.03, -0.1, 0.94, 1.02),
    hoop(4.6, 0.03, -0.12, 0.92, 0.98),
    hoop(5.15, 0.03, -0.1, 0.84, 0.88),
    hoop(5.45, 0.02, -0.08, 0.62, 0.64),
];

/// A plate standing off a limb's core, along its axis (y): across from `x.0` to `x.1` at its foot,
/// narrowing by `narrow` each side to its top; from `y.0` to `y.1`; its back at `z` and its front
/// at `front` (foot, top).
fn limb_plate(x: (f32, f32), y: (f32, f32), z: (f32, f32), front: (f32, f32), narrow: f32) -> [Vec3; 8] {
    hexa_of([
        (x.0, y.0, z.0),
        (x.1, y.0, z.0),
        (x.0 + narrow, y.1, z.1),
        (x.1 - narrow, y.1, z.1),
        (x.0, y.0, front.0),
        (x.1, y.0, front.0),
        (x.0 + narrow, y.1, front.1),
        (x.1 - narrow, y.1, front.1),
    ])
}

/// The thigh on side `s` at `xf`: the slim core on a grey ball at the hip, and the armour standing
/// off it: two angular plates down its front, broken across above the knee, a dark vent high on
/// the upper, and a plate down its outside, each with its edges cut.
pub(crate) fn thigh(th: &mut On<'_>, s: Side, xf: Affine3A) {
    let (hip, knee) = (j(Bone::ThighR), j(Bone::ShinR));
    th.seed(0.31 + s.pick(0.0, 0.2));
    th.sphere(0.74, 6, GREY, xf * sided(s, Affine3A::from_translation(hip)));
    let axis = xf * sided(s, place(knee, toward(hip - knee)));
    th.loft(&THIGH, 4.6, 28, BODY, axis);
    th.hexa(limb_plate((-0.66, 0.84), (0.7, 1.85), (0.66, 0.66), (1.14, 1.14), -0.06), 0.14, BODY, axis);
    th.hexa(limb_plate((-0.68, 0.86), (1.95, 3.05), (0.64, 0.6), (1.12, 1.04), 0.08), 0.14, BODY, axis);
    let vent_at = axis * place(v(0.1, 2.6, 1.13), rx(0.07));
    th.block(v(0.8, 0.42, 0.1), v2(0.7, 1.0), Vec2::ZERO, 0.02, FRAME, vent_at);
    let side = hexa_of([
        (0.9, 0.7, -0.74),
        (1.22, 0.7, -0.74),
        (0.9, 2.9, -0.64),
        (1.18, 2.9, -0.64),
        (0.9, 0.7, 0.72),
        (1.22, 0.7, 0.72),
        (0.9, 2.9, 0.68),
        (1.18, 2.9, 0.68),
    ]);
    th.hexa(side, 0.14, BODY, axis);
    th.greeble(|th| {
        th.cube(v(0.06, 0.6, 0.05), 0.0, FRAME, axis * at(0.55, 1.3, 1.15));
        th.cube(v(0.05, 0.06, 1.0), 0.0, FRAME, axis * at(1.22, 1.5, 0.0));
        th.cube(v(0.05, 1.6, 0.05), 0.0, FRAME, axis * place(v(0.2, 1.9, -0.98), ry(PI)));
    });
}

/// The shin on side `s` at `xf`: the knee's barrel and the core of the lower leg, slim under the
/// knee and flaring broad to the ankle; its armour standing off it: a plate down its front in two,
/// widening as it falls, a plate down its outside flaring with it, a dark vent in it, and the
/// calf's back plated; and the knee guard standing sharp out of the front, its top a point.
pub(crate) fn shin(m: &mut On<'_>, s: Side, xf: Affine3A) {
    let (knee, ankle) = (j(Bone::ShinR), j(Bone::FootR));
    m.seed(0.37 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let tube = xf * sided(s, place(ankle, toward(knee - ankle)));
    m.cylinder(0.6, 1.3, 12, GREY, flat * place(knee, rz(FRAC_PI_2)));
    m.loft(&SHIN, 5.0, 28, BODY, tube);
    m.hexa(limb_plate((-0.92, 0.92), (0.2, 2.2), (1.3, 1.02), (1.72, 1.36), 0.14), 0.16, BODY, tube);
    m.hexa(limb_plate((-0.76, 0.76), (2.3, 4.3), (1.0, 0.66), (1.32, 1.04), 0.1), 0.14, BODY, tube);
    let outside = hexa_of([
        (1.16, 0.2, -1.12),
        (1.46, 0.2, -1.12),
        (0.92, 3.7, -0.9),
        (1.14, 3.7, -0.9),
        (1.16, 0.2, 1.2),
        (1.46, 0.2, 1.2),
        (0.92, 3.7, 0.78),
        (1.14, 3.7, 0.78),
    ]);
    m.hexa(outside, 0.14, BODY, tube);
    let lean = (0.27f32 / 3.5).atan();
    vent(m, 0.7, 0.9, 4, GREY, GREY, tube * place(v(1.38, 1.5, 0.0), rz(lean) * ry(FRAC_PI_2)));
    let calf = hexa_of([
        (-0.72, 1.1, -1.5),
        (0.76, 1.1, -1.5),
        (-0.62, 3.6, -1.22),
        (0.66, 3.6, -1.22),
        (-0.72, 1.1, -1.05),
        (0.76, 1.1, -1.05),
        (-0.62, 3.6, -0.8),
        (0.66, 3.6, -0.8),
    ]);
    m.hexa(calf, 0.14, BODY, tube);
    m.greeble(|m| {
        m.cube(v(1.2, 0.06, 0.05), 0.0, FRAME, tube * place(v(0.0, 2.25, 1.38), rx(-0.17)));
        m.cube(v(0.06, 0.9, 0.05), 0.0, FRAME, tube * place(v(-0.45, 3.3, 1.18), rx(-0.14)));
        m.cube(v(0.06, 1.1, 0.05), 0.0, FRAME, tube * place(v(0.3, 2.4, -1.4), ry(PI) * rx(-0.11)));
    });
    // The ankle's front: a dark slot in the flare over the foot.
    m.cube(v(0.5, 0.36, 0.12), 0.02, FRAME, tube * place(v(0.0, 0.75, 1.68), rx(-0.08)));
    let guard = flat * Affine3A::from_translation(knee);
    let cap = hexa_of([
        (-0.66, -1.1, 0.45),
        (0.66, -1.1, 0.45),
        (-0.54, 1.0, 0.3),
        (0.54, 1.0, 0.3),
        (-0.66, -1.1, 1.42),
        (0.66, -1.1, 1.42),
        (-0.54, 1.0, 1.2),
        (0.54, 1.0, 1.2),
    ]);
    m.hexa(cap, 0.14, BODY, guard);
    let point = hexa_of([
        (-0.54, 1.0, 0.3),
        (0.54, 1.0, 0.3),
        (-0.08, 1.75, 0.38),
        (0.08, 1.75, 0.38),
        (-0.54, 1.0, 1.2),
        (0.54, 1.0, 1.2),
        (-0.08, 1.75, 0.86),
        (0.08, 1.75, 0.86),
    ]);
    m.hexa(point, 0.06, BODY, guard);
    let tilt = (0.22f32 / 2.1).atan();
    let panel = guard * place(v(0.0, -0.2, 1.36), rx(-tilt));
    m.block(v(0.56, 1.1, 0.12), v2(0.7, 1.0), Vec2::ZERO, 0.03, GREY, panel);
}

/// How far a foot's sides lean in, per metre up.
const FOOT_LEAN: f32 = 0.16;
/// The foot's base, a little over the sole's plate, and the top of the instep at the toes' hinge.
const FOOT_BASE: f32 = SOLE + 0.28;
const INSTEP: f32 = -8.1;

/// The foot on side `s` at `xf`, but for its sole: red, the grey ball of the ankle, a tall heel, the
/// instep falling from it to the toes' hinge, a spur up the heel's back, seams.
pub(crate) fn heel(f: &mut On<'_>, s: Side, xf: Affine3A) {
    let (ankle, hinge) = (j(Bone::FootR), j(Bone::ToeR));
    let x = ankle.x;
    f.seed(0.43 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    f.sphere(0.5, 5, GREY, flat * Affine3A::from_translation(ankle));
    let back = [-2.0, -1.72, -0.25, -0.25];
    f.hexa(boot(x, FOOT_BASE, (-7.7, -7.75), (1.2, 1.25), back, FOOT_LEAN), 0.26, ACCENT, flat);
    let instep = [-0.3, -0.3, hinge.z - 0.02, hinge.z - 0.1];
    f.hexa(boot(x, FOOT_BASE, (-7.82, INSTEP), (1.25, 1.25), instep, FOOT_LEAN), 0.2, ACCENT, flat);
    let spur = [v2(0.0, 0.0), v2(0.7, 0.0), v2(0.4, 0.34), v2(-0.34, 0.7), v2(-0.18, 0.24)];
    f.extrude(&spur, 0.26, ACCENT, flat * place(v(x, -7.82, -1.7), ry(FRAC_PI_2)));
    f.greeble(|f| {
        f.cube(v(2.0, 0.05, 0.05), 0.0, FRAME, flat * at(x, -7.76, -0.28));
        for side in [-1.0, 1.0] {
            let face = flat * place(v(x + side * 1.16, -8.3, -1.0), rz(side * FOOT_LEAN.atan()));
            f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, face);
        }
    });
}

/// The toe cap on side `s` at `xf`, but for its sole: red, falling and narrowing from the hinge to
/// its blunt nose, a white plate along its top.
pub(crate) fn toe_cap(t: &mut On<'_>, s: Side, xf: Affine3A) {
    let hinge = j(Bone::ToeR);
    let x = hinge.x;
    t.seed(0.47 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let cap = [hinge.z + 0.02, hinge.z + 0.02, 3.3, 2.95];
    t.hexa(boot(x, FOOT_BASE, (INSTEP - 0.02, -8.5), (1.25, 0.86), cap, FOOT_LEAN), 0.26, ACCENT, flat);
    t.hexa(boot(x, -8.4, (-7.98, -8.32), (0.56, 0.42), [1.45, 1.5, 2.8, 2.62], 0.0), 0.06, BODY, flat);
    t.greeble(|t| {
        t.cube(v(1.6, 0.05, 0.05), 0.0, FRAME, flat * place(v(x, -8.36, 2.9), rx(0.3)));
    });
}

/// The soles under the heel and under the toes (right foot).
pub(crate) const HEEL_SOLE: Sole =
    Sole { z: (-2.06, 1.26), half: (1.27, 1.31), top: 0.28, pads: GREY, vernier: -0.7, r: 0.4 };
pub(crate) const TOE_SOLE: Sole =
    Sole { z: (1.34, 3.4), half: (1.31, 0.92), top: 0.28, pads: GREY, vernier: 2.3, r: 0.32 };

/// A sole as [`frames::sole`](crate::frames::sole) draws one, on side `s` at `xf`, centred on `x`,
/// but with no socket: a foot folded away doesn't fire its verniers.
pub(crate) fn quiet_sole(o: &mut On<'_>, s: Side, x: f32, sole: Sole, xf: Affine3A) {
    let Sole { z, half, top, pads, vernier: at_, r } = sole;
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let width = |w: f32| half.0 + (half.1 - half.0) * (w - z.0) / (z.1 - z.0);
    o.hexa(
        boot(x, SOLE + 0.05, (SOLE + top, SOLE + top), half, [z.0, z.0, z.1, z.1], 0.0),
        0.05,
        FRAME,
        flat,
    );
    let inset = 0.12;
    for (a, b) in [(z.0 + inset, at_ - r - 0.08), (at_ + r + 0.08, z.1 - inset)] {
        if b - a < 0.15 {
            continue;
        }
        let pad = boot(
            x,
            SOLE,
            (SOLE + 0.07, SOLE + 0.07),
            (width(a) - inset, width(b) - inset),
            [a, a, b, b],
            0.0,
        );
        o.hexa(pad, 0.03, pads, flat);
    }
    let mouth = flat * at(x, SOLE + 0.015, at_);
    o.lathe(&band(r * 0.62, r, 0.0, 0.07), 16, TRIM, mouth);
    o.cylinder(r * 0.64, 0.06, 14, FRAME, mouth * at(0.0, 0.04, 0.0));
    o.cylinder(r * 0.3, 0.02, 10, THROAT, mouth * at(0.0, 0.012, 0.0));
}

// --- Backpack and wings. ---

/// The backpack, up the back from under the thrusters to its top.
const PACK: [Hoop; 4] = [
    hoop(2.95, 0.0, -2.05, 1.0, 0.55),
    hoop(3.25, 0.0, -2.12, 1.22, 0.68),
    hoop(5.05, 0.0, -2.12, 1.28, 0.7),
    hoop(5.45, 0.0, -2.05, 1.0, 0.56),
];

/// The backpack at `xf`, but for its thrusters: the pack, a blue spine down its back, a grey skirt
/// over the thrusters, a grille in its top, the binders' pivots at its top corners and blue plates
/// down its sides.
pub(crate) fn backpack(b: &mut On<'_>, xf: Affine3A) {
    b.seed(0.5);
    b.loft(&PACK, 3.4, 24, BODY, xf);
    b.cube(v(0.8, 1.9, 0.14), 0.04, TRIM, xf * at(0.0, 4.25, -2.84));
    slot(b, 1.2, xf * place(v(0.0, 4.25, -2.92), ry(PI)));
    b.block(v(2.0, 0.34, 1.2), v2(0.92, 0.9), Vec2::ZERO, 0.06, GREY, xf * at(0.0, 3.02, -2.2));
    vent(b, 1.3, 0.6, 4, GREY, GREY, xf * place(v(0.0, 5.44, -2.12), rx(-FRAC_PI_2)));
    for s in Side::BOTH {
        let flat = xf * sided(s, Affine3A::IDENTITY);
        b.lathe(&band(0.0, 0.5, 0.0, 0.3), 16, GREY, flat * place(v(1.2, 4.9, -2.75), rx(-FRAC_PI_2)));
        b.cube(v(0.5, 1.4, 0.4), 0.06, TRIM, flat * at(1.18, 3.9, -2.62));
        slot(b, 0.8, flat * place(v(1.3, 4.0, -1.8), ry(FRAC_PI_2)));
    }
}

/// Where each wing hangs from its binder (right side): the hinge behind the shoulder's top.
pub(crate) const HINGE: Vec3 = Vec3::new(4.0, 6.3, -3.95);

/// A wing's own frame at the hinge (right side): x out and back along its chord, y up it, z out of
/// its outer face, which looks out and forward past the shoulder.
pub(crate) fn wing_plane() -> Affine3A {
    let chord = v(0.78, 0.0, -0.62).normalize();
    Affine3A::from_mat3_translation(Mat3::from_cols(chord, Vec3::Y, chord.cross(Vec3::Y)), HINGE)
}

/// The wing's outline, in its frame (x out along the chord from the leading edge, y up from the
/// hinge): its top sweeping up and out to a tip high beside the head, the trailing edge stepping
/// down from it, the leading edge straight down to a pointed foot by the knee.
const WING: [Vec2; 10] = [
    Vec2::new(0.0, 1.2),
    Vec2::new(0.0, -6.6),
    Vec2::new(0.5, -8.4),
    Vec2::new(1.75, -6.0),
    Vec2::new(2.55, -2.6),
    Vec2::new(2.7, 0.5),
    Vec2::new(3.1, 1.6),
    Vec2::new(2.85, 4.1),
    Vec2::new(2.3, 3.8),
    Vec2::new(0.9, 2.2),
];
/// The blue tip over the hinge, the red band under it, a raised plate down the wing's middle, the
/// red stripe down the leading edge, and the smaller plate that steps out behind the wing, inside
/// it.
const WING_TIP: [Vec2; 7] = [
    Vec2::new(0.08, 1.15),
    Vec2::new(0.08, 0.75),
    Vec2::new(3.04, 1.62),
    Vec2::new(2.82, 3.98),
    Vec2::new(2.32, 3.68),
    Vec2::new(0.92, 2.1),
    Vec2::new(0.4, 1.55),
];
const WING_BAND: [Vec2; 4] =
    [Vec2::new(0.08, 0.58), Vec2::new(2.95, 1.42), Vec2::new(2.88, 1.02), Vec2::new(0.08, 0.18)];
const WING_PLATE: [Vec2; 5] = [
    Vec2::new(0.95, -0.2),
    Vec2::new(2.45, 0.2),
    Vec2::new(2.35, -2.6),
    Vec2::new(1.6, -5.4),
    Vec2::new(0.95, -4.8),
];
const WING_STRIPE: [Vec2; 5] = [
    Vec2::new(0.06, -1.0),
    Vec2::new(0.75, -1.1),
    Vec2::new(0.75, -5.0),
    Vec2::new(0.48, -6.8),
    Vec2::new(0.06, -6.4),
];
const SUB_WING: [Vec2; 6] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.9, -0.2),
    Vec2::new(2.2, -1.6),
    Vec2::new(1.6, -5.0),
    Vec2::new(0.6, -6.3),
    Vec2::new(0.0, -5.0),
];
/// The main wing's and the smaller plate's thickness.
const WING_DEPTH: f32 = 0.44;
const SUB_DEPTH: f32 = 0.32;

/// A wing binder on side `s`, its arm at `arm_xf` and its wing at `wing_xf` (each its rest place in
/// the suit's frame): the arm up and out from the pivot on the pack to the hinge behind the
/// shoulder's top, two thruster bells at its root; the grey hinge; and the wing hanging from it,
/// solid plates stepping like a folded wing: the main wing, white, its blue tip rising over the
/// shoulder, a red stripe down its leading edge and a vent high in its face, the smaller plate
/// stepping out behind it with a blue edge.
pub(crate) fn binder(o: &mut On<'_>, s: Side, arm_xf: Affine3A, wing_xf: Affine3A) {
    o.seed(0.55 + s.pick(0.0, 0.2));
    let root = j(Bone::WingR);
    let flat = arm_xf * sided(s, Affine3A::IDENTITY);
    let arm = flat * along(root + v(0.0, 0.0, -0.1), HINGE);
    let len = (HINGE - root).length();
    o.lathe(&band(0.3, 0.46, -0.24, 0.24), 14, GREY, flat * place(root, rx(-FRAC_PI_2)));
    o.block(v(0.95, len + 0.3, 0.85), v2(0.82, 0.9), Vec2::ZERO, 0.12, BODY, arm);
    o.cube(v(0.12, len * 0.62, 0.62), 0.03, TRIM, arm * at(0.48, -0.1, 0.0));
    o.cube(v(0.12, len * 0.4, 0.4), 0.03, ACCENT, arm * at(-0.48, 0.2, 0.0));
    for (up, out) in [(-0.9, 0.0), (-0.4, 0.42)] {
        bell(o, 0.3, arm * place(v(out - 0.2, up, -0.42), toward(v(0.1, -0.5, -0.86))));
    }
    let plane = wing_xf * sided(s, wing_plane());
    o.cylinder(0.55, 1.0, 16, GREY, plane * place(v(0.3, 0.0, 0.0), rx(FRAC_PI_2)));
    o.cylinder(0.3, 1.1, 12, BODY, plane * place(v(0.3, 0.0, 0.0), rx(FRAC_PI_2)));
    let half = WING_DEPTH * 0.5;
    o.extrude(&WING, WING_DEPTH, BODY, plane);
    o.extrude(&WING_TIP, 0.1, TRIM, plane * at(0.0, 0.0, half + 0.02));
    o.extrude(&WING_BAND, 0.1, ACCENT, plane * at(0.0, 0.0, half + 0.02));
    o.extrude(&WING_STRIPE, 0.1, ACCENT, plane * at(0.0, 0.0, half + 0.02));
    o.extrude(&WING_PLATE, 0.12, BODY, plane * at(0.0, 0.0, half + 0.03));
    vent(o, 0.8, 0.7, 3, GREY, GREY, plane * at(1.7, -1.5, half + 0.1));
    o.greeble(|o| {
        for (x, y, len, turn) in [(1.45, -3.4, 1.4, 0.0), (1.75, -2.6, 0.8, FRAC_PI_2), (1.0, -6.0, 1.0, 0.3)]
        {
            o.cube(v(0.07, len, 0.05), 0.0, FRAME, plane * place(v(x, y, half + 0.1), rz(turn)));
        }
    });
    let sub = plane * at(0.8, -1.4, -(half + SUB_DEPTH * 0.5 + 0.12));
    o.extrude(&SUB_WING, SUB_DEPTH, BODY, sub);
    let edge = [v2(0.06, -0.3), v2(0.42, -0.36), v2(0.42, -4.6), v2(0.06, -4.9)];
    o.extrude(&edge, 0.08, TRIM, sub * at(0.0, 0.0, -(SUB_DEPTH * 0.5 + 0.02)));
}

// --- The Twin Buster Rifle. ---

/// The Twin Buster Rifle's two halves either side of the grip (x), and how high their barrels ride
/// over it.
const HALF_X: f32 = 0.56;
const BORE_Y: f32 = 0.66;
/// How high each half's energy cartridge rides over its barrel: over the forearm behind the fist
/// at the ready ([`crate::hold`]), and ahead of it carried; and how far behind the grip it reaches.
const CART_UP: f32 = 0.92;
const REAR: f32 = -1.7;
/// How far ahead of the grip the muzzles are: as far as the rifle can hang muzzle down from the
/// fist and stay clear of the floor.
const MUZZLE: f32 = 9.0;

/// One of the Twin Buster Rifle's halves, a whole rifle, at `xf` (its barrel along +z from over the
/// grip at the origin, up +y), `out` (±1) the side away from its twin: the long grey receiver from
/// just ahead of the grip, banded yellow, a light deck along its top, a panel and a yellow stripe
/// down its outside; the energy cartridge riding high on a mount over its rear, banded and capped;
/// the shroud, the long round barrel with a light collar and the stepped muzzle.
pub(crate) fn buster(o: &mut On<'_>, out: f32, xf: Affine3A) {
    let along_z = |z: f32| xf * place(v(0.0, 0.0, z), rx(FRAC_PI_2));
    o.block(v(0.8, 0.94, 4.3), v2(0.9, 0.88), Vec2::ZERO, 0.12, GUN, xf * at(0.0, 0.0, 2.3));
    for z in [0.75, 3.95] {
        o.cube(v(0.86, 0.98, 0.22), 0.03, YELLOW, xf * at(0.0, -0.02, z));
    }
    o.cube(v(0.54, 0.1, 2.6), 0.02, LIGHT, xf * at(0.0, 0.47, 2.35));
    o.cube(v(0.08, 0.56, 2.4), 0.02, GREY, xf * at(out * 0.42, 0.0, 2.35));
    o.cube(v(0.06, 0.14, 2.0), 0.0, YELLOW, xf * at(out * 0.46, -0.22, 2.35));
    let cart = xf * place(v(0.0, CART_UP, REAR + 1.3), rx(FRAC_PI_2));
    o.cylinder(0.4, 2.6, 14, GREY, cart);
    o.lathe(&band(0.34, 0.44, 0.8, 1.0), 14, YELLOW, cart);
    o.lathe(&band(0.34, 0.44, -0.9, -0.75), 14, YELLOW, cart);
    o.lathe(&[(0.0, -1.38), (0.3, -1.38), (0.35, -1.3), (0.0, -1.3)], 12, GUN, cart);
    o.cube(v(0.4, 0.4, 0.8), 0.03, GUN, xf * at(0.0, 0.6, 0.5));
    o.cylinder(0.38, 1.3, 14, GUN, along_z(5.1));
    o.lathe(&band(0.34, 0.44, -0.08, 0.08), 14, YELLOW, along_z(4.7));
    o.cylinder(0.26, 3.3, 12, GUN, along_z(7.3));
    o.lathe(&band(0.24, 0.32, -0.25, 0.25), 12, LIGHT, along_z(6.6));
    let tip = [(0.0, 0.0), (0.34, 0.0), (0.36, 0.06), (0.36, 0.42), (0.3, 0.48), (0.0, 0.48)];
    o.lathe(&tip, 14, GUN, along_z(MUZZLE - 0.48));
    o.lathe(&band(0.34, 0.4, 0.1, 0.22), 14, LIGHT, along_z(MUZZLE - 0.48));
    o.cylinder(0.2, 0.04, 12, FRAME, along_z(MUZZLE - 0.02));
    o.greeble(|o| {
        for k in 0..5 {
            o.cube(v(0.4, 0.06, 0.16), 0.0, FRAME, xf * at(0.0, 0.53, 1.4 + k as f32 * 0.4));
        }
        o.cylinder(0.14, 0.06, 10, FRAME, along_z(MUZZLE));
        o.cube(v(0.05, 0.3, 1.6), 0.0, FRAME, xf * at(out * 0.41, 0.2, 3.1));
    });
}

/// The Twin Buster Rifle at `xf` (the grip at the origin, barrels along +z): its two halves side by
/// side, clamped together by light brackets (between the receivers, between the cartridges and
/// round the barrels), the grip and its guard under the join. Returns where its beam leaves,
/// between the muzzles, in the suit's frame.
pub(crate) fn twin_rifle(o: &mut On<'_>, xf: Affine3A) -> Vec3 {
    o.seed(0.61);
    for side in [-1.0f32, 1.0] {
        buster(o, side, xf * at(side * HALF_X, BORE_Y, 0.0));
    }
    o.block(v(0.5, 0.6, 1.2), v2(1.0, 0.9), Vec2::ZERO, 0.05, LIGHT, xf * at(0.0, BORE_Y, 1.5));
    o.block(v(0.5, 0.6, 0.8), v2(1.0, 0.9), Vec2::ZERO, 0.05, LIGHT, xf * at(0.0, BORE_Y, 3.6));
    o.cube(v(0.5, 0.3, 0.9), 0.04, LIGHT, xf * at(0.0, BORE_Y + CART_UP, 0.0));
    o.cube(v(1.9, 0.7, 0.44), 0.06, LIGHT, xf * at(0.0, BORE_Y, 6.0));
    o.cube(v(0.5, 0.3, 0.3), 0.03, GUN, xf * at(0.0, BORE_Y + 0.42, 6.0));
    o.cube(v(0.34, 0.86, 0.46), 0.05, GUN, xf * place(v(0.0, -0.2, 0.32), rx(-0.08)));
    o.cube(v(0.3, 0.12, 0.6), 0.02, GUN, xf * at(0.0, -0.1, 0.62));
    o.cube(v(0.28, 0.5, 0.12), 0.02, GUN, xf * at(0.0, 0.16, 0.8));
    xf.transform_point3(v(0.0, BORE_Y, MUZZLE + 0.05))
}

// --- Shield and saber. ---

/// The shield's outline, in its own plane (x across, y along it, its point down): long, its top
/// squared, tapering to the point.
const SHIELD: [Vec2; 12] = [
    Vec2::new(0.0, 3.3),
    Vec2::new(0.72, 3.2),
    Vec2::new(1.0, 2.7),
    Vec2::new(1.0, 0.4),
    Vec2::new(0.62, -1.6),
    Vec2::new(0.22, -2.9),
    Vec2::new(0.0, -3.3),
    Vec2::new(-0.22, -2.9),
    Vec2::new(-0.62, -1.6),
    Vec2::new(-1.0, 0.4),
    Vec2::new(-1.0, 2.7),
    Vec2::new(-0.72, 3.2),
];

/// Where the shield hangs on the left forearm: outside it, its face outward (-x), its point down
/// toward the knee and a little forward.
fn shield_place() -> Affine3A {
    place(v(-4.75, -0.15, 0.95), Quat::from_rotation_arc(Vec3::Z, v(-0.86, 0.0, 0.5).normalize()) * rz(0.12))
}

/// The shield at `xf` (its face looking along +z, its point down -y): a white plate, the red face
/// raised in it, a white band across its top, a ridge down its middle, the yellow point with a
/// green light; a grey mount behind.
pub(crate) fn shield(o: &mut On<'_>, xf: Affine3A) {
    o.seed(0.71);
    o.extrude(&SHIELD, 0.34, BODY, xf);
    let face: Vec<Vec2> = SHIELD.iter().map(|p| v2(p.x * 0.7, p.y * 0.74 + 0.3)).collect();
    o.extrude(&face, 0.24, ACCENT, xf * at(0.0, 0.0, 0.16));
    let tip = [v2(-0.52, -1.75), v2(0.52, -1.75), v2(0.22, -2.95), v2(0.0, -3.34), v2(-0.22, -2.95)];
    o.extrude(&tip, 0.38, YELLOW, xf * at(0.0, 0.0, 0.03));
    o.cube(v(0.16, 0.16, 0.1), 0.02, EYE, xf * at(0.0, -2.5, 0.25));
    let top = [v2(-0.82, 2.5), v2(0.82, 2.5), v2(0.62, 3.05), v2(0.0, 3.15), v2(-0.62, 3.05)];
    o.extrude(&top, 0.28, BODY, xf * at(0.0, 0.0, 0.2));
    let ridge = [v2(-0.1, 2.3), v2(0.1, 2.3), v2(0.1, -1.3), v2(0.0, -1.6), v2(-0.1, -1.3)];
    o.extrude(&ridge, 0.36, BODY, xf * at(0.0, 0.0, 0.24));
    o.cube(v(0.8, 1.4, 0.5), 0.06, GREY, xf * at(0.0, 1.0, -0.35));
}

/// The beam saber's hilt in the left hand, its emitter up and forward.
fn saber(d: &mut Designer) {
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.55, w.z + 0.2);
    let dir = v(0.0, 0.62, 0.78).normalize();
    let xf = place(hilt, toward(dir));
    let mut h = d.on(Bone::HandL);
    h.cylinder(0.17, 1.2, 10, GUN, xf);
    h.lathe(&[(0.0, 0.6), (0.2, 0.6), (0.24, 0.7), (0.2, 0.85), (0.0, 0.85)], 12, STEEL, xf);
    h.cylinder(0.2, 0.14, 10, GREY, xf * at(0.0, -0.62, 0.0));
    h.greeble(|h| {
        for k in 0..3 {
            h.cylinder(0.19, 0.06, 10, FRAME, xf * at(0.0, -0.3 + k as f32 * 0.2, 0.0));
        }
    });
    d.sockets.saber = (Designer::local(Bone::HandL, hilt + dir * 0.8), dir);
}
