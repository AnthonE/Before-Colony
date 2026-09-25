//! Third-person chase camera looking along the pilot's aim, with G-strain gray-out and the ZERO
//! seizure's chromatic aberration. The tier's post-processing is attached by `gfx`.

use bevy::post_process::effect_stack::{ChromaticAberration, Vignette};
use bevy::prelude::*;

use crate::view::{CameraTarget, VisTime};

#[derive(Component)]
pub struct MainCamera;

pub fn spawn_camera(mut commands: Commands) {
    let mut cam = commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 70f32.to_radians(),
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

#[allow(clippy::type_complexity)]
pub fn follow(
    target: Res<CameraTarget>,
    time: Res<VisTime>,
    mut cam: Query<
        (&mut Transform, Option<&mut Vignette>, Option<&mut ChromaticAberration>),
        With<MainCamera>,
    >,
) {
    let Ok((mut tf, vignette, aberration)) = cam.single_mut() else { return };
    let Some(t) = target.0 else {
        // Before spawning: a slow establishing shot of the colony.
        let a = (time.now * 0.03) as f32;
        *tf = Transform::from_xyz(6_500.0 * a.cos(), 2_600.0, 6_500.0 * a.sin())
            .looking_at(Vec3::new(0.0, 600.0, 0.0), Vec3::Y);
        return;
    };
    let target_pos = t.pos - t.aim * 42.0 + t.up * 10.0;
    let k = 1.0 - (-time.dt * 14.0).exp();
    tf.translation = if tf.translation.distance(target_pos) > 500.0 {
        target_pos
    } else {
        tf.translation.lerp(target_pos, k)
    };
    tf.look_at(t.pos + t.aim * 800.0, t.up);
    if let Some(mut v) = vignette {
        v.intensity = (t.g_strain * 1.1).min(1.0);
        v.radius = 0.9 - 0.5 * t.g_strain;
    }
    if let Some(mut c) = aberration {
        c.intensity = if t.seized { 0.06 } else { 0.0 };
    }
}
