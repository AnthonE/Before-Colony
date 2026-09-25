//! Before Colony simulation core.
//!
//! `#![no_std]`. The server runs it authoritatively and the browser runs the same code to predict
//! its own suit (and to draw ZERO's predicted futures). After [`Sim::new`] nothing allocates: storage
//! is fixed-capacity structure-of-arrays, and `storage.rs` is the only file that touches the heap.

#![no_std]

extern crate alloc;

pub mod ai;
pub mod collide;
pub mod config;
pub mod content;
pub mod events;
pub mod flight;
pub mod handle;
pub mod hash;
pub mod lagcomp;
pub mod math;
pub mod perception;
pub mod projectiles;
pub mod sensors;
pub mod sim;
pub mod spatial;
pub mod storage;
pub mod suits;
pub mod world;
pub mod zero;

pub use config::{DT, SimConfig, TICK_HZ};
pub use handle::SuitId;
pub use sim::Sim;
