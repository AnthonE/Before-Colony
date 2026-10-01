//! One pilot's session, from the moment they have a slot to the moment they give it back.
//!
//! - **Arcade rules:** the pilot is seated in a suit at once (the one they left asleep, or a new
//!   one of the frame they chose) and flies until they leave.
//! - **Survival rules:** the pilot starts on foot in their hangar bay. The session keeps their
//!   [`Hangar`] (in their record, if they signed in; a guest gets the starter kit for the visit),
//!   answers its requests (fabricating, fitting, repairing, trading on the exchange), launches
//!   the suit they built into the sector, and takes it home again when it docks, or learns it
//!   was lost. A pilot who left their suit asleep out there wakes in it.
//!
//! Whatever happens, the slot is handed back in the order that keeps it race-free: stop sending,
//! settle the roster, let the sector put the suit to sleep (or release it), then return the lease.

use std::time::{Duration, Instant};

use bc_econ::wire::{self, HangarView, MarketView, Outcome as SortieOutcome, Place, Request, Update};
use bc_econ::{Bay, Hangar, Item};
use bc_proto::auth::Address;
use bc_proto::control::{
    self, ControlMsg, Frame, Name, RejectReason, bye, notice, roster_flags, welcome_flags,
};
use bc_proto::{
    Faction, FrameId, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PacketKind, PilotKind, packet_kind,
};
use bc_sector::{Comeback, Control, InputMsg, Metrics, Outcome, Report, SlotLease, SlotState};
use bc_sim::sim::Loadout;
use tokio::sync::{broadcast, oneshot};
use wtransport::{Connection, RecvStream, SendStream};

use super::NetStats;
use super::game::{
    EgressCmd, GameShared, HangarEntry, RateLimit, RosterEntry, RosterUpdate, forget, process_notes, reject,
    send_control, set_roster_flags, wait_slot,
};
use crate::pilots::{self, Fate, ParkedSuit, PilotRecord, Sleeper};

/// Who the Hello said the pilot is.
pub(super) struct Who {
    pub pilot: PilotKind,
    pub frame: FrameId,
    pub faction: Faction,
    pub name: Name,
    pub address: Option<Address>,
}

/// How often the session looks at its suit's reports, its stations and the market.
const TICK: Duration = Duration::from_millis(100);
/// The market is resent at most this often while it moves.
const MARKET_EVERY: Duration = Duration::from_secs(2);

/// Takes a slot, plays, and gives the slot back.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    conn: &Connection,
    game: &GameShared,
    stats: &NetStats,
    tx: &mut SendStream,
    rx: &mut RecvStream,
    pending: Vec<u8>,
    who: Who,
    kicked: Option<oneshot::Receiver<()>>,
) -> anyhow::Result<()> {
    let Some(lease) = game.sector.leases.pop() else {
        NetStats::add(&stats.sessions_rejected, 1);
        return reject(tx, RejectReason::ServerFull).await;
    };
    let slot = lease.slot;
    let callsign = if who.name.is_empty() { format!("Pilot-{slot}") } else { who.name.as_str().to_string() };
    let record = match who.address {
        Some(a) => Some(game.pilots.load_or_new(&a).await),
        None => None,
    };
    let trader = match who.address {
        Some(a) => pilots::key(&a),
        None => format!("guest-{slot}-{:x}", game.sector.now_us()),
    };
    let mut s = Session {
        conn,
        game,
        stats,
        tx,
        lease: Some(lease),
        slot,
        pilot: who.pilot,
        frame: who.frame,
        faction: who.faction,
        callsign,
        address: who.address,
        record,
        max_datagram: conn.max_datagram_size().unwrap_or(MAX_DATAGRAM).min(MAX_DATAGRAM) as u16,
        suit: None,
        hangar: Hangar::default(),
        trader,
        place: Place::Hangar,
        watching: None,
        market_seen: 0,
        market_sent: Instant::now() - MARKET_EVERY,
        lost: false,
        entered: false,
    };
    let result = match s.enter().await {
        Ok(true) => s.play(rx, pending, kicked).await,
        Ok(false) => Ok(()),
        Err(e) => Err(e),
    };
    s.leave().await;
    result
}

struct Session<'a> {
    conn: &'a Connection,
    game: &'a GameShared,
    stats: &'a NetStats,
    tx: &'a mut SendStream,
    /// The slot's lease: its input ring and report ring (handed back last).
    lease: Option<SlotLease>,
    slot: u16,
    pilot: PilotKind,
    /// The frame chosen (arcade), and the side flown for.
    frame: FrameId,
    faction: Faction,
    callsign: String,
    address: Option<Address>,
    record: Option<PilotRecord>,
    max_datagram: u16,
    /// The suit flown, while the pilot is in the sector: (entity slot, generation).
    suit: Option<(u16, u16)>,
    /// Survival: the pilot's hangar, their name on the exchange, and where they are.
    hangar: Hangar,
    trader: String,
    place: Place,
    /// The item whose book the pilot is looking at.
    watching: Option<Item>,
    market_seen: u64,
    market_sent: Instant,
    /// The suit was destroyed; its wreck is still out there.
    lost: bool,
    /// Welcomed (so leaving has a roster entry, a record and a lease to settle).
    entered: bool,
}

/// What a pilot is told the first time they wake in their bay.
pub const ARRIVAL: &str =
    "ARRIVAL REGISTERED · THE CHARTER BOARD ADVANCES YOU 2,000 CR AND A LEO · WELCOME TO THE FIRST COLONY";

impl Session<'_> {
    fn survival(&self) -> bool {
        self.game.survival
    }

    fn lease(&mut self) -> &mut SlotLease {
        self.lease.as_mut().expect("the lease is held until the session leaves")
    }

    /// Seats the pilot, welcomes them, and tells them how things stand. `false`: refused (and
    /// told why).
    async fn enter(&mut self) -> anyhow::Result<bool> {
        let run = self.game.pilots.run;
        let left = self.record.as_ref().and_then(|r| r.sleeper).filter(|s| s.run == run);
        let had_sleeper = self.record.as_ref().is_some_and(|r| r.sleeper.is_some());
        let mut woke = false;
        // A pilot with no hangar yet has only just arrived.
        let mut arrived = false;
        if self.survival() {
            self.hangar = self.record.as_mut().and_then(|r| r.hangar.take()).unwrap_or_else(|| {
                arrived = true;
                Hangar::starter()
            });
            // Out in the sector, asleep: wake in it, if it's still there.
            if matches!(self.hangar.bay, Bay::Out { .. }) {
                let line = self.hangar_line();
                if let Some(sleeper) = left {
                    let comeback = Comeback { sleeper: Some((sleeper.suit, sleeper.generation)), credits: 0 };
                    woke = self.seat(line, None, comeback).await == Some(Outcome::Woke);
                }
            }
        } else {
            let comeback = Comeback {
                sleeper: left.map(|s| (s.suit, s.generation)),
                credits: self.record.as_ref().map_or(0, |r| r.credits),
            };
            match self.seat(self.frame, None, comeback).await {
                Some(outcome) => woke = outcome == Outcome::Woke,
                None => {
                    NetStats::add(&self.stats.sessions_rejected, 1);
                    let _ =
                        send_control(self.tx, ControlMsg::Reject { reason: RejectReason::ServerFull }).await;
                    return Ok(false);
                }
            }
        }
        self.entered = true;
        // What became of the suit they left, if they didn't wake in it.
        let news = match self.address {
            Some(a) if had_sleeper && !woke => {
                // The sector reports a sleeper's end before it can find it gone: it's in by now.
                process_notes(self.game);
                Some(self.game.pilots.take_news(&a).unwrap_or(Fate::Lost))
            }
            Some(a) => {
                // News without a sleeper to go with it is stale.
                let _ = self.game.pilots.take_news(&a);
                None
            }
            None => None,
        };
        let mut sortie = None;
        if self.survival() && !woke && matches!(self.hangar.bay, Bay::Out { .. }) {
            sortie = Some(match news {
                Some(Fate::Destroyed { .. }) => (SortieOutcome::Lost, self.hangar.lost(0)),
                // Cleared for room, or from before the server restarted: towed in.
                _ => {
                    self.hangar.recover();
                    (SortieOutcome::Recovered, "THE COLONY'S TUGS BROUGHT YOUR SUIT IN".to_string())
                }
            });
        }
        if let Some(r) = self.record.as_mut() {
            // Woken, or gone: either way it's no longer out there asleep (in a hide spot or not).
            r.sleeper = None;
            r.parked = None;
            r.name = self.callsign.clone();
            r.frame = self.frame.slug().to_string();
            r.seen_unix = pilots::unix_now();
        }
        self.save().await;

        let mut flags = 0;
        if self.address.is_some() {
            flags |= welcome_flags::SIGNED_IN;
        }
        if woke {
            flags |= welcome_flags::WOKE;
        }
        if self.survival() {
            flags |= welcome_flags::SURVIVAL;
        }
        if self.game.anime {
            flags |= welcome_flags::ANIME;
        }
        send_control(
            self.tx,
            ControlMsg::Welcome {
                version: PROTOCOL_VERSION,
                client_slot: self.slot,
                tick: self.game.sector.tick.load(std::sync::atomic::Ordering::Acquire),
                tick_hz: bc_sim::TICK_HZ as u8,
                sector: 1,
                zero_allowed: true,
                max_datagram: self.max_datagram,
                field_seed: self.game.sector.field_seed,
                field_rocks: self.game.sector.field_rocks,
                flags,
                landmarks: self.game.sector.landmarks,
            },
        )
        .await?;
        if let Some(a) = self.address {
            send_control(self.tx, ControlMsg::Token { token: self.game.pilots.issue_token(a) }).await?;
        }
        match news {
            Some(Fate::Destroyed { by }) => {
                send_control(
                    self.tx,
                    ControlMsg::Notice { code: notice::SLEEPER_DESTROYED, name: Name::new(&by) },
                )
                .await?;
            }
            Some(Fate::Lost) => {
                send_control(self.tx, ControlMsg::Notice { code: notice::SLEEPER_LOST, name: Name::new("") })
                    .await?;
            }
            None => {}
        }
        tracing::info!(slot = self.slot, suit = ?self.suit, pilot = ?self.pilot, name = %self.callsign, woke, "pilot joined");
        // Everyone flying, for the pilot's roster.
        let everyone: Vec<(u16, RosterEntry)> = self
            .game
            .roster
            .read()
            .map(|r| r.iter().map(|(k, v)| (*k, v.clone())).collect())
            .unwrap_or_default();
        for (s, e) in everyone {
            send_control(
                self.tx,
                ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name), flags: e.flags },
            )
            .await?;
        }
        let _ = self.game.egress.push(EgressCmd::Attach(self.slot, self.conn.clone()));
        self.game.egress_thread.unpark();
        if self.survival() {
            self.place = if self.suit.is_some() { Place::Space } else { Place::Hangar };
            self.settle(true);
            self.send_place().await?;
            if let Some((outcome, text)) = sortie {
                self.send(&Update::Sortie { outcome, text }).await?;
            }
            if arrived {
                self.send(&Update::News { text: ARRIVAL.to_string() }).await?;
            }
            self.send_hangar().await?;
            self.send_market().await?;
            self.publish_hangar();
        }
        Ok(true)
    }

    /// The line of the suit in (or out of) the bay.
    fn hangar_line(&self) -> FrameId {
        match &self.hangar.bay {
            Bay::Docked { suit } | Bay::Out { suit } => suit.line,
            Bay::Empty => self.frame,
        }
    }

    /// Asks the sector for a suit: the sleeper in `comeback`, the `launch`ed build, or (arcade) a
    /// new `frame`. How it went, if the pilot is seated.
    async fn seat(&mut self, frame: FrameId, launch: Option<Loadout>, comeback: Comeback) -> Option<Outcome> {
        let sector = &self.game.sector;
        let status = &sector.slots[self.slot as usize];
        let epoch = status.epoch();
        let mut join = Control::Join {
            slot: self.slot,
            pilot: self.pilot,
            frame,
            faction: self.faction,
            max_datagram: self.max_datagram,
            comeback,
            launch,
        };
        while let Err(back) = sector.control.push(join) {
            join = back;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let state = wait_slot(sector, self.slot, |s, e| e != epoch && s != SlotState::Free).await;
        let (Some(SlotState::Active), Some((suit, generation))) = (state, status.suit_id()) else {
            return None;
        };
        self.suit = Some((suit, generation));
        self.lost = false;
        if let Some(a) = self.address {
            self.game.pilots.bind_suit((suit, generation), a);
        }
        // On everyone's roster while it flies.
        let flags = if self.address.is_some() { roster_flags::VERIFIED } else { 0 };
        if let Ok(mut r) = self.game.roster.write() {
            r.insert(
                suit,
                RosterEntry {
                    name: self.callsign.clone(),
                    pilot: self.pilot,
                    client_slot: self.slot,
                    flags,
                    address: self.address.as_ref().map(pilots::short),
                },
            );
        }
        let _ = self.game.roster_tx.send(RosterUpdate {
            suit,
            pilot: self.pilot,
            name: self.callsign.clone(),
            flags,
        });
        Some(status.outcome())
    }

    /// The suit is out of the sector (docked, or its wreck cleared): off the roster.
    fn unseat(&mut self) {
        if let Some((suit, generation)) = self.suit.take() {
            let _ = self.game.pilots.suit_gone((suit, generation));
            forget(self.game, suit, self.pilot);
        }
    }

    async fn play(
        &mut self,
        rx: &mut RecvStream,
        mut pending: Vec<u8>,
        mut kicked: Option<oneshot::Receiver<()>>,
    ) -> anyhow::Result<()> {
        let mut roster_rx = self.game.roster_tx.subscribe();
        let mut rate = RateLimit { tokens: 240.0, last: Instant::now() };
        let mut buf = [0u8; 4_096];
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_settle = Instant::now();
        // When the client last sent input: a session in the sector that goes quiet for too long
        // is ended (in the hangar, the pilot sends nothing while they walk about).
        let mut heard = tokio::time::Instant::now();
        loop {
            tokio::select! {
                d = self.conn.receive_datagram() => {
                    let d = d?;
                    NetStats::add(&self.stats.datagrams_in, 1);
                    NetStats::add(&self.stats.bytes_in, d.len() as u64);
                    if packet_kind(&d) != Some(PacketKind::Input) {
                        NetStats::add(&self.stats.malformed, 1);
                        continue;
                    }
                    match InputPacket::decode(&d) {
                        Ok(packet) if rate.allow() => {
                            heard = tokio::time::Instant::now();
                            let recv_us = self.game.sector.now_us();
                            let _ = self.lease().input.push(InputMsg { packet, recv_us });
                        }
                        Ok(_) => {}
                        Err(_) => NetStats::add(&self.stats.malformed, 1),
                    }
                }
                r = rx.read(&mut buf) => {
                    match r? {
                        Some(n) => pending.extend_from_slice(&buf[..n]),
                        None => return Ok(()),
                    }
                    while let Some((frame, used)) = Frame::decode(&pending).map_err(|e| anyhow::anyhow!("{e}"))? {
                        let bytes = match frame {
                            Frame::Msg(ControlMsg::Respawn { frame }) => {
                                if !self.survival() {
                                    let _ = self.game.sector.control.push(Control::Respawn { slot: self.slot, frame });
                                }
                                None
                            }
                            Frame::Msg(ControlMsg::Bye { .. }) => return Ok(()),
                            Frame::Msg(_) => None,
                            Frame::Hangar(p) => Some(p.to_vec()),
                        };
                        pending.drain(..used);
                        if let Some(bytes) = bytes {
                            self.on_request(&bytes).await?;
                        }
                    }
                }
                u = roster_rx.recv() => {
                    match u {
                        Ok(u) => send_control(self.tx, ControlMsg::Roster { slot: u.suit, pilot: u.pilot, name: Name::new(&u.name), flags: u.flags }).await?,
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let everyone: Vec<(u16, RosterEntry)> = self.game.roster.read().map(|r| r.iter().map(|(k, v)| (*k, v.clone())).collect()).unwrap_or_default();
                            for (s, e) in everyone {
                                send_control(self.tx, ControlMsg::Roster { slot: s, pilot: e.pilot, name: Name::new(&e.name), flags: e.flags }).await?;
                            }
                        }
                        Err(_) => return Ok(()),
                    }
                }
                _ = async { match kicked.as_mut() { Some(k) => { let _ = k.await; } None => std::future::pending::<()>().await } } => {
                    // Signed in somewhere else: that session has the pilot now.
                    let _ = send_control(self.tx, ControlMsg::Bye { reason: bye::TAKEN_OVER }).await;
                    return Ok(());
                }
                _ = every.tick() => {
                    if self.suit.is_none() {
                        heard = tokio::time::Instant::now();
                    }
                    if self.survival() {
                        let settle = last_settle.elapsed() >= Duration::from_secs(1);
                        if settle {
                            last_settle = Instant::now();
                        }
                        self.on_tick(settle).await?;
                    }
                }
                _ = tokio::time::sleep_until(heard + self.game.idle) => {
                    // Nobody at the controls (the tab is frozen, or the client hung): as if they'd left.
                    tracing::info!(slot = self.slot, name = %self.callsign, "idle: ending the session");
                    let _ = send_control(self.tx, ControlMsg::Bye { reason: bye::IDLE }).await;
                    return Ok(());
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // Survival: the hangar
    // ---------------------------------------------------------------------------------------------

    async fn send(&mut self, update: &Update) -> anyhow::Result<()> {
        let json = wire::encode(update);
        let mut frame = vec![0u8; json.len() + 3];
        let n = control::encode_hangar(&json, &mut frame)
            .ok_or_else(|| anyhow::anyhow!("hangar message too long"))?;
        self.tx.write_all(&frame[..n]).await?;
        Ok(())
    }

    async fn note(&mut self, text: impl Into<String>, ok: bool) -> anyhow::Result<()> {
        self.send(&Update::Note { text: text.into(), ok }).await
    }

    async fn send_place(&mut self) -> anyhow::Result<()> {
        let bay = (self.slot % 99 + 1) as u8;
        self.send(&Update::Place { place: self.place, bay }).await
    }

    async fn send_hangar(&mut self) -> anyhow::Result<()> {
        let view = HangarView::of(&self.hangar, pilots::unix_now());
        self.send(&Update::Hangar(view)).await
    }

    async fn send_market(&mut self) -> anyhow::Result<()> {
        self.market_seen = self.game.market.version();
        self.market_sent = Instant::now();
        let view = self.game.market.with(|ex| MarketView::of(ex, &self.trader));
        self.send(&Update::Market(view)).await?;
        if let Some(item) = self.watching {
            let (depth, history) = self.game.market.with(|ex| (ex.depth(item, 8), ex.history(item)));
            self.send(&Update::Book { depth, history }).await?;
        }
        Ok(())
    }

    /// Runs the stations' clocks and collects what the exchange owes. What happened, to tell.
    fn settle(&mut self, collect: bool) -> Vec<String> {
        let mut notes: Vec<String> = self
            .hangar
            .settle(pilots::unix_now())
            .into_iter()
            .map(|(item, qty)| format!("MADE {} {}", item.amount(qty), item.name().to_uppercase()))
            .collect();
        if collect {
            let trader = self.trader.clone();
            let hangar = &mut self.hangar;
            if self.game.market.with(|ex| ex.owed(&trader)) {
                notes.extend(self.game.market.with(|ex| hangar.collect(ex, &trader)));
            }
        }
        notes
    }

    /// The hangar as `/status` lists it.
    fn publish_hangar(&self) {
        if !self.survival() {
            return;
        }
        let h = &self.hangar;
        let (bay, suit) = match &h.bay {
            Bay::Empty => ("empty", None),
            Bay::Docked { suit } => ("docked", Some(suit)),
            Bay::Out { suit } => ("out", Some(suit)),
        };
        let entry = HangarEntry {
            name: self.callsign.clone(),
            address: self.address.as_ref().map(pilots::short),
            place: match self.place {
                Place::Hangar => "hangar",
                Place::Space => "space",
            },
            credits: h.credits,
            bay,
            line: suit.map(|s| s.line.slug()),
            parts: suit.map_or(0, |s| s.fitted().count()),
            jobs: h.works.fabricator.jobs.len() + h.works.foundry.jobs.len(),
            stores_kg: h.stores.stock().filter(|(i, _)| i.bulk()).map(|(_, q)| q).sum(),
            stores_pieces: h.stores.stock().filter(|(i, _)| !i.bulk()).map(|(_, q)| q).sum::<u64>()
                + h.stores.parts().len() as u64,
        };
        if let Ok(mut all) = self.game.hangars.write() {
            all.insert(self.slot, entry);
        }
    }

    /// Keeps the record (a signed-in pilot's) in step with the hangar.
    async fn save(&mut self) {
        if let Some(r) = self.record.as_mut() {
            if self.game.survival {
                r.hangar = Some(self.hangar.clone());
            }
            let r = r.clone();
            self.game.pilots.save(r).await;
        }
        self.publish_hangar();
    }

    async fn on_request(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        if !self.survival() {
            return Ok(());
        }
        let Some(req) = wire::decode::<Request>(bytes) else {
            return self.note("the hangar didn't understand that", false).await;
        };
        match req {
            Request::Launch => self.launch().await,
            Request::Dock => self.dock().await,
            Request::Watch { item } => {
                self.watching = item.filter(|i| i.valid());
                self.send_market().await
            }
            other => {
                let now = pilots::unix_now();
                let (hangar, trader, econ) = (&mut self.hangar, self.trader.as_str(), self.game.econ);
                let done = self.game.market.with(|ex| wire::apply(&other, hangar, ex, trader, now, &econ));
                let trades = matches!(other, Request::Order { .. } | Request::CancelOrder { .. });
                if trades {
                    self.game.market.changed();
                }
                match done {
                    Some(Ok(text)) => self.note(text, true).await?,
                    Some(Err(why)) => self.note(why, false).await?,
                    None => {}
                }
                self.send_hangar().await?;
                if trades {
                    self.send_market().await?;
                }
                self.save().await;
                Ok(())
            }
        }
    }

    /// Boards the suit in the bay and launches it.
    async fn launch(&mut self) -> anyhow::Result<()> {
        if self.suit.is_some() || self.place == Place::Space {
            return self.note("you're already out", false).await;
        }
        let line = self.hangar_line();
        let loadout = match self.hangar.launch() {
            Ok(l) => l,
            Err(why) => return self.note(why, false).await,
        };
        match self.seat(line, Some(loadout), Comeback::default()).await {
            Some(_) => {
                self.place = Place::Space;
                tracing::info!(slot = self.slot, name = %self.callsign, line = line.slug(), "launched");
                self.send_place().await?;
                self.send_hangar().await?;
            }
            None => {
                self.hangar.recover();
                self.note("the launch rail is busy: the sector is full, try again in a moment", false)
                    .await?;
                self.send_hangar().await?;
            }
        }
        self.save().await;
        Ok(())
    }

    /// Takes the suit into the bay, if it's at rest in the dock.
    async fn dock(&mut self) -> anyhow::Result<()> {
        if self.suit.is_none() || self.lost {
            return self.note("there's nothing to dock", false).await;
        }
        let _ = self.game.sector.control.push(Control::Dock { slot: self.slot });
        // The sector answers within a tick or two.
        for _ in 0..200 {
            let Ok(report) = self.lease().reports.pop() else {
                tokio::time::sleep(Duration::from_millis(5)).await;
                continue;
            };
            let answered = matches!(report, Report::Home(_) | Report::DockRefused);
            self.on_report(report).await?;
            if answered {
                return Ok(());
            }
        }
        self.note("the dock didn't answer", false).await
    }

    async fn on_report(&mut self, report: Report) -> anyhow::Result<()> {
        match report {
            Report::Home(home) => {
                self.unseat();
                let text = self.hangar.came_home(&home);
                self.place = Place::Hangar;
                tracing::info!(slot = self.slot, name = %self.callsign, "docked: {text}");
                self.save().await;
                self.send(&Update::Sortie { outcome: SortieOutcome::Docked, text }).await?;
                self.send_place().await?;
                self.send_hangar().await?;
                self.send_market().await?;
            }
            Report::DockRefused => {
                self.note("come to rest inside the dock's ring of lights to dock", false).await?;
            }
            // Only ever sent as the pilot leaves (`leave` reads it).
            Report::Parked { .. } => {}
            Report::Lost { bounty } => {
                self.lost = true;
                let text = self.hangar.lost(bounty);
                tracing::info!(slot = self.slot, name = %self.callsign, "suit lost");
                self.save().await;
                self.send(&Update::Sortie { outcome: SortieOutcome::Lost, text }).await?;
                self.send_hangar().await?;
            }
        }
        Ok(())
    }

    /// A tenth of a second on: the suit's reports, its wreck clearing, the stations (`settle`: once
    /// a second) and the market.
    async fn on_tick(&mut self, settle: bool) -> anyhow::Result<()> {
        while let Ok(report) = self.lease().reports.pop() {
            self.on_report(report).await?;
        }
        if self.lost && self.game.sector.slots[self.slot as usize].state() == SlotState::Free {
            // The wreck is gone: the pilot is back in the hangar.
            self.lost = false;
            self.unseat();
            self.place = Place::Hangar;
            self.send_place().await?;
            self.send_hangar().await?;
            self.send_market().await?;
            self.publish_hangar();
        }
        if settle {
            let notes = self.settle(true);
            if !notes.is_empty() {
                for n in &notes {
                    self.note(n.clone(), true).await?;
                }
                self.send_hangar().await?;
                self.save().await;
            }
        }
        if self.place == Place::Hangar
            && self.game.market.version() != self.market_seen
            && self.market_sent.elapsed() >= MARKET_EVERY
        {
            self.send_market().await?;
        }
        Ok(())
    }

    /// The pilot is gone: their suit sleeps (signed in) or goes, their record is kept, and the
    /// slot is handed back.
    async fn leave(&mut self) {
        let _ = self.game.egress.push(EgressCmd::Detach(self.slot));
        self.game.egress_thread.unpark();
        if !self.entered {
            if let Some(lease) = self.lease.take() {
                let _ = self.game.sector.leases.push(lease);
            }
            return;
        }
        let game = self.game;
        let slot = self.slot;
        let credits =
            u32::try_from(Metrics::load(&game.sector.metrics.pilots[slot as usize].credits)).unwrap_or(0);
        let status = &game.sector.slots[slot as usize];
        if let Some((suit, generation)) = self.suit {
            if self.address.is_some() {
                // The suit stays, its pilot asleep in the cockpit: marked so in the roster before
                // it sleeps, so news of its end finds it marked.
                set_roster_flags(
                    game,
                    suit,
                    self.pilot,
                    &self.callsign,
                    roster_flags::VERIFIED | roster_flags::ASLEEP,
                );
            } else {
                forget(game, suit, self.pilot);
            }
            let mut bye =
                if self.address.is_some() { Control::Sleep { slot } } else { Control::Leave { slot } };
            while let Err(back) = game.sector.control.push(bye) {
                bye = back;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let _ = wait_slot(&game.sector, slot, |s, _| s == SlotState::Free).await;
            // What the sector had left to say about the suit: that it was left in a hide spot
            // (survival), or destroyed on the way out (with the bounties it had earned). Nothing
            // of it is for whoever has the slot next.
            let (mut parked, mut bounty) = (None, 0);
            while let Some(report) = self.lease.as_mut().and_then(|l| l.reports.pop().ok()) {
                match report {
                    Report::Parked { rec, tick } => parked = Some((rec, tick)),
                    Report::Lost { bounty: b } => bounty = b,
                    Report::Home(_) | Report::DockRefused => {}
                }
            }
            let asleep = match (status.outcome(), status.suit_id()) {
                (Outcome::Asleep, Some((s, g))) => Some(Sleeper {
                    run: game.pilots.run,
                    suit: s,
                    generation: g,
                    since_unix: pilots::unix_now(),
                }),
                _ => None,
            };
            let parked = asleep
                .zip(parked)
                .map(|(s, (rec, tick))| ParkedSuit { tick, ..ParkedSuit::new(&rec, s.since_unix) });
            if let (Some(a), Some(sleeper)) = (self.address, asleep) {
                let hidden = parked.is_some();
                tracing::info!(address = %pilots::short(&a), suit = sleeper.suit, hidden, "asleep in the cockpit");
            } else {
                // A wreck (or a guest's suit) doesn't sleep: it's gone.
                let _ = game.pilots.suit_gone((suit, generation));
                forget(game, suit, self.pilot);
                if self.survival() && matches!(self.hangar.bay, Bay::Out { .. }) {
                    self.hangar.lost(bounty);
                }
            }
            if let Some(r) = self.record.as_mut() {
                r.sleeper = asleep;
                r.parked = parked;
            }
        }
        if self.survival() {
            let _ = self.settle(true);
            if self.address.is_none() {
                // A guest's orders go with them.
                let trader = self.trader.clone();
                game.market.with(|ex| {
                    ex.cancel_all(&trader);
                    let _ = ex.collect(&trader);
                });
                game.market.changed();
            }
            if let Ok(mut all) = game.hangars.write() {
                all.remove(&slot);
            }
        }
        if let Some(r) = self.record.as_mut() {
            if game.survival {
                r.hangar = Some(self.hangar.clone());
            } else {
                r.credits = credits;
            }
            r.seen_unix = pilots::unix_now();
            let r = r.clone();
            game.pilots.save(r).await;
        }
        if let Some(lease) = self.lease.take() {
            let _ = game.sector.leases.push(lease);
        }
    }
}
