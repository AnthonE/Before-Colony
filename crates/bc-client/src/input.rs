//! Keyboard and mouse: pointer-lock free aim, 6DOF thrust, weapons, and state toggles.
//!
//! The same list, for players, is `bc_client_core::controls::BINDINGS` (the title screen and F1
//! show it); keep them together. Esc and F1 belong to the page (`page.rs`), and the pointer's lock
//! to `pointer.rs`.
//!
//! | Key | Action |
//! |---|---|
//! | mouse | aim (click to lock the pointer, Esc for the menu) |
//! | W/S, A/D, Space/C | thrust forward/back, left/right, up/down; double-tapped, a burst step |
//! | Q/E | roll |
//! | L | grip: armed, a suit coming in slow and close is caught and landed; off, it lets go |
//! | on a body: W/A/S/D, Shift, Space, C | walk, run, hop (held: lift off), crouch (a toggle) |
//! | Shift | boost · X brake · R RCS (fast turns, burns propellant) |
//! | LMB / RMB / F | primary / secondary / melee |
//! | H | the frame's special: Neo-Bird or the Hyper Jammer on/off, or held: Full Open, Cross Crusher |
//! | V | flight assist on/off · Z ZERO System on/off |
//! | Tab, mouse wheel | the chase camera or the cockpit (wheel in: the cockpit, out: chasing) |
//! | M | the chart: the sector and the Earth Sphere in 3D, the objectives, courses (`chart.rs`) |
//! | 1–6 | respawn as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock, Shenlong (when destroyed) |
//!
//! Down is C alone: Left Ctrl held with W would be Ctrl+W, which closes the browser's tab.
//!
//! While the chart is open the keys and the mouse are the chart's, and the stick is let go (flight
//! assist holds the suit still, unless the auto-nav flies it). The auto-nav hands the stick back
//! the moment the pilot thrusts, boosts or brakes; moving the mouse takes back only the aim (it
//! keeps flying the course, whichever way the suit looks).
//!
//! On a body, `thrust[1]` says what the legs do (`bc_sim::ground`): Space sends 127 (stand, then
//! hop; held, the thrusters lift off), the crouch toggle -127 (crouch, and stay down whatever
//! comes), and standing back up from one 64, until the stance is full; otherwise 0, which keeps
//! the stance. In the air in a body's grip, Space and C are the thrusters' again. While the suit is
//! on a turning body the aim turns with it, so a still mouse keeps its bearing on the deck.

use bc_client_core::doubletap::{DIRECTIONS, DoubleTap};
use bc_client_core::lockon::{Broke, HOLD_TO_RELEASE};
use bc_client_core::settings::CameraView;
use bc_proto::buttons::{
    BOOST, BRAKE, BURST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, GRIP, JETTISON, MELEE, MODE,
    RCS_SHARP, SPECIAL, STOW, THROW, ZERO,
};
use bc_proto::snapshot::footing;
use bc_proto::{InputCmd, NO_SLOT};
use bc_sim::bodies::Body;
use bc_sim::content::{PLAYABLE_ORDER, frame};
use bc_sim::ground::{Footing, STANCE, STAND_LEVEL};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

use crate::net::GameClient;
use crate::page::Ui;
use crate::pointer::PointerRes;
use crate::session::Pilot;
use crate::settings::SettingsRes;

const SENSITIVITY: f32 = 0.0022;
/// `thrust[1]` standing back up from a crouch: past the stand level, short of a hop's.
const STAND_UP: i8 = 64;
const _: () = assert!(STAND_UP >= STAND_LEVEL && STAND_UP < bc_sim::ground::JUMP_LEVEL);

/// Switches the flight camera between the chase camera and the cockpit.
pub const CAMERA_KEY: KeyCode = KeyCode::Tab;
/// Locks on (`bc_client_core::lockon`); so does a click of the middle button.
pub const LOCK_KEY: KeyCode = KeyCode::KeyY;
/// A trackpad's scroll this small (pixels in a frame) is a brush, not a turn of the wheel.
const SCROLL_PX: f32 = 8.0;

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
    /// The free hand grabs whatever comes in reach (and holds it) while on.
    pub grab: bool,
    /// The frame's toggled special (Neo-Bird, the Hyper Jammer) is asked for.
    pub mode: bool,
    /// The grip is armed (L): coming in slow and close to a body, the suit is caught and landed;
    /// on one, it holds on.
    pub grip: bool,
    /// Crouched, on the ground (C toggles it there; leaving the ground clears it).
    pub crouch: bool,
    /// Standing back up out of a crouch: the stance isn't full yet.
    pub standing_up: bool,
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
            grab: false,
            mode: false,
            grip: false,
            crouch: false,
            standing_up: false,
            locked: false,
            swallow_click: false,
        }
    }
}

impl Controls {
    /// The command, aimed along `aim`, designating `lock` for a missile lock.
    pub fn command(&self, aim: Vec3, lock: Option<u16>) -> InputCmd {
        let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
        let mut buttons = self.buttons;
        if self.flight_assist {
            buttons |= FLIGHT_ASSIST;
        }
        if self.zero {
            buttons |= ZERO;
        }
        if self.grab {
            buttons |= GRAB;
        }
        if self.mode {
            buttons |= MODE;
        }
        if self.grip {
            buttons |= GRIP;
        }
        // Standing up says so plainly, short of a hop's level.
        let lift = if self.standing_up && self.thrust.y == 0.0 { STAND_UP } else { q(self.thrust.y) };
        InputCmd {
            aim,
            thrust: [q(self.thrust.x), lift, q(self.thrust.z)],
            roll: q(self.roll),
            buttons,
            lock_target: lock.unwrap_or(NO_SLOT),
            ..InputCmd::default()
        }
    }
}

/// Locks on and lets go (`bc_client_core::lockon`). [`LOCK_KEY`] (or a click of the middle button):
/// a tap locks the hostile nearest the crosshair, or moves the lock on to the next; held for
/// [`HOLD_TO_RELEASE`], it lets go. The lock also lets go by itself: the target downed, out of sight
/// or too far, or the pilot's own suit gone or stepped out of. A lock taken while the auto-nav flies
/// takes the stick back from it.
fn lock_on(
    game: &mut crate::net::Game,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    aim: Vec3,
    ui: &mut Ui,
    pressed_at: &mut Option<f64>,
    flying: bool,
) {
    let now = crate::net::now_s();
    let t = game.core.render_tick(now);
    let from = game.core.own_view().map_or(game.core.predict.state.pos, |v| v.pos);
    let alive = game.core.world.own.is_some_and(|o| o.alive);
    if !alive || !flying {
        if game.hard.locked() && !alive {
            game.hard.release();
        }
        *pressed_at = None;
        return;
    }
    if let Some(why) = game.hard.validate(&game.core.world, from, t) {
        ui.toast(match why {
            Broke::Downed => "TARGET DOWN: LOCK RELEASED",
            Broke::Lost => "LOCK LOST: OUT OF SIGHT",
            Broke::Range => "LOCK LOST: OUT OF RANGE",
            Broke::Released => "LOCK RELEASED",
        });
    }
    let held = keys.pressed(LOCK_KEY) || mouse.pressed(MouseButton::Middle);
    // A tap that went down and up between two frames (a slow frame) is still a press.
    if pressed_at.is_none() && (keys.just_pressed(LOCK_KEY) || mouse.just_pressed(MouseButton::Middle)) {
        *pressed_at = Some(now);
    }
    match (*pressed_at, held) {
        (None, true) => *pressed_at = Some(now),
        // Held long enough: let go (once; the press is spent).
        (Some(at), true) if now - at >= HOLD_TO_RELEASE => {
            if game.hard.locked() {
                game.hard.release();
                ui.toast("LOCK RELEASED");
            }
            *pressed_at = Some(f64::INFINITY);
        }
        // A tap.
        (Some(at), false) => {
            if at.is_finite() {
                let was = game.hard.slot();
                if game.hard.tap(&game.core.world, from, aim, t) {
                    let name = game.hard.slot().map(|s| game.core.world.name_of(s)).unwrap_or_default();
                    // Locking on is choosing to fight: the stick comes back from the auto-nav.
                    let nav = if game.nav.take().is_some() { " · AUTO-NAV OFF" } else { "" };
                    ui.toast(format!("LOCKED ON: {name}{nav}"));
                } else if was.is_none() {
                    ui.toast("NOTHING TO LOCK ON TO");
                }
            }
            *pressed_at = None;
        }
        _ => {}
    }
}

/// [`CAMERA_KEY`] switches between the chase camera and the cockpit; the mouse wheel goes in (the
/// cockpit) or out (chasing). Kept in the settings, so the next sortie starts in the same view.
pub fn toggle_camera(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    pointer: Res<PointerRes>,
    mut ui: ResMut<Ui>,
    mut settings: ResMut<SettingsRes>,
    indoors: Res<crate::hangar::Indoors>,
    map: Res<crate::map::MapOpen>,
) {
    if !ui.playing() || ui.panel_open() || indoors.0 || map.0 {
        return;
    }
    let now = settings.0.camera;
    let wheel = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel if scroll.delta.y.abs() >= SCROLL_PX => scroll.delta.y,
        MouseScrollUnit::Pixel => 0.0,
    };
    let view = if keys.just_pressed(CAMERA_KEY) {
        now.toggled()
    } else if pointer.0.flying() && wheel > 0.0 {
        CameraView::Cockpit
    } else if pointer.0.flying() && wheel < 0.0 {
        CameraView::Chase
    } else {
        now
    };
    if view != now {
        settings.0.camera = view;
        ui.toast(format!("CAMERA: {}", view.name().to_uppercase()));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut controls: ResMut<Controls>,
    mut aim: ResMut<Aim>,
    pointer: Res<PointerRes>,
    mut ui: ResMut<Ui>,
    settings: Res<SettingsRes>,
    indoors: Res<crate::hangar::Indoors>,
    map: Res<crate::map::MapOpen>,
    mut pilot: ResMut<Pilot>,
    game: NonSend<GameClient>,
    mut deck: Local<Option<(Body, Quat)>>,
    mut lock_key: Local<Option<f64>>,
    mut tap: Local<DoubleTap>,
) {
    let mut game = game.borrow_mut();
    // Keep the aim sane across (re)spawns: start looking where the suit looks.
    if let Some(own) = game.core.world.own
        && own.alive
        && aim.initialized_for != Some((own.slot, own.generation))
    {
        aim.dir = own.rot * Vec3::Z;
        aim.initialized_for = Some((own.slot, own.generation));
        // A fresh suit comes out in its first form, jammer off. One woken on a body holds on to
        // it, standing or crouched as it was left.
        controls.mode = false;
        controls.grip = own.surface.is_some();
        controls.crouch = own
            .surface
            .is_some_and(|s| s.footing == footing::GROUNDED && f32::from(s.stance_q) / 16.0 < STANCE);
        if let Some(spot) = crate::hud::hide_spot(&game.core)
            && game.core.welcome.is_some_and(|w| w.woke)
            && own.cover == bc_proto::snapshot::cover::HIDDEN
        {
            ui.toast(format!("WOKE IN {spot} - hidden. Move or fire and you're seen."));
        }
    }
    // On a turning body the aim turns with it, so a still mouse holds its bearing on the deck.
    let on = game.core.own_view().and_then(|v| Some((v.ground?.body, v.t_view)));
    let now = on.and_then(|(b, t)| Some((b, game.core.world.bodies.pose_at(b, t)?.rot)));
    if let (Some((b, rot)), Some((was, before))) = (now, *deck)
        && b == was
    {
        aim.dir = (rot * before.conjugate() * aim.dir).normalize_or(aim.dir);
    }
    *deck = now;
    if game.autopilot {
        return;
    }
    // A click that takes the pointer doesn't also fire.
    if mouse.just_pressed(MouseButton::Left) && !pointer.0.flying() {
        controls.swallow_click = true;
    }
    if !mouse.pressed(MouseButton::Left) {
        controls.swallow_click = false;
    }
    controls.locked = pointer.0.flying();
    // Not on the chart either: its middle drag pans, which mustn't lock on or let go.
    let flying = ui.playing() && !ui.panel_open() && !indoors.0 && !map.0;
    lock_on(&mut game, &keys, &mouse, aim.dir, &mut ui, &mut lock_key, flying);
    if !ui.playing() || ui.panel_open() || indoors.0 || map.0 {
        // Hands off the stick in menus, on the chart, and on foot (or while the bay launches the
        // suit); the toggles stay as they were.
        controls.thrust = Vec3::ZERO;
        controls.roll = 0.0;
        controls.buttons = 0;
        tap.clear();
        return;
    }

    // Flying by hand takes the stick back from the auto-nav; the mouse takes only the aim.
    let flown = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::Space, KeyCode::KeyC]
        .into_iter()
        .chain([KeyCode::KeyX, KeyCode::ShiftLeft, KeyCode::ShiftRight])
        .any(|k| keys.just_pressed(k));
    if flown && game.nav.take().is_some() {
        ui.toast("AUTO-NAV OFF: you have the stick");
    }
    if controls.locked
        && motion.delta.length() > 2.0
        && let Some(n) = game.nav.as_mut().filter(|n| n.look)
    {
        n.look = false;
    }
    // Turn the aim about the suit's up as drawn (as the camera shows it).
    let up = game.core.own_view().map_or(game.core.predict.state.rot, |v| v.rot) * Vec3::Y;
    if controls.locked && motion.delta != Vec2::ZERO {
        let mut d = motion.delta * SENSITIVITY * settings.0.sensitivity;
        if settings.0.invert_y {
            d.y = -d.y;
        }
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
    // On the ground C toggles a crouch, and Space hops (out of a crouch, it stands first); in the
    // air they're the thrusters. Leaving the ground stands the crouch down.
    let mover = game.core.predict.mover();
    let lift = if mover.footing == Footing::Grounded {
        if keys.just_pressed(KeyCode::KeyC) {
            controls.crouch = !controls.crouch;
        }
        if keys.pressed(KeyCode::Space) {
            controls.crouch = false;
            1.0
        } else if controls.crouch {
            -1.0
        } else {
            0.0
        }
    } else {
        controls.crouch = false;
        axis(KeyCode::Space, KeyCode::KeyC)
    };
    controls.standing_up =
        mover.footing == Footing::Grounded && !controls.crouch && mover.anchor.stance < STANCE;
    controls.thrust = Vec3::new(axis(KeyCode::KeyD, KeyCode::KeyA), lift, axis(KeyCode::KeyW, KeyCode::KeyS));
    controls.roll = axis(KeyCode::KeyE, KeyCode::KeyQ);
    let mut b = 0;
    // A direction double-tapped, off the ground: a burst step that way, the stick held that way
    // while it's pressed.
    const STEP_KEYS: [KeyCode; DIRECTIONS] =
        [KeyCode::KeyD, KeyCode::KeyA, KeyCode::Space, KeyCode::KeyC, KeyCode::KeyW, KeyCode::KeyS];
    if settings.0.double_tap && mover.footing != Footing::Grounded {
        let now = crate::net::now_s();
        for (k, key) in STEP_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                tap.press(k, now);
            }
        }
        if let Some((axis, sign)) = tap.stepping(STEP_KEYS.map(|k| keys.pressed(k)), now) {
            controls.thrust[axis] = sign;
            b |= BURST;
        }
    } else {
        tap.clear();
    }
    if controls.locked && !controls.swallow_click && mouse.pressed(MouseButton::Left) {
        b |= FIRE_PRIMARY;
    }
    if controls.locked && mouse.pressed(MouseButton::Right) {
        b |= FIRE_SECONDARY;
    }
    if keys.pressed(KeyCode::KeyF) {
        b |= MELEE;
    }
    // H: a toggled special flips MODE; the others are pressed.
    if let Some(own) = game.core.world.own {
        if frame(own.frame).special.is_toggle() {
            if keys.just_pressed(KeyCode::KeyH) {
                controls.mode = !controls.mode;
            }
        } else if keys.pressed(KeyCode::KeyH) {
            b |= SPECIAL;
        }
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
    // Salvage: B puts what's in hand in the hold, T throws it (and lets go of the grab), J dumps
    // the hold.
    if keys.pressed(KeyCode::KeyB) {
        b |= STOW;
    }
    if keys.pressed(KeyCode::KeyT) {
        b |= THROW;
    }
    if keys.pressed(KeyCode::KeyJ) {
        b |= JETTISON;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        controls.grab = false;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        controls.grab = !controls.grab;
    }
    controls.buttons = b;
    if keys.just_pressed(KeyCode::KeyV) {
        controls.flight_assist = !controls.flight_assist;
        // Loud: V is the camera key in other games, and flying unassisted by mistake is no small
        // thing.
        ui.toast(if controls.flight_assist {
            "FLIGHT ASSIST ON"
        } else {
            "FLIGHT ASSIST OFF: fully Newtonian (V)"
        });
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        controls.zero = !controls.zero;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        controls.grip = !controls.grip;
        let on_body = mover.footing != Footing::Free;
        ui.toast(match (controls.grip, on_body) {
            (true, _) => "GRIP ON: come in slow and close, and it lands you",
            (false, true) => "GRIP OFF: letting go",
            (false, false) => "GRIP OFF",
        });
    }
    let dead = game.core.world.own.is_some_and(|o| !o.alive);
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ];
    for (key, f) in digits.into_iter().zip(PLAYABLE_ORDER) {
        if dead && keys.just_pressed(key) {
            game.respawn_request = Some(f);
            // A reconnect brings back the frame being flown.
            pilot.frame = f;
        }
    }
}
