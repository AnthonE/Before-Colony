//! The colony as the people inside it know it, in closed forms everyone evaluates alike: its frames
//! (`frame`: the strips, and a point on the floor in city coordinates), its day (`time`: the
//! mirrors open and close on the tick's clock), its mirrors (`mirrors`), the docking hub's
//! structures (`hub`), its city (`city`: streets, blocks and buildings, worked out where they're
//! asked for), its street furniture (`furniture`: lamps, trees, benches; solid to people, not to
//! suits), its trams (`transit`), its traffic (`traffic`: the cars on its streets and their signals)
//! and the people on its streets (`walkers`), on the tick's clock; and the Proving Ground's course
//! through its air (`course`) and its Blast Hall's live fire (`hall`).
//!
//! To suits and shots the colony is still `world`'s solid cylinder; this is the rest of it. Like
//! the bodies, nothing here allocates or reads a clock of its own, and every function is the same
//! to the bit on the server and in the browser (libm through `math`).

pub mod city;
pub mod course;
pub mod frame;
pub mod furniture;
pub mod hall;
pub mod hub;
pub mod interior;
pub mod mirrors;
pub mod pools;
pub mod time;
pub mod traffic;
pub mod transit;
pub mod walkers;
