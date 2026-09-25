//! Shared suit assets that aren't armour (armour is `model`'s meshes in `materials::HullMaterial`).

use bevy::prelude::*;

use crate::blast::ShockMaterial;

#[derive(Resource)]
pub struct Palette {
    /// A red shell glowing at its rim, round a suit under the ZERO System.
    pub zero_aura: Handle<ShockMaterial>,
    /// A cold shimmer round a jamming Deathscythe, seen by its side (its enemies see nothing).
    pub jammer: Handle<ShockMaterial>,
}

#[derive(Resource)]
pub struct MeshLib {
    pub sphere: Handle<Mesh>,
    /// Radius 1, 1 long along y, centred.
    pub cylinder: Handle<Mesh>,
}

pub fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shells: ResMut<Assets<ShockMaterial>>,
) {
    commands.insert_resource(Palette {
        zero_aura: shells.add(ShockMaterial::new(Vec3::new(2.4, 0.25, 0.45), 2.5)),
        jammer: shells.add(ShockMaterial::new(Vec3::new(0.12, 0.34, 0.75), 7.0)),
    });
    commands.insert_resource(MeshLib {
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(3).expect("icosphere")),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(8)),
    });
}
