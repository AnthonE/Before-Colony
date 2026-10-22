//! The page around the game. The title screen, the pause menu, the controls sheet, the reconnect
//! banner, the "click to fly" prompt, toasts, and (survival rules) the hangar's terminals and the
//! prompt on foot are HTML (`web/ui.js`), drawn over the live scene; the cockpit HUD stays in
//! Bevy. This module is the bridge, and Rust owns every decision. What crosses it is
//! `bc_client_core::page`'s, as JSON (its tests hold `web/ui.js` to the same names):
//! - the page pushes commands ([`UiCmd`]) onto `window.bcInbox` (an array), drained once a frame
//!   into [`UiCmds`];
//! - Rust sends what the page should show ([`View`]) through `window.bcUi.update(view)`, whenever
//!   it changes, and the static lists ([`Init`]: the frames, the controls, the city's map) once
//!   through `window.bcUi.init(...)`. The hangar's terminals get their data through `terminal.rs`.

use bc_client_core::bay::Spot;
use bc_client_core::page::{Init, MapAt, PanelKind, View};
use bevy::prelude::*;
use js_sys::{Array, Function, JSON, Reflect};
use wasm_bindgen::{JsCast, JsValue};

use crate::net::{LaunchConfigRes, now_s};

/// A command from the page (or from keys the page forwards), as `bc_client_core::page` defines it.
pub use bc_client_core::page::Cmd as UiCmd;
/// Which screen the page shows.
pub use bc_client_core::page::Screen;

/// This frame's commands.
#[derive(Resource, Default)]
pub struct UiCmds(pub Vec<UiCmd>);

impl UiCmds {
    pub fn has(&self, cmd: &UiCmd) -> bool {
        self.0.contains(cmd)
    }

    /// This frame's [`UiCmd::Look`]s, added up.
    pub fn look(&self) -> Vec2 {
        self.0
            .iter()
            .map(|c| match *c {
                // Degrees, yaw to the right and pitch up; the mouse's radians, y down.
                UiCmd::Look { yaw, pitch } => Vec2::new(yaw.to_radians(), -pitch.to_radians()),
                _ => Vec2::ZERO,
            })
            .sum()
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
    /// One of the hangar bay's terminals (survival rules).
    Terminal(Spot),
    /// The Charter Board (survival rules): the terminals' panel, on its own tab.
    Board,
    /// The Proving Ground's board, at the Blast Hall's desk: the terminals' panel, on its tab.
    Proving,
    /// The Most Wanted, pinned up behind the bar at The Arrival: the terminals' panel, on its tab.
    Arrival,
    /// A home's door or its panel, in the city (`bc_sim::colony::homes::HomeId`): the terminals'
    /// panel, on its tab.
    Home(u32),
}

/// The page's state, as Rust decides it.
#[derive(Resource, Default)]
pub struct Ui {
    pub screen: Screen,
    pub panel: Panel,
    /// The controls sheet (F1 or `?`) is up. It isn't modal: flying goes on under it.
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
    /// At rest on or against a body: leaving now parks the suit there.
    pub parkable: bool,
    /// In the air over a body, in its grip: leaving now, the suit settles onto it and parks.
    pub aloft: bool,
    /// Standing in one of a landmark's hide spots: its name (empty: none). Leaving there hides the
    /// suit better, and under survival rules it outlasts a server restart (`survival`).
    pub hide_spot: &'static str,
    pub survival: bool,
    /// What the link screens say.
    pub message: String,
    pub retryable: bool,
    pub reload: bool,
    pub attempt: u32,
    /// Seconds to the next redial.
    pub retry_in: u32,
    toast_seq: u32,
    toast: String,
    /// Survival rules: where the pilot is (`hangar`, `space`, `city`, or empty: arcade, or not yet
    /// told), and whether they're walking about the bay or the city (no sequence going on).
    pub place: &'static str,
    pub on_foot: bool,
    /// At the wheel in the city: `car` or `scooter` (empty: not).
    pub driving: &'static str,
    /// Flying inside the colony (its own sector), and in a trainer from the Blast Hall's gantry:
    /// leaving there, the colony's tugs bring the suit back to the bay (a trainer, to its gantry).
    pub inside: bool,
    pub trainer: bool,
    /// A launch or homecoming is playing (nothing to click, nothing to fly).
    pub sequence: bool,
    /// On foot: what the pilot can use where they stand ("E  FABRICATOR"), and their bay's line
    /// (its number, their credits).
    pub prompt: String,
    pub bay_line: String,
    /// In the colony: the map (M) is up, and where the pilot stands on it (strip, `x` along, `s`
    /// across, heading in the map's terms: radians from +x towards −s).
    pub map: bool,
    /// When Esc last closed the map (the browser's lock going, and the page's key, are one Esc).
    map_closed_at: Option<f64>,
    pub map_at: Option<[f32; 4]>,
    /// The objective's place on the map, by slug (its door gets the ◆).
    pub map_goal: Option<&'static str>,
    /// The sights found (`bc_client_core::sights`): the map marks them, and lists them.
    pub map_found: u32,
    /// A sortie's news, shown large for a few seconds.
    news_seq: u32,
    news: String,
    /// Whether the news is bad (a suit lost).
    news_bad: bool,
    /// A sortie's payout sheet (`bc_econ::debrief`): its lines, what and credits, and its net.
    debrief_seq: u32,
    debrief: Vec<(String, i64)>,
    debrief_net: i64,
    /// The colony radio's line is open: the pilot is typing (no keys reach the suit).
    pub chat: bool,
    /// The radio's latest lines, who and what, and how many have been heard (`chat.rs`).
    pub radio: Vec<(String, String)>,
    pub radio_seq: u32,
}

impl Ui {
    pub fn playing(&self) -> bool {
        self.screen == Screen::Playing
    }

    /// A panel is up, or the radio's line is open: either way the keys aren't the suit's.
    pub fn panel_open(&self) -> bool {
        self.panel != Panel::None || self.chat
    }

    pub fn open_pause(&mut self) {
        if self.playing() && self.panel == Panel::None {
            self.panel = Panel::Pause;
            self.paused_at = now_s();
        }
    }

    /// Esc with the colony's map up closes it, as M does. Chrome keeps the Esc that ends a pointer
    /// lock to itself (the lock going would open the menu) and other browsers pass it on too:
    /// whichever comes first closes the map, and the other, a moment later, is the same Esc.
    pub fn esc_closes_map(&mut self) -> bool {
        let now = now_s();
        if self.map && self.panel == Panel::None {
            self.map = false;
            self.map_closed_at = Some(now);
            return true;
        }
        self.map_closed_at.is_some_and(|at| now - at < 0.4)
    }

    fn close_settings(&mut self, from_pause: bool) {
        self.panel = if from_pause && self.playing() { Panel::Pause } else { Panel::None };
    }

    /// A short message at the top of the screen.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast_seq += 1;
        self.toast = text.into();
    }

    /// A sortie's news, large, in the middle of the screen (`bad`: in red).
    pub fn news(&mut self, text: impl Into<String>, bad: bool) {
        self.news_seq += 1;
        self.news = text.into();
        self.news_bad = bad;
    }

    /// A sortie's payout sheet, under its news for a while.
    pub fn debrief(&mut self, d: &bc_econ::Debrief) {
        self.debrief_seq += 1;
        self.debrief = d.lines.iter().map(|l| (l.what.clone(), l.cr)).collect();
        self.debrief_net = d.net();
    }

    /// The terminal open, if one is.
    pub fn terminal(&self) -> Option<Spot> {
        match self.panel {
            Panel::Terminal(spot) => Some(spot),
            _ => None,
        }
    }

    /// The terminals' tab to open on (empty: no terminal is open).
    pub fn terminal_tab(&self) -> &'static str {
        match self.panel {
            Panel::Board => "charter",
            Panel::Proving => "proving",
            Panel::Arrival => "wanted",
            Panel::Home(_) => "home",
            _ => self.terminal().map_or("", Spot::slug),
        }
    }
}

pub fn get(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

/// A command as the page wrote it (an object on `bcInbox`).
fn parse(v: &JsValue) -> Option<UiCmd> {
    UiCmd::parse(&JSON::stringify(v).ok()?.as_string()?)
}

/// `value` as the page takes it: a plain object.
pub fn to_js(value: &impl serde::Serialize) -> JsValue {
    serde_json::to_string(value).ok().and_then(|json| JSON::parse(&json).ok()).unwrap_or(JsValue::UNDEFINED)
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

/// Esc, F1 (or `?`), Resume and the pause menu's own buttons (the link's commands are `session`'s).
pub fn apply_ui_cmds(cmds: Res<UiCmds>, mut ui: ResMut<Ui>, mut map: ResMut<crate::map::MapOpen>) {
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Back => {
                if ui.help {
                    ui.help = false;
                } else if map.0 && ui.panel == Panel::None {
                    // Esc closes the chart, as M does.
                    map.0 = false;
                } else if ui.esc_closes_map() {
                    // The colony's map, likewise.
                } else {
                    match ui.panel {
                        Panel::Settings { from_pause } => ui.close_settings(from_pause),
                        Panel::Terminal(_)
                        | Panel::Board
                        | Panel::Proving
                        | Panel::Arrival
                        | Panel::Home(_) => ui.panel = Panel::None,
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
            UiCmd::Help { show } => ui.help = show.unwrap_or(!ui.help),
            UiCmd::Settings { show: true } => {
                let from_pause = ui.panel == Panel::Pause;
                ui.panel = Panel::Settings { from_pause };
            }
            UiCmd::Settings { show: false } => {
                if let Panel::Settings { from_pause } = ui.panel {
                    ui.close_settings(from_pause);
                }
            }
            _ => {}
        }
    }
    // The menu and the terminals are the world's; settings may open over the title too.
    if !ui.playing()
        && matches!(
            ui.panel,
            Panel::Pause
                | Panel::Terminal(_)
                | Panel::Board
                | Panel::Proving
                | Panel::Arrival
                | Panel::Home(_)
        )
    {
        ui.panel = Panel::None;
    }
    if !ui.playing() && ui.panel == (Panel::Settings { from_pause: true }) {
        ui.panel = Panel::Settings { from_pause: false };
    }
}

/// What the page draws, from the page's state as Rust decides it.
fn view_of(ui: &Ui) -> View {
    View {
        screen: ui.screen,
        panel: match ui.panel {
            Panel::None => PanelKind::None,
            Panel::Pause => PanelKind::Pause,
            Panel::Settings { .. } => PanelKind::Settings,
            Panel::Terminal(_) | Panel::Board | Panel::Proving | Panel::Arrival | Panel::Home(_) => {
                PanelKind::Terminal
            }
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
        aloft: ui.aloft,
        hide_spot: ui.hide_spot,
        survival: ui.survival,
        terminal: ui.terminal_tab(),
        place: ui.place,
        on_foot: ui.on_foot,
        driving: ui.driving,
        inside: ui.inside,
        trainer: ui.trainer,
        sequence: ui.sequence,
        prompt: ui.prompt.clone(),
        bay_line: ui.bay_line.clone(),
        map: ui.map_at.filter(|_| ui.map).map(|[strip, x, s, heading]| MapAt { strip, x, s, heading }),
        map_goal: ui.map_goal.filter(|_| ui.map),
        map_found: ui.map_found,
        news_seq: ui.news_seq,
        news: ui.news.clone(),
        news_bad: ui.news_bad,
        debrief_seq: ui.debrief_seq,
        debrief: ui.debrief.clone(),
        debrief_net: ui.debrief_net,
        chat: ui.chat,
        radio_seq: ui.radio_seq,
        radio: ui.radio.clone(),
    }
}

pub fn call_ui(method: &str, arg: &JsValue) {
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
    let view = view_of(&ui);
    if last.as_ref() != Some(&view) {
        call_ui("update", &to_js(&view));
        *last = Some(view);
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
    let saved = settings.as_ref().map(|s| &s.0);
    let pick = |url: &str, saved: Option<&str>| {
        if url.is_empty() { saved.unwrap_or_default().to_string() } else { url.to_string() }
    };
    let init = Init::new(
        pick(&cfg.0.name, saved.map(|s| s.name.as_str())),
        pick(&cfg.0.frame, saved.map(|s| s.frame.as_str())),
        cfg.0.autoplay,
    );
    call_ui("init", &to_js(&init));
}
