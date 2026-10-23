//! Snapshot building: sensor-filtered interest, priority accumulation, events and changed rocks
//! repeated until acked, and salvage chunks kept in step with what each client has acked.

use bc_proto::events::Event;
use bc_proto::objects::ROCK_RECORD_BITS;
use bc_proto::snapshot::{ENTITY_MAX_BITS, header_flags};
use bc_proto::{Delta, NO_SLOT, ObjectState, QEntity, SnapshotHeader, SnapshotWriter};
use bc_sim::Sim;
use bc_sim::chunks::{MAX_CHUNKS, Motion};
use bc_sim::missiles::MAX_MISSILES;
use bc_sim::sim::Sight;
use bc_sim::storage::boxed;
use glam::Vec3;

use crate::clients::{
    ClientState, NONE, OBJS_PER_SNAPSHOT, ROCKS_PER_SNAPSHOT, SENT_RING, SentRecord, packed,
};

/// Events older than this are no longer worth re-sending (ticks).
const EVENT_MAX_AGE: u32 = 45;
/// Room kept for entities when packing events: six of the largest records.
const ENTITY_RESERVE_BITS: usize = (ENTITY_MAX_BITS + 1) * 6;
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
const MISSILE_KEEP_BITS: usize = (ENTITY_MAX_BITS + 1) * 12;
/// A suit standing still on a body is worth this much of a moving one's refreshes: it is sent in its
/// body's frame, so every refresh would say the same thing but for its flags and parts.
const STILL_WEIGHT: f32 = 0.1;

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
        Event::SystemHit { target, .. } => near(target),
        // A suit doomed is seen burning by whoever sees it; a capsule thrown clear, and a reactor
        // blown, as far off as a beam.
        Event::Doomed { suit, .. } | Event::Staggered { suit, .. } => near(suit),
        Event::Eject { suit, pos, .. } | Event::Blast { suit, pos, .. } => {
            near(suit)
                || (pos - sim.suits.flight[me].pos).length_squared() < BEAM_NOTICE_RANGE * BEAM_NOTICE_RANGE
        }
        // A target in the Blast Hall flashing: its shooter, and anyone near the hall.
        Event::TargetHit { target, shooter, tick, .. } => {
            near(shooter)
                || (bc_sim::colony::hall::target(usize::from(target), tick, 0.0) - sim.suits.flight[me].pos)
                    .length_squared()
                    < BEAM_NOTICE_RANGE * BEAM_NOTICE_RANGE
        }
        Event::Leave { .. } => false,
    }
}

/// Priority weight of entity `j` for client suit `me` (per tick).
#[cfg(test)]
fn weight(sim: &Sim, me: usize, j: usize) -> f32 {
    let des = |i: usize| sim.designation(i).map_or(NO_SLOT, |t| t as u16);
    weight_with(sim, me, j, des(me), des(j))
}

/// [`weight`], with what `me` and `j` designate found already (`NO_SLOT`: nothing).
fn weight_with(sim: &Sim, me: usize, j: usize, me_designates: u16, j_designates: u16) -> f32 {
    let d = (sim.suits.flight[j].pos - sim.suits.flight[me].pos).length();
    let mut w = 1.0 / (1.0 + d / 800.0);
    if usize::from(j_designates) == me || usize::from(me_designates) == j {
        w *= 3.0;
    }
    if !sim.suits.alive.get(j) {
        w *= 0.3;
    }
    if sim.is_still(j) {
        w *= STILL_WEIGHT;
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
    /// Every suit's record as a spectator would be sent it this tick, in codes (built once, for
    /// every client; a client's adds what depends on it, `Sim::viewer_flags`).
    qs: Box<[QEntity]>,
    /// What each suit shows sensors, how far its own reach, and what it designates (`NO_SLOT`:
    /// nothing), this tick: the sensor model's answers, found once a suit rather than once a pair.
    sights: Box<[Sight]>,
    reach: Box<[f32]>,
    designates: Box<[u16]>,
    /// How much of what it could send a client is sent this tick, 0.2 to 1: the budget and the
    /// suits listed scale with it ([`pace_after`]).
    pub pace: f32,
    /// Suit records written this tick, their bits, and how many were deltas.
    pub records: u64,
    pub record_bits: u64,
    pub deltas: u64,
}

impl Work {
    pub fn new(max_suits: usize, rocks: usize) -> Self {
        Self {
            entities: boxed(max_suits, (0.0, 0)),
            objects: boxed(MAX_CHUNKS, (0.0, 0)),
            rocks: boxed(rocks, (0.0, 0)),
            missiles: boxed(MAX_MISSILES, (0.0, 0)),
            chunk_pos: boxed(MAX_CHUNKS, Vec3::ZERO),
            qs: boxed(max_suits, QEntity::default()),
            sights: boxed(max_suits, Sight::default()),
            reach: boxed(max_suits, 0.0),
            designates: boxed(max_suits, NO_SLOT),
            pace: 1.0,
            records: 0,
            record_bits: 0,
            deltas: 0,
        }
    }

    /// What every client's snapshot shares, found once a tick: where every live chunk is, and
    /// every suit's record but for its viewer's flags.
    pub fn prepare(&mut self, sim: &Sim) {
        for k in sim.chunks.alive.iter() {
            self.chunk_pos[k] = sim.chunk_pose(k).0;
        }
        for j in sim.suits.used.iter() {
            self.qs[j] = QEntity::quantize(&sim.entity_state_watched(j));
            self.sights[j] = sim.sight(j);
            self.reach[j] = sim.sensor_reach(j);
            self.designates[j] = sim.designation(j).map_or(NO_SLOT, |t| t as u16);
        }
        self.records = 0;
        self.record_bits = 0;
        self.deltas = 0;
    }

    /// Suit `j`'s record as `viewer` is sent it.
    fn record_for(&self, sim: &Sim, j: usize, viewer: usize) -> QEntity {
        let mut q = self.qs[j];
        q.flags |= sim.viewer_flags(j, viewer);
        q
    }

    fn count(&mut self, bits: usize, delta: bool) {
        self.records += 1;
        self.record_bits += bits as u64;
        self.deltas += u64::from(delta);
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

/// A tick may take this share of its period before replication sends less (Gates' degradation
/// ladder: what's sent shrinks before the tick overruns), and under this one it sends more again.
const PACE_HIGH: f32 = 0.5;
const PACE_LOW: f32 = 0.3;
/// The least of what it could send a client is sent: a fifth (still more suits than whole records
/// fit in a datagram).
const PACE_MIN: f32 = 0.2;

/// The pace after a tick that took `spent_us`: down a fifth when the tick took more than
/// [`PACE_HIGH`] of its period, up a twentieth when under [`PACE_LOW`].
pub(crate) fn pace_after(pace: f32, spent_us: u64) -> f32 {
    let load = spent_us as f32 * bc_sim::TICK_HZ as f32 / 1e6;
    if load > PACE_HIGH {
        (pace * 0.8).max(PACE_MIN)
    } else if load < PACE_LOW {
        (pace * 1.05).min(1.0)
    } else {
        pace
    }
}

/// What [`build_snapshot`] wrote: the datagram's length, and how far down the client's sorted
/// suits it got (`next` of `top`), for a tick's further parts ([`build_more`]).
pub(crate) struct Built {
    pub len: usize,
    pub next: usize,
    pub top: usize,
}

/// Writes one snapshot for `client` into `buf`: its tick's first datagram.
pub(crate) fn build_snapshot(
    sim: &Sim,
    client: &mut ClientState,
    header: &SnapshotHeader,
    buf: &mut [u8],
    work: &mut Work,
    deltas: bool,
) -> Option<Built> {
    let me = client.suit.idx();
    let t = header.tick;
    let (dgram, stream) = client.next_seq();
    let mut w = SnapshotWriter::new(buf, client.max_datagram);
    w.header(&SnapshotHeader { seq: dgram as u16, stream, ..*header });
    let own = sim.own_state(me);
    w.own(Some(&own));
    let orrery = sim.orrery_info(me);
    w.orrery(orrery.as_ref());

    // --- Interest: update the known set first so fresh Leave notices go out in this snapshot. ---
    let candidates = &mut work.entities;
    let (reach, me_designates) = (work.reach[me], work.designates[me]);
    let mut n_cand = 0;
    for j in sim.suits.used.iter() {
        if j == me {
            continue;
        }
        if sim.visible_by(me, reach, j, &work.sights[j]) {
            let fresh = !client.known.get(j);
            let w = weight_with(sim, me, j, me_designates, work.designates[j]);
            client.prio[j] += w + if fresh { 10.0 } else { 0.0 };
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
    let mut rec = SentRecord { seq: dgram, events_done: done, leaves: leaves_sent, ..SentRecord::default() };
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
    // As many as a tick's parts could hold, deltas of still suits being 30 bits.
    let top_n = ((96 * usize::from(client.max_parts.max(1))) as f32 * work.pace) as usize;
    if cand.len() > top_n {
        cand.select_nth_unstable_by(top_n, |a, b| b.0.total_cmp(&a.0));
    }
    let top = cand.len().min(top_n);
    cand[..top].sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    // Each as a delta against the newest record of it the client acked, if it still holds that.
    rec.ents_from = client.ents_head;
    let next = write_suits(sim, client, work, &mut w, &mut rec, (0, top), object_reserve, deltas);

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
    client.sent[dgram as usize % SENT_RING] = rec;
    Some(Built { len: n, next, top })
}

/// Writes the client's sorted suits from `from` up to `to` (`work.entities`) while they fit, leaving
/// `keep` bits; returns where it stopped. Each goes as a delta against the newest record of it the
/// client acked, if it still holds that.
#[allow(clippy::too_many_arguments)]
fn write_suits(
    sim: &Sim,
    client: &mut ClientState,
    work: &mut Work,
    w: &mut SnapshotWriter<'_>,
    rec: &mut SentRecord,
    (from, to): (usize, usize),
    keep: usize,
    deltas: bool,
) -> usize {
    let me = client.suit.idx();
    let t = sim.tick();
    for k in from..to {
        let j = work.entities[k].1 as usize;
        let q = work.record_for(sim, j, me);
        let generation = sim.suits.generation[j];
        let delta = deltas
            .then(|| {
                client.baseline(j, generation, &q, t).and_then(|(base, back)| Delta::plan(&q, base, back))
            })
            .flatten();
        let Some(bits) = w.entity_q(&q, delta.as_ref(), keep) else { return k };
        work.count(bits, delta.is_some());
        client.note_sent(j, generation, t, &q);
        rec.n_ents += 1;
        client.prio[j] = 0.0;
        client.known.set(j, true);
    }
    to
}

/// Writes a tick's next datagram for `client` after its first ([`build_snapshot`]): more of the
/// suits it sees, from `from` down its sorted list, and nothing else (`header_flags::MORE`).
/// Returns its length and where it stopped.
pub(crate) fn build_more(
    sim: &Sim,
    client: &mut ClientState,
    header: &SnapshotHeader,
    buf: &mut [u8],
    work: &mut Work,
    (from, top): (usize, usize),
    deltas: bool,
) -> Option<(usize, usize)> {
    let (seq, stream) = client.next_seq();
    let mut w = SnapshotWriter::new(buf, client.max_datagram);
    w.header(&SnapshotHeader {
        seq: seq as u16,
        stream,
        flags: header.flags | header_flags::MORE,
        ..*header
    });
    w.own(None);
    w.orrery(None);
    let mut rec = SentRecord { seq, ents_from: client.ents_head, ..SentRecord::default() };
    let next = write_suits(sim, client, work, &mut w, &mut rec, (from, top), 0, deltas);
    let n = w.finish()?;
    client.sent[seq as usize % SENT_RING] = rec;
    Some((n, next))
}

/// How far a spectator sees suits from, m (a suit 2.5 km off is a few pixels through the colony's
/// haze).
pub(crate) const WATCH_RANGE: f32 = 2_500.0;

/// How long a watcher is told of a strike on one of the Blast Hall's targets, ticks: a watcher never
/// acks, so each goes in every snapshot for this long (the client keeps the first it hears).
pub(crate) const WATCH_EVENT_TICKS: u32 = 15;

/// Writes one snapshot for a spectator watching from `at`: no own suit and no Orrery, the suits
/// within [`WATCH_RANGE`] of them by the same priority accumulation as everyone's (weighted by
/// distance), notices of those that left their view or the sector, and the strikes on the Blast
/// Hall's targets near them lately (inside the colony nothing else fires; its rounds aren't drawn on
/// foot, its targets flashing are). Nothing else: rocks, missiles or salvage.
pub(crate) fn build_watch(
    sim: &Sim,
    client: &mut ClientState,
    at: Vec3,
    header: &SnapshotHeader,
    buf: &mut [u8],
    work: &mut Work,
) -> Option<usize> {
    let t = header.tick;
    let (seq, stream) = client.next_seq();
    let mut w = SnapshotWriter::new(buf, client.max_datagram);
    w.header(&SnapshotHeader { seq: seq as u16, stream, ..*header });
    w.own(None);
    w.orrery(None);
    let candidates = &mut work.entities;
    let mut n_cand = 0;
    for j in 0..sim.suits.cap {
        let d = sim.suits.used.get(j).then(|| sim.suits.flight[j].pos.distance(at));
        match d {
            Some(d) if d < WATCH_RANGE => {
                let fresh = !client.known.get(j);
                client.prio[j] += 1.0 / (1.0 + d / 800.0) + if fresh { 10.0 } else { 0.0 };
                if n_cand < candidates.len() {
                    candidates[n_cand] = (client.prio[j], j as u16);
                    n_cand += 1;
                }
            }
            _ if client.known.get(j) => {
                client.known.set(j, false);
                client.prio[j] = 0.0;
                client.queue_leave(j as u16, t);
            }
            _ => {}
        }
    }
    for k in 0..client.n_leaves {
        let (slot, tick) = client.leaves[k];
        if !w.event(&Event::Leave { tick, slot }, ENTITY_RESERVE_BITS) {
            break;
        }
    }
    // The hall's targets struck lately, if the watcher's near enough the hall to see them.
    if bc_sim::colony::hall::gantry().distance(at) < WATCH_RANGE {
        let next = sim.events.next_seq();
        let mut seq = next.saturating_sub(64).max(sim.events.oldest_seq());
        while seq < next {
            if let Some(e @ Event::TargetHit { tick, .. }) = sim.events.get(seq)
                && t.saturating_sub(*tick) <= WATCH_EVENT_TICKS
                && !w.event(e, ENTITY_RESERVE_BITS)
            {
                break;
            }
            seq += 1;
        }
    }
    w.end_events();
    client.retire_watched_leaves(t);
    let cand = &mut work.entities[..n_cand];
    const TOP: usize = 48;
    if cand.len() > TOP {
        cand.select_nth_unstable_by(TOP, |a, b| b.0.total_cmp(&a.0));
    }
    let top = cand.len().min(TOP);
    cand[..top].sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for k in 0..top {
        let j = work.entities[k].1 as usize;
        let Some(bits) = w.entity_q(&work.qs[j], None, 0) else { break };
        work.count(bits, false);
        client.prio[j] = 0.0;
        client.known.set(j, true);
    }
    w.finish()
}

#[cfg(test)]
mod tests {
    use bc_proto::buttons::GRIP;
    use bc_proto::{BodyRef, Faction, FrameId, InputCmd, PilotKind, RiderOn};
    use bc_sim::bodies::{Bodies, Body};
    use bc_sim::{SimConfig, SuitId};
    use glam::{Quat, Vec3};

    use super::*;

    /// An Argo standing on Fornax along `dir` (its frame).
    fn on_fornax(sim: &mut Sim, dir: Vec3) -> SuitId {
        let id = sim
            .spawn_at(FrameId::Argo, Faction::Almagest, PilotKind::Human, Vec3::Y * 5_000.0, Quat::IDENTITY)
            .unwrap();
        assert!(sim.place_on(id, Body::Landmark(0), dir));
        id
    }

    #[test]
    fn still_riders_yield_bandwidth() {
        let mut sim = Sim::new(SimConfig { target_drones: 0, field_rocks: 0, ..SimConfig::default() });
        let (still, walking) = (on_fornax(&mut sim, Vec3::new(-1.5, 1.0, 0.0)), on_fornax(&mut sim, Vec3::Y));
        let at = sim.suits.flight[still.idx()].pos;
        let me = sim
            .spawn_at(
                FrameId::Argo,
                Faction::Colonies,
                PilotKind::Human,
                at + Vec3::Y * 400.0,
                Quat::IDENTITY,
            )
            .unwrap()
            .idx();
        for _ in 0..40 {
            let t = sim.next_tick();
            // Each aims where it faces on the deck, as the deck will be turned (a still mouse on a
            // client, which turns the aim with it).
            let deck = Bodies::at(&sim.field, sim.landmarks(), t).pose(Body::Landmark(0)).unwrap();
            for (id, forward) in [(still, 0), (walking, 127)] {
                let aim = deck.rot * sim.suits.anchor[id.idx()].rot * Vec3::Z;
                let cmd =
                    InputCmd { tick: t, aim, thrust: [0, 0, forward], buttons: GRIP, ..InputCmd::default() };
                sim.set_input(id, cmd);
            }
            sim.step();
        }
        let (s, w) = (still.idx(), walking.idx());
        assert!(sim.is_still(s) && !sim.is_still(w));
        // Weighed by distance as anyone is, then a tenth of that for standing still.
        let by_distance =
            |j: usize| 1.0 / (1.0 + sim.suits.flight[j].pos.distance(at + Vec3::Y * 400.0) / 800.0);
        assert!((weight(&sim, me, s) - by_distance(s) * STILL_WEIGHT).abs() < 1e-6);
        assert!((weight(&sim, me, w) - by_distance(w)).abs() < 1e-6);
        // Both go out on the deck, the still one at rest on it.
        let (es, ew) = (sim.entity_state(s, me), sim.entity_state(w, me));
        let on = Some(RiderOn { body: BodyRef::Landmark(0), aloft: false });
        assert_eq!((es.on, ew.on), (on, on));
        assert!(es.vel.length() < 0.05 && ew.vel.length() > 3.0, "{} {}", es.vel, ew.vel);
        assert_eq!(es.encoded_bits(), 195);
    }
}
