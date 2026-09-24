//! [`BotClient`]: an agent's connection to a sector.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use bc_client_core::{ClientConfig, ClientCore, InputContext, Phase, World};
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
    /// Connects, sends the handshake (as [`PilotKind::Agent`]) and waits for the Welcome.
    pub async fn connect(cfg: &BotConfig) -> anyhow::Result<Self> {
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
        });
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
        while bot.core.phase == Phase::Handshake {
            if Instant::now() > deadline {
                bail!("no Welcome within 5 s");
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
                    let d = d.context("connection lost")?;
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
        self.core.frame(now, 0.01);
        if matches!(self.core.phase, Phase::Closed | Phase::Rejected(_)) {
            bail!("session ended: {:?}", self.core.phase);
        }
        Ok(())
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

    /// Requests a respawn in `frame` (after death).
    pub async fn respawn(&mut self, frame: FrameId) -> anyhow::Result<()> {
        let bytes = self.core.respawn(frame);
        self.ctrl_tx.write_all(&bytes).await?;
        Ok(())
    }

    pub fn close(self) {
        self.conn.close(0u32.into(), b"bye");
    }
}
