use bevy::prelude::*;

use crate::assets::setup_assets;
use crate::camera::{Chase, follow, pilot_effects, spawn_camera};
use crate::config::LaunchConfig;
use crate::dev_hooks::{DevHooksPlugin, publish_game};
use crate::echo::EchoPlugin;
use crate::fx::{FxState, setup_fx, update_fx, update_fx_lights};
use crate::gfx::{Gfx, GfxPlugin};
use crate::hud::{setup_hud, update_hud};
use crate::input::{Aim, Controls, read_input};
use crate::net::{LaunchConfigRes, NetPlugin, drive, game_client, start_net_loop};
use crate::net_view::{sync_view, tick_vis_time};
use crate::particles::{setup_particles, update_particles};
use crate::showcase::{Scene, ShowcasePlugin};
use crate::suits_vis::{build_suits, pose_suits, suit_lod};
use crate::view::{BeamFeed, CameraTarget, FxEvents, SuitIndex, Vis, VisTime};

pub fn run() {
    console_error_panic_hook::set_once();
    let cfg = LaunchConfig::from_window();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Before Colony".into(),
            canvas: Some("#bc".into()),
            fit_canvas_to_parent: true,
            prevent_default_event_handling: true,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(LaunchConfigRes(cfg.clone()))
    .insert_resource(ClearColor(Color::BLACK))
    .add_plugins((DevHooksPlugin, GfxPlugin(Gfx::from_config(&cfg))));
    if cfg.perf {
        app.add_plugins(crate::perf::PerfPlugin);
    }
    if cfg.echo {
        app.add_plugins((NetPlugin, EchoPlugin)).add_systems(Startup, crate::echo::setup_echo_scene);
    } else if let Some(scene) = cfg.showcase.as_deref() {
        app.add_plugins((
            VisualsPlugin,
            ShowcasePlugin {
                scene: Scene::parse(scene).unwrap_or(Scene::Lineup),
                t0: cfg.showcase_t,
                cam: cfg.showcase_cam,
                realtime: cfg.showcase_realtime,
                hold: cfg.showcase_hold,
            },
        ));
    } else {
        app.add_plugins((NetPlugin, VisualsPlugin))
            .insert_non_send(game_client(&cfg))
            .init_resource::<Controls>()
            .init_resource::<Aim>()
            .add_systems(Startup, (start_net_loop, setup_hud))
            .add_systems(
                Update,
                (
                    read_input,
                    drive,
                    tick_vis_time,
                    sync_view,
                    crate::rocks::follow_server_field,
                    crate::salvage_vis::sync_chunks,
                )
                    .chain()
                    .in_set(Vis::Drive),
            )
            .add_systems(Update, (follow, pilot_effects).chain().in_set(Vis::Camera))
            .add_systems(
                Update,
                (update_hud, crate::zero_overlay::draw_ghosts, publish_game).chain().in_set(Vis::Hud),
            );
    }
    app.run();
}

/// The world as drawn, shared by game mode and the showcase: the scene, the suits, effects and the
/// camera, all driven by the view model (`view`).
struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisTime>()
            .init_resource::<SuitIndex>()
            .init_resource::<BeamFeed>()
            .init_resource::<FxEvents>()
            .init_resource::<CameraTarget>()
            .init_resource::<FxState>()
            .init_resource::<Chase>()
            .add_plugins((
                crate::sky::SkyPlugin,
                crate::materials::MaterialsPlugin,
                crate::colony::ColonyPlugin,
                crate::particles::ParticlesPlugin,
                crate::beams::BeamsPlugin,
                crate::blast::BlastPlugin,
                crate::ambience::AmbiencePlugin,
                crate::zero_vision::ZeroVisionPlugin,
            ))
            .configure_sets(Update, (Vis::Drive, Vis::Suits, Vis::Camera, Vis::Fx, Vis::Hud).chain())
            .add_systems(
                Startup,
                (
                    (
                        setup_assets,
                        crate::materials::setup_materials,
                        crate::beams::setup_ribbons,
                        crate::model::build_suit_meshes,
                    ),
                    (
                        crate::colony::setup_colony,
                        crate::rocks::setup_field,
                        spawn_camera,
                        setup_fx,
                        setup_particles,
                        crate::blast::setup_blasts,
                        crate::ambience::setup_ambience,
                    ),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    build_suits,
                    pose_suits,
                    crate::anim::animate_suits,
                    crate::damage::damage_suits,
                    crate::damage::update_debris,
                    suit_lod,
                )
                    .chain()
                    .in_set(Vis::Suits),
            )
            .add_systems(
                Update,
                (
                    update_fx,
                    update_fx_lights,
                    update_particles,
                    crate::blast::update_blasts,
                    crate::ambience::update_ambience,
                    crate::rocks::rock_lod,
                )
                    .chain()
                    .in_set(Vis::Fx),
            );
    }
}
