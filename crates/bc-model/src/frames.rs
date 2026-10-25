//! The frames, built from the kit on the shared skeleton: each in its own module, and what they
//! share here. Each is described for the suit's right side and mirrored for the left; coordinates
//! are the suit's frame at rest, in metres (x right, y up, z forward, origin at the torso; about
//! 17 m from sole to crown).
//!
//! Paint slots: `Body`, `Trim` and `Accent` take the livery's colours, `Eye` its sensor glow; the
//! inner frame, weapons and details use fixed paints.

use std::f32::consts::FRAC_PI_2;

use bc_proto::FrameId;
use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::kit::{Paint, mirrored};
use crate::paint;
use crate::rig::{Bone, Side};
use crate::{Designer, algol, altair, antares, aquila, argo, canopus, cetus, corvus, prototypes, thuban};

pub(crate) const BODY: Paint = Paint::Body;
pub(crate) const TRIM: Paint = Paint::Trim;
pub(crate) const ACCENT: Paint = Paint::Accent;
pub(crate) const EYE: Paint = Paint::Eye;
/// The inner frame: joints, abdomen, hands.
pub(crate) const FRAME: Paint = Paint::Fixed(paint::DARK);
pub(crate) const GUN: Paint = Paint::Fixed(paint::GUNMETAL);
pub(crate) const STEEL: Paint = Paint::Metal(paint::GUNMETAL);
pub(crate) const YELLOW: Paint = Paint::Fixed(paint::YELLOW);
pub(crate) const GLASS: Paint = Paint::Fixed(paint::GLASS);
/// Hot nozzle throats.
pub(crate) const THROAT: Paint = Paint::Glow(paint::YELLOW);

pub fn design(frame: FrameId, d: &mut Designer) {
    match frame {
        FrameId::Altair => altair::altair(d),
        FrameId::Argo => argo::argo(d),
        FrameId::Corvus => corvus::corvus(d),
        FrameId::Cetus => cetus::cetus(d),
        FrameId::Antares => antares::antares(d),
        FrameId::Algol => algol::algol(d),
        FrameId::Canopus => canopus::canopus(d),
        FrameId::Thuban => thuban::thuban(d),
        FrameId::Aquila => aquila::aquila(d),
        FrameId::Sagittarius => prototypes::sagittarius(d),
        FrameId::Corona => prototypes::corona(d),
    }
}

// --- Placement helpers. ---

pub(crate) fn j(b: Bone) -> Vec3 {
    b.def().joint
}

pub(crate) fn place(pos: Vec3, rot: Quat) -> Affine3A {
    Affine3A::from_rotation_translation(rot, pos)
}

pub(crate) fn at(x: f32, y: f32, z: f32) -> Affine3A {
    Affine3A::from_translation(Vec3::new(x, y, z))
}

/// Centred between `a` and `b`, local +y running from `a` to `b`.
pub(crate) fn along(a: Vec3, b: Vec3) -> Affine3A {
    place((a + b) * 0.5, Quat::from_rotation_arc(Vec3::Y, (b - a).normalize_or(Vec3::Y)))
}

/// A placement described for the right side, on side `s`.
pub(crate) fn sided(s: Side, xf: Affine3A) -> Affine3A {
    if s == Side::L { mirrored(xf) } else { xf }
}

/// A point described for the right side, on side `s`.
pub(crate) fn sp(s: Side, p: Vec3) -> Vec3 {
    Vec3::new(p.x * s.sign(), p.y, p.z)
}

pub(crate) fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

pub(crate) fn v2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

pub(crate) fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

pub(crate) fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}

pub(crate) fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

// --- Parts every frame shares. ---

/// The inner frame showing between armour: ball joints, knee and elbow barrels, neck, abdomen.
pub(crate) fn inner_frame(d: &mut Designer, waist_r: f32) {
    d.on(Bone::Torso).seed(0.11).lathe(
        &[
            (0.0, -0.1),
            (waist_r, 0.0),
            (waist_r * 1.08, 0.35),
            (waist_r, 0.7),
            (waist_r * 1.1, 1.05),
            (0.0, 1.2),
        ],
        16,
        FRAME,
        at(0.0, 1.95, 0.0),
    );
    d.on(Bone::Head).cylinder(0.45, 0.8, 10, FRAME, at(0.0, 5.75, 0.1));
    for s in Side::BOTH {
        let k = s.pick(0.0, 0.5);
        d.on(s.pick(Bone::ThighL, Bone::ThighR)).seed(0.2 + k).sphere(
            0.85,
            6,
            FRAME,
            sided(s, Affine3A::from_translation(j(Bone::ThighR))),
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

/// How long a thigh's and a shin's armour run between the joints (overlapping their guards a
/// little), from the rig: on the anime skeleton the shin is the longer.
pub(crate) fn leg_lengths() -> (f32, f32) {
    let (hip, knee, ankle) = (j(Bone::ThighR), j(Bone::ShinR), j(Bone::FootR));
    let thigh = (knee + v(0.0, 0.4, -0.15) - (hip + v(0.0, -0.4, 0.0))).length() + 0.2;
    let shin = (ankle + v(0.0, 0.4, 0.0) - (knee + v(0.0, -0.3, 0.0))).length() + 0.5;
    (thigh, shin)
}

/// Where the waist's pieces sit, up from where these designs first drew them (round the old,
/// low hips): the waist rides over the hips.
pub(crate) const WAIST_UP: f32 = 1.75;

/// Hands: a palm and four fingers curled round a grip, and a thumb.
pub(crate) fn hands(d: &mut Designer, paint: Paint) {
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
pub(crate) fn nozzle(d: &mut Designer, bone: Bone, pos: Vec3, dir: Vec3, r: f32) {
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

// --- Feet. ---

/// The sole: how high the ground is under the origin at rest.
pub(crate) const SOLE: f32 = -9.07;

/// A block of a foot, centred on `x`, in the suit's frame: its foot level at `base`, narrowing
/// along z from `half.0` at the back to `half.1` at the front, its sides leaning in `lean` per
/// metre as they rise; its top falling from `top.0` at the back to `top.1` at the front; `z` its
/// back at the foot and at the top, then its front at the foot and at the top (a face slants
/// where they differ). Each side lies in one plane (its half-width is linear in z and in height),
/// so every face stays flat.
pub(crate) fn boot(
    x: f32,
    base: f32,
    top: (f32, f32),
    half: (f32, f32),
    z: [f32; 4],
    lean: f32,
) -> [Vec3; 8] {
    let [back, back_top, front, front_top] = z;
    let h = |at: f32, y: f32| half.0 + (half.1 - half.0) * (at - back) / (front - back) - lean * (y - base);
    std::array::from_fn(|i| {
        let (zz, y) = match (i & 4 != 0, i & 2 != 0) {
            (false, false) => (back, base),
            (false, true) => (back_top, top.0),
            (true, false) => (front, base),
            (true, true) => (front_top, top.1),
        };
        v(x + if i & 1 != 0 { h(zz, y) } else { -h(zz, y) }, y, zz)
    })
}

/// A vernier in a sole on side `s`'s `bone`, `r` round, its mouth at `mouth` (described for the
/// right foot, in the suit's frame): a grey ring standing just out of the sole's plate round a dark
/// throat glowing at its heart, all of it clear of the ground; a socket that fires it down.
pub(crate) fn vernier(d: &mut Designer, bone: Bone, s: Side, mouth: Vec3, r: f32) {
    let face = sided(s, Affine3A::from_translation(mouth));
    let mut o = d.on(bone);
    o.lathe(&[(r * 0.62, 0.0), (r, 0.0), (r, 0.07), (r * 0.62, 0.07), (r * 0.62, 0.0)], 16, TRIM, face);
    o.cylinder(r * 0.64, 0.06, 14, FRAME, face * at(0.0, 0.04, 0.0));
    o.cylinder(r * 0.3, 0.02, 10, THROAT, face * at(0.0, 0.012, 0.0));
    d.sockets.soles.push((bone, Designer::local(bone, sp(s, mouth)), Vec3::NEG_Y));
}

/// A sole under a foot's heel and instep, or under its toes ([`sole`]), described for the right
/// foot in the suit's frame: from `z.0` back to `z.1` forward, as wide as `half` at each end, the
/// plate's top `top` over the ground, its pads in `pads`, and its vernier `r` round at `vernier`
/// along z.
#[derive(Clone, Copy)]
pub(crate) struct Sole {
    pub z: (f32, f32),
    pub half: (f32, f32),
    pub top: f32,
    pub pads: Paint,
    pub vernier: f32,
    pub r: f32,
}

/// A sole on side `s`'s `bone`, centred on `x`: a dark plate with its foot a little off the ground,
/// and pads under it, inset from its edge, either side of the vernier, that touch the ground.
pub(crate) fn sole(d: &mut Designer, bone: Bone, s: Side, x: f32, sole: Sole) {
    let Sole { z, half, top, pads, vernier: at, r } = sole;
    let flat = sided(s, Affine3A::IDENTITY);
    let width = |w: f32| half.0 + (half.1 - half.0) * (w - z.0) / (z.1 - z.0);
    let mut o = d.on(bone);
    o.hexa(
        boot(x, SOLE + 0.05, (SOLE + top, SOLE + top), half, [z.0, z.0, z.1, z.1], 0.0),
        0.05,
        FRAME,
        flat,
    );
    let inset = 0.12;
    for (a, b) in [(z.0 + inset, at - r - 0.08), (at + r + 0.08, z.1 - inset)] {
        if b - a < 0.15 {
            continue;
        }
        let pad = boot(
            x,
            SOLE,
            (SOLE + 0.07, SOLE + 0.07),
            (width(a) - inset, width(b) - inset),
            [a, a, b, b],
            0.0,
        );
        o.hexa(pad, 0.03, pads, flat);
    }
    vernier(d, bone, s, v(x, SOLE + 0.015, at), r);
}

/// A Lodestar's feet ([`foot`]): their paints, how wide they are, and whether their heels have spurs.
#[derive(Clone, Copy)]
pub(crate) struct FootStyle {
    /// The heel's and the instep's armour, the toe cap's, and the pads under the sole.
    pub armour: Paint,
    pub toe: Paint,
    pub pads: Paint,
    /// Half the foot's width across its ball, m.
    pub half: f32,
    /// A spur up the back of the heel.
    pub spur: bool,
}

/// How far a foot's sides lean in, per metre up.
const FOOT_LEAN: f32 = 0.16;

/// The Lodestars' foot, as a model kit's is, on each foot bone and its toes: a tall heel with the
/// instep falling from it to the toes' hinge, a cover over the instep rising to the ankle, and a
/// guard either side of the ankle's ball with a round boss on it; the toe cap on the toe bone,
/// falling to a blunt tip with a ridge down its middle; a spur up the heel's back if `spur`; and
/// under both the sole: a dark plate split at the hinge, with pads that touch the ground and a
/// vernier between them under the heel and under the toes ([`sole`]).
pub(crate) fn foot(d: &mut Designer, style: FootStyle) {
    let (ankle, hinge) = (j(Bone::FootR), j(Bone::ToeR));
    let (x, h) = (ankle.x, style.half);
    let base = SOLE + 0.26;
    // The instep's top at the hinge, where the toe cap's takes over.
    let instep = -8.18;
    for s in Side::BOTH {
        let (foot, toe) = s.pick((Bone::FootL, Bone::ToeL), (Bone::FootR, Bone::ToeR));
        let flat = sided(s, Affine3A::IDENTITY);
        let k = s.pick(0.0, 0.2);
        let mut f = d.on(foot);
        f.seed(0.43 + k);
        f.sphere(0.52, 6, FRAME, sided(s, Affine3A::from_translation(ankle)));
        // The heel: a tall block behind the ankle, its back leaning in.
        f.hexa(
            boot(x, base, (-7.72, -7.72), (h - 0.04, h), [-1.8, -1.55, -0.25, -0.25], FOOT_LEAN),
            0.26,
            style.armour,
            flat,
        );
        // The instep, falling from the ankle to the hinge.
        f.hexa(
            boot(x, base, (-7.78, instep), (h, h), [-0.3, -0.3, hinge.z - 0.02, hinge.z - 0.08], FOOT_LEAN),
            0.2,
            style.armour,
            flat,
        );
        // The tongue over the instep, rising from it to cover the ankle's front.
        f.hexa(
            boot(x, -8.3, (-7.3, -8.02), (h * 0.62, h * 0.66), [-0.05, 0.2, hinge.z - 0.08, 1.12], 0.04),
            0.12,
            style.armour,
            flat,
        );
        // The ankle's guards, either side of its ball, a round boss on each.
        for side in [-1.0f32, 1.0] {
            let guard = sided(s, place(v(x + side * (h - 0.1), -7.85, -0.12), rz(side * 0.08)));
            f.block(v(0.26, 1.15, 1.45), v2(1.0, 0.82), v2(0.0, -0.05), 0.1, style.armour, guard);
            let boss = sided(s, place(v(x + side * (h + 0.06), -7.95, -0.2), rz(side * -FRAC_PI_2)));
            f.lathe(&[(0.0, 0.0), (0.3, 0.0), (0.3, 0.08), (0.2, 0.14), (0.0, 0.14)], 12, TRIM, boss);
        }
        if style.spur {
            // The spur, back and up off the heel's top.
            let spur = [v2(0.0, 0.0), v2(0.6, 0.0), v2(0.35, 0.3), v2(-0.3, 0.62), v2(-0.15, 0.2)];
            f.extrude(&spur, 0.2, style.armour, sided(s, place(v(x, -7.86, -1.45), ry(FRAC_PI_2))));
        }
        f.greeble(|f| {
            // The seam between the heel and the instep, and panel lines down the heel's sides.
            f.cube(v(h * 1.6, 0.05, 0.05), 0.0, FRAME, sided(s, at(x, -7.75, -0.28)));
            for side in [-1.0, 1.0] {
                let face = sided(s, place(v(x + side * (h - 0.02), -8.3, -1.0), rz(side * FOOT_LEAN.atan())));
                f.cube(v(0.04, 0.5, 0.05), 0.0, FRAME, face);
            }
        });
        let under = Sole {
            z: (-1.86, hinge.z - 0.04),
            half: (h + 0.06, h + 0.1),
            top: 0.27,
            pads: style.pads,
            vernier: -0.66,
            r: 0.34,
        };
        sole(d, foot, s, x, under);

        // The toe cap, falling and narrowing to its blunt nose, a ridge down its middle.
        let mut t = d.on(toe);
        t.seed(0.47 + k);
        t.hexa(
            boot(
                x,
                base,
                (instep - 0.02, -8.6),
                (h, h * 0.64),
                [hinge.z + 0.02, hinge.z + 0.02, 3.0, 2.66],
                FOOT_LEAN,
            ),
            0.24,
            style.toe,
            flat,
        );
        t.hexa(
            boot(
                x,
                -8.45,
                (-8.1, -8.38),
                (h * 0.3, h * 0.24),
                [hinge.z + 0.1, hinge.z + 0.14, 2.45, 2.3],
                0.0,
            ),
            0.06,
            style.toe,
            flat,
        );
        t.greeble(|t| {
            t.cube(v(h * 1.1, 0.05, 0.05), 0.0, FRAME, sided(s, place(v(x, -8.42, 2.3), rx(0.29))));
        });
        let under = Sole {
            z: (hinge.z + 0.04, 3.06),
            half: (h + 0.1, h * 0.7),
            top: 0.27,
            pads: style.pads,
            vernier: 2.12,
            r: 0.28,
        };
        sole(d, toe, s, x, under);
    }
}

/// The beam saber's hilt, in the left hand, blade forward and up.
pub(crate) fn saber_hilt(d: &mut Designer) {
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.55, w.z + 0.2);
    let dir = v(0.0, 0.62, 0.78).normalize();
    d.on(Bone::HandL).cylinder(0.2, 1.4, 8, GUN, place(hilt, Quat::from_rotation_arc(Vec3::Y, dir)));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt + dir * 0.7), dir);
}

/// A rifle in the right hand: stock, body, barrel to the muzzle at `length` ahead of the grip.
pub(crate) fn rifle(d: &mut Designer, length: f32, bore: f32, body: Paint, trim: Paint) {
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
pub(crate) fn plate(
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

/// Upper arms and forearms, with a cuff band at the wrist.
pub(crate) fn arm_segments(d: &mut Designer, upper: Vec3, fore: Vec3, paint: Paint, cuff: Paint) {
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
pub(crate) fn shield(d: &mut Designer, outline: &[(f32, f32)], face: Paint, spine: Paint) {
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
