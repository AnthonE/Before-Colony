//! The homes' buildings from inside (`bc_sim::colony::homes`, `docs/OPEN_CITY.md`): the inside of a
//! home building is drawn only while the camera is in it or at its doors, the storeys either side of
//! the one it's on (from the street, the ground floor and the one over it), built from the rules'
//! boxes (`city_mesh::home_inside`) and drawn with the city's material, which lights it as a room.
//! Its outside is the city's chunks' (`city_mesh::chunk`), with its doors cut through it close up.

use std::collections::HashMap;

use bc_client_core::city_mesh;
use bc_econ::homes::Piece;
use bc_econ::wire::HomesView;
use bc_sim::colony::frame::{Under, from_colony};
use bc_sim::colony::homes;
use bc_sim::content::homes::Furniture;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::city::{
    CITY_LAYER, CityView, Placed, RenderOrigin, Streamer, camera_point, colony_point, to_mesh,
};
use crate::net::GameClient;

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

/// A piece being put down, as drawn: its home, kind, where, turns, storey and whether it fits.
type Ghost = (u32, u8, (u16, u16), u8, u8, bool);
/// The homes' furniture as drawn: each home's id, its pieces, and the beds someone's asleep in.
type Furnishings = Vec<(u32, Vec<Piece>, Vec<u16>)>;

/// The insides drawn now; the shut doors' leaves, as drawn (`city_mesh::door_leaves`); the
/// furniture of the homes near (each home's id and pieces, `city_mesh::home_furniture`); and the
/// piece being put down in build mode (`city_mesh::furniture_ghost`).
#[derive(Resource, Default)]
pub struct HomeInsides {
    built: HashMap<Key, Entity>,
    doors: Option<(Vec<bc_sim::colony::city::CityBox>, Entity)>,
    furniture: Option<(Furnishings, Entity)>,
    ghost: Option<(Ghost, Entity)>,
}

/// Once a frame in the colony: draws the insides of the home buildings the camera is in or beside,
/// and lets the rest go.
pub fn stream_homes(
    mut commands: Commands,
    (view, origin, streamer): (Res<CityView>, Res<RenderOrigin>, Res<Streamer>),
    mut meshes: ResMut<Assets<Mesh>>,
    mut insides: ResMut<HomeInsides>,
    cams: Query<&Transform, With<MainCamera>>,
    (me, game): (Res<crate::onfoot::OnFoot>, NonSend<GameClient>),
) {
    let g = game.borrow();
    draw_doors(&mut commands, &view, &streamer, &mut meshes, &mut insides, &me);
    draw_furniture(&mut commands, &view, &streamer, &mut meshes, &mut insides, &me, &g.core.hangar.homes);
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

/// The leaves of the shut doors near the pilot on foot (`onfoot::CityFoot::doors`): one mesh,
/// built again whenever they change.
fn draw_doors(
    commands: &mut Commands,
    view: &CityView,
    streamer: &Streamer,
    meshes: &mut Assets<Mesh>,
    insides: &mut HomeInsides,
    me: &crate::onfoot::OnFoot,
) {
    let (strip, leaves) = match me.city.as_ref() {
        Some(c) if view.active => (c.strip, c.doors.clone()),
        _ => (0, Vec::new()),
    };
    if insides.doors.as_ref().is_some_and(|(drawn, _)| *drawn == leaves) {
        return;
    }
    if let Some((_, e)) = insides.doors.take() {
        commands.entity(e).despawn();
    }
    let Some(root) = streamer.root() else { return };
    if leaves.is_empty() {
        return;
    }
    let (anchor, mesh) = city_mesh::door_leaves(strip, &leaves);
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
    insides.doors = Some((leaves, e));
}

/// The furniture of the held homes near the pilot on foot, on their strip, as everyone who comes
/// in sees it, with whoever's asleep in its beds (one mesh, built again whenever a piece comes,
/// goes or moves, or someone lies down or gets up); and the piece they're
/// putting down, if they are, green where it fits and red where it doesn't.
fn draw_furniture(
    commands: &mut Commands,
    view: &CityView,
    streamer: &Streamer,
    meshes: &mut Assets<Mesh>,
    insides: &mut HomeInsides,
    me: &crate::onfoot::OnFoot,
    homes_view: &HomesView,
) {
    let city = me.city.as_ref().filter(|_| view.active);
    let placed: Furnishings = match city {
        Some(c) => homes_view
            .homes
            .iter()
            .filter(|l| !l.pieces.is_empty() && homes::HomeId(l.id).strip() == c.strip)
            .map(|l| (l.id, l.pieces.clone(), l.asleep.iter().map(|(bed, _)| *bed).collect()))
            .collect(),
        None => Vec::new(),
    };
    let ghost: Option<Ghost> = city.and_then(|c| {
        let b = c.build.as_ref()?;
        let (id, at, storey) = b.at?;
        Some((id.0, b.kind as u8, at, b.turn, storey, b.why.is_none()))
    });
    let root = streamer.root();
    if insides.furniture.as_ref().is_none_or(|(drawn, _)| *drawn != placed) {
        if let Some((_, e)) = insides.furniture.take() {
            commands.entity(e).despawn();
        }
        if let (Some(root), Some(c)) = (root, city)
            && !placed.is_empty()
        {
            let homes: Vec<city_mesh::Furnished> = placed
                .iter()
                .filter_map(|(id, pieces, asleep)| {
                    homes::home(homes::HomeId(*id)).map(|h| (h, pieces.clone(), asleep.clone()))
                })
                .collect();
            let (anchor, mesh) = city_mesh::home_furniture(c.strip, &homes);
            let e = spawn_city_mesh(commands, meshes, streamer, root, anchor, mesh);
            insides.furniture = Some((placed, e));
        }
    }
    if insides.ghost.as_ref().map(|(drawn, _)| *drawn) != ghost {
        if let Some((_, e)) = insides.ghost.take() {
            commands.entity(e).despawn();
        }
        if let (Some(root), Some(gh)) = (root, ghost)
            && let (Some(home), Some(kind)) = (homes::home(homes::HomeId(gh.0)), Furniture::from_u8(gh.1))
        {
            let at = (f32::from(gh.2.0) / 10.0, f32::from(gh.2.1) / 10.0);
            let (anchor, mesh) = city_mesh::furniture_ghost(&home, kind, at, gh.3, u32::from(gh.4), gh.5);
            let e = spawn_city_mesh(commands, meshes, streamer, root, anchor, mesh);
            insides.ghost = Some((gh, e));
        }
    }
}

/// A mesh of the city's, drawn with its material under the streamer's root.
fn spawn_city_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    streamer: &Streamer,
    root: Entity,
    anchor: bc_sim::colony::frame::CityPos,
    mesh: city_mesh::CityMesh,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(to_mesh(mesh))),
            MeshMaterial3d(streamer.material().clone()),
            Transform::default(),
            Placed(colony_point(anchor)),
            RenderLayers::layer(CITY_LAYER),
            ChildOf(root),
        ))
        .id()
}
