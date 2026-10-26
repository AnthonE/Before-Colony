//! The colony as the people inside it know it, in closed forms everyone evaluates alike: its day
//! (`time`: the louvres open and close on the tick's clock), its louvres (`louvres`), the docking
//! hub's structures (`hub`: the drum, the bays, the spine and the spokes); and the retired
//! cylinder's inside (`docs/RING.md`), kept until it's cleared: its frames (`frame`: the strips, and a
//! point on the floor in city coordinates), its city (`city`: streets, blocks and buildings, worked out where they're
//! asked for), its street furniture (`furniture`: lamps, trees, benches; solid to people, not to
//! suits), its trams (`transit`), its traffic (`traffic`: the cars on its streets and their signals)
//! and the people on its streets (`walkers`), on the tick's clock; the Proving Ground's course
//! through its air (`course`) and its Blast Hall's live fire (`hall`); and the homes inside its
//! buildings (`homes`: lobbies, stairs, corridors, flats and houses, each at its own address).
//!
//! To suits and shots the colony is `world`'s solid ring and the hub's landmark; this is the rest of
//! it. Like
//! the bodies, nothing here allocates or reads a clock of its own, and every function is the same
//! to the bit on the server and in the browser (libm through `math`).

pub mod city;
pub mod course;
pub mod frame;
pub mod furniture;
pub mod hall;
pub mod homes;
pub mod hub;
pub mod interior;
pub mod louvres;
pub mod pools;
pub mod time;
pub mod traffic;
pub mod transit;
pub mod walkers;
