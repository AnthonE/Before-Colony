//! ZERO's view. While the system is engaged the picture carries a faint echo of itself, as if the
//! pilot saw a split second ahead, and its edges take a magenta cast that deepens as ZERO's strain
//! builds; in a seizure the echo tears and the view flickers. A fullscreen pass on the HDR picture
//! (`shaders/zero_vision.wgsl`), on tiers with post effects, drawn only while it shows.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::render::sync_component::SyncComponent;
use bevy::shader::ShaderRef;

pub struct ZeroVisionPlugin;

impl Plugin for ZeroVisionPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/zero_vision.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<ZeroVision>::default());
    }
}

/// On the main camera; `camera::pilot_effects` drives it.
#[derive(Component, Clone, Copy, Debug, Default, ShaderType)]
pub struct ZeroVision {
    /// 0..1: ZERO engaged (eased in and out).
    pub engaged: f32,
    /// 0..1: how close ZERO is to seizing control.
    pub strain: f32,
    /// 0..1: a seizure.
    pub seizure: f32,
    /// Seconds, for the flicker.
    pub time: f32,
}

impl SyncComponent for ZeroVision {
    type Target = Self;
}

impl ExtractComponent for ZeroVision {
    type QueryData = Read<ZeroVision>;
    type QueryFilter = With<Camera>;
    type Out = Self;

    fn extract_component(z: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        // No pass at all while it would change nothing.
        (z.engaged > 1e-3 || z.seizure > 1e-3).then_some(*z)
    }
}

impl FullscreenMaterial for ZeroVision {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/zero_vision.wgsl".into()
    }
}
