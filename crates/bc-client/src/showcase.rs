//! `?showcase=<scene>`: offline, scripted scenes for building and reviewing visuals. No server; the
//! scene drives the same view model the network does ([`SuitDrive`], [`BeamFeed`], [`FxEvents`]).
//!
//! Time runs on a fixed 60 Hz step from `?t=` (unless `?realtime=1`), so a screenshot after N frames
//! is the same on any machine. Controls: drag to orbit, wheel to zoom, WASD/Space/C to move,
//! 1-9 camera presets, P to pause, F10 to cycle the graphics tier.

use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, WeaponKind};
use bc_sim::content::frame;
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::dev_hooks::DevStatus;
use crate::gfx::Gfx;
use crate::view::{BeamFeed, BeamView, FxEvent, FxEvents, SuitDrive, VisTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// Every frame and livery side by side, with thrusters, saber, charge, ZERO aura and a wreck.
    Lineup,
    /// Wing Zero and a Leo circling and trading fire; a Taurus is shot down on a loop.
    Duel,
    /// A Taurus squad skimming the colony hull.
    Colony,
    /// Inside the debris field.
    Field,
}

impl Scene {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "lineup" => Some(Self::Lineup),
            "duel" => Some(Self::Duel),
            "colony" => Some(Self::Colony),
            "field" | "salvage" => Some(Self::Field),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Lineup => "lineup",
            Self::Duel => "duel",
            Self::Colony => "colony",
            Self::Field => "field",
        }
    }

    /// Camera presets 1..: orbit target, yaw, pitch (radians) and distance.
    fn presets(self) -> &'static [Orbit] {
        match self {
            Self::Lineup => &LINEUP_CAMS,
            Self::Duel => &DUEL_CAMS,
            Self::Colony => &COLONY_CAMS,
            Self::Field => &FIELD_CAMS,
        }
    }
}

const fn orbit(target: Vec3, yaw: f32, pitch: f32, dist: f32) -> Orbit {
    Orbit { target, yaw, pitch, dist }
}

const LINEUP_CAMS: [Orbit; 5] = [
    orbit(LINEUP, 0.25, 0.08, 95.0),
    orbit(Vec3::new(-70.0, 1_203.0, 0.0), 0.6, 0.05, 32.0),
    orbit(Vec3::new(-23.4, 1_203.0, 0.0), -0.5, 0.2, 36.0),
    orbit(Vec3::new(70.0, 1_203.0, 0.0), -2.6, 0.1, 40.0),
    orbit(LINEUP, 3.0, 0.35, 170.0),
];
const DUEL_CAMS: [Orbit; 3] =
    [orbit(DUEL, 0.8, 0.3, 380.0), orbit(DUEL, -1.2, 0.1, 260.0), orbit(DUEL, 2.4, -0.25, 320.0)];
const COLONY_CAMS: [Orbit; 4] = [
    orbit(COLONY_CENTER, 0.9, 0.35, 42_000.0),
    orbit(SQUAD_START, 2.2, 0.15, 260.0),
    orbit(Vec3::new(16_000.0, -4_200.0, 0.0), -1.8, 0.3, 14_000.0),
    orbit(Vec3::new(0.0, -700.0, 0.0), -0.4, -0.05, 6_000.0),
];
const FIELD_CAMS: [Orbit; 3] = [
    orbit(FIELD, 0.3, 0.1, 400.0),
    orbit(FIELD, 2.3, -0.3, 1_600.0),
    orbit(Vec3::new(0.0, 900.0, 0.0), 1.0, 0.6, 12_000.0),
];

const LINEUP: Vec3 = Vec3::new(0.0, 1_200.0, 0.0);
const DUEL: Vec3 = Vec3::new(0.0, 1_500.0, 0.0);
const SQUAD_START: Vec3 = Vec3::new(-3_000.0, COLONY_CENTER.y + COLONY_RADIUS + 300.0, 0.0);
const FIELD: Vec3 = Vec3::new(2_600.0, 900.0, 1_400.0);
const STEP: f64 = 1.0 / 60.0;

/// An orbit camera: looks at `target` from `dist` away.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    dist: f32,
}

impl Orbit {
    fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * sy, sp, cp * cy) * self.dist
    }
}

#[derive(Resource)]
struct Show {
    scene: Scene,
    t0: f64,
    /// Scene time last frame (events fire when their time falls in (prev, now]).
    prev: f64,
    frames: u64,
    realtime: bool,
    paused: bool,
    cam: Orbit,
    preset: u32,
    suits: Vec<Entity>,
}

#[derive(Component)]
struct ShowcaseText;

pub struct ShowcasePlugin {
    pub scene: Scene,
    pub t0: f64,
    pub cam: u32,
    pub realtime: bool,
}

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        let presets = self.scene.presets();
        let preset = (self.cam as usize).clamp(1, presets.len());
        app.insert_resource(Show {
            scene: self.scene,
            t0: self.t0,
            prev: self.t0,
            frames: 0,
            realtime: self.realtime,
            paused: false,
            cam: presets[preset - 1],
            preset: preset as u32,
            suits: Vec::new(),
        })
        .add_systems(Startup, spawn_showcase)
        .add_systems(Update, (advance_clock, controls, script).chain().in_set(crate::view::Vis::Drive))
        .add_systems(Update, (place_camera, overlay).chain().in_set(crate::view::Vis::Camera));
    }
}

/// The cast of each scene, in script order.
fn cast(scene: Scene) -> Vec<(FrameId, Faction)> {
    use FrameId::*;
    match scene {
        Scene::Lineup => vec![
            (WingZero, Faction::Colonies),
            (Leo, Faction::Oz),
            (Leo, Faction::Colonies),
            (Leo, Faction::Alliance),
            (Taurus, Faction::Oz),
            (Virgo, Faction::Oz),
            (Leo, Faction::Oz),
        ],
        Scene::Duel => vec![(WingZero, Faction::Colonies), (Leo, Faction::Oz), (Taurus, Faction::Oz)],
        Scene::Colony => vec![(Taurus, Faction::Oz), (Taurus, Faction::Oz), (Taurus, Faction::Oz)],
        Scene::Field => vec![(Leo, Faction::Oz), (Leo, Faction::Colonies)],
    }
}

fn spawn_showcase(mut commands: Commands, mut show: ResMut<Show>) {
    let suits = cast(show.scene)
        .into_iter()
        .enumerate()
        .map(|(i, (frame, faction))| {
            let d = SuitDrive {
                slot: i as u16,
                frame,
                faction,
                generation: 1,
                own: false,
                pos: LINEUP,
                rot: Quat::IDENTITY,
                vel: Vec3::ZERO,
                aim: Vec3::Z,
                flags: 0,
            };
            commands.spawn((d, Transform::from_translation(LINEUP), Visibility::default())).id()
        })
        .collect();
    show.suits = suits;
    commands.spawn((
        ShowcaseText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(13.0), ..default() },
        TextColor(Color::srgba(0.7, 0.92, 1.0, 0.85)),
        Node { position_type: PositionType::Absolute, left: Val::Px(14.0), top: Val::Px(10.0), ..default() },
    ));
}

fn advance_clock(
    mut show: ResMut<Show>,
    mut vis: ResMut<VisTime>,
    real: Res<Time<Real>>,
    mut dev: ResMut<DevStatus>,
) {
    let dt = if show.paused {
        0.0
    } else if show.realtime {
        real.delta_secs_f64()
    } else {
        STEP
    };
    if show.frames == 0 {
        // The first frame shows `?t=` exactly, and fires events due at it.
        vis.now = show.t0;
        show.prev = show.t0 - STEP;
    } else {
        show.prev = vis.now;
        vis.now += dt;
    }
    vis.dt = dt as f32;
    show.frames += 1;
    dev.set("mode", "showcase");
    dev.set("scene", show.scene.name());
    dev.set("showcase_t", vis.now);
    dev.set("showcase_frames", show.frames as f64);
}

fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    real: Res<Time<Real>>,
    mut show: ResMut<Show>,
) {
    let presets = show.scene.presets();
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, k) in digits.iter().enumerate() {
        if keys.just_pressed(*k) && i < presets.len() {
            show.cam = presets[i];
            show.preset = i as u32 + 1;
        }
    }
    if keys.just_pressed(KeyCode::KeyP) {
        show.paused = !show.paused;
    }
    let cam = &mut show.cam;
    if mouse.pressed(MouseButton::Left) && motion.delta != Vec2::ZERO {
        cam.yaw -= motion.delta.x * 0.005;
        cam.pitch = (cam.pitch + motion.delta.y * 0.005).clamp(-1.5, 1.5);
    }
    if scroll.delta.y != 0.0 {
        cam.dist = (cam.dist * 0.9f32.powf(scroll.delta.y.signum())).clamp(5.0, 200_000.0);
    }
    let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
    let fwd = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
    let right = Vec3::new(cam.yaw.cos(), 0.0, -cam.yaw.sin());
    let mv = fwd * axis(KeyCode::KeyW, KeyCode::KeyS)
        + right * axis(KeyCode::KeyD, KeyCode::KeyA)
        + Vec3::Y * axis(KeyCode::Space, KeyCode::KeyC);
    if mv != Vec3::ZERO {
        let boost = if keys.pressed(KeyCode::ShiftLeft) { 5.0 } else { 1.0 };
        cam.target += mv * cam.dist * 0.6 * boost * real.delta_secs();
    }
}

fn place_camera(show: Res<Show>, mut cam: Query<&mut Transform, With<MainCamera>>) {
    if let Ok(mut tf) = cam.single_mut() {
        *tf = Transform::from_translation(show.cam.eye()).looking_at(show.cam.target, Vec3::Y);
    }
}

fn overlay(
    show: Res<Show>,
    vis: Res<VisTime>,
    gfx: Res<Gfx>,
    mut text: Query<&mut Text, With<ShowcaseText>>,
) {
    if let Ok(mut t) = text.single_mut() {
        **t = format!(
            "SHOWCASE {}  t {:.1}s  cam {}  {}{}\ndrag orbit | wheel zoom | WASD Space C move | 1-9 cameras | P pause | F10 tier",
            show.scene.name(),
            vis.now,
            show.preset,
            gfx.tier.name().to_uppercase(),
            if show.paused { "  PAUSED" } else { "" },
        );
    }
}

/// Faces `rot` toward `dir` with +Y roughly up.
fn facing(dir: Vec3) -> Quat {
    Transform::IDENTITY.looking_to(-dir.normalize_or(Vec3::Z), Vec3::Y).rotation
}

/// A shot of the scripted duel.
struct Shot {
    /// Scene time it leaves the muzzle.
    t: f64,
    shooter: usize,
    target: usize,
    weapon: WeaponKind,
}

impl Shot {
    fn speed(&self) -> f32 {
        match self.weapon {
            WeaponKind::TwinBusterRifle => 8_000.0,
            _ => 4_000.0,
        }
    }
}

/// Duel positions: Wing Zero and the Leo on opposite sides of a slowly turning circle, a Taurus
/// crossing through; all as pure functions of time.
fn duel_pos(i: usize, t: f64) -> Vec3 {
    let w = 0.25 * t as f32;
    match i {
        0 => DUEL + Vec3::new(250.0 * w.cos(), 60.0 * (2.0 * w).sin(), 250.0 * w.sin()),
        1 => DUEL + Vec3::new(-250.0 * w.cos(), -60.0 * (2.0 * w).sin(), -250.0 * w.sin()),
        _ => {
            let u = (t % 10.0) as f32;
            DUEL + Vec3::new(-500.0 + 90.0 * u, 140.0, 120.0)
        }
    }
}

fn duel_shots(t: f64) -> impl Iterator<Item = Shot> {
    // The Leo's rifle every 1.2 s at Wing Zero; Wing Zero's Twin Buster every 5 s at the Taurus
    // (which dies at 7 s of every 10), otherwise at the Leo.
    let rifle =
        (0..).map(|k| Shot { t: 0.3 + k as f64 * 1.2, shooter: 1, target: 0, weapon: WeaponKind::BeamRifle });
    let buster = (0..).map(|k| {
        let t = 2.0 + k as f64 * 5.0;
        let target = if (t % 10.0 - 7.0).abs() < 0.01 { 2 } else { 1 };
        Shot { t, shooter: 0, target, weapon: WeaponKind::TwinBusterRifle }
    });
    rifle
        .take_while(move |s| s.t <= t)
        .chain(buster.take_while(move |s| s.t <= t))
        .filter(move |s| t - s.t < 1.5)
}

fn script(
    show: Res<Show>,
    vis: Res<VisTime>,
    mut suits: Query<&mut SuitDrive>,
    mut beams: ResMut<BeamFeed>,
    mut events: ResMut<FxEvents>,
) {
    let t = vis.now;
    let crossed = |at: f64| at > show.prev && at <= t;
    beams.0.clear();
    let mut set = |i: usize, f: &mut dyn FnMut(&mut SuitDrive)| {
        if let Some(mut d) = show.suits.get(i).and_then(|e| suits.get_mut(*e).ok()) {
            f(&mut d);
        }
    };
    match show.scene {
        Scene::Lineup => {
            let flags = [
                ent_flags::CHARGING,
                ent_flags::BOOST,
                ent_flags::SABER,
                ent_flags::FIRING_SECONDARY,
                ent_flags::ZERO,
                ent_flags::BOOST,
                ent_flags::WRECK,
            ];
            for (i, f) in flags.into_iter().enumerate() {
                set(i, &mut |d| {
                    let wreck = f == ent_flags::WRECK;
                    d.pos = if wreck {
                        LINEUP + Vec3::new(0.0, 4.0, -45.0)
                    } else {
                        LINEUP + Vec3::new(-70.0 + 23.3 * i as f32, 0.0, 0.0)
                    };
                    let sway = (t * 0.4 + i as f64).sin() as f32 * 0.15;
                    d.rot = if wreck {
                        Quat::from_rotation_x(t as f32 * 0.7) * Quat::from_rotation_z(t as f32 * 0.23)
                    } else {
                        Quat::from_rotation_y(sway)
                    };
                    d.aim = d.rot * Vec3::Z;
                    d.flags = f;
                });
            }
        }
        Scene::Duel => {
            for i in 0..3 {
                let p = duel_pos(i, t);
                let vel = (duel_pos(i, t + 0.05) - duel_pos(i, t - 0.05)) * 10.0;
                let foe = duel_pos(if i == 0 { 1 } else { 0 }, t);
                let dead = i == 2 && (7.0..10.0).contains(&(t % 10.0));
                let charging = i == 0 && duel_shots(t + 0.6).any(|s| s.shooter == 0 && s.t > t);
                let cannon = i == 1 && (t % 4.0) >= 2.0 && (t % 4.0) < 3.0;
                set(i, &mut |d| {
                    d.pos = p;
                    d.vel = vel;
                    d.aim = (foe - p).normalize_or(Vec3::Z);
                    d.rot = if i == 2 { facing(vel) } else { facing(foe - p) };
                    d.flags = if dead {
                        ent_flags::WRECK
                    } else {
                        let mut f = ent_flags::BOOST;
                        if charging {
                            f |= ent_flags::CHARGING;
                        }
                        if cannon {
                            f |= ent_flags::FIRING_SECONDARY;
                        }
                        f
                    };
                    if dead {
                        let since = (t % 10.0 - 7.0) as f32;
                        d.rot *= Quat::from_rotation_x(since * 0.7);
                    }
                });
            }
            for s in duel_shots(t) {
                let spec = frame(if s.shooter == 0 { FrameId::WingZero } else { FrameId::Leo });
                let from = duel_pos(s.shooter, s.t);
                let to = duel_pos(s.target, s.t);
                let muzzle =
                    from + facing(to - from) * spec.loadout[0].map_or(Vec3::ZERO, |m| m.arm.muzzle());
                let dir = (to - muzzle).normalize_or(Vec3::Z);
                let flight = muzzle.distance(to) / s.speed();
                let age = (t - s.t) as f32;
                if age <= flight {
                    let head = muzzle + dir * s.speed() * age;
                    beams.0.push(BeamView { head, dir, travelled: head.distance(muzzle), weapon: s.weapon });
                }
                if crossed(s.t + flight as f64) {
                    events.0.push(FxEvent::Hit { pos: duel_pos(s.target, t), weapon: s.weapon });
                }
            }
            if crossed((t / 10.0).floor() * 10.0 + 7.0) {
                events.0.push(FxEvent::Kill { pos: duel_pos(2, t) });
            }
        }
        Scene::Colony => {
            // A squad circling above the hull, in echelon.
            let a = 0.35 * t as f32;
            let heading = Vec3::new(-a.sin(), 0.0, a.cos());
            for i in 0..3 {
                let off = Vec3::new(-30.0 * i as f32, 18.0 * (i % 2) as f32, 26.0 * i as f32 - 26.0);
                set(i, &mut |d| {
                    d.pos = SQUAD_START + Vec3::new(120.0 * a.cos(), 0.0, 120.0 * a.sin()) + off;
                    d.vel = heading * 42.0;
                    d.rot = facing(heading);
                    d.aim = heading;
                    d.flags = ent_flags::BOOST;
                });
            }
        }
        Scene::Field => {
            for i in 0..2 {
                let a = 0.08 * t as f32 + i as f32 * 2.8;
                set(i, &mut |d| {
                    d.pos = FIELD + Vec3::new(60.0 * a.cos(), 10.0 * i as f32, 60.0 * a.sin());
                    d.vel = Vec3::new(-a.sin(), 0.0, a.cos()) * 4.8;
                    d.rot = facing(d.vel) * Quat::from_rotation_z(0.2);
                    d.aim = d.rot * Vec3::Z;
                    d.flags = if i == 0 { ent_flags::BOOST } else { 0 };
                });
            }
        }
    }
}
