//! The Ring Deck: the bay ring's suit deck, a hall that runs along the ring behind its bays, where
//! suits walk between the bays and the lifts. The colony's own infrastructure in its clean language
//! (`docs/COLONY_LOOK.md`, "The station language"): white panels, the lines' colours for
//! wayfinding, safety yellow and black only where there's a hazard, stencilled signs in plain
//! English, and glass in the rim's floor with Earth wheeling past under it as the ring turns.
//!
//! A showcase set for now (`?showcase=ring`), drawn by `bc-client`'s `station`: nothing walks or
//! collides with it, and the server knows nothing of it.
//!
//! Metres, the deck's floor centre the origin: x along the colony's axis (+x the bays' wall, −x the
//! rim, where the floor stops at a rail over the glass), y up (towards the spin axis), z along the
//! ring (−z the launch tunnels' blast door, +z the passage to the cap lifts). The deck's axes are
//! the world's, so the colony's spin turns the sky about x.

use glam::Vec3;

use crate::city_mesh::CityMesh;

/// Where the deck is in the world: far below the sector, so that nothing in it (the colony, the
/// field, the landmarks) is in view through the rim's glass, which looks out and down.
pub const DECK_ORIGIN: Vec3 = Vec3::new(0.0, -30_000.0, 0.0);

/// The hall: x ±24, z ±96, 34 m high.
pub const HALF_WIDTH: f32 = 24.0;
pub const HALF_LENGTH: f32 = 96.0;
pub const HEIGHT: f32 = 34.0;
/// Where the floor stops at the rim's rail, and how deep the glass under it goes.
pub const RIM_X: f32 = -8.0;
pub const TROUGH: f32 = -6.0;
/// The bays' doors in the +x wall: their numbers, centres along the deck, half-width and height.
pub const BAYS: [u8; 3] = [7, 8, 9];
pub const DOOR_Z: [f32; 3] = [-50.0, 0.0, 50.0];
pub const DOOR_HALF_WIDTH: f32 = 12.0;
pub const DOOR_HEIGHT: f32 = 26.0;
/// The catwalk along the rim's wall, at a suit's chest, and where it runs; the stairs down from
/// its end, and the bridge across the trough to the deck at their foot.
pub const CATWALK_Y: f32 = 14.0;
pub const CATWALK_Z: [f32; 2] = [-70.0, 62.0];
const CATWALK_WIDTH: f32 = 4.5;
const STEPS: usize = 47;
const STEP_RUN: f32 = 0.4;
const BRIDGE_Z: [f32; 2] = [80.8, 84.0];
/// The opening in the +z end wall into the passage, and how far the passage goes.
pub const OPENING_X: [f32; 2] = [-4.0, 16.0];
const OPENING_HEIGHT: f32 = 26.0;
pub const PASSAGE_X: [f32; 2] = [-30.0, 40.0];
pub const PASSAGE_END: f32 = 124.0;
const PASSAGE_HEIGHT: f32 = 32.0;
/// The pillars by the bays' wall, between and beyond the doors (and the beams over them).
pub const PILLAR_Z: [f32; 4] = [-75.0, -25.0, 25.0, 75.0];
const PILLAR_X: [f32; 2] = [19.0, 21.4];

// What the shader paints (`station.wgsl` keeps the same numbers).
/// Wall panels, m.
pub const PANEL_W: f32 = 3.0;
pub const PANEL_H: f32 = 2.0;
/// Deck and ceiling plates, m.
pub const PLATE: f32 = 4.0;
/// The dark kick plate at a wall's foot, m.
pub const KICK: f32 = 1.0;
/// The wayfinding band round the walls, and the arrows in it, one pair every so often, m.
pub const BAND_Y0: f32 = 27.0;
pub const BAND_Y1: f32 = 30.0;
pub const ARROW_EVERY: f32 = 10.0;
/// The suits' two lanes down the deck (their centres across it, and half their width), and the
/// chevrons painted in them, one every so often, m: the first lane runs towards the lifts (+z),
/// the second back.
pub const LANE_X0: f32 = -1.5;
pub const LANE_X1: f32 = 8.0;
pub const LANE_HALF: f32 = 4.0;
pub const CHEVRON_EVERY: f32 = 18.0;
/// The ceiling's light strips run along the deck, one every so often across it, m.
pub const STRIP_EVERY: f32 = 12.0;
/// Hazard stripes' period, m.
pub const HAZARD_PERIOD: f32 = 1.2;
/// A pillar's collar in its paint, m up.
pub const COLLAR_Y0: f32 = 6.0;
pub const COLLAR_Y1: f32 = 7.5;

/// What a face is, for the station shader (`r` of its vertices' colour, in 255ths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    /// White panels (vents, access hatches), a dark kick plate; the band and its arrows, and
    /// chevrons at its foot, as its flags say.
    Wall = 1,
    /// Deck plates; the suits' lanes and their chevrons, as its flags say.
    Floor = 2,
    /// Panels and light strips.
    Ceiling = 3,
    /// Grey, its collar in its paint; chevrons at its foot, as its flags say.
    Pillar = 4,
    /// Flat paint: rails, ribs, stringers.
    Trim = 5,
    /// Light in its paint: a bay door's frame.
    Glow = 6,
    /// A door's leaves, meeting at its middle (`u` 0); chevrons along the meeting, as its flags say.
    Door = 7,
    /// The control room's window: dark glass, screens lit behind it.
    Window = 8,
    /// Yellow and black stripes.
    Hazard = 9,
}

/// Paints, the deck palette's entries (`g` of a vertex's colour, in 255ths).
pub const WHITE: u8 = 0;
pub const CHARTER: u8 = 1;
pub const CANAL: u8 = 2;
pub const GARDENS: u8 = 3;
pub const YELLOW: u8 = 4;
pub const DARK: u8 = 5;
pub const ORANGE: u8 = 6;
pub const RED: u8 = 7;

/// The palette, linear: white panels; the lines' own colours (as the city paints them,
/// `city_facade.wgsl`'s `f_line`: Charter blue, Canal teal, Gardens green); safety yellow, black,
/// safety orange, alarm red.
pub const PAINT: [[f32; 3]; 8] = [
    [0.8, 0.81, 0.82],
    [0.12, 0.32, 0.72],
    [0.05, 0.5, 0.52],
    [0.2, 0.55, 0.22],
    [0.85, 0.55, 0.02],
    [0.025, 0.026, 0.03],
    [0.85, 0.22, 0.03],
    [0.6, 0.04, 0.03],
];

/// What a face carries painted on it (`b` of a vertex's colour, in 255ths).
pub const BAND: u8 = 1;
/// Arrows in the band, pointing along `u` (or back along it, with [`BACK`]).
pub const ARROWS: u8 = 2;
pub const BACK: u8 = 4;
/// Chevrons at a wall's or a pillar's foot, or along a door's meeting.
pub const CHEVRONS: u8 = 8;
/// The suits' lanes in a floor.
pub const LANES: u8 = 16;

/// A solid: a box with edges `a`, `b`, `c` from its corner `o` (a parallelepiped: most are
/// axis-aligned, the rim's ribs and the stairs' stringers aren't).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub o: Vec3,
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub surface: Surface,
    pub paint: u8,
    pub flags: u8,
    /// Its faces' `u` is measured from its middle (a door's leaves meet at `u` 0), not the deck's.
    pub centred: bool,
}

impl Block {
    /// An axis-aligned box.
    pub fn new(min: [f32; 3], max: [f32; 3], surface: Surface) -> Self {
        let (min, max) = (Vec3::from(min), Vec3::from(max));
        let d = max - min;
        Self::slab(min, Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z, surface)
    }

    pub fn slab(o: Vec3, a: Vec3, b: Vec3, c: Vec3, surface: Surface) -> Self {
        Self { o, a, b, c, surface, paint: WHITE, flags: 0, centred: false }
    }

    fn paint(mut self, paint: u8) -> Self {
        self.paint = paint;
        self
    }

    fn flags(mut self, flags: u8) -> Self {
        self.flags = flags;
        self
    }

    fn centred(mut self) -> Self {
        self.centred = true;
        self
    }

    pub fn centre(&self) -> Vec3 {
        self.o + (self.a + self.b + self.c) * 0.5
    }

    /// Its corners' bounds.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for k in 0..8 {
            let p = self.o
                + if k & 1 == 1 { self.a } else { Vec3::ZERO }
                + if k & 2 == 2 { self.b } else { Vec3::ZERO }
                + if k & 4 == 4 { self.c } else { Vec3::ZERO };
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (lo, hi)
    }

    /// Its six faces: a corner and the two edges from it, wound so that `e1 × e2` points out.
    pub fn faces(&self) -> [(Vec3, Vec3, Vec3); 6] {
        let (o, a, b, c) = (self.o, self.a, self.b, self.c);
        let mid = self.centre();
        [(o, b, c), (o + a, b, c), (o, a, c), (o + b, a, c), (o, a, b), (o + c, a, b)].map(|(p, e1, e2)| {
            let out = (p + (e1 + e2) * 0.5) - mid;
            if e1.cross(e2).dot(out) >= 0.0 { (p, e1, e2) } else { (p, e2, e1) }
        })
    }
}

/// A sign stencilled on a face: its words, where its middle is (on the face), the way the face
/// looks and the way up the words read, how tall a line of them is (m), and its ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sign {
    pub text: &'static str,
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

/// The deck: every solid, every sign, and the rim's glass.
#[derive(Clone, Debug)]
pub struct Layout {
    pub blocks: Vec<Block>,
    pub signs: Vec<Sign>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        use Surface::*;
        let (w, l, h, t) = (HALF_WIDTH, HALF_LENGTH, HEIGHT, 1.0);
        let walls = BAND | ARROWS;
        let mut b = vec![
            // The deck, out to the rim; the ceiling.
            Block::new([RIM_X, -t, -l], [w, 0.0, l], Floor).flags(LANES),
            Block::new([-w, h, -l], [w, h + t, l], Ceiling),
            // The rim's wall, over the glass; under the deck's edge, the trough's inner side.
            Block::new([-w - t, 0.0, -l], [-w, h, l], Wall).paint(CHARTER).flags(walls),
            Block::new([RIM_X, TROUGH - t, -l], [RIM_X + t, -t, l], Wall),
            // The rim's hazard line, along the deck's edge.
            Block::new([RIM_X, 0.0, -l], [RIM_X + 1.2, 0.02, l], Hazard),
            // The launch tunnels' end: its wall, the blast door, the hazard frame round it.
            Block::new([-w - t, TROUGH - t, -l - t], [w + t, h + t, -l], Wall).paint(CHARTER).flags(BAND),
            Block::new([-8.0, 0.0, -l], [20.0, 24.0, -l + 0.4], Door).flags(CHEVRONS).centred(),
            Block::new([-9.2, 0.0, -l], [-8.0, 25.2, -l + 0.3], Hazard),
            Block::new([20.0, 0.0, -l], [21.2, 25.2, -l + 0.3], Hazard),
            Block::new([-8.0, 24.0, -l], [20.0, 25.2, -l + 0.3], Hazard),
            // The lifts' end: its wall round the opening into the passage, a hazard frame and line.
            Block::new([-w - t, TROUGH - t, l], [OPENING_X[0], h + t, l + t], Wall)
                .paint(CHARTER)
                .flags(BAND),
            Block::new([OPENING_X[1], TROUGH - t, l], [w + t, h + t, l + t], Wall).paint(CHARTER).flags(BAND),
            Block::new([OPENING_X[0], OPENING_HEIGHT, l], [OPENING_X[1], h + t, l + t], Wall)
                .paint(CHARTER)
                .flags(BAND),
            Block::new([OPENING_X[0] - 1.2, 0.0, l - 0.3], [OPENING_X[0], OPENING_HEIGHT, l], Hazard),
            Block::new([OPENING_X[1], 0.0, l - 0.3], [OPENING_X[1] + 1.2, OPENING_HEIGHT, l], Hazard),
            Block::new([OPENING_X[0], 0.0, l - 0.6], [OPENING_X[1], 0.02, l + 0.6], Hazard),
        ];

        // The bays' wall: full height between and beyond the doors, and over each door; the doors
        // a metre back in it, a lit frame round each and a hazard line on the deck before it.
        let mut z0 = -l;
        for zc in DOOR_Z {
            let (d0, d1) = (zc - DOOR_HALF_WIDTH, zc + DOOR_HALF_WIDTH);
            b.push(Block::new([w, 0.0, z0], [w + t, h, d0], Wall).paint(CHARTER).flags(walls));
            b.push(Block::new([w, DOOR_HEIGHT, d0], [w + t, h, d1], Wall).paint(CHARTER).flags(walls));
            b.push(
                Block::new([w + t, 0.0, d0], [w + t + 1.0, DOOR_HEIGHT, d1], Door).flags(CHEVRONS).centred(),
            );
            let f = DOOR_HEIGHT + 0.6;
            b.push(Block::new([w - 0.15, 0.0, d0 - 0.6], [w, f, d0], Glow).paint(GARDENS));
            b.push(Block::new([w - 0.15, 0.0, d1], [w, f, d1 + 0.6], Glow).paint(GARDENS));
            b.push(Block::new([w - 0.15, DOOR_HEIGHT, d0], [w, f, d1], Glow).paint(GARDENS));
            b.push(Block::new([w - 1.6, 0.0, d0], [w, 0.02, d1], Hazard));
            z0 = d1;
        }
        b.push(Block::new([w, 0.0, z0], [w + t, h, l], Wall).paint(CHARTER).flags(walls));

        // The pillars by the bays' wall, and the beams across the ceiling over them.
        for z in PILLAR_Z {
            b.push(
                Block::new([PILLAR_X[0], 0.0, z - 1.2], [PILLAR_X[1], h, z + 1.2], Pillar)
                    .paint(CHARTER)
                    .flags(CHEVRONS),
            );
            b.push(Block::new([-w, h - 2.5, z - 0.9], [w, h, z + 0.9], Pillar).paint(DARK));
        }

        // The rim's rail, a suit's knee high, open where the bridge from the stairs comes over.
        let open = (BRIDGE_Z[0] - 0.3, BRIDGE_Z[1] + 0.3);
        for (z0, z1) in [(-l, open.0), (open.1, l)] {
            for (y0, y1) in [(1.5, 1.8), (3.0, 3.5)] {
                b.push(Block::new([RIM_X - 0.35, y0, z0], [RIM_X + 0.35, y1, z1], Trim).paint(ORANGE));
            }
        }
        let mut z = -l + 4.0;
        while z < l {
            if z < open.0 - 0.3 || z > open.1 + 0.3 {
                b.push(
                    Block::new([RIM_X - 0.3, 0.0, z - 0.3], [RIM_X + 0.3, 3.0, z + 0.3], Trim).paint(ORANGE),
                );
            }
            z += 8.0;
        }

        // The glass's ribs, every 12 m down the deck, from the rim's wall to the trough's foot.
        let down = Vec3::new(RIM_X + w, TROUGH, 0.0);
        let inward = glass_normal() * 0.5;
        let mut z = -l + 6.0;
        while z < l {
            b.push(Block::slab(Vec3::new(-w, 0.0, z - 0.3), down, Vec3::Z * 0.6, inward, Trim));
            z += 12.0;
        }

        // The catwalk along the rim's wall, its rail, and the brackets under it.
        let (cy, cw) = (CATWALK_Y, CATWALK_WIDTH);
        let [cz0, cz1] = CATWALK_Z;
        b.push(Block::new([-w, cy - 0.3, cz0], [-w + cw, cy, cz1], Floor));
        for (y0, y1) in [(0.5, 0.56), (1.0, 1.08)] {
            b.push(Block::new([-w + cw - 0.15, cy + y0, cz0], [-w + cw, cy + y1, cz1], Trim).paint(ORANGE));
        }
        let mut z = cz0 + 1.0;
        while z < cz1 {
            b.push(
                Block::new([-w + cw - 0.15, cy, z - 0.04], [-w + cw, cy + 1.0, z + 0.04], Trim).paint(ORANGE),
            );
            z += 2.5;
        }
        let mut z = cz0 + 5.0;
        while z < cz1 {
            b.push(Block::new([-w, cy - 1.6, z - 0.15], [-w + cw - 0.7, cy - 0.3, z + 0.15], Pillar));
            z += 10.0;
        }
        // The stairs down from its end, open treads; their stringers and rail; the landing at
        // their foot and the bridge across the trough to the deck.
        let rise = cy / STEPS as f32;
        let sw = 3.5;
        for k in 0..STEPS {
            let (y, z) = (cy - (k + 1) as f32 * rise, cz1 + k as f32 * STEP_RUN);
            b.push(Block::new([-w, y - 0.1, z], [-w + sw, y, z + STEP_RUN], Floor));
        }
        let run = Vec3::new(0.0, -cy, STEPS as f32 * STEP_RUN);
        for x in [-w, -w + sw] {
            b.push(
                Block::slab(Vec3::new(x, cy, cz1), run, Vec3::X * 0.12, Vec3::NEG_Y * 0.5, Trim).paint(DARK),
            );
        }
        b.push(
            Block::slab(Vec3::new(-w + sw, cy + 1.0, cz1), run, Vec3::X * 0.12, Vec3::Y * 0.08, Trim)
                .paint(ORANGE),
        );
        b.push(Block::new([-w, -0.3, BRIDGE_Z[0]], [RIM_X, 0.0, BRIDGE_Z[1]], Floor));

        // The control room at the catwalk's far end, its window on the deck.
        b.push(Block::new([-w, cy - 0.3, -88.0], [-w + 6.0, cy + 6.0, cz0], Wall));
        b.push(Block::new([-w + 6.0, cy + 1.2, -86.5], [-w + 6.1, cy + 4.6, -71.5], Window));

        // The passage to the lifts: its floor and ceiling, its walls (the lifts' line's colour,
        // arrows each way along the back wall), the lifts' door at its −x end and the airlock's
        // at its +x end, each framed in light.
        let [px0, px1] = PASSAGE_X;
        let (pz0, pz1, ph) = (l, PASSAGE_END, PASSAGE_HEIGHT);
        b.extend([
            Block::new([px0, -t, pz0], [px1, 0.0, pz1], Floor),
            Block::new([px0, ph, pz0 + t], [px1, ph + t, pz1], Ceiling),
            Block::new([px0 - t, 0.0, pz1], [5.0, ph, pz1 + t], Wall).paint(CANAL).flags(walls | BACK),
            Block::new([5.0, 0.0, pz1], [px1 + t, ph, pz1 + t], Wall).paint(CANAL).flags(walls),
            Block::new([px0 - t, 0.0, pz0 + t], [px0, ph, pz1], Wall).paint(CANAL).flags(BAND),
            Block::new([px1, 0.0, pz0 + t], [px1 + t, ph, pz1], Wall).paint(CANAL).flags(BAND),
            Block::new([px0, 0.0, 102.0], [px0 + 0.4, 20.0, 118.0], Door).centred(),
            Block::new([px0, 0.0, 101.4], [px0 + 0.5, 20.6, 102.0], Glow).paint(CANAL),
            Block::new([px0, 0.0, 118.0], [px0 + 0.5, 20.6, 118.6], Glow).paint(CANAL),
            Block::new([px0, 20.0, 102.0], [px0 + 0.5, 20.6, 118.0], Glow).paint(CANAL),
            Block::new([px0, 0.0, 102.0], [px0 + 1.2, 0.02, 118.0], Hazard),
            Block::new([px1 - 0.4, 0.0, 106.0], [px1, 12.0, 114.0], Door).centred(),
            Block::new([px1 - 0.5, 0.0, 105.4], [px1, 12.6, 106.0], Glow).paint(GARDENS),
            Block::new([px1 - 0.5, 0.0, 114.0], [px1, 12.6, 114.6], Glow).paint(GARDENS),
            Block::new([px1 - 0.5, 12.0, 106.0], [px1, 12.6, 114.0], Glow).paint(GARDENS),
        ]);

        Self { blocks: b, signs: signs() }
    }

    /// The deck's solids as the renderer's vertex arrays (`r` the surface, `g` the paint, `b` the
    /// flags, `a` a seed; UVs metres across the face: on an axis-aligned face the deck's own
    /// coordinates in its plane, so panels line up from one block to the next).
    pub fn mesh(&self) -> CityMesh {
        let mut m = CityMesh::default();
        for (i, block) in self.blocks.iter().enumerate() {
            let seed = ((i * 97 + 13) % 251) as f32 / 255.0;
            let colour = [
                f32::from(block.surface as u8) / 255.0,
                f32::from(block.paint) / 255.0,
                f32::from(block.flags) / 255.0,
                seed,
            ];
            let mid = block.centre();
            for (p, e1, e2) in block.faces() {
                let n = e1.cross(e2).normalize();
                let corners = [p, p + e1, p + e1 + e2, p + e2];
                let base = m.positions.len() as u32;
                for q in corners {
                    m.positions.push(q.to_array());
                    m.normals.push(n.to_array());
                    m.uvs.push(face_uv(q, n, p, e1, e2, block.centred.then_some(mid)));
                    m.colors.push(colour);
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        m
    }

    /// The rim's glass, one pane down the deck's length from the rim's wall to the trough's foot,
    /// facing into the hall (UVs metres across and along it).
    pub fn glass(&self) -> CityMesh {
        let (w, l) = (HALF_WIDTH, HALF_LENGTH);
        let top = Vec3::new(-w, 0.0, -l);
        let down = Vec3::new(RIM_X + w, TROUGH, 0.0);
        let along = Vec3::Z * 2.0 * l;
        let n = glass_normal();
        let corners = [top, top + along, top + along + down, top + down];
        CityMesh {
            positions: corners.iter().map(|q| q.to_array()).collect(),
            normals: vec![n.to_array(); 4],
            uvs: vec![[0.0, 0.0], [2.0 * l, 0.0], [2.0 * l, down.length()], [0.0, down.length()]],
            colors: vec![[1.0; 4]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }
}

/// The way the rim's glass faces, into the hall (up, and in from the rim): out through it is down
/// and aft, where Earth passes under the ring as it turns.
pub fn glass_normal() -> Vec3 {
    Vec3::new(-TROUGH, RIM_X + HALF_WIDTH, 0.0).normalize()
}

/// A face's UVs at `q`: on an axis-aligned face the deck's coordinates in its plane (along the deck
/// or across it, then up; on a floor or a ceiling, along then across), measured from `centre` if
/// the block's are its own; on any other face, metres along its edges.
fn face_uv(q: Vec3, n: Vec3, p: Vec3, e1: Vec3, e2: Vec3, centre: Option<Vec3>) -> [f32; 2] {
    let r = q - centre.unwrap_or(Vec3::ZERO);
    if n.x.abs() > 0.999 {
        [if centre.is_some() { r.z } else { q.z }, q.y]
    } else if n.y.abs() > 0.999 {
        [q.z, q.x]
    } else if n.z.abs() > 0.999 {
        [if centre.is_some() { r.x } else { q.x }, q.y]
    } else {
        let d = q - p;
        [d.dot(e1.normalize()), d.dot(e2.normalize())]
    }
}

/// The deck's signs: the bays' numbers on their doors, the deck's name on the rim's wall, the ends'
/// names in the band, the passage's on its back wall (the lifts' with the three lines', each in
/// its colour), the floor's KEEP CLEAR before each bay, the control room's.
fn signs() -> Vec<Sign> {
    let (w, l) = (HALF_WIDTH, HALF_LENGTH);
    let band = (BAND_Y0 + BAND_Y1) * 0.5;
    let sign = |text, at: [f32; 3], normal: Vec3, up: Vec3, height, ink| Sign {
        text,
        at: Vec3::from(at),
        normal,
        up,
        height,
        ink,
    };
    let mut s = vec![
        sign("RING DECK 2", [-w, 21.5, -20.0], Vec3::X, Vec3::Y, 5.5, CHARTER),
        sign("SECTION 14", [-w, 17.4, -20.0], Vec3::X, Vec3::Y, 2.0, DARK),
        sign("LAUNCH TUNNELS", [6.0, band, -l], Vec3::Z, Vec3::Y, 2.4, WHITE),
        sign("CAP LIFTS", [6.0, band, l], Vec3::NEG_Z, Vec3::Y, 2.4, WHITE),
        sign("SPOKE 2", [w, 15.0, 86.0], Vec3::NEG_X, Vec3::Y, 3.0, DARK),
        sign("CAP LIFTS", [-12.0, 18.5, PASSAGE_END], Vec3::NEG_Z, Vec3::Y, 3.6, CANAL),
        sign("CHARTER", [-12.0, 14.0, PASSAGE_END], Vec3::NEG_Z, Vec3::Y, 1.8, CHARTER),
        sign("CANAL", [-12.0, 11.8, PASSAGE_END], Vec3::NEG_Z, Vec3::Y, 1.8, CANAL),
        sign("GARDENS", [-12.0, 9.6, PASSAGE_END], Vec3::NEG_Z, Vec3::Y, 1.8, GARDENS),
        sign("AIRLOCK 4", [24.0, 16.0, PASSAGE_END], Vec3::NEG_Z, Vec3::Y, 3.0, DARK),
        sign("CONTROL", [-w + 6.0, CATWALK_Y + 5.3, -79.0], Vec3::X, Vec3::Y, 0.8, DARK),
    ];
    for (k, zc) in DOOR_Z.into_iter().enumerate() {
        s.push(sign(BAY_NAMES[k], [w + 1.0, 18.0, zc], Vec3::NEG_X, Vec3::Y, 5.0, DARK));
        s.push(sign("KEEP CLEAR", [w - 3.4, 0.0, zc], Vec3::Y, Vec3::X, 2.2, YELLOW));
    }
    s
}

const BAY_NAMES: [&str; 3] = ["BAY 07", "BAY 08", "BAY 09"];

/// Where the deck's suits are: one standing before bay 07's door (its torso; it faces the hall,
/// −x), and one flying the lanes on a loop ([`flyer`]).
pub const STANDING: Vec3 = Vec3::new(17.0, 9.6, -50.0);
/// How long the flying suit's loop takes, s, how high it flies (its torso), and its turns' radius.
pub const LOOP_SECS: f32 = 40.0;
const FLY_Y: f32 = 12.0;
const FLY_ENDS: f32 = 70.0;

/// The flying suit at `t` s: where (its torso, in the deck) and how fast. Down the first lane
/// towards the lifts, round, and back up the second, rising and falling a little as it goes.
pub fn flyer(t: f32) -> (Vec3, Vec3) {
    let r = (LANE_X1 - LANE_X0) * 0.5;
    let cx = (LANE_X0 + LANE_X1) * 0.5;
    let straight = 2.0 * FLY_ENDS;
    let turn = std::f32::consts::PI * r;
    let lap = 2.0 * (straight + turn);
    let speed = lap / LOOP_SECS;
    let s = (t * speed).rem_euclid(lap);
    let (x, z, dir) = if s < straight {
        (LANE_X0, -FLY_ENDS + s, Vec3::Z)
    } else if s < straight + turn {
        let a = (s - straight) / r;
        (cx - r * a.cos(), FLY_ENDS + r * a.sin(), Vec3::new(a.sin(), 0.0, a.cos()))
    } else if s < 2.0 * straight + turn {
        (LANE_X1, FLY_ENDS - (s - straight - turn), Vec3::NEG_Z)
    } else {
        let a = (s - 2.0 * straight - turn) / r;
        (cx + r * a.cos(), -FLY_ENDS - r * a.sin(), Vec3::new(-a.sin(), 0.0, -a.cos()))
    };
    let bob = std::f32::consts::TAU / LOOP_SECS * 3.0;
    let y = FLY_Y + 1.5 * (t * bob).sin();
    (Vec3::new(x, y, z), dir * speed + Vec3::Y * 1.5 * bob * (t * bob).cos())
}

/// The showcase's cameras, in the deck: an eye and what it looks at.
pub const CAMS: [(Vec3, Vec3); 4] = [
    // Down the deck from the blast door, at a suit's eye.
    (Vec3::new(4.0, 16.0, -90.0), Vec3::new(4.0, 9.0, 40.0)),
    // Low at the standing suit's feet, up at its bay's door and its number.
    (Vec3::new(6.0, 1.7, -32.0), Vec3::new(24.0, 15.0, -52.0)),
    // Over the rim's rail at a suit's eye, down through the glass at Earth (under it a few
    // seconds in).
    (Vec3::new(-6.5, 7.0, 10.0), Vec3::new(-13.5, -10.0, 18.0)),
    // By a pillar, towards the opening and the lifts.
    (Vec3::new(15.0, 4.0, 56.0), Vec3::new(6.0, 12.0, 120.0)),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgsl;

    const SHADER: &str = include_str!("../../bc-client/src/shaders/station.wgsl");
    const FACADE: &str = include_str!("../../bc-client/src/shaders/city_facade.wgsl");

    fn triangles(m: &CityMesh) -> impl Iterator<Item = [usize; 3]> + '_ {
        m.indices.chunks(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
    }

    /// Every face is wound the way its normal points (front faces are counter-clockwise), and its
    /// UVs are metres: a face's triangles keep their lengths in UV.
    #[test]
    fn faces_point_where_they_face_and_their_uvs_are_metres() {
        for m in [Layout::new().mesh(), Layout::new().glass()] {
            for [i, j, k] in triangles(&m) {
                let p = |v: usize| Vec3::from(m.positions[v]);
                let uv = |v: usize| glam::Vec2::from(m.uvs[v]);
                let n = Vec3::from(m.normals[i]);
                assert!((n.length() - 1.0).abs() < 1e-4);
                assert!((p(j) - p(i)).cross(p(k) - p(i)).dot(n) > 0.0, "a face wound backwards at {}", p(i));
                for (a, b) in [(i, j), (j, k), (k, i)] {
                    let (d3, d2) = (p(a).distance(p(b)), uv(a).distance(uv(b)));
                    assert!((d3 - d2).abs() < 1e-3 * d3.max(1.0), "{} → {}: {d3} m, {d2} in UV", p(a), p(b));
                }
            }
        }
    }

    /// A ray from `from` along `dir`: the nearest face it meets, and that face's normal.
    fn hit(m: &CityMesh, from: Vec3, dir: Vec3) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        for [i, j, k] in triangles(m) {
            let (a, b, c) =
                (Vec3::from(m.positions[i]), Vec3::from(m.positions[j]), Vec3::from(m.positions[k]));
            let (e1, e2) = (b - a, c - a);
            let pv = dir.cross(e2);
            let det = e1.dot(pv);
            if det.abs() < 1e-9 {
                continue;
            }
            let tv = from - a;
            let u = tv.dot(pv) / det;
            let qv = tv.cross(e1);
            let v = dir.dot(qv) / det;
            let t = e2.dot(qv) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 1e-4 && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, Vec3::from(m.normals[i])));
            }
        }
        best
    }

    /// From inside the deck and the passage, every way out meets a face turned towards it: the
    /// hall is closed (but for the rim's glass), and its walls show their painted sides.
    #[test]
    fn the_hall_is_closed_and_shows_its_inside() {
        let layout = Layout::new();
        let mut m = layout.mesh();
        let g = layout.glass();
        let base = m.positions.len() as u32;
        m.positions.extend(g.positions);
        m.normals.extend(g.normals);
        m.uvs.extend(g.uvs);
        m.colors.extend(g.colors);
        m.indices.extend(g.indices.iter().map(|i| i + base));
        let dirs = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
        let spots = [
            Vec3::new(3.0, 20.0, 10.0),
            Vec3::new(0.0, 5.0, -60.0),
            Vec3::new(-18.0, 6.0, 30.0),
            Vec3::new(10.0, 12.0, 110.0),
        ];
        for from in spots {
            for dir in dirs.iter().chain([Vec3::new(-0.6, -0.8, 0.0), Vec3::new(0.3, 0.17, 0.9)].iter()) {
                let dir = dir.normalize();
                let (t, n) =
                    hit(&m, from, dir).unwrap_or_else(|| panic!("from {from} along {dir} meets nothing"));
                assert!(n.dot(dir) < 0.0, "from {from} along {dir}, the face {t:.1} m off turns away");
            }
        }
    }

    /// Every sign is on a face that looks its way, within the face, and stencilled in letters
    /// the atlas draws.
    #[test]
    fn signs_are_on_the_faces_they_name() {
        let layout = Layout::new();
        for s in &layout.signs {
            assert!(s.normal.dot(s.up).abs() < 1e-6 && (s.normal.length() - 1.0).abs() < 1e-6);
            assert!(s.text.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || " ·".contains(c)));
            assert!(!s.text.is_empty() && s.text.chars().count() <= 16, "{}", s.text);
            let on = layout.blocks.iter().any(|b| {
                b.faces().iter().any(|&(p, e1, e2)| {
                    let n = e1.cross(e2).normalize();
                    let d = s.at - p;
                    n.dot(s.normal) > 0.999
                        && d.dot(n).abs() < 1e-3
                        && (0.0..=e1.length()).contains(&d.dot(e1.normalize()))
                        && (0.0..=e2.length()).contains(&d.dot(e2.normalize()))
                })
            });
            assert!(on, "{} at {} is on no face looking {}", s.text, s.at, s.normal);
        }
    }

    fn overlaps(lo: Vec3, hi: Vec3, b: &Block) -> bool {
        let (blo, bhi) = b.bounds();
        lo.cmplt(bhi).all() && hi.cmpgt(blo).all()
    }

    /// The suits stand and fly clear of everything (a suit's box: 8 m across, 18 m tall).
    #[test]
    fn the_suits_stand_and_fly_clear() {
        let layout = Layout::new();
        let suit = |at: Vec3| (at + Vec3::new(-4.0, -9.6, -4.0), at + Vec3::new(4.0, 8.4, 4.0));
        let (lo, hi) = suit(STANDING);
        assert!(lo.y >= 0.0 - 1e-3);
        for b in &layout.blocks {
            assert!(!overlaps(lo + Vec3::Y * 0.05, hi, b), "the standing suit is in {b:?}");
        }
        for k in 0..400 {
            let t = k as f32 * LOOP_SECS / 400.0;
            let (at, vel) = flyer(t);
            let (lo, hi) = suit(at);
            for b in &layout.blocks {
                assert!(!overlaps(lo, hi, b), "the flyer at {t} s ({at}) is in {b:?}");
            }
            let (ahead, _) = flyer(t + 0.01);
            assert!((ahead - at).distance(vel * 0.01) < 0.02, "its velocity at {t} s");
            let (lo2, hi2) = (lo - Vec3::new(4.0, 0.0, 4.0), hi + Vec3::new(4.0, 0.0, 4.0));
            assert!(
                !(lo2.cmplt(STANDING).all() && hi2.cmpgt(STANDING).all()),
                "it flies into the other at {t} s"
            );
        }
    }

    /// The cameras are inside, clear of everything.
    #[test]
    fn the_cameras_are_inside() {
        let layout = Layout::new();
        for (eye, _) in CAMS {
            for b in &layout.blocks {
                assert!(!overlaps(eye - Vec3::splat(0.6), eye + Vec3::splat(0.6), b), "{eye} is in {b:?}");
            }
        }
    }

    #[test]
    fn the_shaders_numbers_are_the_decks() {
        let c = |name| wgsl::constant(SHADER, name);
        for (name, v) in [
            ("PANEL_W", PANEL_W),
            ("PANEL_H", PANEL_H),
            ("PLATE", PLATE),
            ("KICK", KICK),
            ("BAND_Y0", BAND_Y0),
            ("BAND_Y1", BAND_Y1),
            ("ARROW_EVERY", ARROW_EVERY),
            ("LANE_X0", LANE_X0),
            ("LANE_X1", LANE_X1),
            ("LANE_HALF", LANE_HALF),
            ("CHEVRON_EVERY", CHEVRON_EVERY),
            ("STRIP_EVERY", STRIP_EVERY),
            ("HAZARD_PERIOD", HAZARD_PERIOD),
            ("COLLAR_Y0", COLLAR_Y0),
            ("COLLAR_Y1", COLLAR_Y1),
        ] {
            assert_eq!(c(name), v, "{name}");
        }
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
        assert_eq!(c("YELLOW"), f32::from(YELLOW));
        assert_eq!(c("DARK"), f32::from(DARK));
    }

    /// The lines' colours are the city's: Charter blue, Canal teal, Gardens green.
    #[test]
    fn the_lines_colours_are_the_citys() {
        let body: String = FACADE
            .split("fn f_line(")
            .nth(1)
            .and_then(|s| s.split("\n}").next())
            .expect("f_line in city_facade.wgsl")
            .to_string();
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
    /// surface branch.
    #[test]
    fn the_station_shader_takes_its_derivatives_at_the_top() {
        let lines: Vec<&str> = SHADER.lines().map(|l| l.split("//").next().unwrap_or("")).collect();
        let derivative = |l: &str| ["dpdx", "dpdy", "fwidth"].iter().any(|d| l.contains(d));
        let first_branch = lines.iter().position(|l| l.trim_start().starts_with("if (surface")).unwrap();
        let start = lines.iter().position(|l| l.contains("fn fragment(")).unwrap();
        for (k, l) in lines.iter().enumerate() {
            assert!(!derivative(l) || (start..first_branch).contains(&k), "station.wgsl line {}: {l}", k + 1);
        }
    }
}
