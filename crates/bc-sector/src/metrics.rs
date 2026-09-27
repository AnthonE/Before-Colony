//! Sector metrics: relaxed atomics written by the sector thread, read by `/status`.

use std::sync::atomic::{AtomicU64, Ordering};

/// Tick durations in log2(µs) buckets.
const BUCKETS: usize = 32;

#[derive(Default)]
pub struct PilotStats {
    pub shots: AtomicU64,
    pub hits: AtomicU64,
    pub kills: AtomicU64,
    pub deaths: AtomicU64,
    /// The kit at work: hits by weapon class (beam, ballistic, missile, melee, cone), specials
    /// used, missiles launched, and the frame flown (`FrameId` as a number).
    pub hits_by_class: [AtomicU64; 5],
    pub specials: AtomicU64,
    pub missiles: AtomicU64,
    pub frame: AtomicU64,
    /// Suit entity slot + 1 (0 = none).
    pub suit: AtomicU64,
}

pub struct Metrics {
    pub ticks: AtomicU64,
    pub overruns: AtomicU64,
    tick_hist: [AtomicU64; BUCKETS],
    pub tick_last_us: AtomicU64,
    pub tick_max_us: AtomicU64,
    pub snapshots: AtomicU64,
    pub snapshot_bytes: AtomicU64,
    pub snapshot_max_bytes: AtomicU64,
    pub out_drops: AtomicU64,
    pub inputs: AtomicU64,
    pub inputs_stale: AtomicU64,
    pub inputs_missing: AtomicU64,
    pub clients: AtomicU64,
    pub suits_alive: AtomicU64,
    pub projectiles: AtomicU64,
    pub events: AtomicU64,
    pub pictures: AtomicU64,
    pub pictures_dropped: AtomicU64,
    pub advice: AtomicU64,
    pub pilots: Box<[PilotStats]>,
}

impl Metrics {
    /// Startup only.
    pub fn new(max_clients: usize) -> Self {
        Self {
            ticks: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
            tick_hist: Default::default(),
            tick_last_us: AtomicU64::new(0),
            tick_max_us: AtomicU64::new(0),
            snapshots: AtomicU64::new(0),
            snapshot_bytes: AtomicU64::new(0),
            snapshot_max_bytes: AtomicU64::new(0),
            out_drops: AtomicU64::new(0),
            inputs: AtomicU64::new(0),
            inputs_stale: AtomicU64::new(0),
            inputs_missing: AtomicU64::new(0),
            clients: AtomicU64::new(0),
            suits_alive: AtomicU64::new(0),
            projectiles: AtomicU64::new(0),
            events: AtomicU64::new(0),
            pictures: AtomicU64::new(0),
            pictures_dropped: AtomicU64::new(0),
            advice: AtomicU64::new(0),
            pilots: (0..max_clients).map(|_| PilotStats::default()).collect(),
        }
    }

    #[inline]
    pub fn add(c: &AtomicU64, n: u64) {
        c.fetch_add(n, Ordering::Relaxed);
    }

    #[inline]
    pub fn set(c: &AtomicU64, v: u64) {
        c.store(v, Ordering::Relaxed);
    }

    #[inline]
    pub fn max(c: &AtomicU64, v: u64) {
        c.fetch_max(v, Ordering::Relaxed);
    }

    pub fn record_tick(&self, us: u64) {
        let b = (64 - us.max(1).leading_zeros() as usize - 1).min(BUCKETS - 1);
        self.tick_hist[b].fetch_add(1, Ordering::Relaxed);
        Self::add(&self.ticks, 1);
        Self::set(&self.tick_last_us, us);
        Self::max(&self.tick_max_us, us);
    }

    /// Upper bound of the bucket holding quantile `q` of tick durations, µs.
    pub fn tick_quantile_us(&self, q: f64) -> u64 {
        let counts: Vec<u64> = self.tick_hist.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return 0;
        }
        let target = (total as f64 * q).ceil() as u64;
        let mut acc = 0;
        for (b, c) in counts.iter().enumerate() {
            acc += c;
            if acc >= target {
                return 1u64 << (b + 1);
            }
        }
        1u64 << BUCKETS
    }

    pub fn load(c: &AtomicU64) -> u64 {
        c.load(Ordering::Relaxed)
    }
}
