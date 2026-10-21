//! Signs over the colony's places, on foot in the city: where the objective is (◆ over its place's
//! door, from anywhere on the strip, G walks the pilot there), what each desk's counter is and that
//! E uses it (the terminals read as terminals from across the room), the Blast Hall's gantry hatch
//! (`BOARD A TRAINER · E`) and the trainer standing on its pad. Each is a label on the screen over
//! its point in the world, shown when the pilot is near enough and it's ahead of them, brighter the
//! nearer.

use bc_sim::colony::city::{KERB, place_door, room};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::hall;
use bc_sim::content::city::{PLACES, PROVING_GROUND, PlaceKind};
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::city::{RenderOrigin, colony_point};
use crate::hud::UiFont;
use crate::net::GameClient;

pub struct SignsPlugin;

impl Plugin for SignsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_signs).add_systems(Update, draw_signs.after(crate::view::Vis::Camera));
    }
}

/// The most signs shown at once.
const SIGNS: usize = 8;

/// A sign's label (its index in this frame's list).
#[derive(Component)]
struct SignLabel(usize);

#[derive(Component)]
struct SignText(usize);

/// A sign: its point (the colony's frame), what it says, and how near it shows from, m.
struct Sign {
    at: DVec3,
    title: String,
    line: &'static str,
    near: f64,
}

/// What a desk does, under its name.
fn desk_line(kind: PlaceKind) -> &'static str {
    match kind {
        PlaceKind::Bar => "MOST WANTED · SALVAGE TERMS · E",
        PlaceKind::Exchange => "THE EXCHANGE · ORDERS ON THE BOOK · E",
        PlaceKind::Charter => "CONTRACTS · PAY · THE CHARTER · E",
        PlaceKind::Proving => "TRAINING · THE TEST RANGE · E",
        PlaceKind::HubGate => "THE CAP LIFT UP · E",
    }
}

/// The signs on strip `strip`: the objective's place (`goal`, its slug), each desk's counter, the
/// gantry's hatch and its trainer (`trainer`: its name, the suit the gantry has ready).
fn signs(strip: u8, trainer: &str, goal: Option<&str>) -> Vec<Sign> {
    let mut out = Vec::new();
    if let Some((_, p)) = goal.and_then(bc_sim::colony::city::place).filter(|(_, p)| p.strip == strip) {
        let ((s, x), _) = place_door(p);
        let at = colony_point(CityPos::new(p.strip, x, s, KERB + 9.0));
        out.push(Sign {
            at,
            title: format!("◆ {}", p.name),
            line: "THE OBJECTIVE · G: WALK THERE",
            near: f64::INFINITY,
        });
    }
    for (i, p) in PLACES.iter().enumerate() {
        if p.strip != strip {
            continue;
        }
        let Some(r) = room(i) else { continue };
        // Over the counter, a little past where the pilot stands at it.
        let ((s, x), (ds, dx)) = r.counter_spot();
        let at = colony_point(CityPos::new(r.strip, x + dx * 2.0, s + ds * 2.0, KERB + 3.6));
        out.push(Sign { at, title: p.name.to_string(), line: desk_line(p.kind), near: 45.0 });
        if i == PROVING_GROUND {
            let ((s, x), _) = hall::hatch();
            let at = colony_point(CityPos::new(r.strip, x, s, KERB + 4.5));
            out.push(Sign {
                at,
                title: "THE GANTRY'S HATCH".into(),
                line: "BOARD A TRAINER · E",
                near: 90.0,
            });
            let up = bc_sim::colony::frame::up_at(hall::gantry());
            let at = (hall::gantry() + up * 21.0).as_dvec3();
            out.push(Sign {
                at,
                title: trainer.to_string(),
                line: "ON THE GANTRY · READY TO BOARD",
                near: 120.0,
            });
        }
    }
    out
}

fn setup_signs(mut commands: Commands, font: Res<UiFont>) {
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            GlobalZIndex(-1),
            Pickable::IGNORE,
        ))
        .id();
    for k in 0..SIGNS {
        let label = commands
            .spawn((
                SignLabel(k),
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    border: UiRect::left(Val::Px(3.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
                UiTransform { translation: Val2::percent(-50.0, -100.0), ..default() },
                BackgroundColor(Color::srgba(0.02, 0.05, 0.08, 0.72)),
                BorderColor::all(Color::srgb(0.45, 0.95, 1.0)),
                Visibility::Hidden,
                Pickable::IGNORE,
                ChildOf(root),
            ))
            .id();
        commands.spawn((
            SignText(2 * k),
            Text::new(""),
            font.heading(15.0),
            TextColor(Color::srgb(0.92, 0.97, 1.0)),
            TextLayout::default().with_no_wrap(),
            Pickable::IGNORE,
            ChildOf(label),
        ));
        commands.spawn((
            SignText(2 * k + 1),
            Text::new(""),
            font.text(12.0),
            TextColor(Color::srgb(0.45, 0.95, 1.0)),
            TextLayout::default().with_no_wrap(),
            Pickable::IGNORE,
            ChildOf(label),
        ));
    }
}

/// Places the signs near the pilot on foot in the city over their points, nearest first.
#[allow(clippy::type_complexity)]
fn draw_signs(
    game: Option<NonSend<GameClient>>,
    origin: Res<RenderOrigin>,
    ui: Option<Res<crate::page::Ui>>,
    onfoot: Option<Res<crate::onfoot::OnFoot>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut labels: Query<
        (&SignLabel, &mut Node, &mut Visibility, &mut BackgroundColor, &ComputedNode),
        Without<SignText>,
    >,
    mut texts: Query<(&SignText, &mut Text, &mut TextColor)>,
) {
    let mut shown: Vec<(f64, Vec2, String, &'static str)> = Vec::new();
    let mut view = None;
    if let (Some(game), Ok((cam, cam_tf))) = (game, camera.single()) {
        view = cam.logical_viewport_size();
        let g = game.borrow();
        let core = &g.core;
        let playing = ui.as_ref().is_some_and(|u| u.playing() && !u.panel_open());
        let city = onfoot.as_ref().and_then(|o| o.city.as_ref());
        if let Some(c) = city.filter(|_| playing && core.hangar.in_city() && !core.inside()) {
            let eye = origin.0 + cam_tf.translation().as_dvec3();
            let goal = ui.as_ref().and_then(|u| u.map_goal);
            for sign in signs(c.strip, &core.hangar.trainer_name(), goal) {
                let d = (sign.at - eye).length();
                if d > sign.near {
                    continue;
                }
                if let Ok(p) = cam.world_to_viewport(cam_tf, origin.place(sign.at)) {
                    // The objective's says how far, as its panel does: along the floor to the door.
                    let title = if sign.near.is_infinite() {
                        let walk = goal
                            .and_then(bc_sim::colony::city::place)
                            .map(|(i, _)| crate::map::to_door(c, i));
                        format!("{}  {}", sign.title, crate::map::km(walk.unwrap_or(d as f32)))
                    } else {
                        sign.title
                    };
                    // (The objective's first, whatever's nearer.)
                    let rank = if sign.near.is_infinite() { -1.0 } else { d };
                    shown.push((rank, p, title, sign.line));
                }
            }
        }
    }
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (label, mut node, mut vis, mut bg, computed) in &mut labels {
        match shown.get(label.0) {
            Some(&(d, mut p, _, _)) => {
                // The objective's stays whole on the screen, at its edge when its door is off it.
                if let Some(view) = view.filter(|_| d < 0.0) {
                    let half = computed.size().x * computed.inverse_scale_factor() / 2.0 + 8.0;
                    p.x = p.x.clamp(half.min(view.x / 2.0), (view.x - half).max(view.x / 2.0));
                }
                node.left = Val::Px(p.x);
                node.top = Val::Px(p.y);
                // Nearer, more solid (the objective's always).
                bg.0 = Color::srgba(0.02, 0.05, 0.08, (0.85 - (d.max(0.0) as f32 / 150.0)).clamp(0.35, 0.85));
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    for (t, mut text, _) in &mut texts {
        let want = shown.get(t.0 / 2).map_or("", |s| if t.0 % 2 == 0 { s.2.as_str() } else { s.3 });
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}
