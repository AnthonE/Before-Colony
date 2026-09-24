//! Before Colony game server.
//!
//! Threads:
//! - **tokio (2 workers)**: WebTransport accept loop, one task per connection (control stream plus
//!   datagram ingress), the oracle worker, and the axum dev HTTP server.
//! - **`sector-0`**: owns the simulation. It never touches tokio and never locks or allocates while
//!   ticking; see `bc-sector`.
//! - **`egress-0`**: parked until the sector unparks it once per tick, then drains the per-slot
//!   packet rings into `send_datagram`.
//!
//! [`start`] boots everything in-process, which is also how the integration tests run a real server.

pub mod config;
pub mod http;
pub mod net;
pub mod telemetry;

use std::net::SocketAddr;
use std::sync::Arc;

pub use config::{Config, Mode, OracleKind};

/// Handle to a running server. Dropping it does not stop the server; call [`ServerHandle::shutdown`].
pub struct ServerHandle {
    /// UDP port of the WebTransport endpoint.
    pub wt_port: u16,
    /// Address of the dev HTTP server (static client, `/cert-hash`, `/status`).
    pub http_addr: SocketAddr,
    /// SHA-256 of the self-signed certificate, for `serverCertificateHashes`.
    pub cert_hash: [u8; 32],
    /// Live counters shared with `/status`.
    pub stats: Arc<net::NetStats>,
    shutdown: tokio::sync::watch::Sender<bool>,
    game: Option<net::game::GameRuntime>,
    status_view: Option<net::game::StatusView>,
}

impl ServerHandle {
    /// URL clients connect to, e.g. `https://127.0.0.1:4433/bc`.
    pub fn wt_url(&self) -> String {
        format!("https://127.0.0.1:{}/bc", self.wt_port)
    }

    /// JSON snapshot of the server's metrics (same document `/status` serves).
    pub fn status(&self) -> serde_json::Value {
        net::status_json(&self.stats, self.status_view.as_ref())
    }

    /// Stops accepting connections and shuts the sector threads down.
    pub fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(game) = self.game.take() {
            game.stop();
        }
    }
}

/// Installs the process-wide rustls crypto provider (ring, the same one wtransport uses). Safe to
/// call more than once.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Boots the server: WebTransport endpoint, sector runtime (game mode) and dev HTTP.
pub async fn start(cfg: Config) -> anyhow::Result<ServerHandle> {
    install_crypto_provider();
    let (identity, cert_hash) = net::endpoint::self_signed_identity()?;
    let endpoint = net::endpoint::make_endpoint(cfg.wt_port, identity)?;
    let wt_port = endpoint.local_addr()?.port();
    let stats = Arc::new(net::NetStats::default());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let game = match cfg.mode {
        Mode::Echo => None,
        Mode::Game => Some(net::game::GameRuntime::start(&cfg, stats.clone())?),
    };
    let shared = game.as_ref().map(|g| g.shared());

    let listener = tokio::net::TcpListener::bind(cfg.http_addr).await?;
    let http_addr = listener.local_addr()?;
    let status_stats = stats.clone();
    let status_view = game.as_ref().map(|g| g.status_view());
    let status_game = status_view.clone();
    let http_state = http::HttpState {
        cert_hash_hex: hex::encode(cert_hash),
        wt_port,
        web_dir: cfg.web_dir.clone(),
        status: Arc::new(move || net::status_json(&status_stats, status_game.as_ref())),
    };
    let router = http::router(http_state);
    let mut http_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = http_shutdown.changed().await;
        });
        if let Err(e) = serve.await {
            tracing::error!("http server stopped: {e}");
        }
    });

    tokio::spawn(net::endpoint::accept_loop(endpoint, cfg.mode, stats.clone(), shared, shutdown_rx));

    tracing::info!(
        mode = ?cfg.mode,
        wt_port,
        http = %http_addr,
        cert_hash = %hex::encode(cert_hash),
        "server up"
    );
    Ok(ServerHandle { wt_port, http_addr, cert_hash, stats, shutdown: shutdown_tx, game, status_view })
}
