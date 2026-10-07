//! Cockpit HUD, drawn as a mobile suit's monitor: flight and armour readouts with the suit's damage
//! silhouette, weapons and the frame's special, missile lock, amber target corners and markers on
//! missiles tracking you, chevrons at the view's edge on what's off it, kill feed, the ZERO
//! System's recommendations, and cautions in a hazard-striped banner. In the page's fonts
//! (`UiFont`) and palette (`bc_client_core::palette`), on chamfered plates (`ui_panel`).
//!
//! The instruments (the panels) are one tree: chasing, it's drawn in the screen's corners; from the
//! cockpit, into the texture the cockpit's monitors show (`place_instruments`, `cockpit`).
//!
//! The crosshair is the aim. A weapon bears only within its mount's reach of the body's axis (a
//! hand's 50°, Neo-Bird's nose 2°), so while the suit is still turning onto the aim the crosshair
//! dims and a second marker shows where the primary weapon would fire. The velocity vector shows
//! which way the suit is drifting (`-o-`), or, moving backwards, the way it's drifting from
//! (`-x-`): relative to the body it's on, or to a landmark within 2 km.
//!
//! The bodies: a landmark within 3 km is named with its range and range rate (negative closing),
//! relative to its surface, and its hide spots marked (`<>`, from compiled content: they never say who is in one).
//! With the grip armed, a landing ring `( _ )` sits on the surface the suit is coming in on, green
//! when the next tick would catch it (the simulation's own test). On a body the flight panel says
//! how the suit stands, and its speed is over the body; the alerts say how well hidden it is.

use bc_client_core::palette;
use bc_client_core::surface::{LetGo, SurfaceHint, let_go, range_rate, surface_hint};
use bc_client_core::world::ObjectMotion;
use bc_client_core::{ClientCore, FeedLine};
use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRIP, MELEE, MODE};
use bc_proto::snapshot::{cover, ent_flags, own_flags, zero_mode};
use bc_proto::{ChunkKind, NO_CHUNK, NO_SLOT, OwnState, Part, PilotKind};
use bc_sim::bodies::Body;
use bc_sim::chunks;
use bc_sim::content::kits::CRASH_TICKS;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::{CATCH_SPEED, DOCK_CENTER, PRICE, REACH, hold_kg, material};
use bc_sim::content::systems::{DAMAGED, FAILED};
use bc_sim::content::{
    ArmSlot, FrameSpec, PLAYABLE_ORDER, SpecialKind, WeaponClass, frame, frame_name, weapon, weapon_name,
};
use bc_sim::content::{Kit, Kits, System, Systems};
use bc_sim::ground::{Footing, RELEASE_SPEED, STANCE};
use bc_sim::sim::LURK_SETTLE_TICKS;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bc_sim::zero::fire_control::intercept;
use bc_sim::zero::hypotheses::Maneuver;
use bevy::prelude::*;
use bevy::text::LetterSpacing;

use bc_model::cockpit::Show;
use bevy::ui_render::prelude::MaterialNode;

use crate::camera::{Chase, MainCamera};
use crate::input::{Aim, Controls};
use crate::net::{GameClient, now_s};
use crate::suits_vis::pilot_tag;
use crate::ui_panel::PanelMaterial;
use crate::view::DrawnBodies;

const fn colour(hex: palette::Hex) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgb(r, g, b)
}

const CYAN: Color = colour(palette::CYAN);
const AMBER: Color = colour(palette::AMBER);
const RED: Color = colour(palette::RED);
const GREEN: Color = colour(palette::GREEN);
const ZERO_PINK: Color = colour(palette::PINK);
const WHITE: Color = colour(palette::WHITE);
/// Target brackets on suits, then markers on missiles tracking the pilot.
const BRACKETS: usize = 24;
const MISSILE_MARKERS: usize = 8;
/// How many of the nearest suits' brackets carry a name and range.
const TAGGED: usize = 6;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum HudText {
    Status,
    Flight,
    Armor,
    Weapons,
    Feed,
    Zero,
    Alert,
    Salvage,
}

/// The HUD's root: shown only in the world.
#[derive(Component)]
pub struct HudRoot;

/// The screen marker on the chunk nearest the free hand.
#[derive(Component)]
pub struct GrabMarker;
/// The screen marker on the colony's dock (while there is something to sell).
#[derive(Component)]
pub struct DockMarker;
/// Where the primary weapon would fire, while the aim is beyond its reach.
#[derive(Component)]
pub struct BoreMarker;
/// The velocity vector.
#[derive(Component)]
pub struct VelocityMarker;
/// The landing ring: where the armed grip would land the suit.
#[derive(Component)]
pub struct LandingMarker;
/// A landmark's name, range and range rate (negative closing), while it's near.
#[derive(Component)]
pub struct LandmarkMarker(u8);
/// A hide spot: which landmark's, and which of its spots.
#[derive(Component)]
pub struct HideMarker(u8, u8);

/// A landmark is named, and its hide spots marked, within this range of its surface (m).
const LANDMARK_NEAR: f32 = 3_000.0;
/// The velocity vector is taken relative to a landmark within this range of its surface (m).
const RELATIVE_NEAR: f32 = 2_000.0;
/// With the grip armed this close to the colony's hull (m), the HUD says it can't grip there.
const HULL_NEAR: f32 = 500.0;
/// How long the HUD says the grip lost its hold (s), and that a hider was seen (s).
const LOST_SECS: f64 = 3.0;
const SEEN_SECS: f64 = 5.0;
/// Pale blue, for COLD; grey, for what's merely possible.
const PALE_BLUE: Color = Color::srgb(0.55, 0.72, 1.0);
const GREY: Color = Color::srgb(0.62, 0.66, 0.7);

/// The own suit and the bodies, as predicted: how it stands, what it could land on, which hide
/// spot it's in. What the HUD, the hints and the pause menu say of it.
#[derive(Clone, Copy, Debug)]
pub struct Footed {
    pub footing: Footing,
    pub body: Body,
    /// How high its origin rides over the ground, m.
    pub stance: f32,
    /// The hide spot it stands in.
    pub spot: Option<&'static str>,
    /// Flying: the surface its grip, armed, would land it on (the landing ring).
    pub hint: Option<SurfaceHint>,
    /// Flying within a kilometre of a surface it could land on.
    pub near: bool,
}

/// How the own suit stands with respect to the bodies, as predicted after the newest command;
/// what it could land on is the simulation's test for the next tick.
pub fn footed(core: &ClientCore) -> Footed {
    let p = &core.predict;
    let m = p.mover();
    let bodies = p.bodies(p.tick + 1);
    let spot = match (m.footing, m.anchor.body) {
        (Footing::Grounded, b @ Body::Landmark(k)) => bodies
            .hide_spot_of(b, m.anchor.local)
            .and_then(|s| p.landmarks().get(usize::from(k))?.hides.get(usize::from(s)))
            .map(|h| h.name),
        _ => None,
    };
    let flying = m.footing == Footing::Free && core.world.own.is_some_and(|o| o.alive);
    Footed {
        footing: m.footing,
        body: m.anchor.body,
        stance: m.anchor.stance,
        spot,
        hint: if flying { surface_hint(&bodies, &m.flight) } else { None },
        near: flying && bodies.nearest_grippable(&m.flight, 1_000.0, f32::INFINITY, f32::INFINITY).is_some(),
    }
}

/// Whether the own suit, just let go of `body`, climbed out of its grip on the thrusters: Space
/// held (so it rose past the release height), the body still there, and the suit no faster over it
/// than a grip holds. Any other letting go, still armed, the pilot didn't ask for.
fn lifted_off(core: &ClientCore, body: Body) -> bool {
    let p = &core.predict;
    let f = &p.mover().flight;
    let bodies = p.bodies(p.tick + 1);
    core.last_cmd.thrust[1] > 0
        && bodies.alive(body)
        && bodies.pose(body).is_some_and(|b| (f.vel - b.point_vel(f.pos)).length() <= RELEASE_SPEED)
}

/// The hide spot the own suit is in, as the server last had it: its name.
pub fn hide_spot(core: &ClientCore) -> Option<&'static str> {
    let own = core.world.own_sent()?;
    let s = own.surface.filter(|s| s.footing == bc_proto::snapshot::footing::GROUNDED)?;
    let bc_proto::BodyRef::Landmark(k) = s.body else { return None };
    let def = core.world.bodies.landmarks().get(usize::from(k))?;
    // The body's frame (the own state on a body is in it), as `Bodies::hide_spot_of` has it.
    def.hides.iter().find(|h| (own.pos - h.center).length() <= h.radius).map(|h| h.name)
}

/// How far a point is outside the colony's hull (m): to its side, or past an end.
fn off_the_hull(p: Vec3) -> f32 {
    let rel = p - COLONY_CENTER;
    let radial = Vec2::new(rel.y, rel.z).length() - COLONY_RADIUS;
    let axial = rel.x.abs() - COLONY_HALF_LENGTH;
    if axial <= 0.0 { radial.abs() } else { Vec2::new(axial, radial.max(0.0)).length() }
}

/// What the HUD remembers of hiding between frames.
#[derive(Default)]
pub struct Lurk {
    cover: u8,
    /// When the own suit began to settle into hiding (s).
    settling: Option<f64>,
    /// Seen (it fired, or was hit, while settled) until then (s).
    seen_until: f64,
    hits_taken: u32,
    /// How it stood last frame and on what, and when the grip last lost its hold (s).
    footing: Option<(Footing, Body)>,
    lost_at: Option<f64>,
}

/// The velocity vector's colour, and the speed below which it isn't shown (m/s).
const PALE_GREEN: Color = Color::srgb(0.75, 1.0, 0.85);
const DRIFT: f32 = 2.0;

/// Credits seen last frame, and the last sale: (amount, when).
#[derive(Default)]
pub struct Sales {
    credits: Option<u32>,
    last: Option<(u32, f64)>,
}

const ORES: [&str; 4] = ["NI-FE", "TITANIUM", "VOLATILES", "EXOTICS"];

/// A chunk's name on the HUD.
fn chunk_name(kind: ChunkKind) -> String {
    match kind {
        ChunkKind::Ore { ore } => format!("{} ORE", ORES[usize::from(ore) % 4]),
        ChunkKind::Limb { frame: f, part, .. } => {
            let part = ["HEAD", "TORSO", "L-ARM", "R-ARM", "LEGS", "BACKPACK"][part as usize];
            format!("{} {part}", frame_name(f).to_uppercase())
        }
        ChunkKind::Hulk { frame: f, .. } => format!("{} HULK", frame_name(f).to_uppercase()),
    }
}

fn tonnes(kg: u32) -> String {
    if kg < 1_000 { format!("{kg} kg") } else { format!("{:.1} t", kg as f32 / 1_000.0) }
}

#[derive(Component)]
pub struct Reticle;
/// A gun's spread: a ring round the crosshair that its shots land within.
#[derive(Component)]
pub struct SpreadRing;
#[derive(Component)]
pub struct LeadMarker;
#[derive(Component)]
pub struct Bracket(usize);

/// The fonts the HUD (and the page) is set in (`web/fonts`, SIL OFL): Share Tech Mono for readouts
/// and numbers, Chakra Petch for the panels' labels and the cautions.
#[derive(Resource, Clone)]
pub struct UiFont {
    pub mono: Handle<Font>,
    pub label: Handle<Font>,
}

impl UiFont {
    /// Adds the fonts the page loads to the app's fonts.
    pub fn load(fonts: &mut Assets<Font>) -> Self {
        let mono = include_bytes!("../../../web/fonts/ShareTechMono-Regular.ttf");
        let label = include_bytes!("../../../web/fonts/ChakraPetch-SemiBold.ttf");
        Self {
            mono: fonts.add(Font::from_bytes(mono.to_vec())),
            label: fonts.add(Font::from_bytes(label.to_vec())),
        }
    }

    /// Readouts.
    pub fn text(&self, size: f32) -> TextFont {
        TextFont { font: self.mono.clone().into(), font_size: FontSize::Px(size), ..default() }
    }

    /// Labels and cautions.
    pub fn heading(&self, size: f32) -> TextFont {
        TextFont { font: self.label.clone().into(), font_size: FontSize::Px(size), ..default() }
    }
}

/// A soft shadow under HUD text, so it reads over anything bright.
pub const SHADOW: TextShadow =
    TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.75) };

fn label(
    font: &UiFont,
    size: f32,
    color: Color,
    node: Node,
) -> (Text, TextFont, TextColor, TextShadow, Node) {
    (Text::new(""), font.text(size), TextColor(color), SHADOW, node)
}

/// A full-width row at `top`, centring what's in it.
fn banner(top: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        ..default()
    }
}

/// Centres a node on its `left`/`top` point (a symbol on the spot it marks).
fn centred() -> UiTransform {
    UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() }
}

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, Val::Px),
        right: right.map_or(Val::Auto, Val::Px),
        top: top.map_or(Val::Auto, Val::Px),
        bottom: bottom.map_or(Val::Auto, Val::Px),
        ..default()
    }
}

/// The instruments: the readouts that live on the cockpit's monitors in the first-person view, and
/// in the corners of the screen when chasing. One tree, drawn to whichever it is (`place_instruments`).
#[derive(Component)]
pub struct Instruments;

/// One of the instruments' panels, by the monitor it goes on.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Panel(pub Show);

/// A text's size on screen (px); on the cockpit's monitors it's drawn larger, to be read there.
#[derive(Component, Clone, Copy)]
pub struct BaseSize(f32);

/// A block of the suit's damage silhouette, coloured by its part's armour.
#[derive(Component, Clone, Copy)]
pub struct PartBlock(Part);

/// The caution banner the alerts come up in.
#[derive(Component)]
pub struct Caution;

/// A target bracket's corners and its tag (by the bracket's index).
#[derive(Component)]
pub struct BracketCorner(usize);
#[derive(Component)]
pub struct BracketTag(usize);

/// A marker at the edge of the view on something off it: a hostile close by, whoever is locking
/// on to the pilot, a missile tracking them.
#[derive(Component)]
pub struct EdgeMarker(usize);
const EDGE_MARKERS: usize = 12;

/// The looks the HUD's panels and cautions are drawn with.
#[derive(Resource, Clone)]
pub struct HudLooks {
    plate: Handle<PanelMaterial>,
    zero: Handle<PanelMaterial>,
    caution: Handle<PanelMaterial>,
    notice: Handle<PanelMaterial>,
}

impl HudLooks {
    pub fn new(panels: &mut Assets<PanelMaterial>) -> Self {
        Self {
            plate: panels.add(PanelMaterial::plate(PLATE, PLATE_EDGE, 12.0, 1.0)),
            zero: panels.add(PanelMaterial::plate(
                Color::srgba(0.09, 0.02, 0.08, 0.72),
                Color::srgba(1.0, 0.45, 0.8, 0.5),
                12.0,
                1.0,
            )),
            caution: panels.add(PanelMaterial::stripes(AMBER, INK, 6.0)),
            notice: panels.add(PanelMaterial::plate(PLATE, PLATE_EDGE, 8.0, 1.0)),
        }
    }
}

/// The damage silhouette's frame (sized up on the cockpit's monitors).
#[derive(Component)]
pub struct Silhouette;
const SILHOUETTE: Vec2 = Vec2::new(46.0, 80.0);

/// B's colours for the panels: the plate, its edge, its labels.
const PLATE: Color = Color::srgba(0.03, 0.07, 0.12, 0.78);
const PLATE_EDGE: Color = Color::srgba(0.62, 0.78, 0.9, 0.45);
const LABEL: Color = colour(palette::LABEL);
const INK: Color = colour(palette::INK);

fn corner(i: usize, left: bool, top: bool) -> impl Bundle {
    let b = Val::Px(2.0);
    (
        BracketCorner(i),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(9.0),
            height: Val::Px(9.0),
            left: if left { Val::Px(0.0) } else { Val::Auto },
            right: if left { Val::Auto } else { Val::Px(0.0) },
            top: if top { Val::Px(0.0) } else { Val::Auto },
            bottom: if top { Val::Auto } else { Val::Px(0.0) },
            border: UiRect {
                left: if left { b } else { Val::Px(0.0) },
                right: if left { Val::Px(0.0) } else { b },
                top: if top { b } else { Val::Px(0.0) },
                bottom: if top { Val::Px(0.0) } else { b },
            },
            ..default()
        },
        BorderColor::all(AMBER),
    )
}

/// A panel of the instruments: a plate with a label along its top and the text below.
#[allow(clippy::too_many_arguments)]
fn panel(
    p: &mut ChildSpawnerCommands,
    f: &UiFont,
    looks: &HudLooks,
    show: Show,
    title: &str,
    text: HudText,
    color: Color,
    extra: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let look = if show == Show::Top { looks.zero.clone() } else { looks.plate.clone() };
    p.spawn((
        Panel(show),
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            row_gap: Val::Px(4.0),
            overflow: Overflow::clip(),
            ..default()
        },
        MaterialNode(look),
    ))
    .with_children(|p| {
        p.spawn((
            Text::new(title),
            f.heading(10.0),
            BaseSize(10.0),
            TextColor(if show == Show::Top { ZERO_PINK } else { LABEL }),
            LetterSpacing::Px(2.0),
        ));
        p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(12.0), ..default() })
            .with_children(|row| {
                extra(row);
                row.spawn((
                    text,
                    Text::new(""),
                    f.text(13.0),
                    BaseSize(13.0),
                    TextColor(color),
                    TextLayout::default().with_no_wrap(),
                ));
            });
    });
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
            BackgroundColor(Color::WHITE),
            BorderColor::all(Color::NONE),
        )
    };
    p.spawn((Silhouette, Node { width: Val::Px(SILHOUETTE.x), height: Val::Px(SILHOUETTE.y), ..default() }))
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

pub fn setup_hud(mut commands: Commands, font: Res<UiFont>, mut panels: ResMut<Assets<PanelMaterial>>) {
    let f = &*font;
    let looks = HudLooks::new(&mut panels);
    commands.insert_resource(looks.clone());
    // What's drawn over the view: the reticle and markers, target corners, cautions, the link.
    commands
        .spawn((
            HudRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((HudText::Status, label(f, 11.0, LABEL, abs(Some(14.0), None, Some(10.0), None))));
            p.spawn((HudText::Feed, label(f, 12.0, AMBER, abs(None, Some(14.0), Some(10.0), None))));
            p.spawn((
                GrabMarker,
                label(f, 13.0, GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                DockMarker,
                label(f, 13.0, AMBER, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                BoreMarker,
                label(f, 18.0, CYAN, abs(Some(0.0), None, Some(0.0), None)),
                centred(),
                Visibility::Hidden,
            ));
            p.spawn((
                VelocityMarker,
                label(f, 16.0, PALE_GREEN, abs(Some(0.0), None, Some(0.0), None)),
                centred(),
                Visibility::Hidden,
            ));
            p.spawn((
                LandingMarker,
                label(f, 18.0, GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            for (k, def) in LANDMARKS.iter().enumerate() {
                p.spawn((
                    LandmarkMarker(k as u8),
                    label(f, 13.0, CYAN, abs(Some(0.0), None, Some(0.0), None)),
                    Visibility::Hidden,
                ));
                for i in 0..def.hides.len() {
                    p.spawn((
                        HideMarker(k as u8, i as u8),
                        label(f, 12.0, AMBER, abs(Some(0.0), None, Some(0.0), None)),
                        Visibility::Hidden,
                    ));
                }
            }
            // Cautions, centred below the top of the view: hazard stripes round a dark plate.
            p.spawn(banner(Val::Percent(20.0))).with_children(|row| {
                row.spawn((
                    Caution,
                    Node { padding: UiRect::all(Val::Px(5.0)), ..default() },
                    MaterialNode(looks.caution.clone()),
                    Visibility::Hidden,
                ))
                .with_children(|b| {
                    b.spawn((
                        Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(5.0)), ..default() },
                        BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.88)),
                    ))
                    .with_children(|t| {
                        t.spawn((
                            HudText::Alert,
                            Text::new(""),
                            f.heading(20.0),
                            TextColor(RED),
                            SHADOW,
                            TextLayout::justify(Justify::Center),
                            LetterSpacing::Px(3.0),
                        ));
                    });
                });
            });
            p.spawn((
                Reticle,
                label(
                    f,
                    26.0,
                    CYAN,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        ..default()
                    },
                ),
                centred(),
            ));
            p.spawn((
                SpreadRing,
                Node {
                    position_type: PositionType::Absolute,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BorderColor::all(Color::srgba(0.55, 0.92, 1.0, 0.45)),
                Visibility::Hidden,
            ));
            p.spawn((
                LeadMarker,
                label(f, 16.0, ZERO_PINK, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            // Target corners, each with its tag to the right.
            for i in 0..BRACKETS + MISSILE_MARKERS {
                let size = if i < BRACKETS { 34.0 } else { 20.0 };
                p.spawn((
                    Bracket(i),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(size),
                        height: Val::Px(size),
                        ..default()
                    },
                    centred(),
                    Visibility::Hidden,
                ))
                .with_children(|b| {
                    for (left, top) in [(true, true), (false, true), (true, false), (false, false)] {
                        b.spawn(corner(i, left, top));
                    }
                    b.spawn((
                        BracketTag(i),
                        Text::new(""),
                        TextLayout::default().with_no_wrap(),
                        f.heading(11.0),
                        TextColor(AMBER),
                        SHADOW,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(size + 5.0),
                            top: Val::Px(-2.0),
                            ..default()
                        },
                    ));
                });
            }
            // Chevrons at the edge of the view, pointing at what's off it.
            for i in 0..EDGE_MARKERS {
                p.spawn((
                    EdgeMarker(i),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(14.0),
                        height: Val::Px(14.0),
                        border: UiRect { top: Val::Px(3.0), right: Val::Px(3.0), ..default() },
                        ..default()
                    },
                    BorderColor::all(RED),
                    UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() },
                    Visibility::Hidden,
                ));
            }
        });
    spawn_instruments(&mut commands, f, &looks);
}

/// The instruments (placed and targeted by `place_instruments`).
pub fn spawn_instruments(commands: &mut Commands, f: &UiFont, looks: &HudLooks) {
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
            panel(p, f, looks, Show::Top, "ZERO SYSTEM", HudText::Zero, ZERO_PINK, |_| {});
            panel(p, f, looks, Show::Suit, "SUIT", HudText::Armor, WHITE, silhouette);
            panel(p, f, looks, Show::Flight, "FLIGHT", HudText::Flight, WHITE, |_| {});
            panel(p, f, looks, Show::Arms, "ARMS", HudText::Weapons, WHITE, |_| {});
            panel(p, f, looks, Show::Log, "HOLD", HudText::Salvage, AMBER, |_| {});
        });
}

/// Where each panel goes: the corners of the screen (chasing), or its monitor's region of the
/// cockpit's screen texture (from the seat), where its text is drawn larger to be read there.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn place_instruments(
    mut commands: Commands,
    chase: Res<Chase>,
    cockpit: Option<Res<crate::cockpit::Cockpit>>,
    roots: Query<Entity, With<Instruments>>,
    mut panels: Query<(&Panel, &mut Node, &mut UiTransform), Without<Instruments>>,
    mut root_nodes: Query<&mut Node, (With<Instruments>, Without<Panel>)>,
    mut texts: Query<(&BaseSize, &mut TextFont)>,
    mut silhouettes: Query<&mut Node, (With<Silhouette>, Without<Panel>, Without<Instruments>)>,
    mut last: Local<Option<bool>>,
) {
    let inside = chase.cockpit() && cockpit.is_some();
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
    let scale = if inside { 2.0 } else { 1.0 };
    for (base, mut font) in &mut texts {
        font.font_size = FontSize::Px(base.0 * scale);
    }
    for mut n in &mut silhouettes {
        n.width = Val::Px(SILHOUETTE.x * scale * 1.3);
        n.height = Val::Px(SILHOUETTE.y * scale * 1.3);
    }
    let design = bc_model::cockpit::build_screens();
    for (p, mut node, mut ui) in &mut panels {
        let reset = |node: &mut Node| {
            node.left = Val::Auto;
            node.right = Val::Auto;
            node.top = Val::Auto;
            node.bottom = Val::Auto;
            node.width = Val::Auto;
            node.height = Val::Auto;
        };
        reset(&mut node);
        *ui = UiTransform::default();
        if inside {
            if let Some(s) = design.iter().find(|s| s.show == p.0) {
                // Its region, inset a little from the monitor's rim.
                node.left = Val::Px(s.region[0] * tw + 10.0);
                node.top = Val::Px(s.region[1] * th + 10.0);
                node.width = Val::Px((s.region[2] - s.region[0]) * tw - 20.0);
                node.height = Val::Px((s.region[3] - s.region[1]) * th - 20.0);
            }
            continue;
        }
        match p.0 {
            // Below the kill feed, clear of the cautions.
            Show::Top => {
                node.top = Val::Px(112.0);
                node.right = Val::Px(14.0);
            }
            Show::Suit => {
                node.left = Val::Px(14.0);
                node.bottom = Val::Px(12.0);
            }
            Show::Flight => {
                node.left = Val::Px(238.0);
                node.bottom = Val::Px(12.0);
            }
            Show::Arms => {
                node.right = Val::Px(14.0);
                node.bottom = Val::Px(12.0);
            }
            Show::Log => {
                node.right = Val::Px(14.0);
                node.bottom = Val::Px(150.0);
            }
        }
    }
}

pub(crate) fn bar(f: f32, n: usize) -> String {
    let k = ((f.clamp(0.0, 1.0) * n as f32).round() as usize).min(n);
    format!("{}{}", "|".repeat(k), "·".repeat(n - k))
}

fn km(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

/// Seconds, from ticks.
fn secs(ticks: f32) -> f32 {
    ticks / bc_sim::TICK_HZ as f32
}

/// The frame's special on the H key, and its state: `asked` is whether MODE is held.
fn special_line(spec: &FrameSpec, o: &OwnState, asked: bool) -> Option<String> {
    let active = o.flags & own_flags::SPECIAL_ACTIVE != 0;
    let ready = o.weapon_ready & 8 != 0;
    let cooling = || format!("{:.0} s", secs(f32::from(o.special_cooldown) * 4.0).ceil());
    let timer = secs(f32::from(o.special_timer));
    let (name, state) = match spec.special {
        SpecialKind::None => return None,
        SpecialKind::Transform { to, .. } => {
            let name = if to == bc_proto::FrameId::WingZeroBird { "NEO-BIRD" } else { "MS MODE" };
            let state = if o.flags & own_flags::TRANSFORMING != 0 {
                format!("CHANGING {timer:.1} s")
            } else {
                "READY".into()
            };
            (name, state)
        }
        SpecialKind::HyperJammer { .. } => {
            let state = match (active, asked) {
                (true, _) => "JAMMING",
                // Held, but broken by firing, or short of energy.
                (false, true) => "SUPPRESSED",
                (false, false) => "OFF",
            };
            ("HYPER JAMMER", state.into())
        }
        SpecialKind::FullOpen { .. } => {
            let state = if active {
                format!("FIRING {timer:.1} s")
            } else if ready {
                "READY".into()
            } else {
                cooling()
            };
            ("FULL OPEN ATTACK", state)
        }
        SpecialKind::MeleeMove { .. } => {
            let state = if active {
                "STRIKING".into()
            } else if ready {
                "READY".into()
            } else {
                cooling()
            };
            ("CROSS CRUSHER", state)
        }
    };
    Some(format!("H   {name:<18} {state}\n"))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud(
    game: NonSend<GameClient>,
    controls: Res<Controls>,
    aim: Res<Aim>,
    chase: Res<Chase>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor)>,
    mut reticle: Query<
        (&mut Text, &mut TextColor),
        (With<Reticle>, Without<HudText>, Without<LeadMarker>, Without<Bracket>),
    >,
    mut lead: Query<
        (&mut Node, &mut Text, &mut Visibility),
        (With<LeadMarker>, Without<HudText>, Without<Bracket>, Without<Reticle>),
    >,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut markers: Query<
        (
            &mut Node,
            &mut Text,
            &mut TextColor,
            &mut Visibility,
            Has<GrabMarker>,
            Has<BoreMarker>,
            Has<VelocityMarker>,
            Has<LandingMarker>,
            Option<&LandmarkMarker>,
            Option<&HideMarker>,
        ),
        (
            Or<(
                With<GrabMarker>,
                With<DockMarker>,
                With<BoreMarker>,
                With<VelocityMarker>,
                With<LandingMarker>,
                With<LandmarkMarker>,
                With<HideMarker>,
            )>,
            Without<HudText>,
            Without<LeadMarker>,
            Without<Bracket>,
            Without<Reticle>,
        ),
    >,
    mut sales: Local<Sales>,
    mut lurk: Local<Lurk>,
    bodies: Res<DrawnBodies>,
    mut ui: ResMut<crate::page::Ui>,
    settings: Res<crate::settings::SettingsRes>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = now_s();
    let t = core.render_tick(now);
    // The lock-on's target, as drawn.
    let locked = game.hard.target(world, t).filter(|_| world.own.is_some_and(|o| o.alive));
    let own = world.own;
    let zero = world.zero;
    // Survival rules: the pilot flies what they built, and docks to go home.
    let survival = core.welcome.is_some_and(|w| w.survival);
    // Anime flight rules: the tank is the boost gauge.
    let anime = core.welcome.is_some_and(|w| w.anime);
    let mut set = |which: HudText, s: String, color: Option<Color>| {
        for (h, mut text, mut c) in &mut texts {
            if *h == which {
                text.0.clone_from(&s);
                if let Some(color) = color {
                    c.0 = color;
                }
            }
        }
    };

    // --- Status (top left). ---
    // As flown: the command the prediction flies (the autopilot's too, which flies unassisted when
    // G-strain builds); the server's word while ZERO has the controls.
    let assisted = match own {
        Some(o) if o.zero_mode == zero_mode::SEIZED => o.flags & own_flags::FLIGHT_ASSIST != 0,
        Some(_) if core.inputs.newest != 0 => core.last_cmd.buttons & FLIGHT_ASSIST != 0,
        _ => controls.flight_assist,
    };
    let fa = if assisted { "FA ON" } else { "FA OFF" };
    // The grip, likewise armed or not as flown.
    let grip = if core.inputs.newest != 0 { core.last_cmd.buttons & GRIP != 0 } else { controls.grip };
    let mode = if game.autopilot { format!("AUTOPILOT ({})", game.brain.name()) } else { "MANUAL".into() };
    // The ping is a default until the first snapshot measures it.
    let ping =
        if core.clock.synced() { format!("{:.0} ms", core.clock.rtt * 1_000.0) } else { "--".to_string() };
    set(
        HudText::Status,
        format!(
            "BEFORE COLONY  L1 COLONY CLUSTER\nLINK OK  ping {}  tick {}\n{}  {}\ncontacts {}  hits {}  kills {}  deaths {}",
            ping,
            world.tick,
            mode,
            fa,
            world.entities.iter().flatten().count(),
            world.my_hits,
            world.my_kills,
            world.my_deaths
        ),
        None,
    );

    // --- Flight, armour, weapons (bottom). ---
    if let Some(o) = own {
        // The suit as drawn: its form, speed and the pilot's body, all at the same moment.
        let view = core.own_view().copied();
        let form = view.map_or(o.frame, |v| v.frame);
        let spec = frame(form);
        let s = &core.predict.state;
        // The suit's stat sheet, from the snapshot as the server builds it: the tank and hold it
        // has, what's broken inside it.
        let tuned = bc_sim::tuning::own_tuning(&o);
        let tank = bc_sim::tuning::tank_cap(spec, &tuned);
        // On a body, the speed is over it.
        // On a body, the speed is over it; locked on, it's relative to the target.
        let (speed, g, strain, limited) = view.map_or((o.vel.length(), 0.0, o.g_strain, false), |v| {
            let speed = match (v.ground, locked) {
                (Some(g), _) => g.rel_vel.length(),
                (None, Some(p)) => (v.flight_vel - p.vel).length(),
                (None, None) => v.flight_vel.length(),
            };
            (speed, v.g, v.g_strain, v.g_limited)
        });
        let feet = footed(core);
        let stands = match feet.footing {
            Footing::Grounded if feet.stance < STANCE => "  CROUCHED".to_string(),
            Footing::Grounded => "  GROUNDED".to_string(),
            Footing::Aloft => {
                let alt = view.and_then(|v| v.ground).map_or(0.0, |g| g.height - STANCE);
                format!("  ALOFT  ALT {:.0} m", alt.max(0.0))
            }
            Footing::Free if grip => "  GRIP ARMED".to_string(),
            Footing::Free if locked.is_some() => "  LOCKED ON".to_string(),
            Footing::Free => String::new(),
        };
        set(
            HudText::Flight,
            format!(
                "{} {}\nSPD   {:>6.0} m/s{}\n{} {} {:>3.0}%{}\nHEAT  {} {:>3.0}%\nENGY  {} {:>3.0}%\nG     {:>4.1} g {:<3} STRAIN {}",
                bc_sim::content::frame_designation(form),
                frame_name(form).to_uppercase(),
                speed,
                stands,
                if anime { "BOOST" } else { "PROP " },
                bar(s.propellant / tank, 10),
                100.0 * s.propellant / tank,
                if tuned.leak_kg_s > 0.0 {
                    format!(" LEAK -{:.0} kg/s", tuned.leak_kg_s)
                } else if s.burst.cooldown > 0 {
                    // The burst step cooling down (double-tap a direction).
                    format!(" STEP {:.1}s", f32::from(s.burst.cooldown) * bc_sim::DT)
                } else {
                    " STEP".to_string()
                },
                bar(o.heat, 10),
                o.heat * 100.0,
                bar(o.energy, 10),
                o.energy * 100.0,
                g,
                // Flight assist holding the pilot under their G tolerance.
                if limited { "LIM" } else { "" },
                bar(strain, 8),
            ),
            None,
        );
        let names = ["HEAD", "TORSO", "L-ARM", "R-ARM", "LEGS", "BPACK"];
        let systems = Systems(o.systems);
        let gone = bc_sim::tuning::own_gone(&o);
        let mut armor = String::new();
        for (i, n) in names.iter().enumerate() {
            let f = o.parts[i];
            // Without the head's main camera the cockpit sees through the sub-camera.
            let state = match f <= 0.0 {
                true if i == Part::Head as usize && chase.sub_camera() => "SUB-CAM".to_string(),
                true => "LOST".to_string(),
                // The armour, and what's broken inside it.
                false => {
                    let broken: Vec<String> = System::of_part(Part::ALL[i])
                        .filter_map(|x| match systems.level(x, gone) {
                            DAMAGED => Some(format!("{} DMG", x.tag())),
                            FAILED => Some(format!("{} OUT", x.tag())),
                            _ => None,
                        })
                        .collect();
                    if broken.is_empty() { bar(f, 8) } else { format!("{} {}", bar(f, 8), broken.join(" ")) }
                }
            };
            armor.push_str(&format!("{n:<6}{state}\n"));
        }
        // What the suit is under: a scram, a concussion, a repair under way.
        let mut status = Vec::new();
        if o.scram > 0 {
            status.push(format!("SCRAM {:.1}s", f32::from(o.scram) / 30.0));
        }
        if o.concussed > 0 {
            status.push(format!("CONCUSSED {:.1}s", f32::from(o.concussed) / 30.0));
        }
        if let Some(x) = System::from_index(usize::from(o.repairing)) {
            status.push(format!("REPAIRING {} {:.0}s", x.tag(), f32::from(o.repair_left) * 8.0 / 30.0));
        }
        // A stim: the lift, then the crash.
        if o.stim > CRASH_TICKS {
            status.push(format!("STIM +1G {:.0}s", f32::from(o.stim - CRASH_TICKS) / 30.0));
        } else if o.stim > 0 {
            status.push(format!("CRASHING {:.0}s", f32::from(o.stim) / 30.0));
        }
        armor.push_str(&status.join(" · "));
        let worst = systems.worst(gone);
        let hull_color = if o.parts[Part::Torso as usize] < 0.3 || worst == FAILED {
            RED
        } else if worst == DAMAGED {
            AMBER
        } else {
            CYAN
        };
        set(HudText::Armor, armor, Some(hull_color));
        let mut w = String::new();
        // Inside the colony nothing fires, by its law and the sector's, but in the Blast Hall.
        if core.welcome.is_some_and(|wl| wl.interior) {
            if core.own_view().is_some_and(|v| bc_sim::colony::hall::weapons_free(v.pos)) {
                let hits = core.world.my_target_hits;
                w.push_str(&format!("WEAPONS FREE · THE BLAST HALL · TRAINING ROUNDS · TARGETS {hits}\n"));
            } else {
                w.push_str("WEAPONS SAFE · INSIDE THE COLONY\n");
            }
        }
        for (slot, key) in [(0usize, "LMB"), (1, "RMB"), (2, "F")] {
            if let Some(m) = spec.loadout[slot] {
                let ready = o.weapon_ready & (1 << slot) != 0;
                let ammo = if weapon(m.weapon).ammo > 0 {
                    format!(" {:>3}", o.ammo[slot.min(1)])
                } else {
                    String::new()
                };
                // The Twin Buster's charge, or a charged shot's once the tap is past (full: let go).
                let gun = weapon(m.weapon);
                let shown = match gun.charged {
                    Some(c) => o.charge * f32::from(c.full()) > f32::from(c.tap) + 0.5,
                    None => gun.charge_ticks > 0 && o.charge > 0.0,
                };
                let extra = if slot == 0 && shown {
                    let full = gun.charged.is_some() && o.charge > 0.99;
                    format!(" CHARGE {}{}", bar(o.charge, 6), if full { " FULL" } else { "" })
                } else {
                    String::new()
                };
                w.push_str(&format!(
                    "{key:<3} {:<18}{}{}{}\n",
                    weapon_name(m.weapon).to_uppercase(),
                    if ready { " RDY" } else { " ---" },
                    ammo,
                    extra
                ));
            }
        }
        if let Some(line) = special_line(spec, &o, core.last_cmd.buttons & MODE != 0) {
            w.push_str(&line);
        }
        if let Some(lock) = spec.lock_spec() {
            let line = if o.lock_target == NO_SLOT {
                "LOCK --  aim at a hostile".to_string()
            } else if o.flags & own_flags::LOCK_ACQUIRED != 0 {
                format!("LOCK {}  ACQUIRED", world.name_of(o.lock_target))
            } else {
                let progress = f32::from(o.lock_progress) / f32::from(lock.lock_ticks);
                format!("LOCK {}  {}", world.name_of(o.lock_target), bar(progress, 8))
            };
            w.push_str(&line);
            w.push('\n');
        }
        if o.flags & own_flags::ZERO_CAPABLE != 0 {
            let z = match o.zero_mode {
                zero_mode::ACTIVE => "ACTIVE",
                zero_mode::SEIZED => "SEIZED",
                zero_mode::LOCKOUT => "LOCKOUT",
                _ => "STANDBY (Z)",
            };
            w.push_str(&format!("ZERO {z}  STRAIN {}", bar(o.zero_strain, 8)));
            w.push('\n');
        }
        // The rack (survival): keys 1-4.
        if core.hangar.place.is_some() {
            let rack = Kits(o.kits);
            let line: Vec<String> = Kit::ALL
                .iter()
                .enumerate()
                .map(|(k, kit)| format!("{} {} {}", k + 1, kit.tag(), rack.get(*kit)))
                .collect();
            w.push_str(&format!("RACK {}", line.join("  ")));
        }
        set(HudText::Weapons, w, None);

        // --- Salvage (bottom middle). ---
        let hold = hold_kg(o.frame) + tuned.hold_kg;
        let cargo: u32 = o.cargo_kg.iter().map(|kg| u32::from(*kg)).sum();
        let mut sv = String::new();
        if hold > 0 {
            sv.push_str(&format!(
                "HOLD {} {}/{}",
                bar(cargo as f32 / hold as f32, 10),
                tonnes(cargo),
                tonnes(hold)
            ));
        } else {
            sv.push_str("NO HOLD");
        }
        if survival {
            // The hangar's credits; what this sortie has earned in bounties is paid on docking.
            sv.push_str(&format!("   CR {}", core.hangar.credits()));
            if o.credits > 0 {
                sv.push_str(&format!("  +{} bounty", o.credits));
            }
            sv.push('\n');
        } else {
            sv.push_str(&format!("   CR {}\n", o.credits));
            if let Some(before) = sales.credits
                && o.credits > before
            {
                sales.last = Some((o.credits - before, now));
            }
            sales.credits = Some(o.credits);
        }
        let base = spec.mass(core.predict.state.propellant);
        let accel = base / (base + o.extra_mass_kg as f32);
        match world.objects.get(usize::from(o.held)).and_then(Option::as_ref).filter(|_| o.held != NO_CHUNK) {
            Some(c) => {
                let fits = !matches!(c.desc.kind, ChunkKind::Hulk { .. })
                    && c.desc.mass_kg <= 2_500
                    && cargo + c.desc.mass_kg <= hold;
                sv.push_str(&format!(
                    "{} {} {}  {}T throw{}\n",
                    if fits { "IN HAND" } else { "TOWING" },
                    chunk_name(c.desc.kind),
                    tonnes(c.desc.mass_kg),
                    if fits { "B stow  " } else { "" },
                    if accel < 0.995 { format!("   accel {:.0}%", accel * 100.0) } else { String::new() }
                ));
            }
            None => sv.push_str(if controls.grab {
                "GRAB ON (G)  free hand reaching\n"
            } else {
                "G grab   J jettison\n"
            }),
        }
        if let Some((amount, at)) = sales.last
            && now - at < 5.0
        {
            sv.push_str(&format!("SOLD +{amount} cr\n"));
        }
        if o.flags & own_flags::DOCKED != 0 {
            let on_hub = o.surface.is_some_and(|s| {
                s.body == bc_proto::BodyRef::Landmark(bc_sim::content::landmarks::DOCKING_HUB)
            });
            sv.push_str(if core.hangar.trainer {
                "ON THE GANTRY  ENTER: climb out\n"
            } else if survival && on_hub {
                "ON THE DECK HATCH  ENTER: into your bay\n"
            } else if survival {
                "IN THE DOCK  ENTER: into your bay\n"
            } else {
                "DOCKED  colony salvage yard\n"
            });
        }
        set(HudText::Salvage, sv, None);
    } else {
        set(HudText::Salvage, String::new(), None);
    }

    // --- ZERO panel. ---
    match zero {
        Some(z) => {
            let target =
                if z.rec_target != bc_proto::NO_SLOT { world.name_of(z.rec_target) } else { "-".into() };
            let level = ["LOW", "MODERATE", "HIGH", "LETHAL"][z.threat_level.min(3) as usize];
            let mut s = format!(
                "{}TARGET   {target}  {:.0}%\nMANEUVER {}  {:.0}%\nTHREAT   {level}  (conf {:.2})\nFLANKED  {:.0}%\n",
                if z.source_jev { "[Jev]\n" } else { "" },
                z.rec_target_p * 100.0,
                Maneuver::from_index(z.rec_maneuver as usize).own_label(),
                z.rec_maneuver_p * 100.0,
                z.threat_confidence,
                z.flanked * 100.0
            );
            for th in &z.threats[..z.threat_count as usize] {
                let mut best = 0;
                for k in 1..7 {
                    if th.probs[k] > th.probs[best] {
                        best = k;
                    }
                }
                s.push_str(&format!(
                    "{:<14} next {} {:.0}%\n",
                    world.name_of(th.slot),
                    Maneuver::from_index(best).label(),
                    th.probs[best] * 100.0
                ));
            }
            set(HudText::Zero, s, None);
        }
        // On the cockpit's top monitor, idle; chasing, the panel isn't shown.
        None => set(HudText::Zero, if chase.cockpit() { "STANDBY".into() } else { String::new() }, None),
    }

    // --- Kill feed. ---
    let mut feed = String::new();
    for line in world.feed.iter().rev().take(6) {
        match *line {
            FeedLine::Kill { victim, killer, .. } => {
                feed.push_str(&format!("{} > {}\n", world.name_of(killer), world.name_of(victim)))
            }
            FeedLine::Seizure { pilot, active, .. } => feed.push_str(&format!(
                "{} {}\n",
                world.name_of(pilot),
                if active { "ZERO SEIZURE" } else { "released" }
            )),
            FeedLine::Clash { a, b, .. } => {
                feed.push_str(&format!("{} x {} CLASH\n", world.name_of(a), world.name_of(b)))
            }
        }
    }
    set(HudText::Feed, feed, None);

    // --- Alerts. ---
    let drawn = core.own_view().copied();
    let own_pos = drawn.map_or(Vec3::ZERO, |v| v.pos);
    // Missiles tracking the pilot, nearest first.
    let mut incoming: Vec<(f32, Vec3)> = world
        .missiles()
        .filter(|m| m.latest.targets_you)
        .map(|m| {
            let p = m.pos_at(t);
            (p.distance(own_pos), p)
        })
        .collect();
    incoming.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Hiding, as the server has it: settling into it, and seen again for firing or being hit.
    let feet = footed(core);
    let cover_now = own.filter(|o| o.alive).map_or(cover::EXPOSED, |o| o.cover);
    if cover_now == cover::SETTLING && lurk.cover != cover::SETTLING {
        lurk.settling = Some(now);
    }
    let fought = core.last_cmd.buttons & (FIRE_PRIMARY | FIRE_SECONDARY | MELEE) != 0
        || world.hits_taken != lurk.hits_taken;
    if fought && matches!(lurk.cover, cover::COLD | cover::HIDDEN) {
        lurk.seen_until = now + SEEN_SECS;
    }
    if cover_now != cover::SETTLING {
        lurk.settling = None;
    }
    lurk.cover = cover_now;
    lurk.hits_taken = world.hits_taken;
    // The grip let go, still armed: on purpose, climbing out of it on the thrusters or changing
    // form, or of its own accord (too fast, the rock gone, blown off).
    if feet.footing == Footing::Free
        && grip
        && let Some((was, body)) = lurk.footing.filter(|f| f.0 != Footing::Free)
    {
        let form = &core.predict.form;
        let can_grip = frame(form.frame).has_legs() && !form.changing();
        match let_go(was, lifted_off(core, body), can_grip) {
            LetGo::Flying => ui.toast("FLYING"),
            LetGo::Transformed => {}
            LetGo::Lost => lurk.lost_at = Some(now),
        }
    }
    lurk.footing = Some((feet.footing, feet.body));
    let lurking = own.filter(|o| o.alive).and_then(|_| {
        let left = |since: f64| {
            let settle = f64::from(LURK_SETTLE_TICKS) / f64::from(bc_sim::TICK_HZ);
            (settle - (now - since)).ceil().max(1.0)
        };
        Some(match cover_now {
            _ if now < lurk.seen_until => (format!("SEEN {:.0}", (lurk.seen_until - now).ceil()), AMBER),
            cover::SETTLING => (format!("HIDING {:.0}", lurk.settling.map_or(3.0, left)), AMBER),
            cover::HIDDEN => match feet.spot.or(hide_spot(core)) {
                Some(spot) => (format!("HIDDEN - {spot}"), GREEN),
                None => ("HIDDEN".into(), GREEN),
            },
            cover::COLD => ("COLD".into(), PALE_BLUE),
            _ => return None,
        })
    });
    let own_now = drawn.filter(|v| v.alive).map(|v| v.pos);
    let landing = feet.hint.filter(|_| feet.footing == Footing::Free).map(|h| {
        if !grip {
            ("L - GRIP".to_string(), GREY)
        } else if h.catch {
            (format!("LAND {:.0} m {:.1} m/s", h.height.max(0.0), h.speed), GREEN)
        } else if h.speed > bc_sim::ground::CATCH_SPEED {
            (format!("TOO FAST {:.0} m/s", h.speed), AMBER)
        } else {
            (format!("LAND {:.0} m {:.1} m/s", h.height.max(0.0), h.speed), AMBER)
        }
    });
    // (Inside the colony, its city is a body like any: no hull spins under the suit.)
    let hull = grip
        && feet.footing == Footing::Free
        && !core.predict.interior()
        && own_now.is_some_and(|p| off_the_hull(p) < HULL_NEAR);
    let signed_in = core.welcome.is_some_and(|w| w.signed_in);
    let (alert, alert_color) = match own {
        Some(o) if !o.alive && survival => ("SUIT LOST\nthe colony's rescue boat is on its way".into(), RED),
        Some(o) if !o.alive => {
            let menu: Vec<String> = PLAYABLE_ORDER
                .iter()
                .enumerate()
                .map(|(k, f)| format!("[{}] {}", k + 1, frame_name(*f)))
                .collect();
            let respawn = f32::from(o.respawn_in) * 4.0 / 30.0;
            (
                format!(
                    "DESTROYED\nrespawn in {respawn:.0} s\n{}\n{}",
                    menu[..3].join("  "),
                    menu[3..].join("  ")
                ),
                RED,
            )
        }
        Some(o) if o.zero_mode == zero_mode::SEIZED => ("ZERO HAS THE CONTROLS".into(), RED),
        Some(o) if o.alive && drawn.is_some_and(|v| v.blackout) => ("G-LOC  BLACKOUT".into(), RED),
        Some(o) if o.flags & own_flags::MISSILE_INCOMING != 0 => {
            let near = incoming.first().map_or(String::new(), |(d, _)| format!("  {}", km(*d)));
            (format!("MISSILE{near}"), RED)
        }
        Some(o) if o.flags & own_flags::MISSILE_LOCK != 0 => ("MISSILE LOCK".into(), RED),
        Some(o) if o.flags & own_flags::LOCKED_ON != 0 => ("LOCK WARNING".into(), RED),
        // A blow just got through to something inside: say what (for two seconds).
        Some(o)
            if o.alive
                && let Some((_, _, sys, level)) =
                    world.system_hits.iter().rev().find(|(tick, target, ..)| {
                        *target == o.slot && world.tick.saturating_sub(*tick) < 60
                    }) =>
        {
            let name = System::from_index(usize::from(*sys)).map_or("SYSTEM", |x| x.name()).to_uppercase();
            let what = match (*sys, *level) {
                (s, _) if s == System::Reactor as u8 && o.scram > 0 => "SCRAM".to_string(),
                (s, _) if s == System::Cockpit as u8 && o.concussed > 0 => "HIT · CONCUSSED".to_string(),
                (_, FAILED) => "FAILED".to_string(),
                _ => "DAMAGED".to_string(),
            };
            (format!("{name} {what}"), if *level >= FAILED { RED } else { AMBER })
        }
        Some(o) if o.flags & own_flags::TRANSFORMING != 0 => ("TRANSFORMING".into(), CYAN),
        Some(o) if o.alive && lurk.lost_at.is_some_and(|at| now - at < LOST_SECS) => {
            ("GRIP LOST".into(), AMBER)
        }
        Some(_) if lurking.is_some() => lurking.unwrap_or_default(),
        Some(_) if landing.is_some() => landing.unwrap_or_default(),
        Some(_) if hull => ("HULL SPINS - NO GRIP".into(), GREY),
        // At rest on a body, signed in: leave now and the suit stays parked here, hidden; in a
        // hide spot, better hidden.
        Some(o) if o.flags & own_flags::PARKABLE != 0 && signed_in && feet.spot.is_some() => {
            ("HIDE SPOT - log off to leave your suit hidden".into(), GREEN)
        }
        Some(o) if o.flags & own_flags::PARKABLE != 0 && signed_in => {
            ("PARKED - safe to log off here".into(), GREEN)
        }
        _ => (String::new(), RED),
    };
    set(HudText::Alert, alert, Some(alert_color));
    // Where the primary weapon would fire: the aim, held within its mount's reach of the body's
    // axis (as the server fires it). Off the aim while the suit is still turning onto it; none
    // once the part carrying it is shot off.
    let bore = match (own, drawn) {
        (Some(o), Some(v)) if o.alive && v.alive => {
            frame(v.frame).loadout[0].filter(|m| o.parts[m.arm.part() as usize] > 0.0).map(|m| {
                let cone = bc_sim::tuning::cone(m.arm, &bc_sim::tuning::own_tuning(&o));
                bc_sim::math::clamp_to_cone(aim.dir, v.rot * Vec3::Z, cone)
            })
        }
        _ => None,
    }
    .filter(|b| b.angle_between(aim.dir) > 1f32.to_radians());
    if let Ok((mut r, mut color)) = reticle.single_mut() {
        r.0 = "+".into();
        color.0 = if bore.is_some() { Color::srgba(0.55, 0.92, 1.0, 0.35) } else { CYAN };
    }

    // --- Screen-space markers. ---
    let Ok((cam, cam_tf)) = camera.single() else { return };
    if let Ok((mut node, mut text, mut vis)) = lead.single_mut() {
        // ZERO's firing solution, which weighs the target's maneuvers; else, locked on, where a
        // shot from the primary meets the target if it flies on as it is (the ◆, and how long
        // the shot takes).
        // A full charge leads for the charged shot.
        let primary = own.and_then(|o| {
            let w = weapon(frame(o.frame).loadout[0]?.weapon);
            Some(match w.charged {
                Some(c) if o.charge > 0.99 => weapon(c.shot),
                _ => w,
            })
        });
        let mark = match (zero, own, locked, drawn) {
            (Some(z), Some(o), _, _) if z.has_solution && o.alive => {
                Some((own_pos + z.solution * 1_500.0, format!("[ ]{:.0}%", z.hit_p * 100.0), 16.0))
            }
            (_, Some(o), Some(p), Some(v)) if o.alive && settings.0.lead => primary
                .filter(|w| matches!(w.class, WeaponClass::Beam | WeaponClass::Ballistic))
                .and_then(|w| intercept(own_pos, v.flight_vel, w.speed, p.pos, p.vel, Vec3::ZERO))
                .map(|sol| (own_pos + sol.dir * 1_500.0, format!("◆ {:.2}s", sol.t), 6.0)),
            _ => None,
        };
        match mark.map(|(point, s, dx)| (cam.world_to_viewport(cam_tf, point), s, dx)) {
            Some((Ok(p), s, dx)) => {
                node.left = Val::Px(p.x - dx);
                node.top = Val::Px(p.y - 10.0);
                text.0 = s;
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    // The chunk nearest the free hand: green when it can be grabbed.
    let mut grab_at: Option<(Vec3, String, Color)> = None;
    let mut dock_at: Option<(Vec3, String)> = None;
    if let (Some(o), Some(view)) = (own.filter(|o| o.alive), drawn) {
        let rot = view.rot;
        let right = o.parts[Part::ArmL as usize] <= 0.0;
        let hand = own_pos + rot * if right { ArmSlot::Right } else { ArmSlot::Left }.muzzle();
        let vel = view.flight_vel;
        let held_by = Some((view.pos, view.rot));
        let mut best: Option<(f32, u16)> = None;
        if o.held == NO_CHUNK {
            for (id, c) in world.objects.iter().enumerate() {
                let Some(c) = c else { continue };
                if !matches!(c.motion, ObjectMotion::Free(_)) {
                    continue;
                }
                let Some((p, _)) = world.object_pose(id as u16, t, held_by) else { continue };
                let gap = p.distance(hand) - chunks::radius(&c.desc);
                if gap < 150.0 && best.is_none_or(|(g, _)| gap < g) {
                    best = Some((gap, id as u16));
                }
            }
        }
        if let Some((gap, id)) = best
            && let (Some(c), Some((p, _))) =
                (world.objects[usize::from(id)].as_ref(), world.object_pose(id, t, held_by))
        {
            let ObjectMotion::Free(seg) = c.motion else { unreachable!() };
            let catchable = gap <= REACH && (seg.vel - vel).length() <= CATCH_SPEED;
            let color = if catchable { GREEN } else { AMBER };
            grab_at = Some((
                p,
                format!(
                    "[{}] {} {}  {:.0} m",
                    if catchable { "G" } else { " " },
                    chunk_name(c.desc.kind),
                    tonnes(c.desc.mass_kg),
                    gap.max(0.0)
                ),
                color,
            ));
        }
        let cargo_value: u32 = o.cargo_kg.iter().enumerate().map(|(k, kg)| u32::from(*kg) * PRICE[k]).sum();
        let held_value = world
            .objects
            .get(usize::from(o.held))
            .and_then(Option::as_ref)
            .filter(|_| o.held != NO_CHUNK)
            .map_or(0, |c| c.desc.mass_kg * PRICE[material(c.desc.kind)]);
        if survival {
            // Home is always marked: at rest inside the dock's ring of lights, Enter. Inside the
            // colony that's the inner gate's ring, and for a trainer its gantry in the Blast Hall.
            let (at, name, home) = if core.hangar.trainer {
                (bc_sim::colony::hall::gantry(), "GANTRY", "climb out")
            } else if core.predict.interior() {
                (bc_sim::colony::interior::INNER_GATE, "INNER GATE", "home")
            } else {
                (DOCK_CENTER, "DOCK", "home")
            };
            let d = km(at.distance(own_pos));
            let text = if o.flags & own_flags::DOCKED != 0 {
                format!("{name} {d}  ENTER: {home}")
            } else {
                format!("{name} {d}")
            };
            dock_at = Some((at, text));
        } else if cargo_value + held_value > 0 {
            dock_at = Some((
                DOCK_CENTER,
                format!("DOCK {}  ~{} cr", km(DOCK_CENTER.distance(own_pos)), cargo_value + held_value),
            ));
        }
    }
    // Which way the suit drifts: ahead of the camera, or behind it (then the way it comes from).
    // On a body, over it; flying, relative to a landmark near enough to matter.
    let landmarks: Vec<(u8, bc_sim::bodies::BodyPose, f32, Vec3)> = bodies
        .set
        .landmarks()
        .iter()
        .enumerate()
        .filter_map(|(k, d)| {
            let pose = bodies.pose(Body::Landmark(k as u8))?;
            let at = own_now?;
            let pr = d.shape.probe(pose.to_local(at));
            Some((k as u8, pose, pr.dist, pose.rot * pr.normal))
        })
        .collect();
    let drift = drawn.map(|v| match (v.ground, locked) {
        (Some(g), _) => g.rel_vel,
        // Locked on: how the suit moves about its target.
        (None, Some(p)) => v.flight_vel - p.vel,
        (None, None) => landmarks
            .iter()
            .filter(|l| l.2 < RELATIVE_NEAR)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map_or(v.flight_vel, |l| v.flight_vel - l.1.point_vel(v.pos)),
    });
    let velocity = match (own, drawn, drift) {
        (Some(o), Some(v), Some(drift)) if o.alive && v.alive && drift.length() > DRIFT => {
            let dir = drift.normalize();
            let ahead = dir.dot(cam_tf.forward().as_vec3()) >= 0.0;
            Some(if ahead { (dir, "-o-") } else { (-dir, "-x-") })
        }
        _ => None,
    };
    // The landing ring, on the surface the armed grip would land the suit on.
    let ring = feet
        .hint
        .filter(|_| grip && feet.footing == Footing::Free && own_now.is_some())
        .map(|h| (h.point, "( _ )".to_string(), if h.catch { GREEN } else { AMBER }));
    for (mut node, mut text, mut color, mut vis, grab, is_bore, is_velocity, is_ring, landmark, hide) in
        &mut markers
    {
        let centred = is_bore || is_velocity || is_ring;
        let what = if is_bore {
            bore.map(|b| (own_pos + b * 1_500.0, "( )".to_string(), CYAN))
        } else if is_velocity {
            velocity.map(|(d, s)| (cam_tf.translation() + d * 1_500.0, s.to_string(), PALE_GREEN))
        } else if is_ring {
            ring.clone()
        } else if let Some(&LandmarkMarker(k)) = landmark {
            // Named while it's near, but not while standing on it.
            landmarks
                .iter()
                .find(|l| l.0 == k && l.2 < LANDMARK_NEAR && feet.body != Body::Landmark(k))
                .zip(drawn)
                .map(|(l, v)| {
                    let rate = range_rate(v.flight_vel - l.1.point_vel(v.pos), l.3);
                    let name = LANDMARKS[usize::from(k)].name;
                    (l.1.pos, format!("{name}  {}  {rate:+.0} m/s", km(l.2.max(0.0))), CYAN)
                })
        } else if let Some(&HideMarker(k, i)) = hide {
            // Not the one it stands in.
            landmarks.iter().find(|l| l.0 == k && l.2 < LANDMARK_NEAR).and_then(|l| {
                let spot = LANDMARKS[usize::from(k)].hides.get(usize::from(i))?;
                if feet.body == Body::Landmark(k) && feet.spot == Some(spot.name) {
                    return None;
                }
                let at = l.1.to_world(spot.center);
                Some((at, format!("<> {} {}", spot.name, km(at.distance(own_pos))), AMBER))
            })
        } else if grab {
            grab_at.clone()
        } else {
            dock_at.clone().map(|(p, s)| (p, s, AMBER))
        };
        match what.map(|(p, s, c)| (cam.world_to_viewport(cam_tf, p), s, c)) {
            Some((Ok(p), s, c)) => {
                // Symbols are centred on their point (their `UiTransform`); labels start just left
                // of theirs.
                let (dx, dy) = if centred { (0.0, 0.0) } else { (-20.0, -8.0) };
                node.left = Val::Px(p.x + dx);
                node.top = Val::Px(p.y + dy);
                text.0 = s;
                color.0 = c;
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}

/// Sizes the [`SpreadRing`] to the widest cone of the suit's guns (`bc_sim::tuning::scatter`), as
/// the camera draws it round the aim; hidden without a gun that spreads, or one so tight the
/// crosshair covers it.
pub fn update_spread_ring(
    game: NonSend<GameClient>,
    aim: Res<Aim>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut ring: Query<(&mut Node, &mut Visibility), With<SpreadRing>>,
) {
    let Ok((mut node, mut vis)) = ring.single_mut() else { return };
    let game = game.borrow();
    let spread = game.core.world.own.filter(|o| o.alive).map_or(0.0, |o| {
        frame(o.frame).loadout[..2]
            .iter()
            .flatten()
            .map(|m| weapon(m.weapon))
            .filter(|w| matches!(w.class, WeaponClass::Beam | WeaponClass::Ballistic))
            .map(|w| w.spread)
            .fold(0.0, f32::max)
    });
    let radius = camera.single().ok().filter(|_| spread > 0.0).and_then(|(cam, tf)| {
        let eye = tf.translation();
        let side = aim.dir.any_orthonormal_vector();
        let edge = aim.dir * spread.cos() + side * spread.sin();
        let c = cam.world_to_viewport(tf, eye + aim.dir * 1_000.0).ok()?;
        let e = cam.world_to_viewport(tf, eye + edge * 1_000.0).ok()?;
        Some((c, c.distance(e)))
    });
    match radius {
        Some((c, r)) if r >= 3.0 => {
            node.left = Val::Px(c.x - r);
            node.top = Val::Px(c.y - r);
            node.width = Val::Px(2.0 * r);
            node.height = Val::Px(2.0 * r);
            *vis = Visibility::Inherited;
        }
        _ => *vis = Visibility::Hidden,
    }
}

/// The HUD is the cockpit's: hidden on the title and while the link is down.
#[allow(clippy::type_complexity)]
pub fn show_hud(
    ui: Res<crate::page::Ui>,
    indoors: Res<crate::hangar::Indoors>,
    map: Res<crate::map::MapOpen>,
    mut root: Query<&mut Visibility, Or<(With<HudRoot>, With<Instruments>)>>,
) {
    // On foot in the bay the page draws what the pilot needs; the map, when it's open, takes the
    // screen (it marks the suits in sight itself).
    let want = if ui.playing() && !indoors.0 && !map.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(want);
    }
}

/// What a target bracket shows: where, its tag, its colour.
struct Mark {
    at: Vec2,
    tag: String,
    color: Color,
}

/// Target brackets on the nearest suits and on missiles tracking the pilot, and chevrons at the
/// edge of the view on what's off it that matters: a missile tracking them, whoever is locking on
/// to them, a hostile within [`NEAR`].
#[allow(clippy::type_complexity)]
pub fn update_marks(
    game: NonSend<GameClient>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut brackets: Query<(&Bracket, &mut Node, &mut Visibility), Without<EdgeMarker>>,
    mut corners: Query<(&BracketCorner, &mut BorderColor), Without<EdgeMarker>>,
    mut tags: Query<(&BracketTag, &mut Text, &mut TextColor)>,
    mut edges: Query<
        (&EdgeMarker, &mut Node, &mut UiTransform, &mut BorderColor, &mut Visibility),
        (Without<Bracket>, Without<BracketCorner>),
    >,
) {
    const NEAR: f32 = 2_000.0;
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let t = core.render_tick(now_s());
    let own = world.own;
    let zero = world.zero;
    let own_pos = core.own_view().map_or(Vec3::ZERO, |v| v.pos);
    let Ok((cam, cam_tf)) = camera.single() else { return };
    let Some(rect) = cam.logical_viewport_rect() else { return };
    // On the screen (in front and within the view), or off it.
    let project = |p: Vec3| cam.world_to_viewport(cam_tf, p).ok().filter(|v| rect.contains(*v));
    let mut marks: [Option<Mark>; BRACKETS + MISSILE_MARKERS] = std::array::from_fn(|_| None);
    let mut off: Vec<(Vec3, Color)> = Vec::new();
    let mut incoming: Vec<(f32, Vec3)> = world
        .missiles()
        .filter(|m| m.latest.targets_you)
        .map(|m| {
            let p = m.pos_at(t);
            (p.distance(own_pos), p)
        })
        .collect();
    incoming.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (k, &(d, p)) in incoming.iter().enumerate() {
        match project(p) {
            Some(at) if k < MISSILE_MARKERS => {
                marks[BRACKETS + k] = Some(Mark { at, tag: format!("MSL {}", km(d)), color: RED })
            }
            Some(_) => {}
            None => off.push((p, RED)),
        }
    }
    let mut shown: Vec<(f32, u16)> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(slot, tr)| {
            tr.as_ref().map(|tr| (tr.sample(t, &world.bodies).pos.distance(own_pos), slot as u16))
        })
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    let lock = own.filter(|o| o.alive && o.lock_target != NO_SLOT);
    let hard = game.hard.slot();
    let own_vel = core.own_view().map_or(Vec3::ZERO, |v| v.flight_vel);
    for (k, &(dist, slot)) in shown.iter().enumerate() {
        let Some(track) = world.entity(slot) else { continue };
        let e = &track.latest;
        let pos = track.sample(t, &world.bodies).pos;
        let hostile = e.faction != core.cfg.faction;
        let wreck = e.flags & ent_flags::WRECK != 0;
        let locked_on_you = e.flags & ent_flags::LOCKED_ON_YOU != 0;
        // The pilot's own lock-on: where it is always shows, on screen or off it.
        let locked_on = hard == Some(slot);
        let Some(at) = project(pos) else {
            if locked_on {
                off.push((pos, CYAN));
            } else if !wreck && (locked_on_you || (hostile && dist < NEAR)) {
                off.push((pos, if locked_on_you { RED } else { AMBER }));
            }
            continue;
        };
        if k >= BRACKETS {
            continue;
        }
        // Its pilot is offline, asleep in the cockpit.
        let asleep = e.flags & ent_flags::ASLEEP != 0 && !wreck;
        let zero_target = zero.is_some_and(|z| z.rec_target == slot);
        let warn = if locked_on_you { "  LOCK!" } else { "" };
        // The pilot's own missile lock on it: building, or acquired.
        let locking = lock.filter(|o| o.lock_target == slot).map(|o| match frame(o.frame).lock_spec() {
            _ if o.flags & own_flags::LOCK_ACQUIRED != 0 => "\nLOCKED".to_string(),
            Some(l) => format!("\n{}", bar(f32::from(o.lock_progress) / f32::from(l.lock_ticks), 6)),
            None => String::new(),
        });
        // Named: the nearest few, and any that matter (a lock either way, ZERO's pick).
        // Locked on: how fast it closes (+) or opens.
        let closing = locked_on.then(|| {
            let to = (pos - own_pos).normalize_or_zero();
            format!("\nLOCK-ON {:+.0} m/s", -(track.sample(t, &world.bodies).vel - own_vel).dot(to))
        });
        let named = k < TAGGED || locked_on_you || locking.is_some() || zero_target || locked_on;
        let tag = if !named {
            String::new()
        } else {
            format!(
                "{}{}{}\n{}{}{}{}{}",
                world.name_of(slot),
                pilot_tag(e.pilot),
                warn,
                km(dist),
                if e.pilot == PilotKind::Agent { " agent" } else { "" },
                if asleep { " ASLEEP" } else { "" },
                locking.as_deref().unwrap_or(""),
                closing.as_deref().unwrap_or("")
            )
        };
        let color = if wreck {
            Color::srgb(0.5, 0.5, 0.5)
        } else if locked_on {
            CYAN
        } else if locking.is_some() {
            AMBER
        } else if zero_target {
            ZERO_PINK
        } else if asleep {
            Color::srgb(0.55, 0.66, 0.8)
        } else if hostile {
            RED
        } else {
            GREEN
        };
        marks[k] = Some(Mark { at, tag, color });
    }
    for (b, mut node, mut vis) in &mut brackets {
        match &marks[b.0] {
            Some(m) => {
                node.left = Val::Px(m.at.x);
                node.top = Val::Px(m.at.y);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    for (c, mut border) in &mut corners {
        if let Some(m) = &marks[c.0] {
            *border = BorderColor::all(m.color);
        }
    }
    for (tag, mut text, mut color) in &mut tags {
        if let Some(m) = &marks[tag.0] {
            if text.0 != m.tag {
                text.0.clone_from(&m.tag);
            }
            color.0 = m.color;
        }
    }
    // The chevrons: from the middle of the view toward each, where that meets the view's edge.
    let centre = rect.center();
    let half = (rect.half_size() - Vec2::splat(26.0)).max(Vec2::splat(1.0));
    let to_cam = cam_tf.affine().inverse();
    for (m, mut node, mut ui, mut border, mut vis) in &mut edges {
        let Some(&(p, color)) = off.get(m.0) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        let local = to_cam.transform_point3(p);
        // Screen space: x right, y down. Straight behind reads as below.
        let d = Vec2::new(local.x, -local.y).try_normalize().unwrap_or(Vec2::Y);
        let reach = (half.x / d.x.abs().max(1e-4)).min(half.y / d.y.abs().max(1e-4));
        let at = centre + d * reach;
        node.left = Val::Px(at.x);
        node.top = Val::Px(at.y);
        ui.rotation = Rot2::radians(d.y.atan2(d.x) + std::f32::consts::FRAC_PI_4);
        *border = BorderColor::all(color);
        vis.set_if_neq(Visibility::Inherited);
    }
}

/// The caution banner (hazard stripes for a warning, a plain plate for a notice, hidden without
/// one), and the suit's damage silhouette.
#[allow(clippy::type_complexity)]
pub fn update_panels(
    game: NonSend<GameClient>,
    looks: Res<HudLooks>,
    alert: Query<(&HudText, &Text, &TextColor)>,
    mut caution: Query<(&mut MaterialNode<PanelMaterial>, &mut Visibility), With<Caution>>,
    mut blocks: Query<(&PartBlock, &mut BackgroundColor, &mut BorderColor)>,
    mut panels: Query<(&Panel, &mut Visibility), Without<Caution>>,
    chase: Res<Chase>,
) {
    let (text, color) = alert
        .iter()
        .find(|(h, ..)| **h == HudText::Alert)
        .map_or((true, RED), |(_, t, c)| (t.0.is_empty(), c.0));
    for (mut look, mut vis) in &mut caution {
        vis.set_if_neq(if text { Visibility::Hidden } else { Visibility::Inherited });
        let want = if color == RED { &looks.caution } else { &looks.notice };
        if look.0 != *want {
            look.0 = want.clone();
        }
    }
    let game = game.borrow();
    let own = game.core.world.own;
    let zero = game.core.world.zero.is_some();
    for (p, mut vis) in &mut panels {
        let shown = p.0 != Show::Top || zero || chase.cockpit();
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (b, mut fill, mut edge) in &mut blocks {
        let f = own.map_or(1.0, |o| o.parts[b.0 as usize]);
        // Whole, damaged, failing; lost is an outline.
        let (c, e) = if f <= 0.0 {
            (Color::NONE, RED)
        } else if f > 0.66 {
            (WHITE, Color::NONE)
        } else if f > 0.33 {
            (AMBER, Color::NONE)
        } else {
            (RED, Color::NONE)
        };
        fill.0 = c;
        *edge = BorderColor::all(e);
    }
}
