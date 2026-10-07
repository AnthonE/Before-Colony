//! Specials charged by the fight, after Titanfall's Core (`docs/PEERS.md`, "The mech games"; the
//! simulation's side is `sim::specials`): a special with a cooldown (Full Open Attack, the Cross
//! Crusher) charges back by itself over it, and faster from the fight. Every armour point the
//! suit's blows take off a hostile suit, and every point it takes, comes off what's left. The
//! special's own blows charge nothing, and Full Open charges nothing until its lockout is over.
//!
//! The pilot's client hears of the fight's share from its own state (it can't foresee a hit), so
//! its prediction of a special a blow made ready lags by a snapshot, as it does for the blow.

/// Armour points dealt that charge a special from empty...
pub const DEALT_FULL: f32 = 450.0;
/// ...and taken (a suit under fire is owed its answer sooner).
pub const TAKEN_FULL: f32 = 300.0;
