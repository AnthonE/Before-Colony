//! Proximity voice in the browser (`bc_proto::voice`; the server picks who hears, `bc_server::voice`).
//!
//! The codec is the page's: `web/voice.js` (`window.bcVoice`) taps the microphone with an
//! AudioWorklet (`web/voice-worklet.js`), encodes it with the browser's own WebCodecs Opus (24 kbps,
//! 20 ms frames), and decodes each talker's frames with an AudioDecoder of their own: off the game's
//! frame, and a peer's bytes never reach a codec inside the wasm. Rust decides everything else:
//!
//! - **When the pilot is on the air:** ` or the mouse's back button, held or toggled
//!   (`bc_client_core::voice::Keyed`), never while a panel or the radio's line has the keys, nor
//!   where a voice doesn't carry (the bay; flying inside the colony). The page hands each encoded
//!   frame to `window.bcVoiceUp`, installed here, which refuses it while the key is up (but the last,
//!   its END) and sends it as a datagram.
//! - **Who's heard:** relayed frames (split off at arrival, `transport.rs`) go to the page with who's
//!   talking, unless they're muted (`/mute`, kept by callsign in localStorage, never sent).
//! - **Where from:** on foot, each talker's mouth from the camera and how loud
//!   (`bc_client_core::voice::falloff`), for their PannerNode; in a suit, the radio, through a
//!   band-pass, with its squelch (`bc_sound`'s `RadioOpen` and `RadioClose`) as it opens and closes.
//! - **The page's corner** (`page::VoiceView`): on the air, the microphone's state, who's talking.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bc_client_core::ClientCore;
use bc_client_core::page::VoiceView;
use bc_client_core::voice::{Keyed, Mute, REACH_M, TALKING_S, falloff, find, talker_key};
use bc_proto::voice::{Via, VoiceFrame};
use bc_sound::Cue;
use bc_sound::mixer::Request;
use bevy::prelude::*;
use js_sys::{Array, Float32Array, Function, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

use crate::camera::MainCamera;
use crate::dev_hooks::DevStatus;
use crate::net::{GameClient, NetState, now_s};
use crate::page::{Ui, UiCmd, UiCmds};
use crate::settings::SettingsRes;

/// `window.bcVoiceUp(seq, end, bytes)`: whether the frame went.
type VoiceUp = Closure<dyn FnMut(u16, bool, Uint8Array) -> bool>;

/// Where the muted callsigns are kept.
const MUTES: &str = "bc.mutes";
/// After the key goes up, the frame that ends the go (its END) may still go out for this long, s.
const END_GRACE: f64 = 0.3;
/// A talker not heard from for this long is forgotten (their lane in the page goes too), s.
const FORGET_S: f64 = 5.0;
/// A refused microphone stays on the corner this long after the press that asked for it, s.
const MIC_SAID_S: f64 = 6.0;

/// Where the pilot's voice carries from, as the server will have it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Carry {
    /// On foot: through the air.
    Air,
    /// At the controls of a live suit out in space: its radio.
    Radio,
    Nowhere,
}

fn carry(core: &ClientCore) -> Carry {
    let h = &core.hangar;
    if h.in_city() || h.in_base() || h.in_town() {
        return Carry::Air;
    }
    let survival = core.welcome.is_some_and(|w| w.survival);
    let in_space = !survival || h.place == Some(bc_econ::wire::Place::Space);
    if in_space && !core.inside() && core.world.own.is_some_and(|o| o.alive) {
        Carry::Radio
    } else {
        Carry::Nowhere
    }
}

struct Talker {
    name: String,
    talker: u16,
    via: Via,
    heard: f64,
    ended: bool,
}

impl Talker {
    fn talking(&self, now: f64) -> bool {
        !self.ended && now - self.heard < TALKING_S
    }
}

/// Non-send: the voice's state on the Rust side.
pub struct Voice {
    keyed: Keyed,
    /// Keyed now, and when it last was (s): what `window.bcVoiceUp` reads.
    held: Rc<Cell<bool>>,
    held_at: Rc<Cell<f64>>,
    /// Frames sent and refused, for the dev hooks.
    sent: Rc<Cell<u32>>,
    refused: Rc<Cell<u32>>,
    talkers: HashMap<String, Talker>,
    mutes: HashSet<String>,
    was_keyed: bool,
    /// The page told voice is on (None: not yet told).
    told_on: Option<bool>,
    /// When the talk key was last pressed (the corner says a refused microphone for a while after).
    pressed_at: f64,
    heard: u32,
    up: Option<VoiceUp>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            keyed: Keyed::default(),
            held: Rc::default(),
            held_at: Rc::new(Cell::new(f64::NEG_INFINITY)),
            sent: Rc::default(),
            refused: Rc::default(),
            talkers: HashMap::new(),
            mutes: load_mutes(),
            was_keyed: false,
            told_on: None,
            pressed_at: f64::NEG_INFINITY,
            heard: 0,
            up: None,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn load_mutes() -> HashSet<String> {
    let text = storage().and_then(|s| s.get_item(MUTES).ok().flatten()).unwrap_or_default();
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| !n.is_empty())
        .collect()
}

fn save_mutes(mutes: &HashSet<String>) {
    let mut list: Vec<&String> = mutes.iter().collect();
    list.sort();
    list.truncate(200);
    if let (Some(s), Ok(text)) = (storage(), serde_json::to_string(&list)) {
        let _ = s.set_item(MUTES, &text);
    }
}

/// `window.bcVoice`, once the page's script is in.
fn page() -> Option<JsValue> {
    let w = web_sys::window()?;
    let v = Reflect::get(&w, &JsValue::from_str("bcVoice")).ok()?;
    (!v.is_undefined() && !v.is_null()).then_some(v)
}

fn call(v: &JsValue, method: &str, args: &[JsValue]) -> JsValue {
    let Ok(f) = Reflect::get(v, &JsValue::from_str(method)).and_then(|f| f.dyn_into::<Function>()) else {
        return JsValue::UNDEFINED;
    };
    let args: Array = args.iter().collect();
    f.apply(v, &args).unwrap_or(JsValue::UNDEFINED)
}

/// What the page says of the microphone, as the corner names it.
fn mic_state(v: Option<&JsValue>) -> &'static str {
    let s = v.and_then(|v| Reflect::get(v, &JsValue::from_str("mic")).ok()).and_then(|s| s.as_string());
    match s.as_deref() {
        Some("asking") => "asking",
        Some("on") => "on",
        Some("denied") => "denied",
        Some("none") => "none",
        Some("busy") => "busy",
        Some("unsupported") => "unsupported",
        Some("failed") => "failed",
        _ => "off",
    }
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(Voice::default());
    }
}

/// `window.bcVoiceUp(seq, end, bytes)`: the page's encoder hands each frame here. It goes out as a
/// datagram on whatever link is up now, while the key is down (and, its END, just after).
fn install_up(voice: &mut Voice, net: &NetState) {
    let (slot, held, held_at) = (net.transport.clone(), voice.held.clone(), voice.held_at.clone());
    let (sent, refused) = (voice.sent.clone(), voice.refused.clone());
    let up = VoiceUp::new(move |seq, end, bytes: Uint8Array| {
        let live = held.get() || (end && now_s() - held_at.get() < END_GRACE);
        let datagram = live.then(|| ClientCore::voice_up(seq, end, &bytes.to_vec())).flatten();
        let ok = match (datagram, slot.borrow().as_ref()) {
            (Some(d), Some(t)) => t.send_datagram(&d),
            _ => false,
        };
        let counter = if ok { &sent } else { &refused };
        counter.set(counter.get().wrapping_add(1));
        ok
    });
    if let Some(w) = web_sys::window() {
        let _ = Reflect::set(&w, &JsValue::from_str("bcVoiceUp"), up.as_ref());
    }
    voice.up = Some(up);
}

/// `/mute` and `/unmute`: answered here (a toast), never sent (`chat.rs` keeps them off the radio).
fn mute(voice: &mut Voice, m: Mute, near: &[String], now: f64) -> String {
    // Only who's known by name: a talker not yet resolved ("") is no callsign to keep, and muting
    // "" would silence every talker not yet resolved.
    let anyone = voice.talkers.values().any(|t| t.talking(now));
    let talking: Vec<String> = voice
        .talkers
        .values()
        .filter(|t| t.talking(now) && !t.name.is_empty())
        .map(|t| t.name.clone())
        .collect();
    let said = match m {
        Mute::Mute(None) if !anyone => return "VOICE: NOBODY IS TALKING (/MUTE NAME)".into(),
        Mute::Mute(None) if talking.is_empty() => {
            return "VOICE: NOBODY TO MUTE BY NAME YET (/MUTE NAME)".into();
        }
        Mute::Mute(None) => {
            voice.mutes.extend(talking.iter().map(|n| n.to_lowercase()));
            format!("VOICE: MUTED {}", talking.join(", "))
        }
        Mute::Mute(Some(who)) => {
            let names = near.iter().chain(voice.talkers.values().map(|t| &t.name)).map(String::as_str);
            let Some(name) = find(&who, names) else {
                return format!("VOICE: NOBODY NEAR IS CALLED {}", who.to_uppercase());
            };
            voice.mutes.insert(name.to_lowercase());
            format!("VOICE: MUTED {name}")
        }
        Mute::Unmute(None) => {
            voice.mutes.clear();
            "VOICE: EVERYONE UNMUTED".into()
        }
        Mute::Unmute(Some(who)) => {
            let Some(name) = find(&who, voice.mutes.iter().map(String::as_str)).map(str::to_string) else {
                return format!("VOICE: {} ISN'T MUTED", who.to_uppercase());
            };
            voice.mutes.remove(&name);
            format!("VOICE: UNMUTED {}", name.to_uppercase())
        }
    };
    save_mutes(&voice.mutes);
    said
}

/// Once a frame: the talk key, the frames heard, where each talker is, and the page's corner.
#[allow(clippy::too_many_arguments)]
pub fn drive_voice(
    mut voice: NonSendMut<Voice>,
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    (keys, mouse): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    cmds: Res<UiCmds>,
    settings: Res<SettingsRes>,
    mut ui: ResMut<Ui>,
    heads: Res<crate::people::Heads>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut sound: ResMut<crate::audio::Sound>,
    mut dev: ResMut<DevStatus>,
) {
    let voice = &mut *voice;
    if voice.up.is_none() {
        install_up(voice, &net);
    }
    let now = now_s();
    let js = page();
    let s = &settings.0;
    let on = s.voice_chat;
    if voice.told_on != Some(on)
        && let Some(v) = &js
    {
        call(v, "configure", &[JsValue::from_bool(on)]);
        voice.told_on = Some(on);
    }
    let g = game.borrow();
    let core = &g.core;
    let here = carry(core);

    // `/mute` lines.
    for cmd in &cmds.0 {
        if let UiCmd::Say { text } = cmd
            && let Some(m) = Mute::parse(text)
        {
            let mut near: Vec<String> = core.hangar.people.values().cloned().collect();
            near.extend(core.world.roster.values().map(|(n, _)| n.clone()));
            let said = mute(voice, m, &near, now);
            ui.toast(said);
        }
    }

    // The talk key.
    let held = keys.pressed(KeyCode::Backquote) || mouse.pressed(MouseButton::Back);
    let pressed = keys.just_pressed(KeyCode::Backquote) || mouse.just_pressed(MouseButton::Back);
    let free = ui.playing() && !ui.panel_open();
    if pressed && free {
        voice.pressed_at = now;
        if !on {
            ui.toast("VOICE IS OFF: SETTINGS > SOUND");
        } else if here == Carry::Nowhere {
            ui.toast("NOBODY HEARS YOU HERE: ON FOOT IN THE TOWN, OR OVER YOUR SUIT'S RADIO");
        }
    }
    let can = on && free && here != Carry::Nowhere;
    let keyed = voice.keyed.step(s.talk_mode, can, held, pressed);
    voice.held.set(keyed);
    if keyed {
        voice.held_at.set(now);
    }
    if keyed != voice.was_keyed {
        if let Some(v) = &js {
            call(v, "setKeyed", &[JsValue::from_bool(keyed)]);
        }
        // The suit's radio keying, and letting go.
        if here == Carry::Radio {
            sound.mixer.request(Request::own(if keyed { Cue::RadioOpen } else { Cue::RadioClose }));
        }
        voice.was_keyed = keyed;
    }

    // What's been heard.
    if let Some(t) = net.get() {
        for bytes in t.drain_voice() {
            let Ok(f) = VoiceFrame::decode_down(&bytes) else { continue };
            if !on {
                continue;
            }
            let name = match f.via {
                Via::Air => core.hangar.people.get(&f.talker).cloned().unwrap_or_default(),
                Via::Radio => core.world.name_of(f.talker),
            };
            if voice.mutes.contains(&name.to_lowercase()) {
                continue;
            }
            let key = talker_key(f.talker, f.via);
            let was = voice.talkers.get(&key).is_some_and(|t| t.talking(now));
            voice
                .talkers
                .insert(key.clone(), Talker { name, talker: f.talker, via: f.via, heard: now, ended: f.end });
            if f.via == Via::Radio && (!was || f.end) {
                sound.mixer.request(Request::own(if f.end { Cue::RadioClose } else { Cue::RadioOpen }));
            }
            if let Some(v) = &js {
                let payload = Uint8Array::from(f.payload());
                let args = [
                    JsValue::from_str(&key),
                    JsValue::from_bool(f.via == Via::Radio),
                    JsValue::from(f.seq),
                    JsValue::from_bool(f.end),
                    payload.into(),
                ];
                call(v, "hear", &args);
            }
            voice.heard = voice.heard.wrapping_add(1);
        }
        dev.set("voice_dropped", t.voice_dropped() as u32);
    }
    voice.talkers.retain(|_, t| now - t.heard < FORGET_S);

    // Where each voice through the air comes from: their mouth from the camera, and how loud.
    if let (Some(v), Ok(cam)) = (&js, camera.single()) {
        let air: Vec<(&String, &Talker)> =
            voice.talkers.iter().filter(|(_, t)| t.via == Via::Air && now - t.heard < 1.0).collect();
        if !air.is_empty() {
            let keys: Array = air.iter().map(|(k, _)| JsValue::from_str(k)).collect();
            let mut at = Vec::with_capacity(air.len() * 4);
            let (eye, turn) = (cam.translation(), cam.rotation().inverse());
            for (_, t) in &air {
                let (rel, gain) = match heads.0.get(&t.talker) {
                    Some(mouth) => (turn * (*mouth - eye), falloff(mouth.distance(eye))),
                    // Not drawn (out of view's reach a moment): ahead, at half the reach's level.
                    None => (Vec3::NEG_Z, falloff(REACH_M * 0.5)),
                };
                at.extend_from_slice(&[rel.x, rel.y, rel.z, gain]);
            }
            call(v, "place", &[keys.into(), Float32Array::from(&at[..]).into()]);
        }
    }

    // The corner.
    let mic = mic_state(js.as_ref());
    let mut talking: Vec<String> = voice
        .talkers
        .values()
        .filter(|t| t.talking(now))
        .map(|t| if t.name.is_empty() { "SOMEONE".to_string() } else { t.name.to_uppercase() })
        .collect();
    talking.sort();
    let refused = matches!(mic, "denied" | "none" | "busy" | "unsupported" | "failed");
    let show = on
        && ui.playing()
        && (keyed || !talking.is_empty() || (refused && now - voice.pressed_at < MIC_SAID_S));
    let want =
        show.then(|| VoiceView { on_air: keyed && mic == "on", radio: here == Carry::Radio, mic, talking });
    if ui.voice != want {
        ui.voice = want;
    }
    dev.set("voice_keyed", keyed);
    dev.set("voice_heard", voice.heard);
    dev.set("voice_sent", voice.sent.get());
    dev.set("voice_refused", voice.refused.get());
    dev.set("voice_mic", mic);
}
