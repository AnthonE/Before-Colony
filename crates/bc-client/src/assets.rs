//! Shared suit assets that aren't armour (armour is `model`'s meshes in `materials::HullMaterial`).

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::blast::ShockMaterial;

#[derive(Resource)]
pub struct Palette {
    /// A red shell glowing at its rim, round a suit under the ZERO System.
    pub zero_aura: Handle<ShockMaterial>,
    /// A cold shimmer round a jamming Deathscythe, seen by its side (its enemies see nothing).
    pub jammer: Handle<ShockMaterial>,
    /// The soft dark under a suit on a body: its contact shadow, on every graphics tier.
    pub contact: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct MeshLib {
    pub sphere: Handle<Mesh>,
    /// Radius 1, 1 long along y, centred.
    pub cylinder: Handle<Mesh>,
    /// Radius 1 in the xz plane, facing +y: dark in the middle, fading out to its edge.
    pub disc: Handle<Mesh>,
}

/// A flat disc of radius 1 facing +y, its vertex colours black and fading from `alpha` in the
/// middle to nothing at the edge.
fn shadow_disc(alpha: f32, sectors: u32) -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    let mut colours = vec![[0.0, 0.0, 0.0, alpha]];
    for ring in [0.55f32, 1.0] {
        for s in 0..sectors {
            let a = TAU * s as f32 / sectors as f32;
            positions.push([ring * a.cos(), 0.0, ring * a.sin()]);
            colours.push([0.0, 0.0, 0.0, if ring < 1.0 { alpha * 0.6 } else { 0.0 }]);
        }
    }
    let mut indices = Vec::new();
    for s in 0..sectors {
        let (a, b) = (1 + s, 1 + (s + 1) % sectors);
        indices.extend([0, b, a]);
        let (c, d) = (a + sectors, b + sectors);
        indices.extend([a, b, d, a, d, c]);
    }
    let n = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
}

pub fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shells: ResMut<Assets<ShockMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(Palette {
        zero_aura: shells.add(ShockMaterial::new(Vec3::new(2.4, 0.25, 0.45), 2.5)),
        jammer: shells.add(ShockMaterial::new(Vec3::new(0.12, 0.34, 0.75), 7.0)),
        contact: standard.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    });
    commands.insert_resource(MeshLib {
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(3).expect("icosphere")),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(8)),
        disc: meshes.add(shadow_disc(0.6, 24)),
    });
}
