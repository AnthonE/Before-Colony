//! Simulation constants and per-sector configuration.

/// Fixed simulation rate. Everything is specified in seconds and converted with [`DT`].
pub const TICK_HZ: u32 = 30;
/// Seconds per tick.
pub const DT: f32 = 1.0 / TICK_HZ as f32;
/// Standard gravity, for G-loads and specific impulse.
pub const G0: f32 = 9.806_65;
/// Lag compensation never rewinds further than this (≈267 ms).
pub const MAX_REWIND_TICKS: u32 = 8;
/// How far behind the server's time a client draws everyone else (ticks), for a round trip of
/// `rtt` seconds: more jitter, more delay, from 2 to 6 ticks. The client's clock sets its
/// `interp_delay` with it, and the sector's rewind floor ([`rewind_floor_q4`]) expects it.
pub fn interp_delay_ticks(rtt: f64) -> f64 {
    (2.0 + rtt * TICK_HZ as f64 * 0.25).clamp(2.0, 6.0)
}
/// How much older than its link explains a command's view may be before the sector puts a floor
/// under it ([`rewind_floor_q4`]), in sixteenths of a tick: what an honest client's clock can put
/// between them, from the clock's own bounds (`bc_client_core::clock`, whose tests hold it to this):
/// - a tick: a command waits up to one for the tick it arrived in;
/// - 4 ticks (the clock's `EASE_SNAP`): the drawn view eases onto a corrected estimate rather than
///   jumping, and trails it by at most this before it jumps;
/// - ¾ of a tick (`LATE_STEP × LATE_RUN`): a burst of late snapshots (a stalled page) drags the
///   estimate back by at most 1/20 of a tick each, 15 of them, before it gives in.
///
/// Settled, the view trails its estimate by far less than those 4 ticks, which is what covers a
/// lost input packet (its command comes about a tick later, in the next) and jitter on the way up.
pub const REWIND_SLACK_Q4: u32 = 16 + 4 * 16 + 12;

/// The oldest view (sixteenths of a tick) the sector takes for a command that first arrived as
/// tick `arrived` was next, over a link whose round trip the transport measures at `rtt_us`
/// (µs): an honest client stamps its view `rtt/2 + interp` behind the server's time as it sends
/// ([`interp_delay_ticks`]), which the command reaches the server half a round trip later; a
/// claim further back than that and [`REWIND_SLACK_Q4`] is a pilot looking back at what it
/// never saw (backtracking). Rounded down: never a sixteenth stricter than the slack says.
///
/// [`MAX_REWIND_TICKS`] still caps every rewind; this is what lets that cap rise for pilots far
/// away without handing near ones a deeper look back than their link explains.
pub fn rewind_floor_q4(arrived: u32, rtt_us: u32) -> u32 {
    let rtt = f64::from(rtt_us) / 1e6;
    let back = rtt * 0.5 * TICK_HZ as f64 + interp_delay_ticks(rtt);
    let floor = f64::from(arrived) * 16.0 - back * 16.0 - f64::from(REWIND_SLACK_Q4);
    // Truncation is floor() for a positive value.
    if floor > 0.0 { floor as u32 } else { 0 }
}
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
    /// Suit slots (players, agents and drones). ≤ 1023 (wire slot limit).
    pub max_suits: usize,
    pub max_projectiles: usize,
    /// Event ring capacity (events are kept ~1 s for redundant delivery).
    pub max_events: usize,
    /// Drones the spawner keeps in the sector.
    pub target_drones: u32,
    /// Deterministic seed.
    pub seed: u64,
    /// Seconds a destroyed human/agent suit waits before respawning.
    pub respawn_secs: f32,
    /// Drones re-plan every N ticks (staggered by slot).
    pub drone_think_interval: u32,
    /// The Orrery rollouts run every N ticks per pilot (staggered).
    pub orrery_interval: u32,
    pub friendly_fire: bool,
    /// Test hook: the Orrery is available on every frame.
    pub orrery_on_all_frames: bool,
    /// The debris field: its seed and how many rocks (≤ `Field::MAX_ROCKS`; 0 for none). Clients
    /// are told both in their Welcome and build the identical field.
    pub field_seed: u32,
    pub field_rocks: u16,
    /// Landmarks in the sector: the first `landmarks` of `content::landmarks::LANDMARKS` (0 for
    /// none; at most `bodies::MAX_LANDMARKS`).
    pub landmarks: u8,
    /// The most suits asleep at once (offline pilots'); past it the longest asleep is cleared.
    pub max_sleepers: usize,
    /// Survival rules: pilots fly the suits they built (`Sim::launch`), bring them home
    /// (`Sim::dock`) and don't respawn; the dock neither buys nor refuels (the hangar does); the
    /// colony pays bounties for drones. Off: arcade rules (any frame, free respawns, sell
    /// and refuel at the dock).
    pub survival: bool,
    /// How pilots fly: the simulator's rocket equation, or anime rules (a boost gauge that fills
    /// back up). Clients are told in their Welcome, and predict by the same rules. Drones
    /// fly by the real rules either way.
    pub flight: crate::tuning::FlightRules,
    /// Space round the colony, or the colony's inside (`colony::interior`): its own frame, the
    /// spin's pull and the air, weapons safe by the colony's law. Clients are told in their
    /// Welcome. An interior has no drones, no field and no landmarks.
    pub world: crate::colony::interior::WorldKind,
    /// Ticks between the Almagest's aces (`content::aces`), one out at a time among the drones; 0: none.
    pub ace_every: u32,
    /// Who harms whom (`colony::reach`): by side, or pilots against pilots in open space past the
    /// colony's reach. Clients are told in their Welcome.
    pub law: crate::colony::reach::Law,
}

impl SimConfig {
    /// The sector's landmarks, by id: the first [`landmarks`](Self::landmarks) of those there are,
    /// and no more than a sector holds. Clients are told how many in their Welcome.
    pub fn landmark_defs(&self) -> &'static [crate::content::landmarks::LandmarkDef] {
        let all = &crate::content::landmarks::LANDMARKS;
        &all[..usize::from(self.landmarks).min(all.len()).min(crate::bodies::MAX_LANDMARKS)]
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_suits: 512,
            max_projectiles: 8192,
            max_events: 8192,
            target_drones: 24,
            seed: 0xBC_0195,
            respawn_secs: 5.0,
            drone_think_interval: 3,
            orrery_interval: 3,
            friendly_fire: false,
            orrery_on_all_frames: false,
            field_seed: crate::field::Field::DEFAULT_SEED,
            field_rocks: crate::field::Field::DEFAULT_ROCKS,
            landmarks: crate::content::landmarks::LANDMARKS.len() as u8,
            max_sleepers: 256,
            survival: false,
            flight: crate::tuning::FlightRules::Real,
            world: crate::colony::interior::WorldKind::Space,
            ace_every: crate::content::aces::ACE_EVERY,
            law: crate::colony::reach::Law::Sides,
        }
    }
}
