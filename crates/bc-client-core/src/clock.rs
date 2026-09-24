//! Server-time estimation and input lead.
//!
//! - `server_now(now)` is the server's simulation time in ticks (continuous): snapshot `T` is
//!   produced as tick `T` completes, so on arrival the server is about half an RTT past `T`.
//! - Commands for tick `k` must reach the server before it simulates `k`. The client runs its
//!   input clock `lead` ticks ahead of `server_now`, steering `lead` so the server reports
//!   ~2 ticks of buffered input (`input_health`).
//! - Remote entities are drawn `interp_delay` ticks in the past, between two received snapshots.

use bc_sim::TICK_HZ;

const HZ: f64 = TICK_HZ as f64;
/// Target input-buffer depth at the server, ticks.
pub const TARGET_HEALTH: f64 = 2.0;

#[derive(Clone, Debug)]
pub struct Clock {
    offset: Option<f64>,
    /// Smoothed round-trip time, s.
    pub rtt: f64,
    /// Input clock lead over server time, ticks.
    pub lead: f64,
    /// Remote entities are rendered this many ticks behind server time.
    pub interp_delay: f64,
    /// Latest input-buffer depth reported by the server.
    pub health: i8,
}

impl Default for Clock {
    fn default() -> Self {
        Self { offset: None, rtt: 0.1, lead: 4.0, interp_delay: 3.0, health: 0 }
    }
}

impl Clock {
    pub fn synced(&self) -> bool {
        self.offset.is_some()
    }

    /// Server time (ticks) at local time `now` (s).
    pub fn server_now(&self, now: f64) -> f64 {
        now * HZ + self.offset.unwrap_or(0.0)
    }

    /// Tick the next input should target.
    pub fn input_tick(&self, now: f64) -> u32 {
        (self.server_now(now) + self.lead).floor().max(0.0) as u32
    }

    /// Time (ticks) at which remote entities are rendered, and shots are lag-compensated against.
    pub fn view_tick(&self, now: f64) -> f64 {
        self.server_now(now) - self.interp_delay
    }

    /// Feeds one snapshot: its tick, arrival time, and optional RTT sample (s).
    pub fn on_snapshot(&mut self, tick: u32, recv_now: f64, rtt_sample: Option<f64>, health: i8) {
        if let Some(r) = rtt_sample.filter(|r| r.is_finite() && *r >= 0.0 && *r < 5.0) {
            self.rtt = if self.offset.is_none() { r } else { self.rtt * 0.9 + r * 0.1 };
        }
        let sample = f64::from(tick) + self.rtt * 0.5 * HZ - recv_now * HZ;
        self.offset = Some(match self.offset {
            None => sample,
            // Large disagreement (first sync, server hitch): snap. Otherwise track gently; late
            // snapshots pull the estimate back less than early ones push it forward.
            Some(o) if (sample - o).abs() > 6.0 => sample,
            Some(o) if sample > o => o + 0.1 * (sample - o),
            Some(o) => o + 0.02 * (sample - o),
        });
        self.health = health;
        let base = self.rtt * 0.5 * HZ;
        self.lead = (self.lead + 0.05 * (TARGET_HEALTH - f64::from(health))).clamp(base + 0.5, base + 12.0);
        // More jitter → more interpolation delay (2–6 ticks).
        self.interp_delay = (2.0 + self.rtt * HZ * 0.25).clamp(2.0, 6.0);
    }
}
