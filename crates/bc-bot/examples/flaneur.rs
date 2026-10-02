//! An agent on foot in the colony (survival rules, the colony open): it rides the cap lift down to
//! a strip's Hub Gate and strolls up and down the avenue in front of it, so a pilot coming down
//! finds someone there. It walks with the browser's own legs (`bc_client_core::walker`) among the
//! city's walls (`bc_client_core::city`), so the server takes every step.
//!
//!   cargo run -p bc-bot --release --example flaneur -- --server http://127.0.0.1:8080 --name Flaneur-01

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig, Stroll};
use bc_proto::{Faction, FrameId};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Flaneur-01")]
    name: String,
    /// The land strip to go down to (0 Charter, 1 Canal, 2 Gardens).
    #[arg(long, default_value_t = 0)]
    strip: u8,
    /// How far up the avenue from Hub Gate's door it strolls, m.
    #[arg(long, default_value_t = 60.0)]
    reach: f32,
    /// Seconds to stay (0 = until interrupted).
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
    let cfg =
        BotConfig { server: a.server, name: a.name.clone(), frame: FrameId::Leo, faction: Faction::Colonies };
    let mut bot = loop {
        match BotClient::connect(&cfg).await {
            Ok(b) => break b,
            Err(e) => {
                tracing::warn!("connect failed: {e:#}; retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };
    anyhow::ensure!(bot.survival(), "the colony's inside is survival's");
    bot.wait_until(10.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    anyhow::ensure!(bot.core.welcome.as_ref().is_some_and(|w| w.colony), "the colony isn't open (--colony)");
    let strip = a.strip % 3;
    bot.enter_city(strip).await?;
    let mut stroll = Stroll::new(strip, a.reach, 4.0);
    tracing::info!(name = %a.name, strip, "down in the colony, strolling");
    let started = Instant::now();
    let mut last = Instant::now();
    let mut last_report = started;
    loop {
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        let pose = stroll.step(dt);
        bot.set_pose(pose);
        bot.step(&mut |_| bc_proto::InputCmd::default()).await?;
        if last_report.elapsed() > Duration::from_secs(10) {
            last_report = Instant::now();
            let people: Vec<String> = bot.people().into_iter().map(|(_, n, _)| n).collect();
            tracing::info!(x = stroll.along() as i32, ?people, "strolling");
        }
        if a.secs > 0 && started.elapsed() > Duration::from_secs(a.secs) {
            break;
        }
    }
    bot.leave_city().await?;
    bot.close().await;
    Ok(())
}
