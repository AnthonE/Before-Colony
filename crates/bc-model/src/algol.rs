//! PX-02 Algol: ribbed sail binders, a beam scythe, the claw shield.

use std::f32::consts::FRAC_PI_2;

use glam::Vec2;

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, FRAME, GUN, THROAT, TRIM, along, arm_segments, at, hands, inner_frame, j, place, rx, ry,
    rz, shield, sided, v, v2,
};
use crate::lodestars::{backpack, helmet, legs, v_fin, waist};
use crate::rig::{Bone, Side};

pub(crate) fn algol(d: &mut Designer) {
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
    c.cube(v(2.8, 0.9, 2.5), 0.15, TRIM, at(0.0, 2.85, 0.0));
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
        at(0.0, 2.35, 0.05),
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

    // Sail binders: ribs fanned from the shoulders, with the sail between.
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

    // The claw shield: a long pointed shield whose tip opens on the beam.
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
