//! The sector thread: fixed 30 Hz pacing (sleep, then spin to the deadline), or in step with another
//! sector's tick (`spawn_follower`); metrics; the egress wake.

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
    /// The OS thread it runs on (to wake it: [`spawn_waking`]).
    pub fn thread(&self) -> Thread {
        self.handle.as_ref().expect("running until stopped").thread().clone()
    }

    /// Signals the thread to stop and waits for it.
    pub fn stop(mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Runs `sector` on its own thread (`sector-0`), waking `egress` after every tick.
pub fn spawn(sector: Sector, egress: Option<Thread>) -> std::io::Result<SectorThread> {
    spawn_named(sector, egress, "sector-0")
}

/// Runs `sector` on a thread called `name`, waking `egress` after every tick.
#[allow(clippy::disallowed_methods)]
pub fn spawn_named(sector: Sector, egress: Option<Thread>, name: &str) -> std::io::Result<SectorThread> {
    spawn_waking(sector, egress.into_iter().collect(), name)
}

/// Runs `sector` on a thread called `name`, paced at the tick rate, waking every thread of `wake`
/// after every tick: its egress thread, and any sector that keeps in step with it
/// ([`spawn_follower`]). A futex wake each, never a lock.
#[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
pub fn spawn_waking(mut sector: Sector, wake: Vec<Thread>, name: &str) -> std::io::Result<SectorThread> {
    let shared = sector.shared().clone();
    let hot = sector_hot_guard(&sector);
    let handle = thread::Builder::new().name(name.into()).spawn(move || {
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
            for t in &wake {
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

/// Runs `sector` on a thread called `name` in step with another sector's tick (the colony's inside
/// keeps space's, so the colony has one clock: its day, its trams and its people's poses are the
/// same moment in both): it ticks each time `leader` has finished one, right after it, and never
/// gets ahead of it. Behind (it stalled, or it started late), it catches up a tick at a time. The
/// leader's thread wakes it ([`spawn_waking`]); it waits no longer than a tick in any case.
#[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
pub fn spawn_follower(
    mut sector: Sector,
    egress: Option<Thread>,
    name: &str,
    leader: Arc<SectorShared>,
) -> std::io::Result<SectorThread> {
    let shared = sector.shared().clone();
    let hot = sector_hot_guard(&sector);
    let handle = thread::Builder::new().name(name.into()).spawn(move || {
        let period = Duration::from_secs_f64(1.0 / f64::from(bc_sim::TICK_HZ));
        let shared = sector.shared().clone();
        let m = &shared.metrics;
        while !shared.stop.load(Ordering::Acquire) {
            if sector.sim.tick() >= leader.tick.load(Ordering::Acquire) {
                thread::park_timeout(period);
                continue;
            }
            let start = Instant::now();
            if let Some(g) = hot {
                g(true);
            }
            sector.tick();
            if let Some(g) = hot {
                g(false);
            }
            m.record_tick(start.elapsed().as_micros() as u64);
            shared.tick.store(sector.sim.tick(), Ordering::Release);
            if let Some(t) = &egress {
                t.unpark();
            }
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
