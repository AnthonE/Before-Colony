//! The sector thread: fixed 30 Hz pacing (sleep, then spin to the deadline), metrics, egress wake.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use crate::Sector;
use crate::queues::SectorShared;

/// A running sector thread.
pub struct SectorThread {
    pub shared: Arc<SectorShared>,
    handle: Option<JoinHandle<()>>,
}

impl SectorThread {
    /// Signals the thread to stop and waits for it.
    pub fn stop(mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Runs `sector` on its own thread, waking `egress` after every tick.
#[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
pub fn spawn(mut sector: Sector, egress: Option<Thread>) -> std::io::Result<SectorThread> {
    let shared = sector.shared().clone();
    let hot = sector_hot_guard(&sector);
    let handle = thread::Builder::new().name("sector-0".into()).spawn(move || {
        let period = Duration::from_secs_f64(1.0 / f64::from(bc_sim::TICK_HZ));
        let spin_window = Duration::from_micros(1_500);
        let shared = sector.shared().clone();
        let m = &shared.metrics;
        let mut deadline = Instant::now() + period;
        while !shared.stop.load(Ordering::Acquire) {
            let start = Instant::now();
            if let Some(g) = hot {
                g(true);
            }
            sector.tick();
            if let Some(g) = hot {
                g(false);
            }
            let spent = start.elapsed();
            m.record_tick(spent.as_micros() as u64);
            shared.tick.store(sector.sim.tick(), Ordering::Release);
            if let Some(t) = &egress {
                t.unpark();
            }
            // Pace: sleep until shortly before the deadline, then spin.
            let now = Instant::now();
            if now > deadline + period * 3 {
                // Badly behind (debugger, suspend): resynchronize instead of bursting.
                crate::Metrics::add(&m.overruns, 1);
                deadline = now + period;
                continue;
            }
            if now > deadline {
                crate::Metrics::add(&m.overruns, 1);
            }
            if let Some(sleep) = deadline.checked_duration_since(now).and_then(|d| d.checked_sub(spin_window))
            {
                thread::sleep(sleep);
            }
            while Instant::now() < deadline {
                std::hint::spin_loop();
            }
            deadline += period;
        }
    })?;
    Ok(SectorThread { shared, handle: Some(handle) })
}

fn sector_hot_guard(sector: &Sector) -> Option<fn(bool)> {
    sector.hot_guard()
}

impl Sector {
    pub(crate) fn hot_guard(&self) -> Option<fn(bool)> {
        self.config().hot_guard
    }
}
