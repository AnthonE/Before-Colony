//! The sky and the sunlight.
//!
//! - `shaders/sky.wgsl` draws the sky at infinity: procedural stars and the Milky Way, the Sun,
//!   Earth and the Moon. At L1 they sit on opposite sides of the sky.
//! - The Sun is the one directional light WebGL2 allows, casting shadows on higher tiers and
//!   dimming when the camera passes into the colony's shadow.
//! - Image-based lighting generated here from the same sky gives night sides their blue
//!   Earthshine and metal something to reflect.

use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::Exposure;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{
    CascadeShadowConfigBuilder, DirectionalLightShadowMap, EnvironmentMapLight, NotShadowCaster,
    NotShadowReceiver,
};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, Extent3d, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::shader::ShaderRef;

use crate::camera::MainCamera;
use crate::gfx::Gfx;
use crate::view::VisTime;

/// Direction to the Sun. The colony's axis points roughly at it (its mirrors are at the sunward
/// +X cap), far enough off-axis that the hull catches the light.
pub const SUN_DIR: Vec3 = Vec3::new(0.84788, 0.34913, 0.39900);
/// Angular radius of the Sun's disc, radians (enlarged about 2.5×, so it reads at a glance).
const SUN_RADIUS: f32 = 0.012;
/// Direction to Earth, and its angular radius: vast and low in the sky.
pub const EARTH_DIR: Vec3 = Vec3::new(-0.29987, -0.54975, 0.77965);
const EARTH_RADIUS: f32 = 0.29;
/// The Moon, roughly opposite Earth (L1 lies between them), half lit.
pub const MOON_DIR: Vec3 = Vec3::new(0.35934, 0.49908, -0.78854);
const MOON_RADIUS: f32 = 0.065;
/// Normal of the Milky Way's plane.
pub const GALAXY_NORMAL: Vec3 = Vec3::new(0.4703, 0.7705, -0.4303);

/// Sunlight, lux (direct sunlight at 1 AU is about 100,000 lux, a little more above an atmosphere).
pub const SUN_LUX: f32 = 100_000.0;
/// Camera exposure: full sunlight on a white suit comes out near 1.0 on screen.
pub const EV100: f32 = 14.5;
/// Radiance of a white Lambertian surface in full sunlight, cd/m².
const WHITE: f32 = SUN_LUX / std::f32::consts::PI;
/// Radiance that lands near 1.0 on screen at [`EV100`].
const SCREEN: f32 = 27_800.0;

const SKY_SHADER: &str = "embedded://bc_client/shaders/sky.wgsl";

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sky.wgsl");
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .add_systems(Startup, setup_sky)
            .add_systems(Update, (apply_light_tier, eclipse, animate_sky));
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyMaterial {
    #[uniform(0)]
    sky: SkyUniform,
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct SkyUniform {
    /// xyz: direction; w: angular radius.
    sun: Vec4,
    earth: Vec4,
    moon: Vec4,
    /// xyz: the galaxy's plane normal; w: seconds.
    galaxy: Vec4,
    /// x: star brightness; y: detail 0..1; z: sun disc scale.
    params: Vec4,
}

impl Material for SkyMaterial {
    fn vertex_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Drawn from inside, behind everything (the vertex shader pins it to the far plane).
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        Ok(())
    }
}

/// The sky dome.
#[derive(Component)]
struct SkyDome(Handle<SkyMaterial>);

/// The Sun's light.
#[derive(Component)]
pub struct Sun;

/// Image-based lighting maps, generated once.
#[derive(Resource)]
struct SkyMaps {
    diffuse: Handle<Image>,
    specular: Handle<Image>,
}

fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
    mut images: ResMut<Assets<Image>>,
    gfx: Res<Gfx>,
) {
    let material = materials.add(SkyMaterial {
        sky: SkyUniform {
            sun: SUN_DIR.extend(SUN_RADIUS),
            earth: EARTH_DIR.extend(EARTH_RADIUS),
            moon: MOON_DIR.extend(MOON_RADIUS),
            galaxy: GALAXY_NORMAL.extend(0.0),
            params: Vec4::new(1.0, gfx.settings.sky_detail, 1.0, 0.0),
        },
    });
    commands.spawn((
        SkyDome(material.clone()),
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(2).expect("ico sphere"))),
        MeshMaterial3d(material),
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.spawn((
        Sun,
        DirectionalLight { illuminance: SUN_LUX, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(-SUN_DIR, Vec3::Y),
    ));
    let (diffuse, specular) = environment_maps(&mut images);
    commands.insert_resource(SkyMaps { diffuse, specular });
}

/// Exposure, image-based lighting, ambient fill and shadows for the current tier.
#[allow(clippy::too_many_arguments)]
fn apply_light_tier(
    mut commands: Commands,
    gfx: Res<Gfx>,
    maps: Res<SkyMaps>,
    cams: Query<Entity, With<MainCamera>>,
    added: Query<(), Added<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut skies: Query<&SkyDome>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
) {
    if !gfx.is_changed() && added.is_empty() {
        return;
    }
    let s = gfx.settings;
    for cam in &cams {
        let mut e = commands.entity(cam);
        e.insert(Exposure { ev100: EV100 });
        if s.ibl {
            e.insert(EnvironmentMapLight {
                diffuse_map: maps.diffuse.clone(),
                specular_map: maps.specular.clone(),
                intensity: 1.0,
                ..default()
            });
        } else {
            e.remove::<EnvironmentMapLight>();
        }
    }
    // Space has no ambient light; a faint fill keeps night sides readable. Without image-based
    // lighting, a stronger, Earth-blue fill stands in for it.
    ambient.color = Color::srgb(0.55, 0.65, 0.9);
    ambient.brightness = if s.ibl { 250.0 } else { 1_500.0 };
    for (e, mut light) in &mut suns {
        light.shadow_maps_enabled = s.shadows;
        commands.entity(e).insert(
            CascadeShadowConfigBuilder {
                num_cascades: s.cascades,
                minimum_distance: 0.5,
                first_cascade_far_bound: (s.shadow_distance * 0.25).min(200.0),
                maximum_distance: s.shadow_distance,
                ..default()
            }
            .build(),
        );
    }
    commands.insert_resource(DirectionalLightShadowMap { size: s.shadow_map });
    for dome in &mut skies {
        if let Some(mut m) = sky_materials.get_mut(&dome.0) {
            m.sky.params.y = s.sky_detail;
        }
    }
}

/// Whether the ray from `p` toward the Sun is blocked by the colony (hull or end caps).
fn colony_blocks(p: Vec3, s: Vec3) -> bool {
    let rel = p - COLONY_CENTER;
    // The hull: |(rel + t s).yz| = R.
    let (oy, oz, dy, dz) = (rel.y, rel.z, s.y, s.z);
    let a = dy * dy + dz * dz;
    let b = 2.0 * (oy * dy + oz * dz);
    let c = oy * oy + oz * oz - COLONY_RADIUS * COLONY_RADIUS;
    if a > 1e-9 {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / (2.0 * a);
            if t > 0.0 && (rel.x + t * s.x).abs() <= COLONY_HALF_LENGTH {
                return true;
            }
        }
    }
    // The end caps.
    if s.x.abs() > 1e-6 {
        for cap in [-COLONY_HALF_LENGTH, COLONY_HALF_LENGTH] {
            let t = (cap - rel.x) / s.x;
            if t > 0.0 {
                let y = rel.y + t * s.y;
                let z = rel.z + t * s.z;
                if y * y + z * z <= COLONY_RADIUS * COLONY_RADIUS {
                    return true;
                }
            }
        }
    }
    false
}

/// How much of the Sun's disc the camera sees past the colony, 0..1, sampled over the disc.
fn sun_visibility(p: Vec3) -> f32 {
    let u = SUN_DIR.any_orthonormal_vector();
    let v = SUN_DIR.cross(u);
    let r = SUN_RADIUS * 0.7;
    let samples = [SUN_DIR, SUN_DIR + u * r, SUN_DIR - u * r, SUN_DIR + v * r, SUN_DIR - v * r];
    let seen = samples.iter().filter(|s| !colony_blocks(p, s.normalize())).count();
    seen as f32 / samples.len() as f32
}

/// Dims the sunlight when the camera is in the colony's shadow, so a fight on the night side is
/// lit by its beams, blasts and Earthshine.
fn eclipse(
    time: Res<VisTime>,
    cams: Query<&Transform, With<MainCamera>>,
    mut suns: Query<&mut DirectionalLight, With<Sun>>,
    mut vis: Local<Option<f32>>,
) {
    let Ok(cam) = cams.single() else { return };
    let target = sun_visibility(cam.translation);
    let k = 1.0 - (-time.dt * 6.0).exp();
    let v = match *vis {
        Some(v) => v + (target - v) * k,
        None => target,
    };
    *vis = Some(v);
    for mut light in &mut suns {
        let want = SUN_LUX * v;
        if (light.illuminance - want).abs() > 1.0 {
            light.illuminance = want;
        }
    }
}

/// Slow cloud drift on Earth.
fn animate_sky(
    time: Res<VisTime>,
    skies: Query<&SkyDome>,
    mut materials: ResMut<Assets<SkyMaterial>>,
    mut last: Local<f64>,
) {
    if (time.now - *last).abs() < 0.5 {
        return;
    }
    *last = time.now;
    for dome in &skies {
        if let Some(mut m) = materials.get_mut(&dome.0) {
            m.sky.galaxy.w = (time.now % 100_000.0) as f32;
        }
    }
}

// --- Image-based lighting. --------------------------------------------------------------------------

/// Where a view ray meets a distant sphere (angular radius `ang`, direction `c`): its surface normal.
fn hit_sphere(d: Vec3, c: Vec3, ang: f32) -> Option<Vec3> {
    let r = ang.sin();
    let b = d.dot(c);
    let disc = b * b - (1.0 - r * r);
    if disc < 0.0 || b < 0.0 {
        return None;
    }
    let t = b - disc.sqrt();
    Some((d * t - c) / r)
}

/// Radiance (cd/m²) of the sky's low-frequency part in direction `d`: Earth's lit disc and blue
/// limb, the Moon, and a faint starlit floor. The Sun is left out: it is the directional light.
/// The broad strokes of `sky.wgsl`, for lighting.
fn env_radiance(d: Vec3) -> Vec3 {
    let floor = Vec3::new(0.8, 0.85, 1.0) * (0.0015 * SCREEN);
    if let Some(n) = hit_sphere(d, EARTH_DIR, EARTH_RADIUS) {
        let ndl = n.dot(SUN_DIR);
        // Average albedo of ocean, land and cloud.
        let lit = Vec3::new(0.28, 0.32, 0.4) * ndl.max(0.0) * WHITE;
        let rim = (1.0 - n.dot(-d).max(0.0)).powi(3);
        let haze = Vec3::new(0.3, 0.55, 1.0) * rim * smoothstep(-0.2, 0.4, ndl) * 0.45 * WHITE;
        return lit + haze;
    }
    let theta = d.dot(EARTH_DIR).clamp(-1.0, 1.0).acos();
    let above = (theta - EARTH_RADIUS) / (EARTH_RADIUS * 0.022);
    let mut glow = Vec3::ZERO;
    if above < 6.0 {
        let up = (d - EARTH_DIR * d.dot(EARTH_DIR)).normalize_or_zero();
        let lit = smoothstep(-0.35, 0.25, up.dot(SUN_DIR));
        glow = Vec3::new(0.25, 0.5, 1.0) * (-above).exp() * lit * 0.9 * WHITE;
    }
    if let Some(n) = hit_sphere(d, MOON_DIR, MOON_RADIUS) {
        return Vec3::splat(0.12 * n.dot(SUN_DIR).max(0.0) * WHITE) + glow;
    }
    floor + glow
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// World direction of texel (`x`, `y`) on cube face `face` (+X, -X, +Y, -Y, +Z, -Z). Bevy samples
/// its cubemaps with z negated (they are left-handed), so the world direction flips z back.
fn texel_dir(face: usize, x: usize, y: usize, size: usize) -> Vec3 {
    let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
    let v = 2.0 * (y as f32 + 0.5) / size as f32 - 1.0;
    let c = match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    };
    Vec3::new(c.x, c.y, -c.z).normalize()
}

/// The i-th of `n` Hammersley points in [0, 1)².
fn hammersley(i: u32, n: u32) -> Vec2 {
    Vec2::new(i as f32 / n as f32, i.reverse_bits() as f32 * 2.328_306_4e-10)
}

/// A GGX-distributed direction around `n` (the split-sum "N = V = R" prefilter).
fn ggx_sample(xi: Vec2, n: Vec3, alpha: f32) -> Vec3 {
    let phi = std::f32::consts::TAU * xi.x;
    let cos_t = ((1.0 - xi.y) / (1.0 + (alpha * alpha - 1.0) * xi.y)).sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let h_t = Vec3::new(sin_t * phi.cos(), sin_t * phi.sin(), cos_t);
    let tx = n.any_orthonormal_vector();
    let ty = n.cross(tx);
    let h = (tx * h_t.x + ty * h_t.y + n * h_t.z).normalize();
    (2.0 * n.dot(h) * h - n).normalize()
}

/// One cube face of texels (`size`², RGBA half floats) from `f(direction)`.
fn push_face(out: &mut Vec<u8>, face: usize, size: usize, f: &mut dyn FnMut(Vec3) -> Vec3) {
    for y in 0..size {
        for x in 0..size {
            let c = f(texel_dir(face, x, y, size));
            for v in [c.x, c.y, c.z, 1.0] {
                out.extend_from_slice(&half::f16::from_f32(v.min(60_000.0)).to_le_bytes());
            }
        }
    }
}

fn cubemap(size: usize, levels: u32, data: Vec<u8>) -> Image {
    let mut image = Image::new_uninit(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 6 },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    image
}

/// Diffuse (irradiance) and GGX-prefiltered specular cubemaps of [`env_radiance`].
fn environment_maps(images: &mut Assets<Image>) -> (Handle<Image>, Handle<Image>) {
    // Fixed sample directions over the whole sphere, with their radiance, for the diffuse map.
    const SPHERE: u32 = 4096;
    let samples: Vec<(Vec3, Vec3)> = (0..SPHERE)
        .map(|i| {
            let p = hammersley(i, SPHERE);
            let z = 1.0 - 2.0 * p.y;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let phi = std::f32::consts::TAU * p.x;
            let d = Vec3::new(r * phi.cos(), z, r * phi.sin());
            (d, env_radiance(d))
        })
        .collect();
    const DIFFUSE: usize = 16;
    let mut diffuse = Vec::with_capacity(DIFFUSE * DIFFUSE * 6 * 8);
    for face in 0..6 {
        // E(n)/π = (1/π) ∫ L cosθ dω, with dω = 4π / SPHERE per sample.
        push_face(&mut diffuse, face, DIFFUSE, &mut |n| {
            let sum: Vec3 = samples.iter().map(|(d, l)| *l * n.dot(*d).max(0.0)).sum();
            sum * (4.0 / SPHERE as f32)
        });
    }
    const SPECULAR: usize = 64;
    const LEVELS: u32 = 7;
    const GGX: u32 = 48;
    let mut specular = Vec::new();
    for face in 0..6 {
        for level in 0..LEVELS {
            let size = SPECULAR >> level;
            // Bevy picks the mip as perceptual roughness × (levels - 1).
            let roughness = level as f32 / (LEVELS - 1) as f32;
            let alpha = (roughness * roughness).max(1e-3);
            push_face(&mut specular, face, size, &mut |r| {
                if level == 0 {
                    return env_radiance(r);
                }
                let mut sum = Vec3::ZERO;
                let mut weight = 0.0;
                for i in 0..GGX {
                    let l = ggx_sample(hammersley(i, GGX), r, alpha);
                    let w = r.dot(l);
                    if w > 0.0 {
                        sum += env_radiance(l) * w;
                        weight += w;
                    }
                }
                sum / weight.max(1e-6)
            });
        }
    }
    (images.add(cubemap(DIFFUSE, 1, diffuse)), images.add(cubemap(SPECULAR, LEVELS, specular)))
}
