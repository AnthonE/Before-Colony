//! The HUD's panels in the mobile-suit monitor's style: translucent plates with two corners cut
//! off and a thin edge, and hazard stripes for a caution (`shaders/ui_panel.wgsl`).

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::ui_render::prelude::{UiMaterial, UiMaterialPlugin};

pub struct UiPanelPlugin;

impl Plugin for UiPanelPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/ui_panel.wgsl");
        app.add_plugins(UiMaterialPlugin::<PanelMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PanelMaterial {
    #[uniform(0)]
    panel: PanelParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct PanelParams {
    fill: Vec4,
    edge: Vec4,
    params: Vec4,
}

impl UiMaterial for PanelMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/ui_panel.wgsl".into()
    }
}

fn linear(c: Color) -> Vec4 {
    let l = c.to_linear();
    Vec4::new(l.red, l.green, l.blue, l.alpha)
}

impl PanelMaterial {
    /// A plate: its fill and edge colours, corners cut by `chamfer` px, an edge `edge` px wide.
    pub fn plate(fill: Color, edge: Color, chamfer: f32, edge_px: f32) -> Self {
        Self {
            panel: PanelParams {
                fill: linear(fill),
                edge: linear(edge),
                params: Vec4::new(chamfer, 0.0, edge_px, 0.0),
            },
        }
    }

    /// Hazard stripes of two colours.
    pub fn stripes(a: Color, b: Color, chamfer: f32) -> Self {
        Self {
            panel: PanelParams {
                fill: linear(a),
                edge: linear(b),
                params: Vec4::new(chamfer, 1.0, 0.0, 0.0),
            },
        }
    }
}
