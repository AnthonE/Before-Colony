//! [`BotClient`]: an agent's connection to a sector.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use bc_auth::LocalWallet;
use bc_client_core::{ClientConfig, ClientCore, Identity, InputContext, Phase, World};
use bc_econ::wire::{Place, Request};
use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
use tokio::sync::mpsc;
use wtransport::{Connection, SendStream};

use crate::transport::{EndpointInfo, connect, discover};

/// How an agent identifies itself.
#[derive(Clone, Debug)]
pub struct BotConfig {
    /// `http://host:port` of a dev server (discovers the port and certificate hash), or a
    /// `https://host:port/bc` WebTransport URL with a real certificate.
    pub server: String,
    pub name: String,
    pub frame: FrameId,
    pub faction: Faction,
}

/// A connected agent. Drive it with [`BotClient::step`] from a loop.
pub struct BotClient {
    conn: Connection,
    ctrl_tx: SendStream,
    ctrl_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    pub core: ClientCore,
    epoch: Instant,
}

impl BotClient {
    /// Connects as a guest, sends the handshake (as [`PilotKind::Agent`]) and waits for the Welcome.
    pub async fn connect(cfg: &BotConfig) -> anyhow::Result<Self> {
        Self::connect_as(cfg, None).await
    }

    /// Connects signed in with `wallet` (its suit sleeps in the sector when it leaves, and wakes
    /// when it comes back), or as a guest.
    pub async fn connect_as(cfg: &BotConfig, wallet: Option<&LocalWallet>) -> anyhow::Result<Self> {
        let identity =
            wallet.map_or(Identity::Guest, |w| Identity::Wallet { address: w.address(), resume: None });
        Self::connect_with(cfg, identity, wallet).await
    }

    /// Connects as `identity`, signing with `wallet` if the server asks (a resume token in the
    /// identity reconnects without signing).
    pub async fn connect_with(
        cfg: &BotConfig,
        identity: Identity,
        wallet: Option<&LocalWallet>,
    ) -> anyhow::Result<Self> {
        let info = if cfg.server.starts_with("http://") {
            discover(&cfg.server).await.context("discovering the server")?
        } else {
            EndpointInfo { url: cfg.server.clone(), cert_hash: None }
        };
        let conn = connect(&info.url, info.cert_hash).await.context("WebTransport connect")?;
        let (mut tx, mut rx) = conn.open_bi().await?.await?;
        let core = ClientCore::new(ClientConfig {
            name: cfg.name.clone(),
            pilot: PilotKind::Agent,
            frame: cfg.frame,
            faction: cfg.faction,
        })
        .with_identity(identity);
        tx.write_all(&core.hello()).await?;
        let (ctrl_in, ctrl_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            while let Ok(Some(n)) = rx.read(&mut buf).await {
                if ctrl_in.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut bot = Self { conn, ctrl_tx: tx, ctrl_rx, core, epoch: Instant::now() };
        let deadline = Instant::now() + Duration::from_secs(5);
        while matches!(bot.core.phase, Phase::Handshake | Phase::Signing(_)) {
            if Instant::now() > deadline {
                bail!("no Welcome within 5 s");
            }
            if let (Phase::Signing(c), Some(w)) = (bot.core.phase, wallet) {
                let signature = w.sign_in(c.domain.as_str(), &c.nonce, c.issued_at);
                let auth = bot.core.auth(signature);
                bot.ctrl_tx.write_all(&auth).await?;
                continue;
            }
            match tokio::time::timeout(Duration::from_millis(100), bot.ctrl_rx.recv()).await {
                Ok(Some(bytes)) => bot.core.on_control(&bytes),
                Ok(None) => bail!("control stream closed during handshake"),
                Err(_) => {}
            }
        }
        if let Phase::Rejected(reason) = bot.core.phase {
            bail!("server rejected us: {reason:?}");
        }
        Ok(bot)
    }

    /// Seconds since connecting (the client clock).
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    pub fn world(&self) -> &World {
        &self.core.world
    }

    /// Receives for up to ~10 ms, then sends every input that is due. `brain` decides the controls
    /// for each tick from the agent's (sensor-limited) view.
    pub async fn step<F>(&mut self, brain: &mut F) -> anyhow::Result<()>
    where
        F: FnMut(&InputContext) -> InputCmd + Send,
    {
        let until = tokio::time::Instant::now() + Duration::from_millis(10);
        loop {
            tokio::select! {
                d = self.conn.receive_datagram() => {
                    let Ok(d) = d else {
                        // What the server said last (a goodbye, and why) still counts.
                        self.drain_control().await;
                        bail!("connection lost ({:?})", self.core.phase);
                    };
                    let now = self.now();
                    self.core.on_datagram(&d, now);
                }
                c = self.ctrl_rx.recv() => match c {
                    Some(bytes) => self.core.on_control(&bytes),
                    None => bail!("control stream closed"),
                },
                _ = tokio::time::sleep_until(until) => break,
            }
        }
        let now = self.now();
        for p in self.core.poll_inputs(now, brain) {
            // Oversized/unsendable datagrams are dropped like any lost packet.
            let _ = self.conn.send_datagram(p);
        }
        // On foot in the colony: where the agent stands (`ClientCore::set_pose`).
        if let Some(p) = self.core.poll_pose(now) {
            let _ = self.conn.send_datagram(p);
        }
        self.core.frame(now, 0.01);
        if matches!(self.core.phase, Phase::Closed | Phase::Rejected(_)) {
            bail!("session ended: {:?}", self.core.phase);
        }
        Ok(())
    }

    /// Takes whatever the control stream still has to say (briefly waiting for it).
    async fn drain_control(&mut self) {
        while let Ok(Some(bytes)) =
            tokio::time::timeout(Duration::from_millis(100), self.ctrl_rx.recv()).await
        {
            self.core.on_control(&bytes);
        }
    }

    /// Runs `brain` for `duration`.
    pub async fn run_for<F>(&mut self, duration: Duration, brain: &mut F) -> anyhow::Result<()>
    where
        F: FnMut(&InputContext) -> InputCmd + Send,
    {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            self.step(brain).await?;
        }
        Ok(())
    }

    /// Survival rules: where the pilot is (`None`: arcade rules).
    pub fn place(&self) -> Option<Place> {
        self.core.hangar.place
    }

    /// Whether the server plays survival rules (the pilot flies the suit in their hangar).
    pub fn survival(&self) -> bool {
        self.core.welcome.is_some_and(|w| w.survival)
    }

    /// Asks the hangar for something (survival rules); its answer comes as a note
    /// (`core.hangar.notes`) and a new view of the hangar.
    pub async fn request(&mut self, req: &Request) -> anyhow::Result<()> {
        let bytes = self.core.request(req);
        self.ctrl_tx.write_all(&bytes).await?;
        Ok(())
    }

    /// Steps, hands off, until `done` holds; fails after `secs`, or if the hangar refuses
    /// something meanwhile (with its reason).
    pub async fn wait_until(
        &mut self,
        secs: f64,
        what: &str,
        done: impl Fn(&ClientCore) -> bool,
    ) -> anyhow::Result<()> {
        let refusals = self.core.hangar.notes.iter().filter(|(_, ok)| !ok).count();
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while !done(&self.core) {
            if let Some((why, _)) = self.core.hangar.notes.iter().filter(|(_, ok)| !ok).nth(refusals) {
                bail!("{what}: refused: {why}");
            }
            if Instant::now() > end {
                bail!("timed out waiting for {what}");
            }
            self.step(&mut |_| InputCmd::default()).await?;
        }
        Ok(())
    }

    /// Survival, with the colony open: rides the cap lift down to strip `strip`'s Hub Gate, and
    /// returns once there. Set where the agent stands with [`BotClient::set_pose`].
    pub async fn enter_city(&mut self, strip: u8) -> anyhow::Result<()> {
        self.request(&Request::EnterCity { strip }).await?;
        self.wait_until(10.0, "the city", |c| c.hangar.in_city()).await
    }

    /// Rides back up to the bay.
    pub async fn leave_city(&mut self) -> anyhow::Result<()> {
        self.request(&Request::LeaveCity).await?;
        self.wait_until(10.0, "the bay", |c| c.hangar.in_hangar()).await
    }

    /// Where the agent stands in the city: sent 15 times a second as it steps.
    pub fn set_pose(&mut self, pose: bc_proto::presence::PersonPose) {
        self.core.set_pose(Some(pose));
    }

    /// The people near the agent in the city, as drawn now: slot, name and pose.
    pub fn people(&self) -> Vec<(u16, String, bc_proto::presence::PersonPose)> {
        self.core.people(self.now()).into_iter().map(|(id, name, p)| (id, name.to_string(), p)).collect()
    }

    /// Survival rules: boards the suit in the bay and launches it. Returns once the pilot is out
    /// in the sector, flying it.
    pub async fn launch(&mut self) -> anyhow::Result<()> {
        self.request(&Request::Launch).await?;
        self.wait_until(10.0, "the launch", |c| {
            c.hangar.place == Some(Place::Space) && c.world.own.is_some_and(|o| o.alive)
        })
        .await
    }

    /// Survival rules, the colony open: boards the suit in the bay and launches it into the
    /// colony through the inner gate. Returns once the pilot is flying it in there (welcomed to the
    /// colony's inside).
    pub async fn launch_inside(&mut self) -> anyhow::Result<()> {
        self.request(&Request::LaunchInside).await?;
        self.wait_until(10.0, "the launch into the colony", |c| {
            c.hangar.place == Some(Place::Space)
                && c.welcome.is_some_and(|w| w.interior)
                && c.world.own.is_some_and(|o| o.alive)
        })
        .await
    }

    /// Gets the agent flying, whatever the rules: under survival rules, a pilot in the hangar
    /// launches the suit in the bay (`false` if there isn't one: it was lost); under arcade
    /// rules, or already out, there's nothing to do.
    pub async fn sortie(&mut self) -> anyhow::Result<bool> {
        if !self.survival() || self.place() == Some(Place::Space) {
            return Ok(true);
        }
        self.wait_until(5.0, "the hangar", |c| c.hangar.view.is_some()).await?;
        let suit =
            self.core.hangar.view.as_ref().is_some_and(|v| matches!(v.bay, bc_econ::Bay::Docked { .. }));
        if suit {
            self.launch().await?;
        }
        Ok(suit)
    }

    /// Says `text` on the colony's radio (any rules): everyone connected hears it, this agent too.
    pub async fn say(&mut self, text: &str) -> anyhow::Result<()> {
        self.request(&Request::Say { text: text.to_string() }).await
    }

    /// What's been heard on the colony's radio since this agent connected: who said what, oldest
    /// first.
    pub fn heard(&self) -> &[(String, String)] {
        &self.core.hangar.said
    }

    /// Asks the hangar for something and waits for its answer: what it said, or why it refused.
    pub async fn ask(&mut self, req: &Request) -> anyhow::Result<String> {
        let n = self.core.hangar.notes.len();
        self.request(req).await?;
        let end = Instant::now() + Duration::from_secs(5);
        while self.core.hangar.notes.len() <= n {
            if Instant::now() > end {
                bail!("no answer to {req:?}");
            }
            self.step(&mut |_| InputCmd::default()).await?;
        }
        let (text, ok) = self.core.hangar.notes[n].clone();
        if ok { Ok(text) } else { bail!("{text}") }
    }

    /// Survival rules: sells everything the stores hold of `item` to whoever pays best right now.
    pub async fn sell_all(&mut self, item: bc_econ::Item) -> anyhow::Result<Option<String>> {
        let have = self
            .core
            .hangar
            .view
            .as_ref()
            .map_or(0, |v| v.stock.iter().find(|(i, _)| *i == item).map_or(0, |(_, q)| *q));
        if have == 0 {
            return Ok(None);
        }
        let req = Request::Order { item, side: bc_econ::Side::Sell, price: 1, qty: have, rest: false };
        self.ask(&req).await.map(Some)
    }

    /// Survival rules, in flight: uses a consumable from the suit's rack. Returns once the own
    /// snapshot shows one fewer in the rack (or fails after a second: none there, or nothing for it
    /// to do).
    pub async fn use_kit(&mut self, kit: bc_sim::content::Kit) -> anyhow::Result<()> {
        use bc_sim::content::Kits;
        let count = |c: &ClientCore| c.world.own.map_or(0, |o| Kits(o.kits).get(kit));
        let before = count(&self.core);
        anyhow::ensure!(before > 0, "no {} in the rack", kit.name());
        self.request(&Request::UseKit { kit }).await?;
        self.wait_until(1.0, kit.name(), |c| count(c) < before).await
    }

    /// Whether the agent's suit is doomed: its torso breached, its reactor going (`own.doom`
    /// ticks left). Its pilot can still [`eject`](Self::eject) or
    /// [`self_destruct`](Self::self_destruct).
    pub fn doomed(&self) -> bool {
        self.core.world.own.is_some_and(|o| o.alive && o.doom > 0)
    }

    /// Ejects from the suit (doomed or not; never inside the colony): the suit is lost, and under
    /// survival rules the colony's tugs go out for its wreck (`docs/DESIGN.md`, "Doom and
    /// ejecting"). Returns once the own snapshot shows the suit gone (or fails after a second).
    pub async fn eject(&mut self) -> anyhow::Result<()> {
        self.leave_suit(false).await
    }

    /// Doomed: blows the suit up with the agent aboard, hurting every enemy suit close by
    /// (nothing is left of it to salvage). Returns once the suit is gone (or fails after a second:
    /// it wasn't doomed).
    pub async fn self_destruct(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(self.doomed(), "only a doomed suit can be blown up");
        self.leave_suit(true).await
    }

    async fn leave_suit(&mut self, destruct: bool) -> anyhow::Result<()> {
        anyhow::ensure!(self.core.world.own.is_some_and(|o| o.alive), "no suit to leave");
        self.request(&Request::Eject { destruct }).await?;
        self.wait_until(1.0, if destruct { "self-destruct" } else { "eject" }, |c| {
            c.world.own.is_none_or(|o| !o.alive)
        })
        .await
    }

    /// Survival rules: delivers what the stores hold of `item` to the Charter Board's supply
    /// contracts that ask for it (the colony's pay above its desks), best paying first. What the
    /// board said to each delivery.
    pub async fn deliver_all(&mut self, item: bc_econ::Item) -> anyhow::Result<Vec<String>> {
        use bc_econ::charter::Task;
        self.request(&Request::WatchBoard { on: true }).await?;
        let seen = self.core.hangar.charter.is_some();
        let v0 = self.core.hangar.version;
        self.wait_until(5.0, "the Charter Board", |c| {
            c.hangar.charter.is_some() && (seen || c.hangar.version > v0)
        })
        .await?;
        let mut wanted: Vec<(u64, u64, u64)> = self
            .core
            .hangar
            .charter
            .as_ref()
            .map(|b| {
                b.contracts
                    .iter()
                    .filter(|c| !c.mine)
                    .filter_map(|c| match c.task {
                        Task::Supply { item: i, qty, delivered } if i == item && qty > delivered => {
                            Some((c.id, qty - delivered, (c.reward - c.paid) * 1_000 / (qty - delivered)))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        wanted.sort_by_key(|w| std::cmp::Reverse(w.2));
        let mut said = Vec::new();
        for (id, left, _) in wanted {
            let have = self
                .core
                .hangar
                .view
                .as_ref()
                .map_or(0, |v| v.stock.iter().find(|(i, _)| *i == item).map_or(0, |(_, q)| *q));
            if have == 0 {
                break;
            }
            match self.ask(&Request::Deliver { id, qty: left.min(have) }).await {
                Ok(text) => said.push(text),
                Err(e) => said.push(format!("{e:#}")),
            }
        }
        self.request(&Request::WatchBoard { on: false }).await?;
        Ok(said)
    }

    /// Survival rules: buys up to `qty` of `item` at no more than `price` (credits a tonne, or a
    /// piece), whatever fills now.
    pub async fn buy(&mut self, item: bc_econ::Item, qty: u64, price: u64) -> anyhow::Result<String> {
        self.ask(&Request::Order { item, side: bc_econ::Side::Buy, price, qty, rest: false }).await
    }

    /// The colony open, on foot in its city: walks the agent from where it stands to `to` (`(s, x)`
    /// on its strip) with the browser's own legs and guide (`bc_client_core::{walker, city_nav}`),
    /// in through a room's door if it's in one, the server taking every step. Fails past `secs`.
    pub async fn walk_to(&mut self, to: (f32, f32), secs: f64) -> anyhow::Result<()> {
        use bc_client_core::city::{CityGround, pose_of};
        use bc_client_core::walker::{Guide, Walker};
        use bc_sim::colony::city::{KERB, Stage, room_at};
        use bc_sim::colony::frame::CityPos;
        let strip = self.core.hangar.strip.unwrap_or(0);
        // From where it last stood; just down the lift, from outside Hub Gate's door.
        let from = match self.core.pose() {
            Some(p) => CityPos::new(strip, p.x, p.s, p.h),
            None => {
                let gate = bc_sim::content::city::PLACES
                    .iter()
                    .find(|p| p.kind == bc_sim::content::city::PlaceKind::HubGate && p.strip == strip)
                    .ok_or_else(|| anyhow::anyhow!("walk_to: no Hub Gate on strip {strip}"))?;
                let ((s, x), (ds, dx)) = bc_sim::colony::city::place_door(gate);
                CityPos::new(strip, x - dx * 6.0, s - ds * 6.0, 0.0)
            }
        };
        let mut route = bc_client_core::city_nav::route_from(strip, from, to);
        // Into the room `to` is in, through its door.
        if let Some(r) = room_at(strip, to.0, to.1, 1.0)
            && room_at(strip, from.s, from.x, from.h.max(0.0) + 1.0).is_none()
        {
            let (outside, inside) = r.threshold();
            route = bc_client_core::city_nav::route_from(strip, from, outside);
            let walk = |(s, x): (f32, f32)| CityPos::new(strip, x, s, KERB).walker();
            route.push(walk(inside));
            route.push(walk(to));
        }
        let ground = CityGround { strip, stage: Stage(0) };
        let mut w = Walker::at(from.walker(), glam::Vec3::X);
        w.grounded = true;
        let mut guide = Guide::new(route, None);
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while !guide.arrived() {
            if Instant::now() > end {
                bail!("walk_to: timed out, at {:?}", CityPos::from_walker(strip, w.feet));
            }
            let dt = 1.0 / bc_sim::TICK_HZ as f32;
            let stride = guide.steer(&mut w, dt);
            w.step(&ground, &stride, dt);
            self.set_pose(pose_of(strip, &w));
            self.step(&mut |_| InputCmd::default()).await?;
        }
        Ok(())
    }

    /// The colony open, on foot in the Blast Hall at its gantry's hatch: boards one of the Charter
    /// Board's trainers there. Returns once the pilot is flying it (welcomed to the colony's
    /// inside), standing on the gantry.
    pub async fn board_trainer(&mut self) -> anyhow::Result<()> {
        self.request(&Request::BoardTrainer).await?;
        self.wait_until(10.0, "a trainer", |c| {
            c.hangar.place == Some(Place::Space)
                && c.hangar.trainer
                && c.welcome.is_some_and(|w| w.interior)
                && c.world.own.is_some_and(|o| o.alive)
        })
        .await
    }

    /// Survival rules: takes the suit into the bay (it has to be at rest in the dock). Returns
    /// once the pilot is back in the hangar.
    pub async fn dock(&mut self) -> anyhow::Result<()> {
        self.request(&Request::Dock).await?;
        self.wait_until(5.0, "docking", |c| c.hangar.place == Some(Place::Hangar)).await
    }

    /// Requests a respawn in `frame` (after death).
    pub async fn respawn(&mut self, frame: FrameId) -> anyhow::Result<()> {
        let bytes = self.core.respawn(frame);
        self.ctrl_tx.write_all(&bytes).await?;
        Ok(())
    }

    /// Says goodbye and closes the connection.
    pub async fn close(mut self) {
        let _ = self.ctrl_tx.write_all(&self.core.bye(bc_proto::control::bye::LEAVE)).await;
        let _ = tokio::time::timeout(Duration::from_millis(200), self.ctrl_tx.finish()).await;
        self.conn.close(0u32.into(), b"bye");
    }

    /// Drops the connection without a goodbye (as a crashed client or a lost network would).
    pub fn drop_link(self) {
        self.conn.close(0u32.into(), b"");
    }
}
