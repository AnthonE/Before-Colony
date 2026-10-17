//! An AI agent that flies with the drone brain and the frame's whole kit.
//!
//!   cargo run -p bc-bot --release --example drone -- --server http://127.0.0.1:8080 --name Agent-01
//!   ... --frame algol   (any frame pilots fly: argo, altair, antares, algol,
//!                              canopus, thuban)

use std::time::Duration;

use bc_bot::{BotClient, BotConfig, DroneBrain, parse_faction, parse_frame};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Agent-01")]
    name: String,
    #[arg(long, default_value = "argo")]
    frame: String,
    #[arg(long, default_value = "colonies")]
    faction: String,
    /// Seconds to fly (0 = until interrupted).
    #[arg(long, default_value_t = 0)]
    secs: u64,
    /// The agent's wallet: a key file, made on first use and readable by its owner only
    /// (`bc_bot::key`). Without one the agent flies as a guest.
    #[arg(long)]
    key: Option<std::path::PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();
    let a = Args::parse();
    let cfg = BotConfig {
        server: a.server,
        name: a.name.clone(),
        frame: parse_frame(&a.frame).ok_or_else(|| anyhow::anyhow!("unknown frame {}", a.frame))?,
        faction: parse_faction(&a.faction).ok_or_else(|| anyhow::anyhow!("unknown faction {}", a.faction))?,
    };
    let wallet = a.key.as_deref().map(bc_bot::key::load_or_create).transpose()?;
    let mut bot = connect(&cfg, wallet.as_ref()).await;
    tracing::info!(name = %a.name, "connected as an agent (shown in-game as DR)");
    let mut brain = DroneBrain::new(0xA11CE);
    let started = std::time::Instant::now();
    let mut last_report = started;
    loop {
        bot.step(&mut |ctx| brain.decide(ctx)).await?;
        // Doomed: get out, so the tugs bring the wreck home.
        if bot.doomed()
            && let Err(e) = bot.eject().await
        {
            tracing::warn!("eject: {e:#}");
        }
        // Survival: back in the hangar without a suit (it was lost). A guest's bay is stocked
        // afresh each visit, so the agent comes back for another; a wallet's is kept, and the
        // Charter Board advances it a worn suit at most every half hour, so it waits a while first.
        if bot.survival() && bot.place() == Some(bc_econ::wire::Place::Hangar) && !bot.sortie().await? {
            bot.close().await;
            if wallet.is_some() {
                tracing::info!("no suit in the bay: waiting for the Charter Board to advance one");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            bot = connect(&cfg, wallet.as_ref()).await;
        }
        if last_report.elapsed() > Duration::from_secs(10) {
            last_report = std::time::Instant::now();
            let w = bot.world();
            tracing::info!(
                hits = w.my_hits,
                kills = w.my_kills,
                deaths = w.my_deaths,
                snapshots = bot.core.stats.snapshots,
                "status"
            );
        }
        if a.secs > 0 && started.elapsed() > Duration::from_secs(a.secs) {
            break;
        }
    }
    bot.close().await;
    Ok(())
}

/// Connects (retrying until the server's there) and gets flying.
async fn connect(cfg: &BotConfig, wallet: Option<&bc_auth::LocalWallet>) -> BotClient {
    loop {
        match BotClient::connect_as(cfg, wallet).await {
            Ok(mut b) => match b.sortie().await {
                Ok(_) => return b,
                Err(e) => tracing::warn!("launch failed ({e:#}); retrying"),
            },
            Err(e) => tracing::warn!("connect failed ({e:#}); retrying"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
