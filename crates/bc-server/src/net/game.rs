//! Game mode: WebTransport sessions ↔ the sector thread, the egress thread, and the pilot roster.
//!
//! Everything here is network-side: it may allocate and may use tokio primitives. It reaches the
//! sector only through `bc-sector`'s lock-free queues.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use bc_proto::control::{ControlMsg, MAX_FRAME, Name, RejectReason};
use bc_proto::{InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PacketKind, PilotKind, packet_kind};
use bc_sector::{
    Control, EgressEnds, InputMsg, Metrics, SectorConfig, SectorShared, SectorThread, SlotState, read_packet,
};
use bc_sim::SimConfig;
use crossbeam_queue::ArrayQueue;
use tokio::sync::broadcast;
use wtransport::{Connection, RecvStream, SendStream};

use super::NetStats;
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
}

/// Change to the roster, fanned out to every session.
#[derive(Clone, Debug)]
pub struct RosterUpdate {
    /// Entity slot of the pilot's suit.
    pub suit: u16,
    pub pilot: PilotKind,
    /// Empty = the pilot left.
    pub name: String,
}

/// What each session task needs to reach the sector.
#[derive(Clone)]
pub struct GameShared {
    pub sector: Arc<SectorShared>,
    egress: Arc<ArrayQueue<EgressCmd>>,
    egress_thread: thread::Thread,
    roster: Arc<RwLock<HashMap<u16, RosterEntry>>>,
    roster_tx: broadcast::Sender<RosterUpdate>,
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
    pub fn start(cfg: &Config, stats: Arc<NetStats>) -> anyhow::Result<Self> {
        let jev_key = cfg.jev_key.clone();
        let use_jev = cfg.oracle == OracleKind::Jev && jev_key.is_some();
        if cfg.oracle == OracleKind::Jev && !use_jev {
            tracing::warn!(
                "--oracle jev requested but TYPESAFE_API_KEY is not set: using the local oracle only"
            );
        }
        let sector_cfg = SectorConfig {
            sim: SimConfig { target_dolls: cfg.mobile_dolls, seed: cfg.seed, ..SimConfig::default() },
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
        Ok(Self {
            sector: Some(sector_thread),
            egress: Some(egress),
            egress_stop: stop,
            shared: GameShared {
                sector: shared,
                egress: queue,
                egress_thread,
                roster: Arc::new(RwLock::new(HashMap::new())),
                roster_tx,
            },
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
            pilots.push(serde_json::json!({
                "client_slot": slot,
                "suit": suit,
                "name": entry.map(|e| e.name.clone()).unwrap_or_default(),
                "pilot": entry.map(|e| format!("{:?}", e.pilot)).unwrap_or_default(),
                "shots": l(&p.shots),
                "hits": l(&p.hits),
                "kills": l(&p.kills),
                "deaths": l(&p.deaths),
            }));
        }
        serde_json::json!({
            "tick": s.tick.load(Ordering::Acquire),
            "tick_hz": bc_sim::TICK_HZ,
            "oracle": self.oracle,
            "clients": l(&m.clients),
            "suits_alive": l(&m.suits_alive),
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

async fn session(conn: Connection, game: GameShared, stats: Arc<NetStats>) -> anyhow::Result<()> {
    let (mut tx, mut rx) = tokio::time::timeout(Duration::from_secs(5), conn.accept_bi()).await??;
    let mut pending = Vec::new();
    let hello = tokio::time::timeout(Duration::from_secs(5), read_control(&mut rx, &mut pending)).await??;
    let ControlMsg::Hello { version, pilot, frame, faction, name } = hello else {
        send_control(&mut tx, ControlMsg::Reject { reason: RejectReason::BadHello }).await?;
        anyhow::bail!("first frame was not Hello");
    };
    if version != PROTOCOL_VERSION {
        send_control(&mut tx, ControlMsg::Reject { reason: RejectReason::VersionMismatch }).await?;
        anyhow::bail!("protocol version {version}");
    }
    // Only the Bot SDK may claim to be an agent; nobody may claim to be a server-side doll.
    let pilot = if pilot == PilotKind::MobileDoll { PilotKind::Agent } else { pilot };
    let Some(mut lease) = game.sector.leases.pop() else {
        NetStats::add(&stats.sessions_rejected, 1);
        send_control(&mut tx, ControlMsg::Reject { reason: RejectReason::ServerFull }).await?;
        anyhow::bail!("server full");
    };
    let slot = lease.slot;
    let max_datagram = conn.max_datagram_size().unwrap_or(MAX_DATAGRAM).min(MAX_DATAGRAM) as u16;
    let epoch = game.sector.slots[slot as usize].epoch();
    let mut join = Control::Join { slot, pilot, frame, faction, max_datagram };
    while let Err(back) = game.sector.control.push(join) {
        join = back;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let state = wait_slot(&game.sector, slot, |s, e| e != epoch && s != SlotState::Free).await;
    let suit = match (state, game.sector.slots[slot as usize].suit()) {
        (Some(SlotState::Active), Some(suit)) => suit,
        _ => {
            let _ = game.sector.leases.push(lease);
            NetStats::add(&stats.sessions_rejected, 1);
            send_control(&mut tx, ControlMsg::Reject { reason: RejectReason::ServerFull }).await?;
            anyhow::bail!("sector refused the pilot");
        }
    };
    let result =
        in_game(&conn, &game, &stats, &mut tx, &mut rx, pending, &mut lease, suit, pilot, name).await;

    // Tear down in the order that keeps the slot race-free: stop sending, forget the name, let the
    // sector release the suit, and only then hand the lease back.
    let _ = game.egress.push(EgressCmd::Detach(slot));
    game.egress_thread.unpark();
    if let Ok(mut r) = game.roster.write() {
        r.remove(&suit);
    }
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: String::new() });
    let mut leave = Control::Leave { slot };
    while let Err(back) = game.sector.control.push(leave) {
        leave = back;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let _ = wait_slot(&game.sector, slot, |s, _| s == SlotState::Free).await;
    let _ = game.sector.leases.push(lease);
    result
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
    name: Name,
) -> anyhow::Result<()> {
    let slot = lease.slot;
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
        },
    )
    .await?;
    let callsign = if name.is_empty() { format!("Pilot-{slot}") } else { name.as_str().to_string() };
    tracing::info!(slot, suit, ?pilot, name = %callsign, "pilot joined");
    let mut roster_rx = game.roster_tx.subscribe();
    let everyone: Vec<(u16, RosterEntry)> = {
        let mut r = game.roster.write().map_err(|_| anyhow::anyhow!("roster poisoned"))?;
        r.insert(suit, RosterEntry { name: callsign.clone(), pilot, client_slot: slot });
        r.iter().map(|(k, v)| (*k, v.clone())).collect()
    };
    let _ = game.roster_tx.send(RosterUpdate { suit, pilot, name: callsign });
    for (s, e) in everyone {
        send_control(tx, ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name) }).await?;
    }
    let _ = game.egress.push(EgressCmd::Attach(slot, conn.clone()));
    game.egress_thread.unpark();

    let mut rate = RateLimit { tokens: 240.0, last: std::time::Instant::now() };
    let mut buf = [0u8; 512];
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
                    Ok(u) => send_control(tx, ControlMsg::Roster { slot: u.suit, pilot: u.pilot, name: Name::new(&u.name) }).await?,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let everyone: Vec<(u16, RosterEntry)> = game.roster.read().map(|r| r.iter().map(|(k, v)| (*k, v.clone())).collect()).unwrap_or_default();
                        for (s, e) in everyone {
                            send_control(tx, ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name) }).await?;
                        }
                    }
                    Err(_) => return Ok(()),
                }
            }
        }
    }
}
