//! The other pilots in the colony's city, drawn: a figure in a flight suit of each pilot's own
//! colour (`bc_client_core::figure`, `plaza::suit_colour`), striding as fast as they move, and
//! their name over them when they're near. Who's where is the [`Crowd`]: the plaza's people in game
//! mode (`fill_crowd`), or a showcase's.

use std::collections::HashMap;

use bc_client_core::figure::{self, Paint, Piece};
use bc_client_core::plaza::suit_colour;
use bc_proto::presence::{PersonPose, RIDE_CAR, RIDE_SCOOTER};
use bc_sim::colony::frame::{CityPos, local_frame};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

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
    /// Their callsign, as the radio names them.
    name: String,
    phase: f32,
    last: Option<(f32, f32, f32)>,
    joints: [Entity; 12],
    tag: Entity,
    car: Entity,
    scooter: Entity,
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
    commands.insert_resource(PeopleScene { meshes: out, trim, visor, car, scooter, glass, tyre });
}

/// Builds a figure for `id`, named `name`.
fn spawn_figure(
    commands: &mut Commands,
    scene: &PeopleScene,
    materials: &mut Assets<StandardMaterial>,
    font: &UiFont,
    id: u16,
    name: &str,
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
    let layer = RenderLayers::layer(CITY_LAYER);
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
        name: name.to_string(),
        phase: 0.0,
        last: None,
        joints,
        tag,
        car,
        scooter,
    });
    root
}

/// Keeps a figure for everyone in the crowd, where they are, striding as they move.
#[allow(clippy::too_many_arguments)]
fn draw_people(
    mut commands: Commands,
    crowd: Res<Crowd>,
    view: Res<CityView>,
    scene: Option<Res<PeopleScene>>,
    font: Res<UiFont>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut figures: Query<(Entity, &mut Figure, &mut Placed, &mut Transform)>,
    mut joints: Query<(&mut Transform, &mut Visibility), Without<Figure>>,
) {
    let Some(scene) = scene else { return };
    let want: HashMap<u16, (&str, &PersonPose)> = if view.active {
        crowd.0.iter().map(|(id, n, p)| (*id, (n.as_str(), p))).collect()
    } else {
        HashMap::new()
    };
    let mut have = Vec::new();
    for (e, mut f, mut placed, mut tf) in &mut figures {
        let Some((_, pose)) = want.get(&f.id) else {
            commands.entity(f.tag).despawn();
            commands.entity(e).despawn();
            continue;
        };
        have.push(f.id);
        let at = CityPos::new(pose.strip, pose.x, pose.s, pose.h);
        placed.0 = colony_point(at);
        tf.rotation = local_frame(pose.strip, pose.s) * Quat::from_rotation_y(pose.yaw);
        // Stride by how far they went.
        if let Some((x, s, _)) = f.last {
            let d = (pose.x - x).hypot(pose.s - s).min(2.0);
            f.phase = (f.phase + figure::stride_phase(d, pose.running)) % std::f32::consts::TAU;
        }
        f.last = Some((pose.x, pose.s, pose.h));
        // Driving: in a car, out of sight in it; on a scooter, standing on its deck, still. Seated,
        // sitting.
        let (in_car, on_scooter, seated) = (pose.ride == RIDE_CAR, pose.ride == RIDE_SCOOTER, pose.seated());
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
            let want = if in_car { Visibility::Hidden } else { Visibility::Inherited };
            if *vis != want {
                *vis = want;
            }
        }
        for (e, on) in [(f.car, in_car), (f.scooter, on_scooter)] {
            if let Ok((_, mut vis)) = joints.get_mut(e) {
                let want = if on { Visibility::Inherited } else { Visibility::Hidden };
                if *vis != want {
                    *vis = want;
                }
            }
        }
    }
    for (id, (name, _)) in &want {
        if !have.contains(id) {
            spawn_figure(&mut commands, &scene, &mut materials, &font, *id, name);
        }
    }
}

/// How long a line said on the radio stays over its speaker's head, s.
const SAID_SECS: f64 = 8.0;

/// Names over the near ones, and over each, what they last said on the radio (for a while).
fn name_tags(
    origin: Res<RenderOrigin>,
    figures: Query<(&Figure, &Placed, &Transform)>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut tags: Query<(&mut Node, &mut Visibility, &mut Text)>,
    spoken: Res<crate::chat::Spoken>,
) {
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let now = crate::net::now_s();
    for (f, placed, tf) in &figures {
        let Ok((mut node, mut vis, mut text)) = tags.get_mut(f.tag) else { continue };
        let said = spoken.0.get(&f.name).filter(|(_, at)| now - at < SAID_SECS);
        let want = match said {
            Some((line, _)) => format!("{}\n{line}", f.name),
            None => f.name.clone(),
        };
        if text.0 != want {
            text.0 = want;
        }
        let head = origin.place(placed.0) + tf.rotation * Vec3::Y * 2.05;
        let near = f.id != OWN && cam_tf.translation().distance(head) < NAME_REACH;
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
    let mut people: Vec<(u16, String, PersonPose)> = if g.core.hangar.in_city() || g.core.inside() {
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
