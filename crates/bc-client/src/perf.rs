//! `?perf=1`: a frame-time readout in the corner (fps, frame time, tier, entity count).

use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::gfx::Gfx;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup)
            .add_systems(Update, update);
    }
}

#[derive(Component)]
struct PerfText;

fn setup(mut commands: Commands) {
    commands.spawn((
        PerfText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(12.0), ..default() },
        TextColor(Color::srgb(0.6, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(14.0),
            bottom: Val::Px(70.0),
            ..default()
        },
    ));
}

fn update(
    diagnostics: Res<DiagnosticsStore>,
    gfx: Res<Gfx>,
    time: Res<Time<Real>>,
    entities: Query<()>,
    mut last: Local<f32>,
    mut text: Query<&mut Text, With<PerfText>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 0.5 {
        return;
    }
    *last = now;
    let value = |path: DiagnosticPath| diagnostics.get(&path).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let fps = value(FrameTimeDiagnosticsPlugin::FPS);
    let ms = value(FrameTimeDiagnosticsPlugin::FRAME_TIME);
    if let Ok(mut t) = text.single_mut() {
        **t = format!(
            "{fps:.0} fps  {ms:.1} ms  {} {}  {} entities",
            gfx.tier.name(),
            gfx.backend,
            entities.iter().count()
        );
    }
}
