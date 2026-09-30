//! An AI agent that mines: it cuts rocks apart with its saber, stows the ore and takes it to the
//! dock. Under arcade rules the dock buys it; under survival rules the agent docks, sells the ore
//! on the Colony Exchange, tops its tank up and goes back out.
//!
//!   cargo run -p bc-bot --release --example miner -- --server http://127.0.0.1:8080 --name Miner-01

use std::time::Duration;

use bc_bot::{BotClient, BotConfig, MinerBrain, parse_faction, parse_frame};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Miner-01")]
    name: String,
    /// A Leo's hold takes 3 t, a Wing Zero's 1.5 t.
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
    let mut bot = connect(&cfg).await;
    tracing::info!(name = %a.name, "connected as a mining agent");
    let mut brain = MinerBrain::new();
    let started = std::time::Instant::now();
    let mut last_report = started;
    loop {
        bot.step(&mut |ctx| brain.decide(ctx)).await?;
        if bot.survival() && brain.ready_to_dock(bot.world()) {
            match bring_it_home(&mut bot).await {
                Ok(()) => brain.unloaded(),
                Err(e) => tracing::warn!("docking: {e:#}"),
            }
        }
        // Survival: back in the hangar without a suit (it was lost). A guest's bay is stocked
        // afresh each visit, so the agent comes back for another.
        if bot.survival() && bot.place() == Some(bc_econ::wire::Place::Hangar) && !bot.sortie().await? {
            bot.close().await;
            bot = connect(&cfg).await;
        }
        if last_report.elapsed() > Duration::from_secs(10) {
            last_report = std::time::Instant::now();
            if let Some(v) = bot.world().salvage_view() {
                // Survival: the hangar's credits (the suit's own are this sortie's bounties).
                let credits = if bot.survival() { bot.core.hangar.credits() } else { u64::from(v.credits) };
                tracing::info!(
                    hold_kg = v.cargo_total_kg(),
                    of_kg = v.capacity_kg,
                    in_hand = v.held.is_some(),
                    credits,
                    hauling = brain.hauling(),
                    "status"
                );
            }
        }
        if a.secs > 0 && started.elapsed() > Duration::from_secs(a.secs) {
            break;
        }
    }
    bot.close().await;
    Ok(())
}

/// Connects (retrying until the server's there) and gets flying.
async fn connect(cfg: &BotConfig) -> BotClient {
    loop {
        match BotClient::connect(cfg).await {
            Ok(mut b) => match b.sortie().await {
                Ok(_) => return b,
                Err(e) => tracing::warn!("launch failed ({e:#}); retrying"),
            },
            Err(e) => tracing::warn!("connect failed ({e:#}); retrying"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Docks, sells the haul, buys propellant for the next trip, and launches again.
async fn bring_it_home(bot: &mut BotClient) -> anyhow::Result<()> {
    use bc_econ::item::{Item, Material, Ore};
    bot.dock().await?;
    if let Some((_, text)) = bot.core.hangar.sorties.last() {
        tracing::info!("home: {text}");
    }
    for ore in Ore::ALL {
        if let Some(text) = bot.sell_all(Item::Ore(ore)).await? {
            tracing::info!("{text}");
        }
    }
    let propellant = Item::Material(Material::Propellant);
    let have = bot
        .core
        .hangar
        .view
        .as_ref()
        .map_or(0, |v| v.stock.iter().find(|(i, _)| *i == propellant).map_or(0, |(_, q)| *q));
    if have < 2_000
        && let Err(e) = bot.buy(propellant, 2_000 - have, 2_000).await
    {
        tracing::warn!("no propellant: {e:#}");
    }
    tracing::info!(credits = bot.core.hangar.credits(), "back out");
    bot.sortie().await?;
    Ok(())
}
