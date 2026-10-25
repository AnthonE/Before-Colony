//! PX-01 Antares: a walking arsenal.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use crate::Designer;
use crate::detail::toward;
use crate::frames::{
    BODY, FRAME, GUN, STEEL, TRIM, arm_segments, at, hands, inner_frame, j, place, rx, ry, sided, sp, v, v2,
};
use crate::lodestars::{backpack, helmet, legs, missile_pod, v_fin, waist};
use crate::rig::{Bone, Side};

pub(crate) fn antares(d: &mut Designer) {
    inner_frame(d, 1.3);
    hands(d, FRAME);

    // Head: white, a short yellow V-fin over a red crest, a red chin.
    helmet(d, v(1.55, 1.2, 1.7), v2(0.8, 0.8), TRIM);
    v_fin(d, 1.15, 0.8, 0.2);
    let mut h = d.on(Bone::Head);
    h.seed(0.06);
    h.block(v(0.5, 0.45, 0.5), v2(0.5, 0.6), v2(0.0, -0.1), 0.05, TRIM, at(0.0, 7.2, 0.75));
    h.cube(v(0.32, 0.3, 1.2), 0.08, BODY, at(0.0, 7.4, -0.1));

    // Chest: broad, red below, with the hatches over the gatlings Full Salvo fires.
    let mut c = d.on(Bone::Chest);
    c.seed(0.12);
    c.block(v(4.9, 2.7, 3.2), v2(1.0, 0.9), v2(0.0, -0.1), 0.22, BODY, at(0.0, 4.1, 0.05));
    c.cube(v(3.0, 0.9, 2.7), 0.15, TRIM, at(0.0, 2.85, 0.0));
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
        at(0.0, 2.35, 0.05),
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
        missile_pod(d, thigh, sp(s, v(2.55, -0.3, 0.3)), v(0.7, 1.6, 1.5), Vec3::Z, 3, 2);
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

    // The combat knife in the left hand, blade forward.
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
