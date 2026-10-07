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
    /// Dev HTTP address (static client, /cert-hash, /status).
    #[arg(long, default_value = "127.0.0.1:8080")]
    http: SocketAddr,
    /// Built web client directory to serve at `/`.
    #[arg(long, default_value = "web/dist")]
    web_dir: PathBuf,
    /// Mobile Doll NPCs to keep in the sector.
    #[arg(long, default_value_t = 24)]
    mobile_dolls: u32,
    /// Seconds between Zodiac's aces among the Dolls, one out at a time (0: none).
    #[arg(long, default_value_t = 300)]
    ace_every: u64,
    /// Tactical oracle behind the ZERO System (`jev` needs TYPESAFE_API_KEY).
    #[arg(long, value_enum, default_value_t = OracleKind::Local)]
    oracle: OracleKind,
    /// Player/agent slots.
    #[arg(long, default_value_t = 64)]
    max_clients: usize,
    /// Simulation seed.
    #[arg(long, default_value_t = 0xBC_0195)]
    seed: u64,
    /// The host (and port) wallets sign in to: where pages are served from. Default: --http.
    #[arg(long)]
    siwe_domain: Option<String>,
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
    /// Open the colony (survival): the bays' airlocks lead to the cap lifts, down into its city.
    #[arg(long)]
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
        mobile_dolls: args.mobile_dolls,
        ace_every: std::time::Duration::from_secs(args.ace_every),
        oracle: args.oracle,
        max_clients: args.max_clients,
        seed: args.seed,
        jev_key: std::env::var("TYPESAFE_API_KEY").ok().filter(|k| !k.is_empty()),
        jev_url: std::env::var("TYPESAFE_BASE_URL").ok().filter(|u| !u.is_empty()),
        siwe_domain: args.siwe_domain,
        require_auth: args.require_auth,
        rules: args.rules,
        flight: args.flight,
        craft_speed: args.craft_speed.max(0.01),
        data_dir: args.data_dir,
        colony: args.colony,
        limits: bc_server::net::admit::Limits {
            connections: args.max_connections.max(1),
            per_address: args.per_address.max(1),
            ..bc_server::net::admit::Limits::default()
        },
        ..Config::default()
    };
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
