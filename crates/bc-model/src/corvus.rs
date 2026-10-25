//! AL-11D Corvus: slim, angular, a nose-cone backpack.

use glam::Vec2;

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, FootStyle, GLASS, TRIM, WAIST_UP, along, arm_segments, at, foot, hands,
    inner_frame, j, leg_lengths, nozzle, place, plate, rifle, rx, ry, rz, saber_hilt, sided, sp, v, v2,
};
use crate::rig::{Bone, Side};

pub(crate) fn corvus(d: &mut Designer) {
    inner_frame(d, 1.15);
    hands(d, FRAME);

    let mut h = d.on(Bone::Head);
    h.seed(0.08);
    h.block(v(1.3, 1.1, 1.9), v2(0.6, 0.5), v2(0.0, -0.2), 0.1, BODY, at(0.0, 6.6, 0.15));
    h.cube(v(1.1, 0.25, 0.4), 0.04, GLASS, at(0.0, 6.7, 0.95));
    h.sphere(0.14, 5, EYE, at(0.0, 6.7, 1.12));
    h.block(v(0.2, 1.1, 1.8), v2(1.0, 0.4), v2(0.0, -0.5), 0.04, TRIM, at(0.0, 7.4, -0.1));

    let mut c = d.on(Bone::Chest);
    c.seed(0.15);
    c.block(v(4.0, 2.6, 2.8), v2(1.15, 0.85), v2(0.0, -0.15), 0.18, BODY, at(0.0, 4.1, 0.05));
    c.block(v(2.4, 1.2, 0.4), v2(0.6, 1.0), Vec2::ZERO, 0.06, TRIM, at(0.0, 4.3, 1.45));
    c.cube(v(2.0, 0.45, 2.0), 0.08, TRIM, at(0.0, 5.5, -0.1));
    d.on(Bone::Torso).seed(0.18).block(
        v(2.2, 1.4, 1.9),
        v2(1.3, 1.1),
        Vec2::ZERO,
        0.1,
        BODY,
        at(0.0, 2.35, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.21);
    w.cube(v(3.0, 0.4, 2.2), 0.06, FRAME, at(0.0, 0.25 + WAIST_UP, 0.0));
    w.block(v(0.9, 1.2, 1.5), v2(1.5, 1.1), Vec2::ZERO, 0.08, TRIM, at(0.0, -0.5 + WAIST_UP, 0.2));
    for s in Side::BOTH {
        let front = v(1.0, -0.75 + WAIST_UP, 1.1);
        plate(d, Bone::Waist, s, v(1.2, 1.6, 0.25), front, rz(0.14) * rx(0.2), 0.06, BODY);
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.26 + s.pick(0.0, 0.4));
        sh.block(v(1.7, 1.2, 2.4), v2(0.6, 0.7), v2(0.3, -0.2), 0.12, BODY, sided(s, at(4.0, 4.9, 0.0)));
        sh.block(
            v(0.25, 1.5, 2.0),
            v2(1.0, 0.5),
            v2(0.0, -0.5),
            0.04,
            TRIM,
            sided(s, place(v(4.9, 5.1, -0.2), rz(-0.3))),
        );
    }
    arm_segments(d, v(1.1, 1.8, 1.1), v(1.3, 1.9, 1.4), BODY, TRIM);
    let (thigh_len, shin_len) = leg_lengths();
    for s in Side::BOTH {
        let (thigh, shin) = s.pick((Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.32 + s.pick(0.0, 0.2)).block(
            v(1.4, thigh_len, 1.6),
            v2(1.1, 1.1),
            Vec2::ZERO,
            0.12,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.1), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.38 + s.pick(0.0, 0.2));
        sh.block(
            v(1.6, shin_len, 1.9),
            v2(0.85, 1.1),
            v2(0.0, 0.15),
            0.15,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.0), knee + v(0.0, -0.3, 0.0))),
        );
        // A swept fin down the back of the calf.
        let fin = [Vec2::new(0.0, 0.0), Vec2::new(0.4, 3.0), Vec2::new(-0.3, 3.2), Vec2::new(-1.2, 0.6)];
        let calf = ankle + v(0.0, 0.6, -1.05);
        sh.extrude(&fin, 0.18, TRIM, sided(s, place(calf, ry(std::f32::consts::FRAC_PI_2))));
    }
    foot(d, FootStyle { armour: BODY, toe: TRIM, pads: ACCENT, half: 0.8, spur: true });

    // The nose-cone backpack, pointing back and up (its fighter mode's nose), with fins.
    let mut b = d.on(Bone::Backpack);
    b.seed(0.52);
    b.block(v(2.2, 2.2, 1.4), v2(0.9, 0.9), Vec2::ZERO, 0.15, BODY, at(0.0, 4.1, -2.5));
    b.lathe(
        &[(0.0, 0.0), (1.0, 0.0), (0.95, 1.2), (0.5, 3.0), (0.0, 3.8)],
        14,
        TRIM,
        place(v(0.0, 4.9, -3.1), rx(-1.2)),
    );
    for s in Side::BOTH {
        let wing = [Vec2::new(0.0, 0.0), Vec2::new(2.6, -0.6), Vec2::new(2.7, -1.1), Vec2::new(0.0, -1.6)];
        b.extrude(&wing, 0.15, BODY, sided(s, place(v(0.9, 4.4, -3.1), rx(-0.3))));
    }
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.6, 3.2, -3.3)), v(0.0, -0.4, -1.0), 0.45);
    }
    rifle(d, 7.0, 0.24, BODY, TRIM);
    saber_hilt(d);
}
