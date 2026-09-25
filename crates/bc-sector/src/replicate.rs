//! Snapshot building: sensor-filtered interest, priority accumulation, events repeated until acked.

use bc_proto::events::Event;
use bc_proto::snapshot::{ENTITY_BITS, ent_flags};
use bc_proto::{SnapshotHeader, SnapshotWriter};
use bc_sim::Sim;

use crate::clients::{ClientState, SENT_RING, SentRecord};

/// Events older than this are no longer worth re-sending (ticks).
const EVENT_MAX_AGE: u32 = 45;
/// Room kept for entities when packing events.
const ENTITY_RESERVE_BITS: usize = (ENTITY_BITS + 1) * 6;
/// Beams whose origin is this close are shown even if the shooter isn't on sensors.
const BEAM_NOTICE_RANGE: f32 = 5_000.0;

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
        Event::Leave { .. } => false,
    }
}

/// Priority weight of entity `j` for client suit `me` (per tick).
fn weight(sim: &Sim, me: usize, j: usize) -> f32 {
    let d = (sim.suits.flight[j].pos - sim.suits.flight[me].pos).length();
    let mut w = 1.0 / (1.0 + d / 800.0);
    if sim.suits.input[j].lock_target == me as u16 || sim.suits.input[me].lock_target == j as u16 {
        w *= 3.0;
    }
    if !sim.suits.alive.get(j) {
        w *= 0.3;
    }
    w
}

/// Writes one snapshot for `client` into `buf`; returns its length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_snapshot(
    sim: &Sim,
    client: &mut ClientState,
    header: &SnapshotHeader,
    buf: &mut [u8],
    candidates: &mut [(f32, u16)],
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

    // --- Events: Leave notices first, then simulation events in sequence order. ---
    let mut leaves_sent = 0u8;
    for k in 0..client.n_leaves {
        let (slot, tick) = client.leaves[k];
        if !w.event(&Event::Leave { tick, slot }, ENTITY_RESERVE_BITS) {
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
                if w.event(e, ENTITY_RESERVE_BITS) {
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

    // --- Entities, highest priority first. ---
    let cand = &mut candidates[..n_cand];
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
        if !w.entity(&e, 0) {
            break;
        }
        client.prio[j] = 0.0;
        client.known.set(j, true);
    }
    let n = w.finish()?;
    client.sent[t as usize % SENT_RING] = SentRecord { tick: t, events_done: done, leaves: leaves_sent };
    Some(n)
}
