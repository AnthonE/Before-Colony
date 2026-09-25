//! The four frames, built from the kit on the shared skeleton. Each is described for the suit's
//! right side and mirrored for the left; coordinates are the suit's frame at rest, in metres (x
//! right, y up, z forward, origin at the torso; about 17 m from sole to crown).
//!
//! Paint slots: `Body`, `Trim` and `Accent` take the livery's colours, `Eye` its sensor glow; the
//! inner frame, weapons and details use fixed paints.

use bc_proto::FrameId;
use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::Designer;
use crate::kit::{Paint, mirrored};
use crate::paint;
use crate::rig::{Bone, Side};

const BODY: Paint = Paint::Body;
const TRIM: Paint = Paint::Trim;
const ACCENT: Paint = Paint::Accent;
const EYE: Paint = Paint::Eye;
/// The inner frame: joints, abdomen, hands.
const FRAME: Paint = Paint::Fixed(paint::DARK);
const GUN: Paint = Paint::Fixed(paint::GUNMETAL);
const STEEL: Paint = Paint::Metal(paint::GUNMETAL);
const YELLOW: Paint = Paint::Fixed(paint::YELLOW);
const GLASS: Paint = Paint::Fixed(paint::GLASS);
/// Hot nozzle throats.
const THROAT: Paint = Paint::Glow(paint::YELLOW);

pub fn design(frame: FrameId, d: &mut Designer) {
    match frame {
        FrameId::WingZero => wing_zero(d),
        FrameId::Leo => leo(d),
        FrameId::Taurus => taurus(d),
        FrameId::Virgo => virgo(d),
    }
}

// --- Placement helpers. ---

fn j(b: Bone) -> Vec3 {
    b.def().joint
}

fn place(pos: Vec3, rot: Quat) -> Affine3A {
    Affine3A::from_rotation_translation(rot, pos)
}

fn at(x: f32, y: f32, z: f32) -> Affine3A {
    Affine3A::from_translation(Vec3::new(x, y, z))
}

/// Centred between `a` and `b`, local +y running from `a` to `b`.
fn along(a: Vec3, b: Vec3) -> Affine3A {
    place((a + b) * 0.5, Quat::from_rotation_arc(Vec3::Y, (b - a).normalize_or(Vec3::Y)))
}

/// A placement described for the right side, on side `s`.
fn sided(s: Side, xf: Affine3A) -> Affine3A {
    if s == Side::L { mirrored(xf) } else { xf }
}

/// A point described for the right side, on side `s`.
fn sp(s: Side, p: Vec3) -> Vec3 {
    Vec3::new(p.x * s.sign(), p.y, p.z)
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn v2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}

fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

// --- Parts every frame shares. ---

/// The inner frame showing between armour: ball joints, knee and elbow barrels, neck, abdomen.
fn inner_frame(d: &mut Designer, waist_r: f32) {
    d.on(Bone::Torso).seed(0.11).lathe(
        &[
            (0.0, -0.1),
            (waist_r, 0.0),
            (waist_r * 1.08, 0.5),
            (waist_r, 1.0),
            (waist_r * 1.1, 1.5),
            (0.0, 1.7),
        ],
        16,
        FRAME,
        at(0.0, 0.3, 0.0),
    );
    d.on(Bone::Head).cylinder(0.45, 0.8, 10, FRAME, at(0.0, 5.75, 0.1));
    for s in Side::BOTH {
        let k = s.pick(0.0, 0.5);
        d.on(s.pick(Bone::ThighL, Bone::ThighR)).seed(0.2 + k).sphere(
            0.85,
            6,
            FRAME,
            sided(s, at(1.3, -0.7, 0.0)),
        );
        d.on(s.pick(Bone::ShinL, Bone::ShinR)).seed(0.3 + k).cylinder(
            0.72,
            1.3,
            12,
            FRAME,
            sided(s, place(j(Bone::ShinR), rz(std::f32::consts::FRAC_PI_2))),
        );
        d.on(s.pick(Bone::FootL, Bone::FootR)).sphere(
            0.55,
            5,
            FRAME,
            sided(s, Affine3A::from_translation(j(Bone::FootR))),
        );
        d.on(s.pick(Bone::UpperArmL, Bone::UpperArmR)).sphere(
            0.75,
            6,
            FRAME,
            sided(s, Affine3A::from_translation(j(Bone::UpperArmR))),
        );
        d.on(s.pick(Bone::ForearmL, Bone::ForearmR)).cylinder(
            0.52,
            1.05,
            10,
            FRAME,
            sided(s, place(j(Bone::ForearmR), rz(std::f32::consts::FRAC_PI_2))),
        );
    }
}

/// Hands: a palm and four fingers curled round a grip, and a thumb.
fn hands(d: &mut Designer, paint: Paint) {
    for s in Side::BOTH {
        let bone = s.pick(Bone::HandL, Bone::HandR);
        let w = j(Bone::HandR);
        let mut h = d.on(bone);
        h.seed(0.4 + s.pick(0.0, 0.3));
        h.cube(v(0.8, 1.0, 0.95), 0.08, paint, sided(s, at(w.x, w.y - 0.45, w.z + 0.1)));
        h.greeble(|h| {
            for f in 0..4 {
                let z = w.z + 0.45 - f as f32 * 0.28;
                h.cube(v(0.3, 0.26, 0.24), 0.04, paint, sided(s, place(v(w.x - 0.3, w.y - 1.0, z), rz(0.5))));
            }
            h.cube(
                v(0.26, 0.5, 0.26),
                0.04,
                paint,
                sided(s, place(v(w.x - 0.2, w.y - 0.55, w.z + 0.62), rx(0.6))),
            );
        });
        h.cylinder(0.42, 0.4, 10, FRAME, sided(s, at(w.x, w.y + 0.1, w.z)));
    }
}

/// A thruster bell at `pos` exhausting along `dir`, with its glowing throat; returns it as a socket.
fn nozzle(d: &mut Designer, bone: Bone, pos: Vec3, dir: Vec3, r: f32) {
    let rot = Quat::from_rotation_arc(Vec3::Y, dir.normalize());
    let xf = place(pos, rot);
    d.on(bone).lathe(
        &[
            (r * 0.55, -r * 0.2),
            (r * 0.62, 0.0),
            (r * 0.8, r * 0.6),
            (r, r * 1.3),
            (r * 0.9, r * 1.32),
            (r * 0.5, r * 0.1),
        ],
        14,
        STEEL,
        xf,
    );
    d.on(bone).lathe(&[(0.0, r * 0.15), (r * 0.5, r * 0.15)], 10, THROAT, xf);
    let local = Designer::local(bone, pos);
    d.sockets.nozzles.push((local, rot * Vec3::Y));
}

/// The beam saber's hilt, in the left hand, blade forward and up.
fn saber_hilt(d: &mut Designer) {
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.55, w.z + 0.2);
    let dir = v(0.0, 0.62, 0.78).normalize();
    d.on(Bone::HandL).cylinder(0.2, 1.4, 8, GUN, place(hilt, Quat::from_rotation_arc(Vec3::Y, dir)));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt + dir * 0.7), dir);
}

/// A rifle in the right hand: stock, body, barrel to the muzzle at `length` ahead of the grip.
fn rifle(d: &mut Designer, length: f32, bore: f32, body: Paint, trim: Paint) {
    let g = j(Bone::Weapon);
    let mut w = d.on(Bone::Weapon);
    w.seed(0.61);
    // Grip, receiver and stock.
    w.cube(v(0.35, 1.0, 0.45), 0.05, GUN, place(v(g.x, g.y - 0.2, g.z), rx(-0.25)));
    w.block(
        v(0.8, 1.0, length * 0.45),
        v2(0.85, 0.9),
        Vec2::ZERO,
        0.1,
        body,
        at(g.x, g.y + 0.35, g.z + length * 0.12),
    );
    w.block(v(0.6, 0.8, 1.6), v2(0.9, 0.7), v2(0.0, -0.1), 0.08, body, at(g.x, g.y + 0.25, g.z - 1.2));
    // Barrel and its shroud.
    let barrel = g.z + length * 0.35;
    w.cylinder(
        bore * 1.5,
        length * 0.3,
        12,
        trim,
        place(v(g.x, g.y + 0.45, barrel + length * 0.15), rx(std::f32::consts::FRAC_PI_2)),
    );
    w.cylinder(
        bore,
        length * 0.65,
        10,
        GUN,
        place(v(g.x, g.y + 0.45, g.z + length * 0.675), rx(std::f32::consts::FRAC_PI_2)),
    );
    w.greeble(|w| {
        // Sight and a vent row.
        w.cube(v(0.3, 0.35, 1.0), 0.05, GUN, at(g.x, g.y + 1.0, g.z + 0.6));
        for k in 0..3 {
            w.cube(v(0.84, 0.12, 0.2), 0.02, FRAME, at(g.x, g.y + 0.75, g.z + 0.9 + k as f32 * 0.4));
        }
    });
    d.sockets.muzzle = Designer::local(Bone::Weapon, v(g.x, g.y + 0.45, g.z + length));
}

/// A plate hung on a bone: size, place, and a gentle taper toward its lower edge.
#[allow(clippy::too_many_arguments)]
fn plate(
    d: &mut Designer,
    bone: Bone,
    s: Side,
    size: Vec3,
    pos: Vec3,
    rot: Quat,
    chamfer: f32,
    paint: Paint,
) {
    d.on(bone).block(size, v2(0.9, 0.95), Vec2::ZERO, chamfer, paint, sided(s, place(pos, rot)));
}

// --- XXXG-00W0 Wing Gundam Zero. ---

fn wing_zero(d: &mut Designer) {
    inner_frame(d, 1.25);
    hands(d, FRAME);

    // Head: white helmet, yellow V-fin, green eyes, red chin, vulcans at the temples.
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
    c.cube(v(2.9, 1.2, 2.6), 0.15, BODY, at(0.0, 2.55, 0.0));
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
        v(2.6, 1.3, 2.1),
        v2(1.2, 1.1),
        Vec2::ZERO,
        0.12,
        BODY,
        at(0.0, 1.45, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.19);
    w.cube(v(3.3, 0.45, 2.5), 0.08, FRAME, at(0.0, 0.25, 0.0));
    w.block(v(1.0, 1.3, 1.6), v2(1.5, 1.1), Vec2::ZERO, 0.1, BODY, at(0.0, -0.55, 0.25));
    w.cube(v(0.7, 0.45, 0.4), 0.05, ACCENT, at(0.0, -0.2, 1.1));
    for s in Side::BOTH {
        plate(d, Bone::Waist, s, v(1.3, 1.9, 0.3), v(1.05, -0.8, 1.25), rz(0.1) * rx(0.12), 0.08, TRIM);
        plate(d, Bone::Waist, s, v(0.3, 1.8, 1.9), v(2.2, -0.55, 0.0), rz(0.12), 0.08, BODY);
    }
    plate(d, Bone::Waist, Side::R, v(2.4, 1.4, 0.3), v(0.0, -0.4, -1.35), rx(-0.12), 0.08, BODY);

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
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.3 + s.pick(0.0, 0.2)).block(
            v(1.65, 3.2, 1.8),
            v2(1.05, 1.1),
            Vec2::ZERO,
            0.15,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.15), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.36 + s.pick(0.0, 0.2));
        sh.block(
            v(2.0, 3.5, 2.3),
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
        sh.cube(v(0.25, 1.4, 1.0), 0.04, YELLOW, sided(s, at(2.25, -6.3, -0.2)));
        sh.greeble(|sh| {
            sh.block(v(1.3, 1.6, 0.4), v2(0.8, 1.0), Vec2::ZERO, 0.06, TRIM, sided(s, at(1.3, -6.0, -1.25)));
        });
        let mut f = d.on(foot);
        f.seed(0.42 + s.pick(0.0, 0.2));
        f.block(v(1.45, 0.85, 3.1), v2(0.8, 0.65), v2(0.0, -0.2), 0.12, BODY, sided(s, at(1.3, -8.45, 0.45)));
        f.cube(v(1.3, 0.35, 0.8), 0.06, ACCENT, sided(s, at(1.3, -8.6, 1.75)));
        f.cube(v(1.55, 0.25, 3.3), 0.04, FRAME, sided(s, at(1.3, -8.95, 0.45)));
    }

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

    // Twin Buster Rifle: long, blue and grey, yellow bands.
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

/// Upper arms and forearms, with a cuff band at the wrist.
fn arm_segments(d: &mut Designer, upper: Vec3, fore: Vec3, paint: Paint, cuff: Paint) {
    for s in Side::BOTH {
        let (up, fo) = s.pick((Bone::UpperArmL, Bone::ForearmL), (Bone::UpperArmR, Bone::ForearmR));
        let (sh, el, wr) = (j(Bone::UpperArmR), j(Bone::ForearmR), j(Bone::HandR));
        d.on(up).seed(0.27 + s.pick(0.0, 0.3)).block(
            upper,
            v2(0.9, 0.9),
            Vec2::ZERO,
            0.12,
            paint,
            sided(s, along(el, sh)),
        );
        let mut f = d.on(fo);
        f.seed(0.33 + s.pick(0.0, 0.3));
        f.block(fore, v2(0.8, 0.85), Vec2::ZERO, 0.14, paint, sided(s, along(wr + (el - wr) * 0.1, el)));
        f.block(
            v(fore.x * 1.05, 0.45, fore.z * 1.05),
            v2(1.0, 1.0),
            Vec2::ZERO,
            0.06,
            cuff,
            sided(s, along(wr + (el - wr) * 0.12, wr + (el - wr) * 0.36)),
        );
    }
}

/// A shield outline (in its own x-y, y along the forearm) with a spine, on the left forearm facing out.
fn shield(d: &mut Designer, outline: &[(f32, f32)], face: Paint, spine: Paint) {
    let o: Vec<Vec2> = outline.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let pos = j(Bone::Shield);
    // Face outward (-x), long axis down the forearm and forward.
    let rot = Quat::from_rotation_arc(Vec3::Z, Vec3::NEG_X) * Quat::from_rotation_z(0.35);
    let mut s = d.on(Bone::Shield);
    s.seed(0.71);
    s.extrude(&o, 0.35, face, place(pos, rot));
    let spine_o: Vec<Vec2> = o.iter().map(|p| Vec2::new(p.x * 0.2, p.y * 0.92)).collect();
    s.extrude(&spine_o, 0.5, spine, place(pos + v(-0.1, 0.0, 0.0), rot));
}

// --- OZ-06MS Leo. ---

fn leo(d: &mut Designer) {
    inner_frame(d, 1.35);
    hands(d, FRAME);

    // Head: a rounded helmet with the mono-eye rail.
    let mut h = d.on(Bone::Head);
    h.seed(0.07);
    h.lathe(
        &[(0.0, 6.05), (0.95, 6.15), (1.08, 6.7), (0.9, 7.3), (0.45, 7.55), (0.0, 7.6)],
        16,
        BODY,
        at(0.0, 0.0, 0.1),
    );
    h.cube(v(1.9, 0.32, 0.5), 0.06, GLASS, at(0.0, 6.85, 0.85));
    h.sphere(0.17, 5, EYE, at(0.35, 6.85, 1.08));
    h.cube(v(0.7, 0.4, 0.35), 0.05, TRIM, at(0.0, 6.35, 0.95));
    h.greeble(|h| {
        for k in 0..3 {
            h.cube(v(0.6, 0.06, 0.1), 0.0, FRAME, at(0.0, 6.24 + k as f32 * 0.1, 1.13));
        }
        h.cube(v(0.18, 0.5, 1.1), 0.04, TRIM, at(0.0, 7.45, -0.05));
    });

    // Chest: a boxy block with a grey hatch; abdomen rings.
    let mut c = d.on(Bone::Chest);
    c.seed(0.14);
    c.block(v(4.4, 2.8, 3.2), v2(0.95, 0.9), v2(0.0, -0.1), 0.25, BODY, at(0.0, 4.0, 0.05));
    c.block(v(1.7, 1.4, 0.35), v2(0.85, 1.0), Vec2::ZERO, 0.08, TRIM, at(0.0, 3.9, 1.72));
    c.cube(v(2.2, 0.5, 2.4), 0.1, TRIM, at(0.0, 5.55, -0.1));
    c.greeble(|c| {
        for s in Side::BOTH {
            c.cube(v(0.8, 0.5, 0.2), 0.04, FRAME, sided(s, at(1.4, 4.9, 1.6)));
        }
    });
    let mut t = d.on(Bone::Torso);
    t.seed(0.16);
    for (k, r) in [1.45f32, 1.38, 1.3].iter().enumerate() {
        t.cylinder(*r, 0.38, 16, TRIM, at(0.0, 1.05 + k as f32 * 0.42, 0.0));
    }

    // Waist and skirt.
    let mut w = d.on(Bone::Waist);
    w.seed(0.2);
    w.cube(v(3.3, 0.5, 2.5), 0.08, FRAME, at(0.0, 0.25, 0.0));
    w.block(v(1.1, 1.3, 1.7), v2(1.4, 1.1), Vec2::ZERO, 0.12, BODY, at(0.0, -0.55, 0.2));
    for s in Side::BOTH {
        plate(d, Bone::Waist, s, v(1.5, 1.7, 0.35), v(1.05, -0.75, 1.2), rz(0.1) * rx(0.15), 0.12, BODY);
        plate(d, Bone::Waist, s, v(0.35, 1.6, 1.8), v(2.15, -0.5, 0.0), rz(0.12), 0.1, TRIM);
    }

    // Round shoulder armour, the Leo's mark, banded in grey.
    for s in Side::BOTH {
        let bone = s.pick(Bone::ShoulderL, Bone::ShoulderR);
        let axis = sided(s, place(v(4.2, 4.6, 0.0), rz(-std::f32::consts::FRAC_PI_2)));
        let mut sh = d.on(bone);
        sh.seed(0.25 + s.pick(0.0, 0.4));
        sh.lathe(
            &[(0.0, -1.25), (1.2, -1.1), (1.75, -0.4), (1.8, 0.2), (1.35, 1.0), (0.0, 1.2)],
            18,
            BODY,
            axis,
        );
        sh.lathe(&[(1.84, -0.1), (1.84, 0.25)], 18, TRIM, axis);
    }

    arm_segments(d, v(1.3, 1.9, 1.3), v(1.55, 2.0, 1.65), BODY, TRIM);

    // Legs: thick, segmented knees, big feet.
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.31 + s.pick(0.0, 0.2)).block(
            v(1.7, 3.2, 1.9),
            v2(1.05, 1.05),
            Vec2::ZERO,
            0.2,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.1), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.37 + s.pick(0.0, 0.2));
        sh.block(
            v(2.05, 3.5, 2.35),
            v2(0.8, 0.85),
            Vec2::ZERO,
            0.28,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.05), knee + v(0.0, -0.3, 0.0))),
        );
        sh.block(
            v(1.3, 1.1, 0.6),
            v2(0.8, 0.8),
            Vec2::ZERO,
            0.12,
            TRIM,
            sided(s, place(knee + v(0.0, 0.05, 0.95), rx(-0.25))),
        );
        sh.greeble(|sh| {
            sh.cube(v(1.5, 0.3, 0.3), 0.04, TRIM, sided(s, at(1.3, -7.3, 1.2)));
        });
        let mut f = d.on(foot);
        f.seed(0.43 + s.pick(0.0, 0.2));
        f.block(v(1.6, 0.95, 3.3), v2(0.8, 0.7), v2(0.0, -0.15), 0.15, BODY, sided(s, at(1.3, -8.45, 0.45)));
        f.cube(v(1.7, 0.25, 3.5), 0.04, FRAME, sided(s, at(1.3, -8.95, 0.45)));
    }

    // Backpack: a flat box with two bells.
    let mut b = d.on(Bone::Backpack);
    b.seed(0.51);
    b.block(v(2.8, 2.7, 1.7), v2(0.9, 0.9), Vec2::ZERO, 0.22, BODY, at(0.0, 4.0, -2.65));
    b.greeble(|b| {
        b.cube(v(2.0, 0.3, 0.3), 0.04, TRIM, at(0.0, 5.3, -3.2));
    });
    for s in Side::BOTH {
        nozzle(d, Bone::Backpack, sp(s, v(0.75, 3.1, -3.55)), v(0.0, -0.25, -1.0), 0.6);
    }

    // Beam rifle, and the drum-fed machine cannon on the left forearm.
    rifle(d, 7.5, 0.26, GUN, TRIM);
    let el = j(Bone::ForearmL);
    let mut f = d.on(Bone::ForearmL);
    f.seed(0.66);
    f.cylinder(
        0.25,
        3.0,
        10,
        GUN,
        place(v(el.x - 0.95, el.y - 1.0, el.z + 1.6), rx(std::f32::consts::FRAC_PI_2)),
    );
    f.cylinder(
        0.85,
        0.45,
        16,
        GUN,
        place(v(el.x - 1.05, el.y - 0.6, el.z + 0.2), rz(std::f32::consts::FRAC_PI_2)),
    );
    saber_hilt(d);
}

// --- OZ-13MS Taurus: slim, angular, a nose-cone backpack. ---

fn taurus(d: &mut Designer) {
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
        at(0.0, 1.5, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.21);
    w.cube(v(3.0, 0.4, 2.2), 0.06, FRAME, at(0.0, 0.25, 0.0));
    w.block(v(0.9, 1.2, 1.5), v2(1.5, 1.1), Vec2::ZERO, 0.08, TRIM, at(0.0, -0.5, 0.2));
    for s in Side::BOTH {
        plate(d, Bone::Waist, s, v(1.2, 1.6, 0.25), v(1.0, -0.75, 1.1), rz(0.14) * rx(0.2), 0.06, BODY);
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
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.32 + s.pick(0.0, 0.2)).block(
            v(1.4, 3.2, 1.6),
            v2(1.1, 1.1),
            Vec2::ZERO,
            0.12,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.1), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.38 + s.pick(0.0, 0.2));
        sh.block(
            v(1.6, 3.5, 1.9),
            v2(0.85, 1.1),
            v2(0.0, 0.15),
            0.15,
            BODY,
            sided(s, along(ankle + v(0.0, 0.4, 0.0), knee + v(0.0, -0.3, 0.0))),
        );
        // A swept fin down the back of the calf.
        let fin = [Vec2::new(0.0, 0.0), Vec2::new(0.4, 3.0), Vec2::new(-0.3, 3.2), Vec2::new(-1.2, 0.6)];
        sh.extrude(&fin, 0.18, TRIM, sided(s, place(v(1.3, -7.2, -1.0), ry(std::f32::consts::FRAC_PI_2))));
        let mut f = d.on(foot);
        f.seed(0.44 + s.pick(0.0, 0.2));
        f.block(v(1.2, 0.8, 3.2), v2(0.6, 0.5), v2(0.0, -0.3), 0.1, BODY, sided(s, at(1.3, -8.45, 0.55)));
        f.cube(v(1.3, 0.2, 3.3), 0.03, FRAME, sided(s, at(1.3, -8.92, 0.55)));
    }

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

// --- OZ-02MD Virgo: bulky, dark, a beam cannon and Planet Defensors. ---

fn virgo(d: &mut Designer) {
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
        at(0.0, 1.4, 0.05),
    );
    let mut w = d.on(Bone::Waist);
    w.seed(0.22);
    w.cube(v(3.8, 0.55, 2.8), 0.1, FRAME, at(0.0, 0.25, 0.0));
    w.block(v(1.3, 1.4, 1.9), v2(1.4, 1.1), Vec2::ZERO, 0.12, BODY, at(0.0, -0.55, 0.2));
    for s in Side::BOTH {
        plate(d, Bone::Waist, s, v(1.7, 1.9, 0.4), v(1.15, -0.85, 1.3), rz(0.12) * rx(0.12), 0.14, BODY);
        plate(d, Bone::Waist, s, v(0.4, 1.9, 2.1), v(2.35, -0.6, 0.0), rz(0.14), 0.12, BODY);
        let mut sh = d.on(s.pick(Bone::ShoulderL, Bone::ShoulderR));
        sh.seed(0.27 + s.pick(0.0, 0.4));
        sh.block(v(2.6, 2.3, 3.1), v2(0.9, 0.85), v2(0.1, 0.0), 0.3, BODY, sided(s, at(4.3, 4.9, 0.0)));
        sh.block(v(2.7, 0.4, 3.2), v2(1.0, 1.0), Vec2::ZERO, 0.06, TRIM, sided(s, at(4.3, 4.2, 0.0)));
    }
    arm_segments(d, v(1.6, 1.9, 1.6), v(1.9, 2.1, 1.95), BODY, TRIM);
    for s in Side::BOTH {
        let (thigh, shin, foot) =
            s.pick((Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR));
        let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
        d.on(thigh).seed(0.33 + s.pick(0.0, 0.2)).block(
            v(1.95, 3.2, 2.1),
            v2(1.05, 1.05),
            Vec2::ZERO,
            0.22,
            BODY,
            sided(s, along(knee + v(0.0, 0.4, -0.1), hip + v(0.0, -0.4, 0.0))),
        );
        let mut sh = d.on(shin);
        sh.seed(0.39 + s.pick(0.0, 0.2));
        sh.block(
            v(2.3, 3.5, 2.6),
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
        let mut f = d.on(foot);
        f.seed(0.45 + s.pick(0.0, 0.2));
        f.block(v(1.8, 1.0, 3.4), v2(0.85, 0.75), v2(0.0, -0.1), 0.15, BODY, sided(s, at(1.3, -8.45, 0.4)));
        f.cube(v(1.9, 0.25, 3.6), 0.04, FRAME, sided(s, at(1.3, -8.95, 0.4)));
    }
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

    // Planet Defensors: four discs riding round the suit.
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
