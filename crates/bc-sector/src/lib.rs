//! Before Colony sector runtime: the hot loop.
//!
//! One OS thread per sector owns a [`bc_sim::Sim`] and ticks it at 30 Hz. It talks to the network
//! side only through preallocated lock-free queues:
//!
//! - inputs: one `rtrb` SPSC ring per client slot. The producer end travels inside a [`SlotLease`]
//!   that the session task holds.
//! - control (join/leave/respawn): a crossbeam `ArrayQueue`.
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
    Control, EgressEnds, InputMsg, OracleEnds, SectorShared, SlotLease, SlotState, build, read_packet,
};
pub use runtime::{SectorThread, spawn};
pub use sector::{Sector, SectorConfig};
