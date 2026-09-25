//! Beams, tracers, beam-saber blades and thruster plumes as camera-facing ribbons
//! (`shaders/ribbon.wgsl`): a white-hot core inside a coloured glow for beams and blades
//! (`shaders/beam.wgsl`), a blue-white jet with shock diamonds for thrusters
//! (`shaders/plume.wgsl`). One material per look, so they batch.

use bc_proto::WeaponKind;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::{Indices, MeshTag, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::{Shader, ShaderRef};

use crate::view::VisTime;

pub struct BeamsPlugin;

impl Plugin for BeamsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/ribbon.wgsl");
        embedded_asset!(app, "shaders/glow.wgsl");
        embedded_asset!(app, "shaders/beam.wgsl");
        embedded_asset!(app, "shaders/plume.wgsl");
        app.add_plugins((
            MaterialPlugin::<BeamMaterial>::default(),
            MaterialPlugin::<PlumeMaterial>::default(),
        ))
        .add_systems(Update, tick_ribbons);
    }
}

/// Glow effects: additive, drawn from both sides, no depth writes, no shadows.
fn glow_pipeline(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
    if let Some(depth) = descriptor.depth_stencil.as_mut() {
        depth.depth_write_enabled = Some(false);
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct BeamMaterial {
    #[uniform(0)]
    style: BeamStyle,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct BeamStyle {
    /// rgb: glow colour (raw HDR); a: core brightness.
    color: Vec4,
    /// x: core width (fraction of the half-width); y: flicker; z: seconds; w: striation.
    params: Vec4,
}

impl Material for BeamMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://bc_client/shaders/beam.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/beam.wgsl".into()
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
        Ok(())
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PlumeMaterial {
    #[uniform(0)]
    style: PlumeStyle,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct PlumeStyle {
    /// rgb: core colour.
    core: Vec4,
    /// rgb: outer glow colour; a: seconds.
    glow: Vec4,
}

impl Material for PlumeMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://bc_client/shaders/plume.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/plume.wgsl".into()
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
        Ok(())
    }
}

/// How one weapon's shots look.
#[derive(Clone, Debug)]
pub struct Look {
    pub material: Handle<BeamMaterial>,
    /// Half-width of the glow (m).
    pub half_width: f32,
    /// Longest streak drawn (m).
    pub length: f32,
    /// The plasma colour of its muzzle flash and impact.
    pub color: Vec3,
}

/// The shared ribbon mesh and every look.
#[derive(Resource)]
pub struct Ribbons {
    pub mesh: Handle<Mesh>,
    pub rifle: Look,
    pub cannon: Look,
    pub buster: Look,
    pub tracer: Look,
    /// Short beam bolts from rapid-fire beam guns (Heavyarms' gatling, Sandrock's machine gun).
    pub bolt: Look,
    /// Deathscythe's buster shield's slow beam.
    pub shield: Look,
    pub saber: Look,
    /// Deathscythe's beam scythe, and Shenlong's beam glaive.
    pub scythe: Look,
    pub glaive: Look,
    /// A heat shotel's edge, glowing as it cuts.
    pub heat: Look,
    /// A missile motor's exhaust, streaming back from the nozzle.
    pub exhaust: Look,
    pub plume: Handle<PlumeMaterial>,
    // Imported by the effect shaders; held so they stay loaded.
    _ribbon: Handle<Shader>,
    _glow: Handle<Shader>,
}

impl Ribbons {
    pub fn look(&self, weapon: WeaponKind) -> &Look {
        match weapon {
            WeaponKind::TwinBusterRifle => &self.buster,
            WeaponKind::BeamCannon => &self.cannon,
            WeaponKind::MachineCannon | WeaponKind::HeadVulcan | WeaponKind::ChestGatling => &self.tracer,
            WeaponKind::BeamGatling | WeaponKind::BeamMachineGun => &self.bolt,
            WeaponKind::BusterShield => &self.shield,
            WeaponKind::BeamScythe => &self.scythe,
            WeaponKind::BeamGlaive => &self.glaive,
            WeaponKind::BeamSaber => &self.saber,
            _ => &self.rifle,
        }
    }

    fn looks(&self) -> [&Look; 11] {
        [
            &self.rifle,
            &self.cannon,
            &self.buster,
            &self.tracer,
            &self.bolt,
            &self.shield,
            &self.saber,
            &self.scythe,
            &self.glaive,
            &self.heat,
            &self.exhaust,
        ]
    }

    /// The glow along a blade as it strikes (none for the army knife, or the Dragon Fang).
    pub fn blade(&self, weapon: WeaponKind) -> Option<&Look> {
        match weapon {
            WeaponKind::BeamSaber => Some(&self.saber),
            WeaponKind::BeamScythe => Some(&self.scythe),
            WeaponKind::BeamGlaive => Some(&self.glaive),
            WeaponKind::HeatShotel | WeaponKind::CrossCrusher => Some(&self.heat),
            _ => None,
        }
    }
}

/// A strip across x in {-1, 1} and along y in [0, 1].
fn ribbon_mesh() -> Mesh {
    const ALONG: u32 = 8;
    let mut positions = Vec::new();
    for j in 0..=ALONG {
        let y = j as f32 / ALONG as f32;
        positions.push([-1.0, y, 0.0]);
        positions.push([1.0, y, 0.0]);
    }
    let mut idx = Vec::new();
    for j in 0..ALONG {
        let b = j * 2;
        idx.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(idx))
}

pub fn setup_ribbons(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut beams: ResMut<Assets<BeamMaterial>>,
    mut plumes: ResMut<Assets<PlumeMaterial>>,
) {
    let mut look =
        |color: Vec3, core: f32, core_width: f32, flicker: f32, striae: f32, half_width, length| Look {
            material: beams.add(BeamMaterial {
                style: BeamStyle {
                    color: color.extend(core),
                    params: Vec4::new(core_width, flicker, 0.0, striae),
                },
            }),
            half_width,
            length,
            color,
        };
    let ribbons = Ribbons {
        mesh: meshes.add(ribbon_mesh()),
        rifle: look(Vec3::new(6.0, 4.2, 1.0), 18.0, 0.22, 0.1, 0.0, 1.2, 110.0),
        cannon: look(Vec3::new(1.2, 7.0, 2.4), 16.0, 0.22, 0.1, 0.0, 2.0, 140.0),
        buster: look(Vec3::new(9.0, 4.0, 12.0), 30.0, 0.3, 0.15, 0.35, 12.0, 480.0),
        tracer: look(Vec3::new(8.0, 3.5, 0.8), 8.0, 0.3, 0.0, 0.0, 0.35, 22.0),
        bolt: look(Vec3::new(9.0, 5.5, 1.2), 14.0, 0.25, 0.1, 0.0, 0.7, 30.0),
        shield: look(Vec3::new(3.0, 9.0, 4.0), 18.0, 0.25, 0.15, 0.2, 1.8, 50.0),
        saber: look(Vec3::new(14.0, 2.4, 7.0), 22.0, 0.3, 0.12, 0.0, 1.0, 14.0),
        scythe: look(Vec3::new(3.0, 12.0, 5.0), 22.0, 0.28, 0.14, 0.0, 1.1, 12.0),
        glaive: look(Vec3::new(12.0, 5.0, 2.0), 22.0, 0.3, 0.12, 0.0, 1.0, 13.0),
        heat: look(Vec3::new(10.0, 2.6, 0.5), 6.0, 0.35, 0.2, 0.0, 0.3, 3.4),
        exhaust: look(Vec3::new(9.0, 4.2, 1.1), 9.0, 0.3, 0.25, 0.0, 0.45, 35.0),
        plume: plumes.add(PlumeMaterial {
            style: PlumeStyle { core: Vec4::new(6.0, 8.0, 14.0, 0.0), glow: Vec4::new(0.6, 1.4, 5.0, 0.0) },
        }),
        _ribbon: assets.load("embedded://bc_client/shaders/ribbon.wgsl"),
        _glow: assets.load("embedded://bc_client/shaders/glow.wgsl"),
    };
    commands.insert_resource(ribbons);
}

/// Places a ribbon so it runs back from `head` along `dir` for `length`.
pub fn place_ribbon(tf: &mut Transform, head: Vec3, dir: Vec3, length: f32, half_width: f32) {
    tf.translation = head - dir * length;
    tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
    tf.scale = Vec3::new(half_width, length, 1.0);
}

/// A beam ribbon's MeshTag: a seed for its flicker, and whether it's a saber blade (full length
/// from the hilt, rounded at the tip) rather than a shot (fading along its tail).
pub fn beam_tag(seed: u8, blade: bool) -> MeshTag {
    MeshTag(u32::from(seed) | u32::from(blade) << 8)
}

/// A plume's MeshTag: power 0..1 and a seed for its flicker.
pub fn plume_tag(power: f32, seed: u8) -> MeshTag {
    MeshTag((power.clamp(0.0, 1.0) * 255.0) as u32 | u32::from(seed) << 8)
}

/// The clock the flicker runs on.
fn tick_ribbons(
    time: Res<VisTime>,
    ribbons: Option<Res<Ribbons>>,
    mut beams: ResMut<Assets<BeamMaterial>>,
    mut plumes: ResMut<Assets<PlumeMaterial>>,
) {
    let Some(r) = ribbons else { return };
    let t = (time.now % 1_000.0) as f32;
    for look in r.looks() {
        if let Some(mut m) = beams.get_mut(&look.material) {
            m.style.params.z = t;
        }
    }
    if let Some(mut m) = plumes.get_mut(&r.plume) {
        m.style.glow.w = t;
    }
}
