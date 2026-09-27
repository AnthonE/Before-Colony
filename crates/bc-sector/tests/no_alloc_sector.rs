//! The sector tick (input drain, simulation, snapshot encoding for 64 clients) never allocates.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_alloc::CountingAlloc;
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, ZERO};
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PilotKind};
use bc_sector::{Control, InputMsg, SectorConfig, read_packet};
use bc_sim::SimConfig;
use glam::Vec3;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn sector_tick_never_allocates() {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 256, seed: 3, ..SimConfig::default() },
        max_clients: 64,
        oracle: true,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, mut oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let frame = if l.slot % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        shared
            .control
            .push(Control::Join {
                slot: l.slot,
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
                max_datagram: MAX_DATAGRAM as u16,
            })
            .unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let mut total = 0u64;
    let mut snapshots = 0u64;
    for step in 0..1_300u32 {
        // Network side (outside the counted region): inputs in, packets out, oracle ring drained.
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
                buttons: FLIGHT_ASSIST | FIRE_PRIMARY | ZERO,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n; // the first 300 ticks let the doll spawner fill the sector
        }
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                assert!(len <= MAX_DATAGRAM);
                snapshots += 1;
            }
        }
        while oracle.pictures.pop().is_ok() {}
    }
    let m = &shared.metrics;
    println!(
        "snapshots {snapshots}, max {} B, pictures {}, alive {}",
        bc_sector::Metrics::load(&m.snapshot_max_bytes),
        bc_sector::Metrics::load(&m.pictures),
        bc_sector::Metrics::load(&m.suits_alive)
    );
    assert!(snapshots > 60_000, "snapshots {snapshots}");
    assert!(bc_sector::Metrics::load(&m.pictures) > 0);
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}

#[test]
fn a_missile_barrage_never_allocates() {
    use bc_proto::SnapshotReader;
    use bc_proto::buttons::{FIRE_SECONDARY, SPECIAL};

    // 32 missile boats (Heavyarms and Sandrocks, each locking the nearest doll) against 128 Mobile
    // Dolls, with Full Open Attacks.
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 128, seed: 9, ..SimConfig::default() },
        max_clients: 32,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let frame = if l.slot % 2 == 0 { FrameId::Heavyarms } else { FrameId::Sandrock };
        shared
            .control
            .push(Control::Join {
                slot: l.slot,
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
                max_datagram: MAX_DATAGRAM as u16,
            })
            .unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let (mut total, mut missiles_seen) = (0u64, 0u64);
    for step in 0..1_200u32 {
        let next = sector.sim.next_tick();
        for (n, l) in leases.iter_mut().enumerate() {
            // Aim at and designate the nearest Oz suit (outside the counted region).
            let me = sector
                .sim
                .suits
                .alive
                .iter()
                .find(|&i| sector.sim.suits.pilot[i] == PilotKind::Human && i % 32 == n);
            let (aim, lock) = me
                .and_then(|i| {
                    let at = sector.sim.suits.flight[i].pos;
                    sector
                        .sim
                        .suits
                        .alive
                        .iter()
                        .filter(|&j| sector.sim.suits.faction[j] == Faction::Oz)
                        .min_by(|&a, &b| {
                            let d = |j: usize| (sector.sim.suits.flight[j].pos - at).length_squared();
                            d(a).total_cmp(&d(b))
                        })
                        .map(|j| ((sector.sim.suits.flight[j].pos - at).normalize_or(Vec3::Z), j as u16))
                })
                .unwrap_or((Vec3::Z, bc_proto::NO_SLOT));
            let mut buttons = FLIGHT_ASSIST | FIRE_SECONDARY | FIRE_PRIMARY;
            if (step + n as u32).is_multiple_of(450) {
                buttons |= SPECIAL;
            }
            let mut p = InputPacket {
                ack_snapshot: next.saturating_sub(3),
                client_time_ms: step as u16,
                count: 1,
                ..InputPacket::default()
            };
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim,
                thrust: [0, 0, 60],
                buttons,
                lock_target: lock,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n;
        }
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                assert!(len <= MAX_DATAGRAM);
                let mut r = SnapshotReader::new(&buf[..len]).expect("a snapshot");
                let (_, _) = (r.own().expect("own"), r.zero().expect("zero"));
                while let Ok(Some(_)) = r.next_event() {}
                while let Ok(Some(_)) = r.next_rock() {}
                while let Ok(Some(_)) = r.next_missile() {
                    missiles_seen += 1;
                }
            }
        }
    }
    println!(
        "peak missiles {}, missile records sent {missiles_seen}, max snapshot {} B",
        sector.sim.peak_missiles,
        bc_sector::Metrics::load(&shared.metrics.snapshot_max_bytes)
    );
    assert!(sector.sim.peak_missiles > 150, "peak missiles {}", sector.sim.peak_missiles);
    assert!(missiles_seen > 10_000, "missiles reached the clients: {missiles_seen}");
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}
