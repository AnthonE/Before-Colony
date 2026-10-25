//! PX-00 Altair.

use glam::{Quat, Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, FootStyle, GLASS, GUN, TRIM, WAIST_UP, YELLOW, along, arm_segments, at, foot,
    hands, inner_frame, j, leg_lengths, nozzle, place, plate, rifle, rx, rz, saber_hilt, shield, sided, sp,
    v, v2,
};
use crate::rig::{Bone, Side};

pub(crate) fn altair(d: &mut Designer) {
    inner_frame(d, 1.25);
    hands(d, FRAME);

    // Head: white helmet, yellow V-fin, green eyes, red chin, head guns at the temples.
    let mut h = d.on(Bone::Head);
    h.seed(0.05);
    h.block(v(1.5, 1.25, 1.7), v2(0.85, 0.8), v2(0.0, -0.05), 0.12, BODY, at(0.0, 6.75, 0.1));
    h.cube(v(0.95, 0.65, 0.3), 0.05, GLASS, at(0.0, 6.6, 0.92));
    h.cube(v(0.32, 0.26, 0.3), 0.05, ACCENT, at(0.0, 6.2, 0.95));
    h.cube(v(0.34, 0.4, 0.26), 0.05, ACCENT, at(0.0, 7.12, 0.98));
    h.cube(v(0.32, 0.3, 1.3), 0.08, BODY, at(0.0, 7.45, -0.05));
    for s in Side::BOTH {
        h.cube(v(0.3, 0.1, 0.08), 0.02, EYE, sided(s, place(v(0.23, 6.74, 1.08), rz(-0.12))));
        h.cube(v(0.35, 0.75, 1.1), 0.06, BODY, sided(s, at(0.8, 6.7, 0.1)));
        h.cylinder(0.14, 0.5, 8, GUN, sided(s, place(v(0.95, 6.85, 0.45), rx(std::f32::consts::FRAC_PI_2))));
        // The V-fin: a swept blade from the forehead.
        let fin =
            [v(0.0, 0.0, 0.0), v(0.3, 0.12, 0.0), v(1.75, 1.3, 0.0), v(1.6, 1.36, 0.0), v(0.05, 0.3, 0.0)]
                .map(|p| p.truncate());
        h.extrude(&fin, 0.1, YELLOW, sided(s, place(v(0.02, 7.05, 1.0), rx(-0.25))));
    }

    // Chest: blue, with yellow intakes either side of a white centre.
    let mut c = d.on(Bone::Chest);
    c.seed(0.13);
    c.block(v(4.7, 2.5, 3.0), v2(1.05, 0.9), v2(0.0, -0.1), 0.22, TRIM, at(0.0, 4.15, 0.05));
    c.block(v(2.5, 0.55, 2.3), v2(0.9, 0.9), Vec2::ZERO, 0.1, BODY, at(0.0, 5.55, -0.1));
    c.block(v(1.3, 1.7, 0.4), v2(0.7, 1.0), Vec2::ZERO, 0.08, BODY, at(0.0, 4.0, 1.62));
    c.cube(v(0.8, 0.5, 0.3), 0.06, ACCENT, at(0.0, 2.95, 1.5));
    c.cube(v(2.9, 0.9, 2.6), 0.15, BODY, at(0.0, 2.85, 0.0));
    for s in Side::BOTH {
        c.cube(v(1.05, 0.85, 0.25), 0.05, YELLOW, sided(s, at(1.35, 4.35, 1.58)));
        c.greeble(|c| {
            for k in 0..4 {
                c.cube(v(0.98, 0.07, 0.1), 0.0, FRAME, sided(s, at(1.35, 4.05 + k as f32 * 0.2, 1.72)));
            }
        });
        // Shoulder machine cannons, above the collarbones.
        c.cylinder(0.2, 1.2, 8, GUN, sided(s, place(v(2.0, 5.4, 0.8), rx(std::f32::consts::FRAC_PI_2))));
    }

    // Abdomen and waist.
    d.on(Bone::Torso).seed(0.17).block(
        v(2.6, 0.7, 2.1),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 2.35, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.19);
    w.cube(v(3.3, 0.45, 2.5), 0.08, FRAME, at(0.0, 0.25 + WAIST_UP, 0.0));
    w.block(v(1.0, 1.3, 1.6), v2(1.5, 1.1), Vec2::ZERO, 0.1, BODY, at(0.0, -0.55 + WAIST_UP, 0.25));
    w.cube(v(0.7, 0.45, 0.4), 0.05, ACCENT, at(0.0, -0.2 + WAIST_UP, 1.1));
    for s in Side::BOTH {
        let (front, side) = (v(1.05, -0.8 + WAIST_UP, 1.25), v(2.3, -0.55 + WAIST_UP, 0.0));
        plate(d, Bone::Waist, s, v(1.3, 1.9, 0.3), front, rz(0.1) * rx(0.12), 0.08, TRIM);
        plate(d, Bone::Waist, s, v(0.3, 1.8, 1.9), side, rz(0.12), 0.08, BODY);
    }
    plate(d, Bone::Waist, Side::R, v(2.4, 1.4, 0.3), v(0.0, -0.4 + WAIST_UP, -1.35), rx(-0.12), 0.08, BODY);

    // Shoulders: big blue blocks under white caps.
    for s in Side::BOTH {
        let bone = s.pick(Bone::ShoulderL, Bone::ShoulderR);
        let mut sh = d.on(bone);
        sh.seed(0.23 + s.pick(0.0, 0.4));
        sh.block(v(2.1, 2.0, 2.7), v2(0.85, 0.9), v2(0.15, 0.0), 0.2, TRIM, sided(s, at(4.1, 4.8, 0.0)));
        sh.block(v(2.0, 0.5, 2.4), v2(0.8, 0.85), Vec2::ZERO, 0.12, BODY, sided(s, at(4.2, 5.95, 0.0)));
        sh.greeble(|sh| {
            sh.cube(v(0.2, 1.0, 1.6), 0.04, YELLOW, sided(s, at(5.2, 4.8, 0.0)));
        });
    }

    // Arms: white, blue cuffs.
    arm_segments(d, v(1.35, 1.9, 1.35), v(1.6, 2.05, 1.7), BODY, TRIM);

    // Legs: white thighs, big shins with blue knees and yellow vents, red-toed feet.
    let (thigh_len, shin_len) = leg_lengths();
    for s in Side::BOTH {
        let (thigh, shin) = s.pick((Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        let mid = (knee + ankle) * 0.5;
        d.on(thigh).seed(0.3 + s.pick(0.0, 0.2)).block(
            v(1.65, thigh_len, 1.8),
            v2(1.05, 1.1),
            Vec2::ZERO,
            0.15,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.15), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.36 + s.pick(0.0, 0.2));
        sh.block(
            v(2.0, shin_len, 2.3),
            v2(0.85, 0.9),
            v2(0.0, -0.1),
            0.2,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.0), knee + v(0.0, -0.3, 0.0))),
        );
        sh.block(
            v(1.2, 1.2, 0.55),
            v2(0.8, 0.7),
            Vec2::ZERO,
            0.1,
            TRIM,
            sided(s, place(knee + v(0.0, 0.1, 0.95), rx(-0.2))),
        );
        sh.cube(v(0.25, 1.4, 1.0), 0.04, YELLOW, sided(s, at(mid.x + 0.95, mid.y - 1.0, -0.2)));
        sh.greeble(|sh| {
            sh.block(
                v(1.3, 1.6, 0.4),
                v2(0.8, 1.0),
                Vec2::ZERO,
                0.06,
                TRIM,
                sided(s, at(mid.x, mid.y - 0.7, -1.25)),
            );
        });
    }
    foot(d, FootStyle { armour: BODY, toe: ACCENT, pads: ACCENT, half: 0.9, spur: true });

    // Backpack, main thrusters, and the wing binders.
    let mut b = d.on(Bone::Backpack);
    b.seed(0.5);
    b.block(v(2.6, 2.5, 1.6), v2(0.9, 0.9), v2(0.0, 0.1), 0.2, BODY, at(0.0, 4.0, -2.6));
    b.cube(v(1.6, 1.4, 0.5), 0.08, TRIM, at(0.0, 4.2, -3.45));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.7, 3.2, -3.5)), v(0.0, -0.3, -1.0), 0.55);
    }
    for s in Side::BOTH {
        let bone = s.pick(Bone::WingL, Bone::WingR);
        let root = j(Bone::WingR);
        let dir = v(0.55, 0.8, -0.25).normalize();
        let tip = root + dir * 7.5;
        let mut wg = d.on(bone);
        wg.seed(0.55 + s.pick(0.0, 0.2));
        wg.block(v(0.9, 7.6, 0.55), v2(0.6, 0.7), Vec2::ZERO, 0.12, BODY, sided(s, along(root, tip)));
        wg.cube(
            v(0.7, 1.4, 0.8),
            0.08,
            TRIM,
            sided(s, place(root + dir * 0.6, Quat::from_rotation_arc(Vec3::Y, dir))),
        );
        // Feathers fanning down and out from the spar.
        let feather: Vec<Vec2> =
            [(0.0, 0.0), (0.42, 0.35), (0.34, 4.3), (0.0, 5.0), (-0.28, 4.2), (-0.38, 0.35)]
                .iter()
                .map(|&(x, y)| Vec2::new(x, y))
                .collect();
        let n = if d.near() { 5 } else { 3 };
        for k in 0..n {
            let t = 0.35 + 0.6 * k as f32 / (n - 1) as f32;
            let pos = root + dir * (7.5 * t);
            let droop = Quat::from_rotation_z(-2.3 + 0.35 * k as f32) * Quat::from_rotation_y(0.15);
            d.on(bone).seed(0.6 + k as f32 * 0.07).extrude(
                &feather,
                0.18,
                BODY,
                sided(s, place(pos + v(0.0, 0.0, -0.1), droop)),
            );
            d.on(bone).extrude(&feather[..4], 0.12, TRIM, sided(s, place(pos + v(0.0, 0.0, -0.3), droop)));
        }
        // The smaller lower binder.
        let low = root + v(-0.2, -1.2, -0.2);
        d.on(bone).block(
            v(0.7, 3.6, 0.45),
            v2(0.5, 0.8),
            Vec2::ZERO,
            0.08,
            BODY,
            sided(s, along(low, low + v(0.9, -3.2, -0.6))),
        );
    }

    // Heavy Beam Rifle: long, blue and grey, yellow bands.
    rifle(d, 10.5, 0.36, GUN, TRIM);
    let g = j(Bone::Weapon);
    d.on(Bone::Weapon).greeble(|w| {
        for z in [3.5, 6.0] {
            w.cylinder(
                0.62,
                0.3,
                12,
                YELLOW,
                place(v(g.x, g.y + 0.45, g.z + z), rx(std::f32::consts::FRAC_PI_2)),
            );
        }
    });

    // Shield on the left forearm: red, with a white spine.
    shield(d, &[(0.0, 3.2), (1.1, 2.6), (1.2, -1.0), (0.0, -3.4), (-1.2, -1.0), (-1.1, 2.6)], ACCENT, BODY);
    saber_hilt(d);
}
