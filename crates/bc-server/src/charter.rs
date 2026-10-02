//! The colony's Charter Board, shared by every session: one [`Board`] behind a lock (network side;
//! the sector never sees it), a version that moves when it does, and its file in the data
//! directory, if there is one. Lock order: the board, then the exchange
//! ([`Market`](crate::market::Market)), never the other way round.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::Board;

use crate::market::Market;
use crate::pilots::write_atomically;

pub struct Charter {
    board: Mutex<Board>,
    version: AtomicU64,
    path: Option<PathBuf>,
}

impl Charter {
    /// The board kept at `path` (a new one if there's nothing there), or in memory only.
    pub fn open(path: Option<PathBuf>) -> Self {
        let loaded = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|bytes| {
            serde_json::from_slice::<Board>(&bytes)
                .map_err(|e| tracing::warn!("charter board file unreadable, starting afresh: {e}"))
                .ok()
        });
        if let Some(b) = &loaded {
            tracing::info!(era = b.era(), contracts = b.contracts().count(), "charter board restored");
        }
        Self { board: Mutex::new(loaded.unwrap_or_default()), version: AtomicU64::new(1), path }
    }

    /// Runs `f` on the board.
    pub fn with<R>(&self, f: impl FnOnce(&mut Board) -> R) -> R {
        let mut b = self.board.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut b)
    }

    /// Something on the board moved: views are out of date.
    pub fn changed(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// The clock at `now` (Unix seconds): contracts expire, the colony posts what it needs.
    pub fn tick(&self, now: u64, market: &Market) {
        let moved = self.with(|b| market.with(|ex| b.tick(now, ex)));
        if moved {
            self.changed();
        }
    }

    /// Writes the board to its file, if it has one.
    pub async fn save(&self) {
        let Some(path) = self.path.clone() else { return };
        let bytes = match self.with(|b| serde_json::to_vec(b)) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("charter board: {e}");
                return;
            }
        };
        match tokio::task::spawn_blocking(move || write_atomically(&path, &bytes)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("charter board not saved: {e}"),
            Err(e) => tracing::warn!("charter board not saved: {e}"),
        }
    }
}
