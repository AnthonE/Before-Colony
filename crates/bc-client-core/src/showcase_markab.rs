//! The Markab showcase's script (`?showcase=markab`), testable: a Markab's stunt in the Blast Hall
//! (`bc_sim::colony::hall`), the rocket car's first public run (`docs/COLONY.md`, phase 5) done the
//! colony's way. Its test driver comes in through the blast doors, drives up the back wall and on
//! along the ceiling, stops there and hangs upside down a few seconds, backs along it and down the
//! wall tail first onto the floor, turns round, and runs for the doors, leaping out of them on the
//! lift jets in a barrel roll to come down on its wheels on Hub Gate's square. All of it is the real
//! car (`markab`) at the scene's fixed step with the driver at its keys: the same on every run.

use bc_proto::presence::PersonPose;
use bc_sim::colony::hall::hall;
use glam::Vec3;

use crate::markab::{Chase, Controls, Markab};

/// The test driver, by name (who rode the winged horse).
pub const DRIVER: &str = "Bellerophon";
/// The stunt's loop, s.
pub const PERIOD: f32 = 42.0;
/// Its step, s (the showcase's clock's).
const STEP: f32 = 1.0 / 60.0;
/// Its lane up the hall: this far along its front from the doors' middle, m (clear of the
/// instructor's desk and the gantry's pad).
const LANE: f32 = 12.0;
/// On the ceiling it brakes once it's this far in from the hall's front face, m, and hangs there
/// this long, s.
const HANG_AT: f32 = 58.0;
const HANG_SECS: f32 = 3.5;
/// It leaps this far in from the front face, m.
const LEAP_AT: f32 = 6.0;

/// What the driver is doing, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Act {
    /// In through the blast doors, up the back wall, and on along the ceiling.
    Up,
    /// Braking to a stop on the ceiling.
    Stop,
    /// Hanging there upside down.
    Hang,
    /// Back along the ceiling, and down the wall tail first onto the floor.
    Back,
    /// Braking to a stop on the floor, nose to the wall.
    Settle,
    /// Turning round to face the doors.
    Turn,
    /// For the doors.
    Run,
    /// Off the floor on the lift jets, the tail's lit.
    Leap,
    /// A barrel roll over the square.
    Roll,
    /// Down, and braking.
    Land,
    Done,
}

/// How many acts there are.
pub const ACTS: usize = Act::Done as usize + 1;

/// The stunt so far.
#[derive(Clone, Copy, Debug)]
pub struct Stunt {
    /// How far into the loop, s.
    pub t: f32,
    pub car: Markab,
    /// The camera behind it.
    pub chase: Chase,
    pub act: Act,
    /// When each act began, s (−1: not yet).
    pub began: [f32; ACTS],
}

/// A point of the Blast Hall in the walker's frame: `u` along its front from the doors' middle,
/// `v` in from its front face, `h` up.
pub fn in_hall(u: f32, v: f32, h: f32) -> Vec3 {
    let (s, x) = hall().front.point(u, v);
    Vec3::new(x, h, -s)
}

/// Into the hall, in the walker's frame.
fn inward() -> Vec3 {
    let (ds, dx) = hall().inward;
    Vec3::new(dx, 0.0, -ds)
}

impl Default for Stunt {
    fn default() -> Self {
        Self::new()
    }
}

impl Stunt {
    /// At the start: outside the blast doors, facing in through them.
    pub fn new() -> Self {
        let r = hall();
        let start = in_hall(LANE, -25.0, 0.0);
        let (ds, dx) = r.inward;
        let car = Markab::new(r.strip, -start.z, start.x, dx.atan2(-ds));
        let mut began = [-1.0; ACTS];
        began[0] = 0.0;
        Self { t: 0.0, car, chase: Chase::new(&car), act: Act::Up, began }
    }

    /// Runs on to `t` s into the loop (from its start again, if that's behind it).
    pub fn run_to(&mut self, t: f32) {
        let t = t.rem_euclid(PERIOD);
        if t < self.t - 1e-4 {
            *self = Self::new();
        }
        while self.t + STEP <= t + 1e-4 {
            let c = self.driver();
            self.car.step(&c, STEP);
            self.chase.follow(&self.car, STEP);
            self.t += STEP;
        }
    }

    /// The driver, as the plaza would have them.
    pub fn pose(&self) -> PersonPose {
        self.car.pose()
    }

    fn next(&mut self, act: Act) {
        self.act = act;
        self.began[act as usize] = self.t;
    }

    /// How long the act has gone on, s.
    fn since(&self) -> f32 {
        self.t - self.began[self.act as usize]
    }

    /// The driver at its keys: what the act asks for, and on to the next once it's done.
    fn driver(&mut self) -> Controls {
        let m = self.car;
        let idle = Controls::default();
        let brake = Controls { throttle: -m.ahead().signum(), ..idle };
        // How far in from the hall's front face it is.
        let v = (m.pos - in_hall(0.0, 0.0, m.pos.y)).dot(inward());
        match self.act {
            Act::Up if m.face() == Some(Vec3::NEG_Y) && v < HANG_AT => {
                self.next(Act::Stop);
                brake
            }
            Act::Up => Controls { throttle: 1.0, ..idle },
            Act::Stop if m.speed() < 0.05 => {
                self.next(Act::Hang);
                idle
            }
            Act::Stop => brake,
            Act::Hang => {
                if self.since() > HANG_SECS {
                    self.next(Act::Back);
                }
                idle
            }
            Act::Back => {
                if m.face() == Some(Vec3::Y) {
                    self.next(Act::Settle);
                }
                Controls { throttle: -1.0, ..idle }
            }
            Act::Settle if m.speed() < 0.05 => {
                self.next(Act::Turn);
                idle
            }
            Act::Settle => brake,
            Act::Turn => {
                if m.forward().dot(-inward()) > 0.98 {
                    self.next(Act::Run);
                }
                Controls { throttle: 0.4, steer: 1.0, ..idle }
            }
            Act::Run => {
                if v < LEAP_AT {
                    self.next(Act::Leap);
                }
                // For the doors' middle.
                let to = (in_hall(0.0, 0.0, m.pos.y) - m.pos).normalize_or(Vec3::ZERO);
                Controls { throttle: 1.0, steer: (m.left().dot(to) * 3.0).clamp(-1.0, 1.0), ..idle }
            }
            Act::Leap => {
                if self.since() > 0.4 {
                    self.next(Act::Roll);
                }
                Controls { lift: true, boost: true, ..idle }
            }
            Act::Roll => {
                if self.since() > 1.3 {
                    self.next(Act::Land);
                }
                Controls { roll: 1.0, lift: self.since() < 0.5, ..idle }
            }
            Act::Land if m.wheels_down() && m.speed() < 0.05 => {
                self.next(Act::Done);
                idle
            }
            Act::Land if m.wheels_down() => brake,
            Act::Land | Act::Done => idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{Stage, solid};
    use std::f32::consts::PI;

    #[test]
    fn the_stunt_goes_through_and_comes_down_on_its_wheels() {
        let mut st = Stunt::new();
        let mut hung: Option<Vec3> = None;
        let (mut rolled, mut last) = (0.0f32, st.car.left());
        let mut high = 0.0f32;
        for k in 0..(PERIOD / STEP) as u32 {
            st.run_to(k as f32 * STEP);
            let m = st.car;
            let e = Vec3::splat(0.1);
            assert!(!solid(m.strip, m.pos - e, m.pos + e, Stage(0)), "inside a wall: {st:?}");
            match st.act {
                Act::Hang => {
                    assert_eq!(m.face(), Some(Vec3::NEG_Y), "on the ceiling");
                    assert!(m.up().y < -0.99, "upside down");
                    let at = *hung.get_or_insert(m.pos);
                    assert!(m.pos.distance(at) < 0.05, "it hangs still: {:?} {at:?}", m.pos);
                }
                Act::Roll | Act::Land if !m.wheels_down() => {
                    rolled += last.angle_between(m.left());
                    high = high.max(m.pos.y);
                }
                _ => {}
            }
            last = m.left();
        }
        println!("acts begin at {:?}", st.began);
        assert_eq!(st.act, Act::Done, "{st:?}");
        assert!(st.began.windows(2).all(|w| w[0] >= 0.0 && w[1] > w[0]), "in order: {:?}", st.began);
        assert!(st.began[Act::Back as usize] - st.began[Act::Hang as usize] > HANG_SECS);
        assert!(rolled > 1.8 * PI, "a whole roll in the air: {} deg", rolled.to_degrees());
        assert!(high > 5.0, "{high}");
        // Down on its wheels on the square, out of the hall.
        let m = st.car;
        assert_eq!(m.face(), Some(Vec3::Y), "{m:?}");
        assert!((m.pos - in_hall(0.0, 0.0, m.pos.y)).dot(inward()) < -10.0, "out on the square: {m:?}");
    }

    #[test]
    fn a_moment_of_it_is_the_same_however_it_was_come_to() {
        let mut a = Stunt::new();
        a.run_to(20.0);
        let mut b = Stunt::new();
        for k in 0..=40 {
            b.run_to(k as f32 * 0.5);
        }
        assert_eq!(a.car, b.car);
        // Round the loop and back to the same moment.
        b.run_to(PERIOD + 20.0 - 1.0);
        b.run_to(20.0);
        assert_eq!(a.car, b.car);
    }
}
