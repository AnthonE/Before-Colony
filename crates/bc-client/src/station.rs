//! The colony's station language, drawn (`bc_client_core::station`, `docs/RING.md`), and the Ring
//! Deck in it (`bc_client_core::ring_deck`), a suit-scale test set (`?showcase=ring`).
//!
//! - **The materials** ([`Stations`]): `shaders/station.wgsl` paints a face from what it carries
//!   (white panels, the band in the lines' colours and its arrows, lanes, hazard stripes, light
//!   strips, lit frames), at a suit's scale for the deck and a person's for the Muster (`base`), each
//!   material made once at start-up and never changed. Under `?look=anime` they're painted as the
//!   hull's are (`bc::anime`).
//! - **The signs** ([`SignAtlas`]): a camera of its own lays every letter out once in a texture with
//!   the UI (the page's Chakra Petch), as the Proving Ground's board is drawn, and measures each; a
//!   sign is its letters' quads on its face (`station::lay_out`), every sign of an ink one mesh.
//! - **The deck's glass** looks out and down, onto the sky itself (the deck is on the world's layer,
//!   far from anything in it), turning as the colony turns and dimmed to the deck's exposure
//!   ([`SkyTurn`]), so Earth wheels past under the rail.
//!
//! The deck is built the first time it's shown ([`Indoors`] on the [`IndoorSet::Deck`]); the game
//! never shows it.

use bc_client_core::ring_deck::{self, DECK_ORIGIN, Layout};
use bc_client_core::station::{GLYPHS, PAINT, PAINTS, Scale, Sign, lay_out};
use bc_sim::TICK_HZ;
use bc_sim::world::colony_spin_angle;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;

use crate::city::to_mesh;
use crate::gfx::Gfx;
use crate::hangar::{Flood, IndoorSet, Indoors};
use crate::hud::UiFont;
use crate::sky::{EARTH_DIR, SkyTurn};
use crate::view::VisTime;

/// Camera exposure on the deck: brighter than a bay's (the colony's own halls are lit even and
/// high-key), and the ambient light round it, and its air.
pub const DECK_EV100: f32 = 9.5;
pub const DECK_AMBIENT: f32 = 300.0;
pub const DECK_FOG: Color = Color::srgb(0.3, 0.33, 0.37);
pub const DECK_FOG_DENSITY: f32 = 0.0018;
/// The sky's light through the rim's glass, against how it's drawn outside: the deck's exposure
/// would blow Earth out (the sky is in sunlight's units), so it's turned down by the difference,
/// leaving Earth twice as bright as from outside, a glow under the rail.
const SKY_GAIN: f32 = 2.0 / 32.0;

/// The ceiling's lamps and the floods, lumens.
const LAMP: f32 = 6.0e6;
const PASSAGE_LAMP: f32 = 2.5e6;
const FLOOD: f32 = 5.0e6;

/// The letters' texture: a cell each, `COLUMNS` across, each letter at its cell's top left, px.
const CELL: [f32; 2] = [160.0, 200.0];
const COLUMNS: usize = 12;
const ATLAS: [u32; 2] =
    [CELL[0] as u32 * COLUMNS as u32, CELL[1] as u32 * GLYPHS.len().div_ceil(COLUMNS) as u32];
const FONT: f32 = 150.0;
/// The layer the letters' camera draws (nothing in the world is on it: only the UI).
const LAYER: usize = 7;
/// How many frames the letters' camera draws once they're laid out.
const DRAW_FRAMES: u32 = 4;
/// How far a sign stands off its face, m.
const OFF: f32 = 0.03;

pub type StationMaterial = ExtendedMaterial<StandardMaterial, StationExt>;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct StationExt {
    #[uniform(100)]
    pub station: StationParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct StationParams {
    /// rgb: the palette, linear (`station::PAINT`).
    pub paint: [Vec4; PAINTS],
    /// x: seconds.
    pub time: Vec4,
    /// The sizes it paints at (`station::Scale::uniform`).
    pub size: Vec4,
    pub band: Vec4,
    pub lanes: Vec4,
    pub marks: Vec4,
    /// x: 1 to paint it (`?look=anime`); y: the kick plate's paint.
    pub look: Vec4,
}

impl StationParams {
    fn new(scale: &Scale, painted: bool) -> Self {
        let ([size, band, lanes, marks], kick) = scale.uniform();
        Self {
            paint: PAINT.map(|[r, g, b]| Vec4::new(r, g, b, 1.0)),
            time: Vec4::ZERO,
            size: Vec4::from_array(size),
            band: Vec4::from_array(band),
            lanes: Vec4::from_array(lanes),
            marks: Vec4::from_array(marks),
            look: Vec4::new(if painted { 1.0 } else { 0.0 }, f32::from(kick), 0.0, 0.0),
        }
    }
}

/// The station materials: a suit's deck's, and a person's halls' (the Muster's).
#[derive(Resource)]
pub struct Stations {
    pub deck: Handle<StationMaterial>,
    pub muster: Handle<StationMaterial>,
}

impl MaterialExtension for StationExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/station.wgsl".into()
    }
}

pub struct StationPlugin;

impl Plugin for StationPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/station.wgsl");
        app.add_plugins(MaterialPlugin::<StationMaterial>::default())
            .add_systems(Update, (measure_glyphs, show_deck, hang_deck_signs, turn_sky).chain());
    }
}

/// The deck, once built, and whether its signs are up.
#[derive(Resource)]
struct Deck {
    root: Entity,
    hung: bool,
}

/// The letters' texture, and what each letter measured once the UI laid it out.
#[derive(Resource)]
pub struct SignAtlas {
    camera: Entity,
    image: Handle<Image>,
    /// Each letter's size in the texture, px (its advance and a line's height), once laid out.
    sizes: Option<Vec<Vec2>>,
    frames: u32,
    /// The material for each ink drawn so far.
    inks: Vec<(u8, Handle<StandardMaterial>)>,
}

/// A letter's cell in the texture.
#[derive(Component)]
struct GlyphCell(usize);

impl SignAtlas {
    /// Whether its letters are laid out and measured, so signs can go up.
    pub fn ready(&self) -> bool {
        self.sizes.is_some()
    }

    /// Hangs `signs` under `parent` (in its frame): every sign of an ink one mesh, its letters'
    /// quads in a row on its face. Nothing if the letters aren't measured yet.
    pub fn hang(
        &mut self,
        signs: &[Sign],
        parent: Entity,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        standard: &mut Assets<StandardMaterial>,
    ) {
        let Some(sizes) = &self.sizes else { return };
        let aspect: Vec<f32> = sizes.iter().map(|s| s.x / s.y).collect();
        let [aw, ah] = ATLAS.map(|v| v as f32);
        let mut inks: Vec<u8> = signs.iter().map(|s| s.ink).collect();
        inks.sort_unstable();
        inks.dedup();
        for ink in inks {
            let (mut p, mut n, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for s in signs.iter().filter(|s| s.ink == ink) {
                for q in lay_out(s, &aspect, OFF) {
                    let (col, row) = (q.glyph % COLUMNS, q.glyph / COLUMNS);
                    let o = Vec2::new(col as f32 * CELL[0], row as f32 * CELL[1]);
                    let (u0, v0) = (o.x / aw, o.y / ah);
                    let (u1, v1) = ((o.x + sizes[q.glyph].x) / aw, (o.y + sizes[q.glyph].y) / ah);
                    let base = p.len() as u32;
                    p.extend(q.corners.map(|c| c.to_array()));
                    n.extend([s.normal.to_array(); 4]);
                    uv.extend([[u0, v0], [u0, v1], [u1, v1], [u1, v0]]);
                    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
                .with_inserted_indices(Indices::U32(idx));
            let material = match self.inks.iter().find(|(i, _)| *i == ink) {
                Some((_, m)) => m.clone(),
                None => {
                    let [r, g, b] = PAINT[usize::from(ink) % PAINTS];
                    let m = standard.add(StandardMaterial {
                        base_color: Color::linear_rgb(r, g, b),
                        base_color_texture: Some(self.image.clone()),
                        alpha_mode: AlphaMode::Mask(0.5),
                        perceptual_roughness: 0.45,
                        ..default()
                    });
                    self.inks.push((ink, m.clone()));
                    m
                }
            };
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::default(),
                NotShadowCaster,
                ChildOf(parent),
            ));
        }
    }
}

/// Makes the station materials and lays the letters out (a start-up system, before anything is
/// built with them).
pub fn setup_stations(
    mut commands: Commands,
    gfx: Res<Gfx>,
    font: Res<UiFont>,
    mut stations: ResMut<Assets<StationMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let material = |scale: &Scale| ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.45, ..default() },
        extension: StationExt { station: StationParams::new(scale, gfx.paint) },
    };
    commands.insert_resource(Stations {
        deck: stations.add(material(&Scale::DECK)),
        muster: stations.add(material(&Scale::MUSTER)),
    });
    let [w, h] = ATLAS;
    let image = images.add(Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None));
    let camera = commands
        .spawn((
            Camera2d,
            Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: w as f32, height: h as f32 },
                ..OrthographicProjection::default_2d()
            }),
            RenderTarget::Image(image.clone().into()),
            RenderLayers::layer(LAYER),
        ))
        .id();
    commands
        .spawn((
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(w as f32),
                height: Val::Px(h as f32),
                ..default()
            },
        ))
        .with_children(|root| {
            for (k, c) in GLYPHS.chars().enumerate() {
                root.spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px((k % COLUMNS) as f32 * CELL[0]),
                    top: Val::Px((k / COLUMNS) as f32 * CELL[1]),
                    width: Val::Px(CELL[0]),
                    height: Val::Px(CELL[1]),
                    align_items: AlignItems::Start,
                    justify_content: JustifyContent::Start,
                    ..default()
                })
                .with_children(|cell| {
                    cell.spawn((
                        GlyphCell(k),
                        Text::new(c.to_string()),
                        font.heading(FONT),
                        TextColor(Color::WHITE),
                        TextLayout::default().with_no_wrap(),
                    ));
                });
            }
        });
    commands.insert_resource(SignAtlas { camera, image, sizes: None, frames: DRAW_FRAMES, inks: Vec::new() });
}

/// Measures the letters once the UI has laid them out, and lets their camera rest a few frames on.
fn measure_glyphs(
    atlas: Option<ResMut<SignAtlas>>,
    cells: Query<(&GlyphCell, &ComputedNode)>,
    mut cameras: Query<&mut Camera>,
) {
    let Some(mut atlas) = atlas else { return };
    if atlas.sizes.is_none() {
        let mut sizes = vec![Vec2::ZERO; GLYPHS.len()];
        for (cell, node) in &cells {
            if let Some(s) = sizes.get_mut(cell.0) {
                *s = node.size().min(Vec2::from(CELL));
            }
        }
        if sizes.iter().any(|s| s.x < 1.0 || s.y < 1.0) {
            return;
        }
        atlas.sizes = Some(sizes);
    }
    let draw = atlas.frames > 0;
    if draw {
        atlas.frames -= 1;
    }
    if let Ok(mut cam) = cameras.get_mut(atlas.camera)
        && cam.is_active != draw
    {
        cam.is_active = draw;
    }
}

/// Builds the deck the first time it's wanted, and shows it while it is.
#[allow(clippy::too_many_arguments)]
fn show_deck(
    mut commands: Commands,
    indoors: Res<Indoors>,
    set: Res<IndoorSet>,
    deck: Option<Res<Deck>>,
    stations: Res<Stations>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut vis: Query<&mut Visibility>,
) {
    let want = indoors.0 && *set == IndoorSet::Deck;
    if let Some(deck) = deck {
        if let Ok(mut v) = vis.get_mut(deck.root) {
            v.set_if_neq(if want { Visibility::Inherited } else { Visibility::Hidden });
        }
        return;
    }
    if !want {
        return;
    }
    let layout = Layout::new();
    let material = stations.deck.clone();
    let glass = standard.add(StandardMaterial {
        base_color: Color::srgba(0.5, 0.6, 0.68, 0.1),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.05,
        reflectance: 0.6,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let root = commands
        .spawn((Transform::from_translation(DECK_ORIGIN), Visibility::Inherited))
        .with_children(|deck| {
            deck.spawn((
                Mesh3d(meshes.add(to_mesh(layout.mesh()))),
                MeshMaterial3d(material),
                Transform::default(),
            ));
            deck.spawn((
                Mesh3d(meshes.add(to_mesh(layout.glass()))),
                MeshMaterial3d(glass),
                Transform::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            // The ceiling's lamps, two rows down the deck, and the passage's.
            for x in [-6.0, 12.0] {
                for z in [-72.0, -24.0, 24.0, 72.0] {
                    deck.spawn(lamp(Vec3::new(x, ring_deck::HEIGHT - 3.0, z), LAMP, 90.0));
                }
            }
            for x in [-10.0, 20.0] {
                deck.spawn(lamp(Vec3::new(x, 28.0, 110.0), PASSAGE_LAMP, 45.0));
            }
            // Floods on bay 07 and the suit before it, and down onto the rim.
            for (k, (at, to)) in [
                (Vec3::new(2.0, 30.0, -34.0), ring_deck::STANDING + Vec3::new(4.0, 2.0, -2.0)),
                (Vec3::new(-2.0, 30.0, 34.0), Vec3::new(-16.0, 0.0, 20.0)),
            ]
            .into_iter()
            .enumerate()
            {
                deck.spawn((
                    Flood(k),
                    SpotLight {
                        intensity: FLOOD,
                        range: 120.0,
                        radius: 0.4,
                        inner_angle: 0.3,
                        outer_angle: 0.62,
                        color: Color::srgb(1.0, 0.97, 0.92),
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(at).looking_at(to, Vec3::Y),
                ));
            }
        })
        .id();

    commands.insert_resource(Deck { root, hung: false });
}

fn lamp(at: Vec3, intensity: f32, range: f32) -> impl Bundle {
    (
        PointLight {
            intensity,
            range,
            radius: 0.6,
            color: Color::srgb(1.0, 0.98, 0.95),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(at),
    )
}

/// Hangs the deck's signs once it's built and the letters are measured.
fn hang_deck_signs(
    mut commands: Commands,
    deck: Option<ResMut<Deck>>,
    mut atlas: ResMut<SignAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut deck) = deck else { return };
    if deck.hung || !atlas.ready() {
        return;
    }
    atlas.hang(&Layout::new().signs, deck.root, &mut commands, &mut meshes, &mut standard);
    deck.hung = true;
}

/// While the deck's shown, the sky turns as the colony does (about its axis, x), Earth
/// passing under the rail once a turn, and is dimmed to the deck's exposure; elsewhere it's as it
/// was.
fn turn_sky(time: Res<VisTime>, indoors: Res<Indoors>, set: Res<IndoorSet>, mut sky: ResMut<SkyTurn>) {
    let want = if indoors.0 && *set == IndoorSet::Deck {
        let ticks = time.now.max(0.0) * f64::from(TICK_HZ);
        let spin =
            colony_spin_angle((ticks.floor() as u64 % u64::from(u32::MAX)) as u32, ticks.fract() as f32);
        // At no turn, Earth straight down through the glass and aft (the most of it through it).
        let phase = EARTH_DIR.z.atan2(-EARTH_DIR.y);
        SkyTurn { turn: Quat::from_rotation_x(phase - spin), gain: SKY_GAIN }
    } else {
        SkyTurn::default()
    };
    if *sky != want {
        *sky = want;
    }
}
