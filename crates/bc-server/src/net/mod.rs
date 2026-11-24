//! Network side of the server: WebTransport sessions and the egress thread. Code here may allocate
//! and use tokio primitives; it talks to the sector thread only through the lock-free queues in
//! `bc-sector`.

pub mod admit;
mod desk;
pub mod echo;
pub mod endpoint;
pub mod game;
mod session;
pub mod sessions;

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters maintained by the network tasks (all relaxed; they're only for `/status`).
#[derive(Default)]
pub struct NetStats {
    pub sessions_total: AtomicU64,
    pub sessions_active: AtomicU64,
    pub sessions_rejected: AtomicU64,
    pub datagrams_in: AtomicU64,
    pub datagrams_out: AtomicU64,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub malformed: AtomicU64,
    /// Connection attempts turned away (`admit`): the server full, an address over its share,
    /// sent a Retry to prove their address, or too slow to finish their handshakes.
    pub refused_full: AtomicU64,
    pub refused_address: AtomicU64,
    pub retried: AtomicU64,
    pub handshake_timeouts: AtomicU64,
    /// The connections QUIC really holds (`Endpoint::open_connections`, published by the accept
    /// loop as it goes round and every half second): whatever the admission's bookkeeping says, a
    /// connection that outlived its pass (closing, or stuck) is counted here. Past
    /// [`admit::Limits::quic_connections`] a new one is refused (`refused_open`).
    pub open_connections: AtomicU64,
    pub refused_open: AtomicU64,
    /// Hangar requests refused for coming too fast, and sessions ended for it.
    pub requests_refused: AtomicU64,
    pub flooders_ended: AtomicU64,
    /// Sessions open now past their Hello, by who's at the controls: people and agents (together,
    /// the store's player count, `/status.json`).
    pub humans_active: AtomicU64,
    pub agents_active: AtomicU64,
    /// Signed calls to Elo Pros (`crate::serverkey`): taken, refused (a 4xx it meant), never got
    /// through after every try; and the sessions' stats rows dropped for a full queue
    /// (`crate::gamestats`).
    pub elo_sent: AtomicU64,
    pub elo_refused: AtomicU64,
    pub elo_failed: AtomicU64,
    pub elo_dropped: AtomicU64,
}

impl NetStats {
    #[inline]
    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    #[inline]
    pub fn set(gauge: &AtomicU64, v: u64) {
        gauge.store(v, Ordering::Relaxed);
    }
}

/// The `/status` document.
pub fn status_json(stats: &NetStats, game: Option<&game::StatusView>) -> serde_json::Value {
    let r = |c: &AtomicU64| c.load(Ordering::Relaxed);
    let net = serde_json::json!({
        "sessions_total": r(&stats.sessions_total),
        "sessions_active": r(&stats.sessions_active),
        "sessions_rejected": r(&stats.sessions_rejected),
        "datagrams_in": r(&stats.datagrams_in),
        "datagrams_out": r(&stats.datagrams_out),
        "bytes_in": r(&stats.bytes_in),
        "bytes_out": r(&stats.bytes_out),
        "malformed": r(&stats.malformed),
        "refused_full": r(&stats.refused_full),
        "refused_address": r(&stats.refused_address),
        "retried": r(&stats.retried),
        "handshake_timeouts": r(&stats.handshake_timeouts),
        "open_connections": r(&stats.open_connections),
        "refused_open": r(&stats.refused_open),
        "requests_refused": r(&stats.requests_refused),
        "flooders_ended": r(&stats.flooders_ended),
        "humans_active": r(&stats.humans_active),
        "agents_active": r(&stats.agents_active),
        "elo_sent": r(&stats.elo_sent),
        "elo_refused": r(&stats.elo_refused),
        "elo_failed": r(&stats.elo_failed),
        "elo_dropped": r(&stats.elo_dropped),
    });
    match game {
        Some(view) => serde_json::json!({ "mode": "game", "net": net, "game": view.json() }),
        None => serde_json::json!({ "mode": "echo", "net": net }),
    }
}

/// The `/status.json` document, in the shape the Elo Pros launcher and store poll (Gates' shard
/// serves the same): `players` are everyone connected, people and agents (drones apart), and
/// `agents` the share of them agents fly, so the store's people are `players - agents`
/// (`meter/launcher.py`); `max_players` the slots, `tick` the sector's.
pub fn launcher_status_json(
    stats: &NetStats,
    game: Option<&game::StatusView>,
    max_players: usize,
) -> serde_json::Value {
    let r = |c: &AtomicU64| c.load(Ordering::Relaxed);
    let (humans, agents) = (r(&stats.humans_active), r(&stats.agents_active));
    serde_json::json!({
        "players": humans + agents,
        "agents": agents,
        "max_players": max_players,
        "tick": game.map_or(0, |g| g.tick()),
        "proto": bc_proto::PROTOCOL_VERSION,
        "mode": if game.is_some() { "game" } else { "echo" },
    })
}
