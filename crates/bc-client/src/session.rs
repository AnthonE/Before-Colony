//! Game mode's link to the server: dials when the page says Play, notices the handshake's outcome
//! and the link dropping, redials, and keeps the page's link screens in step. The rules (timeouts,
//! backoff, what gives up) are `bc_client_core::session`'s; this is the browser half.
//!
//! Every dial asks the page for the server's address and certificate hash again
//! (`window.bcDiscover`): the dev server makes a new self-signed certificate each time it starts,
//! so a redial after a restart needs the new hash.

use std::cell::RefCell;
use std::rc::Rc;

use bc_client_core::Phase;
use bc_client_core::session::{Link, LinkAction, LinkError, LinkState};
use bc_proto::FrameId;
use bevy::prelude::*;
use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::config::parse_frame;
use crate::input::{Aim, Controls};
use crate::net::{GameClient, LaunchConfigRes, NetState, now_s};
use crate::page::{Screen, Ui, UiCmd, UiCmds};
use crate::transport::Transport;

/// Who is flying: the callsign and frame chosen on the title screen (the frame follows respawns,
/// so a reconnect brings back the suit being flown).
#[derive(Resource, Clone)]
pub struct Pilot {
    pub name: String,
    pub frame: FrameId,
}

/// The link's state machine.
#[derive(Resource, Default)]
pub struct LinkRes(pub Link);

/// A dial's outcome, as the async task left it.
type DialResult = Rc<RefCell<Option<(u32, Result<Transport, String>)>>>;

/// Non-send: the dial in flight (numbered, so a late result from an abandoned dial is dropped).
#[derive(Default)]
pub struct Dialer {
    generation: u32,
    result: DialResult,
}

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LinkRes>().insert_non_send(Dialer::default());
    }
}

/// The server's address and certificate hash, from the page (`window.bcDiscover`), or the ones
/// the page was loaded with.
async fn discover(fallback: &LaunchConfigRes) -> Result<(String, Option<Vec<u8>>), String> {
    let fallback = || Ok((fallback.0.wt_url.clone(), fallback.0.cert_hash.clone()));
    let Some(w) = web_sys::window() else { return fallback() };
    let Ok(f) = Reflect::get(&w, &JsValue::from_str("bcDiscover")).and_then(|f| f.dyn_into::<Function>())
    else {
        return fallback();
    };
    let promise = f.call0(&JsValue::NULL).map_err(|e| js_text(&e))?;
    let info = JsFuture::from(Promise::resolve(&promise)).await.map_err(|e| js_text(&e))?;
    let url = Reflect::get(&info, &JsValue::from_str("wtUrl")).ok().and_then(|v| v.as_string());
    let hash = Reflect::get(&info, &JsValue::from_str("certHash"))
        .ok()
        .and_then(|v| v.as_string())
        .and_then(|h| crate::config::decode_hex(&h));
    match url {
        Some(url) => Ok((url, hash)),
        None => Err("the server's page didn't say where to connect".into()),
    }
}

/// A JS error's message.
pub fn js_text(e: &JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

fn start_dial(dialer: &mut Dialer, cfg: &LaunchConfigRes) {
    dialer.generation = dialer.generation.wrapping_add(1);
    let generation = dialer.generation;
    let result = dialer.result.clone();
    let cfg = cfg.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let outcome = match discover(&cfg).await {
            Ok((url, hash)) => Transport::connect(&url, hash).await,
            Err(e) => Err(e),
        };
        *result.borrow_mut() = Some((generation, outcome));
    });
}

/// Drops the transport (the network timer stops pumping it) and closes it.
fn close(net: &NetState) {
    if let Some(t) = net.transport.borrow_mut().take() {
        t.close();
    }
}

#[allow(clippy::too_many_arguments)]
fn act(
    action: LinkAction,
    net: &NetState,
    dialer: &mut Dialer,
    game: &GameClient,
    pilot: &Pilot,
    cfg: &LaunchConfigRes,
    controls: &mut Controls,
    aim: &mut Aim,
) {
    close(net);
    // An empty world either way: a new session starts from nothing, and a dead one shows nothing.
    game.reset(&pilot.name, pilot.frame);
    *controls = Controls::default();
    aim.initialized_for = None;
    if action == LinkAction::Dial {
        start_dial(dialer, cfg);
    }
}

/// Once a frame, before input: the page's link commands, the dial's outcome, the handshake, the
/// link's health, timeouts and redials; then what the page should show.
#[allow(clippy::too_many_arguments)]
pub fn drive_link(
    cmds: Res<UiCmds>,
    cfg: Res<LaunchConfigRes>,
    net: NonSend<NetState>,
    mut dialer: NonSendMut<Dialer>,
    game: NonSend<GameClient>,
    mut link: ResMut<LinkRes>,
    mut pilot: ResMut<Pilot>,
    mut ui: ResMut<Ui>,
    mut controls: ResMut<Controls>,
    mut aim: ResMut<Aim>,
    mut started: Local<bool>,
) {
    let now = now_s();
    let mut actions: Vec<LinkAction> = Vec::new();
    if !*started {
        *started = true;
        if cfg.0.no_web_transport {
            link.0.unsupported();
        } else if cfg.0.autoplay {
            actions.extend(link.0.play(now));
        }
    }
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Play { name, frame } => {
                pilot.name = name.trim().chars().take(bc_proto::control::MAX_NAME).collect();
                if let Some(f) = parse_frame(frame) {
                    pilot.frame = f;
                }
                actions.extend(link.0.play(now));
            }
            UiCmd::Retry => actions.extend(link.0.play(now)),
            UiCmd::Cancel => actions.push(link.0.cancel()),
            UiCmd::Disconnect => {
                // Goodbye first, so the server knows the pilot left on purpose.
                if let Some(t) = net.transport.borrow_mut().take() {
                    t.close_with(game.borrow().core.bye(0));
                }
                actions.push(link.0.cancel());
            }
            UiCmd::DropLink => {
                if let Some(t) = net.get() {
                    t.close();
                }
            }
            _ => {}
        }
    }

    // The dial's outcome.
    let finished = dialer.result.borrow_mut().take();
    if let Some((generation, outcome)) = finished {
        match outcome {
            Ok(t) if generation == dialer.generation && matches!(link.0.state, LinkState::Dialing { .. }) => {
                t.start_pump(now_s);
                *net.transport.borrow_mut() = Some(t);
                link.0.dialed(now);
            }
            // An abandoned dial that got through after all.
            Ok(t) => t.close(),
            Err(e) if generation == dialer.generation => {
                web_sys::console::warn_1(&format!("dial failed: {e}").into());
                actions.extend(link.0.dial_failed(now, e));
            }
            Err(_) => {}
        }
    }

    // The handshake, and the link's health.
    if let Some(t) = net.get() {
        let phase = game.borrow().core.phase;
        match phase {
            Phase::InGame if matches!(link.0.state, LinkState::Handshake { .. }) => link.0.welcomed(now),
            Phase::Rejected(reason) => actions.extend(link.0.rejected(now, reason)),
            Phase::Closed => actions.extend(link.0.server_bye(now, 0)),
            _ if t.is_closed() => actions.extend(link.0.lost(now)),
            _ => {}
        }
    }
    actions.extend(link.0.tick(now));
    for a in actions {
        act(a, &net, &mut dialer, &game, &pilot, &cfg, &mut controls, &mut aim);
    }

    // What the page shows.
    let l = &link.0;
    ui.screen = match l.state {
        LinkState::Idle => Screen::Title,
        LinkState::Dialing { attempt: 0, .. } | LinkState::Handshake { attempt: 0, .. } => Screen::Connecting,
        LinkState::Dialing { .. } | LinkState::Handshake { .. } | LinkState::Retrying { .. } => {
            Screen::Reconnecting
        }
        LinkState::InGame { .. } => Screen::Playing,
        LinkState::Failed(_) => Screen::Failed,
    };
    ui.attempt = l.attempt();
    ui.retry_in = match l.state {
        LinkState::Retrying { at, .. } => (at - now).max(0.0).ceil() as u32,
        _ => 0,
    };
    let err: Option<&LinkError> = match &l.state {
        LinkState::Failed(e) | LinkState::Retrying { why: e, .. } => Some(e),
        LinkState::Dialing { attempt, .. } | LinkState::Handshake { attempt, .. } if *attempt > 0 => {
            l.last_error.as_ref()
        }
        _ => None,
    };
    ui.message = err.map(LinkError::text).unwrap_or_default();
    ui.retryable = err.is_some_and(LinkError::retryable);
    ui.reload = err.is_some_and(LinkError::needs_reload);
}
