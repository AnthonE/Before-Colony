//! The Proving Ground inside the colony (`docs/TRAINING.md`), drawn on the city's layer for pilots
//! flying inside and for those on foot watching from the city, and what the HUD says of it (the
//! objectives' panel and waypoint, `map.rs`, show it while flying inside):
//! - **The course** (`bc_sim::colony::course`): its rings of light in the colony's air and its pad
//!   on Hub Gate's square. The next ring burns bright and pulses, the rest of the course glows dim
//!   ahead of it, and the rings flown go out; not running, the start ring by the inner gate burns
//!   bright. The run's clock (`bc_client_core::course`) is stepped with the predicted suit tick by
//!   tick, as the server's sector steps its own: the time it reads is the time the board keeps.
//! - **The Blast Hall** (`bc_sim::colony::hall`), where weapons are free: its targets hang in its
//!   air where the tick has them (on everyone's screen) and flash white when a training round
//!   scores on one. The pilot's drill (kept in the world, fed their own strikes as the sector feeds
//!   its own) lights its next target for them, and the panel runs its clock.
//! - **The gantry**: a pad of light on the hall's floor where the Charter Board's trainers stand,
//!   bright for a pilot flying one (it docks there).

use bc_client_core::course::{Class, Event, Run, clock};
use bc_sim::bodies::Body;
use bc_sim::colony::city::KERB;
use bc_sim::colony::course::{GATES, PAD_RADIUS, centre, on_pad, pad_centre, way};
use bc_sim::colony::frame::up_at;
use bc_sim::colony::hall::{self, DRILL, DRILL_BONUS_S, DRILL_PAR_S, Drill, DrillEvent, GANTRY_RADIUS};
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

/// How long the HUD says how the last run (or drill) went, s.
const FLOWN_FOR: f64 = 15.0;
/// The rings' tubes, m thick each side of their line.
const TUBE: f32 = 1.2;

/// How a drill ended.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drilled {
    /// Cleared, in so long (s).
    Cleared(f64),
    /// The clock ran out with so many struck.
    Out(usize),
}

/// The run, and what the HUD shows of it.
#[derive(Resource, Default)]
pub struct CourseState {
    run: Run,
    /// The newest predicted tick the run has been stepped with.
    stepped: Option<u32>,
    /// The last run flown: its time (s), whether it was the best yet, and when (the page's clock).
    flown: Option<(f64, bool, f64)>,
    /// How the last drill ended, whether it was the best yet, and when.
    drilled: Option<(Drilled, bool, f64)>,
    /// The panel's lines while flying inside (its heading, its title and how), and the waypoint:
    /// the next ring, or the pad (none when not running: the inner gate's, or a trainer's gantry, is
    /// shown).
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

/// The targets' looks: waiting, struck, and lit for the pilot's drill (pulsing).
#[derive(Resource)]
struct TargetLooks {
    idle: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
    lit: Handle<StandardMaterial>,
}

/// The lit target's glow, at the top of its pulse.
const LIT_GLOW: [f32; 3] = [3.0, 12.0, 14.0];

/// The trainers' gantry's pad of light on the hall's floor: dim, and bright for a trainer's pilot.
#[derive(Component)]
struct GantryPad;

#[derive(Resource)]
struct GantryLooks {
    dim: Handle<StandardMaterial>,
    bright: Handle<StandardMaterial>,
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
    let lit_look = lit(&mut materials, Color::srgb(0.7, 1.0, 1.0), LIT_GLOW);
    let r = hall::TARGET_RADIUS;
    let ball = meshes.add(Sphere::new(r * 0.55).mesh().ico(3).expect("icosphere"));
    let ring = meshes.add(Mesh::from(Torus::new(r - 0.25, r)));
    for i in 0..hall::TARGETS {
        commands
            .spawn((
                HallTarget(i),
                Mesh3d(ball.clone()),
                MeshMaterial3d(idle.clone()),
                Transform::default(),
                Placed(hall::target(i, 0, 0.0).as_dvec3()),
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
    commands.insert_resource(TargetLooks { idle, hit, lit: lit_look });
    // The gantry: two rings of light on the hall's floor, round where a trainer stands (flattened
    // to strips a few centimetres high, just over the floor, which is the kerb's height).
    let dim = lit(&mut materials, Color::srgb(0.7, 0.45, 0.1), [1.2, 0.7, 0.1]);
    let bright = lit(&mut materials, Color::srgb(1.0, 0.85, 0.4), [8.0, 5.0, 1.0]);
    let up = up_at(hall::gantry());
    let rings = [(GANTRY_RADIUS, 0.4), (GANTRY_RADIUS * 0.45, 0.25)];
    for (r, tube) in rings {
        commands.spawn((
            GantryPad,
            Mesh3d(meshes.add(Mesh::from(Torus::new(r - tube, r + tube)))),
            MeshMaterial3d(dim.clone()),
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, up))
                .with_scale(Vec3::new(1.0, 0.2, 1.0)),
            Placed((hall::gantry() + up * (KERB + 0.1)).as_dvec3()),
            RenderLayers::layer(CITY_LAYER),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(GantryLooks { dim, bright });
}

/// The Blast Hall's targets where the tick has them (the render clock flying inside, the plaza's
/// on foot), flashing when struck, for anyone in the city; the target the pilot's drill has lit,
/// pulsing, for them; and the gantry's pad, bright for a trainer's pilot.
#[allow(clippy::type_complexity)]
fn draw_targets(
    game: Option<NonSend<GameClient>>,
    looks: Option<Res<TargetLooks>>,
    gantry_looks: Option<Res<GantryLooks>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut targets: Query<
        (&HallTarget, &mut Placed, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        Without<GantryPad>,
    >,
    mut pads: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<GantryPad>>,
) {
    let (Some(game), Some(looks), Some(gantry_looks)) = (game, looks, gantry_looks) else {
        for (_, _, mut v, _) in &mut targets {
            v.set_if_neq(Visibility::Hidden);
        }
        for (mut v, _) in &mut pads {
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
    // The pilot's drill lights its next target for them: in the hall, or once it's running.
    let drill = &core.world.drill;
    let in_hall = core.own_view().is_some_and(|v| hall::in_hall(v.pos));
    let lit_now = (core.inside() && (drill.running() || in_hall)).then(|| usize::from(drill.lit()));
    if lit_now.is_some()
        && let Some(mut m) = materials.get_mut(&looks.lit)
    {
        // (Unlit, what's drawn is the base colour: it pulses.)
        let k = 0.45 + 0.55 * (0.5 + 0.5 * (now * 6.0).sin() as f32);
        m.base_color = Color::linear_rgb(0.55 * k, k, k);
        m.emissive = LinearRgba::rgb(LIT_GLOW[0] * k, LIT_GLOW[1] * k, LIT_GLOW[2] * k);
    }
    let (k, frac) = (t.max(0.0).floor(), (t - t.max(0.0).floor()) as f32);
    for (target, mut placed, mut vis, mut mat) in &mut targets {
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if !shown {
            continue;
        }
        placed.0 = hall::target(target.0, k as u32, frac).as_dvec3();
        let struck = core.world.target_hits.iter().any(|h| {
            usize::from(h.target) == target.0
                && (t - f64::from(h.tick)) < FLASH_TICKS
                && t >= f64::from(h.tick)
        });
        let look = if struck {
            &looks.hit
        } else if lit_now == Some(target.0) {
            &looks.lit
        } else {
            &looks.idle
        };
        if mat.0 != *look {
            mat.0 = look.clone();
        }
    }
    let look = if core.inside() && core.hangar.trainer { &gantry_looks.bright } else { &gantry_looks.dim };
    for (mut vis, mut mat) in &mut pads {
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if mat.0 != *look {
            mat.0 = look.clone();
        }
    }
}

/// Steps the run on the pilot's suit flying inside the colony, says how it and the drill go, keeps
/// the best times, and lights the rings.
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
    let mut g = game.borrow_mut();
    let core = &mut g.core;
    let inside = core.inside();
    let watching = core.hangar.in_city();
    let now = now_s();
    let view = core.own_view().filter(|v| v.alive).filter(|_| inside).map(|v| (v.pos, v.t));
    let mut events = Vec::new();
    match view {
        // Tick by tick as the prediction flew them, as the server's sector steps its own run.
        Some(_) => {
            let p = &core.predict;
            let from = state.stepped.map_or(p.tick, |t| t.saturating_add(1).max(p.tick.saturating_sub(120)));
            for t in from..=p.tick {
                if let Some(s) = p.sample(t) {
                    let standing = s.footing == Footing::Grounded && s.body == Body::City && on_pad(s.pos);
                    events.extend(state.run.step(s.pos, f64::from(t), standing));
                }
            }
            state.stepped = Some(p.tick);
        }
        // Out of the colony, or between suits: whatever comes next starts afresh.
        None => {
            state.run = Run::default();
            state.stepped = None;
        }
    }
    let best = settings.as_ref().map_or(0, |s| s.0.course_best_ms);
    for e in events {
        match e {
            Event::Started => {
                if let Some(ui) = ui.as_mut() {
                    ui.toast("PROVING GROUND · THE CLOCK IS RUNNING");
                }
                chime(cmds.as_deref_mut());
            }
            Event::Gate(_) => chime(cmds.as_deref_mut()),
            Event::Finished(secs) => {
                let ms = (secs * 1_000.0).round() as u32;
                let new_best = best == 0 || ms < best;
                if new_best && let Some(s) = settings.as_mut() {
                    s.0.course_best_ms = ms;
                }
                if let Some(ui) = ui.as_mut() {
                    let mark = if new_best && best != 0 { " · NEW BEST" } else { "" };
                    ui.news(
                        format!("COURSE FLOWN · {} · {}{mark}", clock(secs), Class::of(secs).name()),
                        false,
                    );
                }
                chime(cmds.as_deref_mut());
                state.flown = Some((secs, new_best, now));
            }
            Event::Lapsed => {
                if let Some(ui) = ui.as_mut() {
                    ui.toast("PROVING GROUND · THE RUN LAPSED: FLY THE START RING TO GO AGAIN");
                }
            }
        }
    }
    // The drill, as the world's strikes moved it.
    let drill_best = settings.as_ref().map_or(0, |s| s.0.drill_best_ms);
    for e in std::mem::take(&mut core.world.drill_news) {
        match e {
            DrillEvent::Started => {
                if let Some(ui) = ui.as_mut() {
                    ui.toast("THE DRILL · THE CLOCK IS RUNNING");
                }
                chime(cmds.as_deref_mut());
            }
            DrillEvent::Struck(_) => chime(cmds.as_deref_mut()),
            DrillEvent::Cleared(secs) => {
                let ms = (secs * 1_000.0).round() as u32;
                let new_best = drill_best == 0 || ms < drill_best;
                if new_best && let Some(s) = settings.as_mut() {
                    s.0.drill_best_ms = ms;
                }
                if let Some(ui) = ui.as_mut() {
                    let mark = if new_best && drill_best != 0 { " · NEW BEST" } else { "" };
                    let class = Class::against(secs, DRILL_PAR_S).name();
                    ui.news(format!("DRILL CLEARED · {} · {class}{mark}", clock(secs)), false);
                }
                chime(cmds.as_deref_mut());
                state.drilled = Some((Drilled::Cleared(secs), new_best, now));
            }
            DrillEvent::Out(n) => {
                if let Some(ui) = ui.as_mut() {
                    ui.toast(format!("THE DRILL · TIME · {n} OF {} STRUCK", DRILL.len()));
                }
                state.drilled = Some((Drilled::Out(n), false, now));
            }
        }
    }
    let best = settings.as_ref().map_or(0, |s| s.0.course_best_ms);
    let drill_best = settings.as_ref().map_or(0, |s| s.0.drill_best_ms);
    let from = view.map(|v| v.0);
    let t = view.map_or(0.0, |v| v.1);
    let drill = core.world.drill;
    let in_hall = from.is_some_and(hall::in_hall);
    let (lines, waypoint) = if (in_hall || drill.running()) && !state.run.running() {
        // In the Blast Hall: its live fire, and the drill.
        (hall_lines(&state, &drill, core.world.my_target_hits, drill_best, t, now), None)
    } else {
        hud(&state, best, from, t, now)
    };
    state.lines = inside.then_some(lines);
    state.waypoint = waypoint.filter(|_| inside);
    if let Some(mut dev) = dev {
        dev.set("course_next", state.run.next().map_or(-1, |n| n as i32));
        dev.set("course_best_ms", best);
        dev.set("in_hall", in_hall);
        dev.set("hall_targets", core.world.my_target_hits);
        dev.set("trainer", core.hangar.trainer);
        dev.set("drill_lit", u32::from(drill.lit()));
        dev.set("drill_struck", drill.struck() as u32);
        dev.set("drill_left", drill.left(t).unwrap_or(-1.0));
        dev.set("drill_best_ms", drill_best);
        dev.set("in_gantry", core.own_view().is_some_and(|v| hall::in_gantry(v.pos, v.flight_vel)));
        // The Proving Ground's board as the server last sent it: how many on each of the day's
        // lists, and the pilot's own best through the drill.
        let board = core.hangar.proving.as_ref();
        dev.set("board_course", board.map_or(0, |b| b.course.len() as u32));
        dev.set("board_drill", board.map_or(0, |b| b.drill.len() as u32));
        dev.set("board_mine_drill_ms", board.and_then(|b| b.mine.drill_ms).unwrap_or(0));
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

/// A ring flown, or a lit target struck: the cockpit's confirming chime.
fn chime(cmds: Option<&mut UiCmds>) {
    if let Some(c) = cmds {
        c.0.push(UiCmd::Sfx("confirm".into()));
    }
}

/// A best time, as the panel says it: `BEST 1:58.2 FIRST CLASS`, or that it's not done yet.
fn best_line(best_ms: u32, par: f64, none: &str) -> String {
    if best_ms == 0 {
        return none.into();
    }
    let secs = f64::from(best_ms) / 1_000.0;
    format!("BEST {} {}", clock(secs), Class::against(secs, par).name())
}

/// The panel's lines and the waypoint, for the run as it stands: flying it, just flown, or not yet
/// started. `from` is where the suit is, `t` its clock (ticks), `now` the page's.
fn hud(state: &CourseState, best_ms: u32, from: Option<Vec3>, t: f64, now: f64) -> ([String; 3], Waypoint) {
    let n = GATES.len();
    let range = |p: Vec3| from.map_or(String::new(), |f| format!("   {}", metres(p.distance(f))));
    let best = best_line(best_ms, bc_client_core::course::PAR_S, "NOT YET FLOWN");
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
                "The Charter Board's flight certificate, and the board's time. Fly the start ring to go again, or dock."
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
                "The Charter Board's course for new pilots: {n} lit rings in order, from by the inner gate down to Hub Gate's square, and land on its pad. Par is {}.",
                par(bc_client_core::course::PAR_S)
            ),
        ],
        None,
    )
}

/// The panel in the Blast Hall: weapons free, the pilot's targets struck, and the drill (running,
/// just done, or waiting for its first target). `t` is the suit's clock (ticks), `now` the page's.
fn hall_lines(state: &CourseState, drill: &Drill, hits: u32, best_ms: u32, t: f64, now: f64) -> [String; 3] {
    let n = DRILL.len();
    let best = best_line(best_ms, DRILL_PAR_S, "NOT YET CLEARED");
    if drill.running() {
        let left = drill.left(t).unwrap_or(0.0);
        let elapsed = drill.elapsed(t).unwrap_or(0.0);
        return [
            format!("THE DRILL   TARGET {}/{n}   {best}", drill.struck() + 1),
            format!("{} LEFT   {}", clock(left), clock(elapsed)),
            format!(
                "Strike the lit target. Each one struck puts {} s back on the clock; clear all {n} before it runs out.",
                DRILL_BONUS_S
            ),
        ];
    }
    if let Some((how, new_best, at)) = state.drilled
        && now - at < FLOWN_FOR
    {
        return match how {
            Drilled::Cleared(secs) => [
                "THE DRILL   CLEARED".into(),
                format!(
                    "{}   {}{}",
                    clock(secs),
                    Class::against(secs, DRILL_PAR_S).name(),
                    if new_best { "   NEW BEST" } else { "" }
                ),
                "The Charter Board's certificate, and the board's time. Strike the lit target to go again."
                    .into(),
            ],
            Drilled::Out(k) => [
                "THE DRILL   TIME".into(),
                format!("{k} OF {n} STRUCK   {best}"),
                "The clock ran out. Strike the lit target to go again: the clock starts on it.".into(),
            ],
        };
    }
    [
        format!("PROVING GROUND   THE BLAST HALL   {best}"),
        format!("WEAPONS FREE   TARGETS {hits}   STRIKE THE LIT TARGET"),
        format!(
            "The drill: {n} targets lit in turn against the clock, which starts on the first. Par is {}. Training rounds never touch a suit; out through the blast doors, weapons are safe again.",
            par(DRILL_PAR_S)
        ),
    ]
}

fn metres(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

/// A par, in whole minutes and seconds: `2:00`.
fn par(secs: f64) -> String {
    let s = secs.round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}
