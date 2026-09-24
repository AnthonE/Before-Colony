//! WebTransport endpoint setup and the accept loop.

use std::sync::Arc;
use std::time::Duration;

use wtransport::config::IpBindConfig;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Endpoint, Identity, ServerConfig};

use super::NetStats;
use crate::Mode;

/// Path clients must request; anything else gets a 404.
pub const WT_PATH: &str = "/bc";

/// Self-signed ECDSA P-256 identity valid for 14 days: the constraints browsers impose on
/// `serverCertificateHashes`. Returns the identity and the certificate's SHA-256.
pub fn self_signed_identity() -> anyhow::Result<(Identity, [u8; 32])> {
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])?;
    let hash = *identity.certificate_chain().as_slice()[0].hash().as_ref();
    Ok((identity, hash))
}

/// Binds dual-stack when the host has IPv6, IPv4-only otherwise (e.g. containers without IPv6).
pub fn make_endpoint(port: u16, identity: Identity) -> anyhow::Result<Endpoint<Server>> {
    let build = |bind: IpBindConfig, identity: Identity| -> anyhow::Result<ServerConfig> {
        Ok(ServerConfig::builder()
            .with_bind_config(bind, port)
            .with_identity(identity)
            .keep_alive_interval(Some(Duration::from_secs(3)))
            .max_idle_timeout(Some(Duration::from_secs(10)))?
            .build())
    };
    match Endpoint::server(build(IpBindConfig::InAddrAnyDual, identity.clone_identity())?) {
        Ok(endpoint) => Ok(endpoint),
        Err(e) => {
            tracing::info!("dual-stack bind failed ({e}); using IPv4 only");
            Ok(Endpoint::server(build(IpBindConfig::InAddrAnyV4, identity)?)?)
        }
    }
}

pub async fn accept_loop(
    endpoint: Endpoint<Server>,
    mode: Mode,
    stats: Arc<NetStats>,
    game: Option<super::game::GameShared>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        let incoming = tokio::select! {
            incoming = endpoint.accept() => incoming,
            _ = shutdown.changed() => break,
        };
        let stats = stats.clone();
        let game = game.clone();
        tokio::spawn(async move {
            let request = match incoming.await {
                Ok(request) => request,
                Err(e) => {
                    tracing::debug!("handshake failed: {e}");
                    return;
                }
            };
            if request.path() != WT_PATH {
                request.not_found().await;
                return;
            }
            let conn = match request.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::debug!("session accept failed: {e}");
                    return;
                }
            };
            NetStats::add(&stats.sessions_total, 1);
            NetStats::add(&stats.sessions_active, 1);
            match (mode, game) {
                (Mode::Echo, _) => super::echo::run(conn, stats.clone()).await,
                (Mode::Game, Some(game)) => super::game::run_session(conn, game, stats.clone()).await,
                (Mode::Game, None) => tracing::error!("game mode without a sector"),
            }
            stats.sessions_active.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        });
    }
    endpoint.close(0u32.into(), b"server shutting down");
}
