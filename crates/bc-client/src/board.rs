//! The Proving Ground's board on the Blast Hall's back wall (`docs/TRAINING.md`): X-Wing's
//! high-score table, high over the hall's floor and over the course drawn in light. It shows the
//! day's best round the course and through the drill, and the best ever, as the server keeps them
//! (`bc_econ::proving`, sent to every pilot in the colony): names and times, the pilot's own lit.
//!
//! Its text is laid out by the UI into a texture of its own, drawn by a camera of its own for a few
//! frames each time the board changes (the cockpit's monitors are drawn into theirs alike), and
//! hung on the wall on the city's layer for anyone in the colony to see.

use bc_econ::proving::{BoardView, Row, clock};
use bc_sim::colony::frame::{CityPos, up_at};
use bc_sim::colony::hall::hall;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::city::{CITY_LAYER, Placed};
use crate::hud::UiFont;
use crate::net::GameClient;

pub struct WallBoardPlugin;

impl Plugin for WallBoardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_board).add_systems(Update, draw_board);
    }
}

/// The board's texture, px.
const TEXTURE: [u32; 2] = [1536, 480];
/// The layer its camera draws (nothing in the world is on it: only its UI).
const LAYER: usize = 5;
/// On the back wall: its middle this high up, m, its width and height (the texture's shape), and
/// how far out from the wall it hangs.
const AT_H: f32 = 48.0;
const WIDTH: f32 = 64.0;
const HEIGHT: f32 = WIDTH * TEXTURE[1] as f32 / TEXTURE[0] as f32;
const OUT: f32 = 0.4;
/// The day's best each column lists.
const ROWS: usize = 8;
/// How many frames the camera draws once the board changes (the UI lays its text out first).
const DRAW_FRAMES: u32 = 4;

const AMBER: Color = Color::srgb(1.0, 0.7, 0.2);
const CYAN: Color = Color::srgb(0.45, 0.95, 1.0);
const WHITE: Color = Color::srgb(0.92, 0.95, 0.96);
const DIM: Color = Color::srgb(0.5, 0.6, 0.62);
const YOU: Color = Color::srgb(1.0, 0.92, 0.45);

/// The board's camera and what it last drew (and the hangar's version it was looked at in).
#[derive(Resource)]
struct WallBoard {
    camera: Entity,
    shown: Option<BoardView>,
    seen: u64,
    frames: u32,
}

/// One of the board's lines of text.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum BoardText {
    /// A column's heading: the course's (0) or the drill's (1).
    Heading(usize),
    /// Row `k` of a column.
    Row(usize, usize),
    /// A column's best ever.
    Record(usize),
}

/// The board on the wall.
#[derive(Component)]
struct OnTheWall;

fn setup_board(
    mut commands: Commands,
    font: Res<UiFont>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let [w, h] = TEXTURE;
    let image = images.add(Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None));
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -2,
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::srgb(0.012, 0.018, 0.024)),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: w as f32, height: h as f32 },
                ..OrthographicProjection::default_2d()
            }),
            RenderTarget::Image(image.clone().into()),
            RenderLayers::layer(LAYER),
        ))
        .id();
    let f = &*font;
    commands
        .spawn((
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(w as f32),
                height: Val::Px(h as f32),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(36.0), Val::Px(20.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("THE PROVING GROUND · TODAY'S BEST"),
                f.heading(46.0),
                TextColor(AMBER),
                TextLayout::default().with_no_wrap(),
            ));
            root.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(56.0), ..default() })
                .with_children(|cols| {
                    for c in 0..2 {
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            width: Val::Percent(50.0),
                            row_gap: Val::Px(2.0),
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn((
                                BoardText::Heading(c),
                                Text::new(""),
                                f.heading(34.0),
                                TextColor(CYAN),
                                TextLayout::default().with_no_wrap(),
                            ));
                            for k in 0..ROWS {
                                col.spawn((
                                    BoardText::Row(c, k),
                                    Text::new(""),
                                    f.text(31.0),
                                    TextColor(WHITE),
                                    TextLayout::default().with_no_wrap(),
                                ));
                            }
                            col.spawn((
                                BoardText::Record(c),
                                Text::new(""),
                                f.text(27.0),
                                TextColor(DIM),
                                TextLayout::default().with_no_wrap(),
                            ));
                        });
                    }
                });
        });

    // On the back wall, facing the hall: the quad looks the texture's way up (its top-left corner
    // the texture's).
    let r = hall();
    let v = r.depth() - OUT;
    let point = |u: f32, v: f32, h: f32| {
        let (s, x) = r.front.point(u, v);
        CityPos::new(r.strip, x, s, h).to_colony()
    };
    let at = point(0.0, v, AT_H);
    let up = up_at(at);
    let facing = (point(0.0, v - 1.0, AT_H) - at).normalize();
    let right = up.cross(facing);
    let rot = Quat::from_mat3(&Mat3::from_cols(right, up, facing)).normalize();
    commands.spawn((
        OnTheWall,
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(WIDTH, HEIGHT)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(image),
            unlit: true,
            ..default()
        })),
        Transform::from_rotation(rot),
        Placed(at.as_dvec3()),
        RenderLayers::layer(CITY_LAYER),
        NotShadowCaster,
        NotShadowReceiver,
        Visibility::Hidden,
    ));
    commands.insert_resource(WallBoard { camera, shown: None, seen: u64::MAX, frames: 0 });
}

/// A row of a column: its place, name and time, and its certificate's class.
fn row_text(k: usize, r: &Row) -> String {
    let name: String = r.name.chars().take(16).collect();
    format!("{:>2}  {name:<16}  {}", k + 1, clock(r.ms))
}

/// A column's heading and its best ever.
fn column(view: &BoardView, c: usize) -> (String, &[Row], String) {
    let (name, rows, record, par) = if c == 0 {
        ("THE COURSE", &view.course[..], &view.course_record, view.course_par_ms)
    } else {
        ("THE DRILL", &view.drill[..], &view.drill_record, view.drill_par_ms)
    };
    let record = record.as_ref().map_or_else(
        || "THE BEST EVER  -".to_string(),
        |r| format!("THE BEST EVER  {}  {}", clock(r.ms), r.name),
    );
    (format!("{name}   PAR {}", clock(par)), rows, record)
}

/// Shows the board in the colony, and lays it out again when the server's word on it changes.
#[allow(clippy::type_complexity)]
fn draw_board(
    game: Option<NonSend<GameClient>>,
    board: Option<ResMut<WallBoard>>,
    mut cameras: Query<&mut Camera>,
    mut wall: Query<&mut Visibility, With<OnTheWall>>,
    mut texts: Query<(&BoardText, &mut Text, &mut TextColor)>,
) {
    let Some(mut board) = board else { return };
    let (shown, view) = match &game {
        Some(game) => {
            let g = game.borrow();
            let h = &g.core.hangar;
            // Looked at again only when the hangar's word has moved.
            let view = (h.version != board.seen).then(|| h.proving.clone()).flatten();
            board.seen = h.version;
            (g.core.inside() || h.in_city(), view)
        }
        None => (false, None),
    };
    // Up on the wall in the colony, once there's a board to show.
    let known = board.shown.is_some() || view.is_some();
    for mut v in &mut wall {
        v.set_if_neq(if shown && known { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(view) = view.filter(|v| board.shown.as_ref() != Some(v)) {
        for (which, mut text, mut color) in &mut texts {
            let (s, c) = match *which {
                BoardText::Heading(c) => (column(&view, c).0, CYAN),
                BoardText::Row(c, k) => match column(&view, c).1.get(k) {
                    Some(r) => (row_text(k, r), if r.you { YOU } else { WHITE }),
                    None if k == 0 => ("    nobody yet today".to_string(), DIM),
                    None => (String::new(), WHITE),
                },
                BoardText::Record(c) => (column(&view, c).2, DIM),
            };
            if text.0 != s {
                text.0 = s;
            }
            if color.0 != c {
                color.0 = c;
            }
        }
        board.shown = Some(view);
        board.frames = DRAW_FRAMES;
    }
    // Drawn only for a few frames after it changes (and while it's up on the wall).
    let draw = board.frames > 0 && shown;
    if draw {
        board.frames -= 1;
    }
    if let Ok(mut cam) = cameras.get_mut(board.camera)
        && cam.is_active != draw
    {
        cam.is_active = draw;
    }
}
