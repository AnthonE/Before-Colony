//! The sector's bodies as a client knows them, and what the pilot's view needs of them: where each
//! is at any moment, the camera kept out of them, the landing ring, and how the HUD reads them.
//!
//! A body's pose never travels on the wire. Rocks come from the Welcome's seed and never move;
//! landmarks are compiled content whose pose is a closed form in the tick
//! ([`bc_sim::bodies::landmark_pose`]); the colony's city stands still in an interior sector. So a
//! client works out any body's pose at any (fractional) tick exactly as the server does, and draws
//! every body on the view clock.

use std::sync::Arc;

use bc_proto::BodyRef;
use bc_sim::bodies::{Bodies, Body, BodyPose, MAX_LANDMARKS, Near, Shape, landmark_pose, sweep_landmarks};
use bc_sim::content::landmarks::{LANDMARKS, LandmarkDef};
use bc_sim::field::Field;
use bc_sim::flight::FlightState;
use bc_sim::ground::{CATCH_LEAVE, CATCH_RANGE, CATCH_SPEED, Footing, LEVEL_RANGE, LEVEL_SPEED, STANCE};
use glam::{Quat, Vec3};

/// The sector's bodies: its field of rocks (from the Welcome's seed) and the landmarks it has; in
/// the colony's inside, its city.
#[derive(Clone, Debug)]
pub struct BodySet {
    pub field: Arc<Field>,
    landmarks: u8,
    /// The sector is the colony's inside (the Welcome's INTERIOR): its city is a body.
    interior: bool,
}

impl Default for BodySet {
    /// No rocks, and every landmark this build knows (until a Welcome says otherwise).
    fn default() -> Self {
        Self {
            field: Arc::new(Field::empty()),
            landmarks: LANDMARKS.len().min(MAX_LANDMARKS) as u8,
            interior: false,
        }
    }
}

impl BodySet {
    /// `field`, and the first `landmarks` of the compiled ones (no more than this build knows of).
    pub fn new(field: Arc<Field>, landmarks: u8) -> Self {
        Self { field, landmarks: landmarks.min(LANDMARKS.len().min(MAX_LANDMARKS) as u8), interior: false }
    }

    /// The same, in the colony's inside if `interior`: its city is a body.
    pub fn inside(self, interior: bool) -> Self {
        Self { interior, ..self }
    }

    /// Whether the sector is the colony's inside.
    pub fn interior(&self) -> bool {
        self.interior
    }

    /// The sector's landmarks, by id.
    pub fn landmarks(&self) -> &'static [LandmarkDef] {
        &LANDMARKS[..usize::from(self.landmarks)]
    }

    /// Whether `body` names one of these: a rock of the field, a landmark of the sector, or the
    /// city of the colony it's the inside of.
    pub fn knows(&self, body: BodyRef) -> bool {
        match body {
            BodyRef::Rock(r) => usize::from(r) < self.field.len(),
            BodyRef::Landmark(k) => usize::from(k) < self.landmarks().len(),
            BodyRef::City => self.interior,
            BodyRef::Bay(n) => !self.interior && bc_sim::colony::hub::is_bay(n),
        }
    }

    /// Every body at tick `t`, as the simulation has them.
    pub fn at(&self, t: u32) -> Bodies<'_> {
        Bodies::at(&self.field, self.landmarks(), t).inside(self.interior)
    }

    /// Where `b` is at time `t` (ticks, fractional): a shattered rock where it was.
    pub fn pose_at(&self, b: Body, t: f64) -> Option<BodyPose> {
        match b {
            Body::City => self.interior.then(|| BodyPose::fixed(Vec3::ZERO, Quat::IDENTITY)),
            Body::Bay(_) if self.interior => None,
            _ => body_pose(&self.field, self.landmarks(), b, t),
        }
    }

    /// The shape of `b`, in its frame.
    pub fn shape(&self, b: Body) -> Option<Shape> {
        match b {
            Body::City => self.interior.then(Shape::city),
            Body::Bay(_) if self.interior => None,
            _ => body_shape(&self.field, self.landmarks(), b),
        }
    }

    /// Rock `i` shattered (or grew back), as a rock record says. The field is the view's own: the
    /// predictor's copy revives a dated break while it replays the ticks before it.
    pub fn set_rock_dead(&mut self, i: usize, dead: bool) {
        if self.field.is_dead(i) != dead {
            Arc::make_mut(&mut self.field).set_dead(i, dead);
        }
    }
}

/// Where `b`, one of `field`'s rocks or of `landmarks`, is at time `t` (ticks, fractional), exactly
/// as the simulation poses it at a tick and the fraction of the next: a shattered rock where it was.
pub fn body_pose(field: &Field, landmarks: &[LandmarkDef], b: Body, t: f64) -> Option<BodyPose> {
    let k = t.max(0.0).floor();
    let frac = (t.max(0.0) - k) as f32;
    match b {
        // (Only a [`BodySet`] in the colony's inside knows its city.)
        Body::None | Body::City => None,
        Body::Rock(r) => field.rocks().get(usize::from(r)).map(|rock| BodyPose::fixed(rock.pos, rock.rot)),
        Body::Landmark(i) => landmarks.get(usize::from(i)).map(|d| landmark_pose(d, k as u32, frac)),
        Body::Bay(n) => {
            bc_sim::colony::hub::is_bay(n).then(|| bc_sim::colony::hub::bay_pose(n, k as u32, frac))
        }
    }
}

/// The shape of `b`, one of `field`'s rocks or of `landmarks`, in its frame.
pub fn body_shape(field: &Field, landmarks: &[LandmarkDef], b: Body) -> Option<Shape> {
    match b {
        Body::None | Body::City => None,
        Body::Rock(r) => field.rocks().get(usize::from(r)).map(|rock| Shape::ellipsoid(rock.axes)),
        Body::Landmark(i) => landmarks.get(usize::from(i)).map(|d| d.shape),
        Body::Bay(n) => bc_sim::colony::hub::is_bay(n).then(bc_sim::colony::hub::bay_shape),
    }
}

/// How far the chase camera keeps from any body's surface, m.
pub const CAM_CLEAR: f32 = 4.0;

/// The camera's place on the way from `from` (the suit) to `to` (where the rig would put it),
/// stopped short of the first rock or landmark (posed at time `t`, ticks) by [`CAM_CLEAR`]: the
/// camera never looks out from inside a body.
pub fn camera_clamp(bodies: &BodySet, t: f64, from: Vec3, to: Vec3) -> Vec3 {
    let k = t.max(0.0).floor();
    let frac = (t.max(0.0) - k) as f32;
    let rock = bodies.field.sweep(from, to, CAM_CLEAR).map(|(s, _)| s);
    let landmark = sweep_landmarks(bodies.landmarks(), from, to, CAM_CLEAR, k as u32, frac).map(|(s, _)| s);
    match (rock, landmark) {
        (Some(a), Some(b)) => from + (to - from) * a.min(b),
        (Some(s), None) | (None, Some(s)) => from + (to - from) * s,
        (None, None) => to,
    }
}

/// The surface a suit coming in with its grip armed is heading for: what the HUD's landing ring
/// shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHint {
    pub body: Body,
    /// The point on the surface under the suit, and the surface's normal there (sector frame).
    pub point: Vec3,
    pub normal: Vec3,
    /// How high the suit's feet are over it, m.
    pub height: f32,
    /// How fast the suit moves relative to it, m/s.
    pub speed: f32,
    /// Coming in like this, it would be caught: the very test the simulation makes
    /// ([`bc_sim::ground::catch_candidate`]). Otherwise it is too fast, or too high.
    pub catch: bool,
}

/// The nearest surface a suit flying as `f` could land on: its feet within [`LEVEL_RANGE`] of it,
/// at under [`LEVEL_SPEED`] relative to it (where an armed grip starts to roll the feet toward it).
/// Whether it would be caught is the simulation's own test, so a green ring means exactly what
/// the next tick will do with the grip armed.
pub fn surface_hint(b: &Bodies, f: &FlightState) -> Option<SurfaceHint> {
    let near = b.nearest_grippable(f, LEVEL_RANGE, LEVEL_SPEED, f32::INFINITY)?;
    let catch =
        b.nearest_grippable(f, CATCH_RANGE, CATCH_SPEED, CATCH_LEAVE).is_some_and(|c| c.body == near.body);
    Some(hint_of(&near, catch))
}

fn hint_of(near: &Near, catch: bool) -> SurfaceHint {
    // The surface below the origin: the stance and the feet's height under it.
    let foot = near.local - near.n_local * (near.h + STANCE);
    SurfaceHint {
        body: near.body,
        point: near.pose.to_world(foot),
        normal: near.n_world,
        height: near.h,
        speed: near.v_rel.length(),
        catch,
    }
}

/// How fast the gap between a suit and a surface opens, m/s: the suit's velocity relative to the
/// surface (`rel_vel`) along its outward normal. Negative while it closes on it, as a range rate
/// reads: the HUD's figure by a landmark's name.
pub fn range_rate(rel_vel: Vec3, outward: Vec3) -> f32 {
    rel_vel.dot(outward)
}

/// How the own suit's armed grip let go of a body, as the HUD tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LetGo {
    /// It climbed out of the grip on the thrusters, as asked: flying.
    Flying,
    /// Its pilot changed its form, which can't hold on (a Neo-Bird can't grip): taking off, as
    /// asked.
    Transformed,
    /// Of its own accord: too high, too fast, the rock gone, blown off. `GRIP LOST`.
    Lost,
}

/// How a grip still armed let go: from how the suit was on the body (`was`), whether it climbed
/// out of the grip on the thrusters (`lifted_off`), and whether it can grip now at all (`can_grip`:
/// a frame with legs, not changing form, as the step has it).
pub fn let_go(was: Footing, lifted_off: bool, can_grip: bool) -> LetGo {
    if !can_grip {
        LetGo::Transformed
    } else if was == Footing::Aloft && lifted_off {
        LetGo::Flying
    } else {
        LetGo::Lost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_rate_is_negative_closing() {
        let n = Vec3::Y;
        assert_eq!(range_rate(Vec3::new(3.0, -50.0, 0.0), n), -50.0, "closing");
        assert_eq!(range_rate(Vec3::new(0.0, 50.0, 4.0), n), 50.0, "opening");
    }

    #[test]
    fn folding_into_a_bird_on_a_body_is_not_losing_the_grip() {
        // Changing form lets go (T2), from the ground or the air: asked for, so not GRIP LOST.
        for was in [Footing::Grounded, Footing::Aloft] {
            assert_eq!(let_go(was, false, false), LetGo::Transformed);
        }
        assert_eq!(let_go(Footing::Aloft, true, true), LetGo::Flying);
        assert_eq!(let_go(Footing::Grounded, false, true), LetGo::Lost, "the rock shattered underfoot");
        assert_eq!(let_go(Footing::Aloft, false, true), LetGo::Lost, "too fast, or blown off");
    }

    #[test]
    fn bodies_are_where_the_simulation_has_them_at_any_moment() {
        let field = Arc::new(Field::generate(0xDEB12, 160));
        let set = BodySet::new(field.clone(), 2);
        assert!(set.knows(BodyRef::Rock(159)) && !set.knows(BodyRef::Rock(160)));
        assert!(set.knows(BodyRef::Landmark(1)) && !set.knows(BodyRef::Landmark(2)));
        let mo_ii = Body::Landmark(0);
        for t in [0u32, 1, 9_599, 9_600, 54_001, 1_000_000] {
            // On the tick, exactly the simulation's.
            assert_eq!(set.pose_at(mo_ii, f64::from(t)), set.at(t).pose(mo_ii), "at {t}");
            // Between ticks, the closed form at the fraction.
            assert_eq!(set.pose_at(mo_ii, f64::from(t) + 0.25), Some(landmark_pose(&LANDMARKS[0], t, 0.25)));
        }
        let rock = field.rocks()[7];
        assert_eq!(set.pose_at(Body::Rock(7), 12.5).map(|p| p.pos), Some(rock.pos));
        // A sector with only MO-II has no Hermit.
        let one = BodySet::new(field, 1);
        assert!(one.pose_at(Body::Landmark(1), 0.0).is_none() && !one.knows(BodyRef::Landmark(1)));
    }

    #[test]
    fn the_ring_is_green_exactly_when_the_simulation_would_catch() {
        let set = BodySet::new(Arc::new(Field::empty()), 2);
        // Over the top of MO-II's +Y pylon, as it rolls and drifts.
        let mo_ii = Body::Landmark(0);
        let b = set.at(500);
        let pose = b.pose(mo_ii).unwrap();
        let (p, n) = b.surface_along(mo_ii, Vec3::Y).unwrap();
        let (p, n) = (pose.to_world(p), pose.rot * n);
        let over = |h: f32, rel: Vec3| {
            let pos = p + n * (STANCE + h);
            FlightState { pos, vel: pose.point_vel(pos) + rel, ..FlightState::default() }
        };
        // Slow and close: green, the ring on the surface under the suit.
        let hint = surface_hint(&b, &over(18.0, -n * 3.2)).expect("a surface");
        assert!(hint.catch && hint.body == mo_ii);
        assert!((hint.height - 18.0).abs() < 0.05 && (hint.speed - 3.2).abs() < 1e-3);
        assert!(hint.point.distance(p) < 0.05, "{:?} off {p:?}", hint.point);
        // Too fast, or too high: shown, not green.
        assert!(!surface_hint(&b, &over(18.0, -n * 14.0)).unwrap().catch);
        assert!(!surface_hint(&b, &over(60.0, -n * 3.0)).unwrap().catch);
        // Out of reach, or far too fast: nothing.
        assert!(surface_hint(&b, &over(160.0, Vec3::ZERO)).is_none());
        assert!(surface_hint(&b, &over(20.0, -n * 45.0)).is_none());
    }
}
