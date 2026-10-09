//! The chart (M): a holographic map of everything a pilot can find, from a rock in the field out
//! to the Earth Sphere, that they can turn about, zoom through, pick from and set a course on.
//!
//! - **Seen in 3D.** The sector is drawn as light on its own layer (`holo`): the colony turning
//!   with its windows lit, the docking hub and the dock's ring, MO-II rolling on its circle,
//!   Hermit and its craters, the field's rocks, every suit in sight with where it's heading, and
//!   missiles. A plane through the pilot (or the sector's middle, `P`) is ruled in a grid with
//!   range rings round the pilot, and a stalk drops from everything to it, so "above" and "below"
//!   read at a glance.
//! - **Out to the Earth Sphere.** Zooming out past the sector carries on, without a cut, to Earth,
//!   the Moon and the five Lagrange points at their true distances (`bc_client_core::sphere`):
//!   the sector is a light at L1, between them. The rest can be found and read about, but no lane
//!   leaves L1 yet.
//! - **Picking and courses.** Hover anything for its name and range; click it for what it is,
//!   how far, how long to get there and (by the real flight rules) what it would burn. Enter (or
//!   a right click, or SET COURSE) sets a course to it: the shortest way round the colony and the
//!   landmarks (`bc_client_core::nav`), drawn here and laid out in space ahead of the suit when the
//!   chart closes, with a ◇ on the HUD. N engages the auto-nav, which flies it and stops at rest
//!   just off what it was sent to. Double-click empty space to mark a point on the plane.
//! - **Hands off.** While the chart is open the cursor is free and the keys are the chart's, so
//!   the suit's stick is let go: flight assist holds it still, unless the auto-nav is flying. The
//!   sector doesn't pause. Esc or M closes it.
//!
//! | Input | In the chart |
//! |---|---|
//! | drag | turn the view about its focus |
//! | right or middle drag, Shift+drag | pan |
//! | wheel | zoom (in toward the cursor; out, past the sector, to the Earth Sphere) |
//! | click · double-click | select · focus on it (or, on empty space, mark a nav point) |
//! | right click | set a course to it |
//! | Enter · N · Backspace | set a course to the selection · auto-nav · clear the course |
//! | W/A/S/D, R/F, Q/E | pan, rise and sink, turn |
//! | [ · ] | the previous or next place the chart lists (and the view flies to it) |
//! | H · 1 · 2 · 3 · T · P | back to your suit · the views: you, the sector, the Earth Sphere · top-down · the plane |

use std::f32::consts::{FRAC_PI_2, TAU};

use bc_client_core::chart::{
    self as view, ChartCam, Label, Pickable, Pose, clock, declutter, nice_step, pick, range,
};
use bc_client_core::nav::{self, Arrival, AutoNav, Course, NavState, Place};
use bc_client_core::palette;
use bc_client_core::sphere::{self, Lagrange};
use bc_proto::PilotKind;
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_sim::bodies::Body;
use bc_sim::colony::frame::FIRST_WINDOW;
use bc_sim::config::SECTOR_LIMIT;
use bc_sim::content::frame;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_HUB_LENGTH, DOCK_RADIUS};
use bc_sim::field::FIELD_CENTER;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS, colony_spin_angle};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::dots::{Blink, DotLook, DotMaterial, Dots};
use crate::gfx::Gfx;
use crate::holo::{ChartSkyMaterial, ChartSkyUniform, HoloKind, HoloMaterial, tube};
use crate::hud::{SHADOW, UiFont};
use crate::map::{MapOpen, ObjectiveState};
use crate::net::{GameClient, now_s};
use crate::page::Ui;
use crate::settings::SettingsRes;
use crate::ui_panel::PanelMaterial;
use crate::view::VisTime;

/// Engages the auto-nav (on the chart, or in flight on the course set).
pub const NAV_KEY: KeyCode = KeyCode::KeyN;
/// The render layer the chart is drawn on (nothing else is on it; only its camera sees it).
pub const CHART_LAYER: usize = 4;
/// The panels' widths, left and right, px.
const LEFT_W: f32 = 280.0;
const RIGHT_W: f32 = 310.0;
/// Labels the chart can show at once.
const LABELS: usize = 40;
/// Suits in sight it marks, at most (the nearest).
const SUITS: usize = 48;
/// A click is a drag once the cursor has moved this far, px; two within this long are a double.
const DRAG_PX: f32 = 4.0;
const DOUBLE_S: f64 = 0.35;
/// How often the set course is plotted again for the chart and the HUD, s.
const REPLOT_S: f64 = 0.5;
/// The pilot's suit's velocity is drawn this far ahead, s; everyone else's this far.
const OWN_AHEAD_S: f32 = 30.0;
const SUIT_AHEAD_S: f32 = 10.0;
/// The scales the chart's two worlds fade between (m of view distance): the sector's lines out
/// between the first two, the Earth Sphere's in between the last two.
const SECTOR_FADE: (f32, f32) = (600_000.0, 6_000_000.0);
const SPHERE_FADE: (f32, f32) = (1_500_000.0, 30_000_000.0);

const fn colour(hex: palette::Hex) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgb(r, g, b)
}

const CYAN: Color = colour(palette::CYAN);
const AMBER: Color = colour(palette::AMBER);
const RED: Color = colour(palette::RED);
const GREEN: Color = colour(palette::GREEN);
const WHITE: Color = colour(palette::WHITE);
const LABEL: Color = colour(palette::LABEL);
const GREY: Color = Color::srgb(0.58, 0.63, 0.7);
/// The objectives' yellow (as the HUD's ◆).
const OBJECTIVE: Color = crate::map::OBJECTIVE;
/// The course's teal.
pub const COURSE: Color = Color::srgb(0.36, 1.0, 0.86);
const PLATE: Color = Color::srgba(0.02, 0.05, 0.1, 0.82);
const PLATE_EDGE: Color = Color::srgba(0.45, 0.85, 1.0, 0.5);
const CHIP: Color = Color::srgba(0.25, 0.6, 0.85, 0.16);
const CHIP_ON: Color = Color::srgba(0.36, 1.0, 0.86, 0.32);
const CHIP_HOVER: Color = Color::srgba(0.45, 0.85, 1.0, 0.3);

/// The chart's lines in raw HDR (they bloom where the tier has bloom).
fn glow(c: Color, k: f32, a: f32) -> Color {
    let l = c.to_linear();
    Color::LinearRgba(LinearRgba::new(l.red * k, l.green * k, l.blue * k, a))
}

/// The chart's fine lines: the grid, outlines, stalks.
#[derive(Default, Reflect)]
pub struct ChartLines;
impl GizmoConfigGroup for ChartLines {}
/// Its bold ones: the course, the pilot's suit, the selection.
#[derive(Default, Reflect)]
pub struct ChartBold;
impl GizmoConfigGroup for ChartBold {}
/// Its dashed ones: what's ahead, orbits, the sector's limit, the lanes not yet open.
#[derive(Default, Reflect)]
pub struct ChartDashes;
impl GizmoConfigGroup for ChartDashes {}
/// The course laid out in space ahead of the suit, in the world (the main camera's layer).
#[derive(Default, Reflect)]
pub struct CourseLines;
impl GizmoConfigGroup for CourseLines {}

/// The chart's state while it's open, and what it keeps between openings.
#[derive(Resource)]
pub struct Chart {
    pub cam: ChartCam,
    pub hover: Option<Place>,
    pub selected: Option<Place>,
    /// The plane is through the pilot's suit (else the sector's middle).
    pub plane_on_pilot: bool,
    drag: Option<Drag>,
    last_click: Option<(f64, Vec2)>,
    /// The view as drawn last frame.
    view: Projector,
    /// What can be picked, and where, as drawn last frame.
    picks: Vec<(Place, Pickable)>,
    /// The labels wanted this frame.
    labels: Vec<Want>,
    /// The cursor over the chart (not a panel), logical px.
    cursor: Option<Vec2>,
    was_open: bool,
}

impl Default for Chart {
    fn default() -> Self {
        Self {
            cam: ChartCam::default(),
            hover: None,
            selected: None,
            plane_on_pilot: true,
            drag: None,
            last_click: None,
            view: Projector::default(),
            picks: Vec::new(),
            labels: Vec::new(),
            cursor: None,
            was_open: false,
        }
    }
}

/// A drag under way.
#[derive(Clone, Copy, Debug)]
struct Drag {
    button: MouseButton,
    start: Vec2,
    last: Vec2,
    moved: bool,
}

/// The course the pilot has set (it outlives the chart: the HUD and the world show it).
#[derive(Resource, Default)]
pub struct NavTarget {
    pub place: Option<Place>,
    pub course: Course,
    pub arrival: Option<Arrival>,
    plotted: f64,
}

/// A label the chart would show this frame.
#[derive(Clone, Debug)]
struct Want {
    at: Vec2,
    text: String,
    color: Color,
    rank: u16,
}

/// The view as the chart's camera draws it: where it is, which way, how wide, and the screen.
#[derive(Clone, Copy, Debug, Default)]
struct Projector {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    fwd: Vec3,
    tan: f32,
    size: Vec2,
}

impl Projector {
    fn new(pose: &Pose, size: Vec2) -> Self {
        let fwd = -pose.back();
        let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(fwd);
        Self { eye: pose.eye(), right, up, fwd, tan: (view::FOV * 0.5).tan(), size: size.max(Vec2::ONE) }
    }

    /// Where `p` is on the screen (logical px from the top left), if it's in front.
    fn project(&self, p: Vec3) -> Option<Vec2> {
        let r = p - self.eye;
        let z = r.dot(self.fwd);
        if z <= r.length() * 1e-4 + 1e-3 {
            return None;
        }
        let aspect = self.size.x / self.size.y;
        let x = r.dot(self.right) / (z * self.tan * aspect);
        let y = r.dot(self.up) / (z * self.tan);
        Some(Vec2::new((x + 1.0) * 0.5 * self.size.x, (1.0 - y) * 0.5 * self.size.y))
    }

    /// Whether a point on the screen is on it.
    fn on_screen(&self, s: Vec2, margin: f32) -> bool {
        s.x >= -margin && s.y >= -margin && s.x <= self.size.x + margin && s.y <= self.size.y + margin
    }

    /// The way from the eye through a point on the screen.
    fn ray(&self, s: Vec2) -> Vec3 {
        let aspect = self.size.x / self.size.y;
        let x = (s.x / self.size.x * 2.0 - 1.0) * self.tan * aspect;
        let y = (1.0 - s.y / self.size.y * 2.0) * self.tan;
        (self.fwd + self.right * x + self.up * y).normalize()
    }

    /// Metres a pixel covers at `p`.
    fn per_px(&self, p: Vec3) -> f32 {
        2.0 * (p - self.eye).dot(self.fwd).max(1e-3) * self.tan / self.size.y
    }
}

#[derive(Component)]
pub struct ChartCamera;
#[derive(Component)]
pub struct ChartUi;
/// A label from the pool.
#[derive(Component)]
pub struct ChartLabel;
/// A panel (the cursor over it isn't over the chart).
#[derive(Component)]
pub struct ChartPanel;
/// The selection card (hidden with nothing selected).
#[derive(Component)]
pub struct ChartCard;
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChartText {
    Scale,
    Status,
    Objectives,
    Name,
    Kind,
    About,
    Facts,
    Hover,
    Help,
}
/// Something to press.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub enum ChartButton {
    SetCourse,
    AutoNav,
    Clear,
    Focus,
    You,
    Sector,
    Sphere,
    TopDown,
    Plane,
    Go(Place),
}
#[derive(Component)]
pub struct ButtonText;

/// The chart's 3D scene: its camera and the holograms it moves each frame.
#[derive(Resource)]
pub struct ChartScene {
    camera: Entity,
    colony: Entity,
    landmarks: Vec<Entity>,
    rocks: Option<Entity>,
    /// What the rocks were drawn from: the field's seed and size, and which were shattered.
    rocks_drawn: Option<(u32, u16, u64)>,
    sky: Handle<ChartSkyMaterial>,
    /// The holograms' materials, with how bright each is at full (their clocks and fades are set
    /// each frame).
    holos: Vec<(Handle<HoloMaterial>, bool)>,
}

pub struct ChartPlugin;

impl Plugin for ChartPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::holo::HoloPlugin)
            .init_gizmo_group::<ChartLines>()
            .init_gizmo_group::<ChartBold>()
            .init_gizmo_group::<ChartDashes>()
            .init_gizmo_group::<CourseLines>()
            .init_resource::<Chart>()
            .init_resource::<NavTarget>()
            .add_systems(
                Startup,
                (setup_chart_gizmos, setup_chart_scene, setup_chart_ui, setup_nav_marker).chain(),
            )
            .add_systems(Update, apply_chart_tier);
    }
}

fn setup_chart_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let layer = RenderLayers::layer(CHART_LAYER);
    let (c, _) = store.config_mut::<ChartLines>();
    c.render_layers = layer.clone();
    c.line.width = 1.4;
    let (c, _) = store.config_mut::<ChartBold>();
    c.render_layers = layer.clone();
    c.line.width = 3.0;
    c.line.joints = GizmoLineJoint::Round(4);
    let (c, _) = store.config_mut::<ChartDashes>();
    c.render_layers = layer;
    c.line.width = 1.6;
    c.line.style = GizmoLineStyle::Dashed { gap_scale: 4.0, line_scale: 6.0 };
    // The course in the world: in front of everything near it, so it reads against a rock.
    let (c, _) = store.config_mut::<CourseLines>();
    c.line.width = 2.6;
    c.line.joints = GizmoLineJoint::Round(4);
    c.depth_bias = -0.2;
}

/// The chart's camera (off until the chart opens) and its holograms.
#[allow(clippy::too_many_arguments)]
fn setup_chart_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut holos: ResMut<Assets<HoloMaterial>>,
    mut skies: ResMut<Assets<ChartSkyMaterial>>,
) {
    let layer = RenderLayers::layer(CHART_LAYER);
    let camera = commands
        .spawn((
            ChartCamera,
            Camera3d::default(),
            Camera {
                order: 1,
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::srgb(0.004, 0.009, 0.02)),
                ..default()
            },
            Projection::Perspective(PerspectiveProjection {
                fov: view::FOV,
                near: 1.0,
                far: 1.0e11,
                ..default()
            }),
            layer.clone(),
            Transform::default(),
        ))
        .id();

    let sun = sphere::SUN_DIR;
    let mut made = Vec::new();
    let mut holo = |m: HoloMaterial, bright: bool| {
        let h = holos.add(m);
        made.push((h.clone(), bright));
        h
    };
    let cyan = Vec3::new(0.12, 0.42, 0.62);
    let rim = Vec3::new(0.35, 0.95, 1.4);
    let hidden = (NotShadowCaster, NotShadowReceiver, layer.clone());
    // The colony: a tube along its axis, with its windows; caps at its ends; the docking hub.
    let mut colony_look = HoloMaterial::new(HoloKind::Colony, cyan, 0.3, rim, 500.0, sun);
    colony_look.holo.sun.w = FIRST_WINDOW;
    let colony = commands
        .spawn((
            Mesh3d(meshes.add(tube(COLONY_RADIUS, COLONY_HALF_LENGTH, 96, 32))),
            MeshMaterial3d(holo(colony_look, true)),
            Transform::from_translation(COLONY_CENTER),
            hidden.clone(),
        ))
        .id();
    let cap_look = holo(HoloMaterial::new(HoloKind::Plain, cyan * 0.6, 0.35, rim * 0.8, 400.0, sun), true);
    for side in [-1.0f32, 1.0] {
        commands.spawn((
            Mesh3d(meshes.add(Circle::new(COLONY_RADIUS).mesh().resolution(96))),
            MeshMaterial3d(cap_look.clone()),
            Transform::from_translation(COLONY_CENTER + Vec3::X * side * COLONY_HALF_LENGTH)
                .with_rotation(Quat::from_rotation_y(side * FRAC_PI_2)),
            hidden.clone(),
        ));
    }
    let hub_len = DOCK_CENTER.x.abs() - (COLONY_HALF_LENGTH - COLONY_CENTER.x) - DOCK_RADIUS * 0.5;
    let hub_len = hub_len.max(DOCK_HUB_LENGTH);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(160.0, hub_len).mesh().resolution(32))),
        MeshMaterial3d(holo(HoloMaterial::new(HoloKind::Plain, cyan, 0.5, rim, 60.0, sun), true)),
        Transform::from_translation(COLONY_CENTER - Vec3::X * (COLONY_HALF_LENGTH + hub_len * 0.5))
            .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        hidden.clone(),
    ));
    // The landmarks, from the very shapes suits stand on.
    let landmark_look =
        holo(HoloMaterial::new(HoloKind::Plain, Vec3::new(0.1, 0.36, 0.5), 0.6, rim, 25.0, sun), true);
    let landmarks = LANDMARKS
        .iter()
        .map(|def| {
            let mut data = bc_client_core::body_mesh::MeshData::default();
            for part in crate::landmarks::far_meshes(&def.shape) {
                data.append(&part);
            }
            commands
                .spawn((
                    Mesh3d(meshes.add(crate::landmarks::to_mesh(&data))),
                    MeshMaterial3d(landmark_look.clone()),
                    Transform::from_translation(def.center),
                    hidden.clone(),
                ))
                .id()
        })
        .collect();
    // Earth and the Moon, where they are.
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(sphere::EARTH_RADIUS).mesh().uv(96, 48))),
        MeshMaterial3d(holo(
            HoloMaterial::new(HoloKind::Earth, Vec3::splat(1.0), 0.6, Vec3::new(0.3, 0.7, 1.3), 1.0, sun),
            false,
        )),
        Transform::from_translation(sphere::earth()),
        hidden.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(sphere::MOON_RADIUS).mesh().uv(64, 32))),
        MeshMaterial3d(holo(
            HoloMaterial::new(HoloKind::Moon, Vec3::splat(1.0), 0.6, Vec3::new(0.5, 0.7, 1.0), 1.0, sun),
            false,
        )),
        Transform::from_translation(sphere::moon()),
        hidden.clone(),
    ));
    // Deep space behind it all.
    let sky = skies.add(ChartSkyMaterial {
        sky: ChartSkyUniform { sun: sun.extend(0.0), galaxy: sphere::GALAXY_NORMAL.extend(1.0) },
    });
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(2).expect("ico sphere"))),
        MeshMaterial3d(sky.clone()),
        Transform::default(),
        NoFrustumCulling,
        hidden,
    ));
    commands.insert_resource(ChartScene {
        camera,
        colony,
        landmarks,
        rocks: None,
        rocks_drawn: None,
        sky,
        holos: made,
    });
}

/// The graphics tier on the chart's camera too (at the start, and whenever F10 changes it): HDR
/// and bloom where the tier has them, so the chart's light glows; multisampling for its lines.
fn apply_chart_tier(mut commands: Commands, gfx: Res<Gfx>, cams: Query<Entity, With<ChartCamera>>) {
    if !gfx.is_changed() {
        return;
    }
    for cam in &cams {
        let mut e = commands.entity(cam);
        e.insert(if gfx.settings.msaa > 1 { Msaa::Sample4 } else { Msaa::Off });
        if gfx.settings.hdr {
            e.insert((bevy::camera::Hdr, gfx.tonemapping, Bloom { intensity: 0.22, ..Bloom::NATURAL }));
        } else {
            e.remove::<(bevy::camera::Hdr, Bloom)>();
        }
    }
}

/// The places the chart lists, for a click to go to: the sector's, then the Earth Sphere's.
fn listed() -> Vec<Place> {
    let mut out = vec![Place::Colony, Place::Dock, Place::Field];
    for (k, def) in LANDMARKS.iter().enumerate() {
        out.push(Place::Landmark(k as u8));
        for s in 0..def.hides.len() {
            out.push(Place::HideSpot(k as u8, s as u8));
        }
    }
    out.extend([Place::Earth, Place::Moon, Place::Sun]);
    out.extend(Lagrange::ALL.map(Place::Lagrange));
    out
}

/// A place's short name for its chip (ASCII, caps), without a world to ask.
fn chip_name(p: Place) -> String {
    match p {
        Place::Colony => "COLONY".into(),
        Place::Dock => "DOCK".into(),
        Place::Field => "FIELD".into(),
        Place::Landmark(k) => LANDMARKS.get(usize::from(k)).map_or("?".into(), |d| d.name.to_string()),
        Place::HideSpot(k, s) => LANDMARKS
            .get(usize::from(k))
            .and_then(|d| d.hides.get(usize::from(s)))
            .map_or("?".into(), |h| h.name.to_string()),
        Place::Earth => "EARTH".into(),
        Place::Moon => "MOON".into(),
        Place::Sun => "SUN".into(),
        Place::Lagrange(l) => l.name().into(),
        _ => String::new(),
    }
}

fn setup_chart_ui(
    mut commands: Commands,
    font: Res<UiFont>,
    scene: Res<ChartScene>,
    mut panels: ResMut<Assets<PanelMaterial>>,
) {
    let f = &*font;
    let plate = panels.add(PanelMaterial::plate(PLATE, PLATE_EDGE, 10.0, 1.0));
    let text = |p: &mut ChildSpawnerCommands, which: ChartText, size: f32, color: Color, mono: bool| {
        p.spawn((
            which,
            Text::new(""),
            if mono { f.text(size) } else { f.heading(size) },
            TextColor(color),
            SHADOW,
        ));
    };
    let chip = |p: &mut ChildSpawnerCommands, b: ChartButton, label: &str| {
        p.spawn((
            b,
            Button,
            Interaction::default(),
            ChartPanel,
            Node {
                padding: UiRect::axes(Val::Px(7.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(CHIP),
            BorderColor::all(Color::srgba(0.45, 0.85, 1.0, 0.35)),
        ))
        .with_children(|c| {
            c.spawn((
                ButtonText,
                Text::new(label),
                f.heading(10.5),
                TextColor(WHITE),
                LetterSpacing::Px(1.0),
                TextLayout::default().with_no_wrap(),
            ));
        });
    };
    commands
        .spawn((
            ChartUi,
            UiTargetCamera(scene.camera),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|root| {
            // The labels, under everything else.
            for _ in 0..LABELS {
                root.spawn((
                    ChartLabel,
                    Node { position_type: PositionType::Absolute, ..default() },
                    Text::new(""),
                    f.heading(11.0),
                    TextColor(LABEL),
                    SHADOW,
                    LetterSpacing::Px(1.0),
                    TextLayout::default().with_no_wrap(),
                    Visibility::Hidden,
                ));
            }
            // Top left: what this is, the scale, the views.
            root.spawn((
                ChartPanel,
                Interaction::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(14.0),
                    top: Val::Px(12.0),
                    width: Val::Px(LEFT_W),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                MaterialNode(plate.clone()),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("NAV CHART  ·  SECTOR L1"),
                    f.heading(14.0),
                    TextColor(CYAN),
                    LetterSpacing::Px(3.0),
                    SHADOW,
                ));
                text(p, ChartText::Scale, 11.0, LABEL, true);
                p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(5.0), row_gap: Val::Px(5.0), ..default() })
                    .with_children(|row| {
                        chip(row, ChartButton::You, "YOU  H");
                        chip(row, ChartButton::Sector, "SECTOR  2");
                        chip(row, ChartButton::Sphere, "EARTH SPHERE  3");
                        chip(row, ChartButton::TopDown, "TOP  T");
                        chip(row, ChartButton::Plane, "PLANE  P");
                    });
                p.spawn((
                    Text::new("OBJECTIVES"),
                    f.heading(10.0),
                    TextColor(LABEL),
                    LetterSpacing::Px(2.0),
                    Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                ));
                text(p, ChartText::Objectives, 11.5, WHITE, true);
            });
            // Top middle, between the panels: the course, the auto-nav, and what's closing in.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                left: Val::Px(LEFT_W + 28.0),
                right: Val::Px(RIGHT_W + 28.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    ChartText::Status,
                    Text::new(""),
                    f.heading(13.0),
                    TextColor(COURSE),
                    SHADOW,
                    LetterSpacing::Px(2.0),
                    TextLayout::justify(Justify::Center),
                ));
            });
            // Right: what's selected (only while something is).
            root.spawn((
                ChartCard,
                ChartPanel,
                Interaction::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(14.0),
                    top: Val::Px(12.0),
                    width: Val::Px(RIGHT_W),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    row_gap: Val::Px(5.0),
                    display: Display::None,
                    ..default()
                },
                MaterialNode(plate.clone()),
            ))
            .with_children(|p| {
                text(p, ChartText::Kind, 10.0, LABEL, false);
                text(p, ChartText::Name, 17.0, WHITE, false);
                text(p, ChartText::Facts, 11.5, CYAN, true);
                text(p, ChartText::About, 11.0, LABEL, true);
                p.spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(5.0),
                    row_gap: Val::Px(5.0),
                    margin: UiRect::top(Val::Px(3.0)),
                    ..default()
                })
                .with_children(|row| {
                    chip(row, ChartButton::SetCourse, "SET COURSE  ENTER");
                    chip(row, ChartButton::AutoNav, "AUTO-NAV  N");
                    chip(row, ChartButton::Focus, "FOCUS");
                    chip(row, ChartButton::Clear, "CLEAR COURSE");
                });
            });
            // Bottom left: the places to go, a click each.
            root.spawn((
                ChartPanel,
                Interaction::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(14.0),
                    bottom: Val::Px(40.0),
                    max_width: Val::Percent(46.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                MaterialNode(plate.clone()),
            ))
            .with_children(|p| {
                p.spawn((Text::new("PLACES   [ ]"), f.heading(10.0), TextColor(LABEL), LetterSpacing::Px(2.0)));
                p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() })
                    .with_children(|row| {
                        for place in listed() {
                            chip(row, ChartButton::Go(place), &chip_name(place));
                        }
                    });
            });
            // The hover's tag, by the cursor.
            root.spawn((
                ChartText::Hover,
                Text::new(""),
                f.heading(12.0),
                TextColor(WHITE),
                SHADOW,
                Node { position_type: PositionType::Absolute, ..default() },
                TextLayout::default().with_no_wrap(),
                Visibility::Hidden,
            ));
            // Bottom: the keys.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    ChartText::Help,
                    Text::new(
                        "DRAG turn   RIGHT-DRAG pan   WHEEL zoom   CLICK select   DOUBLE-CLICK focus / mark   \
                         [ ] step through places   ENTER / RIGHT-CLICK set course   N auto-nav   M close",
                    ),
                    f.heading(10.5),
                    TextColor(LABEL),
                    SHADOW,
                    LetterSpacing::Px(1.0),
                ));
            });
        });
}

/// What the chart knows of the pilot's suit this frame.
#[derive(Clone, Copy, Debug)]
struct Me {
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
}

fn me(core: &bc_client_core::ClientCore) -> Option<Me> {
    core.own_view().filter(|v| v.alive).map(|v| Me { pos: v.pos, vel: v.flight_vel, rot: v.rot })
}

/// Opens with the view on the pilot's suit; keeps the camera, the selection and the plane between
/// openings.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn chart_input(
    open: Res<MapOpen>,
    mut chart: ResMut<Chart>,
    mut target: ResMut<NavTarget>,
    mut ui: ResMut<Ui>,
    game: NonSend<GameClient>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<VisTime>,
    window: Single<&Window, With<PrimaryWindow>>,
    panels: Query<&Interaction, With<ChartPanel>>,
    pressed: Query<(&ChartButton, &Interaction), Changed<Interaction>>,
) {
    let c = &mut *chart;
    if !open.0 {
        c.was_open = false;
        c.drag = None;
        c.hover = None;
        return;
    }
    let mut g = game.borrow_mut();
    let own = me(&g.core);
    if !c.was_open {
        c.was_open = true;
        // Back on the pilot's suit, if the view was following it.
        if c.cam.tracking
            && let Some(m) = own
        {
            c.cam.now.focus = m.pos;
            c.cam.goal.focus = m.pos;
        }
    }
    let dt = time.dt.min(0.1);
    let over_panel = panels.iter().any(|i| *i != Interaction::None);
    let cursor = window.cursor_position().filter(|_| !over_panel);
    c.cursor = cursor;
    let size = Vec2::new(window.width(), window.height());
    // The point under the cursor at the focus's depth (what zooming in closes on).
    let under = |c: &Chart, s: Vec2| {
        let ray = c.view.ray(s);
        let depth = (c.cam.now.focus - c.view.eye).dot(c.view.fwd).max(1.0);
        c.view.eye + ray * depth / ray.dot(c.view.fwd).max(1e-3)
    };
    // The wheel.
    let wheel = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 60.0,
    };
    if wheel != 0.0 && !over_panel {
        let at = cursor.map(|s| under(c, s));
        c.cam.zoom(wheel.clamp(-3.0, 3.0), at);
    }
    // Hover.
    c.hover = cursor.and_then(|s| {
        let items: Vec<Pickable> = c.picks.iter().map(|p| p.1).collect();
        pick(&items, s, 8.0).map(|i| c.picks[i].0)
    });
    // Drags and clicks.
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if mouse.just_pressed(b)
            && let Some(s) = cursor
        {
            c.drag = Some(Drag { button: b, start: s, last: s, moved: false });
        }
    }
    let now_pos = window.cursor_position();
    let mut click: Option<(MouseButton, Vec2)> = None;
    if let Some(mut d) = c.drag {
        if let Some(s) = now_pos {
            let delta = s - d.last;
            d.moved |= s.distance(d.start) > DRAG_PX;
            if d.moved && delta != Vec2::ZERO {
                if d.button == MouseButton::Left && !shift {
                    c.cam.orbit(delta);
                } else {
                    c.cam.pan(delta, size.y);
                }
            }
            d.last = s;
        }
        if mouse.just_released(d.button) {
            if !d.moved {
                click = Some((d.button, d.start));
            }
            c.drag = None;
        } else {
            c.drag = Some(d);
        }
    }
    let t = g.core.render_tick(now_s());
    if let Some((button, s)) = click {
        let now = time.now;
        let double = c.last_click.is_some_and(|(at, p)| now - at < DOUBLE_S && p.distance(s) < 10.0);
        c.last_click = if double { None } else { Some((now, s)) };
        match (button, c.hover) {
            (MouseButton::Left, Some(p)) if double => focus_on(c, &g.core.world, p, t),
            (MouseButton::Left, Some(p)) => c.selected = Some(p),
            (MouseButton::Left, None) if double => {
                // A point on the plane, under the cursor.
                let plane_y = plane_height(c, own);
                let ray = c.view.ray(s);
                if ray.y.abs() > 1e-4 {
                    let k = (plane_y - c.view.eye.y) / ray.y;
                    let p = c.view.eye + ray * k;
                    if k > 0.0 && p.abs().max_element() < SECTOR_LIMIT - 500.0 {
                        c.selected = Some(Place::Point(p));
                    } else {
                        ui.toast("THAT'S OUTSIDE THE SECTOR: MARK A POINT ON THE PLANE INSIDE IT");
                    }
                }
            }
            (MouseButton::Left, None) => c.selected = None,
            (MouseButton::Right, Some(p)) => {
                c.selected = Some(p);
                set_course(&mut target, &mut g, &mut ui, p, false);
            }
            _ => {}
        }
    }
    // The panels' buttons.
    for (b, i) in &pressed {
        if *i != Interaction::Pressed {
            continue;
        }
        match *b {
            ChartButton::SetCourse => {
                if let Some(p) = c.selected {
                    set_course(&mut target, &mut g, &mut ui, p, false);
                }
            }
            ChartButton::AutoNav => toggle_auto(c, &mut target, &mut g, &mut ui),
            ChartButton::Clear => clear_course(&mut target, &mut g, &mut ui),
            ChartButton::Focus => {
                if let Some(p) = c.selected {
                    focus_on(c, &g.core.world, p, t);
                }
            }
            ChartButton::You => home(c, own),
            ChartButton::Sector => c.cam.fly_to(Vec3::new(0.0, -1_500.0, 0.0), 75_000.0, false),
            ChartButton::Sphere => sphere_view(c),
            ChartButton::TopDown => {
                let on = c.cam.goal.pitch < 1.4;
                c.cam.top_down(on);
            }
            ChartButton::Plane => c.plane_on_pilot = !c.plane_on_pilot,
            ChartButton::Go(p) => {
                c.selected = Some(p);
                focus_on(c, &g.core.world, p, t);
            }
        }
    }
    // The keys. [ and ] step through the places the chart lists (those the sector has).
    let step =
        keys.just_pressed(KeyCode::BracketRight) as i32 - keys.just_pressed(KeyCode::BracketLeft) as i32;
    if step != 0 {
        let have = g.core.world.bodies.landmarks().len();
        let places: Vec<Place> = listed()
            .into_iter()
            .filter(|p| !matches!(p, Place::Landmark(k) | Place::HideSpot(k, _) if usize::from(*k) >= have))
            .collect();
        let at = c.selected.and_then(|s| places.iter().position(|p| *p == s));
        let next = match at {
            Some(i) => (i as i32 + step).rem_euclid(places.len() as i32) as usize,
            None if step > 0 => 0,
            None => places.len() - 1,
        };
        c.selected = Some(places[next]);
        focus_on(c, &g.core.world, places[next], t);
    }
    if keys.just_pressed(KeyCode::Enter)
        && let Some(p) = c.selected
    {
        set_course(&mut target, &mut g, &mut ui, p, false);
    }
    if keys.just_pressed(NAV_KEY) {
        toggle_auto(c, &mut target, &mut g, &mut ui);
    }
    if keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::Delete) {
        clear_course(&mut target, &mut g, &mut ui);
    }
    if keys.just_pressed(KeyCode::KeyH)
        || keys.just_pressed(KeyCode::Home)
        || keys.just_pressed(KeyCode::Digit1)
    {
        home(c, own);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        c.cam.fly_to(Vec3::new(0.0, -1_500.0, 0.0), 75_000.0, false);
    }
    if keys.just_pressed(KeyCode::Digit3) {
        sphere_view(c);
    }
    if keys.just_pressed(KeyCode::KeyT) {
        let on = c.cam.goal.pitch < 1.4;
        c.cam.top_down(on);
    }
    if keys.just_pressed(KeyCode::KeyP) {
        c.plane_on_pilot = !c.plane_on_pilot;
        ui.toast(if c.plane_on_pilot {
            "CHART PLANE: THROUGH YOUR SUIT"
        } else {
            "CHART PLANE: THE SECTOR'S MIDDLE"
        });
    }
    let axis = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32;
    let fly = Vec3::new(
        axis(KeyCode::KeyD, KeyCode::KeyA),
        axis(KeyCode::KeyR, KeyCode::KeyF),
        axis(KeyCode::KeyW, KeyCode::KeyS),
    );
    if fly != Vec3::ZERO {
        // Across the plane as the view faces it, at a screen's width in two seconds.
        let back = c.cam.goal.back();
        let ahead = Vec3::new(-back.x, 0.0, -back.z).normalize_or(Vec3::Z);
        let right = ahead.cross(Vec3::Y);
        let speed = c.cam.goal.dist * 0.6;
        c.cam.goal.focus += (right * fly.x + Vec3::Y * fly.y + ahead * fly.z) * speed * dt;
        c.cam.tracking = false;
    }
    let turn = axis(KeyCode::KeyE, KeyCode::KeyQ);
    if turn != 0.0 {
        c.cam.orbit(Vec2::new(turn * 220.0 * dt, 0.0));
    }
}

/// The whole Earth Sphere, seen from above the Moon's orbit (tilted toward the Moon's side), so
/// the orbit is a ring rather than a line.
fn sphere_view(c: &mut Chart) {
    c.cam.fly_to(view::sphere_centre(), 1.3e9, false);
    c.cam.look_from(sphere::orbit_normal() * 0.8 + sphere::MOON_DIR * 0.6);
}

/// The height of the chart's plane.
fn plane_height(c: &Chart, own: Option<Me>) -> f32 {
    match own {
        Some(m) if c.plane_on_pilot => m.pos.y,
        _ => 0.0,
    }
}

/// Back to the pilot's suit, following it.
fn home(c: &mut Chart, own: Option<Me>) {
    if let Some(m) = own {
        let dist = if c.cam.goal.dist > 60_000.0 { 9_000.0 } else { c.cam.goal.dist };
        c.cam.fly_to(m.pos, dist, true);
    }
}

/// Flies the view to `p`, near enough to see it whole.
fn focus_on(c: &mut Chart, world: &bc_client_core::World, p: Place, t: f64) {
    let Some(at) = p.locate(world, t) else { return };
    let dist = match p {
        Place::Earth | Place::Moon => at.reach * 7.0,
        Place::Sun => {
            // Look sunward from the Earth Sphere: the Sun itself is a light at infinity.
            c.cam.fly_to(view::sphere_centre(), 1.2e9, false);
            return;
        }
        Place::Lagrange(Lagrange::L1) => 75_000.0,
        Place::Lagrange(_) => 1.5e8,
        Place::Colony => 70_000.0,
        Place::Field => 22_000.0,
        Place::Point(_) => 5_000.0,
        Place::Suit(..) => 1_500.0,
        _ => (at.reach * 6.0).max(900.0),
    };
    c.cam.fly_to(at.pos, dist, false);
}

/// Sets a course to `p` (none beyond the sector), and with `auto`, engages the auto-nav on it.
fn set_course(target: &mut NavTarget, g: &mut crate::net::Game, ui: &mut Ui, p: Place, auto: bool) {
    let world = &g.core.world;
    if !p.reachable() {
        ui.toast(format!("NO LANE TO {} FROM L1 YET", p.name(world)));
        return;
    }
    let name = p.name(world);
    target.place = Some(p);
    target.plotted = f64::NEG_INFINITY;
    target.course = Course::default();
    if auto {
        if g.core.world.own.is_none_or(|o| !o.alive) {
            ui.toast("AUTO-NAV: NO SUIT TO FLY");
            return;
        }
        if g.core.world.own.is_some_and(|o| o.flags & own_flags::DOCKED != 0) {
            ui.toast("AUTO-NAV: DOCKED");
            return;
        }
        g.nav = Some(AutoNav::new(p));
        ui.toast(format!("AUTO-NAV: {name} · any flight key takes the stick back"));
    } else {
        // A new course over the one being flown flies the new one.
        if let Some(n) = g.nav.as_mut()
            && n.place != p
        {
            *n = AutoNav::new(p);
        }
        ui.toast(format!("COURSE SET: {name}"));
    }
}

fn toggle_auto(c: &mut Chart, target: &mut NavTarget, g: &mut crate::net::Game, ui: &mut Ui) {
    if g.nav.is_some() {
        g.nav = None;
        ui.toast("AUTO-NAV OFF");
        return;
    }
    let Some(p) = c.selected.filter(|p| p.reachable()).or(target.place) else {
        ui.toast("AUTO-NAV: SELECT SOMEWHERE IN THE SECTOR FIRST");
        return;
    };
    set_course(target, g, ui, p, true);
}

fn clear_course(target: &mut NavTarget, g: &mut crate::net::Game, ui: &mut Ui) {
    if target.place.take().is_some() || g.nav.is_some() {
        ui.toast("COURSE CLEARED");
    }
    target.course = Course::default();
    target.arrival = None;
    g.nav = None;
}

/// N in flight: the auto-nav on the course set on the chart, or off.
pub fn nav_key(
    keys: Res<ButtonInput<KeyCode>>,
    open: Res<MapOpen>,
    indoors: Res<crate::hangar::Indoors>,
    mut ui: ResMut<Ui>,
    mut target: ResMut<NavTarget>,
    game: NonSend<GameClient>,
) {
    if open.0 || !ui.playing() || ui.panel_open() || indoors.0 || ui.on_foot || !keys.just_pressed(NAV_KEY) {
        return;
    }
    let mut g = game.borrow_mut();
    if g.nav.take().is_some() {
        ui.toast("AUTO-NAV OFF");
        return;
    }
    match target.place {
        Some(p) => set_course(&mut target, &mut g, &mut ui, p, true),
        None => ui.toast("AUTO-NAV: SET A COURSE ON THE CHART FIRST (M)"),
    }
}

/// Keeps the set course plotted from where the suit is (a couple of times a second), follows the
/// auto-nav's, and says when it's over: arrived, or what it was going to is gone.
pub fn track_course(
    mut target: ResMut<NavTarget>,
    mut ui: ResMut<Ui>,
    mut controls: ResMut<crate::input::Controls>,
    game: NonSend<GameClient>,
) {
    let mut g = game.borrow_mut();
    // A suit destroyed or docked has nothing to fly: the auto-nav lets go without a word (the
    // course stays set for the next sortie). While it flies, the grip is off: it's the pilot's to
    // arm when they're there.
    let flying = g.core.world.own.is_some_and(|o| o.alive && o.flags & own_flags::DOCKED == 0);
    if g.nav.is_some() {
        if flying {
            controls.grip = false;
        } else {
            g.nav = None;
        }
    }
    // The auto-nav's news.
    let done = g.nav.as_ref().map(|n| (n.state, n.place));
    match done {
        Some((NavState::Arrived, p)) => {
            g.nav = None;
            let name = p.name(&g.core.world);
            let next = match p {
                Place::Landmark(_) | Place::HideSpot(..) | Place::Rock(_) => " · L arms the grip to land",
                Place::Dock if g.core.welcome.is_some_and(|w| w.survival) => " · Enter docks",
                _ => "",
            };
            ui.toast(format!("ARRIVED: {name}{next}"));
            // The course is done with.
            if target.place == Some(p) {
                target.place = None;
            }
            // The grip is the pilot's to arm.
            controls.grip = false;
        }
        Some((NavState::Lost, p)) => {
            g.nav = None;
            target.place = None;
            ui.toast(format!("AUTO-NAV: LOST {}", p.name(&g.core.world)));
        }
        _ => {}
    }
    let Some(place) = target.place else {
        target.course = Course::default();
        target.arrival = None;
        return;
    };
    let core = &g.core;
    let Some(m) = me(core) else { return };
    // Flown there by hand: there, at rest, the course is done with too.
    if g.nav.is_none()
        && let Some(a) = target.arrival
        && m.pos.distance(a.point) < nav::ARRIVE_RANGE * 1.5
        && (m.vel - a.vel).length() < nav::ARRIVE_SPEED * 2.0
    {
        ui.toast(format!("ARRIVED: {}", place.name(&core.world)));
        target.place = None;
        target.course = Course::default();
        target.arrival = None;
        return;
    }
    let t = core.render_tick(now_s());
    let now = now_s();
    if let Some(n) = g.nav.as_ref().filter(|n| n.place == place && n.course().points.len() >= 2) {
        // Flying it: the auto-nav's own course, from the suit as drawn.
        target.course = n.course().clone();
        target.course.points[0] = m.pos;
        target.arrival = n.arrival;
        return;
    }
    if now - target.plotted < REPLOT_S {
        if let Some(first) = target.course.points.first_mut() {
            *first = m.pos;
        }
        return;
    }
    target.plotted = now;
    match place.arrival(&core.world, t, m.pos) {
        Some(a) => {
            target.course = nav::plot(&core.world.bodies, t, m.pos, a.point);
            target.arrival = Some(a);
        }
        None => {
            ui.toast(format!("COURSE: LOST {}", place.name(&core.world)));
            target.place = None;
            target.course = Course::default();
            target.arrival = None;
        }
    }
}

/// How bright the sector's lines are at this view distance, and the Earth Sphere's.
fn fades(dist: f32) -> (f32, f32) {
    let s = |a: f32, b: f32, x: f32| {
        let t = ((x.ln() - a.ln()) / (b.ln() - a.ln())).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    (1.0 - s(SECTOR_FADE.0, SECTOR_FADE.1, dist), s(SPHERE_FADE.0, SPHERE_FADE.1, dist))
}

/// The chart's camera: on while the chart is open (and the world's off: nothing of it shows), its
/// view eased to the chart's.
#[allow(clippy::type_complexity)]
pub fn chart_camera(
    open: Res<MapOpen>,
    mut chart: ResMut<Chart>,
    scene: Res<ChartScene>,
    game: NonSend<GameClient>,
    time: Res<VisTime>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cams: Query<
        (&mut Camera, &mut Transform, &mut Projection, Has<MainCamera>),
        Or<(With<MainCamera>, With<ChartCamera>)>,
    >,
) {
    for (mut cam, _, _, main) in &mut cams {
        let on = if main { !open.0 } else { open.0 };
        if cam.is_active != on {
            cam.is_active = on;
        }
    }
    if !open.0 {
        return;
    }
    let held = me(&game.borrow().core).map(|m| m.pos);
    let c = &mut *chart;
    c.cam.step(time.dt, held);
    let size = Vec2::new(window.width(), window.height());
    c.view = Projector::new(&c.cam.now, size);
    if let Ok((_, mut tf, mut proj, _)) = cams.get_mut(scene.camera) {
        *tf = Transform::from_translation(c.view.eye).looking_to(c.view.fwd, Vec3::Y);
        if let Projection::Perspective(p) = &mut *proj {
            p.near = c.cam.near();
        }
    }
}

/// Everything the chart draws in 3D: the holograms posed, the lines, the marks, and what can be
/// picked and labelled where.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn chart_draw(
    open: Res<MapOpen>,
    mut chart: ResMut<Chart>,
    mut scene: ResMut<ChartScene>,
    target: Res<NavTarget>,
    objectives: Res<ObjectiveState>,
    settings: Res<SettingsRes>,
    game: NonSend<GameClient>,
    time: Res<VisTime>,
    mut commands: Commands,
    (mut meshes, mut dots, mut dot_look): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<DotMaterial>>,
        ResMut<DotLook>,
    ),
    (mut holos, mut skies): (ResMut<Assets<HoloMaterial>>, ResMut<Assets<ChartSkyMaterial>>),
    mut tfs: Query<(&mut Transform, &mut Visibility), Without<ChartCamera>>,
    (mut lines, mut bold, mut dashes): (Gizmos<ChartLines>, Gizmos<ChartBold>, Gizmos<ChartDashes>),
) {
    if !open.0 {
        return;
    }
    let g = game.borrow();
    let core = &g.core;
    let world = &core.world;
    let t = core.render_tick(now_s());
    let own = me(core);
    let c = &mut *chart;
    let v = c.view;
    let (sector, sphere_a) = fades(c.cam.now.dist);
    let secs = (time.now % 3_600.0) as f32;
    let pulse = 0.5 + 0.5 * (secs * 3.0).sin();
    let mut picks: Vec<(Place, Pickable)> = Vec::new();
    let mut labels: Vec<Want> = Vec::new();
    let mut label = |p: Vec3, text: String, color: Color, rank: u16| {
        if let Some(s) = v.project(p).filter(|s| v.on_screen(*s, 0.0)) {
            labels.push(Want { at: s, text, color, rank });
        }
    };
    let mut pickable = |place: Place, p: Vec3, radius: f32, rank: u8| {
        if let Some(s) = v.project(p).filter(|s| v.on_screen(*s, 20.0)) {
            picks.push((place, Pickable { at: s, radius: radius.min(400.0), rank }));
        }
    };
    // A glyph a few pixels across, facing the view.
    let px = |p: Vec3| v.per_px(p);
    let (right, up) = (v.right, v.up);

    // The holograms: posed and faded.
    let spin = colony_spin_angle(t.max(0.0).floor() as u32, t.max(0.0).fract() as f32);
    if let Ok((mut tf, _)) = tfs.get_mut(scene.colony) {
        tf.rotation = Quat::from_rotation_x(spin);
    }
    for (k, e) in scene.landmarks.clone().into_iter().enumerate() {
        let pose = world.bodies.pose_at(Body::Landmark(k as u8), t);
        if let Ok((mut tf, mut vis)) = tfs.get_mut(e) {
            match pose {
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
    for (h, sector_side) in &scene.holos {
        if let Some(mut m) = holos.get_mut(h) {
            m.holo.params.y = secs;
            m.holo.params.z = if *sector_side { sector.max(0.0) } else { 1.0 };
        }
    }
    if let Some(mut s) = skies.get_mut(&scene.sky) {
        s.sky.sun.w = secs;
    }
    // The field's rocks, as lights that keep a least size on screen (drawn again when the field
    // changes or a rock shatters or grows back).
    let field = &world.bodies.field;
    let dead: u64 =
        (0..field.len()).filter(|&i| field.is_dead(i)).fold(0u64, |h, i| h.rotate_left(5) ^ i as u64);
    let key = core.welcome.map(|w| (w.field_seed, w.field_rocks, dead));
    if key != scene.rocks_drawn {
        scene.rocks_drawn = key;
        if let Some(e) = scene.rocks.take() {
            commands.entity(e).despawn();
        }
        let mut d = Dots::default();
        let ore = [
            Vec3::new(0.9, 0.8, 0.7),
            Vec3::new(0.7, 0.9, 1.3),
            Vec3::new(0.5, 1.2, 1.3),
            Vec3::new(1.3, 0.6, 1.4),
        ];
        for (i, r) in field.rocks().iter().enumerate() {
            if !field.is_dead(i) {
                d.add(r.pos, r.radius, ore[usize::from(r.ore.min(3))] * 0.5, Blink::Steady);
            }
        }
        if !field.is_empty() {
            let material: Handle<DotMaterial> = dot_look.get(&mut dots);
            scene.rocks = Some(
                commands
                    .spawn((
                        Mesh3d(meshes.add(d.mesh())),
                        MeshMaterial3d(material),
                        Transform::default(),
                        NoFrustumCulling,
                        RenderLayers::layer(CHART_LAYER),
                    ))
                    .id(),
            );
        }
    }

    // The plane: a grid round the focus (fading out from it), range rings round the pilot.
    let plane_y = plane_height(c, own);
    let span = c.cam.now.dist * 1.6;
    let step = nice_step(span);
    if sector > 0.01 {
        let centre = Vec3::new(c.cam.now.focus.x, plane_y, c.cam.now.focus.z);
        let snap = (centre / step).round() * step;
        let n = 12;
        let reach = step * n as f32;
        let grid = |a: f32| glow(CYAN, 0.55, a * sector);
        for k in -n..=n {
            let o = k as f32 * step;
            for (a, b) in [
                (
                    Vec3::new(snap.x + o, plane_y, snap.z - reach),
                    Vec3::new(snap.x + o, plane_y, snap.z + reach),
                ),
                (
                    Vec3::new(snap.x - reach, plane_y, snap.z + o),
                    Vec3::new(snap.x + reach, plane_y, snap.z + o),
                ),
            ] {
                // Brightest through the focus, gone at the edge.
                let mid = a.lerp(b, 0.5);
                let near = 1.0 - (mid.distance(centre) / reach).min(1.0);
                let alpha = 0.22 * near * near;
                lines.line_gradient(a, mid, grid(0.0), grid(alpha));
                lines.line_gradient(mid, b, grid(alpha), grid(0.0));
            }
        }
        if let Some(m) = own {
            let foot = Vec3::new(m.pos.x, plane_y, m.pos.z);
            let flat = Isometry3d::new(foot, Quat::from_rotation_x(FRAC_PI_2));
            for r in [1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0] {
                let on = (r / c.cam.now.dist).clamp(0.0, 1.0);
                if r < c.cam.now.dist * 4.0 && r > c.cam.now.dist * 0.04 {
                    lines.circle(flat, r, glow(CYAN, 0.8, 0.28 * sector * (0.4 + on))).resolution(96);
                    label(foot + Vec3::X * r, range(r), glow(LABEL, 1.0, 0.75), 2);
                }
            }
            // Which way is which: the sunward cap's end of the axis, and the dock's.
            let r = c.cam.now.dist * 0.9;
            label(foot + Vec3::X * r, "+X  SUNWARD".into(), LABEL, 3);
            label(foot - Vec3::X * r, "-X  DOCK END".into(), LABEL, 3);
        }
    }
    // A stalk from `p` down (or up) to the plane, with a ring at its foot.
    let stalk = |lines: &mut Gizmos<ChartLines>, p: Vec3, color: Color| {
        if (p.y - plane_y).abs() < 1.0 || sector < 0.01 {
            return;
        }
        let foot = Vec3::new(p.x, plane_y, p.z);
        lines.line(p, foot, color.with_alpha(0.45 * sector));
        lines.circle(
            Isometry3d::new(foot, Quat::from_rotation_x(FRAC_PI_2)),
            px(foot) * 4.0,
            color.with_alpha(0.6 * sector),
        );
    };

    if sector > 0.01 {
        // The sector's limit.
        let l = SECTOR_LIMIT;
        // (Only once the view is out far enough to take it in: up close its edges are just
        // lines across the view.)
        let wide = ((c.cam.now.dist - 15_000.0) / 35_000.0).clamp(0.0, 1.0);
        let box_color = glow(AMBER, 0.6, 0.18 * sector * wide);
        if wide > 0.0 {
            for (a, b) in cube_edges(l) {
                dashes.line(a, b, box_color);
            }
        }
        // The colony: its ends, the edges of its windows turning with it, the dock's ring.
        let colony_col = glow(CYAN, 1.0, 0.55 * sector);
        for side in [-1.0, 1.0] {
            let at = COLONY_CENTER + Vec3::X * side * COLONY_HALF_LENGTH;
            lines
                .circle(Isometry3d::new(at, Quat::from_rotation_y(FRAC_PI_2)), COLONY_RADIUS, colony_col)
                .resolution(96);
        }
        for k in 0..6 {
            let a = FIRST_WINDOW + spin + (k as f32 + 0.5) * TAU / 6.0 - TAU / 12.0;
            let off = Vec3::new(0.0, a.cos(), a.sin()) * COLONY_RADIUS;
            lines.line(
                COLONY_CENTER + off - Vec3::X * COLONY_HALF_LENGTH,
                COLONY_CENTER + off + Vec3::X * COLONY_HALF_LENGTH,
                glow(CYAN, 0.8, 0.3 * sector),
            );
        }
        let dock_col = glow(AMBER, 1.6, 0.9 * sector);
        bold.circle(Isometry3d::new(DOCK_CENTER, Quat::from_rotation_y(FRAC_PI_2)), DOCK_RADIUS, dock_col)
            .resolution(48);
        label(
            COLONY_CENTER + Vec3::new(COLONY_HALF_LENGTH * 0.35, COLONY_RADIUS * 1.25, 0.0),
            "THE FIRST COLONY".into(),
            CYAN,
            50,
        );
        label(DOCK_CENTER + Vec3::new(0.0, DOCK_RADIUS * 1.4, 0.0), "DOCK".into(), AMBER, 55);
        // The colony is picked wherever along it the cursor is: at the point of its axis nearest.
        if let Some(cur) = c.cursor {
            let (a, b) =
                (COLONY_CENTER - Vec3::X * COLONY_HALF_LENGTH, COLONY_CENTER + Vec3::X * COLONY_HALF_LENGTH);
            if let (Some(sa), Some(sb)) = (v.project(a), v.project(b)) {
                let ab = sb - sa;
                let k = ((cur - sa).dot(ab) / ab.length_squared().max(1.0)).clamp(0.0, 1.0);
                let on = a.lerp(b, k);
                pickable(Place::Colony, on, COLONY_RADIUS / px(on), 0);
            }
        }
        pickable(Place::Dock, DOCK_CENTER, (DOCK_RADIUS / px(DOCK_CENTER)).max(8.0), 4);
        // The debris field's middle.
        lines
            .circle(
                Isometry3d::new(FIELD_CENTER, Quat::from_rotation_x(FRAC_PI_2)),
                7_500.0,
                glow(GREY, 0.8, 0.12 * sector),
            )
            .resolution(96);
        label(FIELD_CENTER + Vec3::new(0.0, 0.0, -7_700.0), "DEBRIS FIELD".into(), GREY, 30);
        pickable(Place::Field, FIELD_CENTER, 8.0, 0);
        // The landmarks, their circles, their hide spots.
        for (k, def) in world.bodies.landmarks().iter().enumerate() {
            let k = k as u8;
            let Some(pose) = world.bodies.pose_at(Body::Landmark(k), t) else { continue };
            if def.orbit_radius > 0.0 {
                let centre = def.center;
                dashes.circle(
                    Isometry3d::new(centre, Quat::from_rotation_x(FRAC_PI_2)),
                    def.orbit_radius,
                    glow(CYAN, 0.7, 0.35 * sector),
                );
            }
            stalk(&mut lines, pose.pos, CYAN);
            let r_px = def.bound / px(pose.pos);
            label(pose.pos + up * (def.bound + 12.0 * px(pose.pos)), def.name.to_string(), CYAN, 60);
            pickable(Place::Landmark(k), pose.pos, r_px.max(10.0), 3);
            for (s, h) in def.hides.iter().enumerate() {
                let at = pose.to_world(h.center);
                let n = pose.rot * def.shape.probe(h.center).normal;
                let ring = Isometry3d::new(at + n * 2.0, Quat::from_rotation_arc(Vec3::Z, n));
                lines.circle(ring, h.radius, glow(AMBER, 1.2, 0.8 * sector)).resolution(24);
                if c.cam.now.dist < def.bound * 12.0 || c.selected == Some(Place::HideSpot(k, s as u8)) {
                    label(at + n * (h.radius + 8.0 * px(at)), h.name.to_string(), AMBER, 25);
                }
                pickable(Place::HideSpot(k, s as u8), at, (h.radius / px(at)).max(6.0), 5);
            }
        }
        // The big rocks can be picked (and labelled when close).
        for (i, r) in field.rocks().iter().enumerate() {
            if field.is_dead(i) {
                continue;
            }
            let rpx = r.radius / px(r.pos);
            pickable(Place::Rock(i as u16), r.pos, rpx.max(4.0), 0);
            if rpx > 6.0 && r.axes.min_element() >= bc_sim::bodies::GRIP_MIN_AXIS {
                label(r.pos + up * (r.radius + 6.0 * px(r.pos)), format!("ROCK {i}"), GREY, 4);
            }
        }
        // The suits in sight, nearest first.
        let from = own.map_or(c.cam.now.focus, |m| m.pos);
        let mut seen: Vec<_> = world
            .entities
            .iter()
            .flatten()
            .filter(|tr| Some(tr.latest.slot) != world.own.map(|o| o.slot))
            .map(|tr| {
                let s = tr.sample(t, &world.bodies);
                (tr.latest, s.pos, s.vel)
            })
            .collect();
        seen.sort_by(|a, b| a.1.distance_squared(from).total_cmp(&b.1.distance_squared(from)));
        for (n, (e, p, vel)) in seen.into_iter().take(SUITS).enumerate() {
            let wreck = e.flags & ent_flags::WRECK != 0;
            let color = if wreck {
                GREY
            } else if e.faction != world.faction {
                RED
            } else {
                GREEN
            };
            let s = px(p) * 6.0;
            let hot = glow(color, 1.6, 0.95 * sector);
            if wreck {
                for (a, b) in [
                    (right + up, right - up),
                    (right - up, -right - up),
                    (-right - up, -right + up),
                    (-right + up, right + up),
                ] {
                    lines.line(p + a * s * 0.7, p + b * s * 0.7, hot);
                }
            } else {
                for (a, b) in [(up, right), (right, -up), (-up, -right), (-right, up)] {
                    lines.line(p + a * s, p + b * s, hot);
                }
                if vel.length() > 2.0 {
                    dashes.line(p, p + vel * SUIT_AHEAD_S, glow(color, 1.0, 0.55 * sector));
                }
            }
            stalk(&mut lines, p, color);
            let place = Place::Suit(e.slot, e.generation);
            pickable(place, p, 9.0, 2);
            if n < 6 || c.selected == Some(place) {
                // A Doll is "MD" but for Zodiac's ace, which goes by its name.
                let tag = match (e.pilot, world.roster.get(&e.slot)) {
                    (_, Some((name, _))) => name.to_uppercase(),
                    (PilotKind::MobileDoll, None) => "MD".to_string(),
                    _ => String::new(),
                };
                let what = if wreck { "WRECK".into() } else { tag };
                label(p + up * s * 1.6, format!("{what}  {}", range(p.distance(from))), color, 35);
            }
        }
        // Missiles in flight.
        for m in world.missiles.iter().flatten() {
            let p = m.pos_at(t);
            let s = px(p) * 3.0;
            lines.line(p - right * s, p + right * s, glow(RED, 2.0, sector));
            lines.line(p - up * s, p + up * s, glow(RED, 2.0, sector));
        }
        // The objective's waypoint.
        if settings.0.objectives
            && let Some((p, name)) = &objectives.waypoint
        {
            let s = px(*p) * 7.0;
            let col = glow(OBJECTIVE, 1.6, sector);
            for (a, b) in [(up, right), (right, -up), (-up, -right), (-right, up)] {
                bold.line(*p + a * s, *p + b * s, col);
            }
            stalk(&mut lines, *p, OBJECTIVE);
            label(*p + up * s * 1.5, format!("◆ {name}"), OBJECTIVE, 70);
        }
        // A point marked on the chart (selected, or the course's end).
        for place in [c.selected, target.place].into_iter().flatten() {
            if let Place::Point(p) = place {
                let s = px(p) * 6.0;
                let col = glow(COURSE, 1.6, sector);
                lines.circle(Isometry3d::new(p, Quat::from_rotation_arc(Vec3::Z, -v.fwd)), s, col);
                lines.line(p - up * s * 1.6, p + up * s * 1.6, col);
                stalk(&mut lines, p, COURSE);
                pickable(place, p, 8.0, 6);
                label(p + right * s * 1.6, "NAV POINT".into(), COURSE, 75);
            }
        }
        // The course: bold through its turns, with chevrons flowing along it to where it ends.
        if target.course.points.len() >= 2 {
            let col = glow(COURSE, 2.0, sector);
            bold.linestrip(target.course.points.iter().copied(), col);
            let step = px(c.cam.now.focus) * 38.0;
            let flow = (secs * 0.8).fract() * step;
            for (p, d) in target.course.marks_from(flow, step, 160) {
                let s = px(p) * 6.0;
                let side = d.cross(-v.fwd).normalize_or(right) * s;
                lines.line(p - d * s + side, p, col);
                lines.line(p - d * s - side, p, col);
            }
            if let Some(a) = target.arrival {
                let ring = Isometry3d::new(a.point, Quat::from_rotation_arc(Vec3::Z, -v.fwd));
                bold.circle(ring, px(a.point) * (8.0 + 3.0 * pulse), col).resolution(32);
            }
        }
        // The pilot's suit: a bright arrowhead along its way, where it's going, its stalk.
        if let Some(m) = own {
            let s = px(m.pos) * 9.0;
            let way = if m.vel.length() > 3.0 { m.vel.normalize() } else { m.rot * Vec3::Z };
            let flat = (way - v.fwd * way.dot(v.fwd)).normalize_or(up);
            let side = flat.cross(v.fwd).normalize_or(right);
            let (tip, l, r) = (
                m.pos + flat * s,
                m.pos - flat * s * 0.7 + side * s * 0.65,
                m.pos - flat * s * 0.7 - side * s * 0.65,
            );
            let col = glow(WHITE, 2.2, 1.0);
            bold.linestrip([tip, l, m.pos - flat * s * 0.3, r, tip], col);
            if m.vel.length() > 2.0 {
                dashes.line(m.pos, m.pos + m.vel * OWN_AHEAD_S, glow(WHITE, 1.2, 0.7));
            }
            stalk(&mut lines, m.pos, WHITE);
            let speed = m.vel.length();
            label(m.pos + up * s * 1.6, format!("YOU  {speed:.0} M/S"), WHITE, 90);
        }
    }

    // The Earth Sphere: the Moon's orbit, the Lagrange points, the lanes not yet open, the Sun.
    let sector_mark = 1.0 - sector;
    if sphere_a > 0.01 || sector_mark > 0.01 {
        let a = sphere_a;
        let (e, m) = (sphere::earth(), sphere::moon());
        let orbit =
            Isometry3d::new(sphere::barycentre(), Quat::from_rotation_arc(Vec3::Z, sphere::orbit_normal()));
        lines.circle(orbit, sphere::barycentre().distance(m), glow(CYAN, 0.8, 0.4 * a)).resolution(192);
        dashes.line(Lagrange::L3.pos(), Lagrange::L2.pos(), glow(CYAN, 0.6, 0.25 * a));
        for l in [Lagrange::L4, Lagrange::L5] {
            dashes.line(e, l.pos(), glow(CYAN, 0.5, 0.18 * a));
            dashes.line(m, l.pos(), glow(CYAN, 0.5, 0.18 * a));
            // The lanes the Cluster's expeditions will open.
            dashes.line(Vec3::ZERO, l.pos(), glow(AMBER, 0.8, 0.25 * a));
        }
        dashes.line(Vec3::ZERO, Lagrange::L2.pos(), glow(AMBER, 0.8, 0.25 * a));
        for l in Lagrange::ALL {
            let p = l.pos();
            let s = px(p) * 7.0;
            let col = glow(
                if l == Lagrange::L1 { COURSE } else { AMBER },
                1.6,
                a.max(if l == Lagrange::L1 { sector_mark } else { 0.0 }),
            );
            for (x, y) in [(up, right), (right, -up), (-up, -right), (-right, up)] {
                lines.line(p + x * s, p + y * s, col);
            }
            let name = if l == Lagrange::L1 {
                "L1  ·  SECTOR L1, THE FIRST COLONY".to_string()
            } else {
                format!("{}  ·  UNOPENED", l.name())
            };
            if a > 0.3 || l == Lagrange::L1 && sector_mark > 0.3 {
                label(
                    p + up * s * 1.5,
                    name,
                    if l == Lagrange::L1 { COURSE } else { AMBER },
                    if l == Lagrange::L1 { 95 } else { 72 },
                );
            }
            pickable(Place::Lagrange(l), p, 10.0, 4);
        }
        // The Sun's way, from L1.
        let sun_len = sphere::L1_TO_EARTH * 0.4;
        lines
            .arrow(Vec3::ZERO, sphere::SUN_DIR * sun_len, glow(Color::srgb(1.0, 0.85, 0.5), 1.4, 0.6 * a))
            .with_tip_length(sun_len * 0.06);
        if a > 0.3 {
            label(
                sphere::SUN_DIR * sun_len * 1.02,
                "TO THE SUN  ·  1 AU".into(),
                Color::srgb(1.0, 0.85, 0.5),
                60,
            );
        }
    }
    // Earth and the Moon can always be picked (they're there at every scale, if small).
    let e_px = sphere::EARTH_RADIUS / px(sphere::earth());
    let m_px = sphere::MOON_RADIUS / px(sphere::moon());
    pickable(Place::Earth, sphere::earth(), e_px.max(8.0), 6);
    pickable(Place::Moon, sphere::moon(), m_px.max(8.0), 6);
    // Too small to see from far off: ringed, so they can be found.
    for (c, r_px) in [(sphere::earth(), e_px), (sphere::moon(), m_px)] {
        if r_px < 6.0 && sphere_a > 0.01 {
            let face = Quat::from_rotation_arc(Vec3::Z, -v.fwd);
            lines
                .circle(Isometry3d::new(c, face), px(c) * 7.0, glow(CYAN, 1.2, 0.7 * sphere_a))
                .resolution(24);
        }
    }
    if e_px > 1.5 || sphere_a > 0.3 {
        label(
            sphere::earth() + up * (sphere::EARTH_RADIUS + 10.0 * px(sphere::earth())),
            "EARTH".into(),
            CYAN,
            90,
        );
    }
    if m_px > 1.5 || sphere_a > 0.3 {
        label(
            sphere::moon() + up * (sphere::MOON_RADIUS + 10.0 * px(sphere::moon())),
            "THE MOON".into(),
            CYAN,
            88,
        );
    }
    if let Some(s) = v.project(v.eye + sphere::SUN_DIR * 1e9).filter(|s| v.on_screen(*s, 0.0)) {
        picks.push((Place::Sun, Pickable { at: s, radius: 10.0, rank: 2 }));
    }
    // The selection, bracketed; the hover, ringed.
    let mark = |b: &mut Gizmos<ChartBold>, l: &mut Gizmos<ChartLines>, place: Place, sel: bool| {
        let Some(at) = place.locate(world, t) else { return };
        let p = at.pos;
        if v.project(p).is_none() {
            return;
        }
        let r_px = (at.reach / px(p)).clamp(10.0, 160.0) + 8.0;
        let r = r_px * px(p);
        let face = Quat::from_rotation_arc(Vec3::Z, -v.fwd);
        if sel {
            let k = r * (1.0 + 0.08 * pulse);
            let col = glow(WHITE, 1.8, 1.0);
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)] {
                let corner = p + (right * sx + up * sy) * k;
                b.line(corner, corner - right * sx * k * 0.35, col);
                b.line(corner, corner - up * sy * k * 0.35, col);
            }
        } else {
            l.circle(Isometry3d::new(p, face), r, glow(WHITE, 1.2, 0.6)).resolution(40);
        }
    };
    if let Some(p) = c.hover.filter(|h| Some(*h) != c.selected) {
        mark(&mut bold, &mut lines, p, false);
    }
    if let Some(p) = c.selected {
        mark(&mut bold, &mut lines, p, true);
    }
    c.picks = picks;
    c.labels = labels;
}

/// The twelve edges of the cube ±`l`.
fn cube_edges(l: f32) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for a in [-l, l] {
        for b in [-l, l] {
            out.push((Vec3::new(-l, a, b), Vec3::new(l, a, b)));
            out.push((Vec3::new(a, -l, b), Vec3::new(a, l, b)));
            out.push((Vec3::new(a, b, -l), Vec3::new(a, b, l)));
        }
    }
    out
}

/// The chart's words: its labels placed (without piling up), the scale, the course, the
/// selection's card, the objectives, the hover's tag.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn chart_panels(
    open: Res<MapOpen>,
    chart: Res<Chart>,
    target: Res<NavTarget>,
    objectives: Res<ObjectiveState>,
    settings: Res<SettingsRes>,
    game: NonSend<GameClient>,
    mut root: Query<
        &mut Visibility,
        (With<ChartUi>, Without<ChartLabel>, Without<ChartCard>, Without<ChartText>),
    >,
    mut card: Query<&mut Node, (With<ChartCard>, Without<ChartLabel>, Without<ChartUi>, Without<ChartText>)>,
    mut pool: Query<
        (&mut Node, &mut Text, &mut TextColor, &mut Visibility),
        (With<ChartLabel>, Without<ChartText>),
    >,
    mut texts: Query<(&ChartText, &mut Text, &mut Node, &mut Visibility), Without<ChartLabel>>,
    mut buttons: Query<(&ChartButton, &Interaction, &mut BackgroundColor)>,
) {
    let want = if open.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(want);
    }
    if !open.0 {
        return;
    }
    let g = game.borrow();
    let core = &g.core;
    let world = &core.world;
    let t = core.render_tick(now_s());
    let own = me(core);
    let c = &*chart;
    // The labels, highest rank first, none on another.
    let boxes: Vec<Label> = c
        .labels
        .iter()
        .map(|w| Label {
            at: w.at + Vec2::new(8.0, -7.0),
            size: Vec2::new(w.text.chars().count() as f32 * 7.4, 14.0),
            rank: w.rank,
        })
        .collect();
    let shown = declutter(&boxes, 3.0);
    let mut chosen = c.labels.iter().zip(shown).filter(|(_, s)| *s).map(|(w, _)| w);
    for (mut node, mut text, mut color, mut vis) in &mut pool {
        match chosen.next() {
            Some(w) => {
                node.left = Val::Px(w.at.x + 8.0);
                node.top = Val::Px(w.at.y - 7.0);
                if text.0 != w.text {
                    text.0.clone_from(&w.text);
                }
                color.0 = w.color;
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    let survival = core.welcome.is_some_and(|w| w.survival);
    let real = core.welcome.is_some_and(|w| !w.anime);
    // What's selected: its card.
    let sel = c.selected.and_then(|p| Some((p, p.locate(world, t)?)));
    for mut n in &mut card {
        let want = if sel.is_some() { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    let facts = |p: Place, at: &nav::Located| -> String {
        let Some(m) = own else { return format!("RANGE FROM L1  {}", range(at.pos.length())) };
        let d = at.pos.distance(m.pos);
        if p.beyond() {
            return format!("RANGE  {}\nNO LANE FROM L1 YET", range(d));
        }
        let to = (at.pos - m.pos).normalize_or(Vec3::Z);
        let closing = (m.vel - at.vel).dot(to);
        // (Never "-0".)
        let closing = if closing.abs() < 0.5 { 0.0 } else { closing };
        let up = at.pos.y - m.pos.y;
        let above = if up.abs() < 200.0 {
            "LEVEL WITH YOU".to_string()
        } else if up > 0.0 {
            format!("{} ABOVE YOU", range(up))
        } else {
            format!("{} BELOW YOU", range(-up))
        };
        let spec = core.world.own.map(|o| frame(o.frame));
        let mut s = format!("RANGE  {}   {above}\nCLOSING  {closing:.0} M/S", range(d));
        if let (Some(spec), Some(o)) = (spec, core.world.own) {
            let course = p.arrival(world, t, m.pos).map(|a| nav::plot(&world.bodies, t, m.pos, a.point));
            let left = course.as_ref().map_or(d, |c| c.length());
            // On the thrust it has: a dry tank crawls on an ion drive, or goes nowhere.
            let brake = nav::planned_braking(spec, core.predict.mods(), o.propellant, left);
            let cruise = spec.fa_speed.min(nav::NAV_CRUISE);
            let eta = nav::eta(left, closing.max(0.0), cruise, brake);
            let eta = if eta.is_finite() { clock(eta) } else { "TANK DRY".to_string() };
            s += &format!("\nBY AUTO-NAV  {}  ·  {eta}", range(left));
            if real {
                let isp = bc_sim::tuning::own_tuning(&o).isp;
                let burn = nav::burn_estimate(spec, isp, o.propellant, left, cruise, brake);
                s += &format!("\nBURNS ABOUT  {burn:.0} KG OF {:.0}", o.propellant);
            }
            if course.is_some_and(|c| c.points.len() > 2) {
                s += "\nTHE WAY GOES ROUND WHAT'S IN BETWEEN";
            }
        }
        s
    };
    let status = match (&g.nav, target.place) {
        (Some(n), _) => {
            let left = target.course.length();
            format!("AUTO-NAV  ·  {}  ·  {} TO GO", n.place.name(world), range(left))
        }
        (None, Some(p)) => format!(
            "COURSE SET  ·  {}  ·  {}  ·  N ENGAGES THE AUTO-NAV",
            p.name(world),
            range(target.course.length())
        ),
        (None, None) => String::new(),
    };
    let status = match (status.is_empty(), core.world.own) {
        (_, Some(o)) if o.alive && o.flags & own_flags::DOCKED != 0 => format!("DOCKED\n{status}"),
        _ => status,
    };
    let hostiles = own.map_or(0, |m| {
        world
            .entities
            .iter()
            .flatten()
            .filter(|tr| tr.latest.faction != world.faction && tr.latest.flags & ent_flags::WRECK == 0)
            .filter(|tr| tr.sample(t, &world.bodies).pos.distance(m.pos) < 3_000.0)
            .count()
    });
    let mut warn = Vec::new();
    if let Some(o) = core.world.own.filter(|o| o.alive) {
        if o.flags & own_flags::MISSILE_INCOMING != 0 {
            warn.push("MISSILE INBOUND".to_string());
        } else if o.flags & (own_flags::MISSILE_LOCK | own_flags::LOCKED_ON) != 0 {
            warn.push("LOCK WARNING".to_string());
        }
    }
    if hostiles > 0 {
        warn.push(format!("{hostiles} HOSTILE WITHIN 3 KM"));
    }
    let status = if warn.is_empty() {
        status
    } else {
        format!("{status}\n{} · THE SECTOR DOESN'T PAUSE", warn.join(" · ")).trim_start().to_string()
    };
    let objectives_text =
        objectives_list(&objectives, settings.0.objectives_done, settings.0.dolls_downed, survival);
    let dist = c.cam.now.dist;
    let width = 2.0 * dist * (view::FOV * 0.5).tan() * c.view.size.x / c.view.size.y.max(1.0);
    let scale = format!(
        "VIEW  {} ACROSS   ·   {}",
        range(width),
        if dist > SECTOR_FADE.1 {
            "THE EARTH SPHERE"
        } else if dist > 60_000.0 {
            "SECTOR L1 AND BEYOND"
        } else {
            "THE SECTOR"
        }
    );
    for (which, mut text, mut node, mut vis) in &mut texts {
        let s = match which {
            ChartText::Scale => scale.clone(),
            ChartText::Status => status.clone(),
            ChartText::Objectives => objectives_text.clone(),
            ChartText::Name => sel.map_or(String::new(), |(p, _)| p.name(world)),
            ChartText::Kind => sel.map_or(String::new(), |(p, _)| p.kind().to_string()),
            ChartText::About => sel.map_or(String::new(), |(p, _)| p.about(world, survival)),
            ChartText::Facts => sel.map_or(String::new(), |(p, at)| facts(p, &at)),
            ChartText::Hover => {
                let tag = c.hover.filter(|h| Some(*h) != c.selected).and_then(|h| {
                    let at = h.locate(world, t)?;
                    let from = own.map_or(Vec3::ZERO, |m| m.pos);
                    Some(format!("{}  {}", h.name(world), range(at.pos.distance(from))))
                });
                match (tag, c.cursor) {
                    (Some(tag), Some(cur)) => {
                        node.left = Val::Px(cur.x + 16.0);
                        node.top = Val::Px(cur.y + 12.0);
                        vis.set_if_neq(Visibility::Inherited);
                        tag
                    }
                    _ => {
                        vis.set_if_neq(Visibility::Hidden);
                        String::new()
                    }
                }
            }
            ChartText::Help => continue,
        };
        if text.0 != s {
            text.0 = s;
        }
    }
    // The chips: lit while they're on, or hovered.
    let reachable = c.selected.is_some_and(|p| p.reachable());
    for (b, i, mut bg) in &mut buttons {
        let on = match *b {
            ChartButton::Go(p) => c.selected == Some(p),
            ChartButton::AutoNav => g.nav.is_some(),
            ChartButton::Plane => !c.plane_on_pilot,
            ChartButton::TopDown => c.cam.goal.pitch > 1.4,
            ChartButton::You => c.cam.tracking,
            _ => false,
        };
        let usable = match *b {
            ChartButton::SetCourse | ChartButton::AutoNav => {
                reachable || g.nav.is_some() && *b == ChartButton::AutoNav
            }
            ChartButton::Clear => target.place.is_some() || g.nav.is_some(),
            ChartButton::Go(Place::Landmark(k)) | ChartButton::Go(Place::HideSpot(k, _)) => {
                usize::from(k) < world.bodies.landmarks().len()
            }
            _ => true,
        };
        let want = if !usable {
            Color::srgba(0.2, 0.3, 0.4, 0.08)
        } else if on {
            CHIP_ON
        } else if *i != Interaction::None {
            CHIP_HOVER
        } else {
            CHIP
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// The objectives, one a line: done, current or not, with their progress.
fn objectives_list(state: &ObjectiveState, done: u32, downed: u32, survival: bool) -> String {
    use bc_client_core::objectives::Objective;
    let input = state.input();
    let mut list = String::new();
    for o in Objective::order(survival) {
        if !o.available(input) {
            continue;
        }
        let mark = if done & o.bit() != 0 {
            "[x]"
        } else if state.current == Some(*o) {
            "[>]"
        } else {
            "[ ]"
        };
        let progress = match o.progress(input, downed) {
            Some((a, b)) if done & o.bit() == 0 => {
                format!("  {a}/{b}{}", if *o == Objective::Mine { " KG" } else { "" })
            }
            _ => String::new(),
        };
        list.push_str(&format!("{mark} {}{progress}\n", o.title(survival)));
    }
    if state.current.is_none() {
        list.push_str("Every objective done. The sector is yours.");
    }
    list.trim_end().to_string()
}

/// The course's ◇ on the HUD: on where it ends, or at the edge of the view toward it, with its
/// range, the time to it and what to do about the speed.
#[derive(Component)]
pub struct NavMarker;
#[derive(Component)]
pub struct NavMarkerText;

fn setup_nav_marker(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            NavMarker,
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTransform { translation: Val2::percent(-50.0, -100.0), ..default() },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                NavMarkerText,
                Text::new(""),
                font.heading(11.0),
                TextColor(COURSE),
                SHADOW,
                TextLayout::justify(Justify::Center).with_no_wrap(),
            ));
            // A hollow diamond (the fonts have no ◇): a square's outline turned on its corner.
            p.spawn((
                Node {
                    width: Val::Px(11.0),
                    height: Val::Px(11.0),
                    border: UiRect::all(Val::Px(2.0)),
                    margin: UiRect::top(Val::Px(5.0)),
                    ..default()
                },
                BorderColor::all(COURSE),
                UiTransform { rotation: Rot2::degrees(45.0), ..default() },
            ));
        });
}

/// The course in flight: the ◇ on the HUD, and chevrons laid out in space along the way ahead.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn show_course(
    open: Res<MapOpen>,
    target: Res<NavTarget>,
    ui: Res<Ui>,
    indoors: Res<crate::hangar::Indoors>,
    game: NonSend<GameClient>,
    time: Res<VisTime>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut marker: Query<(&mut Node, &mut Visibility), With<NavMarker>>,
    mut text: Query<&mut Text, With<NavMarkerText>>,
    mut gizmos: Gizmos<CourseLines>,
) {
    let Ok((mut node, mut vis)) = marker.single_mut() else { return };
    let g = game.borrow();
    let core = &g.core;
    let own = me(core);
    let flying = ui.playing() && !indoors.0 && !ui.on_foot && !open.0;
    let (Some(m), Some(place), Some(end), true, Ok((cam, cam_tf))) =
        (own, target.place, target.course.end(), flying, camera.single())
    else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    // Chevrons along the next few kilometres, flowing toward the end, fading with distance.
    let step = 120.0;
    let flow = ((time.now * 0.6).fract() as f32) * step;
    let marks = target.course.marks_from(flow, step, 36);
    let eye = cam_tf.translation();
    for (p, d) in marks {
        let far = p.distance(eye);
        let fade = (1.0 - far / 4_500.0).clamp(0.0, 1.0) * (far / 60.0).min(1.0);
        if fade <= 0.01 {
            continue;
        }
        let s = (far * 0.012).clamp(3.0, 40.0);
        let side = d.cross(p - eye).normalize_or(Vec3::Y) * s;
        let col = glow(COURSE, 1.8, 0.85 * fade);
        gizmos.line(p - d * s + side, p, col);
        gizmos.line(p - d * s - side, p, col);
    }
    // The ring it ends in, while it's out ahead (not round the camera).
    if let Some(a) = target.arrival
        && (nav::ARRIVE_RANGE * 4.0..6_000.0).contains(&a.point.distance(eye))
    {
        let face = Quat::from_rotation_arc(Vec3::Z, (eye - a.point).normalize_or(Vec3::Z));
        gizmos
            .circle(Isometry3d::new(a.point, face), nav::ARRIVE_RANGE, glow(COURSE, 1.8, 0.9))
            .resolution(32);
    }
    // The ◇: on the end, or at the view's edge toward it.
    let Some(rect) = cam.logical_viewport_rect() else { return };
    let on_screen = cam.world_to_viewport(cam_tf, end).ok().filter(|v| rect.contains(*v));
    let spot = on_screen.unwrap_or_else(|| {
        let local = cam_tf.affine().inverse().transform_point3(end);
        let d = Vec2::new(local.x, -local.y).try_normalize().unwrap_or(Vec2::Y);
        let half = (rect.half_size() - Vec2::new(90.0, 50.0)).max(Vec2::ONE);
        let reach = (half.x / d.x.abs().max(1e-4)).min(half.y / d.y.abs().max(1e-4));
        rect.center() + d * reach
    });
    node.left = Val::Px(spot.x);
    node.top = Val::Px(spot.y + 9.0);
    let world = &core.world;
    let to = (end - m.pos).normalize_or(Vec3::Z);
    let pace = target.arrival.map_or(Vec3::ZERO, |a| a.vel);
    let closing = (m.vel - pace).dot(to);
    let left = target.course.length();
    let line2 = match (&g.nav, core.world.own) {
        (Some(_), _) => "AUTO-NAV".to_string(),
        (None, Some(o)) => {
            let spec = frame(o.frame);
            // On the thrust it has: a dry tank crawls on an ion drive, or goes nowhere.
            let brake = nav::planned_braking(spec, core.predict.mods(), o.propellant, left);
            let eta = nav::eta(left, closing.max(0.0), spec.fa_speed.min(nav::NAV_CRUISE), brake);
            if !eta.is_finite() {
                "TANK DRY".to_string()
            } else {
                let cue = match nav::cue(left, closing, brake) {
                    nav::Cue::Brake => "  BRAKE",
                    nav::Cue::Burn => "",
                    nav::Cue::Coast => "",
                };
                format!("{}{cue}", clock(eta))
            }
        }
        _ => String::new(),
    };
    let s = format!("{}  {}\n{line2}", place.name(world), range(left));
    if let Ok(mut t) = text.single_mut()
        && t.0 != s
    {
        t.0 = s;
    }
    vis.set_if_neq(Visibility::Inherited);
}
