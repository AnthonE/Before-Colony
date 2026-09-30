//! The pilot's settings in the browser: read from localStorage when the game starts, changed from
//! the page's settings panel (and F10, and launching), saved half a second after the last change.
//! What they are, their ranges and the text they're kept as are `bc_client_core::settings`'s; the
//! page's panel is drawn from its `KNOBS`.
//!
//! First-flight hints live here too: which ones the pilot has seen is a setting.

use bc_client_core::hints::{HintInput, Hints};
use bc_client_core::settings::{
    self, CameraView, GfxChoice, KNOBS, Kind, Loaded, SETTINGS_VERSION, Settings,
};
use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, MELEE};
use bevy::prelude::*;
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};

use crate::config::LaunchConfig;
use crate::gfx::{Gfx, GfxTier};
use crate::input::Controls;
use crate::net::{GameClient, now_s};
use crate::page::{Ui, UiCmd, UiCmds};
use crate::pointer::PointerRes;
use crate::view::ViewPrefs;

/// Where the settings are kept.
const KEY: &str = "bc.settings";
/// Seconds after the last change before saving (a dragged slider changes every frame).
const SAVE_AFTER: f64 = 0.5;

#[derive(Resource, Clone, Debug)]
pub struct SettingsRes(pub Settings);

/// What the stored text had besides this build's settings, and when to save next.
#[derive(Resource, Default)]
pub struct SettingsStore {
    version: u32,
    unknown: Vec<String>,
    save_at: Option<f64>,
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Reads the saved settings. A first visit starts calm if the browser asks for reduced motion.
pub fn load(cfg: &LaunchConfig) -> (SettingsRes, SettingsStore) {
    let mut defaults = Settings::default();
    if cfg.calm {
        defaults.shake = 0.25;
    }
    let loaded = match storage().and_then(|s| s.get_item(KEY).ok().flatten()) {
        Some(text) => settings::parse(&text, defaults),
        None => Loaded { settings: defaults, version: SETTINGS_VERSION, unknown: Vec::new() },
    };
    (
        SettingsRes(loaded.settings),
        SettingsStore { version: loaded.version, unknown: loaded.unknown, save_at: None },
    )
}

/// The tier the pilot chose, unless the URL named one for this visit.
fn chosen_tier(cfg: &LaunchConfig, s: &Settings, auto: GfxTier) -> Option<GfxTier> {
    if GfxTier::parse(&cfg.quality_param).is_some() {
        return None;
    }
    Some(match s.gfx {
        GfxChoice::Auto => auto,
        g => GfxTier::parse(g.name()).unwrap_or(auto),
    })
}

/// Starts the graphics on the pilot's tier.
pub fn apply_saved_tier(gfx: &mut Gfx, cfg: &LaunchConfig, s: &Settings) {
    if let Some(t) = chosen_tier(cfg, s, gfx.auto) {
        gfx.set_tier(t);
    }
}

/// The view's preferences from the settings.
pub fn view_prefs(s: &Settings) -> ViewPrefs {
    ViewPrefs { fov: s.fov, shake: s.shake, cockpit: s.camera == CameraView::Cockpit }
}

/// Applies the page's changes, saves when due, and hands the view its preferences.
pub fn update_settings(
    cmds: Res<UiCmds>,
    cfg: Res<crate::net::LaunchConfigRes>,
    mut settings: ResMut<SettingsRes>,
    mut store: ResMut<SettingsStore>,
    mut gfx: ResMut<Gfx>,
    mut prefs: ResMut<ViewPrefs>,
) {
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Set { key, value } => {
                if settings.0.set(key, value)
                    && key == "gfx"
                    && let Some(t) = chosen_tier(&cfg.0, &settings.0, gfx.auto)
                {
                    gfx.set_tier(t);
                }
            }
            UiCmd::Play { name, frame, .. } => {
                // The next visit starts where this one launched.
                settings.0.set("name", name);
                settings.0.set("frame", frame);
            }
            _ => {}
        }
    }
    if settings.is_changed() && !settings.is_added() {
        store.save_at = Some(now_s() + SAVE_AFTER);
        let p = view_prefs(&settings.0);
        if *prefs != p {
            *prefs = p;
        }
    }
    if store.save_at.is_some_and(|at| now_s() >= at) {
        store.save_at = None;
        let text = settings::serialize(&settings.0, store.version, &store.unknown);
        if let Some(s) = storage() {
            let _ = s.set_item(KEY, &text);
        }
    }
}

fn set(obj: &Object, key: &str, value: impl Into<JsValue>) {
    let _ = Reflect::set(obj, &JsValue::from_str(key), &value.into());
}

/// Tells the page the settings (the panel's rows and their values) whenever they change.
pub fn publish_settings(settings: Res<SettingsRes>, mut sent: Local<bool>) {
    if *sent && !settings.is_changed() {
        return;
    }
    let Some(w) = web_sys::window() else { return };
    let ui = Reflect::get(&w, &JsValue::from_str("bcUi")).unwrap_or(JsValue::UNDEFINED);
    let Ok(f) = Reflect::get(&ui, &JsValue::from_str("settings")).and_then(|f| f.dyn_into::<Function>())
    else {
        return; // the page script isn't in yet: next frame
    };
    let rows = Array::new();
    for k in KNOBS {
        let o = Object::new();
        set(&o, "key", k.key);
        set(&o, "label", k.label);
        set(&o, "group", k.group);
        match k.kind {
            Kind::Range { min, max, step } => {
                set(&o, "kind", "range");
                set(&o, "min", min);
                set(&o, "max", max);
                set(&o, "step", step);
            }
            Kind::Toggle => set(&o, "kind", "toggle"),
            Kind::Choice(names) => {
                set(&o, "kind", "choice");
                let c = Array::new();
                for n in names {
                    c.push(&JsValue::from_str(n));
                }
                set(&o, "choices", c);
            }
        }
        set(&o, "value", settings.0.get(k.key).unwrap_or_default());
        rows.push(&o);
    }
    let all = Object::new();
    set(&all, "rows", rows);
    let _ = f.call1(&ui, &all);
    *sent = true;
}

/// First-flight hints.
#[derive(Resource, Default)]
pub struct HintState {
    hints: Hints,
    last_assist: Option<bool>,
    last_camera: Option<CameraView>,
}

#[allow(clippy::too_many_arguments)]
pub fn update_hints(
    mut state: ResMut<HintState>,
    mut settings: ResMut<SettingsRes>,
    mut ui: ResMut<Ui>,
    controls: Res<Controls>,
    pointer: Res<PointerRes>,
    keys: Res<ButtonInput<KeyCode>>,
    cmds: Res<UiCmds>,
    indoors: Res<crate::hangar::Indoors>,
    onfoot: Res<crate::onfoot::OnFoot>,
    game: NonSend<GameClient>,
    time: Res<Time<Real>>,
) {
    let (alive, survival) = {
        let g = game.borrow();
        (g.core.world.own.is_some_and(|o| o.alive), g.core.welcome.is_some_and(|w| w.survival))
    };
    let live = settings.0.hints && ui.playing() && !ui.panel_open() && pointer.0.flying();
    let flying = live && alive && !indoors.0;
    let walking = live && ui.on_foot;
    let toggled = state.last_assist.is_some_and(|a| a != controls.flight_assist);
    state.last_assist = Some(controls.flight_assist);
    let switched = state.last_camera.is_some_and(|c| c != settings.0.camera);
    state.last_camera = Some(settings.0.camera);
    let used = keys.just_pressed(KeyCode::KeyE) || cmds.has(&UiCmd::Use);
    let walk_keys = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD];
    let input = HintInput {
        flying,
        thrusting: controls.thrust != Vec3::ZERO,
        boosting: controls.buttons & BOOST != 0,
        firing: controls.buttons & (FIRE_PRIMARY | FIRE_SECONDARY | MELEE) != 0,
        toggled_assist: toggled,
        switched_camera: switched,
        grabbing: controls.grab,
        survival,
        walking,
        strolling: keys.any_pressed(walk_keys),
        using: used && onfoot.focus.is_some(),
        boarding: used && onfoot.focus == Some(bc_client_core::bay::Spot::Cockpit),
        docking: keys.just_pressed(KeyCode::Enter),
    };
    let mut seen = settings.0.hints_seen;
    let hint = state.hints.step(&mut seen, now_s(), f64::from(time.delta_secs()), &input);
    if seen != settings.0.hints_seen {
        settings.0.hints_seen = seen;
    }
    let text = hint.filter(|_| flying || walking).map(|h| h.text()).unwrap_or_default();
    if ui.hint != text {
        ui.hint = text.to_string();
    }
}
