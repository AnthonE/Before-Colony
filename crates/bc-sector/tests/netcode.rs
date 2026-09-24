//! End-to-end netcode without sockets: a real `Sector` and a real `ClientCore` connected by a
//! simulated link (latency, jitter, loss). Measures own-suit prediction error and checks the
//! protocol invariants (snapshot size, acks, input redundancy).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bc_client_core::{ClientConfig, ClientCore, InputContext};
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::control::ControlMsg;
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PilotKind};
use bc_sector::{Control, InputMsg, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::math::Rng;
use glam::Vec3;

/// One direction of a lossy link: packets delivered at `send + base ± jitter`, some dropped.
struct Link {
    rng: Rng,
    base: f64,
    jitter: f64,
    loss: f32,
    queue: BinaryHeap<Reverse<(u64, u64, Vec<u8>)>>, // (deliver µs, seq, bytes)
    seq: u64,
}

impl Link {
    fn new(seed: u64, base: f64, jitter: f64, loss: f32) -> Self {
        Self { rng: Rng::new(seed), base, jitter, loss, queue: BinaryHeap::new(), seq: 0 }
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
        while let Some(Reverse((at, _, _))) = self.queue.peek() {
            if *at as f64 / 1e6 > now {
                break;
            }
            out.push(self.queue.pop().unwrap().0.2);
        }
        out
    }
}

fn weaving_pilot(ctx: &InputContext) -> InputCmd {
    let t = f64::from(ctx.tick) / 30.0;
    let q = |v: f64| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        aim: Vec3::new((t * 0.4).sin() as f32 * 0.6, (t * 0.23).cos() as f32 * 0.3, 1.0).normalize(),
        thrust: [q((t * 0.9).sin()), q((t * 0.5).cos() * 0.5), q(0.6 + (t * 0.3).sin() * 0.4)],
        buttons: FLIGHT_ASSIST,
        ..InputCmd::default()
    }
}

#[test]
fn prediction_holds_up_over_a_bad_link() {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, seed: 1, ..SimConfig::default() },
        max_clients: 4,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut lease = shared.leases.pop().expect("lease");
    shared
        .control
        .push(Control::Join {
            slot: lease.slot,
            pilot: PilotKind::Human,
            frame: FrameId::Leo,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
        })
        .unwrap();
    // 100 ms RTT, ±20 ms jitter, 5 % loss each way.
    let mut up = Link::new(1, 0.05, 0.02, 0.05);
    let mut down = Link::new(2, 0.05, 0.02, 0.05);
    let mut client = ClientCore::new(ClientConfig {
        name: "Heero".into(),
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
    });

    let mut t = 0.0f64;
    let mut next_tick = 0.0;
    let mut next_frame = 0.0;
    let mut buf = [0u8; 2048];
    let mut errors = Vec::new();
    let mut welcomed = false;
    let mut max_len = 0;
    let mut brain = weaving_pilot;
    while t < 40.0 {
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
            for bytes in up.deliver(t) {
                let packet = InputPacket::decode(&bytes).expect("input decodes");
                let _ = lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
            }
            sector.tick_at((t * 1e6) as u64);
            if !welcomed && shared.slots[lease.slot as usize].state() == SlotState::Active {
                welcomed = true;
                let mut w = [0u8; 64];
                let n = ControlMsg::Welcome {
                    version: PROTOCOL_VERSION,
                    client_slot: lease.slot,
                    tick: sector.sim.tick(),
                    tick_hz: 30,
                    sector: 1,
                    zero_allowed: true,
                    max_datagram: MAX_DATAGRAM as u16,
                }
                .encode(&mut w)
                .unwrap();
                client.on_control(&w[..n]);
            }
            while let Some(n) = read_packet(&mut egress.rings[lease.slot as usize], &mut buf) {
                max_len = max_len.max(n);
                down.send(t, buf[..n].to_vec());
            }
        }
        if t >= next_frame {
            next_frame += 1.0 / 60.0;
            let before = client.stats.snapshots;
            for bytes in down.deliver(t) {
                client.on_datagram(&bytes, t);
            }
            if client.stats.snapshots > before && t > 5.0 {
                errors.push(client.stats.prediction_error);
            }
            for p in client.poll_inputs(t, &mut brain) {
                up.send(t, p);
            }
            client.frame(t, 1.0 / 60.0);
        }
        t += 0.001;
    }
    errors.sort_by(f32::total_cmp);
    let p = |q: f64| errors[((errors.len() - 1) as f64 * q) as usize];
    println!(
        "snapshots {}  prediction error p50 {:.4} m  p99 {:.4} m  max {:.4} m  (lead {:.1} ticks, health {}, rtt {:.0} ms, max snapshot {max_len} B)",
        client.stats.snapshots,
        p(0.5),
        p(0.99),
        errors[errors.len() - 1],
        client.clock.lead,
        client.clock.health,
        client.clock.rtt * 1000.0
    );
    assert!(errors.len() > 800, "snapshots measured: {}", errors.len());
    assert!(p(0.99) < 0.25, "prediction error p99 {:.3} m", p(0.99));
    assert!(max_len <= MAX_DATAGRAM);
    assert!((client.clock.rtt - 0.1).abs() < 0.03, "rtt estimate {:.3}", client.clock.rtt);
    let own = client.world.own.expect("own state");
    assert!(own.alive);
    assert!((client.predict.state.pos - own.pos).length() < 200.0);
}
