//! The First Colony from outside: a ring 2 km across and 600 m wide (`docs/RING.md`), turning once a
//! minute for 1 g at its floor, drawn after the angular colonies of the 1990s anime its owner chose
//! for it. Its hull is flat slabs round the rim with dark seams between them, ruled with rows of small
//! lights; its ceiling, facing the hole, is the window lattice, glowing with the town's day. In the
//! hole the docking hub's drum stands forward of the ring (−X), the pilots' bays round its face; the
//! spine runs back through the hole, six spokes reach out to the ceiling, and the louvres round the
//! spine open with the colony's day (`bc_sim::colony::time`) to throw the sun in.
//!
//! It turns on the simulation's clock (`bc_sim::world::colony_spin_angle`) at the view clock's
//! time, and its day runs on the same clock, so every client draws it at the same angle and hour.
//! Collision stays the simulation's still ring and hub (`bc_sim::world`, the hub landmark), which
//! don't care that the visual one spins.

use std::f32::consts::TAU;

use bc_sim::colony::hub::{
    BAY_DOOR_X, BAY_RADIUS, BAYS, DECK_HATCH_RADIUS, DRUM_RADIUS, DRUM_X, MOUTH_X, SPINE_RADIUS, SPINE_X,
    SPOKE_RADIUS, SPOKES, spoke_angle,
};
use bc_sim::colony::louvres::{LOUVRE_LENGTH, LOUVRE_THICKNESS, LOUVRE_WIDTH, LOUVRES, Louvre};
use bc_sim::colony::time::{Day, day};
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
use bc_sim::world::{
    COLONY_CENTER, COLONY_HALF_WIDTH, COLONY_INNER_RADIUS, COLONY_RADIUS, colony_spin_angle,
};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::dots::{Blink, DotLook, DotMaterial, Dots};
use crate::materials::{HullTag, Surfaces, paint};
use crate::view::{DrawnBodies, Vis, VisTime};

pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        // The retired cylinder's city (`city.rs`, behind the server's `--colony`) imports it.
        embedded_asset!(app, "shaders/city_lib.wgsl");
        app.init_resource::<ColonyDay>().add_systems(
            Update,
            ((spin, open_louvres).chain().in_set(Vis::Suits), blink_beacons, light_bays),
        );
    }
}

/// Keeps the city's shader library loaded (it's only imported).
#[derive(Resource)]
pub struct CityLib(#[allow(dead_code)] Handle<Shader>);

/// The retired cylinder's city atlas as an image: one RGBA8 texel a block, and under them the
/// roofs' height map (`bc_client_core::city_atlas::atlas`), for its city (`city.rs`).
pub fn atlas_image() -> Image {
    use bc_client_core::city_atlas::{IMAGE_H, IMAGE_W, atlas};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    static ATLAS: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    Image::new(
        Extent3d { width: IMAGE_W, height: IMAGE_H, depth_or_array_layers: 1 },
        TextureDimension::D2,
        ATLAS.get_or_init(|| atlas(bc_sim::colony::city::Stage(0))).clone(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A pilot's bay door on the drum's face, by its bay's number: open (not drawn) while a suit is
/// launching out of it.
#[derive(Component)]
pub struct BayDoor(pub u8);

/// The spinning part of the colony (everything visual), and the ceiling's glass, which glows with
/// the town's day.
#[derive(Component)]
struct ColonyRoot {
    glass: Handle<StandardMaterial>,
}

/// A bay's floods, lit while its door stands open: the drum's face looks away from the sun, and a
/// suit thrown out of it was a black shape against the ceiling's glass.
#[derive(Component)]
struct BayFlood(u8);

/// A blinking navigation light.
#[derive(Component)]
struct Beacon {
    phase: f32,
}

/// Louvre `k`'s plate, opened with the day.
#[derive(Component)]
struct LouvrePlate(u32);

/// The colony's hour as drawn this frame (its day runs on the view clock).
#[derive(Resource, Clone, Copy, Debug)]
pub struct ColonyDay(pub Day);

impl Default for ColonyDay {
    fn default() -> Self {
        Self(day(0, 0.0))
    }
}

/// Facets round the rim: the hull's slabs, flat, with seams where they meet.
const FACETS: u32 = 48;
/// The ceiling's frames: across the ring every this many metres of x.
const CEILING_BAY: f32 = 100.0;
/// How far a chamfer cuts the hull's corners, m.
const CHAMFER: f32 = 24.0;
/// The courses of slabs round the ring's sides, from and to how far out (m from the axis): in from
/// the ceiling, the dark band, out to the chamfer.
const SIDE_COURSES: [(f32, f32); 3] = [
    (COLONY_INNER_RADIUS + 14.0, COLONY_INNER_RADIUS + 96.0),
    (COLONY_INNER_RADIUS + 104.0, COLONY_INNER_RADIUS + 136.0),
    (COLONY_INNER_RADIUS + 144.0, COLONY_RADIUS - CHAMFER - 12.0),
];

/// Where louvre `k` is and how it's turned, opened `beta`: a unit cube stretched to the plate.
fn louvre_transform(k: u32, beta: f32) -> Transform {
    let l = Louvre::new(k, beta);
    Transform::from_translation(l.centre)
        .with_rotation(Quat::from_mat3(&Mat3::from_cols(l.along, l.face, l.across)))
        .with_scale(Vec3::new(LOUVRE_LENGTH, LOUVRE_THICKNESS, LOUVRE_WIDTH))
}

/// Point at `angle` round the axis, `r` from it, `x` along it (the colony's frame).
pub fn around(angle: f32, r: f32, x: f32) -> Vec3 {
    Vec3::new(x, r * angle.cos(), r * angle.sin())
}

/// Flat-shaded geometry laid out in one mesh: each vertex's colour carries the hull shader's paint
/// slot (`16 + paint`, or `48 + paint` lit), so one entity draws slabs of several paints.
#[derive(Default)]
pub struct Kit {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 4]>,
    i: Vec<u32>,
}

impl Kit {
    /// A flat quad `a b c d` (counter-clockwise seen from its front), painted `slot`.
    pub fn quad(&mut self, [a, b, c, d]: [Vec3; 4], slot: u8, bevel: f32) {
        let n = (b - a).cross(d - a).normalize_or_zero();
        let base = self.p.len() as u32;
        for v in [a, b, c, d] {
            self.p.push(v.to_array());
            self.n.push(n.to_array());
            self.c.push([f32::from(slot) / 255.0, bevel, (base % 89) as f32 / 97.0, 1.0]);
        }
        self.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A box: its middle, its three half-axes (a right-handed set), painted `slot`.
    pub fn cuboid(&mut self, c: Vec3, [u, v, w]: [Vec3; 3], slot: u8) {
        for (n, a, b) in [(u, v, w), (-u, w, v), (v, w, u), (-v, u, w), (w, u, v), (-w, v, u)] {
            let o = c + n;
            self.quad([o - a - b, o + a - b, o + a + b, o - a + b], slot, 0.3);
        }
    }

    /// A box standing on the axis's frame: its middle `r` out at `angle` and `x` along, sized `size`
    /// (x along the axis, y out from it, z round it).
    pub fn radial(&mut self, angle: f32, r: f32, x: f32, size: Vec3, slot: u8) {
        let out = around(angle, 1.0, 0.0);
        let round = Vec3::new(0.0, -angle.sin(), angle.cos());
        self.cuboid(
            around(angle, r, x),
            [Vec3::X * size.x * 0.5, out * size.y * 0.5, round * size.z * 0.5],
            slot,
        );
    }

    /// A faceted solid of revolution about X: each edge of `profile` (points (x, r), in order, the
    /// solid's outside on their right as x runs to the right and r up) swept round in `segs` flat
    /// facets.
    pub fn lathe(&mut self, profile: &[(f32, f32)], segs: u32, slot: u8) {
        for e in profile.windows(2) {
            let ((x0, r0), (x1, r1)) = (e[0], e[1]);
            for k in 0..segs {
                let (a0, a1) = (TAU * k as f32 / segs as f32, TAU * (k + 1) as f32 / segs as f32);
                self.quad(
                    [around(a0, r0, x0), around(a0, r1, x1), around(a1, r1, x1), around(a1, r0, x0)],
                    slot,
                    0.0,
                );
            }
        }
    }

    pub fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.p)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.n)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.c)
            .with_inserted_indices(Indices::U32(self.i))
    }
}

/// A paint, as a slot in a [`Kit`]'s vertex colour.
pub const fn painted(p: u8) -> u8 {
    16 + p
}

/// The ring's hull: the rim's slabs (each facet a flat slab standing a little proud of its
/// neighbours' seams), the chamfers and the sides, flat-shaded.
fn hull_kit() -> Kit {
    let mut k = Kit::default();
    let (h, out, inn) = (COLONY_HALF_WIDTH, COLONY_RADIUS, COLONY_INNER_RADIUS);
    // The rim: the floor's underside, chamfered into the sides.
    k.lathe(
        &[
            (h, inn),
            (h, out - CHAMFER),
            (h - CHAMFER, out),
            (-h + CHAMFER, out),
            (-h, out - CHAMFER),
            (-h, inn),
        ],
        FACETS,
        painted(paint::HULL),
    );
    // Slabs on the rim: two to a facet across the ring, a dark seam between each and the next.
    let facet = TAU / FACETS as f32;
    for f in 0..FACETS {
        let a = facet * (f as f32 + 0.5);
        for (x, w) in
            [(-h * 0.5 + CHAMFER * 0.25, h - CHAMFER * 1.5), (h * 0.5 - CHAMFER * 0.25, h - CHAMFER * 1.5)]
        {
            let slot = if (f + (x > 0.0) as u32).is_multiple_of(5) {
                painted(paint::HULL_DARK)
            } else {
                painted(paint::HULL)
            };
            k.radial(a, out * (facet * 0.5).cos() + 2.0, x, Vec3::new(w, 4.0, out * facet - 10.0), slot);
        }
        // A rib along every other seam, standing proud.
        if f % 2 == 0 {
            k.radial(
                facet * f as f32,
                out + 4.0,
                0.0,
                Vec3::new(2.0 * h - 2.0 * CHAMFER, 8.0, 6.0),
                painted(paint::HULL_DARK),
            );
        }
    }
    // The sides in courses of slabs, so they read as built from plates, not as one flat face: a
    // course in from the ceiling, a dark band round the middle, and a broad course out to the
    // chamfer, each slab a little proud of the face with a seam to the next, a few in white or in
    // service grey in a staggered rhythm (the references' panelled faces, never one even tone).
    for f in 0..FACETS {
        let a = facet * (f as f32 + 0.5);
        for (course, (r0, r1)) in SIDE_COURSES.iter().enumerate() {
            let r = (r0 + r1) * 0.5;
            let slot = match course {
                1 => painted(paint::HULL_DARK),
                _ if (f * 7 + course as u32 * 3).is_multiple_of(11) => painted(paint::WHITE),
                _ if (f + course as u32 * 2).is_multiple_of(4) => painted(paint::HULL_DARK),
                _ => painted(paint::HULL),
            };
            for side in [-1.0f32, 1.0] {
                k.radial(a, r, side * (h + 1.5), Vec3::new(3.0, r1 - r0, r * facet - 9.0), slot);
            }
        }
    }
    // Ribs round the sides: from the ceiling out to the chamfer, at every facet's edge.
    for f in 0..FACETS {
        let a = facet * f as f32;
        for side in [-1.0f32, 1.0] {
            k.radial(
                a,
                (inn + out - CHAMFER) * 0.5,
                side * (h + 3.0),
                Vec3::new(6.0, out - CHAMFER - inn, 8.0),
                painted(paint::HULL_DARK),
            );
        }
    }
    k
}

/// The ceiling's frames, seen through the hole: a lattice of girders round and across the ring,
/// standing in from the glass (the glass itself is drawn apart, lit).
fn lattice_kit() -> Kit {
    let mut k = Kit::default();
    let (h, inn) = (COLONY_HALF_WIDTH, COLONY_INNER_RADIUS);
    let facet = TAU / FACETS as f32;
    // Along the ring's width, at every facet's edge.
    for f in 0..FACETS {
        k.radial(facet * f as f32, inn - 4.0, 0.0, Vec3::new(2.0 * h, 8.0, 10.0), painted(paint::HULL_DARK));
    }
    // Round the ring every [`CEILING_BAY`] across it, the sides' rims among them.
    let n = (2.0 * h / CEILING_BAY).round() as i32;
    for j in 0..=n {
        let x = -h + 2.0 * h * j as f32 / n as f32;
        let w = if j == 0 || j == n { 16.0 } else { 8.0 };
        k.lathe(&[(x - w * 0.5, inn - 8.0), (x + w * 0.5, inn - 8.0)], FACETS, painted(paint::HULL_DARK));
        k.lathe(&[(x + w * 0.5, inn - 8.0), (x + w * 0.5, inn)], FACETS, painted(paint::HULL_DARK));
        k.lathe(&[(x - w * 0.5, inn), (x - w * 0.5, inn - 8.0)], FACETS, painted(paint::HULL_DARK));
    }
    k
}

/// The ceiling's glass, facing the hole: a faceted band at the ceiling.
fn glass_mesh() -> Mesh {
    let mut k = Kit::default();
    let h = COLONY_HALF_WIDTH;
    k.lathe(&[(-h, COLONY_INNER_RADIUS), (h, COLONY_INNER_RADIUS)], FACETS, 0);
    let mut m = k.mesh();
    m.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    m
}

/// The docking hub: the drum (a thick, chamfered disc, faceted), the spine, the spokes as slabs out
/// to the ceiling, and the deck hatch in the middle of the mouth.
fn hub_kit() -> Kit {
    let mut k = Kit::default();
    let (x0, x1) = DRUM_X;
    let (r, c) = (DRUM_RADIUS, 30.0);
    // The drum: its mouth (the face the bays open on), its rim, its back.
    k.lathe(
        &[(x1, SPINE_RADIUS), (x1, r - c), (x1 - c, r), (x0 + c, r), (x0, r - c), (x0, SPINE_RADIUS)],
        24,
        painted(paint::HULL),
    );
    // The deck hatch in the mouth's middle, a dark disc flush with its face: suits land on it, so
    // nothing stands proud of the face the landmark has them stand on.
    k.lathe(&[(x0 - 0.1, DECK_HATCH_RADIUS + 6.0), (x0 - 0.1, 0.0)], 24, painted(paint::HULL_DARK));
    // Slabs round the drum's rim, as on the ring's.
    for f in 0..24u32 {
        let a = TAU * (f as f32 + 0.5) / 24.0;
        k.radial(
            a,
            r + 2.0,
            (x0 + x1) * 0.5,
            Vec3::new(x1 - x0 - 2.0 * c - 12.0, 4.0, TAU * r / 24.0 - 14.0),
            painted(if f % 3 == 0 { paint::HULL_DARK } else { paint::HULL }),
        );
    }
    // The spine, through the hole to the ring's far side.
    k.lathe(
        &[
            (SPINE_X.1, 0.0),
            (SPINE_X.1, SPINE_RADIUS - 10.0),
            (SPINE_X.1 - 10.0, SPINE_RADIUS),
            (SPINE_X.0, SPINE_RADIUS),
        ],
        16,
        painted(paint::HULL_DARK),
    );
    // The spokes: flat slabs, broad along the axis, out to the ceiling, with a lift shaft's housing
    // where each meets it.
    for s in 0..SPOKES {
        let a = spoke_angle(s);
        let (r0, r1) = (SPINE_RADIUS, COLONY_INNER_RADIUS);
        k.radial(a, (r0 + r1) * 0.5, 0.0, Vec3::new(70.0, r1 - r0, SPOKE_RADIUS * 1.4), painted(paint::HULL));
        k.radial(
            a,
            (r0 + r1) * 0.5,
            0.0,
            Vec3::new(12.0, r1 - r0 - 20.0, SPOKE_RADIUS * 1.8),
            painted(paint::HULL_DARK),
        );
        k.radial(a, r1 - 30.0, 0.0, Vec3::new(90.0, 40.0, 50.0), painted(paint::HULL_DARK));
    }
    k
}

pub fn setup_colony(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut dot_materials: ResMut<Assets<DotMaterial>>,
    mut dot_look: ResMut<DotLook>,
    assets: Res<AssetServer>,
    surfaces: Res<Surfaces>,
) {
    commands.insert_resource(CityLib(assets.load("embedded://bc_client/shaders/city_lib.wgsl")));
    let dot_material = dot_look.get(&mut dot_materials);
    // The ceiling's glass: the town's daylight coming up through it (set each frame by the hour).
    let glass = standard.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.07, 0.1),
        emissive: LinearRgba::rgb(0.32, 0.5, 0.82),
        perceptual_roughness: 0.15,
        ..default()
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
    let hull = |paint: u8, seed: u8| HullTag::paint(paint, seed).tag();
    let beacon_mesh = meshes.add(Sphere::new(5.0).mesh().ico(1).expect("icosphere"));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    let root = commands
        .spawn((Transform::from_translation(COLONY_CENTER), Visibility::default()))
        .with_children(|c| {
            for (kit, seed) in [(hull_kit(), 1u8), (lattice_kit(), 2), (hub_kit(), 3)] {
                c.spawn((
                    Mesh3d(meshes.add(kit.mesh())),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL, seed),
                    NotShadowCaster,
                ));
            }
            c.spawn((Mesh3d(meshes.add(glass_mesh())), MeshMaterial3d(glass.clone()), NotShadowCaster));
            // The louvres, opening with the day.
            let noon = day(0, 0.0).mirror_beta;
            for k in 0..LOUVRES {
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.mirror.clone()),
                    HullTag::paint(paint::MIRROR, 60 + (k % 64) as u8).tag(),
                    louvre_transform(k, noon),
                    LouvrePlate(k),
                    NotShadowCaster,
                ));
            }
            // The bays' doors round the mouth, each a dark slab with lamps at its corners.
            let amber = Vec3::new(1.0, 0.55, 0.12) * 3.4;
            let lamp = Vec3::new(0.75, 1.0, 0.82) * 2.4;
            let green = Vec3::new(0.35, 1.0, 0.45) * 2.6;
            let warm = Vec3::new(1.0, 0.82, 0.55) * 2.6;
            let red = Vec3::new(1.0, 0.12, 0.06) * 5.0;
            let mut lamps = Dots::default();
            for b in 0..BAYS {
                let a = TAU * b as f32 / BAYS as f32;
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(surfaces.colony.clone()),
                    hull(paint::HULL_DARK, (b % 251) as u8),
                    Transform::from_translation(around(a, BAY_RADIUS, (MOUTH_X + BAY_DOOR_X) * 0.5))
                        .with_rotation(Quat::from_rotation_x(a))
                        .with_scale(Vec3::new(MOUTH_X - BAY_DOOR_X, 22.0, 20.0)),
                    NotShadowCaster,
                    BayDoor(b as u8 + 1),
                ));
                // Its floods, out in front of the door, shining back on its mouth and what leaves it:
                // tens of thousands of lux, to read against space's exposure (the Sun's 100,000).
                c.spawn((
                    PointLight {
                        intensity: 1.2e9,
                        range: 400.0,
                        color: Color::srgb(1.0, 0.94, 0.84),
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(around(a, BAY_RADIUS - 30.0, BAY_DOOR_X - 45.0)),
                    Visibility::Hidden,
                    BayFlood(b as u8 + 1),
                ));
                let out = around(a, 1.0, 0.0);
                for dr in [-13.0f32, 13.0] {
                    lamps.add(around(a, BAY_RADIUS, BAY_DOOR_X - 1.0) + out * dr, 1.6, amber, Blink::Steady);
                }
            }
            // The deck hatch, ringed in amber; the mouth's rim and the drum's back in rows.
            lamps.ring(MOUTH_X - 0.6, DECK_HATCH_RADIUS + 3.0, 24, 2.2, amber);
            lamps.ring(MOUTH_X - 2.0, BAY_RADIUS - 30.0, 120, 2.0, lamp);
            lamps.ring(MOUTH_X - 2.0, BAY_RADIUS - 100.0, 96, 1.8, green);
            lamps.ring(DRUM_X.1 + 2.0, DRUM_RADIUS - 40.0, 160, 2.4, lamp);
            // The spokes' edges, lamps the length of them; a red light where each meets the ceiling.
            for s in 0..SPOKES {
                let a = spoke_angle(s);
                for dx in [-36.0f32, 36.0] {
                    lamps.line(
                        around(a, SPINE_RADIUS + 10.0, dx),
                        around(a, COLONY_INNER_RADIUS - 50.0, dx),
                        24.0,
                        1.8,
                        warm,
                    );
                }
                lamps.add(
                    around(a, COLONY_INNER_RADIUS - 52.0, -50.0),
                    4.0,
                    red,
                    Blink::Every { period: 2.4, phase: s as f32 / 6.0, lit: 0.15 },
                );
            }
            // The ring: a row along every seam of the rim and the ceiling, rows round its sides'
            // edges (white at the hull, green at the ceiling), and red at the rim's corners.
            let (h, out, inn) = (COLONY_HALF_WIDTH, COLONY_RADIUS, COLONY_INNER_RADIUS);
            let facet = TAU / FACETS as f32;
            for f in 0..FACETS {
                let a = facet * f as f32;
                lamps.line(
                    around(a, out + 9.0, -h + CHAMFER),
                    around(a, out + 9.0, h - CHAMFER),
                    20.0,
                    1.8,
                    lamp,
                );
                lamps.line(around(a, inn - 10.0, -h), around(a, inn - 10.0, h), 25.0, 1.5, green * 0.8);
                for side in [-1.0f32, 1.0] {
                    lamps.line(
                        around(a, inn + 10.0, side * (h + 7.0)),
                        around(a, out - CHAMFER - 10.0, side * (h + 7.0)),
                        30.0,
                        1.8,
                        lamp * 0.7,
                    );
                }
            }
            for side in [-1.0f32, 1.0] {
                lamps.ring(side * (h - CHAMFER * 0.5), out - CHAMFER * 0.5 + 2.0, 240, 2.2, lamp);
                lamps.ring(side * (h + 2.0), inn + 3.0, 180, 2.0, green);
                // Rows round the rim ruling its slabs, as the ring's lights run in the references.
                for x in [h * 0.25, h * 0.75] {
                    lamps.ring(side * x, out + 6.5, 480, 1.4, lamp * 0.8);
                }
            }
            for i in 0..12 {
                let a = TAU * i as f32 / 12.0;
                for side in [-1.0f32, 1.0] {
                    c.spawn((
                        Mesh3d(beacon_mesh.clone()),
                        MeshMaterial3d(if i % 2 == 0 { beacon_red.clone() } else { beacon_white.clone() }),
                        Transform::from_translation(around(a, out + 8.0, side * h)),
                        Beacon { phase: i as f32 * 0.11 + side * 0.25 },
                    ));
                }
            }
            c.spawn((
                Mesh3d(meshes.add(lamps.mesh())),
                MeshMaterial3d(dot_material.clone()),
                NoFrustumCulling,
            ));
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
    commands.entity(root).insert(ColonyRoot { glass });
}

/// How far along a ray (unit `dir`) it meets the colony's ring, if it does within 60 km. The solid
/// is the simulation's still ring (`bc_sim::world::colony_sweep`).
pub fn ray_hit(origin: Vec3, dir: Vec3) -> Option<f32> {
    const REACH: f32 = 60_000.0;
    bc_sim::world::colony_sweep(origin, origin + dir * REACH, 0.0).map(|f| f * REACH)
}

/// Turns the colony as the simulation has it at the view clock's time, keeps its hour, and lights
/// the ceiling's glass with the town's day.
fn spin(
    bodies: Res<DrawnBodies>,
    mut hour: ResMut<ColonyDay>,
    mut roots: Query<(&ColonyRoot, &mut Transform)>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let t = bodies.t.max(0.0);
    let (tick, frac) = (t.floor() as u32, (t - t.floor()) as f32);
    let angle = colony_spin_angle(tick, frac);
    hour.0 = day(tick, frac);
    for (root, mut tf) in &mut roots {
        tf.rotation = Quat::from_rotation_x(angle);
        if let Some(mut m) = standard.get_mut(&root.glass) {
            // By day the town's light comes up through the glass, a pale sky blue (dim enough that
            // the lattice's panes still read against it); by night, its lamps.
            let d = hour.0.daylight;
            let night = Vec3::new(1.0, 0.7, 0.4) * 0.35 * hour.0.lamps;
            let lit = Vec3::new(0.32, 0.5, 0.82) * d + night;
            m.emissive = LinearRgba::rgb(lit.x, lit.y, lit.z);
        }
    }
}

/// Opens the louvres to the hour.
fn open_louvres(hour: Res<ColonyDay>, mut plates: Query<(&LouvrePlate, &mut Transform)>) {
    let beta = hour.0.mirror_beta;
    for (LouvrePlate(k), mut tf) in &mut plates {
        *tf = louvre_transform(*k, beta);
    }
}

/// Lights a bay's floods while its door is open (not drawn: `launch_shot::open_bay_doors`).
fn light_bays(
    doors: Query<(&BayDoor, &Visibility)>,
    mut floods: Query<(&BayFlood, &mut Visibility), Without<BayDoor>>,
) {
    for (flood, mut vis) in &mut floods {
        let open = doors.iter().any(|(d, v)| d.0 == flood.0 && *v == Visibility::Hidden);
        vis.set_if_neq(if open { Visibility::Inherited } else { Visibility::Hidden });
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
