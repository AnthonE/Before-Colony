//! Load test: many agents at once.
//!
//!   cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bc_bot::{BotClient, BotConfig, DollBrain};
use bc_proto::Faction;
use bc_sim::content::PLAYABLE_ORDER;
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value_t = 16)]
    bots: usize,
    #[arg(long, default_value_t = 30)]
    secs: u64,
}

#[derive(Default)]
struct Totals {
    connected: AtomicU64,
    snapshots: AtomicU64,
    bytes: AtomicU64,
    max_snapshot: AtomicU64,
    hits: AtomicU64,
    kills: AtomicU64,
    errors: AtomicU64,
    pred_err_mm_max: AtomicU64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let totals = Arc::new(Totals::default());
    let mut tasks = Vec::new();
    for k in 0..a.bots {
        let totals = totals.clone();
        let server = a.server.clone();
        let secs = a.secs;
        tasks.push(tokio::spawn(async move {
            let cfg = BotConfig {
                server,
                name: format!("Swarm-{k:03}"),
                // Every frame pilots can fly, in turn.
                frame: PLAYABLE_ORDER[k % PLAYABLE_ORDER.len()],
                faction: if k % 2 == 0 { Faction::Colonies } else { Faction::Alliance },
            };
            let mut bot = match BotClient::connect(&cfg).await {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("bot {k}: {e:#}");
                    totals.errors.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };
            totals.connected.fetch_add(1, Ordering::Relaxed);
            let mut brain = DollBrain::new(k as u32 * 7919 + 1);
            let mut worst = 0.0f32;
            let end = std::time::Instant::now() + Duration::from_secs(secs);
            while std::time::Instant::now() < end {
                if bot.step(&mut |ctx| brain.decide(ctx)).await.is_err() {
                    totals.errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                if bot.core.stats.snapshots > 60 {
                    worst = worst.max(bot.core.stats.prediction_error);
                }
            }
            let s = bot.core.stats;
            totals.snapshots.fetch_add(s.snapshots, Ordering::Relaxed);
            totals.bytes.fetch_add(s.bytes, Ordering::Relaxed);
            totals.max_snapshot.fetch_max(s.max_snapshot as u64, Ordering::Relaxed);
            totals.hits.fetch_add(u64::from(bot.world().my_hits), Ordering::Relaxed);
            totals.kills.fetch_add(u64::from(bot.world().my_kills), Ordering::Relaxed);
            totals.pred_err_mm_max.fetch_max((worst * 1000.0) as u64, Ordering::Relaxed);
            bot.close();
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    let t = &totals;
    let secs = a.secs.max(1) as f64;
    let connected = t.connected.load(Ordering::Relaxed).max(1) as f64;
    println!("bots connected     {}/{}", t.connected.load(Ordering::Relaxed), a.bots);
    println!("snapshots/bot/s    {:.1}", t.snapshots.load(Ordering::Relaxed) as f64 / connected / secs);
    println!(
        "downlink/bot       {:.1} KB/s",
        t.bytes.load(Ordering::Relaxed) as f64 / connected / secs / 1024.0
    );
    println!("max snapshot       {} B", t.max_snapshot.load(Ordering::Relaxed));
    println!("hits / kills       {} / {}", t.hits.load(Ordering::Relaxed), t.kills.load(Ordering::Relaxed));
    println!("worst pred. error  {} mm", t.pred_err_mm_max.load(Ordering::Relaxed));
    println!("errors             {}", t.errors.load(Ordering::Relaxed));
    if let Ok(status) = reqwest::get(format!("{}/status", a.server)).await
        && let Ok(json) = status.json::<serde_json::Value>().await
    {
        let g = &json["game"];
        println!(
            "server tick        p50 ≤{} µs  p99 ≤{} µs  max {} µs  overruns {}  out drops {}  hot-path allocations {}",
            g["tick_us"]["p50"],
            g["tick_us"]["p99"],
            g["tick_us"]["max"],
            g["overruns"],
            g["out_drops"],
            g["hot_path_allocations"]
        );
    }
    Ok(())
}
