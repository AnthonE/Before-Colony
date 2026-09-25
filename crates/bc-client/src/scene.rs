//! The L1 Colony Cluster: the colony cylinder and a debris field. Everything is generated (no asset
//! pipeline). The sky, the Sun, Earth and the Moon are in `sky`.

use bc_sim::math::Rng;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::prelude::*;

use crate::assets::MeshLib;

pub fn setup_scene(
    mut commands: Commands,
    lib: Res<MeshLib>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The colony: an O'Neill cylinder lying along X.
    let hull = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.57, 0.6),
        metallic: 0.6,
        perceptual_roughness: 0.4,
        ..default()
    });
    let windows = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(1.8, 2.2, 3.0),
        ..default()
    });
    let axis = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(COLONY_RADIUS, COLONY_HALF_LENGTH * 2.0).mesh().resolution(96))),
        MeshMaterial3d(hull.clone()),
        Transform::from_translation(COLONY_CENTER).with_rotation(axis),
    ));
    for k in 0..3 {
        let angle = k as f32 * std::f32::consts::TAU / 3.0 + 0.4;
        let dir = Vec3::new(0.0, angle.cos(), angle.sin());
        commands.spawn((
            Mesh3d(lib.cube.clone()),
            MeshMaterial3d(windows.clone()),
            Transform::from_translation(COLONY_CENTER + dir * (COLONY_RADIUS + 6.0))
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir))
                .with_scale(Vec3::new(COLONY_HALF_LENGTH * 1.8, 4.0, 700.0)),
        ));
    }
    // Mirror struts at the sunward cap.
    for k in 0..3 {
        let angle = k as f32 * std::f32::consts::TAU / 3.0;
        let dir = Vec3::new(0.0, angle.cos(), angle.sin());
        commands.spawn((
            Mesh3d(lib.cube.clone()),
            MeshMaterial3d(hull.clone()),
            Transform::from_translation(
                COLONY_CENTER
                    + Vec3::new(COLONY_HALF_LENGTH + 2_500.0, 0.0, 0.0)
                    + dir * (COLONY_RADIUS + 1_600.0),
            )
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir) * Quat::from_rotation_z(0.6))
            .with_scale(Vec3::new(6_000.0, 30.0, 2_600.0)),
        ));
    }
    // Debris field: rocks and wreckage around the combat zone (visual only).
    let rock = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.29, 0.26),
        perceptual_roughness: 0.95,
        ..default()
    });
    let mut rng = Rng::new(0xDEB12);
    for _ in 0..160 {
        let dir = Vec3::new(rng.signed(), rng.signed() * 0.5, rng.signed()).normalize_or(Vec3::X);
        let r = 1_200.0 + rng.next_f32() * 6_000.0;
        let pos = Vec3::new(0.0, 900.0, 0.0) + dir * r;
        let s = 4.0 + rng.next_f32() * rng.next_f32() * 60.0;
        commands.spawn((
            Mesh3d(lib.sphere.clone()),
            MeshMaterial3d(rock.clone()),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    rng.signed() * 3.0,
                    rng.signed() * 3.0,
                    rng.signed() * 3.0,
                ))
                .with_scale(Vec3::new(s * (0.6 + rng.next_f32()), s * (0.5 + rng.next_f32() * 0.6), s)),
        ));
    }
}
