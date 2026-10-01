//! The other pilots in the colony's city, drawn: a figure in a flight suit of each pilot's own
//! colour (`bc_client_core::figure`, `plaza::suit_colour`), striding as fast as they move, and
//! their name over them when they're near. Who's where is the [`Crowd`]: the plaza's people in game
//! mode (`fill_crowd`), or a showcase's.

use std::collections::HashMap;

use bc_client_core::figure::{self, Paint, Piece};
use bc_client_core::plaza::suit_colour;
use bc_proto::presence::PersonPose;
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

/// The people to draw: slot, name and pose.
#[derive(Resource, Default)]
pub struct Crowd(pub Vec<(u16, String, PersonPose)>);

pub struct PeoplePlugin;

impl Plugin for PeoplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Crowd>()
            .add_systems(Startup, setup_people)
            .add_systems(Update, draw_people.in_set(crate::view::Vis::Suits))
            .add_systems(Update, name_tags.in_set(crate::view::Vis::Hud));
    }
}

/// The figure's meshes (a piece's paints apart) and its trim's and visor's materials.
#[derive(Resource)]
struct PeopleScene {
    meshes: Vec<(usize, Paint, Handle<Mesh>)>,
    trim: Handle<StandardMaterial>,
    visor: Handle<StandardMaterial>,
}

/// A pilot drawn: their slot, their stride, where they were last frame (to stride by it), their
/// pieces' joints and their name tag.
#[derive(Component)]
struct Figure {
    id: u16,
    phase: f32,
    last: Option<(f32, f32, f32)>,
    joints: [Entity; 12],
    tag: Entity,
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
    commands.insert_resource(PeopleScene { meshes: out, trim, visor });
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
    commands.entity(root).insert(Figure { id, phase: 0.0, last: None, joints, tag });
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
    mut joints: Query<&mut Transform, Without<Figure>>,
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
        let turns = figure::pose(f.phase, pose.speed, pose.pitch, pose.grounded, pose.running);
        for (i, j) in f.joints.iter().enumerate() {
            if let Ok(mut t) = joints.get_mut(*j) {
                t.rotation = turns[i];
            }
        }
    }
    for (id, (name, _)) in &want {
        if !have.contains(id) {
            spawn_figure(&mut commands, &scene, &mut materials, &font, *id, name);
        }
    }
}

/// Names over the near ones.
fn name_tags(
    origin: Res<RenderOrigin>,
    figures: Query<(&Figure, &Placed, &Transform)>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut tags: Query<(&mut Node, &mut Visibility), With<Text>>,
) {
    let Ok((cam, cam_tf)) = cams.single() else { return };
    for (f, placed, tf) in &figures {
        let Ok((mut node, mut vis)) = tags.get_mut(f.tag) else { continue };
        let head = origin.place(placed.0) + tf.rotation * Vec3::Y * 2.05;
        let near = cam_tf.translation().distance(head) < NAME_REACH;
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

/// Game mode: the people near the pilot, from the plaza (none outside the city), and the colony's
/// clock for the trams.
pub fn fill_crowd(
    game: NonSend<crate::net::GameClient>,
    mut crowd: ResMut<Crowd>,
    mut trams: ResMut<crate::trams::TramClock>,
) {
    let g = game.borrow();
    let (tick, frac) = g.core.colony_tick(crate::net::now_s());
    *trams = crate::trams::TramClock(tick, frac);
    let people = if g.core.hangar.in_city() {
        g.core
            .people(crate::net::now_s())
            .into_iter()
            .map(|(id, name, p)| (id, name.to_string(), p))
            .collect()
    } else {
        Vec::new()
    };
    if crowd.0 != people {
        crowd.0 = people;
    }
}
