//! End-to-end netcode without sockets: a real `Sector` and a real `ClientCore` connected by a
//! simulated link (latency, jitter, loss). Measures own-suit prediction error and checks the
//! protocol invariants (snapshot size, acks, input redundancy).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::Ordering;

use bc_client_core::chase::{self, ChaseRig, Follow};
use bc_client_core::{ClientConfig, ClientCore, InputContext, OwnView};
use bc_proto::buttons::{BOOST, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, MODE};
use bc_proto::control::ControlMsg;
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, Part, PilotKind};
use bc_sector::{Comeback, Control, InputMsg, SectorConfig, SlotState, read_packet};
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
    /// Own-suit prediction error at each snapshot after warm-up, with the time it arrived.
    errors: Vec<f32>,
    error_times: Vec<f64>,
    /// At each snapshot after warm-up, how far from the server's suit the prediction first had it
    /// (flying the tick ahead of any news of it): what the drawn suit is corrected by.
    ahead: Vec<f32>,
    /// The same, at the snapshots whose own suit was touching a rock.
    touching: Vec<f32>,
    max_len: usize,
    /// Ticks in the last 20 s for which the server had no command from this client.
    missing_late: u64,
    /// Snapshots after warm-up that found the suit in another form than it joined in.
    other_form: usize,
    /// With a render rate: every frame after warm-up, its time, the own suit as drawn, and where
    /// the newest prediction had it.
    drawn: Vec<(f64, OwnView, Vec3)>,
}

/// One direction of a link: base delay, ± jitter (s), and the share of packets lost.
#[derive(Clone, Copy)]
struct LinkSpec {
    base: f64,
    jitter: f64,
    loss: f32,
}

/// 100 ms round trip, ±20 ms of jitter each way, 5 % loss.
const BAD: LinkSpec = LinkSpec { base: 0.05, jitter: 0.02, loss: 0.05 };
/// The same delays with nothing lost.
const CLEAN: LinkSpec = LinkSpec { loss: 0.0, ..BAD };

/// What a run flies and how the client behaves.
struct Scenario<'a> {
    frame: FrameId,
    /// Seconds between input polls (a browser frame, or a slow agent's think cycle).
    input_period: f64,
    /// Parts the client's suit is missing from the start.
    lost: &'a [Part],
    /// No polls in this window (s): a background tab, or a long hitch. Datagrams still arrive.
    stall: Option<(f64, f64)>,
    /// Render like the browser at this rate (Hz): each frame polls inputs, then draws; the 8 ms
    /// network timer polls between frames. Without one, inputs are polled every `input_period`.
    render_hz: Option<f64>,
    link: LinkSpec,
}

impl Default for Scenario<'_> {
    fn default() -> Self {
        Self {
            frame: FrameId::Leo,
            input_period: 1.0 / 60.0,
            lost: &[],
            stall: None,
            render_hz: None,
            link: BAD,
        }
    }
}

/// A real sector and a real client over a 100 ms-RTT, ±20 ms-jitter, 5 %-loss link for 40 s. The
/// client takes datagrams as they arrive but only sends inputs every `input_period` seconds (a
/// browser frame, or a slow agent's think cycle).
fn run(input_period: f64, brain: &mut dyn FnMut(&InputContext) -> InputCmd) -> Outcome {
    run_scenario(&Scenario { input_period, ..Scenario::default() }, brain)
}

/// [`run`], with the client's suit missing `lost` parts from the start.
fn run_with(input_period: f64, brain: &mut dyn FnMut(&InputContext) -> InputCmd, lost: &[Part]) -> Outcome {
    run_scenario(&Scenario { input_period, lost, ..Scenario::default() }, brain)
}

/// [`run_with`], flying `frame`.
fn run_as(
    frame: FrameId,
    input_period: f64,
    brain: &mut dyn FnMut(&InputContext) -> InputCmd,
    lost: &[Part],
) -> Outcome {
    run_scenario(&Scenario { frame, input_period, lost, ..Scenario::default() }, brain)
}

fn run_scenario(sc: &Scenario, brain: &mut dyn FnMut(&InputContext) -> InputCmd) -> Outcome {
    let Scenario { frame, input_period, lost, stall, render_hz, link } = *sc;
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
            comeback: Comeback::default(),
            launch: None,
        })
        .unwrap();
    let mut up = Link::new(1, link.base, link.jitter, link.loss);
    let mut down = Link::new(2, link.base, link.jitter, link.loss);
    let mut client = ClientCore::new(ClientConfig {
        name: "Heero".into(),
        pilot: PilotKind::Human,
        frame,
        faction: Faction::Colonies,
    });

    let mut t = 0.0f64;
    let mut next_tick = 0.0;
    let mut next_input = 0.0;
    let (mut next_frame, mut next_timer) = (0.0, 0.0);
    let mut drawn = Vec::new();
    let mut buf = [0u8; 2048];
    let mut errors = Vec::new();
    let mut error_times = Vec::new();
    let mut ahead = Vec::new();
    // Each tick's position as the prediction first flew it.
    let mut first: HashMap<u32, Vec3> = HashMap::new();
    let note = |client: &ClientCore, first: &mut HashMap<u32, Vec3>| {
        let newest = client.predict.tick;
        for tick in newest.saturating_sub(40)..=newest {
            if let Some(s) = client.predict.sample(tick) {
                first.entry(tick).or_insert(s.pos);
            }
        }
    };
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
                    flags: 0,
                    landmarks: shared.landmarks,
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
            error_times.push(t);
            let own = client.world.own.expect("own state");
            if let Some(p) = first.get(&client.world.tick) {
                ahead.push(p.distance(own.pos));
            }
            if client.predict.field.rocks().iter().any(|r| r.touches(own.pos, SUIT_CLEARANCE + 1.0)) {
                touching.push(client.stats.prediction_error);
            }
            other_form += usize::from(own.frame != frame);
        }
        if stall.is_some_and(|(from, to)| (from..to).contains(&t)) {
            next_input = t + input_period;
        } else if let Some(hz) = render_hz {
            if t >= next_frame {
                next_frame += 1.0 / hz;
                for p in client.poll_inputs(t, brain) {
                    up.send(t, p);
                }
                note(&client, &mut first);
                client.frame(t, (1.0 / hz) as f32);
                if let Some(v) = client.own_view().filter(|_| t > 5.0) {
                    drawn.push((t, *v, client.predict.state.pos));
                }
            } else if t >= next_timer {
                for p in client.poll_inputs(t, brain) {
                    up.send(t, p);
                }
                note(&client, &mut first);
            }
            if t >= next_timer {
                next_timer += 0.008;
            }
        } else if t >= next_input {
            next_input += input_period;
            for p in client.poll_inputs(t, brain) {
                up.send(t, p);
            }
            note(&client, &mut first);
            client.frame(t, input_period as f32);
        }
        t += 0.001;
    }
    let missing_late =
        shared.metrics.inputs_missing.load(Ordering::Relaxed) - missing_at_20s.expect("ran past 20 s");
    Outcome { client, errors, error_times, ahead, touching, max_len, missing_late, other_form, drawn }
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

/// A client that stops sending for 1.5 s (a background tab) comes back on the timeline: the server
/// flew its suit on stand-ins through the silence (the last command again, then hands-off), and
/// the prediction flies the same through the gap in what it sent instead of stopping there.
#[test]
fn a_stall_keeps_prediction_on_the_timeline() {
    let sc = Scenario { stall: Some((20.0, 21.5)), ..Scenario::default() };
    let Outcome { client, mut errors, error_times, .. } = run_scenario(&sc, &mut weaving_pilot);
    // Once the first commands after the stall have reached the server (a round trip, plus the
    // lead's worth of commands it had already flown on stand-ins).
    let mut after: Vec<f32> =
        errors.iter().zip(&error_times).filter(|(_, t)| **t > 22.0).map(|(e, _)| *e).collect();
    let (all, since) = (percentile(&mut errors, 0.99), percentile(&mut after, 0.99));
    println!("stalled 1.5 s: prediction error p99 {all:.4} m overall, {since:.4} m after it");
    // As exact as a damaged suit's or a change of form's: nothing the prediction can't foresee.
    assert!(since < 0.01, "prediction error after the stall p99 {since:.4} m");
    assert!(all < 0.25, "prediction error p99 {all:.3} m");
    assert_eq!(client.predict.tick, client.inputs.newest, "the prediction is at the newest command");
}

/// Keyboard flying in a 9 s cycle, flight assist on, aiming along +z: W and Shift for 3 s, W alone
/// for 1 s, then hands off. The suit sprints, eases back to cruise, and stops.
fn sprinter(ctx: &InputContext) -> InputCmd {
    let t = f64::from(ctx.tick) / 30.0 % 9.0;
    let (forward, boost) = if t < 3.0 {
        (127, BOOST)
    } else if t < 4.0 {
        (127, 0)
    } else {
        (0, 0)
    };
    InputCmd { aim: Vec3::Z, thrust: [0, 0, forward], buttons: FLIGHT_ASSIST | boost, ..InputCmd::default() }
}

/// Flying fast and stopping, as the browser draws it at 60 and 144 Hz. The own suit is drawn
/// between the ticks it has flown, so along its flight it never steps back (no rubber band), how
/// fast it moves changes only as its acceleration changes it, and seen from the chase camera it
/// doesn't surge toward and away at the tick rate.
#[test]
fn sprint_and_stop_draws_smoothly() {
    // A Wing Zero boosting on a light tank: no more than 16 g.
    const MOST: f32 = 16.0 * 9.806_65;
    const TICK: f32 = 1.0 / 30.0;
    for hz in [60.0, 144.0] {
        let sc =
            Scenario { frame: FrameId::WingZero, render_hz: Some(hz), link: CLEAN, ..Scenario::default() };
        let Outcome { drawn, mut errors, .. } = run_scenario(&sc, &mut sprinter);
        assert!(drawn.len() > (30.0 * hz) as usize, "{} frames drawn", drawn.len());
        // How fast the drawn suit moves over each frame: it steps back, or changes by more than a
        // tick of acceleration, only if it's drawn in steps (or corrected).
        let (mut back, mut kink, mut lag, mut top) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for w in drawn.windows(3) {
            let [(ta, a, _), (tb, b, _), (tc, c, newest)] = w else { unreachable!() };
            back = back.max(b.pos.z - c.pos.z);
            let before = (b.pos - a.pos) / (tb - ta) as f32;
            let after = (c.pos - b.pos) / (tc - tb) as f32;
            kink = kink.max(after.distance(before));
            lag = lag.max(c.pos.distance(*newest) - 2.0 * c.flight_vel.length() * TICK);
            top = top.max(c.flight_vel.length());
        }
        // Seen from the chase camera: how much nearer or further the suit is from one frame to the
        // next. The camera trails by acceleration / ω², so that changes no faster than the
        // acceleration does.
        let mut rig = ChaseRig::default();
        let (mut surge, mut prev): (f32, Option<(f64, f32)>) = (0.0, None);
        for (t, v, _) in &drawn {
            let dt = prev.map_or(1.0 / hz, |(p, _)| t - p) as f32;
            let f = Follow {
                pos: v.pos,
                vel: v.vel,
                aim: Vec3::Z,
                up: v.rot * Vec3::Y,
                cut: v.cut,
                ground: false,
            };
            let cut = rig.step(&f, dt);
            let gap = (v.pos - rig.pos).dot(Vec3::Z);
            if let (Some((_, p)), false) = (prev, cut) {
                surge = surge.max((gap - p).abs() / dt);
            }
            prev = Some((*t, gap));
        }
        let p99 = percentile(&mut errors, 0.99);
        println!(
            "{hz} Hz, top speed {top:.0} m/s: stepped back {back:.4} m, pace changed {kink:.2} m/s a frame, lagged {lag:.3} m, surged {surge:.2} m/s; prediction p99 {p99:.4} m"
        );
        // Fast enough that a suit drawn tick by tick would step 5 m at a time.
        assert!(top > 150.0, "sprinted to only {top:.0} m/s");
        assert!(back < 0.01, "the drawn suit stepped back {back:.3} m at {hz} Hz");
        let most_kink = MOST * TICK + 0.5;
        assert!(
            kink < most_kink,
            "the drawn suit's pace changed {kink:.2} m/s in a frame at {hz} Hz (at most {most_kink:.2})"
        );
        assert!(lag < 0.5, "drawn {lag:.2} m further behind the newest prediction than two ticks");
        let most_surge = MOST / chase::OMEGA + 1.0;
        assert!(surge < most_surge, "the suit surged at {surge:.1} m/s against the camera at {hz} Hz");
        assert!(p99 < 0.01, "prediction error p99 {p99:.4} m on a clean link");
    }
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

/// Weaves, strikes every two seconds (the blade's lunge drives the suit on) and fires the secondary
/// in bursts between, so the arms are busy much of the time and the turns slow with them.
fn swordsman(ctx: &InputContext) -> InputCmd {
    let mut cmd = weaving_pilot(ctx);
    let k = ctx.tick % 60;
    if k < 2 {
        cmd.buttons |= MELEE;
    }
    if (20..35).contains(&k) {
        cmd.buttons |= FIRE_SECONDARY;
    }
    cmd
}

/// Strikes lunge, and busy arms slow AMBAC's turning. The client rolls its suit's arms on tick by
/// tick as the server does, so even the ticks it flies ahead of any news (what's drawn) are where
/// the server's suit turns out to be, over the bad link.
#[test]
fn prediction_holds_up_through_strikes_and_fire() {
    for frame in [FrameId::Leo, FrameId::Deathscythe, FrameId::Sandrock] {
        let Outcome { client, mut errors, mut ahead, .. } = run_as(frame, 1.0 / 60.0, &mut swordsman, &[]);
        let p99 = percentile(&mut errors, 0.99);
        let ahead_p99 = percentile(&mut ahead, 0.99);
        println!(
            "{frame:?} striking and firing: prediction error p99 {p99:.4} m; flown ahead p50 {:.4} m  p99 {ahead_p99:.4} m  max {:.4} m",
            percentile(&mut ahead, 0.5),
            ahead[ahead.len() - 1],
        );
        assert!(ahead.len() > 800, "snapshots measured: {}", ahead.len());
        assert!(p99 < 0.01, "{frame:?}: prediction error p99 {p99:.3} m");
        assert!(ahead_p99 < 0.01, "{frame:?}: flown ahead, p99 {ahead_p99:.3} m off");
        assert!(client.world.own.expect("own state").alive);
    }
}
