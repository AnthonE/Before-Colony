//! Before Colony's economy: everything a pilot owns, makes and trades, off the simulation's hot
//! path.
//!
//! - [`item`]: ores, materials, mobile-suit parts for each frame line, weapons.
//! - [`catalogue`]: what everything is made of, how long it takes, and what the colony pays.
//! - [`stores`]: a hangar's stock, and its parts one by one with their condition.
//! - [`faults`]: what's damaged or failed inside a suit's parts, and what restoring it takes.
//! - [`suit`]: the suit standing in the bay, what it launches with and what it comes home as.
//! - [`wear`]: thruster hours, barrel wear and reactor cycles wearing its systems down.
//! - [`fab`]: the fabricator's and the zero-G foundry's job queues, on the wall clock.
//! - [`exchange`]: the Colony Exchange, pilots' order books with the colony as a trader whose
//!   prices follow its stock.
//! - [`charter`]: the Charter Board: contracts with their rewards in escrow, and the colony's
//!   great works.
//! - [`hangar`]: a pilot's hangar, and every change they can ask of it.
//! - [`wire`]: the hangar's messages (JSON on the control stream).
//!
//! The server holds the truth (each pilot's [`hangar::Hangar`] in their record, one
//! [`exchange::Exchange`] per colony); clients get views of it and send requests, and use the
//! same catalogue to show what can be made.

pub mod catalogue;
pub mod charter;
pub mod exchange;
pub mod fab;
pub mod faults;
pub mod hangar;
pub mod item;
pub mod stores;
pub mod suit;
pub mod wear;
pub mod wire;

pub use catalogue::{Recipe, Station, recipe, recipes};
pub use charter::{Board, Work};
pub use exchange::{Exchange, Side};
pub use faults::Faults;
pub use hangar::{Bay, Hangar, Rules};
pub use item::{Item, Material, Ore};
pub use stores::{PartUnit, Stores};
pub use suit::{Slot, Suit};
pub use wire::{Place, Request, Update};
