//! The ring's town, drawn (`docs/FREE_ROAM.md`, V1): its streets, blocks, canal, lifts, faces and
//! ceiling a cell round the ring at a time (`bc_client_core::town_mesh`, the same boxes the walker
//! and the cars meet), in the station language at a town's scale, painted like the anime unless the
//! look is plain (`Gfx::paint`, `shaders/town.wgsl`).
//!
//! - **Round the view.** The town is drawn in the frame upright where the view stands
//!   ([`TownAt`]): the floor level underfoot, rising ahead and behind until the ceiling hides it.
//!   Each cell's mesh is in its own frame and is turned round the ring to the view's
//!   (`town_mesh::seen_from`), so nothing is ever far from the origin, however far round the pilot
//!   has gone. It's on the city's render layer: while the view is in the town the camera sees that
//!   layer and nothing of space.
//! - **Streamed, at three distances.** The cells within sight (3.3 km: where the rising floor meets
//!   the ceiling) are built within the frame's budget a part at a time (`town_mesh::CellBuild`), the
//!   nearest first: those near in full, each its own mesh; past them a face a quad, merged four
//!   cells to a mesh; past a kilometre each building its box in the haze, a district to a mesh
//!   ([`Piece`]). A piece still to build has last frame's in its place, so nothing goes missing
//!   while one tier gives way to the next; pieces are kept a while once out of sight, and a wall of
//!   haze closes the tunnel where the built cells end. What it costs is [`TownStats`] (`?perf=1`,
//!   `window.__bc`'s `town_*`).
//! - **Lit by the colony's hour** ([`ColonyDay`], `city_hour`'s colour script): the light from the
//!   louvres overhead, leaning along the ring by the hour; the sky's light in the ceiling's glass;
//!   and the haze, which the town's shader lays down in painted layers.
//! - **Its life** ([`draw_life`]): the traffic and the people `bc_sim::colony::street` has on its
//!   roads and round its blocks at the colony's tick, the cars at its kerbs
//!   (`bc_client_core::town::each_parked`), the ones the pilot left where they got out and the one
//!   they drive, all with the colony's life's meshes and its one material (`life.rs`), in pools
//!   picked nearest first up to the tier's caps.

use std::collections::HashMap;
use std::ops::Range;

use bc_client_core::crowd;
use bc_client_core::life::{self as plan, Lights, Livery, Slots};
use bc_client_core::life_mesh::{self, Body, Gait};
use bc_client_core::station::{PAINT, PAINTS};
use bc_client_core::town as town_foot;
use bc_client_core::town_mesh::{self, CellBuild, Detail, seen_from};
use bc_client_core::vehicle::Vehicle;
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::street;
use bc_sim::colony::town;
use bc_sim::content::town::DistrictKind;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::Exposure;
use bevy::camera::visibility::RenderLayers;
use bevy::light::cluster::ClusterConfig;
use bevy::light::{CascadeShadowConfigBuilder, EnvironmentMapLight, NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::pbr::{DistanceFog, ExtendedMaterial, FogFalloff, MaterialExtension, MaterialPlugin};
use bevy::platform::time::Instant;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::view::ColorGrading;
use bevy::shader::ShaderRef;

use crate::camera::MainCamera;
use crate::city::CITY_LAYER;
use crate::city_hour;
use crate::colony::ColonyDay;
use crate::gfx::{Gfx, GfxTier};
use crate::sky::Sun;
use crate::view::VisTime;

/// How many cells either side of the view's are built in full, how many a face a quad before the
/// haze's boxes (8: past a kilometre), and how many at all (High; fewer on the lower tiers): 26
/// cells is 3.4 km, past where the rising floor meets the ceiling.
const NEAR_CELLS: i32 = 4;
const MID_CELLS: i32 = 8;
const REACH_CELLS: i32 = 26;
/// Cells to a far piece smaller than a district (three to a district).
const QUAD: u16 = 4;
/// How many cells a district at the end of sight may run on past it and still be drawn whole
/// (behind the haze's wall there), not as quads.
const SLACK: i32 = 4;
/// The haze's density against the hour's (`city_hour`'s is the cylinder's, 6 km across): the town's
/// far end is lost in it.
const HAZE: f32 = 3.3;
/// The sky's fill against the colour script's (`city_hour`'s is the cylinder's, where the far
/// strips overhead fill the shade).
const SKY_FILL: f32 = 2.6;
/// Nobody of the town's own is drawn this near the eye on foot, m: the pilot walks through nobody.
const CLEAR: f32 = 0.9;

pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/town.wgsl");
        app.add_plugins(MaterialPlugin::<TownMaterial>::default())
            .init_resource::<TownView>()
            .init_resource::<TownAt>()
            .init_resource::<TownCars>()
            .init_resource::<Streamer>()
            .init_resource::<TownLife>()
            .init_resource::<TownStats>()
            .add_systems(
                Update,
                (stream_town, draw_life, light_town, publish_stats).chain().in_set(crate::view::Vis::Fx),
            );
    }
}

/// Whether the view is in the ring's town (it has its own light: `sky.rs` stands aside, as for the
/// city and the base), and whether to build every cell in sight before showing any (a showcase's
/// screenshots).
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
pub struct TownView {
    pub active: bool,
    pub sync: bool,
}

/// Where the view stands on the ring's floor: round it (the deck's `s`, m) and across it. The town
/// is drawn upright there.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct TownAt {
    pub s: f32,
    pub x: f32,
}

impl Default for TownAt {
    fn default() -> Self {
        let (x, t) = town::out_of_the_lift(0);
        Self { s: town::deck_s(t), x }
    }
}

/// The cars the pilot has to do with: the one they drive, the ones they left where they got out
/// (each with the seed its look comes from), and the kerbs' they drove off in (their bays are empty
/// now).
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct TownCars {
    pub driving: Option<(Vehicle, u32)>,
    pub left: Vec<(Vehicle, u32)>,
    pub taken: Vec<(i32, i32)>,
    /// The Watch's units near the pilot, the server's (`bc_client_core::watch`): its cars, and its
    /// hounds (where over the floor each is).
    pub watch: Vec<Vehicle>,
    pub hounds: Vec<Vehicle>,
    /// The pilots' cars the town's people step aside from: the pilot's own, and the others' near.
    pub movers: Vec<town_foot::Mover>,
    /// The town's people knocked down: the server's word, and the pilot's own hits till it comes.
    pub down: Vec<bc_econ::wire::Down>,
    /// The town's people out of its crowd round the pilot, the server's (`bc_client_core::crowd`):
    /// drawn where it has them, and not where the crowd would.
    pub loose: bc_client_core::crowd::Loose,
    /// The traffic within `traffic_reach` m of the pilot as this client has it (held back by
    /// what's in its lanes: `bc_client_core::traffic`), and the cars taken out of it, by id.
    pub traffic: Vec<bc_client_core::traffic::Shown>,
    pub traffic_reach: f32,
    pub gone: Vec<u32>,
    /// The cars taken from the traffic, by their seeds: what kind each was (it keeps its look).
    pub jacked: Vec<(u32, street::Kind)>,
}

/// The seed a parked car's look comes from.
pub fn parked_seed(v: &Vehicle) -> u32 {
    let id = town_foot::parked_id(v);
    town::mix(id.0 as u32, id.1 as u32, 0xC0)
}

pub type TownMaterial = ExtendedMaterial<StandardMaterial, TownExt>;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct TownExt {
    #[uniform(100)]
    pub town: TownParams,
}

/// `town.wgsl`'s uniform.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct TownParams {
    /// rgb: the palette, linear (`station::PAINT`).
    pub paint: [Vec4; PAINTS],
    /// x: seconds; y: daylight; z: lamps lit; w: 1 to paint it in bands.
    pub day: Vec4,
    /// rgb: the haze, as it reaches the screen; w: its density, /m.
    pub haze: Vec4,
    /// rgb: the sky's light in the ceiling's glass, as it reaches the screen; w: the picture's
    /// gain (what takes the camera's grade's opening-up back out).
    pub sky: Vec4,
}

impl MaterialExtension for TownExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/town.wgsl".into()
    }
}

/// A piece of the town built as one mesh: `count` cells from `first` (a cell, a [`QUAD`] or a
/// district) at a detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Piece {
    first: u16,
    count: u16,
    detail: Detail,
}

impl Piece {
    const fn cell(cell: u16, detail: Detail) -> Self {
        Self { first: cell, count: 1, detail }
    }

    fn cells(self) -> Range<u16> {
        self.first..self.first + self.count
    }

    /// Whether it and `o` have a cell in common.
    fn meets(self, o: Piece) -> bool {
        self.first < o.first + o.count && o.first < self.first + self.count
    }

    /// The same cells at the other details, the likeliest first: what stands in for it, built, when
    /// nothing was shown there last frame.
    fn others(self) -> [Piece; 2] {
        let at = |detail| Piece { detail, ..self };
        match self.detail {
            Detail::Near => [at(Detail::Far), at(Detail::Haze)],
            Detail::Far => [at(Detail::Near), at(Detail::Haze)],
            Detail::Haze => [at(Detail::Far), at(Detail::Near)],
        }
    }
}

/// A piece as built: its entity, the frame it was last wanted, its triangles, and where round the
/// ring the view stood when it was last placed.
struct Built {
    entity: Entity,
    wanted: u64,
    tris: u32,
    placed: f32,
}

/// The town's root, its material, its pieces as built, the haze's walls where they end, and the
/// kerbs' cars.
#[derive(Resource, Default)]
pub struct Streamer {
    root: Option<Entity>,
    material: Handle<TownMaterial>,
    /// The pieces built: those shown, the rest kept (hidden) for when they're in sight again, up to
    /// [`cache`]'s many.
    pieces: HashMap<Piece, Built>,
    /// The piece being built a part at a time, if one is.
    building: Option<(Piece, CellBuild)>,
    /// This frame's pieces in sight, nearest first, each at the detail it's wanted at.
    sight: Vec<Piece>,
    /// This frame's pieces shown, and last frame's: what's shown where a piece is still to build.
    shown: Vec<Piece>,
    last: Vec<Piece>,
    /// The cells in sight that something shown covers.
    covered: u32,
    frame: u64,
    caps: Option<([Entity; 2], Handle<StandardMaterial>)>,
    /// The slowest frame's building lately.
    spent: Slowest,
    /// The pieces in sight not built yet.
    missing: u32,
    /// The frames the latest fill has taken: one a frame that began with a piece in sight still to
    /// build, counted afresh after a frame that began with none (`town_fill_frames`). Streamed
    /// within the frame's budget, a fill takes several; built all at once (a showcase's held
    /// clock), one.
    fill_frames: u32,
    filling: bool,
    /// Pieces built whose signs wait for the letters to be measured (`station::SignAtlas`).
    unsigned: Vec<(Piece, Entity)>,
}

/// One of the town's life's pools: its entities, who each shows, and which are shown.
struct LifePool {
    entities: Vec<Entity>,
    slots: Slots,
    shown: Vec<bool>,
}

/// Someone or something of the town's life to draw this frame: how far from the eye (m, squared),
/// who (the same every frame), where on the floor, the heading, and what.
#[derive(Clone, Copy)]
struct Seen<T> {
    d2: f32,
    id: u64,
    x: f32,
    s: f32,
    yaw: f32,
    what: T,
    /// Knocked down: lying where they fell. (A car: one of the Watch's Sirius hounds, a small body
    /// of its livery flown over the road.)
    lie: bool,
}

/// Faster than this, m/s, one of the town's people is drawn running (only those out of its crowd
/// ever are: `bc_sim::colony::crowd`).
const RUNNING: f32 = 2.8;
/// A Sirius hound as drawn: how much smaller than a car, and how far over the road, m.
const HOUND: (f32, f32) = (0.32, 3.2);

/// A car to draw: its look, its lights, its seed.
#[derive(Clone, Copy)]
struct CarLook {
    livery: Livery,
    lights: Lights,
    seed: u32,
}

/// The town's life as drawn: its pools (people near and far, cars near and far; made the first
/// time the town is shown, at the highest tier's caps), this frame's people and cars (kept
/// between frames: their buffers), and how many of each were drawn.
#[derive(Resource, Default)]
pub struct TownLife {
    pools: Option<[LifePool; 4]>,
    people: Vec<Seen<street::Person>>,
    cars: Vec<Seen<CarLook>>,
    ids: Vec<u64>,
    seats: Vec<usize>,
    /// Which of a pool's places this frame takes (kept between frames: its buffer).
    used: Vec<bool>,
    /// This frame's meshes of them, and how many of each are drawn: each mesh one draw (Bevy
    /// batches the rest, one material and the looks in their tags), its triangles that many times.
    meshes: Vec<(AssetId<Mesh>, u32)>,
    drawn: (u32, u32),
    /// The slowest frame's placing of them lately.
    spent: Slowest,
}

/// The slowest frame's spell at something lately, ms: this second's and the last's.
#[derive(Default, Clone, Copy, Debug)]
struct Slowest {
    this: f32,
    last: f32,
    window: f32,
}

impl Slowest {
    fn add(&mut self, ms: f32, dt: f32) {
        self.this = self.this.max(ms);
        self.window += dt;
        if self.window >= 1.0 {
            self.window = 0.0;
            self.last = std::mem::take(&mut self.this);
        }
    }

    fn ms(&self) -> f32 {
        self.this.max(self.last)
    }
}

/// What the town costs to draw, while the view is in it: what `?perf=1`'s readout shows and
/// `window.__bc` has (`town_*`, [`publish_stats`]).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct TownStats {
    /// The cells in sight shown, the pieces they're shown in, and their triangles.
    pub cells: u32,
    pub pieces: u32,
    pub tris: u32,
    /// The slowest frame's building of cells lately, and its placing of the town's people and cars,
    /// ms.
    pub build_ms: f32,
    pub life_ms: f32,
    /// The pieces in sight still to build, and the pieces built (shown or kept).
    pub missing: u32,
    pub cached: u32,
    /// The frames the latest fill of the pieces in sight took (or has taken so far).
    pub fill_frames: u32,
    /// The people and the cars drawn.
    pub people: u32,
    pub cars: u32,
    /// The meshes on the town's layer in view (of the camera or a shadow's cascade: Bevy's
    /// `ViewVisibility`, last frame's), each drawn once a pass it's in: the draws, near enough.
    pub draws: u32,
    /// The people's and the cars' triangles (not in `tris`), and the batches they're drawn in: one a
    /// mesh of theirs in use (a frame of a stride, a car's body), however many share it.
    pub life_tris: u32,
    pub life_batches: u32,
}

/// One of the town's life's entities.
#[derive(Component)]
struct TownLifeSlot;

fn to_mesh(m: bc_client_core::piece_mesh::PieceMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

/// The town's root and material (a start-up system; its cells are built as the view comes near).
pub fn setup_town(
    mut commands: Commands,
    mut materials: ResMut<Assets<TownMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut streamer: ResMut<Streamer>,
    gfx: Res<Gfx>,
) {
    let layer = RenderLayers::layer(CITY_LAYER);
    let root =
        commands.spawn((Name::new("town"), Transform::default(), Visibility::Hidden, layer.clone())).id();
    streamer.root = Some(root);
    streamer.material = materials.add(ExtendedMaterial {
        // The town lays its own haze down (in painted layers), so Bevy's distance fog is off for it.
        base: StandardMaterial { perceptual_roughness: 0.6, fog_enabled: false, ..default() },
        extension: TownExt {
            town: TownParams {
                paint: PAINT.map(|[r, g, b]| Vec4::new(r, g, b, 1.0)),
                day: Vec4::new(0.0, 1.0, 0.0, painted(&gfx)),
                haze: Vec4::new(0.4, 0.55, 0.8, 7e-4),
                sky: Vec4::new(0.75, 0.9, 1.1, 1.0),
            },
        },
    });
    // The haze's walls across the ring where the built cells end, ahead and behind.
    let haze = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    });
    let wall = meshes.add(Rectangle::new(2.0 * bc_sim::world::COLONY_HALF_WIDTH + 40.0, town::ROOF + 40.0));
    let cap = |commands: &mut Commands| {
        commands
            .spawn((
                Mesh3d(wall.clone()),
                MeshMaterial3d(haze.clone()),
                Transform::default(),
                NotShadowCaster,
                NotShadowReceiver,
                layer.clone(),
                ChildOf(root),
            ))
            .id()
    };
    streamer.caps = Some(([cap(&mut commands), cap(&mut commands)], haze));
}

/// `town.wgsl`'s `day.w`: 1 to paint the town in bands, as the rest of the colony is (`?look=plain`
/// leaves it unpainted, to compare).
fn painted(gfx: &Gfx) -> f32 {
    if gfx.paint { 1.0 } else { 0.0 }
}

/// How far the tier builds: the cells in full either side of the view's, the cells a face a quad,
/// and the cells at all.
fn reach(tier: GfxTier) -> (i32, i32, i32) {
    match tier {
        GfxTier::Low => (2, 5, 12),
        GfxTier::Medium => (3, 6, 20),
        GfxTier::High | GfxTier::Ultra => (NEAR_CELLS, MID_CELLS, REACH_CELLS),
    }
}

/// How many pieces are kept built: twice those in sight and a few, so the ones just passed, and a
/// piece's other detail, are there again without building them when the view turns back or comes
/// nearer (a near cell is half a megabyte at most, a district in the haze less).
fn cache(in_sight: usize) -> usize {
    2 * in_sight + 8
}

/// The pieces in sight of cell `here`, nearest first (with how many cells off each is): the cells
/// within `near` in full (and one further that was), each its own piece; past them out to `mid`,
/// a face a quad, by [`QUAD`]s of cells (a quad with a cell in full in it, as cells); and past
/// `mid` out to `far`, boxes in the haze, a district at a time (as quads where it's only partly
/// so, or runs on past the end of sight).
fn choose(
    here: u16,
    (near, mid, far): (i32, i32, i32),
    was_near: impl Fn(u16) -> bool,
    out: &mut Vec<(i32, Piece)>,
) {
    let cells = i32::from(town::CELLS);
    let here = i32::from(here);
    let step = |c: u16| {
        let d = (i32::from(c) - here).rem_euclid(cells);
        d.min(cells - d)
    };
    let full = |c: u16| {
        let s = step(c);
        s <= near || (s <= near + 1 && was_near(c))
    };
    let span =
        |cells: Range<u16>| cells.map(step).fold((i32::MAX, i32::MIN), |(a, b), s| (a.min(s), b.max(s)));
    let per = i32::from(town::CELLS_PER_DISTRICT);
    out.clear();
    for d in (here - far).div_euclid(per)..=(here + far).div_euclid(per) {
        let first = (d.rem_euclid(i32::from(town::DISTRICT_COUNT)) * per) as u16;
        let district = Piece { first, count: per as u16, detail: Detail::Haze };
        let (lo, hi) = span(district.cells());
        if lo > far {
            continue;
        }
        if lo > mid && hi <= far + SLACK {
            out.push((lo, district));
            continue;
        }
        for q in (first..first + per as u16).step_by(usize::from(QUAD)) {
            let quad = Piece { first: q, count: QUAD, detail: Detail::Haze };
            let (lo, _) = span(quad.cells());
            if lo > far {
                continue;
            }
            if lo > mid {
                out.push((lo, quad));
            } else if !quad.cells().any(full) {
                out.push((lo, Piece { detail: Detail::Far, ..quad }));
            } else {
                for c in quad.cells().filter(|&c| step(c) <= far) {
                    out.push((step(c), Piece::cell(c, if full(c) { Detail::Near } else { Detail::Far })));
                }
            }
        }
    }
    out.sort_by_key(|(step, _)| *step);
}

/// Keeps the pieces within sight built round the view, each turned to the view's frame, and the
/// town shown only while the view is in it. A piece is built a part at a time within the frame's
/// budget (nearest first, the one under way finished first); till it's built, what was shown there
/// last frame stays (or failing that, its cells at another detail), and it's kept once out of sight
/// (up to [`cache`]'s many, the longest unwanted going first); the pieces shown are turned only
/// when the view has moved.
#[allow(clippy::too_many_arguments)]
fn stream_town(
    mut commands: Commands,
    view: Res<TownView>,
    at: Res<TownAt>,
    gfx: Res<Gfx>,
    time: Res<Time<Real>>,
    mut streamer: ResMut<Streamer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
    (mut atlas, mut standard): (ResMut<crate::station::SignAtlas>, ResMut<Assets<StandardMaterial>>),
    mut chosen: Local<Vec<(i32, Piece)>>,
) {
    let Some(root) = streamer.root else { return };
    if view.is_changed()
        && let Ok((_, mut vis)) = tfs.get_mut(root)
    {
        let want = if view.active { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    if !view.active {
        if streamer.filling {
            streamer.filling = false;
        }
        return;
    }
    let s = &mut *streamer;
    s.frame += 1;
    let frame = s.frame;
    let (near, mid, far) = reach(gfx.tier);
    let here = town::cell_at(town::town_t(at.s));
    let budget = if view.sync { f32::INFINITY } else { (time.delta_secs() * 1000.0 * 0.2).clamp(2.0, 5.0) };
    let start = Instant::now();
    let place = |p: Piece| seen_from(at.s, 0.0, town::deck_s(town_mesh::cell_start(p.first)), 0.0);

    // The pieces in sight, nearest first; a cell shown in full last frame stays so a cell further.
    let shown_last = &s.shown;
    choose(here, (near, mid, far), |c| shown_last.contains(&Piece::cell(c, Detail::Near)), &mut chosen);
    s.sight.clear();
    s.sight.extend(chosen.iter().map(|(_, p)| *p));

    // What's missing, built within the budget a part at a time: the piece under way first, if it's
    // still in sight as it's being built, then the nearest.
    if s.building.as_ref().is_some_and(|(p, _)| !s.sight.contains(p)) {
        s.building = None;
    }
    let to_build = s.sight.iter().any(|p| !s.pieces.contains_key(p));
    if to_build {
        s.fill_frames = if s.filling { s.fill_frames + 1 } else { 1 };
    }
    s.filling = to_build;
    let layer = RenderLayers::layer(CITY_LAYER);
    let mut fresh = Vec::new();
    while start.elapsed().as_secs_f32() * 1000.0 <= budget {
        if s.building.is_none() {
            match s.sight.iter().find(|p| !s.pieces.contains_key(*p)) {
                Some(&p) => s.building = Some((p, CellBuild::range(p.first, p.count, p.detail))),
                None => break,
            }
        }
        let Some((_, job)) = s.building.as_mut() else { break };
        job.step();
        if !job.done() {
            continue;
        }
        let Some((key, job)) = s.building.take() else { break };
        let mesh = job.finish();
        let tris = mesh.triangles() as u32;
        let (translation, rotation) = place(key);
        let mut e = commands.spawn((
            Mesh3d(meshes.add(to_mesh(mesh))),
            MeshMaterial3d(s.material.clone()),
            Transform { translation, rotation, ..default() },
            Visibility::Hidden,
            layer.clone(),
            ChildOf(root),
        ));
        // (In the haze, past where the light's cascades reach: it neither casts nor takes a shadow.)
        if key.detail == Detail::Haze {
            e.insert((NotShadowCaster, NotShadowReceiver));
        }
        let entity = e.id();
        s.pieces.insert(key, Built { entity, wanted: 0, tris, placed: at.s });
        s.unsigned.push((key, entity));
        fresh.push((key, entity));
    }
    s.spent.add(start.elapsed().as_secs_f32() * 1000.0, time.delta_secs());

    // What's shown: each piece in sight that's built; where one isn't yet, what was shown there last
    // frame (or failing that, its cells built at another detail), and nothing else over it.
    std::mem::swap(&mut s.shown, &mut s.last);
    s.shown.clear();
    s.missing = 0;
    let (built, last, shown) = (&s.pieces, &s.last, &mut s.shown);
    for &p in s.sight.iter().filter(|p| !built.contains_key(*p)) {
        s.missing += 1;
        let mut stood = false;
        for &q in last.iter().filter(|q| q.meets(p) && built.contains_key(*q)) {
            stood = true;
            if !shown.contains(&q) {
                shown.push(q);
            }
        }
        if !stood
            && let Some(q) = p.others().into_iter().find(|q| built.contains_key(q))
            && !shown.contains(&q)
        {
            shown.push(q);
        }
    }
    let standing = shown.len();
    for &p in s.sight.iter().filter(|p| built.contains_key(*p)) {
        if !shown[..standing].iter().any(|q| q.meets(p)) {
            shown.push(p);
        }
    }
    // (One built this frame is shown from the start, so nothing's missing for a frame between it and
    // what stood in for it.)
    for (key, entity) in fresh {
        if shown.contains(&key) {
            commands.entity(entity).insert(Visibility::Inherited);
        }
    }
    let mut covered = [false; town::CELLS as usize];
    for p in shown.iter() {
        for c in p.cells() {
            covered[usize::from(c)] = true;
        }
    }
    let cells = i32::from(town::CELLS);
    s.covered =
        (-far..=far).filter(|d| covered[(i32::from(here) + d).rem_euclid(cells) as usize]).count() as u32;
    for p in s.sight.iter().chain(shown.iter()) {
        if let Some(b) = s.pieces.get_mut(p) {
            b.wanted = frame;
        }
    }

    // The pieces kept out of sight, the longest unwanted going first past the cache's size (they're
    // quick to build again).
    let cap = cache(s.sight.len());
    if s.pieces.len() > cap {
        let mut old: Vec<(Piece, u64)> =
            s.pieces.iter().filter(|(_, b)| b.wanted != frame).map(|(k, b)| (*k, b.wanted)).collect();
        old.sort_unstable_by_key(|(_, wanted)| *wanted);
        for (key, _) in old.into_iter().take(s.pieces.len() - cap) {
            if let Some(b) = s.pieces.remove(&key) {
                commands.entity(b.entity).despawn();
            }
        }
        let built = &s.pieces;
        s.unsigned.retain(|(key, entity)| built.get(key).is_some_and(|b| b.entity == *entity));
    }

    // Their signs, once the letters are measured: stencilled on each piece's own faces, so they go
    // when it does.
    if atlas.ready() && !s.unsigned.is_empty() {
        for (key, entity) in std::mem::take(&mut s.unsigned) {
            let signs = town_mesh::piece_signs(key.first, key.count);
            if !signs.is_empty() && s.pieces.get(&key).is_some_and(|b| b.entity == entity) {
                atlas.hang_on(&signs, entity, Some(CITY_LAYER), &mut commands, &mut meshes, &mut standard);
            }
        }
    }

    // Each shown where the view has it (turned again only when the view has moved), the rest hidden.
    for (key, b) in &mut s.pieces {
        let Ok((mut tf, mut vis)) = tfs.get_mut(b.entity) else { continue };
        let on = s.shown.contains(key);
        if on && b.placed != at.s {
            let (translation, rotation) = place(*key);
            tf.translation = translation;
            tf.rotation = rotation;
            b.placed = at.s;
        }
        vis.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }

    // The haze's walls, where the cells end.
    if let Some((caps, _)) = &s.caps {
        // (From the view's cell's start: the cells out to `far` either side of it are built.)
        let cell_s = town::deck_s(town_mesh::cell_start(here));
        let back = round_from(RING, at.s, cell_s);
        for (cap, d) in
            caps.iter().zip([back + (far + 1) as f32 * town::CELL, back - far as f32 * town::CELL])
        {
            if let Ok((mut tf, _)) = tfs.get_mut(*cap) {
                let (translation, rotation) = seen_from(0.0, 0.0, d, town::ROOF * 0.5);
                let scale = tf.scale;
                tf.set_if_neq(Transform { translation, rotation, scale });
            }
        }
    }
}

/// How a district's people dress (`life::outfit`): for the office, for work, for leisure, or as they
/// like.
fn dress(kind: DistrictKind) -> u8 {
    match kind {
        DistrictKind::Business | DistrictKind::Civic => 1,
        DistrictKind::Works | DistrictKind::Site => 2,
        DistrictKind::Park | DistrictKind::Residential => 3,
        DistrictKind::Midtown | DistrictKind::OldTown => 0,
    }
}

/// A car's look from its kind and its seed, as the colony's traffic's is (`life::livery`); the
/// Watch's own: white, a blue band, its roof's bar lit.
fn livery(kind: street::Kind, seed: u32, district: DistrictKind) -> Livery {
    use bc_sim::colony::traffic::Kind as K;
    use bc_sim::content::city::DistrictKind as D;
    let d = match district {
        DistrictKind::Business | DistrictKind::Civic => Some(D::Business),
        DistrictKind::Works | DistrictKind::Site => Some(D::Works),
        _ => None,
    };
    match kind {
        street::Kind::Watch => Livery { body: Body::Taxi, paint: 0, paint2: 29, wear: 0 },
        street::Kind::Car => plan::livery(K::Car, seed, 0, d),
        street::Kind::Van => plan::livery(K::Van, seed, 0, d),
        street::Kind::Taxi => plan::livery(K::Taxi, seed, 0, d),
    }
}

/// The town's life round the view: who and what `colony::street` has within the tier's reach at the
/// colony's tick, the kerbs' cars, the pilot's own; the nearest in full, the rest as far figures, up
/// to each pool's cap, each where the view has it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_life(
    mut commands: Commands,
    view: Res<TownView>,
    at: Res<TownAt>,
    own: Res<TownCars>,
    gfx: Res<Gfx>,
    clock: Res<crate::trams::TramClock>,
    hour: Res<ColonyDay>,
    crowd: Res<crate::people::Crowd>,
    scene: Option<Res<crate::life::LifeScene>>,
    streamer: Res<Streamer>,
    time: Res<Time<Real>>,
    mut life: ResMut<TownLife>,
    cams: Query<&Transform, (With<MainCamera>, Without<TownLifeSlot>)>,
    mut shown: Query<
        (&mut Transform, &mut MeshTag, &mut Mesh3d, &mut Visibility),
        (With<TownLifeSlot>, Without<MainCamera>),
    >,
) {
    let (Some(scene), Some(root)) = (scene, streamer.root) else { return };
    if !view.active {
        return;
    }
    let start = Instant::now();
    let life = &mut *life;
    // The pools, the first time: hidden, under the town's root (so they go when it does).
    let pools = life.pools.get_or_insert_with(|| {
        let most = plan::TIERS[3];
        [most.near_people, most.mid_people, most.near_cars, most.mid_cars].map(|l| {
            let entities = (0..l.cap)
                .map(|_| {
                    commands
                        .spawn((
                            TownLifeSlot,
                            Mesh3d(scene.vehicles[0][1].clone()),
                            MeshMaterial3d(scene.material.clone()),
                            MeshTag(0),
                            Transform::default(),
                            Visibility::Hidden,
                            NotShadowCaster,
                            RenderLayers::layer(CITY_LAYER),
                            ChildOf(root),
                        ))
                        .id()
                })
                .collect();
            LifePool { entities, slots: Slots::new(l.cap), shown: vec![false; l.cap] }
        })
    });
    // (The town is open ground, seen further across than the cylinder's streets: its people are
    // picked out half as far again.)
    let mut tier = if gfx.life { gfx.settings.life } else { plan::TIERS[0] };
    tier.near_people.reach *= 1.5;
    tier.mid_people.reach *= 1.5;
    let (tick, frac) = (clock.0, clock.1);
    let t = town::town_t(at.s);
    let lamps = hour.0.lamps;
    let district = town::district(town::cell_at(t) / town::CELLS_PER_DISTRICT).kind;
    // On foot the eye is where the pilot is; behind a car, it isn't in anyone's way.
    let on_foot = own.driving.is_none() && cams.single().is_ok_and(|c| c.translation.y < 2.2);
    let near = |x: f32, dt: f32| (x - at.x) * (x - at.x) + dt * dt;

    // The people: `colony::street`'s, but under the pilot and the other pilots.
    life.people.clear();
    if gfx.life {
        let reach = tier.mid_people.reach;
        let people = &mut life.people;
        street::each_person(at.x - reach, at.x + reach, t - reach, t + reach, tick, frac, &mut |p| {
            let d2 = near(p.x, p.t - t);
            let hidden = (on_foot && d2 < CLEAR * CLEAR)
                || crowd.0.iter().any(|(_, _, o)| {
                    !o.driving()
                        && (o.x - p.x).abs() < CLEAR
                        && round_from(RING, o.s, at.s + (p.t - t)).abs() < CLEAR
                });
            // (Nor anyone knocked down: they're drawn where they lie, below, and a moment after
            // they're up, till the server's word has them up out of the crowd; nor anyone out of it.)
            let down = own.down.iter().any(|d| d.id == p.id && d.until + crowd::UP_GRACE > tick);
            if d2 <= reach * reach && !hidden && !down && !own.loose.has(p.id) {
                // Stepped aside from the pilots' cars.
                let s = at.s + (p.t - t);
                let (mut x, mut ps) = (p.x, s);
                for car in &own.movers {
                    let (dx, ds) = town_foot::dodge(p.x, s, p.seed, car);
                    x += dx;
                    ps += ds;
                }
                people.push(Seen { d2, id: u64::from(p.id), x, s: ps, yaw: p.yaw, what: *p, lie: false });
            }
        });
        // Those out of the crowd, where the server has them.
        // (Not one run down a moment ago: they're lying where they fell, below.)
        let lying = |id: u32| own.down.iter().any(|d| d.id == id && d.until > tick);
        for l in own.loose.people.iter().filter(|l| !lying(l.id)) {
            let ds = round_from(RING, at.s, l.s);
            let d2 = near(l.x, ds);
            let hidden = on_foot && d2 < CLEAR * CLEAR;
            if d2 <= reach * reach && !hidden {
                let what = l.person(at.s, t);
                people.push(Seen {
                    d2,
                    id: u64::from(l.id),
                    x: l.x,
                    s: at.s + ds,
                    yaw: l.yaw,
                    what,
                    lie: false,
                });
            }
        }
        // Those knocked down, where they lie (and a moment after, standing there, till the server's
        // word has them up).
        let up = |d: &&bc_econ::wire::Down| {
            d.until > tick || (d.until + crowd::UP_GRACE > tick && !own.loose.has(d.id))
        };
        for d in own.down.iter().filter(|d| !d.away).filter(up) {
            let ds = round_from(RING, at.s, d.s);
            let d2 = near(d.x, ds);
            if d2 <= reach * reach {
                let what = street::Person {
                    id: d.id,
                    x: d.x,
                    t: t + ds,
                    yaw: d.yaw,
                    speed: 0.0,
                    stride: 0.0,
                    pose: street::Pose::Stand,
                    seed: street::seed_of(d.id),
                };
                people.push(Seen {
                    d2,
                    id: u64::from(d.id),
                    x: d.x,
                    s: at.s + ds,
                    yaw: d.yaw,
                    what,
                    lie: d.until > tick,
                });
            }
        }
        people.sort_by(|a, b| a.d2.total_cmp(&b.d2));
    }

    // The cars: the traffic, the kerbs' (but the ones driven off), the ones left, and the pilot's.
    life.cars.clear();
    let reach = tier.mid_cars.reach;
    let cars = &mut life.cars;
    if gfx.life {
        // Near the pilot the traffic's where this client has it; past that, where the tick does.
        let held = own.traffic.iter().map(|c| (c.car, at.s + round_from(RING, at.s, c.s)));
        let mut near_ones: Vec<(street::Car, f32)> = held.collect();
        let clear = (own.traffic_reach - 8.0).max(0.0);
        street::each_car(at.x - reach, at.x + reach, t - reach, t + reach, tick, frac, &mut |c| {
            let theirs = own.traffic_reach > 0.0 && ((c.t - t).abs() <= clear && (c.x - at.x).abs() <= clear);
            if !theirs && !own.gone.contains(&c.id) && !own.traffic.iter().any(|h| h.car.id == c.id) {
                near_ones.push((*c, at.s + (c.t - t)));
            }
        });
        for (c, s) in &near_ones {
            let look = CarLook {
                livery: livery(c.kind, c.seed, district),
                lights: Lights {
                    lamps: lamps > 0.15,
                    sign: c.kind == street::Kind::Watch || (c.kind == street::Kind::Taxi && c.seed & 64 != 0),
                    ..Lights::default()
                },
                seed: c.seed,
            };
            cars.push(Seen {
                d2: near(c.x, s - at.s),
                id: u64::from(c.id),
                x: c.x,
                s: *s,
                yaw: c.yaw,
                what: look,
                lie: false,
            });
        }
    }
    let standing = |v: &Vehicle, seed: u32, id: u64, lit: bool| {
        // (One taken from the traffic keeps the look it had there.)
        let kind = own.jacked.iter().find(|j| j.0 == seed).map_or(street::Kind::Car, |j| j.1);
        let look = CarLook {
            livery: livery(kind, seed, district),
            lights: Lights { parked: !lit, lamps: lit && lamps > 0.15, ..Lights::default() },
            seed,
        };
        Seen {
            d2: near(v.x, round_from(RING, at.s, v.s)),
            id,
            x: v.x,
            s: v.s,
            yaw: v.yaw,
            what: look,
            lie: false,
        }
    };
    town_foot::each_parked(at.x, at.s, reach.min(320.0), |v| {
        let id = town_foot::parked_id(&v);
        if !own.taken.contains(&id) {
            let key = 1 << 40 | (id.0 as u32 as u64) << 20 | id.1 as u64;
            cars.push(standing(&v, parked_seed(&v), key, false));
        }
    });
    for (k, (v, seed)) in own.left.iter().enumerate() {
        cars.push(standing(v, *seed, 2 << 40 | u64::from(*seed) << 4 | k as u64, false));
    }
    if let Some((v, seed)) = &own.driving {
        cars.push(Seen { d2: -1.0, ..standing(v, *seed, 3 << 40, true) });
    }
    // The Watch's units near the pilot: its livery, the bar on the roof flashing; and its hounds,
    // small and flown over the road, flashing too.
    for (k, (v, hound)) in
        own.watch.iter().map(|v| (v, false)).chain(own.hounds.iter().map(|v| (v, true))).enumerate()
    {
        let look = CarLook {
            livery: livery(street::Kind::Watch, 0, district),
            lights: Lights {
                lamps: true,
                sign: (tick / 6 + k as u32).is_multiple_of(2),
                ..Lights::default()
            },
            seed: k as u32,
        };
        let d2 = near(v.x, round_from(RING, at.s, v.s));
        cars.push(Seen { d2, id: 4 << 40 | k as u64, x: v.x, s: v.s, yaw: v.yaw, what: look, lie: hound });
    }
    cars.retain(|c| c.d2 <= reach * reach);
    cars.sort_by(|a, b| a.d2.total_cmp(&b.d2));

    // Each pool takes its share, nearest first: the near pool to its cap and reach, the far pool
    // the rest to its own.
    let levels = [tier.near_people, tier.mid_people, tier.near_cars, tier.mid_cars];
    let split = |d2s: &mut dyn Iterator<Item = f32>, near: plan::Level, far: plan::Level| {
        let (mut a, mut b) = (0, 0);
        for d2 in d2s {
            if a < near.cap && d2 <= near.reach * near.reach {
                a += 1;
            } else if b < far.cap && d2 <= far.reach * far.reach {
                b += 1;
            } else {
                break;
            }
        }
        (a, a + b)
    };
    let (pn, pe) = split(&mut life.people.iter().map(|p| p.d2), levels[0], levels[1]);
    let (cn, ce) = split(&mut life.cars.iter().map(|c| c.d2), levels[2], levels[3]);
    let ranges = [(0, pn), (pn, pe), (0, cn), (cn, ce)];
    life.meshes.clear();
    for (k, pool) in pools.iter_mut().enumerate() {
        let (a, b) = ranges[k];
        let is_car = k >= 2;
        let full = k % 2 == 0;
        life.ids.clear();
        if is_car {
            life.ids.extend(life.cars[a..b].iter().map(|c| c.id));
        } else {
            life.ids.extend(life.people[a..b].iter().map(|p| p.id));
        }
        pool.slots.assign(&life.ids, &mut life.seats);
        let used = &mut life.used;
        used.clear();
        used.resize(pool.entities.len(), false);
        for n in 0..(b - a) {
            let seat = life.seats[n];
            let Some(&entity) = pool.entities.get(seat) else { continue };
            let Ok((mut tf, mut tag, mut mesh, mut vis)) = shown.get_mut(entity) else { continue };
            let (x, s, yaw, scale, want_mesh, want_tag) = if is_car {
                let c = &life.cars[a + n];
                let m = &scene.vehicles[c.what.livery.body.index()][usize::from(!full)];
                (
                    c.x,
                    c.s,
                    c.yaw,
                    Vec3::splat(if c.lie { HOUND.0 } else { 1.0 }),
                    m,
                    plan::car_tag(&c.what.livery, c.what.lights, 1.0, c.what.seed),
                )
            } else {
                let p = &life.people[a + n];
                let w = &p.what;
                let o = plan::outfit(w.seed, dress(district));
                let (wide, tall) = plan::build(w.seed);
                // (Running, out of the crowd and scared off: the run's cycle.)
                let g = match w.pose {
                    street::Pose::Stand => Gait::Stand,
                    _ if w.speed > RUNNING => Gait::Run,
                    _ => Gait::Walk,
                };
                let f = life_mesh::bank_frame(g, full, w.stride);
                let m = if full {
                    &scene.near_figures[o.cut * life_mesh::bank_frames(true) + f]
                } else {
                    &scene.mid_figures[f]
                };
                (p.x, p.s, p.yaw, Vec3::new(wide, tall, wide), m, plan::figure_tag(&o, 1.0, w.seed))
            };
            // (Someone knocked down lies on their back where they fell; a hound flies over the road.)
            let lie = !is_car && life.people[a + n].lie;
            let hound = is_car && life.cars[a + n].lie;
            let up = if lie {
                0.22
            } else if hound {
                HOUND.1
            } else {
                0.0
            };
            let (translation, turn) = seen_from(at.s, x, s, up);
            let fallen =
                if lie { Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) } else { Quat::IDENTITY };
            tf.set_if_neq(Transform {
                translation,
                rotation: turn * Quat::from_rotation_y(yaw) * fallen,
                scale,
            });
            tag.set_if_neq(MeshTag(want_tag));
            if mesh.0 != *want_mesh {
                mesh.0 = want_mesh.clone();
            }
            match life.meshes.iter_mut().find(|(id, _)| *id == want_mesh.id()) {
                Some((_, n)) => *n += 1,
                None => life.meshes.push((want_mesh.id(), 1)),
            }
            vis.set_if_neq(Visibility::Inherited);
            used[seat] = true;
        }
        for (seat, on) in used.iter().enumerate() {
            if !on
                && pool.shown[seat]
                && let Ok((.., mut vis)) = shown.get_mut(pool.entities[seat])
            {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
        pool.shown.copy_from_slice(used);
    }
    life.drawn = (pe as u32, ce as u32);
    life.spent.add(start.elapsed().as_secs_f32() * 1000.0, time.delta_secs());
}

/// Where a pilot in the town (as the plaza has them) is drawn, and how their figure is turned
/// (`turned`: about their own up), with the view at `at`.
pub fn place_pose(p: &bc_proto::presence::PersonPose, turned: Quat, at: &TownAt) -> (Vec3, Quat) {
    let (pos, turn) = seen_from(at.s, p.x, p.s, p.h);
    (pos, turn * turned)
}

/// The town's light while the view is in it, by the colony's hour: the Sun as the louvres let it
/// down (overhead at noon, leaning along the ring before and after), the sky's light, the camera's
/// exposure, grade and bloom, the town's shader's haze and sky, and Bevy's fog to match for what
/// isn't the town's own (people, cars). Whatever space's light set on the way in is set back the
/// frame after (`sky.rs`, `city::switch_view`).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn light_town(
    mut commands: Commands,
    view: Res<TownView>,
    hour: Res<ColonyDay>,
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    streamer: Res<Streamer>,
    mut cams: Query<(Entity, Option<&mut ColorGrading>, Option<&mut Bloom>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight, &mut Transform), (With<Sun>, Without<MainCamera>)>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut materials: ResMut<Assets<TownMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
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
    let first = !std::mem::replace(&mut *lit, true);
    let d = hour.0;
    let look = city_hour::look(&d);
    // The camera's grade opens the picture up before the tonemapper (`Gfx::exposure_bias`); the
    // town's paints are given as they should reach the screen, so its shader takes that back out.
    let gain = 1.24 * 2f32.powf(-gfx.exposure_bias());
    // The light from overhead, leaning the way the ring turns in the morning and back in the evening.
    let elev = d.sun_elev.max(0.45);
    let lean = if d.phase < city_hour::NOON_PHASE { 1.0 } else { -1.0 };
    let along = Vec3::new(0.45, 0.0, -0.9 * lean).normalize();
    let to_sun = Vec3::Y * elev.sin() + along * elev.cos();
    for (sun, mut light, mut tf) in &mut suns {
        light.illuminance = look.lux;
        light.color = Color::linear_rgb(look.sun.x, look.sun.y, look.sun.z);
        light.shadow_maps_enabled = gfx.settings.shadows;
        *tf = Transform::default().looking_to(-to_sun, Vec3::X);
        if first || gfx.is_changed() {
            commands.entity(sun).insert(
                CascadeShadowConfigBuilder {
                    num_cascades: gfx.settings.cascades,
                    minimum_distance: 0.3,
                    first_cascade_far_bound: 40.0,
                    maximum_distance: 320.0,
                    ..default()
                }
                .build(),
            );
        }
    }
    // The sky's fill, from the whole lit ceiling and the haze: enough that what's turned from the
    // light is painted in the half-light's band, not the night's.
    ambient.color = Color::linear_rgb(look.sky.x, look.sky.y, look.sky.z);
    ambient.brightness = look.sky_nits * SKY_FILL;
    // The sky's light in the ceiling's glass, as it reaches the screen: a sky blue, turned towards
    // the hour's own colour (pale at noon, amber at the golden hour, magenta at dusk) the lower the
    // light, and deep blue by night. The haze is the same, a shade whiter (the far end of the ring
    // burns bright by day), and thicker than the cylinder's.
    let day = d.daylight.clamp(0.0, 1.0);
    let bright = day.sqrt();
    let hue = look.glow / look.glow.max_element().max(1.0);
    let night = Vec3::new(0.012, 0.016, 0.05);
    let sky = Vec3::new(0.16, 0.45, 0.95).lerp(hue, 0.25 + 0.45 * (1.0 - day)) * bright + night;
    let haze = sky * 0.85 + Vec3::splat(0.12 * bright);
    let density = look.density * HAZE * if gfx.tier == GfxTier::Low { 1.7 } else { 1.0 };
    // (Bevy's fog, for what isn't the town's own, in the same haze: dimmed as the town's is.)
    let fog = haze * gain;
    for (cam, grading, bloom) in &mut cams {
        commands.entity(cam).insert((
            Exposure { ev100: look.ev },
            ClusterConfig::default(),
            DistanceFog {
                color: Color::linear_rgb(fog.x, fog.y, fog.z),
                falloff: FogFalloff::Exponential { density },
                ..default()
            },
        ));
        commands.entity(cam).remove::<EnvironmentMapLight>();
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
    }
    if let Some(mut m) = materials.get_mut(&streamer.material) {
        let seconds = (time.now % 3_600.0) as f32;
        m.extension.town.day = Vec4::new(seconds, d.daylight, d.lamps, painted(&gfx));
        m.extension.town.haze = haze.extend(density);
        m.extension.town.sky = sky.extend(gain);
    }
    if let Some((_, cap)) = &streamer.caps
        && let Some(mut m) = standard.get_mut(cap)
    {
        // Where the floor is lost: the haze itself, a shade brighter.
        let c = haze * 1.08 * gain;
        m.base_color = Color::linear_rgb(c.x, c.y, c.z);
    }
}

/// [`TownStats`], and `window.__bc`'s `town_cells`, `town_pieces`, `town_tris`, `town_ms` (the build's),
/// `town_life_ms`, `town_missing`, `town_cached`, `town_fill_frames`, `town_people`, `town_cars` and
/// `town_draws` from it.
fn publish_stats(
    view: Res<TownView>,
    streamer: Res<Streamer>,
    life: Res<TownLife>,
    meshes: Query<(&ViewVisibility, &RenderLayers), With<Mesh3d>>,
    scene: Option<Res<crate::life::LifeScene>>,
    mut stats: ResMut<TownStats>,
    status: Option<ResMut<crate::dev_hooks::DevStatus>>,
) {
    let now = if view.active {
        let shown = streamer.shown.iter().filter_map(|p| streamer.pieces.get(p));
        let (pieces, tris) = shown.fold((0u32, 0u32), |(n, t), b| (n + 1, t + b.tris));
        let town = RenderLayers::layer(CITY_LAYER);
        let draws = meshes.iter().filter(|(seen, layers)| seen.get() && layers.intersects(&town)).count();
        let tris_of = |id: AssetId<Mesh>| scene.as_ref().and_then(|s| s.tris.get(&id).copied()).unwrap_or(0);
        let life_tris = life.meshes.iter().map(|&(id, n)| n * tris_of(id)).sum();
        TownStats {
            cells: streamer.covered,
            pieces,
            tris,
            build_ms: streamer.spent.ms(),
            life_ms: life.spent.ms(),
            missing: streamer.missing,
            cached: streamer.pieces.len() as u32,
            fill_frames: streamer.fill_frames,
            people: life.drawn.0,
            cars: life.drawn.1,
            draws: draws as u32,
            life_tris,
            life_batches: life.meshes.len() as u32,
        }
    } else {
        TownStats::default()
    };
    stats.set_if_neq(now);
    let Some(mut status) = status else { return };
    status.set("town_cells", now.cells);
    status.set("town_pieces", now.pieces);
    status.set("town_tris", now.tris);
    status.set("town_ms", now.build_ms);
    status.set("town_life_ms", now.life_ms);
    status.set("town_missing", now.missing);
    status.set("town_cached", now.cached);
    status.set("town_fill_frames", now.fill_frames);
    status.set("town_people", now.people);
    status.set("town_cars", now.cars);
    status.set("town_draws", now.draws);
    status.set("town_life_tris", now.life_tris);
    status.set("town_life_batches", now.life_batches);
}
