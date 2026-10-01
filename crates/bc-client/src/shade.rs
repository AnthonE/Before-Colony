//! Contact shadows: soft darkening on the floor where things stand on it and along the foot of the
//! walls, the shade that bounced light can't reach. There's no screen-space ambient occlusion on
//! the web, so the hangar bay lays these down as quads just above its deck, blended by
//! multiplying what's already drawn (`shaders/shade.wgsl`): exposure, fog and lamps don't change
//! how dark they make it.

use bevy::asset::embedded_asset;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

pub struct ShadePlugin;

impl Plugin for ShadePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/shade.wgsl");
        app.add_plugins(MaterialPlugin::<ShadeMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
pub struct ShadeMaterial {}

impl Material for ShadeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/shade.wgsl".into()
    }

    // In the transparent pass, after everything solid is drawn; the blend is replaced below.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // What's behind, times the shade.
        let multiply = BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::Src,
            operation: BlendOperation::Add,
        };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState { color: multiply, alpha: BlendComponent::OVER });
            }
        }
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// A shade's `MeshTag`: how dark at its darkest (0..1) and how far its edge fades (m).
fn shade_tag(strength: f32, margin: f32) -> MeshTag {
    let s = (strength.clamp(0.0, 1.0) * 255.0) as u32;
    let m = (margin * 10.0).clamp(1.0, 255.0) as u32;
    MeshTag(s | m << 8)
}

/// The pieces a shade is made of.
#[derive(Resource, Clone)]
pub struct Shades {
    pub quad: Handle<Mesh>,
    pub material: Handle<ShadeMaterial>,
}

impl Shades {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<ShadeMaterial>) -> Self {
        Self {
            quad: meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(0.5))),
            material: materials.add(ShadeMaterial::default()),
        }
    }

    /// A shade on the floor under `parent`, centred at `at` (its local space; y is the floor), a
    /// rectangle of `size` (x, z) m whose edge fades out over `margin` m.
    pub fn spawn(
        &self,
        commands: &mut Commands,
        parent: Entity,
        at: Vec3,
        size: Vec2,
        margin: f32,
        strength: f32,
    ) {
        commands.spawn((
            Mesh3d(self.quad.clone()),
            MeshMaterial3d(self.material.clone()),
            shade_tag(strength, margin),
            Transform::from_translation(at + Vec3::Y * 0.02).with_scale(Vec3::new(size.x, 1.0, size.y)),
            NotShadowCaster,
            NotShadowReceiver,
            ChildOf(parent),
        ));
    }
}
