//! Wreckage and rocks over the network: a real `Sector` and a real `ClientCore` on a simulated
//! lossy link. Chunks reach the client exactly as the server moves them (across bounces), go away
//! when they're gone, a kill hands its wreck to its hulk, changed rocks arrive, a shattered rock is
//! gone for prediction too, and a miner agent earns its living.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bc_client_core::world::ObjectMotion;
use bc_client_core::{ClientConfig, ClientCore, InputContext};
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST};
use bc_proto::control::ControlMsg;
use bc_proto::{
    ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, Part,
    PilotKind, Segment,
};
use bc_sector::{Control, InputMsg, Sector, SectorConfig, SlotState, read_packet};
use bc_sim::SimConfig;
use bc_sim::chunks::Motion;
use bc_sim::math::{Rng, look_rotation};
use glam::{Quat, Vec3};

/// One direction of a lossy link: packets delivered at `send + base ± jitter`, some dropped.
struct Link {
    rng: Rng,
    base: f64,
    jitter: f64,
    loss: f32,
    queue: BinaryHeap<Reverse<(u64, u64, Vec<u8>)>>,
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

/// A sector with one client (a Leo) over a 100 ms-RTT, 5 %-loss link, run for `secs`. `each_tick`
/// runs after every server tick, with the client's suit slot once it has one.
fn run(
    secs: f64,
    brain: &mut dyn FnMut(&InputContext) -> InputCmd,
    each_tick: &mut dyn FnMut(&mut Sector, &ClientCore, Option<usize>),
) -> (Sector, ClientCore) {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 0, seed: 5, ..SimConfig::default() },
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
    let mut up = Link::new(11, 0.05, 0.02, 0.05);
    let mut down = Link::new(12, 0.05, 0.02, 0.05);
    let mut client = ClientCore::new(ClientConfig {
        name: "Duo".into(),
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
    });
    let (mut t, mut next_tick, mut next_input) = (0.0f64, 0.0, 0.0);
    let mut buf = [0u8; 2048];
    let mut welcomed = false;
    while t < secs {
        for bytes in up.deliver(t) {
            let packet = InputPacket::decode(&bytes).expect("input decodes");
            let _ = lease.input.push(InputMsg { packet, recv_us: (t * 1e6) as u64 });
        }
        if t >= next_tick {
            next_tick += 1.0 / 30.0;
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
                    field_seed: shared.field_seed,
                    field_rocks: shared.field_rocks,
                }
                .encode(&mut w)
                .unwrap();
                client.on_control(&w[..n]);
            }
            let me = client.world.own.map(|o| o.slot as usize);
            each_tick(&mut sector, &client, me);
            while let Some(n) = read_packet(&mut egress.rings[lease.slot as usize], &mut buf) {
                assert!(n <= MAX_DATAGRAM, "snapshot of {n} bytes");
                down.send(t, buf[..n].to_vec());
            }
        }
        for bytes in down.deliver(t) {
            client.on_datagram(&bytes, t);
        }
        if t >= next_input {
            next_input += 1.0 / 60.0;
            for p in client.poll_inputs(t, brain) {
                up.send(t, p);
            }
            client.frame(t, 1.0 / 60.0);
        }
        t += 0.001;
    }
    (sector, client)
}

fn hold_still(_: &InputContext) -> InputCmd {
    InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() }
}

fn ore(seed: u8) -> ChunkDesc {
    ChunkDesc { kind: ChunkKind::Ore { ore: seed % 4 }, seed, mass_kg: 300 + 10 * u32::from(seed) }
}

#[test]
fn chunks_reach_the_client_exactly_across_bounces() {
    // Every segment each chunk has had on the server, in order.
    let mut history: Vec<(usize, Vec<Motion>)> = Vec::new();
    let (sector, client) = run(20.0, &mut hold_still, &mut |sector, _, me| {
        let Some(me) = me else { return };
        let sim = &mut sector.sim;
        if history.is_empty() {
            let at = sim.suits.flight[me].pos;
            let t = sim.tick();
            // Chunks thrown at the rock nearest the pilot from all round (they bounce off), and
            // some drifting nearby.
            let rock = *sim
                .field
                .rocks()
                .iter()
                .min_by(|a, b| a.pos.distance(at).total_cmp(&b.pos.distance(at)))
                .unwrap();
            assert!(rock.pos.distance(at) < 2_600.0, "nearest rock {} m off", rock.pos.distance(at));
            for k in 0..12u8 {
                let a = f32::from(k) * 0.52;
                let dir = Vec3::new(a.cos(), 0.2, a.sin()).normalize();
                let pos = rock.pos - dir * (rock.radius + 60.0 + 5.0 * f32::from(k));
                let vel = dir * (20.0 + f32::from(k));
                let seg = Segment { t0: t, pos, vel, rot: Quat::IDENTITY, spin: Vec3::new(0.1, 0.4, 0.0) }
                    .quantized();
                let id = sim.chunks.spawn(ore(k), Motion::Free(seg), t + 9_000, t).unwrap();
                history.push((id as usize, vec![Motion::Free(seg)]));
            }
            for k in 0..6u8 {
                let pos = at + Vec3::new(200.0 + 40.0 * f32::from(k), 30.0, 100.0);
                let seg =
                    Segment { t0: t, pos, vel: Vec3::new(-3.0, 0.5, 1.0), ..Segment::default() }.quantized();
                let id = sim.chunks.spawn(ore(100 + k), Motion::Free(seg), t + 9_000, t).unwrap();
                history.push((id as usize, vec![Motion::Free(seg)]));
            }
        }
        for (k, motions) in &mut history {
            if motions.last() != Some(&sim.chunks.motion[*k]) {
                motions.push(sim.chunks.motion[*k]);
            }
        }
    });
    let sim = &sector.sim;
    let mut bounced = 0;
    for (k, motions) in &history {
        assert!(sim.chunks.alive.get(*k), "chunk {k} went");
        let track = client.world.objects[*k].as_ref().unwrap_or_else(|| panic!("the client lacks chunk {k}"));
        let as_client = |m: &Motion| match *m {
            Motion::Free(seg) => ObjectMotion::Free(seg),
            Motion::Held { holder, right, rot, since } => ObjectMotion::Held { holder, right, rot, since },
        };
        // The newest segment, and the one before it, are the server's to the bit, so every pose
        // at every tick agrees.
        assert_eq!(track.motion, as_client(motions.last().unwrap()), "chunk {k}");
        if motions.len() > 1 {
            bounced += 1;
            assert_eq!(
                track.prev,
                Some(as_client(&motions[motions.len() - 2])),
                "chunk {k} before its bounce"
            );
        }
        let ObjectMotion::Free(seg) = track.motion else { unreachable!() };
        let Motion::Free(server) = sim.chunks.motion[*k] else { unreachable!() };
        for dt in 0..300 {
            let t = f64::from(sim.tick() + dt);
            assert_eq!(bc_sim::chunks::segment_pos(&seg, t), bc_sim::chunks::segment_pos(&server, t));
        }
    }
    assert!(bounced >= 8, "only {bounced} of 12 chunks bounced off the rock");
}

#[test]
fn gone_chunks_leave_the_client() {
    let mut spawned = false;
    let mut most = 0;
    let (_, client) = run(15.0, &mut hold_still, &mut |sector, client, me| {
        most = most.max(client.world.objects.iter().flatten().count());
        let Some(me) = me else { return };
        if spawned {
            return;
        }
        spawned = true;
        let sim = &mut sector.sim;
        let (at, t) = (sim.suits.flight[me].pos, sim.tick());
        for k in 0..20u8 {
            let pos = at + Vec3::new(f32::from(k) * 15.0 - 150.0, 60.0, 250.0);
            let seg = Segment { t0: t, pos, ..Segment::default() };
            sim.chunks.spawn(ore(k), Motion::Free(seg.quantized()), t + 150, t).unwrap();
        }
    });
    assert_eq!(most, 20, "the client saw {most} of the 20 chunks");
    let left = client.world.objects.iter().flatten().count();
    assert_eq!(left, 0, "{left} chunks outlived their expiry on the client");
}

#[test]
fn a_kill_hands_the_wreck_to_its_hulk() {
    use std::cell::Cell;
    use std::rc::Rc;
    let target: Rc<Cell<Option<u16>>> = Rc::new(Cell::new(None));
    let aim_at = target.clone();
    // The pilot fires at the target whenever it can see it.
    let mut brain = move |ctx: &InputContext| {
        let me = ctx.predict.state.pos;
        let Some(slot) = aim_at.get() else { return hold_still(ctx) };
        let Some(p) = ctx.world.pose(slot, ctx.resolve_tick) else { return hold_still(ctx) };
        InputCmd {
            aim: (p.pos - me).normalize(),
            buttons: FLIGHT_ASSIST | FIRE_PRIMARY,
            ..InputCmd::default()
        }
    };
    let mut hulk = None;
    let (mut hidden_while_wreck, mut shown_after) = (false, false);
    let (sector, client) = run(12.0, &mut brain, &mut |sector, client, me| {
        let Some(me) = me else { return };
        let sim = &mut sector.sim;
        if target.get().is_none() {
            let f = sim.suits.flight[me];
            let pos = f.pos + f.rot * Vec3::new(0.0, 0.0, 400.0);
            let id = sim
                .spawn_at(
                    FrameId::Taurus,
                    Faction::Oz,
                    PilotKind::Human,
                    pos,
                    look_rotation(f.pos - pos, Vec3::Y),
                )
                .unwrap();
            sim.suits.part_hp[id.idx()] = [1.0; Part::COUNT];
            target.set(Some(id.idx() as u16));
        }
        let slot = target.get().unwrap();
        if let Some(&k) = client.world.hulks.get(&slot) {
            hulk = Some(k);
            if client.world.objects[k as usize].is_some() {
                let wreck = client.world.entity(slot).is_some();
                if wreck {
                    hidden_while_wreck |= client.world.wreck_on_show(k);
                } else {
                    shown_after |= !client.world.wreck_on_show(k);
                }
            }
        }
        // The server's wreck sits where its hulk is.
        let j = slot as usize;
        if !sim.suits.alive.get(j) && sim.suits.used.get(j) {
            let (h, _) = sim.suits.hulk[j];
            if sim.chunks.is_alive(h) {
                assert!(sim.chunk_pose(h as usize).0.distance(sim.suits.flight[j].pos) < 1e-3);
            }
        }
    });
    let k = hulk.expect("the client heard of the kill and its hulk");
    let track = client.world.objects[k as usize].expect("the client has the hulk");
    assert!(matches!(track.desc.kind, ChunkKind::Hulk { frame: FrameId::Taurus, faction: Faction::Oz, .. }));
    assert!(sector.sim.chunks.is_alive(k));
    assert!(hidden_while_wreck, "the hulk was drawn on top of the wreck");
    assert!(shown_after, "the hulk never took over from the wreck");
}

#[test]
fn changed_rocks_reach_the_client() {
    let mut done = false;
    let (sector, client) = run(8.0, &mut hold_still, &mut |sector, _, me| {
        if me.is_none() || done || sector.sim.tick() < 60 {
            return;
        }
        done = true;
        let r = &mut sector.sim.rocks;
        r.hp[3] *= 0.5;
        r.ore_kg[3] /= 4;
        r.touch(3);
        r.destroyed.set(5, true);
        r.regrow_at[5] = u32::MAX;
        r.touch(5);
    });
    for i in [3u16, 5] {
        assert_eq!(client.world.rocks.get(&i), Some(&sector.sim.rock_state(i as usize)), "rock {i}");
    }
    assert!(client.world.rocks[&5].destroyed);
    assert_eq!(client.world.rocks.len(), 2, "only changed rocks are sent");
}

/// A pilot towing a 6 t hulk: the extra mass reaches the client in its own state, and its
/// prediction flies the heavier suit.
#[test]
fn towing_is_predicted() {
    use bc_proto::buttons::GRAB;
    use bc_sim::content::ArmSlot;
    let mut placed = false;
    let mut errors = Vec::new();
    let mut weave = |ctx: &InputContext| {
        let t = f64::from(ctx.tick) / 30.0;
        let q = |v: f64| (v.clamp(-1.0, 1.0) * 127.0) as i8;
        InputCmd {
            aim: Vec3::new((t * 0.4).sin() as f32 * 0.6, (t * 0.23).cos() as f32 * 0.3, 1.0).normalize(),
            thrust: [q((t * 0.9).sin()), q((t * 0.5).cos() * 0.5), q(0.6 + (t * 0.3).sin() * 0.4)],
            buttons: FLIGHT_ASSIST | GRAB,
            ..InputCmd::default()
        }
    };
    let (sector, client) = run(30.0, &mut weave, &mut |sector, client, me| {
        let Some(me) = me else { return };
        let sim = &mut sector.sim;
        if !placed {
            placed = true;
            let f = sim.suits.flight[me];
            let hand = f.pos + f.rot * ArmSlot::Left.muzzle();
            let desc = ChunkDesc {
                kind: ChunkKind::Hulk { frame: FrameId::Leo, faction: Faction::Oz, parts: 0b11_1111 },
                seed: 1,
                mass_kg: 6_000,
            };
            let at = hand - f.rot * Vec3::X * (bc_sim::chunks::radius(&desc) + 1.0);
            let t = sim.tick();
            let seg = Segment { t0: t, pos: at, vel: f.vel, ..Segment::default() }.quantized();
            sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap();
        }
        if sim.tick() > 150 && client.world.own.is_some_and(|o| o.extra_mass_kg >= 6_000) {
            errors.push(client.stats.prediction_error);
        }
    });
    let me = client.world.own.expect("own state");
    assert!(sector.sim.held_chunk(me.slot as usize).is_some(), "the hulk isn't in tow");
    assert_eq!(me.extra_mass_kg, 6_000);
    assert!(errors.len() > 500, "towing for only {} ticks", errors.len());
    errors.sort_by(f32::total_cmp);
    let p99 = errors[errors.len() * 99 / 100];
    println!("towing: prediction error p99 {p99:.4} m over {} ticks", errors.len());
    assert!(p99 < 0.25, "prediction error p99 {p99:.3} m while towing");
}

/// A shattered rock is gone for the client's prediction too: coasting straight through where it
/// was mispredicts no more than open flight does.
#[test]
fn a_shattered_rock_is_flown_through() {
    use bc_sim::collide::segment_near_point;
    use bc_sim::field::{Field, SUIT_CLEARANCE};
    let cfg = SimConfig::default();
    let field = Field::generate(cfg.field_seed, cfg.field_rocks);
    // A big rock well clear of the colony, with nothing else near a run through it along +X.
    let run_of = |r: &bc_sim::field::Rock| {
        let d = Vec3::X * (r.radius + SUIT_CLEARANCE + 150.0);
        (r.pos - d, r.pos + d)
    };
    let (i, rock) = (0..field.len())
        .map(|i| (i, field.rocks()[i]))
        .find(|(i, r)| {
            let (a, b) = run_of(r);
            r.radius > 15.0
                && r.pos.y > 0.0
                && field.rocks().iter().enumerate().all(|(j, o)| {
                    j == *i || !segment_near_point(a, b, o.pos, o.radius + SUIT_CLEARANCE + 20.0)
                })
        })
        .expect("a rock with a clear run through it");
    let mut coast = |_: &InputContext| InputCmd { aim: Vec3::X, ..InputCmd::default() };
    let mut shattered_at = None;
    let mut errors = Vec::new();
    let (sector, client) = run(12.0, &mut coast, &mut |sector, client, me| {
        let Some(me) = me else { return };
        let sim = &mut sector.sim;
        let t = sim.tick();
        match shattered_at {
            None if t >= 60 => {
                shattered_at = Some(t);
                let f = &mut sim.suits.flight[me];
                f.pos = run_of(&rock).0;
                f.vel = Vec3::X * 60.0;
                sim.rocks.destroyed.set(i, true);
                sim.rocks.regrow_at[i] = t + 90_000;
                sim.rocks.touch(i);
                sim.field.set_dead(i, true);
            }
            Some(s) if t > s + 30 => errors.push(client.stats.prediction_error),
            _ => {}
        }
    });
    errors.sort_by(f32::total_cmp);
    let p99 = errors[errors.len() * 99 / 100];
    println!("through a shattered rock: prediction error p99 {p99:.4} m over {} ticks", errors.len());
    let me = client.world.own.expect("own state");
    let pos = sector.sim.suits.flight[me.slot as usize].pos;
    assert!(
        pos.x > rock.pos.x + rock.radius + SUIT_CLEARANCE,
        "never got through: {pos:?}, rock at {:?}",
        rock.pos
    );
    assert!(client.world.rocks.get(&(i as u16)).is_some_and(|r| r.destroyed), "the client never heard");
    assert!(client.predict.field.is_dead(i), "the client's prediction still has the rock");
    assert!(p99 < 0.25, "prediction error p99 {p99:.3} m");
}

/// A miner agent left to itself in an empty sector, over the lossy link: it cuts rocks apart with
/// its saber, stows the ore, and sells it at the dock (round the colony) within ten minutes.
#[test]
fn a_miner_earns_credits() {
    use bc_client_core::MinerBrain;
    let mut brain = MinerBrain::new();
    let (mut broke, mut most_kg, mut sold_at) = (0, 0u32, None);
    let (sector, client) = run(600.0, &mut |ctx| brain.decide(ctx), &mut |sector, _, me| {
        let Some(me) = me else { return };
        let sim = &sector.sim;
        most_kg = most_kg.max(sim.suits.cargo_kg[me].iter().map(|&kg| u32::from(kg)).sum());
        broke = sim.rocks.destroyed.iter().count();
        if sold_at.is_none() && sim.suits.credits[me] > 0 {
            sold_at = Some(sim.tick());
        }
    });
    let me = client.world.own.expect("own state");
    println!(
        "miner: {broke} rocks broken, hold up to {most_kg} kg, first sale at {:?} s, {} credits in 10 min",
        sold_at.map(|t| t / 30),
        me.credits
    );
    assert!(broke > 0, "never broke a rock");
    assert!(sold_at.is_some(), "never sold anything (hold up to {most_kg} kg)");
    assert_eq!(me.credits, sector.sim.suits.credits[me.slot as usize]);
}
