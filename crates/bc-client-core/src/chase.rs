//! The chase camera's rig, kept free of the renderer so it can be tested natively.
//!
//! A critically damped spring holds the camera behind and above the suit, in the frame moving with
//! the suit as drawn: at any steady speed it sits still, so the suit does too; it trails by
//! acceleration / ω² (about 5 m under 10 g) and swings with turns of the aim, which move its place
//! round the suit. The step is exact, so the camera moves the same at any frame rate.

use glam::Vec3;

/// The chase spring (rad/s).
pub const OMEGA: f32 = 4.5;
/// The furthest the spring lets the camera stray from its place behind the suit (m).
pub const SLACK: f32 = 25.0;
/// Where the camera sits: behind the suit along the aim, and above it (m).
pub const BACK: f32 = 42.0;
pub const RISE: f32 = 10.0;
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
}

impl Follow {
    /// The camera's place: behind the suit along the aim, and above it.
    pub fn ideal(&self) -> Vec3 {
        self.pos - self.aim * BACK + self.up * RISE
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
}

impl ChaseRig {
    /// Moves the camera `dt` seconds on; returns whether it cut to its place.
    pub fn step(&mut self, f: &Follow, dt: f32) -> bool {
        let ideal = f.ideal();
        if !self.placed || f.cut || (self.pos + self.vel * dt).distance(ideal) > CUT {
            *self = Self { placed: true, pos: ideal, vel: f.vel };
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
        self.pos = x;
        self.vel = u + f.vel;
        false
    }
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
        Follow { pos, vel, aim: AIM, up: UP, cut: false }
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
}
