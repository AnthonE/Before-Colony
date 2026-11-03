//! The ring's town as meshes (`docs/FREE_ROAM.md`, V1): a chunk of it at a time, from the very boxes
//! the walker meets (`bc_sim::colony::town`), dressed in the colony's station language (`station`):
//! its buildings' faces in storeys of panels and windows with a rule of their quarter's colour at
//! every floor, roofs of deck plates, the roads dark, the pavements and squares in plates, grass in
//! the parks and on the rim walks, the canal's water and its orange railings, the lifts' feet in
//! pillars and panels, and the building site's frames in yellow.
//!
//! A chunk is [`CHUNK_CELLS`] cells round the ring and the whole of it across, in the colony's own
//! frame, bent round the ring's floor in strips at most a few metres long ([`Lod::Near`]) or tens
//! ([`Lod::Far`], which leaves out the railings). The client builds the chunks within sight of the
//! view, a few a frame (`bc-client`'s `town`).

use bc_sim::colony::deck::RING;
use bc_sim::colony::town::{
    self, BOULEVARD_HALF, BlockId, BlockUse, CANAL, CELL, CELLS, COLUMNS, MEDIAN_HALF, RIM_ROAD, RIM_WALK,
    STREET, TownBox, block_rect, block_use, column, each_block_box, each_lift_box, each_rail,
};
use bc_sim::content::town::Quarter;
use bc_sim::world::COLONY_HALF_WIDTH as HALF;
use glam::Vec3;

use crate::piece_mesh::{Frame, PieceMesh, walker_uv};
use crate::station::{
    CANAL as CANAL_PAINT, CHARTER, GARDENS, GUNMETAL, HULL, ORANGE, Surface, WHITE, YELLOW, colour,
};

/// Cells round the ring in a chunk, and how many chunks there are.
pub const CHUNK_CELLS: u16 = 2;
pub const CHUNKS: u16 = CELLS / CHUNK_CELLS;
/// A chunk's length round the ring, m (262).
pub const CHUNK: f32 = CELL * CHUNK_CELLS as f32;

/// How finely a chunk is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lod {
    /// Bent round the floor every 8 m (it sags 3 mm), its railings and all.
    Near,
    /// Every 48 m (10 cm), no railings: for past a kilometre, in the haze.
    Far,
}

impl Lod {
    fn strip(self) -> f32 {
        match self {
            Lod::Near => 8.0,
            Lod::Far => 48.0,
        }
    }
}

/// The chunk at `t` round the ring (any `t`: it wraps).
pub fn chunk_at(t: f32) -> u16 {
    town::cell_at(t) / CHUNK_CELLS
}

/// Chunk `k`'s start round the ring, `t`.
pub fn chunk_t(k: u16) -> f32 {
    CHUNK * f32::from(k % CHUNKS)
}

/// The paint a quarter's buildings are ruled in: its tram line's colour.
pub fn quarter_paint(q: Quarter) -> u8 {
    match q {
        Quarter::Charter => CHARTER,
        Quarter::Canal => CANAL_PAINT,
        Quarter::Gardens => GARDENS,
        Quarter::Site => YELLOW,
    }
}

/// Lays a chunk out in the walker's frame on the ring's deck, `t` turned into the deck's `s` from the
/// chunk's start (so a chunk astride the deck's seam is one piece).
struct Builder {
    m: PieceMesh,
    t0: f32,
    s0: f32,
    strip: f32,
}

impl Builder {
    fn new(k: u16, lod: Lod) -> Self {
        let t0 = chunk_t(k);
        Self { m: PieceMesh::default(), t0, s0: town::deck_s(t0), strip: lod.strip() }
    }

    /// The walker's frame at `x` across, `t` round, `h` up.
    fn w(&self, x: f32, t: f32, h: f32) -> Vec3 {
        Vec3::new(x, h, -(self.s0 + (t - self.t0)))
    }

    fn rect(&mut self, o: Vec3, u: Vec3, v: Vec3, c: [f32; 4]) {
        let uv = |w: Vec3, n: Vec3| walker_uv(w, n, 0.0);
        self.m.rect_in_strips(RING, &Frame::COLONY, o, u, v, c, &uv, self.strip);
    }

    /// The floor from `x0` to `x1` across and `t0` to `t1` round, facing up.
    fn ground(&mut self, x0: f32, x1: f32, t0: f32, t1: f32, c: [f32; 4]) {
        if x1 <= x0 || t1 <= t0 {
            return;
        }
        let o = self.w(x0, t0, 0.0);
        self.rect(o, Vec3::X * (x1 - x0), Vec3::NEG_Z * (t1 - t0), c);
    }

    /// A box standing on the floor: its top and its four sides (nothing sees its underside).
    fn boxed(&mut self, b: &TownBox, top: [f32; 4], side: [f32; 4]) {
        let lo = self.w(b.x0, b.t1, b.h0);
        let hi = self.w(b.x1, b.t0, b.h1);
        let d = hi - lo;
        let (x, y, z) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
        self.rect(Vec3::new(lo.x, hi.y, lo.z), z, x, top);
        for (o, along) in [
            (Vec3::new(hi.x, lo.y, lo.z), -x),
            (Vec3::new(hi.x, lo.y, hi.z), -z),
            (Vec3::new(lo.x, lo.y, hi.z), x),
            (lo, z),
        ] {
            self.rect(o, along, y, side);
        }
    }
}

/// A block's seed for the shader's panels and windows.
fn seed(id: BlockId) -> u8 {
    (town::mix(u32::from(id.cell), u32::from(id.column), 0x5EED) % 251) as u8
}

/// Chunk `k` of the town, in the colony's own frame.
pub fn chunk_mesh(k: u16, lod: Lod) -> PieceMesh {
    use Surface::*;
    let mut b = Builder::new(k % CHUNKS, lod);
    let road = colour(Road, HULL, 0, 1);
    let plates = colour(Floor, WHITE, 0, 2);
    let grass = colour(Grass, GARDENS, 0, 3);
    for cell in (k % CHUNKS) * CHUNK_CELLS..(k % CHUNKS + 1) * CHUNK_CELLS {
        let (c0, c1) = (CELL * f32::from(cell), CELL * f32::from(cell + 1));
        // Across the ring outside the blocks: the rim walks and roads, the boulevard and its median,
        // the canal (its water between the bridges at the cell's ends).
        let rim = HALF - RIM_WALK;
        b.ground(-HALF, -rim, c0, c1, grass);
        b.ground(rim, HALF, c0, c1, grass);
        b.ground(-rim, -rim + RIM_ROAD, c0, c1, road);
        b.ground(rim - RIM_ROAD, rim, c0, c1, road);
        b.ground(-BOULEVARD_HALF, -MEDIAN_HALF, c0, c1, road);
        b.ground(MEDIAN_HALF, BOULEVARD_HALF, c0, c1, road);
        b.ground(-MEDIAN_HALF, MEDIAN_HALF, c0, c1, colour(Trim, GUNMETAL, 0, 4));
        let first = block_rect(BlockId::new(cell, 0));
        let (w0, w1) = (first.t0, first.t1);
        b.ground(CANAL.0, CANAL.1, c0, w0, road);
        b.ground(CANAL.0, CANAL.1, w0, w1, colour(Water, CANAL_PAINT, 0, 5));
        b.ground(CANAL.0, CANAL.1, w1, c1, road);
        for c in 0..COLUMNS {
            let id = BlockId::new(cell, c);
            let r = block_rect(id);
            let (x0, x1, lo, hi) = column(c);
            // The block, its streets round the cell's ends, and either side a street's half or the
            // pavement or quay along its band.
            let floor = match block_use(id) {
                BlockUse::Park => grass,
                _ => colour(Floor, WHITE, 0, seed(id)),
            };
            b.ground(r.x0, r.x1, r.t0, r.t1, floor);
            b.ground(x0, x1, c0, r.t0, road);
            b.ground(x0, x1, r.t1, c1, road);
            let side = |half: f32| if half == STREET * 0.5 { road } else { plates };
            b.ground(x0, r.x0, r.t0, r.t1, side(lo));
            b.ground(r.x1, x1, r.t0, r.t1, side(hi));
            // What stands on it.
            let ink = quarter_paint(town::district(id.district()).quarter);
            let s = seed(id);
            let (top, face) = match block_use(id) {
                BlockUse::Frames => (colour(Trim, YELLOW, 0, s), colour(Pillar, YELLOW, 0, s)),
                _ => (colour(Floor, WHITE, 0, s), colour(Facade, ink, 0, s)),
            };
            each_block_box(id, &mut |bx| b.boxed(&bx, top, face));
        }
        if lod == Lod::Near {
            let rail = colour(Trim, ORANGE, 0, 6);
            each_rail(cell, &mut |bx| b.boxed(&bx, rail, rail));
        }
    }
    // The lifts' feet standing in the chunk.
    let (t0, t1) = (chunk_t(k), chunk_t(k) + CHUNK);
    for l in 0..town::LIFTS {
        let at = town::lift_t(l);
        if (t0..t1).contains(&at) {
            each_lift_box(l, &mut |bx| {
                let (top, side) = if bx.h0 > 0.0 {
                    (colour(Trim, HULL, 0, 7), colour(Wall, YELLOW, 0, 7))
                } else {
                    (colour(Trim, HULL, 0, 8), colour(Pillar, YELLOW, crate::station::CHEVRONS, 8))
                };
                b.boxed(&bx, top, side);
            });
        }
    }
    b.m
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::deck::deck_at;
    use bc_sim::world::{COLONY_INNER_RADIUS, COLONY_RADIUS};

    #[test]
    fn a_chunk_stands_in_its_own_stretch_of_the_ring() {
        for k in [0u16, 1, 17, CHUNKS / 2, CHUNKS - 1] {
            for lod in [Lod::Near, Lod::Far] {
                let m = chunk_mesh(k, lod);
                assert!(m.triangles() > 500, "chunk {k}: {} triangles", m.triangles());
                assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
                assert_eq!(m.positions.len(), m.colors.len());
                assert_eq!(m.positions.len(), m.uvs.len());
                let (t0, t1) = (chunk_t(k), chunk_t(k) + CHUNK);
                for p in &m.positions {
                    let p = Vec3::from_array(*p);
                    let r = p.y.hypot(p.z);
                    assert!(p.x.abs() <= HALF + 0.01, "chunk {k}: {p} past the ring's face");
                    assert!(
                        r <= COLONY_RADIUS + 0.01 && r >= COLONY_INNER_RADIUS - 0.01,
                        "chunk {k}: {p} at r {r}"
                    );
                    // Round the ring, in the chunk's stretch (a lift's foot reaches a little either side).
                    let at = deck_at(p).expect("over the ring's deck");
                    assert_eq!(at.deck, RING);
                    let t = town::town_t(at.s);
                    let off = (t - t0).rem_euclid(town::ROUND);
                    let margin = town::LIFT_HALF_T + 0.01;
                    assert!(
                        off <= t1 - t0 + margin || off >= town::ROUND - margin,
                        "chunk {k}: {p} at t {t}"
                    );
                }
            }
        }
    }

    #[test]
    fn far_is_coarser_and_a_chunk_is_within_its_budget() {
        let mut most = 0;
        for k in 0..CHUNKS {
            let (near, far) = (chunk_mesh(k, Lod::Near), chunk_mesh(k, Lod::Far));
            // (The building site's frames are thin columns, bent no more at either.)
            assert!(far.positions.len() < near.positions.len(), "chunk {k}");
            most = most.max(near.positions.len());
        }
        // A frame builds a chunk or two: a near one is at most this many vertices.
        assert!(most < 60_000, "{most} vertices");
        let (near, far): (usize, usize) = (0..CHUNKS)
            .map(|k| (chunk_mesh(k, Lod::Near).positions.len(), chunk_mesh(k, Lod::Far).positions.len()))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        eprintln!("the whole ring: {near} vertices near, {far} far; the most in a chunk {most}");
    }

    #[test]
    fn the_same_chunk_comes_out_the_same() {
        assert_eq!(chunk_mesh(5, Lod::Near), chunk_mesh(5, Lod::Near));
        assert_eq!(chunk_at(chunk_t(9) + 1.0), 9);
        assert_eq!(chunk_at(-1.0), CHUNKS - 1);
    }
}
