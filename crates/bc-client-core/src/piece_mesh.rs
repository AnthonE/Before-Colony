//! The base's buildings as meshes (`docs/RING.md`, "Buildings from pieces"): each piece's boxes
//! (`bc_sim::pieces::parts`, the same table its solids come from) drawn as themselves and dressed in
//! the colony's station language (`station`, "The station language"): white panels over the base's
//! olive dado on the walls with the band in Charter blue, deck plates underfoot and light strips
//! overhead, hazard stripes up a doorway's posts and its head lit green, gunmetal treads. Every
//! vertex lies on its box, so what's drawn is what's walked.
//!
//! Plain mesh data in the building's own frame ([`site_frame`]: upright at its foot), bent round its
//! deck as it stands; the client draws it with the Muster's station material (`station.wgsl` reads
//! each vertex's colour: its surface, paint, flags and a seed; and its UVs, metres across the face).

use bc_sim::colony::deck::{DeckPos, local_frame};
use bc_sim::pieces::parts::{Aabb, Part, parts};
use bc_sim::pieces::prefab::Site;
use bc_sim::pieces::shape::{DOORWAY, LEVEL_H_M, level_y};
use glam::{Quat, Vec3};

use crate::station::{BAND, CHARTER, DARK, GARDENS, GUNMETAL, HULL, Sign, Surface, WHITE, colour};

/// A mesh: positions and normals in its frame, the station shader's vertex colours, and UVs in
/// metres across each face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PieceMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// A wall's dado: below this, m over its storey's floor, it's the base's olive (the Muster's kick
/// plate, `station::Scale::MUSTER`).
pub const DADO: f32 = 1.0;
/// How long a face's strips are at most round the deck, m: short enough that bending it round the
/// deck sags it under a millimetre at the base's 400 m.
const STRIP: f32 = 1.5;

/// A frame to put a deck's points in: the colony's own, moved by `-foot` and turned by `inv`.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub foot: Vec3,
    pub inv: Quat,
}

impl Frame {
    /// The colony's own frame.
    pub const COLONY: Self = Self { foot: Vec3::ZERO, inv: Quat::IDENTITY };
}

/// A face's UVs at a point of the walker's frame on its deck, with `n` its normal there: on a
/// floor or a ceiling, metres along the deck and across it; on a wall, along it and up over `base`
/// (its storey's floor, so a storey's band is where the next one's is).
pub fn walker_uv(w: Vec3, n: Vec3, base: f32) -> [f32; 2] {
    if n.y.abs() > 0.5 {
        [w.z, w.x]
    } else if n.x.abs() > 0.5 {
        [w.z, w.y - base]
    } else {
        [w.x, w.y - base]
    }
}

impl PieceMesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// A flat rectangle in the walker's frame on deck `deck`: corner `o`, sides `u` and `v`
    /// (counter-clockwise seen from where `u × v` points), cut into strips along whichever side runs
    /// round the deck, each strip bent onto the deck, put in `frame`, coloured `colour` and given
    /// its UVs by `uv` (of a walker point and the face's normal).
    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &mut self,
        deck: u8,
        frame: &Frame,
        o: Vec3,
        u: Vec3,
        v: Vec3,
        colour: [f32; 4],
        uv: &dyn Fn(Vec3, Vec3) -> [f32; 2],
    ) {
        let n = u.cross(v).normalize_or_zero();
        // Cut along the side that runs round the deck (its z, which is −s).
        let (cuts_u, cuts_v) =
            (((u.z.abs() / STRIP).ceil() as u32).max(1), ((v.z.abs() / STRIP).ceil() as u32).max(1));
        let base = self.positions.len() as u32;
        for j in 0..=cuts_v {
            for i in 0..=cuts_u {
                let w = o + u * (i as f32 / cuts_u as f32) + v * (j as f32 / cuts_v as f32);
                let at = DeckPos::from_walker(deck, w);
                self.positions.push((frame.inv * (at.to_colony() - frame.foot)).to_array());
                self.normals.push((frame.inv * (local_frame(deck, at.s) * n)).to_array());
                self.uvs.push(uv(w, n));
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

    /// A box's six faces, in the walker's frame: its top, its underside and its four sides each
    /// coloured as given, the sides' UVs measured up from `base`.
    #[allow(clippy::too_many_arguments)]
    pub fn cuboid(
        &mut self,
        deck: u8,
        frame: &Frame,
        b: &Aabb,
        top: [f32; 4],
        under: [f32; 4],
        side: [f32; 4],
        uv: &dyn Fn(Vec3, Vec3) -> [f32; 2],
    ) {
        let (lo, hi) = (b.min, b.max);
        let d = hi - lo;
        let (x, y, z) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
        self.rect(deck, frame, Vec3::new(lo.x, hi.y, lo.z), z, x, top, uv);
        self.rect(deck, frame, lo, x, z, under, uv);
        // The four sides, each from its foot up (along × up is out).
        for (o, along) in [
            (Vec3::new(hi.x, lo.y, lo.z), -x), // −z
            (Vec3::new(hi.x, lo.y, hi.z), -z), // +x
            (Vec3::new(lo.x, lo.y, hi.z), x),  // +z
            (lo, z),                           // −x
        ] {
            self.rect(deck, frame, o, along, y, side, uv);
        }
    }
}

/// A building's frame in the colony's own: its foot on the deck, and the turn from it (the walker's
/// frame there: up is +Y, the axis +X).
pub fn site_frame(site: &Site) -> (Vec3, Quat) {
    (DeckPos::new(site.deck, site.x, site.s, 0.0).to_colony(), local_frame(site.deck, site.s))
}

/// A building's mesh as it stands, in its own frame ([`site_frame`]).
pub fn site_mesh(site: &Site) -> PieceMesh {
    use Surface::*;
    let mut m = PieceMesh::default();
    let (foot, turn) = site_frame(site);
    let frame = Frame { foot, inv: turn.inverse() };
    let trim = |p: u8, seed: u8| colour(Trim, p, 0, seed);
    for (k, p) in site.prefab.pieces.iter().enumerate() {
        let seed = ((k * 7 + 3) % 251) as u8;
        let storey = level_y(p.level);
        let uv = move |w: Vec3, n: Vec3| walker_uv(w, n, storey);
        for (part, b) in parts(p.shape, &p.addr(0)).as_slice() {
            let w = site.box_to_walker(b);
            let (top, under, side) = match part {
                Part::Slab => {
                    (colour(Floor, WHITE, 0, seed), colour(Ceiling, WHITE, 0, seed), trim(HULL, seed))
                }
                Part::Wall => (trim(HULL, seed), trim(HULL, seed), colour(Wall, CHARTER, BAND, seed)),
                Part::Post => (trim(HULL, seed), trim(HULL, seed), colour(Hazard, WHITE, 0, seed)),
                Part::Lintel => {
                    let lit = colour(Glow, GARDENS, 0, seed);
                    (lit, lit, lit)
                }
                Part::Tread => (colour(Floor, WHITE, 0, seed), trim(GUNMETAL, seed), trim(GUNMETAL, seed)),
            };
            m.cuboid(site.deck, &frame, &w, top, under, side, &uv);
        }
    }
    m
}

/// A building's name over its doorways outside, on the wall over each doorway's head (the colony's
/// own frame).
pub fn site_signs(site: &Site) -> Vec<Sign> {
    let bounds = site.prefab.bounds();
    let middle = (bounds.min + bounds.max) * 0.5;
    let mut solids: Vec<Aabb> = Vec::new();
    for p in site.prefab.pieces {
        for (_, b) in parts(p.shape, &p.addr(0)).as_slice() {
            solids.push(*b);
        }
    }
    let mut out = Vec::new();
    for p in site.prefab.pieces.iter().filter(|p| p.shape == DOORWAY && p.level == 0) {
        let Some((_, head)) =
            parts(p.shape, &p.addr(0)).as_slice().iter().copied().find(|(k, _)| *k == Part::Lintel)
        else {
            continue;
        };
        let c = (head.min + head.max) * 0.5;
        let across_x = head.max.x - head.min.x < head.max.z - head.min.z;
        let y = LEVEL_H_M + 0.55;
        // The solid over the head, and its face on the doorway's outer side.
        let over = solids.iter().find(|b| {
            let along = if across_x { (b.min.z, b.max.z, c.z) } else { (b.min.x, b.max.x, c.x) };
            let thin = if across_x { b.max.x - b.min.x } else { b.max.z - b.min.z };
            along.0 <= along.2 && along.2 <= along.1 && b.min.y <= y && y <= b.max.y && thin < 0.5
        });
        let Some(over) = over else { continue };
        let (normal, at) = if across_x {
            let out = (c.x - middle.x).signum();
            (Vec3::X * out, Vec3::new(if out > 0.0 { over.max.x } else { over.min.x }, y, c.z))
        } else {
            let out = (c.z - middle.z).signum();
            (Vec3::Z * out, Vec3::new(c.x, y, if out > 0.0 { over.max.z } else { over.min.z }))
        };
        let w = site.to_walker(at);
        let n = site.to_walker(at + normal) - w;
        let deck = DeckPos::from_walker(site.deck, w);
        let turn = local_frame(site.deck, deck.s);
        out.push(Sign {
            text: site.prefab.name.into(),
            at: deck.to_colony(),
            normal: turn * n,
            up: turn * Vec3::Y,
            height: 0.45,
            ink: DARK,
        });
    }
    out
}

/// A point's UV distance check, for the tests: how far apart two points are in UV against in the
/// walker's frame.
#[cfg(test)]
pub(crate) fn uv_stretch(m: &PieceMesh) -> f32 {
    let mut worst: f32 = 0.0;
    for t in m.indices.chunks(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (a, b) = (a as usize, b as usize);
            let d3 = Vec3::from(m.positions[a]).distance(Vec3::from(m.positions[b]));
            let d2 = glam::Vec2::from(m.uvs[a]).distance(glam::Vec2::from(m.uvs[b]));
            if d3 > 0.05 {
                worst = worst.max((d2 / d3 - 1.0).abs());
            }
        }
    }
    worst
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

    /// Its UVs are metres across its faces (as near as bending them round the deck allows), so the
    /// panels, the band and the stripes are the sizes the station shader means them to be.
    #[test]
    fn its_uvs_are_metres() {
        for site in &SITES {
            let m = site_mesh(site);
            assert_eq!(m.uvs.len(), m.positions.len());
            let s = uv_stretch(&m);
            assert!(s < 0.03, "{}: UVs off metres by {:.1}%", site.prefab.name, s * 100.0);
        }
    }

    /// Its walls' kick plate is the dado the base paints them with, and their band over a
    /// doorway's head and under a storey's ceiling.
    #[test]
    fn the_dado_is_the_kick_plate() {
        let s = crate::station::Scale::MUSTER;
        assert_eq!(s.kick, DADO);
        assert_eq!(s.kick_paint, crate::station::OLIVE);
        assert!(s.band[0] > bc_sim::pieces::parts::OPENING_HEAD && s.band[1] < LEVEL_H_M);
    }

    /// Its name goes up over its doorway outside, on the wall there, facing out, in letters the
    /// atlas holds.
    #[test]
    fn its_name_is_over_its_doorway_outside() {
        for site in &SITES {
            let signs = site_signs(site);
            assert!(!signs.is_empty(), "{}: no sign", site.prefab.name);
            let mut solids = Vec::new();
            site.each_solid(&mut |b| solids.push(*b));
            let middle = {
                let b = site.bounds();
                (b.min + b.max) * 0.5
            };
            for s in &signs {
                assert!(crate::station::stencils(&s.text));
                let at = deck_at(s.at).expect("over its deck");
                let w = at.walker();
                let turn = local_frame(site.deck, at.s);
                let n = turn.inverse() * s.normal;
                assert!(n.y.abs() < 1e-4 && (n.length() - 1.0).abs() < 1e-4);
                assert!(n.dot(w - middle) > 0.0, "{}: it faces in", s.text);
                // On a solid's face: just inside it a little way behind the sign, and clear of it in front.
                let inside = |p: Vec3| solids.iter().any(|b| p.cmpgt(b.min).all() && p.cmplt(b.max).all());
                assert!(inside(w - n * 0.01), "{}: nothing behind it at {w}", s.text);
                assert!(!inside(w + n * 0.01), "{}: inside a solid at {w}", s.text);
            }
        }
    }
}
