//! Graphics quality tiers: every rendering knob in one place.
//!
//! The tier comes from `?quality=low|medium|high|ultra`, else the pilot's saved choice (game mode),
//! else `web/loader.js`'s pick for the GPU: Low for software rasterisers (SwiftShader, llvmpipe),
//! High otherwise. F10 (or O) cycles it in game, and the settings keep the choice. Later milestones
//! add their knobs to [`TierSettings`].

use bc_client_core::life;
use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::effect_stack::{ChromaticAberration, LensDistortion, Vignette};
use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::config::LaunchConfig;
use crate::dev_hooks::DevStatus;
use crate::orrery_vision::OrreryVision;

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

    /// The next tier, wrapping (F10, or O).
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
        let webgpu = cfg!(feature = "webgpu");
        let (cascades, reach) = if webgpu { (3, 3_000.0) } else { (1, 500.0) };
        let base = TierSettings {
            hdr: true,
            msaa: 4,
            smaa: false,
            ssao: None,
            post: true,
            max_dpr: 1.5,
            ibl: true,
            shadows: true,
            shadow_map: 2048,
            cascades,
            shadow_distance: reach,
            sky_detail: 1.0,
            fx_lights: 12,
            particles: 4_000,
            dust: 1_500,
            flare: true,
            life: life::TIERS[2],
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
                particles: 300,
                dust: 0,
                flare: false,
                life: life::TIERS[0],
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
                particles: 1_500,
                dust: 600,
                life: life::TIERS[1],
                ..base
            },
            // Screen-space ambient occlusion where the GPU can run it (WebGPU's compute; WebGL2 has
            // none): shade where things meet, in their creases and under them.
            Self::High => TierSettings {
                ssao: webgpu.then_some(ScreenSpaceAmbientOcclusionQualityLevel::Medium),
                ..base
            },
            Self::Ultra => TierSettings {
                ssao: webgpu.then_some(ScreenSpaceAmbientOcclusionQualityLevel::High),
                max_dpr: 4.0,
                shadow_map: 4096,
                fx_lights: 24,
                particles: 10_000,
                dust: 2_500,
                life: life::TIERS[3],
                ..base
            },
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
    /// Screen-space ambient occlusion, at this quality, on GPUs that can run it ([`ssao_runs`]).
    /// It needs multisampling off, so where it runs SMAA stands in for it.
    pub ssao: Option<ScreenSpaceAmbientOcclusionQualityLevel>,
    /// The pilot's post effects: G-strain grey-out and tunnel vision, hit flashes, the Orrery's vision
    /// and its seizure's warp and fringes (see `camera::pilot_effects`).
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
    /// Effect particles alive at once.
    pub particles: usize,
    /// Dust motes around the camera (0: none).
    pub dust: usize,
    /// The Sun's lens flare.
    pub flare: bool,
    /// The city's traffic and people: how many are drawn, how far (`bc_client_core::life`).
    pub life: life::LifeTier,
}

/// The active tier.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Gfx {
    pub tier: GfxTier,
    pub settings: TierSettings,
    /// Which build was loaded: "webgl2" or "webgpu".
    pub backend: &'static str,
    pub tonemapping: Tonemapping,
    /// The game's own look (grade, vignette, lit smoke); `?look=0` turns it off, to compare.
    pub look: bool,
    /// The city's traffic and people drawn; `?life=0` hides them, to compare and to measure.
    pub life: bool,
    /// Ambient occlusion where the tier and the GPU have it; `?ssao=0` turns it off, to compare.
    pub ssao: bool,
    /// The page's pick for this GPU (what the "auto" setting means).
    pub auto: GfxTier,
}

impl Gfx {
    pub fn from_config(cfg: &LaunchConfig) -> Self {
        let fallback = if cfg.low_quality { GfxTier::Low } else { GfxTier::High };
        let tier = GfxTier::parse(&cfg.quality).unwrap_or(fallback);
        let backend = if cfg!(feature = "webgpu") { "webgpu" } else { "webgl2" };
        // ACES's filmic curve, with a firm toe and shoulder: shade on a suit's far side, light
        // pooling under a lamp, paint and beams that stay saturated. Under TonyMcMapface, Bevy's
        // default, every value sat nearer the middle and the picture read flat.
        let tonemapping = match cfg.tonemap.to_ascii_lowercase().as_str() {
            "agx" => Tonemapping::AgX,
            "tony" => Tonemapping::TonyMcMapface,
            _ => Tonemapping::AcesFitted,
        };
        let auto = GfxTier::parse(&cfg.quality_auto).unwrap_or(tier);
        Self {
            tier,
            settings: tier.settings(),
            backend,
            tonemapping,
            look: cfg.look,
            life: cfg.life,
            ssao: cfg.ssao,
            auto,
        }
    }

    pub fn set_tier(&mut self, tier: GfxTier) {
        self.tier = tier;
        self.settings = tier.settings();
    }

    /// Stops the picture is opened up by before the tonemapper, so the exposures (`sky::EV100`,
    /// the bay's, the colony's colour script) mean the same under each: ACES maps a value about a
    /// stop darker than TonyMcMapface does through the darks and lower midtones, and opened a stop
    /// it meets Tony there, keeping its own deeper shade below and brighter highlights above.
    pub fn exposure_bias(&self) -> f32 {
        if self.tonemapping == Tonemapping::AcesFitted { 1.0 } else { 0.0 }
    }

    /// How much brighter the stars are drawn (they're artistic, `sky.wgsl`), so the faint ones come
    /// through ACES's toe as they do through Tony's.
    pub fn starlight(&self) -> f32 {
        if self.tonemapping == Tonemapping::AcesFitted { 1.8 } else { 1.0 }
    }

    /// The Milky Way's lift (`sky.wgsl`): the exponent on its haze, so its faint reaches come through
    /// ACES's toe about as bright as through Tony's and its core no brighter.
    pub fn milky_way_lift(&self) -> f32 {
        if self.tonemapping == Tonemapping::AcesFitted { 0.55 } else { 1.0 }
    }
}

pub struct GfxPlugin(pub Gfx);

impl Plugin for GfxPlugin {
    fn build(&self, app: &mut App) {
        // The tier a key chose is on the camera, and in `window.__bc`, the frame it was chosen: the
        // next can take a software renderer seconds, building the new tier's pipelines.
        app.insert_resource(self.0)
            .add_systems(Update, (cycle_tier, apply_camera_tier.after(cycle_tier), apply_resolution));
    }
}

/// F10's second key: the function row needs Fn on a Mac laptop, and a 60% keyboard has none
/// (`docs/CONTROLS.md`, item 6). Nothing else binds O, in any mode.
pub const TIER_KEY: KeyCode = KeyCode::KeyO;

/// F10 or O: next tier (kept in the settings, in game mode). O is a letter, so not while the
/// radio's line is open: what's typed there is never a key.
fn cycle_tier(
    keys: Res<ButtonInput<KeyCode>>,
    mut gfx: ResMut<Gfx>,
    ui: Option<ResMut<crate::page::Ui>>,
    settings: Option<ResMut<crate::settings::SettingsRes>>,
) {
    let typing = ui.as_ref().is_some_and(|ui| ui.chat);
    if keys.just_pressed(KeyCode::F10) || (keys.just_pressed(TIER_KEY) && !typing) {
        let next = gfx.tier.next();
        gfx.set_tier(next);
        info!("graphics tier: {}", next.name());
        if let Some(mut ui) = ui {
            ui.toast(format!("GRAPHICS: {}", next.name().to_uppercase()));
        }
        if let Some(mut s) = settings {
            s.0.set("gfx", next.name());
        }
    }
}

/// The picture's grade before anything happens to the pilot: a touch warmer, a little more contrast
/// in the midtones, and shadows and highlights a shade less saturated, as film would take it.
/// Applied on every tier (LDR tiers grade in their shaders). `camera::pilot_effects` works on top.
/// The tonemapper's exposure bias ([`Gfx::exposure_bias`]) is in it with or without the look.
pub fn base_grading(gfx: &Gfx) -> ColorGrading {
    let exposure = gfx.exposure_bias();
    if !gfx.look {
        return ColorGrading { global: ColorGradingGlobal { exposure, ..default() }, ..default() };
    }
    let section = |saturation: f32, contrast: f32| ColorGradingSection { saturation, contrast, ..default() };
    ColorGrading {
        global: ColorGradingGlobal { temperature: 0.03, exposure, ..default() },
        shadows: section(0.9, 1.0),
        midtones: section(1.0, 1.06),
        highlights: section(0.95, 1.0),
    }
}

/// The vignette's resting intensity: the edges of the picture a little darker, like a lens's.
pub fn base_vignette(look: bool) -> f32 {
    if look { 0.22 } else { 0.0 }
}

/// Whether this GPU can run Bevy's ambient occlusion: its compute passes write five storage
/// textures, and its depth is an R32Float texture sampled with filtering. Without those, binding
/// it would be a validation error, and Bevy stops the app on one.
pub fn ssao_runs(device: &RenderDevice) -> bool {
    device.limits().max_storage_textures_per_shader_stage >= 5
        && device.features().contains(WgpuFeatures::FLOAT32_FILTERABLE)
}

/// Puts the tier's post-processing on the main camera (at spawn and whenever the tier changes).
fn apply_camera_tier(
    mut commands: Commands,
    gfx: Res<Gfx>,
    device: Option<Res<RenderDevice>>,
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
    let ssao = s.ssao.filter(|_| gfx.ssao && device.as_deref().is_some_and(ssao_runs));
    dev.set("ssao", ssao.is_some());
    let msaa = if ssao.is_some() { 1 } else { s.msaa };
    for cam in &cams {
        let mut e = commands.entity(cam);
        e.insert(match msaa {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            _ => Msaa::Sample4,
        });
        if s.smaa || msaa != s.msaa {
            e.insert(Smaa::default());
        } else {
            e.remove::<Smaa>();
        }
        // (Taking the occlusion off leaves the prepasses it brought, unless they go too.)
        match ssao {
            Some(quality_level) => {
                e.insert(ScreenSpaceAmbientOcclusion { quality_level, ..default() });
            }
            None => {
                e.remove::<(ScreenSpaceAmbientOcclusion, DepthPrepass, NormalPrepass)>();
            }
        }
        // The tonemapper on every tier: LDR ones tonemap in their shaders, with the same curve.
        e.insert(gfx.tonemapping);
        if s.hdr {
            e.insert((Hdr, Bloom::NATURAL));
        } else {
            e.remove::<(Hdr, Bloom)>();
        }
        e.insert(base_grading(&gfx));
        if s.post {
            e.insert((
                Vignette { intensity: base_vignette(gfx.look), ..default() },
                ChromaticAberration { intensity: 0.0, ..default() },
                LensDistortion { intensity: 0.0, ..default() },
                OrreryVision::default(),
            ));
        } else {
            e.remove::<(Vignette, ChromaticAberration, LensDistortion, OrreryVision)>();
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
