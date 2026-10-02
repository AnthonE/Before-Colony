//! Where to go and what to do: the current objective on the HUD with its waypoint in the world
//! (◆, held at the edge of the view while it's off it), and the map of the sector on M.
//!
//! The objectives themselves (what they are, when each is done, where its waypoint is) are
//! `bc_client_core::objectives`; this draws them, and keeps what's done in the settings.
//!
//! The map looks down on the sector (the colony's axis across it, +X to the right): the colony and
//! its docking hub, the dock, the field's rocks, the landmarks, the pilot's suit (▲, pointing where
//! it's headed), the suits in sight (red hostile, green friendly, grey wrecks) and the objective's
//! waypoint; beside it, every objective, done or not. The sector doesn't pause while it's open.

use bc_client_core::objectives::{Objective, ObjectiveInput, Objectives, Waypoint as Goal, waypoint_at};
use bc_client_core::palette;
use bc_econ::wire::Place;
use bc_proto::snapshot::{cover, ent_flags, own_flags};
use bc_proto::{NO_CHUNK, PilotKind};
use bc_sim::bodies::Body;
use bc_sim::colony::city::place_door;
use bc_sim::content::city::PLACES;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::DOCK_CENTER;
use bc_sim::field::FIELD_CENTER;
use bc_sim::ground::Footing;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui_render::prelude::MaterialNode;

use crate::camera::MainCamera;
use crate::hud::{SHADOW, UiFont};
use crate::net::{GameClient, now_s};
use crate::page::Ui;
use crate::settings::SettingsRes;
use crate::ui_panel::PanelMaterial;

/// Opens and closes the map.
pub const MAP_KEY: KeyCode = KeyCode::KeyM;

const fn colour(hex: palette::Hex) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgb(r, g, b)
}

/// The objectives' own colour: a warmer yellow than the dock's amber.
pub const OBJECTIVE: Color = Color::srgb(1.0, 0.89, 0.36);
const CYAN: Color = colour(palette::CYAN);
const AMBER: Color = colour(palette::AMBER);
const RED: Color = colour(palette::RED);
const GREEN: Color = colour(palette::GREEN);
const WHITE: Color = colour(palette::WHITE);
const LABEL: Color = colour(palette::LABEL);
const GREY: Color = Color::srgb(0.55, 0.6, 0.66);
const PLATE: Color = Color::srgba(0.03, 0.07, 0.12, 0.78);
const PLATE_EDGE: Color = Color::srgba(0.62, 0.78, 0.9, 0.45);

/// What the map shows of the sector (m): the colony's axis across it, the landmarks within it.
const MAP_X: (f32, f32) = (-20_000.0, 20_000.0);
const MAP_Z: (f32, f32) = (-14_000.0, 9_000.0);
/// Grid lines this far apart (m).
const GRID: f32 = 5_000.0;
/// Suits in sight drawn on the map, at most.
const CONTACTS: usize = 64;

/// Whether the map is open.
#[derive(Resource, Default)]
pub struct MapOpen(pub bool);

/// The objective on the HUD and its waypoint, as worked out this frame.
#[derive(Resource, Default)]
pub struct ObjectiveState {
    objectives: Objectives,
    pub current: Option<Objective>,
    /// The waypoint: where, and its name.
    pub waypoint: Option<(Vec3, String)>,
    /// What the objectives were checked against.
    input: ObjectiveInput,
}

#[derive(Component)]
pub struct ObjectivePanel;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ObjectiveText {
    Heading,
    Title,
    How,
}
#[derive(Component)]
pub struct Waypoint;
#[derive(Component)]
pub struct WaypointLabel;

#[derive(Component)]
pub struct MapRoot;
/// The map's frame, which its marks are placed in.
#[derive(Component)]
pub struct MapFrame;
/// Something on the map: placed each frame it's open.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum MapMark {
    Me,
    Waypoint,
    Landmark(u8),
    Rock(u16),
    Contact(usize),
}
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum MapText {
    Objectives,
    Info,
    Waypoint,
}

/// Where a point of the sector is on the map, in percent of it (left, top): +X to the right, +Z
/// down, looking down on it from above.
fn on_map(p: Vec3) -> Vec2 {
    Vec2::new((p.x - MAP_X.0) / (MAP_X.1 - MAP_X.0) * 100.0, (p.z - MAP_Z.0) / (MAP_Z.1 - MAP_Z.0) * 100.0)
}

fn km(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

/// A node centred on a point of the map.
fn at(p: Vec2) -> (Node, UiTransform) {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(p.x),
            top: Val::Percent(p.y),
            ..default()
        },
        UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() },
    )
}

/// A dot `size` px across.
fn dot(size: f32, color: Color) -> impl Bundle {
    (
        dot_node(size, Vec2::ZERO),
        UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() },
        BackgroundColor(color),
    )
}

/// A round dot's node, `size` px across, at a point of the map.
fn dot_node(size: f32, p: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(p.x),
        top: Val::Percent(p.y),
        width: Val::Px(size),
        height: Val::Px(size),
        border_radius: BorderRadius::MAX,
        ..default()
    }
}

pub fn setup_map(mut commands: Commands, font: Res<UiFont>, mut panels: ResMut<Assets<PanelMaterial>>) {
    let f = &*font;
    let plate = panels.add(PanelMaterial::plate(PLATE, PLATE_EDGE, 10.0, 1.0));
    // The objective, top left under the status lines.
    commands
        .spawn((
            ObjectivePanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Px(84.0),
                max_width: Val::Px(380.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                row_gap: Val::Px(3.0),
                ..default()
            },
            MaterialNode(plate.clone()),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                ObjectiveText::Heading,
                Text::new(""),
                f.heading(10.0),
                TextColor(LABEL),
                LetterSpacing::Px(2.0),
            ));
            p.spawn((ObjectiveText::Title, Text::new(""), f.heading(14.0), TextColor(OBJECTIVE), SHADOW));
            p.spawn((ObjectiveText::How, Text::new(""), f.text(11.0), TextColor(WHITE)));
        });
    // The waypoint: the label over the diamond, the diamond on the point.
    commands
        .spawn((
            Waypoint,
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
                WaypointLabel,
                Text::new(""),
                f.heading(11.0),
                TextColor(OBJECTIVE),
                SHADOW,
                TextLayout::default().with_no_wrap(),
            ));
            p.spawn((Text::new("◆"), f.heading(16.0), TextColor(OBJECTIVE), SHADOW));
        });
    spawn_map(&mut commands, f, plate);
}

/// The map: hidden until M.
fn spawn_map(commands: &mut Commands, f: &UiFont, plate: Handle<PanelMaterial>) {
    commands
        .spawn((
            MapRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(16.0),
                padding: UiRect::all(Val::Px(24.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.8)),
            GlobalZIndex(5),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            // The sector, from above.
            root.spawn((
                MapFrame,
                Node {
                    height: Val::Vh(78.0),
                    max_width: Val::Vw(68.0),
                    aspect_ratio: Some((MAP_X.1 - MAP_X.0) / (MAP_Z.1 - MAP_Z.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.05, 0.09, 0.85)),
                BorderColor::all(PLATE_EDGE),
            ))
            .with_children(|m| {
                // The grid, every 5 km.
                let line = Color::srgba(0.62, 0.78, 0.9, 0.08);
                let mut x = (MAP_X.0 / GRID).ceil() * GRID;
                while x <= MAP_X.1 {
                    let p = on_map(Vec3::new(x, 0.0, MAP_Z.0));
                    m.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(p.x),
                            top: Val::Percent(0.0),
                            width: Val::Px(1.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(line),
                    ));
                    x += GRID;
                }
                let mut z = (MAP_Z.0 / GRID).ceil() * GRID;
                while z <= MAP_Z.1 {
                    let p = on_map(Vec3::new(MAP_X.0, 0.0, z));
                    m.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(0.0),
                            top: Val::Percent(p.y),
                            width: Val::Percent(100.0),
                            height: Val::Px(1.0),
                            ..default()
                        },
                        BackgroundColor(line),
                    ));
                    z += GRID;
                }
                // The colony: a cylinder along X, seen from above (it lies below the field).
                let a = on_map(COLONY_CENTER - Vec3::new(COLONY_HALF_LENGTH, 0.0, COLONY_RADIUS));
                let b = on_map(COLONY_CENTER + Vec3::new(COLONY_HALF_LENGTH, 0.0, COLONY_RADIUS));
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(a.x),
                        top: Val::Percent(a.y),
                        width: Val::Percent(b.x - a.x),
                        height: Val::Percent(b.y - a.y),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.62, 0.78, 0.9, 0.07)),
                    BorderColor::all(Color::srgba(0.62, 0.78, 0.9, 0.35)),
                ));
                // Its docking hub, out from the −X end cap to the dock.
                let hub_a = on_map(Vec3::new(COLONY_CENTER.x - COLONY_HALF_LENGTH, 0.0, -180.0));
                let hub_b = on_map(Vec3::new(DOCK_CENTER.x, 0.0, 180.0));
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(hub_b.x),
                        top: Val::Percent(hub_a.y),
                        width: Val::Percent(hub_a.x - hub_b.x),
                        height: Val::Percent((hub_b.y - hub_a.y).max(0.4)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.62, 0.78, 0.9, 0.35)),
                ));
                let label = |m: &mut ChildSpawnerCommands, p: Vec3, text: &str, color: Color, size: f32| {
                    let (node, ui) = at(on_map(p));
                    m.spawn((
                        node,
                        ui,
                        Text::new(text),
                        f.heading(size),
                        TextColor(color),
                        SHADOW,
                        LetterSpacing::Px(2.0),
                        TextLayout::default().with_no_wrap(),
                    ));
                };
                label(
                    m,
                    COLONY_CENTER + Vec3::new(COLONY_HALF_LENGTH * 0.62, 0.0, COLONY_RADIUS * 0.55),
                    "THE COLONY",
                    LABEL,
                    11.0,
                );
                label(m, FIELD_CENTER + Vec3::new(0.0, 0.0, -8_200.0), "DEBRIS FIELD", GREY, 10.0);
                // The dock, its ring of lights.
                let (node, ui) = at(on_map(DOCK_CENTER));
                m.spawn((
                    node,
                    ui,
                    Text::new("O DOCK"),
                    f.heading(11.0),
                    TextColor(AMBER),
                    SHADOW,
                    TextLayout::default().with_no_wrap(),
                ));
                // (The field's rocks come with the Welcome: `sync_map_rocks`.)
                // The landmarks.
                for (k, def) in LANDMARKS.iter().enumerate() {
                    let (node, ui) = at(on_map(def.center));
                    m.spawn((
                        MapMark::Landmark(k as u8),
                        node,
                        ui,
                        Text::new(format!("■ {}", def.name)),
                        f.heading(11.0),
                        TextColor(CYAN),
                        SHADOW,
                        TextLayout::default().with_no_wrap(),
                    ));
                }
                // The suits in sight.
                for i in 0..CONTACTS {
                    m.spawn((MapMark::Contact(i), dot(6.0, RED), Visibility::Hidden));
                }
                // The waypoint, and the pilot over everything.
                let (node, ui) = at(Vec2::ZERO);
                m.spawn((
                    MapMark::Waypoint,
                    node,
                    ui,
                    Text::new("◆"),
                    f.heading(18.0),
                    TextColor(OBJECTIVE),
                    SHADOW,
                    Visibility::Hidden,
                ));
                let (node, ui) = at(Vec2::ZERO);
                m.spawn((
                    MapMark::Me,
                    node,
                    ui,
                    Text::new("▲"),
                    f.heading(16.0),
                    TextColor(WHITE),
                    SHADOW,
                    Visibility::Hidden,
                ));
                // The scale: 5 km of the map's width.
                m.spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(10.0),
                    bottom: Val::Px(8.0),
                    width: Val::Percent(GRID / (MAP_X.1 - MAP_X.0) * 100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|s| {
                    s.spawn((Text::new("5 km"), f.text(10.0), TextColor(LABEL)));
                    s.spawn((
                        Node { width: Val::Percent(100.0), height: Val::Px(2.0), ..default() },
                        BackgroundColor(LABEL),
                    ));
                });
            });
            // Beside it: the objectives, the waypoint, the legend.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                    row_gap: Val::Px(10.0),
                    max_width: Val::Px(340.0),
                    ..default()
                },
                MaterialNode(plate),
            ))
            .with_children(|side| {
                side.spawn((
                    Text::new("SECTOR L1 - MAP"),
                    f.heading(13.0),
                    TextColor(LABEL),
                    LetterSpacing::Px(3.0),
                ));
                side.spawn((
                    Text::new("OBJECTIVES"),
                    f.heading(10.0),
                    TextColor(LABEL),
                    LetterSpacing::Px(2.0),
                ));
                side.spawn((MapText::Objectives, Text::new(""), f.text(12.0), TextColor(WHITE)));
                // In the labels' face: the readouts' has no ◆ ▲ ■.
                side.spawn((MapText::Waypoint, Text::new(""), f.heading(12.0), TextColor(OBJECTIVE)));
                side.spawn((MapText::Info, Text::new(""), f.heading(11.0), TextColor(LABEL)));
            });
        });
}

/// The field's rocks on the map, once the Welcome has said which field it is (and again if a
/// later Welcome names another).
pub fn sync_map_rocks(
    mut commands: Commands,
    game: NonSend<GameClient>,
    open: Res<MapOpen>,
    frame: Query<Entity, With<MapFrame>>,
    rocks: Query<(Entity, &MapMark)>,
    mut drawn: Local<Option<(u32, u16)>>,
) {
    let g = game.borrow();
    let Some(w) = g.core.welcome.filter(|_| open.0) else { return };
    if *drawn == Some((w.field_seed, w.field_rocks)) {
        return;
    }
    let Ok(frame) = frame.single() else { return };
    *drawn = Some((w.field_seed, w.field_rocks));
    for (e, m) in &rocks {
        if matches!(m, MapMark::Rock(_)) {
            commands.entity(e).despawn();
        }
    }
    let field = &g.core.world.bodies.field;
    commands.entity(frame).with_children(|m| {
        for (i, r) in field.rocks().iter().enumerate() {
            let size = if r.axes.min_element() >= 10.0 { 4.0 } else { 3.0 };
            let p = on_map(r.pos);
            m.spawn((MapMark::Rock(i as u16), dot(size, Color::srgba(0.7, 0.66, 0.6, 0.55))))
                // Under the landmarks, the suits and the pilot.
                .insert((dot_node(size, p), ZIndex(-1)));
        }
    });
}

/// M opens and closes the map; it closes by itself when the pilot leaves the sector.
pub fn toggle_map(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<Ui>,
    indoors: Res<crate::hangar::Indoors>,
    mut open: ResMut<MapOpen>,
) {
    let can = ui.playing() && !indoors.0 && !ui.on_foot;
    if !can {
        if open.0 {
            open.0 = false;
        }
        return;
    }
    if keys.just_pressed(MAP_KEY) && !ui.panel_open() {
        open.0 = !open.0;
    }
}

/// Checks the objectives against what the pilot is doing, keeps what's done, and draws the
/// current one and its waypoint.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_objectives(
    game: NonSend<GameClient>,
    mut state: ResMut<ObjectiveState>,
    mut settings: ResMut<SettingsRes>,
    mut ui: ResMut<Ui>,
    indoors: Res<crate::hangar::Indoors>,
    open: Res<MapOpen>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut panel: Query<&mut Visibility, (With<ObjectivePanel>, Without<Waypoint>)>,
    mut texts: Query<(&ObjectiveText, &mut Text)>,
    mut marker: Query<(&mut Node, &mut Visibility), (With<Waypoint>, Without<ObjectivePanel>)>,
    mut label: Query<&mut Text, (With<WaypointLabel>, Without<ObjectiveText>)>,
    onfoot: Res<crate::onfoot::OnFoot>,
) {
    let g = game.borrow();
    let core = &g.core;
    let now = now_s();
    let t = core.render_tick(now);
    let own = core.world.own.filter(|o| o.alive);
    let view = core.own_view().filter(|v| v.alive);
    let feet = crate::hud::footed(core);
    let survival = core.welcome.is_some_and(|w| w.survival);
    let in_sector = ui.playing() && !indoors.0 && !ui.on_foot;
    let landed_on = match (feet.footing, feet.body) {
        (Footing::Grounded, Body::Landmark(k)) => Some(k),
        _ => None,
    };
    let hidden = own.is_some_and(|o| o.cover == cover::HIDDEN);
    let in_spot = feet.spot.is_some() || crate::hud::hide_spot(core).is_some();
    let held = own
        .filter(|o| o.held != NO_CHUNK)
        .and_then(|o| core.world.objects.get(usize::from(o.held))?.as_ref())
        .map_or(0, |c| c.desc.mass_kg);
    let input = ObjectiveInput {
        survival,
        flying: in_sector && own.is_some(),
        landmarks: core.world.bodies.landmarks().len() as u8,
        rocks: !core.world.bodies.field.is_empty(),
        landed_on,
        hidden_on: match feet.body {
            Body::Landmark(k) if hidden && in_spot => Some(k),
            _ => None,
        },
        cargo_kg: own.map_or(0, |o| o.cargo_kg.iter().map(|kg| u32::from(*kg)).sum::<u32>()) + held,
        docked: own.is_some_and(|o| o.flags & own_flags::DOCKED != 0),
        doll_kills: core.world.doll_kills,
        colony: core.welcome.is_some_and(|w| w.colony),
        in_city: core.hangar.place == Some(Place::City),
        at_exchange: core.hangar.place == Some(Place::City)
            && onfoot
                .city
                .as_ref()
                .is_some_and(|c| c.focus == Some(usize::from(bc_client_core::objectives::exchange()))),
        sales: core.hangar.sales,
    };
    let (mut done, mut downed) = (settings.0.objectives_done, settings.0.dolls_downed);
    let before = done;
    let current = state.objectives.step(&mut done, &mut downed, &input);
    if done != settings.0.objectives_done || downed != settings.0.dolls_downed {
        settings.0.objectives_done = done;
        settings.0.dolls_downed = downed;
    }
    // Say so when one is done.
    if let Some(o) = Objective::ALL.iter().find(|o| before & o.bit() == 0 && done & o.bit() != 0) {
        ui.toast(format!("OBJECTIVE DONE - {}", o.title(survival)));
    }
    let from = view.map_or(Vec3::ZERO, |v| v.pos);
    // On foot, the first of those done on foot (the flight's wait for the next sortie).
    let afoot = ui.playing() && ui.on_foot;
    let current = if afoot {
        Objective::order(survival)
            .iter()
            .copied()
            .find(|o| o.on_foot() && done & o.bit() == 0 && o.available(&input))
    } else {
        current
    };
    state.current = current;
    state.input = input;
    // On foot, the way is on the page's map (M): the place's door, and how far off it is.
    let door = current.and_then(|o| match o.waypoint() {
        Goal::Place(k) => PLACES.get(usize::from(k)),
        _ => None,
    });
    ui.map_goal = door.filter(|_| afoot).map(|p| p.slug);
    let to_door = door.zip(onfoot.city.as_ref()).filter(|(p, c)| p.strip == c.strip).map(|(p, c)| {
        let ((s, x), _) = place_door(p);
        let at = c.feet();
        Vec2::new(at.s - s, at.x - x).length()
    });
    state.waypoint =
        if afoot { None } else { current.and_then(|o| waypoint_at(core, o.waypoint(), from, t)) };

    let shown = settings.0.objectives
        && ((in_sector && own.is_some()) || (afoot && current.is_some_and(|o| o.on_foot())));
    // The panel: hidden under the map, which lists them all.
    let want = if shown && current.is_some() && !open.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut panel {
        v.set_if_neq(want);
    }
    if let Some(o) = current {
        let order = Objective::order(survival);
        let count = order.iter().filter(|o| o.available(&input)).count();
        let n = order.iter().filter(|o| o.available(&input) && done & o.bit() != 0).count();
        let progress =
            o.progress(&input, downed).map_or(String::new(), |(a, b)| format!("   {a}/{b}{}", unit(o)));
        let dist = match (&state.waypoint, to_door) {
            (Some((p, _)), _) => format!("   {}", km(p.distance(from))),
            (None, Some(d)) => format!("   {}", km(d)),
            _ => String::new(),
        };
        for (which, mut text) in &mut texts {
            let s = match which {
                ObjectiveText::Heading => format!("OBJECTIVE {}/{count}   M - MAP", n + 1),
                ObjectiveText::Title => format!("{}{progress}{dist}", o.title(survival)),
                ObjectiveText::How => o.how(survival),
            };
            if text.0 != s {
                text.0 = s;
            }
        }
    }

    // The waypoint in the world: on its point, or at the edge of the view toward it.
    let Ok((mut node, mut vis)) = marker.single_mut() else { return };
    let (Some((p, name)), true, Ok((cam, cam_tf))) =
        (state.waypoint.clone(), shown && !open.0, camera.single())
    else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let Some(rect) = cam.logical_viewport_rect() else { return };
    let on_screen = cam.world_to_viewport(cam_tf, p).ok().filter(|v| rect.contains(*v));
    let spot = on_screen.unwrap_or_else(|| {
        let local = cam_tf.affine().inverse().transform_point3(p);
        let d = Vec2::new(local.x, -local.y).try_normalize().unwrap_or(Vec2::Y);
        let half = (rect.half_size() - Vec2::new(70.0, 40.0)).max(Vec2::ONE);
        let reach = (half.x / d.x.abs().max(1e-4)).min(half.y / d.y.abs().max(1e-4));
        rect.center() + d * reach
    });
    node.left = Val::Px(spot.x);
    node.top = Val::Px(spot.y + 8.0);
    if let Ok(mut l) = label.single_mut() {
        let s = format!("{name} {}", km(p.distance(from)));
        if l.0 != s {
            l.0 = s;
        }
    }
    vis.set_if_neq(Visibility::Inherited);
}

/// What an objective's progress counts.
fn unit(o: Objective) -> &'static str {
    match o {
        Objective::Mine => " KG",
        _ => "",
    }
}

/// The map, while it's open.
#[allow(clippy::type_complexity)]
pub fn update_map(
    game: NonSend<GameClient>,
    open: Res<MapOpen>,
    state: Res<ObjectiveState>,
    settings: Res<SettingsRes>,
    mut root: Query<&mut Visibility, (With<MapRoot>, Without<MapMark>)>,
    mut marks: Query<
        (&MapMark, &mut Node, &mut UiTransform, &mut Visibility, Option<&mut BackgroundColor>),
        Without<MapRoot>,
    >,
    mut texts: Query<(&MapText, &mut Text)>,
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
    let view = core.own_view().filter(|v| v.alive);
    let me = view.map(|v| v.pos);
    // The suits in sight, nearest first.
    let mut seen: Vec<(Vec3, Color)> = world
        .entities
        .iter()
        .flatten()
        .filter(|tr| Some(tr.latest.slot) != world.own.map(|o| o.slot))
        .map(|tr| {
            let e = &tr.latest;
            let pos = tr.sample(t, &world.bodies).pos;
            let color = if e.flags & ent_flags::WRECK != 0 {
                GREY
            } else if e.faction != core.cfg.faction {
                RED
            } else {
                GREEN
            };
            (pos, color)
        })
        .collect();
    if let Some(m) = me {
        seen.sort_by(|a, b| a.0.distance_squared(m).total_cmp(&b.0.distance_squared(m)));
    }
    let in_view = |p: Vec2| (0.0..=100.0).contains(&p.x) && (0.0..=100.0).contains(&p.y);
    // Held at the map's edge if it's off it.
    let clamp = |p: Vec2| p.clamp(Vec2::splat(1.0), Vec2::splat(99.0));
    for (mark, mut node, mut ui, mut vis, bg) in &mut marks {
        let place = |node: &mut Node, p: Vec2| {
            node.left = Val::Percent(p.x);
            node.top = Val::Percent(p.y);
        };
        let shown = match *mark {
            MapMark::Me => match view {
                Some(v) => {
                    place(&mut node, clamp(on_map(v.pos)));
                    // Pointing the way it's headed (or, at rest, the way it faces).
                    let ahead = if v.flight_vel.length() > 5.0 { v.flight_vel } else { v.rot * Vec3::Z };
                    let d = Vec2::new(ahead.x, ahead.z);
                    let turn =
                        if d.length() > 1e-3 { d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2 } else { 0.0 };
                    ui.rotation = Rot2::radians(turn);
                    true
                }
                None => false,
            },
            MapMark::Waypoint => match &state.waypoint {
                Some((p, _)) if settings.0.objectives => {
                    place(&mut node, clamp(on_map(*p)));
                    true
                }
                _ => false,
            },
            MapMark::Landmark(k) => {
                if let Some(pose) = world.bodies.pose_at(Body::Landmark(k), t) {
                    place(&mut node, on_map(pose.pos));
                }
                usize::from(k) < world.bodies.landmarks().len()
            }
            MapMark::Rock(i) => {
                let field = &world.bodies.field;
                usize::from(i) < field.len() && !field.is_dead(usize::from(i))
            }
            MapMark::Contact(i) => match seen.get(i) {
                Some(&(p, color)) if in_view(on_map(p)) => {
                    place(&mut node, on_map(p));
                    if let Some(mut bg) = bg {
                        bg.0 = color;
                    }
                    true
                }
                _ => false,
            },
        };
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }

    let survival = state.input.survival;
    let done = settings.0.objectives_done;
    let mut list = String::new();
    for o in Objective::order(survival) {
        if !o.available(&state.input) {
            continue;
        }
        let mark = if done & o.bit() != 0 {
            "[x]"
        } else if state.current == Some(*o) {
            "[>]"
        } else {
            "[ ]"
        };
        let progress = match o.progress(&state.input, settings.0.dolls_downed) {
            Some((a, b)) if done & o.bit() == 0 => format!("  {a}/{b}{}", unit(*o)),
            _ => String::new(),
        };
        list.push_str(&format!("{mark} {}{progress}\n", o.title(survival)));
    }
    if state.current.is_none() {
        list.push_str("\nEvery objective done. The sector is yours.\n");
    }
    let waypoint = match (&state.waypoint, me, state.current) {
        (Some((p, name)), Some(m), Some(o)) => {
            let up = p.y - m.y;
            let above = if up.abs() < 200.0 {
                "level with you".to_string()
            } else if up > 0.0 {
                format!("{} above you", km(up))
            } else {
                format!("{} below you", km(-up))
            };
            format!("◆ {name}  {}  {above}\n{}", km(p.distance(m)), o.how(survival))
        }
        _ => String::new(),
    };
    let anime = core.welcome.is_some_and(|w| w.anime);
    let info = format!(
        "▲ you   ◆ objective   ■ landmark   O dock\nred hostile   green friendly   grey wreck\n\n{}\n\nM closes the map. The sector doesn't pause.",
        if anime {
            "ANIME FLIGHT: only boost burns your gauge, and it refills when you let go of Shift."
        } else {
            "REAL FLIGHT: every burn spends propellant. Brake before you're dry."
        }
    );
    let them = world.entities.iter().flatten().filter(|tr| tr.latest.pilot == PilotKind::MobileDoll).count();
    let info = if them > 0 {
        format!("{}S IN SIGHT  {them}\n{info}", bc_sim::content::doll_name().to_uppercase())
    } else {
        info
    };
    for (which, mut text) in &mut texts {
        let s = match which {
            MapText::Objectives => list.clone(),
            MapText::Waypoint => waypoint.clone(),
            MapText::Info => info.clone(),
        };
        if text.0 != s {
            text.0 = s;
        }
    }
}
