//! One pilot's session, from the moment they have a slot to the moment they give it back.
//!
//! - **Arcade rules:** the pilot is seated in a suit at once (the one they left asleep, or a new
//!   one of the frame they chose) and flies until they leave.
//! - **Survival rules:** the pilot starts on foot in their hangar bay. The session keeps their
//!   [`Hangar`] (in their record, if they signed in; a guest gets the starter kit for the visit),
//!   answers its requests (fabricating, fitting, repairing, trading on the exchange), launches
//!   the suit they built into the sector, and takes it home again when it docks, or learns it
//!   was lost. A pilot who left their suit asleep out there wakes in it.
//! - **The Proving Ground** (the colony open): on foot at the Blast Hall's gantry, the pilot boards
//!   one of the Charter Board's trainers (nothing of their hangar's goes with it) and climbs out
//!   there again when it docks. The times the inside's sector checks, round the course and through
//!   the drill, go on the Proving Ground's board, and a signed-in pilot's bests on their record.
//!
//! Whatever happens, the slot is handed back in the order that keeps it race-free: stop sending,
//! settle the roster, let the sector put the suit to sleep (or release it), then return the lease.

use std::time::{Duration, Instant};

use bc_econ::proving::{self as board, Bests, Feat};
use bc_econ::wire::{self, HangarView, MarketView, Outcome as SortieOutcome, Place, Request, Update};
use bc_econ::{Bay, Hangar, Item};
use bc_proto::auth::Address;
use bc_proto::control::{
    self, ControlMsg, Frame, Name, RejectReason, bye, notice, roster_flags, welcome_flags,
};
use bc_proto::presence::{PlazaWriter, PosePacket};
use bc_proto::{
    Faction, FrameId, InputPacket, MAX_DATAGRAM, PROTOCOL_VERSION, PacketKind, PilotKind, packet_kind,
};
use bc_sector::{Comeback, Control, InputMsg, Loss, Metrics, Outcome, Report, SlotLease, SlotState};
use bc_sim::sim::Loadout;
use tokio::sync::{broadcast, oneshot};
use wtransport::{Connection, RecvStream, SendStream};

use super::NetStats;
use super::admit::{Asked, Requests};
use super::game::{
    EgressCmd, GameShared, HangarEntry, RateLimit, RosterEntry, RosterUpdate, forget, process_notes, reject,
    send_control, set_roster_flags, wait_slot,
};
use crate::pilots::{self, Fate, ParkedSuit, PilotRecord, Sleeper};
use crate::radio::Mouth;
use bc_econ::proving::Trainer;

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
/// In the bay, every this many session ticks a plaza datagram with no one in it: the sector's
/// tick, for the colony's clock (2 Hz). In the city it goes every tick (10 Hz).
const HEARTBEAT_TICKS: u32 = 5;
/// On foot in the city, where the pilot watches the colony's inside from moves this often (session
/// ticks: 2 Hz).
const WATCH_TICKS: u32 = 5;

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
        strip: 0,
        named: Default::default(),
        ticks: 0,
        watching: None,
        market_seen: 0,
        market_sent: Instant::now() - MARKET_EVERY,
        board: false,
        board_seen: 0,
        board_sent: Instant::now() - MARKET_EVERY,
        proving_seen: 0,
        proving_sent: Instant::now() - MARKET_EVERY,
        bests: Bests::default(),
        trainer: false,
        trainer_build: Trainer::Board,
        news_seen: game.charter.with(|b| b.news_seq()),
        radio_heard: 0,
        mouth: Mouth::default(),
        lost: false,
        towing: false,
        inside: None,
        spectating: None,
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
    /// In the city: the land strip the pilot came down to, and the people whose names they've
    /// been told.
    strip: u8,
    named: std::collections::HashSet<u16>,
    /// Session ticks, for the bay's heartbeat.
    ticks: u32,
    /// The item whose book the pilot is looking at.
    watching: Option<Item>,
    market_seen: u64,
    market_sent: Instant,
    /// The pilot is looking at the Charter Board; the version they saw, and when it was sent.
    board: bool,
    board_seen: u64,
    board_sent: Instant,
    /// The version of the Proving Ground's board the pilot saw, and when it was sent; their best
    /// times there (a signed-in pilot's, from their record).
    proving_seen: u64,
    proving_sent: Instant,
    bests: Bests,
    /// Flying one of the Charter Board's trainers, boarded at the Blast Hall's gantry.
    trainer: bool,
    /// What the Blast Hall's gantry readies for the pilot (the test range, `proving::Trainer`).
    trainer_build: Trainer,
    /// The newest of the board's notices the pilot has heard.
    news_seen: u64,
    /// The last line of the colony's radio passed on, and how fast the pilot may talk on it.
    radio_heard: u64,
    mouth: Mouth,
    /// The suit was destroyed; its wreck is still out there.
    lost: bool,
    /// The pilot ejected, and the colony's tugs have yet to say what they brought home of the wreck
    /// (`Report::Towed`).
    towing: bool,
    /// Flying inside the colony: the lease on the inside sector's slot the suit is flown through
    /// (`self.suit` names it there).
    inside: Option<SlotLease>,
    /// On foot in the city: the lease on the inside sector's slot the pilot watches its suits
    /// through (a spectator's, `Control::Watch`).
    spectating: Option<SlotLease>,
    /// Welcomed (so leaving has a roster entry, a record and a lease to settle).
    entered: bool,
}

/// How near the gantry's hatch a pilot on foot boards a trainer, m (the plaza's last pose of them).
const HATCH_REACH: f32 = 12.0;

/// Whether `at` (the colony's own frame) is at the Blast Hall's gantry's hatch.
fn at_the_hatch(at: glam::Vec3) -> bool {
    use bc_sim::colony::frame::{Under, from_colony};
    let ((s, x), _) = bc_sim::colony::hall::hatch();
    match from_colony(at) {
        Under::Land(c) => {
            c.strip == bc_sim::colony::hall::hall().strip && (c.s - s).hypot(c.x - x) < HATCH_REACH
        }
        Under::Window { .. } => false,
    }
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
        // The radio from now on.
        self.radio_heard = self.game.radio.said();
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
                Some(Fate::Destroyed { .. }) => {
                    let (text, debrief) = self.hangar.lost_debriefed(0);
                    (SortieOutcome::Lost, text, Some(debrief))
                }
                // Cleared for room, or from before the server restarted: towed in.
                _ => {
                    self.hangar.recover();
                    (SortieOutcome::Recovered, "THE COLONY'S TUGS BROUGHT YOUR SUIT IN".to_string(), None)
                }
            });
        }
        // Lost everything: the Charter Board's advance (`Hangar::reissue`).
        let advanced = if self.survival() { self.hangar.reissue(pilots::unix_now()) } else { None };
        self.bests = self.record.as_ref().map_or_else(Bests::default, |r| r.proving);
        if let Some(r) = self.record.as_mut() {
            // Woken, or gone: either way it's no longer out there asleep (in a hide spot or not).
            r.sleeper = None;
            r.parked = None;
            r.name = self.callsign.clone();
            r.frame = self.frame.slug().to_string();
            r.seen_unix = pilots::unix_now();
        }
        self.save().await;

        self.welcome(woke).await?;
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
            if let Some((outcome, text, debrief)) = sortie {
                self.send(&Update::Sortie { outcome, text, debrief }).await?;
            }
            if let Some(text) = advanced {
                self.send(&Update::News { text }).await?;
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

    /// The Welcome: at the handshake (`woke`: in the suit they left), and again whenever the pilot
    /// moves between sectors (into the colony's inside, and back out).
    async fn welcome(&mut self, woke: bool) -> anyhow::Result<()> {
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
        if self.game.colony {
            flags |= welcome_flags::COLONY;
        }
        let (sector, slot, number) = match (&self.inside, &self.game.inside) {
            (Some(lease), Some(inside)) => {
                flags |= welcome_flags::INTERIOR;
                (&inside.sector, lease.slot, 2)
            }
            _ => (&self.game.sector, self.slot, 1),
        };
        send_control(
            self.tx,
            ControlMsg::Welcome {
                version: PROTOCOL_VERSION,
                client_slot: slot,
                tick: sector.tick.load(std::sync::atomic::Ordering::Acquire),
                tick_hz: bc_sim::TICK_HZ as u8,
                sector: number,
                zero_allowed: true,
                max_datagram: self.max_datagram,
                field_seed: sector.field_seed,
                field_rocks: sector.field_rocks,
                flags,
                landmarks: sector.landmarks,
            },
        )
        .await
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
        // On everyone's roster while it flies, with how weathered it is for everyone to see.
        let verified = if self.address.is_some() { roster_flags::VERIFIED } else { 0 };
        let flags = verified | roster_flags::weathering(self.hangar.weathering());
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
        // Inside the colony the suit was its sector's, and on nobody's roster.
        if self.inside.is_some() {
            self.suit = None;
            return;
        }
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
        // How fast it may ask things of its hangar (`admit::Requests`).
        let mut requests = Requests::new(Instant::now());
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
                    match packet_kind(&d) {
                        Some(PacketKind::Input) => {}
                        // On foot in the city: where the pilot is.
                        Some(PacketKind::Pose) => {
                            match PosePacket::decode(&d) {
                                Ok(p) if rate.allow() && self.place == Place::City => {
                                    let tick = self.game.sector.tick.load(std::sync::atomic::Ordering::Acquire);
                                    self.game.plaza.accept(self.slot, p.seq, p.pose, Instant::now(), tick);
                                }
                                Ok(_) => {}
                                Err(_) => NetStats::add(&self.stats.malformed, 1),
                            }
                            continue;
                        }
                        _ => {
                            NetStats::add(&self.stats.malformed, 1);
                            continue;
                        }
                    }
                    match InputPacket::decode(&d) {
                        Ok(packet) if rate.allow() => {
                            heard = tokio::time::Instant::now();
                            let recv_us = self.game.sector.now_us();
                            // Inside the colony, to its sector.
                            let input = match self.inside.as_mut() {
                                Some(lease) => &mut lease.input,
                                None => &mut self.lease().input,
                            };
                            let _ = input.push(InputMsg { packet, recv_us });
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
                        // Asking too fast is refused, and asking on and on ends the session.
                        let asks = matches!(frame, Frame::Hangar(_) | Frame::Msg(ControlMsg::Respawn { .. }));
                        let asked = if asks { requests.ask(Instant::now()) } else { Asked::Take };
                        let bytes = match (asked, frame) {
                            (_, Frame::Msg(ControlMsg::Bye { .. })) => return Ok(()),
                            (Asked::Take, Frame::Msg(ControlMsg::Respawn { frame })) => {
                                if !self.survival() {
                                    let _ = self.game.sector.control.push(Control::Respawn { slot: self.slot, frame });
                                }
                                None
                            }
                            (Asked::Take, Frame::Hangar(p)) => Some(p.to_vec()),
                            _ => None,
                        };
                        pending.drain(..used);
                        match asked {
                            Asked::Take => {}
                            Asked::Refuse => NetStats::add(&self.stats.requests_refused, 1),
                            Asked::End => {
                                NetStats::add(&self.stats.requests_refused, 1);
                                NetStats::add(&self.stats.flooders_ended, 1);
                                tracing::info!(slot = self.slot, name = %self.callsign, "flooding the control stream: ending the session");
                                let _ = send_control(self.tx, ControlMsg::Bye { reason: bye::LEAVE }).await;
                                return Ok(());
                            }
                        }
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
                    self.hear().await?;
                    if self.survival() {
                        self.send_plaza().await?;
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
        let bay = bc_sim::colony::hub::bay_of_slot(self.slot);
        let strip = (self.place == Place::City).then_some(self.strip);
        self.send(&Update::Place { place: self.place, bay, strip, trainer: self.trainer }).await
    }

    /// In the colony (on foot in its city, or flying inside it): the Proving Ground's board, as
    /// this pilot sees it.
    fn in_the_colony(&self) -> bool {
        self.place == Place::City || self.inside.is_some()
    }

    async fn send_proving(&mut self) -> anyhow::Result<()> {
        self.proving_seen = self.game.proving.version();
        self.proving_sent = Instant::now();
        let mut view = self.game.proving.with(|b| b.view(&self.trader, pilots::unix_now()));
        view.mine = self.bests;
        view.trainer = self.trainer_build;
        self.send(&Update::Proving(view)).await
    }

    /// A time the inside's sector checked: on the board, on the pilot's record if it's their best,
    /// and told to them (with where it went).
    async fn feat(&mut self, feat: Feat, ms: u32) -> anyhow::Result<()> {
        let now = pilots::unix_now();
        let placed = self.game.proving.with(|b| b.record(feat, &self.trader, &self.callsign, ms, now));
        self.game.proving.changed();
        let best = self.bests.better(feat, ms);
        tracing::info!(slot = self.slot, name = %self.callsign, ?feat, ms, rank = ?placed.rank, best, "the proving ground");
        if best && let Some(r) = self.record.as_mut() {
            r.proving = self.bests;
            self.save().await;
        }
        let mut text =
            format!("THE BOARD · {} {} · {}", feat.name(), board::clock(ms), feat.class(ms).name());
        if placed.record {
            text.push_str(" · THE BEST EVER");
        } else if let Some(rank) = placed.rank.filter(|_| placed.improved) {
            text.push_str(&format!(" · {} TODAY", board::ordinal(rank)));
        } else if best {
            text.push_str(" · YOUR BEST");
        }
        self.note(text, true).await?;
        self.send_proving().await
    }

    /// Where the pilot's suit is inside the colony (the colony's own frame), while they fly it there.
    fn suit_inside(&self) -> Option<glam::Vec3> {
        match (&self.inside, &self.game.inside) {
            (Some(lease), Some(inside)) if self.suit.is_some() && !self.lost => {
                Some(inside.sector.metrics.pilots[lease.slot as usize].pos())
            }
            _ => None,
        }
    }

    /// In the city, the people near the pilot (and the names of any they haven't seen before);
    /// flying inside the colony, the people round the suit; in the bay, now and then, just the
    /// sector's tick.
    async fn send_plaza(&mut self) -> anyhow::Result<()> {
        self.ticks = self.ticks.wrapping_add(1);
        let city = self.place == Place::City;
        let suit = self.suit_inside();
        if !city
            && suit.is_none()
            && !(self.place == Place::Hangar && self.ticks.is_multiple_of(HEARTBEAT_TICKS))
        {
            return Ok(());
        }
        let tick = self.game.sector.tick.load(std::sync::atomic::Ordering::Acquire);
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut shown = Vec::new();
        let strip = if city { Some(self.strip) } else { suit.map(|at| crate::plaza::strip_under(at).0) };
        let mut w = PlazaWriter::new(&mut buf, tick, strip);
        match suit {
            _ if city => self.game.plaza.fill(self.slot, Instant::now(), tick, &mut w, &mut shown),
            Some(at) => self.game.plaza.fill_around(at, Instant::now(), tick, &mut w, &mut shown),
            None => {}
        }
        if city && self.ticks.is_multiple_of(WATCH_TICKS) {
            self.watch();
        }
        let n = w.finish();
        if self.conn.send_datagram(&buf[..n]).is_ok() {
            NetStats::add(&self.stats.datagrams_out, 1);
            NetStats::add(&self.stats.bytes_out, n as u64);
        }
        let new: Vec<wire::Person> = shown
            .into_iter()
            .filter(|id| self.named.insert(*id))
            .filter_map(|id| Some(wire::Person { id, name: self.game.plaza.name(id)? }))
            .collect();
        if !new.is_empty() {
            self.send(&Update::People { people: new }).await?;
        }
        Ok(())
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

    async fn send_board(&mut self) -> anyhow::Result<()> {
        self.board_seen = self.game.charter.version();
        self.board_sent = Instant::now();
        let mut view = self.game.charter.with(|b| b.view(&self.trader, pilots::unix_now()));
        // Which of Zodiac's aces flies among the Dolls now.
        let out = bc_sector::ace_of_word(self.game.sector.ace.load(std::sync::atomic::Ordering::Acquire));
        for w in &mut view.wanted {
            w.out = out.is_some_and(|(_, a, flying)| flying && a == w.ace);
        }
        self.send(&Update::Charter(view)).await
    }

    /// The board's notices the pilot hasn't heard yet (the colony's great works finished, an era
    /// begun), as news.
    async fn send_notices(&mut self) -> anyhow::Result<()> {
        let seen = self.news_seen;
        let (latest, news): (u64, Vec<String>) =
            self.game.charter.with(|b| (b.news_seq(), b.news_since(seen).map(|n| n.text.clone()).collect()));
        self.news_seen = latest;
        for text in news {
            self.send(&Update::News { text }).await?;
        }
        Ok(())
    }

    /// A sortie earned `bounty`: it counts toward the patrol the pilot holds.
    async fn patrol_bounties(&mut self, bounty: u32) -> anyhow::Result<()> {
        let now = pilots::unix_now();
        let notes = self.game.charter.with(|b| b.bounties(&self.trader, u64::from(bounty), now));
        if notes.is_empty() {
            return Ok(());
        }
        self.game.charter.changed();
        for n in notes {
            self.note(n, true).await?;
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
            if self.game.charter.with(|b| b.owed(&trader)) {
                notes.extend(self.game.charter.with(|b| b.collect(hangar, &trader)));
                self.game.charter.changed();
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
                Place::City => "city",
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
        let req = wire::decode::<Request>(bytes);
        // The radio is everyone's, whatever the rules; and so is ejecting from a suit.
        if let Some(Request::Say { text }) = &req {
            return self.say(text).await;
        }
        if let Some(Request::Eject { destruct }) = req {
            // In the sector (under arcade rules a pilot is always there). Inside the colony nothing
            // strikes a suit, and nobody ejects.
            let flying = self.place == Place::Space || !self.survival();
            if flying && self.suit.is_some() && !self.lost && self.inside.is_none() {
                let _ = self.game.sector.control.push(Control::Eject { slot: self.slot, destruct });
            }
            return Ok(());
        }
        if !self.survival() {
            return Ok(());
        }
        let Some(req) = req else {
            return self.note("the hangar didn't understand that", false).await;
        };
        match req {
            Request::Launch => self.launch().await,
            Request::LaunchInside => self.launch_inside().await,
            Request::BoardTrainer => self.board_trainer().await,
            Request::Trainer { build } => {
                // Ready only what can be: the bay's build while it stands there, a line the
                // colony builds.
                if let Err(why) = build.loadout(&self.hangar) {
                    return self.note(why, false).await;
                }
                self.trainer_build = build;
                let text = format!("THE GANTRY READIES {}", build.name(&self.hangar));
                self.note(text, true).await?;
                if self.in_the_colony() { self.send_proving().await } else { Ok(()) }
            }
            Request::Dock => self.dock().await,
            Request::EnterCity { strip } => self.enter_city(strip).await,
            Request::LeaveCity => self.leave_city().await,
            Request::Watch { item } => {
                self.watching = item.filter(|i| i.valid());
                self.send_market().await
            }
            Request::UseKit { kit } => {
                if self.place == Place::Space && self.suit.is_some() && !self.lost {
                    match (&self.inside, &self.game.inside) {
                        (Some(lease), Some(inside)) => {
                            let _ = inside.sector.control.push(Control::UseKit { slot: lease.slot, kit });
                        }
                        _ => {
                            let _ = self.game.sector.control.push(Control::UseKit { slot: self.slot, kit });
                        }
                    }
                }
                Ok(())
            }
            // (Handled above, whatever the rules.)
            Request::Eject { .. } => Ok(()),
            Request::WatchBoard { on } => {
                self.board = on;
                if on { self.send_board().await } else { Ok(()) }
            }
            Request::Post { .. }
            | Request::Withdraw { .. }
            | Request::Deliver { .. }
            | Request::TakePatrol { .. }
            | Request::DropPatrol { .. }
            | Request::Contribute { .. }
            | Request::AceTerms { .. }
            | Request::Sign => {
                let now = pilots::unix_now();
                let (hangar, trader, name) = (&mut self.hangar, self.trader.as_str(), self.callsign.as_str());
                let (market, charter) = (&self.game.market, &self.game.charter);
                let done = charter
                    .with(|b| market.with(|ex| wire::apply_charter(&req, hangar, b, ex, trader, name, now)));
                charter.changed();
                if matches!(req, Request::Deliver { .. }) {
                    // A colony contract's goods went to its desk.
                    market.changed();
                }
                match done {
                    Some(Ok(text)) if text.is_empty() => {}
                    Some(Ok(text)) => self.note(text, true).await?,
                    Some(Err(why)) => self.note(why, false).await?,
                    None => {}
                }
                self.send_hangar().await?;
                self.send_board().await?;
                self.send_notices().await?;
                self.save().await;
                Ok(())
            }
            other => {
                let now = pilots::unix_now();
                let effects = self.game.charter.with(|b| b.effects());
                let (hangar, trader, econ) =
                    (&mut self.hangar, self.trader.as_str(), self.game.econ.with(&effects));
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

    /// A line for the colony's radio from this pilot: cleaned, and at most five in ten seconds.
    /// Never logged.
    async fn say(&mut self, text: &str) -> anyhow::Result<()> {
        let Some(line) = wire::clean_line(text) else { return Ok(()) };
        if !self.mouth.allow(Instant::now()) {
            return self.note("the radio's busy: give it a moment", false).await;
        }
        self.game.radio.say(&self.callsign, line);
        Ok(())
    }

    /// Passes on what's been said on the radio since the pilot last heard it (their own lines
    /// too, so everyone hears them in the same order).
    async fn hear(&mut self) -> anyhow::Result<()> {
        for line in self.game.radio.since(self.radio_heard) {
            self.radio_heard = line.seq;
            self.send(&Update::Said { from: line.from, text: line.text }).await?;
        }
        Ok(())
    }

    /// Down a cap lift into the colony's city, from the bay.
    async fn enter_city(&mut self, strip: u8) -> anyhow::Result<()> {
        if !self.game.colony {
            return self.note("the cap lifts are closed", false).await;
        }
        if self.place != Place::Hangar || self.suit.is_some() {
            return self.note("the cap lifts run from the bays", false).await;
        }
        self.place = Place::City;
        self.strip = strip % bc_sim::colony::frame::STRIPS as u8;
        self.game.plaza.enter(self.slot, &self.callsign, self.strip);
        self.named.clear();
        tracing::info!(slot = self.slot, name = %self.callsign, strip = self.strip, "went down into the colony");
        self.watch();
        self.send_place().await?;
        self.send_proving().await?;
        self.publish_hangar();
        Ok(())
    }

    /// On foot in the city: watches the suits flying inside the colony from where the pilot is (a
    /// spectator's slot in the inside's sector, its snapshots on the pilot's connection), or, as
    /// they walk, moves where from. Without a slot to spare, they just don't see suits.
    fn watch(&mut self) {
        let Some(inside) = self.game.inside.clone() else { return };
        let tick = self.game.sector.tick.load(std::sync::atomic::Ordering::Acquire);
        let Some(at) = self.game.plaza.where_is(self.slot, tick) else { return };
        if self.spectating.is_none() {
            let Some(lease) = inside.sector.leases.pop() else { return };
            let _ = inside.egress.push(EgressCmd::Attach(lease.slot, self.conn.clone()));
            inside.egress_thread.unpark();
            self.spectating = Some(lease);
        }
        let Some(slot) = self.spectating.as_ref().map(|l| l.slot) else { return };
        // Full, the next move will do.
        let _ = inside.sector.control.push(Control::Watch { slot, at, max_datagram: self.max_datagram });
    }

    /// Stops watching the colony's inside (up the lift, or gone): its slot goes back, once the sector
    /// has had the last of what this session sent it (a `Watch` still queued would seat it again
    /// under whoever has the slot next).
    async fn unwatch(&mut self) {
        let (Some(mut lease), Some(inside)) = (self.spectating.take(), self.game.inside.clone()) else {
            return;
        };
        let _ = inside.egress.push(EgressCmd::Detach(lease.slot));
        inside.egress_thread.unpark();
        let tick = || inside.sector.tick.load(std::sync::atomic::Ordering::Acquire);
        let mut bye = Control::Leave { slot: lease.slot };
        while let Err(back) = inside.sector.control.push(bye) {
            bye = back;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        // Every tick drains the control queue first: two ticks on, the Leave has been heard.
        let pushed = tick();
        for _ in 0..1_500 {
            if tick() > pushed + 1 && inside.sector.slots[lease.slot as usize].state() == SlotState::Free {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        while lease.reports.pop().is_ok() {}
        let _ = inside.sector.leases.push(lease);
    }

    /// Back up the cap lift to the bay.
    async fn leave_city(&mut self) -> anyhow::Result<()> {
        if self.place != Place::City {
            return self.note("you're not in the colony", false).await;
        }
        self.game.plaza.leave(self.slot);
        self.unwatch().await;
        self.place = Place::Hangar;
        self.send_place().await?;
        self.send_hangar().await?;
        self.publish_hangar();
        Ok(())
    }

    /// Boards the suit in the bay and launches it.
    async fn launch(&mut self) -> anyhow::Result<()> {
        if self.suit.is_some() || self.place == Place::Space {
            return self.note("you're already out", false).await;
        }
        if self.place == Place::City {
            return self
                .note("you're down in the colony: ride the lift back up to your bay to board", false)
                .await;
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

    /// Boards the suit in the bay and launches it into the colony through the inner gate: a seat in
    /// the inside sector (its own slot, rings and egress), and a Welcome to it.
    async fn launch_inside(&mut self) -> anyhow::Result<()> {
        let Some(inside) = self.game.inside.clone() else {
            return self.note("the inner gate is closed", false).await;
        };
        if self.suit.is_some() || self.place != Place::Hangar {
            return self.note("the inner gate launches from your bay", false).await;
        }
        let Some(lease) = inside.sector.leases.pop() else {
            return self.note("the inner gate is busy: the colony's inside is full", false).await;
        };
        let line = self.hangar_line();
        let loadout = match self.hangar.launch() {
            Ok(l) => l,
            Err(why) => {
                let _ = inside.sector.leases.push(lease);
                return self.note(why, false).await;
            }
        };
        let slot = lease.slot;
        let status = &inside.sector.slots[slot as usize];
        let epoch = status.epoch();
        let mut join = Control::Join {
            slot,
            pilot: self.pilot,
            frame: line,
            faction: self.faction,
            max_datagram: self.max_datagram,
            comeback: Comeback::default(),
            launch: Some(loadout),
        };
        while let Err(back) = inside.sector.control.push(join) {
            join = back;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let state = wait_slot(&inside.sector, slot, |s, e| e != epoch && s != SlotState::Free).await;
        let (Some(SlotState::Active), Some(suit)) = (state, status.suit_id()) else {
            let _ = inside.sector.leases.push(lease);
            self.hangar.recover();
            self.note("the inner gate is busy: try again in a moment", false).await?;
            return self.send_hangar().await;
        };
        self.suit = Some(suit);
        self.lost = false;
        self.inside = Some(lease);
        self.place = Place::Space;
        // Its snapshots come from the inside's egress now.
        let _ = self.game.egress.push(EgressCmd::Detach(self.slot));
        self.game.egress_thread.unpark();
        let _ = inside.egress.push(EgressCmd::Attach(slot, self.conn.clone()));
        inside.egress_thread.unpark();
        tracing::info!(slot = self.slot, inside = slot, name = %self.callsign, line = line.slug(), "launched into the colony");
        self.welcome(false).await?;
        self.send_place().await?;
        self.send_hangar().await?;
        self.send_proving().await?;
        self.save().await;
        Ok(())
    }

    /// On foot at the Blast Hall's gantry (where the plaza last had them): aboard one of the
    /// Charter Board's trainers, standing on the gantry's pad, in the inside's sector (its own
    /// slot, rings and egress, as a launch through the inner gate is) and welcomed to it. Nothing
    /// of the pilot's hangar goes with it, and their own suit stays in their bay.
    async fn board_trainer(&mut self) -> anyhow::Result<()> {
        let Some(inside) = self.game.inside.clone() else {
            return self.note("the proving ground is closed", false).await;
        };
        let tick = self.game.sector.tick.load(std::sync::atomic::Ordering::Acquire);
        let at_hatch = self.game.plaza.where_is(self.slot, tick).is_some_and(at_the_hatch);
        if self.place != Place::City || self.suit.is_some() || !at_hatch {
            return self.note("the trainers are boarded at the blast hall's gantry, on foot", false).await;
        }
        // What the pilot asked the gantry for (the test range); the bay's build as it stands now.
        let (frame, loadout) = match self.trainer_build.loadout(&self.hangar) {
            Ok(it) => it,
            Err(why) => return self.note(why, false).await,
        };
        let Some(lease) = inside.sector.leases.pop() else {
            return self.note("the gantry is busy: the colony's inside is full", false).await;
        };
        let slot = lease.slot;
        let status = &inside.sector.slots[slot as usize];
        let epoch = status.epoch();
        let mut board = Control::Board {
            slot,
            pilot: self.pilot,
            frame,
            faction: self.faction,
            max_datagram: self.max_datagram,
            loadout,
        };
        while let Err(back) = inside.sector.control.push(board) {
            board = back;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let state = wait_slot(&inside.sector, slot, |s, e| e != epoch && s != SlotState::Free).await;
        let (Some(SlotState::Active), Some(suit)) = (state, status.suit_id()) else {
            let _ = inside.sector.leases.push(lease);
            return self.note("the gantry is busy: try again in a moment", false).await;
        };
        // Off the street, and no longer watching the inside from it: flying in it.
        self.game.plaza.leave(self.slot);
        self.unwatch().await;
        self.suit = Some(suit);
        self.lost = false;
        self.inside = Some(lease);
        self.trainer = true;
        self.place = Place::Space;
        let _ = self.game.egress.push(EgressCmd::Detach(self.slot));
        self.game.egress_thread.unpark();
        let _ = inside.egress.push(EgressCmd::Attach(slot, self.conn.clone()));
        inside.egress_thread.unpark();
        tracing::info!(slot = self.slot, inside = slot, name = %self.callsign, "boarded a trainer at the blast hall's gantry");
        self.welcome(false).await?;
        self.send_place().await?;
        self.send_proving().await?;
        self.publish_hangar();
        Ok(())
    }

    /// The trainer docked on its gantry: the pilot climbs out onto the hall's floor at its hatch,
    /// back on foot in the city (and the Board keeps its suit).
    async fn out_of_the_trainer(&mut self) -> anyhow::Result<()> {
        self.unseat();
        self.out_of_the_colony().await?;
        self.trainer = false;
        self.place = Place::City;
        let hall = bc_sim::colony::hall::hall();
        self.strip = hall.strip;
        let (hatch, _) = bc_sim::colony::hall::hatch();
        self.game.plaza.enter_at(self.slot, &self.callsign, self.strip, hatch);
        self.named.clear();
        self.watch();
        tracing::info!(slot = self.slot, name = %self.callsign, "out of a trainer at the blast hall's gantry");
        self.note("THE TRAINER'S BACK ON ITS GANTRY", true).await?;
        self.send_place().await?;
        self.send_proving().await?;
        self.publish_hangar();
        Ok(())
    }

    /// Docked back from the colony's inside: its slot goes back, and the pilot is welcomed back to
    /// the sector their bay is in.
    async fn out_of_the_colony(&mut self) -> anyhow::Result<()> {
        let (Some(lease), Some(inside)) = (self.inside.take(), self.game.inside.clone()) else {
            return Ok(());
        };
        let _ = inside.egress.push(EgressCmd::Detach(lease.slot));
        inside.egress_thread.unpark();
        let _ = inside.sector.leases.push(lease);
        let _ = self.game.egress.push(EgressCmd::Attach(self.slot, self.conn.clone()));
        self.game.egress_thread.unpark();
        self.welcome(false).await
    }

    /// The pilot left while flying inside the colony: the suit goes from its sector, and the slot
    /// back.
    async fn leave_the_colony(&mut self) {
        let (Some(mut lease), Some(inside)) = (self.inside.take(), self.game.inside.clone()) else {
            return;
        };
        let _ = inside.egress.push(EgressCmd::Detach(lease.slot));
        inside.egress_thread.unpark();
        let mut bye = Control::Leave { slot: lease.slot };
        while let Err(back) = inside.sector.control.push(bye) {
            bye = back;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let _ = wait_slot(&inside.sector, lease.slot, |s, _| s == SlotState::Free).await;
        while lease.reports.pop().is_ok() {}
        let _ = inside.sector.leases.push(lease);
        self.suit = None;
    }

    /// Takes the suit into the bay, if it's at rest in the dock.
    async fn dock(&mut self) -> anyhow::Result<()> {
        if self.suit.is_none() || self.lost {
            return self.note("there's nothing to dock", false).await;
        }
        match (&self.inside, &self.game.inside) {
            (Some(lease), Some(inside)) => {
                let _ = inside.sector.control.push(Control::Dock { slot: lease.slot });
            }
            _ => {
                let _ = self.game.sector.control.push(Control::Dock { slot: self.slot });
            }
        }
        // The sector answers within a tick or two.
        for _ in 0..200 {
            let report = match self.inside.as_mut() {
                Some(lease) => lease.reports.pop(),
                None => self.lease().reports.pop(),
            };
            let Ok(report) = report else {
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
            // The Board's suit stays the Board's: nothing of it comes home.
            Report::Home(_) if self.trainer => self.out_of_the_trainer().await?,
            Report::Home(home) => {
                self.unseat();
                self.out_of_the_colony().await?;
                let (text, debrief) = self.hangar.came_home_debriefed(&home);
                self.patrol_bounties(home.bounty).await?;
                self.place = Place::Hangar;
                tracing::info!(slot = self.slot, name = %self.callsign, "docked: {text}");
                self.save().await;
                self.send(&Update::Sortie { outcome: SortieOutcome::Docked, text, debrief: Some(debrief) })
                    .await?;
                self.send_place().await?;
                self.send_hangar().await?;
                self.send_market().await?;
            }
            Report::DockRefused => {
                let text = if self.trainer {
                    "come to rest on the blast hall's gantry to dock the trainer"
                } else if self.inside.is_some() {
                    "come to rest inside the inner gate's ring of lights to dock"
                } else {
                    "come to rest inside the dock's ring of lights to dock"
                };
                self.note(text, false).await?;
            }
            Report::Towed { wreck, torso, ace } => {
                self.towing = false;
                if self.survival() {
                    let mut notes = self.towed(wreck.as_ref(), torso, ace);
                    tracing::info!(slot = self.slot, name = %self.callsign, found = wreck.is_some(), ?ace, "wreck towed");
                    // Back in the bay with nothing to build on, even so: the Charter Board's advance.
                    if !self.lost
                        && self.suit.is_none()
                        && let Some(text) = self.hangar.reissue(pilots::unix_now())
                    {
                        notes.push(text);
                    }
                    self.save().await;
                    for text in notes {
                        self.send(&Update::News { text }).await?;
                    }
                    self.send_hangar().await?;
                }
            }
            Report::AceDown { ace, hulk, generation } => self.ace_down(ace, hulk, generation).await?,
            Report::Course { ms } => self.feat(Feat::Course, ms).await?,
            Report::Drill { ms } => self.feat(Feat::Drill, ms).await?,
            // Nothing is lost inside the colony; the Board's trainers least of all.
            Report::Lost { .. } if self.trainer => {}
            // Only ever sent as the pilot leaves (`leave` reads it).
            Report::Parked { .. } => {}
            Report::Lost { bounty, how } => {
                self.lost = true;
                let (mut text, debrief) = self.hangar.lost_debriefed(bounty);
                match how {
                    Loss::Destroyed => {}
                    Loss::Ejected => {
                        self.towing = true;
                        text.push_str(" · YOU EJECTED · THE TUGS ARE GOING OUT FOR THE WRECK");
                    }
                    Loss::Blown => text.push_str(" · SELF-DESTRUCTED"),
                }
                self.patrol_bounties(bounty).await?;
                tracing::info!(slot = self.slot, name = %self.callsign, "suit lost");
                self.save().await;
                self.send(&Update::Sortie { outcome: SortieOutcome::Lost, text, debrief: Some(debrief) })
                    .await?;
                self.send_hangar().await?;
            }
        }
        Ok(())
    }

    /// What the tugs brought home goes to the stores: the wreck of the suit the pilot ejected from,
    /// or of the ace they downed (`ace`). Its notes.
    fn towed(&mut self, wreck: Option<&bc_proto::ChunkDesc>, torso: bool, ace: Option<u8>) -> Vec<String> {
        match ace {
            Some(a) => self.hangar.towed_ace(wreck, bc_sim::content::aces::ace(a).name),
            None => self.hangar.towed(wreck, torso),
        }
    }

    /// The pilot downed one of Zodiac's aces (`bc_sim::content::aces`): on the Most Wanted, and its
    /// bounty as their terms say: paid, or the rights to its wreck, which the tugs go out for.
    async fn ace_down(&mut self, ace: u8, hulk: u16, generation: u8) -> anyhow::Result<()> {
        if !self.survival() {
            return Ok(());
        }
        let a = bc_sim::content::aces::ace(ace);
        let now = pilots::unix_now();
        let (trader, name) = (self.trader.as_str(), self.callsign.as_str());
        let salvage = self.game.charter.with(|b| b.ace_downed(ace, trader, name, now));
        tracing::info!(slot = self.slot, name = %self.callsign, ace = a.name, salvage, "an ace downed");
        // The rights to its wreck, if the tugs aren't out for another already (else, pay).
        let claimed = salvage
            && hulk != bc_proto::NO_CHUNK
            && !self.towing
            && self
                .game
                .sector
                .control
                .push(Control::Claim { slot: self.slot, hulk, generation, ace })
                .is_ok();
        let text = if claimed {
            self.towing = true;
            format!("{} DOWNED · YOUR TERMS: ITS WRECK · THE TUGS ARE GOING OUT FOR IT", a.name)
        } else {
            let (hangar, trader) = (&mut self.hangar, self.trader.as_str());
            let text = self.game.charter.with(|b| b.pay_ace(hangar, ace, trader));
            self.save().await;
            self.send_hangar().await?;
            text
        };
        self.game.charter.changed();
        self.send(&Update::News { text }).await
    }

    /// A tenth of a second on: the suit's reports, its wreck clearing, the stations (`settle`: once
    /// a second) and the market.
    async fn on_tick(&mut self, settle: bool) -> anyhow::Result<()> {
        while let Ok(report) = self.lease().reports.pop() {
            self.on_report(report).await?;
        }
        while let Some(report) = self.inside.as_mut().and_then(|l| l.reports.pop().ok()) {
            self.on_report(report).await?;
        }
        if self.lost && self.game.sector.slots[self.slot as usize].state() == SlotState::Free {
            // The wreck is gone: the pilot is back in the hangar (and, lost everything, advanced
            // another suit, unless the tugs are still bringing their own wreck home).
            self.lost = false;
            self.unseat();
            self.place = Place::Hangar;
            self.send_place().await?;
            if !self.towing
                && let Some(text) = self.hangar.reissue(pilots::unix_now())
            {
                self.save().await;
                self.send(&Update::News { text }).await?;
            }
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
        if matches!(self.place, Place::Hangar | Place::City)
            && self.game.market.version() != self.market_seen
            && self.market_sent.elapsed() >= MARKET_EVERY
        {
            self.send_market().await?;
        }
        if self.board
            && matches!(self.place, Place::Hangar | Place::City)
            && self.game.charter.version() != self.board_seen
            && self.board_sent.elapsed() >= MARKET_EVERY
        {
            self.send_board().await?;
        }
        if settle && self.game.charter.with(|b| b.news_seq()) != self.news_seen {
            self.send_notices().await?;
        }
        if self.in_the_colony()
            && self.game.proving.version() != self.proving_seen
            && self.proving_sent.elapsed() >= MARKET_EVERY
        {
            self.send_proving().await?;
        }
        Ok(())
    }

    /// The pilot is gone: their suit sleeps (signed in) or goes, their record is kept, and the
    /// slot is handed back.
    async fn leave(&mut self) {
        self.game.plaza.leave(self.slot);
        self.unwatch().await;
        // Inside the colony, the suit doesn't sleep there: the colony's tugs bring it back to the
        // bay (a trainer, back to its gantry: the pilot's own suit never left the bay).
        if self.inside.is_some() {
            self.leave_the_colony().await;
            if self.survival() && !self.trainer {
                self.hangar.recover();
            }
            self.trainer = false;
        }
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
                    roster_flags::VERIFIED
                        | roster_flags::ASLEEP
                        | roster_flags::weathering(self.hangar.weathering()),
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
            let (mut parked, mut bounty, mut home) = (None, 0, None);
            while let Some(report) = self.lease.as_mut().and_then(|l| l.reports.pop().ok()) {
                match report {
                    Report::Parked { rec, tick } => parked = Some((rec, tick)),
                    Report::Lost { bounty: b, .. } => bounty = b,
                    // Left before the catapult threw it out: it's still in the bay.
                    Report::Home(h) => home = Some(h),
                    Report::Towed { wreck, torso, ace } => {
                        self.towing = false;
                        let _ = self.towed(wreck.as_ref(), torso, ace);
                    }
                    // An ace downed on the way out: on the Most Wanted, and paid (whatever the
                    // terms: there's no one here to send the tugs for).
                    Report::AceDown { ace, .. } if self.survival() => {
                        let now = pilots::unix_now();
                        let (hangar, trader, name) =
                            (&mut self.hangar, self.trader.as_str(), self.callsign.as_str());
                        game.charter.with(|b| {
                            b.ace_downed(ace, trader, name, now);
                            b.pay_ace(hangar, ace, trader)
                        });
                        game.charter.changed();
                    }
                    Report::DockRefused
                    | Report::Course { .. }
                    | Report::Drill { .. }
                    | Report::AceDown { .. } => {}
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
                    match home {
                        Some(h) => {
                            self.hangar.came_home(&h);
                        }
                        None => {
                            self.hangar.lost(bounty);
                        }
                    }
                }
            }
            if let Some(r) = self.record.as_mut() {
                r.sleeper = asleep;
                r.parked = parked;
            }
        }
        // The wreck the tugs were going out for comes in now, before the record is saved.
        if self.towing && self.survival() {
            let mut ask = Control::Tow { slot };
            while let Err(back) = game.sector.control.push(ask) {
                ask = back;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            for _ in 0..500 {
                match self.lease.as_mut().and_then(|l| l.reports.pop().ok()) {
                    Some(Report::Towed { wreck, torso, ace }) => {
                        let _ = self.towed(wreck.as_ref(), torso, ace);
                        break;
                    }
                    Some(_) => {}
                    None => tokio::time::sleep(Duration::from_millis(2)).await,
                }
            }
            self.towing = false;
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
