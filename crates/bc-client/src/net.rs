//! Connection management and the game client: bytes from the browser's WebTransport go into the
//! shared `ClientCore`; commands come back out as datagrams.
//!
//! The network loop ([`pump`]) runs on a browser timer as well as once per rendered frame, so
//! inputs, the autopilot and clock sync keep their 30 Hz cadence even when rendering is slow
//! (software GPUs, hitches). Both run on the page's single thread, so they never overlap.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use bc_client_core::{ClientConfig, ClientCore, DollBrain};
use bc_proto::buttons::ZERO;
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::content::frame;
use bevy::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::config::LaunchConfig;
use crate::dev_hooks::DevStatus;
use crate::input::{Aim, Controls};
use crate::transport::Transport;

/// Non-send: the browser session is single-threaded JS state.
#[derive(Default)]
pub struct NetState {
    pub transport: Rc<RefCell<Option<Transport>>>,
    pub error: Rc<RefCell<Option<String>>>,
}

impl NetState {
    pub fn get(&self) -> Option<Transport> {
        self.transport.borrow().clone()
    }
}

/// The launch config as a resource.
#[derive(Resource, Clone)]
pub struct LaunchConfigRes(pub LaunchConfig);

/// The game client state (the `ClientCore` is shared with bots through `bc-client-core`).
pub struct Game {
    pub core: ClientCore,
    pub hello_sent: bool,
    /// `?autopilot=1`: the Mobile Doll brain flies (and ZERO stays engaged).
    pub autopilot: bool,
    pub brain: DollBrain,
    pub respawn_request: Option<FrameId>,
    /// The suit lock assist has designated.
    pub lock: Option<u16>,
    pub disconnected: bool,
    /// The pilot's controls as of the last rendered frame; the timer repeats them until the next.
    pub controls: InputCmd,
}

/// Non-send resource: the game state, shared by the render systems and the network timer.
#[derive(Clone)]
pub struct GameClient(Rc<RefCell<Game>>);

impl GameClient {
    pub fn borrow(&self) -> Ref<'_, Game> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, Game> {
        self.0.borrow_mut()
    }
}

pub fn now_ms() -> f64 {
    now_s() * 1_000.0
}

pub fn now_s() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now()) / 1_000.0
}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(NetState::default()).add_systems(Startup, connect).add_systems(Update, report);
    }
}

/// Timer period of the network loop (ms). Commands go out when their tick is due, so this only
/// bounds how late they can be.
const NET_LOOP_MS: i32 = 8;

/// Starts the timer-driven network loop (game mode).
pub fn start_net_loop(net: NonSend<NetState>, game: NonSend<GameClient>) {
    let slot = net.transport.clone();
    let game = game.clone();
    let tick = Closure::<dyn FnMut()>::new(move || {
        let Some(t) = slot.borrow().clone() else { return };
        if let Ok(mut g) = game.0.try_borrow_mut() {
            pump(&mut g, &t, now_s());
        }
    });
    if let Some(w) = web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(
            tick.as_ref().unchecked_ref(),
            NET_LOOP_MS,
        );
    }
    // The loop lives as long as the page.
    tick.forget();
}

fn connect(net: NonSend<NetState>, cfg: Res<LaunchConfigRes>) {
    let slot = net.transport.clone();
    let err = net.error.clone();
    let url = cfg.0.wt_url.clone();
    let hash = cfg.0.cert_hash.clone();
    let pump = !cfg.0.echo;
    wasm_bindgen_futures::spawn_local(async move {
        match Transport::connect(&url, hash).await {
            Ok(t) => {
                if pump {
                    t.start_pump(now_s);
                }
                *slot.borrow_mut() = Some(t);
            }
            Err(e) => {
                web_sys::console::error_1(&e.clone().into());
                *err.borrow_mut() = Some(e);
            }
        }
    });
}

fn report(net: NonSend<NetState>, mut dev: ResMut<DevStatus>) {
    match net.get() {
        Some(t) => {
            dev.set("connected", !t.is_closed());
            dev.set("max_datagram", t.max_datagram_size() as u32);
        }
        None => {
            dev.set("connected", false);
            if let Some(e) = net.error.borrow().as_ref() {
                dev.set("error", e.as_str());
            }
        }
    }
}

pub fn game_client(cfg: &LaunchConfig) -> GameClient {
    let frame = crate::config::parse_frame(&cfg.frame).unwrap_or(FrameId::WingZero);
    GameClient(Rc::new(RefCell::new(Game {
        core: ClientCore::new(ClientConfig {
            name: cfg.name.clone(),
            pilot: PilotKind::Human,
            frame,
            faction: Faction::Colonies,
        }),
        hello_sent: false,
        autopilot: cfg.autopilot,
        brain: DollBrain::new(0x5EED),
        respawn_request: None,
        lock: None,
        disconnected: false,
        controls: InputCmd::default(),
    })))
}

/// One pass of the network loop: receives everything that arrived, then sends the commands that
/// are due.
fn pump(g: &mut Game, t: &Transport, now: f64) {
    if t.is_closed() {
        g.disconnected = true;
        return;
    }
    if !g.hello_sent {
        t.send_control(g.core.hello());
        g.hello_sent = true;
    }
    let ctrl = t.take_control();
    if !ctrl.is_empty() {
        g.core.on_control(&ctrl);
    }
    for (d, arrived) in t.drain() {
        g.core.on_datagram(&d, arrived);
    }
    if let Some(frame) = g.respawn_request.take() {
        t.send_control(g.core.respawn(frame));
    }
    let packets = if g.autopilot {
        let brain = &mut g.brain;
        // ZERO stays engaged; flight assist is the brain's call (it flies unassisted to spare
        // its pilot G-strain).
        g.core.poll_inputs(now, &mut |ctx| {
            let mut cmd = brain.decide(ctx);
            cmd.buttons |= ZERO;
            cmd
        })
    } else {
        let cmd = g.controls;
        g.core.poll_inputs(now, &mut |_| cmd)
    };
    for p in &packets {
        t.send_datagram(p);
    }
}

/// Per rendered frame: publishes the pilot's controls, runs the network loop once, and advances
/// the client's per-frame state.
pub fn drive(
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    controls: Res<Controls>,
    mut aim: ResMut<Aim>,
    time: Res<Time<Real>>,
) {
    let Some(t) = net.get() else { return };
    let now = now_s();
    let mut g = game.borrow_mut();
    // Lock assist, for frames with missiles to guide: the hostile the reticle is on.
    let launcher = g.core.world.own.is_some_and(|o| o.alive && frame(o.frame).lock_spec().is_some());
    g.lock = if launcher {
        let (from, t) = (g.core.predict.state.pos, g.core.render_tick(now));
        g.core.world.lock_assist(from, aim.dir, g.lock, t)
    } else {
        None
    };
    g.controls = controls.command(aim.dir, g.lock);
    pump(&mut g, &t, now);
    if g.autopilot && g.core.inputs.newest != 0 {
        aim.dir = g.core.last_cmd.aim;
    }
    g.core.frame(now, time.delta_secs());
}
