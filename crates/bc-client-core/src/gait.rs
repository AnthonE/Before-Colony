//! The walk cycle a suit on a body is drawn with: where each foot is, planted or swinging.
//!
//! The server never sees feet: a suit on the ground is its origin riding over the surface
//! (`bc_sim::ground`). The gait is the client's, worked out from how the suit moves over its
//! body, so every client draws the same kind of walk without a byte on the wire.
//!
//! Everything is in the body's frame. A planted foot is held there, so it doesn't slide however
//! the body turns and drifts under the camera; each plant is put on the surface by the body's
//! own surface query ([`Shape::probe`]), so a sole meets the ground the suit stands on.
//!
//! - Steps lengthen with speed, `2.5 + 0.35·speed` m (no more than 8.8 m), so the cadence rises
//!   less than the speed does.
//! - A foot is down 62% of its cycle walking, and 38% running: a run has a moment with both feet
//!   off the ground.
//! - A swinging foot heads for where it will be under its hip halfway through its next stance,
//!   lifted 0.9 m at the top of its arc walking, 2.2 m running.
//! - Stopped, a foot left more than 1.2 m from where it would rest takes one settling step.

use bc_sim::bodies::{Body, Shape};
use glam::{Quat, Vec3};

/// Step length: `STEP_BASE + STEP_PER_SPEED · speed` m, at most `STEP_MAX`.
pub const STEP_BASE: f32 = 2.5;
pub const STEP_PER_SPEED: f32 = 0.35;
pub const STEP_MAX: f32 = 8.8;
/// How much of its cycle a foot is down, walking and running.
pub const WALK_DUTY: f32 = 0.62;
pub const RUN_DUTY: f32 = 0.38;
/// A walk becomes a run at this speed (between a walk's 8 m/s and a run's 16), m/s.
pub const RUN_FROM: f32 = 12.0;
/// How high a swinging foot is lifted at the top of its arc, walking and running, m.
pub const WALK_APEX: f32 = 0.9;
pub const RUN_APEX: f32 = 2.2;
/// Slower than this the suit is standing (m/s)...
pub const STANDING: f32 = 0.4;
/// ...and a foot further than this from its rest takes a settling step, m.
pub const SETTLE_OFF: f32 = 1.2;
/// How long a settling step takes, s.
const SETTLE_TIME: f32 = 0.35;
/// The hips are this far either side of the suit's middle, m (the rig's).
pub const HIP_SPREAD: f32 = 1.3;
/// Lifted this little, a settling step barely clears the ground, m.
const SETTLE_APEX: f32 = 0.4;

/// Step length at `speed` m/s, m.
pub fn step_length(speed: f32) -> f32 {
    (STEP_BASE + STEP_PER_SPEED * speed).clamp(STEP_BASE, STEP_MAX)
}

/// How much of its cycle a foot is down at `speed` m/s.
pub fn duty(speed: f32) -> f32 {
    if speed >= RUN_FROM { RUN_DUTY } else { WALK_DUTY }
}

/// How a suit stands on its body this frame, all in the body's frame.
#[derive(Clone, Copy, Debug)]
pub struct Stand<'a> {
    pub body: Body,
    pub shape: &'a Shape,
    /// The suit's origin and orientation.
    pub local: Vec3,
    pub rot: Quat,
    /// How fast it moves over the body, m/s.
    pub vel: Vec3,
    /// The surface's outward normal under it.
    pub up: Vec3,
    /// How high its origin rides over the surface, m.
    pub stance: f32,
}

impl Stand<'_> {
    /// Where foot `side` (0 left, 1 right) rests: on the surface under its hip.
    pub fn rest(&self, side: usize) -> Vec3 {
        let right = self.rot * Vec3::X;
        let right = (right - self.up * right.dot(self.up)).normalize_or(Vec3::X);
        let hip =
            self.local - self.up * self.stance + right * (HIP_SPREAD * if side == 0 { -1.0 } else { 1.0 });
        snap(self.shape, hip)
    }

    /// Its speed along the ground, m/s.
    fn speed(&self) -> f32 {
        (self.vel - self.up * self.vel.dot(self.up)).length()
    }
}

/// `p` put on the surface of `shape` (along its normal there).
fn snap(shape: &Shape, p: Vec3) -> Vec3 {
    let mut p = p;
    for _ in 0..2 {
        let pr = shape.probe(p);
        p -= pr.normal * pr.dist;
    }
    p
}

/// One foot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Foot {
    /// Where the sole is, in the body's frame: on the surface while planted.
    pub at: Vec3,
    pub planted: bool,
    /// A swing: from where, to where, how far through it (0..1), and how fast it goes (1/s).
    from: Vec3,
    to: Vec3,
    swing: f32,
    rate: f32,
    apex: f32,
}

/// A suit's walk cycle.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gait {
    /// The body it's walking on (a new one starts the feet over).
    pub body: Option<Body>,
    /// Where it is in a cycle (0..1): the left foot comes down at 0, the right at 0.5.
    pub phase: f32,
    /// Left, then right.
    pub feet: [Foot; 2],
    /// Feet put down so far: one each time a foot is planted.
    pub footfalls: u32,
}

impl Gait {
    /// Starts over, standing at rest as `s` stands.
    pub fn reset(&mut self, s: &Stand) {
        let at = [s.rest(0), s.rest(1)];
        *self = Self {
            body: Some(s.body),
            phase: 0.0,
            feet: at.map(|at| Foot { at, planted: true, ..Foot::default() }),
            footfalls: self.footfalls,
        };
    }

    /// Moves the walk on `dt` s, the suit standing as `s` stands.
    pub fn step(&mut self, s: &Stand, dt: f32) {
        if self.body != Some(s.body) {
            self.reset(s);
            return;
        }
        let speed = s.speed();
        if speed < STANDING {
            self.stand(s, dt);
            return;
        }
        let len = step_length(speed);
        // A cycle is two steps, one each foot.
        let cycle = speed / (2.0 * len);
        let d = duty(speed);
        let apex = if speed >= RUN_FROM { RUN_APEX } else { WALK_APEX };
        self.phase = (self.phase + cycle * dt).fract();
        for side in 0..2 {
            let p = (self.phase + 0.5 * side as f32).fract();
            let foot = &mut self.feet[side];
            if p < d {
                if !foot.planted {
                    // Down where it was heading.
                    foot.at = snap(s.shape, foot.to);
                    foot.planted = true;
                    self.footfalls += 1;
                }
            } else {
                if foot.planted || foot.rate != 0.0 {
                    // Lifted (or a settling step taken over by the walk): off to where its hip
                    // will be halfway through its next stance.
                    let stance_time = d / cycle;
                    let ahead = s.vel - s.up * s.vel.dot(s.up);
                    foot.from = foot.at;
                    foot.to = snap(s.shape, s.rest(side) + ahead * ((1.0 - d) / cycle + stance_time * 0.5));
                    foot.planted = false;
                    foot.apex = apex;
                    foot.rate = 0.0;
                }
                foot.swing = (p - d) / (1.0 - d);
                foot.at = swung(foot, s.up);
            }
        }
    }

    /// Standing: a swing under way finishes, then a foot left far from its rest steps to it.
    fn stand(&mut self, s: &Stand, dt: f32) {
        for side in 0..2 {
            let foot = &mut self.feet[side];
            if foot.planted {
                continue;
            }
            if foot.rate == 0.0 {
                // A walk's swing, cut short: it comes down where it's heading.
                foot.rate = 1.0 / SETTLE_TIME;
                foot.from = foot.at;
                foot.to = s.rest(side);
                foot.apex = SETTLE_APEX;
                foot.swing = 0.0;
            }
            foot.swing += foot.rate * dt;
            if foot.swing >= 1.0 {
                foot.at = snap(s.shape, foot.to);
                foot.planted = true;
                foot.rate = 0.0;
                self.footfalls += 1;
            } else {
                foot.at = swung(foot, s.up);
            }
        }
        if self.feet.iter().all(|f| f.planted) {
            let off = |side: usize| self.feet[side].at.distance(s.rest(side));
            let far = if off(0) >= off(1) { 0 } else { 1 };
            if off(far) > SETTLE_OFF {
                let to = s.rest(far);
                let foot = &mut self.feet[far];
                *foot = Foot {
                    from: foot.at,
                    to,
                    planted: false,
                    rate: 1.0 / SETTLE_TIME,
                    apex: SETTLE_APEX,
                    ..*foot
                };
            }
        }
    }
}

/// Where a swinging foot is: along the way, lifted in an arc.
fn swung(foot: &Foot, up: Vec3) -> Vec3 {
    let u = foot.swing.clamp(0.0, 1.0);
    let ease = u * u * (3.0 - 2.0 * u);
    foot.from.lerp(foot.to, ease) + up * (foot.apex * (core::f32::consts::PI * u).sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::bodies::{Base, Prim, landmark_pose};
    use bc_sim::content::landmarks::LANDMARKS;
    use bc_sim::ground::STANCE;

    const DT: f32 = 1.0 / 60.0;

    /// Level ground at y = 0 for kilometres.
    static FLOOR: [Prim; 1] = [Prim::RoundBox {
        c: Vec3::new(0.0, -500.0, 0.0),
        half: Vec3::new(5_000.0, 500.0, 5_000.0),
        round: 1.0,
    }];

    fn floor() -> Shape {
        Shape { base: Base::Union(&FLOOR), cuts: &[] }
    }

    /// Walking along the top of MO-II's core (its frame), at `speed` m/s along +X.
    fn on_the_core(x: f32, speed: f32) -> (Vec3, Vec3) {
        (Vec3::new(x, 60.0 + STANCE, 0.0), Vec3::X * speed)
    }

    /// Walking along level ground at `speed` m/s along +X.
    fn on_the_floor(x: f32, speed: f32) -> (Vec3, Vec3) {
        (Vec3::new(x, STANCE, 0.0), Vec3::X * speed)
    }

    fn stand<'a>(shape: &'a Shape, local: Vec3, vel: Vec3) -> Stand<'a> {
        let rot = bc_sim::math::look_rotation(Vec3::X, Vec3::Y);
        Stand { body: Body::Landmark(0), shape, local, rot, vel, up: Vec3::Y, stance: STANCE }
    }

    #[test]
    fn planted_feet_do_not_slide_on_mo_ii() {
        let shape = LANDMARKS[0].shape;
        let mut g = Gait::default();
        // Clear of the pylons (|x| < 40) and the fore module (from x = 200).
        let mut x = 50.0;
        let mut held: [Option<(Vec3, Vec3)>; 2] = [None, None];
        let mut stances = 0;
        for k in 0..900 {
            let (local, vel) = on_the_core(x, 8.0);
            g.step(&stand(&shape, local, vel), DT);
            x += 8.0 * DT;
            // MO-II as drawn this frame (it rolls and drifts under the walk).
            let t = 2_000.0 + k as f32 * DT * 30.0;
            let deck = landmark_pose(&LANDMARKS[0], t as u32, t.fract());
            for (side, foot) in g.feet.iter().enumerate() {
                if !foot.planted {
                    held[side] = None;
                    continue;
                }
                assert!(shape.probe(foot.at).dist.abs() < 0.02, "a sole off the deck");
                let world = deck.to_world(foot.at);
                match held[side] {
                    // Where it was put down, on the deck, however the deck has moved.
                    Some((at, _)) => assert_eq!(foot.at, at, "foot {side} slid at frame {k}"),
                    None => {
                        held[side] = Some((foot.at, world));
                        stances += 1;
                    }
                }
                // Within a leg's reach of its hip.
                assert!(foot.at.distance(local - Vec3::Y * STANCE) < 9.0, "foot {side} left behind");
            }
        }
        assert!(stances > 20, "{stances} steps");
    }

    #[test]
    fn stride_and_cadence_follow_speed() {
        let shape = floor();
        let mut cadence = Vec::new();
        for speed in [4.0f32, 8.0, 16.0] {
            let mut g = Gait::default();
            let (mut x, secs) = (0.0, 30.0);
            let mut plants: Vec<f32> = Vec::new();
            let (mut both_up, mut frames) = (0, 0);
            let mut was = g.footfalls;
            for _ in 0..(secs / DT) as usize {
                let (local, vel) = on_the_floor(x, speed);
                g.step(&stand(&shape, local, vel), DT);
                x += speed * DT;
                if g.footfalls > was && g.feet[0].planted && plants.len() < 1_000 {
                    plants.push(g.feet[0].at.x);
                }
                was = g.footfalls;
                both_up += usize::from(!g.feet[0].planted && !g.feet[1].planted);
                frames += 1;
            }
            let rate = g.footfalls as f32 / secs;
            let want = speed / step_length(speed);
            assert!((rate - want).abs() < want * 0.1, "{speed} m/s: {rate} steps/s, want {want}");
            // The same foot's plants are two steps apart.
            let left: Vec<f32> = plants.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.5).collect();
            let stride = left.iter().sum::<f32>() / left.len() as f32;
            assert!(
                (stride - 2.0 * step_length(speed)).abs() < 0.15 * 2.0 * step_length(speed),
                "{speed}: {stride} m"
            );
            // Running has a moment with both feet up; walking never does.
            if speed >= RUN_FROM {
                assert!(both_up * 10 > frames, "{speed} m/s: both feet up {both_up} of {frames} frames");
            } else {
                assert_eq!(both_up, 0, "{speed} m/s");
            }
            cadence.push(rate);
        }
        assert!(cadence.windows(2).all(|w| w[1] > w[0]), "{cadence:?}");
        // Longer steps at speed, as far as the legs reach.
        assert_eq!((step_length(0.0), step_length(8.0), step_length(30.0)), (2.5, 5.3, 8.8));
    }

    #[test]
    fn feet_settle_when_stopping() {
        let shape = floor();
        let mut g = Gait::default();
        let mut x = 0.0;
        for _ in 0..(2.3 / DT) as usize {
            let (local, vel) = on_the_floor(x, 8.0);
            g.step(&stand(&shape, local, vel), DT);
            x += 8.0 * DT;
        }
        let (local, _) = on_the_floor(x, 0.0);
        let s = stand(&shape, local, Vec3::ZERO);
        for _ in 0..(1.5 / DT) as usize {
            g.step(&s, DT);
        }
        for side in 0..2 {
            assert!(g.feet[side].planted, "foot {side} still in the air");
            let off = g.feet[side].at.distance(s.rest(side));
            assert!(off <= SETTLE_OFF, "foot {side} {off} m from its rest");
        }
        // Settled: no more steps.
        let steps = g.footfalls;
        for _ in 0..120 {
            g.step(&s, DT);
        }
        assert_eq!(g.footfalls, steps);
    }
}
