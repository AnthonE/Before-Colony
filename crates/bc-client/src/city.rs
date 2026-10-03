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
//!   the colony's shared day (`colony::ColonyDay`), through its haze (the sky function,
//!   `shaders/colony_sky.wgsl`), all coloured by the hour ([`crate::city_hour`]'s colour script).

use std::collections::{HashMap, HashSet};

use bc_client_core::city_mesh::{self, ChunkKey, CityMesh};
use bc_sim::colony::city::{HUB_GATE, SITE, Stage};
use bc_sim::colony::frame::{CityPos, STRIPS, Under, from_colony, window_centre};
use bc_sim::colony::time::key_light;
use bc_sim::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::Exposure;
use bevy::camera::visibility::RenderLayers;
use bevy::light::cluster::ClusterConfig;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::math::DVec3;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{
    ExtendedMaterial, Material, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
    MaterialPlugin, StandardMaterial,
};
use bevy::platform::time::Instant;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::render::view::ColorGrading;
use bevy::shader::ShaderRef;

use crate::camera::MainCamera;
use crate::city_hour::{self, Sky};
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
/// In a key place's room (`colony::city::Room`): the exposure, its lamps' light on what isn't the
/// room's own (people, a bench; lux, from overhead) and the ambient's (nits), and how fast the eye
/// adapts going in or out (1/s).
const EV_INDOOR: f32 = 8.0;
const INDOOR_LUX: f32 = 900.0;
const INDOOR_SKY: f32 = 40.0;
const ADAPT: f32 = 3.0;
/// The glass runs this far (rad) past each strip's edge and lies this far (m) outside the floor, so
/// it tucks under the ground there instead of meeting it edge to edge: two meshes whose shared edge
/// rounds differently leave hairline cracks, and 15 m keeps the two apart in a far view's depth.
const GLASS_TUCK: f32 = 0.003;
const GLASS_OUT: f32 = 15.0;

pub struct CityPlugin;

impl Plugin for CityPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/city.wgsl");
        embedded_asset!(app, "shaders/city_inside.wgsl");
        embedded_asset!(app, "shaders/colony_sky.wgsl");
        embedded_asset!(app, "shaders/city_facade.wgsl");
        app.add_plugins((
            MaterialPlugin::<CityMaterial>::default(),
            MaterialPlugin::<InsideMaterial>::default(),
        ))
        .init_resource::<CityView>()
        .init_resource::<RenderOrigin>()
        .init_resource::<Streamer>()
        .add_systems(
            Update,
            (switch_view, stream_city, place_all, light_city, publish_stats)
                .chain()
                .in_set(crate::view::Vis::Fx),
        );
    }
}

/// `window.__bc.city_chunks`, `city_tris` and `city_ms`: what the streamer shows, and the most a
/// frame spent building chunks lately.
fn publish_stats(
    view: Res<CityView>,
    streamer: Res<Streamer>,
    status: Option<ResMut<crate::dev_hooks::DevStatus>>,
) {
    let Some(mut status) = status else { return };
    let (chunks, tris, ms) = if view.active { streamer.stats() } else { (0, 0, 0.0) };
    status.set("city_chunks", chunks);
    status.set("city_tris", tris);
    status.set("city_ms", ms);
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
#[bind_group_data(CityKey)]
pub struct CityExt {
    #[uniform(100)]
    pub city: CityParams,
    /// The block atlas (`bc_client_core::city_atlas`): the ground's streets and blocks.
    #[texture(101)]
    pub atlas: Handle<Image>,
    /// The Low tier's city: the facades' cheap variant (`FACADE_LOW`) and the ground's sketch
    /// (`city_sketch`, without `CITY_DETAIL`). Software rasterisers get Low, and run every branch of
    /// a shader for every pixel, so the full shader would cost them several times as much.
    pub low: bool,
}

/// What the city's pipeline is compiled for. Public because it's `CityExt`'s bind group data, and
/// `Copy` because `ExtendedMaterial` packs it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CityKey {
    low: bool,
}

impl From<&CityExt> for CityKey {
    fn from(e: &CityExt) -> Self {
        Self { low: e.low }
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct CityParams {
    /// xyz: the render origin in the colony's frame; w: the camera's strip.
    pub origin: Vec4,
    /// x: daylight; y: lamps lit; z: seconds; w: the sun's elevation (rad).
    pub day: Vec4,
    /// x: the key light (lux) on the other strips; y: the sky's light (nits); z: 1 while the camera
    /// is in a key place's room.
    pub light: Vec4,
    /// The hour's air and light, for the sky function (`bc::colony_sky`).
    pub sky: Sky,
}

impl MaterialExtension for CityExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/city.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The prepass and shadow pipelines have no fragment stage here, or don't read these.
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment
                .shader_defs
                .push(if key.bind_group_data.low { "FACADE_LOW" } else { "CITY_DETAIL" }.into());
        }
        Ok(())
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
    /// The hour's air and light, for the sky function (`bc::colony_sky`).
    sky: Sky,
}

/// Keeps the sky function's and the facades' shaders loaded (they're only imported).
#[derive(Resource)]
struct SkyLib(#[allow(dead_code)] Handle<Shader>, #[allow(dead_code)] Handle<Shader>);

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
    /// Built chunks: their entity (none if empty), when they were last wanted, and their
    /// triangles.
    built: HashMap<ChunkKey, (Option<Entity>, u64, u32)>,
    shown: HashSet<ChunkKey>,
    frame: u64,
    /// The slowest frame's building in the last second, ms, and the one before (`__bc.city_ms`).
    build_ms: (f32, f32),
    build_window: f32,
}

impl Streamer {
    /// For `window.__bc`: the chunks shown, their triangles, and the most a frame spent building
    /// chunks over the last second or so (ms).
    pub fn stats(&self) -> (u32, u32, f32) {
        let tris = self.shown.iter().filter_map(|k| self.built.get(k)).map(|b| b.2).sum();
        (self.shown.len() as u32, tris, self.build_ms.0.max(self.build_ms.1))
    }
}

fn to_mesh(m: CityMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// The inside of window `k`, from `x0` to `x1` along: glass facing the axis, cut every degree,
/// tucked under the strips' edges either side ([`GLASS_TUCK`]).
fn window_mesh(k: usize, x0: f32, x1: f32) -> (DVec3, Mesh) {
    let w = window_centre(k);
    let anchor = Vec3::new((x0 + x1) * 0.5, COLONY_RADIUS * w.cos(), COLONY_RADIUS * w.sin());
    let segs = 60u32;
    let (half, r) = (std::f32::consts::FRAC_PI_6 + GLASS_TUCK, COLONY_RADIUS + GLASS_OUT);
    let mut p = Vec::new();
    let mut n = Vec::new();
    for x in [x0, x1] {
        for i in 0..=segs {
            let a = w - half + 2.0 * half * i as f32 / segs as f32;
            p.push((Vec3::new(x, r * a.cos(), r * a.sin()) - anchor).to_array());
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
#[allow(clippy::too_many_arguments)]
pub fn setup_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut insides: ResMut<Assets<InsideMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut streamer: ResMut<Streamer>,
    assets: Res<AssetServer>,
    gfx: Res<Gfx>,
) {
    commands.insert_resource(SkyLib(
        assets.load("embedded://bc_client/shaders/colony_sky.wgsl"),
        assets.load("embedded://bc_client/shaders/city_facade.wgsl"),
    ));
    let atlas = images.add(crate::colony::atlas_image());
    let material = materials.add(ExtendedMaterial {
        // The city draws its own haze (`bc::colony_sky`), so Bevy's distance fog is off for it.
        base: StandardMaterial { perceptual_roughness: 0.85, fog_enabled: false, ..default() },
        // Built for the tier from the start, so a software rasteriser never compiles the full city.
        extension: CityExt { city: CityParams::default(), atlas, low: gfx.tier == GfxTier::Low },
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
/// both layers; only one is seen at a time. Inside, `light_city` grades the picture, sets its bloom
/// and colours and turns the Sun by the hour; on the way out, space's come back.
#[allow(clippy::type_complexity)]
fn switch_view(
    view: Res<CityView>,
    streamer: Res<Streamer>,
    gfx: Res<Gfx>,
    mut cams: Query<(&mut RenderLayers, Option<&mut ColorGrading>, Option<&mut Bloom>), With<MainCamera>>,
    mut suns: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut vis: Query<&mut Visibility>,
) {
    if !view.is_changed() {
        return;
    }
    if !view.active {
        // Space's Sun as `sky::setup_sky` spawns it (`sky::eclipse` puts its illuminance back):
        // otherwise the colony's hour (a red dusk) and its strip's direction follow it out.
        for (mut light, mut tf) in &mut suns {
            light.color = Color::linear_rgb(1.0, 0.965, 0.92);
            *tf = Transform::default().looking_to(-crate::sky::SUN_DIR, Vec3::Y);
        }
    }
    let layers = if view.active {
        RenderLayers::layer(CITY_LAYER)
    } else {
        RenderLayers::from_layers(&[0, crate::cockpit::LAYER])
    };
    for (mut l, grading, bloom) in &mut cams {
        *l = layers.clone();
        if view.active {
            continue;
        }
        // Space's grade (`camera::pilot_effects` keeps its exposure and saturation) and bloom.
        if let Some(mut g) = grading {
            let base = crate::gfx::base_grading(gfx.look);
            g.global.temperature = base.global.temperature;
            g.global.tint = base.global.tint;
            g.shadows = base.shadows;
            g.midtones = base.midtones;
            g.highlights = base.highlights;
        }
        if let Some(mut b) = bloom {
            b.intensity = Bloom::NATURAL.intensity;
        }
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
        let tris = mesh.triangles() as u32;
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
        streamer.built.insert(*key, (entity, frame, tris));
    }
    let spent = start.elapsed().as_secs_f32() * 1000.0;
    streamer.build_ms.0 = streamer.build_ms.0.max(spent);
    streamer.build_window += time.delta_secs();
    if streamer.build_window >= 1.0 {
        streamer.build_window = 0.0;
        streamer.build_ms = (0.0, streamer.build_ms.0);
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
    for (key, (entity, _, _)) in &streamer.built {
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
            .map(|(k, (_, f, _))| (*k, *f))
            .collect();
        old.sort_by_key(|(_, f)| *f);
        let excess = streamer.built.len() - CACHE;
        for (k, _) in old.into_iter().take(excess) {
            if let Some((Some(e), _, _)) = streamer.built.remove(&k) {
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

/// How far the Sun's shadows reach inside, m: the nearest few hundred metres on foot, more from a
/// roof or a suit, in 100 m steps.
fn shadow_reach(height: f32) -> u32 {
    let m = (300.0 + 2.0 * height.max(0.0)).min(1_500.0);
    (m / 100.0).ceil() as u32 * 100
}

/// The Sun's cascades inside: WebGL2 has one, WebGPU three that reach on into the haze.
fn city_cascades(reach: f32) -> CascadeShadowConfig {
    if cfg!(feature = "webgpu") {
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            minimum_distance: 0.15,
            first_cascade_far_bound: 40.0,
            maximum_distance: (reach * 4.0).min(3_000.0),
            ..default()
        }
        .build()
    } else {
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.15,
            maximum_distance: reach,
            ..default()
        }
        .build()
    }
}

/// The colony's light inside, at its hour (`city_hour`'s colour script): the Sun as the camera's
/// strip has it (the mirrors' sun in the window overhead), the sky's light, the sky function's
/// uniform (the haze and every strip's light), the exposure, the picture's grade and bloom, and the
/// distance fog for what the city's own shader doesn't draw (people, cars, trams).
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
    mut cams: Query<(Entity, &Transform, Option<&mut ColorGrading>, Option<&mut Bloom>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight, &mut Transform), (With<Sun>, Without<MainCamera>)>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut materials: ResMut<Assets<CityMaterial>>,
    mut insides: ResMut<Assets<InsideMaterial>>,
    status: Option<ResMut<crate::dev_hooks::DevStatus>>,
    mut adapted: Local<Option<f32>>,
    mut reach: Local<Option<u32>>,
) {
    if !view.active {
        *adapted = None;
        *reach = None;
        return;
    }
    let Ok((cam, cam_tf, grading, bloom)) = cams.single_mut() else { return };
    let d = hour.0;
    let look = city_hour::look(&d);
    let eye = camera_point(&origin, cam_tf).as_vec3();
    let (strip, room) = match from_colony(eye) {
        Under::Land(c) => (c.strip as usize, bc_sim::colony::city::room_at(c.strip, c.s, c.x, c.h)),
        Under::Window { k, .. } => (k as usize, None),
    };
    let to_sun = key_light(strip, &d);
    // The eye adapts to where it is: the hour outside, or a room's lamps.
    let target = if room.is_some() { EV_INDOOR } else { look.ev };
    let ev = match *adapted {
        Some(e) => e + (target - e) * (1.0 - (-ADAPT * time.dt.min(0.25)).exp()),
        None => target,
    };
    *adapted = Some(ev);
    let exposure = 1.0 / (2f32.powf(ev) * 1.2);
    let up = (-Vec3::new(0.0, eye.y, eye.z)).normalize_or(Vec3::Y);
    // The shadows reach as far as the camera's height needs, in 100 m steps (`sky.rs` puts space's
    // back on the way out).
    let height = COLONY_RADIUS - Vec2::new(eye.y, eye.z).length();
    let want = shadow_reach(height);
    let new_reach = *reach != Some(want);
    *reach = Some(want);
    for (sun, mut light, mut tf) in &mut suns {
        if new_reach {
            commands.entity(sun).insert(city_cascades(want as f32));
        }
        if room.is_some() {
            // In a room, the scene's one light is its lamps, from overhead.
            light.illuminance = INDOOR_LUX;
            light.color = Color::linear_rgb(1.0, 0.93, 0.82);
            light.shadow_maps_enabled = false;
            *tf = Transform::default().looking_to(-up, Vec3::X);
        } else {
            light.illuminance = look.lux;
            light.color = Color::linear_rgb(look.sun.x, look.sun.y, look.sun.z);
            light.shadow_maps_enabled = gfx.settings.shadows;
            *tf = Transform::default().looking_to(-to_sun, up);
        }
    }
    if room.is_some() {
        ambient.color = Color::srgb(0.78, 0.85, 1.0);
        ambient.brightness = INDOOR_SKY;
    } else {
        ambient.color = Color::linear_rgb(look.sky.x, look.sky.y, look.sky.z);
        ambient.brightness = look.sky_nits;
    }
    if let Some(mut status) = status {
        // `window.__bc`: the room the camera is in (its place's slug), and the exposure.
        status
            .set("city_room", room.map_or("", |r| bc_sim::content::city::PLACES[usize::from(r.place)].slug));
        status.set("city_ev", ev);
    }
    // Bevy's fog, for what keeps it, matched to the haze near the floor (the city's shader draws the
    // haze itself: its material has the fog off).
    commands.entity(cam).insert((
        Exposure { ev100: ev },
        ClusterConfig::default(),
        look.fog(height, exposure),
    ));
    commands.entity(cam).remove::<EnvironmentMapLight>();
    // The grade and the bloom by the hour. `camera::pilot_effects` owns the grade's exposure and
    // post-saturation (G-strain greys the picture out), so those are left to it; `?look=0` keeps the
    // plain grade.
    if let Some(mut g) = grading
        && gfx.look
    {
        g.global.temperature = look.grading.global.temperature;
        g.global.tint = look.grading.global.tint;
        g.shadows = look.grading.shadows;
        g.midtones = look.grading.midtones;
        g.highlights = look.grading.highlights;
    }
    if let Some(mut b) = bloom {
        b.intensity = look.bloom;
    }
    let t = bodies.t.max(0.0);
    let spin = bc_sim::world::colony_spin_angle(t.floor() as u32, (t - t.floor()) as f32);
    let seconds = (time.now % 3_600.0) as f32;
    let sky = look.sky_params(&d, spin, seconds, gfx.tier != GfxTier::Low);
    let o = origin.0.as_vec3();
    if let Some(mut m) = materials.get_mut(&streamer.material) {
        // F10 recompiles the city for the new tier (each variant once).
        m.extension.low = gfx.tier == GfxTier::Low;
        m.extension.city = CityParams {
            origin: o.extend(strip as f32),
            day: Vec4::new(d.daylight, d.lamps, seconds, d.sun_elev),
            light: Vec4::new(look.lux, look.sky_nits, if room.is_some() { 1.0 } else { 0.0 }, 0.0),
            sky,
        };
    }
    if let Some(mut m) = insides.get_mut(&streamer.inside) {
        m.inside = InsideParams {
            origin: o.extend(spin),
            day: Vec4::new(d.daylight, d.lamps, d.mirror_beta, look.density),
            haze: look.haze.extend(0.0),
            sky,
        };
    }
}
