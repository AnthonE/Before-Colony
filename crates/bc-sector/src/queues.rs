//! The lock-free plumbing between the sector thread and the network side, allocated once.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use bc_proto::{Faction, FrameId, InputPacket, PilotKind};
use bc_sim::zero::{TacticalAdvice, TacticalPicture};
use crossbeam_queue::ArrayQueue;

use crate::metrics::Metrics;
use crate::sector::SectorConfig;

/// An input datagram, decoded on the network side.
#[derive(Clone, Copy, Debug)]
pub struct InputMsg {
    pub packet: InputPacket,
    /// Server clock (µs since sector start) when it arrived: lets the snapshot report how long the
    /// server held the client's RTT timestamp.
    pub recv_us: u64,
}

/// Network → sector commands.
#[derive(Clone, Copy, Debug)]
pub enum Control {
    Join { slot: u16, pilot: PilotKind, frame: FrameId, faction: Faction, max_datagram: u16 },
    Leave { slot: u16 },
    Respawn { slot: u16, frame: FrameId },
}

/// Lifecycle of a client slot, published by the sector through an atomic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SlotState {
    Free = 0,
    Active = 1,
    /// The sector could not seat the pilot (no suit slot left).
    Refused = 2,
}

/// Per-slot status visible to the network side.
pub struct SlotStatus {
    state: AtomicU32,
    /// Entity slot of the pilot's suit.
    suit: AtomicU32,
    /// Bumped every time the slot is (re)joined, so a waiter can tell its join apart from an
    /// earlier session's.
    epoch: AtomicU32,
}

impl SlotStatus {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(SlotState::Free as u32),
            suit: AtomicU32::new(u32::MAX),
            epoch: AtomicU32::new(0),
        }
    }

    pub fn state(&self) -> SlotState {
        match self.state.load(Ordering::Acquire) {
            1 => SlotState::Active,
            2 => SlotState::Refused,
            _ => SlotState::Free,
        }
    }

    pub fn suit(&self) -> Option<u16> {
        let s = self.suit.load(Ordering::Acquire);
        (s != u32::MAX).then_some(s as u16)
    }

    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(crate) fn publish(&self, state: SlotState, suit: Option<u16>) {
        self.suit.store(suit.map_or(u32::MAX, u32::from), Ordering::Release);
        if state != SlotState::Free {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.state.store(state as u32, Ordering::Release);
    }
}

/// The right to use one client slot: owned by exactly one session task at a time. Carries the
/// producer end of that slot's input ring.
pub struct SlotLease {
    pub slot: u16,
    pub input: rtrb::Producer<InputMsg>,
}

/// Shared between the sector thread and the network side.
pub struct SectorShared {
    pub control: ArrayQueue<Control>,
    /// Free slot leases. A session pops one, and pushes it back once the sector has freed the slot.
    pub leases: ArrayQueue<SlotLease>,
    pub slots: Box<[SlotStatus]>,
    pub metrics: Metrics,
    /// Last completed tick (Release after each tick).
    pub tick: AtomicU32,
    pub stop: AtomicBool,
    pub max_clients: usize,
    /// The debris field the simulation runs, for clients' Welcome.
    pub field_seed: u32,
    pub field_rocks: u16,
    started: std::time::Instant,
}

impl SectorShared {
    /// Microseconds since the sector was built (the clock used for RTT hold times).
    pub fn now_us(&self) -> u64 {
        self.started.elapsed().as_micros() as u64
    }
}

/// Egress ends: one packet-byte ring per client slot, for the egress thread.
pub struct EgressEnds {
    pub rings: Vec<rtrb::Consumer<u8>>,
}

/// Oracle worker ends: pictures out of the sector, advice back in.
pub struct OracleEnds {
    pub pictures: rtrb::Consumer<TacticalPicture>,
    pub advice: rtrb::Producer<TacticalAdvice>,
}

/// Sector-side ends (moved into the sector thread).
pub(crate) struct SectorEnds {
    pub inputs: Vec<rtrb::Consumer<InputMsg>>,
    pub outputs: Vec<rtrb::Producer<u8>>,
    pub pictures: rtrb::Producer<TacticalPicture>,
    pub advice: rtrb::Consumer<TacticalAdvice>,
}

/// Bytes of outbound queue per client (≈ 14 full snapshots).
pub const OUT_RING_BYTES: usize = 16 * 1024;
/// Input messages buffered per client.
pub const IN_RING: usize = 64;

/// Allocates every queue and the simulation. Returns the sector itself plus the ends the network
/// side needs. This is the only place the runtime allocates.
#[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
pub fn build(cfg: SectorConfig) -> (crate::Sector, Arc<SectorShared>, EgressEnds, OracleEnds) {
    let n = cfg.max_clients;
    let control = ArrayQueue::new(n * 4 + 16);
    let leases = ArrayQueue::new(n);
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut egress = Vec::new();
    for slot in 0..n {
        let (p, c) = rtrb::RingBuffer::<InputMsg>::new(IN_RING);
        inputs.push(c);
        let _ = leases.push(SlotLease { slot: slot as u16, input: p });
        let (op, oc) = rtrb::RingBuffer::<u8>::new(OUT_RING_BYTES);
        outputs.push(op);
        egress.push(oc);
    }
    let (pic_p, pic_c) = rtrb::RingBuffer::new(256);
    let (adv_p, adv_c) = rtrb::RingBuffer::new(256);
    let shared = Arc::new(SectorShared {
        control,
        leases,
        slots: (0..n).map(|_| SlotStatus::new()).collect(),
        metrics: Metrics::new(n),
        tick: AtomicU32::new(0),
        stop: AtomicBool::new(false),
        max_clients: n,
        field_seed: cfg.sim.field_seed,
        field_rocks: cfg.sim.field_rocks,
        started: std::time::Instant::now(),
    });
    let ends = SectorEnds { inputs, outputs, pictures: pic_p, advice: adv_c };
    let sector = crate::Sector::new(cfg, shared.clone(), ends);
    (sector, shared, EgressEnds { rings: egress }, OracleEnds { pictures: pic_c, advice: adv_p })
}

/// Reads one `[u16 len][payload]` frame from an egress ring into `buf`. Returns the payload length.
pub fn read_packet(ring: &mut rtrb::Consumer<u8>, buf: &mut [u8]) -> Option<usize> {
    if ring.slots() < 2 {
        return None;
    }
    let mut len = [0u8; 2];
    ring.pop_entire_slice(&mut len).ok()?;
    let n = u16::from_le_bytes(len) as usize;
    if n > buf.len() || ring.pop_entire_slice(&mut buf[..n]).is_err() {
        // Corrupt framing cannot happen with a single producer, but never panic on the egress path.
        return None;
    }
    Some(n)
}
