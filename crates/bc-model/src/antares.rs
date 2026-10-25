//! PX-01 Antares, the colonies' gunship: a walking arsenal, built broad and heavy round what it
//! carries, its bulk in boxy masses with their edges rounded, plated over and broken into panels:
//!
//! - a small head set low between the shoulders: a rounded cranium with a ridge over its crown, a
//!   faceted brow shading the visor, the two eyes lit green behind its dark glass, a faceplate
//!   narrowing to a red chin with a slit either side of its middle, cheek guards framing it, a
//!   round sensor on each temple and a small gun in each side of the brow; a short yellow V-fin
//!   springing from a red crest, and a plate with vents at the back;
//! - a broad chest: a red keel down its middle, the cockpit's hatch, between two housings standing
//!   forward of it, each with a hatch door hinged at its outer edge over the muzzle of a gatling,
//!   four barrels round a spindle in a round port, which Full Salvo fires; louvred vents in the
//!   lower chest under them, plates either side of the neck and a ribbed grey collar behind it;
//!   a grey abdomen with two red plates over its front;
//! - a red chevron on the crotch, the belt's red buckle, front skirts turned out round the thighs
//!   with a raised panel and a red hem, heavy armour over each hip, and the rear skirt;
//! - high, broad shoulder armour, big rounded boxes, the homing missiles' nine hatches in a red
//!   frame across each front, a red lip along its foot, a plate along its top with the running
//!   light at its front corner, a thick plate on its outer face with a round vernier port, vents in
//!   its back;
//! - short white upper arms with a raised ring, and big gauntlets over red cuffs, plated down
//!   their outer faces, a raised plate down their fronts and a guard over the elbow; grey fists;
//! - thick thighs and full calves, each a lofted core under plates: the thigh's front in two tiers,
//!   each two halves meeting in a ridge, a step between them, a plate down its outer side with a
//!   raised panel and one over its back; a red knee guard standing out of the shin; the shin guard
//!   ridged down its front and widening as the calf swells, plates down the calf's inside and over
//!   its back; the micro-missiles' pod down the outside of each calf, red at its front, its eight
//!   hatches in its outer face;
//! - big boots: a rounded shaft over the ankle, a thick tongue rising up its front, a grey band
//!   behind and grey hubs either side, on a thick platform with a high heel and a long red toe on
//!   its own hinge, a white block raised along it, and a vernier in the sole under the heel and
//!   under the toe;
//! - a backpack between two ribbed gunmetal drums, a red cap over its top with a carry handle,
//!   vents and a round hatch down its back, a small vernier on each of its shoulders and the two
//!   main thrusters under it.
//!
//! It carries the beam gatling in its right hand, held as a pistol is: a gunmetal receiver high on
//! its grip, all of it ahead of the fist but the carry handle running back over the forearm, a
//! white cover, a sight and a power pack, the red-banded drum turning six barrels through two
//! clamps. At the ready the forearm lies under the handle; carried, muzzle down, it hangs clear in
//! front of the leg. The combat knife is in its left hand, a heavy clipped blade with its honed
//! edge bright.
//!
//! The armour is white, the trim red; the inner frame, the hands and the ankles are grey, the
//! weapons gunmetal.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Affine3A, Vec2, Vec3};

use crate::Designer;
use crate::On;
use crate::detail::{Cut, GREY, LENS, band, port, slot, stacked, toward};
use crate::frames::{
    BODY, EYE, FRAME, GLASS, GUN, SOLE, STEEL, Sole, TRIM, YELLOW, at, j, nozzle, place, rx, ry, rz, sided,
    sole, sp, v, v2,
};
use crate::kit::{Hoop, Light, Paint, hoop};
use crate::paint;
use crate::rig::{Bone, Side};

pub(crate) fn antares(d: &mut Designer) {
    head(d);
    chest(d);
    waist(d);
    shoulders(d);
    arms(d);
    legs(d);
    backpack(d);
    gatling(d);
    knife(d);
}

// --- Shared detail. ---

const FLAT: Affine3A = Affine3A::IDENTITY;

/// A frame on a stacked block's front at (`x`, `y`), lifted `lift` off it: z out of the face, y
/// up it.
fn front(block: [Cut; 2], x: f32, y: f32, lift: f32) -> Affine3A {
    let [(y0, .., f0), (y1, .., f1)] = block;
    let z = f0 + (f1 - f0) * (y - y0) / (y1 - y0);
    let tilt = ((f1 - f0) / (y1 - y0)).atan();
    place(v(x, y, z), rx(tilt)) * at(0.0, 0.0, lift)
}

/// A plate's corners from its foot and its top (each as for [`stacked`], the front's z at its
/// low-x edge), its front falling back `fall` per metre out along x, so a pair of them meet in a
/// ridge. Every face stays flat.
fn slab([(y0, a0, b0, k0, f0), (y1, a1, b1, k1, f1)]: [Cut; 2], fall: f32) -> [Vec3; 8] {
    [
        v(a0, y0, k0),
        v(b0, y0, k0),
        v(a1, y1, k1),
        v(b1, y1, k1),
        v(a0, y0, f0),
        v(b0, y0, f0 - fall * (b0 - a0)),
        v(a1, y1, f1),
        v(b1, y1, f1 - fall * (b1 - a1)),
    ]
}

/// A panel of missile hatches on the face at `xf`'s origin (looking along its +z, y up it): `cols`
/// by `rows` white doors a `cell` apart in a dark well, framed in red, each with its hinge line
/// and its latch. Returns each door's middle, where its missile leaves.
fn hatches(o: &mut On<'_>, cols: u32, rows: u32, cell: Vec2, xf: Affine3A) -> Vec<Vec3> {
    let (w, h) = (cell.x * cols as f32, cell.y * rows as f32);
    o.cube(v(w + 0.28, h + 0.28, 0.34), 0.06, TRIM, xf * at(0.0, 0.0, -0.15));
    o.cube(v(w + 0.04, h + 0.04, 0.1), 0.02, FRAME, xf * at(0.0, 0.0, 0.03));
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x = (c as f32 + 0.5 - cols as f32 * 0.5) * cell.x;
            let y = (r as f32 + 0.5 - rows as f32 * 0.5) * cell.y;
            let door = xf * at(x, y, 0.08);
            o.cube(v(cell.x - 0.1, cell.y - 0.1, 0.1), 0.03, BODY, door);
            o.greeble(|o| {
                o.cube(v(cell.x - 0.22, 0.04, 0.03), 0.0, FRAME, door * at(0.0, cell.y * 0.3, 0.05));
                o.cube(v(0.14, 0.07, 0.04), 0.0, GREY, door * at(0.0, -cell.y * 0.3, 0.05));
            });
            out.push(door.transform_point3(v(0.0, 0.0, 0.05)));
        }
    }
    out
}

/// A louvred vent on the face at `xf`'s origin: a dark well `w` by `h`, `n` grey slats across it.
fn louvre(o: &mut On<'_>, w: f32, h: f32, n: u32, xf: Affine3A) {
    o.cube(v(w, h, 0.12), 0.02, FRAME, xf);
    let pitch = h / n as f32;
    for k in 0..n {
        let y = -h * 0.5 + (k as f32 + 0.5) * pitch;
        let slat = xf * at(0.0, y, 0.03) * Affine3A::from_rotation_x(0.4);
        o.cube(v(w - 0.08, pitch * 0.55, 0.1), 0.01, GREY, slat);
    }
}

/// A bolt's hex head on the face at `xf`'s origin.
fn bolt(o: &mut On<'_>, xf: Affine3A) {
    o.greeble(|o| {
        o.cylinder(0.07, 0.06, 6, GREY, xf * Affine3A::from_rotation_x(FRAC_PI_2));
    });
}

// --- Head. ---

/// Heights on the head: under the chin, and the crown.
const CHIN: f32 = 6.14;
const CROWN: f32 = 7.46;

/// The cockpit's camera, between the eyes behind the visor: where the simulation's pilot looks
/// from.
const EYES: Vec3 = Vec3::new(0.0, 6.75, 1.1);

/// The cranium, up the head from under the brow to its crown (x out, z forward): a rounded box,
/// widest at the temples, closing in to its crown.
const CRANIUM: [Hoop; 6] = [
    hoop(6.26, 0.0, 0.14, 0.6, 0.62),
    hoop(6.46, 0.0, 0.2, 0.68, 0.78),
    hoop(6.98, 0.0, 0.24, 0.72, 0.8),
    hoop(7.24, 0.0, 0.18, 0.66, 0.74),
    hoop(7.4, 0.0, 0.1, 0.5, 0.58),
    hoop(CROWN, 0.0, 0.04, 0.26, 0.34),
];

fn head(d: &mut Designer) {
    let mut h = d.on(Bone::Head);
    h.seed(0.06);
    // The neck, grey, turning inside the collar.
    h.cylinder(0.42, 0.9, 12, GREY, at(0.0, 5.9, 0.15));
    h.loft(&CRANIUM, 3.0, 28, BODY, FLAT);
    // The visor: dark glass across the face, the two eyes lit behind it.
    h.cube(v(1.08, 0.32, 0.32), 0.03, GLASS, at(0.0, 6.76, 0.96));
    for s in Side::BOTH {
        let eye = [v2(0.07, -0.08), v2(0.4, -0.05), v2(0.45, 0.08), v2(0.12, 0.04)];
        h.extrude(&eye, 0.04, EYE, sided(s, at(0.0, 6.75, 1.13)));
    }
    for s in Side::BOTH {
        let flat = sided(s, FLAT);
        // The brow: a faceted lid over the visor, its front edge shading it, falling away to the
        // temples.
        let brow = [
            v(0.0, 6.88, 0.45),
            v(0.6, 6.86, 0.35),
            v(0.0, 7.18, 0.45),
            v(0.6, 7.08, 0.35),
            v(0.0, 6.88, 1.18),
            v(0.6, 6.86, 1.0),
            v(0.0, 7.18, 1.08),
            v(0.6, 7.08, 0.927),
        ];
        h.hexa(brow, 0.05, BODY, flat);
        // The faceplate under the visor: a wedge to the chin, a slit beside its middle.
        let face = [
            v(0.0, 6.22, 0.45),
            v(0.32, 6.22, 0.45),
            v(0.0, 6.64, 0.45),
            v(0.42, 6.64, 0.45),
            v(0.0, 6.22, 1.02),
            v(0.32, 6.22, 0.887),
            v(0.0, 6.64, 1.1),
            v(0.42, 6.64, 0.925),
        ];
        h.hexa(face, 0.04, BODY, flat);
        h.greeble(|h| {
            let (x, y) = (0.13, 6.42);
            let z = 1.02 - 0.4167 * x + 0.1905 * (y - 6.22);
            h.cube(v(0.05, 0.26, 0.04), 0.0, FRAME, sided(s, place(v(x, y, z), ry(-0.4) * rz(-0.18))));
        });
        // The cheek guard framing the face, swept back at its foot.
        let cheek = [
            v(0.38, 6.22, 0.15),
            v(0.7, 6.3, 0.1),
            v(0.38, 6.96, 0.15),
            v(0.7, 6.96, 0.1),
            v(0.38, 6.22, 0.92),
            v(0.7, 6.3, 0.72),
            v(0.38, 6.96, 1.1),
            v(0.7, 6.96, 0.88),
        ];
        h.hexa(cheek, 0.05, BODY, flat);
        slot(&mut h, 0.4, sided(s, place(v(0.71, 6.6, 0.55), ry(FRAC_PI_2))));
        // The ear: a round housing on the temple with its sensor looking out.
        let ear = sided(s, place(v(0.72, 6.72, 0.3), rz(-FRAC_PI_2)));
        h.lathe(
            &[(0.0, -0.04), (0.34, -0.04), (0.38, 0.04), (0.38, 0.14), (0.3, 0.2), (0.0, 0.2)],
            18,
            BODY,
            ear,
        );
        let lens = sided(s, place(v(0.92, 6.72, 0.3), rz(-FRAC_PI_2)));
        h.lathe(&band(0.1, 0.2, -0.04, 0.06), 12, TRIM, lens);
        h.lathe(&[(0.0, -0.02), (0.12, -0.02), (0.1, 0.04), (0.0, 0.06)], 12, LENS, lens);
        // The small gun in the brow's side.
        let gun = sided(s, place(v(0.44, 6.96, 1.0), rx(FRAC_PI_2)));
        h.cylinder(0.08, 0.2, 8, GUN, gun);
        h.greeble(|h| {
            h.cylinder(0.045, 0.04, 6, FRAME, gun * at(0.0, 0.09, 0.0));
        });
    }
    // The red chin.
    h.hexa(stacked([(CHIN, -0.22, 0.22, 0.55, 0.94), (6.3, -0.28, 0.28, 0.55, 1.0)]), 0.04, TRIM, FLAT);
    // The crest the V-fin springs from, red, and the fin's two short yellow blades.
    h.hexa(stacked([(6.95, -0.15, 0.15, 0.7, 1.22), (7.28, -0.15, 0.15, 0.7, 1.12)]), 0.04, TRIM, FLAT);
    for s in Side::BOTH {
        let fin = [v2(0.0, 0.0), v2(0.2, -0.02), v2(0.74, 0.4), v2(0.72, 0.5), v2(0.12, 0.2), v2(0.0, 0.18)];
        h.extrude(&fin, 0.08, YELLOW, sided(s, place(v(0.08, 7.08, 1.14), rx(-0.35))));
    }
    // The ridge over the crown, and the plate at the back of the head with its vents.
    h.hexa(stacked([(7.3, -0.14, 0.14, -0.4, 0.8), (7.52, -0.14, 0.14, -0.3, 0.45)]), 0.04, BODY, FLAT);
    let back = at(0.0, 6.62, -0.6);
    h.cube(v(0.84, 0.6, 0.3), 0.06, BODY, back);
    h.greeble(|h| {
        for x in [-0.2, 0.0, 0.2] {
            h.cube(v(0.07, 0.36, 0.05), 0.0, FRAME, back * at(x, 0.0, -0.15));
        }
    });
    d.sockets.eye = Designer::local(Bone::Head, EYES);
    d.head_base = 6.2;
}

// --- Chest, abdomen and back. ---

/// The chest's core (right half), the mass behind its blocks.
const CORE: [Cut; 2] = [(3.3, 0.0, 2.05, -1.3, 0.9), (6.0, 0.0, 2.25, -1.5, 0.7)];
/// The housing over each chest gatling (right side), standing forward of the core either side of
/// the keel, its front leaning back toward the collar.
const HOUSING: [Cut; 2] = [(3.8, 0.55, 2.3, -0.6, 1.5), (5.88, 0.6, 2.2, -0.6, 1.2)];
/// The red keel down the chest's middle: the cockpit's hatch.
const KEEL: [Cut; 2] = [(3.4, -0.62, 0.62, -0.6, 1.62), (5.75, -0.66, 0.66, -0.6, 1.28)];
/// The lower chest under the housings (right side), the vents in its front.
const BELLY: [Cut; 2] = [(3.2, 0.6, 2.0, -0.3, 1.18), (3.82, 0.6, 2.12, -0.3, 1.3)];
/// Each gatling's muzzle on its housing's front (right side), and its hatch door over it.
const GATLING: Vec2 = Vec2::new(1.42, 4.3);
const DOOR: (Vec2, Vec2) = (Vec2::new(1.42, 5.25), Vec2::new(1.34, 0.94));

fn chest(d: &mut Designer) {
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    for s in Side::BOTH {
        let flat = sided(s, FLAT);
        c.hexa(stacked(CORE), 0.2, BODY, flat);
        c.hexa(stacked(HOUSING), 0.18, BODY, flat);
        c.hexa(stacked(BELLY), 0.12, BODY, flat);
        // The hatch over the gatling: a door in its dark seam, a raised panel on it, hinged at its
        // outer edge, its latch at the inner.
        let door = sided(s, front(HOUSING, DOOR.0.x, DOOR.0.y, 0.0));
        let size = DOOR.1;
        c.cube(v(size.x + 0.08, size.y + 0.08, 0.1), 0.02, FRAME, door);
        c.cube(v(size.x, size.y, 0.14), 0.05, BODY, door * at(0.0, 0.0, 0.02));
        c.cube(v(size.x - 0.42, size.y - 0.36, 0.08), 0.03, BODY, door * at(-0.1, 0.02, 0.1));
        for y in [-0.28, 0.28] {
            c.cylinder(0.07, 0.24, 8, GREY, door * at(size.x * 0.5 + 0.02, y, 0.06));
        }
        c.greeble(|c| {
            c.cube(v(0.08, 0.26, 0.04), 0.0, GREY, door * at(-size.x * 0.5 + 0.12, 0.0, 0.1));
            c.cube(v(size.x - 0.2, 0.04, 0.03), 0.0, FRAME, door * at(0.0, -size.y * 0.5 + 0.1, 0.09));
        });
        // The gatling's muzzle under it: four barrels round a spindle in a round port, a collar
        // round their ends.
        let muzzle =
            sided(s, front(HOUSING, GATLING.x, GATLING.y, 0.0)) * Affine3A::from_rotation_x(FRAC_PI_2);
        c.cylinder(0.38, 0.12, 16, FRAME, muzzle);
        c.lathe(&band(0.36, 0.47, -0.05, 0.1), 16, GUN, muzzle);
        c.cylinder(0.07, 0.46, 8, GUN, muzzle * at(0.0, 0.2, 0.0));
        for k in 0..4 {
            let a = PI / 4.0 + k as f32 * FRAC_PI_2;
            let barrel = muzzle * at(0.15 * a.cos(), 0.16, 0.15 * a.sin());
            c.cylinder(0.075, 0.52, 8, STEEL, barrel);
            c.greeble(|c| {
                c.cylinder(0.04, 0.03, 6, FRAME, barrel * at(0.0, 0.26, 0.0));
            });
        }
        c.lathe(&band(0.2, 0.28, 0.28, 0.36), 14, GUN, muzzle);
        // The vents in the lower chest's front, and the seam over them.
        louvre(&mut c, 1.0, 0.36, 3, sided(s, front(BELLY, 1.3, 3.5, 0.02)));
        c.greeble(|c| {
            c.cube(v(1.3, 0.05, 0.04), 0.0, FRAME, sided(s, front(BELLY, 1.3, 3.76, 0.01)));
        });
        // The collar's plates either side of the neck, and the shoulder's socket in the side.
        c.hexa(stacked([(5.85, 0.72, 1.7, -1.05, 0.5), (6.3, 0.8, 1.5, -0.9, 0.3)]), 0.1, BODY, flat);
        c.cylinder(0.62, 0.6, 14, GREY, sided(s, place(v(2.35, 4.55, 0.0), rz(FRAC_PI_2))));
        // The back's plates, a slot down each.
        let plate = stacked([(3.7, 0.15, 1.35, -1.72, -1.2), (5.7, 0.15, 1.2, -1.75, -1.3)]);
        c.hexa(plate, 0.12, BODY, flat);
        slot(&mut c, 0.8, sided(s, place(v(0.95, 4.7, -1.74), ry(PI))));
    }
    // The keel, its seams across under its top and over its foot, and its hatch's handle.
    c.hexa(stacked(KEEL), 0.12, TRIM, FLAT);
    c.greeble(|c| {
        c.cube(v(1.2, 0.05, 0.04), 0.0, FRAME, front(KEEL, 0.0, 5.3, 0.01));
        c.cube(v(1.2, 0.05, 0.04), 0.0, FRAME, front(KEEL, 0.0, 3.85, 0.01));
    });
    c.cube(v(0.5, 0.12, 0.12), 0.03, GREY, front(KEEL, 0.0, 4.55, 0.04));
    // The ribbed collar behind the neck.
    let (from, to) = (FRAC_PI_2 + 1.2, FRAC_PI_2 - 1.2 + TAU);
    for k in 0..2 {
        let y = 5.94 + k as f32 * 0.18;
        c.lathe_arc(&band(0.62, 1.0 - k as f32 * 0.08, y, y + 0.13), 18, from, to, GREY, at(0.0, 0.0, 0.1));
    }

    // The abdomen: the grey frame, and the red plates over its front.
    let mut a = d.on(Bone::Torso);
    a.seed(0.17);
    let squash = Affine3A::from_scale(v(1.0, 1.0, 0.85));
    a.lathe(&[(0.0, 2.3), (1.3, 2.3), (1.38, 2.45), (1.28, 3.35), (0.0, 3.35)], 14, GREY, squash);
    a.block(v(1.4, 0.44, 0.5), v2(1.1, 1.0), Vec2::ZERO, 0.08, TRIM, at(0.0, 2.58, 0.98));
    a.block(v(1.62, 0.42, 0.5), v2(1.08, 1.0), Vec2::ZERO, 0.08, TRIM, at(0.0, 3.04, 1.0));
}

// --- Waist and skirts. ---

/// The belt round the hips.
const BELT: [Hoop; 4] = [
    hoop(1.98, 0.0, 0.0, 1.75, 1.3),
    hoop(2.06, 0.0, 0.0, 1.86, 1.4),
    hoop(2.42, 0.0, 0.0, 1.86, 1.4),
    hoop(2.5, 0.0, 0.0, 1.76, 1.3),
];
/// The crotch block under the belt's front.
const CROTCH: [Cut; 2] = [(0.8, -0.42, 0.42, -0.6, 1.12), (2.0, -0.62, 0.62, -0.7, 1.45)];

fn waist(d: &mut Designer) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    w.loft(&BELT, 3.2, 28, BODY, FLAT);
    w.hexa(stacked([(2.0, -0.5, 0.5, 1.0, 1.5), (2.5, -0.55, 0.55, 1.0, 1.44)]), 0.06, TRIM, FLAT);
    w.hexa(stacked(CROTCH), 0.14, BODY, FLAT);
    // The red chevron on its front.
    let chevron =
        [v2(-0.46, 0.3), v2(-0.3, 0.3), v2(0.0, -0.12), v2(0.3, 0.3), v2(0.46, 0.3), v2(0.0, -0.38)];
    w.extrude(&chevron, 0.08, TRIM, front(CROTCH, 0.0, 1.45, 0.03));
    for s in Side::BOTH {
        // The front skirt: a shield of a plate hung from the belt and turned out round the
        // thigh, its hem rising toward the outside, a raised panel down its inner half and a red
        // band along its hem.
        let skirt = sided(s, place(v(1.25, 1.45, 1.5), ry(0.34) * rx(-0.16)));
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
        w.hexa(plate(-0.8, 0.8, 0.82, (-1.05, -0.6), -0.2, 0.2), 0.14, BODY, skirt);
        w.hexa(plate(-0.7, 0.0, 0.64, (-0.62, -0.42), 0.1, 0.32), 0.06, BODY, skirt);
        w.hexa(plate(-0.82, 0.82, -0.66, (-1.07, -0.62), -0.1, 0.25), 0.04, TRIM, skirt);
        w.greeble(|w| {
            w.cube(v(0.06, 1.1, 0.05), 0.0, FRAME, skirt * at(0.3, 0.05, 0.21));
        });
        // The hip's side armour over the thigh's ball joint, a raised panel on it.
        let side = sided(s, place(v(2.28, 1.55, -0.1), rz(0.14)));
        w.block(v(0.55, 1.5, 2.1), v2(0.85, 0.9), Vec2::ZERO, 0.16, BODY, side);
        w.block(v(0.16, 0.9, 1.3), v2(0.9, 0.9), Vec2::ZERO, 0.05, BODY, side * at(0.3, -0.05, 0.0));
        slot(&mut w, 0.5, side * Affine3A::from_rotation_y(FRAC_PI_2) * at(0.0, 0.0, 0.39));
        // The rear skirt's half, swept back over the seat.
        let rear = sided(s, place(v(0.95, 1.55, -1.45), rx(0.3)));
        w.block(v(1.6, 1.55, 0.6), v2(0.92, 0.85), v2(0.0, 0.05), 0.18, BODY, rear);
        slot(&mut w, 0.4, rear * at(0.25, 0.0, -0.31) * Affine3A::from_rotation_y(PI));
    }
}

// --- Shoulders: the homing missiles' armour. ---

/// The shoulder armour's axis (right side), out along x at this height and depth.
const ARMOUR_Y: f32 = 5.05;
const ARMOUR_Z: f32 = 0.05;
/// The shoulder armour, as hoops out along x from beside the chest, each as high as twice its
/// half width and as deep as twice its half depth: a big rounded box, its outer face set in a
/// little round its edge.
const ARMOUR: [Hoop; 5] = [
    hoop(2.26, 0.0, 0.0, 1.26, 1.36),
    hoop(2.42, 0.0, 0.0, 1.46, 1.54),
    hoop(4.66, 0.0, 0.0, 1.5, 1.56),
    hoop(4.9, 0.0, 0.0, 1.32, 1.4),
    hoop(4.98, 0.0, 0.0, 1.06, 1.16),
];

fn shoulders(d: &mut Designer) {
    for s in Side::BOTH {
        let bone = s.pick(Bone::ShoulderL, Bone::ShoulderR);
        let flat = sided(s, FLAT);
        let mut o = d.on(bone);
        o.seed(0.24 + s.pick(0.0, 0.4));
        let axis = sided(s, place(v(0.0, ARMOUR_Y, ARMOUR_Z), rz(-FRAC_PI_2)));
        o.loft(&ARMOUR, 3.6, 32, BODY, axis);
        // The red lip along its foot, and the plate along its top, two seams across it.
        o.hexa(stacked([(3.42, 2.62, 4.8, -1.42, 1.5), (3.84, 2.58, 4.86, -1.48, 1.56)]), 0.1, TRIM, flat);
        o.hexa(stacked([(6.42, 2.85, 4.55, -1.25, 1.3), (6.68, 2.9, 4.45, -1.16, 1.2)]), 0.1, BODY, flat);
        o.greeble(|o| {
            for z in [-0.4, 0.4] {
                o.cube(v(1.4, 0.04, 0.05), 0.0, FRAME, sided(s, at(3.7, 6.69, z)));
            }
        });
        // The outer face: a thick plate, a smaller panel raised on it with a round vernier port
        // and a slot, a red stripe over them, bolts at its corners.
        o.hexa(slab([(3.8, 4.8, 5.06, -1.24, 1.24), (6.25, 4.8, 5.0, -1.18, 1.18)], 0.0), 0.16, BODY, flat);
        o.hexa(slab([(4.15, 4.9, 5.12, -0.98, 0.98), (5.65, 4.9, 5.09, -0.94, 0.94)], 0.0), 0.05, BODY, flat);
        o.cube(v(0.08, 0.2, 1.9), 0.02, TRIM, sided(s, at(5.04, 5.98, 0.0)));
        port(&mut o, 0.28, sided(s, place(v(5.12, 4.62, 0.46), ry(FRAC_PI_2))));
        slot(&mut o, 0.6, sided(s, place(v(5.12, 4.95, -0.55), ry(FRAC_PI_2))));
        for (y, z) in [(4.0, -1.0), (4.0, 1.0), (6.0, -0.98), (6.0, 0.98)] {
            bolt(&mut o, sided(s, place(v(5.06, y, z), ry(FRAC_PI_2))));
        }
        // The vents in its back.
        louvre(&mut o, 1.3, 0.9, 4, sided(s, place(v(3.7, 5.0, -1.55), ry(PI))));
        // A running light at its outer front corner, high: red on the suit's own left (+x),
        // green on its right.
        let nav = sided(s, at(4.55, 6.66, 1.36));
        o.cube(v(0.34, 0.16, 0.22), 0.03, GUN, nav);
        let lamp = Paint::Light(s.pick(Light::Starboard, Light::Port));
        o.sphere(0.12, 6, lamp, nav * at(0.0, 0.06, 0.08) * Affine3A::from_scale(v(1.0, 0.6, 1.0)));
        // The homing missiles' hatches across its front.
        let doors = hatches(&mut o, 3, 3, v2(0.6, 0.58), sided(s, at(3.66, 5.1, 1.64)));
        for p in doors {
            d.sockets.missiles.push((bone, Designer::local(bone, p)));
        }
    }
}

// --- Arms. ---

/// The upper arm's drum, up the arm from the elbow: a raised ring round its middle.
const UPPER_ARM: [(f32, f32); 10] = [
    (0.0, 0.45),
    (0.7, 0.45),
    (0.8, 0.56),
    (0.8, 0.86),
    (0.88, 0.94),
    (0.88, 1.12),
    (0.8, 1.2),
    (0.8, 1.55),
    (0.7, 1.65),
    (0.0, 1.65),
];

/// The red cuff round the wrist, and the gauntlet up the forearm from it (x out, z forward): a
/// big rounded box swelling to the middle of the forearm and rounding in over the elbow.
const CUFF: [Hoop; 4] = [
    hoop(0.0, 0.0, 0.05, 0.88, 0.94),
    hoop(0.06, 0.0, 0.05, 0.95, 1.0),
    hoop(0.4, 0.0, 0.05, 0.95, 1.0),
    hoop(0.46, 0.0, 0.05, 0.88, 0.94),
];
const GAUNTLET: [Hoop; 6] = [
    hoop(0.46, 0.0, 0.05, 0.9, 0.96),
    hoop(0.52, 0.0, 0.05, 1.0, 1.06),
    hoop(1.2, 0.03, 0.04, 1.04, 1.1),
    hoop(1.62, 0.03, 0.0, 0.94, 0.98),
    hoop(1.9, 0.0, -0.02, 0.72, 0.74),
    hoop(2.0, 0.0, -0.02, 0.5, 0.52),
];

fn arms(d: &mut Designer) {
    let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    for s in Side::BOTH {
        let (up, fore) = s.pick((Bone::UpperArmL, Bone::ForearmL), (Bone::UpperArmR, Bone::ForearmR));
        let k = s.pick(0.0, 0.4);

        // The upper arm: a white drum with a raised ring, on the grey frame, out from under the
        // shoulder's armour.
        let up_xf = sided(s, place(el, toward(sh - el)));
        let mut u = d.on(up);
        u.seed(0.28 + k);
        u.sphere(0.68, 6, GREY, sided(s, Affine3A::from_translation(sh)));
        u.cylinder(0.46, 1.7, 12, GREY, up_xf * at(0.0, 0.95, 0.0));
        u.lathe(&UPPER_ARM, 16, BODY, up_xf);

        // The forearm: the elbow's barrel, then the red cuff and the big gauntlet, a grey panel
        // on its outer face, a raised plate down its front, the elbow's guard behind.
        let fo_xf = sided(s, place(wr, toward(el - wr)));
        let mut f = d.on(fore);
        f.seed(0.33 + k);
        f.cylinder(0.6, 1.2, 14, GREY, sided(s, place(el, rz(FRAC_PI_2))));
        f.loft(&CUFF, 3.2, 24, TRIM, fo_xf);
        f.loft(&GAUNTLET, 3.2, 24, BODY, fo_xf);
        f.hexa(slab([(0.6, 0.9, 1.16, -0.82, 0.8), (1.72, 0.84, 1.1, -0.74, 0.72)], 0.0), 0.1, BODY, fo_xf);
        f.cube(v(0.1, 0.62, 0.8), 0.02, GREY, fo_xf * at(1.16, 1.2, -0.04));
        f.block(v(0.8, 0.95, 0.14), v2(0.85, 1.0), Vec2::ZERO, 0.04, BODY, fo_xf * at(0.0, 1.05, 1.1));
        f.hexa(stacked([(1.45, -0.6, 0.6, -1.15, -0.5), (2.1, -0.5, 0.5, -0.9, -0.3)]), 0.1, BODY, fo_xf);
        f.greeble(|f| {
            f.cube(v(0.05, 0.08, 1.3), 0.0, FRAME, fo_xf * at(1.17, 0.78, 0.0));
            f.cube(v(0.6, 0.05, 0.04), 0.0, FRAME, fo_xf * at(0.0, 0.8, 1.18));
        });
        for y in [0.85, 1.55] {
            bolt(&mut f, fo_xf * place(v(1.15, y, 0.62), ry(FRAC_PI_2)));
        }
        f.cylinder(0.46, 0.3, 12, GREY, fo_xf);
    }
    crate::frames::hands(d, GREY);
}

// --- Legs. ---

/// The thigh's core, up its axis from the knee to the hip (x out, z forward): a rounded mass from
/// just over the knee to under the belt, which the plates stand on.
const THIGH: [Hoop; 6] = [
    hoop(0.45, 0.2, 0.05, 1.05, 0.98),
    hoop(1.0, 0.36, 0.06, 1.38, 1.18),
    hoop(2.4, 0.42, 0.02, 1.48, 1.24),
    hoop(3.2, 0.36, -0.04, 1.34, 1.12),
    hoop(3.65, 0.28, -0.05, 1.04, 0.9),
    hoop(3.92, 0.2, -0.05, 0.64, 0.58),
];

/// The thigh's plates, in the same frame (each its foot and its top, and how far its front falls
/// back per metre out along x): the front in two tiers, each in two halves meeting in a ridge, the
/// upper a step back from the lower; the plate down its outer side, a raised panel on it; and the
/// plate over its back.
const THIGH_PLATES: [([Cut; 2], f32); 7] = [
    ([(0.8, 0.32, 1.7, 0.4, 1.44), (2.05, 0.32, 1.68, 0.4, 1.42)], 0.3),
    ([(0.8, -0.85, 0.32, 0.4, 1.142), (2.05, -0.88, 0.32, 0.4, 1.114)], -0.255),
    ([(2.15, 0.32, 1.65, 0.4, 1.36), (3.25, 0.32, 1.46, 0.4, 1.24)], 0.3),
    ([(2.15, -0.85, 0.32, 0.4, 1.062), (3.25, -0.74, 0.32, 0.4, 0.97)], -0.255),
    ([(0.7, 1.6, 2.04, -1.0, 0.92), (3.15, 1.52, 1.94, -0.9, 0.84)], 0.0),
    ([(1.2, 1.96, 2.08, -0.62, 0.52), (2.6, 1.9, 2.02, -0.57, 0.47)], 0.0),
    ([(0.9, -0.6, 1.25, -1.4, -0.9), (3.0, -0.55, 1.2, -1.3, -0.85)], 0.0),
];

/// The lower leg's core, up its axis from the ankle to the knee (x out, z forward): the full calf,
/// swelling back and out to its widest under the knee and closing in behind the knee guard.
const SHIN: [Hoop; 8] = [
    hoop(0.9, 0.0, -0.05, 0.86, 0.86),
    hoop(1.6, 0.04, -0.1, 0.98, 0.98),
    hoop(2.6, 0.08, -0.24, 1.18, 1.16),
    hoop(3.5, 0.1, -0.34, 1.36, 1.3),
    hoop(4.3, 0.1, -0.38, 1.42, 1.34),
    hoop(4.9, 0.08, -0.34, 1.3, 1.2),
    hoop(5.35, 0.06, -0.28, 0.96, 0.9),
    hoop(5.62, 0.04, -0.22, 0.52, 0.5),
];

/// The lower leg's plates, in the same frame: the shin guard, in two halves meeting in a ridge
/// down its front and widening as the calf swells; a plate either side of the knee guard over the
/// calf's top, a step back from the shin guard; the plate down the inside of the calf, and the
/// one over its back.
const SHIN_PLATES: [([Cut; 2], f32); 6] = [
    ([(1.4, 0.05, 1.04, 0.2, 1.36), (4.1, 0.05, 1.22, 0.2, 1.5)], 0.42),
    ([(1.4, -0.94, 0.05, 0.2, 0.944), (4.1, -1.12, 0.05, 0.2, 1.002)], -0.42),
    ([(4.0, 0.5, 1.45, 0.0, 1.36), (5.1, 0.5, 1.32, 0.0, 1.16)], 0.42),
    ([(4.0, -1.42, -0.5, 0.0, 0.97), (5.1, -1.3, -0.5, 0.0, 0.81)], -0.42),
    ([(1.7, -1.5, -1.12, -1.05, 0.62), (4.95, -1.56, -1.12, -1.15, 0.66)], 0.0),
    ([(2.3, -0.85, 1.0, -1.5, -1.0), (4.95, -0.92, 1.06, -1.8, -1.2)], 0.0),
];

/// The micro-missiles' pod on the outside of the lower leg (right side, x as its inner and outer
/// faces, z its back and front), and where on its outer face the hatches sit.
const POD: [Cut; 2] = [(-6.3, 2.6, 3.52, -0.95, 0.7), (-2.9, 2.5, 3.66, -1.12, 0.98)];
const POD_HATCHES: Vec2 = Vec2::new(-4.75, -0.1);

fn legs(d: &mut Designer) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    for s in Side::BOTH {
        let (thigh, shin, foot, toe) = s.pick(
            (Bone::ThighL, Bone::ShinL, Bone::FootL, Bone::ToeL),
            (Bone::ThighR, Bone::ShinR, Bone::FootR, Bone::ToeR),
        );
        let k = s.pick(0.0, 0.2);
        let flat = sided(s, FLAT);

        // The thigh: a heavy core on a grey ball at the hip, plated front, side and back; seams
        // across the step in its front and down its side.
        let mut th = d.on(thigh);
        th.seed(0.31 + k);
        th.sphere(0.82, 6, GREY, sided(s, Affine3A::from_translation(hip)));
        let along_thigh = sided(s, place(knee, toward(hip - knee)));
        th.loft(&THIGH, 2.6, 24, BODY, along_thigh);
        for (plate, fall) in THIGH_PLATES {
            th.hexa(slab(plate, fall), 0.12, BODY, along_thigh);
        }
        th.greeble(|th| {
            th.cube(v(0.05, 1.6, 0.05), 0.0, FRAME, along_thigh * at(2.04, 1.95, 0.72));
            th.cube(v(0.9, 0.05, 0.05), 0.0, FRAME, along_thigh * place(v(0.95, 1.55, 1.3), ry(-0.29)));
        });
        bolt(&mut th, along_thigh * place(v(2.02, 2.9, -0.6), ry(FRAC_PI_2)));
        bolt(&mut th, along_thigh * place(v(2.06, 1.0, -0.6), ry(FRAC_PI_2)));

        // The shin: the knee's grey barrel, the calf's core plated front, inside and back, and the
        // red knee guard standing out of its front with its top tucked up under the thigh's plates.
        let tube = sided(s, place(ankle, toward(knee - ankle)));
        let mut m = d.on(shin);
        m.seed(0.37 + k);
        m.cylinder(0.64, 1.3, 14, GREY, sided(s, place(knee, rz(FRAC_PI_2))));
        m.loft(&SHIN, 2.6, 24, BODY, tube);
        for (plate, fall) in SHIN_PLATES {
            m.hexa(slab(plate, fall), 0.12, BODY, tube);
        }
        m.greeble(|m| {
            m.cube(v(0.05, 2.4, 0.05), 0.0, FRAME, tube * at(-1.59, 3.2, -0.2));
            m.cube(v(1.4, 0.05, 0.05), 0.0, FRAME, tube * at(0.08, 4.0, -1.86));
            for side in [-1.0, 1.0] {
                let half = tube * place(v(side * 0.5, 2.6, 1.22), ry(side * 0.4));
                m.cube(v(0.6, 0.05, 0.05), 0.0, FRAME, half);
            }
        });
        let guard = sided(s, at(knee.x + 0.02, knee.y, knee.z));
        m.hexa(
            [
                (-0.7, -0.9, 0.42),
                (0.7, -0.9, 0.42),
                (-0.62, 0.95, 0.22),
                (0.62, 0.95, 0.22),
                (-0.62, -1.0, 1.6),
                (0.62, -1.0, 1.6),
                (-0.56, 0.95, 1.16),
                (0.56, 0.95, 1.16),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.16,
            TRIM,
            guard,
        );
        m.hexa(
            [
                (-0.54, -1.5, 0.62),
                (0.54, -1.5, 0.62),
                (-0.66, -0.85, 0.46),
                (0.66, -0.85, 0.46),
                (-0.16, -1.5, 1.46),
                (0.16, -1.5, 1.46),
                (-0.26, -0.85, 1.34),
                (0.26, -0.85, 1.34),
            ]
            .map(|(x, y, z)| v(x, y, z)),
            0.08,
            TRIM,
            guard,
        );
        m.block(
            v(0.36, 0.9, 0.12),
            v2(0.85, 1.0),
            Vec2::ZERO,
            0.03,
            FRAME,
            guard * place(v(0.0, 0.05, 1.4), rx(-0.2)),
        );

        // The pod down the outside of the calf: a faceted box, its front red, the hatches in its
        // outer face, a vent under it.
        m.hexa(stacked(POD), 0.18, BODY, flat);
        let [(y0, i0, o0, _, f0), (y1, i1, o1, _, f1)] = POD;
        let cap = [
            (y0 + 0.15, i0 + 0.2, o0 - 0.08, f0 - 0.2, f0 + 0.1),
            (y1 - 0.15, i1 + 0.2, o1 - 0.08, f1 - 0.2, f1 + 0.1),
        ];
        m.hexa(stacked(cap), 0.08, TRIM, flat);
        let lean = (o1 - o0) / (y1 - y0);
        let (hy, hz) = (POD_HATCHES.x, POD_HATCHES.y);
        let outer = place(v(o0 + lean * (hy - y0), hy, hz), rz(-lean.atan()) * ry(FRAC_PI_2));
        let doors = hatches(&mut m, 2, 4, v2(0.62, 0.6), sided(s, outer));
        m.greeble(|m| {
            m.cube(v(0.5, 0.06, 1.2), 0.0, FRAME, sided(s, at((i0 + o0) * 0.5 + 0.2, y0 + 0.01, -0.1)));
        });

        // The boot's shaft round the ankle: a rounded box over the foot of the shin, a grey band
        // round its back at the joint, and the grey hubs either side where it meets the foot.
        let x = ankle.x;
        let (y0, y1) = (ankle.y - 0.6, ankle.y + 1.1);
        m.loft(
            &[
                hoop(y0, x, -0.1, 1.2, 1.04),
                hoop(y0 + 0.9, x, -0.12, 1.18, 1.02),
                hoop(y1, x, -0.14, 1.04, 0.92),
                hoop(y1 + 0.08, x, -0.14, 0.9, 0.8),
            ],
            3.6,
            24,
            BODY,
            flat,
        );
        // Its front: a thick plate rising from over the instep up the shin like a boot's tongue,
        // wider at its foot, a notch near its top.
        let tongue = [
            (-1.08, y0 + 0.15, 0.55),
            (1.08, y0 + 0.15, 0.55),
            (-0.88, y1 + 0.25, 0.62),
            (0.88, y1 + 0.25, 0.62),
            (-1.08, y0 + 0.15, 1.86),
            (1.08, y0 + 0.15, 1.86),
            (-0.88, y1 + 0.25, 1.24),
            (0.88, y1 + 0.25, 1.24),
        ]
        .map(|(dx, y, z)| v(x + dx, y, z));
        m.hexa(tongue, 0.2, BODY, flat);
        let rise = v(0.0, y1 + 0.25 - (y0 + 0.15), 1.24 - 1.86).normalize();
        let face = sided(s, place(v(x, y1 - 0.02, 1.34), toward(rise) * rx(0.0)));
        m.greeble(|m| {
            m.cube(v(0.44, 0.07, 0.05), 0.0, FRAME, face * at(-0.06, 0.0, 0.0));
            m.cube(v(0.07, 0.36, 0.05), 0.0, FRAME, face * at(-0.24, -0.15, 0.0));
            m.cube(v(1.3, 0.06, 0.05), 0.0, FRAME, face * at(0.0, -1.0, 0.04));
        });
        let back = sided(s, at(x, 0.0, ankle.z - 0.12));
        m.lathe_arc(
            &band(1.06, 1.2, ankle.y - 0.32, ankle.y + 0.08),
            18,
            0.2,
            PI - 0.2,
            GREY,
            back * Affine3A::from_rotation_y(PI),
        );
        for (side, r) in [(1.0, 0.64), (-1.0, 0.5)] {
            let hub = sided(s, place(v(x + side * 1.2, ankle.y - 0.2, ankle.z - 0.14), rz(FRAC_PI_2)));
            m.lathe(
                &[(0.0, -0.17), (r * 0.92, -0.17), (r, -0.1), (r, 0.1), (r * 0.92, 0.17), (0.0, 0.17)],
                14,
                GREY,
                hub,
            );
            m.cylinder(r * 0.45, 0.38, 10, FRAME, hub);
        }
        for p in doors {
            d.sockets.missiles.push((shin, Designer::local(shin, p)));
        }

        // The foot: a thick platform, the heel high at its back, the middle under the shaft's
        // tongue, and the long red toe (on its own hinge) falling away to its tip with a white
        // block raised along it; a dark sole under it all, split at the hinge, a vernier in each
        // half, and a grey band round the heel over it.
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.52, 6, GREY, sided(s, Affine3A::from_translation(ankle)));
        let base = SOLE + 0.32;
        f.hexa(boot(x, base, (-7.45, -7.7), (1.36, 1.3), [-2.2, -1.95, 1.28, 1.28]), 0.24, BODY, flat);
        f.hexa(
            boot(x, SOLE + 0.05, (base + 0.08, base + 0.08), (1.46, 1.46), [-2.32, -2.29, -0.6, -0.6]),
            0.05,
            GREY,
            flat,
        );
        f.greeble(|f| {
            for side in [-1.0, 1.0] {
                f.cube(v(0.04, 0.05, 3.3), 0.0, FRAME, sided(s, at(x + side * 1.22, -8.3, -0.35)));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, sided(s, at(x + side * 1.2, -8.25, 1.26)));
            }
            f.cube(v(0.5, 0.07, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.05, -2.1), rx(-0.25))));
        });
        let heel = Sole { z: (-2.3, 1.26), half: (1.44, 1.36), top: 0.34, pads: GREY, vernier: 0.1, r: 0.44 };
        sole(d, foot, s, x, heel);
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(boot(x, base, (-7.8, -8.06), (1.32, 1.12), [1.32, 1.32, 3.45, 3.0]), 0.34, TRIM, flat);
        t.hexa(boot(x, -8.1, (-7.62, -7.92), (0.62, 0.52), [1.36, 1.36, 2.9, 2.6]), 0.12, BODY, flat);
        let toes = Sole { z: (1.34, 3.55), half: (1.38, 1.2), top: 0.34, pads: GREY, vernier: 2.35, r: 0.36 };
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

/// The pack's body (right half), and the drums either side of it.
const PACK: [Cut; 2] = [(2.9, 0.0, 1.35, -2.75, -1.45), (5.4, 0.0, 1.2, -2.6, -1.45)];
const DRUM: Vec3 = Vec3::new(1.62, 4.1, -2.2);

fn backpack(d: &mut Designer) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    for s in Side::BOTH {
        let flat = sided(s, FLAT);
        b.hexa(stacked(PACK), 0.2, BODY, flat);
        // The red cap over its top.
        b.hexa(stacked([(5.3, 0.0, 1.05, -2.5, -1.6), (5.62, 0.0, 0.95, -2.4, -1.7)]), 0.08, TRIM, flat);
        // The drum on its side: a ribbed cylinder in gunmetal, domed at each end.
        let drum = sided(s, Affine3A::from_translation(DRUM));
        b.lathe(
            &[(0.0, -1.3), (0.4, -1.25), (0.56, -1.1), (0.56, 1.1), (0.4, 1.25), (0.0, 1.3)],
            16,
            GUN,
            drum,
        );
        for y in [-0.6, 0.6] {
            b.lathe(&band(0.5, 0.62, y - 0.09, y + 0.09), 16, GREY, drum);
        }
        b.cube(v(0.3, 1.4, 0.5), 0.05, GREY, sided(s, at(1.25, 4.1, -2.2)));
        // The vents down its back.
        louvre(&mut b, 0.7, 1.1, 4, sided(s, place(v(0.62, 4.35, -2.73), ry(PI) * rx(-0.06))));
        // A small vernier on the shoulder of the pack, pointing up and back.
        let bell = sided(s, place(v(0.85, 5.55, -2.45), toward(v(0.0, 0.8, -0.6))));
        b.lathe(
            &[(0.18, -0.08), (0.2, 0.0), (0.26, 0.18), (0.3, 0.32), (0.27, 0.33), (0.14, 0.04), (0.0, 0.02)],
            12,
            GUN,
            bell,
        );
    }
    // The round hatch in the middle of its back.
    let hatch = place(v(0.0, 3.55, -2.74), rx(-FRAC_PI_2));
    b.lathe(&band(0.32, 0.46, -0.02, 0.12), 18, GREY, hatch);
    b.lathe(&[(0.0, 0.0), (0.34, 0.0), (0.3, 0.08), (0.0, 0.1)], 16, BODY, hatch);
    // A carry handle across its top.
    for x in [-0.5, 0.5] {
        b.cube(v(0.14, 0.4, 0.16), 0.02, GREY, at(x, 5.78, -2.0));
    }
    b.cube(v(1.14, 0.14, 0.16), 0.02, GREY, at(0.0, 5.98, -2.0));
    // The main thrusters under it.
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.72, 2.72, -2.3)), v(0.0, -0.55, -0.85), 0.5);
    }
}

// --- The beam gatling. ---

/// The gatling's axis over its grip, and the heights along it (z from the grip): the receiver's
/// back and front, the drum's front, the barrels' ends. It's held as a pistol is (`crate::hold`):
/// its body all ahead of the grip and high over it, so at the ready the forearm lies under the
/// handle behind it, and carried, muzzle down, it hangs clear of the leg in front of it.
const GAT_UP: f32 = 1.15;
const RECEIVER: (f32, f32) = (-0.05, 1.85);
const ROTOR: f32 = 3.3;
const BARRELS: f32 = 7.2;

/// The beam gatling in the right hand: a gunmetal receiver on its pistol grip, its top a white
/// cover with a carry handle running back over the forearm and a sight beside it, a power pack on
/// its outer side; the red-banded drum turning six barrels in two clamps, each with its collar and
/// bore.
fn gatling(d: &mut Designer) {
    let g = j(Bone::Weapon);
    let axis = |z: f32| v(g.x, g.y + GAT_UP, g.z + z);
    let along = |z: f32| place(axis(z), rx(FRAC_PI_2));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    // The grip in the fist, raked back, and the trigger's guard ahead of it.
    w.block(
        v(0.32, 0.62, 0.42),
        v2(1.0, 1.1),
        Vec2::ZERO,
        0.05,
        GUN,
        place(v(g.x, g.y + 0.16, g.z + 0.2), rx(-0.15)),
    );
    w.cube(v(0.1, 0.08, 0.4), 0.02, GUN, at(g.x, g.y + 0.3, g.z + 0.6));
    w.cube(v(0.1, 0.22, 0.08), 0.02, GUN, at(g.x, g.y + 0.38, g.z + 0.78));
    // The receiver, and its white cover.
    let (r0, r1) = RECEIVER;
    let mid = (r0 + r1) * 0.5;
    w.block(
        v(1.04, 1.2, r1 - r0),
        v2(0.88, 0.94),
        Vec2::ZERO,
        0.14,
        GUN,
        at(g.x, g.y + GAT_UP - 0.1, g.z + mid),
    );
    let top = g.y + GAT_UP + 0.5;
    w.block(
        v(0.84, 0.2, r1 - r0 - 0.3),
        v2(0.8, 0.92),
        Vec2::ZERO,
        0.06,
        BODY,
        at(g.x, top + 0.08, g.z + mid),
    );
    w.greeble(|w| {
        for k in 0..5 {
            let z = g.z + r0 + 0.45 + k as f32 * 0.24;
            w.cube(v(0.05, 0.5, 0.1), 0.0, FRAME, at(g.x - 0.5, g.y + GAT_UP - 0.15, z));
        }
        w.cube(v(0.84, 0.04, 0.04), 0.0, FRAME, at(g.x, top + 0.19, g.z + r1 - 0.45));
    });
    // The carry handle over the cover, running back high over the forearm: two posts and the bar.
    for z in [0.25, 1.45] {
        w.cube(v(0.16, 0.36, 0.2), 0.03, GUN, at(g.x + 0.12, top + 0.32, g.z + z));
    }
    w.cube(v(0.18, 0.16, 2.2), 0.04, GUN, at(g.x + 0.12, top + 0.54, g.z + 0.5));
    // The sight beside it on the cover, its round lens looking forward.
    w.cube(v(0.24, 0.3, 0.7), 0.03, GUN, at(g.x - 0.24, top + 0.32, g.z + 1.1));
    w.cylinder(0.1, 0.08, 10, LENS, place(v(g.x - 0.24, top + 0.34, g.z + 1.46), rx(FRAC_PI_2)));
    // The power pack on its outer side, a red strip down it, its cable's plug at its back.
    let pack = at(g.x + 0.7, g.y + GAT_UP - 0.15, g.z + 0.95);
    w.block(v(0.36, 0.8, 1.5), v2(0.8, 0.92), Vec2::ZERO, 0.08, GUN, pack);
    w.cube(v(0.08, 0.5, 1.1), 0.02, TRIM, pack * at(0.18, 0.0, 0.0));
    w.cylinder(0.14, 0.2, 10, GREY, pack * place(v(0.0, 0.0, -0.8), rx(FRAC_PI_2)));
    // The drum turning the barrels, a red band round it.
    w.lathe(
        &[
            (0.0, r1 - 0.1),
            (0.74, r1 - 0.1),
            (0.86, r1 + 0.04),
            (0.86, ROTOR - 0.2),
            (0.74, ROTOR - 0.04),
            (0.5, ROTOR),
            (0.0, ROTOR),
        ],
        20,
        GUN,
        along(0.0),
    );
    w.lathe(&band(0.84, 0.9, r1 + 0.45, r1 + 0.8), 20, TRIM, along(0.0));
    w.greeble(|w| {
        for k in 0..8 {
            let a = k as f32 * TAU / 8.0;
            let p = v(0.86 * a.cos(), 0.86 * a.sin(), ROTOR - 0.45);
            w.cube(v(0.08, 0.08, 0.36), 0.0, FRAME, place(axis(0.0) + p, rz(a)));
        }
    });
    // The six barrels round the spindle, their collars and bores; the two clamps round them.
    let (b0, b1) = (ROTOR - 0.3, BARRELS);
    w.cylinder(0.22, b1 - b0 - 0.4, 12, GUN, along(0.0) * at(0.0, (b0 + b1 - 0.4) * 0.5, 0.0));
    for k in 0..6 {
        let a = k as f32 * TAU / 6.0;
        let off = v(0.42 * a.cos(), 0.42 * a.sin(), 0.0);
        let barrel = place(axis(0.0) + off, rx(FRAC_PI_2));
        w.cylinder(0.13, b1 - b0, 10, STEEL, barrel * at(0.0, (b0 + b1) * 0.5, 0.0));
        w.cylinder(0.17, 0.3, 10, GUN, barrel * at(0.0, b1 - 0.2, 0.0));
        w.greeble(|w| {
            w.cylinder(0.08, 0.04, 8, FRAME, barrel * at(0.0, b1 - 0.04, 0.0));
        });
    }
    for z in [b0 + 1.5, b1 - 1.0] {
        w.lathe(
            &[
                (0.0, z - 0.14),
                (0.62, z - 0.14),
                (0.68, z - 0.07),
                (0.68, z + 0.07),
                (0.62, z + 0.14),
                (0.0, z + 0.14),
            ],
            16,
            GUN,
            along(0.0),
        );
    }
    d.sockets.muzzle = Designer::local(Bone::Weapon, axis(b1 + 0.1));
}

// --- The combat knife. ---

/// The combat knife in the left hand, blade forward: a wrapped grip, a guard, and a heavy blade
/// with a clipped point, a fuller near its spine and its honed edge bright.
fn knife(d: &mut Designer) {
    let wr = j(Bone::HandL);
    let hilt = v(wr.x - 0.1, wr.y - 0.55, wr.z + 0.2);
    let dir = v(0.0, 0.3, 0.95).normalize();
    let grip = place(hilt, toward(dir));
    let mut k = d.on(Bone::HandL);
    k.seed(0.72);
    k.cylinder(0.16, 1.0, 10, GUN, grip);
    k.cube(v(0.3, 0.2, 0.36), 0.04, GUN, grip * at(0.0, -0.55, 0.0));
    k.greeble(|k| {
        for y in [-0.25, 0.0, 0.25] {
            k.cylinder(0.175, 0.05, 8, FRAME, grip * at(0.0, y, 0.0));
        }
    });
    let base = hilt + dir * 0.5;
    // The guard, across the blade's flat.
    k.cube(v(0.36, 0.14, 0.82), 0.04, GUN, place(base, toward(dir)));
    // The blade: its flat in the plane of `dir` and up, the spine up, the edge down.
    let flat = place(base, toward(dir) * ry(FRAC_PI_2));
    let blade = [
        v2(-0.2, 0.0),
        v2(0.26, 0.0),
        v2(0.27, 1.65),
        v2(0.12, 2.25),
        v2(-0.02, 2.62),
        v2(-0.16, 2.1),
        v2(-0.2, 1.0),
    ];
    k.extrude(&blade, 0.14, STEEL, flat);
    let edge = [
        v2(-0.31, 0.06),
        v2(-0.18, 0.06),
        v2(-0.14, 1.0),
        v2(-0.1, 2.1),
        v2(0.08, 2.42),
        v2(-0.02, 2.78),
        v2(-0.27, 2.12),
        v2(-0.31, 1.0),
    ];
    k.extrude(&edge, 0.06, Paint::Metal(paint::WHITE), flat);
    k.greeble(|k| {
        for z in [-0.075, 0.075] {
            k.cube(v(0.08, 1.4, 0.02), 0.0, FRAME, flat * at(0.15, 0.85, z));
        }
    });
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, base), dir));
    d.sockets.saber = (Designer::local(Bone::HandL, base), dir);
}
