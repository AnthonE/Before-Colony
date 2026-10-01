//! The lock-free plumbing between the sector thread and the network side, allocated once.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use bc_proto::{Faction, FrameId, InputPacket, PilotKind};
use bc_sim::SuitId;
use bc_sim::sim::{Homecoming, Loadout, ParkRecord, SleeperFate};
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

/// A signed-in pilot coming back: the suit they left asleep, and what they'd earned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Comeback {
    /// The sleeping suit to wake (entity slot, generation).
    pub sleeper: Option<(u16, u16)>,
    /// Credits a new suit starts with.
    pub credits: u32,
}

/// Network → sector commands.
#[derive(Clone, Copy, Debug)]
pub enum Control {
    /// Seats a pilot: in the suit they left asleep, if it's still there; else, under survival
    /// rules, in the suit they `launch` (none: they aren't seated); under arcade rules, a new
    /// `frame` at their faction's spawn.
    Join {
        slot: u16,
        pilot: PilotKind,
        frame: FrameId,
        faction: Faction,
        max_datagram: u16,
        comeback: Comeback,
        launch: Option<Loadout>,
    },
    /// Takes the pilot's suit into the hangar, if it's at rest in the dock (survival rules).
    Dock {
        slot: u16,
    },
    /// The pilot left: the suit goes too.
    Leave {
        slot: u16,
    },
    /// A signed-in pilot left: the suit stays, its pilot asleep in the cockpit (a wreck goes).
    Sleep {
        slot: u16,
    },
    Respawn {
        slot: u16,
        frame: FrameId,
    },
    /// Puts a suit left in a hide spot before the server restarted back there, asleep. The
    /// sector answers on [`SectorShared::restored`] with the same `key`, or not at all (no room).
    Restore {
        key: u32,
        rec: ParkRecord,
    },
}

/// A suit [`Control::Restore`] put back: which request, and the suit (entity slot, generation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Restored {
    pub key: u32,
    pub suit: u16,
    pub generation: u16,
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

/// How a slot's last join or departure went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Outcome {
    /// Seated in a new suit.
    Fresh = 0,
    /// Seated in the suit they'd left asleep.
    Woke = 1,
    /// Left, and the suit sleeps on ([`SlotStatus::suit_id`] says which).
    Asleep = 2,
    /// Left, and the suit is gone.
    Released = 3,
    /// The suit docked: the pilot is in the hangar ([`Report::Home`] says with what).
    Docked = 4,
    /// The suit was destroyed and its wreck is gone: the pilot is back in the hangar.
    Lost = 5,
}

/// What the sector tells a slot's session about its suit (survival rules), through the slot's
/// report ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Report {
    /// The suit docked, bringing this home.
    Home(Homecoming),
    /// It asked to dock, but it isn't at rest in the dock.
    DockRefused,
    /// The suit was destroyed; the bounties it had earned.
    Lost { bounty: u32 },
    /// Its pilot left it asleep in a landmark's hide spot: what it takes to put it back there
    /// after a restart. Sent before the slot is published free.
    Parked(ParkRecord),
}

/// Per-slot status visible to the network side.
pub struct SlotStatus {
    state: AtomicU32,
    /// The pilot's suit: generation << 16 | entity slot.
    suit: AtomicU32,
    /// Bumped every time the slot is (re)joined, so a waiter can tell its join apart from an
    /// earlier session's.
    epoch: AtomicU32,
    outcome: AtomicU32,
}

impl SlotStatus {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(SlotState::Free as u32),
            suit: AtomicU32::new(u32::MAX),
            epoch: AtomicU32::new(0),
            outcome: AtomicU32::new(Outcome::Fresh as u32),
        }
    }

    pub fn state(&self) -> SlotState {
        match self.state.load(Ordering::Acquire) {
            1 => SlotState::Active,
            2 => SlotState::Refused,
            _ => SlotState::Free,
        }
    }

    /// The suit's entity slot.
    pub fn suit(&self) -> Option<u16> {
        self.suit_id().map(|(idx, _)| idx)
    }

    /// The suit's entity slot and generation.
    pub fn suit_id(&self) -> Option<(u16, u16)> {
        let s = self.suit.load(Ordering::Acquire);
        (s != u32::MAX).then_some((s as u16, (s >> 16) as u16))
    }

    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn outcome(&self) -> Outcome {
        match self.outcome.load(Ordering::Acquire) {
            1 => Outcome::Woke,
            2 => Outcome::Asleep,
            3 => Outcome::Released,
            4 => Outcome::Docked,
            5 => Outcome::Lost,
            _ => Outcome::Fresh,
        }
    }

    pub(crate) fn publish(&self, state: SlotState, suit: Option<SuitId>, outcome: Outcome) {
        let packed = suit.map_or(u32::MAX, |id| u32::from(id.0.generation) << 16 | u32::from(id.0.idx));
        self.suit.store(packed, Ordering::Release);
        self.outcome.store(outcome as u32, Ordering::Release);
        if state != SlotState::Free {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.state.store(state as u32, Ordering::Release);
    }
}

/// The right to use one client slot: owned by exactly one session task at a time. Carries the
/// producer end of that slot's input ring, and the consumer end of its report ring.
pub struct SlotLease {
    pub slot: u16,
    pub input: rtrb::Producer<InputMsg>,
    pub reports: rtrb::Consumer<Report>,
}

/// Shared between the sector thread and the network side.
pub struct SectorShared {
    pub control: ArrayQueue<Control>,
    /// What became of sleeping suits (destroyed, or cleared for room), for the server to tell
    /// their pilots. Whose each was is the server's to know.
    pub notes: ArrayQueue<SleeperFate>,
    /// Suits put back by [`Control::Restore`], for the server to hand to their pilots.
    pub restored: ArrayQueue<Restored>,
    /// Free slot leases. A session pops one, and pushes it back once the sector has freed the slot.
    pub leases: ArrayQueue<SlotLease>,
    pub slots: Box<[SlotStatus]>,
    pub metrics: Metrics,
    /// Last completed tick (Release after each tick).
    pub tick: AtomicU32,
    pub stop: AtomicBool,
    pub max_clients: usize,
    /// The debris field the simulation runs, and how many of the compiled landmarks the sector has
    /// (`Sim::landmarks`), for clients' Welcome.
    pub field_seed: u32,
    pub field_rocks: u16,
    pub landmarks: u8,
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
    pub reports: Vec<rtrb::Producer<Report>>,
    pub outputs: Vec<rtrb::Producer<u8>>,
    pub pictures: rtrb::Producer<TacticalPicture>,
    pub advice: rtrb::Consumer<TacticalAdvice>,
}

/// Bytes of outbound queue per client (≈ 14 full snapshots).
pub const OUT_RING_BYTES: usize = 16 * 1024;
/// Input messages buffered per client.
pub const IN_RING: usize = 64;
/// Sleepers' fates waiting for the server.
pub const NOTES: usize = 256;
/// Reports waiting for a slot's session.
pub const REPORTS: usize = 8;
/// Restored suits waiting for the server.
pub const RESTORED: usize = 256;

/// Allocates every queue and the simulation. Returns the sector itself plus the ends the network
/// side needs. This is the only place the runtime allocates.
#[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
pub fn build(cfg: SectorConfig) -> (crate::Sector, Arc<SectorShared>, EgressEnds, OracleEnds) {
    let n = cfg.max_clients;
    let control = ArrayQueue::new(n * 4 + 16);
    let leases = ArrayQueue::new(n);
    let mut inputs = Vec::new();
    let mut reports = Vec::new();
    let mut outputs = Vec::new();
    let mut egress = Vec::new();
    for slot in 0..n {
        let (p, c) = rtrb::RingBuffer::<InputMsg>::new(IN_RING);
        inputs.push(c);
        let (rp, rc) = rtrb::RingBuffer::<Report>::new(REPORTS);
        reports.push(rp);
        let _ = leases.push(SlotLease { slot: slot as u16, input: p, reports: rc });
        let (op, oc) = rtrb::RingBuffer::<u8>::new(OUT_RING_BYTES);
        outputs.push(op);
        egress.push(oc);
    }
    let (pic_p, pic_c) = rtrb::RingBuffer::new(256);
    let (adv_p, adv_c) = rtrb::RingBuffer::new(256);
    let shared = Arc::new(SectorShared {
        control,
        notes: ArrayQueue::new(NOTES),
        restored: ArrayQueue::new(RESTORED),
        leases,
        slots: (0..n).map(|_| SlotStatus::new()).collect(),
        metrics: Metrics::new(n),
        tick: AtomicU32::new(0),
        stop: AtomicBool::new(false),
        max_clients: n,
        field_seed: cfg.sim.field_seed,
        field_rocks: cfg.sim.field_rocks,
        landmarks: cfg.sim.landmark_defs().len() as u8,
        started: std::time::Instant::now(),
    });
    let ends = SectorEnds { inputs, reports, outputs, pictures: pic_p, advice: adv_c };
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
