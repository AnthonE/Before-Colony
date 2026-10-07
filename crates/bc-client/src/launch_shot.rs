//! The launch shot: after the bay's catapult cuts to black, the camera opens outside, by where the
//! suit comes out, and watches it go before handing over to the chase camera.
//!
//! - **Into space**, the suit is thrown out of its pilot's own bay door on the spinning bay ring
//!   (`bc_sim::colony::hub`): the camera rides with it a little ahead and to the side, outside
//!   the ring, its up toward the colony's axis (the bay's), so the open door and the ring wheel
//!   away behind the suit.
//! - **Into the colony** (Q at the cockpit), it comes out of the port in the end cap's inner face
//!   behind the inner gate: the suit as drawn eases out of the port to where it really is (the
//!   server put it by the gate), and the camera looks back at it with the cap behind.
//!
//! The shot holds for [`HOLD_SECS`], then blends into the chase camera over [`BLEND_SECS`]; any
//! flight key cuts it short. Only the drawing: the suit flies as the server flies it throughout.

use bc_sim::colony::frame::up_at;
use bc_sim::colony::hub::bay_pose;
use bc_sim::colony::interior::INNER_PORT;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::net::GameClient;
use crate::onfoot::{OnFoot, Seq};
use crate::page::Ui;
use crate::view::{CameraTarget, DrawnBodies};

/// How long the shot holds outside, s, and how long it blends into the chase camera after, s.
pub const HOLD_SECS: f32 = 2.6;
pub const BLEND_SECS: f32 = 1.2;
/// Inside the colony, how long the suit as drawn takes to come out of the port to where it is, s.
const EMERGE_SECS: f32 = 1.4;

/// The shot going on, if any.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LaunchShot {
    /// When it began (the page's clock, s).
    since: Option<f64>,
    /// Into the colony (the port), rather than out of a bay door.
    inside: bool,
    /// The bay the suit came out of (outside).
    pub bay: u8,
    /// The camera's place round the suit and its up, fixed as the shot begins (sector frame).
    ahead: Vec3,
    side: Vec3,
    up: Vec3,
    /// A flight key cut the hold short at this moment (the blend starts then).
    cut: Option<f64>,
    /// How many shots have begun (for the E2E tests: one may be over between their looks).
    pub count: u32,
}

impl LaunchShot {
    /// Seconds into the shot at `now`, while it's on.
    fn t(&self, now: f64) -> Option<f32> {
        let t = (now - self.since?) as f32;
        (t < HOLD_SECS + BLEND_SECS).then_some(t)
    }

    /// The shot is on (holding, or blending into the chase camera).
    pub fn active(&self, now: f64) -> bool {
        self.t(now).is_some()
    }

    /// How far the shot has handed over to the chase camera at `now`, 0..1.
    fn handover(&self, now: f64) -> f32 {
        let Some(since) = self.since else { return 1.0 };
        let from = self.cut.map_or(since + f64::from(HOLD_SECS), |c| c.min(since + f64::from(HOLD_SECS)));
        smoothstep(0.0, BLEND_SECS, (now - from) as f32)
    }

    /// Inside the colony: how far the own suit is drawn from where it is, coming out of the port
    /// (`pos`: where it is, in the colony's frame).
    pub fn emerging(&self, now: f64, pos: Vec3) -> Vec3 {
        match self.t(now) {
            Some(t) if self.inside && t < EMERGE_SECS => {
                let left = (1.0 - t / EMERGE_SECS).powi(3);
                Vec3::X * ((INNER_PORT.x - pos.x) * left)
            }
            _ => Vec3::ZERO,
        }
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Starts the shot as a launch's catapult ends (or is skipped) and the pilot is out: the sequence
/// on foot went from venting or the catapult straight to flying.
pub fn begin_launch_shot(
    me: Res<OnFoot>,
    game: NonSend<GameClient>,
    bodies: Res<DrawnBodies>,
    mut ui: ResMut<Ui>,
    mut shot: ResMut<LaunchShot>,
    mut was: Local<Seq>,
) {
    let seq = me.seq;
    let launched = matches!(*was, Seq::Venting | Seq::Catapult) && seq == Seq::Walking;
    *was = seq;
    if !launched {
        return;
    }
    let g = game.borrow();
    if g.core.hangar.place != Some(bc_econ::wire::Place::Space) {
        return;
    }
    let now = crate::net::now_s();
    let inside = g.core.welcome.is_some_and(|w| w.interior);
    let bay = g.core.hangar.bay;
    let (ahead, side, up) = if inside {
        // Down the colony, its up toward the axis.
        let at = g.core.own_view().map_or(bc_sim::colony::interior::INNER_GATE, |v| v.pos);
        let up = up_at(at);
        (Vec3::X, Vec3::X.cross(up).normalize_or(Vec3::Z), up)
    } else {
        // Out of the door, its up toward the axis, the side the way the spin carries the bay.
        let tick = bodies.t.max(0.0);
        let p = bay_pose(bay, tick.floor() as u32, tick.fract() as f32);
        (p.rot * -Vec3::X, p.rot * Vec3::Z, p.rot * -Vec3::Y)
    };
    *shot = LaunchShot { since: Some(now), inside, bay, ahead, side, up, cut: None, count: shot.count + 1 };
    if !inside {
        ui.news(
            format!("BAY {bay:02} · OUT OF ITS DOOR · HOME: AT REST IN THE DOCK'S RING OFF THE HUB'S MOUTH"),
            false,
        );
    }
}

/// While the shot is on, the camera: placed round the suit as drawn, looking back at it (the door,
/// or the port, behind it), then easing into the chase camera `follow` placed this frame.
pub fn launch_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    target: Res<CameraTarget>,
    mut shot: ResMut<LaunchShot>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    let now = crate::net::now_s();
    let Some(t) = shot.t(now) else { return };
    let Some(suit) = target.0 else { return };
    let Ok(mut tf) = cam.single_mut() else { return };
    // Flying by hand cuts the hold short.
    let flown =
        [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::Space, KeyCode::ShiftLeft]
            .into_iter()
            .any(|k| keys.just_pressed(k))
            || mouse.just_pressed(MouseButton::Left);
    if flown && shot.cut.is_none() && t < HOLD_SECS {
        shot.cut = Some(now);
    }
    // Ahead of the suit and to the side, swinging round behind it as the hold goes on.
    let u = smoothstep(0.0, HOLD_SECS, t);
    let (ahead, side, up) = (shot.ahead, shot.side, shot.up);
    let (near, far) = if shot.inside {
        (ahead * 70.0 + side * 30.0 + up * 12.0, ahead * 20.0 + side * 45.0 + up * 18.0)
    } else {
        (ahead * 55.0 + side * 40.0 + up * 18.0, ahead * 15.0 + side * 55.0 + up * 22.0)
    };
    let eye = suit.pos + near.lerp(far, u);
    let held = Transform::from_translation(eye).looking_at(suit.pos, up);
    let k = shot.handover(now);
    let chase = *tf;
    tf.translation = held.translation.lerp(chase.translation, k);
    tf.rotation = held.rotation.slerp(chase.rotation, k);
}

/// Opens the pilot's bay door while their suit rides its cradle in it and as it's thrown out (the
/// door isn't drawn), and closes it again once the suit is clear.
pub fn open_bay_doors(
    game: NonSend<GameClient>,
    shot: Res<LaunchShot>,
    mut doors: Query<(&crate::colony::BayDoor, &mut Visibility)>,
) {
    let g = game.borrow();
    let now = crate::net::now_s();
    let open = g.core.predict.bay().or((shot.active(now) && !shot.inside).then_some(shot.bay));
    for (door, mut vis) in &mut doors {
        let want = if Some(door.0) == open { Visibility::Hidden } else { Visibility::Inherited };
        vis.set_if_neq(want);
    }
}

/// For the E2E tests: whether the launch shot is on.
pub fn publish_launch_shot(shot: Res<LaunchShot>, mut dev: ResMut<crate::dev_hooks::DevStatus>) {
    dev.set("launch_shot", shot.active(crate::net::now_s()));
    dev.set("launch_shots", shot.count);
}
