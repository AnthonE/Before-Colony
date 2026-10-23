//! The instruments: the panels of readouts on the cockpit's monitors in the first-person view, and
//! in the corners of the screen when chasing (`docs/UI.md`, rules 9 and 10). One tree, drawn to
//! whichever it is ([`place_instruments`]), each panel as large as it fits its monitor
//! ([`fit_instruments`]). What they show are the [`Readings`] (`bc_client_core::instruments`): the
//! game reads them off the suit (`hud::update_hud`), the showcase makes them up, and
//! [`update_instruments`] draws them: each panel's heading, its dials, its table of rows (a key, a
//! label, a gauge's bar, a number, a state) and its notes, and the suit's damage silhouette.

use bc_client_core::instruments::{DIAL_SEGMENTS, Gauge, ROWS, Tone, fit};
use bc_model::cockpit::Show;
use bc_proto::Part;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui_render::prelude::MaterialNode;

use crate::camera::Chase;
use crate::hud::{AMBER, CYAN, GREEN, HudLooks, HudText, LABEL, ORRERY_PINK, RED, UiFont, WHITE};
use crate::ui_panel::PanelMaterial;

/// The instruments: the readouts that live on the cockpit's monitors in the first-person view, and
/// in the corners of the screen when chasing. One tree, drawn to whichever it is (`place_instruments`).
#[derive(Component)]
pub struct Instruments;

/// One of the instruments' panels, by the monitor it goes on.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Panel(pub Show);

/// A text's size on a panel (px), at its scale of 1: on the cockpit's monitors it's drawn larger,
/// to fill them ([`fit_instruments`]).
#[derive(Component, Clone, Copy)]
pub struct BaseSize(f32);

/// A block of the suit's damage silhouette, coloured by its part's armour.
#[derive(Component, Clone, Copy)]
pub struct PartBlock(Part);

/// The damage silhouette's frame (sized up on the cockpit's monitors).
#[derive(Component)]
pub struct Silhouette;
const SILHOUETTE: Vec2 = Vec2::new(46.0, 80.0);

/// A panel's own size of text (px) or of a box (px; 0 leaves that side as it is), and its least
/// width: drawn at the panel's scale ([`fit_instruments`]).
#[derive(Component, Clone, Copy)]
pub struct BaseBox(Vec2);
#[derive(Component, Clone, Copy)]
pub struct BaseMin(f32);
/// What's drawn on a panel, scaled with it.
#[derive(Component, Clone, Copy)]
pub struct OnPanel(Show);
/// A panel's content, as large as it is: measured to fit it to its monitor ([`fit_instruments`]).
#[derive(Component, Clone, Copy)]
pub struct PanelBody(Show);
/// The word on the right of a panel's heading ([`bc_client_core::instruments::Reading::title`]).
#[derive(Component, Clone, Copy)]
pub struct TitleNote(Show);
/// A row of a panel's table, by its panel and its place in it ([`bc_client_core::instruments::Row`]), and its cells.
#[derive(Component, Clone, Copy)]
pub struct RowOf(Show, u8);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Key,
    Label,
    Bar,
    Value,
    State,
}
#[derive(Component, Clone, Copy)]
pub struct CellOf(Show, u8, Cell);
/// A row's key cap, round its [`Cell::Key`].
#[derive(Component, Clone, Copy)]
pub struct KeyCap(Show, u8);
/// One of the flight panel's dials: its ring, the number in it, its unit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DialPart {
    Ring,
    Number,
    Unit,
}
#[derive(Component, Clone, Copy)]
pub struct DialOf(u8, DialPart);

/// The instruments' readings (`bc_client_core::instruments`): the game reads them off the suit
/// (`hud::update_hud`), the showcase makes them up; [`update_instruments`] draws them.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct Readings(pub bc_client_core::instruments::Readings);

/// A panel's dial (px across), and its table's bars (px).
const DIAL: f32 = 60.0;
/// How a panel's table is laid out: whether its rows have keys, its labels' width, its bars' size.
#[derive(Clone, Copy)]
struct Table {
    keys: bool,
    label: f32,
    bar: Vec2,
    value: f32,
}

/// A text on a panel, in `font` at its size at the panel's scale of 1.
fn panel_text(show: Show, text: &str, font: TextFont, color: Color, justify: Justify) -> impl Bundle {
    let size = match font.font_size {
        FontSize::Px(px) => px,
        _ => 12.0,
    };
    (
        Text::new(text),
        font,
        BaseSize(size),
        OnPanel(show),
        TextColor(color),
        TextLayout::new(justify, LineBreak::NoWrap),
    )
}

fn tone(t: Tone) -> Color {
    match t {
        Tone::Plain => WHITE,
        Tone::Label => LABEL,
        Tone::Dim => LABEL.with_alpha(0.45),
        Tone::Cyan => CYAN,
        Tone::Amber => AMBER,
        Tone::Red => RED,
        Tone::Green => GREEN,
    }
}

/// A gauge's look: lit in its tone, its track the label blue, faint.
fn gauge_look(dial: bool, g: &Gauge) -> PanelMaterial {
    let track = LABEL.with_alpha(if dial { 0.16 } else { 0.14 });
    PanelMaterial::gauge(dial, g.segments, g.lit(), tone(g.tone), track, RED, g.mark, g.red_from)
}

/// A panel of the instruments: glass with its heading along its top (a short bar, its name and,
/// on the right, a word of its state) and what `body` puts under it.
fn panel(
    p: &mut ChildSpawnerCommands,
    f: &UiFont,
    looks: &HudLooks,
    show: Show,
    title: &str,
    body: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let look = if show == Show::Top { looks.orrery.clone() } else { looks.plate.clone() };
    p.spawn((
        Panel(show),
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        MaterialNode(look),
    ))
    .with_children(|p| {
        p.spawn((
            PanelBody(show),
            // Never squeezed into the panel: it's measured as large as it is, to fit the panel to it.
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: Val::Px(5.0),
                ..default()
            },
        ))
        .with_children(|p| {
            // Its title, after a short bar (`docs/UI.md`), and its state on the right.
            let accent = if show == Show::Top { ORRERY_PINK } else { CYAN };
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|head| {
                head.spawn((
                    Node { width: Val::Px(2.0), height: Val::Px(9.0), ..default() },
                    BackgroundColor(accent),
                ));
                let color = if show == Show::Top { ORRERY_PINK } else { LABEL };
                head.spawn((
                    panel_text(show, title, f.heading(10.0), color, Justify::Left),
                    LetterSpacing::Px(2.0),
                ));
                head.spawn(Node { flex_grow: 1.0, min_width: Val::Px(12.0), ..default() });
                head.spawn((
                    TitleNote(show),
                    panel_text(show, "", f.heading(9.0), LABEL, Justify::Left),
                    LetterSpacing::Px(1.0),
                ));
            });
            body(p);
        });
    });
}

/// A panel's table: [`ROWS`] rows, hidden past the ones its reading has.
fn table(p: &mut ChildSpawnerCommands, f: &UiFont, mats: &mut Assets<PanelMaterial>, show: Show, t: Table) {
    p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() })
        .with_children(|p| {
            for i in 0..ROWS as u8 {
                p.spawn((
                    RowOf(show, i),
                    Node {
                        display: Display::None,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                ))
                .with_children(|r| {
                    if t.keys {
                        // What to press, as a key (`docs/UI.md`, rule 8): its cap, the key
                        // in the middle of it.
                        r.spawn((
                            KeyCap(show, i),
                            Node {
                                min_width: Val::Px(24.0),
                                border: UiRect::all(Val::Px(1.0)),
                                padding: UiRect::axes(Val::Px(2.0), Val::Px(0.0)),
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BaseMin(24.0),
                            OnPanel(show),
                            BorderColor::all(LABEL.with_alpha(0.45)),
                        ))
                        .with_child((
                            CellOf(show, i, Cell::Key),
                            panel_text(show, "", f.heading(8.0), LABEL, Justify::Left),
                        ));
                    }
                    r.spawn((
                        CellOf(show, i, Cell::Label),
                        panel_text(show, "", f.heading(9.0), LABEL, Justify::Left),
                        Node { min_width: Val::Px(t.label), ..default() },
                        BaseMin(t.label),
                    ));
                    r.spawn((
                        CellOf(show, i, Cell::Bar),
                        MaterialNode(mats.add(gauge_look(false, &Gauge::new(0.0, 10, Tone::Cyan)))),
                        Node { width: Val::Px(t.bar.x), height: Val::Px(t.bar.y), ..default() },
                        BaseBox(t.bar),
                        OnPanel(show),
                    ));
                    // The number, at the right of its column (a text lines up only within itself).
                    r.spawn((
                        Node {
                            min_width: Val::Px(t.value),
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        },
                        BaseMin(t.value),
                        OnPanel(show),
                    ))
                    .with_child((
                        CellOf(show, i, Cell::Value),
                        panel_text(show, "", f.text(11.0), WHITE, Justify::Left),
                    ));
                    r.spawn((
                        CellOf(show, i, Cell::State),
                        panel_text(show, "", f.heading(9.0), CYAN, Justify::Left),
                    ));
                });
            }
        });
}

/// A dial of the flight panel: its ring of segments, the number in it and its unit under that.
fn dial(p: &mut ChildSpawnerCommands, f: &UiFont, mats: &mut Assets<PanelMaterial>, i: u8) {
    let show = Show::Flight;
    p.spawn((
        DialOf(i, DialPart::Ring),
        MaterialNode(mats.add(gauge_look(true, &Gauge::new(0.0, DIAL_SEGMENTS, Tone::Cyan)))),
        Node {
            width: Val::Px(DIAL),
            height: Val::Px(DIAL),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BaseBox(Vec2::splat(DIAL)),
        OnPanel(show),
    ))
    .with_children(|d| {
        d.spawn((DialOf(i, DialPart::Number), panel_text(show, "", f.text(16.0), WHITE, Justify::Center)));
        d.spawn((
            DialOf(i, DialPart::Unit),
            panel_text(show, "", f.heading(8.0), LABEL, Justify::Center),
            LetterSpacing::Px(1.0),
        ));
    });
}

/// A panel's notes, under or beside its table.
fn notes(p: &mut ChildSpawnerCommands, f: &UiFont, show: Show, text: HudText) {
    p.spawn((text, panel_text(show, "", f.text(10.0), LABEL, Justify::Left)));
}

/// The suit's damage silhouette: head, torso and backpack, arms and legs.
fn silhouette(p: &mut ChildSpawnerCommands) {
    let block = |part: Part, x: f32, y: f32, w: f32, h: f32| {
        (
            PartBlock(part),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(x),
                top: Val::Percent(y),
                width: Val::Percent(w),
                height: Val::Percent(h),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        )
    };
    p.spawn((
        Silhouette,
        Node { width: Val::Px(SILHOUETTE.x), height: Val::Px(SILHOUETTE.y), ..default() },
        BaseBox(SILHOUETTE),
        OnPanel(Show::Suit),
    ))
    .with_children(|s| {
        // The anime's proportions: a small head, a short torso, legs three-fifths of the height.
        s.spawn(block(Part::Backpack, 30.0, 12.0, 40.0, 10.0));
        s.spawn(block(Part::Head, 38.0, 0.0, 24.0, 10.0));
        s.spawn(block(Part::Torso, 28.0, 12.0, 44.0, 28.0));
        s.spawn(block(Part::ArmL, 2.0, 13.0, 22.0, 32.0));
        s.spawn(block(Part::ArmR, 76.0, 13.0, 22.0, 32.0));
        s.spawn(block(Part::Legs, 28.0, 42.0, 44.0, 58.0));
    });
}

/// The instruments (placed and targeted by `place_instruments`, sized by `fit_instruments`, drawn
/// by `update_instruments`).
pub fn spawn_instruments(
    commands: &mut Commands,
    f: &UiFont,
    looks: &HudLooks,
    mats: &mut Assets<PanelMaterial>,
) {
    commands
        .spawn((
            Instruments,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            panel(p, f, looks, Show::Top, "ORRERY", |p| {
                p.spawn((
                    HudText::Orrery,
                    panel_text(Show::Top, "", f.text(13.0), ORRERY_PINK, Justify::Left),
                ));
            });
            // The silhouette beside each part's armour, what's broken under it.
            panel(p, f, looks, Show::Suit, "SUIT", |p| {
                p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(12.0), ..default() })
                    .with_children(|row| {
                        silhouette(row);
                        row.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|col| {
                            let t =
                                Table { keys: false, label: 38.0, bar: Vec2::new(56.0, 5.0), value: 30.0 };
                            table(col, f, mats, Show::Suit, t);
                            notes(col, f, Show::Suit, HudText::Armor);
                        });
                    });
            });
            // The dials, how the suit stands beside them, and the gauges under them.
            panel(p, f, looks, Show::Flight, "FLIGHT", |p| {
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    dial(row, f, mats, 0);
                    dial(row, f, mats, 1);
                    notes(row, f, Show::Flight, HudText::Flight);
                });
                let t = Table { keys: false, label: 44.0, bar: Vec2::new(90.0, 5.0), value: 30.0 };
                table(p, f, mats, Show::Flight, t);
            });
            // A table of the weapons, the special, the lock and the Orrery (MechWarrior's).
            panel(p, f, looks, Show::Arms, "ARMS", |p| {
                let t = Table { keys: true, label: 96.0, bar: Vec2::new(48.0, 5.0), value: 24.0 };
                table(p, f, mats, Show::Arms, t);
                notes(p, f, Show::Arms, HudText::Weapons);
            });
            panel(p, f, looks, Show::Log, "HOLD", |p| {
                // Its numbers as wide as a hold's load, so the credits line up under it.
                let t = Table { keys: false, label: 30.0, bar: Vec2::new(72.0, 5.0), value: 70.0 };
                table(p, f, mats, Show::Log, t);
                notes(p, f, Show::Log, HudText::Salvage);
            });
        });
}

/// Where each panel goes: the corners of the screen (chasing), or its monitor's region of the
/// cockpit's screen texture (from the seat).
#[allow(clippy::type_complexity)]
pub fn place_instruments(
    mut commands: Commands,
    chase: Res<Chase>,
    cockpit: Option<Res<crate::cockpit::Cockpit>>,
    roots: Query<Entity, With<Instruments>>,
    mut panels: Query<(&Panel, &mut Node, &mut UiTransform), Without<Instruments>>,
    mut root_nodes: Query<&mut Node, (With<Instruments>, Without<Panel>)>,
    mut last: Local<Option<bool>>,
) {
    let inside = chase.in_cockpit() && cockpit.is_some();
    let Ok(root) = roots.single() else { return };
    if *last == Some(inside) {
        return;
    }
    *last = Some(inside);
    let [tw, th] = bc_model::cockpit::TEXTURE.map(|v| v as f32);
    match cockpit.filter(|_| inside) {
        Some(c) => {
            commands.entity(root).insert(UiTargetCamera(c.screen_camera));
            if let Ok(mut n) = root_nodes.get_mut(root) {
                n.width = Val::Px(tw);
                n.height = Val::Px(th);
            }
        }
        None => {
            commands.entity(root).remove::<UiTargetCamera>();
            if let Ok(mut n) = root_nodes.get_mut(root) {
                n.width = Val::Percent(100.0);
                n.height = Val::Percent(100.0);
            }
        }
    }
    let design = bc_model::cockpit::build_screens();
    for (p, mut node, mut ui) in &mut panels {
        node.left = Val::Auto;
        node.right = Val::Auto;
        node.top = Val::Auto;
        node.bottom = Val::Auto;
        node.width = Val::Auto;
        node.height = Val::Auto;
        *ui = UiTransform::default();
        if inside {
            if let Some(s) = design.iter().find(|s| s.show == p.0) {
                // Its region, inset a little from the monitor's rim.
                node.left = Val::Px(s.region[0] * tw + INSET);
                node.top = Val::Px(s.region[1] * th + INSET);
                node.width = Val::Px((s.region[2] - s.region[0]) * tw - 2.0 * INSET);
                node.height = Val::Px((s.region[3] - s.region[1]) * th - 2.0 * INSET);
            }
            continue;
        }
        // Flight beside the suit's, the hold over the arms' (`fit_instruments` keeps them there).
        match p.0 {
            // Below the kill feed, clear of the cautions.
            Show::Top => {
                node.top = Val::Px(112.0);
                node.right = Val::Px(14.0);
            }
            Show::Suit | Show::Flight => {
                node.left = Val::Px(14.0);
                node.bottom = Val::Px(12.0);
            }
            Show::Arms | Show::Log => {
                node.right = Val::Px(14.0);
                node.bottom = Val::Px(12.0);
            }
        }
    }
}

/// A panel's inset from its monitor's rim, and its padding (px).
const INSET: f32 = 10.0;
const PADDING: Vec2 = Vec2::new(12.0, 8.0);
/// Between the chase view's panels (px).
const PANEL_GAP: f32 = 10.0;

/// Sizes what's drawn on the panels. On the cockpit's monitors each panel's content is as large as
/// it fits its monitor, measured (`instruments::fit`: in steps, so a line coming and going doesn't
/// make it jitter): a hold's three lines fill their screen as the flight's dials and gauges do
/// theirs. Chasing, they're drawn at their own size, the flight's panel beside the suit's and the
/// hold's over the arms', however large those come out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn fit_instruments(
    chase: Res<Chase>,
    cockpit: Option<Res<crate::cockpit::Cockpit>>,
    bodies: Query<(&PanelBody, &ComputedNode)>,
    mut panels: Query<(&Panel, &ComputedNode, &mut Node)>,
    mut texts: Query<(&OnPanel, &BaseSize, &mut TextFont)>,
    mut boxes: Query<(&OnPanel, Option<&BaseBox>, Option<&BaseMin>, &mut Node), Without<Panel>>,
    mut screens: Local<Vec<bc_model::cockpit::Screen>>,
    mut drawn: Local<Option<(bool, [f32; 5])>>,
) {
    let inside = chase.in_cockpit() && cockpit.is_some();
    let at = |s: Show| s as usize;
    let mut scale = match *drawn {
        Some((was, scale)) if was == inside => scale,
        // Switched: from the start, and measured from the next frame.
        _ => [if inside { 2.0 } else { 1.0 }; 5],
    };
    let fresh = drawn.is_none_or(|(was, _)| was != inside);
    if inside && !fresh {
        if screens.is_empty() {
            *screens = bc_model::cockpit::build_screens();
        }
        let [tw, th] = bc_model::cockpit::TEXTURE.map(|v| v as f32);
        for (b, c) in &bodies {
            let Some(s) = screens.iter().find(|s| s.show == b.0) else { continue };
            let room = [
                (s.region[2] - s.region[0]) * tw - 2.0 * (INSET + PADDING.x),
                (s.region[3] - s.region[1]) * th - 2.0 * (INSET + PADDING.y),
            ];
            let size = c.size() * c.inverse_scale_factor();
            scale[at(b.0)] = fit(scale[at(b.0)], size.to_array(), room);
        }
    }
    if !inside {
        // The flight's panel beside the suit's, the hold's over the arms'.
        let size = |show: Show| {
            panels
                .iter()
                .find(|(p, ..)| p.0 == show)
                .map_or(Vec2::ZERO, |(_, c, _)| c.size() * c.inverse_scale_factor())
        };
        let (suit, arms) = (size(Show::Suit), size(Show::Arms));
        for (p, _, mut node) in &mut panels {
            match p.0 {
                Show::Flight => {
                    let left = Val::Px(14.0 + suit.x + PANEL_GAP);
                    if node.left != left {
                        node.left = left;
                    }
                }
                Show::Log => {
                    let bottom = Val::Px(12.0 + arms.y + PANEL_GAP);
                    if node.bottom != bottom {
                        node.bottom = bottom;
                    }
                }
                _ => {}
            }
        }
    }
    let changed: [bool; 5] =
        std::array::from_fn(|i| drawn.is_none_or(|(was, s)| was != inside || s[i] != scale[i]));
    *drawn = Some((inside, scale));
    if !changed.contains(&true) {
        return;
    }
    for (on, base, mut font) in &mut texts {
        if changed[at(on.0)] {
            font.font_size = FontSize::Px(base.0 * scale[at(on.0)]);
        }
    }
    for (on, size, least, mut node) in &mut boxes {
        if !changed[at(on.0)] {
            continue;
        }
        let k = scale[at(on.0)];
        if let Some(b) = size {
            if b.0.x > 0.0 {
                node.width = Val::Px(b.0.x * k);
            }
            if b.0.y > 0.0 {
                node.height = Val::Px(b.0.y * k);
            }
        }
        if let Some(m) = least {
            node.min_width = Val::Px(m.0 * k);
        }
    }
}

/// Draws the instruments' readings ([`Readings`]) on their panels: each panel's heading word, its
/// dials, its table's rows (hidden past the ones it has) and notes, and the suit's silhouette.
/// A gauge's look changes only when a segment lights or goes out, or its colour changes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_instruments(
    readings: Res<Readings>,
    mut rows: Query<(&RowOf, &mut Node), Without<CellOf>>,
    mut cells: Query<
        (&CellOf, &mut Text, &mut TextColor, &mut Node),
        (Without<RowOf>, Without<HudText>, Without<TitleNote>, Without<DialOf>),
    >,
    mut bars: Query<(&CellOf, &MaterialNode<PanelMaterial>, &mut Visibility), Without<Text>>,
    mut dial_texts: Query<
        (&DialOf, &mut Text, &mut TextColor),
        (Without<CellOf>, Without<HudText>, Without<TitleNote>),
    >,
    rings: Query<(&DialOf, &MaterialNode<PanelMaterial>), Without<Text>>,
    mut words: Query<
        (Option<&HudText>, Option<&TitleNote>, &mut Text, &mut TextColor),
        (Or<(With<HudText>, With<TitleNote>)>, Without<CellOf>, Without<DialOf>),
    >,
    mut caps: Query<(&KeyCap, &mut BorderColor), Without<PartBlock>>,
    mut blocks: Query<(&PartBlock, &mut BackgroundColor, &mut BorderColor), Without<KeyCap>>,
    mut mats: ResMut<Assets<PanelMaterial>>,
    mut looks: Local<bevy::platform::collections::HashMap<AssetId<PanelMaterial>, GaugeKey>>,
) {
    let r = &readings.0;
    let reading = |show: Show| match show {
        Show::Suit => Some(&r.suit),
        Show::Flight => Some(&r.flight),
        Show::Arms => Some(&r.arms),
        Show::Log => Some(&r.hold),
        Show::Top => None,
    };
    let put = |text: &mut Text, color: &mut TextColor, s: &str, c: Color| {
        if text.0 != s {
            text.0.clear();
            text.0.push_str(s);
        }
        if color.0 != c {
            color.0 = c;
        }
    };
    let mut look = |mat: &Handle<PanelMaterial>, dial: bool, g: &Gauge| {
        let key = GaugeKey::of(g);
        if looks.get(&mat.id()) != Some(&key) {
            looks.insert(mat.id(), key);
            if let Some(mut m) = mats.get_mut(mat) {
                *m = gauge_look(dial, g);
            }
        }
    };
    for (row, mut node) in &mut rows {
        let has = reading(row.0).is_some_and(|p| p.rows.len() > usize::from(row.1));
        let want = if has { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    for (cell, mut text, mut color, mut node) in &mut cells {
        let Some(row) = reading(cell.0).and_then(|p| p.rows.get(usize::from(cell.1))) else { continue };
        let (s, c, shown) = match cell.2 {
            Cell::Key => (row.key, LABEL, !row.key.is_empty()),
            Cell::Label => (row.label.as_str(), tone(row.label_tone), true),
            Cell::Value => (row.value.as_str(), tone(row.value_tone), true),
            Cell::State => (row.state.as_str(), tone(row.state_tone), !row.state.is_empty()),
            Cell::Bar => continue,
        };
        put(&mut text, &mut color, s, c);
        // A state's room goes without one.
        let want = if shown || cell.2 == Cell::Key { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    // A key's cap, drawn round a key (its room stays without one, so the labels line up).
    for (cap, mut edge) in &mut caps {
        let key = reading(cap.0)
            .and_then(|p| p.rows.get(usize::from(cap.1)))
            .is_some_and(|row| !row.key.is_empty());
        let e = BorderColor::all(if key { LABEL.with_alpha(0.45) } else { Color::NONE });
        if *edge != e {
            *edge = e;
        }
    }
    for (cell, mat, mut vis) in &mut bars {
        let gauge = reading(cell.0).and_then(|p| p.rows.get(usize::from(cell.1))).and_then(|row| row.gauge);
        // Without a bar its room stays, so the numbers line up.
        vis.set_if_neq(if gauge.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        if let Some(g) = gauge {
            look(&mat.0, false, &g);
        }
    }
    let dials = &r.flight.dials;
    for (d, mut text, mut color) in &mut dial_texts {
        let Some(dial) = dials.get(usize::from(d.0)) else {
            put(&mut text, &mut color, "", WHITE);
            continue;
        };
        match d.1 {
            // White, unless it warns.
            DialPart::Number => {
                let c = if matches!(dial.gauge.tone, Tone::Amber | Tone::Red) {
                    tone(dial.gauge.tone)
                } else {
                    WHITE
                };
                put(&mut text, &mut color, &dial.number, c);
            }
            DialPart::Unit => put(&mut text, &mut color, &dial.unit, tone(dial.unit_tone)),
            DialPart::Ring => {}
        }
    }
    for (d, mat) in &rings {
        let g = dials.get(usize::from(d.0)).map_or(Gauge::new(0.0, DIAL_SEGMENTS, Tone::Dim), |d| d.gauge);
        look(&mat.0, true, &g);
    }
    for (h, title, mut text, mut color) in &mut words {
        let (s, c) = match (h, title) {
            (_, Some(t)) => match reading(t.0) {
                Some(p) => (p.title.as_str(), tone(p.title_tone)),
                None => continue,
            },
            (Some(h), None) => {
                let p = match h {
                    HudText::Flight => &r.flight,
                    HudText::Armor => &r.suit,
                    HudText::Weapons => &r.arms,
                    HudText::Salvage => &r.hold,
                    _ => continue,
                };
                (p.notes.as_str(), tone(p.notes_tone))
            }
            (None, None) => continue,
        };
        put(&mut text, &mut color, s, c);
    }
    for (b, mut fill, mut edge) in &mut blocks {
        let f = r.parts[b.0 as usize];
        // Whole, damaged, failing, lit glass in their colours; lost, an outline.
        let (c, e) = if f <= 0.0 {
            (Color::NONE, RED.with_alpha(0.7))
        } else if f > 0.66 {
            (CYAN.with_alpha(0.22), CYAN.with_alpha(0.85))
        } else if f > 0.33 {
            (AMBER.with_alpha(0.35), AMBER)
        } else {
            (RED.with_alpha(0.4), RED)
        };
        if fill.0 != c {
            fill.0 = c;
        }
        let e = BorderColor::all(e);
        if *edge != e {
            *edge = e;
        }
    }
}

/// What a gauge's look is drawn from: it changes when a segment lights or goes out.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GaugeKey(u8, u8, Tone, i32, i32);

impl GaugeKey {
    fn of(g: &Gauge) -> Self {
        let q = |v: Option<f32>| v.map_or(-1, |v| (v * 1000.0).round() as i32);
        Self(g.lit(), g.segments, g.tone, q(g.mark), q(g.red_from))
    }
}
