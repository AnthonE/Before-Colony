//! The base's buildings as meshes (`docs/RING.md`, "Buildings from pieces"): each piece's boxes
//! (`bc_sim::pieces::parts`, the same table its solids come from) drawn as themselves and dressed in
//! the colony's and the base's paints: white panels over an olive dado on the walls, dark deck plating
//! underfoot, hazard yellow up a doorway's posts, gunmetal treads. Every vertex lies on its box, so
//! what's drawn is what's walked.
//!
//! Plain mesh data in the building's own frame ([`site_frame`]: upright at its foot, so the hull
//! shader's plates, laid in it, run square with its walls), bent round its deck as it stands; the
//! client draws it with the hull material (`hull.wgsl` reads each vertex's colour: its paint slot,
//! a bevel, a seed, its occlusion).

use bc_model::paint;
use bc_sim::colony::deck::{DeckPos, local_frame};
use bc_sim::pieces::parts::{Aabb, Part, parts};
use bc_sim::pieces::prefab::Site;
use glam::{Quat, Vec3};

/// A mesh: positions and normals in its building's frame ([`site_frame`]), the hull shader's vertex
/// colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PieceMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// A wall's dado: below this, m over its storey's floor, it's the base's olive.
pub const DADO: f32 = 1.0;
/// How long a face's strips are at most round the deck, m: short enough that bending it round the
/// deck sags it under a millimetre at the base's 400 m.
const STRIP: f32 = 1.5;

/// The hull shader's slot for a paint: `16 + paint`.
const fn slot(p: u8) -> f32 {
    (16 + p) as f32 / 255.0
}

impl PieceMesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// A flat rectangle in the walker's frame on `site`'s deck: corner `o`, sides `u` and `v`
    /// (counter-clockwise seen from where `u × v` points), cut into strips along whichever side runs
    /// round the deck, each strip bent onto the deck, and put in the building's frame `frame`.
    #[allow(clippy::too_many_arguments)]
    fn rect(&mut self, site: &Site, frame: &Frame, o: Vec3, u: Vec3, v: Vec3, p: u8, seed: u32) {
        let n = u.cross(v).normalize_or_zero();
        // Cut along the side that runs round the deck (its z, which is −s).
        let (cuts_u, cuts_v) =
            (((u.z.abs() / STRIP).ceil() as u32).max(1), ((v.z.abs() / STRIP).ceil() as u32).max(1));
        let colour = [slot(p), 0.0, (seed % 89) as f32 / 97.0, 1.0];
        let base = self.positions.len() as u32;
        for j in 0..=cuts_v {
            for i in 0..=cuts_u {
                let w = o + u * (i as f32 / cuts_u as f32) + v * (j as f32 / cuts_v as f32);
                let at = DeckPos::from_walker(site.deck, w);
                self.positions.push((frame.inv * (at.to_colony() - frame.foot)).to_array());
                self.normals.push((frame.inv * (local_frame(site.deck, at.s) * n)).to_array());
                self.colors.push(colour);
            }
        }
        let row = cuts_u + 1;
        for j in 0..cuts_v {
            for i in 0..cuts_u {
                let a = base + j * row + i;
                self.indices.extend([a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            }
        }
    }

    /// A box's six faces, in the walker's frame, the side faces split at `split` (its height over
    /// the box's foot, if any) into two paints.
    #[allow(clippy::too_many_arguments)]
    fn cuboid(
        &mut self,
        site: &Site,
        frame: &Frame,
        b: &Aabb,
        top: u8,
        side: u8,
        lower: Option<(f32, u8)>,
        seed: u32,
    ) {
        let (lo, hi) = (b.min, b.max);
        let d = hi - lo;
        let (x, y, z) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
        // Top and bottom.
        self.rect(site, frame, Vec3::new(lo.x, hi.y, lo.z), z, x, top, seed);
        self.rect(site, frame, lo, x, z, side, seed);
        // The four sides, each from its foot up (along × up is out), split where asked.
        let sides = [
            (Vec3::new(hi.x, lo.y, lo.z), -x), // −z
            (Vec3::new(hi.x, lo.y, hi.z), -z), // +x
            (Vec3::new(lo.x, lo.y, hi.z), x),  // +z
            (lo, z),                           // −x
        ];
        for (o, along) in sides {
            match lower {
                Some((h, p)) if h > 0.0 && h < d.y => {
                    self.rect(site, frame, o, along, Vec3::Y * h, p, seed);
                    self.rect(site, frame, o + Vec3::Y * h, along, Vec3::Y * (d.y - h), side, seed);
                }
                _ => self.rect(site, frame, o, along, y, side, seed),
            }
        }
    }
}

/// A building's frame in the colony's own: its foot on the deck, and the turn from it (the walker's
/// frame there: up is +Y, the axis +X).
pub fn site_frame(site: &Site) -> (Vec3, Quat) {
    (DeckPos::new(site.deck, site.x, site.s, 0.0).to_colony(), local_frame(site.deck, site.s))
}

/// [`site_frame`], the way back into it.
struct Frame {
    foot: Vec3,
    inv: Quat,
}

/// A building's mesh as it stands, in its own frame ([`site_frame`]).
pub fn site_mesh(site: &Site) -> PieceMesh {
    let mut m = PieceMesh::default();
    let (foot, turn) = site_frame(site);
    let frame = Frame { foot, inv: turn.inverse() };
    for (k, p) in site.prefab.pieces.iter().enumerate() {
        let seed = k as u32 * 7 + 3;
        let storey = bc_sim::pieces::shape::level_y(p.level);
        for (part, b) in parts(p.shape, &p.addr(0)).as_slice() {
            let w = site.box_to_walker(b);
            match part {
                Part::Slab => m.cuboid(site, &frame, &w, paint::HULL_DARK, paint::HULL, None, seed),
                Part::Wall => m.cuboid(
                    site,
                    &frame,
                    &w,
                    paint::HULL,
                    paint::WHITE,
                    Some((storey + DADO - w.min.y, paint::CETUS_OLIVE)),
                    seed,
                ),
                Part::Post => m.cuboid(site, &frame, &w, paint::HULL, paint::YELLOW, None, seed),
                Part::Lintel => m.cuboid(site, &frame, &w, paint::HULL, paint::HULL_DARK, None, seed),
                Part::Tread => m.cuboid(site, &frame, &w, paint::GUNMETAL, paint::HULL_DARK, None, seed),
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::deck::deck_at;
    use bc_sim::content::base::SITES;

    /// Every vertex lies on a box of its building's solids: what's drawn is what's walked.
    #[test]
    fn every_vertex_lies_on_a_solid() {
        for site in &SITES {
            let m = site_mesh(site);
            assert!(m.triangles() > 100, "{}: {} triangles", site.prefab.name, m.triangles());
            let mut solids = Vec::new();
            site.each_solid(&mut |b| solids.push(*b));
            let (foot, turn) = site_frame(site);
            for p in &m.positions {
                let at = deck_at(turn * Vec3::from_array(*p) + foot).expect("over its deck");
                let w = at.walker();
                let on = solids.iter().any(|b| {
                    let near = |v: f32, lo: f32, hi: f32| v >= lo - 1e-3 && v <= hi + 1e-3;
                    near(w.x, b.min.x, b.max.x) && near(w.y, b.min.y, b.max.y) && near(w.z, b.min.z, b.max.z)
                });
                assert!(on, "{}: a vertex at {w} is on none of its solids", site.prefab.name);
            }
        }
    }
}
