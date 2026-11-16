//! `?perf=1`: a frame-time readout in the corner (fps, frame time, tier, entity count), and in the
//! ring's town what it costs to draw (`town::TownStats`): its cells shown and the pieces they're
//! drawn in (a near cell, four cells a face a quad, a district in the haze), the pieces built and
//! still to build, their triangles, the most a frame spent building them and placing its life
//! lately, the people and the cars drawn, and the meshes in view.

use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::gfx::Gfx;
use crate::town::{TownStats, TownView};

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

fn setup(mut commands: Commands, font: Res<crate::hud::UiFont>) {
    commands.spawn((
        PerfText,
        Text::new(""),
        font.text(12.0),
        crate::hud::SHADOW,
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
    (town, view): (Res<TownStats>, Res<TownView>),
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
        if view.active {
            let s = *town;
            t.push_str(&format!(
                "\ntown: {} cells in {} pieces ({} built, {} to build)  {}k tris  build {:.1} ms  life {:.1} ms\n{} people  {} cars  {} meshes in view",
                s.cells,
                s.pieces,
                s.cached,
                s.missing,
                s.tris / 1000,
                s.build_ms,
                s.life_ms,
                s.people,
                s.cars,
                s.draws
            ));
        }
    }
}
