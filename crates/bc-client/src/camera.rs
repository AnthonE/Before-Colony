//! Third-person chase camera looking along the pilot's aim, with G-strain gray-out and the ZERO
//! seizure's chromatic aberration.

use bc_proto::snapshot::zero_mode;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::post_process::effect_stack::{ChromaticAberration, Vignette};
use bevy::prelude::*;

use crate::input::Aim;
use crate::net::{GameClient, LaunchConfigRes};

#[derive(Component)]
pub struct MainCamera;

pub fn spawn_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>, cfg: Res<LaunchConfigRes>) {
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
        crate::scene::skybox(&mut images),
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
    if cfg.0.low_quality {
        // Weak GPUs: no multisampling and no post-processing.
        cam.insert(Msaa::Off);
    } else {
        cam.insert((
            bevy::camera::Hdr,
            Tonemapping::TonyMcMapface,
            Bloom::NATURAL,
            Vignette { intensity: 0.0, ..default() },
            ChromaticAberration { intensity: 0.0, ..default() },
        ));
    }
}

#[allow(clippy::type_complexity)]
pub fn follow(
    game: NonSend<GameClient>,
    aim: Res<Aim>,
    time: Res<Time>,
    mut cam: Query<
        (&mut Transform, Option<&mut Vignette>, Option<&mut ChromaticAberration>),
        With<MainCamera>,
    >,
) {
    let Ok((mut tf, vignette, aberration)) = cam.single_mut() else { return };
    let game = game.borrow();
    let core = &game.core;
    let Some(own) = core.world.own else {
        // Before spawning: a slow establishing shot of the colony.
        let a = time.elapsed_secs() * 0.03;
        *tf = Transform::from_xyz(6_500.0 * a.cos(), 2_600.0, 6_500.0 * a.sin())
            .looking_at(Vec3::new(0.0, 600.0, 0.0), Vec3::Y);
        return;
    };
    let base = if own.alive { core.predict.render_pos() } else { own.pos };
    let up = core.predict.state.rot * Vec3::Y;
    let dir = aim.dir;
    let target_pos = base - dir * 42.0 + up * 10.0;
    let k = 1.0 - (-time.delta_secs() * 14.0).exp();
    tf.translation = if tf.translation.distance(target_pos) > 500.0 {
        target_pos
    } else {
        tf.translation.lerp(target_pos, k)
    };
    tf.look_at(base + dir * 800.0, up);
    if let Some(mut v) = vignette {
        let strain = own.g_strain.clamp(0.0, 1.0);
        v.intensity = (strain * 1.1).min(1.0);
        v.radius = 0.9 - 0.5 * strain;
    }
    if let Some(mut c) = aberration {
        c.intensity = if own.zero_mode == zero_mode::SEIZED { 0.06 } else { 0.0 };
    }
}
