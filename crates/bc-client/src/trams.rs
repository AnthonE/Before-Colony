//! The colony's trams, drawn (`bc_sim::colony::transit`): every train of every strip's line where
//! the timetable has it at the [`TramClock`], three cars each, their doors open at the stations.
//! A car is a shell (floor, sides, glass, roof), so a pilot riding one sees out of it. Nothing
//! about them comes over the wire: the clock is the sector's tick, the same on every screen.

use bc_sim::colony::frame::{CityPos, STRIPS, local_frame};
use bc_sim::colony::transit::{
    CAR_HEIGHT, CAR_LENGTH, CAR_WIDTH, CARS, DOOR_AT, DOOR_WIDTH, FLOOR, TRAINS, TrainState, train,
};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::city::{CITY_LAYER, CityView, Placed, RenderOrigin, colony_point};

/// Trains further than this from the camera aren't drawn, m.
const REACH: f32 = 4_500.0;
/// Each line's colour: Charter's blue, Canal's teal, Gardens' green.
const LINE_COLOURS: [Color; 3] =
    [Color::srgb(0.12, 0.32, 0.72), Color::srgb(0.05, 0.5, 0.52), Color::srgb(0.2, 0.55, 0.22)];

/// When the trams are drawn: the sector's tick and the fraction of the next (game mode: the
/// colony's clock, `ClientCore::colony_tick`; a showcase: its own).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct TramClock(pub u32, pub f32);

pub struct TramsPlugin;

impl Plugin for TramsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TramClock>()
            .add_systems(Startup, setup_trams)
            .add_systems(Update, draw_trams.in_set(crate::view::Vis::Suits));
    }
}

/// A car: its train, which car it is, and its doors (hidden while open).
#[derive(Component)]
struct Car {
    strip: u8,
    k: u8,
    c: usize,
    doors: Entity,
}

/// Boxes `(centre, half extents)` as one mesh, faces out.
pub fn boxes(parts: &[(Vec3, Vec3)]) -> Mesh {
    let (mut pos, mut nrm, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for (c, h) in parts {
        for (n, u, v) in [
            (Vec3::X, Vec3::Y, Vec3::Z),
            (-Vec3::X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (-Vec3::Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (-Vec3::Z, Vec3::Y, Vec3::X),
        ] {
            let base = pos.len() as u32;
            let f = *c + n * *h;
            let (du, dv) = (u * *h, v * *h);
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                pos.push((f + du * a + dv * b).to_array());
                nrm.push(n.to_array());
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx))
}

/// A car in its own frame (`x` along, `y` up from the ground, `z` = −s): the body's panels, the
/// glass, the line's stripe, and the doors.
fn car_meshes() -> [Mesh; 4] {
    let (hl, hw) = (0.5 * CAR_LENGTH, 0.5 * CAR_WIDTH);
    let (sill, eaves, top) = (FLOOR + 0.95, FLOOR + 2.05, CAR_HEIGHT);
    let t = 0.05;
    let mut body = vec![
        // The floor and the underframe, the roof, the ends.
        (Vec3::new(0.0, FLOOR - 0.05, 0.0), Vec3::new(hl, 0.05, hw)),
        (Vec3::new(0.0, 0.62, 0.0), Vec3::new(hl - 2.0, 0.3, hw - 0.2)),
        (Vec3::new(0.0, top - 0.15, 0.0), Vec3::new(hl, 0.15, hw)),
        (Vec3::new(-hl + t, 0.5 * (FLOOR + top), 0.0), Vec3::new(t, 0.5 * (top - FLOOR), hw)),
        (Vec3::new(hl - t, 0.5 * (FLOOR + top), 0.0), Vec3::new(t, 0.5 * (top - FLOOR), hw)),
    ];
    let mut glass = Vec::new();
    let mut stripe = Vec::new();
    let mut doors = Vec::new();
    let half_door = 0.5 * DOOR_WIDTH;
    let spans =
        [(-hl, -DOOR_AT - half_door), (-DOOR_AT + half_door, DOOR_AT - half_door), (DOOR_AT + half_door, hl)];
    for side in [-1.0f32, 1.0] {
        let z = side * (hw - t);
        for (x0, x1) in spans {
            let (cx, half) = (0.5 * (x0 + x1), 0.5 * (x1 - x0));
            body.push((Vec3::new(cx, 0.5 * (FLOOR + sill), z), Vec3::new(half, 0.5 * (sill - FLOOR), t)));
            glass.push((
                Vec3::new(cx, 0.5 * (sill + eaves), z),
                Vec3::new(half, 0.5 * (eaves - sill), t * 0.5),
            ));
            body.push((
                Vec3::new(cx, 0.5 * (eaves + top - 0.3), z),
                Vec3::new(half, 0.5 * (top - 0.3 - eaves), t),
            ));
            stripe.push((Vec3::new(cx, sill - 0.25, z + side * 0.01), Vec3::new(half, 0.12, t)));
        }
        for at in [-DOOR_AT, DOOR_AT] {
            doors.push((
                Vec3::new(at, 0.5 * (FLOOR + eaves + 0.3), z),
                Vec3::new(half_door, 0.5 * (eaves + 0.3 - FLOOR), t),
            ));
        }
    }
    [boxes(&body), boxes(&glass), boxes(&stripe), boxes(&doors)]
}

fn setup_trams(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let [body, glass, stripe, doors] = car_meshes().map(|m| meshes.add(m));
    let white = materials.add(StandardMaterial {
        base_color: Color::srgb(0.86, 0.88, 0.9),
        perceptual_roughness: 0.4,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let pane = materials.add(StandardMaterial {
        base_color: Color::srgba(0.1, 0.16, 0.2, 0.35),
        perceptual_roughness: 0.05,
        metallic: 0.2,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let layer = RenderLayers::layer(CITY_LAYER);
    for strip in 0..STRIPS as u8 {
        let line = materials.add(StandardMaterial {
            base_color: LINE_COLOURS[strip as usize],
            perceptual_roughness: 0.5,
            cull_mode: None,
            ..default()
        });
        let door = materials.add(StandardMaterial {
            base_color: LINE_COLOURS[strip as usize].mix(&Color::WHITE, 0.35),
            perceptual_roughness: 0.5,
            cull_mode: None,
            double_sided: true,
            ..default()
        });
        for k in 0..TRAINS as u8 {
            for c in 0..CARS {
                let root = commands
                    .spawn((
                        Transform::default(),
                        Visibility::Hidden,
                        Placed(Default::default()),
                        layer.clone(),
                    ))
                    .id();
                for (mesh, material) in [(&body, &white), (&glass, &pane), (&stripe, &line)] {
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::default(),
                        layer.clone(),
                        ChildOf(root),
                    ));
                }
                let doors = commands
                    .spawn((
                        Mesh3d(doors.clone()),
                        MeshMaterial3d(door.clone()),
                        Transform::default(),
                        Visibility::Inherited,
                        layer.clone(),
                        ChildOf(root),
                    ))
                    .id();
                commands.entity(root).insert(Car { strip, k, c, doors });
            }
        }
    }
}

/// Every car where its train is, near the camera; their doors open at the stations.
fn draw_trams(
    view: Res<CityView>,
    clock: Res<TramClock>,
    origin: Res<RenderOrigin>,
    cams: Query<&Transform, (With<MainCamera>, Without<Car>)>,
    mut cars: Query<(&Car, &mut Placed, &mut Transform, &mut Visibility)>,
    mut doors: Query<&mut Visibility, Without<Car>>,
) {
    let eye = cams.single().map(|c| origin.0 + c.translation.as_dvec3()).unwrap_or_default();
    let mut trains: [[Option<TrainState>; TRAINS as usize]; 3] = [[None; TRAINS as usize]; 3];
    for (car, mut placed, mut tf, mut vis) in &mut cars {
        let t = *trains[car.strip as usize][car.k as usize]
            .get_or_insert_with(|| train(car.strip, car.k, clock.0, clock.1));
        let at = colony_point(CityPos::new(car.strip, t.car_x(car.c), t.s, 0.0));
        let shown = view.active && (at - eye).length() < f64::from(REACH);
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !shown {
            continue;
        }
        placed.0 = at;
        tf.rotation = local_frame(car.strip, t.s);
        if let Ok(mut v) = doors.get_mut(car.doors) {
            let want = if t.doors { Visibility::Hidden } else { Visibility::Inherited };
            if *v != want {
                *v = want;
            }
        }
    }
}
