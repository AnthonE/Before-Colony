//! The debris field, drawn from `bc_sim::field` (the generator the server shares): procedural
//! asteroids, one mesh per shape so they batch, with a cheaper mesh in the distance. In game it is
//! the server's field, from the Welcome.

use bc_sim::field::{Field, SHAPES};
use bc_sim::math::Rng;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::materials::{Surfaces, rock_tag};
use crate::noise::fbm;

/// A rock's two meshes and the distance at which it switches between them. (Switched on the CPU:
/// Bevy 0.19's `VisibilityRange` fails pipeline validation on WebGL2.)
#[derive(Component)]
pub struct RockLod {
    near: Entity,
    far: Entity,
    pos: Vec3,
    switch: f32,
    detailed: bool,
}

/// An asteroid: an icosphere pushed around by noise and pocked with craters, scaled so that it
/// fits inside the unit sphere (the field's collider radius bounds it).
fn rock_mesh(shape: u8, subdivisions: u32) -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().ico(subdivisions).expect("icosphere");
    let mut rng = Rng::new(0xA57E_0000 + u64::from(shape));
    let craters: Vec<(Vec3, f32, f32)> = (0..6 + rng.next_u32() % 5)
        .map(|_| {
            let c = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or(Vec3::Y);
            (c, 0.18 + rng.next_f32() * 0.35, 0.04 + rng.next_f32() * 0.08)
        })
        .collect();
    let offset = Vec3::splat(f32::from(shape) * 13.7);
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh;
    };
    let mut out: Vec<Vec3> = positions
        .iter()
        .map(|p| {
            let d = Vec3::from(*p).normalize();
            let mut r =
                1.0 + 0.34 * (fbm(d * 1.6 + offset, 5) - 0.5) + 0.08 * (fbm(d * 5.0 + offset, 3) - 0.5);
            for (c, rad, depth) in &craters {
                let a = d.dot(*c).clamp(-1.0, 1.0).acos();
                if a < *rad {
                    let t = a / rad;
                    r -= depth * (1.0 - t * t);
                } else if a < rad * 1.4 {
                    // A raised rim.
                    let t = (a - rad) / (rad * 0.4);
                    r += depth * 0.4 * (1.0 - t) * (1.0 - t);
                }
            }
            d * r
        })
        .collect();
    let max = out.iter().map(|p| p.length()).fold(0.0, f32::max).max(1e-3);
    for p in &mut out {
        *p /= max;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, out.iter().map(|p| p.to_array()).collect::<Vec<_>>());
    mesh.compute_smooth_normals();
    mesh
}

/// Every rock shape's meshes, detailed and coarse.
#[derive(Resource)]
pub struct RockMeshes {
    near: Vec<Handle<Mesh>>,
    far: Vec<Handle<Mesh>>,
}

impl RockMeshes {
    /// A shape's coarse mesh (small pieces: loose ore).
    pub fn coarse(&self, shape: usize) -> Handle<Mesh> {
        self.far[shape % self.far.len()].clone()
    }
}

/// Which field is on screen: (seed, rocks).
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
pub struct ShownField(pub u32, pub u16);

/// Marks every rock entity (to clear the field when it's replaced).
#[derive(Component)]
pub struct RockPiece;

/// Builds the rock meshes and spawns the default field.
pub fn setup_field(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, surfaces: Res<Surfaces>) {
    let lib = RockMeshes {
        near: (0..SHAPES).map(|s| meshes.add(rock_mesh(s, 7))).collect(),
        far: (0..SHAPES).map(|s| meshes.add(rock_mesh(s, 2))).collect(),
    };
    let shown = ShownField(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
    spawn_rocks(&mut commands, &lib, &surfaces, shown);
    commands.insert_resource(lib);
    commands.insert_resource(shown);
}

/// Replaces the field on screen with `want` if it's a different one (the server's, once welcomed).
pub fn show_field(
    want: ShownField,
    commands: &mut Commands,
    lib: &RockMeshes,
    surfaces: &Surfaces,
    shown: &mut ShownField,
    pieces: &Query<Entity, With<RockPiece>>,
) {
    if want == *shown {
        return;
    }
    for e in pieces {
        commands.entity(e).despawn();
    }
    spawn_rocks(commands, lib, surfaces, want);
    *shown = want;
}

/// Spawns each rock twice: a detailed mesh up close and a coarse one beyond.
fn spawn_rocks(commands: &mut Commands, lib: &RockMeshes, surfaces: &Surfaces, field: ShownField) {
    let (near, far) = (&lib.near, &lib.far);
    let field = Field::generate(field.0, field.1);
    for (i, r) in field.rocks().iter().enumerate() {
        let tf = Transform::from_translation(r.pos).with_rotation(r.rot).with_scale(r.axes);
        let tag = rock_tag(r.ore, i as u8);
        let switch = 1_200.0 + r.radius * 40.0;
        let shape = usize::from(r.shape);
        let material = MeshMaterial3d(surfaces.rock.clone());
        let near =
            commands.spawn((Mesh3d(near[shape].clone()), material.clone(), tf, tag.clone(), RockPiece)).id();
        let far = commands
            .spawn((Mesh3d(far[shape].clone()), material, tf, tag, Visibility::Hidden, RockPiece))
            .id();
        commands.spawn((RockLod { near, far, pos: r.pos, switch, detailed: true }, RockPiece));
    }
}

/// Swaps rocks between their detailed and coarse meshes by distance from the camera, with a
/// little hysteresis so a rock at the boundary doesn't flicker.
pub fn rock_lod(
    cams: Query<&Transform, With<MainCamera>>,
    mut lods: Query<&mut RockLod>,
    mut vis: Query<&mut Visibility>,
) {
    let Ok(cam) = cams.single() else { return };
    let eye = cam.translation;
    for mut lod in &mut lods {
        let d = lod.pos.distance(eye);
        let want = if lod.detailed { d < lod.switch * 1.1 } else { d < lod.switch };
        if want == lod.detailed {
            continue;
        }
        lod.detailed = want;
        for (e, on) in [(lod.near, want), (lod.far, !want)] {
            if let Ok(mut v) = vis.get_mut(e) {
                *v = if on { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

/// In game, draws the server's debris field (as told in the Welcome).
pub fn follow_server_field(
    game: NonSend<crate::net::GameClient>,
    mut commands: Commands,
    lib: Option<Res<RockMeshes>>,
    surfaces: Option<Res<Surfaces>>,
    shown: Option<ResMut<ShownField>>,
    pieces: Query<Entity, With<RockPiece>>,
) {
    let (Some(lib), Some(surfaces), Some(mut shown)) = (lib, surfaces, shown) else { return };
    let Some(w) = game.borrow().core.welcome else { return };
    show_field(ShownField(w.field_seed, w.field_rocks), &mut commands, &lib, &surfaces, &mut shown, &pieces);
}
