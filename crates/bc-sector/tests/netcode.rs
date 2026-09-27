//! End-to-end netcode without sockets: a real `Sector` and a real `ClientCore` connected by a
//! simulated link (latency, jitter, loss). Measures own-suit prediction error and checks the
//! protocol invariants (snapshot size, acks, input redundancy).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::Ordering;

use bc_client_core::{ClientConfig, ClientCore, InputContext};
use bc_proto::buttons::{FLIGHT_ASSIST, MODE};
use bc_proto::control::ControlMsg;
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, Part, PilotKind};
use bc_sector::{Control, InputMsg, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::field::SUIT_CLEARANCE;
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

/// Rams the biggest rock near where it starts, then keeps pressing into it while sliding round it.
fn rock_rammer() -> impl FnMut(&InputContext) -> InputCmd {
    let mut target = None;
    move |ctx: &InputContext| {
        let pos = ctx.predict.state.pos;
        let rock = *target.get_or_insert_with(|| {
            let field = &ctx.predict.field;
            let big = field.rocks().iter().filter(|r| r.radius > 25.0);
            big.min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos))).expect("a big rock").pos
        });
        let t = f64::from(ctx.tick) / 30.0;
        InputCmd {
            aim: (rock - pos).normalize_or(Vec3::Z),
            thrust: [((t * 0.7).sin() * 90.0) as i8, 0, 127],
            buttons: FLIGHT_ASSIST,
            ..InputCmd::default()
        }
    }
}

struct Outcome {
    client: ClientCore,
    /// Own-suit prediction error at each snapshot after warm-up.
    errors: Vec<f32>,
    /// The same, at the snapshots whose own suit was touching a rock.
    touching: Vec<f32>,
    max_len: usize,
    /// Ticks in the last 20 s for which the server had no command from this client.
    missing_late: u64,
    /// Snapshots after warm-up that found the suit in another form than it joined in.
    other_form: usize,
}

/// A real sector and a real client over a 100 ms-RTT, ±20 ms-jitter, 5 %-loss link for 40 s. The
/// client takes datagrams as they arrive but only sends inputs every `input_period` seconds (a
/// browser frame, or a slow agent's think cycle).
fn run(input_period: f64, brain: &mut dyn FnMut(&InputContext) -> InputCmd) -> Outcome {
    run_with(input_period, brain, &[])
}

/// [`run`], with the client's suit missing `lost` parts from the start.
fn run_with(input_period: f64, brain: &mut dyn FnMut(&InputContext) -> InputCmd, lost: &[Part]) -> Outcome {
    run_as(FrameId::Leo, input_period, brain, lost)
}

/// [`run_with`], flying `frame`.
fn run_as(
    frame: FrameId,
    input_period: f64,
    brain: &mut dyn FnMut(&InputContext) -> InputCmd,
    lost: &[Part],
) -> Outcome {
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
            frame,
            faction: Faction::Colonies,
            max_datagram: MAX_DATAGRAM as u16,
        })
        .unwrap();
    let mut up = Link::new(1, 0.05, 0.02, 0.05);
    let mut down = Link::new(2, 0.05, 0.02, 0.05);
    let mut client = ClientCore::new(ClientConfig {
        name: "Heero".into(),
        pilot: PilotKind::Human,
        frame,
        faction: Faction::Colonies,
    });

    let mut t = 0.0f64;
    let mut next_tick = 0.0;
    let mut next_input = 0.0;
    let mut buf = [0u8; 2048];
    let mut errors = Vec::new();
    let mut touching = Vec::new();
    let mut welcomed = false;
    let mut max_len = 0;
    let mut missing_at_20s = None;
    let mut damaged = lost.is_empty();
    let mut other_form = 0;
    while t < 40.0 {
        if !damaged && let Some(own) = client.world.own {
            for p in lost {
                sector.sim.suits.part_hp[own.slot as usize][*p as usize] = 0.0;
            }
            damaged = true;
        }
        for bytes in up.deliver(t) {
            let packet = InputPacket::decode(&bytes).expect("input decodes");
            let _ = lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
        }
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
            sector.tick_at((t * 1e6) as u64);
            if missing_at_20s.is_none() && t >= 20.0 {
                missing_at_20s = Some(shared.metrics.inputs_missing.load(Ordering::Relaxed));
            }
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
                    field_seed: shared.field_seed,
                    field_rocks: shared.field_rocks,
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
        let before = client.stats.snapshots;
        for bytes in down.deliver(t) {
            client.on_datagram(&bytes, t);
        }
        if client.stats.snapshots > before && t > 5.0 {
            errors.push(client.stats.prediction_error);
            let own = client.world.own.expect("own state");
            if client.predict.field.rocks().iter().any(|r| r.touches(own.pos, SUIT_CLEARANCE + 1.0)) {
                touching.push(client.stats.prediction_error);
            }
            other_form += usize::from(own.frame != frame);
        }
        if t >= next_input {
            next_input += input_period;
            for p in client.poll_inputs(t, brain) {
                up.send(t, p);
            }
            client.frame(t, input_period as f32);
        }
        t += 0.001;
    }
    let missing_late =
        shared.metrics.inputs_missing.load(Ordering::Relaxed) - missing_at_20s.expect("ran past 20 s");
    Outcome { client, errors, touching, max_len, missing_late, other_form }
}

fn percentile(errors: &mut [f32], q: f64) -> f32 {
    errors.sort_by(f32::total_cmp);
    errors[((errors.len() - 1) as f64 * q) as usize]
}

#[test]
fn prediction_holds_up_over_a_bad_link() {
    let Outcome { client, mut errors, max_len, missing_late, .. } = run(1.0 / 60.0, &mut weaving_pilot);
    errors.sort_by(f32::total_cmp);
    let p = |q: f64| errors[((errors.len() - 1) as f64 * q) as usize];
    println!(
        "snapshots {}  prediction error p50 {:.4} m  p99 {:.4} m  max {:.4} m  (lead {:.1} ticks, health {}, rtt {:.0} ms, max snapshot {max_len} B, missing inputs {missing_late}/600)",
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
    assert!(missing_late <= 6, "{missing_late} of 600 ticks had no command");
    let own = client.world.own.expect("own state");
    assert!(own.alive);
    assert!((client.predict.state.pos - own.pos).length() < 200.0);
}

/// A client that sends inputs only twice a second (a slow agent, or a browser rendering at 2 fps):
/// the server holds each input echo for up to 500 ms, past what the 8-bit hold field can say, so
/// saturated echoes must not count as RTT samples. Its inputs must still arrive in time.
#[test]
fn bursty_inputs_keep_an_accurate_clock() {
    let Outcome { client, missing_late, .. } = run(0.5, &mut weaving_pilot);
    println!(
        "rtt {:.0} ms, lead {:.1} ticks, health {}, snapshots {}, missing inputs {missing_late}/600",
        client.clock.rtt * 1000.0,
        client.clock.lead,
        client.clock.health,
        client.stats.snapshots
    );
    assert!((client.clock.rtt - 0.1).abs() < 0.03, "rtt estimate {:.3}", client.clock.rtt);
    // The commands still arrive before the server needs them, despite the bursts and the loss.
    assert!(missing_late <= 6, "{missing_late} of 600 ticks had no command");
    assert!(client.world.own.expect("own state").alive);
}

/// The client predicts its suit against the same rocks as the server: ramming one and sliding round
/// it over the bad link mispredicts no more than open flight does.
#[test]
fn prediction_holds_up_against_rocks() {
    let Outcome { client, mut errors, mut touching, .. } = run(1.0 / 60.0, &mut rock_rammer());
    assert!(client.predict.field.len() > 100, "the client has the server's field");
    let all = percentile(&mut errors, 0.99);
    println!(
        "snapshots touching a rock {} of {}  prediction error there p50 {:.4} m  p99 {:.4} m  (p99 overall {all:.4} m)",
        touching.len(),
        errors.len(),
        percentile(&mut touching, 0.5),
        percentile(&mut touching, 0.99),
    );
    assert!(touching.len() > 300, "touching a rock at only {} snapshots", touching.len());
    assert!(percentile(&mut touching, 0.99) < 0.25, "p99 {:.3} m", percentile(&mut touching, 0.99));
    assert!(all < 0.25, "prediction error p99 {all:.3} m");
    assert!(client.world.own.expect("own state").alive);
}

/// A suit with parts shot off flies with weaker AMBAC and thrust. The client predicts it with the
/// factors the server sends, which the server rounds the same way before flying with them.
#[test]
fn prediction_holds_up_for_a_damaged_suit() {
    let lost = [Part::ArmR, Part::Legs, Part::Backpack];
    let Outcome { client, mut errors, .. } = run_with(1.0 / 60.0, &mut weaving_pilot, &lost);
    let own = client.world.own.expect("own state");
    assert!(own.thrust_factor < 0.5 && own.ambac_factor < 0.75, "the damage took: {own:?}");
    let p99 = percentile(&mut errors, 0.99);
    println!("damaged suit: prediction error p50 {:.4} m  p99 {p99:.4} m", percentile(&mut errors, 0.5));
    assert!(p99 < 0.01, "prediction error p99 {p99:.3} m");
}

/// A Wing Zero weaving and changing into Neo-Bird and back every 3 s over the bad link. The client
/// steps each change as the server does, so its prediction stays exact through them.
#[test]
fn prediction_holds_up_through_changes_of_form() {
    let mut brain = |ctx: &InputContext| {
        let mut cmd = weaving_pilot(ctx);
        if (ctx.tick / 90) % 2 == 1 {
            cmd.buttons |= MODE;
        }
        cmd
    };
    let Outcome { client, mut errors, other_form, .. } =
        run_as(FrameId::WingZero, 1.0 / 60.0, &mut brain, &[]);
    let p99 = percentile(&mut errors, 0.99);
    println!(
        "changing form: {other_form} of {} snapshots as Neo-Bird, prediction error p50 {:.4} m  p99 {p99:.4} m",
        errors.len(),
        percentile(&mut errors, 0.5)
    );
    assert!(other_form > 200, "it was a bird for {other_form} snapshots");
    assert!(p99 < 0.01, "prediction error p99 {p99:.3} m");
    assert!(client.world.own.expect("own state").alive);
}
