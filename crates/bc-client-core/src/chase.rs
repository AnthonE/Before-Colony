//! The chase camera's rig, kept free of the renderer so it can be tested natively.
//!
//! A critically damped spring holds the camera behind and above the suit, in the frame moving with
//! the suit as drawn: at any steady speed it sits still, so the suit does too; it trails by
//! acceleration / ω² (about 5 m under 10 g) and swings with turns of the aim, which move its place
//! round the suit. The step is exact, so the camera moves the same at any frame rate.
//!
//! On a body the camera comes in closer and higher, to see over rims, and it is kept out of the
//! bodies: the client clamps it short of any surface between the suit and its place, which it
//! reaches over the suit's head ([`ChaseRig::step_clamped`], [`reach`], with
//! `surface::camera_clamp`), so a wall at the suit's back never puts the camera inside the suit.

use glam::Vec3;

/// The chase spring (rad/s).
pub const OMEGA: f32 = 4.5;
/// The furthest the spring lets the camera stray from its place behind the suit (m).
pub const SLACK: f32 = 25.0;
/// Where the camera sits: behind the suit along the aim, and above it (m).
pub const BACK: f32 = 42.0;
pub const RISE: f32 = 10.0;
/// ...and on a body (m): closer and higher, to see over a crater's rim.
pub const GROUND_BACK: f32 = 36.0;
pub const GROUND_RISE: f32 = 14.0;
/// How fast the camera moves between the two (1/s).
const GROUND_EASE: f32 = 3.0;
/// How far ahead along the aim the camera looks (m).
pub const LOOK: f32 = 800.0;
/// A camera this far from its place has lost the suit (a respawn, a teleport): it cuts.
const CUT: f32 = 300.0;

/// A critically damped spring's exact step toward `target` (stable at any frame time).
pub fn spring(x: &mut Vec3, v: &mut Vec3, target: Vec3, w: f32, dt: f32) {
    let y = *x - target;
    let j = *v + y * w;
    let e = (-w * dt).exp();
    *x = target + (y + j * dt) * e;
    *v = (*v - j * (w * dt)) * e;
}

/// What the camera follows this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Follow {
    /// The suit as drawn.
    pub pos: Vec3,
    /// How fast the drawn suit moves (not just its flight velocity: whatever moves it on screen).
    pub vel: Vec3,
    pub aim: Vec3,
    pub up: Vec3,
    /// The view jumped (spawn, respawn, a relocation): cut rather than chase.
    pub cut: bool,
    /// The suit is on a body (standing on it, or in the air in its grip).
    pub ground: bool,
}

impl Follow {
    /// The camera's place: behind the suit along the aim, and above it.
    pub fn ideal(&self) -> Vec3 {
        self.ideal_at(0.0)
    }

    /// The camera's place `mix` (0..1) of the way from where it sits in flight to where it sits on
    /// a body.
    fn ideal_at(&self, mix: f32) -> Vec3 {
        let back = BACK + (GROUND_BACK - BACK) * mix;
        let rise = RISE + (GROUND_RISE - RISE) * mix;
        self.pos - self.aim * back + self.up * rise
    }

    /// Where the camera looks.
    pub fn look_at(&self) -> Vec3 {
        self.pos + self.aim * LOOK
    }
}

/// The camera's own state between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChaseRig {
    pub placed: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    /// How far (0..1) it has come in to where it sits on a body.
    pub ground: f32,
}

impl ChaseRig {
    /// Moves the camera `dt` seconds on; returns whether it cut to its place.
    pub fn step(&mut self, f: &Follow, dt: f32) -> bool {
        self.step_clamped(f, dt, &|_, to| to)
    }

    /// [`ChaseRig::step`], with the camera's place on the way from the suit to it passed through
    /// `clamp(from, to)`, which may stop it short (of a body's surface): see [`reach`].
    pub fn step_clamped(&mut self, f: &Follow, dt: f32, clamp: &dyn Fn(Vec3, Vec3) -> Vec3) -> bool {
        let goal = if f.ground { 1.0 } else { 0.0 };
        let cut = !self.placed || f.cut;
        self.ground =
            if cut { goal } else { self.ground + (goal - self.ground) * (1.0 - (-GROUND_EASE * dt).exp()) };
        let ideal = f.ideal_at(self.ground);
        if cut || (self.pos + self.vel * dt).distance(ideal) > CUT {
            *self = Self { placed: true, pos: reach(f, ideal, clamp), vel: f.vel, ground: self.ground };
            return true;
        }
        // In the frame moving with the drawn suit: the camera starts where that frame carries it,
        // with what it has of its own velocity, and springs toward its place.
        let mut x = self.pos + f.vel * dt;
        let mut u = self.vel - f.vel;
        spring(&mut x, &mut u, ideal, OMEGA, dt);
        let off = x - ideal;
        let len = off.length();
        if len > SLACK {
            // At the end of its tether: held there, and not still moving away.
            let n = off / len;
            x = ideal + n * SLACK;
            u -= n * u.dot(n).max(0.0);
        }
        self.pos = reach(f, x, clamp);
        self.vel = u + f.vel;
        false
    }
}

/// Where the camera gets to on its way to `x` from the suit, `clamp` stopping it short of a body:
/// up over the suit's head first (as high as `x` is over it), then across to it. Straight from the
/// suit, a surface just behind it would stop the camera inside the suit itself; this way it stops
/// over its head, looking past it. With nothing in the way, it's `x`.
pub fn reach(f: &Follow, x: Vec3, clamp: &dyn Fn(Vec3, Vec3) -> Vec3) -> Vec3 {
    let over = f.pos + f.up * (x - f.pos).dot(f.up).max(0.0);
    let up = clamp(f.pos, over);
    clamp(up, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIM: Vec3 = Vec3::Z;
    const UP: Vec3 = Vec3::Y;

    /// The rig before this one: semi-implicit steps against the flight velocity, clamped without
    /// touching its velocity. Kept to check the new one keeps its swing.
    fn old_step(rig: &mut ChaseRig, f: &Follow, dt: f32) {
        let ideal = f.ideal();
        let coast = rig.pos + rig.vel * dt;
        let x = coast - ideal;
        let a = -OMEGA * OMEGA * x - 2.0 * OMEGA * (rig.vel - f.vel);
        rig.vel += a * dt;
        rig.pos = ideal + (x + a * dt * dt).clamp_length_max(SLACK);
    }

    fn follow(pos: Vec3, vel: Vec3) -> Follow {
        Follow { pos, vel, aim: AIM, up: UP, cut: false, ground: false }
    }

    /// Flies the rig behind a suit drawn at `path(t)` at `hz` for `secs`, told how far the suit
    /// moved over each frame (as the client does), and returns the camera's offset from its place
    /// at the end.
    fn fly(hz: f32, secs: f32, path: impl Fn(f32) -> Vec3) -> Vec3 {
        let dt = 1.0 / hz;
        let mut rig = ChaseRig::default();
        let mut t = 0.0;
        let mut f = follow(path(0.0), Vec3::ZERO);
        rig.step(&f, dt);
        while t < secs {
            t += dt;
            f = follow(path(t), (path(t) - path(t - dt)) / dt);
            rig.step(&f, dt);
        }
        rig.pos - f.ideal()
    }

    #[test]
    fn no_standing_offset_when_the_drawn_suit_runs_fast() {
        // The drawn suit runs 5 % fast (its clock easing onto the server's) at 300 m/s: the rig
        // follows how it moves, so it sits exactly in place.
        let v = Vec3::new(0.0, 0.0, 300.0 * 1.05);
        let off = fly(60.0, 5.0, |t| v * t);
        assert!(off.length() < 0.01, "offset {off:?}");
    }

    #[test]
    fn trails_by_acceleration_over_omega_squared() {
        let a = Vec3::new(0.0, 0.0, 10.0 * 9.806_65);
        let off = fly(60.0, 4.0, |t| a * (0.5 * t * t));
        let want = a.length() / (OMEGA * OMEGA);
        assert!((off.z + want).abs() < want * 0.05, "offset {off:?}, want {want} behind");
    }

    #[test]
    fn the_same_at_any_frame_rate() {
        // A 30° turn of the aim at rest (inside the slack): the camera swings round, the same at
        // 30, 60 and 144 Hz.
        let swing = |hz: f32| {
            let dt = 1.0 / hz;
            let mut rig = ChaseRig::default();
            rig.step(&follow(Vec3::ZERO, Vec3::ZERO), dt);
            let yaw = std::f32::consts::FRAC_PI_6;
            let turned =
                Follow { aim: Vec3::new(yaw.sin(), 0.0, yaw.cos()), ..follow(Vec3::ZERO, Vec3::ZERO) };
            let mut t = 0.0;
            let mut path = Vec::new();
            while t < 1.0 - 1e-4 {
                t += dt;
                rig.step(&turned, dt);
                path.push((t, rig.pos));
            }
            path
        };
        let reference = swing(144.0);
        let at = |path: &[(f32, Vec3)], t: f32| {
            path.iter().min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs())).map(|p| p.1).unwrap()
        };
        for hz in [30.0, 60.0] {
            let path = swing(hz);
            for t in [1.0 / 3.0, 2.0 / 3.0, 1.0] {
                let d = at(&path, t).distance(at(&reference, t));
                assert!(d < 0.05, "{hz} Hz at {t:.2} s is {d:.3} m off 144 Hz");
            }
        }
    }

    #[test]
    fn keeps_the_old_swing_on_turns() {
        // How far the camera strays from its place when the aim swings 30° round at 0.5 rad/s.
        let peak = |step: &dyn Fn(&mut ChaseRig, &Follow, f32)| {
            let dt = 1.0 / 60.0;
            let mut rig = ChaseRig::default();
            rig.step(&follow(Vec3::ZERO, Vec3::ZERO), dt);
            let mut worst = 0.0f32;
            for k in 1..=120 {
                let yaw = (k as f32 * dt * 0.5).min(std::f32::consts::FRAC_PI_6);
                let f =
                    Follow { aim: Vec3::new(yaw.sin(), 0.0, yaw.cos()), ..follow(Vec3::ZERO, Vec3::ZERO) };
                step(&mut rig, &f, dt);
                worst = worst.max(rig.pos.distance(f.ideal()));
            }
            worst
        };
        let new = peak(&|r, f, dt| {
            r.step(f, dt);
        });
        let old = peak(&|r, f, dt| old_step(r, f, dt));
        assert!((new - old).abs() < old * 0.1, "swing {new:.2} m, was {old:.2} m");
    }

    #[test]
    fn a_sudden_stop_holds_at_the_tether_without_surging_on() {
        // A suit at 300 m/s stops dead against a rock: the camera, still moving, runs up against
        // its slack and is held there, with nothing left carrying it further away.
        let dt = 1.0 / 60.0;
        let mut rig = ChaseRig::default();
        let v = Vec3::new(0.0, 0.0, 300.0);
        rig.step(&follow(Vec3::ZERO, v), dt);
        for k in 1..60 {
            rig.step(&follow(v * (k as f32 * dt), v), dt);
        }
        let stopped = follow(v * 1.0, Vec3::ZERO);
        let mut worst = 0.0f32;
        for _ in 0..120 {
            assert!(!rig.step(&stopped, dt), "no cut");
            let off = rig.pos - stopped.ideal();
            worst = worst.max(off.length());
            let n = off.normalize_or_zero();
            assert!(
                off.length() < SLACK - 1e-3 || rig.vel.dot(n) <= 1e-3,
                "still moving away: {:?}",
                rig.vel
            );
        }
        assert!(worst <= SLACK + 1e-3, "strayed {worst} m");
        assert!(rig.pos.distance(stopped.ideal()) < 0.5, "settles back in place");
    }

    #[test]
    fn cuts_when_asked_or_lost() {
        let mut rig = ChaseRig::default();
        assert!(rig.step(&follow(Vec3::ZERO, Vec3::ZERO), 0.016), "first placement");
        assert!(!rig.step(&follow(Vec3::ZERO, Vec3::ZERO), 0.016));
        assert!(rig.step(&Follow { cut: true, ..follow(Vec3::ZERO, Vec3::ZERO) }, 0.016));
        assert!(rig.step(&follow(Vec3::splat(5_000.0), Vec3::ZERO), 0.016), "a teleport");
        assert_eq!(rig.pos, follow(Vec3::splat(5_000.0), Vec3::ZERO).ideal());
    }

    #[test]
    fn grounded_offsets_ease() {
        // Landing: the camera comes in to 36 m back and 14 m up over a third of a second or so,
        // and goes back out on taking off.
        let dt = 1.0 / 60.0;
        let mut rig = ChaseRig::default();
        let flying = follow(Vec3::ZERO, Vec3::ZERO);
        rig.step(&flying, dt);
        assert_eq!(rig.pos, flying.ideal());
        let landed = Follow { ground: true, ..flying };
        let place = |rig: &ChaseRig| (-(rig.pos - landed.pos).dot(AIM), (rig.pos - landed.pos).dot(UP));
        let mut prev = place(&rig);
        for k in 0..120 {
            rig.step(&landed, dt);
            let (back, rise) = place(&rig);
            assert!(back <= prev.0 + 1e-3 && rise >= prev.1 - 1e-3, "frame {k}: eases one way");
            prev = (back, rise);
            if k == 5 {
                assert!(back > 39.0, "no jump: {back} m back after 6 frames");
            }
        }
        assert!((prev.0 - GROUND_BACK).abs() < 0.2 && (prev.1 - GROUND_RISE).abs() < 0.2, "{prev:?}");
        for _ in 0..180 {
            rig.step(&flying, dt);
        }
        assert!(rig.pos.distance(flying.ideal()) < 0.05);
        // A cut goes straight there.
        rig.step(&Follow { cut: true, ..landed }, dt);
        assert!(rig.pos.distance(landed.pos - AIM * GROUND_BACK + UP * GROUND_RISE) < 1e-3);
    }

    #[test]
    fn the_camera_stays_outside_the_body() {
        use crate::surface::{BodySet, CAM_CLEAR, camera_clamp};
        use bc_sim::bodies::Body;
        use bc_sim::field::Field;
        use bc_sim::ground::STANCE;
        use std::sync::Arc;

        // Standing on Hermit, aiming down at the ground: the camera's place behind and above the
        // aim is under the surface, and it's held out of it.
        let bodies = BodySet::new(Arc::new(Field::empty()), 2);
        let hermit = Body::Landmark(1);
        let t = 100.0;
        let pose = bodies.pose_at(hermit, t).unwrap();
        let shape = bodies.shape(hermit).unwrap();
        let (p, n) = bodies.at(100).surface_along(hermit, Vec3::new(0.4, 1.0, -0.3)).unwrap();
        let (p, n) = (pose.to_world(p), pose.rot * n);
        let side = n.cross(Vec3::X).normalize();
        let clear = |x: Vec3| shape.probe(pose.to_local(x)).dist;
        let dt = 1.0 / 60.0;
        let mut clamped = 0;
        for pitch in [-1.2f32, -0.6, 0.0, 0.8, 1.2] {
            // Aimed up (positive), the camera goes down behind the suit.
            let aim = (side * pitch.cos() + n * pitch.sin()).normalize();
            let f = Follow { pos: p + n * STANCE, vel: Vec3::ZERO, aim, up: n, cut: false, ground: true };
            clamped += u32::from(clear(f.ideal_at(1.0)) < 0.0);
            let mut rig = ChaseRig::default();
            for _ in 0..240 {
                rig.step_clamped(&f, dt, &|from, to| camera_clamp(&bodies, t, from, to));
                assert!(clear(rig.pos) > CAM_CLEAR * 0.9, "pitch {pitch}: {} m clear", clear(rig.pos));
            }
            let open = clear(f.ideal_at(1.0)) > CAM_CLEAR;
            if open {
                assert!(rig.pos.distance(f.ideal_at(1.0)) < 0.05, "pitch {pitch}: held short for nothing");
            }
        }
        // Places under the ground came up: the clamp had work to do.
        assert_eq!(clamped, 2);
    }

    #[test]
    fn a_wall_behind_the_suit_puts_the_camera_over_its_head_not_in_it() {
        use crate::surface::{BodySet, CAM_CLEAR, camera_clamp};
        use bc_sim::bodies::Body;
        use bc_sim::field::Field;
        use bc_sim::ground::BODY_CLEAR;
        use std::sync::Arc;

        // Its back to MO-II's fore module, as near as a suit on it can be (BODY_CLEAR), and facing
        // away: the camera's place is inside the module, and straight back from the suit it would
        // stop a metre behind its origin, in its torso.
        let bodies = BodySet::new(Arc::new(Field::empty()), 1);
        let mo_ii = Body::Landmark(0);
        let t = 100.0;
        let pose = bodies.pose_at(mo_ii, t).unwrap();
        let shape = bodies.shape(mo_ii).unwrap();
        let clear = |x: Vec3| shape.probe(pose.to_local(x)).dist;
        let (aim, up) = (pose.rot * Vec3::X, pose.rot * Vec3::Y);
        let pos = pose.to_world(Vec3::new(260.0 + BODY_CLEAR, 40.0, 40.0));
        let f = Follow { pos, vel: Vec3::ZERO, aim, up, cut: false, ground: true };
        let clamp = |from, to| camera_clamp(&bodies, t, from, to);
        assert!(clamp(pos, f.ideal_at(1.0)).distance(pos) < 2.0, "straight back, it was in the suit");
        let mut rig = ChaseRig::default();
        for _ in 0..240 {
            rig.step_clamped(&f, 1.0 / 60.0, &clamp);
            assert!(clear(rig.pos) > CAM_CLEAR * 0.9, "{} m clear", clear(rig.pos));
            assert!(
                (rig.pos - pos).dot(up) > GROUND_RISE - 0.5,
                "{} m over the suit",
                (rig.pos - pos).dot(up)
            );
        }
    }
}
