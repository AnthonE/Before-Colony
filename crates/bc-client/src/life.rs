//! The city's life, drawn: its traffic (`bc_sim::colony::traffic`) and its people (`walkers`) where
//! the closed forms have them at the [`TramClock`], the same on every screen. Nothing about them
//! comes over the wire, and nobody can touch them: a pilot's own car and body pass through them.
//!
//! - **Pools.** Entities made at start-up (at Ultra's caps, hidden), five pools: people near and
//!   far, cars near and far, and cars parked for good. Each frame `bc_client_core::life::Choice`
//!   picks who each pool draws round the camera, nearest first up to the tier's caps; an entity
//!   keeps the car or person it showed while they're still picked (`life::Slots`).
//! - **One material.** What each shows (its paints, its lights, its fade) rides in its `MeshTag`
//!   (`shaders/life.wgsl`), so a pool is a few draws whatever's in it.
//! - **Posed by swapping meshes.** A figure's mesh is its gait's frame for its stride, from a bank
//!   made at start-up (`life_mesh`): no skinning, no vertex shader.
//! - **Out of the way.** Nothing within a few metres of a suit standing on the city, nor at the
//!   camera, and a civilian dissolves under a pilot on foot (or, in the showcase, gives way to the
//!   named pilot it is: [`LifeBorrowed`]).

use bc_client_core::life::{self as plan, CARS_NEAR, Choice, Clears, Eye, PEOPLE_NEAR, Slots};
use bc_client_core::life_mesh::{self, Body, CUTS, Gait, LifeMesh};
use bc_sim::colony::frame::{CityPos, Under, from_colony, local_frame};
use bc_sim::colony::time::day;
use bc_sim::colony::walkers::Pose;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};

use crate::camera::MainCamera;
use crate::city::{CITY_LAYER, CityView, Placed, RenderOrigin, colony_point};
use crate::gfx::Gfx;
use crate::people::Crowd;
use crate::trams::TramClock;
use crate::view::{SuitDrive, VisTime};

pub type LifeMaterial = ExtendedMaterial<StandardMaterial, LifeExt>;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct LifeExt {
    #[uniform(100)]
    pub life: LifeParams,
}

/// Set once: nothing in it changes after start-up, so the material never does (a change would
/// re-specialise every entity using it). What changes rides in each entity's `MeshTag`.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct LifeParams {
    /// rgb: linear base colour; a: perceptual roughness (`bc_client_core::life::PALETTE`).
    pub palette: [Vec4; 64],
}

impl MaterialExtension for LifeExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/life.wgsl".into()
    }
}

/// The city's people the showcase has named (`walkers::Walker::id`): drawn as its pilots, so life
/// leaves them out. Empty in game mode.
#[derive(Resource, Default)]
pub struct LifeBorrowed(pub Vec<u32>);

pub struct LifePlugin;

impl Plugin for LifePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/life_lib.wgsl");
        embedded_asset!(app, "shaders/life.wgsl");
        app.add_plugins(MaterialPlugin::<LifeMaterial>::default())
            .init_resource::<LifeBorrowed>()
            .init_resource::<LifeFrame>()
            .add_systems(Startup, setup_life)
            .add_systems(Update, (draw_life, publish_life).chain().in_set(crate::view::Vis::Suits));
    }
}

/// One of the pools' entities.
#[derive(Component)]
struct LifeSlot;

/// One pool: its entities, who each shows, and which are shown.
struct Pool {
    entities: Vec<Entity>,
    slots: Slots,
    shown: Vec<bool>,
}

/// The pools and the meshes they pick from.
#[derive(Resource)]
struct LifeScene {
    pools: [Pool; 5],
    /// Near figures by cut, then the bank's frame (`life_mesh::bank_frame`); far figures by frame.
    near_figures: Vec<Handle<Mesh>>,
    mid_figures: Vec<Handle<Mesh>>,
    /// Vehicles by body: near and mid.
    vehicles: [[Handle<Mesh>; 2]; 5],
    /// Keeps the library loaded, so `life.wgsl` can import it.
    _lib: Handle<Shader>,
}

/// What life worked out this frame, its buffers kept; what it drew; the slowest frame lately.
#[derive(Resource, Default)]
struct LifeFrame {
    choice: Choice,
    ids: Vec<u64>,
    seats: Vec<usize>,
    used: Vec<bool>,
    suits: Vec<(f32, f32)>,
    pilots: Vec<(f32, f32)>,
    people: u32,
    cars: u32,
    /// The slowest frame's time in life over the last second, ms, and the one before.
    worst: (f32, f32),
    window: f32,
}

fn to_mesh(m: LifeMesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

fn palette() -> [Vec4; 64] {
    plan::PALETTE.map(|(r, g, b, rough)| {
        let c = Color::srgb(r, g, b).to_linear();
        Vec4::new(c.red, c.green, c.blue, rough)
    })
}

/// Makes the meshes, the material and the pools (hidden).
fn setup_life(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LifeMaterial>>,
    assets: Res<AssetServer>,
) {
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.5, ..default() },
        extension: LifeExt { life: LifeParams { palette: palette() } },
    });
    let mut near_figures = Vec::new();
    for cut in 0..CUTS {
        for (g, u) in life_mesh::bank(true) {
            near_figures.push(meshes.add(to_mesh(life_mesh::civilian_mesh(cut, g, u))));
        }
    }
    let mid_figures = life_mesh::bank(false)
        .into_iter()
        .map(|(g, u)| meshes.add(to_mesh(life_mesh::mid_figure(g, u))))
        .collect();
    let vehicles =
        Body::ALL.map(|b| [true, false].map(|near| meshes.add(to_mesh(life_mesh::vehicle_mesh(b, near)))));
    let most = plan::TIERS[3].levels();
    let pools = most.map(|l| {
        let entities = (0..l.cap)
            .map(|_| {
                commands
                    .spawn((
                        LifeSlot,
                        Mesh3d(vehicles[0][1].clone()),
                        MeshMaterial3d(material.clone()),
                        MeshTag(0),
                        Placed(Default::default()),
                        Transform::default(),
                        Visibility::Hidden,
                        NotShadowCaster,
                        RenderLayers::layer(CITY_LAYER),
                    ))
                    .id()
            })
            .collect();
        Pool { entities, slots: Slots::new(l.cap), shown: vec![false; l.cap] }
    });
    commands.insert_resource(LifeScene {
        pools,
        near_figures,
        mid_figures,
        vehicles,
        _lib: assets.load("embedded://bc_client/shaders/life_lib.wgsl"),
    });
}

type Shown = (
    &'static mut Placed,
    &'static mut Transform,
    &'static mut MeshTag,
    &'static mut Mesh3d,
    &'static mut Visibility,
);

/// Picks who's drawn round the camera and puts each pool's entities on them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_life(
    view: Res<CityView>,
    clock: Res<TramClock>,
    gfx: Res<Gfx>,
    origin: Res<RenderOrigin>,
    time: Res<VisTime>,
    crowd: Res<Crowd>,
    borrowed: Res<LifeBorrowed>,
    scene: Option<ResMut<LifeScene>>,
    mut frame: ResMut<LifeFrame>,
    cams: Query<&Transform, (With<MainCamera>, Without<LifeSlot>)>,
    suits: Query<&SuitDrive>,
    mut shown: Query<Shown, With<LifeSlot>>,
) {
    let Some(mut scene) = scene else { return };
    let start = Instant::now();
    let frame = &mut *frame;
    let eye = cams.single().ok().map(|c| (origin.0 + c.translation.as_dvec3(), c.forward().as_vec3()));
    let on = eye.and_then(|(at, look)| match from_colony(at.as_vec3()) {
        Under::Land(c) => Some((c, look)),
        Under::Window { .. } => None,
    });
    let Some((at, look)) = on.filter(|_| view.active && gfx.life) else {
        for pool in &mut scene.pools {
            hide_all(pool, &mut shown);
            pool.slots.assign(&[], &mut frame.seats);
        }
        frame.people = 0;
        frame.cars = 0;
        return;
    };
    let strip = at.strip;
    // The look in the strip's frame where the eye is (`x` along, `y` up, `z` = −s).
    let forward = local_frame(strip, at.s).inverse() * look;
    let eye = Eye { strip, s: at.s, x: at.x, h: at.h, forward };
    // Round the suits standing on this strip, and under the pilots on foot on it.
    frame.suits.clear();
    for d in &suits {
        if let Under::Land(c) = from_colony(d.pos)
            && c.strip == strip
            && c.h < 30.0
        {
            frame.suits.push((c.s, c.x));
        }
    }
    frame.pilots.clear();
    frame.pilots.extend(
        crowd.0.iter().filter(|(_, _, p)| p.strip == strip && !p.driving()).map(|(_, _, p)| (p.s, p.x)),
    );
    // On foot, the pilot's own place too: nobody walks through the camera.
    if at.h < 2.5 {
        frame.pilots.push((at.s, at.x));
    }
    let clears = Clears { suits: &frame.suits, pilots: &frame.pilots, borrowed: &borrowed.0 };
    let tier = gfx.settings.life;
    frame.choice.choose(&eye, &tier, clock.0, clock.1, &clears);
    let lamps = day(clock.0, clock.1).lamps;
    // The indicators' beat, on the visuals' clock (so the showcase's shots are the same each run).
    let beat = plan::beat(time.now);
    let scene = &mut *scene;
    let (mut people, mut cars) = (0, 0);
    for k in 0..5 {
        let pool = &mut scene.pools[k];
        let picks = &frame.choice.picks[k];
        let car_pool = k >= CARS_NEAR;
        frame.ids.clear();
        if car_pool {
            frame.ids.extend(picks.iter().map(|i| frame.choice.cars[*i].id));
        } else {
            frame.ids.extend(picks.iter().map(|i| frame.choice.people[*i].id));
        }
        pool.slots.assign(&frame.ids, &mut frame.seats);
        frame.used.clear();
        frame.used.resize(pool.entities.len(), false);
        for (n, i) in picks.iter().enumerate() {
            let seat = frame.seats[n];
            let Some(&entity) = pool.entities.get(seat) else { continue };
            let Ok((mut placed, mut tf, mut tag, mut mesh, mut vis)) = shown.get_mut(entity) else {
                continue;
            };
            let (at, rotation, scale, want_mesh, want_tag) = if car_pool {
                let c = &frame.choice.cars[*i];
                let livery = plan::car_livery(c, strip);
                // The nose dips braking and lifts pulling away.
                let pitch = (-0.007 * c.accel).clamp(-0.03, 0.03);
                let rot =
                    local_frame(strip, c.s) * Quat::from_rotation_y(c.yaw) * Quat::from_rotation_x(pitch);
                let near = k == CARS_NEAR;
                let m = &scene.vehicles[livery.body.index()][usize::from(!near)];
                let t = plan::car_tag(&livery, plan::lights(c, lamps, beat), 1.0, c.seed);
                cars += 1;
                (CityPos::new(strip, c.x, c.s, 0.0), rot, Vec3::ONE, m, t)
            } else {
                let p = &frame.choice.people[*i];
                let w = &p.w;
                let o = plan::person_outfit(p, strip);
                let (wide, tall) = plan::build(w.seed);
                // A sitter's hips stay on the seat.
                let tall = if w.pose == Pose::Sit { 1.0 } else { tall };
                let g = Gait::of(w.pose);
                let near = k == PEOPLE_NEAR;
                let f = life_mesh::bank_frame(g, near, w.stride);
                let m = if near {
                    &scene.near_figures[o.cut * life_mesh::bank_frames(true) + f]
                } else {
                    &scene.mid_figures[f]
                };
                let rot = local_frame(strip, w.s) * Quat::from_rotation_y(w.yaw);
                people += 1;
                (
                    CityPos::new(strip, w.x, w.s, w.h),
                    rot,
                    Vec3::new(wide, tall, wide),
                    m,
                    plan::figure_tag(&o, w.fade, w.seed),
                )
            };
            placed.set_if_neq(Placed(colony_point(at)));
            tf.set_if_neq(Transform { translation: tf.translation, rotation, scale });
            tag.set_if_neq(MeshTag(want_tag));
            if mesh.0 != *want_mesh {
                mesh.0 = want_mesh.clone();
            }
            vis.set_if_neq(Visibility::Inherited);
            frame.used[seat] = true;
        }
        for (seat, on) in frame.used.iter().enumerate() {
            if !on
                && pool.shown[seat]
                && let Ok((.., mut vis)) = shown.get_mut(pool.entities[seat])
            {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
        pool.shown.copy_from_slice(&frame.used);
    }
    frame.people = people;
    frame.cars = cars;
    let ms = start.elapsed().as_secs_f32() * 1000.0;
    frame.worst.0 = frame.worst.0.max(ms);
    frame.window += time.dt;
    if frame.window >= 1.0 {
        frame.window = 0.0;
        frame.worst = (0.0, frame.worst.0);
    }
}

/// Hides a pool's entities.
fn hide_all(pool: &mut Pool, shown: &mut Query<Shown, With<LifeSlot>>) {
    for seat in 0..pool.entities.len() {
        if pool.shown[seat] {
            if let Ok((.., mut vis)) = shown.get_mut(pool.entities[seat]) {
                vis.set_if_neq(Visibility::Hidden);
            }
            pool.shown[seat] = false;
        }
    }
}

/// `window.__bc.ambient_people`, `ambient_cars` and `life_ms`: the figures and cars drawn, and the
/// most a frame spent on them over the last second or so (ms).
fn publish_life(frame: Res<LifeFrame>, status: Option<ResMut<crate::dev_hooks::DevStatus>>) {
    let Some(mut status) = status else { return };
    status.set("ambient_people", frame.people);
    status.set("ambient_cars", frame.cars);
    status.set("life_ms", frame.worst.0.max(frame.worst.1));
}
