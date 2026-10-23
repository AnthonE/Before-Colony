use std::net::SocketAddr;
use std::path::PathBuf;

use bc_server::{Config, Flight, Mode, OracleKind, Ruleset};
use clap::Parser;

/// Counts heap operations; the sector thread marks its ticks as hot regions, so any allocation
/// inside a tick shows up as `hot_path_allocations` in /status.
#[global_allocator]
static ALLOC: bc_alloc::CountingAlloc = bc_alloc::CountingAlloc;

/// Before Colony game server.
#[derive(Parser, Debug)]
#[command(name = "bc-server", version, about)]
struct Args {
    /// `game` runs the sector; `echo` bounces datagrams back (transport smoke test).
    #[arg(long, value_enum, default_value_t = Mode::Game)]
    mode: Mode,
    /// UDP port for WebTransport.
    #[arg(long, default_value_t = 4433)]
    wt_port: u16,
    /// HTTP address (static client, /cert-hash, /status, /status.json).
    #[arg(long, default_value = "127.0.0.1:8080")]
    http: SocketAddr,
    /// Built web client directory to serve at `/`.
    #[arg(long, default_value = "web/dist")]
    web_dir: PathBuf,
    /// PEM certificate chain for WebTransport (e.g. Let's Encrypt's fullchain.pem). Without it the
    /// server self-signs and browsers pin the certificate's hash.
    #[arg(long, requires = "key")]
    cert: Option<PathBuf>,
    /// PEM private key for `--cert`.
    #[arg(long, requires = "cert")]
    key: Option<PathBuf>,
    /// Drone NPCs to keep in the sector. The alias is the flag's name before 2026-10-14, kept so
    /// pre-rename launch scripts still start; nothing shows it.
    #[arg(long, alias = "mobile-dolls", default_value_t = 24)]
    drones: u32,
    /// Seconds between the Almagest's aces among the drones, one out at a time (0: none).
    #[arg(long, default_value_t = 300)]
    ace_every: u64,
    /// Tactical oracle behind the Orrery (`jev` needs TYPESAFE_API_KEY).
    #[arg(long, value_enum, default_value_t = OracleKind::Local)]
    oracle: OracleKind,
    /// Suits flying in space at once (pilots and agents): the sector's slots.
    #[arg(long, default_value_t = 64)]
    max_clients: usize,
    /// Sessions at once, in the bay, on foot or flying.
    #[arg(long, default_value_t = 512)]
    max_sessions: usize,
    /// Simulation seed.
    #[arg(long, default_value_t = 0xBC_0195)]
    seed: u64,
    /// The hosts (and ports) wallets sign in to: where pages are served from. Repeat it or
    /// separate with commas (`game.elopros.com,elopros.com`); each session is asked to sign for the
    /// one its page came from. Default: --http.
    #[arg(long)]
    siwe_domain: Vec<String>,
    /// Origins whose pages may read /status, /status.json and /cert-hash from another site (CORS).
    /// Repeat it or separate with commas.
    #[arg(long, default_value = bc_server::config::STORE_ORIGIN)]
    cors_origin: Vec<String>,
    /// Admit signed-in pilots only (agents excepted).
    #[arg(long)]
    require_auth: bool,
    /// `survival`: pilots build and keep their suits, and trade on the exchange; `arcade`: any
    /// frame, free respawns.
    #[arg(long, value_enum, default_value_t = Ruleset::Survival)]
    rules: Ruleset,
    /// `anime`: the tank is a boost gauge that fills back up, and pilots bear more G; `real`:
    /// every newton burns propellant (the simulator).
    #[arg(long, value_enum, default_value_t = Flight::Anime)]
    flight: Flight,
    /// The fabricator and foundry work this many times faster than their recipes say.
    #[arg(long, default_value_t = 1.0)]
    craft_speed: f64,
    /// Keep pilot records and the exchange in this directory (otherwise they last one run).
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Close the colony: the bays' airlocks no longer lead to the cap lifts, down into its city,
    /// and no suit flies inside it. (It is open by default under survival rules; arcade rules
    /// have no colony to open.)
    #[arg(long)]
    no_colony: bool,
    /// The colony used to be opened by hand; it's open by default now. Kept so old command lines
    /// still run.
    #[arg(long, hide = true)]
    colony: bool,
    /// Connections taken in all, handshaking or open.
    #[arg(long, default_value_t = bc_server::net::admit::Limits::default().connections)]
    max_connections: u32,
    /// Connections taken from one address (loopback excepted).
    #[arg(long, default_value_t = bc_server::net::admit::Limits::default().per_address)]
    per_address: u32,
}

fn main() -> anyhow::Result<()> {
    bc_server::telemetry::init();
    let args = Args::parse();
    let web_dir = args.web_dir.is_dir().then_some(args.web_dir.clone());
    if web_dir.is_none() {
        tracing::warn!(
            "{} not found: serving no web client (run scripts/build-web.sh)",
            args.web_dir.display()
        );
    }
    let cfg = Config {
        mode: args.mode,
        wt_port: args.wt_port,
        http_addr: args.http,
        web_dir,
        drones: args.drones,
        ace_every: std::time::Duration::from_secs(args.ace_every),
        oracle: args.oracle,
        max_clients: args.max_clients,
        max_sessions: args.max_sessions.clamp(1, bc_server::net::sessions::MAX_SESSIONS),
        seed: args.seed,
        jev_key: std::env::var("TYPESAFE_API_KEY").ok().filter(|k| !k.is_empty()),
        jev_url: std::env::var("TYPESAFE_BASE_URL").ok().filter(|u| !u.is_empty()),
        siwe_domains: bc_server::config::split_list(&args.siwe_domain),
        tls: args.cert.zip(args.key),
        cors_origins: bc_server::config::split_list(&args.cors_origin),
        require_auth: args.require_auth,
        rules: args.rules,
        flight: args.flight,
        craft_speed: args.craft_speed.max(0.01),
        data_dir: args.data_dir,
        colony: args.rules == Ruleset::Survival && !args.no_colony,
        limits: bc_server::net::admit::Limits {
            connections: args.max_connections.max(1),
            per_address: args.per_address.max(1),
            ..bc_server::net::admit::Limits::default()
        },
        ..Config::default()
    };
    match (args.rules, args.no_colony) {
        (Ruleset::Survival, false) => {
            tracing::info!("the colony is open: its cap lifts run (--no-colony closes it)")
        }
        (Ruleset::Survival, true) => tracing::info!("the colony is closed (--no-colony)"),
        (Ruleset::Arcade, _) if args.colony => {
            tracing::warn!("--colony: arcade rules have no colony to open")
        }
        (Ruleset::Arcade, _) => tracing::info!("the colony is closed: arcade rules have no colony"),
    }
    // Two workers are plenty: all game work happens on the dedicated sector thread.
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(async move {
        let handle = bc_server::start(cfg).await?;
        tracing::info!("open http://{} (WebTransport on UDP {})", handle.http_addr, handle.wt_port);
        tokio::signal::ctrl_c().await?;
        tracing::info!("shutting down");
        handle.shutdown();
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siwe_domains_take_commas_or_repeats() {
        let a = Args::parse_from(["bc-server", "--siwe-domain", "game.elopros.com,elopros.com"]);
        assert_eq!(bc_server::config::split_list(&a.siwe_domain), ["game.elopros.com", "elopros.com"]);
        let a = Args::parse_from([
            "bc-server",
            "--siwe-domain",
            "game.elopros.com",
            "--siwe-domain",
            "elopros.com",
        ]);
        assert_eq!(bc_server::config::split_list(&a.siwe_domain), ["game.elopros.com", "elopros.com"]);
        let a = Args::parse_from(["bc-server"]);
        assert!(a.siwe_domain.is_empty());
        assert_eq!(a.cors_origin, [bc_server::config::STORE_ORIGIN]);
    }

    #[test]
    fn a_certificate_comes_with_its_key() {
        assert!(Args::try_parse_from(["bc-server", "--cert", "c.pem"]).is_err());
        assert!(Args::try_parse_from(["bc-server", "--key", "k.pem"]).is_err());
        let a = Args::parse_from(["bc-server", "--cert", "c.pem", "--key", "k.pem"]);
        assert_eq!(a.cert.zip(a.key), Some((PathBuf::from("c.pem"), PathBuf::from("k.pem"))));
    }
}
