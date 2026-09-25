//! The L1 colony, "Colony 03": an Island-3 cylinder 6.4 km across and 32 km long, turning once
//! every 113 s for 1 g at the rim. Three land strips alternate with three windows that look into
//! the colony (`shaders/colony_window.wgsl`), structural rings band the hull, the end caps carry a
//! docking hub and the mirror hub, and three hinged mirrors at the sunward end throw light in.
//!
//! Collision stays the simulation's static cylinder (`bc_sim::world`), which doesn't care that the
//! visual one spins.

use std::f32::consts::{FRAC_PI_3, FRAC_PI_6, TAU};

use bc_sim::content::salvage::{DOCK_CENTER, DOCK_HUB_LENGTH, DOCK_RADIUS};
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use crate::materials::{HullTag, Surfaces, paint};
use crate::sky::{SUN_DIR, SUN_LUX, Sun};
use crate::view::VisTime;

/// Spin for 1 g at the rim: ω = √(g / R).
const SPIN: f32 = 0.055_37;
/// Centre angle of the first window strip (the others follow every 120°).
const FIRST_WINDOW: f32 = 0.4;
/// One colony day (the mirrors open and close), seconds.
const DAY_SECS: f64 = 1_200.0;

pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/colony_window.wgsl");
        app.add_plugins(MaterialPlugin::<WindowMaterial>::default())
            .add_systems(Update, (spin, blink_beacons));
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct WindowMaterial {
    #[uniform(0)]
    colony: ColonyUniform,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct ColonyUniform {
    /// xyz: centre; w: spin angle.
    centre: Vec4,
    /// x: radius; y: half-length; z: first window's angle; w: daylight.
    shape: Vec4,
    /// xyz: direction to the Sun; w: sunlight scale.
    sun: Vec4,
}

impl Material for WindowMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/colony_window.wgsl".into()
    }
}

/// The spinning part of the colony (everything visual).
#[derive(Component)]
struct ColonyRoot {
    windows: Handle<WindowMaterial>,
}

/// A blinking navigation light.
#[derive(Component)]
struct Beacon {
    phase: f32,
}

/// How far along a ray (unit `dir`) it meets the colony's hull or an end cap, if it does. The
/// solid is the simulation's static cylinder.
pub fn ray_hit(origin: Vec3, dir: Vec3) -> Option<f32> {
    let rel = origin - COLONY_CENTER;
    let mut best: Option<f32> = None;
    let mut consider = |t: f32| {
        if t > 0.0 && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    };
    // The hull: |rel.yz + t dir.yz| = R, within the length.
    let a = dir.y * dir.y + dir.z * dir.z;
    let b = 2.0 * (rel.y * dir.y + rel.z * dir.z);
    let c = rel.y * rel.y + rel.z * rel.z - COLONY_RADIUS * COLONY_RADIUS;
    if a > 1e-9 {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / (2.0 * a);
            if (rel.x + t * dir.x).abs() <= COLONY_HALF_LENGTH {
                consider(t);
            }
        }
    }
    // The end caps.
    if dir.x.abs() > 1e-6 {
        for cap in [-COLONY_HALF_LENGTH, COLONY_HALF_LENGTH] {
            let t = (cap - rel.x) / dir.x;
            let (y, z) = (rel.y + t * dir.y, rel.z + t * dir.z);
            if y * y + z * z <= COLONY_RADIUS * COLONY_RADIUS {
                consider(t);
            }
        }
    }
    best
}

/// Point on the hull at `angle` around the axis (colony space: x along the axis).
fn around(angle: f32, r: f32, x: f32) -> Vec3 {
    Vec3::new(x, r * angle.cos(), r * angle.sin())
}

fn mesh(positions: Vec<Vec3>, normals: Vec<Vec3>, indices: Vec<u32>) -> Mesh {
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p.x, p.y]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            positions.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            normals.iter().map(|n| n.to_array()).collect::<Vec<_>>(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// Quads over a (`cols` + 1) × (`rows` + 1) grid of vertices, facing along `positions × normals`
/// counter-clockwise.
fn grid_indices(cols: u32, rows: u32) -> Vec<u32> {
    let mut idx = Vec::with_capacity((cols * rows * 6) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let a = j * (cols + 1) + i;
            let b = a + 1;
            let c = a + cols + 1;
            let d = c + 1;
            idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    idx
}

/// The outer wall of the cylinder between two angles, full length.
fn wall(a0: f32, a1: f32) -> Mesh {
    let (cols, rows) = (24u32, 16u32);
    let mut p = Vec::new();
    let mut n = Vec::new();
    for j in 0..=rows {
        let x = -COLONY_HALF_LENGTH + 2.0 * COLONY_HALF_LENGTH * j as f32 / rows as f32;
        for i in 0..=cols {
            let a = a0 + (a1 - a0) * i as f32 / cols as f32;
            p.push(around(a, COLONY_RADIUS, x));
            n.push(around(a, 1.0, 0.0));
        }
    }
    mesh(p, n, grid_indices(cols, rows))
}

/// A structural ring: a band standing `height` off the hull, `width` wide, all the way round.
fn ring(height: f32, width: f32) -> Mesh {
    let segs = 144u32;
    let mut p = Vec::new();
    let mut n = Vec::new();
    // Outer face, then the two side faces.
    for (r0, r1, x0, x1, side) in [
        (COLONY_RADIUS + height, COLONY_RADIUS + height, -width / 2.0, width / 2.0, 0.0),
        (COLONY_RADIUS, COLONY_RADIUS + height, width / 2.0, width / 2.0, 1.0),
        (COLONY_RADIUS + height, COLONY_RADIUS, -width / 2.0, -width / 2.0, -1.0),
    ] {
        for (r, x) in [(r0, x0), (r1, x1)] {
            for i in 0..=segs {
                let a = TAU * i as f32 / segs as f32;
                p.push(around(a, r, x));
                n.push(if side == 0.0 { around(a, 1.0, 0.0) } else { Vec3::X * side });
            }
        }
    }
    let mut idx = Vec::new();
    for face in 0..3u32 {
        let base = face * 2 * (segs + 1);
        for i in 0..segs {
            let (a, b) = (base + i, base + i + 1);
            let (c, d) = (a + segs + 1, b + segs + 1);
            // Counter-clockwise seen from outside: the outer face runs round then along; the side
            // faces run outward then round.
            if face == 0 {
                idx.extend_from_slice(&[a, b, c, b, d, c]);
            } else {
                idx.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
    mesh(p, n, idx)
}

/// An end cap: a disc of rings, facing along `side` (±X).
fn cap(side: f32) -> Mesh {
    let (rings, segs) = (6u32, 96u32);
    let mut p = Vec::new();
    let mut n = Vec::new();
    for j in 0..=rings {
        let r = COLONY_RADIUS * j as f32 / rings as f32;
        for i in 0..=segs {
            let a = TAU * i as f32 / segs as f32;
            p.push(around(a, r, side * COLONY_HALF_LENGTH));
            n.push(Vec3::X * side);
        }
    }
    // Round then outward faces −X; flip for the +X cap.
    let mut idx = grid_indices(segs, rings);
    if side > 0.0 {
        for t in idx.chunks_mut(3) {
            t.swap(1, 2);
        }
    }
    mesh(p, n, idx)
}

pub fn setup_colony(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut windows: ResMut<Assets<WindowMaterial>>,
    surfaces: Res<Surfaces>,
) {
    let window_material = windows.add(WindowMaterial {
        colony: ColonyUniform {
            centre: COLONY_CENTER.extend(0.0),
            shape: Vec4::new(COLONY_RADIUS, COLONY_HALF_LENGTH, FIRST_WINDOW, 1.0),
            sun: SUN_DIR.extend(1.0),
        },
    });
    let beacon_red = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(40.0, 2.0, 1.0),
        ..default()
    });
    let beacon_white = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(30.0, 30.0, 34.0),
        ..default()
    });
    let hull = |paint: u8, seed: u8| HullTag::paint(paint, seed).tag();
    let beacon_amber = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(40.0, 16.0, 2.0),
        ..default()
    });
    let marker_amber = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(6.0, 2.4, 0.3),
        ..default()
    });
    let beacon_mesh = meshes.add(Sphere::new(6.0).mesh().ico(1).expect("icosphere"));

    let root = commands
        .spawn((Transform::from_translation(COLONY_CENTER), Visibility::default()))
        .with_children(|c| {
            for k in 0..3 {
                let w = FIRST_WINDOW + k as f32 * TAU / 3.0;
                // A window, then the land strip after it.
                c.spawn((
                    Mesh3d(meshes.add(wall(w - FRAC_PI_6, w + FRAC_PI_6))),
                    MeshMaterial3d(window_material.clone()),
                    NotShadowCaster,
                ));
                c.spawn((
                    Mesh3d(meshes.add(wall(w + FRAC_PI_6, w + FRAC_PI_6 + FRAC_PI_3))),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL, k as u8),
                    NotShadowCaster,
                ));
                // A mirror hinged at the sunward end of the window, opened 28° off the hull.
                let radial = around(w, 1.0, 0.0);
                let tangent = Vec3::new(0.0, -w.sin(), w.cos());
                let beta = 28f32.to_radians();
                let along = (-Vec3::X * beta.cos() + radial * beta.sin()).normalize();
                let normal = tangent.cross(along);
                let (length, width) = (7_000.0, COLONY_RADIUS);
                let hinge = around(w, COLONY_RADIUS + 60.0, COLONY_HALF_LENGTH);
                c.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                    MeshMaterial3d(surfaces.mirror.clone()),
                    HullTag { metal: true, ..HullTag::paint(paint::MIRROR, 60 + k as u8) }.tag(),
                    Transform::from_translation(hinge + along * (length / 2.0))
                        .with_rotation(Quat::from_mat3(&Mat3::from_cols(along, normal, tangent)))
                        .with_scale(Vec3::new(length, 12.0, width)),
                    NotShadowCaster,
                ));
                // Lights at the mirror's free corners.
                for s in [-0.5f32, 0.5] {
                    c.spawn((
                        Mesh3d(beacon_mesh.clone()),
                        MeshMaterial3d(beacon_red.clone()),
                        Transform::from_translation(hinge + along * length + tangent * width * s),
                        Beacon { phase: k as f32 * 0.37 + s },
                    ));
                }
            }
            // Greebles on the land strips: equipment housings, radiators and long conduits
            // running along the axis. One mesh, so they batch into a single draw.
            let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
            let mut rng = bc_sim::math::Rng::new(0x0C01_04E5);
            for i in 0..420u32 {
                let strip = (i % 3) as f32;
                let a =
                    FIRST_WINDOW + strip * TAU / 3.0 + FRAC_PI_6 + FRAC_PI_3 * (0.08 + 0.84 * rng.next_f32());
                let x = (rng.signed() * 0.97) * COLONY_HALF_LENGTH;
                let conduit = i % 7 == 0;
                let size = if conduit {
                    Vec3::new(
                        400.0 + rng.next_f32() * 1_400.0,
                        4.0 + rng.next_f32() * 4.0,
                        6.0 + rng.next_f32() * 6.0,
                    )
                } else {
                    Vec3::new(
                        10.0 + rng.next_f32() * 70.0,
                        3.0 + rng.next_f32() * 14.0,
                        8.0 + rng.next_f32() * 36.0,
                    )
                };
                let up = around(a, 1.0, 0.0);
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(if i % 3 == 0 { paint::HULL } else { paint::HULL_DARK }, (i % 251) as u8),
                    Transform::from_translation(around(a, COLONY_RADIUS + size.y * 0.5, x))
                        .with_rotation(Quat::from_rotation_arc(Vec3::Y, up))
                        .with_scale(size),
                    NotShadowCaster,
                ));
            }
            // Structural rings every 2 km.
            let ring_mesh = meshes.add(ring(14.0, 40.0));
            for i in 0..=16 {
                let x = -COLONY_HALF_LENGTH + 2_000.0 * i as f32;
                c.spawn((
                    Mesh3d(ring_mesh.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, 40 + i as u8),
                    Transform::from_xyz(x, 0.0, 0.0),
                    NotShadowCaster,
                ));
            }
            for (side, hub_radius, hub_length) in [(-1.0f32, 340.0, DOCK_HUB_LENGTH), (1.0, 520.0, 500.0)] {
                c.spawn((
                    Mesh3d(meshes.add(cap(side))),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, if side < 0.0 { 90 } else { 91 }),
                    NotShadowCaster,
                ));
                // The docking hub (−X; the dock is off its mouth) and the mirror hub (+X).
                c.spawn((
                    Mesh3d(meshes.add(Cylinder::new(hub_radius, hub_length).mesh().resolution(48))),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL, if side < 0.0 { 92 } else { 93 }),
                    Transform::from_xyz(side * (COLONY_HALF_LENGTH + hub_length / 2.0), 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
                // Navigation lights round the cap's rim and the hub's mouth.
                for i in 0..16 {
                    let a = TAU * i as f32 / 16.0;
                    c.spawn((
                        Mesh3d(beacon_mesh.clone()),
                        MeshMaterial3d(if i % 2 == 0 { beacon_red.clone() } else { beacon_white.clone() }),
                        Transform::from_translation(around(
                            a,
                            COLONY_RADIUS + 20.0,
                            side * COLONY_HALF_LENGTH,
                        )),
                        Beacon { phase: i as f32 * 0.11 },
                    ));
                    c.spawn((
                        Mesh3d(beacon_mesh.clone()),
                        MeshMaterial3d(beacon_white.clone()),
                        Transform::from_translation(around(
                            a,
                            hub_radius + 10.0,
                            side * (COLONY_HALF_LENGTH + hub_length),
                        )),
                        Beacon { phase: 0.5 + i as f32 * 0.07 },
                    ));
                }
            }
            // The dock, off the hub's mouth: a ring of amber lights round the edge of where to stop,
            // with brighter ones chasing round it.
            for i in 0..24 {
                let a = TAU * i as f32 / 24.0;
                let at = Transform::from_translation(around(a, DOCK_RADIUS, DOCK_CENTER.x - COLONY_CENTER.x));
                c.spawn((Mesh3d(beacon_mesh.clone()), MeshMaterial3d(marker_amber.clone()), at));
                c.spawn((
                    Mesh3d(beacon_mesh.clone()),
                    MeshMaterial3d(beacon_amber.clone()),
                    at.with_scale(Vec3::splat(1.6)),
                    Beacon { phase: i as f32 / 24.0 },
                ));
            }
        })
        .id();
    commands.entity(root).insert(ColonyRoot { windows: window_material });
}

/// Turns the colony, and tells the window shader the spin, the time of day and the sunlight.
fn spin(
    time: Res<VisTime>,
    mut roots: Query<(&ColonyRoot, &mut Transform)>,
    suns: Query<&DirectionalLight, With<Sun>>,
    mut windows: ResMut<Assets<WindowMaterial>>,
) {
    let angle = (time.now * f64::from(SPIN)) % std::f64::consts::TAU;
    let day = (std::f64::consts::TAU * time.now / DAY_SECS).cos() as f32;
    let daylight = ((day + 0.25) / 0.5).clamp(0.0, 1.0);
    let sunlight = suns.iter().next().map_or(1.0, |l| l.illuminance / SUN_LUX);
    for (root, mut tf) in &mut roots {
        tf.rotation = Quat::from_rotation_x(angle as f32);
        if let Some(mut m) = windows.get_mut(&root.windows) {
            m.colony.centre.w = angle as f32;
            m.colony.shape.w = daylight * daylight * (3.0 - 2.0 * daylight);
            m.colony.sun.w = sunlight;
        }
    }
}

fn blink_beacons(time: Res<VisTime>, mut beacons: Query<(&Beacon, &mut Visibility)>) {
    for (b, mut v) in &mut beacons {
        let on = (time.now as f32 * 0.8 + b.phase).rem_euclid(1.0) < 0.12;
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}
