//! The HUD's panels in the page's look (`docs/UI.md`): dark glass with fine scanlines and four
//! corner ticks where a box would have its border, and hazard stripes for a caution
//! (`shaders/ui_panel.wgsl`). The glass and its ticks are `bc_client_core::palette`'s, as the page's
//! are. The instruments' gauges are drawn by it too: the vitals' arcs, a dial's ring and a bar, each
//! of lit segments (`bc_client_core::instruments::Gauge`).

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
    /// A gauge's: x its mark (0..1, below 0 none), y where its red begins (0..1, past 1 none).
    more: Vec4,
    /// A gauge's red.
    alert: Vec4,
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

const NO_GAUGE: PanelParams = PanelParams {
    fill: Vec4::ZERO,
    edge: Vec4::ZERO,
    params: Vec4::ZERO,
    more: Vec4::new(-1.0, 2.0, 0.0, 0.0),
    alert: Vec4::ZERO,
};

impl PanelMaterial {
    /// Glass of `fill`, its corner ticks `tick` (1 px wide, `palette::TICK_LEN` long).
    pub fn glass(fill: Color, tick: Color) -> Self {
        Self {
            panel: PanelParams {
                fill: linear(fill),
                edge: linear(tick),
                params: Vec4::new(0.0, 0.0, 1.0, TICK_LEN),
                ..NO_GAUGE
            },
        }
    }

    /// A vital's gauge beside the crosshair: a quarter of a ring on `side` (-1 left, +1 right) of
    /// the node's middle, `thick` px, lit in `fill` from the bottom up to `full` (0..1) of it, the
    /// rest a track in `track`.
    pub fn arc(fill: Color, track: Color, full: f32, side: f32, thick: f32) -> Self {
        Self {
            panel: PanelParams {
                fill: linear(fill),
                edge: linear(track),
                params: Vec4::new(full, 2.0, side, thick),
                ..NO_GAUGE
            },
        }
    }

    /// An arc's reading: how full, and its colours.
    pub fn set_arc(&mut self, fill: Color, track: Color, full: f32) {
        self.panel.fill = linear(fill);
        self.panel.edge = linear(track);
        self.panel.params.x = full;
    }

    /// A gauge: a dial's ring of segments (`dial`), round from its bottom left over the top to its
    /// bottom right, or a bar of them, left to right; `lit` of `segments` lit in `fill`, the rest a
    /// track in `track`, red past `red_from`, a mark across it at `mark` (in the track's colour).
    #[allow(clippy::too_many_arguments)]
    pub fn gauge(
        dial: bool,
        segments: u8,
        lit: u8,
        fill: Color,
        track: Color,
        red: Color,
        mark: Option<f32>,
        red_from: Option<f32>,
    ) -> Self {
        let n = f32::from(segments.max(1));
        Self {
            panel: PanelParams {
                fill: linear(fill),
                edge: linear(track),
                params: Vec4::new(f32::from(lit) / n, if dial { 3.0 } else { 4.0 }, n, 0.0),
                more: Vec4::new(mark.unwrap_or(-1.0), red_from.unwrap_or(2.0), 0.0, 0.0),
                alert: linear(red),
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
                ..NO_GAUGE
            },
        }
    }
}
