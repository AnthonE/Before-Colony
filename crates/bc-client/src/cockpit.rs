//! The cockpit, from the pilot's seat (the first-person view): `bc_model::cockpit`'s frame of
//! monitors hung on the camera, round the panoramic view of space.
//!
//! - It's on its own render layer ([`LAYER`]) with its own light, the monitors' glow: the Sun and
//!   the battle's lights are outside, and don't reach in.
//! - Its monitors show the instruments (`hud`), which the UI draws into one texture while the
//!   cockpit is in view (by a camera of its own, [`Cockpit::screen_camera`]); each monitor shows its
//!   region of it.
//! - The radar sphere at the bottom holds its bearings in space as the suit turns, with a blip for
//!   every suit round about and every missile.
//! - The frame lags a touch behind the view as the suit accelerates, so G is felt as well as read;
//!   a hit flickers the monitors (with flashing effects on), and with the head's main camera gone
//!   they go grey with static.

use bc_model::cockpit::{self, TEXTURE};
use bc_proto::snapshot::ent_flags;
use bevy::asset::embedded_asset;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{MeshTag, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::IsDefaultUiCamera;

use crate::blast::ShockMaterial;
use crate::camera::{Chase, MainCamera};
use crate::materials::{HullTag, Surfaces, paint};
use crate::suits_vis::{SuitVisual, livery};
use crate::view::{MissileFeed, SuitDrive, ViewPrefs, VisTime};

/// The render layer the cockpit is on (the main camera sees it and the world's layer 0).
pub const LAYER: usize = 1;
/// The layer the instruments' camera draws (nothing in the world is on it).
pub const SCREEN_LAYER: usize = 2;
/// How far out the radar reaches (m), and how many blips it has.
const RADAR_RANGE: f32 = 3_000.0;
const BLIPS: usize = 32;

pub struct CockpitPlugin;

impl Plugin for CockpitPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/monitor.wgsl");
        app.add_plugins(MaterialPlugin::<MonitorMaterial>::default());
    }
}

/// A monitor's face: its region of the instruments' texture.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct MonitorMaterial {
    #[texture(0)]
    #[sampler(1)]
    screen: Handle<Image>,
    /// x: seconds; y: static; z: flicker; w: brightness.
    #[uniform(2)]
    params: Vec4,
}

impl Material for MonitorMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/monitor.wgsl".into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// The cockpit and the instruments' texture.
#[derive(Resource)]
pub struct Cockpit {
    root: Entity,
    shell: Entity,
    /// The camera that draws the instruments into [`Cockpit::screen`].
    pub screen_camera: Entity,
    material: Handle<MonitorMaterial>,
    radar: Entity,
    blips: Vec<Entity>,
    blip_looks: [Handle<StandardMaterial>; 4],
    /// The frame's lag behind the view (m, camera space), and its rate.
    lag: Vec3,
    lag_vel: Vec3,
    last_vel: Option<Vec3>,
}

impl Cockpit {
    /// The cockpit is on screen (so the instruments are on its monitors).
    pub fn shown(chase: &Chase) -> bool {
        chase.cockpit()
    }
}

#[derive(Component)]
pub struct Blip;

fn layer() -> RenderLayers {
    RenderLayers::layer(LAYER)
}

/// A glowing, unlit look (radar rings and blips). Unlit, what's drawn is the base colour.
fn glowing(materials: &mut Assets<StandardMaterial>, rgb: Vec3, add: bool) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(rgb.x, rgb.y, rgb.z)),
        unlit: true,
        alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Opaque },
        ..default()
    })
}

/// Builds the cockpit on the main camera (hidden until the view goes into the cockpit) and the
/// instruments' texture and camera.
#[allow(clippy::too_many_arguments)]
pub fn setup_cockpit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut monitors: ResMut<Assets<MonitorMaterial>>,
    mut shocks: ResMut<Assets<ShockMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    surfaces: Res<Surfaces>,
    cams: Query<Entity, With<MainCamera>>,
) {
    let Ok(cam) = cams.single() else { return };
    let design = cockpit::build();
    // The instruments' texture, and the camera that draws the UI into it (only while it's seen).
    let screen =
        images.add(Image::new_target_texture(TEXTURE[0], TEXTURE[1], TextureFormat::Rgba8UnormSrgb, None));
    let screen_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -1,
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::srgb(0.012, 0.022, 0.034)),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: TEXTURE[0] as f32, height: TEXTURE[1] as f32 },
                ..OrthographicProjection::default_2d()
            }),
            RenderTarget::Image(screen.clone().into()),
            RenderLayers::layer(SCREEN_LAYER),
        ))
        .id();
    // The main camera sees the world and the cockpit, and draws the HUD.
    commands.entity(cam).insert((RenderLayers::from_layers(&[0, LAYER]), IsDefaultUiCamera));

    let root =
        commands.spawn((Name::new("cockpit"), Transform::default(), Visibility::Hidden, ChildOf(cam))).id();
    let shell = commands
        .spawn((
            Mesh3d(meshes.add(crate::model::mesh(design.shell))),
            MeshMaterial3d(surfaces.armour.clone()),
            HullTag::livery(paint::WHITE, paint::BLUE, paint::RED, 0, 91).tag(),
            layer(),
            NotShadowCaster,
            NotShadowReceiver,
            ChildOf(root),
        ))
        .id();
    // The monitors' faces, each mapped onto its region of the texture.
    let material =
        monitors.add(MonitorMaterial { screen: screen.clone(), params: Vec4::new(0.0, 0.0, 0.0, 1.0) });
    for s in &design.screens {
        let mut quad = Mesh::from(Rectangle::new(s.size.x, s.size.y));
        // The face's own coordinates too (its rim).
        if let Some(own) = quad.attribute(Mesh::ATTRIBUTE_UV_0).cloned() {
            quad.insert_attribute(Mesh::ATTRIBUTE_UV_1, own);
        }
        if let Some(VertexAttributeValues::Float32x2(uvs)) = quad.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
            for uv in uvs.iter_mut() {
                uv[0] = s.region[0] + uv[0] * (s.region[2] - s.region[0]);
                uv[1] = s.region[1] + uv[1] * (s.region[3] - s.region[1]);
            }
        }
        commands.spawn((
            Mesh3d(meshes.add(quad)),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(s.centre).with_rotation(s.rotation),
            layer(),
            NotShadowCaster,
            NotShadowReceiver,
            ChildOf(root),
        ));
    }
    // The monitors' glow, all the light there is in here. Strong enough to read against the
    // exposure set for sunlight outside.
    commands.spawn((
        PointLight {
            intensity: 3.0e5,
            range: 5.0,
            color: Color::srgb(0.7, 0.88, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(design.light),
        layer(),
        ChildOf(root),
    ));
    // The radar: a glass sphere of light, its rings, and blips.
    let r = design.radar.radius;
    let radar = commands
        .spawn((Transform::from_translation(design.radar.centre), Visibility::Inherited, ChildOf(root)))
        .id();
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(r).mesh().ico(3).expect("ico sphere"))),
        MeshMaterial3d(shocks.add(ShockMaterial::new(Vec3::new(0.08, 0.4, 0.55), 2.6))),
        MeshTag(255),
        layer(),
        NotShadowCaster,
        NotShadowReceiver,
        ChildOf(radar),
    ));
    // Dark glass behind it, so it reads over a sunlit hull.
    let back = design.radar.centre.normalize();
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(r * 1.08))),
        MeshMaterial3d(standard.add(StandardMaterial {
            base_color: Color::srgb(0.008, 0.016, 0.024),
            unlit: true,
            ..default()
        })),
        Transform::from_translation(design.radar.centre + back * r * 1.02)
            .with_rotation(Quat::from_rotation_arc(Vec3::Z, -back)),
        layer(),
        NotShadowCaster,
        NotShadowReceiver,
        ChildOf(root),
    ));
    let ring = glowing(&mut standard, Vec3::new(0.12, 0.7, 0.8), true);
    let torus = meshes.add(Torus::new(r * 0.985, r));
    let thin = meshes.add(Torus::new(r * 0.99, r * 1.002));
    for (mesh, rot) in [
        (torus.clone(), Quat::IDENTITY),
        (thin.clone(), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        (thin.clone(), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        (thin, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_z(0.8)),
    ] {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(ring.clone()),
            Transform::from_rotation(rot),
            layer(),
            NotShadowCaster,
            NotShadowReceiver,
            ChildOf(radar),
        ));
    }
    // Blips: hostile, friendly, a missile, and the pilot's own suit at the middle.
    let blip_looks = [
        glowing(&mut standard, Vec3::new(4.0, 0.5, 0.45), false),
        glowing(&mut standard, Vec3::new(0.6, 3.5, 1.1), false),
        glowing(&mut standard, Vec3::new(4.0, 2.2, 0.3), false),
        glowing(&mut standard, Vec3::new(2.5, 3.0, 3.2), false),
    ];
    let dot = meshes.add(Sphere::new(r * 0.045).mesh().ico(1).expect("ico sphere"));
    let blips = (0..BLIPS)
        .map(|_| {
            commands
                .spawn((
                    Blip,
                    Mesh3d(dot.clone()),
                    MeshMaterial3d(blip_looks[0].clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    layer(),
                    NotShadowCaster,
                    NotShadowReceiver,
                    ChildOf(radar),
                ))
                .id()
        })
        .collect();
    commands.insert_resource(Cockpit {
        root,
        shell,
        screen_camera,
        material,
        radar,
        blips,
        blip_looks,
        lag: Vec3::ZERO,
        lag_vel: Vec3::ZERO,
        last_vel: None,
    });
}

/// Where a contact goes in the radar sphere (radar space, world-aligned): its bearing, at a
/// distance growing with the log of its range, so close ones spread out and far ones still show.
pub fn radar_point(rel: Vec3, radius: f32) -> Option<Vec3> {
    let d = rel.length();
    if !(1.0..=RADAR_RANGE).contains(&d) {
        return None;
    }
    let k = (1.0 + d / 150.0).ln() / (1.0 + RADAR_RANGE / 150.0).ln();
    Some(rel / d * radius * 0.92 * k)
}

/// Shows the cockpit while the view is in it, and runs it: its livery, its lag under G, the
/// monitors' state, and the radar.
#[allow(clippy::too_many_arguments)]
pub fn drive_cockpit(
    time: Res<VisTime>,
    chase: Res<Chase>,
    prefs: Res<ViewPrefs>,
    feed: Res<MissileFeed>,
    mut cockpit: Option<ResMut<Cockpit>>,
    suits: Query<(&SuitDrive, Option<&SuitVisual>)>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut transforms: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut tags: Query<&mut MeshTag>,
    mut cameras: Query<&mut Camera>,
    mut monitors: ResMut<Assets<MonitorMaterial>>,
    mut blip_looks: Query<&mut MeshMaterial3d<StandardMaterial>, With<Blip>>,
) {
    let Some(c) = cockpit.as_deref_mut() else { return };
    let on = Cockpit::shown(&chase);
    if let Ok(mut v) = vis.get_mut(c.root) {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Ok(mut cam) = cameras.get_mut(c.screen_camera)
        && cam.is_active != on
    {
        cam.is_active = on;
    }
    if !on {
        c.last_vel = None;
        return;
    }
    let dt = time.dt.min(0.1);
    let Some((own, visual)) = suits.iter().find(|(d, _)| d.own) else { return };
    // The livery of the suit the pilot sits in.
    let (body, trim, accent, _) = livery(own.frame, own.faction);
    let want = visual
        .map_or_else(|| HullTag::livery(body, trim, accent, 0, 91), |v| HullTag { seed: 91, ..v.tag() });
    if let Ok(mut t) = tags.get_mut(c.shell)
        && *t != want.tag()
    {
        *t = want.tag();
    }
    let Ok(cam) = cams.single() else { return };
    let cam_rot = cam.compute_transform().rotation;
    // The frame sways against the suit's acceleration (a critically damped spring), a few
    // centimetres at most.
    let acc = match c.last_vel {
        Some(v) if dt > 0.0 => (own.vel - v) / dt,
        _ => Vec3::ZERO,
    };
    c.last_vel = Some(own.vel);
    let target = (cam_rot.inverse() * acc * -0.0012).clamp_length_max(0.05) * prefs.shake.clamp(0.0, 1.0);
    let w = 9.0;
    let spring = (target - c.lag) * w * w - c.lag_vel * 2.0 * w;
    c.lag_vel += spring * dt;
    c.lag += c.lag_vel * dt;
    if let Ok(mut tf) = transforms.get_mut(c.root) {
        tf.translation = c.lag;
    }
    // The monitors: static without the head's main camera, a flicker when hit.
    if let Some(mut m) = monitors.get_mut(&c.material) {
        let flicker = if prefs.flashing { chase.hit_flash() } else { 0.0 };
        m.params =
            Vec4::new((time.now % 1_000.0) as f32, if chase.sub_camera() { 0.85 } else { 0.0 }, flicker, 1.0);
    }
    // The radar holds its bearings in space: undo the view's turn.
    if let Ok(mut tf) = transforms.get_mut(c.radar) {
        tf.rotation = cam_rot.inverse();
    }
    let radius = 0.17;
    let mut shown = 0;
    let mut place = |at: Vec3, look: usize, shown: &mut usize| {
        let Some(&e) = c.blips.get(*shown) else { return };
        *shown += 1;
        if let Ok(mut tf) = transforms.get_mut(e) {
            tf.translation = at;
        }
        if let Ok(mut m) = blip_looks.get_mut(e)
            && m.0 != c.blip_looks[look]
        {
            m.0 = c.blip_looks[look].clone();
        }
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(Visibility::Inherited);
        }
    };
    place(Vec3::ZERO, 3, &mut shown);
    for (d, _) in &suits {
        if d.own || d.flags & ent_flags::WRECK != 0 {
            continue;
        }
        if let Some(at) = radar_point(d.pos - own.pos, radius) {
            place(at, if d.faction == own.faction { 1 } else { 0 }, &mut shown);
        }
    }
    for m in &feed.0 {
        if let Some(at) = radar_point(m.pos - own.pos, radius) {
            place(at, 2, &mut shown);
        }
    }
    for &e in &c.blips[shown.min(c.blips.len())..] {
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(Visibility::Hidden);
        }
    }
}
