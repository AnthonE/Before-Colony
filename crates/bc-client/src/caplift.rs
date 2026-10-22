//! The cap lifts' cars (`bc_client_core::caplift`): one to each strip, standing at its landing
//! beside Hub Gate's terminal with its doors open, until the pilot rides it (`onfoot::OnFoot::car`):
//! then it goes up or down its shaft with them, doors shut, and they look out of its glass. Drawn
//! with the city's material, and its glass with a clear one of its own.

use bc_client_core::{caplift, city_mesh};
use bc_sim::colony::frame::{CityPos, STRIPS};
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::city::{CITY_LAYER, Placed, Streamer, colony_point, to_mesh};
use crate::onfoot::OnFoot;

/// Each strip's car once it's built: the car (where it is) and its doors' glass.
#[derive(Resource, Default)]
pub struct LiftCars(Vec<(Entity, Entity)>);

/// Where strip `strip`'s car is drawn, its floor `floor` over the street: the whole car up its
/// shaft, which is plumb under its middle.
fn car_at(strip: u8, floor: f32) -> DVec3 {
    let (s, x) = caplift::car_rect().middle();
    colony_point(CityPos::new(strip, x, s, floor))
}

/// Builds the cars once the city is set up, and puts each where it is this frame.
pub fn draw_lift_cars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    streamer: Res<Streamer>,
    me: Res<OnFoot>,
    mut cars: ResMut<LiftCars>,
    mut placed: Query<&mut Placed>,
    mut shown: Query<&mut Visibility>,
) {
    let Some(root) = streamer.root() else { return };
    if cars.0.is_empty() {
        let glass = materials.add(StandardMaterial {
            base_color: Color::srgba(0.78, 0.88, 0.95, 0.1),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.06,
            reflectance: 0.7,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let layer = RenderLayers::layer(CITY_LAYER);
        for strip in 0..STRIPS as u8 {
            let (_, frame, sides, doors) = city_mesh::lift_car(strip);
            let car = commands
                .spawn((
                    Transform::default(),
                    Visibility::Inherited,
                    Placed(car_at(strip, 0.0)),
                    layer.clone(),
                    ChildOf(root),
                ))
                .id();
            commands.spawn((
                Mesh3d(meshes.add(to_mesh(frame))),
                MeshMaterial3d(streamer.material().clone()),
                Transform::default(),
                layer.clone(),
                ChildOf(car),
            ));
            let mut pane = |m: city_mesh::CityMesh, shown: Visibility| {
                let mut mesh = to_mesh(m);
                mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
                commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(glass.clone()),
                        Transform::default(),
                        shown,
                        NotShadowCaster,
                        layer.clone(),
                        ChildOf(car),
                    ))
                    .id()
            };
            pane(sides, Visibility::Inherited);
            let doors = pane(doors, Visibility::Hidden);
            cars.0.push((car, doors));
        }
    }
    for (strip, &(car, doors)) in cars.0.iter().enumerate() {
        let ridden = me.car.filter(|(k, _)| usize::from(*k) == strip).map(|(_, floor)| floor);
        let at = car_at(strip as u8, ridden.unwrap_or(0.0));
        if let Ok(mut p) = placed.get_mut(car)
            && p.0 != at
        {
            p.0 = at;
        }
        // Shut while it rides; open at its landing.
        let want = if ridden.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Ok(mut v) = shown.get_mut(doors)
            && *v != want
        {
            *v = want;
        }
    }
}
