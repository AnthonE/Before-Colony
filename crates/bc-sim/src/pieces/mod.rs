//! Building pieces (`docs/RING.md`, "Buildings from pieces"): the lattice path of Gates' building
//! system (`AnthonE/Gates`, MIT), ported to the simulation's rules (nothing allocates, every number
//! the same on the server and in the browser). A building is pieces on a lattice of 3 m squares and
//! triangles, 3 m storeys: foundations, floors, roofs, walls, doorways, windows and flights.
//!
//! - [`lattice`]: exact points of the tiling, and exact overlap.
//! - [`sock`]: the footprint a piece takes on it.
//! - [`shape`]: the shapes, and the storeys' heights.
//! - [`support`]: what holds a piece up.
//! - [`check`]: whether a piece may stand where it's written.
//! - [`parts`]: what each piece is made of, as boxes: its solids, and what's drawn of it.
//! - [`prefab`]: buildings written as data, and where they stand on a deck.

pub mod check;
pub mod lattice;
pub mod parts;
pub mod prefab;
pub mod shape;
pub mod sock;
pub mod support;
