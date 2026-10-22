//! The HUD's panels in the page's look (`docs/UI.md`): dark glass with fine scanlines and four
//! corner ticks where a box would have its border, and hazard stripes for a caution
//! (`shaders/ui_panel.wgsl`). The glass and its ticks are `bc_client_core::palette`'s, as the page's
//! are.

use bc_client_core::palette::{self, GLASS_ALPHA, GLASS_DEEP_ALPHA, TICK_ALPHA, TICK_LEN};
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

const fn alpha(hex: palette::Hex, a: f32) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgba(r, g, b, a)
}

/// The glass, its deeper kind, and its ticks (the page's `--glass`, `--glass-deep`, `--tick`).
pub const GLASS: Color = alpha(palette::INK, GLASS_ALPHA);
pub const GLASS_DEEP: Color = alpha(palette::INK, GLASS_DEEP_ALPHA);
pub const TICK: Color = alpha(palette::LABEL, TICK_ALPHA);

impl PanelMaterial {
    /// Glass of `fill`, its corner ticks `tick` (1 px wide, `palette::TICK_LEN` long).
    pub fn glass(fill: Color, tick: Color) -> Self {
        Self {
            panel: PanelParams {
                fill: linear(fill),
                edge: linear(tick),
                params: Vec4::new(0.0, 0.0, 1.0, TICK_LEN),
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
