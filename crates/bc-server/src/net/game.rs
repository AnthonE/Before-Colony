//! Game mode: wires WebTransport sessions to the sector runtime (filled in P5).

use std::sync::Arc;

use wtransport::Connection;

use super::NetStats;
use crate::Config;

/// Owns the sector and egress threads.
pub struct GameRuntime;

/// What each session task needs to reach the sector.
#[derive(Clone)]
pub struct GameShared;

/// Read-only view of the sector metrics for `/status`.
#[derive(Clone)]
pub struct StatusView;

impl GameRuntime {
    pub fn start(_cfg: &Config, _stats: Arc<NetStats>) -> anyhow::Result<Self> {
        anyhow::bail!("game mode is not wired up yet")
    }
    pub fn shared(&self) -> GameShared {
        GameShared
    }
    pub fn status_view(&self) -> StatusView {
        StatusView
    }
    pub fn stop(self) {}
}

impl StatusView {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

pub async fn run_session(_conn: Connection, _game: GameShared, _stats: Arc<NetStats>) {}
