//! Salvage on screen: every chunk the server tells this client about (loose ore, limbs shot off,
//! hulks) drawn with the rocks' and suits' own meshes, where its segment, or its holder's hand,
//! puts it at the drawn moment. A hulk stays hidden while its suit is still on show as a wreck.

use std::collections::HashMap;

use bc_client_core::world::ObjectTrack;
use bc_model::Lod;
use bc_model::rig::{self, Bone};
use bc_proto::ChunkKind;
use bc_sim::chunks;
use bc_sim::content::frame;
use bc_sim::field::SHAPES;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::materials::{HullTag, Surfaces, rock_tag};
use crate::model::SuitMeshLib;
use crate::net::GameClient;
use crate::rocks::RockMeshes;
use crate::suits_vis::livery;
use crate::view::VisTime;

/// Chunks switch to the far suit meshes beyond this distance (m), and back within `LOD_NEAR`.
const LOD_FAR: f32 = 450.0;
const LOD_NEAR: f32 = 350.0;

/// A chunk on screen.
#[derive(Component)]
pub struct ChunkVisual {
    generation: u8,
    kind: ChunkKind,
    /// Its bones (limbs and hulks), to swap meshes by distance.
    bones: Vec<(Entity, Bone)>,
    lod: Lod,
}

/// Which bones make up a chunk: a limb's part, or every part still on a hulk.
fn bones_of(kind: ChunkKind) -> impl Iterator<Item = Bone> {
    rig::ALL.into_iter().filter(move |b| match kind {
        ChunkKind::Ore { .. } => false,
        ChunkKind::Limb { part, .. } => b.def().part == part,
        ChunkKind::Hulk { parts, .. } => parts & (1 << b.def().part as u8) != 0,
    })
}

/// Builds a chunk's visual at `tf`.
pub fn spawn_chunk(
    commands: &mut Commands,
    track: &ObjectTrack,
    tf: Transform,
    lib: &SuitMeshLib,
    rocks: &RockMeshes,
    surfaces: &Surfaces,
) -> Entity {
    let root = commands.spawn((tf, Visibility::default())).id();
    let seed = track.desc.seed;
    let kind = track.desc.kind;
    let mut bones = Vec::new();
    // A limb's pose is its part's middle; a hulk's is its suit's.
    let (f, faction, origin) = match kind {
        ChunkKind::Ore { ore } => {
            let r = chunks::radius(&track.desc);
            let s = f32::from(seed) / 255.0;
            commands.spawn((
                Mesh3d(rocks.coarse(usize::from(seed) % usize::from(SHAPES))),
                MeshMaterial3d(surfaces.rock.clone()),
                rock_tag(ore, seed),
                Transform::from_scale(Vec3::new(r * (0.85 + 0.3 * s), r * (1.1 - 0.3 * s), r)),
                ChildOf(root),
            ));
            commands.entity(root).insert(ChunkVisual {
                generation: track.generation,
                kind,
                bones,
                lod: Lod::Far,
            });
            return root;
        }
        ChunkKind::Limb { frame: f, faction, part } => {
            let c = frame(f).capsules[part as usize];
            (f, faction, (c.a + c.b) * 0.5)
        }
        ChunkKind::Hulk { frame: f, faction, .. } => (f, faction, Vec3::ZERO),
    };
    let (body, trim, accent, eye) = livery(f, faction);
    let tag = HullTag { armour: 1, wreck: true, ..HullTag::livery(body, trim, accent, eye, seed) };
    let model = lib.model(f, Lod::Near);
    for b in bones_of(kind) {
        let Some(mesh) = &model.bones[b.index()] else { continue };
        let e = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(surfaces.armour.clone()),
                tag.tag(),
                Transform::from_translation(b.def().joint - origin),
                ChildOf(root),
            ))
            .id();
        bones.push((e, b));
    }
    commands.entity(root).insert(ChunkVisual { generation: track.generation, kind, bones, lod: Lod::Near });
    root
}

/// Keeps a visual for every chunk this client knows of, posed at the drawn moment.
#[allow(clippy::too_many_arguments)]
pub fn sync_chunks(
    mut commands: Commands,
    game: NonSend<GameClient>,
    vis: Res<VisTime>,
    lib: Res<SuitMeshLib>,
    rocks: Res<RockMeshes>,
    surfaces: Res<Surfaces>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut shown: Local<HashMap<u16, Entity>>,
    mut visuals: Query<(&mut ChunkVisual, &mut Transform, &mut Visibility)>,
    mut meshes: Query<&mut Mesh3d>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let t = core.render_tick(vis.now);
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    // Visuals for chunks that are gone, or are now something else, go.
    shown.retain(|id, e| {
        let same = world.objects[*id as usize].as_ref().is_some_and(|track| {
            visuals.get(*e).is_ok_and(|(v, ..)| v.generation == track.generation && v.kind == track.desc.kind)
        });
        if !same {
            commands.entity(*e).despawn();
        }
        same
    });
    for (id, track) in world.objects.iter().enumerate() {
        let Some(track) = track else { continue };
        let id = id as u16;
        let Some((pos, rot)) = world.object_pose(id, t, &core.predict) else { continue };
        let on = !world.wreck_on_show(id);
        let Some(&e) = shown.get(&id) else {
            let tf = Transform::from_translation(pos).with_rotation(rot);
            let e = spawn_chunk(&mut commands, track, tf, &lib, &rocks, &surfaces);
            if !on {
                commands.entity(e).insert(Visibility::Hidden);
            }
            shown.insert(id, e);
            continue;
        };
        let Ok((mut v, mut tf, mut visibility)) = visuals.get_mut(e) else { continue };
        tf.translation = pos;
        tf.rotation = rot;
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != want {
            *visibility = want;
        }
        // Far meshes beyond a few hundred metres.
        let d = pos.distance(eye);
        let lod = match v.lod {
            Lod::Near if d > LOD_FAR => Lod::Far,
            Lod::Far if d < LOD_NEAR => Lod::Near,
            other => other,
        };
        if lod != v.lod {
            v.lod = lod;
            if let ChunkKind::Limb { frame: f, .. } | ChunkKind::Hulk { frame: f, .. } = v.kind {
                let model = lib.model(f, lod);
                for &(bone_e, b) in &v.bones {
                    if let (Ok(mut m), Some(mesh)) = (meshes.get_mut(bone_e), &model.bones[b.index()]) {
                        m.0 = mesh.clone();
                    }
                }
            }
        }
    }
}
