//! Graphics quality tiers: every rendering knob in one place.
//!
//! The tier comes from `?quality=low|medium|high|ultra`; with `auto` (the default) `web/loader.js`
//! picks Low for software rasterisers (SwiftShader, llvmpipe) and High otherwise. F10 cycles it in
//! game. Later milestones add their knobs to [`TierSettings`].

use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::post_process::effect_stack::{ChromaticAberration, Vignette};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::config::LaunchConfig;
use crate::dev_hooks::DevStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxTier {
    Low,
    Medium,
    High,
    Ultra,
}

impl GfxTier {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// The next tier, wrapping (F10).
    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Ultra,
            Self::Ultra => Self::Low,
        }
    }

    pub fn settings(self) -> TierSettings {
        // WebGL2 allows one shadow cascade; the WebGPU build gets more reach.
        let (cascades, reach) = if cfg!(feature = "webgpu") { (3, 3_000.0) } else { (1, 500.0) };
        let base = TierSettings {
            hdr: true,
            msaa: 4,
            smaa: false,
            post: true,
            max_dpr: 1.5,
            ibl: true,
            shadows: true,
            shadow_map: 2048,
            cascades,
            shadow_distance: reach,
            sky_detail: 1.0,
            fx_lights: 12,
        };
        match self {
            // Software rasterisers and weak GPUs: plain LDR, no multisampling, no post effects,
            // no shadows or image-based lighting, the cheapest sky.
            Self::Low => TierSettings {
                hdr: false,
                msaa: 1,
                post: false,
                max_dpr: 1.0,
                ibl: false,
                shadows: false,
                sky_detail: 0.0,
                fx_lights: 0,
                ..base
            },
            // HDR and bloom with the cheaper post-process anti-aliasing; no shadows.
            Self::Medium => TierSettings {
                msaa: 1,
                smaa: true,
                max_dpr: 1.0,
                shadows: false,
                sky_detail: 0.5,
                fx_lights: 4,
                ..base
            },
            Self::High => base,
            Self::Ultra => TierSettings { max_dpr: 4.0, shadow_map: 4096, fx_lights: 24, ..base },
        }
    }
}

/// What a tier turns on.
#[derive(Clone, Copy, Debug)]
pub struct TierSettings {
    /// HDR rendering with tonemapping and bloom. Off: plain LDR output.
    pub hdr: bool,
    /// Multisample count (1 = off).
    pub msaa: u32,
    /// Post-process anti-aliasing (SMAA) when multisampling is off.
    pub smaa: bool,
    /// Gameplay post effects: the G-strain vignette and the ZERO seizure's aberration.
    pub post: bool,
    /// Highest device-pixel ratio the backbuffer is rendered at; the browser upscales the rest.
    pub max_dpr: f32,
    /// Image-based lighting from the sky (Earthshine on night sides, reflections on metal).
    pub ibl: bool,
    /// Sun shadows: map size, cascades and reach (m).
    pub shadows: bool,
    pub shadow_map: usize,
    pub cascades: usize,
    pub shadow_distance: f32,
    /// Sky shader detail, 0 (cheapest) to 1.
    pub sky_detail: f32,
    /// Point lights for effects (muzzle flashes, hits, blasts, sabers).
    pub fx_lights: usize,
}

/// The active tier.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Gfx {
    pub tier: GfxTier,
    pub settings: TierSettings,
    /// Which build was loaded: "webgl2" or "webgpu".
    pub backend: &'static str,
}

impl Gfx {
    pub fn from_config(cfg: &LaunchConfig) -> Self {
        let fallback = if cfg.low_quality { GfxTier::Low } else { GfxTier::High };
        let tier = GfxTier::parse(&cfg.quality).unwrap_or(fallback);
        let backend = if cfg!(feature = "webgpu") { "webgpu" } else { "webgl2" };
        Self { tier, settings: tier.settings(), backend }
    }

    fn set_tier(&mut self, tier: GfxTier) {
        self.tier = tier;
        self.settings = tier.settings();
    }
}

pub struct GfxPlugin(pub Gfx);

impl Plugin for GfxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0).add_systems(Update, (cycle_tier, apply_camera_tier, apply_resolution));
    }
}

/// F10: next tier.
fn cycle_tier(keys: Res<ButtonInput<KeyCode>>, mut gfx: ResMut<Gfx>) {
    if keys.just_pressed(KeyCode::F10) {
        let next = gfx.tier.next();
        gfx.set_tier(next);
        info!("graphics tier: {}", next.name());
    }
}

/// Puts the tier's post-processing on the main camera (at spawn and whenever the tier changes).
fn apply_camera_tier(
    mut commands: Commands,
    gfx: Res<Gfx>,
    cams: Query<Entity, With<MainCamera>>,
    added: Query<(), Added<MainCamera>>,
    mut dev: ResMut<DevStatus>,
) {
    if !gfx.is_changed() && added.is_empty() {
        return;
    }
    dev.set("gfx_tier", gfx.tier.name());
    dev.set("backend", gfx.backend);
    let s = gfx.settings;
    for cam in &cams {
        let mut e = commands.entity(cam);
        e.insert(match s.msaa {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            _ => Msaa::Sample4,
        });
        if s.smaa {
            e.insert(Smaa::default());
        } else {
            e.remove::<Smaa>();
        }
        if s.hdr {
            e.insert((Hdr, Tonemapping::TonyMcMapface, Bloom::NATURAL));
        } else {
            e.remove::<(Hdr, Bloom)>();
        }
        if s.post {
            e.insert((
                Vignette { intensity: 0.0, ..default() },
                ChromaticAberration { intensity: 0.0, ..default() },
            ));
        } else {
            e.remove::<(Vignette, ChromaticAberration)>();
        }
    }
}

/// Caps the backbuffer's device-pixel ratio (high-DPI screens at lower tiers).
fn apply_resolution(gfx: Res<Gfx>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !gfx.is_changed() {
        return;
    }
    for mut w in &mut windows {
        let native = w.resolution.base_scale_factor();
        let cap = gfx.settings.max_dpr;
        w.resolution.set_scale_factor_override((native > cap).then_some(cap));
    }
}
