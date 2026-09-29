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
use bc_proto::control::{
    ControlMsg, MAX_FRAME, Name, RejectReason, bye, hello_flags, notice, roster_flags, welcome_flags,
};
use bc_proto::{InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PacketKind, PilotKind, packet_kind};
use bc_sector::{
    Comeback, Control, EgressEnds, InputMsg, Metrics, Outcome, SectorConfig, SectorShared, SectorThread,
    SlotState, read_packet,
};
use bc_sim::SimConfig;
use bc_sim::sim::Gone;
use crossbeam_queue::ArrayQueue;
use tokio::sync::{broadcast, oneshot};
use wtransport::{Connection, RecvStream, SendStream};

use super::NetStats;
use crate::pilots::{self, Claim, Fate, MemoryStore, Pilots, Sleeper};
use crate::{Config, OracleKind};

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

/// What each session task needs to reach the sector.
#[derive(Clone)]
pub struct GameShared {
    pub sector: Arc<SectorShared>,
    egress: Arc<ArrayQueue<EgressCmd>>,
    egress_thread: thread::Thread,
    roster: Arc<RwLock<HashMap<u16, RosterEntry>>>,
    roster_tx: broadcast::Sender<RosterUpdate>,
    pub pilots: Arc<Pilots>,
    sign_in: SignIn,
    /// A session with no input for this long ends.
    idle: Duration,
}

/// Read-only view for `/status`.
#[derive(Clone)]
pub struct StatusView {
    sector: Arc<SectorShared>,
    roster: Arc<RwLock<HashMap<u16, RosterEntry>>>,
    oracle: &'static str,
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
        let sector_cfg = SectorConfig {
            sim: SimConfig {
                target_dolls: cfg.mobile_dolls,
                seed: cfg.seed,
                max_sleepers: cfg.max_sleepers,
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
            "sector running at 30 Hz"
        );
        let game = GameShared {
            sector: shared,
            egress: queue,
            egress_thread,
            roster: Arc::new(RwLock::new(HashMap::new())),
            roster_tx,
            pilots: Arc::new(Pilots::new(Arc::new(MemoryStore::default()), cfg.resume_ttl)),
            sign_in: SignIn {
                domain: domain.into(),
                required: cfg.require_auth,
                wait: cfg.sign_wait,
                waiting: Arc::new(tokio::sync::Semaphore::new(128)),
            },
            idle: cfg.idle_timeout,
        };
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
        }
    }

    pub fn stop(mut self) {
        if let Some(s) = self.sector.take() {
            s.stop();
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
        serde_json::json!({
            "tick": s.tick.load(Ordering::Acquire),
            "tick_hz": bc_sim::TICK_HZ,
            "oracle": self.oracle,
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

async fn send_control(tx: &mut SendStream, msg: ControlMsg) -> anyhow::Result<()> {
    let mut buf = [0u8; MAX_FRAME];
    let n = msg.encode(&mut buf).ok_or_else(|| anyhow::anyhow!("control frame too large"))?;
    tx.write_all(&buf[..n]).await?;
    Ok(())
}

async fn read_control(rx: &mut RecvStream, pending: &mut Vec<u8>) -> anyhow::Result<ControlMsg> {
    let mut buf = [0u8; 256];
    loop {
        if let Some((msg, used)) =
            ControlMsg::decode(pending).map_err(|e| anyhow::anyhow!("bad control frame: {e}"))?
        {
            pending.drain(..used);
            return Ok(msg);
        }
        match rx.read(&mut buf).await? {
            Some(n) => pending.extend_from_slice(&buf[..n]),
            None => anyhow::bail!("control stream closed"),
        }
    }
}

async fn wait_slot(
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
struct RateLimit {
    tokens: f64,
    last: std::time::Instant,
}

impl RateLimit {
    fn allow(&mut self) -> bool {
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

async fn reject(tx: &mut SendStream, reason: RejectReason) -> anyhow::Result<()> {
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
    if !bc_sim::content::playable(frame) {
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
    let result =
        seated(&conn, &game, &stats, &mut tx, &mut rx, pending, pilot, frame, faction, name, address, kicked)
            .await;
    if let Some((address, session)) = held {
        game.pilots.release(address, session);
    }
    result
}

/// Takes a slot in the sector, flies, and gives the slot back. A signed-in pilot wakes in the suit
/// they left asleep, if it's still there, and leaves it asleep again.
#[allow(clippy::too_many_arguments)]
async fn seated(
    conn: &Connection,
    game: &GameShared,
    stats: &NetStats,
    tx: &mut SendStream,
    rx: &mut RecvStream,
    pending: Vec<u8>,
    pilot: PilotKind,
    frame: bc_proto::FrameId,
    faction: bc_proto::Faction,
    name: Name,
    address: Option<Address>,
    kicked: Option<oneshot::Receiver<()>>,
) -> anyhow::Result<()> {
    let Some(mut lease) = game.sector.leases.pop() else {
        NetStats::add(&stats.sessions_rejected, 1);
        return reject(tx, RejectReason::ServerFull).await;
    };
    let slot = lease.slot;
    let callsign = if name.is_empty() { format!("Pilot-{slot}") } else { name.as_str().to_string() };
    // A signed-in pilot's record: the suit they left asleep, and what they'd earned.
    let mut record = match address {
        Some(a) => Some(game.pilots.load_or_new(&a).await),
        None => None,
    };
    let left = record.as_ref().and_then(|r| r.sleeper);
    let comeback = Comeback {
        // A sleeper from before the server restarted is long gone.
        sleeper: left.filter(|s| s.run == game.pilots.run).map(|s| (s.suit, s.generation)),
        credits: record.as_ref().map_or(0, |r| r.credits),
    };
    let max_datagram = conn.max_datagram_size().unwrap_or(MAX_DATAGRAM).min(MAX_DATAGRAM) as u16;
    let epoch = game.sector.slots[slot as usize].epoch();
    let mut join = Control::Join { slot, pilot, frame, faction, max_datagram, comeback, launch: None };
    while let Err(back) = game.sector.control.push(join) {
        join = back;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let state = wait_slot(&game.sector, slot, |s, e| e != epoch && s != SlotState::Free).await;
    let status = &game.sector.slots[slot as usize];
    let (suit, generation) = match (state, status.suit_id()) {
        (Some(SlotState::Active), Some(id)) => id,
        _ => {
            let _ = game.sector.leases.push(lease);
            NetStats::add(&stats.sessions_rejected, 1);
            return reject(tx, RejectReason::ServerFull).await;
        }
    };
    let woke = status.outcome() == Outcome::Woke;
    // What became of the suit they left, if they didn't wake in it.
    let news = match (address, left) {
        (Some(a), Some(_)) if !woke => {
            // The sector reports a sleeper's end before it can find it gone: it's in by now.
            process_notes(game);
            Some(game.pilots.take_news(&a).unwrap_or(Fate::Lost))
        }
        (Some(a), _) => {
            // News without a sleeper to go with it is stale.
            let _ = game.pilots.take_news(&a);
            None
        }
        (None, _) => None,
    };
    if let (Some(a), Some(r)) = (address, record.as_mut()) {
        game.pilots.bind_suit((suit, generation), a);
        r.sleeper = None;
        r.name = callsign.clone();
        r.frame = frame.slug().to_string();
        r.seen_unix = pilots::unix_now();
        game.pilots.save(r.clone()).await;
    }
    let result = in_game(
        conn, game, stats, tx, rx, pending, &mut lease, suit, pilot, &callsign, address, kicked, woke, news,
    )
    .await;

    // Tear down in the order that keeps the slot race-free: stop sending, settle the roster, let
    // the sector put the suit to sleep (or release it), and only then hand the lease back.
    let _ = game.egress.push(EgressCmd::Detach(slot));
    game.egress_thread.unpark();
    let credits =
        u32::try_from(Metrics::load(&game.sector.metrics.pilots[slot as usize].credits)).unwrap_or(0);
    if address.is_some() {
        // The suit stays, its pilot asleep in the cockpit: marked so in the roster before it
        // sleeps, so news of its end finds it marked.
        set_roster_flags(game, suit, pilot, &callsign, roster_flags::VERIFIED | roster_flags::ASLEEP);
    } else {
        forget(game, suit, pilot);
    }
    let mut bye = if address.is_some() { Control::Sleep { slot } } else { Control::Leave { slot } };
    while let Err(back) = game.sector.control.push(bye) {
        bye = back;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let _ = wait_slot(&game.sector, slot, |s, _| s == SlotState::Free).await;
    if let (Some(a), Some(r)) = (address, record.as_mut()) {
        let now = pilots::unix_now();
        r.credits = credits;
        r.seen_unix = now;
        match (status.outcome(), status.suit_id()) {
            (Outcome::Asleep, Some((s, g))) => {
                r.sleeper = Some(Sleeper { run: game.pilots.run, suit: s, generation: g, since_unix: now });
                tracing::info!(address = %pilots::short(&a), suit = s, "asleep in the cockpit");
            }
            // A wreck doesn't sleep: it's gone.
            _ => {
                let _ = game.pilots.suit_gone((suit, generation));
                forget(game, suit, pilot);
            }
        }
        game.pilots.save(r.clone()).await;
    }
    let _ = game.sector.leases.push(lease);
    result
}

/// Sets a pilot's roster entry's flags (and tells everyone).
fn set_roster_flags(game: &GameShared, suit: u16, pilot: PilotKind, name: &str, flags: u8) {
    if let Ok(mut r) = game.roster.write()
        && let Some(e) = r.get_mut(&suit)
    {
        e.flags = flags;
    }
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: name.to_string(), flags });
}

/// The roster forgets a suit's pilot (and tells everyone).
fn forget(game: &GameShared, suit: u16, pilot: PilotKind) {
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

#[allow(clippy::too_many_arguments)]
async fn in_game(
    conn: &Connection,
    game: &GameShared,
    stats: &NetStats,
    tx: &mut SendStream,
    rx: &mut RecvStream,
    mut pending: Vec<u8>,
    lease: &mut bc_sector::SlotLease,
    suit: u16,
    pilot: PilotKind,
    callsign: &str,
    address: Option<Address>,
    mut kicked: Option<oneshot::Receiver<()>>,
    woke: bool,
    news: Option<Fate>,
) -> anyhow::Result<()> {
    let slot = lease.slot;
    let flags = if address.is_some() { roster_flags::VERIFIED } else { 0 };
    let mut welcome = 0;
    if address.is_some() {
        welcome |= welcome_flags::SIGNED_IN;
    }
    if woke {
        welcome |= welcome_flags::WOKE;
    }
    send_control(
        tx,
        ControlMsg::Welcome {
            version: PROTOCOL_VERSION,
            client_slot: slot,
            tick: game.sector.tick.load(Ordering::Acquire),
            tick_hz: bc_sim::TICK_HZ as u8,
            sector: 1,
            zero_allowed: true,
            max_datagram: conn.max_datagram_size().unwrap_or(MAX_DATAGRAM).min(MAX_DATAGRAM) as u16,
            field_seed: game.sector.field_seed,
            field_rocks: game.sector.field_rocks,
            flags: welcome,
        },
    )
    .await?;
    if let Some(a) = address {
        send_control(tx, ControlMsg::Token { token: game.pilots.issue_token(a) }).await?;
    }
    match news {
        Some(Fate::Destroyed { by }) => {
            send_control(tx, ControlMsg::Notice { code: notice::SLEEPER_DESTROYED, name: Name::new(&by) })
                .await?;
        }
        Some(Fate::Lost) => {
            send_control(tx, ControlMsg::Notice { code: notice::SLEEPER_LOST, name: Name::new("") }).await?;
        }
        None => {}
    }
    tracing::info!(slot, suit, ?pilot, name = %callsign, woke, "pilot joined");
    let mut roster_rx = game.roster_tx.subscribe();
    let everyone: Vec<(u16, RosterEntry)> = {
        let mut r = game.roster.write().map_err(|_| anyhow::anyhow!("roster poisoned"))?;
        r.insert(
            suit,
            RosterEntry {
                name: callsign.to_string(),
                pilot,
                client_slot: slot,
                flags,
                address: address.as_ref().map(pilots::short),
            },
        );
        r.iter().map(|(k, v)| (*k, v.clone())).collect()
    };
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: callsign.to_string(), flags });
    for (s, e) in everyone {
        send_control(
            tx,
            ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name), flags: e.flags },
        )
        .await?;
    }
    let _ = game.egress.push(EgressCmd::Attach(slot, conn.clone()));
    game.egress_thread.unpark();

    let mut rate = RateLimit { tokens: 240.0, last: std::time::Instant::now() };
    let mut buf = [0u8; 512];
    // When the client last sent input: a session that goes quiet for too long is ended.
    let mut heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            d = conn.receive_datagram() => {
                let d = d?;
                NetStats::add(&stats.datagrams_in, 1);
                NetStats::add(&stats.bytes_in, d.len() as u64);
                if packet_kind(&d) != Some(PacketKind::Input) {
                    NetStats::add(&stats.malformed, 1);
                    continue;
                }
                match InputPacket::decode(&d) {
                    Ok(packet) if rate.allow() => {
                        heard = tokio::time::Instant::now();
                        let _ = lease.input.push(InputMsg { packet, recv_us: game.sector.now_us() });
                    }
                    Ok(_) => {}
                    Err(_) => NetStats::add(&stats.malformed, 1),
                }
            }
            r = rx.read(&mut buf) => {
                match r? {
                    Some(n) => pending.extend_from_slice(&buf[..n]),
                    None => return Ok(()),
                }
                while let Some((msg, used)) = ControlMsg::decode(&pending).map_err(|e| anyhow::anyhow!("{e}"))? {
                    pending.drain(..used);
                    match msg {
                        ControlMsg::Respawn { frame } => { let _ = game.sector.control.push(Control::Respawn { slot, frame }); }
                        ControlMsg::Bye { .. } => return Ok(()),
                        _ => {}
                    }
                }
            }
            u = roster_rx.recv() => {
                match u {
                    Ok(u) => send_control(tx, ControlMsg::Roster { slot: u.suit, pilot: u.pilot, name: Name::new(&u.name), flags: u.flags }).await?,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let everyone: Vec<(u16, RosterEntry)> = game.roster.read().map(|r| r.iter().map(|(k, v)| (*k, v.clone())).collect()).unwrap_or_default();
                        for (s, e) in everyone {
                            send_control(tx, ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name), flags: e.flags }).await?;
                        }
                    }
                    Err(_) => return Ok(()),
                }
            }
            _ = async { match kicked.as_mut() { Some(k) => { let _ = k.await; } None => std::future::pending::<()>().await } } => {
                // Signed in somewhere else: that session has the pilot now.
                let _ = send_control(tx, ControlMsg::Bye { reason: bye::TAKEN_OVER }).await;
                return Ok(());
            }
            _ = tokio::time::sleep_until(heard + game.idle) => {
                // Nobody at the controls (the tab is frozen, or the client hung): as if they'd left.
                tracing::info!(slot, name = %callsign, "idle: ending the session");
                let _ = send_control(tx, ControlMsg::Bye { reason: bye::IDLE }).await;
                return Ok(());
            }
        }
    }
}
