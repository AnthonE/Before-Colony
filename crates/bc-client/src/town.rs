//! The ring's town, drawn (`docs/FREE_ROAM.md`, V1): its streets and buildings a chunk at a time
//! (`bc_client_core::town_mesh`, from the very boxes the walker meets, in the station language at the
//! town's scale), the ring's shell round it (the ceiling's glass and its lattice 500 m up, the ring's
//! two faces), and its own light.
//!
//! - **Drawn round the view.** The town is drawn far above anything in the sector ([`TOWN_ORIGIN`]),
//!   turned so the floor is upright where the view is ([`frame`]): a floating origin, so whatever
//!   is near is near the origin, as the base is drawn (`base`).
//! - **Streamed.** The chunks within the sight line ([`SIGHT`]: past it the floor rising ahead is
//!   behind the ceiling) are built round the camera, the nearest first, a few a frame
//!   ([`BUILDS_PER_FRAME`]), finely within [`NEAR`] and coarsely past it, and dropped when they fall
//!   out of sight. [`TownStats`] counts them for the page's `__bc`.
//! - **Its light**: the Sun turned to fall from the ceiling, the exposure of a bright day, the sky's
//!   fill, and the haze along the ring in which the far floor fades as it rises (the painted
//!   anime's layers of it, `docs/RING.md`, "The look").
//!
//! Only `?showcase=town` shows it yet; walking it comes with the town's first walkers (P1, O1).

use std::collections::HashMap;
use std::f32::consts::TAU;

use bc_client_core::town_mesh::{CHUNK, CHUNKS, Lod, chunk_at, chunk_mesh, chunk_t};
use bc_sim::colony::deck::{DeckPos, RING, deck_at, local_frame};
use bc_sim::colony::town::{self, ROUND};
use bc_sim::world::{COLONY_HALF_WIDTH, COLONY_INNER_RADIUS, COLONY_RADIUS};
use bevy::asset::RenderAssetUsages;
use bevy::platform::time::Instant;
use bevy::camera::Exposure;
use bevy::light::cluster::ClusterConfig;
use bevy::light::{CascadeShadowConfigBuilder, EnvironmentMapLight, NotShadowCaster};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::camera::{FillLight, MainCamera};
use crate::colony::{Kit, painted};
use crate::gfx::Gfx;
use crate::materials::{HullTag, Surfaces, paint};
use crate::sky::Sun;
use crate::station::Stations;

/// Where the town is drawn: far above anything in the sector, the bay and the base.
pub const TOWN_ORIGIN: Vec3 = Vec3::new(0.0, 34_000.0, 0.0);
/// How far round the ring a chunk is drawn, m: the floor rising ahead meets the ceiling's line of
/// sight 3.3 km on.
pub const SIGHT: f32 = 3_400.0;
/// Within this a chunk is built finely, m.
pub const NEAR: f32 = 900.0;
/// How many chunks a frame may build.
pub const BUILDS_PER_FRAME: usize = 2;
/// The day falling from the ceiling, lux, and the way it falls (out from the axis, slanting along
/// it and round so the towers' faces catch it).
const DAY: f32 = 32_000.0;
const DAY_DIR: Vec3 = Vec3::new(0.45, -1.0, 0.3);
/// The camera's exposure in the town: a bright day's.
const TOWN_EV100: f32 = 13.0;
/// The fill from the ceiling's glass and the floor all round, cd/m².
const SKY: f32 = 2_400.0;
/// The haze along the ring.
const HAZE: Color = Color::srgb(0.66, 0.76, 0.9);
const HAZE_DENSITY: f32 = 0.00042;

/// Whether the view is in the town (it has its own light: `sky.rs` stands aside), and where the
/// town is drawn upright: `x` across, `t` round from Hub Gate.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct TownView {
    pub active: bool,
    pub x: f32,
    pub t: f32,
}

impl Default for TownView {
    fn default() -> Self {
        Self { active: false, x: 0.0, t: 200.0 }
    }
}

/// What the town has drawn, for the page's `__bc`.
#[derive(Resource, Clone, Copy, Default, Debug)]
pub struct TownStats {
    /// Chunks drawn, finely and coarsely; how many wait to be built.
    pub near: u32,
    pub far: u32,
    pub pending: u32,
    /// Vertices drawn.
    pub vertices: u32,
    /// What the last frame's building took, ms.
    pub build_ms: f32,
}

/// The root everything of the town is drawn under.
#[derive(Component)]
struct TownRoot;

/// The chunks drawn, by number: their entity, how finely they're built, and their vertices.
#[derive(Resource, Default)]
struct Chunks(HashMap<u16, (Entity, Lod, u32)>);

pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TownView>()
            .init_resource::<TownStats>()
            .init_resource::<Chunks>()
            .add_systems(Update, (follow, stream_chunks, light_town).chain().in_set(crate::view::Vis::Fx))
            .add_systems(Update, publish_town);
    }
}

/// How a point of the colony's own frame is drawn with the view at `x` across and `t` round: the
/// floor's walker frame there turned upright at [`TOWN_ORIGIN`] (+Y up, +X along the axis, +Z the way
/// `t` falls).
pub fn frame(x: f32, t: f32) -> Transform {
    let s = town::deck_s(t);
    let back = local_frame(RING, s).inverse();
    let foot = DeckPos::new(RING, x, s, 0.0).to_colony();
    Transform::from_translation(TOWN_ORIGIN - back * foot).with_rotation(back)
}

/// Where a point of the town (`x` across, `t` round, `h` up) is drawn with the view as `view` has it.
pub fn world(view: &TownView, x: f32, t: f32, h: f32) -> Vec3 {
    frame(view.x, view.t).transform_point(DeckPos::new(RING, x, town::deck_s(t), h).to_colony())
}

/// Keeps the town drawn round the view, and shown only while the view is in it.
fn follow(view: Res<TownView>, mut roots: Query<(&mut Transform, &mut Visibility), With<TownRoot>>) {
    if !view.is_changed() {
        return;
    }
    for (mut tf, mut vis) in &mut roots {
        *tf = frame(view.x, view.t);
        vis.set_if_neq(if view.active { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// The ring's shell round the town: its two faces, inside, from the floor to the ceiling; the
/// ceiling's lattice; and (apart, lit) its glass.
fn shell_kit() -> Kit {
    let mut k = Kit::default();
    let (h, out, inn) = (COLONY_HALF_WIDTH, COLONY_RADIUS, COLONY_INNER_RADIUS);
    let segs = 720;
    // The faces, facing in (the solid's outside on a profile's right as x runs right and r up).
    k.lathe(&[(-h, inn), (-h, out + 0.05)], segs, painted(paint::HULL));
    k.lathe(&[(h, out + 0.05), (h, inn)], segs, painted(paint::HULL));
    // Frames up the faces, every half a degree of the ring.
    for f in 0..segs {
        let a = TAU * f as f32 / segs as f32;
        for side in [-1.0f32, 1.0] {
            k.radial(
                a,
                (out + inn) * 0.5,
                side * (h - 1.5),
                Vec3::new(3.0, out - inn, 4.0),
                painted(paint::HULL_DARK),
            );
        }
    }
    // The lattice under the glass: girders across the ring every half degree, and round it every
    // 100 m across.
    for f in 0..segs {
        let a = TAU * f as f32 / segs as f32;
        k.radial(a, inn + 3.0, 0.0, Vec3::new(2.0 * h, 6.0, 5.0), painted(paint::HULL_DARK));
    }
    let n = (2.0 * h / 100.0) as i32;
    for j in 0..=n {
        let x = -h + 2.0 * h * j as f32 / n as f32;
        k.lathe(&[(x + 3.0, inn + 6.0), (x - 3.0, inn + 6.0)], segs, painted(paint::HULL_DARK));
    }
    k
}

/// The ceiling's glass, facing down onto the town (lit: the day through it).
fn glass_mesh() -> Mesh {
    let mut k = Kit::default();
    let h = COLONY_HALF_WIDTH;
    // Facing out from the axis: down, from the floor.
    k.lathe(&[(h, COLONY_INNER_RADIUS), (-h, COLONY_INNER_RADIUS)], 720, 0);
    let mut m = k.mesh();
    m.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    m
}

/// A chunk's mesh, as the client draws it.
fn to_mesh(m: bc_client_core::piece_mesh::PieceMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// Builds the town's root and its shell (hidden till the view is in the town); its chunks come as
/// they're wanted ([`stream_chunks`]).
pub fn setup_town(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    surfaces: Res<Surfaces>,
) {
    let view = TownView::default();
    let root = commands.spawn((Name::new("town"), TownRoot, frame(view.x, view.t), Visibility::Hidden)).id();
    commands.spawn((
        Mesh3d(meshes.add(shell_kit().mesh())),
        MeshMaterial3d(surfaces.colony.clone()),
        HullTag::paint(paint::HULL, 9).tag(),
        NotShadowCaster,
        ChildOf(root),
    ));
    // The day through the glass: the pale sky blue the vanishing point burns white-blue in.
    let glass = standard.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.72, 0.88),
        emissive: LinearRgba::rgb(0.72, 0.86, 1.0) * 9_000.0,
        unlit: false,
        perceptual_roughness: 0.2,
        ..default()
    });
    commands.spawn((Mesh3d(meshes.add(glass_mesh())), MeshMaterial3d(glass), NotShadowCaster, ChildOf(root)));
}

/// How far round the ring from `t` to the nearest of chunk `k`, m.
fn chunk_distance(t: f32, k: u16) -> f32 {
    let (a, b) = (chunk_t(k), chunk_t(k) + CHUNK);
    let off = |p: f32| {
        let d = (p - t).rem_euclid(ROUND);
        d.min(ROUND - d)
    };
    if (t - a).rem_euclid(ROUND) < CHUNK { 0.0 } else { off(a).min(off(b)) }
}

/// Builds the chunks within sight of the camera, the nearest first and a few a frame, finely near
/// it; drops the rest.
#[allow(clippy::too_many_arguments)]
fn stream_chunks(
    mut commands: Commands,
    view: Res<TownView>,
    stations: Res<Stations>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: ResMut<Chunks>,
    mut stats: ResMut<TownStats>,
    roots: Query<(Entity, &Transform), With<TownRoot>>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut last_t: Local<Option<f32>>,
) {
    let Ok((root, root_tf)) = roots.single() else { return };
    if !view.active {
        return;
    }
    // Where the camera is round the ring (above the floor and under the ceiling; else where it was).
    let t = cams
        .single()
        .ok()
        .and_then(|cam| deck_at(root_tf.compute_affine().inverse().transform_point3(cam.translation())))
        .filter(|d| d.deck == RING)
        .map(|d| town::town_t(d.s))
        .or(*last_t)
        .unwrap_or(view.t);
    *last_t = Some(t);
    let reach = (SIGHT / CHUNK).ceil() as i32;
    let here = i32::from(chunk_at(t));
    let mut want: Vec<(f32, u16, Lod)> = (-reach..=reach)
        .map(|d| (here + d).rem_euclid(i32::from(CHUNKS)) as u16)
        .map(|k| {
            let d = chunk_distance(t, k);
            (d, k, if d < NEAR { Lod::Near } else { Lod::Far })
        })
        .filter(|(d, _, _)| *d <= SIGHT)
        .collect();
    want.sort_by(|a, b| a.0.total_cmp(&b.0));
    want.dedup_by_key(|w| w.1);
    // Out of sight: dropped.
    let keep: Vec<u16> = want.iter().map(|w| w.1).collect();
    chunks.0.retain(|k, (e, _, _)| {
        let kept = keep.contains(k);
        if !kept {
            commands.entity(*e).despawn();
        }
        kept
    });
    // The nearest wanted that isn't drawn as wanted, a few a frame.
    let started = Instant::now();
    let mut built = 0;
    let mut pending = 0;
    for (_, k, lod) in want {
        if chunks.0.get(&k).is_some_and(|c| c.1 == lod) {
            continue;
        }
        if built >= BUILDS_PER_FRAME {
            pending += 1;
            continue;
        }
        let m = chunk_mesh(k, lod);
        let vertices = m.positions.len() as u32;
        let e = commands
            .spawn((Mesh3d(meshes.add(to_mesh(m))), MeshMaterial3d(stations.town.clone()), ChildOf(root)))
            .id();
        if let Some((old, _, _)) = chunks.0.insert(k, (e, lod, vertices)) {
            commands.entity(old).despawn();
        }
        built += 1;
    }
    *stats = TownStats {
        near: chunks.0.values().filter(|c| c.1 == Lod::Near).count() as u32,
        far: chunks.0.values().filter(|c| c.1 == Lod::Far).count() as u32,
        pending,
        vertices: chunks.0.values().map(|c| c.2).sum(),
        build_ms: if built > 0 { started.elapsed().as_secs_f32() * 1_000.0 } else { stats.build_ms },
    };
}

/// The town's light while the view is in it: the Sun turned to fall from the ceiling, its shadows
/// over the streets near the view, the exposure for that day, the sky's fill, and the haze along the
/// ring. The camera's fill light, which is for space, is put out. Off the town, space's Sun is put
/// back as `sky::setup_sky` spawns it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn light_town(
    mut commands: Commands,
    view: Res<TownView>,
    gfx: Res<Gfx>,
    cams: Query<(Entity, Option<&Exposure>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight, &mut Transform), (With<Sun>, Without<MainCamera>)>,
    mut fill: Query<&mut Visibility, With<FillLight>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lit: Local<bool>,
) {
    if !view.active {
        if std::mem::take(&mut *lit) {
            for (_, mut light, mut tf) in &mut suns {
                light.color = Color::linear_rgb(1.0, 0.965, 0.92);
                *tf = Transform::default().looking_to(-crate::sky::SUN_DIR, Vec3::Y);
            }
        }
        return;
    }
    *lit = true;
    let s = gfx.settings;
    let stale = cams.iter().any(|(_, e)| e.is_none_or(|e| e.ev100 != TOWN_EV100));
    for (sun, mut light, mut tf) in &mut suns {
        if light.illuminance != DAY || light.shadow_maps_enabled != s.shadows {
            light.illuminance = DAY;
            light.color = Color::srgb(1.0, 0.97, 0.92);
            light.shadow_maps_enabled = s.shadows;
        }
        if stale || gfx.is_changed() {
            *tf = Transform::default().looking_to(DAY_DIR, Vec3::X);
            commands.entity(sun).insert(
                CascadeShadowConfigBuilder {
                    num_cascades: s.cascades,
                    minimum_distance: 1.0,
                    first_cascade_far_bound: 60.0,
                    maximum_distance: 700.0,
                    ..default()
                }
                .build(),
            );
        }
    }
    if !stale && !gfx.is_changed() {
        return;
    }
    for (cam, _) in &cams {
        commands.entity(cam).insert((
            Exposure { ev100: TOWN_EV100 },
            ClusterConfig::Single,
            DistanceFog {
                color: HAZE,
                falloff: FogFalloff::Exponential { density: HAZE_DENSITY },
                ..default()
            },
        ));
        commands.entity(cam).remove::<EnvironmentMapLight>();
    }
    ambient.color = Color::srgb(0.78, 0.86, 1.0);
    ambient.brightness = SKY;
    for mut v in &mut fill {
        *v = Visibility::Hidden;
    }
}

/// `window.__bc.town_chunks`, `town_far`, `town_pending`, `town_vertices` and `town_build_ms`: the
/// chunks drawn finely and coarsely, how many wait, the vertices drawn, and the last frame's
/// building (ms).
fn publish_town(
    stats: Res<TownStats>,
    view: Res<TownView>,
    status: Option<ResMut<crate::dev_hooks::DevStatus>>,
) {
    let Some(mut status) = status else { return };
    if !view.active {
        return;
    }
    status.set("town_chunks", stats.near);
    status.set("town_far", stats.far);
    status.set("town_pending", stats.pending);
    status.set("town_vertices", stats.vertices);
    status.set("town_build_ms", stats.build_ms);
}
