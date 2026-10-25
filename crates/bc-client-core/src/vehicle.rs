//! Driving in the colony's city: cars and scooters from its motor pools (`bc_sim::colony::pools`),
//! on `bc_sim::car`'s bicycle model (shared with the tick), and the driver as the plaza has them.
//!
//! Headings are the walker's yaw (0 faces −s, across the strip; τ/4 faces +x, along it).

use bc_proto::presence::{PersonPose, RIDE_CAR, RIDE_SCOOTER};

pub use bc_sim::car::{Drive, Kind, Spec, Vehicle, forward, spec};

/// The driver of a vehicle, as the plaza has them.
pub trait DriverPose {
    fn pose(&self, pitch: f32) -> PersonPose;
}

impl DriverPose for Vehicle {
    fn pose(&self, pitch: f32) -> PersonPose {
        PersonPose {
            strip: self.strip,
            x: self.x,
            s: self.s,
            h: self.ground(),
            yaw: self.yaw,
            pitch,
            roll: 0.0,
            speed: self.speed.abs(),
            grounded: true,
            running: false,
            ride: if self.kind == Kind::Car { RIDE_CAR } else { RIDE_SCOOTER },
            jets: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::frame::STRIP_WIDTH;

    #[test]
    fn others_see_a_driver_in_a_car() {
        let v = Vehicle::new(Kind::Car, 0, STRIP_WIDTH * 0.5 - 15.0, -12_000.0, std::f32::consts::FRAC_PI_2);
        let p = v.pose(0.0);
        assert!(p.driving() && p.riding().is_none());
        assert_eq!((p.x, p.s, p.h), (v.x, v.s, v.ground()));
    }
}
