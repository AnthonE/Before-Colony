//! Before Colony client core: everything a client does except moving bytes.
//!
//! The browser client (Bevy) and the Bot SDK both drive a [`ClientCore`], so agents run exactly the
//! human code path: handshake, clock sync, redundant inputs, own-suit prediction with the shared
//! flight model, and interpolation of everyone else.
//!
//! Transport glue calls [`ClientCore::hello`] once, then feeds control-stream bytes to
//! [`ClientCore::on_control`] and datagrams to [`ClientCore::on_datagram`], and sends whatever
//! [`ClientCore::poll_inputs`] returns.

pub mod bay;
pub mod brains;
pub mod chase;
pub mod clock;
pub mod controls;
pub mod hangar;
pub mod hints;
pub mod inputs;
pub mod interp;
pub mod own;
pub mod pointer;
pub mod predict;
pub mod salvage;
pub mod session;
pub mod settings;
pub mod walker;
pub mod world;

use bc_proto::auth::{Address, Domain, NONCE_BYTES, Signature, TOKEN_BYTES};
use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::control::{ControlMsg, Frame, RejectReason, hello_flags, roster_flags, welcome_flags};
use bc_proto::snapshot::{own_flags, zero_mode};
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, PROTOCOL_VERSION, PilotKind, SnapshotReader};
use bc_sim::DT;
use bc_sim::config::{G0, MAX_REWIND_TICKS};
use bc_sim::content::{Mount, Replication, frame, weapon};
use bc_sim::math::{clamp_to_cone, integrate_rotation, normalize_or};
use glam::Vec3;

pub use brains::{DollBrain, MinerBrain};
pub use clock::Clock;
pub use hangar::HangarState;
pub use inputs::InputHistory;
pub use own::OwnView;
pub use predict::{OwnPose, Predictor};
pub use salvage::{LooseChunk, SalvageView};
pub use world::{Beam, FeedLine, Ghost, HitMark, World};

/// Who this client is.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub name: String,
    pub pilot: PilotKind,
    pub frame: FrameId,
    pub faction: Faction,
}

/// Who the pilot is to the server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Identity {
    /// No sign-in: the suit goes when the pilot does.
    #[default]
    Guest,
    /// A wallet: sign in (or, with a resume token, reconnect without signing again).
    Wallet { address: Address, resume: Option<[u8; TOKEN_BYTES]> },
}

/// What the server told us at the handshake.
#[derive(Clone, Copy, Debug)]
pub struct Welcome {
    pub client_slot: u16,
    pub tick_hz: u8,
    pub zero_allowed: bool,
    pub max_datagram: u16,
    /// The sector's debris field.
    pub field_seed: u32,
    pub field_rocks: u16,
    /// Signed in: the suit sleeps in the world when the pilot leaves.
    pub signed_in: bool,
    /// The pilot woke in the suit they'd left.
    pub woke: bool,
    /// Survival rules: the pilot starts in their hangar, and launches the suit they built.
    pub survival: bool,
    /// How many of the compiled landmarks the sector has (no more than this build knows of).
    pub landmarks: u8,
}

/// The server's sign-in challenge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: [u8; NONCE_BYTES],
    pub issued_at: u64,
    pub domain: Domain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Handshake,
    /// The server asked the wallet to sign ([`ClientCore::sign_in_text`], then [`ClientCore::auth`]).
    Signing(Challenge),
    InGame,
    Rejected(RejectReason),
    /// The server closed the session (its [`ControlMsg::Bye`] reason, or 0).
    Closed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientStats {
    pub snapshots: u64,
    pub bytes: u64,
    pub max_snapshot: usize,
    pub decode_errors: u64,
    pub packets_sent: u64,
    pub last_snapshot_at: f64,
    /// Prediction error at the most recent reconciled tick, m.
    pub prediction_error: f32,
}

/// Inputs available to whoever produces the command for a tick (keyboard, autopilot, bot brain).
pub struct InputContext<'a> {
    pub tick: u32,
    /// Time (ticks) of the world this client is showing.
    pub view_tick: f64,
    /// Time (ticks) the server will resolve this command's shots against: the view time, unless
    /// that is further back than lag compensation reaches (`MAX_REWIND_TICKS`). Brains should aim
    /// at targets as they are then.
    pub resolve_tick: f64,
    pub now: f64,
    pub world: &'a World,
    pub predict: &'a Predictor,
}

pub struct ClientCore {
    pub cfg: ClientConfig,
    pub identity: Identity,
    pub phase: Phase,
    /// The server's goodbye reason, if it said goodbye.
    pub bye_reason: Option<u8>,
    /// Reconnect with this rather than signing again (signed-in pilots).
    pub resume_token: Option<[u8; TOKEN_BYTES]>,
    /// What the server wanted the pilot to know: `bc_proto::control::notice` codes and their
    /// detail. Taken by the UI.
    pub notices: Vec<(u8, String)>,
    /// Survival: the pilot's hangar, and where they are.
    pub hangar: HangarState,
    ctrl_buf: Vec<u8>,
    pub welcome: Option<Welcome>,
    pub clock: Clock,
    pub inputs: InputHistory,
    pub predict: Predictor,
    pub world: World,
    next_cmd_tick: u32,
    pub shot_seq: u8,
    last_shot_tick: u32,
    pub stats: ClientStats,
    pub last_cmd: InputCmd,
    /// The own suit as drawn, between frames.
    drawn: own::Drawn,
    /// Felt acceleration between the last two snapshots, g: the reading while ZERO flies the suit
    /// (and the prediction, flying the pilot's commands, is beside the point).
    heard_g: f32,
}

impl ClientCore {
    pub fn new(cfg: ClientConfig) -> Self {
        let faction = cfg.faction;
        Self {
            cfg,
            identity: Identity::Guest,
            phase: Phase::Handshake,
            bye_reason: None,
            resume_token: None,
            notices: Vec::new(),
            hangar: HangarState::default(),
            ctrl_buf: Vec::new(),
            welcome: None,
            clock: Clock::default(),
            inputs: InputHistory::default(),
            predict: Predictor::default(),
            world: World::new(faction),
            next_cmd_tick: 0,
            shot_seq: 0,
            last_shot_tick: 0,
            stats: ClientStats::default(),
            last_cmd: InputCmd::default(),
            drawn: own::Drawn::default(),
            heard_g: 0.0,
        }
    }

    /// Signs in as `identity` (before [`ClientCore::hello`]).
    pub fn with_identity(mut self, identity: Identity) -> Self {
        self.identity = identity;
        self
    }

    /// The first control-stream frame.
    pub fn hello(&self) -> Vec<u8> {
        let mut msg = ControlMsg::hello(self.cfg.pilot, self.cfg.frame, self.cfg.faction, &self.cfg.name);
        if let ControlMsg::Hello { flags, resume, .. } = &mut msg {
            match self.identity {
                Identity::Guest => {}
                Identity::Wallet { resume: Some(token), .. } => {
                    *flags = hello_flags::RESUME;
                    *resume = token;
                }
                Identity::Wallet { resume: None, .. } => *flags = hello_flags::SIGN_IN,
            }
        }
        encode(msg)
    }

    /// The text the wallet must sign, while [`Phase::Signing`].
    pub fn sign_in_text(&self) -> Option<String> {
        let (Phase::Signing(c), Identity::Wallet { address, .. }) = (self.phase, self.identity) else {
            return None;
        };
        let mut out = [0u8; bc_auth::SIWE_MAX];
        let n = bc_auth::siwe_message(c.domain.as_str(), &address, &c.nonce, c.issued_at, &mut out);
        core::str::from_utf8(&out[..n]).ok().map(str::to_string)
    }

    /// Answers the challenge with the wallet's signature; the Welcome comes next.
    pub fn auth(&mut self, signature: Signature) -> Vec<u8> {
        let Identity::Wallet { address, .. } = self.identity else { return Vec::new() };
        self.phase = Phase::Handshake;
        encode(ControlMsg::Auth { address, signature })
    }

    /// Says goodbye (the pilot is leaving on purpose).
    pub fn bye(&self, reason: u8) -> Vec<u8> {
        encode(ControlMsg::Bye { reason })
    }

    /// Asks to respawn in `frame` (after death).
    pub fn respawn(&self, frame: FrameId) -> Vec<u8> {
        encode(ControlMsg::Respawn { frame })
    }

    /// Asks the hangar for something (survival rules): the frame to send.
    pub fn request(&self, req: &bc_econ::Request) -> Vec<u8> {
        hangar::frame(req)
    }

    /// The pilot left the sector (docked, or lost): what was flown is forgotten, and the next
    /// sortie syncs its clock afresh. The field, the roster and the tallies stay.
    fn left_the_sector(&mut self) {
        let old = std::mem::replace(&mut self.world, World::new(self.cfg.faction));
        self.world.roster = old.roster;
        self.world.roster_flags = old.roster_flags;
        self.world.my_hits = old.my_hits;
        self.world.my_kills = old.my_kills;
        self.world.my_deaths = old.my_deaths;
        self.world.hits_taken = old.hits_taken;
        self.world.kit = old.kit;
        let field = self.predict.field.clone();
        self.predict = Predictor::default();
        self.predict.field = field;
        self.clock = Clock::default();
        self.inputs = InputHistory::default();
        self.next_cmd_tick = 0;
        self.last_cmd = InputCmd::default();
        self.drawn = own::Drawn::default();
        self.heard_g = 0.0;
    }

    /// Feeds bytes read from the control stream.
    pub fn on_control(&mut self, bytes: &[u8]) {
        self.ctrl_buf.extend_from_slice(bytes);
        loop {
            match Frame::decode(&self.ctrl_buf) {
                Ok(Some((frame, used))) => {
                    match frame {
                        Frame::Msg(msg) => self.handle_control(msg),
                        Frame::Hangar(p) => {
                            if let Some(update) = bc_econ::wire::decode(p)
                                && self.hangar.apply(update)
                            {
                                self.left_the_sector();
                            }
                        }
                    }
                    self.ctrl_buf.drain(..used);
                }
                Ok(None) => break,
                Err(_) => {
                    self.phase = Phase::Closed;
                    self.ctrl_buf.clear();
                    break;
                }
            }
        }
    }

    fn handle_control(&mut self, msg: ControlMsg) {
        match msg {
            ControlMsg::Welcome {
                version,
                client_slot,
                tick_hz,
                zero_allowed,
                max_datagram,
                field_seed,
                field_rocks,
                flags,
                landmarks,
                ..
            } => {
                if version != PROTOCOL_VERSION {
                    self.phase = Phase::Rejected(RejectReason::VersionMismatch);
                    return;
                }
                self.predict.set_landmarks(landmarks);
                self.welcome = Some(Welcome {
                    client_slot,
                    tick_hz,
                    zero_allowed,
                    max_datagram,
                    field_seed,
                    field_rocks,
                    signed_in: flags & welcome_flags::SIGNED_IN != 0,
                    woke: flags & welcome_flags::WOKE != 0,
                    survival: flags & welcome_flags::SURVIVAL != 0,
                    landmarks: self.predict.landmarks().len() as u8,
                });
                self.predict.set_field(bc_sim::field::Field::generate(field_seed, field_rocks));
                self.phase = Phase::InGame;
            }
            ControlMsg::Reject { reason } => self.phase = Phase::Rejected(reason),
            ControlMsg::Roster { slot, pilot, name, flags } => {
                if name.is_empty() {
                    self.world.roster.remove(&slot);
                    self.world.roster_flags.remove(&slot);
                } else {
                    self.world.roster.insert(slot, (name.as_str().to_string(), pilot));
                    self.world.roster_flags.insert(slot, flags);
                }
            }
            ControlMsg::Bye { reason } => {
                self.bye_reason = Some(reason);
                self.phase = Phase::Closed;
            }
            ControlMsg::Challenge { nonce, issued_at, domain } => {
                if self.phase == Phase::Handshake && matches!(self.identity, Identity::Wallet { .. }) {
                    self.phase = Phase::Signing(Challenge { nonce, issued_at, domain });
                }
            }
            ControlMsg::Token { token } => {
                self.resume_token = Some(token);
                if let Identity::Wallet { resume, .. } = &mut self.identity {
                    *resume = Some(token);
                }
            }
            ControlMsg::Notice { code, name } => self.notices.push((code, name.as_str().to_string())),
            ControlMsg::Hello { .. } | ControlMsg::Respawn { .. } | ControlMsg::Auth { .. } => {}
        }
    }

    /// Feeds a datagram received at local time `now` (seconds).
    pub fn on_datagram(&mut self, bytes: &[u8], now: f64) {
        let Ok(mut r) = SnapshotReader::new(bytes) else {
            self.stats.decode_errors += 1;
            return;
        };
        let h = *r.header();
        if h.tick <= self.world.tick && self.stats.snapshots > 0 {
            return; // duplicate or reordered: everything in it is repeated in newer snapshots
        }
        let (Ok(own), Ok(zero)) = (r.own(), r.zero()) else {
            self.stats.decode_errors += 1;
            return;
        };
        let mut events = Vec::new();
        while let Ok(Some(e)) = r.next_event() {
            events.push(e);
        }
        let mut rocks = Vec::new();
        while let Ok(Some(rock)) = r.next_rock() {
            rocks.push(rock);
        }
        let mut missiles = Vec::new();
        loop {
            match r.next_missile() {
                Ok(Some(m)) => missiles.push(m),
                Ok(None) => break,
                Err(_) => {
                    self.stats.decode_errors += 1;
                    break;
                }
            }
        }
        let mut ents = Vec::new();
        loop {
            match r.next_entity() {
                Ok(Some(e)) => ents.push(e),
                Ok(None) => break,
                Err(_) => {
                    self.stats.decode_errors += 1;
                    break;
                }
            }
        }
        let mut objects = Vec::new();
        loop {
            match r.next_object() {
                Ok(Some(o)) => objects.push(o),
                Ok(None) => break,
                Err(_) => {
                    self.stats.decode_errors += 1;
                    break;
                }
            }
        }
        // The own suit where it was last drawn, from what it was drawn from, before this news.
        let drawn_at = self.drawn.recent(now);
        let before = drawn_at.and_then(|t| self.own_source(t)).map(|(p, _)| p);
        let life_before = self.world.own.map(|o| (o.slot, o.generation, o.alive));
        // A hold of 255 ms is saturated (the client sent nothing for that long), so the true hold
        // is unknown and the sample would overstate the RTT.
        let rtt = (h.time_echo_ms != 0 && h.echo_hold_ms < u8::MAX).then(|| {
            let now_ms = (now * 1_000.0) as u64 as u16;
            f64::from(now_ms.wrapping_sub(h.time_echo_ms)) / 1_000.0 - f64::from(h.echo_hold_ms) / 1_000.0
        });
        self.clock.on_snapshot(h.tick, now, rtt, h.input_health);
        let heard = self.world.own.filter(|o| o.alive).map(|o| (self.world.tick, o.vel));
        self.world.apply_missiles(h.tick, &missiles);
        self.world.apply(h.tick, own, zero, &events, &ents, &self.predict.bodies(h.tick));
        // (The world has the own suit in the sector's frame, whatever frame it came in.)
        if let (Some(now), Some((t0, v0))) = (self.world.own.filter(|o| o.alive), heard) {
            self.heard_g = (now.vel - v0).length() / ((h.tick - t0) as f32 * DT) / G0;
        }
        self.world.apply_salvage(&rocks, &objects);
        for r in &rocks {
            self.predict.set_rock_dead(usize::from(r.id), r.destroyed);
        }
        if let Some(own) = own {
            // A new life (a respawn, or the first): nothing before it is to be drawn from.
            let new_life = own.alive
                && life_before.is_none_or(|(slot, generation, alive)| {
                    !alive || slot != own.slot || generation != own.generation
                });
            if new_life {
                self.predict.forget_before(h.tick);
            }
            self.predict.reconcile(h.tick, &own, &self.inputs);
            self.stats.prediction_error = self.predict.last_error;
            // Keep the suit where it was drawn, and blend what the news changed out.
            let after = drawn_at.and_then(|t| self.own_source(t)).map(|(p, _)| p);
            if new_life || drawn_at.is_none() {
                self.drawn.cut();
            } else {
                self.drawn.correct(before, after);
            }
        }
        self.stats.snapshots += 1;
        self.stats.bytes += bytes.len() as u64;
        self.stats.max_snapshot = self.stats.max_snapshot.max(bytes.len());
        self.stats.last_snapshot_at = now;
    }

    /// Generates commands for every tick that is due at `now` and returns the datagrams to send.
    /// `brain` fills in the controls (tick, view and shot sequence are managed here).
    pub fn poll_inputs(
        &mut self,
        now: f64,
        brain: &mut dyn FnMut(&InputContext) -> InputCmd,
    ) -> Vec<Vec<u8>> {
        if self.phase != Phase::InGame || !self.clock.synced() || self.world.own.is_none() {
            return Vec::new();
        }
        let target = self.clock.input_tick(now);
        if self.next_cmd_tick == 0 || target > self.next_cmd_tick + 30 {
            // First command, or we stalled (background tab): resume a few ticks back.
            self.next_cmd_tick = target.saturating_sub(3).max(self.inputs.newest + 1);
        }
        if target < self.next_cmd_tick {
            return Vec::new();
        }
        let first = self.next_cmd_tick;
        while self.next_cmd_tick <= target {
            let tick = self.next_cmd_tick;
            let view = self.clock.view_tick(now).min(f64::from(tick));
            let resolve = view.max(f64::from(tick.saturating_sub(MAX_REWIND_TICKS)));
            let ctx = InputContext {
                tick,
                view_tick: view,
                resolve_tick: resolve,
                now,
                world: &self.world,
                predict: &self.predict,
            };
            let mut cmd = brain(&ctx);
            cmd.tick = tick;
            cmd.view_tick_q4 = ((view.max(0.0) * 16.0) as u32).min(tick << 4);
            let shot = self.shot_due(&cmd, tick);
            cmd.shot_seq = self.shot_seq;
            let q = cmd.quantized();
            self.inputs.push(q);
            self.predict.advance(&q, &self.inputs);
            if let Some(mount) = shot {
                self.predict_shot(mount, &q);
            }
            self.last_cmd = q;
            self.next_cmd_tick += 1;
        }
        // Each packet carries the newest command and the three before it. A burst (several ticks
        // at once) is sent as overlapping windows, so every command still travels twice and a
        // single lost packet costs nothing.
        let last = self.next_cmd_tick - 1;
        let step = bc_proto::input::MAX_CMDS as u32 / 2;
        let mut packets = Vec::new();
        let mut newest = last;
        loop {
            let mut p = InputPacket {
                ack_snapshot: self.world.tick,
                client_time_ms: (now * 1_000.0) as u64 as u16,
                ..InputPacket::default()
            };
            let mut n = 0;
            while n < bc_proto::input::MAX_CMDS {
                match newest.checked_sub(n as u32).and_then(|t| self.inputs.get(t)) {
                    Some(c) => p.cmds[n] = c,
                    None => break,
                }
                n += 1;
            }
            p.count = n as u8;
            if n > 0 {
                let mut buf = [0u8; 128];
                if let Some(len) = p.encode(&mut buf) {
                    packets.push(buf[..len].to_vec());
                }
            }
            if newest < first + step {
                break;
            }
            newest -= step;
        }
        self.stats.packets_sent += packets.len() as u64;
        packets
    }

    /// Whether this command fires the primary weapon, which should be ready: if so, the shot takes
    /// the next `shot_seq` and is drawn now (the server's spawn event confirms it). Only weapons
    /// whose every shot is an event are drawn this way: a stream's tracers, a blade or a flame
    /// have nothing to confirm.
    fn shot_due(&mut self, cmd: &InputCmd, tick: u32) -> Option<Mount> {
        let own = self.world.own?;
        if !own.alive || cmd.buttons & FIRE_PRIMARY == 0 || own.weapon_ready & 1 == 0 {
            return None;
        }
        let mount = frame(own.frame).loadout[0]?;
        let w = weapon(mount.weapon);
        if w.replication != Replication::PerShot
            || w.charge_ticks > 0
            || tick < self.last_shot_tick + u32::from(w.cooldown)
        {
            return None;
        }
        self.last_shot_tick = tick;
        self.shot_seq = self.shot_seq.wrapping_add(1);
        Some(mount)
    }

    /// Draws the shot `cmd` fires from `mount`, as the server fires it: from the suit after that
    /// tick's flight, within the arm's reach off the nose.
    fn predict_shot(&mut self, mount: Mount, cmd: &InputCmd) {
        let Some(own) = self.world.own else { return };
        let w = weapon(mount.weapon);
        let s = &self.predict.state;
        let fwd = s.rot * Vec3::Z;
        let dir = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, mount.arm.cone());
        let muzzle = s.pos + s.rot * mount.arm.muzzle();
        self.world.predict_beam(
            own.slot,
            w.kind,
            self.shot_seq,
            muzzle,
            s.vel + dir * w.speed,
            f64::from(cmd.tick),
        );
    }

    /// What the own suit is drawn from at `t` (input-clock ticks), and whether it's alive: the
    /// prediction, between the ticks it has flown; or, for the wreck and while ZERO flies the suit
    /// (not the pilot's commands), the server's word carried on at its velocity and spin.
    fn own_source(&self, t: f64) -> Option<(OwnPose, bool)> {
        let own = self.world.own?;
        let seized = own.zero_mode == zero_mode::SEIZED;
        if own.alive
            && !seized
            && self.predict.initialized
            && let Some(p) = self.predict.pose_at(t)
        {
            return Some((p, true));
        }
        let ahead = (t - f64::from(self.world.tick)).clamp(0.0, 15.0) as f32 * DT;
        let pose = OwnPose {
            pos: own.pos + own.vel * ahead,
            // (A wreck drifts without turning.)
            rot: if own.alive { integrate_rotation(own.rot, own.ang_vel, ahead) } else { own.rot },
            vel: own.vel,
            dpos: own.vel,
            g_load: if own.alive { self.heard_g } else { 0.0 },
            g_strain: own.g_strain,
            blackout: own.flags & own_flags::BLACKOUT != 0,
            boosting: own.alive && own.flags & own_flags::BOOSTING != 0,
            throttle: Vec3::ZERO,
            g_limited: false,
            frame: own.frame,
            strike: (own.alive && matches!(own.arms.phase, 1 | 2)).then_some(own.arms.slot),
        };
        Some((pose, own.alive))
    }

    /// The own suit as drawn this frame (after [`ClientCore::frame`]).
    pub fn own_view(&self) -> Option<&OwnView> {
        self.drawn.view.as_ref()
    }

    /// Time (ticks) to render remote entities at.
    pub fn render_tick(&self, now: f64) -> f64 {
        self.clock.view_tick(now)
    }

    /// Per-frame housekeeping: draws the own suit for this frame ([`ClientCore::own_view`]) and
    /// prunes what's done.
    pub fn frame(&mut self, now: f64, frame_dt: f32) {
        self.world.prune(self.render_tick(now));
        // A tick behind the input clock, between the last two ticks predicted.
        let own_t = self.clock.own_tick(now) - 1.0;
        match self.own_source(own_t) {
            Some((src, alive)) => {
                let rate = self.clock.own_rate(now) as f32;
                self.drawn.draw(own_t, now, &src, alive, frame_dt, rate);
            }
            None => self.drawn.view = None,
        }
    }
}

/// One control frame as bytes.
fn encode(msg: ControlMsg) -> Vec<u8> {
    let mut buf = [0u8; bc_proto::control::MAX_FRAME];
    let n = msg.encode(&mut buf).unwrap_or(0);
    buf[..n].to_vec()
}

/// Roster flags as the world keeps them.
pub fn asleep(world: &World, slot: u16) -> bool {
    world.roster_flags.get(&slot).is_some_and(|f| f & roster_flags::ASLEEP != 0)
}
