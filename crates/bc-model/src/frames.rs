//! The frames, built from the kit on the shared skeleton: each in its own module, and what they
//! share here. Each is described for the suit's right side and mirrored for the left; coordinates
//! are the suit's frame at rest, in metres (x right, y up, z forward, origin at the torso; about
//! 17 m from sole to crown).
//!
//! Paint slots: `Body`, `Trim` and `Accent` take the livery's colours, `Eye` its sensor glow; the
//! inner frame, weapons and details use fixed paints.

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

/// The beam saber's hilt, in the left hand, blade forward and up.
pub(crate) fn saber_hilt(d: &mut Designer) {
    let w = j(Bone::HandL);
    let hilt = v(w.x - 0.1, w.y - 0.55, w.z + 0.2);
    let dir = v(0.0, 0.62, 0.78).normalize();
    d.on(Bone::HandL).cylinder(0.2, 1.4, 8, GUN, place(hilt, Quat::from_rotation_arc(Vec3::Y, dir)));
    d.sockets.saber = (Designer::local(Bone::HandL, hilt + dir * 0.7), dir);
}
