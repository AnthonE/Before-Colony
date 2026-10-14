//! The Orrery's view. While the system is engaged the picture carries a faint echo of itself, as if the
//! pilot saw a split second ahead, and its edges take a magenta cast that deepens as the Orrery's strain
//! builds; in a seizure the echo tears and the view flickers. A fullscreen pass on the HDR picture
//! (`shaders/orrery_vision.wgsl`), on tiers with post effects, drawn only while it shows.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::render::sync_component::SyncComponent;
use bevy::shader::ShaderRef;

pub struct OrreryVisionPlugin;

impl Plugin for OrreryVisionPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/orrery_vision.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<OrreryVision>::default());
    }
}

/// On the main camera; `camera::pilot_effects` drives it.
#[derive(Component, Clone, Copy, Debug, Default, ShaderType)]
pub struct OrreryVision {
    /// 0..1: the Orrery engaged (eased in and out).
    pub engaged: f32,
    /// 0..1: how close the Orrery is to seizing control.
    pub strain: f32,
    /// 0..1: a seizure.
    pub seizure: f32,
    /// Seconds, for the flicker.
    pub time: f32,
    /// 1: a seizure may flicker the whole view; 0: the pilot turned flashing effects off.
    pub flicker: f32,
    /// WebGL2 wants uniforms in multiples of 16 bytes.
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl SyncComponent for OrreryVision {
    type Target = Self;
}

impl ExtractComponent for OrreryVision {
    type QueryData = Read<OrreryVision>;
    type QueryFilter = With<Camera>;
    type Out = Self;

    fn extract_component(z: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        // No pass at all while it would change nothing.
        (z.engaged > 1e-3 || z.seizure > 1e-3).then_some(*z)
    }
}

impl FullscreenMaterial for OrreryVision {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/orrery_vision.wgsl".into()
    }
}
