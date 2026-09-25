//! Keyboard and mouse: pointer-lock free aim, 6DOF thrust, weapons, and state toggles.
//!
//! | Key | Action |
//! |---|---|
//! | mouse | aim (click to lock the pointer, Esc to release) |
//! | W/S, A/D, Space/C | thrust forward/back, left/right, up/down |
//! | Q/E | roll |
//! | Shift | boost · X brake · R RCS (fast turns, burns propellant) |
//! | LMB / RMB / F | primary / secondary / beam saber |
//! | V | flight assist on/off · Z ZERO System on/off |
//! | 1 / 2 | respawn as Leo / Wing Gundam Zero (when destroyed) |

use bc_proto::FrameId;
use bc_proto::buttons::{BOOST, BRAKE, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, RCS_SHARP, ZERO};
use bc_proto::{InputCmd, NO_SLOT};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::net::GameClient;

const SENSITIVITY: f32 = 0.0022;

/// Where the pilot is aiming (world direction; the camera looks along it).
#[derive(Resource)]
pub struct Aim {
    pub dir: Vec3,
    pub initialized_for: Option<(u16, u8)>,
}

impl Default for Aim {
    fn default() -> Self {
        Self { dir: Vec3::Z, initialized_for: None }
    }
}

/// The pilot's current controls.
#[derive(Resource)]
pub struct Controls {
    pub thrust: Vec3,
    pub roll: f32,
    pub buttons: u16,
    pub flight_assist: bool,
    pub zero: bool,
    pub locked: bool,
    swallow_click: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            thrust: Vec3::ZERO,
            roll: 0.0,
            buttons: 0,
            flight_assist: true,
            zero: false,
            locked: false,
            swallow_click: false,
        }
    }
}

impl Controls {
    pub fn command(&self, aim: Vec3) -> InputCmd {
        let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
        let mut buttons = self.buttons;
        if self.flight_assist {
            buttons |= FLIGHT_ASSIST;
        }
        if self.zero {
            buttons |= ZERO;
        }
        InputCmd {
            aim,
            thrust: [q(self.thrust.x), q(self.thrust.y), q(self.thrust.z)],
            roll: q(self.roll),
            buttons,
            lock_target: NO_SLOT,
            ..InputCmd::default()
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut controls: ResMut<Controls>,
    mut aim: ResMut<Aim>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    game: NonSend<GameClient>,
) {
    let mut game = game.borrow_mut();
    // Keep the aim sane across (re)spawns: start looking where the suit looks.
    if let Some(own) = game.core.world.own
        && own.alive
        && aim.initialized_for != Some((own.slot, own.generation))
    {
        aim.dir = own.rot * Vec3::Z;
        aim.initialized_for = Some((own.slot, own.generation));
    }
    if game.autopilot {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && cursor.grab_mode != CursorGrabMode::Locked {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
        controls.swallow_click = true;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    controls.locked = cursor.grab_mode == CursorGrabMode::Locked;
    if !mouse.pressed(MouseButton::Left) {
        controls.swallow_click = false;
    }

    let up = game.core.predict.state.rot * Vec3::Y;
    if controls.locked && motion.delta != Vec2::ZERO {
        let d = motion.delta * SENSITIVITY;
        let right = aim.dir.cross(up).normalize_or(Vec3::X);
        let dir = Quat::from_axis_angle(up, -d.x) * (Quat::from_axis_angle(right, -d.y) * aim.dir);
        // Don't let the aim flip over the suit's head.
        if dir.dot(up).abs() < 0.97 {
            aim.dir = dir.normalize();
        } else {
            aim.dir = (Quat::from_axis_angle(up, -d.x) * aim.dir).normalize();
        }
    }
    let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
    controls.thrust = Vec3::new(
        axis(KeyCode::KeyD, KeyCode::KeyA),
        (keys.pressed(KeyCode::Space) as i32
            - (keys.pressed(KeyCode::KeyC) || keys.pressed(KeyCode::ControlLeft)) as i32) as f32,
        axis(KeyCode::KeyW, KeyCode::KeyS),
    );
    controls.roll = axis(KeyCode::KeyE, KeyCode::KeyQ);
    let mut b = 0;
    if controls.locked && !controls.swallow_click && mouse.pressed(MouseButton::Left) {
        b |= FIRE_PRIMARY;
    }
    if controls.locked && mouse.pressed(MouseButton::Right) {
        b |= FIRE_SECONDARY;
    }
    if keys.pressed(KeyCode::KeyF) {
        b |= MELEE;
    }
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        b |= BOOST;
    }
    if keys.pressed(KeyCode::KeyX) {
        b |= BRAKE;
    }
    if keys.pressed(KeyCode::KeyR) {
        b |= RCS_SHARP;
    }
    controls.buttons = b;
    if keys.just_pressed(KeyCode::KeyV) {
        controls.flight_assist = !controls.flight_assist;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        controls.zero = !controls.zero;
    }
    let dead = game.core.world.own.is_some_and(|o| !o.alive);
    if dead && keys.just_pressed(KeyCode::Digit1) {
        game.respawn_request = Some(FrameId::Leo);
    }
    if dead && keys.just_pressed(KeyCode::Digit2) {
        game.respawn_request = Some(FrameId::WingZero);
    }
}
