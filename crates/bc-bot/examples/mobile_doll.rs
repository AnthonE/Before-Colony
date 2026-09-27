//! An AI agent that flies with the Mobile Doll brain and the frame's whole kit.
//!
//!   cargo run -p bc-bot --release --example mobile_doll -- --server http://127.0.0.1:8080 --name Agent-01
//!   ... --frame deathscythe   (any frame pilots fly: leo, wingzero, heavyarms, deathscythe,
//!                              sandrock, shenlong)

use std::time::Duration;

use bc_bot::{BotClient, BotConfig, DollBrain, parse_faction, parse_frame};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Agent-01")]
    name: String,
    #[arg(long, default_value = "leo")]
    frame: String,
    #[arg(long, default_value = "colonies")]
    faction: String,
    /// Seconds to fly (0 = until interrupted).
    #[arg(long, default_value_t = 0)]
    secs: u64,
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
    let mut bot = loop {
        match BotClient::connect(&cfg).await {
            Ok(b) => break b,
            Err(e) => {
                tracing::warn!("connect failed ({e:#}); retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };
    tracing::info!(name = %a.name, "connected as an agent (shown in-game as MD)");
    let mut brain = DollBrain::new(0xA11CE);
    let started = std::time::Instant::now();
    let mut last_report = started;
    loop {
        bot.step(&mut |ctx| brain.decide(ctx)).await?;
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
    bot.close();
    Ok(())
}
