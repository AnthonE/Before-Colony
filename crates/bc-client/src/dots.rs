//! Small lights by the thousand, in one mesh a group (`shaders/light_dots.wgsl`): the dotted rows of
//! lamps that outline the colony's structure from far off. Each light keeps a least size on screen,
//! dimmed as it's spread, and may blink. They glow (raw HDR, like the effects), so they bloom on the
//! tiers with bloom and are tonemapped in the shader on the ones without.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues,
};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;

use crate::view::VisTime;

const SHADER: &str = "embedded://bc_client/shaders/light_dots.wgsl";
const ATTRIBUTE_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("DotCorner", 0x5EC0_0101, VertexFormat::Float32x2);
const ATTRIBUTE_SHAPE: MeshVertexAttribute =
    MeshVertexAttribute::new("DotShape", 0x5EC0_0102, VertexFormat::Float32x4);
const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
/// The least diameter a light keeps on screen, px.
const LEAST_PX: f32 = 2.2;

pub struct DotsPlugin;

impl Plugin for DotsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/light_dots.wgsl");
        app.add_plugins(MaterialPlugin::<DotMaterial>::default())
            .init_resource::<DotLook>()
            .add_systems(Update, tick_dots);
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
pub struct DotMaterial {
    #[uniform(0)]
    params: DotParams,
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct DotParams {
    /// x: seconds; y: least diameter on screen (px); z: brightness; w: unused.
    p: Vec4,
}

impl Material for DotMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
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
            Mesh::ATTRIBUTE_COLOR.at_shader_location(2),
            ATTRIBUTE_SHAPE.at_shader_location(3),
        ])?];
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// The one material every group of lights shares (its clock and least size).
#[derive(Resource, Default)]
pub struct DotLook(pub Option<Handle<DotMaterial>>);

impl DotLook {
    /// The shared material, made the first time it's asked for.
    pub fn get(&mut self, materials: &mut Assets<DotMaterial>) -> Handle<DotMaterial> {
        self.0
            .get_or_insert_with(|| {
                materials.add(DotMaterial { params: DotParams { p: Vec4::new(0.0, LEAST_PX, 1.0, 0.0) } })
            })
            .clone()
    }
}

fn tick_dots(time: Res<VisTime>, look: Res<DotLook>, mut materials: ResMut<Assets<DotMaterial>>) {
    if let Some(mut m) = look.0.as_ref().and_then(|h| materials.get_mut(h)) {
        m.params.p.x = (time.now % 86_400.0) as f32;
    }
}

/// How a light shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blink {
    Steady,
    /// On for `lit` of every `period` seconds, starting `phase` (0..1) of the way in.
    Every {
        period: f32,
        phase: f32,
        lit: f32,
    },
}

/// A group of lights being laid out, in its entity's space.
#[derive(Default)]
pub struct Dots {
    centre: Vec<[f32; 3]>,
    corner: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    shape: Vec<[f32; 4]>,
}

impl Dots {
    /// One light at `at`, `radius` metres, of colour `rgb` (raw HDR).
    pub fn add(&mut self, at: Vec3, radius: f32, rgb: Vec3, blink: Blink) {
        let shape = match blink {
            Blink::Steady => [radius, 0.0, 0.0, 1.0],
            Blink::Every { period, phase, lit } => [radius, period, phase, lit],
        };
        for c in CORNERS {
            self.centre.push(at.to_array());
            self.corner.push(c);
            self.color.push([rgb.x, rgb.y, rgb.z, 1.0]);
            self.shape.push(shape);
        }
    }

    /// Lights every `spacing` metres from `a` to `b`, both ends included.
    pub fn line(&mut self, a: Vec3, b: Vec3, spacing: f32, radius: f32, rgb: Vec3) {
        let n = ((b - a).length() / spacing).round().max(1.0) as u32;
        for i in 0..=n {
            self.add(a.lerp(b, i as f32 / n as f32), radius, rgb, Blink::Steady);
        }
    }

    /// `n` lights round the X axis at radius `r`, at `x` (the colony's convention: angle `a` from
    /// +Y towards +Z).
    pub fn ring(&mut self, x: f32, r: f32, n: u32, radius: f32, rgb: Vec3) {
        for i in 0..n {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            self.add(Vec3::new(x, r * a.cos(), r * a.sin()), radius, rgb, Blink::Steady);
        }
    }

    /// The mesh: a quad a light. Not to be frustum-culled by its points alone (the quads grow).
    pub fn mesh(self) -> Mesh {
        let n = self.centre.len() as u32 / 4;
        let mut idx = Vec::with_capacity(n as usize * 6);
        for i in 0..n {
            let b = i * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(self.centre));
        mesh.insert_attribute(ATTRIBUTE_CORNER, VertexAttributeValues::Float32x2(self.corner));
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(self.color));
        mesh.insert_attribute(ATTRIBUTE_SHAPE, VertexAttributeValues::Float32x4(self.shape));
        mesh.insert_indices(Indices::U32(idx));
        mesh
    }
}
