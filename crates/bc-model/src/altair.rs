//! PX-00 Altair, the colonies' hero suit: sturdy and squared off under four great white wings, a
//! small head set low between high, broad shoulders, a narrow waist and long, heavy legs, built as
//! a model kit of its design is. Part by part:
//!
//! - a small white head: a rounded helmet with a ridge over the crown, the brow overhanging green
//!   eyes that slant up to their outer corners, the tall yellow V-fin swept back out of a red crest
//!   with its green camera, a white mask with two vents over the red chin, cheek guards either side
//!   and a round ear on each side of the helmet;
//! - a blue chest in two tiers, the lower swelling forward and coming down to a point over the
//!   abdomen, the upper leaning back to the collar: the hatch raised down its middle, a yellow
//!   louvred vent either side of it, a white gorget under the chin and the collar round the back of
//!   the neck; blue plates down the back beside the pack;
//! - the machine cannons in white housings with blue lids on the chest's top corners, under the
//!   shoulders' inner edges, their barrels out of dark ports;
//! - shoulders that are tall white blocks, a little wider at the top, their edges cut deep: a lid
//!   raised over the top, a plate on the outer face with a blue inset and a yellow vent, a plate on
//!   the front, a blue lip round the foot and a running light at the top front corner;
//! - white upper arms with a raised ring; heavy gauntlets with blue cuffs, a blue panel outside
//!   and a guard over the elbow; grey fists;
//! - the abdomen's white plates on the grey frame; a white belt with a blue buckle, the blue
//!   crotch with its red V, front skirts with blue hems, side skirts with blue panels, and the rear
//!   skirt;
//! - heavy legs, armour plated over lofted cores: the thighs with two plates down the front,
//!   broken above the knee, and a plate down the outside; a tall knee guard with a pointed top and a
//!   blue panel in its face; full calves plated down the front, down the outside (a yellow vent in
//!   it) and down the back in two; the boot's shaft round the ankle with the blue tongue up its
//!   front and a grey hub either side; and big feet: a tall heel with a spur up its back, the instep
//!   falling to the toes' hinge, the toe cap on its own bone with a red ridge along it, and under
//!   both the soles with a vernier under the heel and under the toes;
//! - the pack on the back, its two thrusters under a grey skirt and a grille in its top, and on its
//!   top corners the two wing binders ([`Bone::WingL`], [`Bone::WingR`], which the client flares on
//!   boost), each with a vernier in its arm and two feathered wings off its knuckle: the upper
//!   rising over the shoulder as an angel's, its long feathers fanning out and down from the wrist
//!   and hanging from the spar, coverts over their roots; the lower out and down beside the legs;
//! - the Heavy Beam Rifle, two rifles joined side by side, held as [`crate::hold`] has it: each a
//!   receiver with a white panel from just ahead of the grip, a vented shroud between yellow bands,
//!   the barrel and a flared muzzle, and a blue energy pack riding on the receiver's rear, over the
//!   fist and back over the forearm at the ready, standing ahead of it carried; the grip and its
//!   guard under the join, the scope over the packs, clamps round the barrels. It is no longer
//!   than it can hang muzzle down from the fist and clear the toes;
//! - the shield on the left forearm, pointed: white, its red face raised in it, a blue cap across
//!   its top and a ridge down its middle; and the beam saber's hilt in the left hand.
//!
//! The livery paints the body white, the trim blue and the accent red, the eyes, the crest's camera
//! green; the vents, the V-fin and the rifle's bands are yellow. The inner frame, the fists, the
//! hubs and the binders' joints are grey, the weapons gunmetal.
//!
//! Each part is drawn through a placement in the suit's frame, so the Aquila ([`crate::aquila`])
//! folds the very same parts into its flight form.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::detail::{GREY, LENS, band, slot, toward};
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, SOLE, STEEL, Sole, THROAT, TRIM, YELLOW, along, at, boot, j, nozzle,
    place, rx, ry, rz, sided, sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::rig::{Bone, Side};
use crate::{Designer, On};

pub(crate) fn altair(d: &mut Designer) {
    let rest = Affine3A::IDENTITY;
    head(&mut d.on(Bone::Head), rest);
    d.head_base = FACE_FOOT + 0.08;
    let mut c = d.on(Bone::Chest);
    chest(&mut c, rest);
    for s in Side::BOTH {
        cannon(&mut c, sided(s, Affine3A::from_translation(CANNON)));
    }
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
        binder(&mut d.on(s.pick(Bone::WingL, Bone::WingR)), s, rest, &UPPER_WING, &LOWER_WING);
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
    h.block(
        v(0.34, 0.16, 0.44),
        v2(1.25, 1.0),
        Vec2::ZERO,
        0.04,
        ACCENT,
        xf * at(0.0, FACE_FOOT + 0.08, 0.9),
    );
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
        // The V-fin, leaning back from the forehead.
        h.extrude(&V_FIN, 0.1, YELLOW, flat * place(v(0.04, 7.2, 1.06), rx(-0.3)));
    }
    // The crest on the forehead, red, its camera lit green.
    h.block(
        v(0.26, 0.3, 0.34),
        v2(0.7, 0.75),
        v2(0.0, -0.03),
        0.04,
        ACCENT,
        xf * place(v(0.0, 7.24, 1.0), rx(-0.3)),
    );
    h.cube(v(0.12, 0.1, 0.06), 0.02, EYE, xf * place(v(0.0, 7.2, 1.17), rx(-0.3)));
    // A ridge over the crown from the crest back, and down the back of the helmet.
    h.block(
        v(0.28, 0.14, 1.1),
        v2(0.8, 0.9),
        Vec2::ZERO,
        0.04,
        BODY,
        xf * place(v(0.0, 7.53, 0.12), rx(0.12)),
    );
    h.block(
        v(0.34, 0.9, 0.3),
        v2(0.7, 0.9),
        Vec2::ZERO,
        0.06,
        BODY,
        xf * place(v(0.0, 6.95, -0.74), rx(0.15)),
    );
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half), behind its plate: (y, outer x, back z, front z) at its foot and
/// its top.
const CORE: [(f32, f32, f32, f32); 2] = [(3.3, 1.5, -1.25, 0.6), (5.85, 1.95, -1.4, 0.6)];

/// The blue chest plate's two tiers (right half), each its corners in front view (x, y): its foot
/// inside and outside, its top inside and outside. The lower tier's foot rises outward from the
/// middle, so the plate comes down to a point over the abdomen.
const PLATE: [[(f32, f32); 4]; 2] = [
    [(0.0, 3.35), (1.75, 3.85), (0.0, 4.55), (2.15, 4.55)],
    [(0.0, 4.55), (2.15, 4.55), (0.0, 5.85), (2.25, 5.85)],
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

/// The machine cannon's housing on the chest's top beside the shoulder (right side): its centre.
const CANNON: Vec3 = Vec3::new(2.25, 5.88, -0.05);

/// The chest at `xf`: the blue core and its plate in two tiers, the hatch raised down its middle,
/// the yellow vents, the shoulders' sockets, the plates on its back, the gorget under the chin and
/// the collar round the neck.
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
        // The hatch down the middle, raised off both tiers.
        for (k, (ya, yb), (wa, wb)) in [(0, (3.55, 4.55), (0.3, 0.42)), (1, (4.55, 5.5), (0.42, 0.32))] {
            let at_ = |x: f32, y: f32, out: f32| {
                let (p, n) = plate_front(k, x, y);
                p + n * out
            };
            let panel = [
                at_(0.0, ya, -0.06),
                at_(wa, ya, -0.06),
                at_(0.0, yb, -0.06),
                at_(wb, yb, -0.06),
                at_(0.0, ya, 0.12),
                at_(wa, ya, 0.12),
                at_(0.0, yb, 0.12),
                at_(wb, yb, 0.12),
            ];
            c.hexa(panel, 0.04, TRIM, flat);
        }
        // The yellow vents in the upper tier, either side of the hatch, and seams across the lower.
        vent(c, 1.2, 0.8, 4, YELLOW, YELLOW, flat * on_plate(1, 1.22, 5.08) * Affine3A::from_rotation_z(0.1));
        c.greeble(|c| {
            c.cube(v(1.4, 0.06, 0.05), 0.0, FRAME, flat * on_plate(0, 1.15, 4.35));
            c.cube(v(0.06, 0.6, 0.05), 0.0, FRAME, flat * on_plate(0, 1.4, 4.0));
        });
        // The shoulder's socket in the chest's side, and the back's plates beside the backpack.
        c.cylinder(0.6, 0.6, 14, GREY, flat * place(v(2.3, 4.5, 0.0), rz(FRAC_PI_2)));
        let plate = flat * place(v(1.4, 4.6, -1.5), ry(-0.2));
        c.block(v(0.9, 2.0, 0.34), v2(0.85, 0.85), Vec2::ZERO, 0.14, TRIM, plate);
        slot(c, 0.5, plate * at(0.15, 0.2, -0.18) * Affine3A::from_rotation_y(PI));
    }
    // The gorget across the chest's top under the chin, and the collar round the back of the neck.
    c.block(v(1.7, 0.36, 0.7), v2(0.9, 0.8), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 5.92, 0.68));
    let (from, to) = (FRAC_PI_2 + 0.8, FRAC_PI_2 - 0.8 + TAU);
    c.lathe_arc(&band(0.55, 0.95, 5.8, 6.06), 20, from, to, BODY, xf);
    c.lathe_arc(&band(0.5, 0.78, 6.06, 6.2), 20, from, to, GREY, xf);
}

/// A machine cannon in its housing at `xf` (centred, its barrel along +z): a white box with a blue
/// lid, the barrel out of a dark port in its front, its muzzle ringed.
pub(crate) fn cannon(c: &mut On<'_>, xf: Affine3A) {
    c.block(v(0.78, 0.56, 2.0), v2(0.9, 0.92), Vec2::ZERO, 0.1, BODY, xf);
    c.cube(v(0.6, 0.1, 1.5), 0.03, TRIM, xf * at(0.0, 0.3, -0.12));
    c.cube(v(0.44, 0.36, 0.08), 0.02, FRAME, xf * at(0.0, -0.04, 1.0));
    let barrel = xf * place(v(0.0, -0.04, 1.3), rx(FRAC_PI_2));
    c.cylinder(0.12, 0.7, 10, GUN, barrel);
    c.lathe(&band(0.08, 0.17, 0.25, 0.42), 12, GUN, barrel);
    c.cylinder(0.08, 0.04, 8, FRAME, barrel * at(0.0, 0.43, 0.0));
    c.greeble(|c| {
        for k in 0..3 {
            for x in [-0.39, 0.39] {
                c.cube(v(0.05, 0.05, 0.4), 0.0, FRAME, xf * at(x, -0.1 + k as f32 * 0.12, 0.0));
            }
        }
    });
}

/// The abdomen at `xf`: the grey frame, white plates across its front and down its sides.
pub(crate) fn abdomen(a: &mut On<'_>, xf: Affine3A) {
    a.seed(0.17);
    let squash = xf * Affine3A::from_scale(v(1.0, 1.0, 0.85));
    a.lathe(&[(0.0, 2.3), (1.12, 2.3), (1.2, 2.5), (1.1, 3.5), (0.0, 3.5)], 14, GREY, squash);
    a.block(v(1.5, 0.42, 0.5), v2(0.94, 0.9), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 2.71, 0.8));
    a.block(v(1.36, 0.38, 0.5), v2(0.94, 0.9), Vec2::ZERO, 0.08, BODY, xf * at(0.0, 3.16, 0.8));
    for s in Side::BOTH {
        let side = xf * sided(s, place(v(1.0, 2.9, 0.1), rz(0.1)));
        a.block(v(0.4, 0.9, 1.3), v2(0.8, 0.9), Vec2::ZERO, 0.08, BODY, side);
    }
}

// --- Waist and skirts. ---

/// The belt round the hips.
const BELT: [Hoop; 3] =
    [hoop(1.9, 0.0, 0.0, 1.48, 1.1), hoop(2.0, 0.0, 0.0, 1.58, 1.18), hoop(2.45, 0.0, 0.0, 1.58, 1.18)];

/// The waist at `xf`: the belt and its buckle, the blue crotch block with its red V, and the
/// skirts: front, side and rear.
pub(crate) fn waist(w: &mut On<'_>, xf: Affine3A) {
    w.seed(0.2);
    w.loft(&BELT, 3.2, 28, BODY, xf);
    w.cube(v(0.9, 0.42, 0.22), 0.06, TRIM, xf * at(0.0, 2.22, 1.2));
    w.cube(v(0.34, 0.24, 0.1), 0.03, YELLOW, xf * at(0.0, 2.22, 1.32));
    w.block(v(0.95, 1.2, 1.3), v2(1.3, 1.0), v2(0.0, -0.05), 0.14, TRIM, xf * at(0.0, 1.38, 0.45));
    let vee = [v2(-0.44, 0.42), v2(0.44, 0.42), v2(0.44, 0.02), v2(0.0, -0.46), v2(-0.44, 0.02)];
    w.extrude(&vee, 0.12, ACCENT, xf * at(0.0, 1.42, 1.13));
    for s in Side::BOTH {
        // The front skirt over the thigh, its hem rising outboard; a raised panel down its inner
        // half and a blue band along its hem.
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
        w.hexa(plate(x0 - 0.02, x1 + 0.02, -0.02, Some(0.3), 0.0, -0.19, 0.2), 0.06, TRIM, skirt);
        slot(w, 0.7, skirt * at(0.3, -0.25, 0.17));
        // The side skirt over the hip, a blue panel in it; the rear skirt's half, swept back.
        let side = xf * sided(s, place(v(2.42, 1.5, -0.1), rz(0.18)));
        w.block(v(0.42, 1.4, 1.9), v2(0.85, 0.9), Vec2::ZERO, 0.14, BODY, side);
        w.cube(v(0.1, 0.7, 0.9), 0.03, TRIM, side * at(0.22, -0.1, 0.0));
        let rear = xf * sided(s, place(v(0.8, 1.5, -1.32), rx(0.3)));
        w.block(v(1.45, 1.5, 0.42), v2(0.92, 0.85), v2(0.0, 0.05), 0.16, BODY, rear);
        slot(w, 0.4, rear * at(0.2, 0.0, -0.22) * Affine3A::from_rotation_y(PI));
    }
}

// --- Shoulders and arms. ---

/// The shoulder armour's block (right side): its corners, inside and out, foot and top, back and
/// front. A tall box, a little wider at its top, its front leaning forward as it rises; its edges
/// cut deep.
const SHOULDER: [(f32, f32, f32); 8] = [
    (2.55, 3.75, -1.22),
    (4.85, 3.75, -1.22),
    (2.55, 6.2, -1.22),
    (4.95, 6.2, -1.22),
    (2.55, 3.75, 1.15),
    (4.85, 3.75, 1.15),
    (2.55, 6.2, 1.3),
    (4.95, 6.2, 1.3),
];

/// The shoulder armour on side `s` at `xf`: the block, a raised lid over its top, a plate on its
/// outer face with a blue inset and a yellow vent, a plate on its front, a blue lip round its
/// foot, a running light at its outer front corner, and the grey socket into the chest.
pub(crate) fn shoulder_armour(o: &mut On<'_>, s: Side, xf: Affine3A) {
    o.seed(0.24 + s.pick(0.0, 0.4));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    o.hexa(hexa_of(SHOULDER), 0.32, BODY, flat);
    let lid = hexa_of([
        (2.8, 6.05, -1.02),
        (4.7, 6.05, -1.02),
        (2.8, 6.6, -1.02),
        (4.7, 6.44, -1.02),
        (2.8, 6.05, 1.06),
        (4.7, 6.05, 1.06),
        (2.8, 6.6, 1.06),
        (4.7, 6.44, 1.06),
    ]);
    o.hexa(lid, 0.16, BODY, flat);
    let outer = hexa_of([
        (4.85, 4.0, -0.95),
        (5.1, 4.0, -0.95),
        (4.9, 5.85, -1.0),
        (5.16, 5.85, -1.0),
        (4.85, 4.0, 0.95),
        (5.1, 4.0, 0.95),
        (4.9, 5.85, 1.05),
        (5.16, 5.85, 1.05),
    ]);
    o.hexa(outer, 0.1, BODY, flat);
    let lean = (0.06f32 / 1.85).atan();
    o.cube(v(0.1, 0.8, 0.7), 0.03, TRIM, flat * place(v(5.16, 4.75, -0.32), rz(-lean)));
    vent(o, 0.42, 0.6, 3, YELLOW, GREY, flat * place(v(5.18, 4.85, 0.45), ry(FRAC_PI_2) * rx(lean)));
    let rise = (0.15f32 / 2.45).atan();
    let front = flat * place(v(3.7, 4.95, 1.28), rx(-rise));
    o.block(v(1.7, 1.7, 0.16), v2(0.92, 1.0), Vec2::ZERO, 0.06, BODY, front);
    o.cube(v(0.3, 1.0, 0.1), 0.02, TRIM, front * at(0.62, -0.1, 0.1));
    o.cube(v(2.2, 0.24, 2.36), 0.06, TRIM, flat * at(3.72, 3.8, -0.03));
    slot(o, 0.6, front * at(-0.4, 0.2, 0.09));
    slot(o, 0.9, flat * place(v(3.6, 6.53, 0.0), rx(-FRAC_PI_2) * rz(FRAC_PI_2)));
    // A running light: red on the suit's own left (+x), green on its right.
    let nav = flat * at(4.5, 6.12, 1.2);
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
    f.block(
        v(0.9, 0.9, 0.32),
        v2(0.6, 0.8),
        Vec2::ZERO,
        0.08,
        BODY,
        fo * place(v(0.0, 1.75, -0.7), rx(0.25)),
    );
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

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): heavy, squared off,
/// swelling a little out and forward and closing in under the hip.
const THIGH: [Hoop; 5] = [
    hoop(0.35, 0.05, 0.05, 0.98, 0.96),
    hoop(0.8, 0.1, 0.08, 1.16, 1.1),
    hoop(2.4, 0.14, 0.06, 1.22, 1.14),
    hoop(3.2, 0.1, 0.0, 1.08, 1.02),
    hoop(3.62, 0.06, -0.02, 0.76, 0.74),
];

/// The lower leg's core, up its axis from the ankle to the knee: full calves swelling back and out
/// to their widest a little under the knee.
const SHIN: [Hoop; 7] = [
    hoop(0.75, 0.02, 0.0, 0.98, 1.0),
    hoop(1.6, 0.04, -0.08, 1.06, 1.1),
    hoop(2.7, 0.07, -0.22, 1.22, 1.3),
    hoop(3.7, 0.09, -0.34, 1.32, 1.42),
    hoop(4.5, 0.08, -0.36, 1.3, 1.4),
    hoop(5.05, 0.06, -0.28, 1.12, 1.18),
    hoop(5.4, 0.04, -0.2, 0.8, 0.82),
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

/// The thigh on side `s` at `xf`: the core on a grey ball at the hip, and the armour standing off
/// it: two plates down its front, broken across above the knee, and a plate down its outside,
/// each with its edges cut.
pub(crate) fn thigh(th: &mut On<'_>, s: Side, xf: Affine3A) {
    let (hip, knee) = (j(Bone::ThighR), j(Bone::ShinR));
    th.seed(0.31 + s.pick(0.0, 0.2));
    th.sphere(0.78, 6, GREY, xf * sided(s, Affine3A::from_translation(hip)));
    let axis = xf * sided(s, place(knee, toward(hip - knee)));
    th.loft(&THIGH, 4.4, 28, BODY, axis);
    th.hexa(limb_plate((-0.8, 0.98), (0.7, 1.85), (0.76, 0.76), (1.3, 1.3), 0.0), 0.14, BODY, axis);
    th.hexa(limb_plate((-0.78, 0.96), (1.95, 3.05), (0.74, 0.7), (1.28, 1.2), 0.06), 0.14, BODY, axis);
    let side = hexa_of([
        (1.02, 0.7, -0.8),
        (1.4, 0.7, -0.8),
        (1.02, 2.9, -0.7),
        (1.36, 2.9, -0.7),
        (1.02, 0.7, 0.8),
        (1.4, 0.7, 0.8),
        (1.02, 2.9, 0.75),
        (1.36, 2.9, 0.75),
    ]);
    th.hexa(side, 0.14, BODY, axis);
    th.greeble(|th| {
        th.cube(v(0.06, 0.7, 0.05), 0.0, FRAME, axis * at(0.62, 2.55, 1.24));
        th.cube(v(0.05, 0.06, 1.1), 0.0, FRAME, axis * at(1.4, 1.5, 0.0));
        th.cube(v(0.05, 1.6, 0.05), 0.0, FRAME, axis * place(v(0.2, 1.9, -1.08), ry(PI)));
    });
}

/// The shin on side `s` at `xf`: the knee's barrel and the core of the calf, its armour standing
/// off it: a plate down its front in two, a plate down its outside with the yellow vent, the
/// calf's back plated in two; the knee guard standing out of the front with a pointed top and a
/// blue panel in its face; and the boot's shaft round the ankle with the blue tongue up its front
/// and a grey hub either side.
pub(crate) fn shin(m: &mut On<'_>, s: Side, xf: Affine3A) {
    let (knee, ankle) = (j(Bone::ShinR), j(Bone::FootR));
    m.seed(0.37 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let tube = xf * sided(s, place(ankle, toward(knee - ankle)));
    m.cylinder(0.6, 1.3, 12, GREY, flat * place(knee, rz(FRAC_PI_2)));
    m.loft(&SHIN, 4.2, 28, BODY, tube);
    m.hexa(limb_plate((-0.66, 0.66), (2.6, 4.35), (0.7, 0.7), (1.2, 1.24), 0.04), 0.14, BODY, tube);
    m.hexa(limb_plate((-0.7, 0.7), (1.05, 2.5), (0.7, 0.7), (1.16, 1.2), 0.04), 0.14, BODY, tube);
    let outside = hexa_of([
        (1.0, 1.4, -0.95),
        (1.24, 1.4, -0.95),
        (1.25, 4.6, -1.3),
        (1.54, 4.6, -1.3),
        (1.0, 1.4, 0.6),
        (1.24, 1.4, 0.6),
        (1.25, 4.6, 0.75),
        (1.54, 4.6, 0.75),
    ]);
    m.hexa(outside, 0.14, BODY, tube);
    vent(m, 0.7, 1.0, 4, YELLOW, GREY, tube * place(v(1.44, 2.7, -0.3), ry(FRAC_PI_2) * rz(0.08)));
    for (y, back) in [((1.9, 3.75), (-1.5, -1.9)), ((3.85, 4.85), (-1.9, -1.72))] {
        let calf = hexa_of([
            (-0.74, y.0, back.0),
            (0.78, y.0, back.0),
            (-0.7, y.1, back.1),
            (0.74, y.1, back.1),
            (-0.74, y.0, back.0 + 0.45),
            (0.78, y.0, back.0 + 0.45),
            (-0.7, y.1, back.1 + 0.45),
            (0.74, y.1, back.1 + 0.45),
        ]);
        m.hexa(calf, 0.14, BODY, tube);
    }
    m.greeble(|m| {
        m.cube(v(1.1, 0.06, 0.05), 0.0, FRAME, tube * at(0.0, 2.55, 1.21));
        m.cube(v(0.06, 1.0, 0.05), 0.0, FRAME, tube * at(-0.42, 3.4, 1.24));
        m.cube(v(0.05, 1.6, 0.05), 0.0, FRAME, tube * at(1.5, 3.6, 0.4));
        m.cube(v(0.06, 1.2, 0.05), 0.0, FRAME, tube * place(v(0.3, 2.9, -1.72), ry(PI) * rx(0.2)));
    });
    let guard = flat * Affine3A::from_translation(knee);
    let cap = hexa_of([
        (-0.72, -1.1, 0.45),
        (0.72, -1.1, 0.45),
        (-0.6, 1.0, 0.3),
        (0.6, 1.0, 0.3),
        (-0.72, -1.1, 1.38),
        (0.72, -1.1, 1.38),
        (-0.6, 1.0, 1.12),
        (0.6, 1.0, 1.12),
    ]);
    m.hexa(cap, 0.16, BODY, guard);
    let point = hexa_of([
        (-0.6, 1.0, 0.3),
        (0.6, 1.0, 0.3),
        (-0.14, 1.55, 0.35),
        (0.14, 1.55, 0.35),
        (-0.6, 1.0, 1.12),
        (0.6, 1.0, 1.12),
        (-0.14, 1.55, 0.82),
        (0.14, 1.55, 0.82),
    ]);
    m.hexa(point, 0.08, BODY, guard);
    let tilt = (0.26f32 / 2.1).atan();
    m.block(
        v(0.62, 1.2, 0.14),
        v2(0.8, 1.0),
        Vec2::ZERO,
        0.04,
        TRIM,
        guard * place(v(0.0, -0.1, 1.27), rx(-tilt)),
    );

    let x = ankle.x;
    let (y0, y1) = (ankle.y - 0.65, ankle.y + 1.0);
    m.loft(
        &[
            hoop(y0, x, -0.1, 1.16, 1.1),
            hoop(y0 + 0.9, x, -0.12, 1.15, 1.08),
            hoop(y1, x, -0.14, 1.05, 1.0),
            hoop(y1 + 0.1, x, -0.14, 0.9, 0.86),
        ],
        3.8,
        24,
        BODY,
        flat,
    );
    let tongue = [
        (-0.86, y0 + 0.15, 0.62),
        (0.86, y0 + 0.15, 0.62),
        (-0.7, y1 + 0.3, 0.6),
        (0.7, y1 + 0.3, 0.6),
        (-0.86, y0 + 0.15, 1.66),
        (0.86, y0 + 0.15, 1.66),
        (-0.7, y1 + 0.3, 1.16),
        (0.7, y1 + 0.3, 1.16),
    ]
    .map(|(dx, y, z)| v(x + dx, y, z));
    m.hexa(tongue, 0.16, TRIM, flat);
    for side in [1.0f32, -1.0] {
        let hub = flat * place(v(x + side * 1.18, ankle.y - 0.15, ankle.z - 0.1), rz(FRAC_PI_2));
        m.lathe(
            &[(0.0, -0.16), (0.5, -0.16), (0.56, -0.1), (0.56, 0.1), (0.5, 0.16), (0.0, 0.16)],
            16,
            GREY,
            hub,
        );
        m.cylinder(0.26, 0.36, 10, FRAME, hub);
    }
}

/// How far a foot's sides lean in, per metre up.
const FOOT_LEAN: f32 = 0.16;
/// The foot's base, a little over the sole's plate, and the top of the instep at the toes' hinge.
const FOOT_BASE: f32 = SOLE + 0.28;
const INSTEP: f32 = -8.1;

/// The foot on side `s` at `xf`, but for its sole: the grey ball of the ankle, a tall heel, the
/// instep falling from it to the toes' hinge, a spur up the heel's back, seams.
pub(crate) fn heel(f: &mut On<'_>, s: Side, xf: Affine3A) {
    let (ankle, hinge) = (j(Bone::FootR), j(Bone::ToeR));
    let x = ankle.x;
    f.seed(0.43 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    f.sphere(0.5, 5, GREY, flat * Affine3A::from_translation(ankle));
    let back = [-2.0, -1.72, -0.25, -0.25];
    f.hexa(boot(x, FOOT_BASE, (-7.7, -7.75), (1.2, 1.25), back, FOOT_LEAN), 0.26, BODY, flat);
    let instep = [-0.3, -0.3, hinge.z - 0.02, hinge.z - 0.1];
    f.hexa(boot(x, FOOT_BASE, (-7.82, INSTEP), (1.25, 1.25), instep, FOOT_LEAN), 0.2, BODY, flat);
    let spur = [v2(0.0, 0.0), v2(0.7, 0.0), v2(0.4, 0.34), v2(-0.34, 0.7), v2(-0.18, 0.24)];
    f.extrude(&spur, 0.26, BODY, flat * place(v(x, -7.82, -1.7), ry(FRAC_PI_2)));
    f.greeble(|f| {
        f.cube(v(2.0, 0.05, 0.05), 0.0, FRAME, flat * at(x, -7.76, -0.28));
        for side in [-1.0, 1.0] {
            let face = flat * place(v(x + side * 1.16, -8.3, -1.0), rz(side * FOOT_LEAN.atan()));
            f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, face);
        }
    });
}

/// The toe cap on side `s` at `xf`, but for its sole: falling and narrowing from the hinge to its
/// blunt nose, a red ridge along it.
pub(crate) fn toe_cap(t: &mut On<'_>, s: Side, xf: Affine3A) {
    let hinge = j(Bone::ToeR);
    let x = hinge.x;
    t.seed(0.47 + s.pick(0.0, 0.2));
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let cap = [hinge.z + 0.02, hinge.z + 0.02, 3.3, 2.95];
    t.hexa(boot(x, FOOT_BASE, (INSTEP - 0.02, -8.5), (1.25, 0.86), cap, FOOT_LEAN), 0.26, BODY, flat);
    t.hexa(boot(x, -8.4, (-7.98, -8.32), (0.46, 0.36), [1.45, 1.5, 2.8, 2.62], 0.0), 0.06, ACCENT, flat);
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

/// The backpack at `xf`, but for its thrusters: the pack, a blue spine down its back, the binders'
/// pivots at its top corners and blue plates down its sides.
pub(crate) fn backpack(b: &mut On<'_>, xf: Affine3A) {
    b.seed(0.5);
    b.loft(&PACK, 3.4, 24, BODY, xf);
    b.cube(v(0.8, 1.9, 0.14), 0.04, TRIM, xf * at(0.0, 4.25, -2.84));
    slot(b, 1.2, xf * place(v(0.0, 4.25, -2.92), ry(PI)));
    // The grey skirt the thrusters hang under, and a grille in the pack's top.
    b.block(v(2.0, 0.34, 1.2), v2(0.92, 0.9), Vec2::ZERO, 0.06, GREY, xf * at(0.0, 3.02, -2.2));
    vent(b, 1.3, 0.6, 4, GREY, GREY, xf * place(v(0.0, 5.44, -2.12), rx(-FRAC_PI_2)));
    for s in Side::BOTH {
        let flat = xf * sided(s, Affine3A::IDENTITY);
        b.lathe(&band(0.0, 0.5, 0.0, 0.3), 16, GREY, flat * place(v(1.2, 4.9, -2.75), rx(-FRAC_PI_2)));
        b.cube(v(0.5, 1.4, 0.4), 0.06, TRIM, flat * at(1.18, 3.9, -2.62));
        slot(b, 0.8, flat * place(v(1.3, 4.0, -1.8), ry(FRAC_PI_2)));
    }
}

/// A wing laid out in its own plane, x out from the binder's knuckle and y up: its spar at an
/// angle (degrees up from x) and so long; its feathers, each rooted a fraction along the spar,
/// pointing at an angle, so long; and the coverts over their roots.
pub(crate) struct Fan {
    pub spar: (f32, f32),
    pub primaries: &'static [(f32, f32, f32)],
    pub coverts: &'static [(f32, f32, f32)],
}

/// The big upper wing, as an angel's: its spar rising out over the shoulder to the wrist, the
/// long feathers fanning out and down from the wrist, and shorter ones hanging from the spar.
const UPPER_WING: Fan = Fan {
    spar: (48.0, 5.2),
    primaries: &[
        (1.0, 20.0, 3.6),
        (0.97, 0.0, 4.4),
        (0.93, -20.0, 5.0),
        (0.88, -40.0, 5.4),
        (0.82, -58.0, 5.6),
        (0.7, -74.0, 5.4),
        (0.56, -86.0, 5.0),
        (0.42, -96.0, 4.4),
        (0.28, -104.0, 3.6),
    ],
    coverts: &[
        (0.92, -30.0, 2.2),
        (0.78, -58.0, 2.4),
        (0.63, -78.0, 2.4),
        (0.48, -90.0, 2.2),
        (0.33, -100.0, 1.8),
    ],
};

/// The smaller lower wing, under the upper and behind it, out and down beside the legs.
const LOWER_WING: Fan = Fan {
    spar: (-35.0, 2.6),
    primaries: &[
        (1.0, -40.0, 4.0),
        (0.9, -58.0, 4.4),
        (0.75, -74.0, 4.4),
        (0.6, -88.0, 4.0),
        (0.42, -100.0, 3.4),
    ],
    coverts: &[(0.8, -60.0, 1.8), (0.55, -80.0, 1.8)],
};

/// One feather, at `xf` (x along it from its root, y across, z out of its wing's plane): a long
/// blade to a sharp point, and (if `spine`) a raised spine down it up close.
fn feather(o: &mut On<'_>, len: f32, width: f32, spine: bool, xf: Affine3A) {
    let w = width * 0.5;
    let blade = [
        v2(-0.1, -w * 0.55),
        v2(0.2 * len, -w),
        v2(0.8 * len, -w * 0.85),
        v2(len, 0.0),
        v2(0.78 * len, w * 0.7),
        v2(0.15 * len, w),
        v2(-0.1, w * 0.6),
    ];
    o.extrude(&blade, 0.12, BODY, xf);
    if !spine {
        return;
    }
    o.greeble(|o| {
        let spine =
            [v2(0.0, -0.07), v2(0.8 * len, -0.05), v2(0.92 * len, 0.0), v2(0.8 * len, 0.05), v2(0.0, 0.07)];
        o.extrude(&spine, 0.2, BODY, xf);
    });
}

/// A wing at `xf` (its plane's x out, y up, z toward the suit), from its knuckle at the origin:
/// the spar with a blue panel, the grey wrist, the feathers and the coverts over their roots.
fn wing(o: &mut On<'_>, fan: &Fan, xf: Affine3A) {
    let dir = Vec2::from_angle(fan.spar.0.to_radians());
    let len = fan.spar.1;
    let tip = dir * len;
    let spar = xf * along(v(0.0, 0.0, 0.06), tip.extend(0.06));
    o.block(v(0.5, len + 0.3, 0.42), v2(0.55, 0.7), Vec2::ZERO, 0.08, BODY, spar);
    o.cube(v(0.36, len * 0.35, 0.1), 0.02, TRIM, spar * at(0.0, -len * 0.25, 0.24));
    let wrist = xf * place(tip.extend(0.06), rx(FRAC_PI_2));
    o.lathe(&band(0.0, 0.34, -0.3, 0.3), 14, BODY, wrist);
    o.cylinder(0.18, 0.66, 10, GREY, wrist);
    for (k, &(t, phi, l)) in fan.primaries.iter().enumerate() {
        let root = dir * (len * t);
        feather(
            o,
            l,
            0.55 + 0.09 * l,
            true,
            xf * place(root.extend(-0.04 - 0.05 * k as f32), rz(phi.to_radians())),
        );
    }
    for &(t, phi, l) in fan.coverts {
        let root = dir * (len * t);
        feather(o, l, 0.62, false, xf * place(root.extend(0.16), rz(phi.to_radians())));
    }
}

/// Where the binder's arm meets its wings (right side).
const KNUCKLE: Vec3 = Vec3::new(1.95, 5.85, -3.5);

/// A wing binder on side `s` at `xf`: its arm up and out from the pivot on the pack to the
/// knuckle, and its two wings off the knuckle, laid out as `upper` and `lower`, the upper in front.
pub(crate) fn binder(o: &mut On<'_>, s: Side, xf: Affine3A, upper: &Fan, lower: &Fan) {
    o.seed(0.55 + s.pick(0.0, 0.2));
    let root = j(Bone::WingR);
    let flat = xf * sided(s, Affine3A::IDENTITY);
    let arm = flat * along(root, KNUCKLE);
    let len = (KNUCKLE - root).length();
    o.lathe(&band(0.3, 0.42, -0.2, 0.2), 14, GREY, flat * place(root, rx(-FRAC_PI_2)));
    o.block(v(0.62, len + 0.4, 0.55), v2(0.85, 0.9), Vec2::ZERO, 0.1, BODY, arm);
    o.cube(v(0.12, len * 0.6, 0.4), 0.03, TRIM, arm * at(0.32, 0.0, 0.0));
    // A vernier in the arm's back.
    let bell = arm * place(v(0.0, -0.1, -0.26), rx(-FRAC_PI_2));
    o.lathe(
        &[(0.12, -0.04), (0.15, 0.0), (0.19, 0.14), (0.25, 0.32), (0.22, 0.33), (0.12, 0.03)],
        12,
        STEEL,
        bell,
    );
    o.cylinder(0.13, 0.04, 10, FRAME, bell * at(0.0, 0.04, 0.0));
    o.lathe(&band(0.0, 0.48, -0.3, 0.3), 16, GREY, flat * place(KNUCKLE, rx(-FRAC_PI_2)));
    o.lathe(&band(0.0, 0.3, 0.3, 0.42), 12, BODY, flat * place(KNUCKLE, rx(-FRAC_PI_2)));
    let plane = flat * place(KNUCKLE + v(0.0, 0.0, -0.3), ry(0.35) * rx(-0.12));
    wing(o, upper, plane);
    wing(o, lower, plane * at(0.0, -0.15, -0.5));
}

// --- The Heavy Beam Rifle. ---

/// The Heavy Beam Rifle's two halves either side of its middle (x), and how high their barrels ride
/// over the grip.
const BARRELS: (f32, f32) = (0.42, 0.62);
/// How high the energy packs ride over the barrels: over the forearm behind the fist at the ready
/// ([`crate::hold`]), and ahead of it carried.
const PACK_UP: f32 = 0.94;
/// How far behind the grip the packs reach, and how far ahead of it the muzzles are: no further
/// than the rifle can hang muzzle down from the fist and still clear the toes.
const REAR: f32 = -1.2;
const MUZZLE: f32 = 8.1;

/// One half of the Heavy Beam Rifle, at `xf` (its barrel along +z from over the grip at the origin,
/// up +y), `out` (±1) the side away from its twin: the receiver from just ahead of the grip, a white
/// panel down its outside; the blue energy pack riding on its rear, over the fist; the vented
/// shroud, the barrel and the flared muzzle.
fn rifle_half(o: &mut On<'_>, out: f32, xf: Affine3A) {
    let along_z = |z: f32| xf * place(v(0.0, 0.0, z), rx(FRAC_PI_2));
    o.block(v(0.6, 0.74, 3.0), v2(0.92, 0.9), Vec2::ZERO, 0.1, GUN, xf * at(0.0, 0.0, 1.7));
    o.cube(v(0.08, 0.46, 2.2), 0.02, BODY, xf * at(out * 0.32, 0.04, 1.7));
    let pack = xf * at(0.0, PACK_UP, REAR + 1.15);
    o.block(v(0.62, 0.82, 2.3), v2(0.88, 0.9), Vec2::ZERO, 0.12, TRIM, pack);
    o.cube(v(0.5, 0.6, 0.14), 0.03, GUN, pack * at(0.0, 0.0, -1.19));
    o.cube(v(0.68, 0.88, 0.2), 0.03, YELLOW, pack * at(0.0, 0.0, 0.55));
    o.cube(v(0.4, 0.34, 1.0), 0.03, GUN, xf * at(0.0, 0.52, 0.6));
    o.cylinder(0.3, 2.2, 14, GUN, along_z(4.25));
    for z in [3.3, 5.2] {
        o.lathe(&band(0.26, 0.35, -0.08, 0.08), 14, YELLOW, along_z(z));
    }
    o.cylinder(0.2, 2.0, 12, GUN, along_z(6.3));
    let bell = [(0.0, 0.0), (0.22, 0.0), (0.3, 0.42), (0.38, 0.74), (0.38, 0.86), (0.0, 0.86)];
    o.lathe(&bell, 14, GUN, along_z(MUZZLE - 0.86));
    o.cylinder(0.2, 0.04, 12, FRAME, along_z(MUZZLE - 0.02));
    o.greeble(|o| {
        for k in 0..4 {
            o.cube(v(0.34, 0.06, 0.16), 0.0, FRAME, xf * at(0.0, 0.3, 3.55 + k as f32 * 0.45));
        }
        o.cylinder(0.12, 0.06, 10, FRAME, along_z(MUZZLE));
    });
}

/// The Heavy Beam Rifle at `xf` (the grip at the origin, barrels along +z): its two halves joined
/// along their middle, the packs joined over them, the grip and its guard under them, the scope
/// over the packs and the clamps round the barrels. Returns where its beam leaves, between the
/// muzzles, in the suit's frame.
pub(crate) fn twin_rifle(o: &mut On<'_>, xf: Affine3A) -> Vec3 {
    o.seed(0.61);
    let (bx, by) = BARRELS;
    for side in [-1.0f32, 1.0] {
        rifle_half(o, side, xf * at(side * bx, by, 0.0));
    }
    o.cube(v(0.34, 0.86, 0.46), 0.05, GUN, xf * place(v(0.0, -0.2, 0.32), rx(-0.08)));
    o.cube(v(0.3, 0.12, 0.6), 0.02, GUN, xf * at(0.0, -0.1, 0.62));
    o.block(v(0.3, 0.56, 2.9), v2(1.0, 1.0), Vec2::ZERO, 0.04, TRIM, xf * at(0.0, by, 1.7));
    o.block(v(0.3, 0.5, 2.0), v2(1.0, 1.0), Vec2::ZERO, 0.04, TRIM, xf * at(0.0, by + PACK_UP, 0.0));
    // The scope on its mount over the packs, its lens forward.
    let top = by + PACK_UP;
    o.cube(v(0.22, 0.3, 0.5), 0.03, GUN, xf * at(0.0, top + 0.4, 0.5));
    o.block(v(0.32, 0.34, 1.2), v2(0.9, 0.9), Vec2::ZERO, 0.04, GUN, xf * at(0.0, top + 0.65, 0.6));
    o.cylinder(0.12, 0.06, 12, LENS, xf * place(v(0.0, top + 0.65, 1.22), rx(FRAC_PI_2)));
    o.cube(v(1.36, 0.5, 0.36), 0.06, GUN, xf * at(0.0, by, 4.25));
    o.cube(v(1.1, 0.16, 0.3), 0.03, GUN, xf * at(0.0, by, 6.8));
    xf.transform_point3(v(0.0, by, MUZZLE + 0.05))
}

// --- Shield and saber. ---

/// The shield's outline, in its own plane (x across, y along it, its point down).
const SHIELD: [Vec2; 10] = [
    Vec2::new(0.0, 2.4),
    Vec2::new(0.8, 2.25),
    Vec2::new(1.15, 1.6),
    Vec2::new(1.1, -0.6),
    Vec2::new(0.55, -2.0),
    Vec2::new(0.0, -2.7),
    Vec2::new(-0.55, -2.0),
    Vec2::new(-1.1, -0.6),
    Vec2::new(-1.15, 1.6),
    Vec2::new(-0.8, 2.25),
];

/// Where the shield hangs on the left forearm: its face outward (-x), its long axis down the
/// forearm and forward.
fn shield_place() -> Affine3A {
    place(j(Bone::Shield) + v(-0.15, -0.6, 0.0), Quat::from_rotation_arc(Vec3::Z, Vec3::NEG_X) * rz(0.35))
}

/// The shield at `xf` (its face looking along +z, its point down -y): a white plate, the red face
/// raised in it, a blue cap across its top and a ridge down its middle; a grey mount behind.
pub(crate) fn shield(o: &mut On<'_>, xf: Affine3A) {
    o.seed(0.71);
    o.extrude(&SHIELD, 0.3, BODY, xf);
    let face: Vec<Vec2> = SHIELD.iter().map(|p| v2(p.x * 0.74, p.y * 0.8 - 0.15)).collect();
    o.extrude(&face, 0.22, ACCENT, xf * at(0.0, 0.0, 0.16));
    let cap = [v2(-0.95, 1.62), v2(0.95, 1.62), v2(0.75, 2.12), v2(0.0, 2.26), v2(-0.75, 2.12)];
    o.extrude(&cap, 0.24, TRIM, xf * at(0.0, 0.0, 0.18));
    let ridge = [v2(-0.12, 1.5), v2(0.12, 1.5), v2(0.12, -1.6), v2(0.0, -2.1), v2(-0.12, -1.6)];
    o.extrude(&ridge, 0.36, BODY, xf * at(0.0, 0.0, 0.2));
    o.cube(v(0.9, 1.2, 0.5), 0.06, GREY, xf * at(0.0, 0.3, -0.35));
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
