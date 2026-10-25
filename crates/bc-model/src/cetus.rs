//! AL-09D Cetus: bulky, dark, a beam cannon and shield pods.

use glam::{Quat, Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    BODY, EYE, FRAME, FootStyle, GLASS, GUN, THROAT, TRIM, WAIST_UP, along, arm_segments, at, foot, hands,
    inner_frame, j, leg_lengths, nozzle, place, plate, rifle, rx, rz, saber_hilt, sided, sp, v, v2,
};
use crate::rig::{Bone, Side};

pub(crate) fn cetus(d: &mut Designer) {
    inner_frame(d, 1.5);
    hands(d, FRAME);

    let mut h = d.on(Bone::Head);
    h.seed(0.09);
    h.block(v(1.6, 1.0, 1.6), v2(0.8, 0.8), Vec2::ZERO, 0.12, BODY, at(0.0, 6.5, 0.2));
    h.cube(v(1.4, 0.3, 0.3), 0.04, GLASS, at(0.0, 6.55, 0.95));
    h.sphere(0.15, 5, EYE, at(-0.3, 6.55, 1.1));

    let mut c = d.on(Bone::Chest);
    c.seed(0.16);
    c.block(v(5.3, 3.0, 3.5), v2(0.95, 0.85), v2(0.0, -0.15), 0.3, BODY, at(0.0, 4.0, 0.05));
    c.block(v(3.2, 0.9, 0.4), v2(0.8, 1.0), Vec2::ZERO, 0.08, TRIM, at(0.0, 4.7, 1.85));
    c.greeble(|c| {
        for k in 0..4 {
            c.cube(v(2.6, 0.1, 0.12), 0.0, FRAME, at(0.0, 3.2 + k as f32 * 0.22, 1.82));
        }
    });
    d.on(Bone::Torso).seed(0.19).block(
        v(3.2, 1.5, 2.6),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.15,
        TRIM,
        at(0.0, 2.35, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.22);
    w.cube(v(3.8, 0.55, 2.8), 0.1, FRAME, at(0.0, 0.25 + WAIST_UP, 0.0));
    w.block(v(1.3, 1.4, 1.9), v2(1.4, 1.1), Vec2::ZERO, 0.12, BODY, at(0.0, -0.55 + WAIST_UP, 0.2));
    for s in Side::BOTH {
        let (front, side) = (v(1.15, -0.85 + WAIST_UP, 1.3), v(2.45, -0.6 + WAIST_UP, 0.0));
        plate(d, Bone::Waist, s, v(1.7, 1.9, 0.4), front, rz(0.12) * rx(0.12), 0.14, BODY);
        plate(d, Bone::Waist, s, v(0.4, 1.9, 2.1), side, rz(0.14), 0.12, BODY);
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.27 + s.pick(0.0, 0.4));
        sh.block(v(2.6, 2.3, 3.1), v2(0.9, 0.85), v2(0.1, 0.0), 0.3, BODY, sided(s, at(4.3, 4.9, 0.0)));
        sh.block(v(2.7, 0.4, 3.2), v2(1.0, 1.0), Vec2::ZERO, 0.06, TRIM, sided(s, at(4.3, 4.2, 0.0)));
    }
    arm_segments(d, v(1.6, 1.9, 1.6), v(1.9, 2.1, 1.95), BODY, TRIM);
    let (thigh_len, shin_len) = leg_lengths();
    for s in Side::BOTH {
        let (thigh, shin) = s.pick((Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.33 + s.pick(0.0, 0.2)).block(
            v(1.95, thigh_len, 2.1),
            v2(1.05, 1.05),
            Vec2::ZERO,
            0.22,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.1), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.39 + s.pick(0.0, 0.2));
        sh.block(
            v(2.3, shin_len, 2.6),
            v2(0.8, 0.8),
            Vec2::ZERO,
            0.3,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.05), knee + v(0.0, -0.3, 0.0))),
        );
        sh.block(
            v(1.4, 1.2, 0.6),
            v2(0.8, 0.8),
            Vec2::ZERO,
            0.12,
            TRIM,
            sided(s, place(knee + v(0.0, 0.0, 1.05), rx(-0.25))),
        );
    }
    foot(d, FootStyle { armour: BODY, toe: TRIM, pads: TRIM, half: 1.0, spur: false });
    let mut b = d.on(Bone::Backpack);
    b.seed(0.53);
    b.block(v(3.2, 2.8, 1.8), v2(0.9, 0.9), Vec2::ZERO, 0.25, BODY, at(0.0, 4.0, -2.7));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.9, 3.1, -3.6)), v(0.0, -0.3, -1.0), 0.65);
    }

    // The beam cannon, carried under the right arm.
    rifle(d, 9.0, 0.5, GUN, BODY);
    let g = j(Bone::Weapon);
    d.on(Bone::Weapon).cylinder(
        0.95,
        2.2,
        14,
        TRIM,
        place(v(g.x, g.y + 0.45, g.z + 1.6), rx(std::f32::consts::FRAC_PI_2)),
    );
    saber_hilt(d);

    // shield pods: four discs riding round the suit.
    for k in 0..4 {
        let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
        let pos = v(6.0 * a.cos(), 2.5 + 2.2 * a.sin(), -1.0);
        let face = place(pos, Quat::from_rotation_arc(Vec3::Y, v(a.cos(), a.sin() * 0.3, 0.6).normalize()));
        let mut p = d.on(Bone::Props);
        p.seed(0.8 + k as f32 * 0.05);
        p.lathe(&[(0.0, -0.25), (1.2, -0.2), (1.4, 0.0), (1.2, 0.2), (0.0, 0.25)], 16, TRIM, face);
        p.lathe(&[(0.45, 0.26), (0.25, 0.3), (0.0, 0.3)], 12, THROAT, face);
    }
}
