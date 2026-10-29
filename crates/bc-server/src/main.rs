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
    /// Player/agent slots.
    #[arg(long, default_value_t = 64)]
    max_clients: usize,
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
    /// Where the store lists the traders on its trade page, who stand on the Exchange floor
    /// (`none`: nobody). Default: the store's list, when wallets sign in to elopros.com
    /// (--siwe-domain); else nobody.
    #[arg(long)]
    floor_visitors: Option<String>,
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
    /// Open the colony's inside: the bays' airlocks lead to the cap lifts, down into the
    /// cylinder's city, and suits fly inside it. That inside is retired (`docs/RING.md`): it's
    /// closed by default and kept behind this flag for the browser suites until it's cleared.
    /// (Arcade rules have no colony to open.)
    #[arg(long)]
    colony: bool,
    /// The colony was open by default once, and this closed it; it's closed by default now. Kept
    /// so old command lines still run.
    #[arg(long, hide = true)]
    no_colony: bool,
    /// Connections taken in all, handshaking or open.
    #[arg(long, default_value_t = bc_server::net::admit::Limits::default().connections)]
    max_connections: u32,
    /// Connections taken from one address (loopback excepted).
    #[arg(long, default_value_t = bc_server::net::admit::Limits::default().per_address)]
    per_address: u32,
}

/// The store's list of traders to stand on the Exchange floor: `--floor-visitors` as given (`none`:
/// nobody), else the store's own when wallets sign in to it (a deployment serving the store's page).
fn floor_visitors(flag: Option<&str>, siwe_domains: &[String]) -> Option<String> {
    match flag {
        Some("none" | "") => None,
        Some(url) => Some(url.to_string()),
        None => siwe_domains
            .iter()
            .any(|d| d == "elopros.com")
            .then(|| bc_server::config::STORE_VISITORS.to_string()),
    }
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
        colony: args.rules == Ruleset::Survival && args.colony,
        floor_visitors: floor_visitors(
            args.floor_visitors.as_deref(),
            &bc_server::config::split_list(&args.siwe_domain),
        ),
        limits: bc_server::net::admit::Limits {
            connections: args.max_connections.max(1),
            per_address: args.per_address.max(1),
            ..bc_server::net::admit::Limits::default()
        },
        ..Config::default()
    };
    match (args.rules, args.colony) {
        (Ruleset::Survival, true) => {
            tracing::info!("the colony's inside is open (--colony): its cap lifts run")
        }
        (Ruleset::Survival, false) => {
            tracing::info!(
                "the colony's inside is closed while the ring is built (--colony opens the old one)"
            )
        }
        (Ruleset::Arcade, true) => tracing::warn!("--colony: arcade rules have no colony to open"),
        (Ruleset::Arcade, false) => tracing::info!("the colony is closed: arcade rules have no colony"),
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
    fn the_stores_traders_come_with_the_stores_sign_in() {
        let store = ["game.elopros.com".to_string(), "elopros.com".to_string()];
        assert_eq!(floor_visitors(None, &store).as_deref(), Some(bc_server::config::STORE_VISITORS));
        assert_eq!(floor_visitors(None, &["127.0.0.1:8080".to_string()]), None);
        assert_eq!(floor_visitors(Some("none"), &store), None);
        assert_eq!(
            floor_visitors(Some("http://127.0.0.1:3600/floor/x/visitors"), &[]).as_deref(),
            Some("http://127.0.0.1:3600/floor/x/visitors")
        );
    }

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
