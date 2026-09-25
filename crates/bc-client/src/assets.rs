//! Shared meshes and glow materials (created once; every suit reuses the same handles). Painted
//! surfaces use `materials::HullMaterial`.

use bevy::prelude::*;

use crate::blast::ShockMaterial;

#[derive(Resource)]
pub struct Palette {
    pub eye_green: Handle<StandardMaterial>,
    pub eye_pink: Handle<StandardMaterial>,
    /// A red shell glowing at its rim, round a suit under the ZERO System.
    pub zero_aura: Handle<ShockMaterial>,
}

#[derive(Resource)]
pub struct MeshLib {
    pub cube: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
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
    mut shells: ResMut<Assets<ShockMaterial>>,
) {
    let m = &mut *materials;
    commands.insert_resource(Palette {
        eye_green: glow(m, LinearRgba::rgb(0.2, 6.0, 1.2)),
        eye_pink: glow(m, LinearRgba::rgb(6.0, 0.4, 2.2)),
        zero_aura: shells.add(ShockMaterial::new(Vec3::new(2.4, 0.25, 0.45), 2.5)),
    });
    commands.insert_resource(MeshLib {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap()),
        capsule: meshes.add(Capsule3d::new(0.5, 1.0)),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0)),
    });
}
