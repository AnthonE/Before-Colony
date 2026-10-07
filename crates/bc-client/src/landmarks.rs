//! The sector's landmarks: MO-II, the resource satellite rolling on its station-keeping circle, and
//! Hermit, the big asteroid. Each is drawn from the very shape suits stand on
//! (`bc_client_core::body_mesh`), so feet meet what's drawn, and posed every frame by the
//! simulation's own closed form ([`bc_sim::bodies::landmark_pose`]) at the view clock's time, as
//! every client and the server have it. The paint is in the body's own space, so its panels turn
//! with it.
//!
//! - Hide spots are ringed with amber lights round their rims, in the dock's style: dim ones all
//!   round, brighter ones chasing round them; a work light hangs over each floor.
//! - Far off, a landmark is drawn coarse (switched on the CPU, as rocks are: Bevy 0.19's
//!   `VisibilityRange` fails pipeline validation on WebGL2).

use std::f32::consts::{PI, TAU};

use bc_client_core::body_mesh::{self, MeshData};
use bc_sim::bodies::{Base, Body, Prim, Shape};
use bc_sim::content::landmarks::{HideSpot, LANDMARKS, LandmarkDef};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::materials::{HullTag, Surfaces, paint, rock_tag};
use crate::view::{DrawnBodies, Vis, VisTime};

/// A landmark is drawn coarse beyond this many times its bounding radius (m), and in detail again
/// within nine tenths of it.
const LOD_BOUNDS: f32 = 12.0;
/// Lights round each hide spot's rim, and how far off the surface they stand (m).
const RIM_LIGHTS: usize = 24;
const RIM_LIFT: f32 = 1.5;
/// Each hide spot's work light: how bright (lm), how far it reaches and how high over the floor it
/// hangs (m).
const LAMP_LUMENS: f32 = 6.0e7;
const LAMP_RANGE: f32 = 90.0;
const LAMP_HEIGHT: f32 = 22.0;

pub struct LandmarksPlugin;

impl Plugin for LandmarksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, pose_landmarks.in_set(Vis::Suits))
            .add_systems(Update, (landmark_lod, blink_rims).in_set(Vis::Fx));
    }
}

/// A landmark's root: where it is drawn, and its two meshes' groups.
#[derive(Component)]
pub struct LandmarkRoot {
    pub id: u8,
    near: Entity,
    far: Entity,
    switch: f32,
    detailed: bool,
}

/// One of the lights round a hide spot's rim.
#[derive(Component)]
struct RimLight {
    phase: f32,
}

/// `m` as a Bevy mesh.
pub(crate) fn to_mesh(m: &MeshData) -> Mesh {
    let uvs: Vec<[f32; 2]> = m.positions.iter().map(|p| [p[0], p[1]]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(m.indices.clone()))
}

/// A shape's coarse meshes, for far off: each primitive with a few segments, an ellipsoid as a
/// small cube-sphere. Too far to see a hide spot's bowl, so none is cut.
pub(crate) fn far_meshes(shape: &Shape) -> Vec<MeshData> {
    match shape.base {
        Base::Ellipsoid(axes) => vec![body_mesh::ellipsoid(axes, 20, &[])],
        Base::Union(prims) => prims
            .iter()
            .map(|p| match *p {
                Prim::Sphere { c, r } => body_mesh::capsule(c, c, r, 4),
                Prim::Capsule { a, b, r } => body_mesh::capsule(a, b, r, 2),
                Prim::CylinderX { c, half_len, r, round } => {
                    body_mesh::round_cylinder_x(c, half_len, r, round, 16)
                }
                Prim::RoundBox { c, half, round } => body_mesh::round_box(c, half, round, 2),
            })
            .collect(),
        // The city's meshes are its streamer's.
        Base::City => Vec::new(),
    }
}

/// Where the rim of hide spot `spot` is, `RIM_LIGHTS` points round it, with the surface's normal
/// at each (the body's frame): where the bowl's sphere comes out of the body's surface.
fn rim(def: &LandmarkDef, spot: &HideSpot) -> Vec<(Vec3, Vec3)> {
    // The cut whose bowl the spot's floor is on, and its way out of the body.
    let Some(cut) = def.shape.cuts.iter().min_by(|a, b| {
        (a.c.distance(spot.center) - a.r).abs().total_cmp(&(b.c.distance(spot.center) - b.r).abs())
    }) else {
        return Vec::new();
    };
    let out = (cut.c - spot.center).normalize_or(Vec3::Y);
    let (e1, e2) = out.any_orthonormal_pair();
    let solid = Shape { base: def.shape.base, cuts: &[] };
    (0..RIM_LIGHTS)
        .filter_map(|k| {
            let a = TAU * k as f32 / RIM_LIGHTS as f32;
            let side = e1 * a.cos() + e2 * a.sin();
            // Up the bowl's sphere from its floor, until it leaves the solid.
            let at = |phi: f32| cut.c + (-out * phi.cos() + side * phi.sin()) * cut.r;
            let (mut lo, mut hi) = (0.0f32, PI);
            if solid.probe(at(lo)).dist >= 0.0 {
                return None;
            }
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                if solid.probe(at(mid)).dist < 0.0 { lo = mid } else { hi = mid }
            }
            let p = at(hi);
            let n = solid.probe(p).normal;
            Some((p + n * RIM_LIFT, n))
        })
        .collect()
}

/// Builds every landmark this client knows of (a sector has the first so many of them: the rest
/// are hidden by [`pose_landmarks`]).
pub fn setup_landmarks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    surfaces: Res<Surfaces>,
) {
    let amber = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(6.0, 2.4, 0.3),
        ..default()
    });
    let bright = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(40.0, 16.0, 2.0),
        ..default()
    });
    let bulb = meshes.add(Sphere::new(0.9).mesh().ico(1).expect("icosphere"));
    // (The docking hub is the colony's, and drawn with it: `colony.rs`.)
    for (k, def) in LANDMARKS.iter().enumerate().filter(|(_, d)| !d.colony) {
        let rock = matches!(def.shape.base, Base::Ellipsoid(_));
        let mut group = |commands: &mut Commands, parts: Vec<MeshData>, visible: bool| {
            let group = commands
                .spawn((
                    Transform::default(),
                    if visible { Visibility::Inherited } else { Visibility::Hidden },
                ))
                .id();
            for (i, part) in parts.iter().enumerate() {
                let mesh = Mesh3d(meshes.add(to_mesh(part)));
                let piece = if rock {
                    commands
                        .spawn((mesh, MeshMaterial3d(surfaces.rock.clone()), rock_tag(0, 200 + k as u8)))
                        .id()
                } else {
                    // The core and the modules a darker grey than the pylons and the mast.
                    let shade = if i < 3 { paint::HULL_DARK } else { paint::HULL };
                    let tag = HullTag::paint(shade, 120 + (k * 16 + i) as u8).tag();
                    commands.spawn((mesh, MeshMaterial3d(surfaces.station.clone()), tag)).id()
                };
                commands.entity(group).add_child(piece);
            }
            group
        };
        let near = group(&mut commands, body_mesh::shape_meshes(&def.shape), true);
        let far = group(&mut commands, far_meshes(&def.shape), false);
        let root = commands
            .spawn((
                LandmarkRoot { id: k as u8, near, far, switch: def.bound * LOD_BOUNDS, detailed: true },
                Transform::from_translation(def.center).with_rotation(def.rot0),
                Visibility::default(),
            ))
            .add_children(&[near, far])
            .id();
        for (s, spot) in def.hides.iter().enumerate() {
            // A work light hung over the bowl's floor (a hide spot is often on a body's dark
            // side): enough to make out what's in it, from close.
            let lamp = commands
                .spawn((
                    PointLight {
                        intensity: LAMP_LUMENS,
                        range: LAMP_RANGE,
                        color: Color::srgb(1.0, 0.78, 0.5),
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(
                        spot.center + def.shape.probe(spot.center).normal * LAMP_HEIGHT,
                    ),
                ))
                .id();
            commands.entity(root).add_child(lamp);
            for (i, (p, n)) in rim(def, spot).into_iter().enumerate() {
                let at = Transform::from_translation(p).with_rotation(Quat::from_rotation_arc(Vec3::Y, n));
                let dim = commands.spawn((Mesh3d(bulb.clone()), MeshMaterial3d(amber.clone()), at)).id();
                let lit = commands
                    .spawn((
                        Mesh3d(bulb.clone()),
                        MeshMaterial3d(bright.clone()),
                        at.with_scale(Vec3::splat(1.6)),
                        RimLight { phase: i as f32 / RIM_LIGHTS as f32 + s as f32 * 0.37 },
                    ))
                    .id();
                commands.entity(root).add_children(&[dim, lit]);
            }
        }
    }
}

/// Puts each landmark where the simulation has it at the view clock's time; one the sector hasn't
/// got isn't drawn.
pub fn pose_landmarks(
    bodies: Res<DrawnBodies>,
    mut roots: Query<(&LandmarkRoot, &mut Transform, &mut Visibility)>,
) {
    for (root, mut tf, mut vis) in &mut roots {
        match bodies.pose(Body::Landmark(root.id)) {
            Some(p) => {
                tf.translation = p.pos;
                tf.rotation = p.rot;
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// Swaps landmarks between their detailed and coarse meshes by distance from the camera.
fn landmark_lod(
    cams: Query<&Transform, With<MainCamera>>,
    mut roots: Query<(&mut LandmarkRoot, &Transform), Without<MainCamera>>,
    mut vis: Query<&mut Visibility, Without<LandmarkRoot>>,
) {
    let Ok(cam) = cams.single() else { return };
    for (mut root, tf) in &mut roots {
        let d = tf.translation.distance(cam.translation);
        let want = if root.detailed { d < root.switch } else { d < root.switch * 0.9 };
        if want == root.detailed {
            continue;
        }
        root.detailed = want;
        for (e, on) in [(root.near, want), (root.far, !want)] {
            if let Ok(mut v) = vis.get_mut(e) {
                v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
}

/// The bright lights chase round each hide spot's rim.
fn blink_rims(time: Res<VisTime>, mut lights: Query<(&RimLight, &mut Visibility)>) {
    for (l, mut v) in &mut lights {
        let on = (time.now as f32 * 0.5 - l.phase).rem_euclid(1.0) < 0.12;
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
}
