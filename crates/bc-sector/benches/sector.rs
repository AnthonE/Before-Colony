//! How the sector tick grows with its pilots: `cargo bench -p bc-sector --bench sector`.
//!
//! Every pilot joins at its faction's spawn and flies, firing, so the whole crowd is in one fight
//! and on everyone's sensors: the worst case for snapshot building, which is per client over every
//! suit. Prints, for each crowd, the tick and its halves (the simulation and replication) as
//! p50/p99, datagrams and their bytes, and the suit records a pilot is sent a tick and their bits
//! (`docs/SCALE.md`, `docs/NETCODE.md`).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::time::Instant;

use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST};
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PilotKind};
use bc_sector::{Comeback, Control, InputMsg, Metrics, SectorConfig, read_packet};
use bc_sim::SimConfig;
use glam::Vec3;

const WARMUP: u32 = 150;
const TICKS: u32 = 450;

fn run(pilots: usize, drones: usize) {
    let cfg = SectorConfig {
        sim: SimConfig { target_drones: drones as u32, max_suits: 1_000, seed: 7, ..SimConfig::default() },
        max_clients: pilots,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let join = Control::Join {
            slot: l.slot,
            pilot: PilotKind::Human,
            frame: FrameId::Argo,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
            comeback: Comeback::default(),
            launch: None,
        };
        shared.control.push(join).unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let (mut tick, mut sim, mut rep) = (Vec::new(), Vec::new(), Vec::new());
    let (mut bytes, mut packets) = (0u64, 0u64);
    let m = &shared.metrics;
    let (mut records, mut bits) = (0, 0);
    for step in 0..WARMUP + TICKS {
        if step == WARMUP {
            records = Metrics::load(&m.entity_records);
            bits = Metrics::load(&m.entity_bits);
        }
        let next = sector.sim.next_tick();
        for l in &mut leases {
            let mut p = InputPacket {
                ack_snapshot: next.saturating_sub(3),
                client_time_ms: step as u16,
                count: 1,
                ..InputPacket::default()
            };
            let a = (step as f32 * 0.02 + f32::from(l.slot)).sin();
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim: Vec3::new(a, 0.1, 1.0).normalize(),
                thrust: [40, 0, 90],
                buttons: FLIGHT_ASSIST | FIRE_PRIMARY,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let t = Instant::now();
        sector.tick();
        let us = t.elapsed().as_micros() as u64;
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                if step >= WARMUP {
                    bytes += len as u64;
                    packets += 1;
                }
            }
        }
        while shared.notes.pop().is_some() {}
        if step >= WARMUP {
            let split = sector.last_timing();
            tick.push(us);
            sim.push(split.sim_us);
            rep.push(split.replicate_us);
        }
    }
    let records = Metrics::load(&m.entity_records) - records;
    let bits = Metrics::load(&m.entity_bits) - bits;
    let pilot_ticks = f64::from(TICKS) * pilots as f64;
    let secs = f64::from(TICKS) / 30.0;
    println!(
        "{pilots:>4} pilots {drones:>3} drones | tick {} | sim {} | replicate {} | {:>4} B/datagram {:>4.2}/pilot-tick | \
         {:>5.1} suits/pilot-tick {:>5.1} bits/suit | {:>5.1} KB/s a pilot {:>6.2} MB/s out",
        pcts(&mut tick),
        pcts(&mut sim),
        pcts(&mut rep),
        bytes / packets.max(1),
        packets as f64 / pilot_ticks,
        records as f64 / pilot_ticks,
        bits as f64 / records.max(1) as f64,
        bytes as f64 / secs / pilots as f64 / 1000.0,
        bytes as f64 / secs / 1e6,
    );
}

/// "p50/p99" of durations in µs, as ms.
fn pcts(us: &mut [u64]) -> String {
    us.sort_unstable();
    let at = |p: f64| us[((us.len() - 1) as f64 * p) as usize] as f64 / 1000.0;
    format!("{:>5.2}/{:>5.2} ms", at(0.5), at(0.99))
}

fn main() {
    // `cargo bench` passes `--bench`; anything else (a filter) is ignored.
    for (pilots, drones) in [(64, 256), (128, 256), (256, 256), (448, 64)] {
        run(pilots, drones);
    }
}
