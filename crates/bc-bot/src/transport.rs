//! Native WebTransport client (wtransport) used by bots and integration tests.

use std::time::Duration;

use wtransport::config::IpBindConfig;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint};

/// Installs the rustls ring provider (shared with reqwest). Safe to call repeatedly.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Where to connect, as advertised by the server's `/cert-hash` endpoint.
#[derive(Clone, Debug)]
pub struct EndpointInfo {
    pub url: String,
    pub cert_hash: Option<[u8; 32]>,
}

/// Asks a dev server (`http://host:port`) for its WebTransport port and certificate hash.
pub async fn discover(http_base: &str) -> anyhow::Result<EndpointInfo> {
    install_crypto_provider();
    let base = http_base.trim_end_matches('/');
    let doc: serde_json::Value =
        reqwest::get(format!("{base}/cert-hash")).await?.error_for_status()?.json().await?;
    let port = doc["port"].as_u64().ok_or_else(|| anyhow::anyhow!("/cert-hash: missing port"))?;
    let path = doc["path"].as_str().unwrap_or("/bc");
    let hex_hash = doc["hash"].as_str().ok_or_else(|| anyhow::anyhow!("/cert-hash: missing hash"))?;
    let mut hash = [0u8; 32];
    hex::decode_to_slice(hex_hash, &mut hash)?;
    let host = base
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .rsplit_once(':')
        .map_or("127.0.0.1", |(h, _)| h)
        .to_string();
    Ok(EndpointInfo { url: format!("https://{host}:{port}{path}"), cert_hash: Some(hash) })
}

/// Opens a WebTransport session. With `cert_hash` the server's self-signed certificate is pinned by
/// hash (exactly what browsers do); without it the system roots are used.
///
/// Binds dual-stack when possible and falls back to IPv4 on hosts without IPv6.
pub async fn connect(url: &str, cert_hash: Option<[u8; 32]>) -> anyhow::Result<Connection> {
    install_crypto_provider();
    let build = |bind: IpBindConfig| -> anyhow::Result<ClientConfig> {
        let builder = ClientConfig::builder().with_bind_config(bind);
        Ok(match cert_hash {
            Some(hash) => builder.with_server_certificate_hashes([Sha256Digest::new(hash)]),
            None => builder.with_native_certs(),
        }
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .max_idle_timeout(Some(Duration::from_secs(10)))?
        .build())
    };
    let endpoint = match Endpoint::client(build(IpBindConfig::InAddrAnyDual)?) {
        Ok(endpoint) => endpoint,
        Err(_) => Endpoint::client(build(IpBindConfig::InAddrAnyV4)?)?,
    };
    Ok(endpoint.connect(url).await?)
}
