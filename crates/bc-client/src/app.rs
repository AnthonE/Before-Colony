use bevy::prelude::*;

use crate::assets::setup_assets;
use crate::config::LaunchConfig;
use crate::dev_hooks::{DevHooksPlugin, publish_game};
use crate::echo::EchoPlugin;
use crate::fx::{FxState, setup_fx, update_fx};
use crate::hud::{setup_hud, update_hud};
use crate::input::{Aim, Controls, read_input};
use crate::net::{LaunchConfigRes, NetPlugin, drive, game_client, start_net_loop};
use crate::suits_vis::{SuitIndex, sync_suits, tag_parts};

pub fn run() {
    console_error_panic_hook::set_once();
    let cfg = LaunchConfig::from_window();
    let echo = cfg.echo;
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
    .add_plugins((DevHooksPlugin, NetPlugin));
    if echo {
        app.add_plugins(EchoPlugin).add_systems(Startup, crate::echo::setup_echo_scene);
    } else {
        app.insert_non_send(game_client(&cfg))
            .init_resource::<Controls>()
            .init_resource::<Aim>()
            .init_resource::<SuitIndex>()
            .init_resource::<FxState>()
            .add_systems(
                Startup,
                (
                    start_net_loop,
                    (
                        setup_assets,
                        (crate::scene::setup_scene, crate::camera::spawn_camera, setup_fx, setup_hud),
                    )
                        .chain(),
                ),
            )
            .add_systems(
                Update,
                (
                    read_input,
                    drive,
                    sync_suits,
                    tag_parts,
                    crate::camera::follow,
                    update_fx,
                    update_hud,
                    crate::zero_overlay::draw_ghosts,
                    publish_game,
                )
                    .chain(),
            );
    }
    app.run();
}
