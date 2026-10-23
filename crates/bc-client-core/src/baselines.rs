//! The suits' records the client took, kept so that a delta against one of them can be undone
//! (`bc_proto::entity`, `docs/NETCODE.md`).

use bc_proto::QEntity;
use bc_proto::entity::KEPT;

/// The newest [`KEPT`] records of a suit taken, with their ticks.
#[derive(Clone, Copy)]
struct Ring {
    at: [(u32, QEntity); KEPT],
    /// Where the next goes, and how many are held.
    head: usize,
    n: usize,
}

/// Every suit's kept records, by slot. A suit's are kept whatever becomes of it (out of sight, a
/// new life in its slot): the server names them by slot and tick, and only ones it knows the client
/// took. A new stream (another seat, another sector) starts them all over.
#[derive(Default)]
pub struct Baselines {
    slots: Vec<Option<Box<Ring>>>,
}

impl Baselines {
    /// Forgets everything (a new stream).
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// The record of `slot` taken at `tick`, if it's kept.
    pub fn get(&self, slot: u16, tick: u32) -> Option<QEntity> {
        let ring = self.slots.get(usize::from(slot))?.as_ref()?;
        (0..ring.n)
            .map(|k| ring.at[(ring.head + KEPT - 1 - k) % KEPT])
            .find(|&(t, _)| t == tick)
            .map(|(_, q)| q)
    }

    /// Keeps a record taken at `tick` (a datagram's records go in once it's taken whole).
    pub fn put(&mut self, tick: u32, q: QEntity) {
        let slot = usize::from(q.slot);
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        let ring = self.slots[slot]
            .get_or_insert_with(|| Box::new(Ring { at: [(0, QEntity::default()); KEPT], head: 0, n: 0 }));
        ring.at[ring.head] = (tick, q);
        ring.head = (ring.head + 1) % KEPT;
        ring.n = (ring.n + 1).min(KEPT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_of_each_suit() {
        let mut b = Baselines::default();
        let q = |slot| QEntity { slot, ..QEntity::default() };
        for t in 0..40 {
            b.put(t, QEntity { flags: t as u16, ..q(7) });
        }
        b.put(5, q(900));
        assert_eq!(b.get(7, 39).map(|e| e.flags), Some(39));
        assert_eq!(b.get(7, 40 - KEPT as u32).map(|e| e.flags), Some(40 - KEPT as u16));
        assert_eq!(b.get(7, 39 - KEPT as u32), None);
        assert_eq!(b.get(900, 5), Some(q(900)));
        assert_eq!(b.get(901, 5), None);
        b.clear();
        assert_eq!(b.get(7, 39), None);
    }
}
