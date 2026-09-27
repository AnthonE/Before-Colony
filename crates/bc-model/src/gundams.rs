//! The Operation Meteor Gundams and Neo-Bird, on the same skeleton and helpers as `frames`. Each
//! carries its own kit, and the sockets the client draws that kit from: blades, the Dragon Fang
//! and its flamethrower, missile hatches.
//!
//! Blades held in a hand stay short (the hand's armour must stay near its hit capsule); long
//! weapons ride the `Weapon` bone.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Quat, Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GLASS, GUN, STEEL, THROAT, TRIM, YELLOW, along, arm_segments, at, hands,
    inner_frame, j, nozzle, place, plate, rifle, rx, ry, rz, shield, sided, sp, v, v2,
};
use crate::kit::Paint;
use crate::paint;
use crate::rig::{Bone, Side};

/// A heated blade's edge.
const HOT: Paint = Paint::Glow(paint::RED);

/// Local +y along `dir`.
fn toward(dir: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, dir.normalize())
}

/// A head: the helmet block, visor, chin, eye slits and cheek guards.
fn helmet(d: &mut Designer, size: Vec3, top: Vec2, chin: Paint) {
    let mut h = d.on(Bone::Head);
    h.block(size, top, v2(0.0, -0.05), 0.12, BODY, at(0.0, 6.7, 0.1));
    h.cube(v(size.x * 0.64, 0.55, 0.3), 0.05, GLASS, at(0.0, 6.6, 0.1 + size.z * 0.48));
    h.cube(v(0.34, 0.28, 0.3), 0.05, chin, at(0.0, 6.18, 0.12 + size.z * 0.5));
    for s in Side::BOTH {
        h.cube(v(0.3, 0.1, 0.08), 0.02, EYE, sided(s, place(v(0.23, 6.73, 0.24 + size.z * 0.5), rz(-0.12))));
        h.cube(v(0.32, 0.75, size.z * 0.7), 0.06, BODY, sided(s, at(size.x * 0.5 + 0.05, 6.65, 0.05)));
    }
}

/// A V-fin from the forehead: `span` wide each side, `rise` high.
fn v_fin(d: &mut Designer, span: f32, rise: f32, sweep: f32) {
    for s in Side::BOTH {
        let fin = [v2(0.0, 0.0), v2(0.3, 0.12), v2(span, rise), v2(span - 0.15, rise + 0.07), v2(0.05, 0.3)];
        d.on(Bone::Head).extrude(&fin, 0.1, YELLOW, sided(s, place(v(0.02, 7.05, 1.0), rx(-sweep))));
    }
}

/// Waist block and skirt plates.
fn waist(d: &mut Designer, front: Paint, side: Paint) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.19);
    w.cube(v(3.3, 0.45, 2.5), 0.08, FRAME, at(0.0, 0.25, 0.0));
    w.block(v(1.0, 1.3, 1.6), v2(1.5, 1.1), Vec2::ZERO, 0.1, BODY, at(0.0, -0.55, 0.25));
    w.cube(v(0.7, 0.45, 0.4), 0.05, ACCENT, at(0.0, -0.2, 1.1));
    for s in Side::BOTH {
        plate(d, Bone::Waist, s, v(1.3, 1.9, 0.3), v(1.05, -0.8, 1.25), rz(0.1) * rx(0.12), 0.08, front);
        plate(d, Bone::Waist, s, v(0.3, 1.8, 1.9), v(2.2, -0.55, 0.0), rz(0.12), 0.08, side);
    }
    plate(d, Bone::Waist, Side::R, v(2.4, 1.4, 0.3), v(0.0, -0.4, -1.35), rx(-0.12), 0.08, BODY);
}

/// Thighs, shins and feet: `thigh` and `shin` sizes, knee guards and toes in `trim`.
fn legs(d: &mut Designer, thigh: Vec3, shin: Vec3, trim: Paint, toe: Paint) {
    for s in Side::BOTH {
        let (th, sh, ft) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(th).seed(0.3 + s.pick(0.0, 0.2)).block(
            thigh,
            v2(1.05, 1.1),
            Vec2::ZERO,
            0.15,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.15), hip + v(0.0, -0.4, 0.0))),
        );
        let mut m = d.on(sh);
        m.seed(0.36 + s.pick(0.0, 0.2));
        m.block(
            shin,
            v2(0.85, 0.9),
            v2(0.0, -0.1),
            0.2,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.0), knee + v(0.0, -0.3, 0.0))),
        );
        m.block(
            v(1.2, 1.2, 0.55),
            v2(0.8, 0.7),
            Vec2::ZERO,
            0.1,
            trim,
            sided(s, place(knee + v(0.0, 0.1, 0.95), rx(-0.2))),
        );
        let mut f = d.on(ft);
        f.seed(0.42 + s.pick(0.0, 0.2));
        f.block(v(1.5, 0.9, 3.2), v2(0.8, 0.65), v2(0.0, -0.2), 0.12, BODY, sided(s, at(1.3, -8.45, 0.45)));
        f.cube(v(1.35, 0.35, 0.8), 0.06, toe, sided(s, at(1.3, -8.6, 1.8)));
        f.cube(v(1.6, 0.25, 3.4), 0.04, FRAME, sided(s, at(1.3, -8.95, 0.45)));
    }
}

/// A flat backpack with a pair of main thrusters.
fn backpack(d: &mut Designer, size: Vec3, bell: f32) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.5);
    b.block(size, v2(0.9, 0.9), v2(0.0, 0.1), 0.2, BODY, at(0.0, 4.0, -2.6));
    b.cube(v(size.x * 0.6, size.y * 0.55, 0.5), 0.08, TRIM, at(0.0, 4.2, -2.6 - size.z * 0.55));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.7, 3.2, -3.5)), v(0.0, -0.3, -1.0), bell);
    }
}

/// A box of missile tubes at `pos` on `bone`, hatches facing `face`; one launch socket at its
/// front.
fn missile_pod(d: &mut Designer, bone: Bone, pos: Vec3, size: Vec3, face: Vec3, rows: u32, cols: u32) {
    let rot = Quat::from_rotation_arc(Vec3::Z, face.normalize());
    let mut p = d.on(bone);
    p.cube(size, 0.08, BODY, place(pos, rot));
    let front = pos + rot * v(0.0, 0.0, size.z * 0.5);
    p.greeble(|p| {
        for r in 0..rows {
            for c in 0..cols {
                let x = (c as f32 + 0.5) / cols as f32 - 0.5;
                let y = (r as f32 + 0.5) / rows as f32 - 0.5;
                let at = front + rot * v(x * size.x * 0.8, y * size.y * 0.8, 0.02);
                p.cube(
                    v(size.x * 0.6 / cols as f32, size.y * 0.6 / rows as f32, 0.08),
                    0.0,
                    FRAME,
                    place(at, rot),
                );
            }
        }
    });
    d.sockets.missiles.push((bone, Designer::local(bone, front)));
}

// --- XXXG-01H Gundam Heavyarms: a walking arsenal. ---

pub(crate) fn heavyarms(d: &mut Designer) {
    inner_frame(d, 1.3);
    hands(d, FRAME);

    // Head: white, a short yellow V-fin over a red crest, a red chin.
    helmet(d, v(1.55, 1.2, 1.7), v2(0.8, 0.8), TRIM);
    v_fin(d, 1.15, 0.8, 0.2);
    let mut h = d.on(Bone::Head);
    h.seed(0.06);
    h.block(v(0.5, 0.45, 0.5), v2(0.5, 0.6), v2(0.0, -0.1), 0.05, TRIM, at(0.0, 7.2, 0.75));
    h.cube(v(0.32, 0.3, 1.2), 0.08, BODY, at(0.0, 7.4, -0.1));

    // Chest: broad, red below, with the hatches over the gatlings Full Open fires.
    let mut c = d.on(Bone::Chest);
    c.seed(0.12);
    c.block(v(4.9, 2.7, 3.2), v2(1.0, 0.9), v2(0.0, -0.1), 0.22, BODY, at(0.0, 4.1, 0.05));
    c.cube(v(3.0, 1.2, 2.7), 0.15, TRIM, at(0.0, 2.55, 0.0));
    c.block(v(2.4, 0.55, 2.3), v2(0.9, 0.9), Vec2::ZERO, 0.1, BODY, at(0.0, 5.55, -0.1));
    for s in Side::BOTH {
        c.cube(v(1.3, 1.25, 0.3), 0.06, TRIM, sided(s, at(0.95, 3.95, 1.72)));
        // Three barrels a side behind the hatch.
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0;
            let p = v(0.95 + 0.22 * a.cos(), 3.6 + 0.22 * a.sin(), 1.95);
            c.cylinder(0.11, 0.7, 8, GUN, sided(s, place(p, rx(FRAC_PI_2))));
        }
        c.greeble(|c| {
            for k in 0..3 {
                c.cube(v(1.1, 0.07, 0.1), 0.0, FRAME, sided(s, at(0.95, 4.25 + k as f32 * 0.18, 1.88)));
            }
        });
    }
    d.on(Bone::Torso).seed(0.17).block(
        v(2.7, 1.3, 2.2),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 1.45, 0.05),
    );
    waist(d, BODY, TRIM);

    // Shoulders: tall blocks, the homing missiles' pods on top.
    for s in Side::BOTH {
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.23 + s.pick(0.0, 0.4));
        sh.block(v(2.2, 2.2, 2.8), v2(0.9, 0.9), v2(0.1, 0.0), 0.2, BODY, sided(s, at(4.1, 4.8, 0.0)));
        sh.cube(v(2.3, 0.4, 2.9), 0.06, TRIM, sided(s, at(4.1, 3.75, 0.0)));
        // The pods ride the chest (the simulation ties them to the torso).
        missile_pod(d, Bone::Chest, sp(s, v(3.45, 6.2, 0.2)), v(1.3, 0.8, 1.8), Vec3::Z, 2, 3);
    }

    arm_segments(d, v(1.45, 1.9, 1.45), v(1.75, 2.1, 1.8), BODY, TRIM);

    // Legs: heavy, the micro-missile pods on the outside of the thighs.
    legs(d, v(1.75, 3.2, 1.9), v(2.1, 3.5, 2.4), TRIM, TRIM);
    for s in Side::BOTH {
        let thigh = s.pick(Bone::ThighL, Bone::ThighR);
        missile_pod(d, thigh, sp(s, v(2.45, -2.6, 0.3)), v(0.7, 1.6, 1.5), Vec3::Z, 3, 2);
    }

    backpack(d, v(2.8, 2.6, 1.7), 0.6);

    // The beam gatling: six barrels round a drum, on the right arm.
    let g = j(Bone::Weapon);
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    w.cube(v(0.4, 1.0, 0.5), 0.05, GUN, place(v(g.x, g.y - 0.2, g.z), rx(-0.25)));
    w.cylinder(0.95, 2.6, 16, TRIM, place(v(g.x, g.y + 0.5, g.z + 0.6), rx(FRAC_PI_2)));
    for k in 0..6 {
        let a = k as f32 * TAU / 6.0;
        let p = v(g.x + 0.45 * a.cos(), g.y + 0.5 + 0.45 * a.sin(), g.z + 4.1);
        w.cylinder(0.16, 4.4, 8, GUN, place(p, rx(FRAC_PI_2)));
    }
    w.cylinder(0.7, 0.25, 14, STEEL, place(v(g.x, g.y + 0.5, g.z + 5.4), rx(FRAC_PI_2)));
    w.greeble(|w| {
        w.cube(v(0.5, 0.5, 1.8), 0.05, GUN, at(g.x - 0.9, g.y + 0.5, g.z + 0.4));
    });
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + 0.5, g.z + 6.3));

    // The army knife in the left hand, blade forward.
    let wr = j(Bone::HandL);
    let hilt = v(wr.x - 0.1, wr.y - 0.55, wr.z + 0.2);
    let dir = v(0.0, 0.3, 0.95).normalize();
    let mut k = d.on(Bone::HandL);
    k.cylinder(0.18, 0.9, 8, GUN, place(hilt, toward(dir)));
    let base = hilt + dir * 0.45;
    let blade = [v2(-0.22, 0.0), v2(0.22, 0.0), v2(0.16, 1.9), v2(0.0, 2.3), v2(-0.24, 1.8)];
    k.extrude(&blade, 0.1, STEEL, place(base, toward(dir) * ry(FRAC_PI_2)));
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, base), dir));
    d.sockets.saber = (Designer::local(Bone::HandL, base), dir);
}

// --- XXXG-01D Gundam Deathscythe: bat-wing binders, a beam scythe, the buster shield. ---

pub(crate) fn deathscythe(d: &mut Designer) {
    inner_frame(d, 1.2);
    hands(d, FRAME);

    // Head: a narrow dark helmet under a tall swept V-fin, a pointed red chin.
    helmet(d, v(1.4, 1.2, 1.75), v2(0.75, 0.7), ACCENT);
    v_fin(d, 1.9, 1.5, 0.3);
    let mut h = d.on(Bone::Head);
    h.seed(0.04);
    h.block(v(0.3, 0.9, 1.4), v2(0.5, 0.4), v2(0.0, -0.3), 0.05, BODY, at(0.0, 7.4, -0.1));
    for s in Side::BOTH {
        h.cylinder(0.13, 0.5, 8, GUN, sided(s, place(v(0.88, 6.85, 0.5), rx(FRAC_PI_2))));
    }

    // Chest: slim and dark, blue vents.
    let mut c = d.on(Bone::Chest);
    c.seed(0.11);
    c.block(v(4.4, 2.5, 2.9), v2(1.05, 0.85), v2(0.0, -0.15), 0.2, BODY, at(0.0, 4.15, 0.05));
    c.block(v(1.4, 1.6, 0.4), v2(0.6, 1.0), Vec2::ZERO, 0.06, TRIM, at(0.0, 4.1, 1.5));
    c.cube(v(2.8, 1.2, 2.5), 0.15, TRIM, at(0.0, 2.55, 0.0));
    c.block(v(2.3, 0.5, 2.2), v2(0.9, 0.9), Vec2::ZERO, 0.1, BODY, at(0.0, 5.5, -0.1));
    for s in Side::BOTH {
        c.greeble(|c| {
            for k in 0..3 {
                c.cube(v(0.9, 0.07, 0.1), 0.0, ACCENT, sided(s, at(1.3, 4.0 + k as f32 * 0.22, 1.5)));
            }
        });
    }
    d.on(Bone::Torso).seed(0.15).block(
        v(2.4, 1.3, 2.0),
        v2(1.25, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 1.45, 0.05),
    );
    waist(d, TRIM, BODY);

    // Shoulders: swept, pointed.
    for s in Side::BOTH {
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.24 + s.pick(0.0, 0.4));
        sh.block(v(1.9, 1.8, 2.6), v2(0.6, 0.8), v2(0.35, 0.0), 0.15, BODY, sided(s, at(4.0, 4.9, 0.0)));
        sh.block(
            v(0.3, 1.4, 2.2),
            v2(1.0, 0.4),
            v2(0.0, -0.4),
            0.04,
            TRIM,
            sided(s, place(v(4.8, 5.5, -0.1), rz(-0.35))),
        );
    }
    arm_segments(d, v(1.25, 1.9, 1.25), v(1.5, 2.0, 1.6), BODY, TRIM);
    legs(d, v(1.55, 3.2, 1.7), v(1.85, 3.5, 2.1), TRIM, ACCENT);

    backpack(d, v(2.4, 2.4, 1.5), 0.55);

    // Bat-wing binders: ribs fanned from the shoulders, with the membrane between.
    for s in Side::BOTH {
        let bone = s.pick(Bone::WingL, Bone::WingR);
        let root = j(Bone::WingR);
        let mut wg = d.on(bone);
        wg.seed(0.56 + s.pick(0.0, 0.2));
        let ribs = [v(0.75, 0.62, -0.25), v(0.95, 0.05, -0.3), v(0.75, -0.65, -0.2)];
        let lens = [6.5, 7.5, 6.0];
        for (k, (dir, len)) in ribs.iter().zip(lens).enumerate() {
            let tip = root + dir.normalize() * len;
            wg.seed(0.6 + k as f32 * 0.05).block(
                v(0.45, len, 0.4),
                v2(0.4, 0.6),
                Vec2::ZERO,
                0.06,
                BODY,
                sided(s, along(root, tip)),
            );
        }
        let web = [
            v2(0.0, 0.0),
            v2(4.0, 3.9),
            v2(5.0, 2.2),
            v2(7.3, 0.4),
            v2(5.2, -1.3),
            v2(4.6, -3.9),
            v2(0.3, -0.6),
        ];
        wg.extrude(&web, 0.12, TRIM, sided(s, place(root + v(0.0, 0.0, -0.3), ry(0.25))));
    }

    // The beam scythe: a long haft in the right hand, the emitter at its head.
    let g = j(Bone::Weapon);
    let (low, high) = (v(g.x, g.y - 3.6, g.z - 0.2), v(g.x, g.y + 6.4, g.z + 0.3));
    let mut w = d.on(Bone::Weapon);
    w.seed(0.62);
    w.cylinder(0.2, low.distance(high), 10, GUN, along(low, high));
    w.cylinder(0.3, 0.8, 10, TRIM, along(g + v(0.0, -0.6, 0.0), g + v(0.0, 0.2, 0.0)));
    let head = high + v(0.0, -0.3, 0.0);
    w.block(v(0.7, 1.3, 1.6), v2(0.6, 0.5), v2(0.0, 0.3), 0.08, ACCENT, place(head, rx(-0.3)));
    w.cylinder(0.22, 0.6, 8, THROAT, place(head + v(0.0, 0.0, 0.9), rx(FRAC_PI_2)));
    // The beam leaves the emitter forward and down: the scythe's blade.
    let blade = v(0.0, -0.35, 1.0).normalize();
    let emitter = head + v(0.0, 0.0, 1.2);
    d.sockets.blade_right = Some((Designer::local(Bone::Weapon, emitter), blade));
    d.sockets.muzzle = Designer::local(Bone::Weapon, emitter);
    // The saber socket keeps the left arm's rest (the scythe is the right's).
    let wr = j(Bone::HandL);
    d.sockets.saber =
        (Designer::local(Bone::HandL, v(wr.x, wr.y - 0.5, wr.z + 0.4)), v(0.0, 0.62, 0.78).normalize());

    // The buster shield: a long pointed shield whose tip opens on the beam.
    shield(
        d,
        &[(0.0, 4.4), (0.9, 2.4), (1.4, -0.6), (0.9, -2.6), (-0.9, -2.6), (-1.4, -0.6), (-0.9, 2.4)],
        ACCENT,
        BODY,
    );
    let sp_ = j(Bone::Shield);
    d.on(Bone::Shield).greeble(|s| {
        s.cube(v(0.3, 0.3, 0.8), 0.04, THROAT, place(sp_ + v(-0.45, -0.6, 2.9), rx(0.4)));
    });
}

// --- XXXG-01SR Gundam Sandrock: heat shotels, a beam machine gun, the Cross Crusher. ---

pub(crate) fn sandrock(d: &mut Designer) {
    inner_frame(d, 1.4);
    hands(d, FRAME);

    // Head: broad, a single swept crest, a white face.
    helmet(d, v(1.7, 1.2, 1.8), v2(0.85, 0.85), TRIM);
    let mut h = d.on(Bone::Head);
    h.seed(0.03);
    let crest = [v2(0.0, 0.0), v2(0.4, 0.0), v2(0.25, 1.4), v2(-0.9, 1.7), v2(-0.3, 0.9)];
    h.extrude(&crest, 0.16, YELLOW, place(v(0.0, 7.1, 0.8), ry(-FRAC_PI_2)));
    h.cube(v(1.1, 0.25, 0.3), 0.04, TRIM, at(0.0, 6.35, 1.0));

    // Chest: deep, vented; the shoulders open to vent heat.
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    c.block(v(5.0, 2.8, 3.3), v2(1.0, 0.9), v2(0.0, -0.1), 0.25, BODY, at(0.0, 4.05, 0.05));
    c.cube(v(3.1, 1.2, 2.8), 0.15, TRIM, at(0.0, 2.5, 0.0));
    c.block(v(2.5, 0.55, 2.4), v2(0.9, 0.9), Vec2::ZERO, 0.1, BODY, at(0.0, 5.55, -0.1));
    c.cube(v(0.9, 0.6, 0.3), 0.06, ACCENT, at(0.0, 3.0, 1.55));
    c.greeble(|c| {
        for k in 0..5 {
            c.cube(v(2.6, 0.08, 0.12), 0.0, FRAME, at(0.0, 3.6 + k as f32 * 0.2, 1.74));
        }
    });
    // Its homing missiles ride the chest's upper corners.
    for s in Side::BOTH {
        missile_pod(d, Bone::Chest, sp(s, v(2.05, 5.45, 1.1)), v(1.0, 0.7, 0.6), v(0.25, 0.3, 1.0), 2, 2);
    }
    d.on(Bone::Torso).seed(0.18).block(
        v(2.8, 1.4, 2.3),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 1.45, 0.05),
    );
    waist(d, BODY, TRIM);

    // Shoulders: big, rounded, with vent louvres.
    for s in Side::BOTH {
        let axis = sided(s, place(v(4.2, 4.7, 0.0), rz(-FRAC_PI_2)));
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.25 + s.pick(0.0, 0.4));
        sh.lathe(
            &[(0.0, -1.2), (1.3, -1.1), (1.7, -0.3), (1.6, 0.5), (1.1, 1.0), (0.0, 1.1)],
            16,
            BODY,
            axis,
        );
        sh.greeble(|sh| {
            for k in 0..3 {
                sh.cube(v(0.12, 0.9, 1.9), 0.0, FRAME, sided(s, at(5.0, 5.2 - k as f32 * 0.35, 0.0)));
            }
        });
    }
    arm_segments(d, v(1.5, 1.9, 1.5), v(1.85, 2.1, 1.9), BODY, TRIM);
    legs(d, v(1.85, 3.2, 2.0), v(2.2, 3.5, 2.5), TRIM, ACCENT);
    backpack(d, v(2.9, 2.6, 1.7), 0.6);

    // The beam machine gun in the right hand, the right shotel slung under it; the left shotel in
    // the left hand. Both blades glow at the edge.
    rifle(d, 5.0, 0.3, GUN, TRIM);
    let g = j(Bone::Weapon);
    let dir_r = v(0.15, -0.35, 0.92).normalize();
    let hilt_r = g + v(0.0, -0.6, 0.2);
    shotel(d, Bone::Weapon, hilt_r, dir_r, 1.0);
    d.sockets.blade_right = Some((Designer::local(Bone::Weapon, hilt_r), dir_r));
    let wr = j(Bone::HandL);
    let hilt_l = v(wr.x - 0.1, wr.y - 0.5, wr.z + 0.3);
    let dir_l = v(-0.05, 0.6, 0.8).normalize();
    shotel(d, Bone::HandL, hilt_l, dir_l, -1.0);
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, hilt_l), dir_l));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt_l), dir_l);

    shield(d, &[(0.0, 2.8), (1.3, 2.2), (1.3, -1.6), (0.0, -2.8), (-1.3, -1.6), (-1.3, 2.2)], TRIM, BODY);
}

/// A heat shotel on `bone`: a grip at `hilt`, a crescent blade out along `dir` curving to side
/// `curl` (+1 right, -1 left), its inner edge glowing.
fn shotel(d: &mut Designer, bone: Bone, hilt: Vec3, dir: Vec3, curl: f32) {
    let rot = toward(dir) * ry(FRAC_PI_2);
    let mut s = d.on(bone);
    s.cylinder(0.2, 1.0, 8, GUN, place(hilt - dir * 0.3, toward(dir)));
    let c = curl;
    let outer = [
        v2(0.0, 0.2),
        v2(0.5 * c, 1.2),
        v2(0.75 * c, 2.3),
        v2(0.55 * c, 3.2),
        v2(0.0, 3.6),
        v2(0.25 * c, 2.5),
        v2(0.2 * c, 1.3),
    ];
    let o: Vec<Vec2> = if c > 0.0 { outer.to_vec() } else { outer.iter().rev().copied().collect() };
    s.extrude(&o, 0.14, STEEL, place(hilt, rot));
    let edge = [
        v2(0.0, 0.3),
        v2(0.22 * c, 1.3),
        v2(0.27 * c, 2.5),
        v2(0.05, 3.5),
        v2(0.15 * c, 2.5),
        v2(0.12 * c, 1.3),
    ];
    let e: Vec<Vec2> = if c > 0.0 { edge.to_vec() } else { edge.iter().rev().copied().collect() };
    s.extrude(&e, 0.16, HOT, place(hilt, rot));
}

// --- XXXG-01S Shenlong Gundam: the dragon arm, a beam glaive. ---

pub(crate) fn shenlong(d: &mut Designer) {
    inner_frame(d, 1.25);
    hands(d, FRAME);

    // Head: white, twin long yellow fins swept back, green eyes, green crest.
    helmet(d, v(1.5, 1.2, 1.7), v2(0.8, 0.8), TRIM);
    v_fin(d, 1.4, 1.6, 0.55);
    let mut h = d.on(Bone::Head);
    h.seed(0.02);
    h.cube(v(0.4, 0.45, 0.35), 0.05, TRIM, at(0.0, 7.15, 0.95));
    h.cube(v(0.3, 0.3, 1.25), 0.08, BODY, at(0.0, 7.45, -0.05));

    // Chest: white with green panels and a yellow collar.
    let mut c = d.on(Bone::Chest);
    c.seed(0.14);
    c.block(v(4.6, 2.6, 3.0), v2(1.05, 0.9), v2(0.0, -0.1), 0.22, BODY, at(0.0, 4.15, 0.05));
    c.cube(v(2.9, 1.2, 2.6), 0.15, TRIM, at(0.0, 2.55, 0.0));
    c.block(v(2.4, 0.55, 2.3), v2(0.9, 0.9), Vec2::ZERO, 0.1, ACCENT, at(0.0, 5.55, -0.1));
    for s in Side::BOTH {
        c.cube(v(1.1, 1.0, 0.25), 0.05, TRIM, sided(s, at(1.25, 4.3, 1.55)));
    }
    d.on(Bone::Torso).seed(0.16).block(
        v(2.5, 1.3, 2.1),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 1.45, 0.05),
    );
    waist(d, TRIM, BODY);

    // Shoulders: round caps; the right one bigger, where the dragon's neck begins.
    for s in Side::BOTH {
        let big = s.pick(1.0, 1.15);
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.26 + s.pick(0.0, 0.4));
        sh.block(
            v(2.0 * big, 1.9 * big, 2.5 * big),
            v2(0.8, 0.85),
            v2(0.1, 0.0),
            0.25,
            BODY,
            sided(s, at(4.05, 4.85, 0.0)),
        );
        sh.block(
            v(1.9 * big, 0.4, 2.3 * big),
            v2(0.85, 0.85),
            Vec2::ZERO,
            0.08,
            TRIM,
            sided(s, at(4.1, 5.95, 0.0)),
        );
    }

    // The left arm is a plain one; the right is the dragon's neck, green scales to the head.
    let (sh, el, wr) = (j(Bone::UpperArmL), j(Bone::ForearmL), j(Bone::HandL));
    d.on(Bone::UpperArmL).seed(0.28).block(
        v(1.3, 1.9, 1.3),
        v2(0.9, 0.9),
        Vec2::ZERO,
        0.12,
        BODY,
        along(el, sh),
    );
    let mut f = d.on(Bone::ForearmL);
    f.seed(0.34);
    f.block(v(1.6, 2.0, 1.7), v2(0.8, 0.85), Vec2::ZERO, 0.14, BODY, along(wr + (el - wr) * 0.1, el));
    f.block(
        v(1.7, 0.45, 1.8),
        Vec2::ONE,
        Vec2::ZERO,
        0.06,
        TRIM,
        along(wr + (el - wr) * 0.12, wr + (el - wr) * 0.36),
    );
    let (rs, re, rw) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
    d.on(Bone::UpperArmR).seed(0.29).block(
        v(1.4, 1.9, 1.4),
        v2(0.9, 0.9),
        Vec2::ZERO,
        0.12,
        TRIM,
        along(re, rs),
    );
    let mut n = d.on(Bone::ForearmR);
    n.seed(0.35);
    for k in 0..4 {
        let t0 = k as f32 / 4.0;
        let a = re + (rw - re) * t0;
        let b = re + (rw - re) * (t0 + 0.24);
        n.block(
            v(1.45 - 0.06 * k as f32, a.distance(b), 1.55),
            v2(0.9, 0.9),
            Vec2::ZERO,
            0.1,
            if k % 2 == 0 { TRIM } else { BODY },
            along(a, b),
        );
    }

    // The dragon's head on the right hand: skull and snout (the fang), the lower jaw on the weapon
    // bone, the flamethrower in its mouth.
    let mut hd = d.on(Bone::HandR);
    hd.seed(0.45);
    hd.block(v(1.3, 1.1, 1.8), v2(0.8, 0.7), v2(0.0, 0.1), 0.1, TRIM, at(rw.x, rw.y + 0.2, rw.z + 0.9));
    hd.block(v(0.9, 0.6, 1.6), v2(0.7, 0.8), v2(0.0, -0.1), 0.08, TRIM, at(rw.x, rw.y + 0.3, rw.z + 2.3));
    for s in [-1.0f32, 1.0] {
        hd.cube(v(0.18, 0.08, 0.3), 0.02, EYE, at(rw.x + 0.45 * s, rw.y + 0.55, rw.z + 1.4));
        // Horns swept back.
        hd.block(
            v(0.15, 1.1, 0.2),
            v2(0.3, 0.3),
            v2(0.0, -0.4),
            0.02,
            YELLOW,
            place(v(rw.x + 0.4 * s, rw.y + 0.8, rw.z + 0.6), rx(-1.1)),
        );
        // Fangs.
        hd.block(
            v(0.12, 0.35, 0.12),
            v2(0.2, 0.2),
            Vec2::ZERO,
            0.0,
            Paint::Fixed(paint::WHITE),
            place(v(rw.x + 0.3 * s, rw.y - 0.05, rw.z + 2.8), rx(std::f32::consts::PI)),
        );
    }
    let snout = v(rw.x, rw.y + 0.25, rw.z + 3.1);
    let mouth = v(rw.x, rw.y - 0.05, rw.z + 2.6);
    hd.cylinder(0.18, 0.5, 8, THROAT, place(mouth, rx(FRAC_PI_2)));
    d.sockets.fang = Some((Designer::local(Bone::HandR, snout), Vec3::Z));
    d.sockets.flame = Some((Designer::local(Bone::HandR, mouth + v(0.0, 0.0, 0.3)), Vec3::Z));
    let g = j(Bone::Weapon);
    let mut jaw = d.on(Bone::Weapon);
    jaw.seed(0.63);
    jaw.block(v(0.95, 0.4, 2.0), v2(0.7, 0.9), Vec2::ZERO, 0.06, TRIM, at(g.x, g.y - 0.45, g.z + 1.3));
    jaw.cube(v(0.7, 0.12, 1.6), 0.0, FRAME, at(g.x, g.y - 0.22, g.z + 1.4));
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y - 0.3, g.z + 2.4));

    // Legs, backpack.
    legs(d, v(1.65, 3.2, 1.8), v(2.0, 3.5, 2.3), TRIM, ACCENT);
    backpack(d, v(2.6, 2.5, 1.6), 0.55);

    // The beam glaive in the left hand: a short staff, the emitter at its tip.
    let staff_dir = v(0.0, 0.5, 0.87).normalize();
    let grip = v(wr.x - 0.1, wr.y - 0.55, wr.z + 0.2);
    let (s0, s1) = (grip - staff_dir * 0.6, grip + staff_dir * 2.6);
    let mut gl = d.on(Bone::HandL);
    gl.cylinder(0.16, s0.distance(s1), 8, GUN, along(s0, s1));
    gl.cylinder(0.26, 0.5, 8, ACCENT, along(s1 - staff_dir * 0.4, s1 + staff_dir * 0.1));
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, s1), staff_dir));
    d.sockets.saber = (Designer::local(Bone::HandL, s1), staff_dir);

    // Its shield, round, on the left forearm.
    shield(
        d,
        &[
            (0.0, 2.2),
            (1.4, 1.4),
            (1.6, 0.0),
            (1.2, -1.6),
            (0.0, -2.2),
            (-1.2, -1.6),
            (-1.6, 0.0),
            (-1.4, 1.4),
        ],
        TRIM,
        ACCENT,
    );
}

// --- Neo-Bird: Wing Zero folded into its flight form. ---

/// The nose leads along +z; the wings are the arms, the tail the legs, the dorsal thrusters the
/// backpack, the shield the nose cone, and the Twin Buster Rifles hang under the fuselage (see
/// the simulation's `BIRD` capsules).
pub(crate) fn neo_bird(d: &mut Designer) {
    // Fuselage: the chest and torso stretched along z.
    let mut t = d.on(Bone::Torso);
    t.seed(0.1);
    t.lathe(
        &[(0.0, -4.6), (1.3, -4.2), (1.7, -2.0), (1.8, 0.5), (1.5, 3.0), (1.1, 4.8), (0.0, 5.2)],
        16,
        BODY,
        place(Vec3::ZERO, rx(FRAC_PI_2)),
    );
    let mut c = d.on(Bone::Chest);
    c.seed(0.12);
    c.block(v(2.6, 1.0, 5.0), v2(0.7, 0.9), Vec2::ZERO, 0.15, TRIM, at(0.0, 1.3, 0.8));
    for s in Side::BOTH {
        c.cube(v(0.9, 0.6, 1.6), 0.06, YELLOW, sided(s, at(1.35, 0.5, 2.8)));
    }
    let mut w = d.on(Bone::Waist);
    w.seed(0.19);
    w.block(v(2.2, 0.9, 3.0), v2(0.8, 0.8), Vec2::ZERO, 0.1, TRIM, at(0.0, -1.3, -1.5));

    // Nose: the shield as its cone, the canopy and sensors behind, the nose guns.
    let mut h = d.on(Bone::Head);
    h.seed(0.05);
    let o: Vec<Vec2> = [(0.0, 4.2), (1.0, 3.0), (1.1, 0.4), (0.0, -1.2), (-1.1, 0.4), (-1.0, 3.0)]
        .iter()
        .map(|&(x, y)| Vec2::new(x, y))
        .collect();
    h.extrude(&o, 0.5, ACCENT, place(v(0.0, 0.6, 5.2), rx(FRAC_PI_2)));
    let spine: Vec<Vec2> = o.iter().map(|p| Vec2::new(p.x * 0.2, p.y * 0.92)).collect();
    h.extrude(&spine, 0.8, BODY, place(v(0.0, 0.7, 5.2), rx(FRAC_PI_2)));
    h.cube(v(1.0, 0.45, 1.4), 0.08, GLASS, at(0.0, 1.35, 4.9));
    for s in Side::BOTH {
        h.cube(v(0.3, 0.1, 0.08), 0.02, EYE, sided(s, at(0.35, 1.2, 5.65)));
        h.cylinder(0.14, 1.2, 8, GUN, sided(s, place(v(0.55, 0.8, 7.2), rx(FRAC_PI_2))));
    }

    // Wings: swept, on the arms, with the arm armour folded along their roots.
    for s in Side::BOTH {
        let wing = [v2(0.0, 1.6), v2(6.8, -1.2), v2(6.9, -2.6), v2(0.0, -2.2)];
        d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR)).seed(0.23 + s.pick(0.0, 0.4)).extrude(
            &wing,
            0.3,
            BODY,
            sided(s, place(v(1.3, 0.0, 0.0), rx(FRAC_PI_2))),
        );
        let (up, fo, hand) = s.pick(
            (Bone::UpperArmL, Bone::ForearmL, Bone::HandL),
            (Bone::UpperArmR, Bone::ForearmR, Bone::HandR),
        );
        d.on(up).block(
            v(1.2, 2.6, 0.9),
            v2(0.9, 0.9),
            Vec2::ZERO,
            0.1,
            TRIM,
            sided(s, along(v(2.2, 0.35, 0.8), v(4.6, 0.35, -0.2))),
        );
        d.on(fo).block(
            v(1.0, 2.4, 0.7),
            v2(0.8, 0.8),
            Vec2::ZERO,
            0.1,
            BODY,
            sided(s, along(v(4.6, 0.3, -0.2), v(6.8, 0.3, -1.0))),
        );
        d.on(hand).cube(v(0.8, 0.4, 1.2), 0.06, ACCENT, sided(s, at(7.4, 0.1, -1.6)));
    }

    // Tail: the legs folded back, the feet as fins.
    for s in Side::BOTH {
        let (th, sh, ft) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        d.on(th).seed(0.3 + s.pick(0.0, 0.2)).block(
            v(1.2, 2.8, 1.3),
            v2(0.9, 0.9),
            Vec2::ZERO,
            0.12,
            BODY,
            sided(s, along(v(0.75, -0.2, -3.8), v(0.75, 0.0, -6.4))),
        );
        d.on(sh).seed(0.36 + s.pick(0.0, 0.2)).block(
            v(1.3, 3.0, 1.4),
            v2(0.8, 0.8),
            Vec2::ZERO,
            0.15,
            BODY,
            sided(s, along(v(0.75, 0.1, -6.4), v(0.75, 0.3, -9.3))),
        );
        let fin = [v2(0.0, 0.0), v2(1.4, 0.0), v2(0.6, 1.8), v2(-0.3, 1.9)];
        d.on(ft).seed(0.42 + s.pick(0.0, 0.2)).extrude(
            &fin,
            0.2,
            TRIM,
            sided(s, place(v(0.8, 0.6, -10.4), ry(-FRAC_PI_2) * rz(0.35))),
        );
    }

    // Dorsal thrusters.
    let mut b = d.on(Bone::Backpack);
    b.seed(0.5);
    b.block(v(2.2, 1.2, 3.4), v2(0.8, 0.8), Vec2::ZERO, 0.15, BODY, at(0.0, 2.1, -2.4));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.65, 2.0, -4.2)), v(0.0, 0.0, -1.0), 0.5);
    }

    // The wing binders, spread back over the fuselage.
    for s in Side::BOTH {
        let bone = s.pick(Bone::WingL, Bone::WingR);
        let root = v(1.0, 2.4, -1.8);
        let dir = v(0.8, 0.12, -0.6).normalize();
        let tip = root + dir * 8.0;
        let mut wg = d.on(bone);
        wg.seed(0.55 + s.pick(0.0, 0.2));
        wg.block(v(0.8, 8.0, 0.45), v2(0.6, 0.7), Vec2::ZERO, 0.1, BODY, sided(s, along(root, tip)));
        let feather: Vec<Vec2> =
            [(0.0, 0.0), (0.42, 0.35), (0.34, 4.0), (0.0, 4.6), (-0.28, 3.9), (-0.38, 0.35)]
                .iter()
                .map(|&(x, y)| Vec2::new(x, y))
                .collect();
        let n = if d.near() { 4 } else { 2 };
        for k in 0..n {
            let at_ = root + dir * (8.0 * (0.3 + 0.65 * k as f32 / (n - 1) as f32));
            let lay = Quat::from_rotation_arc(Vec3::Y, v(0.2, 0.0, -1.0).normalize()) * rz(0.2 * k as f32);
            d.on(bone).seed(0.6 + k as f32 * 0.07).extrude(&feather, 0.16, BODY, sided(s, place(at_, lay)));
        }
    }

    // The Twin Buster Rifles, joined under the fuselage.
    let mut g = d.on(Bone::Weapon);
    g.seed(0.61);
    for s in Side::BOTH {
        g.cylinder(0.42, 9.0, 12, GUN, sided(s, place(v(0.55, -2.0, 3.5), rx(FRAC_PI_2))));
        g.cylinder(0.6, 0.35, 12, YELLOW, sided(s, place(v(0.55, -2.0, 5.5), rx(FRAC_PI_2))));
        g.cylinder(0.28, 1.4, 10, TRIM, sided(s, place(v(0.55, -2.0, 8.6), rx(FRAC_PI_2))));
    }
    g.cube(v(1.8, 0.8, 3.0), 0.1, TRIM, at(0.0, -1.7, 0.5));
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(0.0, -2.0, 9.3));
    // Nothing is swung in this form; the saber's rest direction still has to be a direction.
    d.sockets.saber = (Vec3::ZERO, Vec3::Z);
}
