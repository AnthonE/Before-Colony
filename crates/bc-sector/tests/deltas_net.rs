//! Delta records end to end: a real sector full of drones and real clients over lossy links that
//! reorder datagrams. Every suit record a client takes, delta or whole, is exactly what the server
//! would have sent whole (`docs/NETCODE.md`).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use bc_client_core::{ClientConfig, ClientCore, InputContext};
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST};
use bc_proto::control::ControlMsg;
use bc_proto::{EntityState, Faction, FrameId, InputCmd, InputPacket, PROTOCOL_VERSION, PilotKind, QEntity};
use bc_sector::{Comeback, Control, InputMsg, Metrics, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::math::Rng;
use glam::Vec3;

/// One direction of a link: `base ± jitter` late, some lost; jitter wider than a tick reorders.
struct Link {
    rng: Rng,
    base: f64,
    jitter: f64,
    loss: f32,
    queue: BinaryHeap<Reverse<(u64, u64, Vec<u8>)>>,
    seq: u64,
}

impl Link {
    fn new(seed: u64, loss: f32) -> Self {
        Self { rng: Rng::new(seed), base: 0.05, jitter: 0.04, loss, queue: BinaryHeap::new(), seq: 0 }
    }
    fn send(&mut self, now: f64, bytes: Vec<u8>) {
        if self.rng.next_f32() < self.loss {
            return;
        }
        let at = now + self.base + f64::from(self.rng.signed()) * self.jitter;
        self.seq += 1;
        self.queue.push(Reverse(((at * 1e6) as u64, self.seq, bytes)));
    }
    fn deliver(&mut self, now: f64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while self.queue.peek().is_some_and(|Reverse((at, _, _))| *at as f64 / 1e6 <= now) {
            out.push(self.queue.pop().unwrap().0.2);
        }
        out
    }
}

fn weaving(ctx: &InputContext) -> InputCmd {
    let t = f64::from(ctx.tick) / 30.0;
    let q = |v: f64| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        aim: Vec3::new((t * 0.7).sin() as f32 * 0.6, (t * 0.3).cos() as f32 * 0.3, 1.0).normalize(),
        thrust: [q((t * 0.9).sin()), q((t * 0.5).cos() * 0.5), q(0.6 + (t * 0.3).sin() * 0.4)],
        buttons: FLIGHT_ASSIST | if (t * 2.0).sin() > 0.5 { FIRE_PRIMARY } else { 0 },
        ..InputCmd::default()
    }
}

struct Run {
    /// Records the clients took, and how many of them were exactly the server's.
    checked: u64,
    wrong: u64,
    /// The clients' datagrams dropped as stale (reordered), and decode errors (a baseline missing).
    stale: u64,
    decode_errors: u64,
    /// What the sector sent: suit records, how many were deltas, and datagrams after a tick's first.
    records: u64,
    deltas: u64,
    extra_parts: u64,
    /// A tick's further datagrams the clients took.
    parts: u64,
}

/// Four pilots and `drones` drones for 30 s, over links losing `loss` each way and reordering, the
/// pilots' datagrams at most `max_datagram` bytes.
fn run(deltas: bool, loss: f32, drones: u32, max_datagram: u16) -> Run {
    let cfg = SectorConfig {
        sim: SimConfig { target_drones: drones, seed: 5, ..SimConfig::default() },
        max_clients: 4,
        deltas,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    struct Seat {
        lease: bc_sector::SlotLease,
        client: ClientCore,
        up: Link,
        down: Link,
        suit: Option<usize>,
    }
    let mut seats: Vec<Seat> = (0..4)
        .map(|k| {
            let lease = shared.leases.pop().expect("lease");
            shared
                .control
                .push(Control::Join {
                    slot: lease.slot,
                    pilot: PilotKind::Human,
                    frame: FrameId::Argo,
                    faction: Faction::Colonies,
                    max_datagram,
                    comeback: Comeback::default(),
                    launch: None,
                })
                .unwrap();
            Seat {
                lease,
                client: ClientCore::new(ClientConfig {
                    name: format!("Pilot {k}"),
                    pilot: PilotKind::Human,
                    frame: FrameId::Argo,
                    faction: Faction::Colonies,
                }),
                up: Link::new(100 + k, loss),
                down: Link::new(200 + k, loss),
                suit: None,
            }
        })
        .collect();
    // What each pilot's client should have of each suit at each tick: the server's record, whole.
    let mut truth: HashMap<(u32, usize, u16), EntityState> = HashMap::new();
    let mut out = Run {
        checked: 0,
        wrong: 0,
        stale: 0,
        decode_errors: 0,
        records: 0,
        deltas: 0,
        extra_parts: 0,
        parts: 0,
    };
    let (mut t, mut next_tick, mut next_poll) = (0.0f64, 0.0f64, 0.0f64);
    let mut buf = [0u8; 2048];
    while t < 30.0 {
        for s in &mut seats {
            for bytes in s.up.deliver(t) {
                let packet = InputPacket::decode(&bytes).expect("input decodes");
                let _ = s.lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
            }
        }
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
            sector.tick_at((t * 1e6) as u64);
            let tick = sector.sim.tick();
            for (k, s) in seats.iter_mut().enumerate() {
                let slot = s.lease.slot as usize;
                if s.suit.is_none() && shared.slots[slot].state() == SlotState::Active {
                    let (idx, _) = shared.slots[slot].suit_id().expect("a suit");
                    s.suit = Some(usize::from(idx));
                    let mut w = [0u8; 64];
                    let n = ControlMsg::Welcome {
                        version: PROTOCOL_VERSION,
                        client_slot: s.lease.slot,
                        tick,
                        tick_hz: 30,
                        sector: 1,
                        orrery_allowed: true,
                        max_datagram,
                        field_seed: shared.field_seed,
                        field_rocks: shared.field_rocks,
                        flags: 0,
                        landmarks: shared.landmarks,
                        faction: Faction::Colonies,
                    }
                    .encode(&mut w)
                    .unwrap();
                    s.client.on_control(&w[..n]);
                }
                if let Some(me) = s.suit {
                    for j in sector.sim.suits.used.iter().filter(|&j| j != me) {
                        let e = QEntity::quantize(&sector.sim.entity_state(j, me)).state();
                        truth.insert((tick, k, j as u16), e);
                    }
                }
                while let Some(n) = read_packet(&mut egress.rings[slot], &mut buf) {
                    s.down.send(t, buf[..n].to_vec());
                }
            }
        }
        for (k, s) in seats.iter_mut().enumerate() {
            for bytes in s.down.deliver(t) {
                s.client.on_datagram(&bytes, t);
                let w = &s.client.world;
                for (j, e) in w.entities.iter().enumerate() {
                    if let Some(e) = e.as_ref().filter(|e| e.latest_tick == w.tick) {
                        out.checked += 1;
                        if truth.get(&(e.latest_tick, k, j as u16)) != Some(&e.latest) {
                            out.wrong += 1;
                        }
                    }
                }
            }
            if t >= next_poll {
                for p in s.client.poll_inputs(t, &mut weaving) {
                    s.up.send(t, p);
                }
            }
        }
        if t >= next_poll {
            next_poll += 1.0 / 60.0;
        }
        t += 0.001;
    }
    for s in &seats {
        out.stale += s.client.stats.stale;
        out.decode_errors += s.client.stats.decode_errors;
        out.parts += s.client.stats.parts;
    }
    let m = &shared.metrics;
    out.records = Metrics::load(&m.entity_records);
    out.deltas = Metrics::load(&m.entity_deltas);
    out.extra_parts = Metrics::load(&m.extra_parts);
    out
}

#[test]
fn deltas_rebuild_every_suit_exactly_over_a_lossy_reordering_link() {
    let r = run(true, 0.1, 48, bc_proto::MAX_DATAGRAM as u16);
    println!(
        "checked {} records ({} wrong), {} stale datagrams, {} decode errors; sent {} records, {} deltas",
        r.checked, r.wrong, r.stale, r.decode_errors, r.records, r.deltas
    );
    assert!(r.checked > 20_000, "records checked: {}", r.checked);
    assert_eq!(r.wrong, 0, "records not exactly the server's");
    assert_eq!(r.decode_errors, 0, "a delta whose baseline the client didn't have");
    assert!(r.stale > 0, "the link reordered nothing");
    assert!(r.deltas * 2 > r.records, "deltas {} of {} records", r.deltas, r.records);
}

#[test]
fn with_deltas_off_every_record_is_whole_and_just_as_exact() {
    let r = run(false, 0.1, 48, bc_proto::MAX_DATAGRAM as u16);
    assert!(r.checked > 20_000, "records checked: {}", r.checked);
    assert_eq!((r.wrong, r.decode_errors, r.deltas), (0, 0, 0));
}

/// 120 drones and 400-byte datagrams: more than one holds, so a tick sends a pilot more, as its
/// budget allows. The further ones are deltas too, and every record is as exact.
#[test]
fn a_crowd_goes_in_more_datagrams_a_tick_and_stays_exact() {
    let r = run(true, 0.1, 120, 400);
    println!(
        "checked {} records ({} wrong), {} decode errors; sent {} records, {} deltas, {} further datagrams ({} taken)",
        r.checked, r.wrong, r.decode_errors, r.records, r.deltas, r.extra_parts, r.parts
    );
    assert!(r.checked > 20_000, "records checked: {}", r.checked);
    assert_eq!((r.wrong, r.decode_errors), (0, 0));
    assert!(
        r.extra_parts > 1_000 && r.parts > 500,
        "further datagrams {} sent, {} taken",
        r.extra_parts,
        r.parts
    );
    assert!(r.deltas * 2 > r.records, "deltas {} of {} records", r.deltas, r.records);
}
