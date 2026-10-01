//! The colours of the cockpit HUD and the page around the game, in one place: the HUD (Bevy) draws
//! with these, and `web/style.css` declares the same values as its `:root` variables (a test
//! checks they agree), so whatever the look becomes, it changes here and there together.

/// An sRGB colour as `#rrggbb`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hex(pub &'static str);

const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

impl Hex {
    /// Its channels, 0..1 (sRGB, not linear).
    pub const fn srgb(self) -> [f32; 3] {
        let b = self.0.as_bytes();
        let r = digit(b[1]) as u32 * 16 + digit(b[2]) as u32;
        let g = digit(b[3]) as u32 * 16 + digit(b[4]) as u32;
        let bl = digit(b[5]) as u32 * 16 + digit(b[6]) as u32;
        [r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0]
    }
}

/// Readouts, the reticle, what's normal.
pub const CYAN: Hex = Hex("#9fe8ff");
/// Keys, values and cautions: the kill feed, salvage, a lock building.
pub const AMBER: Hex = Hex("#ffb547");
/// Warnings and hostiles.
pub const RED: Hex = Hex("#ff4b4b");
/// Friendlies, what can be grabbed, a safe place to park.
pub const GREEN: Hex = Hex("#8dffa8");
/// The ZERO System.
pub const PINK: Hex = Hex("#ff5fc8");
/// Values on the panels: the suit's own readouts, what's whole.
pub const WHITE: Hex = Hex("#eef4fb");
/// Labels on the panels, their edges, what's in reserve.
pub const LABEL: Hex = Hex("#9fc6e6");
/// The dark the panels are made of (and the text on a bright button).
pub const INK: Hex = Hex("#040a12");

/// The CSS variable each colour is declared as, in `web/style.css`.
pub const CSS: [(&str, Hex); 8] = [
    ("--cyan", CYAN),
    ("--amber", AMBER),
    ("--red", RED),
    ("--green", GREEN),
    ("--pink", PINK),
    ("--ink", INK),
    ("--white", WHITE),
    ("--label", LABEL),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_as_srgb() {
        assert_eq!(Hex("#ff0080").srgb(), [1.0, 0.0, 128.0 / 255.0]);
    }

    #[test]
    fn the_page_declares_the_same_palette() {
        let css = include_str!("../../../web/style.css");
        for (var, hex) in CSS {
            let decl = format!("{var}: {};", hex.0);
            assert!(css.contains(&decl), "web/style.css should declare `{decl}` (bc_client_core::palette)");
        }
    }
}
