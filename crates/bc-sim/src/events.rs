//! Ring of recent events. The sector re-sends each client the events it hasn't acknowledged, so they
//! must survive for a while (≈1 s at typical rates).

use alloc::boxed::Box;
use bc_proto::Event;

use crate::storage::boxed;

pub struct EventRing {
    buf: Box<[Event]>,
    /// Sequence number stored in each slot (to detect overwrites).
    seqs: Box<[u32]>,
    /// Next sequence number to assign.
    next: u32,
}

fn with_id(mut e: Event, id: u16) -> Event {
    match &mut e {
        Event::BeamSpawn { id: i, .. }
        | Event::Hit { id: i, .. }
        | Event::Kill { id: i, .. }
        | Event::Clash { id: i, .. }
        | Event::Seizure { id: i, .. }
        | Event::Detach { id: i, .. }
        | Event::RockBreak { id: i, .. }
        | Event::MissileBurst { id: i, .. } => *i = id,
        Event::Leave { .. } => {}
    }
    e
}

impl EventRing {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(16).next_power_of_two();
        Self { buf: boxed(cap, Event::Leave { tick: 0, slot: 0 }), seqs: boxed(cap, u32::MAX), next: 0 }
    }

    /// Stores an event, stamping it with the low 16 bits of its sequence number as `id`.
    pub fn push(&mut self, e: Event) -> u32 {
        let seq = self.next;
        self.next = self.next.wrapping_add(1);
        let slot = seq as usize & (self.buf.len() - 1);
        self.buf[slot] = with_id(e, seq as u16);
        self.seqs[slot] = seq;
        seq
    }

    /// Sequence number the next event will get (events `< next_seq()` exist).
    pub fn next_seq(&self) -> u32 {
        self.next
    }

    /// The event with sequence `seq`, if it hasn't been overwritten.
    pub fn get(&self, seq: u32) -> Option<&Event> {
        let slot = seq as usize & (self.buf.len() - 1);
        (self.seqs[slot] == seq).then(|| &self.buf[slot])
    }

    /// Oldest sequence still stored.
    pub fn oldest_seq(&self) -> u32 {
        self.next.saturating_sub(self.buf.len() as u32)
    }
}
