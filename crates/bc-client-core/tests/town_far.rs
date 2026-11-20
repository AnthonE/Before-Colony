//! The town's far tiers are built with nothing allocated past what's reserved for them up front
//! (`town_mesh::CellBuild::range`, `Detail::caps`): a frame's budget can stop between any two parts,
//! and the parts themselves add only to buffers already there.

use bc_alloc::{CountingAlloc, count};
use bc_client_core::town_mesh::{CellBuild, Detail};
use bc_sim::colony::town::{CELLS, CELLS_PER_DISTRICT};

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

#[test]
fn the_far_pieces_build_without_allocating() {
    let pieces = (0..CELLS)
        .step_by(usize::from(CELLS_PER_DISTRICT))
        .map(|d| (d, CELLS_PER_DISTRICT, Detail::Haze))
        .chain((0..CELLS).step_by(4).map(|q| (q, 4, Detail::Far)))
        .chain([(37, 1, Detail::Far), (37, 1, Detail::Haze), (143, 1, Detail::Far)]);
    for (first, n, detail) in pieces {
        let mut build = CellBuild::range(first, n, detail);
        let ((), allocs) = count(|| {
            while !build.done() {
                build.step();
            }
        });
        assert_eq!(allocs, 0, "{n} cells from {first} at {detail:?}");
        assert!(build.finish().triangles() > 100);
    }
}
