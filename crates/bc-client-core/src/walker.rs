//! On foot: a first-person body (a box 0.6 m across and 1.8 m tall) under the gravity of where it
//! is. It slides along what it runs into, steps up stairs and kerbs, jumps, and falls. The body is
//! `bc_sim::foot`'s, shared with the tick; what it walks among is a [`Solid`]: the bay's layout
//! (here), the colony's city (`crate::city`), the inside of a car it rides. And a guide that walks
//! it along a route, for tests, agents and the browser's autopilot.

use glam::Vec3;

use crate::bay::Layout;

pub use bc_sim::foot::{EYE, STEP, Solid, Stride, Walker};

// The bay keeps its own name for its pull; the walker's default is the same.
const _: () = assert!(crate::bay::GRAVITY == bc_sim::foot::GRAVITY);

impl Solid for Layout {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        Layout::hits(self, min, max)
    }
}

/// A guide stuck this long (s) gives up.
const GIVE_UP: f32 = 6.0;
/// Once caught on something (a lamp post, a corner), it keeps stepping aside this long (s) after
/// it's moving again: long enough to get past a post or a trunk, not just off it. Each time it's
/// caught again it tries the other side (the first, right: caught on a post's edge, the short way
/// round may be either).
const SIDESTEP: f32 = 0.25;

/// Walks a pilot along a route of waypoints (from [`Layout::route`]).
#[derive(Clone, Debug, Default)]
pub struct Guide {
    pub route: Vec<Vec3>,
    /// The way to face on arrival.
    pub facing: Option<Vec3>,
    stuck: f32,
    aside: f32,
    /// How many sidesteps it's begun: the odd ones go right, the even ones left.
    sidesteps: u32,
    last: Option<Vec3>,
}

impl Guide {
    pub fn new(route: Vec<Vec3>, facing: Option<Vec3>) -> Self {
        Self { route, facing, stuck: 0.0, aside: 0.0, sidesteps: 0, last: None }
    }

    pub fn arrived(&self) -> bool {
        self.route.is_empty()
    }

    /// Points the walker at the next waypoint and says how to stride there (hands off once it's
    /// arrived, turned to face `facing`).
    pub fn steer(&mut self, w: &mut Walker, dt: f32) -> Stride {
        while let Some(next) = self.route.first().copied() {
            let flat = Vec3::new(next.x - w.feet.x, 0.0, next.z - w.feet.z);
            if flat.length() < 0.35 && (next.y - w.feet.y).abs() < 1.0 {
                self.route.remove(0);
                continue;
            }
            // Turn towards it, not in one snap.
            let want = flat.x.atan2(flat.z);
            let d = (want - w.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            w.turn(d.clamp(-6.0 * dt, 6.0 * dt), -w.pitch * (4.0 * dt).min(1.0));
            // Nudge sideways out of a corner it's caught on.
            let moved = self.last.map_or(1.0, |p| (w.feet - p).length());
            self.last = Some(w.feet);
            self.stuck = if moved < 0.004 { self.stuck + dt } else { 0.0 };
            if self.stuck > 0.4 {
                if self.aside <= 0.0 {
                    self.sidesteps += 1;
                }
                self.aside = SIDESTEP;
            } else {
                self.aside = (self.aside - dt).max(0.0);
            }
            // Nudged and jumped for long enough without getting anywhere: it gives up (the way is
            // shut: a tram's doors closed on it, say), and whoever asked can ask again.
            if self.stuck > GIVE_UP {
                self.route.clear();
                return Stride::default();
            }
            let facing_it = d.abs() < 0.6;
            return Stride {
                forward: if facing_it { 1.0 } else { 0.25 },
                right: match (self.aside > 0.0, self.sidesteps % 2) {
                    (false, _) => 0.0,
                    (true, 1) => 0.8,
                    (true, _) => -0.8,
                },
                run: flat.length() > 4.0,
                jump: self.stuck > 1.2,
            };
        }
        if let Some(f) = self.facing {
            let want = f.x.atan2(f.z);
            let d = (want - w.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            w.turn(d.clamp(-6.0 * dt, 6.0 * dt), -w.pitch * (4.0 * dt).min(1.0));
        }
        Stride::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bay::{CATWALK_Y, SPAWN, Spot};

    const DT: f32 = 1.0 / 60.0;

    fn settle(w: &mut Walker, bay: &Layout) {
        for _ in 0..60 {
            w.step(bay, &Stride::default(), DT);
        }
    }

    #[test]
    fn a_pilot_stands_walks_and_runs_into_walls() {
        let bay = Layout::new();
        let mut w = Walker::at(SPAWN, Vec3::X);
        settle(&mut w, &bay);
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
        let still = w.feet;
        settle(&mut w, &bay);
        assert!((w.feet - still).length() < 1e-4, "no drift");
        // Into the left wall (x = −17): it stops at the wall.
        let mut w = Walker::at(Vec3::new(-10.0, 0.0, 10.0), -Vec3::X);
        for _ in 0..600 {
            w.step(&bay, &Stride { forward: 1.0, run: true, ..Stride::default() }, DT);
        }
        assert!(w.feet.x > -17.0 + 0.29 && w.feet.x < -17.0 + 0.4, "{:?}", w.feet);
        // A jump goes up and comes down.
        let mut w = Walker::at(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);
        settle(&mut w, &bay);
        w.step(&bay, &Stride { jump: true, ..Stride::default() }, DT);
        let mut top: f32 = 0.0;
        for _ in 0..120 {
            w.step(&bay, &Stride::default(), DT);
            top = top.max(w.feet.y);
        }
        // As high as a jump at 0.9 g, though the bay's pull is 0.4 g (`bc_sim::foot::JUMP`).
        assert!(top > 0.65 && top < 0.8 && w.grounded && w.feet.y.abs() < 1e-3, "{top}");
    }

    #[test]
    fn the_guide_walks_from_the_airlock_to_every_place() {
        let bay = Layout::new();
        for spot in Spot::ALL {
            let mut w = Walker::at(SPAWN, Vec3::X);
            settle(&mut w, &bay);
            let (_, facing) = spot.stand();
            let mut g = Guide::new(bay.route(w.feet, spot), Some(facing));
            let mut t = 0.0;
            while !g.arrived() || (w.heading().dot(facing) < 0.95 && t < 60.0) {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
                t += DT;
                assert!(t < 60.0, "{spot:?}: lost at {:?} after {t:.0} s", w.feet);
            }
            for _ in 0..30 {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
            }
            assert_eq!(Layout::spot_in_view(w.eye(), w.look()), Some(spot), "{spot:?} at {:?}", w.feet);
            if spot == Spot::Cockpit {
                assert!((w.feet.y - CATWALK_Y).abs() < 0.05, "up on the catwalk: {:?}", w.feet);
            }
            // And back down, to the fabricator.
            let mut g = Guide::new(bay.route(w.feet, Spot::Fabricator), None);
            let mut t = 0.0;
            while !g.arrived() {
                let s = g.steer(&mut w, DT);
                w.step(&bay, &s, DT);
                t += DT;
                assert!(t < 60.0, "{spot:?} → fabricator: lost at {:?}", w.feet);
            }
        }
    }

    #[test]
    fn benches_are_a_step_up() {
        // The city's benches are stepped up onto, not walked into.
        const { assert!(bc_sim::colony::furniture::BENCH_HEIGHT < STEP) };
    }

    #[test]
    fn a_fall_from_the_catwalk_lands_on_the_floor() {
        let bay = Layout::new();
        let mut w = Walker::at(Vec3::new(4.0, CATWALK_Y, 1.9), -Vec3::Z);
        settle(&mut w, &bay);
        assert!(w.grounded && (w.feet.y - CATWALK_Y).abs() < 1e-3);
        // Over the rail and down.
        w.feet = Vec3::new(4.0, CATWALK_Y + 1.3, 0.2);
        for _ in 0..240 {
            w.step(&bay, &Stride { forward: 1.0, ..Stride::default() }, DT);
        }
        assert!(w.grounded && w.feet.y.abs() < 1e-3, "{:?}", w.feet);
    }
}
