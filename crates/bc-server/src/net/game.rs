//! Game mode: WebTransport sessions ↔ the sector thread, the egress thread, and the pilot roster.
//!
//! Everything here is network-side: it may allocate and may use tokio primitives. It reaches the
//! sector only through `bc-sector`'s lock-free queues.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use bc_proto::auth::{Address, Domain, NONCE_BYTES};
use bc_proto::control::{ControlMsg, Frame, MAX_FRAME, RejectReason, hello_flags, roster_flags};
use bc_proto::{MAX_DATAGRAM, PROTOCOL_VERSION, PilotKind};
use bc_sector::{EgressEnds, Metrics, SectorConfig, SectorShared, SectorThread, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::sim::Gone;
use crossbeam_queue::ArrayQueue;
use tokio::sync::broadcast;
use wtransport::{Connection, RecvStream, SendStream};

use super::NetStats;
use crate::market::Market;
use crate::pilots::{self, Claim, Fate, FileStore, MemoryStore, PilotStore, Pilots};
use crate::{Config, OracleKind, Ruleset};

/// Commands for the egress thread.
pub enum EgressCmd {
    Attach(u16, Connection),
    Detach(u16),
}

#[derive(Clone, Debug)]
pub struct RosterEntry {
    pub name: String,
    pub pilot: PilotKind,
    pub client_slot: u16,
    /// `roster_flags` (signed in, asleep).
    pub flags: u8,
    /// A signed-in pilot's wallet, shortened (`0x1234…abcd`).
    pub address: Option<String>,
}

/// Change to the roster, fanned out to every session.
#[derive(Clone, Debug)]
pub struct RosterUpdate {
    /// Entity slot of the pilot's suit.
    pub suit: u16,
    pub pilot: PilotKind,
    /// Empty = the pilot left.
    pub name: String,
    pub flags: u8,
}

/// Wallet sign-in settings.
#[derive(Clone)]
pub struct SignIn {
    /// What wallets sign in to (the pages' host).
    pub domain: Arc<str>,
    pub required: bool,
    pub wait: Duration,
    /// At most this many sign-ins wait on wallets at once.
    pub waiting: Arc<tokio::sync::Semaphore>,
}

/// A pilot in their hangar bay (survival rules), as `/status` lists them.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HangarEntry {
    pub name: String,
    pub address: Option<String>,
    /// `hangar` or `space`.
    pub place: &'static str,
    pub credits: u64,
    /// `empty`, `docked` or `out`, and the suit's line.
    pub bay: &'static str,
    pub line: Option<&'static str>,
    /// Parts fitted, and jobs queued at both stations.
    pub parts: usize,
    pub jobs: usize,
    /// What the stores hold: kilograms of bulk goods, pieces of everything else.
    pub stores_kg: u64,
    pub stores_pieces: u64,
}

/// What each session task needs to reach the sector.
#[derive(Clone)]
pub struct GameShared {
    pub sector: Arc<SectorShared>,
    pub(super) egress: Arc<ArrayQueue<EgressCmd>>,
    pub(super) egress_thread: thread::Thread,
    pub(super) roster: Arc<RwLock<HashMap<u16, RosterEntry>>>,
    pub(super) roster_tx: broadcast::Sender<RosterUpdate>,
    pub pilots: Arc<Pilots>,
    sign_in: SignIn,
    /// A session with no input for this long ends (in the sector: a pilot in their hangar sends
    /// nothing while they walk about).
    pub(super) idle: Duration,
    /// Survival rules (else arcade).
    pub survival: bool,
    /// The Colony Exchange.
    pub market: Arc<Market>,
    pub econ: bc_econ::Rules,
    /// Pilots' hangars, by client slot, for `/status`.
    pub(super) hangars: Arc<RwLock<HashMap<u16, HangarEntry>>>,
}

/// Read-only view for `/status`.
#[derive(Clone)]
pub struct StatusView {
    sector: Arc<SectorShared>,
    roster: Arc<RwLock<HashMap<u16, RosterEntry>>>,
    oracle: &'static str,
    survival: bool,
    market: Arc<Market>,
    hangars: Arc<RwLock<HashMap<u16, HangarEntry>>>,
}

/// Owns the sector and egress threads.
pub struct GameRuntime {
    sector: Option<SectorThread>,
    egress: Option<thread::JoinHandle<()>>,
    egress_stop: Arc<AtomicBool>,
    shared: GameShared,
    oracle: &'static str,
    _oracle_worker: Option<bc_zero::WorkerHandle>,
}

impl GameRuntime {
    pub fn start(cfg: &Config, stats: Arc<NetStats>, domain: String) -> anyhow::Result<Self> {
        let jev_key = cfg.jev_key.clone();
        let use_jev = cfg.oracle == OracleKind::Jev && jev_key.is_some();
        if cfg.oracle == OracleKind::Jev && !use_jev {
            tracing::warn!(
                "--oracle jev requested but TYPESAFE_API_KEY is not set: using the local oracle only"
            );
        }
        let survival = cfg.rules == Ruleset::Survival;
        let sector_cfg = SectorConfig {
            sim: SimConfig {
                target_dolls: cfg.mobile_dolls,
                seed: cfg.seed,
                max_sleepers: cfg.max_sleepers,
                survival,
                ..SimConfig::default()
            },
            max_clients: cfg.max_clients,
            oracle: use_jev,
            hot_guard: Some(bc_alloc::set_hot),
        };
        let (sector, shared, egress_ends, oracle_ends) = bc_sector::build(sector_cfg);
        let queue = Arc::new(ArrayQueue::new(1_024));
        let stop = Arc::new(AtomicBool::new(false));
        let egress = spawn_egress(egress_ends, queue.clone(), stats, stop.clone())?;
        let egress_thread = egress.thread().clone();
        let sector_thread = bc_sector::spawn(sector, Some(egress_thread.clone()))?;
        let oracle_worker = match jev_key.filter(|_| use_jev) {
            Some(key) => {
                let jev = match &cfg.jev_url {
                    Some(url) => bc_zero::JevOracle::with_endpoint(
                        key,
                        url.clone(),
                        bc_zero::jev::DEFAULT_MODEL.into(),
                    ),
                    None => bc_zero::JevOracle::new(key),
                };
                Some(bc_zero::spawn_worker(oracle_ends.pictures, oracle_ends.advice, jev)?)
            }
            None => None,
        };
        let (roster_tx, _) = broadcast::channel(256);
        let oracle = if use_jev { "jev+local" } else { "local" };
        tracing::info!(
            dolls = cfg.mobile_dolls,
            max_clients = cfg.max_clients,
            oracle,
            rules = ?cfg.rules,
            "sector running at 30 Hz"
        );
        let store: Arc<dyn PilotStore> = match &cfg.data_dir {
            Some(dir) => Arc::new(FileStore::new(dir.join("pilots"))?),
            None => Arc::new(MemoryStore::default()),
        };
        let market = Arc::new(Market::open(cfg.data_dir.as_ref().map(|d| d.join("exchange.json"))));
        let game = GameShared {
            sector: shared,
            egress: queue,
            egress_thread,
            roster: Arc::new(RwLock::new(HashMap::new())),
            roster_tx,
            pilots: Arc::new(Pilots::new(store, cfg.resume_ttl)),
            sign_in: SignIn {
                domain: domain.into(),
                required: cfg.require_auth,
                wait: cfg.sign_wait,
                waiting: Arc::new(tokio::sync::Semaphore::new(128)),
            },
            idle: cfg.idle_timeout,
            survival,
            market,
            econ: bc_econ::Rules { craft_speed: cfg.craft_speed },
            hangars: Arc::new(RwLock::new(HashMap::new())),
        };
        // The exchange's clock, and its file.
        if survival {
            let market = game.market.clone();
            let sector = game.sector.clone();
            tokio::spawn(async move {
                let mut every = tokio::time::interval(Duration::from_secs(1));
                let mut n = 0u64;
                while !sector.stop.load(Ordering::Acquire) {
                    every.tick().await;
                    market.tick(1.0);
                    n += 1;
                    if n.is_multiple_of(60) {
                        market.save().await;
                    }
                }
                market.save().await;
            });
        }
        // What became of sleepers: news for their pilots, a few times a second.
        let notes = game.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_millis(250));
            while !notes.sector.stop.load(Ordering::Acquire) {
                every.tick().await;
                process_notes(&notes);
            }
        });
        Ok(Self {
            sector: Some(sector_thread),
            egress: Some(egress),
            egress_stop: stop,
            shared: game,
            oracle,
            _oracle_worker: oracle_worker,
        })
    }

    pub fn shared(&self) -> GameShared {
        self.shared.clone()
    }

    pub fn status_view(&self) -> StatusView {
        StatusView {
            sector: self.shared.sector.clone(),
            roster: self.shared.roster.clone(),
            oracle: self.oracle,
            survival: self.shared.survival,
            market: self.shared.market.clone(),
            hangars: self.shared.hangars.clone(),
        }
    }

    pub fn stop(mut self) {
        if let Some(s) = self.sector.take() {
            s.stop();
        }
        // The exchange's last word goes to its file.
        let market = self.shared.market.clone();
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move { market.save().await });
        }
        self.egress_stop.store(true, Ordering::Release);
        if let Some(e) = self.egress.take() {
            e.thread().unpark();
            let _ = e.join();
        }
    }
}

/// The egress thread: parked until the sector unparks it after each tick, then drains every slot's
/// packet ring into its connection.
fn spawn_egress(
    mut ends: EgressEnds,
    queue: Arc<ArrayQueue<EgressCmd>>,
    stats: Arc<NetStats>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("egress-0".into()).spawn(move || {
        let mut conns: Vec<Option<Connection>> = (0..ends.rings.len()).map(|_| None).collect();
        let mut buf = [0u8; MAX_DATAGRAM + 64];
        while !stop.load(Ordering::Acquire) {
            thread::park_timeout(Duration::from_millis(50));
            while let Some(cmd) = queue.pop() {
                match cmd {
                    EgressCmd::Attach(slot, conn) => {
                        let s = slot as usize;
                        // Anything queued before this session attached belongs to nobody.
                        while read_packet(&mut ends.rings[s], &mut buf).is_some() {}
                        conns[s] = Some(conn);
                    }
                    EgressCmd::Detach(slot) => conns[slot as usize] = None,
                }
            }
            for (s, ring) in ends.rings.iter_mut().enumerate() {
                while let Some(n) = read_packet(ring, &mut buf) {
                    if let Some(conn) = &conns[s]
                        && conn.send_datagram(&buf[..n]).is_ok()
                    {
                        NetStats::add(&stats.datagrams_out, 1);
                        NetStats::add(&stats.bytes_out, n as u64);
                    }
                }
            }
        }
    })
}

impl StatusView {
    pub fn json(&self) -> serde_json::Value {
        let s = &self.sector;
        let m = &s.metrics;
        let l = Metrics::load;
        let roster = self.roster.read().map(|r| r.clone()).unwrap_or_default();
        let mut pilots = Vec::new();
        for (slot, p) in m.pilots.iter().enumerate() {
            let suit = l(&p.suit);
            if suit == 0 {
                continue;
            }
            let suit = (suit - 1) as u16;
            let entry = roster.get(&suit);
            // Leaving signed in: listed with the sleepers from the moment the pilot goes.
            if entry.is_some_and(|e| e.flags & roster_flags::ASLEEP != 0) {
                continue;
            }
            pilots.push(serde_json::json!({
                "client_slot": slot,
                "suit": suit,
                "name": entry.map(|e| e.name.clone()).unwrap_or_default(),
                "verified": entry.is_some_and(|e| e.flags & roster_flags::VERIFIED != 0),
                "address": entry.and_then(|e| e.address.clone()),
                "credits": l(&p.credits),
                "pilot": entry.map(|e| format!("{:?}", e.pilot)).unwrap_or_default(),
                "shots": l(&p.shots),
                "hits": l(&p.hits),
                "kills": l(&p.kills),
                "deaths": l(&p.deaths),
                "frame": bc_proto::FrameId::ALL.get(l(&p.frame) as usize).map_or("", |f| f.slug()),
                "hits_by_class": {
                    "beam": l(&p.hits_by_class[0]),
                    "ballistic": l(&p.hits_by_class[1]),
                    "missile": l(&p.hits_by_class[2]),
                    "melee": l(&p.hits_by_class[3]),
                    "cone": l(&p.hits_by_class[4]),
                },
                "specials": l(&p.specials),
                "missiles": l(&p.missiles),
            }));
        }
        let sleepers: Vec<serde_json::Value> = roster
            .iter()
            .filter(|(_, e)| e.flags & roster_flags::ASLEEP != 0)
            .map(|(suit, e)| serde_json::json!({ "suit": suit, "name": e.name, "address": e.address }))
            .collect();
        let hangars: Vec<HangarEntry> =
            self.hangars.read().map(|h| h.values().cloned().collect()).unwrap_or_default();
        let exchange = self.market.with(|ex| {
            let (escrow, goods) = ex.escrowed();
            serde_json::json!({
                "fees": ex.fees,
                "colony_paid": ex.colony_paid,
                "colony_took": ex.colony_took,
                "escrow_credits": escrow,
                "escrow_items": goods.len(),
                "traded_items": ex.quotes().iter().filter(|q| q.volume > 0).count(),
            })
        });
        serde_json::json!({
            "tick": s.tick.load(Ordering::Acquire),
            "tick_hz": bc_sim::TICK_HZ,
            "oracle": self.oracle,
            "rules": if self.survival { "survival" } else { "arcade" },
            // Survival: pilots in their hangar bays (and out of them), and the exchange's ledger.
            "hangars": hangars,
            "exchange": exchange,
            "clients": l(&m.clients),
            "suits_alive": l(&m.suits_alive),
            "sleepers_parked": l(&m.parked),
            "notes_dropped": l(&m.notes_dropped),
            "projectiles": l(&m.projectiles),
            "events": l(&m.events),
            "tick_us": {
                "p50": m.tick_quantile_us(0.5),
                "p99": m.tick_quantile_us(0.99),
                "max": l(&m.tick_max_us),
                "last": l(&m.tick_last_us),
            },
            "overruns": l(&m.overruns),
            "snapshots": l(&m.snapshots),
            "snapshot_bytes": l(&m.snapshot_bytes),
            "snapshot_max_bytes": l(&m.snapshot_max_bytes),
            "out_drops": l(&m.out_drops),
            "inputs": l(&m.inputs),
            "inputs_stale": l(&m.inputs_stale),
            "inputs_missing": l(&m.inputs_missing),
            "pictures": l(&m.pictures),
            "pictures_dropped": l(&m.pictures_dropped),
            "advice": l(&m.advice),
            // Heap operations observed on the sector thread inside ticks (must stay 0).
            "hot_path_allocations": bc_alloc::violations(),
            "pilots": pilots,
            // Signed-in pilots who left: their suits, asleep in the cockpit.
            "sleepers": sleepers,
        })
    }
}

pub(super) async fn send_control(tx: &mut SendStream, msg: ControlMsg) -> anyhow::Result<()> {
    let mut buf = [0u8; MAX_FRAME];
    let n = msg.encode(&mut buf).ok_or_else(|| anyhow::anyhow!("control frame too large"))?;
    tx.write_all(&buf[..n]).await?;
    Ok(())
}

async fn read_control(rx: &mut RecvStream, pending: &mut Vec<u8>) -> anyhow::Result<ControlMsg> {
    let mut buf = [0u8; 256];
    loop {
        match Frame::decode(pending).map_err(|e| anyhow::anyhow!("bad control frame: {e}"))? {
            Some((Frame::Msg(msg), used)) => {
                pending.drain(..used);
                return Ok(msg);
            }
            // A hangar message before the handshake is done means nothing yet.
            Some((Frame::Hangar(_), used)) => {
                pending.drain(..used);
                continue;
            }
            None => {}
        }
        match rx.read(&mut buf).await? {
            Some(n) => pending.extend_from_slice(&buf[..n]),
            None => anyhow::bail!("control stream closed"),
        }
    }
}

pub(super) async fn wait_slot(
    sector: &SectorShared,
    slot: u16,
    until: impl Fn(SlotState, u32) -> bool,
) -> Option<SlotState> {
    for _ in 0..1_500 {
        let st = &sector.slots[slot as usize];
        let state = st.state();
        if until(state, st.epoch()) {
            return Some(state);
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    None
}

/// Simple token bucket: inputs are ~30/s; allow bursts, drop floods.
pub(super) struct RateLimit {
    pub(super) tokens: f64,
    pub(super) last: std::time::Instant,
}

impl RateLimit {
    pub(super) fn allow(&mut self) -> bool {
        let now = std::time::Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f64() * 120.0).min(240.0);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub async fn run_session(conn: Connection, game: GameShared, stats: Arc<NetStats>) {
    if let Err(e) = session(conn, game, stats).await {
        tracing::debug!("session ended: {e}");
    }
}

pub(super) async fn reject(tx: &mut SendStream, reason: RejectReason) -> anyhow::Result<()> {
    send_control(tx, ControlMsg::Reject { reason }).await?;
    anyhow::bail!("rejected: {reason:?}")
}

/// Who a Hello says the pilot is: a wallet proved by a signature (or a resume token), or a guest.
async fn identify(
    game: &GameShared,
    tx: &mut SendStream,
    rx: &mut RecvStream,
    pending: &mut Vec<u8>,
    flags: u8,
    resume: &[u8; bc_proto::auth::TOKEN_BYTES],
    pilot: PilotKind,
) -> anyhow::Result<Option<Address>> {
    let sign_in = &game.sign_in;
    if flags & hello_flags::RESUME != 0 {
        return match game.pilots.redeem(resume) {
            Some(address) => Ok(Some(address)),
            None => reject(tx, RejectReason::ResumeExpired).await.map(|_| None),
        };
    }
    if flags & hello_flags::SIGN_IN == 0 {
        if sign_in.required && pilot == PilotKind::Human {
            reject(tx, RejectReason::AuthRequired).await?;
        }
        return Ok(None);
    }
    // Only so many sign-ins may wait on a wallet at once.
    let Ok(_permit) = sign_in.waiting.clone().try_acquire_owned() else {
        return reject(tx, RejectReason::ServerFull).await.map(|_| None);
    };
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    let issued_at = pilots::unix_now();
    let domain = Domain::new(&sign_in.domain);
    send_control(tx, ControlMsg::Challenge { nonce, issued_at, domain }).await?;
    let answer = match tokio::time::timeout(sign_in.wait, read_control(rx, pending)).await {
        Ok(answer) => answer?,
        Err(_) => return reject(tx, RejectReason::AuthTimeout).await.map(|_| None),
    };
    let ControlMsg::Auth { address, signature } = answer else {
        return reject(tx, RejectReason::BadHello).await.map(|_| None);
    };
    match bc_auth::verify(domain.as_str(), &nonce, issued_at, &address, &signature) {
        Ok(()) => Ok(Some(address)),
        Err(e) => {
            tracing::info!(address = %pilots::short(&address), "sign-in refused: {e:?}");
            reject(tx, RejectReason::AuthFailed).await.map(|_| None)
        }
    }
}

async fn session(conn: Connection, game: GameShared, stats: Arc<NetStats>) -> anyhow::Result<()> {
    let (mut tx, mut rx) = tokio::time::timeout(Duration::from_secs(5), conn.accept_bi()).await??;
    let mut pending = Vec::new();
    let hello = tokio::time::timeout(Duration::from_secs(5), read_control(&mut rx, &mut pending)).await??;
    let ControlMsg::Hello { version, pilot, frame, faction, name, flags, resume } = hello else {
        return reject(&mut tx, RejectReason::BadHello).await;
    };
    if version != PROTOCOL_VERSION {
        return reject(&mut tx, RejectReason::VersionMismatch).await;
    }
    // (Under survival rules the frame asked for doesn't matter: pilots fly what they built.)
    if !game.survival && !bc_sim::content::playable(frame) {
        return reject(&mut tx, RejectReason::FrameNotAllowed).await;
    }
    // Only the Bot SDK may claim to be an agent; nobody may claim to be a server-side doll.
    let pilot = if pilot == PilotKind::MobileDoll { PilotKind::Agent } else { pilot };
    let address = identify(&game, &mut tx, &mut rx, &mut pending, flags, &resume, pilot).await?;
    // A pilot flies in one place at a time: a newer session takes over.
    let (held, kicked) = match address {
        Some(a) => {
            let Claim { address, session, kicked } = game.pilots.claim(a).await;
            (Some((address, session)), Some(kicked))
        }
        None => (None, None),
    };
    let who = super::session::Who { pilot, frame, faction, name, address };
    let result = super::session::run(&conn, &game, &stats, &mut tx, &mut rx, pending, who, kicked).await;
    if let Some((address, session)) = held {
        game.pilots.release(address, session);
    }
    result
}

/// Sets a pilot's roster entry's flags (and tells everyone).
pub(super) fn set_roster_flags(game: &GameShared, suit: u16, pilot: PilotKind, name: &str, flags: u8) {
    if let Ok(mut r) = game.roster.write()
        && let Some(e) = r.get_mut(&suit)
    {
        e.flags = flags;
    }
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: name.to_string(), flags });
}

/// The roster forgets a suit's pilot (and tells everyone).
pub(super) fn forget(game: &GameShared, suit: u16, pilot: PilotKind) {
    if let Ok(mut r) = game.roster.write() {
        r.remove(&suit);
    }
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: String::new(), flags: 0 });
}

/// What became of sleepers, from the sector: news for their pilots, and the roster forgets them.
pub fn process_notes(game: &GameShared) {
    while let Some(fate) = game.sector.notes.pop() {
        let Some(address) = game.pilots.suit_gone((fate.suit, fate.generation)) else { continue };
        let news = match fate.gone {
            Gone::Destroyed { killer } => Fate::Destroyed {
                by: game
                    .roster
                    .read()
                    .ok()
                    .and_then(|r| r.get(&killer).map(|e| e.name.clone()))
                    .unwrap_or_default(),
            },
            Gone::Evicted => Fate::Lost,
        };
        tracing::info!(address = %pilots::short(&address), suit = fate.suit, "sleeper gone: {news:?}");
        game.pilots.tell(address, news);
        // Only the sleeper's entry: the slot may have a new pilot already.
        let asleep = game
            .roster
            .read()
            .ok()
            .is_some_and(|r| r.get(&fate.suit).is_some_and(|e| e.flags & roster_flags::ASLEEP != 0));
        if asleep {
            forget(game, fate.suit, PilotKind::Human);
        }
    }
}
