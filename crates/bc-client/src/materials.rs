//! Procedural surface materials: `HullMaterial` for painted armour and hull plating, and
//! `RockMaterial` for asteroids. Both extend Bevy's `StandardMaterial` (so they get its lighting,
//! shadows and image-based lighting) with a WGSL fragment that paints the surface.
//!
//! What varies per piece (paint, armour left, heat, wreck) rides in its `MeshTag`, so every suit
//! shares one material and identical pieces still batch.

use bevy::asset::embedded_asset;
use bevy::mesh::MeshTag;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};

use crate::gfx::Gfx;
use crate::view::VisTime;

pub type HullMaterial = ExtendedMaterial<StandardMaterial, HullExt>;
pub type RockMaterial = ExtendedMaterial<StandardMaterial, RockExt>;

/// Palette indices for [`HullTag::paint`].
pub mod paint {
    pub const WHITE: u8 = 0;
    pub const BLUE: u8 = 1;
    pub const RED: u8 = 2;
    pub const YELLOW: u8 = 3;
    /// Weapons, joints and inner frame.
    pub const DARK: u8 = 4;
    pub const OZ_GREEN: u8 = 5;
    pub const OZ_GREY: u8 = 6;
    pub const TAURUS_WHITE: u8 = 7;
    pub const TAURUS_BLUE: u8 = 8;
    pub const VIRGO_OLIVE: u8 = 9;
    pub const ALLIANCE_TAN: u8 = 10;
    /// Colony hull plating.
    pub const HULL: u8 = 11;
    /// Colony hull, darker service plating.
    pub const HULL_DARK: u8 = 12;
    /// The colony's mirrors: aluminised film on a frame.
    pub const MIRROR: u8 = 13;
}

/// sRGB colour and perceptual roughness of each paint.
const PALETTE: [(f32, f32, f32, f32); 14] = [
    (0.9, 0.91, 0.93, 0.38),
    (0.1, 0.22, 0.66, 0.4),
    (0.72, 0.08, 0.08, 0.4),
    (0.95, 0.75, 0.1, 0.35),
    (0.11, 0.12, 0.13, 0.5),
    (0.28, 0.38, 0.27, 0.5),
    (0.44, 0.47, 0.5, 0.45),
    (0.83, 0.85, 0.88, 0.38),
    (0.2, 0.32, 0.6, 0.42),
    (0.38, 0.41, 0.27, 0.55),
    (0.6, 0.5, 0.34, 0.55),
    (0.62, 0.63, 0.64, 0.5),
    (0.3, 0.31, 0.33, 0.6),
    (0.66, 0.71, 0.8, 0.2),
];

/// Per-piece parameters for [`HullMaterial`], packed into a `MeshTag`.
#[derive(Clone, Copy, Debug)]
pub struct HullTag {
    pub paint: u8,
    /// Armour left, 0 (destroyed) to 7 (pristine).
    pub armour: u8,
    pub seed: u8,
    /// Recent-hit glow, 0 to 31.
    pub heat: u8,
    pub wreck: bool,
    /// Bare metal instead of paint.
    pub metal: bool,
}

impl HullTag {
    pub fn paint(paint: u8, seed: u8) -> Self {
        Self { paint, armour: 7, seed, heat: 0, wreck: false, metal: false }
    }

    pub fn tag(self) -> MeshTag {
        MeshTag(
            u32::from(self.paint & 15)
                | u32::from(self.armour.min(7)) << 4
                | u32::from(self.seed) << 7
                | u32::from(self.heat.min(31)) << 15
                | u32::from(self.wreck) << 20
                | u32::from(self.metal) << 21,
        )
    }
}

/// A rock's `MeshTag`: its ore kind and a seed.
pub fn rock_tag(ore: u8, seed: u8) -> MeshTag {
    MeshTag(u32::from(ore & 3) | u32::from(seed) << 2)
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct HullExt {
    #[uniform(100)]
    pub hull: HullParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct HullParams {
    /// rgb: linear base colour; a: perceptual roughness.
    pub palette: [Vec4; 16],
    /// x: plate size (m); y: seam width (m); z: seam depth; w: grime.
    pub panel: Vec4,
    /// x: seconds.
    pub time: Vec4,
}

impl MaterialExtension for HullExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/hull.wgsl".into()
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct RockExt {
    #[uniform(100)]
    pub rock: RockParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct RockParams {
    /// Per ore kind: rgb speck colour (linear); a: how metallic.
    pub ore: [Vec4; 4],
    /// x: detail 0..1.
    pub detail: Vec4,
}

impl MaterialExtension for RockExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bc_client/shaders/rock.wgsl".into()
    }
}

/// The shared material handles.
#[derive(Resource)]
pub struct Surfaces {
    /// Mobile-suit armour: plates about a metre across.
    pub armour: Handle<HullMaterial>,
    /// The colony: plates tens of metres across.
    pub colony: Handle<HullMaterial>,
    /// The colony's mirrors: segments over a hundred metres across.
    pub mirror: Handle<HullMaterial>,
    pub rock: Handle<RockMaterial>,
    /// Keeps the shared WGSL library loaded, so shaders can import it.
    _noise: Handle<Shader>,
}

pub struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/noise.wgsl");
        embedded_asset!(app, "shaders/hull.wgsl");
        embedded_asset!(app, "shaders/rock.wgsl");
        app.add_plugins((
            MaterialPlugin::<HullMaterial>::default(),
            MaterialPlugin::<RockMaterial>::default(),
        ))
        .add_systems(Update, tick_materials);
    }
}

fn palette() -> [Vec4; 16] {
    let mut out = [Vec4::new(1.0, 0.0, 1.0, 0.5); 16];
    for (i, (r, g, b, rough)) in PALETTE.iter().enumerate() {
        let c = Color::srgb(*r, *g, *b).to_linear();
        out[i] = Vec4::new(c.red, c.green, c.blue, *rough);
    }
    out
}

fn hull(panel: Vec4) -> HullMaterial {
    ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.4, ..default() },
        extension: HullExt { hull: HullParams { palette: palette(), panel, time: Vec4::ZERO } },
    }
}

/// Creates the shared materials (a startup system, before anything is built with them).
pub fn setup_materials(
    mut commands: Commands,
    assets: Res<AssetServer>,
    gfx: Res<Gfx>,
    mut hulls: ResMut<Assets<HullMaterial>>,
    mut rocks: ResMut<Assets<RockMaterial>>,
) {
    let ore = |r: f32, g: f32, b: f32, metal: f32| {
        let c = Color::srgb(r, g, b).to_linear();
        Vec4::new(c.red, c.green, c.blue, metal)
    };
    commands.insert_resource(Surfaces {
        armour: hulls.add(hull(Vec4::new(1.1, 0.035, 0.6, 0.35))),
        colony: hulls.add(hull(Vec4::new(42.0, 0.6, 0.5, 0.35))),
        mirror: hulls.add(hull(Vec4::new(160.0, 1.2, 0.3, 0.05))),
        rock: rocks.add(ExtendedMaterial {
            base: StandardMaterial { perceptual_roughness: 0.9, ..default() },
            extension: RockExt {
                rock: RockParams {
                    ore: [
                        ore(0.42, 0.36, 0.3, 0.6),
                        ore(0.8, 0.82, 0.86, 1.0),
                        ore(0.7, 0.85, 0.95, 0.0),
                        ore(0.3, 0.95, 0.85, 1.0),
                    ],
                    detail: Vec4::new(gfx.settings.sky_detail, 0.0, 0.0, 0.0),
                },
            },
        }),
        _noise: assets.load("embedded://bc_client/shaders/noise.wgsl"),
    });
}

/// The clock that makes wreck embers flicker.
fn tick_materials(
    time: Res<VisTime>,
    surfaces: Option<Res<Surfaces>>,
    mut hulls: ResMut<Assets<HullMaterial>>,
    mut last: Local<f64>,
) {
    let Some(s) = surfaces else { return };
    if (time.now - *last).abs() < 1.0 / 30.0 {
        return;
    }
    *last = time.now;
    if let Some(mut m) = hulls.get_mut(&s.armour) {
        m.extension.hull.time.x = (time.now % 10_000.0) as f32;
    }
}
