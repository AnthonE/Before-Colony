//! The homes' buildings from inside (`bc_sim::colony::homes`, `docs/OPEN_CITY.md`): the inside of a
//! home building is drawn only while the camera is in it or at its doors, the storeys either side of
//! the one it's on (from the street, the ground floor and the one over it), built from the rules'
//! boxes (`city_mesh::home_inside`) and drawn with the city's material, which lights it as a room.
//! Its outside is the city's chunks' (`city_mesh::chunk`), with its doors cut through it close up.

use std::collections::HashMap;

use bc_client_core::city_mesh;
use bc_sim::colony::frame::{Under, from_colony};
use bc_sim::colony::homes;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::city::{
    CITY_LAYER, CityView, Placed, RenderOrigin, Streamer, camera_point, colony_point, to_mesh,
};

/// How near a home building's walls the camera is for its inside to be drawn, m (through its doors
/// from across the pavement).
const NEAR: f32 = 14.0;

/// A building's inside as drawn: which building (its strip, block, row and lot) and its storeys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    strip: u8,
    bx: i32,
    row: i32,
    lot: u8,
    from: u32,
    to: u32,
}

/// The insides drawn now.
#[derive(Resource, Default)]
pub struct HomeInsides {
    built: HashMap<Key, Entity>,
}

/// Once a frame in the colony: draws the insides of the home buildings the camera is in or beside,
/// and lets the rest go.
pub fn stream_homes(
    mut commands: Commands,
    view: Res<CityView>,
    origin: Res<RenderOrigin>,
    streamer: Res<Streamer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut insides: ResMut<HomeInsides>,
    cams: Query<&Transform, With<MainCamera>>,
) {
    let mut want: Vec<(Key, homes::Plan)> = Vec::new();
    if view.active
        && let Ok(cam) = cams.single()
        && let Under::Land(c) = from_colony(camera_point(&origin, cam).as_vec3())
    {
        homes::each_plan_near(c.strip, c.s, c.x, NEAR, |p| {
            let inside = p.foot.contains(c.s, c.x) && c.h < p.height;
            let (from, to) = if inside {
                let k = p.storey_at(c.h);
                (k.saturating_sub(1), (k + 1).min(p.storeys - 1))
            } else {
                (0, 1.min(p.storeys - 1))
            };
            let key = Key { strip: p.strip, bx: p.bx, row: p.row, lot: p.lot, from, to };
            want.push((key, *p));
        });
    }
    insides.built.retain(|key, e| {
        let keep = want.iter().any(|(k, _)| k == key);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    let Some(root) = streamer.root() else { return };
    for (key, plan) in want {
        if insides.built.contains_key(&key) {
            continue;
        }
        let (anchor, mesh) = city_mesh::home_inside(&plan, key.from, key.to);
        let e = commands
            .spawn((
                Mesh3d(meshes.add(to_mesh(mesh))),
                MeshMaterial3d(streamer.material().clone()),
                Transform::default(),
                Placed(colony_point(anchor)),
                RenderLayers::layer(CITY_LAYER),
                ChildOf(root),
            ))
            .id();
        insides.built.insert(key, e);
    }
}
