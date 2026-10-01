use bevy::prelude::*;

use crate::assets::setup_assets;
use crate::camera::{Chase, follow, pilot_effects, spawn_camera};
use crate::config::LaunchConfig;
use crate::dev_hooks::{DevHooksPlugin, publish_game};
use crate::echo::EchoPlugin;
use crate::fx::{FxState, setup_fx, update_fx, update_fx_lights};
use crate::gfx::{Gfx, GfxPlugin};
use crate::hud::{place_instruments, setup_hud, show_hud, update_hud, update_marks, update_panels};
use crate::input::{Aim, Controls, read_input};
use crate::net::{LaunchConfigRes, NetPlugin, drive, game_client, start_net_loop};
use crate::net_view::{sync_view, tick_vis_time, track_bodies};
use crate::page::{Ui, UiCmds, apply_ui_cmds, drain_inbox, init_page, publish_view};
use crate::particles::{setup_particles, update_particles};
use crate::pointer::{PointerRes, update_pointer};
use crate::session::{Pilot, SessionPlugin, drive_link};
use crate::settings::{HintState, publish_settings, update_hints, update_settings};
use crate::showcase::{Scene, ShowcasePlugin};
use crate::suits_vis::{build_suits, pose_suits, suit_lod};
use crate::view::{
    BeamFeed, CameraTarget, DrawnBodies, FxEvents, MissileFeed, SuitIndex, ViewPrefs, Vis, VisTime,
};

pub fn run() {
    console_error_panic_hook::set_once();
    let cfg = LaunchConfig::from_window();
    let game_mode = !cfg.echo && cfg.showcase.is_none();
    // The pilot's settings (game mode): the graphics tier and the view start from them.
    let saved = game_mode.then(|| crate::settings::load(&cfg));
    let mut gfx = Gfx::from_config(&cfg);
    let mut prefs =
        ViewPrefs { shake: if cfg.calm { 0.25 } else { 1.0 }, flashing: !cfg.calm, ..ViewPrefs::default() };
    if let Some((s, _)) = &saved {
        crate::settings::apply_saved_tier(&mut gfx, &cfg, &s.0);
        prefs = crate::settings::view_prefs(&s.0);
    }
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
    .insert_resource(prefs)
    .add_plugins((DevHooksPlugin, GfxPlugin(gfx)));
    // The page's font, for everything Bevy writes on the screen.
    let font = crate::hud::UiFont::load(&mut app.world_mut().resource_mut::<Assets<Font>>());
    app.insert_resource(font);
    if cfg.perf {
        app.add_plugins(crate::perf::PerfPlugin);
    }
    if cfg.echo {
        app.add_plugins((NetPlugin { dial_at_startup: true }, EchoPlugin))
            .add_systems(Startup, crate::echo::setup_echo_scene);
    } else if let Some(scene) = cfg.showcase.as_deref() {
        app.add_plugins((
            VisualsPlugin,
            ShowcasePlugin {
                scene: Scene::parse(scene).unwrap_or(Scene::Lineup),
                t0: cfg.showcase_t,
                cam: cfg.showcase_cam,
                realtime: cfg.showcase_realtime,
                hz: cfg.showcase_hz,
                hold: cfg.showcase_hold,
                frame: crate::config::parse_frame(&cfg.frame).unwrap_or(bc_proto::FrameId::WingZero),
            },
        ));
    } else if let Some((settings, store)) = saved {
        let frame = crate::config::parse_frame(&cfg.frame)
            .or_else(|| crate::config::parse_frame(&settings.0.frame))
            .unwrap_or(bc_proto::FrameId::WingZero);
        app.add_plugins((NetPlugin { dial_at_startup: false }, VisualsPlugin, SessionPlugin))
            .insert_resource(settings)
            .insert_resource(store)
            .init_resource::<HintState>()
            .insert_non_send(game_client(&cfg))
            .insert_resource(Pilot::new(cfg.name.clone(), frame))
            .init_resource::<Controls>()
            .init_resource::<Aim>()
            .init_resource::<Ui>()
            .init_resource::<UiCmds>()
            .init_resource::<PointerRes>()
            .init_resource::<crate::onfoot::OnFoot>()
            .init_resource::<crate::terminal::TerminalLog>()
            .add_systems(First, drain_inbox)
            .add_systems(Startup, (start_net_loop, setup_hud, crate::onfoot::setup_onfoot))
            .add_systems(
                Update,
                (
                    drive_link,
                    apply_ui_cmds,
                    crate::input::toggle_camera,
                    update_settings,
                    update_pointer,
                    read_input,
                    update_hints,
                    drive,
                    tick_vis_time,
                    track_bodies,
                    sync_view,
                    crate::onfoot::drive_onfoot,
                    crate::rocks::follow_server_field,
                    crate::rocks::follow_rock_states,
                    crate::salvage_vis::sync_chunks,
                )
                    .chain()
                    .in_set(Vis::Drive),
            )
            .add_systems(
                Update,
                (follow, pilot_effects, crate::onfoot::onfoot_camera).chain().in_set(Vis::Camera),
            )
            .add_plugins(crate::audio::AudioPlugin)
            .add_systems(First, crate::audio::build_bank)
            .add_systems(Update, crate::audio::play_sound.in_set(Vis::Audio))
            .add_systems(
                Update,
                (
                    show_hud,
                    update_hud,
                    update_marks,
                    update_panels,
                    place_instruments,
                    crate::zero_overlay::draw_ghosts,
                    publish_game,
                    crate::onfoot::publish_onfoot,
                )
                    .chain()
                    .in_set(Vis::Hud),
            )
            .add_systems(
                Last,
                (init_page, publish_view, publish_settings, crate::terminal::publish_terminal),
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
            .init_resource::<DrawnBodies>()
            .init_resource::<SuitIndex>()
            .init_resource::<BeamFeed>()
            .init_resource::<MissileFeed>()
            .init_resource::<FxEvents>()
            .init_resource::<CameraTarget>()
            .init_resource::<FxState>()
            .init_resource::<Chase>()
            .add_plugins((
                crate::sky::SkyPlugin,
                crate::materials::MaterialsPlugin,
                crate::dots::DotsPlugin,
                crate::colony::ColonyPlugin,
                crate::landmarks::LandmarksPlugin,
                crate::particles::ParticlesPlugin,
                crate::beams::BeamsPlugin,
                crate::blast::BlastPlugin,
                crate::ambience::AmbiencePlugin,
                crate::zero_vision::ZeroVisionPlugin,
                crate::hangar::HangarPlugin,
                crate::shade::ShadePlugin,
                crate::cockpit::CockpitPlugin,
                crate::ui_panel::UiPanelPlugin,
            ))
            .configure_sets(
                Update,
                (Vis::Drive, Vis::Suits, Vis::Camera, Vis::Audio, Vis::Fx, Vis::Hud).chain(),
            )
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
                        crate::landmarks::setup_landmarks,
                        crate::rocks::setup_field,
                        spawn_camera,
                        setup_fx,
                        setup_particles,
                        crate::missiles_vis::setup_missiles,
                        crate::blast::setup_blasts,
                        crate::ambience::setup_ambience,
                        crate::hangar::setup_bay,
                    ),
                    crate::cockpit::setup_cockpit,
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
                    crate::missiles_vis::update_missiles,
                    update_fx_lights,
                    update_particles,
                    crate::blast::update_blasts,
                    crate::ambience::update_ambience,
                    crate::rocks::rock_lod,
                    crate::cockpit::drive_cockpit,
                )
                    .chain()
                    .in_set(Vis::Fx),
            );
    }
}
