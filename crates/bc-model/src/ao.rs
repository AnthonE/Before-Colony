//! Ambient occlusion baked into the suits' vertices: how much of the open sky each point on the
//! armour sees past the pieces round it, so the seams between plates, the gaps under a shoulder and
//! the vents in a chest hold shadow, lit or not.
//!
//! Every shape leaves a rough stand-in for its volume ([`Proxy`]); the whole suit's, at rest, are
//! what occludes. Each vertex looks out along its normal a few steps and adds up how far inside
//! the other shapes those steps land (the signed-distance estimate): nothing near, no occlusion; a
//! plate tucked under another, a lot. A shape never shades itself (all of them are convex).

use glam::Vec3;

use crate::kit::{Builder, Proxy};

/// Steps out along the normal (m), and how much each counts.
const STEPS: [(f32, f32); 5] = [(0.08, 1.0), (0.25, 0.75), (0.5, 0.55), (0.8, 0.4), (1.2, 0.3)];
/// How much occlusion takes off, and the least a vertex is left with.
const STRENGTH: f32 = 1.1;
const FLOOR: f32 = 0.35;

/// Bakes every bone's occlusion. `joints`: each bone's joint in the suit's frame at rest (its
/// vertices are relative to it).
pub fn bake(bones: &mut [Builder], joints: &[Vec3]) {
    let reach = STEPS[STEPS.len() - 1].0;
    // Every shape in the suit's frame: (bone, shape, volume, bounding centre and radius).
    let mut all: Vec<(usize, usize, Proxy, Vec3, f32)> = Vec::new();
    for (b, builder) in bones.iter().enumerate() {
        for (k, proxy) in builder.proxies().enumerate() {
            let proxy = proxy.shifted(joints[b]);
            let (c, r) = proxy.bounds();
            all.push((b, k, proxy, c, r));
        }
    }
    for (b, builder) in bones.iter_mut().enumerate() {
        let shapes = builder.vertex_shapes();
        let verts: Vec<(Vec3, Vec3)> = builder.vertices().map(|(p, n)| (p + joints[b], n)).collect();
        if verts.is_empty() {
            continue;
        }
        // Only shapes that could reach this bone at all.
        let (lo, hi) =
            verts.iter().fold((verts[0].0, verts[0].0), |(lo, hi), (p, _)| (lo.min(*p), hi.max(*p)));
        let (centre, radius) = ((lo + hi) * 0.5, (hi - lo).length() * 0.5 + reach);
        let near: Vec<&(usize, usize, Proxy, Vec3, f32)> =
            all.iter().filter(|s| s.3.distance(centre) - s.4 < radius).collect();
        let mut close: Vec<&Proxy> = Vec::new();
        let ao: Vec<f32> = verts
            .iter()
            .zip(&shapes)
            .map(|(&(p, n), &own)| {
                close.clear();
                close.extend(
                    near.iter()
                        .filter(|s| !(s.0 == b && s.1 == own) && s.3.distance(p) - s.4 < reach)
                        .map(|s| &s.2),
                );
                if close.is_empty() {
                    return 1.0;
                }
                let mut occ = 0.0;
                for (h, weight) in STEPS {
                    let q = p + n * h;
                    let d = close.iter().map(|s| s.distance(q)).fold(f32::MAX, f32::min);
                    occ += (h - d).clamp(0.0, h) / h * weight;
                }
                let total: f32 = STEPS.iter().map(|s| s.1).sum();
                (1.0 - STRENGTH * occ / total).clamp(FLOOR, 1.0)
            })
            .collect();
        builder.set_occlusion(&ao);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{Paint, at};

    fn bake_one(b: Builder) -> Vec<f32> {
        let mut bones = [b];
        bake(&mut bones, &[Vec3::ZERO]);
        let [b] = bones;
        b.finish().colors.iter().map(|c| c[3]).collect()
    }

    #[test]
    fn a_shape_on_its_own_is_open() {
        let mut b = Builder::default();
        b.cube(Vec3::splat(2.0), 0.1, Paint::Body, at(0.0, 0.0, 0.0));
        assert!(bake_one(b).iter().all(|a| *a == 1.0));
    }

    #[test]
    fn a_corner_between_two_shapes_is_shaded() {
        // A plate standing on a slab: the foot of the plate's sides is shaded by the slab; its top
        // edge, and the slab's far corners, see open sky.
        let mut b = Builder::default();
        b.cube(Vec3::new(4.0, 0.4, 4.0), 0.0, Paint::Body, at(0.0, 0.0, 0.0));
        b.cube(Vec3::new(0.4, 2.0, 4.0), 0.0, Paint::Body, at(0.0, 1.2, 0.0));
        let mut bones = [b];
        bake(&mut bones, &[Vec3::ZERO]);
        let [b] = bones;
        let verts: Vec<(Vec3, Vec3)> = b.vertices().collect();
        let mesh = b.finish();
        let worst = |want: &dyn Fn(Vec3, Vec3) -> bool| {
            verts
                .iter()
                .zip(&mesh.colors)
                .filter(|((p, n), _)| want(*p, *n))
                .map(|(_, c)| c[3])
                .fold(1.0, f32::min)
        };
        let foot = worst(&|p, n| n.x.abs() > 0.9 && p.x.abs() < 0.3 && p.y < 0.3);
        let top = worst(&|p, _| p.y > 2.1);
        let far = worst(&|p, n| n.y > 0.9 && p.x.abs() > 1.9 && p.y < 0.3);
        assert!(foot < 0.8, "the plate's foot: {foot}");
        assert_eq!(top, 1.0, "the plate's top edge");
        assert_eq!(far, 1.0, "the slab's far edge");
        assert!(mesh.colors.iter().all(|c| (FLOOR..=1.0).contains(&c[3])));
    }

    #[test]
    fn every_frame_bakes_quickly() {
        let t = std::time::Instant::now();
        for frame in bc_proto::FrameId::ALL {
            for lod in crate::LODS {
                crate::build(frame, lod);
            }
        }
        // Startup bakes every frame at both levels of detail (in the browser, a few times slower).
        let secs = t.elapsed().as_secs_f32();
        assert!(secs < 1.5, "building every frame took {secs:.2} s");
    }
}
