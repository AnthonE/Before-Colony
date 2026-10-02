//! End-to-end netcode without sockets: a real `Sector` and a real `ClientCore` connected by a
//! simulated link (latency, jitter, loss). Measures own-suit prediction error and checks the
//! protocol invariants (snapshot size, acks, input redundancy).
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::Ordering;

use bc_client_core::brains::Plan;
use bc_client_core::chase::{self, ChaseRig, Follow};
use bc_client_core::{ClientConfig, ClientCore, InputContext, LanderBrain, OwnView};
use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRIP, MELEE, MODE};
use bc_proto::control::ControlMsg;
use bc_proto::snapshot::footing;
use bc_proto::{
    Event, Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, Part, PilotKind,
    WeaponKind,
};
use bc_sector::{Comeback, Control, InputMsg, Sector, SectorConfig, SlotState, read_packet};
use bc_sim::bodies::{Bodies, Body};
use bc_sim::content::{ModuleKind, Modules, Systems};
use bc_sim::content::{frame as frame_spec, weapon};
use bc_sim::field::SUIT_CLEARANCE;
use bc_sim::ground::Footing;
use bc_sim::handle::Handle;
use bc_sim::math::{Rng, look_rotation};
use bc_sim::{SimConfig, SuitId};
use glam::{Quat, Vec3};

/// One direction of a lossy link: packets delivered at `send + base ± jitter`, some dropped.
struct Link {
    rng: Rng,
    base: f64,
    jitter: f64,
    loss: f32,
    /// Everything sent in this window (s) is lost.
    outage: Option<(f64, f64)>,
    queue: BinaryHeap<Reverse<(u64, u64, Vec<u8>)>>, // (deliver µs, seq, bytes)
    seq: u64,
}

impl Link {
    fn new(seed: u64, spec: LinkSpec) -> Self {
        let LinkSpec { base, jitter, loss, outage } = spec;
        Self { rng: Rng::new(seed), base, jitter, loss, outage, queue: BinaryHeap::new(), seq: 0 }
    }
    fn send(&mut self, now: f64, bytes: Vec<u8>) {
        if self.rng.next_f32() < self.loss || self.outage.is_some_and(|(from, to)| (from..to).contains(&now))
        {
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
    /// Snapshots after warm-up that had the suit standing on a body, and in a body's grip aloft.
    grounded: usize,
    aloft: usize,
    /// Where the server had the suit after each tick (the sector's frame).
    server: HashMap<u32, Vec3>,
}

/// One direction of a link: base delay, ± jitter (s), the share of packets lost, and a window
/// (s) in which everything is.
#[derive(Clone, Copy)]
struct LinkSpec {
    base: f64,
    jitter: f64,
    loss: f32,
    outage: Option<(f64, f64)>,
}

/// 100 ms round trip, ±20 ms of jitter each way, 5 % loss.
const BAD: LinkSpec = LinkSpec { base: 0.05, jitter: 0.02, loss: 0.05, outage: None };
/// The same delays with nothing lost.
const CLEAN: LinkSpec = LinkSpec { loss: 0.0, ..BAD };

/// What a run flies and how the client behaves.
struct Scenario<'a> {
    frame: FrameId,
    /// Seconds between input polls (a browser frame, or a slow agent's think cycle).
    input_period: f64,
    /// Parts the client's suit is missing from the start.
    lost: &'a [Part],
    /// What's damaged or failed inside the client's suit from the start.
    faults: Systems,
    /// The equipment on it.
    modules: Modules,
    /// No polls in this window (s): a background tab, or a long hitch. Datagrams still arrive.
    stall: Option<(f64, f64)>,
    /// Render like the browser at this rate (Hz): each frame polls inputs, then draws; the 8 ms
    /// network timer polls between frames. Without one, inputs are polled every `input_period`.
    render_hz: Option<f64>,
    link: LinkSpec,
    /// What the suit is put to as it joins (given its index), before its client hears of it: on
    /// a body, say, rather than at the faction's base far from any.
    place: Option<fn(&mut Sector, usize)>,
}

impl Default for Scenario<'_> {
    fn default() -> Self {
        Self {
            frame: FrameId::Leo,
            input_period: 1.0 / 60.0,
            lost: &[],
            faults: Systems::OK,
            modules: Modules::NONE,
            stall: None,
            render_hz: None,
            link: BAD,
            place: None,
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
    let Scenario { frame, input_period, lost, faults, modules, stall, render_hz, link, place } = *sc;
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
    let mut up = Link::new(1, link);
    let mut down = Link::new(2, link);
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
    let mut damaged = lost.is_empty() && faults.is_ok() && modules == Modules::NONE;
    let mut other_form = 0;
    let (mut grounded, mut aloft) = (0, 0);
    let mut server = HashMap::new();
    let mut suit: Option<usize> = None;
    while t < 40.0 {
        if !damaged && let Some(own) = client.world.own {
            for p in lost {
                sector.sim.suits.part_hp[own.slot as usize][*p as usize] = 0.0;
            }
            sector.sim.suits.systems[own.slot as usize] = faults;
            sector.sim.suits.modules[own.slot as usize] = modules;
            damaged = true;
        }
        for bytes in up.deliver(t) {
            let packet = InputPacket::decode(&bytes).expect("input decodes");
            let _ = lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
        }
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
            sector.tick_at((t * 1e6) as u64);
            if let Some(i) = suit {
                server.insert(sector.sim.tick(), sector.sim.suits.flight[i].pos);
            }
            if missing_at_20s.is_none() && t >= 20.0 {
                missing_at_20s = Some(shared.metrics.inputs_missing.load(Ordering::Relaxed));
            }
            if !welcomed && shared.slots[lease.slot as usize].state() == SlotState::Active {
                welcomed = true;
                let (idx, _) = shared.slots[lease.slot as usize].suit_id().expect("a suit");
                suit = Some(usize::from(idx));
                if let Some(place) = place {
                    place(&mut sector, usize::from(idx));
                    // The client first hears of its suit where it was put.
                    while read_packet(&mut egress.rings[lease.slot as usize], &mut buf).is_some() {}
                }
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
            match own.surface.map(|on| on.footing) {
                Some(footing::GROUNDED) => grounded += 1,
                Some(footing::ALOFT) => aloft += 1,
                _ => {}
            }
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
    Outcome {
        client,
        errors,
        error_times,
        ahead,
        touching,
        max_len,
        missing_late,
        other_form,
        drawn,
        grounded,
        aloft,
        server,
    }
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
    let tuned = bc_sim::tuning::own_tuning(&own);
    assert!(tuned.main < 0.5 && tuned.ambac < 0.75, "the damage took: {tuned:?}");
    let p99 = percentile(&mut errors, 0.99);
    println!("damaged suit: prediction error p50 {:.4} m  p99 {p99:.4} m", percentile(&mut errors, 0.5));
    assert!(p99 < 0.01, "prediction error p99 {p99:.3} m");
}

/// A suit whose systems are failing: its main thrusters cough, its tank leaks, its gyros and leg
/// thrusters are weak, its boosters half there and its pilot hurt; and it carries a thruster kit,
/// a G-seat, leg verniers and a cargo rack. The client builds the same stat sheet from the
/// snapshot, coughs on the same ticks and leaks the same kilograms, so its prediction holds as
/// well as for a whole suit.
#[test]
fn prediction_holds_up_for_a_suit_with_failing_systems() {
    use bc_sim::content::System;
    use bc_sim::content::systems::{DAMAGED, FAILED};
    let faults = Systems::OK
        .with(System::MainThrusters, DAMAGED)
        .with(System::Tank, FAILED)
        .with(System::Gyros, DAMAGED)
        .with(System::LegThrusters, DAMAGED)
        .with(System::Boosters, DAMAGED)
        .with(System::Cockpit, DAMAGED);
    let mut modules = Modules::NONE;
    modules.set(1, Some(ModuleKind::GSeat));
    modules.set(3, Some(ModuleKind::CargoRack));
    modules.set(4, Some(ModuleKind::ThrusterKit));
    let sc = Scenario { faults, modules, ..Scenario::default() };
    let Outcome { client, mut errors, .. } = run_scenario(&sc, &mut weaving_pilot);
    let own = client.world.own.expect("own state");
    let tuned = bc_sim::tuning::own_tuning(&own);
    // (The hurt pilot bears 5 g; the G-seat gives one back.)
    assert!(tuned.sputter && tuned.leak_kg_s > 0.0 && tuned.isp < 1.0 && tuned.hold_kg > 0, "{tuned:?}");
    let p99 = percentile(&mut errors, 0.99);
    println!("failing systems: prediction error p50 {:.4} m  p99 {p99:.4} m", percentile(&mut errors, 0.5));
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

const MO_II: Body = Body::Landmark(0);
const HERMIT: Body = Body::Landmark(1);
/// Where on MO-II a lander comes down and paces its square (its frame, out from its middle): on
/// the core 94 m fore of the middle, clear of the pylons and the fore module.
const ON_MO_II: Vec3 = Vec3::new(1.0, 0.45, 0.45);
/// Where on Hermit a suit stands for a static body to set MO-II against: clear of its bowls.
const ON_HERMIT: Vec3 = Vec3::new(0.5, 1.0, 0.3);
/// How fast any point of MO-II's surface moves (its spin and its drift), m/s. A suit standing or
/// hopping on the core where [`ON_MO_II`] puts it moves slower than that (2.2 m/s at most).
const MO_II_SURFACE_SPEED: f32 = 2.87;

/// Stands the suit on `body`, where its surface is out from its middle along `dir`.
fn stand(sector: &mut Sector, i: usize, body: Body, dir: Vec3) {
    let id = SuitId(Handle { idx: i as u16, generation: sector.sim.suits.generation[i] });
    assert!(sector.sim.place_on(id, body, dir), "stood on {body:?}");
}

/// Puts the suit `range` m out from `body`'s surface along `dir`, at rest, facing it.
fn hover(sector: &mut Sector, i: usize, body: Body, dir: Vec3, range: f32) {
    let sim = &mut sector.sim;
    let bodies = Bodies::at(&sim.field, sim.landmarks(), sim.tick());
    let (pose, (p, n)) =
        (bodies.pose(body).expect("a body"), bodies.surface_along(body, dir).expect("a surface"));
    let (at, to) = (pose.to_world(p + n * range), pose.to_world(p));
    let rot = look_rotation((to - at).normalize(), pose.rot * n.cross(Vec3::Y).normalize_or(Vec3::X));
    let f = &mut sim.suits.flight[i];
    (f.pos, f.vel, f.rot, f.ang_vel) = (at, Vec3::ZERO, rot, Vec3::ZERO);
    sim.suits.aim[i] = rot * Vec3::Z;
    sim.suits.input[i].aim = rot * Vec3::Z;
}

fn on_mo_ii(sector: &mut Sector, i: usize) {
    stand(sector, i, MO_II, ON_MO_II);
}

fn on_hermit(sector: &mut Sector, i: usize) {
    stand(sector, i, HERMIT, ON_HERMIT);
}

/// 600 m out over where [`lander`] walks.
fn over_mo_ii(sector: &mut Sector, i: usize) {
    hover(sector, i, MO_II, ON_MO_II, 600.0);
}

/// 1 400 m out over where [`lander`] walks: as far as a [`gunner`] shoots.
fn far_over_mo_ii(sector: &mut Sector, i: usize) {
    hover(sector, i, MO_II, ON_MO_II, 1_400.0);
}

/// 40 m over where [`lander`] walks: a free suit beside the station.
fn beside_mo_ii(sector: &mut Sector, i: usize) {
    hover(sector, i, MO_II, ON_MO_II, 40.0);
}

/// 600 m out over where [`on_hermit`] stands a suit.
fn over_hermit(sector: &mut Sector, i: usize) {
    hover(sector, i, HERMIT, ON_HERMIT, 600.0);
}

/// A lander walking its square on MO-II, hopping every 3 s ([`Plan::Walk`]).
fn lander() -> impl FnMut(&InputContext) -> InputCmd {
    let mut brain = LanderBrain::new(Plan::Walk { body: MO_II, dir_local: ON_MO_II });
    move |ctx: &InputContext| brain.decide(ctx)
}

/// [`lander`], letting go once it has been on MO-II for 5 s: it pushes off, flies out over its
/// spot and comes down on it again.
fn letting_go() -> impl FnMut(&InputContext) -> InputCmd {
    let mut brain = lander();
    let mut since = None;
    move |ctx: &InputContext| {
        let mut cmd = brain(ctx);
        let m = ctx.predict.mover();
        if m.footing == Footing::Free {
            since = None;
        } else if m.footing == Footing::Grounded && ctx.tick - *since.get_or_insert(ctx.tick) >= 150 {
            cmd.buttons &= !GRIP;
        }
        cmd
    }
}

/// Crouches where it stands and keeps still, facing the same way over the deck as it turns (as
/// a still mouse does on a client).
fn crouched(ctx: &InputContext) -> InputCmd {
    let m = ctx.predict.mover();
    let deck = ctx.predict.body_pose(m.anchor.body, f64::from(ctx.tick));
    let aim = deck.map_or(m.flight.rot * Vec3::Z, |p| p.rot * m.anchor.rot * Vec3::Z);
    InputCmd { aim, thrust: [0, -127, 0], buttons: GRIP, ..InputCmd::default() }
}

/// Holds still with flight assist, looking at MO-II.
fn watching(ctx: &InputContext) -> InputCmd {
    let at = ctx.predict.body_pose(MO_II, f64::from(ctx.tick)).map_or(Vec3::ZERO, |p| p.pos);
    InputCmd {
        aim: (at - ctx.predict.state.pos).normalize_or(Vec3::Z),
        buttons: FLIGHT_ASSIST,
        ..InputCmd::default()
    }
}

/// Flies a 30 m square at walking speed with flight assist: a free target to set a rider against.
fn pacing(ctx: &InputContext) -> InputCmd {
    // 8 m/s of the Leo's 220 m/s cruise is a stick of 4.6.
    let stick = [[5, 0, 0], [0, 0, 5], [-5, 0, 0], [0, 0, -5]][(ctx.tick / 112 % 4) as usize];
    InputCmd {
        aim: ctx.predict.state.rot * Vec3::Z,
        thrust: stick,
        buttons: FLIGHT_ASSIST,
        ..InputCmd::default()
    }
}

/// How far a [`gunner`] shoots, m.
const GUN_RANGE: f32 = 1_500.0;

/// Fires the beam rifle at the nearest suit in range whenever it's ready, from its muzzle where it
/// saw it (on a body that moves, on the body as drawn: the view clock's), leading the suit seen at
/// the tick the server judges the shot at by its flight. On a body it keeps its grip and stands
/// its ground.
fn gunner(ctx: &InputContext) -> InputCmd {
    // A press when the server last said it was ready, which the client draws as it fires.
    let ready = ctx.world.own.is_some_and(|o| o.weapon_ready & 1 != 0);
    shoot(ctx, ready, weapon(WeaponKind::BeamRifle).speed)
}

/// Like the [`gunner`], but holding the trigger for 1.5 s, past a full charge, and letting go: a
/// charged shot every 2 s, led for its speed.
fn sniper(ctx: &InputContext) -> InputCmd {
    shoot(ctx, ctx.tick % 60 < 45, weapon(WeaponKind::BeamRifleCharged).speed)
}

/// Aims the rifle at the nearest suit in range, led for a shot at `speed`, and pulls the trigger
/// if `pull`.
fn shoot(ctx: &InputContext, pull: bool, speed: f32) -> InputCmd {
    let me = ctx.predict.mover();
    let grip = if me.footing == Footing::Free { FLIGHT_ASSIST } else { GRIP };
    let view_tick_q4 = ((ctx.view_tick.max(0.0) * 16.0) as u32).min(ctx.tick << 4);
    let seen = ctx.predict.as_seen(&InputCmd { tick: ctx.tick, view_tick_q4, ..InputCmd::default() });
    let rifle = frame_spec(ctx.predict.frame()).loadout[0].expect("a rifle");
    let muzzle = seen.pos + seen.rot * rifle.arm.muzzle();
    let own = ctx.world.own_slot();
    let target = (0..ctx.world.entities.len() as u16)
        .filter(|&j| Some(j) != own)
        .filter_map(|j| ctx.world.pose(j, ctx.resolve_tick))
        .min_by(|a, b| a.pos.distance(muzzle).total_cmp(&b.pos.distance(muzzle)));
    let Some(target) = target.filter(|p| p.pos.distance(muzzle) < GUN_RANGE) else {
        return InputCmd { aim: seen.rot * Vec3::Z, buttons: grip, ..InputCmd::default() };
    };
    let flight = target.pos.distance(muzzle) / speed;
    let lead = target.pos + (target.vel - seen.vel) * flight;
    let fire = if pull { FIRE_PRIMARY } else { 0 };
    InputCmd { aim: (lead - muzzle).normalize(), buttons: grip | fire, ..InputCmd::default() }
}

/// Where the server had a suit at `t` (ticks), between the ticks either side.
fn server_at(server: &HashMap<u32, Vec3>, t: f64) -> Option<Vec3> {
    let k = t.floor() as u32;
    Some(server.get(&k)?.lerp(*server.get(&(k + 1))?, (t - f64::from(k)) as f32))
}

/// A lander walks its square on MO-II as the station rolls and drifts, hopping every 3 s: the
/// client predicts it there in the station's frame, over a clean link as exactly as in open
/// space and over the bad one as well as flying.
#[test]
fn prediction_holds_up_grounded_on_mo_ii() {
    for (link, most, name) in [(CLEAN, 0.01, "clean"), (BAD, 0.25, "bad")] {
        let sc = Scenario { link, place: Some(on_mo_ii), ..Scenario::default() };
        let Outcome { client, mut errors, grounded, aloft, .. } = run_scenario(&sc, &mut lander());
        let p99 = percentile(&mut errors, 0.99);
        println!(
            "{name} link, on MO-II: {grounded} snapshots standing and {aloft} hopping of {}; prediction error (its frame) p50 {:.4} m  p99 {p99:.4} m  max {:.4} m; {} relocations",
            errors.len(),
            percentile(&mut errors, 0.5),
            errors[errors.len() - 1],
            client.predict.relocations,
        );
        assert!(errors.len() > 800, "snapshots measured: {}", errors.len());
        assert!(grounded > 400 && aloft > 200, "standing at {grounded} snapshots, hopping at {aloft}");
        assert_eq!(grounded + aloft, errors.len(), "never let go of MO-II");
        assert!(p99 < most, "{name} link: prediction error p99 {p99:.4} m");
        assert!(client.predict.relocations <= 1, "{} relocations", client.predict.relocations);
        assert!(client.world.own.expect("own state").alive);
    }
}

/// The own suit on MO-II is drawn on the station as the station is drawn (on the view clock),
/// which is where it truly is (at the input clock's time) moved on or back by the station's
/// motion between the two clocks: never further from it than the fastest point of MO-II moves
/// in that time.
#[test]
fn drawn_own_rider_stays_within_the_clock_bound() {
    for (link, name) in [(CLEAN, "clean"), (BAD, "bad")] {
        let sc = Scenario { render_hz: Some(60.0), link, place: Some(on_mo_ii), ..Scenario::default() };
        let Outcome { drawn, server, .. } = run_scenario(&sc, &mut lander());
        assert!(drawn.len() > 30 * 60, "{} frames drawn", drawn.len());
        let (mut over, mut off, mut gaps, mut on, mut seen) = (f32::MIN, 0.0f32, (f64::MAX, 0.0f64), 0, 0);
        // (The last few frames are drawn ahead of the last tick the server stepped.)
        for (_, v, _) in &drawn {
            let Some(truth) = server_at(&server, v.t) else { continue };
            seen += 1;
            let gap = v.t - v.t_view;
            gaps = (gaps.0.min(gap), gaps.1.max(gap));
            let bound = MO_II_SURFACE_SPEED * gap as f32 / 30.0 + 0.05;
            off = off.max(v.pos.distance(truth));
            over = over.max(v.pos.distance(truth) - bound);
            on += usize::from(v.ground.is_some_and(|g| g.body == MO_II));
        }
        println!(
            "{name} link: {seen} frames, {on} on MO-II; the clocks {:.1} to {:.1} ticks apart; drawn up to {off:.3} m from where it was, {:.3} m inside the bound at worst",
            gaps.0, gaps.1, -over
        );
        assert!(seen > drawn.len() - 30, "{seen} of {} frames checked", drawn.len());
        assert_eq!(on, seen, "drawn on MO-II throughout");
        assert!(gaps.1 > 2.0, "the clocks were only {:.1} ticks apart", gaps.1);
        assert!(
            over <= 0.0,
            "{name} link: drawn {over:.3} m further from where it was than the station moves"
        );
    }
}

/// Letting go of MO-II and landing on it again changes how the own suit is drawn (on the station
/// as drawn, or where it is), by the station's motion between the clocks. That is blended out,
/// as a correction is: the camera never cuts, and the drawn suit never jumps (moves further in a
/// frame than it is drawn moving).
#[test]
fn attach_and_detach_blend_without_a_cut() {
    for (hz, link) in [(60.0, CLEAN), (144.0, CLEAN), (60.0, BAD), (144.0, BAD)] {
        let sc = Scenario { render_hz: Some(hz), link, place: Some(on_mo_ii), ..Scenario::default() };
        let Outcome { drawn, mut errors, .. } = run_scenario(&sc, &mut letting_go());
        let (mut attached, mut detached, mut cuts, mut step) = (0, 0, 0, f32::MIN);
        for w in drawn.windows(2) {
            let [(ta, a, _), (tb, b, _)] = w else { unreachable!() };
            match (a.ground.is_some(), b.ground.is_some()) {
                (false, true) => attached += 1,
                (true, false) => detached += 1,
                _ => {}
            }
            cuts += usize::from(b.cut);
            // (As drawn: its flight, the clock's easing and the blending.)
            let speed = a.vel.length().max(b.vel.length());
            step = step.max(a.pos.distance(b.pos) - speed * (tb - ta) as f32);
        }
        let p99 = percentile(&mut errors, 0.99);
        println!(
            "{hz} Hz: landed {attached} times and let go {detached} times; {cuts} cuts; a frame's step beyond the suit's speed {step:.4} m at most; prediction p99 {p99:.4} m"
        );
        assert!(attached >= 2 && detached >= 2, "landed {attached} times, let go {detached} times");
        assert_eq!(cuts, 0, "the camera cut at {hz} Hz");
        assert!(step < 0.05, "the drawn suit stepped {step:.3} m further than its speed at {hz} Hz");
        assert!(p99 < 0.01, "prediction error p99 {p99:.4} m");
    }
}

/// A client of a [`party`]: the suit it joins in, its link, and how it flies. It draws at 60 Hz
/// as the browser does (inputs each frame, and between them on the 8 ms network timer).
struct Guest<'a> {
    frame: FrameId,
    faction: Faction,
    link: LinkSpec,
    /// What its suit is put to as it joins, before its client hears of it ([`Scenario::place`]).
    place: Option<fn(&mut Sector, usize)>,
    max_datagram: u16,
    brain: Box<dyn FnMut(&InputContext) -> InputCmd + 'a>,
}

impl<'a> Guest<'a> {
    fn new(
        faction: Faction,
        link: LinkSpec,
        place: fn(&mut Sector, usize),
        brain: impl FnMut(&InputContext) -> InputCmd + 'a,
    ) -> Self {
        Self {
            frame: FrameId::Leo,
            faction,
            link,
            place: Some(place),
            max_datagram: MAX_DATAGRAM as u16,
            brain: Box::new(brain),
        }
    }
}

/// A frame a guest drew, after warm-up.
struct Frame {
    t: f64,
    /// The view clock's time it was drawn at (ticks): everyone else's.
    view: f64,
    /// Where it drew each guest's suit (not its own).
    suits: Vec<Option<Vec3>>,
}

/// What a guest of a [`party`] saw.
struct Visit {
    client: ClientCore,
    /// Its suit's slot.
    suit: usize,
    frames: Vec<Frame>,
    /// How many snapshots after warm-up carried each suit, by slot.
    carried: HashMap<u16, u32>,
    /// The shots it drew before the server's word of them, by shot sequence: where each left the
    /// muzzle.
    shots: HashMap<u8, Vec3>,
}

/// A suit as the server had it after a tick.
#[derive(Clone, Copy)]
struct Truth {
    pos: Vec3,
    footing: Footing,
    body: Body,
    local: Vec3,
    still: bool,
}

/// What a [`party`] leaves: the sector, what each guest saw, and each guest's suit as the server
/// had it after every tick.
struct Party {
    sector: Sector,
    visits: Vec<Visit>,
    server: HashMap<u32, Vec<Truth>>,
    /// Every beam the server fired: its shooter's slot, its shot sequence, its muzzle and its weapon.
    shots: Vec<(u16, u8, Vec3, WeaponKind)>,
}

impl Party {
    /// Where the server had guest `g`'s suit at `t` (ticks), between the ticks either side.
    fn at(&self, g: usize, t: f64) -> Option<Vec3> {
        let k = t.floor() as u32;
        let (a, b) = (self.server.get(&k)?.get(g)?, self.server.get(&(k + 1))?.get(g)?);
        Some(a.pos.lerp(b.pos, (t - f64::from(k)) as f32))
    }
}

/// Several real clients of one real sector, each over its own link, for 40 s. `before_tick` sees
/// the sector before each tick.
fn party(mut guests: Vec<Guest>, before_tick: &mut dyn FnMut(&mut Sector)) -> Party {
    struct Seat {
        lease: bc_sector::SlotLease,
        client: ClientCore,
        up: Link,
        down: Link,
        suit: Option<usize>,
        next_frame: f64,
        next_timer: f64,
        frames: Vec<Frame>,
        carried: HashMap<u16, u32>,
        shots: HashMap<u8, Vec3>,
    }
    const HZ: f64 = 60.0;
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, seed: 1, ..SimConfig::default() },
        max_clients: 4,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut seats: Vec<Seat> = guests
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let lease = shared.leases.pop().expect("lease");
            shared
                .control
                .push(Control::Join {
                    slot: lease.slot,
                    pilot: PilotKind::Human,
                    frame: g.frame,
                    faction: g.faction,
                    max_datagram: g.max_datagram,
                    comeback: Comeback::default(),
                    launch: None,
                })
                .unwrap();
            let seed = 10 * k as u64;
            Seat {
                lease,
                client: ClientCore::new(ClientConfig {
                    name: format!("Pilot {k}"),
                    pilot: PilotKind::Human,
                    frame: g.frame,
                    faction: g.faction,
                }),
                up: Link::new(seed + 1, g.link),
                down: Link::new(seed + 2, g.link),
                suit: None,
                next_frame: 0.0,
                next_timer: 0.0,
                frames: Vec::new(),
                carried: HashMap::new(),
                shots: HashMap::new(),
            }
        })
        .collect();
    let (mut server, mut shots) = (HashMap::new(), Vec::new());
    let mut seq = sector.sim.events.next_seq();
    let (mut t, mut next_tick) = (0.0f64, 0.0);
    let mut buf = [0u8; 2048];
    while t < 40.0 {
        for s in &mut seats {
            for bytes in s.up.deliver(t) {
                let packet = InputPacket::decode(&bytes).expect("input decodes");
                let _ = s.lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
            }
        }
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
            before_tick(&mut sector);
            sector.tick_at((t * 1e6) as u64);
            let sim = &sector.sim;
            for e in (seq..sim.events.next_seq()).filter_map(|k| sim.events.get(k)) {
                if let Event::BeamSpawn { shooter, shot_seq, origin, weapon, .. } = *e {
                    shots.push((shooter, shot_seq, origin, weapon));
                }
            }
            seq = sim.events.next_seq();
            let truth = |i: usize| Truth {
                pos: sim.suits.flight[i].pos,
                footing: sim.suits.footing[i],
                body: sim.suits.anchor[i].body,
                local: sim.suits.anchor[i].local,
                still: sim.is_still(i),
            };
            if let Some(all) = seats.iter().map(|s| s.suit.map(truth)).collect::<Option<Vec<_>>>() {
                server.insert(sim.tick(), all);
            }
            for (s, g) in seats.iter_mut().zip(&guests) {
                let slot = s.lease.slot as usize;
                if s.suit.is_none() && shared.slots[slot].state() == SlotState::Active {
                    let (idx, _) = shared.slots[slot].suit_id().expect("a suit");
                    s.suit = Some(usize::from(idx));
                    if let Some(place) = g.place {
                        place(&mut sector, usize::from(idx));
                        // The client first hears of its suit where it was put.
                        while read_packet(&mut egress.rings[slot], &mut buf).is_some() {}
                    }
                    let mut w = [0u8; 64];
                    let n = ControlMsg::Welcome {
                        version: PROTOCOL_VERSION,
                        client_slot: s.lease.slot,
                        tick: sector.sim.tick(),
                        tick_hz: 30,
                        sector: 1,
                        zero_allowed: true,
                        max_datagram: g.max_datagram,
                        field_seed: shared.field_seed,
                        field_rocks: shared.field_rocks,
                        flags: 0,
                        landmarks: shared.landmarks,
                    }
                    .encode(&mut w)
                    .unwrap();
                    s.client.on_control(&w[..n]);
                }
                while let Some(n) = read_packet(&mut egress.rings[slot], &mut buf) {
                    assert!(n <= usize::from(g.max_datagram));
                    s.down.send(t, buf[..n].to_vec());
                }
            }
        }
        let suits: Vec<Option<usize>> = seats.iter().map(|s| s.suit).collect();
        for (s, g) in seats.iter_mut().zip(&mut guests) {
            let before = s.client.stats.snapshots;
            for bytes in s.down.deliver(t) {
                s.client.on_datagram(&bytes, t);
            }
            if s.client.stats.snapshots > before && t > 5.0 {
                let w = &s.client.world;
                for (j, e) in w.entities.iter().enumerate() {
                    if e.as_ref().is_some_and(|e| e.latest_tick == w.tick) {
                        *s.carried.entry(j as u16).or_default() += 1;
                    }
                }
            }
            if t >= s.next_frame {
                s.next_frame += 1.0 / HZ;
                for p in s.client.poll_inputs(t, &mut *g.brain) {
                    s.up.send(t, p);
                }
                for b in s.client.world.beams.iter().filter(|b| b.predicted) {
                    s.shots.entry(b.shot_seq).or_insert(b.origin);
                }
                s.client.frame(t, (1.0 / HZ) as f32);
                if t > 5.0 {
                    let view = s.client.render_tick(t);
                    let drawn = |i: &Option<usize>| {
                        i.filter(|&i| Some(i) != s.suit)
                            .and_then(|i| s.client.world.pose(i as u16, view))
                            .map(|p| p.pos)
                    };
                    s.frames.push(Frame { t, view, suits: suits.iter().map(drawn).collect() });
                }
            } else if t >= s.next_timer {
                for p in s.client.poll_inputs(t, &mut *g.brain) {
                    s.up.send(t, p);
                }
                for b in s.client.world.beams.iter().filter(|b| b.predicted) {
                    s.shots.entry(b.shot_seq).or_insert(b.origin);
                }
            }
            if t >= s.next_timer {
                s.next_timer += 0.008;
            }
        }
        t += 0.001;
    }
    let visits = seats
        .into_iter()
        .map(|s| Visit {
            client: s.client,
            suit: s.suit.expect("joined"),
            frames: s.frames,
            carried: s.carried,
            shots: s.shots,
        })
        .collect();
    Party { sector, visits, server, shots }
}

/// A watcher hovering 600 m off MO-II draws a lander walking its square on it (hopping, as the
/// station rolls and drifts) where the server had it at the moment drawn: what lag compensation
/// judges its shots against. A rider is sent and interpolated in the station's frame, so the
/// station's motion costs nothing.
#[test]
fn remote_riders_are_drawn_where_the_server_had_them() {
    for (link, name) in [(CLEAN, "clean"), (BAD, "bad")] {
        let guests = vec![
            Guest::new(Faction::Colonies, link, on_mo_ii, lander()),
            Guest::new(Faction::Colonies, link, over_mo_ii, watching),
        ];
        let party = party(guests, &mut |_| {});
        let watcher = &party.visits[1];
        let (mut off, mut walked, mut hopped) = (Vec::new(), 0, 0);
        for f in &watcher.frames {
            let (Some(drawn), Some(truth)) = (f.suits[0], party.at(0, f.view)) else { continue };
            off.push(drawn.distance(truth));
            let k = f.view.floor() as u32;
            match party.server.get(&k).map(|all| all[0].footing) {
                Some(Footing::Grounded) => walked += 1,
                Some(Footing::Aloft) => hopped += 1,
                _ => {}
            }
        }
        let p99 = percentile(&mut off, 0.99);
        println!(
            "{name} link: {} frames ({walked} walking, {hopped} hopping): drawn off the server's rider p50 {:.4} m  p99 {p99:.4} m  max {:.4} m",
            off.len(),
            percentile(&mut off, 0.5),
            off[off.len() - 1],
        );
        assert!(off.len() > 30 * 60, "{} frames drew the rider", off.len());
        assert!(walked > 600 && hopped > 600, "{walked} frames walking, {hopped} hopping");
        assert!(p99 < 0.02, "{name} link: drawn p99 {p99:.4} m off the server's rider");
        assert_eq!(watcher.client.world.stats.unresolved_bodies, 0);
    }
}

/// A rider crouched still on MO-II is drawn glued to the station as it rolls, over the bad link
/// and through 4 s in which nothing at all reaches the watcher: in the station's frame it doesn't
/// move, so there is nothing to mispredict, however long the news takes.
#[test]
fn still_riders_stay_glued_through_loss() {
    const OUTAGE: (f64, f64) = (20.0, 24.0);
    let blackout = LinkSpec { outage: Some(OUTAGE), ..BAD };
    let guests = vec![
        Guest::new(Faction::Colonies, BAD, on_mo_ii, crouched),
        Guest::new(Faction::Colonies, blackout, over_mo_ii, watching),
    ];
    let party = party(guests, &mut |_| {});
    let watcher = &party.visits[1];
    let bodies = &watcher.client.world.bodies;
    // Once it has crouched and settled (the server finds it still), in MO-II's frame as drawn.
    let (mut first, mut drift, mut off, mut through, mut n) = (None, 0.0f32, 0.0f32, 0, 0);
    for f in watcher.frames.iter().filter(|f| f.t > 8.0) {
        let Some(drawn) = f.suits[0] else { continue };
        let truth = party.server[&(f.view.floor() as u32)][0];
        assert!(truth.still && truth.body == MO_II, "still on MO-II at {:.1}", f.view);
        let local = bodies.pose_at(MO_II, f.view).expect("MO-II").to_local(drawn);
        drift = drift.max(local.distance(*first.get_or_insert(local)));
        off = off.max(local.distance(truth.local));
        through += usize::from((OUTAGE.0 + 0.2..OUTAGE.1).contains(&f.t));
        n += 1;
    }
    let deck = bodies
        .pose_at(MO_II, 8.0 * 30.0)
        .unwrap()
        .pos
        .distance(bodies.pose_at(MO_II, 40.0 * 30.0).unwrap().pos);
    println!(
        "{n} frames ({through} with no news): drawn drifting {drift:.4} m over the deck (which moved {deck:.1} m), {off:.4} m from the server's rider at most"
    );
    assert!(n > 30 * 60 && through > 3 * 60, "{n} frames drew it, {through} in the outage");
    assert!(drift < 0.01, "drifted {drift:.4} m over MO-II");
    // (Its place on the deck is sent to 1.5625 cm: within half that on each axis.)
    assert!(off < 0.0136, "drawn {off:.4} m off the server's rider");
    assert_eq!(watcher.client.world.stats.unresolved_bodies, 0);
}

/// Keeps every suit whole, charged and cool, so a gunner fires as often as its rifle allows and
/// its targets live on.
fn refit(sector: &mut Sector) {
    let s = &mut sector.sim.suits;
    for i in s.alive.iter() {
        let spec = frame_spec(s.frame[i]);
        (s.part_hp[i], s.energy[i], s.heat[i], s.overheated[i]) = (spec.part_hp, spec.energy_cap, 0.0, false);
    }
}

/// How many of guest `g`'s shots hit, of how many.
fn hit_rate(party: &Party, g: usize) -> (f32, u32) {
    let stats = party.sector.sim.suits.stats[party.visits[g].suit];
    (stats.hits as f32 / stats.shots.max(1) as f32, stats.shots)
}

/// A pilot 1 400 m off MO-II shoots at a lander walking its square on the station (and hopping),
/// aiming where it draws the rider, led by the shot's flight. Lag compensation judges it against
/// the rider where the server had it then, which is where it was drawn: it hits as often as it
/// hits a suit flying the same square at the same speed beside the station, free.
#[test]
fn a_pilot_hits_a_walking_rider_it_aims_at() {
    let run = |target: Guest| {
        let guests = vec![target, Guest::new(Faction::Oz, BAD, far_over_mo_ii, gunner)];
        let party = party(guests, &mut refit);
        assert!(party.visits[0].client.world.own.expect("own").alive);
        hit_rate(&party, 1)
    };
    let (rider, shots) = run(Guest::new(Faction::Colonies, BAD, on_mo_ii, lander()));
    let (free, free_shots) = run(Guest::new(Faction::Colonies, BAD, beside_mo_ii, pacing));
    println!(
        "hits on the rider {:.0}% of {shots} shots; on the free suit {:.0}% of {free_shots}",
        rider * 100.0,
        free * 100.0
    );
    assert!(shots > 30 && free_shots > 30, "{shots} and {free_shots} shots");
    assert!(rider >= 0.8, "hit the rider with {:.0}% of its shots", rider * 100.0);
    assert!(rider >= free, "hit the rider {:.0}%, the free suit {:.0}%", rider * 100.0, free * 100.0);
}

/// A rider standing on MO-II shoots at a suit pacing 600 m off it. It saw itself on the station as
/// the station was drawn (the view clock's, some ticks behind where the station truly is), and the
/// server fires the shot from there, on the station as it was then: so each shot leaves the muzzle
/// the pilot saw (where its client drew it), and hits as often as from Hermit, which doesn't move.
#[test]
fn a_rider_shooting_from_mo_ii_hits_what_it_saw() {
    let run = |stand: fn(&mut Sector, usize), over: fn(&mut Sector, usize)| {
        let guests = vec![
            Guest::new(Faction::Colonies, BAD, stand, gunner),
            Guest::new(Faction::Oz, BAD, over, pacing),
        ];
        let party = party(guests, &mut refit);
        let gunner = &party.visits[0];
        // Each shot the server fired, against the same shot as its client drew it.
        let me = gunner.suit as u16;
        let mut off: Vec<f32> = party
            .shots
            .iter()
            .filter(|(shooter, ..)| *shooter == me)
            .filter_map(|(_, seq, origin, _)| Some(gunner.shots.get(seq)?.distance(*origin)))
            .collect();
        let (rate, shots) = hit_rate(&party, 0);
        let lead = gunner.client.clock.lead;
        (rate, shots, percentile(&mut off, 0.99), off.len(), lead)
    };
    let (moving, shots, off, matched, lead) = run(on_mo_ii, over_mo_ii);
    let (fixed, fixed_shots, fixed_off, _, _) = run(on_hermit, over_hermit);
    println!(
        "from MO-II {:.0}% of {shots} shots hit (muzzles {matched} matched, p99 {off:.4} m off the drawn one; lead {lead:.1} ticks); from Hermit {:.0}% of {fixed_shots} (p99 {fixed_off:.4} m)",
        moving * 100.0,
        fixed * 100.0
    );
    assert!(shots > 30 && fixed_shots > 30, "{shots} and {fixed_shots} shots");
    assert!(matched as u32 > shots * 9 / 10, "{matched} of {shots} shots drawn");
    assert!(
        off < 0.01 && fixed_off < 0.01,
        "the shots left {off:.4} m (MO-II), {fixed_off:.4} m (Hermit) off the drawn muzzle"
    );
    assert!(
        (moving - fixed).abs() <= 0.05,
        "hit {:.0}% from MO-II, {:.0}% from Hermit",
        moving * 100.0,
        fixed * 100.0
    );
}

/// Tap fires, hold charges: a rider on Hermit holds its rifle's trigger to a full charge and lets
/// go, and its client draws each charged shot as it leaves, from the muzzle the server fired it
/// from, as it draws a tap's.
#[test]
fn a_charged_shot_is_drawn_as_it_leaves() {
    let guests = vec![
        Guest::new(Faction::Colonies, BAD, on_hermit, sniper),
        Guest::new(Faction::Oz, BAD, over_hermit, pacing),
    ];
    let party = party(guests, &mut refit);
    let sniper = &party.visits[0];
    let me = sniper.suit as u16;
    let charged: Vec<_> = party
        .shots
        .iter()
        .filter(|&&(shooter, .., w)| shooter == me && w == WeaponKind::BeamRifleCharged)
        .collect();
    let mut off: Vec<f32> = charged
        .iter()
        .filter_map(|(_, seq, origin, _)| Some(sniper.shots.get(seq)?.distance(*origin)))
        .collect();
    let (rate, shots) = hit_rate(&party, 0);
    println!(
        "{} charged shots of {shots}, {} drawn as they left (p99 {:.4} m off), {:.0}% of all shots hit",
        charged.len(),
        off.len(),
        percentile(&mut off, 0.99),
        rate * 100.0
    );
    assert!(charged.len() >= 12, "{} charged shots", charged.len());
    assert!(off.len() * 10 >= charged.len() * 9, "{} of {} drawn", off.len(), charged.len());
    assert!(percentile(&mut off, 0.99) < 0.01, "drawn off the muzzle");
}

/// How many suits stand about on Hermit for [`still_riders_yield_bandwidth`], and how many of
/// them walk.
const CROWD: usize = 24;

/// Before the first tick: [`CROWD`] suits on Hermit around where [`on_hermit`] stands one, the
/// first `walking` of them walking off (the rest stand still on their feet).
fn crowd(walking: usize) -> impl FnMut(&mut Sector) {
    let mut done = false;
    move |sector: &mut Sector| {
        if std::mem::replace(&mut done, true) {
            return;
        }
        let sim = &mut sector.sim;
        for k in 0..CROWD {
            let id = sim
                .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::ZERO, Quat::IDENTITY)
                .expect("room");
            let dir =
                ON_HERMIT + Vec3::new((k % 6) as f32 * 0.03 - 0.075, 0.0, (k / 6) as f32 * 0.03 - 0.045);
            assert!(sim.place_on(id, HERMIT, dir));
            if k < walking {
                let aim = sim.suits.flight[id.idx()].rot * Vec3::Z;
                let t = sim.next_tick();
                let walk = InputCmd {
                    tick: t,
                    view_tick_q4: t << 4,
                    aim,
                    thrust: [0, 0, 127],
                    buttons: GRIP,
                    ..InputCmd::default()
                };
                sim.set_input(id, walk);
            }
        }
    }
}

/// A client on a 256-byte budget watches 24 suits on Hermit, more than its snapshots hold. Half
/// stand still: each record of one would say just what the last did (it is sent in Hermit's
/// frame), so it is sent a tenth as often, its track kept alive longer, and the half that walk
/// are refreshed more often for it.
#[test]
fn still_riders_yield_bandwidth() {
    let watch = |walking: usize| {
        let mut watcher = Guest::new(Faction::Colonies, CLEAN, over_hermit, watching);
        watcher.max_datagram = 256;
        let party = party(vec![watcher], &mut crowd(walking));
        let visit = &party.visits[0];
        let crowd: Vec<u16> = (0..party.sector.sim.suits.cap as u16)
            .filter(|&j| usize::from(j) != visit.suit && party.sector.sim.suits.alive.get(usize::from(j)))
            .collect();
        assert_eq!(crowd.len(), CROWD);
        let world = &visit.client.world;
        assert!(crowd.iter().all(|&j| world.entities[usize::from(j)].is_some()), "a track was dropped");
        // Refreshes a second of the walkers and of those standing still (the crowd spawned in order).
        let rate = |js: &[u16]| {
            let n: u32 = js.iter().map(|j| visit.carried.get(j).copied().unwrap_or(0)).sum();
            n as f32 / js.len().max(1) as f32 / 35.0
        };
        let still = crowd[walking..].iter().all(|&j| party.sector.sim.is_still(usize::from(j)));
        assert!(still, "the ones left standing are still");
        (rate(&crowd[..walking]), rate(&crowd[walking..]))
    };
    let (walkers, standing) = watch(CROWD / 2);
    let (all_walking, _) = watch(CROWD);
    println!(
        "of {CROWD} suits on Hermit: walkers refreshed {walkers:.1} Hz beside those standing still ({standing:.1} Hz), {all_walking:.1} Hz when all walk"
    );
    assert!(walkers > 5.0 * standing, "walkers {walkers:.1} Hz, standing {standing:.1} Hz");
    assert!(walkers > 1.5 * all_walking, "walkers {walkers:.1} Hz, {all_walking:.1} Hz when all walk");
    // Often enough to stay well inside a still rider's track's life.
    let stale = bc_client_core::interp::STALE_STILL_TICKS as f32 / 30.0;
    assert!(standing > 3.0 / stale, "standing refreshed {standing:.2} Hz");
}
