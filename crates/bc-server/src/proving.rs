//! The Proving Ground's board, shared by every session: one [`Board`] behind a lock (network side;
//! the sector never sees it: it reports the times it checks on each pilot's report ring), a
//! version that moves when it does, and its file in the data directory, if there is one.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::proving::Board;

use crate::pilots::write_atomically;

pub struct Proving {
    board: Mutex<Board>,
    version: AtomicU64,
    path: Option<PathBuf>,
}

impl Proving {
    /// The board kept at `path` (a new one if there's nothing there), or in memory only.
    pub fn open(path: Option<PathBuf>) -> Self {
        let loaded = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|bytes| {
            serde_json::from_slice::<Board>(&bytes)
                .map_err(|e| tracing::warn!("proving ground board file unreadable, starting afresh: {e}"))
                .ok()
        });
        if loaded.is_some() {
            tracing::info!("the proving ground's board restored");
        }
        Self { board: Mutex::new(loaded.unwrap_or_default()), version: AtomicU64::new(1), path }
    }

    /// Runs `f` on the board.
    pub fn with<R>(&self, f: impl FnOnce(&mut Board) -> R) -> R {
        let mut b = self.board.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut b)
    }

    /// A time went on it, or the day turned: views are out of date.
    pub fn changed(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// The clock at `now` (Unix seconds): at midnight, a new day's lists.
    pub fn tick(&self, now: u64) {
        if self.with(|b| b.turn(now)) {
            self.changed();
        }
    }

    /// Writes the board to its file, if it has one.
    pub async fn save(&self) {
        let Some(path) = self.path.clone() else { return };
        let bytes = match self.with(|b| serde_json::to_vec(b)) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("proving ground board: {e}");
                return;
            }
        };
        match tokio::task::spawn_blocking(move || write_atomically(&path, &bytes)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("proving ground board not saved: {e}"),
            Err(e) => tracing::warn!("proving ground board not saved: {e}"),
        }
    }
}
