//! The colony's inside, drawn: the First Colony's city at full scale, 32 km long and curving up all
//! round overhead.
//!
//! - **Its own world.** The city is drawn in the colony's own frame, where nothing moves, on a
//!   render layer of its own ([`CITY_LAYER`]): while the view is inside ([`CityView`]), the camera
//!   sees that layer and nothing of space, and the Sun is the strip's.
//! - **A floating origin.** The city spans ±16 km, too far for `f32` to keep a walker's surroundings
//!   steady, so everything in it is placed relative to a render origin ([`RenderOrigin`]) that
//!   follows the camera, a kilometre at a time ([`Placed`]: where it is in the colony, in `f64`).
//! - **Streamed.** The buildings are chunks of blocks at four levels of detail
//!   (`bc_client_core::city_mesh`), chosen round the camera by a quadtree of distances, built a few
//!   at a time within a frame budget (all at once for a showcase), cached, and swapped in only
//!   once what replaces them is built, so the city never shows a hole. The ground, the windows, the
//!   end caps and Hub Gate's terminals are built once.
//! - **Lit strip by strip** by the mirrors' sun in the window over each (`shaders/city.wgsl`), on
//!   the colony's shared day (`colony::ColonyDay`), through its haze (the camera's distance fog).

use std::collections::{HashMap, HashSet};

use bc_client_core::city_mesh::{self, ChunkKey, CityMesh};
use bc_sim::colony::city::{HUB_GATE, SITE, Stage};
use bc_sim::colony::frame::{CityPos, STRIPS, Under, from_colony, window_centre};
use bc_sim::colony::time::key_light;
use bc_sim::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::Exposure;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::light::cluster::ClusterConfig;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{
    DistanceFog, ExtendedMaterial, FogFalloff, Material, MaterialExtension, MaterialPlugin, StandardMaterial,
};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::view::ColorGrading;
use bevy::shader::ShaderRef;

use crate::camera::MainCamera;
use crate::colony::ColonyDay;
use crate::gfx::{Gfx, GfxTier};
use crate::sky::Sun;
use crate::view::{DrawnBodies, VisTime};

/// The render layer the city is on (0 is space and the bay, 1 the cockpit, 2 its screens).
pub const CITY_LAYER: usize = 3;
/// How far the camera strays from the render origin before it moves, m.
const REBASE: f64 = 1_000.0;
/// Below these distances a chunk at L3, L2, L1 splits into the next level's (High; scaled by tier).
const SPLIT: [f32; 3] = [350.0, 900.0, 2_200.0];
/// Chunks kept built, at most.
const CACHE: usize = 1_400;
/// Sunlight through the mirrors at noon, lux; the sky's light at noon, nits.
const NOON_LUX: f32 = 62_000.0;
const NOON_SKY: f32 = 1_900.0;
/// The haze: its density (/m) and its colour at noon (nits): pale blue, the air lit by the mirrors.
const HAZE_DENSITY: f32 = 1.1e-4;
const HAZE_NOON: Vec3 = Vec3::new(5_200.0, 8_000.0, 13_500.0);
/// The camera's exposure (EV100) at noon, following the light down (as an eye's would) until night's.
const EV_NOON: f32 = 14.5;
const EV_NIGHT: f32 = 9.0;

pub struct CityPlugin;

impl Plugin for CityPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/city.wgsl");
        embedded_asset!(app, "shaders/city_inside.wgsl");
        app.add_plugins((
            MaterialPlugin::<CityMaterial>::default(),
            MaterialPlugin::<InsideMaterial>::default(),
        ))
        .init_resource::<CityView>()
        .init_resource::<RenderOrigin>()
        .init_resource::<Streamer>()
        .add_systems(
            Update,
            (switch_view, stream_city, place_all, light_city).chain().in_set(crate::view::Vis::Fx),
        );
    }
}

/// Whether the view is inside the colony, and how its city is built.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct CityView {
    pub active: bool,
    /// Build every chunk wanted before showing any (a showcase's screenshots).
    pub sync: bool,
}

/// Where the render origin is in the colony's frame: city entities are drawn relative to it.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct RenderOrigin(pub DVec3);

impl RenderOrigin {
    /// Moves it to `eye` (the camera, in the colony's frame) once the camera has strayed too far.
    pub fn follow(&mut self, eye: DVec3) {
        if (eye - self.0).length() > REBASE {
            self.0 = eye.round();
        }
    }

    /// Where a point of the colony's frame is drawn.
    pub fn place(&self, p: DVec3) -> Vec3 {
        (p - self.0).as_vec3()
    }
}

/// Where an entity of the city is, in the colony's frame.
#[derive(Component, Clone, Copy, Debug)]
pub struct Placed(pub DVec3);

/// A point of the colony's frame, in `f64`.
pub fn colony_point(p: CityPos) -> DVec3 {
    p.to_colony().as_dvec3()
}

pub type CityMaterial = ExtendedMaterial<StandardMaterial, CityExt>;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct CityExt {
    #[uniform(100)]
    pub city: CityParams,
    /// The block atlas (`bc_client_core::city_atlas`): the ground's streets and blocks.
    #[texture(101)]
    pub atlas: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct CityParams {
    /// xyz: the render origin in the colony's frame; w: the camera's strip.
    pub origin: Vec4,
    /// x: daylight; y: lamps lit; z: seconds; w: the sun's elevation (rad).
    pub day: Vec4,
    /// x: the key light (lux) on the other strips; y: the sky's light (nits).
    pub light: Vec4,
}

impl MaterialExtension for CityExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/city.wgsl".into()
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct InsideMaterial {
    #[uniform(0)]
    inside: InsideParams,
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct InsideParams {
    /// xyz: the render origin in the colony's frame; w: the colony's spin angle.
    origin: Vec4,
    /// x: daylight; y: lamps lit; z: the mirrors' opening; w: the haze's density.
    day: Vec4,
    /// rgb: the haze's colour (nits).
    haze: Vec4,
}

impl Material for InsideMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/city_inside.wgsl".into()
    }
}

/// The city's root, its materials, and its chunks: built, shown, wanted.
#[derive(Resource, Default)]
pub struct Streamer {
    root: Option<Entity>,
    material: Handle<CityMaterial>,
    inside: Handle<InsideMaterial>,
    /// Built chunks: their entity (none if empty) and when they were last wanted.
    built: HashMap<ChunkKey, (Option<Entity>, u64)>,
    shown: HashSet<ChunkKey>,
    frame: u64,
}

fn to_mesh(m: CityMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// The inside of window `k`, from `x0` to `x1` along: glass facing the axis, cut every degree.
fn window_mesh(k: usize, x0: f32, x1: f32) -> (DVec3, Mesh) {
    let w = window_centre(k);
    let anchor = Vec3::new((x0 + x1) * 0.5, COLONY_RADIUS * w.cos(), COLONY_RADIUS * w.sin());
    let segs = 60u32;
    let mut p = Vec::new();
    let mut n = Vec::new();
    for x in [x0, x1] {
        for i in 0..=segs {
            let a = w - std::f32::consts::FRAC_PI_6 + std::f32::consts::FRAC_PI_3 * i as f32 / segs as f32;
            p.push((Vec3::new(x, COLONY_RADIUS * a.cos(), COLONY_RADIUS * a.sin()) - anchor).to_array());
            n.push([0.0, -a.cos(), -a.sin()]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..segs {
        let (a, b) = (i, i + segs + 1);
        // Facing the axis (counter-clockwise seen from inside).
        idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
        .with_inserted_indices(Indices::U32(idx));
    (anchor.as_dvec3(), mesh)
}

/// Builds what's always there inside: the ground, the windows, the end caps, Hub Gate's terminals,
/// the tram stations.
pub fn setup_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut insides: ResMut<Assets<InsideMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut streamer: ResMut<Streamer>,
) {
    let atlas = images.add(crate::colony::atlas_image());
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.85, ..default() },
        extension: CityExt { city: CityParams::default(), atlas },
    });
    let inside = insides.add(InsideMaterial { inside: InsideParams::default() });
    let layer = RenderLayers::layer(CITY_LAYER);
    let root = commands.spawn((Transform::default(), Visibility::Hidden, layer.clone())).id();
    let mut part = |commands: &mut Commands, mesh: Mesh, at: DVec3, city: bool| {
        let mut e = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            Transform::default(),
            Placed(at),
            layer.clone(),
            ChildOf(root),
        ));
        if city {
            e.insert(MeshMaterial3d(material.clone()));
        } else {
            e.insert((MeshMaterial3d(inside.clone()), NotShadowCaster));
        }
    };
    for strip in 0..STRIPS as u8 {
        for seg in 0..16 {
            let (anchor, mesh) = city_mesh::ground(strip, seg * 16, seg * 16 + 16);
            if !mesh.is_empty() {
                part(&mut commands, to_mesh(mesh), colony_point(anchor), true);
            }
        }
        let (anchor, mesh) = city_mesh::hub_gate(strip);
        part(&mut commands, to_mesh(mesh), colony_point(anchor), true);
        for i in 0..bc_sim::colony::transit::STATIONS {
            let (anchor, mesh) = city_mesh::station(strip, i);
            part(&mut commands, to_mesh(mesh), colony_point(anchor), true);
        }
    }
    for k in 0..STRIPS {
        for seg in 0..16 {
            let x0 = -COLONY_HALF_LENGTH + 2_000.0 * seg as f32;
            let (anchor, mesh) = window_mesh(k, x0, x0 + 2_000.0);
            part(&mut commands, mesh, anchor, false);
        }
    }
    for side in [-1.0f32, 1.0] {
        let (centre, mesh) = city_mesh::cap(side);
        part(&mut commands, to_mesh(mesh), centre.as_dvec3(), true);
    }
    streamer.root = Some(root);
    streamer.material = material;
    streamer.inside = inside;
}

/// What the camera sees: the city (on its layer) inside, space and the bay outside. The Sun lights
/// both layers; only one is seen at a time.
fn switch_view(
    view: Res<CityView>,
    streamer: Res<Streamer>,
    gfx: Res<Gfx>,
    mut cams: Query<(&mut RenderLayers, Option<&mut ColorGrading>), With<MainCamera>>,
    mut vis: Query<&mut Visibility>,
) {
    // Inside, the picture's white balance is left alone: the space scene's warm grade turns the
    // colony's blue haze mauve (`apply_camera_tier` puts it back whenever the tier changes).
    let temperature = if view.active { 0.0 } else { crate::gfx::base_grading(gfx.look).global.temperature };
    for (_, grading) in &mut cams {
        if let Some(mut g) = grading
            && g.global.temperature != temperature
        {
            g.global.temperature = temperature;
        }
    }
    if !view.is_changed() {
        return;
    }
    let layers = if view.active {
        RenderLayers::layer(CITY_LAYER)
    } else {
        RenderLayers::from_layers(&[0, crate::cockpit::LAYER])
    };
    for (mut l, _) in &mut cams {
        *l = layers.clone();
    }
    if let Some(mut v) = streamer.root.and_then(|r| vis.get_mut(r).ok()) {
        *v = if view.active { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// The camera's place in the colony's frame.
fn camera_point(origin: &RenderOrigin, cam: &Transform) -> DVec3 {
    origin.0 + cam.translation.as_dvec3()
}

/// How far chunk `key` is from the camera, m: from its nearest point on its own strip, or from its
/// middle less its reach on another.
fn chunk_distance(key: ChunkKey, eye: Vec3, on: Option<CityPos>) -> f32 {
    let r = key.rect();
    match on {
        Some(c) if c.strip == key.strip => {
            let s = c.s.clamp(r.s0, r.s1);
            let x = c.x.clamp(r.x0, r.x1);
            (CityPos::new(key.strip, x, s, 0.0).to_colony() - eye).length()
        }
        _ => {
            let reach = 0.5 * (r.width() * r.width() + r.length() * r.length()).sqrt();
            ((key.anchor().to_colony() - eye).length() - reach).max(0.0)
        }
    }
}

/// Whether a chunk holds any blocks.
fn has_blocks(key: ChunkKey) -> bool {
    let ((b0, b1), (r0, r1)) = key.blocks();
    r0 < r1 && b1 > HUB_GATE.0 && b0 <= SITE.1
}

/// The chunks wanted round the camera: a quadtree from the 2 km chunks down, split while near.
fn wanted(eye: Vec3, on: Option<CityPos>, reach: f32, out: &mut Vec<(ChunkKey, f32)>) {
    fn visit(key: ChunkKey, eye: Vec3, on: Option<CityPos>, reach: f32, out: &mut Vec<(ChunkKey, f32)>) {
        if !has_blocks(key) {
            return;
        }
        let d = chunk_distance(key, eye, on);
        if key.lod > 0 && d < SPLIT[key.lod as usize - 1] * reach {
            for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let child =
                    ChunkKey { strip: key.strip, lod: key.lod - 1, i: key.i * 2 + di, j: key.j * 2 + dj };
                visit(child, eye, on, reach, out);
            }
        } else {
            out.push((key, d));
        }
    }
    for strip in 0..STRIPS as u8 {
        for i in 0..16 {
            for j in 0..2 {
                visit(ChunkKey { strip, lod: 3, i, j }, eye, on, reach, out);
            }
        }
    }
}

/// Whether two chunks of a strip overlap (one holds the other, in the quadtree).
fn overlap(a: ChunkKey, b: ChunkKey) -> bool {
    if a.strip != b.strip {
        return false;
    }
    let ((ab0, ab1), (ar0, ar1)) = a.blocks();
    let ((bb0, bb1), (br0, br1)) = b.blocks();
    ab0 < bb1 && bb0 < ab1 && ar0 < br1 && br0 < ar1
}

/// Picks the chunks round the camera, builds what's missing within the frame's budget, and shows
/// the wanted ones that are built (keeping what they replace until they are).
#[allow(clippy::too_many_arguments)]
fn stream_city(
    mut commands: Commands,
    view: Res<CityView>,
    origin: Res<RenderOrigin>,
    gfx: Res<Gfx>,
    time: Res<Time<Real>>,
    mut streamer: ResMut<Streamer>,
    mut meshes: ResMut<Assets<Mesh>>,
    cams: Query<&Transform, With<MainCamera>>,
    mut vis: Query<&mut Visibility>,
) {
    if !view.active {
        return;
    }
    let Some(root) = streamer.root else { return };
    let Ok(cam) = cams.single() else { return };
    streamer.frame += 1;
    let frame = streamer.frame;
    let eye = camera_point(&origin, cam).as_vec3();
    let on = match from_colony(eye) {
        Under::Land(c) => Some(c),
        Under::Window { .. } => None,
    };
    let reach = match gfx.tier {
        GfxTier::Low => 0.55,
        GfxTier::Medium => 0.85,
        GfxTier::High => 1.0,
        GfxTier::Ultra => 1.4,
    };
    let mut want = Vec::new();
    wanted(eye, on, reach, &mut want);
    // Nearest and finest first.
    want.sort_by(|a, b| a.0.lod.cmp(&b.0.lod).then(a.1.total_cmp(&b.1)));
    let budget = if view.sync {
        f32::INFINITY
    } else {
        let ms = (time.delta_secs() * 1000.0 * 0.15).clamp(1.5, 4.0);
        ms * if gfx.tier == GfxTier::Low { 0.6 } else { 1.0 }
    };
    let start = Instant::now();
    let material = streamer.material.clone();
    for (key, _) in &want {
        if let Some(entry) = streamer.built.get_mut(key) {
            entry.1 = frame;
            continue;
        }
        if start.elapsed().as_secs_f32() * 1000.0 > budget {
            continue;
        }
        let mesh = city_mesh::chunk(*key, Stage(0));
        let entity = (!mesh.is_empty()).then(|| {
            commands
                .spawn((
                    Mesh3d(meshes.add(to_mesh(mesh))),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Placed(colony_point(key.anchor())),
                    RenderLayers::layer(CITY_LAYER),
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id()
        });
        streamer.built.insert(*key, (entity, frame));
    }
    // Show what's wanted and built; keep showing what it replaces until all of that is.
    let wanted_keys: HashSet<ChunkKey> = want.iter().map(|(k, _)| *k).collect();
    let ready = |k: &ChunkKey, s: &Streamer| s.built.contains_key(k);
    let mut shown = HashSet::new();
    for k in &wanted_keys {
        if ready(k, &streamer) {
            shown.insert(*k);
        }
    }
    for old in streamer.shown.iter() {
        if shown.contains(old) {
            continue;
        }
        let covered = wanted_keys.iter().filter(|k| overlap(**k, *old)).all(|k| ready(k, &streamer));
        if !covered {
            shown.insert(*old);
        }
    }
    for (key, (entity, _)) in &streamer.built {
        let Some(e) = entity else { continue };
        if let Ok(mut v) = vis.get_mut(*e) {
            let want = if shown.contains(key) { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    }
    streamer.shown = shown;
    // Let the least recently wanted go, past the cache's size.
    if streamer.built.len() > CACHE {
        let mut old: Vec<(ChunkKey, u64)> = streamer
            .built
            .iter()
            .filter(|(k, _)| !streamer.shown.contains(*k))
            .map(|(k, (_, f))| (*k, *f))
            .collect();
        old.sort_by_key(|(_, f)| *f);
        let excess = streamer.built.len() - CACHE;
        for (k, _) in old.into_iter().take(excess) {
            if let Some((Some(e), _)) = streamer.built.remove(&k) {
                commands.entity(e).despawn();
            }
        }
    }
}

/// Puts the city's entities where they are, relative to the render origin.
fn place_all(origin: Res<RenderOrigin>, mut placed: Query<(Ref<Placed>, &mut Transform)>) {
    let moved = origin.is_changed();
    for (p, mut tf) in &mut placed {
        if moved || p.is_added() || p.is_changed() {
            tf.translation = origin.place(p.0);
        }
    }
}

/// The colony's light inside, at its hour: the Sun as the camera's strip has it (the mirrors' sun
/// in the window overhead), the sky's light, the haze, and the exposure.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn light_city(
    mut commands: Commands,
    view: Res<CityView>,
    origin: Res<RenderOrigin>,
    hour: Res<ColonyDay>,
    bodies: Res<DrawnBodies>,
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    streamer: Res<Streamer>,
    cams: Query<(Entity, &Transform), With<MainCamera>>,
    mut suns: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<MainCamera>)>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut insides: ResMut<Assets<InsideMaterial>>,
) {
    if !view.active {
        return;
    }
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let d = hour.0;
    let eye = camera_point(&origin, cam_tf).as_vec3();
    let strip = match from_colony(eye) {
        Under::Land(c) => c.strip as usize,
        Under::Window { k, .. } => k as usize,
    };
    let to_sun = key_light(strip, &d);
    let daylight = d.daylight;
    let ev = (EV_NOON + daylight.max(1e-4).log2()).max(EV_NIGHT);
    let exposure = 1.0 / (2f32.powf(ev) * 1.2);
    let haze = HAZE_NOON * daylight + Vec3::new(14.0, 18.0, 30.0) * (1.0 - daylight);
    let sky = NOON_SKY * daylight + 6.0;
    let lux = NOON_LUX * daylight;
    // Warmer when the mirrors are low.
    let warm = (1.0 - (d.sun_elev / 0.9).min(1.0)) * daylight.min(1.0);
    for (mut light, mut tf) in &mut suns {
        light.illuminance = lux;
        light.color = Color::linear_rgb(1.0, 0.97 - 0.17 * warm, 0.94 - 0.39 * warm);
        light.shadow_maps_enabled = gfx.settings.shadows;
        let up = (-Vec3::new(0.0, eye.y, eye.z)).normalize_or(Vec3::Y);
        *tf = Transform::default().looking_to(-to_sun, up);
    }
    ambient.color = Color::srgb(0.78, 0.85, 1.0);
    ambient.brightness = sky;
    let fog = haze * exposure;
    commands.entity(cam).insert((
        Exposure { ev100: ev },
        ClusterConfig::default(),
        DistanceFog {
            color: Color::linear_rgb(fog.x, fog.y, fog.z),
            directional_light_color: Color::linear_rgba(1.0, 0.96, 0.88, 0.12 * daylight),
            directional_light_exponent: 12.0,
            falloff: FogFalloff::Exponential { density: HAZE_DENSITY },
        },
    ));
    commands.entity(cam).remove::<EnvironmentMapLight>();
    let o = origin.0.as_vec3();
    if let Some(mut m) = materials.get_mut(&streamer.material) {
        m.extension.city = CityParams {
            origin: o.extend(strip as f32),
            day: Vec4::new(daylight, d.lamps, (time.now % 3_600.0) as f32, d.sun_elev),
            light: Vec4::new(lux, sky, 0.0, 0.0),
        };
    }
    if let Some(mut m) = insides.get_mut(&streamer.inside) {
        let t = bodies.t.max(0.0);
        let spin = bc_sim::world::colony_spin_angle(t.floor() as u32, (t - t.floor()) as f32);
        m.inside = InsideParams {
            origin: o.extend(spin),
            day: Vec4::new(daylight, d.lamps, d.mirror_beta, HAZE_DENSITY),
            haze: haze.extend(0.0),
        };
    }
}
