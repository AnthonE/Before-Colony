//! A hangar bay in the colony's docking hub, where a pilot walks on foot: its solid geometry (what
//! the walker collides with, and what the browser draws), the places a pilot can use (the
//! fabricator, the stores, the exchange terminal, the suit's maintenance console, the cockpit,
//! the airlock), and routes between them for anyone who'd rather be walked there.
//!
//! The bay hangs in the hub's spin ring, at 0.7 g. The suit stands in the middle, in its gantry,
//! facing the bay doors it launches through; a catwalk crosses in front of its chest at the
//! cockpit hatch, reached by the stairs along the left wall. Metres: x right (looking at the bay
//! doors), y up, z from the bay doors back; the floor's centre is the origin.

use glam::Vec3;

/// What a solid box is, for drawing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    Floor,
    Ceiling,
    Wall,
    /// The bay doors (they open for a launch) and the airlock's door.
    BayDoor,
    AirlockDoor,
    Catwalk,
    Stair,
    Rail,
    Pillar,
    /// The fabricator's bulk, and the stores' racks.
    Machine,
    Rack,
    /// A terminal's console.
    Console,
    Crate,
    /// Where the suit stands (drawn as the suit itself, not a box).
    Suit,
}

/// An axis-aligned solid box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub min: Vec3,
    pub max: Vec3,
    pub look: Look,
}

impl Block {
    const fn new(min: [f32; 3], max: [f32; 3], look: Look) -> Self {
        Self { min: Vec3::new(min[0], min[1], min[2]), max: Vec3::new(max[0], max[1], max[2]), look }
    }

    pub fn centre(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

/// A place in the bay the pilot can use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spot {
    /// Build: ore into materials, materials into parts and weapons.
    Fabricator,
    /// What's on the shelves.
    Stores,
    /// The Colony Exchange.
    Exchange,
    /// The suit's maintenance console: fit, strip, repair.
    Suit,
    /// Board the suit, and launch.
    Cockpit,
    /// The personnel airlock, out to the concourse.
    Airlock,
}

impl Spot {
    pub const ALL: [Spot; 6] =
        [Spot::Fabricator, Spot::Stores, Spot::Exchange, Spot::Suit, Spot::Cockpit, Spot::Airlock];

    pub fn name(self) -> &'static str {
        match self {
            Spot::Fabricator => "FABRICATOR",
            Spot::Stores => "STORES",
            Spot::Exchange => "COLONY EXCHANGE",
            Spot::Suit => "SUIT MAINTENANCE",
            Spot::Cockpit => "COCKPIT",
            Spot::Airlock => "AIRLOCK",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Spot::Fabricator => "fabricator",
            Spot::Stores => "stores",
            Spot::Exchange => "exchange",
            Spot::Suit => "suit",
            Spot::Cockpit => "cockpit",
            Spot::Airlock => "airlock",
        }
    }

    pub fn from_slug(s: &str) -> Option<Spot> {
        Self::ALL.into_iter().find(|p| p.slug() == s)
    }

    /// Where a pilot stands to use it (on the floor, or the catwalk), and which way they face.
    pub fn stand(self) -> (Vec3, Vec3) {
        match self {
            Spot::Fabricator => (Vec3::new(11.6, 0.0, 8.0), Vec3::X),
            Spot::Stores => (Vec3::new(13.8, 0.0, -12.0), Vec3::X),
            Spot::Exchange => (Vec3::new(-14.5, 0.0, -21.4), -Vec3::Z),
            Spot::Suit => (Vec3::new(6.5, 0.0, -3.0), Vec3::Z),
            Spot::Cockpit => (Vec3::new(0.0, CATWALK_Y, 2.0), Vec3::Z),
            Spot::Airlock => (Vec3::new(-15.4, 0.0, -20.0), -Vec3::X),
        }
    }

    /// Where it is (what a pilot looks at to use it).
    pub fn at(self) -> Vec3 {
        match self {
            Spot::Fabricator => Vec3::new(13.1, 1.3, 8.0),
            Spot::Stores => Vec3::new(15.4, 1.3, -12.0),
            Spot::Exchange => Vec3::new(-14.5, 1.3, -22.7),
            Spot::Suit => Vec3::new(6.5, 1.3, -1.2),
            Spot::Cockpit => HATCH,
            Spot::Airlock => Vec3::new(-16.9, 1.5, -20.0),
        }
    }
}

/// The bay's extent: x ±17, z ±24, 30 m high.
pub const HALF_WIDTH: f32 = 17.0;
pub const HALF_LENGTH: f32 = 24.0;
pub const HEIGHT: f32 = 30.0;
/// The bay doors' opening: x ±12, 26 m high, in the front wall (z = −24).
pub const DOOR_HALF_WIDTH: f32 = 12.0;
pub const DOOR_HEIGHT: f32 = 26.0;
/// The catwalk's floor.
pub const CATWALK_Y: f32 = 11.4;
/// Where the suit stands: its origin (the torso) above the floor, facing the bay doors (−z).
pub const SUIT_AT: Vec3 = Vec3::new(0.0, 9.6, 6.0);
/// The cockpit hatch, in the front of the suit's chest.
pub const HATCH: Vec3 = Vec3::new(0.0, 12.6, 3.7);
/// Gravity in the hub's spin ring, m/s² (0.7 g).
pub const GRAVITY: f32 = 6.9;
/// Where a pilot comes in: just inside the airlock, facing the suit.
pub const SPAWN: Vec3 = Vec3::new(-15.4, 0.0, -20.0);

/// The stairs: 38 steps of 0.3 m, 0.4 m deep, climbing along the left wall towards the catwalk.
pub const STEPS: usize = 38;
const STEP_RISE: f32 = 0.3;
const STEP_RUN: f32 = 0.4;
const STAIR_X: [f32; 2] = [-16.6, -13.4];
const STAIR_Z0: f32 = -14.6;
const CATWALK_Z: [f32; 2] = [0.6, 3.2];

/// A bay: every solid block, in a fixed order (the doors first).
#[derive(Clone, Debug)]
pub struct Layout {
    pub blocks: Vec<Block>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        use Look::*;
        let (w, l, h) = (HALF_WIDTH, HALF_LENGTH, HEIGHT);
        let t = 1.0;
        let mut b = vec![
            // The bay doors, and the wall round them.
            Block::new([-DOOR_HALF_WIDTH, 0.0, -l - t], [DOOR_HALF_WIDTH, DOOR_HEIGHT, -l], BayDoor),
            Block::new([-w, 0.0, -l - t], [-DOOR_HALF_WIDTH, h, -l], Wall),
            Block::new([DOOR_HALF_WIDTH, 0.0, -l - t], [w, h, -l], Wall),
            Block::new([-DOOR_HALF_WIDTH, DOOR_HEIGHT, -l - t], [DOOR_HALF_WIDTH, h, -l], Wall),
            // The airlock's door, in the left wall.
            Block::new([-w - t, 0.0, -21.3], [-w, 3.0, -18.7], AirlockDoor),
            Block::new([-w - t, 0.0, -l], [-w, h, -21.3], Wall),
            Block::new([-w - t, 0.0, -18.7], [-w, h, l], Wall),
            Block::new([-w - t, 3.0, -21.3], [-w, h, -18.7], Wall),
            Block::new([w, 0.0, -l], [w + t, h, l], Wall),
            Block::new([-w, 0.0, l], [w, h, l + t], Wall),
            Block::new([-w, -t, -l], [w, 0.0, l], Floor),
            Block::new([-w, h, -l], [w, h + t, l], Ceiling),
            // The catwalk across the suit's chest, and its landing at the top of the stairs.
            Block::new([STAIR_X[0], CATWALK_Y - 0.4, CATWALK_Z[0]], [9.0, CATWALK_Y, CATWALK_Z[1]], Catwalk),
            Block::new(
                [STAIR_X[1], CATWALK_Y, CATWALK_Z[0] - 0.15],
                [9.0, CATWALK_Y + 1.1, CATWALK_Z[0]],
                Rail,
            ),
            Block::new([9.0, CATWALK_Y, CATWALK_Z[0]], [9.15, CATWALK_Y + 1.1, CATWALK_Z[1]], Rail),
            // The rail on the suit's side, open at the hatch.
            Block::new(
                [STAIR_X[1], CATWALK_Y, CATWALK_Z[1]],
                [-1.3, CATWALK_Y + 1.1, CATWALK_Z[1] + 0.15],
                Rail,
            ),
            Block::new([1.3, CATWALK_Y, CATWALK_Z[1]], [9.0, CATWALK_Y + 1.1, CATWALK_Z[1] + 0.15], Rail),
            Block::new([-13.2, 0.0, 1.7], [-12.8, CATWALK_Y - 0.4, 2.1], Pillar),
            Block::new([8.4, 0.0, 1.7], [8.8, CATWALK_Y - 0.4, 2.1], Pillar),
            // The suit: its legs and its body, so nobody walks through it.
            Block::new([-2.7, 0.0, 4.3], [-0.2, 9.2, 7.6], Suit),
            Block::new([0.2, 0.0, 4.3], [2.7, 9.2, 7.6], Suit),
            Block::new([-2.6, 9.2, 3.5], [2.6, 17.5, 8.6], Suit),
            // The fabricator along the right wall, the stores' racks, and their consoles.
            Block::new([13.4, 0.0, 2.0], [w, 6.5, 14.0], Machine),
            Block::new([15.6, 0.0, -18.0], [w, 4.2, -6.0], Rack),
            Block::new([12.6, 0.0, 7.6], [13.4, 1.1, 8.4], Console),
            Block::new([15.0, 0.0, -12.4], [15.6, 1.1, -11.6], Console),
            Block::new([-14.9, 0.0, -24.0], [-14.1, 1.1, -23.2], Console),
            Block::new([6.1, 0.0, -1.6], [6.9, 1.1, -0.8], Console),
            // Crates at the back.
            Block::new([9.0, 0.0, 17.0], [11.0, 2.0, 19.0], Crate),
            Block::new([11.4, 0.0, 17.2], [13.2, 1.8, 19.0], Crate),
            Block::new([9.6, 2.0, 17.4], [11.0, 3.4, 18.8], Crate),
            Block::new([-6.0, 0.0, 19.0], [-3.6, 2.4, 21.4], Crate),
        ];
        // The stairs, each step solid to the floor.
        for k in 0..STEPS {
            let z = STAIR_Z0 + k as f32 * STEP_RUN;
            b.push(Block::new(
                [STAIR_X[0], 0.0, z],
                [STAIR_X[1], (k + 1) as f32 * STEP_RISE, z + STEP_RUN],
                Stair,
            ));
        }
        Self { blocks: b }
    }

    /// Whether a box overlaps anything solid.
    pub fn hits(&self, min: Vec3, max: Vec3) -> bool {
        self.blocks.iter().any(|b| overlap(min, max, b))
    }

    /// The pilot's feet at `feet` are standing where they could use `spot`, and looking at it
    /// (`dir`) near enough.
    pub fn can_use(spot: Spot, eye: Vec3, dir: Vec3) -> bool {
        let at = spot.at();
        let to = at - eye;
        let reach = if spot == Spot::Cockpit { 2.6 } else { 2.8 };
        to.length() < reach + 0.8 && dir.normalize_or_zero().dot(to.normalize_or_zero()) > 0.6
    }

    /// The place the pilot at `eye` looking along `dir` could use, if any (the nearest).
    pub fn spot_in_view(eye: Vec3, dir: Vec3) -> Option<Spot> {
        Spot::ALL
            .into_iter()
            .filter(|s| Self::can_use(*s, eye, dir))
            .min_by(|a, b| (a.at() - eye).length_squared().total_cmp(&(b.at() - eye).length_squared()))
    }

    /// Waypoints from `from` (feet) to where one stands to use `spot`: round the suit on the
    /// floor, and by the stairs to and from the catwalk.
    pub fn route(&self, from: Vec3, spot: Spot) -> Vec<Vec3> {
        let foot = Vec3::new(-15.0, 0.0, STAIR_Z0 - 1.2);
        let top = Vec3::new(-15.0, CATWALK_Y, 1.9);
        let on_catwalk = from.y > CATWALK_Y - 1.0;
        let (goal, _) = spot.stand();
        let mut out = Vec::new();
        let floor_leg = |out: &mut Vec<Vec3>, a: Vec3, b: Vec3| {
            // Round the suit's feet and the stairs, via a point clear of both.
            if self.floor_path_blocked(a, b) {
                let side = if (a.x + b.x) * 0.5 > -6.0 { 6.5 } else { -9.0 };
                out.push(Vec3::new(side, 0.0, -4.0));
            }
            out.push(b);
        };
        match (on_catwalk, goal.y > 1.0) {
            (false, false) => floor_leg(&mut out, from, goal),
            (false, true) => {
                floor_leg(&mut out, from, foot);
                out.push(top);
                out.push(goal);
            }
            (true, false) => {
                out.push(top);
                out.push(foot);
                floor_leg(&mut out, foot, goal);
            }
            (true, true) => out.push(goal),
        }
        out
    }

    /// Whether walking straight from `a` to `b` on the floor would run into something tall.
    fn floor_path_blocked(&self, a: Vec3, b: Vec3) -> bool {
        let steps = ((b - a).length() / 0.25).ceil().max(1.0) as usize;
        (0..=steps).any(|k| {
            let p = a + (b - a) * (k as f32 / steps as f32);
            let half = Vec3::new(0.45, 0.0, 0.45);
            self.blocks.iter().any(|blk| {
                blk.max.y > 0.35 && overlap(p - half + Vec3::Y * 0.4, p + half + Vec3::Y * 1.8, blk)
            })
        })
    }
}

fn overlap(min: Vec3, max: Vec3, b: &Block) -> bool {
    min.x < b.max.x
        && max.x > b.min.x
        && min.y < b.max.y
        && max.y > b.min.y
        && min.z < b.max.z
        && max.z > b.min.z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_places_stand_clear_and_face_what_they_use() {
        let bay = Layout::new();
        for s in Spot::ALL {
            let (feet, facing) = s.stand();
            let body = Vec3::new(0.3, 0.0, 0.3);
            assert!(
                !bay.hits(feet - body + Vec3::Y * 0.05, feet + body + Vec3::Y * 1.8),
                "{s:?} stands in something"
            );
            let eye = feet + Vec3::Y * 1.62;
            assert!(Layout::can_use(s, eye, (s.at() - eye).normalize()), "{s:?} out of reach");
            assert_eq!(Layout::spot_in_view(eye, (s.at() - eye).normalize()), Some(s), "{s:?}");
            assert!(facing.dot((s.at() - feet).with_y(0.0).normalize()) > 0.7, "{s:?} faces away");
            assert_eq!(Spot::from_slug(s.slug()), Some(s));
        }
        // Looking away uses nothing.
        let (feet, facing) = Spot::Fabricator.stand();
        assert_eq!(Layout::spot_in_view(feet + Vec3::Y * 1.62, -facing), None);
    }

    #[test]
    fn the_stairs_climb_to_the_catwalk() {
        let bay = Layout::new();
        let top = bay.blocks.iter().filter(|b| b.look == Look::Stair).map(|b| b.max.y).fold(0.0, f32::max);
        assert!((top - CATWALK_Y).abs() < 1e-4, "{top}");
        // The hatch is at a pilot's eye on the catwalk.
        assert!((HATCH.y - (CATWALK_Y + 1.62)).abs() < 1.0);
        const { assert!(HATCH.z > CATWALK_Z[1]) };
    }
}
