//! The colony's exchange, shared by every session: one [`Exchange`] behind a lock (network side;
//! the sector never sees it), a version that moves when prices do, and its file in the data
//! directory, if there is one.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::Exchange;

use crate::pilots::write_atomically;

/// How often the colony's drifting prices count as a change worth resending, s.
const DRIFT_SECS: f64 = 15.0;

pub struct Market {
    exchange: Mutex<Exchange>,
    version: AtomicU64,
    path: Option<PathBuf>,
    drift: Mutex<f64>,
}

impl Market {
    /// The exchange kept at `path` (a new one if there's nothing there), or in memory only.
    pub fn open(path: Option<PathBuf>) -> Self {
        let loaded = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|bytes| {
            serde_json::from_slice::<Exchange>(&bytes)
                .map_err(|e| tracing::warn!("exchange file unreadable, starting afresh: {e}"))
                .ok()
        });
        let loaded = loaded.map(|mut x: Exchange| {
            let opened = x.seed_missing();
            if opened > 0 {
                tracing::info!("the colony opened {opened} new desks on the exchange");
            }
            x
        });
        if loaded.is_some() {
            tracing::info!(
                "exchange restored from {}",
                path.as_ref().map_or_else(String::new, |p| p.display().to_string())
            );
        }
        Self {
            exchange: Mutex::new(loaded.unwrap_or_default()),
            version: AtomicU64::new(1),
            path,
            drift: Mutex::new(0.0),
        }
    }

    /// Runs `f` on the exchange.
    pub fn with<R>(&self, f: impl FnOnce(&mut Exchange) -> R) -> R {
        let mut ex = self.exchange.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut ex)
    }

    /// Something traded, or an order came or went: views are out of date.
    pub fn changed(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// The clock runs on `dt` seconds (the colony's stock settles; prices drift).
    pub fn tick(&self, dt: f64) {
        self.with(|ex| ex.tick(dt));
        let mut drift = self.drift.lock().unwrap_or_else(|e| e.into_inner());
        *drift += dt;
        if *drift >= DRIFT_SECS {
            *drift = 0.0;
            self.changed();
        }
    }

    /// Writes the exchange to its file, if it has one.
    pub async fn save(&self) {
        let Some(path) = self.path.clone() else { return };
        let bytes = match self.with(|ex| serde_json::to_vec(ex)) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("exchange: {e}");
                return;
            }
        };
        match tokio::task::spawn_blocking(move || write_atomically(&path, &bytes)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("exchange not saved: {e}"),
            Err(e) => tracing::warn!("exchange not saved: {e}"),
        }
    }
}
