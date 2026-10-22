//! The colony's register of homes, shared by every session (`docs/OPEN_CITY.md`, H1): one
//! [`Homes`] behind a lock (network side; the sector never sees it), a version that moves when it
//! does, and its file in the data directory, if there is one. The plaza asks it (through
//! [`HomeRegister::may_enter`]) whether a pose may cross into a home: the plaza's lock is taken
//! first, then this one, and never the other way round.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_econ::homes::Homes;
use bc_sim::colony::homes::HomeId;

use crate::pilots::write_atomically;

pub struct HomeRegister {
    homes: Mutex<Homes>,
    version: AtomicU64,
    path: Option<PathBuf>,
    saving: tokio::sync::Mutex<()>,
}

impl HomeRegister {
    /// The register kept at `path` (a new one if there's nothing there), or in memory only.
    pub fn open(path: Option<PathBuf>) -> Self {
        let loaded = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|bytes| {
            serde_json::from_slice::<Homes>(&bytes)
                .map_err(|e| tracing::warn!("register of homes unreadable, starting afresh: {e}"))
                .ok()
        });
        if let Some(h) = &loaded {
            tracing::info!(held = h.all().count(), "the register of homes restored");
        }
        Self {
            homes: Mutex::new(loaded.unwrap_or_default()),
            version: AtomicU64::new(1),
            path,
            saving: tokio::sync::Mutex::new(()),
        }
    }

    /// Runs `f` on the register.
    pub fn with<R>(&self, f: impl FnOnce(&mut Homes) -> R) -> R {
        let mut h = self.homes.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut h)
    }

    /// Something changed: views are out of date.
    pub fn changed(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Whether `who` may go into home `id` (`Homes::may_enter`).
    pub fn may_enter(&self, id: HomeId, who: &str) -> bool {
        self.with(|h| h.may_enter(id, who))
    }

    /// The clock at `now` (Unix seconds): rent charged, leases lapsed, homes left a month sold back.
    pub fn tick(&self, now: u64) {
        if self.with(|h| h.tick(now)) {
            self.changed();
        }
    }

    /// Writes the register to its file, if it has one (one write at a time).
    pub async fn save(&self) {
        let Some(path) = self.path.clone() else { return };
        let _one = self.saving.lock().await;
        let bytes = match self.with(|h| serde_json::to_vec(h)) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("register of homes: {e}");
                return;
            }
        };
        match tokio::task::spawn_blocking(move || write_atomically(&path, &bytes)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("register of homes not saved: {e}"),
            Err(e) => tracing::warn!("register of homes not saved: {e}"),
        }
    }
}
