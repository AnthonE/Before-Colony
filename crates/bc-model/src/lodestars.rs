//! What the five Lodestars share, on the same skeleton and helpers as `frames`: the helmet, the
//! V-fin, the waist, the legs and the backpack. Each Lodestar is its own module
//! (`antares`, `algol`, `canopus`, `thuban`, and the Aquila, Altair's flight form), carrying its
//! own kit and the sockets the client draws that kit from: blades, the Talon and its flamethrower,
//! missile hatches.
//!
//! Blades held in a hand stay short (the hand's armour must stay near its hit capsule); long
//! weapons ride the `Weapon` bone.

use glam::{Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, FRAME, FootStyle, GLASS, TRIM, WAIST_UP, YELLOW, along, at, foot, j, leg_lengths,
    nozzle, place, plate, rx, rz, sided, sp, v, v2,
};
use crate::kit::Paint;
use crate::rig::{Bone, Side};

/// A head: the helmet block, visor, chin, eye slits and cheek guards.
pub(crate) fn helmet(d: &mut Designer, size: Vec3, top: Vec2, chin: Paint) {
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
pub(crate) fn v_fin(d: &mut Designer, span: f32, rise: f32, sweep: f32) {
    for s in Side::BOTH {
        let fin = [v2(0.0, 0.0), v2(0.3, 0.12), v2(span, rise), v2(span - 0.15, rise + 0.07), v2(0.05, 0.3)];
        d.on(Bone::Head).extrude(&fin, 0.1, YELLOW, sided(s, place(v(0.02, 7.05, 1.0), rx(-sweep))));
    }
}

/// Waist block and skirt plates.
pub(crate) fn waist(d: &mut Designer, front: Paint, side: Paint) {
    let mut w = d.on(Bone::Waist);
    w.seed(0.19);
    w.cube(v(3.3, 0.45, 2.5), 0.08, FRAME, at(0.0, 0.25 + WAIST_UP, 0.0));
    w.block(v(1.0, 1.3, 1.6), v2(1.5, 1.1), Vec2::ZERO, 0.1, BODY, at(0.0, -0.55 + WAIST_UP, 0.25));
    w.cube(v(0.7, 0.45, 0.4), 0.05, ACCENT, at(0.0, -0.2 + WAIST_UP, 1.1));
    for s in Side::BOTH {
        let (fore, flank) = (v(1.05, -0.8 + WAIST_UP, 1.25), v(2.3, -0.55 + WAIST_UP, 0.0));
        plate(d, Bone::Waist, s, v(1.3, 1.9, 0.3), fore, rz(0.1) * rx(0.12), 0.08, front);
        plate(d, Bone::Waist, s, v(0.3, 1.8, 1.9), flank, rz(0.12), 0.08, side);
    }
    plate(d, Bone::Waist, Side::R, v(2.4, 1.4, 0.3), v(0.0, -0.4 + WAIST_UP, -1.35), rx(-0.12), 0.08, BODY);
}

/// Thighs, shins and feet: `thigh` and `shin` sizes, knee guards in `trim`, the feet the
/// Lodestars' ([`foot`]) with their toe caps in `toe`.
pub(crate) fn legs(d: &mut Designer, thigh: Vec3, shin: Vec3, trim: Paint, toe: Paint) {
    // The blocks' widths and depths are the frame's; their lengths are the rig's.
    let (thigh_len, shin_len) = leg_lengths();
    let (thigh, shin) = (v(thigh.x, thigh_len, thigh.z), v(shin.x, shin_len, shin.z));
    for s in Side::BOTH {
        let (th, sh) = s.pick((Bone::ThighL, Bone::ShinL), (Bone::ThighR, Bone::ShinR));
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
    }
    foot(d, FootStyle { armour: BODY, toe, pads: ACCENT, half: 0.92, spur: true });
}

/// A flat backpack with a pair of main thrusters.
pub(crate) fn backpack(d: &mut Designer, size: Vec3, bell: f32) {
    let mut b = d.on(Bone::Backpack);
    b.seed(0.5);
    b.block(size, v2(0.9, 0.9), v2(0.0, 0.1), 0.2, BODY, at(0.0, 4.0, -2.6));
    b.cube(v(size.x * 0.6, size.y * 0.55, 0.5), 0.08, TRIM, at(0.0, 4.2, -2.6 - size.z * 0.55));
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.7, 3.2, -3.5)), v(0.0, -0.3, -1.0), bell);
    }
}
