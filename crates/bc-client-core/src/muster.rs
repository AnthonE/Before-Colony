//! Bay Row in the station language (`docs/RING.md`, "The base"; `station`): the walkway along the
//! base deck's back end in the hub's drum, every bay's airlock opening onto it. The client draws it
//! with the Muster's station material (`bc-client`'s `base`), in the colony's own frame, under the
//! base's root.
//!
//! - **The back end's wall**, the height of a doorway's lamp and more, in white panels over the
//!   base's olive dado, the band round it in Charter blue, its arrows pointing the short way round
//!   to the Muster's buildings.
//! - **The walkway**, deck plates with a lane each way in Charter blue, a hazard line along its edge.
//! - **Each bay's airlock** (`deck::airlock`): its door with chevrons at the leaves' meeting, a frame
//!   lit Gardens green round it, hazard posts either side, its lamp's head over the lintel with the
//!   airlock's amber light, and between the doors a lamp on the wall.
//! - **Signs:** each bay's number over its door, and KEEP CLEAR on the walkway before it.
//!
//! What's drawn matches what the walker meets: only the back end is solid here (`deck::solid`); the
//! doors, posts and lamps stand within the half-metre in front of it, as they always have.

use bc_sim::colony::deck::{BASE, BAY_ROW_WIDTH, DECKS, airlock, local_frame};
use bc_sim::colony::hub::BAYS as BAY_COUNT;
use bc_sim::content::base::SITES;
use bc_sim::pieces::parts::Aabb;
use glam::Vec3;

use crate::piece_mesh::{Frame, PieceMesh, walker_uv};
use crate::station::{
    ARROWS, BACK, BAND, CHARTER, CHEVRONS, DARK, GARDENS, HULL, LANES, Sign, Surface, WHITE, YELLOW, colour,
};

/// How high the back end's wall is drawn in the station language over the walkway, m: up to where
/// the drum's ribs stand on it.
pub const WALL_H: f32 = 9.0;
/// An airlock's door: half its width, its height, how far it stands out of the wall (m).
pub const DOOR_HALF: f32 = 2.0;
pub const DOOR_H: f32 = 4.8;
const DOOR_OUT: f32 = 0.6;
/// The lit frame round a door.
const FRAME: f32 = 0.15;
/// The hazard posts either side, their middles from the door's, half their width and their height.
const POST_AT: f32 = 2.6;
const POST_HALF: f32 = 0.3;
const POST_H: f32 = 5.0;
const POST_OUT: f32 = 0.9;
/// The lamp's head over the lintel, and its light.
const HEAD: [f32; 2] = [DOOR_H + FRAME, DOOR_H + FRAME + 0.9];
const HEAD_HALF: f32 = 2.8;
/// Where the number goes over a door, how tall, and where KEEP CLEAR goes before it.
const NUMBER_H: f32 = 6.45;
const NUMBER_SIZE: f32 = 0.75;
const KEEP_CLEAR_X: f32 = 1.85;
const KEEP_CLEAR_SIZE: f32 = 0.9;
/// The hazard line along the walkway's edge, m wide.
const EDGE: f32 = 0.6;
/// The bays, numbered from 1.
const BAYS: u8 = BAY_COUNT as u8;

fn deck() -> &'static bc_sim::colony::deck::Deck {
    &DECKS[BASE as usize]
}

/// How far round the deck from `s` to `to`, the short way, m.
fn round_to(s: f32, to: f32) -> f32 {
    let round = deck().round();
    (to - s + round * 1.5).rem_euclid(round) - round * 0.5
}

/// Where along the walker's frame's z a bay's door is (its `s`, as `−z`).
fn door_s(n: u8) -> f32 {
    airlock(n).s
}

/// Bay Row, in the colony's own frame.
pub fn bay_row() -> PieceMesh {
    use Surface::*;
    let d = deck();
    let (x0, round) = (d.x0, d.round());
    let mut m = PieceMesh::default();
    let f = Frame::COLONY;
    let flat = |w: Vec3, n: Vec3| walker_uv(w, n, 0.0);
    // The wall and the walkway round the whole deck, a strip at a time (their arrows each pointing
    // the short way round to the buildings).
    let strips = (round / 6.0).ceil() as usize;
    let to = SITES[0].s;
    for k in 0..strips {
        let (s0, s1) = (round * k as f32 / strips as f32, round * (k + 1) as f32 / strips as f32);
        let towards = round_to((s0 + s1) * 0.5, to);
        // UVs along a wall facing +x run along −s: arrows along +u point to lower s.
        let arrows = if towards < 0.0 { ARROWS } else { ARROWS | BACK };
        let len = Vec3::NEG_Z * (s1 - s0);
        m.rect(
            BASE,
            &f,
            Vec3::new(x0, 0.0, -s0),
            len,
            Vec3::Y * WALL_H,
            colour(Wall, CHARTER, BAND | arrows, 7),
            &flat,
        );
        let across = |w: Vec3, _: Vec3| [w.z, w.x - x0];
        let floor = colour(Floor, CHARTER, LANES, 11);
        m.rect(BASE, &f, Vec3::new(x0, 0.02, -s0), Vec3::X * BAY_ROW_WIDTH, len, floor, &across);
        let edge = colour(Hazard, WHITE, 0, 3);
        m.rect(BASE, &f, Vec3::new(x0 + BAY_ROW_WIDTH, 0.025, -s0), Vec3::X * EDGE, len, edge, &across);
    }
    // Each bay's airlock.
    let spacing = round / f32::from(BAYS);
    for n in 1..=BAYS {
        let s = door_s(n);
        let z = -s;
        let seed = n.wrapping_mul(37);
        let boxed = |m: &mut PieceMesh, lo: [f32; 3], hi: [f32; 3], c: [f32; 4], centred: bool| {
            let b =
                Aabb::new(Vec3::new(x0 + lo[0], lo[1], z + lo[2]), Vec3::new(x0 + hi[0], hi[1], z + hi[2]));
            let uv = move |w: Vec3, n: Vec3| {
                let mut uv = walker_uv(w, n, 0.0);
                if centred {
                    uv[0] -= z;
                }
                uv
            };
            m.cuboid(BASE, &f, &b, c, c, c, &uv);
        };
        // The door, its leaves meeting at its middle; the frame lit round it.
        boxed(
            &mut m,
            [0.0, 0.0, -DOOR_HALF],
            [DOOR_OUT, DOOR_H, DOOR_HALF],
            colour(Door, WHITE, CHEVRONS, seed),
            true,
        );
        let lit = colour(Glow, GARDENS, 0, seed);
        for side in [-1.0, 1.0] {
            let (a, b) = (side * DOOR_HALF, side * (DOOR_HALF + FRAME));
            boxed(&mut m, [0.0, 0.0, a.min(b)], [DOOR_OUT + 0.05, DOOR_H + FRAME, a.max(b)], lit, false);
        }
        boxed(&mut m, [0.0, DOOR_H, -DOOR_HALF], [DOOR_OUT + 0.05, DOOR_H + FRAME, DOOR_HALF], lit, false);
        // Its hazard posts.
        for side in [-1.0, 1.0] {
            let c = side * POST_AT;
            boxed(
                &mut m,
                [0.0, 0.0, c - POST_HALF],
                [POST_OUT, POST_H, c + POST_HALF],
                colour(Hazard, WHITE, 0, seed),
                false,
            );
        }
        // The lamp's head over the lintel, the airlock's amber light on its face.
        boxed(
            &mut m,
            [0.0, HEAD[0], -HEAD_HALF],
            [1.0, HEAD[1], HEAD_HALF],
            colour(Trim, HULL, 0, seed),
            false,
        );
        boxed(
            &mut m,
            [1.0, HEAD[0] + 0.1, -1.5],
            [1.1, HEAD[0] + 0.45, 1.5],
            colour(Glow, YELLOW, 0, seed),
            false,
        );
        // Between this door and the next, a lamp on the wall.
        let lamp = -spacing * 0.5;
        boxed(&mut m, [0.0, 6.75, lamp - 0.8], [0.8, 7.25, lamp + 0.8], colour(Glow, WHITE, 0, seed), false);
    }
    m
}

/// The road round the deck: how far out from the back end it starts and how wide it is (a lane each
/// way, `station::Scale::MUSTER`'s lanes), m.
const ROAD_X: f32 = 100.0;
const ROAD_W: f32 = 12.5;
/// The pads out past the road, where the base's carriers and suits stand: how far out from the back
/// end, how big, how far apart round the deck and the width of their hazard border, m.
const PAD_X: f32 = 130.0;
const PAD: f32 = 30.0;
const PAD_EVERY: f32 = 72.0;
const PAD_BORDER: f32 = 0.8;
/// How far under the deck its markings lie, m (the walker stands on the deck itself, and the
/// buildings' foundations, flush with it, hide what's under them).
const UNDER: f32 = 0.05;

/// The deck out past Bay Row, the drum's floor, so it reads as a parade ground in use, not a bare
/// plain (`docs/RING.md`, "The base"): deck plates over it all, a road round the deck with a lane
/// each way and hazard edges, and pads out past the road bordered in hazard stripes. Only paint:
/// nothing of it stands on the deck to walk into. In the colony's own frame.
pub fn deck_floor() -> PieceMesh {
    use Surface::*;
    let d = deck();
    let (x0, round) = (d.x0, d.round());
    let mut m = PieceMesh::default();
    let f = Frame::COLONY;
    let start = x0 + BAY_ROW_WIDTH + EDGE;
    let len = Vec3::NEG_Z * round;
    let flat = |from: f32| move |w: Vec3, _: Vec3| [w.z, w.x - from];
    // Side by side across the deck, none over another: plates, the road's edge, the road, its other
    // edge, plates.
    let road = x0 + ROAD_X;
    let plates = colour(Floor, WHITE, 0, 17);
    let edge = colour(Hazard, WHITE, 0, 3);
    for (from, to, c) in [
        (start, road - EDGE, plates),
        (road - EDGE, road, edge),
        (road, road + ROAD_W, colour(Floor, CHARTER, LANES, 13)),
        (road + ROAD_W, road + ROAD_W + EDGE, edge),
        (road + ROAD_W + EDGE, d.x1, plates),
    ] {
        m.rect(BASE, &f, Vec3::new(from, -UNDER, 0.0), Vec3::X * (to - from), len, c, &flat(from));
    }
    // The pads, each a square border of hazard stripes.
    let border = colour(Hazard, WHITE, 0, 5);
    let pads = (round / PAD_EVERY).floor() as usize;
    for k in 0..pads {
        let s = round * k as f32 / pads as f32 + PAD_EVERY * 0.25;
        let (x, z) = (x0 + PAD_X, -s);
        let uv = move |w: Vec3, _: Vec3| [w.z, w.x - x];
        let h = -UNDER + 0.02;
        let b = PAD_BORDER;
        for (at, across, along) in [
            (Vec3::new(x, h, z), Vec3::X * PAD, Vec3::NEG_Z * b),
            (Vec3::new(x, h, z - PAD + b), Vec3::X * PAD, Vec3::NEG_Z * b),
            (Vec3::new(x, h, z - b), Vec3::X * b, Vec3::NEG_Z * (PAD - 2.0 * b)),
            (Vec3::new(x + PAD - b, h, z - b), Vec3::X * b, Vec3::NEG_Z * (PAD - 2.0 * b)),
        ] {
            m.rect(BASE, &f, at, across, along, border, &uv);
        }
    }
    m
}

/// Bay Row's signs, in the colony's own frame: each bay's number over its door, KEEP CLEAR on the
/// walkway before it.
pub fn signs() -> Vec<Sign> {
    let d = deck();
    let mut out = Vec::with_capacity(2 * BAYS as usize);
    for n in 1..=BAYS {
        let s = door_s(n);
        let turn = local_frame(BASE, s);
        let at = |x: f32, h: f32| bc_sim::colony::deck::DeckPos::new(BASE, x, s, h).to_colony();
        out.push(Sign {
            text: format!("BAY {n:02}").into(),
            at: at(d.x0, NUMBER_H),
            normal: turn * Vec3::X,
            up: turn * Vec3::Y,
            height: NUMBER_SIZE,
            ink: DARK,
        });
        out.push(Sign {
            text: "KEEP CLEAR".into(),
            at: at(d.x0 + KEEP_CLEAR_X, 0.02),
            normal: turn * Vec3::Y,
            up: turn * Vec3::NEG_X,
            height: KEEP_CLEAR_SIZE,
            ink: YELLOW,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::deck::{deck_at, solid};

    fn triangles(m: &PieceMesh) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        m.indices.chunks(3).map(|t| [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize])))
    }

    /// Its faces are wound the way they face, and its UVs are metres across them.
    #[test]
    fn faces_point_where_they_face_and_their_uvs_are_metres() {
        let m = bay_row();
        assert_eq!(m.uvs.len(), m.positions.len());
        for (t, idx) in triangles(&m).zip(m.indices.chunks(3)) {
            let n = Vec3::from(m.normals[idx[0] as usize]);
            let area = (t[1] - t[0]).cross(t[2] - t[0]);
            if area.length() > 1e-6 {
                assert!(area.dot(n) > 0.0, "a face wound backwards at {}", t[0]);
            }
        }
        let s = crate::piece_mesh::uv_stretch(&m);
        assert!(s < 0.03, "UVs off metres by {:.1}%", s * 100.0);
    }

    /// One door to each bay's airlock, standing in front of the back end where the walker's
    /// airlock is, and nothing of Bay Row out past the walkway.
    #[test]
    fn a_door_to_every_airlock() {
        let m = bay_row();
        let d = deck();
        for n in 1..=BAYS {
            let door = airlock(n);
            let near = m
                .positions
                .iter()
                .filter_map(|p| deck_at(Vec3::from(*p)))
                .filter(|p| round_to(p.s, door.s).abs() < DOOR_HALF + 0.01 && p.h > 0.5 && p.x > d.x0 + 0.3)
                .count();
            assert!(near > 0, "no door at bay {n}");
        }
        for p in &m.positions {
            let at = deck_at(Vec3::from(*p)).expect("over the base deck");
            assert!(at.x >= d.x0 - 1e-3 && at.x <= d.x0 + BAY_ROW_WIDTH + EDGE + 1e-3, "{at:?}");
            assert!(at.h > -1e-3 && at.h <= WALL_H + 1e-3);
        }
        // The back end is solid where the wall's drawn; the walkway in front of the doors isn't.
        let w = airlock(1).walker();
        assert!(solid(BASE, w + Vec3::new(-0.4, 1.0, -0.1), w + Vec3::new(-0.2, 1.2, 0.1)));
        assert!(!solid(BASE, w + Vec3::new(2.3, 0.1, -0.2), w + Vec3::new(2.7, 1.8, 0.2)));
    }

    /// Every sign is on a face of Bay Row looking its way, in letters the atlas holds.
    #[test]
    fn signs_are_on_its_faces() {
        let m = bay_row();
        let tris: Vec<([Vec3; 3], Vec3)> = triangles(&m)
            .zip(m.indices.chunks(3))
            .map(|(t, idx)| (t, Vec3::from(m.normals[idx[0] as usize])))
            .collect();
        let signs = signs();
        assert_eq!(signs.len(), 2 * BAYS as usize);
        for s in &signs {
            assert!(crate::station::stencils(&s.text), "{}", s.text);
            let on = tris.iter().any(|(t, n)| {
                if n.dot(s.normal) < 0.999 {
                    return false;
                }
                let d = (s.at - t[0]).dot(*n);
                if d.abs() > 0.03 {
                    return false;
                }
                let p = s.at - *n * d;
                let (v0, v1, v2) = (t[1] - t[0], t[2] - t[0], p - t[0]);
                let (d00, d01, d11, d20, d21) = (v0.dot(v0), v0.dot(v1), v1.dot(v1), v2.dot(v0), v2.dot(v1));
                let den = d00 * d11 - d01 * d01;
                let (v, w) = ((d11 * d20 - d01 * d21) / den, (d00 * d21 - d01 * d20) / den);
                v >= -1e-4 && w >= -1e-4 && v + w <= 1.0 + 1e-4
            });
            assert!(on, "{} at {} is on no face looking {}", s.text, s.at, s.normal);
        }
        assert_eq!(signs[0].text, "BAY 01");
        assert_eq!(signs[2 * (BAYS as usize - 1)].text, format!("BAY {BAYS}"));
    }

    /// The deck's markings lie just under it, over the whole deck past Bay Row, wound to face up,
    /// clear of the Muster's buildings' insides (under their foundations), their UVs metres.
    #[test]
    fn the_deck_is_marked_under_its_surface() {
        let m = deck_floor();
        let d = deck();
        assert_eq!(m.uvs.len(), m.positions.len());
        for p in &m.positions {
            let at = deck_at(Vec3::from(*p)).expect("over the base deck");
            assert!(at.h < 0.0 && at.h > -UNDER - 1e-3, "{at:?}");
            assert!(at.x >= d.x0 + BAY_ROW_WIDTH + EDGE - 1e-3 && at.x <= d.x1 + 1e-3, "{at:?}");
        }
        for (t, idx) in triangles(&m).zip(m.indices.chunks(3)) {
            let n = Vec3::from(m.normals[idx[0] as usize]);
            let at = deck_at(t[0]).unwrap();
            let up = local_frame(BASE, at.s) * Vec3::Y;
            assert!(n.dot(up) > 0.99, "a marking not facing up at {at:?}");
        }
        assert!(crate::piece_mesh::uv_stretch(&m) < 0.03);
        // The pads and the road keep clear of the buildings' footprints.
        for site in &SITES {
            let b = site.bounds();
            assert!(b.max.x < d.x0 + ROAD_X - EDGE, "{} reaches the road", site.prefab.name);
        }
    }

    /// The band's arrows point the short way round to the Muster's buildings, on both sides of them.
    #[test]
    fn the_arrows_point_to_the_buildings() {
        let to = SITES[0].s;
        assert!(round_to(to - 50.0, to) > 0.0 && round_to(to + 50.0, to) < 0.0);
        let round = deck().round();
        assert!((round_to(to + round * 0.5 - 1.0, to) - (1.0 - round * 0.5)).abs() < 1e-2);
    }
}
