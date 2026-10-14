//! The oracle worker: a tokio task between the sector's lock-free rings and an oracle.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::task::JoinSet;

use crate::{TacticalAdvice, TacticalOracle, TacticalPicture};

/// Oracle calls in flight at once.
const MAX_IN_FLIGHT: usize = 8;

/// Counters and a stop switch for a running worker.
pub struct WorkerHandle {
    stop: Arc<AtomicBool>,
    pub calls: Arc<AtomicU64>,
    pub failures: Arc<AtomicU64>,
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Spawns the worker on the current tokio runtime.
pub fn spawn_worker<O: TacticalOracle>(
    mut pictures: rtrb::Consumer<TacticalPicture>,
    mut advice: rtrb::Producer<TacticalAdvice>,
    oracle: O,
) -> std::io::Result<WorkerHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicU64::new(0));
    let failures = Arc::new(AtomicU64::new(0));
    let handle = WorkerHandle { stop: stop.clone(), calls: calls.clone(), failures: failures.clone() };
    let oracle = Arc::new(oracle);
    let rt = tokio::runtime::Handle::try_current().map_err(std::io::Error::other)?;
    rt.spawn(async move {
        let mut latest: HashMap<u16, TacticalPicture> = HashMap::new();
        let mut in_flight: JoinSet<(u16, Result<TacticalAdvice, crate::OracleError>)> = JoinSet::new();
        let mut busy: HashMap<u16, ()> = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        let mut warned = false;
        while !stop.load(Ordering::Acquire) {
            interval.tick().await;
            while let Ok(p) = pictures.pop() {
                latest.insert(p.pilot, p); // keep only the freshest picture per pilot
            }
            let ready: Vec<u16> = latest.keys().copied().filter(|k| !busy.contains_key(k)).collect();
            for pilot in ready {
                if in_flight.len() >= MAX_IN_FLIGHT {
                    break;
                }
                let Some(p) = latest.remove(&pilot) else { continue };
                busy.insert(pilot, ());
                let o = oracle.clone();
                calls.fetch_add(1, Ordering::Relaxed);
                in_flight.spawn(async move { (pilot, o.assess(&p).await) });
            }
            while let Some(done) = in_flight.try_join_next() {
                let Ok((pilot, result)) = done else { continue };
                busy.remove(&pilot);
                match result {
                    Ok(a) => {
                        let _ = advice.push(a);
                    }
                    Err(e) => {
                        failures.fetch_add(1, Ordering::Relaxed);
                        if !warned {
                            tracing::warn!(
                                "{} oracle: {e} (falling back to the local oracle)",
                                oracle.name()
                            );
                            warned = true;
                        }
                    }
                }
            }
        }
    });
    Ok(handle)
}
