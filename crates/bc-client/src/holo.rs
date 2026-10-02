//! The chart's light (`chart.rs`): holograms of the bodies (`shaders/holo.wgsl`) and the deep
//! space behind them (`shaders/chart_sky.wgsl`), on the chart's own render layer.
//!
//! A hologram is a shell of light added over what's behind it, brightest where the view grazes it,
//! with no depth written, so the chart's lines and marks show through every body. Its colour is raw
//! HDR: it blooms on the tiers with bloom, and is tonemapped in the shader on the ones without.

use std::f32::consts::TAU;

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

const HOLO_SHADER: &str = "embedded://bc_client/shaders/holo.wgsl";
const SKY_SHADER: &str = "embedded://bc_client/shaders/chart_sky.wgsl";

pub struct HoloPlugin;

impl Plugin for HoloPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/holo.wgsl");
        embedded_asset!(app, "shaders/chart_sky.wgsl");
        app.add_plugins((
            MaterialPlugin::<HoloMaterial>::default(),
            MaterialPlugin::<ChartSkyMaterial>::default(),
        ));
    }
}

/// What a hologram is of: how the shader draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoloKind {
    Plain = 0,
    Colony = 1,
    Earth = 2,
    Moon = 3,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct HoloMaterial {
    #[uniform(0)]
    pub holo: HoloUniform,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct HoloUniform {
    /// rgb: the fill's colour (raw HDR); a: how much of it fills the shell.
    pub color: Vec4,
    /// rgb: the rim's colour; a: how tight the rim is.
    pub rim: Vec4,
    /// x: kind; y: seconds; z: brightness; w: scan band spacing (m).
    pub params: Vec4,
    /// xyz: direction to the Sun; w: the colony's first window's angle.
    pub sun: Vec4,
}

impl HoloMaterial {
    /// A hologram of `kind`, filled `fill` of `color` with a rim of `rim`, scanned every `bands` m.
    pub fn new(kind: HoloKind, color: Vec3, fill: f32, rim: Vec3, bands: f32, sun: Vec3) -> Self {
        Self {
            holo: HoloUniform {
                color: color.extend(fill),
                rim: rim.extend(2.6),
                params: Vec4::new(kind as u32 as f32, 0.0, 1.0, bands.max(1.0)),
                sun: sun.extend(0.0),
            },
        }
    }
}

impl Material for HoloMaterial {
    fn fragment_shader() -> ShaderRef {
        HOLO_SHADER.into()
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
        // Light from both faces, through everything.
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ChartSkyMaterial {
    #[uniform(0)]
    pub sky: ChartSkyUniform,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct ChartSkyUniform {
    /// xyz: direction to the Sun; w: seconds.
    pub sun: Vec4,
    /// xyz: the galaxy's plane normal; w: brightness.
    pub galaxy: Vec4,
}

impl Material for ChartSkyMaterial {
    fn vertex_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
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
        // Seen from inside, behind everything (the vertex shader pins it to the far plane).
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        Ok(())
    }
}

/// An open tube along X, `radius` round and `half` long each way, `around` segments round and
/// `along` along: uv.x runs round it (from +Y toward +Z, the colony's convention), uv.y along it.
pub fn tube(radius: f32, half: f32, around: u32, along: u32) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for j in 0..=along {
        let v = j as f32 / along as f32;
        let x = -half + 2.0 * half * v;
        for i in 0..=around {
            let u = i as f32 / around as f32;
            let (s, c) = (u * TAU).sin_cos();
            positions.push([x, radius * c, radius * s]);
            normals.push([0.0, c, s]);
            uvs.push([u, v]);
        }
    }
    let row = around + 1;
    for j in 0..along {
        for i in 0..around {
            let a = j * row + i;
            indices.extend_from_slice(&[a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}
