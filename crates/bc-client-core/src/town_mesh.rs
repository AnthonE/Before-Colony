//! The ring's town as meshes (`docs/FREE_ROAM.md`, V1): one cell round the ring at a time
//! ([`cell`]), everything `bc_sim::colony::town` says stands in it and lies under it, bent round the
//! ring's floor. Every wall is one of the town's own boxes (`town::each_box`), so what's drawn is
//! what's walked into; what isn't solid there (trees, lamps, the frames' beams overhead, the ceiling)
//! is drawn only.
//!
//! **Its frame.** A cell's mesh is in the frame upright at the cell's start ([`bend`]: `x` across the
//! ring, `y` up towards the axis there, `−z` the way `t` grows), so its numbers stay small and the
//! client turns it round the view ([`seen_from`]).
//!
//! **What a face carries** for `bc-client`'s `town.wgsl`, in its vertices' colour, as the station
//! language's does (`station`): `r` its [`Surface`], `g` its paint (a facade's district and tint, a
//! road's [`Road`] class), `b` its flags (a facade's [`Style`]; [`DISTANT`] past the near cells), `a`
//! a seed; its UVs are metres.
//!
//! **At three distances** ([`Detail`]): near, everything, its faces cut in strips that follow the
//! floor's bend; further off, one quad a face (a tower's faces outside its podium only), the
//! windows and bands painted from its UVs alone; past about a kilometre, each building its box in
//! the haze. A piece of the town is a run of cells built as one mesh ([`CellBuild::range`]): a
//! cell, or the far tiers merged by four cells or a district.

use bc_sim::colony::town::{
    self, AVENUE, BOULEVARD_COLUMNS, BOULEVARD_HALF, Band, BlockId, BlockUse, CANAL, CELL, CELLS,
    CELLS_PER_DISTRICT, COLUMNS, Ground, LIFT_HALF_T, LIFT_HALF_X, LIFT_LINTEL, LIFT_PIER, MEDIAN_HALF,
    RIM_ROAD, RIM_WALK, ROOF, STREET, TownBox,
};
use bc_sim::content::town::{DistrictKind, Quarter};
use bc_sim::world::{COLONY_HALF_WIDTH as HALF, COLONY_RADIUS as R};
use glam::{Quat, Vec3};

use crate::piece_mesh::PieceMesh;
use crate::station::Sign;

/// What a face is (`r` of its vertices' colour, in 255ths): `town.wgsl`'s surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    /// A building's wall: panels, its storeys' windows, a ground floor; by its [`Style`].
    Facade = 1,
    /// A roof: plates inside a parapet.
    Roof = 2,
    /// A carriageway, `u` across it from its middle and `v` along: lanes by its [`Road`] class.
    Road = 3,
    /// Where roads meet: plain.
    Junction = 4,
    /// Pavement, a quay, the rim walks.
    Pavement = 5,
    /// The boulevard's median: the tram's rails.
    Median = 6,
    /// The canal's water.
    Water = 7,
    /// A park's grass.
    Grass = 8,
    /// A lift's square.
    Square = 9,
    /// A built block's own ground, between its buildings.
    Lot = 10,
    /// Flat paint: a railing, a post, a frame's steel.
    Trim = 11,
    /// A lift's portal: panels, the band, chevrons at its foot.
    Portal = 12,
    /// Light in its paint: a lift's door's frame, a lamp's head.
    Glow = 13,
    /// The ring's faces, rising from the rim walks to the ceiling.
    Rim = 14,
    /// The ceiling: its lattice, and the sky's light in its glass.
    Sky = 15,
    /// A tree's leaves.
    Foliage = 16,
    /// A key place's site, kept clear for it; the building site's ground.
    Site = 17,
    /// A lift's door's leaves.
    Door = 18,
}

/// A road's class (`g` of a [`Surface::Road`]'s colour): how wide it is and how it's marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Road {
    /// A cross street or a street between columns: 20 m, a lane or two each way.
    Street = 0,
    /// An avenue between districts: 40 m.
    Avenue = 1,
    /// One of the boulevard's carriageways, one way: 24 m.
    Boulevard = 2,
    /// A rim road: 30 m.
    Rim = 3,
}

impl Road {
    /// Half its carriageway's width, m.
    pub const fn half(self) -> f32 {
        match self {
            Road::Street => STREET * 0.5,
            Road::Avenue => AVENUE * 0.5,
            Road::Boulevard => (BOULEVARD_HALF - MEDIAN_HALF) * 0.5,
            Road::Rim => RIM_ROAD * 0.5,
        }
    }
}

/// How a facade is built (`b` of a [`Surface::Facade`]'s colour): its district's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Style {
    Civic = 0,
    /// The money's podium, and (with [`TOWER`]) its tower.
    Business = 1,
    Midtown = 2,
    Residential = 3,
    OldTown = 4,
    /// Sheds and depots.
    Works = 5,
}

/// A business block's tower rather than its podium (added to its [`Style`]).
pub const TOWER: u8 = 8;

/// How long a face's strips are round the ring, m: bent onto the floor they sag about a centimetre.
pub const STRIP: f32 = CELL / 8.0;
/// How far under the floor a wall's foot goes, so no chord of the floor shows under it, m.
const SINK: f32 = 0.3;
/// The canal's water under its quays, m.
const WATER: f32 = 1.4;
/// The ceiling's frames across the ring: one every this many metres of `x` (`town.wgsl` rules them).
pub const SKY_FRAME: f32 = 125.0;
/// Lamp posts along the boulevard's pavements: their height, m, and how far from the kerb they stand.
const LAMP_H: f32 = 9.0;
const LAMP_OFF: f32 = 1.2;
/// Trees in a park: one about every this many metres.
const TREE_PITCH: f32 = 21.0;

/// A point of the town in the frame upright where `dt` is 0: `x` across the ring, `dt` m round it
/// from there (the way `t` grows), `h` up from the floor.
pub fn bend(x: f32, dt: f32, h: f32) -> Vec3 {
    let phi = dt / R;
    let (s, c) = phi.sin_cos();
    // R − (R − h) cos φ, with 1 − cos φ as 2 sin²(φ/2): nothing is lost near the frame's middle.
    let half = (0.5 * phi).sin();
    Vec3::new(x, h * c + 2.0 * R * half * half, -(R - h) * s)
}

/// Up (towards the axis) `dt` m round from the frame's middle, and the way `t` grows there.
pub fn up(dt: f32) -> Vec3 {
    let (s, c) = (dt / R).sin_cos();
    Vec3::new(0.0, c, s)
}

pub fn along(dt: f32) -> Vec3 {
    let (s, c) = (dt / R).sin_cos();
    Vec3::new(0.0, s, -c)
}

/// Where something at `(x, s, h)` on the ring's floor is drawn, and how it's turned, seen from a
/// view upright at `view_s` (the deck's `s`, any turn of it): the short way round.
pub fn seen_from(view_s: f32, x: f32, s: f32, h: f32) -> (Vec3, Quat) {
    let dt = bc_sim::colony::deck::round_from(bc_sim::colony::deck::RING, view_s, s);
    (bend(x, dt, h), Quat::from_rotation_x(dt / R))
}

/// A vertex's colour: its surface, paint, flags and seed.
pub fn colour(surface: Surface, paint: u8, flags: u8, seed: u8) -> [f32; 4] {
    [f32::from(surface as u8), f32::from(paint), f32::from(flags), f32::from(seed)].map(|v| v / 255.0)
}

/// A facade's paint: its district's quarter (0 Charter, 1 Canal, 2 Gardens, 3 the Site) × 4, plus
/// its tint.
pub fn district_paint(d: u16) -> u8 {
    let def = town::district(d);
    let q = match def.quarter {
        Quarter::Charter => 0,
        Quarter::Canal => 1,
        Quarter::Gardens => 2,
        Quarter::Site => 3,
    };
    q * 4 + (def.tint & 3)
}

fn style_of(kind: DistrictKind) -> Style {
    match kind {
        DistrictKind::Civic => Style::Civic,
        DistrictKind::Business => Style::Business,
        DistrictKind::Midtown => Style::Midtown,
        DistrictKind::Residential | DistrictKind::Park => Style::Residential,
        DistrictKind::OldTown => Style::OldTown,
        DistrictKind::Works | DistrictKind::Site => Style::Works,
    }
}

/// A road's UVs, of where on the floor a corner is: `u` across it from its middle, `v` along.
#[derive(Clone, Copy)]
enum RoadUv {
    /// Along the ring, its middle at `mid` across it, `v` growing the way it's driven (`way`, ±1).
    Along { mid: f32, way: f32 },
    /// Across the ring, its middle the cell's edge at `boundary`.
    Across { boundary: f32 },
}

impl RoadUv {
    fn at(self, x: f32, t: f32) -> [f32; 2] {
        match self {
            RoadUv::Along { mid, way } => [(x - mid) * way, t * way],
            RoadUv::Across { boundary } => [t - boundary, -x],
        }
    }
}

/// A face's flag past the near cells (`b` of its colour): `town.wgsl` paints it the distant way.
pub const DISTANT: u8 = 128;
/// How far a wall standing on another box's roof goes down into it past the near cells, m: there a
/// face is one quad, and the longer chord of the roof under it sags further (0.7 m over a cell).
const SEAT: f32 = 0.8;

/// Builds a run of cells' mesh in its own frame: `t0` is where the frame is upright.
struct Builder {
    mesh: PieceMesh,
    t0: f32,
    /// How long a face's strips are round the ring: [`STRIP`] near, a cell further off (a face is
    /// then one quad).
    strip: f32,
    /// What every face's flags carry besides its own: [`DISTANT`] past the near cells.
    flag: u8,
    /// How far a wall standing on a roof goes down into it ([`SEAT`] past the near cells).
    seat: f32,
}

impl Builder {
    fn new(t0: f32, detail: Detail) -> Self {
        let near = detail == Detail::Near;
        Self {
            mesh: PieceMesh::default(),
            t0,
            strip: if near { STRIP } else { CELL },
            flag: if near { 0 } else { DISTANT },
            seat: if near { 0.0 } else { SEAT },
        }
    }

    /// A quad, its corners in the town's `(x, t, h)`, facing `n` (the town's `(x, t, h)` too), each
    /// with its UV.
    fn quad(&mut self, p: [[f32; 3]; 4], n: [f32; 3], uv: [[f32; 2]; 4], mut colour: [f32; 4]) {
        if self.flag != 0 {
            colour[2] = f32::from((colour[2] * 255.0).round() as u8 | self.flag) / 255.0;
        }
        let base = self.mesh.positions.len() as u32;
        let mut at = [Vec3::ZERO; 4];
        for (k, [x, t, h]) in p.into_iter().enumerate() {
            let dt = t - self.t0;
            at[k] = bend(x, dt, h);
            let normal = Vec3::X * n[0] + along(dt) * n[1] + up(dt) * n[2];
            self.mesh.positions.push(at[k].to_array());
            self.mesh.normals.push(normal.to_array());
            self.mesh.uvs.push(uv[k]);
            self.mesh.colors.push(colour);
        }
        // Wound to face its normal, whichever way its corners came.
        let mid = 0.5 * (p[0][1] + p[2][1]) - self.t0;
        let want = Vec3::X * n[0] + along(mid) * n[1] + up(mid) * n[2];
        let faces = (at[1] - at[0]).cross(at[2] - at[0]).dot(want) > 0.0;
        let order: [u32; 6] = if faces { [0, 1, 2, 0, 2, 3] } else { [0, 2, 1, 0, 3, 2] };
        self.mesh.indices.extend(order.map(|i| base + i));
    }

    /// The cuts of `t0..t1` into strips: its ends, and every `strip` of the ring between.
    fn strips(strip: f32, t0: f32, t1: f32, mut f: impl FnMut(f32, f32)) {
        let mut a = t0;
        loop {
            // (A cut that rounds to where this strip starts is the one after.)
            let mut next = ((a / strip).floor() + 1.0) * strip;
            if next <= a + 0.01 {
                next += strip;
            }
            let b = if next >= t1 - 0.01 { t1 } else { next };
            f(a, b);
            if b >= t1 {
                return;
            }
            a = b;
        }
    }

    /// A level face at `h`, looking up (or down), its UVs by `uv` of `(x, t)`.
    #[allow(clippy::too_many_arguments)]
    fn flat(
        &mut self,
        (x0, x1): (f32, f32),
        (t0, t1): (f32, f32),
        h: f32,
        up: bool,
        colour: [f32; 4],
        uv: impl Fn(f32, f32) -> [f32; 2],
    ) {
        if x1 - x0 < 1e-3 || t1 - t0 < 1e-3 {
            return;
        }
        let n = [0.0, 0.0, if up { 1.0 } else { -1.0 }];
        Self::strips(self.strip, t0, t1, |a, b| {
            self.quad(
                [[x0, a, h], [x1, a, h], [x1, b, h], [x0, b, h]],
                n,
                [uv(x0, a), uv(x1, a), uv(x1, b), uv(x0, b)],
                colour,
            );
        });
    }

    /// A wall along the ring at `x`, facing `face` (±1) across it, its UVs by `uv` of `(t, h)`.
    #[allow(clippy::too_many_arguments)]
    fn wall_t(
        &mut self,
        x: f32,
        (t0, t1): (f32, f32),
        (h0, h1): (f32, f32),
        face: f32,
        colour: [f32; 4],
        uv: impl Fn(f32, f32) -> [f32; 2],
    ) {
        if t1 - t0 < 1e-3 || h1 - h0 < 1e-3 {
            return;
        }
        Self::strips(self.strip, t0, t1, |a, b| {
            self.quad(
                [[x, a, h0], [x, b, h0], [x, b, h1], [x, a, h1]],
                [face, 0.0, 0.0],
                [uv(a, h0), uv(b, h0), uv(b, h1), uv(a, h1)],
                colour,
            );
        });
    }

    /// A wall across the ring at `t`, facing `face` (±1) round it, its UVs by `uv` of `(x, h)`.
    #[allow(clippy::too_many_arguments)]
    fn wall_x(
        &mut self,
        t: f32,
        (x0, x1): (f32, f32),
        (h0, h1): (f32, f32),
        face: f32,
        colour: [f32; 4],
        uv: impl Fn(f32, f32) -> [f32; 2],
    ) {
        if x1 - x0 < 1e-3 || h1 - h0 < 1e-3 {
            return;
        }
        self.quad(
            [[x0, t, h0], [x1, t, h0], [x1, t, h1], [x0, t, h1]],
            [0.0, face, 0.0],
            [uv(x0, h0), uv(x1, h0), uv(x1, h1), uv(x0, h1)],
            colour,
        );
    }

    /// A box's four walls (their UVs metres along each from its corner, and up from `base`) and
    /// its top.
    fn boxed(&mut self, b: &TownBox, walls: [f32; 4], top: Option<[f32; 4]>, base: f32) {
        let h = (b.h0 - if b.h0 <= 0.0 { SINK } else { 0.0 }, b.h1);
        self.wall_t(b.x0, (b.t0, b.t1), h, -1.0, walls, |t, y| [b.t1 - t, y - base]);
        self.wall_t(b.x1, (b.t0, b.t1), h, 1.0, walls, |t, y| [t - b.t0, y - base]);
        self.wall_x(b.t0, (b.x0, b.x1), h, -1.0, walls, |x, y| [x - b.x0, y - base]);
        self.wall_x(b.t1, (b.x0, b.x1), h, 1.0, walls, |x, y| [b.x1 - x, y - base]);
        if let Some(top) = top {
            self.flat((b.x0, b.x1), (b.t0, b.t1), b.h1, true, top, |x, t| [x - b.x0, t - b.t0]);
        }
    }

    /// A box with every face one colour and plain UVs (a post, a rail, a beam), its underside too
    /// if it's off the floor.
    fn solid(&mut self, b: &TownBox, colour: [f32; 4]) {
        self.boxed(b, colour, Some(colour), b.h0);
        if b.h0 > 0.0 {
            self.flat((b.x0, b.x1), (b.t0, b.t1), b.h0, false, colour, |x, t| [x, t]);
        }
    }

    /// A tree at `(x, t)`: a trunk, and two diamonds of leaves, `size` its crown's reach, m.
    fn tree(&mut self, x: f32, t: f32, size: f32, seed: u8) {
        let trunk = colour(Surface::Trim, 5, 0, seed);
        let leaves = colour(Surface::Foliage, 0, 0, seed);
        let w = 0.18;
        self.boxed(&TownBox::new(x - w, x + w, t - w, t + w, 0.0, size * 1.1), trunk, None, 0.0);
        for (r, lo, hi) in [(size, size * 0.8, size * 2.3), (size * 0.72, size * 1.7, size * 3.1)] {
            let mid = lo + (hi - lo) * 0.42;
            let ring = [[x - r, t, mid], [x, t - r, mid], [x + r, t, mid], [x, t + r, mid]];
            for k in 0..4 {
                let (a, b) = (ring[k], ring[(k + 1) % 4]);
                let out = [(a[0] + b[0]) * 0.5 - x, (a[1] + b[1]) * 0.5 - t];
                for (tip, nh) in [([x, t, hi], 0.6), ([x, t, lo], -0.6)] {
                    let n = Vec3::new(out[0], out[1], nh * r).normalize();
                    self.quad([a, b, tip, tip], n.to_array(), [[0.0, 0.0]; 4], leaves);
                }
            }
        }
    }
}

/// Most breaks across the ring: the bands' and each column's ends and insets.
const X_BREAKS: usize = 8 + 4 * COLUMNS as usize;

/// The breaks across the ring between one kind of ground and the next, fore to aft: the array's
/// first so many, and how many.
fn x_breaks() -> ([f32; X_BREAKS], usize) {
    let mut xs = [0.0; X_BREAKS];
    xs[..8].copy_from_slice(&[
        -HALF,
        -HALF + RIM_WALK,
        -HALF + RIM_WALK + RIM_ROAD,
        -MEDIAN_HALF,
        MEDIAN_HALF,
        HALF - RIM_WALK - RIM_ROAD,
        HALF - RIM_WALK,
        HALF,
    ]);
    for c in 0..COLUMNS {
        let (x0, x1, lo, hi) = town::column(c);
        let k = 8 + 4 * usize::from(c);
        xs[k..k + 4].copy_from_slice(&[x0, x0 + lo, x1 - hi, x1]);
    }
    xs.sort_unstable_by(f32::total_cmp);
    let mut n = 1;
    for k in 1..X_BREAKS {
        if (xs[k] - xs[n - 1]).abs() >= 1e-3 {
            xs[n] = xs[k];
            n += 1;
        }
    }
    (xs, n)
}

/// Whether the avenue at a cell's end (`first`: its start, else its end) is a lift's square's.
fn lift_avenue(cell: u16, first: bool) -> bool {
    let k = cell % CELLS_PER_DISTRICT;
    let d = cell / CELLS_PER_DISTRICT;
    if first {
        k == 0 && d.is_multiple_of(2)
    } else {
        k + 1 == CELLS_PER_DISTRICT && (d + 1).is_multiple_of(2)
    }
}

/// A stable byte from a block and a number.
fn seed(id: BlockId, k: u32) -> u8 {
    (town::mix(u32::from(id.cell), u32::from(id.column), 0xB10C + k) >> 9) as u8
}

/// How much of a cell to build: V1's three distances (`docs/FREE_ROAM.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Detail {
    /// Everything: trees, lamps, railings, the frames' beams; faces cut in strips round the ring so
    /// they follow the floor's bend.
    Near,
    /// Past the near cells: the ground, the buildings and what crowns them, the lifts, the faces and
    /// the ceiling, each face one quad (a tower's only over its podium), painted the distant way.
    Far,
    /// Past about a kilometre, in the haze: as [`Detail::Far`], but each building only its box.
    Haze,
}

impl Detail {
    /// The most vertices and indices a cell's mesh has at this detail, reserved before it's built
    /// (none near: a near cell is built once, and its props vary).
    pub const fn caps(self) -> (usize, usize) {
        match self {
            Detail::Near => (0, 0),
            Detail::Far => FAR_CAPS,
            Detail::Haze => HAZE_CAPS,
        }
    }
}

/// [`Detail::caps`]: the most any cell comes to (`tests::the_far_tiers_are_within_their_caps`), and a
/// little over.
const FAR_CAPS: (usize, usize) = (3_200, 4_800);
const HAZE_CAPS: (usize, usize) = (1_900, 2_850);

/// Cell `cell`'s mesh, in the frame upright at its start ([`cell_start`]).
pub fn cell(cell: u16, detail: Detail) -> PieceMesh {
    piece(cell, 1, detail)
}

/// The mesh of `count` cells from `first` as one, in the frame upright at `first`'s start.
pub fn piece(first: u16, count: u16, detail: Detail) -> PieceMesh {
    let mut build = CellBuild::range(first, count, detail);
    while !build.done() {
        build.step();
    }
    build.finish()
}

/// The parts a cell is built in: its floor, each column's buildings, the rest.
const PARTS: u32 = COLUMNS as u32 + 2;

/// A run of cells' mesh built a part at a time, so a frame's budget can stop between parts: each
/// cell's floor, each of its columns' buildings, then the rest (the railings, the lifts' feet, the
/// lamps and trees, the faces and the ceiling). Finished, one cell's is [`cell`]'s mesh exactly. Past
/// the near cells its buffers are reserved for the run up front ([`Detail::caps`]): its parts add
/// to them and allocate nothing.
pub struct CellBuild {
    first: u16,
    count: u16,
    detail: Detail,
    part: u32,
    b: Builder,
}

impl CellBuild {
    pub fn new(cell: u16, detail: Detail) -> Self {
        Self::range(cell, 1, detail)
    }

    /// `count` cells from `first`, up to the town's last (a run doesn't go round past its start).
    pub fn range(first: u16, count: u16, detail: Detail) -> Self {
        let first = first % CELLS;
        let count = count.clamp(1, CELLS - first);
        let mut b = Builder::new(cell_start(first), detail);
        let (verts, indices) = detail.caps();
        let n = usize::from(count);
        b.mesh.positions.reserve_exact(verts * n);
        b.mesh.normals.reserve_exact(verts * n);
        b.mesh.uvs.reserve_exact(verts * n);
        b.mesh.colors.reserve_exact(verts * n);
        b.mesh.indices.reserve_exact(indices * n);
        Self { first, count, detail, part: 0, b }
    }

    /// The first cell of the run.
    pub fn cell(&self) -> u16 {
        self.first
    }

    /// How many cells the run is.
    pub fn count(&self) -> u16 {
        self.count
    }

    pub fn detail(&self) -> Detail {
        self.detail
    }

    /// Whether every part is built.
    pub fn done(&self) -> bool {
        self.part >= u32::from(self.count) * PARTS
    }

    /// Builds the next part.
    pub fn step(&mut self) {
        if self.done() {
            return;
        }
        let cell = self.first + (self.part / PARTS) as u16;
        match self.part % PARTS {
            0 => cell_floor(&mut self.b, cell),
            p if p <= u32::from(COLUMNS) => cell_column(&mut self.b, cell, p as u8 - 1, self.detail),
            _ => cell_rest(&mut self.b, cell, self.detail),
        }
        self.part += 1;
    }

    pub fn finish(self) -> PieceMesh {
        self.b.mesh
    }
}

/// A cell's paint, and its lift squares' (a lift's square is its own district's, the one that starts
/// at the lift, either side of it).
fn cell_paints(cell: u16) -> (u8, u8) {
    let district = cell / CELLS_PER_DISTRICT;
    let paint = district_paint(district);
    let square_paint = if cell % CELLS_PER_DISTRICT + 1 == CELLS_PER_DISTRICT {
        district_paint((district + 1) % town::DISTRICT_COUNT)
    } else {
        paint
    };
    (paint, square_paint)
}

/// A cell's floor, a rectangle of it at a time: whatever the plan says it is.
fn cell_floor(b: &mut Builder, cell: u16) {
    let t0 = cell_start(cell);
    let t1 = t0 + CELL;
    let (paint, square_paint) = cell_paints(cell);
    let row = town::block_rect(BlockId::new(cell, 0));
    let ts = [t0, row.t0, row.t1, t1];
    let (xs, n) = x_breaks();
    for (j, tw) in ts.windows(2).enumerate() {
        let (ta, tb) = (tw[0], tw[1]);
        let tm = 0.5 * (ta + tb);
        // An end of the cell: half a cross street, or half an avenue.
        let end = j != 1;
        let wide = (tb - ta) > STREET * 0.5 + 1.0;
        let boundary = if j == 0 { t0 } else { t1 };
        let square = end && wide && lift_avenue(cell, j == 0);
        for xw in xs[..n].windows(2) {
            let (xa, xb) = (xw[0], xw[1]);
            let xm = 0.5 * (xa + xb);
            let span = ((xa, xb), (ta, tb));
            let plain = |x: f32, t: f32| [x, t];
            match town::ground_at(xm, tm) {
                Ground::Outside => {}
                Ground::RimWalk => {
                    b.flat(span.0, span.1, 0.0, true, colour(Surface::Pavement, 1, 0, 0), plain)
                }
                Ground::Pavement => {
                    b.flat(span.0, span.1, 0.0, true, colour(Surface::Pavement, 0, 0, 0), plain)
                }
                Ground::Median => b.flat(span.0, span.1, 0.0, true, colour(Surface::Median, 0, 0, 0), plain),
                Ground::Water => {
                    b.flat(span.0, span.1, -WATER, true, colour(Surface::Water, 0, 0, 0), plain);
                    let quay = colour(Surface::Pavement, 0, 0, 0);
                    b.wall_t(xa, span.1, (-WATER - 0.5, 0.0), 1.0, quay, |t, h| [t, h]);
                    b.wall_t(xb, span.1, (-WATER - 0.5, 0.0), -1.0, quay, |t, h| [t, h]);
                }
                Ground::Block(id) => {
                    let (surface, p) = match town::block_use(id) {
                        BlockUse::Built => (Surface::Lot, paint),
                        BlockUse::Park => (Surface::Grass, 0),
                        BlockUse::Square => (Surface::Square, square_paint),
                        BlockUse::Place(_) => (Surface::Site, 0),
                        BlockUse::Frames => (Surface::Site, 1),
                    };
                    let r = town::block_rect(id);
                    let (mx, mt) = (0.5 * (r.x0 + r.x1), 0.5 * (r.t0 + r.t1));
                    b.flat(span.0, span.1, 0.0, true, colour(surface, p, 0, seed(id, 0)), |x, t| {
                        [x - mx, t - mt]
                    });
                }
                Ground::Road => {
                    let band = town::band_at(xm);
                    let (class, u_of) = match band {
                        Band::RimRoad => {
                            let mid = (HALF - RIM_WALK - 0.5 * RIM_ROAD).copysign(xm);
                            (Road::Rim, RoadUv::Along { mid, way: 1.0 })
                        }
                        Band::Boulevard => {
                            let mid = (0.5 * (BOULEVARD_HALF + MEDIAN_HALF)).copysign(xm);
                            // Each carriageway runs one way: its `v` grows the way it's driven.
                            let way = if xm < 0.0 { -1.0 } else { 1.0 };
                            (Road::Boulevard, RoadUv::Along { mid, way })
                        }
                        _ if !end => {
                            // A street between two columns, along the ring: its middle the columns' line.
                            let mid = match band {
                                Band::Blocks(c) => {
                                    let (x0, x1, ..) = town::column(c);
                                    if (xm - x0).abs() < (xm - x1).abs() { x0 } else { x1 }
                                }
                                _ => xm,
                            };
                            (Road::Street, RoadUv::Along { mid, way: 1.0 })
                        }
                        _ => {
                            // A cross street or an avenue, across the ring: its middle the cell's edge.
                            let class = if wide { Road::Avenue } else { Road::Street };
                            (class, RoadUv::Across { boundary })
                        }
                    };
                    // Where a street between columns meets the cross street, a junction; and round a
                    // lift, the avenue is its square's.
                    let between = match band {
                        Band::Blocks(c) => {
                            let r = town::block_rect(BlockId::new(cell, c));
                            let (_, _, lo, hi) = town::column(c);
                            (xm < r.x0 && lo == STREET * 0.5) || (xm > r.x1 && hi == STREET * 0.5)
                        }
                        _ => false,
                    };
                    let in_square = square
                        && xa >= town::column(BOULEVARD_COLUMNS.0).0 - 1e-3
                        && xb <= town::column(BOULEVARD_COLUMNS.1).1 + 1e-3
                        && matches!(band, Band::Blocks(_));
                    let c = if in_square {
                        colour(Surface::Square, square_paint, 0, 0)
                    } else if end && between {
                        colour(Surface::Junction, 0, 0, 0)
                    } else {
                        colour(Surface::Road, class as u8, 0, 0)
                    };
                    if in_square {
                        b.flat(span.0, span.1, 0.0, true, c, |x, t| [x, t - boundary]);
                    } else {
                        b.flat(span.0, span.1, 0.0, true, c, |x, t| u_of.at(x, t));
                    }
                }
            }
        }
    }
}

/// The buildings of a cell's column `c`: near and further off what crowns them; past the near
/// cells, a tower only over its podium (the rest of it is inside).
fn cell_column(b: &mut Builder, cell: u16, c: u8, detail: Detail) {
    let near = detail == Detail::Near;
    let style = style_of(town::district(cell / CELLS_PER_DISTRICT).kind);
    let (paint, _) = cell_paints(cell);
    let id = BlockId::new(cell, c);
    let used = town::block_use(id);
    let mut k = 0u32;
    let mut podium = 0.0;
    town::each_block_box(id, &mut |bx| {
        let s = seed(id, k + 1);
        match used {
            BlockUse::Frames => {
                // A frame's columns and a crane's mast: steel, in safety paint (in the haze, the
                // masts alone: the columns are thinner than a pixel there).
                let mast = bx.x1 - bx.x0 > 2.0;
                if mast || detail != Detail::Haze {
                    b.solid(&bx, colour(Surface::Trim, if mast { 4 } else { 6 }, 0, s));
                }
            }
            _ => {
                let tower = style == Style::Business && k == 1;
                let flags = style as u8 + if tower { TOWER } else { 0 };
                let paint = building_paint(paint, s);
                if k == 0 {
                    podium = bx.h1;
                }
                // (Its storeys still counted from the floor.)
                let walls = if tower && !near { TownBox { h0: podium - b.seat, ..bx } } else { bx };
                b.boxed(
                    &walls,
                    colour(Surface::Facade, paint, flags, s),
                    Some(colour(Surface::Roof, paint, 0, s)),
                    0.0,
                );
                if !tower && detail != Detail::Haze {
                    crown(b, &bx, paint, flags, s);
                }
            }
        }
        k += 1;
    });
    if near && used == BlockUse::Frames {
        frame_beams(b, id);
    }
    if near && used == BlockUse::Park {
        park_trees(b, id);
    }
}

/// The rest of a cell: the canal's railings, a lift's foot, the lamps and trees, the ring's faces and
/// the ceiling.
fn cell_rest(b: &mut Builder, cell: u16, detail: Detail) {
    let near = detail == Detail::Near;
    let t0 = cell_start(cell);
    let t1 = t0 + CELL;
    let (paint, _) = cell_paints(cell);
    // The canal's railings.
    if near {
        town::each_rail(cell, &mut |r| b.solid(&r, colour(Surface::Trim, 9, 0, 0)));
    }

    // A lift's foot, in the cells either side of it.
    for k in 0..town::LIFTS {
        let lt = town::lift_t(k);
        for turn in [-town::ROUND, 0.0, town::ROUND] {
            let at = lt + turn;
            if at + LIFT_HALF_T < t0 - 1.0 || at - LIFT_HALF_T > t1 + 1.0 {
                continue;
            }
            // Each cell draws the half of it on its own side of the avenue's middle.
            let (lo, hi) = if at <= t0 + 1.0 { (at, at + LIFT_HALF_T) } else { (at - LIFT_HALF_T, at) };
            lift(b, k, turn, (lo, hi), near);
        }
    }

    // Lamps along the boulevard's pavements, and trees along the rim walks.
    if near {
        for (x, t) in lamps(cell) {
            lamp(b, x, t, -x.signum());
        }
        for (x, t, size, seed) in rim_trees(cell) {
            b.tree(x, t, size, seed);
        }
    }

    // The ring's faces, from the rim walks up to the ceiling; and the ceiling.
    for side in [-1.0f32, 1.0] {
        b.wall_t(side * HALF, (t0, t1), (-SINK, ROOF), -side, colour(Surface::Rim, paint, 0, 0), |t, h| {
            [t, h]
        });
    }
    b.flat((-HALF, HALF), (t0, t1), ROOF, false, colour(Surface::Sky, 0, 0, 0), |x, t| [x, t]);
}

/// The signs stencilled in cell `cell`, in its mesh's frame: on a lift's shaft over the boulevard,
/// the gate's name (Hub Gate's; the others take their district's) and the district it opens on,
/// read coming up the boulevard either way; and over its door, where it goes.
pub fn cell_signs(cell: u16) -> Vec<Sign> {
    piece_signs(cell, 1)
}

/// The signs stencilled in `count` cells from `first`, in their mesh's frame (`piece`'s).
pub fn piece_signs(first: u16, count: u16) -> Vec<Sign> {
    let first = first % CELLS;
    let mut signs = Vec::new();
    for cell in first..first + count.clamp(1, CELLS - first) {
        signs_of(cell, cell_start(first), &mut signs);
    }
    signs
}

/// Cell `cell`'s signs, in the frame upright at `frame` round the ring.
fn signs_of(cell: u16, frame: f32, signs: &mut Vec<Sign>) {
    let t0 = cell_start(cell);
    let shift = t0 - frame;
    for k in 0..town::LIFTS {
        let lt = town::lift_t(k);
        for turn in [-town::ROUND, 0.0, town::ROUND] {
            let at = lt + turn;
            // The half of the lift in this cell: the one after its middle, or the one before.
            let side = if (at - t0).abs() < 1.0 {
                1.0
            } else if (at - (t0 + CELL)).abs() < 1.0 {
                -1.0
            } else {
                continue;
            };
            let district = town::district(town::cell_at(lt + 1.0) / CELLS_PER_DISTRICT).name;
            let name = town::lift_name(k);
            let dt = at + side * LIFT_HALF_T - t0 + shift;
            let face = |text: &'static str, h: f32, height: f32, ink: u8| Sign {
                text: text.into(),
                at: bend(0.0, dt, h),
                normal: along(dt) * side,
                up: up(dt),
                height,
                ink,
            };
            signs.push(face(name, LIFT_LINTEL + 9.5, 5.0, crate::station::DARK));
            if k == 0 {
                signs.push(face(district, LIFT_LINTEL + 4.0, 2.4, crate::station::CHARTER));
            }
            // Over its door, in the fore pier on its district's side.
            let (dx, door_t) = town::lift_door(k);
            let door_dt = door_t + turn - t0;
            if (0.0..CELL).contains(&door_dt) {
                let door_dt = door_dt + shift;
                for (text, h, height) in [("LIFT", 3.55, 0.5), ("TO THE BAYS", 3.05, 0.26)] {
                    signs.push(Sign {
                        text: text.into(),
                        at: bend(dx, door_dt, h),
                        normal: -Vec3::X,
                        up: up(door_dt),
                        height,
                        ink: crate::station::DARK,
                    });
                }
            }
        }
    }
}

/// Where cell `cell` starts round the ring (the town's `t`), m: where its mesh's frame is upright.
pub fn cell_start(cell: u16) -> f32 {
    CELL * f32::from(cell % CELLS)
}

/// The half of lift `k`'s foot between `lo` and `hi` round the ring (its `t`, in the turn `turn`
/// puts it in): its piers there, its shaft over the lintel, and at Hub Gate's side its door.
fn lift(b: &mut Builder, k: u16, turn: f32, (lo, hi): (f32, f32), near: bool) {
    let portal = colour(Surface::Portal, 0, 0, k as u8);
    town::each_lift_box(k, &mut |bx| {
        let bx = TownBox { t0: (bx.t0 + turn).max(lo), t1: (bx.t1 + turn).min(hi), ..bx };
        if bx.t1 - bx.t0 < 0.01 {
            return;
        }
        if bx.h0 > 0.0 {
            // The shaft: its walls, and its soffit over the boulevard.
            b.boxed(&bx, portal, None, 0.0);
            b.flat(
                (bx.x0, bx.x1),
                (bx.t0, bx.t1),
                bx.h0,
                false,
                colour(Surface::Portal, 0, 1, k as u8),
                |x, t| [x, t],
            );
        } else {
            b.boxed(&bx, portal, None, 0.0);
        }
    });
    // Its door, in the outer face of its fore pier on its district's side: a lit frame, and its leaves.
    let (dx, dt) = town::lift_door(k);
    let dt = dt + turn;
    if near && (lo..hi).contains(&dt) {
        let (w, h, proud) = (1.3, 2.5, 0.04);
        let x = dx - proud;
        b.wall_t(x, (dt - w, dt + w), (0.0, h), -1.0, colour(Surface::Door, 0, 0, 0), |t, y| [t - dt, y]);
        let glow = colour(Surface::Glow, 3, 0, 0);
        let f = 0.18;
        let frame = x - 0.02;
        b.wall_t(frame, (dt - w - f, dt - w), (0.0, h + f), -1.0, glow, |t, y| [t, y]);
        b.wall_t(frame, (dt + w, dt + w + f), (0.0, h + f), -1.0, glow, |t, y| [t, y]);
        b.wall_t(frame, (dt - w, dt + w), (h, h + f), -1.0, glow, |t, y| [t, y]);
    }
}

/// A building's own paint: its district's, or (two in five) another of the four tints, still in
/// its quarter's colour. A street of one district isn't one building over and over.
fn building_paint(paint: u8, s: u8) -> u8 {
    match s % 5 {
        0 => (paint & !3) | (paint.wrapping_add(1) & 3),
        1 => (paint & !3) | (paint.wrapping_add(2) & 3),
        _ => paint,
    }
}

/// What stands on a wide building's roof, for the skyline's sake: a set-back upper block, a
/// pavilion at each end, a block to one side, or nothing. The mesh's alone: nothing walks or
/// drives up there, so the town's solid boxes (`town::each_block_box`) know nothing of it.
fn crown(b: &mut Builder, bx: &TownBox, paint: u8, flags: u8, s: u8) {
    let (w, l) = (bx.x1 - bx.x0, bx.t1 - bx.t0);
    let short = w.min(l);
    if short < 24.0 {
        return;
    }
    let h = u32::from(s);
    let roof = colour(Surface::Roof, paint, 0, s);
    let block = |b: &mut Builder, x: (f32, f32), t: (f32, f32), storeys: u32, k: u8| {
        let top = (bx.h1 + town::FLOOR * storeys as f32).min(town::MAX_HEIGHT);
        let c = TownBox::new(x.0, x.1, t.0, t.1, bx.h1 - b.seat, top);
        // (Its storeys start at its own foot: no ground floor's band up here.)
        let walls = colour(Surface::Facade, paint, flags, s.wrapping_mul(29).wrapping_add(k));
        b.boxed(&c, walls, Some(roof), bx.h1 - town::GROUND_FLOOR);
    };
    let inset = short * (0.14 + 0.04 * ((h / 5) % 4) as f32);
    match h % 6 {
        // A set-back block over the middle, one to four storeys.
        0 | 1 => block(b, (bx.x0 + inset, bx.x1 - inset), (bx.t0 + inset, bx.t1 - inset), 1 + (h / 7) % 4, 1),
        // A pavilion at each end of its longer side.
        2 => {
            let n = 2 + (h / 7) % 3;
            if l >= w {
                let d = l * 0.28;
                block(b, (bx.x0 + 3.0, bx.x1 - 3.0), (bx.t0 + 3.0, bx.t0 + d), n, 1);
                block(b, (bx.x0 + 3.0, bx.x1 - 3.0), (bx.t1 - d, bx.t1 - 3.0), n + (h / 11) % 2, 2);
            } else {
                let d = w * 0.28;
                block(b, (bx.x0 + 3.0, bx.x0 + d), (bx.t0 + 3.0, bx.t1 - 3.0), n, 1);
                block(b, (bx.x1 - d, bx.x1 - 3.0), (bx.t0 + 3.0, bx.t1 - 3.0), n + (h / 11) % 2, 2);
            }
        }
        // A taller block to one side, and a low one beside it.
        3 => {
            let (lo, hi) = if (h / 7).is_multiple_of(2) { (0.08, 0.45) } else { (0.55, 0.92) };
            let n = 3 + (h / 9) % 4;
            if l >= w {
                block(b, (bx.x0 + 4.0, bx.x1 - 4.0), (bx.t0 + l * lo, bx.t0 + l * hi), n, 1);
            } else {
                block(b, (bx.x0 + w * lo, bx.x0 + w * hi), (bx.t0 + 4.0, bx.t1 - 4.0), n, 1);
            }
            block(b, (bx.x0 + inset, bx.x1 - inset), (bx.t0 + inset, bx.t1 - inset), 1, 3);
        }
        _ => {}
    }
}

/// A lamp post at `(x, t)` on a pavement, its arm reaching `over` (±1) across towards the road.
fn lamp(b: &mut Builder, x: f32, t: f32, over: f32) {
    let post = colour(Surface::Trim, 9, 0, 0);
    let w = 0.12;
    b.solid(&TownBox::new(x - w, x + w, t - w, t + w, 0.0, LAMP_H), post);
    let reach = 3.0 * over;
    let (a, z) = (x.min(x + reach), x.max(x + reach));
    b.solid(&TownBox::new(a, z, t - 0.08, t + 0.08, LAMP_H - 0.2, LAMP_H), post);
    let head = x + reach;
    b.solid(
        &TownBox::new(head - 0.7, head + 0.7, t - 0.25, t + 0.25, LAMP_H - 0.36, LAMP_H - 0.2),
        colour(Surface::Glow, 0, 1, 0),
    );
}

/// The beams of a frame going up on the building site: round its four columns at every storey over
/// the ground floor (overhead: nothing to walk into).
fn frame_beams(b: &mut Builder, id: BlockId) {
    let mut cols: Vec<TownBox> = Vec::new();
    town::each_block_box(id, &mut |bx| {
        if bx.x1 - bx.x0 < 2.0 {
            cols.push(bx);
        }
    });
    if cols.len() < 4 {
        return;
    }
    let (x0, x1) = (
        cols.iter().map(|c| c.x0).fold(f32::MAX, f32::min),
        cols.iter().map(|c| c.x1).fold(f32::MIN, f32::max),
    );
    let (t0, t1) = (
        cols.iter().map(|c| c.t0).fold(f32::MAX, f32::min),
        cols.iter().map(|c| c.t1).fold(f32::MIN, f32::max),
    );
    let top = cols[0].h1;
    let steel = colour(Surface::Trim, 6, 0, seed(id, 9));
    let (w, d) = (0.5, 0.6);
    let mut h = town::GROUND_FLOOR;
    while h < top - 0.5 {
        for (a, z) in [(x0, x0 + w), (x1 - w, x1)] {
            b.solid(&TownBox::new(a, z, t0, t1, h - d, h), steel);
        }
        for (a, z) in [(t0, t0 + w), (t1 - w, t1)] {
            b.solid(&TownBox::new(x0, x1, a, z, h - d, h), steel);
        }
        h += town::FLOOR * 2.0;
    }
}

/// The lamp posts of a cell, along the boulevard's two pavements: where each stands, `(x, t)`.
fn lamps(cell: u16) -> Vec<(f32, f32)> {
    let t0 = cell_start(cell);
    let row = town::block_rect(BlockId::new(cell, 0));
    let mut out = Vec::new();
    for i in 0..4 {
        let t = t0 + (i as f32 + 0.5) * CELL / 4.0;
        if !(row.t0 + 2.0..row.t1 - 2.0).contains(&t) {
            continue;
        }
        for side in [-1.0f32, 1.0] {
            out.push((side * (BOULEVARD_HALF + LAMP_OFF), t));
        }
    }
    out
}

/// The trees along a cell's two rim walks: where each stands, its crown's reach and its seed.
fn rim_trees(cell: u16) -> Vec<(f32, f32, f32, u8)> {
    let t0 = cell_start(cell);
    let mut out = Vec::new();
    for i in 0..3 {
        let t = t0 + (i as f32 + 0.5) * CELL / 3.0;
        for side in [-1.0f32, 1.0] {
            let x = side * (HALF - RIM_WALK * 0.5);
            let s = (town::mix(u32::from(cell), i, 0x7EE) >> 8) as u8;
            out.push((x + (f32::from(s) / 255.0 - 0.5) * 16.0, t, 3.2 + f32::from(s % 5) * 0.3, s));
        }
    }
    out
}

/// Half how thick a lamp's post and a tree's trunk are to whoever walks or drives into one, m (a
/// little over what's drawn).
const POST: (f32, f32) = (0.2, 0.28);

/// What stands in a cell thin enough to walk round: each lamp's post and each tree's trunk, where
/// (`x`, `t`) and half how thick, m. The mesh's own spots, so what's drawn is what's walked into.
pub fn each_post(cell: u16, f: &mut impl FnMut(f32, f32, f32)) {
    for (x, t) in lamps(cell) {
        f(x, t, POST.0);
    }
    for (x, t, ..) in rim_trees(cell) {
        f(x, t, POST.1);
    }
    for column in 0..town::COLUMNS {
        let id = BlockId::new(cell, column);
        if town::block_use(id) == BlockUse::Park {
            for (x, t, ..) in park_tree_spots(id) {
                f(x, t, POST.1);
            }
        }
    }
}

/// A park's trees.
fn park_trees(b: &mut Builder, id: BlockId) {
    for (x, t, size, seed) in park_tree_spots(id) {
        b.tree(x, t, size, seed);
    }
}

/// A park's trees, on a jittered grid clear of its edges: where each stands, its crown's reach and
/// its seed.
fn park_tree_spots(id: BlockId) -> Vec<(f32, f32, f32, u8)> {
    let mut out = Vec::new();
    let r = town::block_rect(id).inset(6.0);
    let (nx, nt) = (((r.x1 - r.x0) / TREE_PITCH) as u32, ((r.t1 - r.t0) / TREE_PITCH) as u32);
    for i in 0..nx {
        for j in 0..nt {
            let h = town::mix(u32::from(id.cell) * 31 + u32::from(id.column), i * 64 + j, 0x7EE5);
            // A clearing here and there, and a path down the middle each way.
            if h.is_multiple_of(7) || i == nx / 2 || j == nt / 2 {
                continue;
            }
            let (jx, jt) = (town::unit(h, 1) - 0.5, town::unit(h, 2) - 0.5);
            let x = r.x0 + (i as f32 + 0.5 + jx * 0.6) * (r.x1 - r.x0) / nx as f32;
            let t = r.t0 + (j as f32 + 0.5 + jt * 0.6) * (r.t1 - r.t0) / nt as f32;
            out.push((x, t, 2.6 + town::unit(h, 3) * 1.8, (h >> 8) as u8));
        }
    }
    out
}

/// The canal's width, for the shader's ripples and the tests.
pub const CANAL_WIDTH: f32 = CANAL.1 - CANAL.0;
/// A lift's portal's size, for its signs.
pub const PORTAL: (f32, f32, f32, f32) = (LIFT_HALF_X, LIFT_HALF_T, LIFT_LINTEL, LIFT_PIER);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgsl;
    use bc_sim::colony::deck::{DeckPos, RING, local_frame};

    const SHADER: &str = include_str!("../../bc-client/src/shaders/town.wgsl");

    #[test]
    fn the_bend_is_the_decks() {
        // A cell's frame put where the deck has it is the colony's own place for every point.
        for cell in [0u16, 37, 143] {
            let t0 = cell_start(cell);
            let s0 = town::deck_s(t0);
            let (foot, turn) = (DeckPos::new(RING, 0.0, s0, 0.0).to_colony(), local_frame(RING, s0));
            for (x, dt, h) in
                [(0.0f32, 0.0f32, 0.0f32), (-900.0, 60.0, 0.0), (400.0, 130.9, 180.0), (10.0, -20.0, 500.0)]
            {
                let want = DeckPos::new(RING, x, s0 + dt, h).to_colony();
                let got = foot + turn * (bend(x, dt, h) - Vec3::X * 0.0);
                // (The frame's own origin is at x 0.)
                assert!((got - want).length() < 0.02, "cell {cell} ({x}, {dt}, {h}): {got} vs {want}");
                assert!((turn * up(dt) - (-want.with_x(0.0)).normalize()).length() < 1e-4);
            }
        }
        // Seen from where it is, a thing is at its own height, upright; a quarter of the ring on, on
        // its side overhead.
        let (at, turn) = seen_from(500.0, 12.0, 500.0, 1.7);
        assert!(
            (at - Vec3::new(12.0, 1.7, 0.0)).length() < 1e-4 && turn.angle_between(Quat::IDENTITY) < 1e-4
        );
        let (at, _) = seen_from(0.0, 0.0, town::ROUND * 0.25, 0.0);
        assert!((at - Vec3::new(0.0, R, -R)).length() < 0.5, "{at}");
        let (near, _) = seen_from(town::ROUND - 5.0, 0.0, 5.0, 0.0);
        assert!((near.z + 10.0).abs() < 0.01, "the short way round: {near}");
    }

    /// Whether every face of `m` is wound to face its normal.
    fn faces_outwards(m: &PieceMesh) -> bool {
        m.indices.chunks(3).all(|tri| {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.positions[i as usize]));
            let n = (b - a).cross(c - a);
            n.length() < 1e-6 || n.normalize().dot(Vec3::from(m.normals[tri[0] as usize])) > 0.5
        })
    }

    #[test]
    fn every_cell_builds_and_its_faces_look_outwards() {
        let mut tris = 0;
        for c in (0..CELLS).step_by(5).chain([0, 1, 142, 143, 70, 71, 72, 137]) {
            let m = cell(c, Detail::Near);
            assert!(m.triangles() > 200 && m.triangles() < 60_000, "cell {c}: {} triangles", m.triangles());
            let (far, haze) = (cell(c, Detail::Far), cell(c, Detail::Haze));
            for m in [&m, &far, &haze] {
                assert_eq!(m.positions.len(), m.normals.len());
                assert_eq!(m.positions.len(), m.uvs.len());
                assert_eq!(m.positions.len(), m.colors.len());
                assert!(faces_outwards(m), "cell {c}: a face wound away from its normal");
                assert!(m.positions.iter().all(|p| p.iter().all(|v| v.is_finite())));
            }
            assert!(haze.triangles() <= far.triangles() && far.triangles() <= m.triangles());
            tris += m.triangles();
        }
        assert!(tris > 20_000, "{tris}");
    }

    /// The far tiers (`docs/FREE_ROAM.md`, V1): a face a quad past the near cells, and boxes in the
    /// haze, each cell within what's reserved for it, at a share of the near cell's triangles; past
    /// the near cells every face flagged for the shader's distant path, and none near.
    #[test]
    fn the_far_tiers_are_within_their_caps() {
        let (mut most, mut sum) = ([(0, 0); 3], [0; 3]);
        for c in 0..CELLS {
            for (k, detail) in [Detail::Near, Detail::Far, Detail::Haze].into_iter().enumerate() {
                let m = cell(c, detail);
                most[k] = (most[k].0.max(m.positions.len()), most[k].1.max(m.indices.len()));
                sum[k] += m.triangles();
                let distant = m.colors.iter().filter(|c| (c[2] * 255.0).round() as u8 & DISTANT != 0).count();
                assert_eq!(
                    distant,
                    if detail == Detail::Near { 0 } else { m.colors.len() },
                    "cell {c} {detail:?}"
                );
            }
        }
        println!("most (vertices, indices) near, far, haze: {most:?}; triangles in all: {sum:?}");
        for (k, detail) in [(1, Detail::Far), (2, Detail::Haze)] {
            let (verts, indices) = detail.caps();
            assert!(most[k].0 <= verts && most[k].1 <= indices, "{detail:?}: {most:?}");
        }
        // Far, under half a near cell's; in the haze, under a third.
        assert!(sum[1] * 2 < sum[0] && sum[2] * 3 < sum[0], "{sum:?}");
        assert!(sum[2] < sum[1], "{sum:?}");
        // Each cap within a fifth over what a cell comes to.
        for (k, detail) in [(1, Detail::Far), (2, Detail::Haze)] {
            let (verts, indices) = detail.caps();
            assert!(verts * 5 <= most[k].0 * 6 && indices * 5 <= most[k].1 * 6, "{detail:?}: {most:?}");
        }
    }

    /// A piece of cells built as one is its cells' meshes, each turned into the piece's frame: a
    /// district's or a quad's, as the client draws the far tiers.
    #[test]
    fn a_piece_is_its_cells() {
        for (first, count, detail) in [(24, 12, Detail::Haze), (84, 4, Detail::Far), (140, 4, Detail::Haze)] {
            let m = piece(first, count, detail);
            assert!(faces_outwards(&m));
            let s0 = town::deck_s(cell_start(first));
            let mut at = 0;
            for c in first..first + count {
                let one = cell(c, detail);
                let (shift, turn) = seen_from(s0, 0.0, town::deck_s(cell_start(c)), 0.0);
                for (k, p) in one.positions.iter().enumerate().step_by(97) {
                    let want = shift + turn * Vec3::from(*p);
                    let got = Vec3::from(m.positions[at + k]);
                    assert!((got - want).length() < 0.02, "cell {c} of {first}+{count}: {got} vs {want}");
                    assert_eq!(m.colors[at + k], one.colors[k]);
                }
                at += one.positions.len();
            }
            assert_eq!(at, m.positions.len());
        }
        // (A run doesn't go round past the town's last cell.)
        assert_eq!(piece(140, 8, Detail::Haze).positions, piece(140, 4, Detail::Haze).positions);
    }

    #[test]
    fn every_lift_is_named_on_its_shaft_and_over_its_door() {
        use crate::station::stencils;
        let mut all = Vec::new();
        for c in 0..CELLS {
            for s in cell_signs(c) {
                assert!(stencils(&s.text), "{:?} can't be stencilled", s.text);
                assert!((s.normal.length() - 1.0).abs() < 1e-3 && s.normal.dot(s.up).abs() < 1e-3);
                all.push((c, s));
            }
        }
        // Each lift: its name either side of its shaft, and its door's two lines; Hub Gate its
        // district under its name too.
        let named = |text: &str| all.iter().filter(|(_, s)| s.text == text).count();
        assert_eq!((named("HUB GATE"), named("CHARTER SQUARE"), named("TOWER HILL")), (2, 2, 2));
        assert_eq!((named("LIFT"), named("TO THE BAYS")), (6, 6));
        // Hub Gate's door's sign is in cell 0, over the door, on the pier's outer face.
        let (dx, dt) = town::lift_door(0);
        let (_, lift) = all.iter().find(|(c, s)| *c == 0 && s.text == "LIFT").expect("over Hub Gate's door");
        assert!((lift.at - bend(dx, dt, 3.55)).length() < 1e-3 && lift.normal == -Vec3::X);
    }

    #[test]
    fn the_floor_tiles_a_cell_once() {
        // The level faces at the floor's own height cover the cell's 2 km by 131 m once over (the
        // canal's water lies under its quays).
        for detail in [Detail::Near, Detail::Far, Detail::Haze] {
            let m = cell(40, detail);
            let mut area = 0.0;
            for tri in m.indices.chunks(3) {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.positions[i as usize]));
                let surface = (m.colors[tri[0] as usize][0] * 255.0).round() as u8;
                let up = Vec3::from(m.normals[tri[0] as usize]).y > 0.9;
                let ground = !matches!(surface, 2 | 11 | 12 | 13 | 15 | 16);
                if up && ground {
                    area += 0.5 * (b - a).cross(c - a).length();
                }
            }
            let want = 2.0 * HALF * CELL;
            assert!((area - want).abs() < want * 0.002, "{detail:?}: {area} vs {want}");
        }
    }

    #[test]
    fn the_shaders_surfaces_are_these() {
        let c = |name| wgsl::constant(SHADER, name);
        for (name, s) in [
            ("FACADE", Surface::Facade),
            ("ROOF", Surface::Roof),
            ("ROAD", Surface::Road),
            ("JUNCTION", Surface::Junction),
            ("PAVEMENT", Surface::Pavement),
            ("MEDIAN", Surface::Median),
            ("WATER", Surface::Water),
            ("GRASS", Surface::Grass),
            ("SQUARE", Surface::Square),
            ("LOT", Surface::Lot),
            ("TRIM", Surface::Trim),
            ("PORTAL", Surface::Portal),
            ("GLOW", Surface::Glow),
            ("RIM", Surface::Rim),
            ("SKY", Surface::Sky),
            ("FOLIAGE", Surface::Foliage),
            ("SITE", Surface::Site),
            ("DOOR", Surface::Door),
        ] {
            assert_eq!(c(name), f32::from(s as u8), "{name}");
        }
        assert_eq!(c("TOWER"), f32::from(TOWER));
        assert_eq!(c("DISTANT"), f32::from(DISTANT));
        assert_eq!(c("SKY_FRAME"), SKY_FRAME);
        assert_eq!(c("CELL"), (CELL * 1000.0).round() / 1000.0);
        assert_eq!(c("BAY"), (crate::town::BAY * 10_000.0).round() / 10_000.0);
        for (name, r) in [
            ("R_STREET", Road::Street),
            ("R_AVENUE", Road::Avenue),
            ("R_BOULEVARD", Road::Boulevard),
            ("R_RIM", Road::Rim),
        ] {
            assert_eq!(c(name), f32::from(r as u8), "{name}");
        }
        for (name, r) in [
            ("H_STREET", Road::Street),
            ("H_AVENUE", Road::Avenue),
            ("H_BOULEVARD", Road::Boulevard),
            ("H_RIM", Road::Rim),
        ] {
            assert_eq!(c(name), r.half(), "{name}");
        }
        assert_eq!(c("GROUND_FLOOR"), town::GROUND_FLOOR);
        assert_eq!(c("STOREY"), town::FLOOR);
    }

    /// As in `city.wgsl` and `station.wgsl` (`CLAUDE.md`): every derivative at the top of
    /// `fragment()`, before its first branch (on the surface, or the distant path's).
    #[test]
    fn the_town_shader_takes_its_derivatives_at_the_top() {
        let lines: Vec<&str> = SHADER.lines().map(|l| l.split("//").next().unwrap_or("")).collect();
        let derivative = |l: &str| ["dpdx", "dpdy", "fwidth"].iter().any(|d| l.contains(d));
        let start = lines.iter().position(|l| l.contains("fn fragment(")).unwrap();
        let branches = ["if ", "if(", "switch ", "for ", "for(", "while ", "loop "];
        let first_branch = start
            + lines[start..]
                .iter()
                .position(|l| branches.iter().any(|b| l.trim_start().starts_with(b)))
                .unwrap();
        for (k, l) in lines.iter().enumerate() {
            assert!(!derivative(l) || (start..first_branch).contains(&k), "town.wgsl line {}: {l}", k + 1);
        }
    }
}
