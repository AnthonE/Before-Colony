//! Mobile Doll AI: the reflex layer (every tick, deterministic, allocation-free).
//!
//! Mobile Dolls lead their targets perfectly *linearly*, feel no G-forces and never flinch, but they
//! are predictable: a pilot who keeps changing acceleration will out-juke them. They drive their
//! suits by emitting the same [`InputCmd`](bc_proto::InputCmd) a human sends. A suit on the ground
//! they come at from above, not through the body it stands on.

pub mod kit;
pub mod mobile_doll;

pub use mobile_doll::{Action, AiState, DOLL, DollProfile, PILOT, SEIZED, drive, think};
