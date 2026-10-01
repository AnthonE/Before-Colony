//! Game mode's link to the server: dials when the page says Play, notices the handshake's outcome
//! and the link dropping, redials, and keeps the page's link screens in step. The rules (timeouts,
//! backoff, what gives up) are `bc_client_core::session`'s; this is the browser half.
//!
//! Every dial asks the page for the server's address and certificate hash again
//! (`window.bcDiscover`): the dev server makes a new self-signed certificate each time it starts,
//! so a redial after a restart needs the new hash.
//!
//! Signing in: the server sends a challenge, the core writes the sign-in text (EIP-4361, the same
//! bytes the server rebuilds to verify), and the page's wallet signs it
//! (`window.bcWallet.sign(text, address)`, a promise of the `0x…` signature). The server then
//! hands out a resume token, so a redial, or a reload (the token is kept in sessionStorage), comes
//! back without asking the wallet again.

use std::cell::RefCell;
use std::rc::Rc;

use bc_client_core::session::{Link, LinkAction, LinkError, LinkState};
use bc_client_core::{Identity, Phase};
use bc_proto::FrameId;
use bc_proto::auth::{Address, TOKEN_BYTES};
use bc_proto::control::{RejectReason, notice};
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
/// so a reconnect brings back the suit being flown), and the wallet they signed in with.
#[derive(Resource, Clone)]
pub struct Pilot {
    pub name: String,
    pub frame: FrameId,
    /// The wallet (`None`: a guest).
    pub address: Option<Address>,
    /// The latest resume token and whose it is: the next dial as that wallet comes back on it
    /// rather than asking the wallet to sign again.
    pub resume: Option<(Address, [u8; TOKEN_BYTES])>,
}

impl Pilot {
    /// A guest, with the resume token this tab kept (a reload signs back in on it).
    pub fn new(name: String, frame: FrameId) -> Self {
        Self { name, frame, address: None, resume: load_token() }
    }

    fn identity(&self) -> Identity {
        match self.address {
            Some(address) => Identity::Wallet {
                address,
                resume: self.resume.filter(|(a, _)| *a == address).map(|(_, t)| t),
            },
            None => Identity::Guest,
        }
    }

    fn keep_token(&mut self, token: Option<(Address, [u8; TOKEN_BYTES])>) {
        if self.resume != token {
            self.resume = token;
            save_token(token);
        }
    }
}

/// Where this tab keeps its resume token (sessionStorage: it goes with the tab).
const TOKEN_KEY: &str = "bc.resume";

fn session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok()?
}

fn load_token() -> Option<(Address, [u8; TOKEN_BYTES])> {
    let text = session_storage()?.get_item(TOKEN_KEY).ok()??;
    let (address, token) = text.split_once(' ')?;
    let token = crate::config::decode_hex(token)?.try_into().ok()?;
    Some((bc_auth::parse_address(address)?, token))
}

fn save_token(token: Option<(Address, [u8; TOKEN_BYTES])>) {
    let Some(store) = session_storage() else { return };
    let _ = match token {
        Some((address, token)) => {
            let hex: String = token.iter().map(|b| format!("{b:02x}")).collect();
            store.set_item(TOKEN_KEY, &format!("{} {hex}", short_hex(&address, false)))
        }
        None => store.remove_item(TOKEN_KEY),
    };
}

/// An address as `0x…` hex (`short`: `0x1234…abcd`).
pub fn short_hex(address: &Address, short: bool) -> String {
    let hex = bc_auth::checksum_hex(address);
    let s = core::str::from_utf8(&hex).unwrap_or_default();
    if short { format!("{}…{}", &s[..6], &s[s.len() - 4..]) } else { s.to_string() }
}

/// The link's state machine.
#[derive(Resource, Default)]
pub struct LinkRes(pub Link);

/// A dial's outcome, as the async task left it.
type DialResult = Rc<RefCell<Option<(u32, Result<Transport, String>)>>>;
/// The wallet's answer, as the async task left it: a `0x…` signature, or why not.
type SignResult = Rc<RefCell<Option<(u32, Result<String, String>)>>>;

/// Non-send: the dial in flight, and the wallet's signature for it (numbered, so a late result
/// from an abandoned dial is dropped).
#[derive(Default)]
pub struct Dialer {
    generation: u32,
    result: DialResult,
    signature: SignResult,
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

/// A JS error's message (wallets reject with plain objects that have one).
pub fn js_text(e: &JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .or_else(|| Reflect::get(e, &JsValue::from_str("message")).ok().and_then(|m| m.as_string()))
        .unwrap_or_else(|| format!("{e:?}"))
}

/// Hands the sign-in text to the page's wallet; its answer lands in `dialer.signature`.
fn ask_wallet(dialer: &Dialer, text: Option<String>, address: Option<Address>) {
    let generation = dialer.generation;
    let slot = dialer.signature.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let answer = match (text, address) {
            (Some(text), Some(address)) => sign(&text, &address).await,
            _ => Err("there was nothing to sign".into()),
        };
        *slot.borrow_mut() = Some((generation, answer));
    });
}

/// `window.bcWallet.sign(text, address)`.
async fn sign(text: &str, address: &Address) -> Result<String, String> {
    let w = web_sys::window().ok_or("no window")?;
    let wallet = Reflect::get(&w, &JsValue::from_str("bcWallet")).map_err(|e| js_text(&e))?;
    let f = Reflect::get(&wallet, &JsValue::from_str("sign"))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .ok_or("this browser has no wallet")?;
    let promise = f
        .call2(&wallet, &JsValue::from_str(text), &JsValue::from_str(&short_hex(address, false)))
        .map_err(|e| js_text(&e))?;
    let answer = JsFuture::from(Promise::resolve(&promise)).await.map_err(|e| js_text(&e))?;
    answer.as_string().ok_or_else(|| "the wallet answered with something that isn't a signature".into())
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
    game.reset(&pilot.name, pilot.frame, pilot.identity());
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
            UiCmd::Play { name, frame, address } => {
                pilot.name = name.trim().chars().take(bc_proto::control::MAX_NAME).collect();
                if let Some(f) = parse_frame(frame) {
                    pilot.frame = f;
                }
                pilot.address = bc_auth::parse_address(address);
                if pilot.address.is_none() && !address.trim().is_empty() {
                    web_sys::console::warn_1(&format!("not a wallet address: {address}").into());
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

    // The wallet's signature: on to the server.
    let answer = dialer.signature.borrow_mut().take();
    if let Some((generation, answer)) = answer
        && generation == dialer.generation
        && matches!(link.0.state, LinkState::Signing { .. })
    {
        let signature = answer.and_then(|s| {
            bc_auth::parse_signature(&s).ok_or_else(|| "its answer isn't a signature".to_string())
        });
        match (signature, net.get()) {
            (Ok(signature), Some(t)) => {
                t.send_control(game.borrow_mut().core.auth(signature));
                link.0.signed(now);
            }
            (Ok(_), None) => {}
            (Err(why), _) => actions.extend(link.0.wallet_failed(why)),
        }
    }

    // The handshake, and the link's health.
    if let Some(t) = net.get() {
        let (phase, used_token, bye_reason) = {
            let g = game.borrow();
            let used_token = matches!(g.core.identity, Identity::Wallet { resume: Some(_), .. });
            (g.core.phase, used_token, g.core.bye_reason.unwrap_or(0))
        };
        match phase {
            Phase::Signing(_) if matches!(link.0.state, LinkState::Handshake { .. }) => {
                link.0.signing(now);
                ask_wallet(&dialer, game.borrow().core.sign_in_text(), pilot.address);
            }
            Phase::InGame if matches!(link.0.state, LinkState::Handshake { .. }) => {
                let first = link.0.attempt() == 0;
                link.0.welcomed(now);
                let welcome = game.borrow().core.welcome;
                match (welcome, pilot.address) {
                    (Some(w), _) if w.woke => ui.toast("YOU WAKE IN YOUR COCKPIT"),
                    (Some(w), Some(a)) if w.signed_in && first => {
                        ui.toast(format!("SIGNED IN AS {}", short_hex(&a, true)));
                    }
                    _ => {}
                }
            }
            // The server has forgotten the token (it expired, or the server restarted). The
            // player just pressed Launch: sign in afresh. A redial of its own doesn't open the
            // wallet on them; it stops and says why.
            Phase::Rejected(RejectReason::ResumeExpired) if used_token => {
                pilot.keep_token(None);
                if link.0.attempt() == 0 {
                    actions.extend(link.0.redial(now));
                } else {
                    actions.extend(link.0.rejected(now, RejectReason::ResumeExpired));
                }
            }
            Phase::Rejected(reason) => actions.extend(link.0.rejected(now, reason)),
            Phase::Closed => actions.extend(link.0.server_bye(now, bye_reason)),
            _ if t.is_closed() => actions.extend(link.0.lost(now)),
            _ => {}
        }
    }

    // The token for the next dial, and what the server wanted the pilot to know.
    {
        let mut g = game.borrow_mut();
        if let (Some(token), Some(address)) = (g.core.resume_token, pilot.address) {
            pilot.keep_token(Some((address, token)));
        }
        for (code, name) in g.core.notices.drain(..) {
            match code {
                notice::SLEEPER_DESTROYED if name.is_empty() => {
                    ui.toast("YOUR SUIT WAS DESTROYED WHILE YOU SLEPT");
                }
                notice::SLEEPER_DESTROYED => {
                    ui.toast(format!("YOUR SUIT WAS DESTROYED WHILE YOU SLEPT, BY {}", name.to_uppercase()));
                }
                notice::SLEEPER_LOST => ui.toast("YOUR SUIT WAS CLEARED FROM THE SECTOR WHILE YOU SLEPT"),
                _ => {}
            }
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
        LinkState::Dialing { attempt: 0, .. }
        | LinkState::Handshake { attempt: 0, .. }
        | LinkState::Signing { attempt: 0, .. } => Screen::Connecting,
        LinkState::Dialing { .. }
        | LinkState::Handshake { .. }
        | LinkState::Signing { .. }
        | LinkState::Retrying { .. } => Screen::Reconnecting,
        LinkState::InGame { .. } => Screen::Playing,
        LinkState::Failed(_) => Screen::Failed,
    };
    ui.signing = matches!(l.state, LinkState::Signing { .. });
    {
        let g = game.borrow();
        ui.signed_in = l.in_game() && g.core.welcome.is_some_and(|w| w.signed_in);
        ui.parkable = ui.signed_in
            && g.core.world.own.is_some_and(|o| o.flags & bc_proto::snapshot::own_flags::PARKABLE != 0);
        let alive = l.in_game() && g.core.world.own.is_some_and(|o| o.alive);
        ui.hide_spot = if alive { crate::hud::footed(&g.core).spot.unwrap_or("") } else { "" };
        ui.survival = g.core.welcome.is_some_and(|w| w.survival);
    }
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
