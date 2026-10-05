//! The Proving Ground's course, flown inside the colony (`bc_sim::colony::course`,
//! `docs/TRAINING.md`): its rings of light in the colony's air and its pad on Hub Gate's square, drawn
//! on the city's layer for pilots flying inside and for those on foot watching from the city; the
//! run's clock (`bc_client_core::course`), kept on the pilot's predicted suit; and what the HUD says
//! about it (the objectives' panel and waypoint, `map.rs`, show it while flying inside).
//!
//! The next ring burns bright and pulses, the rest of the course glows dim ahead of it, and the
//! rings flown go out. Not running, the start ring by the inner gate burns bright.
//!
//! In the Blast Hall (`bc_sim::colony::hall`) weapons are free: its targets hang in its air where
//! the tick has them (on everyone's screen), flash white when a training round scores on one, and
//! the panel counts the pilot's.

use bc_client_core::course::{Class, Event, Run, clock};
use bc_sim::bodies::Body;
use bc_sim::colony::course::{GATES, PAD_RADIUS, centre, on_pad, pad_centre, way};
use bc_sim::colony::frame::up_at;
use bc_sim::ground::Footing;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::city::{CITY_LAYER, Placed};
use crate::net::{GameClient, now_s};
use crate::page::{Ui, UiCmd, UiCmds};
use crate::settings::SettingsRes;

pub struct CoursePlugin;

impl Plugin for CoursePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CourseState>().add_systems(Startup, setup_course).add_systems(
            Update,
            (run_course, draw_targets).after(crate::view::Vis::Suits).before(crate::view::Vis::Camera),
        );
    }
}

/// How long the HUD says how the last run went, s.
const FLOWN_FOR: f64 = 15.0;
/// The rings' tubes, m thick each side of their line.
const TUBE: f32 = 1.2;

/// The run, and what the HUD shows of it.
#[derive(Resource, Default)]
pub struct CourseState {
    run: Run,
    /// The last run flown: its time (s), whether it was the best yet, and when (the page's clock).
    flown: Option<(f64, bool, f64)>,
    /// The panel's lines while flying inside (its heading, its title and how), and the waypoint:
    /// the next ring, or the pad (none when not running: the inner gate's is shown).
    pub lines: Option<[String; 3]>,
    pub waypoint: Waypoint,
}

/// Where the HUD's waypoint is, and what it's called.
type Waypoint = Option<(Vec3, String)>;

/// A ring of the course (its index in `GATES`), or the pad (`GATES.len()`).
#[derive(Component)]
struct CourseRing(usize);

/// The rings' looks: the next (pulsing), those ahead, and the pad's.
#[derive(Resource)]
struct RingLooks {
    next: Handle<StandardMaterial>,
    ahead: Handle<StandardMaterial>,
}

/// The next ring's glow, at the top of its pulse.
const NEXT_GLOW: [f32; 3] = [1.6, 7.0, 8.0];

/// One of the Blast Hall's targets (its index in `hall`'s).
#[derive(Component)]
struct HallTarget(usize);

/// The targets' looks: waiting, and struck.
#[derive(Resource)]
struct TargetLooks {
    idle: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
}

/// How long a target flashes once struck, ticks.
const FLASH_TICKS: f64 = 12.0;

fn lit(materials: &mut Assets<StandardMaterial>, base: Color, glow: [f32; 3]) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: base,
        emissive: LinearRgba::rgb(glow[0], glow[1], glow[2]),
        unlit: true,
        ..default()
    })
}

fn setup_course(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let next = lit(&mut materials, Color::srgb(0.6, 0.95, 1.0), NEXT_GLOW);
    let ahead = lit(&mut materials, Color::srgb(0.35, 0.6, 0.7), [0.25, 0.9, 1.1]);
    // A ring stands across its way; the pad lies on the square, a metre up.
    let up = up_at(pad_centre());
    let pad = (GATES.len(), PAD_RADIUS, pad_centre() + up, up);
    let gates = GATES.iter().enumerate().map(|(i, g)| (i, g.radius, centre(i), way(i)));
    for (i, r, at, axis) in gates.chain(std::iter::once(pad)) {
        let ring = meshes.add(Mesh::from(Torus::new(r - TUBE, r + TUBE)));
        commands.spawn((
            CourseRing(i),
            Mesh3d(ring),
            MeshMaterial3d(ahead.clone()),
            // A torus lies round its Y axis.
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, axis)),
            Placed(at.as_dvec3()),
            RenderLayers::layer(CITY_LAYER),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(RingLooks { next, ahead });
    // The hall's targets: holograms of a torso's size, amber, with a ring round each.
    let idle = lit(&mut materials, Color::srgb(1.0, 0.55, 0.15), [6.0, 2.4, 0.4]);
    let hit = lit(&mut materials, Color::WHITE, [14.0, 14.0, 14.0]);
    let r = bc_sim::colony::hall::TARGET_RADIUS;
    let ball = meshes.add(Sphere::new(r * 0.55).mesh().ico(3).expect("icosphere"));
    let ring = meshes.add(Mesh::from(Torus::new(r - 0.25, r)));
    for i in 0..bc_sim::colony::hall::TARGETS {
        commands
            .spawn((
                HallTarget(i),
                Mesh3d(ball.clone()),
                MeshMaterial3d(idle.clone()),
                Transform::default(),
                Placed(bc_sim::colony::hall::target(i, 0, 0.0).as_dvec3()),
                RenderLayers::layer(CITY_LAYER),
                Visibility::Hidden,
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(ring.clone()),
                    MeshMaterial3d(idle.clone()),
                    Transform::from_rotation(Quat::from_rotation_x(core::f32::consts::FRAC_PI_2)),
                    RenderLayers::layer(CITY_LAYER),
                ));
            });
    }
    commands.insert_resource(TargetLooks { idle, hit });
}

/// The Blast Hall's targets where the tick has them (the render clock flying inside, the plaza's
/// on foot), flashing when struck, for anyone in the city.
fn draw_targets(
    game: Option<NonSend<GameClient>>,
    looks: Option<Res<TargetLooks>>,
    mut targets: Query<(&HallTarget, &mut Placed, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let (Some(game), Some(looks)) = (game, looks) else {
        for (_, _, mut v, _) in &mut targets {
            v.set_if_neq(Visibility::Hidden);
        }
        return;
    };
    let g = game.borrow();
    let core = &g.core;
    let shown = core.inside() || core.hangar.in_city();
    let now = now_s();
    let t = if core.inside() {
        core.render_tick(now)
    } else {
        let (t, frac) = core.colony_tick(now);
        f64::from(t) + f64::from(frac)
    };
    let (k, frac) = (t.max(0.0).floor(), (t - t.max(0.0).floor()) as f32);
    for (target, mut placed, mut vis, mut mat) in &mut targets {
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if !shown {
            continue;
        }
        placed.0 = bc_sim::colony::hall::target(target.0, k as u32, frac).as_dvec3();
        let struck = core.world.target_hits.iter().any(|h| {
            usize::from(h.target) == target.0
                && (t - f64::from(h.tick)) < FLASH_TICKS
                && t >= f64::from(h.tick)
        });
        let look = if struck { &looks.hit } else { &looks.idle };
        if mat.0 != *look {
            mat.0 = look.clone();
        }
    }
}

/// Steps the run on the pilot's suit flying inside the colony, says how it goes, keeps the best
/// time, and lights the rings.
#[allow(clippy::too_many_arguments)]
fn run_course(
    game: Option<NonSend<GameClient>>,
    mut state: ResMut<CourseState>,
    mut settings: Option<ResMut<SettingsRes>>,
    mut ui: Option<ResMut<Ui>>,
    mut cmds: Option<ResMut<UiCmds>>,
    looks: Option<Res<RingLooks>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rings: Query<(&CourseRing, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
    dev: Option<ResMut<crate::dev_hooks::DevStatus>>,
) {
    let Some(looks) = looks else { return };
    // A showcase has no game, and no course.
    let Some(game) = game else {
        for (_, mut v, _) in &mut rings {
            v.set_if_neq(Visibility::Hidden);
        }
        return;
    };
    let g = game.borrow();
    let core = &g.core;
    let inside = core.inside();
    let watching = core.hangar.in_city();
    let now = now_s();
    let view = core.own_view().filter(|v| v.alive).filter(|_| inside);
    let mut events = None;
    match view {
        Some(v) => {
            let feet = crate::hud::footed(core);
            let standing = feet.footing == Footing::Grounded && feet.body == Body::City;
            events = state.run.step(v.pos, v.t, standing && on_pad(v.pos));
        }
        // Out of the colony, or between suits: whatever comes next starts afresh.
        None => state.run = Run::default(),
    }
    let best = settings.as_ref().map_or(0, |s| s.0.course_best_ms);
    match events {
        Some(Event::Started) => {
            if let Some(ui) = ui.as_mut() {
                ui.toast("PROVING GROUND · THE CLOCK IS RUNNING");
            }
            chime(cmds.as_deref_mut());
        }
        Some(Event::Gate(_)) => chime(cmds.as_deref_mut()),
        Some(Event::Finished(secs)) => {
            let ms = (secs * 1_000.0).round() as u32;
            let new_best = best == 0 || ms < best;
            if new_best && let Some(s) = settings.as_mut() {
                s.0.course_best_ms = ms;
            }
            if let Some(ui) = ui.as_mut() {
                let mark = if new_best && best != 0 { " · NEW BEST" } else { "" };
                ui.news(format!("COURSE FLOWN · {} · {}{mark}", clock(secs), Class::of(secs).name()), false);
            }
            chime(cmds.as_deref_mut());
            state.flown = Some((secs, new_best, now));
        }
        Some(Event::Lapsed) => {
            if let Some(ui) = ui.as_mut() {
                ui.toast("PROVING GROUND · THE RUN LAPSED: FLY THE START RING TO GO AGAIN");
            }
        }
        None => {}
    }
    let best = settings.as_ref().map_or(0, |s| s.0.course_best_ms);
    let from = view.map(|v| v.pos);
    let (lines, waypoint) = match from.filter(|p| bc_sim::colony::hall::in_hall(*p)) {
        // In the Blast Hall, not flying the course: its live fire.
        Some(_) if !state.run.running() => (hall_lines(core.world.my_target_hits), None),
        _ => hud(&state, best, from, view.map_or(0.0, |v| v.t), now),
    };
    state.lines = inside.then_some(lines);
    state.waypoint = waypoint.filter(|_| inside);
    if let Some(mut dev) = dev {
        dev.set("course_next", state.run.next().map_or(-1, |n| n as i32));
        dev.set("course_best_ms", best);
        dev.set("in_hall", from.is_some_and(bc_sim::colony::hall::in_hall));
        dev.set("hall_targets", core.world.my_target_hits);
    }

    // The rings: on the city's layer for anyone in it, the next one pulsing.
    let next = state.run.next().unwrap_or(0);
    let shown = inside || watching;
    for (ring, mut vis, mut mat) in &mut rings {
        let i = ring.0;
        let want = if !shown || i < next { Visibility::Hidden } else { Visibility::Inherited };
        vis.set_if_neq(want);
        let look = if i == next { &looks.next } else { &looks.ahead };
        if mat.0 != *look {
            mat.0 = look.clone();
        }
    }
    if shown && let Some(mut m) = materials.get_mut(&looks.next) {
        let k = 0.55 + 0.45 * (0.5 + 0.5 * (now * 5.0).sin() as f32);
        m.emissive = LinearRgba::rgb(NEXT_GLOW[0] * k, NEXT_GLOW[1] * k, NEXT_GLOW[2] * k);
    }
}

/// A ring flown: the cockpit's confirming chime.
fn chime(cmds: Option<&mut UiCmds>) {
    if let Some(c) = cmds {
        c.0.push(UiCmd::Sfx("confirm".into()));
    }
}

/// The panel's lines and the waypoint, for the run as it stands: flying it, just flown, or not yet
/// started. `from` is where the suit is, `t` its clock (ticks), `now` the page's.
fn hud(state: &CourseState, best_ms: u32, from: Option<Vec3>, t: f64, now: f64) -> ([String; 3], Waypoint) {
    let n = GATES.len();
    let range = |p: Vec3| from.map_or(String::new(), |f| format!("   {}", metres(p.distance(f))));
    let best = if best_ms == 0 {
        "NOT YET FLOWN".to_string()
    } else {
        let secs = f64::from(best_ms) / 1_000.0;
        format!("BEST {} {}", clock(secs), Class::of(secs).name())
    };
    if let Some(next) = state.run.next() {
        let elapsed = clock(state.run.elapsed(t).unwrap_or(0.0));
        if next < n {
            // The stretch is named by the last ring that names one.
            let stretch = GATES[..=next].iter().rev().find(|g| !g.name.is_empty()).map_or("", |g| g.name);
            let at = centre(next);
            return (
                [
                    format!("PROVING GROUND   RING {}/{n}   {best}", next + 1),
                    format!("{stretch}   {elapsed}{}", range(at)),
                    "Through the lit ring. Flying the start ring again starts the clock over.".into(),
                ],
                Some((at, format!("RING {}", next + 1))),
            );
        }
        let at = pad_centre();
        return (
            [
                format!("PROVING GROUND   THE PAD   {best}"),
                format!("LAND ON THE PAD   {elapsed}{}", range(at)),
                "Come to rest over the lit pad on Hub Gate's square and arm the grip (L): the clock stops when you stand on it."
                    .into(),
            ],
            Some((at, "PAD".into())),
        );
    }
    if let Some((secs, new_best, at)) = state.flown
        && now - at < FLOWN_FOR
    {
        let mark = if new_best { "   NEW BEST" } else { "" };
        return (
            [
                "PROVING GROUND   COURSE FLOWN".into(),
                format!("{}   {}{mark}", clock(secs), Class::of(secs).name()),
                "The Charter Board's flight certificate. Fly the start ring to go again, or dock: at rest in the inner gate's ring, Enter."
                    .into(),
            ],
            None,
        );
    }
    (
        [
            format!("PROVING GROUND   {best}"),
            format!("FLY THE START RING{}", range(centre(0))),
            format!(
                "The Charter Board's course for new pilots: {n} lit rings in order, down to Hub Gate's square, and land on its pad. Par is {}. At rest in the inner gate's ring, Enter docks.",
                par()
            ),
        ],
        None,
    )
}

/// The panel in the Blast Hall: weapons free, and the pilot's targets struck.
fn hall_lines(hits: u32) -> [String; 3] {
    [
        "PROVING GROUND   THE BLAST HALL".into(),
        format!("WEAPONS FREE   TARGETS {hits}"),
        "Training rounds: they score on the hall's targets and never touch a suit. Out through the blast doors, weapons are safe again."
            .into(),
    ]
}

fn metres(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

/// The par, in whole minutes and seconds: `2:00`.
fn par() -> String {
    let s = bc_client_core::course::PAR_S.round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}
