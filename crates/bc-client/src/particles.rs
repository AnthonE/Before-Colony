//! Effect particles, simulated on the CPU and drawn as one mesh (`shaders/particles.wgsl`): every
//! spark, ember, flash, fireball and puff of vapour in a single draw call. The pool is sized by
//! the tier, the mesh is rewritten every frame, and the physics is vacuum's: no drag, no gravity,
//! sparks fly straight.

use bc_sim::math::Rng;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues,
};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;

use crate::camera::MainCamera;
use crate::gfx::Gfx;
use crate::view::VisTime;

const SHADER: &str = "embedded://bc_client/shaders/particles.wgsl";
const ATTRIBUTE_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("ParticleCorner", 0x5EC0_0001, VertexFormat::Float32x2);
const ATTRIBUTE_SHAPE: MeshVertexAttribute =
    MeshVertexAttribute::new("ParticleShape", 0x5EC0_0002, VertexFormat::Float32x4);
const ATTRIBUTE_DIR: MeshVertexAttribute =
    MeshVertexAttribute::new("ParticleDir", 0x5EC0_0003, VertexFormat::Float32x3);
const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/particles.wgsl");
        app.add_plugins(MaterialPlugin::<ParticleMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
pub struct ParticleMaterial {}

impl Material for ParticleMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
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
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_CORNER.at_shader_location(1),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(2),
            ATTRIBUTE_SHAPE.at_shader_location(3),
            ATTRIBUTE_DIR.at_shader_location(4),
        ])?];
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// How a particle's colour and opacity change over its life (raw HDR, like the other glows).
#[derive(Clone, Copy, Debug)]
pub enum Ramp {
    /// White-hot to orange to red: sparks and molten droplets.
    Spark,
    /// Orange to dull red, slowly.
    Ember,
    /// A fireball puff: white, yellow, orange, then dark.
    Fire,
    /// A blinding instant.
    Flash,
    /// Grey-blue vapour, alpha-blended.
    Vapour,
    /// Rock dust, alpha-blended: slow to clear.
    Dust,
    /// A glow of the given colour that fades out (beam plasma, saber, charge).
    Glow(Vec3),
}

impl Ramp {
    /// Colour and opacity at `t` in [0, 1] of the particle's life.
    fn at(self, t: f32) -> (Vec3, f32) {
        let mix = |a: Vec3, b: Vec3, u: f32| a + (b - a) * u.clamp(0.0, 1.0);
        match self {
            Ramp::Spark => {
                let c = if t < 0.3 {
                    mix(Vec3::new(14.0, 11.0, 7.0), Vec3::new(10.0, 4.5, 1.0), t / 0.3)
                } else {
                    mix(Vec3::new(10.0, 4.5, 1.0), Vec3::new(2.5, 0.35, 0.05), (t - 0.3) / 0.7)
                };
                (c, 1.0 - t * t)
            }
            Ramp::Ember => (mix(Vec3::new(5.0, 1.8, 0.35), Vec3::new(0.8, 0.12, 0.02), t), 1.0 - t),
            Ramp::Fire => {
                let c = if t < 0.25 {
                    mix(Vec3::new(24.0, 18.0, 10.0), Vec3::new(14.0, 6.5, 1.6), t / 0.25)
                } else {
                    mix(Vec3::new(14.0, 6.5, 1.6), Vec3::new(0.6, 0.12, 0.04), (t - 0.25) / 0.75)
                };
                (c, (1.0 - t).powf(1.5))
            }
            Ramp::Flash => (Vec3::new(40.0, 36.0, 30.0), (1.0 - t).powi(3)),
            Ramp::Vapour => (Vec3::new(0.32, 0.35, 0.4), 0.35 * (1.0 - t) * (1.0 - t)),
            Ramp::Dust => (Vec3::new(0.3, 0.26, 0.22), 0.5 * (1.0 - t)),
            Ramp::Glow(c) => (c, (1.0 - t) * (1.0 - t)),
        }
    }

    fn puff(self) -> bool {
        matches!(self, Ramp::Vapour | Ramp::Dust)
    }
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: (f32, f32),
    /// Streak length, as seconds of motion (0: a round billboard).
    streak: f32,
    core: f32,
    ramp: Ramp,
}

/// The particle pool and its mesh.
#[derive(Resource)]
pub struct Particles {
    live: Vec<Particle>,
    rng: Rng,
    mesh: Handle<Mesh>,
}

/// A burst's size and where it's flying.
#[derive(Clone, Copy, Debug)]
pub struct At {
    pub pos: Vec3,
    /// Velocity the debris inherits (the suit's, say).
    pub vel: Vec3,
}

impl Particles {
    fn spawn(&mut self, cap: usize, p: Particle) {
        if self.live.len() < cap {
            self.live.push(p);
        } else if cap > 0 {
            // Full: replace a random one (a fading spark is rarely missed).
            let i = self.rng.next_u32() as usize % self.live.len();
            self.live[i] = p;
        }
    }

    fn unit(&mut self) -> Vec3 {
        Vec3::new(self.rng.signed(), self.rng.signed(), self.rng.signed()).normalize_or(Vec3::Y)
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rng.next_f32()
    }

    /// A direction within `spread` (0: exactly `dir`, 1: the hemisphere) of `dir`.
    fn around(&mut self, dir: Vec3, spread: f32) -> Vec3 {
        let r = self.unit();
        let d = (dir + r * spread * 1.4).normalize_or(dir);
        if d.dot(dir) < 0.0 { d - dir * 2.0 * d.dot(dir) } else { d }
    }

    #[allow(clippy::too_many_arguments)]
    fn shower(
        &mut self,
        cap: usize,
        n: u32,
        at: At,
        dir: Option<(Vec3, f32)>,
        speed: (f32, f32),
        life: (f32, f32),
        size: (f32, f32),
        streak: f32,
        ramp: Ramp,
    ) {
        for _ in 0..n {
            let d = match dir {
                Some((d, spread)) => self.around(d, spread),
                None => self.unit(),
            };
            let v = at.vel + d * self.range(speed.0, speed.1);
            let life = self.range(life.0, life.1);
            let pos = at.pos + d * self.range(0.0, size.0);
            self.spawn(cap, Particle { pos, vel: v, age: 0.0, life, size, streak, core: 0.6, ramp });
        }
    }

    /// A beam or round striking armour (or the hull): a flash, sparks off the surface, molten
    /// droplets and a little plasma of the beam's colour.
    pub fn impact(&mut self, cap: usize, at: At, normal: Vec3, color: Vec3, scale: f32) {
        let n = normal.normalize_or(Vec3::Y);
        let flash = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.12,
            size: (2.5 * scale, 5.0 * scale),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Flash,
        };
        self.spawn(cap, flash);
        self.shower(
            cap,
            (14.0 * scale) as u32,
            at,
            Some((n, 0.9)),
            (30.0, 140.0),
            (0.25, 0.7),
            (0.22, 0.12),
            0.03,
            Ramp::Spark,
        );
        self.shower(
            cap,
            4,
            at,
            Some((n, 0.5)),
            (4.0, 14.0),
            (0.2, 0.4),
            (1.5 * scale, 3.5 * scale),
            0.0,
            Ramp::Glow(color),
        );
        self.shower(cap, 5, at, Some((n, 0.8)), (5.0, 25.0), (0.8, 1.8), (0.35, 0.2), 0.0, Ramp::Ember);
    }

    /// A rock coming apart: a brief flash, a slow cloud of its dust, sparks, and glints of its ore.
    pub fn rock_burst(&mut self, cap: usize, at: At, radius: f32, ore: Vec3) {
        let r = radius.max(4.0);
        let flash = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.15,
            size: (r * 0.25, r * 0.7),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Flash,
        };
        self.spawn(cap, flash);
        self.shower(cap, 36, at, None, (2.0, 10.0), (3.0, 7.0), (r * 0.35, r * 1.1), 0.0, Ramp::Dust);
        self.shower(cap, 40, at, None, (20.0, 90.0), (0.4, 1.2), (0.3, 0.15), 0.04, Ramp::Spark);
        self.shower(cap, 24, at, None, (4.0, 18.0), (2.0, 5.0), (0.8, 0.4), 0.0, Ramp::Glow(ore * 3.0));
    }

    /// A saber blade in rock, for `dt` s: sparks and molten rock spraying off the cut, and glints
    /// of its ore.
    pub fn cutting(&mut self, cap: usize, at: At, normal: Vec3, ore: Vec3, dt: f32) {
        let n = normal.normalize_or(Vec3::Y);
        let sparks = self.count(600.0 * dt);
        self.shower(
            cap,
            sparks,
            at,
            Some((n, 0.8)),
            (20.0, 90.0),
            (0.3, 0.9),
            (0.25, 0.1),
            0.04,
            Ramp::Spark,
        );
        let melt = self.count(80.0 * dt);
        self.shower(cap, melt, at, Some((n, 0.6)), (3.0, 14.0), (0.6, 1.6), (0.4, 0.2), 0.0, Ramp::Ember);
        let dust = self.count(20.0 * dt);
        self.shower(cap, dust, at, Some((n, 0.7)), (2.0, 6.0), (1.5, 3.0), (1.0, 3.5), 0.0, Ramp::Dust);
        let glints = self.count(16.0 * dt);
        let glint = Ramp::Glow(ore * 2.5);
        self.shower(cap, glints, at, Some((n, 0.5)), (2.0, 8.0), (0.8, 1.8), (0.5, 0.25), 0.0, glint);
        self.glow(cap, at, 3.0, Vec3::new(6.0, 2.4, 0.7));
    }

    /// A whole number of particles, `expected` on average.
    fn count(&mut self, expected: f32) -> u32 {
        expected as u32 + u32::from(self.rng.next_f32() < expected.fract())
    }

    /// A shot leaving the muzzle.
    pub fn muzzle(&mut self, cap: usize, at: At, dir: Vec3, color: Vec3, scale: f32) {
        let flash = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.08,
            size: (2.0 * scale, 4.0 * scale),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Glow(color * 2.5),
        };
        self.spawn(cap, flash);
        self.shower(
            cap,
            5,
            at,
            Some((dir, 0.35)),
            (20.0, 70.0),
            (0.08, 0.16),
            (0.9 * scale, 0.3),
            0.0,
            Ramp::Glow(color),
        );
    }

    /// A suit's reactor going up, as it would in vacuum: a flash, a fireball that expands and cools
    /// in a sphere, sparks and embers flying straight out, and a puff of vapour.
    pub fn explosion(&mut self, cap: usize, at: At, scale: f32) {
        let flash = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.2,
            size: (25.0 * scale, 60.0 * scale),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Flash,
        };
        self.spawn(cap, flash);
        self.shower(
            cap,
            22,
            at,
            None,
            (10.0 * scale, 55.0 * scale),
            (0.8, 1.6),
            (6.0 * scale, 24.0 * scale),
            0.0,
            Ramp::Fire,
        );
        self.shower(cap, 50, at, None, (60.0, 260.0), (0.4, 1.1), (0.4, 0.2), 0.035, Ramp::Spark);
        self.shower(cap, 24, at, None, (5.0, 40.0), (1.5, 4.0), (0.5, 0.3), 0.0, Ramp::Ember);
        self.shower(
            cap,
            10,
            at,
            None,
            (8.0, 30.0),
            (1.5, 2.5),
            (8.0 * scale, 40.0 * scale),
            0.0,
            Ramp::Vapour,
        );
    }

    /// Beam sabers clashing: a pink-white flash and a spray of sparks.
    pub fn clash(&mut self, cap: usize, at: At) {
        let pink = Vec3::new(14.0, 3.0, 8.0);
        let flash = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.12,
            size: (6.0, 11.0),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Glow(pink * 2.0),
        };
        self.spawn(cap, flash);
        self.shower(cap, 24, at, None, (40.0, 160.0), (0.2, 0.5), (0.3, 0.15), 0.03, Ramp::Glow(pink));
    }

    /// Ionised gas glowing along a Twin Buster shot's path, lingering after the shot has passed.
    /// `length` is the streak drawn this frame, ending at `head`.
    pub fn trail(&mut self, cap: usize, head: Vec3, dir: Vec3, length: f32, color: Vec3, dt: f32) {
        // Each stretch of the path spends a few frames inside the streak, so spawning in
        // proportion to its length (per second) leaves an even trail at any frame rate.
        let expected = length / 30.0 * dt * 60.0;
        let n = expected as u32 + u32::from(self.rng.next_f32() < expected.fract());
        for _ in 0..n.min(64) {
            let pos = head - dir * self.range(0.0, length);
            let vel = self.unit() * self.range(1.0, 4.0);
            let life = self.range(0.8, 1.6);
            let p = Particle {
                pos,
                vel,
                age: 0.0,
                life,
                size: (1.5, 5.0),
                streak: 0.0,
                core: 0.3,
                ramp: Ramp::Glow(color * 0.2),
            };
            self.spawn(cap, p);
        }
    }

    /// An attitude jet firing along `dir`: quick puffs of cold vapour, `rate` a second.
    pub fn jet(&mut self, cap: usize, at: At, dir: Vec3, rate: f32, dt: f32) {
        let expected = rate * dt;
        let n = expected as u32 + u32::from(self.rng.next_f32() < expected.fract());
        for _ in 0..n {
            let d = self.around(dir, 0.15);
            let vel = at.vel + d * self.range(25.0, 45.0);
            let life = self.range(0.25, 0.45);
            let p = Particle {
                pos: at.pos,
                vel,
                age: 0.0,
                life,
                size: (0.3, 2.2),
                streak: 0.0,
                core: 0.0,
                ramp: Ramp::Vapour,
            };
            self.spawn(cap, p);
        }
    }

    /// A glow that lasts about a frame, renewed while it should show: a nozzle's fire seen end-on.
    pub fn glow(&mut self, cap: usize, at: At, size: f32, color: Vec3) {
        let p = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.035,
            size: (size, size),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Glow(color),
        };
        self.spawn(cap, p);
    }

    /// Energy drawn into a charging Twin Buster Rifle: sparks of it falling in toward the muzzle,
    /// where a glow gathers.
    pub fn charge(&mut self, cap: usize, at: At, dt: f32) {
        let color = Vec3::new(12.0, 7.0, 16.0);
        let expected = 110.0 * dt;
        let n = expected as u32 + u32::from(self.rng.next_f32() < expected.fract());
        for _ in 0..n {
            let d = self.unit();
            let r = self.range(6.0, 12.0);
            let life = self.range(0.4, 0.6);
            let p = Particle {
                pos: at.pos + d * r,
                vel: at.vel - d * (r / life),
                age: 0.0,
                life,
                size: (0.12, 0.25),
                streak: 0.06,
                core: 1.0,
                ramp: Ramp::Glow(color),
            };
            self.spawn(cap, p);
        }
        let flicker = 0.8 + 0.4 * self.rng.next_f32();
        let glow = Particle {
            pos: at.pos,
            vel: at.vel,
            age: 0.0,
            life: 0.05,
            size: (1.6 * flicker, 1.2 * flicker),
            streak: 0.0,
            core: 1.0,
            ramp: Ramp::Glow(color * 0.6),
        };
        self.spawn(cap, glow);
    }
}

impl Particles {
    /// A missile motor burning, for `dt` s: a white-hot glow at the nozzle, and smoke left behind
    /// in space along the path it flew this frame (it doesn't follow the missile). The streak of
    /// flame itself is a ribbon (`missiles_vis`).
    pub fn exhaust(&mut self, cap: usize, at: At, scale: f32, dt: f32) {
        self.glow(cap, at, 0.8 * scale, Vec3::new(16.0, 9.0, 3.0));
        let flown = at.vel * dt;
        let smoke = self.count(flown.length() / 4.0);
        for _ in 0..smoke.min(10) {
            let pos = at.pos - flown * self.range(0.0, 1.0);
            let vel = self.unit() * self.range(0.5, 2.0);
            let life = self.range(1.0, 1.8);
            let size = (0.8 * scale, 3.0 * scale);
            self.spawn(
                cap,
                Particle { pos, vel, age: 0.0, life, size, streak: 0.0, core: 0.0, ramp: Ramp::Vapour },
            );
        }
    }

    /// A flamethrower's jet along `dir`, for `dt` s: burning propellant billowing out to `range`
    /// m inside a cone of `half_angle`, cooling from white-yellow through orange to dark.
    pub fn flame(&mut self, cap: usize, at: At, dir: Vec3, range: f32, half_angle: f32, dt: f32) {
        let n = self.count(320.0 * dt);
        // `around` spreads over about 1.4 × `spread` radians.
        let spread = half_angle / 1.4;
        for _ in 0..n {
            let d = self.around(dir, spread);
            let life = self.range(0.3, 0.5);
            let speed = range / 0.45 * self.range(0.75, 1.05);
            let p = Particle {
                pos: at.pos + d * self.range(0.0, 2.0),
                vel: at.vel + d * speed,
                age: 0.0,
                life,
                size: (0.6, range * self.range(0.06, 0.12)),
                streak: 0.0,
                core: 0.5,
                ramp: Ramp::Fire,
            };
            self.spawn(cap, p);
        }
        let embers = self.count(40.0 * dt);
        self.shower(
            cap,
            embers,
            at,
            Some((dir, spread)),
            (range * 1.5, range * 2.5),
            (0.3, 0.6),
            (0.2, 0.1),
            0.03,
            Ramp::Spark,
        );
        self.glow(cap, at, 1.8, Vec3::new(14.0, 7.0, 1.6));
    }
}

/// Spawns the particle mesh (empty to start) and the pool.
pub fn setup_particles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ParticleMaterial>>,
) {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    write_mesh(&mut mesh, &[], Vec3::ZERO);
    let mesh = meshes.add(mesh);
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(materials.add(ParticleMaterial {})),
        Transform::IDENTITY,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.insert_resource(Particles { live: Vec::new(), rng: Rng::new(0x9A27_1C1E), mesh });
}

/// One quad per particle (at least one, transparent, so the buffers are never empty), far ones
/// first.
fn write_mesh(mesh: &mut Mesh, live: &[Particle], eye: Vec3) {
    let n = live.len().max(1);
    let mut order: Vec<usize> = (0..live.len()).collect();
    order.sort_by(|a, b| live[*b].pos.distance_squared(eye).total_cmp(&live[*a].pos.distance_squared(eye)));
    let mut pos = Vec::with_capacity(n * 4);
    let mut corner = Vec::with_capacity(n * 4);
    let mut color = Vec::with_capacity(n * 4);
    let mut shape = Vec::with_capacity(n * 4);
    let mut dir = Vec::with_capacity(n * 4);
    for &i in &order {
        let p = &live[i];
        let t = (p.age / p.life).clamp(0.0, 1.0);
        let (c, a) = p.ramp.at(t);
        let size = p.size.0 + (p.size.1 - p.size.0) * t;
        let speed = p.vel.length();
        let streak = p.streak * speed;
        let d = if streak > 0.0 { p.vel / speed } else { Vec3::ZERO };
        let s = [size, streak, if p.ramp.puff() { 1.0 } else { 0.0 }, p.core];
        for k in CORNERS {
            pos.push(p.pos.to_array());
            corner.push(k);
            color.push([c.x, c.y, c.z, a]);
            shape.push(s);
            dir.push(d.to_array());
        }
    }
    if live.is_empty() {
        for k in CORNERS {
            pos.push([0.0; 3]);
            corner.push(k);
            color.push([0.0; 4]);
            shape.push([0.0; 4]);
            dir.push([0.0; 3]);
        }
    }
    let mut idx = Vec::with_capacity(n * 6);
    for q in 0..n as u32 {
        let b = q * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(pos));
    mesh.insert_attribute(ATTRIBUTE_CORNER, VertexAttributeValues::Float32x2(corner));
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(color));
    mesh.insert_attribute(ATTRIBUTE_SHAPE, VertexAttributeValues::Float32x4(shape));
    mesh.insert_attribute(ATTRIBUTE_DIR, VertexAttributeValues::Float32x3(dir));
    mesh.insert_indices(Indices::U32(idx));
}

/// Ages and moves every particle, then rewrites the mesh.
pub fn update_particles(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut particles: ResMut<Particles>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.dt;
    let cap = gfx.settings.particles;
    let p = &mut *particles;
    for q in &mut p.live {
        q.age += dt;
        q.pos += q.vel * dt;
    }
    p.live.retain(|q| q.age < q.life);
    p.live.truncate(cap);
    let eye = cams.single().map_or(Vec3::ZERO, |c| c.translation());
    if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
        write_mesh(&mut mesh, &p.live, eye);
    }
}
