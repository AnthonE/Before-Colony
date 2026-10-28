//! The Muster, drawn (`docs/RING.md`, "The base"): the base's deck in the hub's drum, and the
//! buildings standing on it from their pieces (`bc_client_core::piece_mesh`: the same boxes the
//! walker collides with, dressed in the colony's station language, `station`). It's drawn far above the sector ([`BASE_ORIGIN`]) as the bay is,
//! and only while the view is on the deck ([`BaseView`]: a pilot out of their airlock on Bay Row, or
//! `?showcase=base`), turned so the deck is upright where the view is ([`frame`]): the deck's walker
//! frame there at the origin, whoever's walking it and however far round.
//!
//! - The drum's inside: the deck's floor round it (r 400), rising ahead and behind until it meets
//!   itself overhead past the spine's casing (r 120); the drum's two ends; along the back end, Bay
//!   Row in the station language (`bc_client_core::muster`): its wall with the band and arrows to
//!   the buildings, its walkway's lanes, an airlock door to each bay in a lit frame between hazard
//!   posts, its lamp over the lintel, its number over it and KEEP CLEAR before it.
//! - Its light ([`BaseView`], its own as the city has its): the Sun turned to shine down from strips
//!   along the spine's casing (the deck's day, a shaft of it standing in through a building's
//!   skylight where the tier has shadows), lamps along Bay Row and in the buildings, and a blue haze
//!   down the drum that the far deck overhead fades into.

use std::f32::consts::TAU;

use bc_client_core::muster;
use bc_client_core::piece_mesh::{PieceMesh, site_frame, site_mesh, site_signs};
use bc_proto::presence::PersonPose;
use bc_sim::colony::deck::{BASE, DECKS, DeckPos, local_frame};
use bc_sim::colony::hub::{BAYS, bay_angle};
use bc_sim::content::base::SITES;
use bevy::asset::RenderAssetUsages;
use bevy::camera::Exposure;
use bevy::light::cluster::ClusterConfig;
use bevy::light::{CascadeShadowConfigBuilder, EnvironmentMapLight, NotShadowCaster};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::camera::{FillLight, MainCamera};
use crate::colony::{Kit, around, painted};
use crate::gfx::Gfx;
use crate::materials::{HullTag, Surfaces, paint};
use crate::sky::Sun;
use crate::station::{SignAtlas, Stations};

/// Where the base is drawn: far above anything in the sector, and the bay.
pub const BASE_ORIGIN: Vec3 = Vec3::new(0.0, 26_000.0, 0.0);
/// The deck's daylight from the spine's casing, lux, and the way it falls (a little along the axis
/// and round, so walls catch it and the skylight's shaft leans).
const DAY: f32 = 6_000.0;
const DAY_DIR: Vec3 = Vec3::new(0.35, -1.0, 0.2);
/// The camera's exposure on the deck: an overcast day's, between the bay's lamps and the Sun.
const BASE_EV100: f32 = 11.0;
/// The fill from the deck's air and the floor overhead, cd/m².
const SKY: f32 = 380.0;
/// A building's lamps, lumens.
const LAMP: f32 = 5.0e5;

/// Where along the axis the deck is drawn from: its middle.
const VIEW_X: f32 = (DECKS[BASE as usize].x0 + DECKS[BASE as usize].x1) * 0.5;

/// Whether the view is on the base's deck (it has its own light: `sky.rs` stands aside, as for the
/// city), and how far round the deck it is (`s`, m: the deck is drawn upright there).
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct BaseView {
    pub active: bool,
    pub s: f32,
}

impl Default for BaseView {
    fn default() -> Self {
        Self { active: false, s: SITES[0].s }
    }
}

/// The root everything of the base is drawn under.
#[derive(Component)]
struct BaseRoot;

pub struct BasePlugin;

impl Plugin for BasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BaseView>()
            .add_systems(Update, (follow, light_base).chain().in_set(crate::view::Vis::Fx))
            .add_systems(Update, hang_signs);
    }
}

/// How a point of the colony's own frame is drawn with the view `s` round the deck: the deck's
/// walker frame there turned upright at [`BASE_ORIGIN`] (+Y up, +X along the axis, +Z the way `s`
/// falls), its middle along the axis at the origin.
pub fn frame(s: f32) -> Transform {
    let back = local_frame(BASE, s).inverse();
    let foot = DeckPos::new(BASE, VIEW_X, s, 0.0).to_colony();
    Transform::from_translation(BASE_ORIGIN - back * foot).with_rotation(back)
}

/// Where a point of the deck's walker frame is drawn with the view `s` round the deck.
pub fn world(w: Vec3, s: f32) -> Vec3 {
    frame(s).transform_point(DeckPos::from_walker(BASE, w).to_colony())
}

/// Where a pilot on the deck (as the plaza has them) is drawn, and how their figure is turned
/// (`turned`: about their own up), with the view `s` round the deck.
pub fn place_pose(p: &PersonPose, turned: Quat, s: f32) -> (Vec3, Quat) {
    let at = DeckPos::new(BASE, p.x, p.s, p.h);
    let f = frame(s);
    (f.transform_point(at.to_colony()), f.rotation * local_frame(BASE, p.s) * turned)
}

/// Keeps the base drawn round the view, and shown only while the view is on the deck.
fn follow(view: Res<BaseView>, mut roots: Query<(&mut Transform, &mut Visibility), With<BaseRoot>>) {
    if !view.is_changed() {
        return;
    }
    for (mut tf, mut vis) in &mut roots {
        *tf = frame(view.s);
        let want = if view.active { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The drum's inside round the deck: its ends (the back end over Bay Row's wall, which
/// `muster` draws), the spine's casing overhead with its light strips.
fn drum_kit() -> Kit {
    let mut k = Kit::default();
    let d = &DECKS[BASE as usize];
    let (x0, x1, r) = (d.x0, d.x1, d.floor_r);
    let casing = r - d.roof_h;
    // (The floor is `muster::deck_floor`'s plates and markings.)
    // The ends, facing in.
    k.lathe(&[(x0, casing), (x0, r - muster::WALL_H)], 256, painted(paint::HULL));
    k.lathe(&[(x1, r + 0.05), (x1, casing)], 256, painted(paint::HULL));
    // The spine's casing overhead, its frames round it, and the strips its light comes from.
    k.lathe(&[(x1, casing), (x0, casing)], 96, painted(paint::HULL));
    for j in 0..=6 {
        let x = x0 + (x1 - x0) * j as f32 / 6.0;
        k.lathe(
            &[(x + 3.0, casing), (x + 3.0, casing + 4.0), (x - 3.0, casing + 4.0), (x - 3.0, casing)],
            96,
            painted(paint::HULL_DARK),
        );
    }
    for s in 0..16 {
        let a = TAU * (s as f32 + 0.5) / 16.0;
        k.radial(a, casing + 1.0, (x0 + x1) * 0.5, Vec3::new(x1 - x0 - 24.0, 2.0, 5.0), 48 + paint::WHITE);
    }
    // Ribs up the ends to the casing (at the back, from over Bay Row's doors).
    for f in 0..64 {
        let a = TAU * f as f32 / 64.0;
        for (x, foot) in [(x0 + 1.0, 9.0), (x1 - 1.0, 0.0)] {
            let (lo, hi) = (casing, r - foot);
            k.radial(a, (lo + hi) * 0.5, x, Vec3::new(2.0, hi - lo, 3.0), painted(paint::HULL_DARK));
        }
    }
    k
}

/// A building's mesh, or Bay Row's, as the client draws it.
fn to_mesh(m: PieceMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// Builds the base (hidden till the view is on its deck): the drum's inside, Bay Row, its buildings
/// and their lamps. (Their signs go up once the letters are measured: [`hang_signs`].)
pub fn setup_base(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    surfaces: Res<Surfaces>,
    stations: Res<Stations>,
) {
    let root = commands.spawn((Name::new("base"), BaseRoot, frame(SITES[0].s), Visibility::Hidden)).id();
    commands.spawn((
        Mesh3d(meshes.add(drum_kit().mesh())),
        // The colony's own clean slabs: Fornax's weathered plating read as blotches overhead.
        MeshMaterial3d(surfaces.colony.clone()),
        HullTag::paint(paint::HULL, 5).tag(),
        NotShadowCaster,
        ChildOf(root),
    ));
    commands.spawn((
        Mesh3d(meshes.add(to_mesh(muster::deck_floor()))),
        MeshMaterial3d(stations.muster.clone()),
        NotShadowCaster,
        ChildOf(root),
    ));
    commands.spawn((
        Mesh3d(meshes.add(to_mesh(muster::bay_row()))),
        MeshMaterial3d(stations.muster.clone()),
        NotShadowCaster,
        ChildOf(root),
    ));
    for site in &SITES {
        let (foot, turn) = site_frame(site);
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(site_mesh(site)))),
            MeshMaterial3d(stations.muster.clone()),
            Transform::from_translation(foot).with_rotation(turn),
            ChildOf(root),
        ));
        // Its lamps: one in each storey's rooms, and one over its doorways outside.
        let b = site.bounds();
        let at = |w: Vec3| DeckPos::from_walker(site.deck, w).to_colony();
        let mid = (b.min + b.max) * 0.5;
        for (w, lumens) in [
            (Vec3::new(mid.x, 2.6, mid.z), LAMP),
            (Vec3::new(mid.x, 5.6, mid.z + (b.max.z - mid.z) * 0.5), LAMP * 0.7),
            (Vec3::new(mid.x, 3.4, b.min.z - 2.0), LAMP * 0.6),
        ] {
            commands.spawn((
                PointLight {
                    intensity: lumens,
                    range: 24.0,
                    color: Color::srgb(1.0, 0.93, 0.82),
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(at(w)),
                ChildOf(root),
            ));
        }
    }
    // Bay Row's lamps, warm and close to the wall: lit for the stretch of it by the buildings (the
    // rest glow on the wall unlit, for the view up the row).
    let d = &DECKS[BASE as usize];
    let here = d.seam + SITES[0].s / d.floor_r;
    for n in (1..=BAYS as u8).step_by(2) {
        let a = bay_angle(n) + TAU / (2.0 * BAYS as f32);
        let off = (a - here + TAU * 1.5).rem_euclid(TAU) - TAU * 0.5;
        if off.abs() > 0.3 {
            continue;
        }
        commands.spawn((
            PointLight {
                intensity: 2.5e5,
                range: 18.0,
                color: Color::srgb(1.0, 0.9, 0.75),
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(around(a, d.floor_r - 6.0, d.x0 + 2.0)),
            ChildOf(root),
        ));
    }
}

/// Hangs Bay Row's signs and the buildings' names, once the letters are measured.
fn hang_signs(
    mut commands: Commands,
    mut atlas: ResMut<SignAtlas>,
    roots: Query<Entity, With<BaseRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut hung: Local<bool>,
) {
    if *hung || !atlas.ready() {
        return;
    }
    let Ok(root) = roots.single() else { return };
    let mut signs = muster::signs();
    for site in &SITES {
        signs.extend(site_signs(site));
    }
    atlas.hang(&signs, root, &mut commands, &mut meshes, &mut standard);
    *hung = true;
}

/// The base's light while the view is on its deck: the Sun turned to fall from the spine's casing,
/// its shadows as the tier has them (reaching across the deck's buildings, not kilometres of
/// space), the exposure for that day, the deck's fill, and the haze down the drum. The camera's
/// fill light, which is for space, is put out. Whatever space's light set on the way in is set
/// back the frame after.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn light_base(
    mut commands: Commands,
    view: Res<BaseView>,
    gfx: Res<Gfx>,
    cams: Query<(Entity, Option<&Exposure>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight, &mut Transform), (With<Sun>, Without<MainCamera>)>,
    mut fill: Query<&mut Visibility, With<FillLight>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lit: Local<bool>,
) {
    if !view.active {
        // Off the deck: space's Sun as `sky::setup_sky` spawns it, as the city puts it back (the
        // rest of space's light is `sky.rs`'s, which sees the view leave).
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
    let stale = cams.iter().any(|(_, e)| e.is_none_or(|e| e.ev100 != BASE_EV100));
    for (sun, mut light, mut tf) in &mut suns {
        if light.illuminance != DAY || light.shadow_maps_enabled != s.shadows {
            light.illuminance = DAY;
            light.color = Color::srgb(0.95, 0.97, 1.0);
            light.shadow_maps_enabled = s.shadows;
        }
        if stale || gfx.is_changed() {
            *tf = Transform::default().looking_to(DAY_DIR, Vec3::X);
            commands.entity(sun).insert(
                CascadeShadowConfigBuilder {
                    num_cascades: s.cascades,
                    minimum_distance: 0.5,
                    first_cascade_far_bound: 20.0,
                    maximum_distance: 150.0,
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
            Exposure { ev100: BASE_EV100 },
            ClusterConfig::Single,
            DistanceFog {
                color: Color::srgb(0.34, 0.44, 0.58),
                falloff: FogFalloff::Exponential { density: 0.0018 },
                ..default()
            },
        ));
        commands.entity(cam).remove::<EnvironmentMapLight>();
    }
    ambient.color = Color::srgb(0.75, 0.84, 1.0);
    ambient.brightness = SKY;
    for mut v in &mut fill {
        *v = Visibility::Hidden;
    }
}
