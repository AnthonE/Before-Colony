//! The sector: one simulation plus its clients, advanced by [`Sector::tick`].

use std::sync::Arc;

use bc_proto::{InputCmd, MAX_DATAGRAM, SnapshotHeader};
use bc_sim::handle::Handle;
use bc_sim::zero::TacticalPicture;
use bc_sim::{Sim, SimConfig, SuitId};

use crate::clients::ClientState;
use crate::metrics::Metrics;
use crate::queues::{Control, Outcome, Report, Restored, SectorEnds, SectorShared, SlotState};
use crate::replicate::{Work, build_snapshot};

/// Ticks between tactical pictures per ZERO pilot when an external oracle is attached (≈3.75 Hz).
const PICTURE_INTERVAL: u32 = 8;

#[derive(Clone, Copy, Debug)]
pub struct SectorConfig {
    pub sim: SimConfig,
    pub max_clients: usize,
    /// Send tactical pictures to an external oracle (TypeSafe Jev).
    pub oracle: bool,
    /// Called with `true`/`false` around every tick, e.g. `bc_alloc::set_hot` to count allocations
    /// on the hot path.
    pub hot_guard: Option<fn(bool)>,
}

impl Default for SectorConfig {
    fn default() -> Self {
        Self { sim: SimConfig::default(), max_clients: 64, oracle: false, hot_guard: None }
    }
}

pub struct Sector {
    pub sim: Sim,
    cfg: SectorConfig,
    shared: Arc<SectorShared>,
    ends: SectorEnds,
    clients: Box<[ClientState]>,
    scratch: Box<[u8]>,
    work: Work,
    picture: TacticalPicture,
}

impl Sector {
    /// Allocates everything (startup only).
    #[allow(clippy::disallowed_methods, clippy::disallowed_macros)]
    pub(crate) fn new(cfg: SectorConfig, shared: Arc<SectorShared>, ends: SectorEnds) -> Self {
        let sim = Sim::new(cfg.sim);
        let max_suits = sim.suits.cap;
        let rocks = sim.field.len();
        Self {
            clients: (0..cfg.max_clients).map(|_| ClientState::new(max_suits, rocks)).collect(),
            scratch: vec![0u8; MAX_DATAGRAM + 64].into_boxed_slice(),
            work: Work::new(max_suits, rocks),
            picture: TacticalPicture::default(),
            sim,
            cfg,
            shared,
            ends,
        }
    }

    pub fn shared(&self) -> &Arc<SectorShared> {
        &self.shared
    }

    pub fn config(&self) -> &SectorConfig {
        &self.cfg
    }

    /// One full server tick. Allocation-free and lock-free.
    pub fn tick(&mut self) {
        let now_us = self.shared.now_us();
        self.tick_at(now_us);
    }

    /// [`tick`](Self::tick) with an explicit clock (µs), for simulated-time tests.
    pub fn tick_at(&mut self, now_us: u64) {
        self.drain_control();
        self.drain_inputs();
        self.drain_advice();
        self.apply_inputs();
        self.sim.step();
        self.pass_on_fates();
        if self.cfg.sim.survival {
            self.watch_losses();
        }
        if self.cfg.oracle {
            self.send_pictures();
        }
        self.replicate(now_us);
        self.publish_metrics();
    }

    fn drain_control(&mut self) {
        while let Some(msg) = self.shared.control.pop() {
            match msg {
                Control::Join { slot, pilot, frame, faction, max_datagram, comeback, launch } => {
                    let s = slot as usize;
                    if s >= self.clients.len() {
                        continue;
                    }
                    if self.clients[s].active {
                        self.sim.leave(self.clients[s].suit);
                    }
                    // Back in the suit they left asleep, if it's still there.
                    let woke = comeback
                        .sleeper
                        .map(|(idx, generation)| SuitId(Handle { idx, generation }))
                        .filter(|&id| self.sim.wake(id));
                    let seated = match (woke, launch) {
                        (Some(id), _) => Some((id, Outcome::Woke)),
                        // Survival: the suit its pilot built, out of the docking hub.
                        (None, Some(loadout)) => {
                            self.sim.ensure_free_suits(1);
                            self.sim.launch(frame, faction, pilot, &loadout).map(|id| (id, Outcome::Fresh))
                        }
                        // Survival: nothing to wake and nothing launched, no suit.
                        (None, None) if self.cfg.sim.survival => None,
                        (None, None) => {
                            // A full sector makes room: the longest asleep go first.
                            self.sim.ensure_free_suits(1);
                            let id = self.sim.join(frame, faction, pilot);
                            if let Some(id) = id {
                                self.sim.set_credits(id, comeback.credits);
                            }
                            id.map(|id| (id, Outcome::Fresh))
                        }
                    };
                    match seated {
                        Some((id, outcome)) => {
                            let seq = self.sim.events.next_seq();
                            self.clients[s].seat(id, pilot, max_datagram as usize, seq);
                            Metrics::set(&self.shared.metrics.pilots[s].suit, id.idx() as u64 + 1);
                            self.shared.slots[s].publish(SlotState::Active, Some(id), outcome);
                        }
                        None => self.shared.slots[s].publish(SlotState::Refused, None, Outcome::Fresh),
                    }
                }
                Control::Leave { slot } | Control::Sleep { slot } => {
                    let s = slot as usize;
                    if s >= self.clients.len() {
                        continue;
                    }
                    let mut asleep = None;
                    if self.clients[s].active {
                        let id = self.clients[s].suit;
                        if matches!(msg, Control::Sleep { .. }) && self.sim.sleep(id) {
                            asleep = Some(id);
                            // Survival: left in a hide spot, it outlives the server. The session
                            // hears of it before it sees the slot free.
                            if self.cfg.sim.survival
                                && let Some(rec) = self.sim.park_record(id.idx())
                                && self.ends.reports[s].push(Report::Parked(rec)).is_err()
                            {
                                Metrics::add(&self.shared.metrics.notes_dropped, 1);
                            }
                        } else {
                            self.sim.leave(id);
                        }
                        self.clients[s].active = false;
                    }
                    while self.ends.inputs[s].pop().is_ok() {}
                    Metrics::set(&self.shared.metrics.pilots[s].suit, 0);
                    let outcome = if asleep.is_some() { Outcome::Asleep } else { Outcome::Released };
                    self.shared.slots[s].publish(SlotState::Free, asleep, outcome);
                }
                Control::Dock { slot } => {
                    let s = slot as usize;
                    let Some(c) = self.clients.get_mut(s) else { continue };
                    let home = if c.active { self.sim.dock(c.suit) } else { None };
                    let report = match home {
                        Some(home) => {
                            c.active = false;
                            while self.ends.inputs[s].pop().is_ok() {}
                            Metrics::set(&self.shared.metrics.pilots[s].suit, 0);
                            self.shared.slots[s].publish(SlotState::Free, None, Outcome::Docked);
                            Report::Home(home)
                        }
                        None => Report::DockRefused,
                    };
                    if self.ends.reports[s].push(report).is_err() {
                        Metrics::add(&self.shared.metrics.notes_dropped, 1);
                    }
                }
                Control::Respawn { slot, frame } => {
                    if let Some(c) = self.clients.get(slot as usize).filter(|c| c.active) {
                        self.sim.set_respawn_frame(c.suit, frame);
                    }
                }
                Control::Restore { key, rec } => {
                    if let Some(id) = self.sim.restore_sleeper(&rec) {
                        let (suit, generation) = (id.0.idx, id.0.generation);
                        if self.shared.restored.push(Restored { key, suit, generation }).is_err() {
                            Metrics::add(&self.shared.metrics.notes_dropped, 1);
                        }
                    }
                }
            }
        }
    }

    fn drain_inputs(&mut self) {
        let next = self.sim.next_tick();
        let m = &self.shared.metrics;
        for (s, client) in self.clients.iter_mut().enumerate() {
            let ring = &mut self.ends.inputs[s];
            while let Ok(msg) = ring.pop() {
                if !client.active {
                    continue;
                }
                Metrics::add(&m.inputs, 1);
                let p = &msg.packet;
                for k in 0..p.count as usize {
                    if !client.jitter.insert(p.cmds[k], next) && k == 0 {
                        Metrics::add(&m.inputs_stale, 1);
                    }
                }
                client.on_ack(p.ack_snapshot);
                client.time_echo_ms = p.client_time_ms;
                client.time_echo_recv_us = msg.recv_us;
            }
        }
    }

    /// Survival: a pilot whose suit is destroyed hears of it at once (with the bounties it had
    /// earned), and is let go when its wreck is cleared (the pilot is back in the hangar).
    fn watch_losses(&mut self) {
        for (s, c) in self.clients.iter_mut().enumerate() {
            if !c.active {
                continue;
            }
            if !self.sim.suits.valid(c.suit) {
                c.active = false;
                while self.ends.inputs[s].pop().is_ok() {}
                Metrics::set(&self.shared.metrics.pilots[s].suit, 0);
                self.shared.slots[s].publish(SlotState::Free, None, Outcome::Lost);
            } else if !c.lost && !self.sim.is_alive(c.suit.idx()) {
                c.lost = true;
                let bounty = self.sim.suits.credits[c.suit.idx()];
                if self.ends.reports[s].push(Report::Lost { bounty }).is_err() {
                    Metrics::add(&self.shared.metrics.notes_dropped, 1);
                }
            }
        }
    }

    /// Sleepers destroyed or cleared this tick, on to the server.
    fn pass_on_fates(&mut self) {
        let shared = &self.shared;
        self.sim.drain_fates(|fate| {
            if shared.notes.push(fate).is_err() {
                Metrics::add(&shared.metrics.notes_dropped, 1);
            }
        });
    }

    fn drain_advice(&mut self) {
        while let Ok(advice) = self.ends.advice.pop() {
            Metrics::add(&self.shared.metrics.advice, 1);
            self.sim.apply_advice(&advice);
        }
    }

    fn apply_inputs(&mut self) {
        let next = self.sim.next_tick();
        for client in self.clients.iter_mut().filter(|c| c.active) {
            let cmd = match client.jitter.take(next) {
                Some(cmd) => {
                    client.missing = 0;
                    client.last_real = next;
                    client.last_cmd = cmd;
                    cmd
                }
                None => {
                    // The last command again without firing, then hands-off: the rule the owner's
                    // prediction flies through its own gaps too. Until the client is first heard
                    // from, the last command is the one the sector left its suit with, so a suit
                    // woken (or put) on a body keeps its grip rather than letting go.
                    client.missing += 1;
                    Metrics::add(&self.shared.metrics.inputs_missing, 1);
                    let last = if client.last_real == u32::MAX {
                        self.sim.suits.input[client.suit.idx()]
                    } else {
                        client.last_cmd
                    };
                    InputCmd::stand_in(&last, next, client.missing)
                }
            };
            self.sim.set_input(client.suit, cmd);
        }
    }

    fn send_pictures(&mut self) {
        let t = self.sim.tick();
        let m = &self.shared.metrics;
        for client in self.clients.iter_mut().filter(|c| c.active) {
            let i = client.suit.idx();
            if t < client.next_picture || !self.sim.suits.zero[i].active() {
                continue;
            }
            client.next_picture = t + PICTURE_INTERVAL;
            if self.sim.picture_for(i, &mut self.picture) {
                match self.ends.pictures.push(self.picture) {
                    Ok(()) => Metrics::add(&m.pictures, 1),
                    Err(_) => Metrics::add(&m.pictures_dropped, 1),
                }
            }
        }
    }

    fn replicate(&mut self, now_us: u64) {
        let t = self.sim.tick();
        let m = &self.shared.metrics;
        self.work.locate_chunks(&self.sim);
        for (s, client) in self.clients.iter_mut().enumerate() {
            if !client.active {
                continue;
            }
            let hold_ms = (now_us.saturating_sub(client.time_echo_recv_us) / 1_000).min(255) as u8;
            let header = SnapshotHeader {
                tick: t,
                ack_input_tick: client.last_real,
                input_health: (i64::from(client.jitter.newest) - i64::from(t)).clamp(-128, 127) as i8,
                time_echo_ms: client.time_echo_ms,
                echo_hold_ms: if client.time_echo_recv_us == 0 { 0 } else { hold_ms },
                tidi_pct: 100,
                flags: 0,
            };
            let Some(n) = build_snapshot(&self.sim, client, &header, &mut self.scratch, &mut self.work)
            else {
                continue;
            };
            let out = &mut self.ends.outputs[s];
            if out.slots() < n + 2 {
                Metrics::add(&m.out_drops, 1);
                continue;
            }
            let _ = out.push_entire_slice(&(n as u16).to_le_bytes());
            let _ = out.push_entire_slice(&self.scratch[..n]);
            Metrics::add(&m.snapshots, 1);
            Metrics::add(&m.snapshot_bytes, n as u64);
            Metrics::max(&m.snapshot_max_bytes, n as u64);
            // Per-pilot combat stats for /status.
            let st = self.sim.stats(client.suit.idx());
            let ps = &m.pilots[s];
            Metrics::set(&ps.shots, u64::from(st.shots));
            Metrics::set(&ps.hits, u64::from(st.hits));
            Metrics::set(&ps.kills, u64::from(st.kills));
            Metrics::set(&ps.deaths, u64::from(st.deaths));
            for (c, n) in ps.hits_by_class.iter().zip(st.hits_by_class) {
                Metrics::set(c, u64::from(n));
            }
            Metrics::set(&ps.specials, u64::from(st.specials));
            Metrics::set(&ps.missiles, u64::from(st.missiles));
            Metrics::set(&ps.frame, self.sim.suits.frame[client.suit.idx()] as u64);
            Metrics::set(&ps.credits, u64::from(self.sim.suits.credits[client.suit.idx()]));
        }
    }

    fn publish_metrics(&mut self) {
        let m = &self.shared.metrics;
        Metrics::set(&m.clients, self.clients.iter().filter(|c| c.active).count() as u64);
        Metrics::set(&m.suits_alive, self.sim.alive_count() as u64);
        Metrics::set(&m.sleepers, self.sim.sleepers() as u64);
        Metrics::set(&m.parked, self.sim.parked() as u64);
        Metrics::set(&m.grounded, u64::from(self.sim.n_grounded));
        Metrics::set(&m.aloft, u64::from(self.sim.n_aloft));
        Metrics::set(&m.hidden, u64::from(self.sim.n_hidden));
        Metrics::set(&m.sleepers_hidden, u64::from(self.sim.n_hidden_asleep));
        Metrics::set(&m.projectiles, self.sim.projectiles.count() as u64);
        Metrics::set(&m.events, u64::from(self.sim.events.next_seq()));
    }
}
