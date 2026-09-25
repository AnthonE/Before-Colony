//! Snapshot building: sensor-filtered interest, priority accumulation, events and changed rocks
//! repeated until acked, and salvage chunks kept in step with what each client has acked.

use bc_proto::events::Event;
use bc_proto::objects::ROCK_RECORD_BITS;
use bc_proto::snapshot::{ENTITY_BITS, ent_flags};
use bc_proto::{ObjectState, SnapshotHeader, SnapshotWriter};
use bc_sim::Sim;
use bc_sim::chunks::{MAX_CHUNKS, Motion};
use bc_sim::missiles::MAX_MISSILES;
use bc_sim::storage::boxed;
use glam::Vec3;

use crate::clients::{
    ClientState, NONE, OBJS_PER_SNAPSHOT, ROCKS_PER_SNAPSHOT, SENT_RING, SentRecord, packed,
};

/// Events older than this are no longer worth re-sending (ticks).
const EVENT_MAX_AGE: u32 = 45;
/// Room kept for entities when packing events.
const ENTITY_RESERVE_BITS: usize = (ENTITY_BITS + 1) * 6;
/// Beams whose origin is this close are shown even if the shooter isn't on sensors.
const BEAM_NOTICE_RANGE: f32 = 5_000.0;
/// Free chunks within this range are sent; a client that has one keeps it until a tenth further.
const OBJECT_RANGE: f32 = 3_000.0;
/// Room kept for objects when packing events and entities: up to this many of the largest.
const OBJECT_RESERVE: usize = 6;
/// Missiles a snapshot carries at most, and how far off they're shown (those tracking the client
/// go first, wherever they are).
const MISSILES_PER_SNAPSHOT: usize = 12;
const MISSILE_RANGE: f32 = 5_000.0;
/// Room missiles leave for entities.
const MISSILE_KEEP_BITS: usize = (ENTITY_BITS + 1) * 12;

fn relevant(sim: &Sim, me: usize, e: &Event) -> bool {
    let near = |j: u16| j as usize == me || sim.visible_to(me, j as usize);
    match *e {
        Event::BeamSpawn { shooter, origin, .. } => {
            near(shooter)
                || (origin - sim.suits.flight[me].pos).length_squared()
                    < BEAM_NOTICE_RANGE * BEAM_NOTICE_RANGE
        }
        Event::Hit { target, shooter, .. } => near(target) || shooter as usize == me,
        // The kill feed is sector-wide.
        Event::Kill { .. } => true,
        Event::Clash { a, b, .. } => near(a) || near(b),
        Event::Seizure { pilot, .. } => near(pilot),
        Event::Detach { source, from_hulk, .. } => from_hulk || near(source),
        // A rock shattering is seen from as far off as a beam.
        Event::RockBreak { rock, .. } => sim.field.rocks().get(rock as usize).is_some_and(|r| {
            (r.pos - sim.suits.flight[me].pos).length_squared() < BEAM_NOTICE_RANGE * BEAM_NOTICE_RANGE
        }),
        // So is a missile bursting.
        Event::MissileBurst { pos, .. } => {
            (pos - sim.suits.flight[me].pos).length_squared() < BEAM_NOTICE_RANGE * BEAM_NOTICE_RANGE
        }
        Event::Leave { .. } => false,
    }
}

/// Priority weight of entity `j` for client suit `me` (per tick).
fn weight(sim: &Sim, me: usize, j: usize) -> f32 {
    let d = (sim.suits.flight[j].pos - sim.suits.flight[me].pos).length();
    let mut w = 1.0 / (1.0 + d / 800.0);
    if sim.designation(j) == Some(me) || sim.designation(me) == Some(j) {
        w *= 3.0;
    }
    if !sim.suits.alive.get(j) {
        w *= 0.3;
    }
    w
}

/// Scratch the snapshot builder works in, sized at construction.
pub(crate) struct Work {
    /// Entity candidates: (priority, slot).
    entities: Box<[(f32, u16)]>,
    /// Object candidates: (distance², id), or -1 for a Gone record.
    objects: Box<[(f32, u16)]>,
    /// Rock candidates: (distance², id).
    rocks: Box<[(f32, u16)]>,
    /// Missile candidates: (key, id), those tracking the client keyed below all others.
    missiles: Box<[(f32, u16)]>,
    /// Where each live chunk is this tick (found once per tick, for every client).
    chunk_pos: Box<[Vec3]>,
}

impl Work {
    pub fn new(max_suits: usize, rocks: usize) -> Self {
        Self {
            entities: boxed(max_suits, (0.0, 0)),
            objects: boxed(MAX_CHUNKS, (0.0, 0)),
            rocks: boxed(rocks, (0.0, 0)),
            missiles: boxed(MAX_MISSILES, (0.0, 0)),
            chunk_pos: boxed(MAX_CHUNKS, Vec3::ZERO),
        }
    }

    /// Notes where every live chunk is at the simulation's current tick.
    pub fn locate_chunks(&mut self, sim: &Sim) {
        for k in sim.chunks.alive.iter() {
            self.chunk_pos[k] = sim.chunk_pose(k).0;
        }
    }
}

/// The `k` lowest-keyed candidates, in order.
fn lowest(cand: &mut [(f32, u16)], k: usize) -> &[(f32, u16)] {
    if cand.len() > k {
        cand.select_nth_unstable_by(k, |a, b| a.0.total_cmp(&b.0));
    }
    let n = cand.len().min(k);
    cand[..n].sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    &cand[..n]
}

/// Rocks whose state the client hasn't acked, keyed by distance.
fn rock_candidates(sim: &Sim, client: &ClientState, at: Vec3, out: &mut [(f32, u16)]) -> usize {
    let mut n = 0;
    for (i, r) in sim.field.rocks().iter().enumerate() {
        if client.rock_acked[i] != sim.rocks.version[i] {
            out[n] = ((r.pos - at).length_squared(), i as u16);
            n += 1;
        }
    }
    n
}

/// Missiles near client suit `me`, or tracking it, keyed nearest first with those tracking it
/// ahead of all.
fn missile_candidates(sim: &Sim, me: usize, out: &mut [(f32, u16)]) -> usize {
    let at = sim.suits.flight[me].pos;
    let m = &sim.missiles;
    let mut n = 0;
    for k in m.alive.iter() {
        let d2 = (m.pos[k] - at).length_squared();
        let key = if m.target[k] == me as u16 {
            d2 - 1e15
        } else if d2 < MISSILE_RANGE * MISSILE_RANGE {
            d2
        } else {
            continue;
        };
        out[n] = (key, k as u16);
        n += 1;
    }
    n
}

/// Whether client suit `me` should have chunk `k`: free ones in range (a little further for one it
/// already has), held ones in its own hand or a hand it can see.
fn wants(sim: &Sim, me: usize, k: usize, pos: Vec3, has: bool) -> bool {
    match sim.chunks.motion[k] {
        Motion::Free(_) => {
            let r = if has { OBJECT_RANGE * 1.1 } else { OBJECT_RANGE };
            (pos - sim.suits.flight[me].pos).length_squared() < r * r
        }
        Motion::Held { holder, .. } => holder as usize == me || sim.visible_to(me, holder as usize),
    }
}

/// Chunks whose state the client lacks (keyed by distance), and ones it has but no longer should
/// (keyed -1: their Gone records go first).
fn object_candidates(
    sim: &Sim,
    client: &ClientState,
    me: usize,
    pos: &[Vec3],
    out: &mut [(f32, u16)],
) -> usize {
    if sim.chunks.count() == 0 && client.obj_known == 0 {
        return 0;
    }
    let at = sim.suits.flight[me].pos;
    let mut n = 0;
    for (k, (&acked, &p)) in client.obj_acked.iter().zip(pos).enumerate() {
        let has = acked != NONE;
        let key = if sim.chunks.alive.get(k) && wants(sim, me, k, p, has) {
            if acked == packed(sim.chunks.generation[k], sim.chunks.version[k]) {
                continue;
            }
            (p - at).length_squared()
        } else if has {
            -1.0
        } else {
            continue;
        };
        out[n] = (key, k as u16);
        n += 1;
    }
    n
}

/// Writes one snapshot for `client` into `buf`; returns its length.
pub(crate) fn build_snapshot(
    sim: &Sim,
    client: &mut ClientState,
    header: &SnapshotHeader,
    buf: &mut [u8],
    work: &mut Work,
) -> Option<usize> {
    let me = client.suit.idx();
    let t = header.tick;
    let mut w = SnapshotWriter::new(buf, client.max_datagram);
    w.header(header);
    let own = sim.own_state(me);
    w.own(Some(&own));
    let zero = sim.zero_info(me);
    w.zero(zero.as_ref());

    // --- Interest: update the known set first so fresh Leave notices go out in this snapshot. ---
    let candidates = &mut work.entities;
    let mut n_cand = 0;
    for j in sim.suits.used.iter() {
        if j == me {
            continue;
        }
        if sim.visible_to(me, j) {
            let fresh = !client.known.get(j);
            client.prio[j] += weight(sim, me, j) + if fresh { 10.0 } else { 0.0 };
            if n_cand < candidates.len() {
                candidates[n_cand] = (client.prio[j], j as u16);
                n_cand += 1;
            }
        } else if client.known.get(j) {
            client.known.set(j, false);
            client.prio[j] = 0.0;
            client.queue_leave(j as u16, t);
        }
    }
    // Entities whose slot was released entirely (a disconnect, a cleared wreck): forget them too.
    for j in 0..sim.suits.cap {
        if client.known.get(j) && !sim.suits.used.get(j) {
            client.known.set(j, false);
            client.queue_leave(j as u16, t);
        }
    }

    // --- What rocks and objects need sending, so events and entities leave room for them. ---
    let n_rocks = rock_candidates(sim, client, sim.suits.flight[me].pos, &mut work.rocks);
    let n_objs = object_candidates(sim, client, me, &work.chunk_pos, &mut work.objects);
    let object_reserve = n_objs.min(OBJECT_RESERVE) * (ObjectState::MAX_BITS + 1);
    let rock_reserve = n_rocks.min(ROCKS_PER_SNAPSHOT) * (ROCK_RECORD_BITS + 1);
    let keep_for_events = ENTITY_RESERVE_BITS + rock_reserve + object_reserve;

    // --- Events: Leave notices first, then simulation events in sequence order. ---
    let mut leaves_sent = 0u8;
    for k in 0..client.n_leaves {
        let (slot, tick) = client.leaves[k];
        if !w.event(&Event::Leave { tick, slot }, keep_for_events) {
            break;
        }
        leaves_sent += 1;
    }
    let next = sim.events.next_seq();
    let oldest = sim.events.oldest_seq();
    let mut seq = client.event_acked.max(oldest);
    // Skip stale events entirely.
    while seq < next {
        match sim.events.get(seq) {
            Some(e) if t.saturating_sub(e.tick()) > EVENT_MAX_AGE => seq += 1,
            Some(_) => break,
            None => seq += 1,
        }
    }
    let mut done = seq;
    let mut contiguous = true;
    while seq < next {
        if let Some(e) = sim.events.get(seq) {
            if relevant(sim, me, e) {
                if w.event(e, keep_for_events) {
                    if contiguous {
                        done = seq + 1;
                    }
                } else {
                    contiguous = false;
                }
            } else if contiguous {
                done = seq + 1;
            }
        } else if contiguous {
            done = seq + 1;
        }
        seq += 1;
    }
    w.end_events();

    // --- Changed rocks, nearest first. ---
    let mut rec = SentRecord { tick: t, events_done: done, leaves: leaves_sent, ..SentRecord::default() };
    for &(_, i) in lowest(&mut work.rocks[..n_rocks], ROCKS_PER_SNAPSHOT) {
        if !w.rock(&sim.rock_state(i as usize), ENTITY_RESERVE_BITS + object_reserve) {
            break;
        }
        rec.rocks[rec.n_rocks as usize] = (i, sim.rocks.version[i as usize]);
        rec.n_rocks += 1;
    }

    // --- Missiles in flight: those tracking the client first, then the nearest. ---
    let n_missiles = missile_candidates(sim, me, &mut work.missiles);
    for &(_, k) in lowest(&mut work.missiles[..n_missiles], MISSILES_PER_SNAPSHOT) {
        if !w.missile(&sim.missile_state(k as usize, me), MISSILE_KEEP_BITS + object_reserve) {
            break;
        }
    }

    // --- Entities, highest priority first. ---
    let cand = &mut work.entities[..n_cand];
    const TOP: usize = 48;
    if cand.len() > TOP {
        cand.select_nth_unstable_by(TOP, |a, b| b.0.total_cmp(&a.0));
    }
    let top = cand.len().min(TOP);
    cand[..top].sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for &(_, j) in cand[..top].iter() {
        let j = j as usize;
        let mut e = sim.entity_state(j, me);
        if !sim.suits.alive.get(j) {
            e.flags |= ent_flags::WRECK;
        }
        if !w.entity(&e, object_reserve) {
            break;
        }
        client.prio[j] = 0.0;
        client.known.set(j, true);
    }

    // --- Objects: what the client should forget first, then the nearest. ---
    for &(key, k) in lowest(&mut work.objects[..n_objs], OBJS_PER_SNAPSHOT) {
        let (state, holds) = if key < 0.0 {
            (ObjectState::Gone { id: k }, NONE)
        } else {
            let c = &sim.chunks;
            (sim.object_state(k as usize), packed(c.generation[k as usize], c.version[k as usize]))
        };
        if !w.object(&state) {
            break;
        }
        rec.objs[rec.n_objs as usize] = (k, holds);
        rec.n_objs += 1;
    }
    let n = w.finish()?;
    client.sent[t as usize % SENT_RING] = rec;
    Some(n)
}
