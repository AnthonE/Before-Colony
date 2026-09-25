//! The L1 Colony Cluster: a procedural starfield, the sun, the colony cylinder, distant Earth and a
//! debris field. Everything is generated (no asset pipeline).

use bc_sim::math::Rng;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::asset::RenderAssetUsages;
use bevy::light::Skybox;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use crate::assets::MeshLib;

/// Direction the sunlight comes *from*.
pub const SUN_DIR: Vec3 = Vec3::new(0.55, 0.35, -0.76);

/// A cubemap of stars (6 faces stacked, then reinterpreted as a cube).
pub fn starfield(images: &mut Assets<Image>) -> Handle<Image> {
    const N: usize = 256;
    let mut data = vec![0u8; N * N * 6 * 4];
    let mut rng = Rng::new(0x5_7A25);
    for face in 0..6 {
        for _ in 0..900 {
            let x = (rng.next_u32() as usize) % N;
            let y = (rng.next_u32() as usize) % N;
            let b = rng.next_f32();
            let v = (40.0 + 215.0 * b * b * b) as u8;
            let tint = rng.next_f32();
            let (r, g, bl) = if tint < 0.1 {
                (v, v / 2 + 60, v / 3 + 40)
            } else if tint < 0.2 {
                (v / 2 + 60, v / 2 + 70, v)
            } else {
                (v, v, v)
            };
            let i = ((face * N + y) * N + x) * 4;
            data[i..i + 4].copy_from_slice(&[r, g, bl, 255]);
        }
    }
    for px in data.chunks_mut(4) {
        px[3] = 255;
    }
    let mut image = Image::new(
        Extent3d { width: N as u32, height: (N * 6) as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.reinterpret_stacked_2d_as_array(6).expect("6 faces");
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    images.add(image)
}

pub fn skybox(images: &mut Assets<Image>) -> Skybox {
    Skybox { image: Some(starfield(images)), brightness: 600.0, ..default() }
}

pub fn setup_scene(
    mut commands: Commands,
    lib: Res<MeshLib>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Space is harsh, but a pilot has to see the suits' night side.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.55, 0.62, 0.85),
        brightness: 380.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(-SUN_DIR, Vec3::Y),
    ));
    // The sun itself: a small, very bright disc far away (bloom does the rest).
    commands.spawn((
        Mesh3d(lib.sphere.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(60.0, 52.0, 40.0),
            ..default()
        })),
        Transform::from_translation(SUN_DIR * 400_000.0).with_scale(Vec3::splat(3_500.0)),
    ));
    // Earth, far below and behind.
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(64, 32))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.14, 0.32, 0.72),
            emissive: LinearRgba::rgb(0.02, 0.06, 0.16),
            perceptual_roughness: 0.7,
            ..default()
        })),
        Transform::from_xyz(-180_000.0, -260_000.0, 420_000.0).with_scale(Vec3::splat(160_000.0)),
    ));
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
