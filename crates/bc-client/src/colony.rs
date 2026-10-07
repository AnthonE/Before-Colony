//! The First Colony: an Island-3 cylinder 6.4 km across and 32 km long, turning once every 113 s for
//! 1 g at the rim. Three land strips alternate with three windows that look into the colony
//! (`shaders/colony_window.wgsl`), structural rings band the hull, the end caps carry the docking
//! hub and the axis port, and three mirrors hinged at the docking hub's end open towards the sun
//! with the colony's day (`bc_sim::colony::time`) to throw its light in.
//!
//! It turns on the simulation's clock (`bc_sim::world::colony_spin_angle`) at the view clock's
//! time, and its day runs on the same clock, so every client draws it at the same angle and hour.
//! Collision stays the simulation's static cylinder (`bc_sim::world`), which doesn't care that the
//! visual one spins.

use std::f32::consts::{FRAC_PI_3, FRAC_PI_6, TAU};

use bc_sim::colony::frame::{FIRST_WINDOW, STRIPS, window_centre};
use bc_sim::colony::hub::{
    BAY_DOOR_X, BAY_RADIUS, BAY_RING_INNER, BAY_RING_OUTER, BAY_RING_X, BAYS, DECK_HATCH_RADIUS,
    SPIRE_RADIUS, SPIRE_TIERS, SPOKES,
};
use bc_sim::colony::mirrors::{MIRROR_LENGTH, MIRROR_THICKNESS, MIRROR_WIDTH, Mirror};
use bc_sim::colony::time::{Day, day};
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_HUB_LENGTH, DOCK_RADIUS};
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS, colony_spin_angle};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use crate::dots::{Blink, DotLook, DotMaterial, Dots};
use crate::materials::{HullTag, Surfaces, paint};
use crate::sky::{SUN_DIR, SUN_LUX, Sun};
use crate::view::{DrawnBodies, Vis, VisTime};

pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/colony_window.wgsl");
        embedded_asset!(app, "shaders/city_lib.wgsl");
        app.add_plugins(MaterialPlugin::<WindowMaterial>::default())
            .init_resource::<ColonyDay>()
            .add_systems(Update, ((spin, open_mirrors).chain().in_set(Vis::Suits), blink_beacons));
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct WindowMaterial {
    #[uniform(0)]
    colony: ColonyUniform,
    /// The city's block atlas (`bc_client_core::city_atlas`): what the windows show of it.
    #[texture(1)]
    atlas: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct ColonyUniform {
    /// xyz: centre; w: spin angle.
    centre: Vec4,
    /// x: radius; y: half-length; z: first window's angle; w: daylight.
    shape: Vec4,
    /// xyz: direction to the Sun; w: sunlight scale.
    sun: Vec4,
    /// x: how many of the city's lamps are lit; yzw: unused.
    extra: Vec4,
}

impl Material for WindowMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/colony_window.wgsl".into()
    }
}

/// Keeps the city's shader library loaded (it's only imported).
#[derive(Resource)]
pub struct CityLib(#[allow(dead_code)] Handle<Shader>);

/// The city's block atlas as an image: one RGBA8 texel a block.
pub fn atlas_image() -> Image {
    use bc_client_core::city_atlas::{ATLAS_H, ATLAS_W, block_atlas};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    Image::new(
        Extent3d { width: ATLAS_W, height: ATLAS_H, depth_or_array_layers: 1 },
        TextureDimension::D2,
        block_atlas(bc_sim::colony::city::Stage(0)),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A pilot's bay door on the bay ring, by its bay's number: open (not drawn) while a suit is
/// launching out of it.
#[derive(Component)]
pub struct BayDoor(pub u8);

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

/// Mirror `k`'s plate, opened with the day.
#[derive(Component)]
struct MirrorPlate(usize);

/// The lamps along mirror `k`'s edges, in its own frame (hinge at the origin, +X along it, +Z
/// across).
#[derive(Component)]
struct MirrorFrame(usize);

/// A light on a corner of mirror `k`'s far edge (`side` 0 or 1).
#[derive(Component)]
struct MirrorTip {
    k: usize,
    side: usize,
}

/// The colony's hour as drawn this frame (its day runs on the view clock).
#[derive(Resource, Clone, Copy, Debug)]
pub struct ColonyDay(pub Day);

impl Default for ColonyDay {
    fn default() -> Self {
        Self(day(0, 0.0))
    }
}

/// Mirror `k`'s own frame opened `beta`: the hinge's middle, +X along it, +Y out of its back, +Z
/// across.
fn mirror_frame(k: usize, beta: f32) -> Transform {
    let m = Mirror::new(k, beta);
    Transform::from_translation(m.hinge)
        .with_rotation(Quat::from_mat3(&Mat3::from_cols(m.along, m.back, m.across)))
}

/// Where mirror `k` is and how it's turned, opened `beta`: a unit cube stretched to the plate.
fn mirror_transform(k: usize, beta: f32) -> Transform {
    let m = Mirror::new(k, beta);
    Transform::from_translation(m.centre())
        .with_rotation(Quat::from_mat3(&Mat3::from_cols(m.along, m.back, m.across)))
        .with_scale(Vec3::new(MIRROR_LENGTH, MIRROR_THICKNESS, MIRROR_WIDTH))
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

/// A solid of revolution about X from a closed profile of (x, r) points, counter-clockwise with x
/// to the right and r up (so its faces face out), each edge a flat band with its own normals.
fn lathe(profile: &[(f32, f32)], segs: u32) -> Mesh {
    let mut p = Vec::new();
    let mut n = Vec::new();
    let mut idx = Vec::new();
    for (e, &(x0, r0)) in profile.iter().enumerate() {
        let (x1, r1) = profile[(e + 1) % profile.len()];
        // Outward in the (x, r) half-plane: (dr, −dx), turned round with each angle.
        let (nx, nr) = (r1 - r0, -(x1 - x0));
        let len = (nx * nx + nr * nr).sqrt().max(1e-6);
        let base = p.len() as u32;
        for (x, r) in [(x0, r0), (x1, r1)] {
            for i in 0..=segs {
                let a = TAU * i as f32 / segs as f32;
                p.push(around(a, r, x));
                n.push(Vec3::new(nx / len, nr / len * a.cos(), nr / len * a.sin()));
            }
        }
        for i in 0..segs {
            let (a, b) = (base + i, base + segs + 1 + i);
            idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    mesh(p, n, idx)
}

/// A box on the hull or a hub, stretched to `size` (x along the axis, y out from it, z round it),
/// with its middle `r` out at angle `a` and `x` along.
fn radial_box(a: f32, r: f32, x: f32, size: Vec3) -> Transform {
    Transform::from_translation(around(a, r, x)).with_rotation(Quat::from_rotation_x(a)).with_scale(size)
}

#[allow(clippy::too_many_arguments)]
pub fn setup_colony(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut windows: ResMut<Assets<WindowMaterial>>,
    mut dot_materials: ResMut<Assets<DotMaterial>>,
    mut dot_look: ResMut<DotLook>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
    surfaces: Res<Surfaces>,
) {
    commands.insert_resource(CityLib(assets.load("embedded://bc_client/shaders/city_lib.wgsl")));
    let dot_material = dot_look.get(&mut dot_materials);
    let window_material = windows.add(WindowMaterial {
        colony: ColonyUniform {
            centre: COLONY_CENTER.extend(0.0),
            shape: Vec4::new(COLONY_RADIUS, COLONY_HALF_LENGTH, FIRST_WINDOW, 1.0),
            sun: SUN_DIR.extend(1.0),
            extra: Vec4::ZERO,
        },
        atlas: images.add(atlas_image()),
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
            let noon = day(0, 0.0).mirror_beta;
            for k in 0..STRIPS {
                let w = window_centre(k);
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
                // The window's mirror, hinged at the docking hub's end, opening with the day.
                c.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                    MeshMaterial3d(surfaces.mirror.clone()),
                    HullTag { metal: true, ..HullTag::paint(paint::MIRROR, 60 + k as u8) }.tag(),
                    mirror_transform(k, noon),
                    MirrorPlate(k),
                    NotShadowCaster,
                ));
                // Lamps along its edges, standing a little off them.
                let mut lamps = Dots::default();
                let (l, h) = (MIRROR_LENGTH, MIRROR_WIDTH * 0.5);
                let frame_lamp = Vec3::new(0.75, 1.0, 0.82) * 2.4;
                lamps.line(Vec3::new(-4.0, 0.0, -h), Vec3::new(-4.0, 0.0, h), 100.0, 3.0, frame_lamp);
                lamps.line(Vec3::new(l + 4.0, 0.0, -h), Vec3::new(l + 4.0, 0.0, h), 100.0, 3.0, frame_lamp);
                for z in [-h - 4.0, h + 4.0] {
                    lamps.line(Vec3::new(0.0, 0.0, z), Vec3::new(l, 0.0, z), 100.0, 3.0, frame_lamp);
                }
                c.spawn((
                    Mesh3d(meshes.add(lamps.mesh())),
                    MeshMaterial3d(dot_material.clone()),
                    mirror_frame(k, noon),
                    MirrorFrame(k),
                    NoFrustumCulling,
                ));
                // Lights at its far corners.
                for (side, at) in Mirror::new(k, noon).tips().into_iter().enumerate() {
                    c.spawn((
                        Mesh3d(beacon_mesh.clone()),
                        MeshMaterial3d(beacon_red.clone()),
                        Transform::from_translation(at),
                        Beacon { phase: k as f32 * 0.37 + side as f32 * 0.5 },
                        MirrorTip { k, side },
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
                // The docking hub (−X; the dock is off its mouth) and the axis port (+X).
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
            // The docking hub, as built: modules stacked on the spire, the bay ring standing off the
            // end cap where the bays hang at 0.7 g, and the spokes between them.
            let lamp = Vec3::new(0.75, 1.0, 0.82) * 2.4;
            let warm = Vec3::new(1.0, 0.82, 0.55) * 2.6;
            let amber = Vec3::new(1.0, 0.55, 0.12) * 3.4;
            let red = Vec3::new(1.0, 0.12, 0.06) * 5.0;
            let mut lamps = Dots::default();
            // The deck hatch in the middle of the hub's mouth, where a suit landed on the face walks
            // in: a dark plate ringed with amber lamps.
            let mouth = -(COLONY_HALF_LENGTH + DOCK_HUB_LENGTH);
            c.spawn((
                Mesh3d(meshes.add(Circle::new(DECK_HATCH_RADIUS))),
                MeshMaterial3d(surfaces.colony.clone()),
                hull(paint::HULL_DARK, 94),
                Transform::from_xyz(mouth - 0.4, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
                NotShadowCaster,
            ));
            lamps.ring(mouth - 0.9, DECK_HATCH_RADIUS, 32, 2.6, amber);
            lamps.ring(mouth - 0.9, DECK_HATCH_RADIUS * 0.45, 12, 2.0, warm);
            for (i, (x, r, t)) in SPIRE_TIERS.into_iter().enumerate() {
                let (x0, x1) = (x - t / 2.0, x + t / 2.0);
                let chamfer = t * 0.3;
                c.spawn((
                    Mesh3d(meshes.add(lathe(
                        &[
                            (x0, SPIRE_RADIUS),
                            (x1, SPIRE_RADIUS),
                            (x1, r - chamfer),
                            (x1 - chamfer, r),
                            (x0 + chamfer, r),
                            (x0, r - chamfer),
                        ],
                        72,
                    ))),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(if i % 2 == 0 { paint::HULL } else { paint::HULL_DARK }, 100 + i as u8),
                    NotShadowCaster,
                ));
                lamps.ring(x0 - 2.0, r - chamfer * 0.5, 64, 3.0, lamp);
            }
            let (rx0, rx1) = BAY_RING_X;
            let ch = 40.0;
            c.spawn((
                Mesh3d(meshes.add(lathe(
                    &[
                        (rx0, BAY_RING_INNER + ch),
                        (rx0 + ch, BAY_RING_INNER),
                        (rx1 - ch, BAY_RING_INNER),
                        (rx1, BAY_RING_INNER + ch),
                        (rx1, BAY_RING_OUTER - ch),
                        (rx1 - ch, BAY_RING_OUTER),
                        (rx0 + ch, BAY_RING_OUTER),
                        (rx0, BAY_RING_OUTER - ch),
                    ],
                    192,
                ))),
                MeshMaterial3d(surfaces.colony.clone()),
                hull(paint::HULL, 110),
                NotShadowCaster,
            ));
            for x in [rx0 - 3.0, rx1 + 3.0] {
                lamps.ring(x, BAY_RING_OUTER - ch * 0.5, 280, 3.5, lamp);
                lamps.ring(x, BAY_RING_INNER + ch * 0.5, 205, 3.0, lamp);
            }
            // The bays' doors on the ring's outer face, each with a lamp at its corners.
            for b in 0..BAYS {
                let a = TAU * b as f32 / BAYS as f32;
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, (b % 251) as u8),
                    radial_box(
                        a,
                        BAY_RADIUS,
                        (rx0 + BAY_DOOR_X) * 0.5,
                        Vec3::new(rx0 - BAY_DOOR_X, 32.0, 40.0),
                    ),
                    NotShadowCaster,
                    BayDoor(b as u8 + 1),
                ));
                for (dr, dz) in [(-16.0f32, -20.0f32), (-16.0, 20.0), (16.0, -20.0), (16.0, 20.0)] {
                    let up = around(a, 1.0, 0.0);
                    let side = Vec3::new(0.0, -a.sin(), a.cos());
                    lamps.add(
                        around(a, BAY_RADIUS, rx0 - 8.0) + up * dr + side * dz,
                        2.2,
                        amber,
                        Blink::Steady,
                    );
                }
            }
            for k in 0..SPOKES {
                let a = TAU * (k as f32 + 0.5) / SPOKES as f32;
                let (r0, r1) = (SPIRE_RADIUS, BAY_RING_INNER);
                let x = (rx0 + rx1) / 2.0;
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, 120 + k as u8),
                    radial_box(a, (r0 + r1) / 2.0, x, Vec3::new(90.0, r1 - r0, 60.0)),
                    NotShadowCaster,
                ));
                for dx in [-48.0f32, 48.0] {
                    lamps.line(around(a, r0, x + dx), around(a, r1, x + dx), 50.0, 2.6, warm);
                }
                lamps.add(
                    around(a, r1 - 30.0, rx0 - 6.0),
                    5.0,
                    red,
                    Blink::Every { period: 2.4, phase: k as f32 / 6.0, lit: 0.15 },
                );
            }
            // Frames along the windows' edges, and lamps on them and on the rings' edges, outlining
            // the hull from far off.
            for k in 0..STRIPS {
                for edge in [-FRAC_PI_6, FRAC_PI_6] {
                    let a = window_centre(k) + edge;
                    c.spawn((
                        Mesh3d(cube.clone()),
                        MeshMaterial3d(surfaces.colony.clone()),
                        hull(paint::HULL_DARK, 130 + k as u8),
                        radial_box(
                            a,
                            COLONY_RADIUS + 5.0,
                            0.0,
                            Vec3::new(2.0 * COLONY_HALF_LENGTH, 10.0, 18.0),
                        ),
                        NotShadowCaster,
                    ));
                    lamps.line(
                        around(a, COLONY_RADIUS + 12.0, -COLONY_HALF_LENGTH),
                        around(a, COLONY_RADIUS + 12.0, COLONY_HALF_LENGTH),
                        90.0,
                        3.0,
                        lamp,
                    );
                }
            }
            for i in 0..=16 {
                // A sparse row round each ring, dimmer than the frames': the hull's bands read from
                // afar without wiring it up.
                let x = -COLONY_HALF_LENGTH + 2_000.0 * i as f32;
                lamps.ring(x, COLONY_RADIUS + 15.0, 96, 3.0, lamp * 0.45);
            }
            // The axis port's end (+X), still being finished: scaffold towers and cranes on the cap.
            for k in 0..4 {
                let a = TAU * (k as f32 + 0.25) / 4.0;
                let r = 1_100.0 + 300.0 * (k % 2) as f32;
                let height = 260.0 + 60.0 * k as f32;
                let x = COLONY_HALF_LENGTH + height / 2.0;
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, 140 + k as u8),
                    radial_box(a, r, x, Vec3::new(height, 24.0, 24.0)),
                    NotShadowCaster,
                ));
                let top = COLONY_HALF_LENGTH + height;
                lamps.line(
                    around(a, r + 14.0, COLONY_HALF_LENGTH),
                    around(a, r + 14.0, top),
                    30.0,
                    2.4,
                    warm,
                );
                if k % 2 == 0 {
                    // A crane's boom off the tower's top, reaching in towards the axis.
                    let reach = 520.0;
                    c.spawn((
                        Mesh3d(cube.clone()),
                        MeshMaterial3d(surfaces.colony.clone()),
                        hull(paint::HULL, 150 + k as u8),
                        radial_box(a, r - reach / 2.0, top - 8.0, Vec3::new(14.0, reach, 12.0)),
                        NotShadowCaster,
                    ));
                    lamps.line(around(a, r, top + 2.0), around(a, r - reach, top + 2.0), 40.0, 2.4, warm);
                    lamps.add(
                        around(a, r - reach, top + 6.0),
                        6.0,
                        red,
                        Blink::Every { period: 1.6, phase: 0.3 * k as f32, lit: 0.2 },
                    );
                }
            }
            c.spawn((
                Mesh3d(meshes.add(lamps.mesh())),
                MeshMaterial3d(dot_material.clone()),
                NoFrustumCulling,
            ));
        })
        .id();
    commands.entity(root).insert(ColonyRoot { windows: window_material });
}

/// Turns the colony as the simulation has it at the view clock's time, keeps its hour, and tells
/// the window shader the spin, the daylight and the sunlight.
fn spin(
    bodies: Res<DrawnBodies>,
    mut hour: ResMut<ColonyDay>,
    mut roots: Query<(&ColonyRoot, &mut Transform)>,
    suns: Query<&DirectionalLight, With<Sun>>,
    mut windows: ResMut<Assets<WindowMaterial>>,
) {
    let t = bodies.t.max(0.0);
    let (tick, frac) = (t.floor() as u32, (t - t.floor()) as f32);
    let angle = colony_spin_angle(tick, frac);
    hour.0 = day(tick, frac);
    let sunlight = suns.iter().next().map_or(1.0, |l| l.illuminance / SUN_LUX);
    for (root, mut tf) in &mut roots {
        tf.rotation = Quat::from_rotation_x(angle);
        if let Some(mut m) = windows.get_mut(&root.windows) {
            m.colony.centre.w = angle;
            m.colony.shape.w = hour.0.daylight;
            m.colony.sun.w = sunlight;
            m.colony.extra.x = hour.0.lamps;
        }
    }
}

/// Opens the mirrors to the hour.
#[allow(clippy::type_complexity)]
fn open_mirrors(
    hour: Res<ColonyDay>,
    mut plates: Query<(&MirrorPlate, &mut Transform), (Without<MirrorTip>, Without<MirrorFrame>)>,
    mut frames: Query<(&MirrorFrame, &mut Transform), (Without<MirrorTip>, Without<MirrorPlate>)>,
    mut tips: Query<(&MirrorTip, &mut Transform), (Without<MirrorPlate>, Without<MirrorFrame>)>,
) {
    let beta = hour.0.mirror_beta;
    for (MirrorPlate(k), mut tf) in &mut plates {
        *tf = mirror_transform(*k, beta);
    }
    for (MirrorFrame(k), mut tf) in &mut frames {
        *tf = mirror_frame(*k, beta);
    }
    for (tip, mut tf) in &mut tips {
        tf.translation = Mirror::new(tip.k, beta).tips()[tip.side];
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
