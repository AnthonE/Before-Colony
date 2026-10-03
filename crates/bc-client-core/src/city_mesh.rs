//! Meshes of the city, built from its rules (`bc_sim::colony::city`), so what's drawn is what's
//! walked: every building is its boxes, bent onto the cylinder (a plumb wall is a radial plane; a
//! roof and a wall facing along the axis curve with the floor, cut every 24 m across).
//!
//! The city is drawn in chunks of blocks at four levels of detail, coarser further off:
//! - L0, 2 × 2 blocks: every building whole, the pavements' kerbs, trees, the canal's walls and
//!   water, railings, the site's frames and cranes.
//! - L1, 4 × 4: the buildings and the trees.
//! - L2, 8 × 8: one box a lot.
//! - L3, 16 × 16: a block's bulk, and its tallest building where it stands out.
//!
//! The ground under it all (streets, parks, the avenue's median, painted from the block atlas) is
//! one mesh a stretch of each strip ([`ground`]): it needs no levels, being flat.
//!
//! Positions are relative to the chunk's anchor (its middle, on the floor) in the colony's own
//! frame, so they keep their precision anywhere in the 32 km; the renderer places the anchor.
//! Vertex colours carry what the city shader paints: `r` the surface ([`Surface`]), `g` a seed,
//! `b` ambient occlusion, `a` the building's height fraction of the tallest (for the facade).
//! UVs: on walls, metres along the wall and up from the floor; on roofs and the ground, `s` and `x`.

use bc_sim::colony::city::{
    BLOCK, BlockKind, Building, CANAL_DEPTH, CityBox, DOOR_HEIGHT, DOOR_WIDTH, GRID_X0, KERB, MAX_HEIGHT,
    MAX_SOLIDS, RAILING, ROWS, Rect, Room, Stage, Style, WALL, block, channel, lots, mix, unit,
};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, strip_edge};
use bc_sim::content::city::{PLACES, PlaceKind};
use bc_sim::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};
use glam::Vec3;

/// The finest a curved face is cut across, m.
const CUT: f32 = 24.0;

/// What a vertex is, for the city shader (`r` of its colour, in 255ths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    Ground = 0,
    Wall = 1,
    Roof = 2,
    Pavement = 3,
    Kerb = 4,
    Water = 5,
    CanalWall = 6,
    Tree = 7,
    Trunk = 8,
    Steel = 9,
    Railing = 10,
    Hall = 11,
    Glass = 12,
    /// An end cap's inner face.
    Cap = 13,
    /// Inside a key place's room (lit indoors, `bc_sim::colony::city::Room`): its walls, its floor,
    /// its ceiling (lamps in it), its counter, and its back wall, which shows what the place is
    /// (the seed: 1 the bar's shelves, 2 the Exchange's boards, 3 the Charter Board's notices).
    Interior = 14,
    Floor = 15,
    Ceiling = 16,
    Counter = 17,
    Display = 18,
}

/// A chunk's mesh: the renderer's vertex arrays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl CityMesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// A chunk of the city: strip, level of detail, and where (`i` along in chunks from block 0, `j`
/// across in chunks from the strip's edge row).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkKey {
    pub strip: u8,
    pub lod: u8,
    pub i: i32,
    pub j: i32,
}

/// The row counted from the strip's edge, 0..24, and back.
pub fn row_index(row: i32) -> i32 {
    if row < 0 { row + ROWS } else { row - 1 + ROWS }
}

pub fn row_of_index(ri: i32) -> i32 {
    if ri < ROWS { ri - ROWS } else { ri - ROWS + 1 }
}

impl ChunkKey {
    /// Blocks a side.
    pub fn size(&self) -> i32 {
        2 << self.lod
    }

    /// Its blocks along (`bx`), and rows across (row indices, 0..24).
    pub fn blocks(&self) -> ((i32, i32), (i32, i32)) {
        let n = self.size();
        ((self.i * n, self.i * n + n), (self.j * n, (self.j * n + n).min(2 * ROWS)))
    }

    /// The chunk holding block `bx`, row `row` at level `lod`.
    pub fn of(strip: u8, lod: u8, bx: i32, row: i32) -> Self {
        let n = 2 << lod;
        Self { strip, lod, i: bx.div_euclid(n), j: row_index(row).div_euclid(n) }
    }

    /// What it covers on its strip.
    pub fn rect(&self) -> Rect {
        let ((b0, b1), (r0, r1)) = self.blocks();
        let (lo, _) = bc_sim::colony::city::row_span(row_of_index(r0));
        let (_, hi) = bc_sim::colony::city::row_span(row_of_index(r1 - 1));
        Rect::new(lo, hi, GRID_X0 + BLOCK * b0 as f32, GRID_X0 + BLOCK * b1 as f32)
    }

    /// Its middle on the floor: where its mesh's positions are measured from.
    pub fn anchor(&self) -> CityPos {
        let r = self.rect();
        let (s, x) = r.middle();
        CityPos::new(self.strip, x, s, 0.0)
    }
}

/// Builds meshes in a chunk's own frame.
struct Builder {
    mesh: CityMesh,
    strip: u8,
    origin: Vec3,
}

impl Builder {
    fn new(strip: u8, anchor: CityPos) -> Self {
        Self { mesh: CityMesh::default(), strip, origin: anchor.to_colony() }
    }

    fn at(&self, s: f32, x: f32, h: f32) -> Vec3 {
        CityPos::new(self.strip, x, s, h).to_colony() - self.origin
    }

    /// Up (towards the axis) where `s` is.
    fn up(&self, s: f32) -> Vec3 {
        let a = strip_edge(self.strip as usize) + s / COLONY_RADIUS;
        Vec3::new(0.0, -a.cos(), -a.sin())
    }

    /// The way `s` grows where it is.
    fn across(&self, s: f32) -> Vec3 {
        let a = strip_edge(self.strip as usize) + s / COLONY_RADIUS;
        Vec3::new(0.0, -a.sin(), a.cos())
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2], color: [f32; 4]) -> u32 {
        let i = self.mesh.positions.len() as u32;
        self.mesh.positions.push(p.to_array());
        self.mesh.normals.push(n.to_array());
        self.mesh.uvs.push(uv);
        self.mesh.colors.push(color);
        i
    }

    /// A quad a, b, c, d (round its edge), wound to face `n`.
    fn quad(&mut self, v: [u32; 4], n: Vec3) {
        let p = |i: u32| Vec3::from_array(self.mesh.positions[i as usize]);
        let g = (p(v[1]) - p(v[0])).cross(p(v[2]) - p(v[0]));
        if g.dot(n) >= 0.0 {
            self.mesh.indices.extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
        } else {
            self.mesh.indices.extend_from_slice(&[v[0], v[2], v[1], v[0], v[3], v[2]]);
        }
    }

    /// Cuts of `[a, b]` at most `CUT` long (or one, if `whole`).
    fn cuts(a: f32, b: f32, whole: bool) -> u32 {
        if whole { 1 } else { (((b - a) / CUT).ceil() as u32).max(1) }
    }

    /// A box's sides and top (no bottom), bent onto the cylinder unless `flat`.
    fn city_box(&mut self, b: &CityBox, surface: Surface, roof: Surface, seed: f32, tall: f32, flat: bool) {
        let r = b.rect;
        let (h0, h1) = (b.h0, b.h1);
        let ao = |h: f32| if h - h0 < 3.0 { 0.62 } else { 1.0 };
        let col = |s: Surface, h: f32| [s as u8 as f32 / 255.0, seed, ao(h), tall];
        let n = Self::cuts(r.s0, r.s1, flat);
        // The top: a curved strip, cut across.
        let mut row0 = Vec::with_capacity(n as usize + 1);
        let mut row1 = Vec::with_capacity(n as usize + 1);
        for i in 0..=n {
            let s = r.s0 + r.width() * i as f32 / n as f32;
            let up = self.up(s);
            row0.push(self.vertex(self.at(s, r.x0, h1), up, [s, r.x0], col(roof, h1 + 9.0)));
            row1.push(self.vertex(self.at(s, r.x1, h1), up, [s, r.x1], col(roof, h1 + 9.0)));
        }
        for i in 0..n as usize {
            let up = self.up(r.s0 + r.width() * (i as f32 + 0.5) / n as f32);
            self.quad([row0[i], row0[i + 1], row1[i + 1], row1[i]], up);
        }
        // The walls facing along the axis (−X at x0, +X at x1): flat in x, curved across.
        for (x, nx) in [(r.x0, -Vec3::X), (r.x1, Vec3::X)] {
            let mut lo = Vec::with_capacity(n as usize + 1);
            let mut hi = Vec::with_capacity(n as usize + 1);
            for i in 0..=n {
                let s = r.s0 + r.width() * i as f32 / n as f32;
                let u = s - r.s0;
                lo.push(self.vertex(self.at(s, x, h0), nx, [u, h0], col(surface, h0)));
                hi.push(self.vertex(self.at(s, x, h1), nx, [u, h1], col(surface, h1)));
            }
            for i in 0..n as usize {
                self.quad([lo[i], lo[i + 1], hi[i + 1], hi[i]], nx);
            }
        }
        // The walls facing across (−s at s0, +s at s1): plumb, so plane.
        for (s, sign) in [(r.s0, -1.0f32), (r.s1, 1.0)] {
            let nn = self.across(s) * sign;
            let v = [
                self.vertex(self.at(s, r.x0, h0), nn, [0.0, h0], col(surface, h0)),
                self.vertex(self.at(s, r.x1, h0), nn, [r.length(), h0], col(surface, h0)),
                self.vertex(self.at(s, r.x1, h1), nn, [r.length(), h1], col(surface, h1)),
                self.vertex(self.at(s, r.x0, h1), nn, [0.0, h1], col(surface, h1)),
            ];
            self.quad(v, nn);
        }
    }

    /// A wall at `x` along, from `s0` to `s1` across and `h0` to `h1` up, facing ±X (`nx`): flat in
    /// x, curved across. UVs: metres across from `u0`, and up.
    #[allow(clippy::too_many_arguments)]
    fn wall_x(
        &mut self,
        x: f32,
        s0: f32,
        s1: f32,
        h0: f32,
        h1: f32,
        nx: f32,
        u0: f32,
        col: impl Fn(f32) -> [f32; 4],
    ) {
        let n = Self::cuts(s0.min(s1), s0.max(s1), false);
        let normal = Vec3::X * nx;
        let (mut lo, mut hi) = (Vec::with_capacity(n as usize + 1), Vec::with_capacity(n as usize + 1));
        for i in 0..=n {
            let s = s0 + (s1 - s0) * i as f32 / n as f32;
            let u = (s - u0).abs();
            lo.push(self.vertex(self.at(s, x, h0), normal, [u, h0], col(h0)));
            hi.push(self.vertex(self.at(s, x, h1), normal, [u, h1], col(h1)));
        }
        for i in 0..n as usize {
            self.quad([lo[i], lo[i + 1], hi[i + 1], hi[i]], normal);
        }
    }

    /// A wall at `s` across, from `x0` to `x1` along and `h0` to `h1` up, facing ±s (`ns`): plumb,
    /// so plane. UVs: metres along from `u0`, and up.
    #[allow(clippy::too_many_arguments)]
    fn wall_s(
        &mut self,
        s: f32,
        x0: f32,
        x1: f32,
        h0: f32,
        h1: f32,
        ns: f32,
        u0: f32,
        col: impl Fn(f32) -> [f32; 4],
    ) {
        let normal = self.across(s) * ns;
        let (ua, ub) = ((x0 - u0).abs(), (x1 - u0).abs());
        let v = [
            self.vertex(self.at(s, x0, h0), normal, [ua, h0], col(h0)),
            self.vertex(self.at(s, x1, h0), normal, [ub, h0], col(h0)),
            self.vertex(self.at(s, x1, h1), normal, [ub, h1], col(h1)),
            self.vertex(self.at(s, x0, h1), normal, [ua, h1], col(h1)),
        ];
        self.quad(v, normal);
    }

    /// A level face over `r` at `h`, facing up (or down, `down`): curved across. UVs: `s`, `x`.
    fn level(&mut self, r: &Rect, h: f32, down: bool, col: [f32; 4]) {
        let n = Self::cuts(r.s0, r.s1, false);
        let sign = if down { -1.0 } else { 1.0 };
        let (mut a, mut b) = (Vec::with_capacity(n as usize + 1), Vec::with_capacity(n as usize + 1));
        for i in 0..=n {
            let s = r.s0 + r.width() * i as f32 / n as f32;
            let up = self.up(s) * sign;
            a.push(self.vertex(self.at(s, r.x0, h), up, [s, r.x0], col));
            b.push(self.vertex(self.at(s, r.x1, h), up, [s, r.x1], col));
        }
        for i in 0..n as usize {
            let up = self.up(r.s0 + r.width() * (i as f32 + 0.5) / n as f32) * sign;
            self.quad([a[i], a[i + 1], b[i + 1], b[i]], up);
        }
    }

    /// A tree: a trunk and a crown (an octahedron), standing at `(s, x)` on `h`.
    fn tree(&mut self, s: f32, x: f32, h: f32, size: f32, seed: f32) {
        let trunk =
            CityBox { rect: Rect::new(s - 0.2, s + 0.2, x - 0.2, x + 0.2), h0: h, h1: h + size * 0.45 };
        self.city_box(&trunk, Surface::Trunk, Surface::Trunk, seed, 0.0, true);
        let c = self.at(s, x, h + size * 0.95);
        let up = self.up(s);
        let across = self.across(s);
        let r = size * 0.55;
        let tips = [up * r * 1.1, -up * r * 0.8, Vec3::X * r, -Vec3::X * r, across * r, -across * r];
        let col = [Surface::Tree as u8 as f32 / 255.0, seed, 1.0, 0.0];
        let ring = [2usize, 4, 3, 5];
        for k in 0..4 {
            let (a, b) = (tips[ring[k]], tips[ring[(k + 1) % 4]]);
            for pole in [0usize, 1] {
                let t = tips[pole];
                let n = (a + b + t).normalize();
                let v0 = self.vertex(c + t, n, [s, x], col);
                let v1 = self.vertex(c + a, n, [s, x], col);
                let v2 = self.vertex(c + b, n, [s, x], col);
                let p = |v: Vec3| v;
                let g = (p(a) - p(t)).cross(p(b) - p(t));
                if g.dot(n) >= 0.0 {
                    self.mesh.indices.extend_from_slice(&[v0, v1, v2]);
                } else {
                    self.mesh.indices.extend_from_slice(&[v0, v2, v1]);
                }
            }
        }
    }
}

/// What a building looks like from its style.
fn faces(b: &Building) -> (Surface, Surface) {
    match b.style {
        Style::Frame | Style::Crane => (Surface::Steel, Surface::Steel),
        Style::Hall => (Surface::Hall, Surface::Roof),
        Style::Tower => (Surface::Glass, Surface::Roof),
        _ => (Surface::Wall, Surface::Roof),
    }
}

/// A key place's hall with the room behind its door (`bc_sim::colony::city::Room`), close up: its
/// outside, its front cut for the door and the door's reveal through the wall; inside, the room's
/// walls, the back wall showing what the place is, the ceiling with its lamps, the floor, and the
/// counter.
fn hall_with_room(m: &mut Builder, b: &Building, room: &Room, seed: f32, tall: f32) {
    let f = room.front;
    let foot = b.foot;
    // The hall's extent along its front, about its middle, and how deep it runs in.
    let (u0, u1, depth) = if f.along_s {
        (foot.s0 - f.middle, foot.s1 - f.middle, foot.length())
    } else {
        (foot.x0 - f.middle, foot.x1 - f.middle, foot.width())
    };
    let (top, c) = (b.height, room.ceiling);
    let (w, d) = (room.width() * 0.5, room.depth());
    let door = DOOR_WIDTH * 0.5;
    let lintel = KERB + DOOR_HEIGHT;
    let kind = match PLACES[usize::from(room.place)].kind {
        PlaceKind::Bar => 1.0,
        PlaceKind::Exchange => 2.0,
        PlaceKind::Charter => 3.0,
        PlaceKind::HubGate => 0.0,
    } / 255.0;
    let col = |surface: Surface, seed: f32, floor: f32| {
        move |h: f32| [surface as u8 as f32 / 255.0, seed, if h - floor < 3.0 { 0.62 } else { 1.0 }, tall]
    };
    let hall = col(Surface::Hall, seed, KERB);
    let inside = col(Surface::Interior, kind, KERB);
    let display = col(Surface::Display, kind, KERB);
    // A face square to the way in, `v` in from the front, `ua..ub` along it: facing in (`inward`),
    // or out of the hall toward the front.
    let vface = |m: &mut Builder,
                 v: f32,
                 (ua, ub): (f32, f32),
                 (h0, h1): (f32, f32),
                 inward: bool,
                 colour: &dyn Fn(f32) -> [f32; 4]| {
        let at = f.face + f.sign * v;
        let n = if inward { f.sign } else { -f.sign };
        if f.along_s {
            m.wall_x(at, f.middle + ua, f.middle + ub, h0, h1, n, f.middle + u0, colour);
        } else {
            m.wall_s(at, f.middle + ua, f.middle + ub, h0, h1, n, f.middle + u0, colour);
        }
    };
    // A face along the way in, `u` along the front, `va..vb` in: facing +along (`plus`) or −.
    let uface = |m: &mut Builder,
                 u: f32,
                 (va, vb): (f32, f32),
                 (h0, h1): (f32, f32),
                 plus: bool,
                 colour: &dyn Fn(f32) -> [f32; 4]| {
        let at = f.middle + u;
        let (a, b) = (f.face + f.sign * va, f.face + f.sign * vb);
        let n = if plus { 1.0 } else { -1.0 };
        if f.along_s {
            m.wall_s(at, a.min(b), a.max(b), h0, h1, n, a.min(b), colour);
        } else {
            m.wall_x(at, a.min(b), a.max(b), h0, h1, n, a.min(b), colour);
        }
    };
    // Outside: the roof, the back and the ends, and the front round the door.
    m.level(&foot, top, false, [Surface::Roof as u8 as f32 / 255.0, seed, 1.0, tall]);
    vface(m, depth, (u0, u1), (KERB, top), true, &hall);
    uface(m, u0, (0.0, depth), (KERB, top), false, &hall);
    uface(m, u1, (0.0, depth), (KERB, top), true, &hall);
    vface(m, 0.0, (u0, -door), (KERB, top), false, &hall);
    vface(m, 0.0, (door, u1), (KERB, top), false, &hall);
    vface(m, 0.0, (-door, door), (lintel, top), false, &hall);
    // The door's reveal, through the front wall.
    uface(m, -door, (0.0, WALL), (KERB, lintel), true, &inside);
    uface(m, door, (0.0, WALL), (KERB, lintel), false, &inside);
    m.level(&f.rect(-door, door, 0.0, WALL), lintel, true, inside(lintel));
    // Inside: the front wall round the door, the sides, the back wall's display.
    vface(m, WALL, (-w, -door), (KERB, c), true, &inside);
    vface(m, WALL, (door, w), (KERB, c), true, &inside);
    vface(m, WALL, (-door, door), (lintel, c), true, &inside);
    uface(m, -w, (WALL, d), (KERB, c), true, &inside);
    uface(m, w, (WALL, d), (KERB, c), false, &inside);
    vface(m, d, (-w, w), (KERB, c), false, &display);
    // Its ceiling, its floor (just over the kerb's top, which is the street's pavement), its counter.
    m.level(&room.rect, c, true, [Surface::Ceiling as u8 as f32 / 255.0, kind, 1.0, tall]);
    m.level(&room.rect, KERB + 0.005, false, [Surface::Floor as u8 as f32 / 255.0, kind, 1.0, tall]);
    let counter = CityBox { rect: room.counter, h0: KERB, h1: KERB + bc_sim::colony::city::COUNTER_HEIGHT };
    m.city_box(&counter, Surface::Counter, Surface::Counter, kind, tall, true);
}

/// A site's frame: corner columns and a floor's beams every second floor.
fn frame(m: &mut Builder, b: &Building, seed: f32) {
    let f = b.foot;
    let c = 1.2;
    let mut boxes = [CityBox::default(); MAX_SOLIDS];
    let n = b.solids(&mut boxes);
    for bx in &boxes[..n] {
        m.city_box(bx, Surface::Steel, Surface::Steel, seed, 0.0, true);
    }
    let mut h = KERB + 7.2;
    while h < b.height - 1.0 {
        for r in [
            Rect::new(f.s0, f.s1, f.x0, f.x0 + c),
            Rect::new(f.s0, f.s1, f.x1 - c, f.x1),
            Rect::new(f.s0, f.s0 + c, f.x0, f.x1),
            Rect::new(f.s1 - c, f.s1, f.x0, f.x1),
        ] {
            m.city_box(
                &CityBox { rect: r, h0: h - 0.8, h1: h },
                Surface::Steel,
                Surface::Steel,
                seed,
                0.0,
                false,
            );
        }
        h += 7.2;
    }
}

/// A crane: its mast, and its jib along the axis at the top.
fn crane(m: &mut Builder, b: &Building, seed: f32) {
    m.city_box(
        &CityBox { rect: b.foot, h0: KERB, h1: b.height },
        Surface::Steel,
        Surface::Steel,
        seed,
        0.0,
        true,
    );
    let (s, x) = b.foot.middle();
    let jib = Rect::new(s - 1.0, s + 1.0, x - 18.0, x + 45.0);
    m.city_box(
        &CityBox { rect: jib, h0: b.height - 3.0, h1: b.height },
        Surface::Steel,
        Surface::Steel,
        seed,
        0.0,
        true,
    );
}

/// The mesh of chunk `key` at the site's `stage`.
pub fn chunk(key: ChunkKey, stage: Stage) -> CityMesh {
    let mut m = Builder::new(key.strip, key.anchor());
    let ((b0, b1), (r0, r1)) = key.blocks();
    let mut boxes = [CityBox::default(); MAX_SOLIDS];
    for bx in b0..b1 {
        for ri in r0..r1 {
            let row = row_of_index(ri);
            let Some(b) = block(key.strip, bx, row, stage) else { continue };
            let seed = (b.seed & 0xff) as f32 / 255.0;
            let all = lots(&b);
            let buildings = all.as_slice();
            match key.lod {
                0 | 1 => {
                    if key.lod == 0 {
                        kerb_and_canal(&mut m, &b.rect, b.kind, seed);
                    }
                    for bd in buildings {
                        let s = (bd.seed & 0xff) as f32 / 255.0;
                        let tall = bd.top() / MAX_HEIGHT;
                        match (bd.style, bd.room) {
                            (Style::Frame, _) if key.lod == 0 => frame(&mut m, bd, s),
                            (Style::Crane, _) => crane(&mut m, bd, s),
                            // A key place's hall: close up, its room inside; further off, its bulk.
                            (_, Some(room)) if key.lod == 0 => hall_with_room(&mut m, bd, &room, s, tall),
                            (_, Some(_)) => {
                                let (wall, roof) = faces(bd);
                                let cb = CityBox { rect: bd.foot, h0: KERB, h1: bd.height };
                                m.city_box(&cb, wall, roof, s, tall, false);
                            }
                            _ => {
                                let n = bd.solids(&mut boxes);
                                let (wall, roof) = faces(bd);
                                for (k, piece) in boxes[..n].iter().enumerate() {
                                    let face =
                                        if k == 0 && bd.style == Style::Tower { Surface::Wall } else { wall };
                                    m.city_box(piece, face, roof, s, tall, false);
                                }
                            }
                        }
                    }
                    trees(&mut m, &b.rect, b.kind, b.seed, key.lod);
                }
                2 => {
                    for bd in buildings {
                        let (wall, roof) = faces(bd);
                        let s = (bd.seed & 0xff) as f32 / 255.0;
                        let foot =
                            bd.tower.map_or(bd.foot, |(t, h)| if h > 2.0 * bd.height { t } else { bd.foot });
                        let cb = CityBox { rect: foot, h0: KERB, h1: bd.top() };
                        if bd.style == Style::Frame {
                            continue;
                        }
                        m.city_box(&cb, wall, roof, s, bd.top() / MAX_HEIGHT, false);
                    }
                }
                _ => {
                    // The block's bulk at its buildings' mean roof, and its tallest if it stands out.
                    let (mut area, mut volume, mut tallest) = (0.0f32, 0.0f32, None::<&Building>);
                    for bd in buildings.iter().filter(|b| b.style != Style::Frame && b.style != Style::Crane)
                    {
                        let a = bd.foot.width() * bd.foot.length();
                        area += a;
                        volume += a * bd.height;
                        if tallest.is_none_or(|t| bd.top() > t.top()) {
                            tallest = Some(bd);
                        }
                    }
                    if area > 0.0 {
                        let mean = volume / area;
                        let bulk = b.rect.inset(8.0);
                        m.city_box(
                            &CityBox { rect: bulk, h0: KERB, h1: mean },
                            Surface::Wall,
                            Surface::Roof,
                            seed,
                            0.0,
                            true,
                        );
                        if let Some(t) = tallest
                            && t.top() > mean * 1.6
                        {
                            let foot = t.tower.map_or(t.foot, |(r, _)| r);
                            m.city_box(
                                &CityBox { rect: foot, h0: mean, h1: t.top() },
                                Surface::Glass,
                                Surface::Roof,
                                seed,
                                t.top() / MAX_HEIGHT,
                                true,
                            );
                        }
                    }
                }
            }
        }
    }
    m.mesh
}

/// A block's kerb (it stands a step up from the street), and in the canal's row the channel's
/// walls, its water and the quays' railings.
fn kerb_and_canal(m: &mut Builder, r: &Rect, kind: BlockKind, seed: f32) {
    if kind != BlockKind::Canal {
        m.city_box(
            &CityBox { rect: *r, h0: -0.3, h1: KERB },
            Surface::Kerb,
            Surface::Pavement,
            seed,
            0.0,
            false,
        );
        return;
    }
    let ch = channel(r);
    for q in [Rect::new(r.s0, ch.s0, r.x0, r.x1), Rect::new(ch.s1, r.s1, r.x0, r.x1)] {
        m.city_box(
            &CityBox { rect: q, h0: -CANAL_DEPTH, h1: KERB },
            Surface::CanalWall,
            Surface::Pavement,
            seed,
            0.0,
            false,
        );
    }
    // The water, a metre below the street; the channel's ends under the bridges.
    let n = Builder::cuts(ch.s0, ch.s1, false);
    let mut a = Vec::new();
    let mut b = Vec::new();
    for i in 0..=n {
        let s = ch.s0 + ch.width() * i as f32 / n as f32;
        let up = m.up(s);
        a.push(m.vertex(
            m.at(s, r.x0 - 12.0, -1.0),
            up,
            [s, r.x0],
            [Surface::Water as u8 as f32 / 255.0, seed, 1.0, 0.0],
        ));
        b.push(m.vertex(
            m.at(s, r.x1 + 12.0, -1.0),
            up,
            [s, r.x1],
            [Surface::Water as u8 as f32 / 255.0, seed, 1.0, 0.0],
        ));
    }
    for i in 0..n as usize {
        let up = m.up(ch.s0 + ch.width() * (i as f32 + 0.5) / n as f32);
        m.quad([a[i], a[i + 1], b[i + 1], b[i]], up);
    }
    for rail in [Rect::new(ch.s0 - 0.3, ch.s0, r.x0, r.x1), Rect::new(ch.s1, ch.s1 + 0.3, r.x0, r.x1)] {
        m.city_box(
            &CityBox { rect: rail, h0: KERB, h1: KERB + RAILING },
            Surface::Railing,
            Surface::Railing,
            seed,
            0.0,
            true,
        );
    }
}

/// Trees in a park, and along the pavements of blocks with room.
fn trees(m: &mut Builder, r: &Rect, kind: BlockKind, seed: u32, lod: u8) {
    let count = match kind {
        BlockKind::Park => {
            if lod == 0 {
                36
            } else {
                14
            }
        }
        BlockKind::Plaza => 6,
        _ => 0,
    };
    let area = r.inset(6.0);
    for i in 0..count {
        let h = mix(seed, 31, i);
        let (s, x) = (area.s0 + area.width() * unit(h, 1), area.x0 + area.length() * unit(h, 2));
        m.tree(s, x, KERB, 7.0 + 6.0 * unit(h, 3), unit(h, 4));
    }
}

/// The ground of strip `strip` from block `b0` to `b1` (exclusive), flat on the floor across the
/// whole strip: cut every 32 m across and every block along. The shader paints it from the block
/// atlas and leaves out the canal's channel.
pub fn ground(strip: u8, b0: i32, b1: i32) -> (CityPos, CityMesh) {
    let x0 = (GRID_X0 + BLOCK * b0 as f32).max(-COLONY_HALF_LENGTH);
    let x1 = (GRID_X0 + BLOCK * b1 as f32).min(COLONY_HALF_LENGTH);
    let anchor = CityPos::new(strip, (x0 + x1) * 0.5, STRIP_WIDTH * 0.5, 0.0);
    let mut m = Builder::new(strip, anchor);
    let across = (STRIP_WIDTH / 32.0).ceil() as u32;
    let along = (((x1 - x0) / BLOCK).ceil() as u32).max(1);
    // The strip rides in the seed, for the shader's lookup in the block atlas.
    let col = [Surface::Ground as u8 as f32 / 255.0, strip as f32 / 255.0, 1.0, 0.0];
    for j in 0..=along {
        let x = x0 + (x1 - x0) * j as f32 / along as f32;
        for i in 0..=across {
            let s = STRIP_WIDTH * i as f32 / across as f32;
            let up = m.up(s);
            m.vertex(m.at(s, x, 0.0), up, [s, x], col);
        }
    }
    let w = across + 1;
    for j in 0..along {
        for i in 0..across {
            let a = j * w + i;
            let up = m.up(STRIP_WIDTH * (i as f32 + 0.5) / across as f32);
            m.quad([a, a + 1, a + 1 + w, a + w], up);
        }
    }
    (anchor, m.mesh)
}

/// An end cap's inner face (`side` −1 the docking hub's, +1 the far one), facing into the colony:
/// a disc in rings out to the hull. Positions are from its middle, on the axis.
pub fn cap(side: f32) -> (Vec3, CityMesh) {
    let x = side * COLONY_HALF_LENGTH;
    let centre = Vec3::new(x, 0.0, 0.0);
    let mut mesh = CityMesh::default();
    let (rings, segs) = (24u32, 192u32);
    let normal = Vec3::X * -side;
    let col = [Surface::Cap as u8 as f32 / 255.0, if side < 0.0 { 0.0 } else { 1.0 }, 1.0, 0.0];
    for j in 0..=rings {
        let r = COLONY_RADIUS * j as f32 / rings as f32;
        for i in 0..=segs {
            let a = std::f32::consts::TAU * i as f32 / segs as f32;
            mesh.positions.push([0.0, r * a.cos(), r * a.sin()]);
            mesh.normals.push(normal.to_array());
            mesh.uvs.push([r, a]);
            mesh.colors.push(col);
        }
    }
    let w = segs + 1;
    for j in 0..rings {
        for i in 0..segs {
            let a = j * w + i;
            let (b, c, d) = (a + 1, a + w, a + w + 1);
            let p = |k: u32| Vec3::from_array(mesh.positions[k as usize]);
            let g = (p(b) - p(a)).cross(p(d) - p(a));
            if g.dot(normal) >= 0.0 || g.length() < 1e-6 {
                mesh.indices.extend_from_slice(&[a, b, d, a, d, c]);
            } else {
                mesh.indices.extend_from_slice(&[a, d, b, a, c, d]);
            }
        }
    }
    (centre, mesh)
}

/// Tram station `i` on strip `strip`: its island platform and steps (`transit::platform_solids`),
/// and a canopy over the platform on its posts.
pub fn station(strip: u8, i: usize) -> (CityPos, CityMesh) {
    use bc_sim::colony::frame::STRIP_WIDTH;
    use bc_sim::colony::transit::{FLOOR, PLATFORM_HALF, PLATFORM_LENGTH, platform_solids, station_x};
    let (x, mid) = (station_x(i), STRIP_WIDTH * 0.5);
    let anchor = CityPos::new(strip, x, mid, 0.0);
    let mut m = Builder::new(strip, anchor);
    let area = Rect::new(mid - 1.0, mid + 1.0, x - PLATFORM_LENGTH, x + PLATFORM_LENGTH);
    platform_solids(&area, |b| {
        if (b.rect.x0 + b.rect.x1 - 2.0 * x).abs() < PLATFORM_LENGTH {
            m.city_box(&CityBox { h0: -0.2, ..*b }, Surface::Kerb, Surface::Pavement, 0.4, 0.0, true);
        }
        false
    });
    let roof = Rect::new(mid - PLATFORM_HALF - 0.3, mid + PLATFORM_HALF + 0.3, x - 30.0, x + 30.0);
    m.city_box(
        &CityBox { rect: roof, h0: FLOOR + 4.2, h1: FLOOR + 4.45 },
        Surface::Steel,
        Surface::Roof,
        0.6,
        0.0,
        true,
    );
    for k in 0..4 {
        let px = x - 27.0 + 18.0 * k as f32;
        let post = Rect::new(mid - 0.15, mid + 0.15, px - 0.15, px + 0.15);
        m.city_box(
            &CityBox { rect: post, h0: FLOOR, h1: FLOOR + 4.2 },
            Surface::Steel,
            Surface::Steel,
            0.6,
            0.0,
            true,
        );
    }
    (anchor, m.mesh)
}

/// Hub Gate on strip `strip`: the terminal at the foot of the docking hub's end cap, and the cap
/// lift's glass shaft up the cap from it to the bay ring, 948 m up.
pub fn hub_gate(strip: u8) -> (CityPos, CityMesh) {
    use bc_sim::colony::city::{TERMINAL_HEIGHT, terminal_rect};
    use bc_sim::colony::hub::BAY_RADIUS;
    let t = terminal_rect();
    let (ms, _) = t.middle();
    let anchor = CityPos::new(strip, t.x0, ms, 0.0);
    let mut m = Builder::new(strip, anchor);
    m.city_box(
        &CityBox { rect: t, h0: -0.3, h1: TERMINAL_HEIGHT },
        Surface::Hall,
        Surface::Roof,
        0.3,
        0.2,
        false,
    );
    let shaft = Rect::new(ms - 7.0, ms + 7.0, t.x0, t.x0 + 14.0);
    let top = COLONY_RADIUS - BAY_RADIUS;
    m.city_box(
        &CityBox { rect: shaft, h0: TERMINAL_HEIGHT, h1: top },
        Surface::Glass,
        Surface::Steel,
        0.7,
        1.0,
        true,
    );
    for side in [-1.0f32, 1.0] {
        let rail = Rect::new(ms + side * 9.0 - 1.0, ms + side * 9.0 + 1.0, t.x0, t.x0 + 3.0);
        m.city_box(
            &CityBox { rect: rail, h0: TERMINAL_HEIGHT, h1: top },
            Surface::Steel,
            Surface::Steel,
            0.5,
            0.0,
            true,
        );
    }
    (anchor, m.mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{CITY, SITE};
    use bc_sim::colony::frame::{Under, from_colony};
    use bc_sim::content::city::SPECIAL;

    /// Where a chunk's vertex is in city coordinates.
    fn back(key: ChunkKey, p: [f32; 3]) -> CityPos {
        let world = key.anchor().to_colony() + Vec3::from_array(p);
        match from_colony(world) {
            Under::Land(c) => c,
            other => panic!("{other:?} isn't over land"),
        }
    }

    #[test]
    fn every_vertex_lies_on_a_box_of_the_rules() {
        for key in [
            ChunkKey::of(0, 0, 30, -2),
            ChunkKey::of(1, 0, 60, 4),
            ChunkKey::of(2, 0, SITE.0 + 4, 3),
            ChunkKey::of(0, 1, 100, 5),
            ChunkKey::of(1, 2, 140, -8),
            ChunkKey::of(2, 3, 40, 1),
        ] {
            let m = chunk(key, Stage(0));
            assert!(!m.is_empty(), "{key:?} is empty");
            assert_eq!(m.positions.len(), m.normals.len());
            assert_eq!(m.positions.len(), m.uvs.len());
            assert_eq!(m.positions.len(), m.colors.len());
            let r = key.rect();
            for (p, n) in m.positions.iter().zip(&m.normals) {
                assert!(p.iter().all(|v| v.is_finite()));
                assert!((Vec3::from_array(*n).length() - 1.0).abs() < 1e-3, "{n:?}");
                let c = back(key, *p);
                assert_eq!(c.strip, key.strip);
                // Within the chunk (trees' crowns and the canal's water reach a little over), and
                // no taller than anything is.
                assert!(
                    c.s > r.s0 - 15.0 && c.s < r.s1 + 15.0 && c.x > r.x0 - 15.0 && c.x < r.x1 + 15.0,
                    "{c:?} {r:?}"
                );
                assert!(c.h > -CANAL_DEPTH - 0.4 && c.h < MAX_HEIGHT + 1.0, "{c:?}");
            }
        }
    }

    #[test]
    fn faces_face_out() {
        // A tower's block, and the Exchange floor's (its room's walls face into the room).
        for key in [ChunkKey::of(0, 0, 30, -2), ChunkKey::of(0, 0, 10, 2)] {
            let m = chunk(key, Stage(0));
            for t in m.indices.chunks(3) {
                let p = |i: u32| Vec3::from_array(m.positions[i as usize]);
                let g = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                let n = Vec3::from_array(m.normals[t[0] as usize]);
                if g.length() > 1e-4 {
                    assert!(g.normalize().dot(n) > 0.5, "a face wound inwards in {key:?}");
                }
            }
        }
    }

    #[test]
    fn the_key_places_rooms_are_drawn_close_up() {
        use bc_sim::colony::city::room;
        let has = |m: &CityMesh, s: Surface| m.colors.iter().any(|c| (c[0] * 255.0).round() as u8 == s as u8);
        for (i, p) in PLACES.iter().enumerate() {
            let Some(r) = room(i) else { continue };
            let near = chunk(ChunkKey::of(p.strip, 0, p.bx, p.row), Stage(0));
            for s in [Surface::Interior, Surface::Floor, Surface::Ceiling, Surface::Counter, Surface::Display]
            {
                assert!(has(&near, s), "{}: no {s:?} close up", p.name);
            }
            // The room's ceiling is where the rules have it, facing down into it.
            let key = ChunkKey::of(p.strip, 0, p.bx, p.row);
            let ceiling = near
                .positions
                .iter()
                .zip(&near.colors)
                .filter(|(_, c)| (c[0] * 255.0).round() as u8 == Surface::Ceiling as u8)
                .map(|(pos, _)| back(key, *pos).h)
                .fold(0.0f32, f32::max);
            assert!((ceiling - r.ceiling).abs() < 0.01, "{}: {ceiling} vs {}", p.name, r.ceiling);
            // Further off, the hall is its bulk.
            let far = chunk(ChunkKey::of(p.strip, 1, p.bx, p.row), Stage(0));
            assert!(!has(&far, Surface::Interior) && has(&far, Surface::Hall), "{} at L1", p.name);
        }
    }

    #[test]
    fn the_axis_view_tower_is_drawn_at_its_height() {
        let (_, _, _, tower) = SPECIAL[0];
        let bc_sim::content::city::Special::Tower(h) = tower else { panic!() };
        for lod in 0..4 {
            let key = ChunkKey::of(0, lod, 30, -2);
            let m = chunk(key, Stage(0));
            let top = m.positions.iter().map(|p| back(key, *p).h).fold(0.0f32, f32::max);
            assert!((top - h).abs() < 0.05, "L{lod}: {top}");
        }
    }

    #[test]
    fn levels_get_lighter_and_stay_in_budget() {
        // The busiest chunks: the business district.
        let mut worst = [0usize; 4];
        for lod in 0..4u8 {
            let n = 2 << lod;
            for i in (CITY.0 / n)..(CITY.0 / n + 64 / n) {
                for j in 0..(24 / n) {
                    let m = chunk(ChunkKey { strip: 0, lod, i, j }, Stage(0));
                    worst[lod as usize] = worst[lod as usize].max(m.triangles());
                }
            }
        }
        let budget = [25_000, 12_000, 8_000, 6_000];
        for lod in 0..4 {
            assert!(worst[lod] <= budget[lod], "L{lod}: {} triangles", worst[lod]);
        }
        // Per block, each level is lighter than the one before.
        let per_block = |lod: usize| worst[lod] as f32 / ((2 << lod) * (2 << lod)) as f32;
        for lod in 1..4 {
            assert!(per_block(lod) < per_block(lod - 1), "L{lod}: {:?}", worst);
        }
    }

    #[test]
    fn the_same_chunk_comes_out_the_same() {
        let key = ChunkKey::of(1, 0, 77, 5);
        assert_eq!(chunk(key, Stage(0)), chunk(key, Stage(0)));
        let (a, ga) = ground(2, 8, 24);
        let (b, gb) = ground(2, 8, 24);
        assert_eq!((a, ga.triangles()), (b, gb.triangles()));
        assert!(ga.triangles() > 1_000);
    }

    #[test]
    fn chunks_tile_the_strip() {
        // Every block's chunk at every level contains it.
        for lod in 0..4u8 {
            for (bx, row) in [(8, -12), (8, 12), (100, -1), (100, 1), (249, 7)] {
                let k = ChunkKey::of(0, lod, bx, row);
                let ((b0, b1), (r0, r1)) = k.blocks();
                assert!((b0..b1).contains(&bx) && (r0..r1).contains(&row_index(row)), "{k:?}");
                assert!(k.rect().holds(&bc_sim::colony::city::block_rect(bx, row)));
            }
        }
        for ri in 0..24 {
            assert_eq!(row_index(row_of_index(ri)), ri);
        }
    }
}
