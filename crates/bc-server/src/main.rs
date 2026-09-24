use std::net::SocketAddr;
use std::path::PathBuf;

use bc_server::{Config, Mode, OracleKind};
use clap::Parser;

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
    /// Tactical oracle behind the ZERO System (`jev` needs TYPESAFE_API_KEY).
    #[arg(long, value_enum, default_value_t = OracleKind::Local)]
    oracle: OracleKind,
    /// Player/agent slots.
    #[arg(long, default_value_t = 64)]
    max_clients: usize,
    /// Simulation seed.
    #[arg(long, default_value_t = 0xBC_0195)]
    seed: u64,
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
        oracle: args.oracle,
        max_clients: args.max_clients,
        seed: args.seed,
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
