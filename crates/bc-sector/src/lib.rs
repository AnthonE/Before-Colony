//! Before Colony sector runtime: the hot loop.
//!
//! One OS thread per sector owns a [`bc_sim::Sim`] and ticks it at 30 Hz. It talks to the network
//! side only through preallocated lock-free queues:
//!
//! - inputs: one `rtrb` SPSC ring per client slot. The producer end travels inside a [`SlotLease`]
//!   that the session task holds.
//! - control (join/leave/sleep/respawn/dock): a crossbeam `ArrayQueue`; sleepers' fates go back
//!   the same way.
//! - reports (a suit docked, or was lost): one `rtrb` SPSC ring per slot, whose consumer end also
//!   travels in the [`SlotLease`].
//! - outbound packets: one `rtrb` byte ring per slot, drained by the egress thread. The sector
//!   `unpark()`s that thread once per tick (a futex wake; waking tokio would take a mutex).
//! - tactical pictures and advice: an SPSC ring each way, to the ZERO oracle worker.
//!
//! No tokio dependency, no locks, and no allocation after [`build`] (enforced by `clippy.toml`
//! and the `no_alloc_sector` test).

mod clients;
mod jitter;
pub mod metrics;
mod queues;
mod replicate;
mod runtime;
mod sector;

pub use jitter::JitterBuffer;
pub use metrics::Metrics;
pub use queues::{
    Comeback, Control, EgressEnds, InputMsg, NOTES, OracleEnds, Outcome, REPORTS, Report, SectorShared,
    SlotLease, SlotState, build, read_packet,
};
pub use runtime::{SectorThread, spawn};
pub use sector::{Sector, SectorConfig};
