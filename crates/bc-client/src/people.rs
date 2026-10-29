//! The other pilots in the colony's city, drawn: a figure in a flight suit of each pilot's own
//! colour (`bc_client_core::figure`, `plaza::suit_colour`), striding as fast as they move, and
//! their name over them when they're near; driving, their car, scooter or Markab in their colour
//! (a Markab turned whichever way its pose has it, its fan's skirt lit while the fan runs, and its
//! jets' plumes as they fire). The Elo Pros store's traders on the Exchange floor wear an avatar
//! instead (`bc_client_core::avatar`: a model loaded once someone wearing it is in view, standing
//! about with a little sway, or playing its own idle). Who's where is the [`Crowd`]: the plaza's
//! people in game mode (`fill_crowd`), or a showcase's.

use std::collections::HashMap;

use bc_client_core::figure::{self, Paint, Piece};
use bc_client_core::markab::attitude;
use bc_client_core::markab_mesh::{self, Paint as Coat};
use bc_client_core::plaza::suit_colour;
use bc_proto::presence::{BASE_DECK, JET_BOOST, JET_LIFT, PersonPose, RIDE_CAR, RIDE_SCOOTER};
use bc_sim::colony::frame::{CityPos, local_frame};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::camera::MainCamera;
use crate::city::{CITY_LAYER, CityView, Placed, RenderOrigin, colony_point};
use crate::hud::UiFont;

/// Names show within this distance, m.
const NAME_REACH: f32 = 40.0;
/// The pilot's own place in the crowd (driving: their vehicle).
pub const OWN: u16 = u16::MAX;

/// The people to draw: slot, name and pose.
#[derive(Resource, Default)]
pub struct Crowd(pub Vec<(u16, String, PersonPose)>);

pub struct PeoplePlugin;

impl Plugin for PeoplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Crowd>()
            .init_resource::<AvatarLib>()
            .add_systems(Startup, (setup_people, setup_benches))
            .add_systems(Update, draw_people.in_set(crate::view::Vis::Suits))
            .add_systems(Update, name_tags.in_set(crate::view::Vis::Hud));
    }
}

/// The figure's meshes (a piece's paints apart) and its trim's and visor's materials; a car's
/// and a scooter's (body, glass, tyres).
#[derive(Resource)]
struct PeopleScene {
    meshes: Vec<(usize, Paint, Handle<Mesh>)>,
    trim: Handle<StandardMaterial>,
    visor: Handle<StandardMaterial>,
    car: [Handle<Mesh>; 3],
    scooter: [Handle<Mesh>; 3],
    glass: Handle<StandardMaterial>,
    tyre: Handle<StandardMaterial>,
    /// A Markab's parts (its body takes its driver's paint), and its jets' plume.
    markab: Vec<(Coat, Handle<Mesh>, Option<Handle<StandardMaterial>>)>,
    plume: (Handle<Mesh>, Handle<StandardMaterial>),
}

/// A Markab's parts but its body, which is its driver's colour: the materials, by part.
fn markab_coat(
    coat: Coat,
    materials: &mut Assets<StandardMaterial>,
    glass: &Handle<StandardMaterial>,
    tyre: &Handle<StandardMaterial>,
) -> Option<Handle<StandardMaterial>> {
    let lit = |materials: &mut Assets<StandardMaterial>, base: Color| {
        materials.add(StandardMaterial { base_color: base, unlit: true, ..default() })
    };
    match coat {
        Coat::Body => None,
        Coat::Glass => Some(glass.clone()),
        Coat::Tyre => Some(tyre.clone()),
        Coat::Trim => Some(materials.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.055, 0.06),
            perceptual_roughness: 0.35,
            metallic: 0.5,
            ..default()
        })),
        Coat::Skirt => Some(lit(materials, Color::srgb(0.45, 0.95, 1.0))),
        Coat::Lamp => Some(lit(materials, Color::srgb(1.0, 0.97, 0.9))),
        Coat::Tail => Some(lit(materials, Color::srgb(1.0, 0.12, 0.08))),
    }
}

/// A shape of `bc_client_core::markab_mesh` as a mesh.
fn shape_mesh(s: &markab_mesh::Shape) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, s.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, s.normals.clone())
        .with_inserted_indices(Indices::U32(s.indices.clone()))
}

/// The Arrival's benches by its door (`bc_sim::colony::city::arrival_seats`), two seats each: a
/// seat and a back, in the seats' own frame (facing +z, the way a pilot sits on them).
fn setup_benches(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.29, 0.18),
        perceptual_roughness: 0.7,
        ..default()
    });
    let iron = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.13, 0.14),
        perceptual_roughness: 0.45,
        metallic: 0.6,
        ..default()
    });
    let layer = RenderLayers::layer(CITY_LAYER);
    let slab = meshes.add(Cuboid::new(2.4, 0.07, 0.5));
    let back = meshes.add(Cuboid::new(2.4, 0.45, 0.06));
    let leg = meshes.add(Cuboid::new(0.07, 0.45, 0.45));
    let seats = bc_sim::colony::city::arrival_seats();
    for pair in seats.chunks(2) {
        let (a, b) = (pair[0], pair[pair.len() - 1]);
        let at = CityPos::new(a.strip, 0.5 * (a.x + b.x), 0.5 * (a.s + b.s), 0.0);
        let root = commands
            .spawn((
                Transform::from_rotation(local_frame(a.strip, at.s) * Quat::from_rotation_y(a.yaw)),
                Visibility::Inherited,
                Placed(colony_point(at)),
                layer.clone(),
            ))
            .id();
        let parts = [
            (slab.clone(), wood.clone(), Vec3::new(0.0, 0.45, 0.0)),
            (back.clone(), wood.clone(), Vec3::new(0.0, 0.8, -0.24)),
            (leg.clone(), iron.clone(), Vec3::new(-1.05, 0.22, 0.0)),
            (leg.clone(), iron.clone(), Vec3::new(1.05, 0.22, 0.0)),
        ];
        for (mesh, material, offset) in parts {
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(offset),
                layer.clone(),
                ChildOf(root),
            ));
        }
    }
}

/// A car (facing +z, wheels on the ground): its body, its glass, its tyres.
fn car_meshes() -> [Mesh; 3] {
    use bc_client_core::vehicle::{Kind, spec};
    let sp = spec(Kind::Car);
    let (hl, hw) = (0.5 * sp.length, 0.5 * sp.width);
    let body = [
        (Vec3::new(0.0, 0.62, 0.0), Vec3::new(hw, 0.33, hl)),
        (Vec3::new(0.0, sp.height - 0.04, -0.2), Vec3::new(hw - 0.12, 0.04, 1.05)),
    ];
    let glass = [(Vec3::new(0.0, 1.17, -0.2), Vec3::new(hw - 0.1, 0.22, 1.15))];
    let wheel = |x: f32, z: f32| (Vec3::new(x, 0.32, z), Vec3::new(0.12, 0.32, 0.32));
    let tyres =
        [wheel(-hw + 0.1, 1.35), wheel(hw - 0.1, 1.35), wheel(-hw + 0.1, -1.35), wheel(hw - 0.1, -1.35)];
    [crate::trams::boxes(&body), crate::trams::boxes(&glass), crate::trams::boxes(&tyres)]
}

/// A scooter (facing +z): its deck and column, a screen, its two wheels.
fn scooter_meshes() -> [Mesh; 3] {
    let body = [
        (Vec3::new(0.0, 0.32, -0.1), Vec3::new(0.22, 0.08, 0.7)),
        (Vec3::new(0.0, 0.75, 0.62), Vec3::new(0.05, 0.45, 0.05)),
        (Vec3::new(0.0, 1.18, 0.62), Vec3::new(0.32, 0.03, 0.04)),
    ];
    let glass = [(Vec3::new(0.0, 1.05, 0.7), Vec3::new(0.18, 0.15, 0.01))];
    let tyres = [
        (Vec3::new(0.0, 0.22, 0.72), Vec3::new(0.06, 0.22, 0.22)),
        (Vec3::new(0.0, 0.22, -0.72), Vec3::new(0.06, 0.22, 0.22)),
    ];
    [crate::trams::boxes(&body), crate::trams::boxes(&glass), crate::trams::boxes(&tyres)]
}

/// A pilot drawn: their slot, their stride, where they were last frame (to stride by it), their
/// pieces' joints, their name tag, and what they might drive (a car, a scooter).
#[derive(Component)]
struct Figure {
    id: u16,
    /// The floor they were drawn on (`PersonPose::strip`: a strip, or the base's deck): the city's
    /// figures are on its render layer, the deck's on the world's.
    floor: u8,
    /// Their callsign, as the radio names them.
    name: String,
    phase: f32,
    last: Option<(f32, f32, f32)>,
    joints: [Entity; 12],
    tag: Entity,
    car: Entity,
    scooter: Entity,
    /// A Markab, its fan's skirt light, and its jets' plumes (the lift's four, then the tail's).
    markab: Entity,
    skirt: Vec<Entity>,
    plumes: [Entity; 8],
    /// A store trader's avatar standing in for the figure (which stays hidden), its turn and scale
    /// as it stands (`avatar::Look`), and its sway's phase.
    avatar: Option<(Entity, Quat, f32, f32)>,
}

/// The avatars' models and idle animations, each loaded the first time someone wearing it is
/// drawn (`bc_client_core::avatar::LOOKS`, by index).
#[derive(Resource, Default)]
struct AvatarLib {
    scenes: Vec<Option<Handle<WorldAsset>>>,
    idles: Vec<Option<(Handle<AnimationGraph>, AnimationNodeIndex)>>,
}

impl AvatarLib {
    fn scene(
        &mut self,
        look: usize,
        assets: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> Handle<WorldAsset> {
        use bc_client_core::avatar::LOOKS;
        if self.scenes.len() < LOOKS.len() {
            self.scenes.resize(LOOKS.len(), None);
            self.idles.resize(LOOKS.len(), None);
        }
        let l = LOOKS[look];
        if self.idles[look].is_none()
            && let Some(i) = l.idle
        {
            let (graph, node) =
                AnimationGraph::from_clip(assets.load(GltfAssetLabel::Animation(i).from_asset(l.path)));
            self.idles[look] = Some((graphs.add(graph), node));
        }
        self.scenes[look]
            .get_or_insert_with(|| assets.load(GltfAssetLabel::Scene(0).from_asset(l.path)))
            .clone()
    }
}

/// An avatar standing in a figure's place: which look, for its idle.
#[derive(Component)]
struct AvatarOf(usize);

/// An avatar's model is in: every part of it on the avatar's render layer (the city's, or the
/// world's on the base's deck), and its idle playing, if it has one.
fn avatar_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    avatars: Query<(&AvatarOf, &RenderLayers)>,
    mut players: Query<&mut AnimationPlayer>,
    lib: Res<AvatarLib>,
) {
    let Ok((AvatarOf(look), layer)) = avatars.get(ready.entity) else { return };
    let idle = lib.idles.get(*look).cloned().flatten();
    for e in children.iter_descendants(ready.entity) {
        commands.entity(e).insert(layer.clone());
        if let (Some((graph, node)), Ok(mut player)) = (&idle, players.get_mut(e)) {
            player.play(*node).repeat();
            commands.entity(e).insert(AnimationGraphHandle(graph.clone()));
        }
    }
}

fn setup_people(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut out = Vec::new();
    for (i, piece) in Piece::ALL.iter().enumerate() {
        let m = figure::mesh(*piece);
        for paint in [Paint::Suit, Paint::Trim, Paint::Visor] {
            // This paint's triangles alone.
            let mut map: HashMap<u32, u32> = HashMap::new();
            let (mut pos, mut nrm, mut idx) = (Vec::new(), Vec::new(), Vec::new());
            for tri in m.indices.chunks(3) {
                if tri.iter().any(|v| m.paint[*v as usize] != paint) {
                    continue;
                }
                for v in tri {
                    let k = *map.entry(*v).or_insert_with(|| {
                        pos.push(m.positions[*v as usize]);
                        nrm.push(m.normals[*v as usize]);
                        pos.len() as u32 - 1
                    });
                    idx.push(k);
                }
            }
            if idx.is_empty() {
                continue;
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
                .with_inserted_indices(Indices::U32(idx));
            out.push((i, paint, meshes.add(mesh)));
        }
    }
    let trim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.86, 0.87, 0.85),
        perceptual_roughness: 0.55,
        ..default()
    });
    let visor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.03, 0.04, 0.05),
        perceptual_roughness: 0.08,
        metallic: 0.6,
        ..default()
    });
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.12, 0.18, 0.22, 0.4),
        perceptual_roughness: 0.05,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let tyre = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.04, 0.045),
        perceptual_roughness: 0.9,
        ..default()
    });
    let car = car_meshes().map(|m| meshes.add(m));
    let scooter = scooter_meshes().map(|m| meshes.add(m));
    let markab = markab_mesh::shapes()
        .iter()
        .map(|(coat, shape)| {
            (*coat, meshes.add(shape_mesh(shape)), markab_coat(*coat, &mut materials, &glass, &tyre))
        })
        .collect();
    // Cold gas: a cone of frost, seen through.
    let plume = (
        meshes.add(shape_mesh(&markab_mesh::plume(1.6))),
        materials.add(StandardMaterial {
            base_color: Color::srgba(0.8, 0.9, 1.0, 0.35),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
    );
    commands.insert_resource(PeopleScene {
        meshes: out,
        trim,
        visor,
        car,
        scooter,
        glass,
        tyre,
        markab,
        plume,
    });
}

/// Builds a figure for `id`, named `name`, on floor `floor` (the base's deck's on the world's render
/// layer; a strip's on the city's).
fn spawn_figure(
    commands: &mut Commands,
    scene: &PeopleScene,
    materials: &mut Assets<StandardMaterial>,
    font: &UiFont,
    (id, floor): (u16, u8),
    name: &str,
    avatar: Option<(usize, Handle<WorldAsset>)>,
) -> Entity {
    let [r, g, b] = suit_colour(name);
    let suit = materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: 0.7,
        ..default()
    });
    // Their car's paint: their colour, glossy, seen from inside too.
    let paint = materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: 0.25,
        metallic: 0.3,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let layer = RenderLayers::layer(if floor == BASE_DECK { 0 } else { CITY_LAYER });
    let root = commands
        .spawn((Transform::default(), Visibility::default(), Placed(Default::default()), layer.clone()))
        .id();
    let mut joints = [root; 12];
    for (i, piece) in Piece::ALL.iter().enumerate() {
        let parent =
            piece.parent().map_or(root, |p| joints[Piece::ALL.iter().position(|x| *x == p).unwrap()]);
        joints[i] = commands
            .spawn((
                Transform::from_translation(piece.joint()),
                Visibility::default(),
                layer.clone(),
                ChildOf(parent),
            ))
            .id();
    }
    for (i, paint, mesh) in &scene.meshes {
        let material = match paint {
            Paint::Suit => suit.clone(),
            Paint::Trim => scene.trim.clone(),
            Paint::Visor => scene.visor.clone(),
        };
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            layer.clone(),
            ChildOf(joints[*i]),
        ));
    }
    let mut vehicle = |meshes: &[Handle<Mesh>; 3]| {
        let v = commands.spawn((Transform::default(), Visibility::Hidden, layer.clone(), ChildOf(root))).id();
        for (mesh, material) in meshes.iter().zip([&paint, &scene.glass, &scene.tyre]) {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                layer.clone(),
                ChildOf(v),
            ));
        }
        v
    };
    let car = vehicle(&scene.car);
    let scooter = vehicle(&scene.scooter);
    let markab =
        commands.spawn((Transform::default(), Visibility::Hidden, layer.clone(), ChildOf(root))).id();
    let mut skirt = Vec::new();
    for (coat, mesh, material) in &scene.markab {
        let e = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone().unwrap_or_else(|| paint.clone())),
                Transform::default(),
                Visibility::Inherited,
                layer.clone(),
                ChildOf(markab),
            ))
            .id();
        if *coat == Coat::Skirt {
            skirt.push(e);
        }
    }
    let jets = markab_mesh::jets();
    let plumes = std::array::from_fn(|k| {
        let (at, dir) = jets[k];
        commands
            .spawn((
                Mesh3d(scene.plume.0.clone()),
                MeshMaterial3d(scene.plume.1.clone()),
                Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Z, dir)),
                Visibility::Hidden,
                layer.clone(),
                ChildOf(markab),
            ))
            .id()
    });
    // A store trader: their avatar, standing where the figure would (which stays hidden).
    let avatar = avatar.map(|(look, model)| {
        let l = bc_client_core::avatar::LOOKS[look];
        let turn = Quat::from_rotation_y(l.turn);
        let e = commands
            .spawn((
                WorldAssetRoot(model),
                Transform::from_rotation(turn).with_scale(Vec3::splat(l.scale)),
                Visibility::Inherited,
                layer.clone(),
                AvatarOf(look),
                ChildOf(root),
            ))
            .observe(avatar_ready)
            .id();
        commands.entity(joints[0]).insert(Visibility::Hidden);
        (e, turn, l.scale, bc_client_core::avatar::phase(name))
    });
    let tag = commands
        .spawn((
            Text::new(name.to_string()),
            font.heading(13.0),
            TextColor(Color::srgb(0.5 + 0.5 * r, 0.5 + 0.5 * g, 0.5 + 0.5 * b)),
            TextShadow::default(),
            Node { position_type: PositionType::Absolute, ..default() },
            Visibility::Hidden,
        ))
        .id();
    commands.entity(root).insert(Figure {
        id,
        floor,
        name: name.to_string(),
        phase: 0.0,
        last: None,
        joints,
        tag,
        car,
        scooter,
        markab,
        skirt,
        plumes,
        avatar,
    });
    root
}

/// Keeps a figure for everyone in the crowd, where they are, striding as they move.
#[allow(clippy::too_many_arguments)]
fn draw_people(
    mut commands: Commands,
    vis: Res<crate::view::VisTime>,
    crowd: Res<Crowd>,
    (view, base): (Res<CityView>, Res<crate::base::BaseView>),
    scene: Option<Res<PeopleScene>>,
    font: Res<UiFont>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut figures: Query<(Entity, &mut Figure, &mut Placed, &mut Transform)>,
    mut joints: Query<(&mut Transform, &mut Visibility), Without<Figure>>,
    (assets, mut lib, mut graphs): (Res<AssetServer>, ResMut<AvatarLib>, ResMut<Assets<AnimationGraph>>),
) {
    let Some(scene) = scene else { return };
    let want: HashMap<u16, (&str, &PersonPose)> = if view.active || base.active {
        crowd.0.iter().map(|(id, n, p)| (*id, (n.as_str(), p))).collect()
    } else {
        HashMap::new()
    };
    let mut have = Vec::new();
    for (e, mut f, mut placed, mut tf) in &mut figures {
        // Gone, or on another floor than their figure's (it's built again there).
        let Some((_, pose)) = want.get(&f.id).filter(|(_, p)| p.strip == f.floor) else {
            commands.entity(f.tag).despawn();
            commands.entity(e).despawn();
            continue;
        };
        have.push(f.id);
        // A Markab turns every way: its pose is its middle, and its whole attitude.
        let turned = if pose.markab() {
            attitude(pose.yaw, pose.pitch, pose.roll)
        } else {
            Quat::from_rotation_y(pose.yaw)
        };
        if pose.strip == BASE_DECK {
            // On the base's deck, drawn round the view as the deck is.
            (tf.translation, tf.rotation) = crate::base::place_pose(pose, turned, base.s);
        } else {
            placed.0 = colony_point(CityPos::new(pose.strip, pose.x, pose.s, pose.h));
            tf.rotation = local_frame(pose.strip, pose.s) * turned;
        }
        // Stride by how far they went.
        if let Some((x, s, _)) = f.last {
            let d = (pose.x - x).hypot(pose.s - s).min(2.0);
            f.phase = (f.phase + figure::stride_phase(d, pose.running)) % std::f32::consts::TAU;
        }
        f.last = Some((pose.x, pose.s, pose.h));
        // Driving: in a car or a Markab, out of sight in it; on a scooter, standing on its deck,
        // still. Seated, sitting.
        let (in_markab, on_scooter, seated) = (pose.markab(), pose.ride == RIDE_SCOOTER, pose.seated());
        let in_car = pose.ride == RIDE_CAR || in_markab;
        let still = in_car || on_scooter;
        let turns = if seated {
            figure::seated(pose.pitch)
        } else {
            figure::pose(
                if still { 0.0 } else { f.phase },
                if still { 0.0 } else { pose.speed },
                pose.pitch,
                pose.grounded || still,
                pose.running,
            )
        };
        for (i, j) in f.joints.iter().enumerate() {
            if let Ok((mut t, _)) = joints.get_mut(*j) {
                t.rotation = turns[i];
            }
        }
        if let Ok((mut hips, mut vis)) = joints.get_mut(f.joints[0]) {
            hips.translation.y = Piece::Hips.joint().y + if on_scooter { 0.4 } else { 0.0 }
                - if seated { figure::SEAT_DROP } else { 0.0 };
            let want = if in_car || f.avatar.is_some() { Visibility::Hidden } else { Visibility::Inherited };
            if *vis != want {
                *vis = want;
            }
        }
        // An avatar stands about: a slow lean and a breath, its own.
        if let Some((e, turn, scale, phase)) = f.avatar
            && let Ok((mut tf, _)) = joints.get_mut(e)
        {
            let (lx, lz, breath) = bc_client_core::avatar::sway(vis.now as f32, phase);
            tf.rotation = Quat::from_rotation_x(lx) * Quat::from_rotation_z(lz) * turn;
            tf.scale = Vec3::new(scale, scale * breath, scale);
        }
        show(&mut joints, f.car, pose.ride == RIDE_CAR);
        show(&mut joints, f.scooter, on_scooter);
        show(&mut joints, f.markab, in_markab);
        for e in &f.skirt {
            show(&mut joints, *e, in_markab && pose.running);
        }
        // The jets' plumes, flickering as the gas comes.
        let t = vis.now as f32;
        for (k, e) in f.plumes.iter().enumerate() {
            let jet = if k < 4 { JET_LIFT } else { JET_BOOST };
            let on = in_markab && pose.jets & jet != 0;
            show(&mut joints, *e, on);
            if on && let Ok((mut tf, _)) = joints.get_mut(*e) {
                let flicker =
                    0.85 + 0.15 * (t * 37.0 + k as f32 * 1.7).sin() + 0.1 * (t * 23.0 + k as f32).sin();
                tf.scale = Vec3::new(1.0, 1.0, flicker);
            }
        }
    }
    for (id, (name, pose)) in &want {
        if !have.contains(id) {
            let avatar = bc_client_core::avatar::look(*id, name)
                .map(|look| (look, lib.scene(look, &assets, &mut graphs)));
            spawn_figure(&mut commands, &scene, &mut materials, &font, (*id, pose.strip), name, avatar);
        }
    }
}

/// Shows or hides a part of a figure (or of what they drive).
fn show(parts: &mut Query<(&mut Transform, &mut Visibility), Without<Figure>>, e: Entity, on: bool) {
    if let Ok((_, mut vis)) = parts.get_mut(e) {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

/// How long a line said on the radio stays over its speaker's head, s.
const SAID_SECS: f64 = 8.0;

/// Names over the near ones, and over each, what they last said on the radio (for a while; a
/// showcase has no radio).
fn name_tags(
    origin: Res<RenderOrigin>,
    figures: Query<(&Figure, &Placed, &Transform)>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut tags: Query<(&mut Node, &mut Visibility, &mut Text)>,
    spoken: Option<Res<crate::chat::Spoken>>,
) {
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let now = crate::net::now_s();
    // Where the eye is in the city: someone on another floor of a home's building, or behind a
    // home's walls, has no name over them from here (`homes::same_space`).
    let land = |p: Vec3| match bc_sim::colony::frame::from_colony(p) {
        bc_sim::colony::frame::Under::Land(c) => Some(c),
        _ => None,
    };
    let eye = land((origin.0 + cam_tf.translation().as_dvec3()).as_vec3());
    for (f, placed, tf) in &figures {
        let Ok((mut node, mut vis, mut text)) = tags.get_mut(f.tag) else { continue };
        let said = spoken.as_ref().and_then(|s| s.0.get(&f.name)).filter(|(_, at)| now - at < SAID_SECS);
        let want = match said {
            Some((line, _)) => format!("{}\n{line}", f.name),
            None => f.name.clone(),
        };
        if text.0 != want {
            text.0 = want;
        }
        // (The base's deck is drawn where its figures stand, round the view.)
        let on_deck = f.floor == BASE_DECK;
        let head =
            if on_deck { tf.translation } else { origin.place(placed.0) } + tf.rotation * Vec3::Y * 2.05;
        let seen = on_deck
            || match (eye, land(placed.0.as_vec3())) {
                (Some(e), Some(c)) if e.strip == c.strip => {
                    bc_sim::colony::homes::same_space(c.strip, (e.s, e.x, e.h), (c.s, c.x, c.h))
                }
                _ => true,
            };
        let near = f.id != OWN && seen && cam_tf.translation().distance(head) < NAME_REACH;
        let shown = near.then(|| cam.world_to_viewport(cam_tf, head).ok()).flatten();
        match shown {
            Some(p) => {
                node.left = Val::Px(p.x - 40.0);
                node.top = Val::Px(p.y - 18.0);
                if *vis != Visibility::Visible {
                    *vis = Visibility::Visible;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

/// Game mode: the people near the pilot, from the plaza (none outside the city but round a suit
/// flying inside it), and the colony's clock for the trams.
pub fn fill_crowd(
    game: NonSend<crate::net::GameClient>,
    me: Res<crate::onfoot::OnFoot>,
    mut crowd: ResMut<Crowd>,
    mut trams: ResMut<crate::trams::TramClock>,
) {
    let g = game.borrow();
    let (tick, frac) = g.core.colony_tick(crate::net::now_s());
    *trams = crate::trams::TramClock(tick, frac);
    let mut people: Vec<(u16, String, PersonPose)> =
        if g.core.hangar.in_city() || g.core.hangar.in_base() || g.core.inside() {
            g.core
                .people(crate::net::now_s())
                .into_iter()
                .map(|(id, name, p)| (id, name.to_string(), p))
                .collect()
        } else {
            Vec::new()
        };
    // The pilot's own vehicle, as everyone else sees it (in a car their figure is out of sight).
    if let Some(v) = me.city.as_ref().and_then(|c| c.drive) {
        people.push((OWN, g.core.cfg.name.clone(), v.pose(0.0)));
    }
    if crowd.0 != people {
        crowd.0 = people;
    }
}
