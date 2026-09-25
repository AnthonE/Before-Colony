//! Before Colony client core: everything a client does except moving bytes.
//!
//! The browser client (Bevy) and the Bot SDK both drive a [`ClientCore`], so agents run exactly the
//! human code path: handshake, clock sync, redundant inputs, own-suit prediction with the shared
//! flight model, and interpolation of everyone else.
//!
//! Transport glue calls [`ClientCore::hello`] once, then feeds control-stream bytes to
//! [`ClientCore::on_control`] and datagrams to [`ClientCore::on_datagram`], and sends whatever
//! [`ClientCore::poll_inputs`] returns.

pub mod brains;
pub mod clock;
pub mod inputs;
pub mod interp;
pub mod predict;
pub mod salvage;
pub mod world;

use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::control::{ControlMsg, RejectReason};
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, PROTOCOL_VERSION, PilotKind, SnapshotReader};
use bc_sim::config::MAX_REWIND_TICKS;
use bc_sim::content::{Replication, frame, weapon};
use glam::Vec3;

pub use brains::{DollBrain, MinerBrain};
pub use clock::Clock;
pub use inputs::InputHistory;
pub use predict::Predictor;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Handshake,
    InGame,
    Rejected(RejectReason),
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
    pub phase: Phase,
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
}

impl ClientCore {
    pub fn new(cfg: ClientConfig) -> Self {
        let faction = cfg.faction;
        Self {
            cfg,
            phase: Phase::Handshake,
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
        }
    }

    /// The first control-stream frame.
    pub fn hello(&self) -> Vec<u8> {
        let mut buf = [0u8; bc_proto::control::MAX_FRAME];
        let n = ControlMsg::hello(self.cfg.pilot, self.cfg.frame, self.cfg.faction, &self.cfg.name)
            .encode(&mut buf)
            .unwrap_or(0);
        buf[..n].to_vec()
    }

    /// Asks to respawn in `frame` (after death).
    pub fn respawn(&self, frame: FrameId) -> Vec<u8> {
        let mut buf = [0u8; bc_proto::control::MAX_FRAME];
        let n = ControlMsg::Respawn { frame }.encode(&mut buf).unwrap_or(0);
        buf[..n].to_vec()
    }

    /// Feeds bytes read from the control stream.
    pub fn on_control(&mut self, bytes: &[u8]) {
        self.ctrl_buf.extend_from_slice(bytes);
        loop {
            match ControlMsg::decode(&self.ctrl_buf) {
                Ok(Some((msg, used))) => {
                    self.ctrl_buf.drain(..used);
                    self.handle_control(msg);
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
                ..
            } => {
                if version != PROTOCOL_VERSION {
                    self.phase = Phase::Rejected(RejectReason::VersionMismatch);
                    return;
                }
                self.welcome = Some(Welcome {
                    client_slot,
                    tick_hz,
                    zero_allowed,
                    max_datagram,
                    field_seed,
                    field_rocks,
                });
                self.predict.set_field(bc_sim::field::Field::generate(field_seed, field_rocks));
                self.phase = Phase::InGame;
            }
            ControlMsg::Reject { reason } => self.phase = Phase::Rejected(reason),
            ControlMsg::Roster { slot, pilot, name } => {
                if name.is_empty() {
                    self.world.roster.remove(&slot);
                } else {
                    self.world.roster.insert(slot, (name.as_str().to_string(), pilot));
                }
            }
            ControlMsg::Bye { .. } => self.phase = Phase::Closed,
            ControlMsg::Hello { .. } | ControlMsg::Respawn { .. } => {}
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
        // A hold of 255 ms is saturated (the client sent nothing for that long), so the true hold
        // is unknown and the sample would overstate the RTT.
        let rtt = (h.time_echo_ms != 0 && h.echo_hold_ms < u8::MAX).then(|| {
            let now_ms = (now * 1_000.0) as u64 as u16;
            f64::from(now_ms.wrapping_sub(h.time_echo_ms)) / 1_000.0 - f64::from(h.echo_hold_ms) / 1_000.0
        });
        self.clock.on_snapshot(h.tick, now, rtt, h.input_health);
        self.world.apply_missiles(h.tick, &missiles);
        self.world.apply(h.tick, own, zero, &events, &ents);
        self.world.apply_salvage(&rocks, &objects);
        for r in &rocks {
            self.predict.set_rock_dead(usize::from(r.id), r.destroyed);
        }
        if let Some(own) = own {
            self.predict.reconcile(h.tick, &own, &self.inputs);
            self.stats.prediction_error = self.predict.last_error;
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
            self.maybe_predict_shot(&mut cmd, tick);
            cmd.shot_seq = self.shot_seq;
            let q = cmd.quantized();
            self.inputs.push(q);
            self.predict.advance(&q);
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

    /// If this command fires the primary weapon and the weapon should be ready, draw the beam now
    /// (the server's spawn event confirms it via `shot_seq`). Only weapons whose every shot is an
    /// event are drawn this way: a stream's tracers, a blade or a flame have nothing to confirm.
    fn maybe_predict_shot(&mut self, cmd: &mut InputCmd, tick: u32) {
        let Some(own) = self.world.own else { return };
        if !own.alive || cmd.buttons & FIRE_PRIMARY == 0 || own.weapon_ready & 1 == 0 {
            return;
        }
        let spec = frame(own.frame);
        let Some(mount) = spec.loadout[0] else { return };
        let w = weapon(mount.weapon);
        if w.replication != Replication::PerShot
            || w.charge_ticks > 0
            || tick < self.last_shot_tick + u32::from(w.cooldown)
        {
            return;
        }
        self.last_shot_tick = tick;
        self.shot_seq = self.shot_seq.wrapping_add(1);
        let s = &self.predict.state;
        let muzzle = s.pos + s.rot * mount.arm.muzzle();
        let dir = cmd.aim.normalize_or(s.rot * Vec3::Z);
        self.world.predict_beam(
            own.slot,
            w.kind,
            self.shot_seq,
            muzzle,
            s.vel + dir * w.speed,
            f64::from(tick),
        );
    }

    /// Time (ticks) to render remote entities at.
    pub fn render_tick(&self, now: f64) -> f64 {
        self.clock.view_tick(now)
    }

    /// Per-frame housekeeping (visual correction decay, pruning).
    pub fn frame(&mut self, now: f64, frame_dt: f32) {
        self.predict.decay(frame_dt);
        let t = self.render_tick(now);
        self.world.prune(t);
    }
}
