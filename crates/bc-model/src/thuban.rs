//! PX-04 Thuban: the Talon arm, a beam glaive.

use std::f32::consts::FRAC_PI_2;

use glam::{Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, GUN, THROAT, TRIM, YELLOW, along, at, hands, inner_frame, j, place, rx, shield,
    sided, v, v2,
};
use crate::kit::Paint;
use crate::lodestars::{backpack, helmet, legs, v_fin, waist};
use crate::paint;
use crate::rig::{Bone, Side};

pub(crate) fn thuban(d: &mut Designer) {
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
    c.cube(v(2.9, 0.9, 2.6), 0.15, TRIM, at(0.0, 2.85, 0.0));
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
        at(0.0, 2.35, 0.05),
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
    d.sockets.talon = Some((Designer::local(Bone::HandR, snout), Vec3::Z));
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
