//! What's left of an explosion after the flash: a shockwave shell racing outward
//! (`shaders/shock.wgsl`) and spinning chips of armour flying straight away; and the energy ring
//! a Twin Buster shot throws off round the muzzle (a flattened shell). All pooled.

use bc_sim::math::Rng;
use bevy::asset::embedded_asset;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

use crate::materials::{HullTag, Surfaces, paint};
use crate::view::VisTime;

const SHOCKS: usize = 8;
const CHIPS: usize = 64;

pub struct BlastPlugin;

impl Plugin for BlastPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/shock.wgsl");
        app.add_plugins(MaterialPlugin::<ShockMaterial>::default());
    }
}

/// A glowing shell, lit where it grazes the line of sight (see `shaders/shock.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ShockMaterial {
    #[uniform(0)]
    color: ShockColor,
}

impl ShockMaterial {
    /// `color` in raw HDR; `rim` sharpens the glow toward the silhouette (higher: thinner).
    pub fn new(color: Vec3, rim: f32) -> Self {
        Self { color: ShockColor { color: color.extend(rim) } }
    }
}

#[derive(ShaderType, Clone, Copy, Debug)]
struct ShockColor {
    color: Vec4,
}

/// MeshTag bit for a shell drawn as a flat ring (on the disc mesh).
const RING: u32 = 1 << 8;

impl Material for ShockMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/shock.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
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
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

#[derive(Component, Default)]
pub struct Shock {
    live: bool,
    pos: Vec3,
    vel: Vec3,
    born: f64,
    /// Seconds to full size.
    life: f32,
    radius: f32,
    /// A flat ring square to local z (else a sphere), and its orientation.
    ring: bool,
    rot: Quat,
}

#[derive(Component, Default)]
pub struct Chip {
    live: bool,
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    spin: Vec3,
    born: f64,
    life: f32,
    size: Vec3,
}

/// Blasts waiting to be placed (shells and chip bursts), and the shells' looks.
#[derive(Resource)]
pub struct Blasts {
    shocks: Vec<Shock>,
    chips: Vec<(Vec3, Vec3, u32)>,
    sphere: Handle<Mesh>,
    disc: Handle<Mesh>,
    fire: Handle<ShockMaterial>,
    plasma: Handle<ShockMaterial>,
    next_shock: usize,
    next_chip: usize,
    rng: Rng,
}

impl Blasts {
    /// A shockwave of `radius` (m) at the end of its expansion.
    pub fn shockwave(&mut self, pos: Vec3, vel: Vec3, radius: f32) {
        let rot = Quat::IDENTITY;
        self.shocks.push(Shock { live: true, pos, vel, born: 0.0, life: 0.8, radius, ring: false, rot });
    }

    /// An energy ring thrown off round a muzzle, square to the shot along `dir`.
    pub fn ring(&mut self, pos: Vec3, vel: Vec3, dir: Vec3, radius: f32) {
        let rot = Quat::from_rotation_arc(Vec3::Z, dir.normalize_or(Vec3::Z));
        self.shocks.push(Shock { live: true, pos, vel, born: 0.0, life: 0.4, radius, ring: true, rot });
    }

    /// `n` chips of armour flung from `pos`, inheriting `vel`.
    pub fn chips(&mut self, pos: Vec3, vel: Vec3, n: u32) {
        self.chips.push((pos, vel, n));
    }
}

pub fn setup_blasts(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shocks: ResMut<Assets<ShockMaterial>>,
    surfaces: Res<Surfaces>,
) {
    let sphere = meshes.add(Sphere::new(1.0).mesh().ico(4).expect("icosphere"));
    let disc = meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE));
    // In vacuum there's no air to carry a shock: what glows is a thin, fast shell of hot gas.
    let fire = shocks.add(ShockMaterial::new(Vec3::new(1.8, 1.1, 0.6), 8.0));
    let plasma = shocks.add(ShockMaterial::new(Vec3::new(7.0, 4.0, 12.0), 1.0));
    for _ in 0..SHOCKS {
        commands.spawn((
            Shock::default(),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(fire.clone()),
            MeshTag(0),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for i in 0..CHIPS {
        commands.spawn((
            Chip::default(),
            Mesh3d(cube.clone()),
            MeshMaterial3d(surfaces.armour.clone()),
            HullTag { heat: 20, ..HullTag::paint(paint::DARK, i as u8) }.tag(),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
    commands.insert_resource(Blasts {
        shocks: Vec::new(),
        chips: Vec::new(),
        sphere,
        disc,
        fire,
        plasma,
        next_shock: 0,
        next_chip: 0,
        rng: Rng::new(0xB1A5_7000),
    });
}

/// Starts queued blasts on pooled entities, then moves and fades the live ones.
#[allow(clippy::type_complexity)]
pub fn update_blasts(
    time: Res<VisTime>,
    mut blasts: ResMut<Blasts>,
    mut shocks: Query<
        (
            &mut Shock,
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &mut Mesh3d,
            &mut MeshMaterial3d<ShockMaterial>,
        ),
        Without<Chip>,
    >,
    mut chips: Query<(&mut Chip, &mut Transform, &mut Visibility), Without<Shock>>,
) {
    let now = time.now;
    let dt = time.dt;
    let b = &mut *blasts;
    for order in b.shocks.drain(..) {
        if let Some((mut s, _, _, _, mut mesh, mut mat)) = shocks.iter_mut().nth(b.next_shock % SHOCKS) {
            let (want_mesh, want_mat) = if order.ring { (&b.disc, &b.plasma) } else { (&b.sphere, &b.fire) };
            if mesh.0 != *want_mesh {
                mesh.0 = want_mesh.clone();
            }
            if mat.0 != *want_mat {
                mat.0 = want_mat.clone();
            }
            *s = Shock { born: now, ..order };
        }
        b.next_shock += 1;
    }
    for (pos, vel, n) in b.chips.drain(..) {
        for _ in 0..n {
            let dir = Vec3::new(b.rng.signed(), b.rng.signed(), b.rng.signed()).normalize_or(Vec3::Y);
            let size = Vec3::new(
                0.3 + b.rng.next_f32() * 1.4,
                0.1 + b.rng.next_f32() * 0.5,
                0.3 + b.rng.next_f32() * 1.2,
            );
            let chip = Chip {
                live: true,
                pos,
                vel: vel + dir * (15.0 + b.rng.next_f32() * 70.0),
                rot: Quat::from_axis_angle(dir, b.rng.next_f32() * 6.0),
                spin: Vec3::new(b.rng.signed(), b.rng.signed(), b.rng.signed()) * 9.0,
                born: now,
                life: 3.0 + b.rng.next_f32() * 3.0,
                size,
            };
            if let Some((mut c, ..)) = chips.iter_mut().nth(b.next_chip % CHIPS) {
                *c = chip;
            }
            b.next_chip += 1;
        }
    }
    for (s, mut tf, mut vis, mut tag, ..) in &mut shocks {
        let age = (now - s.born) as f32 / s.life.max(1e-3);
        let on = s.live && age < 1.0;
        set_visible(&mut vis, on);
        if on {
            // Fast at first, easing out.
            let r = (s.radius * (1.0 - (1.0 - age).powi(3))).max(0.5);
            tf.translation = s.pos + s.vel * (now - s.born) as f32;
            tf.rotation = s.rot;
            tf.scale = if s.ring { Vec3::new(r, r, 1.0) } else { Vec3::splat(r) };
            *tag = MeshTag(((1.0 - age) * 255.0) as u32 | if s.ring { RING } else { 0 });
        }
    }
    for (mut c, mut tf, mut vis) in &mut chips {
        let age = (now - c.born) as f32;
        let on = c.live && age < c.life;
        set_visible(&mut vis, on);
        if on {
            let (spin, vel) = (c.spin * dt, c.vel);
            c.pos += vel * dt;
            c.rot = (Quat::from_scaled_axis(spin) * c.rot).normalize();
            tf.translation = c.pos;
            tf.rotation = c.rot;
            // Shrink away over the last second.
            tf.scale = c.size * (c.life - age).clamp(0.0, 1.0);
        }
    }
}

fn set_visible(v: &mut Visibility, on: bool) {
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
}
