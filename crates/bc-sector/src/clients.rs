//! Per-client replication state. All arrays are sized at construction.

use bc_proto::{InputCmd, PilotKind};
use bc_sim::SuitId;
use bc_sim::storage::{BitSet, boxed};

use crate::jitter::JitterBuffer;

/// Snapshots remembered for ack processing.
pub(crate) const SENT_RING: usize = 64;
/// Pending "left your sensors" notices per client.
pub(crate) const LEAVE_QUEUE: usize = 64;

#[derive(Clone, Copy, Default)]
pub(crate) struct SentRecord {
    pub tick: u32,
    /// Every event with a lower sequence was delivered in (or irrelevant to) this snapshot.
    pub events_done: u32,
    /// How many queued Leave notices it carried (from the front of the queue).
    pub leaves: u8,
}

pub(crate) struct ClientState {
    pub active: bool,
    pub suit: SuitId,
    pub pilot: PilotKind,
    pub max_datagram: usize,
    pub jitter: JitterBuffer,
    pub last_cmd: InputCmd,
    /// Consecutive ticks without a fresh command from the client.
    pub missing: u32,
    /// Tick of the last command that really came from the client (`u32::MAX` = none yet).
    pub last_real: u32,
    /// Newest snapshot tick the client says it received.
    pub ack_snapshot: u32,
    pub time_echo_ms: u16,
    pub time_echo_recv_us: u64,
    /// Events with a lower sequence number are delivered.
    pub event_acked: u32,
    pub sent: [SentRecord; SENT_RING],
    pub leaves: [(u16, u32); LEAVE_QUEUE],
    pub n_leaves: usize,
    /// Entities this client has been told about (and not told to forget).
    pub known: BitSet,
    /// Priority accumulator per entity.
    pub prio: Box<[f32]>,
    pub next_picture: u32,
}

impl ClientState {
    pub fn new(max_suits: usize) -> Self {
        Self {
            active: false,
            suit: SuitId::NONE,
            pilot: PilotKind::Human,
            max_datagram: bc_proto::MAX_DATAGRAM,
            jitter: JitterBuffer::default(),
            last_cmd: InputCmd::default(),
            missing: 0,
            last_real: u32::MAX,
            ack_snapshot: 0,
            time_echo_ms: 0,
            time_echo_recv_us: 0,
            event_acked: 0,
            sent: [SentRecord::default(); SENT_RING],
            leaves: [(0, 0); LEAVE_QUEUE],
            n_leaves: 0,
            known: BitSet::new(max_suits),
            prio: boxed(max_suits, 0.0f32),
            next_picture: 0,
        }
    }

    /// Seats a new session in this slot (no allocation: everything is reset in place).
    pub fn seat(&mut self, suit: SuitId, pilot: PilotKind, max_datagram: usize, event_seq: u32) {
        self.active = true;
        self.suit = suit;
        self.pilot = pilot;
        self.max_datagram = max_datagram.clamp(256, bc_proto::MAX_DATAGRAM);
        self.jitter.clear();
        self.last_cmd = InputCmd::default();
        self.missing = 0;
        self.last_real = u32::MAX;
        self.ack_snapshot = 0;
        self.time_echo_ms = 0;
        self.time_echo_recv_us = 0;
        self.event_acked = event_seq;
        self.sent = [SentRecord::default(); SENT_RING];
        self.n_leaves = 0;
        self.known.clear();
        self.prio.fill(0.0);
        self.next_picture = 0;
    }

    pub fn queue_leave(&mut self, slot: u16, tick: u32) {
        if self.n_leaves < LEAVE_QUEUE {
            self.leaves[self.n_leaves] = (slot, tick);
            self.n_leaves += 1;
        }
    }

    /// The client received snapshot `tick`: retire what it carried.
    pub fn on_ack(&mut self, tick: u32) {
        if tick <= self.ack_snapshot && self.ack_snapshot != 0 {
            return;
        }
        self.ack_snapshot = tick;
        let rec = self.sent[tick as usize % SENT_RING];
        if rec.tick != tick {
            return;
        }
        self.event_acked = self.event_acked.max(rec.events_done);
        let k = (rec.leaves as usize).min(self.n_leaves);
        if k > 0 {
            self.leaves.copy_within(k..self.n_leaves, 0);
            self.n_leaves -= k;
            // Later records counted leaves from the old queue front: shift them down.
            for r in self.sent.iter_mut() {
                if r.tick > tick {
                    r.leaves = r.leaves.saturating_sub(k as u8);
                }
            }
        }
    }
}
