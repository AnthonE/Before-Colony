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
pub mod body_mesh;
pub mod brains;
pub mod chart;
pub mod chase;
pub mod city;
pub mod city_atlas;
pub mod city_mesh;
pub mod city_nav;
pub mod clock;
pub mod controls;
pub mod course;
pub mod doubletap;
pub mod figure;
pub mod gait;
pub mod hangar;
pub mod hints;
pub mod inputs;
pub mod interp;
pub mod life;
pub mod life_mesh;
pub mod lockon;
pub mod nav;
pub mod objectives;
pub mod own;
pub mod palette;
pub mod plaza;
pub mod pointer;
pub mod predict;
pub mod salvage;
pub mod session;
pub mod settings;
pub mod showcase_city;
pub mod sights;
pub mod sphere;
pub mod surface;
pub mod tram;
pub mod vehicle;
pub mod walker;
pub mod world;

use bc_proto::auth::{Address, Domain, NONCE_BYTES, Signature, TOKEN_BYTES};
use bc_proto::buttons::FIRE_PRIMARY;
use bc_proto::control::{ControlMsg, Frame, RejectReason, hello_flags, roster_flags, welcome_flags};
use bc_proto::events::Event;
use bc_proto::presence::{PersonPose, PlazaReader, PosePacket};
use bc_proto::snapshot::{header_flags, own_flags, zero_mode};
use bc_proto::{
    Faction, FrameId, InputCmd, InputPacket, PROTOCOL_VERSION, PacketKind, PilotKind, SnapshotReader,
    WeaponKind, packet_kind,
};
use bc_sim::DT;
use bc_sim::bodies::Body;
use bc_sim::config::{G0, MAX_REWIND_TICKS};
use bc_sim::content::{Mount, Replication, frame, weapon};
use bc_sim::math::{clamp_to_cone, integrate_rotation, normalize_or};
use bc_sim::tuning::FlightRules;
use glam::Vec3;

pub use brains::{DollBrain, LanderBrain, MinerBrain};
pub use clock::Clock;
pub use hangar::HangarState;
pub use inputs::InputHistory;
pub use own::OwnView;
pub use predict::{OwnPose, Predictor};
pub use salvage::{LooseChunk, SalvageView};
pub use surface::BodySet;
pub use world::{Beam, FeedLine, Ghost, HitMark, World};

/// None of the pilot's own snapshots for this long, s, and the plaza's datagrams keep the clock
/// (on foot).
const SNAPSHOTS_GONE: f64 = 0.5;

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
    /// Anime flight rules: the tank is a boost gauge that fills back up.
    pub anime: bool,
    /// The colony is open: the bay's airlock leads to the cap lifts and down into its city.
    pub colony: bool,
    /// How many of the compiled landmarks the sector has (no more than this build knows of).
    pub landmarks: u8,
    /// The sector is the colony's inside (the pilot flew in through the inner gate).
    pub interior: bool,
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
    /// When the last of the pilot's own snapshots came (not a spectator's), local s.
    pub last_own_snapshot_at: Option<f64>,
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
    /// The cooldown of the shot drawn at `last_shot_tick`, ticks.
    last_shot_wait: u16,
    pub stats: ClientStats,
    pub last_cmd: InputCmd,
    /// The own suit as drawn, between frames.
    drawn: own::Drawn,
    /// Felt acceleration between the last two snapshots, g: the reading while ZERO flies the suit
    /// (and the prediction, flying the pilot's commands, is beside the point).
    heard_g: f32,
    /// Survival, on foot in the colony: the people near the pilot, and the pilot's own pose to
    /// send (with its sequence number and when it last went).
    pub plaza: plaza::PlazaView,
    pose: Option<PersonPose>,
    pose_seq: u16,
    pose_sent: f64,
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
            last_shot_wait: 0,
            stats: ClientStats::default(),
            last_cmd: InputCmd::default(),
            drawn: own::Drawn::default(),
            heard_g: 0.0,
            plaza: plaza::PlazaView::default(),
            pose: None,
            pose_seq: 0,
            pose_sent: f64::NEG_INFINITY,
        }
    }

    /// On foot in the colony: where the pilot is now (`None`: not in the city, so nothing to send).
    pub fn set_pose(&mut self, pose: Option<PersonPose>) {
        self.pose = pose;
    }

    /// On foot in the colony: where the pilot last said they are.
    pub fn pose(&self) -> Option<PersonPose> {
        self.pose
    }

    /// The pose datagram that's due at local time `now` (s), if one is (15 a second).
    pub fn poll_pose(&mut self, now: f64) -> Option<Vec<u8>> {
        let pose = self.pose?;
        if now - self.pose_sent < plaza::POSE_EVERY {
            return None;
        }
        self.pose_sent = now;
        self.pose_seq = self.pose_seq.wrapping_add(1);
        let mut buf = [0u8; 32];
        let n = PosePacket { seq: self.pose_seq, pose }.encode(&mut buf)?;
        Some(buf[..n].to_vec())
    }

    /// When the colony's people and trams are drawn at local time `now`: the sector's tick and
    /// the fraction of the next.
    pub fn colony_tick(&self, now: f64) -> (u32, f32) {
        let t = (self.clock.server_now(now) - plaza::DELAY_TICKS).max(0.0);
        (t.floor() as u32, (t - t.floor()) as f32)
    }

    /// Flying a suit inside the colony (its people, trams and cars are round it).
    pub fn inside(&self) -> bool {
        self.hangar.place == Some(bc_econ::wire::Place::Space) && self.welcome.is_some_and(|w| w.interior)
    }

    /// The people near the pilot in the city (or round their suit inside the colony) as they're
    /// drawn at local time `now`: their slot, name and pose, in city coordinates (a rider where
    /// their train is then; still marked as riding it).
    pub fn people(&self, now: f64) -> Vec<(u16, &str, PersonPose)> {
        let t = self.clock.server_now(now) - plaza::DELAY_TICKS;
        let (tick, frac) = self.colony_tick(now);
        self.plaza
            .people_at(t)
            .into_iter()
            .map(|(id, mut p)| {
                if let Some(k) = p.riding() {
                    let tr = bc_sim::colony::transit::train(p.strip, k, tick, frac);
                    p.x += tr.x;
                    p.s += tr.s - bc_proto::presence::RIDER_S;
                    p.h += bc_sim::colony::transit::FLOOR;
                }
                (id, self.hangar.people.get(&id).map_or("", String::as_str), p)
            })
            .collect()
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

    /// The pilot left the sector (docked, or lost), or the city (where they watched the colony's
    /// inside): what was flown or watched is forgotten, and the next sortie syncs its clock afresh.
    /// The field, the roster and the tallies stay.
    fn left_the_sector(&mut self) {
        let old = std::mem::replace(&mut self.world, World::new(self.cfg.faction));
        self.world.roster = old.roster;
        self.world.roster_flags = old.roster_flags;
        self.world.my_hits = old.my_hits;
        self.world.my_kills = old.my_kills;
        self.world.my_deaths = old.my_deaths;
        self.world.hits_taken = old.hits_taken;
        self.world.kit = old.kit;
        self.world.bodies = old.bodies;
        let (field, landmarks) = (self.predict.field.clone(), self.predict.landmarks().len() as u8);
        let (rules, interior) = (self.predict.rules(), self.predict.interior());
        self.predict = Predictor::default();
        self.predict.field = field;
        self.predict.set_landmarks(landmarks);
        // How the sector flies stays (the Welcome said).
        self.predict.set_rules(rules);
        self.predict.set_interior(interior);
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
                // Welcomed again mid-session: into another sector (the colony's inside, or back
                // out of it). What was flown in the last one is forgotten.
                if self.welcome.is_some() {
                    self.left_the_sector();
                }
                self.predict.set_interior(flags & welcome_flags::INTERIOR != 0);
                self.predict.set_landmarks(landmarks);
                let anime = flags & welcome_flags::ANIME != 0;
                self.predict.set_rules(if anime { FlightRules::Anime } else { FlightRules::Real });
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
                    anime,
                    colony: flags & welcome_flags::COLONY != 0,
                    interior: flags & welcome_flags::INTERIOR != 0,
                    landmarks: self.predict.landmarks().len() as u8,
                });
                self.predict.set_field(bc_sim::field::Field::generate(field_seed, field_rocks));
                // The same rocks, shared until a shattering parts them: the view's go by the rock
                // records, the prediction's by the tick it replays.
                self.world.bodies = BodySet::new(self.predict.field.clone(), landmarks)
                    .inside(flags & welcome_flags::INTERIOR != 0);
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
        if packet_kind(bytes) == Some(PacketKind::Plaza) {
            match PlazaReader::new(bytes) {
                Ok(r) => {
                    // The sector's tick keeps the clock on foot. While the pilot's own snapshots
                    // come (a suit inside the colony, whose sector keeps the same tick), they keep
                    // it: they carry the round trip and the input buffer's health. A spectator's
                    // carry neither, and leave it to the plaza.
                    if self.stats.last_own_snapshot_at.is_none_or(|t| now - t > SNAPSHOTS_GONE) {
                        self.clock.on_snapshot(r.tick, now, None, clock::TARGET_HEALTH as i8);
                    }
                    self.plaza.on_datagram(r, now);
                }
                Err(_) => self.stats.decode_errors += 1,
            }
            return;
        }
        let Ok(mut r) = SnapshotReader::new(bytes) else {
            self.stats.decode_errors += 1;
            return;
        };
        let h = *r.header();
        if h.tick <= self.world.tick && self.stats.snapshots > 0 {
            return; // duplicate or reordered: everything in it is repeated in newer snapshots
        }
        // A spectator's (on foot in the city, the suits inside the colony near the pilot): taken
        // only there. With no input of the pilot's to answer for, it leaves the clock to the plaza.
        let spectator = h.flags & header_flags::SPECTATOR != 0;
        if spectator && !self.hangar.in_city() {
            return;
        }
        // What's watched is the colony's inside, where the suits standing on its city ride it.
        if spectator && !self.world.bodies.interior() {
            self.world.bodies = std::mem::take(&mut self.world.bodies).inside(true);
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
        let before = drawn_at.and_then(|(o, v)| self.own_source(o, v)).map(|(p, _)| p);
        let life_before = self.world.own.map(|o| (o.slot, o.generation, o.alive));
        if !spectator {
            // A hold of 255 ms is saturated (the client sent nothing for that long), so the true
            // hold is unknown and the sample would overstate the RTT.
            let rtt = (h.time_echo_ms != 0 && h.echo_hold_ms < u8::MAX).then(|| {
                let now_ms = (now * 1_000.0) as u64 as u16;
                f64::from(now_ms.wrapping_sub(h.time_echo_ms)) / 1_000.0 - f64::from(h.echo_hold_ms) / 1_000.0
            });
            self.clock.on_snapshot(h.tick, now, rtt, h.input_health);
            self.stats.last_own_snapshot_at = Some(now);
        }
        let heard = self.world.own.filter(|o| o.alive).map(|o| (self.world.tick, o.vel));
        self.world.apply_missiles(h.tick, &missiles);
        self.world.apply(h.tick, own, zero, &events, &ents);
        // (The world has the own suit in the sector's frame, whatever frame it came in.)
        if let (Some(now), Some((t0, v0))) = (self.world.own.filter(|o| o.alive), heard) {
            self.heard_g = (now.vel - v0).length() / ((h.tick - t0) as f32 * DT) / G0;
        }
        self.world.apply_salvage(&rocks, &objects);
        // A rock is gone from the tick after the server broke it: its riders let go then.
        for e in &events {
            if let Event::RockBreak { tick, rock, .. } = *e {
                self.predict.note_rock_break(rock, tick + 1);
            }
        }
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
            // A gun in hand: what the secondary's trigger fires.
            self.predict.set_in_hand(self.world.gun_in_hand());
            self.predict.reconcile(h.tick, &own, &self.inputs);
            self.stats.prediction_error = self.predict.last_error;
            // Keep the suit where it was drawn, and blend what the news changed out.
            let after = drawn_at.and_then(|(o, v)| self.own_source(o, v)).map(|(p, _)| p);
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
            if let Some((mount, kind)) = shot {
                self.predict_shot(mount, kind, &q);
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

    /// Whether this command fires the primary weapon, which should be ready: if so, the shot (the
    /// weapon's own, or its charged shot) takes the next `shot_seq` and is drawn now (the server's
    /// spawn event confirms it). Only weapons whose every shot is an event are drawn this way: a
    /// stream's tracers, a blade or a flame have nothing to confirm.
    fn shot_due(&mut self, cmd: &InputCmd, tick: u32) -> Option<(Mount, WeaponKind)> {
        let own = self.world.own?;
        if !own.alive || own.weapon_ready & 1 == 0 {
            return None;
        }
        let mount = frame(own.frame).loadout[0]?;
        let w = weapon(mount.weapon);
        let held = cmd.buttons & FIRE_PRIMARY != 0;
        // Tap fires, hold charges (`bc_sim::arms::charged_pull`, with the hold as predicted up to
        // this command): the tap's shot early in a pull, the charged shot on letting go full.
        let kind = match w.charged {
            Some(c) if held && self.predict.arms.charge < u16::from(c.tap) => w.kind,
            Some(c) if !held && self.predict.arms.charge >= c.full() => c.shot,
            None if held => w.kind,
            _ => return None,
        };
        if w.replication != Replication::PerShot
            || w.charge_ticks > 0
            || tick < self.last_shot_tick + u32::from(self.last_shot_wait)
        {
            return None;
        }
        self.last_shot_tick = tick;
        self.last_shot_wait = weapon(kind).cooldown;
        self.shot_seq = self.shot_seq.wrapping_add(1);
        Some((mount, kind))
    }

    /// Draws the shot `cmd` fires from `mount`, as the server fires it: from the suit after that
    /// tick's flight (on a moving body, where the pilot saw it on the body), within the arm's
    /// reach off the nose.
    fn predict_shot(&mut self, mount: Mount, kind: WeaponKind, cmd: &InputCmd) {
        let Some(own) = self.world.own else { return };
        let w = weapon(kind);
        let s = &self.predict.as_seen(cmd);
        let fwd = s.rot * Vec3::Z;
        // Within what its actuators leave of the arm's reach, wandering if the pilot's concussed
        // (the server counts the concussion down after each tick's shots).
        let tuned = bc_sim::tuning::own_tuning(&own);
        let mut dir = clamp_to_cone(normalize_or(cmd.aim, fwd), fwd, bc_sim::tuning::cone(mount.arm, &tuned));
        // Within the weapon's cone, as the server scatters it (the primary is mount 0).
        dir = bc_sim::tuning::scatter(dir, w.spread, cmd.tick, own.slot, 0);
        let since = cmd.tick.saturating_sub(self.world.tick).saturating_sub(1);
        if u32::from(own.concussed) > since {
            dir = bc_sim::tuning::wobble(dir, cmd.tick, own.slot, 0);
        }
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
    /// (not the pilot's commands), the server's word carried on at its velocity and spin. On a
    /// body, its place on the body then, on the body as drawn at `t_view` (view-clock ticks).
    fn own_source(&self, t: f64, t_view: f64) -> Option<(OwnPose, bool)> {
        let own = self.world.own?;
        let seized = own.zero_mode == zero_mode::SEIZED;
        if own.alive
            && !seized
            && self.predict.initialized
            && let Some(p) = self.predict.pose_at_on(t, t_view)
        {
            return Some((p, true));
        }
        let ahead = (t - f64::from(self.world.tick)).clamp(0.0, 15.0) as f32 * DT;
        let mut pose = OwnPose {
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
            ion: false,
            frame: own.frame,
            strike: (own.alive && matches!(own.arms.phase, 1 | 2)).then_some(own.arms.slot),
            ground: None,
            touchdown: None,
        };
        // On a body: carried on over it, in its frame.
        if let Some(sent) = self.world.own_sent().filter(|_| own.alive)
            && let Some(on) = sent.surface
            && let body = Body::from(on.body)
            && let (Some(p), Some(shape)) =
                (self.world.bodies.pose_at(body, t_view), self.world.bodies.shape(body))
        {
            let local = sent.pos + sent.vel * ahead;
            let probe = shape.probe(local);
            pose.pos = p.to_world(local);
            pose.rot = p.rot * integrate_rotation(sent.rot, sent.ang_vel, ahead);
            pose.vel = p.point_vel(pose.pos) + p.rot * sent.vel;
            pose.dpos = pose.vel;
            pose.ground = Some(interp::GroundPose {
                body,
                aloft: on.footing == bc_proto::snapshot::footing::ALOFT,
                up: p.rot * probe.normal,
                rel_vel: p.rot * sent.vel,
                height: probe.dist,
            });
        }
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
        let view_t = self.render_tick(now);
        self.world.prune(view_t);
        // A tick behind the input clock, between the last two ticks predicted; on a body, on the
        // body as it's drawn (with everyone else, at the view clock's time).
        let own_t = self.clock.own_tick(now) - 1.0;
        let at = own::Moment { own: own_t, view: view_t, now };
        match self.own_source(own_t, view_t) {
            Some((src, alive)) => {
                let rate = self.clock.own_rate(now) as f32;
                let bodies = &self.world.bodies;
                self.drawn.reframe(&src, at, &|b, t| bodies.pose_at(b, t));
                let deck = src.on().and_then(|b| bodies.pose_at(b, view_t)).map(|p| p.rot);
                self.drawn.draw(at, &src, alive, frame_dt, rate, deck);
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

/// How weathered a suit's paint is, 0 (factory fresh, or nobody's on the roster for it) to 7.
pub fn weathering(world: &World, slot: u16) -> u8 {
    world.roster_flags.get(&slot).map_or(0, |f| roster_flags::weathering_of(*f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::{RockState, SnapshotHeader, SnapshotWriter};
    use bc_sim::field::Field;
    use surface::{CAM_CLEAR, camera_clamp};

    #[test]
    fn the_camera_passes_through_a_rock_once_it_has_shattered() {
        let (seed, rocks) = (0xDEB12, 160);
        let mut core = ClientCore::new(ClientConfig {
            name: "test".into(),
            pilot: PilotKind::Human,
            frame: FrameId::Leo,
            faction: Faction::Colonies,
        });
        core.on_control(&encode(ControlMsg::Welcome {
            version: PROTOCOL_VERSION,
            client_slot: 0,
            tick: 0,
            tick_hz: bc_sim::config::TICK_HZ as u8,
            sector: 0,
            zero_allowed: false,
            max_datagram: 1_100,
            field_seed: seed,
            field_rocks: rocks,
            flags: 0,
            landmarks: 0,
        }));
        assert_eq!(core.phase, Phase::InGame);
        // A line through a rock, and through nothing else.
        let mut field = Field::generate(seed, rocks);
        let (i, from, to) = (0..field.len())
            .find_map(|i| {
                let rock = field.rocks()[i];
                let reach = rock.radius + CAM_CLEAR + 20.0;
                let (from, to) = (rock.pos + Vec3::Y * reach, rock.pos - Vec3::Y * reach);
                field.set_dead(i, true);
                let clear = field.sweep(from, to, CAM_CLEAR).is_none();
                field.set_dead(i, false);
                clear.then_some((i, from, to))
            })
            .expect("a rock on its own");
        let view = &core.world.bodies;
        assert!(camera_clamp(view, 10.0, from, to).distance(to) > 1.0, "stopped short of the rock");
        // The server breaks it on tick 40, and says so (and that it's gone) in the snapshot of 41.
        let mut buf = [0u8; 1_100];
        let mut w = SnapshotWriter::new(&mut buf, 1_100);
        w.header(&SnapshotHeader { tick: 41, ..SnapshotHeader::default() });
        w.own(None);
        w.zero(None);
        assert!(w.event(&Event::RockBreak { id: 1, tick: 40, rock: i as u16, by: 3 }, 0));
        assert!(w.rock(&RockState::new(i as u16, true, 0.0, 0.0), 0));
        let n = w.finish().unwrap();
        core.on_datagram(&buf[..n], 1.0);
        assert_eq!(core.stats.decode_errors, 0);
        // The view sees through it now, whatever the prediction makes of it.
        assert!(core.world.bodies.field.is_dead(i));
        assert_eq!(camera_clamp(&core.world.bodies, 40.5, from, to), to);
        // Grown back, it is in the way again.
        let mut w = SnapshotWriter::new(&mut buf, 1_100);
        w.header(&SnapshotHeader { tick: 90, ..SnapshotHeader::default() });
        w.own(None);
        w.zero(None);
        assert!(w.rock(&RockState::new(i as u16, false, 1.0, 1.0), 0));
        let n = w.finish().unwrap();
        core.on_datagram(&buf[..n], 2.0);
        assert!(camera_clamp(&core.world.bodies, 90.0, from, to).distance(to) > 1.0);
    }
}
