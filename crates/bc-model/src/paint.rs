//! Palette indices shared by the models and the client's hull shader (which holds the colours).

pub const WHITE: u8 = 0;
pub const BLUE: u8 = 1;
pub const RED: u8 = 2;
pub const YELLOW: u8 = 3;
/// Joints, hands and the inner frame.
pub const DARK: u8 = 4;
pub const ALMAGEST_GREEN: u8 = 5;
pub const ALMAGEST_GREY: u8 = 6;
pub const CORVUS_WHITE: u8 = 7;
pub const CORVUS_BLUE: u8 = 8;
pub const CETUS_OLIVE: u8 = 9;
pub const ALLIANCE_TAN: u8 = 10;
/// Colony hull plating.
pub const HULL: u8 = 11;
/// Colony hull, darker service plating.
pub const HULL_DARK: u8 = 12;
/// The colony's mirrors: aluminised film on a frame.
pub const MIRROR: u8 = 13;
/// Weapons.
pub const GUNMETAL: u8 = 14;
/// Black glass, a livery's glossy blue-black (Coalsack's): no darker than Black, so no bought
/// paint hides a suit against space. The suits' own visors are [`SENSOR_GLASS`].
pub const GLASS: u8 = 15;

/// The Almagest suits' inner frame: the Argo's joints, hands and feet, a dark olive brown. The first 16
/// entries are what a livery can name (a hull tag has four bits for each of its paints); from here
/// on, only a model's fixed paints reach them.
pub const FRAME_BROWN: u8 = 16;
/// The Argo's head: the red ribs round its sides and back.
pub const RIB_RED: u8 = 17;
/// The Argo's visor window, amber glass over the mono-eye.
pub const VISOR_AMBER: u8 = 18;
/// Sensor lenses and the glass of a suit's lights, a pale blue-grey.
pub const LENS: u8 = 19;
/// The Sagittarius's pale mint: its shoulder spheres, its waist band and the band round its hood.
pub const MINT: u8 = 20;
/// The Corona's pale lilac, where its twin has the mint.
pub const LILAC: u8 = 21;
/// Visors and sensor glass, near black: under every sensor's glow (the hull shader's eye slot),
/// and a model's fixed paint. No livery names it.
pub const SENSOR_GLASS: u8 = 22;

/// How many there are (the palette's size).
pub const COUNT: usize = 23;

/// Each paint's sRGB colour and perceptual roughness, in palette order: the hull shader's (the
/// client's `materials`), and the maintenance console's swatches.
pub const SRGB: [(f32, f32, f32, f32); COUNT] = [
    (0.9, 0.91, 0.93, 0.38),
    (0.1, 0.22, 0.66, 0.4),
    (0.72, 0.08, 0.08, 0.4),
    (0.95, 0.75, 0.1, 0.35),
    (0.11, 0.12, 0.13, 0.5),
    (0.4, 0.5, 0.32, 0.5),
    (0.44, 0.47, 0.5, 0.45),
    (0.83, 0.85, 0.88, 0.38),
    (0.2, 0.32, 0.6, 0.42),
    (0.38, 0.41, 0.27, 0.55),
    (0.6, 0.5, 0.34, 0.55),
    (0.62, 0.63, 0.64, 0.5),
    (0.3, 0.31, 0.33, 0.6),
    (0.66, 0.71, 0.8, 0.2),
    (0.26, 0.28, 0.31, 0.32),
    (0.11, 0.13, 0.16, 0.08),
    (0.33, 0.24, 0.18, 0.55),
    (0.78, 0.22, 0.16, 0.45),
    (0.96, 0.66, 0.12, 0.12),
    (0.62, 0.66, 0.76, 0.12),
    (0.6, 0.82, 0.78, 0.4),
    (0.76, 0.6, 0.76, 0.4),
    (0.02, 0.025, 0.03, 0.08),
];

/// The sensors' glows as a swatch shows them (`hull.wgsl`'s `EYES`, scaled to their brightest
/// channel): green, pink, amber, cyan.
pub const EYE_SRGB: [(f32, f32, f32); 4] =
    [(0.03, 1.0, 0.2), (1.0, 0.07, 0.37), (1.0, 0.5, 0.05), (0.07, 0.58, 1.0)];

/// A colour as `#rrggbb`.
pub fn hex((r, g, b): (f32, f32, f32)) -> String {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}
/// The most the hull shader's palette holds.
pub const CAPACITY: usize = 32;
const _: () = assert!(COUNT <= CAPACITY);

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::content::cosmetics::{DECALS, EYES, PAINTS};

    /// Relative luminance of an sRGB colour (Rec. 709, linear light).
    fn luminance((r, g, b): (f32, f32, f32)) -> f32 {
        let lin = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    fn rgb(k: u8) -> (f32, f32, f32) {
        let (r, g, b, _) = SRGB[usize::from(k)];
        (r, g, b)
    }

    /// The store sells appearance, never an edge (Elo Pros' no-pay-to-win rule): in a free-aim game
    /// against black space, nothing bought, and no decal, is darker than the darkest free paint.
    #[test]
    fn nothing_bought_is_darker_than_the_darkest_free_paint() {
        let floor = PAINTS
            .iter()
            .filter(|s| s.needs.is_none())
            .map(|s| luminance(rgb(s.paint)))
            .fold(f32::INFINITY, f32::min);
        assert!(floor > 0.01, "the darkest free paint, {floor}");
        for s in PAINTS.iter().filter(|s| s.needs.is_some()) {
            let l = luminance(rgb(s.paint));
            assert!(l >= floor, "{}: {l} under {floor}", s.name);
        }
        for s in EYES.iter().filter(|s| s.needs.is_some()) {
            let l = luminance(EYE_SRGB[usize::from(s.paint)]);
            assert!(l >= floor, "{}: {l} under {floor}", s.name);
        }
        assert!(DECALS.iter().any(|d| d.needs.is_some()));
        for ink in crate::decal::INKS {
            let l =
                luminance((f32::from(ink[0]) / 255.0, f32::from(ink[1]) / 255.0, f32::from(ink[2]) / 255.0));
            assert!(l >= floor, "decal ink {ink:?}: {l} under {floor}");
        }
    }
}
