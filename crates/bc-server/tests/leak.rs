//! What QUIC really holds, over real sockets (`net::endpoint::accept_loop`): a peer that finishes
//! QUIC's handshake but grants no stream credit, so the server can't even send its SETTINGS, is
//! let go once its handshake deadline passes, however long it keeps the link alive. That was
//! wtransport's remote connection leak, fixed upstream in 1df1c05 (the pin in `Cargo.toml`): the
//! accept loop gave the pass back and the connection lived on, out of the count. Past the
//! ceiling, counted on what QUIC holds rather than on the passes, a new connection is refused; and
//! one address opening and closing connections as fast as it can (each lingering in QUIC's count
//! while it closes) can't fill that ceiling and shut everyone else out.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bc_server::net::admit::{HANDSHAKE, Limits, QUIC_HEADROOM};
use bc_server::{Config, Mode};
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use wtransport::quinn;

/// Takes the server's certificate by its hash, as a browser pins it (`serverCertificateHashes`).
#[derive(Debug)]
struct Pinned {
    hash: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let hash = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref());
        if hash.as_ref() == self.hash {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not the server's certificate".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// A raw QUIC peer on 127.0.0.1 that speaks HTTP/3's ALPN but grants the server no
/// unidirectional streams (so its control stream and SETTINGS can never go), and keeps the link
/// alive every 200 ms.
fn stingy_peer(cert_hash: [u8; 32]) -> anyhow::Result<quinn::Endpoint> {
    stingy_peer_on([127, 0, 0, 1], cert_hash)
}

/// The same, from another loopback address.
fn stingy_peer_on(ip: [u8; 4], cert_hash: [u8; 32]) -> anyhow::Result<quinn::Endpoint> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { hash: cert_hash, provider }))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let mut transport = quinn::TransportConfig::default();
    transport.max_concurrent_uni_streams(0u32.into());
    transport.keep_alive_interval(Some(Duration::from_millis(200)));
    let mut client =
        quinn::ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(tls)?));
    client.transport_config(Arc::new(transport));
    let mut endpoint = quinn::Endpoint::client(SocketAddr::from((ip, 0)))?;
    endpoint.set_default_client_config(client);
    Ok(endpoint)
}

fn config(limits: Limits) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Echo,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        limits,
        ..Config::default()
    })
}

/// What `/status` says of a counter, once it says `want` (or `secs` have passed).
async fn status_reaches(server: &bc_server::ServerHandle, key: &str, want: u64, secs: f64) -> u64 {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    loop {
        let now = server.status()["net"][key].as_u64().unwrap_or(u64::MAX);
        if now == want || Instant::now() > end {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_stuck_before_settings_is_let_go_at_its_deadline() -> anyhow::Result<()> {
    let server = bc_server::start(config(Limits::default())?).await?;
    let peer = stingy_peer(server.cert_hash)?;
    let at = SocketAddr::from(([127, 0, 0, 1], server.wt_port));
    let conn = peer.connect(at, "localhost")?.await?;
    // QUIC holds it while its handshakes may still finish...
    assert_eq!(status_reaches(&server, "open_connections", 1, 2.0).await, 1, "the connection isn't counted");
    // ...and past their deadline it's let go, though the peer would keep it alive for ever.
    let closed = tokio::time::timeout(HANDSHAKE + Duration::from_secs(3), conn.closed()).await;
    assert!(closed.is_ok(), "the server kept a connection that never finished its handshake");
    assert_eq!(status_reaches(&server, "open_connections", 0, 3.0).await, 0, "QUIC still holds it");
    assert_eq!(server.status()["net"]["handshake_timeouts"], 1);
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn past_the_ceiling_quic_holds_a_connection_is_refused() -> anyhow::Result<()> {
    // QUIC may hold one connection in all, loopback's too (the passes would take more).
    let limits = Limits { connections: 8, quic_connections: 1, ..Limits::default() };
    let server = bc_server::start(config(limits)?).await?;
    let at = SocketAddr::from(([127, 0, 0, 1], server.wt_port));
    let first = stingy_peer(server.cert_hash)?;
    let _held = first.connect(at, "localhost")?.await?;
    let second = stingy_peer(server.cert_hash)?;
    let refused = tokio::time::timeout(Duration::from_secs(3), second.connect(at, "localhost")?).await?;
    assert!(refused.is_err(), "a connection past the ceiling got in");
    let net = &server.status()["net"];
    assert_eq!((net["refused_open"].as_u64(), net["refused_full"].as_u64()), (Some(1), Some(0)));
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_address_opening_and_closing_cant_shut_the_others_out() -> anyhow::Result<()> {
    // Four connections in all, two an address, loopback counted as any other address.
    let limits = Limits {
        connections: 4,
        quic_connections: 4 * QUIC_HEADROOM,
        per_address: 2,
        loopback_excepted: false,
        ..Limits::default()
    };
    let server = bc_server::start(config(limits)?).await?;
    let at = SocketAddr::from(([127, 0, 0, 1], server.wt_port));
    // One address connects and closes, again and again: each connection gives its pass back at
    // once but stays in QUIC's count while it closes.
    let churner = stingy_peer_on([127, 0, 0, 2], server.cert_hash)?;
    let mut opened = 0;
    for _ in 0..20 {
        if let Ok(Ok(conn)) =
            tokio::time::timeout(Duration::from_secs(3), churner.connect(at, "localhost")?).await
        {
            opened += 1;
            conn.close(0u32.into(), b"again");
        }
    }
    // Another address still gets in...
    let player = stingy_peer(server.cert_hash)?;
    let joined = tokio::time::timeout(Duration::from_secs(3), player.connect(at, "localhost")?).await?;
    let net = server.status()["net"].clone();
    assert!(joined.is_ok(), "shut out after one address came and went ({opened} times): {net}");
    assert_eq!(net["refused_open"], 0, "{net}");
    // ...because the one that came and went was held to its share.
    assert!(opened <= 3, "it opened {opened} in a moment");
    assert!(net["refused_address"].as_u64() >= Some(16), "{net}");
    server.shutdown();
    Ok(())
}
