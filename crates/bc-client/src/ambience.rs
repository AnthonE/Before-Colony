//! Two touches that sell being in space: dust motes streaking past as you fly
//! (`shaders/dust.wgsl`), because empty space gives no other cue to your speed, and the Sun's
//! lens flare (`shaders/flare.wgsl`).

use bc_sim::math::Rng;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    VertexFormat,
};
use bevy::shader::ShaderRef;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::gfx::Gfx;
use crate::sky::{SUN_DIR, SUN_LUX, Sun};
use crate::view::VisTime;

const ATTRIBUTE_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("DustCorner", 0x5EC0_0010, VertexFormat::Float32x2);
const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
/// Dust box around the camera (m).
const DUST_BOX: f32 = 160.0;

pub struct AmbiencePlugin;

impl Plugin for AmbiencePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/dust.wgsl");
        embedded_asset!(app, "shaders/flare.wgsl");
        app.add_plugins((
            MaterialPlugin::<DustMaterial>::default(),
            MaterialPlugin::<FlareMaterial>::default(),
        ));
    }
}

fn glow_pipeline(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
    if let Some(depth) = descriptor.depth_stencil.as_mut() {
        depth.depth_write_enabled = Some(false);
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct DustMaterial {
    #[uniform(0)]
    dust: DustParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct DustParams {
    /// xyz: camera velocity (m/s); w: streak length in seconds of motion.
    velocity: Vec4,
    /// x: box size (m); y: mote radius (m); z: brightness.
    shape: Vec4,
}

impl Material for DustMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://bc_client/shaders/dust.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/dust.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
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
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_CORNER.at_shader_location(1),
        ])?];
        glow_pipeline(descriptor);
        Ok(())
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct FlareMaterial {
    #[uniform(0)]
    flare: FlareParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct FlareParams {
    /// xyz: direction to the Sun; w: intensity.
    sun: Vec4,
    /// x: height / width.
    aspect: Vec4,
}

impl Material for FlareMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://bc_client/shaders/flare.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/flare.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
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
        glow_pipeline(descriptor);
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_compare = Some(CompareFunction::Always);
        }
        Ok(())
    }
}

#[derive(Component)]
pub struct Dust(Handle<DustMaterial>);

#[derive(Component)]
pub struct Flare(Handle<FlareMaterial>);

/// Quads over `seeds` (one per mote), each corner carrying its seed as the position.
fn motes(seeds: &[Vec3]) -> Mesh {
    let mut pos = Vec::with_capacity(seeds.len() * 4);
    let mut corner = Vec::with_capacity(seeds.len() * 4);
    let mut idx = Vec::with_capacity(seeds.len() * 6);
    for (i, s) in seeds.iter().enumerate() {
        for c in CORNERS {
            pos.push(s.to_array());
            corner.push(c);
        }
        let b = i as u32 * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(ATTRIBUTE_CORNER, corner)
        .with_inserted_indices(Indices::U32(idx))
}

pub fn setup_ambience(
    mut commands: Commands,
    gfx: Res<Gfx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut dust: ResMut<Assets<DustMaterial>>,
    mut flares: ResMut<Assets<FlareMaterial>>,
) {
    let mut rng = Rng::new(0xD057);
    let n = gfx.settings.dust.max(1);
    let seeds: Vec<Vec3> =
        (0..n).map(|_| Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32())).collect();
    let material = dust.add(DustMaterial {
        dust: DustParams {
            velocity: Vec4::new(0.0, 0.0, 0.0, 1.0 / 20.0),
            shape: Vec4::new(DUST_BOX, 0.09, if gfx.settings.dust > 0 { 0.5 } else { 0.0 }, 0.0),
        },
    });
    commands.spawn((
        Dust(material.clone()),
        Mesh3d(meshes.add(motes(&seeds))),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    // Seven ghosts: each a quad whose z is its index.
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for g in 0..7u32 {
        for c in CORNERS {
            pos.push([c[0], c[1], g as f32]);
        }
        let b = g * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let flare_mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_indices(Indices::U32(idx));
    let flare = flares.add(FlareMaterial {
        flare: FlareParams { sun: SUN_DIR.extend(0.0), aspect: Vec4::new(9.0 / 16.0, 0.0, 0.0, 0.0) },
    });
    commands.spawn((
        Flare(flare.clone()),
        Mesh3d(meshes.add(flare_mesh)),
        MeshMaterial3d(flare),
        Transform::IDENTITY,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Feeds the dust the camera's velocity, and the flare how much of the Sun is in view.
#[allow(clippy::too_many_arguments)]
pub fn update_ambience(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    suns: Query<&DirectionalLight, With<Sun>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    dusts: Query<&Dust>,
    flares: Query<&Flare>,
    mut dust_materials: ResMut<Assets<DustMaterial>>,
    mut flare_materials: ResMut<Assets<FlareMaterial>>,
    mut last: Local<Option<Vec3>>,
) {
    let Ok(cam) = cams.single() else { return };
    let eye = cam.translation();
    let vel = match *last {
        Some(prev) if time.dt > 0.0 => (eye - prev) / time.dt,
        _ => Vec3::ZERO,
    };
    *last = Some(eye);
    for d in &dusts {
        if let Some(mut m) = dust_materials.get_mut(&d.0) {
            // Teleports (respawns, camera cuts) would streak across the whole box.
            m.dust.velocity = if vel.length() < 3_000.0 { vel.extend(1.0 / 20.0) } else { Vec4::ZERO };
        }
    }
    let facing = cam.forward().dot(SUN_DIR);
    let sunlight = suns.iter().next().map_or(1.0, |l| l.illuminance / SUN_LUX);
    let on = if gfx.settings.flare { 1.0 } else { 0.0 };
    let intensity = on * sunlight * ((facing - 0.55) / 0.35).clamp(0.0, 1.0);
    let aspect = windows.single().map_or(9.0 / 16.0, |w| w.height() / w.width().max(1.0));
    for f in &flares {
        if let Some(mut m) = flare_materials.get_mut(&f.0) {
            m.flare.sun.w = intensity;
            m.flare.aspect.x = aspect;
        }
    }
}
