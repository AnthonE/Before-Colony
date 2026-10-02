//! Where to go and what to do: the current objective on the HUD with its waypoint in the world
//! (◆, held at the edge of the view while it's off it), and M, which opens the chart (`chart.rs`)
//! with every objective beside it.
//!
//! The objectives themselves (what they are, when each is done, where its waypoint is) are
//! `bc_client_core::objectives`; this draws them, and keeps what's done in the settings.

use bc_client_core::objectives::{Objective, ObjectiveInput, Objectives, waypoint_at};
use bc_client_core::palette;
use bc_proto::NO_CHUNK;
use bc_proto::snapshot::{cover, own_flags};
use bc_sim::bodies::Body;
use bc_sim::ground::Footing;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui_render::prelude::MaterialNode;

use crate::camera::MainCamera;
use crate::hud::{SHADOW, UiFont};
use crate::net::{GameClient, now_s};
use crate::page::Ui;
use crate::settings::SettingsRes;
use crate::ui_panel::PanelMaterial;

/// Opens and closes the chart.
pub const MAP_KEY: KeyCode = KeyCode::KeyM;

const fn colour(hex: palette::Hex) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgb(r, g, b)
}

/// The objectives' own colour: a warmer yellow than the dock's amber.
pub const OBJECTIVE: Color = Color::srgb(1.0, 0.89, 0.36);
const WHITE: Color = colour(palette::WHITE);
const LABEL: Color = colour(palette::LABEL);
const PLATE: Color = Color::srgba(0.03, 0.07, 0.12, 0.78);
const PLATE_EDGE: Color = Color::srgba(0.62, 0.78, 0.9, 0.45);

/// Whether the chart is open.
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

impl ObjectiveState {
    /// What the objectives were last checked against.
    pub fn input(&self) -> &ObjectiveInput {
        &self.input
    }
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

pub fn setup_objectives(
    mut commands: Commands,
    font: Res<UiFont>,
    mut panels: ResMut<Assets<PanelMaterial>>,
) {
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
}

/// M opens and closes the chart; it closes by itself when the pilot leaves the sector.
pub fn toggle_map(
    game: NonSend<GameClient>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<Ui>,
    indoors: Res<crate::hangar::Indoors>,
    mut open: ResMut<MapOpen>,
) {
    // The chart is space's: inside the colony, the gate's marker shows the way home.
    let inside = game.borrow().core.welcome.is_some_and(|w| w.interior);
    if inside && keys.just_pressed(MAP_KEY) && ui.playing() && !ui.on_foot {
        ui.toast("THE CHART IS FOR SPACE: INSIDE, FOLLOW THE INNER GATE'S MARKER");
    }
    let can = ui.playing() && !indoors.0 && !ui.on_foot && !inside;
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
    // Flying inside the colony: no objectives in there, and the waypoint is the inner gate.
    let inside = in_sector && core.welcome.is_some_and(|w| w.interior);
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
    state.current = current;
    state.input = input;
    state.waypoint = current.and_then(|o| waypoint_at(core, o.waypoint(), from, t));
    if inside {
        state.waypoint =
            Some((bc_sim::colony::interior::INNER_GATE, "INNER GATE: DOCK AT REST IN ITS RING".into()));
    }

    let shown = settings.0.objectives && in_sector && own.is_some() && !inside;
    // The panel: hidden under the map, which lists them all.
    let want = if shown && current.is_some() && !open.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut panel {
        v.set_if_neq(want);
    }
    if let Some(o) = current {
        let order = Objective::order(survival);
        let count = order.iter().filter(|o| o.available(input.landmarks, input.rocks)).count();
        let n =
            order.iter().filter(|o| o.available(input.landmarks, input.rocks) && done & o.bit() != 0).count();
        let progress =
            o.progress(&input, downed).map_or(String::new(), |(a, b)| format!("   {a}/{b}{}", unit(o)));
        let dist =
            state.waypoint.as_ref().map_or(String::new(), |(p, _)| format!("   {}", km(p.distance(from))));
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
        (state.waypoint.clone(), (shown || inside) && !open.0, camera.single())
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

fn km(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

/// What an objective's progress counts.
fn unit(o: Objective) -> &'static str {
    match o {
        Objective::Mine => " KG",
        _ => "",
    }
}
