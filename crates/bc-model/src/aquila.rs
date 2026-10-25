//! Aquila: Altair folded into its flight form.

use std::f32::consts::FRAC_PI_2;

use glam::{Quat, Vec2, Vec3};

use crate::Designer;
use crate::frames::{
    ACCENT, BODY, EYE, GLASS, GUN, TRIM, YELLOW, along, at, nozzle, place, rx, ry, rz, sided, sp, v, v2,
};
use crate::rig::{Bone, Side};

/// The nose leads along +z; the wings are the arms, the tail the legs, the dorsal thrusters the
/// backpack, the shield the nose cone, and the Heavy Beam Rifles hang under the fuselage (see
/// the simulation's `BIRD` capsules).
pub(crate) fn aquila(d: &mut Designer) {
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
    // The cockpit view looks out from over the canopy, along the nose.
    d.sockets.eye = Designer::local(Bone::Head, v(0.0, 2.4, 4.6));
    d.sockets.eye_in_head = false;

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

    // The Heavy Beam Rifles, joined under the fuselage.
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
