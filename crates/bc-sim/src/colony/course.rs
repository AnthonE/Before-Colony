//! The Proving Ground's course (`docs/TRAINING.md`): rings in the colony's air that a suit flies
//! through in order. It starts just off the inner gate, where suits come in from the bays, and drops
//! out over the first window to the Charter strip. Then it runs along the avenue between the towers
//! and slaloms over its carriageways. It climbs, comes over the top and heads home. It ends on a pad
//! on Hub Gate's square, where landing on its feet stops the clock. X-Wing's Proving Ground, flown in
//! a mobile suit: the course teaches what the colony's inside does to a suit. There's no pull near
//! the axis and a g at the floor, Coriolis on the way down, the air, the city's walls, and the grip.
//!
//! Like the rest of the colony it's a closed form: the gates are content, and whether a suit's move
//! crossed one is geometry. Nothing of it is stored or sent. The pilot's client keeps the clock
//! (`bc_client_core::course`).

use core::f32::consts::FRAC_PI_6;

use glam::Vec3;

use super::frame::{CityPos, FIRST_WINDOW, STRIP_WIDTH, Under, from_colony, local_frame};
use crate::math::{length, normalize_or};
use crate::world::COLONY_RADIUS;

/// The strip the course runs over: Charter's, where the browser's cap lift comes down.
pub const STRIP: u8 = 0;

/// The middle of the strip's avenue, m across from its edge.
const AVENUE: f32 = STRIP_WIDTH * 0.5;

/// How a gate faces: the way a suit flies through it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Way {
    /// Along the course: from the gate before it to the gate after it (the pad after the last).
    Path,
    /// Set, in the strip's axes at the gate: along the colony, up (towards the axis), across.
    Local([f32; 3]),
}

/// A ring of the course.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// What the HUD calls the stretch it opens (only the gates that open one have a name).
    pub name: &'static str,
    /// Its centre in [`STRIP`]'s city coordinates: along the axis, across from the strip's edge
    /// (negative over the window before it), and up from the floor, m.
    pub x: f32,
    pub s: f32,
    pub h: f32,
    pub way: Way,
    /// How far from its centre a suit's middle may pass, m.
    pub radius: f32,
}

/// `s` on [`STRIP`] at angle `a` round the axis (rad, from +Y towards +Z): for the gates near the
/// axis, which aren't over the strip.
const fn across_at(a: f32) -> f32 {
    (a - (FIRST_WINDOW + FRAC_PI_6)) * COLONY_RADIUS
}

/// `h` at radius `r` from the axis.
const fn up_at_radius(r: f32) -> f32 {
    COLONY_RADIUS - r
}

const fn gate(name: &'static str, x: f32, s: f32, h: f32, way: Way, radius: f32) -> Gate {
    Gate { name, x, s, h, way, radius }
}

const ALONG: Way = Way::Local([1.0, 0.0, 0.0]);

/// The course, in the order it's flown.
pub const GATES: [Gate; 13] = [
    // Straight on from the inner gate's mouth, down the axis, where nothing pulls.
    gate("START", -15_500.0, across_at(0.0), up_at_radius(300.0), ALONG, 60.0),
    // Out over the first window towards the Charter strip, the pull growing all the way down.
    gate("DESCENT", -15_150.0, across_at(0.50), up_at_radius(800.0), Way::Path, 50.0),
    gate("", -14_800.0, across_at(1.00), up_at_radius(1_500.0), Way::Path, 50.0),
    gate("", -14_450.0, across_at(1.33), up_at_radius(2_300.0), Way::Path, 45.0),
    // Over the avenue, and down onto it between the towers.
    gate("AVENUE", -14_050.0, AVENUE, 260.0, Way::Path, 40.0),
    gate("", -13_650.0, AVENUE, 50.0, ALONG, 30.0),
    // Over one carriageway and the other, low.
    gate("SLALOM", -13_300.0, AVENUE - 15.0, 32.0, ALONG, 20.0),
    gate("", -12_950.0, AVENUE + 15.0, 32.0, ALONG, 20.0),
    gate("", -12_600.0, AVENUE - 15.0, 32.0, ALONG, 20.0),
    // Up, and over the top: heading home, upside down if you like.
    gate("CLIMB", -12_150.0, AVENUE, 280.0, Way::Local([0.5, 0.866, 0.0]), 40.0),
    gate("OVER THE TOP", -12_350.0, AVENUE, 620.0, Way::Local([-1.0, 0.0, 0.0]), 50.0),
    // Home down the avenue, and in low over Hub Gate's square.
    gate("HOME", -13_600.0, AVENUE, 400.0, Way::Path, 50.0),
    gate("FINAL", -15_250.0, AVENUE, 60.0, Way::Path, 40.0),
];

/// The pad on Hub Gate's square where the course ends: its middle (along, across) on [`STRIP`],
/// and how far from it a suit may stand, m.
pub const PAD: (f32, f32) = (-15_600.0, AVENUE + 130.0);
pub const PAD_RADIUS: f32 = 30.0;

/// A move longer than this isn't flown (a launch, a dock, a correction across the colony): it
/// crosses nothing, m.
pub const JUMP: f32 = 300.0;

/// Gate `i`'s centre, in the colony's own frame.
pub fn centre(i: usize) -> Vec3 {
    let g = &GATES[i];
    CityPos::new(STRIP, g.x, g.s, g.h).to_colony()
}

/// The pad's middle on the ground, in the colony's own frame.
pub fn pad_centre() -> Vec3 {
    CityPos::new(STRIP, PAD.0, PAD.1, 0.0).to_colony()
}

/// The way through gate `i`: a unit vector in the colony's own frame.
pub fn way(i: usize) -> Vec3 {
    let g = &GATES[i];
    match g.way {
        Way::Local([x, h, s]) => {
            // The walker's axes there are (x, h, −s).
            let local = Vec3::new(x, h, -s);
            normalize_or(local_frame(STRIP, g.s) * local, Vec3::X)
        }
        Way::Path => {
            let before = if i == 0 { centre(0) } else { centre(i - 1) };
            let after = if i + 1 < GATES.len() { centre(i + 1) } else { pad_centre() };
            normalize_or(after - before, Vec3::X)
        }
    }
}

/// Whether a suit moving from `a` to `b` (the colony's own frame) flew through gate `i` the right
/// way: how far along the move it crossed, 0..1.
pub fn crossed(i: usize, a: Vec3, b: Vec3) -> Option<f32> {
    if length(b - a) > JUMP {
        return None;
    }
    let (c, n) = (centre(i), way(i));
    let (da, db) = ((a - c).dot(n), (b - c).dot(n));
    if !(da < 0.0 && db >= 0.0) {
        return None;
    }
    let f = da / (da - db);
    let at = a + (b - a) * f;
    (length(at - c) <= GATES[i].radius).then_some(f)
}

/// Whether a suit standing at `p` (the colony's own frame) is on the pad.
pub fn on_pad(p: Vec3) -> bool {
    match from_colony(p) {
        Under::Land(c) if c.strip == STRIP => {
            let (ds, dx) = (c.s - PAD.1, c.x - PAD.0);
            ds * ds + dx * dx <= PAD_RADIUS * PAD_RADIUS
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::interior::{INNER_GATE, ground_under, probe};
    use crate::math::{angle_between, cos, sin};

    /// A suit's hull, m: no Leo-sized suit flown through a gate's middle touches anything.
    const HULL: f32 = 10.0;

    /// Points over the disc of gate `i` within `k` of its radius: its centre, and rings round it.
    fn disc(i: usize, k: f32, mut f: impl FnMut(Vec3)) {
        let (c, n) = (centre(i), way(i));
        let u = n.any_orthonormal_vector();
        let v = n.cross(u);
        f(c);
        for ring in 1..=4 {
            let r = GATES[i].radius * k * ring as f32 / 4.0;
            for j in 0..24 {
                let a = j as f32 * core::f32::consts::TAU / 24.0;
                f(c + (u * cos(a) + v * sin(a)) * r);
            }
        }
    }

    #[test]
    fn every_gate_is_inside_the_colony_and_clear_of_the_city() {
        for (i, g) in GATES.iter().enumerate() {
            disc(i, 0.8, |p| {
                let d = probe(p).dist;
                assert!(d > HULL, "gate {i} ({}): {d} m from a wall at {p}", g.name);
            });
        }
    }

    #[test]
    fn the_way_between_the_gates_is_clear() {
        // Straight from each gate to the next is clear by two hulls; on the last stretch, down
        // onto the pad, the nearest thing is the ground coming up to meet the suit.
        let mut points: [Vec3; GATES.len() + 1] = [Vec3::ZERO; GATES.len() + 1];
        for (i, p) in points.iter_mut().enumerate().take(GATES.len()) {
            *p = centre(i);
        }
        points[GATES.len()] = pad_centre();
        for w in 0..GATES.len() {
            let (a, b) = (points[w], points[w + 1]);
            let last = w + 1 == GATES.len();
            let n = 200;
            for k in 0..n {
                let f = k as f32 / n as f32;
                let p = a + (b - a) * f;
                let d = probe(p).dist;
                let clear = match from_colony(p) {
                    Under::Land(c) if last => (2.0 * HULL).min(0.9 * c.h),
                    _ => 2.0 * HULL,
                };
                assert!(d > clear, "between gates {w} and {}: {d} m at {p}", w + 1);
            }
        }
        // And the start is straight on from the inner gate.
        let to_start = centre(0) - INNER_GATE;
        assert!(angle_between(to_start, way(0)) < 0.1);
        assert!(to_start.length() > 150.0 && to_start.length() < 400.0, "{}", to_start.length());
    }

    #[test]
    fn each_gate_faces_the_way_the_course_comes_through_it() {
        // Flying from the gate before to the gate after passes through a gate's face, not its
        // edge: within 50° of its way, but for the climb and the turn over the top, which are flown
        // round a curve.
        for (i, g) in GATES.iter().enumerate().take(GATES.len() - 1).skip(1) {
            if matches!(g.name, "CLIMB" | "OVER THE TOP") {
                continue;
            }
            let along = centre(i + 1) - centre(i - 1);
            let off = angle_between(along, way(i)).to_degrees();
            assert!(off < 50.0, "gate {i}: {off}°");
        }
        assert!(way(10).dot(Vec3::X) < -0.99, "over the top heads home");
        assert!(way(9).dot(crate::colony::frame::up_at(centre(9))) > 0.8, "the climb climbs");
    }

    #[test]
    fn the_pad_is_open_ground_on_hub_gate_square() {
        // Flat ground under the pad and all round it: nothing stands on it, and a suit landing on it
        // lands on the square.
        for (ds, dx) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let c = CityPos::new(STRIP, PAD.0 + dx * PAD_RADIUS, PAD.1 + ds * PAD_RADIUS, 40.0);
            let under = ground_under(c.to_colony()).expect("over the city");
            assert!((under - 40.0).abs() < 0.5, "{under} m down to the square at {c:?}");
        }
        assert!(on_pad(pad_centre()));
        assert!(on_pad(CityPos::new(STRIP, PAD.0 + 20.0, PAD.1 - 20.0, 9.0).to_colony()));
        assert!(!on_pad(CityPos::new(STRIP, PAD.0 + 40.0, PAD.1, 9.0).to_colony()));
        assert!(!on_pad(CityPos::new(1, PAD.0, PAD.1, 9.0).to_colony()), "another strip's square");
        // Hub Gate's square runs to the city's first blocks at x −15,360.
        assert!(PAD.0 + PAD_RADIUS < -15_360.0 - 2.0 * HULL);
    }

    #[test]
    fn a_gate_counts_only_flown_through_its_ring_the_right_way() {
        let (c, n) = (centre(5), way(5));
        let side = n.any_orthonormal_vector();
        // Through the middle: half way along the move.
        let f = crossed(5, c - n * 2.0, c + n * 2.0).expect("through it");
        assert!((f - 0.5).abs() < 1e-3);
        // Near its rim, and just outside it.
        assert!(crossed(5, c - n + side * 28.0, c + n + side * 28.0).is_some());
        assert!(crossed(5, c - n + side * 31.0, c + n + side * 31.0).is_none());
        // The wrong way, alongside, and a jump straight past it.
        assert!(crossed(5, c + n * 2.0, c - n * 2.0).is_none());
        assert!(crossed(5, c + side * 2.0, c + side * 4.0).is_none());
        assert!(crossed(5, c - n * 200.0, c + n * 200.0).is_none());
        // Starting on its face and moving through counts once: from just behind it, not from on it.
        assert!(crossed(5, c, c + n).is_none());
    }
}
