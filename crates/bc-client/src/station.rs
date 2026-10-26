//! The Ring Deck, drawn (`bc_client_core::ring_deck`): the bay ring's suit deck in the colony's
//! clean language, a showcase set for now (`?showcase=ring`).
//!
//! - Its solids are one mesh, painted by `shaders/station.wgsl` from what each face carries (white
//!   panels, the band in the lines' colours and its arrows, the suits' lanes, hazard stripes, light
//!   strips, the bays' lit frames).
//! - Its signs are stencilled from one texture: a camera of its own lays every sign's words out
//!   in it once with the UI (the page's Chakra Petch), as the Proving Ground's board is drawn, and
//!   each sign is a quad on its wall that shows its row.
//! - The rim's glass looks out and down, onto the sky itself (the deck is on the world's layer,
//!   far from anything in it), turning as the ring turns and dimmed to the deck's exposure
//!   ([`SkyTurn`]), so Earth wheels past under the rail.
//!
//! It's built the first time it's shown ([`Indoors`] on the [`IndoorSet::Deck`]); the game never
//! shows it.

use bc_client_core::ring_deck::{self, DECK_ORIGIN, Layout, PAINT, Sign};
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

/// The signs' texture, px: a row each, the words at the row's left.
const ATLAS: [u32; 2] = [2048, 2048];
const ROW: f32 = 136.0;
const PAD: f32 = 16.0;
const FONT: f32 = 104.0;
/// The layer the signs' camera draws (nothing in the world is on it: only the UI).
const LAYER: usize = 7;
/// How many frames the signs' camera draws once their words are laid out.
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
    /// rgb: the palette, linear (`ring_deck::PAINT`).
    pub paint: [Vec4; 8],
    /// x: seconds.
    pub time: Vec4,
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
            .add_systems(Update, (show_deck, hang_signs, turn_sky).chain());
    }
}

/// The deck, once built.
#[derive(Resource)]
struct Deck {
    root: Entity,
    camera: Entity,
    /// The signs' words, a row each (a sign's row is its words'), and whether they're up yet.
    rows: Vec<&'static str>,
    hung: bool,
    frames: u32,
    atlas: Handle<Image>,
}

/// A row of the signs' texture: the words laid out there.
#[derive(Component)]
struct SignRow(usize);

fn ink(i: u8) -> Color {
    let [r, g, b] = PAINT[usize::from(i)];
    Color::linear_rgb(r, g, b)
}

/// Builds the deck the first time it's wanted, and shows it while it is.
#[allow(clippy::too_many_arguments)]
fn show_deck(
    mut commands: Commands,
    indoors: Res<Indoors>,
    set: Res<IndoorSet>,
    deck: Option<Res<Deck>>,
    font: Res<UiFont>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut stations: ResMut<Assets<StationMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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
    let material = stations.add(ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.45, ..default() },
        extension: StationExt {
            station: StationParams {
                paint: PAINT.map(|[r, g, b]| Vec4::new(r, g, b, 1.0)),
                time: Vec4::ZERO,
            },
        },
    });
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

    // The signs' words, a row each in a texture of their own.
    let mut rows: Vec<&'static str> = Vec::new();
    for s in &layout.signs {
        if !rows.contains(&s.text) {
            rows.push(s.text);
        }
    }
    let [w, h] = ATLAS;
    let atlas = images.add(Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None));
    let camera = commands
        .spawn((
            Camera2d,
            Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: w as f32, height: h as f32 },
                ..OrthographicProjection::default_2d()
            }),
            RenderTarget::Image(atlas.clone().into()),
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
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .with_children(|col| {
            for (k, text) in rows.iter().enumerate() {
                col.spawn(Node {
                    height: Val::Px(ROW),
                    padding: UiRect::left(Val::Px(PAD)),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        SignRow(k),
                        Text::new(*text),
                        font.heading(FONT),
                        TextColor(Color::WHITE),
                        TextLayout::default().with_no_wrap(),
                    ));
                });
            }
        });
    commands.insert_resource(Deck { root, camera, rows, hung: false, frames: DRAW_FRAMES, atlas });
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

/// Once the signs' words are laid out, hangs each sign on its face (a quad showing its row of the
/// texture, as tall as its sign says and as wide as its words come out), and lets the camera that
/// drew them rest a few frames on.
fn hang_signs(
    mut commands: Commands,
    deck: Option<ResMut<Deck>>,
    rows: Query<(&SignRow, &ComputedNode)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut cameras: Query<&mut Camera>,
) {
    let Some(mut deck) = deck else { return };
    if !deck.hung {
        let mut size = vec![Vec2::ZERO; deck.rows.len()];
        for (row, node) in &rows {
            if let Some(s) = size.get_mut(row.0) {
                *s = node.size();
            }
        }
        if size.iter().any(|s| s.x < 1.0 || s.y < 1.0) {
            return;
        }
        let mut inks: Vec<(u8, Handle<StandardMaterial>)> = Vec::new();
        let [aw, ah] = ATLAS.map(|v| v as f32);
        let layout = Layout::new();
        let signs: Vec<Entity> = layout
            .signs
            .iter()
            .map(|s| {
                let k = deck.rows.iter().position(|t| *t == s.text).unwrap_or(0);
                let px = size[k];
                let top = k as f32 * ROW + (ROW - px.y) * 0.5;
                let uv0 = Vec2::new(PAD / aw, top / ah);
                let uv1 = Vec2::new((PAD + px.x) / aw, (top + px.y) / ah);
                let material = match inks.iter().find(|(i, _)| *i == s.ink) {
                    Some((_, m)) => m.clone(),
                    None => {
                        let m = standard.add(StandardMaterial {
                            base_color: ink(s.ink),
                            base_color_texture: Some(deck.atlas.clone()),
                            alpha_mode: AlphaMode::Mask(0.5),
                            perceptual_roughness: 0.45,
                            ..default()
                        });
                        inks.push((s.ink, m.clone()));
                        m
                    }
                };
                let mesh = meshes.add(sign_quad(s, s.height * px.x / px.y, uv0, uv1));
                commands
                    .spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::default(), NotShadowCaster))
                    .id()
            })
            .collect();
        commands.entity(deck.root).add_children(&signs);
        deck.hung = true;
    }
    // Drawn for a few frames once they're up, and then not again.
    let draw = deck.frames > 0;
    if draw {
        deck.frames -= 1;
    }
    if let Ok(mut cam) = cameras.get_mut(deck.camera)
        && cam.is_active != draw
    {
        cam.is_active = draw;
    }
}

/// A sign's quad, `width` across: on its face (a hair off it), its words reading the sign's way.
fn sign_quad(s: &Sign, width: f32, uv0: Vec2, uv1: Vec2) -> Mesh {
    let (right, up) = (s.right() * width * 0.5, s.up * s.height * 0.5);
    let c = s.at + s.normal * OFF;
    let corners = [c - right + up, c - right - up, c + right - up, c + right + up];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, corners.map(|p| p.to_array()).to_vec())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![s.normal.to_array(); 4])
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_UV_0,
            vec![[uv0.x, uv0.y], [uv0.x, uv1.y], [uv1.x, uv1.y], [uv1.x, uv0.y]],
        )
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// While the deck's shown, the sky turns as the ring does (about the colony's axis, x), Earth
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
