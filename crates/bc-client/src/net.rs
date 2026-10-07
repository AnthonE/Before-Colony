//! Connection management and the game client: bytes from the browser's WebTransport go into the
//! shared `ClientCore`; commands come back out as datagrams.
//!
//! The network loop ([`pump`]) runs on a browser timer as well as once per rendered frame, so
//! inputs, the autopilot and clock sync keep their 30 Hz cadence even when rendering is slow
//! (software GPUs, hitches). Both run on the page's single thread, so they never overlap.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use bc_client_core::lockon::{self, Lock};
use bc_client_core::nav::AutoNav;
use bc_client_core::{ClientConfig, ClientCore, DollBrain, Identity, LanderBrain};
use bc_proto::buttons::{BOOST, BRAKE, BURST, FLIGHT_ASSIST, GRIP, ZERO};
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use bc_sim::content::frame;
use bevy::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::config::{Autopilot, LaunchConfig};
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

/// The brain flying for the pilot, under `?autopilot=`.
pub enum Brain {
    Doll(DollBrain),
    Lander(LanderBrain),
}

impl Brain {
    fn new(pilot: Option<Autopilot>) -> Self {
        match pilot {
            Some(Autopilot::Lander(plan)) => Brain::Lander(LanderBrain::new(plan)),
            _ => Brain::Doll(DollBrain::new(0x5EED)),
        }
    }

    /// What the HUD calls it.
    pub fn name(&self) -> &'static str {
        match self {
            Brain::Doll(_) => "Mobile Doll brain",
            Brain::Lander(_) => "lander",
        }
    }
}

/// The game client state (the `ClientCore` is shared with bots through `bc-client-core`).
pub struct Game {
    pub core: ClientCore,
    pub hello_sent: bool,
    /// `?autopilot=`: a brain flies (the Mobile Doll's, with ZERO engaged, or a lander).
    pub autopilot: bool,
    pub pilot: Option<Autopilot>,
    pub brain: Brain,
    pub respawn_request: Option<FrameId>,
    /// The pilot asked to leave their suit: eject (`false`), or blow up their doomed suit (`true`)
    /// (`input::eject_key`).
    pub eject_request: Option<bool>,
    /// The suit designated: the lock-on's target, else lock assist's (frames with missiles).
    pub lock: Option<u16>,
    /// The pilot's lock-on (Y, or the middle button): `bc_client_core::lockon`.
    pub hard: Lock,
    /// The pilot's controls as of the last rendered frame; the timer repeats them until the next.
    pub controls: InputCmd,
    /// The suit (slot, generation) those controls were set up for (`input::read_input` seeds them
    /// from its first own state: the grip, a crouch).
    pub controls_for: Option<(u16, u8)>,
    /// The auto-nav, while it's flying the pilot's course (`chart.rs`): it holds the stick every
    /// tick, and the pilot's buttons (but boost, brake and the grip) still count.
    pub nav: Option<AutoNav>,
    /// A dev hook's errand inside the colony (`fly_to`): flight assist flies the suit to this point
    /// of the colony's frame (or, `fly_out`, of space's: the dock), and lets go once it's there.
    pub fly_to: Option<Vec3>,
    pub fly_out: bool,
    /// Launching from the bay (`onfoot`): while the suit rides its bay's catapult cradle, hold on
    /// (flight assist and the grip, hands off) until the catapult fires, whoever flies it.
    pub bay_hold: bool,
}

impl Game {
    fn new(name: &str, frame: FrameId, pilot: Option<Autopilot>, identity: Identity) -> Self {
        Self {
            core: ClientCore::new(ClientConfig {
                name: name.to_string(),
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
            })
            .with_identity(identity),
            hello_sent: false,
            autopilot: pilot.is_some(),
            pilot,
            brain: Brain::new(pilot),
            respawn_request: None,
            eject_request: None,
            lock: None,
            hard: Lock::default(),
            controls: InputCmd::default(),
            controls_for: None,
            nav: None,
            fly_to: None,
            fly_out: false,
            bay_hold: false,
        }
    }
}

/// Non-send resource: the game state, shared by the render systems and the network timer.
#[derive(Clone)]
pub struct GameClient(Rc<RefCell<Game>>);

impl GameClient {
    /// Starts over for a new session (a dial, or the link going down): an empty world.
    pub fn reset(&self, name: &str, frame: FrameId, identity: Identity) {
        let pilot = self.0.borrow().pilot;
        *self.0.borrow_mut() = Game::new(name, frame, pilot, identity);
    }

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

/// The transport slot and its status hooks. `dial_at_startup`: connect once at startup (the echo
/// spike); game mode dials through `session` instead.
pub struct NetPlugin {
    pub dial_at_startup: bool,
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(NetState::default()).add_systems(Update, report);
        if self.dial_at_startup {
            app.add_systems(Startup, connect);
        }
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
    GameClient(Rc::new(RefCell::new(Game::new(&cfg.name, frame, cfg.autopilot, Identity::Guest))))
}

/// One pass of the network loop: receives everything that arrived, then sends the commands that
/// are due.
fn pump(g: &mut Game, t: &Transport, now: f64) {
    if t.is_closed() {
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
    if let Some(destruct) = g.eject_request.take() {
        t.send_control(g.core.request(&bc_econ::Request::Eject { destruct }));
    }
    // In the bay's cradle until the catapult fires: held there, whatever would fly it.
    let hold = g.bay_hold;
    let held = move |ctx: &bc_client_core::InputContext| {
        let own = ctx.world.own.filter(|_| hold && ctx.predict.bay().is_some())?;
        Some(InputCmd { aim: own.rot * Vec3::Z, buttons: FLIGHT_ASSIST | GRIP, ..InputCmd::default() })
    };
    let packets = if g.autopilot {
        let brain = &mut g.brain;
        g.core.poll_inputs(now, &mut |ctx| {
            if let Some(cmd) = held(ctx) {
                return cmd;
            }
            match brain {
                // ZERO stays engaged; flight assist is the brain's call (it flies unassisted to
                // spare its pilot G-strain).
                Brain::Doll(b) => {
                    let mut cmd = b.decide(ctx);
                    cmd.buttons |= ZERO;
                    cmd
                }
                Brain::Lander(b) => b.decide(ctx),
            }
        })
    } else {
        let (cmd, seeded, fly_to) = (g.controls, g.controls_for, g.fly_to);
        let Game { core, nav, hard, .. } = &mut *g;
        core.poll_inputs(now, &mut |ctx| {
            if let Some(cmd) = held(ctx) {
                return cmd;
            }
            match ctx.world.own {
                // News of a suit the controls aren't set up for yet came in between frames (one woken
                // on a body, say): hold on as the server does until the next frame sets them, rather
                // than let go of the body.
                Some(o) if seeded != Some((o.slot, o.generation)) => InputCmd {
                    aim: o.rot * Vec3::Z,
                    buttons: FLIGHT_ASSIST | if o.surface.is_some() { GRIP } else { 0 },
                    ..InputCmd::default()
                },
                // A dev hook's errand (`fly_to`): flight assist toward the point, slowing as it nears.
                Some(o) if fly_to.is_some() => {
                    fly_toward(ctx, frame(o.frame).fa_speed, fly_to.unwrap_or(ctx.predict.state.pos))
                }
                // The auto-nav holds the stick: a velocity for flight assist to fly, worked out from
                // the prediction tick by tick, in the suit's own axes (so no lock-on rides with it, and
                // no burst step). Else, locked on, the keys move the suit about its target.
                _ => match nav.as_mut().and_then(|n| Some((n.decide(ctx)?, n.look))) {
                    Some((n, look)) => InputCmd {
                        thrust: n.thrust,
                        aim: if look { n.aim } else { cmd.aim },
                        buttons: (cmd.buttons & !(BOOST | BRAKE | GRIP | BURST)) | n.buttons,
                        lockon: None,
                        ..cmd
                    },
                    None => lockon::shape(cmd, hard, ctx),
                },
            }
        })
    };
    for p in &packets {
        t.send_datagram(p);
    }
    // (On foot in the colony, where the pilot stands is sent by `onfoot`, the frame it's taken.)
}

/// The command flying a suit (on flight assist) toward `to` in its sector's frame: a velocity there,
/// eased as it nears and no faster than 120 m/s, in the suit's own axes (`fa_speed` full stick).
fn fly_toward(ctx: &bc_client_core::InputContext, fa_speed: f32, to: Vec3) -> InputCmd {
    let s = &ctx.predict.state;
    let d = to - s.pos;
    let want = d.normalize_or_zero() * (d.length() * 0.3).min(120.0);
    let stick = s.rot.conjugate() * want / fa_speed.max(1.0);
    let q = |x: f32| (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
    InputCmd {
        thrust: [q(stick.x), q(stick.y), q(stick.z)],
        aim: d.normalize_or(s.rot * Vec3::Z),
        buttons: FLIGHT_ASSIST,
        ..InputCmd::default()
    }
}

/// Where a `fly_to` errand goes, in the colony's frame: `up` metres over a place's door (by its
/// slug), `ahead` metres out from it; or the inner gate (`inner_gate`), `ahead` metres down the
/// colony from it. Out in space (the sector's frame): the dock off the hub's mouth (`dock`), `up`
/// metres over its middle.
fn fly_target(slug: &str, up: f32, ahead: f32) -> Option<Vec3> {
    if slug == "inner_gate" {
        return Some(bc_sim::colony::interior::INNER_GATE + Vec3::X * ahead);
    }
    if slug == "dock" {
        return Some(bc_sim::content::salvage::DOCK_CENTER + Vec3::Y * up);
    }
    let (_, p) = bc_sim::colony::city::place(slug)?;
    let ((s, x), (ds, dx)) = bc_sim::colony::city::place_door(p);
    Some(bc_sim::colony::frame::CityPos::new(p.strip, x - dx * ahead, s - ds * ahead, up).to_colony())
}

/// Per rendered frame: publishes the pilot's controls, runs the network loop once, and advances
/// the client's per-frame state.
pub fn drive(
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    controls: Res<Controls>,
    cmds: Res<crate::page::UiCmds>,
    mut aim: ResMut<Aim>,
    time: Res<Time<Real>>,
    mut dev: ResMut<DevStatus>,
) {
    let Some(t) = net.get() else { return };
    let now = now_s();
    let mut g = game.borrow_mut();
    for cmd in &cmds.0 {
        if let crate::page::UiCmd::FlyTo(to) = cmd {
            g.fly_to = to.as_ref().and_then(|(slug, up, ahead)| fly_target(slug, *up, *ahead));
            g.fly_out = to.as_ref().is_some_and(|(slug, ..)| slug == "dock");
        }
    }
    // The errand's done once the suit is there (or gone, or out of the colony, or into it).
    let here = g.core.own_view().filter(|v| v.alive).map(|v| v.pos);
    let astray = g.core.inside() == g.fly_out;
    if g.fly_to.is_some_and(|to| astray || here.is_none_or(|p| p.distance(to) < 8.0)) {
        g.fly_to = None;
    }
    dev.set("flying_to", g.fly_to.is_some());
    // The lock-on's target; else lock assist, for frames with missiles to guide: the hostile the
    // reticle is on.
    let launcher = g.core.world.own.is_some_and(|o| o.alive && frame(o.frame).lock_spec().is_some());
    g.lock = if let Some(slot) = g.hard.slot() {
        Some(slot)
    } else if launcher {
        let from = g.core.own_view().map_or(g.core.predict.state.pos, |v| v.pos);
        let t = g.core.render_tick(now);
        g.core.world.lock_assist(from, aim.dir, g.lock, t)
    } else {
        None
    };
    g.controls = controls.command(aim.dir, g.lock);
    g.controls_for = aim.initialized_for;
    pump(&mut g, &t, now);
    // The view follows what the autopilot (or the auto-nav, while it turns the suit) aims at.
    let steered = g.nav.as_ref().is_some_and(|n| n.look);
    if (g.autopilot || steered) && g.core.inputs.newest != 0 {
        aim.dir = g.core.last_cmd.aim;
    }
    g.core.frame(now, time.delta_secs());
}
