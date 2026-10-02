//! Strolling in the colony: an agent on foot walks up an avenue from a strip's Hub Gate and back,
//! with the browser's own legs (`bc_client_core::walker`) among the city's walls
//! (`bc_client_core::city`), so the server takes every step. The `flaneur` example and
//! `bc-swarm --walkers` walk this way.

use bc_client_core::city::{CityGround, pose_of};
use bc_client_core::walker::{Stride, Walker};
use bc_proto::presence::PersonPose;
use bc_sim::colony::city::{Stage, place_door};
use bc_sim::colony::frame::CityPos;
use bc_sim::content::city::{PLACES, PlaceKind};
use glam::Vec3;

pub struct Stroll {
    pub strip: u8,
    start: CityPos,
    along: Vec3,
    reach: f32,
    pub walker: Walker,
    ground: CityGround,
}

impl Stroll {
    /// Out of strip `strip`'s Hub Gate door, `aside` metres to one side, facing up the avenue,
    /// to stroll `reach` metres up it and back.
    pub fn new(strip: u8, reach: f32, aside: f32) -> Self {
        let strip = strip % 3;
        let gate =
            PLACES.iter().find(|p| p.kind == PlaceKind::HubGate && p.strip == strip).expect("a Hub Gate");
        let ((s, x), (ds, dx)) = place_door(gate);
        let start = CityPos::new(strip, x - dx * 6.0, s - ds * 6.0 + aside, 0.0);
        let along = (CityPos::new(strip, start.x + 1.0, start.s, 0.0).walker() - start.walker()).normalize();
        Self {
            strip,
            start,
            along,
            reach,
            walker: Walker::at(start.walker(), along),
            ground: CityGround { strip, stage: Stage(0) },
        }
    }

    /// Where the walker is along the avenue, m from where it started.
    pub fn along(&self) -> f32 {
        CityPos::from_walker(self.strip, self.walker.feet).x - self.start.x
    }

    /// Walks on `dt` seconds (up the avenue to its reach, then back to the door, and again): the
    /// pose to send.
    pub fn step(&mut self, dt: f32) -> PersonPose {
        let x = self.along();
        let out = self.walker.heading().dot(self.along) > 0.0;
        if (out && x > self.reach) || (!out && x < 0.0) {
            self.walker.turn(std::f32::consts::PI, 0.0);
        }
        self.walker.step(&self.ground, &Stride { forward: 1.0, ..Stride::default() }, dt);
        pose_of(self.strip, &self.walker)
    }
}
