//! Simulation constants and per-sector configuration.

/// Fixed simulation rate. Everything is specified in seconds and converted with [`DT`].
pub const TICK_HZ: u32 = 30;
/// Seconds per tick.
pub const DT: f32 = 1.0 / TICK_HZ as f32;
/// Standard gravity, for G-loads and specific impulse.
pub const G0: f32 = 9.806_65;
/// Lag compensation never rewinds further than this (≈267 ms).
pub const MAX_REWIND_TICKS: u32 = 8;
/// Ticks of hitbox history kept for lag compensation and motion estimates.
pub const HISTORY_TICKS: usize = 16;
/// Anything this close is visible regardless of sensors (eyeball range, metres).
pub const VISUAL_RANGE: f32 = 1_500.0;
/// Suits are kept this far inside the ±32.768 km sector box.
pub const SECTOR_LIMIT: f32 = 30_000.0;

/// Converts seconds to whole ticks (rounded).
pub const fn secs(s: f32) -> u32 {
    (s * TICK_HZ as f32 + 0.5) as u32
}

/// Per-sector knobs, fixed at construction.
#[derive(Clone, Copy, Debug)]
pub struct SimConfig {
    /// Suit slots (players, agents and Mobile Dolls). ≤ 1023 (wire slot limit).
    pub max_suits: usize,
    pub max_projectiles: usize,
    /// Event ring capacity (events are kept ~1 s for redundant delivery).
    pub max_events: usize,
    /// Mobile Dolls the spawner keeps in the sector.
    pub target_dolls: u32,
    /// Deterministic seed.
    pub seed: u64,
    /// Seconds a destroyed human/agent suit waits before respawning.
    pub respawn_secs: f32,
    /// Dolls re-plan every N ticks (staggered by slot).
    pub doll_think_interval: u32,
    /// ZERO rollouts run every N ticks per pilot (staggered).
    pub zero_interval: u32,
    pub friendly_fire: bool,
    /// Test hook: the ZERO System is available on every frame.
    pub zero_on_all_frames: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_suits: 512,
            max_projectiles: 4096,
            max_events: 4096,
            target_dolls: 24,
            seed: 0xBC_0195,
            respawn_secs: 5.0,
            doll_think_interval: 3,
            zero_interval: 3,
            friendly_fire: false,
            zero_on_all_frames: false,
        }
    }
}
