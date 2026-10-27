//! The pilot's settings in the browser: read from localStorage when the game starts, changed from
//! the page's settings panel (and F10 or O, and launching), saved half a second after the last
//! change. What they are, their ranges and the text they're kept as are `bc_client_core::settings`'s;
//! the page's panel is drawn from its `KNOBS` (`bc_client_core::page::SettingsView`).
//!
//! First-flight hints live here too: which ones the pilot has seen is a setting.

use bc_client_core::hints::{HintInput, Hints};
use bc_client_core::page::SettingsView;
use bc_client_core::settings::{
    self, CameraView, FlightControls, GfxChoice, Loaded, SETTINGS_VERSION, Settings,
};
use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, MELEE};
use bc_sim::content::body::Fed;
use bevy::prelude::*;
use js_sys::{Function, Reflect};
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

/// Reads the saved settings. A first visit starts calm, without flashing, if the browser asks for
/// reduced motion.
pub fn load(cfg: &LaunchConfig) -> (SettingsRes, SettingsStore) {
    let mut defaults = Settings::default();
    if cfg.calm {
        defaults.shake = 0.25;
        defaults.flashing = false;
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
    ViewPrefs {
        fov: s.fov,
        shake: s.shake,
        cockpit: s.camera == CameraView::Cockpit,
        holo: s.cockpit_holo,
        flashing: s.flashing,
    }
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
    let _ = f.call1(&ui, &crate::page::to_js(&SettingsView::of(&settings.0)));
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
    enter: Res<crate::chat::Enter>,
    game: NonSend<GameClient>,
    time: Res<Time<Real>>,
) {
    let (alive, survival, feet, hidden, hostile_near, locked, colony, inside, near_hub, trainer, hungry) = {
        let g = game.borrow();
        let own = g.core.world.own.filter(|o| o.alive);
        // A hostile within a couple of kilometres: something to lock on to.
        let from = g.core.own_view().map_or(g.core.predict.state.pos, |v| v.pos);
        let w = &g.core.world;
        let hostile_near = w.entities.iter().flatten().any(|tr| {
            let e = &tr.latest;
            e.faction != w.faction
                && e.flags & bc_proto::snapshot::ent_flags::WRECK == 0
                && tr.latest.pos.distance(from) < 2_000.0
                && e.on.is_none()
        });
        (
            own.is_some(),
            g.core.welcome.is_some_and(|w| w.survival),
            crate::hud::footed(&g.core),
            own.is_some_and(|o| o.cover == bc_proto::snapshot::cover::HIDDEN),
            hostile_near,
            g.hard.locked(),
            g.core.welcome.is_some_and(|w| w.colony),
            g.core.welcome.is_some_and(|w| w.interior),
            // Within a couple of kilometres of the docking hub's mouth (on the colony's axis).
            {
                let mouth = bc_sim::world::COLONY_CENTER + Vec3::X * bc_sim::colony::hub::MOUTH_X;
                !g.core.inside() && from.distance(mouth) < 2_000.0
            },
            g.core.hangar.trainer,
            // The body's word: hungry or starving, The Arrival's meals are pointed at.
            matches!(g.core.hangar.fed(), Some(Fed::Hungry | Fed::Starving)),
        )
    };
    let grounded = feet.footing == bc_sim::ground::Footing::Grounded;
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
        // Only a press that asks the dock (`chat::radio` routed it): away from where the suit
        // docks, Enter opens the radio, and that's not docking.
        docking: enter.0 == bc_client_core::dock::EnterKey::Dock,
        near_surface: feet.near,
        gripping: feet.footing != bc_sim::ground::Footing::Free,
        grounded,
        walked: grounded && keys.any_pressed(walk_keys),
        hopped: grounded && keys.just_pressed(KeyCode::Space),
        in_hide_spot: grounded && feet.spot.is_some(),
        hidden,
        opened_map: keys.just_pressed(crate::map::MAP_KEY),
        hostile_near,
        locked,
        stepped: controls.buttons & bc_proto::buttons::BURST != 0,
        colony,
        in_city: onfoot.city.is_some(),
        inside,
        went_down: used && onfoot.focus == Some(bc_client_core::bay::Spot::Airlock),
        near_hub,
        trainer: inside && trainer,
        hungry,
    };
    let mut seen = settings.0.hints_seen;
    let hint = state.hints.step(&mut seen, now_s(), f64::from(time.delta_secs()), &input);
    if seen != settings.0.hints_seen {
        settings.0.hints_seen = seen;
    }
    let shooter = settings.0.flight_controls == FlightControls::Shooter;
    let text = hint.filter(|_| flying || walking).map(|h| h.text(colony, shooter)).unwrap_or_default();
    if ui.hint != text {
        ui.hint = text.to_string();
    }
}
