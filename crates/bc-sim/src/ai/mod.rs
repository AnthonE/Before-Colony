//! Drone AI: the reflex layer (every tick, deterministic, allocation-free).
//!
//! Drones lead their targets perfectly *linearly*, feel no G-forces and never flinch, but they
//! are predictable: a pilot who keeps changing acceleration will out-juke them. They drive their
//! suits by emitting the same [`InputCmd`](bc_proto::InputCmd) a human sends. A suit on the ground
//! they come at from above, not through the body it stands on.

pub mod drone;
pub mod kit;

pub use drone::{Action, AiState, DRONE, DroneProfile, LEASH, PILOT, SEIZED, drive, think};
