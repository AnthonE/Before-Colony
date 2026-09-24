use bevy::prelude::*;

use crate::config::LaunchConfig;
use crate::dev_hooks::DevHooksPlugin;
use crate::echo::EchoPlugin;
use crate::net::{LaunchConfigRes, NetPlugin};

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
    .insert_resource(LaunchConfigRes(cfg))
    .add_plugins((DevHooksPlugin, NetPlugin))
    .add_systems(Startup, setup)
    .add_systems(Update, spin);
    if echo {
        app.add_plugins(EchoPlugin);
    }
    app.run();
}

#[derive(Component)]
struct Spinner;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        bevy::camera::Hdr,
        bevy::post_process::bloom::Bloom::NATURAL,
        Transform::from_xyz(0.0, 2.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Spinner,
        Mesh3d(meshes.add(Cuboid::new(1.5, 1.5, 1.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.3, 0.2),
            emissive: LinearRgba::rgb(4.0, 1.2, 0.4),
            ..default()
        })),
    ));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(3.0, 5.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spin(time: Res<Time>, mut q: Query<&mut Transform, With<Spinner>>) {
    for mut t in &mut q {
        t.rotate_y(time.delta_secs() * 0.8);
        t.rotate_x(time.delta_secs() * 0.3);
    }
}
