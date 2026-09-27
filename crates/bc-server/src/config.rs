use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// What the server runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// The game: one sector simulation plus Mobile Doll NPCs.
    Game,
    /// Transport smoke test: datagrams and stream bytes are echoed back.
    Echo,
}

/// Which tactical oracle feeds the ZERO System. The in-sim local oracle always runs; `jev` adds
/// TypeSafe Jev as a blended prior when `TYPESAFE_API_KEY` is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OracleKind {
    Local,
    Jev,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: Mode,
    /// UDP port for WebTransport (0 = pick a free port).
    pub wt_port: u16,
    /// Dev HTTP address (port 0 = pick a free port).
    pub http_addr: SocketAddr,
    /// Built web client to serve at `/` (optional).
    pub web_dir: Option<PathBuf>,
    /// Mobile Doll NPCs to keep in the sector.
    pub mobile_dolls: u32,
    pub oracle: OracleKind,
    /// Player/agent slots in the sector.
    pub max_clients: usize,
    /// Seed for the deterministic simulation.
    pub seed: u64,
    /// TypeSafe API key (from `TYPESAFE_API_KEY`); never sent to clients.
    pub jev_key: Option<String>,
    /// Override for the Jev endpoint (`TYPESAFE_BASE_URL`), e.g. a proxy or a test mock.
    pub jev_url: Option<String>,
    /// The name wallets sign in to: the host (and port) pages are served from. Defaults to the
    /// HTTP address; a deployment behind a domain must set it, or wallets warn about a mismatch.
    pub siwe_domain: Option<String>,
    /// Admit signed-in pilots only (agents excepted).
    pub require_auth: bool,
    /// How long a wallet may take to sign (people read the message first).
    pub sign_wait: Duration,
    /// How long a resume token lasts after its session ends.
    pub resume_ttl: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Game,
            wt_port: 4433,
            http_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            web_dir: None,
            mobile_dolls: 24,
            oracle: OracleKind::Local,
            max_clients: 64,
            seed: 0xBC_0195,
            jev_key: None,
            jev_url: None,
            siwe_domain: None,
            require_auth: false,
            sign_wait: Duration::from_secs(60),
            resume_ttl: Duration::from_secs(15 * 60),
        }
    }
}
