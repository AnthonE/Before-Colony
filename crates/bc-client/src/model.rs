//! Every frame's suit meshes, baked once at startup from `bc_model`'s designs: one mesh per bone,
//! per frame, per level of detail, all drawn with the shared hull material.

use std::collections::HashMap;

use bc_model::kit::MeshData;
use bc_model::rig::BONES;
use bc_model::{LODS, Lod, Sockets};
use bc_proto::FrameId;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// One frame at one level of detail: a mesh per bone, in `Bone` order (None where the bone has
/// nothing).
pub struct FrameModel {
    pub bones: [Option<Handle<Mesh>>; BONES],
}

/// Every frame's meshes and sockets.
#[derive(Resource)]
pub struct SuitMeshLib {
    models: HashMap<(FrameId, Lod), FrameModel>,
    sockets: HashMap<FrameId, Sockets>,
}

impl SuitMeshLib {
    pub fn model(&self, frame: FrameId, lod: Lod) -> &FrameModel {
        &self.models[&(frame, lod)]
    }

    pub fn sockets(&self, frame: FrameId) -> &Sockets {
        &self.sockets[&frame]
    }
}

fn mesh(m: MeshData) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// Builds every frame's meshes (a few milliseconds).
pub fn build_suit_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut models = HashMap::new();
    let mut sockets = HashMap::new();
    for frame in FrameId::ALL {
        for lod in LODS {
            let model = bc_model::build(frame, lod);
            let mut bones: [Option<Handle<Mesh>>; BONES] = std::array::from_fn(|_| None);
            for (slot, m) in bones.iter_mut().zip(model.bones) {
                *slot = m.map(|m| meshes.add(mesh(m)));
            }
            models.insert((frame, lod), FrameModel { bones });
            if lod == Lod::Near {
                sockets.insert(frame, model.sockets);
            }
        }
    }
    commands.insert_resource(SuitMeshLib { models, sockets });
}
