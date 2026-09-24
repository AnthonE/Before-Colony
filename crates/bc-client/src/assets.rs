//! Shared meshes and materials (created once; every suit reuses the same handles).

use bevy::prelude::*;

#[derive(Resource)]
pub struct Palette {
    pub white: Handle<StandardMaterial>,
    pub blue: Handle<StandardMaterial>,
    pub red: Handle<StandardMaterial>,
    pub yellow: Handle<StandardMaterial>,
    pub dark: Handle<StandardMaterial>,
    pub oz_green: Handle<StandardMaterial>,
    pub oz_grey: Handle<StandardMaterial>,
    pub taurus_white: Handle<StandardMaterial>,
    pub taurus_blue: Handle<StandardMaterial>,
    pub virgo_olive: Handle<StandardMaterial>,
    pub alliance_tan: Handle<StandardMaterial>,
    pub eye_green: Handle<StandardMaterial>,
    pub eye_pink: Handle<StandardMaterial>,
    pub thruster: Handle<StandardMaterial>,
    pub saber: Handle<StandardMaterial>,
    pub charge: Handle<StandardMaterial>,
    pub zero_aura: Handle<StandardMaterial>,
    pub beam_rifle: Handle<StandardMaterial>,
    pub beam_cannon: Handle<StandardMaterial>,
    pub buster: Handle<StandardMaterial>,
    pub tracer: Handle<StandardMaterial>,
    pub spark: Handle<StandardMaterial>,
    pub blast: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct MeshLib {
    pub cube: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    pub cone: Handle<Mesh>,
}

fn solid(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    metallic: f32,
    rough: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, metallic, perceptual_roughness: rough, ..default() })
}

/// Self-lit material. (Not `unlit`: Bevy's unlit path ignores `emissive`, and emissive values above
/// 1 are what drive the bloom.)
fn glow(materials: &mut Assets<StandardMaterial>, c: LinearRgba) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: c,
        perceptual_roughness: 1.0,
        ..default()
    })
}

pub fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let m = &mut *materials;
    commands.insert_resource(Palette {
        white: solid(m, Color::srgb(0.92, 0.93, 0.95), 0.3, 0.45),
        blue: solid(m, Color::srgb(0.12, 0.26, 0.72), 0.3, 0.5),
        red: solid(m, Color::srgb(0.78, 0.1, 0.1), 0.3, 0.5),
        yellow: solid(m, Color::srgb(0.95, 0.78, 0.12), 0.4, 0.4),
        dark: solid(m, Color::srgb(0.12, 0.13, 0.15), 0.6, 0.6),
        oz_green: solid(m, Color::srgb(0.30, 0.40, 0.30), 0.4, 0.55),
        oz_grey: solid(m, Color::srgb(0.46, 0.49, 0.52), 0.5, 0.5),
        taurus_white: solid(m, Color::srgb(0.84, 0.86, 0.9), 0.4, 0.45),
        taurus_blue: solid(m, Color::srgb(0.22, 0.35, 0.62), 0.4, 0.45),
        virgo_olive: solid(m, Color::srgb(0.42, 0.45, 0.30), 0.5, 0.5),
        alliance_tan: solid(m, Color::srgb(0.62, 0.52, 0.36), 0.4, 0.55),
        eye_green: glow(m, LinearRgba::rgb(0.2, 6.0, 1.2)),
        eye_pink: glow(m, LinearRgba::rgb(6.0, 0.4, 2.2)),
        thruster: glow(m, LinearRgba::rgb(2.5, 4.0, 12.0)),
        saber: glow(m, LinearRgba::rgb(14.0, 2.0, 7.0)),
        charge: glow(m, LinearRgba::rgb(12.0, 6.0, 14.0)),
        // A faint additive shell, so it never hides what's inside it.
        zero_aura: m.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.15, 0.2, 0.12),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
        beam_rifle: glow(m, LinearRgba::rgb(12.0, 9.0, 2.0)),
        beam_cannon: glow(m, LinearRgba::rgb(2.0, 12.0, 4.0)),
        buster: glow(m, LinearRgba::rgb(18.0, 10.0, 22.0)),
        tracer: glow(m, LinearRgba::rgb(10.0, 6.0, 1.5)),
        spark: glow(m, LinearRgba::rgb(14.0, 10.0, 5.0)),
        blast: glow(m, LinearRgba::rgb(16.0, 6.0, 1.6)),
    });
    commands.insert_resource(MeshLib {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap()),
        capsule: meshes.add(Capsule3d::new(0.5, 1.0)),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0)),
        cone: meshes.add(Cone { radius: 1.0, height: 1.0 }),
    });
}
