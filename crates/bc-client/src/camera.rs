//! Third-person chase camera looking along the pilot's aim, and the post effects that stand in for
//! the pilot's body:
//! - a critically damped spring holds the camera behind the suit in the suit's moving frame, so it
//!   sits still at any cruising speed and swings only with acceleration and turns;
//! - the field of view widens on boost, and blasts, hits and the Twin Buster Rifle shake it;
//! - G-strain greys the view out and closes it to a tunnel, and a blackout (G-LOC) takes it to
//!   black;
//! - a hit flashes the edges red;
//! - ZERO tints the view while it's engaged (`zero_vision`); a seizure warps, fringes and tears it.
//!
//! `?calm=1`, or the browser's reduced-motion setting, turns the shake, kicks and warps down.

use bc_proto::WeaponKind;
use bevy::post_process::effect_stack::{ChromaticAberration, LensDistortion, Vignette};
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use crate::net::LaunchConfigRes;
use crate::view::{CameraTarget, FxEvent, FxEvents, VisTime};
use crate::zero_vision::ZeroVision;

#[derive(Component)]
pub struct MainCamera;

/// Field of view (degrees) cruising, and on boost.
const FOV: f32 = 70.0;
const BOOST_FOV: f32 = 77.0;
/// The chase spring (rad/s): the camera trails by acceleration / ω², about 5 m under 10 g.
const OMEGA: f32 = 4.5;
/// The furthest the spring lets the camera stray from its place behind the suit (m).
const SLACK: f32 = 25.0;

/// The chase camera and the pilot effects, between frames.
#[derive(Resource, Clone, Copy, Default)]
pub struct Chase {
    placed: bool,
    /// The camera just cut (spawn, respawn, teleport): eased effects start where they belong.
    cut: bool,
    pos: Vec3,
    vel: Vec3,
    fov: f32,
    /// Shake, 0..1: kicked by events, decaying; the camera shakes by its square.
    trauma: f32,
    /// The red flash of a hit, 0..1.
    flash: f32,
    /// Blackout, ZERO engaged and seizure, each eased 0..1.
    black: f32,
    zero: f32,
    seizure: f32,
}

pub fn spawn_camera(mut commands: Commands) {
    let mut cam = commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV.to_radians(),
            near: 0.5,
            far: 2_000_000.0,
            ..default()
        }),
        Transform::from_xyz(-6_000.0, 2_500.0, -6_000.0).looking_at(Vec3::new(0.0, 900.0, 0.0), Vec3::Y),
    ));
    // A fill light that travels with the camera, so the pilot's own suit (and anything close) reads
    // against black space. A point light, because WebGL2 allows one directional light: the sun.
    cam.with_children(|c| {
        c.spawn((
            PointLight {
                // About 2,500 lux on the own suit, 43 m ahead: E = Φ / (4π d²).
                intensity: 6.0e7,
                range: 600.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::default(),
        ));
    });
}

/// How much a reduced-motion setting leaves of shakes, kicks and warps.
fn motion(cfg: &LaunchConfigRes) -> f32 {
    if cfg.0.calm { 0.25 } else { 1.0 }
}

/// Places the chase camera, shakes it and sets its field of view.
#[allow(clippy::type_complexity)]
pub fn follow(
    target: Res<CameraTarget>,
    time: Res<VisTime>,
    events: Res<FxEvents>,
    cfg: Res<LaunchConfigRes>,
    mut chase: ResMut<Chase>,
    mut cam: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut tf, mut projection)) = cam.single_mut() else { return };
    let c = &mut *chase;
    let dt = time.dt.min(0.1);
    let Some(t) = target.0 else {
        // Before spawning: a slow establishing shot of the colony.
        let a = (time.now * 0.03) as f32;
        *tf = Transform::from_xyz(6_500.0 * a.cos(), 2_600.0, 6_500.0 * a.sin())
            .looking_at(Vec3::new(0.0, 600.0, 0.0), Vec3::Y);
        c.placed = false;
        return;
    };

    // Behind and above the suit, on a spring in the suit's moving frame: coast at the camera's own
    // velocity, then pull toward the ideal place and the suit's velocity.
    let ideal = t.pos - t.aim * 42.0 + t.up * 10.0;
    let coast = c.pos + c.vel * dt;
    if !c.placed || coast.distance(ideal) > 300.0 {
        // Spawning, respawning or a teleport: cut.
        let fov = if t.boost { FOV + (BOOST_FOV - FOV) * motion(&cfg) } else { FOV };
        *c = Chase { placed: true, cut: true, pos: ideal, vel: t.vel, fov, ..*c };
    } else {
        let x = coast - ideal;
        let a = -OMEGA * OMEGA * x - 2.0 * OMEGA * (c.vel - t.vel);
        c.vel += a * dt;
        c.pos = ideal + (x + a * dt * dt).clamp_length_max(SLACK);
    }

    // Shake from blasts, hits and big guns nearby.
    let eye = c.pos;
    let near =
        |p: Vec3, full: f32, none: f32| 1.0 - ((p.distance(eye) - full) / (none - full)).clamp(0.0, 1.0);
    for ev in &events.0 {
        c.trauma += match *ev {
            FxEvent::Kill { pos } => 0.9 * near(pos, 120.0, 1_500.0),
            FxEvent::Struck { weapon } => {
                c.flash = 1.0;
                if weapon == WeaponKind::TwinBusterRifle { 0.9 } else { 0.35 }
            }
            FxEvent::Muzzle { pos, weapon: WeaponKind::TwinBusterRifle, .. } => 0.6 * near(pos, 60.0, 900.0),
            FxEvent::Clash { pos } => 0.4 * near(pos, 40.0, 400.0),
            FxEvent::RockBreak { pos, radius, .. } => 0.7 * near(pos, radius * 2.0, radius * 40.0),
            _ => 0.0,
        };
    }
    c.trauma = (c.trauma.min(1.0) - 0.9 * dt).max(0.0);
    c.flash = (c.flash - 2.5 * dt).max(0.0);

    tf.translation = c.pos;
    tf.look_at(t.pos + t.aim * 800.0, t.up);
    let shake = c.trauma * c.trauma * motion(&cfg);
    if shake > 0.0 {
        // Smooth pseudo-noise per axis: two incommensurate sines.
        let n = |f: f64, p: f64| {
            ((time.now * f + p).sin() * 0.6 + (time.now * f * 2.31 + p * 1.7).sin() * 0.4) as f32
        };
        let (yaw, pitch, roll) = (n(21.0, 0.0) * 0.03, n(17.0, 2.0) * 0.03, n(13.0, 4.0) * 0.05);
        tf.rotate_local(Quat::from_euler(EulerRot::YXZ, yaw * shake, pitch * shake, roll * shake));
    }

    // Wider on boost.
    let want = if t.boost { FOV + (BOOST_FOV - FOV) * motion(&cfg) } else { FOV };
    c.fov += (want - c.fov) * (1.0 - (-4.0 * dt).exp());
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = c.fov.to_radians();
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The pilot's body on the picture: G-strain greying the view out, tunnel vision and blackout; a
/// red flash when hit; ZERO's vision, and its seizure's warp and colour fringes. On tiers with
/// post effects (the components are there only then).
#[allow(clippy::type_complexity)]
pub fn pilot_effects(
    target: Res<CameraTarget>,
    time: Res<VisTime>,
    cfg: Res<LaunchConfigRes>,
    mut chase: ResMut<Chase>,
    mut cam: Query<
        (
            Option<&mut Vignette>,
            Option<&mut ChromaticAberration>,
            Option<&mut LensDistortion>,
            Option<&mut ColorGrading>,
            Option<&mut ZeroVision>,
        ),
        With<MainCamera>,
    >,
) {
    let Ok((vignette, aberration, lens, grading, zero)) = cam.single_mut() else { return };
    let dt = time.dt.min(0.1);
    let calm = motion(&cfg);
    let t = target.0.unwrap_or_default();
    let c = &mut *chase;
    let cut = std::mem::take(&mut c.cut);
    let ease =
        |x: &mut f32, to: f32, rate: f32| *x += (to - *x) * if cut { 1.0 } else { 1.0 - (-rate * dt).exp() };
    // Blacking out takes a moment; coming back takes longer.
    ease(&mut c.black, if t.blackout { 1.0 } else { 0.0 }, if t.blackout { 3.0 } else { 0.8 });
    ease(&mut c.zero, if t.zero || t.seized { 1.0 } else { 0.0 }, 2.0);
    ease(&mut c.seizure, if t.seized { 1.0 } else { 0.0 }, 5.0);

    let grey = smoothstep(0.35, 0.85, t.g_strain).max(c.black);
    let tunnel = smoothstep(0.6, 1.0, t.g_strain).max(c.black);
    if let Some(mut v) = vignette {
        let red = c.flash * c.flash;
        v.intensity = (0.1 * t.g_strain + 0.9 * tunnel).max(0.5 * red).min(1.0);
        v.radius = 1.05 - 0.85 * tunnel;
        v.smoothness = 2.5;
        // Black for the tunnel; red while a hit's flash outweighs it.
        let share = red / (red + tunnel).max(1e-3);
        v.color = Color::linear_rgb(0.35 * share, 0.01 * share, 0.015 * share);
    }
    if let Some(mut g) = grading {
        g.global.post_saturation = 1.0 - 0.85 * grey;
        g.global.exposure = -1.2 * grey - 8.0 * c.black * c.black;
    }
    if let Some(mut a) = aberration {
        a.intensity = (0.05 * c.seizure + 0.008 * c.zero * t.zero_strain) * calm;
    }
    if let Some(mut l) = lens {
        // In a seizure the view breathes in and out.
        let pulse = (time.now * 2.4).sin() as f32;
        l.intensity = c.seizure * calm * (0.1 + 0.12 * pulse);
        l.scale = 1.0 + 0.2 * l.intensity.abs();
    }
    if let Some(mut z) = zero {
        z.engaged = c.zero;
        z.strain = t.zero_strain;
        z.seizure = c.seizure * calm;
        z.time = (time.now % 1_000.0) as f32;
    }
}
