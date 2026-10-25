//! The detailing every frame's armour shares, first worked out on the prototype pair: round ports
//! with their lenses, engraved slots, lathed bands, and faceted blocks from their sections.

use std::f32::consts::FRAC_PI_2;

use glam::{Affine3A, Quat, Vec3};

use crate::frames::{FRAME, TRIM, v};
use crate::kit::Paint;
use crate::{On, paint};

/// The inner frame, the hands, the ankles' discs and the heels' bands.
pub(crate) const GREY: Paint = Paint::Fixed(paint::HULL_DARK);
/// The cameras' and sensors' glass.
pub(crate) const LENS: Paint = Paint::Fixed(paint::LENS);

/// Local +y along `dir`.
pub(crate) fn toward(dir: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, dir.normalize())
}

/// A closed rectangle in a lathe's (radius, height) plane, for rings and bands.
pub(crate) fn band(r0: f32, r1: f32, y0: f32, y1: f32) -> [(f32, f32); 5] {
    [(r0, y0), (r1, y0), (r1, y1), (r0, y1), (r0, y0)]
}

/// An engraved slot on the face at `xf`'s origin (looking along its +z), along its y.
pub(crate) fn slot(o: &mut On<'_>, len: f32, xf: Affine3A) {
    o.greeble(|o| {
        o.cube(v(0.07, len, 0.05), 0.0, FRAME, xf);
    });
}

/// A round port on the face at `xf`'s origin, looking along its +z: a grey rim round a dark
/// socket and its lens.
pub(crate) fn port(o: &mut On<'_>, r: f32, xf: Affine3A) {
    let face = xf * Affine3A::from_rotation_x(FRAC_PI_2);
    o.lathe(&band(r * 0.62, r, -0.06, 0.08), 18, TRIM, face);
    o.cylinder(r * 0.64, 0.1, 16, FRAME, face);
    o.lathe(&[(0.0, 0.05), (r * 0.42, 0.05), (r * 0.3, 0.1), (0.0, 0.12)], 14, LENS, face);
}

/// A hexahedron's corners mirrored across x, reordered so its low-x corners stay the low ones.
pub(crate) fn swap_x(c: [Vec3; 8]) -> [Vec3; 8] {
    std::array::from_fn(|i| c[i ^ 1])
}

/// A block's section across at one height: (y, inner x, outer x, back z, front z).
pub(crate) type Cut = (f32, f32, f32, f32, f32);

/// A block's corners from its foot and its top.
pub(crate) fn stacked([(y0, i0, o0, b0, f0), (y1, i1, o1, b1, f1)]: [Cut; 2]) -> [Vec3; 8] {
    [
        v(i0, y0, b0),
        v(o0, y0, b0),
        v(i1, y1, b1),
        v(o1, y1, b1),
        v(i0, y0, f0),
        v(o0, y0, f0),
        v(i1, y1, f1),
        v(o1, y1, f1),
    ]
}
