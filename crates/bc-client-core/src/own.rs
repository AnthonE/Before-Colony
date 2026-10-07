//! The own suit as drawn: between the ticks the prediction has flown, a tick behind the input
//! clock as it eases along, with corrections blended in rather than snapped.
//!
//! When a snapshot puts the prediction right, the suit's drawn place and motion are kept and the
//! difference is blended out on a critically damped spring, so a correction bends the suit's
//! path without ever jerking it. Only a new life, or a relocation too big to be a misprediction,
//! cuts.
//!
//! On a body the suit is drawn on the body as the body is drawn (on the view clock), so its feet
//! stay on the deck the pilot sees. A correction still being blended out turns with the body, as
//! though held in its frame. Landing on a body, or leaving one, changes how the suit is drawn by
//! the body's motion between the two clocks: that is blended out too, never cut.

use bc_proto::FrameId;
use bc_sim::bodies::{Body, BodyPose};
use glam::{Quat, Vec3};

use crate::chase::spring;
use crate::interp::GroundPose;
use crate::predict::{OwnPose, RELOCATION};

/// How fast a correction is blended out (rad/s): about 0.3 s to a tenth of it.
const MEND: f32 = 12.0;
/// The G shown settles over about this long (s).
const G_SMOOTH: f32 = 0.15;
/// Corrections measured longer than this after the last frame (s) aren't blended but cut to: the
/// page wasn't drawing (a background tab), so there's no motion on screen to keep.
const STALE: f64 = 0.25;

/// When a frame is drawn: the input clock's time the own suit is drawn at, the view clock's the
/// bodies (and everyone else) are drawn at, both in ticks, and the local time, s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    pub own: f64,
    pub view: f64,
    pub now: f64,
}

/// The own suit as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnView {
    /// The input clock's time drawn (ticks): a tick behind the newest command, as eased.
    pub t: f64,
    /// The view clock's time the bodies are drawn at (ticks): a suit on one is drawn on it then.
    pub t_view: f64,
    pub pos: Vec3,
    pub rot: Quat,
    /// How fast the drawn suit moves, m/s: its flight, the clock's easing and any correction
    /// being blended out. What a camera following it should keep pace with.
    pub vel: Vec3,
    /// The suit's velocity through space (the flight model's), m/s.
    pub flight_vel: Vec3,
    /// Felt acceleration, g, smoothed for reading.
    pub g: f32,
    pub g_strain: f32,
    pub blackout: bool,
    pub boosting: bool,
    /// Thrust applied per local axis, as a fraction of each axis' unboosted maximum.
    pub throttle: Vec3,
    /// Flight assist is holding the pilot's G down.
    pub g_limited: bool,
    /// The ion drive is working.
    pub ion: bool,
    /// The form drawn.
    pub frame: FrameId,
    /// The mount of a strike in its windup or stroke, as predicted: the swing starts with the
    /// lunge.
    pub strike: Option<u8>,
    /// On a body: which, and how it stands on it.
    pub ground: Option<GroundPose>,
    /// It landed over the tick drawn, this fast into the surface, m/s.
    pub touchdown: Option<f32>,
    pub alive: bool,
    /// The drawn suit jumped (spawn, respawn, a relocation): a camera should cut, not chase.
    pub cut: bool,
}

/// What carries over between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct Drawn {
    /// Position correction still to blend out, and its rate.
    err: Vec3,
    err_v: Vec3,
    /// Rotation correction still to blend out (scaled axis), and its rate.
    turn: Vec3,
    turn_v: Vec3,
    g: f32,
    /// When the suit was last drawn: the input-clock and view-clock times drawn, and the local
    /// time (s).
    pub last: Option<Moment>,
    /// The body the suit was last drawn on (and so how it was drawn), if it was on one.
    pub last_frame: Option<Body>,
    /// That body's orientation as last drawn: the corrections turn with it.
    deck: Option<(Body, Quat)>,
    cut: bool,
    pub view: Option<OwnView>,
}

impl Drawn {
    /// The input-clock and view-clock times last drawn, if recent enough at `now` to keep what's
    /// on screen.
    pub fn recent(&self, now: f64) -> Option<(f64, f64)> {
        self.last.filter(|m| now - m.now <= STALE).map(|m| (m.own, m.view))
    }

    /// What the suit is drawn from moved under it (`before` → `after`, both at the time last
    /// drawn): keep the drawn suit where it was and as it moved, and blend the difference out.
    /// `after` is how the suit is drawn from now on (on a body, or free).
    pub fn correct(&mut self, before: Option<OwnPose>, after: Option<OwnPose>) {
        match (before, after) {
            (Some(b), Some(a)) if b.pos.distance(a.pos) <= RELOCATION => {
                self.err += b.pos - a.pos;
                self.err_v += b.dpos - a.dpos;
                let q = Quat::from_scaled_axis(self.turn) * b.rot * a.rot.inverse();
                self.turn = if q.w < 0.0 { -q } else { q }.to_scaled_axis();
            }
            (_, Some(_)) => self.cut(),
            _ => {}
        }
        if let Some(a) = after {
            self.last_frame = a.on();
        }
    }

    /// The suit is drawn by another rule from now on: it landed on a body, left one, or went from
    /// one to another between frames. Free, it's drawn where it is at the input clock's time; on a
    /// body, its place on the body then, on the body as drawn at the view clock's. The change, the
    /// body's motion between the two clocks, is a correction: blended out, never cut. `pose` says
    /// where a body is at a time (ticks).
    pub fn reframe(&mut self, src: &OwnPose, at: Moment, pose: &dyn Fn(Body, f64) -> Option<BodyPose>) {
        if self.last.is_none() || self.cut || src.on() == self.last_frame {
            // (Nothing drawn yet, or about to cut to it anyway.)
            return;
        }
        // Where the suit truly is at the input clock's time (off the body it's drawn on)...
        let mut truth = *src;
        if let Some(b) = src.on()
            && let (Some(drawn), Some(then)) = (pose(b, at.view), pose(b, at.own))
        {
            truth = src.moved_with(&drawn, &then);
        }
        // ...drawn as it was until now: on the body it was on, as that is drawn.
        let mut before = truth;
        if let Some(b) = self.last_frame
            && let (Some(then), Some(drawn)) = (pose(b, at.own), pose(b, at.view))
        {
            before = truth.moved_with(&then, &drawn);
        }
        self.correct(Some(before), Some(*src));
    }

    /// Starts over from where the suit is: nothing to blend, and a camera cuts.
    pub fn cut(&mut self) {
        (self.err, self.err_v, self.turn, self.turn_v) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        self.cut = true;
    }

    /// The suit as drawn at moment `at` from `src`, `dt` s after the last frame; `rate`: how fast
    /// the clock it's drawn on runs against real time; `deck`: the orientation of the body `src`
    /// is on, as drawn.
    pub fn draw(
        &mut self,
        at: Moment,
        src: &OwnPose,
        alive: bool,
        dt: f32,
        rate: f32,
        deck: Option<Quat>,
    ) -> OwnView {
        // Still on the same body: what's left to blend out turns with it.
        if let (Some(rot), Some((body, was))) = (deck, self.deck)
            && src.on() == Some(body)
        {
            let turn = rot * was.conjugate();
            (self.err, self.err_v) = (turn * self.err, turn * self.err_v);
            (self.turn, self.turn_v) = (turn * self.turn, turn * self.turn_v);
        }
        self.deck = src.on().zip(deck);
        self.last_frame = src.on();
        spring(&mut self.err, &mut self.err_v, Vec3::ZERO, MEND, dt);
        spring(&mut self.turn, &mut self.turn_v, Vec3::ZERO, MEND, dt);
        let cut = std::mem::take(&mut self.cut);
        self.g =
            if cut { src.g_load } else { self.g + (src.g_load - self.g) * (1.0 - (-dt / G_SMOOTH).exp()) };
        let view = OwnView {
            t: at.own,
            t_view: at.view,
            pos: src.pos + self.err,
            rot: (Quat::from_scaled_axis(self.turn) * src.rot).normalize(),
            vel: src.dpos * rate + self.err_v,
            flight_vel: src.vel,
            g: self.g,
            g_strain: src.g_strain,
            blackout: src.blackout,
            boosting: src.boosting,
            throttle: src.throttle,
            g_limited: src.g_limited,
            ion: src.ion,
            frame: src.frame,
            strike: src.strike,
            ground: src.ground,
            touchdown: src.touchdown,
            alive,
            cut,
        };
        self.last = Some(at);
        self.view = Some(view);
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(pos: Vec3, dpos: Vec3) -> OwnPose {
        OwnPose {
            pos,
            rot: Quat::IDENTITY,
            vel: dpos,
            dpos,
            g_load: 0.0,
            g_strain: 0.0,
            blackout: false,
            boosting: false,
            throttle: Vec3::ZERO,
            g_limited: false,
            ion: false,
            frame: FrameId::Leo,
            strike: None,
            ground: None,
            touchdown: None,
        }
    }

    fn at(t: f32) -> Moment {
        Moment { own: f64::from(t) * 30.0, view: f64::from(t) * 30.0 - 6.0, now: f64::from(t) }
    }

    #[test]
    fn a_correction_bends_the_path_without_a_jerk() {
        // Flying at 300 m/s, drawn at 60 Hz, when the server's news moves the source 3 m back and
        // 20 m/s slower.
        let dt = 1.0 / 60.0;
        let v = Vec3::new(0.0, 0.0, 300.0);
        let old = |t: f32| pose(v * t, v);
        let new = |t: f32| pose(v * t - Vec3::Z * (3.0 + 20.0 * (t - 1.0)), v - Vec3::Z * 20.0);
        let mut d = Drawn::default();
        d.cut();
        let mut t = 0.0f32;
        while t < 1.0 {
            t += dt;
            d.draw(at(t), &old(t), true, dt, 1.0, None);
        }
        // At the moment of the news, nothing drawn moves: not where the suit is, nor how it moves.
        let before = d.view.unwrap();
        d.correct(Some(old(t)), Some(new(t)));
        let after = d.draw(at(t), &new(t), true, 0.0, 1.0, None);
        assert!(after.pos.distance(before.pos) < 1e-3, "jumped {:?}", after.pos - before.pos);
        assert!(after.vel.distance(before.vel) < 1e-3, "lurched {:?}", after.vel - before.vel);
        // Then it bends onto the new path, never faster than the suit flies.
        let mut prev = after;
        let t0 = t;
        while t < t0 + 0.6 {
            t += dt;
            let now = d.draw(at(t), &new(t), true, dt, 1.0, None);
            assert!(
                now.pos.distance(prev.pos) <= v.length() * dt + 1e-3,
                "a step of {:?}",
                now.pos - prev.pos
            );
            prev = now;
        }
        let off = prev.pos.distance(new(t).pos);
        assert!(off < 0.03, "{off} m still to go after 0.6 s");
    }

    #[test]
    fn a_frame_switch_is_a_correction_not_a_cut() {
        use bc_sim::bodies::landmark_pose;
        use bc_sim::content::landmarks::LANDMARKS;

        // Standing still on MO-II's aft module, 8 ticks between the clocks, drawn at 60 Hz: drawn
        // as flying free (where it is at the input clock's time) until it's caught, then on the
        // deck as drawn at the view clock's.
        let mo_ii = |t: f64| landmark_pose(&LANDMARKS[0], t.floor() as u32, t.fract() as f32);
        let body = Body::Landmark(0);
        let local = Vec3::new(-230.0, 0.0, 80.0 + bc_sim::ground::STANCE);
        let world = |t: f64| {
            let p = mo_ii(t);
            let pos = p.to_world(local);
            OwnPose { rot: p.rot, ..pose(pos, p.point_vel(pos)) }
        };
        let on_deck = |own: f64, view: f64| {
            let p = mo_ii(view);
            let ground = GroundPose { body, aloft: false, up: Vec3::Y, rel_vel: Vec3::ZERO, height: 9.125 };
            OwnPose { ground: Some(ground), ..world(own).moved_with(&mo_ii(own), &p) }
        };
        let gap = 8.0;
        let frame_dt = 1.0 / 60.0;
        let mut d = Drawn::default();
        d.cut();
        let mut prev: Option<OwnView> = None;
        let (mut worst_step, mut jump) = (0.0f32, 0.0f32);
        for k in 0..60 {
            let now = f64::from(k) * f64::from(frame_dt);
            let at = Moment { own: 9_000.0 + now * 30.0, view: 9_000.0 + now * 30.0 - gap, now };
            let caught = k >= 20;
            let src = if caught { on_deck(at.own, at.view) } else { world(at.own) };
            d.reframe(&src, at, &|_, t| Some(mo_ii(t)));
            let deck = caught.then(|| mo_ii(at.view).rot);
            let v = d.draw(at, &src, true, frame_dt, 1.0, deck);
            if let Some(p) = prev {
                assert!(!v.cut, "cut at frame {k}");
                worst_step = worst_step.max(v.pos.distance(p.pos));
            }
            if k == 20 {
                jump = src.pos.distance(world(at.own).pos);
            }
            if k == 20 + 21 {
                // 0.35 s on: a tenth of the jump is left, held on the deck.
                let left = v.pos.distance(src.pos);
                assert!(left <= jump * 0.1, "{left} m of {jump} m left");
            }
            prev = Some(v);
        }
        // The body's motion over 8 ticks (up to 2.87 m/s on MO-II), blended over many frames.
        assert!(jump > 0.2 && jump < 2.87 * 8.0 / 30.0 + 0.01, "the switch moved it {jump} m");
        assert!(worst_step < 0.1, "a step of {worst_step} m");
    }

    #[test]
    fn a_relocation_cuts() {
        let mut d = Drawn::default();
        d.draw(at(0.0), &pose(Vec3::ZERO, Vec3::ZERO), true, 0.016, 1.0, None);
        d.correct(Some(pose(Vec3::ZERO, Vec3::ZERO)), Some(pose(Vec3::X * 500.0, Vec3::ZERO)));
        let v = d.draw(at(0.016), &pose(Vec3::X * 500.0, Vec3::ZERO), true, 0.016, 1.0, None);
        assert!(v.cut);
        assert_eq!(v.pos, Vec3::X * 500.0);
        let v = d.draw(at(0.032), &pose(Vec3::X * 500.0, Vec3::ZERO), true, 0.016, 1.0, None);
        assert!(!v.cut, "a cut lasts a frame");
    }
}
