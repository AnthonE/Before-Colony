//! Per-client replication state. All arrays are sized at construction.

use bc_proto::{InputCmd, PilotKind};
use bc_sim::SuitId;
use bc_sim::chunks::MAX_CHUNKS;
use bc_sim::storage::{BitSet, boxed};

use crate::jitter::JitterBuffer;

/// Snapshots remembered for ack processing.
pub(crate) const SENT_RING: usize = 64;
/// Pending "left your sensors" notices per client.
pub(crate) const LEAVE_QUEUE: usize = 64;
/// Most objects and rocks one snapshot carries (its record remembers them for the ack).
pub(crate) const OBJS_PER_SNAPSHOT: usize = 24;
pub(crate) const ROCKS_PER_SNAPSHOT: usize = 16;
/// A client holds nothing of a chunk.
pub(crate) const NONE: u16 = u16::MAX;

/// What a client holds of a chunk: its generation and version.
pub(crate) fn packed(generation: u8, version: u8) -> u16 {
    u16::from(generation & 0x7F) << 8 | u16::from(version)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SentRecord {
    pub tick: u32,
    /// Every event with a lower sequence was delivered in (or irrelevant to) this snapshot.
    pub events_done: u32,
    /// How many queued Leave notices it carried (from the front of the queue).
    pub leaves: u8,
    /// Objects it carried: (chunk, what the client holds of it once it has this snapshot).
    pub objs: [(u16, u16); OBJS_PER_SNAPSHOT],
    pub n_objs: u8,
    /// Rocks it carried: (rock, version).
    pub rocks: [(u16, u8); ROCKS_PER_SNAPSHOT],
    pub n_rocks: u8,
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
    /// Per chunk: what the client has acked of it (`packed`, or [`NONE`]).
    pub obj_acked: Box<[u16]>,
    /// How many chunks it holds something of.
    pub obj_known: usize,
    /// Per rock: the version the client has acked (0: as generated).
    pub rock_acked: Box<[u8]>,
}

impl ClientState {
    pub fn new(max_suits: usize, rocks: usize) -> Self {
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
            obj_acked: boxed(MAX_CHUNKS, NONE),
            obj_known: 0,
            rock_acked: boxed(rocks, 0u8),
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
        self.obj_acked.fill(NONE);
        self.obj_known = 0;
        self.rock_acked.fill(0);
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
        for &(id, state) in &rec.objs[..rec.n_objs as usize] {
            let had = &mut self.obj_acked[id as usize];
            match (*had == NONE, state == NONE) {
                (true, false) => self.obj_known += 1,
                (false, true) => self.obj_known -= 1,
                _ => {}
            }
            *had = state;
        }
        for &(id, version) in &rec.rocks[..rec.n_rocks as usize] {
            self.rock_acked[id as usize] = version;
        }
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
