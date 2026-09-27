//! `?showcase=<scene>`: offline, scripted scenes for building and reviewing visuals. No server; the
//! scene drives the same view model the network does ([`SuitDrive`], [`BeamFeed`], [`FxEvents`],
//! and in the chase scene the [`CameraTarget`]).
//!
//! Time runs on a fixed 60 Hz step from `?t=` (unless `?realtime=1`), so a screenshot after N frames
//! is the same on any machine; `?hold=N` stops the clock after N frames. Controls: drag to orbit, wheel to zoom, WASD/Space/C to move,
//! 1-9 camera presets, P to pause, F10 to cycle the graphics tier.

use bc_client_core::world::{ObjectMotion, ObjectTrack};
use bc_proto::snapshot::ent_flags;
use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, Part, Segment, WeaponKind};
use bc_sim::content::frame;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::field::{Field, Rock};
use bc_sim::world::{COLONY_CENTER, COLONY_RADIUS};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::camera::{MainCamera, follow, pilot_effects};
use crate::dev_hooks::DevStatus;
use crate::gfx::Gfx;
use crate::view::{
    BeamFeed, BeamView, CameraTarget, ChaseTarget, FxEvent, FxEvents, MissileFeed, MissileView, SuitDrive,
    VisTime,
};

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
    /// The sky: presets look at Earth, the Moon, the Sun and the galactic core.
    Sky,
    /// The pilot's view: the chase camera over Wing Zero through the field, and what the pilot's
    /// body puts on the picture. Every 20 s: a boost (2-5 s), two hits (6 s), a hard turn that
    /// greys out to a blackout (8-12.5 s), then ZERO (from 13 s) and its seizure (17-19 s).
    Chase,
    /// After a fight by a rock: hulks, limbs shot off and loose ore tumbling, and a Leo come to
    /// pick through them.
    Salvage,
    /// A Leo mining: every 1.5 s its saber cuts into a rock, which cracks as it's worked while
    /// chips of ore drift off, until it shatters (at 9.75 s of every 12) and grows back.
    Mining,
    /// The pilots' frames side by side, each at its signature, on a 6 s loop: a Leo's machine
    /// cannon, Wing Zero charging, Heavyarms' Full Open Attack (missile salvos from its pods),
    /// Deathscythe jamming and reaping, Sandrock's shotels and Cross Crusher, Shenlong's Dragon
    /// Fang and flamethrower, and Neo-Bird on full burn.
    Gundams,
}

impl Scene {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "lineup" => Some(Self::Lineup),
            "duel" => Some(Self::Duel),
            "colony" => Some(Self::Colony),
            "field" => Some(Self::Field),
            "salvage" => Some(Self::Salvage),
            "mining" => Some(Self::Mining),
            "sky" => Some(Self::Sky),
            "chase" | "pilot" => Some(Self::Chase),
            "gundams" => Some(Self::Gundams),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Lineup => "lineup",
            Self::Duel => "duel",
            Self::Colony => "colony",
            Self::Field => "field",
            Self::Sky => "sky",
            Self::Chase => "chase",
            Self::Salvage => "salvage",
            Self::Mining => "mining",
            Self::Gundams => "gundams",
        }
    }

    /// Camera presets 1..: orbit target, yaw, pitch (radians) and distance.
    fn presets(self) -> Vec<Orbit> {
        match self {
            Self::Gundams => {
                let at = |i: usize, up: f32| gundam_pos(i) + Vec3::new(0.0, up, 8.0);
                vec![
                    orbit(GUNDAMS + Vec3::new(0.0, 0.0, 15.0), 0.3, 0.12, 150.0),
                    // Heavyarms in Full Open, from ahead and below its salvos.
                    orbit(at(2, 2.0), 0.55, 0.05, 48.0),
                    orbit(at(3, 2.0), -0.6, 0.12, 40.0),
                    orbit(at(4, 0.0), 0.7, 0.1, 38.0),
                    // Shenlong from the side, to see the fang go out its 35 m.
                    orbit(gundam_pos(5) + Vec3::new(0.0, 0.0, 18.0), 1.35, 0.1, 55.0),
                    orbit(at(6, 0.0), 0.9, 0.25, 36.0),
                    orbit(at(1, 0.0), 0.4, 0.1, 40.0),
                ]
            }
            Self::Lineup => LINEUP_CAMS.to_vec(),
            Self::Duel => DUEL_CAMS.to_vec(),
            Self::Colony => COLONY_CAMS.to_vec(),
            Self::Field => FIELD_CAMS.to_vec(),
            // The chase camera places itself; this only seeds the orbit state.
            Self::Chase => vec![orbit(CHASE, 0.0, 0.3, 900.0)],
            Self::Salvage => {
                let c = salvage_site();
                vec![
                    // From below, so the sky is behind the wreckage (the colony is under it).
                    orbit(c + Vec3::new(4.0, 2.0, 0.0), 0.7, -0.22, 62.0),
                    orbit(c + Vec3::new(8.0, 4.0, -6.0), -2.2, -0.3, 30.0),
                    orbit(c, 2.9, -0.1, 160.0),
                ]
            }
            Self::Mining => {
                let field = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
                let (_, rock, side) = mining_site(&field);
                let (face, stance) = mining_stance(&rock, side);
                let across = side.cross(Vec3::Y).normalize();
                vec![
                    // Beside the Leo and a little below, so the sky is behind the cut (the colony is
                    // under the field).
                    orbit_from((face + stance) * 0.5, across + side * 0.5 - Vec3::Y * 0.4, 38.0),
                    // Further back, to see the rock whole as it shatters.
                    orbit_from(
                        rock.pos + side * rock.radius * 0.5,
                        across * 0.7 + side - Vec3::Y * 0.45,
                        65.0,
                    ),
                ]
            }
            Self::Sky => {
                let eye = Vec3::new(0.0, 2_000.0, 0.0);
                let core = crate::sky::GALAXY_NORMAL.cross(Vec3::Z).normalize();
                vec![
                    look(eye, crate::sky::EARTH_DIR),
                    look(eye, crate::sky::MOON_DIR),
                    look(eye, crate::sky::SUN_DIR),
                    look(eye, core),
                    look(eye, (crate::sky::EARTH_DIR + crate::sky::SUN_DIR).normalize()),
                ]
            }
        }
    }
}

const fn orbit(target: Vec3, yaw: f32, pitch: f32, dist: f32) -> Orbit {
    Orbit { target, yaw, pitch, dist }
}

/// An orbit about `target`, seen from along `from` (out of the target).
fn orbit_from(target: Vec3, from: Vec3, dist: f32) -> Orbit {
    let d = from.normalize();
    orbit(target, d.x.atan2(d.z), d.y.clamp(-1.0, 1.0).asin(), dist)
}

/// An orbit whose eye sits at `eye`, looking along `dir`.
fn look(eye: Vec3, dir: Vec3) -> Orbit {
    let back = -dir.normalize();
    let pitch = back.y.clamp(-1.0, 1.0).asin();
    let yaw = back.x.atan2(back.z);
    Orbit { target: eye + dir.normalize() * 1_000.0, yaw, pitch, dist: 1_000.0 }
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
const COLONY_CAMS: [Orbit; 5] = [
    orbit(COLONY_CENTER, 0.9, 0.35, 42_000.0),
    orbit(SQUAD_START, 2.2, 0.35, 190.0),
    orbit(Vec3::new(16_000.0, -4_200.0, 0.0), -1.8, 0.3, 14_000.0),
    orbit(Vec3::new(0.0, -700.0, 0.0), -0.4, -0.05, 6_000.0),
    // The dock, off the docking hub's mouth at the −X end.
    orbit(Vec3::new(DOCK_CENTER.x + 300.0, DOCK_CENTER.y, DOCK_CENTER.z), -0.6, 0.12, 1_100.0),
];
const FIELD_CAMS: [Orbit; 3] = [
    orbit(FIELD, 0.3, 0.1, 400.0),
    orbit(FIELD, 2.3, -0.3, 1_600.0),
    orbit(Vec3::new(0.0, 900.0, 0.0), 1.0, 0.6, 12_000.0),
];

const LINEUP: Vec3 = Vec3::new(0.0, 1_200.0, 0.0);
const GUNDAMS: Vec3 = Vec3::new(0.0, 1_300.0, 600.0);
/// The gundams scene's loop (s).
const GUNDAMS_CYCLE: f64 = 6.0;
const DUEL: Vec3 = Vec3::new(0.0, 1_500.0, 0.0);
const SQUAD_START: Vec3 = Vec3::new(-3_000.0, COLONY_CENTER.y + COLONY_RADIUS + 45.0, 0.0);
const FIELD: Vec3 = Vec3::new(2_600.0, 900.0, 1_400.0);
/// The chase scene's circle through the field, and its cycle (s).
const CHASE: Vec3 = Vec3::new(2_600.0, 1_050.0, 1_400.0);
const CHASE_RADIUS: f32 = 500.0;
const CHASE_CYCLE: f64 = 20.0;
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
    /// Stop the clock after this many frames (0: never).
    hold: u64,
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
    pub hold: u64,
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
            hold: self.hold,
            cam: presets[preset - 1],
            preset: preset as u32,
            suits: Vec::new(),
        })
        .add_systems(
            Startup,
            (spawn_showcase, (spawn_wreckage, spawn_chips).after(crate::rocks::setup_field)),
        )
        .add_systems(Update, (drift_wreckage, drift_chips).in_set(crate::view::Vis::Drive))
        .add_systems(
            Update,
            (advance_clock, controls, script, work_rock).chain().in_set(crate::view::Vis::Drive),
        )
        .add_systems(Update, overlay.in_set(crate::view::Vis::Camera));
        if self.scene == Scene::Chase {
            app.add_systems(Update, (follow, pilot_effects).chain().in_set(crate::view::Vis::Camera));
        } else {
            app.add_systems(Update, place_camera.in_set(crate::view::Vis::Camera));
        }
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
        Scene::Sky => vec![],
        Scene::Chase => vec![(WingZero, Faction::Colonies), (Leo, Faction::Oz), (Taurus, Faction::Oz)],
        Scene::Salvage => vec![(Leo, Faction::Colonies)],
        Scene::Mining => vec![(Leo, Faction::Colonies)],
        Scene::Gundams => vec![
            (Leo, Faction::Colonies),
            (WingZero, Faction::Colonies),
            (Heavyarms, Faction::Colonies),
            (Deathscythe, Faction::Colonies),
            (Sandrock, Faction::Colonies),
            (Shenlong, Faction::Colonies),
            (WingZeroBird, Faction::Colonies),
        ],
    }
}

/// Where each of the gundams scene's suits stands.
fn gundam_pos(i: usize) -> Vec3 {
    GUNDAMS + Vec3::new(-90.0 + 30.0 * i as f32, 0.0, 0.0)
}

/// Whether a strike of `weapon` begun every `period` s (at `phase`) is under way at `t`: the
/// simulation's windup and stroke, and its recovery.
fn striking(t: f64, period: f64, phase: f64, weapon: WeaponKind) -> bool {
    let d = bc_sim::content::weapon(weapon).melee.map_or(0.0, |m| f64::from(m.duration()) / 30.0);
    (t - phase).rem_euclid(period) < d
}

/// Heavyarms' scripted missiles: one leaves a pod every 0.2 s for the first 3 s of each loop, flies
/// out and up for 1.8 s, and bursts. (born, from, heading)
fn salvo(t: f64) -> impl Iterator<Item = (f64, Vec3, Vec3)> {
    let loop0 = (t / GUNDAMS_CYCLE).floor() * GUNDAMS_CYCLE;
    [loop0 - GUNDAMS_CYCLE, loop0].into_iter().flat_map(|l| {
        (0..15).map(move |k| {
            let born = l + 0.2 * k as f64;
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let from = gundam_pos(2) + Vec3::new(3.45 * side, 6.2, 1.2);
            let spread = (k as f32 * 1.7).sin() * 0.35;
            let heading = Vec3::new(0.25 * side + spread * 0.5, 0.35 + spread * 0.3, 1.0).normalize();
            (born, from, heading)
        })
    })
}

/// A scripted missile `age` s out: off the rail at 120 m/s, the motor pushing it to 600 m/s, curving
/// round toward +z.
fn missile_at(from: Vec3, heading: Vec3, age: f32) -> (Vec3, Vec3) {
    let dist = 120.0 * age + 135.0 * age * age;
    let bend = Vec3::new(-heading.x * 0.6, -heading.y * 0.3, 0.0) * age * age * 0.4;
    let dir = (heading + bend).normalize();
    (from + dir * dist, dir * (120.0 + 270.0 * age))
}

/// Where the salvage scene is: beside the biggest rock near the field scene.
fn salvage_site() -> Vec3 {
    let field = Field::generate(Field::DEFAULT_SEED, Field::DEFAULT_ROCKS);
    let rock = field
        .rocks()
        .iter()
        .filter(|r| r.radius > 20.0)
        .min_by(|a, b| a.pos.distance(FIELD).total_cmp(&b.pos.distance(FIELD)))
        .copied()
        .unwrap_or_default();
    rock.pos + Vec3::new(1.0, 0.25, 0.6).normalize() * (rock.radius + 45.0)
}

/// A piece of the salvage scene's wreckage, drifting and turning from where it starts.
#[derive(Component)]
struct Wreckage {
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    spin: Vec3,
}

/// The salvage scene's wreckage: (what, where from the site, drift, spin).
fn wreckage() -> Vec<(ChunkKind, u32, Vec3, Vec3, Vec3)> {
    use FrameId::*;
    let all: u8 = (1 << Part::COUNT) - 1;
    let without = |parts: &[Part]| parts.iter().fold(all, |m, p| m & !(1 << *p as u8));
    vec![
        (
            ChunkKind::Hulk { frame: Leo, faction: Faction::Oz, parts: without(&[Part::ArmR, Part::Head]) },
            6_000,
            Vec3::ZERO,
            Vec3::new(0.1, 0.0, -0.05),
            Vec3::new(0.05, 0.12, 0.03),
        ),
        (
            ChunkKind::Hulk { frame: Taurus, faction: Faction::Oz, parts: without(&[Part::Legs]) },
            4_800,
            Vec3::new(38.0, -9.0, 28.0),
            Vec3::new(-0.1, 0.05, 0.0),
            Vec3::new(0.1, -0.06, 0.08),
        ),
        (
            ChunkKind::Limb { frame: Leo, faction: Faction::Oz, part: Part::ArmR },
            570,
            Vec3::new(13.0, 5.0, -10.0),
            Vec3::new(0.6, 0.2, -0.2),
            Vec3::new(0.4, 0.9, 0.1),
        ),
        (
            ChunkKind::Limb { frame: Leo, faction: Faction::Oz, part: Part::Head },
            280,
            Vec3::new(5.0, 13.0, 5.0),
            Vec3::new(0.1, 0.3, 0.2),
            Vec3::new(1.1, 0.3, 0.5),
        ),
        (
            ChunkKind::Limb { frame: Taurus, faction: Faction::Oz, part: Part::Legs },
            1_170,
            Vec3::new(52.0, -22.0, 12.0),
            Vec3::new(0.2, -0.1, 0.1),
            Vec3::new(0.2, 0.3, 0.1),
        ),
        (
            ChunkKind::Ore { ore: 0 },
            2_400,
            Vec3::new(-14.0, -6.0, 18.0),
            Vec3::new(0.1, 0.0, 0.1),
            Vec3::new(0.1, 0.2, 0.0),
        ),
        (
            ChunkKind::Ore { ore: 1 },
            900,
            Vec3::new(-6.0, 9.0, 24.0),
            Vec3::new(0.0, 0.1, 0.1),
            Vec3::new(0.3, 0.1, 0.2),
        ),
        (
            ChunkKind::Ore { ore: 2 },
            500,
            Vec3::new(20.0, -3.0, -22.0),
            Vec3::new(-0.1, 0.0, 0.0),
            Vec3::new(0.2, 0.5, 0.1),
        ),
        (
            ChunkKind::Ore { ore: 3 },
            300,
            Vec3::new(26.0, 8.0, 6.0),
            Vec3::new(0.0, -0.1, 0.1),
            Vec3::new(0.6, 0.2, 0.4),
        ),
        (
            ChunkKind::Ore { ore: 0 },
            1_200,
            Vec3::new(-28.0, 3.0, -8.0),
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(0.1, 0.1, 0.3),
        ),
        (
            ChunkKind::Ore { ore: 1 },
            150,
            Vec3::new(-10.0, -12.0, -16.0),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.7, 0.4, 0.1),
        ),
    ]
}

/// The mining scene's cycle (s), when each stroke starts, how long after it the blade is in the
/// rock, and when the rock shatters (on the last stroke).
const MINING_CYCLE: f64 = 12.0;
const STROKES: [f64; 7] = [0.5, 2.0, 3.5, 5.0, 6.5, 8.0, 9.5];
const LAND: f64 = 0.25;
const MINING_BREAK: f64 = 9.75;

/// The mining scene: a rock about a suit's size near the field scene, and the side of it the Leo
/// works (out of the rock, toward the sun).
fn mining_site(field: &Field) -> (usize, Rock, Vec3) {
    let (i, rock) = field
        .rocks()
        .iter()
        .enumerate()
        .filter(|(_, r)| (14.0..24.0).contains(&r.radius))
        .min_by(|(_, a), (_, b)| a.pos.distance(FIELD).total_cmp(&b.pos.distance(FIELD)))
        .map_or((0, Rock::default()), |(i, r)| (i, *r));
    (i, rock, (crate::sky::SUN_DIR + Vec3::new(0.0, -0.2, 0.6)).normalize())
}

/// The face of the rock the Leo cuts, and where it holds station off it.
fn mining_stance(rock: &Rock, side: Vec3) -> (Vec3, Vec3) {
    let face = rock.surface(rock.pos + side * 1_000.0, 0.0);
    (face, face + side * 7.5)
}

/// A piece of ore the mining scene knocks loose: from `born` s into the cycle it drifts and turns
/// from where it starts, until the cycle ends.
#[derive(Component)]
struct Chip {
    born: f64,
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    spin: Vec3,
}

/// Builds the ore the mining scene's strokes chip off and its shattering scatters.
fn spawn_chips(
    mut commands: Commands,
    show: Res<Show>,
    field: Res<crate::rocks::VisField>,
    lib: Res<crate::model::SuitMeshLib>,
    rocks: Res<crate::rocks::RockMeshes>,
    surfaces: Res<crate::materials::Surfaces>,
) {
    if show.scene != Scene::Mining {
        return;
    }
    let (_, rock, side) = mining_site(&field.0);
    let (face, _) = mining_stance(&rock, side);
    let across = side.cross(Vec3::Y).normalize();
    let up = across.cross(side);
    let mut pieces = Vec::new();
    // A chip off each stroke but the last, from where the blade went in.
    for (k, s) in STROKES[..STROKES.len() - 1].iter().enumerate() {
        let a = k as f32 * 2.1;
        let off = across * a.cos() + up * a.sin();
        pieces.push((s + LAND, 200, face + off * 2.0 + side * 1.5, side * 2.5 + off * 1.2));
    }
    // What's left, scattered as it shatters.
    for k in 0..6 {
        let a = k as f32 * 1.05;
        let dir = (side * 0.8 + across * a.cos() + up * a.sin()).normalize();
        pieces.push((MINING_BREAK, 900, rock.pos + dir * rock.radius * 0.5, dir * 4.5));
    }
    for (k, (born, mass_kg, pos, vel)) in pieces.into_iter().enumerate() {
        let track = ObjectTrack {
            generation: 1,
            desc: ChunkDesc {
                kind: ChunkKind::Ore { ore: rock.ore },
                seed: (k as u8).wrapping_mul(37),
                mass_kg,
            },
            motion: ObjectMotion::Free(Segment::default()),
            prev: None,
        };
        let rot = Quat::from_euler(EulerRot::YXZ, k as f32 * 1.7, k as f32 * 0.9, k as f32 * 0.3);
        let tf = Transform::from_translation(pos).with_rotation(rot);
        let e = crate::salvage_vis::spawn_chunk(&mut commands, &track, tf, &lib, &rocks, &surfaces);
        let spin = Vec3::new(0.4 + 0.1 * k as f32, 0.7, 0.2);
        commands.entity(e).insert((Chip { born, pos, vel, rot, spin }, Visibility::Hidden));
    }
}

/// Shows the mining scene's ore once it's knocked loose, drifting on the scene clock.
fn drift_chips(vis: Res<VisTime>, mut chips: Query<(&Chip, &mut Transform, &mut Visibility)>) {
    let u = vis.now.rem_euclid(MINING_CYCLE);
    for (c, mut tf, mut v) in &mut chips {
        let age = (u - c.born) as f32;
        let want = if age >= 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        tf.translation = c.pos + c.vel * age.max(0.0);
        tf.rotation = Quat::from_scaled_axis(c.spin * age.max(0.0)) * c.rot;
    }
}

/// Works the mining scene's rock: cracked and worked out a stroke at a time, gone once it
/// shatters, whole again each cycle.
fn work_rock(
    show: Res<Show>,
    vis: Res<VisTime>,
    mut lods: Query<&mut crate::rocks::RockLod>,
    mut shown: Query<&mut Visibility>,
    mut tags: Query<&mut bevy::mesh::MeshTag>,
    mut field: ResMut<crate::rocks::VisField>,
) {
    if show.scene != Scene::Mining {
        return;
    }
    let (i, ..) = mining_site(&field.0);
    let u = vis.now.rem_euclid(MINING_CYCLE);
    let k = STROKES.iter().filter(|&&s| s + LAND <= u).count() as u8;
    let state = (u >= MINING_BREAK, 7u8.saturating_sub(k).max(1), 15u8.saturating_sub(2 * k));
    for mut lod in &mut lods {
        if usize::from(lod.id()) == i {
            crate::rocks::set_rock_state(&mut lod, state, &mut shown, &mut tags, &mut field);
        }
    }
}

/// Builds the salvage scene's wreckage with the game's own chunk visuals.
fn spawn_wreckage(
    mut commands: Commands,
    show: Res<Show>,
    lib: Res<crate::model::SuitMeshLib>,
    rocks: Res<crate::rocks::RockMeshes>,
    surfaces: Res<crate::materials::Surfaces>,
) {
    if show.scene != Scene::Salvage {
        return;
    }
    let site = salvage_site();
    for (i, (kind, mass_kg, at, vel, spin)) in wreckage().into_iter().enumerate() {
        let track = ObjectTrack {
            generation: 1,
            desc: ChunkDesc { kind, seed: (i as u8).wrapping_mul(53), mass_kg },
            motion: ObjectMotion::Free(Segment::default()),
            prev: None,
        };
        let rot = Quat::from_euler(EulerRot::YXZ, i as f32 * 1.3, i as f32 * 0.7, i as f32 * 0.4);
        let tf = Transform::from_translation(site + at).with_rotation(rot);
        let e = crate::salvage_vis::spawn_chunk(&mut commands, &track, tf, &lib, &rocks, &surfaces);
        commands.entity(e).insert(Wreckage { pos: site + at, vel, rot, spin });
    }
}

/// Drifts and turns the salvage scene's wreckage on the scene clock.
fn drift_wreckage(vis: Res<VisTime>, mut pieces: Query<(&Wreckage, &mut Transform)>) {
    let t = vis.now as f32;
    for (w, mut tf) in &mut pieces {
        tf.translation = w.pos + w.vel * t;
        tf.rotation = Quat::from_scaled_axis(w.spin * t) * w.rot;
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
                thrust: Vec3::ZERO,
                parts: [7; Part::COUNT],
                holding: None,
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
    let held = show.hold > 0 && show.frames >= show.hold;
    let dt = if show.paused || held {
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

/// 0 before `lo`, 1 after `hi`, easing between.
fn smooth(lo: f64, hi: f64, x: f64) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

/// The pilot's path in the chase scene: a wide circle through the field.
fn chase_pos(t: f64) -> Vec3 {
    let a = chase_angle(t) as f32;
    CHASE + Vec3::new(CHASE_RADIUS * a.cos(), 40.0 * (2.0 * a).sin(), CHASE_RADIUS * a.sin())
}

/// How far round the chase circle the pilot is by `t` (radians): 0.24 rad/s (120 m/s), plus a
/// boost from 2 s to 5 s of each cycle that eases off by 8 s.
fn chase_angle(t: f64) -> f64 {
    let extra = |u: f64| {
        if u < 2.0 {
            0.0
        } else if u < 5.0 {
            0.05 * (u - 2.0).powi(2)
        } else if u < 8.0 {
            0.45 + 0.3 * (u - 5.0) - 0.05 * (u - 5.0).powi(2)
        } else {
            0.9
        }
    };
    let cycles = (t / CHASE_CYCLE).floor();
    0.24 * t + cycles * extra(CHASE_CYCLE) + extra(t - cycles * CHASE_CYCLE)
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

#[allow(clippy::too_many_arguments)]
fn script(
    show: Res<Show>,
    vis: Res<VisTime>,
    mut suits: Query<&mut SuitDrive>,
    mut beams: ResMut<BeamFeed>,
    mut missiles: ResMut<MissileFeed>,
    mut events: ResMut<FxEvents>,
    mut target: ResMut<CameraTarget>,
    field: Res<crate::rocks::VisField>,
) {
    let t = vis.now;
    let crossed = |at: f64| at > show.prev && at <= t;
    beams.0.clear();
    missiles.0.clear();
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
                    // Each aims somewhere of its own, for the arms and heads to follow.
                    let a = (t * 0.5 + i as f64 * 1.3) as f32;
                    d.aim = d.rot * Vec3::new(0.45 * a.sin(), 0.3 * (a * 0.7).cos(), 1.0).normalize();
                    d.flags = f;
                    d.thrust = if f == ent_flags::BOOST { Vec3::Z } else { Vec3::ZERO };
                    // The Alliance Leo has lost its left arm and is badly hurt; the wreck is gone.
                    d.parts = match i {
                        3 => [5, 3, 0, 4, 2, 7],
                        6 => [0; Part::COUNT],
                        _ => [7; Part::COUNT],
                    };
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
                    d.thrust = if dead { Vec3::ZERO } else { Vec3::new(0.3, 0.0, 0.7) };
                    // The Taurus is worn down until it dies; the Leo is hurt by the Twin Buster.
                    let u = t % 10.0;
                    d.parts = match i {
                        2 => {
                            let k = u.min(6.0) as u8;
                            [7, 7 - k, 7, 7 - k / 2, 7 - k / 2, 7]
                        }
                        1 if u >= 2.05 => [7, 5, 7, 3, 6, 7],
                        _ => [7; Part::COUNT],
                    };
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
                if crossed(s.t) {
                    events.0.push(FxEvent::Muzzle {
                        pos: muzzle,
                        dir,
                        vel: Vec3::ZERO,
                        weapon: s.weapon,
                        shooter: Some(s.shooter as u16),
                    });
                }
                if crossed(s.t + flight as f64) {
                    events.0.push(FxEvent::Hit {
                        pos: duel_pos(s.target, t),
                        weapon: s.weapon,
                        normal: None,
                        target: Some((s.target as u16, Part::Torso)),
                    });
                }
            }
            if crossed((t / 10.0).floor() * 10.0 + 7.0) {
                events.0.push(FxEvent::Kill { pos: duel_pos(2, t) });
            }
            // Blades crossing, for the effect (the duellists stay far apart).
            if crossed((t / 10.0).floor() * 10.0 + 4.5) {
                events.0.push(FxEvent::Clash { pos: DUEL + Vec3::new(0.0, 30.0, 0.0) });
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
                    d.thrust = Vec3::new(0.0, 0.0, 0.8);
                });
            }
        }
        Scene::Sky => {}
        Scene::Chase => {
            let u = t % CHASE_CYCLE;
            let cycle = t - u;
            let own = chase_pos(t);
            let vel = (chase_pos(t + 0.05) - chase_pos(t - 0.05)) * 10.0;
            let heading = vel.normalize_or(Vec3::Z);
            let boost = (2.0..5.0).contains(&u);
            let g = smooth(8.0, 10.5, u) * (1.0 - smooth(12.0, 14.0, u));
            // Banked into the turn, harder while pulling G.
            let rot = facing(heading) * Quat::from_rotation_z(-0.2 - 0.9 * g);
            set(0, &mut |d| {
                d.own = true;
                d.pos = own;
                d.vel = vel;
                d.rot = rot;
                d.aim = heading;
                d.flags = if boost { ent_flags::BOOST } else { 0 };
                d.thrust = if boost { Vec3::Z } else { Vec3::new(0.0, 0.6 * g, 0.3) };
                let torso = if u >= 6.45 {
                    5
                } else if u >= 6.05 {
                    6
                } else {
                    7
                };
                d.parts = [7, torso, 7, 7, 7, 7];
            });
            // A Leo ahead, turned back to fire at the pilot, and a Taurus crossing.
            let leo = chase_pos(t + 1.6) + Vec3::new(0.0, 25.0, 0.0);
            set(1, &mut |d| {
                d.pos = leo;
                d.vel = vel;
                d.rot = facing(own - leo);
                d.aim = (own - leo).normalize_or(Vec3::Z);
                d.flags = 0;
                d.thrust = Vec3::ZERO;
            });
            let a = (u * 0.15) as f32;
            set(2, &mut |d| {
                d.pos = CHASE + Vec3::new(-600.0 + 1_200.0 * a, 120.0, 200.0);
                d.vel = Vec3::new(180.0, 0.0, 0.0);
                d.rot = facing(Vec3::X);
                d.aim = Vec3::X;
                d.flags = ent_flags::BOOST;
                d.thrust = Vec3::Z;
            });
            // The Leo's two rifle shots strike home.
            for shot in [6.0, 6.4] {
                let fired = cycle + shot;
                let from = chase_pos(fired + 1.6) + Vec3::new(0.0, 25.0, 0.0);
                let to = chase_pos(fired);
                let dir = (to - from).normalize_or(Vec3::Z);
                let flight = (from.distance(to) / 4_000.0) as f64;
                let age = t - fired;
                if (0.0..=flight).contains(&age) {
                    let head = from + dir * 4_000.0 * age as f32;
                    beams.0.push(BeamView {
                        head,
                        dir,
                        travelled: head.distance(from),
                        weapon: WeaponKind::BeamRifle,
                    });
                }
                if crossed(fired) {
                    events.0.push(FxEvent::Muzzle {
                        pos: from,
                        dir,
                        vel,
                        weapon: WeaponKind::BeamRifle,
                        shooter: Some(1),
                    });
                }
                if crossed(fired + flight) {
                    events.0.push(FxEvent::Hit {
                        pos: own + Vec3::Y * 2.0,
                        weapon: WeaponKind::BeamRifle,
                        normal: Some((from - own).normalize_or(Vec3::Z)),
                        target: Some((0, Part::Torso)),
                    });
                    events.0.push(FxEvent::Struck { weapon: WeaponKind::BeamRifle });
                }
            }
            target.0 = Some(ChaseTarget {
                pos: own,
                vel,
                up: rot * Vec3::Y,
                aim: heading,
                boost,
                g_strain: g,
                blackout: (10.5..12.5).contains(&u),
                zero: u >= 13.0,
                zero_strain: smooth(13.0, 17.0, u),
                seized: (17.0..19.0).contains(&u),
            });
        }
        Scene::Salvage => {
            // The Leo hangs off the Leo hulk, looking it over.
            let site = salvage_site();
            let hulk = site + Vec3::new(0.1, 0.0, -0.05) * t as f32;
            set(0, &mut |d| {
                d.pos = site + Vec3::new(-24.0, 6.0, 14.0) + Vec3::new(0.3, 0.05, -0.1) * t as f32;
                d.vel = Vec3::new(0.3, 0.05, -0.1);
                d.rot = facing(hulk - d.pos);
                d.aim = d.rot * Vec3::Z;
                d.flags = 0;
                d.thrust = Vec3::new(0.0, 0.1, 0.0);
            });
        }
        Scene::Mining => {
            let (_, rock, side) = mining_site(&field.0);
            let (face, stance) = mining_stance(&rock, side);
            let u = t.rem_euclid(MINING_CYCLE);
            let swinging = STROKES.iter().any(|&s| (s..s + 0.6).contains(&u));
            set(0, &mut |d| {
                // Holding station off the face, bobbing a little.
                d.pos = stance + Vec3::Y * (0.3 * (t * 0.8).sin()) as f32;
                d.vel = Vec3::ZERO;
                d.rot = facing(face - stance);
                d.aim = d.rot * Vec3::Z;
                d.flags = if swinging { ent_flags::SABER } else { 0 };
                d.thrust = Vec3::new(0.0, 0.0, 0.1);
            });
            if crossed((t / MINING_CYCLE).floor() * MINING_CYCLE + MINING_BREAK) {
                let ore = crate::materials::ore_colour(usize::from(rock.ore));
                events.0.push(FxEvent::RockBreak { pos: rock.pos, radius: rock.radius, ore });
            }
        }
        Scene::Gundams => {
            let u = t.rem_euclid(GUNDAMS_CYCLE);
            for i in 0..7 {
                let flags = match i {
                    0 => ent_flags::FIRING_SECONDARY,
                    1 => ent_flags::CHARGING,
                    // Full Open for half the loop, the gatling otherwise.
                    2 if u < 3.0 => {
                        ent_flags::SPECIAL | ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY
                    }
                    2 => ent_flags::FIRING_PRIMARY,
                    // Jamming (as its side sees it), and reaping every 2 s.
                    3 => {
                        let reap = striking(t, 2.0, 0.5, WeaponKind::BeamScythe);
                        ent_flags::SPECIAL | if reap { ent_flags::SABER } else { 0 }
                    }
                    // The shotels, and the Cross Crusher every other time.
                    4 if striking(t, 3.0, 0.2, WeaponKind::CrossCrusher)
                        && (t / 3.0).floor() as i64 % 2 == 1 =>
                    {
                        ent_flags::SABER | ent_flags::SPECIAL
                    }
                    4 if striking(t, 3.0, 0.2, WeaponKind::HeatShotel) => ent_flags::SABER,
                    // The fang, then the flame.
                    5 if striking(t, 3.0, 0.3, WeaponKind::DragonFang) => {
                        ent_flags::SABER | ent_flags::MELEE_ALT
                    }
                    5 if (1.6..2.8).contains(&t.rem_euclid(3.0)) => ent_flags::FIRING_SECONDARY,
                    6 => ent_flags::BOOST,
                    _ => 0,
                };
                set(i, &mut |d| {
                    let bird = d.frame == FrameId::WingZeroBird;
                    let bob = (t * 0.7 + i as f64).sin() as f32;
                    d.pos = gundam_pos(i) + Vec3::new(0.0, bob * 0.8, 0.0);
                    d.rot = if bird {
                        Quat::from_rotation_z(bob * 0.25) * Quat::from_rotation_x(-0.1)
                    } else {
                        Quat::from_rotation_y(bob * 0.08)
                    };
                    let a = (t * 0.5 + i as f64 * 1.3) as f32;
                    d.aim = d.rot * Vec3::new(0.2 * a.sin(), 0.12 * (a * 0.7).cos(), 1.0).normalize();
                    d.flags = flags;
                    d.thrust = if flags & ent_flags::BOOST != 0 { Vec3::Z } else { Vec3::ZERO };
                });
            }
            // Heavyarms' salvos.
            for (born, from, heading) in salvo(t) {
                let age = (t - born) as f32;
                if (0.0..1.8).contains(&age) {
                    let (pos, vel) = missile_at(from, heading, age);
                    missiles.0.push(MissileView {
                        pos,
                        vel,
                        kind: WeaponKind::HomingMissile,
                        targets_you: false,
                    });
                }
                if crossed(born + 1.8) {
                    let (pos, _) = missile_at(from, heading, 1.8);
                    events.0.push(FxEvent::MissileBurst {
                        pos,
                        kind: WeaponKind::HomingMissile,
                        struck: false,
                    });
                }
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
                    d.thrust = if i == 0 { Vec3::new(0.0, 0.0, 0.6) } else { Vec3::new(0.0, 0.3, 0.0) };
                });
            }
        }
    }
}
