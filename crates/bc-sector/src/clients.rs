//! Per-client replication state. All arrays are sized at construction.

use bc_proto::entity::KEPT;
use bc_proto::{InputCmd, PilotKind, QEntity};
use bc_sim::SuitId;
use bc_sim::chunks::MAX_CHUNKS;
use bc_sim::colony::course::Run;
use bc_sim::colony::hall::Drill;
use bc_sim::storage::{BitSet, boxed};
use glam::Vec3;

use crate::jitter::JitterBuffer;

/// Snapshots remembered for ack processing.
pub(crate) const SENT_RING: usize = 64;
/// Pending "left your sensors" notices per client.
pub(crate) const LEAVE_QUEUE: usize = 64;
/// Most objects and rocks one snapshot carries (its record remembers them for the ack).
pub(crate) const OBJS_PER_SNAPSHOT: usize = 24;
pub(crate) const ROCKS_PER_SNAPSHOT: usize = 16;
/// Suit records the datagrams in flight carried, remembered for their acks (a ring).
pub(crate) const SENT_ENTS: usize = 4_096;
/// A client holds nothing of a chunk.
pub(crate) const NONE: u16 = u16::MAX;
/// How long a spectator is told a suit left their view, ticks (it acks nothing; its client also
/// forgets a suit it stops hearing of).
pub(crate) const WATCH_LEAVE_TICKS: u32 = 15;

/// What a client holds of a chunk: its generation and version.
pub(crate) fn packed(generation: u8, version: u8) -> u16 {
    u16::from(generation & 0x7F) << 8 | u16::from(version)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SentRecord {
    /// The datagram's number (`ClientState::seq` when it went).
    pub seq: u32,
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
    /// The suit records it carried: `n_ents` from `ents_from` in [`ClientState::sent_ents`].
    pub ents_from: u32,
    pub n_ents: u16,
}

/// A suit's record that went to the client: of which life (the suit's full generation), which of
/// the suit's records it was ([`ClientState::sends`] when it went), at which tick, and its codes.
#[derive(Clone, Copy)]
pub(crate) struct SentEnt {
    pub generation: u16,
    pub nth: u32,
    pub tick: u32,
    pub q: QEntity,
}

/// The newest record of a suit the client acked, which its next records are deltas against.
#[derive(Clone, Copy, Default)]
pub(crate) struct Baseline {
    pub has: bool,
    pub generation: u16,
    pub nth: u32,
    pub tick: u32,
    pub q: QEntity,
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
    /// The stream this client's datagrams count on (`bc_proto::ack::stream`: the sector and the
    /// seat), the seat it names, and the number the next datagram takes.
    pub stream: u8,
    pub epoch: u8,
    pub seq: u32,
    /// Datagrams under this number have been acked or given up on (their records retired or not).
    pub ack_floor: u32,
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
    /// Its suit's loss has been reported (survival rules)...
    pub lost: bool,
    /// ...and how it was lost.
    pub loss: crate::queues::Loss,
    /// The bytes this client may still be sent (a bucket filled by `SectorConfig::rate` a tick, to
    /// twice that), and how many datagrams a tick at most.
    pub bucket: i32,
    pub max_parts: u8,
    /// Per suit slot: how many records of it went to this client, and the newest the client
    /// acked (its baseline). And the records the datagrams in flight carried, for their acks.
    pub sends: Box<[u32]>,
    pub base: Box<[Baseline]>,
    pub sent_ents: Box<[SentEnt]>,
    pub ents_head: u32,
    /// A spectator (`Control::Watch`): where they watch from, with no suit of their own (`active`
    /// stays false).
    pub watch: Option<Vec3>,
    /// Inside the colony, the Proving Ground (`docs/TRAINING.md`): the pilot's run of its course
    /// and their drill in the Blast Hall, kept by the sector to check their times.
    pub course: Run,
    pub drill: Drill,
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
            stream: 0,
            epoch: 0,
            seq: 0,
            ack_floor: 0,
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
            lost: false,
            loss: crate::queues::Loss::Destroyed,
            bucket: 0,
            max_parts: 1,
            sends: boxed(max_suits, 0u32),
            base: boxed(max_suits, Baseline::default()),
            sent_ents: boxed(SENT_ENTS, SentEnt { generation: 0, nth: 0, tick: 0, q: QEntity::default() }),
            ents_head: 0,
            watch: None,
            course: Run::default(),
            drill: Drill::default(),
        }
    }

    /// Seats a new session in this slot (no allocation: everything is reset in place).
    pub fn seat(&mut self, sector: u8, suit: SuitId, pilot: PilotKind, max_datagram: usize, event_seq: u32) {
        self.active = true;
        self.watch = None;
        self.suit = suit;
        self.pilot = pilot;
        self.max_datagram = max_datagram.clamp(256, bc_proto::MAX_DATAGRAM);
        self.jitter.clear();
        self.last_cmd = InputCmd::default();
        self.missing = 0;
        self.last_real = u32::MAX;
        // A new seat counts on a new stream: the client forgets what it held of the last.
        self.epoch = bc_proto::ack::next_epoch(self.epoch);
        self.stream = bc_proto::ack::stream(sector, self.epoch);
        self.seq = 0;
        self.ack_floor = 0;
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
        self.lost = false;
        self.loss = crate::queues::Loss::Destroyed;
        self.bucket = 0;
        self.max_parts = 1;
        self.sends.fill(0);
        self.base.fill(Baseline::default());
        self.ents_head = 0;
        self.course.clear();
        self.drill.reset();
    }

    /// Seats a spectator in this slot, watching from `at` (no allocation, as [`Self::seat`]).
    pub fn spectate(&mut self, sector: u8, at: Vec3, max_datagram: usize, event_seq: u32) {
        self.seat(sector, SuitId::NONE, PilotKind::Human, max_datagram, event_seq);
        self.active = false;
        self.watch = Some(at);
    }

    /// A spectator acks nothing (it sends no input): a "left your view" notice is repeated for
    /// [`WATCH_LEAVE_TICKS`] and then dropped.
    pub fn retire_watched_leaves(&mut self, tick: u32) {
        let mut k = 0;
        for i in 0..self.n_leaves {
            if tick.saturating_sub(self.leaves[i].1) < WATCH_LEAVE_TICKS {
                self.leaves[k] = self.leaves[i];
                k += 1;
            }
        }
        self.n_leaves = k;
    }

    pub fn queue_leave(&mut self, slot: u16, tick: u32) {
        if self.n_leaves < LEAVE_QUEUE {
            self.leaves[self.n_leaves] = (slot, tick);
            self.n_leaves += 1;
        }
    }

    /// The baseline record `q` of suit `j` (of life `generation`) can be sent as a delta against at
    /// `tick`, and how many ticks back it is: one the client acked, of this life, fewer than
    /// [`KEPT`] of the suit's records ago (so the client still holds it).
    pub fn baseline(&self, j: usize, generation: u16, q: &QEntity, tick: u32) -> Option<(&QEntity, u32)> {
        let b = &self.base[j];
        (b.has
            && b.generation == generation
            && self.sends[j] - b.nth < KEPT as u32
            && b.q.same_suit(q)
            && tick > b.tick)
            .then_some((&b.q, tick - b.tick))
    }

    /// A record of suit `j` went in this tick's datagram: remembered for its ack.
    pub fn note_sent(&mut self, j: usize, generation: u16, tick: u32, q: &QEntity) {
        let nth = self.sends[j];
        self.sends[j] += 1;
        self.sent_ents[self.ents_head as usize % SENT_ENTS] = SentEnt { generation, nth, tick, q: *q };
        self.ents_head = self.ents_head.wrapping_add(1);
    }

    /// The next datagram's number, and its stream.
    pub fn next_seq(&mut self) -> (u32, u8) {
        let s = self.seq;
        self.seq += 1;
        (s, self.stream)
    }

    /// An input packet's ack (`bc_proto::ack::AckWindow::ack`): retire what the datagrams it names
    /// carried, oldest first. The client takes a stream's datagrams in order, so none it acks is
    /// older than one it acked before: anything under the floor is done with.
    pub fn on_ack(&mut self, stream: u8, seq: u16, bits: u32) {
        if stream == 0 || stream != self.stream || self.seq == 0 {
            return;
        }
        let newest = bc_proto::ack::expand(seq, self.seq - 1);
        if newest >= self.seq {
            return;
        }
        for i in (0..32u32).rev() {
            if bits & (1 << i) != 0
                && let Some(s) = newest.checked_sub(1 + i)
            {
                self.retire(s);
            }
        }
        self.retire(newest);
    }

    /// Datagram `seq` arrived: what it carried is the client's.
    fn retire(&mut self, seq: u32) {
        if seq < self.ack_floor {
            return;
        }
        let rec = self.sent[seq as usize % SENT_RING];
        if rec.seq != seq {
            return;
        }
        self.ack_floor = seq + 1;
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
        // The suits' records it carried are the client's now: each its suit's baseline, if newer.
        if self.ents_head.wrapping_sub(rec.ents_from) <= SENT_ENTS as u32 {
            for k in 0..u32::from(rec.n_ents) {
                let e = self.sent_ents[(rec.ents_from + k) as usize % SENT_ENTS];
                let b = &mut self.base[usize::from(e.q.slot)];
                if !b.has || e.nth > b.nth {
                    *b = Baseline { has: true, generation: e.generation, nth: e.nth, tick: e.tick, q: e.q };
                }
            }
        }
        let k = (rec.leaves as usize).min(self.n_leaves);
        if k > 0 {
            self.leaves.copy_within(k..self.n_leaves, 0);
            self.n_leaves -= k;
            // Later records counted leaves from the old queue front: shift them down.
            for r in self.sent.iter_mut() {
                if r.seq > seq {
                    r.leaves = r.leaves.saturating_sub(k as u8);
                }
            }
        }
    }
}
