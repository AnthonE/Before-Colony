//! The page around the game. The title screen, the pause menu, the controls sheet, the reconnect
//! banner, the "click to fly" prompt and toasts are HTML (`web/ui.js`), drawn over the live scene;
//! the cockpit HUD stays in Bevy. This module is the bridge, and Rust owns every decision:
//! - the page pushes commands onto `window.bcInbox` (an array), drained once a frame into
//!   [`UiCmds`];
//! - Rust sends what the page should show through `window.bcUi.update(view)`, whenever it changes,
//!   and the static lists (the frames, the controls) once through `window.bcUi.init(...)`.

use bc_client_core::controls::{BINDINGS, Group};
use bc_sim::content::{PLAYABLE_ORDER, SpecialKind, frame, frame_designation, frame_name, weapon_name};
use bevy::prelude::*;
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};

use crate::net::{LaunchConfigRes, now_s};

/// A command from the page (or from keys the page forwards).
#[derive(Clone, Debug, PartialEq)]
pub enum UiCmd {
    /// Launch with this callsign and frame slug, signed in with the wallet at `address` (`0x…`)
    /// or as a guest (empty).
    Play { name: String, frame: String, address: String },
    /// Dial again (after a failure, or before the next scheduled redial).
    Retry,
    /// Stop connecting and go back to the title.
    Cancel,
    /// Close the pause menu and fly.
    Resume,
    /// Open the pause menu.
    Pause,
    /// Leave the world for the title screen.
    Disconnect,
    /// Esc: close whatever is on top, or open the pause menu.
    Back,
    /// F1: show or hide the controls sheet (`None`: toggle).
    Help(Option<bool>),
    /// Open (`true`) or close the settings panel.
    Settings(bool),
    /// A setting changed on the panel: its key and new value, as text.
    Set { key: String, value: String },
    /// A menu sound: `click` or `confirm`.
    Sfx(String),
    /// Dev hook: drop the link as if the network had failed (tests the reconnect path).
    DropLink,
}

/// This frame's commands.
#[derive(Resource, Default)]
pub struct UiCmds(pub Vec<UiCmd>);

impl UiCmds {
    pub fn has(&self, cmd: &UiCmd) -> bool {
        self.0.contains(cmd)
    }
}

/// Which screen the page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Screen {
    /// Callsign, frame, Launch.
    #[default]
    Title,
    /// The first dial and handshake.
    Connecting,
    /// In the world.
    Playing,
    /// The link dropped and is being redialed (the banner).
    Reconnecting,
    /// Down, waiting for the player (the title, with the reason).
    Failed,
}

impl Screen {
    fn name(self) -> &'static str {
        match self {
            Screen::Title => "title",
            Screen::Connecting => "connecting",
            Screen::Playing => "playing",
            Screen::Reconnecting => "reconnecting",
            Screen::Failed => "failed",
        }
    }
}

/// A modal panel (it frees the pointer and holds the controls).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    None,
    Pause,
    /// Settings, over the menu (`from_pause`) or the title screen.
    Settings {
        from_pause: bool,
    },
}

/// The page's state, as Rust decides it.
#[derive(Resource, Default)]
pub struct Ui {
    pub screen: Screen,
    pub panel: Panel,
    /// The controls sheet (F1) is up. It isn't modal: flying goes on under it.
    pub help: bool,
    /// When the pause menu last opened (Esc arriving just after the browser dropped the lock must
    /// not close it again).
    paused_at: f64,
    /// Flying needs a click (the pointer isn't locked).
    pub click_to_fly: bool,
    /// The browser refused to lock the pointer; the prompt says to click again.
    pub refused: bool,
    /// A first-flight hint (empty: none).
    pub hint: String,
    /// The wallet is being asked to sign the pilot in.
    pub signing: bool,
    /// Flying signed in (the suit stays when the pilot leaves).
    pub signed_in: bool,
    /// At rest against an asteroid: leaving now parks the suit there.
    pub parkable: bool,
    /// What the link screens say.
    pub message: String,
    pub retryable: bool,
    pub reload: bool,
    pub attempt: u32,
    /// Seconds to the next redial.
    pub retry_in: u32,
    toast_seq: u32,
    toast: String,
}

impl Ui {
    pub fn playing(&self) -> bool {
        self.screen == Screen::Playing
    }

    pub fn panel_open(&self) -> bool {
        self.panel != Panel::None
    }

    pub fn open_pause(&mut self) {
        if self.playing() && self.panel == Panel::None {
            self.panel = Panel::Pause;
            self.paused_at = now_s();
        }
    }

    fn close_settings(&mut self, from_pause: bool) {
        self.panel = if from_pause && self.playing() { Panel::Pause } else { Panel::None };
    }

    /// A short message at the top of the screen.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast_seq += 1;
        self.toast = text.into();
    }
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn set(obj: &Object, key: &str, value: impl Into<JsValue>) {
    let _ = Reflect::set(obj, &JsValue::from_str(key), &value.into());
}

fn parse(v: &JsValue) -> Option<UiCmd> {
    let s = |k: &str| get(v, k).as_string().unwrap_or_default();
    Some(match s("cmd").as_str() {
        "play" => UiCmd::Play { name: s("name"), frame: s("frame"), address: s("address") },
        "retry" => UiCmd::Retry,
        "cancel" => UiCmd::Cancel,
        "resume" => UiCmd::Resume,
        "pause" => UiCmd::Pause,
        "disconnect" => UiCmd::Disconnect,
        "back" => UiCmd::Back,
        "help" => UiCmd::Help(get(v, "show").as_bool()),
        "settings" => UiCmd::Settings(get(v, "show").as_bool().unwrap_or(true)),
        "set" => UiCmd::Set { key: s("key"), value: s("value") },
        "drop_link" => UiCmd::DropLink,
        "sfx" => UiCmd::Sfx(s("cue")),
        _ => return None,
    })
}

/// First thing each frame: takes the page's commands.
pub fn drain_inbox(mut cmds: ResMut<UiCmds>) {
    cmds.0.clear();
    let Some(w) = web_sys::window() else { return };
    let inbox = get(&w, "bcInbox");
    if !Array::is_array(&inbox) {
        return;
    }
    let inbox: Array = inbox.unchecked_into();
    cmds.0.extend(inbox.iter().filter_map(|v| parse(&v)));
    inbox.set_length(0);
}

/// Esc, F1, Resume and the pause menu's own buttons (the link's commands are `session`'s).
pub fn apply_ui_cmds(cmds: Res<UiCmds>, mut ui: ResMut<Ui>) {
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Back => {
                if ui.help {
                    ui.help = false;
                } else {
                    match ui.panel {
                        Panel::Settings { from_pause } => ui.close_settings(from_pause),
                        Panel::None if ui.playing() => ui.open_pause(),
                        // Esc that belonged to the browser dropping the lock (which opened the
                        // menu) must not close it again.
                        Panel::Pause if now_s() - ui.paused_at > 0.4 => ui.panel = Panel::None,
                        _ => {}
                    }
                }
            }
            UiCmd::Pause => ui.open_pause(),
            UiCmd::Resume => {
                ui.panel = Panel::None;
                ui.help = false;
            }
            UiCmd::Help(show) => ui.help = show.unwrap_or(!ui.help),
            UiCmd::Settings(true) => {
                let from_pause = ui.panel == Panel::Pause;
                ui.panel = Panel::Settings { from_pause };
            }
            UiCmd::Settings(false) => {
                if let Panel::Settings { from_pause } = ui.panel {
                    ui.close_settings(from_pause);
                }
            }
            _ => {}
        }
    }
    // The menu is the world's; settings may open over the title too.
    if !ui.playing() && ui.panel == Panel::Pause {
        ui.panel = Panel::None;
    }
    if !ui.playing() && ui.panel == (Panel::Settings { from_pause: true }) {
        ui.panel = Panel::Settings { from_pause: false };
    }
}

/// What the page draws: sent when it changes.
#[derive(Clone, PartialEq, Default)]
pub struct View {
    screen: &'static str,
    panel: &'static str,
    help: bool,
    click_to_fly: bool,
    refused: bool,
    message: String,
    retryable: bool,
    reload: bool,
    attempt: u32,
    retry_in: u32,
    toast_seq: u32,
    toast: String,
    hint: String,
    signing: bool,
    signed_in: bool,
    parkable: bool,
}

impl View {
    fn of(ui: &Ui) -> Self {
        View {
            screen: ui.screen.name(),
            panel: match ui.panel {
                Panel::None => "none",
                Panel::Pause => "pause",
                Panel::Settings { .. } => "settings",
            },
            help: ui.help,
            click_to_fly: ui.click_to_fly,
            refused: ui.refused,
            message: ui.message.clone(),
            retryable: ui.retryable,
            reload: ui.reload,
            attempt: ui.attempt,
            retry_in: ui.retry_in,
            toast_seq: ui.toast_seq,
            toast: ui.toast.clone(),
            hint: ui.hint.clone(),
            signing: ui.signing,
            signed_in: ui.signed_in,
            parkable: ui.parkable,
        }
    }

    fn to_js(&self) -> Object {
        let o = Object::new();
        set(&o, "screen", self.screen);
        set(&o, "panel", self.panel);
        set(&o, "help", self.help);
        set(&o, "clickToFly", self.click_to_fly);
        set(&o, "refused", self.refused);
        set(&o, "message", self.message.as_str());
        set(&o, "retryable", self.retryable);
        set(&o, "reload", self.reload);
        set(&o, "attempt", self.attempt);
        set(&o, "retryIn", self.retry_in);
        set(&o, "toastSeq", self.toast_seq);
        set(&o, "toast", self.toast.as_str());
        set(&o, "hint", self.hint.as_str());
        set(&o, "signing", self.signing);
        set(&o, "signedIn", self.signed_in);
        set(&o, "parkable", self.parkable);
        o
    }
}

fn call_ui(method: &str, arg: &JsValue) {
    let Some(w) = web_sys::window() else { return };
    let ui = get(&w, "bcUi");
    if let Ok(f) = get(&ui, method).dyn_into::<Function>() {
        let _ = f.call1(&ui, arg);
    }
}

/// Last thing each frame: tells the page what changed. Sends everything again if the page (its
/// script loads alongside the game) wasn't ready the first time.
pub fn publish_view(ui: Res<Ui>, mut last: Local<Option<View>>, mut ready: Local<bool>) {
    if !*ready {
        let Some(w) = web_sys::window() else { return };
        if get(&get(&w, "bcUi"), "update").is_undefined() {
            return;
        }
        *ready = true;
        *last = None;
    }
    let view = View::of(&ui);
    if last.as_ref() != Some(&view) {
        call_ui("update", &view.to_js());
        *last = Some(view);
    }
}

/// The special on the H key, by name.
fn special_name(kind: SpecialKind) -> Option<&'static str> {
    match kind {
        SpecialKind::None => None,
        SpecialKind::Transform { .. } => Some("Neo-Bird: changes into a fast cruiser form"),
        SpecialKind::HyperJammer { .. } => Some("Hyper Jammer: hides it from sensors"),
        SpecialKind::FullOpen { .. } => Some("Full Open Attack: every gun at once"),
        SpecialKind::MeleeMove { .. } => Some("Cross Crusher: the shield's claws"),
    }
}

/// Once: the frames to choose from, the controls sheet, and the pilot to prefill (the URL's, else
/// the last one launched).
pub fn init_page(
    cfg: Res<LaunchConfigRes>,
    settings: Option<Res<crate::settings::SettingsRes>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Some(w) = web_sys::window() else { return };
    if get(&get(&w, "bcUi"), "init").is_undefined() {
        return; // the page script isn't in yet: next frame
    }
    *done = true;
    let frames = Array::new();
    for f in PLAYABLE_ORDER {
        let spec = frame(f);
        let o = Object::new();
        set(&o, "slug", f.slug());
        set(&o, "name", frame_name(f));
        set(&o, "designation", frame_designation(f));
        let weapons = Array::new();
        for m in spec.loadout.iter().flatten() {
            weapons.push(&JsValue::from_str(weapon_name(m.weapon)));
        }
        set(&o, "weapons", weapons);
        if let Some(s) = special_name(spec.special) {
            set(&o, "special", s);
        }
        set(&o, "zero", spec.zero);
        frames.push(&o);
    }
    let controls = Array::new();
    for g in Group::ALL {
        let group = Object::new();
        set(&group, "title", g.title());
        let rows = Array::new();
        for b in BINDINGS.iter().filter(|b| b.group == g) {
            let row = Array::new();
            row.push(&JsValue::from_str(b.keys));
            row.push(&JsValue::from_str(b.action));
            rows.push(&row);
        }
        set(&group, "rows", rows);
        controls.push(&group);
    }
    let init = Object::new();
    set(&init, "frames", frames);
    set(&init, "controls", controls);
    let saved = settings.as_ref().map(|s| &s.0);
    let pick = |url: &str, saved: Option<&str>| {
        if url.is_empty() { saved.unwrap_or_default().to_string() } else { url.to_string() }
    };
    set(&init, "name", pick(&cfg.0.name, saved.map(|s| s.name.as_str())));
    set(&init, "frame", pick(&cfg.0.frame, saved.map(|s| s.frame.as_str())));
    set(&init, "autoplay", cfg.0.autoplay);
    call_ui("init", &init);
}
