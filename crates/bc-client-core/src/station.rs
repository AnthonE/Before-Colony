//! The colony's station language (`docs/RING.md`, "The station language"): what a face drawn in it
//! carries for `bc-client`'s `station.wgsl`, the palette it's painted from, the sizes it's painted at
//! (a suit's deck, a person's halls), and its stencilled signs, laid out a letter at a time from the
//! glyph atlas.
//!
//! A face's vertices carry it in their colour: `r` its [`Surface`], `g` its paint (an entry of
//! [`PAINT`]), `b` what's painted on it (the flags: [`BAND`] and the rest), `a` a seed; and its UVs
//! are metres across it.

use std::borrow::Cow;

use glam::Vec3;

/// What a face is (`r` of its vertices' colour, in 255ths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    /// White panels (vents, access hatches), a kick plate; the band and its arrows, and chevrons at
    /// its foot, as its flags say.
    Wall = 1,
    /// Deck plates; the lanes and their chevrons, as its flags say.
    Floor = 2,
    /// Panels and light strips.
    Ceiling = 3,
    /// Grey, its collar in its paint; chevrons at its foot, as its flags say.
    Pillar = 4,
    /// Flat paint: rails, ribs, stringers, a slab's edge.
    Trim = 5,
    /// Light in its paint: a door's frame, a doorway's head, a lamp.
    Glow = 6,
    /// A door's leaves, meeting at its middle (`u` 0); chevrons along the meeting, as its flags say.
    Door = 7,
    /// A control room's window: dark glass, screens lit behind it.
    Window = 8,
    /// Yellow and black stripes.
    Hazard = 9,
}

/// Paints, the palette's entries (`g` of a vertex's colour, in 255ths).
pub const WHITE: u8 = 0;
pub const CHARTER: u8 = 1;
pub const CANAL: u8 = 2;
pub const GARDENS: u8 = 3;
pub const YELLOW: u8 = 4;
pub const DARK: u8 = 5;
pub const ORANGE: u8 = 6;
pub const RED: u8 = 7;
pub const OLIVE: u8 = 8;
pub const GUNMETAL: u8 = 9;
pub const HULL: u8 = 10;
/// How many there are.
pub const PAINTS: usize = 11;

/// The palette, linear: white panels; the lines' own colours (as the town's are painted,
/// `city_facade.wgsl`'s `f_line`: Charter blue, Canal teal, Gardens green); safety yellow, black,
/// safety orange, alarm red; and the base's own (`bc_model::paint`'s, made linear): military olive,
/// gunmetal, hull grey.
pub const PAINT: [[f32; 3]; PAINTS] = [
    [0.8, 0.81, 0.82],
    [0.12, 0.32, 0.72],
    [0.05, 0.5, 0.52],
    [0.2, 0.55, 0.22],
    [0.85, 0.55, 0.02],
    [0.025, 0.026, 0.03],
    [0.85, 0.22, 0.03],
    [0.6, 0.04, 0.03],
    [0.12, 0.141, 0.06],
    [0.055, 0.065, 0.078],
    [0.342, 0.355, 0.368],
];

/// What a face carries painted on it (`b` of a vertex's colour, in 255ths).
pub const BAND: u8 = 1;
/// Arrows in the band, pointing along `u` (or back along it, with [`BACK`]).
pub const ARROWS: u8 = 2;
pub const BACK: u8 = 4;
/// Chevrons at a wall's or a pillar's foot, or along a door's meeting.
pub const CHEVRONS: u8 = 8;
/// The lanes in a floor.
pub const LANES: u8 = 16;

/// A vertex's colour: its surface, paint, flags and seed.
pub fn colour(surface: Surface, paint: u8, flags: u8, seed: u8) -> [f32; 4] {
    [f32::from(surface as u8), f32::from(paint), f32::from(flags), f32::from(seed)].map(|v| v / 255.0)
}

/// The sizes the language is painted at, m: its panels, plates and joints, the kick plate at a
/// wall's foot (and its paint), the band round the walls and its arrows, the lanes in a floor and
/// their chevrons, the ceiling's light strips, the hazard stripes and a pillar's collar. One
/// material each (`bc-client`'s `Stations`), never changed after start-up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub panel: [f32; 2],
    pub plate: f32,
    pub joint: f32,
    pub kick: f32,
    pub kick_paint: u8,
    pub band: [f32; 2],
    pub arrow_every: f32,
    /// The two lanes' middles across a floor (its UV's `v`), and half their width.
    pub lanes: [f32; 2],
    pub lane_half: f32,
    pub chevron_every: f32,
    pub strip_every: f32,
    pub hazard_period: f32,
    pub collar: [f32; 2],
}

impl Scale {
    /// A suit's deck: the Ring Deck's (`ring_deck`), the band high over the suits' heads, a lane
    /// each way down the deck.
    pub const DECK: Self = Self {
        panel: [3.0, 2.0],
        plate: 4.0,
        joint: 0.03,
        kick: 1.0,
        kick_paint: HULL,
        band: [27.0, 30.0],
        arrow_every: 10.0,
        lanes: [-1.5, 8.0],
        lane_half: 4.0,
        chevron_every: 18.0,
        strip_every: 12.0,
        hazard_period: 1.2,
        collar: [6.0, 7.5],
    };

    /// A person's halls: the Muster's, Bay Row and the base's buildings (3 m storeys, 2.2 m
    /// doorways). The kick plate is the base's olive dado (`piece_mesh::DADO`), the band over the
    /// doorways' heads, and Bay Row's lanes a walk each way along it (`muster`).
    pub const MUSTER: Self = Self {
        panel: [1.5, 1.0],
        plate: 1.5,
        joint: 0.012,
        kick: 1.0,
        kick_paint: OLIVE,
        band: [2.35, 2.65],
        arrow_every: 4.0,
        lanes: [4.0, 8.5],
        lane_half: 1.6,
        chevron_every: 9.0,
        strip_every: 3.0,
        hazard_period: 0.5,
        collar: [1.6, 2.0],
    };

    /// As the shader's uniform has it (`station.wgsl`'s `Station`: `size`, `band`, `lanes`,
    /// `marks`), and the kick plate's paint.
    pub fn uniform(&self) -> ([[f32; 4]; 4], u8) {
        (
            [
                [self.panel[0], self.panel[1], self.plate, self.joint],
                [self.band[0], self.band[1], self.arrow_every, self.kick],
                [self.lanes[0], self.lanes[1], self.lane_half, self.chevron_every],
                [self.strip_every, self.hazard_period, self.collar[0], self.collar[1]],
            ],
            self.kick_paint,
        )
    }
}

/// A sign stencilled on a face: its words, where its middle is (on the face), the way the face
/// looks and the way up the words read, how tall a line of them is (m), and its ink.
#[derive(Clone, Debug, PartialEq)]
pub struct Sign {
    pub text: Cow<'static, str>,
    pub at: Vec3,
    pub normal: Vec3,
    pub up: Vec3,
    pub height: f32,
    pub ink: u8,
}

impl Sign {
    /// The way along the words read, looking at them.
    pub fn right(&self) -> Vec3 {
        self.up.cross(self.normal)
    }
}

/// The letters the glyph atlas holds, each in a cell of its own; a space is only a gap.
pub const GLYPHS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// A space, as a share of a line's height.
pub const SPACE: f32 = 0.3;

/// Which cell of the atlas a letter is in, if it has one.
pub fn glyph(c: char) -> Option<usize> {
    GLYPHS.find(c)
}

/// Whether the atlas can stencil `text`: its letters and spaces.
pub fn stencils(text: &str) -> bool {
    text.chars().all(|c| c == ' ' || glyph(c).is_some())
}

/// One letter of a sign: its cell in the atlas, and its quad's corners (top left, bottom left,
/// bottom right, top right, seen from where the sign faces).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    pub glyph: usize,
    pub corners: [Vec3; 4],
}

/// A sign's letters laid out along it, centred on it: each as wide as its glyph comes out at the
/// sign's height (`aspect`: a glyph's width over a line's height, as the atlas measured it), a
/// space [`SPACE`] of the height, and lifted `off` off its face.
pub fn lay_out(sign: &Sign, aspect: &[f32], off: f32) -> Vec<GlyphQuad> {
    let h = sign.height;
    let width = |c: char| glyph(c).map_or(SPACE, |g| aspect.get(g).copied().unwrap_or(SPACE)) * h;
    let total: f32 = sign.text.chars().map(width).sum();
    let (right, up) = (sign.right(), sign.up * h * 0.5);
    let mut x = -total * 0.5;
    let mut out = Vec::new();
    for c in sign.text.chars() {
        let w = width(c);
        if let Some(g) = glyph(c) {
            let left = sign.at + sign.normal * off + right * x;
            let rt = left + right * w;
            out.push(GlyphQuad { glyph: g, corners: [left + up, left - up, rt - up, rt + up] });
        }
        x += w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgsl;

    const SHADER: &str = include_str!("../../bc-client/src/shaders/station.wgsl");
    const ANIME: &str = include_str!("../../bc-client/src/shaders/anime.wgsl");
    const FACADE: &str = include_str!("../../bc-client/src/shaders/city_facade.wgsl");

    #[test]
    fn the_shaders_surfaces_and_flags_are_these() {
        let c = |name| wgsl::constant(SHADER, name);
        for (name, s) in [
            ("WALL", Surface::Wall),
            ("FLOOR", Surface::Floor),
            ("CEILING", Surface::Ceiling),
            ("PILLAR", Surface::Pillar),
            ("TRIM", Surface::Trim),
            ("GLOW", Surface::Glow),
            ("DOOR", Surface::Door),
            ("WINDOW", Surface::Window),
            ("HAZARD", Surface::Hazard),
        ] {
            assert_eq!(c(name), f32::from(s as u8), "{name}");
        }
        for (name, f) in [
            ("F_BAND", BAND),
            ("F_ARROWS", ARROWS),
            ("F_BACK", BACK),
            ("F_CHEVRONS", CHEVRONS),
            ("F_LANES", LANES),
        ] {
            assert_eq!(c(name), f32::from(f), "{name}");
        }
        for (name, p) in [("WHITE", WHITE), ("YELLOW", YELLOW), ("DARK", DARK)] {
            assert_eq!(c(name), f32::from(p), "{name}");
        }
        assert_eq!(c("PAINTS"), PAINTS as f32);
    }

    /// The uniform's fields, in the order `Scale::uniform` fills them (and `bc-client`'s
    /// `StationParams` declares them).
    #[test]
    fn the_shaders_uniform_is_laid_out_as_the_scale_fills_it() {
        let body = SHADER.split("struct Station {").nth(1).and_then(|s| s.split("};").next()).unwrap();
        let fields: Vec<&str> = body
            .lines()
            .map(|l| l.split("//").next().unwrap_or("").trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.split(':').next().unwrap().trim())
            .collect();
        assert_eq!(fields, ["paint", "time", "size", "band", "lanes", "marks", "look"]);
        assert!(body.contains(&format!("array<vec4<f32>, {PAINTS}>")), "the palette's length");
    }

    /// The lines' colours are the town's: Charter blue, Canal teal, Gardens green.
    #[test]
    fn the_lines_colours_are_the_towns() {
        let body = FACADE
            .split("fn f_line(")
            .nth(1)
            .and_then(|s| s.split("\n}").next())
            .expect("f_line in city_facade.wgsl");
        let colours: Vec<[f32; 3]> = body
            .lines()
            .filter_map(|l| l.split("vec3(").nth(1))
            .map(|v| {
                let v: Vec<f32> = v
                    .trim_end_matches(';')
                    .trim_end_matches(')')
                    .split(',')
                    .map(|x| x.trim().parse().unwrap())
                    .collect();
                [v[0], v[1], v[2]]
            })
            .collect();
        assert_eq!(colours, vec![PAINT[CHARTER as usize], PAINT[CANAL as usize], PAINT[GARDENS as usize]]);
    }

    /// As in `city.wgsl` (`CLAUDE.md`): every derivative at the top of `fragment()`, before the
    /// surface branch; and the painted look it shares with the hull shader takes none.
    #[test]
    fn the_station_shader_takes_its_derivatives_at_the_top() {
        let code = |src: &'static str| -> Vec<&'static str> {
            src.lines().map(|l| l.split("//").next().unwrap_or("")).collect()
        };
        let derivative = |l: &str| ["dpdx", "dpdy", "fwidth"].iter().any(|d| l.contains(d));
        assert!(!code(ANIME).iter().any(|l| derivative(l)), "anime.wgsl takes a derivative");
        let lines = code(SHADER);
        let first_branch = lines.iter().position(|l| l.trim_start().starts_with("if (surface")).unwrap();
        let start = lines.iter().position(|l| l.contains("fn fragment(")).unwrap();
        for (k, l) in lines.iter().enumerate() {
            assert!(!derivative(l) || (start..first_branch).contains(&k), "station.wgsl line {}: {l}", k + 1);
        }
    }

    /// Each scale's numbers make sense as a wall's: the kick under the band, the band inside a
    /// storey (or a suit's deck), the stripes and collar a whole number of panels or less.
    #[test]
    fn the_scales_paint_walls_that_read() {
        for s in [Scale::DECK, Scale::MUSTER] {
            assert!(s.kick < s.band[0] && s.band[0] < s.band[1]);
            assert!(s.joint < s.panel[0].min(s.panel[1]) * 0.05);
            assert!(s.lanes[0] + s.lane_half < s.lanes[1] - s.lane_half, "the lanes overlap");
            assert!(s.collar[0] < s.collar[1] && (s.kick_paint as usize) < PAINTS);
        }
        // A person's band over a doorway's head and under a storey's ceiling: `piece_mesh`'s tests.
    }

    #[test]
    fn a_signs_letters_are_laid_out_along_it_centred() {
        let sign = Sign {
            text: "BAY 07".into(),
            at: Vec3::ZERO,
            normal: Vec3::Z,
            up: Vec3::Y,
            height: 1.0,
            ink: DARK,
        };
        let aspect: Vec<f32> = (0..GLYPHS.len()).map(|g| 0.5 + 0.01 * g as f32).collect();
        let quads = lay_out(&sign, &aspect, 0.02);
        assert_eq!(quads.len(), 5);
        let total: f32 = sign.text.chars().map(|c| glyph(c).map_or(SPACE, |g| aspect[g])).sum();
        assert!((quads[0].corners[0].x + total * 0.5).abs() < 1e-5, "it starts half its width left");
        assert!((quads[4].corners[3].x - total * 0.5).abs() < 1e-5, "it ends half its width right");
        for q in &quads {
            assert!(q.corners.iter().all(|c| (c.z - 0.02).abs() < 1e-6), "on its face, a hair off");
            assert!((q.corners[0].y - 0.5).abs() < 1e-6 && (q.corners[1].y + 0.5).abs() < 1e-6);
        }
        // Each letter after the last, the space a gap.
        for pair in quads.windows(2) {
            assert!(pair[1].corners[0].x >= pair[0].corners[3].x - 1e-6);
        }
        assert!(
            (quads[3].corners[0].x - quads[2].corners[3].x - SPACE).abs() < 1e-5,
            "the space between Y and 0"
        );
        assert!(stencils("RING DECK 2") && !stencils("Bay 7") && !stencils("A·B"));
    }
}
