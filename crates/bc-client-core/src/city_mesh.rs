//! Meshes of the city, built from its rules (`bc_sim::colony::city`), so what's drawn is what's
//! walked: every building is its boxes, bent onto the cylinder (a plumb wall is a radial plane; a
//! roof and a wall facing along the axis curve with the floor, cut every 24 m across).
//!
//! The city is drawn in chunks of blocks at four levels of detail, coarser further off:
//! - L0, 2 × 2 blocks: every building whole, the pavements' kerbs, the canal's quays, railings,
//!   the site's frames and cranes; the street's furniture (`bc_sim::colony::furniture`: lamp posts
//!   with their arms, heads and lanterns, benches, each exactly the solid a walker meets) and its
//!   trees whole, the rules' (the avenue's and the quays') and the parks' and plazas'.
//! - L1, 4 × 4: the buildings and the trees (each a trunk and one blob).
//! - L2, 8 × 8: one box a lot.
//! - L3, 16 × 16: a block's bulk, and its tallest building where it stands out.
//!
//! At every level the canal's channel has its water and its sides (`canal_water`). The avenue
//! belongs to no block's chunk: its pavements' furniture and trees are drawn by the chunks either
//! side of it ([`ChunkKey::draw_rect`]), and the bank road's far kerb by the edge rows'.
//!
//! The ground under it all (streets, parks, the avenue's median, painted from the block atlas) is
//! one mesh a stretch of each strip ([`ground`]): it needs no levels, being flat.
//!
//! Positions are relative to the chunk's anchor (its middle, on the floor) in the colony's own
//! frame, so they keep their precision anywhere in the 32 km; the renderer places the anchor.
//! Vertex colours carry what the city shader paints: `r` the surface ([`Surface`]), `g` a seed,
//! `b` ambient occlusion, `a` the building's height fraction of the tallest (for the facade), or
//! for the street a code (what a post holds up, whether a lantern always burns, a tree's species).
//! UVs: on walls, metres along the wall and up from the floor; on roofs and the ground, `s` and `x`.

use bc_sim::colony::city::{
    BLOCK, BlockInfo, BlockKind, Building, CANAL_DEPTH, CityBox, Form, GRID_X0, KERB, MAX_HEIGHT, MAX_SOLIDS,
    Part, Piece, RAILING, ROWS, Rect, Room, SIDEWALK, Stage, Style, WALL, block, block_index, channel,
    grid_x, lots, mix, row_at, unit,
};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, strip_edge};
use bc_sim::colony::furniture::{self, Furniture, Kind as Furn, TRUNK, each_furniture, tree_size};
use bc_sim::content::city::{PLACES, PlaceKind};
use bc_sim::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};
use glam::{Vec2, Vec3};

/// The finest a curved face is cut across, m.
const CUT: f32 = 24.0;

/// The canal's water, a metre below the street.
const WATER: f32 = -1.0;

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
    /// A tree's leaves (`a`: its species, [`Species`], in the low four bits, 16 more in blossom;
    /// UVs: how far out of the trunk's reach the leaf is, for a sway to come, and 0).
    Tree = 7,
    /// Its trunk and limbs (`a` as its leaves'; UVs: metres round, and up the trunk or along a
    /// limb).
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
    /// The colony's lamp posts and what they hold up (`a`: what, [`POST_STREET`] and on): white
    /// panels, the strip's line colour in a band. UVs on their sides: metres round from a corner (or
    /// along), and up from the foot (a head's and an arm's from their own underside).
    Post = 19,
    /// A lamp's lantern (`a`: [`LANTERN_ROW`] it burns as the ground's lamps light its pool,
    /// [`LANTERN_ALWAYS`] always, a plaza's; UVs: where it hangs, `(s, x)`, the same at every
    /// vertex: the shader reads its pool's brightness there).
    Lantern = 20,
    /// A bench's slab (UVs on its sides: along, and up from its foot; on top, `s` and `x`).
    Bench = 21,
    // 22 to 24: kept for kiosks and vending machines, the tram's wires, rows of trees from afar.
    /// A tower's crown, a lantern, a belfry (`bc_sim::colony::city::Part::Crown`): louvres and
    /// panels, no storeys of windows, a band of light at night.
    Crown = 25,
    /// What stands on a roof (`Part::Plant`): plant rooms, lift overruns, tanks, chimneys, a
    /// works' rooflights.
    Plant = 26,
}

/// What a post is or holds up (`a` of a [`Surface::Post`] vertex, in 255ths): a street lamp's post,
/// an avenue lamp's, a path's or a plaza's, a street lamp's head, its arm.
pub const POST_STREET: u8 = 0;
pub const POST_AVENUE: u8 = 1;
pub const POST_PATH: u8 = 2;
pub const POST_HEAD: u8 = 3;
pub const POST_ARM: u8 = 4;
/// Whether a lantern burns as the ground's lamps light its pool (the city's lamps, some dim, one in
/// sixteen out) or always (the colony's own, a plaza's): `a` of a [`Surface::Lantern`] vertex.
pub const LANTERN_ROW: u8 = 0;
pub const LANTERN_ALWAYS: u8 = 1;

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

    /// Where a chunk draws its street furniture and the rules' trees: its rect, out to the avenue's
    /// middle line when it borders the avenue (no block's chunk holds the avenue, so its pavements'
    /// lamps, trees and benches are drawn by the chunks of rows ±1 beside it), and out to the glass at
    /// the strip's edges (the bank road's far kerb). Half-open: `s0 <= s < s1`, `x0 <= x < x1`.
    pub fn draw_rect(&self) -> Rect {
        let (_, (r0, r1)) = self.blocks();
        let mut r = self.rect();
        let mid = STRIP_WIDTH * 0.5;
        if r0 == row_index(1) {
            r.s0 = mid;
        }
        if r1 == row_index(-1) + 1 {
            r.s1 = mid;
        }
        if r0 == 0 {
            r.s0 = 0.0;
        }
        if r1 == 2 * ROWS {
            r.s1 = STRIP_WIDTH;
        }
        r
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

    /// An upright piece exactly its solid `b`: four sides and a top. `round`: its sides' normals lean
    /// out at its edges (each corner's the mean of its two sides'), so a square post shades round, as
    /// the colony's are; else flat (benches). UVs on its sides: metres round from its −s −x corner
    /// (flat: along each face), and up from `foot`; on top, `s` and `x`.
    #[allow(clippy::too_many_arguments)]
    fn post(&mut self, b: &CityBox, foot: f32, surface: Surface, seed: f32, code: u8, round: bool) {
        let r = b.rect;
        let col = |h: f32| {
            [surface as u8 as f32 / 255.0, seed, (0.7 + 0.25 * (h - foot)).min(1.0), f32::from(code) / 255.0]
        };
        // Its corners round from −s −x, and the faces between them (out: −X, +s, +X, −s).
        let corner = [(r.s0, r.x0), (r.s1, r.x0), (r.s1, r.x1), (r.s0, r.x1)];
        let face = [-Vec3::X, self.across(r.s1), Vec3::X, -self.across(r.s0)];
        let side = [r.width(), r.length(), r.width(), r.length()];
        if round {
            let mut ids = [[0u32; 5]; 2];
            let mut u = 0.0;
            for j in 0..5 {
                let (s, x) = corner[j % 4];
                let n = (face[(j + 3) % 4] + face[j % 4]).normalize();
                for (e, h) in [b.h0, b.h1].into_iter().enumerate() {
                    ids[e][j] = self.vertex(self.at(s, x, h), n, [u, h - foot], col(h));
                }
                u += side[j % 4];
            }
            for j in 0..4 {
                self.quad([ids[0][j], ids[0][j + 1], ids[1][j + 1], ids[1][j]], face[j]);
            }
        } else {
            for j in 0..4 {
                let ((sa, xa), (sb, xb)) = (corner[j], corner[(j + 1) % 4]);
                let v = [
                    self.vertex(self.at(sa, xa, b.h0), face[j], [0.0, b.h0 - foot], col(b.h0)),
                    self.vertex(self.at(sb, xb, b.h0), face[j], [side[j], b.h0 - foot], col(b.h0)),
                    self.vertex(self.at(sb, xb, b.h1), face[j], [side[j], b.h1 - foot], col(b.h1)),
                    self.vertex(self.at(sa, xa, b.h1), face[j], [0.0, b.h1 - foot], col(b.h1)),
                ];
                self.quad(v, face[j]);
            }
        }
        self.level(&r, b.h1, false, col(b.h1));
    }

    /// A box over anyone's reach (an arm, a lamp's head), all six faces: its sides and top `side`
    /// (UVs on its sides: along, and up from its own underside), its underside `under` (a lantern's
    /// diffuser: then `pool`, its pool's middle, is the underside's UV at every vertex).
    fn overhead(&mut self, b: &CityBox, side: [f32; 4], under: [f32; 4], pool: Option<[f32; 2]>) {
        let r = b.rect;
        let c = move |_: f32| side;
        let sides = self.mesh.uvs.len();
        self.wall_x(r.x0, r.s0, r.s1, b.h0, b.h1, -1.0, r.s0, c);
        self.wall_x(r.x1, r.s0, r.s1, b.h0, b.h1, 1.0, r.s0, c);
        self.wall_s(r.s0, r.x0, r.x1, b.h0, b.h1, -1.0, r.x0, c);
        self.wall_s(r.s1, r.x0, r.x1, b.h0, b.h1, 1.0, r.x0, c);
        for uv in &mut self.mesh.uvs[sides..] {
            uv[1] -= b.h0;
        }
        self.level(&r, b.h1, false, side);
        let first = self.mesh.uvs.len();
        self.level(&r, b.h0, true, under);
        if let Some(uv) = pool {
            self.mesh.uvs[first..].fill(uv);
        }
    }

    /// A lantern on top of a post: every face lit, every UV where it hangs (`pool`), from which the
    /// shader reads its pool's brightness.
    fn lantern_box(&mut self, b: &CityBox, col: [f32; 4], pool: [f32; 2]) {
        let first = self.mesh.uvs.len();
        self.overhead(b, col, col, None);
        self.mesh.uvs[first..].fill(pool);
    }

    /// A tree's frame at its foot.
    fn local(&self, s: f32, x: f32, h: f32) -> Local {
        Local { o: self.at(s, x, h), up: self.up(s), across: self.across(s) }
    }

    /// A triangle wound to face away from `inside` (a point within what it bounds).
    fn tri(&mut self, v: [u32; 3], inside: Vec3) {
        let p = |i: u32| Vec3::from_array(self.mesh.positions[i as usize]);
        let g = (p(v[1]) - p(v[0])).cross(p(v[2]) - p(v[0]));
        let c = (p(v[0]) + p(v[1]) + p(v[2])) / 3.0;
        if g.dot(c - inside) >= 0.0 {
            self.mesh.indices.extend_from_slice(&v);
        } else {
            self.mesh.indices.extend_from_slice(&[v[0], v[2], v[1]]);
        }
    }

    /// A tree whole (L0): its trunk (the rules' solid at its foot), its limbs (the rules' trees,
    /// seen from under them), its crown grown by its habit and kept clear of the lanterns round it
    /// (`(s, x, top)`), each leaf vertex carrying how far out of the trunk's reach it is.
    fn tree_whole(&mut self, t: &TreeAt, lanterns: &[(f32, f32, f32)]) {
        let f = self.local(t.s, t.x, t.h);
        let hab = &HABITS[t.species as usize];
        let crown = Crown::grow(hab, t, lanterns);
        let (seed, code) = ((t.seed & 0xff) as f32 / 255.0, t.code());
        let bark =
            |u: f32| [Surface::Trunk as u8 as f32 / 255.0, seed, 0.55 + 0.45 * (u / 2.0).min(1.0), code];
        self.trunk(&f, t.half, crown.fork, 0.7, bark);
        if t.limbs {
            // Its limbs, from just under the fork out into the ring's blobs (a poplar's leader
            // straight up).
            let from = Vec3::Y * (crown.fork - 0.25);
            let r0 = t.half * 0.55;
            for k in 0..hab.limbs.min(crown.n as u32) as usize {
                let (c, r) = crown.blobs[if hab.limbs == 1 { 0 } else { (k + 1).min(crown.n - 1) }];
                let out = Vec3::new(c.x, 0.0, c.z) * (1.0 - hab.rise * 0.5);
                let to = Vec3::new(out.x, (c.y - r.y * 0.3).max(crown.fork + 0.8), out.z);
                self.limb(&f, from, to, r0, r0 * 0.35, bark(9.0));
            }
        }
        // The crown, shaded as one soft volume.
        let leaf = |p: Vec3, n: Vec3| {
            let q = (p - crown.mid) / crown.r;
            let depth = ((q.length() - 0.35) / 0.75).clamp(0.0, 1.0);
            let ao = ((0.42 + 0.58 * depth) * (0.8 + 0.2 * n.y)).clamp(0.3, 1.0);
            [Surface::Tree as u8 as f32 / 255.0, seed, ao, code]
        };
        // How much a leaf will sway: none at the fork, all of it at the crown's top edge.
        let sway = |p: Vec3, _: Vec3| {
            let up = ((p.y - crown.fork) / (crown.top - crown.fork)).clamp(0.0, 1.0);
            [up * (0.6 + 0.4 * (Vec2::new(p.x, p.z).length() / crown.r.x.max(0.1)).min(1.0)), 0.0]
        };
        for k in 0..crown.n {
            let (c, r) = crown.blobs[k];
            self.blob(&f, c, r, &crown, mix(t.seed, 0xB10B, k as u32), &leaf, &sway);
        }
    }

    /// A tree from 350 to 900 m (L1): its trunk a three-sided prism to the fork, its crown one lumpy
    /// blob filling it; the same species, seed and colours as close up.
    fn tree_far(&mut self, t: &TreeAt, lanterns: &[(f32, f32, f32)]) {
        let f = self.local(t.s, t.x, t.h);
        let crown = Crown::grow(&HABITS[t.species as usize], t, lanterns);
        let (seed, code) = ((t.seed & 0xff) as f32 / 255.0, t.code());
        let bark = [Surface::Trunk as u8 as f32 / 255.0, seed, 0.8, code];
        self.limb(&f, Vec3::ZERO, Vec3::Y * crown.fork, t.half, t.half * 0.7, bark);
        let (mid, r) = crown.far_blob();
        let col = |p: Vec3, n: Vec3| {
            let ao = (0.55 + 0.45 * ((p.y - mid.y) / r.y * 0.5 + 0.5)) * (0.85 + 0.15 * n.y);
            [Surface::Tree as u8 as f32 / 255.0, seed, ao.clamp(0.35, 1.0), code]
        };
        self.blob(&f, mid, r, &crown, t.seed, &col, &|_, _| [0.0, 0.0]);
    }

    /// One of a crown's blobs: an icosahedron at `c` with radii `r` (local), each vertex pushed in or
    /// out by up to half of `LUMP`; its normals lean out from the crown's middle (65% the crown
    /// ellipsoid's own normal), so the crown shades as one volume and its blobs show in its outline.
    #[allow(clippy::too_many_arguments)]
    fn blob(
        &mut self,
        f: &Local,
        c: Vec3,
        r: Vec3,
        crown: &Crown,
        seed: u32,
        col: &impl Fn(Vec3, Vec3) -> [f32; 4],
        uv: &impl Fn(Vec3, Vec3) -> [f32; 2],
    ) {
        let mut ids = [0u32; 12];
        for (j, v) in ICO.iter().enumerate() {
            let v = Vec3::from_array(*v);
            let p = c + r * v * (1.0 + LUMP * (unit(seed, j as u32) - 0.5));
            let e = ((p - crown.mid) / (crown.r * crown.r)).normalize_or(v);
            let n = (v * 0.35 + e * 0.65).normalize();
            ids[j] = self.vertex(f.at(p), f.dir(n), uv(p, n), col(p, n));
        }
        let inside = f.at(c);
        for t in ICO_FACES {
            self.tri([ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]], inside);
        }
    }

    /// A trunk from the foot to its fork: an octagon whose four broad sides stand on the solid's
    /// (`half` out across and along: every vertex at its foot lies on the rules' box, so what a walker
    /// meets is what's drawn), tapering to `taper` of it. Normals round; UVs round and up.
    fn trunk(&mut self, f: &Local, half: f32, fork: f32, taper: f32, col: impl Fn(f32) -> [f32; 4]) {
        use std::f32::consts::{FRAC_PI_4, FRAC_PI_8};
        let rad = half / FRAC_PI_8.cos();
        let mut ring = [[0u32; 9]; 2];
        for (ids, (u, k)) in ring.iter_mut().zip([(0.0, 1.0), (fork, taper)]) {
            for (j, id) in ids.iter_mut().enumerate() {
                // (j = 8 is j = 0 again, for the seam in u.)
                let a = FRAC_PI_8 + FRAC_PI_4 * j as f32;
                let n = Vec3::new(a.cos(), 0.0, a.sin());
                *id = self.vertex(f.at(n * rad * k + Vec3::Y * u), f.dir(n), [a * half, u], col(u));
            }
        }
        let axis = f.at(Vec3::Y * fork * 0.5);
        for j in 0..8 {
            self.tri([ring[0][j], ring[0][j + 1], ring[1][j + 1]], axis);
            self.tri([ring[0][j], ring[1][j + 1], ring[1][j]], axis);
        }
    }

    /// A limb (or an L1 trunk): three-sided, from `from` to `to` (local), tapering from `r0` to `r1`.
    fn limb(&mut self, f: &Local, from: Vec3, to: Vec3, r0: f32, r1: f32, col: [f32; 4]) {
        let axis = (to - from).normalize();
        let side = axis.cross(Vec3::Y).normalize_or(Vec3::X);
        let other = axis.cross(side);
        let mut ends = [[0u32; 3]; 2];
        for (ids, (p, r)) in ends.iter_mut().zip([(from, r0), (to, r1)]) {
            for (j, id) in ids.iter_mut().enumerate() {
                let a = std::f32::consts::TAU * j as f32 / 3.0;
                let n = side * a.cos() + other * a.sin();
                *id = self.vertex(f.at(p + n * r), f.dir(n), [a * r0, (p - from).length()], col);
            }
        }
        let inside = f.at((from + to) * 0.5);
        for j in 0..3 {
            let k = (j + 1) % 3;
            self.tri([ends[0][j], ends[0][k], ends[1][k]], inside);
            self.tri([ends[0][j], ends[1][k], ends[1][j]], inside);
        }
    }
}

/// How a piece of a building is drawn: its walls' surface and its top's, and whether it's small
/// enough to draw flat (not bent onto the cylinder).
fn faces(b: &Building, part: Part) -> (Surface, Surface, bool) {
    match (part, b.style) {
        (_, Style::Frame | Style::Crane) => (Surface::Steel, Surface::Steel, false),
        (Part::Body, Style::Hall) => (Surface::Hall, Surface::Roof, false),
        (Part::Body, _) => (Surface::Wall, Surface::Roof, false),
        (Part::Tier, Style::Tower) => (Surface::Glass, Surface::Roof, false),
        (Part::Tier, _) => (Surface::Wall, Surface::Roof, false),
        (Part::Crown, _) => (Surface::Crown, Surface::Roof, false),
        (Part::Plant, _) => (Surface::Plant, Surface::Roof, true),
        (Part::Mast, _) => (Surface::Steel, Surface::Steel, true),
    }
}

/// A piece drawn: its own top is its height fraction (the facade's roof line), its surfaces its
/// part's. Small pieces are drawn flat (a shed's rooflights, which can run across it, aren't: flat,
/// their middle would stand off its curved roof); so is anything far off (`far`).
fn piece(m: &mut Builder, b: &Building, q: &CityBox, part: Part, seed: f32, far: bool) {
    let (wall, roof, flat) = faces(b, part);
    m.city_box(q, wall, roof, seed, q.h1 / MAX_HEIGHT, far || (flat && q.rect.width() <= CUT));
}

/// A tower from afar. At L2: its podium, its shaft as one box from its lowest tier's foot to its
/// highest tier's top, its crown and its mast. At L3 (standing out of its block's bulk, from `from`
/// up): its shaft to the crown's top, and its mast. Its plant is left off.
fn tower_far(m: &mut Builder, b: &Building, seed: f32, from: Option<f32>) {
    let mut p = [Piece::default(); MAX_SOLIDS];
    let n = b.pieces(&mut p);
    let mut shaft: Option<CityBox> = None;
    for q in &p[..n] {
        match (q.part, from) {
            (Part::Tier, _) => shaft.get_or_insert(q.b).h1 = q.b.h1,
            (Part::Crown, Some(_)) => {
                if let Some(s) = &mut shaft {
                    s.h1 = s.h1.max(q.b.h1);
                }
            }
            (Part::Body, None) | (Part::Crown, None) => piece(m, b, &q.b, q.part, seed, false),
            (Part::Mast, _) => piece(m, b, &q.b, q.part, seed, true),
            _ => {}
        }
    }
    if let Some(mut s) = shaft {
        s.h0 = from.unwrap_or(s.h0);
        piece(m, b, &s, Part::Tier, seed, from.is_some());
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
    let door = room.door_width() * 0.5;
    let lintel = KERB + room.door_height;
    let kind = match PLACES[usize::from(room.place)].kind {
        PlaceKind::Bar => 1.0,
        PlaceKind::Exchange => 2.0,
        PlaceKind::Charter => 3.0,
        PlaceKind::Proving => 4.0,
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
    let mut pieces = [Piece::default(); MAX_SOLIDS];
    // The street's furniture round the chunk (a little wider, so a tree by the rect's edge sees the
    // lanterns past it): drawn by `street`, and the trees kept clear of its lanterns.
    let near: Vec<Furniture> = if key.lod <= 1 {
        let mut v = Vec::new();
        each_furniture(key.strip, &key.draw_rect().inset(-LAMP_REACH), stage, |p| {
            v.push(*p);
            false
        });
        v
    } else {
        Vec::new()
    };
    let lanterns: Vec<(f32, f32, f32)> = near.iter().filter_map(lantern_top).collect();
    for bx in b0..b1 {
        for ri in r0..r1 {
            let row = row_of_index(ri);
            let Some(b) = block(key.strip, bx, row, stage) else { continue };
            let seed = (b.seed & 0xff) as f32 / 255.0;
            let all = lots(&b);
            let buildings = all.as_slice();
            if b.kind == BlockKind::Canal {
                canal_water(&mut m, &b, seed, key.lod, stage);
            }
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
                                let cb = CityBox { rect: bd.foot, h0: KERB, h1: bd.height };
                                m.city_box(&cb, Surface::Hall, Surface::Roof, s, tall, false);
                            }
                            _ => {
                                let n = bd.pieces(&mut pieces);
                                for q in &pieces[..n] {
                                    piece(&mut m, bd, &q.b, q.part, s, false);
                                }
                            }
                        }
                    }
                    decor_trees(&mut m, &b, buildings, &lanterns, key.lod);
                }
                2 => {
                    // One box a lot at its bulk; a tower, its podium, shaft, crown and mast.
                    for bd in buildings {
                        let s = (bd.seed & 0xff) as f32 / 255.0;
                        if bd.style == Style::Frame {
                            continue;
                        }
                        if matches!(bd.form, Form::Tower { .. }) {
                            tower_far(&mut m, bd, s, None);
                        } else {
                            let cb = CityBox { rect: bd.foot, h0: KERB, h1: bd.bulk() };
                            piece(&mut m, bd, &cb, Part::Body, s, false);
                        }
                    }
                }
                _ => {
                    // The block's bulk at its buildings' mean roof, and its tallest if it stands out.
                    let (mut area, mut volume, mut tallest) = (0.0f32, 0.0f32, None::<(&Building, f32)>);
                    for bd in buildings.iter().filter(|b| b.style != Style::Frame && b.style != Style::Crane)
                    {
                        let a = bd.foot.width() * bd.foot.length();
                        let top = sky(bd);
                        area += a;
                        volume +=
                            a * if matches!(bd.form, Form::Tower { .. }) { bd.height } else { bd.bulk() };
                        if tallest.is_none_or(|(_, t)| top > t) {
                            tallest = Some((bd, top));
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
                        if let Some((t, top)) = tallest
                            && top > mean * 1.6
                        {
                            let s = (t.seed & 0xff) as f32 / 255.0;
                            if matches!(t.form, Form::Tower { .. }) {
                                tower_far(&mut m, t, s, Some(mean));
                            } else {
                                let cb = CityBox { rect: t.foot, h0: mean, h1: top };
                                piece(&mut m, t, &cb, Part::Tier, s, true);
                            }
                        }
                    }
                }
            }
        }
    }
    if key.lod <= 1 {
        street(&mut m, &key, &near, &lanterns);
    }
    m.mesh
}

/// How tall a building stands from afar: a tower, to its crown; anything else, its bulk.
fn sky(b: &Building) -> f32 {
    match b.form {
        Form::Tower { top, .. } => top,
        _ => b.bulk(),
    }
}

/// A block's kerb (it stands a step up from the street), and in the canal's row the quays either
/// side of the channel and their railings.
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

/// A canal block's channel, at every level (the ground leaves it open, `city.wgsl`): its water
/// from grid line to grid line, on under the bridges to meet the next block's, and its sides from
/// the bed up, so it never shows the clear colour through. Under the bridges the sides meet the
/// street's underside, and where the canal ends they close it. Along the block L0 has its quays;
/// further off the sides stand to the kerb's height, over the ground's edge (its chords stand a
/// few cm off the curve), so no crack shows there at a glance. The sides start at the bed, as the
/// quays do, so the waterline is never a seam, and along the block they are the quays' faces
/// exactly (where, how shaded, UVs), so a level's swap for another shows no change.
fn canal_water(m: &mut Builder, b: &BlockInfo, seed: f32, lod: u8, stage: Stage) {
    let r = b.rect;
    let ch = channel(&r);
    let (x0, x1) = (grid_x(b.bx).max(-COLONY_HALF_LENGTH), grid_x(b.bx + 1).min(COLONY_HALF_LENGTH));
    let water = [Surface::Water as u8 as f32 / 255.0, seed, 1.0, 0.0];
    m.level(&Rect::new(ch.s0, ch.s1, x0, x1), WATER, false, water);
    // Shaded at the bed, and all the way up in the dark under a bridge.
    let bed = -CANAL_DEPTH;
    let wall = |top: f32| {
        move |h: f32| [Surface::CanalWall as u8 as f32 / 255.0, seed, if h > bed { top } else { 0.62 }, 0.0]
    };
    for (s, ns) in [(ch.s0, 1.0), (ch.s1, -1.0)] {
        m.wall_s(s, x0, r.x0, bed, 0.0, ns, x0, wall(0.62));
        m.wall_s(s, r.x1, x1, bed, 0.0, ns, x0, wall(0.62));
        if lod > 0 {
            m.wall_s(s, r.x0, r.x1, bed, KERB, ns, r.x0, wall(1.0));
        }
    }
    let canal = |bx: i32| block(b.strip, bx, b.row, stage).is_some_and(|n| n.kind == BlockKind::Canal);
    if !canal(b.bx - 1) {
        m.wall_x(x0, ch.s0, ch.s1, bed, 0.0, 1.0, ch.s0, wall(0.62));
    }
    if !canal(b.bx + 1) {
        m.wall_x(x1, ch.s0, ch.s1, bed, 0.0, -1.0, ch.s0, wall(0.62));
    }
}

// ------------------------------------------------------------------------------------ the street

/// How far round a chunk's draw rect its street furniture is gathered: a tree by the rect's edge
/// sees the lanterns past it.
const LAMP_REACH: f32 = 12.0;
/// A lantern on top of a post (an avenue's, a path's, a plaza's): this tall.
const LANTERN_HEIGHT: f32 = 0.4;

/// Where a lamp's lantern hangs and its top, for the trees round it.
fn lantern_top(p: &Furniture) -> Option<(f32, f32, f32)> {
    let (s, x, h) = p.lantern()?;
    Some((s, x, if p.kind == Furn::StreetLamp { h } else { h + LANTERN_HEIGHT }))
}

/// The street's furniture and the rules' trees standing in the chunk's draw rect: close up (L0)
/// all of it, further off (L1) the trees alone.
fn street(m: &mut Builder, key: &ChunkKey, near: &[Furniture], lanterns: &[(f32, f32, f32)]) {
    let r = key.draw_rect();
    for p in near.iter().filter(|p| r.s0 <= p.s && p.s < r.s1 && r.x0 <= p.x && p.x < r.x1) {
        match (p.kind, key.lod) {
            (Furn::Tree, 0) => m.tree_whole(&TreeAt::rules(key.strip, p), lanterns),
            (Furn::Tree, _) => m.tree_far(&TreeAt::rules(key.strip, p), lanterns),
            (_, 0) => furniture(m, p),
            _ => {}
        }
    }
}

/// From `(s, x)` to `(ls, lx)` (one of them the same), `half` either side.
fn span(s: f32, x: f32, ls: f32, lx: f32, half: f32) -> Rect {
    Rect::new(s.min(ls) - half, s.max(ls) + half, x.min(lx) - half, x.max(lx) + half)
}

/// A rect about `(s, x)`, `along` either way along `facing` and `across` either way across it.
fn about(s: f32, x: f32, facing: (f32, f32), along: f32, across: f32) -> Rect {
    let (hs, hx) = if facing.0.abs() > 0.5 { (along, across) } else { (across, along) };
    Rect::new(s - hs, s + hs, x - hx, x + hx)
}

/// A piece of the street's furniture close up (L0): what's solid exactly as the rules have it, and
/// over anyone's reach its arm, its head, its lantern.
fn furniture(m: &mut Builder, p: &Furniture) {
    let seed = (p.seed & 0xff) as f32 / 255.0;
    let col = |s: Surface, code: u8| [s as u8 as f32 / 255.0, seed, 1.0, f32::from(code) / 255.0];
    let top = p.solid.h1;
    let (ls, lx, _) = p.lantern().unwrap_or((p.s, p.x, top));
    match p.kind {
        Furn::StreetLamp => {
            m.post(&p.solid, p.h, Surface::Post, seed, POST_STREET, true);
            let arm = CityBox { rect: span(p.s, p.x, ls, lx, 0.06), h0: top - 0.35, h1: top - 0.2 };
            m.overhead(&arm, col(Surface::Post, POST_ARM), col(Surface::Post, POST_ARM), None);
            let head = CityBox { rect: about(ls, lx, p.facing, 0.35, 0.175), h0: top - 0.45, h1: top - 0.2 };
            m.overhead(
                &head,
                col(Surface::Post, POST_HEAD),
                col(Surface::Lantern, LANTERN_ROW),
                Some([ls, lx]),
            );
        }
        Furn::AvenueLamp | Furn::PathLamp | Furn::PlazaLamp => {
            let (code, half) = match p.kind {
                Furn::AvenueLamp => (POST_AVENUE, 0.25),
                _ => (POST_PATH, 0.175),
            };
            m.post(&p.solid, p.h, Surface::Post, seed, code, true);
            let lit = if p.kind == Furn::PlazaLamp { LANTERN_ALWAYS } else { LANTERN_ROW };
            let b = CityBox {
                rect: Rect::new(p.s - half, p.s + half, p.x - half, p.x + half),
                h0: top,
                h1: top + LANTERN_HEIGHT,
            };
            m.lantern_box(&b, col(Surface::Lantern, lit), [ls, lx]);
        }
        Furn::Bench => m.post(&p.solid, p.h, Surface::Bench, seed, 0, false),
        Furn::Tree => {}
    }
}

// ------------------------------------------------------------------------------------- the trees

/// A tree's kind: its habit (`HABITS`) and the shader's palette and bark (`bc::facade`'s
/// `f_leaves`, `f_bark`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Species {
    Plane = 0,
    Lime = 1,
    Poplar = 2,
    Alder = 3,
    Cherry = 4,
    Apple = 5,
    Oak = 6,
    Birch = 7,
}

/// Where a tree is planted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Avenue,
    Quay,
    Park,
    Plaza,
}

/// What grows where (`u` a draw from the tree's, or its stretch of avenue's, seed), and whether it's
/// in blossom (a cherry, half of them; a plaza's always). Each strip reads as its own from
/// overhead: Charter's yellow-green planes, the Canal's dark columns of poplars, the Gardens'
/// cherries.
fn species(strip: u8, setting: Setting, u: f32, v: f32) -> (Species, bool) {
    use Species::*;
    let s = match (strip, setting) {
        (0, Setting::Avenue) => {
            if u < 0.7 {
                Plane
            } else {
                Lime
            }
        }
        (0, Setting::Quay | Setting::Plaza) => Lime,
        (0, Setting::Park) => {
            [Oak, Oak, Oak, Oak, Lime, Lime, Lime, Plane, Plane, Birch][(u * 10.0) as usize % 10]
        }
        (1, Setting::Avenue) => Poplar,
        (1, Setting::Quay) => {
            if u < 0.6 {
                Alder
            } else {
                Poplar
            }
        }
        (1, Setting::Park) => {
            if u < 0.35 {
                Alder
            } else if u < 0.7 {
                Birch
            } else {
                Poplar
            }
        }
        (1, Setting::Plaza) => Birch,
        (_, Setting::Avenue | Setting::Plaza) => Cherry,
        (_, Setting::Quay) => {
            if u < 0.5 {
                Cherry
            } else {
                Apple
            }
        }
        (_, Setting::Park) => {
            if u < 0.5 {
                Apple
            } else if u < 0.8 {
                Cherry
            } else {
                Oak
            }
        }
    };
    (s, s == Cherry && (setting == Setting::Plaza || v < 0.5))
}

/// How a species grows (`Species` order), in its size: the trunk's fork, the crown's base, top and
/// radius; how many blobs ring it and how far out (of the radius), or stacked up it (with one
/// offset, an alder's); its limbs and how steeply they rise (0 flat, 1 straight up).
#[derive(Clone, Copy)]
struct Habit {
    fork: f32,
    base: f32,
    top: f32,
    radius: f32,
    ring: u32,
    out: f32,
    stacked: bool,
    offset: bool,
    limbs: u32,
    rise: f32,
}

const HABITS: [Habit; 8] = [
    // A plane: broad, irregular, lifted clear of the street.
    Habit {
        fork: 0.42,
        base: 0.46,
        top: 1.25,
        radius: 0.48,
        ring: 4,
        out: 0.45,
        stacked: false,
        offset: false,
        limbs: 3,
        rise: 0.55,
    },
    // A lime: dense, ovate.
    Habit {
        fork: 0.40,
        base: 0.45,
        top: 1.30,
        radius: 0.40,
        ring: 3,
        out: 0.40,
        stacked: false,
        offset: false,
        limbs: 2,
        rise: 0.70,
    },
    // A poplar: a column.
    Habit {
        fork: 0.30,
        base: 0.32,
        top: 1.70,
        radius: 0.17,
        ring: 3,
        out: 0.0,
        stacked: true,
        offset: false,
        limbs: 1,
        rise: 1.0,
    },
    // An alder: conical, dark.
    Habit {
        fork: 0.36,
        base: 0.42,
        top: 1.20,
        radius: 0.32,
        ring: 3,
        out: 0.0,
        stacked: true,
        offset: true,
        limbs: 2,
        rise: 0.75,
    },
    // A cherry: a wide vase.
    Habit {
        fork: 0.42,
        base: 0.48,
        top: 1.05,
        radius: 0.55,
        ring: 4,
        out: 0.55,
        stacked: false,
        offset: false,
        limbs: 3,
        rise: 0.60,
    },
    // An apple: low and spreading.
    Habit {
        fork: 0.26,
        base: 0.32,
        top: 0.85,
        radius: 0.50,
        ring: 3,
        out: 0.50,
        stacked: false,
        offset: false,
        limbs: 3,
        rise: 0.40,
    },
    // An oak: broad and lumpy.
    Habit {
        fork: 0.36,
        base: 0.40,
        top: 1.25,
        radius: 0.55,
        ring: 4,
        out: 0.50,
        stacked: false,
        offset: false,
        limbs: 3,
        rise: 0.50,
    },
    // A birch: light and narrow.
    Habit {
        fork: 0.30,
        base: 0.36,
        top: 1.35,
        radius: 0.25,
        ring: 2,
        out: 0.35,
        stacked: false,
        offset: false,
        limbs: 2,
        rise: 0.85,
    },
];

/// The unit icosahedron: a crown's blob before its lumps.
const ICO: [[f32; 3]; 12] = {
    const T: f32 = 0.850_650_8;
    const O: f32 = 0.525_731_1;
    [
        [-O, T, 0.0],
        [O, T, 0.0],
        [-O, -T, 0.0],
        [O, -T, 0.0],
        [0.0, -O, T],
        [0.0, O, T],
        [0.0, -O, -T],
        [0.0, O, -T],
        [T, 0.0, -O],
        [T, 0.0, O],
        [-T, 0.0, -O],
        [-T, 0.0, O],
    ]
};
const ICO_FACES: [[u8; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

/// A tree's local frame, flat at its foot: up (towards the axis), across the strip, along the axis.
/// (A 12 m tree drifts 6 mm from the curve over its crown: nothing to bend.) Local vectors are
/// `(across, up, along)`.
#[derive(Clone, Copy)]
struct Local {
    o: Vec3,
    up: Vec3,
    across: Vec3,
}

impl Local {
    fn at(&self, v: Vec3) -> Vec3 {
        self.o + self.across * v.x + self.up * v.y + Vec3::X * v.z
    }

    fn dir(&self, v: Vec3) -> Vec3 {
        (self.across * v.x + self.up * v.y + Vec3::X * v.z).normalize()
    }
}

/// A tree to draw.
struct TreeAt {
    s: f32,
    x: f32,
    h: f32,
    size: f32,
    /// Its trunk's half-width at the foot: the rules' solid's for theirs (0.2 m).
    half: f32,
    /// How low its leaves may hang over its foot.
    clear: f32,
    species: Species,
    blossom: bool,
    seed: u32,
    /// At most this many blobs round its crown.
    ring_max: u32,
    /// Limbs from its fork into its crown (the rules' trees, seen from under them).
    limbs: bool,
}

/// No leaf of the rules' trees lower than this over their ground: over the avenue's lanterns by
/// `LANTERN_CLEAR`, and out of anyone's reach. The parks' trees, walked through, over a head.
pub(crate) const TREE_CLEAR: f32 = 5.5;
const DECOR_CLEAR: f32 = 2.9;
/// The clear space a crown keeps round a lantern, m.
pub(crate) const LANTERN_CLEAR: f32 = 1.0;
/// The most a crown reaches out from its trunk, in tree sizes.
const MAX_REACH: f32 = 0.6;
/// How far a blob's vertices stray in or out of its radius, for lumps.
const LUMP: f32 = 0.3;

impl TreeAt {
    /// One of the rules' trees: in the avenue's pits (a species a run of four blocks and a side)
    /// or on a quay.
    fn rules(strip: u8, p: &Furniture) -> Self {
        let mid = STRIP_WIDTH * 0.5;
        let (setting, u) = if row_at(p.s) == 0 {
            let run = mix(u32::from(strip) + 1, (block_index(p.x) >> 2) as u32, u32::from(p.s > mid));
            (Setting::Avenue, unit(run, 1))
        } else {
            (Setting::Quay, unit(p.seed, 5))
        };
        let (species, blossom) = species(strip, setting, u, unit(p.seed, 6));
        Self {
            s: p.s,
            x: p.x,
            h: p.h,
            size: tree_size(p.seed),
            half: p.solid.rect.width() * 0.5,
            clear: TREE_CLEAR,
            species,
            blossom,
            seed: p.seed,
            ring_max: 4,
            limbs: true,
        }
    }

    /// A park's or a plaza's (decoration): `h` its draw.
    fn decor(strip: u8, kind: BlockKind, h: u32, s: f32, x: f32) -> Self {
        let setting = if kind == BlockKind::Plaza { Setting::Plaza } else { Setting::Park };
        let (species, blossom) = species(strip, setting, unit(h, 5), unit(h, 6));
        Self {
            s,
            x,
            h: KERB,
            size: 7.0 + 6.0 * unit(h, 3),
            half: 0.2,
            clear: DECOR_CLEAR,
            species,
            blossom,
            seed: h,
            ring_max: 3,
            limbs: false,
        }
    }

    fn code(&self) -> f32 {
        f32::from(self.species as u8 | u8::from(self.blossom) << 4) / 255.0
    }
}

/// A tree's crown, local to its foot (`(across, up, along)`): its ellipsoid, the blobs that make
/// it, its fork and its top.
struct Crown {
    mid: Vec3,
    r: Vec3,
    blobs: [(Vec3, Vec3); 6],
    n: usize,
    fork: f32,
    top: f32,
}

impl Crown {
    /// Grown by its habit, no wider than `MAX_REACH` of its size, and clear of the lanterns round it.
    fn grow(h: &Habit, t: &TreeAt, lanterns: &[(f32, f32, f32)]) -> Crown {
        use std::f32::consts::TAU;
        let size = t.size;
        let reach = 1.0 + LUMP * 0.5;
        let base = (h.base * size).max(t.clear);
        let top = (h.top * size).max(base + 2.5);
        let (rh, rv) = (h.radius * size, (top - base) * 0.5);
        let mid = Vec3::new(0.0, base + rv, 0.0);
        let fork = (h.fork * size).max(if t.clear >= TREE_CLEAR { TRUNK + 0.6 } else { 2.2 }).min(base + 0.4);
        let mut c =
            Crown { mid, r: Vec3::new(rh, rv, rh), blobs: [(Vec3::ZERO, Vec3::ZERO); 6], n: 0, fork, top };
        let turn = unit(t.seed, 11) * TAU;
        let ring = h.ring.min(t.ring_max);
        if h.stacked {
            // A column (or a cone) of blobs up the crown, narrowing.
            for k in 0..ring {
                let f = (k as f32 + 0.5) / ring as f32;
                let w = rh * (1.15 - 0.5 * f) * (0.9 + 0.2 * unit(t.seed, 20 + k));
                let a = turn + k as f32 * 2.1;
                let at = Vec3::new(
                    a.cos() * rh * 0.15,
                    base + (top - base) * (0.12 + 0.76 * f),
                    a.sin() * rh * 0.15,
                );
                c.push(at, Vec3::new(w, (top - base) / ring as f32 * 0.8, w));
            }
            if h.offset {
                c.push(mid + Vec3::new(turn.cos(), 0.0, turn.sin()) * rh * 0.4, Vec3::new(rh, rv, rh) * 0.5);
            }
        } else {
            // One on top, a ring round it a little lower.
            c.push(mid + Vec3::Y * rv * 0.3, Vec3::new(rh, rv, rh) * 0.62);
            for k in 0..ring {
                let a = turn + TAU * k as f32 / ring as f32 + (unit(t.seed, 30 + k) - 0.5) * 0.8;
                let rho = rh * h.out * (0.8 + 0.4 * unit(t.seed, 40 + k));
                let y = mid.y + rv * (unit(t.seed, 50 + k) - 0.6) * 0.5;
                let s = 0.55 + 0.2 * unit(t.seed, 60 + k);
                c.push(Vec3::new(rho * a.cos(), y, rho * a.sin()), Vec3::new(rh * s, rv * s * 1.1, rh * s));
            }
        }
        // Nothing of it hangs below its clearance, lumps and all: the blobs rise to meet it.
        for b in &mut c.blobs[..c.n] {
            let low = b.0.y - b.1.y * reach;
            if low < t.clear {
                b.0.y += t.clear - low;
            }
        }
        c.cap(MAX_REACH * t.size);
        c.keep_clear(t, lanterns);
        c
    }

    fn push(&mut self, at: Vec3, r: Vec3) {
        if self.n < self.blobs.len() {
            self.blobs[self.n] = (at, r);
            self.n += 1;
        }
    }

    /// How far it reaches out from its trunk, lumps and all.
    fn reach(&self) -> f32 {
        self.blobs[..self.n]
            .iter()
            .map(|(c, r)| Vec2::new(c.x, c.z).length() + r.x.max(r.z) * (1.0 + 0.5 * LUMP))
            .fold(0.0, f32::max)
    }

    /// Its lowest leaf over its foot, lumps and all.
    fn bottom(&self) -> f32 {
        self.blobs[..self.n].iter().map(|(c, r)| c.y - r.y * (1.0 + 0.5 * LUMP)).fold(f32::MAX, f32::min)
    }

    /// Narrowed to reach no further than `most` across (its height kept).
    fn cap(&mut self, most: f32) {
        let reach = self.reach();
        if reach > most && reach > 0.0 {
            let k = most / reach;
            for b in &mut self.blobs[..self.n] {
                b.0.x *= k;
                b.0.z *= k;
                b.1.x *= k;
                b.1.z *= k;
            }
            self.r.x *= k;
            self.r.z *= k;
        }
    }

    /// Kept `LANTERN_CLEAR` from the lanterns round it (`(s, x, top)`): where one stands level with
    /// the crown (its top within the clearance of the lowest leaf, or higher), the crown reaches no
    /// nearer to it. Under a canopy that's high enough, nothing changes.
    fn keep_clear(&mut self, t: &TreeAt, lanterns: &[(f32, f32, f32)]) {
        let (reach, bottom) = (self.reach(), t.h + self.bottom());
        let mut most = reach;
        for &(ls, lx, top) in lanterns {
            let d = (ls - t.s).hypot(lx - t.x);
            if d < reach + LANTERN_CLEAR && top + LANTERN_CLEAR > bottom {
                most = most.min(d - LANTERN_CLEAR);
            }
        }
        self.cap(most.max(0.4 * reach));
    }

    /// The one blob that stands for it from further off (L1): its ellipsoid a little smaller, no
    /// further out than its blobs reach and no lower than its lowest leaf, lumps and all (so it
    /// keeps the clearances the whole crown keeps).
    fn far_blob(&self) -> (Vec3, Vec3) {
        let lump = 1.0 + 0.5 * LUMP;
        let across = self.reach() / lump;
        let r = Vec3::new((self.r.x * 0.95).min(across), self.r.y * 0.95, (self.r.z * 0.95).min(across));
        (Vec3::new(self.mid.x, self.mid.y.max(self.bottom() + r.y * lump), self.mid.z), r)
    }
}

/// A park's paths as `city_lib.wgsl`'s `paint_park` paints them (its `PARK_*`, checked by
/// `city_atlas`'s tests): the loop `furniture::PARK_LOOP` in from the pavement and its half-width,
/// the diagonals' half-width, the round plaza in the middle and its flower beds' outer edge (the
/// trees keep off the beds, so off the plaza inside them).
pub(crate) const PARK_PATH: f32 = 1.5;
pub(crate) const PARK_DIAG: f32 = 1.2;
#[cfg(test)]
pub(crate) const PARK_PLAZA: f32 = 11.0;
pub(crate) const PARK_BEDS: f32 = 14.0;
/// Trees a park has close up; further off, the first of them (the same trees). A plaza's.
const PARK_TREES: u32 = 30;
const PARK_TREES_FAR: u32 = 14;
const PLAZA_TREES: u32 = 6;
/// A decoration tree stands this far from a lamp and from a pavilion's or monument's foot.
const DECOR_SPACE: f32 = 6.0;

/// Whether nothing built and no lamp is within `DECOR_SPACE` of `(s, x)`.
fn clear_of(all: &[Building], lanterns: &[(f32, f32, f32)], s: f32, x: f32) -> bool {
    all.iter().all(|p| !p.foot.inset(-DECOR_SPACE).contains(s, x))
        && lanterns.iter().all(|(ls, lx, _)| (ls - s).hypot(lx - x) > DECOR_SPACE)
}

/// Whether a park tree may stand at `(s, x)`: on its lawns, off every path, clear of its pavilions
/// and lamps.
fn on_a_lawn(b: &BlockInfo, all: &[Building], lanterns: &[(f32, f32, f32)], s: f32, x: f32) -> bool {
    let r = b.rect;
    let lr = r.inset(SIDEWALK + furniture::PARK_LOOP);
    // Inside the loop's line (negative outside), as the paint's `inside()`.
    let dl = (s - lr.s0).min(lr.s1 - s).min(x - lr.x0).min(lr.x1 - x);
    let (ms, mx) = r.middle();
    let (qs, qx) = (s - ms, x - mx);
    let d = Vec2::new(lr.width(), lr.length()).normalize();
    let diag = (qs * d.y - qx * d.x).abs().min((qs * d.y + qx * d.x).abs());
    r.inset(SIDEWALK + 1.0).contains(s, x)
        && dl.abs() > PARK_PATH + 1.5
        && (dl < 0.0 || diag > PARK_DIAG + 1.0)
        && qs.hypot(qx) > PARK_BEDS + 1.5
        && clear_of(all, lanterns, s, x)
}

/// A park's or a plaza's trees (decoration, walked through): each of the first `count` tries three
/// places and takes the first it may stand on (or none). The same trees close up and further off.
fn decor_trees(m: &mut Builder, b: &BlockInfo, all: &[Building], lanterns: &[(f32, f32, f32)], lod: u8) {
    let (count, area) = match b.kind {
        BlockKind::Park => (if lod == 0 { PARK_TREES } else { PARK_TREES_FAR }, b.rect.inset(SIDEWALK + 1.0)),
        BlockKind::Plaza => (PLAZA_TREES, b.rect.inset(SIDEWALK + 2.0)),
        _ => return,
    };
    for i in 0..count {
        for k in 0..3u32 {
            let h = mix(b.seed, 31 + 7 * k, i);
            let (s, x) = (area.s0 + area.width() * unit(h, 1), area.x0 + area.length() * unit(h, 2));
            let ok = if b.kind == BlockKind::Park {
                on_a_lawn(b, all, lanterns, s, x)
            } else {
                clear_of(all, lanterns, s, x)
            };
            if ok {
                let t = TreeAt::decor(m.strip, b.kind, h, s, x);
                if lod == 0 {
                    m.tree_whole(&t, lanterns);
                } else {
                    m.tree_far(&t, lanterns);
                }
                break;
            }
        }
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
        // Its height fraction of the tallest, which the hall's trim is painted from.
        TERMINAL_HEIGHT / MAX_HEIGHT,
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
    use bc_sim::colony::city::{CANAL_ROW, CITY, HUB_GATE, SITE};
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
            ChunkKey::of(1, 1, 60, CANAL_ROW),
            ChunkKey::of(2, 2, 248, CANAL_ROW),
        ] {
            let m = chunk(key, Stage(0));
            assert!(!m.is_empty(), "{key:?} is empty");
            assert_eq!(m.positions.len(), m.normals.len());
            assert_eq!(m.positions.len(), m.uvs.len());
            assert_eq!(m.positions.len(), m.colors.len());
            let r = key.draw_rect();
            for (p, n) in m.positions.iter().zip(&m.normals) {
                assert!(p.iter().all(|v| v.is_finite()));
                assert!((Vec3::from_array(*n).length() - 1.0).abs() < 1e-3, "{n:?}");
                let c = back(key, *p);
                assert_eq!(c.strip, key.strip);
                // Within where the chunk draws (trees' crowns and lamps' arms reach a little over),
                // and no taller than anything is.
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
        // A tower's block, the Exchange floor's (its room's walls face into the room), the avenue's
        // furniture, and the canal's at every level. (Not the trees: their normals are soft, leaning
        // out from the crown and round the trunk, and each blob is wound by its own middle.)
        let canal = (0..4).map(|lod| ChunkKey::of(1, lod, 60, CANAL_ROW));
        let near = [ChunkKey::of(0, 0, 30, -2), ChunkKey::of(0, 0, 10, 2), ChunkKey::of(0, 0, 30, 1)];
        for key in near.into_iter().chain(canal) {
            let m = chunk(key, Stage(0));
            for t in m.indices.chunks(3) {
                let surface = (m.colors[t[0] as usize][0] * 255.0).round() as u8;
                if surface == Surface::Tree as u8 || surface == Surface::Trunk as u8 {
                    continue;
                }
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
    fn the_skyline_is_drawn_with_its_crowns_and_masts() {
        // The Axis View tower close up: its podium's walls carry the podium's own roof line (the
        // facade's coping there), its crown is drawn as a crown, its mast as steel to 240 m. Round
        // it, roofs have their plant. From afar the crown and mast stay (L2), then the mast (L3);
        // the plant goes.
        let surfaces = |m: &CityMesh| {
            let mut seen = [false; 64];
            for c in &m.colors {
                seen[(c[0] * 255.0).round() as usize] = true;
            }
            seen
        };
        let key = ChunkKey::of(0, 0, 30, -2);
        let near = chunk(key, Stage(0));
        let s = surfaces(&near);
        assert!(s[Surface::Crown as usize] && s[Surface::Steel as usize] && s[Surface::Glass as usize]);
        // Its podium: four storeys.
        let podium = (bc_sim::colony::city::GROUND_FLOOR + 3.0 * bc_sim::colony::city::FLOOR) / MAX_HEIGHT;
        assert!(
            near.colors
                .iter()
                .any(|c| (c[0] * 255.0).round() as u8 == Surface::Wall as u8 && (c[3] - podium).abs() < 1e-4),
            "the podium's walls end at its own roof"
        );
        let wide = surfaces(&chunk(ChunkKey::of(0, 1, 30, -2), Stage(0)));
        assert!(wide[Surface::Plant as usize], "plant on the roofs round it");
        let l2 = surfaces(&chunk(ChunkKey::of(0, 2, 30, -2), Stage(0)));
        assert!(l2[Surface::Crown as usize] && l2[Surface::Steel as usize] && !l2[Surface::Plant as usize]);
        let l3 = surfaces(&chunk(ChunkKey::of(0, 3, 30, -2), Stage(0)));
        assert!(l3[Surface::Steel as usize] && !l3[Surface::Crown as usize] && !l3[Surface::Plant as usize]);
    }

    #[test]
    fn levels_get_lighter_and_stay_in_budget() {
        // The busiest chunks: the business district at every level; and close up and further off,
        // where the trees are (Central Park's district, the Gardens' parks, the canal).
        let mut worst = [(0usize, None::<ChunkKey>); 4];
        let mut weigh = |key: ChunkKey| {
            let n = chunk(key, Stage(0)).triangles();
            let w = &mut worst[key.lod as usize];
            if n > w.0 {
                *w = (n, Some(key));
            }
        };
        for lod in 0..4u8 {
            let n = 2 << lod;
            for i in (CITY.0 / n)..(CITY.0 / n + 64 / n) {
                for j in 0..(24 / n) {
                    weigh(ChunkKey { strip: 0, lod, i, j });
                }
            }
        }
        for lod in 0..2u8 {
            let n = 2 << lod;
            for (strip, (b0, b1), rows) in [
                (0u8, (96, 128), (-ROWS, ROWS)),
                (2, (32, 64), (-ROWS, ROWS)),
                (2, (112, 144), (-ROWS, ROWS)),
                (1, (0, 64), (3, 5)),
            ] {
                let (j0, j1) = (row_index(rows.0) / n, row_index(rows.1) / n);
                for i in b0 / n..b1 / n {
                    for j in j0..=j1 {
                        weigh(ChunkKey { strip, lod, i, j });
                    }
                }
            }
        }
        let budget = [25_000, 12_000, 8_000, 6_000];
        for lod in 0..4 {
            assert!(worst[lod].0 <= budget[lod], "L{lod}: {:?} (worst per level: {worst:?})", worst[lod]);
        }
        // Per block, each level is lighter than the one before.
        let per_block = |lod: usize| worst[lod].0 as f32 / ((2 << lod) * (2 << lod)) as f32;
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

    /// Where triangle `t` (corners `[u, v, w]`) lies over `(u, v)`: its `w` there.
    fn over(t: &[[f32; 3]; 3], u: f32, v: f32) -> Option<f32> {
        let [a, b, c] = t;
        let d = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if d.abs() < 1e-6 {
            return None;
        }
        let l1 = ((u - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (v - a[1])) / d;
        let l2 = ((b[0] - a[0]) * (v - a[1]) - (u - a[0]) * (b[1] - a[1])) / d;
        let l0 = 1.0 - l1 - l2;
        (l0.min(l1).min(l2) > -1e-4).then(|| l0 * a[2] + l1 * b[2] + l2 * c[2])
    }

    /// A chunk's faces of `surface` in city coordinates (corners `[s, x, h]`), and which way each
    /// faces in them.
    fn faces_of(key: ChunkKey, m: &CityMesh, surface: Surface) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        let city = |p: Vec3| {
            let c = back(key, p.to_array());
            [c.s, c.x, c.h]
        };
        m.indices
            .chunks(3)
            .filter(|t| (m.colors[t[0] as usize][0] * 255.0).round() as u8 == surface as u8)
            .map(|t| {
                let p = |k: usize| Vec3::from_array(m.positions[t[k] as usize]);
                let (a, out) = (city(p(0)), city(p(0) + Vec3::from_array(m.normals[t[0] as usize])));
                ([0, 1, 2].map(|k| city(p(k))), [0, 1, 2].map(|k| out[k] - a[k]))
            })
            .collect()
    }

    #[test]
    fn the_canal_is_water_at_every_level() {
        // Down every strip's canal at every level (the ground leaves its channel open), the channel
        // is water from grid line to grid line, walled from under the water to the street on both
        // sides and closed at its ends, all facing in: nowhere does it show the clear colour through.
        let keys = (0..3u8).flat_map(|strip| {
            (0..4u8).flat_map(move |lod| {
                let n = 2 << lod;
                (HUB_GATE.0 / n..=SITE.1 / n).map(move |i| ChunkKey::of(strip, lod, i * n, CANAL_ROW))
            })
        });
        for key in keys {
            let m = chunk(key, Stage(0));
            let (water, walls) = (faces_of(key, &m, Surface::Water), faces_of(key, &m, Surface::CanalWall));
            // Whether a wall facing `sign` along coordinate `w` (0 s, 1 x) stands at `want` there,
            // over `(u, v)` in the other two.
            let walled = |w: usize, sign: f32, want: f32, u: f32, v: f32| {
                walls.iter().any(|(t, out)| {
                    let t = t.map(|c| if w == 0 { [c[1], c[2], c[0]] } else { [c[0], c[2], c[1]] });
                    out[w] * sign > 0.9 && over(&t, u, v).is_some_and(|at| (at - want).abs() < 0.05)
                })
            };
            let ((b0, b1), _) = key.blocks();
            for bx in b0..b1 {
                let Some(b) = block(key.strip, bx, CANAL_ROW, Stage(0)) else { continue };
                assert_eq!(b.kind, BlockKind::Canal, "{key:?}: block {bx}");
                let ch = channel(&b.rect);
                let across = |k: usize| ch.s0 + 0.5 + (ch.width() - 1.0) * k as f32 / 8.0;
                // Along the block, and on under the bridges either end.
                for x in (0..=16).map(|k| grid_x(bx) + 0.5 + (BLOCK - 1.0) * k as f32 / 16.0) {
                    for s in (0..=8).map(across) {
                        let h = water.iter().find_map(|(t, out)| over(t, s, x).filter(|_| out[2] > 0.9));
                        assert!(
                            h.is_some_and(|h| (h - WATER).abs() < 0.1),
                            "{key:?}: no water at block {bx} ({s}, {x}): {h:?}"
                        );
                    }
                    for (side, sign) in [(ch.s0, 1.0), (ch.s1, -1.0)] {
                        for h in [WATER - 0.5, WATER + 0.05, -0.05] {
                            assert!(walled(0, sign, side, x, h), "{key:?}: open side at {side}, {x}, {h} up");
                        }
                    }
                }
                for (end, x, sign) in
                    [(bx == HUB_GATE.0, grid_x(bx), 1.0), (bx == SITE.1, grid_x(bx + 1), -1.0)]
                {
                    if !end {
                        continue;
                    }
                    for s in (0..=8).map(across) {
                        for h in [WATER - 0.5, WATER + 0.05, -0.05] {
                            assert!(
                                walled(1, sign, x, s, h),
                                "{key:?}: the end at {x} is open at {s}, {h} up"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_canal_is_the_same_at_every_level() {
        // While a chunk swaps for its children both show: what a coarser level draws of the canal,
        // L0 draws the same (where, UVs, colour), so nothing fights or pops.
        type Face = ([glam::DVec3; 3], [[f32; 2]; 3], [[f32; 4]; 3]);
        let canal = |key: ChunkKey, x0: f64, x1: f64| -> Vec<Face> {
            let m = chunk(key, Stage(0));
            let o = key.anchor().to_colony().as_dvec3();
            m.indices
                .chunks(3)
                .filter(|t| {
                    let s = (m.colors[t[0] as usize][0] * 255.0).round() as u8;
                    s == Surface::Water as u8 || s == Surface::CanalWall as u8
                })
                .map(|t| {
                    let v = [t[0], t[1], t[2]].map(|i| i as usize);
                    (
                        v.map(|i| o + Vec3::from_array(m.positions[i]).as_dvec3()),
                        v.map(|i| m.uvs[i]),
                        v.map(|i| m.colors[i]),
                    )
                })
                .filter(|(p, _, _)| {
                    (p[0].x + p[1].x + p[2].x) / 3.0 > x0 && (p[0].x + p[1].x + p[2].x) / 3.0 < x1
                })
                .collect()
        };
        for bx in [4, 60, 64, 248] {
            let near = ChunkKey::of(1, 0, bx, CANAL_ROW);
            let ((b0, b1), _) = near.blocks();
            let (x0, x1) = (f64::from(grid_x(b0)), f64::from(grid_x(b1)));
            let fine = canal(near, x0, x1);
            for lod in 1..4 {
                for (p, uv, col) in canal(ChunkKey::of(1, lod, bx, CANAL_ROW), x0, x1) {
                    let twin = fine.iter().any(|(q, quv, qcol)| {
                        (0..3).all(|i| {
                            (p[i] - q[i]).length() < 1e-3
                                && (uv[i][0] - quv[i][0]).abs() < 1e-3
                                && (uv[i][1] - quv[i][1]).abs() < 1e-3
                                && (0..4).all(|j| (col[i][j] - qcol[i][j]).abs() < 1e-4)
                        })
                    });
                    assert!(twin, "block {bx}, L{lod}: a canal face L0 doesn't draw the same: {p:?}");
                }
            }
        }
    }

    /// A chunk's street: the furniture it draws, all of it round the chunk, the lanterns' tops.
    type Street = (Vec<Furniture>, Vec<Furniture>, Vec<(f32, f32, f32)>);

    /// The street furniture chunk `key` draws (what stands in its draw rect, as `street` picks it),
    /// all of it round the chunk as `chunk` gathers it, and the lanterns' tops round it.
    fn street_of(key: ChunkKey) -> Street {
        let r = key.draw_rect();
        let mut all = Vec::new();
        each_furniture(key.strip, &r.inset(-LAMP_REACH), Stage(0), |p| {
            all.push(*p);
            false
        });
        let drawn = all.iter().filter(|p| r.s0 <= p.s && p.s < r.s1 && r.x0 <= p.x && p.x < r.x1).copied();
        let lanterns = all.iter().filter_map(lantern_top).collect();
        (drawn.collect(), all, lanterns)
    }

    /// A chunk's vertices of `surface`, in city coordinates, with their UVs and colours.
    fn verts_of(key: ChunkKey, m: &CityMesh, surface: Surface) -> Vec<(CityPos, [f32; 2], [f32; 4])> {
        (0..m.positions.len())
            .filter(|&i| (m.colors[i][0] * 255.0).round() as u8 == surface as u8)
            .map(|i| (back(key, m.positions[i]), m.uvs[i], m.colors[i]))
            .collect()
    }

    /// Whether `c` lies on a face of box `b` (within `tol`).
    fn on_box(b: &CityBox, c: &CityPos, tol: f32) -> bool {
        let r = b.rect;
        let d = [c.s - r.s0, r.s1 - c.s, c.x - r.x0, r.x1 - c.x, c.h - b.h0, b.h1 - c.h];
        d.iter().all(|d| *d > -tol) && d.iter().any(|d| d.abs() < tol)
    }

    #[test]
    fn the_furniture_is_drawn_where_its_walked() {
        // What's drawn is what's walked: down to over anyone's head, a post or a bench is drawn as
        // exactly its solid (people and their cars bump into what they see), and a tree's trunk
        // stands on its solid's faces. The avenue's (from the row beside it), a lane's, a canal
        // block's, a park's, a plaza's, the bank road's far kerb.
        use Furn::*;
        for (key, kinds) in [
            (ChunkKey::of(0, 0, 30, 1), &[StreetLamp, AvenueLamp, Tree, Bench][..]),
            (ChunkKey::of(0, 0, 20, -3), &[StreetLamp]),
            (ChunkKey::of(1, 0, 60, CANAL_ROW), &[StreetLamp, PathLamp, Tree]),
            (ChunkKey::of(0, 0, 104, 6), &[StreetLamp, PathLamp]),
            (ChunkKey::of(0, 0, 11, -1), &[AvenueLamp, PlazaLamp, Tree, Bench]),
            (ChunkKey::of(1, 0, 40, -12), &[StreetLamp]),
        ] {
            let m = chunk(key, Stage(0));
            let (drawn, _, _) = street_of(key);
            for k in kinds {
                assert!(drawn.iter().any(|p| p.kind == *k), "{key:?}: no {k:?}");
            }
            let mut low = verts_of(key, &m, Surface::Post);
            low.extend(verts_of(key, &m, Surface::Bench));
            low.retain(|(c, _, _)| c.h < 2.6);
            for (c, _, _) in &low {
                assert!(
                    drawn.iter().any(|p| p.kind != Tree && on_box(&p.solid, c, 5e-3)),
                    "{key:?}: a post or bench at {c:?} on no piece's solid"
                );
            }
            let trunks = verts_of(key, &m, Surface::Trunk);
            for p in &drawn {
                let b = p.solid;
                if p.kind != Tree {
                    let r = b.rect;
                    for (s, x) in [(r.s0, r.x0), (r.s1, r.x0), (r.s1, r.x1), (r.s0, r.x1)] {
                        let near = |c: &CityPos| {
                            (c.s - s).abs() < 5e-3 && (c.x - x).abs() < 5e-3 && (c.h - b.h0).abs() < 5e-3
                        };
                        assert!(
                            low.iter().any(|(c, _, _)| near(c)),
                            "{key:?}: {p:?} lacks its corner {s}, {x}"
                        );
                    }
                    continue;
                }
                // A tree: its trunk's foot on its solid's faces, touching all four.
                let foot: Vec<&CityPos> = trunks
                    .iter()
                    .map(|(c, _, _)| c)
                    .filter(|c| (c.h - p.h).abs() < 0.01 && b.rect.inset(-0.05).contains(c.s, c.x))
                    .collect();
                assert!(!foot.is_empty(), "{key:?}: {p:?} has no trunk");
                for c in &foot {
                    let r = b.rect;
                    let d = [c.s - r.s0, r.s1 - c.s, c.x - r.x0, r.x1 - c.x];
                    assert!(d.iter().all(|d| *d > -5e-3), "{key:?}: {c:?} outside {b:?}");
                }
                let lo_hi = |f: fn(&CityPos) -> f32| {
                    foot.iter().map(|c| f(c)).fold((f32::MAX, f32::MIN), |a, v| (a.0.min(v), a.1.max(v)))
                };
                let ((s0, s1), (x0, x1)) = (lo_hi(|c| c.s), lo_hi(|c| c.x));
                let r = b.rect;
                assert!(
                    (s0 - r.s0).abs() < 5e-3
                        && (s1 - r.s1).abs() < 5e-3
                        && (x0 - r.x0).abs() < 5e-3
                        && (x1 - r.x1).abs() < 5e-3,
                    "{key:?}: {p:?}'s trunk doesn't touch its solid's four faces"
                );
            }
        }
    }

    #[test]
    fn trees_stand_on_their_trunks_with_their_crowns_up_and_clear_of_the_lamps() {
        // An avenue stretch on each strip (both sides), a canal block, two parks (in the second a
        // tree's crown would reach a lamp unless narrowed), a plaza, and an avenue chunk further
        // off: the rules' trees keep every leaf out of reach and within their reach of their
        // trunks; every tree keeps its leaves clear of every lantern; no bark sticks out of a
        // trunk's solid within anyone's reach.
        for key in [
            ChunkKey::of(0, 0, 30, -1),
            ChunkKey::of(0, 0, 30, 1),
            ChunkKey::of(1, 0, 100, 1),
            ChunkKey::of(2, 0, 130, -1),
            ChunkKey::of(1, 0, 60, CANAL_ROW),
            ChunkKey::of(0, 0, 104, 6),
            ChunkKey::of(2, 0, 36, 11),
            ChunkKey::of(0, 0, 11, -1),
            ChunkKey::of(1, 1, 60, 1),
        ] {
            let m = chunk(key, Stage(0));
            let (_, all, lanterns) = street_of(key);
            let trees: Vec<&Furniture> = all.iter().filter(|p| p.kind == Furn::Tree).collect();
            let leaves = verts_of(key, &m, Surface::Tree);
            assert!(!leaves.is_empty(), "{key:?}: no trees");
            let dist = |t: &Furniture, c: &CityPos| (t.s - c.s).hypot(t.x - c.x);
            for (c, _, _) in &leaves {
                let near = trees.iter().filter(|t| dist(t, c) < 8.0);
                if let Some(t) = near.clone().min_by(|a, b| dist(a, c).total_cmp(&dist(b, c))) {
                    assert!(c.h >= t.h + TREE_CLEAR - 1e-3, "{key:?}: a leaf in reach at {c:?}");
                    assert!(
                        trees.iter().any(|t| dist(t, c) <= MAX_REACH * tree_size(t.seed) + 0.05),
                        "{key:?}: a leaf at {c:?} out of every trunk's reach"
                    );
                }
                for &(ls, lx, top) in &lanterns {
                    let d = (c.s - ls).hypot(c.x - lx);
                    assert!(
                        c.h >= top + LANTERN_CLEAR - 0.05 || d >= LANTERN_CLEAR - 0.05,
                        "{key:?}: a lantern at ({ls}, {lx}, {top}) in a crown at {c:?}"
                    );
                }
            }
            for (c, _, _) in verts_of(key, &m, Surface::Trunk) {
                for t in trees.iter().filter(|t| dist(t, &c) < 1.0) {
                    let b = t.solid;
                    assert!(
                        c.h >= b.h1 || b.rect.inset(-5e-3).contains(c.s, c.x),
                        "{key:?}: bark at {c:?} out of {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn lanterns_hang_where_their_lamps_are() {
        // Every lantern carries where its lamp hangs (the shader lights it as the ground's lamps
        // light that spot), hangs there, and says whether it always burns (a plaza's).
        for key in [
            ChunkKey::of(0, 0, 30, -2),
            ChunkKey::of(0, 0, 30, 1),
            ChunkKey::of(1, 0, 60, CANAL_ROW),
            ChunkKey::of(0, 0, 11, -1),
        ] {
            let m = chunk(key, Stage(0));
            let (drawn, _, _) = street_of(key);
            let lanterns = verts_of(key, &m, Surface::Lantern);
            assert!(!lanterns.is_empty(), "{key:?}: no lanterns");
            for (at, uv, col) in &lanterns {
                let lamp = drawn.iter().find(|p| {
                    p.lantern()
                        .is_some_and(|(ls, lx, _)| (ls - uv[0]).abs() < 1e-3 && (lx - uv[1]).abs() < 1e-2)
                });
                let p = lamp.unwrap_or_else(|| panic!("{key:?}: a lantern over no lamp's pool, {uv:?}"));
                let (ls, lx, lh) = p.lantern().unwrap();
                assert!(
                    (at.s - ls).hypot(at.x - lx) < 0.6 && at.h > lh - 0.7 && at.h < lh + LANTERN_HEIGHT + 0.7,
                    "{key:?}: a lantern at {at:?} away from {p:?}"
                );
                let always = (col[3] * 255.0).round() as u8 == LANTERN_ALWAYS;
                assert_eq!(always, p.kind == Furn::PlazaLamp, "{key:?}: {p:?}");
            }
        }
    }

    #[test]
    fn levels_draw_the_street_as_they_should() {
        // By the avenue: close up the furniture and the trees; further off the trees alone; from
        // afar none of it.
        let street = [Surface::Post, Surface::Lantern, Surface::Bench, Surface::Tree, Surface::Trunk];
        for lod in 0..4 {
            let m = chunk(ChunkKey::of(0, lod, 30, 1), Stage(0));
            let has = |s: Surface| m.colors.iter().any(|c| (c[0] * 255.0).round() as u8 == s as u8);
            let want: &[bool] = match lod {
                0 => &[true; 5],
                1 => &[false, false, false, true, true],
                _ => &[false; 5],
            };
            for (s, w) in street.iter().zip(want) {
                assert_eq!(has(*s), *w, "L{lod}: {s:?}");
            }
        }
    }
}
