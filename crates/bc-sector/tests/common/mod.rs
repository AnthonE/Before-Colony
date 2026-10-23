//! What the tests (and the bench) share: a client's acks, as `bc-client-core` keeps them.
#![allow(dead_code)]

use bc_proto::ack::AckWindow;
use bc_proto::{InputPacket, SnapshotReader};

/// Each slot's snapshot datagrams taken, acked in its input packets as a client acks them.
pub struct Acks(Vec<AckWindow>);

impl Acks {
    pub fn new(slots: usize) -> Self {
        Self(vec![AckWindow::default(); slots])
    }

    /// Slot `slot` received `datagram`: taken if it's news (`AckWindow::check`). Returns whether
    /// it was.
    pub fn take(&mut self, slot: usize, datagram: &[u8]) -> bool {
        let Ok(r) = SnapshotReader::new(datagram) else { return false };
        let h = *r.header();
        let a = &mut self.0[slot];
        let v = a.check(h.stream, h.seq, h.tick);
        a.commit(h.stream, v, h.tick);
        v != bc_proto::ack::Verdict::Stale
    }

    /// Slot `slot`'s acks, into its next input packet.
    pub fn stamp(&self, slot: usize, p: &mut InputPacket) {
        (p.ack_stream, p.ack_seq, p.ack_bits) = self.0[slot].ack();
    }
}
