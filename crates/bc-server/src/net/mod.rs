//! Network side of the server: WebTransport sessions and the egress thread. Code here may allocate
//! and use tokio primitives; it talks to the sector thread only through the lock-free queues in
//! `bc-sector`.

pub mod admit;
mod desk;
pub mod echo;
pub mod endpoint;
pub mod game;
mod session;

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
    /// Hangar requests refused for coming too fast, and sessions ended for it.
    pub requests_refused: AtomicU64,
    pub flooders_ended: AtomicU64,
    /// Sessions open now past their Hello, by who's at the controls: people (the store's player
    /// count, `/status.json`) and agents.
    pub humans_active: AtomicU64,
    pub agents_active: AtomicU64,
}

impl NetStats {
    #[inline]
    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
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
        "requests_refused": r(&stats.requests_refused),
        "flooders_ended": r(&stats.flooders_ended),
        "humans_active": r(&stats.humans_active),
        "agents_active": r(&stats.agents_active),
    });
    match game {
        Some(view) => serde_json::json!({ "mode": "game", "net": net, "game": view.json() }),
        None => serde_json::json!({ "mode": "echo", "net": net }),
    }
}

/// The `/status.json` document, in the shape the Elo Pros launcher and store poll (Gates' shard
/// serves the same): `players` are the people connected (agents and drones apart), `max_players`
/// the slots, `tick` the sector's.
pub fn launcher_status_json(
    stats: &NetStats,
    game: Option<&game::StatusView>,
    max_players: usize,
) -> serde_json::Value {
    let r = |c: &AtomicU64| c.load(Ordering::Relaxed);
    serde_json::json!({
        "players": r(&stats.humans_active),
        "agents": r(&stats.agents_active),
        "max_players": max_players,
        "tick": game.map_or(0, |g| g.tick()),
        "proto": bc_proto::PROTOCOL_VERSION,
        "mode": if game.is_some() { "game" } else { "echo" },
    })
}
