//! The decals a suit can wear on its right shoulder (`bc_sim::content::cosmetics::DECALS`, by id),
//! drawn here as square RGBA images (sRGB, straight alpha), so the client has no art to decode:
//! each a roundel of plain shapes, read at a glance across a hangar.

/// An image's side, pixels.
pub const SIZE: usize = 128;

/// How many samples a pixel's edge takes across (its coverage is the share of `AA`² inside).
const AA: usize = 4;

type Shape = fn(f32, f32) -> bool;

/// Decal `id`'s image, `SIZE` × `SIZE` × 4 bytes, top row first (none for an id there isn't).
pub fn image(id: u8) -> Option<Vec<u8>> {
    let layers: &[(Shape, [u8; 3])] = match id {
        // The Ring: the colony's wheel, its rim and six spokes, white on the dark.
        1 => &[(disc, INK), (ring_rim, WHITE), (ring_spokes, WHITE), (hub, WHITE)],
        // The Lodestar: a white star in a gold ring.
        2 => &[(disc, NAVY), (gold_band, GOLD), (star, WHITE)],
        // Crux: the Southern Cross, its four bright stars and the fifth, small, between two.
        3 => &[(disc, NAVY), (edge_band, WHITE), (crux, WHITE)],
        _ => return None,
    };
    let mut out = vec![0u8; SIZE * SIZE * 4];
    for py in 0..SIZE {
        for px in 0..SIZE {
            // Over the layers in order, by how much of the pixel each covers.
            let (mut rgb, mut a) = ([0.0f32; 3], 0.0f32);
            for (shape, colour) in layers {
                let mut hits = 0;
                for sy in 0..AA {
                    for sx in 0..AA {
                        let x = ((px * AA + sx) as f32 + 0.5) / (SIZE * AA) as f32 * 2.0 - 1.0;
                        let y = 1.0 - ((py * AA + sy) as f32 + 0.5) / (SIZE * AA) as f32 * 2.0;
                        hits += usize::from(shape(x, y));
                    }
                }
                let c = hits as f32 / (AA * AA) as f32;
                for (k, v) in rgb.iter_mut().enumerate() {
                    *v = *v * (1.0 - c) + f32::from(colour[k]) * c;
                }
                a = a + c - a * c;
            }
            let at = (py * SIZE + px) * 4;
            for k in 0..3 {
                out[at + k] = rgb[k].round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = (a * 255.0).round() as u8;
        }
    }
    Some(out)
}

const WHITE: [u8; 3] = [236, 238, 240];
const INK: [u8; 3] = [22, 26, 34];
const NAVY: [u8; 3] = [18, 34, 74];
const GOLD: [u8; 3] = [226, 172, 40];

fn r(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn disc(x: f32, y: f32) -> bool {
    r(x, y) < 0.96
}

fn ring_rim(x: f32, y: f32) -> bool {
    (0.6..0.8).contains(&r(x, y))
}

fn hub(x: f32, y: f32) -> bool {
    r(x, y) < 0.2
}

fn ring_spokes(x: f32, y: f32) -> bool {
    let d = r(x, y);
    if !(0.15..0.65).contains(&d) {
        return false;
    }
    // Six spokes, one straight up: the distance off the nearest.
    let a = y.atan2(x) - core::f32::consts::FRAC_PI_2;
    let step = core::f32::consts::TAU / 6.0;
    let off = (a / step).round() * step - a;
    (d * off.sin()).abs() < 0.045
}

fn gold_band(x: f32, y: f32) -> bool {
    (0.8..0.92).contains(&r(x, y))
}

fn edge_band(x: f32, y: f32) -> bool {
    (0.86..0.92).contains(&r(x, y))
}

/// Whether `(x, y)` is in a five-pointed star round `(cx, cy)`, its points `outer` out (one up)
/// and its notches `inner`.
fn in_star(x: f32, y: f32, cx: f32, cy: f32, outer: f32, inner: f32) -> bool {
    let (dx, dy) = (x - cx, y - cy);
    let d = r(dx, dy);
    if d > outer {
        return false;
    }
    if d < inner {
        return true;
    }
    // Folded into one point's half: the angle off the nearest point, 0 to 36°.
    let step = core::f32::consts::TAU / 5.0;
    let a = dy.atan2(dx) - core::f32::consts::FRAC_PI_2;
    let off = (a - (a / step).round() * step).abs();
    // The edge from the point (outer, 0) to the notch (inner, 36°), as a line.
    let half = step / 2.0;
    let (px, py) = (outer, 0.0);
    let (qx, qy) = (inner * half.cos(), inner * half.sin());
    let (sx, sy) = (d * off.cos(), d * off.sin());
    // Inside is the origin's side of the edge.
    let side = |ax: f32, ay: f32| (qx - px) * (ay - py) - (qy - py) * (ax - px);
    side(sx, sy) * side(0.0, 0.0) >= 0.0
}

fn star(x: f32, y: f32) -> bool {
    in_star(x, y, 0.0, 0.0, 0.66, 0.27)
}

fn crux(x: f32, y: f32) -> bool {
    // Gacrux at the top, Acrux at the foot, Mimosa and δ across, ε between Acrux and δ.
    [(0.0, 0.56, 0.2), (0.04, -0.6, 0.23), (-0.46, 0.08, 0.19), (0.42, 0.16, 0.16), (0.24, -0.2, 0.09)]
        .into_iter()
        .any(|(cx, cy, s)| in_star(x, y, cx, cy, s, s * 0.42))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(img: &[u8], px: usize, py: usize) -> u8 {
        img[(py * SIZE + px) * 4 + 3]
    }

    #[test]
    fn each_decal_is_a_roundel_and_none_other() {
        for id in 1..=3u8 {
            let img = image(id).unwrap_or_else(|| panic!("decal {id}"));
            assert_eq!(img.len(), SIZE * SIZE * 4);
            // Solid in the middle, clear in the corners (the mask's cut is at half).
            assert_eq!(alpha(&img, SIZE / 2, SIZE / 2), 255, "{id}");
            assert_eq!(alpha(&img, 0, 0), 0, "{id}");
            assert_eq!(alpha(&img, SIZE - 1, SIZE - 1), 0, "{id}");
            // Something light on the dark: the design is there.
            let light = img.chunks(4).filter(|p| p[3] > 128 && p[0] > 200).count();
            let dark = img.chunks(4).filter(|p| p[3] > 128 && p[0] < 60).count();
            assert!(light > 400 && dark > 2_000, "{id}: {light} light, {dark} dark");
        }
        assert!(image(0).is_none() && image(4).is_none());
        // The Lodestar's star points up: white at the top of its point, the dark beside it.
        let star = image(2).unwrap();
        let px = |x: f32, y: f32| {
            let (i, j) = (((x + 1.0) / 2.0 * SIZE as f32) as usize, ((1.0 - y) / 2.0 * SIZE as f32) as usize);
            star[(j * SIZE + i) * 4]
        };
        assert!(px(0.0, 0.6) > 200);
        assert!(px(0.2, 0.6) < 60);
    }
}
