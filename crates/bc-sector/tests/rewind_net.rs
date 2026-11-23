//! The rewind floor over a simulated link, with real numbers (`bc_sim::config::rewind_floor_q4`):
//! a real `Sector` and a real `ClientCore` (its clock steering its lead and easing its view), the
//! session's part played here (the link's round trip on the lease, as QUIC measures it). An
//! honest client is never cut at 20, 100 or 200 ms, jitter, loss and a sudden drop in delay
//! included; a backtracker (a client claiming a view older than it saw) is cut to what its link
//! explains; a link not measured is left alone; and the tick still never allocates.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::Ordering;
use std::time::Duration;

use bc_alloc::CountingAlloc;
use bc_client_core::{ClientConfig, ClientCore, InputContext};
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::control::ControlMsg;
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PilotKind};
use bc_sector::{Comeback, Control, InputMsg, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::config::{REWIND_SLACK_Q4, TICK_HZ, interp_delay_ticks, rewind_floor_q4};
use bc_sim::math::Rng;
use glam::Vec3;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const HZ: f64 = TICK_HZ as f64;

/// One direction of a link: packets delivered at `send + base ± jitter`, some lost; its delay
/// can change mid-run.
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

/// What a run flies over and how its client claims its views.
#[derive(Clone, Copy)]
struct Run {
    /// Round trip (s), half each way, ± `jitter` each way, `loss` of the packets lost.
    rtt: f64,
    jitter: f64,
    loss: f32,
    /// At this time (s) the round trip becomes this (a queue somewhere drains, say).
    step: Option<(f64, f64)>,
    /// The session measures the link (QUIC's RTT on the lease); else the sector is never told.
    measured: bool,
    /// The client claims its views this many ticks older than it saw them.
    backtrack: u32,
    secs: f64,
}

impl Run {
    fn honest(rtt_ms: f64, jitter_ms: f64) -> Self {
        Self {
            rtt: rtt_ms / 1e3,
            jitter: jitter_ms / 1e3,
            loss: 0.03,
            step: None,
            measured: true,
            backtrack: 0,
            secs: 20.0,
        }
    }
}

/// One real command as the sector took it: its tick, the tick that was next when it first
/// arrived, the view it claimed and the view applied (sixteenths), and the round trip measured.
#[derive(Clone, Copy, Debug)]
struct Taken {
    tick: u32,
    arrived: u32,
    claimed: u32,
    applied: u32,
    rtt_us: u32,
}

struct Seen {
    taken: Vec<Taken>,
    cut: u64,
    /// Heap operations inside the sector's ticks after the first second.
    allocations: u64,
}

impl Seen {
    /// The most any claim went past what its link explains (rtt/2 + interp, from its arrival), ticks.
    fn worst_excess(&self) -> f64 {
        self.taken
            .iter()
            .map(|c| {
                let rtt = f64::from(c.rtt_us) / 1e6;
                let back = f64::from(c.arrived) - f64::from(c.claimed) / 16.0;
                back - rtt * 0.5 * HZ - interp_delay_ticks(rtt)
            })
            .fold(f64::MIN, f64::max)
    }

    /// The most an applied view looked back from its command's tick, ticks.
    fn deepest(&self) -> f64 {
        self.taken.iter().map(|c| f64::from(c.tick) - f64::from(c.applied) / 16.0).fold(0.0, f64::max)
    }
}

fn weaving_pilot(ctx: &InputContext) -> InputCmd {
    let t = f64::from(ctx.tick) / HZ;
    let q = |v: f64| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        aim: Vec3::new((t * 0.4).sin() as f32 * 0.6, (t * 0.23).cos() as f32 * 0.3, 1.0).normalize(),
        thrust: [q((t * 0.9).sin()), q((t * 0.5).cos() * 0.5), q(0.6 + (t * 0.3).sin() * 0.4)],
        buttons: FLIGHT_ASSIST,
        ..InputCmd::default()
    }
}

fn fly(run: Run) -> Seen {
    let cfg = SectorConfig {
        sim: SimConfig { target_drones: 0, seed: 1, ..SimConfig::default() },
        max_clients: 2,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut lease = shared.leases.pop().expect("lease");
    let slot = lease.slot;
    let join = Control::Join {
        slot,
        pilot: PilotKind::Human,
        frame: FrameId::Argo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback: Comeback::default(),
        launch: None,
    };
    shared.control.push(join).unwrap();
    let way = run.rtt / 2.0;
    let mut up = Link::new(1, way, run.jitter, run.loss);
    let mut down = Link::new(2, way, run.jitter, run.loss);
    let mut client = ClientCore::new(ClientConfig {
        name: "Kai".into(),
        pilot: PilotKind::Human,
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    });
    // QUIC's smoothed round trip: an eighth of each new sample.
    let mut srtt = run.rtt;
    let mut rtt_us = 0;
    // Commands heard but not yet taken in by a tick (their views as claimed), and those taken in:
    // the tick that was next when each first arrived.
    let mut heard: HashMap<u32, u32> = HashMap::new();
    let mut arrived: HashMap<u32, (u32, u32)> = HashMap::new();
    let (mut t, mut next_tick, mut next_frame) = (0.0f64, 0.0, 0.0);
    let mut buf = [0u8; 2048];
    let (mut welcomed, mut suit) = (false, None);
    let mut seen = Seen { taken: Vec::new(), cut: 0, allocations: 0 };
    while t < run.secs {
        if let Some((at, rtt)) = run.step
            && t >= at
        {
            up.base = rtt / 2.0;
            down.base = rtt / 2.0;
        }
        for bytes in up.deliver(t) {
            let mut packet = InputPacket::decode(&bytes).expect("input decodes");
            for cmd in &mut packet.cmds[..packet.count as usize] {
                // As far back as the wire carries (15 15/16 ticks before its tick).
                let oldest = (cmd.tick << 4).saturating_sub(255);
                cmd.view_tick_q4 = cmd.view_tick_q4.saturating_sub(run.backtrack << 4).max(oldest);
                heard.entry(cmd.tick).or_insert(cmd.view_tick_q4);
            }
            if run.measured {
                srtt += (up.base + down.base - srtt) / 8.0;
                let rtt = Duration::from_secs_f64(srtt);
                rtt_us = rtt.as_micros() as u32;
                lease.set_link_rtt(rtt);
            }
            let _ = lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
        }
        if t >= next_tick {
            next_tick += 1.0 / HZ;
            let next = sector.sim.next_tick();
            for (tick, claimed) in heard.drain() {
                if tick >= next {
                    arrived.entry(tick).or_insert((next, claimed));
                }
            }
            let ((), n) = bc_alloc::count(|| sector.tick_at((t * 1e6) as u64));
            if t > 1.0 {
                seen.allocations += n;
            }
            if let Some(i) = suit {
                let cmd: InputCmd = sector.sim.suits.input[i];
                if let Some((first, claimed)) =
                    arrived.remove(&cmd.tick).filter(|_| cmd.tick == sector.sim.tick())
                {
                    seen.taken.push(Taken {
                        tick: cmd.tick,
                        arrived: first,
                        claimed,
                        applied: cmd.view_tick_q4,
                        rtt_us,
                    });
                }
            }
            if !welcomed && shared.slots[slot as usize].state() == SlotState::Active {
                welcomed = true;
                suit = shared.slots[slot as usize].suit_id().map(|(idx, _)| usize::from(idx));
                let mut w = [0u8; 64];
                let n = ControlMsg::Welcome {
                    version: PROTOCOL_VERSION,
                    client_slot: slot,
                    tick: sector.sim.tick(),
                    tick_hz: 30,
                    sector: 1,
                    orrery_allowed: true,
                    max_datagram: MAX_DATAGRAM as u16,
                    field_seed: shared.field_seed,
                    field_rocks: shared.field_rocks,
                    flags: 0,
                    landmarks: shared.landmarks,
                    faction: Faction::Colonies,
                }
                .encode(&mut w)
                .unwrap();
                client.on_control(&w[..n]);
            }
            while let Some(n) = read_packet(&mut egress.rings[slot as usize], &mut buf) {
                down.send(t, buf[..n].to_vec());
            }
        }
        for bytes in down.deliver(t) {
            client.on_datagram(&bytes, t);
        }
        // A browser at 60 Hz: inputs polled, then a frame drawn.
        if t >= next_frame {
            next_frame += 1.0 / 60.0;
            for p in client.poll_inputs(t, &mut weaving_pilot) {
                up.send(t, p);
            }
            client.frame(t, 1.0 / 60.0);
        }
        t += 0.001;
    }
    seen.cut = shared.metrics.rewind_cut.load(Ordering::Relaxed);
    seen
}

fn report(what: &str, seen: &Seen) {
    println!(
        "{what}: {} commands, worst {:.2} ticks past rtt/2 + interp, deepest look back {:.2} ticks, {} cut",
        seen.taken.len(),
        seen.worst_excess(),
        seen.deepest(),
        seen.cut
    );
}

#[test]
fn an_honest_client_is_never_cut() {
    let slack = f64::from(REWIND_SLACK_Q4) / 16.0;
    for (rtt, jitter) in [(20.0, 3.0), (100.0, 15.0), (200.0, 25.0)] {
        let seen = fly(Run::honest(rtt, jitter));
        report(&format!("honest at {rtt} ms ± {jitter}"), &seen);
        assert!(seen.taken.len() > 400, "too few commands taken: {}", seen.taken.len());
        assert_eq!(seen.cut, 0, "an honest client at {rtt} ms was cut");
        assert!(
            seen.worst_excess() < slack,
            "at {rtt} ms its claims came within {:.2} ticks of the floor",
            slack - seen.worst_excess()
        );
        assert!(seen.taken.iter().all(|c| c.applied == c.claimed));
        assert_eq!(seen.allocations, 0, "the tick allocated");
    }
}

#[test]
fn an_honest_client_is_never_cut_when_its_delay_drops() {
    // 200 ms, then 80 (a queue on the way drained): the measured round trip falls faster than
    // the client's clock eases onto the newer time.
    let run = Run { step: Some((10.0, 0.08)), ..Run::honest(200.0, 10.0) };
    let seen = fly(run);
    report("200 ms dropping to 80", &seen);
    assert_eq!(seen.cut, 0, "an honest client was cut as its delay fell");
}

#[test]
fn a_backtracker_is_cut_to_what_its_link_explains() {
    for rtt in [20.0, 100.0, 200.0] {
        let seen = fly(Run { backtrack: 12, ..Run::honest(rtt, 5.0) });
        report(&format!("backtracking 12 ticks at {rtt} ms"), &seen);
        // Most of its claims went past the floor (far away, the wire's 16 ticks leave less room to
        // backtrack past what the link explains), and each was put on it.
        let beyond = seen.taken.iter().filter(|c| c.claimed < rewind_floor_q4(c.arrived, c.rtt_us)).count();
        assert!(
            beyond > seen.taken.len() / 2 && seen.cut as usize >= beyond,
            "{beyond} beyond, {} cut",
            seen.cut
        );
        for c in &seen.taken {
            let floor = rewind_floor_q4(c.arrived, c.rtt_us);
            assert_eq!(c.applied, floor.max(c.claimed).min(c.tick << 4), "{c:?}");
        }
        // From where it arrived, no further back than rtt/2 + interp and the slack.
        let rtt_s = rtt / 1e3;
        let explained = rtt_s * 0.5 * HZ + interp_delay_ticks(rtt_s) + f64::from(REWIND_SLACK_Q4) / 16.0;
        let back =
            seen.taken.iter().map(|c| f64::from(c.arrived) - f64::from(c.applied) / 16.0).fold(0.0, f64::max);
        assert!(
            back <= explained + 0.1,
            "at {rtt} ms it still looked back {back:.2} ticks (explained {explained:.2})"
        );
        assert_eq!(seen.allocations, 0, "the tick allocated");
    }
}

#[test]
fn a_link_never_measured_is_left_alone() {
    let seen = fly(Run { backtrack: 12, measured: false, ..Run::honest(20.0, 3.0) });
    report("backtracking 12 ticks, unmeasured", &seen);
    assert_eq!(seen.cut, 0);
    assert!(seen.taken.iter().all(|c| c.applied == c.claimed));
}
