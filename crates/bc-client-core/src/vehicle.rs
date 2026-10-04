//! Driving in the colony's city: cars and scooters from its motor pools (`bc_sim::colony::pools`),
//! on a bicycle model in a strip's city coordinates, against the city's walls
//! (`bc_sim::colony::city::solid`; kerbs it rides over, the canal it doesn't drive into).
//!
//! Headings are the walker's yaw (0 faces −s, across the strip; τ/4 faces +x, along it).

use bc_proto::presence::{PersonPose, RIDE_CAR, RIDE_SCOOTER};
use bc_sim::colony::city::{Stage, ground, solid};
use bc_sim::colony::frame::CityPos;
use glam::Vec3;

/// What's driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Car,
    Scooter,
}

/// How a kind of vehicle is built and handles: its size (m), wheelbase (m), top speed forward and
/// back (m/s), acceleration and braking (m/s²), the most its front wheel turns (rad) and how fast
/// (rad/s), and the driver's eye above the ground (m).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub wheelbase: f32,
    pub top: f32,
    pub reverse: f32,
    pub accel: f32,
    pub brake: f32,
    pub steer: f32,
    pub steer_rate: f32,
    pub eye: f32,
}

pub fn spec(kind: Kind) -> Spec {
    match kind {
        Kind::Car => Spec {
            length: 4.4,
            width: 1.8,
            height: 1.5,
            wheelbase: 2.7,
            top: 30.0,
            reverse: 6.0,
            accel: 4.0,
            brake: 9.0,
            steer: 0.6,
            steer_rate: 1.6,
            eye: 1.15,
        },
        Kind::Scooter => Spec {
            length: 1.9,
            width: 0.7,
            height: 1.2,
            wheelbase: 1.3,
            top: 22.0,
            reverse: 0.0,
            accel: 5.0,
            brake: 8.0,
            steer: 0.7,
            steer_rate: 2.2,
            eye: 1.55,
        },
    }
}

/// The driver's hands and feet: throttle (+) or brake and reverse (−), steering (+ to the left),
/// the handbrake.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub throttle: f32,
    pub steer: f32,
    pub handbrake: bool,
}

/// A vehicle on a strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vehicle {
    pub kind: Kind,
    pub strip: u8,
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    /// Along its heading (back: negative), m/s.
    pub speed: f32,
    /// How far its front wheel is turned, rad.
    pub wheel: f32,
}

impl Vehicle {
    pub fn new(kind: Kind, strip: u8, s: f32, x: f32, yaw: f32) -> Self {
        Self { kind, strip, x, s, yaw, speed: 0.0, wheel: 0.0 }
    }

    pub fn spec(&self) -> Spec {
        spec(self.kind)
    }

    /// Its heading in city terms: `(dx, ds)`.
    pub fn forward(&self) -> (f32, f32) {
        forward(self.yaw)
    }

    /// Whether it fits at `(s, x)` heading `yaw`: three boxes down its length clear of the walls
    /// above kerb height, and none of its corners over the canal.
    fn fits(&self, s: f32, x: f32, yaw: f32) -> bool {
        let sp = self.spec();
        let (fx, fs) = forward(yaw);
        let half = 0.5 * sp.width;
        for k in [-1.0f32, 0.0, 1.0] {
            let along = k * (0.5 * sp.length - half);
            let (cx, cs) = (x + fx * along, s + fs * along);
            let min = Vec3::new(cx - half, 0.35, -cs - half);
            let max = Vec3::new(cx + half, sp.height, -cs + half);
            if solid(self.strip, min, max, Stage(0)) {
                return false;
            }
        }
        let (rx, rs) = (-fs, fx);
        [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)].iter().all(|(a, b)| {
            let px = x + fx * a * 0.5 * sp.length + rx * b * half;
            let ps = s + fs * a * 0.5 * sp.length + rs * b * half;
            ground(self.strip, ps, px, Stage(0)) > -0.5
        })
    }

    /// Drives `dt` seconds on.
    pub fn step(&mut self, d: &Drive, dt: f32) {
        let sp = self.spec();
        let dt = dt.clamp(0.0, 0.1);
        // The wheel: less lock at speed.
        let lock = sp.steer / (1.0 + self.speed.abs() / 14.0);
        let want = d.steer.clamp(-1.0, 1.0) * lock;
        self.wheel += (want - self.wheel).clamp(-sp.steer_rate * dt, sp.steer_rate * dt);
        // Throttle, brakes, and the road's drag.
        let t = d.throttle.clamp(-1.0, 1.0);
        let mut a = if t > 0.0 {
            if self.speed < -0.1 {
                sp.brake * t
            } else {
                sp.accel * t * (1.0 - (self.speed / sp.top).powi(3)).max(0.0)
            }
        } else if t < 0.0 {
            if self.speed > 0.1 {
                sp.brake * t
            } else {
                sp.accel * t * (1.0 + self.speed / sp.reverse.max(0.1)).max(0.0)
            }
        } else {
            0.0
        };
        // Air (by the square of the speed) and the tyres.
        a -= self.speed * self.speed.abs() * 0.0012 + self.speed.signum() * 0.2;
        if d.handbrake {
            a -= self.speed.signum() * sp.brake;
        }
        let before = self.speed;
        self.speed += a * dt;
        // Braking stops it; it doesn't run it backwards (that takes another moment on the pedal).
        if before * self.speed < 0.0 {
            self.speed = 0.0;
        }
        if self.speed.abs() < 0.05 && t == 0.0 {
            self.speed = 0.0;
        }
        // The bicycle: it turns as fast as its speed and its wheel say.
        let turn = self.speed / sp.wheelbase * self.wheel.tan();
        let yaw = self.yaw + turn * dt;
        let (fx, fs) = forward(yaw);
        let (x, s) = (self.x + fx * self.speed * dt, self.s + fs * self.speed * dt);
        if self.fits(s, x, yaw) {
            (self.x, self.s, self.yaw) = (x, s, yaw.rem_euclid(std::f32::consts::TAU));
        } else {
            // A knock: it stops dead, bounced back a little.
            self.speed *= -0.2;
        }
    }

    /// Where its wheels stand, m (the kerb when it's up one).
    pub fn ground(&self) -> f32 {
        ground(self.strip, self.s, self.x, Stage(0)).max(0.0)
    }

    /// The driver, as the plaza has them.
    pub fn pose(&self, pitch: f32) -> PersonPose {
        PersonPose {
            strip: self.strip,
            x: self.x,
            s: self.s,
            h: self.ground(),
            yaw: self.yaw,
            pitch,
            speed: self.speed.abs(),
            grounded: true,
            running: false,
            ride: if self.kind == Kind::Car { RIDE_CAR } else { RIDE_SCOOTER },
        }
    }

    /// Where its driver gets out: beside it on the left, or, if something stands there (a lamp post,
    /// a tree), on the right, or behind it.
    pub fn door(&self) -> CityPos {
        let (fx, fs) = self.forward();
        let side = 0.5 * self.spec().width + 0.7;
        let back = 0.5 * self.spec().length + 0.7;
        // Left of the heading: the walker frame's heading turned a quarter.
        let spots = [(fs * side, -fx * side), (-fs * side, fx * side), (-fx * back, -fs * back)];
        let free = |(dx, ds): (f32, f32)| {
            let (x, s) = (self.x + dx, self.s + ds);
            let h = ground(self.strip, s, x, Stage(0)).max(0.0) + 0.05;
            !solid(
                self.strip,
                Vec3::new(x - 0.3, h, -s - 0.3),
                Vec3::new(x + 0.3, h + 1.8, -s + 0.3),
                Stage(0),
            )
        };
        let (dx, ds) = spots.into_iter().find(|d| free(*d)).unwrap_or(spots[0]);
        CityPos::new(self.strip, self.x + dx, self.s + ds, 0.0)
    }
}

/// A heading's direction in city terms, `(dx, ds)`: the walker frame's `(sin, 0, cos)` with
/// `s` = −z.
pub fn forward(yaw: f32) -> (f32, f32) {
    let (sy, cy) = yaw.sin_cos();
    (sy, -cy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::Rect;
    use bc_sim::colony::frame::STRIP_WIDTH;
    use bc_sim::colony::furniture::{AVENUE_LAMP, Furniture, Kind as FurnitureKind, each_furniture};
    use std::f32::consts::FRAC_PI_2;

    const DT: f32 = 1.0 / 60.0;

    fn on_the_avenue() -> Vehicle {
        // The in-bound road, heading +x.
        Vehicle::new(Kind::Car, 0, STRIP_WIDTH * 0.5 - 15.0, -12_000.0, FRAC_PI_2)
    }

    #[test]
    fn a_car_gets_up_to_speed_and_stops() {
        let mut v = on_the_avenue();
        for _ in 0..(20.0 / DT) as u32 {
            v.step(&Drive { throttle: 1.0, ..Drive::default() }, DT);
        }
        assert!(v.speed > 24.0 && v.speed <= 30.0, "{} m/s", v.speed);
        assert!(v.x > -12_000.0 + 300.0, "it went +x: {}", v.x);
        assert!((v.s - (STRIP_WIDTH * 0.5 - 15.0)).abs() < 0.01, "straight: {}", v.s);
        let mut stopped = 0.0;
        for k in 0..(10.0 / DT) as u32 {
            v.step(&Drive { throttle: -1.0, ..Drive::default() }, DT);
            if v.speed == 0.0 && stopped == 0.0 {
                stopped = k as f32 * DT;
            }
            if stopped > 0.0 {
                break;
            }
        }
        assert!(stopped > 0.0 && stopped < 5.0, "braked to a stop in {stopped} s");
        // Held on the brake, it backs up, slowly.
        for _ in 0..(5.0 / DT) as u32 {
            v.step(&Drive { throttle: -1.0, ..Drive::default() }, DT);
        }
        assert!(v.speed < 0.0 && v.speed >= -6.0, "{}", v.speed);
    }

    #[test]
    fn it_turns_the_way_its_wheel_says_and_walls_stop_it() {
        let mut v = on_the_avenue();
        v.speed = 8.0;
        for _ in 0..(2.0 / DT) as u32 {
            v.step(&Drive { throttle: 0.2, steer: 1.0, ..Drive::default() }, DT);
        }
        // Heading +x (yaw τ/4) with the walker's up, left is −z: +s. The yaw grows turning left.
        assert!(v.yaw > FRAC_PI_2 + 0.5, "it turned left: {}", v.yaw);
        assert!(v.s > STRIP_WIDTH * 0.5 - 15.0 + 2.0, "towards +s: {}", v.s);
        // Straight at a building across the avenue's rows: it stops at the wall.
        let mut v =
            Vehicle::new(Kind::Car, 0, STRIP_WIDTH * 0.5 - 30.0, -12_000.0 + 64.0, std::f32::consts::PI);
        for _ in 0..(15.0 / DT) as u32 {
            v.step(&Drive { throttle: 1.0, ..Drive::default() }, DT);
        }
        let probe = Vehicle { s: v.s + 1.0, ..v };
        assert!(v.speed.abs() < 10.0 || !probe.fits(v.s + 1.0, v.x, v.yaw), "{v:?}");
        assert!(v.fits(v.s, v.x, v.yaw), "never inside a wall: {v:?}");
    }

    /// An avenue lamp near x = −12,000 on strip 0's −s pavement.
    fn avenue_lamp() -> Furniture {
        let s = STRIP_WIDTH * 0.5 - AVENUE_LAMP;
        let area = Rect::new(s - 0.5, s + 0.5, -12_050.0, -11_950.0);
        let mut found = None;
        each_furniture(0, &area, Stage(0), |p| {
            found = Some(*p);
            p.kind == FurnitureKind::AvenueLamp
        });
        found.filter(|p| p.kind == FurnitureKind::AvenueLamp).expect("a lamp")
    }

    #[test]
    fn a_driver_parked_against_a_lamp_post_gets_out_the_other_side() {
        // Heading +x, its door's side 0.5 m from the post.
        let post = avenue_lamp();
        let reach = 0.5 * spec(Kind::Car).width + 0.5 + FurnitureKind::AvenueLamp.size().0;
        let v = Vehicle::new(Kind::Car, 0, post.s + reach, post.x, FRAC_PI_2);
        let d = v.door();
        assert!(d.s > v.s + 1.5 && (d.x - v.x).abs() < 0.01, "out the other side: {d:?}");
        let h = 0.05;
        let clear = !solid(
            0,
            Vec3::new(d.x - 0.3, h, -d.s - 0.3),
            Vec3::new(d.x + 0.3, h + 1.8, -d.s + 0.3),
            Stage(0),
        );
        assert!(clear, "into the clear: {d:?}");
    }

    #[test]
    fn a_car_driven_onto_the_pavement_stops_at_a_post() {
        let post = avenue_lamp();
        let mut v = Vehicle::new(Kind::Car, 0, post.s, post.x - 12.0, FRAC_PI_2);
        let front = |v: &Vehicle| v.x + 0.5 * v.spec().length;
        let mut most = front(&v);
        for _ in 0..(5.0 / DT) as u32 {
            v.step(&Drive { throttle: 1.0, ..Drive::default() }, DT);
            most = most.max(front(&v));
        }
        let face = post.solid.rect.x0;
        assert!(most <= face + 1e-3 && most > face - 1.0, "it stopped short of the post: {most} vs {face}");
    }

    #[test]
    fn the_driver_gets_out_beside_it_and_others_see_a_car() {
        let v = on_the_avenue();
        let d = v.door();
        assert!(((d.x - v.x).hypot(d.s - v.s) - 1.6).abs() < 0.01);
        let p = v.pose(0.0);
        assert!(p.driving() && p.riding().is_none());
    }
}
