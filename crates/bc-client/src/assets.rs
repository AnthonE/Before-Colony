//! Shared meshes and glow materials (created once; every suit reuses the same handles). Painted
//! surfaces use `materials::HullMaterial`.

use bevy::prelude::*;

#[derive(Resource)]
pub struct Palette {
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
