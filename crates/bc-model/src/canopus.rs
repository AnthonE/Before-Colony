//! PX-03 Canopus: heat blades, a beam machine gun, the Pincer Strike.

use std::f32::consts::FRAC_PI_2;

use glam::{Vec2, Vec3};

use crate::Designer;
use crate::detail::toward;
use crate::frames::{
    ACCENT, BODY, FRAME, GUN, STEEL, TRIM, YELLOW, arm_segments, at, hands, inner_frame, j, place, rifle, ry,
    rz, shield, sided, sp, v, v2,
};
use crate::kit::Paint;
use crate::lodestars::{backpack, helmet, legs, missile_pod, waist};
use crate::paint;
use crate::rig::{Bone, Side};

/// A heated blade's edge.
const HOT: Paint = Paint::Glow(paint::RED);

pub(crate) fn canopus(d: &mut Designer) {
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
    c.cube(v(3.1, 0.9, 2.8), 0.15, TRIM, at(0.0, 2.85, 0.0));
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
        at(0.0, 2.35, 0.05),
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

    // The beam machine gun in the right hand, the right blade slung under it; the left blade in
    // the left hand. Both blades glow at the edge.
    rifle(d, 5.0, 0.3, GUN, TRIM);
    let g = j(Bone::Weapon);
    let dir_r = v(0.15, -0.35, 0.92).normalize();
    let hilt_r = g + v(0.0, -0.6, 0.2);
    blade(d, Bone::Weapon, hilt_r, dir_r, 1.0);
    d.sockets.blade_right = Some((Designer::local(Bone::Weapon, hilt_r), dir_r));
    let wr = j(Bone::HandL);
    let hilt_l = v(wr.x - 0.1, wr.y - 0.5, wr.z + 0.3);
    let dir_l = v(-0.05, 0.6, 0.8).normalize();
    blade(d, Bone::HandL, hilt_l, dir_l, -1.0);
    d.sockets.blade_left = Some((Designer::local(Bone::HandL, hilt_l), dir_l));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt_l), dir_l);

    shield(d, &[(0.0, 2.8), (1.3, 2.2), (1.3, -1.6), (0.0, -2.8), (-1.3, -1.6), (-1.3, 2.2)], TRIM, BODY);
}

/// A heat blade on `bone`: a grip at `hilt`, a crescent blade out along `dir` curving to side
/// `curl` (+1 right, -1 left), its inner edge glowing.
fn blade(d: &mut Designer, bone: Bone, hilt: Vec3, dir: Vec3, curl: f32) {
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
